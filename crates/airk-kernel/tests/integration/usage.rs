// SPDX-License-Identifier: Apache-2.0
use airk_core::usage::*;
use airk_kernel::usage::{ByteTokenHeuristic, summarize};

fn observation(id: u8) -> UsageObservation {
    UsageObservation {
        schema_version: USAGE_SCHEMA_VERSION,
        event_id: [id; 16],
        scope: UsageScope {
            collection: "project-fixture".into(),
            root_binding: [7; 32],
        },
        unix_ms: 100,
        generation: Some(4),
        operation: UsageOperation::Search,
        client: ClientFamily::Codex,
        content_kind: ContentKind::Code,
        outcome: UsageOutcome::Success,
        elapsed_us: 4_000,
        cache: CacheObservation::Warm,
        returned_items: 2,
        truncated: false,
        response_representation: ResponseRepresentation::McpToolResultJson,
        response_utf8_bytes: Some(4_800),
        baseline_unavailable: None,
        baseline: Some(ContextBaseline {
            kind: BaselineKind::FullReturnedSources,
            generation: 4,
            representation: ResponseRepresentation::McpToolResultJson,
            utf8_bytes: 48_000,
            unique_sources: 2,
        }),
        index_work: None,
        resources: None,
    }
}

fn supplied_session() -> SessionMeasurement {
    SessionMeasurement {
        model_revision_sha256: "a".repeat(64),
        requests: 2,
        input_tokens: Some(120),
        cached_input_tokens: Some(20),
        output_tokens: Some(12),
        elapsed_ms: Some(1000),
        tool_round_trips: None,
    }
}

#[test]
fn resource_aggregation_keeps_unknown_overlap_and_high_water_semantics() -> anyhow::Result<()> {
    let mut first = observation(201);
    first.resources = Some(ResourceMeasurement {
        overlapped: false,
        counters: [
            (ResourceCounter::ProcessCpuUs, 40),
            (ResourceCounter::OsStorageReadBytes, 4096),
        ]
        .into(),
        peak_rss_bytes: Some(9000),
    });
    let mut second = observation(202);
    second.resources = Some(ResourceMeasurement {
        overlapped: true,
        counters: [(ResourceCounter::ProcessCpuUs, 999)].into(),
        peak_rss_bytes: Some(8000),
    });
    let mut report = summarize(
        &first.scope,
        0,
        1000,
        [&first, &second],
        ByteTokenHeuristic::default(),
    )?;
    let old = observation(203);
    report.merge(&summarize(
        &old.scope,
        0,
        1000,
        [&old],
        ByteTokenHeuristic::default(),
    )?)?;
    let summary = &report.operations[&UsageOperation::Search];
    assert_eq!(summary.calls, 3);
    assert_eq!(summary.resources.intervals, 2);
    assert_eq!(summary.resources.overlapped_intervals, 1);
    assert_eq!(
        summary.resources.counters[&ResourceCounter::ProcessCpuUs].value,
        40
    );
    assert_eq!(
        summary.resources.counters[&ResourceCounter::ProcessCpuUs].samples,
        1
    );
    assert!(
        !summary
            .resources
            .counters
            .contains_key(&ResourceCounter::OsStorageWriteBytes)
    );
    assert_eq!(summary.resources.peak_rss_bytes, Some(9000));
    Ok(())
}

fn supplemental(id: u8) -> SupplementalMeasurement {
    SupplementalMeasurement {
        airk_request_ids: Vec::new(),
        schema_version: 1,
        event_id: format!("{id:032x}"),
        unix_ms: airk_store::usage::unix_ms(),
        client: ClientFamily::Codex,
        content_kind: ContentKind::Code,
        provenance: MeasurementProvenance::ClientReported,
        context: Some(ContextMeasurement {
            baseline_kind: BaselineKind::ObservedAlternative,
            generation: 1,
            representation: ResponseRepresentation::LogicalResultJson,
            baseline_utf8_bytes: 80,
            returned_utf8_bytes: 400,
            tokens: Some(TokenMeasurement {
                tokenizer: TokenizerIdentity {
                    algorithm: TokenizerAlgorithm::ClientReported,
                    definition_sha256: "b".repeat(64),
                    model_revision_sha256: "a".repeat(64),
                },
                baseline_tokens: 40,
                returned_tokens: 150,
            }),
        }),
        session: Some(supplied_session()),
        paired_task: Some(PairedTaskMeasurement {
            same_task_and_model: true,
            with_airk: supplied_session(),
            without_airk: supplied_session(),
            with_airk_result: TaskResult::Failed,
            without_airk_result: TaskResult::Passed,
        }),
    }
}

fn scenario() -> PriceScenario {
    PriceScenario {
        output_price_per_million: None,
        model: "fictional-model".into(),
        currency: "USD".into(),
        rate_date: "2026-10-14".into(),
        input_price_per_million: "2.500000".into(),
        cached_input_treatment: CachePriceTreatment::SeparateRate,
        cached_input_price_per_million: Some("0.25".into()),
        context_assumes_uncached: true,
    }
}

#[test]
fn explicit_price_scenarios_preserve_sign_cache_unknown_and_decimal_precision() -> anyhow::Result<()>
{
    let mut price = scenario();
    assert!(price.output_equivalent(12)?.is_none());
    price.output_price_per_million = Some("10.00".into());
    assert_eq!(
        price.output_equivalent(-12)?.as_deref(),
        Some("-0.000120000000")
    );
    assert_eq!(
        price.input_equivalent(-80, false)?.as_deref(),
        Some("-0.000200000000")
    );
    assert_eq!(
        price.input_equivalent(20, true)?.as_deref(),
        Some("0.000005000000")
    );
    price.cached_input_treatment = CachePriceTreatment::Unknown;
    price.cached_input_price_per_million = None;
    assert!(price.input_equivalent(20, true)?.is_none());
    price.rate_date = "2025-02-29".into();
    assert!(price.validate().is_err());
    price.rate_date = "2024-02-29".into();
    price.validate()?;
    for invalid in ["NaN", "-1", "1e9", "0.0000001", "1000001"] {
        assert!(price_microunits(invalid).is_err());
    }
    Ok(())
}

#[test]
fn paired_prices_require_complete_counters_and_preserve_negative_differences() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("fixture.txt"), "original usage fixture")?;
    let kernel = airk_kernel::Kernel::open(root.path(), None)?;
    kernel.index()?;
    let mut event = supplemental(91);
    event.paired_task.as_mut().unwrap().with_airk.output_tokens = Some(100);
    kernel.usage_import(&event)?;
    let mut price = scenario();
    price.output_price_per_million = Some("10".into());
    let view = kernel.usage_report_with_price(30, &price)?;
    let paired = &view.price_scenario.as_ref().unwrap()["paired_cost_equivalents"][0];
    assert_eq!(paired["output_reduction"], "-0.000880000000");
    assert_eq!(paired["quality"][2], 1);
    event.event_id = format!("{:032x}", 92);
    event.paired_task.as_mut().unwrap().with_airk.output_tokens = None;
    kernel.usage_import(&event)?;
    let view = kernel.usage_report_with_price(30, &price)?;
    assert!(
        view.price_scenario.as_ref().unwrap()["paired_cost_equivalents"][0]["output_reduction"]
            .is_null()
    );
    Ok(())
}

#[test]
fn supplemental_client_import_is_separate_bounded_deduplicated_and_disabled_safely()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("fixture.txt"),
        "private_supplement_fixture",
    )?;
    let kernel = airk_kernel::Kernel::open(root.path(), None)?;
    kernel.index()?;
    let event = supplemental(1);
    assert_eq!(kernel.usage_import(&event)?["recorded"], true);
    assert_eq!(kernel.usage_import(&event)?["duplicate"], true);
    let report = kernel.usage_report_with_price(30, &scenario())?;
    assert_eq!(report.report.calls, 0);
    assert!(!report.report.provider_token_usage_observed);
    assert!(report.report.estimated_bill_savings.is_none());
    let summary = &report.supplemental[0].summaries[0];
    assert_eq!(summary.measurements, 1);
    assert_eq!(summary.token_reduction, -110);
    assert_eq!(summary.heuristic_absolute_token_error, 70);
    assert_eq!(summary.paired_quality, [0, 0, 1, 0, 0]);
    assert_eq!(summary.sessions.input_tokens, 120);
    let price = report.price_scenario.as_ref().unwrap();
    assert_eq!(
        price["context_equivalents"][0]["nominal_input_cost_equivalent"],
        "-0.000275000000"
    );
    assert_eq!(
        price["session_input_equivalents"][0]["uncached_input_equivalent"],
        "0.000250000000"
    );
    let mut unknown = supplemental(2);
    unknown.session.as_mut().unwrap().cached_input_tokens = None;
    kernel.usage_import(&unknown)?;
    let report = kernel.usage_report_with_price(30, &scenario())?;
    assert_eq!(
        report.price_scenario.unwrap()["session_input_equivalents"][0]["unpriced_input_tokens_unknown_cache_split"],
        "120"
    );
    let ledger = root
        .path()
        .join(".airk/collections-1.2/default/usage.sqlite3");
    assert!(
        !String::from_utf8_lossy(&std::fs::read(&ledger)?).contains("private_supplement_fixture")
    );
    let mut invalid = serde_json::to_value(&event)?;
    invalid["prompt"] = "must not persist".into();
    assert!(serde_json::from_value::<SupplementalMeasurement>(invalid).is_err());
    kernel.configure_usage(RecordingMode::Disabled)?;
    let before = std::fs::read(&ledger)?;
    assert!(kernel.usage_import(&supplemental(3)).is_err());
    assert_eq!(std::fs::read(&ledger)?, before);
    kernel.clear_usage()?;
    assert!(kernel.usage_report(30)?.supplemental.is_empty());
    Ok(())
}

#[test]
fn offline_bpe_calibration_counts_exact_definition_without_retaining_samples() -> anyhow::Result<()>
{
    use airk_kernel::usage::CalibrationRequest;
    let root = tempfile::tempdir()?;
    let kernel = airk_kernel::Kernel::open(root.path(), None)?;
    kernel.initialize()?;
    let tokenizer = root.path().join("tokens.json");
    let baseline = root.path().join("before.json");
    let returned = root.path().join("after.json");
    std::fs::write(
        &tokenizer,
        serde_json::to_vec(
            &serde_json::json!({"schema":1,"model_revision_sha256":"a".repeat(64),"merges":[["68","65"],["6865","6c"],["68656c","6c"],["68656c6c","6f"]]}),
        )?,
    )?;
    // JSON quote bytes count too: hello is one merged symbol, plus two quote tokens.
    std::fs::write(&baseline, "\"hellohello\"")?;
    std::fs::write(&returned, "\"hello\"")?;
    let request = |record| CalibrationRequest {
        tokenizer: &tokenizer,
        baseline: &baseline,
        returned: &returned,
        generation: 1,
        content_kind: ContentKind::PlainText,
        record,
    };
    let preview = kernel.usage_calibrate(request(false))?;
    assert_eq!(preview["summary"]["baseline_tokens"], "4");
    assert_eq!(preview["summary"]["returned_tokens"], "3");
    assert_eq!(preview["summary"]["token_reduction"], "1");
    assert!(
        !root
            .path()
            .join(".airk/collections-1.2/default/usage.sqlite3")
            .exists()
    );
    kernel.usage_calibrate(request(true))?;
    let stored = kernel.usage_report(30)?;
    assert_eq!(stored.report.calls, 0);
    assert_eq!(
        stored.supplemental[0].summaries[0].stratum.provenance,
        MeasurementProvenance::LocalTokenizer
    );
    let ledger = std::fs::read(
        root.path()
            .join(".airk/collections-1.2/default/usage.sqlite3"),
    )?;
    assert!(!String::from_utf8_lossy(&ledger).contains("hellohello"));
    // Byte fallback remains exact for multilingual text and has no hidden normalizer.
    std::fs::write(&returned, "\"é中🙂\"")?;
    assert_eq!(
        kernel.usage_calibrate(request(false))?["summary"]["returned_tokens"],
        "11"
    );
    std::fs::write(
        &tokenizer,
        serde_json::to_vec(
            &serde_json::json!({"schema":1,"model_revision_sha256":"a".repeat(64),"merges":[["6865","6c"]]}),
        )?,
    )?;
    assert!(kernel.usage_calibrate(request(false)).is_err());
    Ok(())
}

#[test]
fn estimate_is_labelled_sensitive_signed_and_overflow_safe() -> anyhow::Result<()> {
    let heuristic = ByteTokenHeuristic::default();
    let estimate = heuristic.compare(48_000, 4_800)?;
    assert_eq!(estimate.nominal_reduction, 10_800);
    assert_eq!(estimate.assumption_low, 5_600);
    assert_eq!(estimate.assumption_high, 23_200);
    assert_eq!(heuristic.compare(80, 400)?.nominal_reduction, -80);
    assert_eq!(heuristic.compare(u64::MAX, u64::MAX)?.nominal_reduction, 0);
    assert!(
        ByteTokenHeuristic {
            minimum_bytes_per_token: 0,
            ..heuristic
        }
        .compare(1, 1)
        .is_err()
    );
    Ok(())
}

#[test]
fn report_filters_scope_window_and_replays_without_claiming_billed_savings() -> anyhow::Result<()> {
    let first = observation(1);
    let duplicate = first.clone();
    let mut other = observation(2);
    other.scope.root_binding = [8; 32];
    let mut later = observation(3);
    later.unix_ms = 200;
    let report = summarize(
        &first.scope,
        0,
        200,
        [&first, &duplicate, &other, &later],
        ByteTokenHeuristic::default(),
    )?;
    assert_eq!(report.calls, 1);
    assert_eq!(report.duplicate_observations, 1);
    assert_eq!(report.outside_scope_or_window, 2);
    assert_eq!(
        report.comparisons[&BaselineKind::FullReturnedSources]
            .token_estimate
            .nominal_reduction,
        10_800
    );
    assert!(!report.provider_token_usage_observed);
    assert!(report.estimated_bill_savings.is_none());
    Ok(())
}

#[test]
fn failed_empty_missing_and_incompatible_baselines_never_inflate_reduction() -> anyhow::Result<()> {
    let mut failed = observation(1);
    failed.outcome = UsageOutcome::Failed;
    let mut empty = observation(2);
    empty.outcome = UsageOutcome::Empty;
    empty.returned_items = 0;
    let mut missing = observation(3);
    missing.baseline = None;
    let mut wrong = observation(4);
    wrong.baseline.as_mut().unwrap().generation = 3;
    let mut representation = observation(5);
    representation.baseline.as_mut().unwrap().representation =
        ResponseRepresentation::LogicalResultJson;
    let report = summarize(
        &failed.scope,
        0,
        200,
        [&failed, &empty, &missing, &wrong, &representation],
        ByteTokenHeuristic::default(),
    )?;
    assert_eq!(report.calls, 5);
    assert!(report.comparisons.is_empty());
    assert_eq!(report.excluded_from_estimation_calls, 2);
    assert_eq!(report.missing_baseline_calls, 1);
    assert_eq!(report.mismatched_baseline_calls, 2);
    assert_eq!(
        report.baseline_unavailable[&BaselineUnavailable::NotRecorded],
        1
    );
    assert_eq!(
        report.baseline_unavailable[&BaselineUnavailable::RevisionMismatch],
        2
    );
    let search = &report.operations[&UsageOperation::Search];
    assert_eq!(search.failed, 1);
    assert_eq!(search.empty, 1);
    Ok(())
}

#[test]
fn unavailable_reason_buckets_merge_and_old_reports_remain_readable() -> anyhow::Result<()> {
    let mut event = observation(100);
    event.baseline = None;
    event.baseline_unavailable = Some(BaselineUnavailable::InventoryLimit);
    let report = summarize(
        &event.scope,
        0,
        200,
        [&event],
        ByteTokenHeuristic::default(),
    )?;
    let mut old = serde_json::to_value(&report)?;
    old.as_object_mut().unwrap().remove("baseline_unavailable");
    let mut old: UsageReport = serde_json::from_value(old)?;
    assert!(old.baseline_unavailable.is_empty());
    old.merge(&report)?;
    assert_eq!(old.missing_baseline_calls, 2);
    assert_eq!(
        old.baseline_unavailable[&BaselineUnavailable::InventoryLimit],
        1
    );
    assert!(old.comparisons.is_empty());
    Ok(())
}

#[test]
fn alternative_baselines_stay_separate_and_response_overhead_can_expand_context()
-> anyhow::Result<()> {
    let full = observation(1);
    let mut observed = observation(2);
    let baseline = observed.baseline.as_mut().unwrap();
    baseline.kind = BaselineKind::ObservedAlternative;
    baseline.utf8_bytes = 80;
    observed.response_utf8_bytes = Some(400);
    let report = summarize(
        &full.scope,
        0,
        200,
        [&full, &observed],
        ByteTokenHeuristic::default(),
    )?;
    assert_eq!(report.comparisons.len(), 2);
    assert_eq!(
        report.comparisons[&BaselineKind::ObservedAlternative]
            .token_estimate
            .nominal_reduction,
        -80
    );
    Ok(())
}

#[test]
fn index_reuse_and_latency_are_measured_separately_from_token_estimates() -> anyhow::Result<()> {
    let mut first = observation(1);
    first.operation = UsageOperation::Index;
    first.cache = CacheObservation::NotApplicable;
    first.index_work = Some(IndexWork {
        indexed: 1,
        reused: 9,
        source_bytes: u64::MAX,
        index_disk_bytes: Some(80),
        index_disk_growth_bytes: Some(-10),
        scan_extract_us: Some(4),
        publish_us: Some(2),
        metrics: [
            (IndexMetric::EmbeddingActualForwards, 0),
            (IndexMetric::EmbeddingInputs, 12),
        ]
        .into_iter()
        .collect(),
        ..IndexWork::default()
    });
    let mut second = first.clone();
    second.event_id = [2; 16];
    second.elapsed_us = 16_000_000;
    second.index_work.as_mut().unwrap().metrics.clear();
    let report = summarize(
        &first.scope,
        0,
        200,
        [&first, &second],
        ByteTokenHeuristic::default(),
    )?;
    assert!(report.comparisons.is_empty());
    let index = &report.operations[&UsageOperation::Index];
    assert_eq!(index.index_work.reused, 18);
    assert_eq!(
        index.index_work.processed_source_bytes,
        2 * u128::from(u64::MAX)
    );
    assert_eq!(index.cache_not_applicable, 2);
    assert_eq!(index.index_work.disk_samples, 2);
    assert_eq!(index.index_work.disk_growth_bytes, -20);
    assert_eq!(index.index_work.scan_extract_samples, 2);
    assert_eq!(index.index_work.scan_extract_us, 8);
    assert_eq!(index.index_work.publish_us, 4);
    let forwards = &index.index_work.metrics[&IndexMetric::EmbeddingActualForwards];
    assert_eq!((forwards.samples, forwards.value), (1, 0));
    assert_eq!(
        index.index_work.metrics[&IndexMetric::EmbeddingInputs].value,
        12
    );
    assert!(
        !index
            .index_work
            .metrics
            .contains_key(&IndexMetric::EmbeddingInferenceUs)
    );
    let mut merged = report.clone();
    merged.merge(&report)?;
    let inputs = &merged.operations[&UsageOperation::Index].index_work.metrics
        [&IndexMetric::EmbeddingInputs];
    assert_eq!((inputs.samples, inputs.value), (2, 24));
    assert_eq!(index.latency_buckets[1], 1);
    assert_eq!(index.latency_buckets[7], 1);
    assert_eq!(UsagePolicy::default().mode, RecordingMode::AggregatesOnly);
    Ok(())
}

#[test]
fn disabled_hooks_leave_usage_history_untouched() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("fixture.txt"), "original fixture")?;
    let kernel = airk_kernel::Kernel::open(root.path(), None)?;
    kernel.index()?;
    kernel.configure_usage(RecordingMode::Disabled)?;
    let policy = root
        .path()
        .join(".airk/collections-1.2/default/usage-policy.json");
    let before = std::fs::read(&policy)?;
    let span = kernel.usage_span(UsageOperation::Status, ClientFamily::Cli);
    assert!(span.request_id().is_none());
    let value = serde_json::to_value(kernel.status()?)?;
    kernel.finish_usage(
        span,
        Some(&value),
        100,
        false,
        ResponseRepresentation::LogicalResultJson,
    );
    assert_eq!(std::fs::read(policy)?, before);
    assert!(
        !root
            .path()
            .join(".airk/collections-1.2/default/usage.sqlite3")
            .exists()
    );
    assert_eq!(kernel.usage_report(30)?.status, "disabled");
    Ok(())
}

#[test]
fn export_encodes_wide_totals_as_decimal_strings() -> anyhow::Result<()> {
    let mut event = observation(1);
    event.operation = UsageOperation::Index;
    event.index_work = Some(IndexWork {
        source_bytes: u64::MAX,
        ..Default::default()
    });
    let mut second = event.clone();
    second.event_id = [2; 16];
    let report = summarize(
        &event.scope,
        0,
        200,
        [&event, &second],
        ByteTokenHeuristic::default(),
    )?;
    let value = serde_json::to_value(&report)?;
    assert_eq!(
        value["operations"]["index"]["index_work"]["processed_source_bytes"],
        (2 * u128::from(u64::MAX)).to_string()
    );
    let decoded: airk_kernel::usage::UsageReport = serde_json::from_value(value)?;
    assert_eq!(decoded.calls, 2);
    Ok(())
}

#[test]
fn busy_recorder_cannot_change_successful_retrieval() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("fixture.txt"), "original budget fixture")?;
    let kernel = airk_kernel::Kernel::open(root.path(), None)?;
    kernel.index()?;
    let guard =
        airk_store::usage::deletion_guard(&root.path().join(".airk/collections-1.2/default"))?;
    let span = kernel.usage_span(UsageOperation::Search, ClientFamily::Cli);
    let response = kernel.search(&airk_core::SearchQuery {
        text: "budget".into(),
        path_prefix: None,
        limit: 1,
        max_bytes: 1024,
    })?;
    assert_eq!(response.hits.len(), 1);
    let value = serde_json::to_value(&response)?;
    kernel.finish_usage(
        span,
        Some(&value),
        100,
        false,
        ResponseRepresentation::LogicalResultJson,
    );
    drop(guard);
    assert_eq!(kernel.usage_report(30)?.report.calls, 0);
    assert_eq!(kernel.status()?.generation, response.generation);
    Ok(())
}

#[test]
fn native_jobs_record_actual_work_once_in_the_jobs_collection() -> anyhow::Result<()> {
    const CHILD_MODE: &str = "AIRK_USAGE_JOB_ACCOUNTING_TEST";
    let Ok(mode) = std::env::var(CHILD_MODE) else {
        // Recording health is process-wide. Isolate it from other usage tests so
        // an unrelated dropped observation cannot satisfy this job's assertions.
        for mode in ["uncontended", "reused_busy", "failed_busy"] {
            let output = std::process::Command::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "usage::native_jobs_record_actual_work_once_in_the_jobs_collection",
                    "--nocapture",
                ])
                .env(CHILD_MODE, mode)
                .output()?;
            assert!(
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains("running 1 test"),
                "{mode}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return Ok(());
    };
    assert!(["uncontended", "reused_busy", "failed_busy"].contains(&mode.as_str()));
    let root = tempfile::tempdir()?;
    let host_root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("fixture.txt"),
        "original scheduled fixture",
    )?;
    let target =
        airk_kernel::Kernel::open_collection(root.path(), Some(state.path().into()), "target")?;
    let host =
        airk_kernel::Kernel::open_collection(host_root.path(), Some(state.path().into()), "host")?;
    let before = target.usage_report(30)?;
    assert_eq!(before.report.calls, 0);
    assert_eq!(before.process_dropped_observations, 0);
    let first = target.scheduled_index(false, false)?;
    assert_eq!(first.indexed, 1);
    let after_first = target.usage_report(30)?;
    assert_job_observation(&before, &after_first, Some(&first), false)?;
    let queue = airk_store::scheduler::Queue::new(state.path());
    let root_id = target.root().to_string_lossy().into_owned();
    queue.enqueue(
        "target",
        &root_id,
        false,
        true,
        false,
        airk_store::usage::unix_ms(),
    )?;
    let directory = state.path().join("collections-1.2/target");
    let usage_guard = (mode == "reused_busy")
        .then(|| airk_store::usage::deletion_guard(&directory))
        .transpose()?;
    let reused = host.run_scheduled_once()?;
    drop(usage_guard);
    assert_eq!(reused.state, "indexed");
    let reused = reused.report.expect("successful queued job has its report");
    assert_eq!(reused.reused, 1);
    let after_reused = target.usage_report(30)?;
    assert_job_observation(
        &after_first,
        &after_reused,
        Some(&reused),
        mode == "reused_busy",
    )?;
    assert_eq!(host.run_scheduled_once()?.state, "idle");
    let after_idle = target.usage_report(30)?;
    assert_eq!(after_idle.report.calls, after_reused.report.calls);
    assert_eq!(
        after_idle.process_dropped_observations, after_reused.process_dropped_observations,
        "an idle worker must not attempt to record another job"
    );
    let writer = airk_store::FileStore::new(state.path().join("collections-1.2/target")).lock()?;
    queue.enqueue(
        "target",
        &root_id,
        false,
        true,
        false,
        airk_store::usage::unix_ms(),
    )?;
    assert_eq!(host.run_scheduled_once()?.state, "busy");
    let after_contention = target.usage_report(30)?;
    assert_eq!(after_contention.report.calls, after_idle.report.calls);
    assert_eq!(
        after_contention.process_dropped_observations,
        after_idle.process_dropped_observations
    );
    drop(writer);
    // An incompatible legacy marker is a real failure, unlike a live writer's
    // transient advisory lock. It must still be recorded and block the job.
    std::fs::write(
        directory.join("writer.lock"),
        "original legacy writer fixture",
    )?;
    let usage_guard = (mode == "failed_busy")
        .then(|| airk_store::usage::deletion_guard(&directory))
        .transpose()?;
    let failed = host.run_scheduled_once()?;
    drop(usage_guard);
    assert_eq!(failed.state, "failed");
    assert!(
        failed
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("legacy index writer locked")
    );
    std::fs::remove_file(directory.join("writer.lock"))?;
    let report = target.usage_report(30)?;
    assert_job_observation(&after_idle, &report, None, mode == "failed_busy")?;
    assert_eq!(
        report.report.calls + report.process_dropped_observations,
        3,
        "each executed job must be recorded exactly once or explicitly dropped"
    );
    assert!(report.report.comparisons.is_empty());
    assert_eq!(host.usage_report(30)?.report.calls, 0);
    // Ensure there is a ledger to compare even if all optional writes were
    // dropped. This creates no observations and does not retry any queue job.
    airk_store::usage::UsageStore::open(
        &directory,
        &target.usage_scope(),
        &target.usage_policy()?,
    )?
    .expect("recording is enabled");
    target.configure_usage(RecordingMode::Disabled)?;
    let ledger = state.path().join("collections-1.2/target/usage.sqlite3");
    let before = std::fs::read(&ledger)?;
    target.scheduled_index(false, false)?;
    assert_eq!(
        std::fs::read(ledger)?,
        before,
        "disabled native jobs must leave the ledger unchanged"
    );
    let disabled = target.usage_report(30)?;
    assert_eq!(disabled.report.calls, report.report.calls);
    assert_eq!(
        disabled.process_dropped_observations, report.process_dropped_observations,
        "disabled recording is an opt-out, not a dropped observation"
    );
    Ok(())
}

fn assert_job_observation(
    before: &airk_kernel::usage::UsageView,
    after: &airk_kernel::usage::UsageView,
    actual_work: Option<&airk_ingest::IndexReport>,
    forced_drop: bool,
) -> anyhow::Result<()> {
    let recorded = after.report.calls - before.report.calls;
    let dropped = after.process_dropped_observations - before.process_dropped_observations;
    assert_eq!(
        (recorded + dropped, recorded.min(dropped)),
        (1, 0),
        "one job must produce exactly one observation or one visible recording failure"
    );
    if forced_drop {
        assert_eq!(dropped, 1, "the held usage lock must prevent recording");
    }
    if dropped == 1 {
        assert_eq!(
            serde_json::to_value(&after.report.operations)?,
            serde_json::to_value(&before.report.operations)?,
            "a dropped observation must not partially update work totals"
        );
        assert_eq!(after.report.clients, before.report.clients);
        return Ok(());
    }
    assert_eq!(after.report.clients.len(), 1);
    assert_eq!(
        after.report.clients[&ClientFamily::NativeScheduler],
        after.report.calls
    );
    assert_eq!(after.report.operations.len(), 1);
    let previous = before
        .report
        .operations
        .get(&UsageOperation::Index)
        .cloned()
        .unwrap_or_default();
    let current = &after.report.operations[&UsageOperation::Index];
    assert_eq!(current.calls, previous.calls + 1);
    assert_eq!(
        current.successful,
        previous.successful + u64::from(actual_work.is_some())
    );
    assert_eq!(
        current.failed,
        previous.failed + u64::from(actual_work.is_none())
    );
    assert_eq!(current.empty, 0);
    let expected = actual_work.map_or([0; 5], |work| {
        [
            work.indexed as u128,
            work.reused as u128,
            work.removed as u128,
            (work.skipped.len() + work.skipped_truncated) as u128,
            work.source_bytes as u128,
        ]
    });
    let old = &previous.index_work;
    let new = &current.index_work;
    assert_eq!(
        [
            new.indexed - old.indexed,
            new.reused - old.reused,
            new.removed - old.removed,
            new.skipped - old.skipped,
            new.processed_source_bytes - old.processed_source_bytes,
        ],
        expected,
        "recorded work must match this exact job's returned report"
    );
    Ok(())
}
