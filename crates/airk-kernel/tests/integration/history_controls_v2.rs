// SPDX-License-Identifier: Apache-2.0
//! The 1.2 boundary replaces historical reads with exact current-token checks.
use airk_core::evidence::{EvidenceReadOptions, EvidenceReadStatus};
use airk_kernel::{HistoryAvailabilityRequest, Kernel, history_controls::HistoryWorkBudget};
use anyhow::Result;
use std::fs;

#[test]
fn superseded_exact_file_context_info_outline_fail_without_live_fallback() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        source.path().join("original.rs"),
        "pub fn original() -> u32 { 17 }\n",
    )?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    kernel.index()?;
    let old = kernel.status()?.publication.unwrap().token;
    fs::write(
        source.path().join("original.rs"),
        "pub fn original() -> u32 { 29 }\n",
    )?;
    kernel.index()?;
    let current = kernel.status()?.publication.unwrap().token;
    fs::write(
        source.path().join("original.rs"),
        "LIVE_UNINDEXED_SENTINEL\n",
    )?;
    let request = EvidenceReadOptions {
        revision: Some(old),
        ..Default::default()
    };
    assert!(
        kernel
            .read_file_conditional("original.rs", 1, 1, 8192, &request)
            .is_err()
    );
    assert!(
        kernel
            .context_conditional("original.rs", 1, 0, 0, 8192, &request)
            .is_err()
    );
    assert!(
        kernel
            .file_info_conditional("original.rs", &request)
            .is_err()
    );
    assert!(
        kernel
            .outline_conditional("original.rs", 10, 8192, &request)
            .is_err()
    );
    let request = EvidenceReadOptions {
        revision: Some(current),
        ..Default::default()
    };
    let read = kernel.read_file_conditional("original.rs", 1, 1, 8192, &request)?;
    assert_eq!(read.read.status, EvidenceReadStatus::Content);
    assert!(!read.read.historical);
    assert!(read.content.unwrap().hits[0].text.contains("29"));
    assert!(
        kernel
            .file_info_conditional("original.rs", &request)?
            .content
            .is_some()
    );
    assert!(
        !kernel
            .outline_conditional("original.rs", 10, 8192, &request)?
            .content
            .unwrap()
            .facts
            .is_empty()
    );
    assert_eq!(
        fs::read_to_string(source.path().join("original.rs"))?,
        "LIVE_UNINDEXED_SENTINEL\n"
    );
    Ok(())
}

#[test]
fn historical_controls_are_explicitly_unsupported_and_do_not_publish() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    kernel.initialize()?;
    let before = kernel.status()?.publication.unwrap().token;
    assert!(
        kernel
            .history_policy()
            .unwrap_err()
            .to_string()
            .contains("unsupported")
    );
    assert!(kernel.configure_history(Default::default()).is_err());
    assert!(kernel.checkpoints().is_err());
    assert!(
        kernel
            .maintain_history(HistoryWorkBudget::default())
            .is_err()
    );
    assert!(
        kernel
            .history_availability(&HistoryAvailabilityRequest {
                revision: Some(before),
                cursor: None,
                limit: 10,
                max_bytes: 8192
            })
            .is_err()
    );
    assert!(kernel.configure_retention(Default::default()).is_err());
    assert!(kernel.maintain_retention().is_err());
    assert!(kernel.prune_indexes(false).is_err());
    assert_eq!(kernel.status()?.publication.unwrap().token, before);
    assert!(
        !data
            .path()
            .join("collections-1.2/default/generations")
            .exists()
    );
    Ok(())
}
