// SPDX-License-Identifier: Apache-2.0
//! Explicit administration of shared local storage policy.
use crate::Kernel;
pub use airk_store::quota::QuotaPolicy;
use airk_store::quota::{Quota, QuotaStatus};
pub use airk_store::relocation::{
    plan as relocation_plan, recover as recover_relocation, relocate,
};
use anyhow::{Result, ensure};

impl Kernel {
    fn storage_quota(&self) -> Result<Quota> {
        // Validate the selected collection before using its shared data root.
        self.load_optional()?;
        let quota = self.collection_quota()?;
        ensure!(
            !self.root.starts_with(quota.root()),
            "index storage must not contain its source root"
        );
        Ok(quota)
    }

    pub fn quota_status(&self) -> Result<QuotaStatus> {
        self.storage_quota()?.status()
    }

    pub fn quota_policy(&self) -> Result<QuotaPolicy> {
        self.storage_quota()?.policy()
    }

    pub fn configure_quota_if_current(
        &self,
        policy: QuotaPolicy,
        expected: Option<&QuotaPolicy>,
    ) -> Result<QuotaStatus> {
        self.storage_quota()?.configure_if_current(policy, expected)
    }

    pub fn configure_quota(&self, policy: QuotaPolicy) -> Result<QuotaStatus> {
        self.storage_quota()?.configure(policy)
    }
}
