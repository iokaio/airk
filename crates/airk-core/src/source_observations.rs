// SPDX-License-Identifier: Apache-2.0
//! Bounded saved-root observations. These suggestions do not confer authority.
use crate::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::{IndexId, SourceRootId, unique_bounded},
    service::ProjectBinding,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_ROOT_METADATA_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RELATIONSHIP_ROWS: usize = 64;
pub const MAX_RELATIONSHIP_BYTES: usize = 60 * 1024;
pub const MIN_RELATIONSHIP_BYTES: usize = 8192;
pub const MAX_CHILD_ENTRIES: usize = 128;
pub const MAX_REPOSITORY_BYTES: usize = 24 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerEntryKind {
    Directory,
    File,
    Alias,
    Other,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitTargetState {
    NotApplicable,
    Captured,
    Truncated,
    Invalid,
    Unavailable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryMarker {
    pub relative_root: String,
    pub marker: String,
    pub entry_kind: MarkerEntryKind,
    pub gitdir_target: Option<String>,
    pub target_state: GitTargetState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BroadRootKind {
    Home,
    VolumeRoot,
    Desktop,
    Documents,
    Downloads,
    Temporary,
    Synced,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryObservation {
    pub schema: u32,
    pub root: String,
    pub observed_unix_ms: u64,
    /// Environment and conventional-folder hints; absence is not a safety proof.
    pub broad_root_reasons: Vec<BroadRootKind>,
    pub repository_markers: Vec<RepositoryMarker>,
    pub children_requested: bool,
    /// Includes at most one lookahead entry beyond the inspected sample.
    pub child_entries_observed: usize,
    pub child_directories_inspected: usize,
    pub child_aliases_skipped: usize,
    pub non_utf8_children_skipped: usize,
    pub marker_checks: usize,
    pub git_marker_reads: usize,
    pub git_marker_bytes_captured: usize,
    /// Admission upper bound, including reads that failed after partial I/O.
    pub git_marker_read_budget_bytes: usize,
    pub observation_errors: usize,
    pub children_truncated: bool,
    pub reply_truncated: bool,
    pub deadline_exhausted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRelation {
    Same,
    Contains,
    ContainedBy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootObservationBasis {
    LegacyOwnerAndWatchCache,
    CatalogSources,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRelationship {
    pub binding: ProjectBinding,
    pub index_id: Option<IndexId>,
    pub source_root_id: Option<SourceRootId>,
    pub collection: String,
    pub root: String,
    pub relation: SourceRelation,
    pub registered: bool,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipCursor {
    pub namespace: Incarnation,
    pub snapshot: Sha256Digest,
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRelationshipsRequest {
    /// The native caller establishes canonical host identity, not this string.
    pub root: String,
    /// None is an explicit administrative library read. Scoped adapters must
    /// supply their configured/authorized bindings; an empty set never means all.
    pub bindings: Option<Vec<ProjectBinding>>,
    pub after: Option<RelationshipCursor>,
    pub limit: usize,
    pub max_bytes: usize,
}
impl SourceRelationshipsRequest {
    pub fn validate(&self) -> Result<()> {
        crate::source_policy::validate_root_path(&self.root)?;
        ensure!(
            !self.root.is_empty() && self.root.len() <= 4096 && !self.root.contains('\0'),
            "invalid source observation root"
        );
        ensure!(
            (1..=MAX_RELATIONSHIP_ROWS).contains(&self.limit)
                && (MIN_RELATIONSHIP_BYTES + 128..=MAX_RELATIONSHIP_BYTES)
                    .contains(&self.max_bytes),
            "invalid source relationship budget"
        );
        if let Some(bindings) = &self.bindings {
            unique_bounded(bindings, 32)?;
            ensure!(!bindings.is_empty(), "empty source observation selection");
        }
        if let Some(cursor) = &self.after {
            ensure!(
                cursor.offset <= 1024 * 17,
                "invalid source observation continuation"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRelationships {
    pub schema: u32,
    pub namespace: Incarnation,
    pub observed_unix_ms: u64,
    pub root: String,
    pub basis: RootObservationBasis,
    pub selected_bindings: Option<Vec<ProjectBinding>>,
    pub relationships: Vec<SourceRelationship>,
    pub matching_roots: usize,
    pub inspected_roots: usize,
    pub root_metadata_bytes: usize,
    pub cached_alias_metadata_bytes: usize,
    /// Legacy watcher caches cannot prove every configured alias is known.
    pub unknown_assignment_indexes: usize,
    pub snapshot: Sha256Digest,
    pub truncated: bool,
    pub after: Option<RelationshipCursor>,
}
