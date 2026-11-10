// SPDX-License-Identifier: Apache-2.0
//! Inert combined-read contracts. Selection and publication identities confer no authority.
use crate::{
    evidence::RevisionEnvelope,
    identity::{Sha256Digest, framed_digest},
    index_catalog::{
        CatalogRevision, GrantId, GrantedMembership, IndexId, MAX_SELECTION, QueryProjectId,
        ReviewedGrant, ScopeAtom, SourceRootId, encoded_size, relative_path, unique_bounded,
    },
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const RANK_FUSION_POLICY: &str = "airk.rank-fusion.rrf-equal/1";
pub const COMBINED_READ_SCHEMA: &str = "airk.combined-read/1";
pub mod header_templates;
pub mod origins;
pub mod query_scope;
pub mod read_retry;
pub mod recommendations;
pub mod shared_values;
pub use origins::{PublicationMemberRef, QualifiedOrigin, RankedOrigin, materialize_ranked_origin};

/// Logical metadata view only. Retain and validate the original wire body and
/// delivery receipt separately; this expanded copy is never a replacement for
/// the bytes authenticated by that receipt.
pub fn materialize_combined_read(combined: &serde_json::Value) -> Result<serde_json::Value> {
    use serde_json::Value;
    ensure!(
        combined.get("schema").and_then(Value::as_str) == Some(COMBINED_READ_SCHEMA),
        "invalid combined read namespace"
    );
    encoded_size(combined, crate::MAX_RESPONSE_BYTES * (MAX_SELECTION + 4))?;
    let members = combined
        .get("members")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("combined members are missing"))?;
    ensure!(
        !members.is_empty() && members.len() <= MAX_SELECTION,
        "invalid combined member count"
    );
    let ranking = combined
        .get("ranking")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("combined ranking is missing"))?;
    ensure!(ranking.len() <= 100, "combined ranking exceeds bound");
    let mut logical = combined.clone();
    if let Some(fields) = logical.as_object_mut() {
        fields.remove("delivery_observation");
    }
    logical["derived_view"] = serde_json::json!("materialized_native_members");
    for (offset, member) in members.iter().enumerate() {
        let index: IndexId = serde_json::from_value(member["index_id"].clone())?;
        logical["members"][offset]["result"] =
            shared_values::materialize_combined_member_result(combined, index)?;
    }
    for (offset, rank) in ranking.iter().enumerate() {
        logical["ranking"][offset]["origin"] =
            serde_json::to_value(materialize_ranked_origin(combined, rank)?)?;
    }
    Ok(logical)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberAvailability {
    #[default]
    Report,
    Require,
}

/// New Project selectors only. Legacy `project`, including mixed selectors,
/// is rejected by this closed shape and retains its separate one-index route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuerySelection {
    pub project_ids: Vec<QueryProjectId>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub index_ids: Option<Vec<IndexId>>,
    #[serde(default)]
    pub member_availability: MemberAvailability,
}

fn present<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

impl QuerySelection {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.project_ids.is_empty(),
            "Project selection is required"
        );
        unique_bounded(&self.project_ids, MAX_SELECTION)?;
        if let Some(indexes) = &self.index_ids {
            ensure!(!indexes.is_empty(), "index subset must not be empty");
            unique_bounded(indexes, MAX_SELECTION)?;
        }
        encoded_size(self, 4096)?;
        Ok(())
    }

    /// Resolve only the frozen grant. The native owner must separately check
    /// host, namespace, current membership/source/policy revisions and delivery.
    /// Defaults and memberships outside the selected Projects never add scope.
    pub fn resolve(&self, grant: &ReviewedGrant) -> Result<ResolvedSelection> {
        self.validate()?;
        grant.validate()?;
        // A persisted catalog itself is bounded to 32 MiB. Check before
        // cloning selected memberships, including for direct native callers.
        encoded_size(grant, 32 * 1024 * 1024)?;
        ensure!(!grant.revoked, "reviewed grant is revoked");
        let projects = self.project_ids.iter().copied().collect::<BTreeSet<_>>();
        let mut permitted = BTreeMap::<IndexId, Vec<GrantedMembership>>::new();
        let mut seen_projects = BTreeSet::new();
        for membership in &grant.memberships {
            if projects.contains(&membership.membership.project_id) {
                seen_projects.insert(membership.membership.project_id);
                permitted
                    .entry(membership.membership.index_id)
                    .or_default()
                    .push(membership.clone());
            }
        }
        ensure!(projects == seen_projects, "unknown or ungranted Project");
        if let Some(indexes) = &self.index_ids {
            ensure!(
                indexes.iter().all(|id| permitted.contains_key(id)),
                "index is outside the selected Project union"
            );
            permitted.retain(|id, _| indexes.contains(id));
        }
        ensure!(
            !permitted.is_empty() && permitted.len() <= MAX_SELECTION,
            "selected Project union is empty or exceeds capacity"
        );
        let mut members = Vec::with_capacity(permitted.len());
        for (index_id, mut memberships) in permitted {
            memberships.sort_by_key(|member| member.membership.project_id);
            let first = &memberships[0];
            ensure!(
                memberships.iter().all(|member| {
                    member.source_revision == first.source_revision
                        && member.policy_epoch == first.policy_epoch
                        && member.policy_digest == first.policy_digest
                }),
                "inconsistent shared-index authority"
            );
            let scope = canonical_scope_union(
                memberships
                    .iter()
                    .flat_map(|member| member.membership.scope.iter().cloned()),
            )?;
            members.push(ResolvedMember {
                index_id,
                memberships,
                scope,
            });
        }
        Ok(ResolvedSelection {
            project_ids: projects.into_iter().collect(),
            members,
            grant_id: grant.id,
            grant_revision: grant.revision,
            member_availability: self.member_availability,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedSelection {
    pub project_ids: Vec<QueryProjectId>,
    /// Sorted by unique index ID, independently of caller or Project order.
    pub members: Vec<ResolvedMember>,
    pub grant_id: GrantId,
    pub grant_revision: CatalogRevision,
    pub member_availability: MemberAvailability,
}

/// Canonical public selection observation. The native owner retains the full
/// frozen memberships and union predicates; their digest binds every scope and
/// revision without serializing those predicates repeatedly in each read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedScopeObservation {
    pub project_ids: Vec<QueryProjectId>,
    pub index_ids: Vec<IndexId>,
    pub grant_id: GrantId,
    pub grant_revision: CatalogRevision,
    pub member_availability: MemberAvailability,
    pub identity: Sha256Digest,
}

impl ResolvedSelection {
    pub fn observation(&self) -> Result<SelectedScopeObservation> {
        Ok(SelectedScopeObservation {
            project_ids: self.project_ids.clone(),
            index_ids: self.members.iter().map(|member| member.index_id).collect(),
            grant_id: self.grant_id,
            grant_revision: self.grant_revision,
            member_availability: self.member_availability,
            identity: self.identity()?,
        })
    }
    pub fn validate(&self) -> Result<()> {
        unique_bounded(&self.project_ids, MAX_SELECTION)?;
        ensure!(
            !self.project_ids.is_empty()
                && self.project_ids.windows(2).all(|pair| pair[0] < pair[1]),
            "noncanonical selected Projects"
        );
        let ids = self
            .members
            .iter()
            .map(|member| member.index_id)
            .collect::<Vec<_>>();
        unique_bounded(&ids, MAX_SELECTION)?;
        ensure!(
            !ids.is_empty() && ids.windows(2).all(|pair| pair[0] < pair[1]),
            "noncanonical selected indexes"
        );
        for member in &self.members {
            ensure!(
                !member.memberships.is_empty(),
                "selected member has no authorization"
            );
            let projects = member.project_ids();
            unique_bounded(&projects, MAX_SELECTION)?;
            ensure!(
                projects.windows(2).all(|pair| pair[0] < pair[1])
                    && projects.iter().all(|id| self.project_ids.contains(id)),
                "foreign or noncanonical membership"
            );
            let first = &member.memberships[0];
            for membership in &member.memberships {
                membership.membership.validate()?;
                ensure!(
                    membership.membership.index_id == member.index_id
                        && membership.source_revision == first.source_revision
                        && membership.policy_epoch == first.policy_epoch
                        && membership.policy_digest == first.policy_digest
                        && membership.policy_epoch <= i64::MAX as u64
                        && (membership.policy_epoch == 0) == membership.policy_digest.is_none(),
                    "inconsistent selected member identity"
                );
            }
            ensure!(
                member.scope
                    == canonical_scope_union(
                        member.memberships.iter().flat_map(|membership| membership
                            .membership
                            .scope
                            .iter()
                            .cloned())
                    )?,
                "selected scope differs from membership union"
            );
        }
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/combined-selection/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
    pub fn member(&self, id: IndexId) -> Option<&ResolvedMember> {
        self.members.iter().find(|member| member.index_id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedMember {
    pub index_id: IndexId,
    pub memberships: Vec<GrantedMembership>,
    /// A positive canonical union; disjoint scopes never become an ancestor.
    pub scope: Vec<ScopeAtom>,
}

impl ResolvedMember {
    pub fn project_ids(&self) -> Vec<QueryProjectId> {
        self.memberships
            .iter()
            .map(|member| member.membership.project_id)
            .collect()
    }
    /// The supplied path is canonical source-relative text, never a virtual
    /// alias path. Source identity must come from the native retained reader.
    pub fn projects_for(&self, root: SourceRootId, path: &str) -> Result<Vec<QueryProjectId>> {
        relative_path(path, false)?;
        Ok(self
            .memberships
            .iter()
            .filter_map(|member| {
                member
                    .membership
                    .scope
                    .iter()
                    .any(|atom| atom.matches_canonical(root, path))
                    .then_some(member.membership.project_id)
            })
            .collect())
    }
}

pub fn canonical_scope_union(atoms: impl IntoIterator<Item = ScopeAtom>) -> Result<Vec<ScopeAtom>> {
    let mut unique = BTreeSet::new();
    // Bound before collection as well as after canonicalization. Redundant
    // input cannot turn this metadata helper into an unbounded allocation.
    for (offset, atom) in atoms.into_iter().enumerate() {
        ensure!(
            offset < MAX_SELECTION * crate::index_catalog::MAX_ATOMS,
            "scope union capacity exceeded"
        );
        atom.validate()?;
        unique.insert(atom);
    }
    Ok(unique
        .iter()
        .filter(|atom| {
            !unique.iter().any(|other| {
                other != *atom
                    && other.kind == crate::index_catalog::PathScope::Subtree
                    && other.matches_canonical(atom.source_root_id, &atom.path)
            })
        })
        .cloned()
        .collect())
}

/// One total budget, allocated without multiplying allowances by member count.
/// Zero allocations are explicit unqueried members, never successful zero hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedBudgets {
    pub limit: usize,
    pub max_bytes: usize,
    pub deadline_ms: u64,
    pub verification_max_sources: usize,
    pub verification_max_bytes: u64,
    pub verification_deadline_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberAllocation {
    pub index_id: IndexId,
    pub limit: usize,
    pub max_bytes: usize,
    pub deadline_ms: u64,
    pub verification_max_sources: usize,
    pub verification_max_bytes: u64,
    pub verification_deadline_ms: u64,
}

impl MemberAllocation {
    pub fn admits_read(&self) -> bool {
        self.limit > 0 && self.max_bytes > 0 && self.deadline_ms > 0
    }
}

impl SharedBudgets {
    pub fn validate(&self) -> Result<()> {
        crate::validate_budget(self.limit, self.max_bytes)?;
        ensure!(
            (1..=30_000).contains(&self.deadline_ms),
            "combined deadline must be 1..30000 ms"
        );
        ensure!(
            self.verification_max_sources <= crate::read_limits::MAX_SOURCES
                && self.verification_max_bytes <= crate::read_limits::MAX_BYTES
                && self.verification_deadline_ms <= crate::read_limits::MAX_DEADLINE_MS,
            "combined verification budget exceeds bounds"
        );
        Ok(())
    }
    pub fn allocate(&self, selection: &ResolvedSelection) -> Result<Vec<MemberAllocation>> {
        self.validate()?;
        selection.validate()?;
        let members = selection
            .members
            .iter()
            .map(|member| member.index_id)
            .collect::<Vec<_>>();
        unique_bounded(&members, MAX_SELECTION)?;
        ensure!(
            !members.is_empty() && members.windows(2).all(|pair| pair[0] < pair[1]),
            "noncanonical combined members"
        );
        let n = members.len() as u64;
        let part = |total: u64, offset: usize| total / n + u64::from((offset as u64) < total % n);
        Ok(members
            .into_iter()
            .enumerate()
            .map(|(offset, index_id)| MemberAllocation {
                index_id,
                limit: part(self.limit as u64, offset) as usize,
                max_bytes: part(self.max_bytes as u64, offset) as usize,
                deadline_ms: part(self.deadline_ms, offset),
                verification_max_sources: part(self.verification_max_sources as u64, offset)
                    as usize,
                verification_max_bytes: part(self.verification_max_bytes, offset),
                verification_deadline_ms: part(self.verification_deadline_ms, offset),
            })
            .collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberPublication {
    pub index_id: IndexId,
    pub publication: Option<RevisionEnvelope>,
}

/// An interval of independent member observations, never an atomic global head.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationVector {
    pub authority: Sha256Digest,
    pub observed_from_unix_ms: u64,
    pub observed_to_unix_ms: u64,
    pub members: Vec<MemberPublication>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationRelation {
    Equal,
    Older,
    Newer,
    Incomparable,
    Inconsistent,
    Unknown,
}

impl PublicationVector {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.observed_to_unix_ms >= self.observed_from_unix_ms,
            "invalid publication observation interval"
        );
        let ids = self
            .members
            .iter()
            .map(|member| member.index_id)
            .collect::<Vec<_>>();
        unique_bounded(&ids, MAX_SELECTION)?;
        ensure!(
            !ids.is_empty() && ids.windows(2).all(|pair| pair[0] < pair[1]),
            "noncanonical publication vector"
        );
        for member in &self.members {
            if let Some(publication) = &member.publication {
                ensure!(
                    !publication.collection.is_empty()
                        && publication.collection.len() <= 128
                        && publication.incarnation == publication.token.incarnation
                        && publication.generation == publication.token.generation
                        && publication.revision == publication.token.revision,
                    "inconsistent member publication"
                );
            }
        }
        Ok(())
    }
    /// Causal display comparison only. This is not an ancestry proof or a
    /// historical read selector; generation comparisons require same authority,
    /// index set, collection and backing incarnation.
    pub fn compare(&self, other: &Self) -> PublicationRelation {
        use PublicationRelation::*;
        if self.validate().is_err() || other.validate().is_err() {
            return Unknown;
        }
        if self.authority != other.authority
            || self
                .members
                .iter()
                .map(|m| m.index_id)
                .ne(other.members.iter().map(|m| m.index_id))
        {
            return Incomparable;
        }
        let (mut older, mut newer) = (false, false);
        for (left, right) in self.members.iter().zip(&other.members) {
            let (Some(left), Some(right)) = (&left.publication, &right.publication) else {
                return Unknown;
            };
            if left.collection != right.collection || left.incarnation != right.incarnation {
                return Incomparable;
            }
            match left.generation.cmp(&right.generation) {
                std::cmp::Ordering::Less => older = true,
                std::cmp::Ordering::Greater => newer = true,
                std::cmp::Ordering::Equal if left.revision != right.revision => {
                    return Inconsistent;
                }
                std::cmp::Ordering::Equal => {}
            }
        }
        match (older, newer) {
            (true, true) => Incomparable,
            (true, false) => Older,
            (false, true) => Newer,
            (false, false) => Equal,
        }
    }
}

/// Identity for a future paged union. Search itself has no continuation.
/// A transport must validate the encoded cursor and live native authority;
/// this consistency digest is not a capability.
pub fn continuation_identity(
    vector: &PublicationVector,
    selection: &ResolvedSelection,
    filters_projection_budgets: &serde_json::Value,
    merge_positions: &BTreeMap<IndexId, usize>,
) -> Result<Sha256Digest> {
    vector.validate()?;
    selection.validate()?;
    ensure!(
        vector
            .members
            .iter()
            .map(|member| member.index_id)
            .eq(selection.members.iter().map(|member| member.index_id)),
        "cursor publication set differs from selection"
    );
    ensure!(
        merge_positions
            .keys()
            .copied()
            .eq(selection.members.iter().map(|member| member.index_id)),
        "cursor must bind every selected member position"
    );
    encoded_size(filters_projection_budgets, crate::MAX_RESPONSE_BYTES)?;
    Ok(framed_digest(
        b"airk/combined-continuation/1",
        &[
            &serde_json::to_vec(&(vector.authority, &vector.members))?,
            &serde_json::to_vec(selection)?,
            &serde_json::to_vec(filters_projection_budgets)?,
            &serde_json::to_vec(merge_positions)?,
        ],
    ))
}

/// Rank contributions deliberately ignore raw backend/model scores.
pub fn rank_contribution(zero_based_local_rank: usize) -> f64 {
    1.0 / (61.0 + zero_based_local_rank as f64)
}

#[cfg(test)]
mod tests;
