// SPDX-License-Identifier: Apache-2.0
//! Actual canonical Current storage, independent copied archive admission.
use super::*;
use crate::legacy_adoption::{CaptureLimits, VerifiedLegacyArchive, capture_tree, copy_tree};

fn limits() -> CaptureLimits {
    CaptureLimits {
        max_files: 1000,
        max_bytes: 64 << 20,
        max_file_bytes: 32 << 20,
        max_depth: 16,
        max_snapshot_bytes: 16 << 20,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn balanced_input(fixture: &Fixture) -> Result<CapturedInput> {
    let text = "saved evidence without live source";
    let initial = retained_input_at(fixture, "evidence.txt", text)?;
    let payload = fixture.store.load_extraction_payload(&initial.artifact)?;
    // This text fixture has no facts, relationships or continuity to filter.
    // Rebind and retain the complete payload under an explicit Balanced
    // profile; changing only collection configuration would be invalid.
    ensure!(
        payload.facts.is_empty()
            && payload.relationships.is_empty()
            && payload.continuity.is_empty(),
        "Balanced fixture needs an explicitly filtered text payload"
    );
    let mut profile = initial
        .artifact
        .retained
        .as_ref()
        .and_then(|retained| retained.profile.clone())
        .context("fixture extraction profile missing")?;
    profile
        .options
        .insert("index_detail".into(), "balanced".into());
    ensure!(
        airk_core::extraction::IndexDetail::from_profile(&profile)?
            == airk_core::extraction::IndexDetail::Balanced,
        "fixture profile differs from applied coverage"
    );
    let artifact = airk_core::binding::bind_payload(
        &payload,
        &profile,
        airk_core::binding::ArtifactBinding {
            path: "evidence.txt",
            source_hash: initial.artifact.content_hash.parse()?,
            source_bytes: text.len(),
        },
    )?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 1 << 20)?;
    let retained = fixture.store.retain_payload_with_budget(
        &artifact,
        &payload,
        &profile,
        Some(text.as_bytes()),
        Some(&reservation),
    )?;
    let input = CapturedInput::new(retained);
    input.verify(&fixture.store)?;
    Ok(input)
}
fn saved_current() -> Result<Fixture> {
    let mut fixture = Fixture::empty()?;
    let lexical = lexical::LexicalManifest::empty()?;
    fixture.head.lexical_commit = lexical.digest()?;
    let input = balanced_input(&fixture)?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.initialize(fixture.head.clone())?;
    current
        .install_initial_lexical_manifest(&fixture.head.token()?, &lexical.canonical_bytes()?)?;
    let configuration = collection::CollectionConfiguration {
        retained_profile: None,
        source_policy_digest: Some(Sha256Digest::from_bytes([77; 32])),
        roots: Default::default(),
        extractor_version: "historical-current-fixture/1".into(),
        extraction_policy_digest: Sha256Digest::from_bytes([78; 32]),
        index_detail: airk_core::extraction::IndexDetail::Balanced,
        vector_selection: Default::default(),
        model: None,
    };
    let configured = current.initialize_collection(&fixture.head.token()?, &configuration)?;
    current.commit_artifact(&configured.token, "evidence.txt", Some(&input))?;
    drop(current);
    drop(writer);
    Ok(fixture)
}
fn copy(
    fixture: &Fixture,
) -> Result<(
    tempfile::TempDir,
    std::path::PathBuf,
    crate::legacy_adoption::LegacyInventory,
)> {
    let owner = tempfile::tempdir()?;
    let target = owner
        .path()
        .join("collections-1.2")
        .join(&fixture.scope.collection);
    fs::create_dir_all(&target)?;
    let inventory = capture_tree(fixture.store.directory(), limits(), deadline())?;
    copy_tree(
        fixture.store.directory(),
        &target,
        &inventory,
        limits(),
        deadline(),
        &mut || Ok(()),
    )?;
    Ok((owner, target, inventory))
}

#[test]
fn historical_current_archive_preserves_actual_token_configuration_and_retained_payload()
-> Result<()> {
    let fixture = saved_current()?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let token = read.token()?;
    let original_artifact = fixture
        .store
        .load_retained_bounded(&read.artifact("evidence.txt")?.unwrap(), 1 << 20)?;
    drop(read);
    let (_copy, target, inventory) = copy(&fixture)?;
    fs::remove_dir(fixture.directory.path().join("source"))?;
    let archive = VerifiedLegacyArchive::open_discovered_validation_copy(
        &target,
        &inventory,
        limits(),
        deadline(),
    )?;
    assert!(archive.identity().header.is_none());
    let original = archive
        .identity()
        .current
        .as_ref()
        .context("Current evidence missing")?;
    assert_eq!(original.token, token);
    assert_eq!(archive.identity().original_root_sha256, token.digest);
    assert_eq!(
        original.collection.configuration.index_detail,
        airk_core::extraction::IndexDetail::Balanced
    );
    assert_eq!(
        original.collection.configuration.source_policy_digest,
        Some(Sha256Digest::from_bytes([77; 32]))
    );
    let saved = archive.artifact("evidence.txt", 1 << 20)?;
    let mut expected = original_artifact;
    expected.retained = None;
    assert_eq!(
        serde_json::to_value(saved.artifact)?,
        serde_json::to_value(expected)?
    );
    assert!(saved.profile.is_some() && saved.payload.is_some());
    let saved_scope = archive.scope().clone();
    drop(archive);
    let reopened = VerifiedLegacyArchive::open_validation_copy(
        &target,
        saved_scope,
        &inventory,
        limits(),
        deadline(),
    )?;
    assert_eq!(reopened.identity().original_root_sha256, token.digest);
    assert!(
        reopened
            .artifact("evidence.txt", 1 << 20)?
            .payload
            .is_some()
    );
    drop(reopened);
    assert_eq!(
        capture_tree(fixture.store.directory(), limits(), deadline())?,
        inventory,
        "original bytes changed"
    );
    assert!(!fixture.directory.path().join("source").exists());
    Ok(())
}

#[test]
fn historical_current_archive_refuses_mixed_missing_owner_and_prepared_state() -> Result<()> {
    let fixture = saved_current()?;
    for failure in ["mixed", "owner_missing", "prepared"] {
        let (_copy, target, _) = copy(&fixture)?;
        match failure {
            "mixed" => fs::write(target.join("snapshot.json"), b"{}")?,
            "owner_missing" => fs::remove_file(target.join("current-owner.json"))?,
            "prepared" => {
                let db = rusqlite::Connection::open(target.join("current.sqlite3"))?;
                db.execute("UPDATE current_state SET phase='prepared' WHERE id=1", [])?;
            }
            _ => unreachable!(),
        }
        let inventory = capture_tree(&target, limits(), deadline())?;
        assert!(
            VerifiedLegacyArchive::open_discovered_validation_copy(
                &target,
                &inventory,
                limits(),
                deadline()
            )
            .is_err(),
            "accepted {failure}"
        );
    }
    let (_copy, target, inventory) = copy(&fixture)?;
    fs::write(target.join("unexpected"), b"crossed archive")?;
    assert!(
        VerifiedLegacyArchive::open_discovered_validation_copy(
            &target,
            &inventory,
            limits(),
            deadline()
        )
        .is_err()
    );
    let journal_owner = tempfile::tempdir()?;
    let journal_only = journal_owner.path().join("collections-1.2/project-current");
    fs::create_dir_all(&journal_only)?;
    fs::write(
        journal_only.join("current.sqlite3-journal"),
        b"interrupted Current owner",
    )?;
    fs::write(journal_only.join("snapshot.json"), b"{}")?;
    let inventory = capture_tree(&journal_only, limits(), deadline())?;
    let error = match VerifiedLegacyArchive::open_discovered_validation_copy(
        &journal_only,
        &inventory,
        limits(),
        deadline(),
    ) {
        Ok(_) => anyhow::bail!("journal-only Current marker fell through to catalog"),
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("mixed historical Current"));
    Ok(())
}
