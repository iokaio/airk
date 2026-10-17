// SPDX-License-Identifier: Apache-2.0
//! Original tiny store fixtures: authoritative v2 state, receipts and mixed-format lifecycle.
use airk_core::changes::{
    ChangeCoverage, CoverageFloor, CoverageReason, FileStamps, FileState, LineageEnvelope,
    PublicationObservation, PublicationReason,
};
use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan};
use airk_core::identity::{
    ArtifactId, Incarnation, PublicationReceipt, RevisionToken, Sha256Digest, revision_digest,
};
use airk_core::publication::{InventoryFile, InventoryV2};
use airk_core::{ArtifactSource, DocumentLocation, Snapshot, SnapshotStore};
use airk_store::{
    ExpectedHead, FileStore,
    quota::{Quota, Reservation},
};
use anyhow::Result;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn prepare(
    store: &FileStore,
    text: &str,
    generation: u64,
    parent: Option<RevisionToken>,
) -> Result<(InventoryV2, PublicationReceipt, Reservation)> {
    let root = store.directory().parent().unwrap().parent().unwrap();
    let reservation = Quota::open(root)?.reserve("fixture", 16 * 1024 * 1024)?;
    let profile = ExtractionProfile {
        contract_version: 1,
        adapter_id: "text".into(),
        adapter_version: "fixture/1".into(),
        language: "text".into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 1,
        options: BTreeMap::new(),
        configuration_inputs: BTreeMap::new(),
    };
    let payload = ExtractionPayload {
        continuity_coverage: BTreeMap::new(),
        contract_version: 1,
        extraction_method: "utf8".into(),
        parse_status: "plain".into(),
        provenance: vec![],
        text: text.into(),
        facts: vec![],
        syntax_identities: vec![],
        chunks: vec![LocalChunk {
            span: LocalSpan {
                start: 0,
                end: text.len(),
            },
            start_line: 1,
            end_line: 1,
            location: DocumentLocation::Lines { start: 1, end: 1 },
            region: None,
        }],
        regions: vec![],
        relationships: vec![],
        continuity: vec![],
        diagnostics: vec![],
        partial: false,
    };
    let source_hash = digest(text.as_bytes());
    let artifact = airk_core::binding::bind_payload(
        &payload,
        &profile,
        airk_core::binding::ArtifactBinding {
            path: "note.txt",
            source_hash,
            source_bytes: text.len(),
        },
    )?;
    let compact = store.retain_payload_with_budget(
        &artifact,
        &payload,
        &profile,
        Some(text.as_bytes()),
        Some(&reservation),
    )?;
    let reference = compact.retained.as_ref().unwrap();
    let floor = CoverageFloor {
        complete_after: generation,
        reason: Some(CoverageReason::BaselineUnavailable),
    };
    let incarnation = parent.map_or(Incarnation::from_bytes([1; 16]), |token| token.incarnation);
    let lineage = LineageEnvelope {
        version: 1,
        incarnation,
        observation: PublicationObservation {
            generation,
            parent,
            observed_unix_seconds: generation,
            reason: PublicationReason::Index,
        },
        coverage: ChangeCoverage {
            files: floor.clone(),
            code: floor.clone(),
            documents: floor.clone(),
            ancestry: floor.clone(),
            resolution: floor,
        },
        ancestry: vec![],
        events: vec![],
        resolvers: vec![],
    };
    let inventory = InventoryV2 {
        embedding_pipeline_digest: None,
        index_detail: None,
        extraction_policy_digest: None,
        version: 2,
        incarnation,
        generation,
        collection: "fixture".into(),
        root: "/original".into(),
        roots: BTreeMap::new(),
        lineage_digest: digest(&serde_json::to_vec(&lineage)?),
        profiles: BTreeMap::from([(profile.identity()?, profile.clone())]),
        files: BTreeMap::from([(
            "note.txt".into(),
            InventoryFile {
                continuity: Vec::new(),
                continuity_coverage: Default::default(),
                state: FileState {
                    artifact_id: ArtifactId::from_bytes([2; 16]),
                    stamps: FileStamps {
                        first_generation: 1,
                        path_generation: 1,
                        content_generation: generation,
                        evidence_generation: generation,
                    },
                    source_hash,
                    profile_id: profile.identity()?,
                    payload_digest: reference.object.parse()?,
                    evidence_digest: digest(b"fixture-bound-evidence"),
                    root: "/original".into(),
                    path: "note.txt".into(),
                },
                source_bytes: text.len(),
                payload_bytes: reference.object_bytes,
                extracted_bytes: reference.extracted_bytes,
                chunks: 1,
                indexed_lines: 1,
                facts: 0,
                raw_source: true,
                language: "text".into(),
                extraction_method: "utf8".into(),
                parse_status: "plain".into(),
            },
        )]),
        retrieval_specification:
            "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-4/inventory-2".into(),
        embedding_model: None,
        extractor_version: "native-facts/7".into(),
        analysis: None,
        imported_facts: vec![],
        imported_relationships: vec![],
    };
    let bytes = inventory.canonical_bytes()?;
    let token = RevisionToken {
        incarnation,
        generation,
        revision: revision_digest(inventory.retrieval_specification.as_bytes(), &bytes),
    };
    let staging = reservation
        .staging_directory()?
        .join(format!("prepared-{generation}"));
    fs::create_dir_all(&staging)?;
    let snapshot = inventory.projection()?;
    let source = airk_store::blobs::RetainedSource {
        store,
        snapshot: &snapshot,
    };
    store.prepare_catalog_v2(
        &staging,
        &inventory,
        &lineage,
        &token,
        &source,
        Some(&reservation),
        2 * 1024 * 1024,
    )?;
    reservation.charge(bytes.len() as u64)?;
    fs::write(staging.join("inventory.json"), &bytes)?;
    let mut components = BTreeMap::new();
    for name in ["inventory.json", "lineage.json", "catalog.sqlite"] {
        let bytes = fs::read(staging.join(name))?;
        components.insert(name, json!({"bytes":bytes.len(),"sha256":digest(&bytes)}));
    }
    let manifest = serde_json::to_vec(
        &json!({"schema":2,"engine":inventory.retrieval_specification,"token":token,"collection":"fixture","chunks":1,"vectors":0,"model_sha256":null,"embedding":"none","inventory_digest":digest(&bytes),"components":components}),
    )?;
    reservation.charge(manifest.len() as u64)?;
    fs::write(staging.join("manifest.json"), &manifest)?;
    let lease =
        airk_store::blobs::reader_lease_path(store.directory(), &token.revision.to_string())?;
    fs::create_dir_all(lease.parent().unwrap())?;
    fs::write(lease, [])?;
    fs::create_dir_all(store.directory().join("generations"))?;
    fs::rename(
        staging,
        store
            .directory()
            .join("generations")
            .join(token.revision.to_string()),
    )?;
    Ok((
        inventory,
        PublicationReceipt {
            token,
            inventory_digest: digest(&bytes),
            manifest_digest: digest(&manifest),
        },
        reservation,
    ))
}

fn track_prepared(
    store: &FileStore,
    receipt: &PublicationReceipt,
    reservation: &Reservation,
) -> Result<()> {
    let target = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    let private = reservation.staging_directory()?.join("before-rename");
    fs::rename(&target, &private)?;
    reservation.track_prepared_publication(&store.expected_head()?, receipt)?;
    fs::rename(private, target)?;
    Ok(())
}

#[test]
fn prepared_publication_crash_child() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_PUBLICATION_CRASH_ROOT") else {
        return Ok(());
    };
    let root = std::path::PathBuf::from(root);
    let mode = std::env::var("AIRK_PUBLICATION_CRASH_MODE")?;
    let store = FileStore::new(root.join("collections/fixture"));
    let _writer = store.lock()?;
    let expected = store.expected_head()?;
    let parent = match expected {
        ExpectedHead::V2(token) => Some(token),
        _ => None,
    };
    let generation = parent.map_or(1, |token| token.generation + 1);
    let (inventory, receipt, reservation) =
        prepare(&store, "Crash recovery evidence.\n", generation, parent)?;
    track_prepared(&store, &receipt, &reservation)?;
    if mode == "committed" {
        store.publish_catalog_v2(&inventory, &receipt, &expected, &reservation)?;
    } else if mode == "detached" {
        fs::rename(
            store
                .directory()
                .join("generations")
                .join(receipt.token.revision.to_string()),
            reservation
                .staging_directory()?
                .join("abandoned-publication"),
        )?;
    }
    fs::write(
        root.join("crashed-receipt.json"),
        serde_json::to_vec(&receipt)?,
    )?;
    // Deliberately bypass reservation destructors, exactly at the rename/commit
    // boundary (or during recovery), without production fault-injection hooks.
    std::process::exit(0);
}

#[test]
fn dead_preparation_recovers_without_adopting_or_deleting_committed_evidence() -> Result<()> {
    for existing in [false, true] {
        for mode in ["prepared", "committed", "detached"] {
            let root = tempfile::tempdir()?;
            let store = FileStore::new(root.path().join("collections/fixture"));
            if existing {
                let (inventory, receipt, reservation) =
                    prepare(&store, "Previous evidence.\n", 1, None)?;
                store.publish_catalog_v2(
                    &inventory,
                    &receipt,
                    &ExpectedHead::Empty,
                    &reservation,
                )?;
            }
            let previous = store.expected_head()?;
            let child = std::process::Command::new(std::env::current_exe()?)
                .args(["--exact", "prepared_publication_crash_child", "--nocapture"])
                .env("AIRK_PUBLICATION_CRASH_ROOT", root.path())
                .env("AIRK_PUBLICATION_CRASH_MODE", mode)
                .output()?;
            assert!(
                child.status.success(),
                "{}",
                String::from_utf8_lossy(&child.stderr)
            );
            let receipt: PublicationReceipt =
                serde_json::from_slice(&fs::read(root.path().join("crashed-receipt.json"))?)?;
            let target = store
                .directory()
                .join("generations")
                .join(receipt.token.revision.to_string());
            let status = Quota::open(root.path())?.status()?;
            assert_eq!(status.reserved_bytes, 0);
            assert_eq!(target.exists(), mode == "committed");
            if mode == "committed" {
                assert_eq!(store.receipt(&receipt.token)?, Some(receipt.clone()));
                assert_eq!(store.expected_head()?, ExpectedHead::V2(receipt.token));
            } else {
                assert_eq!(store.expected_head()?, previous);
                let parent = match previous {
                    ExpectedHead::V2(token) => Some(token),
                    _ => None,
                };
                let (inventory, retry, reservation) = prepare(
                    &store,
                    "Crash recovery evidence.\n",
                    receipt.token.generation,
                    parent,
                )?;
                assert_eq!(
                    retry.token, receipt.token,
                    "same-second/same-revision retry remains possible"
                );
                track_prepared(&store, &retry, &reservation)?;
                store.publish_catalog_v2(&inventory, &retry, &previous, &reservation)?;
                drop(reservation);
                assert_eq!(store.receipt(&retry.token)?, Some(retry));
            }
        }
    }
    Ok(())
}

#[test]
fn prepared_recovery_refuses_corruption_and_respects_live_readers() -> Result<()> {
    for corrupt in [false, true] {
        let root = tempfile::tempdir()?;
        let store = FileStore::new(root.path().join("collections/fixture"));
        let (_, receipt, reservation) = prepare(&store, "Protected prepared evidence.\n", 1, None)?;
        assert!(
            reservation
                .track_prepared_publication(&ExpectedHead::Empty, &receipt)
                .is_err(),
            "an existing directory cannot be adopted by a new journal"
        );
        track_prepared(&store, &receipt, &reservation)?;
        let target = store
            .directory()
            .join("generations")
            .join(receipt.token.revision.to_string());
        let lease_path = airk_store::blobs::reader_lease_path(
            store.directory(),
            &receipt.token.revision.to_string(),
        )?;
        let reader = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(lease_path)?;
        reader.lock_shared()?;
        if corrupt {
            fs::write(target.join("manifest.json"), b"corrupt prepared manifest")?;
        }
        drop(reservation);
        assert!(Quota::open(root.path())?.status().is_err());
        assert!(target.exists());
        drop(reader);
        if corrupt {
            assert!(Quota::open(root.path())?.status().is_err());
            assert!(
                target.exists(),
                "corrupt journal-bound data is never silently deleted"
            );
        } else {
            Quota::open(root.path())?.status()?;
            assert!(!target.exists());
        }
        assert_eq!(store.expected_head()?, ExpectedHead::Empty);
    }
    Ok(())
}

#[test]
fn prepared_recovery_preserves_data_when_authoritative_head_changes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let (first_inventory, first, first_reservation) =
        prepare(&store, "First evidence.\n", 1, None)?;
    store.publish_catalog_v2(
        &first_inventory,
        &first,
        &ExpectedHead::Empty,
        &first_reservation,
    )?;
    drop(first_reservation);
    let (_, pending, pending_reservation) =
        prepare(&store, "Uncommitted evidence.\n", 2, Some(first.token))?;
    track_prepared(&store, &pending, &pending_reservation)?;
    let (other_inventory, other, other_reservation) = prepare(
        &store,
        "Different committed evidence.\n",
        2,
        Some(first.token),
    )?;
    store.publish_catalog_v2(
        &other_inventory,
        &other,
        &ExpectedHead::V2(first.token),
        &other_reservation,
    )?;
    drop(other_reservation);
    drop(pending_reservation);
    assert!(Quota::open(root.path())?.status().is_err());
    assert!(
        store
            .directory()
            .join("generations")
            .join(pending.token.revision.to_string())
            .exists()
    );
    assert_eq!(store.expected_head()?, ExpectedHead::V2(other.token));
    assert_eq!(store.receipt(&other.token)?, Some(other));
    Ok(())
}

#[test]
#[cfg(windows)]
fn empty_locked_coordination_component_remains_verifiable() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let (inventory, mut receipt, reservation) =
        prepare(&store, "Concurrent lexical reader fixture.\n", 1, None)?;
    let generation = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    fs::create_dir(generation.join("lexical"))?;
    let name = "lexical/.tantivy-meta.lock";
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(generation.join(name))?;
    lock.lock()?;
    let manifest_path = generation.join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    manifest["components"][name] = json!({"bytes":0,"sha256":digest(&[])});
    let manifest = serde_json::to_vec(&manifest)?;
    receipt.manifest_digest = digest(&manifest);
    fs::write(manifest_path, manifest)?;
    store.publish_catalog_v2(&inventory, &receipt, &ExpectedHead::Empty, &reservation)?;
    assert_eq!(store.catalog()?.unwrap().receipt, Some(receipt));
    drop(lock);
    fs::write(generation.join(name), b"unexpected nonempty lock component")?;
    assert!(store.catalog().is_err());
    Ok(())
}

#[test]
fn atomic_receipt_migration_reopen_and_indexed_metadata() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let mut legacy = Snapshot::empty("/original".into());
    legacy.collection = "fixture".into();
    store.save(&legacy)?;
    let expected = store.expected_head()?;
    let (inventory, receipt, reservation) =
        prepare(&store, "Original fictional evidence.\n", 1, None)?;
    assert!(
        store
            .publish_catalog_v2(&inventory, &receipt, &ExpectedHead::Empty, &reservation)
            .is_err()
    );
    assert_eq!(store.expected_head()?, expected);
    store.publish_catalog_v2(&inventory, &receipt, &expected, &reservation)?;
    assert_eq!(store.receipt(&receipt.token)?, Some(receipt.clone()));
    assert!(store.save(&legacy).is_err(), "old writer must fail closed");
    let catalog = store.catalog()?.unwrap();
    assert_eq!(
        catalog.load_artifact("note.txt")?.chunks[0].text,
        "Original fictional evidence.\n"
    );
    assert_eq!(catalog.file("note.txt")?.unwrap().indexed_lines, 1);
    assert!(catalog.file("missing.txt")?.is_none());
    // A verified live catalog uses its SQLite index, rather than re-reading inventory JSON.
    let inventory_path = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string())
        .join("inventory.json");
    let saved = fs::read(&inventory_path)?;
    fs::write(&inventory_path, b"corrupt")?;
    assert!(catalog.file("note.txt")?.is_some());
    assert!(store.catalog().is_err());
    fs::write(inventory_path, saved)?;
    let database = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string())
        .join("catalog.sqlite");
    let saved_database = fs::read(&database)?;
    let conn = rusqlite::Connection::open(&database)?;
    let mut changed = catalog.file("note.txt")?.unwrap();
    changed.indexed_lines = 99;
    conn.execute(
        "UPDATE files SET payload=?1 WHERE path='note.txt'",
        [serde_json::to_string(&changed)?],
    )?;
    assert!(
        catalog.file("note.txt").is_err(),
        "valid SQL edits must not bypass pinned inventory"
    );
    conn.execute("DELETE FROM files WHERE path='note.txt'", [])?;
    assert!(
        catalog.file("note.txt").is_err(),
        "missing indexed row is not a missing observed file"
    );
    drop(conn);
    fs::write(database, saved_database)?;
    drop(catalog);
    assert!(store.catalog()?.unwrap().inventory.is_some());
    Ok(())
}

#[test]
fn failed_catalog_and_receipt_tampering_do_not_advance_pointer() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let (first, receipt, reservation) = prepare(&store, "One.\n", 1, None)?;
    store.publish_catalog_v2(&first, &receipt, &ExpectedHead::Empty, &reservation)?;
    drop(reservation);
    let mut wrong_parent = receipt.token;
    wrong_parent.revision = digest(b"wrong parent revision");
    let (wrong, wrong_receipt, wrong_reservation) =
        prepare(&store, "Wrong parent.\n", 2, Some(wrong_parent))?;
    assert!(
        store
            .publish_catalog_v2(
                &wrong,
                &wrong_receipt,
                &ExpectedHead::V2(receipt.token),
                &wrong_reservation
            )
            .is_err()
    );
    drop(wrong_reservation);
    let (second, candidate, reservation) = prepare(&store, "Two.\n", 2, Some(receipt.token))?;
    fs::write(
        store
            .directory()
            .join("generations")
            .join(candidate.token.revision.to_string())
            .join("catalog.sqlite"),
        b"corrupt",
    )?;
    assert!(
        store
            .publish_catalog_v2(
                &second,
                &candidate,
                &ExpectedHead::V2(receipt.token),
                &reservation
            )
            .is_err()
    );
    assert_eq!(store.expected_head()?, ExpectedHead::V2(receipt.token));
    assert!(store.receipt(&candidate.token)?.is_none());
    let conn = rusqlite::Connection::open(store.directory().join("index.sqlite3"))?;
    conn.execute("DELETE FROM publication_receipts", [])?;
    assert!(store.persistent_revision().is_err());
    assert!(store.catalog().is_err());
    Ok(())
}

#[test]
fn warmed_fact_verification_detects_same_size_corruption_replacement_and_new_publication()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let (first, receipt, reservation) = prepare(&store, "Original one.\n", 1, None)?;
    store.publish_catalog_v2(&first, &receipt, &ExpectedHead::Empty, &reservation)?;
    drop(reservation);
    let query = airk_core::FactQuery {
        path_prefix: String::new(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        limit: 10,
        max_bytes: 4096,
        path_exact: false,
        document_only: false,
    };
    for _ in 0..3 {
        assert_eq!(
            store
                .facts("/original", "fixture", &query, false)?
                .unwrap()
                .generation,
            1
        );
    }
    let directory = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    let path = directory.join("catalog.sqlite");
    let original = fs::read(&path)?;
    let modified = fs::metadata(&path)?.modified()?;
    let mut corrupt = original.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    fs::write(&path, &corrupt)?;
    fs::OpenOptions::new()
        .write(true)
        .open(&path)?
        .set_modified(modified)?;
    assert!(
        store.facts("/original", "fixture", &query, false).is_err(),
        "same size and timestamp must not authenticate changed bytes"
    );
    fs::write(&path, &original)?;
    assert!(
        store
            .facts("/original", "fixture", &query, false)?
            .is_some()
    );
    let replaced = root.path().join("saved-catalog");
    fs::rename(&path, &replaced)?;
    fs::write(&path, &corrupt)?;
    assert!(
        store.facts("/original", "fixture", &query, false).is_err(),
        "replacement must be re-authenticated"
    );
    fs::remove_file(&path)?;
    fs::rename(&replaced, &path)?;
    let overlay = directory.join("catalog.sqlite-wal");
    fs::write(&overlay, b"unexpected overlay")?;
    assert!(store.facts("/original", "fixture", &query, false).is_err());
    fs::remove_file(overlay)?;
    let (second, next, reservation) = prepare(&store, "Original two.\n", 2, Some(receipt.token))?;
    store.publish_catalog_v2(
        &second,
        &next,
        &ExpectedHead::V2(receipt.token),
        &reservation,
    )?;
    assert_eq!(
        store
            .facts("/original", "fixture", &query, false)?
            .unwrap()
            .generation,
        2
    );
    assert!(store.facts("/original", "foreign", &query, false).is_err());
    Ok(())
}

#[test]
fn failed_json_migration_never_shadows_legacy_state_with_an_empty_database() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let mut snapshot = Snapshot::empty("/original".into());
    snapshot.collection = "fixture".into();
    snapshot.generation = 9;
    fs::write(
        store.directory().join("snapshot.json"),
        serde_json::to_vec_pretty(&snapshot)?,
    )?;
    let expected = store.expected_head()?;
    let (wrong_sequence, receipt, reservation) =
        prepare(&store, "Private migration candidate.\n", 1, None)?;
    assert!(
        store
            .publish_catalog_v2(
                &wrong_sequence,
                &receipt,
                &ExpectedHead::Empty,
                &reservation
            )
            .is_err()
    );
    assert!(!store.directory().join("index.sqlite3").exists());
    assert_eq!(store.load()?.unwrap().generation, 9);
    assert!(
        store
            .publish_catalog_v2(&wrong_sequence, &receipt, &expected, &reservation)
            .is_err()
    );
    assert!(!store.directory().join("index.sqlite3").exists());
    assert_eq!(store.load()?.unwrap().generation, 9);
    drop(reservation);
    let (valid, receipt, reservation) = prepare(&store, "Successful migration.\n", 10, None)?;
    store.publish_catalog_v2(&valid, &receipt, &expected, &reservation)?;
    assert_eq!(store.catalog()?.unwrap().snapshot.generation, 10);
    Ok(())
}

#[test]
fn unlisted_sqlite_sidecars_are_rejected_by_cold_catalog_verification() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let (inventory, receipt, reservation) = prepare(&store, "Anchored main database.\n", 1, None)?;
    store.publish_catalog_v2(&inventory, &receipt, &ExpectedHead::Empty, &reservation)?;
    let catalog = store.catalog()?.unwrap();
    let generation = store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    fs::write(generation.join("catalog.sqlite-wal"), b"unlisted overlay")?;
    assert!(store.catalog().is_err());
    assert!(
        catalog.file("note.txt")?.is_some(),
        "immutable pinned metadata must ignore sidecars"
    );
    fs::remove_file(generation.join("catalog.sqlite-wal"))?;
    fs::create_dir(generation.join("unlisted-directory"))?;
    assert!(store.catalog().is_err());
    Ok(())
}

#[test]
fn reader_leases_protect_v2_payloads_and_unpinned_objects_are_reclaimed() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let (first, receipt, reservation) = prepare(&store, "Old.\n", 1, None)?;
    store.publish_catalog_v2(&first, &receipt, &ExpectedHead::Empty, &reservation)?;
    drop(reservation);
    let old = store.catalog()?.unwrap();
    let (second, candidate, reservation) = prepare(&store, "New.\n", 2, Some(receipt.token))?;
    store.publish_catalog_v2(
        &second,
        &candidate,
        &ExpectedHead::V2(receipt.token),
        &reservation,
    )?;
    let active = candidate.token.revision.to_string();
    assert_eq!(
        airk_store::retention::prune(store.directory(), &active, false)?.pinned,
        vec![receipt.token.revision.to_string()]
    );
    assert_eq!(old.load_artifact("note.txt")?.chunks[0].text, "Old.\n");
    drop(old);
    let report = airk_store::retention::prune(store.directory(), &active, false)?;
    assert!(report.removed_bytes > 0);
    assert_eq!(report.retired_receipts, 1);
    assert!(
        !report.receipt_cleanup_pending,
        "{:?}",
        report.receipt_cleanup_error
    );
    assert!(store.receipt(&receipt.token)?.is_none());
    assert!(store.catalog()?.unwrap().file("note.txt")?.is_some());
    Ok(())
}

#[test]
fn legacy_original_inventory_bytes_survive_new_default_fields() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let mut snapshot = Snapshot::empty("/original".into());
    snapshot.collection = "fixture".into();
    snapshot.storage_format = 1;
    snapshot.persistent_indexes = true;
    let bytes = serde_json::to_vec_pretty(&snapshot)?;
    let engine = "fixture-legacy/generation-3";
    let mut hash = Sha256::new();
    hash.update(engine.as_bytes());
    hash.update(&bytes);
    let revision = format!("{:x}", hash.finalize());
    store.save_with_components(&snapshot, &revision)?;
    let mut header = store.persistent_revision()?.unwrap();
    header.inventory_sha256 = Some(digest(&bytes).to_string());
    let conn = rusqlite::Connection::open(store.directory().join("index.sqlite3"))?;
    conn.execute(
        "UPDATE index_components SET payload=?1",
        [serde_json::to_string(&header)?],
    )?;
    let generation = store.directory().join("generations").join(&revision);
    fs::create_dir_all(&generation)?;
    fs::write(generation.join("inventory.json"), &bytes)?;
    fs::write(
        generation.join("manifest.json"),
        serde_json::to_vec(&json!({"engine":engine}))?,
    )?;
    let lease = airk_store::blobs::reader_lease_path(store.directory(), &revision)?;
    fs::create_dir_all(lease.parent().unwrap())?;
    fs::write(lease, [])?;
    assert!(store.catalog()?.is_some());
    assert_ne!(
        bytes,
        serde_json::to_vec(&store.catalog()?.unwrap().snapshot)?
    );
    let expected = store.expected_head()?;
    conn.execute("UPDATE revision SET payload=payload || ' '", [])?;
    assert_ne!(
        expected,
        store.expected_head()?,
        "legacy expectation binds original bytes"
    );
    Ok(())
}

#[test]
fn retirement_admission_failure_preserves_all_evidence_until_retry() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let writer = store.lock()?;
    let (first, receipt, reservation) = prepare(&store, "Old quota fixture.\n", 1, None)?;
    store.publish_catalog_v2(&first, &receipt, &ExpectedHead::Empty, &reservation)?;
    drop(reservation);
    let (second, current, reservation) =
        prepare(&store, "Current quota fixture.\n", 2, Some(receipt.token))?;
    store.publish_catalog_v2(
        &second,
        &current,
        &ExpectedHead::V2(receipt.token),
        &reservation,
    )?;
    drop(reservation);
    let quota = Quota::open(root.path())?;
    let used = quota.status()?;
    quota.configure(airk_store::quota::QuotaPolicy {
        root_bytes: used.used_bytes + 1,
        default_collection_bytes: used.collections["fixture"].used_bytes + 1,
        collections: BTreeMap::new(),
    })?;
    assert!(
        airk_store::retention::prune(
            store.directory(),
            &current.token.revision.to_string(),
            false,
        )
        .is_err()
    );
    let rejected = store.maintain_retention(
        &writer,
        &quota,
        airk_store::maintenance::MaintenanceTrigger::ExplicitLegacyPrune,
        airk_store::maintenance::MaintenanceBudget::default(),
    );
    assert_eq!(
        rejected.receipt.state,
        airk_store::maintenance::MaintenanceState::Blocked
    );
    let pressure = rejected
        .receipt
        .pressure
        .as_ref()
        .expect("typed quota rejection");
    assert!(pressure.required_bytes > pressure.available_bytes);
    assert_eq!(rejected.receipt.reclaimed_bytes, 0);
    assert_eq!(rejected.receipt.retired_receipts, 0);
    assert!(rejected.receipt.pending);
    assert!(store.receipt(&receipt.token)?.is_some());
    assert!(
        store
            .directory()
            .join("generations")
            .join(receipt.token.revision.to_string())
            .is_dir()
    );
    {
        let old = store
            .catalog_at(&receipt.token, 16 * 1024 * 1024)?
            .expect("old evidence remains readable");
        let artifact = &old.snapshot.artifacts["note.txt"];
        assert_eq!(
            store.load_retained(artifact)?.chunks[0].text,
            "Old quota fixture.\n"
        );
        assert_eq!(
            store.read_retained_source(artifact)?,
            b"Old quota fixture.\n"
        );
    }
    assert_eq!(
        store.persistent_revision()?.unwrap().token,
        Some(current.token)
    );
    quota.configure(airk_store::quota::QuotaPolicy::default())?;
    let retried = airk_store::retention::prune(
        store.directory(),
        &current.token.revision.to_string(),
        false,
    )?;
    assert!(retried.removed_bytes > 0);
    assert_eq!(retried.retired_receipts, 1);
    assert!(!retried.receipt_cleanup_pending);
    assert!(store.receipt(&receipt.token)?.is_none());
    assert!(
        store
            .catalog_at(&receipt.token, 16 * 1024 * 1024)?
            .is_none()
    );
    assert_eq!(
        store.persistent_revision()?.unwrap().token,
        Some(current.token)
    );
    assert!(
        store
            .catalog_at(&current.token, 16 * 1024 * 1024)?
            .is_some()
    );
    Ok(())
}

#[test]
fn relocation_preserves_v2_receipts_and_source_bindings() -> Result<()> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("state");
    let store = FileStore::new(source.join("collections/fixture"));
    let writer = store.lock()?;
    let (inventory, receipt, reservation) = prepare(&store, "Relocated evidence.\n", 1, None)?;
    store.publish_catalog_v2(&inventory, &receipt, &ExpectedHead::Empty, &reservation)?;
    drop(reservation);
    drop(writer);
    let destination = root.path().join("destination");
    airk_store::relocation::relocate(&source, &destination)?;
    let moved = FileStore::new(destination.join("collections/fixture"));
    assert_eq!(moved.receipt(&receipt.token)?, Some(receipt));
    assert_eq!(
        moved.catalog()?.unwrap().inventory.unwrap().root,
        "/original"
    );
    assert!(Path::new(&source).exists());
    Ok(())
}
