// SPDX-License-Identifier: Apache-2.0
//! Execution under an already claimed native split permit and prepaid quota.
use crate::Kernel;
use airk_core::{
    catalog_split::{SplitJobPurpose, SplitJobResult},
    identity::Incarnation,
};
use airk_store::service::registry::catalog::split::{ClaimedSplitWork, NativeSplitJobResult};
use anyhow::{Context, Result, ensure};

impl Kernel {
    pub fn execute_split_current_work(
        &self,
        work: &ClaimedSplitWork,
        policy_operation: Incarnation,
    ) -> Result<NativeSplitJobResult> {
        ensure!(
            self.split_authority.is_some(),
            "split execution lacks its claimed native authority"
        );
        ensure!(
            self.collection_quota()?.funding_use().is_some(),
            "split execution lacks prepaid quota"
        );
        let policy = match work.job.purpose {
            SplitJobPurpose::DestinationBuild => &work.snapshot.plan.destination,
            SplitJobPurpose::OriginalCleanup => &work.snapshot.plan.remainder,
            _ => anyhow::bail!("split current execution purpose is unsupported"),
        };
        let selected = self.with_expected_source_policy(Some(policy.clone()))?;
        let mut application = selected.apply_source_policy(policy, policy_operation)?;
        if work.job.purpose == SplitJobPurpose::OriginalCleanup {
            // Each native turn deletes at most 4096 owned files. Keep the job
            // finite while permitting real bounded progress; an actual cleanup
            // error stops immediately and remains in the retained report.
            for _ in 0..15 {
                if !application.physical_cleanup_pending || application.cleanup_error.is_some() {
                    break;
                }
                crate::service_execution::checkpoint()?;
                selected.checkpoint_completed_split_work()?;
                application = selected.apply_source_policy(policy, policy_operation)?;
            }
        }
        if work.job.purpose == SplitJobPurpose::DestinationBuild {
            if work.snapshot.request.require_semantic_ready && !selected.vectors_enabled()? {
                selected.configure_vectors(true)?;
            }
            // Establish and account the actual current journal before the scan
            // borrows its long-lived capture and failure reservations.
            selected.initialize_current()?;
            selected.checkpoint_completed_split_work()?;
            selected.index_service_sources()?;
            selected.checkpoint_completed_split_work()?;
            if work.snapshot.request.require_semantic_ready {
                // Each turn retains the native semantic continuation. A bounded
                // failure leaves the same durable work for explicit recovery.
                for _ in 0..32 {
                    crate::service_execution::checkpoint()?;
                    if selected.status()?.semantic_readiness.state
                        == airk_core::readiness::SemanticState::Ready
                    {
                        break;
                    }
                    selected.service_enrichment_turn()?;
                    selected.checkpoint_completed_split_work()?;
                }
            }
        }
        selected.checkpoint_completed_split_work()?;
        let status = selected.status()?;
        let result = SplitJobResult {
            schema: 1,
            operation_id: work.snapshot.receipt.operation_id,
            request_digest: work.snapshot.receipt.request_digest,
            purpose: work.job.purpose,
            publication: status.publication.map(|publication| publication.token),
            policy_digest: policy.identity()?,
            selected_sources: status.files.try_into()?,
            actual_bytes: None,
            semantic_ready: status.semantic_readiness.state
                == airk_core::readiness::SemanticState::Ready,
            cleanup_complete: !application.physical_cleanup_pending,
            policy_application: Some(application),
            remainder_witness: None,
        };
        selected.qualify_split_job_result(work, result)
    }

    pub fn checkpoint_completed_split_work(&self) -> Result<()> {
        let quota = self.collection_quota()?;
        ensure!(
            quota.funding_use().is_some(),
            "split checkpoint has no prepaid quota"
        );
        let writer = self.store.lock()?;
        let scope = self
            .current_scope()?
            .context("split checkpoint lacks current publication")?;
        let capacity = self.store.current_database_capacity(&scope)?;
        self.store
            .current_write(&writer, &quota, &scope, capacity)?
            .reconcile_operation_funding()?;
        Ok(())
    }
}
