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
