// SPDX-License-Identifier: Apache-2.0
//! Metadata fence for resumable, explicitly approved inactive-project cleanup.
use super::{ProjectLease, ServiceRegistry};
use airk_core::{
    identity::Sha256Digest,
    service::{JobState, ProjectBinding},
    service_cleanup::DerivedCleanupReport,
    service_configuration::ProjectConfiguration,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, TransactionBehavior, params};

impl ServiceRegistry {
    pub fn derived_work_pending(&self, project: ProjectBinding) -> Result<bool> {
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM service_jobs j JOIN service_configuration c ON c.job=j.id WHERE j.project=?1 AND j.state IN('pending','running') AND json_extract(CAST(c.payload AS TEXT),'$.request.configuration.operation') IN('prepare_derived_cleanup','delete_derived'))",[project.to_string()],|row|row.get(0))?)
    }
    /// Persist before any removal. Failed/canceled attempts retain this barrier.
    pub fn begin_derived_cleanup(
        &mut self,
        lease: &ProjectLease,
        job: Sha256Digest,
        guard: Sha256Digest,
    ) -> Result<()> {
        ensure!(
            lease.derived,
            "derived cleanup requires its own worker role"
        );
        self.validate_inactive_worker(lease)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = super::jobs::worker_epoch(&transaction, &self.root, lease)?;
        let record = super::jobs::record(&transaction, job)?;
        ensure!(
            record.project == lease.binding
                && record.fencing_generation == epoch
                && record.state == JobState::Running,
            "cleanup job is not owned by this worker"
        );
        let work = super::configuration::work(&transaction, job)?
            .context("cleanup configuration is missing")?;
        ensure!(
            work.request.configuration == ProjectConfiguration::DeleteDerived { preview: guard },
            "cleanup approval differs from the accepted request"
        );
        ensure!(transaction.execute("UPDATE projects SET cleanup_guard=?1 WHERE binding=?2 AND registered=0 AND (cleanup_guard IS NULL OR cleanup_guard=?1)",params![guard.to_string(),lease.binding.to_string()])?==1,"another cleanup must finish before this approval can run");
        transaction.commit()?;
        Ok(())
    }
}

/// Result capture precedes completion; barrier removal and success are atomic.
pub(super) fn finish_if_requested(
    connection: &Connection,
    project: ProjectBinding,
    job: Sha256Digest,
) -> Result<()> {
    let Some(work) = super::configuration::work(connection, job)? else {
        return Ok(());
    };
    let ProjectConfiguration::DeleteDerived { preview } = work.request.configuration else {
        return Ok(());
    };
    let report: DerivedCleanupReport = serde_json::from_value(
        super::configuration::result(connection, job)?
            .context("cleanup completion receipt missing")?,
    )?;
    report.validate()?;
    ensure!(
        report.complete && report.guard == preview,
        "cleanup is not complete for this approval"
    );
    ensure!(connection.execute("UPDATE projects SET cleanup_guard=NULL WHERE binding=?1 AND registered=0 AND cleanup_guard=?2",params![project.to_string(),preview.to_string()])?==1,"cleanup barrier differs from completion");
    super::bump(connection)?;
    Ok(())
}
