// SPDX-License-Identifier: Apache-2.0
//! Durable source desired state and bounded worker-reported root metadata.
use super::{ServiceRegistry, bump};
use crate::service::ProjectLease;
use airk_core::{
    identity::Sha256Digest,
    service::{ProjectBinding, RegisteredProject},
    service_watch::{WatchBackend, WatchFailure, WatchObservation, WatchTraversal},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

pub(super) const TRAVERSAL_SCHEMA: &str = "
    CREATE TABLE service_watch_traversal(
        project TEXT PRIMARY KEY REFERENCES projects(binding) ON DELETE CASCADE,
        configuration_epoch INTEGER NOT NULL CHECK(configuration_epoch>=0),
        observation BLOB NOT NULL CHECK(length(observation)<=8192),
        observed_ms INTEGER NOT NULL CHECK(observed_ms>=0)) WITHOUT ROWID;";

pub(super) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS service_watch(
        project TEXT PRIMARY KEY REFERENCES projects(binding),
        enabled INTEGER NOT NULL CHECK(enabled IN(0,1)),
        requested INTEGER NOT NULL CHECK(requested>=0),
        indexed INTEGER NOT NULL CHECK(indexed>=0 AND indexed<=requested),
        configuration_epoch INTEGER NOT NULL CHECK(configuration_epoch>=0),
        roots BLOB NOT NULL CHECK(length(roots)<=98304),roots_observed INTEGER NOT NULL CHECK(roots_observed IN(0,1)),
        native_roots INTEGER NOT NULL CHECK(native_roots BETWEEN 0 AND 17),
        backend TEXT NOT NULL,failure TEXT,observed_ms INTEGER,last_reconciled_ms INTEGER,
        attempts INTEGER NOT NULL CHECK(attempts BETWEEN 0 AND 3),next_ms INTEGER NOT NULL,
        blocked_revision INTEGER) WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS service_job_source(
        job TEXT PRIMARY KEY REFERENCES service_jobs(id) ON DELETE CASCADE,
        revision INTEGER NOT NULL CHECK(revision>=0)) WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS service_watch_events(
        project TEXT PRIMARY KEY REFERENCES projects(binding),worker TEXT NOT NULL,
        configuration_epoch INTEGER NOT NULL CHECK(configuration_epoch>=0),
        sequence INTEGER NOT NULL CHECK(sequence>0)) WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS service_source_outcomes(
        project TEXT PRIMARY KEY REFERENCES projects(binding) ON DELETE CASCADE,
        seq INTEGER NOT NULL CHECK(typeof(seq)='integer' AND seq>0),
        job TEXT NOT NULL CHECK(length(job)=64),
        state TEXT NOT NULL CHECK(state IN('succeeded','failed','canceled')),
        outcome BLOB NOT NULL CHECK(length(outcome)<=2048)) WITHOUT ROWID;
    CREATE TRIGGER IF NOT EXISTS service_source_outcome_updated AFTER UPDATE OF state,outcome ON service_jobs
    WHEN NEW.state IN('succeeded','failed','canceled') AND EXISTS(SELECT 1 FROM service_job_source WHERE job=NEW.id)
    BEGIN
        INSERT INTO service_source_outcomes(project,seq,job,state,outcome) VALUES(NEW.project,NEW.seq,NEW.id,NEW.state,NEW.outcome)
        ON CONFLICT(project) DO UPDATE SET seq=excluded.seq,job=excluded.job,state=excluded.state,outcome=excluded.outcome
        WHERE excluded.seq>=service_source_outcomes.seq;
    END;
    INSERT INTO service_source_outcomes(project,seq,job,state,outcome)
        SELECT j.project,j.seq,j.id,j.state,j.outcome FROM service_jobs j JOIN service_job_source s ON s.job=j.id
        WHERE j.seq IN(SELECT max(latest.seq) FROM service_jobs latest JOIN service_job_source ls ON ls.job=latest.id
            WHERE latest.state IN('succeeded','failed','canceled') GROUP BY latest.project)
        ON CONFLICT(project) DO UPDATE SET seq=excluded.seq,job=excluded.job,state=excluded.state,outcome=excluded.outcome
        WHERE excluded.seq>service_source_outcomes.seq;";

pub struct WatchProject {
    pub project: RegisteredProject,
    pub configuration_epoch: u64,
    pub additional_roots: Vec<String>,
    pub observation: WatchObservation,
}

pub(super) fn ensure_row(connection: &rusqlite::Connection, project: ProjectBinding) -> Result<()> {
    connection.execute("INSERT OR IGNORE INTO service_watch VALUES(?1,0,0,0,0,?2,0,0,'disabled',NULL,NULL,NULL,0,0,NULL)",params![project.to_string(),b"[]".as_slice()])?;
    Ok(())
}

pub(super) fn block_source_retry(
    connection: &rusqlite::Connection,
    job: Sha256Digest,
) -> Result<()> {
    connection.execute("UPDATE service_watch SET blocked_revision=max(coalesce(blocked_revision,0),
        (SELECT revision FROM service_job_source WHERE job=?1)) WHERE project=(SELECT project FROM service_jobs WHERE id=?1)
        AND EXISTS(SELECT 1 FROM service_job_source WHERE job=?1)",[job.to_string()])?;
    Ok(())
}

pub(super) fn roots(bytes: &[u8]) -> Result<Vec<String>> {
    ensure!(
        bytes.len() <= 96 * 1024,
        "watch root metadata exceeds byte budget"
    );
    let roots: Vec<String> = serde_json::from_slice(bytes)?;
    ensure!(roots.len() <= 16, "watch additional root limit exceeded");
    let mut unique = std::collections::BTreeSet::new();
    for root in &roots {
        ensure!(
            !root.is_empty()
                && root.len() <= 4096
                && !root.chars().any(char::is_control)
                && unique.insert(root),
            "invalid or duplicate source watch root"
        );
    }
    Ok(roots)
}

impl ServiceRegistry {
    /// A new private watcher incarnation cannot inherit another process's claim
    /// that native registrations are active. Root configuration stays retained.
    pub fn reset_native_watch_observations(&mut self) -> Result<()> {
        self.connection.execute("UPDATE service_watch SET native_roots=0,
            backend=CASE WHEN enabled=1 THEN 'periodic_fallback' ELSE 'disabled' END,
            failure=CASE WHEN failure='metadata_unavailable' THEN failure WHEN enabled=1 THEN 'backend_unavailable' ELSE NULL END,
            observed_ms=NULL",[])?;
        Ok(())
    }
    /// A bounded rotating roster, not one atomic multi-page snapshot.
    pub fn watch_projects(
        &self,
        after: Option<ProjectBinding>,
        limit: usize,
    ) -> Result<Vec<WatchProject>> {
        ensure!((1..=32).contains(&limit), "invalid watch roster page limit");
        let mut statement = self
            .connection
            .prepare("SELECT binding FROM projects WHERE binding>?1 AND registered=1 ORDER BY binding LIMIT ?2")?;
        let ids = statement
            .query_map(
                params![
                    after.map(|id| id.to_string()).unwrap_or_default(),
                    limit as i64
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut page = Vec::new();
        let mut bytes = 0;
        for id in ids {
            let project = self.watch(id.parse()?)?;
            let weight = project
                .additional_roots
                .iter()
                .map(String::len)
                .sum::<usize>()
                + serde_json::to_vec(&project.project)?.len()
                + serde_json::to_vec(&project.observation)?.len();
            if bytes + weight > 512 * 1024 {
                break;
            }
            bytes += weight;
            page.push(project);
        }
        Ok(page)
    }

    pub fn due_source_projects(
        &self,
        after: Option<ProjectBinding>,
        now_ms: u64,
        limit: usize,
    ) -> Result<Vec<(ProjectBinding, u64)>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid source admission page limit"
        );
        let mut statement=self.connection.prepare("SELECT w.project,w.requested FROM service_watch w JOIN projects p ON p.binding=w.project
            WHERE w.project>?1 AND w.enabled=1 AND p.paused=0 AND w.requested>w.indexed AND w.attempts<3
            AND (w.blocked_revision IS NULL OR w.blocked_revision<w.requested) AND w.next_ms<=?2
            AND NOT EXISTS(SELECT 1 FROM service_recovery_budget b WHERE b.scope='source:'||w.project AND (b.failures>=3 OR b.next_ms>?2))
            AND NOT EXISTS(SELECT 1 FROM service_jobs j WHERE j.project=w.project AND j.state IN('pending','running') AND NOT EXISTS(SELECT 1 FROM service_job_recovery r WHERE r.job=j.id AND r.waiting=1 AND j.kind='model_setup')) ORDER BY w.project LIMIT ?3")?;
        let rows = statement.query_map(
            params![
                after.map(|id| id.to_string()).unwrap_or_default(),
                i64::try_from(now_ms)?,
                limit as i64
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        rows.map(|row| {
            let (id, revision) = row?;
            Ok((id.parse()?, u64::try_from(revision)?))
        })
        .collect()
    }

    pub fn watch(&self, project: ProjectBinding) -> Result<WatchProject> {
        let registered = self.get(project)?;
        let row=self.connection.query_row("SELECT enabled,requested,indexed,configuration_epoch,substr(roots,1,98305),roots_observed,native_roots,backend,failure,observed_ms,last_reconciled_ms,attempts,blocked_revision FROM service_watch WHERE project=?1",
            [project.to_string()],|row|Ok((row.get::<_,bool>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?,row.get::<_,Vec<u8>>(4)?,row.get::<_,bool>(5)?,row.get::<_,u16>(6)?,row.get::<_,String>(7)?,row.get::<_,Option<String>>(8)?,row.get::<_,Option<i64>>(9)?,row.get::<_,Option<i64>>(10)?,row.get::<_,u8>(11)?,row.get::<_,Option<i64>>(12)?))).optional()?;
        let Some((
            enabled,
            requested,
            indexed,
            epoch,
            bytes,
            observed,
            native_roots,
            backend,
            failure,
            observed_ms,
            last,
            attempts,
            blocked,
        )) = row
        else {
            return Ok(WatchProject {
                project: registered,
                configuration_epoch: 0,
                additional_roots: vec![],
                observation: WatchObservation {
                    enabled: false,
                    requested_revision: 0,
                    completed_revision: 0,
                    automatic_retries_exhausted: false,
                    automatic_retry_canceled: false,
                    configuration_observed: false,
                    configured_roots: 1,
                    native_roots: 0,
                    backend: WatchBackend::Disabled,
                    failure: None,
                    observed_unix_ms: None,
                    last_reconciled_unix_ms: None,
                    traversal: None,
                    traversal_observed_unix_ms: None,
                },
            });
        };
        let additional_roots = roots(&bytes)?;
        let traversal = if self.watch_traversal_state {
            self.connection.query_row(
            "SELECT substr(observation,1,8193),observed_ms FROM service_watch_traversal WHERE project=?1 AND configuration_epoch=?2",
            params![project.to_string(),epoch], |row| Ok((row.get::<_,Vec<u8>>(0)?,row.get::<_,i64>(1)?))).optional()?
        } else {
            None
        };
        let traversal = traversal
            .map(|(bytes, time)| -> Result<_> {
                ensure!(bytes.len() <= 8192, "oversized watch traversal observation");
                let value: WatchTraversal = serde_json::from_slice(&bytes)?;
                value.validate()?;
                Ok((value, u64::try_from(time)?))
            })
            .transpose()?;
        let observation = WatchObservation {
            enabled,
            requested_revision: u64::try_from(requested)?,
            completed_revision: u64::try_from(indexed)?,
            automatic_retries_exhausted: enabled
                && requested > indexed
                && (attempts >= 3 || self.source_failure_blocked(project)?),
            automatic_retry_canceled: enabled
                && requested > indexed
                && blocked.is_some_and(|blocked| blocked >= requested),
            configuration_observed: observed,
            configured_roots: 1 + additional_roots.len() as u16,
            native_roots,
            backend: serde_json::from_value(backend.into())?,
            failure: failure
                .map(|value| serde_json::from_value(value.into()))
                .transpose()?,
            observed_unix_ms: observed_ms.map(u64::try_from).transpose()?,
            last_reconciled_unix_ms: last.map(u64::try_from).transpose()?,
            traversal_observed_unix_ms: traversal.as_ref().map(|(_, time)| *time),
            traversal: traversal.map(|(value, _)| value),
        };
        observation.validate()?;
        Ok(WatchProject {
            project: registered,
            configuration_epoch: u64::try_from(epoch)?,
            additional_roots,
            observation,
        })
    }

    /// Scalar desired state is idempotent. Enabling records a durable reconciliation
    /// requirement before any background queue admission takes place.
    pub fn configure_watch(&mut self, project: ProjectBinding, enabled: bool) -> Result<()> {
        self.get(project)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let registered: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE binding=?1 AND registered=1)",
            [project.to_string()],
            |row| row.get(0),
        )?;
        ensure!(registered, "service project is not registered");
        ensure_row(&tx, project)?;
        let changed=tx.execute("UPDATE service_watch SET enabled=?1,configuration_epoch=configuration_epoch+1,
            requested=requested+CASE WHEN ?1 THEN 1 ELSE 0 END,attempts=0,next_ms=0,blocked_revision=NULL,
            backend=CASE WHEN ?1 THEN 'periodic_fallback' ELSE 'disabled' END,native_roots=0
            WHERE project=?2 AND enabled<>?1 AND configuration_epoch<9223372036854775807
            AND (NOT ?1 OR requested<9223372036854775807)",params![enabled,project.to_string()])?;
        if changed > 0 {
            bump(&tx)?;
        } else {
            let saved: bool = tx.query_row(
                "SELECT enabled FROM service_watch WHERE project=?1",
                [project.to_string()],
                |row| row.get(0),
            )?;
            ensure!(saved == enabled, "source watcher revision exhausted");
        }
        tx.commit()?;
        Ok(())
    }

    /// Only a fenced project owner may supply additional roots. The kernel validates
    /// their canonical source/config binding; this ledger never opens collection state.
    pub fn record_watch_roots(
        &mut self,
        lease: &ProjectLease,
        additional: &[String],
    ) -> Result<Sha256Digest> {
        let mut additional = additional.to_vec();
        additional.sort();
        let bytes = serde_json::to_vec(&additional)?;
        roots(&bytes)?;
        let digest = airk_core::identity::framed_digest(
            b"airk-service-watch-roots/1",
            &[lease.binding.as_bytes(), &bytes],
        );
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::jobs::worker_epoch(&tx, &self.root, lease)?;
        let payload: Vec<u8> = tx.query_row(
            "SELECT substr(payload,1,20481) FROM projects WHERE binding=?1",
            [lease.binding.to_string()],
            |row| row.get(0),
        )?;
        let registered = super::decode(&payload, 0, false)?;
        super::source_isolation::validate(&tx, &self.root, &registered.registration, &additional)?;
        ensure_row(&tx, lease.binding)?;
        let prior: (Vec<u8>, bool) = tx.query_row(
            "SELECT substr(roots,1,98305),roots_observed FROM service_watch WHERE project=?1",
            [lease.binding.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        ensure!(prior.0.len() <= 96 * 1024, "invalid retained watch roots");
        if prior.0 != bytes || !prior.1 {
            let total: i64 =
                tx.query_row("SELECT sum(length(roots)) FROM service_watch", [], |row| {
                    row.get(0)
                })?;
            ensure!(
                total >= prior.0.len() as i64
                    && total as usize - prior.0.len() + bytes.len() <= 8 * 1024 * 1024,
                "global watch root metadata budget exhausted"
            );
            let changed=tx.execute("UPDATE service_watch SET roots=?1,roots_observed=1,configuration_epoch=configuration_epoch+1,
                requested=requested+enabled,attempts=0,next_ms=0,blocked_revision=NULL,native_roots=0,
                backend=CASE WHEN enabled=1 THEN 'periodic_fallback' ELSE 'disabled' END,failure=NULL,observed_ms=NULL
                WHERE project=?2 AND configuration_epoch<9223372036854775807 AND (enabled=0 OR requested<9223372036854775807)",params![bytes,lease.binding.to_string()])?;
            ensure!(changed == 1, "source watcher revision exhausted");
            bump(&tx)?;
        }
        tx.commit()?;
        Ok(digest)
    }

    /// Durable duplicate suppression for a private, fenced watcher incarnation.
    /// Stale configurations are acknowledged without applying their old hints.
    pub fn source_event(
        &mut self,
        project: ProjectBinding,
        configuration_epoch: u64,
        worker: airk_core::identity::Incarnation,
        sequence: u64,
    ) -> Result<bool> {
        ensure!(sequence > 0, "invalid source event sequence");
        let sequence = i64::try_from(sequence)?;
        let epoch = i64::try_from(configuration_epoch)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let eligible:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM service_watch WHERE project=?1 AND enabled=1 AND configuration_epoch=?2)",params![project.to_string(),epoch],|row|row.get(0))?;
        if !eligible {
            return Ok(false);
        }
        let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM service_watch_events WHERE project=?1 AND worker=?2 AND configuration_epoch=?3 AND sequence>=?4)",
            params![project.to_string(),worker.to_string(),epoch,sequence],|row|row.get(0))?;
        if duplicate {
            return Ok(false);
        }
        ensure!(tx.execute("UPDATE service_watch SET requested=requested+1,attempts=0,next_ms=0,blocked_revision=NULL WHERE project=?1 AND requested<9223372036854775807",[project.to_string()])?==1,"source revision exhausted");
        tx.execute("INSERT INTO service_watch_events VALUES(?1,?2,?3,?4) ON CONFLICT(project) DO UPDATE SET worker=excluded.worker,configuration_epoch=excluded.configuration_epoch,sequence=excluded.sequence",
            params![project.to_string(),worker.to_string(),epoch,sequence])?;
        tx.commit()?;
        Ok(true)
    }

    pub fn source_changed(
        &mut self,
        project: ProjectBinding,
        configuration_epoch: u64,
    ) -> Result<()> {
        let changed=self.connection.execute("UPDATE service_watch SET requested=requested+1,attempts=0,next_ms=0,blocked_revision=NULL
            WHERE project=?1 AND enabled=1 AND configuration_epoch=?2 AND requested<9223372036854775807",params![project.to_string(),i64::try_from(configuration_epoch)?])?;
        ensure!(
            changed == 1,
            "source watcher configuration changed or is disabled"
        );
        Ok(())
    }

    /// Retain the last known roots for diagnosis without calling them current.
    /// Repeated identical failures do not create an endless reconciliation loop.
    pub fn invalidate_watch_roots(&mut self, lease: &ProjectLease) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::jobs::worker_epoch(&tx, &self.root, lease)?;
        ensure_row(&tx, lease.binding)?;
        let changed=tx.execute("UPDATE service_watch SET roots_observed=0,native_roots=0,
            backend=CASE WHEN enabled=1 THEN 'periodic_fallback' ELSE 'disabled' END,failure='metadata_unavailable',
            configuration_epoch=configuration_epoch+1,requested=requested+enabled
            WHERE project=?1 AND (roots_observed=1 OR failure IS NULL OR failure<>'metadata_unavailable')
            AND configuration_epoch<9223372036854775807 AND (enabled=0 OR requested<9223372036854775807)",[lease.binding.to_string()])?;
        if changed > 0 {
            bump(&tx)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn record_watch_observation(
        &mut self,
        project: ProjectBinding,
        configuration_epoch: u64,
        native_roots: u16,
        backend: WatchBackend,
        failure: Option<WatchFailure>,
        now_ms: u64,
    ) -> Result<()> {
        let mut state = self.watch(project)?;
        state.observation.native_roots = native_roots;
        state.observation.backend = backend;
        state.observation.failure = failure;
        state.observation.validate()?;
        let backend = serde_json::to_value(backend)?
            .as_str()
            .context("invalid watcher backend")?
            .to_owned();
        let failure = failure
            .map(|failure| {
                serde_json::to_value(failure).map(|value| value.as_str().unwrap_or("").to_owned())
            })
            .transpose()?;
        let changed=self.connection.execute("UPDATE service_watch SET native_roots=?1,backend=?2,failure=?3,observed_ms=?4 WHERE project=?5 AND configuration_epoch=?6",
            params![native_roots,backend,failure,i64::try_from(now_ms)?,project.to_string(),i64::try_from(configuration_epoch)?])?;
        ensure!(changed == 1, "source watcher configuration changed");
        Ok(())
    }

    /// Merge observed failures until their root is fully traversed. Return only
    /// availability transitions for the bounded, path-free process log. Durable
    /// state suppresses repeat events across watcher/service restarts.
    pub fn record_watch_traversal(
        &mut self,
        project: ProjectBinding,
        epoch: u64,
        report: &WatchTraversal,
        now_ms: u64,
    ) -> Result<Option<bool>> {
        report.validate()?;
        let state = self.watch(project)?;
        ensure!(
            state.configuration_epoch == epoch,
            "source watcher configuration changed"
        );
        ensure!(
            report
                .incomplete_roots
                .iter()
                .all(|index| *index < state.observation.configured_roots),
            "foreign watch root index"
        );
        let roots = std::iter::once(state.project.registration.root.as_str())
            .chain(state.additional_roots.iter().map(String::as_str))
            .collect::<Vec<_>>();
        for entry in &report.unreadable {
            let path = std::path::Path::new(&entry.path);
            ensure!(
                path.starts_with(roots[usize::from(entry.root)])
                    && !path
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir)),
                "foreign watch diagnostic path"
            );
        }
        let previous = state.observation.traversal.unwrap_or_default();
        let was_unavailable = previous.unavailable();
        let mut merged = WatchTraversal {
            unreadable: vec![],
            incomplete_roots: report.incomplete_roots.clone(),
            truncated: report.truncated
                || (previous.truncated && !report.incomplete_roots.is_empty()),
        };
        for entry in previous
            .unreadable
            .into_iter()
            .filter(|entry| report.incomplete_roots.contains(&entry.root))
        {
            merged.push(entry);
        }
        for entry in &report.unreadable {
            merged.push(entry.clone());
        }
        merged.validate()?;
        let unavailable = merged.unavailable();
        let bytes = serde_json::to_vec(&merged)?;
        let changed = self.connection.execute("INSERT INTO service_watch_traversal(project,configuration_epoch,observation,observed_ms)
            SELECT project,configuration_epoch,?3,?4 FROM service_watch WHERE project=?1 AND configuration_epoch=?2
            ON CONFLICT(project) DO UPDATE SET configuration_epoch=excluded.configuration_epoch,observation=excluded.observation,observed_ms=excluded.observed_ms",
            params![project.to_string(),i64::try_from(epoch)?,bytes,i64::try_from(now_ms)?])?;
        ensure!(changed == 1, "source watcher configuration changed");
        Ok((was_unavailable != unavailable).then_some(unavailable))
    }
}

#[cfg(test)]
#[path = "service_watch_tests.rs"]
mod tests;
