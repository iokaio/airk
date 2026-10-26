// SPDX-License-Identifier: Apache-2.0
//! Service source publication and semantic turns have independent durable jobs.
use crate::Kernel;
use anyhow::{Result, ensure};
impl Kernel {
    /// Typed temporary exclusion, independent of durable semantic failure.
    /// Native workers may repeat a retained semantic turn under the same grant.
    pub fn service_enrichment_contention(error: &anyhow::Error) -> bool {
        Self::service_enrichment_wait_reason(error).is_some()
    }
    pub fn service_enrichment_wait_reason(
        error: &anyhow::Error,
    ) -> Option<airk_core::service_resources::WaitReason> {
        use airk_core::service_resources::WaitReason;
        if error.is::<crate::execution::CapacityBusy>() {
            Some(WaitReason::IndexCapacity)
        } else if matches!(
            error.downcast_ref::<crate::CurrentGateContention>(),
            Some(crate::CurrentGateContention::Readers)
        ) {
            Some(WaitReason::ReaderLease)
        } else if error.is::<crate::CurrentGateContention>()
            || error.is::<airk_store::current::CurrentReadContention>()
            || error.is::<airk_store::current::CurrentOpenContention>()
        {
            Some(WaitReason::Publication)
        } else if error.is::<airk_store::service::collection::SelectionContention>() {
            Some(WaitReason::CollectionSelection)
        } else if matches!(
            error.downcast_ref::<std::fs::TryLockError>(),
            Some(std::fs::TryLockError::WouldBlock)
        ) {
            Some(WaitReason::NativeLock)
        } else {
            None
        }
    }
    pub fn index_service_sources(&self) -> Result<airk_ingest::IndexReport> {
        if let Some(selected) = self.active_service_view()? {
            return selected.index_service_sources();
        }
        ensure!(
            self.is_service_collection(),
            "source-only service indexing requires its project owner"
        );
        crate::service_execution::with_database_admission(|| {
            self.observe_service_index(airk_core::usage::UsageOperation::Index, || {
                self.index_current(false, true)
            })
        })
    }
    /// One bounded 32-artifact semantic batch; the soft deadline is checked at
    /// publication boundaries. Native inference still has its own hard budgets.
    pub fn service_enrichment_turn(&self) -> Result<airk_ingest::IndexReport> {
        if let Some(selected) = self.active_service_view()? {
            return selected.service_enrichment_turn();
        }
        ensure!(
            self.is_service_collection(),
            "service enrichment requires its project owner"
        );
        crate::service_execution::with_database_admission(|| {
            self.observe_service_index(airk_core::usage::UsageOperation::Index, || {
                self.resume_current_enrichment_batches_bounded(
                    1,
                    Some(std::time::Duration::from_secs(30)),
                )
                .map(|(_, report)| report)
            })
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
        // Temporary exclusion produced no committed report; repeated contention
        // must not invent failed extraction or duplicate completed index work.
        if result
            .as_ref()
            .is_err_and(Self::service_enrichment_contention)
        {
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
    fn semantic_wait_reasons_preserve_typed_cause_through_context() {
        use airk_core::service_resources::WaitReason;
        for (error, expected) in [
            (
                anyhow::Error::new(crate::execution::CapacityBusy),
                WaitReason::IndexCapacity,
            ),
            (
                anyhow::anyhow!("publication gate").context(crate::CurrentGateContention::Readers),
                WaitReason::ReaderLease,
            ),
            (
                airk_store::current::CurrentReadContention.into(),
                WaitReason::Publication,
            ),
            (
                airk_store::service::collection::SelectionContention.into(),
                WaitReason::CollectionSelection,
            ),
            (
                std::fs::TryLockError::WouldBlock.into(),
                WaitReason::NativeLock,
            ),
        ] {
            assert_eq!(
                Kernel::service_enrichment_wait_reason(&error.context("retained turn")),
                Some(expected)
            );
        }
    }
    #[test]
    fn semantic_contention_uses_typed_causes_and_preserves_terminal_failures() {
        for error in [
            anyhow::Error::new(crate::execution::CapacityBusy),
            anyhow::anyhow!("publication gate").context(crate::CurrentGateContention::Readers),
            airk_store::current::CurrentReadContention.into(),
            airk_store::service::collection::SelectionContention.into(),
        ] {
            assert!(Kernel::service_enrichment_contention(
                &error.context("retained turn")
            ));
        }
        for error in [
            anyhow::anyhow!("corrupt model; recording failure also encountered a busy gate"),
            std::io::Error::from(std::io::ErrorKind::PermissionDenied).into(),
            anyhow::Error::new(crate::service_execution::Canceled),
        ] {
            assert!(!Kernel::service_enrichment_contention(&error));
        }
    }

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
