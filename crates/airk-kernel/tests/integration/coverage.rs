// SPDX-License-Identifier: Apache-2.0
use airk_ingest::inventory::InventoryLimits;
use airk_kernel::Kernel;
use std::{fs, time::Duration};

#[test]
fn coverage_tracks_disk_hashes_removals_and_unknown_cache_without_indexing() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("a.custom"), "first fixture")?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    assert!(kernel.coverage(false)?.percent.is_none());
    let unindexed = kernel.coverage(true)?;
    assert!(!unindexed.indexed);
    assert_eq!(
        (unindexed.eligible, unindexed.missing, unindexed.current),
        (1, 1, 0)
    );
    kernel.index()?;
    assert!(
        !kernel.coverage(false)?.scan_complete,
        "Publication invalidates prior-generation coverage"
    );
    let indexed = kernel.coverage(true)?;
    assert_eq!(indexed.percent, Some(100));
    assert_eq!(
        kernel.coverage(false)?.inventory_revision,
        indexed.inventory_revision
    );
    fs::write(root.path().join("a.custom"), "second fixture")?;
    fs::write(root.path().join("b.rs"), "fn new_fixture() {}")?;
    let changed = kernel.coverage(true)?;
    assert_eq!((changed.stale, changed.missing, changed.current), (1, 1, 0));
    fs::remove_file(root.path().join("a.custom"))?;
    assert_eq!(kernel.coverage(true)?.pending_removals, 1);
    kernel.index()?;
    assert_eq!(kernel.coverage(true)?.percent, Some(100));
    Ok(())
}

#[test]
fn eligibility_respects_shared_ignores_limits_documents_and_binary_fallback() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let state = root.path().join("separate-index-state");
    fs::write(root.path().join(".airkignore"), "ignored/\n.airkignore\n")?;
    fs::create_dir(root.path().join("ignored"))?;
    fs::write(root.path().join("ignored/private.txt"), "private fixture")?;
    fs::write(root.path().join(".env"), "secret fixture")?;
    fs::write(root.path().join("fallback.strange"), "searchable fixture")?;
    fs::write(
        root.path().join("drawing.svg"),
        "<svg><title>fixture</title></svg>",
    )?;
    fs::write(root.path().join("binary.dat"), [0, 255, 159])?;
    for filename in ["broken.pdf", "broken.docx", "broken.pptx", "broken.xlsx"] {
        fs::write(root.path().join(filename), [0, 255, 159])?;
    }
    fs::File::create(root.path().join("huge.txt"))?
        .set_len((airk_ingest::MAX_FILE_BYTES + 1) as u64)?;
    let kernel = Kernel::open(root.path(), Some(state))?;
    kernel.index()?;
    let report = kernel.coverage(true)?;
    assert!(report.scan_complete);
    assert_eq!(report.eligible, 6);
    assert_eq!(report.current, 2);
    assert_eq!(report.extraction_failed, 4);
    assert_eq!(report.unsupported, 1);
    assert_eq!(report.excluded, 1);
    assert_eq!(report.percent, Some(33));
    assert_eq!(
        kernel.coverage(true)?.percent,
        Some(33),
        "cached extraction classifications remain accurate"
    );
    Ok(())
}

#[test]
fn empty_incomplete_and_capacity_limited_inventories_never_claim_full_coverage()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.index()?;
    let empty = kernel.coverage(true)?;
    assert!(empty.scan_complete);
    assert_eq!((empty.eligible, empty.percent), (0, None));
    fs::write(root.path().join("one.txt"), "fixture")?;
    let partial = kernel.coverage_with_limits(
        true,
        InventoryLimits {
            candidates: 0,
            read_bytes: 1024,
            duration: Duration::from_secs(1),
        },
    )?;
    assert!(!partial.scan_complete);
    assert!(partial.budget_exhausted);
    assert_eq!(partial.percent, None);
    // Oversized but individually eligible sources must stay in the denominator.
    kernel.configure_scan_limits(airk_ingest::ScanLimits {
        corpus_bytes: 64 * 1024 * 1024,
        ..kernel.scan_limits()?
    })?;
    let mut bytes = vec![b'x'; 1_048_576];
    bytes[..2].copy_from_slice(b"//");
    for index in 0..65 {
        fs::write(root.path().join(format!("{index}.rs")), &bytes)?;
    }
    assert!(kernel.index().is_err());
    let blocked = kernel.coverage_with_limits(
        true,
        InventoryLimits {
            duration: Duration::from_secs(120),
            ..InventoryLimits::default()
        },
    )?;
    assert!(blocked.scan_complete);
    assert_eq!(blocked.eligible, 66);
    assert_eq!(blocked.capacity_blocked, 66);
    assert_eq!(blocked.percent, Some(0));
    assert_eq!(
        kernel.status()?.files,
        0,
        "Failed publication retains prior evidence"
    );
    Ok(())
}

#[test]
fn coverage_cache_is_bound_to_root_collection_and_tolerates_torn_data() -> anyhow::Result<()> {
    let first = tempfile::tempdir()?;
    let second = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(first.path().join("one.txt"), "fixture")?;
    let kernel = Kernel::open(first.path(), Some(state.path().into()))?;
    kernel.coverage(true)?;
    let other = Kernel::open(second.path(), Some(state.path().into()))?;
    assert_eq!(other.coverage(false)?.inventory_revision, 0);
    fs::write(
        state.path().join("collections/default/coverage.json"),
        "{torn",
    )?;
    assert_eq!(kernel.coverage(false)?.percent, None);
    assert!(kernel.coverage(true)?.scan_complete);
    Ok(())
}
