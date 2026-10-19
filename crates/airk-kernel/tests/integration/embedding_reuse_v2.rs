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
fn cache_manifest(store: &FileStore) -> Result<airk_store::vector_cache::VectorCacheManifest> {
    let catalog = store.catalog()?.context("catalog missing")?;
    store
        .embedding_cache_manifest(catalog.receipt.as_ref().context("receipt missing")?)?
        .context("cache manifest missing")
}

#[test]
fn exact_units_reuse_across_publications_and_moves_but_group_edits_recompute() -> Result<()> {
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
        "only the new vector-producing semantic input requires inference; no-vector outcomes are retried truthfully"
    );
    fs::rename(root.path().join("auth.rs"), root.path().join("moved.rs"))?;
    let moved = kernel.index()?;
    assert_eq!(
        moved.embedding_inferences, moved.embedding_no_vector,
        "unchanged supported inputs have zero inference calls; unsupported inputs are not cached"
    );
    assert!(moved.embedding_cache_hits >= 2);
    let before = kernel.semantic_search(&query())?;
    assert_eq!(before.hits[0].citation.path, "moved.rs");
    fs::write(
        root.path().join("moved.rs"),
        format!("// inserted original comment\n\n{original}"),
    )?;
    let shifted = kernel.index()?;
    assert_eq!(
        shifted.embedding_cache_hits, 1,
        "the unrelated input is unchanged; inserting text into a coalesced group changes that group's exact input"
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
    let reference = cache_manifest(&store)?
        .references
        .into_iter()
        .next()
        .context("cache reference missing")?;
    let path = store
        .directory()
        .join("objects/embedding-packs")
        .join(reference.pack.to_string());
    let original = fs::read(&path)?;
    let forged = CachedEmbedding::new(reference.key, vec![0.0, 1.0], 2)?;
    let mut forged_bytes = original.clone();
    let offset = (airk_store::vector_cache::PACK_HEADER_BYTES
        + u64::from(reference.record) * airk_store::vector_cache::record_bytes(2)?)
        as usize;
    forged_bytes[offset + 36..offset + 68].copy_from_slice(forged.checksum.as_bytes());
    for (index, value) in forged.vector.iter().enumerate() {
        forged_bytes[offset + 68 + index * 4..offset + 72 + index * 4]
            .copy_from_slice(&value.to_be_bytes());
    }
    fs::write(root.path().join("extra.txt"), "invoice extra evidence\n")?;
    let mut corrupt_index = original.clone();
    *corrupt_index.last_mut().context("empty pack")? ^= 1;
    for damaged in [forged_bytes, corrupt_index] {
        fs::write(&path, damaged)?;
        assert!(kernel.index().is_err());
        assert_eq!(
            store
                .persistent_revision()?
                .context("publication missing")?
                .token,
            before.token
        );
        // A reusable computation cache is not queried to answer the already
        // committed DiskANN index; corruption blocks its reuse, not this read.
        assert_eq!(
            kernel.semantic_search(&query())?.hits[0].citation.path,
            "auth.txt"
        );
        fs::write(&path, &original)?;
    }
    assert_eq!(
        kernel.semantic_search(&query())?.hits[0].citation.path,
        "auth.txt"
    );
    Ok(())
}

#[test]
fn relocated_collection_reuses_receipt_bound_packs_without_cross_collection_adoption() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let parent = unlinked_tempdir()?;
    let data = parent.path().join("original-data");
    model(&data, "original relocation fixture")?;
    fs::write(
        root.path().join("auth.txt"),
        "authenticate retained original evidence\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.clone()))?;
    manual(&kernel)?;
    let cold = kernel.index()?;
    assert_eq!(cold.embedding_inferences, 1);
    let store = FileStore::new(data.join("collections/default"));
    let before = cache_manifest(&store)?;
    let hits = serde_json::to_value(kernel.semantic_search(&query())?.hits)?;
    drop(kernel);
    let destination = parent.path().join("relocated-data");
    airk_kernel::storage::relocate(&data, &destination)?;
    let reopened = Kernel::open(root.path(), Some(destination.clone()))?;
    assert_eq!(
        serde_json::to_value(reopened.semantic_search(&query())?.hits)?,
        hits
    );
    fs::write(root.path().join("invoice.txt"), "invoice new evidence\n")?;
    let changed = reopened.index()?;
    assert_eq!(changed.embedding_cache_hits, 1);
    assert_eq!(changed.embedding_inferences, 1);
    let relocated = FileStore::new(destination.join("collections/default"));
    assert!(
        cache_manifest(&relocated)?
            .references
            .iter()
            .any(|item| item.key == before.references[0].key)
    );
    let other = Kernel::open_collection(root.path(), Some(destination.clone()), "other")?;
    // Byte-identical foreign objects carry no authority without this collection's receipt.
    let foreign = destination.join("collections/other/objects/embedding-packs");
    fs::create_dir_all(&foreign)?;
    for pack in &before.packs {
        fs::copy(
            airk_store::vector_cache::pack_path(relocated.directory(), &pack.sha256)?,
            foreign.join(pack.sha256.to_string()),
        )?;
    }
    let isolated = other.index()?;
    assert_eq!(isolated.embedding_cache_hits, 0);
    assert_eq!(isolated.embedding_inferences, 2);
    assert_eq!(
        fs::read_to_string(root.path().join("auth.txt"))?,
        "authenticate retained original evidence\n"
    );
    Ok(())
}

#[test]
fn sparse_pack_compaction_keeps_leased_bytes_until_the_old_reader_releases() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = unlinked_tempdir()?;
    model(data.path(), "original compaction fixture")?;
    for index in 0..4 {
        fs::write(
            root.path().join(format!("evidence-{index}.txt")),
            format!("authenticate original record {index}\n"),
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let reader = store.catalog()?.context("catalog missing")?;
    let before = store
        .embedding_cache_manifest(reader.receipt.as_ref().unwrap())?
        .unwrap();
    assert_eq!((before.packs.len(), before.references.len()), (1, 4));
    let original_pack =
        airk_store::vector_cache::pack_path(store.directory(), &before.packs[0].sha256)?;
    for index in 1..4 {
        fs::remove_file(root.path().join(format!("evidence-{index}.txt")))?;
    }
    let changed = kernel.index()?;
    assert_eq!(changed.embedding_cache_hits, 1);
    assert_eq!(changed.embedding_inferences, 0);
    assert_eq!(changed.embedding_cache_compacted_records, 1);
    assert_eq!(changed.embedding_cache_packs_written, 1);
    let after = cache_manifest(&store)?;
    assert_eq!(
        (
            after.packs.len(),
            after.references.len(),
            after.packs[0].records
        ),
        (1, 1, 1)
    );
    assert_ne!(after.packs[0].sha256, before.packs[0].sha256);
    assert!(original_pack.is_file());
    assert_eq!(
        kernel.semantic_search(&query())?.hits[0].citation.path,
        "evidence-0.txt"
    );
    drop(reader);
    for _ in 0..8 {
        let outcome = kernel.maintain_retention()?;
        if !outcome.receipt.pending {
            break;
        }
    }
    assert!(!original_pack.exists());
    assert!(
        airk_store::vector_cache::pack_path(store.directory(), &after.packs[0].sha256)?.is_file()
    );
    assert_eq!(
        fs::read_to_string(root.path().join("evidence-0.txt"))?,
        "authenticate original record 0\n"
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
    let first = cache_manifest(&store)?
        .packs
        .into_iter()
        .next()
        .context("first vector missing")?;
    let first_path = store
        .directory()
        .join("objects/embedding-packs")
        .join(first.sha256.to_string());
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
    for reference in cache_manifest(&store)?.packs {
        assert!(
            store
                .directory()
                .join("objects/embedding-packs")
                .join(reference.sha256.to_string())
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
