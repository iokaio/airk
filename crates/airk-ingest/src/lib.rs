// SPDX-License-Identifier: Apache-2.0
//! Native code/document extraction and UTF-8 fallback, with explicit exclusions.
use airk_core::{Artifact, Chunk, DocumentLocation, EXTRACTOR_VERSION, Snapshot};
use anyhow::{Context, Result, ensure};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::fs::File;
use std::{collections::BTreeMap, path::Path};

mod actions;
pub mod adapters;
mod build_cmake;
mod build_facts;
mod build_make;
pub mod chunking;
mod compose;
mod composite;
pub mod continuity;
mod detail;
mod extraction_jobs;
mod extraction_pool;
pub use extraction_pool::ExtractionExecution;
pub mod header_context;
mod host_components;
mod host_html;
mod host_prose;
mod instruction_documents;
mod languages_a;
mod languages_b_data;
mod languages_b_functional;
mod languages_b_r_perl_groovy;
mod msbuild;
mod notebook;
pub mod observations;
mod project_build;
mod project_schemas;
mod razor;
mod repository_binding;
mod repository_syntax;
mod scss;
mod semantic_projection;
mod source_read;
mod specialist_business;
mod specialist_gpu;
mod specialist_hardware;
mod specialist_vendor;
mod structured_view;
mod structured_yaml;
mod syntax_repairs;

pub const MAX_FILE_BYTES: usize = airk_core::extraction::MAX_SOURCE_BYTES;
/// Structured parsers never inherit the larger raw-file admission ceiling.
pub(crate) const MAX_STRUCTURED_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CORPUS_BYTES: usize = 2 * 1024 * 1024 * 1024;
pub const MAX_EXTRACTED_CORPUS_BYTES: usize = 4 * 1024 * 1024 * 1024;
pub const MAX_FILES: usize = 100_000;
pub const MAX_DISCOVERY_ENTRIES: usize = 1_000_000;
pub const MAX_SCAN_READ_BYTES: usize = 8 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanLimits {
    pub file_bytes: usize,
    pub corpus_bytes: usize,
    pub extracted_bytes: usize,
    pub files: usize,
    pub discovery_entries: usize,
    pub read_bytes: usize,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            file_bytes: MAX_FILE_BYTES,
            corpus_bytes: MAX_CORPUS_BYTES,
            extracted_bytes: MAX_EXTRACTED_CORPUS_BYTES,
            files: MAX_FILES,
            discovery_entries: MAX_DISCOVERY_ENTRIES,
            read_bytes: MAX_SCAN_READ_BYTES,
        }
    }
}

impl ScanLimits {
    pub fn maximum() -> Self {
        Self {
            file_bytes: MAX_FILE_BYTES,
            corpus_bytes: 64 * 1024 * 1024 * 1024,
            extracted_bytes: 128 * 1024 * 1024 * 1024,
            files: 1_000_000,
            discovery_entries: 10_000_000,
            read_bytes: 128 * 1024 * 1024 * 1024,
        }
    }

    pub fn validate(self) -> Result<()> {
        let maximum = Self::maximum();
        ensure!(
            (1..=maximum.file_bytes).contains(&self.file_bytes),
            "file limit must be 1..256 MiB"
        );
        ensure!(
            (1..=maximum.corpus_bytes).contains(&self.corpus_bytes),
            "corpus limit must be 1 byte..64 GiB"
        );
        ensure!(
            (1..=maximum.extracted_bytes).contains(&self.extracted_bytes),
            "extracted limit must be 1 byte..128 GiB"
        );
        ensure!(
            (1..=maximum.files).contains(&self.files),
            "file count limit must be 1..1000000"
        );
        ensure!(
            (1..=maximum.discovery_entries).contains(&self.discovery_entries),
            "discovery limit must be 1..10000000"
        );
        ensure!(
            (1..=maximum.read_bytes).contains(&self.read_bytes),
            "read limit must be 1 byte..128 GiB"
        );
        Ok(())
    }
}
mod documents;
pub mod inventory;
pub mod ocr;
pub mod pdf_worker;
mod prose;
mod provider_process;
pub mod providers;
pub mod semantic;
mod spreadsheets;
pub mod syntax;
mod tables;

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct CurrentIndexWork {
    pub changed_artifacts: usize,
    pub lexical_documents: u64,
    pub graph_records: u64,
    pub graph_read_bytes: u64,
    /// Verified base-record fetches; repeated visits may be served by an
    /// update-local authenticated proof cache.
    pub graph_base_record_reads: u64,
    pub graph_proof_cache_hits: u64,
    pub graph_proof_cache_evictions: u64,
    pub linked_lexical_bytes: u64,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct IndexReport {
    /// Elapsed from index admission until committed lexical/fact evidence is readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_ready_us: Option<u64>,
    /// Elapsed until selected semantic work is fully committed in this invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fully_ready_us: Option<u64>,
    /// Changed-path work from the current runtime; logical counts, not physical I/O.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<CurrentIndexWork>,
    /// Cleanup failure after a successful publication does not undo that publication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintenance_error: Option<String>,
    /// Selection warnings do not imply that an eligible file was skipped.
    #[serde(default)]
    pub selection_diagnostics: Vec<SkippedFile>,
    #[serde(default)]
    pub selection_diagnostics_truncated: usize,
    #[serde(default)]
    pub embedding_cache_hits: u64,
    #[serde(default)]
    pub embedding_cache_finalize_us: u64,
    #[serde(default)]
    pub embedding_cache_file_opens: u64,
    #[serde(default)]
    pub embedding_cache_read_bytes: u64,
    #[serde(default)]
    pub embedding_cache_written_bytes: u64,
    #[serde(default)]
    pub embedding_cache_record_verification_bytes: u64,
    #[serde(default)]
    pub embedding_cache_index_verification_bytes: u64,
    #[serde(default)]
    pub embedding_cache_full_pack_verification_bytes: u64,
    #[serde(default)]
    pub embedding_cache_syncs: u64,
    #[serde(default)]
    pub embedding_cache_index_hits: u64,
    #[serde(default)]
    pub embedding_cache_index_peak_bytes: u64,
    #[serde(default)]
    pub embedding_cache_packs_written: u64,
    #[serde(default)]
    pub embedding_cache_packs_reused: u64,
    #[serde(default)]
    pub embedding_cache_compacted_records: u64,

    #[serde(default)]
    pub embedding_cache_misses: u64,
    #[serde(default)]
    pub embedding_inferences: u64,
    #[serde(default)]
    pub embedding_no_vector: u64,
    /// Inputs presented to the embedding adapter, including cache hits and blanks.
    #[serde(default)]
    pub embedding_inputs: u64,
    #[serde(default)]
    pub embedding_blank_inputs: u64,
    #[serde(default)]
    pub embedding_duplicate_inputs: u64,
    /// Actual neural plan executions; static word-vector lookups do not count.
    #[serde(default)]
    pub embedding_actual_forwards: u64,
    #[serde(default)]
    pub embedding_tokenized_inputs: u64,
    /// Tokens include processor specials and apply only to executed cache misses.
    #[serde(default)]
    pub embedding_input_tokens: u64,
    #[serde(default)]
    pub embedding_embedded_tokens: u64,
    #[serde(default)]
    pub embedding_padding_tokens: u64,
    #[serde(default)]
    pub embedding_truncated_inputs: u64,
    #[serde(default)]
    pub embedding_plan_compilations: u64,
    #[serde(default)]
    pub embedding_plan_compile_us: u64,
    /// Peak declared allocation charged to the bounded plan cache, not process RSS.
    #[serde(default)]
    pub embedding_plan_cache_peak_bytes: u64,
    #[serde(default)]
    pub vector_eligible_artifacts: u64,
    #[serde(default)]
    pub vector_excluded_artifacts: u64,
    #[serde(default)]
    pub vector_reused_inputs: u64,
    #[serde(default)]
    pub vector_graph_nodes: u64,
    #[serde(default)]
    pub vector_registry_rows: u64,
    #[serde(default)]
    pub vector_registry_bytes: u64,
    #[serde(default)]
    pub vector_units: u64,
    #[serde(default)]
    pub vector_body_bytes: u64,
    /// Semantic planning only; excludes embedding/cache/graph visitor time.
    #[serde(default)]
    pub vector_planning_us: u64,
    #[serde(default)]
    pub embedding_model_load_us: u64,
    #[serde(default)]
    pub embedding_cache_lookup_us: u64,
    #[serde(default)]
    pub embedding_cache_store_us: u64,
    #[serde(default)]
    pub embedding_inference_us: u64,
    #[serde(default)]
    pub vector_graph_insert_us: u64,
    #[serde(default)]
    pub embedding_workers: usize,
    #[serde(default)]
    pub embedding_batches: u64,
    #[serde(default)]
    pub extraction_workers_requested: usize,
    #[serde(default)]
    pub extraction_worker_limit: usize,
    #[serde(default)]
    pub extraction_workers_spawned: usize,
    #[serde(default)]
    pub extraction_parallel_jobs: u64,
    #[serde(default)]
    pub extraction_serial_jobs: u64,
    /// Summed extraction job wall time, including overlapping jobs; not CPU time.
    #[serde(default)]
    pub extraction_work_us: u64,
    #[serde(default)]
    pub extraction_wait_us: u64,
    /// Source bytes retained by admitted unfinished/completed jobs.
    #[serde(default)]
    pub extraction_source_peak_bytes: u64,
    /// One captured source held outside job admission while the batch drains.
    #[serde(default)]
    pub extraction_coordinator_source_peak_bytes: u64,
    /// Declared completed serialized payload capacity; not an object/RSS bound.
    #[serde(default)]
    pub extraction_result_bound_bytes: u64,
    #[serde(default)]
    pub scan_extract_us: u64,
    #[serde(default)]
    pub publish_us: u64,
    pub generation: u64,
    pub collection: String,
    pub indexed: usize,
    pub reused: usize,
    pub removed: usize,
    pub source_bytes: usize,
    pub skipped_truncated: usize,
    pub skipped: Vec<SkippedFile>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkippedFile {
    pub path: String,
    pub reason: String,
}

pub fn scan(root: &Path, data_dir: &Path, previous: &Snapshot) -> Result<(Snapshot, IndexReport)> {
    scan_with_limits(
        root,
        data_dir,
        previous,
        MAX_DISCOVERY_ENTRIES,
        MAX_SCAN_READ_BYTES,
    )
}

fn scan_with_limits(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    max_entries: usize,
    max_read_bytes: usize,
) -> Result<(Snapshot, IndexReport)> {
    scan_streaming_with_limits(
        root,
        data_dir,
        previous,
        &previous.roots,
        max_entries,
        max_read_bytes,
        |artifact, _| Ok(artifact.clone()),
    )
}

/// Emit one bounded artifact and its original source bytes at a time. The callback
/// may retain payloads and return compact inventory metadata instead of source text.
pub fn scan_streaming(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    emit: impl FnMut(&Artifact, &[u8]) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    scan_streaming_with_limits(
        root,
        data_dir,
        previous,
        roots,
        MAX_DISCOVERY_ENTRIES,
        MAX_SCAN_READ_BYTES,
        emit,
    )
}

fn scan_streaming_with_limits(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    max_entries: usize,
    max_read_bytes: usize,
    emit: impl FnMut(&Artifact, &[u8]) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    scan_streaming_configured(
        root,
        data_dir,
        previous,
        roots,
        ScanLimits {
            discovery_entries: max_entries,
            read_bytes: max_read_bytes,
            ..ScanLimits::default()
        },
        emit,
    )
}

pub fn scan_streaming_with_budgets(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    emit: impl FnMut(&Artifact, &[u8]) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    limits.validate()?;
    scan_streaming_configured(root, data_dir, previous, roots, limits, emit)
}

fn scan_streaming_configured(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    mut emit: impl FnMut(&Artifact, &[u8]) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    let policy = adapters::profile::ExtractionPolicy {
        detail: None,
        version: 1,
        rules: vec![],
    };
    scan_streaming_profiled(
        root,
        data_dir,
        previous,
        roots,
        limits,
        &policy,
        false,
        |path, hash, _profile| {
            Ok(previous
                .artifacts
                .get(path)
                .filter(|old| {
                    old.content_hash == hash.to_string()
                        && previous.extractor_version == EXTRACTOR_VERSION
                })
                .cloned())
        },
        |_, artifact, bytes| emit(artifact, bytes),
    )
}

/// V2 profile-aware scan. Lookup is collection-scoped and returns verified,
/// correctly rebound artifacts; selection precedes lookup and full rebuilds
/// bypass it. Failed lookups abort rather than silently omitting evidence.
#[allow(clippy::too_many_arguments)]
pub fn scan_streaming_v2_with_budgets(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    policy: &adapters::profile::ExtractionPolicy,
    full_reindex: bool,
    lookup: impl FnMut(
        &str,
        &airk_core::identity::Sha256Digest,
        &airk_core::extraction::ExtractionProfile,
    ) -> Result<Option<Artifact>>,
    emit: impl FnMut(&airk_core::extraction::ExtractionProfile, &Artifact, &[u8]) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    limits.validate()?;
    scan_streaming_profiled(
        root,
        data_dir,
        previous,
        roots,
        limits,
        policy,
        full_reindex,
        lookup,
        emit,
    )
}

#[allow(clippy::too_many_arguments)]
fn scan_streaming_profiled(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    policy: &adapters::profile::ExtractionPolicy,
    full_reindex: bool,
    mut lookup: impl FnMut(
        &str,
        &airk_core::identity::Sha256Digest,
        &airk_core::extraction::ExtractionProfile,
    ) -> Result<Option<Artifact>>,
    mut emit: impl FnMut(
        &airk_core::extraction::ExtractionProfile,
        &Artifact,
        &[u8],
    ) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport)> {
    let (snapshot, report, _) = scan_observed_profiled(
        root,
        data_dir,
        previous,
        roots,
        limits,
        policy,
        full_reindex,
        ExtractionExecution::default(),
        |request| {
            Ok(
                lookup(request.path, request.source_hash, request.profile)?.map(|artifact| {
                    observations::VerifiedReuse {
                        artifact,
                        basis: observations::ReuseBasis::SamePath,
                    }
                }),
            )
        },
        |profile, artifact, bytes, _| emit(profile, artifact, bytes),
    )?;
    Ok((snapshot, report))
}

/// Complete, budgeted observations plus independently verified extraction reuse.
/// The caller owns collection isolation, payload verification and binding. Reuse
/// basis does not establish continuity. No observation escapes a failed scan.
#[allow(clippy::too_many_arguments)]
pub fn scan_observed_v2_with_budgets(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    policy: &adapters::profile::ExtractionPolicy,
    full_reindex: bool,
    lookup: impl FnMut(observations::ReuseRequest<'_>) -> Result<Option<observations::VerifiedReuse>>,
    mut emit: impl FnMut(
        &airk_core::extraction::ExtractionProfile,
        &Artifact,
        &[u8],
    ) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport, observations::ScanObservation)> {
    limits.validate()?;
    scan_observed_profiled(
        root,
        data_dir,
        previous,
        roots,
        limits,
        policy,
        full_reindex,
        ExtractionExecution::default(),
        lookup,
        |profile, artifact, bytes, _| emit(profile, artifact, bytes),
    )
}

/// Preserve mapped path-free payloads through persistence without flattening them
/// into the legacy artifact representation and reparsing source.
#[allow(clippy::too_many_arguments)]
pub fn scan_payloads_with_budgets(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    policy: &adapters::profile::ExtractionPolicy,
    full_reindex: bool,
    lookup: impl FnMut(observations::ReuseRequest<'_>) -> Result<Option<observations::VerifiedReuse>>,
    emit: impl FnMut(
        &airk_core::extraction::ExtractionProfile,
        &Artifact,
        &[u8],
        Option<&airk_core::extraction::ExtractionPayload>,
    ) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport, observations::ScanObservation)> {
    limits.validate()?;
    scan_observed_profiled(
        root,
        data_dir,
        previous,
        roots,
        limits,
        policy,
        full_reindex,
        ExtractionExecution::default(),
        lookup,
        emit,
    )
}

/// Parallelize only bounded pure cache misses; reads, reuse and retention stay ordered.
#[allow(clippy::too_many_arguments)]
pub fn scan_payloads_with_execution(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    policy: &adapters::profile::ExtractionPolicy,
    full_reindex: bool,
    execution: ExtractionExecution,
    lookup: impl FnMut(observations::ReuseRequest<'_>) -> Result<Option<observations::VerifiedReuse>>,
    emit: impl FnMut(
        &airk_core::extraction::ExtractionProfile,
        &Artifact,
        &[u8],
        Option<&airk_core::extraction::ExtractionPayload>,
    ) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport, observations::ScanObservation)> {
    limits.validate()?;
    execution.validate()?;
    scan_observed_profiled(
        root,
        data_dir,
        previous,
        roots,
        limits,
        policy,
        full_reindex,
        execution,
        lookup,
        emit,
    )
}

#[allow(clippy::too_many_arguments)]
fn scan_observed_profiled(
    root: &Path,
    data_dir: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
    policy: &adapters::profile::ExtractionPolicy,
    full_reindex: bool,
    execution: ExtractionExecution,
    mut lookup: impl FnMut(
        observations::ReuseRequest<'_>,
    ) -> Result<Option<observations::VerifiedReuse>>,
    emit: impl FnMut(
        &airk_core::extraction::ExtractionProfile,
        &Artifact,
        &[u8],
        Option<&airk_core::extraction::ExtractionPayload>,
    ) -> Result<Artifact>,
) -> Result<(Snapshot, IndexReport, observations::ScanObservation)> {
    use observations::{DisappearanceCause, ReuseBasis};
    let mut observation = observations::ScanObservation::default();
    let registry = adapters::registry::AdapterRegistry::baseline();
    policy.validate(&registry)?;
    let root = root.canonicalize().context("resolve workspace root")?;
    ensure!(
        root.to_string_lossy() == previous.root,
        "workspace root identity changed; collection binding must be preserved"
    );
    let data_dir = data_dir.canonicalize().context("resolve index directory")?;
    let mut report = IndexReport {
        collection: previous.collection.clone(),
        ..Default::default()
    };
    let mut sink = extraction_jobs::ScanSink::new(previous, limits, emit);
    let mut pool = extraction_pool::OrderedPool::new(execution, extraction_jobs::extract)?;
    report.extraction_workers_requested = execution.workers;
    report.extraction_worker_limit = execution.workers;
    report.extraction_result_bound_bytes =
        (execution.workers * airk_core::extraction::MAX_PAYLOAD_BYTES) as u64;
    let mut read_bytes = 0usize;
    let mut sources = vec![(String::new(), root.clone())];
    for (alias, value) in roots {
        let source = Path::new(value).canonicalize()?;
        ensure!(
            source.to_str() == Some(value.as_str()),
            "additional source root identity changed"
        );
        sources.push((format!("@{alias}/"), source));
    }
    for (prefix, source) in &sources {
        observations::verify_root(source, &mut observation.usage, limits.discovery_entries)?;
        observation.roots.insert(
            prefix.trim_start_matches('@').trim_end_matches('/').into(),
            source.to_str().context("non-UTF-8 source root")?.into(),
        );
    }
    let (captured_context, captured_sources) = capture_header_inputs(
        &sources,
        &data_dir,
        policy,
        limits,
        &mut observation.usage,
        &mut read_bytes,
    )?;
    let walker = sources.iter().flat_map(|(prefix, source)| {
        discovery(source, &data_dir).map(move |entry| (prefix, source, entry))
    });
    let scanned = (|| -> Result<()> {
        for (prefix, source, entry) in walker {
            ensure!(
                observation.usage.charged_entries() < limits.discovery_entries,
                "workspace discovery entry budget exhausted; previous generation retained"
            );
            observation.usage.discovery_entries += 1;
            let entry =
                entry.context("workspace discovery failed; previous generation retained")?;
            let Some(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_file() {
                continue;
            }
            let path = entry
                .path()
                .strip_prefix(source)?
                .to_str()
                .context("non-UTF-8 source path is unsupported")?
                .replace('\\', "/");
            let path = format!("{prefix}{path}");
            if path.len() > 4096 {
                sink.drain(&mut pool, &mut report, &mut observation)?;
                report.skip(SkippedFile {
                    path,
                    reason: "path exceeds 4096 bytes".into(),
                });
                continue;
            }
            // Exclusions still consume discovery budget, but known oversized files
            // need no source read. Recheck identity/size inside the bounded reader.
            let source_size = entry.path().symlink_metadata()?.len();
            if source_size > limits.file_bytes as u64 {
                if previous.artifacts.contains_key(&path) {
                    observation
                        .disappeared
                        .insert(path.clone(), DisappearanceCause::Oversized);
                }
                sink.drain(&mut pool, &mut report, &mut observation)?;
                report.skip(SkippedFile {
                    path,
                    reason: format!("file exceeds {} byte configured limit", limits.file_bytes),
                });
                continue;
            }
            let remaining = limits.read_bytes.saturating_sub(read_bytes);
            ensure!(
                captured_sources.contains_key(&path) || source_size <= remaining as u64,
                "workspace raw source read budget exhausted; previous generation retained"
            );
            let bytes = if let Some(bytes) = captured_sources.get(&path) {
                bytes.clone()
            } else {
                let bytes = read_source_with_budget(source, entry.path(), remaining)?;
                read_bytes += bytes.len();
                bytes
            };
            observation.usage.source_read_bytes = read_bytes;
            report.extraction_coordinator_source_peak_bytes = report
                .extraction_coordinator_source_peak_bytes
                .max(bytes.len() as u64);
            if bytes.len() > limits.file_bytes {
                if previous.artifacts.contains_key(&path) {
                    observation
                        .disappeared
                        .insert(path.clone(), DisappearanceCause::Oversized);
                }
                sink.drain(&mut pool, &mut report, &mut observation)?;
                report.skip(SkippedFile {
                    path,
                    reason: format!("file exceeds {} byte configured limit", limits.file_bytes),
                });
                continue;
            }
            let content_hash = format!("{:x}", Sha256::digest(&bytes));
            let hash = content_hash.parse::<airk_core::identity::Sha256Digest>()?;
            let selection = policy.select_in_root_captured(
                &registry,
                &path,
                path.strip_prefix(prefix.as_str()).unwrap_or(&path),
                &bytes,
                &captured_context,
            )?;
            let mut profile = selection.profile;
            // A deliberate OCR operation is retained for unchanged source bytes.
            // Explicit policy overrides and full rebuilds select native extraction.
            if !full_reindex
                && !matches!(
                    selection.basis.as_str(),
                    "exact_override" | "scoped_override"
                )
                && let Some(old) = previous
                    .artifacts
                    .get(&path)
                    .filter(|old| old.content_hash == content_hash && old.language == "ocr")
            {
                profile = match old
                    .retained
                    .as_ref()
                    .and_then(|reference| reference.profile.as_ref())
                {
                    Some(profile) => profile.clone(),
                    None => adapters::legacy::legacy_profile(old)?,
                };
            }
            match policy.detail.unwrap_or_default() {
                airk_core::extraction::IndexDetail::Detailed => {
                    profile.options.remove("index_detail");
                }
                detail => {
                    profile
                        .options
                        .insert("index_detail".into(), detail.as_str().into());
                }
            }
            profile.identity()?;
            if !selection.ambiguity.is_empty() {
                let mut reason = format!(
                    "Ambiguous selection; selected adapter {}: {}",
                    profile.adapter_id,
                    selection
                        .ambiguity
                        .iter()
                        .take(8)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("; ")
                );
                let mut end = reason.len().min(4096);
                while !reason.is_char_boundary(end) {
                    end -= 1;
                }
                reason.truncate(end);
                if report.selection_diagnostics.len() < 1000 {
                    report.selection_diagnostics.push(SkippedFile {
                        path: path.clone(),
                        reason,
                    });
                } else {
                    report.selection_diagnostics_truncated += 1;
                }
            }
            let cached = if full_reindex {
                None
            } else {
                lookup(observations::ReuseRequest {
                    path: &path,
                    canonical_root: source.to_str().context("non-UTF-8 source root")?,
                    source_hash: &hash,
                    source_bytes: bytes.len(),
                    profile: &profile,
                })?
            };
            let captured = extraction_jobs::CapturedExtraction {
                path,
                hash,
                bytes,
                profile,
            };
            if let Some(reused) = cached {
                sink.drain(&mut pool, &mut report, &mut observation)?;
                let old = reused.artifact;
                ensure!(
                    old.path == captured.path
                        && old.content_hash == content_hash
                        && old.bytes == captured.bytes.len()
                        && old.language == captured.profile.language,
                    "cache returned incompatible artifact binding"
                );
                ensure!(
                    old.retained
                        .as_ref()
                        .and_then(|reference| reference.profile.as_ref())
                        .is_none_or(|cached_profile| cached_profile == &captured.profile),
                    "cache returned incompatible extraction profile"
                );
                report.reused += 1;
                match reused.basis {
                    ReuseBasis::SamePath => observation.usage.same_path_hits += 1,
                    ReuseBasis::PriorContent => observation.usage.prior_content_hits += 1,
                }
                sink.retain(&captured, &old, None)?;
            } else {
                ensure!(
                    captured.profile.language != "ocr",
                    "retained OCR payload unavailable at the selected detail; run a full reindex to apply native extraction, then explicitly rerun OCR for affected files"
                );
                observation.usage.parser_calls += 1;
                let parallel = execution.workers > 1
                    && captured.parallel_eligible()
                    && captured.bytes.len() <= execution.max_in_flight_source_bytes;
                if !parallel || !pool.can_admit(captured.bytes.len()) {
                    sink.drain(&mut pool, &mut report, &mut observation)?;
                }
                if parallel {
                    let bytes = captured.bytes.len();
                    pool.push(captured, bytes)?;
                    report.extraction_parallel_jobs += 1;
                } else {
                    report.extraction_serial_jobs += 1;
                    sink.finish(
                        extraction_pool::execute(captured, extraction_jobs::extract),
                        &mut report,
                        &mut observation,
                    )?;
                }
            }
        }
        Ok(())
    })();
    // Earlier source-order failures take precedence over a later discovery/read
    // error. Drop closes channels and joins all workers on either failure path.
    if !sink.failed {
        sink.drain(&mut pool, &mut report, &mut observation)?;
    }
    scanned?;
    report.extraction_workers_spawned = pool.workers_spawned();
    report.extraction_source_peak_bytes = pool.source_peak_bytes as u64;
    report.extraction_wait_us = pool.wait_us;
    let total_bytes = sink.source_bytes;
    let artifacts = sink.artifacts;
    report.removed = previous
        .artifacts
        .keys()
        .filter(|path| !artifacts.contains_key(*path))
        .count();
    for path in previous.artifacts.keys() {
        ensure!(
            observation.usage.charged_entries() < limits.discovery_entries,
            "previous-path reconciliation budget exhausted; previous generation retained"
        );
        observation.usage.reconciled_paths += 1;
        let (alias, relative) = if let Some(aliased) = path.strip_prefix('@') {
            let (alias, relative) = aliased
                .split_once('/')
                .context("invalid previous alias path")?;
            (alias, relative)
        } else {
            ("", path.as_str())
        };
        let old_root = if alias.is_empty() {
            Some(&previous.root)
        } else {
            previous.roots.get(alias)
        };
        let current_root = observation.roots.get(alias);
        let cause = match (old_root, current_root) {
            (_, None) => Some(DisappearanceCause::AliasRemoved),
            (Some(old), Some(current)) if old != current => Some(DisappearanceCause::AliasRebound),
            (None, _) => Some(DisappearanceCause::Unknown),
            (_, Some(current))
                if !artifacts.contains_key(path) && !observation.disappeared.contains_key(path) =>
            {
                Some(observations::reconcile_missing(
                    Path::new(current),
                    relative,
                    &mut observation.usage,
                    limits.discovery_entries,
                )?)
            }
            _ => None,
        };
        if let Some(cause) = cause {
            observation.disappeared.insert(path.clone(), cause);
        }
    }
    for (_, source) in &sources {
        observations::verify_root(source, &mut observation.usage, limits.discovery_entries)?;
    }
    observation.complete = true;
    // Parsing and extraction reuse are independent from inventory changes.
    let changed = artifacts.len() != previous.artifacts.len()
        || artifacts.iter().any(|(path, artifact)| {
            previous.artifacts.get(path).is_none_or(|old| {
                old.content_hash != artifact.content_hash
                    || old.language != artifact.language
                    || old.extraction_method != artifact.extraction_method
                    || old.parse_status != artifact.parse_status
                    || old.retained.as_ref().map(|r| (&r.object, &r.profile))
                        != artifact.retained.as_ref().map(|r| (&r.object, &r.profile))
            })
        })
        || report.indexed > 0
        || previous.extractor_version != EXTRACTOR_VERSION
        || previous.roots != *roots;
    report.generation = if changed {
        previous
            .generation
            .checked_add(1)
            .context("generation overflow")?
    } else {
        previous.generation
    };
    let mut next = Snapshot::empty(previous.root.clone());
    next.collection = previous.collection.clone();
    next.roots = roots.clone();
    next.storage_format = previous.storage_format;
    next.generation = report.generation;
    next.artifacts = artifacts;
    if !changed && let Some(manifest) = &previous.analysis {
        if semantic::validate_build_inputs(&root, manifest).is_ok() {
            next.analysis = previous.analysis.clone();
            next.imported_facts = previous.imported_facts.clone();
            next.imported_relationships = previous.imported_relationships.clone();
        } else {
            next.generation = next
                .generation
                .checked_add(1)
                .context("generation overflow")?;
            report.generation = next.generation;
        }
    }
    report.source_bytes = total_bytes;
    Ok((next, report, observation))
}

impl IndexReport {
    fn skip(&mut self, file: SkippedFile) {
        // Bound diagnostic payloads even when a tree contains many unsupported files.
        if self.skipped.len() < 1000 {
            self.skipped.push(file);
        } else {
            self.skipped_truncated += 1;
        }
    }
}

fn capture_header_inputs(
    sources: &[(String, std::path::PathBuf)],
    data_dir: &Path,
    policy: &adapters::profile::ExtractionPolicy,
    limits: ScanLimits,
    usage: &mut observations::ScanUsage,
    read_bytes: &mut usize,
) -> Result<(header_context::CapturedMap, BTreeMap<String, Vec<u8>>)> {
    let mut wanted: std::collections::BTreeSet<_> =
        policy.declared_configurations().into_iter().collect();
    let mut captured = header_context::CapturedMap::new();
    let mut bytes_by_path = BTreeMap::new();
    if wanted.is_empty() {
        return Ok((captured, bytes_by_path));
    }
    for (prefix, root) in sources {
        for entry in discovery(root, data_dir) {
            ensure!(
                usage.charged_entries() < limits.discovery_entries,
                "header configuration discovery budget exhausted; previous generation retained"
            );
            usage.discovery_entries += 1;
            let entry = entry.context("header configuration discovery failed")?;
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(root)?
                .to_str()
                .context("non-UTF-8 configuration path")?
                .replace('\\', "/");
            let logical = format!("{prefix}{relative}");
            if !wanted.remove(logical.as_str()) {
                continue;
            }
            let maximum = limits
                .file_bytes
                .min(header_context::MAX_CONFIGURATION_BYTES);
            let size = entry.path().symlink_metadata()?.len();
            if size <= maximum as u64 {
                let remaining = limits.read_bytes.saturating_sub(*read_bytes);
                ensure!(
                    size <= remaining as u64,
                    "header configuration read budget exhausted; previous generation retained"
                );
                let bytes =
                    read_source_with_budget(root, entry.path(), remaining.min(maximum + 1))?;
                *read_bytes += bytes.len();
                usage.source_read_bytes = *read_bytes;
                if let Ok(context) = header_context::capture_configuration(&logical, &bytes) {
                    captured.insert(logical.clone(), context);
                }
                bytes_by_path.insert(logical, bytes);
            }
            if wanted.is_empty() {
                return Ok((captured, bytes_by_path));
            }
        }
    }
    Ok((captured, bytes_by_path))
}

fn discovery(root: &Path, data_dir: &Path) -> ignore::Walk {
    let data_dir = data_dir.to_path_buf();
    WalkBuilder::new(root)
        .require_git(false)
        .hidden(false)
        .follow_links(false)
        .add_custom_ignore_filename(".airkignore")
        .filter_entry(move |entry| {
            let name = entry.file_name().to_string_lossy();
            entry.depth() == 0 || (!excluded(&name) && !entry.path().starts_with(&data_dir))
        })
        .build()
}

fn read_source(root: &Path, path: &Path) -> Result<Vec<u8>> {
    read_source_with_budget(root, path, MAX_FILE_BYTES + 1)
}

/// Explicit extraction still respects normal ignore rules and source boundaries.
pub fn read_selected_source(root: &Path, data_dir: &Path, relative: &str) -> Result<Vec<u8>> {
    semantic::validate_path(relative)?;
    let root = root.canonicalize()?;
    let data_dir = data_dir.canonicalize()?;
    let expected = root.join(relative);
    for (visited, entry) in discovery(&root, &data_dir).enumerate() {
        ensure!(
            visited < MAX_DISCOVERY_ENTRIES,
            "selected source discovery budget exhausted"
        );
        let entry = entry?;
        if entry.path() == expected {
            ensure!(
                entry.file_type().is_some_and(|kind| kind.is_file()),
                "selected source is not a regular file"
            );
            let bytes = read_source(&root, &expected)?;
            ensure!(
                bytes.len() <= MAX_FILE_BYTES,
                "selected source exceeds file limit"
            );
            return Ok(bytes);
        }
    }
    anyhow::bail!("selected source is missing or excluded by source discovery policy")
}

fn read_source_with_budget(root: &Path, path: &Path, remaining: usize) -> Result<Vec<u8>> {
    source_read::read(root, path, MAX_FILE_BYTES, remaining)
}

fn excluded(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        ".git"
            | ".airk"
            | "node_modules"
            | "target"
            | ".venv"
            | "__pycache__"
            | ".aws"
            | ".ssh"
            | ".env"
    ) || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
}

pub fn chunk(text: &str) -> Vec<Chunk> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    lines
        .chunks(40)
        .enumerate()
        .map(|(i, lines)| Chunk {
            start_line: i * 40 + 1,
            end_line: i * 40 + lines.len(),
            text: lines.concat(),
            location: DocumentLocation::Lines {
                start: i * 40 + 1,
                end: i * 40 + lines.len(),
            },
        })
        .collect()
}

#[cfg(test)]
mod scan_budget_tests {
    use super::*;

    #[test]
    fn selection_warnings_survive_reuse_without_replacing_native_parse_status() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(root.path().join("original.h"), "int original(\n")?;
        let empty = Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
        let (first, cold) = scan(root.path(), data.path(), &empty)?;
        assert_eq!(cold.selection_diagnostics.len(), 1);
        assert!(
            cold.selection_diagnostics[0]
                .reason
                .contains("selected adapter c")
        );
        assert!(cold.skipped.is_empty());
        assert_eq!(first.artifacts["original.h"].parse_status, "partial-syntax");
        let (second, warm) = scan(root.path(), data.path(), &first)?;
        assert_eq!(warm.reused, 1);
        assert_eq!(warm.selection_diagnostics.len(), 1);
        assert_eq!(
            second.artifacts["original.h"].parse_status,
            first.artifacts["original.h"].parse_status
        );
        assert_eq!(
            warm.selection_diagnostics[0].reason,
            cold.selection_diagnostics[0].reason
        );
        Ok(())
    }

    #[test]
    fn selected_profile_precedes_reuse_and_full_rebuild_bypasses_lookup() -> Result<()> {
        use adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(
            root.path().join("header.h"),
            "class Widget { public: void run() {} };\n",
        )?;
        std::fs::write(root.path().join("stable.rs"), "fn stable() {}\n")?;
        let empty = Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
        let (previous, _) = scan(root.path(), data.path(), &empty)?;
        assert_eq!(previous.artifacts["header.h"].language, "c");
        let policy = ExtractionPolicy {
            detail: None,
            version: 1,
            rules: vec![SelectionRule {
                selector: RuleMatch::Scoped {
                    prefix: String::new(),
                    suffix: Some(".h".into()),
                    basename: None,
                },
                case_sensitive: true,
                adapter_id: "cpp".into(),
            }],
        };
        let mut calls = 0;
        let (next, report) = scan_streaming_v2_with_budgets(
            root.path(),
            data.path(),
            &previous,
            &BTreeMap::new(),
            ScanLimits::default(),
            &policy,
            false,
            |path, hash, profile| {
                calls += 1;
                let old = &previous.artifacts[path];
                assert_eq!(old.content_hash, hash.to_string());
                if path == "header.h" {
                    assert_eq!(profile.language, "cpp");
                }
                Ok((adapters::legacy::legacy_profile(old)? == *profile).then(|| old.clone()))
            },
            |_, artifact, _| Ok(artifact.clone()),
        )?;
        assert_eq!(calls, 2);
        assert_eq!((report.indexed, report.reused), (1, 1));
        assert_eq!(next.artifacts["header.h"].language, "cpp");
        assert!(
            next.artifacts["header.h"]
                .facts
                .iter()
                .any(|fact| fact.name == "Widget" && fact.syntax_kind == "class_specifier")
        );
        let (_, rebuilt) = scan_streaming_v2_with_budgets(
            root.path(),
            data.path(),
            &next,
            &BTreeMap::new(),
            ScanLimits::default(),
            &policy,
            true,
            |_, _, _| anyhow::bail!("full rebuild must not read cache"),
            |_, artifact, _| Ok(artifact.clone()),
        )?;
        assert_eq!((rebuilt.indexed, rebuilt.reused), (2, 0));
        assert!(
            scan_streaming_v2_with_budgets(
                root.path(),
                data.path(),
                &next,
                &BTreeMap::new(),
                ScanLimits::default(),
                &policy,
                false,
                |_, _, _| anyhow::bail!("corrupt verified cache"),
                |_, artifact, _| Ok(artifact.clone())
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn unchanged_ocr_keeps_explicit_profile_without_rerunning_and_full_resets() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let bytes = b"source text only when explicitly re-extracted\n";
        std::fs::write(root.path().join("scan.png"), bytes)?;
        let hash = format!("{:x}", Sha256::digest(bytes));
        let provenance = ocr::OcrProvenance {
            schema: 1,
            source_sha256: hash.clone(),
            tesseract_sha256: hash.clone(),
            renderer_sha256: None,
            traineddata_sha256: BTreeMap::from([("eng".into(), hash.clone())]),
            language: "eng".into(),
            first_page: 1,
            last_page: 1,
            pages_with_text: 0,
            dpi: 150,
            coverage: "fictional fixture".into(),
        };
        let status = serde_json::to_string(&provenance)?;
        let artifact = Artifact {
            path: "scan.png".into(),
            content_hash: hash,
            bytes: bytes.len(),
            chunks: vec![],
            language: "ocr".into(),
            extraction_method: format!(
                "local-tesseract-ocr/1:{:x}",
                Sha256::digest(status.as_bytes())
            ),
            parse_status: status,
            facts: vec![],
            retained: None,
        };
        let mut previous =
            Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
        previous
            .artifacts
            .insert(artifact.path.clone(), artifact.clone());
        let policy = adapters::profile::ExtractionPolicy {
            detail: None,
            version: 1,
            rules: vec![],
        };
        let (retained, report) = scan_streaming_v2_with_budgets(
            root.path(),
            data.path(),
            &previous,
            &BTreeMap::new(),
            ScanLimits::default(),
            &policy,
            false,
            |_, _, profile| {
                assert_eq!(*profile, adapters::legacy::legacy_profile(&artifact)?);
                Ok(Some(artifact.clone()))
            },
            |_, artifact, _| Ok(artifact.clone()),
        )?;
        assert_eq!(report.reused, 1);
        assert_eq!(
            retained.artifacts["scan.png"].parse_status,
            artifact.parse_status
        );
        let (reset, _) = scan_streaming_v2_with_budgets(
            root.path(),
            data.path(),
            &previous,
            &BTreeMap::new(),
            ScanLimits::default(),
            &policy,
            true,
            |_, _, _| anyhow::bail!("full rebuild must bypass OCR cache"),
            |_, artifact, _| Ok(artifact.clone()),
        )?;
        assert_eq!(reset.artifacts["scan.png"].language, "text");
        Ok(())
    }

    #[test]
    fn unsupported_sources_consume_raw_read_budget_without_partial_success() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(root.path().join("one.bin"), [0, 0])?;
        std::fs::write(root.path().join("two.bin"), [0, 0])?;
        let mut previous =
            Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
        previous.generation = 7;
        let error = scan_with_limits(root.path(), data.path(), &previous, 100, 3)
            .expect_err("raw read budget");
        assert!(error.to_string().contains("raw source read budget"));
        let (snapshot, report) = scan_with_limits(root.path(), data.path(), &previous, 100, 4)?;
        assert_eq!(snapshot.generation, 7);
        assert_eq!(report.skipped.len(), 2);
        assert_eq!(
            report.source_bytes, 0,
            "eligible byte accounting stays independent"
        );
        File::create(root.path().join("oversized.bin"))?.set_len((MAX_FILE_BYTES + 1) as u64)?;
        let (_, report) = scan_with_limits(root.path(), data.path(), &previous, 100, 4)?;
        assert_eq!(
            report.skipped.len(),
            3,
            "known oversized sources require no read budget"
        );
        Ok(())
    }

    #[test]
    fn directory_only_trees_consume_discovery_budget() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("one/two/three"))?;
        let previous = Snapshot::empty(root.path().canonicalize()?.to_string_lossy().into_owned());
        let error = scan_with_limits(root.path(), data.path(), &previous, 2, 0)
            .expect_err("directory budget");
        assert!(error.to_string().contains("discovery entry budget"));
        // Four walk entries plus the two charged root-binding checks.
        assert!(scan_with_limits(root.path(), data.path(), &previous, 5, 0).is_err());
        assert!(scan_with_limits(root.path(), data.path(), &previous, 6, 0).is_ok());
        Ok(())
    }
}
