// SPDX-License-Identifier: Apache-2.0
//! Durable source-bound semantic work for the current index.
use super::{CurrentUpdater, SemanticUpdate};
use crate::Kernel;
use airk_core::readiness::{DefaultSearchPolicy, SemanticReadiness, SemanticState};
use airk_core::{Snapshot, identity::Sha256Digest, vector_selection::VectorSelectionPolicy};
use airk_retrieval::model::{EmbeddingCacheContext, EmbeddingCacheStats, EmbeddingModel};
use airk_store::{
    FileStore,
    current::{
        ArtifactChange, CapturedInput, CurrentRead, MAX_PENDING_INPUTS, resources::ResourceChange,
    },
    quota::Reservation,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &str = "semantic-work";
const POLICY_KEY: &str = "semantic-policy";

pub(super) struct DrainOutcome {
    generation: Option<u64>,
    maintenance_error: Option<String>,
}

/// Keep model loading and inference under the same admitted resource lease.
pub(super) struct DrainAdmission<'a> {
    pub(super) execution: &'a crate::execution::ExecutionLease,
    pub(super) deadline: Option<std::time::Instant>,
}

pub(super) fn now_unix_ms() -> Result<u64> {
    Ok(u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedSearchPolicy {
    version: u32,
    policy: DefaultSearchPolicy,
}

pub(super) fn read_policy(read: &CurrentRead, store: &FileStore) -> Result<DefaultSearchPolicy> {
    let Some(bytes) = read.resource(store, POLICY_KEY)? else {
        return Ok(DefaultSearchPolicy::default());
    };
    let state: SavedSearchPolicy = serde_json::from_slice(&bytes)?;
    ensure!(
        state.version == 1 && serde_json::to_vec(&state)? == bytes,
        "invalid saved semantic search policy"
    );
    Ok(state.policy)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingArtifact {
    pub path: String,
    pub source_hash: Sha256Digest,
    pub extraction_object: Sha256Digest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecomputeBoundary {
    pub graph: airk_core::identity::Incarnation,
    pub first_id: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SemanticWork {
    pub version: u32,
    pub selection_digest: Sha256Digest,
    pub pipeline_digest: Sha256Digest,
    pub selected_artifacts: u64,
    pub completed_artifacts: u64,
    pub selected_units: Option<u64>,
    pub completed_units: u64,
    pub all_units_countable: bool,
    pub force_recompute: bool,
    pub first_ready_unix_ms: u64,
    pub fully_ready_unix_ms: Option<u64>,
    pub pending: Vec<PendingArtifact>,
    pub failure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regeneration_job: Option<Sha256Digest>,
    /// Authenticated append-only graph boundary for this forced work chain.
    /// Earlier nodes and embedding-cache entries remain ineligible for reuse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recompute_boundary: Option<RecomputeBoundary>,
}

impl SemanticWork {
    pub fn readiness(&self, default_search_policy: DefaultSearchPolicy) -> SemanticReadiness {
        let state = if self.failure.is_some() {
            SemanticState::Failed
        } else if self.pending.is_empty() {
            SemanticState::Ready
        } else if self.completed_artifacts > 0 {
            SemanticState::Partial
        } else {
            SemanticState::Pending
        };
        SemanticReadiness {
            version: 1,
            state,
            default_search_policy,
            selected_artifacts: self.selected_artifacts,
            completed_artifacts: self.completed_artifacts,
            selected_units: self.selected_units,
            completed_units: self.completed_units,
            first_ready_unix_ms: Some(self.first_ready_unix_ms),
            fully_ready_unix_ms: self.fully_ready_unix_ms,
            selection_digest: Some(self.selection_digest),
            pipeline_digest: Some(self.pipeline_digest),
            failure: self.failure.clone(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "unsupported semantic work version");
        ensure!(
            self.pending.len() <= MAX_PENDING_INPUTS
                && self.selected_artifacts >= self.completed_artifacts
                && self.selected_artifacts - self.completed_artifacts == self.pending.len() as u64
                && self
                    .selected_units
                    .is_none_or(|n| n >= self.completed_units)
                && (self.pending.is_empty() == self.fully_ready_unix_ms.is_some())
                && self.first_ready_unix_ms > 0
                && self
                    .fully_ready_unix_ms
                    .is_none_or(|n| n >= self.first_ready_unix_ms)
                && self.failure.as_ref().is_none_or(|s| s.len() <= 4096),
            "invalid semantic work coverage"
        );
        ensure!(
            self.pending
                .windows(2)
                .all(|pair| pair[0].path < pair[1].path),
            "semantic work paths are unsorted or repeated"
        );
        for item in &self.pending {
            ensure!(
                !item.path.is_empty()
                    && item.path.len() <= 32_768
                    && !item.path.contains('\\')
                    && !item.path.contains('\0')
                    && item
                        .path
                        .split('/')
                        .all(|part| !part.is_empty() && part != "." && part != ".."),
                "invalid semantic work path"
            );
        }
        ensure!(
            self.recompute_boundary.as_ref().is_none_or(
                |boundary| self.force_recompute && boundary.first_id <= u64::from(u32::MAX) + 1
            ),
            "invalid forced semantic work boundary"
        );
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let work: Self = serde_json::from_slice(bytes)?;
        ensure!(
            work.canonical_bytes()? == bytes,
            "noncanonical semantic work"
        );
        Ok(work)
    }

    pub fn resource_change(
        &self,
        store: &FileStore,
        reservation: &Reservation,
    ) -> Result<ResourceChange> {
        let reference = store.retain_current_resource(&self.canonical_bytes()?, reservation)?;
        Ok(ResourceChange {
            key: KEY.into(),
            reference: Some(reference),
        })
    }
}

pub(super) fn read_work(read: &CurrentRead, store: &FileStore) -> Result<Option<SemanticWork>> {
    read.resource(store, KEY)?
        .as_deref()
        .map(SemanticWork::from_bytes)
        .transpose()
        .context("read current semantic work")
}

pub(crate) fn readiness(read: &CurrentRead, store: &FileStore) -> Result<SemanticReadiness> {
    let policy = read_policy(read, store)?;
    if let Some(work) = read_work(read, store)? {
        return Ok(work.readiness(policy));
    }
    let state = read
        .collection_state()?
        .context("current configuration missing")?;
    let c = &state.configuration;
    let enabled = c.model.is_some();
    Ok(SemanticReadiness {
        version: 1,
        state: if enabled {
            SemanticState::Ready
        } else {
            SemanticState::Disabled
        },
        default_search_policy: policy,
        selected_artifacts: if enabled {
            state.counts.vector_eligible
        } else {
            0
        },
        completed_artifacts: if enabled {
            state.counts.vector_eligible
        } else {
            0
        },
        selected_units: None,
        completed_units: 0,
        first_ready_unix_ms: None,
        fully_ready_unix_ms: None,
        selection_digest: Some(c.vector_selection.digest()?),
        pipeline_digest: c.model.as_ref().map(|m| m.pipeline_digest),
        failure: None,
    })
}

pub(super) fn prepare_work(
    snapshot: &Snapshot,
    changes: &BTreeMap<String, ArtifactChange>,
    previous: Option<&SemanticWork>,
    selection: &VectorSelectionPolicy,
    pipeline_digest: Sha256Digest,
    full: bool,
) -> Result<SemanticWork> {
    let selection_digest = selection.digest()?;
    let compatible = previous.filter(|prior| {
        prior.pipeline_digest == pipeline_digest && prior.selection_digest == selection_digest
    });
    let previous_pending = compatible
        .map(|prior| {
            prior
                .pending
                .iter()
                .map(|item| (item.path.as_str(), item))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut selected_artifacts = 0u64;
    let mut pending = Vec::new();
    for (path, artifact) in &snapshot.artifacts {
        let retained = artifact
            .retained
            .as_ref()
            .context("semantic work extraction missing")?;
        let profile = retained
            .profile
            .as_ref()
            .context("semantic work profile missing")?;
        if !selection
            .decision(path, profile, &artifact.language)?
            .eligible
        {
            continue;
        }
        selected_artifacts += 1;
        let current = PendingArtifact {
            path: path.clone(),
            source_hash: artifact.content_hash.parse()?,
            extraction_object: retained.object.parse()?,
        };
        if changes.contains_key(path)
            || previous_pending.get(path.as_str()).is_some_and(|prior| {
                prior.source_hash == current.source_hash
                    && prior.extraction_object == current.extraction_object
            })
        {
            pending.push(current);
        }
    }
    ensure!(
        pending.len() <= MAX_PENDING_INPUTS,
        "semantic work admission exceeded"
    );
    let unchanged = changes.is_empty() && compatible.is_some();
    let all_units_countable = pending.len() as u64 == selected_artifacts;
    let work = SemanticWork {
        version: 1,
        selection_digest,
        pipeline_digest,
        selected_artifacts,
        completed_artifacts: selected_artifacts - pending.len() as u64,
        selected_units: if unchanged {
            compatible.and_then(|p| p.selected_units)
        } else {
            None
        },
        completed_units: if unchanged {
            compatible.map_or(0, |p| p.completed_units)
        } else {
            0
        },
        all_units_countable: if unchanged {
            compatible.is_some_and(|p| p.all_units_countable)
        } else {
            all_units_countable
        },
        force_recompute: full
            || compatible.is_some_and(|p| p.force_recompute && !p.pending.is_empty()),
        first_ready_unix_ms: if unchanged {
            compatible.map_or(now_unix_ms()?, |p| p.first_ready_unix_ms)
        } else {
            now_unix_ms()?
        },
        fully_ready_unix_ms: if unchanged {
            compatible.and_then(|p| p.fully_ready_unix_ms)
        } else if pending.is_empty() {
            Some(now_unix_ms()?)
        } else {
            None
        },
        pending,
        failure: None,
        regeneration_job: compatible
            .filter(|prior| !full && prior.force_recompute && !prior.pending.is_empty())
            .and_then(|prior| prior.regeneration_job),
        recompute_boundary: compatible
            .filter(|prior| !full && prior.force_recompute && !prior.pending.is_empty())
            .and_then(|prior| prior.recompute_boundary.clone()),
    };
    work.validate()?;
    Ok(work)
}

impl Kernel {
    /// Queue every eligible retained extraction without scanning live sources or
    /// discarding readable vectors. Subsequent service turns replace bounded batches.
    pub fn begin_service_vector_regeneration(
        &self,
        job: Sha256Digest,
    ) -> Result<SemanticReadiness> {
        ensure!(
            self.is_service_collection(),
            "service project owner required"
        );
        crate::service_execution::checkpoint()?;
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        let quota = self.collection_quota()?;
        super::recover_current(&self.store, &quota, &scope)?;
        let writer = self.lock_current_writer()?;
        let read = self.store.current_read(&scope)?;
        if let Some(work) = read_work(&read, &self.store)?
            && work.regeneration_job == Some(job)
        {
            return Ok(work.readiness(read_policy(&read, &self.store)?));
        }
        let state = read
            .collection_state()?
            .context("current configuration missing")?;
        let configuration = state.configuration;
        let model = configuration
            .model
            .context("vector regeneration requires an indexed model")?;
        ensure!(
            self.vectors_enabled()?,
            "vectors are disabled for this project"
        );
        let mut pending = Vec::new();
        let mut bytes = 0usize;
        let mut after = None;
        loop {
            crate::service_execution::checkpoint()?;
            let page = read.lookup_paths(
                airk_store::current::postings::Lookup::Files,
                "",
                after.as_deref(),
                256,
            )?;
            for path in page.paths {
                let artifact = read.artifact(&path)?.context("current artifact missing")?;
                let retained = artifact
                    .retained
                    .as_ref()
                    .context("retained extraction missing")?;
                let profile = retained
                    .profile
                    .as_ref()
                    .context("retained profile missing")?;
                if !configuration
                    .vector_selection
                    .decision(&path, profile, &artifact.language)?
                    .eligible
                {
                    continue;
                }
                let item = PendingArtifact {
                    path,
                    source_hash: artifact.content_hash.parse()?,
                    extraction_object: retained.object.parse()?,
                };
                bytes = bytes
                    .checked_add(serde_json::to_vec(&item)?.len())
                    .context("semantic manifest overflow")?;
                ensure!(
                    pending.len() < MAX_PENDING_INPUTS && bytes <= 64 << 20,
                    "semantic regeneration admission exceeded"
                );
                pending.push(item);
            }
            after = page.next_after;
            if after.is_none() {
                break;
            }
        }
        pending.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        let now = now_unix_ms()?;
        let work = SemanticWork {
            version: 1,
            selection_digest: configuration.vector_selection.digest()?,
            pipeline_digest: model.pipeline_digest,
            selected_artifacts: pending.len() as u64,
            completed_artifacts: 0,
            selected_units: None,
            completed_units: 0,
            all_units_countable: true,
            force_recompute: true,
            first_ready_unix_ms: now,
            fully_ready_unix_ms: pending.is_empty().then_some(now),
            pending,
            failure: None,
            regeneration_job: Some(job),
            recompute_boundary: None,
        };
        let policy = read_policy(&read, &self.store)?;
        drop(read);
        let reservation = self.publication_reservation()?;
        CurrentUpdater {
            store: &self.store,
            quota: &quota,
            scope: &scope,
            writer: &writer,
            reservation: &reservation,
            configuration: None,
        }
        .apply_with_resources(
            &[],
            None,
            &[work.resource_change(&self.store, &reservation)?],
        )?;
        Ok(work.readiness(policy))
    }

    pub fn set_default_semantic_search_policy(
        &self,
        policy: DefaultSearchPolicy,
    ) -> Result<SemanticReadiness> {
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        super::recover_current(&self.store, &self.collection_quota()?, &scope)
            .context("recover current before semantic enrichment")?;
        let writer = self.lock_current_writer()?;
        let read = self.store.current_read(&scope)?;
        if read_policy(&read, &self.store)? == policy {
            return readiness(&read, &self.store);
        }
        drop(read);
        let quota = self.collection_quota()?;
        let reservation = self.publication_reservation()?;
        let bytes = serde_json::to_vec(&SavedSearchPolicy { version: 1, policy })?;
        let reference = self.store.retain_current_resource(&bytes, &reservation)?;
        CurrentUpdater {
            store: &self.store,
            quota: &quota,
            scope: &scope,
            writer: &writer,
            reservation: &reservation,
            configuration: None,
        }
        .apply_with_resources(
            &[],
            None,
            &[ResourceChange {
                key: POLICY_KEY.into(),
                reference: Some(reference),
            }],
        )?;
        drop(writer);
        let read = self.store.current_read(&scope)?;
        readiness(&read, &self.store)
    }

    fn record_semantic_failure(&self, error: &anyhow::Error) -> Result<()> {
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        let writer = self.lock_current_writer()?;
        let read = self.store.current_read(&scope)?;
        let Some(mut work) = read_work(&read, &self.store)? else {
            return Ok(());
        };
        if work.pending.is_empty() {
            return Ok(());
        }
        work.failure = Some(format!("{error:#}").chars().take(1024).collect());
        drop(read);
        let quota = self.collection_quota()?;
        let reservation = self.publication_reservation()?;
        CurrentUpdater {
            store: &self.store,
            quota: &quota,
            scope: &scope,
            writer: &writer,
            reservation: &reservation,
            configuration: None,
        }
        .apply_with_resources(
            &[],
            None,
            &[work.resource_change(&self.store, &reservation)?],
        )?;
        Ok(())
    }

    fn semantic_failure(&self, error: anyhow::Error) -> anyhow::Error {
        if Self::service_enrichment_contention(&error) {
            // A reader may release its gate before this handler runs. Temporary
            // publication exclusion must not become durable semantic failure.
            return error;
        }
        match self.record_semantic_failure(&error) {
            Ok(()) => error,
            // Preserve the original typed cause. In particular, a secondary
            // busy gate must not make corruption or permission errors retryable.
            Err(record) => error.context(format!(
                "record failed semantic enrichment also failed: {record:#}"
            )),
        }
    }

    pub(super) fn drain_with_failure_record(
        &self,
        model: &EmbeddingModel,
        selection: &VectorSelectionPolicy,
        stats: &mut EmbeddingCacheStats,
        max_batches: usize,
        current_work: Option<&mut airk_ingest::CurrentIndexWork>,
        admission: DrainAdmission<'_>,
    ) -> Result<Option<u64>> {
        match self.drain_current_enrichment(
            model,
            selection,
            stats,
            max_batches,
            current_work,
            admission,
        ) {
            Ok(result) => Ok(result.generation),
            Err(error) => Err(self.semantic_failure(error)),
        }
    }

    /// Resume retained semantic inputs without reading live workspace files.
    /// Each batch publishes its vectors and reduced pending set in one transaction.
    pub fn resume_current_enrichment(&self) -> Result<Option<u64>> {
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        super::recover_current(&self.store, &self.collection_quota()?, &scope)
            .context("recover current before semantic enrichment")?;
        let read = self.store.current_read(&scope)?;
        let Some(work) = read_work(&read, &self.store)? else {
            return Ok(None);
        };
        if work.pending.is_empty() {
            return Ok(Some(read.head().sequence));
        }
        let execution = match crate::execution::acquire(&self.data_root) {
            Ok(execution) => execution,
            Err(error) => {
                drop(read);
                return Err(self.semantic_failure(error));
            }
        };
        let configuration = read
            .collection_state()?
            .context("current configuration missing")?
            .configuration;
        let objects = configuration
            .model
            .as_ref()
            .context("pending semantic model missing")?;
        ensure!(
            work.pipeline_digest == objects.pipeline_digest
                && work.selection_digest == configuration.vector_selection.digest()?,
            "pending semantic work differs from current configuration"
        );
        let model = match self.open_configured_model(objects) {
            Ok(model) => model,
            Err(error) => {
                drop(read);
                return Err(self.semantic_failure(error));
            }
        };
        drop(read);
        self.drain_with_failure_record(
            &model,
            &configuration.vector_selection,
            &mut EmbeddingCacheStats::default(),
            usize::MAX,
            None,
            DrainAdmission {
                execution: &execution,
                deadline: None,
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn resume_current_enrichment_batches(
        &self,
        max_batches: usize,
    ) -> Result<Option<u64>> {
        self.resume_current_enrichment_batches_bounded(max_batches, None)
            .map(|(generation, _)| generation)
    }

    pub(crate) fn resume_current_enrichment_batches_bounded(
        &self,
        max_batches: usize,
        budget: Option<std::time::Duration>,
    ) -> Result<(Option<u64>, airk_ingest::IndexReport)> {
        ensure!(
            max_batches > 0 && max_batches <= 8,
            "semantic batch limit exceeded"
        );
        let started = std::time::Instant::now();
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        super::recover_current(&self.store, &self.collection_quota()?, &scope)?;
        let read = self.store.current_read(&scope)?;
        let Some(work) = read_work(&read, &self.store)? else {
            let report = self.semantic_report(&read, &EmbeddingCacheStats::default(), started)?;
            return Ok((None, report));
        };
        if work.pending.is_empty() {
            let report = self.semantic_report(&read, &EmbeddingCacheStats::default(), started)?;
            return Ok((Some(read.head().sequence), report));
        }
        let execution = match crate::execution::acquire(&self.data_root) {
            Ok(execution) => execution,
            Err(error) => {
                drop(read);
                return Err(self.semantic_failure(error));
            }
        };
        let configuration = read
            .collection_state()?
            .context("current configuration missing")?
            .configuration;
        let objects = configuration
            .model
            .as_ref()
            .context("pending semantic model missing")?;
        let model_started = std::time::Instant::now();
        let model = match self.open_configured_model(objects) {
            Ok(model) => model,
            Err(error) => {
                drop(read);
                return Err(self.semantic_failure(error));
            }
        };
        let mut stats = EmbeddingCacheStats {
            model_load_us: u64::try_from(model_started.elapsed().as_micros()).unwrap_or(u64::MAX),
            ..Default::default()
        };
        drop(read);
        let mut current_work = airk_ingest::CurrentIndexWork::default();
        let result = self
            .drain_current_enrichment(
                &model,
                &configuration.vector_selection,
                &mut stats,
                max_batches,
                Some(&mut current_work),
                DrainAdmission {
                    execution: &execution,
                    deadline: budget.map(|duration| started + duration),
                },
            )
            .map_err(|error| self.semantic_failure(error))?;
        let read = self.store.current_read(&scope)?;
        let mut report = self.semantic_report(&read, &stats, started)?;
        report.yield_to_source = self.service_collection.is_none()
            && airk_store::scheduler::Queue::new(&self.data_root)
                .status(&self.collection, &self.root_id())?
                .is_some_and(|job| {
                    job.requested_revision > job.completed_revision
                        && !job.blocked
                        && (!job.paused || job.manual)
                });
        report.current = Some(current_work);
        report.maintenance_error = result.maintenance_error;
        Ok((result.generation, report))
    }

    fn semantic_report(
        &self,
        read: &CurrentRead,
        stats: &EmbeddingCacheStats,
        started: std::time::Instant,
    ) -> Result<airk_ingest::IndexReport> {
        let elapsed = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let semantic = readiness(read, &self.store)?;
        let mut report = airk_ingest::IndexReport {
            collection: self.collection.clone(),
            generation: read.head().sequence,
            indexed_vectors: Some(read.head().graph_head.live_nodes),
            fully_ready_us: (semantic.state == SemanticState::Ready).then_some(elapsed),
            semantic_readiness: Some(semantic),
            publish_us: elapsed,
            ..Default::default()
        };
        report.discovery_readiness = Some(super::progress::readiness(read, &self.store)?);
        super::indexing::record_embedding_stats(&mut report, stats);
        Ok(report)
    }

    pub(super) fn drain_current_enrichment(
        &self,
        model: &EmbeddingModel,
        selection: &VectorSelectionPolicy,
        stats: &mut EmbeddingCacheStats,
        max_batches: usize,
        mut current_work: Option<&mut airk_ingest::CurrentIndexWork>,
        admission: DrainAdmission<'_>,
    ) -> Result<DrainOutcome> {
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        let execution = admission.execution;
        let deadline = admission.deadline;
        let writer = self.lock_current_writer()?;
        let quota = self.collection_quota()?;
        let captures = writer.capture_objects();
        let mut generation = None;
        for batch in 0..max_batches {
            crate::service_execution::checkpoint()?;
            if batch > 0 && deadline.is_some_and(|end| std::time::Instant::now() >= end) {
                break;
            }
            let job = if self.service_collection.is_some() {
                None
            } else {
                airk_store::scheduler::Queue::new(&self.data_root)
                    .status(&self.collection, &self.root_id())?
            };
            if max_batches <= 8
                && job.as_ref().is_some_and(|job| {
                    job.paused || job.blocked || job.requested_revision > job.completed_revision
                })
            {
                break;
            }
            let recent: std::collections::BTreeSet<_> = job
                .as_ref()
                .into_iter()
                .flat_map(|job| &job.recent_paths)
                .map(String::as_str)
                .collect();
            let read = self.store.current_read(&scope)?;
            let Some(mut work) = read_work(&read, &self.store)? else {
                break;
            };
            generation = Some(read.head().sequence);
            if work.pending.is_empty() {
                break;
            }
            let state = read
                .collection_state()?
                .context("current configuration missing")?;
            let objects = state
                .configuration
                .model
                .context("pending semantic model missing")?;
            ensure!(
                work.pipeline_digest == objects.pipeline_digest
                    && work.selection_digest == selection.digest()?
                    && state.configuration.vector_selection.digest()? == work.selection_digest,
                "pending semantic work differs from current configuration"
            );
            let batch_len = work.pending.len().min(32);
            // Keep the durable manifest path-sorted for canonical validation,
            // while selecting recent edits and useful repository context first.
            let mut selected: Vec<_> = work
                .pending
                .iter()
                .map(|pending| {
                    (
                        !recent.contains(pending.path.as_str()),
                        airk_ingest::priority::agent_path_priority(&pending.path),
                        pending,
                    )
                })
                .collect();
            if selected.len() > batch_len {
                selected.select_nth_unstable_by(batch_len, |left, right| {
                    (left.0, left.1, &left.2.path).cmp(&(right.0, right.1, &right.2.path))
                });
            }
            selected.truncate(batch_len);
            selected.sort_unstable_by(|left, right| {
                (left.0, left.1, &left.2.path).cmp(&(right.0, right.1, &right.2.path))
            });
            let mut selected: Vec<_> = selected
                .into_iter()
                .map(|(_, _, pending)| pending.clone())
                .collect();
            // Publication validates a canonical path order within each batch.
            selected.sort_unstable_by(|left, right| left.path.cmp(&right.path));
            self.emit_progress(airk_core::readiness::IndexProgress {
                collection: String::new(),
                phase: "enrichment_started",
                revision: None,
                processed_artifacts: 0,
                committed_artifacts: 0,
                selected_artifacts: Some(work.selected_artifacts),
                total_artifacts: None,
                completed_artifacts: Some(work.completed_artifacts),
                completed_units: Some(work.completed_units),
            });
            let mut changes = Vec::with_capacity(batch_len);
            for pending in &selected {
                let artifact = read
                    .artifact(&pending.path)?
                    .context("pending semantic artifact missing")?;
                ensure!(
                    artifact.content_hash.parse::<Sha256Digest>()? == pending.source_hash
                        && artifact
                            .retained
                            .as_ref()
                            .context("pending extraction missing")?
                            .object
                            .parse::<Sha256Digest>()?
                            == pending.extraction_object,
                    "pending semantic input binding expired"
                );
                changes.push(CapturedInput::new(artifact));
            }
            drop(read);
            self.emit_progress(airk_core::readiness::IndexProgress {
                collection: String::new(),
                phase: "enrichment_prepared",
                revision: None,
                processed_artifacts: batch_len as u64,
                committed_artifacts: 0,
                selected_artifacts: Some(work.selected_artifacts),
                total_artifacts: None,
                completed_artifacts: Some(work.completed_artifacts),
                completed_units: Some(work.completed_units),
            });
            let reservation = self.publication_reservation()?;
            let selected_paths: std::collections::BTreeSet<_> = selected
                .iter()
                .map(|pending| pending.path.as_str())
                .collect();
            work.pending
                .retain(|pending| !selected_paths.contains(pending.path.as_str()));
            work.completed_artifacts += batch_len as u64;
            work.failure = None;
            let result = CurrentUpdater {
                store: &self.store,
                quota: &quota,
                scope: &scope,
                writer: &writer,
                reservation: &reservation,
                configuration: None,
            }
            .apply_enrichment(
                &changes,
                SemanticUpdate {
                    model,
                    objects,
                    selection,
                    defer: false,
                    cache: EmbeddingCacheContext {
                        cache: None,
                        bypass: work.force_recompute,
                        stats,
                        max_workers: execution.plan.inference_workers,
                        working_memory_bytes: execution.plan.working_memory_bytes,
                    },
                },
                &mut work,
            )?;
            if let Some(work) = current_work.as_deref_mut() {
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
            }
            generation = Some(result.commit.token.sequence);
            self.emit_progress(airk_core::readiness::IndexProgress {
                collection: String::new(),
                phase: "enrichment_committed",
                revision: Some(super::runtime::revision(result.commit.token).to_string()),
                processed_artifacts: 0,
                committed_artifacts: 0,
                selected_artifacts: Some(work.selected_artifacts),
                total_artifacts: None,
                completed_artifacts: Some(work.completed_artifacts),
                completed_units: Some(work.completed_units),
            });
        }
        drop(captures);
        let maintenance_error = if generation.is_some() {
            super::update::reclaim_current_objects(&self.store, &quota, &scope, &writer)
                .err()
                .map(|error| format!("current object cleanup: {error:#}"))
        } else {
            None
        };
        Ok(DrainOutcome {
            generation,
            maintenance_error,
        })
    }
}

#[cfg(test)]
mod tests;
