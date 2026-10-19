// SPDX-License-Identifier: Apache-2.0
//! One changed-path publication across retained evidence, Tantivy and DiskANN.
use airk_core::{
    RetrievalArtifact,
    embedding::{EmbeddingDescriptor, embedding_key},
    graph_storage::{GraphRecordOverlay, PreparedGraphDelta},
    identity::Sha256Digest,
    model_objects::ModelObjectSet,
    vector_selection::VectorSelectionPolicy,
};
use airk_retrieval::{
    bounded::Charge,
    current_lexical::{CurrentLexical, LexicalChange, LexicalForkWork, LexicalManifest},
    embedding_batch::MAX_BATCH_CHUNKS,
    model::{EmbeddingCacheContext, EmbeddingModel},
    vector::mutable::MutableDiskIndex,
    vector_units::{PreparedVectorUnit, visit_vector_units},
};
use airk_store::{
    FileStore, WriterLock,
    current::{
        ArtifactChange, ArtifactVectorChanges, CurrentCommit, CurrentRead, CurrentScope,
        MAX_PENDING_INPUTS, TransactionPhase,
        graph::{GraphOverlay, GraphOverlayLimits, GraphStatistics},
        map::MapDomain,
        vector_map::{OccurrencePointer, PathOccurrences, PreparedOccurrence},
    },
    quota::{Quota, Reservation},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, sync::Arc};

const MAX_CAPTURED_BYTES: usize = 256 * 1024 * 1024;

pub struct SemanticUpdate<'a> {
    pub model: &'a EmbeddingModel,
    pub objects: ModelObjectSet,
    pub selection: &'a VectorSelectionPolicy,
    pub cache: EmbeddingCacheContext<'a>,
}

pub struct CurrentUpdateResult {
    pub commit: CurrentCommit,
    pub lexical: LexicalForkWork,
    pub graph: GraphStatistics,
    pub occurrences: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrentUpdateStage {
    Prepared,
    Applying,
    Committed,
}

/// The caller owns the ordinary collection writer lock across planning and
/// publication. Read gates are acquired only during preparation and are all
/// released before the exclusive publication gate. No live source is opened.
pub struct CurrentUpdater<'a> {
    pub store: &'a FileStore,
    pub quota: &'a Quota,
    pub scope: &'a CurrentScope,
    pub writer: &'a WriterLock,
    pub reservation: &'a Reservation,
    /// Applied configuration travels in the same authenticated transaction.
    pub configuration: Option<&'a airk_store::current::collection::CollectionConfiguration>,
}

fn path(change: &ArtifactChange) -> &str {
    match change {
        ArtifactChange::Upsert(input) => &input.artifact.path,
        ArtifactChange::Delete(path) => path,
    }
}

struct VectorMembership {
    id: u32,
    remaining: usize,
}

struct VectorPlan<'a> {
    read: &'a CurrentRead,
    registry: BTreeMap<Sha256Digest, Option<u32>>,
    memberships: BTreeMap<Sha256Digest, VectorMembership>,
    occurrences: Vec<PreparedOccurrence>,
}

impl VectorPlan<'_> {
    fn existing(&mut self, key: Sha256Digest) -> Result<Option<u32>> {
        if let Some(binding) = self.memberships.get(&key) {
            return Ok(Some(binding.id));
        }
        let Some(binding) = self.read.vector_binding(key)? else {
            return Ok(None);
        };
        ensure!(
            self.memberships.len() < MAX_PENDING_INPUTS,
            "current vector membership work limit exceeded"
        );
        let id = binding.graph_id;
        self.memberships.insert(
            key,
            VectorMembership {
                id,
                remaining: binding.occurrence_ids.len(),
            },
        );
        Ok(Some(id))
    }

    fn retire(&mut self, changes: &[ArtifactChange]) -> Result<()> {
        let mut work = 0usize;
        for change in changes {
            let Some(index) = self.read.path_occurrences(path(change))? else {
                continue;
            };
            for ordinal in 0..index.count {
                work += 1;
                ensure!(
                    work <= MAX_PENDING_INPUTS,
                    "current vector retirement work limit exceeded"
                );
                let id = index.occurrence_id(&self.read.head().scope, ordinal)?;
                let bytes = self
                    .read
                    .map_get(MapDomain::Occurrence, id.as_bytes())?
                    .context("current occurrence missing")?;
                let pointer = OccurrencePointer::decode(&bytes, id)?;
                ensure!(
                    pointer.path == path(change)
                        && pointer.path_ordinal == ordinal
                        && pointer.source_hash == index.source_hash,
                    "current occurrence differs from changed path"
                );
                self.existing(pointer.embedding_key)?
                    .context("current vector binding missing")?;
                let binding = self
                    .read
                    .vector_binding(pointer.embedding_key)?
                    .context("current vector binding missing")?;
                ensure!(
                    binding.occurrence_ids.binary_search(&id).is_ok(),
                    "current vector omits path occurrence"
                );
                let member = self
                    .memberships
                    .get_mut(&pointer.embedding_key)
                    .expect("loaded above");
                member.remaining = member
                    .remaining
                    .checked_sub(1)
                    .context("current vector retirement underflow")?;
            }
        }
        Ok(())
    }

    fn batch(
        &mut self,
        retained: &RetrievalArtifact,
        units: &mut Vec<PreparedVectorUnit>,
        ordinal: &mut u32,
        descriptor: &EmbeddingDescriptor,
        semantic: &mut SemanticUpdate<'_>,
        graph: &mut MutableDiskIndex,
    ) -> Result<()> {
        if units.is_empty() {
            return Ok(());
        }
        let mut keys = Vec::new();
        let mut missing = BTreeMap::new();
        for (index, unit) in units.iter().enumerate() {
            let key = embedding_key(descriptor, unit.input.as_bytes())?;
            keys.push(key);
            if self.registry.contains_key(&key) {
                continue;
            }
            ensure!(
                self.registry.len() < MAX_PENDING_INPUTS,
                "current vector registry work limit exceeded"
            );
            if let Some(id) = self.existing(key)? {
                self.registry.insert(key, Some(id));
            } else {
                missing.entry(key).or_insert(index);
            }
        }
        let texts = missing
            .values()
            .map(|index| units[*index].input.as_str())
            .collect::<Vec<_>>();
        let vectors = if texts.is_empty() {
            Vec::new()
        } else {
            semantic.model.embed_batch(&texts, &mut semantic.cache)?
        };
        ensure!(
            vectors.len() == missing.len(),
            "current embedding result count differs"
        );
        semantic.cache.stats.vector_reused_inputs += (units.len() - missing.len()) as u64;
        for ((key, _), vector) in missing.into_iter().zip(vectors) {
            ensure!(
                self.registry.len() < MAX_PENDING_INPUTS,
                "current vector registry work limit exceeded"
            );
            let id = if let Some(vector) = vector {
                ensure!(
                    self.memberships.len() < MAX_PENDING_INPUTS,
                    "current vector membership work limit exceeded"
                );
                let started = std::time::Instant::now();
                let id = graph.insert(key, &vector)?;
                semantic.cache.stats.graph_insert_us +=
                    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
                self.memberships
                    .insert(key, VectorMembership { id, remaining: 0 });
                semantic.cache.stats.vector_graph_nodes += 1;
                Some(id)
            } else {
                None
            };
            self.registry.insert(key, id);
        }
        for (unit, key) in units.drain(..).zip(keys) {
            let Some(id) = self.registry[&key] else {
                continue;
            };
            ensure!(
                self.occurrences.len() < MAX_PENDING_INPUTS,
                "current vector occurrence work limit exceeded"
            );
            let member = self
                .memberships
                .get_mut(&key)
                .context("current vector membership missing")?;
            member.remaining = member
                .remaining
                .checked_add(1)
                .context("current vector membership overflow")?;
            ensure!(
                member.remaining <= airk_store::current::vector_map::MAX_VECTOR_OCCURRENCES,
                "current vector occurrence fan-out exceeded"
            );
            let index = PathOccurrences {
                version: 1,
                path: retained.artifact.path.clone(),
                source_hash: retained.artifact.content_hash.parse()?,
                count: ordinal
                    .checked_add(1)
                    .context("current occurrence ordinal overflow")?,
            };
            self.occurrences.push(PreparedOccurrence {
                graph_id: id,
                pointer: OccurrencePointer {
                    version: 1,
                    embedding_key: key,
                    occurrence_id: index.occurrence_id(&self.read.head().scope, *ordinal)?,
                    path: index.path,
                    path_ordinal: *ordinal,
                    source_hash: index.source_hash,
                    payload_digest: retained.payload_digest,
                    object: retained.payload_digest,
                    members: unit.members,
                },
            });
            *ordinal += 1;
        }
        Ok(())
    }
}

impl CurrentUpdater<'_> {
    /// Apply a bounded sorted batch. An empty/no-op scan is handled by the
    /// caller without publication; an explicit full rebuild supplies all paths.
    pub fn apply(
        &self,
        changes: &[ArtifactChange],
        semantic: Option<SemanticUpdate<'_>>,
    ) -> Result<CurrentUpdateResult> {
        self.apply_observed(changes, semantic, &mut |_| {})
    }

    /// Progress notifications occur only after their durable boundary. An
    /// observer cannot veto a successful commit; it must not reenter the store
    /// while the writer gate is held.
    pub fn apply_observed(
        &self,
        changes: &[ArtifactChange],
        mut semantic: Option<SemanticUpdate<'_>>,
        observer: &mut dyn FnMut(CurrentUpdateStage),
    ) -> Result<CurrentUpdateResult> {
        ensure!(
            !changes.is_empty()
                && changes.len() <= MAX_PENDING_INPUTS
                && changes
                    .windows(2)
                    .all(|pair| path(&pair[0]) < path(&pair[1])),
            "invalid current changed-path batch"
        );
        let capacity = self.store.current_database_capacity(self.scope)?;
        {
            let mut writer =
                self.store
                    .current_write(self.writer, self.quota, self.scope, capacity)?;
            ensure!(
                writer.phase()? == TransactionPhase::Ready && writer.pending()?.is_none(),
                "current update requires recovery first"
            );
            writer.reclaim_lexical_candidates()?;
        }
        let read = self.store.current_read(self.scope)?;
        let base = read.head().clone();
        let applied = read.collection_state()?;
        let configuration = self
            .configuration
            .or(applied.as_ref().map(|state| &state.configuration));
        if let (Some(configuration), Some(semantic)) = (configuration, semantic.as_ref()) {
            ensure!(
                configuration.vector_selection.digest()? == semantic.selection.digest()?,
                "current vector selection differs from applied configuration"
            );
        }
        let captured_model = if let Some(semantic) = &semantic {
            Some(semantic.objects.clone())
        } else if let Some(configuration) = self.configuration {
            configuration.model.clone()
        } else {
            applied
                .as_ref()
                .and_then(|state| state.configuration.model.clone())
        };
        ensure!(
            captured_model.is_none()
                || semantic.is_some()
                || changes
                    .iter()
                    .all(|change| matches!(change, ArtifactChange::Delete(_))),
            "current semantic model is required for changed artifacts"
        );
        let manifest = LexicalManifest::decode(&read.lexical_manifest()?, base.lexical_commit)?;
        let verified = manifest.verify_owned(
            &self.store.directory().join(base.lexical_directory_name()?),
            base.lexical_commit,
        )?;
        let overlay = Arc::new(GraphOverlay::new(
            self.reservation,
            read.clone(),
            GraphOverlayLimits::default(),
        )?);
        let mut plan = VectorPlan {
            read: &read,
            registry: BTreeMap::new(),
            memberships: BTreeMap::new(),
            occurrences: Vec::new(),
        };
        plan.retire(changes)?;
        let descriptor = semantic
            .as_ref()
            .map(|semantic| -> Result<_> {
                let descriptor = semantic.model.cache_descriptor()?;
                ensure!(
                    descriptor.identity()? == semantic.objects.pipeline_digest
                        && airk_retrieval::model::descriptor_from_objects_metadata(
                            self.store.directory(),
                            &semantic.objects
                        )?
                        .identity()?
                            == descriptor.identity()?,
                    "current model objects differ from loaded pipeline"
                );
                semantic.selection.validate()?;
                Ok(descriptor)
            })
            .transpose()?;
        let mut graph = semantic
            .as_ref()
            .map(|semantic| {
                if base.graph_head.allocated_count == 0 {
                    MutableDiskIndex::create(
                        overlay.clone(),
                        semantic.model.dimensions(),
                        semantic.objects.pipeline_digest,
                    )
                } else {
                    MutableDiskIndex::open_overlay(
                        overlay.clone(),
                        semantic.model.dimensions(),
                        semantic.objects.pipeline_digest,
                    )
                }
            })
            .transpose()?;
        let mut artifacts = Vec::with_capacity(changes.len());
        let mut captured_bytes = 0usize;
        for change in changes {
            let ArtifactChange::Upsert(input) = change else {
                artifacts.push(None);
                continue;
            };
            let reference = input
                .artifact
                .retained
                .as_ref()
                .context("current changed extraction missing")?;
            captured_bytes = captured_bytes
                .checked_add(input.artifact.bytes)
                .and_then(|n| n.checked_add(reference.object_bytes))
                .context("current changed-byte overflow")?;
            ensure!(
                captured_bytes <= MAX_CAPTURED_BYTES,
                "current changed batch exceeds evidence byte budget"
            );
            let retained = self.store.load_retrieval_artifact(&input.artifact)?;
            self.store.read_retained_source(&input.artifact)?;
            if let Some(semantic) = semantic.as_mut() {
                if semantic
                    .selection
                    .decision(
                        &input.artifact.path,
                        &retained.profile,
                        &input.artifact.language,
                    )?
                    .eligible
                {
                    semantic.cache.stats.vector_eligible_artifacts += 1;
                    let mut units = Vec::with_capacity(MAX_BATCH_CHUNKS);
                    let mut ordinal = 0;
                    let model = semantic.model;
                    let mut flush = |units: &mut Vec<PreparedVectorUnit>| {
                        plan.batch(
                            &retained,
                            units,
                            &mut ordinal,
                            descriptor.as_ref().expect("semantic descriptor"),
                            semantic,
                            graph.as_mut().expect("semantic graph"),
                        )
                    };
                    let stats = visit_vector_units(&retained.payload, model, |unit| {
                        units.push(unit);
                        if units.len() == MAX_BATCH_CHUNKS {
                            flush(&mut units)?;
                        }
                        Ok(())
                    })?;
                    flush(&mut units)?;
                    semantic.cache.stats.vector_units += stats.units;
                    semantic.cache.stats.vector_body_bytes += stats.body_bytes;
                    semantic.cache.stats.vector_planning_us += stats.planning_us;
                } else {
                    semantic.cache.stats.vector_excluded_artifacts += 1;
                }
            }
            artifacts.push(Some(retained.artifact));
        }
        for membership in plan.memberships.values() {
            overlay.set_live(membership.id, membership.remaining != 0)?;
        }
        drop(graph);
        let occurrences = std::mem::take(&mut plan.occurrences);
        drop(plan);
        drop(read);
        let sealed = Arc::try_unwrap(overlay)
            .map_err(|_| anyhow::anyhow!("current graph retained preparation reader"))?
            .seal()?;
        let mut target = base.clone();
        target.sequence = target
            .sequence
            .checked_add(1)
            .context("current sequence overflow")?;
        target.lexical_sequence = target.sequence;
        target.graph_head = sealed.descriptor().target.clone();
        let lexical_changes = changes
            .iter()
            .zip(&artifacts)
            .map(|(change, artifact)| LexicalChange {
                path: path(change),
                artifact: artifact.as_ref(),
            })
            .collect::<Vec<_>>();
        let reservation = self.reservation.clone();
        let charge: Charge = Arc::new(move |bytes| reservation.charge(bytes));
        let (lexical, ticket, lexical_work) = CurrentLexical::fork_changed(
            &verified,
            &self
                .store
                .directory()
                .join(target.lexical_directory_name()?),
            charge,
            &lexical_changes,
        )?;
        drop(lexical);
        let manifest_bytes = ticket.manifest().canonical_bytes()?;
        let mut writer = self
            .store
            .current_write(self.writer, self.quota, self.scope, capacity)?;
        let pending = writer.prepare_artifacts_with_occurrences(
            &base.token()?,
            target,
            &sealed,
            ArtifactVectorChanges {
                artifacts: changes,
                occurrences: &occurrences,
                configuration: self.configuration,
            },
            captured_model,
            &manifest_bytes,
        )?;
        observer(CurrentUpdateStage::Prepared);
        writer.mark_applying(&pending)?;
        observer(CurrentUpdateStage::Applying);
        let mut committed = writer.commit(&pending, pending.target.lexical_commit)?;
        observer(CurrentUpdateStage::Committed);
        if let Err(error) = writer.reclaim_lexical_candidates() {
            let message = format!("current lexical cleanup: {error:#}");
            committed.maintenance_error = Some(match committed.maintenance_error {
                Some(previous) => format!("{previous}; {message}"),
                None => message,
            });
        }
        Ok(CurrentUpdateResult {
            commit: committed,
            lexical: lexical_work,
            graph: sealed.statistics().clone(),
            occurrences: occurrences.len(),
        })
    }
}
