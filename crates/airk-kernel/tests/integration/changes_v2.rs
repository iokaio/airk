// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
use airk_core::changes::*;
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use std::fs;

#[test]
fn obsolete_conditional_reads_require_rebuild_without_evidence_fallback() -> Result<()> {
    use airk_core::{Artifact, Chunk, DocumentLocation, Snapshot};
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let _writer = store.lock()?;
    let mut snapshot = Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
    snapshot.artifacts.insert(
        "old.md".into(),
        Artifact {
            path: "old.md".into(),
            content_hash: "01".repeat(32),
            bytes: 13,
            chunks: vec![Chunk {
                start_line: 1,
                end_line: 1,
                text: "Old evidence.\n".into(),
                location: DocumentLocation::Lines { start: 1, end: 1 },
            }],
            language: "markdown".into(),
            extraction_method: "utf8".into(),
            parse_status: "plain".into(),
            facts: vec![],
            retained: None,
        },
    );
    // A legacy coordination row cannot opt into current readers merely by
    // claiming the current manifest format without receipt-bound identities.
    let claimed_revision = "a".repeat(64);
    store.save_with_components(&snapshot, &claimed_revision)?;
    let components = store.directory().join("generations").join(claimed_revision);
    fs::create_dir_all(&components)?;
    fs::write(
        components.join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 4,
            "engine": "tantivy-0.26.2/retired-v2-fixture",
        }))?,
    )?;
    drop(_writer);
    assert!(Kernel::open(root.path(), Some(data.path().into())).is_err());
    assert!(store.directory().join("index.sqlite3").is_file());
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
    assert!(removed.changes.is_empty());
    assert_eq!(removed.baseline_status, BaselineStatus::BaselineUnavailable);
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
fn expired_baseline_cannot_claim_complete_edit_history() -> Result<()> {
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
    assert_eq!(
        response.baseline_status,
        BaselineStatus::BaselineUnavailable
    );
    assert!(response.changes.is_empty());
    assert_eq!(
        response.coverage.unwrap().files.reason,
        Some(CoverageReason::Unsupported)
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
