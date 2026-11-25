// SPDX-License-Identifier: Apache-2.0
//! Scan preparation and publication for the single current index.
//!
//! The base pass writes into the plain index (`airk_store::index`): every
//! batch is one `CurrentUpdater` publication, one SQLite transaction. The scan
//! keeps each fresh extraction in memory with its batch; nothing is written
//! per file. A file whose stored record is unchanged needs no work.
use super::{CurrentUpdater, enrichment, progress, update::PlainOptions, update::PlainPayloads};
use crate::Kernel;
use airk_core::{Artifact, identity::Incarnation};
use airk_retrieval::model::{EmbeddingCacheStats, EmbeddingModel};
use airk_store::{
    WriterLock,
    current::{
        ArtifactChange, CapturedInput, CurrentScope,
        collection::CollectionConfiguration,
        plain_writer::{CapturedPayload, PlainRead},
        resources::ResourceChange,
    },
    quota::Quota,
};
use anyhow::{Context, Result, ensure};

const MAX_INDEX_RESERVATION_BYTES: u64 = 32 << 30;
/// A pending batch is flushed before its captured source bytes pass this.
const MAX_BATCH_BYTES: usize = 64 << 20;

fn incarnation() -> Result<Incarnation> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("allocate current identity: {e}"))?;
    Ok(Incarnation::from_bytes(bytes))
}

// Spend up to four times the previous publication duration preparing the next
// batch, within a 2-30 second scan interval. Count/byte caps can publish sooner;
// this is not a wall-clock publication deadline.
fn publication_interval(elapsed: std::time::Duration) -> std::time::Duration {
    elapsed.saturating_mul(4).clamp(
        std::time::Duration::from_secs(2),
        std::time::Duration::from_secs(30),
    )
}

fn same_artifact(old: &Artifact, new: &Artifact) -> Result<bool> {
    if old.retained.is_some()
        && new.retained.is_some()
        && old.chunks.is_empty()
        && new.chunks.is_empty()
        && old.facts.is_empty()
        && new.facts.is_empty()
    {
        return Ok(old.path == new.path
            && old.content_hash == new.content_hash
            && old.bytes == new.bytes
            && old.language == new.language
            && old.extraction_method == new.extraction_method
            && old.parse_status == new.parse_status
            && old.retained == new.retained);
    }
    Ok(serde_json::to_vec(old)? == serde_json::to_vec(new)?)
}

/// The changed files of the scan, published in bounded batches as they come.
struct ProgressiveBase<'a> {
    kernel: &'a Kernel,
    quota: &'a Quota,
    writer: &'a WriterLock,
    scope: &'a CurrentScope,
    configuration: &'a CollectionConfiguration,
    objects: Option<&'a airk_core::model_objects::ModelObjectSet>,
    dimensions: Option<usize>,
    selection: &'a airk_core::vector_selection::VectorSelectionPolicy,
    previous: &'a airk_core::Snapshot,
    /// Previous files plus every file published or batched by this scan.
    snapshot: airk_core::Snapshot,
    semantic_work: Option<enrichment::SemanticWork>,
    scan_work: progress::ScanWork,
    batch: std::collections::BTreeMap<String, ArtifactChange>,
    payloads: PlainPayloads,
    batch_bytes: usize,
    /// Publish every observed file (a full rebuild or a configuration change).
    force_all: bool,
    /// Path-scoped scans publish once at the end.
    intermediate: bool,
    last_flush: std::time::Instant,
    flush_interval: std::time::Duration,
    last_pause_check: std::time::Instant,
    next_batch: usize,
    processed: u64,
    committed: u64,
    published_any: bool,
    current: airk_ingest::CurrentIndexWork,
    first_ready_us: Option<u64>,
    started: std::time::Instant,
}

impl ProgressiveBase<'_> {
    fn check_pause(&mut self) -> Result<()> {
        crate::service_execution::checkpoint()?;
        if crate::service_execution::installed() {
            return Ok(());
        }
        self.last_pause_check = std::time::Instant::now();
        if airk_store::scheduler::Queue::new(&self.kernel.data_root)
            .status(&self.kernel.collection, &self.kernel.root_id())?
            .is_some_and(|job| job.paused && !job.manual)
        {
            return Err(crate::scheduler::ScanPaused.into());
        }
        Ok(())
    }

    fn observe(&mut self, artifact: &Artifact, captured: Option<CapturedPayload>) -> Result<()> {
        crate::service_execution::source_bytes(artifact.bytes as u64)?;
        if self.last_pause_check.elapsed() >= std::time::Duration::from_secs(2) {
            self.check_pause()?;
        }
        self.processed += 1;
        if self.intermediate
            && !self.batch.is_empty()
            && self.last_flush.elapsed() >= self.flush_interval
        {
            self.flush()?;
        }
        let same = !self.force_all
            && self
                .previous
                .artifacts
                .get(&artifact.path)
                .map(|old| same_artifact(old, artifact))
                .transpose()?
                .unwrap_or(false);
        if same {
            self.committed += 1;
            return Ok(());
        }
        // A large captured input must not make a pending publication grow
        // without bound. Charge its logical source size conservatively.
        if self.intermediate
            && !self.batch.is_empty()
            && self.batch_bytes.saturating_add(artifact.bytes) > MAX_BATCH_BYTES
        {
            self.flush()?;
        }
        self.snapshot
            .artifacts
            .insert(artifact.path.clone(), artifact.clone());
        self.batch.insert(
            artifact.path.clone(),
            ArtifactChange::Upsert(Box::new(CapturedInput::new(artifact.clone()))),
        );
        match captured {
            Some(captured) => {
                self.payloads.insert(artifact.path.clone(), captured);
            }
            None => {
                self.payloads.remove(&artifact.path);
            }
        }
        self.batch_bytes = self.batch_bytes.saturating_add(artifact.bytes);
        if self.intermediate
            && (self.batch.len() >= self.next_batch || self.batch_bytes >= MAX_BATCH_BYTES)
        {
            self.flush()?;
        }
        Ok(())
    }

    fn resources(&self, scan_work: &progress::ScanWork) -> Result<Vec<ResourceChange>> {
        let reservation = self
            .kernel
            .publication_reservation_with_limit(MAX_INDEX_RESERVATION_BYTES)?;
        let store = &self.kernel.store;
        let semantic = self
            .objects
            .map(|objects| {
                enrichment::prepare_work(
                    &self.snapshot,
                    &self.batch,
                    self.semantic_work.as_ref(),
                    self.selection,
                    objects.pipeline_digest,
                    false,
                )
            })
            .transpose()?;
        let resources = vec![
            scan_work.resource_change(store, self.writer, &reservation)?,
            ResourceChange {
                key: "analysis".into(),
                reference: None,
            },
            match &semantic {
                Some(work) => work.resource_change(store, self.writer, &reservation)?,
                None => ResourceChange {
                    key: "semantic-work".into(),
                    reference: None,
                },
            },
        ];
        Ok(resources)
    }

    fn publish(
        &mut self,
        changes: Vec<ArtifactChange>,
        resources: &[ResourceChange],
        moves: Option<&std::collections::BTreeMap<String, String>>,
        recount: bool,
    ) -> Result<super::CurrentUpdateResult> {
        let reservation = self
            .kernel
            .publication_reservation_with_limit(MAX_INDEX_RESERVATION_BYTES)?;
        let result = CurrentUpdater {
            store: &self.kernel.store,
            quota: self.quota,
            scope: self.scope,
            writer: self.writer,
            reservation: &reservation,
            configuration: Some(self.configuration),
        }
        .apply_scanned(
            &changes,
            resources,
            PlainOptions {
                payloads: std::mem::take(&mut self.payloads),
                moves,
                recount,
                dimensions: self.dimensions,
            },
        )?;
        self.published_any = true;
        self.current.changed_artifacts = self
            .current
            .changed_artifacts
            .saturating_add(result.lexical.changed.changed_artifacts);
        self.current.lexical_documents = self
            .current
            .lexical_documents
            .saturating_add(result.lexical.changed.added_documents);
        Ok(result)
    }

    fn flush(&mut self) -> Result<()> {
        if self.batch.is_empty() {
            return Ok(());
        }
        let _stage = airk_core::stage_timing::scope("index.flush");
        self.check_pause()?;
        let publication_started = std::time::Instant::now();
        let mut next_scan_work = self.scan_work.clone();
        next_scan_work.advanced(self.processed, self.committed + self.batch.len() as u64)?;
        let resources = self.resources(&next_scan_work)?;
        let selected = self
            .objects
            .map(|objects| {
                enrichment::prepare_work(
                    &self.snapshot,
                    &self.batch,
                    self.semantic_work.as_ref(),
                    self.selection,
                    objects.pipeline_digest,
                    false,
                )
            })
            .transpose()?;
        let changes = std::mem::take(&mut self.batch)
            .into_values()
            .collect::<Vec<_>>();
        let count = changes.len() as u64;
        let result = self
            .publish(changes, &resources, None, false)
            .context("publish progressive base batch")?;
        self.committed += count;
        self.batch_bytes = 0;
        self.scan_work = next_scan_work;
        self.semantic_work = selected;
        self.first_ready_us.get_or_insert_with(|| {
            u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX)
        });
        self.kernel
            .emit_progress(airk_core::readiness::IndexProgress {
                collection: String::new(),
                phase: "base_committed",
                revision: Some(super::runtime::revision(result.commit.token).to_string()),
                processed_artifacts: self.processed,
                committed_artifacts: self.committed,
                total_artifacts: None,
                selected_artifacts: self.semantic_work.as_ref().map(|w| w.selected_artifacts),
                completed_artifacts: self.semantic_work.as_ref().map(|w| w.completed_artifacts),
                completed_units: self.semantic_work.as_ref().map(|w| w.completed_units),
            });
        // Amortize publications while keeping the initial batch small and
        // later batches bounded by count, bytes and scan time.
        self.flush_interval = publication_interval(publication_started.elapsed());
        self.next_batch = self.next_batch.saturating_mul(2).min(4096);
        self.last_flush = std::time::Instant::now();
        Ok(())
    }

    fn fail(&mut self, error: &anyhow::Error) -> Result<()> {
        if !self.intermediate {
            return Ok(());
        }
        self.scan_work.advanced(self.processed, self.committed)?;
        self.scan_work.fail(error);
        let reservation = self.kernel.publication_reservation_with_limit(2 << 20)?;
        CurrentUpdater {
            store: &self.kernel.store,
            quota: self.quota,
            scope: self.scope,
            writer: self.writer,
            reservation: &reservation,
            configuration: None,
        }
        .apply_with_resources(
            &[],
            None,
            &[self
                .scan_work
                .resource_change(&self.kernel.store, self.writer, &reservation)?],
        )?;
        Ok(())
    }
}

fn record_scan_failure(
    progressive: &mut ProgressiveBase<'_>,
    error: anyhow::Error,
) -> anyhow::Error {
    if let Err(recording) = progressive.fail(&error) {
        // Context preserves the original typed cause (especially ScanPaused).
        return error.context(format!(
            "could not publish scan failure status: {recording:#}"
        ));
    }
    error
}

impl Kernel {
    /// The old store's bootstrap recipe has no plain counterpart; a plain
    /// index is created in one step when the first scan starts.
    pub(crate) fn recover_runtime_bootstrap(&self) -> Result<()> {
        Ok(())
    }

    fn current_configuration(
        &self,
        policy: &airk_ingest::adapters::profile::ExtractionPolicy,
        objects: Option<airk_core::model_objects::ModelObjectSet>,
    ) -> Result<CollectionConfiguration> {
        Ok(CollectionConfiguration {
            retained_profile: policy.retained_profile,
            source_policy_digest: self.source_policy_digest()?,
            roots: self.configured_roots()?,
            extractor_version: airk_core::EXTRACTOR_VERSION.into(),
            extraction_policy_digest: crate::extraction::policy_digest(policy)?,
            index_detail: policy.legacy_detail(),
            vector_selection: self.vector_selection()?,
            model: objects,
        })
    }

    pub(crate) fn initialize_current(&self) -> Result<crate::Status> {
        let writer = self.store.lock()?;
        self.store
            .discard_old_format_index(&writer, &self.root_id(), &self.collection)?;
        if self.current_scope()?.is_some() && self.store.plain_index_present()? {
            drop(writer);
            return self.status();
        }
        let reservation = self.publication_reservation()?;
        let policy = self.rebuild_extraction_policy()?.policy;
        let objects = self.configured_model_objects(false)?;
        if let Some(objects) = &objects {
            self.retain_configured_model(objects, &reservation)?;
        }
        let configuration = self.current_configuration(&policy, objects)?;
        self.bootstrap_current(&writer, &configuration)?;
        drop(reservation);
        drop(writer);
        self.status()
    }

    /// The owner header and an empty plain index for this collection.
    fn bootstrap_current(
        &self,
        writer: &WriterLock,
        configuration: &CollectionConfiguration,
    ) -> Result<CurrentScope> {
        let scope = match self.current_scope()? {
            Some(scope) => scope,
            None => CurrentScope {
                root: self.root_id(),
                collection: self.collection.clone(),
                incarnation: self
                    .store
                    .service_incarnation()?
                    .map_or_else(incarnation, Ok)?,
            },
        };
        if !self.store.plain_index_present()? {
            self.store.plain_create(writer, &scope, configuration)?;
        }
        Ok(scope)
    }

    pub(crate) fn index_current(
        &self,
        full: bool,
        defer_completion: bool,
    ) -> Result<airk_ingest::IndexReport> {
        self.index_current_selected(full, defer_completion, None)
    }

    /// Refresh saved logical paths; untouched bindings retain their indexed revision.
    pub fn index_paths(&self, paths: &[String]) -> Result<airk_ingest::IndexReport> {
        self.index_current_paths(paths, true)
    }

    pub(crate) fn index_current_paths(
        &self,
        paths: &[String],
        defer_completion: bool,
    ) -> Result<airk_ingest::IndexReport> {
        ensure!(
            !paths.is_empty()
                && paths.len() <= 1024
                && paths.iter().map(String::len).sum::<usize>() <= 1 << 20,
            "path hints exceed admission limits"
        );
        let paths = paths
            .iter()
            .map(|path| {
                let path = path.replace('\\', "/");
                airk_ingest::adapters::profile::validate_relative_path(&path)?;
                Ok(path)
            })
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        self.index_current_selected(false, defer_completion, Some(&paths))
    }

    fn index_current_selected(
        &self,
        full: bool,
        defer_completion: bool,
        paths: Option<&std::collections::BTreeSet<String>>,
    ) -> Result<airk_ingest::IndexReport> {
        self.index_current_once(full, defer_completion, paths)
    }

    /// Discard an old-format collection (the old current store and no plain
    /// index) before model setup. A plain index is unchanged: a full scan
    /// re-extracts and replaces every file in it.
    pub fn prepare_full_rebuild(&self) -> Result<bool> {
        self.source_policy_for_work()?;
        let writer = self.store.lock()?;
        self.store.rebuild_obsolete_index(
            &writer,
            &self.collection_quota()?,
            &self.root_id(),
            &self.collection,
        )
    }

    /// The compact scan projection of the plain index: every stored record.
    fn plain_scan_projection(&self, read: &PlainRead) -> Result<airk_core::Snapshot> {
        let mut snapshot = self.empty_snapshot();
        snapshot.generation = read.sequence();
        snapshot.storage_format = 120;
        if let Some(state) = read.collection_state()? {
            snapshot.roots = state.configuration.roots;
            snapshot.extractor_version = state.configuration.extractor_version;
            snapshot.embedding_model = state
                .configuration
                .model
                .map(|m| m.pipeline_digest.to_string());
        }
        snapshot.artifacts = read.artifacts()?;
        ensure!(
            snapshot.artifacts.len() <= 100_000,
            "current inventory admission exceeded"
        );
        if let Some(bytes) = read.resource("analysis")? {
            let analysis: airk_store::current::analysis::AnalysisEvidence =
                serde_json::from_slice(&bytes)?;
            snapshot.analysis = Some(analysis.manifest);
            snapshot.imported_facts = analysis.facts;
            snapshot.imported_relationships = analysis.relationships;
        }
        Ok(snapshot)
    }

    fn index_current_once(
        &self,
        full: bool,
        defer_completion: bool,
        paths: Option<&std::collections::BTreeSet<String>>,
    ) -> Result<airk_ingest::IndexReport> {
        use crate::diagnostics::IndexFailureStage as Stage;
        // An old-format collection is never read: discard it before anything
        // (source policy, settings) consults the applied state.
        if self.store.plain_old_format()? {
            let writer = self.store.lock().context(Stage("writer_admission"))?;
            self.store
                .discard_old_format_index(&writer, &self.root_id(), &self.collection)
                .context(Stage("rebuild_admission"))?;
        }
        self.source_policy_for_work()?;
        self.require_service_sources()
            .context(Stage("source_access"))?;
        let execution_lease =
            crate::execution::acquire(&self.data_root).context(Stage("execution_admission"))?;
        if full {
            self.prepare_full_rebuild()
                .context(Stage("rebuild_admission"))?;
        }
        let index_started = std::time::Instant::now();
        self.emit_progress(airk_core::readiness::IndexProgress {
            collection: String::new(),
            phase: "scan_started",
            ..Default::default()
        });
        let mut execution = if self.service_collection.is_some() {
            crate::limits::execution_from_grant(&execution_lease.plan)
                .context(Stage("extraction_admission"))?
        } else {
            crate::limits::configured_extraction_execution()
                .context(Stage("extraction_admission"))?
        };
        execution.requested = execution_lease.plan.requested_extraction_workers;
        execution.execution.workers = execution_lease.plan.extraction_workers;
        execution.execution.max_in_flight_source_bytes = execution.execution.workers
            * airk_ingest::ExtractionExecution::MAX_PARALLEL_SOURCE_BYTES;
        execution
            .execution
            .validate()
            .context(Stage("extraction_admission"))?;
        let writer = self.store.lock().context(Stage("writer_admission"))?;
        self.store
            .discard_old_format_index(&writer, &self.root_id(), &self.collection)
            .context(Stage("rebuild_admission"))?;
        let quota = self.collection_quota().context(Stage("quota_open"))?;
        ensure!(
            !self.root.starts_with(quota.root()),
            "index storage must not contain sources"
        );
        let existing = self.current_scope().context(Stage("current_scope"))?;
        let read = if existing.is_some() && self.store.plain_index_present()? {
            Some(self.store.plain_read().context(Stage("read_snapshot"))?)
        } else {
            None
        };
        if let (Some(read), Some(scope)) = (&read, &existing) {
            ensure!(
                read.scope() == scope,
                "plain index belongs to another owner"
            );
        }
        let load_stage = airk_core::stage_timing::scope("index.load_previous");
        let mut previous = read
            .as_ref()
            .map(|read| self.plain_scan_projection(read))
            .transpose()
            .context(Stage("read_snapshot"))?
            .unwrap_or_else(|| self.empty_snapshot());
        drop(load_stage);
        let configured = self
            .rebuild_extraction_policy()
            .context(Stage("extraction_settings"))?;
        let policy = configured.policy;
        let roots = self.configured_roots().context(Stage("source_settings"))?;
        let selection = self.vector_selection().context(Stage("vector_settings"))?;
        let capture_free = self
            .reservation_headroom(&quota)
            .context(Stage("quota_inventory"))?;
        let reservation = quota
            .reserve_available(
                &self.collection,
                (capture_free / 2).min(MAX_INDEX_RESERVATION_BYTES),
            )
            .context(Stage("capture_reservation"))?;
        let model_started = std::time::Instant::now();
        let objects = self
            .configured_model_objects(false)
            .context(Stage("model_settings"))?;
        if let Some(objects) = &objects {
            self.retain_configured_model(objects, &reservation)
                .context(Stage("retain_model"))?;
        }
        let model = objects
            .as_ref()
            .map(|objects| self.open_configured_model(objects))
            .transpose()
            .context(Stage("open_model"))?;
        let model_load_us = u64::try_from(model_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let configuration = self.current_configuration(&policy, objects.clone())?;
        let (applied, prior_semantic_work, prior_scan_work) = match &read {
            Some(read) => (
                read.collection_state()
                    .context(Stage("read_configuration"))?,
                enrichment::read_plain_work(read).context(Stage("read_semantic_work"))?,
                progress::read_plain_work(read).context(Stage("read_scan_work"))?,
            ),
            None => (None, None, None),
        };
        drop(read);
        let configuration_changed = applied
            .as_ref()
            .map(|s| serde_json::to_vec(&s.configuration))
            .transpose()?
            != Some(serde_json::to_vec(&configuration)?);
        if full {
            previous.extractor_version.clear();
        }
        // A hint cannot establish a new inventory or apply global policy changes.
        // Directory and ignore/configuration hints conservatively reconcile all paths.
        let paths = paths.filter(|paths| {
            existing.is_some()
                && !configuration_changed
                && previous.extractor_version == airk_core::EXTRACTOR_VERSION
                && prior_scan_work.as_ref().is_none_or(|work| work.complete())
                && !paths.iter().any(|path| {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    matches!(
                        name,
                        ".gitignore" | ".airkignore" | ".gitmodules" | "compile_commands.json"
                    ) || policy
                        .declared_configurations()
                        .iter()
                        .any(|configuration| configuration == path)
                        || previous.artifacts.keys().any(|old| {
                            old.strip_prefix(path)
                                .is_some_and(|tail| tail.starts_with('/'))
                        })
                        || airk_store::roots::source_location(&previous.root, &previous.roots, path)
                            .and_then(|location| {
                                Ok(std::fs::symlink_metadata(
                                    std::path::Path::new(&location.root)
                                        .join(&location.relative_path),
                                )?
                                .is_dir())
                            })
                            .unwrap_or(false)
                })
        });
        let scope = self
            .bootstrap_current(&writer, &configuration)
            .context("bootstrap current")?;
        // Every unscoped scan publishes in bounded batches; the plain index has
        // no atomic rebuild boundary. A full rebuild and a configuration change
        // replace every file batch by batch and recount at the end.
        let intermediate = paths.is_none();
        let force_all = full || (configuration_changed && applied.is_some());
        let publish_scan_start = intermediate
            && previous.artifacts.is_empty()
            && prior_scan_work.as_ref().is_none_or(|work| !work.complete());
        let mut stats = EmbeddingCacheStats {
            model_load_us,
            ..Default::default()
        };
        let mut scan_work = progress::ScanWork::start()?;
        if !intermediate && let Some(prior) = &prior_scan_work {
            scan_work = prior.clone();
        }
        let mut progressive = ProgressiveBase {
            kernel: self,
            quota: &quota,
            writer: &writer,
            scope: &scope,
            configuration: &configuration,
            objects: objects.as_ref(),
            dimensions: model.as_ref().map(EmbeddingModel::dimensions),
            selection: &selection,
            previous: &previous,
            snapshot: previous.clone(),
            semantic_work: prior_semantic_work.clone(),
            scan_work,
            batch: Default::default(),
            payloads: Default::default(),
            batch_bytes: 0,
            force_all,
            intermediate,
            last_flush: std::time::Instant::now(),
            flush_interval: std::time::Duration::from_secs(2),
            last_pause_check: std::time::Instant::now(),
            next_batch: 32,
            processed: 0,
            committed: 0,
            published_any: false,
            current: Default::default(),
            first_ready_us: None,
            started: index_started,
        };
        if publish_scan_start {
            let resources = [progressive.resources(&progressive.scan_work)?[0].clone()];
            progressive
                .publish(vec![], &resources, None, false)
                .context("publish progressive scan start")?;
        }
        let mut reusable = std::collections::BTreeMap::new();
        for artifact in previous.artifacts.values() {
            let profile = artifact
                .retained
                .as_ref()
                .and_then(|r| r.profile.as_ref())
                .context("prior extraction profile missing")
                .context(Stage("prepare_reuse"))?;
            reusable
                .entry((
                    artifact.content_hash.clone(),
                    artifact.bytes,
                    profile.identity()?,
                ))
                .or_insert(artifact);
        }
        let permanent_policy = self.source_policy_for_work()?;
        let scan_started = std::time::Instant::now();
        let scan_result = airk_ingest::scan_payloads_with_optional_control_snapshot(
            &self.root,
            &self.data_root,
            &previous,
            &roots,
            self.scan_limits().context(Stage("scan_settings"))?,
            &policy,
            full,
            execution.execution,
            paths,
            permanent_policy.as_deref(),
            None,
            |request| {
                let same = previous
                    .artifacts
                    .get(request.path)
                    .filter(|old| {
                        old.content_hash == request.source_hash.to_string()
                            && old.bytes == request.source_bytes
                            && old.retained.as_ref().and_then(|r| r.profile.as_ref())
                                == Some(request.profile)
                    })
                    .filter(|old| {
                        airk_store::roots::source_location(
                            &previous.root,
                            &previous.roots,
                            &old.path,
                        )
                        .is_ok_and(|l| l.root == request.canonical_root)
                    });
                let Some(old) = same.or_else(|| {
                    reusable
                        .get(&(
                            request.source_hash.to_string(),
                            request.source_bytes,
                            request.profile.identity().ok()?,
                        ))
                        .copied()
                }) else {
                    return Ok(None);
                };
                // A stored record's payload row exists by construction: the
                // index keeps every payload a file references.
                let mut artifact = old.clone();
                artifact.path = request.path.to_owned();
                Ok(Some(airk_ingest::observations::VerifiedReuse {
                    artifact,
                    basis: if same.is_some() {
                        airk_ingest::observations::ReuseBasis::SamePath
                    } else {
                        airk_ingest::observations::ReuseBasis::PriorContent
                    },
                }))
            },
            |profile, artifact, _bytes, payload| {
                crate::service_execution::progress_capture();
                if payload.is_some() {
                    crate::service_execution::progress_extracted();
                }
                crate::service_execution::progress_phase(
                    airk_core::service_progress::WorkPhase::Extracting,
                );
                let (compact, captured) = if artifact
                    .retained
                    .as_ref()
                    .is_some_and(|r| r.profile.as_ref() == Some(profile))
                {
                    (artifact.clone(), None)
                } else {
                    let (compact, captured) = CapturedPayload::capture(
                        artifact,
                        payload.context("fresh extraction payload missing")?,
                        profile,
                    )?;
                    (compact, Some(captured))
                };
                progressive.observe(&compact, captured)?;
                Ok(compact)
            },
        );
        let (snapshot, mut report, observation) = match scan_result {
            Ok(value) => value,
            Err(error) => {
                return Err(
                    record_scan_failure(&mut progressive, error).context(Stage("scan_sources"))
                );
            }
        };
        report.repaired_retained_artifacts = 0;
        report.path_scoped = paths.is_some();
        if let Err(error) = progressive.check_pause() {
            return Err(record_scan_failure(&mut progressive, error));
        }
        report.extraction_workers_requested = execution.requested;
        report.execution = Some(execution_lease.plan.clone());
        report.scan_extract_us =
            u64::try_from(scan_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        // An unavailable root is not a complete inventory of zero files. Preserve
        // committed evidence instead of constructing a root-wide deletion batch.
        self.require_service_sources()?;
        // The final batch: files still pending, deletions and moves.
        let changes_stage = airk_core::stage_timing::scope("index.final_changes");
        let mut changes = std::mem::take(&mut progressive.batch);
        for (path, artifact) in &snapshot.artifacts {
            let same = progressive
                .snapshot
                .artifacts
                .get(path)
                .map(|old| same_artifact(old, artifact))
                .transpose()?
                .unwrap_or(false);
            if !same {
                changes.insert(
                    path.clone(),
                    ArtifactChange::Upsert(Box::new(CapturedInput::new(artifact.clone()))),
                );
            }
        }
        for path in progressive
            .snapshot
            .artifacts
            .keys()
            .filter(|p| !snapshot.artifacts.contains_key(*p))
        {
            changes.insert(path.clone(), ArtifactChange::Delete(path.clone()));
        }
        let moves = if observation.complete {
            let _stage = airk_core::stage_timing::scope("index.moves");
            compatible_moves(
                &self.store,
                &previous,
                &snapshot,
                &observation,
                &progressive.payloads,
            )?
        } else {
            Default::default()
        };
        // A move target may already be published under a fresh identity; it
        // is published again with the moved-from file's identity.
        for to in moves.keys() {
            if let Some(artifact) = snapshot.artifacts.get(to) {
                changes.entry(to.clone()).or_insert_with(|| {
                    ArtifactChange::Upsert(Box::new(CapturedInput::new(artifact.clone())))
                });
            }
        }
        drop(changes_stage);
        let mut final_scan_work = intermediate.then(|| progressive.scan_work.clone());
        if let Some(work) = final_scan_work.as_mut() {
            work.finish(snapshot.artifacts.len() as u64)?;
        }
        if changes.is_empty()
            && !progressive.published_any
            && existing.is_some()
            && prior_scan_work.as_ref().is_none_or(|work| work.complete())
        {
            final_scan_work = None;
        }
        if paths.is_some()
            && !changes.is_empty()
            && let Some(prior) = &prior_scan_work
        {
            let mut work = prior.clone();
            work.adjust_complete_inventory(snapshot.artifacts.len() as u64)?;
            final_scan_work = Some(work);
        }
        let semantic_work = objects
            .as_ref()
            .map(|objects| {
                let _stage = airk_core::stage_timing::scope("index.prepare_work");
                enrichment::prepare_work(
                    &snapshot,
                    &changes,
                    progressive
                        .semantic_work
                        .as_ref()
                        .or(prior_semantic_work.as_ref()),
                    &selection,
                    objects.pipeline_digest,
                    full,
                )
            })
            .transpose()?;
        let started = std::time::Instant::now();
        let analysis_expired = previous.analysis.as_ref().is_some_and(|manifest| {
            airk_ingest::semantic::validate_build_inputs(&self.root, manifest).is_err()
        });
        let retry_failed_semantic = prior_semantic_work
            .as_ref()
            .is_some_and(|work| work.failure.is_some());
        let recount = force_all && progressive.published_any;
        if !changes.is_empty()
            || configuration_changed
            || analysis_expired
            || retry_failed_semantic
            || final_scan_work.is_some()
            || recount
        {
            let semantic_resource = semantic_work
                .as_ref()
                .map(|work| work.resource_change(&self.store, &writer, &reservation))
                .transpose()?;
            let mut resources = vec![
                ResourceChange {
                    key: "analysis".into(),
                    reference: None,
                },
                semantic_resource.unwrap_or(ResourceChange {
                    key: "semantic-work".into(),
                    reference: None,
                }),
            ];
            if let Some(work) = &final_scan_work {
                resources.push(work.resource_change(&self.store, &writer, &reservation)?);
            }
            let apply_stage = airk_core::stage_timing::scope("index.final_apply");
            let result = progressive
                .publish(
                    changes.into_values().collect(),
                    &resources,
                    Some(&moves),
                    recount,
                )
                .context("publish final scanned current")?;
            drop(apply_stage);
            report.generation = result.commit.token.sequence;
            let observed = if paths.is_some() {
                report.indexed + report.reused
            } else {
                snapshot.artifacts.len()
            } as u64;
            self.emit_progress(airk_core::readiness::IndexProgress {
                collection: String::new(),
                phase: if paths.is_some() {
                    "base_committed"
                } else {
                    "base_coverage_complete"
                },
                revision: Some(super::runtime::revision(result.commit.token).to_string()),
                processed_artifacts: observed,
                committed_artifacts: observed,
                total_artifacts: paths.is_none().then_some(snapshot.artifacts.len() as u64),
                selected_artifacts: semantic_work.as_ref().map(|w| w.selected_artifacts),
                completed_artifacts: semantic_work.as_ref().map(|w| w.completed_artifacts),
                completed_units: semantic_work.as_ref().map(|w| w.completed_units),
            });
        } else {
            report.generation = self.store.plain_read()?.sequence();
        }
        report.current = Some(progressive.current.clone());
        report.first_ready_us = progressive.first_ready_us.or_else(|| {
            Some(u64::try_from(index_started.elapsed().as_micros()).unwrap_or(u64::MAX))
        });
        drop(progressive);
        drop(writer);
        drop(reservation);
        if !defer_completion
            && let Some(model) = &model
            && let Some(generation) = self.drain_with_failure_record(
                model,
                &selection,
                &mut stats,
                usize::MAX,
                report.current.as_mut(),
                super::enrichment::DrainAdmission {
                    execution: &execution_lease,
                    deadline: None,
                },
            )?
        {
            report.generation = generation;
        }
        if semantic_work
            .as_ref()
            .is_none_or(|work| work.pending.is_empty())
        {
            report.fully_ready_us =
                Some(u64::try_from(index_started.elapsed().as_micros()).unwrap_or(u64::MAX));
        }
        report.publish_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        record_embedding_stats(&mut report, &stats);
        let read = self.store.plain_read()?;
        report.semantic_readiness = Some(enrichment::plain_readiness(&read, &self.store)?);
        report.discovery_readiness = Some(progress::plain_readiness(&read)?);
        report.indexed_vectors = Some(read.reader().vector_setup()?.map_or(0, |(_, _, rows)| rows));
        Ok(report)
    }
}

pub(super) fn record_embedding_stats(
    report: &mut airk_ingest::IndexReport,
    stats: &EmbeddingCacheStats,
) {
    report.vector_eligible_artifacts = stats.vector_eligible_artifacts;
    report.vector_excluded_artifacts = stats.vector_excluded_artifacts;
    report.vector_reused_inputs = stats.vector_reused_inputs;
    report.vector_graph_nodes = stats.vector_graph_nodes;
    report.vector_registry_rows = stats.vector_registry_rows;
    report.vector_registry_bytes = stats.vector_registry_bytes;
    report.embedding_cache_file_opens = stats.cache_file_opens;
    report.embedding_cache_read_bytes = stats.cache_read_bytes;
    report.embedding_cache_written_bytes = stats.cache_written_bytes;
    report.embedding_cache_record_verification_bytes = stats.cache_record_verification_bytes;
    report.embedding_cache_index_verification_bytes = stats.cache_index_verification_bytes;
    report.embedding_cache_full_pack_verification_bytes = stats.cache_full_pack_verification_bytes;
    report.embedding_cache_syncs = stats.cache_syncs;
    report.embedding_cache_index_hits = stats.cache_index_hits;
    report.embedding_cache_index_peak_bytes = stats.cache_index_peak_bytes;
    report.embedding_cache_packs_written = stats.cache_packs_written;
    report.embedding_cache_packs_reused = stats.cache_packs_reused;
    report.embedding_cache_compacted_records = stats.cache_compacted_records;
    report.embedding_cache_finalize_us = stats.cache_finalize_us;
    report.vector_units = stats.vector_units;
    report.vector_body_bytes = stats.vector_body_bytes;
    report.vector_planning_us = stats.vector_planning_us;
    report.embedding_inputs = stats.inputs;
    report.embedding_blank_inputs = stats.blank_inputs;
    report.embedding_duplicate_inputs = stats.duplicate_inputs;
    report.embedding_actual_forwards = stats.actual_forwards;
    report.embedding_tokenized_inputs = stats.tokenized_inputs;
    report.embedding_input_tokens = stats.input_tokens;
    report.embedding_embedded_tokens = stats.embedded_tokens;
    report.embedding_padding_tokens = stats.padding_tokens;
    report.embedding_truncated_inputs = stats.truncated_inputs;
    report.embedding_plan_compilations = stats.plan_compilations;
    report.embedding_plan_compile_us = stats.plan_compile_us;
    report.embedding_plan_cache_peak_bytes = stats.plan_cache_peak_bytes;
    report.embedding_cache_hits = stats.hits;
    report.embedding_cache_misses = stats.misses;
    report.embedding_inferences = stats.inferences;
    report.embedding_no_vector = stats.no_vector;
    report.embedding_cache_lookup_us = stats.cache_lookup_us;
    report.embedding_cache_store_us = stats.cache_store_us;
    report.embedding_inference_us = stats.inference_us;
    report.vector_graph_insert_us = stats.graph_insert_us;
    report.embedding_model_load_us = stats.model_load_us;
    report.embedding_workers = stats.workers;
    report.embedding_batches = stats.batches;
}

/// Unique content (path, profile, size) that disappeared from one path and
/// appeared at one new path in a complete scan keeps its file identity. The
/// destination's text comes from the batch, else from the stored payload.
fn compatible_moves(
    store: &airk_store::FileStore,
    previous: &airk_core::Snapshot,
    next: &airk_core::Snapshot,
    observation: &airk_ingest::observations::ScanObservation,
    payloads: &PlainPayloads,
) -> Result<std::collections::BTreeMap<String, String>> {
    use std::collections::{BTreeMap, BTreeSet};
    ensure!(observation.complete, "incomplete current scan");
    type Key = (String, airk_core::identity::Sha256Digest, usize);
    let groups = |snapshot: &airk_core::Snapshot| -> Result<BTreeMap<Key, Vec<String>>> {
        let mut groups = BTreeMap::<Key, Vec<String>>::new();
        for artifact in snapshot.artifacts.values() {
            let profile = artifact
                .retained
                .as_ref()
                .and_then(|r| r.profile.as_ref())
                .context("move profile missing")?;
            groups
                .entry((
                    artifact.content_hash.clone(),
                    profile.identity()?,
                    artifact.bytes,
                ))
                .or_default()
                .push(artifact.path.clone());
        }
        Ok(groups)
    };
    let old = groups(previous)?;
    let new = groups(next)?;
    let stable = |path: &str| match path.strip_prefix('@').and_then(|p| p.split_once('/')) {
        Some((alias, _))
            if previous.roots.contains_key(alias) || next.roots.contains_key(alias) =>
        {
            previous
                .roots
                .get(alias)
                .is_some_and(|root| next.roots.get(alias) == Some(root))
        }
        _ => previous.root == next.root,
    };
    let mut moves = BTreeMap::new();
    let mut read = None;
    for (identity, paths) in old {
        if paths.len() != 1 || identity.2 < 64 {
            continue;
        }
        let from = &paths[0];
        let Some(to) = new.get(&identity).filter(|p| p.len() == 1).map(|p| &p[0]) else {
            continue;
        };
        if next.artifacts.contains_key(from)
            || previous.artifacts.contains_key(to)
            || !stable(from)
            || !stable(to)
            || observation.disappeared.get(from)
                != Some(&airk_ingest::observations::DisappearanceCause::DeletedObserved)
        {
            continue;
        }
        let stored;
        let chunks = match payloads.get(to) {
            Some(captured) => &captured.artifact.chunks,
            None => {
                if read.is_none() {
                    read = Some(store.plain_read()?);
                }
                let read = read.as_ref().expect("opened above");
                stored = read.captured(&next.artifacts[to])?;
                &stored.artifact.chunks
            }
        };
        let mut words = BTreeSet::new();
        for chunk in chunks {
            for word in chunk
                .text
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() >= 2)
            {
                words.insert(word.to_lowercase());
                if words.len() >= 8 {
                    break;
                }
            }
            if words.len() >= 8 {
                break;
            }
        }
        let boilerplate = chunks.iter().all(|chunk| {
            let text = chunk.text.to_lowercase();
            text.trim().is_empty()
                || text.contains("all rights reserved")
                || text.contains("permission is hereby granted")
                || text.contains("spdx-license-identifier") && text.lines().count() <= 2
        });
        if words.len() >= 8 && !boilerplate {
            moves.insert(to.clone(), from.clone());
        }
    }
    Ok(moves)
}
