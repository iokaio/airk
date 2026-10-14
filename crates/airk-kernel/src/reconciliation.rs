// SPDX-License-Identifier: Apache-2.0
//! Conservative file continuity, independent from extraction cache reuse.
use airk_core::{
    changes::{
        ChangeCause, ChangeDimension, ChangeEndpoint, ChangeEvent, ChangeSubject, ContinuityBasis,
        FileStamps, ResolutionState, ResolverRecord,
    },
    identity::{Sha256Digest, framed_digest},
    publication::{InventoryFile, InventoryV2},
};
use airk_ingest::observations::{DisappearanceCause, ScanObservation};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

const MAX_RECORDS: usize = 8000;

pub(crate) struct ReconciledFiles {
    pub events: Vec<ChangeEvent>,
    /// New projection records only. Publication owns carry/compression/expiry.
    pub resolvers: Vec<ResolverRecord>,
    /// False means publication must advance file/resolution completeness floors.
    pub complete: bool,
    encoded_bytes: usize,
}

type ContentKey = (Sha256Digest, Sha256Digest, usize);

fn key(file: &InventoryFile) -> ContentKey {
    (
        file.state.source_hash,
        file.state.profile_id,
        file.source_bytes,
    )
}

fn endpoint(file: &InventoryFile, citation_path: &str) -> ChangeEndpoint {
    ChangeEndpoint {
        citation_path: Some(citation_path.into()),
        root: file.state.root.clone(),
        path: file.state.path.clone(),
        source_hash: file.state.source_hash,
        profile_id: file.state.profile_id,
    }
}

/// A count and one representative avoid any pairwise candidate expansion.
fn groups(inventory: &InventoryV2) -> BTreeMap<ContentKey, (usize, &str)> {
    let mut groups = BTreeMap::new();
    for (path, file) in &inventory.files {
        let group = groups.entry(key(file)).or_insert((0, path.as_str()));
        group.0 += 1;
    }
    groups
}

fn stable_binding(path: &str, old: &InventoryV2, next: &InventoryV2) -> bool {
    if let Some((alias, _)) = path.strip_prefix('@').and_then(|path| path.split_once('/')) {
        old.roots
            .get(alias)
            .is_some_and(|root| next.roots.get(alias) == Some(root))
    } else {
        old.root == next.root
    }
}

fn observed_cause(path: &str, observation: Option<&ScanObservation>) -> ChangeCause {
    match observation.and_then(|observation| observation.disappeared.get(path)) {
        Some(DisappearanceCause::DeletedObserved) => ChangeCause::DeletedObserved,
        Some(DisappearanceCause::AliasRemoved) => ChangeCause::AliasRemoved,
        Some(DisappearanceCause::AliasRebound) => ChangeCause::AliasRebound,
        Some(DisappearanceCause::ExcludedFromDiscovery) => ChangeCause::Ignored,
        Some(DisappearanceCause::Oversized) => ChangeCause::Oversize,
        Some(DisappearanceCause::Unsupported) => ChangeCause::Unsupported,
        Some(DisappearanceCause::ExtractionFailed) => ChangeCause::ExtractionOmitted,
        _ => ChangeCause::RemovedUnknown,
    }
}

impl ReconciledFiles {
    fn push(&mut self, mut event: ChangeEvent, resolver: Option<ResolverRecord>) -> Result<()> {
        if self.events.len() + self.resolvers.len() + 1 + usize::from(resolver.is_some())
            > MAX_RECORDS
        {
            self.complete = false;
            return Ok(());
        }
        event.event_id = self.events.len() as u64 + 1;
        let bytes = serde_json::to_vec(&event)?.len()
            + resolver
                .as_ref()
                .map(serde_json::to_vec)
                .transpose()?
                .map_or(0, |bytes| bytes.len());
        // Leave four MiB for framing, ancestry, invalidations and coverage.
        if self.encoded_bytes.saturating_add(bytes) > 12 * 1024 * 1024 {
            self.complete = false;
            return Ok(());
        }
        self.encoded_bytes += bytes;
        self.events.push(event);
        if let Some(resolver) = resolver {
            self.resolvers.push(resolver);
        }
        Ok(())
    }
}

fn event(
    generation: u64,
    file: &InventoryFile,
    before: Option<(&InventoryFile, &str)>,
    after: Option<(&InventoryFile, &str)>,
    dimensions: Vec<ChangeDimension>,
    cause: ChangeCause,
    basis: ContinuityBasis,
) -> ChangeEvent {
    ChangeEvent {
        generation,
        event_id: 0,
        subject: ChangeSubject::Artifact {
            artifact_id: file.state.artifact_id,
        },
        before: before.map(|(file, path)| endpoint(file, path)),
        after: after.map(|(file, path)| endpoint(file, path)),
        before_detail: None,
        after_detail: None,
        dimensions,
        cause,
        basis,
    }
}

/// Only an exact, unique, informative, compatible content pair in a complete
/// scan can transfer a handle. It remains a candidate, never filesystem intent.
pub(crate) fn reconcile(
    previous: Option<&InventoryV2>,
    next: &mut InventoryV2,
    observation: Option<&ScanObservation>,
    producer_domains: &BTreeMap<String, Sha256Digest>,
    mut informative: impl FnMut(&InventoryFile) -> Result<bool>,
) -> Result<ReconciledFiles> {
    ensure!(
        observation.is_none_or(|observation| observation.complete),
        "incomplete scan cannot publish file reconciliation"
    );
    if let Some(previous) = previous {
        ensure!(
            previous.incarnation == next.incarnation
                && previous.collection == next.collection
                && previous.root == next.root
                && previous.generation < next.generation,
            "foreign or non-forward file reconciliation"
        );
    }
    if let Some(observation) = observation {
        ensure!(
            observation.roots.get("") == Some(&next.root)
                && observation.roots.len() == next.roots.len() + 1
                && next
                    .roots
                    .iter()
                    .all(|(alias, root)| observation.roots.get(alias) == Some(root)),
            "scan root bindings differ from publication"
        );
    }
    let mut result = ReconciledFiles {
        events: vec![],
        resolvers: vec![],
        complete: true,
        encoded_bytes: 0,
    };
    let old_groups = previous.map(groups).unwrap_or_default();
    let new_groups = groups(next)
        .into_iter()
        .map(|(key, (count, path))| (key, (count, path.to_owned())))
        .collect::<BTreeMap<_, _>>();
    let mut moves = BTreeMap::<String, String>::new();
    if let (Some(old), Some(_)) = (previous, observation) {
        for (path, before) in &old.files {
            if next
                .files
                .get(path)
                .is_some_and(|after| after.state.root == before.state.root)
                || observed_cause(path, observation) != ChangeCause::DeletedObserved
                || !stable_binding(path, old, next)
            {
                continue;
            }
            let content_key = key(before);
            let Some((1, destination)) = new_groups.get(&content_key) else {
                continue;
            };
            if old_groups[&content_key].0 != 1
                || before.source_bytes < 32
                || old.files.contains_key(destination)
                || !stable_binding(destination, old, next)
            {
                continue;
            }
            if informative(&next.files[destination])? {
                moves.insert(destination.clone(), path.clone());
            }
        }
    }
    let moved_sources: BTreeSet<_> = moves.values().cloned().collect();
    for (path, after) in &mut next.files {
        let producer_domain = producer_domains
            .get(path)
            .context("file producer domain missing")?;
        let same = previous
            .and_then(|old| old.files.get(path))
            .filter(|before| before.state.root == after.state.root);
        let moved = moves
            .get(path)
            .and_then(|source| previous.and_then(|old| old.files.get(source)));
        let before = same.or(moved);
        if let Some(before) = before {
            after.state.artifact_id = before.state.artifact_id;
        }
        after.state.evidence_digest = framed_digest(
            b"bound-evidence/1",
            &[
                next.incarnation.as_bytes(),
                after.state.artifact_id.as_bytes(),
                after.state.root.as_bytes(),
                after.state.path.as_bytes(),
                after.state.payload_digest.as_bytes(),
                after.state.profile_id.as_bytes(),
                producer_domain.as_bytes(),
            ],
        );
        after.state.stamps = match before {
            Some(before) => FileStamps {
                first_generation: before.state.stamps.first_generation,
                path_generation: if moved.is_some() {
                    next.generation
                } else {
                    before.state.stamps.path_generation
                },
                content_generation: if before.state.source_hash == after.state.source_hash {
                    before.state.stamps.content_generation
                } else {
                    next.generation
                },
                evidence_generation: if before.state.evidence_digest == after.state.evidence_digest
                {
                    before.state.stamps.evidence_generation
                } else {
                    next.generation
                },
            },
            None => FileStamps {
                first_generation: next.generation,
                path_generation: next.generation,
                content_generation: next.generation,
                evidence_generation: next.generation,
            },
        };
        let Some(before) = before else {
            result.push(
                event(
                    next.generation,
                    after,
                    None,
                    Some((after, path)),
                    vec![
                        ChangeDimension::Path,
                        ChangeDimension::Content,
                        ChangeDimension::Evidence,
                    ],
                    ChangeCause::Added,
                    ContinuityBasis::Unknown,
                ),
                None,
            )?;
            continue;
        };
        let mut dimensions = vec![];
        if moved.is_some() {
            dimensions.push(ChangeDimension::Path);
        }
        if before.state.source_hash != after.state.source_hash {
            dimensions.push(ChangeDimension::Content);
        }
        if before.state.evidence_digest != after.state.evidence_digest {
            dimensions.push(ChangeDimension::Evidence);
        }
        if dimensions.is_empty() {
            continue;
        }
        let cause = if before.state.profile_id != after.state.profile_id {
            ChangeCause::ProfileChanged
        } else if moved.is_none()
            && before.state.payload_digest == after.state.payload_digest
            && before.state.source_hash == after.state.source_hash
        {
            ChangeCause::ProviderInvalidated
        } else {
            ChangeCause::Modified
        };
        let basis = if moved.is_some() {
            ContinuityBasis::UniqueCompatibleContent
        } else {
            ContinuityBasis::SamePathObservation
        };
        let before_path = moves.get(path).map_or(path.as_str(), String::as_str);
        let resolver = moved.map(|_| ResolverRecord {
            artifact_id: after.state.artifact_id,
            observed_generation: next.generation,
            from: endpoint(before, before_path),
            to: Some(endpoint(after, path)),
            state: ResolutionState::ExactMove,
            basis,
        });
        result.push(
            event(
                next.generation,
                after,
                Some((before, before_path)),
                Some((after, path)),
                dimensions,
                cause,
                basis,
            ),
            resolver,
        )?;
    }
    if let Some(old) = previous {
        for (path, before) in &old.files {
            if moved_sources.contains(path)
                || next
                    .files
                    .get(path)
                    .is_some_and(|after| after.state.root == before.state.root)
            {
                continue;
            }
            let cause = observed_cause(path, observation);
            let equivalent =
                cause == ChangeCause::DeletedObserved && new_groups.contains_key(&key(before));
            let basis = if equivalent {
                ContinuityBasis::EquivalentContentCandidate
            } else {
                ContinuityBasis::Unknown
            };
            let state = if equivalent {
                ResolutionState::Ambiguous
            } else if cause == ChangeCause::DeletedObserved {
                ResolutionState::DeletedObserved
            } else {
                ResolutionState::RemovedFromIndex
            };
            let resolver = ResolverRecord {
                artifact_id: before.state.artifact_id,
                observed_generation: next.generation,
                from: endpoint(before, path),
                to: None,
                state,
                basis,
            };
            result.push(
                event(
                    next.generation,
                    before,
                    Some((before, path)),
                    None,
                    vec![ChangeDimension::Path, ChangeDimension::Evidence],
                    cause,
                    basis,
                ),
                Some(resolver),
            )?;
        }
    }
    Ok(result)
}
