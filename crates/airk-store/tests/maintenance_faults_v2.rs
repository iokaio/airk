// SPDX-License-Identifier: Apache-2.0
//! Original tiny store fixtures: authoritative v2 state, receipts and mixed-format lifecycle.
use airk_core::DocumentLocation;
use airk_core::changes::{
    ChangeCoverage, CoverageFloor, CoverageReason, FileStamps, FileState, LineageEnvelope,
    PublicationObservation, PublicationReason,
};
use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan};
use airk_core::identity::{
    ArtifactId, Incarnation, PublicationReceipt, RevisionToken, Sha256Digest, revision_digest,
};
use airk_core::publication::{InventoryFile, InventoryV2};
use airk_store::{
    ExpectedHead, FileStore,
    quota::{Quota, Reservation},
};
use airk_store::{
    WriterLock,
    retention::{
        MaintenanceBudget, MaintenanceStage, MaintenanceState, MaintenanceTrigger, RetentionMode,
        RetentionPolicy,
    },
};
use anyhow::{Result, ensure};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};

struct Fixture {
    _root: tempfile::TempDir,
    store: FileStore,
    quota: Quota,
    writer: WriterLock,
    tokens: Vec<RevisionToken>,
}
impl Fixture {
    fn new() -> Result<Self> {
        Self::with_generations(4)
    }
    fn with_generations(count: u64) -> Result<Self> {
        let root = tempfile::tempdir()?;
        let store = FileStore::new(root.path().join("collections/fixture"));
        let writer = store.lock()?;
        let quota = Quota::open(root.path())?;
        let mut tokens = vec![];
        for generation in 1..=count {
            let parent = tokens.last().copied();
            let (inventory, receipt, reservation) = prepare(
                &store,
                &format!("Original retirement fixture generation {generation}.\n"),
                generation,
                parent,
            )?;
            let expected = parent.map_or(ExpectedHead::Empty, ExpectedHead::V2);
            store.publish_catalog_v2(&inventory, &receipt, &expected, &reservation)?;
            tokens.push(receipt.token);
        }
        store.configure_retention(
            &writer,
            &quota,
            RetentionPolicy {
                mode: RetentionMode::Automatic,
                searchable_predecessors: 1,
                ..Default::default()
            },
        )?;
        Ok(Self {
            _root: root,
            store,
            quota,
            writer,
            tokens,
        })
    }
    fn maintain(&self) -> airk_store::retention::MaintenanceRun {
        self.store.maintain_retention(
            &self.writer,
            &self.quota,
            MaintenanceTrigger::ExplicitMaintenance,
            MaintenanceBudget::default(),
        )
    }
}

fn complete_recovery(store: &FileStore, writer: &WriterLock, quota: &Quota) -> Result<()> {
    let head = store.persistent_revision()?.unwrap().token;
    for _ in 0..8 {
        let run = store.maintain_retention(
            writer,
            quota,
            MaintenanceTrigger::ExplicitMaintenance,
            MaintenanceBudget::default(),
        );
        ensure!(
            run.receipt_persisted && run.receipt.head == head && run.receipt.pressure.is_none(),
            "invalid recovery receipt: {run:?}"
        );
        ensure!(
            matches!(
                run.receipt.state,
                MaintenanceState::Complete | MaintenanceState::Pending
            ) && run.receipt.pending == (run.receipt.state == MaintenanceState::Pending),
            "recovery failed rather than deferring bounded work: {run:?}"
        );
        assert_eq!(store.persistent_revision()?.unwrap().token, head);
        assert_eq!(quota.status()?.active_reservations, 0);
        if !run.receipt.pending {
            ensure!(
                run.receipt.error.is_none(),
                "completed recovery has an error: {run:?}"
            );
            assert_eq!(store.retention_status()?.pending_intents, 0);
            return Ok(());
        }
    }
    anyhow::bail!(
        "bounded recovery did not converge: {:?}",
        store.retention_status()?
    )
}

#[test]
fn every_durable_retirement_boundary_recovers_idempotently() -> Result<()> {
    for stage in [
        MaintenanceStage::IntentDurable,
        MaintenanceStage::RetirementAuthorized,
        MaintenanceStage::Detached,
        MaintenanceStage::PhysicalRemovalComplete,
        MaintenanceStage::IntentComplete,
    ] {
        let fixture = Fixture::new()?;
        let visited = std::cell::Cell::new(false);
        let run = fixture.store.maintain_retention_with_observer(
            &fixture.writer,
            &fixture.quota,
            MaintenanceTrigger::ExplicitMaintenance,
            MaintenanceBudget::default(),
            &|observed, _intent| {
                if observed == stage && !visited.replace(true) {
                    anyhow::bail!("original interruption at {stage:?}");
                }
                Ok(())
            },
        );
        assert!(visited.get(), "did not reach {stage:?}");
        assert!(
            run.receipt.pending,
            "interrupted maintenance needs continuation"
        );
        assert!(run.receipt.error.is_some());
        assert_eq!(
            fixture.store.persistent_revision()?.unwrap().token,
            fixture.tokens.last().copied()
        );
        if stage == MaintenanceStage::PhysicalRemovalComplete {
            // Explicit prune keeps only active state, so pin both remaining
            // nonactive revisions behind real reader leases:
            // no immediate candidate remains, but the durable intent still
            // requires receipt cleanup and prevents a complete object estimate.
            let remaining_readers = fixture.tokens[1..3]
                .iter()
                .map(|token| fixture.store.catalog_at(token, 1_000_000))
                .collect::<Result<Vec<_>>>()?;
            assert!(remaining_readers.iter().all(Option::is_some));
            let preview = fixture
                .store
                .prune_retention(&fixture.writer, &fixture.quota, true)?;
            assert!(preview.candidates.is_empty());
            assert!(
                preview.receipt_cleanup_pending,
                "unfinished durable intent must remain visible in dry-run reporting"
            );
            drop(remaining_readers);
        }
        complete_recovery(&fixture.store, &fixture.writer, &fixture.quota)?;
        complete_recovery(&fixture.store, &fixture.writer, &fixture.quota)?;
        assert_eq!(fixture.store.retention_status()?.pending_intents, 0);
        for token in &fixture.tokens[..2] {
            assert!(fixture.store.catalog_at(token, 1_000_000)?.is_none());
        }
        for token in &fixture.tokens[2..] {
            assert!(fixture.store.catalog_at(token, 1_000_000)?.is_some());
        }
        assert_eq!(fixture.quota.status()?.active_reservations, 0);
    }
    Ok(())
}

#[test]
fn missing_unretired_generation_is_corruption_and_unknown_removing_tree_survives() -> Result<()> {
    let fixture = Fixture::new()?;
    let token = fixture.tokens[0];
    let path = fixture
        .store
        .directory()
        .join("generations")
        .join(token.revision.to_string());
    let detached = path.with_file_name(format!("removing-{}", token.revision));
    fs::rename(&path, &detached)?;
    assert!(fixture.store.catalog_at(&token, 1_000_000).is_err());
    let run = fixture.maintain();
    assert!(run.receipt.error.is_some() || run.receipt.state == MaintenanceState::Blocked);
    assert!(
        detached.exists(),
        "unproven detached state must never be deleted"
    );
    assert!(fixture.store.receipt(&token)?.is_some());
    Ok(())
}

#[test]
fn malformed_and_foreign_retirement_intents_fail_closed() -> Result<()> {
    for foreign in [false, true] {
        let fixture = Fixture::new()?;
        let _ = fixture.store.maintain_retention_with_observer(
            &fixture.writer,
            &fixture.quota,
            MaintenanceTrigger::ExplicitMaintenance,
            MaintenanceBudget::default(),
            &|stage, _| {
                if stage == MaintenanceStage::IntentDurable {
                    anyhow::bail!("stop before retirement authorization");
                }
                Ok(())
            },
        );
        let path = fs::read_dir(fixture.store.directory().join("maintenance/intents"))?
            .next()
            .expect("durable intent missing")?
            .path();
        if foreign {
            let mut intent: airk_store::retention::MaintenanceIntent =
                serde_json::from_slice(&fs::read(&path)?)?;
            intent.token.incarnation = Incarnation::from_bytes([99; 16]);
            fs::write(&path, serde_json::to_vec(&intent)?)?;
        } else {
            fs::write(&path, b"{malformed original intent")?;
        }
        assert!(fixture.maintain().receipt.error.is_some());
        assert!(
            path.exists(),
            "unvalidated intent must remain for inspection"
        );
        for token in &fixture.tokens {
            assert!(
                fixture
                    .store
                    .directory()
                    .join("generations")
                    .join(token.revision.to_string())
                    .exists()
            );
        }
    }
    Ok(())
}

#[test]
fn legacy_generation_is_protected_at_migration_and_by_reader_then_reclaimed() -> Result<()> {
    for count in [1, 4] {
        let fixture = Fixture::with_generations(count)?;
        let mut snapshot = airk_core::Snapshot::empty("/original".into());
        snapshot.collection = "fixture".into();
        snapshot.storage_format = 1;
        snapshot.persistent_indexes = true;
        let bytes = serde_json::to_vec_pretty(&snapshot)?;
        let engine = "tantivy-original-fixture/generation-3";
        let mut hash = Sha256::new();
        hash.update(engine.as_bytes());
        hash.update(&bytes);
        let revision = format!("{:x}", hash.finalize());
        let directory = fixture
            .store
            .directory()
            .join("generations")
            .join(&revision);
        fs::create_dir(&directory)?;
        fs::write(directory.join("inventory.json"), bytes)?;
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&json!({
                "schema":1,"engine":engine,"revision":revision,"collection":"fixture"
            }))?,
        )?;
        let lease_path =
            airk_store::blobs::reader_lease_path(fixture.store.directory(), &revision)?;
        let lease = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lease_path)?;
        lease.lock_shared()?;
        assert!(fixture.maintain().receipt.error.is_none());
        assert!(
            directory.exists(),
            "legacy reader must protect its complete representation"
        );
        drop(lease);
        for _ in 0..4 {
            let run = fixture.maintain();
            assert!(run.receipt.error.is_none(), "{:?}", run.receipt.error);
            if count == 1 || !directory.exists() {
                break;
            }
        }
        assert_eq!(
            directory.exists(),
            count == 1,
            "parentless migration retains conservative legacy predecessor"
        );
    }
    Ok(())
}

#[test]
fn corrupt_protected_inventory_prevents_gc_of_unreferenced_objects() -> Result<()> {
    let fixture = Fixture::new()?;
    let active = *fixture.tokens.last().unwrap();
    let orphan = fixture
        .store
        .directory()
        .join("objects/sources")
        .join("ab".repeat(32));
    fs::create_dir_all(orphan.parent().unwrap())?;
    fs::write(&orphan, b"original unreferenced fixture")?;
    let inventory = fixture
        .store
        .directory()
        .join("generations")
        .join(active.revision.to_string())
        .join("inventory.json");
    fs::write(inventory, b"corrupt protected inventory")?;
    let run = fixture.maintain();
    assert!(run.receipt.error.is_some());
    assert!(
        orphan.exists(),
        "invalid surviving root must stop object sweep"
    );
    assert_eq!(
        fixture.store.persistent_revision()?.unwrap().token,
        Some(active)
    );
    Ok(())
}

#[test]
fn borrowed_reader_lease_defers_retirement_and_preserves_payload() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = fixture.tokens[0];
    let reader = fixture.store.catalog_at(&first, 1_000_000)?.unwrap();
    let run = fixture.maintain();
    assert!(run.receipt.error.is_none());
    assert!(reader.file("note.txt")?.is_some());
    assert!(fixture.store.catalog_at(&first, 1_000_000)?.is_some());
    drop(reader);
    complete_recovery(&fixture.store, &fixture.writer, &fixture.quota)?;
    assert!(fixture.store.catalog_at(&first, 1_000_000)?.is_none());
    Ok(())
}

#[test]
fn process_exit_at_each_retirement_boundary_recovers_without_drop_cleanup() -> Result<()> {
    for stage in [
        MaintenanceStage::IntentDurable,
        MaintenanceStage::RetirementAuthorized,
        MaintenanceStage::Detached,
        MaintenanceStage::PhysicalRemovalComplete,
        MaintenanceStage::IntentComplete,
    ] {
        let Fixture {
            _root,
            store,
            quota,
            writer,
            tokens,
        } = Fixture::new()?;
        drop(writer);
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(["--ignored", "--exact", "maintenance_crash_child"])
            .env("AIRK_TEST_MAINTENANCE_DIRECTORY", store.directory())
            .env(
                "AIRK_TEST_MAINTENANCE_STAGE",
                serde_json::to_string(&stage)?,
            )
            .status()?;
        assert_eq!(status.code(), Some(81), "child did not reach {stage:?}");
        let writer = store.lock()?;
        complete_recovery(&store, &writer, &quota)?;
        assert_eq!(
            store.persistent_revision()?.unwrap().token,
            tokens.last().copied()
        );
        assert_eq!(store.retention_status()?.pending_intents, 0);
        assert_eq!(quota.status()?.active_reservations, 0);
        assert!(store.catalog_at(&tokens[0], 1_000_000)?.is_none());
        drop(writer);
        drop(quota);
        drop(store);
        drop(_root);
    }
    Ok(())
}

#[test]
#[ignore = "isolated child-process crash fixture"]
fn maintenance_crash_child() -> Result<()> {
    let Some(directory) = std::env::var_os("AIRK_TEST_MAINTENANCE_DIRECTORY") else {
        return Ok(());
    };
    let stage: MaintenanceStage =
        serde_json::from_str(&std::env::var("AIRK_TEST_MAINTENANCE_STAGE")?)?;
    let store = FileStore::new(std::path::PathBuf::from(directory));
    let root = store.directory().parent().unwrap().parent().unwrap();
    let quota = Quota::open(root)?;
    let writer = store.lock()?;
    let _ = store.maintain_retention_with_observer(
        &writer,
        &quota,
        MaintenanceTrigger::ExplicitMaintenance,
        MaintenanceBudget::default(),
        &|observed, _intent| {
            if observed == stage {
                std::process::exit(81);
            }
            Ok(())
        },
    );
    anyhow::bail!("requested maintenance crash boundary was not reached")
}

#[test]
fn quota_pressure_keeps_typed_cause_through_reserved_writer_io() -> Result<()> {
    use std::io::Write;
    let root = tempfile::tempdir()?;
    let quota = Quota::open(root.path())?;
    let reservation = quota.reserve("fixture", 1)?;
    let failure = reservation.charge(2).unwrap_err();
    assert!(
        failure
            .downcast_ref::<airk_store::quota::QuotaPressure>()
            .is_some()
    );
    let path = reservation.staging_directory()?.join("charged.bin");
    let mut writer = reservation.create_file(&path)?;
    let failure = anyhow::Error::from(writer.write_all(b"two bytes").unwrap_err());
    assert!(
        failure
            .chain()
            .any(|cause| cause.is::<airk_store::quota::QuotaPressure>()),
        "writer lost pressure cause: {failure:#}"
    );
    drop(writer);
    drop(reservation);
    assert_eq!(quota.status()?.active_reservations, 0);
    Ok(())
}

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
