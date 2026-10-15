// SPDX-License-Identifier: Apache-2.0
// Original bounded history administration fixtures.
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
use anyhow::Result;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn prepare(
    store: &FileStore,
    text: &str,
    generation: u64,
    parent: Option<RevisionToken>,
) -> Result<(InventoryV2, PublicationReceipt, Reservation)> {
    prepare_at(store, text, generation, parent, generation)
}

fn prepare_at(
    store: &FileStore,
    text: &str,
    generation: u64,
    parent: Option<RevisionToken>,
    observed_unix_seconds: u64,
) -> Result<(InventoryV2, PublicationReceipt, Reservation)> {
    prepare_with_model(store, text, generation, parent, observed_unix_seconds, None)
}

fn prepare_with_model(
    store: &FileStore,
    text: &str,
    generation: u64,
    parent: Option<RevisionToken>,
    observed_unix_seconds: u64,
    model: Option<&airk_core::model_objects::ModelObjectSet>,
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
            observed_unix_seconds,
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
        embedding_pipeline_digest: model.map(|set| set.pipeline_digest),
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
        retrieval_specification: if model.is_some() {
            "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-5/inventory-2/model-objects-1"
        } else {
            "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-4/inventory-2"
        }
        .into(),
        embedding_model: model.map(|set| set.legacy_identity.clone()),
        extractor_version: "native-facts/7".into(),
        analysis: Some(airk_core::AnalysisManifest {
            version: 1,
            root: "/original".into(),
            collection: "fixture".into(),
            extractor_version: "native-facts/7".into(),
            source_hashes: BTreeMap::from([("note.txt".into(), source_hash.to_string())]),
            build_inputs: BTreeMap::new(),
            producer: "original-fixture".into(),
            producer_version: "1".into(),
            index_sha256: None,
        }),
        imported_facts: vec![airk_core::FactHit {
            continuity_key: None,
            name: "original_provider_fact".into(),
            kind: "function".into(),
            syntax_kind: "provider".into(),
            language: "text".into(),
            citation: airk_core::Citation {
                path: "note.txt".into(),
                content_hash: source_hash.to_string(),
                start_line: 1,
                end_line: 1,
                location: DocumentLocation::Lines { start: 1, end: 1 },
            },
            document: None,
            code: None,
        }],
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
    if let Some(set) = model {
        let encoded = set.canonical_bytes()?;
        reservation.charge(encoded.len() as u64)?;
        fs::write(staging.join("model-objects.json"), &encoded)?;
        components.insert(
            "model-objects.json",
            json!({"bytes":encoded.len(),"sha256":digest(&encoded)}),
        );
    }
    let manifest = serde_json::to_vec(
        &json!({"schema":if model.is_some(){3}else{2},"engine":inventory.retrieval_specification,"token":token,"collection":"fixture","chunks":1,"vectors":0,"model_sha256":inventory.embedding_model,"embedding":"none","inventory_digest":digest(&bytes),"components":components}),
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

use airk_store::checkpoints::*;
use airk_store::history_policy::*;
use airk_store::retention::MaintenanceBudget;
struct Fixture {
    _root: tempfile::TempDir,
    store: FileStore,
    quota: Quota,
    tokens: Vec<RevisionToken>,
}
impl Fixture {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        let store = FileStore::new(root.path().join("collections/fixture"));
        let writer = store.lock()?;
        let quota = Quota::open(root.path())?;
        let mut tokens = vec![];
        for generation in 1..=4 {
            let (inventory, receipt, reservation) = prepare(
                &store,
                &format!("Original historical publication {generation}.\n"),
                generation,
                tokens.last().copied(),
            )?;
            let expected = tokens
                .last()
                .map_or(ExpectedHead::Empty, |token| ExpectedHead::V2(*token));
            store.publish_catalog_v2(&inventory, &receipt, &expected, &reservation)?;
            tokens.push(receipt.token);
        }
        drop(writer);
        Ok(Self {
            _root: root,
            store,
            quota,
            tokens,
        })
    }
    fn budget() -> MaintenanceBudget {
        MaintenanceBudget {
            max_duration_ms: 60_000,
            ..Default::default()
        }
    }
}

fn budget() -> HistoryWorkBudget {
    HistoryWorkBudget {
        maintenance: Fixture::budget(),
        ..Default::default()
    }
}
#[test]
fn policy_installs_v5_without_rewriting_publication_and_reads_are_inert() -> Result<()> {
    let fixture = Fixture::new()?;
    let head = fixture.tokens[3];
    let original = fixture.store.receipt(&head)?.unwrap();
    let database = fixture.store.directory().join("index.sqlite3");
    let version = || -> Result<u32> {
        Ok(rusqlite::Connection::open(&database)?.pragma_query_value(
            None,
            "user_version",
            |row| row.get(0),
        )?)
    };
    assert_eq!(version()?, 4);
    let policy = fixture.store.history_policy()?;
    assert!(!policy.explicit && !policy.policy.enabled);
    assert!(fixture.store.checkpoints()?.is_empty());
    assert_eq!(version()?, 4);
    let writer = fixture.store.lock()?;
    let policy = fixture.store.configure_history(
        &writer,
        &fixture.quota,
        HistoryPolicyV1 {
            enabled: true,
            ..Default::default()
        },
    )?;
    assert!(policy.explicit);
    assert_eq!(version()?, 5);
    assert_eq!(fixture.store.receipt(&head)?.unwrap(), original);
    Ok(())
}
#[test]
fn checkpoint_admission_is_explicit_and_protects_history_from_retirement() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    let target = fixture.tokens[0];
    let request = CreateCheckpoint {
        label: "release α".into(),
        revision: target,
    };
    fixture
        .store
        .create_checkpoint(&writer, &fixture.quota, &request, budget())?;
    assert!(
        fixture
            .store
            .create_checkpoint(&writer, &fixture.quota, &request, budget())
            .is_err()
    );
    fixture
        .store
        .history_convert(&writer, &fixture.quota, &target, Fixture::budget())?;
    assert!(
        fixture
            .store
            .history_remove(&writer, &fixture.quota, &target, Fixture::budget())
            .is_err()
    );
    assert!(
        fixture
            .store
            .history_at(&target, 64 * 1024 * 1024)?
            .is_some()
    );
    assert!(
        fixture
            .store
            .remove_checkpoint(
                &writer,
                &fixture.quota,
                &RemoveCheckpoint {
                    label: request.label.clone(),
                    expected_revision: fixture.tokens[1]
                },
                budget()
            )
            .is_err()
    );
    fixture.store.remove_checkpoint(
        &writer,
        &fixture.quota,
        &RemoveCheckpoint {
            label: request.label,
            expected_revision: target,
        },
        budget(),
    )?;
    assert!(
        fixture
            .store
            .history_at(&target, 64 * 1024 * 1024)?
            .is_some()
    );
    Ok(())
}
#[test]
fn automatic_selection_demotes_only_old_revisions_and_keeps_exact_receipts() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    fixture.store.configure_history(
        &writer,
        &fixture.quota,
        HistoryPolicyV1 {
            enabled: true,
            ..Default::default()
        },
    )?;
    let plan = fixture
        .store
        .history_preview(&HistorySelectionRequest::default(), budget())?;
    assert!(plan.complete);
    assert_eq!(plan.convert.len(), 2);
    let run = fixture
        .store
        .maintain_history(&writer, &fixture.quota, budget())?;
    assert!(run.error.is_none(), "{run:?}");
    assert!(!run.pending, "{run:?}");
    for target in &fixture.tokens[..2] {
        assert!(
            fixture
                .store
                .history_at(target, 64 * 1024 * 1024)?
                .is_some()
        );
        assert!(fixture.store.receipt(target)?.is_some());
    }
    for target in &fixture.tokens[2..] {
        assert!(
            fixture
                .store
                .catalog_at(target, 64 * 1024 * 1024)?
                .is_some()
        );
    }
    Ok(())
}
#[test]
fn purge_protects_a_bound_package_until_conversion_finishes() -> Result<()> {
    use airk_store::history::ConversionStage;

    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    let target = fixture.tokens[0];
    let interrupted = fixture.store.history_convert_with_observer(
        &writer,
        &fixture.quota,
        &target,
        Fixture::budget(),
        &|stage, _| {
            anyhow::ensure!(
                stage != ConversionStage::BindingCommitted,
                "original interruption"
            );
            Ok(())
        },
    );
    assert!(interrupted.is_err());
    assert!(
        fixture
            .store
            .history_at(&target, 64 * 1024 * 1024)?
            .is_some()
    );
    let request = PurgeRequest {
        revisions: vec![target],
        checkpoints: vec![],
        clear_lineage: false,
    };
    let plan = fixture.store.purge_preview(&request, budget())?;
    assert!(
        plan.targets[0]
            .protected
            .contains(&HistoryProtection::Conversion)
    );
    let result = fixture.store.purge_apply(
        &writer,
        &fixture.quota,
        &PurgeApplyRequest {
            request: request.clone(),
            expected_plan_digest: plan.digest,
        },
        budget(),
    )?;
    assert!(result.error.is_none(), "{result:?}");
    assert!(result.pending && result.removals.is_empty());
    assert_eq!(result.protected, vec![target]);
    assert!(
        fixture
            .store
            .history_at(&target, 64 * 1024 * 1024)?
            .is_some()
    );
    fixture
        .store
        .history_convert(&writer, &fixture.quota, &target, Fixture::budget())?;
    let plan = fixture.store.purge_preview(&request, budget())?;
    assert!(plan.targets[0].protected.is_empty());
    let result = fixture.store.purge_apply(
        &writer,
        &fixture.quota,
        &PurgeApplyRequest {
            request,
            expected_plan_digest: plan.digest,
        },
        budget(),
    )?;
    assert!(result.error.is_none(), "{result:?}");
    assert!(!result.pending && result.removals.len() == 1);
    assert!(
        fixture
            .store
            .history_at(&target, 64 * 1024 * 1024)?
            .is_none()
    );
    assert_eq!(
        fixture.store.retained_receipts(None, 1)?.head,
        fixture.tokens[3]
    );
    Ok(())
}

#[test]
fn purge_binds_operator_state_and_never_implicitly_removes_a_checkpoint() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    let target = fixture.tokens[0];
    fixture
        .store
        .history_convert(&writer, &fixture.quota, &target, Fixture::budget())?;
    let request = PurgeRequest {
        revisions: vec![target],
        checkpoints: vec![],
        clear_lineage: false,
    };
    let stale = fixture.store.purge_preview(&request, budget())?;
    fixture.store.create_checkpoint(
        &writer,
        &fixture.quota,
        &CreateCheckpoint {
            label: "pin".into(),
            revision: target,
        },
        budget(),
    )?;
    assert!(
        fixture
            .store
            .purge_apply(
                &writer,
                &fixture.quota,
                &PurgeApplyRequest {
                    request: request.clone(),
                    expected_plan_digest: stale.digest
                },
                budget()
            )
            .is_err()
    );
    let protected = fixture.store.purge_preview(&request, budget())?;
    assert!(
        protected.targets[0]
            .protected
            .contains(&HistoryProtection::Checkpoint)
    );
    let result = fixture.store.purge_apply(
        &writer,
        &fixture.quota,
        &PurgeApplyRequest {
            request,
            expected_plan_digest: protected.digest,
        },
        budget(),
    )?;
    assert!(result.pending && result.removals.is_empty());
    assert_eq!(fixture.store.checkpoints()?.len(), 1);
    let request = PurgeRequest {
        revisions: vec![target],
        checkpoints: vec![RemoveCheckpoint {
            label: "pin".into(),
            expected_revision: target,
        }],
        clear_lineage: true,
    };
    let plan = fixture.store.purge_preview(&request, budget())?;
    let result = fixture.store.purge_apply(
        &writer,
        &fixture.quota,
        &PurgeApplyRequest {
            request,
            expected_plan_digest: plan.digest,
        },
        budget(),
    )?;
    assert!(result.error.is_none(), "{result:?}");
    assert!(!result.pending, "{result:?}");
    assert!(result.lineage_publication_required);
    assert!(fixture.store.checkpoints()?.is_empty());
    assert!(
        fixture
            .store
            .history_at(&target, 64 * 1024 * 1024)?
            .is_none()
    );
    assert_eq!(
        fixture.store.retained_receipts(None, 1)?.head,
        fixture.tokens[3]
    );
    Ok(())
}
#[test]
fn retained_receipt_pages_include_history_and_have_exact_indexed_boundaries() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    fixture.store.history_convert(
        &writer,
        &fixture.quota,
        &fixture.tokens[0],
        Fixture::budget(),
    )?;
    let first = fixture.store.retained_receipts(None, 2)?;
    assert!(!first.complete);
    assert_eq!(
        first.receipts.iter().map(|r| r.token).collect::<Vec<_>>(),
        vec![fixture.tokens[3], fixture.tokens[2]]
    );
    let second = fixture
        .store
        .retained_receipts(first.next_before_generation, 2)?;
    assert!(second.complete);
    assert_eq!(
        second.receipts.iter().map(|r| r.token).collect::<Vec<_>>(),
        vec![fixture.tokens[1], fixture.tokens[0]]
    );
    assert!(fixture.store.retained_receipts(None, 65).is_err());
    Ok(())
}

#[test]
fn reader_lease_defers_conversion_then_releases_without_revocation() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    fixture.store.configure_history(
        &writer,
        &fixture.quota,
        HistoryPolicyV1 {
            enabled: true,
            ..Default::default()
        },
    )?;
    let pinned = fixture
        .store
        .catalog_at(&fixture.tokens[0], 64 * 1024 * 1024)?
        .unwrap();
    let first = fixture
        .store
        .maintain_history(&writer, &fixture.quota, budget())?;
    assert!(first.error.is_none(), "{first:?}");
    assert!(
        fixture
            .store
            .directory()
            .join("generations")
            .join(fixture.tokens[0].revision.to_string())
            .exists()
    );
    drop(pinned);
    let second = fixture
        .store
        .maintain_history(&writer, &fixture.quota, budget())?;
    assert!(second.error.is_none(), "{second:?}");
    assert!(
        fixture
            .store
            .history_at(&fixture.tokens[0], 64 * 1024 * 1024)?
            .is_some()
    );
    Ok(())
}
#[test]
fn lowered_policy_reports_protected_overage_without_revoking_pin() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    let target = fixture.tokens[0];
    fixture.store.create_checkpoint(
        &writer,
        &fixture.quota,
        &CreateCheckpoint {
            label: "durable".into(),
            revision: target,
        },
        budget(),
    )?;
    fixture.store.configure_history(
        &writer,
        &fixture.quota,
        HistoryPolicyV1 {
            enabled: true,
            max_bytes: 1,
            ..Default::default()
        },
    )?;
    let plan = fixture
        .store
        .history_preview(&HistorySelectionRequest::default(), budget())?;
    assert!(plan.complete && plan.over_target_protected);
    assert!(plan.retained.iter().any(|entry| entry.revision == target));
    assert_eq!(fixture.store.checkpoints()?.len(), 1);
    assert!(
        fixture
            .store
            .create_checkpoint(
                &writer,
                &fixture.quota,
                &CreateCheckpoint {
                    label: "rejected".into(),
                    revision: fixture.tokens[1]
                },
                budget()
            )
            .is_err()
    );
    assert_eq!(fixture.store.checkpoints()?.len(), 1);
    Ok(())
}
#[test]
fn purge_partial_removal_is_durably_recovered_without_changing_head() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    let target = fixture.tokens[0];
    fixture
        .store
        .history_convert(&writer, &fixture.quota, &target, Fixture::budget())?;
    let request = PurgeRequest {
        revisions: vec![target],
        checkpoints: vec![],
        clear_lineage: false,
    };
    let plan = fixture.store.purge_preview(&request, budget())?;
    let mut tiny = budget();
    tiny.maintenance.max_removal_bytes = 1;
    let result = fixture.store.purge_apply(
        &writer,
        &fixture.quota,
        &PurgeApplyRequest {
            request,
            expected_plan_digest: plan.digest,
        },
        tiny,
    )?;
    assert!(result.pending, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    let recovered = fixture
        .store
        .maintain_history(&writer, &fixture.quota, budget())?;
    assert!(recovered.error.is_none(), "{recovered:?}");
    assert!(
        fixture
            .store
            .history_at(&target, 64 * 1024 * 1024)?
            .is_none()
    );
    assert_eq!(
        fixture.store.retained_receipts(None, 1)?.head,
        fixture.tokens[3]
    );
    Ok(())
}

#[test]
fn manual_backlog_larger_than_one_page_advances_without_unprotected_deletion() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    for generation in 5..=260 {
        let parent = *fixture.tokens.last().unwrap();
        let (inventory, receipt, reservation) = prepare(
            &fixture.store,
            &format!("Bounded historical scan publication {generation}.\n"),
            generation,
            Some(parent),
        )?;
        fixture.store.publish_catalog_v2(
            &inventory,
            &receipt,
            &ExpectedHead::V2(parent),
            &reservation,
        )?;
        fixture.tokens.push(receipt.token);
    }
    fixture.store.configure_history(
        &writer,
        &fixture.quota,
        HistoryPolicyV1 {
            enabled: true,
            ..Default::default()
        },
    )?;
    let first = fixture
        .store
        .maintain_history(&writer, &fixture.quota, budget())?;
    assert!(first.error.is_none() && first.pending, "{first:?}");
    assert!(first.conversions.is_empty());
    assert_eq!(
        fs::read_dir(fixture.store.directory().join("generations"))?.count(),
        260
    );
    let mut completed = false;
    for _ in 0..12 {
        let run = fixture
            .store
            .maintain_history(&writer, &fixture.quota, budget())?;
        assert!(run.error.is_none(), "{run:?}");
        if !run.pending {
            completed = true;
            break;
        }
    }
    assert!(
        completed,
        "selection must advance beyond the first 256 receipts"
    );
    for target in &fixture.tokens[255..258] {
        assert!(
            fixture
                .store
                .history_at(target, 64 * 1024 * 1024)?
                .is_some()
        );
    }
    assert!(
        fixture
            .store
            .catalog_at(&fixture.tokens[0], 64 * 1024 * 1024)?
            .is_some(),
        "selection alone does not prune unselected full generations"
    );
    Ok(())
}

#[test]
fn hourly_winners_are_deduplicated_and_oldest_unpinned_targets_thin_first() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let writer = store.lock()?;
    let quota = Quota::open(root.path())?;
    let mut tokens = vec![];
    for generation in 1..=8 {
        let parent = tokens.last().copied();
        let (inventory, receipt, reservation) = prepare_at(
            &store,
            &format!("Hourly historical observation {generation}.\n"),
            generation,
            parent,
            3600 * (10 + generation / 2),
        )?;
        store.publish_catalog_v2(
            &inventory,
            &receipt,
            &parent.map_or(ExpectedHead::Empty, ExpectedHead::V2),
            &reservation,
        )?;
        tokens.push(receipt.token);
    }
    store.configure_history(
        &writer,
        &quota,
        HistoryPolicyV1 {
            enabled: true,
            recent: 1,
            hourly_hours: 24,
            max_packages: 2,
            ..Default::default()
        },
    )?;
    let plan = store.history_preview(
        &HistorySelectionRequest {
            as_of_unix_seconds: Some(3600 * 15),
        },
        budget(),
    )?;
    assert!(plan.complete);
    assert_eq!(
        plan.convert,
        vec![tokens[5], tokens[4]],
        "newest revision per hour, then oldest first thinning"
    );
    assert!(!plan.over_target_protected);
    Ok(())
}
#[test]
fn checkpoint_limit_and_utf8_label_bound_do_not_install_failed_mutations() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    for (index, token) in fixture.tokens.iter().enumerate() {
        fixture.store.create_checkpoint(
            &writer,
            &fixture.quota,
            &CreateCheckpoint {
                label: format!("pin{index}"),
                revision: *token,
            },
            budget(),
        )?;
    }
    assert!(
        fixture
            .store
            .create_checkpoint(
                &writer,
                &fixture.quota,
                &CreateCheckpoint {
                    label: "fifth".into(),
                    revision: fixture.tokens[0]
                },
                budget()
            )
            .is_err()
    );
    assert_eq!(fixture.store.checkpoints()?.len(), 4);
    assert!(validate_label(&"é".repeat(64)).is_ok());
    assert!(validate_label(&"é".repeat(65)).is_err());
    assert!(validate_label("line\nbreak").is_err());
    Ok(())
}

#[test]
fn shared_model_assets_follow_full_reader_roots_but_not_historical_packages() -> Result<()> {
    use airk_core::model_objects::{ModelAssetName, ModelObjectRef, ModelObjectSet};
    use airk_store::history::ConversionStatus;
    let root = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let writer = store.lock()?;
    let quota = Quota::open(root.path())?;
    let provision = |name: &str| -> Result<ModelObjectSet> {
        let source = root.path().join(name);
        fs::create_dir(&source)?;
        let config = format!("{{\"fixture_model\":\"{name}\"}}");
        let vocabulary = b"{\"shared\":[0.25,0.75]}";
        fs::write(source.join("model.json"), config.as_bytes())?;
        fs::write(source.join("vocabulary.json"), vocabulary)?;
        let set = ModelObjectSet {
            version: 1,
            pipeline_digest: digest(name.as_bytes()),
            legacy_identity: digest(config.as_bytes()).to_string(),
            assets: BTreeMap::from([
                (
                    ModelAssetName::ModelJson,
                    ModelObjectRef {
                        sha256: digest(config.as_bytes()),
                        bytes: config.len() as u64,
                    },
                ),
                (
                    ModelAssetName::VocabularyJson,
                    ModelObjectRef {
                        sha256: digest(vocabulary),
                        bytes: vocabulary.len() as u64,
                    },
                ),
            ]),
        };
        let reservation = quota.reserve("fixture", 1024 * 1024)?;
        airk_store::model_objects::ModelObjects::new(store.directory()).retain(
            &source,
            &set,
            &reservation,
        )?;
        Ok(set)
    };
    let first = provision("first")?;
    let second = provision("second")?;
    let mut tokens = vec![];
    let mut receipts = vec![];
    for generation in 1..=4 {
        let model = match generation {
            1 => Some(&first),
            2 | 3 => Some(&second),
            _ => None,
        };
        let parent = tokens.last().copied();
        let (inventory, receipt, reservation) = prepare_with_model(
            &store,
            &format!("Shared immutable model publication {generation}.\n"),
            generation,
            parent,
            generation,
            model,
        )?;
        store.publish_catalog_v2(
            &inventory,
            &receipt,
            &parent.map_or(ExpectedHead::Empty, ExpectedHead::V2),
            &reservation,
        )?;
        tokens.push(receipt.token);
        receipts.push(receipt);
    }
    assert_eq!(
        fs::read_dir(store.directory().join("objects/models"))?.count(),
        3
    );
    assert_eq!(store.model_object_references(&receipts[0])?.len(), 2);
    let pin = store.catalog_at(&tokens[0], 64 * 1024 * 1024)?.unwrap();
    assert_eq!(
        store
            .history_convert(&writer, &quota, &tokens[0], Fixture::budget())?
            .status,
        ConversionStatus::Protected
    );
    let run = store.maintain_retention(
        &writer,
        &quota,
        airk_store::retention::MaintenanceTrigger::ExplicitMaintenance,
        Fixture::budget(),
    );
    assert!(run.receipt.error.is_none(), "{run:?}");
    assert!(
        store
            .directory()
            .join("objects/models")
            .join(first.assets[&ModelAssetName::ModelJson].sha256.to_string())
            .exists()
    );
    drop(pin);
    assert_eq!(
        store
            .history_convert(&writer, &quota, &tokens[0], Fixture::budget())?
            .status,
        ConversionStatus::Complete
    );
    let run = store.maintain_retention(
        &writer,
        &quota,
        airk_store::retention::MaintenanceTrigger::ExplicitMaintenance,
        Fixture::budget(),
    );
    assert!(run.receipt.error.is_none(), "{run:?}");
    assert!(
        !store
            .directory()
            .join("objects/models")
            .join(first.assets[&ModelAssetName::ModelJson].sha256.to_string())
            .exists()
    );
    assert_eq!(
        fs::read_dir(store.directory().join("objects/models"))?.count(),
        2
    );
    assert!(store.history_at(&tokens[0], 64 * 1024 * 1024)?.is_some());
    let asset = store.directory().join("objects/models").join(
        second.assets[&ModelAssetName::VocabularyJson]
            .sha256
            .to_string(),
    );
    let mut bytes = fs::read(&asset)?;
    bytes[0] ^= 1;
    fs::write(asset, bytes)?;
    assert!(store.model_object_references(&receipts[2]).is_err());
    Ok(())
}

#[test]
fn enabling_history_rejects_pending_ordinary_retirement_until_explicit_recovery() -> Result<()> {
    use airk_store::retention::{MaintenanceStage, MaintenanceTrigger};
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    let run = fixture.store.maintain_retention_with_observer(
        &writer,
        &fixture.quota,
        MaintenanceTrigger::ExplicitMaintenance,
        Fixture::budget(),
        &|stage, _| {
            if stage == MaintenanceStage::Detached {
                anyhow::bail!("original interrupted retirement");
            }
            Ok(())
        },
    );
    assert!(run.receipt.pending && run.receipt.error.is_some());
    let error = fixture
        .store
        .configure_history(
            &writer,
            &fixture.quota,
            HistoryPolicyV1 {
                enabled: true,
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("finish ordinary retention maintenance")
    );
    assert!(!fixture.store.history_policy()?.explicit);
    let run = fixture.store.maintain_retention(
        &writer,
        &fixture.quota,
        MaintenanceTrigger::ExplicitMaintenance,
        Fixture::budget(),
    );
    assert!(run.receipt.error.is_none(), "{run:?}");
    fixture.store.configure_history(
        &writer,
        &fixture.quota,
        HistoryPolicyV1 {
            enabled: true,
            ..Default::default()
        },
    )?;
    Ok(())
}
