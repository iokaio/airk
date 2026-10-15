// SPDX-License-Identifier: Apache-2.0
//! Explicit policy controls and writer-only maintenance/retry orchestration.
use crate::Kernel;
pub use airk_store::retention::MaintenanceBudget;
use airk_store::{
    WriterLock,
    quota::{Quota, QuotaPressure},
    retention::{
        MaintenanceReceipt, MaintenanceRun, MaintenanceStage, MaintenanceState, MaintenanceTrigger,
        PolicyReceipt, RetentionMode, RetentionPolicy, RetentionPreview, RetentionStatus,
    },
};
use anyhow::{Context, Result};
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
    fn verify_retention_scope(&self) -> Result<()> {
        if self.persistent_revision()?.is_none() {
            self.load_optional()?;
        }
        Ok(())
    }

    pub fn retention_status(&self) -> Result<RetentionStatus> {
        self.verify_retention_scope()?;
        self.store.retention_status()
    }

    pub fn retention_preview(
        &self,
        proposed: Option<&RetentionPolicy>,
    ) -> Result<RetentionPreview> {
        self.retention_preview_with_budget(proposed, MaintenanceBudget::default())
    }

    pub fn retention_preview_with_budget(
        &self,
        proposed: Option<&RetentionPolicy>,
        budget: MaintenanceBudget,
    ) -> Result<RetentionPreview> {
        self.verify_retention_scope()?;
        self.store.retention_preview(proposed, budget)
    }

    pub fn configure_retention(&self, policy: RetentionPolicy) -> Result<PolicyReceipt> {
        let writer = self.store.lock()?;
        self.verify_retention_scope()?;
        let quota = Quota::open(&self.data_root)?;
        self.store.configure_retention(&writer, &quota, policy)
    }

    pub fn maintain_retention(&self) -> Result<MaintenanceRun> {
        self.maintain_retention_with_budget(MaintenanceBudget::default())
    }

    pub fn maintain_retention_with_budget(
        &self,
        budget: MaintenanceBudget,
    ) -> Result<MaintenanceRun> {
        let writer = self.store.lock()?;
        self.verify_retention_scope()?;
        let quota = Quota::open(&self.data_root)?;
        self.release_idle_pins();
        Ok(self.run_retention_with_budget(
            &writer,
            &quota,
            MaintenanceTrigger::ExplicitMaintenance,
            budget,
        ))
    }

    /// Scoped fault observer for lifecycle qualification; no process-wide state.
    #[doc(hidden)]
    pub fn with_retention_observer(&self, observer: Arc<RetentionObserver>) -> Self {
        let mut kernel = self.clone();
        kernel.retention_observer = Some(observer);
        kernel
    }

    pub(crate) fn retention_observation(
        &self,
        served: Option<airk_core::identity::RevisionToken>,
    ) -> RetentionObservation {
        let mut observation = RetentionObservation {
            policy: None,
            last_maintenance: None,
            error: None,
            receipt_matches_publication: false,
            pending_or_unknown: true,
        };
        match self.store.retention_policy() {
            Ok(policy) => observation.policy = Some(policy),
            Err(error) => {
                observation.error = Some(format!("{error:#}").chars().take(512).collect())
            }
        }
        match self.store.last_maintenance() {
            Ok(receipt) => observation.last_maintenance = receipt,
            Err(error) => {
                observation.error = Some(format!("{error:#}").chars().take(512).collect())
            }
        }
        if let Some(receipt) = &observation.last_maintenance {
            observation.receipt_matches_publication = served.is_some() && receipt.head == served;
            observation.pending_or_unknown = !observation.receipt_matches_publication
                || receipt.pending
                || observation.error.is_some();
        }
        observation
    }

    pub(crate) fn prepare_retention(&self, writer: &WriterLock) -> Result<()> {
        // Missing policy on an existing legacy/v2 collection always means manual.
        if self.load_optional()?.is_none()
            && !self.store.directory().join("retention.json").exists()
        {
            let quota = Quota::open(&self.data_root)?;
            self.store
                .initialize_retention(writer, &quota, RetentionMode::Automatic)?;
        }
        Ok(())
    }

    pub(crate) fn release_idle_pins(&self) {
        let index = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        self.cache.clear_poison();
        let catalog = self
            .catalog_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        self.catalog_cache.clear_poison();
        drop((index, catalog));
    }

    pub(crate) fn run_retention(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        trigger: MaintenanceTrigger,
    ) -> MaintenanceRun {
        self.run_retention_with_budget(writer, quota, trigger, MaintenanceBudget::default())
    }

    fn run_retention_with_budget(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        trigger: MaintenanceTrigger,
        budget: MaintenanceBudget,
    ) -> MaintenanceRun {
        if let Some(observer) = &self.retention_observer {
            self.store.maintain_retention_with_observer(
                writer,
                quota,
                trigger,
                budget,
                &|stage, _| observer(stage),
            )
        } else {
            self.store
                .maintain_retention(writer, quota, trigger, budget)
        }
    }

    pub(crate) fn after_publication(&self, writer: &WriterLock, quota: &Quota) {
        self.release_idle_pins();
        // A failed cleanup receipt is administration state, never a failed publication.
        let _ = self.run_retention(writer, quota, MaintenanceTrigger::PostCommit);
    }

    pub(crate) fn continue_retention(
        &self,
        jobs: &[airk_store::scheduler::ScheduledJob],
    ) -> Result<Option<(String, MaintenanceRun)>> {
        // Queue registration is capped at 1024. Inspect only bounded policy/receipt
        // metadata; no source discovery, artifact hydration or collection discovery.
        let mut pending = Vec::new();
        let mut inspection_failure = None;
        for job in jobs {
            let inspected = (|| -> Result<_> {
                let kernel = Self::open_collection(
                    &job.root,
                    Some(self.data_root.clone()),
                    &job.collection,
                )?;
                anyhow::ensure!(
                    kernel.root_id() == job.root,
                    "registered root identity changed"
                );
                if kernel.store.retention_policy()?.policy.mode != RetentionMode::Automatic {
                    return Ok(None);
                }
                let Some(head) = kernel.persistent_revision()?.and_then(|head| head.token) else {
                    return Ok(None);
                };
                let last = kernel.store.last_maintenance()?;
                Ok(last
                    .as_ref()
                    .is_none_or(|receipt| {
                        receipt.pending
                            || receipt.head != Some(head)
                            || receipt.state == MaintenanceState::Disabled
                    })
                    .then(|| {
                        (
                            last.map_or(0, |receipt| receipt.finished_unix_ms),
                            job.collection.clone(),
                            kernel,
                        )
                    }))
            })();
            match inspected {
                Ok(Some(candidate)) => pending.push(candidate),
                Ok(None) => {}
                Err(error) => {
                    inspection_failure.get_or_insert_with(|| {
                        error.context(format!(
                            "retention inspection for collection {}",
                            job.collection
                        ))
                    });
                }
            }
        }
        pending.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
        for (_, collection, kernel) in pending {
            let writer = match kernel.store.lock() {
                Ok(writer) => writer,
                Err(error)
                    if matches!(
                        error.downcast_ref::<std::fs::TryLockError>(),
                        Some(std::fs::TryLockError::WouldBlock)
                    ) =>
                {
                    continue;
                }
                Err(error) => {
                    inspection_failure.get_or_insert_with(|| {
                        error.context(format!("retention writer for collection {collection}"))
                    });
                    continue;
                }
            };
            let quota = Quota::open(&self.data_root)?;
            kernel.release_idle_pins();
            let run = kernel.run_retention(&writer, &quota, MaintenanceTrigger::PostCommit);
            return Ok(Some((collection, run)));
        }
        match inspection_failure {
            Some(error) => Err(error),
            None => Ok(None),
        }
    }

    pub(crate) fn with_pressure_retry<T>(
        &self,
        writer: &WriterLock,
        mut operation: impl FnMut() -> Result<T>,
    ) -> Result<T> {
        match operation() {
            Ok(value) => Ok(value),
            Err(error)
                if error
                    .chain()
                    .any(|cause| cause.downcast_ref::<QuotaPressure>().is_some()) =>
            {
                if self.store.retention_policy()?.policy.mode != RetentionMode::Automatic {
                    return Err(error
                        .context("quota_blocked: manual retention does not prune automatically"));
                }
                // The failed operation has unwound, releasing reservation clones and staging.
                self.release_idle_pins();
                let quota = Quota::open(&self.data_root)?;
                let maintenance = self.run_retention(writer, &quota, MaintenanceTrigger::Pressure);
                if matches!(
                    maintenance.receipt.state,
                    MaintenanceState::Failed | MaintenanceState::Blocked
                ) {
                    return Err(error.context(format!(
                        "quota_blocked: maintenance could not reclaim safely: {}",
                        maintenance
                            .receipt
                            .error
                            .as_deref()
                            .unwrap_or("protected state")
                    )));
                }
                operation()
                    .context("publication failed after one automatic quota cleanup-and-retry cycle")
            }
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_store::quota::PressureScope;
    use std::cell::Cell;

    fn pressure() -> anyhow::Error {
        QuotaPressure {
            scope: PressureScope::Reservation,
            required_bytes: 16,
            available_bytes: 8,
            protected_bytes: None,
        }
        .into()
    }

    #[test]
    fn only_typed_pressure_retries_once_and_manual_never_retries() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.initialize()?;
        kernel.configure_retention(RetentionPolicy {
            mode: RetentionMode::Automatic,
            ..Default::default()
        })?;
        let writer = kernel.store.lock()?;
        let calls = Cell::new(0);
        let value = kernel.with_pressure_retry(&writer, || {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Err(pressure())
            } else {
                Ok(42)
            }
        })?;
        assert_eq!(value, 42);
        assert_eq!(calls.get(), 2);
        calls.set(0);
        kernel.with_pressure_retry(&writer, || {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                use std::io::Write;
                let quota = Quota::open(kernel.data_dir())?;
                let reservation = quota.reserve(kernel.collection(), 1)?;
                let path = reservation
                    .staging_directory()?
                    .join("original-quota-fixture");
                reservation
                    .create_file(path)?
                    .write_all(b"bounded original bytes")?;
            }
            Ok(())
        })?;
        assert_eq!(calls.get(), 2);
        calls.set(0);
        let result = kernel.with_pressure_retry::<()>(&writer, || {
            calls.set(calls.get() + 1);
            Err(pressure())
        });
        assert!(
            result
                .unwrap_err()
                .downcast_ref::<QuotaPressure>()
                .is_some()
        );
        assert_eq!(calls.get(), 2);
        calls.set(0);
        assert!(
            kernel
                .with_pressure_retry::<()>(&writer, || {
                    calls.set(calls.get() + 1);
                    anyhow::bail!("original parser failure")
                })
                .is_err()
        );
        assert_eq!(calls.get(), 1);
        drop(writer);
        kernel.configure_retention(RetentionPolicy::default())?;
        let writer = kernel.store.lock()?;
        calls.set(0);
        assert!(
            kernel
                .with_pressure_retry::<()>(&writer, || {
                    calls.set(calls.get() + 1);
                    Err(pressure())
                })
                .is_err()
        );
        assert_eq!(calls.get(), 1);
        Ok(())
    }

    #[test]
    fn idle_worker_continues_pending_cleanup_without_republishing_sources() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(
            root.path().join("original.txt"),
            "First original observation.",
        )?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.scheduled_index(false, false)?;
        kernel.configure_retention(RetentionPolicy {
            mode: RetentionMode::Automatic,
            ..Default::default()
        })?;
        let failing = kernel.with_retention_observer(Arc::new(|stage| {
            if stage == MaintenanceStage::IntentDurable {
                anyhow::bail!("original injected cleanup failure");
            }
            Ok(())
        }));
        for text in [
            "Second original observation.",
            "Third original observation.",
        ] {
            std::fs::write(root.path().join("original.txt"), text)?;
            failing.index()?;
        }
        let head = kernel.status()?.publication.unwrap().token;
        assert!(kernel.retention_status()?.last_maintenance.unwrap().pending);
        let unrelated_root = tempfile::tempdir()?;
        let unrelated = Kernel::open_collection(
            unrelated_root.path(),
            Some(data.path().into()),
            "broken_neighbor",
        )?;
        unrelated.scheduled_index(false, false)?;
        std::fs::write(
            unrelated.store.directory().join("retention.json"),
            b"invalid original neighboring policy",
        )?;
        let outcome = kernel.run_scheduled_once()?;
        assert_eq!(outcome.state, "maintained");
        assert!(outcome.report.is_none());
        assert!(!outcome.maintenance.unwrap().receipt.pending);
        assert_eq!(kernel.status()?.publication.unwrap().token, head);
        let neighbor_failure = kernel.run_scheduled_once()?;
        assert_eq!(neighbor_failure.state, "maintenance_failed");
        assert!(neighbor_failure.error.unwrap().contains("broken_neighbor"));
        std::fs::write(
            kernel.store.directory().join("retention.json"),
            b"invalid original fixture",
        )?;
        let failure = kernel.run_scheduled_once()?;
        assert_eq!(failure.state, "maintenance_failed");
        assert!(failure.error.is_some());
        assert!(failure.report.is_none());
        let status = kernel.status()?;
        assert_eq!(status.publication.unwrap().token, head);
        assert!(status.retention.pending_or_unknown);
        assert!(status.retention.error.is_some());
        Ok(())
    }
}
