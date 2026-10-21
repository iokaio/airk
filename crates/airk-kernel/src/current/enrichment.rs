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
    quota::{Quota, Reservation},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const KEY: &str = "semantic-work";
const POLICY_KEY: &str = "semantic-policy";

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
        force_recompute: if unchanged {
            compatible.is_some_and(|p| p.force_recompute)
        } else {
            full
        },
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
    };
    work.validate()?;
    Ok(work)
}

impl Kernel {
    pub fn set_default_semantic_search_policy(
        &self,
        policy: DefaultSearchPolicy,
    ) -> Result<SemanticReadiness> {
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        super::recover_current(&self.store, &Quota::open(&self.data_root)?, &scope)
            .context("recover current before semantic enrichment")?;
        let writer = self.store.lock()?;
        let read = self.store.current_read(&scope)?;
        if read_policy(&read, &self.store)? == policy {
            return readiness(&read, &self.store);
        }
        drop(read);
        let quota = Quota::open(&self.data_root)?;
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
        let writer = self.store.lock()?;
        let read = self.store.current_read(&scope)?;
        let Some(mut work) = read_work(&read, &self.store)? else {
            return Ok(());
        };
        if work.pending.is_empty() {
            return Ok(());
        }
        work.failure = Some(format!("{error:#}").chars().take(1024).collect());
        drop(read);
        let quota = Quota::open(&self.data_root)?;
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
        if error
            .downcast_ref::<crate::CurrentGateContention>()
            .is_some()
        {
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
    ) -> Result<Option<u64>> {
        match self.drain_current_enrichment(model, selection, stats, max_batches, current_work) {
            Ok(result) => Ok(result),
            Err(error) => Err(self.semantic_failure(error)),
        }
    }

    /// Resume retained semantic inputs without reading live workspace files.
    /// Each batch publishes its vectors and reduced pending set in one transaction.
    pub fn resume_current_enrichment(&self) -> Result<Option<u64>> {
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        super::recover_current(&self.store, &Quota::open(&self.data_root)?, &scope)
            .context("recover current before semantic enrichment")?;
        let read = self.store.current_read(&scope)?;
        let Some(work) = read_work(&read, &self.store)? else {
            return Ok(None);
        };
        if work.pending.is_empty() {
            return Ok(Some(read.head().sequence));
        }
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
        let model = match EmbeddingModel::open_objects(self.store.directory(), objects) {
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
        )
    }

    pub(crate) fn resume_current_enrichment_batches(
        &self,
        max_batches: usize,
    ) -> Result<Option<u64>> {
        ensure!(
            max_batches > 0 && max_batches <= 8,
            "semantic batch limit exceeded"
        );
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        super::recover_current(&self.store, &Quota::open(&self.data_root)?, &scope)?;
        let read = self.store.current_read(&scope)?;
        let Some(work) = read_work(&read, &self.store)? else {
            return Ok(None);
        };
        if work.pending.is_empty() {
            return Ok(Some(read.head().sequence));
        }
        let configuration = read
            .collection_state()?
            .context("current configuration missing")?
            .configuration;
        let objects = configuration
            .model
            .as_ref()
            .context("pending semantic model missing")?;
        let model = match EmbeddingModel::open_objects(self.store.directory(), objects) {
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
            max_batches,
            None,
        )
    }

    pub(super) fn drain_current_enrichment(
        &self,
        model: &EmbeddingModel,
        selection: &VectorSelectionPolicy,
        stats: &mut EmbeddingCacheStats,
        max_batches: usize,
        mut current_work: Option<&mut airk_ingest::CurrentIndexWork>,
    ) -> Result<Option<u64>> {
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        let execution = crate::execution::acquire(&self.data_root)?;
        let writer = self.store.lock()?;
        let quota = Quota::open(&self.data_root)?;
        let mut generation = None;
        for _ in 0..max_batches {
            let read = self.store.current_read(&scope)?;
            let Some(mut work) = read_work(&read, &self.store)? else {
                return Ok(generation);
            };
            generation = Some(read.head().sequence);
            if work.pending.is_empty() {
                return Ok(generation);
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
            let mut changes = Vec::with_capacity(batch_len);
            for pending in work.pending.iter().take(batch_len) {
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
                changes.push(CapturedInput { artifact });
            }
            drop(read);
            let reservation = self.publication_reservation()?;
            work.pending.drain(..batch_len);
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
                work.linked_lexical_bytes = work
                    .linked_lexical_bytes
                    .saturating_add(result.lexical.hardlinked_bytes);
            }
            generation = Some(result.commit.token.sequence);
            self.emit_progress(airk_core::readiness::IndexProgress {
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
        Ok(generation)
    }
}

#[cfg(test)]
mod tests;
