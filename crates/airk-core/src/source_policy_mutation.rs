// SPDX-License-Identifier: Apache-2.0
//! Pure reviewed policy transitions. Native admission must separately establish
//! root identity, persist the successor and fence readers before reporting a block.
use super::{DenyReason, DenyRule, POLICY_SCHEMA, SourcePolicy, SourceRoot};
use crate::{
    identity::Sha256Digest,
    index_catalog::{CatalogRevision, DenyId, IndexDefinition, ScopeAtom, SourceImportState},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// A trim cannot change includes, source bindings or another deny. Unblocking
/// always names the exact reviewed rule; it never means clearing all exclusions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum PolicyChange {
    Trim {
        rule_id: DenyId,
        selector: ScopeAtom,
    },
    Unblock {
        rule_id: DenyId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyChangePreview {
    pub before_digest: Sha256Digest,
    pub after_digest: Sha256Digest,
    pub before_epoch: CatalogRevision,
    pub after: SourcePolicy,
    pub change: PolicyChange,
}

impl SourcePolicy {
    /// Build the first trim at epoch 1 from a native-observed empty-policy index.
    /// These metadata checks do not establish filesystem identity or authority;
    /// native admission must recheck the index/source revisions transactionally.
    pub fn initial_trim(
        index: &IndexDefinition,
        roots: Vec<SourceRoot>,
        rule_id: DenyId,
        selector: ScopeAtom,
    ) -> Result<Self> {
        index.validate()?;
        ensure!(
            index.policy.is_none()
                && !index.tombstone
                && index.source_import == SourceImportState::Complete,
            "initial trim requires an available empty-policy index"
        );
        ensure!(
            roots.iter().filter(|root| root.owner).count() == 1
                && roots.iter().any(|root| root.owner
                    && root.canonical_path == index.backing.root
                    && root.aliases.is_empty()),
            "initial trim primary source binding changed"
        );
        let policy = Self {
            schema: POLICY_SCHEMA.into(),
            index_id: index.id,
            owner_binding: index.backing.binding,
            namespace: "collections-1.4".into(),
            collection: index.backing.collection.clone(),
            epoch: CatalogRevision::INITIAL,
            roots,
            positive_includes: None,
            denies: vec![DenyRule {
                source_root_id: selector.source_root_id,
                kind: selector.kind,
                path: selector.path,
                rule_id,
                reason: DenyReason::Trim,
                created_epoch: CatalogRevision::INITIAL,
                inherited_from: None,
            }],
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Constructs no authority and performs no I/O. The native owner must compare
    /// the digest again in its admission transaction; replay uses its exact saved
    /// receipt rather than applying the same change to another policy revision.
    pub fn preview_change(
        &self,
        expected_digest: Sha256Digest,
        change: PolicyChange,
    ) -> Result<PolicyChangePreview> {
        let before_digest = self.identity()?;
        ensure!(
            before_digest == expected_digest,
            "source policy review is stale"
        );
        let mut after = self.clone();
        after.epoch = self.epoch.next()?;
        match &change {
            PolicyChange::Trim { rule_id, selector } => {
                selector.validate()?;
                ensure!(
                    self.roots
                        .iter()
                        .any(|root| root.id == selector.source_root_id),
                    "trim refers to an unknown source root"
                );
                ensure!(
                    !self
                        .denies
                        .iter()
                        .any(|rule| rule.rule_id == *rule_id || rule.atom() == *selector),
                    "trim rule already exists"
                );
                after.denies.push(DenyRule {
                    source_root_id: selector.source_root_id,
                    kind: selector.kind,
                    path: selector.path.clone(),
                    rule_id: *rule_id,
                    reason: DenyReason::Trim,
                    created_epoch: after.epoch,
                    inherited_from: None,
                });
            }
            PolicyChange::Unblock { rule_id } => {
                let position = self
                    .denies
                    .iter()
                    .position(|rule| rule.rule_id == *rule_id)
                    .ok_or_else(|| anyhow::anyhow!("unblock rule is unavailable"))?;
                ensure!(
                    self.denies[position].reason == DenyReason::Trim,
                    "split transfer fences require split recovery"
                );
                after.denies.remove(position);
            }
        }
        let after_digest = after.identity()?;
        Ok(PolicyChangePreview {
            before_digest,
            after_digest,
            before_epoch: self.epoch,
            after,
            change,
        })
    }
}

impl PolicyChangePreview {
    /// Recompute rather than trusting serialized successor fields. This is pure
    /// validation only and cannot substitute for the native live revision fence.
    pub fn validate_against(&self, current: &SourcePolicy) -> Result<()> {
        ensure!(
            *self == current.preview_change(self.before_digest, self.change.clone())?,
            "source policy preview changed"
        );
        Ok(())
    }
}
