// SPDX-License-Identifier: Apache-2.0
//! Original local word-vector fixtures; no ONNX or pretrained-model qualification.
use super::current_inspection::read_current;
use airk_core::SearchQuery;
use airk_kernel::Kernel;
use airk_store::FileStore;
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
#[test]
fn exact_units_reuse_across_publications_and_moves_but_group_edits_recompute() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "original embedding fixture")?;
    let original = "pub fn authenticate() -> u32 {\n    let original = 7;\n    original + 3\n}\n";
    fs::write(root.path().join("auth.rs"), original)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
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
    assert_eq!(
        unrelated.embedding_cache_hits, 0,
        "unchanged artifacts do no embedding-cache work"
    );
    assert_eq!(
        unrelated.embedding_inferences - unrelated.embedding_no_vector,
        1,
        "only the new vector-producing semantic input requires inference; no-vector outcomes are retried truthfully"
    );
    fs::rename(root.path().join("auth.rs"), root.path().join("moved.rs"))?;
    let moved = kernel.index()?;
    assert_eq!(
        moved.embedding_inferences, moved.embedding_no_vector,
        "unchanged supported inputs have zero inference calls; unsupported inputs are not cached"
    );
    assert_eq!(
        moved.embedding_cache_hits, 0,
        "exact graph reuse needs no persistent cache lookup"
    );
    assert!(moved.vector_reused_inputs > 0);
    let before = kernel.semantic_search(&query())?;
    assert_eq!(before.hits[0].citation.path, "moved.rs");
    fs::write(
        root.path().join("moved.rs"),
        format!("// inserted original comment\n\n{original}"),
    )?;
    let shifted = kernel.index()?;
    assert_eq!(
        shifted.embedding_cache_hits, 0,
        "unchanged files do no cache work; edited groups require new exact inputs"
    );
    assert_eq!(
        shifted.embedding_inferences - shifted.embedding_no_vector,
        1
    );
    let optimized = kernel.semantic_search(&query())?;
    let moved_hit = optimized
        .hits
        .iter()
        .find(|hit| hit.citation.path == "moved.rs" && hit.text.contains("pub fn authenticate"))
        .context("moved hit missing")?;
    let declaration_line = |hit: &airk_core::Hit| -> Result<usize> {
        let offset = hit
            .text
            .find("pub fn authenticate")
            .context("declaration missing")?;
        Ok(hit.citation.start_line
            + hit.text[..offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count())
    };
    assert_eq!(
        declaration_line(moved_hit)?,
        declaration_line(&before.hits[0])? + 2
    );
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
fn current_cache_manifest_is_authenticated_and_foreign_packs_are_not_adopted() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "original cache ownership")?;
    fs::write(
        root.path().join("auth.txt"),
        "authenticate original evidence",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(kernel.index()?.embedding_inferences, 1);
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let read = read_current(&store)?;
    let before = read.token()?;
    let reference = read
        .resource_reference("embedding-cache")?
        .context("current cache")?;
    let manifest = airk_store::vector_cache::VectorCacheManifest::from_bytes(
        &read.resource(&store, "embedding-cache")?.unwrap(),
    )?;
    drop(read);
    let path = store
        .directory()
        .join("current-resources")
        .join(format!("{}.json", reference.digest));
    let saved = fs::read(&path)?;
    fs::write(&path, vec![b'x'; saved.len()])?;
    fs::write(root.path().join("invoice.txt"), "invoice extra evidence")?;
    assert!(kernel.index().is_err());
    assert_eq!(read_current(&store)?.token()?, before);
    assert_eq!(
        kernel.semantic_search(&query())?.hits[0].citation.path,
        "auth.txt"
    );
    fs::write(path, saved)?;
    let other = Kernel::open_collection(root.path(), Some(data.path().into()), "other")?;
    let foreign = data
        .path()
        .join("collections-1.2/other/objects/embedding-packs");
    fs::create_dir_all(&foreign)?;
    for pack in manifest.packs {
        fs::copy(
            airk_store::vector_cache::pack_path(store.directory(), &pack.sha256)?,
            foreign.join(pack.sha256.to_string()),
        )?;
    }
    let isolated = other.index()?;
    assert_eq!(isolated.embedding_cache_hits, 0);
    assert_eq!(isolated.embedding_inferences, 2);
    kernel.index()?;
    Ok(())
}

#[test]
fn relocation_keeps_exact_graph_reuse_and_reclaims_unreferenced_cache_packs() -> Result<()> {
    let root = tempfile::tempdir()?;
    let parent = unlinked_tempdir()?;
    let data = parent.path().join("original");
    model(&data, "original relocation fixture")?;
    fs::write(
        root.path().join("auth.txt"),
        "authenticate original retained evidence",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.clone()))?;
    kernel.index()?;
    let before = serde_json::to_value(kernel.semantic_search(&query())?.hits)?;
    let destination = parent.path().join("relocated");
    airk_kernel::storage::relocate(&data, &destination)?;
    let reopened = Kernel::open(root.path(), Some(destination.clone()))?;
    assert_eq!(
        serde_json::to_value(reopened.semantic_search(&query())?.hits)?,
        before
    );
    fs::rename(root.path().join("auth.txt"), root.path().join("moved.txt"))?;
    let report = reopened.index()?;
    assert_eq!(report.embedding_inferences, 0);
    assert_eq!(report.vector_reused_inputs, 1);
    assert_eq!(
        reopened.semantic_search(&query())?.hits[0].citation.path,
        "moved.txt"
    );
    assert_eq!(
        fs::read_dir(destination.join("collections-1.2/default/objects/embedding-packs"))?.count(),
        0
    );
    Ok(())
}
