// SPDX-License-Identifier: Apache-2.0
//! Explicit installer-owned retirement of unavailable legacy index metadata.
//! These values describe review and durable outcomes; they are never read grants
//! or proof that a source path is missing, a native lease is held, or bytes were deleted.
use crate::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{CatalogRevision, IndexId, SourceImportState},
    service::{ProjectRegistration, RequestKey},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_RECOVERY_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeRetirementRequest {
    pub schema: u32,
    pub caller: Incarnation,
    pub key: RequestKey,
    pub upgrade_id: Incarnation,
    pub plan_digest: Sha256Digest,
    pub installer_identity: Sha256Digest,
    pub index_id: IndexId,
    pub registration: ProjectRegistration,
    pub expected_authorizations_digest: Sha256Digest,
}
impl UpgradeRetirementRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported legacy retirement request");
        self.registration.validate()?;
        crate::index_catalog::encoded_size(self, MAX_RECOVERY_BYTES)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/upgrade-retirement-request/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
    pub fn operation_id(&self) -> Result<Incarnation> {
        self.validate()?;
        crate::catalog_management::catalog_operation_id(self.caller, &self.key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeRetirementPreview {
    pub schema: u32,
    pub request: UpgradeRetirementRequest,
    pub request_digest: Sha256Digest,
    pub original_backup_digest: Sha256Digest,
    pub original_legacy_data_digest: Sha256Digest,
    pub source_configuration: SourceImportState,
    pub effect: UpgradeRetirementEffect,
    pub affected_work: UpgradeRetirementWork,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeRetirementWork {
    pub jobs: u32,
    /// Framed digest of the exact sorted pending/running job IDs. Original job
    /// rows and terminal outcomes remain in their existing registry owners.
    pub job_ids_digest: Sha256Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeRetirementEffect {
    /// Logical authority retirement and cancellation of exactly this project's
    /// accepted work, under the upgrade owner's real process exclusion. Physical
    /// storage cleanup remains an explicit pending outcome until ordinary cleanup.
    RetireAndCancelWithCleanupPending,
}
impl UpgradeRetirementPreview {
    pub fn validate(&self) -> Result<()> {
        self.request.validate()?;
        ensure!(
            self.schema == 1
                && self.request_digest == self.request.identity()?
                && self.source_configuration == SourceImportState::PendingMetadata
                && self.affected_work.jobs <= 1024,
            "invalid unavailable legacy retirement review"
        );
        crate::index_catalog::encoded_size(self, MAX_RECOVERY_BYTES)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/upgrade-retirement-review/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeRetirementAuthorization {
    pub preview: UpgradeRetirementPreview,
    pub review_digest: Sha256Digest,
}
impl UpgradeRetirementAuthorization {
    pub fn validate(&self) -> Result<()> {
        self.preview.validate()?;
        ensure!(
            self.review_digest == self.preview.identity()?,
            "legacy retirement review changed"
        );
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/upgrade-retirement-authorization/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeRetirementReceipt {
    pub schema: u32,
    pub authorization: UpgradeRetirementAuthorization,
    pub catalog_revision: CatalogRevision,
    pub canceled_work: UpgradeRetirementWork,
    pub post_effect_legacy_data_digest: Sha256Digest,
    pub cleanup: UpgradeRetirementCleanup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeRetirementCleanup {
    Pending,
}
impl UpgradeRetirementReceipt {
    pub fn validate(&self) -> Result<()> {
        self.authorization.validate()?;
        ensure!(
            self.schema == 1 && self.canceled_work == self.authorization.preview.affected_work,
            "invalid legacy retirement outcome"
        );
        crate::index_catalog::encoded_size(self, MAX_RECOVERY_BYTES)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/upgrade-retirement-receipt/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

/// Complete canonical runtime-admin response. It is not an evidence endpoint.
/// Every variant preserves its full typed body inside the existing 64 KiB frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum UpgradeRetirementReply {
    Review {
        preview: UpgradeRetirementPreview,
        review_digest: Sha256Digest,
    },
    Authorized {
        authorization: UpgradeRetirementAuthorization,
    },
    Applied {
        receipt: UpgradeRetirementReceipt,
    },
}
impl UpgradeRetirementReply {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Review {
                preview,
                review_digest,
            } => {
                preview.validate()?;
                ensure!(
                    *review_digest == preview.identity()?,
                    "native retirement review digest changed"
                );
            }
            Self::Authorized { authorization } => authorization.validate()?,
            Self::Applied { receipt } => receipt.validate()?,
        }
        crate::index_catalog::encoded_size(self, crate::service::MAX_FRAME_BYTES)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn preview() -> Result<UpgradeRetirementPreview> {
        let request = UpgradeRetirementRequest {
            schema: 1,
            caller: Incarnation::from_bytes([1; 16]),
            key: RequestKey {
                namespace: Incarnation::from_bytes([2; 16]),
                epoch: 0,
                nonce: Incarnation::from_bytes([3; 16]),
            },
            upgrade_id: Incarnation::from_bytes([4; 16]),
            plan_digest: Sha256Digest::from_bytes([5; 32]),
            installer_identity: Sha256Digest::from_bytes([6; 32]),
            index_id: IndexId::from_identity(Incarnation::from_bytes([7; 16])),
            registration: ProjectRegistration::new(
                "/fictional/source".into(),
                "/fictional/private".into(),
                "a".into(),
            )?,
            expected_authorizations_digest: Sha256Digest::from_bytes([8; 32]),
        };
        Ok(UpgradeRetirementPreview {
            schema: 1,
            request_digest: request.identity()?,
            request,
            original_backup_digest: Sha256Digest::from_bytes([9; 32]),
            original_legacy_data_digest: Sha256Digest::from_bytes([10; 32]),
            source_configuration: SourceImportState::PendingMetadata,
            effect: UpgradeRetirementEffect::RetireAndCancelWithCleanupPending,
            affected_work: UpgradeRetirementWork {
                jobs: 1,
                job_ids_digest: Sha256Digest::from_bytes([11; 32]),
            },
        })
    }
    #[test]
    fn zero_epoch_and_distinct_service_namespace_preserve_exact_review() -> Result<()> {
        let original = preview()?;
        original.validate()?;
        let mut crossed = original.clone();
        crossed.request.key.namespace = Incarnation::from_bytes([99; 16]);
        assert!(crossed.validate().is_err());
        let mut crossed = original.clone();
        crossed.original_backup_digest = Sha256Digest::from_bytes([99; 32]);
        assert_ne!(crossed.identity()?, original.identity()?);
        let mut ready = original.clone();
        ready.source_configuration = SourceImportState::Complete;
        assert!(ready.validate().is_err());
        Ok(())
    }
    #[test]
    fn frozen_cleanup_receipt_binds_exact_reviewed_work_and_declines_unknown_fields() -> Result<()>
    {
        let preview = preview()?;
        let authorization = UpgradeRetirementAuthorization {
            review_digest: preview.identity()?,
            preview,
        };
        let receipt = UpgradeRetirementReceipt {
            schema: 1,
            canceled_work: authorization.preview.affected_work.clone(),
            authorization,
            catalog_revision: CatalogRevision::INITIAL.next()?,
            post_effect_legacy_data_digest: Sha256Digest::from_bytes([12; 32]),
            cleanup: UpgradeRetirementCleanup::Pending,
        };
        receipt.validate()?;
        let mut crossed = receipt.clone();
        crossed.canceled_work.jobs = 0;
        assert!(crossed.validate().is_err());
        let mut value = serde_json::to_value(&receipt)?;
        value["physical_files_deleted"] = true.into();
        assert!(serde_json::from_value::<UpgradeRetirementReceipt>(value).is_err());
        assert_eq!(
            serde_json::from_slice::<UpgradeRetirementReceipt>(&serde_json::to_vec(&receipt)?)?,
            receipt
        );
        Ok(())
    }
}
