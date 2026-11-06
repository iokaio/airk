// SPDX-License-Identifier: Apache-2.0
//! Pure bounded partition algebra. This allocates no authority and performs no I/O.
use super::{DenyReason, DenyRule, InheritedDeny, SourcePolicy};
use crate::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{
        CatalogRevision, DenyId, IndexId, MAX_ATOMS, PathScope, ScopeAtom, SourceRootId,
    },
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitDestination {
    pub index_id: IndexId,
    pub owner_binding: Sha256Digest,
    pub collection: String,
    /// Complete bijection, including unselected roots. Canonical paths/primary
    /// identity stay unchanged; the destination owns distinct source IDs.
    pub source_ids: BTreeMap<SourceRootId, SourceRootId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitPolicyPlan {
    pub before_digest: Sha256Digest,
    pub operation_id: Incarnation,
    pub destination_binding: SplitDestination,
    /// Normalized reviewed request, in original source identities.
    pub selectors: Vec<ScopeAtom>,
    pub remainder: SourcePolicy,
    pub destination: SourcePolicy,
}

fn contains(outer: &ScopeAtom, inner: &ScopeAtom) -> bool {
    outer.source_root_id == inner.source_root_id
        && (outer == inner
            || outer.kind == PathScope::Subtree
                && outer.matches_canonical(inner.source_root_id, &inner.path))
}

fn normalized(values: &[ScopeAtom]) -> Result<Vec<ScopeAtom>> {
    ensure!(
        values.len() <= MAX_ATOMS,
        "split selector capacity exceeded"
    );
    for atom in values {
        atom.validate()?;
    }
    let unique = values.iter().cloned().collect::<BTreeSet<_>>();
    Ok(unique
        .iter()
        .filter(|atom| {
            !unique
                .iter()
                .any(|other| other != *atom && contains(other, atom))
        })
        .cloned()
        .collect())
}

fn basis(policy: &SourcePolicy) -> Result<Vec<ScopeAtom>> {
    match &policy.positive_includes {
        Some(includes) => normalized(includes),
        None => Ok(policy
            .roots
            .iter()
            .map(|root| ScopeAtom {
                source_root_id: root.id,
                kind: PathScope::Subtree,
                path: String::new(),
            })
            .collect()),
    }
}

fn denied(policy: &SourcePolicy, atom: &ScopeAtom) -> bool {
    policy
        .denies
        .iter()
        .any(|rule| contains(&rule.atom(), atom))
}

fn rule_id(
    domain: &[u8],
    operation: Incarnation,
    before: Sha256Digest,
    destination: IndexId,
    atom: &ScopeAtom,
    inherited: Option<DenyId>,
) -> Result<DenyId> {
    let encoded = serde_json::to_vec(&(destination, atom, inherited))?;
    let hash = framed_digest(domain, &[operation.as_bytes(), before.as_bytes(), &encoded]);
    Ok(DenyId::from_identity(Incarnation::from_bytes(
        hash.as_bytes()[..16].try_into()?,
    )))
}

impl SourcePolicy {
    /// Predicates are complete/disjoint under this exact previous policy. The
    /// native owner must separately qualify filesystem identity, prove actual
    /// nonempty sides under bounded discovery, reserve capacity and fence reads.
    pub fn compile_split(
        &self,
        expected: Sha256Digest,
        operation_id: Incarnation,
        selectors: &[ScopeAtom],
        target: SplitDestination,
    ) -> Result<SplitPolicyPlan> {
        let before_digest = self.identity()?;
        ensure!(expected == before_digest, "split policy review stale");
        ensure!(
            self.roots.iter().filter(|root| root.owner).count() == 1,
            "split requires one permanent primary source"
        );
        ensure!(
            target.index_id != self.index_id && target.owner_binding != self.owner_binding,
            "split destination must be independently owned"
        );
        let original_ids = self
            .roots
            .iter()
            .map(|root| root.id)
            .collect::<BTreeSet<_>>();
        ensure!(
            target.source_ids.len() == original_ids.len()
                && target.source_ids.keys().copied().collect::<BTreeSet<_>>() == original_ids
                && target
                    .source_ids
                    .values()
                    .copied()
                    .collect::<BTreeSet<_>>()
                    .len()
                    == original_ids.len()
                && target
                    .source_ids
                    .values()
                    .all(|id| !original_ids.contains(id)),
            "split source mapping must be a complete independent bijection"
        );
        let selectors = normalized(selectors)?;
        ensure!(
            !selectors.is_empty()
                && selectors
                    .iter()
                    .all(|atom| original_ids.contains(&atom.source_root_id)),
            "split selectors are empty or refer to unknown roots"
        );
        let original_basis = basis(self)?;
        let mut selected = BTreeSet::new();
        // At most 128*128 comparisons. Normalized prefix unions have no
        // overlapping members; intersection outputs need no exponential subtraction.
        for selector in &selectors {
            for allowed in &original_basis {
                let intersection = if contains(selector, allowed) {
                    Some(allowed)
                } else if contains(allowed, selector) {
                    Some(selector)
                } else {
                    None
                };
                if let Some(atom) = intersection
                    && !denied(self, atom)
                {
                    selected.insert(atom.clone());
                    ensure!(
                        selected.len() <= MAX_ATOMS,
                        "split include expansion exceeds capacity"
                    );
                }
            }
        }
        ensure!(!selected.is_empty(), "split destination predicate is empty");
        let selected = selected.into_iter().collect::<Vec<_>>();
        let mut remainder = self.clone();
        remainder.epoch = self.epoch.next()?;
        for atom in &selected {
            remainder.denies.push(DenyRule {
                source_root_id: atom.source_root_id,
                kind: atom.kind,
                path: atom.path.clone(),
                rule_id: rule_id(
                    b"airk/split-transfer/1",
                    operation_id,
                    before_digest,
                    target.index_id,
                    atom,
                    None,
                )?,
                reason: DenyReason::SplitTransfer,
                created_epoch: remainder.epoch,
                inherited_from: None,
            });
        }
        ensure!(
            original_basis.iter().any(|atom| !denied(&remainder, atom)),
            "split remainder predicate is empty"
        );
        remainder.validate()?;
        let mut destination = SourcePolicy {
            schema: self.schema.clone(),
            index_id: target.index_id,
            owner_binding: target.owner_binding,
            namespace: self.namespace.clone(),
            collection: target.collection.clone(),
            epoch: CatalogRevision::INITIAL,
            roots: self
                .roots
                .iter()
                .map(|root| {
                    let mut next = root.clone();
                    next.id = target.source_ids[&root.id];
                    next
                })
                .collect(),
            positive_includes: Some(
                selected
                    .iter()
                    .map(|atom| {
                        let mut next = atom.clone();
                        next.source_root_id = target.source_ids[&atom.source_root_id];
                        next
                    })
                    .collect(),
            ),
            denies: Vec::new(),
        };
        // Preserve every previous deny, including rules outside current positive
        // eligibility. A future reviewed relaxation must not lose inherited rules.
        for rule in &self.denies {
            let mut atom = rule.atom();
            atom.source_root_id = target.source_ids[&atom.source_root_id];
            destination.denies.push(DenyRule {
                source_root_id: atom.source_root_id,
                kind: atom.kind,
                path: atom.path.clone(),
                rule_id: rule_id(
                    b"airk/split-inherited/1",
                    operation_id,
                    before_digest,
                    target.index_id,
                    &atom,
                    Some(rule.rule_id),
                )?,
                reason: rule.reason,
                created_epoch: CatalogRevision::INITIAL,
                inherited_from: Some(InheritedDeny {
                    index_id: self.index_id,
                    rule_id: rule.rule_id,
                    epoch: self.epoch,
                }),
            });
        }
        destination.validate()?;
        Ok(SplitPolicyPlan {
            before_digest,
            operation_id,
            destination_binding: target,
            selectors,
            remainder,
            destination,
        })
    }
}

impl SplitPolicyPlan {
    /// Recompute the entire result; serialized successor fields are not proof.
    pub fn validate_against(&self, before: &SourcePolicy) -> Result<()> {
        ensure!(
            *self
                == before.compile_split(
                    self.before_digest,
                    self.operation_id,
                    &self.selectors,
                    self.destination_binding.clone()
                )?,
            "split policy plan changed"
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "source_policy_split_tests.rs"]
mod tests;
