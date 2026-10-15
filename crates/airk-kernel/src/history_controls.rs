// SPDX-License-Identifier: Apache-2.0
//! Explicit bounded administration of retained committed evidence.
use crate::Kernel;
use airk_store::quota::Quota;
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
        self.catalog()?;
        self.store.history_policy()
    }
    pub fn configure_history(&self, policy: HistoryPolicyV1) -> Result<HistoryPolicyReceipt> {
        let writer = self.store.lock()?;
        self.catalog()?;
        self.store
            .configure_history(&writer, &Quota::open(&self.data_root)?, policy)
    }
    pub fn checkpoints(&self) -> Result<Vec<Checkpoint>> {
        self.catalog()?;
        self.store.checkpoints()
    }
    pub fn create_checkpoint(
        &self,
        request: &CreateCheckpoint,
        budget: HistoryWorkBudget,
    ) -> Result<CheckpointMutationReceipt> {
        let writer = self.store.lock()?;
        self.catalog()?;
        self.store
            .create_checkpoint(&writer, &Quota::open(&self.data_root)?, request, budget)
    }
    pub fn remove_checkpoint(
        &self,
        request: &RemoveCheckpoint,
        budget: HistoryWorkBudget,
    ) -> Result<CheckpointMutationReceipt> {
        let writer = self.store.lock()?;
        self.catalog()?;
        self.store
            .remove_checkpoint(&writer, &Quota::open(&self.data_root)?, request, budget)
    }
    pub fn history_preview(
        &self,
        request: &HistorySelectionRequest,
        budget: HistoryWorkBudget,
    ) -> Result<HistoryPlan> {
        self.catalog()?;
        self.store.history_preview(request, budget)
    }
    pub fn maintain_history(&self, budget: HistoryWorkBudget) -> Result<HistoryMaintenanceRun> {
        let writer = self.store.lock()?;
        self.catalog()?;
        self.release_idle_pins();
        self.store
            .maintain_history(&writer, &Quota::open(&self.data_root)?, budget)
    }
    pub fn purge_history_preview(
        &self,
        request: &PurgeRequest,
        budget: HistoryWorkBudget,
    ) -> Result<PurgePlan> {
        self.catalog()?;
        self.store.purge_preview(request, budget)
    }
    pub fn purge_history(
        &self,
        request: &PurgeApplyRequest,
        budget: HistoryWorkBudget,
    ) -> Result<HistoryPurgeReceipt> {
        let writer = self.store.lock()?;
        self.catalog()?;
        self.release_idle_pins();
        let mut storage =
            self.store
                .purge_apply(&writer, &Quota::open(&self.data_root)?, request, budget)?;
        let mut lineage_publication = None;
        if storage.lineage_publication_required {
            let publish = (|| -> Result<_> {
                let snapshot = self.catalog()?.snapshot.clone();
                self.publish_v2(
                    &writer,
                    &snapshot,
                    self.publication_reservation()?,
                    airk_core::changes::PublicationReason::LineagePurge,
                )?;
                Ok(Self::catalog_envelope(self.catalog()?.as_ref()))
            })();
            match publish {
                Ok(publication) => {
                    lineage_publication = publication;
                    storage.lineage_publication_required = false;
                }
                Err(error) => {
                    storage.pending = true;
                    storage.error = Some(
                        format!("lineage publication pending: {error:#}")
                            .chars()
                            .take(512)
                            .collect(),
                    );
                }
            }
        }
        Ok(HistoryPurgeReceipt {
            storage,
            lineage_publication,
        })
    }
}
