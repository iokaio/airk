// SPDX-License-Identifier: Apache-2.0
use airk_core::{ArtifactSource, Snapshot};
use airk_kernel::Kernel;
use airk_store::{
    FileStore,
    quota::{Quota, QuotaPolicy},
};
use anyhow::Result;
use std::fs;

#[test]
fn catalogs_are_compact_sources_deduplicate_and_file_reads_are_lazy() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let text = "fn retained_source_marker() {}\n";
    fs::write(root.path().join("a.rs"), text)?;
    fs::write(root.path().join("b.rs"), text)?;
    fs::write(root.path().join("c.rs"), "fn distinct_source_marker() {}\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let directory = data.path().join("collections/default");
    let store = FileStore::new(&directory);
    let catalog = store.catalog()?.unwrap();
    assert_eq!(catalog.snapshot.storage_format, 2);
    assert_eq!(
        catalog.snapshot.artifacts["a.rs"]
            .retained
            .as_ref()
            .unwrap()
            .object,
        catalog.snapshot.artifacts["b.rs"]
            .retained
            .as_ref()
            .unwrap()
            .object,
        "path-free payloads share identical native output"
    );
    assert!(
        catalog
            .snapshot
            .artifacts
            .values()
            .all(|a| a.chunks.is_empty() && a.facts.is_empty() && a.retained.is_some())
    );
    assert_eq!(fs::read_dir(directory.join("objects/sources"))?.count(), 2);
    let header = store.persistent_revision()?.unwrap();
    let inventory = fs::read_to_string(
        directory
            .join("generations")
            .join(&header.revision)
            .join("inventory.json"),
    )?;
    assert!(!inventory.contains("retained_source_marker"));
    assert!(store.catalog_bounded(16).is_err());
    assert!(catalog.load_artifact_bounded("a.rs", 16).is_err());
    assert_eq!(
        store.read_retained_source(&catalog.snapshot.artifacts["a.rs"])?,
        text.as_bytes()
    );
    // Corruption of an unrelated object proves per-file reads do not deserialize
    // the entire retained corpus. The requested object still fails closed.
    let bad = &catalog.snapshot.artifacts["c.rs"]
        .retained
        .as_ref()
        .unwrap()
        .object;
    fs::write(directory.join("objects/artifacts").join(bad), b"corrupt")?;
    assert!(
        kernel.read_file("a.rs", 1, 1, 4096)?.hits[0]
            .text
            .contains("retained_source_marker")
    );
    assert!(kernel.read_file("c.rs", 1, 1, 4096).is_err());
    assert_eq!(kernel.list_files("", 10, 4096)?.paths.len(), 3);
    assert!(
        kernel
            .outline("a.rs", 10, 4096)?
            .facts
            .iter()
            .any(|f| f.name == "retained_source_marker")
    );
    Ok(())
}

#[test]
fn old_catalog_leases_keep_content_objects_until_last_reader_leaves() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.txt"), "old evidence\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let directory = data.path().join("collections/default");
    let store = FileStore::new(&directory);
    let old = store.catalog()?.unwrap();
    let old_object = old.snapshot.artifacts["a.txt"]
        .retained
        .as_ref()
        .unwrap()
        .object
        .clone();
    fs::write(root.path().join("a.txt"), "new evidence\n")?;
    kernel.index()?;
    let protected = kernel.prune_indexes(false)?;
    assert_eq!(protected.pinned.len(), 1);
    assert_eq!(old.load_artifact("a.txt")?.chunks[0].text, "old evidence\n");
    assert!(
        directory
            .join("objects/artifacts")
            .join(&old_object)
            .exists()
    );
    drop(old);
    let preview = kernel.prune_indexes(true)?;
    assert_eq!(preview.unreferenced_objects, 2);
    let applied = kernel.prune_indexes(false)?;
    assert_eq!(applied.unreferenced_objects, preview.unreferenced_objects);
    assert_eq!(applied.removed_bytes, preview.reclaimable_bytes);
    assert!(
        !directory
            .join("objects/artifacts")
            .join(old_object)
            .exists()
    );
    assert_eq!(
        kernel.read_file("a.txt", 1, 1, 4096)?.hits[0].text,
        "new evidence\n"
    );
    Ok(())
}

#[test]
fn catalog_digest_binds_paths_and_source_alias_metadata() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.txt"), "original\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let directory = data.path().join("collections/default");
    let store = FileStore::new(&directory);
    let header = store.persistent_revision()?.unwrap();
    let path = directory
        .join("generations")
        .join(header.revision)
        .join("inventory.json");
    let mut inventory: airk_core::publication::InventoryV2 =
        serde_json::from_slice(&fs::read(&path)?)?;
    inventory
        .roots
        .insert("altered".into(), root.path().display().to_string());
    fs::write(path, serde_json::to_vec(&inventory)?)?;
    assert!(store.catalog().is_err());
    assert!(kernel.file_info("a.txt").is_err());
    Ok(())
}

#[test]
fn quota_failure_preserves_published_evidence_and_orphans_are_reclaimed() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.rs"), "fn previous_fact() {}\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let generation = kernel.status()?.generation;
    let quota = Quota::open(data.path())?;
    let before = quota.status()?;
    let mut policy = QuotaPolicy::default();
    let collection_limit = before.collections["default"].used_bytes + 131_072;
    policy
        .collections
        .insert("default".into(), collection_limit);
    quota.configure(policy)?;
    fs::write(root.path().join("a.rs"), "fn unpublished_fact() {}\n")?;
    assert!(kernel.index().is_err());
    assert_eq!(kernel.status()?.generation, generation);
    assert!(
        kernel.read_file("a.rs", 1, 1, 4096)?.hits[0]
            .text
            .contains("previous_fact")
    );
    assert!(
        kernel
            .outline("a.rs", 10, 4096)?
            .facts
            .iter()
            .any(|f| f.name == "previous_fact")
    );
    let after = quota.status()?;
    assert_eq!(after.active_reservations, 0);
    assert!(after.collections["default"].used_bytes <= collection_limit);
    let reclaimed = kernel.prune_indexes(false)?;
    assert!(reclaimed.unreferenced_objects >= 2);
    quota.configure(QuotaPolicy::default())?;
    kernel.index()?;
    assert!(kernel.status()?.generation > generation);
    Ok(())
}

#[test]
fn legacy_snapshot_migrates_to_verified_objects_without_losing_citations() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.rs"), "fn migrated_fact() {}\n")?;
    let previous = Snapshot::empty(root.path().canonicalize()?.to_str().unwrap().into());
    let (legacy, _) = airk_ingest::scan(root.path(), data.path(), &previous)?;
    let directory = data.path().join("collections/default");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join("snapshot.json"),
        serde_json::to_vec(&legacy)?,
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let before = kernel.read_file("a.rs", 1, 1, 4096)?;
    kernel.index()?;
    let after = kernel.read_file("a.rs", 1, 1, 4096)?;
    assert_eq!(
        before.hits[0].citation.content_hash,
        after.hits[0].citation.content_hash
    );
    assert_eq!(before.hits[0].text, after.hits[0].text);
    assert!(after.generation > before.generation);
    assert_eq!(
        FileStore::new(directory)
            .catalog()?
            .unwrap()
            .snapshot
            .storage_format,
        2
    );
    Ok(())
}
