// SPDX-License-Identifier: Apache-2.0
//! A single workspace boundary and one application service for CLI and MCP.
use airk_core::{
    Artifact, ArtifactSource, Citation, DocumentLocation, FactsResponse, Hit, SearchQuery,
    SearchResponse, Snapshot, validate_budget,
};
pub use airk_core::{FactQuery, RelationshipQuery};
use airk_ingest::IndexReport;
pub use airk_ingest::pdf_worker::dispatch as dispatch_pdf_worker;

pub mod analysis;
pub mod changes;
mod conditional;
mod continuity;
pub mod coverage;
mod discovery;
pub mod extraction;
mod historical;
mod suggestions;
pub use historical::{HistoryAvailability, HistoryAvailabilityRequest};
pub mod history_controls;
pub mod limits;
mod lineage;
pub mod maintenance;
mod publication;
mod reconciliation;
mod resources;
pub mod roots;
pub mod scheduler;
pub mod service;
pub mod storage;
mod tokenization;
pub mod usage;
pub mod vectors;
pub mod watcher;
use airk_retrieval::{persistent::PersistentIndex, truncate_utf8};
use airk_store::{CollectionUsage, FileStore};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct Kernel {
    root: PathBuf,
    collection: String,
    data_root: PathBuf,
    store: FileStore,
    cache: Arc<Mutex<Option<Arc<CachedIndex>>>>,
    catalog_cache: Arc<Mutex<Option<Arc<airk_store::blobs::Catalog>>>>,
    retention_observer: Option<Arc<maintenance::RetentionObserver>>,
    #[cfg(test)]
    retention_test_budget: Option<maintenance::MaintenanceBudget>,
}

struct CachedIndex {
    revision: String,
    index: Box<PersistentIndex>,
}

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
    pub retention: maintenance::RetentionObservation,
    pub publication: Option<airk_core::evidence::RevisionEnvelope>,
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
        Ok(Self {
            root,
            collection: collection.into(),
            data_root: directory.clone(),
            store: FileStore::new(directory.join("collections").join(collection)),
            cache: Arc::new(Mutex::new(None)),
            catalog_cache: Arc::new(Mutex::new(None)),
            retention_observer: None,
            #[cfg(test)]
            retention_test_budget: None,
        })
    }

    fn root_id(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    pub fn root(&self) -> &Path {
        &self.root
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
        if id == self.collection {
            *self
                .catalog_cache
                .lock()
                .map_err(|_| anyhow::anyhow!("catalog cache poisoned"))? = None;
            *self
                .cache
                .lock()
                .map_err(|_| anyhow::anyhow!("index cache poisoned"))? = None;
        }
        airk_store::remove_collection(&self.data_root, id)
    }
    pub fn prune_indexes(&self, dry_run: bool) -> Result<airk_store::retention::PruneReport> {
        let writer = self.store.lock()?;
        let snapshot = self.load()?;
        ensure!(
            snapshot.persistent_indexes,
            "reindex before pruning legacy state"
        );
        let quota = airk_store::quota::Quota::open(&self.data_root)?;
        self.release_idle_pins();
        self.store.prune_retention(&writer, &quota, dry_run)
    }
    fn empty_snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot::empty(self.root_id());
        snapshot.collection = self.collection.clone();
        snapshot.persistent_indexes = true;
        snapshot.storage_format = 1;
        snapshot
    }

    fn load_optional(&self) -> Result<Option<Snapshot>> {
        match self.store.index_format(&self.root_id(), &self.collection)? {
            airk_store::IndexFormat::Empty => return Ok(None),
            airk_store::IndexFormat::Current => {}
            _ => anyhow::bail!("obsolete or interrupted index; rebuild with airk index --full"),
        }
        let snapshot = self.store.catalog()?.map(|catalog| catalog.snapshot);
        if let Some(snapshot) = &snapshot {
            ensure!(
                snapshot.root == self.root_id(),
                "index belongs to another root; use a separate data directory"
            );
            ensure!(
                snapshot.collection == self.collection,
                "snapshot belongs to another collection"
            );
        }
        Ok(snapshot)
    }

    fn load(&self) -> Result<Snapshot> {
        self.load_optional()?
            .context("workspace is not indexed; run airk index first")
    }

    fn require_current_index(&self) -> Result<()> {
        ensure!(
            self.store.index_format(&self.root_id(), &self.collection)?
                == airk_store::IndexFormat::Current,
            "workspace is not indexed in the current format; rebuild with airk index --full"
        );
        Ok(())
    }

    fn catalog(&self) -> Result<Arc<airk_store::blobs::Catalog>> {
        self.require_current_index()?;
        let cached = self
            .catalog_cache
            .lock()
            .map_err(|_| anyhow::anyhow!("catalog cache poisoned"))?
            .clone();
        let header = self.persistent_revision()?;
        if let (Some(header), Some(cached)) = (&header, cached.as_ref())
            && cached.receipt.as_ref().is_some_and(|receipt| {
                header.token == Some(receipt.token)
                    && header.inventory_sha256.as_deref()
                        == Some(receipt.inventory_digest.to_string().as_str())
                    && header.manifest_sha256.as_deref()
                        == Some(receipt.manifest_digest.to_string().as_str())
            })
        {
            return Ok(cached.clone());
        }
        let catalog = self
            .store
            .catalog()?
            .context("workspace is not indexed; run airk index first")?;
        ensure!(
            catalog.snapshot.root == self.root_id()
                && catalog.snapshot.collection == self.collection,
            "index belongs to another root or collection"
        );
        let catalog = Arc::new(catalog);
        // Filesystem/SQLite work above is outside the cache mutex. Concurrent
        // callers can keep their pinned view; don't replace a newer installed pin.
        let mut cache = self
            .catalog_cache
            .lock()
            .map_err(|_| anyhow::anyhow!("catalog cache poisoned"))?;
        let replace =
            cache
                .as_ref()
                .is_none_or(|current| match (&current.receipt, &catalog.receipt) {
                    (Some(current), Some(candidate)) => {
                        current.token.incarnation != candidate.token.incarnation
                            || current.token.generation <= candidate.token.generation
                    }
                    _ => true,
                });
        if replace {
            *cache = Some(catalog.clone());
        }
        Ok(catalog)
    }

    pub fn initialize(&self) -> Result<Status> {
        let writer = self.store.lock()?;
        ensure!(
            !self.root.starts_with(self.data_root.canonicalize()?),
            "data directory must not contain the workspace root"
        );
        if self.load_optional()?.is_none() {
            self.prepare_retention(&writer)?;
            self.with_pressure_retry(&writer, || {
                let mut snapshot = self.empty_snapshot();
                snapshot.embedding_model = self.configured_model()?;
                let reservation = self.publication_reservation()?;
                self.publish_v2(
                    &writer,
                    &snapshot,
                    reservation,
                    airk_core::changes::PublicationReason::Initialization,
                )?;
                Ok(())
            })?;
        }
        self.status()
    }

    pub fn index(&self) -> Result<IndexReport> {
        self.index_with_mode(false)
    }

    /// Re-extract all sources. Obsolete derived formats are reset before rebuilding.
    pub fn reindex(&self) -> Result<IndexReport> {
        self.index_with_mode(true)
    }

    fn index_with_mode(&self, full: bool) -> Result<IndexReport> {
        self.release_idle_pins();
        let writer = self.store.lock()?;
        ensure!(
            !self.root.starts_with(self.data_root.canonicalize()?),
            "data directory must not contain the workspace root"
        );
        let format = self.store.index_format(&self.root_id(), &self.collection)?;
        if matches!(
            format,
            airk_store::IndexFormat::Obsolete | airk_store::IndexFormat::ResetRequired
        ) {
            ensure!(
                full,
                "obsolete or interrupted index; rebuild with airk index --full"
            );
            self.store
                .reset_obsolete_index(&writer, &self.root_id(), &self.collection)?;
            // Recovery does not read or prune obsolete catalog/history formats.
            return self.index_attempt(true, &writer);
        }
        self.prepare_retention(&writer)?;
        self.with_pressure_retry(&writer, || self.index_attempt(full, &writer))
    }

    fn index_attempt(&self, full: bool, writer: &airk_store::WriterLock) -> Result<IndexReport> {
        let resetting = self.store.reset_required()?;
        let mut previous = if resetting {
            self.empty_snapshot()
        } else {
            self.load_optional()?
                .unwrap_or_else(|| self.empty_snapshot())
        };
        if full {
            previous.extractor_version.clear();
        }
        let desired_model = self.configured_model()?;
        let desired_pipeline = if desired_model.is_some() {
            Some(
                airk_retrieval::model::inspect_cache_descriptor(
                    &self.data_root.join("models/local"),
                )?
                .identity()?,
            )
        } else {
            None
        };
        let prior_inventory = if resetting {
            None
        } else {
            self.store.catalog()?.and_then(|catalog| catalog.inventory)
        };
        let prior_pipeline = prior_inventory
            .as_ref()
            .and_then(|inventory| inventory.embedding_pipeline_digest);
        let selection_digest = self.vector_selection()?.digest()?;
        let prior_selection_digest = prior_inventory
            .as_ref()
            .map(|inventory| inventory.vector_selection.digest())
            .transpose()?;
        drop(prior_inventory);
        let roots = self.configured_roots()?;
        let reservation = self.publication_reservation()?;
        let scan_started = std::time::Instant::now();
        let configured_policy = if resetting {
            self.rebuild_extraction_policy()?
        } else {
            self.extraction_policy()?
        };
        let policy = configured_policy.policy;
        let mut prior_content = std::collections::BTreeMap::new();
        if !full {
            for old in previous.artifacts.values() {
                if let Some(profile) = old
                    .retained
                    .as_ref()
                    .and_then(|reference| reference.profile.as_ref())
                {
                    let old_root = airk_store::roots::source_location(
                        &previous.root,
                        &previous.roots,
                        &old.path,
                    )?;
                    let current_root =
                        airk_store::roots::source_location(&previous.root, &roots, &old.path)?;
                    if old_root.root == current_root.root {
                        prior_content
                            .entry((old.content_hash.clone(), profile.identity()?, old.bytes))
                            .or_insert(old);
                    }
                }
            }
        }
        let (mut snapshot, mut report, observation) = airk_ingest::scan_payloads_with_budgets(
            &self.root,
            &self.data_root,
            &previous,
            &roots,
            self.scan_limits()?,
            &policy,
            full,
            |request| {
                use airk_ingest::observations::{ReuseBasis, VerifiedReuse};
                let profile = request.profile;
                let source_hash = request.source_hash.to_string();
                let same = previous.artifacts.get(request.path).filter(|old| {
                    old.content_hash == source_hash
                        && old.bytes == request.source_bytes
                        && old
                            .retained
                            .as_ref()
                            .and_then(|reference| reference.profile.as_ref())
                            .is_some_and(|old_profile| old_profile == profile)
                });
                let cached = prior_content
                    .get(&(
                        source_hash.clone(),
                        profile.identity()?,
                        request.source_bytes,
                    ))
                    .copied();
                let Some(old) = same.or(cached) else {
                    return Ok(None);
                };
                let old_root =
                    airk_store::roots::source_location(&previous.root, &previous.roots, &old.path)?;
                let current_root =
                    airk_store::roots::source_location(&previous.root, &roots, &old.path)?;
                if old_root.root != current_root.root {
                    return Ok(None);
                }
                let actual = old
                    .retained
                    .as_ref()
                    .and_then(|r| r.profile.as_ref())
                    .context(
                        "indexed extraction profile missing; rebuild with airk index --full",
                    )?;
                if actual != profile {
                    return Ok(None);
                }
                let basis = if old.path == request.path {
                    ReuseBasis::SamePath
                } else {
                    ReuseBasis::PriorContent
                };
                let mut rebound = old.clone();
                rebound.path = request.path.into();
                // Typed path-free binding reconstructs all destination syntax IDs.
                // Retained objects remain shared only by verified source/profile.
                self.store.load_retained(&rebound)?;
                Ok(Some(VerifiedReuse {
                    artifact: rebound,
                    basis,
                }))
            },
            |profile, artifact, bytes, fresh_payload| {
                if artifact
                    .retained
                    .as_ref()
                    .is_some_and(|reference| reference.profile.as_ref() == Some(profile))
                {
                    return Ok(artifact.clone());
                }
                let hydrated = self.store.load_retained(artifact)?;
                let payload = match fresh_payload {
                    Some(payload) => payload.clone(),
                    None => airk_ingest::continuity::prepare_native(&hydrated, bytes, profile)?,
                };
                let prepared = airk_core::binding::bind_payload(
                    &payload,
                    profile,
                    airk_core::binding::ArtifactBinding {
                        path: &hydrated.path,
                        source_hash: hydrated.content_hash.parse()?,
                        source_bytes: hydrated.bytes,
                    },
                )?;
                self.store.retain_payload_with_budget(
                    &prepared,
                    &payload,
                    profile,
                    Some(bytes),
                    Some(&reservation),
                )
            },
        )?;
        report.scan_extract_us =
            u64::try_from(scan_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        snapshot.persistent_indexes = true;
        snapshot.storage_format = 2;
        snapshot.embedding_model = desired_model;
        if (snapshot.embedding_model != previous.embedding_model
            || previous.storage_format != 2
            || configured_policy.pending_reindex
            || desired_pipeline != prior_pipeline
            || prior_selection_digest != Some(selection_digest))
            && snapshot.generation == previous.generation
        {
            snapshot.generation = snapshot
                .generation
                .checked_add(1)
                .context("generation overflow")?;
            report.generation = snapshot.generation;
        }
        let publish_started = std::time::Instant::now();
        if snapshot.generation != previous.generation
            || !self.store.directory().join("index.sqlite3").exists()
            || self.store.persistent_revision()?.is_none()
        {
            let (generation, embedding) = self.publish_v2_observed_metrics(
                writer,
                &snapshot,
                reservation,
                if full {
                    airk_core::changes::PublicationReason::FullReindex
                } else {
                    airk_core::changes::PublicationReason::Index
                },
                Some(&observation),
            )?;
            report.generation = generation;
            report.embedding_cache_hits = embedding.hits;
            report.embedding_cache_misses = embedding.misses;
            report.embedding_inferences = embedding.inferences;
            report.embedding_no_vector = embedding.no_vector;
            report.embedding_inputs = embedding.inputs;
            report.embedding_blank_inputs = embedding.blank_inputs;
            report.embedding_duplicate_inputs = embedding.duplicate_inputs;
            report.embedding_actual_forwards = embedding.actual_forwards;
            report.embedding_tokenized_inputs = embedding.tokenized_inputs;
            report.embedding_input_tokens = embedding.input_tokens;
            report.embedding_embedded_tokens = embedding.embedded_tokens;
            report.embedding_padding_tokens = embedding.padding_tokens;
            report.embedding_truncated_inputs = embedding.truncated_inputs;
            report.embedding_plan_compilations = embedding.plan_compilations;
            report.embedding_plan_compile_us = embedding.plan_compile_us;
            report.embedding_plan_cache_peak_bytes = embedding.plan_cache_peak_bytes;
            report.vector_eligible_artifacts = embedding.vector_eligible_artifacts;
            report.vector_excluded_artifacts = embedding.vector_excluded_artifacts;
            report.vector_reused_inputs = embedding.vector_reused_inputs;
            report.vector_graph_nodes = embedding.vector_graph_nodes;
            report.vector_registry_rows = embedding.vector_registry_rows;
            report.vector_registry_bytes = embedding.vector_registry_bytes;
            report.vector_units = embedding.vector_units;
            report.vector_body_bytes = embedding.vector_body_bytes;
            report.vector_planning_us = embedding.vector_planning_us;
            report.embedding_model_load_us = embedding.model_load_us;
            report.embedding_cache_lookup_us = embedding.cache_lookup_us;
            report.embedding_cache_store_us = embedding.cache_store_us;
            report.embedding_inference_us = embedding.inference_us;
            report.vector_graph_insert_us = embedding.graph_insert_us;
            report.embedding_workers = embedding.workers;
            report.embedding_batches = embedding.batches;
        } else {
            // Reuse must not report success for a corrupt retained component.
            self.pin_revision(
                self.persistent_revision()?
                    .context("active revision missing")?,
            )?;
            drop(reservation);
            self.after_publication(writer, &airk_store::quota::Quota::open(&self.data_root)?);
        }
        report.publish_us =
            u64::try_from(publish_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        Ok(report)
    }

    pub fn status(&self) -> Result<Status> {
        if let Some(revision) = self.persistent_revision()? {
            let (revision, index) = self.pin_revision(revision)?;
            return Ok(Status {
                retention: self.retention_observation(revision.token),
                publication: revision.token.map(|token| {
                    airk_core::evidence::RevisionEnvelope::new(revision.collection.clone(), token)
                }),
                source_roots: revision
                    .roots
                    .clone()
                    .into_iter()
                    .chain([(String::new(), revision.root.clone())])
                    .collect(),
                generation: revision.generation,
                collection: revision.collection,
                files: revision.files,
                chunks: revision.chunks,
                source_bytes: revision.source_bytes,
                extractor_version: revision.extractor_version,
                exact_fact_backend: "sqlite",
                lexical_backend: "tantivy",
                vector_backend: "microsoft-diskann-0.59.0/airk-file-provider-1",
                local_model_ready: index.model_sha256.is_some(),
                vectors_enabled: self.vectors_enabled()?,
                indexed_vectors: index.vectors,
                vector_selection_digest: Some(index.vector_selection_digest),
                vector_eligible_artifacts: index.vector_eligible_artifacts,
                vector_excluded_artifacts: index.vector_excluded_artifacts,
                semantic_producer: revision.semantic_producer,
                imported_facts: revision.imported_facts,
                imported_relationships: revision.imported_relationships,
            });
        }
        anyhow::bail!(
            "workspace is not indexed in the current format; rebuild with airk index --full"
        )
    }

    pub fn search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        let revision = self.persistent_revision()?.context(
            "workspace is not indexed in the current format; rebuild with airk index --full",
        )?;
        self.cached_persistent(revision)?.index.search(query)
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
            "vector search is disabled for this collection; enable it in the AIRK panel"
        );
        query.validate()?;
        let revision = self
            .persistent_revision()?
            .context("vector retrieval not ready: reindex with a provisioned local model")?;
        let cache = self.cached_persistent(revision)?;
        let index = &cache.index;
        if hybrid {
            index.hybrid_search(query)
        } else {
            index.semantic_search(query)
        }
    }

    fn cached_persistent(
        &self,
        revision: airk_store::PersistentRevision,
    ) -> Result<Arc<CachedIndex>> {
        let cached = self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("index cache poisoned"))?
            .clone();
        let identity = serde_json::to_string(&revision)?;
        if let Some(cached) = cached.as_ref().filter(|cached| cached.revision == identity) {
            return Ok(cached.clone());
        }
        let (revision, index) = self.pin_revision(revision)?;
        let built = Arc::new(CachedIndex {
            revision: serde_json::to_string(&revision)?,
            index: Box::new(index),
        });
        self.install_index(&cached, built.clone())?;
        Ok(built)
    }

    /// Installing a pin is the only locked step. A concurrent caller may keep
    /// its own verified pin without replacing a cache installed in the meantime.
    fn install_index(
        &self,
        observed: &Option<Arc<CachedIndex>>,
        built: Arc<CachedIndex>,
    ) -> Result<()> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("index cache poisoned"))?;
        let unchanged = match (cache.as_ref(), observed.as_ref()) {
            (None, None) => true,
            (Some(current), Some(previous)) => Arc::ptr_eq(current, previous),
            _ => false,
        };
        if unchanged {
            *cache = Some(built);
        }
        Ok(())
    }

    /// A writer may publish and prune between the metadata read and lease open.
    /// Retry only when the active identity changed; stable corruption fails closed.
    fn pin_revision(
        &self,
        mut revision: airk_store::PersistentRevision,
    ) -> Result<(airk_store::PersistentRevision, PersistentIndex)> {
        for _ in 0..3 {
            let opened = (|| {
                let token = revision
                    .token
                    .context("obsolete index format; rebuild with airk index --full")?;
                let receipt = self
                    .store
                    .receipt(&token)?
                    .context("committed receipt missing")?;
                PersistentIndex::open_v2(self.store.directory(), &revision.collection, &receipt)
            })();
            match opened {
                Ok(index) => return Ok((revision, index)),
                Err(error) => {
                    let Some(current) = self.persistent_revision()? else {
                        return Err(error);
                    };
                    if current.revision == revision.revision {
                        return Err(error);
                    }
                    revision = current;
                }
            }
        }
        anyhow::bail!(
            "active generation changed repeatedly while acquiring reader lease; retry request"
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

    fn persistent_revision(&self) -> Result<Option<airk_store::PersistentRevision>> {
        let revision = self.store.persistent_revision()?;
        if let Some(revision) = &revision {
            ensure!(
                revision.root == self.root_id() && revision.collection == self.collection,
                "index belongs to another root or collection"
            );
        }
        Ok(revision)
    }

    fn publication_reservation(&self) -> Result<airk_store::quota::Reservation> {
        let quota = airk_store::quota::Quota::open(&self.data_root)?;
        // The reservation includes temporary and retained bytes together. Each
        // provider charges before writes; exhausting it leaves the active pin intact.
        quota.reserve_available(&self.collection, u64::MAX)
    }

    pub fn read_file(
        &self,
        path: &str,
        start_line: usize,
        end_line: usize,
        max_bytes: usize,
    ) -> Result<SearchResponse> {
        let catalog = self.catalog()?;
        let artifact = catalog.load_artifact(path)?;
        let artifact = self.source_line_artifact(&catalog, artifact)?;
        Self::read_snapshot(
            Self::catalog_envelope(&catalog),
            &catalog.snapshot,
            &artifact,
            path,
            start_line,
            end_line,
            max_bytes,
        )
    }

    pub(crate) fn source_line_artifact(
        &self,
        catalog: &airk_store::blobs::Catalog,
        mut artifact: Artifact,
    ) -> Result<Artifact> {
        if matches!(
            artifact.language.as_str(),
            "html"
                | "vue"
                | "svelte"
                | "astro"
                | "mdx"
                | "notebook"
                | "razor"
                | "rmd"
                | "haskell-literate"
        ) && artifact.extraction_method == "utf8"
            && catalog
                .snapshot
                .artifacts
                .get(&artifact.path)
                .and_then(|entry| entry.retained.as_ref())
                .and_then(|r| r.profile.as_ref())
                .is_some_and(|profile| profile.contract_version == 2)
        {
            let entry = catalog
                .snapshot
                .artifacts
                .get(&artifact.path)
                .context("original source entry missing")?;
            let bytes = self.store.read_retained_source(entry)?;
            artifact.chunks = airk_ingest::chunk(std::str::from_utf8(&bytes)?);
        }
        Ok(artifact)
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
        let catalog = self.catalog()?;
        self.file_info_in(&catalog, path)
    }

    fn catalog_envelope(
        catalog: &airk_store::blobs::Catalog,
    ) -> Option<airk_core::evidence::RevisionEnvelope> {
        catalog.receipt.as_ref().map(|receipt| {
            airk_core::evidence::RevisionEnvelope::new(
                catalog.snapshot.collection.clone(),
                receipt.token,
            )
        })
    }

    fn file_info_in(&self, catalog: &airk_store::blobs::Catalog, path: &str) -> Result<FileInfo> {
        let snapshot = &catalog.snapshot;
        let inventory = catalog
            .inventory
            .as_ref()
            .context("obsolete index format; rebuild with airk index --full")?;
        let file = catalog.file(path)?.context("indexed file not found")?;
        let profile = inventory
            .profiles
            .get(&file.state.profile_id)
            .context("indexed file profile missing")?;
        let vector_selection =
            inventory
                .vector_selection
                .decision(path, profile, &file.language)?;
        Ok(FileInfo {
            publication: Self::catalog_envelope(catalog),
            source: airk_store::roots::source_location(&snapshot.root, &snapshot.roots, path)?,
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            path: path.into(),
            content_hash: file.state.source_hash.to_string(),
            source_bytes: file.source_bytes,
            language: file.language,
            extraction_method: file.extraction_method.clone(),
            parse_status: file.parse_status,
            chunks: file.chunks,
            facts: file.facts,
            indexed_lines: file.indexed_lines,
            line_basis: if file.extraction_method == "utf8" {
                "source"
            } else {
                "extracted-text; cite original location"
            },
            vector_selection,
            vector_selection_digest: inventory.vector_selection.digest()?,
        })
    }

    pub fn relationships(
        &self,
        query: &RelationshipQuery,
    ) -> Result<airk_core::RelationshipsResponse> {
        validate_budget(query.limit, query.max_bytes)?;
        ensure!(
            !query.symbol.is_empty()
                && query.symbol.len() <= 2048
                && query.path_prefix.len() <= 4096
                && query.kind.as_ref().is_none_or(|k| k.len() <= 128),
            "relationship filter invalid/too long"
        );
        ensure!(
            matches!(query.direction.as_str(), "incoming" | "outgoing"),
            "direction must be incoming or outgoing"
        );
        ensure!((1..=3).contains(&query.hops), "hops must be 1..3");
        self.require_current_index()?;
        self.store
            .relationships(&self.root_id(), &self.collection, query)
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
        let catalog = self.catalog()?;
        let mut snapshot = catalog.snapshot.clone();
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
        airk_ingest::semantic::import_with_source(
            &mut snapshot,
            binding.clone(),
            &bytes,
            catalog.as_ref(),
        )?;
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
        let catalog = self.catalog()?;
        let snapshot = &catalog.snapshot;
        let artifact = catalog.load_artifact(path)?;
        let artifact = self.source_line_artifact(&catalog, artifact)?;
        let indexed_lines = artifact.chunks.last().map_or(0, |chunk| chunk.end_line);
        ensure!(
            line > 0 && line <= indexed_lines,
            "line is outside indexed source"
        );
        Self::read_snapshot(
            Self::catalog_envelope(&catalog),
            snapshot,
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
        let catalog = self.catalog()?;
        let snapshot = &catalog.snapshot;
        let mut evidence = SearchResponse {
            publication: Self::catalog_envelope(&catalog),
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            hits: Vec::new(),
            truncated: false,
        };
        let mut remaining = max_bytes;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
        let mut read_bytes = 0usize;
        'files: for (path, entry) in &snapshot.artifacts {
            if !path.starts_with(path_prefix) {
                continue;
            }
            read_bytes = read_bytes.saturating_add(entry.extracted_bytes());
            if read_bytes > 16_777_216 || std::time::Instant::now() >= deadline {
                evidence.truncated = true;
                break;
            }
            let a = catalog.load_artifact(path)?;
            for chunk in &a.chunks {
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
        }
        Ok(ReferencesResponse {
            resolution: "textual-occurrences; includes comments, strings and definitions; no name resolution",
            evidence,
        })
    }
}
