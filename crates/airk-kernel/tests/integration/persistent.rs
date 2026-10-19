// SPDX-License-Identifier: Apache-2.0
use airk_core::SearchQuery;
use airk_kernel::Kernel;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs;

fn unlinked_tempdir() -> Result<tempfile::TempDir> {
    // Model object planning rejects symlinked ancestors such as macOS /var.
    Ok(tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?)
}

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 4096,
    }
}

fn model(data: &std::path::Path) -> Result<()> {
    let directory = data.join("models/local");
    fs::create_dir_all(&directory)?;
    // Tiny original fixture vectors; this is not a distributed pretrained model.
    let vocabulary = br#"{"authenticate":[1.0,0.0],"login":[0.99,0.01],"invoice":[0.0,1.0]}"#;
    fs::write(directory.join("vocabulary.json"), vocabulary)?;
    fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1, "name":"original-test-fixture", "license":"Apache-2.0", "dimensions":2,
            "sha256":format!("{:x}", Sha256::digest(vocabulary))
        }))?,
    )?;
    Ok(())
}

#[test]
fn persistent_lexical_and_real_disk_vector_candidates_keep_revision_identity() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    fs::write(root.path().join("auth.rs"), "fn authenticate() {}\n")?;
    fs::write(root.path().join("billing.rs"), "fn invoice() {}\n")?;
    model(data.path())?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    assert_eq!(kernel.status()?.lexical_backend, "tantivy");
    assert!(kernel.status()?.local_model_ready);
    assert_eq!(kernel.status()?.indexed_vectors, 2);
    assert!(kernel.search(&query("login"))?.hits.is_empty());
    let semantic = kernel.semantic_search(&query("login"))?;
    assert_eq!(semantic.hits[0].citation.path, "auth.rs");
    let hybrid = kernel.hybrid_search(&query("login"))?;
    assert_eq!(hybrid.generation, semantic.generation);
    assert_eq!(hybrid.hits[0].citation.path, "auth.rs");
    let mut budget = query("login");
    budget.max_bytes = 3;
    let bounded = kernel.hybrid_search(&budget)?;
    assert!(bounded.truncated);
    assert!(bounded.hits.iter().map(|hit| hit.text.len()).sum::<usize>() <= 3);
    assert!(
        kernel
            .semantic_search(&query("unknown_fixture_token"))
            .is_err()
    );
    let mut scoped = query("invoice");
    scoped.path_prefix = Some("auth".into());
    assert!(kernel.search(&scoped)?.hits.is_empty());
    fs::rename(root.path().join("auth.rs"), root.path().join("renamed.rs"))?;
    kernel.index()?;
    assert_eq!(
        kernel.semantic_search(&query("login"))?.hits[0]
            .citation
            .path,
        "renamed.rs"
    );
    // Invalid newly supplied assets cannot corrupt the already published model.
    fs::write(data.path().join("models/local/vocabulary.json"), b"{}")?;
    let generation = kernel.status()?.generation;
    assert!(kernel.reindex().is_err());
    assert_eq!(kernel.status()?.generation, generation);
    assert_eq!(
        kernel.semantic_search(&query("login"))?.hits[0]
            .citation
            .path,
        "renamed.rs"
    );
    Ok(())
}

#[test]
fn model_changes_advance_revision_and_reader_leases_protect_previous_components() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    fs::write(root.path().join("auth.rs"), "fn authenticate() {}\n")?;
    let writer = Kernel::open(root.path(), Some(data.path().into()))?;
    writer.index()?;
    let before = writer.status()?.generation;
    let reader = Kernel::open(root.path(), Some(data.path().into()))?;
    reader.search(&query("authenticate"))?;
    model(data.path())?;
    let report = writer.index()?;
    assert_eq!(
        report.indexed, 0,
        "model replacement reuses source extraction"
    );
    assert!(report.generation > before);
    assert!(writer.status()?.local_model_ready);
    assert_eq!(
        writer.semantic_search(&query("login"))?.hits[0]
            .citation
            .path,
        "auth.rs"
    );
    let protected = writer.prune_indexes(false)?;
    assert_eq!(protected.pinned.len(), 1);
    assert_eq!(protected.removed_bytes, 0);
    drop(reader);
    let unpinned = writer.prune_indexes(false)?;
    assert!(unpinned.removed_bytes > 0);
    assert!(writer.status()?.local_model_ready);
    Ok(())
}

#[test]
fn missing_model_and_missing_persistent_generation_fail_explicitly() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    fs::write(root.path().join("test.txt"), "needle\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    assert!(!kernel.status()?.local_model_ready);
    assert!(kernel.semantic_search(&query("needle")).is_err());
    let directory = data.path().join("collections/default");
    let store = airk_store::FileStore::new(&directory);
    let revision = store.persistent_revision()?.expect("component metadata");
    assert!(!kernel.search(&query("needle"))?.hits.is_empty());
    // Windows protects mapped index files from directory renames. The closed
    // manifest can still disappear: a cached reader must fail just like a new one.
    let generation = directory.join("generations").join(revision.revision);
    let manifest = generation.join("manifest.json");
    let hidden_manifest = directory.join("hidden-fixture-manifest.json");
    fs::rename(&manifest, &hidden_manifest)?;
    assert!(kernel.search(&query("needle")).is_err());
    assert!(kernel.status().is_err());
    fs::rename(hidden_manifest, manifest)?;
    drop(kernel); // Release mapped handles before testing a wholly missing directory.
    fs::rename(generation, directory.join("hidden-fixture-generation"))?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(
        kernel.search(&query("needle")).is_err(),
        "missing persistent files cannot silently use a memory fallback"
    );
    assert!(kernel.status().is_err());
    Ok(())
}

fn custom_model(data: &std::path::Path, vocabulary: &serde_json::Value) -> Result<()> {
    let directory = data.join("models/local");
    fs::create_dir_all(&directory)?;
    let bytes = serde_json::to_vec(vocabulary)?;
    fs::write(directory.join("vocabulary.json"), &bytes)?;
    fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1, "name":"original-retrieval-regression", "license":"Apache-2.0",
            "dimensions":2, "sha256":format!("{:x}", Sha256::digest(&bytes))
        }))?,
    )?;
    Ok(())
}

#[test]
fn warm_and_fresh_queries_reject_changed_receipt_bound_components() -> Result<()> {
    use airk_core::{
        model_objects::{ModelAssetName, ModelObjectSet},
        usage::RecordingMode,
    };
    use std::io::Write;
    for recording in [RecordingMode::Disabled, RecordingMode::AggregatesOnly] {
        let root = tempfile::tempdir()?;
        let data = unlinked_tempdir()?;
        model(data.path())?;
        fs::write(root.path().join("a.txt"), "authenticate")?;
        fs::write(root.path().join("b.txt"), "invoice")?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_usage(recording)?;
        kernel.index()?;
        let baseline = kernel.semantic_search(&query("login"))?;
        assert_eq!(baseline.hits[0].citation.path, "a.txt");
        kernel.search(&query("authenticate"))?;
        kernel.hybrid_search(&query("authenticate"))?;
        let publication = baseline.publication.unwrap();
        let collection = data.path().join("collections/default");
        let generation = collection
            .join("generations")
            .join(publication.revision.to_string());
        let graph_path = generation.join("vectors.bin");
        let graph = fs::read(&graph_path)?;
        let dimensions = u32::from_le_bytes(graph[8..12].try_into()?) as usize;
        let degree = u32::from_le_bytes(graph[12..16].try_into()?) as usize;
        let record_bytes = 4 * (dimensions + degree + 1) + 32;
        let mut transplanted = graph.clone();
        let first = 32 + record_bytes;
        let second = first + record_bytes;
        transplanted[first..second].copy_from_slice(&graph[second..second + record_bytes]);
        transplanted[second..second + record_bytes].copy_from_slice(&graph[first..second]);
        let mut incompatible = graph.clone();
        incompatible[12..16].copy_from_slice(&1u32.to_le_bytes());
        let sidecar_path = generation.join("model-objects.json");
        let sidecar = fs::read(&sidecar_path)?;
        let objects = ModelObjectSet::from_bytes(&sidecar)?;
        let vocabulary_path = collection.join("objects/models").join(
            objects.assets[&ModelAssetName::VocabularyJson]
                .sha256
                .to_string(),
        );
        let vocabulary = fs::read(&vocabulary_path)?;
        let mut changed_vocabulary = vocabulary.clone();
        let position = changed_vocabulary
            .iter()
            .position(|byte| *byte == b'1')
            .unwrap();
        changed_vocabulary[position] = b'2';
        let mut changed_sidecar = sidecar.clone();
        changed_sidecar[0] = b'[';
        let lexical_path = fs::read_dir(generation.join("lexical"))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "store")
            })
            .expect("stored lexical payload");
        let lexical = fs::read(&lexical_path)?;
        let mut changed_lexical = lexical.clone();
        changed_lexical[0] ^= 1;
        for (name, path, original, changed) in [
            ("transplanted records", &graph_path, &graph, &transplanted),
            (
                "incompatible graph header",
                &graph_path,
                &graph,
                &incompatible,
            ),
            (
                "changed model references",
                &sidecar_path,
                &sidecar,
                &changed_sidecar,
            ),
            (
                "changed model asset",
                &vocabulary_path,
                &vocabulary,
                &changed_vocabulary,
            ),
            (
                "same-size lexical replacement",
                &lexical_path,
                &lexical,
                &changed_lexical,
            ),
        ] {
            assert_eq!(original.len(), changed.len());
            // Same-size writes preserve mapped-file length on Windows.
            fs::OpenOptions::new()
                .write(true)
                .open(path)?
                .write_all(changed)?;
            assert!(
                kernel.search(&query("authenticate")).is_err(),
                "warm lexical: {name}"
            );
            assert!(
                kernel.semantic_search(&query("login")).is_err(),
                "warm vector: {name}"
            );
            assert!(
                kernel.hybrid_search(&query("authenticate")).is_err(),
                "warm hybrid: {name}"
            );
            let fresh = Kernel::open(root.path(), Some(data.path().into()))?;
            assert!(
                fresh.semantic_search(&query("login")).is_err(),
                "fresh vector: {name}"
            );
            fs::OpenOptions::new()
                .write(true)
                .open(path)?
                .write_all(original)?;
            let restored = kernel.semantic_search(&query("login"))?;
            assert_eq!(restored.publication.unwrap().token, publication.token);
            assert_eq!(restored.hits[0].citation.path, "a.txt");
        }
        let foreign = generation.join("unexpected-sidecar.json");
        fs::write(&foreign, b"{}")?;
        assert!(kernel.search(&query("authenticate")).is_err());
        assert!(kernel.semantic_search(&query("login")).is_err());
        assert!(kernel.hybrid_search(&query("authenticate")).is_err());
        fs::remove_file(foreign)?;
        assert_eq!(
            kernel
                .semantic_search(&query("login"))?
                .publication
                .unwrap()
                .token,
            publication.token
        );
    }
    Ok(())
}

#[test]
fn hybrid_fuses_candidates_beyond_payload_budget_and_preserves_padding_invariance() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    let mut vocabulary =
        serde_json::json!({"needle":[1.0,0.0],"weak":[0.0,1.0],"moderate":[0.5,0.5]});
    for index in 0..8 {
        vocabulary[format!("strong{index}")] = serde_json::json!([1.0, index as f32 * 0.001]);
    }
    for index in 0..128 {
        let token = format!("medium{index}");
        vocabulary[&token] = serde_json::json!([0.8, 0.6 + index as f32 * 0.0005]);
        fs::write(root.path().join(format!("z{index:03}.txt")), token)?;
    }
    custom_model(data.path(), &vocabulary)?;
    fs::write(root.path().join("m.txt"), "needle moderate")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let mut search = query("needle");
    search.limit = 1;
    for padded in [true, false] {
        for index in 0..8 {
            for (path, text) in [
                (format!("a{index}.txt"), "needle weak".to_owned()),
                (format!("b{index}.txt"), format!("strong{index}")),
            ] {
                fs::write(
                    root.path().join(path),
                    if padded { format!("{text:8192}") } else { text },
                )?;
            }
        }
        kernel.index()?;
        assert_eq!(
            kernel.hybrid_search(&search)?.hits[0].citation.path,
            "m.txt"
        );
    }
    search.max_bytes = 2;
    let tiny = kernel.hybrid_search(&search)?;
    assert_eq!(tiny.hits[0].citation.path, "m.txt");
    assert_eq!(tiny.hits[0].text, "ne");
    assert!(tiny.truncated);
    Ok(())
}

#[test]
fn hybrid_keeps_distinct_single_line_chunks_and_merges_each_identity_once() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    custom_model(data.path(), &serde_json::json!({"needle":[1.0,0.0]}))?;
    fs::write(
        root.path().join("long.txt"),
        format!("{:8192}needle SECOND", "needle FIRST"),
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let mut search = query("needle");
    search.max_bytes = 20_000;
    let lexical = kernel.search(&search)?;
    let hybrid = kernel.hybrid_search(&search)?;
    assert_eq!(lexical.hits.len(), 2);
    assert_eq!(hybrid.hits.len(), 2);
    assert_eq!(
        hybrid.hits[0].citation.start_line,
        hybrid.hits[1].citation.start_line
    );
    assert_eq!(
        hybrid.hits[0].citation.end_line,
        hybrid.hits[1].citation.end_line
    );
    assert!(hybrid.hits.iter().any(|hit| hit.text.contains("FIRST")));
    assert!(hybrid.hits.iter().any(|hit| hit.text.contains("SECOND")));
    assert!(!hybrid.truncated);
    // Repeated text is still distinct evidence when chunk ordinals differ.
    fs::write(
        root.path().join("long.txt"),
        format!("{0:8192}{0:8192}", "needle SAME"),
    )?;
    kernel.index()?;
    assert_eq!(kernel.hybrid_search(&search)?.hits.len(), 2);
    fs::write(root.path().join("long.txt"), "éé needle")?;
    kernel.index()?;
    search.max_bytes = 3;
    let bounded = kernel.hybrid_search(&search)?;
    assert_eq!(bounded.hits[0].text, "é");
    assert!(bounded.truncated);
    Ok(())
}

#[test]
fn scoped_vectors_find_sparse_prefixes_aliases_and_distinguish_empty_scope() -> Result<()> {
    let root = tempfile::tempdir()?;
    let alias = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    let mut vocabulary = serde_json::json!({"query":[1.0,0.0],"distant":[0.0,1.0]});
    for index in 0..450 {
        let token = format!("near{index}");
        vocabulary[&token] = serde_json::json!([1.0, index as f32 * 0.0001]);
        fs::write(root.path().join(format!("a{index:03}.txt")), token)?;
    }
    custom_model(data.path(), &vocabulary)?;
    fs::create_dir(root.path().join("wanted"))?;
    fs::write(root.path().join("wanted/evidence.txt"), "distant")?;
    fs::write(root.path().join("wanted/unknown.txt"), "unembedded")?;
    fs::write(alias.path().join("evidence.txt"), "distant")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.add_root("aliased", alias.path())?;
    kernel.index()?;
    let mut search = query("query");
    search.limit = 1;
    for prefix in ["wanted/", "@aliased/"] {
        search.path_prefix = Some(prefix.into());
        let scoped = kernel.semantic_search(&search)?;
        assert_eq!(scoped.hits.len(), 1);
        assert_eq!(
            scoped.hits[0].citation.path,
            format!("{prefix}evidence.txt")
        );
        assert!(!scoped.truncated, "all scoped vectors were compared");
        let hybrid = kernel.hybrid_search(&search)?;
        assert_eq!(
            hybrid.hits[0].citation.path,
            format!("{prefix}evidence.txt")
        );
        assert!(!hybrid.truncated);
    }
    for prefix in ["absent/", "wanted/unknown"] {
        search.path_prefix = Some(prefix.into());
        let empty = kernel.semantic_search(&search)?;
        assert!(empty.hits.is_empty());
        assert!(!empty.truncated, "empty scope was completely checked");
    }
    Ok(())
}

#[test]
fn usage_baseline_authenticates_only_its_catalog_and_reports_logical_read_costs() -> Result<()> {
    use airk_core::usage::{ClientFamily, RecordingMode, ResponseRepresentation, UsageOperation};
    use std::{collections::BTreeSet, io::Write};
    for files in [16, 256] {
        let root = tempfile::tempdir()?;
        let data = unlinked_tempdir()?;
        model(data.path())?;
        for index in 0..files {
            fs::write(
                root.path().join(format!("file{index:03}.txt")),
                format!("{:4096}", "authenticate"),
            )?;
        }
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.index()?;
        let response = kernel.search(&query("authenticate"))?;
        let publication = response.publication.unwrap();
        let sources: BTreeSet<_> = response
            .hits
            .iter()
            .map(|hit| {
                (
                    hit.citation.path.as_str(),
                    hit.citation.content_hash.as_str(),
                )
            })
            .collect();
        let store = airk_store::FileStore::new(data.path().join("collections/default"));
        let header = store.persistent_revision()?.unwrap();
        let generation = store
            .directory()
            .join("generations")
            .join(publication.revision.to_string());
        let manifest_path = generation.join("manifest.json");
        let catalog_path = generation.join("catalog.sqlite");
        let manifest_bytes = fs::read(&manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
        let expected_bytes = manifest_bytes.len() as u64 + fs::metadata(&catalog_path)?.len();
        let all_component_bytes = manifest_bytes.len() as u64
            + manifest["components"]
                .as_object()
                .unwrap()
                .values()
                .map(|value| value["bytes"].as_u64().unwrap())
                .sum::<u64>();
        assert!(expected_bytes < all_component_bytes);
        let mut timings = Vec::new();
        let mut first_call_us = 0;
        for sample in 0..11 {
            let (sizes, metrics) = store.usage_source_sizes_measured(
                &header.root,
                &header.collection,
                &publication.token,
                &sources,
            )?;
            assert!(
                matches!(sizes, airk_store::UsageSourceSizes::Available(ref values) if values.len() == sources.len())
            );
            assert_eq!(metrics.verification_bytes, expected_bytes);
            let elapsed = metrics.verification_us + metrics.lookup_us;
            if sample == 0 {
                first_call_us = elapsed;
            } else {
                timings.push(elapsed);
            }
        }
        timings.sort_unstable();
        eprintln!(
            "usage catalog fixture files={files} verification_bytes={expected_bytes} full_generation_component_bytes={all_component_bytes} first_call_us={first_call_us} repeated_p50_us={} repeated_p95_us={} samples=10 (OS cache not cleared; excludes model assets, ledger and retrieval)",
            timings[4], timings[9]
        );
        for recording in [RecordingMode::Disabled, RecordingMode::AggregatesOnly] {
            kernel.configure_usage(recording)?;
            let mut request_us = Vec::new();
            let mut retrieval_us = Vec::new();
            let mut finalization_us = Vec::new();
            for _ in 0..10 {
                let started = std::time::Instant::now();
                let span = kernel.usage_span(UsageOperation::Search, ClientFamily::Codex);
                let retrieve = std::time::Instant::now();
                let value = serde_json::to_value(kernel.search(&query("authenticate"))?)?;
                retrieval_us.push(retrieve.elapsed().as_micros());
                let bytes = serde_json::to_vec(&value)?.len();
                let finalize = std::time::Instant::now();
                kernel.finish_usage(
                    span,
                    Some(&value),
                    bytes,
                    false,
                    ResponseRepresentation::LogicalResultJson,
                );
                finalization_us.push(finalize.elapsed().as_micros());
                request_us.push(started.elapsed().as_micros());
            }
            request_us.sort_unstable();
            retrieval_us.sort_unstable();
            finalization_us.sort_unstable();
            eprintln!(
                "usage request fixture files={files} mode={recording:?} repeated_p50_us={} repeated_p95_us={} retrieval_serialization_p50_us={} baseline_ledger_p50_us={} samples=10 (OS cache not cleared; single worker)",
                request_us[4], request_us[9], retrieval_us[4], finalization_us[4]
            );
        }
        let catalog = fs::read(&catalog_path)?;
        let mut changed = catalog.clone();
        changed[0] ^= 1;
        fs::OpenOptions::new()
            .write(true)
            .open(&catalog_path)?
            .write_all(&changed)?;
        assert!(
            store
                .usage_source_sizes(
                    &header.root,
                    &header.collection,
                    &publication.token,
                    &sources
                )
                .is_err()
        );
        fs::OpenOptions::new()
            .write(true)
            .open(&catalog_path)?
            .write_all(&catalog)?;
        let sidecar = generation.join("catalog.sqlite-wal");
        fs::write(&sidecar, b"untrusted overlay")?;
        assert!(
            store
                .usage_source_sizes(
                    &header.root,
                    &header.collection,
                    &publication.token,
                    &sources
                )
                .is_err()
        );
        fs::remove_file(sidecar)?;
        // A baseline's counters remain authentic when an unrelated graph has
        // changed. Retrieval must independently reject that same changed graph.
        let graph_path = generation.join("vectors.bin");
        let mut graph = fs::read(&graph_path)?;
        graph[0] ^= 1;
        fs::write(&graph_path, graph)?;
        assert!(matches!(
            store.usage_source_sizes(
                &header.root,
                &header.collection,
                &publication.token,
                &sources
            )?,
            airk_store::UsageSourceSizes::Available(_)
        ));
        assert!(kernel.search(&query("authenticate")).is_err());
        assert!(kernel.semantic_search(&query("login")).is_err());
    }
    Ok(())
}
