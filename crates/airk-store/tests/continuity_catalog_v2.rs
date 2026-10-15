// SPDX-License-Identifier: Apache-2.0
//! Original immutable continuity index and native fact-key fixtures.
use airk_core::DocumentLocation;
use airk_core::changes::{
    ChangeCoverage, CoverageFloor, CoverageReason, FileStamps, FileState, LineageEnvelope,
    PublicationObservation, PublicationReason,
};
use airk_core::continuity::{
    ContinuityDescriptor, ContinuityFamily, ContinuityKey, ContinuityRecord, ContinuitySubject,
};
use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan};
use airk_core::identity::DocumentKey;
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

fn key(ordinal: usize) -> ContinuityKey {
    ContinuityKey::Document(DocumentKey::from_bytes((ordinal as u128 + 1).to_be_bytes()))
}

fn record(ordinal: usize, profile: Sha256Digest, length: usize) -> ContinuityRecord {
    ContinuityRecord {
        ambiguous_predecessor: false,
        key: key(ordinal),
        first_generation: 1,
        changed: BTreeMap::new(),
        descriptor: ContinuityDescriptor {
            algorithm_version: 1,
            subject: ContinuitySubject::Fact(ordinal as u32),
            owner: None,
            region: None,
            family: ContinuityFamily::Documents,
            category: "heading".into(),
            name: Some(digest(b"Original heading")),
            explicit_identity: None,
            profile,
            provenance: digest(b"original fixture"),
            dimensions: BTreeMap::new(),
            unsupported_dimensions: vec![],
            span: LocalSpan {
                start: 0,
                end: length,
            },
            location: DocumentLocation::Lines { start: 1, end: 1 },
            complete: true,
            body_informative: false,
        },
    }
}

#[test]
fn indexed_lookup_projects_large_file_records_and_decorates_only_native_facts() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let (inventory, receipt, reservation) =
        prepare(&store, "Original heading.\n", 1, None, 1000, Some(1))?;
    assert!(serde_json::to_vec(&inventory.files["note.txt"])?.len() > 131_072);
    store.publish_catalog_v2(&inventory, &receipt, &ExpectedHead::Empty, &reservation)?;
    drop(reservation);
    let catalog = store.catalog()?.unwrap();
    let (path, actual) = catalog.continuity_by_key(key(999))?.unwrap();
    assert_eq!(path, "note.txt");
    assert_eq!(actual, inventory.files["note.txt"].continuity[999]);
    assert!(catalog.continuity_by_key(key(1001))?.is_none());
    assert_eq!(
        catalog.file("note.txt")?.unwrap(),
        inventory.files["note.txt"]
    );
    assert_eq!(
        catalog
            .file_by_artifact(ArtifactId::from_bytes([2; 16]))?
            .unwrap(),
        inventory.files["note.txt"]
    );
    let connection = rusqlite::Connection::open_with_flags(
        store
            .directory()
            .join("generations")
            .join(receipt.token.revision.to_string())
            .join("catalog.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let plan: String = connection.query_row(
        "EXPLAIN QUERY PLAN SELECT path,record FROM continuity_records WHERE key=?1",
        [serde_json::to_string(&key(999))?],
        |row| row.get(3),
    )?;
    assert!(plan.contains("SEARCH") && plan.contains("INDEX"), "{plan}");
    let native: String = connection.query_row(
        "SELECT payload FROM facts WHERE name='Original heading'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        serde_json::from_str::<airk_core::FactHit>(&native)?.continuity_key,
        Some(key(0))
    );
    let imported: String = connection.query_row(
        "SELECT payload FROM facts WHERE name='Original imported claim'",
        [],
        |row| row.get(0),
    )?;
    assert!(
        serde_json::from_str::<airk_core::FactHit>(&imported)?
            .continuity_key
            .is_none()
    );
    drop(connection);
    let connection = rusqlite::Connection::open(
        store
            .directory()
            .join("generations")
            .join(receipt.token.revision.to_string())
            .join("catalog.sqlite"),
    )?;
    connection.execute(
        "DELETE FROM continuity_records WHERE key=?1",
        [serde_json::to_string(&key(999))?],
    )?;
    drop(connection);
    assert!(catalog.continuity_by_key(key(999)).is_err());
    assert!(catalog.continuity_by_key(key(1001))?.is_none());
    Ok(())
}

#[test]
fn older_catalog_without_extension_remains_readable() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let (inventory, receipt, reservation) =
        prepare(&store, "Original legacy heading.\n", 1, None, 0, None)?;
    store.publish_catalog_v2(&inventory, &receipt, &ExpectedHead::Empty, &reservation)?;
    let catalog = store.catalog()?.unwrap();
    assert!(catalog.continuity_by_key(key(0))?.is_none());
    assert_eq!(
        catalog.file("note.txt")?.unwrap(),
        inventory.files["note.txt"]
    );
    Ok(())
}

#[test]
fn future_continuity_extension_is_rejected_before_publication() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = FileStore::new(root.path().join("collections/fixture"));
    let _writer = store.lock()?;
    let (inventory, receipt, reservation) =
        prepare(&store, "Original future heading.\n", 1, None, 1, Some(2))?;
    let error = store
        .publish_catalog_v2(&inventory, &receipt, &ExpectedHead::Empty, &reservation)
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("unsupported continuity catalog version"),
        "{error:#}"
    );
    assert!(store.persistent_revision()?.is_none());
    Ok(())
}

fn prepare(
    store: &FileStore,
    text: &str,
    generation: u64,
    parent: Option<RevisionToken>,
    count: usize,
    extension_version: Option<u32>,
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
        facts: vec![airk_core::extraction::LocalFact {
            id: 0,
            owner: None,
            kind: "heading".into(),
            name: "Original heading".into(),
            span: LocalSpan {
                start: 0,
                end: text.len(),
            },
            syntax_kind: "heading".into(),
            start_line: 1,
            end_line: 1,
            location: DocumentLocation::Lines { start: 1, end: 1 },
            document: None,
            code: None,
            region: None,
            attributes: BTreeMap::new(),
        }],
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
        extraction_policy_digest: None,
        version: if count > 0 { 3 } else { 2 },
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
                continuity: (0..count)
                    .map(|ordinal| record(ordinal, profile.identity().unwrap(), text.len()))
                    .collect(),
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
                facts: 1,
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
            continuity_key: Some(key(0)),
            name: "Original imported claim".into(),
            kind: "provider_assertion".into(),
            syntax_kind: "provider".into(),
            language: "text".into(),
            code: None,
            document: None,
            citation: airk_core::Citation {
                path: "note.txt".into(),
                content_hash: source_hash.to_string(),
                start_line: 1,
                end_line: 1,
                location: DocumentLocation::Lines { start: 1, end: 1 },
            },
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
        8 * 1024 * 1024,
    )?;
    {
        let connection = rusqlite::Connection::open(staging.join("catalog.sqlite"))?;
        match extension_version {
            None => connection
                .execute_batch("DROP TABLE continuity_records; DROP TABLE continuity_schema;")?,
            Some(version) => {
                connection.execute("UPDATE continuity_schema SET version=?1", [version])?;
            }
        }
    }
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
