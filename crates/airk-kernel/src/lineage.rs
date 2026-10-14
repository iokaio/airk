// SPDX-License-Identifier: Apache-2.0
//! Immutable bounded history assembled only at successful publication.
use airk_core::{changes::*, identity::RevisionToken, publication::InventoryV2};
use anyhow::Result;
use std::collections::BTreeMap;

pub(crate) struct PendingLineage {
    pub events: Vec<ChangeEvent>,
    pub resolvers: Vec<ResolverRecord>,
    pub complete: bool,
}

pub(crate) fn build(
    previous: Option<&LineageEnvelope>,
    parent: Option<RevisionToken>,
    inventory: &InventoryV2,
    now: u64,
    reason: PublicationReason,
    pending: PendingLineage,
) -> Result<LineageEnvelope> {
    let PendingLineage {
        events: mut new_events,
        resolvers: new_resolvers,
        complete,
    } = pending;
    let generation = inventory.generation;
    let initial = CoverageFloor {
        complete_after: generation,
        reason: Some(CoverageReason::BaselineUnavailable),
    };
    let mut lineage = LineageEnvelope {
        version: LINEAGE_CONTRACT_VERSION,
        incarnation: inventory.incarnation,
        observation: PublicationObservation {
            generation,
            parent,
            observed_unix_seconds: now,
            reason,
        },
        coverage: previous
            .map(|p| p.coverage.clone())
            .unwrap_or(ChangeCoverage {
                files: initial.clone(),
                code: initial.clone(),
                documents: initial.clone(),
                ancestry: initial.clone(),
                resolution: initial,
            }),
        ancestry: previous.map(|p| p.ancestry.clone()).unwrap_or_default(),
        events: previous.map(|p| p.events.clone()).unwrap_or_default(),
        resolvers: previous.map(|p| p.resolvers.clone()).unwrap_or_default(),
    };
    lineage.coverage.code = CoverageFloor {
        complete_after: generation,
        reason: Some(CoverageReason::Unsupported),
    };
    lineage.coverage.documents = lineage.coverage.code.clone();
    if let (Some(previous), Some(parent)) = (previous, parent) {
        lineage.ancestry.push(RetainedPublicationHeader {
            token: parent,
            parent: previous.observation.parent,
            observed_unix_seconds: previous.observation.observed_unix_seconds,
            reason: previous.observation.reason,
        });
    }
    if !complete {
        lineage.coverage.files = CoverageFloor {
            complete_after: generation,
            reason: Some(CoverageReason::Overflow),
        };
        lineage.coverage.resolution = lineage.coverage.files.clone();
        new_events.retain(|event| matches!(event.subject, ChangeSubject::Global { .. }));
        lineage.resolvers.clear();
    } else {
        lineage.resolvers.extend(new_resolvers);
    }
    lineage.events.extend(new_events);
    let live: BTreeMap<_, _> = inventory
        .files
        .iter()
        .map(|(path, file)| (file.state.artifact_id, (path, &file.state)))
        .collect();
    let terminal: BTreeMap<_, _> = lineage
        .resolvers
        .iter()
        .filter(|r| r.state != ResolutionState::ExactMove)
        .map(|r| (r.artifact_id, (r.state, r.basis)))
        .collect();
    for resolver in &mut lineage.resolvers {
        if resolver.state != ResolutionState::ExactMove {
            continue;
        }
        let source_bound = if let Some(path) = &resolver.from.citation_path {
            airk_store::roots::source_location(&inventory.root, &inventory.roots, path).is_ok_and(
                |location| {
                    location.root == resolver.from.root
                        && location.relative_path == resolver.from.path
                },
            )
        } else {
            resolver.from.root == inventory.root
                || inventory
                    .roots
                    .values()
                    .any(|root| root == &resolver.from.root)
        };
        if !source_bound {
            resolver.to = None;
            resolver.state = ResolutionState::RemovedFromIndex;
            resolver.basis = ContinuityBasis::Unknown;
            continue;
        }
        if let Some((path, current)) = live.get(&resolver.artifact_id) {
            if current.source_hash == resolver.from.source_hash
                && current.profile_id == resolver.from.profile_id
            {
                resolver.to = Some(ChangeEndpoint {
                    citation_path: Some((*path).clone()),
                    root: current.root.clone(),
                    path: current.path.clone(),
                    source_hash: current.source_hash,
                    profile_id: current.profile_id,
                });
            } else {
                resolver.to = None;
                resolver.state = ResolutionState::Unknown;
                resolver.basis = ContinuityBasis::Unknown;
            }
        } else {
            let (state, basis) = terminal
                .get(&resolver.artifact_id)
                .copied()
                .unwrap_or((ResolutionState::Unknown, ContinuityBasis::Unknown));
            resolver.to = None;
            resolver.state = state;
            resolver.basis = basis;
        }
    }
    lineage.resolvers.retain(|r| {
        r.to.as_ref()
            .is_none_or(|to| to.root != r.from.root || to.path != r.from.path)
    });
    // Repeated observations of the same remembered endpoint need one projection.
    let mut unique = BTreeMap::new();
    for resolver in std::mem::take(&mut lineage.resolvers) {
        unique.insert(
            (
                resolver.artifact_id,
                resolver.from.root.clone(),
                resolver.from.path.clone(),
                resolver.from.source_hash,
                resolver.from.profile_id,
            ),
            resolver,
        );
    }
    lineage.resolvers = unique.into_values().collect();
    if let Some(generation) = lineage
        .ancestry
        .iter()
        .rev()
        .find(|header| header.observed_age_seconds(now) > MAX_LINEAGE_AGE_SECONDS)
        .map(|header| header.token.generation)
    {
        evict_through(&mut lineage, generation, CoverageReason::Expired);
    }
    loop {
        let count = 6 + lineage.ancestry.len() + lineage.events.len() + lineage.resolvers.len();
        if count <= MAX_LINEAGE_RECORDS && serde_json::to_vec(&lineage)?.len() <= MAX_LINEAGE_BYTES
        {
            break;
        }
        if !lineage.ancestry.is_empty() {
            // Batched whole-publication eviction bounds repeated serialization
            // work even when thousands of individually large groups are old.
            let through = lineage.ancestry[(lineage.ancestry.len() - 1) / 4]
                .token
                .generation;
            evict_through(&mut lineage, through, CoverageReason::Overflow);
        } else {
            lineage
                .events
                .retain(|event| matches!(event.subject, ChangeSubject::Global { .. }));
            lineage.resolvers.clear();
            lineage.coverage.files = CoverageFloor {
                complete_after: generation,
                reason: Some(CoverageReason::Overflow),
            };
            lineage.coverage.resolution = lineage.coverage.files.clone();
            break;
        }
    }
    lineage.validate()?;
    Ok(lineage)
}

fn evict_through(lineage: &mut LineageEnvelope, generation: u64, reason: CoverageReason) {
    lineage
        .ancestry
        .retain(|header| header.token.generation > generation);
    let events_removed = lineage
        .events
        .iter()
        .any(|event| event.generation <= generation);
    let resolvers_removed = lineage
        .resolvers
        .iter()
        .any(|resolver| resolver.observed_generation <= generation);
    lineage.events.retain(|event| event.generation > generation);
    lineage
        .resolvers
        .retain(|resolver| resolver.observed_generation > generation);
    for (floor, affected) in [
        (&mut lineage.coverage.files, events_removed),
        (&mut lineage.coverage.resolution, resolvers_removed),
        (&mut lineage.coverage.ancestry, true),
    ] {
        if affected && floor.complete_after <= generation {
            *floor = CoverageFloor {
                complete_after: generation,
                reason: Some(reason),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::identity::{ArtifactId, Incarnation, Sha256Digest};

    fn inventory(generation: u64) -> InventoryV2 {
        InventoryV2 {
            version: 2,
            incarnation: Incarnation::from_bytes([1; 16]),
            generation,
            root: "/original-fixture".into(),
            collection: "default".into(),
            roots: BTreeMap::new(),
            lineage_digest: Sha256Digest::from_bytes([0; 32]),
            profiles: BTreeMap::new(),
            files: BTreeMap::new(),
            retrieval_specification: "original-test/1".into(),
            embedding_model: None,
            extractor_version: "fixture/1".into(),
            analysis: None,
            imported_facts: vec![],
            imported_relationships: vec![],
        }
    }

    fn token(generation: u64) -> RevisionToken {
        RevisionToken {
            incarnation: Incarnation::from_bytes([1; 16]),
            generation,
            revision: Sha256Digest::from_bytes([generation as u8; 32]),
        }
    }

    fn event(generation: u64, event_id: u64) -> ChangeEvent {
        ChangeEvent {
            generation,
            event_id,
            subject: ChangeSubject::Artifact {
                artifact_id: ArtifactId::from_bytes([2; 16]),
            },
            before: None,
            after: Some(ChangeEndpoint {
                citation_path: Some("note.txt".into()),
                root: "/original-fixture".into(),
                path: "note.txt".into(),
                source_hash: Sha256Digest::from_bytes([3; 32]),
                profile_id: Sha256Digest::from_bytes([4; 32]),
            }),
            before_detail: None,
            after_detail: None,
            dimensions: vec![ChangeDimension::Content],
            cause: ChangeCause::Modified,
            basis: ContinuityBasis::SamePathObservation,
        }
    }

    #[test]
    fn age_expiry_keeps_independent_file_and_resolution_floors() -> Result<()> {
        let floor = CoverageFloor {
            complete_after: 0,
            reason: None,
        };
        let now = MAX_LINEAGE_AGE_SECONDS + 20;
        let previous = LineageEnvelope {
            version: 1,
            incarnation: token(2).incarnation,
            observation: PublicationObservation {
                generation: 2,
                parent: Some(token(1)),
                observed_unix_seconds: now - 5,
                reason: PublicationReason::Index,
            },
            coverage: ChangeCoverage {
                files: floor.clone(),
                code: floor.clone(),
                documents: floor.clone(),
                ancestry: floor.clone(),
                resolution: floor,
            },
            ancestry: vec![RetainedPublicationHeader {
                token: token(1),
                parent: None,
                observed_unix_seconds: 1,
                reason: PublicationReason::Initialization,
            }],
            events: vec![event(2, 1)],
            resolvers: vec![ResolverRecord {
                artifact_id: ArtifactId::from_bytes([5; 16]),
                observed_generation: 1,
                from: event(1, 1).after.unwrap(),
                to: None,
                state: ResolutionState::DeletedObserved,
                basis: ContinuityBasis::Unknown,
            }],
        };
        let result = build(
            Some(&previous),
            Some(token(2)),
            &inventory(3),
            now,
            PublicationReason::Index,
            PendingLineage {
                events: vec![event(3, 1)],
                resolvers: vec![],
                complete: true,
            },
        )?;
        assert_eq!(result.coverage.files.complete_after, 0);
        assert_eq!(result.coverage.resolution.complete_after, 1);
        assert_eq!(
            result.coverage.resolution.reason,
            Some(CoverageReason::Expired)
        );
        assert_eq!(result.coverage.ancestry.complete_after, 1);
        assert_eq!(
            result.coverage.code.reason,
            Some(CoverageReason::Unsupported)
        );
        assert_eq!(result.events.len(), 2);
        assert!(result.resolvers.is_empty());
        Ok(())
    }

    #[test]
    fn oversized_single_observation_preserves_global_invalidation_and_explicit_gap() -> Result<()> {
        let mut events: Vec<_> = (1..=10_001).map(|id| event(1, id)).collect();
        let mut global = event(1, 10_002);
        global.subject = ChangeSubject::Global {
            domain: "extraction/profile".into(),
        };
        global.after = None;
        events.push(global);
        let result = build(
            None,
            None,
            &inventory(1),
            1,
            PublicationReason::Index,
            PendingLineage {
                events,
                resolvers: vec![],
                complete: true,
            },
        )?;
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.coverage.files.reason, Some(CoverageReason::Overflow));
        assert_eq!(result.coverage.resolution.complete_after, 1);
        result.validate()?;
        Ok(())
    }

    #[test]
    fn renamed_alias_cannot_preserve_an_old_forward_even_with_same_physical_root() -> Result<()> {
        let mut first = inventory(1);
        first.roots.insert("docs".into(), "/original-docs".into());
        let mut from = event(1, 1).after.unwrap();
        from.root = "/original-docs".into();
        from.citation_path = Some("@docs/note.txt".into());
        let resolver = ResolverRecord {
            artifact_id: ArtifactId::from_bytes([2; 16]),
            observed_generation: 1,
            from,
            to: Some(ChangeEndpoint {
                citation_path: Some("moved.txt".into()),
                path: "moved.txt".into(),
                ..event(1, 1).after.unwrap()
            }),
            state: ResolutionState::ExactMove,
            basis: ContinuityBasis::UniqueCompatibleContent,
        };
        // The prior committed envelope has already proven this exact move.
        let mut previous = build(
            None,
            None,
            &first,
            1,
            PublicationReason::Index,
            PendingLineage {
                events: vec![],
                resolvers: vec![],
                complete: true,
            },
        )?;
        previous.resolvers.push(resolver);
        previous.validate()?;
        let mut next = inventory(2);
        next.roots.insert("renamed".into(), "/original-docs".into());
        let result = build(
            Some(&previous),
            Some(token(1)),
            &next,
            2,
            PublicationReason::Index,
            PendingLineage {
                events: vec![],
                resolvers: vec![],
                complete: true,
            },
        )?;
        assert_eq!(result.resolvers[0].state, ResolutionState::RemovedFromIndex);
        assert!(result.resolvers[0].to.is_none());
        Ok(())
    }
}
