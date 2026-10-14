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
use anyhow::Result;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

#[test]
fn exact_catalog_queries_pin_old_revision_and_verify_indexed_identity() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let (first, first_receipt, reservation) = prepare(&store, "First evidence.\n", 1, None)?;
    store.publish_catalog_v2(&first, &first_receipt, &ExpectedHead::Empty, &reservation)?;
    drop(reservation);
    let (second, second_receipt, reservation) =
        prepare(&store, "Second evidence.\n", 2, Some(first_receipt.token))?;
    store.publish_catalog_v2(
        &second,
        &second_receipt,
        &ExpectedHead::V2(first_receipt.token),
        &reservation,
    )?;
    drop(reservation);
    let old = store.catalog_at(&first_receipt.token, 1_000_000)?.unwrap();
    let current = store.catalog_at(&second_receipt.token, 1_000_000)?.unwrap();
    assert_eq!(old.snapshot.generation, 1);
    assert_eq!(
        current.publication_header(1)?.unwrap().token,
        first_receipt.token
    );
    assert_eq!(current.events_since(1)?.len(), 1);
    assert!(current.events_since(2)?.is_empty());
    assert!(current.events_since(3).is_err());
    assert_eq!(
        current.resolvers_for_path("/original", "gone.txt")?.len(),
        1
    );
    assert!(
        current
            .resolvers_for_path("/foreign", "gone.txt")?
            .is_empty()
    );
    assert_eq!(
        current
            .resolvers_for_artifact(ArtifactId::from_bytes([3; 16]))?
            .len(),
        1
    );
    let id = first.files["note.txt"].state.artifact_id;
    assert_eq!(old.file_by_artifact(id)?.unwrap(), first.files["note.txt"]);
    assert!(
        old.file_by_artifact(ArtifactId::from_bytes([8; 16]))?
            .is_none()
    );
    airk_store::retention::prune(
        store.directory(),
        &second_receipt.token.revision.to_string(),
        false,
    )?;
    assert!(store.catalog_at(&first_receipt.token, 1_000_000)?.is_some());
    let path = store
        .directory()
        .join("generations")
        .join(first_receipt.token.revision.to_string())
        .join("catalog.sqlite");
    let db = rusqlite::Connection::open(path)?;
    db.execute("DELETE FROM files", [])?;
    drop(db);
    assert!(old.file_by_artifact(id).is_err());
    assert!(store.catalog_at(&first_receipt.token, 1_000_000).is_err());
    Ok(())
}

#[test]
fn exact_catalog_rejects_forks_and_corruption_but_reports_retired_absence() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let (first, first_receipt, reservation) = prepare(&store, "First evidence.\n", 1, None)?;
    store.publish_catalog_v2(&first, &first_receipt, &ExpectedHead::Empty, &reservation)?;
    drop(reservation);
    let (second, second_receipt, reservation) =
        prepare(&store, "Second evidence.\n", 2, Some(first_receipt.token))?;
    store.publish_catalog_v2(
        &second,
        &second_receipt,
        &ExpectedHead::V2(first_receipt.token),
        &reservation,
    )?;
    drop(reservation);
    let mut wrong = first_receipt.token;
    wrong.revision = digest(b"fork");
    assert!(store.catalog_at(&wrong, 1_000_000).is_err());
    wrong = second_receipt.token;
    wrong.generation += 1;
    assert!(store.catalog_at(&wrong, 1_000_000).is_err());
    wrong = second_receipt.token;
    wrong.incarnation = Incarnation::from_bytes([9; 16]);
    assert!(store.catalog_at(&wrong, 1_000_000).is_err());
    assert!(store.catalog_at(&second_receipt.token, 1).is_err());
    let directory = store
        .directory()
        .join("generations")
        .join(first_receipt.token.revision.to_string());
    let hidden = directory.with_extension("hidden");
    fs::rename(&directory, &hidden)?;
    assert!(store.catalog_at(&first_receipt.token, 1_000_000).is_err());
    fs::rename(hidden, directory)?;
    airk_store::retention::prune(
        store.directory(),
        &second_receipt.token.revision.to_string(),
        false,
    )?;
    assert!(store.receipt(&first_receipt.token)?.is_none());
    assert!(store.catalog_at(&first_receipt.token, 1_000_000)?.is_none());
    let mut retired_fork = first_receipt.token;
    retired_fork.revision = digest(b"retired fork");
    assert!(store.catalog_at(&retired_fork, 1_000_000).is_err());
    let db = rusqlite::Connection::open(store.directory().join("index.sqlite3"))?;
    db.execute("DELETE FROM publication_receipts", [])?;
    drop(db);
    assert!(store.catalog_at(&second_receipt.token, 1_000_000).is_err());
    Ok(())
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
        version: airk_core::changes::LINEAGE_CONTRACT_VERSION,
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
        ancestry: parent
            .map(|token| airk_core::changes::RetainedPublicationHeader {
                token,
                parent: None,
                observed_unix_seconds: token.generation,
                reason: PublicationReason::Index,
            })
            .into_iter()
            .collect(),
        events: vec![airk_core::changes::ChangeEvent {
            generation,
            event_id: 0,
            subject: airk_core::changes::ChangeSubject::Artifact {
                artifact_id: ArtifactId::from_bytes([2; 16]),
            },
            before: None,
            after: None,
            before_detail: None,
            after_detail: None,
            dimensions: vec![airk_core::changes::ChangeDimension::Content],
            cause: airk_core::changes::ChangeCause::Modified,
            basis: airk_core::changes::ContinuityBasis::SamePathObservation,
        }],
        resolvers: vec![airk_core::changes::ResolverRecord {
            artifact_id: ArtifactId::from_bytes([3; 16]),
            observed_generation: generation,
            from: airk_core::changes::ChangeEndpoint {
                citation_path: Some("gone.txt".into()),
                root: "/original".into(),
                path: "gone.txt".into(),
                source_hash,
                profile_id: profile.identity()?,
            },
            to: None,
            state: airk_core::changes::ResolutionState::RemovedFromIndex,
            basis: airk_core::changes::ContinuityBasis::Unknown,
        }],
    };
    let inventory = InventoryV2 {
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
        retrieval_specification: "fixture-engine/4".into(),
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
