// SPDX-License-Identifier: Apache-2.0
//! One changed-path publication across retained evidence, SQLite FTS5 and DiskANN.
use airk_core::{
    embedding::{EmbeddingDescriptor, embedding_key},
    graph_storage::{GraphRecordOverlay, GraphRecordReader, PreparedGraphDelta},
    identity::Sha256Digest,
    model_objects::ModelObjectSet,
    vector_selection::VectorSelectionPolicy,
};
use airk_retrieval::{
    embedding_batch::MAX_BATCH_CHUNKS,
    model::{EmbeddingCacheContext, EmbeddingModel},
    vector::mutable::MutableDiskIndex,
    vector_units::{PreparedVectorUnit, visit_vector_units},
};
use airk_store::{
    FileStore, WriterLock,
    current::{
        ArtifactChange, ArtifactVectorChanges, CapturedInput, CurrentCommit, CurrentRead,
        CurrentScope, MAX_PENDING_INPUTS, MAX_PENDING_VECTOR_INPUTS,
        graph::{GraphOverlay, GraphOverlayLimits, GraphStatistics},
        map::MapDomain,
        vector_map::{OccurrencePointer, PathOccurrences, PreparedOccurrence},
    },
    quota::{Quota, Reservation},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, sync::Arc};

const MAX_CAPTURED_BYTES: usize = 8 * 1024 * 1024 * 1024;

pub(super) fn reclaim_current_objects(
    store: &FileStore,
    quota: &Quota,
    scope: &CurrentScope,
    writer: &WriterLock,
) -> Result<()> {
    if writer.has_object_captures() {
        return Ok(());
    }
    let read = store.current_read(scope)?;
    if read.collection_state()?.is_none() {
        return Ok(());
    }
    let plan = read.reclamation_plan(store)?;
    drop(read);
    let capacity = store.current_database_capacity(scope)?;
    store
        .current_write(writer, quota, scope, capacity)?
        .reclaim_objects(&plan)?;
    Ok(())
}

pub struct SemanticUpdate<'a> {
    pub model: &'a EmbeddingModel,
    pub objects: ModelObjectSet,
    pub selection: &'a VectorSelectionPolicy,
    pub cache: EmbeddingCacheContext<'a>,
    /// Publish exact evidence and retire incompatible vectors before inference.
    pub defer: bool,
}

pub struct LexicalPublicationWork {
    pub changed: airk_store::current::lexical::LexicalWork,
}

pub struct CurrentUpdateResult {
    pub commit: CurrentCommit,
    pub lexical: LexicalPublicationWork,
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
    replacing: bool,
    recomputed_from: Option<u64>,
    registry: BTreeMap<Sha256Digest, Option<u32>>,
    memberships: BTreeMap<Sha256Digest, VectorMembership>,
    occurrences: Vec<PreparedOccurrence>,
    ordinals: BTreeMap<String, u32>,
}

struct PendingVectorUnit {
    path: String,
    source_hash: Sha256Digest,
    payload_digest: Sha256Digest,
    unit: PreparedVectorUnit,
}

enum Planned {
    Unit(PendingVectorUnit),
    Stats(bool, airk_retrieval::vector_units::VectorUnitStats),
}

impl VectorPlan<'_> {
    fn existing(&mut self, key: Sha256Digest) -> Result<Option<u32>> {
        if let Some(binding) = self.memberships.get(&key) {
            return Ok(Some(binding.id));
        }
        if self.replacing {
            return Ok(None);
        }
        let Some(binding) = self.read.vector_binding(key)? else {
            return Ok(None);
        };
        ensure!(
            self.memberships.len() < MAX_PENDING_VECTOR_INPUTS,
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
        self.retire_paths(changes.iter().map(path))
    }

    fn retire_paths<'a>(&mut self, paths: impl Iterator<Item = &'a str>) -> Result<()> {
        let mut work = 0usize;
        for path in paths {
            let Some(index) = self.read.path_occurrences(path)? else {
                continue;
            };
            for ordinal in 0..index.count {
                work += 1;
                ensure!(
                    work <= MAX_PENDING_VECTOR_INPUTS,
                    "current vector retirement work limit exceeded"
                );
                let id = index.occurrence_id(&self.read.head().scope, ordinal)?;
                let bytes = self
                    .read
                    .map_get(MapDomain::Occurrence, id.as_bytes())?
                    .context("current occurrence missing")?;
                let pointer = OccurrencePointer::decode(&bytes, id)?;
                ensure!(
                    pointer.path == path
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
        units: &mut Vec<PendingVectorUnit>,
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
            let key = embedding_key(descriptor, unit.unit.input.as_bytes())?;
            keys.push(key);
            if self.registry.contains_key(&key) {
                continue;
            }
            ensure!(
                self.registry.len() < MAX_PENDING_VECTOR_INPUTS,
                "current vector registry work limit exceeded"
            );
            if let Some(id) = self.existing(key)?
                && (!semantic.cache.bypass
                    || self
                        .recomputed_from
                        .is_some_and(|first_id| u64::from(id) >= first_id))
            {
                self.registry.insert(key, Some(id));
            } else {
                missing.entry(key).or_insert(index);
            }
        }
        // Keep graph insertion in captured-unit order across file boundaries.
        // Hash ordering is useful for deduplication, but changing insertion order
        // changes approximate-search tie behavior for repeated passages.
        let mut missing: Vec<_> = missing.into_iter().collect();
        missing.sort_by_key(|(_, index)| *index);
        let texts = missing
            .iter()
            .map(|(_, index)| units[*index].unit.input.as_str())
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
                self.registry.len() < MAX_PENDING_VECTOR_INPUTS,
                "current vector registry work limit exceeded"
            );
            let retained_members = if let Some(prior) = self.memberships.get(&key) {
                graph.set_live(prior.id, key, false)?;
                prior.remaining
            } else {
                0
            };
            let id = if let Some(vector) = vector {
                ensure!(
                    self.memberships.len() < MAX_PENDING_VECTOR_INPUTS,
                    "current vector membership work limit exceeded"
                );
                let started = std::time::Instant::now();
                let id = graph.insert(key, &vector)?;
                semantic.cache.stats.graph_insert_us +=
                    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
                self.memberships.insert(
                    key,
                    VectorMembership {
                        id,
                        remaining: retained_members,
                    },
                );
                semantic.cache.stats.vector_graph_nodes += 1;
                Some(id)
            } else {
                ensure!(
                    retained_members == 0,
                    "recomputed vector omitted retained occurrences"
                );
                None
            };
            self.registry.insert(key, id);
        }
        for (pending, key) in units.drain(..).zip(keys) {
            let Some(id) = self.registry[&key] else {
                continue;
            };
            ensure!(
                self.occurrences.len() < MAX_PENDING_VECTOR_INPUTS,
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
                path: pending.path.clone(),
                source_hash: pending.source_hash,
                count: 0,
            };
            let ordinal = self.ordinals.entry(pending.path).or_default();
            let index = PathOccurrences {
                count: ordinal
                    .checked_add(1)
                    .context("current occurrence ordinal overflow")?,
                ..index
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
                    payload_digest: pending.payload_digest,
                    object: pending.payload_digest,
                    members: pending.unit.members,
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
        semantic: Option<SemanticUpdate<'_>>,
        observer: &mut dyn FnMut(CurrentUpdateStage),
    ) -> Result<CurrentUpdateResult> {
        self.apply_resources_observed(changes, semantic, &[], None, None, None, observer)
    }

    pub fn apply_with_resources(
        &self,
        changes: &[ArtifactChange],
        semantic: Option<SemanticUpdate<'_>>,
        resources: &[airk_store::current::resources::ResourceChange],
    ) -> Result<CurrentUpdateResult> {
        self.apply_resources_observed(changes, semantic, resources, None, None, None, &mut |_| {})
    }

    pub(crate) fn apply_scanned(
        &self,
        changes: &[ArtifactChange],
        semantic: Option<SemanticUpdate<'_>>,
        resources: &[airk_store::current::resources::ResourceChange],
        moves: &BTreeMap<String, String>,
    ) -> Result<CurrentUpdateResult> {
        self.apply_resources_observed(
            changes,
            semantic,
            resources,
            Some(moves),
            None,
            None,
            &mut |_| {},
        )
    }

    /// Attach vectors to already committed exact artifacts. The captured inputs
    /// are validated against the current map and journaled without rewriting
    /// their source, fact, posting or lexical rows.
    pub(super) fn apply_enrichment(
        &self,
        inputs: &[CapturedInput],
        semantic: SemanticUpdate<'_>,
        work: &mut super::enrichment::SemanticWork,
    ) -> Result<CurrentUpdateResult> {
        ensure!(
            semantic.cache.bypass == work.force_recompute
                && semantic.objects.pipeline_digest == work.pipeline_digest
                && semantic.selection.digest()? == work.selection_digest,
            "semantic turn differs from its authenticated work policy"
        );
        self.apply_resources_observed(
            &[],
            Some(semantic),
            &[],
            None,
            Some(inputs),
            Some(work),
            &mut |_| {},
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_resources_observed(
        &self,
        changes: &[ArtifactChange],
        semantic: Option<SemanticUpdate<'_>>,
        resources: &[airk_store::current::resources::ResourceChange],
        moves: Option<&BTreeMap<String, String>>,
        semantic_inputs: Option<&[CapturedInput]>,
        mut semantic_work: Option<&mut super::enrichment::SemanticWork>,
        observer: &mut dyn FnMut(CurrentUpdateStage),
    ) -> Result<CurrentUpdateResult> {
        crate::service_execution::progress_phase(airk_core::service_progress::WorkPhase::Preparing);
        ensure!(
            (!changes.is_empty()
                || self.configuration.is_some()
                || !resources.is_empty()
                || semantic_work.is_some())
                && changes.len() <= MAX_PENDING_INPUTS
                && changes
                    .windows(2)
                    .all(|pair| path(&pair[0]) < path(&pair[1])),
            "invalid current changed-path batch"
        );
        ensure!(
            semantic_inputs.is_none_or(|inputs| {
                changes.is_empty()
                    && inputs.len() <= MAX_PENDING_INPUTS
                    && inputs
                        .windows(2)
                        .all(|pair| pair[0].artifact.path < pair[1].artifact.path)
            }),
            "invalid current semantic-only input batch"
        );
        let capacity = self.store.current_database_capacity(self.scope)?;
        let read = self
            .store
            .current_read(self.scope)
            .context("open current update read")?;
        let trusted_cache = read
            .resource(self.store, "embedding-cache")?
            .as_deref()
            .map(airk_store::vector_cache::VectorCacheManifest::from_bytes)
            .transpose()?;
        let owned_cache = semantic
            .as_ref()
            .filter(|s| !s.defer && s.cache.cache.is_none())
            .map(|s| {
                airk_store::vector_cache::VectorCache::new(
                    self.store.directory(),
                    self.reservation,
                    &s.model.cache_descriptor()?,
                    trusted_cache.as_ref(),
                )
            })
            .transpose()?;
        let mut semantic = semantic.map(|s| SemanticUpdate {
            model: s.model,
            objects: s.objects,
            selection: s.selection,
            defer: s.defer,
            cache: EmbeddingCacheContext {
                cache: s.cache.cache.or(owned_cache
                    .as_ref()
                    .map(|c| c as &dyn airk_core::embedding::EmbeddingCache)),
                bypass: s.cache.bypass,
                stats: s.cache.stats,
                max_workers: s.cache.max_workers,
                working_memory_bytes: s.cache.working_memory_bytes,
            },
        });
        let base = read.head().clone();
        let applied = read.collection_state()?;
        let configuration = self
            .configuration
            .or(applied.as_ref().map(|state| &state.configuration));
        let metadata = super::metadata::prepare(
            self.store,
            &read,
            changes,
            configuration,
            self.reservation,
            moves,
        )
        .context("prepare current metadata")?;
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
        let replacing = semantic
            .as_ref()
            .map(|s| -> Result<bool> {
                Ok((s.cache.bypass && semantic_inputs.is_none())
                    || base.graph_head.scope.specification
                        != airk_retrieval::vector::mutable::specification(
                            s.model.dimensions(),
                            s.objects.pipeline_digest,
                        )?)
            })
            .transpose()?
            .unwrap_or(false);
        let graph_limits = GraphOverlayLimits::for_indexing(self.reservation);
        let overlay = Arc::new(if replacing {
            let semantic = semantic.as_ref().expect("replacement model");
            let mut instance = [0; 16];
            getrandom::fill(&mut instance)
                .map_err(|e| anyhow::anyhow!("allocate graph identity: {e}"))?;
            let mut scope = base.graph_head.scope.clone();
            scope.graph_instance = airk_core::identity::Incarnation::from_bytes(instance);
            scope.specification = airk_retrieval::vector::mutable::specification(
                semantic.model.dimensions(),
                semantic.objects.pipeline_digest,
            )?;
            scope.record_bytes =
                airk_retrieval::vector::mutable::record_bytes(semantic.model.dimensions())?;
            GraphOverlay::empty(self.reservation, scope, graph_limits)
                .context("create replacement graph overlay")?
        } else {
            GraphOverlay::new(self.reservation, read.clone(), graph_limits)
                .context("open current graph overlay")?
        });
        let recomputed_from = if let Some(work) = semantic_work.as_deref_mut()
            && work.force_recompute
        {
            let graph = overlay.scope().graph_instance;
            let count = overlay.counts()?.allocated_count;
            if work
                .recompute_boundary
                .as_ref()
                .is_none_or(|boundary| boundary.graph != graph)
            {
                work.recompute_boundary = Some(super::enrichment::RecomputeBoundary {
                    graph,
                    first_id: count,
                });
            }
            let boundary = work.recompute_boundary.as_ref().expect("set above");
            ensure!(
                boundary.first_id <= count,
                "forced semantic boundary exceeds authenticated graph"
            );
            Some(boundary.first_id)
        } else {
            None
        };
        let mut plan = VectorPlan {
            read: &read,
            replacing,
            recomputed_from,
            registry: BTreeMap::new(),
            memberships: BTreeMap::new(),
            occurrences: Vec::new(),
            ordinals: BTreeMap::new(),
        };
        if !replacing {
            plan.retire(changes)?;
            if let Some(inputs) = semantic_inputs {
                plan.retire_paths(inputs.iter().map(|input| input.artifact.path.as_str()))?;
            }
        }
        let descriptor = semantic
            .as_ref()
            .map(|semantic| -> Result<_> {
                let descriptor = semantic.model.cache_descriptor()?;
                ensure!(
                    descriptor.identity()? == semantic.objects.pipeline_digest
                        && crate::shared_models::descriptor(self.store, &semantic.objects)?
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
                if replacing || base.graph_head.allocated_count == 0 {
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
        let inputs = semantic_inputs
            .map(|inputs| inputs.iter().collect::<Vec<_>>())
            .unwrap_or_else(|| {
                changes
                    .iter()
                    .filter_map(|change| match change {
                        ArtifactChange::Upsert(input) => Some(input.as_ref()),
                        ArtifactChange::Delete(_) => None,
                    })
                    .collect()
            });
        // One persistent planning producer overlaps source verification and
        // tokenization with inference/graph work. Bounded semantic units contain
        // at most 16 KiB input and 64 members; two batches fit in this channel.
        // Full extraction payloads stay on the producer, one file at a time.
        let before_units = semantic.as_ref().map_or(0, |s| s.cache.stats.vector_units);
        let planning = semantic.as_ref().map(|s| (s.model, s.selection, s.defer));
        let store = self.store;
        std::thread::scope(|scope| -> Result<()> {
            let (sender, receiver) =
                std::sync::mpsc::sync_channel::<Result<Planned>>(MAX_BATCH_CHUNKS * 2);
            let producer = scope.spawn(move || {
                let result = (|| -> Result<()> {
                    let mut captured_bytes = 0usize;
                    for input in inputs {
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
                        let retained = store.load_retrieval_artifact(&input.artifact)?;
                        store.verify_retained_source(&input.artifact)?;
                        let Some((model, selection, defer)) = planning else {
                            continue;
                        };
                        let eligible = selection
                            .decision(
                                &input.artifact.path,
                                &retained.profile,
                                &input.artifact.language,
                            )?
                            .eligible;
                        if !eligible {
                            if sender
                                .send(Ok(Planned::Stats(false, Default::default())))
                                .is_err()
                            {
                                return Ok(());
                            }
                            continue;
                        }
                        if defer {
                            continue;
                        }
                        let source_hash = input.artifact.content_hash.parse()?;
                        let stats = visit_vector_units(&retained.payload, model, |unit| {
                            sender
                                .send(Ok(Planned::Unit(PendingVectorUnit {
                                    path: input.artifact.path.clone(),
                                    source_hash,
                                    payload_digest: retained.payload_digest,
                                    unit,
                                })))
                                .map_err(|_| anyhow::anyhow!("vector planning consumer stopped"))
                        })?;
                        if sender.send(Ok(Planned::Stats(true, stats))).is_err() {
                            return Ok(());
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = sender.send(Err(error));
                }
            });
            let result = (|| -> Result<()> {
                let mut units = Vec::with_capacity(MAX_BATCH_CHUNKS);
                for message in receiver.iter() {
                    let message = message?;
                    let semantic = semantic
                        .as_mut()
                        .context("semantic planner without a model")?;
                    match message {
                        Planned::Stats(eligible, stats) => {
                            semantic.cache.stats.vector_eligible_artifacts += u64::from(eligible);
                            semantic.cache.stats.vector_excluded_artifacts += u64::from(!eligible);
                            semantic.cache.stats.vector_units += stats.units;
                            semantic.cache.stats.vector_body_bytes += stats.body_bytes;
                            semantic.cache.stats.vector_planning_us += stats.planning_us;
                        }
                        Planned::Unit(unit) => units.push(unit),
                    }
                    if units.len() == MAX_BATCH_CHUNKS {
                        plan.batch(
                            &mut units,
                            descriptor.as_ref().expect("semantic descriptor"),
                            semantic,
                            graph.as_mut().expect("semantic graph"),
                        )?;
                    }
                }
                if !units.is_empty() {
                    plan.batch(
                        &mut units,
                        descriptor.as_ref().expect("semantic descriptor"),
                        semantic.as_mut().expect("semantic model"),
                        graph.as_mut().expect("semantic graph"),
                    )?;
                }
                Ok(())
            })();
            // Release backpressure before joining after any consumer failure.
            drop(receiver);
            producer
                .join()
                .map_err(|_| anyhow::anyhow!("vector planning worker panicked"))?;
            result
        })?;
        for membership in plan.memberships.values() {
            overlay.set_live(membership.id, membership.remaining != 0)?;
        }
        drop(graph);
        if let Some(semantic) = semantic.as_mut() {
            semantic.cache.stats.vector_registry_rows = semantic
                .cache
                .stats
                .vector_registry_rows
                .max(plan.registry.len() as u64);
        }
        let occurrences = std::mem::take(&mut plan.occurrences);
        drop(plan);
        let mut resources = resources.to_vec();
        if let Some(work) = semantic_work {
            let units = semantic
                .as_ref()
                .context("semantic progress without model")?
                .cache
                .stats
                .vector_units
                .checked_sub(before_units)
                .context("semantic unit count underflow")?;
            work.completed_units = work
                .completed_units
                .checked_add(units)
                .context("semantic unit count overflow")?;
            if work.pending.is_empty() {
                if work.all_units_countable {
                    work.selected_units = Some(work.completed_units);
                }
                work.fully_ready_unix_ms = Some(super::enrichment::now_unix_ms()?);
            }
            resources.push(work.resource_change(self.store, self.reservation)?);
        }
        if let Some(cache) = &owned_cache {
            let started = std::time::Instant::now();
            let manifest = cache.finalize()?;
            if let Some(semantic) = semantic.as_mut() {
                let statistics = cache.statistics()?;
                let stats = &mut semantic.cache.stats;
                stats.cache_finalize_us = stats.cache_finalize_us.saturating_add(
                    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
                );
                stats.cache_file_opens =
                    stats.cache_file_opens.saturating_add(statistics.file_opens);
                stats.cache_read_bytes =
                    stats.cache_read_bytes.saturating_add(statistics.read_bytes);
                stats.cache_written_bytes = stats
                    .cache_written_bytes
                    .saturating_add(statistics.written_bytes);
                stats.cache_record_verification_bytes = stats
                    .cache_record_verification_bytes
                    .saturating_add(statistics.record_verification_bytes);
                stats.cache_index_verification_bytes = stats
                    .cache_index_verification_bytes
                    .saturating_add(statistics.index_verification_bytes);
                stats.cache_full_pack_verification_bytes = stats
                    .cache_full_pack_verification_bytes
                    .saturating_add(statistics.full_pack_verification_bytes);
                stats.cache_syncs = stats.cache_syncs.saturating_add(statistics.syncs);
                stats.cache_index_hits = stats
                    .cache_index_hits
                    .saturating_add(statistics.index_cache_hits);
                stats.cache_index_peak_bytes = stats
                    .cache_index_peak_bytes
                    .max(statistics.index_cache_peak_bytes);
                stats.cache_packs_written = stats
                    .cache_packs_written
                    .saturating_add(statistics.packs_written);
                stats.cache_packs_reused = stats
                    .cache_packs_reused
                    .saturating_add(statistics.packs_reused);
                stats.cache_compacted_records = stats
                    .cache_compacted_records
                    .saturating_add(statistics.compacted_records);
            }
            let reference = self
                .store
                .retain_current_resource(&manifest.canonical_bytes()?, self.reservation)?;
            resources.push(airk_store::current::resources::ResourceChange {
                key: "embedding-cache".into(),
                reference: Some(reference),
            });
        } else if captured_model.is_none() && trusted_cache.is_some() {
            resources.push(airk_store::current::resources::ResourceChange {
                key: "embedding-cache".into(),
                reference: None,
            });
        }
        drop(semantic);
        drop(read);
        let sealed = Arc::try_unwrap(overlay)
            .map_err(|_| anyhow::anyhow!("current graph retained preparation reader"))?
            .seal()
            .context("seal current graph overlay")?;
        let sealed = if replacing {
            sealed.replacing(base.graph_head.clone())?
        } else {
            sealed
        };
        let mut target = base.clone();
        target.sequence = target
            .sequence
            .checked_add(1)
            .context("current sequence overflow")?;
        target.lexical_sequence = target.sequence;
        target.graph_head = sealed.descriptor().target.clone();
        let mut writer = self
            .store
            .current_write(self.writer, self.quota, self.scope, capacity)?;
        let (pending, lexical_work) = writer
            .prepare_artifacts_with_fts(
                &base.token()?,
                target,
                &sealed,
                ArtifactVectorChanges {
                    artifacts: changes,
                    vector_inputs: semantic_inputs,
                    occurrences: &occurrences,
                    configuration: self.configuration,
                },
                captured_model,
                &metadata,
                &resources,
            )
            .context("prepare current journal")?;
        crate::service_execution::progress_prepared(
            semantic_inputs.map_or(changes.len(), |inputs| inputs.len()) as u64,
        );
        observer(CurrentUpdateStage::Prepared);
        crate::service_execution::progress_phase(
            airk_core::service_progress::WorkPhase::Publishing,
        );
        writer
            .mark_applying(&pending)
            .context("mark current journal applying")?;
        observer(CurrentUpdateStage::Applying);
        let mut committed = writer
            .commit(&pending, pending.target.lexical_commit)
            .context("commit current journal")?;
        crate::service_execution::progress_committed(
            semantic_inputs.map_or(changes.len(), |inputs| inputs.len()) as u64,
        );
        observer(CurrentUpdateStage::Committed);
        drop(writer);
        let cleanup = reclaim_current_objects(self.store, self.quota, self.scope, self.writer);
        if let Err(error) = cleanup {
            let message = format!("current object cleanup: {error:#}");
            committed.maintenance_error = Some(match committed.maintenance_error {
                Some(previous) => format!("{previous}; {message}"),
                None => message,
            });
        }
        Ok(CurrentUpdateResult {
            commit: committed,
            lexical: LexicalPublicationWork {
                changed: lexical_work,
            },
            graph: sealed.statistics().clone(),
            occurrences: occurrences.len(),
        })
    }
}
