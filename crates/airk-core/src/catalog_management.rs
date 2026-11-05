// SPDX-License-Identifier: Apache-2.0
//! Inert, bounded administrative catalog contracts. Preferences confer no authority.
use crate::{
    grant_provenance::{PersistedGrant, is_imported_grant_id},
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::*,
    service::RequestKey,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_PAGE_BYTES: usize = 48 * 1024;
pub const MAX_RECEIPT_BYTES: usize = 16 * 1024;

/// Canonical operation identity only; this neither admits work nor authenticates
/// a caller. Native owners validate the bound request key before a new mutation.
pub fn catalog_operation_id(caller: Incarnation, key: &RequestKey) -> Result<Incarnation> {
    let digest = framed_digest(
        b"airk/catalog-operation-key/1",
        &[&serde_json::to_vec(&(caller, key))?],
    )
    .to_string();
    digest[..32].parse()
}

impl Membership {
    /// Pure containment only. Callers must independently fence revisions and
    /// current source/policy readiness before this can support authority.
    pub fn scope_is_subset_of(&self, allowed: &Membership) -> bool {
        self.project_id == allowed.project_id
            && self.index_id == allowed.index_id
            && self.validate().is_ok()
            && allowed.validate().is_ok()
            && self.scope.iter().all(|requested| {
                allowed.scope.iter().any(|atom| {
                    atom == requested
                        || atom.kind == PathScope::Subtree
                            && atom.matches_canonical(requested.source_root_id, &requested.path)
                })
            })
    }
}

#[cfg(test)]
mod containment_tests {
    use super::*;
    #[test]
    fn frozen_membership_containment_preserves_source_and_path_boundaries() {
        let id = Incarnation::from_bytes([1; 16]);
        let mut allowed = Membership {
            project_id: QueryProjectId::from_identity(id),
            index_id: IndexId::from_identity(id),
            revision: CatalogRevision::INITIAL,
            scope: vec![ScopeAtom {
                source_root_id: SourceRootId::from_identity(id),
                kind: PathScope::Subtree,
                path: "src".into(),
            }],
        };
        let mut narrowed = allowed.clone();
        narrowed.scope[0].kind = PathScope::Exact;
        narrowed.scope[0].path = "src/file".into();
        assert!(narrowed.scope_is_subset_of(&allowed));
        narrowed.scope[0].path = "src-other/file".into();
        assert!(!narrowed.scope_is_subset_of(&allowed));
        narrowed.scope[0].path = "src/file".into();
        narrowed.scope[0].source_root_id =
            SourceRootId::from_identity(Incarnation::from_bytes([2; 16]));
        assert!(!narrowed.scope_is_subset_of(&allowed));
        narrowed = allowed.clone();
        allowed.scope[0].kind = PathScope::Exact;
        assert!(!narrowed.scope_is_subset_of(&allowed));
        narrowed.scope[0].kind = PathScope::Exact;
        assert!(narrowed.scope_is_subset_of(&allowed));
        narrowed.project_id = QueryProjectId::from_identity(Incarnation::from_bytes([2; 16]));
        assert!(!narrowed.scope_is_subset_of(&allowed));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogOperation {
    CreateProject {
        id: QueryProjectId,
        name: String,
    },
    RenameProject {
        id: QueryProjectId,
        expected_definition_revision: CatalogRevision,
        name: String,
    },
    DeleteProject {
        id: QueryProjectId,
        expected_definition_revision: CatalogRevision,
        expected_scope_revision: CatalogRevision,
        expected_defaults_revision: CatalogRevision,
    },
    RenameIndex {
        id: IndexId,
        expected_definition_revision: CatalogRevision,
        name: String,
    },
    ChangeIndexPolicy {
        id: IndexId,
        expected_source_revision: CatalogRevision,
        expected_policy_epoch: u64,
        expected_policy_digest: Option<Sha256Digest>,
        change: crate::source_policy::PolicyChange,
        configuration: Box<crate::service_configuration::ConfigurationRequest>,
        engine_identity: Sha256Digest,
    },
    BeginIndexSourceChange {
        id: IndexId,
        expected_source_revision: CatalogRevision,
        plan: Box<crate::catalog_source_change::SourceChangePlan>,
    },
    CompleteIndexSourceChange {
        id: IndexId,
        begin_operation: Incarnation,
        expected_source_revision: CatalogRevision,
        effect_digest: Sha256Digest,
    },
    RetireIndex {
        id: IndexId,
        expected_definition_revision: CatalogRevision,
        expected_source_revision: CatalogRevision,
        reason: crate::catalog_source_change::IndexRetirementReason,
    },
    SetMembership {
        membership: Membership,
        expected_scope_revision: CatalogRevision,
        expected_membership_revision: Option<CatalogRevision>,
    },
    RemoveMembership {
        project_id: QueryProjectId,
        index_id: IndexId,
        expected_scope_revision: CatalogRevision,
        expected_membership_revision: CatalogRevision,
    },
    SetProjectDefaults {
        project_id: QueryProjectId,
        expected_defaults_revision: CatalogRevision,
        index_ids: Vec<IndexId>,
    },
    SetContext {
        context: ContextChoices,
        expected_revision: Option<CatalogRevision>,
    },
    DeleteContext {
        id: ContextId,
        expected_revision: CatalogRevision,
    },
    CreateGrant {
        grant: ReviewedGrant,
    },
    RevokeGrant {
        id: GrantId,
        expected_revision: CatalogRevision,
    },
}
impl CatalogOperation {
    pub fn requires_worker_drain(&self) -> bool {
        matches!(
            self,
            Self::RetireIndex { .. } | Self::ChangeIndexPolicy { .. }
        )
    }
    pub fn requires_authority_fence(&self) -> bool {
        matches!(
            self,
            Self::DeleteProject { .. }
                | Self::SetMembership { .. }
                | Self::RemoveMembership { .. }
                | Self::RevokeGrant { .. }
                | Self::BeginIndexSourceChange { .. }
                | Self::RetireIndex { .. }
                | Self::ChangeIndexPolicy { .. }
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRequest {
    pub caller: Incarnation,
    pub key: RequestKey,
    pub expected_catalog_revision: CatalogRevision,
    pub operation: CatalogOperation,
}
impl CatalogRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.key.epoch <= i64::MAX as u64, "invalid request epoch");
        match &self.operation {
            CatalogOperation::ChangeIndexPolicy {
                expected_policy_epoch,
                expected_policy_digest,
                change,
                configuration,
                ..
            } => {
                ensure!(
                    *expected_policy_epoch <= i64::MAX as u64
                        && (*expected_policy_epoch == 0) == expected_policy_digest.is_none(),
                    "invalid reviewed source policy identity"
                );
                if let crate::source_policy::PolicyChange::Trim { selector, .. } = change {
                    selector.validate()?;
                }
                configuration.validate()?;
                ensure!(
                    !configuration.prepare,
                    "source policy work requires pinned native preparation"
                );
                let crate::service_configuration::ProjectConfiguration::ApplySourcePolicy {
                    policy,
                    operation_id,
                } = &configuration.configuration
                else {
                    anyhow::bail!("source policy change requires its exact configuration job");
                };
                ensure!(
                    *operation_id == catalog_operation_id(self.caller, &self.key)?,
                    "source policy operation identity differs from job"
                );
                policy.validate()?;
            }
            CatalogOperation::CreateProject { name, .. }
            | CatalogOperation::RenameProject { name, .. }
            | CatalogOperation::RenameIndex { name, .. } => label(name)?,
            CatalogOperation::SetMembership {
                membership,
                expected_membership_revision,
                ..
            } => {
                membership.validate()?;
                ensure!(
                    membership.revision
                        == expected_membership_revision
                            .map(|r| r.next())
                            .transpose()?
                            .unwrap_or(CatalogRevision::INITIAL),
                    "invalid next membership revision"
                );
            }
            CatalogOperation::SetProjectDefaults { index_ids, .. } => {
                unique_bounded(index_ids, MAX_SELECTION)?
            }
            CatalogOperation::SetContext {
                context,
                expected_revision,
            } => {
                context.validate()?;
                ensure!(
                    context.revision
                        == expected_revision
                            .map(|r| r.next())
                            .transpose()?
                            .unwrap_or(CatalogRevision::INITIAL),
                    "invalid next context revision"
                );
            }
            CatalogOperation::CreateGrant { grant } => {
                grant.validate()?;
                ensure!(
                    !is_imported_grant_id(grant.id),
                    "imported compatibility IDs are reserved for native cutover"
                );
                ensure!(
                    grant.revision == CatalogRevision::INITIAL && !grant.revoked,
                    "new grant must be unrevoked at initial revision"
                );
            }
            CatalogOperation::BeginIndexSourceChange { plan, .. } => {
                plan.validate()?;
                ensure!(
                    plan.job
                        == crate::catalog_source_change::source_job_id(
                            plan.registration.binding,
                            self.caller,
                            &self.key
                        )?,
                    "source change job identity differs from native request"
                );
            }
            CatalogOperation::RetireIndex {
                reason:
                    crate::catalog_source_change::IndexRetirementReason::ConfirmedMissingSource {
                        registration_epoch,
                        configuration_epoch,
                    },
                ..
            } => {
                ensure!(
                    *registration_epoch <= i64::MAX as u64
                        && *configuration_epoch <= i64::MAX as u64,
                    "invalid missing-source retirement observation epoch"
                );
            }
            _ => {}
        }
        encoded_size(self, MAX_PAGE_BYTES)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/catalog-management/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPreview {
    pub request_digest: Sha256Digest,
    pub catalog_revision: CatalogRevision,
    pub next_revision: CatalogRevision,
    pub operation: CatalogOperation,
    pub before: Vec<CatalogItem>,
    pub after: Vec<CatalogItem>,
    /// Native delivery must acknowledge its own fence before reporting completion.
    pub authority_fence_required: bool,
    /// The admitted operation changes derived index file data (possibly through
    /// its queued native job). This never authorizes writes to source files.
    pub file_data_changes: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_policy_impact: Option<crate::catalog_policy::PolicyImpact>,
}
impl CatalogPreview {
    pub fn identity(&self) -> Result<Sha256Digest> {
        encoded_size(self, MAX_PAGE_BYTES)?;
        Ok(framed_digest(
            b"airk/catalog-preview/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogReceipt {
    pub schema: u32,
    pub operation_id: Incarnation,
    pub caller: Incarnation,
    pub key: RequestKey,
    pub request_digest: Sha256Digest,
    pub catalog_revision: CatalogRevision,
    pub authority_fence_required: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    Projects,
    Indexes,
    Memberships,
    Contexts,
    Grants,
    Sources,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSource {
    pub id: SourceRootId,
    pub index_id: IndexId,
    pub canonical_path: String,
    pub owner: bool,
    pub enabled: bool,
    pub aliases: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogCursor {
    pub kind: CatalogKind,
    pub namespace: Incarnation,
    pub revision: CatalogRevision,
    pub after: String,
    pub index_id: Option<IndexId>,
    pub project_id: Option<QueryProjectId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogEntity {
    Project {
        id: QueryProjectId,
    },
    Index {
        id: IndexId,
    },
    Membership {
        project_id: QueryProjectId,
        index_id: IndexId,
    },
    Context {
        id: ContextId,
    },
    ContextKey {
        host: Sha256Digest,
        key: Sha256Digest,
    },
    Grant {
        id: GrantId,
    },
    Source {
        id: SourceRootId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogDetail {
    pub namespace: Incarnation,
    pub revision: CatalogRevision,
    pub item: Option<CatalogItem>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CatalogItem {
    Project(QueryProject),
    Index(IndexDefinition),
    Membership(Membership),
    Context(ContextChoices),
    Grant(PersistedGrant),
    Source(CatalogSource),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPageRequest {
    pub kind: CatalogKind,
    pub index_id: Option<IndexId>,
    pub project_id: Option<QueryProjectId>,
    pub after: Option<CatalogCursor>,
    pub expected_catalog_revision: Option<CatalogRevision>,
    pub limit: usize,
    pub max_bytes: usize,
}
impl CatalogPageRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=MAX_SELECTION).contains(&self.limit)
                && (1024..=MAX_PAGE_BYTES).contains(&self.max_bytes),
            "invalid catalog page budget"
        );
        ensure!(
            match self.kind {
                CatalogKind::Sources => self.index_id.is_some() && self.project_id.is_none(),
                CatalogKind::Memberships => true,
                _ => self.index_id.is_none() && self.project_id.is_none(),
            },
            "invalid catalog exact filters"
        );
        if let Some(c) = &self.after {
            ensure!(
                c.kind == self.kind
                    && c.index_id == self.index_id
                    && c.project_id == self.project_id
                    && Some(c.revision) == self.expected_catalog_revision,
                "invalid catalog continuation binding"
            );
            match self.kind {
                CatalogKind::Projects => {
                    c.after.parse::<QueryProjectId>()?;
                }
                CatalogKind::Indexes => {
                    c.after.parse::<IndexId>()?;
                }
                CatalogKind::Contexts => {
                    c.after.parse::<ContextId>()?;
                }
                CatalogKind::Grants => {
                    c.after.parse::<GrantId>()?;
                }
                CatalogKind::Sources => {
                    c.after.parse::<SourceRootId>()?;
                }
                CatalogKind::Memberships => {
                    let (p, i) = c
                        .after
                        .split_once('/')
                        .ok_or_else(|| anyhow::anyhow!("invalid membership cursor"))?;
                    p.parse::<QueryProjectId>()?;
                    i.parse::<IndexId>()?;
                }
            }
        }
        ensure!(
            self.after.is_none() || self.expected_catalog_revision.is_some(),
            "continuation requires revision"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPage {
    pub namespace: Incarnation,
    pub revision: CatalogRevision,
    pub items: Vec<CatalogItem>,
    pub continuation: Option<CatalogCursor>,
}
