// SPDX-License-Identifier: Apache-2.0
//! Scan preparation and publication for the single current index.
use super::{CurrentUpdater, SemanticUpdate, enrichment, progress};
use crate::Kernel;
use airk_core::{
    Artifact,
    graph_storage::{GraphHead, GraphScope},
    identity::{Incarnation, framed_digest},
    model_objects::ModelObjectSet,
};
use airk_retrieval::{
    model::{EmbeddingCacheContext, EmbeddingCacheStats, EmbeddingModel},
    vector::mutable,
};
use airk_store::{
    WriterLock,
    current::{
        ArtifactChange, CapturedInput, CurrentHead, CurrentScope,
        collection::CollectionConfiguration,
    },
    quota::Quota,
};
use anyhow::{Context, Result, ensure};
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
mod tests;

const MAX_INDEX_RESERVATION_BYTES: u64 = 32 << 30;
// Bounded grace for a foreign diagnostic page (128 files, 32 MiB by default,
// up to 512 MiB explicitly). Byte limits do not guarantee a disk-time bound;
// allow the existing explicit verification time budget, then fail visibly.
const RETAINED_REPAIR_ADMISSION_BUDGET: std::time::Duration = std::time::Duration::from_secs(120);

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

struct ProgressiveBase<'a> {
    kernel: &'a Kernel,
    quota: &'a Quota,
    failure_reservation: &'a airk_store::quota::Reservation,
    writer: &'a WriterLock,
    scope: &'a CurrentScope,
    configuration: &'a CollectionConfiguration,
    model: Option<&'a EmbeddingModel>,
    objects: Option<&'a ModelObjectSet>,
    selection: &'a airk_core::vector_selection::VectorSelectionPolicy,
    stats: &'a mut EmbeddingCacheStats,
    previous: &'a airk_core::Snapshot,
    snapshot: airk_core::Snapshot,
    semantic_work: Option<enrichment::SemanticWork>,
    scan_work: progress::ScanWork,
    batch: std::collections::BTreeMap<String, ArtifactChange>,
    batch_bytes: usize,
    last_flush: std::time::Instant,
    flush_interval: std::time::Duration,
    last_pause_check: std::time::Instant,
    next_batch: usize,
    processed: u64,
    committed: u64,
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
    fn observe(&mut self, artifact: &Artifact) -> Result<()> {
        crate::service_execution::source_bytes(artifact.bytes as u64)?;
        if self.last_pause_check.elapsed() >= std::time::Duration::from_secs(2) {
            self.check_pause()?;
        }
        self.processed += 1;
        if !self.batch.is_empty() && self.last_flush.elapsed() >= self.flush_interval {
            self.flush()?;
        }
        let same = self
            .previous
            .artifacts
            .get(&artifact.path)
            .map(|old| same_artifact(old, artifact))
            .transpose()?
            .unwrap_or(false);
        if !same {
            // A large captured input must not make a pending publication grow
            // without bound. Charge its logical source size conservatively.
            if !self.batch.is_empty() && self.batch_bytes.saturating_add(artifact.bytes) > 64 << 20
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
            self.batch_bytes = self.batch_bytes.saturating_add(artifact.bytes);
            if self.batch.len() >= self.next_batch || self.batch_bytes >= 64 << 20 {
                self.flush()?;
            }
        } else {
            self.committed += 1;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if self.batch.is_empty() {
            return Ok(());
        }
        self.check_pause()?;
        let publication_started = std::time::Instant::now();
        // Each publication owns its temporary capacity. Completed transactions
        // must not consume the scan's long-lived capture allowance.
        let reservation = self
            .kernel
            .publication_reservation_with_limit(MAX_INDEX_RESERVATION_BYTES)?;
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
        let mut next_scan_work = self.scan_work.clone();
        next_scan_work.advanced(self.processed, self.committed + self.batch.len() as u64)?;
        let mut resources = vec![
            next_scan_work.resource_change(&self.kernel.store, &reservation)?,
            airk_store::current::resources::ResourceChange {
                key: "analysis".into(),
                reference: None,
            },
        ];
        resources.push(
            selected
                .as_ref()
                .map(|work| work.resource_change(&self.kernel.store, &reservation))
                .transpose()?
                .unwrap_or(airk_store::current::resources::ResourceChange {
                    key: "semantic-work".into(),
                    reference: None,
                }),
        );
        let semantic = self
            .model
            .zip(self.objects)
            .map(|(model, objects)| SemanticUpdate {
                model,
                objects: objects.clone(),
                selection: self.selection,
                defer: true,
                cache: EmbeddingCacheContext {
                    cache: None,
                    bypass: false,
                    stats: self.stats,
                    max_workers: 1,
                    working_memory_bytes: 128 << 20,
                },
            });
        let changes = std::mem::take(&mut self.batch)
            .into_values()
            .collect::<Vec<_>>();
        let result = CurrentUpdater {
            store: &self.kernel.store,
            quota: self.quota,
            scope: self.scope,
            writer: self.writer,
            reservation: &reservation,
            configuration: Some(self.configuration),
        }
        .apply_scanned(&changes, semantic, &resources, &Default::default())
        .context("publish progressive base batch")?;
        drop(reservation);
        self.committed += changes.len() as u64;
        self.batch_bytes = 0;
        self.scan_work = next_scan_work;
        self.semantic_work = selected;
        self.current.changed_artifacts = self
            .current
            .changed_artifacts
            .saturating_add(result.lexical.changed.changed_artifacts);
        self.current.lexical_documents = self
            .current
            .lexical_documents
            .saturating_add(result.lexical.changed.added_documents);
        self.current.graph_records = self
            .current
            .graph_records
            .saturating_add(result.graph.sealed_records);
        self.current.graph_read_bytes = self
            .current
            .graph_read_bytes
            .saturating_add(result.graph.read_bytes);
        self.current.graph_base_record_reads = self
            .current
            .graph_base_record_reads
            .saturating_add(result.graph.base_record_reads);
        self.current.graph_proof_cache_hits = self
            .current
            .graph_proof_cache_hits
            .saturating_add(result.graph.base_proof_cache_hits);
        self.current.graph_proof_cache_evictions = self
            .current
            .graph_proof_cache_evictions
            .saturating_add(result.graph.base_proof_cache_evictions);
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
        // Let discovery continue without synchronous neural inference. The durable
        // semantic queue is drained by the scheduler after the source scan.
        // Amortize expensive publications while keeping the initial batch small
        // and later batches bounded by count, bytes and scan time.
        self.flush_interval = publication_interval(publication_started.elapsed());
        self.next_batch = self.next_batch.saturating_mul(2).min(4096);
        self.last_flush = std::time::Instant::now();
        Ok(())
    }

    fn fail(&mut self, error: &anyhow::Error) -> Result<()> {
        self.scan_work.advanced(self.processed, self.committed)?;
        self.scan_work.fail(error);
        let reservation = self.failure_reservation;
        CurrentUpdater {
            store: &self.kernel.store,
            quota: self.quota,
            scope: self.scope,
            writer: self.writer,
            reservation,
            configuration: None,
        }
        .apply_with_resources(
            &[],
            None,
            &[self
                .scan_work
                .resource_change(&self.kernel.store, reservation)?],
        )?;
        Ok(())
    }
}

fn record_scan_failure(
    progressive: Option<&mut ProgressiveBase<'_>>,
    error: anyhow::Error,
) -> anyhow::Error {
    if let Some(progressive) = progressive
        && let Err(recording) = progressive.fail(&error)
    {
        // Context preserves the original typed cause (especially ScanPaused).
        return error.context(format!(
            "could not publish scan failure status: {recording:#}"
        ));
    }
    error
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeBootstrap {
    head: CurrentHead,
    manifest: airk_store::current::lexical::LexicalManifest,
    configuration: CollectionConfiguration,
}

impl Kernel {
    pub(crate) fn recover_runtime_bootstrap(&self) -> Result<()> {
        let Some(bytes) = self.store.runtime_bootstrap()? else {
            return Ok(());
        };
        let recipe: RuntimeBootstrap = serde_json::from_slice(&bytes)?;
        ensure!(
            serde_json::to_vec(&recipe)? == bytes
                && recipe.head.scope.root == self.root_id()
                && recipe.head.scope.collection == self.collection
                && recipe.head.sequence == 0,
            "foreign runtime bootstrap"
        );
        recipe.configuration.validate()?;
        let lock = self.store.lock()?;
        let quota = self.collection_quota()?;
        let mut current = self
            .store
            .current_write(&lock, &quota, &recipe.head.scope, 16 << 20)?;
        if current.head().is_ok_and(|h| h.sequence > 0) {
            self.store.finish_runtime_bootstrap(&lock)?;
            return Ok(());
        }
        let token = recipe.head.token()?;
        current.initialize(recipe.head)?;
        current.install_initial_lexical_manifest(&token, &recipe.manifest.canonical_bytes()?)?;
        current.initialize_collection(&token, &recipe.configuration)?;
        self.store.finish_runtime_bootstrap(&lock)
    }

    pub(crate) fn initialize_current(&self) -> Result<crate::Status> {
        self.recover_runtime_bootstrap()?;
        if self.current_scope()?.is_some() {
            return self.status();
        }
        let writer = self.store.lock()?;
        let quota = self.collection_quota()?;
        let reservation = self.publication_reservation()?;
        let policy = self.rebuild_extraction_policy()?.policy;
        let objects = self.configured_model_objects(false)?;
        if let Some(objects) = &objects {
            self.retain_configured_model(objects, &reservation)?;
        }
        let model = objects
            .as_ref()
            .map(|objects| self.open_configured_model(objects))
            .transpose()?;
        let configuration = CollectionConfiguration {
            roots: self.configured_roots()?,
            extractor_version: airk_core::EXTRACTOR_VERSION.into(),
            extraction_policy_digest: crate::extraction::policy_digest(&policy)?,
            index_detail: policy.detail.unwrap_or_default(),
            vector_selection: self.vector_selection()?,
            model: objects,
        };
        self.bootstrap_current(
            &writer,
            &quota,
            &configuration,
            model.as_ref().map_or(1, EmbeddingModel::dimensions),
            &reservation,
        )?;
        drop(writer);
        self.status()
    }

    fn bootstrap_current(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        configuration: &CollectionConfiguration,
        dimensions: usize,
        reservation: &airk_store::quota::Reservation,
    ) -> Result<CurrentScope> {
        let scope = CurrentScope {
            root: self.root_id(),
            collection: self.collection.clone(),
            incarnation: self
                .store
                .service_incarnation()?
                .map_or_else(incarnation, Ok)?,
        };
        let graph_scope = GraphScope {
            root: scope.root.clone(),
            collection: scope.collection.clone(),
            incarnation: scope.incarnation,
            graph_instance: incarnation()?,
            specification: mutable::specification(
                dimensions,
                configuration
                    .model
                    .as_ref()
                    .map(|m| m.pipeline_digest)
                    .unwrap_or_else(|| framed_digest(b"current-lexical-only/1", &[])),
            )?,
            record_bytes: mutable::record_bytes(dimensions)?,
        };
        let manifest = airk_store::current::lexical::LexicalManifest::empty()?;
        let head = CurrentHead {
            version: 2,
            scope: scope.clone(),
            sequence: 0,
            artifact_root: airk_store::current::map::empty_root(&scope)?,
            lexical_sequence: 0,
            lexical_commit: manifest.digest()?,
            graph_head: GraphHead::empty(graph_scope)?,
        };
        let recipe = serde_json::to_vec(&RuntimeBootstrap {
            head: head.clone(),
            manifest: manifest.clone(),
            configuration: configuration.clone(),
        })?;
        reservation.charge(recipe.len() as u64)?;
        self.store.write_runtime_bootstrap(writer, &recipe)?;
        let mut current = self.store.current_write(writer, quota, &scope, 16 << 20)?;
        let token = head.token()?;
        current.initialize(head)?;
        current.install_initial_lexical_manifest(&token, &manifest.canonical_bytes()?)?;
        current.initialize_collection(&token, configuration)?;
        self.store.finish_runtime_bootstrap(writer)?;
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
        let mut repaired_retained_artifacts = 0;
        match self.index_current_once(
            full,
            defer_completion,
            &mut repaired_retained_artifacts,
            paths,
        ) {
            Ok(report) => Ok(report),
            Err(error)
                if error
                    .downcast_ref::<airk_store::blobs::MissingRetainedObject>()
                    .is_some() =>
            {
                // The failed attempt has released its capture guard, writer and
                // reservations. Never replace captured inputs in a durable journal
                // with live source bytes, even if the live path still exists.
                match self.retained_repair_can_rescan() {
                    Ok(true) => {}
                    Ok(false) => return Err(error.context(
                        "automatic retained-object repair requires a ready index with no pending journal; captured journal inputs must be recovered separately"
                    )),
                    Err(admission) if admission.is::<crate::scheduler::ScanPaused>() => {
                        return Err(admission.context(format!(
                            "automatic retained-object repair paused before a fresh scan; original failure: {error:#}"
                        )));
                    }
                    Err(admission) => return Err(error.context(format!(
                        "automatic retained-object repair could not verify an idle index: {admission:#}"
                    ))),
                }
                let mut report = self.index_current_once(full, defer_completion, &mut repaired_retained_artifacts, paths).context(
                    "automatic retained-object repair failed after one fresh scan; indexing stopped without further retries; run AIRK: Diagnose & Repair to inspect the remaining failure"
                )?;
                report.retained_repair_retries = 1;
                Ok(report)
            }
            Err(error) => Err(error),
        }
    }

    fn retained_repair_can_rescan(&self) -> Result<bool> {
        self.retained_repair_can_rescan_with_budget(RETAINED_REPAIR_ADMISSION_BUDGET)
    }

    fn retained_repair_can_rescan_with_budget(&self, budget: std::time::Duration) -> Result<bool> {
        let started = std::time::Instant::now();
        let mut delay = std::time::Duration::from_millis(25);
        loop {
            if self.service_collection.is_none()
                && airk_store::scheduler::Queue::new(&self.data_root)
                    .status(&self.collection, &self.root_id())?
                    .is_some_and(|job| job.paused && !job.manual)
            {
                return Err(crate::scheduler::ScanPaused.into());
            }
            match self.retained_repair_admission() {
                Err(error)
                    if error
                        .downcast_ref::<crate::CurrentGateContention>()
                        .is_some()
                        && started.elapsed() < budget =>
                {
                    // Release every handle before waiting. A diagnostic reader
                    // may finish between attempts; journal/I/O errors never retry.
                    // Back off repeated opens with at most 250 ms of sleeping
                    // before the next Pause check and admission attempt.
                    std::thread::sleep(budget.saturating_sub(started.elapsed()).min(delay));
                    delay = (delay * 2).min(std::time::Duration::from_millis(250));
                }
                outcome => return outcome,
            }
        }
    }

    fn retained_repair_admission(&self) -> Result<bool> {
        let writer = self.store.lock()?;
        let Some(scope) = self.current_scope()? else {
            return Ok(false);
        };
        let quota = self.collection_quota()?;
        let capacity = self.store.current_database_capacity(&scope)?;
        let current = self
            .store
            .current_write(&writer, &quota, &scope, capacity)?;
        Ok(
            current.phase()? == airk_store::current::TransactionPhase::Ready
                && current.pending()?.is_none(),
        )
    }

    /// Discard obsolete derived index state, including retained copies, before model setup.
    /// Current-format collections are unchanged; explicit full indexing follows.
    pub fn prepare_full_rebuild(&self) -> Result<bool> {
        let writer = self.store.lock()?;
        self.store.rebuild_obsolete_index(
            &writer,
            &self.collection_quota()?,
            &self.root_id(),
            &self.collection,
        )
    }

    fn index_current_once(
        &self,
        full: bool,
        defer_completion: bool,
        repaired_retained_artifacts: &mut usize,
        paths: Option<&std::collections::BTreeSet<String>>,
    ) -> Result<airk_ingest::IndexReport> {
        use crate::diagnostics::IndexFailureStage as Stage;
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
        self.recover_runtime_bootstrap()
            .context(Stage("bootstrap"))?;
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
        // Recovery takes its own writer lock, before this scan owns one.
        if let Some(scope) = self.current_scope().context(Stage("current_scope"))? {
            super::recover_current(&self.store, &self.collection_quota()?, &scope)
                .context(Stage("recover_current"))?;
        }
        let writer = self.store.lock().context(Stage("writer_admission"))?;
        let quota = self.collection_quota().context(Stage("quota_open"))?;
        ensure!(
            !self.root.starts_with(quota.root()),
            "index storage must not contain sources"
        );
        // Intermediate publications must not reclaim the scan's next retained
        // input or superseded objects still needed by prior-content reuse.
        let captured_objects = writer.capture_objects();
        let mut previous = self
            .load_optional()
            .context(Stage("read_snapshot"))?
            .unwrap_or_else(|| self.empty_snapshot());
        let configured = self
            .rebuild_extraction_policy()
            .context(Stage("extraction_settings"))?;
        let policy = configured.policy;
        let compression = self
            .store
            .compression_mode(&self.root_id())
            .context(Stage("compression_settings"))?;
        let roots = self.configured_roots().context(Stage("source_settings"))?;
        let selection = self.vector_selection().context(Stage("vector_settings"))?;
        // Keep a small independent allowance alive before capture/publication
        // can exhaust quota. Failure reporting must not require a fresh grant.
        let failure_reservation = self
            .publication_reservation_with_limit(2 << 20)
            .context(Stage("failure_reservation"))?;
        // Do not reserve all remaining capacity: durable SQL growth/replay owns
        // an independent grant while preparation is still alive.
        let available = quota.status().context(Stage("quota_inventory"))?;
        let root_free = available
            .root_limit_bytes
            .saturating_sub(available.used_bytes)
            .saturating_sub(available.reserved_bytes);
        let collection_free = available
            .collections
            .get(&self.collection)
            .map_or(root_free, |c| {
                c.limit_bytes
                    .saturating_sub(c.used_bytes)
                    .saturating_sub(c.reserved_bytes)
            });
        let reservation = quota
            .reserve_available(
                &self.collection,
                (root_free.min(collection_free) / 2).min(MAX_INDEX_RESERVATION_BYTES),
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
        let configuration = CollectionConfiguration {
            roots: roots.clone(),
            extractor_version: airk_core::EXTRACTOR_VERSION.into(),
            extraction_policy_digest: crate::extraction::policy_digest(&policy)?,
            index_detail: policy.detail.unwrap_or_default(),
            vector_selection: selection.clone(),
            model: objects.clone(),
        };
        let existing = self.current_scope()?;
        let (applied, prior_semantic_work, prior_scan_work) = if let Some(scope) = &existing {
            let read = self
                .store
                .current_read(scope)
                .context(Stage("read_prior_work"))?;
            (
                read.collection_state()
                    .context(Stage("read_configuration"))?,
                enrichment::read_work(&read, &self.store).context(Stage("read_semantic_work"))?,
                progress::read_work(&read, &self.store).context(Stage("read_scan_work"))?,
            )
        } else {
            (None, None, None)
        };
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
        // A newly initialized (or format-rebuilt) empty index needs the same
        // bounded first publications as a collection with no current store.
        // A full scan of an empty base cannot mix old and rebuilt evidence.
        // Populated full/configuration rebuilds retain their atomic boundary.
        let progressive_enabled = defer_completion
            && paths.is_none()
            && (previous.artifacts.is_empty() || (!full && !configuration_changed));
        let publish_scan_start = progressive_enabled
            && previous.artifacts.is_empty()
            && prior_scan_work.as_ref().is_none_or(|work| !work.complete());
        let mut stats = EmbeddingCacheStats {
            model_load_us,
            ..Default::default()
        };
        let progressive_scope = if progressive_enabled {
            Some(match &existing {
                Some(scope) => scope.clone(),
                None => self
                    .bootstrap_current(
                        &writer,
                        &quota,
                        &configuration,
                        model.as_ref().map_or(1, EmbeddingModel::dimensions),
                        &reservation,
                    )
                    .context("bootstrap progressive current")?,
            })
        } else {
            None
        };
        let mut progressive = if let Some(scope) = &progressive_scope {
            let scan_work = progress::ScanWork::start()?;
            if publish_scan_start {
                let publication =
                    self.publication_reservation_with_limit(MAX_INDEX_RESERVATION_BYTES)?;
                CurrentUpdater {
                    store: &self.store,
                    quota: &quota,
                    scope,
                    writer: &writer,
                    reservation: &publication,
                    configuration: None,
                }
                .apply_with_resources(
                    &[],
                    None,
                    &[scan_work.resource_change(&self.store, &publication)?],
                )
                .context("publish progressive scan start")?;
            }
            Some(ProgressiveBase {
                kernel: self,
                quota: &quota,
                failure_reservation: &failure_reservation,
                writer: &writer,
                scope,
                configuration: &configuration,
                model: model.as_ref(),
                objects: objects.as_ref(),
                selection: &selection,
                stats: &mut stats,
                previous: &previous,
                snapshot: previous.clone(),
                semantic_work: prior_semantic_work.clone(),
                scan_work,
                batch: Default::default(),
                batch_bytes: 0,
                last_flush: std::time::Instant::now(),
                flush_interval: std::time::Duration::from_secs(2),
                last_pause_check: std::time::Instant::now(),
                next_batch: 32,
                processed: 0,
                committed: 0,
                current: Default::default(),
                first_ready_us: None,
                started: index_started,
            })
        } else {
            None
        };
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
        let scan_started = std::time::Instant::now();
        let scan_result = airk_ingest::scan_payloads_with_execution_paths(
            &self.root,
            &self.data_root,
            &previous,
            &roots,
            self.scan_limits().context(Stage("scan_settings"))?,
            &policy,
            full,
            execution.execution,
            paths,
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
                if let Err(error) = self.store.check_retained_present(old) {
                    if error
                        .downcast_ref::<airk_store::blobs::MissingRetainedObject>()
                        .is_some()
                    {
                        // The scanner already captured and hashed the live source
                        // before lookup. Reextract those bytes under its selected
                        // profile to restore missing derived objects. Permission
                        // failures still stop indexing; reads that consume retained
                        // bytes and journal recovery authenticate those bytes.
                        *repaired_retained_artifacts += 1;
                        return Ok(None);
                    }
                    return Err(error);
                }
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
            |profile, artifact, bytes, payload| {
                crate::service_execution::progress_capture();
                if payload.is_some() {
                    crate::service_execution::progress_extracted();
                }
                crate::service_execution::progress_phase(
                    airk_core::service_progress::WorkPhase::Extracting,
                );
                let retained = if artifact
                    .retained
                    .as_ref()
                    .is_some_and(|r| r.profile.as_ref() == Some(profile))
                {
                    artifact.clone()
                } else {
                    self.store.retain_payload_encoded(
                        artifact,
                        payload.context("fresh extraction payload missing")?,
                        profile,
                        Some(bytes),
                        Some(&reservation),
                        compression,
                    )?
                };
                #[cfg(test)]
                tests::after_capture(&self.store, &retained)?;
                if let Some(progressive) = progressive.as_mut() {
                    progressive.observe(&retained)?;
                }
                Ok(retained)
            },
        );
        let (snapshot, mut report, observation) = match scan_result {
            Ok(value) => value,
            Err(error) => {
                return Err(
                    record_scan_failure(progressive.as_mut(), error).context(Stage("scan_sources"))
                );
            }
        };
        report.repaired_retained_artifacts = *repaired_retained_artifacts;
        report.path_scoped = paths.is_some();
        if let Some(progressive) = progressive.as_mut()
            && let Err(error) = progressive.check_pause()
        {
            return Err(record_scan_failure(Some(progressive), error));
        }
        let progressive_first_ready_us = progressive.as_ref().and_then(|p| p.first_ready_us);
        let progressive_current = progressive.as_ref().map(|p| p.current.clone());
        let progressive_work = progressive.as_ref().and_then(|p| p.semantic_work.clone());
        let mut final_scan_work = progressive.as_ref().map(|p| p.scan_work.clone());
        drop(progressive);
        if let Some(work) = final_scan_work.as_mut() {
            work.finish(snapshot.artifacts.len() as u64)?;
        }
        report.extraction_workers_requested = execution.requested;
        report.execution = Some(execution_lease.plan.clone());
        report.scan_extract_us =
            u64::try_from(scan_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        // An unavailable root is not a complete inventory of zero files. Preserve
        // committed evidence instead of constructing a root-wide deletion batch.
        self.require_service_sources()?;
        let committed_previous = if progressive_enabled
            && progressive_current
                .as_ref()
                .is_some_and(|work| work.changed_artifacts > 0)
        {
            self.load_optional()?
                .context("progressive current state missing")?
        } else {
            previous.clone()
        };
        let mut changes = std::collections::BTreeMap::new();
        for (path, artifact) in &snapshot.artifacts {
            let same = committed_previous
                .artifacts
                .get(path)
                .map(|old| same_artifact(old, artifact))
                .transpose()?
                .unwrap_or(false);
            if ((full || configuration_changed) && !progressive_enabled) || !same {
                changes.insert(
                    path.clone(),
                    ArtifactChange::Upsert(Box::new(CapturedInput::new(artifact.clone()))),
                );
            }
        }
        for path in committed_previous
            .artifacts
            .keys()
            .filter(|p| !snapshot.artifacts.contains_key(*p))
        {
            changes.insert(path.clone(), ArtifactChange::Delete(path.clone()));
        }
        if changes.is_empty()
            && progressive_current
                .as_ref()
                .is_none_or(|work| work.changed_artifacts == 0)
            && existing.is_some()
            && prior_scan_work.as_ref().is_none_or(|work| work.complete())
            && !publish_scan_start
        {
            final_scan_work = None;
        }
        if paths.is_none()
            && final_scan_work.is_none()
            && (!changes.is_empty()
                || prior_scan_work
                    .as_ref()
                    .is_some_and(|work| !work.complete()))
        {
            let mut work = progress::ScanWork::start()?;
            work.finish(snapshot.artifacts.len() as u64)?;
            final_scan_work = Some(work);
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
                enrichment::prepare_work(
                    &snapshot,
                    &changes,
                    progressive_work.as_ref().or(prior_semantic_work.as_ref()),
                    &selection,
                    objects.pipeline_digest,
                    full,
                )
            })
            .transpose()?;
        let moves = if observation.complete {
            compatible_moves(&self.store, &previous, &snapshot, &observation)?
        } else {
            Default::default()
        };
        // Every remaining input is now bound to the final publication. Its
        // normal cleanup can reclaim unused captures and superseded objects.
        drop(captured_objects);
        let scope = match progressive_scope.or(existing) {
            Some(scope) => scope,
            None => self
                .bootstrap_current(
                    &writer,
                    &quota,
                    &configuration,
                    model.as_ref().map_or(1, EmbeddingModel::dimensions),
                    &reservation,
                )
                .context("bootstrap final current")?,
        };
        let started = std::time::Instant::now();
        let analysis_expired = previous.analysis.as_ref().is_some_and(|manifest| {
            airk_ingest::semantic::validate_build_inputs(&self.root, manifest).is_err()
        });
        let retry_failed_semantic = prior_semantic_work
            .as_ref()
            .is_some_and(|work| work.failure.is_some());
        if !changes.is_empty()
            || configuration_changed
            || analysis_expired
            || retry_failed_semantic
            || progressive_current
                .as_ref()
                .is_some_and(|work| work.changed_artifacts > 0)
            || final_scan_work.is_some()
        {
            let semantic_resource = semantic_work
                .as_ref()
                .map(|work| work.resource_change(&self.store, &reservation))
                .transpose()?;
            let semantic = model
                .as_ref()
                .zip(objects)
                .map(|(model, objects)| -> Result<_> {
                    Ok(SemanticUpdate {
                        model,
                        objects,
                        selection: &selection,
                        defer: true,
                        cache: EmbeddingCacheContext {
                            cache: None,
                            // Empty full rebuilds already published compatible
                            // batches. Replacing their graph here would discard
                            // those bindings and require rewriting every path.
                            bypass: full && !progressive_enabled,
                            stats: &mut stats,
                            max_workers: airk_retrieval::embedding_batch::worker_limit(
                                std::env::var("AIRK_INDEX_WORKERS").ok().as_deref(),
                                std::thread::available_parallelism().map_or(1, usize::from),
                            )?,
                            working_memory_bytes: (crate::limits::machine::memory_bytes()
                                .unwrap_or(2 << 30)
                                / 8)
                            .clamp(128 << 20, 1 << 30)
                                as usize,
                        },
                    })
                })
                .transpose()?;
            let mut resources = vec![
                airk_store::current::resources::ResourceChange {
                    key: "analysis".into(),
                    reference: None,
                },
                semantic_resource.unwrap_or(airk_store::current::resources::ResourceChange {
                    key: "semantic-work".into(),
                    reference: None,
                }),
            ];
            if let Some(work) = &final_scan_work {
                resources.push(work.resource_change(&self.store, &reservation)?);
            }
            let result = CurrentUpdater {
                store: &self.store,
                quota: &quota,
                scope: &scope,
                writer: &writer,
                reservation: &reservation,
                configuration: Some(&configuration),
            }
            .apply_scanned(
                &changes.into_values().collect::<Vec<_>>(),
                semantic,
                &resources,
                &moves,
            )
            .context("publish final scanned current")?;
            report.generation = result.commit.token.sequence;
            report.maintenance_error = result.commit.maintenance_error;
            let mut work = progressive_current.unwrap_or_default();
            work.changed_artifacts = work
                .changed_artifacts
                .saturating_add(result.lexical.changed.changed_artifacts);
            work.lexical_documents = work
                .lexical_documents
                .saturating_add(result.lexical.changed.added_documents);
            work.graph_records = work
                .graph_records
                .saturating_add(result.graph.sealed_records);
            work.graph_read_bytes = work
                .graph_read_bytes
                .saturating_add(result.graph.read_bytes);
            work.graph_base_record_reads = work
                .graph_base_record_reads
                .saturating_add(result.graph.base_record_reads);
            work.graph_proof_cache_hits = work
                .graph_proof_cache_hits
                .saturating_add(result.graph.base_proof_cache_hits);
            work.graph_proof_cache_evictions = work
                .graph_proof_cache_evictions
                .saturating_add(result.graph.base_proof_cache_evictions);
            report.current = Some(work);
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
            let read = self.store.current_read(&scope)?;
            super::open_selected_lexical(&self.store, &read)?;
            report.generation = read.head().sequence;
            report.current = Some(progressive_current.unwrap_or_default());
        }
        report.first_ready_us = progressive_first_ready_us.or_else(|| {
            Some(u64::try_from(index_started.elapsed().as_micros()).unwrap_or(u64::MAX))
        });
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
            || !defer_completion
        {
            report.fully_ready_us =
                Some(u64::try_from(index_started.elapsed().as_micros()).unwrap_or(u64::MAX));
        }
        report.publish_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        record_embedding_stats(&mut report, &stats);
        let read = self.store.current_read(&scope)?;
        report.semantic_readiness = Some(enrichment::readiness(&read, &self.store)?);
        report.discovery_readiness = Some(progress::readiness(&read, &self.store)?);
        report.indexed_vectors = Some(read.head().graph_head.live_nodes);
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

fn compatible_moves(
    store: &airk_store::FileStore,
    previous: &airk_core::Snapshot,
    next: &airk_core::Snapshot,
    observation: &airk_ingest::observations::ScanObservation,
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
        let artifact = store.load_retained(&next.artifacts[to])?;
        let mut words = BTreeSet::new();
        for chunk in &artifact.chunks {
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
        let boilerplate = artifact.chunks.iter().all(|chunk| {
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
