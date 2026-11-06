// SPDX-License-Identifier: Apache-2.0
//! Policy format and canonical predicates; native adapters enforce reviewed authority.
use crate::{
    identity::{Sha256Digest, framed_digest},
    index_catalog::{
        CatalogRevision, DenyId, IndexId, MAX_ATOMS, MAX_SOURCE_ROOTS, ScopeAtom, SourceRootId,
        atoms, unique_bounded,
    },
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const POLICY_SCHEMA: &str = "airk.source-policy/1";
pub const MAX_POLICY_BYTES: usize = 1024 * 1024;

#[path = "source_policy_mutation.rs"]
mod mutation;
pub use mutation::{PolicyChange, PolicyChangePreview};

#[path = "source_policy_split.rs"]
mod split;
pub use split::{SplitDestination, SplitPolicyPlan};

#[cfg(test)]
#[path = "source_policy_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRoot {
    pub id: SourceRootId,
    /// The native adapter establishes canonical identity; this field is not proof.
    pub canonical_path: String,
    pub aliases: Vec<String>,
    pub owner: bool,
}
impl SourceRoot {
    pub fn validate(&self) -> Result<()> {
        validate_root_path(&self.canonical_path)?;
        unique_bounded(&self.aliases, 16)?;
        for alias in &self.aliases {
            ensure!(
                !alias.is_empty() && alias.len() <= 128 && !alias.chars().any(char::is_control),
                "invalid source alias"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyReason {
    Trim,
    SplitTransfer,
}

/// Syntax only. The native adapter establishes actual canonical host identity.
pub fn validate_root_path(path: &str) -> Result<()> {
    let bytes = path.as_bytes();
    let absolute = path.starts_with('/')
        || path.starts_with("\\\\")
        || bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\');
    ensure!(
        !path.is_empty() && path.len() <= 32768 && !path.contains('\0') && absolute,
        "invalid canonical source root"
    );
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InheritedDeny {
    pub index_id: IndexId,
    pub rule_id: DenyId,
    pub epoch: CatalogRevision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DenyRule {
    pub source_root_id: SourceRootId,
    pub kind: crate::index_catalog::PathScope,
    pub path: String,
    pub rule_id: DenyId,
    pub reason: DenyReason,
    pub created_epoch: CatalogRevision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherited_from: Option<InheritedDeny>,
}
impl DenyRule {
    pub fn atom(&self) -> ScopeAtom {
        ScopeAtom {
            source_root_id: self.source_root_id,
            kind: self.kind,
            path: self.path.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePolicy {
    pub schema: String,
    pub index_id: IndexId,
    pub owner_binding: Sha256Digest,
    pub namespace: String,
    pub collection: String,
    pub epoch: CatalogRevision,
    pub roots: Vec<SourceRoot>,
    /// None means configured roots; Some([]) means no eligible sources.
    pub positive_includes: Option<Vec<ScopeAtom>>,
    pub denies: Vec<DenyRule>,
}
impl SourcePolicy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == POLICY_SCHEMA && self.namespace == "collections-1.4",
            "unsupported source policy format"
        );
        ensure!(
            !self.collection.is_empty()
                && self.collection.len() <= 128
                && !matches!(self.collection.as_str(), "." | "..")
                && self
                    .collection
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "invalid policy collection"
        );
        ensure!(
            !self.roots.is_empty() && self.roots.len() <= MAX_SOURCE_ROOTS,
            "invalid policy root capacity"
        );
        ensure!(
            self.denies.len() <= MAX_ATOMS,
            "policy deny capacity exceeded"
        );
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        for root in &self.roots {
            root.validate()?;
            ensure!(
                ids.insert(root.id) && paths.insert(&root.canonical_path),
                "duplicate policy source identity"
            );
            for alias in &root.aliases {
                ensure!(aliases.insert(alias), "ambiguous policy source alias");
            }
        }
        if let Some(includes) = &self.positive_includes {
            atoms(includes)?;
            ensure!(
                includes
                    .iter()
                    .all(|atom| ids.contains(&atom.source_root_id)),
                "policy include refers to unknown root"
            );
        }
        let mut denies = BTreeSet::new();
        let mut predicates = BTreeSet::new();
        for deny in &self.denies {
            let atom = deny.atom();
            atom.validate()?;
            ensure!(
                ids.contains(&deny.source_root_id)
                    && denies.insert(deny.rule_id)
                    && predicates.insert(atom),
                "invalid or duplicate policy deny"
            );
            ensure!(
                deny.created_epoch <= self.epoch,
                "policy deny belongs to a future epoch"
            );
            if let Some(inherited) = &deny.inherited_from {
                ensure!(
                    inherited.index_id != self.index_id,
                    "policy deny cannot inherit from itself"
                );
            }
        }
        ensure!(
            crate::index_catalog::encoded_size(self, MAX_POLICY_BYTES)? <= MAX_POLICY_BYTES,
            "source policy exceeds byte budget"
        );
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/source-policy/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
    /// Pure predicate only. Capture/read/delivery adapters must separately bind
    /// roots and revision fences before this can confer any native authority.
    pub fn allows_canonical(&self, root: SourceRootId, path: &str) -> bool {
        if crate::index_catalog::relative_path(path, false).is_err()
            || !self.roots.iter().any(|r| r.id == root)
        {
            return false;
        }
        let included = self
            .positive_includes
            .as_ref()
            .is_none_or(|rules| rules.iter().any(|rule| rule.matches_canonical(root, path)));
        included
            && !self.denies.iter().any(|rule| {
                rule.source_root_id == root
                    && match rule.kind {
                        crate::index_catalog::PathScope::Exact => rule.path == path,
                        crate::index_catalog::PathScope::Subtree => {
                            rule.path.is_empty()
                                || rule.path == path
                                || path
                                    .strip_prefix(&rule.path)
                                    .is_some_and(|tail| tail.starts_with('/'))
                        }
                    }
            })
    }
}
