// SPDX-License-Identifier: Apache-2.0
//! Original local word-vector fixtures; no ONNX or pretrained-model qualification.
use airk_core::{SearchQuery, embedding::CachedEmbedding};
use airk_kernel::Kernel;
use airk_store::{
    FileStore,
    retention::{RetentionMode, RetentionPolicy},
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn unlinked_tempdir() -> Result<tempfile::TempDir> {
    // macOS presents the default temporary directory through /var, a symlink.
    // Model source ancestors must be real directories on every platform.
    Ok(tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?)
}

fn model(data: &Path, name: &str) -> Result<()> {
    let directory = data.join("models/local");
    fs::create_dir_all(&directory)?;
    let vocabulary = br#"{"authenticate":[1.0,0.0],"login":[0.99,0.01],"invoice":[0.0,1.0]}"#;
    fs::write(directory.join("vocabulary.json"), vocabulary)?;
    fs::write(
        directory.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":name,"license":"Apache-2.0","dimensions":2,
            "sha256":format!("{:x}",Sha256::digest(vocabulary))
        }))?,
    )?;
    Ok(())
}
fn query() -> SearchQuery {
    SearchQuery {
        text: "login".into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 8192,
    }
}
fn manual(kernel: &Kernel) -> Result<()> {
    kernel.configure_retention(RetentionPolicy {
        mode: RetentionMode::Manual,
        ..Default::default()
    })?;
    Ok(())
}
fn references(store: &FileStore) -> Result<Vec<airk_store::vector_cache::VectorCacheReference>> {
    let catalog = store.catalog()?.context("catalog missing")?;
    store.embedding_cache_references(catalog.receipt.as_ref().context("receipt missing")?)
}

#[test]
fn exact_chunks_reuse_vectors_across_publications_moves_and_line_insertions() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "original embedding fixture")?;
    let original = "pub fn authenticate() -> u32 {\n    let original = 7;\n    original + 3\n}\n";
    fs::write(root.path().join("auth.rs"), original)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    manual(&kernel)?;
    let cold = kernel.index()?;
    assert!(cold.embedding_inferences > 0);
    assert_eq!(cold.embedding_cache_hits, 0);
    let cold_vectors = cold.embedding_inferences - cold.embedding_no_vector;
    assert!(cold_vectors > 0);
    fs::write(
        root.path().join("unrelated.txt"),
        "invoice original evidence\n",
    )?;
    let unrelated = kernel.index()?;
    assert!(unrelated.embedding_cache_hits >= cold_vectors);
    assert_eq!(
        unrelated.embedding_inferences - unrelated.embedding_no_vector,
        1,
        "only the new vector-producing text chunk requires inference; no-vector outcomes are retried truthfully"
    );
    fs::rename(root.path().join("auth.rs"), root.path().join("moved.rs"))?;
    let moved = kernel.index()?;
    assert_eq!(
        moved.embedding_inferences, moved.embedding_no_vector,
        "unchanged supported chunks have zero inference calls; unsupported chunks are not cached"
    );
    assert!(moved.embedding_cache_hits >= 2);
    let before = kernel.semantic_search(&query())?;
    assert_eq!(before.hits[0].citation.path, "moved.rs");
    fs::write(
        root.path().join("moved.rs"),
        format!("// inserted original comment\n\n{original}"),
    )?;
    let shifted = kernel.index()?;
    assert!(
        shifted.embedding_cache_hits >= 2,
        "unchanged declaration and unrelated text reuse exact vectors"
    );
    let optimized = kernel.semantic_search(&query())?;
    let moved_hit = optimized
        .hits
        .iter()
        .find(|hit| hit.citation.path == "moved.rs")
        .context("moved hit missing")?;
    assert!(moved_hit.citation.start_line > before.hits[0].citation.start_line);
    let full = kernel.reindex()?;
    assert_eq!(full.embedding_cache_hits, 0);
    assert!(full.embedding_inferences > cold.embedding_inferences);
    assert_eq!(
        serde_json::to_value(&kernel.semantic_search(&query())?.hits)?,
        serde_json::to_value(&optimized.hits)?,
        "full and optimized retrieval agree on payloads and rebound citations"
    );
    // A complete manifest change must invalidate vectors even with the same vocabulary.
    model(data.path(), "changed original descriptor")?;
    let changed_descriptor = kernel.index()?;
    assert!(
        changed_descriptor.generation > full.generation,
        "manifest-only descriptor change publishes new pipeline evidence"
    );
    assert_eq!(changed_descriptor.embedding_cache_hits, 0);
    assert!(changed_descriptor.embedding_inferences >= full.embedding_inferences);
    for (stage, report) in [
        ("cold", &cold),
        ("new_file", &unrelated),
        ("move", &moved),
        ("line_insert", &shifted),
        ("full", &full),
        ("manifest_only", &changed_descriptor),
    ] {
        eprintln!(
            "embedding {stage}: hits={} misses={} inferences={} no_vector={}",
            report.embedding_cache_hits,
            report.embedding_cache_misses,
            report.embedding_inferences,
            report.embedding_no_vector
        );
    }
    assert_eq!(
        fs::read_to_string(root.path().join("moved.rs"))?,
        format!("// inserted original comment\n\n{original}")
    );
    Ok(())
}

#[test]
fn forged_self_consistent_cache_entry_cannot_advance_publication() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "original corruption fixture")?;
    fs::write(
        root.path().join("auth.txt"),
        "authenticate original evidence\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    manual(&kernel)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let before = store
        .persistent_revision()?
        .context("publication missing")?;
    let reference = references(&store)?
        .into_iter()
        .next()
        .context("cache reference missing")?;
    let path = store
        .directory()
        .join("objects/embeddings")
        .join(reference.key.to_string());
    let original = fs::read(&path)?;
    let forged = CachedEmbedding::new(reference.key, vec![0.0, 1.0], 2)?;
    fs::write(&path, serde_json::to_vec(&forged)?)?;
    fs::write(root.path().join("extra.txt"), "invoice extra evidence\n")?;
    assert!(kernel.index().is_err());
    assert_eq!(
        store
            .persistent_revision()?
            .context("publication missing")?
            .token,
        before.token
    );
    fs::write(path, original)?;
    assert_eq!(
        kernel.semantic_search(&query())?.hits[0].citation.path,
        "auth.txt"
    );
    Ok(())
}

#[test]
fn manual_history_retains_vector_objects_and_automatic_gc_preserves_live_references() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "original GC fixture")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    manual(&kernel)?;
    fs::write(
        root.path().join("evidence.txt"),
        "authenticate original first\n",
    )?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let first = references(&store)?
        .into_iter()
        .next()
        .context("first vector missing")?;
    let first_path = store
        .directory()
        .join("objects/embeddings")
        .join(first.key.to_string());
    for text in ["invoice original second\n", "invoice original third\n"] {
        fs::write(root.path().join("evidence.txt"), text)?;
        kernel.index()?;
    }
    assert!(
        first_path.exists(),
        "manual history owns its retained vectors"
    );
    kernel.configure_retention(RetentionPolicy {
        mode: RetentionMode::Automatic,
        ..Default::default()
    })?;
    fs::write(
        root.path().join("evidence.txt"),
        "invoice original fourth\n",
    )?;
    kernel.index()?;
    for _ in 0..32 {
        let outcome = kernel.maintain_retention()?;
        if !outcome.receipt.pending {
            break;
        }
    }
    assert!(
        !first_path.exists(),
        "unreferenced vector is collected after obsolete owner retirement"
    );
    for reference in references(&store)? {
        assert!(
            store
                .directory()
                .join("objects/embeddings")
                .join(reference.key.to_string())
                .exists()
        );
    }
    assert!(!kernel.semantic_search(&query())?.hits.is_empty());
    assert_eq!(
        fs::read_to_string(root.path().join("evidence.txt"))?,
        "invoice original fourth\n"
    );
    Ok(())
}
