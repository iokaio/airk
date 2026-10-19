// SPDX-License-Identifier: Apache-2.0
//! Original deterministic O2b fixtures; no active publication chooses this writer.
mod support;
use airk_core::{
    Artifact, ArtifactSource, Chunk, DocumentLocation, SearchQuery, Snapshot,
    changes::{
        ChangeCoverage, CoverageFloor, FileStamps, FileState, LINEAGE_CONTRACT_VERSION,
        LineageEnvelope, PublicationObservation, PublicationReason,
    },
    extraction::ExtractionProfile,
    identity::{
        ArtifactId, Incarnation, PublicationReceipt, RevisionToken, Sha256Digest, revision_digest,
    },
    publication::{InventoryFile, InventoryV2},
};
use airk_retrieval::{
    bounded::Charge,
    lexical_experiment::{self, ExperimentRequest},
    persistent::{self, PersistentIndex, V2Preparation},
};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

fn unlinked_tempdir() -> Result<tempfile::TempDir> {
    // The staging writer rejects linked ancestors; macOS /var is a symlink.
    Ok(tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?)
}

fn raw(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn artifact(path: &str, chunks: usize, edited: bool) -> Artifact {
    let chunks: Vec<_> = (0..chunks)
        .map(|i| Chunk {
            start_line: i + 1,
            end_line: i + 1,
            text: format!(
                "Original café aurora invoice {path} chunk {i} {}\n",
                if edited { "revised" } else { "stable" }
            ),
            location: DocumentLocation::Lines {
                start: i + 1,
                end: i + 1,
            },
        })
        .collect();
    let text: String = chunks.iter().map(|chunk| chunk.text.as_str()).collect();
    Artifact {
        path: path.into(),
        content_hash: raw(text.as_bytes()).to_string(),
        bytes: text.len(),
        chunks,
        language: "text".into(),
        extraction_method: "utf8".into(),
        parse_status: "complete".into(),
        facts: vec![],
        retained: None,
    }
}
fn source(count: usize) -> Snapshot {
    let mut source = Snapshot::empty("original-lexical-experiment".into());
    source.generation = 1;
    for i in 0..count {
        let path = format!("files/{i:05}.txt");
        source
            .artifacts
            .insert(path.clone(), artifact(&path, 3, false));
    }
    source
}
fn inventory(
    source: &Snapshot,
    parent: Option<(&InventoryV2, RevisionToken)>,
) -> Result<(InventoryV2, LineageEnvelope, RevisionToken)> {
    let profile = ExtractionProfile {
        contract_version: 1,
        adapter_id: "text".into(),
        adapter_version: "native-facts/7".into(),
        language: "text".into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 1,
        options: BTreeMap::new(),
        configuration_inputs: BTreeMap::new(),
    };
    let id = profile.identity()?;
    let incarnation = Incarnation::from_bytes([13; 16]);
    let floor = CoverageFloor {
        complete_after: 1,
        reason: None,
    };
    let lineage = LineageEnvelope {
        version: LINEAGE_CONTRACT_VERSION,
        incarnation,
        observation: PublicationObservation {
            generation: source.generation,
            parent: parent.map(|(_, token)| token),
            observed_unix_seconds: source.generation,
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
    let mut files = BTreeMap::new();
    for (path, artifact) in &source.artifacts {
        if let Some(file) = parent
            .and_then(|(old, _)| old.files.get(path))
            .filter(|file| file.state.source_hash.to_string() == artifact.content_hash)
        {
            files.insert(path.clone(), file.clone());
            continue;
        }
        let hash = artifact.content_hash.parse()?;
        let path_hash = Sha256::digest(path.as_bytes());
        let mut identity = [0u8; 16];
        identity.copy_from_slice(&path_hash[..16]);
        files.insert(
            path.clone(),
            InventoryFile {
                continuity: vec![],
                continuity_coverage: Default::default(),
                state: FileState {
                    artifact_id: ArtifactId::from_bytes(identity),
                    stamps: FileStamps {
                        first_generation: 1,
                        path_generation: 1,
                        content_generation: source.generation,
                        evidence_generation: source.generation,
                    },
                    source_hash: hash,
                    profile_id: id,
                    payload_digest: hash,
                    evidence_digest: hash,
                    root: source.root.clone(),
                    path: path.clone(),
                },
                source_bytes: artifact.bytes,
                payload_bytes: artifact.bytes,
                extracted_bytes: artifact.extracted_bytes(),
                chunks: artifact.chunks.len(),
                indexed_lines: artifact.chunks.len(),
                facts: 0,
                raw_source: true,
                language: "text".into(),
                extraction_method: "utf8".into(),
                parse_status: "complete".into(),
            },
        );
    }
    let inventory = InventoryV2 {
        embedding_pipeline_digest: None,
        index_detail: None,
        extraction_policy_digest: None,
        version: 2,
        incarnation,
        generation: source.generation,
        collection: source.collection.clone(),
        root: source.root.clone(),
        roots: BTreeMap::new(),
        lineage_digest: raw(&serde_json::to_vec(&lineage)?),
        profiles: BTreeMap::from([(id, profile)]),
        files,
        retrieval_specification: String::from_utf8(persistent::retrieval_specification().to_vec())?,
        embedding_model: None,
        extractor_version: source.extractor_version.clone(),
        analysis: None,
        imported_facts: vec![],
        imported_relationships: vec![],
    };
    let token = RevisionToken {
        incarnation,
        generation: source.generation,
        revision: revision_digest(
            inventory.retrieval_specification.as_bytes(),
            &inventory.canonical_bytes()?,
        ),
    };
    Ok((inventory, lineage, token))
}
fn full(
    base: &Path,
    source: &Snapshot,
    inventory: &InventoryV2,
    lineage: &LineageEnvelope,
    token: RevisionToken,
) -> Result<PublicationReceipt> {
    let bytes = inventory.canonical_bytes()?;
    let projection = inventory.projection()?;
    persistent::prepare_streaming(
        base,
        &V2Preparation {
            snapshot: &projection,
            canonical_inventory: &bytes,
            token,
        },
        None,
        &support::FixtureSource::new(source)?,
        None,
        None,
        None,
        |directory| {
            fs::write(
                directory.join("catalog.sqlite"),
                b"original retrieval-only closed catalog fixture",
            )?;
            fs::write(directory.join("lineage.json"), serde_json::to_vec(lineage)?)?;
            Ok(())
        },
        |_| Ok(()),
    )
}
struct Counted<'a> {
    source: &'a Snapshot,
    loads: AtomicUsize,
}
impl ArtifactSource for Counted<'_> {
    fn load_artifact(&self, path: &str) -> Result<Artifact> {
        self.loads.fetch_add(1, Ordering::Relaxed);
        self.source.load_artifact(path)
    }
}
fn queries() -> Vec<SearchQuery> {
    let mut queries = vec![];
    for text in ["aurora", "revised", "stable", "absenttoken"] {
        for prefix in [None, Some("files/000".into()), Some("files/0000[".into())] {
            for (limit, max_bytes) in [(1, 1), (3, 40), (100, 65536)] {
                queries.push(SearchQuery {
                    text: text.into(),
                    path_prefix: prefix.clone(),
                    limit,
                    max_bytes,
                });
            }
        }
    }
    queries
}

#[test]
fn sparse_edit_early_insert_and_chunk_count_shift_match_full_queries() -> Result<()> {
    let temporary = unlinked_tempdir()?;
    let base = temporary.path().join("base");
    let source = source(16);
    let (old, lineage, token) = inventory(&source, None)?;
    let receipt = full(&base, &source, &old, &lineage, token)?;
    let original_manifest = fs::read(
        base.join("generations")
            .join(token.revision.to_string())
            .join("manifest.json"),
    )?;
    for scenario in 0..3 {
        let mut next = source.clone();
        next.generation = 2;
        match scenario {
            0 => {
                next.artifacts.insert(
                    "files/00007.txt".into(),
                    artifact("files/00007.txt", 3, true),
                );
            }
            1 => {
                next.artifacts
                    .insert("aaa.txt".into(), artifact("aaa.txt", 2, true));
            }
            _ => {
                next.artifacts.insert(
                    "files/00000.txt".into(),
                    artifact("files/00000.txt", 5, true),
                );
                next.artifacts.remove("files/00015.txt");
            }
        }
        let (target, lineage, token) = inventory(&next, Some((&old, receipt.token)))?;
        let counted = Counted {
            source: &next,
            loads: AtomicUsize::new(0),
        };
        let prototype = lexical_experiment::prepare(ExperimentRequest {
            collection_directory: &base,
            base_receipt: &receipt,
            base_inventory: &old,
            inventory: &target,
            lineage: &lineage,
            target: token,
            source: &counted,
            staging_parent: temporary.path(),
            charge: Arc::new(|_| Ok(())),
        })?;
        let full_base = temporary.path().join(format!("full{scenario}"));
        let full_receipt = full(&full_base, &next, &target, &lineage, token)?;
        let reader = PersistentIndex::open_v2(&full_base, "default", &full_receipt)?;
        for query in queries() {
            assert_eq!(
                serde_json::to_value(prototype.search(&query)?)?,
                serde_json::to_value(reader.search(&query)?)?,
                "scenario {scenario}: {query:?}"
            );
        }
        assert_eq!(
            prototype.metrics.loaded_artifacts,
            counted.loads.load(Ordering::Relaxed)
        );
        if scenario == 0 {
            assert_eq!(prototype.metrics.loaded_artifacts, 1);
            assert!(prototype.metrics.input.ordinal_shifted.is_empty());
        } else {
            assert!(prototype.metrics.input.ordinal_shifted.len() >= 14);
        }
        assert!(prototype.metrics.charged_bytes >= prototype.metrics.copied_bytes);
        assert_eq!(
            fs::read(
                base.join("generations")
                    .join(receipt.token.revision.to_string())
                    .join("manifest.json")
            )?,
            original_manifest
        );
    }
    Ok(())
}

#[test]
fn copy_quota_failure_and_tampering_leave_base_readable_and_private_staging_clean() -> Result<()> {
    let temporary = unlinked_tempdir()?;
    let base = temporary.path().join("base");
    let staging = temporary.path().join("staging");
    fs::create_dir(&staging)?;
    let source = source(4);
    let (old, lineage, token) = inventory(&source, None)?;
    let receipt = full(&base, &source, &old, &lineage, token)?;
    let mut next = source.clone();
    next.generation = 2;
    next.artifacts.insert(
        "files/00000.txt".into(),
        artifact("files/00000.txt", 3, true),
    );
    let (target, lineage, token) = inventory(&next, Some((&old, receipt.token)))?;
    let request = |charge: Charge| ExperimentRequest {
        collection_directory: &base,
        base_receipt: &receipt,
        base_inventory: &old,
        inventory: &target,
        lineage: &lineage,
        target: token,
        source: &next,
        staging_parent: &staging,
        charge,
    };
    assert!(
        lexical_experiment::prepare(request(Arc::new(|bytes| {
            anyhow::ensure!(bytes == 0, "original quota refusal");
            Ok(())
        })))
        .is_err()
    );
    assert_eq!(fs::read_dir(&staging)?.count(), 0);
    let retained_directory = base
        .join("generations")
        .join(receipt.token.revision.to_string());
    let mut unsafe_staging = request(Arc::new(|_| Ok(())));
    unsafe_staging.staging_parent = &retained_directory;
    assert!(lexical_experiment::prepare(unsafe_staging).is_err());
    assert!(
        !PersistentIndex::open_v2(&base, "default", &receipt)?
            .search(&queries()[2])?
            .hits
            .is_empty()
    );
    let manifest_path = base
        .join("generations")
        .join(receipt.token.revision.to_string())
        .join("manifest.json");
    let original = fs::read(&manifest_path)?;
    fs::write(&manifest_path, b"{}")?;
    assert!(lexical_experiment::prepare(request(Arc::new(|_| Ok(())))).is_err());
    fs::write(manifest_path, original)?;
    let mut incompatible = target.clone();
    incompatible.extractor_version.push_str("-different");
    assert!(
        lexical_experiment::changed_input(&old, &incompatible, receipt.token, token)?
            .full_rebuild_reasons
            .contains(&"extractor")
    );
    Ok(())
}

#[test]
#[ignore = "explicit original-fixture timing campaign; no automatic strategy adoption"]
fn measure_sparse_and_ordinal_shift_workloads() -> Result<()> {
    for count in [16, 1000] {
        let temporary = unlinked_tempdir()?;
        let base = temporary.path().join("base");
        let source = source(count);
        let (old, lineage, token) = inventory(&source, None)?;
        let receipt = full(&base, &source, &old, &lineage, token)?;
        for scenario in ["sparse", "early_insert"] {
            for iteration in 0..3 {
                let mut next = source.clone();
                next.generation = 2;
                let path = if scenario == "sparse" {
                    format!("files/{:05}.txt", count / 2)
                } else {
                    "aaa.txt".into()
                };
                next.artifacts
                    .insert(path.clone(), artifact(&path, 3, true));
                let (target, lineage, token) = inventory(&next, Some((&old, receipt.token)))?;
                let started = Instant::now();
                let full_base = temporary
                    .path()
                    .join(format!("full-{scenario}-{iteration}"));
                let full_receipt = full(&full_base, &next, &target, &lineage, token)?;
                let full_micros = started.elapsed().as_micros();
                let prototype = lexical_experiment::prepare(ExperimentRequest {
                    collection_directory: &base,
                    base_receipt: &receipt,
                    base_inventory: &old,
                    inventory: &target,
                    lineage: &lineage,
                    target: token,
                    source: &next,
                    staging_parent: temporary.path(),
                    charge: Arc::new(|_| Ok(())),
                })?;
                let reader = PersistentIndex::open_v2(&full_base, "default", &full_receipt)?;
                for query in queries() {
                    assert_eq!(
                        serde_json::to_value(prototype.search(&query)?)?,
                        serde_json::to_value(reader.search(&query)?)?
                    );
                }
                println!(
                    "{}",
                    serde_json::json!({
                        "files":count,"scenario":scenario,"iteration":iteration,
                        "full_elapsed_micros":full_micros,
                        "incremental_elapsed_micros":prototype.metrics.elapsed_micros,
                        "copied_bytes":prototype.metrics.copied_bytes,
                        "charged_bytes":prototype.metrics.charged_bytes,
                        "output_bytes":prototype.metrics.output_bytes,
                        "loaded_artifacts":prototype.metrics.loaded_artifacts,
                        "ordinal_shifted_files":prototype.metrics.input.ordinal_shifted.len(),
                    })
                );
            }
        }
    }
    Ok(())
}
