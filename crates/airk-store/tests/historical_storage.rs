// SPDX-License-Identifier: Apache-2.0
//! Original history-contract and eventual durable transition qualification.
use airk_core::identity::{Incarnation, PublicationReceipt, RevisionToken, Sha256Digest};
use airk_store::history::*;
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([value; 32])
}
fn token(generation: u64) -> RevisionToken {
    RevisionToken {
        incarnation: Incarnation::from_bytes([1; 16]),
        generation,
        revision: digest(generation as u8),
    }
}
fn descriptor() -> HistoryDescriptor {
    HistoryDescriptor {
        version: HISTORY_DESCRIPTOR_VERSION,
        publication: PublicationReceipt {
            token: token(1),
            inventory_digest: digest(2),
            manifest_digest: digest(3),
        },
        observed_unix_seconds: 123,
        capabilities: BTreeSet::from([
            HistoryCapability::File,
            HistoryCapability::Context,
            HistoryCapability::Outline,
            HistoryCapability::Info,
        ]),
        metadata: BTreeMap::from([
            (
                HistoryMetadata::Inventory,
                StoredDigest {
                    sha256: digest(2),
                    bytes: 12,
                },
            ),
            (
                HistoryMetadata::Lineage,
                StoredDigest {
                    sha256: digest(4),
                    bytes: 13,
                },
            ),
            (
                HistoryMetadata::Manifest,
                StoredDigest {
                    sha256: digest(3),
                    bytes: 14,
                },
            ),
        ]),
        dependencies: vec![
            ObjectDependency {
                kind: DependencyKind::Source,
                digest: digest(5),
                bytes: 0,
            },
            ObjectDependency {
                kind: DependencyKind::Payload,
                digest: digest(6),
                bytes: 42,
            },
        ],
    }
}

#[test]
fn history_contract_preserves_exact_receipt_and_excludes_search_dependencies() -> Result<()> {
    let original = descriptor();
    let bytes = original.canonical_bytes()?;
    assert_eq!(HistoryDescriptor::from_bytes(&bytes)?, original);
    assert_eq!(original.dependency_bytes()?, 42);
    let mut whitespace = bytes.clone();
    whitespace.push(b'\n');
    assert!(HistoryDescriptor::from_bytes(&whitespace).is_err());
    for kind in [DependencyKind::Embedding, DependencyKind::Model] {
        let mut changed = original.clone();
        changed.dependencies.push(ObjectDependency {
            kind,
            digest: digest(7),
            bytes: 10,
        });
        assert!(changed.validate().is_err());
    }
    let mut changed = original.clone();
    changed
        .metadata
        .get_mut(&HistoryMetadata::Inventory)
        .unwrap()
        .sha256 = digest(9);
    assert!(changed.validate().is_err());
    let mut changed = original.clone();
    changed.dependencies.push(changed.dependencies[1].clone());
    assert!(changed.validate().is_err());
    let mut changed = original;
    changed.dependencies.reverse();
    assert!(changed.validate().is_err());
    Ok(())
}

#[test]
fn conversion_intent_identity_binds_receipt_head_and_descriptor_without_stage_cycle() -> Result<()>
{
    let descriptor = descriptor();
    let mut intent = ConversionIntent::new(
        descriptor.publication.clone(),
        token(2),
        descriptor.binding_digest()?,
    )?;
    let id = intent.id;
    for stage in [
        ConversionStage::IntentDurable,
        ConversionStage::PackageVerified,
        ConversionStage::PackagePublished,
        ConversionStage::BindingCommitted,
        ConversionStage::FullDetached,
        ConversionStage::FullRemoved,
        ConversionStage::IntentComplete,
    ] {
        intent.stage = stage;
        intent.validate()?;
        assert_eq!(intent.id, id);
    }
    intent.descriptor.sha256 = digest(11);
    assert!(intent.validate().is_err());
    assert!(
        ConversionIntent::new(
            descriptor.publication.clone(),
            token(1),
            descriptor.binding_digest()?
        )
        .is_err()
    );
    let mut foreign = token(2);
    foreign.incarnation = Incarnation::from_bytes([9; 16]);
    assert!(
        ConversionIntent::new(
            descriptor.publication.clone(),
            foreign,
            descriptor.binding_digest()?
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn future_malformed_and_oversized_contracts_fail_closed() -> Result<()> {
    let mut future = descriptor();
    future.version += 1;
    assert!(future.validate().is_err());
    let mut future = descriptor();
    future
        .metadata
        .get_mut(&HistoryMetadata::Manifest)
        .unwrap()
        .bytes = 131_073;
    assert!(future.validate().is_err());
    let mut overflow = descriptor();
    overflow.dependencies[0].bytes = u64::MAX;
    assert!(overflow.validate().is_err());
    let mut unknown = serde_json::to_value(descriptor())?;
    unknown["undeclared_capability"] = true.into();
    assert!(HistoryDescriptor::from_bytes(&serde_json::to_vec(&unknown)?).is_err());
    let mut empty_payload = descriptor();
    empty_payload.dependencies[1].bytes = 0;
    assert!(empty_payload.validate().is_err());
    Ok(())
}

mod original {
    use airk_core::changes::{
        ChangeCoverage, CoverageFloor, CoverageReason, FileStamps, FileState, LineageEnvelope,
        PublicationObservation, PublicationReason,
    };
    use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan};
    use airk_core::identity::{
        ArtifactId, Incarnation, PublicationReceipt, RevisionToken, Sha256Digest, revision_digest,
    };
    use airk_core::publication::{InventoryFile, InventoryV2};
    use airk_core::{ArtifactSource, DocumentLocation};
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
        let incarnation =
            parent.map_or(Incarnation::from_bytes([1; 16]), |token| token.incarnation);
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

    use airk_store::history::{ConversionStage, ConversionStatus, HistoryRemovalStage};
    use airk_store::retention::{MaintenanceBudget, MaintenanceTrigger};

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

    #[test]
    fn original_metadata_and_source_survive_conversion_and_gc_with_same_receipt() -> Result<()> {
        let fixture = Fixture::new()?;
        let writer = fixture.store.lock()?;
        let token = fixture.tokens[0];
        let original_receipt = fixture.store.receipt(&token)?.unwrap();
        let full = fixture
            .store
            .directory()
            .join("generations")
            .join(token.revision.to_string());
        let bytes = ["inventory.json", "lineage.json", "manifest.json"]
            .map(|name| (name, fs::read(full.join(name)).unwrap()));
        let converted =
            fixture
                .store
                .history_convert(&writer, &fixture.quota, &token, Fixture::budget())?;
        assert_eq!(converted.status, ConversionStatus::Complete);
        assert!(!full.exists());
        assert_eq!(fixture.store.receipt(&token)?.unwrap(), original_receipt);
        assert!(
            fixture
                .store
                .catalog_at(&token, 64 * 1024 * 1024)?
                .is_none()
        );
        for (name, original) in bytes {
            assert_eq!(
                fs::read(
                    fixture
                        .store
                        .directory()
                        .join("history")
                        .join(token.revision.to_string())
                        .join(name)
                )?,
                original
            );
        }
        let catalog = fixture.store.history_at(&token, 64 * 1024 * 1024)?.unwrap();
        assert_eq!(
            catalog.source("note.txt")?,
            b"Original historical publication 1.\n"
        );
        assert!(!catalog.snapshot.persistent_indexes);
        assert_eq!(
            catalog.inventory.imported_facts[0].name,
            "original_provider_fact"
        );
        assert_eq!(
            catalog.snapshot.imported_facts[0].name,
            "original_provider_fact"
        );
        assert_eq!(catalog.load_artifact("note.txt")?.path, "note.txt");
        let run = fixture.store.maintain_retention(
            &writer,
            &fixture.quota,
            MaintenanceTrigger::ExplicitMaintenance,
            Fixture::budget(),
        );
        assert!(run.receipt.error.is_none(), "{:?}", run.receipt.error);
        assert_eq!(
            catalog.source("note.txt")?,
            b"Original historical publication 1.\n"
        );
        assert_eq!(
            fixture
                .store
                .history_remove(&writer, &fixture.quota, &token, Fixture::budget())?
                .status,
            ConversionStatus::Protected
        );
        drop(catalog);
        assert_eq!(
            fixture
                .store
                .history_remove(&writer, &fixture.quota, &token, Fixture::budget())?
                .status,
            ConversionStatus::Complete
        );
        assert!(fixture.store.receipt(&token)?.is_none());
        assert!(
            fixture
                .store
                .history_at(&token, 64 * 1024 * 1024)?
                .is_none()
        );
        assert!(
            fixture
                .store
                .catalog_at(&token, 64 * 1024 * 1024)?
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn reader_and_searchable_keep_set_protect_conversion() -> Result<()> {
        let fixture = Fixture::new()?;
        let writer = fixture.store.lock()?;
        let pinned = fixture
            .store
            .catalog_at(&fixture.tokens[0], 64 * 1024 * 1024)?
            .unwrap();
        assert_eq!(
            fixture
                .store
                .history_convert(
                    &writer,
                    &fixture.quota,
                    &fixture.tokens[0],
                    Fixture::budget()
                )?
                .status,
            ConversionStatus::Protected
        );
        for token in &fixture.tokens[2..] {
            assert!(
                fixture
                    .store
                    .history_convert(&writer, &fixture.quota, token, Fixture::budget())
                    .is_err()
            );
        }
        drop(pinned);
        assert_eq!(
            fixture
                .store
                .history_convert(
                    &writer,
                    &fixture.quota,
                    &fixture.tokens[0],
                    Fixture::budget()
                )?
                .status,
            ConversionStatus::Complete
        );
        Ok(())
    }

    #[test]
    fn every_conversion_boundary_recovers_without_changing_identity() -> Result<()> {
        for stage in [
            ConversionStage::IntentDurable,
            ConversionStage::PackageVerified,
            ConversionStage::PackagePublished,
            ConversionStage::BindingCommitted,
            ConversionStage::FullDetached,
            ConversionStage::FullRemoved,
            ConversionStage::IntentComplete,
        ] {
            let fixture = Fixture::new()?;
            let writer = fixture.store.lock()?;
            let token = fixture.tokens[0];
            let receipt = fixture.store.receipt(&token)?;
            let failed = fixture.store.history_convert_with_observer(
                &writer,
                &fixture.quota,
                &token,
                Fixture::budget(),
                &|observed, _| {
                    anyhow::ensure!(observed != stage, "injected conversion boundary");
                    Ok(())
                },
            );
            assert!(failed.is_err(), "{stage:?}");
            let reopened = FileStore::new(fixture.store.directory());
            let recovered = reopened.history_recover(&writer, &fixture.quota, Fixture::budget())?;
            assert!(!recovered.pending, "{stage:?}");
            assert_eq!(reopened.receipt(&token)?, receipt);
            assert!(reopened.history_at(&token, 64 * 1024 * 1024)?.is_some());
            assert!(reopened.catalog_at(&token, 64 * 1024 * 1024)?.is_none());
            assert!(
                reopened
                    .history_recover(&writer, &fixture.quota, Fixture::budget())?
                    .conversions
                    .is_empty()
            );
        }
        Ok(())
    }

    #[test]
    fn every_history_removal_boundary_recovers_and_retains_receipt_until_bytes_gone() -> Result<()>
    {
        for stage in [
            HistoryRemovalStage::IntentDurable,
            HistoryRemovalStage::PackageDetached,
            HistoryRemovalStage::PhysicalRemoved,
            HistoryRemovalStage::ReceiptRetired,
            HistoryRemovalStage::IntentComplete,
        ] {
            let fixture = Fixture::new()?;
            let writer = fixture.store.lock()?;
            let token = fixture.tokens[0];
            fixture
                .store
                .history_convert(&writer, &fixture.quota, &token, Fixture::budget())?;
            let failed = fixture.store.history_remove_with_observer(
                &writer,
                &fixture.quota,
                &token,
                Fixture::budget(),
                &|observed, _| {
                    anyhow::ensure!(observed != stage, "injected historical removal boundary");
                    Ok(())
                },
            );
            assert!(failed.is_err(), "{stage:?}");
            if matches!(
                stage,
                HistoryRemovalStage::IntentDurable
                    | HistoryRemovalStage::PackageDetached
                    | HistoryRemovalStage::PhysicalRemoved
            ) {
                assert!(fixture.store.receipt(&token)?.is_some());
            } else {
                assert!(fixture.store.receipt(&token)?.is_none());
            }
            if stage == HistoryRemovalStage::IntentDurable {
                let witness = fixture
                    .store
                    .directory()
                    .join("maintenance/history-descriptors")
                    .join(format!("{}.json", token.revision));
                fs::create_dir_all(witness.parent().unwrap())?;
                fs::write(witness, b"interrupted private witness write")?;
            }
            if stage != HistoryRemovalStage::IntentDurable {
                assert!(
                    fixture
                        .store
                        .history_at(&token, 64 * 1024 * 1024)?
                        .is_none()
                );
            }
            let recovered =
                fixture
                    .store
                    .history_recover(&writer, &fixture.quota, Fixture::budget())?;
            assert!(!recovered.pending, "{stage:?}");
            assert!(fixture.store.receipt(&token)?.is_none());
            assert!(
                fixture
                    .store
                    .history_at(&token, 64 * 1024 * 1024)?
                    .is_none()
            );
        }
        Ok(())
    }

    #[test]
    fn v3_reads_do_not_migrate_and_conversion_preserves_original_receipt_bytes() -> Result<()> {
        let fixture = Fixture::new()?;
        let writer = fixture.store.lock()?;
        let token = fixture.tokens[0];
        let database = fixture.store.directory().join("index.sqlite3");
        let conn = rusqlite::Connection::open(&database)?;
        conn.execute_batch("DROP TABLE history_bindings; PRAGMA user_version=3;")?;
        let original: String = conn.query_row(
            "SELECT payload FROM publication_receipts WHERE revision=?1",
            [token.revision.to_string()],
            |row| row.get(0),
        )?;
        drop(conn);
        assert!(
            fixture
                .store
                .catalog_at(&token, 64 * 1024 * 1024)?
                .is_some()
        );
        assert!(
            fixture
                .store
                .history_at(&token, 64 * 1024 * 1024)?
                .is_none()
        );
        let conn = rusqlite::Connection::open(&database)?;
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))?,
            3
        );
        drop(conn);
        fixture
            .store
            .history_convert(&writer, &fixture.quota, &token, Fixture::budget())?;
        let conn = rusqlite::Connection::open(&database)?;
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))?,
            4
        );
        let after: String = conn.query_row(
            "SELECT payload FROM publication_receipts WHERE revision=?1",
            [token.revision.to_string()],
            |row| row.get(0),
        )?;
        assert_eq!(after, original);
        Ok(())
    }

    #[test]
    fn corrupt_payload_and_unknown_package_members_are_never_expired_history() -> Result<()> {
        for corrupt_payload in [true, false] {
            let fixture = Fixture::new()?;
            let writer = fixture.store.lock()?;
            let token = fixture.tokens[0];
            fixture
                .store
                .history_convert(&writer, &fixture.quota, &token, Fixture::budget())?;
            let catalog = fixture.store.history_at(&token, 64 * 1024 * 1024)?.unwrap();
            let digest = catalog.inventory.files["note.txt"].state.payload_digest;
            drop(catalog);
            if corrupt_payload {
                fs::write(
                    fixture
                        .store
                        .directory()
                        .join("objects/artifacts")
                        .join(digest.to_string()),
                    b"corrupt",
                )?;
            } else {
                fs::create_dir(
                    fixture
                        .store
                        .directory()
                        .join("history")
                        .join(token.revision.to_string())
                        .join("unknown"),
                )?;
            }
            assert!(fixture.store.history_at(&token, 64 * 1024 * 1024).is_err());
            assert!(fixture.store.catalog_at(&token, 64 * 1024 * 1024).is_err());
            assert!(
                fixture
                    .store
                    .history_remove(&writer, &fixture.quota, &token, Fixture::budget())
                    .is_err()
            );
            assert!(fixture.store.receipt(&token)?.is_some());
        }
        Ok(())
    }

    #[test]
    fn historical_readers_protect_relocation_and_removal_and_moved_history_reopens() -> Result<()> {
        let fixture = Fixture::new()?;
        let writer = fixture.store.lock()?;
        let token = fixture.tokens[0];
        fixture
            .store
            .history_convert(&writer, &fixture.quota, &token, Fixture::budget())?;
        drop(writer);
        let pinned = fixture.store.history_at(&token, 64 * 1024 * 1024)?.unwrap();
        let original = pinned.descriptor.publication.clone();
        let target = tempfile::tempdir()?;
        let destination = target.path().join("relocated");
        assert!(airk_store::relocation::relocate(fixture._root.path(), &destination).is_err());
        assert!(airk_store::remove_collection(fixture._root.path(), "fixture").is_err());
        assert_eq!(
            pinned.source("note.txt")?,
            b"Original historical publication 1.\n"
        );
        drop(pinned);
        airk_store::relocation::relocate(fixture._root.path(), &destination)?;
        let moved = FileStore::new(destination.join("collections/fixture"));
        assert_eq!(moved.receipt(&token)?, Some(original));
        assert!(moved.catalog_at(&token, 64 * 1024 * 1024)?.is_none());
        let pinned = moved.history_at(&token, 64 * 1024 * 1024)?.unwrap();
        assert_eq!(
            pinned.source("note.txt")?,
            b"Original historical publication 1.\n"
        );
        assert!(airk_store::remove_collection(&destination, "fixture").is_err());
        drop(pinned);
        airk_store::remove_collection(&destination, "fixture")?;
        assert!(!moved.directory().exists());
        Ok(())
    }

    #[test]
    fn quota_and_work_admission_failure_leave_full_revision_and_receipt_readable() -> Result<()> {
        let fixture = Fixture::new()?;
        let writer = fixture.store.lock()?;
        let token = fixture.tokens[0];
        let receipt = fixture.store.receipt(&token)?;
        let mut budget = Fixture::budget();
        budget.max_metadata_bytes = 16 * 1024;
        assert!(
            fixture
                .store
                .history_convert(&writer, &fixture.quota, &token, budget)
                .is_err()
        );
        let status = fixture.quota.status()?;
        fixture.quota.configure(airk_store::quota::QuotaPolicy {
            root_bytes: status.used_bytes,
            default_collection_bytes: status.used_bytes,
            collections: BTreeMap::new(),
        })?;
        let error = fixture
            .store
            .history_convert(&writer, &fixture.quota, &token, Fixture::budget())
            .unwrap_err();
        assert!(
            error
                .chain()
                .any(|cause| cause.is::<airk_store::quota::QuotaPressure>()),
            "{error:#}"
        );
        assert_eq!(fixture.store.receipt(&token)?, receipt);
        assert!(
            fixture
                .store
                .catalog_at(&token, 64 * 1024 * 1024)?
                .is_some()
        );
        assert!(
            fixture
                .store
                .history_at(&token, 64 * 1024 * 1024)?
                .is_none()
        );
        assert!(!fixture.store.directory().join("history").exists());
        Ok(())
    }

    #[test]
    fn bounded_partial_history_removal_recovers_with_retained_descriptor_witness() -> Result<()> {
        let fixture = Fixture::new()?;
        let writer = fixture.store.lock()?;
        let token = fixture.tokens[0];
        fixture
            .store
            .history_convert(&writer, &fixture.quota, &token, Fixture::budget())?;
        let package = fixture
            .store
            .directory()
            .join("history")
            .join(token.revision.to_string());
        let largest = fs::read_dir(&package)?
            .map(|entry| {
                entry
                    .and_then(|entry| entry.metadata())
                    .map(|meta| meta.len())
            })
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap();
        let mut budget = Fixture::budget();
        budget.max_removal_bytes = largest;
        let result = fixture
            .store
            .history_remove(&writer, &fixture.quota, &token, budget)?;
        assert_eq!(result.status, ConversionStatus::Pending);
        assert_eq!(result.stage, Some(HistoryRemovalStage::PackageDetached));
        assert!(result.removed_bytes > 0);
        assert!(fixture.store.receipt(&token)?.is_some());
        assert!(
            fixture
                .store
                .history_at(&token, 64 * 1024 * 1024)?
                .is_none()
        );
        assert!(
            fixture
                .store
                .catalog_at(&token, 64 * 1024 * 1024)?
                .is_none()
        );
        let recovered =
            fixture
                .store
                .history_recover(&writer, &fixture.quota, Fixture::budget())?;
        assert!(!recovered.pending);
        assert_eq!(recovered.removals.len(), 1);
        assert!(fixture.store.receipt(&token)?.is_none());
        Ok(())
    }

    #[test]
    fn missing_or_modified_committed_history_is_corruption_and_blocks_gc() -> Result<()> {
        for name in [
            "descriptor.json",
            "inventory.json",
            "lineage.json",
            "manifest.json",
        ] {
            let fixture = Fixture::new()?;
            let writer = fixture.store.lock()?;
            let token = fixture.tokens[0];
            fixture
                .store
                .history_convert(&writer, &fixture.quota, &token, Fixture::budget())?;
            let path = fixture
                .store
                .directory()
                .join("history")
                .join(token.revision.to_string())
                .join(name);
            fs::write(&path, b"modified")?;
            assert!(fixture.store.history_at(&token, 64 * 1024 * 1024).is_err());
            assert!(fixture.store.catalog_at(&token, 64 * 1024 * 1024).is_err());
            let run = fixture.store.maintain_retention(
                &writer,
                &fixture.quota,
                MaintenanceTrigger::ExplicitMaintenance,
                Fixture::budget(),
            );
            assert!(run.receipt.error.is_some());
            assert!(fixture.store.receipt(&token)?.is_some());
        }
        Ok(())
    }
}
