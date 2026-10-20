// SPDX-License-Identifier: Apache-2.0
//! Explicit bounded administration of retained committed evidence.
use crate::Kernel;
pub use airk_store::{
    checkpoints::{Checkpoint, CheckpointMutationReceipt, CreateCheckpoint, RemoveCheckpoint},
    history_policy::{
        HistoryMaintenanceRun, HistoryPlan, HistoryPolicyReceipt, HistoryPolicyV1,
        HistorySelectionRequest, HistoryWorkBudget, PurgeApplyRequest, PurgePlan, PurgeReceipt,
        PurgeRequest,
    },
};
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HistoryPurgeReceipt {
    pub storage: PurgeReceipt,
    pub lineage_publication: Option<airk_core::evidence::RevisionEnvelope>,
}

impl Kernel {
    pub fn history_policy(&self) -> Result<HistoryPolicyReceipt> {
        let _ = ();
        anyhow::bail!(
            "history and checkpoints are unsupported in AIRK 1.2; only the current index is retained"
        )
    }
    pub fn configure_history(&self, policy: HistoryPolicyV1) -> Result<HistoryPolicyReceipt> {
        let _ = policy;
        anyhow::bail!(
            "history and checkpoints are unsupported in AIRK 1.2; only the current index is retained"
        )
    }
    pub fn checkpoints(&self) -> Result<Vec<Checkpoint>> {
        let _ = ();
        anyhow::bail!(
            "history and checkpoints are unsupported in AIRK 1.2; only the current index is retained"
        )
    }
    pub fn create_checkpoint(
        &self,
        request: &CreateCheckpoint,
        budget: HistoryWorkBudget,
    ) -> Result<CheckpointMutationReceipt> {
        let _ = (request, budget);
        anyhow::bail!(
            "history and checkpoints are unsupported in AIRK 1.2; only the current index is retained"
        )
    }
    pub fn remove_checkpoint(
        &self,
        request: &RemoveCheckpoint,
        budget: HistoryWorkBudget,
    ) -> Result<CheckpointMutationReceipt> {
        let _ = (request, budget);
        anyhow::bail!(
            "history and checkpoints are unsupported in AIRK 1.2; only the current index is retained"
        )
    }
    pub fn history_preview(
        &self,
        request: &HistorySelectionRequest,
        budget: HistoryWorkBudget,
    ) -> Result<HistoryPlan> {
        let _ = (request, budget);
        anyhow::bail!(
            "history and checkpoints are unsupported in AIRK 1.2; only the current index is retained"
        )
    }
    pub fn maintain_history(&self, budget: HistoryWorkBudget) -> Result<HistoryMaintenanceRun> {
        let _ = budget;
        anyhow::bail!(
            "history and checkpoints are unsupported in AIRK 1.2; only the current index is retained"
        )
    }
    pub fn purge_history_preview(
        &self,
        request: &PurgeRequest,
        budget: HistoryWorkBudget,
    ) -> Result<PurgePlan> {
        let _ = (request, budget);
        anyhow::bail!(
            "history and checkpoints are unsupported in AIRK 1.2; only the current index is retained"
        )
    }
    pub fn purge_history(
        &self,
        request: &PurgeApplyRequest,
        budget: HistoryWorkBudget,
    ) -> Result<HistoryPurgeReceipt> {
        let _ = (request, budget);
        anyhow::bail!(
            "history and checkpoints are unsupported in AIRK 1.2; only the current index is retained"
        )
    }
}
