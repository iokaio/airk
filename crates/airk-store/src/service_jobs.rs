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

// Unregister changes registration metadata only. It must not require resuming
// source work to remove a paused project. No other configuration bypasses pause.
const RUN_WHILE_PAUSED: &str = "(j.kind IN('inspect','plan_inspect') OR (j.kind='configure' AND EXISTS(SELECT 1 FROM service_configuration c WHERE c.job=j.id AND json_extract(CAST(c.payload AS TEXT),'$.request.configuration.operation') IN('unregister','prepare_derived_cleanup','delete_derived'))))";

pub(super) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS service_jobs(
        seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
        project TEXT NOT NULL REFERENCES projects(binding), kind TEXT NOT NULL,
        state TEXT NOT NULL, epoch INTEGER NOT NULL CHECK(epoch>=0),
        attempts INTEGER NOT NULL CHECK(attempts>=0 AND attempts<=3),
        outcome BLOB CHECK(outcome IS NULL OR length(outcome)<=2048));
    CREATE INDEX IF NOT EXISTS service_jobs_pending ON service_jobs(project,state,seq);
    CREATE TABLE IF NOT EXISTS service_maintenance(
        project TEXT PRIMARY KEY REFERENCES projects(binding),
        observation BLOB NOT NULL CHECK(length(observation)<=4096),
        pending INTEGER,failed INTEGER NOT NULL CHECK(failed IN(0,1)),
        retries INTEGER NOT NULL CHECK(retries BETWEEN 0 AND 7),next_ms INTEGER NOT NULL) WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS service_job_cancellation(
        job TEXT PRIMARY KEY REFERENCES service_jobs(id) ON DELETE CASCADE) WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS service_requests(
        namespace TEXT NOT NULL, epoch INTEGER NOT NULL CHECK(epoch>=0), nonce TEXT NOT NULL,
        caller TEXT NOT NULL, job TEXT NOT NULL REFERENCES service_jobs(id) ON DELETE CASCADE,
        PRIMARY KEY(namespace,epoch,nonce,caller)) WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS service_retry_requests(
        namespace TEXT NOT NULL,epoch INTEGER NOT NULL,nonce TEXT NOT NULL,caller TEXT NOT NULL,
        source_job TEXT NOT NULL CHECK(length(source_job)=64),
        PRIMARY KEY(namespace,epoch,nonce,caller),
        FOREIGN KEY(namespace,epoch,nonce,caller) REFERENCES service_requests(namespace,epoch,nonce,caller) ON DELETE CASCADE) WITHOUT ROWID;";

const MAX_JOBS: i64 = 1024;
const MAX_REQUESTS: i64 = 4096;

#[derive(Default)]
struct EnqueueOptions {
    configuration: Option<Box<airk_core::service_configuration::ConfigurationWork>>,
    background_revision: Option<u64>,
    maintenance_now_ms: Option<u64>,
    source_request: Option<(u64, u64)>,
    manual_retry: Option<Sha256Digest>,
    plan: Option<super::maintenance::PlannedJob>,
}

fn retry_origin(
    connection: &Connection,
    caller: Incarnation,
    key: &RequestKey,
) -> Result<Option<Sha256Digest>> {
    let source: Option<String> = connection.query_row(
        "SELECT source_job FROM service_retry_requests WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4",
        params![key.namespace.to_string(),i64::try_from(key.epoch).map_err(|_|JobError::KeyExpired)?,key.nonce.to_string(),caller.to_string()],
        |row|row.get(0)).optional()?;
    source.map(|id| id.parse()).transpose()
}
fn metadata_pressure(connection: &Connection) -> Result<bool> {
    let pages: i64 = connection.pragma_query_value(None, "page_count", |row| row.get(0))?;
    let free: i64 = connection.pragma_query_value(None, "freelist_count", |row| row.get(0))?;
    let limit: i64 = connection.pragma_query_value(None, "max_page_count", |row| row.get(0))?;
    ensure!(
        pages >= free && free >= 0 && limit > 0,
        "invalid service metadata capacity"
    );
    let margin = (limit / 8).clamp(1, 1024);
    Ok(pages - free + margin >= limit)
}

pub(super) fn record(connection: &Connection, id: Sha256Digest) -> Result<JobRecord> {
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

pub(super) fn worker_epoch(
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
    pub fn job_runs_while_paused(&self, id: Sha256Digest) -> Result<bool> {
        let job = record(&self.connection, id)?;
        Ok(matches!(job.kind, JobKind::Inspect | JobKind::PlanInspect)
            || job.kind == JobKind::Configure
                && super::configuration::work(&self.connection, id)?.is_some_and(|work| {
                    work.request.configuration.is_derived_cleanup()
                        || work.request.configuration
                            == airk_core::service_configuration::ProjectConfiguration::Unregister
                }))
    }
    /// Fair oldest-pending order, bounded independently of the number of saved projects.
    pub fn pending_projects(&self, limit: usize) -> Result<Vec<ProjectBinding>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid pending project page bound"
        );
        let mut statement = self.connection.prepare(
            &format!("SELECT j.project FROM service_jobs j JOIN projects p ON p.binding=j.project WHERE j.state='running' OR (j.state='pending' AND (p.paused=0 OR {RUN_WHILE_PAUSED}) AND {}) GROUP BY j.project ORDER BY min(j.seq) LIMIT ?1",super::maintenance::ELIGIBLE))?;
        let rows = statement.query_map([limit as i64], |row| row.get::<_, String>(0))?;
        rows.map(|row| row?.parse()).collect()
    }
    /// Stable bounded cursor across all registered pending/running projects.
    pub fn pending_project_page(
        &self,
        after: Option<ProjectBinding>,
        limit: usize,
    ) -> Result<Vec<ProjectBinding>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid pending project page bound"
        );
        let mut statement = self.connection.prepare(
            &format!("SELECT DISTINCT j.project FROM service_jobs j JOIN projects p ON p.binding=j.project
             WHERE (j.state='running' OR (j.state='pending' AND (p.paused=0 OR {RUN_WHILE_PAUSED}) AND {}))
             AND (?1 IS NULL OR j.project>?1) ORDER BY j.project LIMIT ?2",super::maintenance::ELIGIBLE),
        )?;
        let rows = statement.query_map(
            params![after.map(|id| id.to_string()), limit as i64],
            |row| row.get::<_, String>(0),
        )?;
        rows.map(|row| row?.parse()).collect()
    }
    pub fn next_pending_job(&self, project: ProjectBinding) -> Result<Option<JobRecord>> {
        let id: Option<String> = self
            .connection
            .query_row(
                &format!("SELECT j.id FROM service_jobs j JOIN projects p ON p.binding=j.project
             WHERE j.project=?1 AND j.state='pending' AND (p.paused=0 OR {RUN_WHILE_PAUSED}) AND {} ORDER BY j.seq LIMIT 1",super::maintenance::ELIGIBLE),
                [project.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        id.map(|id| record(&self.connection, id.parse()?))
            .transpose()
    }
    /// Rotate the request epoch when terminal receipts are retired, so an old unknown key
    /// always expires rather than accidentally replaying its former mutation.
    fn prune_job_receipts(
        &mut self,
        needs_job: bool,
        protected: Option<Sha256Digest>,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let jobs: i64 =
            transaction.query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
        let requests: i64 =
            transaction.query_row("SELECT count(*) FROM service_requests", [], |row| {
                row.get(0)
            })?;
        let count_full = (needs_job && jobs >= MAX_JOBS) || requests >= MAX_REQUESTS;
        if count_full || metadata_pressure(&transaction)? {
            let deleted = transaction.execute("DELETE FROM service_jobs WHERE seq IN(
                SELECT seq FROM service_jobs WHERE state IN('succeeded','failed','canceled') AND (?1 IS NULL OR id<>?1) ORDER BY seq LIMIT 64)", [protected.map(|id|id.to_string())])?;
            ensure!(deleted > 0 || !count_full, JobError::Capacity);
            if deleted > 0 {
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
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn request_epoch(&mut self) -> Result<(Incarnation, u64)> {
        self.prune_job_receipts(false, None)?;
        self.saved_request_epoch()
    }

    /// Observe an existing epoch without rotating or pruning accepted receipts.
    pub fn saved_request_epoch(&self) -> Result<(Incarnation, u64)> {
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
        self.enqueue_impl(project, caller, key, kind, EnqueueOptions::default())
    }

    /// A deliberate new request preserves the terminal job, joins existing pending
    /// work or creates a fresh bounded job. Its key also binds the original job.
    pub fn retry_job(
        &mut self,
        id: Sha256Digest,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<JobRecord> {
        let accepted: Option<String> = self.connection.query_row(
            "SELECT job FROM service_requests WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4",
            params![key.namespace.to_string(),i64::try_from(key.epoch).map_err(|_|JobError::KeyExpired)?,key.nonce.to_string(),caller.to_string()],
            |row|row.get(0)).optional()?;
        if let Some(accepted) = accepted {
            ensure!(
                retry_origin(&self.connection, caller, key)? == Some(id),
                JobError::KeyConflict
            );
            return record(&self.connection, accepted.parse()?);
        }
        let current_epoch: i64 = self.connection.query_row(
            "SELECT request_epoch FROM service_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        ensure!(
            key.namespace == self.incarnation()? && key.epoch == u64::try_from(current_epoch)?,
            JobError::KeyExpired
        );
        let original = record(&self.connection, id)?;
        ensure!(
            matches!(original.state, JobState::Failed | JobState::Canceled),
            JobError::NotRetryable
        );
        self.enqueue_impl(
            original.project,
            caller,
            key,
            original.kind,
            EnqueueOptions {
                manual_retry: Some(id),
                configuration: self.configuration_work(id)?.map(Box::new),
                ..Default::default()
            },
        )
    }

    pub fn enqueue_configuration(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        key: &RequestKey,
        request: &airk_core::service_configuration::ConfigurationRequest,
        engine: Sha256Digest,
    ) -> Result<JobRecord> {
        request.validate()?;
        let request: airk_core::service_configuration::ConfigurationRequest =
            serde_json::from_slice(&serde_json::to_vec(request)?)?;
        self.enqueue_impl(
            project,
            caller,
            key,
            JobKind::Configure,
            EnqueueOptions {
                configuration: Some(Box::new(
                    airk_core::service_configuration::ConfigurationWork { engine, request },
                )),
                ..Default::default()
            },
        )
    }

    fn enqueue_impl(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        key: &RequestKey,
        kind: JobKind,
        options: EnqueueOptions,
    ) -> Result<JobRecord> {
        let EnqueueOptions {
            configuration,
            background_revision,
            maintenance_now_ms,
            source_request,
            manual_retry,
            plan,
        } = options;
        ensure!(
            (kind == JobKind::Configure) == configuration.is_some(),
            "configuration jobs require a complete typed request"
        );
        ensure!(
            plan.is_some() || !matches!(kind, JobKind::PlanInspect | JobKind::Repair),
            airk_core::service_maintenance::PlanError::InvalidTransition
        );
        let epoch = i64::try_from(key.epoch).map_err(|_| JobError::KeyExpired)?;
        // Accepted keys remain readable even when every queue slot is occupied.
        let existing: Option<String> = self.connection.query_row(
            "SELECT job FROM service_requests WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4",
            params![key.namespace.to_string(),epoch,key.nonce.to_string(),caller.to_string()],
            |row| row.get(0),
        ).optional()?;
        if let Some(id) = existing {
            ensure!(
                retry_origin(&self.connection, caller, key)? == manual_retry,
                JobError::KeyConflict
            );
            let existing = record(&self.connection, id.parse()?)?;
            super::configuration::same_request(
                &self.connection,
                existing.id,
                configuration.as_deref(),
            )?;
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
        self.prune_job_receipts(!pending || plan.is_some(), manual_retry)?;
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
            ensure!(
                retry_origin(&transaction, caller, key)? == manual_retry,
                JobError::KeyConflict
            );
            let existing = record(&transaction, id.parse()?)?;
            super::configuration::same_request(
                &transaction,
                existing.id,
                configuration.as_deref(),
            )?;
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
        let derived = configuration
            .as_deref()
            .is_some_and(|work| work.request.configuration.is_derived_cleanup());
        if let Some(work) = configuration.as_deref().filter(|_| derived) {
            ensure!(
                work.request.incarnation.to_string() == namespace
                    && work.request.settings == project,
                "derived cleanup authority does not match this registry and project"
            );
        }
        let registered: Option<bool> = transaction
            .query_row(
                "SELECT registered FROM projects WHERE binding=?1",
                [project.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        ensure!(
            registered == Some(!derived),
            if derived {
                "unregister the project before derived cleanup"
            } else {
                "service project is not registered"
            }
        );
        ensure!(!metadata_pressure(&transaction)?, JobError::Capacity);
        let planned_source = plan
            .map(|link| super::maintenance::prepare_job(&transaction, link, project, kind))
            .transpose()?
            .unwrap_or(false);
        if let Some(source) = manual_retry {
            let original = record(&transaction, source)?;
            ensure!(
                original.project == project
                    && original.kind == kind
                    && matches!(original.state, JobState::Failed | JobState::Canceled),
                JobError::NotRetryable
            );
        }
        if maintenance_now_ms.is_some() || source_request.is_some() {
            let busy: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND state IN('pending','running'))",
                [project.to_string()], |row| row.get(0))?;
            ensure!(!busy, "project already has admitted work");
        }
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
        let pending = pending
            .map(|id| -> Result<Option<String>> {
                let saved = super::configuration::work(&transaction, id.parse()?)?;
                Ok((saved.as_ref() == configuration.as_deref()).then_some(id))
            })
            .transpose()?
            .flatten();
        let id = if let Some(id) = pending.filter(|_| plan.is_none()) {
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
            if let Some(configuration) = &configuration {
                super::configuration::insert(&transaction, id, configuration)?;
            }
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
        if let Some(source) = manual_retry {
            transaction.execute(
                "INSERT INTO service_retry_requests VALUES(?1,?2,?3,?4,?5)",
                params![
                    key.namespace.to_string(),
                    epoch,
                    key.nonce.to_string(),
                    caller.to_string(),
                    source.to_string()
                ],
            )?;
            if kind == JobKind::Maintain {
                transaction.execute(
                    "UPDATE service_maintenance SET retries=0,next_ms=0 WHERE project=?1",
                    [project.to_string()],
                )?;
            }
        }
        if let Some(revision) = background_revision {
            super::background::debit(&transaction, project, kind, revision, id)?;
        }
        if let Some(now_ms) = maintenance_now_ms {
            // Admission and retry debit share this transaction. Replaying an accepted
            // key returns above, without consuming another attempt after a crash.
            let retries: u32 = transaction.query_row(
                "SELECT m.retries FROM service_maintenance m JOIN projects p ON p.binding=m.project
                 WHERE m.project=?1 AND p.paused=0 AND (m.pending>0 OR m.failed=1)
                 AND m.retries<CASE WHEN m.failed=1 THEN 3 ELSE 7 END AND m.next_ms<=?2
                 AND NOT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND id<>?3 AND state IN('pending','running'))",
                params![project.to_string(), i64::try_from(now_ms)?, id.to_string()],
                |row| row.get(0),
            ).context("maintenance is no longer eligible")?;
            let next = now_ms
                .checked_add((1u64 << (retries + 1)) * 1000)
                .context("maintenance deadline overflow")?;
            transaction.execute(
                "UPDATE service_maintenance SET retries=retries+1,next_ms=?1 WHERE project=?2",
                params![i64::try_from(next)?, project.to_string()],
            )?;
        }
        if matches!(kind, JobKind::Index | JobKind::Reconcile | JobKind::Rebuild) || planned_source
        {
            super::watch::ensure_row(&transaction, project)?;
            if manual_retry.is_some() {
                transaction.execute("UPDATE service_watch SET attempts=0,next_ms=0,blocked_revision=NULL WHERE project=?1",[project.to_string()])?;
            }
            if let Some((revision, now_ms)) = source_request {
                let attempts:u32=transaction.query_row("SELECT w.attempts FROM service_watch w JOIN projects p ON p.binding=w.project
                    WHERE w.project=?1 AND w.enabled=1 AND p.paused=0 AND w.requested=?2 AND w.requested>w.indexed
                    AND w.attempts<3 AND (w.blocked_revision IS NULL OR w.blocked_revision<w.requested) AND w.next_ms<=?3",
                    params![project.to_string(),i64::try_from(revision)?,i64::try_from(now_ms)?],|row|row.get(0))
                    .context("source reconciliation is no longer eligible")?;
                let next = now_ms
                    .checked_add((1u64 << (attempts + 1)) * 1000)
                    .context("source retry deadline exhausted")?;
                transaction.execute(
                    "UPDATE service_watch SET attempts=attempts+1,next_ms=?1 WHERE project=?2",
                    params![i64::try_from(next)?, project.to_string()],
                )?;
            }
            transaction.execute("INSERT INTO service_job_source(job,revision) SELECT ?1,requested FROM service_watch WHERE project=?2
                ON CONFLICT(job) DO UPDATE SET revision=max(service_job_source.revision,excluded.revision)",params![id.to_string(),project.to_string()])?;
        }
        if let Some(link) = plan {
            super::maintenance::associate_job(&transaction, link, id)?;
        }
        let accepted = record(&transaction, id)?;
        transaction.commit()?;
        Ok(accepted)
    }

    pub fn record_maintenance(
        &mut self,
        lease: &ProjectLease,
        observation: &airk_core::service::MaintenanceObservation,
    ) -> Result<()> {
        observation.validate()?;
        let bytes = serde_json::to_vec(observation)?;
        ensure!(bytes.len() <= 4096, "maintenance observation too large");
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        worker_epoch(&tx, &self.root, lease)?;
        let clean = observation.pending_incarnations == Some(0) && !observation.failed;
        tx.execute("INSERT INTO service_maintenance(project,observation,pending,failed,retries,next_ms) VALUES(?1,?2,?3,?4,0,0)
            ON CONFLICT(project) DO UPDATE SET observation=excluded.observation,pending=excluded.pending,failed=excluded.failed,
            retries=CASE WHEN ?5 THEN 0 ELSE service_maintenance.retries END,next_ms=CASE WHEN ?5 THEN 0 ELSE service_maintenance.next_ms END",
            params![lease.binding.to_string(),bytes,observation.pending_incarnations.map(|count|count as i64),observation.failed,clean])?;
        tx.commit()?;
        Ok(())
    }
    pub fn maintenance_observation(
        &self,
        project: ProjectBinding,
    ) -> Result<Option<airk_core::service::MaintenanceObservation>> {
        let bytes: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT substr(observation,1,4097) FROM service_maintenance WHERE project=?1",
                [project.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        bytes
            .map(|bytes| {
                ensure!(bytes.len() <= 4096, "maintenance observation too large");
                let observation: airk_core::service::MaintenanceObservation =
                    serde_json::from_slice(&bytes)?;
                observation.validate()?;
                Ok(observation)
            })
            .transpose()
    }
    pub fn maintenance_retrying(&self, project: ProjectBinding) -> Result<bool> {
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM service_maintenance m JOIN projects p ON p.binding=m.project WHERE m.project=?1 AND p.paused=0 AND (m.pending>0 OR m.failed=1) AND m.retries<CASE WHEN m.failed=1 THEN 3 ELSE 7 END)",[project.to_string()],|row|row.get(0))?)
    }
    pub fn due_maintenance(&self, now_ms: u64) -> Result<Vec<ProjectBinding>> {
        let mut statement=self.connection.prepare("SELECT m.project FROM service_maintenance m JOIN projects p ON p.binding=m.project WHERE p.paused=0 AND (m.pending>0 OR m.failed=1) AND m.retries<CASE WHEN m.failed=1 THEN 3 ELSE 7 END AND m.next_ms<=?1 AND NOT EXISTS(SELECT 1 FROM service_jobs j WHERE j.project=m.project AND j.state IN('pending','running')) ORDER BY m.next_ms,m.project LIMIT 32")?;
        let rows = statement.query_map([i64::try_from(now_ms)?], |row| row.get::<_, String>(0))?;
        rows.map(|row| row?.parse()).collect()
    }
    /// Atomically accept one eligible automatic pass and consume its durable retry.
    pub fn enqueue_due_maintenance(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        nonce: Incarnation,
        now_ms: u64,
    ) -> Result<JobRecord> {
        let (namespace, epoch) = self.request_epoch()?;
        self.enqueue_impl(
            project,
            caller,
            &RequestKey {
                namespace,
                epoch,
                nonce,
            },
            JobKind::Maintain,
            EnqueueOptions {
                maintenance_now_ms: Some(now_ms),
                ..Default::default()
            },
        )
    }
    /// Automatic source work captures the exact requested revision and debits its
    /// retry request atomically with durable acceptance. Exact key replay never debits again.
    pub fn enqueue_source_reconcile(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        nonce: Incarnation,
        revision: u64,
        now_ms: u64,
    ) -> Result<JobRecord> {
        let (namespace, epoch) = self.request_epoch()?;
        self.enqueue_impl(
            project,
            caller,
            &RequestKey {
                namespace,
                epoch,
                nonce,
            },
            JobKind::Reconcile,
            EnqueueOptions {
                source_request: Some((revision, now_ms)),
                ..Default::default()
            },
        )
    }

    /// Internal desired-state requests use the same bounded durable receipts and
    /// pending-kind joining as trusted control clients; this never claims work.
    pub fn enqueue_internal(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        kind: JobKind,
        nonce: Incarnation,
    ) -> Result<JobRecord> {
        let (namespace, epoch) = self.request_epoch()?;
        self.enqueue(
            project,
            caller,
            &RequestKey {
                namespace,
                epoch,
                nonce,
            },
            kind,
        )
    }
    pub(super) fn enqueue_planned(
        &mut self,
        link: super::maintenance::PlannedJob,
    ) -> Result<JobRecord> {
        let (namespace, epoch) = self.request_epoch()?;
        let step = match link.step {
            super::maintenance::PlanStep::Inspect => b"inspect".as_slice(),
            super::maintenance::PlanStep::Action => b"action".as_slice(),
        };
        let digest = framed_digest(b"maintenance-job-caller/1.4", &[link.plan.as_bytes()]);
        let caller = Incarnation::from_bytes(digest.as_bytes()[..16].try_into()?);
        let nonce = framed_digest(
            b"maintenance-job-request/1.4",
            &[
                link.plan.as_bytes(),
                link.project.as_bytes(),
                step,
                &link.now_ms.to_le_bytes(),
            ],
        );
        self.enqueue_impl(
            link.project,
            caller,
            &RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes(nonce.as_bytes()[..16].try_into()?),
            },
            match link.step {
                super::maintenance::PlanStep::Inspect => JobKind::PlanInspect,
                super::maintenance::PlanStep::Action => JobKind::Repair,
            },
            EnqueueOptions {
                plan: Some(link),
                ..Default::default()
            },
        )
    }
    pub fn job(&self, id: Sha256Digest) -> Result<JobRecord> {
        record(&self.connection, id)
    }

    /// Call only after acquiring the project lease and persisting its new fencing generation.
    /// Pure reconciliation may retry; interrupted rebuilds require explicit recovery planning.
    pub fn recover_jobs(&mut self, lease: &ProjectLease) -> Result<()> {
        self.recover_jobs_with_rebuild_receipt(lease, None)
    }
    pub fn enqueue_background_job(
        &mut self,
        project: ProjectBinding,
        revision: u64,
        kind: JobKind,
        caller: Incarnation,
        nonce: Incarnation,
    ) -> Result<JobRecord> {
        ensure!(
            matches!(kind, JobKind::Index | JobKind::Vectors),
            "unsupported background continuation"
        );
        let (namespace, epoch) = self.request_epoch()?;
        self.enqueue_impl(
            project,
            caller,
            &RequestKey {
                namespace,
                epoch,
                nonce,
            },
            kind,
            EnqueueOptions {
                background_revision: Some(revision),
                ..Default::default()
            },
        )
    }
    pub fn recover_jobs_with_rebuild_receipt(
        &mut self,
        lease: &ProjectLease,
        receipt: Option<&crate::service::collection::RebuildReceipt>,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = worker_epoch(&transaction, &self.root, lease)?;
        if let Some(receipt) = receipt {
            let outcome = serde_json::to_vec(&JobOutcome::Succeeded {
                revision: Some(airk_core::identity::RevisionToken {
                    incarnation: receipt.token.incarnation,
                    generation: receipt.token.sequence,
                    revision: receipt.token.digest,
                }),
            })?;
            let completed=transaction.execute("UPDATE service_jobs SET state='succeeded',outcome=?1 WHERE id=?2 AND project=?3 AND kind IN('rebuild','repair') AND state='running' AND epoch<>?4",
                params![outcome,receipt.job.to_string(),lease.binding.to_string(),i64::try_from(epoch)?])?;
            if completed == 1 {
                // The manifest switch may precede the worker's continuation
                // receipt. Recover both in this transaction; a ready model
                // simply completes a no-op semantic turn without loading it.
                super::background::set(&transaction, lease.binding, JobKind::Vectors, true)?;
            }
        }
        let failed = serde_json::to_vec(&JobOutcome::Failed {
            code: "interrupted_maintenance_or_retry_exhausted".into(),
        })?;
        let uncertain = serde_json::to_vec(&JobOutcome::Failed {
            code: "configuration_outcome_unknown_reinspect".into(),
        })?;
        super::configuration::recover_results(&transaction, lease, epoch)?;
        transaction.execute("UPDATE service_jobs SET state='pending' WHERE project=?1 AND kind='configure' AND state='running' AND epoch<>?2 AND attempts<3 AND id IN(SELECT job FROM service_configuration WHERE json_extract(CAST(payload AS TEXT),'$.request.configuration.operation') IN('prepare_derived_cleanup','delete_derived'))",
            params![lease.binding.to_string(),i64::try_from(epoch)?])?;
        transaction.execute("UPDATE service_jobs SET state='failed',outcome=?1 WHERE project=?2 AND kind='configure' AND state='running' AND epoch<>?3",
            params![uncertain,lease.binding.to_string(),i64::try_from(epoch)?])?;
        transaction.execute(
            "UPDATE service_jobs SET state='failed',outcome=?1 WHERE project=?2 AND state='running'
            AND epoch<>?3 AND (kind IN('rebuild','repair') OR attempts>=3)",
            params![failed, lease.binding.to_string(), i64::try_from(epoch)?],
        )?;
        transaction.execute(
            "UPDATE service_jobs SET state='pending' WHERE project=?1 AND state='running' AND epoch<>?2",
            params![lease.binding.to_string(),i64::try_from(epoch)?],
        )?;
        super::maintenance::settle_project_jobs(
            &transaction,
            lease.binding,
            super::maintenance::clock_ms()?,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn claim_job(&mut self, lease: &ProjectLease) -> Result<Option<JobRecord>> {
        self.claim_selected_job(lease, None)
    }
    /// Claim only the exact service-granted job, without substituting another request.
    pub fn claim_job_id(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
    ) -> Result<Option<JobRecord>> {
        self.claim_selected_job(lease, Some(id))
    }
    fn claim_selected_job(
        &mut self,
        lease: &ProjectLease,
        selected: Option<Sha256Digest>,
    ) -> Result<Option<JobRecord>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = worker_epoch(&transaction, &self.root, lease)?;
        let paused: bool = transaction.query_row(
            "SELECT paused FROM projects WHERE binding=?1",
            [lease.binding.to_string()],
            |row| row.get(0),
        )?;
        let running: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND state='running')",
            [lease.binding.to_string()],
            |row| row.get(0),
        )?;
        if running {
            return Ok(None);
        }
        let id: Option<String> = transaction.query_row(&format!("SELECT j.id FROM service_jobs j WHERE j.project=?1 AND j.state='pending' AND (?2 IS NULL OR j.id=?2) AND (NOT ?3 OR {RUN_WHILE_PAUSED}) AND {} ORDER BY j.seq LIMIT 1",super::maintenance::ELIGIBLE), params![lease.binding.to_string(), selected.map(|id|id.to_string()),paused], |row| row.get(0)).optional()?;
        let Some(id) = id else {
            return Ok(None);
        };
        let derived = super::configuration::work(&transaction, id.parse()?)?
            .is_some_and(|work| work.request.configuration.is_derived_cleanup());
        let registered: bool = transaction.query_row(
            "SELECT registered FROM projects WHERE binding=?1",
            [lease.binding.to_string()],
            |row| row.get(0),
        )?;
        ensure!(
            derived == lease.derived && registered != derived,
            "project job belongs to a different worker role"
        );
        transaction.execute("UPDATE service_jobs SET state='running',epoch=?1,attempts=attempts+1 WHERE id=?2 AND state='pending'",
            params![i64::try_from(epoch)?, id])?;
        let claimed = record(&transaction, id.parse()?)?;
        if matches!(
            claimed.kind,
            JobKind::Index | JobKind::Reconcile | JobKind::Rebuild
        ) {
            // An explicit source pass can satisfy an already requested model
            // follow-up too. Capture that intent at claim, preserving any new
            // request that arrives while the pass is running.
            super::background::set(&transaction, lease.binding, JobKind::Index, false)?;
        }
        super::maintenance::job_claimed(&transaction, claimed.id)?;
        transaction.commit()?;
        Ok(Some(claimed))
    }

    pub fn complete_job(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
    ) -> Result<JobRecord> {
        self.complete_job_observed(lease, id, outcome, None, None)
    }

    pub fn complete_job_at(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: u64,
    ) -> Result<JobRecord> {
        self.complete_job_observed(lease, id, outcome, Some(now_ms), None)
    }

    pub fn complete_job_with_maintenance_at(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: u64,
        maintenance: Option<&airk_core::service::MaintenanceObservation>,
    ) -> Result<JobRecord> {
        self.complete_job_observed(lease, id, outcome, Some(now_ms), maintenance)
    }

    /// The supervisor may settle work that never started only while an exclusive
    /// unadmitted project lease proves that no project owner survives.
    pub fn fail_pending_start(&mut self, lease: &ProjectLease, code: &str) -> Result<usize> {
        ensure!(
            lease.root == self.root && lease.epoch.is_none(),
            "startup failure requires unadmitted project ownership"
        );
        ensure!(
            !code.is_empty()
                && code.len() <= 128
                && code
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
            "invalid startup failure code"
        );
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let running: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND state='running')",
            [lease.binding.to_string()],
            |row| row.get(0),
        )?;
        ensure!(
            !running,
            "interrupted running work requires fenced recovery before startup failure settlement"
        );
        let bytes = serde_json::to_vec(&JobOutcome::Failed { code: code.into() })?;
        let failed=tx.execute("UPDATE service_jobs SET state='failed',outcome=?1 WHERE project=?2 AND state='pending'",params![bytes,lease.binding.to_string()])?;
        super::maintenance::settle_project_jobs(
            &tx,
            lease.binding,
            super::maintenance::clock_ms()?,
        )?;
        tx.commit()?;
        Ok(failed)
    }

    fn complete_job_observed(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: Option<u64>,
        maintenance: Option<&airk_core::service::MaintenanceObservation>,
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
        let source_revision: Option<i64> = transaction
            .query_row(
                "SELECT revision FROM service_job_source WHERE job=?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(revision) = source_revision {
            match &outcome {
                JobOutcome::Succeeded { .. } => {
                    ensure!(transaction.execute("UPDATE service_watch SET indexed=max(indexed,?1),last_reconciled_ms=coalesce(?2,last_reconciled_ms)
                        WHERE project=?3 AND requested>=?1",params![revision,now_ms.map(i64::try_from).transpose()?,lease.binding.to_string()])?==1,"source completion revision exceeds desired state");
                }
                JobOutcome::Canceled => {
                    super::watch::block_source_retry(&transaction, id)?;
                }
                JobOutcome::Failed { .. } => {}
            }
        }
        let canceled: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_job_cancellation WHERE job=?1)",
            [id.to_string()],
            |row| row.get(0),
        )?;
        if canceled || super::maintenance::job_control(&transaction, id)?.1 {
            // Cancellation can arrive after atomic publication but before this
            // receipt. Preserve the successful publication and stop its next
            // phase in the same transaction that acknowledges completion.
            let next = match current.kind {
                JobKind::ModelSetup => Some(JobKind::Index),
                JobKind::Index
                | JobKind::Reconcile
                | JobKind::Rebuild
                | JobKind::Repair
                | JobKind::Recover
                | JobKind::Vectors => Some(JobKind::Vectors),
                _ => None,
            };
            if let Some(kind) = next {
                super::background::set(&transaction, current.project, kind, false)?;
            }
        }
        let completed = record(&transaction, id)?;
        super::maintenance::job_completed(
            &transaction,
            id,
            match now_ms {
                Some(now) => now,
                None => super::maintenance::clock_ms()?,
            },
            maintenance,
        )?;
        if matches!(outcome, JobOutcome::Succeeded { .. }) {
            super::unregistration::finish_if_requested(&transaction, current.project, id)?;
            super::cleanup::finish_if_requested(&transaction, current.project, id)?;
        }
        transaction.commit()?;
        Ok(completed)
    }

    /// Persist running cancellation intent; completion still requires the fenced owner.
    pub fn request_cancellation(&mut self, id: Sha256Digest) -> Result<JobRecord> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = record(&transaction, id)?;
        if current.kind == JobKind::Maintain
            && matches!(current.state, JobState::Pending | JobState::Running)
        {
            transaction.execute(
                "UPDATE service_maintenance SET retries=7 WHERE project=?1",
                [current.project.to_string()],
            )?;
        }
        if matches!(current.state, JobState::Pending | JobState::Running) {
            super::watch::block_source_retry(&transaction, id)?;
        }
        match current.state {
            JobState::Pending => {
                transaction.execute(
                    "UPDATE service_jobs SET state='canceled',outcome=?1 WHERE id=?2",
                    params![serde_json::to_vec(&JobOutcome::Canceled)?, id.to_string()],
                )?;
            }
            JobState::Running => {
                transaction.execute(
                    "INSERT OR IGNORE INTO service_job_cancellation(job) VALUES(?1)",
                    [id.to_string()],
                )?;
            }
            _ => {}
        }
        let result = record(&transaction, id)?;
        super::maintenance::job_completed(&transaction, id, super::maintenance::clock_ms()?, None)?;
        transaction.commit()?;
        Ok(result)
    }
    pub fn cancellation_requested(&self, id: Sha256Digest) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_job_cancellation WHERE job=?1)",
            [id.to_string()],
            |row| row.get(0),
        )?)
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
        if current.kind == JobKind::Maintain {
            transaction.execute(
                "UPDATE service_maintenance SET retries=7 WHERE project=?1",
                [current.project.to_string()],
            )?;
        }
        transaction.execute(
            "UPDATE service_jobs SET state='canceled',outcome=?1 WHERE id=?2",
            params![serde_json::to_vec(&JobOutcome::Canceled)?, id.to_string()],
        )?;
        let canceled = record(&transaction, id)?;
        super::maintenance::job_completed(&transaction, id, super::maintenance::clock_ms()?, None)?;
        if current.state == JobState::Pending {
            super::watch::block_source_retry(&transaction, id)?;
        }
        transaction.commit()?;
        Ok(canceled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::service::ProjectRegistration;

    #[test]
    fn full_queue_rotation_protects_the_failure_named_by_a_new_retry() -> Result<()> {
        let root = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([6; 16]);
        let mut registry = ServiceRegistry::open(root.path(), caller)?;
        let project =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&project)?;
        let original = registry.enqueue_internal(
            project.binding,
            caller,
            JobKind::Verify,
            Incarnation::from_bytes([7; 16]),
        )?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.claim_job_id(&lease, original.id)?;
        registry.complete_job(
            &lease,
            original.id,
            JobOutcome::Failed {
                code: "verification_failed".into(),
            },
        )?;
        {
            let tx = registry.connection.transaction()?;
            let outcome = serde_json::to_vec(&JobOutcome::Succeeded { revision: None })?;
            for number in 0..MAX_JOBS - 1 {
                let id = framed_digest(b"fictional-capacity-fixture", &[&number.to_le_bytes()]);
                tx.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts,outcome) VALUES(?1,?2,'verify','succeeded',0,0,?3)",
                    params![id.to_string(),project.binding.to_string(),outcome])?;
            }
            tx.commit()?;
        }
        let (namespace, epoch) = registry.request_epoch()?;
        let mut key = RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([8; 16]),
        };
        assert_eq!(
            registry
                .retry_job(original.id, caller, &key)
                .unwrap_err()
                .downcast_ref::<JobError>(),
            Some(&JobError::KeyExpired)
        );
        assert_eq!(
            registry.job(original.id)?.state,
            JobState::Failed,
            "rotation deleted the failure being retried"
        );
        key.epoch = registry.request_epoch()?.1;
        let accepted = registry.retry_job(original.id, caller, &key)?;
        assert_ne!(accepted.id, original.id);
        assert_eq!(registry.job(original.id)?.state, JobState::Failed);
        assert_eq!(
            registry.retry_job(original.id, caller, &key)?.id,
            accepted.id
        );
        Ok(())
    }

    #[test]
    fn manual_retry_preserves_failed_jobs_resets_source_policy_once_and_binds_the_original()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([7; 16]);
        let mut registry = ServiceRegistry::open(root.path(), caller)?;
        let project =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&project)?;
        registry.configure_watch(project.binding, true)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        let mut failures = Vec::new();
        for attempt in 1..=3u8 {
            let job = registry.enqueue_source_reconcile(
                project.binding,
                caller,
                Incarnation::from_bytes([attempt; 16]),
                1,
                u64::from(attempt) * 1_000_000,
            )?;
            registry.claim_job_id(&lease, job.id)?;
            registry.complete_job(
                &lease,
                job.id,
                JobOutcome::Failed {
                    code: "source_unavailable".into(),
                },
            )?;
            failures.push(job.id);
        }
        assert!(
            registry
                .watch(project.binding)?
                .observation
                .automatic_retries_exhausted
        );
        registry.pause(project.binding, true)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([9; 16]),
        };
        let original = failures[2];
        let retried = registry.retry_job(original, caller, &key)?;
        assert_ne!(retried.id, original);
        assert_eq!(retried.state, JobState::Pending);
        assert_eq!(retried.attempts, 0);
        assert_eq!(registry.job(original)?.state, JobState::Failed);
        assert_eq!(
            registry.job(original)?.outcome,
            Some(JobOutcome::Failed {
                code: "source_unavailable".into()
            })
        );
        assert!(registry.get(project.binding)?.paused);
        assert!(
            !registry
                .watch(project.binding)?
                .observation
                .automatic_retries_exhausted
        );
        let fresh = RequestKey {
            nonce: Incarnation::from_bytes([10; 16]),
            ..key.clone()
        };
        assert_eq!(
            registry
                .retry_job(retried.id, caller, &fresh)
                .unwrap_err()
                .downcast_ref::<JobError>(),
            Some(&JobError::NotRetryable)
        );
        registry.cancel_pending(retried.id)?;
        assert!(
            registry
                .watch(project.binding)?
                .observation
                .automatic_retry_canceled
        );
        assert_eq!(registry.retry_job(original, caller, &key)?.id, retried.id);
        assert!(
            registry
                .watch(project.binding)?
                .observation
                .automatic_retry_canceled,
            "replayed retry reset later cancellation"
        );
        assert_eq!(
            registry
                .retry_job(failures[0], caller, &key)
                .unwrap_err()
                .downcast_ref::<JobError>(),
            Some(&JobError::KeyConflict)
        );
        assert_eq!(
            registry
                .enqueue(project.binding, caller, &key, JobKind::Reconcile)
                .unwrap_err()
                .downcast_ref::<JobError>(),
            Some(&JobError::KeyConflict)
        );
        registry.connection.execute(
            "DELETE FROM service_jobs WHERE id=?1",
            [original.to_string()],
        )?;
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), caller)?;
        assert_eq!(
            registry.retry_job(original, caller, &key)?.id,
            retried.id,
            "accepted retry was lost when its original receipt rotated"
        );
        assert!(
            registry
                .watch(project.binding)?
                .observation
                .automatic_retry_canceled
        );
        let events = registry.events(None, None, 32)?;
        assert_eq!(
            events
                .events
                .iter()
                .filter(|e| e.kind == airk_core::service_events::EventKind::JobRetryRequested)
                .count(),
            1
        );
        Ok(())
    }

    #[test]
    fn manual_retry_reopens_exhausted_maintenance_without_erasing_failure_or_replaying_reset()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([3; 16]);
        let mut registry = ServiceRegistry::open(root.path(), caller)?;
        let project =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&project)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.record_maintenance(
            &lease,
            &airk_core::service::MaintenanceObservation {
                pending_incarnations: Some(1),
                removed_files: 0,
                failed: true,
                observed_unix_ms: 1,
            },
        )?;
        let mut last = None;
        for attempt in 1..=3u8 {
            let job = registry.enqueue_due_maintenance(
                project.binding,
                caller,
                Incarnation::from_bytes([attempt; 16]),
                u64::from(attempt) * 1_000_000,
            )?;
            registry.claim_job_id(&lease, job.id)?;
            registry.complete_job(
                &lease,
                job.id,
                JobOutcome::Failed {
                    code: "retirement_failed".into(),
                },
            )?;
            last = Some(job.id);
        }
        assert!(!registry.maintenance_retrying(project.binding)?);
        let (namespace, epoch) = registry.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([12; 16]),
        };
        let original = last.unwrap();
        let retried = registry.retry_job(original, caller, &key)?;
        assert!(registry.maintenance_retrying(project.binding)?);
        assert_eq!(registry.job(original)?.state, JobState::Failed);
        registry.cancel_pending(retried.id)?;
        assert!(!registry.maintenance_retrying(project.binding)?);
        registry.retry_job(original, caller, &key)?;
        assert!(!registry.maintenance_retrying(project.binding)?);
        Ok(())
    }

    #[test]
    fn maintenance_backoff_is_durable_bounded_and_does_not_consume_paused_or_queued_attempts()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let instance = Incarnation::from_bytes([7; 16]);
        let mut registry = ServiceRegistry::open(root.path(), instance)?;
        let project =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&project)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        let mut observation = airk_core::service::MaintenanceObservation {
            pending_incarnations: Some(1),
            removed_files: 0,
            failed: false,
            observed_unix_ms: 1000,
        };
        registry.record_maintenance(&lease, &observation)?;
        registry.pause(project.binding, true)?;
        assert!(registry.due_maintenance(1_000_000)?.is_empty());
        assert!(
            registry
                .enqueue_due_maintenance(
                    project.binding,
                    instance,
                    Incarnation::from_bytes([42; 16]),
                    1000
                )
                .is_err()
        );
        registry.pause(project.binding, false)?;
        let canceled = registry.enqueue_internal(
            project.binding,
            instance,
            JobKind::Maintain,
            Incarnation::from_bytes([0; 16]),
        )?;
        registry.cancel_pending(canceled.id)?;
        assert!(
            registry.due_maintenance(1_000_000)?.is_empty(),
            "canceled maintenance restarted automatically"
        );
        observation.pending_incarnations = Some(0);
        registry.record_maintenance(&lease, &observation)?;
        observation.pending_incarnations = Some(1);
        registry.record_maintenance(&lease, &observation)?;
        for n in 1..=7u8 {
            let now = u64::from(n) * 1_000_000;
            assert_eq!(registry.due_maintenance(now)?, vec![project.binding]);
            let nonce = Incarnation::from_bytes([n; 16]);
            let job = registry.enqueue_due_maintenance(project.binding, instance, nonce, now)?;
            assert_eq!(
                registry
                    .enqueue_due_maintenance(project.binding, instance, nonce, now)?
                    .id,
                job.id
            );
            assert!(
                registry
                    .enqueue_due_maintenance(
                        project.binding,
                        instance,
                        Incarnation::from_bytes([n + 32; 16]),
                        now + 1_000_000
                    )
                    .is_err()
            );
            assert!(registry.due_maintenance(now)?.is_empty());
            registry.claim_job_id(&lease, job.id)?;
            registry.complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })?;
            registry.record_maintenance(&lease, &observation)?;
        }
        assert!(registry.due_maintenance(100_000_000)?.is_empty());
        assert!(!registry.maintenance_retrying(project.binding)?);
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), instance)?;
        assert!(registry.due_maintenance(1_000_000)?.is_empty());
        assert_eq!(
            registry
                .maintenance_observation(project.binding)?
                .unwrap()
                .pending_incarnations,
            Some(1)
        );
        observation.pending_incarnations = Some(0);
        registry.record_maintenance(&lease, &observation)?;
        observation.pending_incarnations = None;
        observation.failed = true;
        registry.record_maintenance(&lease, &observation)?;
        for n in 8..=10u8 {
            let now = u64::from(n) * 1_000_000;
            assert_eq!(registry.due_maintenance(now)?, vec![project.binding]);
            let nonce = Incarnation::from_bytes([n; 16]);
            let job = registry.enqueue_due_maintenance(project.binding, instance, nonce, now)?;
            assert_eq!(
                registry
                    .enqueue_due_maintenance(project.binding, instance, nonce, now)?
                    .id,
                job.id
            );
            assert!(
                registry
                    .enqueue_due_maintenance(
                        project.binding,
                        instance,
                        Incarnation::from_bytes([n + 32; 16]),
                        now + 1_000_000
                    )
                    .is_err()
            );
            registry.claim_job_id(&lease, job.id)?;
            registry.complete_job(
                &lease,
                job.id,
                JobOutcome::Failed {
                    code: "owned_retirement_failed".into(),
                },
            )?;
            registry.record_maintenance(&lease, &observation)?;
        }
        assert!(registry.due_maintenance(100_000_000)?.is_empty());
        Ok(())
    }
    #[test]
    fn service_grants_claim_exact_jobs_and_cancellation_intent_survives_reopen() -> Result<()> {
        let root = tempfile::tempdir()?;
        let instance = Incarnation::from_bytes([7; 16]);
        let mut registry = ServiceRegistry::open(root.path(), instance)?;
        let project =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&project)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let first = registry.enqueue(
            project.binding,
            instance,
            &RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes([1; 16]),
            },
            JobKind::Index,
        )?;
        let second = registry.enqueue(
            project.binding,
            instance,
            &RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes([2; 16]),
            },
            JobKind::Verify,
        )?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        assert!(
            registry
                .claim_job_id(&lease, Sha256Digest::from_bytes([0; 32]))?
                .is_none()
        );
        assert_eq!(
            registry.claim_job_id(&lease, second.id)?.unwrap().id,
            second.id
        );
        assert_eq!(registry.job(first.id)?.state, JobState::Pending);
        assert!(registry.claim_job_id(&lease, first.id)?.is_none());
        registry.request_cancellation(second.id)?;
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), instance)?;
        assert!(registry.cancellation_requested(second.id)?);
        assert_eq!(registry.job(second.id)?.state, JobState::Running);
        registry.complete_job(&lease, second.id, JobOutcome::Canceled)?;
        assert_eq!(
            registry.claim_job_id(&lease, first.id)?.unwrap().id,
            first.id
        );
        Ok(())
    }
    #[test]
    fn pending_cursor_covers_projects_beyond_first_thirty_two() -> Result<()> {
        let root = tempfile::tempdir()?;
        let instance = Incarnation::from_bytes([7; 16]);
        let mut registry = ServiceRegistry::open(root.path(), instance)?;
        let (namespace, epoch) = registry.request_epoch()?;
        for n in 1..=40u8 {
            let project =
                ProjectRegistration::new(format!("/source/{n}"), "/state".into(), format!("p{n}"))?;
            registry.register(&project)?;
            registry.enqueue(
                project.binding,
                instance,
                &RequestKey {
                    namespace,
                    epoch,
                    nonce: Incarnation::from_bytes([n; 16]),
                },
                JobKind::Verify,
            )?;
        }
        let first = registry.pending_project_page(None, 32)?;
        let second = registry.pending_project_page(first.last().copied(), 32)?;
        assert_eq!(first.len(), 32);
        assert_eq!(second.len(), 8);
        assert!(first.iter().all(|id| !second.contains(id)));
        assert!(
            registry
                .pending_project_page(second.last().copied(), 32)?
                .is_empty()
        );
        Ok(())
    }
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

    #[test]
    fn matching_rebuild_switch_receipt_recovers_success_without_replaying_live_or_foreign_work()
    -> Result<()> {
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
        let receipt = crate::service::collection::RebuildReceipt {
            job: job.id,
            token: crate::current::CurrentToken {
                incarnation: Incarnation::from_bytes([8; 16]),
                sequence: 3,
                digest: Sha256Digest::from_bytes([9; 32]),
            },
        };
        registry.recover_jobs_with_rebuild_receipt(&lease, Some(&receipt))?;
        assert_eq!(registry.job(job.id)?.state, JobState::Running);
        drop(lease);
        let mut replacement = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut replacement)?;
        registry.recover_jobs_with_rebuild_receipt(&replacement, Some(&receipt))?;
        assert_eq!(registry.job(job.id)?.state, JobState::Succeeded);
        assert_eq!(
            registry.due_background_jobs(None, 1)?[0].2,
            JobKind::Vectors
        );
        registry.set_background_pending(&replacement, JobKind::Vectors, false)?;
        registry.recover_jobs_with_rebuild_receipt(&replacement, Some(&receipt))?;
        assert!(
            !registry.has_background_work()?,
            "receipt replay rescheduled completed or canceled enrichment"
        );
        assert!(registry.claim_job(&replacement)?.is_none());
        assert_eq!(
            registry
                .enqueue(project.binding, caller, &key, JobKind::Rebuild)?
                .id,
            job.id
        );
        Ok(())
    }
}
