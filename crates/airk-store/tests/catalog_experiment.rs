// SPDX-License-Identifier: Apache-2.0
//! Original O2a differential fixtures; active publication continues to rebuild.
use airk_core::{
    Artifact, ArtifactSource, DocumentLocation, EvidenceBasis, FactHit, RelationshipHit,
};
use airk_core::{changes::*, continuity::*, extraction::*, identity::*, publication::*};
use airk_store::{
    ExpectedHead, FileStore,
    catalog_experiment::*,
    quota::{Quota, Reservation},
};
use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{cell::Cell, collections::BTreeMap, fs, path::Path, time::Instant};
const ENGINE: &str = airk_store::reset::CURRENT_RETRIEVAL_ENGINE;
const CAP: u64 = 64 * 1024 * 1024;
fn hash(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn profile() -> ExtractionProfile {
    ExtractionProfile {
        contract_version: 1,
        adapter_id: "code".into(),
        adapter_version: "original-experiment/1".into(),
        language: "rust".into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 3,
        options: BTreeMap::new(),
        configuration_inputs: BTreeMap::new(),
    }
}
fn lineage(generation: u64, parent: Option<RevisionToken>) -> LineageEnvelope {
    let floor = CoverageFloor {
        complete_after: 1,
        reason: Some(CoverageReason::BaselineUnavailable),
    };
    LineageEnvelope {
        version: LINEAGE_CONTRACT_VERSION,
        incarnation: Incarnation::from_bytes([9; 16]),
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
            .map(|token| RetainedPublicationHeader {
                token,
                parent: None,
                observed_unix_seconds: token.generation,
                reason: PublicationReason::Index,
            })
            .into_iter()
            .collect(),
        events: vec![],
        resolvers: vec![],
    }
}
struct Source<'a> {
    store: &'a FileStore,
    snapshot: airk_core::Snapshot,
    loads: Cell<usize>,
}
impl ArtifactSource for Source<'_> {
    fn load_artifact(&self, path: &str) -> Result<Artifact> {
        self.loads.set(self.loads.get() + 1);

        airk_store::blobs::RetainedSource {
            store: self.store,
            snapshot: &self.snapshot,
        }
        .load_artifact(path)
    }
}
fn add_file(
    store: &FileStore,
    inventory: &mut InventoryV2,
    path: &str,
    id: u64,
    revision: u64,
    facts: usize,
    reservation: &Reservation,
) -> Result<()> {
    let profile = inventory.profiles.values().next().unwrap().clone();
    let text = format!(
        "Original declaration fixture {path} revision {revision}. {}\n",
        "meaningful body; ".repeat(12)
    );
    let location = DocumentLocation::Lines { start: 1, end: 1 };
    let span = LocalSpan {
        start: 0,
        end: text.len(),
    };
    let descriptor = ContinuityDescriptor {
        algorithm_version: 1,
        subject: ContinuitySubject::Fact(0),
        owner: None,
        region: None,
        family: ContinuityFamily::Code,
        category: "function".into(),
        name: Some(hash(path.as_bytes())),
        explicit_identity: None,
        profile: profile.identity()?,
        provenance: hash(b"original"),
        dimensions: BTreeMap::new(),
        unsupported_dimensions: vec![],
        span: span.clone(),
        location: location.clone(),
        complete: false,
        body_informative: true,
    };
    let payload = ExtractionPayload {
        semantic_projection: Default::default(),
        contract_version: 1,
        extraction_method: "original_fixture".into(),
        parse_status: "parsed".into(),
        provenance: vec![],
        text: text.clone(),
        facts: (0..facts)
            .map(|ordinal| LocalFact {
                id: ordinal as u32,
                owner: None,
                kind: if ordinal == 0 { "function" } else { "call" }.into(),
                name: format!("symbol{ordinal}_{revision}"),
                span: LocalSpan {
                    start: ordinal,
                    end: if ordinal == 0 {
                        text.len()
                    } else {
                        ordinal + 1
                    },
                },
                syntax_kind: "original".into(),
                start_line: 1,
                end_line: 1,
                location: location.clone(),
                document: None,
                region: None,
                attributes: BTreeMap::new(),
                code: Some(LocalCodeEvidence {
                    fingerprints: None,
                    syntax_id: LocalSyntaxId(ordinal as u32),
                    owner_id: (ordinal > 0).then_some(LocalSyntaxId(0)),
                    qualified_name: format!("Fixture::symbol{ordinal}"),
                    scope: vec!["Fixture".into()],
                    signature: Some("()".into()),
                    span: Some(airk_core::CodeSpan {
                        start_byte: ordinal,
                        end_byte: if ordinal == 0 {
                            text.len()
                        } else {
                            ordinal + 1
                        },
                    }),
                    signature_span: None,
                    parameters: None,
                    return_type: None,
                    modifiers: vec![],
                    documentation: None,
                    target: (ordinal > 0).then(|| "written_target".into()),
                    basis: EvidenceBasis::Syntax,
                    producer: "original-experiment".into(),
                    producer_version: "1".into(),
                    coverage: "syntax".into(),
                    details_truncated: false,
                }),
            })
            .collect(),
        syntax_identities: (0..facts)
            .map(|id| LocalSyntaxIdentity {
                id: LocalSyntaxId(id as u32),
                span: LocalSpan {
                    start: id,
                    end: if id == 0 { text.len() } else { id + 1 },
                },
                syntax_kind: "original".into(),
                region: None,
            })
            .collect(),
        chunks: vec![LocalChunk {
            span: span.clone(),
            start_line: 1,
            end_line: 1,
            location: location.clone(),
            region: None,
        }],
        regions: vec![],
        relationships: vec![],
        continuity: vec![descriptor.clone()],
        continuity_coverage: BTreeMap::from([(
            ContinuityFamily::Code,
            ContinuityCoverage {
                supported: true,
                complete: false,
            },
        )]),
        diagnostics: vec![],
        partial: false,
    };
    let artifact = airk_core::binding::bind_payload(
        &payload,
        &profile,
        airk_core::binding::ArtifactBinding {
            path,
            source_hash: hash(text.as_bytes()),
            source_bytes: text.len(),
        },
    )?;
    let compact = store.retain_payload_with_budget(
        &artifact,
        &payload,
        &profile,
        Some(text.as_bytes()),
        Some(reservation),
    )?;
    let reference = compact.retained.unwrap();
    let mut identity = [0u8; 16];
    identity[..8].copy_from_slice(&id.to_le_bytes());
    inventory.files.insert(
        path.into(),
        InventoryFile {
            state: FileState {
                artifact_id: ArtifactId::from_bytes(identity),
                stamps: FileStamps {
                    first_generation: 1,
                    path_generation: revision,
                    content_generation: revision,
                    evidence_generation: revision,
                },
                source_hash: hash(text.as_bytes()),
                profile_id: profile.identity()?,
                payload_digest: reference.object.parse()?,
                evidence_digest: hash(text.as_bytes()),
                root: inventory.root.clone(),
                path: path.into(),
            },
            source_bytes: text.len(),
            payload_bytes: reference.object_bytes,
            extracted_bytes: reference.extracted_bytes,
            chunks: 1,
            indexed_lines: 1,
            facts,
            raw_source: true,
            language: "rust".into(),
            extraction_method: "original_fixture".into(),
            parse_status: "parsed".into(),
            continuity: vec![ContinuityRecord {
                ambiguous_predecessor: false,
                key: ContinuityKey::Symbol(SymbolKey::from_bytes(identity)),
                descriptor,
                first_generation: 1,
                changed: BTreeMap::from([(ChangeDimension::Location, revision)]),
            }],
            continuity_coverage: BTreeMap::from([(
                ContinuityFamily::Code,
                ContinuityCoverage {
                    supported: true,
                    complete: false,
                },
            )]),
        },
    );
    Ok(())
}
fn token(inventory: &InventoryV2) -> Result<RevisionToken> {
    Ok(RevisionToken {
        incarnation: inventory.incarnation,
        generation: inventory.generation,
        revision: revision_digest(
            inventory.retrieval_specification.as_bytes(),
            &inventory.canonical_bytes()?,
        ),
    })
}
fn publish(
    store: &FileStore,
    inventory: &InventoryV2,
    lineage: &LineageEnvelope,
    reservation: &Reservation,
) -> Result<()> {
    let token = token(inventory)?;
    let directory = reservation.staging_directory()?.join("baseline");
    fs::create_dir_all(&directory)?;
    let source = Source {
        store,
        snapshot: inventory.projection()?,
        loads: Cell::new(0),
    };
    store.prepare_catalog_v2(
        &directory,
        inventory,
        lineage,
        &token,
        &source,
        Some(reservation),
        CAP,
    )?;
    let inventory_bytes = inventory.canonical_bytes()?;
    reservation.charge(inventory_bytes.len() as u64)?;
    fs::write(directory.join("inventory.json"), &inventory_bytes)?;
    fs::create_dir_all(directory.join("vector-map"))?;
    fs::write(directory.join("vector-map/meta.json"), b"{}")?;
    let mut components = BTreeMap::new();
    for name in [
        "inventory.json",
        "lineage.json",
        "catalog.sqlite",
        "vector-map/meta.json",
    ] {
        let bytes = fs::read(directory.join(name))?;
        components.insert(name, json!({"bytes":bytes.len(),"sha256":hash(&bytes)}));
    }
    let bytes = serde_json::to_vec(
        &json!({"schema":4,"engine":ENGINE,"token":token,"collection":"experiment","chunks":inventory.files.len(),"vectors":0,"vector_occurrences":0,"vector_selection_digest":inventory.vector_selection.digest()?,"vector_eligible_artifacts":inventory.files.len(),"vector_excluded_artifacts":0,"model_sha256":null,"embedding":"none","inventory_digest":hash(&inventory_bytes),"components":components}),
    )?;
    reservation.charge(bytes.len() as u64)?;
    fs::write(directory.join("manifest.json"), &bytes)?;
    fs::create_dir_all(store.directory().join("generations"))?;
    let lease =
        airk_store::blobs::reader_lease_path(store.directory(), &token.revision.to_string())?;
    fs::create_dir_all(lease.parent().unwrap())?;
    fs::write(lease, [])?;
    fs::rename(
        directory,
        store
            .directory()
            .join("generations")
            .join(token.revision.to_string()),
    )?;
    store.publish_catalog_v2(
        inventory,
        &PublicationReceipt {
            token,
            inventory_digest: hash(&inventory_bytes),
            manifest_digest: hash(&bytes),
        },
        &ExpectedHead::Empty,
        reservation,
    )?;
    Ok(())
}
struct Fixture {
    root: tempfile::TempDir,
    store: FileStore,
    quota: Quota,
    inventory: InventoryV2,
}
impl Fixture {
    fn new(files: usize, facts: usize) -> Result<Self> {
        let root = tempfile::tempdir()?;
        let store = FileStore::new(root.path().join("collections-1.2/experiment"));
        let writer = store.lock()?;
        let quota = Quota::open(root.path())?;
        let reservation = quota.reserve("experiment", 256 * 1024 * 1024)?;
        let profile = profile();
        let lineage = lineage(1, None);
        let mut inventory = InventoryV2 {
            vector_selection: Default::default(),
            version: airk_core::publication::INVENTORY_VERSION,
            incarnation: lineage.incarnation,
            generation: 1,
            collection: "experiment".into(),
            root: "/original".into(),
            roots: BTreeMap::new(),
            lineage_digest: hash(&serde_json::to_vec(&lineage)?),
            profiles: BTreeMap::from([(profile.identity()?, profile)]),
            files: BTreeMap::new(),
            retrieval_specification: ENGINE.into(),
            embedding_model: None,
            embedding_pipeline_digest: None,
            extractor_version: "native-facts/7".into(),
            index_detail: None,
            extraction_policy_digest: None,
            analysis: None,
            imported_facts: vec![],
            imported_relationships: vec![],
        };
        for id in 0..files {
            add_file(
                &store,
                &mut inventory,
                &format!("src/file{id:05}.rs"),
                id as u64 + 1,
                1,
                facts,
                &reservation,
            )?;
        }
        publish(&store, &inventory, &lineage, &reservation)?;
        drop(reservation);
        drop(writer);
        Ok(Self {
            root,
            store,
            quota,
            inventory,
        })
    }
    fn next(&self) -> Result<(InventoryV2, LineageEnvelope)> {
        let lineage = lineage(2, Some(token(&self.inventory)?));
        let mut inventory = self.inventory.clone();
        inventory.generation = 2;
        inventory.lineage_digest = hash(&serde_json::to_vec(&lineage)?);
        Ok((inventory, lineage))
    }
}
fn logical_rows(path: &Path) -> Result<BTreeMap<String, Vec<Vec<String>>>> {
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut result = BTreeMap::new();
    let mut schema = conn.prepare(
        "SELECT name,type,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let entries = schema
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (name, kind, sql) in entries {
        result.insert(
            format!("schema:{name}"),
            vec![vec![kind.clone(), sql.unwrap_or_default()]],
        );
        // Internal dictionary ordinals and unused dictionary rows are physical
        // layout, not evidence. Compare the reconstructed compatibility views.
        if matches!(
            name.as_str(),
            "evidence_files" | "evidence_producers" | "fact_rows" | "edge_rows"
        ) {
            continue;
        }
        if kind != "table" && !matches!(name.as_str(), "facts" | "relationships") {
            continue;
        }
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{name}\""))?;
        let width = stmt.column_count();
        let mut rows = stmt
            .query_map([], |row| {
                (0..width)
                    .map(|column| row.get_ref(column).map(|value| format!("{value:?}")))
                    .collect::<std::result::Result<Vec<_>, _>>()
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.sort();
        result.insert(name.clone(), rows);
        let mut indexes = conn.prepare(&format!("PRAGMA index_list(\"{name}\")"))?;
        let mut values = indexes
            .query_map([], |row| {
                Ok(vec![
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?.to_string(),
                    row.get::<_, String>(3)?,
                ])
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        values.sort();
        result.insert(format!("indexes:{name}"), values);
        let mut foreign = conn.prepare(&format!("PRAGMA foreign_key_list(\"{name}\")"))?;
        assert!(
            foreign.query([])?.next()?.is_none(),
            "unexpected foreign key contract"
        );
    }
    Ok(result)
}
fn compare(
    fixture: &Fixture,
    inventory: &InventoryV2,
    lineage: &LineageEnvelope,
    label: &str,
) -> Result<ExperimentMetrics> {
    let base = fixture.store.catalog()?.context("base missing")?;
    let reservation = fixture.quota.reserve("experiment", 192 * 1024 * 1024)?;
    let staging = reservation.staging_directory()?;
    let full = staging.join("full");
    let incremental = staging.join("incremental");
    fs::create_dir_all(&full)?;
    fs::create_dir_all(&incremental)?;
    let source = Source {
        store: &fixture.store,
        snapshot: inventory.projection()?,
        loads: Cell::new(0),
    };
    let target = token(inventory)?;
    let start = Instant::now();
    fixture.store.prepare_catalog_v2(
        &full,
        inventory,
        lineage,
        &target,
        &source,
        Some(&reservation),
        CAP,
    )?;
    let full_time = start.elapsed().as_micros();
    let full_loads = source.loads.replace(0);
    let metrics = prepare(ExperimentRequest {
        store: &fixture.store,
        base: &base,
        inventory,
        lineage,
        target,
        source: &source,
        reservation: &reservation,
        directory: &incremental,
        max_catalog_bytes: CAP,
    })?;
    let full_rows = logical_rows(&full.join("catalog.sqlite"))?;
    let incremental_rows = logical_rows(&incremental.join("catalog.sqlite"))?;
    assert_eq!(
        full_rows.keys().collect::<Vec<_>>(),
        incremental_rows.keys().collect::<Vec<_>>()
    );
    for (table, rows) in full_rows {
        assert!(
            rows == incremental_rows[&table],
            "{label}: different rows in {table}"
        );
    }
    assert_eq!(
        fs::read(full.join("lineage.json"))?,
        fs::read(incremental.join("lineage.json"))?
    );
    assert_eq!(source.loads.get(), metrics.loaded_artifacts);
    eprintln!(
        "O2A {}",
        json!({"case":label,"files":inventory.files.len(),"full_micros":full_time,"full_payload_loads":full_loads,"full_output_bytes":fs::metadata(full.join("catalog.sqlite"))?.len(),"incremental":metrics})
    );
    assert_eq!(
        fixture.store.persistent_revision()?.unwrap().token,
        base.receipt.as_ref().map(|receipt| receipt.token)
    );
    Ok(metrics)
}

#[test]
fn differential_noop_edit_add_delete_move_and_lineage_rows() -> Result<()> {
    let fixture = Fixture::new(8, 4)?;
    let (mut inventory, mut lineage) = fixture.next()?;
    lineage.events.push(ChangeEvent {
        generation: 2,
        event_id: 1,
        subject: ChangeSubject::Global {
            domain: "original.metadata".into(),
        },
        before: None,
        after: None,
        before_detail: None,
        after_detail: None,
        dimensions: vec![],
        cause: ChangeCause::MetadataMaintenance,
        basis: ContinuityBasis::Unknown,
    });
    inventory.lineage_digest = hash(&serde_json::to_vec(&lineage)?);
    assert_eq!(
        compare(&fixture, &inventory, &lineage, "noop_lineage")?.loaded_artifacts,
        0
    );
    let writer = fixture.store.lock()?;
    let reservation = fixture.quota.reserve("experiment", 16 * 1024 * 1024)?;
    add_file(
        &fixture.store,
        &mut inventory,
        "src/file00000.rs",
        1,
        2,
        7,
        &reservation,
    )?;
    assert_eq!(
        compare(&fixture, &inventory, &lineage, "edit")?.loaded_artifacts,
        1
    );
    add_file(
        &fixture.store,
        &mut inventory,
        "src/new_é.rs",
        99,
        2,
        3,
        &reservation,
    )?;
    inventory.files.remove("src/file00001.rs");
    let moved = inventory.files.remove("src/file00002.rs").unwrap();
    let mut relocated = moved.clone();
    relocated.state.path = "src/moved.rs".into();
    relocated.state.stamps.path_generation = 2;
    inventory.files.insert("src/moved.rs".into(), relocated);
    let after = &inventory.files["src/moved.rs"].state;
    let endpoint = |state: &FileState, citation_path: &str| ChangeEndpoint {
        root: state.root.clone(),
        path: state.path.clone(),
        citation_path: Some(citation_path.into()),
        source_hash: state.source_hash,
        profile_id: state.profile_id,
    };
    lineage.resolvers.push(ResolverRecord {
        artifact_id: moved.state.artifact_id,
        observed_generation: 2,
        from: endpoint(&moved.state, "src/file00002.rs"),
        to: Some(endpoint(after, "src/moved.rs")),
        state: ResolutionState::ExactMove,
        basis: ContinuityBasis::UniqueCompatibleContent,
    });
    inventory.lineage_digest = hash(&serde_json::to_vec(&lineage)?);
    let result = compare(&fixture, &inventory, &lineage, "add_delete_move_unicode")?;
    assert_eq!(result.input.added.len(), 2);
    assert_eq!(result.input.removed.len(), 2);
    assert_eq!(result.loaded_artifacts, 3);
    drop(reservation);
    drop(writer);
    Ok(())
}

#[test]
fn production_row_reuse_matches_full_build_and_reloads_changed_inputs_only() -> Result<()> {
    let fixture = Fixture::new(4, 6)?;
    let base = fixture.store.catalog()?.context("base missing")?;
    let _writer = fixture.store.lock()?;
    let reservation = fixture.quota.reserve("experiment", 256 * 1024 * 1024)?;
    let stage = reservation.staging_directory()?;
    let (mut inventory, lineage) = fixture.next()?;
    for (case, expected_loads) in [
        ("unchanged", 0),
        ("edit", 1),
        ("new_import", 1),
        ("new_root", 4),
    ] {
        if case == "edit" {
            add_file(
                &fixture.store,
                &mut inventory,
                "src/file00000.rs",
                1,
                2,
                9,
                &reservation,
            )?;
        }
        if case == "new_import" {
            inventory.analysis = Some(airk_core::AnalysisManifest {
                version: 1,
                root: inventory.root.clone(),
                collection: inventory.collection.clone(),
                extractor_version: inventory.extractor_version.clone(),
                source_hashes: inventory
                    .files
                    .iter()
                    .map(|(p, f)| (p.clone(), f.state.source_hash.to_string()))
                    .collect(),
                build_inputs: BTreeMap::new(),
                producer: "original provider".into(),
                producer_version: "1".into(),
                index_sha256: None,
            });
            inventory.imported_facts.push(FactHit {
                candidate: false,
                continuity_key: None,
                name: "new provider assertion".into(),
                kind: "definition".into(),
                syntax_kind: "original assertion".into(),
                language: "rust".into(),
                document: None,
                code: None,
                citation: airk_core::Citation {
                    path: "src/file00000.rs".into(),
                    content_hash: inventory.files["src/file00000.rs"]
                        .state
                        .source_hash
                        .to_string(),
                    start_line: 1,
                    end_line: 1,
                    location: DocumentLocation::Lines { start: 1, end: 1 },
                },
            });
        }
        if case == "new_root" {
            inventory.roots.insert("notes".into(), "/secondary".into());
        }
        let full = stage.join(format!("{case}-full"));
        let reused = stage.join(format!("{case}-reused"));
        fs::create_dir_all(&full)?;
        fs::create_dir_all(&reused)?;
        let source = Source {
            store: &fixture.store,
            snapshot: inventory.projection()?,
            loads: Cell::new(0),
        };
        let target = token(&inventory)?;
        fixture.store.prepare_catalog_v2(
            &full,
            &inventory,
            &lineage,
            &target,
            &source,
            Some(&reservation),
            16 * 1024 * 1024,
        )?;
        assert_eq!(source.loads.replace(0), 4);
        fixture.store.prepare_catalog_v2_reusing(
            &reused,
            &inventory,
            &lineage,
            &target,
            &source,
            Some(&reservation),
            16 * 1024 * 1024,
            Some(&base),
        )?;
        assert_eq!(source.loads.get(), expected_loads, "{case}");
        assert_eq!(
            logical_rows(&full.join("catalog.sqlite"))?,
            logical_rows(&reused.join("catalog.sqlite"))?,
            "{case}"
        );
    }
    let connection = Connection::open_with_flags(
        fixture
            .store
            .directory()
            .join("generations")
            .join(base.receipt.as_ref().unwrap().token.revision.to_string())
            .join("catalog.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let files: i64 = connection.query_row(
        "SELECT count(*) FROM evidence_files WHERE length(source_hash)=32",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(files, 4);
    let repeated: i64 = connection.query_row("SELECT count(*) FROM fact_rows WHERE json_type(payload,'$.citation.path') IS NOT NULL OR json_type(payload,'$.citation.content_hash') IS NOT NULL OR json_type(payload,'$.code.producer') IS NOT NULL",[],|r|r.get(0))?;
    assert_eq!(repeated, 0);
    Ok(())
}
#[test]
fn global_profile_roots_and_import_changes_choose_full_rebuild() -> Result<()> {
    let fixture = Fixture::new(4, 3)?;
    let (mut inventory, lineage) = fixture.next()?;
    inventory.roots.insert("notes".into(), "/secondary".into());
    assert_eq!(
        compare(&fixture, &inventory, &lineage, "alias_global")?.strategy,
        "full_fallback"
    );
    let (mut inventory, lineage) = fixture.next()?;
    let mut changed = profile();
    changed.adapter_version = "original-experiment/2".into();
    inventory.profiles.insert(changed.identity()?, changed);
    assert_eq!(
        compare(&fixture, &inventory, &lineage, "profile_global")?.strategy,
        "full_fallback"
    );
    let (mut inventory, lineage) = fixture.next()?;
    let artifact = Source {
        store: &fixture.store,
        snapshot: inventory.projection()?,
        loads: Cell::new(0),
    }
    .load_artifact("src/file00000.rs")?;
    inventory.analysis = Some(airk_core::AnalysisManifest {
        version: 1,
        root: inventory.root.clone(),
        collection: inventory.collection.clone(),
        extractor_version: inventory.extractor_version.clone(),
        source_hashes: inventory
            .files
            .iter()
            .map(|(path, file)| (path.clone(), file.state.source_hash.to_string()))
            .collect(),
        build_inputs: BTreeMap::new(),
        producer: "original".into(),
        producer_version: "1".into(),
        index_sha256: None,
    });
    inventory.imported_facts.push(FactHit {
        candidate: false,
        continuity_key: None,
        name: "independent-provider".into(),
        kind: "function".into(),
        syntax_kind: "producer".into(),
        language: "rust".into(),
        citation: airk_core::Citation {
            path: artifact.path.clone(),
            content_hash: artifact.content_hash.clone(),
            start_line: 1,
            end_line: 1,
            location: DocumentLocation::Lines { start: 1, end: 1 },
        },
        document: None,
        code: artifact.facts[0].code.clone(),
    });
    inventory.imported_relationships.push(RelationshipHit {
        source: "provider_source".into(),
        target: "written_target".into(),
        kind: "calls".into(),
        resolution: "producer".into(),
        basis: EvidenceBasis::Syntax,
        producer: "original".into(),
        producer_version: "1".into(),
        citation: inventory.imported_facts[0].citation.clone(),
    });
    assert_eq!(
        compare(&fixture, &inventory, &lineage, "producer_global")?.strategy,
        "full_fallback"
    );
    Ok(())
}
#[test]
fn quota_failure_and_source_corruption_cannot_modify_active_catalog() -> Result<()> {
    let fixture = Fixture::new(8, 4)?;
    let base = fixture.store.catalog()?.unwrap();
    let receipt = base.receipt.as_ref().unwrap();
    let active = fixture
        .store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string())
        .join("catalog.sqlite");
    let original = fs::read(&active)?;
    let (inventory, lineage) = fixture.next()?;
    let source = Source {
        store: &fixture.store,
        snapshot: inventory.projection()?,
        loads: Cell::new(0),
    };
    let reservation = fixture.quota.reserve("experiment", 16384)?;
    let directory = reservation.staging_directory()?.join("denied");
    fs::create_dir_all(&directory)?;
    assert!(
        prepare(ExperimentRequest {
            store: &fixture.store,
            base: &base,
            inventory: &inventory,
            lineage: &lineage,
            target: token(&inventory)?,
            source: &source,
            reservation: &reservation,
            directory: &directory,
            max_catalog_bytes: CAP
        })
        .is_err()
    );
    assert_eq!(fs::read(&active)?, original);
    assert_eq!(source.loads.get(), 0);
    drop(reservation);
    let reservation = fixture.quota.reserve("experiment", 128 * 1024 * 1024)?;
    let directory = reservation.staging_directory()?.join("corrupt");
    fs::create_dir_all(&directory)?;
    let mut corrupt = original.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    fs::write(&active, &corrupt)?;
    assert!(
        prepare(ExperimentRequest {
            store: &fixture.store,
            base: &base,
            inventory: &inventory,
            lineage: &lineage,
            target: token(&inventory)?,
            source: &source,
            reservation: &reservation,
            directory: &directory,
            max_catalog_bytes: CAP
        })
        .is_err()
    );
    assert_eq!(fs::read(&active)?, corrupt);
    fs::write(active, original)?;
    assert_eq!(
        fixture.store.persistent_revision()?.unwrap().token,
        Some(receipt.token)
    );
    Ok(())
}
#[test]
fn measure_small_and_larger_sparse_catalog_edits() -> Result<()> {
    for files in [16, 1000] {
        let fixture = Fixture::new(files, 12)?;
        let writer = fixture.store.lock()?;
        let reservation = fixture.quota.reserve("experiment", 16 * 1024 * 1024)?;
        let (mut inventory, lineage) = fixture.next()?;
        add_file(
            &fixture.store,
            &mut inventory,
            "src/file00000.rs",
            1,
            2,
            12,
            &reservation,
        )?;
        for sample in 0..3 {
            let metrics = compare(
                &fixture,
                &inventory,
                &lineage,
                &format!("sparse_{files}_sample{sample}"),
            )?;
            assert_eq!(metrics.loaded_artifacts, 1);
            assert_eq!(metrics.temporary_disk_bytes, 0);
            assert!(metrics.copied_bytes > 0);
        }
        drop(reservation);
        drop(writer);
        assert!(fixture.root.path().exists());
    }
    Ok(())
}

#[test]
fn failure_after_private_copy_leaves_active_bytes_and_pointer_unchanged() -> Result<()> {
    struct FailedSource;
    impl ArtifactSource for FailedSource {
        fn load_artifact(&self, _: &str) -> Result<Artifact> {
            anyhow::bail!("original injected retained-payload failure")
        }
    }
    let fixture = Fixture::new(4, 3)?;
    let writer = fixture.store.lock()?;
    let base = fixture.store.catalog()?.unwrap();
    let receipt = base.receipt.as_ref().unwrap();
    let path = fixture
        .store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string())
        .join("catalog.sqlite");
    let original = fs::read(&path)?;
    let reservation = fixture.quota.reserve("experiment", 128 * 1024 * 1024)?;
    let (mut inventory, lineage) = fixture.next()?;
    add_file(
        &fixture.store,
        &mut inventory,
        "src/file00000.rs",
        1,
        2,
        3,
        &reservation,
    )?;
    let directory = reservation.staging_directory()?.join("failed-copy");
    fs::create_dir_all(&directory)?;
    let error = prepare(ExperimentRequest {
        store: &fixture.store,
        base: &base,
        inventory: &inventory,
        lineage: &lineage,
        target: token(&inventory)?,
        source: &FailedSource,
        reservation: &reservation,
        directory: &directory,
        max_catalog_bytes: CAP,
    })
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected retained-payload failure")
    );
    assert!(directory.join("catalog.sqlite").exists());
    assert!(!directory.join("lineage.json").exists());
    assert_eq!(fs::read(path)?, original);
    assert_eq!(
        fixture.store.persistent_revision()?.unwrap().token,
        Some(receipt.token)
    );
    drop(reservation);
    drop(writer);
    Ok(())
}
