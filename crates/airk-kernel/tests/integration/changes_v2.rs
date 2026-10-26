// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
use airk_core::changes::*;
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use std::fs;

#[test]
fn retained_baselines_coalesce_delayed_file_and_unit_changes_and_keep_index_current_explicit()
-> Result<()> {
    use airk_core::extraction::IndexDetail;
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("one.rs"),
        "pub fn alpha(value: u32) -> u32 { value + 1 }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.configure_index_detail(IndexDetail::Balanced)?;
    kernel.index()?;
    let baseline = kernel
        .file_info_conditional("one.rs", &Default::default())?
        .read
        .publication
        .unwrap()
        .token;
    fs::write(
        root.path().join("one.rs"),
        "pub fn alpha(value: u32) -> u32 { value + 2 }\n",
    )?;
    kernel.index()?;
    fs::write(
        root.path().join("one.rs"),
        "pub fn alpha(value: u32) -> u32 { value + 3 }\n",
    )?;
    kernel.index()?;
    let request = ChangesRequest {
        since: Some(baseline),
        include_symbols: true,
        selectors: vec![RememberedSelector::Path {
            path: "one.rs".into(),
        }],
        limit: 100,
        max_bytes: 65_536,
        ..Default::default()
    };
    let changes = kernel.changes(&request)?;
    assert_eq!(changes.baseline_status, BaselineStatus::Verified);
    let file = changes
        .changes
        .iter()
        .find(|row| matches!(row.subject, ChangeSubject::Artifact { .. }))
        .unwrap();
    assert_eq!(file.observations, 2);
    assert!(file.dimensions.contains(&ChangeDimension::Content));
    assert_ne!(
        file.before.as_ref().unwrap().source_hash,
        file.after.as_ref().unwrap().source_hash
    );
    assert!(
        changes
            .changes
            .iter()
            .any(|row| matches!(row.subject, ChangeSubject::Symbol { .. }))
    );
    assert_eq!(
        serde_json::to_value(&changes.validations[0])?["outcome"],
        "index-current"
    );
    assert!(!changes.freshness.live_source_verified);
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    let repeated = reopened.changes(&request)?;
    assert_eq!(
        serde_json::to_value(&changes.changes)?,
        serde_json::to_value(&repeated.changes)?
    );
    let first = reopened.changes(&ChangesRequest {
        limit: 1,
        ..request.clone()
    })?;
    let mut changed_flags = request.clone();
    changed_flags.cursor = first.cursor.clone();
    changed_flags.include_documents = true;
    assert_eq!(
        reopened.changes(&changed_flags)?.status,
        ChangesStatus::CursorExpired
    );
    let mut tiny = request.clone();
    tiny.max_bytes = 1;
    assert!(reopened.changes(&tiny).is_err());
    let mut paged = request.clone();
    paged.limit = 1;
    let mut rows = Vec::new();
    let mut validations = Vec::new();
    loop {
        let page = reopened.changes(&paged)?;
        rows.extend(page.changes);
        validations.extend(page.validations);
        let Some(cursor) = page.cursor else {
            assert!(!page.truncated);
            break;
        };
        paged.cursor = Some(cursor);
    }
    assert_eq!(
        serde_json::to_value(rows)?,
        serde_json::to_value(&changes.changes)?
    );
    assert_eq!(validations.len(), 1);
    let before_repack = reopened.status()?.publication.unwrap().token;
    reopened.configure_compression(airk_core::compression::CompressionMode::Space)?;
    reopened.repack_current(None, 32)?;
    let physical = reopened.changes(&ChangesRequest {
        since: Some(before_repack),
        ..Default::default()
    })?;
    assert_eq!(physical.baseline_status, BaselineStatus::Verified);
    assert!(physical.changes.is_empty());
    let after_repack = reopened.changes(&request)?;
    assert_eq!(
        serde_json::to_value(after_repack.coverage)?,
        serde_json::to_value(changes.coverage)?
    );
    assert_eq!(
        serde_json::to_value(after_repack.changes)?,
        serde_json::to_value(&changes.changes)?
    );
    let mut foreign = request;
    foreign.since.as_mut().unwrap().revision =
        airk_core::identity::Sha256Digest::from_bytes([77; 32]);
    let rejected = reopened.changes(&foreign)?;
    assert_eq!(
        rejected.baseline_status,
        BaselineStatus::BaselineUnavailable
    );
    assert!(rejected.changes.is_empty());
    assert_eq!(rejected.validations[0].outcome, ValidationOutcome::Current);
    Ok(())
}

#[test]
fn historical_alias_paths_keep_prefix_and_remembered_identity_after_rebinding() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original = tempfile::tempdir()?;
    let replacement = tempfile::tempdir()?;
    fs::write(
        original.path().join("note.md"),
        "# Original alias\nRemember this source's exact bytes and binding.\n",
    )?;
    fs::write(
        replacement.path().join("note.md"),
        "# Replacement alias\nDifferent source and document identity.\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.initialize()?;
    kernel.add_root("docs", original.path())?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let old = store.current_test_catalog()?.unwrap();
    let since = old.token;
    let hash = old.inventory.as_ref().unwrap().files["@docs/note.md"]
        .state
        .source_hash;
    drop(old);
    kernel.remove_root("docs")?;
    kernel.index()?;
    let request = ChangesRequest {
        since: Some(since),
        selectors: vec![RememberedSelector::Citation {
            path: "@docs/note.md".into(),
            source_hash: hash,
        }],
        path_prefix: "@docs/".into(),
        max_bytes: 65_536,
        ..Default::default()
    };
    let removed = kernel.changes(&request)?;
    assert_eq!(removed.baseline_status, BaselineStatus::Verified);
    assert!(removed.changes.iter().any(|row| {
        row.before
            .as_ref()
            .is_some_and(|endpoint| endpoint.citation_path.as_deref() == Some("@docs/note.md"))
            && row.after.is_none()
    }));
    assert_eq!(removed.validations[0].outcome, ValidationOutcome::Unknown);
    let read = kernel.file_info_conditional(
        "@docs/note.md",
        &airk_core::evidence::EvidenceReadOptions {
            follow_moves: true,
            known_hash: Some(hash),
            ..Default::default()
        },
    )?;
    assert_eq!(
        read.read.status,
        airk_core::evidence::EvidenceReadStatus::Unknown
    );
    kernel.add_root("docs", replacement.path())?;
    kernel.index()?;
    let rebound = kernel.changes(&request)?;
    assert_eq!(rebound.validations[0].outcome, ValidationOutcome::Modified);
    assert!(rebound.validations[0].candidates.is_empty());
    assert_eq!(
        fs::read_to_string(original.path().join("note.md"))?,
        "# Original alias\nRemember this source's exact bytes and binding.\n"
    );
    Ok(())
}

#[test]
fn indexed_literal_at_paths_remain_valid_without_creating_aliases() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::create_dir(root.path().join("@literal"))?;
    fs::write(
        root.path().join("@literal/note.md"),
        "# Literal primary path\nBounded original evidence.\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let response = kernel.changes(&ChangesRequest {
        selectors: vec![RememberedSelector::Path {
            path: "@literal/note.md".into(),
        }],
        path_prefix: "@lit".into(),
        ..Default::default()
    })?;
    assert_eq!(response.validations[0].outcome, ValidationOutcome::Current);
    let read = kernel.file_info_conditional("@literal/note.md", &Default::default())?;
    assert_eq!(
        read.read.status,
        airk_core::evidence::EvidenceReadStatus::Content
    );
    assert!(
        kernel
            .changes(&ChangesRequest {
                selectors: vec![RememberedSelector::Path {
                    path: "@foreign/note.md".into()
                }],
                ..Default::default()
            })
            .is_err()
    );
    Ok(())
}

#[test]
fn equivalent_copy_candidates_are_bounded_scoped_and_never_redirected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let content = "# Identical evidence\nThis deliberately repeated document retains substantial bounded evidence for an explicit equivalent-content candidate.\n";
    fs::write(root.path().join("original.md"), content)?;
    for index in 0..9 {
        fs::write(root.path().join(format!("copy-{index}.md")), content)?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    fs::remove_file(root.path().join("original.md"))?;
    kernel.index()?;
    let mut request = ChangesRequest {
        selectors: vec![RememberedSelector::Path {
            path: "original.md".into(),
        }],
        max_bytes: 65_536,
        ..Default::default()
    };
    let response = kernel.changes(&request)?;
    assert_eq!(response.validations[0].outcome, ValidationOutcome::Unknown);
    assert!(response.validations[0].candidates.is_empty());
    assert!(!response.validations[0].candidates_truncated);
    request.path_prefix = "copy-0".into();
    let scoped = kernel.changes(&request)?;
    assert!(scoped.validations[0].candidates.is_empty());
    let read = kernel.file_info_conditional(
        "original.md",
        &airk_core::evidence::EvidenceReadOptions {
            follow_moves: true,
            ..Default::default()
        },
    )?;
    assert_eq!(
        read.read.status,
        airk_core::evidence::EvidenceReadStatus::Unknown
    );
    assert!(read.content.is_none());
    request.selectors = vec![RememberedSelector::Path {
        path: "copy-1.md".into(),
    }];
    let excluded = kernel.changes(&request)?;
    assert_eq!(excluded.validations[0].ordinal, 0);
    assert_eq!(excluded.validations[0].outcome, ValidationOutcome::Unknown);
    assert!(excluded.validations[0].current.is_none());
    Ok(())
}

#[test]
fn retained_baseline_reports_round_trip_edits_without_restoring_old_reads() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    fs::write(root.path().join("note.txt"), "original source")?;
    kernel.index()?;
    let baseline = kernel.status()?.publication.unwrap().token;
    for text in ["edited source", "original source"] {
        fs::write(root.path().join("note.txt"), text)?;
        kernel.index()?;
    }
    let response = kernel.changes(&ChangesRequest {
        since: Some(baseline),
        selectors: vec![RememberedSelector::Path {
            path: "note.txt".into(),
        }],
        ..Default::default()
    })?;
    assert_eq!(response.baseline_status, BaselineStatus::Verified);
    let file = response
        .changes
        .iter()
        .find(|row| matches!(row.subject, ChangeSubject::Artifact { .. }))
        .unwrap();
    assert_eq!(file.observations, 2);
    assert_eq!(
        file.before.as_ref().unwrap().source_hash,
        file.after.as_ref().unwrap().source_hash
    );
    assert!(file.dimensions.contains(&ChangeDimension::Content));
    assert!(response.coverage.unwrap().files.complete_after <= baseline.generation);
    assert!(
        kernel
            .file_info_conditional(
                "note.txt",
                &airk_core::evidence::EvidenceReadOptions {
                    revision: Some(baseline),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert_eq!(response.validations[0].outcome, ValidationOutcome::Current);
    Ok(())
}

#[test]
fn remembered_validations_page_on_exact_target_and_expire_without_fallback() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("note.md"),
        "# Remembered\nOriginal bounded evidence.\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let mut request = ChangesRequest {
        selectors: vec![
            RememberedSelector::Path {
                path: "note.md".into(),
            },
            RememberedSelector::Path {
                path: "missing.md".into(),
            },
        ],
        limit: 1,
        ..Default::default()
    };
    let first = kernel.changes(&request)?;
    assert_eq!(first.validations[0].outcome, ValidationOutcome::Current);
    assert!(!first.freshness.live_source_verified);
    assert!(first.truncated);
    request.cursor = first.cursor.clone();
    let second = kernel.changes(&request)?;
    assert_eq!(second.to, first.to);
    assert_eq!(second.validations[0].ordinal, 1);
    assert_eq!(second.validations[0].outcome, ValidationOutcome::Unknown);
    assert!(!second.truncated);
    let mut changed = request.clone();
    changed.path_prefix = "note".into();
    assert_eq!(
        kernel.changes(&changed)?.status,
        ChangesStatus::CursorExpired
    );
    fs::write(
        root.path().join("note.md"),
        "# New\nReplacement evidence.\n",
    )?;
    kernel.index()?;
    let expired = kernel.changes(&request)?;
    assert_eq!(expired.status, ChangesStatus::CursorExpired);
    assert_ne!(expired.to, first.to);
    assert_ne!(expired.current, first.to);
    assert!(expired.validations.is_empty());
    Ok(())
}

#[test]
fn malformed_selectors_and_too_small_record_budget_fail_explicitly() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("note.md"), "# Evidence\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    for path in ["../note.md", "/note.md", "@foreign/note.md"] {
        assert!(
            kernel
                .changes(&ChangesRequest {
                    selectors: vec![RememberedSelector::Path { path: path.into() }],
                    ..Default::default()
                })
                .is_err()
        );
    }
    assert!(
        kernel
            .changes(&ChangesRequest {
                selectors: vec![RememberedSelector::Path {
                    path: "note.md".into()
                }],
                max_bytes: 1,
                ..Default::default()
            })
            .is_err()
    );
    let unsupported = kernel.changes(&ChangesRequest {
        selectors: vec![RememberedSelector::Symbol {
            symbol_key: airk_core::identity::SymbolKey::from_bytes([1; 16]),
        }],
        ..Default::default()
    })?;
    assert_eq!(
        unsupported.validations[0].outcome,
        ValidationOutcome::Unknown
    );
    assert_eq!(
        unsupported.coverage.unwrap().code.reason,
        Some(CoverageReason::Unsupported)
    );
    Ok(())
}

#[test]
fn unavailable_baseline_and_scheduler_do_not_hide_independent_validation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("note.md"),
        "# Indexed\nRemember source identity.\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let catalog = store.current_test_catalog()?.unwrap();
    let current = catalog.token;
    let hash = catalog.inventory.as_ref().unwrap().files["note.md"]
        .state
        .source_hash;
    let mut missing = current;
    missing.generation = 0;
    missing.revision = airk_core::identity::Sha256Digest::from_bytes([55; 32]);
    fs::write(
        data.path().join("scheduler-1.2.sqlite3"),
        b"corrupt scheduler fixture",
    )?;
    let response = kernel.changes(&ChangesRequest {
        since: Some(missing),
        selectors: vec![RememberedSelector::Citation {
            path: "note.md".into(),
            source_hash: hash,
        }],
        ..Default::default()
    })?;
    assert_eq!(
        response.baseline_status,
        BaselineStatus::BaselineUnavailable
    );
    assert_eq!(
        response.validations[0].outcome,
        ValidationOutcome::Unchanged
    );
    assert_eq!(
        response.validations[0].validated_dimensions,
        vec![ChangeDimension::Content]
    );
    assert_eq!(response.freshness.state, FreshnessState::Unavailable);
    assert!(!response.freshness.live_source_verified);
    let mut fork = current;
    fork.revision = missing.revision;
    assert_eq!(
        kernel
            .changes(&ChangesRequest {
                since: Some(fork),
                ..Default::default()
            })?
            .baseline_status,
        BaselineStatus::BaselineUnavailable
    );
    Ok(())
}
