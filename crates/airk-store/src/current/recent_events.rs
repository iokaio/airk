// SPDX-License-Identifier: Apache-2.0
//! Positive file/unit observations from authenticated old/new metadata only.
use super::{
    ArtifactChange, CurrentHead, CurrentScope, map,
    metadata::FileMetadataRef,
    recent::{MAX_BYTES, MAX_RECORDS, Observation},
};
use crate::FileStore;
use airk_core::{
    changes::*,
    continuity::{ContinuityFamily, ContinuityKey, ContinuityRecord},
    identity::{ArtifactId, framed_digest},
    publication::InventoryFile,
};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Pair {
    before: Option<(String, InventoryFile)>,
    after: Option<(String, InventoryFile)>,
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
fn detail(record: &ContinuityRecord) -> Result<ChangeDetail> {
    Ok(ChangeDetail {
        digests: record.descriptor.dimensions.clone(),
        location: Some(record.descriptor.location.clone()),
        region_location_digest: matches!(
            record.descriptor.location,
            airk_core::DocumentLocation::Mapped { .. }
        )
        .then(|| {
            serde_json::to_vec(&record.descriptor.location)
                .map(|bytes| framed_digest(b"recent-unit-location/1", &[&bytes]))
        })
        .transpose()?,
        provenance_digest: record.descriptor.provenance,
    })
}
fn overflow() -> Observation {
    Observation {
        events: vec![],
        file_gap: Some(CoverageReason::Overflow),
        code_gap: Some(CoverageReason::Overflow),
        document_gap: Some(CoverageReason::Overflow),
    }
}
fn gap(prior: &mut Option<CoverageReason>, next: CoverageReason) {
    if *prior != Some(CoverageReason::Unsupported) {
        *prior = Some(next);
    }
}
fn family_gap(file: &InventoryFile, family: ContinuityFamily) -> Option<CoverageReason> {
    match file.continuity_coverage.get(&family) {
        None => Some(CoverageReason::Unsupported),
        Some(value) if !value.supported => Some(CoverageReason::Unsupported),
        Some(value)
            if !value.complete
                || file.continuity.iter().any(|r| {
                    r.descriptor.family == family
                        && (!r.descriptor.complete || r.ambiguous_predecessor)
                }) =>
        {
            Some(CoverageReason::Omitted)
        }
        _ => None,
    }
}

pub(crate) fn observe(
    store: &FileStore,
    scope: &CurrentScope,
    plan: &map::MapPlan<'_>,
    base: &CurrentHead,
    changes: &[ArtifactChange],
    references: &[FileMetadataRef],
) -> Result<Observation> {
    if changes.is_empty() {
        return Ok(Observation::empty());
    }
    let generation = base
        .sequence
        .checked_add(1)
        .context("change generation overflow")?;
    let refs: BTreeMap<_, _> = references.iter().map(|r| (r.path.as_str(), r)).collect();
    ensure!(
        refs.len() == references.len(),
        "duplicate change metadata reference"
    );
    let mut pairs = BTreeMap::<ArtifactId, Pair>::new();
    let mut work = 0usize;
    let mut result = Observation {
        events: vec![],
        file_gap: None,
        code_gap: None,
        document_gap: None,
    };
    for change in changes {
        let path = change.path();
        if path.len() > MAX_CHANGE_PATH_BYTES {
            return Ok(overflow());
        }
        let old = plan.get(map::key(
            scope,
            map::MapDomain::FileMetadata,
            path.as_bytes(),
        )?)?;
        if let Some(bytes) = old {
            let reference: FileMetadataRef = serde_json::from_slice(&bytes)?;
            ensure!(
                reference.path == path && serde_json::to_vec(&reference)? == bytes,
                "invalid previous change metadata binding"
            );
            work = work
                .checked_add(reference.bytes)
                .context("change observation work overflow")?;
            if work > MAX_BYTES {
                return Ok(overflow());
            }
            let file = store.read_current_metadata(&reference)?;
            file.state.validate(base.sequence)?;
            let pair = pairs.entry(file.state.artifact_id).or_default();
            ensure!(
                pair.before.is_none(),
                "duplicate previous artifact identity in changes"
            );
            pair.before = Some((path.into(), file));
        } else if plan
            .get(map::key(scope, map::MapDomain::Artifact, path.as_bytes())?)?
            .is_some()
        {
            result.file_gap = Some(CoverageReason::Omitted);
            gap(&mut result.code_gap, CoverageReason::Omitted);
            gap(&mut result.document_gap, CoverageReason::Omitted);
        }
        if let ArtifactChange::Upsert(_) = change {
            let Some(reference) = refs.get(path) else {
                result.file_gap = Some(CoverageReason::Omitted);
                gap(&mut result.code_gap, CoverageReason::Omitted);
                gap(&mut result.document_gap, CoverageReason::Omitted);
                continue;
            };
            work = work
                .checked_add(reference.bytes)
                .context("change observation work overflow")?;
            if work > MAX_BYTES {
                return Ok(overflow());
            }
            let file = store.read_current_metadata(reference)?;
            file.state.validate(generation)?;
            let pair = pairs.entry(file.state.artifact_id).or_default();
            ensure!(
                pair.after.is_none(),
                "duplicate new artifact identity in changes"
            );
            pair.after = Some((path.into(), file));
        }
    }
    let mut bytes = 0usize;
    for (id, pair) in pairs {
        for (_, file) in [&pair.before, &pair.after].into_iter().flatten() {
            if let Some(reason) = family_gap(file, ContinuityFamily::Code) {
                gap(&mut result.code_gap, reason);
            }
            if let Some(reason) = family_gap(file, ContinuityFamily::Documents) {
                gap(&mut result.document_gap, reason);
            }
        }
        let before = pair.before.as_ref();
        let after = pair.after.as_ref();
        let mut dimensions = Vec::new();
        if before.map(|(p, f)| (p, &f.state.root, &f.state.path))
            != after.map(|(p, f)| (p, &f.state.root, &f.state.path))
        {
            dimensions.push(ChangeDimension::Path);
        }
        if before.map(|(_, f)| f.state.source_hash) != after.map(|(_, f)| f.state.source_hash) {
            dimensions.push(ChangeDimension::Content);
        }
        if before.map(|(_, f)| {
            (
                f.state.profile_id,
                f.state.evidence_digest,
                f.state.payload_digest,
            )
        }) != after.map(|(_, f)| {
            (
                f.state.profile_id,
                f.state.evidence_digest,
                f.state.payload_digest,
            )
        }) {
            dimensions.push(ChangeDimension::Evidence);
        }
        let basis = if before.is_some() && after.is_some() {
            if dimensions.contains(&ChangeDimension::Path) {
                if before.zip(after).is_some_and(|((_, a), (_, b))| {
                    a.state.source_hash == b.state.source_hash
                        && a.state.profile_id == b.state.profile_id
                }) {
                    ContinuityBasis::UniqueCompatibleContent
                } else {
                    ContinuityBasis::Unknown
                }
            } else {
                ContinuityBasis::SamePathObservation
            }
        } else {
            ContinuityBasis::Unknown
        };
        let cause = match (before, after) {
            (None, Some(_)) => ChangeCause::Added,
            (Some(_), None) => ChangeCause::RemovedUnknown,
            (Some((_, a)), Some((_, b))) if a.state.profile_id != b.state.profile_id => {
                ChangeCause::ProfileChanged
            }
            _ => ChangeCause::Modified,
        };
        if !dimensions.is_empty() {
            let event = ChangeEvent {
                generation,
                event_id: 0,
                subject: ChangeSubject::Artifact { artifact_id: id },
                before: before.map(|(p, f)| endpoint(p, f)),
                after: after.map(|(p, f)| endpoint(p, f)),
                before_detail: None,
                after_detail: None,
                dimensions,
                cause,
                basis,
            };
            if !admit(&mut result.events, &mut bytes, event)? {
                return Ok(overflow());
            }
        }
        let old_units: BTreeMap<_, _> = before
            .into_iter()
            .flat_map(|(_, f)| &f.continuity)
            .map(|r| (r.key, r))
            .collect();
        let new_units: BTreeMap<_, _> = after
            .into_iter()
            .flat_map(|(_, f)| &f.continuity)
            .map(|r| (r.key, r))
            .collect();
        let keys: BTreeSet<_> = old_units.keys().chain(new_units.keys()).copied().collect();
        for key in keys {
            let old = old_units.get(&key).copied();
            let new = new_units.get(&key).copied();
            let relocated =
                before.map(|(p, f)| (p, &f.state.root)) != after.map(|(p, f)| (p, &f.state.root));
            if old
                .zip(new)
                .is_some_and(|(a, b)| a.descriptor == b.descriptor)
                && !relocated
            {
                continue;
            }
            let mut dimensions = BTreeSet::new();
            for dimension in old
                .into_iter()
                .chain(new)
                .flat_map(|r| r.descriptor.dimensions.keys())
            {
                if old.and_then(|r| r.descriptor.dimensions.get(dimension))
                    != new.and_then(|r| r.descriptor.dimensions.get(dimension))
                {
                    dimensions.insert(*dimension);
                }
            }
            if relocated
                || old.map(|r| (&r.descriptor.location, &r.descriptor.span))
                    != new.map(|r| (&r.descriptor.location, &r.descriptor.span))
            {
                dimensions.insert(ChangeDimension::Location);
            }
            if old.map(|r| r.descriptor.provenance) != new.map(|r| r.descriptor.provenance) {
                dimensions.insert(ChangeDimension::Evidence);
            }
            let subject = match key {
                ContinuityKey::Symbol(symbol_key) => ChangeSubject::Symbol {
                    artifact_id: id,
                    symbol_key,
                },
                ContinuityKey::Document(document_key) => ChangeSubject::Document {
                    artifact_id: id,
                    document_key,
                },
            };
            let event = ChangeEvent {
                generation,
                event_id: 0,
                subject,
                before: old.and(before).map(|(p, f)| endpoint(p, f)),
                after: new.and(after).map(|(p, f)| endpoint(p, f)),
                before_detail: old.map(detail).transpose()?,
                after_detail: new.map(detail).transpose()?,
                dimensions: dimensions.into_iter().collect(),
                cause: if old.is_none() {
                    ChangeCause::Added
                } else if new.is_none() {
                    ChangeCause::RemovedUnknown
                } else {
                    ChangeCause::Modified
                },
                basis,
            };
            if !admit(&mut result.events, &mut bytes, event)? {
                return Ok(overflow());
            }
        }
    }
    Ok(result)
}

fn admit(events: &mut Vec<ChangeEvent>, bytes: &mut usize, event: ChangeEvent) -> Result<bool> {
    let size = serde_json::to_vec(&event)?.len();
    if events.len() >= MAX_RECORDS - 7 || size > MAX_BYTES.saturating_sub(*bytes) {
        return Ok(false);
    }
    *bytes += size;
    events.push(event);
    Ok(true)
}
