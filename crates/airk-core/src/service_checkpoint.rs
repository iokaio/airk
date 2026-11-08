// SPDX-License-Identifier: Apache-2.0
//! Private installer/service checkpoint observations, never saved execution authority.
use crate::{
    identity::{Incarnation, Sha256Digest},
    inference_provider::{CompletionObservation, CompletionState},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointState {
    Pending,
    Ready,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeCheckpointObservation {
    pub schema: u32,
    pub operation_id: Incarnation,
    pub service_instance: Incarnation,
    pub execution_sha256: Sha256Digest,
    pub candidate_execution_sha256: Sha256Digest,
    pub state: CheckpointState,
    pub pending_operations: Option<usize>,
    pub sealed_workers: usize,
    pub state_sha256: Option<Sha256Digest>,
    pub native_packet_sha256: Option<Sha256Digest>,
    pub provider: Option<CompletionObservation>,
    pub blocked_code: Option<String>,
    pub blocked_details: Option<String>,
}
impl UpgradeCheckpointObservation {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.pending_operations.is_none_or(|count| count <= 32_768)
                && self.sealed_workers <= 1024
                && self
                    .blocked_code
                    .as_ref()
                    .is_none_or(|code| code.len() <= 128),
            "invalid upgrade checkpoint observation"
        );
        ensure!(
            self.blocked_details
                .as_ref()
                .is_none_or(|details| details.len() <= 4096),
            "checkpoint failure details exceed capacity"
        );
        if let Some(provider) = &self.provider {
            provider.validate()?;
            ensure!(
                provider.operation_id == self.operation_id
                    && provider.provider.service_instance == self.service_instance
                    && provider.provider.approved_execution_sha256 == self.execution_sha256,
                "crossed checkpoint provider observation"
            );
        }
        if self.state == CheckpointState::Ready {
            ensure!(
                self.pending_operations == Some(0)
                    && self.state_sha256.is_some()
                    && self.native_packet_sha256.is_some()
                    && self
                        .provider
                        .as_ref()
                        .is_some_and(|provider| provider.state == CompletionState::Complete)
                    && self.blocked_code.is_none()
                    && self.blocked_details.is_none(),
                "checkpoint_ready lacks native completion proof"
            );
        }
        Ok(())
    }
}
