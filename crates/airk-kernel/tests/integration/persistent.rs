// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::read_current;
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
    assert_eq!(kernel.status()?.lexical_backend, "tantivy-current");
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
    writer.maintain_current()?;
    assert!(!reader.semantic_search(&query("login"))?.hits.is_empty());
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
    let directory = data.path().join("collections-1.2/default");
    let store = airk_store::FileStore::new(&directory);
    let read = read_current(&store)?;
    let lexical = directory.join(read.head().lexical_directory_name()?);
    drop(read);
    let meta = lexical.join("meta.json");
    let saved = fs::read(&meta)?;
    fs::remove_file(&meta)?;
    assert!(kernel.search(&query("needle")).is_err());
    fs::write(&meta, saved)?;
    assert!(!kernel.search(&query("needle"))?.hits.is_empty());
    fs::rename(lexical, directory.join("hidden-fixture-lexical"))?;
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(reopened.search(&query("needle")).is_err());
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
fn warm_and_fresh_queries_reject_changed_authenticated_components() -> Result<()> {
    use airk_core::model_objects::ModelAssetName;
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path())?;
    fs::write(root.path().join("a.txt"), "authenticate")?;
    fs::write(root.path().join("b.txt"), "invoice")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let before = kernel.semantic_search(&query("login"))?;
    let store = airk_store::FileStore::new(data.path().join("collections-1.2/default"));
    let read = read_current(&store)?;
    let model = read
        .collection_state()?
        .unwrap()
        .configuration
        .model
        .unwrap();
    let weights = store.directory().join("objects/models").join(
        model.assets[&ModelAssetName::VocabularyJson]
            .sha256
            .to_string(),
    );
    let lexical = store
        .directory()
        .join(read.head().lexical_directory_name()?)
        .join("meta.json");
    let source = store
        .directory()
        .join("objects/sources")
        .join(&read.artifact("a.txt")?.unwrap().content_hash);
    drop(read);
    for path in [&weights, &lexical, &source] {
        let saved = fs::read(path)?;
        let mut damaged = saved.clone();
        damaged[0] ^= 1;
        fs::write(path, damaged)?;
        let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
        assert!(kernel.hybrid_search(&query("authenticate")).is_err());
        assert!(reopened.hybrid_search(&query("authenticate")).is_err());
        fs::write(path, saved)?;
        assert_eq!(
            kernel
                .semantic_search(&query("login"))?
                .publication
                .unwrap()
                .token,
            before.publication.as_ref().unwrap().token
        );
    }
    let db = rusqlite::Connection::open(store.directory().join("current.sqlite3"))?;
    let record: Vec<u8> = db.query_row("SELECT record FROM graph_records WHERE id=1", [], |r| {
        r.get(0)
    })?;
    db.execute(
        "UPDATE graph_records SET record=(SELECT record FROM graph_records WHERE id=2) WHERE id=1",
        [],
    )?;
    assert!(kernel.semantic_search(&query("login")).is_err());
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert!(reopened.semantic_search(&query("login")).is_err());
    db.execute("UPDATE graph_records SET record=?1 WHERE id=1", [&record])?;
    assert_eq!(
        kernel.semantic_search(&query("login"))?.hits[0]
            .citation
            .path,
        "a.txt"
    );
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
fn hybrid_coalesces_members_but_keeps_distinct_single_line_bodies() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    custom_model(data.path(), &serde_json::json!({"needle":[1.0,0.0]}))?;
    let original = format!("{:8192}needle SECOND", "needle FIRST");
    fs::write(root.path().join("long.txt"), &original)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let mut search = query("needle");
    search.max_bytes = 30_000;
    let lexical = kernel.search(&search)?;
    let hybrid = kernel.hybrid_search(&search)?;
    assert_eq!(lexical.hits.len(), 2);
    assert_eq!(hybrid.hits.len(), 1);
    assert_eq!(hybrid.hits[0].text, original);
    assert_eq!(hybrid.hits[0].citation.start_line, 1);
    assert_eq!(hybrid.hits[0].citation.end_line, 1);
    assert!(!hybrid.truncated);
    // More than one semantic body on the same source line remains distinct.
    let repeated = format!("{0:8192}{0:8192}{0:8192}", "needle SAME");
    fs::write(root.path().join("long.txt"), &repeated)?;
    kernel.index()?;
    let hybrid = kernel.hybrid_search(&search)?;
    assert_eq!(hybrid.hits.len(), 2);
    assert!(
        hybrid
            .hits
            .iter()
            .all(|hit| hit.citation.start_line == 1 && hit.citation.end_line == 1)
    );
    assert_eq!(
        hybrid
            .hits
            .iter()
            .map(|hit| hit.text.as_str())
            .collect::<String>(),
        repeated
    );
    assert!(!hybrid.truncated);
    fs::write(root.path().join("long.txt"), "éé needle")?;
    kernel.index()?;
    search.max_bytes = 3;
    let bounded = kernel.hybrid_search(&search)?;
    assert_eq!(bounded.hits[0].text, "nee");
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
fn source_measurements_are_small_authenticated_values_independent_of_graph_bytes() -> Result<()> {
    use airk_store::current::map::MapDomain;
    for files in [16, 256] {
        let root = tempfile::tempdir()?;
        let data = unlinked_tempdir()?;
        for index in 0..files {
            fs::write(
                root.path().join(format!("file{index:03}.txt")),
                format!("{:4096}", "authenticate"),
            )?;
        }
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.index()?;
        let store = airk_store::FileStore::new(data.path().join("collections-1.2/default"));
        let read = read_current(&store)?;
        let bytes = read
            .map_get(MapDomain::SourceMeasurement, b"file000.txt")?
            .unwrap();
        assert!(bytes.len() < 1024);
        let size: airk_core::usage::SourceContextSize = serde_json::from_slice(&bytes)?;
        assert_eq!(
            size.source_hash,
            read.artifact("file000.txt")?.unwrap().content_hash
        );
        drop(read);
        let db = rusqlite::Connection::open(store.directory().join("current.sqlite3"))?;
        db.execute("UPDATE current_map_buckets SET content=x'5b5d'", [])?;
        assert!(
            read_current(&store)?
                .map_get(MapDomain::SourceMeasurement, b"file000.txt")
                .is_err()
        );
    }
    Ok(())
}
