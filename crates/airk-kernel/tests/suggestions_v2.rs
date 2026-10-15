// SPDX-License-Identifier: Apache-2.0
//! Original synthetic fixtures qualify hints, not semantic identity or precision.
use airk_core::{
    changes::{ChangeSuggestion, ChangesRequest, ChangesStatus, SuggestionStatus},
    evidence::{EvidenceReadOptions, EvidenceReadStatus},
    identity::RevisionToken,
};
use airk_kernel::{
    Kernel,
    history_controls::{CreateCheckpoint, HistoryPolicyV1, HistoryWorkBudget},
};
use airk_store::{
    FileStore,
    retention::{RetentionMode, RetentionPolicy},
};
use anyhow::{Context, Result};
use std::fs;

struct Fixture {
    source: tempfile::TempDir,
    _data: tempfile::TempDir,
    kernel: Kernel,
    store: FileStore,
    baseline: RevisionToken,
}
fn original() -> String {
    (0..40)
        .map(|i| format!("billing{i} records original invoice entry {i}.\n"))
        .collect()
}
impl Fixture {
    fn new() -> Result<Self> {
        let source = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        fs::create_dir(source.path().join("old"))?;
        fs::write(source.path().join("old/original.txt"), original())?;
        let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
        kernel.configure_retention(RetentionPolicy {
            mode: RetentionMode::Manual,
            ..Default::default()
        })?;
        kernel.index()?;
        let store = FileStore::new(data.path().join("collections/default"));
        let baseline = store
            .catalog()?
            .context("catalog")?
            .receipt
            .as_ref()
            .unwrap()
            .token;
        fs::create_dir(source.path().join("new"))?;
        fs::remove_file(source.path().join("old/original.txt"))?;
        fs::write(
            source.path().join("new/original.txt"),
            original().replace("entry 17.", "entry revised-seventeen."),
        )?;
        kernel.index()?;
        Ok(Self {
            source,
            _data: data,
            kernel,
            store,
            baseline,
        })
    }
    fn request(&self) -> ChangesRequest {
        ChangesRequest {
            since: Some(self.baseline),
            include_suggestions: true,
            limit: 100,
            max_bytes: 65536,
            ..Default::default()
        }
    }
}

#[test]
fn opt_in_hints_share_record_budgets_and_never_forward_reads() -> Result<()> {
    let fixture = Fixture::new()?;
    let default = fixture.kernel.changes(&ChangesRequest {
        include_suggestions: false,
        ..fixture.request()
    })?;
    assert!(default.suggestions.is_empty() && default.suggestion_coverage.is_none());
    let mut request = ChangesRequest {
        limit: 1,
        ..fixture.request()
    };
    let mut suggestions = vec![];
    loop {
        let response = fixture.kernel.changes(&request)?;
        assert!(
            response.changes.len()
                + response.invalidations.len()
                + response.validations.len()
                + response.suggestions.len()
                <= 1
        );
        let encoded = response
            .changes
            .iter()
            .map(serde_json::to_vec)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            + response
                .invalidations
                .iter()
                .map(serde_json::to_vec)
                .collect::<std::result::Result<Vec<_>, _>>()?
                .iter()
                .map(Vec::len)
                .sum::<usize>()
            + response
                .suggestions
                .iter()
                .map(serde_json::to_vec)
                .collect::<std::result::Result<Vec<_>, _>>()?
                .iter()
                .map(Vec::len)
                .sum::<usize>();
        assert_eq!(response.record_bytes, encoded);
        assert!(encoded <= request.max_bytes);
        assert_eq!(
            response.suggestion_coverage.unwrap().status,
            SuggestionStatus::Complete
        );
        suggestions.extend(response.suggestions);
        request.cursor = response.cursor;
        if request.cursor.is_none() {
            break;
        }
    }
    assert_eq!(suggestions.len(), 1);
    let ChangeSuggestion::File {
        before,
        after,
        score_basis_points,
        ..
    } = &suggestions[0]
    else {
        panic!("file hint")
    };
    assert_eq!(before.path, "old/original.txt");
    assert_eq!(after.path, "new/original.txt");
    assert!(*score_basis_points > 9000);
    let read = fixture.kernel.read_file_conditional(
        "old/original.txt",
        1,
        2,
        8192,
        &EvidenceReadOptions {
            follow_moves: true,
            ..Default::default()
        },
    )?;
    assert_ne!(read.read.status, EvidenceReadStatus::Content);
    assert!(read.content.is_none());
    let scoped = fixture.kernel.changes(&ChangesRequest {
        path_prefix: "old/".into(),
        ..fixture.request()
    })?;
    assert!(scoped.suggestions.is_empty());
    // Live replacement is irrelevant to retained comparisons.
    fs::write(
        fixture.source.path().join("new/original.txt"),
        "UNINDEXED LIVE SOURCE",
    )?;
    assert_eq!(
        fixture
            .kernel
            .changes(&fixture.request())?
            .suggestions
            .len(),
        1
    );
    Ok(())
}

#[test]
fn cursors_bind_opt_in_and_retained_baseline_representation() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut request = ChangesRequest {
        limit: 1,
        ..fixture.request()
    };
    request.cursor = fixture.kernel.changes(&request)?.cursor;
    assert!(request.cursor.is_some());
    assert!(
        fixture
            .kernel
            .changes(&ChangesRequest {
                include_suggestions: false,
                ..request.clone()
            })
            .is_err()
    );
    fixture.kernel.prune_indexes(false)?;
    assert_eq!(
        fixture.kernel.changes(&request)?.status,
        ChangesStatus::CursorExpired
    );
    let fresh = fixture.kernel.changes(&fixture.request())?;
    assert!(fresh.suggestions.is_empty());
    assert_eq!(
        fresh.suggestion_coverage.unwrap().status,
        SuggestionStatus::Unavailable
    );
    Ok(())
}

#[test]
fn exact_retained_history_supplies_original_bytes_without_live_fallback() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.kernel.configure_history(HistoryPolicyV1 {
        enabled: true,
        recent: 0,
        hourly_hours: 0,
        ..Default::default()
    })?;
    fixture.kernel.create_checkpoint(
        &CreateCheckpoint {
            label: "original suggestion baseline".into(),
            revision: fixture.baseline,
        },
        HistoryWorkBudget::default(),
    )?;
    fs::write(
        fixture.source.path().join("unrelated.txt"),
        "unrelated current evidence",
    )?;
    fixture.kernel.index()?;
    for _ in 0..8 {
        fixture
            .kernel
            .maintain_history(HistoryWorkBudget::default())?;
        if fixture
            .store
            .history_at(&fixture.baseline, 134217728)?
            .is_some()
        {
            break;
        }
    }
    assert!(
        fixture
            .store
            .history_at(&fixture.baseline, 134217728)?
            .is_some()
    );
    assert!(
        fixture
            .store
            .catalog_at(&fixture.baseline, 134217728)?
            .is_none()
    );
    let response = fixture.kernel.changes(&fixture.request())?;
    assert_eq!(
        response
            .suggestion_coverage
            .unwrap()
            .baseline_representation
            .as_deref(),
        Some("history")
    );
    assert_eq!(response.suggestions.len(), 1);
    Ok(())
}

#[test]
fn oversized_or_tampered_source_is_bounded_and_never_substituted() -> Result<()> {
    let fixture = Fixture::new()?;
    let catalog = fixture
        .store
        .catalog_at(&fixture.baseline, 134217728)?
        .unwrap();
    let artifact = &catalog.snapshot.artifacts["old/original.txt"];
    let path = fixture
        .store
        .directory()
        .join("objects/sources")
        .join(&artifact.content_hash);
    let original = fs::read(&path)?;
    fs::write(&path, vec![b'x'; 65537])?;
    assert!(
        fixture
            .store
            .read_retained_source_bounded(artifact, 65536)
            .is_err()
    );
    assert!(fixture.kernel.changes(&fixture.request()).is_err());
    fs::write(&path, original)?;
    let mut declared = artifact.clone();
    declared.bytes = 65537;
    assert!(
        fixture
            .store
            .read_retained_source_bounded(&declared, 65536)
            .is_err()
    );
    assert_eq!(
        fixture
            .kernel
            .changes(&fixture.request())?
            .suggestions
            .len(),
        1
    );
    Ok(())
}
