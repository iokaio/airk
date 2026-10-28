// SPDX-License-Identifier: Apache-2.0
//! Bounded recovery metadata. Existing jobs and receipts remain authoritative.
use super::ServiceRegistry;
use airk_core::{
    identity::Sha256Digest,
    service::{JobKind, JobOutcome, JobState, ProjectBinding},
    service_console::RecoveryObservation,
};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS service_recovery_budget(
 scope TEXT PRIMARY KEY,engine TEXT NOT NULL,failures INTEGER NOT NULL CHECK(failures BETWEEN 0 AND 3),
 next_ms INTEGER NOT NULL,started_ms INTEGER NOT NULL,suppressed INTEGER NOT NULL CHECK(suppressed IN(0,1))) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS service_job_recovery(
 job TEXT PRIMARY KEY REFERENCES service_jobs(id) ON DELETE CASCADE,
 reason TEXT NOT NULL CHECK(length(reason)<=128),condition BLOB NOT NULL CHECK(length(condition)<=1024),
 waiting INTEGER NOT NULL CHECK(waiting IN(0,1)),next_ms INTEGER NOT NULL,started_ms INTEGER NOT NULL) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS service_semantic_review(
 project TEXT PRIMARY KEY REFERENCES projects(binding),signature TEXT NOT NULL CHECK(length(signature)<=129)) WITHOUT ROWID;";
pub(super) const ELIGIBLE: &str =
    "NOT EXISTS(SELECT 1 FROM service_job_recovery r WHERE r.job=j.id AND r.waiting=1)";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveryCondition {
    Delay,
    Helper,
    Source,
    Capacity {
        required: u64,
    },
    Quota {
        required: u64,
        scope: crate::quota::PressureScope,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::ProjectLease;
    use airk_core::{
        identity::Incarnation,
        service::{ProjectRegistration, RequestKey},
        service_configuration::{ConfigurationRequest, ProjectConfiguration},
    };
    fn nonce(n: u8) -> Incarnation {
        Incarnation::from_bytes([n; 16])
    }
    fn digest(n: u8) -> Sha256Digest {
        Sha256Digest::from_bytes([n; 32])
    }
    fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, ProjectLease, RequestKey)> {
        let root = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(root.path(), nonce(1))?;
        let project = ProjectRegistration::new(
            "/recovery-source".into(),
            "/recovery-data".into(),
            "recovery".into(),
        )?;
        registry.register(&project)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        let (namespace, epoch) = registry.request_epoch()?;
        Ok((
            root,
            registry,
            lease,
            RequestKey {
                namespace,
                epoch,
                nonce: nonce(2),
            },
        ))
    }
    #[test]
    fn registry_v7_migration_preserves_registration_receipts_and_refuses_missing_v8_state()
    -> Result<()> {
        let (root, mut registry, lease, key) = fixture()?;
        let job = registry.enqueue(lease.identity().0, nonce(3), &key, JobKind::Index)?;
        registry.connection.execute_batch("DROP TABLE service_setup_pending; DROP TABLE service_recovery_budget; DROP TABLE service_job_recovery; DROP TABLE service_semantic_review; PRAGMA user_version=7;")?;
        drop(registry);
        let saved = ServiceRegistry::open_saved(root.path())?;
        assert!(saved.recovery_observations(job.project)?.is_empty());
        drop(saved);
        let registry = ServiceRegistry::open(root.path(), nonce(9))?;
        assert_eq!(
            registry.request_receipt(nonce(3), &key)?.0.unwrap().id,
            job.id
        );
        assert_eq!(registry.incarnation()?, key.namespace);
        registry
            .connection
            .execute_batch("DROP TABLE service_job_recovery")?;
        drop(registry);
        assert!(ServiceRegistry::open(root.path(), nonce(9)).is_err());
        Ok(())
    }
    #[test]
    fn deferred_job_keeps_receipt_survives_restart_and_waiting_does_not_consume_attempts()
    -> Result<()> {
        let (root, mut registry, lease, key) = fixture()?;
        let project = lease.identity().0;
        let accepted = registry.enqueue(project, nonce(3), &key, JobKind::Index)?;
        registry.claim_job(&lease)?.unwrap();
        assert!(registry.defer_job_failure(
            &lease,
            accepted.id,
            "source_unavailable",
            &RecoveryCondition::Source,
            100
        )?);
        assert!(registry.next_pending_job(project)?.is_none());
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(9))?;
        for tick in [10000, 40000, 70000] {
            registry.check_recovery(accepted.id, false, tick)?;
            assert_eq!(registry.job(accepted.id)?.attempts, 1);
        }
        assert_eq!(
            registry.request_receipt(nonce(3), &key)?.0.unwrap().id,
            accepted.id
        );
        registry.pause(project, true)?;
        registry.check_recovery(accepted.id, true, 100000)?;
        assert!(registry.next_pending_job(project)?.is_none());
        registry.pause(project, false)?;
        registry.check_recovery(accepted.id, true, 100001)?;
        assert_eq!(registry.claim_job(&lease)?.unwrap().attempts, 2);
        registry.complete_job(
            &lease,
            accepted.id,
            JobOutcome::Succeeded { revision: None },
        )?;
        assert_eq!(
            registry
                .recovery_observations(project)?
                .last()
                .unwrap()
                .state,
            "recovered"
        );
        Ok(())
    }
    #[test]
    fn waiting_model_setup_allows_watched_text_updates_without_bypassing_pause() -> Result<()> {
        let (_root, mut registry, lease, key) = fixture()?;
        let project = lease.identity().0;
        registry.configure_watch(project, true)?;
        let setup = registry.enqueue(project, nonce(3), &key, JobKind::ModelSetup)?;
        registry.claim_job(&lease)?.unwrap();
        assert!(registry.defer_job_failure(
            &lease,
            setup.id,
            "helper_unavailable",
            &RecoveryCondition::Helper,
            100
        )?);
        let due = registry.due_source_projects(None, 1000, 32)?;
        assert_eq!(due.len(), 1);
        let source =
            registry.enqueue_source_reconcile(project, nonce(3), nonce(4), due[0].1, 1000)?;
        assert_eq!(
            registry.claim_job_id(&lease, source.id)?.unwrap().id,
            source.id
        );
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        assert_eq!(registry.job(setup.id)?.state, JobState::Pending);
        let epoch = registry.watch(project)?.configuration_epoch;
        registry.source_changed(project, epoch)?;
        registry.pause(project, true)?;
        assert!(registry.due_source_projects(None, 10000, 32)?.is_empty());
        registry.pause(project, false)?;
        assert_eq!(registry.due_source_projects(None, 10000, 32)?.len(), 1);
        Ok(())
    }
    #[test]
    fn cancellation_and_cross_job_stall_budget_win_over_recovery() -> Result<()> {
        let (_root, mut registry, lease, mut key) = fixture()?;
        let project = lease.identity().0;
        let first = registry.enqueue(project, nonce(3), &key, JobKind::Index)?;
        registry.claim_job(&lease)?;
        assert!(registry.defer_job_failure(
            &lease,
            first.id,
            "work_stalled",
            &RecoveryCondition::Delay,
            1
        )?);
        registry.cancel_pending(first.id)?;
        registry.check_recovery(first.id, true, 90000)?;
        assert_eq!(registry.job(first.id)?.state, JobState::Canceled);
        let next = registry.enqueue_internal(project, nonce(3), JobKind::Index, nonce(4))?;
        registry.claim_job(&lease)?;
        assert!(!registry.defer_job_failure(
            &lease,
            next.id,
            "work_stalled",
            &RecoveryCondition::Delay,
            90001
        )?);
        assert!(!registry.defer_job_failure(
            &lease,
            next.id,
            "protected_source_boundary",
            &RecoveryCondition::Delay,
            90001
        )?);
        registry.complete_job(
            &lease,
            next.id,
            JobOutcome::Failed {
                code: "work_stalled".into(),
            },
        )?;
        key.nonce = nonce(5);
        let explicit = registry.enqueue(project, nonce(3), &key, JobKind::Index)?;
        registry.claim_job_id(&lease, explicit.id)?.unwrap();
        assert!(registry.defer_job_failure(
            &lease,
            explicit.id,
            "work_stalled",
            &RecoveryCondition::Delay,
            90002
        )?);
        Ok(())
    }
    #[test]
    fn retry_budget_and_explicit_helper_stop_survive_restart() -> Result<()> {
        let (root, mut registry, lease, _) = fixture()?;
        let scope = lease.identity().0.to_string();
        for i in 1..=4 {
            assert_eq!(
                registry
                    .recovery_failed(&scope, digest(3), i * 1000)?
                    .failures,
                (i as u32).min(3)
            );
        }
        registry.suppress_helper_recovery(digest(3))?;
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(8))?;
        assert_eq!(registry.recovery_budget(&scope, digest(3))?.failures, 3);
        assert_eq!(registry.recovery_budget(&scope, digest(4))?.failures, 0);
        assert!(registry.recovery_budget("helper", digest(4))?.suppressed);
        registry.reset_recovery_budget(&scope)?;
        assert_eq!(registry.recovery_budget(&scope, digest(3))?.failures, 0);
        Ok(())
    }
    #[test]
    fn terminal_source_failures_survive_new_hints_and_restart_until_explicit_retry() -> Result<()> {
        let (root, mut registry, lease, mut key) = fixture()?;
        let project = lease.identity().0;
        registry.configure_watch(project, true)?;
        let epoch = registry.watch(project)?.configuration_epoch;
        let mut failed = None;
        for attempt in 1..=3u8 {
            let now = u64::from(attempt) * 100_000;
            let due = registry.due_source_projects(None, now, 32)?;
            let job = registry.enqueue_source_reconcile(
                project,
                nonce(3),
                nonce(10 + attempt),
                due[0].1,
                now,
            )?;
            registry.claim_job_id(&lease, job.id)?.unwrap();
            registry.complete_job_at(
                &lease,
                job.id,
                JobOutcome::Failed {
                    code: "project_operation_failed".into(),
                },
                now + 1,
            )?;
            failed = Some(job.id);
            registry.source_event(project, epoch, nonce(4), u64::from(attempt))?;
            assert!(
                registry.due_source_projects(None, now + 2, 32)?.is_empty(),
                "source hint bypassed backoff"
            );
        }
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(9))?;
        assert!(
            registry
                .due_source_projects(None, 1_000_000, 32)?
                .is_empty()
        );
        assert!(registry.source_failure_blocked(project)?);
        let recovery = registry.recovery_observations(project)?;
        assert!(
            recovery
                .iter()
                .any(|r| r.state == "needs_attention" && r.job == failed)
        );
        key.nonce = nonce(20);
        let retry = registry.retry_job(failed.unwrap(), nonce(3), &key)?;
        assert!(!registry.source_failure_blocked(project)?);
        registry.claim_job_id(&lease, retry.id)?.unwrap();
        registry.complete_job_at(
            &lease,
            retry.id,
            JobOutcome::Succeeded { revision: None },
            1_000_001,
        )?;
        assert!(
            registry
                .recovery_observations(project)?
                .iter()
                .all(|r| r.reason != "project_operation_failed")
        );
        Ok(())
    }
    #[test]
    fn successful_publication_recovery_supersedes_old_attention_without_rewriting_failure()
    -> Result<()> {
        let (_root, mut registry, lease, key) = fixture()?;
        let project = lease.identity().0;
        registry.configure_watch(project, true)?;
        let failed = registry.enqueue(project, nonce(3), &key, JobKind::Index)?;
        registry.claim_job_id(&lease, failed.id)?.unwrap();
        registry.defer_job_failure(
            &lease,
            failed.id,
            "work_stalled",
            &RecoveryCondition::Delay,
            100,
        )?;
        registry.check_recovery(failed.id, true, 100_000)?;
        registry.claim_job_id(&lease, failed.id)?.unwrap();
        registry.complete_job_at(
            &lease,
            failed.id,
            JobOutcome::Failed {
                code: "work_stalled".into(),
            },
            100_001,
        )?;
        assert!(
            registry
                .recovery_observations(project)?
                .iter()
                .any(|r| r.reason == "work_stalled")
        );
        let recovery = registry.enqueue_internal(project, nonce(3), JobKind::Recover, nonce(30))?;
        registry.claim_job_id(&lease, recovery.id)?.unwrap();
        registry.complete_job_at(
            &lease,
            recovery.id,
            JobOutcome::Succeeded {
                revision: Some(airk_core::identity::RevisionToken {
                    incarnation: nonce(1),
                    generation: 1,
                    revision: digest(1),
                }),
            },
            100_002,
        )?;
        assert!(
            registry
                .recovery_observations(project)?
                .iter()
                .all(|r| r.reason != "work_stalled")
        );
        assert_eq!(registry.job(failed.id)?.state, JobState::Failed);
        assert!(!registry.due_source_projects(None, 200_000, 32)?.is_empty());
        Ok(())
    }
    #[test]
    fn scalar_preference_is_durable_before_inspection_and_receipt_lookup_is_read_only() -> Result<()>
    {
        let (root, mut registry, lease, key) = fixture()?;
        let project = lease.identity().0;
        let request = ConfigurationRequest {
            prepare: true,
            incarnation: key.namespace,
            settings: project,
            configuration: ProjectConfiguration::Vectors { enabled: true },
        };
        let job = registry.enqueue_configuration(project, nonce(3), &key, &request, digest(4))?;
        registry.claim_job(&lease)?;
        drop(lease);
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(7))?;
        let mut replacement = ProjectLease::acquire(root.path(), project)?;
        registry.admit_worker(&mut replacement)?;
        registry.recover_jobs(&replacement)?;
        assert_eq!(registry.job(job.id)?.state, JobState::Pending);
        let before = registry.connection.total_changes();
        assert_eq!(
            registry.request_receipt(nonce(3), &key)?.0.unwrap().id,
            job.id
        );
        let unknown = RequestKey {
            namespace: nonce(8),
            epoch: 99,
            nonce: nonce(8),
        };
        let (receipt, expired) = registry.request_receipt(nonce(3), &unknown)?;
        assert!(receipt.is_none() && expired);
        assert_eq!(registry.connection.total_changes(), before);
        let invalid = ConfigurationRequest {
            configuration: ProjectConfiguration::Unregister,
            ..request
        };
        assert!(invalid.validate().is_err());
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct RecoveryBudget {
    pub failures: u32,
    pub next_ms: u64,
    pub suppressed: bool,
}
pub struct WaitingRecovery {
    pub job: Sha256Digest,
    pub project: ProjectBinding,
    pub condition: RecoveryCondition,
}
fn scope_valid(scope: &str) -> Result<()> {
    ensure!(
        scope == "helper"
            || scope
                .strip_prefix("work:")
                .or_else(|| scope.strip_prefix("source:"))
                .unwrap_or(scope)
                .parse::<ProjectBinding>()
                .is_ok(),
        "invalid recovery scope"
    );
    Ok(())
}
fn delay(scope: &str, attempt: u32) -> u64 {
    // Stable bounded jitter; restarting the service cannot shorten the saved deadline.
    (1u64 << attempt.min(3)) * 1000 + scope.bytes().map(u64::from).sum::<u64>() % 1000
}
impl ServiceRegistry {
    pub fn source_failure_blocked(&self, project: ProjectBinding) -> Result<bool> {
        let version: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 8 {
            return Ok(false);
        }
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_recovery_budget WHERE scope=?1 AND failures>=3)",
            [format!("source:{project}")],
            |r| r.get(0),
        )?)
    }
    pub fn has_recovery_waits(&self) -> Result<bool> {
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM service_job_recovery r JOIN service_jobs j ON j.id=r.job JOIN projects p ON p.binding=j.project WHERE r.waiting=1 AND j.state='pending' AND p.registered=1 AND p.paused=0)",[],|r|r.get(0))?)
    }
    /// One reconciliation per engine/settings identity. Canceled work and manual
    /// projects never acquire fresh work merely because a query opens a worker.
    pub fn reconcile_semantic_setup(
        &mut self,
        lease: &crate::service::ProjectLease,
        signature: &str,
    ) -> Result<bool> {
        ensure!(
            signature.len() == 129
                && signature
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() || b == b':'),
            "invalid semantic setup identity"
        );
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::jobs::worker_epoch(&tx, &self.root, lease)?;
        let project = lease.identity().0;
        let eligible:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM projects p JOIN service_watch w ON w.project=p.binding WHERE p.binding=?1 AND p.registered=1 AND p.paused=0 AND w.enabled=1 AND w.blocked_revision IS NULL) AND NOT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND state IN('pending','running','canceled')) AND NOT EXISTS(SELECT 1 FROM service_semantic_review WHERE project=?1 AND signature=?2)",params![project.to_string(),signature],|r|r.get(0))?;
        if !eligible {
            return Ok(false);
        }
        tx.execute("INSERT INTO service_semantic_review VALUES(?1,?2) ON CONFLICT(project) DO UPDATE SET signature=excluded.signature",params![project.to_string(),signature])?;
        super::background::set(&tx, project, JobKind::ModelSetup, true)?;
        super::background::set(&tx, project, JobKind::Index, true)?;
        tx.commit()?;
        Ok(true)
    }
    pub fn recovery_budget(&self, scope: &str, engine: Sha256Digest) -> Result<RecoveryBudget> {
        scope_valid(scope)?;
        let row:Option<(String,u32,i64,bool)>=self.connection.query_row(
            "SELECT engine,failures,next_ms,suppressed FROM service_recovery_budget WHERE scope=?1",[scope],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        Ok(match row {
            Some((saved, failures, next_ms, suppressed)) => RecoveryBudget {
                failures: if saved == engine.to_string() {
                    failures
                } else {
                    0
                },
                next_ms: if saved == engine.to_string() {
                    u64::try_from(next_ms)?
                } else {
                    0
                },
                suppressed,
            },
            None => RecoveryBudget::default(),
        })
    }
    pub fn recovery_failed(
        &mut self,
        scope: &str,
        engine: Sha256Digest,
        now: u64,
    ) -> Result<RecoveryBudget> {
        scope_valid(scope)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO service_recovery_budget VALUES(?1,?2,1,?3,?4,0) ON CONFLICT(scope) DO UPDATE SET failures=CASE WHEN engine=excluded.engine THEN min(failures+1,3) ELSE 1 END,engine=excluded.engine,next_ms=excluded.next_ms",params![scope,engine.to_string(),i64::try_from(now.saturating_add(9000))?,i64::try_from(now)?])?;
        let attempts: u32 = tx.query_row(
            "SELECT failures FROM service_recovery_budget WHERE scope=?1",
            [scope],
            |r| r.get(0),
        )?;
        tx.execute(
            "UPDATE service_recovery_budget SET next_ms=?1 WHERE scope=?2",
            params![
                i64::try_from(now.saturating_add(delay(scope, attempts)))?,
                scope
            ],
        )?;
        tx.commit()?;
        self.recovery_budget(scope, engine)
    }
    pub fn reset_recovery_budget(&mut self, scope: &str) -> Result<()> {
        scope_valid(scope)?;
        self.connection.execute(
            "DELETE FROM service_recovery_budget WHERE scope=?1",
            [scope],
        )?;
        Ok(())
    }
    pub fn suppress_helper_recovery(&mut self, engine: Sha256Digest) -> Result<()> {
        self.connection.execute("INSERT INTO service_recovery_budget VALUES('helper',?1,0,0,0,1) ON CONFLICT(scope) DO UPDATE SET suppressed=1",[engine.to_string()])?;
        Ok(())
    }
    /// Keep the exact accepted job pending; a missing receipt is never recreated.
    pub fn defer_job_failure(
        &mut self,
        lease: &crate::service::ProjectLease,
        id: Sha256Digest,
        code: &str,
        condition: &RecoveryCondition,
        now: u64,
    ) -> Result<bool> {
        let limit = if code == "work_stalled" { 2 } else { 3 };
        let recognized = matches!(
            code,
            "helper_unavailable"
                | "work_stalled"
                | "source_unavailable"
                | "semantic_contention_retry_exhausted"
                | "current_database_admission_busy"
                | "quota_ledger_busy"
                | "physical_storage_headroom"
                | "filesystem_storage_full"
                | "sqlite_storage_full"
                | "storage_quota_root"
                | "storage_quota_collection"
                | "storage_quota_reservation"
        );
        if !recognized {
            return Ok(false);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = super::jobs::worker_epoch(&tx, &self.root, lease)?;
        let job = super::jobs::record(&tx, id)?;
        ensure!(
            job.project == lease.identity().0
                && job.fencing_generation == epoch
                && job.state == JobState::Running,
            "stale recovery owner"
        );
        let safe = matches!(
            job.kind,
            JobKind::Index
                | JobKind::Reconcile
                | JobKind::Vectors
                | JobKind::ModelSetup
                | JobKind::Recover
        ) || job.kind == JobKind::Configure
            && super::configuration::work(&tx, id)?.is_some_and(|w| w.request.prepare);
        let canceled:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM service_job_cancellation WHERE job=?1) OR NOT EXISTS(SELECT 1 FROM projects WHERE binding=?2 AND registered=1)",params![id.to_string(),job.project.to_string()],|r|r.get(0))?;
        if !safe || canceled || job.attempts >= limit {
            return Ok(false);
        }
        // Source edits can create new jobs; they cannot replenish a failure budget.
        let scope = format!("work:{}", job.project);
        let failures: u32 = tx.query_row(
            "SELECT coalesce((SELECT failures FROM service_recovery_budget WHERE scope=?1),0)",
            [&scope],
            |r| r.get(0),
        )?;
        if failures >= limit - 1 {
            return Ok(false);
        }
        tx.execute("INSERT INTO service_recovery_budget VALUES(?1,'work',1,0,?2,0) ON CONFLICT(scope) DO UPDATE SET failures=min(failures+1,3)",params![scope,i64::try_from(now)?])?;
        let bytes = serde_json::to_vec(condition)?;
        ensure!(bytes.len() <= 1024, "recovery condition exceeds bound");
        tx.execute("INSERT INTO service_job_recovery VALUES(?1,?2,?3,1,?4,?5) ON CONFLICT(job) DO UPDATE SET reason=excluded.reason,condition=excluded.condition,waiting=1,next_ms=excluded.next_ms",params![id.to_string(),code,bytes,i64::try_from(now.saturating_add(delay(&id.to_string(),job.attempts)))?,i64::try_from(now)?])?;
        tx.execute(
            "UPDATE service_jobs SET state='pending',outcome=NULL WHERE id=?1",
            [id.to_string()],
        )?;
        tx.commit()?;
        Ok(true)
    }
    pub fn waiting_recoveries(&self, now: u64) -> Result<Vec<WaitingRecovery>> {
        let mut stmt=self.connection.prepare("SELECT r.job,j.project,r.condition FROM service_job_recovery r JOIN service_jobs j ON j.id=r.job JOIN projects p ON p.binding=j.project WHERE r.waiting=1 AND r.next_ms<=?1 AND j.state='pending' AND p.registered=1 AND p.paused=0 ORDER BY r.next_ms,r.job LIMIT 8")?;
        let rows = stmt.query_map([i64::try_from(now)?], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (job, project, bytes) = row?;
            ensure!(bytes.len() <= 1024, "recovery condition exceeds bound");
            Ok(WaitingRecovery {
                job: job.parse()?,
                project: project.parse()?,
                condition: serde_json::from_slice(&bytes)?,
            })
        })
        .collect()
    }
    pub fn check_recovery(&mut self, id: Sha256Digest, ready: bool, now: u64) -> Result<()> {
        self.connection.execute("UPDATE service_job_recovery SET waiting=?1,next_ms=?2 WHERE job=?3 AND EXISTS(SELECT 1 FROM service_jobs j JOIN projects p ON p.binding=j.project WHERE j.id=?3 AND j.state='pending' AND p.registered=1 AND p.paused=0 AND NOT EXISTS(SELECT 1 FROM service_job_cancellation WHERE job=j.id))",params![!ready,i64::try_from(now.saturating_add(30_000))?,id.to_string()])?;
        Ok(())
    }
    pub fn recovery_observations(
        &self,
        project: ProjectBinding,
    ) -> Result<Vec<RecoveryObservation>> {
        let version: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 8 {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        let mut stmt=self.connection.prepare("SELECT scope,failures,next_ms,started_ms,suppressed,engine FROM service_recovery_budget WHERE scope IN(?1,'helper','source:'||?1) ORDER BY scope LIMIT 3")?;
        for row in stmt.query_map([project.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, String>(5)?,
            ))
        })? {
            let (scope, attempts, next, started, stopped, reason) = row?;
            let job = if scope.starts_with("source:") {
                self.connection.query_row("SELECT id FROM service_jobs WHERE project=?1 AND state='failed' AND kind IN('index','reconcile','recover','rebuild') ORDER BY seq DESC LIMIT 1",
                    [project.to_string()],|r|r.get::<_,String>(0)).optional()?.map(|id|id.parse()).transpose()?
            } else {
                None
            };
            result.push(RecoveryObservation {
                reason: if scope.starts_with("source:") {
                    &reason
                } else if scope == "helper" {
                    "helper_unavailable"
                } else {
                    "worker_unavailable"
                }
                .into(),
                state: if stopped {
                    "stopped"
                } else if attempts >= 3 {
                    "needs_attention"
                } else {
                    "retrying"
                }
                .into(),
                attempts,
                limit: 3,
                next_check_unix_ms: u64::try_from(next)?,
                started_unix_ms: u64::try_from(started)?,
                job,
            });
        }
        let mut stmt=self.connection.prepare("SELECT r.reason,r.waiting,r.next_ms,r.started_ms,j.id FROM service_job_recovery r JOIN service_jobs j ON j.id=r.job WHERE j.project=?1 AND NOT EXISTS(SELECT 1 FROM service_jobs newer WHERE newer.project=j.project AND newer.seq>j.seq AND newer.state='succeeded' AND newer.kind IN('index','reconcile','recover','rebuild','repair') AND json_type(CAST(newer.outcome AS TEXT),'$.revision')<>'null') ORDER BY j.seq DESC LIMIT 1")?;
        for row in stmt.query_map([project.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, bool>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })? {
            let (reason, waiting, next, started, id) = row?;
            let job = super::jobs::record(&self.connection, id.parse()?)?;
            let state = match job.outcome {
                Some(JobOutcome::Succeeded { .. }) => "recovered",
                Some(JobOutcome::Canceled) => "canceled",
                Some(JobOutcome::Failed { .. }) => "needs_attention",
                None if waiting => "waiting",
                None => "retrying",
            };
            result.push(RecoveryObservation {
                limit: if reason == "work_stalled" { 2 } else { 3 },
                reason,
                state: state.into(),
                attempts: job.attempts,
                next_check_unix_ms: u64::try_from(next)?,
                started_unix_ms: u64::try_from(started)?,
                job: Some(job.id),
            });
        }
        Ok(result)
    }
}
