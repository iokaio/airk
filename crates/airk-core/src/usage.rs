// SPDX-License-Identifier: Apache-2.0
//! Local usage contracts. No queries, paths, source text, prompts or credentials.
//! Bounded local aggregates and explicitly labelled counterfactual estimates.
use serde::{Deserialize, Serialize};

pub const USAGE_SCHEMA_VERSION: u32 = 1;

macro_rules! wide_number {
    ($module:ident, $ty:ty) => {
        mod $module {
            use serde::{Deserialize, Deserializer, Serializer};
            pub fn serialize<S: Serializer>(value: &$ty, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&value.to_string())
            }
            pub fn deserialize<'de, D: Deserializer<'de>>(
                deserializer: D,
            ) -> Result<$ty, D::Error> {
                String::deserialize(deserializer)?
                    .parse()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}
wide_number!(wide_u128, u128);
wide_number!(wide_i128, i128);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingMode {
    Disabled,
    AggregatesOnly,
    DiagnosticEvents,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsagePolicy {
    pub mode: RecordingMode,
    pub aggregate_retention_days: u16,
    pub diagnostic_retention_hours: u16,
    pub max_collection_bytes: u64,
    pub max_data_root_bytes: u64,
}

impl Default for UsagePolicy {
    fn default() -> Self {
        Self {
            mode: RecordingMode::AggregatesOnly,
            aggregate_retention_days: 30,
            diagnostic_retention_hours: 24,
            max_collection_bytes: 1_048_576,
            max_data_root_bytes: 33_554_432,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageScope {
    pub collection: String,
    /// Digest of the canonical root binding; never the original filesystem path.
    pub root_binding: [u8; 32],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum UsageOperation {
    Search,
    ReadFile,
    ListFiles,
    Status,
    FileInfo,
    Outline,
    Symbols,
    Imports,
    DocumentFacts,
    References,
    Context,
    Capabilities,
    Index,
    Reindex,
    CollectionMaintenance,
    UsageReport,
    CodeFacts,
    Relationships,
    Coverage,
}

impl UsageOperation {
    pub fn returns_source_evidence(self) -> bool {
        matches!(
            self,
            Self::Search
                | Self::ReadFile
                | Self::Outline
                | Self::Symbols
                | Self::Imports
                | Self::DocumentFacts
                | Self::References
                | Self::Context
                | Self::CodeFacts
                | Self::Relationships
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ClientFamily {
    Unknown,
    NativeScheduler,
    Cli,
    Vscode,
    ClaudeCode,
    Codex,
    Cursor,
    Gemini,
    Cline,
    Roo,
    Junie,
    CopilotCli,
    OpenCode,
    Kilo,
    AmazonQ,
    Windsurf,
}

impl ClientFamily {
    /// Normalize informational labels; arbitrary client strings are never stored.
    pub fn from_label(label: &str) -> Self {
        match label.to_ascii_lowercase().as_str() {
            "cli" => Self::Cli,
            "vscode" => Self::Vscode,
            "claude" | "claude-code" | "claude_code" => Self::ClaudeCode,
            "codex" => Self::Codex,
            "cursor" => Self::Cursor,
            "gemini" => Self::Gemini,
            "cline" => Self::Cline,
            "roo" => Self::Roo,
            "junie" => Self::Junie,
            "copilot-cli" => Self::CopilotCli,
            "opencode" => Self::OpenCode,
            "kilo" => Self::Kilo,
            "amazon-q" => Self::AmazonQ,
            "windsurf" => Self::Windsurf,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Code,
    StructuredText,
    Markdown,
    Pdf,
    Docx,
    Xlsx,
    Pptx,
    Svg,
    PlainText,
    Mixed,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageOutcome {
    Success,
    Empty,
    Failed,
    NotReady,
    Cancelled,
    CapacityLimited,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheObservation {
    Warm,
    Cold,
    Unknown,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseRepresentation {
    LogicalResultJson,
    McpToolResultJson,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BaselineKind {
    FullReturnedSources,
    ObservedAlternative,
    UserSupplied,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContextBaseline {
    pub kind: BaselineKind,
    pub generation: u64,
    pub representation: ResponseRepresentation,
    /// Normalized, serialized textual context, including comparable metadata.
    /// Never compressed PDF/OOXML/SVG image size or the entire workspace size.
    pub utf8_bytes: u64,
    pub unique_sources: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IndexWork {
    pub indexed: u64,
    pub reused: u64,
    pub removed: u64,
    pub skipped: u64,
    pub source_bytes: u64,
    pub index_disk_bytes: Option<u64>,
    #[serde(default)]
    pub index_disk_growth_bytes: Option<i64>,
    #[serde(default)]
    pub scan_extract_us: Option<u64>,
    #[serde(default)]
    pub publish_us: Option<u64>,
}

/// A transient completed-request observation. Detailed persistence is opt-in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageObservation {
    pub schema_version: u32,
    pub event_id: [u8; 16],
    pub scope: UsageScope,
    pub unix_ms: u64,
    pub generation: Option<u64>,
    pub operation: UsageOperation,
    /// An informational, self-reported family; never used for authorization.
    pub client: ClientFamily,
    pub content_kind: ContentKind,
    pub outcome: UsageOutcome,
    /// Monotonic duration through serialization, excluding provider/model work.
    pub elapsed_us: u64,
    pub cache: CacheObservation,
    pub returned_items: u32,
    pub truncated: bool,
    pub response_representation: ResponseRepresentation,
    pub response_utf8_bytes: Option<u64>,
    pub baseline: Option<ContextBaseline>,
    pub index_work: Option<IndexWork>,
}

/// Local recorder: callers must handle failure without failing retrieval.
pub trait UsageRecorder: Send + Sync {
    fn record(&self, observation: &UsageObservation) -> anyhow::Result<()>;
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Usefulness {
    Useful,
    PartlyUseful,
    NotUseful,
}

/// Optional explicit feedback; absence is unknown rather than an inferred success.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageFeedback {
    pub scope: UsageScope,
    pub event_id: [u8; 16],
    pub usefulness: Usefulness,
}

use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IndexSummary {
    #[serde(with = "wide_u128")]
    pub indexed: u128,
    #[serde(with = "wide_u128")]
    pub reused: u128,
    #[serde(with = "wide_u128")]
    pub removed: u128,
    #[serde(with = "wide_u128")]
    pub skipped: u128,
    #[serde(with = "wide_u128")]
    pub processed_source_bytes: u128,
    #[serde(default)]
    pub disk_samples: u64,
    #[serde(default, with = "wide_u128")]
    pub measured_disk_bytes: u128,
    #[serde(default)]
    pub disk_growth_samples: u64,
    #[serde(default, with = "wide_i128")]
    pub disk_growth_bytes: i128,
    #[serde(default)]
    pub scan_extract_samples: u64,
    #[serde(default, with = "wide_u128")]
    pub scan_extract_us: u128,
    #[serde(default)]
    pub publish_samples: u64,
    #[serde(default, with = "wide_u128")]
    pub publish_us: u128,
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
    #[serde(with = "wide_i128")]
    pub nominal_reduction: i128,
    #[serde(with = "wide_i128")]
    pub assumption_low: i128,
    #[serde(with = "wide_i128")]
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

impl UsageReport {
    /// Merge bounded aggregate buckets without retaining request-level measurements.
    pub fn merge(&mut self, other: &Self) -> Result<()> {
        ensure!(
            self.scope == other.scope
                && self.heuristic == other.heuristic
                && self.estimator_version == other.estimator_version,
            "incompatible usage aggregate"
        );
        macro_rules! add {
            ($target:expr, $source:expr, $($field:ident),+ $(,)?) => { $(
                $target.$field = $target.$field.checked_add($source.$field)
                    .ok_or_else(|| anyhow::anyhow!("usage aggregate overflow"))?;
            )+ };
        }
        add!(
            self,
            other,
            calls,
            eligible_evidence_calls,
            missing_baseline_calls,
            mismatched_baseline_calls,
            excluded_from_estimation_calls,
            outside_scope_or_window,
            duplicate_observations
        );
        for (key, source) in &other.operations {
            let target = self.operations.entry(*key).or_default();
            add!(
                target,
                source,
                calls,
                successful,
                empty,
                failed,
                not_ready,
                cancelled,
                capacity_limited,
                truncated,
                total_elapsed_us,
                warm_cache,
                cold_cache,
                unknown_cache,
                cache_not_applicable,
                returned_utf8_bytes,
                measured_responses
            );
            for (target, source) in target
                .latency_buckets
                .iter_mut()
                .zip(source.latency_buckets)
            {
                *target = target
                    .checked_add(source)
                    .ok_or_else(|| anyhow::anyhow!("usage aggregate overflow"))?;
            }
            add!(
                target.index_work,
                source.index_work,
                indexed,
                reused,
                removed,
                skipped,
                processed_source_bytes,
                disk_samples,
                measured_disk_bytes,
                disk_growth_samples,
                disk_growth_bytes,
                scan_extract_samples,
                scan_extract_us,
                publish_samples,
                publish_us
            );
        }
        for (key, value) in &other.clients {
            let total = self.clients.entry(*key).or_default();
            *total = total
                .checked_add(*value)
                .ok_or_else(|| anyhow::anyhow!("usage aggregate overflow"))?;
        }
        for (key, value) in &other.content_kinds {
            let total = self.content_kinds.entry(*key).or_default();
            *total = total
                .checked_add(*value)
                .ok_or_else(|| anyhow::anyhow!("usage aggregate overflow"))?;
        }
        for (key, source) in &other.comparisons {
            let target = self.comparisons.entry(*key).or_default();
            add!(
                target,
                source,
                compared_calls,
                baseline_utf8_bytes,
                returned_utf8_bytes
            );
            target.token_estimate.add(source.token_estimate)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OperationSummary {
    pub calls: u64,
    pub successful: u64,
    pub empty: u64,
    pub failed: u64,
    pub not_ready: u64,
    pub cancelled: u64,
    pub capacity_limited: u64,
    pub truncated: u64,
    #[serde(with = "wide_u128")]
    pub total_elapsed_us: u128,
    /// Fixed upper-bound buckets; the final bucket includes durations >= 16 s.
    pub latency_buckets: [u64; 8],
    pub warm_cache: u64,
    pub cold_cache: u64,
    pub unknown_cache: u64,
    pub cache_not_applicable: u64,
    #[serde(with = "wide_u128")]
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
            if let Some(bytes) = index.index_disk_bytes {
                self.index_work.disk_samples += 1;
                self.index_work.measured_disk_bytes += u128::from(bytes);
            }
            if let Some(bytes) = index.index_disk_growth_bytes {
                self.index_work.disk_growth_samples += 1;
                self.index_work.disk_growth_bytes += i128::from(bytes);
            }
            if let Some(duration) = index.scan_extract_us {
                self.index_work.scan_extract_samples += 1;
                self.index_work.scan_extract_us += u128::from(duration);
            }
            if let Some(duration) = index.publish_us {
                self.index_work.publish_samples += 1;
                self.index_work.publish_us += u128::from(duration);
            }
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BaselineSummary {
    pub compared_calls: u64,
    #[serde(with = "wide_u128")]
    pub baseline_utf8_bytes: u128,
    #[serde(with = "wide_u128")]
    pub returned_utf8_bytes: u128,
    pub token_estimate: TokenComparison,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

/// Replayed event IDs are ignored; durable deduplication across batches is the store's responsibility.
/// Bounded batches only: persisted reports merge bounded daily aggregates.
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

/// Explicit supplemental measurements never increment AIRK's observed request counts.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementProvenance {
    ClientReported,
    UserSupplied,
    LocalTokenizer,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TokenizerAlgorithm {
    Utf8ByteBpeV1,
    ClientReported,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TokenizerIdentity {
    pub algorithm: TokenizerAlgorithm,
    /// Exact local definition identity, not a hash of any measured text.
    pub definition_sha256: String,
    pub model_revision_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenMeasurement {
    pub tokenizer: TokenizerIdentity,
    /// Both sides MUST be counted with the same definition and framing.
    pub baseline_tokens: u64,
    pub returned_tokens: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextMeasurement {
    pub baseline_kind: BaselineKind,
    pub generation: u64,
    pub representation: ResponseRepresentation,
    pub baseline_utf8_bytes: u64,
    pub returned_utf8_bytes: u64,
    pub tokens: Option<TokenMeasurement>,
}

/// These are explicitly supplied client counters; AIRK cannot verify provider billing.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionMeasurement {
    pub model_revision_sha256: String,
    pub requests: u64,
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub elapsed_ms: Option<u64>,
    pub tool_round_trips: Option<u64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskResult {
    Passed,
    Failed,
    Unknown,
}

/// An explicitly paired evaluation, not success inferred from opening a result.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairedTaskMeasurement {
    pub same_task_and_model: bool,
    pub with_airk: SessionMeasurement,
    pub without_airk: SessionMeasurement,
    pub with_airk_result: TaskResult,
    pub without_airk_result: TaskResult,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplementalMeasurement {
    pub schema_version: u32,
    /// Opaque random 128-bit ID used for bounded replay protection, not a session label.
    pub event_id: String,
    pub unix_ms: u64,
    pub client: ClientFamily,
    pub content_kind: ContentKind,
    pub provenance: MeasurementProvenance,
    pub context: Option<ContextMeasurement>,
    pub session: Option<SessionMeasurement>,
    pub paired_task: Option<PairedTaskMeasurement>,
}

pub fn validate_hex(value: &str, bytes: usize) -> Result<()> {
    ensure!(
        value.len() == bytes * 2 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid fixed-size hexadecimal identity"
    );
    Ok(())
}

impl SessionMeasurement {
    fn validate(&self) -> Result<()> {
        validate_hex(&self.model_revision_sha256, 32)?;
        ensure!(
            self.requests > 0 && self.requests <= 1_000_000,
            "session requests must be 1..1000000"
        );
        for value in [
            self.input_tokens,
            self.cached_input_tokens,
            self.output_tokens,
            self.elapsed_ms,
            self.tool_round_trips,
        ]
        .into_iter()
        .flatten()
        {
            ensure!(
                value <= 1_000_000_000_000,
                "session measurement exceeds bound"
            );
        }
        if let Some(cached) = self.cached_input_tokens {
            ensure!(
                self.input_tokens.is_some_and(|input| cached <= input),
                "cached input tokens require total input tokens and cannot exceed them"
            );
        }
        Ok(())
    }
}

impl SupplementalMeasurement {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported supplemental usage schema"
        );
        validate_hex(&self.event_id, 16)?;
        ensure!(
            self.context.is_some() || self.session.is_some() || self.paired_task.is_some(),
            "supplemental measurement is empty"
        );
        if let Some(context) = &self.context {
            for value in [context.baseline_utf8_bytes, context.returned_utf8_bytes] {
                ensure!(
                    value <= 1_000_000_000_000,
                    "context measurement exceeds bound"
                );
            }
            ensure!(
                context.baseline_kind != BaselineKind::FullReturnedSources,
                "full-returned-sources is reserved for AIRK's own observations"
            );
            ensure!(
                (self.provenance == MeasurementProvenance::ClientReported)
                    == (context.baseline_kind == BaselineKind::ObservedAlternative),
                "observed alternatives require client-reported provenance; other inputs require user-supplied baseline"
            );
            if let Some(tokens) = &context.tokens {
                validate_hex(&tokens.tokenizer.definition_sha256, 32)?;
                validate_hex(&tokens.tokenizer.model_revision_sha256, 32)?;
                ensure!(
                    tokens.baseline_tokens <= 1_000_000_000_000
                        && tokens.returned_tokens <= 1_000_000_000_000,
                    "token measurement exceeds bound"
                );
                ensure!(
                    (self.provenance == MeasurementProvenance::LocalTokenizer)
                        == (tokens.tokenizer.algorithm == TokenizerAlgorithm::Utf8ByteBpeV1),
                    "local-tokenizer identity is reserved for local counting"
                );
            }
        }
        if let Some(session) = &self.session {
            session.validate()?;
        }
        if let Some(pair) = &self.paired_task {
            ensure!(
                pair.same_task_and_model,
                "paired task must explicitly attest the same task and model"
            );
            pair.with_airk.validate()?;
            pair.without_airk.validate()?;
            ensure!(
                pair.with_airk.model_revision_sha256 == pair.without_airk.model_revision_sha256,
                "paired task model revisions differ"
            );
        }
        let model = self.session.as_ref().map(|s| &s.model_revision_sha256);
        let paired_model = self
            .paired_task
            .as_ref()
            .map(|p| &p.with_airk.model_revision_sha256);
        if let (Some(model), Some(paired_model)) = (model, paired_model) {
            ensure!(
                model == paired_model,
                "session and paired task model revisions differ"
            );
        }
        if self.provenance == MeasurementProvenance::LocalTokenizer {
            ensure!(
                self.context.as_ref().is_some_and(|c| c.tokens.is_some())
                    && self.session.is_none()
                    && self.paired_task.is_none(),
                "local tokenizer measurements must contain only counted context"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SuppliedSessionSummary {
    pub sessions: u64,
    #[serde(with = "wide_u128")]
    pub requests: u128,
    pub input_measured_sessions: u64,
    #[serde(with = "wide_u128")]
    pub input_tokens: u128,
    pub cached_input_measured_sessions: u64,
    #[serde(with = "wide_u128")]
    pub cached_input_tokens: u128,
    /// Input tokens belonging to sessions whose cache split was actually supplied.
    #[serde(with = "wide_u128")]
    pub cache_classified_input_tokens: u128,
    pub output_measured_sessions: u64,
    #[serde(with = "wide_u128")]
    pub output_tokens: u128,
    pub elapsed_measured_sessions: u64,
    #[serde(with = "wide_u128")]
    pub elapsed_ms: u128,
    pub round_trip_measured_sessions: u64,
    #[serde(with = "wide_u128")]
    pub tool_round_trips: u128,
}

impl SuppliedSessionSummary {
    fn validate_accumulator(&self) -> Result<()> {
        ensure!(
            self.sessions <= 1_000_000_000,
            "invalid stored session count"
        );
        let maximum = u128::from(self.sessions) * 1_000_000_000_000;
        ensure!(
            self.requests <= u128::from(self.sessions) * 1_000_000
                && [
                    self.input_tokens,
                    self.cached_input_tokens,
                    self.output_tokens,
                    self.elapsed_ms,
                    self.tool_round_trips
                ]
                .into_iter()
                .all(|value| value <= maximum)
                && [
                    self.input_measured_sessions,
                    self.cached_input_measured_sessions,
                    self.output_measured_sessions,
                    self.elapsed_measured_sessions,
                    self.round_trip_measured_sessions
                ]
                .into_iter()
                .all(|count| count <= self.sessions)
                && self.cached_input_tokens <= self.cache_classified_input_tokens
                && self.cache_classified_input_tokens <= self.input_tokens,
            "invalid stored session aggregate"
        );
        Ok(())
    }

    fn observe(&mut self, session: &SessionMeasurement) {
        self.sessions += 1;
        self.requests += u128::from(session.requests);
        if let Some(value) = session.input_tokens {
            self.input_measured_sessions += 1;
            self.input_tokens += u128::from(value);
        }
        if let Some(value) = session.cached_input_tokens {
            self.cached_input_measured_sessions += 1;
            self.cached_input_tokens += u128::from(value);
            self.cache_classified_input_tokens += u128::from(session.input_tokens.unwrap_or(0));
        }
        if let Some(value) = session.output_tokens {
            self.output_measured_sessions += 1;
            self.output_tokens += u128::from(value);
        }
        if let Some(value) = session.elapsed_ms {
            self.elapsed_measured_sessions += 1;
            self.elapsed_ms += u128::from(value);
        }
        if let Some(value) = session.tool_round_trips {
            self.round_trip_measured_sessions += 1;
            self.tool_round_trips += u128::from(value);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplementStratum {
    pub client: ClientFamily,
    pub content_kind: ContentKind,
    pub provenance: MeasurementProvenance,
    pub baseline_kind: Option<BaselineKind>,
    pub representation: Option<ResponseRepresentation>,
    pub tokenizer: Option<TokenizerIdentity>,
    pub session_model_revision_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SupplementSummary {
    pub stratum: SupplementStratum,
    pub measurements: u64,
    pub context_pairs: u64,
    pub first_generation: Option<u64>,
    pub last_generation: Option<u64>,
    #[serde(with = "wide_u128")]
    pub baseline_utf8_bytes: u128,
    #[serde(with = "wide_u128")]
    pub returned_utf8_bytes: u128,
    pub byte_heuristic: TokenComparison,
    pub tokenizer_pairs: u64,
    #[serde(with = "wide_u128")]
    pub baseline_tokens: u128,
    #[serde(with = "wide_u128")]
    pub returned_tokens: u128,
    #[serde(with = "wide_i128")]
    pub token_reduction: i128,
    /// Sum of per-side absolute heuristic errors against supplied/local tokenizer counts.
    #[serde(with = "wide_u128")]
    pub heuristic_absolute_token_error: u128,
    pub reductions_outside_assumption_range: u64,
    pub sessions: SuppliedSessionSummary,
    pub paired_with_airk: SuppliedSessionSummary,
    pub paired_without_airk: SuppliedSessionSummary,
    /// [both passed, only AIRK passed, only alternative passed, both failed, unknown].
    pub paired_quality: [u64; 5],
}

impl SupplementSummary {
    pub fn new(event: &SupplementalMeasurement) -> Self {
        Self {
            stratum: SupplementStratum {
                client: event.client,
                content_kind: event.content_kind,
                provenance: event.provenance,
                baseline_kind: event.context.as_ref().map(|c| c.baseline_kind),
                representation: event.context.as_ref().map(|c| c.representation),
                tokenizer: event
                    .context
                    .as_ref()
                    .and_then(|c| c.tokens.as_ref())
                    .map(|t| t.tokenizer.clone()),
                session_model_revision_sha256: event
                    .session
                    .as_ref()
                    .map(|s| s.model_revision_sha256.clone())
                    .or_else(|| {
                        event
                            .paired_task
                            .as_ref()
                            .map(|p| p.with_airk.model_revision_sha256.clone())
                    }),
            },
            measurements: 0,
            context_pairs: 0,
            first_generation: None,
            last_generation: None,
            baseline_utf8_bytes: 0,
            returned_utf8_bytes: 0,
            byte_heuristic: TokenComparison::default(),
            tokenizer_pairs: 0,
            baseline_tokens: 0,
            returned_tokens: 0,
            token_reduction: 0,
            heuristic_absolute_token_error: 0,
            reductions_outside_assumption_range: 0,
            sessions: SuppliedSessionSummary::default(),
            paired_with_airk: SuppliedSessionSummary::default(),
            paired_without_airk: SuppliedSessionSummary::default(),
            paired_quality: [0; 5],
        }
    }

    pub fn observe(&mut self, event: &SupplementalMeasurement) -> Result<()> {
        event.validate()?;
        self.sessions.validate_accumulator()?;
        self.paired_with_airk.validate_accumulator()?;
        self.paired_without_airk.validate_accumulator()?;
        let maximum = u128::from(self.measurements) * 1_000_000_000_000;
        ensure!(
            self.context_pairs <= self.measurements
                && self.tokenizer_pairs <= self.context_pairs
                && self.reductions_outside_assumption_range <= self.tokenizer_pairs
                && self
                    .paired_quality
                    .iter()
                    .all(|count| *count <= self.measurements)
                && [
                    self.baseline_utf8_bytes,
                    self.returned_utf8_bytes,
                    self.baseline_tokens,
                    self.returned_tokens,
                    self.token_reduction.unsigned_abs()
                ]
                .into_iter()
                .all(|value| value <= maximum)
                && self.heuristic_absolute_token_error <= maximum * 2,
            "invalid stored supplemental aggregate"
        );
        ensure!(
            self.stratum == Self::new(event).stratum,
            "incompatible supplemental stratum"
        );
        // A lifetime bound also makes all aggregate additions provably fit u128/i128.
        ensure!(
            self.measurements < 1_000_000_000,
            "supplemental aggregate capacity reached"
        );
        self.measurements += 1;
        if let Some(context) = &event.context {
            self.context_pairs += 1;
            self.first_generation = Some(
                self.first_generation
                    .map_or(context.generation, |g| g.min(context.generation)),
            );
            self.last_generation = Some(
                self.last_generation
                    .map_or(context.generation, |g| g.max(context.generation)),
            );
            self.baseline_utf8_bytes += u128::from(context.baseline_utf8_bytes);
            self.returned_utf8_bytes += u128::from(context.returned_utf8_bytes);
            let estimate = ByteTokenHeuristic::default()
                .compare(context.baseline_utf8_bytes, context.returned_utf8_bytes)?;
            self.byte_heuristic.add(estimate)?;
            if let Some(tokens) = &context.tokens {
                self.tokenizer_pairs += 1;
                self.baseline_tokens += u128::from(tokens.baseline_tokens);
                self.returned_tokens += u128::from(tokens.returned_tokens);
                let exact = i128::from(tokens.baseline_tokens) - i128::from(tokens.returned_tokens);
                self.token_reduction += exact;
                self.heuristic_absolute_token_error += u128::from(
                    context
                        .baseline_utf8_bytes
                        .div_ceil(4)
                        .abs_diff(tokens.baseline_tokens),
                ) + u128::from(
                    context
                        .returned_utf8_bytes
                        .div_ceil(4)
                        .abs_diff(tokens.returned_tokens),
                );
                self.reductions_outside_assumption_range +=
                    u64::from(exact < estimate.assumption_low || exact > estimate.assumption_high);
            }
        }
        if let Some(session) = &event.session {
            self.sessions.observe(session);
        }
        if let Some(pair) = &event.paired_task {
            self.paired_with_airk.observe(&pair.with_airk);
            self.paired_without_airk.observe(&pair.without_airk);
            self.paired_quality[match (pair.with_airk_result, pair.without_airk_result) {
                (TaskResult::Passed, TaskResult::Passed) => 0,
                (TaskResult::Passed, TaskResult::Failed) => 1,
                (TaskResult::Failed, TaskResult::Passed) => 2,
                (TaskResult::Failed, TaskResult::Failed) => 3,
                _ => 4,
            }] += 1;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SupplementalBucket {
    pub day: u64,
    pub summaries: Vec<SupplementSummary>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CachePriceTreatment {
    Unknown,
    UncachedRate,
    SeparateRate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceScenario {
    pub model: String,
    pub currency: String,
    pub rate_date: String,
    /// Decimal currency units per million INPUT tokens, at most six fractional places.
    pub input_price_per_million: String,
    pub cached_input_treatment: CachePriceTreatment,
    pub cached_input_price_per_million: Option<String>,
    /// Context counterfactuals have no observed cache split; the assumption must be explicit.
    pub context_assumes_uncached: bool,
}

pub fn price_microunits(value: &str) -> Result<u64> {
    ensure!(value.len() <= 20, "price exceeds decimal bound");
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    ensure!(
        !whole.is_empty()
            && whole.bytes().all(|c| c.is_ascii_digit())
            && fraction.len() <= 6
            && fraction.bytes().all(|c| c.is_ascii_digit()),
        "price must be a nonnegative decimal with at most six fractional places"
    );
    let whole: u64 = whole.parse()?;
    ensure!(whole <= 1_000_000, "price exceeds scenario bound");
    Ok(whole * 1_000_000
        + if fraction.is_empty() {
            0
        } else {
            fraction.parse::<u64>()? * 10u64.pow(6 - fraction.len() as u32)
        })
}

impl PriceScenario {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.model.is_empty()
                && self.model.len() <= 96
                && self
                    .model
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._/-".contains(&c)),
            "model must be a bounded model identifier"
        );
        ensure!(
            self.currency.len() == 3 && self.currency.bytes().all(|c| c.is_ascii_uppercase()),
            "currency must be an uppercase three-letter code"
        );
        let date = self.rate_date.as_bytes();
        ensure!(
            date.len() == 10
                && date[4] == b'-'
                && date[7] == b'-'
                && date
                    .iter()
                    .enumerate()
                    .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit()),
            "rate date must be YYYY-MM-DD"
        );
        let year: u32 = self.rate_date[..4].parse()?;
        let month: usize = self.rate_date[5..7].parse()?;
        let day: u32 = self.rate_date[8..].parse()?;
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        ensure!(
            year > 0 && (1..=12).contains(&month) && day > 0 && day <= days[month - 1],
            "invalid rate date"
        );
        price_microunits(&self.input_price_per_million)?;
        ensure!(
            (self.cached_input_treatment == CachePriceTreatment::SeparateRate)
                == self.cached_input_price_per_million.is_some(),
            "separate cached treatment requires exactly one explicit cached price"
        );
        if let Some(price) = &self.cached_input_price_per_million {
            price_microunits(price)?;
        }
        Ok(())
    }

    /// Fixed decimal arithmetic avoids floating-point rounding or loss of a negative sign.
    pub fn input_equivalent(&self, tokens: i128, cached: bool) -> Result<Option<String>> {
        self.validate()?;
        let rate = if cached {
            match self.cached_input_treatment {
                CachePriceTreatment::Unknown => return Ok(None),
                CachePriceTreatment::UncachedRate => &self.input_price_per_million,
                CachePriceTreatment::SeparateRate => {
                    self.cached_input_price_per_million.as_ref().unwrap()
                }
            }
        } else {
            &self.input_price_per_million
        };
        let amount = tokens
            .checked_mul(i128::from(price_microunits(rate)?))
            .ok_or_else(|| anyhow::anyhow!("price scenario overflow"))?;
        let magnitude = amount.unsigned_abs();
        Ok(Some(format!(
            "{}{}.{:012}",
            if amount < 0 { "-" } else { "" },
            magnitude / 1_000_000_000_000u128,
            magnitude % 1_000_000_000_000u128
        )))
    }
}
