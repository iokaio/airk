// SPDX-License-Identifier: Apache-2.0
//! Current validation must disclose unavailable historical similarity evidence.
use airk_core::{changes::*, evidence::EvidenceReadOptions};
use airk_kernel::Kernel;
use anyhow::Result;
use std::fs;

#[test]
fn historical_suggestions_are_unavailable_without_redirecting_current_reads() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    fs::write(
        source.path().join("old.txt"),
        "original retained evidence for fictional invoice processing",
    )?;
    kernel.index()?;
    let baseline = kernel.status()?.publication.unwrap().token;
    fs::remove_file(source.path().join("old.txt"))?;
    fs::write(
        source.path().join("similar.txt"),
        "revised retained evidence for fictional invoice processing",
    )?;
    kernel.index()?;
    let mut request = ChangesRequest {
        since: Some(baseline),
        include_suggestions: true,
        selectors: vec![
            RememberedSelector::Path {
                path: "old.txt".into(),
            },
            RememberedSelector::Path {
                path: "similar.txt".into(),
            },
        ],
        limit: 1,
        ..Default::default()
    };
    let first = kernel.changes(&request)?;
    assert_eq!(first.baseline_status, BaselineStatus::BaselineUnavailable);
    assert_eq!(
        first.suggestion_coverage.unwrap().status,
        SuggestionStatus::Unavailable
    );
    assert!(first.suggestions.is_empty());
    assert_eq!(first.validations[0].outcome, ValidationOutcome::Unknown);
    request.cursor = first.cursor;
    assert_eq!(
        kernel.changes(&request)?.validations[0].outcome,
        ValidationOutcome::Current
    );
    request.include_suggestions = false;
    assert_eq!(
        kernel.changes(&request)?.status,
        ChangesStatus::CursorExpired
    );
    assert!(
        kernel
            .read_file_conditional(
                "old.txt",
                1,
                1,
                8192,
                &EvidenceReadOptions {
                    follow_moves: true,
                    ..Default::default()
                }
            )?
            .content
            .is_none()
    );
    Ok(())
}
