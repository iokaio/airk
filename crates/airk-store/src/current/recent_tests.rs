// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{changes::CoverageReason, identity::RevisionToken};

fn revision(token: CurrentToken) -> RevisionToken {
    RevisionToken {
        incarnation: token.incarnation,
        generation: token.sequence,
        revision: token.digest,
    }
}

fn append(fixture: &Fixture, observation: recent::Observation, now: u64) -> Result<CurrentToken> {
    let read = fixture.store.current_read(&fixture.scope)?;
    let base = read.head().clone();
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 4 << 20)?;
    let graph = graph::GraphOverlay::new(
        &reservation,
        read,
        graph::GraphOverlayLimits {
            maximum_bytes: 1 << 20,
            ..Default::default()
        },
    )?
    .seal()?;
    let connection = database::open_reader(&fixture.store.directory().join("current.sqlite3"))?;
    let mut plan = map::MapPlan::new(&connection, &fixture.scope, base.artifact_root)?;
    let mut changes = recent::prepare(&fixture.scope, &mut plan, &base, observation, now)?;
    let mut target = base.clone();
    target.sequence += 1;
    target.artifact_root = plan.root()?;
    changes.sort_by_key(map::MapChange::key);
    drop(plan);
    drop(connection);
    let writer = fixture.store.lock()?;
    let capacity = fixture.store.current_database_capacity(&fixture.scope)?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, capacity)?;
    if capacity < 64 << 20 {
        current.grow_capacity(64 << 20)?;
    }
    let pending = current.prepare_mapped(
        &base.token()?,
        target,
        &graph,
        &Vec::<CapturedInput>::new(),
        None,
        &changes,
    )?;
    current.mark_applying(&pending)?;
    Ok(current
        .commit(&pending, pending.target.lexical_commit)?
        .token)
}

fn event(generation: u64) -> airk_core::changes::ChangeEvent {
    use airk_core::changes::*;
    ChangeEvent {
        generation,
        event_id: 0,
        subject: ChangeSubject::Global {
            domain: "original-fixture".into(),
        },
        before: None,
        after: None,
        before_detail: None,
        after_detail: None,
        dimensions: vec![],
        cause: ChangeCause::MetadataMaintenance,
        basis: ContinuityBasis::Unknown,
    }
}

#[test]
fn malformed_authenticated_parent_metadata_is_rejected_instead_of_an_empty_history() -> Result<()> {
    let fixture = Fixture::new()?;
    append(
        &fixture,
        recent::Observation {
            events: vec![event(1)],
            ..recent::Observation::empty()
        },
        1000,
    )?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let base = read.head().clone();
    let mut header: serde_json::Value = serde_json::from_slice(
        &read
            .map_get(map::MapDomain::RecentChanges, b"manifest")?
            .unwrap(),
    )?;
    header["groups"][0]["parent"] = serde_json::to_value(RevisionToken {
        generation: 99,
        ..revision(base.token()?)
    })?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 4 << 20)?;
    let graph = graph::GraphOverlay::new(
        &reservation,
        read,
        graph::GraphOverlayLimits {
            maximum_bytes: 1 << 20,
            ..Default::default()
        },
    )?
    .seal()?;
    let connection = database::open_reader(&fixture.store.directory().join("current.sqlite3"))?;
    let mut plan = map::MapPlan::new(&connection, &fixture.scope, base.artifact_root)?;
    let change = map::MapChange::new(
        &fixture.scope,
        map::MapDomain::RecentChanges,
        b"manifest",
        Some(serde_json::to_vec(&header)?),
    )?;
    plan.put(change.key(), change.value())?;
    let mut target = base.clone();
    target.sequence += 1;
    target.artifact_root = plan.root()?;
    drop(plan);
    drop(connection);
    let writer = fixture.store.lock()?;
    let capacity = fixture.store.current_database_capacity(&fixture.scope)?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, capacity)?;
    let pending = current.prepare_mapped(
        &base.token()?,
        target,
        &graph,
        &Vec::<CapturedInput>::new(),
        None,
        &[change],
    )?;
    current.mark_applying(&pending)?;
    current.commit(&pending, pending.target.lexical_commit)?;
    drop(current);
    drop(writer);
    let error = match fixture
        .store
        .current_read(&fixture.scope)?
        .recent_changes(1000)
    {
        Ok(_) => anyhow::bail!("malformed retained parent was accepted"),
        Err(error) => error,
    };
    assert!(
        error
            .downcast_ref::<recent::InvalidRecentChanges>()
            .is_some()
    );
    Ok(())
}

#[test]
fn whole_groups_trim_under_record_pressure_and_clock_expiry_is_cursor_visible() -> Result<()> {
    let fixture = Fixture::new()?;
    let zero = revision(fixture.head.token()?);
    let one = append(
        &fixture,
        recent::Observation {
            events: vec![event(1); 6000],
            ..recent::Observation::empty()
        },
        1000,
    )?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let before = read.recent_changes(1000)?;
    assert!(before.verifies(zero));
    assert_eq!(before.events.len(), 6000);
    drop(read);
    let two = append(
        &fixture,
        recent::Observation {
            events: vec![event(2); 6000],
            ..recent::Observation::empty()
        },
        900,
    )?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let view = read.recent_changes(1000)?;
    assert_eq!(view.events.len(), 6000);
    assert!(view.events.iter().all(|event| event.generation == 2));
    assert_eq!(view.coverage.files.complete_after, 1);
    assert_eq!(view.coverage.files.reason, Some(CoverageReason::Overflow));
    assert!(!view.verifies(zero));
    assert!(view.verifies(revision(one)));
    let expired = read.recent_changes(900 + recent::AGE_SECONDS)?;
    assert!(expired.events.is_empty());
    assert_ne!(expired.view_digest, view.view_digest);
    assert_eq!(read.token()?, two);
    Ok(())
}

#[test]
fn oversized_group_advances_coverage_instead_of_publishing_a_partial_change_set() -> Result<()> {
    use airk_core::changes::*;
    let fixture = Fixture::new()?;
    let mut large = event(1);
    let detail = ChangeDetail {
        digests: Default::default(),
        location: Some(airk_core::DocumentLocation::WorksheetRow {
            sheet: "X".repeat(4096),
            row: 1,
        }),
        region_location_digest: None,
        provenance_digest: Sha256Digest::from_bytes([5; 32]),
    };
    large.before_detail = Some(detail.clone());
    large.after_detail = Some(detail);
    append(
        &fixture,
        recent::Observation {
            events: vec![large; 2100],
            ..recent::Observation::empty()
        },
        1000,
    )?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let view = read.recent_changes(1000)?;
    assert!(view.events.is_empty());
    assert_eq!(view.coverage.files.complete_after, 1);
    assert_eq!(view.coverage.files.reason, Some(CoverageReason::Overflow));
    Ok(())
}

#[test]
fn recent_parent_tokens_are_exact_bounded_and_expire_without_source_history() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = revision(fixture.head.token()?);
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let mut token = fixture.head.token()?;
    let mut parents = vec![first];
    for n in 0..recent::GENERATIONS + 2 {
        let input = retained_input(&fixture, &format!("original history fixture {n}"))?;
        token = current
            .commit_artifact(&token, &input.artifact.path, Some(&input))?
            .token;
        parents.push(revision(token));
    }
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    let now = recent::unix_seconds();
    let view = read.recent_changes(now)?;
    assert_eq!(view.parents.len(), recent::GENERATIONS);
    assert!(!view.parents.contains(&first));
    assert_eq!(view.parents.first(), Some(&parents[2]));
    assert_eq!(view.parents.last(), Some(&parents[parents.len() - 2]));
    // The low-level catalog fixture lacks file metadata. Its complete parent
    // identity does not manufacture a complete file change observation.
    assert_eq!(view.coverage.files.reason, Some(CoverageReason::Omitted));
    assert!(!view.verifies(parents[2]));
    let mut invented = *view.parents.last().unwrap();
    invented.revision = Sha256Digest::from_bytes([81; 32]);
    assert!(!view.parents.contains(&invented));
    let expired = read.recent_changes(now + recent::AGE_SECONDS + 1)?;
    assert!(expired.parents.is_empty() && expired.events.is_empty());
    assert_eq!(expired.coverage.files.reason, Some(CoverageReason::Expired));
    assert_ne!(expired.view_digest, view.view_digest);
    assert_eq!(read.token()?, token);
    Ok(())
}
