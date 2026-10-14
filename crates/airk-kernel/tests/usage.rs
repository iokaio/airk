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
        baseline: Some(ContextBaseline {
            kind: BaselineKind::FullReturnedSources,
            generation: 4,
            representation: ResponseRepresentation::McpToolResultJson,
            utf8_bytes: 48_000,
            unique_sources: 2,
        }),
        index_work: None,
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

fn supplemental(id: u8) -> SupplementalMeasurement {
    SupplementalMeasurement {
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
    let ledger = root.path().join(".airk/collections/default/usage.sqlite3");
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
            .join(".airk/collections/default/usage.sqlite3")
            .exists()
    );
    kernel.usage_calibrate(request(true))?;
    let stored = kernel.usage_report(30)?;
    assert_eq!(stored.report.calls, 0);
    assert_eq!(
        stored.supplemental[0].summaries[0].stratum.provenance,
        MeasurementProvenance::LocalTokenizer
    );
    let ledger = std::fs::read(root.path().join(".airk/collections/default/usage.sqlite3"))?;
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
    let search = &report.operations[&UsageOperation::Search];
    assert_eq!(search.failed, 1);
    assert_eq!(search.empty, 1);
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
        ..IndexWork::default()
    });
    let mut second = first.clone();
    second.event_id = [2; 16];
    second.elapsed_us = 16_000_000;
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
    assert_eq!(index.latency_buckets[1], 1);
    assert_eq!(index.latency_buckets[7], 1);
    assert_eq!(UsagePolicy::default().mode, RecordingMode::AggregatesOnly);
    Ok(())
}

#[test]
fn live_adapter_span_records_once_with_same_revision_baseline_and_no_content() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("memo.md"),
        "# Private fixture\nconfidential_zircon_word\nA larger piece of original fixture evidence.\n",
    )?;
    let kernel = airk_kernel::Kernel::open(root.path(), None)?;
    kernel.index()?;
    let span = kernel.usage_span(UsageOperation::Search, ClientFamily::Codex);
    let request_id = span.request_id().unwrap();
    let result = kernel.search(&airk_core::SearchQuery {
        text: "confidential_zircon_word".into(),
        path_prefix: None,
        limit: 1,
        max_bytes: 80,
    })?;
    let value = serde_json::to_value(&result)?;
    kernel.finish_usage(
        span,
        Some(&value),
        serde_json::to_vec(&value)?.len(),
        false,
        ResponseRepresentation::LogicalResultJson,
    );
    let report = kernel.usage_report(30)?;
    assert_eq!(report.report.calls, 1);
    assert_eq!(
        report.report.comparisons[&BaselineKind::FullReturnedSources].compared_calls,
        1
    );
    assert_eq!(
        kernel.usage_report(30)?.report.calls,
        1,
        "reports must not count themselves"
    );
    let ledger = std::fs::read(root.path().join(".airk/collections/default/usage.sqlite3"))?;
    assert!(!String::from_utf8_lossy(&ledger).contains("confidential_zircon_word"));
    assert!(!String::from_utf8_lossy(&ledger).contains("memo.md"));
    kernel.usage_feedback(&request_id, Usefulness::Useful)?;
    assert_eq!(kernel.usage_report(30)?.health.useful_feedback, 1);
    kernel.usage_feedback(&request_id, Usefulness::NotUseful)?;
    let updated = kernel.usage_report(30)?;
    assert_eq!(updated.health.useful_feedback, 0);
    assert_eq!(updated.health.not_useful_feedback, 1);
    assert!(
        kernel
            .usage_feedback("00000000000000000000000000000000", Usefulness::Useful)
            .is_err()
    );
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
        .join(".airk/collections/default/usage-policy.json");
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
            .join(".airk/collections/default/usage.sqlite3")
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
    let guard = airk_store::usage::deletion_guard(&root.path().join(".airk/collections/default"))?;
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
    let first = target.scheduled_index(false, false)?;
    assert_eq!(first.indexed, 1);
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
    let reused = host.run_scheduled_once()?;
    assert_eq!(reused.state, "indexed");
    assert_eq!(reused.report.unwrap().reused, 1);
    assert_eq!(host.run_scheduled_once()?.state, "idle");
    let writer = airk_store::FileStore::new(state.path().join("collections/target")).lock()?;
    queue.enqueue(
        "target",
        &root_id,
        false,
        true,
        false,
        airk_store::usage::unix_ms(),
    )?;
    assert_eq!(host.run_scheduled_once()?.state, "failed");
    drop(writer);
    let report = target.usage_report(30)?;
    assert_eq!(report.report.calls, 3);
    assert_eq!(report.report.clients[&ClientFamily::NativeScheduler], 3);
    let work = &report.report.operations[&UsageOperation::Index];
    assert_eq!(work.successful, 2);
    assert_eq!(work.failed, 1);
    assert_eq!(work.index_work.indexed, 1);
    assert_eq!(work.index_work.reused, 1);
    assert!(report.report.comparisons.is_empty());
    assert_eq!(host.usage_report(30)?.report.calls, 0);
    target.configure_usage(RecordingMode::Disabled)?;
    let ledger = state.path().join("collections/target/usage.sqlite3");
    let before = std::fs::read(&ledger)?;
    target.scheduled_index(false, false)?;
    assert_eq!(
        std::fs::read(ledger)?,
        before,
        "disabled native jobs must leave the ledger unchanged"
    );
    Ok(())
}
