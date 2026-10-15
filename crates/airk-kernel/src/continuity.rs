// SPDX-License-Identifier: Apache-2.0
//! Durable keys are assigned only inside independently established file identity.
use airk_core::{
    ArtifactSource,
    changes::{
        ChangeCause, ChangeDetail, ChangeDimension, ChangeEndpoint, ChangeEvent, ChangeSubject,
        ContinuityBasis,
    },
    continuity::{ContinuityDescriptor, ContinuityFamily, ContinuityKey, ContinuityRecord},
    extraction::ExtractionPayload,
    identity::{DocumentKey, SymbolKey},
    publication::InventoryFile,
};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

impl crate::Kernel {
    /// Read one assigned handle from a single pinned publication. Missing keys
    /// return no facts; only retained change observations can establish removal.
    pub fn continuity_facts(
        &self,
        key: ContinuityKey,
        limit: usize,
        max_bytes: usize,
    ) -> Result<airk_core::FactsResponse> {
        crate::validate_budget(limit, max_bytes)?;
        let catalog = self.catalog()?;
        let mut response = airk_core::FactsResponse {
            publication: Self::catalog_envelope(&catalog),
            generation: catalog.snapshot.generation,
            collection: catalog.snapshot.collection.clone(),
            facts: vec![],
            truncated: false,
        };
        let Some((path, record)) = catalog.continuity_by_key(key)? else {
            return Ok(response);
        };
        let artifact = catalog.load_artifact(&path)?;
        let records = catalog
            .inventory
            .as_ref()
            .and_then(|inventory| inventory.files.get(&path))
            .map_or(&[][..], |file| file.continuity.as_slice());
        let index = airk_core::continuity::FactKeyIndex::new(records);
        let mut remaining = max_bytes;
        for (ordinal, fact) in artifact.facts.iter().enumerate() {
            if index.key(ordinal, fact) != Some(key) {
                continue;
            }
            let hit = airk_core::FactHit {
                continuity_key: Some(key),
                name: fact.name.clone(),
                kind: fact.kind.clone(),
                syntax_kind: fact.syntax_kind.clone(),
                language: artifact.language.clone(),
                code: fact.code.clone(),
                document: fact.document.clone(),
                citation: airk_core::Citation {
                    path: path.clone(),
                    content_hash: artifact.content_hash.clone(),
                    start_line: fact.start_line,
                    end_line: fact.end_line,
                    location: fact.location.clone(),
                },
            };
            let bytes = serde_json::to_vec(&hit)?.len();
            if response.facts.len() >= limit || bytes > remaining {
                response.truncated = true;
                break;
            }
            remaining -= bytes;
            response.facts.push(hit);
        }
        if response.facts.is_empty() && !response.truncated {
            let entry = &catalog.snapshot.artifacts[&path];
            let payload = self.store.load_extraction_payload(entry)?;
            // Existing native descriptors are host-local. Future composite
            // units require an original-coordinate binding before this read.
            anyhow::ensure!(
                record.descriptor.region.is_none(),
                "region content requires qualified original mapping"
            );
            let span = record.descriptor.span;
            let text = payload
                .text
                .get(span.start..span.end)
                .ok_or_else(|| anyhow::anyhow!("continuity span is outside retained text"))?;
            let start_line = payload.text[..span.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            let end_line = start_line
                + text
                    .trim_end_matches('\n')
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count();
            let hit = airk_core::FactHit {
                continuity_key: Some(key),
                name: text.into(),
                kind: record.descriptor.category,
                syntax_kind: "continuity_unit".into(),
                language: artifact.language,
                code: None,
                document: None,
                citation: airk_core::Citation {
                    path,
                    content_hash: artifact.content_hash,
                    start_line,
                    end_line,
                    location: record.descriptor.location,
                },
            };
            if serde_json::to_vec(&hit)?.len() <= remaining {
                response.facts.push(hit);
            } else {
                response.truncated = true;
            }
        }
        Ok(response)
    }
}

fn allocate(family: ContinuityFamily) -> Result<ContinuityKey> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("allocate continuity key: {error}"))?;
    Ok(match family {
        ContinuityFamily::Code => ContinuityKey::Symbol(SymbolKey::from_bytes(bytes)),
        ContinuityFamily::Documents => ContinuityKey::Document(DocumentKey::from_bytes(bytes)),
    })
}

fn endpoint(path: &str, file: &InventoryFile) -> ChangeEndpoint {
    ChangeEndpoint {
        citation_path: Some(path.into()),
        root: file.state.root.clone(),
        path: file.state.path.clone(),
        source_hash: file.state.source_hash,
        profile_id: file.state.profile_id,
    }
}

fn detail(item: &ContinuityDescriptor) -> ChangeDetail {
    ChangeDetail {
        digests: item.dimensions.clone(),
        location: Some(item.location.clone()),
        region_location_digest: None,
        provenance_digest: item.provenance,
    }
}

pub(crate) fn removed(
    path: &str,
    file: &InventoryFile,
    generation: u64,
    cause: ChangeCause,
) -> Vec<ChangeEvent> {
    file.continuity
        .iter()
        .map(|record| ChangeEvent {
            generation,
            event_id: 0,
            subject: match record.key {
                ContinuityKey::Symbol(symbol_key) => ChangeSubject::Symbol {
                    artifact_id: file.state.artifact_id,
                    symbol_key,
                },
                ContinuityKey::Document(document_key) => ChangeSubject::Document {
                    artifact_id: file.state.artifact_id,
                    document_key,
                },
            },
            before: Some(endpoint(path, file)),
            after: None,
            before_detail: Some(detail(&record.descriptor)),
            after_detail: None,
            dimensions: vec![],
            cause,
            basis: ContinuityBasis::Unknown,
        })
        .collect()
}

/// Returns bounded observations; the lineage assembler applies its aggregate cap.
pub(crate) fn assign(
    prior: Option<(&str, &InventoryFile)>,
    path: &str,
    file: &mut InventoryFile,
    payload: &ExtractionPayload,
    generation: u64,
) -> Result<Vec<ChangeEvent>> {
    let prior = prior.filter(|(_, old)| old.state.artifact_id == file.state.artifact_id);
    let old_records = prior.map_or(&[][..], |(_, old)| old.continuity.as_slice());
    let old_by_subject: BTreeMap<_, _> = old_records
        .iter()
        .map(|record| (record.descriptor.subject, record))
        .collect();
    let old_descriptors = old_records
        .iter()
        .map(|record| record.descriptor.clone())
        .collect::<Vec<_>>();
    let mut matches =
        airk_ingest::continuity::matching::reconcile(&old_descriptors, &payload.continuity);
    // Identical authenticated payloads preserve local identities even where the
    // parser cannot establish continuity across changed payloads.
    if prior.is_some_and(|(_, old)| {
        old.state.payload_digest == file.state.payload_digest
            && old.state.profile_id == file.state.profile_id
    }) {
        for descriptor in &payload.continuity {
            if old_by_subject
                .get(&descriptor.subject)
                .is_some_and(|old| old.descriptor == *descriptor)
            {
                matches.pairs.insert(descriptor.subject, descriptor.subject);
            }
        }
    }
    let mut used = BTreeSet::new();
    let mut events = Vec::new();
    let mut records = Vec::with_capacity(payload.continuity.len());
    for descriptor in &payload.continuity {
        let old = matches
            .pairs
            .get(&descriptor.subject)
            .and_then(|subject| old_by_subject.get(subject))
            .copied();
        let key = match old {
            Some(old) => {
                used.insert(old.key);
                old.key
            }
            None => allocate(descriptor.family)?,
        };
        let mut changed = BTreeMap::new();
        let mut dimensions = Vec::new();
        for (dimension, digest) in &descriptor.dimensions {
            let unchanged =
                old.is_some_and(|old| old.descriptor.dimensions.get(dimension) == Some(digest));
            changed.insert(
                *dimension,
                if unchanged {
                    old.and_then(|old| old.changed.get(dimension))
                        .copied()
                        .unwrap_or(generation)
                } else {
                    generation
                },
            );
            if !unchanged {
                dimensions.push(*dimension);
            }
        }
        let relocated = old.is_none_or(|old| {
            old.descriptor.location != descriptor.location || old.descriptor.span != descriptor.span
        }) || prior.is_some_and(|(old_path, _)| old_path != path);
        changed.insert(
            ChangeDimension::Location,
            if relocated {
                generation
            } else {
                old.and_then(|old| old.changed.get(&ChangeDimension::Location))
                    .copied()
                    .unwrap_or(generation)
            },
        );
        if relocated && !dimensions.contains(&ChangeDimension::Location) {
            dimensions.push(ChangeDimension::Location);
        }
        if old.is_none() || !dimensions.is_empty() {
            let subject = match key {
                ContinuityKey::Symbol(symbol_key) => ChangeSubject::Symbol {
                    artifact_id: file.state.artifact_id,
                    symbol_key,
                },
                ContinuityKey::Document(document_key) => ChangeSubject::Document {
                    artifact_id: file.state.artifact_id,
                    document_key,
                },
            };
            events.push(ChangeEvent {
                generation,
                event_id: 0,
                subject,
                before: old.and(prior).map(|(path, file)| endpoint(path, file)),
                after: Some(endpoint(path, file)),
                before_detail: old.map(|old| detail(&old.descriptor)),
                after_detail: Some(detail(descriptor)),
                dimensions,
                cause: if old.is_some() {
                    ChangeCause::Modified
                } else {
                    ChangeCause::Added
                },
                basis: if prior.is_some_and(|(old_path, _)| old_path != path) {
                    ContinuityBasis::UniqueCompatibleContent
                } else {
                    ContinuityBasis::SamePathObservation
                },
            });
        }
        records.push(ContinuityRecord {
            ambiguous_predecessor: old.is_none() && matches.ambiguous.contains(&descriptor.subject),
            key,
            descriptor: descriptor.clone(),
            first_generation: old.map_or(generation, |old| old.first_generation),
            changed,
        });
    }
    if let Some((old_path, old_file)) = prior {
        for old in old_records.iter().filter(|old| !used.contains(&old.key)) {
            let subject = match old.key {
                ContinuityKey::Symbol(symbol_key) => ChangeSubject::Symbol {
                    artifact_id: file.state.artifact_id,
                    symbol_key,
                },
                ContinuityKey::Document(document_key) => ChangeSubject::Document {
                    artifact_id: file.state.artifact_id,
                    document_key,
                },
            };
            events.push(ChangeEvent {
                generation,
                event_id: 0,
                subject,
                before: Some(endpoint(old_path, old_file)),
                after: None,
                before_detail: Some(detail(&old.descriptor)),
                after_detail: None,
                dimensions: vec![],
                cause: ChangeCause::RemovedUnknown,
                basis: ContinuityBasis::Unknown,
            });
        }
    }
    file.continuity = records;
    file.continuity_coverage = payload.continuity_coverage.clone();
    for record in &file.continuity {
        if record.ambiguous_predecessor
            && let Some(coverage) = file.continuity_coverage.get_mut(&record.descriptor.family)
        {
            coverage.complete = false;
        }
    }
    Ok(events)
}
