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
        let payload =
            airk_ingest::adapters::legacy::convert_native_legacy(&hydrated, bytes, &profile)?;
        self.store.retain_payload_with_budget(
            &hydrated,
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
                    indexed_lines: self
                        .store
                        .load_retained(artifact)?
                        .chunks
                        .last()
                        .map_or(0, |chunk| chunk.end_line),
                    facts: reference.facts,
                    raw_source: reference.raw_source,
                    language: artifact.language.clone(),
                    extraction_method: artifact.extraction_method.clone(),
                    parse_status: artifact.parse_status.clone(),
                },
            );
        }
        let mut inventory = InventoryV2 {
            version: 2,
            incarnation,
            generation,
            root: projection.root.clone(),
            collection: projection.collection.clone(),
            roots: projection.roots.clone(),
            lineage_digest: Sha256Digest::from_bytes([0; 32]),
            profiles,
            files,
            retrieval_specification: std::str::from_utf8(persistent::retrieval_specification_v2())?
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
        if let Some(prior) = prior {
            let mut invalidations = Vec::new();
            if prior.embedding_model != inventory.embedding_model {
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
            revision: revision_digest(persistent::retrieval_specification_v2(), &canonical),
        };
        let model = self.data_root.join("models/local");
        let source = airk_store::blobs::RetainedSource {
            store: &self.store,
            snapshot: &projection,
        };
        let staging = reservation.staging_directory()?;
        let charge_reservation = reservation.clone();
        let charge: airk_retrieval::bounded::Charge =
            Arc::new(move |bytes| charge_reservation.charge(bytes));
        let receipt = persistent::prepare_streaming_v2(
            self.store.directory(),
            &persistent::V2Preparation {
                snapshot: &projection,
                canonical_inventory: &canonical,
                token,
            },
            model.exists().then_some(model.as_path()),
            &source,
            Some(charge),
            Some(&staging),
            |directory| {
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
        drop((old, reservation));
        self.after_publication(writer, &quota);
        Ok(generation)
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
