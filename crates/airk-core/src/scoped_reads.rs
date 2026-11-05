// SPDX-License-Identifier: Apache-2.0
//! Inert, publication-compiled read selectors. These values are not IPC credentials.
use crate::{
    identity::Sha256Digest,
    index_catalog::{PathScope, SourceRootId, relative_path},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

fn canonical_virtual_path(path: &str, empty: bool) -> bool {
    path.len() <= 8192
        && (empty || !path.is_empty())
        && !path.contains(['\\', ':', '\0'])
        && (path.is_empty()
            || path
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."))
}

/// Stable source identity and its original native root, supplied by the catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDescriptor {
    pub source_root_id: SourceRootId,
    pub original_root: String,
}

/// Exact retained source-name projection supplied only through protected native
/// reads. Current catalog aliases cannot substitute for published aliases.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberSourceProjection {
    pub source_root_id: SourceRootId,
    pub original_root: String,
    pub primary: bool,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedSourceProjection {
    pub publication: crate::evidence::RevisionEnvelope,
    pub scope_fingerprint: Sha256Digest,
    pub sources: Vec<MemberSourceProjection>,
}

impl ScopedSourceProjection {
    pub fn validate(&self) -> Result<()> {
        use crate::index_catalog::{MAX_SOURCE_ROOTS, unique_bounded};
        use std::collections::BTreeSet;
        ensure!(
            !self.publication.collection.is_empty()
                && self.publication.collection.len() <= 128
                && self.publication.incarnation == self.publication.token.incarnation
                && self.publication.generation == self.publication.token.generation
                && self.publication.revision == self.publication.token.revision,
            ScopeReadRefused
        );
        unique_bounded(
            &self
                .sources
                .iter()
                .map(|source| source.source_root_id)
                .collect::<Vec<_>>(),
            MAX_SOURCE_ROOTS,
        )?;
        let mut roots = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        ensure!(
            self.sources.iter().filter(|source| source.primary).count() <= 1,
            ScopeReadRefused
        );
        for source in &self.sources {
            ensure!(
                roots.insert(&source.original_root)
                    && source.original_root.len() <= 4096
                    && std::path::Path::new(&source.original_root).is_absolute()
                    && source.aliases.len() <= 16
                    && (!source.primary || source.aliases.is_empty())
                    && (source.primary || !source.aliases.is_empty()),
                ScopeReadRefused
            );
            for alias in &source.aliases {
                ensure!(
                    !alias.is_empty()
                        && alias.len() <= 32
                        && alias.bytes().all(|byte| byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || b"_-".contains(&byte))
                        && aliases.insert(alias),
                    ScopeReadRefused
                );
            }
        }
        ensure!(aliases.len() <= 16, ScopeReadRefused);
        crate::index_catalog::encoded_size(self, crate::MAX_RESPONSE_BYTES)?;
        Ok(())
    }
}

/// Installed authorization identity only. This is not a freshness observation,
/// a publication selector, a live grant check, or permission to widen a read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeObservation {
    pub schema: u32,
    pub qualifier: ScopeQualifier,
    pub fingerprint: Sha256Digest,
    pub provenance: crate::grant_provenance::GrantProvenanceKind,
    pub grant: ScopeGrantObservation,
    /// P6f contains one member; the same contract admits up to 32 in P7.
    pub members: Vec<ScopeMemberObservation>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeQualifier {
    SelectedSubset,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeGrantObservation {
    pub id: crate::index_catalog::GrantId,
    pub revision: crate::index_catalog::CatalogRevision,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeMemberObservation {
    pub index_id: crate::index_catalog::IndexId,
    pub memberships: Vec<ScopeMembershipObservation>,
    pub source_root_ids: Vec<SourceRootId>,
    pub source_revision: crate::index_catalog::CatalogRevision,
    pub policy_epoch: u64,
    pub policy_digest: Option<Sha256Digest>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeMembershipObservation {
    pub project_id: crate::index_catalog::QueryProjectId,
    pub membership_revision: crate::index_catalog::CatalogRevision,
}
impl ScopeObservation {
    pub fn validate(&self) -> Result<()> {
        use crate::index_catalog::{MAX_SELECTION, MAX_SOURCE_ROOTS, unique_bounded};
        ensure!(
            self.schema == 1 && !self.members.is_empty(),
            ScopeReadRefused
        );
        unique_bounded(
            &self
                .members
                .iter()
                .map(|member| member.index_id)
                .collect::<Vec<_>>(),
            MAX_SELECTION,
        )?;
        for member in &self.members {
            ensure!(
                !member.memberships.is_empty()
                    && member.policy_epoch <= i64::MAX as u64
                    && (member.policy_epoch == 0) == member.policy_digest.is_none(),
                ScopeReadRefused
            );
            unique_bounded(
                &member
                    .memberships
                    .iter()
                    .map(|membership| membership.project_id)
                    .collect::<Vec<_>>(),
                MAX_SELECTION,
            )?;
            unique_bounded(&member.source_root_ids, MAX_SOURCE_ROOTS)?;
        }
        crate::index_catalog::encoded_size(self, 16 * 1024)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ScopeErrorObservation {
    pub observation: ScopeObservation,
    pub message: String,
}
impl std::fmt::Display for ScopeErrorObservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ScopeErrorObservation {}

#[derive(Clone, Copy, Debug)]
pub struct ScopeReplyBudget {
    pub required_bytes: usize,
    pub maximum_bytes: usize,
}
impl ScopeReplyBudget {
    pub fn receipt(self) -> serde_json::Value {
        serde_json::json!({"error":{"code":"scope_reply_budget","retryable":false,"required_bytes":self.required_bytes,"maximum_bytes":self.maximum_bytes}})
    }
}
impl std::fmt::Display for ScopeReplyBudget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Scoped authorization metadata cannot fit this response frame; narrow the requested result or increase its output budget.")
    }
}
impl std::error::Error for ScopeReplyBudget {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledPathAtom {
    pub kind: PathScope,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledReadScope {
    pub fingerprint: Sha256Digest,
    pub atoms: Vec<CompiledPathAtom>,
    pub source_atoms: Vec<CompiledSourceAtom>,
    pub external_aliases: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledSourceAtom {
    pub original_root: String,
    pub kind: PathScope,
    pub path: String,
}
impl CompiledReadScope {
    /// Bounded metadata proof; never enumerates the indexed path roster.
    pub fn covers_sources(
        &self,
        primary: &str,
        aliases: &std::collections::BTreeMap<String, String>,
    ) -> bool {
        let native = |root: &str| {
            self.source_atoms.iter().any(|atom| {
                atom.original_root == root
                    && atom.kind == PathScope::Subtree
                    && atom.path.is_empty()
            })
        };
        let virtual_root = |prefix: &str| {
            self.atoms
                .iter()
                .any(|atom| atom.kind == PathScope::Subtree && atom.path == prefix)
        };
        aliases.len() <= 16
            && self.external_aliases.len() == aliases.len()
            && aliases
                .keys()
                .all(|alias| self.external_aliases.contains(alias))
            && native(primary)
            && virtual_root("")
            && aliases
                .iter()
                .all(|(alias, root)| native(root) && virtual_root(&format!("@{alias}")))
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.external_aliases.len() <= 16
                && self.external_aliases.iter().all(|alias| !alias.is_empty()
                    && alias.len() <= 32
                    && alias.bytes().all(|b| b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || b == b'-'
                        || b == b'_')),
            "invalid compiled aliases"
        );
        ensure!(
            self.atoms.len() <= 32 * 128,
            "compiled scope capacity exceeded"
        );
        for atom in &self.atoms {
            ensure!(
                canonical_virtual_path(&atom.path, atom.kind == PathScope::Subtree),
                "invalid compiled path"
            );
        }
        ensure!(
            self.source_atoms.len() <= 32 * 128,
            "compiled source scope capacity exceeded"
        );
        for atom in &self.source_atoms {
            ensure!(
                !atom.original_root.is_empty() && atom.original_root.len() <= 4096,
                "invalid original source root"
            );
            relative_path(&atom.path, atom.kind == PathScope::Subtree)?;
        }
        Ok(())
    }
    pub fn allows_source(&self, root: &str, path: &str) -> bool {
        canonical_virtual_path(path, false)
            && self.source_atoms.iter().any(|atom| {
                atom.original_root == root
                    && (atom.path == path
                        || atom.kind == PathScope::Subtree
                            && (atom.path.is_empty()
                                || path
                                    .strip_prefix(&atom.path)
                                    .is_some_and(|tail| tail.starts_with('/'))))
            })
    }
    pub fn allows_path(&self, path: &str) -> bool {
        canonical_virtual_path(path, false)
            && self.atoms.iter().any(|atom| {
                self.namespace(path) == self.namespace(&atom.path)
                    && (atom.path == path
                        || atom.kind == PathScope::Subtree
                            && (atom.path.is_empty()
                                || path
                                    .strip_prefix(&atom.path)
                                    .is_some_and(|tail| tail.starts_with('/'))))
            })
    }
    fn namespace<'a>(&'a self, path: &str) -> Option<&'a str> {
        let path = path.strip_prefix('@')?;
        self.external_aliases
            .iter()
            .find(|alias| {
                path == alias.as_str()
                    || path
                        .strip_prefix(alias.as_str())
                        .is_some_and(|tail| tail.starts_with('/'))
            })
            .map(String::as_str)
    }
}

/// A handler cannot safely represent this read under the installed authority.
/// Never convert this refusal into an unrestricted metadata fallback.
#[derive(Clone, Copy, Debug)]
pub struct ScopeReadRefused;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeReadFailureCode {
    ScopedReadUnavailable,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeReadFailure {
    pub schema: u32,
    pub code: ScopeReadFailureCode,
    pub retryable: bool,
}
impl ScopeReadRefused {
    pub const fn receipt(self) -> ScopeReadFailure {
        ScopeReadFailure {
            schema: 1,
            code: ScopeReadFailureCode::ScopedReadUnavailable,
            retryable: false,
        }
    }
}
impl std::fmt::Display for ScopeReadRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("scoped_read_unavailable: read is outside the installed authority or cannot be safely scoped")
    }
}
impl std::error::Error for ScopeReadRefused {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_source_proof_requires_coherent_native_and_virtual_atoms() {
        use std::collections::BTreeMap;
        let aliases = BTreeMap::from([("docs".into(), "external".into())]);
        let mut scope = CompiledReadScope {
            fingerprint: Sha256Digest::from_bytes([1; 32]),
            atoms: vec![],
            external_aliases: vec!["docs".into()],
            source_atoms: vec!["primary", "external"]
                .into_iter()
                .map(|root| CompiledSourceAtom {
                    original_root: root.into(),
                    kind: PathScope::Subtree,
                    path: String::new(),
                })
                .collect(),
        };
        assert!(!scope.covers_sources("primary", &aliases));
        scope.atoms = vec!["", "@docs"]
            .into_iter()
            .map(|path| CompiledPathAtom {
                kind: PathScope::Subtree,
                path: path.into(),
            })
            .collect();
        assert!(scope.covers_sources("primary", &aliases));
        assert!(scope.allows_path("primary.rs"));
        assert!(scope.allows_path("@docs/external.rs"));
        scope.external_aliases.clear();
        assert!(!scope.covers_sources("primary", &aliases));
        scope.external_aliases.push("docs".into());
        scope.atoms[1].kind = PathScope::Exact;
        assert!(!scope.covers_sources("primary", &aliases));
        scope.atoms[1].kind = PathScope::Subtree;
        scope.source_atoms[1].path = "public".into();
        assert!(!scope.covers_sources("primary", &aliases));
    }
    #[test]
    fn exact_and_subtree_are_component_bounded_and_empty_is_deny() {
        let mut scope = CompiledReadScope {
            fingerprint: Sha256Digest::from_bytes([1; 32]),
            atoms: vec![],
            source_atoms: vec![],
            external_aliases: vec!["docs".into()],
        };
        assert!(!scope.allows_path("anything"));
        scope.atoms.push(CompiledPathAtom {
            kind: PathScope::Exact,
            path: "one".into(),
        });
        scope.atoms.push(CompiledPathAtom {
            kind: PathScope::Subtree,
            path: "@docs/public".into(),
        });
        assert!(scope.allows_path("one"));
        assert!(!scope.allows_path("one/child"));
        assert!(scope.allows_path("@docs/public/a"));
        assert!(!scope.allows_path("@docs/publicity/a"));
        assert!(!scope.allows_path("@docs/public/../private"));
        assert!(!scope.allows_path("@docs\\public\\a"));
        scope.atoms.push(CompiledPathAtom {
            kind: PathScope::Subtree,
            path: String::new(),
        });
        assert!(scope.allows_path("@notes.md"));
        assert!(scope.allows_path("@types/file.ts"));
        assert!(!scope.allows_path("@docs/private/a"));
        assert!(scope.allows_path(&"a".repeat(8192)));
        let long = format!("@docs/{}", "x".repeat(4096));
        scope.atoms.push(CompiledPathAtom {
            kind: PathScope::Exact,
            path: long.clone(),
        });
        assert!(scope.validate().is_ok());
        assert!(scope.allows_path(&long));
    }
}
