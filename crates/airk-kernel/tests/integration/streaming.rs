// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
use airk_core::Snapshot;
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
    let directory = data.path().join("collections-1.2/default");
    let store = FileStore::new(&directory);
    let catalog = store.current_test_catalog()?.unwrap();
    assert_eq!(catalog.snapshot.storage_format, 120);
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
    assert!(!serde_json::to_string(&catalog.snapshot)?.contains("retained_source_marker"));
    assert!(
        store
            .load_retained_bounded(&catalog.snapshot.artifacts["a.rs"], 16)
            .is_err()
    );
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
fn quota_failure_preserves_published_evidence_and_orphans_are_reclaimed() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.rs"), "fn previous_fact() {}\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    // This fixture exercises explicit orphan reclamation after a manual failure.
    kernel.index()?;
    let generation = kernel.status()?.generation;
    let quota = Quota::open(data.path())?;
    let before = quota.status()?;
    let mut policy = QuotaPolicy::default();
    let collection_limit = before.collections["default"].used_bytes
        + before.collections["default"].reserved_bytes
        + 131_072;
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
    assert!(after.active_reservations >= before.active_reservations);
    assert!(after.collections["default"].used_bytes <= collection_limit);
    kernel.maintain_current()?;
    quota.configure(QuotaPolicy::default())?;
    kernel.index()?;
    assert!(kernel.status()?.generation > generation);
    Ok(())
}

#[test]
fn obsolete_unbound_json_snapshot_is_rejected_without_source_changes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.rs"), "fn migrated_fact() {}\n")?;
    let previous = Snapshot::empty(root.path().canonicalize()?.to_str().unwrap().into());
    let (legacy, _) = airk_ingest::scan(root.path(), data.path(), &previous)?;
    let directory = data.path().join("collections-1.2/default");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join("snapshot.json"),
        serde_json::to_vec(&legacy)?,
    )?;
    assert!(Kernel::open(root.path(), Some(data.path().into())).is_err());
    assert!(directory.join("snapshot.json").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("a.rs"))?,
        "fn migrated_fact() {}\n"
    );
    Ok(())
}
