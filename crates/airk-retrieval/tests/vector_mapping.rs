// SPDX-License-Identifier: Apache-2.0
//! Original fixtures for independent semantic units and authenticated evidence maps.
mod support;
use airk_core::{
    Artifact, Chunk, DocumentLocation, SearchQuery, Snapshot,
    extraction::{LocalSpan, SemanticBoundary},
    identity::PublicationReceipt,
};
use airk_retrieval::persistent::{ComponentDigest, ManifestV2, PersistentIndex};
use anyhow::Result;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tantivy::{
    Index, IndexReader, TantivyDocument, collector::TopDocs, query::AllQuery, schema::Value,
};

fn model(root: &Path) -> Result<PathBuf> {
    let directory = root.join("model");
    fs::create_dir(&directory)?;
    let bytes = br#"{"needle":[1.0,0.0],"distant":[0.0,1.0]}"#;
    fs::write(directory.join("vocabulary.json"), bytes)?;
    fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
        "schema":1,"name":"original semantic mapping fixture","license":"Apache-2.0",
        "dimensions":2,"sha256":support::digest(bytes).to_string()}))?,
    )?;
    Ok(directory)
}

fn snapshot(path: &str, texts: &[&str]) -> Snapshot {
    let mut snapshot = Snapshot::empty("original-vector-mapping".into());
    snapshot.generation = 1;
    let mut line = 1;
    let chunks = texts
        .iter()
        .map(|text| {
            let start_line = line;
            let end_line = line
                + str::as_bytes(text)[..text.len().saturating_sub(1)]
                    .iter()
                    .filter(|b| **b == b'\n')
                    .count();
            line = end_line + usize::from(text.ends_with('\n'));
            Chunk {
                text: (*text).into(),
                start_line,
                end_line,
                location: DocumentLocation::Lines {
                    start: start_line,
                    end: end_line,
                },
            }
        })
        .collect();
    let text = texts.concat();
    snapshot.artifacts.insert(
        path.into(),
        Artifact {
            path: path.into(),
            content_hash: support::digest(text.as_bytes()).to_string(),
            bytes: text.len(),
            language: "text".into(),
            extraction_method: "utf8".into(),
            parse_status: "complete".into(),
            chunks,
            facts: vec![],
            retained: None,
        },
    );
    snapshot
}

fn directory(base: &Path, receipt: &PublicationReceipt) -> PathBuf {
    base.join("generations")
        .join(receipt.token.revision.to_string())
}

fn records(base: &Path, receipt: &PublicationReceipt) -> Result<Vec<serde_json::Value>> {
    let index = Index::open_in_dir(directory(base, receipt).join("vector-map"))?;
    let reader: IndexReader = index.reader()?;
    let field = index.schema().get_field("record")?;
    reader
        .searcher()
        .search(
            &AllQuery,
            &TopDocs::with_limit(1000).order_by_u64_field("map_ordinal", tantivy::Order::Asc),
        )?
        .into_iter()
        .map(|(_, address)| {
            let document: TantivyDocument = reader.searcher().doc(address)?;
            Ok(serde_json::from_slice(
                document.get_first(field).unwrap().as_bytes().unwrap(),
            )?)
        })
        .collect()
}

fn query(prefix: Option<&str>, bytes: usize) -> SearchQuery {
    SearchQuery {
        text: "needle".into(),
        path_prefix: prefix.map(str::to_owned),
        limit: 1,
        max_bytes: bytes,
    }
}

#[test]
fn long_chunk_tail_is_returned_by_semantic_and_hybrid_with_precise_lines() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let base = temp.path().join("collection");
    let text = format!("{}{}", "distant\n".repeat(512), "needle\n".repeat(256));
    let snapshot = snapshot("tail.txt", &[&text]);
    let source = support::FixtureSource::new(&snapshot)?;
    let receipt = support::prepare(&base, &snapshot, &source, Some(&model))?;
    let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    assert!(
        index.vectors > 1,
        "semantic windows are independent of display chunk count"
    );
    for response in [
        index.semantic_search(&query(None, 14))?,
        index.hybrid_search(&query(None, 14))?,
        index.semantic_search(&query(Some("tail"), 14))?,
    ] {
        assert_eq!(response.hits.len(), 1);
        assert!(response.hits[0].text.starts_with("needle\n"));
        assert_eq!(response.hits[0].citation.start_line, 513);
        assert!(response.truncated);
    }
    Ok(())
}

#[test]
fn coalesced_whitespace_member_does_not_consume_small_response_budget() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let base = temp.path().join("collection");
    let snapshot = snapshot(
        "functions.rs",
        &[
            "   \n",
            "fn first() { needle }\n",
            "fn second() { needle }\n",
        ],
    );
    let mut source = support::FixtureSource::new(&snapshot)?;
    let retained = source.artifacts.get_mut("functions.rs").unwrap();
    for boundary in &mut retained.payload.semantic_projection.boundaries {
        boundary.group = 0;
    }
    retained.payload_digest = support::digest(&serde_json::to_vec(&retained.payload)?);
    let receipt = support::prepare(&base, &snapshot, &source, Some(&model))?;
    let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    assert_eq!(index.vectors, 1);
    let response = index.semantic_search(&query(None, 8))?;
    assert!(response.hits[0].text.contains("needle"));
    assert!(response.hits[0].text.len() <= 8);
    assert_eq!(response.hits[0].citation.start_line, 2);
    assert_eq!(
        records(&base, &receipt)?[0]["members"]
            .as_array()
            .unwrap()
            .len(),
        3,
        "blank evidence stays in the authenticated body map"
    );
    Ok(())
}

#[test]
fn coalesced_late_sibling_remains_retrievable_and_limit_counts_bodies() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let base = temp.path().join("collection");
    let mut functions: Vec<_> = (0..9)
        .map(|index| format!("fn first_{index}() {{ distant }}\n"))
        .collect();
    functions.push("fn late_target() { needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle needle }\n".into());
    functions.extend((0..4).map(|index| format!("fn second_{index}() {{ needle distant }}\n")));
    let texts: Vec<_> = functions.iter().map(String::as_str).collect();
    let snapshot = snapshot("siblings.rs", &texts);
    let mut source = support::FixtureSource::new(&snapshot)?;
    let retained = source.artifacts.get_mut("siblings.rs").unwrap();
    for (index, boundary) in retained
        .payload
        .semantic_projection
        .boundaries
        .iter_mut()
        .enumerate()
    {
        boundary.group = u32::from(index >= 10);
    }
    retained.payload_digest = support::digest(&serde_json::to_vec(&retained.payload)?);
    let receipt = support::prepare(&base, &snapshot, &source, Some(&model))?;
    let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    assert_eq!(index.vectors, 2);
    let mut query = query(None, 4096);
    let top = index.semantic_search(&query)?;
    assert_eq!(top.hits.len(), 1);
    assert!(
        top.hits[0].text.contains("late_target"),
        "limit1 must cover the late member of the best semantic body"
    );
    query.limit = 2;
    for response in [index.semantic_search(&query)?, index.hybrid_search(&query)?] {
        assert_eq!(
            response.hits.len(),
            2,
            "member votes must not crowd another occurrence out"
        );
        assert!(
            response
                .hits
                .iter()
                .any(|hit| hit.text.contains("late_target"))
        );
        assert!(
            response
                .hits
                .iter()
                .any(|hit| hit.text.contains("second_3"))
        );
        assert!(!response.truncated);
    }
    Ok(())
}

#[test]
fn evidence_and_occurrence_ids_survive_unrelated_earlier_path_insertion() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let mut original = snapshot("z.txt", &["needle\n"]);
    let first = temp.path().join("first");
    let receipt = support::prepare(
        &first,
        &original,
        &support::FixtureSource::new(&original)?,
        Some(&model),
    )?;
    let before = records(&first, &receipt)?.remove(0);
    original
        .artifacts
        .extend(snapshot("a.txt", &["distant\n"]).artifacts);
    let second = temp.path().join("second");
    let receipt = support::prepare(
        &second,
        &original,
        &support::FixtureSource::new(&original)?,
        Some(&model),
    )?;
    let after = records(&second, &receipt)?
        .into_iter()
        .find(|record| record["path"] == "z.txt")
        .unwrap();
    assert_ne!(
        before["members"][0]["lexical_ordinal"],
        after["members"][0]["lexical_ordinal"]
    );
    assert_eq!(
        before["members"][0]["evidence_id"],
        after["members"][0]["evidence_id"]
    );
    assert_eq!(before["occurrence_id"], after["occurrence_id"]);
    Ok(())
}

#[test]
fn same_line_native_document_locations_remain_distinct() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let base = temp.path().join("collection");
    let locations = [
        DocumentLocation::Page { number: 1 },
        DocumentLocation::Page { number: 2 },
        DocumentLocation::Slide { number: 1 },
        DocumentLocation::Block { number: 1 },
        DocumentLocation::TableRow { table: 1, row: 1 },
        DocumentLocation::WorksheetRow {
            sheet: "Original".into(),
            row: 1,
        },
    ];
    let mut snapshot = snapshot("native.txt", &vec!["needle\n"; locations.len()]);
    for (chunk, location) in snapshot
        .artifacts
        .get_mut("native.txt")
        .unwrap()
        .chunks
        .iter_mut()
        .zip(&locations)
    {
        chunk.location = location.clone();
        chunk.start_line = 1;
        chunk.end_line = 1;
    }
    let mut source = support::FixtureSource::new(&snapshot)?;
    let retained = source.artifacts.get_mut("native.txt").unwrap();
    for boundary in &mut retained.payload.semantic_projection.boundaries {
        boundary.group = 0;
    }
    retained.payload_digest = support::digest(&serde_json::to_vec(&retained.payload)?);
    let receipt = support::prepare(&base, &snapshot, &source, Some(&model))?;
    let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    let mut query = query(None, 4096);
    query.limit = 10;
    for response in [index.semantic_search(&query)?, index.hybrid_search(&query)?] {
        assert_eq!(response.hits.len(), locations.len());
        for location in &locations {
            assert_eq!(
                response
                    .hits
                    .iter()
                    .filter(|hit| &hit.citation.location == location)
                    .count(),
                1
            );
        }
        assert!(!response.truncated);
    }
    Ok(())
}

#[test]
fn receipt_bound_manifest_counts_and_map_schema_are_checked() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let base = temp.path().join("collection");
    let snapshot = snapshot("counts.txt", &["needle\n"]);
    let source = support::FixtureSource::new(&snapshot)?;
    let receipt = support::prepare(&base, &snapshot, &source, Some(&model))?;
    let generation = directory(&base, &receipt);
    let original = fs::read(generation.join("manifest.json"))?;
    for (field, count) in [
        ("vectors", 2),
        ("vector_occurrences", 0),
        ("vector_occurrences", 2),
        ("vector_eligible_artifacts", 2),
        ("vector_excluded_artifacts", 1),
    ] {
        let mut manifest: serde_json::Value = serde_json::from_slice(&original)?;
        manifest[field] = count.into();
        let bytes = serde_json::to_vec(&manifest)?;
        fs::write(generation.join("manifest.json"), &bytes)?;
        let mut altered = receipt.clone();
        altered.manifest_digest = support::digest(&bytes);
        assert!(
            PersistentIndex::open_v2(&base, &snapshot.collection, &altered).is_err(),
            "{field}={count}"
        );
    }
    fs::write(generation.join("manifest.json"), original)?;
    // Authenticate a deliberately wrong map schema to test structural validation,
    // independently of the byte-digest corruption tests.
    fs::remove_dir_all(generation.join("vector-map"))?;
    fs::create_dir(generation.join("vector-map"))?;
    let mut schema = tantivy::schema::Schema::builder();
    schema.add_text_field("wrong", tantivy::schema::STRING);
    let index = Index::create_in_dir(generation.join("vector-map"), schema.build())?;
    let mut writer = index.writer_with_num_threads::<TantivyDocument>(1, 20_000_000)?;
    writer.commit()?;
    writer.wait_merging_threads()?;
    drop(index);
    let mut altered = receipt;
    rebind(&generation, &mut altered, 1)?;
    let error = PersistentIndex::open_v2(&base, &snapshot.collection, &altered)
        .err()
        .unwrap();
    assert!(error.to_string().contains("vector map schema"), "{error}");
    Ok(())
}

#[test]
fn nested_discontiguous_region_tail_keeps_original_host_location() -> Result<()> {
    mapped_body_fixture(false)
}

#[test]
fn whitespace_heavy_mapped_body_focus_and_unavailable_projection_keep_citations() -> Result<()> {
    mapped_body_fixture(true)
}

fn mapped_body_fixture(whitespace: bool) -> Result<()> {
    use airk_core::{
        extraction::{LocalChunk, LocalRegion},
        location::{ByteSpan, MapSegment, Mapping, RegionLocation, SourceMap},
    };
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let base = temp.path().join("collection");
    let first = if whitespace {
        " ".repeat(5000)
    } else {
        "distant\n".repeat(512)
    };
    let last = "needle\n".repeat(if whitespace { 1 } else { 256 });
    let local = format!("{first}{last}");
    let host = format!("prefix\n{first}original host gap\n{last}");
    let first_start = "prefix\n".len();
    let last_start = first_start + first.len() + "original host gap\n".len();
    let mut snapshot = snapshot("mapped.txt", &[&local]);
    let mut source = support::FixtureSource::new(&snapshot)?;
    let retained = source.artifacts.get_mut("mapped.txt").unwrap();
    retained.profile.contract_version = 2;
    let profile = retained.profile.clone();
    retained.payload.contract_version = 2;
    retained.payload.text = host.clone();
    let outer_pieces = vec![
        ByteSpan {
            start: first_start,
            end: first_start + first.len(),
        },
        ByteSpan {
            start: last_start,
            end: host.len(),
        },
    ];
    retained.payload.regions = vec![
        LocalRegion {
            id: 0,
            parent: None,
            profile: profile.clone(),
            text: local.clone(),
            source_map: SourceMap {
                version: 1,
                segments: vec![
                    MapSegment {
                        local: ByteSpan {
                            start: 0,
                            end: first.len(),
                        },
                        mapping: Mapping::Verbatim {
                            host: outer_pieces[0],
                        },
                    },
                    MapSegment {
                        local: ByteSpan {
                            start: first.len(),
                            end: local.len(),
                        },
                        mapping: Mapping::Verbatim {
                            host: outer_pieces[1],
                        },
                    },
                ],
            },
            location: RegionLocation::Embedded {
                region_id: "outer".into(),
                local_span: ByteSpan {
                    start: 0,
                    end: local.len(),
                },
                original_spans: outer_pieces,
            },
        },
        LocalRegion {
            id: 1,
            parent: Some(0),
            profile: profile.clone(),
            text: local.clone(),
            source_map: SourceMap {
                version: 1,
                segments: vec![MapSegment {
                    local: ByteSpan {
                        start: 0,
                        end: local.len(),
                    },
                    mapping: Mapping::Verbatim {
                        host: ByteSpan {
                            start: 0,
                            end: local.len(),
                        },
                    },
                }],
            },
            location: RegionLocation::Embedded {
                region_id: "inner".into(),
                local_span: ByteSpan {
                    start: 0,
                    end: local.len(),
                },
                original_spans: vec![ByteSpan {
                    start: 0,
                    end: local.len(),
                }],
            },
        },
    ];
    let lines = local.lines().count();
    retained.payload.chunks = vec![LocalChunk {
        region: Some(1),
        span: LocalSpan {
            start: 0,
            end: local.len(),
        },
        start_line: 1,
        end_line: lines,
        location: DocumentLocation::Lines {
            start: 1,
            end: lines,
        },
    }];
    retained.payload.semantic_projection.boundaries = vec![SemanticBoundary {
        region: Some(1),
        span: LocalSpan {
            start: 0,
            end: local.len(),
        },
        group: 0,
        context: None,
        split_context: None,
    }];
    retained.artifact = airk_core::binding::bind_payload(
        &retained.payload,
        &profile,
        airk_core::binding::ArtifactBinding {
            path: "mapped.txt",
            source_hash: support::digest(host.as_bytes()),
            source_bytes: host.len(),
        },
    )?;
    retained.payload_digest = support::digest(&serde_json::to_vec(&retained.payload)?);
    snapshot
        .artifacts
        .insert("mapped.txt".into(), retained.artifact.clone());
    let mut receipt = support::prepare(&base, &snapshot, &source, Some(&model))?;
    let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    let response = index.hybrid_search(&query(None, 14))?;
    assert!(response.hits[0].text.starts_with("needle\n"));
    let DocumentLocation::Mapped {
        region:
            RegionLocation::Embedded {
                region_id,
                original_spans,
                local_span,
            },
    } = &response.hits[0].citation.location
    else {
        panic!("mapped citation lost")
    };
    assert_eq!(region_id, "inner");
    assert_eq!(local_span.start, first.len());
    assert_eq!(
        original_spans,
        &[ByteSpan {
            start: last_start,
            end: (last_start + 14).min(host.len())
        }]
    );
    drop(index);
    if whitespace {
        let mut maps = records(&base, &receipt)?;
        assert_eq!(maps.len(), 1);
        let original = maps.clone();
        // An authenticated enclosing citation remains useful even where a compact
        // projection cannot subdivide an original decoded/unmapped edge.
        for segment in maps[0]["body"]["projection"]["segments"]
            .as_array_mut()
            .unwrap()
        {
            segment["mapping"] = serde_json::json!({"kind":"unmapped"});
        }
        let directory = directory(&base, &receipt);
        write_map_records(&directory, &maps)?;
        rebind(&directory, &mut receipt, 1)?;
        let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
        let response = index.semantic_search(&query(None, 14))?;
        assert_eq!(response.hits[0].text, "needle\n");
        assert!(response.truncated);
        assert_eq!(
            serde_json::to_value(&response.hits[0].citation.location)?,
            original[0]["body"]["location"]
        );
        drop(index);
        let mut malformed = original;
        let mut location: DocumentLocation =
            serde_json::from_value(malformed[0]["members"][0]["location"].clone())?;
        let DocumentLocation::Mapped {
            region: RegionLocation::Embedded { local_span, .. },
        } = &mut location
        else {
            panic!("fixture member lost embedded location")
        };
        local_span.end += 1;
        malformed[0]["members"][0]["location"] = serde_json::to_value(location)?;
        write_map_records(&directory, &malformed)?;
        rebind(&directory, &mut receipt, 1)?;
        let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
        let error = index.semantic_search(&query(None, 14)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("mapped evidence local span mismatch"),
            "{error}"
        );
    }
    Ok(())
}

fn write_map_records(directory: &Path, records: &[serde_json::Value]) -> Result<()> {
    let schema = Index::open_in_dir(directory.join("vector-map"))?.schema();
    fs::remove_dir_all(directory.join("vector-map"))?;
    fs::create_dir(directory.join("vector-map"))?;
    let map = Index::create_in_dir(directory.join("vector-map"), schema.clone())?;
    let mut writer = map.writer_with_num_threads::<TantivyDocument>(1, 20_000_000)?;
    for (ordinal, record) in records.iter().enumerate() {
        let mut document = TantivyDocument::default();
        document.add_u64(schema.get_field("graph_id")?, 1);
        document.add_u64(schema.get_field("map_ordinal")?, ordinal as u64);
        document.add_text(schema.get_field("path")?, record["path"].as_str().unwrap());
        document.add_bytes(schema.get_field("record")?, &serde_json::to_vec(record)?);
        writer.add_document(document)?;
    }
    writer.commit()?;
    writer.wait_merging_threads()?;
    drop(map);
    Ok(())
}

fn rebind(directory: &Path, receipt: &mut PublicationReceipt, vectors: usize) -> Result<()> {
    let mut manifest: ManifestV2 =
        serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    manifest.vectors = vectors;
    manifest.components = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(parent)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = path
                .strip_prefix(directory)?
                .to_str()
                .unwrap()
                .replace('\\', "/");
            if name == "manifest.json" {
                continue;
            }
            let bytes = fs::read(path)?;
            manifest.components.insert(
                name,
                ComponentDigest {
                    bytes: bytes.len() as u64,
                    sha256: support::digest(&bytes),
                },
            );
        }
    }
    let bytes = serde_json::to_vec(&manifest)?;
    fs::write(directory.join("manifest.json"), &bytes)?;
    receipt.manifest_digest = support::digest(&bytes);
    Ok(())
}

#[test]
fn shared_graph_occurrence_fanout_is_bounded_and_scope_filters_before_expansion() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let base = temp.path().join("collection");
    let mut snapshot = snapshot("a-outside.txt", &vec!["needle\n"; 129]);
    snapshot
        .artifacts
        .extend(self::snapshot("wanted/file.txt", &["needle\n"]).artifacts);
    let source = support::FixtureSource::new(&snapshot)?;
    let receipt = support::prepare(&base, &snapshot, &source, Some(&model))?;
    assert_eq!(
        records(&base, &receipt)?.len(),
        130,
        "persist every alias before bounded query expansion"
    );
    let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    assert_eq!(
        index.vectors, 1,
        "exact inputs share a production graph node"
    );
    assert!(index.semantic_search(&query(None, 100))?.truncated);
    let scoped = index.semantic_search(&query(Some("wanted/"), 100))?;
    assert_eq!(scoped.hits[0].citation.path, "wanted/file.txt");
    assert!(!scoped.truncated);
    Ok(())
}

#[derive(Default)]
struct FixtureCache {
    entries: std::sync::Mutex<
        BTreeMap<airk_core::identity::Sha256Digest, airk_core::embedding::CachedEmbedding>,
    >,
    lookups: std::sync::atomic::AtomicUsize,
}
impl airk_core::embedding::EmbeddingCache for FixtureCache {
    fn lookup(
        &self,
        key: &airk_core::identity::Sha256Digest,
    ) -> Result<Option<airk_core::embedding::CachedEmbedding>> {
        self.lookups
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.entries.lock().unwrap().get(key).cloned())
    }
    fn store(&self, entry: &airk_core::embedding::CachedEmbedding) -> Result<()> {
        self.entries
            .lock()
            .unwrap()
            .insert(entry.key, entry.clone());
        Ok(())
    }
}

#[test]
fn exact_inputs_reuse_across_batches_artifacts_and_full_rebuild_without_persisting_negative_cache()
-> Result<()> {
    use airk_retrieval::model::{EmbeddingCacheContext, EmbeddingCacheStats};
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let mut snapshot = snapshot("positive.txt", &vec!["needle\n"; 65]);
    snapshot
        .artifacts
        .extend(self::snapshot("unknown.txt", &vec!["unrecognized\n"; 65]).artifacts);
    snapshot
        .artifacts
        .extend(self::snapshot("alias/positive.txt", &["needle\n"]).artifacts);
    let source = support::FixtureSource::new(&snapshot)?;
    let cache = FixtureCache::default();
    for (run, bypass) in [false, false, true].into_iter().enumerate() {
        let base = temp.path().join(format!("run-{run}"));
        let mut stats = EmbeddingCacheStats::default();
        let before = cache.lookups.load(std::sync::atomic::Ordering::SeqCst);
        let receipt = support::prepare_options(
            &base,
            &snapshot,
            &source,
            Some(&model),
            Default::default(),
            &support::FixtureRegistry::default(),
            Some(&mut EmbeddingCacheContext {
                cache: Some(&cache),
                bypass,
                stats: &mut stats,
                max_workers: 1,
                working_memory_bytes: 128 << 20,
            }),
        )?;
        assert_eq!(stats.vector_units, 131);
        assert_eq!(stats.vector_reused_inputs, 129);
        assert_eq!(stats.vector_graph_nodes, 1);
        assert_eq!(stats.vector_eligible_artifacts, 3);
        assert_eq!(stats.vector_excluded_artifacts, 0);
        assert_eq!(
            stats.inputs, 2,
            "only distinct unseen inputs reach embedding/cache lookup"
        );
        assert_eq!(
            stats.no_vector, 1,
            "OOV is evaluated once per build, not cached persistently"
        );
        assert_eq!(stats.hits, u64::from(run == 1));
        assert_eq!(stats.inferences, if run == 1 { 1 } else { 2 });
        let lookups = cache.lookups.load(std::sync::atomic::Ordering::SeqCst) - before;
        assert_eq!(lookups, if bypass { 0 } else { 2 });
        assert_eq!(cache.entries.lock().unwrap().len(), 1);
        let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
        assert_eq!(index.vectors, 1);
        assert_eq!(records(&base, &receipt)?.len(), 66);
        assert_eq!(
            index.semantic_search(&query(Some("alias/"), 100))?.hits[0]
                .citation
                .path,
            "alias/positive.txt"
        );
    }
    Ok(())
}

#[test]
fn shared_node_survives_canonical_alias_delete_and_move() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let mut snapshot = snapshot("a-first.txt", &["needle\n"]);
    snapshot
        .artifacts
        .extend(self::snapshot("b-second.txt", &["needle\n"]).artifacts);
    snapshot
        .artifacts
        .extend(self::snapshot("wanted/third.txt", &["needle\n"]).artifacts);
    for state in 0..3 {
        if state == 1 {
            snapshot.artifacts.remove("a-first.txt");
        }
        if state == 2 {
            snapshot.artifacts.remove("wanted/third.txt");
            snapshot
                .artifacts
                .extend(self::snapshot("moved/third.txt", &["needle\n"]).artifacts);
        }
        let base = temp.path().join(format!("state-{state}"));
        let receipt = support::prepare(
            &base,
            &snapshot,
            &support::FixtureSource::new(&snapshot)?,
            Some(&model),
        )?;
        let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
        assert_eq!(index.vectors, 1);
        let mut all = query(None, 4096);
        all.limit = 10;
        for response in [index.semantic_search(&all)?, index.hybrid_search(&all)?] {
            assert_eq!(response.hits.len(), snapshot.artifacts.len());
            assert!(!response.truncated);
            assert!(
                response
                    .hits
                    .iter()
                    .all(|hit| snapshot.artifacts.contains_key(&hit.citation.path))
            );
        }
        let scope = if state == 2 { "moved/" } else { "wanted/" };
        assert!(
            index.semantic_search(&query(Some(scope), 100))?.hits[0]
                .citation
                .path
                .starts_with(scope)
        );
        if state == 2 {
            assert!(
                index
                    .semantic_search(&query(Some("wanted/"), 100))?
                    .hits
                    .is_empty()
            );
        }
    }
    Ok(())
}

#[test]
fn exact_bytes_written_context_and_pipeline_remain_separate_graph_inputs() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let snapshot = snapshot(
        "contexts.txt",
        &[
            "distant heading\n",
            "needle body\n",
            "needle heading\n",
            "needle body\n",
            "needle body ",
        ],
    );
    let mut source = support::FixtureSource::new(&snapshot)?;
    let retained = source.artifacts.get_mut("contexts.txt").unwrap();
    retained.payload.semantic_projection.boundaries[1].context =
        Some(retained.payload.chunks[0].span.clone());
    retained.payload.semantic_projection.boundaries[3].context =
        Some(retained.payload.chunks[2].span.clone());
    retained.payload_digest = support::digest(&serde_json::to_vec(&retained.payload)?);
    let first = temp.path().join("first");
    let receipt = support::prepare(&first, &snapshot, &source, Some(&model))?;
    let index = PersistentIndex::open_v2(&first, &snapshot.collection, &receipt)?;
    assert_eq!(index.vectors, 5);
    let before = records(&first, &receipt)?;
    assert_ne!(
        before[1]["embedding_key"], before[3]["embedding_key"],
        "written contexts distinguish equal bodies"
    );
    assert_ne!(
        before[3]["embedding_key"], before[4]["embedding_key"],
        "exact input bytes are not approximated"
    );
    let manifest_path = model.join("model.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    manifest["name"] = "different original pipeline fixture".into();
    fs::write(manifest_path, serde_json::to_vec(&manifest)?)?;
    let second = temp.path().join("second");
    let receipt = support::prepare(&second, &snapshot, &source, Some(&model))?;
    let after = records(&second, &receipt)?;
    assert_ne!(
        before[1]["embedding_key"], after[1]["embedding_key"],
        "complete pipeline identity binds exact inputs"
    );
    Ok(())
}

#[test]
fn vector_selection_preserves_lockfile_lexical_evidence_and_skips_its_planner() -> Result<()> {
    use airk_core::vector_selection::VectorSelectionPolicy;
    use airk_retrieval::model::{EmbeddingCacheContext, EmbeddingCacheStats};
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let mut snapshot = snapshot("Cargo.lock", &["needle dependency_version 42\n"]);
    snapshot
        .artifacts
        .extend(self::snapshot("source.txt", &["needle behavior\n"]).artifacts);
    let mut source = support::FixtureSource::new(&snapshot)?;
    let retained = source.artifacts.get_mut("Cargo.lock").unwrap();
    // This explicit test carrier is valid retained text but has no semantic
    // coverage. An eligible artifact would fail the planner's coverage check.
    retained.payload.semantic_projection.boundaries.clear();
    retained.payload_digest = support::digest(&serde_json::to_vec(&retained.payload)?);
    let policy = VectorSelectionPolicy::parse(
        br#"{"version":1,"exclusions":[{"id":"lockfiles","basename":"Cargo.lock"}]}"#,
    )?;
    let digest = policy.digest()?;
    let base = temp.path().join("selected");
    let mut stats = EmbeddingCacheStats::default();
    let receipt = support::prepare_options(
        &base,
        &snapshot,
        &source,
        Some(&model),
        policy.clone(),
        &support::FixtureRegistry::default(),
        Some(&mut EmbeddingCacheContext {
            cache: None,
            bypass: false,
            stats: &mut stats,
            max_workers: 1,
            working_memory_bytes: 128 << 20,
        }),
    )?;
    let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    assert_eq!(index.vector_selection_digest, digest);
    assert_eq!(
        (
            index.vector_eligible_artifacts,
            index.vector_excluded_artifacts
        ),
        (1, 1)
    );
    assert_eq!(stats.vector_units, 1);
    assert_eq!(
        (
            stats.vector_eligible_artifacts,
            stats.vector_excluded_artifacts
        ),
        (1, 1)
    );
    let mut exact = query(Some("Cargo.lock"), 4096);
    exact.text = "dependency_version 42".into();
    assert_eq!(
        index.search(&exact)?.hits[0].text,
        "needle dependency_version 42\n"
    );
    assert!(
        index
            .semantic_search(&query(Some("Cargo.lock"), 4096))?
            .hits
            .is_empty()
    );
    assert_eq!(
        index.hybrid_search(&query(Some("Cargo.lock"), 4096))?.hits[0]
            .citation
            .path,
        "Cargo.lock"
    );
    assert_eq!(records(&base, &receipt)?.len(), 1);
    let error =
        support::prepare(&temp.path().join("all"), &snapshot, &source, Some(&model)).unwrap_err();
    assert!(error.to_string().contains("semantic"), "{error}");
    // Applied policy is inspectable even when global vectors are disabled.
    let base = temp.path().join("model-disabled");
    let receipt = support::prepare_options(
        &base,
        &snapshot,
        &source,
        None,
        policy,
        &support::FixtureRegistry::default(),
        None,
    )?;
    let index = PersistentIndex::open_v2(&base, &snapshot.collection, &receipt)?;
    assert_eq!(index.vectors, 0);
    assert_eq!(
        (
            index.vector_eligible_artifacts,
            index.vector_excluded_artifacts
        ),
        (1, 1)
    );
    Ok(())
}

#[test]
fn registry_corruption_or_write_failure_never_publishes_a_partial_graph() -> Result<()> {
    use airk_core::{
        identity::Sha256Digest,
        vector_selection::{PreparedVectorNode, VectorNodeRegistry},
    };
    use airk_retrieval::model::{EmbeddingCacheContext, EmbeddingCacheStats};
    struct BrokenRegistry {
        corrupt: bool,
    }
    impl VectorNodeRegistry for BrokenRegistry {
        fn lookup(&self, _: Sha256Digest) -> Result<Option<PreparedVectorNode>> {
            Ok(self
                .corrupt
                .then_some(PreparedVectorNode { graph_id: Some(99) }))
        }
        fn remember(&self, _: Sha256Digest, _: PreparedVectorNode) -> Result<()> {
            anyhow::bail!("original fixture registry reservation denied")
        }
    }
    let temp = tempfile::tempdir()?;
    let model = model(temp.path())?;
    let snapshot = snapshot("original.txt", &["needle\n"]);
    let source = support::FixtureSource::new(&snapshot)?;
    for corrupt in [true, false] {
        let base = temp.path().join(format!("corrupt-{corrupt}"));
        let mut stats = EmbeddingCacheStats::default();
        let error = support::prepare_options(
            &base,
            &snapshot,
            &source,
            Some(&model),
            Default::default(),
            &BrokenRegistry { corrupt },
            Some(&mut EmbeddingCacheContext {
                cache: None,
                bypass: false,
                stats: &mut stats,
                max_workers: 1,
                working_memory_bytes: 128 << 20,
            }),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains(if corrupt {
                "outside this build's graph"
            } else {
                "reservation denied"
            }),
            "{error}"
        );
        assert_eq!(stats.inputs, u64::from(!corrupt));
        assert!(fs::read_dir(base.join("generations"))?.next().is_none());
    }
    Ok(())
}
