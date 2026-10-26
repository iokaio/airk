// SPDX-License-Identifier: Apache-2.0
use airk_core::usage::*;
use airk_store::{
    FileStore,
    usage::{self, DAY_MS, UsageStore},
};

fn setup() -> anyhow::Result<(tempfile::TempDir, std::path::PathBuf, UsageScope)> {
    let root = tempfile::tempdir()?;
    let directory = root.path().join("collections-1.2/project");
    initialize_native(root.path(), &directory, 1)?;
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
        baseline_unavailable: None,
        baseline: Some(ContextBaseline {
            kind: BaselineKind::FullReturnedSources,
            generation: 1,
            representation: ResponseRepresentation::LogicalResultJson,
            utf8_bytes: 80,
            unique_sources: 1,
        }),
        index_work: None,
        resources: None,
    }
}

#[test]
fn lifetime_is_atomic_deduplicated_scoped_and_survives_daily_expiry() -> anyhow::Result<()> {
    let (_root, directory, scope) = setup()?;
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let mut prior = observation(&scope, 1);
    prior.unix_ms -= 2 * DAY_MS;
    store.record(&prior)?;
    store.record(&prior)?;
    let policy = UsagePolicy {
        aggregate_retention_days: 1,
        ..UsagePolicy::default()
    };
    usage::configure(&directory, &policy)?;
    store.record(&observation(&scope, 2))?;
    let read = UsageStore::read(&directory, &scope, 0, u64::MAX / 2)?;
    assert_eq!(read.buckets.len(), 1);
    let lifetime = read.lifetime.unwrap();
    assert_eq!(lifetime.calls, 2);
    assert_eq!(
        lifetime.comparisons[&BaselineKind::FullReturnedSources]
            .token_estimate
            .nominal_reduction,
        -160
    );
    let mut foreign = scope.clone();
    foreign.root_binding = [8; 32];
    assert!(UsageStore::read(&directory, &foreign, 0, u64::MAX / 2).is_err());
    usage::clear(&directory)?;
    assert!(
        UsageStore::read(&directory, &scope, 0, u64::MAX / 2)?
            .lifetime
            .is_none()
    );
    assert!(store.record(&observation(&scope, 3)).is_err());
    Ok(())
}

#[test]
fn session_links_require_retained_local_requests_and_cannot_be_counted_twice() -> anyhow::Result<()>
{
    let (_root, directory, scope) = setup()?;
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    store.record(&observation(&scope, 1))?;
    let mut event: SupplementalMeasurement = serde_json::from_value(serde_json::json!({
        "schema_version":1,"event_id":"00000000000000000000000000000001","unix_ms":usage::unix_ms(),
        "client":"codex","content_kind":"code","provenance":"client_reported","context":null,"paired_task":null,
        "session":{"model_revision_sha256":"a".repeat(64),"requests":1,"input_tokens":10,"cached_input_tokens":null,"output_tokens":null,"elapsed_ms":null,"tool_round_trips":null},
        "airk_request_ids":["01".repeat(16)]
    }))?;
    assert!(store.record_supplement(&event)?);
    assert!(!store.record_supplement(&event)?);
    event.event_id = "02".repeat(16);
    assert!(store.record_supplement(&event).is_err());
    event.airk_request_ids = vec!["03".repeat(16)];
    assert!(store.record_supplement(&event).is_err());
    let read = UsageStore::read(&directory, &scope, 0, u64::MAX / 2)?;
    assert_eq!(read.supplemental[0].summaries[0].sessions.sessions, 1);
    assert_eq!(read.supplemental[0].summaries[0].linked_airk_requests, 1);
    assert_eq!(read.lifetime.unwrap().calls, 1);
    Ok(())
}

#[test]
fn supplemental_dimensions_replay_clear_and_retention_share_lifecycle() -> anyhow::Result<()> {
    let (_root, directory, scope) = setup()?;
    let first = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let second = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let mut event = SupplementalMeasurement {
        airk_request_ids: Vec::new(),
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
        root_bytes: status.used_bytes + status.reserved_bytes,
        default_collection_bytes: status.used_bytes + status.reserved_bytes,
        collections: Default::default(),
    })?;
    let before = std::fs::read(directory.join("usage.airk"))?;
    assert!(store.record(&observation(&scope, 2)).is_err());
    // Identity-only opens are read-only even when storage is full; mutations still fail.
    let reopened =
        UsageStore::open_best_effort(&directory, &scope, &UsagePolicy::default())?.unwrap();
    assert!(reopened.record(&observation(&scope, 2)).is_err());
    assert_eq!(std::fs::read(directory.join("usage.airk"))?, before);
    usage::clear(&directory)?;
    assert!(!directory.join("usage.airk").exists());
    assert!(quota.status()?.used_bytes < status.used_bytes);
    assert!(
        FileStore::new(&directory)
            .current_scope("fixture-root", "project")?
            .is_some()
    );
    Ok(())
}

#[test]
fn ordinary_activity_prunes_expired_supplemental_rows() -> anyhow::Result<()> {
    let (_root, directory, scope) = setup()?;
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let event = SupplementalMeasurement {
        airk_request_ids: Vec::new(),
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
    let state = ledger_state(&directory)?;
    assert_eq!(state["supplemental_daily"].as_object().unwrap().len(), 0);
    assert_eq!(state["supplemental_seen"].as_object().unwrap().len(), 0);
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
fn concurrent_best_effort_workers_initialize_and_record_without_losing_calls() -> anyhow::Result<()>
{
    let (_root, directory, scope) = setup()?;
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|threads| -> anyhow::Result<()> {
        let workers: Vec<_> = (0..4)
            .map(|worker| {
                let (directory, scope, barrier) = (&directory, &scope, &barrier);
                threads.spawn(move || -> anyhow::Result<()> {
                    barrier.wait();
                    for request in 0..12 {
                        let store = UsageStore::open_best_effort(
                            directory,
                            scope,
                            &UsagePolicy::default(),
                        )?
                        .unwrap();
                        let event = observation(scope, worker * 12 + request + 1);
                        store.record(&event)?;
                        store.record(&event)?;
                    }
                    Ok(())
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap()?;
        }
        Ok(())
    })?;
    let saved = UsageStore::read(&directory, &scope, 0, usage::unix_ms() + DAY_MS)?;
    assert_eq!(saved.buckets.iter().map(|b| b.calls).sum::<u64>(), 48);
    assert_eq!(saved.lifetime.unwrap().calls, 48);
    assert_eq!(saved.health.duplicate_observations, 48);
    Ok(())
}

#[test]
fn brief_lifecycle_and_shared_quota_contention_waits_but_stuck_writer_is_bounded()
-> anyhow::Result<()> {
    use std::time::{Duration, Instant};
    let (root, directory, scope) = setup()?;
    let store = UsageStore::open_best_effort(&directory, &scope, &UsagePolicy::default())?.unwrap();
    let lock = usage::deletion_guard(&directory)?;
    std::thread::scope(|threads| -> anyhow::Result<()> {
        threads.spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(lock);
        });
        store.record(&observation(&scope, 1))
    })?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.path().join(".quota-1.2/ledger.lock"))?;
    lock.lock()?;
    std::thread::scope(|threads| -> anyhow::Result<()> {
        threads.spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(lock);
        });
        store.record(&observation(&scope, 2))
    })?;
    let lock = usage::deletion_guard(&directory)?;
    let started = Instant::now();
    assert!(store.record(&observation(&scope, 3)).is_err());
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "busy accounting must not wait indefinitely"
    );
    assert!(
        usage::deletion_guard(&directory).is_err(),
        "collection deletion remains fail-fast"
    );
    drop(lock);
    assert_eq!(
        UsageStore::read(&directory, &scope, 0, usage::unix_ms() + DAY_MS)?
            .lifetime
            .unwrap()
            .calls,
        2
    );
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
    assert!(!directory.join("usage.airk").exists());
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
    assert!(
        FileStore::new(&directory)
            .current_scope("fixture-root", "project")?
            .is_some()
    );
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
    let root = tempfile::tempdir()?;
    let directory = root.path().join("collections-1.2/project");
    let scope = UsageScope {
        collection: "project".into(),
        root_binding: [7; 32],
    };
    let initialize = |incarnation: u8| initialize_native(root.path(), &directory, incarnation);
    initialize(1)?;
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?.unwrap();
    airk_store::remove_collection(root.path(), "project")?;
    assert!(!directory.exists());
    assert!(store.record(&observation(&scope, 1)).is_err());
    assert!(!directory.exists());
    initialize(2)?;
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
    assert_eq!(
        ledger_state(&directory)?["diagnostics"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
    policy.mode = RecordingMode::Disabled;
    usage::configure(&directory, &policy)?;
    store.record(&observation(&scope, 2))?;
    assert_eq!(
        ledger_state(&directory)?["seen"].as_object().unwrap().len(),
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
    assert!(exhausted, "fixture must exercise quota-admitted rollback");
    let report = UsageStore::read(&directory, &scope, 0, usage::unix_ms() + DAY_MS)?;
    assert_eq!(
        report.buckets.iter().map(|b| b.calls).sum::<u64>(),
        committed
    );
    // redb has no rollback sidecar: the whole admitted cap bounds its one file.
    assert!(report.health.database_bytes <= limited.max_collection_bytes);
    assert!(!directory.join("usage.airk-journal").exists());
    assert!(!directory.join("usage.airk-wal").exists());
    assert!(
        FileStore::new(&directory)
            .current_scope("fixture-root", "project")?
            .is_some()
    );
    let other_dir = root.path().join("collections-1.2/other");
    std::fs::create_dir_all(&other_dir)?;
    let other_scope = UsageScope {
        collection: "other".into(),
        root_binding: [9; 32],
    };
    let root_limit = UsagePolicy {
        max_data_root_bytes: 1_048_576,
        ..Default::default()
    };
    assert!(UsageStore::open(&other_dir, &other_scope, &root_limit).is_err());
    assert!(!other_dir.join("usage.airk").exists());
    Ok(())
}

fn ledger_state(directory: &std::path::Path) -> anyhow::Result<serde_json::Value> {
    use redb::ReadableDatabase;
    let database = redb::Builder::new().open_read_only(directory.join("usage.airk"))?;
    let transaction = database.begin_read()?;
    let table = transaction.open_table(redb::TableDefinition::<&str, &[u8]>::new("ledger"))?;
    let value = table.get("state")?.unwrap();
    Ok(serde_json::from_slice(
        &lz4_flex::decompress_size_prepended(value.value())?,
    )?)
}

fn initialize_native(
    root: &std::path::Path,
    directory: &std::path::Path,
    incarnation: u8,
) -> anyhow::Result<()> {
    use airk_core::{
        graph_storage::{GraphHead, GraphScope},
        identity::{Incarnation, Sha256Digest},
    };
    use airk_store::current::{CurrentHead, CurrentScope, map};
    let scope = CurrentScope {
        root: "fixture-root".into(),
        collection: "project".into(),
        incarnation: Incarnation::from_bytes([incarnation; 16]),
    };
    let graph = GraphScope {
        root: scope.root.clone(),
        collection: scope.collection.clone(),
        incarnation: scope.incarnation,
        graph_instance: scope.incarnation,
        specification: Sha256Digest::from_bytes([3; 32]),
        record_bytes: 16,
    };
    let head = CurrentHead {
        version: 2,
        sequence: 0,
        lexical_sequence: 0,
        artifact_root: map::empty_root(&scope)?,
        lexical_commit: airk_store::current::kv::lexical::Manifest::empty(&scope)?.digest()?,
        graph_head: GraphHead::empty(graph)?,
        scope,
    };
    let file = FileStore::new(directory);
    let writer = file.lock()?;
    let quota = airk_store::quota::Quota::open(root)?;
    drop(file.bootstrap_redb(&writer, &quota, &head, 2 << 20)?);
    let current = file.redb_writer(&writer, &quota, &head.scope)?;
    current.initialize_collection(
        &head.token()?,
        &airk_store::current::collection::CollectionConfiguration {
            roots: Default::default(),
            extractor_version: "native-usage-fixture/1".into(),
            extraction_policy_digest: Sha256Digest::from_bytes([4; 32]),
            index_detail: airk_core::extraction::IndexDetail::AgentLean,
            vector_selection: Default::default(),
            model: None,
        },
    )?;
    Ok(())
}
