// SPDX-License-Identifier: Apache-2.0
//! One preparation/finalization path for all collection writers.
use crate::Kernel;
use airk_core::{
    Artifact, Snapshot,
    changes::{
        ChangeCause, ChangeDimension, ChangeEvent, ChangeSubject, ContinuityBasis, FileStamps,
        FileState, PublicationReason,
    },
    identity::{
        ArtifactId, Incarnation, RevisionToken, Sha256Digest, framed_digest, revision_digest,
    },
    publication::{InventoryFile, InventoryV2},
};
use airk_retrieval::persistent;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn random_identity() -> Result<[u8; 16]> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("allocate collection identity: {error}"))?;
    Ok(bytes)
}

impl Kernel {
    /// Convert only verified output; unknown legacy provider identities are not
    /// silently relabeled as native syntax. Live bytes must match indexed bytes.
    pub(crate) fn retain_v2(
        &self,
        artifact: &Artifact,
        bytes: Option<&[u8]>,
        reservation: &airk_store::quota::Reservation,
    ) -> Result<Artifact> {
        if artifact
            .retained
            .as_ref()
            .is_some_and(|reference| reference.profile.is_some())
        {
            self.store.load_retained(artifact)?;
            return Ok(artifact.clone());
        }
        let hydrated = self.store.load_retained(artifact)?;
        let source;
        let bytes = if let Some(bytes) = bytes {
            bytes
        } else {
            source = if artifact
                .retained
                .as_ref()
                .is_some_and(|reference| reference.raw_source)
            {
                self.store.read_retained_source(artifact)?
            } else {
                let roots = self.configured_roots()?;
                let location =
                    airk_store::roots::source_location(&self.root_id(), &roots, &artifact.path)?;
                airk_ingest::read_selected_source(
                    Path::new(&location.root),
                    &self.data_root,
                    &location.relative_path,
                )?
            };
            &source
        };
        let profile = airk_ingest::adapters::legacy::legacy_profile(&hydrated)?;
        let payload = airk_ingest::continuity::prepare_native(&hydrated, bytes, &profile)?;
        let prepared = airk_core::binding::bind_payload(
            &payload,
            &profile,
            airk_core::binding::ArtifactBinding {
                path: &hydrated.path,
                source_hash: hydrated.content_hash.parse()?,
                source_bytes: hydrated.bytes,
            },
        )?;
        self.store.retain_payload_with_budget(
            &prepared,
            &payload,
            &profile,
            Some(bytes),
            Some(reservation),
        )
    }

    pub(crate) fn publish_v2(
        &self,
        writer: &airk_store::WriterLock,
        snapshot: &Snapshot,
        reservation: airk_store::quota::Reservation,
        reason: PublicationReason,
    ) -> Result<u64> {
        self.publish_v2_observed(writer, snapshot, reservation, reason, None)
    }

    pub(crate) fn publish_v2_observed(
        &self,
        writer: &airk_store::WriterLock,
        snapshot: &Snapshot,
        reservation: airk_store::quota::Reservation,
        reason: PublicationReason,
        observation: Option<&airk_ingest::observations::ScanObservation>,
    ) -> Result<u64> {
        Ok(self
            .publish_v2_observed_metrics(writer, snapshot, reservation, reason, observation)?
            .0)
    }

    pub(crate) fn publish_v2_observed_metrics(
        &self,
        writer: &airk_store::WriterLock,
        snapshot: &Snapshot,
        reservation: airk_store::quota::Reservation,
        reason: PublicationReason,
        observation: Option<&airk_ingest::observations::ScanObservation>,
    ) -> Result<(u64, airk_retrieval::model::EmbeddingCacheStats)> {
        let quota = airk_store::quota::Quota::open(&self.data_root)?;
        ensure!(
            snapshot.root == self.root_id() && snapshot.collection == self.collection,
            "foreign publication binding"
        );
        let expected = self.store.expected_head()?;
        let old = self.store.catalog()?;
        let prior = old.as_ref().and_then(|catalog| catalog.inventory.as_ref());
        let committed = self.store.committed_revision()?;
        let parent = match &committed {
            Some(airk_store::CommittedRevision::V2 { receipt, .. }) => Some(receipt.token),
            _ => None,
        };
        let generation = if let Some(parent) = parent {
            parent.next_generation()?
        } else if let Some(old) = &old {
            old.snapshot
                .generation
                .checked_add(1)
                .context("generation overflow")?
        } else if reason == PublicationReason::Initialization {
            0
        } else {
            1
        };
        let incarnation = prior
            .map(|inventory| inventory.incarnation)
            .map(Ok)
            .unwrap_or_else(|| random_identity().map(Incarnation::from_bytes))?;
        let mut projection = snapshot.clone();
        projection.generation = generation;
        projection.schema_version = airk_core::publication::SNAPSHOT_PROJECTION_VERSION;
        projection.storage_format = 2;
        projection.persistent_indexes = true;
        for artifact in projection.artifacts.values_mut() {
            *artifact = self.retain_v2(artifact, None, &reservation)?;
        }
        let producer_domain = digest(&serde_json::to_vec(&(
            &projection.analysis,
            &projection.imported_facts,
            &projection.imported_relationships,
        ))?);
        let mut files = BTreeMap::new();
        let mut profiles = BTreeMap::new();
        for (path, artifact) in &projection.artifacts {
            let reference = artifact
                .retained
                .as_ref()
                .context("publication payload missing")?;
            let profile = reference
                .profile
                .as_ref()
                .context("publication profile missing")?;
            let profile_id = profile.identity()?;
            profiles.insert(profile_id, profile.clone());
            let payload_digest: Sha256Digest = reference.object.parse()?;
            let source_hash: Sha256Digest = artifact.content_hash.parse()?;
            let location =
                airk_store::roots::source_location(&projection.root, &projection.roots, path)?;
            let previous = prior
                .and_then(|inventory| inventory.files.get(path))
                .filter(|file| file.state.root == location.root);
            let artifact_id = previous
                .map(|file| file.state.artifact_id)
                .map(Ok)
                .unwrap_or_else(|| random_identity().map(ArtifactId::from_bytes))?;
            let evidence_digest = framed_digest(
                b"bound-evidence/1",
                &[
                    incarnation.as_bytes(),
                    artifact_id.as_bytes(),
                    location.root.as_bytes(),
                    location.relative_path.as_bytes(),
                    payload_digest.as_bytes(),
                    profile_id.as_bytes(),
                    producer_domain.as_bytes(),
                ],
            );
            let stamps = match previous {
                Some(file) => FileStamps {
                    first_generation: file.state.stamps.first_generation,
                    path_generation: file.state.stamps.path_generation,
                    content_generation: if file.state.source_hash == source_hash {
                        file.state.stamps.content_generation
                    } else {
                        generation
                    },
                    evidence_generation: if file.state.evidence_digest == evidence_digest {
                        file.state.stamps.evidence_generation
                    } else {
                        generation
                    },
                },
                None => FileStamps {
                    first_generation: generation,
                    path_generation: generation,
                    content_generation: generation,
                    evidence_generation: generation,
                },
            };
            files.insert(
                path.clone(),
                InventoryFile {
                    continuity: vec![],
                    continuity_coverage: BTreeMap::new(),
                    state: FileState {
                        artifact_id,
                        stamps,
                        source_hash,
                        profile_id,
                        payload_digest,
                        evidence_digest,
                        root: location.root,
                        path: location.relative_path,
                    },
                    source_bytes: artifact.bytes,
                    payload_bytes: reference.object_bytes,
                    extracted_bytes: reference.extracted_bytes,
                    chunks: reference.chunks,
                    indexed_lines: if reference
                        .profile
                        .as_ref()
                        .is_some_and(|profile| profile.adapter_version == "composite-facts/1")
                        && artifact.extraction_method == "utf8"
                    {
                        std::str::from_utf8(&self.store.read_retained_source(artifact)?)?
                            .split_inclusive('\n')
                            .count()
                    } else {
                        self.store
                            .load_retained(artifact)?
                            .chunks
                            .iter()
                            .map(|chunk| chunk.end_line)
                            .max()
                            .unwrap_or(0)
                    },
                    facts: reference.facts,
                    raw_source: reference.raw_source,
                    language: artifact.language.clone(),
                    extraction_method: artifact.extraction_method.clone(),
                    parse_status: artifact.parse_status.clone(),
                },
            );
        }
        let model_objects = if projection.embedding_model.is_none() {
            None
        } else if matches!(
            reason,
            PublicationReason::Initialization
                | PublicationReason::Index
                | PublicationReason::FullReindex
        ) {
            let directory = self.data_root.join("models/local");
            let objects = airk_retrieval::model::inspect_object_plan(&directory)?;
            airk_store::model_objects::ModelObjects::new(self.store.directory()).retain(
                &directory,
                &objects,
                &reservation,
            )?;
            Some(objects)
        } else {
            // Metadata publications preserve the indexed model, even when user
            // provisioning has changed or disappeared. `old` keeps its reader
            // lease alive throughout authentication, inference and publication.
            let catalog = old
                .as_ref()
                .context("model publication requires prior catalog")?;
            ensure!(
                catalog.snapshot.embedding_model == projection.embedding_model,
                "non-index publication cannot change embedding model"
            );
            let receipt = catalog
                .receipt
                .as_ref()
                .context("metadata publication requires a receipt-authenticated prior model")?;
            if let Some(objects) = self.store.model_object_set(receipt)? {
                Some(objects)
            } else {
                // Schema-two generations retain inline assets. Bind the plan
                // to their receipt-authenticated bytes before copying anything
                // into the immutable pool; never use current provisioning.
                let components = self.store.legacy_model_components(receipt)?;
                let directory = self
                    .store
                    .directory()
                    .join("generations")
                    .join(receipt.token.revision.to_string())
                    .join("model");
                let objects = airk_retrieval::model::inspect_object_plan(&directory)?;
                ensure!(
                    objects.assets == components
                        && Some(objects.legacy_identity.as_str())
                            == projection.embedding_model.as_deref()
                        && prior
                            .and_then(|inventory| inventory.embedding_pipeline_digest)
                            .is_none_or(|pipeline| pipeline == objects.pipeline_digest),
                    "prior inline model disagrees with authenticated publication"
                );
                airk_store::model_objects::ModelObjects::new(self.store.directory()).retain(
                    &directory,
                    &objects,
                    &reservation,
                )?;
                Some(objects)
            }
        };
        let mut inventory = InventoryV2 {
            embedding_pipeline_digest: model_objects
                .as_ref()
                .map(|objects| objects.pipeline_digest),
            version: airk_core::publication::INVENTORY_VERSION,
            extraction_policy_digest: if matches!(
                reason,
                PublicationReason::Initialization
                    | PublicationReason::Index
                    | PublicationReason::FullReindex
            ) {
                Some(self.extraction_policy()?.digest)
            } else {
                prior.and_then(|inventory| inventory.extraction_policy_digest)
            },
            incarnation,
            generation,
            root: projection.root.clone(),
            collection: projection.collection.clone(),
            roots: projection.roots.clone(),
            lineage_digest: Sha256Digest::from_bytes([0; 32]),
            profiles,
            files,
            retrieval_specification: std::str::from_utf8(persistent::retrieval_specification_v3())?
                .into(),
            embedding_model: projection.embedding_model.clone(),
            extractor_version: projection.extractor_version.clone(),
            analysis: projection.analysis.clone(),
            imported_facts: projection.imported_facts.clone(),
            imported_relationships: projection.imported_relationships.clone(),
        };
        let mut facts_by_path: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for fact in &inventory.imported_facts {
            facts_by_path
                .entry(fact.citation.path.as_str())
                .or_default()
                .push(fact);
        }
        let mut relationships_by_path: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for relationship in &inventory.imported_relationships {
            relationships_by_path
                .entry(relationship.citation.path.as_str())
                .or_default()
                .push(relationship);
        }
        let domains = inventory
            .files
            .keys()
            .map(|path| {
                let facts = facts_by_path
                    .get(path.as_str())
                    .map_or(&[][..], |facts| facts.as_slice());
                let relationships = relationships_by_path
                    .get(path.as_str())
                    .map_or(&[][..], |relationships| relationships.as_slice());
                let analysis = if facts.is_empty() && relationships.is_empty() {
                    None
                } else {
                    inventory.analysis.as_ref()
                };
                Ok((
                    path.clone(),
                    digest(&serde_json::to_vec(&(analysis, facts, relationships))?),
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let by_payload: BTreeMap<_, _> = projection
            .artifacts
            .values()
            .filter_map(|artifact| {
                artifact
                    .retained
                    .as_ref()
                    .map(|reference| (reference.object.clone(), artifact))
            })
            .collect();
        let mut reconciled = crate::reconciliation::reconcile(
            prior,
            &mut inventory,
            observation,
            &domains,
            |file| {
                if file.source_bytes < 64 {
                    return Ok(false);
                }
                let artifact = by_payload
                    .get(&file.state.payload_digest.to_string())
                    .context("move payload missing")?;
                let hydrated = self.store.load_retained(artifact)?;
                let mut words = std::collections::BTreeSet::new();
                for chunk in &hydrated.chunks {
                    for word in chunk
                        .text
                        .split(|character: char| !character.is_alphanumeric())
                    {
                        if word.len() >= 2 {
                            words.insert(word.to_lowercase());
                        }
                        if words.len() >= 8 {
                            break;
                        }
                    }
                    if words.len() >= 8 {
                        break;
                    }
                }
                let boilerplate = hydrated.chunks.iter().all(|chunk| {
                    let text = chunk.text.to_lowercase();
                    text.trim().is_empty()
                        || text.contains("all rights reserved")
                        || text.contains("permission is hereby granted")
                        || text.contains("spdx-license-identifier") && text.lines().count() <= 2
                });
                Ok(words.len() >= 8 && !boilerplate)
            },
        )?;
        let prior_by_id: BTreeMap<_, _> = prior
            .into_iter()
            .flat_map(|inventory| inventory.files.iter())
            .map(|(path, file)| (file.state.artifact_id, (path.as_str(), file)))
            .collect();
        let mut continuity_bytes = 0usize;
        let mut charge_continuity = |file: &InventoryFile| -> Result<()> {
            continuity_bytes = continuity_bytes
                .checked_add(serde_json::to_vec(&file.continuity)?.len())
                .context("continuity inventory size overflow")?;
            ensure!(
                continuity_bytes <= airk_core::publication::MAX_INVENTORY_BYTES,
                "continuity inventory exceeds aggregate byte bound"
            );
            Ok(())
        };
        for (path, file) in &mut inventory.files {
            if prior.is_some_and(|inventory| inventory.version >= 3)
                && let Some((old_path, old_file)) = prior_by_id.get(&file.state.artifact_id)
                && *old_path == path
                && old_file.state.payload_digest == file.state.payload_digest
                && old_file.state.profile_id == file.state.profile_id
            {
                file.continuity = old_file.continuity.clone();
                file.continuity_coverage = old_file.continuity_coverage.clone();
                charge_continuity(file)?;
                continue;
            }
            let artifact = projection
                .artifacts
                .get(path)
                .context("continuity artifact missing")?;
            let payload = self.store.load_extraction_payload(artifact)?;
            let events = crate::continuity::assign(
                prior_by_id.get(&file.state.artifact_id).copied(),
                path,
                file,
                &payload,
                generation,
            )?;
            if reconciled.events.len().saturating_add(events.len())
                <= airk_core::changes::MAX_LINEAGE_RECORDS
            {
                reconciled.events.extend(events);
            } else {
                reconciled.complete = false;
            }
            charge_continuity(file)?;
        }
        let live_ids: std::collections::BTreeSet<_> = inventory
            .files
            .values()
            .map(|file| file.state.artifact_id)
            .collect();
        let removal_causes: BTreeMap<_, _> = reconciled
            .events
            .iter()
            .filter_map(|event| match event.subject {
                ChangeSubject::Artifact { artifact_id } if event.after.is_none() => {
                    Some((artifact_id, event.cause))
                }
                _ => None,
            })
            .collect();
        for (id, (path, file)) in &prior_by_id {
            if live_ids.contains(id) {
                continue;
            }
            let cause = removal_causes
                .get(id)
                .copied()
                .unwrap_or(ChangeCause::RemovedUnknown);
            let events = crate::continuity::removed(path, file, generation, cause);
            if reconciled.events.len().saturating_add(events.len())
                <= airk_core::changes::MAX_LINEAGE_RECORDS
            {
                reconciled.events.extend(events);
            } else {
                reconciled.complete = false;
            }
        }
        if let Some(prior) = prior {
            let mut invalidations = Vec::new();
            if prior.embedding_model != inventory.embedding_model
                || prior.embedding_pipeline_digest != inventory.embedding_pipeline_digest
            {
                invalidations.push(("retrieval/model", ChangeCause::ModelChanged));
            }
            if prior.files.iter().any(|(path, file)| {
                inventory
                    .files
                    .get(path)
                    .is_some_and(|next| next.state.profile_id != file.state.profile_id)
            }) {
                invalidations.push(("extraction/profiles", ChangeCause::ProfileChanged));
            }
            if serde_json::to_vec(&(
                &prior.analysis,
                &prior.imported_facts,
                &prior.imported_relationships,
            ))? != serde_json::to_vec(&(
                &inventory.analysis,
                &inventory.imported_facts,
                &inventory.imported_relationships,
            ))? {
                invalidations.push(("producer/assertions", ChangeCause::ProviderInvalidated));
            }
            for (domain, cause) in invalidations {
                reconciled.events.push(ChangeEvent {
                    generation,
                    event_id: reconciled.events.len() as u64 + 1,
                    subject: ChangeSubject::Global {
                        domain: domain.into(),
                    },
                    before: None,
                    after: None,
                    before_detail: None,
                    after_detail: None,
                    dimensions: vec![ChangeDimension::Evidence],
                    cause,
                    basis: ContinuityBasis::Unknown,
                });
            }
        }
        for (index, event) in reconciled.events.iter_mut().enumerate() {
            event.event_id = index as u64 + 1;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let lineage = crate::lineage::build(
            old.as_ref().and_then(|catalog| catalog.lineage.as_ref()),
            parent,
            &inventory,
            now,
            if old.is_some() && prior.is_none() {
                PublicationReason::Migration
            } else {
                reason
            },
            crate::lineage::PendingLineage {
                events: reconciled.events,
                resolvers: reconciled.resolvers,
                complete: reconciled.complete,
            },
        )?;
        inventory.lineage_digest = digest(&serde_json::to_vec(&lineage)?);
        let canonical = inventory.canonical_bytes()?;
        let token = RevisionToken {
            incarnation,
            generation,
            revision: revision_digest(persistent::retrieval_specification_v3(), &canonical),
        };
        let source = airk_store::blobs::RetainedSource {
            store: &self.store,
            snapshot: &projection,
        };
        let staging = reservation.staging_directory()?;
        let charge_reservation = reservation.clone();
        let charge: airk_retrieval::bounded::Charge =
            Arc::new(move |bytes| charge_reservation.charge(bytes));
        let mut cache_stats = airk_retrieval::model::EmbeddingCacheStats::default();
        let vector_cache = if projection.embedding_model.is_some() {
            let trusted = old
                .as_ref()
                .and_then(|catalog| catalog.receipt.as_ref())
                .map(|receipt| self.store.embedding_cache_references(receipt))
                .transpose()?
                .unwrap_or_default();
            Some(
                airk_store::vector_cache::VectorCache::new(self.store.directory(), charge.clone())?
                    .with_trusted_references(&trusted)?,
            )
        } else {
            None
        };
        let mut cache_context = airk_retrieval::model::EmbeddingCacheContext {
            cache: vector_cache
                .as_ref()
                .map(|cache| cache as &dyn airk_core::embedding::EmbeddingCache),
            bypass: reason == PublicationReason::FullReindex,
            stats: &mut cache_stats,
        };
        let receipt = persistent::prepare_streaming_v3_cached(
            self.store.directory(),
            &persistent::V2Preparation {
                snapshot: &projection,
                canonical_inventory: &canonical,
                token,
            },
            model_objects.as_ref(),
            &source,
            Some(charge),
            Some(&staging),
            Some(&mut cache_context),
            |directory| {
                if let Some(cache) = &vector_cache {
                    use std::io::Write;
                    let manifest = airk_store::vector_cache::VectorCacheManifest {
                        version: 1,
                        references: cache.references()?,
                    };
                    let mut file =
                        reservation.create_file(directory.join("embedding-cache.json"))?;
                    file.write_all(&manifest.canonical_bytes()?)?;
                    file.sync_all()?;
                }
                self.store.prepare_catalog_v2(
                    directory,
                    &inventory,
                    &lineage,
                    &token,
                    &source,
                    Some(&reservation),
                    self.store.catalog_budget(&reservation)?,
                )
            },
        )?;
        self.store
            .publish_catalog_v2(&inventory, &receipt, &expected, &reservation)?;
        // Publication is already committed. Cache disposal must neither report
        // a failed publication nor retain superseded idle leases. Active calls
        // own independent Arc pins; drop potentially large indexes outside locks.
        drop(vector_cache);
        drop((old, reservation));
        self.after_publication(writer, &quota);
        Ok((generation, cache_stats))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_retries_when_observed_receipt_was_retired() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(root.path().join("note.txt"), "Original evidence.\n")?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.index()?;
        let observed = kernel.persistent_revision()?.unwrap();
        std::fs::write(root.path().join("note.txt"), "Changed evidence.\n")?;
        kernel.index()?;
        kernel.prune_indexes(false)?;
        assert!(kernel.store.receipt(&observed.token.unwrap())?.is_none());
        let (served, _pin) = kernel.pin_revision(observed)?;
        assert_eq!(served.token, kernel.persistent_revision()?.unwrap().token);
        Ok(())
    }
}
