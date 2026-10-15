// SPDX-License-Identifier: Apache-2.0
//! Conservative payload matching; no durable key allocation or path inference.
use airk_core::{
    changes::ChangeDimension,
    continuity::{ContinuityDescriptor, ContinuityFamily, ContinuitySubject},
    identity::Sha256Digest,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Debug)]
pub struct Matches {
    /// New local subject to old local subject; only unique evidence is promoted.
    pub pairs: BTreeMap<ContinuitySubject, ContinuitySubject>,
    pub ambiguous: BTreeSet<ContinuitySubject>,
}

type Group = (
    ContinuityFamily,
    String,
    Sha256Digest,
    Sha256Digest,
    Option<ContinuitySubject>,
    Option<ContinuitySubject>,
    Sha256Digest,
);

fn fingerprint(item: &ContinuityDescriptor, pass: usize) -> Option<Sha256Digest> {
    match pass {
        0 => item.explicit_identity,
        1 => item.name,
        2 if item.family == ContinuityFamily::Code => item
            .dimensions
            .get(&ChangeDimension::CodeSignature)
            .copied(),
        3 if item.family == ContinuityFamily::Code && item.body_informative => {
            item.dimensions.get(&ChangeDimension::CodeBody).copied()
        }
        2 if item.family == ContinuityFamily::Documents => item
            .dimensions
            .get(&ChangeDimension::Statement)
            .or_else(|| item.dimensions.get(&ChangeDimension::Content))
            .copied(),
        _ => None,
    }
}

/// Call only within independently established artifact continuity. Owners must
/// match first; fingerprints never authorize cross-file or cross-profile moves.
pub fn reconcile(old: &[ContinuityDescriptor], new: &[ContinuityDescriptor]) -> Matches {
    let mut result = Matches::default();
    let mut used = BTreeSet::new();
    // Repeated written IDs are ambiguous even if their body text differs.
    let duplicate_ids = |items: &[ContinuityDescriptor]| {
        let mut counts = BTreeMap::new();
        for item in items {
            if let Some(id) = item.explicit_identity {
                *counts
                    .entry((item.family, item.category.clone(), item.profile, id))
                    .or_insert(0usize) += 1;
            }
        }
        counts
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
    };
    let mut duplicates = duplicate_ids(old);
    duplicates.extend(duplicate_ids(new));
    let blocked = |item: &ContinuityDescriptor| {
        item.explicit_identity.is_some_and(|id| {
            duplicates.contains(&(item.family, item.category.clone(), item.profile, id))
        })
    };
    result.ambiguous.extend(
        new.iter()
            .filter(|item| blocked(item))
            .map(|item| item.subject),
    );
    for _depth in 0..=128 {
        let before = result.pairs.len();
        for pass in 0..4 {
            let mut left: BTreeMap<Group, Vec<ContinuitySubject>> = BTreeMap::new();
            let mut right: BTreeMap<Group, Vec<ContinuitySubject>> = BTreeMap::new();
            for item in old
                .iter()
                .filter(|item| item.complete && !blocked(item) && !used.contains(&item.subject))
            {
                let Some(hash) = fingerprint(item, pass) else {
                    continue;
                };
                if item.owner.is_some_and(|owner| !used.contains(&owner)) {
                    continue;
                }
                let region = if matches!(item.subject, ContinuitySubject::Region(_)) {
                    None
                } else {
                    item.region.map(ContinuitySubject::Region)
                };
                if region.is_some_and(|id| !used.contains(&id)) {
                    continue;
                }
                left.entry((
                    item.family,
                    item.category.clone(),
                    item.profile,
                    item.provenance,
                    item.owner,
                    region,
                    hash,
                ))
                .or_default()
                .push(item.subject);
            }
            for item in new.iter().filter(|item| {
                item.complete && !blocked(item) && !result.pairs.contains_key(&item.subject)
            }) {
                let Some(hash) = fingerprint(item, pass) else {
                    continue;
                };
                let owner = match item.owner {
                    Some(owner) => match result.pairs.get(&owner) {
                        Some(owner) => Some(*owner),
                        None => continue,
                    },
                    None => None,
                };
                let region = if matches!(item.subject, ContinuitySubject::Region(_)) {
                    None
                } else {
                    match item.region {
                        Some(id) => match result.pairs.get(&ContinuitySubject::Region(id)) {
                            Some(old) => Some(*old),
                            None => continue,
                        },
                        None => None,
                    }
                };
                right
                    .entry((
                        item.family,
                        item.category.clone(),
                        item.profile,
                        item.provenance,
                        owner,
                        region,
                        hash,
                    ))
                    .or_default()
                    .push(item.subject);
            }
            for (group, candidates) in right {
                let Some(prior) = left.get(&group) else {
                    continue;
                };
                if prior.len() == 1 && candidates.len() == 1 {
                    used.insert(prior[0]);
                    result.pairs.insert(candidates[0], prior[0]);
                    result.ambiguous.remove(&candidates[0]);
                } else {
                    result.ambiguous.extend(candidates);
                }
            }
        }
        if result.pairs.len() == before {
            break;
        }
    }
    result
}
