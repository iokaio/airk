// SPDX-License-Identifier: Apache-2.0
//! Fenced bounded work snapshots. Reads use ledger metadata, never collection state.
use super::ServiceRegistry;
use airk_core::{
    service::{JobState, ProjectBinding, WorkerIdentity},
    service_progress::{ProgressObservation, WorkProgress},
};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
#[cfg(test)]
#[path = "service_progress_tests.rs"]
mod tests;

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS service_progress(
    project TEXT PRIMARY KEY REFERENCES projects(binding),
    job TEXT NOT NULL REFERENCES service_jobs(id) ON DELETE CASCADE,
    observation BLOB NOT NULL CHECK(length(observation)<=4096)) WITHOUT ROWID;";

impl ServiceRegistry {
    /// The fenced owner or its private supervisor records these observations.
    /// The supervisor first matches the reply to its current resource grant.
    pub fn observe_progress(
        &mut self,
        identity: WorkerIdentity,
        progress: &WorkProgress,
        now_ms: u64,
    ) -> Result<()> {
        progress.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let job = super::jobs::record(&tx, progress.job)?;
        let epoch: i64 = tx.query_row(
            "SELECT epoch FROM projects WHERE binding=?1",
            [identity.project.to_string()],
            |row| row.get(0),
        )?;
        ensure!(
            job.project == identity.project
                && job.fencing_generation == identity.fencing_generation
                && u64::try_from(epoch)? == identity.fencing_generation
                && job.state != JobState::Pending,
            "foreign or stale work observation"
        );
        let prior: Option<Vec<u8>> = tx
            .query_row(
                "SELECT substr(observation,1,4097) FROM service_progress WHERE project=?1",
                [identity.project.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(prior) = prior {
            ensure!(prior.len() <= 4096, "oversized cached work observation");
            let prior: ProgressObservation = serde_json::from_slice(&prior)?;
            if prior.progress.job == progress.job
                && prior.worker == identity
                && prior.progress.grant == progress.grant
            {
                ensure!(
                    progress.sequence >= prior.progress.sequence
                        && progress.elapsed_ms >= prior.progress.elapsed_ms
                        && progress.captured_files >= prior.progress.captured_files
                        && progress.extracted_files >= prior.progress.extracted_files
                        && progress.prepared_changes >= prior.progress.prepared_changes
                        && progress.committed_changes >= prior.progress.committed_changes
                        && progress.publications >= prior.progress.publications
                        && progress.verified_objects >= prior.progress.verified_objects
                        && (!prior.progress.stalled || progress.stalled),
                    "out of order work observation"
                );
            } else if prior.progress.job == progress.job {
                ensure!(
                    identity.fencing_generation > prior.worker.fencing_generation,
                    "old grant cannot replace current progress"
                );
            } else {
                let newer:bool=tx.query_row("SELECT a.seq>b.seq OR a.epoch>b.epoch FROM service_jobs a JOIN service_jobs b ON b.id=?1 WHERE a.id=?2",params![prior.progress.job.to_string(),progress.job.to_string()],|row|row.get(0))?;
                ensure!(newer, "old job cannot replace newer progress");
            }
        }
        let observation = ProgressObservation {
            worker: identity,
            current_fencing_generation: identity.fencing_generation,
            progress: progress.clone(),
            observed_unix_ms: now_ms,
            job,
        };
        let bytes = serde_json::to_vec(&observation)?;
        ensure!(bytes.len() <= 4096, "work observation exceeds byte budget");
        tx.execute("INSERT INTO service_progress VALUES(?1,?2,?3) ON CONFLICT(project) DO UPDATE SET job=excluded.job,observation=excluded.observation",params![identity.project.to_string(),progress.job.to_string(),bytes])?;
        tx.commit()?;
        Ok(())
    }
    pub fn progress_observation(
        &self,
        project: ProjectBinding,
    ) -> Result<Option<ProgressObservation>> {
        let registered = self.get(project)?;
        let bytes: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT substr(observation,1,4097) FROM service_progress WHERE project=?1",
                [project.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        bytes
            .map(|bytes| {
                ensure!(bytes.len() <= 4096, "oversized work observation");
                let mut observation: ProgressObservation = serde_json::from_slice(&bytes)?;
                observation.current_fencing_generation = registered.fencing_generation;
                observation.progress.validate()?;
                observation.job = self.job(observation.progress.job)?;
                ensure!(
                    observation.worker.project == project && observation.job.project == project,
                    "work observation binding changed"
                );
                Ok(observation)
            })
            .transpose()
    }
}
