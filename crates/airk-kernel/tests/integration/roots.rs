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
            // Exercise the shared entry budget, allowing a bounded read at the
            // current per-file ceiling before the second root exhausts entries.
            read_bytes: 2 * airk_ingest::MAX_FILE_BYTES,
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

#[test]
fn current_changes_verification_keeps_aliased_and_primary_source_locations_distinct() -> Result<()>
{
    use airk_core::{
        changes::{ChangesRequest, RememberedSelector},
        verification::{SourceCheckState, VerificationBudget, VerificationMode},
    };
    let primary = tempfile::tempdir()?;
    let documents = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        primary.path().join("requirements.md"),
        "Primary interface\n",
    )?;
    let document = "REQ-42: The external document retains its original source.\n";
    fs::write(documents.path().join("requirements.md"), document)?;
    let kernel =
        Kernel::open_collection(primary.path(), Some(data.path().into()), "changes-roots")?;
    kernel.configure_vectors(false)?;
    kernel.add_root("docs", documents.path())?;
    kernel.index()?;
    let request = ChangesRequest {
        selectors: vec![RememberedSelector::Path {
            path: "@docs/requirements.md".into(),
        }],
        ..Default::default()
    };
    let value = serde_json::to_value(kernel.changes(&request)?)?;
    let budget = VerificationBudget {
        mode: VerificationMode::Require,
        max_sources: 4,
        max_bytes: 4096,
        deadline_ms: 2000,
    };
    let receipt = kernel.verify_returned_sources(&value, budget)?;
    assert_eq!(receipt.status, "verified", "{receipt:?}");
    assert_eq!(receipt.counts.references, Some(1));
    assert_eq!(receipt.counts.bytes_read, document.len() as u64);
    assert_eq!(receipt.sources.len(), 1);
    let checked = &receipt.sources[0];
    assert_eq!(checked.path.as_deref(), Some("@docs/requirements.md"));
    assert_eq!(
        checked.source_relative_path.as_deref(),
        Some("requirements.md")
    );
    assert_eq!(
        checked.original_root.as_deref(),
        Some(documents.path().canonicalize()?.to_str().unwrap())
    );
    assert_eq!(checked.state, SourceCheckState::Matched);
    assert_eq!(checked.expected_hash, checked.observed_hash);
    let mut forged = value;
    forged["validations"][0]["resolved_path"] = serde_json::json!("requirements.md");
    let refused = kernel.verify_returned_sources(&forged, budget)?;
    assert_eq!(refused.status, "requirements_unmet");
    assert_eq!(refused.counts.bytes_read, 0);
    assert_eq!(refused.sources[0].state, SourceCheckState::Unsupported);
    assert_eq!(
        fs::read_to_string(documents.path().join("requirements.md"))?,
        document
    );
    assert_eq!(
        fs::read_to_string(primary.path().join("requirements.md"))?,
        "Primary interface\n"
    );
    Ok(())
}
