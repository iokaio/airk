// SPDX-License-Identifier: Apache-2.0
//! Inert observations for the service-owned in-process inference provider.
//! Deserializing a complete observation never proves native ownership or drain.
use crate::{
    identity::{Incarnation, Sha256Digest},
    service::WorkerIdentity,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_COMPLETION_WORKERS: usize = 128;
pub const MAX_COMPLETION_BYTES: usize = 64 * 1024;
pub const MAX_COMPLETION_LEDGER_BYTES: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderIdentity {
    pub schema: u32,
    pub service_instance: Incarnation,
    pub approved_execution_sha256: Sha256Digest,
    pub provider_generation: Incarnation,
}
impl ProviderIdentity {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported inference provider identity");
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionState {
    Pending,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionBasis {
    NativeEngineBarrier,
    NativeFactoryNeverAllocated,
    NativeFactoryCompletionLedger,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    InProcess,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFailure {
    OwnerFailed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderStatus {
    pub schema: u32,
    pub kind: ProviderKind,
    pub provider: ProviderIdentity,
    pub admission_open: bool,
    pub admitted_high_water: u64,
    pub queued_outstanding: u64,
    pub running_outstanding: u64,
    pub callback_outstanding: u64,
    pub delivery_outstanding: u64,
    pub model_references_outstanding: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<ProviderFailure>,
}
impl ProviderStatus {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported inference provider status");
        self.provider.validate()?;
        ensure!(
            self.failure.is_none() || !self.admission_open,
            "failed inference provider has open admission"
        );
        let total = self
            .queued_outstanding
            .checked_add(self.running_outstanding)
            .ok_or_else(|| anyhow::anyhow!("inference provider count overflow"))?;
        ensure!(
            total <= self.admitted_high_water
                && self.callback_outstanding <= self.admitted_high_water
                && self.delivery_outstanding <= self.admitted_high_water
                && self.model_references_outstanding <= self.admitted_high_water.saturating_add(1),
            "inference provider counts exceed admitted work"
        );
        crate::index_catalog::encoded_size(self, MAX_COMPLETION_BYTES)?;
        Ok(())
    }
}

/// A bounded journal anchor. Deserializing this value never authorizes worker
/// retirement; the service retains a native witness bound to its actual lease.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionLedgerObservation {
    pub schema: u32,
    pub namespace: Incarnation,
    pub service_instance: Incarnation,
    pub approved_execution_sha256: Sha256Digest,
    pub factory_generation: Incarnation,
    pub completed_owners: u64,
    pub admitted_high_water: u64,
    pub head_sha256: Sha256Digest,
}
impl CompletionLedgerObservation {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && self.completed_owners > 0,
            "invalid native completion ledger"
        );
        crate::index_catalog::encoded_size(self, MAX_COMPLETION_LEDGER_BYTES)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionObservation {
    pub schema: u32,
    pub completion_basis: CompletionBasis,
    pub provider: ProviderIdentity,
    /// The exact durable upgrade/lifecycle operation, never an anonymous poll.
    pub operation_id: Incarnation,
    pub captured_workers: Vec<WorkerIdentity>,
    pub admitted_high_water: u64,
    pub admission_closed: bool,
    pub queued_outstanding: u64,
    pub running_outstanding: u64,
    pub callback_outstanding: u64,
    pub delivery_outstanding: u64,
    pub model_references_outstanding: u64,
    pub state: CompletionState,
    /// Bounded cumulative provenance; full original barriers remain immutable
    /// in the native owner's durable completion journal. This DTO is inert.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_ledger: Option<CompletionLedgerObservation>,
}
impl CompletionObservation {
    pub fn validate(&self) -> Result<()> {
        self.provider.validate()?;
        ensure!(
            self.schema == 1,
            "unsupported inference completion observation"
        );
        ensure!(
            self.captured_workers.len() <= MAX_COMPLETION_WORKERS,
            "inference completion worker bound exceeded"
        );
        for (index, worker) in self.captured_workers.iter().enumerate() {
            ensure!(
                worker.service == self.provider.service_instance
                    && worker.fencing_generation > 0
                    && !self.captured_workers[..index]
                        .iter()
                        .any(|prior| prior.worker == worker.worker),
                "crossed inference completion worker identity"
            );
        }
        if self.completion_basis == CompletionBasis::NativeFactoryCompletionLedger {
            let ledger = self
                .completion_ledger
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("factory completion ledger missing"))?;
            ledger.validate()?;
            ensure!(
                self.state == CompletionState::Complete
                    && ledger.service_instance == self.provider.service_instance
                    && ledger.approved_execution_sha256 == self.provider.approved_execution_sha256
                    && ledger.factory_generation == self.provider.provider_generation
                    && ledger.admitted_high_water == self.admitted_high_water,
                "crossed inference factory completion ledger"
            );
        } else {
            ensure!(
                self.completion_ledger.is_none(),
                "non-factory completion carries a ledger"
            );
        }
        let queued_running = self
            .queued_outstanding
            .checked_add(self.running_outstanding)
            .ok_or_else(|| anyhow::anyhow!("inference completion count overflow"))?;
        if self.completion_basis == CompletionBasis::NativeFactoryNeverAllocated {
            ensure!(
                self.admitted_high_water == 0
                    && self.admission_closed
                    && self.queued_outstanding == 0
                    && self.running_outstanding == 0
                    && self.callback_outstanding == 0
                    && self.delivery_outstanding == 0
                    && self.model_references_outstanding == 0
                    && self.state == CompletionState::Complete,
                "zero-provider factory admitted model work"
            );
        }
        ensure!(
            queued_running <= self.admitted_high_water
                && self.callback_outstanding <= self.admitted_high_water
                && self.delivery_outstanding <= self.admitted_high_water
                && self.model_references_outstanding <= self.admitted_high_water.saturating_add(1),
            "inference completion counts exceed admitted work"
        );
        if self.state == CompletionState::Complete {
            ensure!(
                self.admission_closed
                    && queued_running == 0
                    && self.callback_outstanding == 0
                    && self.delivery_outstanding == 0
                    && self.model_references_outstanding == 0,
                "inference completion still owns live work"
            );
        }
        crate::index_catalog::encoded_size(self, MAX_COMPLETION_BYTES)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "inference_provider_tests.rs"]
mod tests;
