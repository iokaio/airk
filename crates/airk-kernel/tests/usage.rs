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
    assert_eq!(index.latency_buckets[1], 1);
    assert_eq!(index.latency_buckets[7], 1);
    assert_eq!(UsagePolicy::default().mode, RecordingMode::AggregatesOnly);
    Ok(())
}
