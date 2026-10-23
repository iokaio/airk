// SPDX-License-Identifier: Apache-2.0
//! A single workspace boundary and one application service for CLI and MCP.
use airk_core::{
    Artifact, Citation, DocumentLocation, FactsResponse, Hit, SearchQuery, SearchResponse,
    Snapshot, validate_budget,
};
pub use airk_core::{FactQuery, RelationshipQuery};
use airk_ingest::IndexReport;
pub use airk_ingest::pdf_worker::dispatch as dispatch_pdf_worker;
pub use airk_store::current::CurrentGateContention;

pub mod analysis;
pub mod changes;
pub mod compression;
mod continuity;
pub mod coverage;
pub mod current;
pub mod diagnostics;
mod discovery;
mod execution;
pub mod extraction;
mod historical;
pub use historical::{HistoryAvailability, HistoryAvailabilityRequest};
pub mod history_controls;
pub mod limits;
pub mod maintenance;
/// Read-only native capability/catalog inspection; does not open collection state.
pub use airk_retrieval::model_catalog::inspect_catalog as inspect_model_catalog;
mod publication;
mod resources;
pub mod roots;
pub mod scheduler;
pub mod service_broker;
pub mod service_execution;
pub mod service_projects;
mod service_publication;
pub mod service_resource_observations;
pub mod service_resources;
pub mod storage;
#[cfg(feature = "local-inference")]
mod tokenization;
#[cfg(not(feature = "local-inference"))]
#[path = "remote_tokenization.rs"]
mod tokenization;
pub mod usage;
pub mod vectors;
pub mod watcher;
use airk_retrieval::truncate_utf8;
use airk_store::{CollectionUsage, FileStore};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct Kernel {
    progress: Option<ProgressObserver>,
    root: PathBuf,
    collection: String,
    data_root: PathBuf,
    store: FileStore,
    service_collection: Option<airk_store::service::collection::Collection>,
    service_selection: Option<Arc<airk_store::service::collection::Selection>>,
}

type ProgressObserver = Arc<dyn Fn(&airk_core::readiness::IndexProgress) + Send + Sync>;

#[cfg(windows)]
fn managed_data_path(path: PathBuf) -> Result<PathBuf> {
    // Resolve the nearest existing directory without creating managed state on
    // read-only open. Canonical Windows paths carry the extended prefix needed
    // by native dependencies when later component filenames cross MAX_PATH.
    let mut existing = std::path::absolute(path)?;
    let mut missing = Vec::new();
    loop {
        match std::fs::metadata(&existing) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_dir(),
                    "data directory ancestor is not a directory"
                );
                let mut resolved = existing.canonicalize()?;
                for component in missing.into_iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    existing
                        .file_name()
                        .context("data directory has no existing ancestor")?
                        .to_os_string(),
                );
                ensure!(existing.pop(), "data directory has no existing ancestor");
            }
            Err(error) => return Err(error).context("resolve managed data directory"),
        }
    }
}

#[derive(Serialize)]
pub struct Status {
    pub execution: airk_core::execution::ExecutionPlan,
    pub indexing: airk_core::extraction::IndexCoverage,
    pub freshness: airk_core::changes::FreshnessObservation,
    pub retention: maintenance::RetentionObservation,
    pub publication: Option<airk_core::evidence::RevisionEnvelope>,
    pub semantic_readiness: airk_core::readiness::SemanticReadiness,
    pub discovery_readiness: airk_core::readiness::DiscoveryReadiness,
    pub source_roots: std::collections::BTreeMap<String, String>,
    pub generation: u64,
    pub collection: String,
    pub files: usize,
    pub chunks: usize,
    pub source_bytes: usize,
    pub extractor_version: String,
    pub exact_fact_backend: &'static str,
    pub lexical_backend: &'static str,
    pub vector_backend: &'static str,
    pub local_model_ready: bool,
    pub vectors_enabled: bool,
    pub indexed_vectors: usize,
    pub vector_selection_digest: Option<airk_core::identity::Sha256Digest>,
    pub vector_eligible_artifacts: usize,
    pub vector_excluded_artifacts: usize,
    pub semantic_producer: Option<String>,
    pub imported_facts: usize,
    pub imported_relationships: usize,
}

#[derive(Serialize)]
pub struct FileList {
    #[serde(flatten)]
    pub page: airk_core::discovery::DiscoveryPage,
    pub publication: Option<airk_core::evidence::RevisionEnvelope>,
    pub generation: u64,
    pub collection: String,
    pub paths: Vec<String>,
    pub truncated: bool,
}

#[derive(Serialize)]
pub struct FileInfo {
    pub publication: Option<airk_core::evidence::RevisionEnvelope>,
    pub source: airk_store::roots::SourceLocation,
    pub generation: u64,
    pub collection: String,
    pub path: String,
    pub content_hash: String,
    pub source_bytes: usize,
    pub language: String,
    pub extraction_method: String,
    pub parse_status: String,
    pub chunks: usize,
    pub facts: usize,
    pub indexed_lines: usize,
    pub line_basis: &'static str,
    pub vector_selection: airk_core::vector_selection::VectorSelectionDecision,
    pub vector_selection_digest: airk_core::identity::Sha256Digest,
}

#[derive(Serialize)]
pub struct ReferencesResponse {
    pub resolution: &'static str,
    pub evidence: SearchResponse,
}

impl Kernel {
    pub fn open(root: impl AsRef<Path>, data_dir: Option<PathBuf>) -> Result<Self> {
        Self::open_collection(root, data_dir, "default")
    }

    pub fn open_collection(
        root: impl AsRef<Path>,
        data_dir: Option<PathBuf>,
        collection: &str,
    ) -> Result<Self> {
        Self::open_collection_with_layout(root, data_dir, collection, true, false)
    }

    /// Service-only fresh storage. Never migrates or opens a legacy collection.
    pub fn open_service_collection(
        root: impl AsRef<Path>,
        data_dir: Option<PathBuf>,
        collection: &str,
    ) -> Result<Self> {
        Self::open_collection_with_layout(root, data_dir, collection, true, true)
    }

    /// The project owner can report failed captured recovery and accept an explicit
    /// fresh rebuild without silently discarding the failed journal on startup.
    pub fn open_service_collection_for_diagnostics(
        root: impl AsRef<Path>,
        data_dir: Option<PathBuf>,
        collection: &str,
    ) -> Result<Self> {
        Self::open_collection_with_layout(root, data_dir, collection, false, true)
    }

    /// Open retained state for inspection without replaying pending publications
    /// or runtime bootstrap recovery. Reads reject an incomplete publication.
    pub fn open_collection_for_diagnostics(
        root: impl AsRef<Path>,
        data_dir: Option<PathBuf>,
        collection: &str,
    ) -> Result<Self> {
        Self::open_collection_with_layout(root, data_dir, collection, false, false)
    }

    fn open_collection_with_recovery(
        root: impl AsRef<Path>,
        data_dir: Option<PathBuf>,
        collection: &str,
        recover: bool,
    ) -> Result<Self> {
        Self::open_collection_with_layout(root, data_dir, collection, recover, false)
    }

    fn open_collection_with_layout(
        root: impl AsRef<Path>,
        data_dir: Option<PathBuf>,
        collection: &str,
        recover: bool,
        service: bool,
    ) -> Result<Self> {
        airk_store::validate_collection(collection)?;
        let root = root
            .as_ref()
            .canonicalize()
            .context("resolve workspace root")?;
        ensure!(
            root.to_str().is_some(),
            "non-UTF-8 workspace root is unsupported; collection identities must be lossless"
        );
        ensure!(root.is_dir(), "workspace root must be a directory");
        let directory = data_dir.unwrap_or_else(|| root.join(".airk"));
        #[cfg(windows)]
        let directory = managed_data_path(directory)?;
        airk_store::relocation::check_root(&directory)?;
        ensure!(
            directory != root,
            "data directory must not be the workspace root"
        );
        if directory.exists() {
            ensure!(
                !root.starts_with(directory.canonicalize()?),
                "data directory must not contain the workspace root"
            );
        }
        let service_collection = if service {
            std::fs::create_dir_all(&directory)?;
            let collection = airk_store::service::collection::Collection::initialize(
                &directory,
                root.to_str().context("non-UTF-8 workspace")?,
                collection,
                service_broker::new_instance_id()?,
            )?;
            collection.recover_abandoned_replacement()?;
            Some(collection)
        } else {
            None
        };
        let store = if let Some(collection) = &service_collection {
            collection.selected_store()?
        } else {
            FileStore::new(directory.join("collections-1.2").join(collection))
        };
        let kernel = Self {
            progress: None,
            root,
            collection: collection.into(),
            data_root: directory.clone(),
            store,
            service_collection,
            service_selection: None,
        };
        if recover {
            kernel.recover_runtime_bootstrap()?;
            if let Some(scope) = kernel.current_scope()? {
                let needs_recovery = match kernel
                    .store
                    .current_read(&scope)
                    .and_then(|r| r.has_prepared_update())
                {
                    Ok(prepared) => prepared,
                    // A writer can acquire the gate after scope discovery.
                    // Preserve contention instead of trying to recover its work
                    // and replacing the cause with a writer-lock error.
                    Err(error) if error.downcast_ref::<CurrentGateContention>().is_some() => {
                        return Err(error);
                    }
                    Err(_) => true,
                };
                if needs_recovery {
                    current::recover_current(
                        &kernel.store,
                        &airk_store::quota::Quota::open(&kernel.data_root)?,
                        &scope,
                    )?;
                }
            }
        }
        Ok(kernel)
    }

    fn root_id(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Observers receive stage updates and committed boundaries and must not reenter the writer.
    pub fn with_index_progress(
        mut self,
        observer: impl Fn(&airk_core::readiness::IndexProgress) + Send + Sync + 'static,
    ) -> Self {
        self.progress = Some(Arc::new(observer));
        self
    }
    pub(crate) fn emit_progress(&self, mut progress: airk_core::readiness::IndexProgress) {
        progress.collection = self.collection.clone();
        if let Some(observer) = &self.progress {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(&progress)));
        }
    }
    pub fn data_dir(&self) -> &Path {
        &self.data_root
    }
    pub fn collection(&self) -> &str {
        &self.collection
    }
    pub fn collections(&self) -> Result<Vec<CollectionUsage>> {
        airk_store::list_collections(&self.data_root)
    }
    pub fn remove_collection(&self, id: &str) -> Result<CollectionUsage> {
        airk_store::remove_collection(&self.data_root, id)
    }
    pub fn prune_indexes(&self, _dry_run: bool) -> Result<airk_store::retention::PruneReport> {
        anyhow::bail!(
            "generation pruning is unsupported in AIRK 1.2; current objects are reclaimed automatically"
        )
    }
    fn empty_snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot::empty(self.root_id());
        snapshot.collection = self.collection.clone();
        snapshot.persistent_indexes = true;
        snapshot.storage_format = 1;
        snapshot
    }

    fn load_optional(&self) -> Result<Option<Snapshot>> {
        let Some(scope) = self.current_scope()? else {
            return Ok(None);
        };
        let read = self.store.current_read(&scope)?;
        self.current_scan_projection(&read).map(Some)
    }

    fn load(&self) -> Result<Snapshot> {
        self.load_optional()?
            .context("workspace is not indexed; run airk index first")
    }

    pub fn initialize(&self) -> Result<Status> {
        self.initialize_current()
    }

    pub fn index(&self) -> Result<IndexReport> {
        self.index_with_mode(false)
    }

    /// Re-extract all sources. Obsolete derived formats are reset before rebuilding.
    pub fn reindex(&self) -> Result<IndexReport> {
        self.index_with_mode(true)
    }

    fn index_with_mode(&self, full: bool) -> Result<IndexReport> {
        self.index_current(full, false)
    }

    pub fn status(&self) -> Result<Status> {
        self.current_status()
    }

    pub fn search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        self.current_search(query, current::CurrentSearchMode::Lexical)
    }

    pub fn semantic_search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        self.vector_search(query, false)
    }

    pub fn hybrid_search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        self.vector_search(query, true)
    }

    fn vector_search(&self, query: &SearchQuery, hybrid: bool) -> Result<SearchResponse> {
        ensure!(
            self.vectors_enabled()?,
            "vector search is disabled for this collection"
        );
        self.current_search(
            query,
            if hybrid {
                current::CurrentSearchMode::Hybrid
            } else {
                current::CurrentSearchMode::Vector
            },
        )
    }

    fn configured_model(&self) -> Result<Option<String>> {
        if !self.vectors_enabled()? {
            return Ok(None);
        }
        let directory = self.data_root.join("models/local");
        if directory.exists() {
            Ok(Some(airk_retrieval::model::inspect(&directory)?.identity))
        } else {
            Ok(None)
        }
    }

    fn publication_reservation(&self) -> Result<airk_store::quota::Reservation> {
        self.publication_reservation_with_limit(4 << 30)
    }

    fn publication_reservation_with_limit(
        &self,
        maximum: u64,
    ) -> Result<airk_store::quota::Reservation> {
        let quota = airk_store::quota::Quota::open(&self.data_root)?;
        // The reservation includes temporary and retained bytes together. Each
        // provider charges before writes; exhausting it leaves the active pin intact.
        let status = quota.status()?;
        let root_free = status
            .root_limit_bytes
            .saturating_sub(status.used_bytes)
            .saturating_sub(status.reserved_bytes);
        let free = status
            .collections
            .get(&self.collection)
            .map_or(root_free, |c| {
                c.limit_bytes
                    .saturating_sub(c.used_bytes)
                    .saturating_sub(c.reserved_bytes)
            })
            .min(root_free);
        quota.reserve_available(&self.collection, (free / 2).min(maximum))
    }

    pub fn read_file(
        &self,
        path: &str,
        start_line: usize,
        end_line: usize,
        max_bytes: usize,
    ) -> Result<SearchResponse> {
        let read = self.current_reader()?;
        let artifact = self.current_artifact(&read, path, true)?;
        let mut metadata = self.empty_snapshot();
        metadata.generation = read.head().sequence;
        Self::read_snapshot(
            Some(current::runtime::envelope(&read)?),
            &metadata,
            &artifact,
            path,
            start_line,
            end_line,
            max_bytes,
        )
    }

    fn read_snapshot(
        publication: Option<airk_core::evidence::RevisionEnvelope>,
        snapshot: &Snapshot,
        artifact: &Artifact,
        path: &str,
        start_line: usize,
        end_line: usize,
        max_bytes: usize,
    ) -> Result<SearchResponse> {
        validate_budget(1, max_bytes)?;
        ensure!(
            start_line > 0 && end_line >= start_line,
            "invalid line range"
        );
        // Lookup only in inventory: caller paths never become filesystem reads.
        let last_line = artifact.chunks.last().map_or(0, |c| c.end_line);
        ensure!(end_line <= last_line, "line range exceeds indexed source");
        let locations: Vec<_> = artifact
            .chunks
            .iter()
            .filter(|c| c.end_line >= start_line && c.start_line <= end_line)
            .map(|c| &c.location)
            .collect();
        let location = if artifact.extraction_method == "utf8" {
            DocumentLocation::Lines {
                start: start_line,
                end: end_line,
            }
        } else {
            let location = locations.first().context("document location unavailable")?;
            ensure!(
                locations.iter().all(|other| *other == *location),
                "range crosses document locations; request one page/block/slide at a time"
            );
            (*location).clone()
        };
        let mut remaining = max_bytes;
        let mut text = String::new();
        let mut truncated = false;
        for chunk in &artifact.chunks {
            for (offset, line) in chunk.text.split_inclusive('\n').enumerate() {
                let number = chunk.start_line + offset;
                if number < start_line || number > end_line {
                    continue;
                }
                let (part, cut) = truncate_utf8(line, remaining);
                remaining -= part.len();
                text.push_str(&part);
                if cut {
                    truncated = true;
                    break;
                }
            }
            if truncated {
                break;
            }
        }
        Ok(SearchResponse {
            publication,
            semantic_readiness: None,
            discovery_readiness: None,
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            truncated,
            hits: vec![Hit {
                citation: Citation {
                    path: path.into(),
                    content_hash: artifact.content_hash.clone(),
                    start_line,
                    end_line,
                    location,
                },
                text,
                truncated,
            }],
        })
    }

    pub fn file_info(&self, path: &str) -> Result<FileInfo> {
        self.current_file_info(self.current_reader()?.as_ref(), path)
    }

    pub fn relationships(
        &self,
        query: &RelationshipQuery,
    ) -> Result<airk_core::RelationshipsResponse> {
        let read = self.current_reader()?;
        let result = read.native_relationships(&self.store, query)?;
        let configuration = read
            .collection_state()?
            .context("current configuration missing")?;
        Ok(airk_core::RelationshipsResponse {
            indexing: configuration.configuration.index_detail.into(),
            publication: Some(current::runtime::envelope(&read)?),
            generation: read.head().sequence,
            collection: self.collection.clone(),
            relationships: result.relationships,
            truncated: result.truncated,
            coverage: result.coverage.into(),
        })
    }

    /// Prepare binding before the external producer runs; outputs JSON through CLI.
    fn verify_source_inventory(&self, snapshot: &Snapshot) -> Result<Snapshot> {
        ensure!(
            self.configured_roots()? == snapshot.roots,
            "index pending source root changes before preparing or publishing provider analysis"
        );
        let policy = self.extraction_policy()?;
        ensure!(
            !policy.pending_reindex,
            "index pending extraction policy before preparing provider analysis"
        );
        let (current, _) = airk_ingest::scan_streaming_v2_with_budgets(
            &self.root,
            &self.data_root,
            snapshot,
            &snapshot.roots,
            self.scan_limits()?,
            &policy.policy,
            false,
            |path, hash, profile| {
                Ok(snapshot
                    .artifacts
                    .get(path)
                    .filter(|artifact| {
                        artifact.content_hash == hash.to_string()
                            && artifact
                                .retained
                                .as_ref()
                                .and_then(|r| r.profile.as_ref())
                                .is_some_and(|p| p == profile)
                    })
                    .cloned())
            },
            |_, artifact, _| {
                // Verification needs identities, never a second in-memory corpus.
                let mut metadata = artifact.clone();
                metadata.chunks.clear();
                metadata.facts.clear();
                Ok(metadata)
            },
        )?;
        Ok(current)
    }

    pub fn prepare_analysis(
        &self,
        producer: &str,
        version: &str,
        build_inputs: &[String],
    ) -> Result<airk_core::AnalysisManifest> {
        let _lock = self.store.lock()?;
        let snapshot = self.load()?;
        ensure!(
            snapshot.roots.is_empty(),
            "SCIP imports require a single primary root; additional source aliases are not supported by this adapter"
        );
        let current = self.verify_source_inventory(&snapshot)?;
        ensure!(
            airk_ingest::semantic::source_inventory(&snapshot)
                == airk_ingest::semantic::source_inventory(&current)
                && snapshot.extractor_version == airk_core::EXTRACTOR_VERSION,
            "reindex before preparing analysis"
        );
        airk_ingest::semantic::prepare(&self.root, &snapshot, producer, version, build_inputs)
    }

    pub fn import_scip(&self, index: &Path, manifest: &Path) -> Result<Status> {
        let writer = self.store.lock()?;
        let baseline = self.load()?;
        let source = airk_store::blobs::RetainedSource {
            store: &self.store,
            snapshot: &baseline,
        };
        let mut snapshot = baseline.clone();
        ensure!(
            snapshot.roots.is_empty(),
            "SCIP imports require a single primary root; additional source aliases are not supported by this adapter"
        );
        let binding: airk_core::AnalysisManifest =
            serde_json::from_slice(&airk_ingest::semantic::read_bounded(manifest, 8_388_608)?)?;
        airk_ingest::semantic::validate_build_inputs(&self.root, &binding)?;
        let current = self.verify_source_inventory(&snapshot)?;
        ensure!(
            airk_ingest::semantic::source_inventory(&snapshot)
                == airk_ingest::semantic::source_inventory(&current)
                && snapshot.extractor_version == airk_core::EXTRACTOR_VERSION,
            "source changed; reindex and regenerate analysis"
        );
        let bytes =
            airk_ingest::semantic::read_bounded(index, airk_ingest::semantic::MAX_IMPORT_BYTES)?;
        airk_ingest::semantic::import_with_source(&mut snapshot, binding.clone(), &bytes, &source)?;
        airk_ingest::semantic::validate_build_inputs(&self.root, &binding)?;
        snapshot.generation = snapshot
            .generation
            .checked_add(1)
            .context("generation overflow")?;
        snapshot.persistent_indexes = true;
        let verified = snapshot;
        self.with_pressure_retry(&writer, || {
            airk_ingest::semantic::validate_build_inputs(&self.root, &binding)?;
            let current = self.verify_source_inventory(&verified)?;
            ensure!(
                airk_ingest::semantic::source_inventory(&current)
                    == airk_ingest::semantic::source_inventory(&verified),
                "source changed before SCIP publication; previous generation retained"
            );
            let mut snapshot = verified.clone();
            let reservation = self.publication_reservation()?;
            for artifact in snapshot.artifacts.values_mut() {
                *artifact =
                    self.store
                        .retain_artifact_with_budget(artifact, None, Some(&reservation))?;
            }
            snapshot.storage_format = 1;
            snapshot.embedding_model = self.configured_model()?;
            self.publish_v2(
                &writer,
                &snapshot,
                reservation,
                airk_core::changes::PublicationReason::ScipImport,
            )?;
            Ok(())
        })?;
        self.status()
    }

    pub fn context(
        &self,
        path: &str,
        line: usize,
        before: usize,
        after: usize,
        max_bytes: usize,
    ) -> Result<SearchResponse> {
        ensure!(
            before <= 100 && after <= 100,
            "context radius must be 0..100"
        );
        let read = self.current_reader()?;
        let mut metadata = self.empty_snapshot();
        metadata.generation = read.head().sequence;
        let artifact = self.current_artifact(&read, path, true)?;
        let indexed_lines = artifact.chunks.last().map_or(0, |chunk| chunk.end_line);
        ensure!(
            line > 0 && line <= indexed_lines,
            "line is outside indexed source"
        );
        Self::read_snapshot(
            Some(current::runtime::envelope(&read)?),
            &metadata,
            &artifact,
            path,
            line.saturating_sub(before).max(1),
            line.saturating_add(after).min(indexed_lines),
            max_bytes,
        )
    }

    pub fn references(
        &self,
        name: &str,
        path_prefix: &str,
        limit: usize,
        max_bytes: usize,
    ) -> Result<ReferencesResponse> {
        validate_budget(limit, max_bytes)?;
        ensure!(
            !name.is_empty()
                && name.len() <= 1024
                && path_prefix.len() <= 4096
                && name.chars().all(|c| c.is_alphanumeric() || c == '_'),
            "reference name must be a single identifier"
        );
        let read = self.current_reader()?;
        let snapshot = self.current_scan_projection(&read)?;
        let mut evidence = SearchResponse {
            publication: Some(current::runtime::envelope(&read)?),
            semantic_readiness: Some(current::enrichment::readiness(&read, &self.store)?),
            discovery_readiness: Some(current::progress::readiness(&read, &self.store)?),
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            hits: Vec::new(),
            truncated: false,
        };
        let mut remaining = max_bytes;
        let mut read_bytes = 0usize;
        let mut artifacts = std::collections::BTreeMap::new();
        let lexical = current::open_selected_lexical(&self.store, &read)?;
        let candidates = lexical.search(&SearchQuery {
            text: name.into(),
            path_prefix: Some(path_prefix.into()),
            limit: 100,
            max_bytes: 65536,
        })?;
        // Bound retained candidate scanning independently of authenticated index selection.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
        evidence.truncated |= candidates.incomplete;
        'files: for candidate in candidates.values {
            let path = &candidate.citation.path;
            let entry = snapshot
                .artifacts
                .get(path)
                .context("reference candidate missing from current index")?;
            if std::time::Instant::now() >= deadline {
                evidence.truncated = true;
                break;
            }
            if !artifacts.contains_key(path) {
                read_bytes = read_bytes.saturating_add(entry.extracted_bytes());
                if read_bytes > 16_777_216 {
                    evidence.truncated = true;
                    break;
                }
                artifacts.insert(path.clone(), self.current_artifact(&read, path, false)?);
            }
            let a = &artifacts[path];
            let chunk = a
                .chunks
                .get(usize::try_from(candidate.local)?)
                .context("reference candidate chunk missing")?;
            ensure!(
                candidate.stable_id
                    == airk_store::current::lexical::stable_id(path, candidate.local)?
                    && candidate.citation.content_hash == a.content_hash
                    && candidate.citation.start_line == chunk.start_line
                    && candidate.citation.end_line == chunk.end_line
                    && candidate.citation.location == chunk.location,
                "reference candidate differs from retained evidence"
            );
            for (offset, line) in chunk.text.split_inclusive('\n').enumerate() {
                if !line
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|word| word == name)
                {
                    continue;
                }
                if evidence.hits.len() == limit || remaining == 0 {
                    evidence.truncated = true;
                    break 'files;
                }
                let (text, truncated) = truncate_utf8(line, remaining);
                remaining -= text.len();
                evidence.truncated |= truncated;
                let number = chunk.start_line + offset;
                let location = if a.extraction_method == "utf8" {
                    DocumentLocation::Lines {
                        start: number,
                        end: number,
                    }
                } else {
                    chunk.location.clone()
                };
                evidence.hits.push(Hit {
                    text,
                    truncated,
                    citation: Citation {
                        path: path.clone(),
                        content_hash: a.content_hash.clone(),
                        start_line: number,
                        end_line: number,
                        location,
                    },
                });
            }
        }
        Ok(ReferencesResponse {
            resolution: "textual-occurrences; includes comments, strings and definitions; no name resolution",
            evidence,
        })
    }
}
