// SPDX-License-Identifier: Apache-2.0
//! Durable desired-state jobs, idempotent receipts and fencing-checked completion.
use super::ServiceRegistry;
use crate::service::ProjectLease;
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    service::{JobError, JobKind, JobOutcome, JobRecord, JobState, ProjectBinding, RequestKey},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(super) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS service_jobs(
        seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
        project TEXT NOT NULL REFERENCES projects(binding), kind TEXT NOT NULL,
        state TEXT NOT NULL, epoch INTEGER NOT NULL CHECK(epoch>=0),
        attempts INTEGER NOT NULL CHECK(attempts>=0 AND attempts<=3),
        outcome BLOB CHECK(outcome IS NULL OR length(outcome)<=2048));
    CREATE INDEX IF NOT EXISTS service_jobs_pending ON service_jobs(project,state,seq);
    CREATE TABLE IF NOT EXISTS service_requests(
        namespace TEXT NOT NULL, epoch INTEGER NOT NULL CHECK(epoch>=0), nonce TEXT NOT NULL,
        caller TEXT NOT NULL, job TEXT NOT NULL REFERENCES service_jobs(id) ON DELETE CASCADE,
        PRIMARY KEY(namespace,epoch,nonce,caller)) WITHOUT ROWID;";

const MAX_JOBS: i64 = 1024;
const MAX_REQUESTS: i64 = 4096;

fn record(connection: &Connection, id: Sha256Digest) -> Result<JobRecord> {
    let fields = connection
        .query_row(
            "SELECT project,kind,state,epoch,attempts,substr(outcome,1,2049)
        FROM service_jobs WHERE id=?1",
            [id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                ))
            },
        )
        .optional()?
        .ok_or(JobError::UnknownJob)?;
    let (project, kind, state, epoch, attempts, outcome) = fields;
    let outcome = outcome
        .map(|bytes| {
            ensure!(bytes.len() <= 2048, "service job receipt exceeds bound");
            Ok(serde_json::from_slice::<JobOutcome>(&bytes)?)
        })
        .transpose()?;
    let state = serde_json::from_value(state.into())?;
    let coherent = matches!(
        (&state, &outcome),
        (JobState::Pending | JobState::Running, None)
            | (JobState::Succeeded, Some(JobOutcome::Succeeded { .. }))
            | (JobState::Failed, Some(JobOutcome::Failed { .. }))
            | (JobState::Canceled, Some(JobOutcome::Canceled))
    );
    ensure!(coherent && attempts <= 3, "invalid durable job state");
    Ok(JobRecord {
        id,
        project: project.parse()?,
        kind: serde_json::from_value(kind.into())?,
        state,
        fencing_generation: u64::try_from(epoch)?,
        attempts,
        outcome,
    })
}

fn worker_epoch(
    connection: &Connection,
    root: &std::path::Path,
    lease: &ProjectLease,
) -> Result<u64> {
    ensure!(lease.root == root, JobError::StaleWorker);
    let epoch = lease.epoch.ok_or(JobError::StaleWorker)?;
    let current: i64 = connection.query_row(
        "SELECT epoch FROM projects WHERE binding=?1",
        [lease.binding.to_string()],
        |row| row.get(0),
    )?;
    ensure!(u64::try_from(current)? == epoch, JobError::StaleWorker);
    Ok(epoch)
}

impl ServiceRegistry {
    /// Fair oldest-pending order, bounded independently of the number of saved projects.
    pub fn pending_projects(&self, limit: usize) -> Result<Vec<ProjectBinding>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid pending project page bound"
        );
        let mut statement = self.connection.prepare(
            "SELECT j.project FROM service_jobs j JOIN projects p ON p.binding=j.project WHERE j.state='running' OR (j.state='pending' AND p.paused=0) GROUP BY j.project ORDER BY min(j.seq) LIMIT ?1")?;
        let rows = statement.query_map([limit as i64], |row| row.get::<_, String>(0))?;
        rows.map(|row| row?.parse()).collect()
    }
    /// Rotate the request epoch when terminal receipts are retired, so an old unknown key
    /// always expires rather than accidentally replaying its former mutation.
    fn prune_job_receipts(&mut self, needs_job: bool) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let jobs: i64 =
            transaction.query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
        let requests: i64 =
            transaction.query_row("SELECT count(*) FROM service_requests", [], |row| {
                row.get(0)
            })?;
        if (needs_job && jobs >= MAX_JOBS) || requests >= MAX_REQUESTS {
            let deleted = transaction.execute("DELETE FROM service_jobs WHERE seq IN(
                SELECT seq FROM service_jobs WHERE state IN('succeeded','failed','canceled') ORDER BY seq LIMIT 64)", [])?;
            ensure!(deleted > 0, JobError::Capacity);
            let epoch: i64 = transaction.query_row(
                "SELECT request_epoch FROM service_meta WHERE id=1",
                [],
                |row| row.get(0),
            )?;
            transaction.execute(
                "UPDATE service_meta SET request_epoch=?1 WHERE id=1",
                [epoch
                    .checked_add(1)
                    .context("service request epoch exhausted")?],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn request_epoch(&mut self) -> Result<(Incarnation, u64)> {
        self.prune_job_receipts(false)?;
        let epoch: i64 = self.connection.query_row(
            "SELECT request_epoch FROM service_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        Ok((self.incarnation()?, u64::try_from(epoch)?))
    }

    pub fn enqueue(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        key: &RequestKey,
        kind: JobKind,
    ) -> Result<JobRecord> {
        let epoch = i64::try_from(key.epoch).map_err(|_| JobError::KeyExpired)?;
        // Accepted keys remain readable even when every queue slot is occupied.
        let existing: Option<String> = self.connection.query_row(
            "SELECT job FROM service_requests WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4",
            params![key.namespace.to_string(),epoch,key.nonce.to_string(),caller.to_string()],
            |row| row.get(0),
        ).optional()?;
        if let Some(id) = existing {
            let existing = record(&self.connection, id.parse()?)?;
            ensure!(
                existing.project == project && existing.kind == kind,
                JobError::KeyConflict
            );
            return Ok(existing);
        }
        let pending: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND kind=?2 AND state='pending')",
            params![project.to_string(),kind.code()], |row| row.get(0),
        )?;
        self.prune_job_receipts(!pending)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT job FROM service_requests
            WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4",
                params![
                    key.namespace.to_string(),
                    epoch,
                    key.nonce.to_string(),
                    caller.to_string()
                ],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            let existing = record(&transaction, id.parse()?)?;
            ensure!(
                existing.project == project && existing.kind == kind,
                JobError::KeyConflict
            );
            return Ok(existing);
        }
        let (namespace, current): (String, i64) = transaction.query_row(
            "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        ensure!(
            namespace == key.namespace.to_string() && current == epoch,
            JobError::KeyExpired
        );
        let registered: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE binding=?1)",
            [project.to_string()],
            |row| row.get(0),
        )?;
        ensure!(registered, "service project is not registered");
        let requests: i64 =
            transaction.query_row("SELECT count(*) FROM service_requests", [], |row| {
                row.get(0)
            })?;
        ensure!(requests < MAX_REQUESTS, JobError::Capacity);
        // Join pending work only. New source events during an active pass must form later work.
        let pending: Option<String> = transaction
            .query_row(
                "SELECT id FROM service_jobs
            WHERE project=?1 AND kind=?2 AND state='pending' ORDER BY seq LIMIT 1",
                params![project.to_string(), kind.code()],
                |row| row.get(0),
            )
            .optional()?;
        let id = if let Some(id) = pending {
            id.parse()?
        } else {
            let jobs: i64 =
                transaction.query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
            ensure!(jobs < MAX_JOBS, JobError::Capacity);
            let id = framed_digest(
                b"airk-service-job/1",
                &[
                    project.as_bytes(),
                    caller.as_bytes(),
                    key.namespace.as_bytes(),
                    &key.epoch.to_le_bytes(),
                    key.nonce.as_bytes(),
                ],
            );
            transaction.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,?3,'pending',0,0)",
                params![id.to_string(),project.to_string(),kind.code()])?;
            id
        };
        transaction.execute(
            "INSERT INTO service_requests VALUES(?1,?2,?3,?4,?5)",
            params![
                key.namespace.to_string(),
                epoch,
                key.nonce.to_string(),
                caller.to_string(),
                id.to_string()
            ],
        )?;
        let accepted = record(&transaction, id)?;
        transaction.commit()?;
        Ok(accepted)
    }

    pub fn job(&self, id: Sha256Digest) -> Result<JobRecord> {
        record(&self.connection, id)
    }

    /// Call only after acquiring the project lease and persisting its new fencing generation.
    /// Pure reconciliation may retry; interrupted rebuilds require explicit recovery planning.
    pub fn recover_jobs(&mut self, lease: &ProjectLease) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = worker_epoch(&transaction, &self.root, lease)?;
        let failed = serde_json::to_vec(&JobOutcome::Failed {
            code: "interrupted_maintenance_or_retry_exhausted".into(),
        })?;
        transaction.execute(
            "UPDATE service_jobs SET state='failed',outcome=?1 WHERE project=?2 AND state='running'
            AND epoch<>?3 AND (kind='rebuild' OR attempts>=3)",
            params![failed, lease.binding.to_string(), i64::try_from(epoch)?],
        )?;
        transaction.execute(
            "UPDATE service_jobs SET state='pending' WHERE project=?1 AND state='running' AND epoch<>?2",
            params![lease.binding.to_string(),i64::try_from(epoch)?],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn claim_job(&mut self, lease: &ProjectLease) -> Result<Option<JobRecord>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = worker_epoch(&transaction, &self.root, lease)?;
        let paused: bool = transaction.query_row(
            "SELECT paused FROM projects WHERE binding=?1",
            [lease.binding.to_string()],
            |row| row.get(0),
        )?;
        if paused {
            return Ok(None);
        }
        let running: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND state='running')",
            [lease.binding.to_string()],
            |row| row.get(0),
        )?;
        if running {
            return Ok(None);
        }
        let id: Option<String> = transaction.query_row("SELECT id FROM service_jobs WHERE project=?1 AND state='pending' ORDER BY seq LIMIT 1", [lease.binding.to_string()], |row| row.get(0)).optional()?;
        let Some(id) = id else {
            return Ok(None);
        };
        transaction.execute("UPDATE service_jobs SET state='running',epoch=?1,attempts=attempts+1 WHERE id=?2 AND state='pending'",
            params![i64::try_from(epoch)?, id])?;
        let claimed = record(&transaction, id.parse()?)?;
        transaction.commit()?;
        Ok(Some(claimed))
    }

    pub fn complete_job(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
    ) -> Result<JobRecord> {
        if let JobOutcome::Failed { code } = &outcome {
            ensure!(
                !code.is_empty()
                    && code.len() <= 128
                    && code
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "job failures require a bounded content-free code"
            );
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = worker_epoch(&transaction, &self.root, lease)?;
        let current = record(&transaction, id)?;
        ensure!(
            current.project == lease.binding
                && current.fencing_generation == epoch
                && current.state == JobState::Running,
            JobError::StaleWorker
        );
        let state = match outcome {
            JobOutcome::Succeeded { .. } => "succeeded",
            JobOutcome::Failed { .. } => "failed",
            JobOutcome::Canceled => "canceled",
        };
        let bytes = serde_json::to_vec(&outcome)?;
        ensure!(bytes.len() <= 2048, "service job receipt exceeds bound");
        transaction.execute(
            "UPDATE service_jobs SET state=?1,outcome=?2 WHERE id=?3",
            params![state, bytes, id.to_string()],
        )?;
        let completed = record(&transaction, id)?;
        transaction.commit()?;
        Ok(completed)
    }

    /// Cancel queued work only. Running work needs a worker cancellation acknowledgement.
    pub fn cancel_pending(&mut self, id: Sha256Digest) -> Result<JobRecord> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = record(&transaction, id)?;
        ensure!(
            current.state == JobState::Pending,
            "running or completed job requires worker-aware cancellation"
        );
        transaction.execute(
            "UPDATE service_jobs SET state='canceled',outcome=?1 WHERE id=?2",
            params![serde_json::to_vec(&JobOutcome::Canceled)?, id.to_string()],
        )?;
        let canceled = record(&transaction, id)?;
        transaction.commit()?;
        Ok(canceled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::service::ProjectRegistration;

    #[test]
    fn full_queue_preserves_receipts_and_aliases_then_expires_retired_keys() -> Result<()> {
        let root = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([7; 16]);
        let mut registry = ServiceRegistry::open(root.path(), caller)?;
        let project =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&project)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: caller,
        };
        let first = registry.enqueue(project.binding, caller, &key, JobKind::Index)?;
        let tx = registry.connection.transaction()?;
        for n in 0..MAX_JOBS - 1 {
            let id = framed_digest(b"queue-fixture", &[&n.to_le_bytes()]);
            tx.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,'verify','pending',0,0)",
                params![id.to_string(),project.binding.to_string()])?;
        }
        tx.commit()?;
        assert_eq!(
            registry
                .enqueue(project.binding, caller, &key, JobKind::Index)?
                .id,
            first.id
        );
        let alias = RequestKey {
            nonce: Incarnation::from_bytes([8; 16]),
            ..key.clone()
        };
        assert_eq!(
            registry
                .enqueue(project.binding, caller, &alias, JobKind::Index)?
                .id,
            first.id
        );
        let unknown = RequestKey {
            nonce: Incarnation::from_bytes([9; 16]),
            ..key.clone()
        };
        assert!(
            registry
                .enqueue(project.binding, caller, &unknown, JobKind::Rebuild)
                .is_err()
        );
        registry.cancel_pending(first.id)?;
        // Pressure retires the terminal receipt, advancing the key epoch atomically.
        assert!(matches!(
            registry
                .enqueue(project.binding, caller, &unknown, JobKind::Rebuild)
                .unwrap_err()
                .downcast_ref(),
            Some(JobError::KeyExpired)
        ));
        assert!(matches!(
            registry
                .enqueue(project.binding, caller, &key, JobKind::Index)
                .unwrap_err()
                .downcast_ref(),
            Some(JobError::KeyExpired)
        ));
        let (_, fresh_epoch) = registry.request_epoch()?;
        assert!(fresh_epoch > epoch);
        let fresh = RequestKey {
            epoch: fresh_epoch,
            ..unknown
        };
        assert_eq!(
            registry
                .enqueue(project.binding, caller, &fresh, JobKind::Rebuild)?
                .state,
            JobState::Pending
        );
        Ok(())
    }

    #[test]
    fn recovery_never_requeues_live_work_and_stale_completion_cannot_publish_receipts() -> Result<()>
    {
        let root = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([7; 16]);
        let mut registry = ServiceRegistry::open(root.path(), caller)?;
        let project =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&project)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: caller,
        };
        let job = registry.enqueue(project.binding, caller, &key, JobKind::Index)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.claim_job(&lease)?;
        registry.recover_jobs(&lease)?;
        assert_eq!(registry.job(job.id)?.state, JobState::Running);
        assert!(registry.claim_job(&lease)?.is_none());
        drop(lease);
        let mut replacement = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut replacement)?;
        assert!(
            registry
                .complete_job(
                    &replacement,
                    job.id,
                    JobOutcome::Succeeded { revision: None }
                )
                .is_err()
        );
        registry.recover_jobs(&replacement)?;
        let retried = registry
            .claim_job(&replacement)?
            .context("missing recovered job")?;
        assert_eq!(retried.attempts, 2);
        registry.complete_job(
            &replacement,
            job.id,
            JobOutcome::Succeeded { revision: None },
        )?;
        Ok(())
    }

    #[test]
    fn durable_keys_join_pending_work_reject_conflicts_and_preserve_later_changes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([7; 16]);
        let mut registry = ServiceRegistry::open(root.path(), caller)?;
        let registration =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&registration)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([1; 16]),
        };
        let first = registry.enqueue(registration.binding, caller, &key, JobKind::Index)?;
        assert_eq!(
            registry
                .enqueue(registration.binding, caller, &key, JobKind::Index)?
                .id,
            first.id
        );
        assert!(
            registry
                .enqueue(registration.binding, caller, &key, JobKind::Verify)
                .is_err()
        );
        let other = RequestKey {
            nonce: Incarnation::from_bytes([2; 16]),
            ..key.clone()
        };
        assert_eq!(
            registry
                .enqueue(registration.binding, caller, &other, JobKind::Index)?
                .id,
            first.id
        );
        let mut lease = ProjectLease::acquire(root.path(), registration.binding)?;
        registry.admit_worker(&mut lease)?;
        let running = registry.claim_job(&lease)?.context("missing pending job")?;
        assert_eq!(running.id, first.id);
        let later = RequestKey {
            nonce: Incarnation::from_bytes([3; 16]),
            ..key.clone()
        };
        let followup = registry.enqueue(registration.binding, caller, &later, JobKind::Index)?;
        assert_ne!(followup.id, first.id);
        registry.complete_job(&lease, first.id, JobOutcome::Succeeded { revision: None })?;
        drop(lease);
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), caller)?;
        assert_eq!(
            registry
                .enqueue(registration.binding, caller, &key, JobKind::Index)?
                .state,
            JobState::Succeeded
        );
        assert_eq!(registry.job(followup.id)?.state, JobState::Pending);
        Ok(())
    }

    #[test]
    fn restart_requeues_safe_work_but_not_ambiguous_maintenance_and_pause_is_durable() -> Result<()>
    {
        let root = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([7; 16]);
        let mut registry = ServiceRegistry::open(root.path(), caller)?;
        let project =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&project)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: caller,
        };
        let job = registry.enqueue(project.binding, caller, &key, JobKind::Rebuild)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.claim_job(&lease)?;
        drop(lease);
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.recover_jobs(&lease)?;
        assert_eq!(registry.job(job.id)?.state, JobState::Failed);
        let key = RequestKey {
            nonce: Incarnation::from_bytes([3; 16]),
            ..key
        };
        registry.enqueue(project.binding, caller, &key, JobKind::Index)?;
        registry.pause(project.binding, true)?;
        assert!(registry.claim_job(&lease)?.is_none());
        registry.pause(project.binding, false)?;
        assert!(registry.claim_job(&lease)?.is_some());
        Ok(())
    }
}
