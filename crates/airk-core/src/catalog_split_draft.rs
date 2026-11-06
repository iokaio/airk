// SPDX-License-Identifier: Apache-2.0
//! Bounded user choices and complete native split reviews. Neither is authority.
use crate::{
    catalog_management::{MAX_PAGE_BYTES, MAX_RECEIPT_BYTES},
    catalog_setup::SetupStorageAction,
    catalog_split::{MAX_SPLIT_PROJECTS, SplitCapacityReview, SplitRequest},
    identity::{Sha256Digest, framed_digest},
    index_catalog::{
        CatalogRevision, IndexId, QueryProjectId, ScopeAtom, atoms, encoded_size, label,
    },
    service::RequestKey,
    source_policy::{SourcePolicy, SplitPolicyPlan, validate_root_path},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Scopes use original source IDs. Native review maps them to the independently
/// assigned destination IDs; choosing membership never issues an evidence grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "destination", rename_all = "snake_case", deny_unknown_fields)]
pub enum SplitMembershipSelection {
    Omit,
    PreserveReviewedScope,
    Selected { scope: Vec<ScopeAtom> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitProjectSelection {
    pub project_id: QueryProjectId,
    pub retain_original: bool,
    pub membership: SplitMembershipSelection,
}

/// Explicit peak-growth ceilings, not inferred size observations or proof that
/// quota was reserved. Native review supplies both actual quota-policy digests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitCapacitySelection {
    pub destination_growth: u64,
    pub destination_temporary: u64,
    pub original_rewrite: u64,
    pub cleanup_allowance: u64,
}
impl SplitCapacitySelection {
    pub fn with_policies(
        self,
        original: Sha256Digest,
        destination: Sha256Digest,
    ) -> Result<SplitCapacityReview> {
        let review = SplitCapacityReview {
            destination_growth: self.destination_growth,
            destination_temporary: self.destination_temporary,
            original_rewrite: self.original_rewrite,
            cleanup_allowance: self.cleanup_allowance,
            original_quota_digest: original,
            destination_quota_digest: destination,
        };
        review.total()?;
        Ok(review)
    }
}

/// Read-only native preparation. The service reconstructs canonical bindings,
/// complete affected-Project revisions, policy plans and storage observations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitDraftSelection {
    pub key: RequestKey,
    pub expected_catalog_revision: CatalogRevision,
    pub index_id: IndexId,
    pub selectors: Vec<ScopeAtom>,
    pub destination_name: String,
    pub destination_data_root: String,
    pub destination_collection: String,
    pub projects: Vec<SplitProjectSelection>,
    pub capacity: SplitCapacitySelection,
    pub require_semantic_ready: bool,
    pub destination_watch_enabled: bool,
}
impl SplitDraftSelection {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.key.epoch <= i64::MAX as u64,
            "split request epoch exceeds supported range"
        );
        atoms(&self.selectors)?;
        ensure!(
            !self.selectors.is_empty(),
            "split requires selected source paths"
        );
        label(&self.destination_name)?;
        validate_root_path(&self.destination_data_root)?;
        ensure!(
            !self.destination_collection.is_empty()
                && self.destination_collection.len() <= 128
                && !matches!(self.destination_collection.as_str(), "." | "..")
                && self
                    .destination_collection
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)),
            "invalid independent destination collection"
        );
        ensure!(
            self.projects.len() <= MAX_SPLIT_PROJECTS,
            "split Project choices exceed supported range"
        );
        let mut seen = BTreeSet::new();
        for project in &self.projects {
            ensure!(
                seen.insert(project.project_id),
                "duplicate split Project choice"
            );
            ensure!(
                project.retain_original
                    || !matches!(project.membership, SplitMembershipSelection::Omit),
                "split Project must retain a reviewed membership"
            );
            if let SplitMembershipSelection::Selected { scope } = &project.membership {
                atoms(scope)?;
                ensure!(
                    !scope.is_empty(),
                    "split destination scope must be explicit and nonempty"
                );
            }
        }
        self.capacity.with_policies(
            Sha256Digest::from_bytes([0; 32]),
            Sha256Digest::from_bytes([0; 32]),
        )?;
        encoded_size(self, MAX_RECEIPT_BYTES)?;
        Ok(())
    }
}

/// Full review returned identically by native management, CLI and Workbench.
/// The metadata actor independently rebuilds it before accepting its exact key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitDraft {
    pub schema: u32,
    pub request: SplitRequest,
    pub request_digest: Sha256Digest,
    pub before_policy: SourcePolicy,
    pub plan: SplitPolicyPlan,
    pub destination_storage_action: SetupStorageAction,
    /// Static bounded dependency rules, not a claim that controls were scanned.
    pub control_inputs: serde_json::Value,
    pub review_digest: Sha256Digest,
}
impl SplitDraft {
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.request
            .validate_plan(&self.before_policy, &self.plan)?;
        crate::catalog_split::validate_snapshot_admission(
            &self.request,
            &self.before_policy,
            &self.plan,
        )?;
        ensure!(
            self.request_digest == self.request.identity()?,
            "split request digest changed"
        );
        ensure!(
            self.schema == 1 && self.control_inputs.is_object(),
            "invalid split native review"
        );
        encoded_size(&self.control_inputs, 2048)?;
        Ok(framed_digest(
            b"airk/catalog-split-draft/1",
            &[&serde_json::to_vec(&(
                self.schema,
                &self.request,
                &self.before_policy,
                &self.plan,
                self.destination_storage_action,
                &self.control_inputs,
            ))?],
        ))
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.review_digest == self.identity()?,
            "split native review changed"
        );
        encoded_size(self, MAX_PAGE_BYTES)?;
        Ok(())
    }
}
