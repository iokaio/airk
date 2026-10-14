// SPDX-License-Identifier: Apache-2.0
//! Tested report/estimation foundation; no live recorder is attached to Kernel yet.
use airk_core::usage::{
    BaselineKind, CacheObservation, ClientFamily, ContentKind, USAGE_SCHEMA_VERSION,
    UsageObservation, UsageOperation, UsageOutcome, UsageScope,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Serialize)]
pub struct IndexSummary {
    pub indexed: u128,
    pub reused: u128,
    pub removed: u128,
    pub skipped: u128,
    pub processed_source_bytes: u128,
}

/// Explicit sensitivity assumptions, not a calibrated confidence interval.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ByteTokenHeuristic {
    pub minimum_bytes_per_token: u32,
    pub nominal_bytes_per_token: u32,
    pub maximum_bytes_per_token: u32,
}

impl Default for ByteTokenHeuristic {
    fn default() -> Self {
        Self {
            minimum_bytes_per_token: 2,
            nominal_bytes_per_token: 4,
            maximum_bytes_per_token: 6,
        }
    }
}

impl ByteTokenHeuristic {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.minimum_bytes_per_token > 0
                && self.minimum_bytes_per_token <= self.nominal_bytes_per_token
                && self.nominal_bytes_per_token <= self.maximum_bytes_per_token,
            "token heuristic ratios must be nonzero and ordered"
        );
        Ok(())
    }

    pub fn compare(self, baseline_bytes: u64, returned_bytes: u64) -> Result<TokenComparison> {
        self.validate()?;
        let count = |bytes: u64, ratio: u32| i128::from(bytes.div_ceil(u64::from(ratio)));
        Ok(TokenComparison {
            nominal_reduction: count(baseline_bytes, self.nominal_bytes_per_token)
                - count(returned_bytes, self.nominal_bytes_per_token),
            assumption_low: count(baseline_bytes, self.maximum_bytes_per_token)
                - count(returned_bytes, self.minimum_bytes_per_token),
            assumption_high: count(baseline_bytes, self.minimum_bytes_per_token)
                - count(returned_bytes, self.maximum_bytes_per_token),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenComparison {
    /// Negative values honestly describe expansion instead of clamping to zero.
    pub nominal_reduction: i128,
    pub assumption_low: i128,
    pub assumption_high: i128,
}

impl TokenComparison {
    fn add(&mut self, other: Self) -> Result<()> {
        self.nominal_reduction = self
            .nominal_reduction
            .checked_add(other.nominal_reduction)
            .ok_or_else(|| anyhow::anyhow!("token estimate overflow"))?;
        self.assumption_low = self
            .assumption_low
            .checked_add(other.assumption_low)
            .ok_or_else(|| anyhow::anyhow!("token estimate overflow"))?;
        self.assumption_high = self
            .assumption_high
            .checked_add(other.assumption_high)
            .ok_or_else(|| anyhow::anyhow!("token estimate overflow"))?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct OperationSummary {
    pub calls: u64,
    pub successful: u64,
    pub empty: u64,
    pub failed: u64,
    pub not_ready: u64,
    pub cancelled: u64,
    pub capacity_limited: u64,
    pub truncated: u64,
    pub total_elapsed_us: u128,
    /// Fixed upper-bound buckets; the final bucket includes durations >= 16 s.
    pub latency_buckets: [u64; 8],
    pub warm_cache: u64,
    pub cold_cache: u64,
    pub unknown_cache: u64,
    pub cache_not_applicable: u64,
    pub returned_utf8_bytes: u128,
    pub measured_responses: u64,
    pub index_work: IndexSummary,
}

impl OperationSummary {
    fn observe(&mut self, event: &UsageObservation) {
        self.calls += 1;
        match event.outcome {
            UsageOutcome::Success => self.successful += 1,
            UsageOutcome::Empty => self.empty += 1,
            UsageOutcome::Failed => self.failed += 1,
            UsageOutcome::NotReady => self.not_ready += 1,
            UsageOutcome::Cancelled => self.cancelled += 1,
            UsageOutcome::CapacityLimited => self.capacity_limited += 1,
        }
        self.truncated += u64::from(event.truncated);
        self.total_elapsed_us += u128::from(event.elapsed_us);
        let bounds = [
            1_000, 5_000, 20_000, 100_000, 500_000, 2_000_000, 16_000_000,
        ];
        self.latency_buckets[bounds
            .iter()
            .position(|upper| event.elapsed_us < *upper)
            .unwrap_or(7)] += 1;
        match event.cache {
            CacheObservation::Warm => self.warm_cache += 1,
            CacheObservation::Cold => self.cold_cache += 1,
            CacheObservation::Unknown => self.unknown_cache += 1,
            CacheObservation::NotApplicable => self.cache_not_applicable += 1,
        }
        if let Some(bytes) = event.response_utf8_bytes {
            self.measured_responses += 1;
            self.returned_utf8_bytes += u128::from(bytes);
        }
        if let Some(index) = &event.index_work {
            self.index_work.indexed += u128::from(index.indexed);
            self.index_work.reused += u128::from(index.reused);
            self.index_work.removed += u128::from(index.removed);
            self.index_work.skipped += u128::from(index.skipped);
            self.index_work.processed_source_bytes += u128::from(index.source_bytes);
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BaselineSummary {
    pub compared_calls: u64,
    pub baseline_utf8_bytes: u128,
    pub returned_utf8_bytes: u128,
    pub token_estimate: TokenComparison,
}

#[derive(Clone, Debug, Serialize)]
pub struct UsageReport {
    pub schema_version: u32,
    pub scope: UsageScope,
    pub start_unix_ms: u64,
    pub end_unix_ms_exclusive: u64,
    pub estimator_version: String,
    pub heuristic: ByteTokenHeuristic,
    pub calls: u64,
    pub operations: BTreeMap<UsageOperation, OperationSummary>,
    pub clients: BTreeMap<ClientFamily, u64>,
    pub content_kinds: BTreeMap<ContentKind, u64>,
    /// Counterfactual and observed alternatives stay separate; no mixed headline.
    pub comparisons: BTreeMap<BaselineKind, BaselineSummary>,
    pub eligible_evidence_calls: u64,
    pub missing_baseline_calls: u64,
    pub mismatched_baseline_calls: u64,
    pub excluded_from_estimation_calls: u64,
    pub outside_scope_or_window: u64,
    pub duplicate_observations: u64,
    /// None means collector/retention coverage was not supplied, rather than zero loss.
    pub dropped_observations: Option<u64>,
    pub earliest_retained_unix_ms: Option<u64>,
    pub provider_token_usage_observed: bool,
    pub estimated_bill_savings: Option<String>,
}

/// Replayed event IDs are ignored; durable deduplication across batches remains a store responsibility.
/// Bounded batches only: the future report repository will merge daily aggregates.
pub fn summarize<'a>(
    scope: &UsageScope,
    start: u64,
    end: u64,
    observations: impl IntoIterator<Item = &'a UsageObservation>,
    heuristic: ByteTokenHeuristic,
) -> Result<UsageReport> {
    ensure!(end > start, "usage report window must be nonempty");
    heuristic.validate()?;
    let mut report = UsageReport {
        schema_version: USAGE_SCHEMA_VERSION,
        scope: scope.clone(),
        start_unix_ms: start,
        end_unix_ms_exclusive: end,
        estimator_version: "utf8-byte-assumptions/1".into(),
        heuristic,
        calls: 0,
        operations: BTreeMap::new(),
        clients: BTreeMap::new(),
        content_kinds: BTreeMap::new(),
        comparisons: BTreeMap::new(),
        eligible_evidence_calls: 0,
        missing_baseline_calls: 0,
        mismatched_baseline_calls: 0,
        excluded_from_estimation_calls: 0,
        outside_scope_or_window: 0,
        duplicate_observations: 0,
        dropped_observations: None,
        earliest_retained_unix_ms: None,
        provider_token_usage_observed: false,
        estimated_bill_savings: None,
    };
    let mut seen = BTreeSet::new();
    for (ordinal, event) in observations.into_iter().enumerate() {
        ensure!(
            ordinal < 100_000,
            "usage input batch exceeds 100000 observations; use daily aggregates"
        );
        if event.scope != *scope || event.unix_ms < start || event.unix_ms >= end {
            report.outside_scope_or_window += 1;
            continue;
        }
        ensure!(
            event.schema_version == USAGE_SCHEMA_VERSION,
            "unsupported usage schema"
        );
        if !seen.insert(event.event_id) {
            report.duplicate_observations += 1;
            continue;
        }
        report.calls += 1;
        report
            .operations
            .entry(event.operation)
            .or_default()
            .observe(event);
        *report.clients.entry(event.client).or_default() += 1;
        *report.content_kinds.entry(event.content_kind).or_default() += 1;
        if !event.operation.returns_source_evidence()
            || event.outcome != UsageOutcome::Success
            || event.returned_items == 0
        {
            report.excluded_from_estimation_calls += 1;
            continue;
        }
        report.eligible_evidence_calls += 1;
        let (Some(baseline), Some(returned)) = (&event.baseline, event.response_utf8_bytes) else {
            report.missing_baseline_calls += 1;
            continue;
        };
        if Some(baseline.generation) != event.generation
            || baseline.representation != event.response_representation
            || baseline.unique_sources == 0
        {
            report.mismatched_baseline_calls += 1;
            continue;
        }
        let compared = report.comparisons.entry(baseline.kind).or_default();
        compared.compared_calls += 1;
        compared.baseline_utf8_bytes += u128::from(baseline.utf8_bytes);
        compared.returned_utf8_bytes += u128::from(returned);
        compared
            .token_estimate
            .add(heuristic.compare(baseline.utf8_bytes, returned)?)?;
    }
    Ok(report)
}
