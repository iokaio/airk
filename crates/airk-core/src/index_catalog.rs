// SPDX-License-Identifier: Apache-2.0
//! Inert Project/index metadata contracts. IDs and context choices grant no access.
use crate::{
    identity::{Incarnation, Sha256Digest},
    service_records::ProjectRegistration,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt, str::FromStr};

/// Latest runtime contract. Schema 13 retains schema-12 physical catalog DDL
/// and requires the in-process provider / embedded default source semantics.
pub const REGISTRY_SCHEMA: u32 = 13;
pub const MAX_DEFINITIONS: usize = 1024;
pub const MAX_SELECTION: usize = 32;
pub const MAX_ATOMS: usize = 128;
pub const MAX_SOURCE_ROOTS: usize = 17;
pub const MAX_LABEL_BYTES: usize = 256;

/// Runtime format support is independent of the application version. In
/// particular, earlier development payloads may share the same 1.9.0 label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogCapabilities {
    pub maximum_registry_schema: u32,
    pub authority_contract: u32,
    pub policy_contract: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_refresh_contract: Option<u32>,
    pub production_activation_enabled: bool,
}
impl CatalogCapabilities {
    pub fn supports(&self, schema: u32, authority: u32, policy: u32) -> bool {
        schema <= self.maximum_registry_schema
            && authority <= self.authority_contract
            && policy <= self.policy_contract
    }
}

/// Measure encoded UTF-8 without first allocating an unbounded JSON buffer.
pub fn encoded_size(value: &impl Serialize, maximum: usize) -> Result<usize> {
    struct Counter {
        bytes: usize,
        maximum: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let next = self
                .bytes
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("catalog encoded size overflow"))?;
            if next > self.maximum {
                return Err(std::io::Error::other("catalog JSON byte budget exceeded"));
            }
            self.bytes = next;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { bytes: 0, maximum };
    serde_json::to_writer(&mut counter, value)?;
    Ok(counter.bytes)
}

#[cfg(test)]
mod tests;

macro_rules! catalog_id {
    ($name:ident, $prefix:literal) => {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(Incarnation);
        impl $name {
            /// Allocation belongs to the native host; this is syntax, not authority.
            pub const fn from_identity(identity: Incarnation) -> Self {
                Self(identity)
            }
            pub const fn identity(self) -> Incarnation {
                self.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $prefix, self.0)
            }
        }
        impl FromStr for $name {
            type Err = anyhow::Error;
            fn from_str(value: &str) -> Result<Self> {
                let suffix = value
                    .strip_prefix($prefix)
                    .ok_or_else(|| anyhow::anyhow!("invalid {} prefix", stringify!($name)))?;
                Ok(Self(suffix.parse()?))
            }
        }
        impl TryFrom<String> for $name {
            type Error = anyhow::Error;
            fn try_from(value: String) -> Result<Self> {
                value.parse()
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.to_string()
            }
        }
    };
}
catalog_id!(QueryProjectId, "prj_");
catalog_id!(IndexId, "idx_");
catalog_id!(SourceRootId, "src_");
catalog_id!(ContextId, "ctx_");
catalog_id!(GrantId, "grt_");
catalog_id!(DenyId, "deny_");

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct CatalogRevision(u64);
impl CatalogRevision {
    pub const INITIAL: Self = Self(1);
    pub const fn get(self) -> u64 {
        self.0
    }
    pub fn next(self) -> Result<Self> {
        Self::try_from(
            self.0
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("catalog revision exhausted"))?,
        )
    }
}
impl TryFrom<u64> for CatalogRevision {
    type Error = anyhow::Error;
    fn try_from(value: u64) -> Result<Self> {
        ensure!(
            (1..=i64::MAX as u64).contains(&value),
            "invalid catalog revision"
        );
        Ok(Self(value))
    }
}
impl From<CatalogRevision> for u64 {
    fn from(value: CatalogRevision) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathScope {
    Exact,
    Subtree,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeAtom {
    pub source_root_id: SourceRootId,
    pub kind: PathScope,
    pub path: String,
}
impl ScopeAtom {
    pub fn validate(&self) -> Result<()> {
        relative_path(&self.path, self.kind == PathScope::Subtree)
    }
    /// Inputs must already use the host's canonical source-component spelling.
    /// No portable case folding or alias resolution is performed here.
    pub fn matches_canonical(&self, root: SourceRootId, path: &str) -> bool {
        if root != self.source_root_id {
            return false;
        }
        self.path == path
            || self.kind == PathScope::Subtree
                && (self.path.is_empty()
                    || path
                        .strip_prefix(&self.path)
                        .is_some_and(|tail| tail.starts_with('/')))
    }
}

pub fn relative_path(path: &str, empty: bool) -> Result<()> {
    ensure!(
        path.len() <= 4096
            && (empty || !path.is_empty())
            && !path.contains(['\\', ':', '\0'])
            && (path.is_empty()
                || path
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != "..")),
        "invalid canonical scope path"
    );
    Ok(())
}
pub fn label(value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value.len() <= MAX_LABEL_BYTES
            && !value.chars().any(char::is_control),
        "invalid catalog label"
    );
    Ok(())
}
pub fn unique_bounded<T: Ord>(values: &[T], maximum: usize) -> Result<()> {
    ensure!(values.len() <= maximum, "catalog capacity exceeded");
    ensure!(
        values.iter().collect::<BTreeSet<_>>().len() == values.len(),
        "duplicate catalog identity or scope"
    );
    Ok(())
}
pub fn atoms(values: &[ScopeAtom]) -> Result<()> {
    unique_bounded(values, MAX_ATOMS)?;
    for atom in values {
        atom.validate()?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyReference {
    pub epoch: CatalogRevision,
    pub digest: Sha256Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceImportState {
    Complete,
    PendingMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexDefinition {
    pub id: IndexId,
    pub name: String,
    /// Immutable backing identity survives deletion of the active registration.
    pub backing: ProjectRegistration,
    pub definition_revision: CatalogRevision,
    pub source_revision: CatalogRevision,
    pub policy: Option<PolicyReference>,
    pub tombstone: bool,
    /// Missing private configuration is not guessed to mean primary-root only.
    pub source_import: SourceImportState,
    /// U105: optional user-editable description. Absent, every encoding and
    /// digest input is the pre-U105 one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
impl IndexDefinition {
    pub fn validate(&self) -> Result<()> {
        label(&self.name)?;
        description(self.description.as_deref())?;
        self.backing.validate()
    }
}

/// The encoded bound of an index or index-set description (U105).
pub const MAX_DESCRIPTION_BYTES: usize = 1024;

/// A description is at most [`MAX_DESCRIPTION_BYTES`] of UTF-8, not blank,
/// and has no control characters other than `\n`.
pub fn description(value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        ensure!(
            !value.trim().is_empty()
                && value.len() <= MAX_DESCRIPTION_BYTES
                && !value.chars().any(|c| c.is_control() && c != '\n'),
            "invalid catalog description"
        );
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryProject {
    pub id: QueryProjectId,
    pub name: String,
    pub definition_revision: CatalogRevision,
    pub scope_revision: CatalogRevision,
    pub defaults_revision: CatalogRevision,
    pub default_indexes: Vec<IndexId>,
    /// U105: optional user-editable description; absent bytes are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
impl QueryProject {
    pub fn validate(&self) -> Result<()> {
        label(&self.name)?;
        description(self.description.as_deref())?;
        unique_bounded(&self.default_indexes, MAX_SELECTION)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Membership {
    pub project_id: QueryProjectId,
    pub index_id: IndexId,
    pub revision: CatalogRevision,
    /// Empty means no authority, never all sources. Store validates root ownership.
    pub scope: Vec<ScopeAtom>,
}
impl Membership {
    pub fn validate(&self) -> Result<()> {
        atoms(&self.scope)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextKind {
    Folder,
    Workspace,
    Empty,
    Untitled,
    LegacyPending,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextChoices {
    pub id: ContextId,
    pub host: Sha256Digest,
    pub key: Sha256Digest,
    pub kind: ContextKind,
    pub name: String,
    pub revision: CatalogRevision,
    pub project_ids: Vec<QueryProjectId>,
    pub index_ids: Vec<IndexId>,
}
impl ContextChoices {
    pub fn validate(&self) -> Result<()> {
        label(&self.name)?;
        unique_bounded(&self.project_ids, MAX_SELECTION)?;
        unique_bounded(&self.index_ids, MAX_SELECTION)?;
        ensure!(
            !self.project_ids.is_empty() || self.index_ids.is_empty(),
            "context index defaults require Project selection"
        );
        ensure!(
            self.kind != ContextKind::LegacyPending
                || self.project_ids.is_empty() && self.index_ids.is_empty(),
            "pending context cannot infer scope"
        );
        Ok(())
    }
}

/// An explicit editor-state import is a preference, never an access grant.
/// Persist the exact request before dispatch; an import key cannot be reused
/// with a different body, and replay must not recreate a deleted context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextImport {
    pub schema: u32,
    pub key: Sha256Digest,
    pub expected_catalog_revision: CatalogRevision,
    pub context: ContextChoices,
}
impl ContextImport {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported context import");
        self.context.validate()?;
        ensure!(
            self.context.revision == CatalogRevision::INITIAL,
            "import cannot overwrite an existing context revision"
        );
        encoded_size(self, 16384)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(crate::identity::framed_digest(
            b"airk/context-import/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextImportReceipt {
    pub schema: u32,
    pub key: Sha256Digest,
    pub request_digest: Sha256Digest,
    pub context_id: ContextId,
    pub catalog_revision: CatalogRevision,
    /// A deleted association is acknowledged, never silently recreated.
    pub context_present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantedMembership {
    pub membership: Membership,
    pub source_revision: CatalogRevision,
    /// Zero denotes the empty-policy fast path; positive epochs bind a ledger.
    pub policy_epoch: u64,
    /// Bind the frozen policy directly, avoiding history walks or expansion
    /// through later relaxation of the current index policy.
    pub policy_digest: Option<Sha256Digest>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedGrant {
    pub id: GrantId,
    pub host: Sha256Digest,
    pub revision: CatalogRevision,
    pub memberships: Vec<GrantedMembership>,
    pub revoked: bool,
}
impl ReviewedGrant {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.memberships.len() <= MAX_SELECTION * MAX_SELECTION,
            "grant membership capacity exceeded"
        );
        let mut pairs = BTreeSet::new();
        let mut projects = BTreeSet::new();
        let mut indexes = BTreeSet::new();
        for grant in &self.memberships {
            grant.membership.validate()?;
            ensure!(
                grant.policy_epoch <= i64::MAX as u64,
                "invalid grant policy epoch"
            );
            ensure!(
                (grant.policy_epoch == 0) == grant.policy_digest.is_none(),
                "grant policy epoch and digest must agree"
            );
            ensure!(
                pairs.insert((grant.membership.project_id, grant.membership.index_id)),
                "duplicate grant membership"
            );
            projects.insert(grant.membership.project_id);
            indexes.insert(grant.membership.index_id);
        }
        ensure!(
            projects.len() <= MAX_SELECTION && indexes.len() <= MAX_SELECTION,
            "grant selection capacity exceeded"
        );
        Ok(())
    }
}
