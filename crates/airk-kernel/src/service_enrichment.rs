// SPDX-License-Identifier: Apache-2.0
//! Service source publication and semantic turns have independent durable jobs.
use crate::Kernel;
use anyhow::{Result, ensure};
impl Kernel {
    pub fn index_service_sources(&self) -> Result<airk_ingest::IndexReport> {
        ensure!(
            self.is_service_collection(),
            "source-only service indexing requires its project owner"
        );
        self.observe_service_index(airk_core::usage::UsageOperation::Index, || {
            self.index_current(false, true)
        })
    }
    /// One bounded 32-artifact semantic batch; the soft deadline is checked at
    /// publication boundaries. Native inference still has its own hard budgets.
    pub fn service_enrichment_turn(&self) -> Result<airk_ingest::IndexReport> {
        ensure!(
            self.is_service_collection(),
            "service enrichment requires its project owner"
        );
        self.observe_service_index(airk_core::usage::UsageOperation::Index, || {
            self.resume_current_enrichment_batches_bounded(
                1,
                Some(std::time::Duration::from_secs(30)),
            )
            .map(|(_, report)| report)
        })
    }

    /// One actual native source/semantic turn, separate from queue acceptance,
    /// brokerage and waiting clients. Only numeric report fields reach the ledger.
    pub(crate) fn observe_service_index(
        &self,
        operation: airk_core::usage::UsageOperation,
        work: impl FnOnce() -> Result<airk_ingest::IndexReport>,
    ) -> Result<airk_ingest::IndexReport> {
        use airk_core::usage::{ClientFamily, ResponseRepresentation, UsageOperation};
        ensure!(
            matches!(operation, UsageOperation::Index | UsageOperation::Reindex),
            "native indexing observation requires an indexing operation"
        );
        let span = self.usage_span(operation, ClientFamily::NativeScheduler);
        let result = work();
        // A deferred admission did no indexing work; repeated queue contention
        // must not inflate observations or invent failed extraction work.
        if result.as_ref().is_err_and(|error| {
            error.is::<crate::execution::CapacityBusy>()
                || matches!(
                    error.downcast_ref::<std::fs::TryLockError>(),
                    Some(std::fs::TryLockError::WouldBlock)
                )
        }) {
            return result;
        }
        let logical = result
            .as_ref()
            .ok()
            .and_then(|report| serde_json::to_value(report).ok());
        let bytes = logical
            .as_ref()
            .and_then(|report| serde_json::to_vec(report).ok())
            .map_or(0, |bytes| bytes.len());
        self.finish_usage(
            span,
            logical.as_ref(),
            bytes,
            result.is_err(),
            ResponseRepresentation::LogicalResultJson,
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::usage::{ClientFamily, RecordingMode, UsageOperation};

    #[test]
    fn native_turns_record_actual_work_once_and_skip_deferred_admission() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let original = "An original native work accounting fixture.\n";
        std::fs::write(root.path().join("note.txt"), original)?;
        let kernel =
            Kernel::open_service_collection(root.path(), Some(data.path().into()), "work")?;
        kernel.configure_vectors(false)?;
        kernel.configure_usage(RecordingMode::AggregatesOnly)?;
        kernel.index_service_sources()?;
        kernel.index_service_sources()?;
        assert!(
            kernel
                .observe_service_index(UsageOperation::Index, || Err(
                    crate::execution::CapacityBusy.into()
                ))
                .is_err()
        );
        let report = kernel.usage_report(30)?.report;
        assert_eq!(report.calls, 2);
        assert_eq!(report.clients[&ClientFamily::NativeScheduler], 2);
        let work = &report.operations[&UsageOperation::Index].index_work;
        assert_eq!((work.indexed, work.reused), (1, 1));
        assert_eq!(work.processed_source_bytes, (original.len() * 2) as u128);
        kernel.rebuild_service(airk_core::identity::Sha256Digest::from_bytes([67; 32]))?;
        let rebuilt = kernel.usage_report(30)?.report;
        assert_eq!(rebuilt.calls, 3);
        assert_eq!(rebuilt.clients[&ClientFamily::NativeScheduler], 3);
        let work = &rebuilt.operations[&UsageOperation::Reindex].index_work;
        assert_eq!((work.indexed, work.reused), (1, 0));
        assert_eq!(work.processed_source_bytes, original.len() as u128);
        assert!(report.comparisons.is_empty());
        assert_eq!(
            std::fs::read_to_string(root.path().join("note.txt"))?,
            original
        );
        Ok(())
    }
}
