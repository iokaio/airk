// SPDX-License-Identifier: Apache-2.0
//! Scan preparation and publication for the single current index.
use super::{CurrentUpdater, SemanticUpdate, enrichment};
use crate::Kernel;
use airk_core::{
    graph_storage::{GraphHead, GraphScope},
    identity::{Incarnation, framed_digest},
};
use airk_retrieval::{
    current_lexical::CurrentLexical,
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
use std::sync::Arc;

fn incarnation() -> Result<Incarnation> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("allocate current identity: {e}"))?;
    Ok(Incarnation::from_bytes(bytes))
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeBootstrap {
    head: CurrentHead,
    manifest: airk_retrieval::current_lexical::LexicalManifest,
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
        let quota = Quota::open(&self.data_root)?;
        let mut current = self
            .store
            .current_write(&lock, &quota, &recipe.head.scope, 16 << 20)?;
        if current.head().is_ok_and(|h| h.sequence > 0) {
            self.store.finish_runtime_bootstrap(&lock)?;
            return Ok(());
        }
        recipe.manifest.verify(
            &self.store.directory().join("lexical-0"),
            recipe.head.lexical_commit,
        )?;
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
        let quota = Quota::open(&self.data_root)?;
        let reservation = self.publication_reservation()?;
        let policy = self.rebuild_extraction_policy()?.policy;
        let directory = self.data_root.join("models/local");
        let model = if self.vectors_enabled()? && directory.try_exists()? {
            Some(EmbeddingModel::open(&directory)?)
        } else {
            None
        };
        let objects = model
            .as_ref()
            .map(|_| airk_retrieval::model::inspect_object_plan(&directory))
            .transpose()?;
        if let Some(objects) = &objects {
            airk_store::model_objects::ModelObjects::new(self.store.directory()).retain(
                &directory,
                objects,
                &reservation,
            )?;
        }
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
            incarnation: incarnation()?,
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
        let initial = self.store.directory().join("lexical-0");
        if initial.try_exists()? {
            // A process can exit after creating Tantivy but before saving its
            // bootstrap recipe. No owner/head exists in this branch.
            ensure!(
                self.current_scope()?.is_none() && self.store.runtime_bootstrap()?.is_none(),
                "cannot replace an owned lexical bootstrap"
            );
            let parent = self.store.directory().canonicalize()?;
            let metadata = std::fs::symlink_metadata(&initial)?;
            ensure!(
                metadata.is_dir()
                    && !metadata.file_type().is_symlink()
                    && initial.canonicalize()?.parent() == Some(parent.as_path()),
                "linked lexical bootstrap"
            );
            let files = std::fs::read_dir(&initial)?.collect::<std::io::Result<Vec<_>>>()?;
            ensure!(
                files.len() <= 4096,
                "lexical bootstrap entry bound exceeded"
            );
            for file in &files {
                let m = std::fs::symlink_metadata(file.path())?;
                ensure!(
                    m.is_file() && !m.file_type().is_symlink(),
                    "invalid lexical bootstrap entry"
                );
            }
            std::fs::remove_dir_all(&initial)?;
        }
        let charged = reservation.clone();
        let lexical = CurrentLexical::create(
            &self.store.directory().join("lexical-0"),
            Arc::new(move |bytes| charged.charge(bytes)),
        )?;
        let manifest = lexical.capture_manifest(None)?;
        let head = CurrentHead {
            version: 2,
            scope: scope.clone(),
            sequence: 0,
            artifact_root: airk_store::current::map::empty_root(&scope)?,
            lexical_sequence: 0,
            lexical_commit: manifest.digest()?,
            graph_head: GraphHead::empty(graph_scope)?,
        };
        drop(lexical);
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
        let index_started = std::time::Instant::now();
        self.recover_runtime_bootstrap()?;
        let execution = crate::limits::configured_extraction_execution()?;
        // Recovery takes its own writer lock, before this scan owns one.
        if let Some(scope) = self.current_scope()? {
            super::recover_current(&self.store, &Quota::open(&self.data_root)?, &scope)?;
        }
        let writer = self.store.lock()?;
        let quota = Quota::open(&self.data_root)?;
        ensure!(
            !self.root.starts_with(quota.root()),
            "index storage must not contain sources"
        );
        let mut previous = self
            .load_optional()?
            .unwrap_or_else(|| self.empty_snapshot());
        let configured = self.rebuild_extraction_policy()?;
        let policy = configured.policy;
        let roots = self.configured_roots()?;
        let selection = self.vector_selection()?;
        // Do not reserve all remaining capacity: durable SQL growth/replay owns
        // an independent grant while preparation is still alive.
        let available = quota.status()?;
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
        let reservation = quota.reserve_available(
            &self.collection,
            (root_free.min(collection_free) / 2).min(32 << 30),
        )?;
        let model_directory = self.data_root.join("models/local");
        let model_started = std::time::Instant::now();
        let model = if self.vectors_enabled()? && model_directory.try_exists()? {
            Some(EmbeddingModel::open(&model_directory)?)
        } else {
            None
        };
        let model_load_us = u64::try_from(model_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let objects = model
            .as_ref()
            .map(|_| airk_retrieval::model::inspect_object_plan(&model_directory))
            .transpose()?;
        if let Some(objects) = &objects {
            airk_store::model_objects::ModelObjects::new(self.store.directory()).retain(
                &model_directory,
                objects,
                &reservation,
            )?;
        }
        let configuration = CollectionConfiguration {
            roots: roots.clone(),
            extractor_version: airk_core::EXTRACTOR_VERSION.into(),
            extraction_policy_digest: crate::extraction::policy_digest(&policy)?,
            index_detail: policy.detail.unwrap_or_default(),
            vector_selection: selection.clone(),
            model: objects.clone(),
        };
        let existing = self.current_scope()?;
        let (applied, prior_semantic_work) = if let Some(scope) = &existing {
            let read = self.store.current_read(scope)?;
            (
                read.collection_state()?,
                enrichment::read_work(&read, &self.store)?,
            )
        } else {
            (None, None)
        };
        let configuration_changed = applied
            .as_ref()
            .map(|s| serde_json::to_vec(&s.configuration))
            .transpose()?
            != Some(serde_json::to_vec(&configuration)?);
        if full {
            previous.extractor_version.clear();
        }
        let mut reusable = std::collections::BTreeMap::new();
        for artifact in previous.artifacts.values() {
            let profile = artifact
                .retained
                .as_ref()
                .and_then(|r| r.profile.as_ref())
                .context("prior extraction profile missing")?;
            reusable
                .entry((
                    artifact.content_hash.clone(),
                    artifact.bytes,
                    profile.identity()?,
                ))
                .or_insert(artifact);
        }
        let scan_started = std::time::Instant::now();
        let (snapshot, mut report, observation) = airk_ingest::scan_payloads_with_execution(
            &self.root,
            &self.data_root,
            &previous,
            &roots,
            self.scan_limits()?,
            &policy,
            full,
            execution.execution,
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
                self.store.load_retained(old)?;
                self.store.read_retained_source(old)?;
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
                if artifact
                    .retained
                    .as_ref()
                    .is_some_and(|r| r.profile.as_ref() == Some(profile))
                {
                    return Ok(artifact.clone());
                }
                self.store.retain_payload_with_budget(
                    artifact,
                    payload.context("fresh extraction payload missing")?,
                    profile,
                    Some(bytes),
                    Some(&reservation),
                )
            },
        )?;
        report.extraction_workers_requested = execution.requested;
        report.scan_extract_us =
            u64::try_from(scan_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let mut changes = std::collections::BTreeMap::new();
        for (path, artifact) in &snapshot.artifacts {
            let same = previous
                .artifacts
                .get(path)
                .map(serde_json::to_vec)
                .transpose()?
                == Some(serde_json::to_vec(artifact)?);
            if full || configuration_changed || !same {
                changes.insert(
                    path.clone(),
                    ArtifactChange::Upsert(Box::new(CapturedInput {
                        artifact: artifact.clone(),
                    })),
                );
            }
        }
        for path in previous
            .artifacts
            .keys()
            .filter(|p| !snapshot.artifacts.contains_key(*p))
        {
            changes.insert(path.clone(), ArtifactChange::Delete(path.clone()));
        }
        let semantic_work = objects
            .as_ref()
            .map(|objects| {
                enrichment::prepare_work(
                    &snapshot,
                    &changes,
                    prior_semantic_work.as_ref(),
                    &selection,
                    objects.pipeline_digest,
                    full,
                )
            })
            .transpose()?;
        let moves = compatible_moves(&self.store, &previous, &snapshot, &observation)?;
        let scope = match existing {
            Some(scope) => scope,
            None => self.bootstrap_current(
                &writer,
                &quota,
                &configuration,
                model.as_ref().map_or(1, EmbeddingModel::dimensions),
                &reservation,
            )?,
        };
        let mut stats = EmbeddingCacheStats {
            model_load_us,
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let analysis_expired = previous.analysis.as_ref().is_some_and(|manifest| {
            airk_ingest::semantic::validate_build_inputs(&self.root, manifest).is_err()
        });
        let retry_failed_semantic = prior_semantic_work
            .as_ref()
            .is_some_and(|work| work.failure.is_some());
        if !changes.is_empty() || configuration_changed || analysis_expired || retry_failed_semantic
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
                            bypass: full,
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
                &[
                    airk_store::current::resources::ResourceChange {
                        key: "analysis".into(),
                        reference: None,
                    },
                    semantic_resource.unwrap_or(airk_store::current::resources::ResourceChange {
                        key: "semantic-work".into(),
                        reference: None,
                    }),
                ],
                &moves,
            )?;
            report.generation = result.commit.token.sequence;
            report.maintenance_error = result.commit.maintenance_error;
            report.current = Some(airk_ingest::CurrentIndexWork {
                changed_artifacts: result.lexical.changed.changed_artifacts,
                lexical_documents: result.lexical.changed.added_documents,
                graph_records: result.graph.sealed_records,
                graph_read_bytes: result.graph.read_bytes,
                graph_base_record_reads: result.graph.base_record_reads,
                graph_proof_cache_hits: result.graph.base_proof_cache_hits,
                graph_proof_cache_evictions: result.graph.base_proof_cache_evictions,
                linked_lexical_bytes: result.lexical.hardlinked_bytes,
            });
        } else {
            let read = self.store.current_read(&scope)?;
            super::open_selected_lexical(&self.store, &read, Arc::new(|_| Ok(())))?;
            report.generation = read.head().sequence;
            report.current = Some(Default::default());
        }
        report.first_ready_us =
            Some(u64::try_from(index_started.elapsed().as_micros()).unwrap_or(u64::MAX));
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
        report.embedding_cache_full_pack_verification_bytes =
            stats.cache_full_pack_verification_bytes;
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
        Ok(report)
    }
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
