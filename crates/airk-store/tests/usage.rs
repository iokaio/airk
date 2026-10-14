// SPDX-License-Identifier: Apache-2.0
use airk_core::{Snapshot, SnapshotStore, usage::*};
use airk_store::{
    FileStore,
    usage::{self, DAY_MS, UsageStore},
};

fn setup() -> anyhow::Result<(tempfile::TempDir, std::path::PathBuf, UsageScope)> {
    let root = tempfile::tempdir()?;
    let directory = root.path().join("collections/project");
    let mut snapshot = Snapshot::empty("fixture-root".into());
    snapshot.collection = "project".into();
    FileStore::new(&directory).save(&snapshot)?;
    Ok((
        root,
        directory,
        UsageScope {
            collection: "project".into(),
            root_binding: [7; 32],
        },
    ))
}

fn observation(scope: &UsageScope, id: u8) -> UsageObservation {
    UsageObservation {
        schema_version: USAGE_SCHEMA_VERSION,
        event_id: [id; 16],
        scope: scope.clone(),
        unix_ms: usage::unix_ms(),
        generation: Some(1),
        operation: UsageOperation::Search,
        client: ClientFamily::Codex,
        content_kind: ContentKind::Markdown,
        outcome: UsageOutcome::Success,
        elapsed_us: 10,
        cache: CacheObservation::Unknown,
        returned_items: 1,
        truncated: false,
        response_representation: ResponseRepresentation::LogicalResultJson,
        response_utf8_bytes: Some(400),
        baseline: Some(ContextBaseline {
            kind: BaselineKind::FullReturnedSources,
            generation: 1,
            representation: ResponseRepresentation::LogicalResultJson,
            utf8_bytes: 80,
            unique_sources: 1,
        }),
        index_work: None,
    }
}

#[test]
fn supplemental_dimensions_replay_clear_and_retention_share_lifecycle() -> anyhow::Result<()> {
    let (_root, directory, scope) = setup()?;
    let first = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let second = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let mut event = SupplementalMeasurement {
        schema_version: 1,
        event_id: format!("{:032x}", 1),
        unix_ms: usage::unix_ms(),
        client: ClientFamily::Codex,
        content_kind: ContentKind::Markdown,
        provenance: MeasurementProvenance::ClientReported,
        context: Some(ContextMeasurement {
            baseline_kind: BaselineKind::ObservedAlternative,
            generation: 1,
            representation: ResponseRepresentation::LogicalResultJson,
            baseline_utf8_bytes: 100,
            returned_utf8_bytes: 1000,
            tokens: Some(TokenMeasurement {
                tokenizer: TokenizerIdentity {
                    algorithm: TokenizerAlgorithm::ClientReported,
                    definition_sha256: format!("{:064x}", 1),
                    model_revision_sha256: "a".repeat(64),
                },
                baseline_tokens: 25,
                returned_tokens: 250,
            }),
        }),
        session: None,
        paired_task: None,
    };
    assert!(first.record_supplement(&event)?);
    assert!(!second.record_supplement(&event)?);
    for ordinal in 2..=16 {
        event.event_id = format!("{ordinal:032x}");
        event
            .context
            .as_mut()
            .unwrap()
            .tokens
            .as_mut()
            .unwrap()
            .tokenizer
            .definition_sha256 = format!("{ordinal:064x}");
        assert!(second.record_supplement(&event)?);
    }
    event.event_id = format!("{:032x}", 17);
    event
        .context
        .as_mut()
        .unwrap()
        .tokens
        .as_mut()
        .unwrap()
        .tokenizer
        .definition_sha256 = format!("{:064x}", 17);
    assert!(first.record_supplement(&event).is_err());
    // Rejection rolled the dedup insert back as well as the aggregate.
    event
        .context
        .as_mut()
        .unwrap()
        .tokens
        .as_mut()
        .unwrap()
        .tokenizer
        .definition_sha256 = format!("{:064x}", 1);
    assert!(first.record_supplement(&event)?);
    let report = UsageStore::read(&directory, &scope, 0, usage::unix_ms() + DAY_MS)?;
    assert!(report.buckets.is_empty());
    assert_eq!(report.health.supplemental_duplicate_observations, 1);
    assert_eq!(report.health.retained_days, 1);
    assert_eq!(report.supplemental[0].summaries.len(), 16);
    assert_eq!(report.supplemental[0].summaries[0].measurements, 2);
    assert_eq!(report.supplemental[0].summaries[0].token_reduction, -450);
    event.event_id = format!("{:032x}", 18);
    event.unix_ms -= 31 * DAY_MS;
    assert!(first.record_supplement(&event).is_err());
    usage::clear(&directory)?;
    event.unix_ms = usage::unix_ms();
    assert!(second.record_supplement(&event).is_err());
    assert!(
        UsageStore::read(&directory, &scope, 0, usage::unix_ms() + DAY_MS)?
            .supplemental
            .is_empty()
    );
    Ok(())
}

#[test]
fn shared_quota_rejects_usage_growth_but_clear_reclaims_when_full() -> anyhow::Result<()> {
    let (root, directory, scope) = setup()?;
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    store.record(&observation(&scope, 1))?;
    let quota = airk_store::quota::Quota::open(root.path())?;
    let status = quota.status()?;
    quota.configure(airk_store::quota::QuotaPolicy {
        root_bytes: status.used_bytes,
        default_collection_bytes: status.used_bytes,
        collections: Default::default(),
    })?;
    let before = std::fs::read(directory.join("usage.sqlite3"))?;
    assert!(store.record(&observation(&scope, 2)).is_err());
    assert!(UsageStore::open_best_effort(&directory, &scope, &UsagePolicy::default()).is_err());
    assert_eq!(std::fs::read(directory.join("usage.sqlite3"))?, before);
    usage::clear(&directory)?;
    assert!(!directory.join("usage.sqlite3").exists());
    assert!(quota.status()?.used_bytes < status.used_bytes);
    assert!(FileStore::new(&directory).load()?.is_some());
    Ok(())
}

#[test]
fn ordinary_activity_prunes_expired_supplemental_rows() -> anyhow::Result<()> {
    let (_root, directory, scope) = setup()?;
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let event = SupplementalMeasurement {
        schema_version: 1,
        event_id: "c".repeat(32),
        unix_ms: usage::unix_ms() - DAY_MS,
        client: ClientFamily::Cli,
        content_kind: ContentKind::Unknown,
        provenance: MeasurementProvenance::UserSupplied,
        context: Some(ContextMeasurement {
            baseline_kind: BaselineKind::UserSupplied,
            generation: 1,
            representation: ResponseRepresentation::LogicalResultJson,
            baseline_utf8_bytes: 8,
            returned_utf8_bytes: 4,
            tokens: None,
        }),
        session: None,
        paired_task: None,
    };
    store.record_supplement(&event)?;
    usage::configure(
        &directory,
        &UsagePolicy {
            aggregate_retention_days: 1,
            ..Default::default()
        },
    )?;
    store.record(&observation(&scope, 1))?;
    let db = rusqlite::Connection::open(directory.join("usage.sqlite3"))?;
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM supplemental_daily", [], |row| row
            .get::<_, i64>(0))?,
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM supplemental_seen", [], |row| row
            .get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn independent_workers_merge_once_and_keep_signed_estimates() -> anyhow::Result<()> {
    let (_root, directory, scope) = setup()?;
    let first = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let second = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let event = observation(&scope, 1);
    first.record(&event)?;
    second.record(&event)?;
    second.record(&observation(&scope, 2))?;
    let saved = UsageStore::read(&directory, &scope, 0, usage::unix_ms() + DAY_MS)?;
    assert_eq!(saved.buckets.len(), 1);
    assert_eq!(saved.buckets[0].calls, 2);
    assert_eq!(saved.health.duplicate_observations, 1);
    assert_eq!(
        saved.buckets[0].comparisons[&BaselineKind::FullReturnedSources]
            .token_estimate
            .nominal_reduction,
        -160
    );
    assert!(saved.health.database_bytes < 1_048_576 / 2);
    assert_eq!(saved.health.dropped_observations, None);
    Ok(())
}

#[test]
fn disabled_never_creates_usage_files_and_foreign_roots_fail_closed() -> anyhow::Result<()> {
    let (_root, directory, scope) = setup()?;
    let disabled = UsagePolicy {
        mode: RecordingMode::Disabled,
        ..Default::default()
    };
    assert!(UsageStore::open(&directory, &scope, &disabled)?.is_none());
    assert!(!directory.join("usage.sqlite3").exists());
    assert!(!directory.join("usage.lock").exists());
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let mut other = scope.clone();
    other.root_binding = [8; 32];
    assert!(UsageStore::open(&directory, &other, &UsagePolicy::default()).is_err());
    assert!(store.record(&observation(&other, 1)).is_err());
    assert!(UsageStore::read(&directory, &other, 0, usage::unix_ms() + DAY_MS).is_err());
    Ok(())
}

#[test]
fn clearing_rejects_late_writers_and_preserves_index() -> anyhow::Result<()> {
    let (_root, directory, scope) = setup()?;
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    store.record(&observation(&scope, 1))?;
    usage::clear(&directory)?;
    assert!(store.record(&observation(&scope, 2)).is_err());
    assert!(FileStore::new(&directory).load()?.is_some());
    assert_eq!(
        UsageStore::read(&directory, &scope, 0, usage::unix_ms() + DAY_MS)?
            .buckets
            .len(),
        0
    );
    Ok(())
}

#[test]
fn deletion_and_recreation_cannot_receive_an_old_workers_history() -> anyhow::Result<()> {
    let (root, directory, scope) = setup()?;
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    airk_store::remove_collection(root.path(), "project")?;
    assert!(!directory.exists());
    assert!(store.record(&observation(&scope, 1)).is_err());
    assert!(!directory.exists());
    let mut snapshot = Snapshot::empty("fixture-root".into());
    snapshot.collection = "project".into();
    FileStore::new(&directory).save(&snapshot)?;
    let replacement = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    assert!(store.record(&observation(&scope, 2)).is_err());
    replacement.record(&observation(&scope, 3))?;
    assert_eq!(
        UsageStore::read(&directory, &scope, 0, usage::unix_ms() + DAY_MS)?.buckets[0].calls,
        1
    );
    Ok(())
}

#[test]
fn diagnostic_opt_in_is_bounded_and_disable_takes_effect_on_existing_workers() -> anyhow::Result<()>
{
    let (_root, directory, scope) = setup()?;
    let mut policy = UsagePolicy {
        mode: RecordingMode::DiagnosticEvents,
        ..Default::default()
    };
    usage::configure(&directory, &policy)?;
    let store = UsageStore::open(&directory, &scope, &policy)?.unwrap();
    store.record(&observation(&scope, 1))?;
    let db = rusqlite::Connection::open(directory.join("usage.sqlite3"))?;
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM diagnostics", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    policy.mode = RecordingMode::Disabled;
    usage::configure(&directory, &policy)?;
    store.record(&observation(&scope, 2))?;
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM seen", [], |r| r.get::<_, i64>(0))?,
        1
    );
    Ok(())
}

#[test]
fn stale_and_busy_observations_fail_without_partial_aggregates() -> anyhow::Result<()> {
    let (_root, directory, scope) = setup()?;
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let mut stale = observation(&scope, 1);
    stale.unix_ms -= 31 * DAY_MS;
    assert!(store.record(&stale).is_err());
    let lock = usage::deletion_guard(&directory)?;
    assert!(store.record(&observation(&scope, 2)).is_err());
    drop(lock);
    assert_eq!(
        UsageStore::read(&directory, &scope, 0, usage::unix_ms() + DAY_MS)?
            .buckets
            .len(),
        0
    );
    store.record(&observation(&scope, 3))?;
    Ok(())
}

#[test]
fn page_and_root_quotas_fail_closed_without_partial_commits() -> anyhow::Result<()> {
    let (root, directory, scope) = setup()?;
    let limited = UsagePolicy {
        mode: RecordingMode::DiagnosticEvents,
        max_collection_bytes: 131_072,
        ..Default::default()
    };
    usage::configure(&directory, &limited)?;
    let store = UsageStore::open(&directory, &scope, &limited)?.unwrap();
    let mut committed = 0;
    let mut exhausted = false;
    for id in 1..=200 {
        match store.record(&observation(&scope, id)) {
            Ok(()) => committed += 1,
            Err(_) => {
                exhausted = true;
                break;
            }
        }
    }
    assert!(exhausted, "fixture must exercise SQLITE_FULL rollback");
    let report = UsageStore::read(&directory, &scope, 0, usage::unix_ms() + DAY_MS)?;
    assert_eq!(
        report.buckets.iter().map(|b| b.calls).sum::<u64>(),
        committed
    );
    assert!(report.health.database_bytes <= limited.max_collection_bytes / 2);
    assert!(FileStore::new(&directory).load()?.is_some());
    let other_dir = root.path().join("collections/other");
    let mut snapshot = Snapshot::empty("other-fixture".into());
    snapshot.collection = "other".into();
    FileStore::new(&other_dir).save(&snapshot)?;
    let other_scope = UsageScope {
        collection: "other".into(),
        root_binding: [9; 32],
    };
    let root_limit = UsagePolicy {
        max_data_root_bytes: 1_048_576,
        ..Default::default()
    };
    assert!(UsageStore::open(&other_dir, &other_scope, &root_limit).is_err());
    assert!(!other_dir.join("usage.sqlite3").exists());
    Ok(())
}
