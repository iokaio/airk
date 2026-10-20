// SPDX-License-Identifier: Apache-2.0
//! Versioned historical controls; current objects have automatic bounded reclamation.
use crate::Kernel;
pub use airk_store::retention::MaintenanceBudget;
use airk_store::{
    WriterLock,
    retention::{
        MaintenanceReceipt, MaintenanceRun, MaintenanceStage, PolicyReceipt, RetentionPolicy,
        RetentionPreview, RetentionStatus,
    },
};
use anyhow::Result;
use serde::Serialize;
use std::sync::Arc;
pub(crate) type RetentionObserver = dyn Fn(MaintenanceStage) -> Result<()> + Send + Sync;
/// Observed administration state, independent of the pinned evidence revision.
#[derive(Serialize)]
pub struct RetentionObservation {
    pub policy: Option<PolicyReceipt>,
    pub last_maintenance: Option<MaintenanceReceipt>,
    pub error: Option<String>,
    pub receipt_matches_publication: bool,
    pub pending_or_unknown: bool,
}

impl Kernel {
    pub fn retention_status(&self) -> Result<RetentionStatus> {
        let _ = ();
        anyhow::bail!(
            "generation retention is unsupported in AIRK 1.2; current objects are reclaimed automatically"
        )
    }
    pub fn retention_preview(
        &self,
        proposed: Option<&RetentionPolicy>,
    ) -> Result<RetentionPreview> {
        let _ = proposed;
        anyhow::bail!(
            "generation retention is unsupported in AIRK 1.2; current objects are reclaimed automatically"
        )
    }
    pub fn retention_preview_with_budget(
        &self,
        proposed: Option<&RetentionPolicy>,
        budget: MaintenanceBudget,
    ) -> Result<RetentionPreview> {
        let _ = (proposed, budget);
        anyhow::bail!(
            "generation retention is unsupported in AIRK 1.2; current objects are reclaimed automatically"
        )
    }
    pub fn configure_retention(&self, policy: RetentionPolicy) -> Result<PolicyReceipt> {
        let _ = policy;
        anyhow::bail!(
            "generation retention is unsupported in AIRK 1.2; current objects are reclaimed automatically"
        )
    }
    pub fn maintain_retention(&self) -> Result<MaintenanceRun> {
        let _ = ();
        anyhow::bail!(
            "generation retention is unsupported in AIRK 1.2; current objects are reclaimed automatically"
        )
    }
    pub fn maintain_retention_with_budget(
        &self,
        budget: MaintenanceBudget,
    ) -> Result<MaintenanceRun> {
        let _ = budget;
        anyhow::bail!(
            "generation retention is unsupported in AIRK 1.2; current objects are reclaimed automatically"
        )
    }
    pub fn continue_automatic_retention(&self) -> Result<Option<MaintenanceRun>> {
        Ok(None)
    }
    #[doc(hidden)]
    pub fn with_retention_observer(&self, _observer: Arc<RetentionObserver>) -> Self {
        self.clone()
    }
    pub(crate) fn continue_retention(
        &self,
        _jobs: &[airk_store::scheduler::ScheduledJob],
    ) -> Result<Option<(String, MaintenanceRun)>> {
        Ok(None)
    }
    pub(crate) fn with_pressure_retry<T>(
        &self,
        _writer: &WriterLock,
        mut operation: impl FnMut() -> Result<T>,
    ) -> Result<T> {
        operation()
    }
}
