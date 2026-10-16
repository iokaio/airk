// SPDX-License-Identifier: Apache-2.0
//! Local request accounting and reports. No source or query content is persisted.
pub use airk_core::usage::*;
use airk_store::usage::{DAY_MS, RecordingHealth, UsageStore, unix_ms};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicU64, Ordering},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);

pub struct RequestSpan {
    started: Instant,
    operation: UsageOperation,
    client: ClientFamily,
    store: Option<UsageStore>,
    enabled: bool,
    event_id: [u8; 16],
    disk_before: Option<u64>,
    resources: Option<crate::resources::ResourceSpan>,
}

impl RequestSpan {
    pub fn request_id(&self) -> Option<String> {
        self.enabled.then(|| {
            self.event_id
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        })
    }
}

#[derive(Serialize)]
pub struct UsageView {
    pub policy: UsagePolicy,
    pub status: &'static str,
    pub report: UsageReport,
    pub history: Vec<UsageReport>,
    pub supplemental: Vec<SupplementalBucket>,
    pub price_scenario: Option<Value>,
    /// Measured exclusive intervals only; see sample counts in resources.
    pub process_cpu_us: Option<u64>,
    /// Hardware physical I/O is not inferred from process transfer counters.
    pub physical_io_bytes: Option<u64>,
    pub resources: ResourceSummary,
    pub health: RecordingHealth,
    pub process_dropped_observations: u64,
    pub notice: &'static str,
}

impl crate::Kernel {
    pub fn usage_scope(&self) -> UsageScope {
        UsageScope {
            collection: self.collection.clone(),
            root_binding: Sha256::digest(self.root_id().as_bytes()).into(),
        }
    }

    pub fn usage_policy(&self) -> Result<UsagePolicy> {
        if std::env::var_os("AIRK_USAGE_MODE").is_some_and(|s| s == "disabled") {
            return Ok(UsagePolicy {
                mode: RecordingMode::Disabled,
                ..UsagePolicy::default()
            });
        }
        airk_store::usage::policy(self.store.directory())
    }

    pub fn configure_usage(&self, mode: RecordingMode) -> Result<UsagePolicy> {
        self.initialize()?;
        let mut policy = airk_store::usage::policy(self.store.directory())?;
        policy.mode = mode;
        airk_store::usage::configure(self.store.directory(), &policy)?;
        Ok(policy)
    }

    pub fn clear_usage(&self) -> Result<Value> {
        self.load()?;
        airk_store::usage::clear(self.store.directory())?;
        Ok(
            serde_json::json!({"cleared":true,"collection":self.collection,"source_files_preserved":true}),
        )
    }

    pub fn usage_feedback(&self, request_id: &str, usefulness: Usefulness) -> Result<Value> {
        airk_store::usage::validate_binding(
            self.store.directory(),
            &self.root_id(),
            &self.collection,
        )?;
        airk_store::usage::feedback(
            self.store.directory(),
            &self.usage_scope(),
            request_id,
            usefulness,
        )?;
        Ok(serde_json::json!({"recorded":true,"usefulness":usefulness}))
    }

    pub fn usage_report(&self, days: u16) -> Result<UsageView> {
        ensure!((1..=30).contains(&days), "usage report days must be 1..30");
        if self.store.directory().exists() {
            airk_store::usage::validate_binding(
                self.store.directory(),
                &self.root_id(),
                &self.collection,
            )?;
        }
        let policy = self.usage_policy()?;
        let end = (unix_ms() / DAY_MS + 1) * DAY_MS;
        let start = end.saturating_sub(u64::from(days) * DAY_MS);
        let scope = self.usage_scope();
        let stored = UsageStore::read(self.store.directory(), &scope, start, end)?;
        let mut report = summarize(&scope, start, end, [], ByteTokenHeuristic::default())?;
        for bucket in &stored.buckets {
            report.merge(bucket)?;
        }
        report.duplicate_observations = stored.health.duplicate_observations;
        report.earliest_retained_unix_ms = stored.health.earliest_retained_unix_ms;
        let status = if policy.mode == RecordingMode::Disabled {
            "disabled"
        } else if report.calls == 0 && stored.supplemental.is_empty() {
            "empty"
        } else {
            "partial"
        };
        let mut resources = ResourceSummary::default();
        for operation in report.operations.values() {
            resources.merge(&operation.resources)?;
        }
        let process_cpu_us = resources
            .counters
            .get(&ResourceCounter::ProcessCpuUs)
            .and_then(|total| u64::try_from(total.value).ok());
        Ok(UsageView {
            policy,
            status,
            report,
            history: stored.buckets,
            supplemental: stored.supplemental,
            price_scenario: None,
            process_cpu_us,
            physical_io_bytes: None,
            resources,
            health: stored.health,
            process_dropped_observations: DROPPED.load(Ordering::Relaxed),
            notice: "Local content-free daily aggregates. UTC day windows; partial-day filtering is unavailable. Unknown recording loss is not zero. Estimates describe full-source counterfactual context, not provider tokens or billed savings. Wide totals are decimal strings.",
        })
    }

    pub fn usage_span(&self, operation: UsageOperation, client: ClientFamily) -> RequestSpan {
        let started = Instant::now();
        let client = if client == ClientFamily::NativeScheduler {
            client // Attribute actual queue jobs independently of the calling editor/CLI.
        } else {
            std::env::var("AIRK_CLIENT")
                .ok()
                .map_or(client, |label| ClientFamily::from_label(&label))
        };
        let policy = self.usage_policy();
        // A corrupt/unreadable policy is a dropped observation, not an implicit opt-out.
        // Only an explicit disabled policy (or environment override) bypasses accounting.
        let enabled = match &policy {
            Ok(policy) => policy.mode != RecordingMode::Disabled,
            Err(_) => true,
        };
        let store = if enabled
            && self.store.directory().exists()
            && airk_store::usage::validate_binding(
                self.store.directory(),
                &self.root_id(),
                &self.collection,
            )
            .is_ok()
        {
            policy.ok().and_then(|p| {
                UsageStore::open_best_effort(self.store.directory(), &self.usage_scope(), &p)
                    .ok()
                    .flatten()
            })
        } else {
            None
        };
        RequestSpan {
            started,
            operation,
            client,
            store,
            enabled,
            event_id: new_event_id(),
            resources: enabled.then(crate::resources::ResourceSpan::start),
            disk_before: if enabled
                && matches!(operation, UsageOperation::Index | UsageOperation::Reindex)
            {
                bounded_disk_bytes(self.store.directory())
            } else {
                None
            },
        }
    }

    /// Count exactly one outer adapter request, after transport serialization.
    /// Recording failure is observable locally but never changes the tool result.
    pub fn finish_usage(
        &self,
        span: RequestSpan,
        logical: Option<&Value>,
        bytes: usize,
        failed: bool,
        representation: ResponseRepresentation,
    ) {
        if !span.enabled {
            return;
        }
        let resources = span.resources.map(crate::resources::ResourceSpan::finish);
        let result = (|| -> Result<()> {
            let mut store = span.store;
            // First indexing creates managed state only after the operation succeeded.
            if store.is_none()
                && !failed
                && matches!(
                    span.operation,
                    UsageOperation::Index | UsageOperation::Reindex
                )
            {
                store = UsageStore::open_best_effort(
                    self.store.directory(),
                    &self.usage_scope(),
                    &self.usage_policy()?,
                )?;
            }
            let Some(store) = store else {
                anyhow::bail!("usage recorder unavailable");
            };
            let generation = logical
                .and_then(|v| {
                    v.get("generation")
                        .or_else(|| v.get("evidence")?.get("generation"))
                })
                .and_then(Value::as_u64);
            let result = logical.and_then(|v| v.get("evidence")).or(logical);
            let returned_items = result
                .and_then(|v| {
                    v.get("hits")
                        .or_else(|| v.get("facts"))
                        .or_else(|| v.get("relationships"))
                        .or_else(|| v.get("paths"))
                })
                .and_then(Value::as_array)
                .map_or(u32::from(logical.is_some()), |v| {
                    v.len().min(u32::MAX as usize) as u32
                });
            let outcome = if failed {
                UsageOutcome::Failed
            } else if returned_items == 0 {
                UsageOutcome::Empty
            } else {
                UsageOutcome::Success
            };
            let mut event = UsageObservation {
                schema_version: USAGE_SCHEMA_VERSION,
                event_id: span.event_id,
                scope: self.usage_scope(),
                unix_ms: unix_ms(),
                generation,
                operation: span.operation,
                client: span.client,
                content_kind: ContentKind::Unknown,
                outcome,
                elapsed_us: span.started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
                cache: if matches!(
                    span.operation,
                    UsageOperation::Index | UsageOperation::Reindex
                ) {
                    CacheObservation::NotApplicable
                } else {
                    CacheObservation::Unknown
                },
                returned_items,
                truncated: result
                    .and_then(|v| v.get("truncated"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                response_representation: representation,
                response_utf8_bytes: if failed
                    && bytes == 0
                    && representation == ResponseRepresentation::LogicalResultJson
                {
                    None // CLI stderr is outside the logical JSON representation.
                } else {
                    Some(bytes as u64)
                },
                baseline: None,
                index_work: None,
                resources,
            };
            if matches!(
                span.operation,
                UsageOperation::Index | UsageOperation::Reindex
            ) && let Some(value) = logical
            {
                let disk_after = bounded_disk_bytes(self.store.directory());
                event.index_work = Some(IndexWork {
                    indexed: value["indexed"].as_u64().unwrap_or(0),
                    reused: value["reused"].as_u64().unwrap_or(0),
                    removed: value["removed"].as_u64().unwrap_or(0),
                    skipped: value["skipped"]
                        .as_array()
                        .map_or(0, |a| a.len() as u64)
                        .saturating_add(value["skipped_truncated"].as_u64().unwrap_or(0)),
                    // A separate source-byte measurement is required; never infer avoided I/O from reuse.
                    source_bytes: value["source_bytes"].as_u64().unwrap_or(0),
                    index_disk_bytes: disk_after,
                    index_disk_growth_bytes: disk_after.zip(span.disk_before).and_then(
                        |(after, before)| {
                            i64::try_from(i128::from(after) - i128::from(before)).ok()
                        },
                    ),
                    scan_extract_us: value["scan_extract_us"].as_u64(),
                    publish_us: value["publish_us"].as_u64(),
                });
            }
            // Count only explicit cited sources. Bound optional comparison work to small
            // retained indexes; missing comparisons remain visible in the report.
            if span.operation.returns_source_evidence()
                && !failed
                && returned_items > 0
                && let Some(value) = logical
                && let Ok(Some((baseline, kind))) =
                    self.usage_baseline(value, generation, representation)
            {
                event.baseline = Some(baseline);
                event.content_kind = kind;
            }
            store.record(&event)
        })();
        if result.is_err() {
            let previous = DROPPED.fetch_add(1, Ordering::Relaxed);
            if previous == 0 {
                eprintln!(
                    "AIRK local usage recording dropped an observation; retrieval is unaffected (history completeness is unknown)."
                );
            }
        }
    }

    fn usage_baseline(
        &self,
        value: &Value,
        generation: Option<u64>,
        representation: ResponseRepresentation,
    ) -> Result<Option<(ContextBaseline, ContentKind)>> {
        let mut sources = BTreeSet::new();
        fn citations<'a>(value: &'a Value, sources: &mut BTreeSet<(&'a str, &'a str)>) {
            match value {
                Value::Object(object) => {
                    if let Some(citation) = object.get("citation")
                        && let (Some(path), Some(hash)) =
                            (citation["path"].as_str(), citation["content_hash"].as_str())
                    {
                        sources.insert((path, hash));
                    }
                    for value in object.values() {
                        citations(value, sources);
                    }
                }
                Value::Array(array) => {
                    for value in array {
                        citations(value, sources);
                    }
                }
                _ => {}
            }
        }
        citations(value, &mut sources);
        if sources.is_empty() || sources.len() > 100 {
            return Ok(None);
        }
        let Some(catalog) = self.store.catalog_bounded(8_388_608)? else {
            return Ok(None);
        };
        let snapshot = &catalog.snapshot;
        if Some(snapshot.generation) != generation
            || snapshot.root != self.root_id()
            || snapshot.collection != self.collection
        {
            return Ok(None);
        }
        let mut hits = Vec::new();
        let mut normalized_bytes = 0usize;
        let mut kinds = BTreeSet::new();
        let mut payload_remaining = 8_388_608usize;
        for (path, hash) in &sources {
            let Some(artifact) = snapshot
                .artifacts
                .get(*path)
                .filter(|a| a.content_hash == *hash)
            else {
                return Ok(None);
            };
            if artifact.extracted_bytes() > 1_048_576usize.saturating_sub(normalized_bytes)
                || artifact
                    .retained
                    .as_ref()
                    .is_some_and(|reference| reference.object_bytes > payload_remaining)
            {
                return Ok(None);
            }
            let artifact = catalog.load_artifact_bounded(path, payload_remaining as u64)?;
            let payload_bytes = serde_json::to_vec(&artifact)?.len();
            if payload_bytes > payload_remaining {
                return Ok(None);
            }
            payload_remaining -= payload_bytes;
            kinds.insert(content_kind(
                &artifact.language,
                &artifact.extraction_method,
            ));
            for chunk in &artifact.chunks {
                normalized_bytes = normalized_bytes.saturating_add(chunk.text.len());
                if normalized_bytes > 1_048_576 {
                    return Ok(None);
                }
                hits.push(airk_core::Hit {
                    citation: airk_core::Citation {
                        path: (*path).into(),
                        content_hash: (*hash).into(),
                        start_line: chunk.start_line,
                        end_line: chunk.end_line,
                        location: chunk.location.clone(),
                    },
                    text: chunk.text.clone(),
                    truncated: false,
                });
            }
        }
        let baseline = airk_core::SearchResponse {
            publication: None,
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            hits,
            truncated: false,
        };
        let text = serde_json::to_string(&baseline)?;
        let bytes = match representation {
            ResponseRepresentation::LogicalResultJson => text.len(),
            ResponseRepresentation::McpToolResultJson => serde_json::to_vec(
                &serde_json::json!({"content":[{"type":"text","text":text}],"isError":false}),
            )?
            .len(),
        };
        Ok(Some((
            ContextBaseline {
                kind: BaselineKind::FullReturnedSources,
                generation: snapshot.generation,
                representation,
                utf8_bytes: bytes as u64,
                unique_sources: sources.len() as u32,
            },
            if kinds.len() == 1 {
                *kinds.first().unwrap_or(&ContentKind::Unknown)
            } else {
                ContentKind::Mixed
            },
        )))
    }
}

fn new_event_id() -> [u8; 16] {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hash = Sha256::new();
    hash.update(nanos.to_le_bytes());
    hash.update(std::process::id().to_le_bytes());
    hash.update(sequence.to_le_bytes());
    hash.finalize()[..16].try_into().expect("fixed digest size")
}

fn content_kind(language: &str, extraction: &str) -> ContentKind {
    if extraction.contains("pdf") {
        ContentKind::Pdf
    } else if extraction.contains("docx") {
        ContentKind::Docx
    } else if extraction.contains("xlsx") {
        ContentKind::Xlsx
    } else if extraction.contains("pptx") {
        ContentKind::Pptx
    } else {
        match language {
            "markdown" => ContentKind::Markdown,
            "svg" => ContentKind::Svg,
            "json" | "yaml" | "toml" | "html" | "css" | "xml" | "csv" | "tsv" | "ini"
            | "dockerfile" => ContentKind::StructuredText,
            "text" | "plain" | "unknown" => ContentKind::PlainText,
            _ => ContentKind::Code,
        }
    }
}

pub fn render_report(view: &UsageView, format: &str) -> Result<String> {
    match format {
        "json" => Ok(serde_json::to_string(view)?),
        "csv" => {
            let mut text = String::from(
                "operation,calls,successful,empty,failed,total_elapsed_us,returned_utf8_bytes,details_json\n",
            );
            for (operation, summary) in &view.report.operations {
                text.push_str(&format!(
                    "{},{},{},{},{},{},{},\n",
                    serde_json::to_string(operation)?.trim_matches('"'),
                    summary.calls,
                    summary.successful,
                    summary.empty,
                    summary.failed,
                    summary.total_elapsed_us,
                    summary.returned_utf8_bytes
                ));
            }
            for bucket in &view.supplemental {
                text.push_str(&format!(
                    "supplemental,,,,,,,\"{}\"\n",
                    serde_json::to_string(bucket)?.replace('"', "\"\"")
                ));
            }
            text.push_str(&format!(
                "resources,,,,,,,\"{}\"\n",
                serde_json::to_string(&view.resources)?.replace('"', "\"\"")
            ));
            if let Some(scenario) = &view.price_scenario {
                text.push_str(&format!(
                    "price_scenario,,,,,,,\"{}\"\n",
                    serde_json::to_string(scenario)?.replace('"', "\"\"")
                ));
            }
            Ok(text)
        }
        "markdown" => {
            let mut text = format!(
                "# AIRK local usage: {}\n\nStatus: {}. Calls: {}.\n\n{}\n\n| Operation | Calls | Errors | Returned bytes |\n|---|---:|---:|---:|\n",
                view.report.scope.collection, view.status, view.report.calls, view.notice
            );
            for (operation, summary) in &view.report.operations {
                text.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    serde_json::to_string(operation)?.trim_matches('"'),
                    summary.calls,
                    summary.failed,
                    summary.returned_utf8_bytes
                ));
            }
            for (kind, comparison) in &view.report.comparisons {
                text.push_str(&format!("\n{} counterfactual: {} nominal tokens of context reduction (signed; {} compared calls).\n", serde_json::to_string(kind)?, comparison.token_estimate.nominal_reduction, comparison.compared_calls));
            }
            text.push_str(&format!("\nProcess resources: {} sampled intervals, {} overlapping intervals excluded from counter attribution; peak process RSS: {:?} bytes. Process deltas include background threads. OS storage bytes and transfer bytes are distinct; hardware physical I/O remains unknown.\n", view.resources.intervals, view.resources.overlapped_intervals, view.resources.peak_rss_bytes));
            for (counter, total) in &view.resources.counters {
                text.push_str(&format!(
                    "\n{:?}: {} across {} measured intervals.\n",
                    counter, total.value, total.samples
                ));
            }
            for bucket in &view.supplemental {
                for summary in &bucket.summaries {
                    text.push_str(&format!("\nSupplemental UTC day {}: {:?}, {:?}, {} context pairs; {} signed tokenizer tokens of reduction ({} tokenizer pairs); {} supplied sessions and {} paired tasks. Not included in AIRK request totals.\n", bucket.day, summary.stratum.provenance, summary.stratum.content_kind, summary.context_pairs, summary.token_reduction, summary.tokenizer_pairs, summary.sessions.sessions, summary.paired_with_airk.sessions));
                }
            }
            if let Some(scenario) = &view.price_scenario {
                text.push_str(&format!("\nExplicit input-price scenario (hypothetical equivalents, not billed savings):\n\n```json\n{}\n```\n", serde_json::to_string_pretty(scenario)?));
            }
            Ok(text)
        }
        _ => anyhow::bail!("usage format must be json, csv or markdown"),
    }
}

/// Reads are bounded even if a file grows between metadata lookup and reading.
pub fn read_usage_input(path: &std::path::Path, maximum: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    ensure!(
        std::fs::metadata(path)?.is_file(),
        "usage input must be a regular file"
    );
    let file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "usage input must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= maximum, "usage input exceeds bounded size");
    Ok(bytes)
}

/// Logical managed index sizes, not physical disk allocation or bytes actually read.
/// Partial, linked, inaccessible or slow inventories remain unknown.
fn bounded_disk_bytes(directory: &std::path::Path) -> Option<u64> {
    if !directory.exists() {
        return Some(0);
    }
    let started = Instant::now();
    let mut pending = vec![directory.to_path_buf()];
    let mut total = 0u64;
    let mut count = 0usize;
    while let Some(directory) = pending.pop() {
        if std::fs::symlink_metadata(&directory)
            .ok()?
            .file_type()
            .is_symlink()
        {
            return None;
        }
        for entry in std::fs::read_dir(directory).ok()? {
            count += 1;
            if count > 10_000 || started.elapsed().as_millis() > 50 {
                return None;
            }
            let entry = entry.ok()?;
            let name = entry.file_name();
            // Usage has its own budget; exclude its files and transient locks from index cost.
            if name.to_string_lossy().starts_with("usage")
                || name.to_string_lossy().ends_with(".lock")
            {
                continue;
            }
            let kind = entry.file_type().ok()?;
            if kind.is_symlink() {
                return None;
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                total = total.checked_add(entry.metadata().ok()?.len())?;
            }
        }
    }
    Some(total)
}

pub struct CalibrationRequest<'a> {
    pub tokenizer: &'a std::path::Path,
    pub baseline: &'a std::path::Path,
    pub returned: &'a std::path::Path,
    pub generation: u64,
    pub content_kind: ContentKind,
    pub record: bool,
}

impl crate::Kernel {
    pub fn usage_import(&self, event: &SupplementalMeasurement) -> Result<Value> {
        ensure!(
            event.provenance != MeasurementProvenance::LocalTokenizer,
            "local-tokenizer provenance requires usage calibrate"
        );
        self.save_supplement(event)
    }

    fn save_supplement(&self, event: &SupplementalMeasurement) -> Result<Value> {
        event.validate()?;
        let policy = self.usage_policy()?;
        ensure!(
            policy.mode != RecordingMode::Disabled,
            "usage recording is disabled"
        );
        airk_store::usage::validate_binding(
            self.store.directory(),
            &self.root_id(),
            &self.collection,
        )?;
        let store = UsageStore::open(self.store.directory(), &self.usage_scope(), &policy)?
            .ok_or_else(|| anyhow::anyhow!("usage recording is disabled"))?;
        let recorded = store.record_supplement(event)?;
        Ok(
            serde_json::json!({"recorded":recorded,"duplicate":!recorded,"scope":self.usage_scope(),"provenance":event.provenance,"airk_request_count_changed":false}),
        )
    }

    pub fn usage_calibrate(&self, request: CalibrationRequest<'_>) -> Result<Value> {
        // Opt-out prohibits persistence, not an explicitly requested transient calculation.
        if request.record {
            ensure!(
                self.usage_policy()?.mode != RecordingMode::Disabled,
                "usage recording is disabled"
            );
        }
        let tokenizer = crate::tokenization::LocalTokenizer::open(request.tokenizer)?;
        let baseline = String::from_utf8(read_usage_input(request.baseline, 262_144)?)?;
        let returned = String::from_utf8(read_usage_input(request.returned, 262_144)?)?;
        // The explicit inputs are logical JSON results; framing cannot be silently mixed.
        let _: Value = serde_json::from_str(&baseline)?;
        let _: Value = serde_json::from_str(&returned)?;
        let measurement = SupplementalMeasurement {
            schema_version: 1,
            event_id: new_event_id()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            unix_ms: unix_ms(),
            client: ClientFamily::Cli,
            content_kind: request.content_kind,
            provenance: MeasurementProvenance::LocalTokenizer,
            context: Some(ContextMeasurement {
                baseline_kind: BaselineKind::UserSupplied,
                generation: request.generation,
                representation: ResponseRepresentation::LogicalResultJson,
                baseline_utf8_bytes: baseline.len() as u64,
                returned_utf8_bytes: returned.len() as u64,
                tokens: Some(TokenMeasurement {
                    tokenizer: tokenizer.identity().clone(),
                    baseline_tokens: tokenizer.count(&baseline)?,
                    returned_tokens: tokenizer.count(&returned)?,
                }),
            }),
            session: None,
            paired_task: None,
        };
        let mut summary = SupplementSummary::new(&measurement);
        summary.observe(&measurement)?;
        let recording = if request.record {
            Some(self.save_supplement(&measurement)?)
        } else {
            None
        };
        Ok(
            serde_json::json!({"summary":summary,"recording":recording,"tokenization":tokenizer.description(),"notice":"Exact counts for the digest-bound local tokenizer and explicit framing, with truncation and padding disabled. User-supplied logical JSON samples; same-generation comparability and provider framing compatibility require independent verification. No source text or sample hash is retained. This is not provider token usage."}),
        )
    }

    pub fn usage_report_with_price(
        &self,
        days: u16,
        scenario: &PriceScenario,
    ) -> Result<UsageView> {
        scenario.validate()?;
        let mut view = self.usage_report(days)?;
        let mut contexts = Vec::new();
        if scenario.context_assumes_uncached {
            for (kind, comparison) in &view.report.comparisons {
                contexts.push(serde_json::json!({"baseline_kind":kind,"provenance":"airk_observed_bytes","estimator":view.report.estimator_version,"compared_pairs":comparison.compared_calls,"nominal_input_cost_equivalent":scenario.input_equivalent(comparison.token_estimate.nominal_reduction,false)?,"assumption_low":scenario.input_equivalent(comparison.token_estimate.assumption_low,false)?,"assumption_high":scenario.input_equivalent(comparison.token_estimate.assumption_high,false)?}));
            }
        }
        let mut sessions = Vec::new();
        for bucket in &view.supplemental {
            for summary in &bucket.summaries {
                if scenario.context_assumes_uncached && summary.context_pairs > 0 {
                    let exact = summary.tokenizer_pairs == summary.context_pairs;
                    let tokens = if exact {
                        summary.token_reduction
                    } else {
                        summary.byte_heuristic.nominal_reduction
                    };
                    contexts.push(serde_json::json!({"day":bucket.day,"stratum":summary.stratum,"compared_pairs":summary.context_pairs,"estimator":if exact {"supplied_tokenizer_counts"} else {"utf8-byte-assumptions/1"},"nominal_input_cost_equivalent":scenario.input_equivalent(tokens,false)?}));
                }
                for (kind, session) in [
                    ("client_session", &summary.sessions),
                    ("paired_with_airk", &summary.paired_with_airk),
                    ("paired_without_airk", &summary.paired_without_airk),
                ] {
                    if session.sessions == 0 {
                        continue;
                    }
                    let classified = session.cache_classified_input_tokens;
                    let uncached = classified.saturating_sub(session.cached_input_tokens);
                    // Unknown cache splits are deliberately excluded instead of assuming zero cached tokens.
                    sessions.push(serde_json::json!({"day":bucket.day,"stratum":summary.stratum,"kind":kind,"sessions":session.sessions,"cache_classified_sessions":session.cached_input_measured_sessions,"uncached_input_equivalent":scenario.input_equivalent(i128::try_from(uncached)?,false)?,"cached_input_equivalent":scenario.input_equivalent(i128::try_from(session.cached_input_tokens)?,true)?,"unpriced_input_tokens_unknown_cache_split":session.input_tokens.saturating_sub(classified).to_string(),"sessions_without_input_counts":session.sessions-session.input_measured_sessions}));
                }
            }
        }
        view.price_scenario = Some(
            serde_json::json!({"provenance":"user_supplied_scenario","scenario":scenario,"context_equivalents":contexts,"session_input_equivalents":sessions,"estimated_bill_savings":null,"notice":"Hypothetical input-cost equivalents at explicitly supplied rates. No model compatibility, provider bill, output-token savings or causal savings are inferred. Context equivalents assume every context token is uncached only when explicitly enabled."}),
        );
        Ok(view)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ByteBpeDefinition {
    schema: u32,
    model_revision_sha256: String,
    /// Ranked merges of hexadecimal byte strings; the 256 byte symbols are implicit.
    merges: Vec<[String; 2]>,
}

pub(crate) struct LocalByteBpe {
    pub(crate) identity: TokenizerIdentity,
    ranks: std::collections::BTreeMap<(Vec<u8>, Vec<u8>), usize>,
}

impl LocalByteBpe {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        let definition: ByteBpeDefinition = serde_json::from_slice(bytes)?;
        ensure!(
            definition.schema == 1 && definition.merges.len() <= 8192,
            "unsupported or oversized byte-BPE definition"
        );
        validate_hex(&definition.model_revision_sha256, 32)?;
        let mut known: BTreeSet<Vec<u8>> = (0..=255).map(|b| vec![b]).collect();
        let mut ranks = std::collections::BTreeMap::new();
        for (rank, pair) in definition.merges.iter().enumerate() {
            let decode = |hex: &str| -> Result<Vec<u8>> {
                ensure!(
                    !hex.is_empty() && hex.len() <= 512 && hex.len().is_multiple_of(2),
                    "invalid BPE symbol length"
                );
                validate_hex(hex, hex.len() / 2)?;
                (0..hex.len())
                    .step_by(2)
                    .map(|i| Ok(u8::from_str_radix(&hex[i..i + 2], 16)?))
                    .collect()
            };
            let left = decode(&pair[0])?;
            let right = decode(&pair[1])?;
            ensure!(
                left.len() + right.len() <= 256 && known.contains(&left) && known.contains(&right),
                "BPE merge must combine prior symbols into at most 256 bytes"
            );
            let merged = [left.as_slice(), right.as_slice()].concat();
            ensure!(
                ranks.insert((left, right), rank).is_none(),
                "duplicate BPE merge"
            );
            known.insert(merged);
        }
        Ok(Self {
            identity: TokenizerIdentity {
                algorithm: TokenizerAlgorithm::Utf8ByteBpeV1,
                definition_sha256: Sha256::digest(bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
                model_revision_sha256: definition.model_revision_sha256.to_ascii_lowercase(),
            },
            ranks,
        })
    }

    pub(crate) fn count(&self, text: &str) -> Result<u64> {
        use std::{cmp::Reverse, collections::BinaryHeap};
        ensure!(text.len() <= 262_144, "tokenizer text exceeds bound");
        let mut symbols: Vec<Vec<u8>> = text.bytes().map(|b| vec![b]).collect();
        let length = symbols.len();
        let mut next: Vec<usize> = (1..=length).collect();
        let mut previous: Vec<usize> = (0..length)
            .map(|i| i.checked_sub(1).unwrap_or(length))
            .collect();
        let mut versions = vec![0u32; length];
        let mut queue = BinaryHeap::new();
        let rank =
            |left: &[u8], right: &[u8]| self.ranks.get(&(left.to_vec(), right.to_vec())).copied();
        for left in 0..length.saturating_sub(1) {
            if let Some(rank) = rank(&symbols[left], &symbols[left + 1]) {
                queue.push(Reverse((rank, left, 0u32, left + 1, 0u32)));
            }
        }
        let mut count = length as u64;
        while let Some(Reverse((_, left, left_version, right, right_version))) = queue.pop() {
            if versions[left] != left_version
                || versions[right] != right_version
                || next[left] != right
                || symbols[left].is_empty()
                || symbols[right].is_empty()
            {
                continue;
            }
            let right_bytes = std::mem::take(&mut symbols[right]);
            symbols[left].extend(right_bytes);
            versions[left] += 1;
            versions[right] += 1;
            next[left] = next[right];
            if next[left] < length {
                previous[next[left]] = left;
            }
            count -= 1;
            for candidate in [previous[left], left] {
                if candidate < length && next[candidate] < length {
                    let right = next[candidate];
                    if let Some(rank) = rank(&symbols[candidate], &symbols[right]) {
                        queue.push(Reverse((
                            rank,
                            candidate,
                            versions[candidate],
                            right,
                            versions[right],
                        )));
                    }
                }
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strict_span(
        kernel: &crate::Kernel,
        operation: UsageOperation,
        client: ClientFamily,
    ) -> Result<RequestSpan> {
        let mut span = kernel.usage_span(operation, client);
        // Exact payload assertions use the strict store path. Integration tests
        // separately exercise real best-effort admission and visible recording loss.
        span.store = UsageStore::open(
            kernel.store.directory(),
            &kernel.usage_scope(),
            &kernel.usage_policy()?,
        )?;
        assert!(span.enabled && span.store.is_some());
        Ok(span)
    }

    #[test]
    fn index_observations_record_exact_work_and_failures() -> Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::write(
            root.path().join("fixture.txt"),
            "original accounting fixture",
        )?;
        let kernel = crate::Kernel::open(root.path(), None)?;
        kernel.initialize()?;
        let mut source_bytes = 0;
        for attempt in 0..3 {
            let writer = (attempt == 2).then(|| kernel.store.lock()).transpose()?;
            let span = strict_span(
                &kernel,
                UsageOperation::Index,
                ClientFamily::NativeScheduler,
            )?;
            let result = kernel.index();
            if attempt == 2 {
                assert!(
                    result
                        .as_ref()
                        .unwrap_err()
                        .to_string()
                        .contains("index writer locked")
                );
            } else {
                let report = result.as_ref().unwrap();
                assert_eq!(
                    (report.indexed, report.reused),
                    if attempt == 0 { (1, 0) } else { (0, 1) }
                );
                source_bytes += report.source_bytes as u128;
            }
            let logical = result.as_ref().ok().map(serde_json::to_value).transpose()?;
            let bytes = logical
                .as_ref()
                .map(serde_json::to_vec)
                .transpose()?
                .map_or(0, |value| value.len());
            kernel.finish_usage(
                span,
                logical.as_ref(),
                bytes,
                result.is_err(),
                ResponseRepresentation::LogicalResultJson,
            );
            drop(writer);
            assert_eq!(kernel.usage_report(30)?.report.calls, attempt + 1);
        }
        let report = kernel.usage_report(30)?;
        assert_eq!(report.report.clients[&ClientFamily::NativeScheduler], 3);
        let work = &report.report.operations[&UsageOperation::Index];
        assert_eq!((work.successful, work.failed, work.empty), (2, 1, 0));
        assert_eq!((work.index_work.indexed, work.index_work.reused), (1, 1));
        assert_eq!(work.index_work.processed_source_bytes, source_bytes);
        assert!(report.report.comparisons.is_empty());
        Ok(())
    }

    #[test]
    fn adapter_observation_preserves_revision_baseline_feedback_and_privacy() -> anyhow::Result<()>
    {
        let root = tempfile::tempdir()?;
        std::fs::write(
            root.path().join("memo.md"),
            "# Private fixture\nconfidential_zircon_word\nA larger piece of original fixture evidence.\n",
        )?;
        let kernel = crate::Kernel::open(root.path(), None)?;
        kernel.index()?;
        let span = strict_span(&kernel, UsageOperation::Search, ClientFamily::Codex)?;
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
}
