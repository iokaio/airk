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

// Selected preparation is exercised through the actual retained-map reader.
// Public recent_changes still selects ALL unexpired groups; no query cutover.
fn selected_view(
    read: &CurrentRead,
    now: u64,
    since: Option<RevisionToken>,
) -> Result<(recent::RecentChanges, usize, usize)> {
    let current = revision(read.token()?);
    let mut chunks = 0usize;
    let mut bytes = 0usize;
    let view = recent::read_recent(
        &read.head().scope,
        read.token()?,
        now,
        |header| {
            since
                .filter(|baseline| *baseline != current && header.verifies(*baseline))
                .map(|baseline| baseline.generation)
        },
        &mut |key| {
            let value = read.map_get(map::MapDomain::RecentChanges, key)?;
            if key.starts_with(b"events/") {
                chunks += 1;
            }
            bytes += value.as_ref().map_or(0, Vec::len);
            Ok(value)
        },
    )?;
    Ok((view, chunks, bytes))
}
#[test]
fn recent_header_selection_reads_only_verified_tail_but_preserves_full_view_identity() -> Result<()>
{
    let fixture = Fixture::new()?;
    let mut tokens = vec![revision(fixture.head.token()?)];
    for generation in 1..=6 {
        tokens.push(revision(append(
            &fixture,
            recent::Observation {
                events: vec![event(generation); 2],
                ..recent::Observation::empty()
            },
            1000 + generation,
        )?));
    }
    let read = fixture.store.current_read(&fixture.scope)?;
    let full = read.recent_changes(1010)?;
    let (selected, chunks, bytes) = selected_view(&read, 1010, Some(tokens[5]))?;
    assert_eq!(
        chunks, 1,
        "one actual128KiB group chunk is selected from six"
    );
    assert_eq!(selected.parents, full.parents);
    assert_eq!(
        serde_json::to_value(&selected.coverage)?,
        serde_json::to_value(&full.coverage)?
    );
    assert_eq!(selected.view_digest, full.view_digest);
    let expected: Vec<_> = full
        .events
        .iter()
        .filter(|event| event.generation > tokens[5].generation)
        .collect();
    assert_eq!(
        serde_json::to_value(&selected.events)?,
        serde_json::to_value(expected)?
    );
    let (_, all_chunks, all_bytes) = selected_view(&read, 1010, Some(tokens[0]))?;
    assert_eq!(all_chunks, 6);
    assert!(bytes < all_bytes);
    // Exact existing preimage: ALL manifest bytes, first live parent and ALL
    // independent coverage floors; selecting a tail must not narrow the digest.
    let manifest = read
        .map_get(map::MapDomain::RecentChanges, b"manifest")?
        .context("manifest")?;
    let expected = airk_core::identity::framed_digest(
        b"recent-change-view/1",
        &[
            &manifest,
            &tokens[0].generation.to_be_bytes(),
            &serde_json::to_vec(&full.coverage)?,
        ],
    );
    assert_eq!(selected.view_digest, expected);
    Ok(())
}
#[test]
fn recent_no_baseline_same_head_and_foreign_baseline_need_headers_but_no_event_chunks() -> Result<()>
{
    let fixture = Fixture::new()?;
    let current = revision(append(
        &fixture,
        recent::Observation {
            events: vec![event(1)],
            ..recent::Observation::empty()
        },
        1000,
    )?);
    let read = fixture.store.current_read(&fixture.scope)?;
    let full = read.recent_changes(1000)?;
    let mut invented = full.parents[0];
    invented.revision = Sha256Digest::from_bytes([91; 32]);
    let foreign = RevisionToken {
        incarnation: airk_core::identity::Incarnation::from_bytes([92; 16]),
        ..current
    };
    for baseline in [None, Some(current), Some(invented), Some(foreign)] {
        let (selected, chunks, bytes) = selected_view(&read, 1000, baseline)?;
        assert_eq!(chunks, 0);
        assert!(bytes > 0, "authenticated bounded header is still read");
        assert!(selected.events.is_empty());
        assert_eq!(selected.parents, full.parents);
        assert_eq!(selected.view_digest, full.view_digest);
        assert_eq!(
            serde_json::to_value(&selected.coverage)?,
            serde_json::to_value(&full.coverage)?
        );
    }
    Ok(())
}
#[test]
fn recent_selection_recomputes_expiry_and_independent_floors_without_new_publication() -> Result<()>
{
    let fixture = Fixture::new()?;
    let zero = revision(fixture.head.token()?);
    let one = append(
        &fixture,
        recent::Observation {
            events: vec![event(1)],
            code_gap: Some(CoverageReason::Omitted),
            ..recent::Observation::empty()
        },
        1000,
    )?;
    let two = append(
        &fixture,
        recent::Observation {
            events: vec![event(2)],
            document_gap: Some(CoverageReason::Unsupported),
            ..recent::Observation::empty()
        },
        1001,
    )?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let (before, _, _) = selected_view(&read, 1000 + recent::AGE_SECONDS - 1, Some(zero))?;
    let (after, chunks, _) = selected_view(&read, 1000 + recent::AGE_SECONDS, Some(revision(one)))?;
    let full = read.recent_changes(1000 + recent::AGE_SECONDS)?;
    assert_eq!(chunks, 1);
    assert_ne!(before.view_digest, after.view_digest);
    assert_eq!(after.view_digest, full.view_digest);
    assert_eq!(
        serde_json::to_value(&after.coverage)?,
        serde_json::to_value(&full.coverage)?
    );
    assert_eq!(after.coverage.files.reason, Some(CoverageReason::Expired));
    assert_eq!(
        after.coverage.documents.reason,
        Some(CoverageReason::Unsupported)
    );
    assert_eq!(
        read.token()?,
        two,
        "age check did not publish or rewrite anything"
    );
    let (unavailable, chunks, _) = selected_view(&read, 1000 + recent::AGE_SECONDS, Some(zero))?;
    assert_eq!(chunks, 0);
    assert!(unavailable.events.is_empty());
    assert!(!unavailable.verifies(zero));
    Ok(())
}
#[test]
fn recent_selected_materialization_skips_unselected_read_fault_but_strict_engine_and_selected_fault_refuse()
-> Result<()> {
    let fixture = Fixture::new()?;
    let one = append(
        &fixture,
        recent::Observation {
            events: vec![event(1)],
            ..recent::Observation::empty()
        },
        1000,
    )?;
    append(
        &fixture,
        recent::Observation {
            events: vec![event(2)],
            ..recent::Observation::empty()
        },
        1001,
    )?;
    let read = fixture.store.current_read(&fixture.scope)?;
    // Faulting read port backed by actual retained map data. This is NOT an
    // on-disk corruption qualification or permission to skip strict checks.
    for (after, fault, success) in [
        (0, b"events/1/".as_slice(), false),
        (one.sequence, b"events/1/".as_slice(), true),
        (one.sequence, b"events/2/".as_slice(), false),
    ] {
        let outcome = recent::read_recent(
            &read.head().scope,
            read.token()?,
            1002,
            |_| Some(after),
            &mut |key| {
                let mut value = read.map_get(map::MapDomain::RecentChanges, key)?;
                if key.starts_with(fault)
                    && let Some(bytes) = value.as_mut()
                {
                    bytes[0] ^= 1;
                }
                Ok(value)
            },
        );
        if success {
            assert_eq!(outcome?.events.len(), 1);
        } else {
            let error = match outcome {
                Ok(_) => anyhow::bail!("corrupt selected/strict group accepted"),
                Err(error) => error,
            };
            assert!(
                error
                    .downcast_ref::<recent::InvalidRecentChanges>()
                    .is_some()
            );
        }
    }
    assert_eq!(
        read.recent_changes(1002)?.events.len(),
        2,
        "public strict path still materializes all actual stored groups"
    );
    Ok(())
}
