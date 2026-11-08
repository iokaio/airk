// SPDX-License-Identifier: Apache-2.0
//! Durable phase continuations independent of the finite public job queue.
use super::ServiceRegistry;
use crate::service::ProjectLease;
use airk_core::service::{JobKind, ProjectBinding};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, TransactionBehavior, params};

#[path = "service_background_audit.rs"]
pub(super) mod audit;

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS service_background(
 project TEXT PRIMARY KEY REFERENCES projects(binding),
 revision INTEGER NOT NULL CHECK(revision>=0),
 source_pending INTEGER NOT NULL CHECK(source_pending IN(0,1)),
 semantic_pending INTEGER NOT NULL CHECK(semantic_pending IN(0,1))) WITHOUT ROWID;";
pub(super) const SETUP_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS service_setup_pending(project TEXT PRIMARY KEY REFERENCES projects(binding)) WITHOUT ROWID;";

fn column(kind: JobKind) -> Result<&'static str> {
    match kind {
        JobKind::Index => Ok("source_pending"),
        JobKind::Vectors => Ok("semantic_pending"),
        _ => anyhow::bail!("unsupported background continuation"),
    }
}

pub(super) fn set(
    connection: &Connection,
    project: ProjectBinding,
    kind: JobKind,
    pending: bool,
) -> Result<()> {
    if kind == JobKind::ModelSetup {
        connection.execute(
            "INSERT OR IGNORE INTO service_background VALUES(?1,0,0,0)",
            [project.to_string()],
        )?;
        let changed = if pending {
            connection.execute(
                "INSERT OR IGNORE INTO service_setup_pending VALUES(?1)",
                [project.to_string()],
            )?
        } else {
            connection.execute(
                "DELETE FROM service_setup_pending WHERE project=?1",
                [project.to_string()],
            )?
        };
        if changed > 0 {
            ensure!(connection.execute("UPDATE service_background SET revision=revision+1 WHERE project=?1 AND revision<9223372036854775807",[project.to_string()])?==1,"background continuation revision exhausted");
        }
        return Ok(());
    }
    let column = column(kind)?;
    connection.execute(
        "INSERT OR IGNORE INTO service_background VALUES(?1,0,0,0)",
        [project.to_string()],
    )?;
    let (current, revision): (bool, i64) = connection.query_row(
        &format!("SELECT {column},revision FROM service_background WHERE project=?1"),
        [project.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if current != pending {
        ensure!(
            revision < i64::MAX,
            "background continuation revision exhausted"
        );
        ensure!(connection.execute(&format!("UPDATE service_background SET {column}=?1,revision=revision+1 WHERE project=?2 AND revision=?3"),params![pending, project.to_string(), revision])?==1,"background continuation changed");
    }
    Ok(())
}

/// Called inside job acceptance, after insertion/join. Failure rolls back both
/// the request receipt and debit, preserving the next unaccepted continuation.
pub(super) fn debit(
    connection: &Connection,
    project: ProjectBinding,
    kind: JobKind,
    revision: u64,
    job: airk_core::identity::Sha256Digest,
    now_ms: u64,
) -> Result<()> {
    let predicate = if kind == JobKind::ModelSetup {
        "EXISTS(SELECT 1 FROM service_setup_pending s WHERE s.project=b.project)".to_owned()
    } else {
        format!("b.{}=1", column(kind)?)
    };
    let expected = i64::try_from(revision)?;
    ensure!(
        expected < i64::MAX,
        "background continuation revision exhausted"
    );
    let eligible:bool=connection.query_row(&format!("SELECT EXISTS(SELECT 1 FROM service_background b JOIN projects p ON p.binding=b.project WHERE b.project=?1 AND b.revision=?2 AND {predicate} AND p.paused=0 AND p.registered=1 AND NOT EXISTS(SELECT 1 FROM service_jobs j WHERE j.project=b.project AND j.id<>?3 AND j.state IN('pending','running') AND NOT (b.source_pending=1 AND j.state='pending' AND j.kind='vectors') AND NOT EXISTS(SELECT 1 FROM service_background_audit a WHERE a.job=j.id AND a.manual=0 AND j.state='pending') AND NOT EXISTS(SELECT 1 FROM service_job_recovery r WHERE r.job=j.id AND r.waiting=1 AND j.kind='model_setup')))"),params![project.to_string(),expected,job.to_string()],|row|row.get(0))?;
    ensure!(eligible, "background continuation is no longer eligible");
    if kind == JobKind::Vectors {
        ensure!(
            super::watch::automatic_source_candidate(connection, project, now_ms, Some(job))?
                .is_none(),
            "source reconciliation takes the next writer opportunity"
        );
    }
    set(connection, project, kind, false)?;
    Ok(())
}

impl ServiceRegistry {
    /// Exact retained phase intent, including paused or presently ineligible
    /// work. A scheduler's empty due page is not evidence of quiescence.
    pub fn project_background_pending(&self, project: ProjectBinding) -> Result<bool> {
        self.get(project)?;
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM service_background WHERE project=?1 AND (source_pending=1 OR semantic_pending=1)) OR EXISTS(SELECT 1 FROM service_setup_pending WHERE project=?1)",[project.to_string()],|row|row.get(0))?)
    }

    /// Only reviewed native setup may create reserved storage during worker startup.
    pub fn project_setup_pending(&self, project: ProjectBinding) -> Result<bool> {
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM service_setup_pending WHERE project=?1) OR EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND kind='model_setup' AND state IN('pending','running'))",[project.to_string()],|row|row.get(0))?)
    }

    /// The lock-bearing native owner records the next phase before completing
    /// its current job. Queue saturation cannot discard this bounded intent.
    pub fn set_background_pending(
        &mut self,
        lease: &ProjectLease,
        kind: JobKind,
        pending: bool,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::jobs::worker_epoch(&transaction, &self.root, lease)?;
        set(&transaction, lease.identity().0, kind, pending)?;
        transaction.commit()?;
        Ok(())
    }
    pub fn due_background_jobs(
        &self,
        after: Option<ProjectBinding>,
        now_ms: u64,
        limit: usize,
    ) -> Result<Vec<(ProjectBinding, u64, JobKind)>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid background continuation page bound"
        );
        let source_ready = super::watch::automatic_source_where("?3", "NULL");
        let mut statement = self.connection.prepare(&format!("SELECT b.project,b.revision,b.source_pending,EXISTS(SELECT 1 FROM service_setup_pending s WHERE s.project=b.project) FROM service_background b JOIN projects p ON p.binding=b.project WHERE b.project>?1 AND p.paused=0 AND p.registered=1 AND (b.source_pending=1 OR b.semantic_pending=1 OR EXISTS(SELECT 1 FROM service_setup_pending s WHERE s.project=b.project)) AND NOT EXISTS(SELECT 1 FROM service_jobs j WHERE j.project=b.project AND j.state IN('pending','running') AND NOT (b.source_pending=1 AND j.state='pending' AND j.kind='vectors') AND NOT EXISTS(SELECT 1 FROM service_background_audit a WHERE a.job=j.id AND a.manual=0 AND j.state='pending') AND NOT EXISTS(SELECT 1 FROM service_job_recovery r WHERE r.job=j.id AND r.waiting=1 AND j.kind='model_setup')) AND (b.source_pending=1 OR EXISTS(SELECT 1 FROM service_setup_pending s WHERE s.project=b.project) OR NOT EXISTS(SELECT 1 FROM service_watch w JOIN projects p ON p.binding=w.project WHERE w.project=b.project AND {source_ready})) ORDER BY b.project LIMIT ?2"))?;
        let rows = statement.query_map(
            params![
                after.map(|id| id.to_string()).unwrap_or_default(),
                limit as i64,
                i64::try_from(now_ms)?
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (project, revision, source, setup) = row?;
            Ok((
                project.parse()?,
                u64::try_from(revision).context("invalid background continuation revision")?,
                if setup {
                    JobKind::ModelSetup
                } else if source {
                    JobKind::Index
                } else {
                    JobKind::Vectors
                },
            ))
        })
        .collect()
    }
    pub fn has_background_work(&self) -> Result<bool> {
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM service_background b JOIN projects p ON p.binding=b.project WHERE p.paused=0 AND p.registered=1 AND (b.source_pending=1 OR b.semantic_pending=1 OR EXISTS(SELECT 1 FROM service_setup_pending s WHERE s.project=b.project)))",[],|row|row.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::{Incarnation, Sha256Digest},
        service::{JobOutcome, JobState, ProjectRegistration, RequestKey},
    };
    fn nonce(value: u16) -> Incarnation {
        let mut bytes = [0u8; 16];
        bytes[..2].copy_from_slice(&value.to_le_bytes());
        Incarnation::from_bytes(bytes)
    }
    fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, ProjectLease)> {
        let root = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(root.path(), nonce(1))?;
        let project = ProjectRegistration::new(
            "/background-source".into(),
            "/background-data".into(),
            "background".into(),
        )?;
        registry.register(&project)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        Ok((root, registry, lease))
    }
    #[test]
    fn exact_project_intent_observation_keeps_paused_and_delayed_phases_visible() -> Result<()> {
        let (root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        let other = ProjectRegistration::new(
            "/other-source".into(),
            "/other-state".into(),
            "other".into(),
        )?;
        registry.register(&other)?;
        assert!(!registry.project_background_pending(project)?);
        for kind in [JobKind::ModelSetup, JobKind::Index, JobKind::Vectors] {
            registry.set_background_pending(&lease, kind, true)?;
            registry.pause(project, true)?;
            assert!(registry.due_background_jobs(None, 0, 32)?.is_empty());
            assert!(registry.project_background_pending(project)?);
            assert!(!registry.project_background_pending(other.binding)?);
            let reader = ServiceRegistry::open_saved(root.path())?;
            assert!(reader.project_background_pending(project)?);
            assert!(!reader.project_background_pending(other.binding)?);
            drop(reader);
            registry.set_background_pending(&lease, kind, false)?;
            assert!(!registry.project_background_pending(project)?);
            registry.pause(project, false)?;
        }
        let (namespace, epoch) = registry.request_epoch()?;
        let caller = nonce(9);
        let held = registry.enqueue(
            project,
            caller,
            &RequestKey {
                namespace,
                epoch,
                nonce: caller,
            },
            JobKind::Inspect,
        )?;
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        assert!(
            registry.due_background_jobs(None, 0, 32)?.is_empty(),
            "accepted inspection delays background admission"
        );
        assert!(registry.project_background_pending(project)?);
        assert_eq!(
            registry.job(held.id)?.state,
            airk_core::service::JobState::Pending
        );
        registry.set_background_pending(&lease, JobKind::Vectors, false)?;
        assert!(!registry.project_background_pending(project)?);
        assert!(!registry.project_background_pending(other.binding)?);
        Ok(())
    }
    #[test]
    fn reviewed_registration_persists_setup_and_source_fallback_without_duplicate_work()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(root.path(), nonce(1))?;
        let registration =
            ProjectRegistration::new("/setup-source".into(), "/setup-data".into(), "setup".into())?;
        let project = registration.binding;
        registry.register_with_setup(&registration, Some(false))?;
        assert!(!registry.watch(project)?.observation.enabled);
        assert!(registry.project_setup_pending(project)?);
        registry.register_with_setup(&registration, Some(true))?;
        assert!(
            !registry.watch(project)?.observation.enabled,
            "repeat registration preserves settings"
        );
        registry.pause(project, true)?;
        assert!(registry.due_background_jobs(None, 0, 32)?.is_empty());
        registry.pause(project, false)?;
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(2))?;
        let due = registry.due_background_jobs(None, 0, 32)?;
        assert_eq!(due.len(), 1);
        let (bound, revision, kind) = due[0];
        assert_eq!((bound, kind), (project, JobKind::ModelSetup));
        let setup =
            registry.enqueue_background_job(project, revision, kind, nonce(3), nonce(4), 0)?;
        assert_eq!(
            registry
                .enqueue_background_job(project, revision, kind, nonce(3), nonce(4), 0)?
                .id,
            setup.id
        );
        assert!(registry.due_background_jobs(None, 0, 32)?.is_empty());
        let mut lease = ProjectLease::acquire(root.path(), project)?;
        registry.admit_worker(&mut lease)?;
        registry
            .claim_job_id(&lease, setup.id)?
            .context("setup claim")?;
        registry.complete_job(
            &lease,
            setup.id,
            JobOutcome::Failed {
                code: "model_unavailable".into(),
            },
        )?;
        let due = registry.due_background_jobs(None, 0, 32)?;
        assert_eq!(due.len(), 1);
        assert_eq!(
            due[0].2,
            JobKind::Index,
            "text indexing survives model failure"
        );
        assert!(!registry.project_setup_pending(project)?);
        registry.register_with_setup(&registration, Some(false))?;
        assert!(
            !registry.project_setup_pending(project)?,
            "reopening cannot restart completed setup"
        );
        Ok(())
    }
    #[test]
    fn continuation_survives_restart_and_acceptance_debits_it_atomically() -> Result<()> {
        let (root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        let source = registry.enqueue_internal(project, nonce(2), JobKind::Index, nonce(3))?;
        registry
            .claim_job_id(&lease, source.id)?
            .context("source claim missing")?;
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        assert!(registry.due_background_jobs(None, 0, 32)?.is_empty());
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(4))?;
        let [(bound, revision, kind)] = registry
            .due_background_jobs(None, 0, 32)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("missing saved continuation"))?;
        assert_eq!((bound, kind), (project, JobKind::Vectors));
        let job =
            registry.enqueue_background_job(project, revision, kind, nonce(2), nonce(5), 0)?;
        assert_eq!(job.state, JobState::Pending);
        assert!(!registry.has_background_work()?);
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        let replay =
            registry.enqueue_background_job(project, revision, kind, nonce(2), nonce(5), 0)?;
        assert_eq!(replay.id, job.id);
        assert!(registry.has_background_work()?);
        Ok(())
    }
    #[test]
    fn stale_pages_pause_and_other_accepted_work_preserve_newer_intents() -> Result<()> {
        let (_root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        registry.set_background_pending(&lease, JobKind::Index, true)?;
        let stale = registry.due_background_jobs(None, 0, 1)?[0];
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        assert!(
            registry
                .enqueue_background_job(project, stale.1, stale.2, nonce(2), nonce(3), 0)
                .is_err()
        );
        assert!(registry.pending_projects(32)?.is_empty());
        registry.pause(project, true)?;
        assert!(registry.due_background_jobs(None, 0, 32)?.is_empty());
        assert!(!registry.has_background_work()?);
        registry.pause(project, false)?;
        let due = registry.due_background_jobs(None, 0, 1)?[0];
        assert_eq!(due.2, JobKind::Index);
        let foreign = registry.enqueue_internal(project, nonce(2), JobKind::Inspect, nonce(4))?;
        assert!(
            registry
                .enqueue_background_job(project, due.1, due.2, nonce(2), nonce(5), 0)
                .is_err()
        );
        registry.claim_job_id(&lease, foreign.id)?;
        registry.complete_job(&lease, foreign.id, JobOutcome::Succeeded { revision: None })?;
        let source =
            registry.enqueue_background_job(project, due.1, due.2, nonce(2), nonce(6), 0)?;
        registry.claim_job_id(&lease, source.id)?;
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        let due = registry.due_background_jobs(None, 0, 1)?[0];
        assert_eq!(due.2, JobKind::Vectors);
        registry.set_background_pending(&lease, JobKind::Vectors, false)?;
        assert!(!registry.has_background_work()?);
        Ok(())
    }
    #[test]
    fn project_lease_fencing_and_page_bounds_apply_before_mutation() -> Result<()> {
        let (_root, mut registry, lease) = fixture()?;
        assert!(
            registry
                .set_background_pending(&lease, JobKind::Verify, true)
                .is_err()
        );
        assert!(!registry.has_background_work()?);
        assert!(registry.due_background_jobs(None, 0, 33).is_err());
        let (_other, _, foreign) = fixture()?;
        assert!(
            registry
                .set_background_pending(&foreign, JobKind::Vectors, true)
                .is_err()
        );
        registry.connection.execute(
            "UPDATE projects SET epoch=epoch+1 WHERE binding=?1",
            [lease.identity().0.to_string()],
        )?;
        assert!(
            registry
                .set_background_pending(&lease, JobKind::Index, true)
                .is_err()
        );
        assert!(!registry.has_background_work()?);
        Ok(())
    }
    #[test]
    fn late_cancellation_keeps_committed_source_success_but_stops_its_continuation() -> Result<()> {
        let (_root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        let source = registry.enqueue_internal(project, nonce(2), JobKind::Index, nonce(3))?;
        registry.claim_job_id(&lease, source.id)?;
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        registry.set_background_pending(&lease, JobKind::Index, true)?;
        registry.request_cancellation(source.id)?;
        let receipt =
            registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        assert_eq!(receipt.state, JobState::Succeeded);
        assert_eq!(
            registry.due_background_jobs(None, 0, 1)?[0].2,
            JobKind::Index,
            "later source work was discarded"
        );
        registry.set_background_pending(&lease, JobKind::Index, false)?;
        assert!(
            !registry.has_background_work()?,
            "late cancellation left semantic work queued"
        );
        Ok(())
    }
    #[test]
    fn manual_source_claim_joins_prior_intent_and_preserves_later_requests() -> Result<()> {
        let (_root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        registry.set_background_pending(&lease, JobKind::Index, true)?;
        let source = registry.enqueue_internal(project, nonce(2), JobKind::Index, nonce(3))?;
        registry.claim_job_id(&lease, source.id)?;
        assert!(
            !registry.has_background_work()?,
            "manual source claim left duplicate automatic work"
        );
        registry.set_background_pending(&lease, JobKind::Index, true)?;
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        assert_eq!(
            registry.due_background_jobs(None, 0, 1)?[0].2,
            JobKind::Index
        );
        Ok(())
    }
    #[test]
    fn due_source_wins_stale_vector_preview_without_losing_saved_intent() -> Result<()> {
        let (_root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        let preview = registry.due_background_jobs(None, 100, 1)?[0];
        let before: (i64, i64) = (
            registry
                .connection
                .query_row("SELECT count(*) FROM service_jobs", [], |r| r.get(0))?,
            registry
                .connection
                .query_row("SELECT count(*) FROM service_requests", [], |r| r.get(0))?,
        );
        registry.configure_watch(project, true)?;
        assert!(registry.due_background_jobs(None, 100, 1)?.is_empty());
        assert!(
            registry
                .enqueue_background_job(project, preview.1, preview.2, nonce(2), nonce(3), 100)
                .is_err()
        );
        let after: (i64, i64) = (
            registry
                .connection
                .query_row("SELECT count(*) FROM service_jobs", [], |r| r.get(0))?,
            registry
                .connection
                .query_row("SELECT count(*) FROM service_requests", [], |r| r.get(0))?,
        );
        assert_eq!(
            before, after,
            "rejected stale preview leaked a job or receipt"
        );
        let (_, revision) = registry.due_source_projects(None, 100, 1)?[0];
        let source =
            registry.enqueue_source_reconcile(project, nonce(2), nonce(4), revision, 100)?;
        registry.claim_job_id(&lease, source.id)?;
        registry.source_changed(project, registry.watch(project)?.configuration_epoch)?;
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        assert!(
            registry.due_background_jobs(None, 100, 1)?.is_empty(),
            "later source revision lost its priority"
        );
        let (_, later) = registry.due_source_projects(None, 100, 1)?[0];
        assert!(later > revision);
        let source = registry.enqueue_source_reconcile(project, nonce(2), nonce(5), later, 100)?;
        registry.claim_job_id(&lease, source.id)?;
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        assert_eq!(
            registry.due_background_jobs(None, 100, 1)?[0],
            preview,
            "source priority consumed or replaced saved vector intent"
        );
        Ok(())
    }

    #[test]
    fn ineligible_source_does_not_suppress_vectors_and_deadlines_are_exact() -> Result<()> {
        for condition in [
            "enabled=0",
            "indexed=requested",
            "attempts=3",
            "blocked_revision=requested",
            "next_ms=101",
        ] {
            let (_root, mut registry, lease) = fixture()?;
            let project = lease.identity().0;
            registry.configure_watch(project, true)?;
            registry.set_background_pending(&lease, JobKind::Vectors, true)?;
            registry.connection.execute(
                &format!("UPDATE service_watch SET {condition} WHERE project=?1"),
                [project.to_string()],
            )?;
            assert!(
                registry.due_source_projects(None, 100, 1)?.is_empty(),
                "{condition}"
            );
            assert_eq!(
                registry.due_background_jobs(None, 100, 1)?[0].2,
                JobKind::Vectors,
                "{condition}"
            );
            if condition == "next_ms=101" {
                assert_eq!(registry.due_source_projects(None, 101, 1)?.len(), 1);
                assert!(registry.due_background_jobs(None, 101, 1)?.is_empty());
            }
            let vector =
                registry.enqueue_internal(project, nonce(2), JobKind::Vectors, nonce(3))?;
            assert!(
                registry
                    .claim_selected_job_at(&lease, Some(vector.id), 100)?
                    .is_some()
            );
            assert!(
                !registry.source_yield_requested(project, vector.id, 100)?,
                "{condition}"
            );
        }
        for (failures, next_ms) in [(3, 0), (1, 101)] {
            let (_root, mut registry, lease) = fixture()?;
            let project = lease.identity().0;
            registry.configure_watch(project, true)?;
            registry.set_background_pending(&lease, JobKind::Vectors, true)?;
            registry.connection.execute(
                "INSERT INTO service_recovery_budget VALUES(?1,'test',?2,?3,0,0)",
                params![format!("source:{project}"), failures, next_ms],
            )?;
            assert!(registry.due_source_projects(None, 100, 1)?.is_empty());
            assert_eq!(
                registry.due_background_jobs(None, 100, 1)?[0].2,
                JobKind::Vectors
            );
            if failures < 3 {
                assert!(registry.due_background_jobs(None, 101, 1)?.is_empty());
            }
            let vector =
                registry.enqueue_internal(project, nonce(2), JobKind::Vectors, nonce(3))?;
            assert!(
                registry
                    .claim_selected_job_at(&lease, Some(vector.id), 100)?
                    .is_some()
            );
            assert!(!registry.source_yield_requested(project, vector.id, 100)?);
        }
        Ok(())
    }

    #[test]
    fn source_priority_preserves_setup_source_continuations_and_accepted_replay() -> Result<()> {
        let (_root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        let preview = registry.due_background_jobs(None, 100, 1)?[0];
        let accepted = registry.enqueue_background_job(
            project,
            preview.1,
            preview.2,
            nonce(2),
            nonce(3),
            100,
        )?;
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        registry.configure_watch(project, true)?;
        assert_eq!(
            registry
                .enqueue_background_job(project, preview.1, preview.2, nonce(2), nonce(3), 100)?
                .id,
            accepted.id
        );
        assert!(
            registry.has_background_work()?,
            "replay consumed newer intent"
        );
        assert!(registry.claim_job_id(&lease, accepted.id)?.is_none());
        let (_, revision) = registry.due_source_projects(None, 100, 1)?[0];
        let source =
            registry.enqueue_source_reconcile(project, nonce(2), nonce(6), revision, 100)?;
        registry
            .claim_job_id(&lease, source.id)?
            .context("source claim missing")?;
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        registry
            .claim_job_id(&lease, accepted.id)?
            .context("retained Vector claim missing")?;
        registry.complete_job(
            &lease,
            accepted.id,
            JobOutcome::Succeeded { revision: None },
        )?;
        registry.set_background_pending(&lease, JobKind::Index, true)?;
        assert_eq!(
            registry.due_background_jobs(None, 100, 1)?[0].2,
            JobKind::Index
        );
        registry.set_background_pending(&lease, JobKind::ModelSetup, true)?;
        assert_eq!(
            registry.due_background_jobs(None, 100, 1)?[0].2,
            JobKind::ModelSetup
        );
        Ok(())
    }

    #[test]
    fn source_yield_is_running_vector_scoped_and_preserves_manual_queue_order() -> Result<()> {
        let (_root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        let vector = registry.enqueue_internal(project, nonce(2), JobKind::Vectors, nonce(3))?;
        registry.configure_watch(project, true)?;
        assert!(
            !registry.source_yield_requested(project, vector.id, 100)?,
            "pending claim transition must not produce a yield hint"
        );
        assert!(
            registry
                .source_yield_requested(project, Sha256Digest::from_bytes([255; 32]), 100)
                .is_err()
        );
        registry.configure_watch(project, false)?;
        registry.claim_job_id(&lease, vector.id)?;
        assert!(!registry.source_yield_requested(project, vector.id, 100)?);
        let other = ProjectRegistration::new(
            "/yield-other".into(),
            "/yield-other-data".into(),
            "other".into(),
        )?;
        registry.register(&other)?;
        assert!(
            registry
                .source_yield_requested(other.binding, vector.id, 100)
                .is_err()
        );
        registry.configure_watch(project, true)?;
        assert!(registry.source_yield_requested(project, vector.id, 100)?);
        registry.pause(project, true)?;
        assert!(!registry.source_yield_requested(project, vector.id, 100)?);
        registry.pause(project, false)?;
        registry.configure_watch(project, false)?;
        assert!(!registry.source_yield_requested(project, vector.id, 100)?);
        registry.set_background_pending(&lease, JobKind::Index, true)?;
        assert!(
            registry.source_yield_requested(project, vector.id, 100)?,
            "saved manual source continuation was ignored"
        );
        registry.set_background_pending(&lease, JobKind::Index, false)?;
        let inspect = registry.enqueue_internal(project, nonce(2), JobKind::Inspect, nonce(4))?;
        let manual = registry.enqueue_internal(project, nonce(2), JobKind::Index, nonce(5))?;
        assert!(
            !registry.source_yield_requested(project, vector.id, 100)?,
            "source bypassed earlier accepted work"
        );
        registry.complete_job(&lease, vector.id, JobOutcome::Succeeded { revision: None })?;
        registry.claim_job_id(&lease, inspect.id)?;
        assert!(
            !registry.source_yield_requested(project, inspect.id, 100)?,
            "nonvector work received a yield hint"
        );
        registry.complete_job(&lease, inspect.id, JobOutcome::Succeeded { revision: None })?;
        assert!(
            !registry.source_yield_requested(project, vector.id, 100)?,
            "completed grant transition must not yield to the pending manual source job"
        );
        // A pending Vector cannot start ahead of an accepted source job.
        let vector = registry.enqueue_internal(project, nonce(2), JobKind::Vectors, nonce(6))?;
        assert!(registry.claim_job_id(&lease, vector.id)?.is_none());
        assert_eq!(registry.job(vector.id)?.state, JobState::Pending);
        registry
            .claim_job_id(&lease, manual.id)?
            .context("manual source claim missing")?;
        registry.complete_job(&lease, manual.id, JobOutcome::Succeeded { revision: None })?;
        registry
            .claim_job_id(&lease, vector.id)?
            .context("new vector claim missing")?;
        // Source accepted after the actual Vector claim requests a safe yield.
        let manual = registry.enqueue_internal(project, nonce(2), JobKind::Index, nonce(7))?;
        assert!(
            registry.source_yield_requested(project, vector.id, 100)?,
            "manual source with watch disabled was ignored"
        );
        registry.pause(project, true)?;
        assert!(!registry.source_yield_requested(project, vector.id, 100)?);
        registry.pause(project, false)?;
        registry.request_cancellation(manual.id)?;
        assert!(!registry.source_yield_requested(project, vector.id, 100)?);
        Ok(())
    }

    #[test]
    fn due_source_priority_is_independent_of_source_page_and_survives_restart() -> Result<()> {
        let (root, mut registry, lease) = fixture()?;
        let first = lease.identity().0;
        registry.configure_watch(first, true)?;
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        let second = ProjectRegistration::new(
            "/second-source".into(),
            "/second-data".into(),
            "second".into(),
        )?;
        registry.register(&second)?;
        registry.configure_watch(second.binding, true)?;
        set(&registry.connection, second.binding, JobKind::Vectors, true)?;
        let first_page = registry.due_source_projects(None, 100, 1)?;
        let second_page = registry.due_source_projects(Some(first_page[0].0), 100, 1)?;
        assert_eq!(second_page.len(), 1);
        assert!(
            registry.due_background_jobs(None, 100, 32)?.is_empty(),
            "off-page due source was starved by a vector"
        );
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(8))?;
        assert!(registry.due_background_jobs(None, 100, 32)?.is_empty());
        let independent = ProjectRegistration::new(
            "/independent-source".into(),
            "/independent-data".into(),
            "independent".into(),
        )?;
        registry.register(&independent)?;
        set(
            &registry.connection,
            independent.binding,
            JobKind::Vectors,
            true,
        )?;
        assert_eq!(
            registry
                .due_background_jobs(None, 100, 32)?
                .iter()
                .map(|row| row.0)
                .collect::<Vec<_>>(),
            vec![independent.binding],
            "one project's source request blocked an independent vector"
        );
        registry.pause(first, true)?;
        assert!(
            super::super::watch::automatic_source_candidate(
                &registry.connection,
                first,
                100,
                None
            )?
            .is_none()
        );
        registry.connection.execute(
            "UPDATE projects SET registered=0 WHERE binding=?1",
            [second.binding.to_string()],
        )?;
        assert!(registry.due_source_projects(None, 100, 32)?.is_empty());
        Ok(())
    }

    #[test]
    fn accepted_source_preempts_pending_vector_without_reordering_other_jobs() -> Result<()> {
        let (_root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        let (_, revision, kind) = registry.due_background_jobs(None, 100, 1)?[0];
        let vector =
            registry.enqueue_background_job(project, revision, kind, nonce(2), nonce(3), 100)?;
        let inspect = registry.enqueue_internal(project, nonce(2), JobKind::Inspect, nonce(4))?;
        let source = registry.enqueue_internal(project, nonce(2), JobKind::Index, nonce(5))?;
        assert_eq!(registry.next_pending_job(project)?.unwrap().id, inspect.id);
        assert!(registry.claim_job_id(&lease, vector.id)?.is_none());
        assert_eq!(registry.job(vector.id)?.attempts, 0);
        assert_eq!(registry.job(vector.id)?.state, JobState::Pending);
        assert_eq!(
            registry
                .enqueue_background_job(project, revision, kind, nonce(2), nonce(3), 100)?
                .id,
            vector.id
        );
        registry
            .claim_job_id(&lease, inspect.id)?
            .context("inspection claim missing")?;
        registry.complete_job(&lease, inspect.id, JobOutcome::Succeeded { revision: None })?;
        assert_eq!(registry.next_pending_job(project)?.unwrap().id, source.id);
        assert_eq!(
            registry
                .claim_job(&lease)?
                .context("source claim missing")?
                .id,
            source.id
        );
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        assert_eq!(registry.next_pending_job(project)?.unwrap().id, vector.id);
        assert!(registry.claim_job_id(&lease, vector.id)?.is_some());
        Ok(())
    }

    #[test]
    fn due_watch_admits_source_ahead_of_saved_pending_vector_across_restart() -> Result<()> {
        let (root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        let vector = registry.enqueue_internal(project, nonce(2), JobKind::Vectors, nonce(3))?;
        registry.configure_watch(project, true)?;
        assert!(registry.claim_job_id(&lease, vector.id)?.is_none());
        let (_, revision) = registry.due_source_projects(None, 100, 1)?[0];
        let source =
            registry.enqueue_source_reconcile(project, nonce(2), nonce(4), revision, 100)?;
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(1))?;
        assert_eq!(registry.next_pending_job(project)?.unwrap().id, source.id);
        assert!(registry.claim_job_id(&lease, vector.id)?.is_none());
        registry
            .claim_job_id(&lease, source.id)?
            .context("source claim missing")?;
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        assert_eq!(registry.job(vector.id)?.attempts, 0);
        assert!(registry.claim_job_id(&lease, vector.id)?.is_some());
        Ok(())
    }

    #[test]
    fn saved_source_continuation_preempts_pending_vector_with_watching_disabled() -> Result<()> {
        let (root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        let vector = registry.enqueue_internal(project, nonce(2), JobKind::Vectors, nonce(3))?;
        registry.set_background_pending(&lease, JobKind::Index, true)?;
        assert!(!registry.watch(project)?.observation.enabled);
        assert!(
            registry.next_pending_job(project)?.is_none(),
            "saved source still needs admission"
        );
        assert!(registry.claim_job_id(&lease, vector.id)?.is_none());
        assert!(registry.claim_job(&lease)?.is_none());
        let (_, revision, kind) = registry.due_background_jobs(None, 100, 1)?[0];
        assert_eq!(kind, JobKind::Index);
        let source =
            registry.enqueue_background_job(project, revision, kind, nonce(2), nonce(4), 100)?;
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(1))?;
        assert_eq!(registry.next_pending_job(project)?.unwrap().id, source.id);
        assert!(registry.claim_job_id(&lease, vector.id)?.is_none());
        registry
            .claim_job_id(&lease, source.id)?
            .context("saved source claim missing")?;
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        assert_eq!(registry.job(vector.id)?.attempts, 0);
        assert!(registry.claim_job_id(&lease, vector.id)?.is_some());
        Ok(())
    }

    #[test]
    fn canceled_source_does_not_block_exact_vector_claim() -> Result<()> {
        let (_root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        let vector = registry.enqueue_internal(project, nonce(2), JobKind::Vectors, nonce(3))?;
        let source = registry.enqueue_internal(project, nonce(2), JobKind::Index, nonce(4))?;
        registry.request_cancellation(source.id)?;
        assert_eq!(registry.next_pending_job(project)?.unwrap().id, vector.id);
        assert!(registry.claim_job_id(&lease, vector.id)?.is_some());
        Ok(())
    }

    #[test]
    fn full_public_queue_cannot_drop_a_saved_continuation() -> Result<()> {
        let (root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        let filler = ProjectRegistration::new(
            "/background-filler".into(),
            "/background-filler-data".into(),
            "filler".into(),
        )?;
        registry.register(&filler)?;
        let transaction = registry.connection.transaction()?;
        let first = Sha256Digest::from_bytes([0; 32]);
        for n in 0u32..1024 {
            let mut bytes = [0u8; 32];
            bytes[..4].copy_from_slice(&n.to_le_bytes());
            transaction.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,'verify','pending',0,0)",params![Sha256Digest::from_bytes(bytes).to_string(),filler.binding.to_string()])?;
        }
        transaction.commit()?;
        let due = registry.due_background_jobs(None, 0, 1)?[0];
        assert!(
            registry
                .enqueue_background_job(project, due.1, due.2, nonce(2), nonce(3), 0)
                .is_err()
        );
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(4))?;
        assert!(registry.has_background_work()?);
        registry.connection.execute(
            "UPDATE service_jobs SET state='succeeded',outcome=?1 WHERE id=?2",
            params![
                serde_json::to_vec(&JobOutcome::Succeeded { revision: None })?,
                first.to_string()
            ],
        )?;
        // Retirement can rotate the public request epoch. The unaccepted intent
        // remains available for a fresh internal key after that rotation.
        if registry
            .enqueue_background_job(project, due.1, due.2, nonce(2), nonce(5), 0)
            .is_err()
        {
            assert!(registry.has_background_work()?);
            registry.enqueue_background_job(project, due.1, due.2, nonce(2), nonce(6), 0)?;
        }
        assert!(!registry.has_background_work()?);
        Ok(())
    }
}
