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

// Shared canonical insertion primitives. Callers retain the same IMMEDIATE
// transaction and establish key, registration, queue and source admission first.
pub(super) fn insert_new_job(
    connection: &Connection,
    project: ProjectBinding,
    caller: Incarnation,
    key: &RequestKey,
    kind: JobKind,
    configuration: Option<&airk_core::service_configuration::ConfigurationWork>,
) -> Result<Sha256Digest> {
    let jobs: i64 =
        connection.query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
    ensure!(jobs < MAX_JOBS, JobError::Capacity);
    let id = airk_core::catalog_source_change::source_job_id(project, caller, key)?;
    connection.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,?3,'pending',0,0)",params![id.to_string(),project.to_string(),kind.code()])?;
    if let Some(work) = configuration {
        super::configuration::insert(connection, id, work)?;
    }
    Ok(id)
}
pub(super) fn insert_request_receipt(
    connection: &Connection,
    caller: Incarnation,
    key: &RequestKey,
    id: Sha256Digest,
) -> Result<()> {
    connection.execute(
        "INSERT INTO service_requests VALUES(?1,?2,?3,?4,?5)",
        params![
            key.namespace.to_string(),
            i64::try_from(key.epoch)?,
            key.nonce.to_string(),
            caller.to_string(),
            id.to_string()
        ],
    )?;
    Ok(())
}
pub(super) fn require_request_capacity(connection: &Connection) -> Result<()> {
    let requests: i64 =
        connection.query_row("SELECT count(*) FROM service_requests", [], |row| {
            row.get(0)
        })?;
    ensure!(requests < MAX_REQUESTS, JobError::Capacity);
    ensure!(!metadata_pressure(connection)?, JobError::Capacity);
    Ok(())
}

// Unregister changes registration metadata only. It must not require resuming
// source work to remove a paused project. Reviewed retained-only source policy
// removal also runs while paused; it never captures or indexes source bytes.
const RUN_WHILE_PAUSED: &str = "(j.kind IN('inspect','plan_inspect') OR (j.kind='configure' AND EXISTS(SELECT 1 FROM service_configuration c WHERE c.job=j.id AND (json_extract(CAST(c.payload AS TEXT),'$.request.configuration.operation') IN('unregister','prepare_derived_cleanup','delete_derived','apply_source_policy') OR (json_extract(CAST(c.payload AS TEXT),'$.request.configuration.operation')='split_work' AND json_extract(CAST(c.payload AS TEXT),'$.request.configuration.purpose') IN('destination_cleanup','original_cleanup','original_witness'))))))";

fn runs_while_paused(
    kind: JobKind,
    configuration: Option<&airk_core::service_configuration::ProjectConfiguration>,
) -> bool {
    matches!(kind, JobKind::Inspect | JobKind::PlanInspect)
        || kind == JobKind::Configure && configuration.is_some_and(|work| work.runs_while_paused())
}

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

fn job_eligibility(connection: &Connection) -> Result<String> {
    Ok(format!(
        "({}) AND {} AND {} AND {}",
        super::maintenance::ELIGIBLE,
        super::recovery::ELIGIBLE,
        super::catalog::policy::job_eligibility(connection)?,
        super::catalog::split::job_eligibility(connection)?
    ))
}

/// Source priority applies to semantic-only work, regardless of who requested
/// it. The legacy registry does not persist automatic/manual Vector provenance.
/// Preserve the relative order of all other accepted jobs.
fn pending_source_work(connection: &Connection, project: ProjectBinding) -> Result<bool> {
    Ok(connection.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM service_jobs j JOIN projects p ON p.binding=j.project
          WHERE j.project=?1 AND p.registered=1 AND j.state='pending'
          AND (p.paused=0 OR {RUN_WHILE_PAUSED}) AND {}
          AND (EXISTS(SELECT 1 FROM service_job_source source WHERE source.job=j.id)
            OR EXISTS(SELECT 1 FROM service_configuration c WHERE c.job=j.id AND json_extract(CAST(c.payload AS TEXT),'$.request.configuration.operation')='refresh_references'))
          AND NOT EXISTS(SELECT 1 FROM service_job_cancellation canceled WHERE canceled.job=j.id))
          OR EXISTS(SELECT 1 FROM service_background b JOIN projects p ON p.binding=b.project
            WHERE b.project=?1 AND b.source_pending=1 AND p.registered=1 AND p.paused=0)",
            job_eligibility(connection)?
        ),
        [project.to_string()],
        |row| row.get(0),
    )?)
}

#[derive(Default)]
struct EnqueueOptions {
    explicit: bool,
    native_refresh: bool,
    configuration: Option<Box<airk_core::service_configuration::ConfigurationWork>>,
    background_revision: Option<(u64, u64)>,
    audit_revision: Option<(u64, u64)>,
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

fn receipt_pressure(connection: &Connection, needs_job: bool) -> Result<(bool, bool)> {
    let jobs: i64 =
        connection.query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
    let requests: i64 =
        connection.query_row("SELECT count(*) FROM service_requests", [], |row| {
            row.get(0)
        })?;
    let count_full = (needs_job && jobs >= MAX_JOBS) || requests >= MAX_REQUESTS;
    Ok((count_full, count_full || metadata_pressure(connection)?))
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
    /// Observation only: no pruning, admission or reapplication of an unknown request.
    pub fn request_receipt(
        &self,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<(Option<JobRecord>, bool)> {
        let id: Option<String> = self.connection.query_row(
            "SELECT job FROM service_requests WHERE namespace=?1 AND epoch=?2 AND nonce=?3 AND caller=?4",
            params![key.namespace.to_string(),i64::try_from(key.epoch).map_err(|_|JobError::KeyExpired)?,key.nonce.to_string(),caller.to_string()],
            |row|row.get(0)).optional()?;
        let observed = id
            .map(|id| record(&self.connection, id.parse()?))
            .transpose()?;
        let (namespace, epoch) = self.saved_request_epoch()?;
        let expired = observed.is_none() && (namespace != key.namespace || epoch != key.epoch);
        Ok((observed, expired))
    }
    pub fn job_runs_while_paused(&self, id: Sha256Digest) -> Result<bool> {
        let job = record(&self.connection, id)?;
        let configuration = if job.kind == JobKind::Configure {
            super::configuration::work(&self.connection, id)?
        } else {
            None
        };
        Ok(runs_while_paused(
            job.kind,
            configuration
                .as_ref()
                .map(|work| &work.request.configuration),
        ))
    }
    /// Fair oldest-pending order, bounded independently of the number of saved projects.
    pub fn pending_projects(&self, limit: usize) -> Result<Vec<ProjectBinding>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid pending project page bound"
        );
        let mut statement = self.connection.prepare(
            &format!("SELECT j.project FROM service_jobs j JOIN projects p ON p.binding=j.project WHERE j.state='running' OR (j.state='pending' AND (p.paused=0 OR {RUN_WHILE_PAUSED}) AND {}) GROUP BY j.project ORDER BY min(j.seq) LIMIT ?1",job_eligibility(&self.connection)?))?;
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
             AND (?1 IS NULL OR j.project>?1) ORDER BY j.project LIMIT ?2",job_eligibility(&self.connection)?),
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
             WHERE j.project=?1 AND j.state='pending' AND (p.paused=0 OR {RUN_WHILE_PAUSED}) AND {} ORDER BY j.seq LIMIT 1",job_eligibility(&self.connection)?),
                [project.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let head = id
            .map(|id| record(&self.connection, id.parse()?))
            .transpose()?;
        if head
            .as_ref()
            .is_some_and(|job| job.kind == JobKind::Vectors)
            && pending_source_work(&self.connection, project)?
        {
            let id: Option<String> = self
                .connection
                .query_row(
                    &format!(
                        "SELECT j.id FROM service_jobs j JOIN projects p ON p.binding=j.project
                    WHERE j.project=?1 AND j.state='pending' AND j.kind<>'vectors'
                    AND (p.paused=0 OR {RUN_WHILE_PAUSED}) AND {} ORDER BY j.seq LIMIT 1",
                        job_eligibility(&self.connection)?
                    ),
                    [project.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            return id
                .map(|id| record(&self.connection, id.parse()?))
                .transpose();
        }
        Ok(head)
    }
    /// Cached-console observation includes paused jobs without admitting work.
    pub fn pending_job_observation(&self, project: ProjectBinding) -> Result<Option<JobRecord>> {
        let id: Option<String> = self.connection.query_row(
            "SELECT id FROM service_jobs WHERE project=?1 AND state='pending' ORDER BY seq LIMIT 1",
            [project.to_string()], |row| row.get(0),
        ).optional()?;
        id.map(|id| record(&self.connection, id.parse()?))
            .transpose()
    }
    /// Full saved desired-state predicate for an offline rebuild owner. Unlike
    /// a bounded console queue page, this includes paused and running work and
    /// model-setup continuations. A stopped process does not prove that an
    /// accepted configuration has a known terminal outcome.
    pub fn has_unsettled_configuration(&self, project: ProjectBinding) -> Result<bool> {
        self.get(project)?;
        self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND state IN('pending','running') AND kind IN('configure','model_setup')) OR EXISTS(SELECT 1 FROM service_setup_pending WHERE project=?1)",
            [project.to_string()], |row| row.get(0),
        ).map_err(Into::into)
    }
    /// Bounded FIFO observations include paused jobs and never admit or mutate work.
    pub fn queued_jobs_observation(
        &self,
        project: ProjectBinding,
        limit: usize,
    ) -> Result<Vec<JobRecord>> {
        ensure!(
            (1..=32).contains(&limit),
            "queued job observation limit must be 1..32"
        );
        self.get(project)?;
        let transaction = self.connection.unchecked_transaction()?;
        let mut statement = transaction.prepare("SELECT id FROM service_jobs WHERE project=?1 AND state='pending' ORDER BY seq LIMIT ?2")?;
        let ids = statement
            .query_map(params![project.to_string(), limit as i64], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        let jobs = ids
            .into_iter()
            .map(|id| record(&transaction, id.parse()?))
            .collect::<Result<Vec<_>>>()?;
        transaction.commit()?;
        Ok(jobs)
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
        let (count_full, pressure) = receipt_pressure(&transaction, needs_job)?;
        if pressure {
            let pinned = super::catalog::source_change::pinned_jobs(&transaction)?;
            let pins =
                serde_json::to_string(&pinned.iter().map(ToString::to_string).collect::<Vec<_>>())?;
            let deleted = transaction.execute("DELETE FROM service_jobs WHERE seq IN(
                SELECT seq FROM service_jobs WHERE state IN('succeeded','failed','canceled') AND (?1 IS NULL OR id<>?1)
                AND id NOT IN(SELECT value FROM json_each(?2)) ORDER BY seq LIMIT 64)", params![protected.map(|id|id.to_string()),pins])?;
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

    /// Periodic management-owner maintenance, independent of an open editor.
    /// The usual no-pressure case takes no writer lock and changes no metadata.
    /// Admission still performs its own authoritative check and protects retry keys.
    pub fn maintain_job_receipts(&mut self) -> Result<()> {
        let observation = self.connection.unchecked_transaction()?;
        let (_, pressure) = receipt_pressure(&observation, false)?;
        observation.commit()?;
        if pressure {
            // Recheck after obtaining exclusion: the observation grants no authority.
            self.prune_job_receipts(false, None)?;
        }
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
        self.enqueue_impl(
            project,
            caller,
            key,
            kind,
            EnqueueOptions {
                explicit: true,
                ..Default::default()
            },
        )
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
        ensure!(
            !self.configuration_work(id)?.is_some_and(|work| matches!(
                work.request.configuration,
                airk_core::service_configuration::ProjectConfiguration::SplitWork { .. }
                    | airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}
            )),
            "split work requires exact saved-operation recovery"
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
        ensure!(
            !matches!(
                request.configuration,
                airk_core::service_configuration::ProjectConfiguration::SplitWork { .. }
                    | airk_core::service_configuration::ProjectConfiguration::RefreshReferences { .. }
            ),
            "split work requires its exact admitted lifecycle"
        );
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

    /// Authenticated native adapter must establish current OS host/grant and
    /// exact backing before this call. This method creates no such authority;
    /// its transaction preserves the adapter's observed catalog fence.
    pub fn enqueue_native_reference_refresh(
        &mut self,
        project: ProjectBinding,
        caller: Incarnation,
        key: &RequestKey,
        request: &airk_core::service_configuration::ConfigurationRequest,
        engine: Sha256Digest,
    ) -> Result<JobRecord> {
        request.validate()?;
        let airk_core::service_configuration::ProjectConfiguration::RefreshReferences { selection } =
            &request.configuration
        else {
            anyhow::bail!("native reference refresh request missing");
        };
        ensure!(
            !request.prepare && selection.caller == caller && selection.key == *key,
            "refresh exact caller/key/request binding changed"
        );
        self.enqueue_impl(
            project,
            caller,
            key,
            JobKind::Configure,
            EnqueueOptions {
                native_refresh: true,
                configuration: Some(Box::new(
                    airk_core::service_configuration::ConfigurationWork {
                        engine,
                        request: request.clone(),
                    },
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
            explicit,
            native_refresh,
            configuration,
            background_revision,
            audit_revision,
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
        let is_refresh = configuration.as_ref().is_some_and(|work| {
            matches!(
                work.request.configuration,
                airk_core::service_configuration::ProjectConfiguration::RefreshReferences { .. }
            )
        });
        ensure!(
            is_refresh == native_refresh,
            "refresh requires the authenticated native admission adapter"
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
        if let Some(work) = configuration.as_deref() {
            super::catalog::source_change::require_configuration_admission(
                &self.connection,
                &work.request,
            )?;
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
        if let Some(work) = configuration.as_deref() {
            super::catalog::source_change::require_configuration_admission(
                &transaction,
                &work.request,
            )?;
        }
        if native_refresh {
            let work = configuration
                .as_ref()
                .context("native refresh work missing")?;
            let airk_core::service_configuration::ProjectConfiguration::RefreshReferences {
                selection,
            } = &work.request.configuration
            else {
                unreachable!()
            };
            super::catalog::require_refresh_admission(&transaction, project, selection)?;
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
        if let Some(work) = configuration.as_deref().filter(|work| work.request.prepare) {
            ensure!(
                work.request.incarnation.to_string() == namespace
                    && work.request.settings == project,
                "indexing preference must name the current registry and exact project"
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
        if derived {
            super::catalog::source_change::require_retired(&transaction, project)?;
        } else {
            super::catalog::source_change::require_ready_registration(&transaction, project)?;
        }
        ensure!(!metadata_pressure(&transaction)?, JobError::Capacity);
        let planned_source = plan
            .map(|link| super::maintenance::prepare_job(&transaction, link, project, kind))
            .transpose()?
            .unwrap_or(false);
        if (explicit
            && matches!(
                kind,
                JobKind::Index | JobKind::Reconcile | JobKind::Recover | JobKind::Rebuild
            ))
            || manual_retry.is_some()
            || planned_source
        {
            super::background::audit::automatic::resume(&transaction, project)?;
            transaction.execute(
                "DELETE FROM service_recovery_budget WHERE scope IN(?1,?2)",
                params![format!("source:{project}"), format!("work:{project}")],
            )?;
        }
        if let Some(source) = manual_retry {
            let original = record(&transaction, source)?;
            ensure!(
                original.project == project
                    && original.kind == kind
                    && matches!(original.state, JobState::Failed | JobState::Canceled),
                JobError::NotRetryable
            );
            transaction.execute(
                "DELETE FROM service_recovery_budget WHERE scope=?1",
                [format!("work:{project}")],
            )?;
        }
        if maintenance_now_ms.is_some() || source_request.is_some() {
            let busy: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM service_jobs j WHERE j.project=?1 AND j.state IN('pending','running') AND NOT (?2=1 AND j.state='pending' AND j.kind='vectors') AND NOT EXISTS(SELECT 1 FROM service_background_audit a WHERE a.job=j.id AND a.manual=0 AND j.state='pending') AND (?2=0 OR NOT EXISTS(SELECT 1 FROM service_job_recovery r WHERE r.job=j.id AND r.waiting=1 AND j.kind='model_setup')))",
                params![project.to_string(), source_request.is_some()], |row| row.get(0))?;
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
        ensure!(
            audit_revision.is_none() || pending.is_none(),
            "foreground Verify already admitted"
        );
        let id = if let Some(id) = pending.filter(|_| plan.is_none()) {
            id.parse()?
        } else {
            insert_new_job(
                &transaction,
                project,
                caller,
                key,
                kind,
                configuration.as_deref(),
            )?
        };
        if let Some((revision, now)) = audit_revision {
            ensure!(kind == JobKind::Verify, "invalid automatic audit kind");
            ensure!(transaction.execute("UPDATE service_background_audit SET job=?1,parked=0,manual=0,campaign=campaign+1,revision=revision+1 WHERE project=?2 AND revision=?3 AND job IS NULL AND intent IS NOT NULL AND blocked IS NULL AND next_ms<=?4 AND campaign<9223372036854775807 AND NOT EXISTS(SELECT 1 FROM service_background b WHERE b.project=service_background_audit.project AND (b.source_pending=1 OR b.semantic_pending=1)) AND NOT EXISTS(SELECT 1 FROM service_setup_pending s WHERE s.project=service_background_audit.project) AND EXISTS(SELECT 1 FROM projects p WHERE p.binding=service_background_audit.project AND p.paused=0 AND p.registered=1) AND NOT EXISTS(SELECT 1 FROM service_jobs busy WHERE busy.project=service_background_audit.project AND busy.id<>?1 AND busy.state IN('pending','running'))",params![id.to_string(),project.to_string(),i64::try_from(revision)?,i64::try_from(now)?])?==1,"automatic audit admission changed");
        } else if explicit && kind == JobKind::Verify {
            super::background::audit::automatic::resume(&transaction, project)?;
            super::background::audit::automatic::promote(&transaction, id)?;
        }
        insert_request_receipt(&transaction, caller, key, id)?;
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
        if let Some((revision, now_ms)) = background_revision {
            super::background::debit(&transaction, project, kind, revision, id, now_ms)?;
        }
        if let Some(now_ms) = maintenance_now_ms {
            // Admission and retry debit share this transaction. Replaying an accepted
            // key returns above, without consuming another attempt after a crash.
            let retries: u32 = transaction.query_row(
                "SELECT m.retries FROM service_maintenance m JOIN projects p ON p.binding=m.project
                 WHERE m.project=?1 AND p.paused=0 AND (m.pending>0 OR m.failed=1)
                 AND m.retries<CASE WHEN m.failed=1 THEN 3 ELSE 7 END AND m.next_ms<=?2
                 AND NOT EXISTS(SELECT 1 FROM service_jobs busy WHERE project=?1 AND id<>?3 AND state IN('pending','running') AND NOT EXISTS(SELECT 1 FROM service_background_audit a WHERE a.job=busy.id AND a.manual=0 AND busy.state='pending'))",
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
                ensure!(
                    super::watch::automatic_source_candidate(
                        &transaction,
                        project,
                        now_ms,
                        Some(id)
                    )? == Some(revision),
                    "source reconciliation is no longer eligible"
                );
                let attempts: u32 = transaction.query_row(
                    "SELECT attempts FROM service_watch WHERE project=?1",
                    [project.to_string()],
                    |row| row.get(0),
                )?;
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
        let clean = observation.pending_incarnations == Some(0)
            && !observation.pending_objects
            && !observation.failed;
        // The retry budget limits consecutive passes without progress, not the
        // number of successful batches needed to retire a large index.
        let progressed = !observation.failed && observation.removed_files > 0;
        let pending = observation
            .pending_incarnations
            .map(|count| count as i64 + i64::from(observation.pending_objects));
        tx.execute("INSERT INTO service_maintenance(project,observation,pending,failed,retries,next_ms) VALUES(?1,?2,?3,?4,0,0)
            ON CONFLICT(project) DO UPDATE SET observation=excluded.observation,pending=excluded.pending,failed=excluded.failed,
            retries=CASE WHEN ?5 THEN 0 ELSE service_maintenance.retries END,next_ms=CASE WHEN ?5 THEN 0 ELSE service_maintenance.next_ms END",
            params![lease.binding.to_string(),bytes,pending,observation.failed,clean || progressed])?;
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
        let mut statement=self.connection.prepare("SELECT m.project FROM service_maintenance m JOIN projects p ON p.binding=m.project WHERE p.paused=0 AND (m.pending>0 OR m.failed=1) AND m.retries<CASE WHEN m.failed=1 THEN 3 ELSE 7 END AND m.next_ms<=?1 AND NOT EXISTS(SELECT 1 FROM service_jobs j WHERE j.project=m.project AND j.state IN('pending','running') AND NOT EXISTS(SELECT 1 FROM service_background_audit a WHERE a.job=j.id AND a.manual=0 AND j.state='pending')) ORDER BY m.next_ms,m.project LIMIT 32")?;
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
        self.enqueue_impl(
            project,
            caller,
            &RequestKey {
                namespace,
                epoch,
                nonce,
            },
            kind,
            EnqueueOptions::default(),
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
    pub fn pending_reference_refresh(&self, project: ProjectBinding) -> Result<bool> {
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM service_jobs j JOIN service_configuration c ON c.job=j.id WHERE j.project=?1 AND j.state IN('pending','running') AND json_extract(CAST(c.payload AS TEXT),'$.request.configuration.operation')='refresh_references')",[project.to_string()],|row|row.get(0))?)
    }
    pub fn job(&self, id: Sha256Digest) -> Result<JobRecord> {
        record(&self.connection, id)
    }

    /// Exact accepted manual-retry lineage. An independent job of the same
    /// project/kind is not a successor and cannot satisfy an older receipt.
    pub fn retry_successors(&self, source: Sha256Digest) -> Result<Vec<JobRecord>> {
        let read = self.connection.unchecked_transaction()?;
        let original = record(&read, source)?;
        let mut statement = read.prepare("SELECT DISTINCT q.job FROM service_retry_requests r JOIN service_requests q ON q.namespace=r.namespace AND q.epoch=r.epoch AND q.nonce=r.nonce AND q.caller=r.caller WHERE r.source_job=?1 ORDER BY q.job LIMIT 1025")?;
        let ids = statement
            .query_map([source.to_string()], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        ensure!(
            ids.len() <= MAX_JOBS as usize,
            "manual retry lineage exceeds complete job roster bound"
        );
        let mut result = Vec::with_capacity(ids.len());
        for id in ids {
            let job = record(&read, id.parse()?)?;
            ensure!(
                job.id != original.id
                    && job.project == original.project
                    && job.kind == original.kind,
                "manual retry lineage crossed its original job authority"
            );
            result.push(job);
        }
        read.commit()?;
        Ok(result)
    }

    /// Latest successful accepted retry in the exact bounded transitive
    /// lineage. Missing, crossed or cyclic authority remains an error.
    pub fn successful_retry(&self, source: Sha256Digest) -> Result<Option<JobRecord>> {
        let read = self.connection.unchecked_transaction()?;
        let original = record(&read, source)?;
        let mut statement = read.prepare("WITH RECURSIVE lineage(id) AS (SELECT ?1 UNION SELECT q.job FROM lineage l JOIN service_retry_requests r ON r.source_job=l.id JOIN service_requests q ON q.namespace=r.namespace AND q.epoch=r.epoch AND q.nonce=r.nonce AND q.caller=r.caller LIMIT 1025) SELECT l.id,j.seq,EXISTS(SELECT 1 FROM service_retry_requests r JOIN service_requests q ON q.namespace=r.namespace AND q.epoch=r.epoch AND q.nonce=r.nonce AND q.caller=r.caller LEFT JOIN service_jobs p ON p.id=r.source_job WHERE q.job=l.id AND r.source_job IN(SELECT id FROM lineage) AND (p.seq IS NULL OR p.seq>=j.seq)) FROM lineage l LEFT JOIN service_jobs j ON j.id=l.id ORDER BY j.seq")?;
        let ids = statement
            .query_map([source.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        ensure!(
            ids.len() <= MAX_JOBS as usize,
            "manual retry lineage exceeds complete job roster bound"
        );
        let mut result = None;
        for (id, sequence, crossed) in ids {
            ensure!(
                sequence.is_some() && !crossed,
                "manual retry lineage is missing or cyclic"
            );
            let job = record(&read, id.parse()?)?;
            ensure!(
                job.project == original.project && job.kind == original.kind,
                "manual retry lineage crossed its original job authority"
            );
            if job.id != original.id && job.state == JobState::Succeeded {
                result = Some(job);
            }
        }
        read.commit()?;
        Ok(result)
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
        now_ms: u64,
    ) -> Result<JobRecord> {
        ensure!(
            matches!(
                kind,
                JobKind::Index | JobKind::Vectors | JobKind::ModelSetup
            ),
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
                background_revision: Some((revision, now_ms)),
                ..Default::default()
            },
        )
    }
    pub fn enqueue_automatic_audit(
        &mut self,
        project: ProjectBinding,
        revision: u64,
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
            JobKind::Verify,
            EnqueueOptions {
                audit_revision: Some((revision, now_ms)),
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
        super::background::audit::automatic::recovered(&transaction, lease.binding, epoch)?;
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
        // A forced watchdog exit has already given cooperative cancellation its grace
        // period. Preserve that reason and allow just one recovery of the same job.
        let stalled = serde_json::to_vec(&JobOutcome::Failed {
            code: "work_stalled".into(),
        })?;
        let stalled_job:Option<String>=transaction.query_row("SELECT j.id FROM service_jobs j JOIN service_progress p ON p.job=j.id WHERE j.project=?1 AND j.state='running' AND j.epoch<>?2 AND json_extract(CAST(p.observation AS TEXT),'$.progress.stalled')=1 AND NOT EXISTS(SELECT 1 FROM service_job_cancellation WHERE job=j.id) LIMIT 1",params![lease.binding.to_string(),i64::try_from(epoch)?],|r|r.get(0)).optional()?;
        if let Some(id) = stalled_job {
            let scope = format!("work:{}", lease.binding);
            let failures: u32 = transaction.query_row(
                "SELECT coalesce((SELECT failures FROM service_recovery_budget WHERE scope=?1),0)",
                [&scope],
                |r| r.get(0),
            )?;
            let now = super::maintenance::clock_ms()?;
            transaction.execute(
                "INSERT OR IGNORE INTO service_job_recovery VALUES(?1,'work_stalled',?2,0,?3,?3)",
                params![
                    id,
                    serde_json::to_vec(&super::recovery::RecoveryCondition::Delay)?,
                    i64::try_from(now)?
                ],
            )?;
            if failures > 0 {
                transaction.execute(
                    "UPDATE service_jobs SET state='failed',outcome=?1 WHERE id=?2",
                    params![stalled, id],
                )?;
            }
            transaction.execute("INSERT INTO service_recovery_budget VALUES(?1,'work',1,0,?2,0) ON CONFLICT(scope) DO UPDATE SET failures=min(failures+1,3)",params![scope,i64::try_from(now)?])?;
        }
        transaction.execute("UPDATE service_jobs SET state='failed',outcome=?1 WHERE project=?2 AND state='running' AND epoch<>?3 AND attempts>=2 AND id IN(SELECT job FROM service_progress WHERE json_extract(CAST(observation AS TEXT),'$.progress.stalled')=1) AND NOT EXISTS(SELECT 1 FROM service_job_cancellation WHERE job=service_jobs.id)",params![stalled,lease.binding.to_string(),i64::try_from(epoch)?])?;
        super::catalog::source_change::recover_pending_jobs(&transaction, lease, epoch)?;
        transaction.execute("UPDATE service_jobs SET state='pending' WHERE project=?1 AND kind='configure' AND state='running' AND epoch<>?2 AND attempts<3 AND id IN(SELECT job FROM service_configuration WHERE json_extract(CAST(payload AS TEXT),'$.request.prepare')=1 OR json_extract(CAST(payload AS TEXT),'$.request.configuration.operation') IN('prepare_derived_cleanup','delete_derived','apply_source_policy'))",
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
        super::background::audit::automatic::settle_terminal(&transaction, lease.binding)?;
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
        self.claim_selected_job_at(lease, selected, super::maintenance::clock_ms()?)
    }
    pub(super) fn claim_selected_job_at(
        &mut self,
        lease: &ProjectLease,
        selected: Option<Sha256Digest>,
        now_ms: u64,
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
        let candidate: Option<(String, bool)> = transaction.query_row(&format!("SELECT j.id,j.kind='vectors' FROM service_jobs j WHERE j.project=?1 AND j.state='pending' AND (?2 IS NULL OR j.id=?2) AND (NOT ?3 OR {RUN_WHILE_PAUSED}) AND {} ORDER BY j.seq LIMIT 1",job_eligibility(&transaction)?), params![lease.binding.to_string(), selected.map(|id|id.to_string()),paused], |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
        let Some((mut id, mut vector)) = candidate else {
            return Ok(None);
        };
        if vector && pending_source_work(&transaction, lease.binding)? {
            if selected.is_some() {
                return Ok(None);
            }
            // An unselected claim follows the same non-Vector FIFO as preview.
            // Exact service grants never substitute a different job.
            let priority: Option<String> = transaction
                .query_row(
                    &format!(
                        "SELECT j.id FROM service_jobs j WHERE j.project=?1 AND j.state='pending'
                    AND j.kind<>'vectors' AND (NOT ?2 OR {RUN_WHILE_PAUSED}) AND {}
                    ORDER BY j.seq LIMIT 1",
                        job_eligibility(&transaction)?
                    ),
                    params![lease.binding.to_string(), paused],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(priority) = priority else {
                return Ok(None);
            };
            id = priority;
            vector = false;
        }
        if vector
            && super::watch::automatic_source_candidate(
                &transaction,
                lease.binding,
                now_ms,
                Some(id.parse()?),
            )?
            .is_some()
        {
            // A supervisor grant can become stale before this transaction. Do
            // not substitute another job, debit retries, or lose the saved
            // Vector request; the worker releases only this unclaimed grant.
            return Ok(None);
        }
        super::catalog::source_change::require_claim(&transaction, id.parse()?)?;
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
        transaction.execute("UPDATE service_jobs SET state='running',epoch=?1,attempts=attempts+CASE WHEN EXISTS(SELECT 1 FROM service_background_audit a WHERE a.job=service_jobs.id AND a.parked=1) THEN 0 ELSE 1 END WHERE id=?2 AND state='pending'",
            params![i64::try_from(epoch)?, id])?;
        transaction.execute(
            "UPDATE service_background_audit SET parked=0 WHERE job=?1",
            [&id],
        )?;
        let claimed = record(&transaction, id.parse()?)?;
        super::integrity_guard::claimed(&transaction, &claimed)?;
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
        self.complete_job_observed(lease, id, outcome, None, None, (None, None))
    }

    pub fn complete_job_at(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: u64,
    ) -> Result<JobRecord> {
        self.complete_job_observed(lease, id, outcome, Some(now_ms), None, (None, None))
    }

    pub fn complete_job_with_maintenance_at(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: u64,
        maintenance: Option<&airk_core::service::MaintenanceObservation>,
    ) -> Result<JobRecord> {
        self.complete_job_observed(lease, id, outcome, Some(now_ms), maintenance, (None, None))
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
        super::background::audit::automatic::settle_terminal(&tx, lease.binding)?;
        super::maintenance::settle_project_jobs(
            &tx,
            lease.binding,
            super::maintenance::clock_ms()?,
        )?;
        tx.commit()?;
        Ok(failed)
    }

    pub fn complete_job_with_maintenance_and_audit_at(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: u64,
        maintenance: Option<&airk_core::service::MaintenanceObservation>,
        evidence: Option<super::AuditEvidence<'_>>,
    ) -> Result<JobRecord> {
        self.complete_job_observed(
            lease,
            id,
            outcome,
            Some(now_ms),
            maintenance,
            (evidence, None),
        )
    }

    /// Settle the public whole-Verify outcome while retaining separate sealed
    /// completion of the fixed actually declared query components.
    pub fn complete_job_with_query_audit_at(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: u64,
        maintenance: Option<&airk_core::service::MaintenanceObservation>,
        audits: (
            Option<super::AuditEvidence<'_>>,
            Option<&crate::service::collection::integrity::verify::QueryComponentAuditProof>,
        ),
    ) -> Result<JobRecord> {
        let (evidence, query) = audits;
        self.complete_job_observed(
            lease,
            id,
            outcome,
            Some(now_ms),
            maintenance,
            (evidence, query),
        )
    }

    fn complete_job_observed(
        &mut self,
        lease: &ProjectLease,
        id: Sha256Digest,
        outcome: JobOutcome,
        now_ms: Option<u64>,
        maintenance: Option<&airk_core::service::MaintenanceObservation>,
        audits: (
            Option<super::AuditEvidence<'_>>,
            Option<&crate::service::collection::integrity::verify::QueryComponentAuditProof>,
        ),
    ) -> Result<JobRecord> {
        let (evidence, query) = audits;
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
        if matches!(outcome, JobOutcome::Succeeded { .. }) {
            super::catalog::source_change::require_success_result(&transaction, id)?;
        }
        if let Some(proof) = query {
            ensure!(
                matches!(outcome, JobOutcome::Succeeded { .. })
                    || matches!(&outcome, JobOutcome::Failed { code } if code == "integrity_coverage_incomplete"),
                "query audit proof cannot settle this public outcome"
            );
            if let JobOutcome::Succeeded { revision } = &outcome {
                ensure!(
                    *revision == Some(proof.revision()),
                    "query audit publication mismatch"
                );
            }
            ensure!(
                matches!(current.kind, JobKind::Verify | JobKind::Repair)
                    && proof.matches(&transaction, &current)?,
                "foreign query audit completion"
            );
            proof.observation().validate()?;
            super::integrity_guard::require_no_owner_damage(
                &transaction,
                current.project,
                proof.revision().incarnation,
            )?;
        }
        let canceled_verify = current.kind == JobKind::Verify
            && transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM service_job_cancellation WHERE job=?1)",
                [id.to_string()],
                |row| row.get::<_, bool>(0),
            )?;
        // A read-only Verify has no publication mutation to preserve after Stop.
        // The same transaction observes cancellation, discards query scheduling
        // authority and retains only safely drained evidence for its veto.
        let outcome = if canceled_verify {
            JobOutcome::Canceled
        } else {
            outcome
        };
        let evidence = if canceled_verify {
            match query {
                Some(proof) => Some(super::AuditEvidence::Drained(proof.drain())),
                None => evidence,
            }
        } else {
            evidence
        };
        let query = if canceled_verify { None } else { query };
        super::integrity_guard::completed(&transaction, &current, &outcome, evidence)?;
        if query.is_some() {
            // Actual declared-profile completion settles uncertainty even when
            // truthful public whole-Verify coverage lacks legacy optional pages.
            transaction.execute(
                "DELETE FROM service_recovery_budget WHERE scope=?1 AND engine=?2",
                params![
                    format!("integrity:{}", current.project),
                    current.id.to_string()
                ],
            )?;
        }
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
        let publication_recovery = current.kind == JobKind::Recover
            || (current.kind == JobKind::Repair
                && super::maintenance::job_recovers_publication(&transaction, id)?);
        if source_revision.is_some() || publication_recovery {
            let scope = format!("source:{}", lease.binding);
            match &outcome {
                JobOutcome::Failed { code } => {
                    let now = now_ms.unwrap_or(0);
                    // Filesystem hints may advance desired source revision, but cannot
                    // replenish the project's terminal-failure budget.
                    transaction.execute("INSERT INTO service_recovery_budget VALUES(?1,?2,1,?3,?4,0)
                        ON CONFLICT(scope) DO UPDATE SET engine=excluded.engine,failures=min(failures+1,3),next_ms=excluded.next_ms",
                        params![scope,code,i64::try_from(now.saturating_add(8000))?,i64::try_from(now)?])?;
                }
                JobOutcome::Succeeded { .. } => {
                    transaction.execute(
                        "DELETE FROM service_recovery_budget WHERE scope=?1",
                        [scope],
                    )?;
                }
                JobOutcome::Canceled => {}
            }
        }
        if publication_recovery && matches!(outcome, JobOutcome::Succeeded { .. }) {
            // Recovery makes another source pass useful. Keep desired revisions,
            // watching preferences, pause and explicit cancellation unchanged.
            transaction.execute(
                "UPDATE service_watch SET attempts=0,next_ms=0 WHERE project=?1",
                [lease.binding.to_string()],
            )?;
        }
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
        if let Some(proof) = query {
            super::background::audit::automatic::finished_query(
                &transaction,
                &current,
                proof,
                now_ms.unwrap_or(super::maintenance::clock_ms()?),
            )?;
        } else {
            super::background::audit::automatic::finished(
                &transaction,
                id,
                matches!(outcome, JobOutcome::Succeeded { .. }),
                now_ms.unwrap_or(super::maintenance::clock_ms()?),
            )?;
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
        let result = request_cancellation_in(&transaction, id)?;
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
        super::background::audit::automatic::canceled(&transaction, id)?;
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

pub(super) fn request_cancellation_in(
    connection: &Connection,
    id: Sha256Digest,
) -> Result<JobRecord> {
    let current = record(connection, id)?;
    super::background::audit::automatic::canceled(connection, id)?;
    if current.kind == JobKind::Maintain
        && matches!(current.state, JobState::Pending | JobState::Running)
    {
        connection.execute(
            "UPDATE service_maintenance SET retries=7 WHERE project=?1",
            [current.project.to_string()],
        )?;
    }
    if matches!(current.state, JobState::Pending | JobState::Running) {
        super::watch::block_source_retry(connection, id)?;
    }
    match current.state {
        JobState::Pending => {
            connection.execute(
                "UPDATE service_jobs SET state='canceled',outcome=?1 WHERE id=?2",
                params![serde_json::to_vec(&JobOutcome::Canceled)?, id.to_string()],
            )?;
        }
        JobState::Running => {
            connection.execute(
                "INSERT OR IGNORE INTO service_job_cancellation(job) VALUES(?1)",
                [id.to_string()],
            )?;
        }
        _ => {}
    }
    let result = record(connection, id)?;
    super::maintenance::job_completed(connection, id, super::maintenance::clock_ms()?, None)?;
    Ok(result)
}

#[cfg(test)]
mod rebuild_settings_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::service::ProjectRegistration;

    #[test]
    fn paused_native_control_matches_exact_sql_classification_for_all_split_purposes() -> Result<()>
    {
        use airk_core::{
            catalog_split::SplitJobPurpose, service_configuration::ProjectConfiguration as C,
        };
        let db = Connection::open_in_memory()?;
        let sql = format!(
            "WITH j(id,kind) AS (VALUES('classification',?1)), service_configuration(job,payload) AS (VALUES('classification',?2)) SELECT {RUN_WHILE_PAUSED} FROM j"
        );
        let classify = |kind: JobKind, configuration: Option<&C>, expected: bool| -> Result<()> {
            // JSON is the actual saved-work shape, but no real job/registration,
            // admitted claim, policy mutation, source or funding is manufactured.
            let payload = configuration
                .map(|configuration| {
                    serde_json::to_string(
                        &serde_json::json!({"request":{"configuration":configuration}}),
                    )
                })
                .transpose()?;
            let database: bool =
                db.query_row(&sql, params![kind.code(), payload], |row| row.get(0))?;
            assert_eq!(
                database, expected,
                "SQL: kind={kind:?},configuration={configuration:?}"
            );
            assert_eq!(
                runs_while_paused(kind, configuration),
                database,
                "native control diverged from actual SQL pause admission classification"
            );
            Ok(())
        };
        for kind in [
            JobKind::Index,
            JobKind::Reconcile,
            JobKind::Vectors,
            JobKind::ModelSetup,
            JobKind::Rebuild,
            JobKind::Recover,
            JobKind::Configure,
            JobKind::Maintain,
            JobKind::Inspect,
            JobKind::PlanInspect,
            JobKind::Repair,
            JobKind::Verify,
        ] {
            classify(
                kind,
                None,
                matches!(kind, JobKind::Inspect | JobKind::PlanInspect),
            )?;
        }
        let digest = Sha256Digest::from_bytes([1; 32]);
        for configuration in [
            C::Unregister,
            C::PrepareDerivedCleanup,
            C::DeleteDerived { preview: digest },
        ] {
            classify(JobKind::Configure, Some(&configuration), true)?;
        }
        let policy: C = serde_json::from_value(serde_json::json!({
            "operation":"apply_source_policy","operation_id":Incarnation::from_bytes([2;16]),
            "policy":{"schema":"airk.source-policy/1","index_id":airk_core::index_catalog::IndexId::from_identity(Incarnation::from_bytes([3;16])),
                "owner_binding":digest,"namespace":"collections-1.4","collection":"classification","epoch":1,
                "roots":[],"positive_includes":null,"denies":[]}}))?;
        classify(JobKind::Configure, Some(&policy), true)?;
        for purpose in [
            SplitJobPurpose::DestinationBuild,
            SplitJobPurpose::OriginalWitness,
            SplitJobPurpose::OriginalRemoval,
            SplitJobPurpose::DestinationCleanup,
            SplitJobPurpose::OriginalCleanup,
        ] {
            let configuration = C::SplitWork {
                operation_id: Incarnation::from_bytes([4; 16]),
                request_digest: digest,
                expected_receipt_digest: digest,
                purpose,
                require_semantic_ready: false,
            };
            classify(
                JobKind::Configure,
                Some(&configuration),
                matches!(
                    purpose,
                    SplitJobPurpose::OriginalWitness
                        | SplitJobPurpose::DestinationCleanup
                        | SplitJobPurpose::OriginalCleanup
                ),
            )?;
        }
        classify(
            JobKind::Configure,
            Some(&C::SplitWork {
                operation_id: Incarnation::from_bytes([4; 16]),
                request_digest: digest,
                expected_receipt_digest: digest,
                purpose: SplitJobPurpose::DestinationBuild,
                require_semantic_ready: true,
            }),
            false,
        )?;
        for configuration in [
            C::Vectors { enabled: true },
            C::Vectors { enabled: false },
            C::Detail {
                level: airk_core::extraction::IndexDetail::Detailed,
            },
            C::BundledModel,
            C::RemoveRoot {
                alias: "classification".into(),
            },
            C::ResetScanLimits,
            C::ClearUsage,
        ] {
            classify(JobKind::Configure, Some(&configuration), false)?;
        }
        Ok(())
    }

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
                pending_objects: false,
                objects_checked: true,
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
            pending_objects: false,
            objects_checked: true,
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
    fn maintenance_continues_successful_batches_beyond_retry_limit_and_restart() -> Result<()> {
        let root = tempfile::tempdir()?;
        let instance = Incarnation::from_bytes([7; 16]);
        let mut registry = ServiceRegistry::open(root.path(), instance)?;
        let project =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&project)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        let mut observation = airk_core::service::MaintenanceObservation {
            pending_incarnations: Some(0),
            pending_objects: true,
            objects_checked: true,
            removed_files: 0,
            failed: false,
            observed_unix_ms: 1000,
        };
        registry.record_maintenance(&lease, &observation)?;
        for n in 1..=12u8 {
            let now = u64::from(n) * 1_000_000;
            assert_eq!(registry.due_maintenance(now)?, vec![project.binding]);
            let job = registry.enqueue_due_maintenance(
                project.binding,
                instance,
                Incarnation::from_bytes([n; 16]),
                now,
            )?;
            registry.claim_job_id(&lease, job.id)?;
            registry.complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })?;
            observation.removed_files = 4096;
            registry.record_maintenance(&lease, &observation)?;
            drop(registry);
            registry = ServiceRegistry::open(root.path(), instance)?;
        }
        assert!(registry.maintenance_retrying(project.binding)?);
        observation.pending_objects = false;
        observation.removed_files = 1;
        registry.record_maintenance(&lease, &observation)?;
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
    fn receipt_maintenance_without_pressure_does_not_require_a_writer() -> Result<()> {
        let root = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([7; 16]);
        let mut registry = ServiceRegistry::open(root.path(), caller)?;
        let mut other = ServiceRegistry::open(root.path(), caller)?;
        let epoch = registry.saved_request_epoch()?;
        let changes = registry.connection.total_changes();
        let writer = other
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        registry.maintain_job_receipts()?;
        assert_eq!(registry.connection.total_changes(), changes);
        assert_eq!(registry.saved_request_epoch()?, epoch);
        writer.rollback()?;
        Ok(())
    }

    #[test]
    fn receipt_maintenance_retires_one_bounded_page_and_expires_only_retired_keys() -> Result<()> {
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
        let running = registry.enqueue(project.binding, caller, &key, JobKind::Index)?;
        let transaction = registry.connection.transaction()?;
        // Fill request pressure with terminal jobs and aliases. The live job's
        // original receipt must remain resolvable across global epoch rotation.
        for number in 0..130i64 {
            let id = framed_digest(b"maintenance-fixture", &[&number.to_le_bytes()]);
            transaction.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,'inspect','succeeded',0,0)", params![id.to_string(), project.binding.to_string()])?;
            for alias in 0..32i64 {
                let mut nonce = [0u8; 16];
                nonce[..8].copy_from_slice(&(number * 32 + alias + 100).to_le_bytes());
                transaction.execute("INSERT INTO service_requests(namespace,epoch,nonce,caller,job) VALUES(?1,?2,?3,?4,?5)", params![namespace.to_string(), i64::try_from(epoch)?, Incarnation::from_bytes(nonce).to_string(), caller.to_string(), id.to_string()])?;
            }
        }
        transaction.commit()?;
        registry.maintain_job_receipts()?;
        let remaining: i64 =
            registry
                .connection
                .query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
        assert_eq!(remaining, 67);
        assert_eq!(registry.saved_request_epoch()?.1, epoch + 1);
        assert_eq!(
            registry
                .enqueue(project.binding, caller, &key, JobKind::Index)?
                .id,
            running.id
        );
        let mut retired_nonce = [0u8; 16];
        retired_nonce[..8].copy_from_slice(&100i64.to_le_bytes());
        let retired = RequestKey {
            nonce: Incarnation::from_bytes(retired_nonce),
            ..key
        };
        assert_eq!(
            registry
                .enqueue(project.binding, caller, &retired, JobKind::Inspect)
                .unwrap_err()
                .downcast_ref::<JobError>(),
            Some(&JobError::KeyExpired)
        );
        let changes = registry.connection.total_changes();
        registry.maintain_job_receipts()?;
        assert_eq!(registry.connection.total_changes(), changes);
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
        let queued = registry.enqueue(project.binding, caller, &key, JobKind::Index)?;
        registry.pause(project.binding, true)?;
        assert_eq!(
            registry
                .pending_job_observation(project.binding)?
                .unwrap()
                .id,
            queued.id
        );
        assert!(registry.next_pending_job(project.binding)?.is_none());
        let observed_queue = registry.queued_jobs_observation(project.binding, 16)?;
        assert_eq!(observed_queue.len(), 1);
        assert_eq!(observed_queue[0].id, queued.id);
        assert!(
            registry
                .queued_jobs_observation(project.binding, 33)
                .is_err()
        );
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
            registry.due_background_jobs(None, 0, 1)?[0].2,
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
