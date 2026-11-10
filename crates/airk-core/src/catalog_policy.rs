// SPDX-License-Identifier: Apache-2.0
//! Complete durable policy observation. A removal receipt is a dated native
//! observation, not a secure-erasure promise or source mutation authority.
use crate::{
    identity::{Incarnation, RevisionToken, Sha256Digest},
    index_catalog::{IndexId, QueryProjectId, ScopeAtom, SourceRootId},
    source_policy::SourcePolicy,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Immutable accepted retry evidence. This is independent of the current policy,
/// which may already belong to a separately reviewed successor operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyResumeReceipt {
    pub source_job: Sha256Digest,
    pub job: crate::service_records::JobRecord,
    pub configuration: crate::service_configuration::ConfigurationWork,
    pub result: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyImpact {
    pub target: ScopeAtom,
    pub canonical_root: String,
    pub affected_artifacts: Option<u64>,
    pub affected_vectors: Option<u64>,
    pub affected_facts: Option<u64>,
    pub affected_relationships: Option<u64>,
    pub counts_basis: String,
    pub affected_source_root_ids: Vec<SourceRootId>,
    pub affected_project_ids: Vec<QueryProjectId>,
    pub other_covering_index_ids: Vec<IndexId>,
    pub overlap_basis: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePolicyApplyReport {
    pub observed_unix_ms: u64,
    pub policy_digest: Sha256Digest,
    pub operation_id: Incarnation,
    pub removal_publication: Option<RevisionToken>,
    pub no_index: bool,
    pub removed_artifacts: u64,
    pub retired_components: Vec<String>,
    pub physical_cleanup_pending: bool,
    pub reclaimed_files: u64,
    pub reclaimed_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_error: Option<String>,
}

impl SourcePolicyApplyReport {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=i64::MAX as u64).contains(&self.observed_unix_ms),
            "invalid source policy observation time"
        );
        ensure!(
            self.removal_publication.is_some() != self.no_index,
            "policy removal must name its publication or explicitly identify an empty index"
        );
        ensure!(
            !self.no_index || self.removed_artifacts == 0 && self.retired_components.is_empty(),
            "empty-index removal cannot claim removed artifacts"
        );
        crate::index_catalog::unique_bounded(&self.retired_components, 2)?;
        ensure!(
            self.retired_components
                .iter()
                .all(|component| matches!(component.as_str(), "analysis" | "embedding_cache")),
            "unknown retired source policy component"
        );
        ensure!(
            self.cleanup_error
                .as_ref()
                .is_none_or(|error| error.len() <= 1024)
                && (self.cleanup_error.is_none() || self.physical_cleanup_pending),
            "physical cleanup failure requires a bounded pending outcome"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicySnapshot {
    pub schema: u32,
    pub operation_id: Incarnation,
    pub policy: SourcePolicy,
    pub job: Sha256Digest,
    pub application: Option<SourcePolicyApplyReport>,
    pub application_job: Option<Sha256Digest>,
    pub last_error: Option<String>,
}

impl PolicySnapshot {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported policy snapshot");
        self.policy.validate()?;
        ensure!(
            self.application.is_some() == self.application_job.is_some(),
            "policy application job receipt is incomplete"
        );
        if let Some(application) = &self.application {
            application.validate()?;
            ensure!(
                application.operation_id == self.operation_id
                    && application.policy_digest == self.policy.identity()?,
                "policy application receipt identity changed"
            );
        }
        ensure!(
            self.last_error
                .as_ref()
                .is_none_or(|error| error.len() <= 1024),
            "policy failure exceeds byte budget"
        );
        Ok(())
    }
}
