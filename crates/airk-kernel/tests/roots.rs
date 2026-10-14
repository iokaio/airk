// SPDX-License-Identifier: Apache-2.0
use airk_core::SearchQuery;
use airk_ingest::inventory::InventoryLimits;
use airk_kernel::Kernel;
use anyhow::Result;
use std::{fs, time::Duration};

#[test]
fn additional_roots_preserve_original_locations_and_published_bindings() -> Result<()> {
    let primary = tempfile::tempdir()?;
    let documents = tempfile::tempdir()?;
    let replacement = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        primary.path().join("requirements.md"),
        "Primary interface\n",
    )?;
    fs::write(
        documents.path().join("requirements.md"),
        "REQ-42: Backup shall finish within 30 seconds.\n",
    )?;
    fs::write(
        replacement.path().join("requirements.md"),
        "Replacement interface\n",
    )?;
    let kernel = Kernel::open_collection(primary.path(), Some(data.path().into()), "roots-test")?;
    kernel.index()?;
    assert_eq!(kernel.coverage(true)?.percent, Some(100));
    assert!(kernel.add_root("docs", documents.path())?.pending_index);
    assert!(kernel.coverage(false)?.percent.is_none());
    let pending = kernel.coverage(true)?;
    assert_eq!(
        (pending.eligible, pending.current, pending.missing),
        (2, 1, 1)
    );
    assert!(pending.source_configuration_pending && pending.percent.is_none());
    kernel.index()?;
    let generation = kernel.status()?.generation;
    assert_eq!(kernel.status()?.files, 2);
    assert_eq!(kernel.coverage(true)?.percent, Some(100));
    let query = SearchQuery {
        text: "Backup".into(),
        path_prefix: Some("@docs/".into()),
        limit: 10,
        max_bytes: 4096,
    };
    assert_eq!(
        kernel.search(&query)?.hits[0].citation.path,
        "@docs/requirements.md"
    );
    let original = kernel.source_location("@docs/requirements.md")?;
    assert_eq!(original.relative_path, "requirements.md");
    assert_eq!(
        original.root,
        documents.path().canonicalize()?.to_str().unwrap()
    );
    kernel.remove_root("docs")?;
    kernel.add_root("docs", replacement.path())?;
    // Config changes cannot silently point already published evidence at the replacement.
    assert_eq!(
        kernel.source_location("@docs/requirements.md")?.root,
        original.root
    );
    assert_eq!(kernel.search(&query)?.generation, generation);
    kernel.index()?;
    assert!(kernel.search(&query)?.hits.is_empty());
    assert_eq!(
        kernel.source_location("@docs/requirements.md")?.root,
        replacement.path().canonicalize()?.to_str().unwrap()
    );
    kernel.remove_root("docs")?;
    assert_eq!(kernel.coverage(true)?.pending_removals, 1);
    kernel.index()?;
    assert!(kernel.source_location("@docs/requirements.md").is_err());
    assert_eq!(kernel.status()?.files, 1);
    assert_eq!(
        fs::read_to_string(documents.path().join("requirements.md"))?,
        "REQ-42: Backup shall finish within 30 seconds.\n"
    );
    Ok(())
}

#[test]
fn multi_root_coverage_shares_budgets_and_failed_scan_retains_previous_evidence() -> Result<()> {
    let primary = tempfile::tempdir()?;
    let documents = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(primary.path().join("main.txt"), "primary fixture")?;
    fs::write(documents.path().join("other.txt"), "other fixture")?;
    let kernel = Kernel::open(primary.path(), Some(data.path().into()))?;
    kernel.add_root("docs", documents.path())?;
    kernel.index()?;
    let generation = kernel.status()?.generation;
    let report = kernel.coverage_with_limits(
        true,
        InventoryLimits {
            candidates: 3,
            read_bytes: 4 * 1024 * 1024,
            duration: Duration::from_secs(5),
        },
    )?;
    assert!(!report.scan_complete && report.budget_exhausted);
    assert_eq!(report.eligible, 1);
    assert!(report.percent.is_none());
    // A new primary-root path cannot shadow an alias from a registered external source.
    fs::write(primary.path().join("@docs"), "namespace collision")?;
    assert!(kernel.index().is_err());
    assert_eq!(kernel.status()?.generation, generation);
    assert_eq!(
        kernel.source_location("@docs/other.txt")?.relative_path,
        "other.txt"
    );
    Ok(())
}
