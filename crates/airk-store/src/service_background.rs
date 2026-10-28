// SPDX-License-Identifier: Apache-2.0
//! Durable phase continuations independent of the finite public job queue.
use super::ServiceRegistry;
use crate::service::ProjectLease;
use airk_core::service::{JobKind, ProjectBinding};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, TransactionBehavior, params};

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
    let eligible:bool=connection.query_row(&format!("SELECT EXISTS(SELECT 1 FROM service_background b JOIN projects p ON p.binding=b.project WHERE b.project=?1 AND b.revision=?2 AND {predicate} AND p.paused=0 AND p.registered=1 AND NOT EXISTS(SELECT 1 FROM service_jobs j WHERE j.project=b.project AND j.id<>?3 AND j.state IN('pending','running') AND NOT EXISTS(SELECT 1 FROM service_job_recovery r WHERE r.job=j.id AND r.waiting=1 AND j.kind='model_setup')))"),params![project.to_string(),expected,job.to_string()],|row|row.get(0))?;
    ensure!(eligible, "background continuation is no longer eligible");
    set(connection, project, kind, false)?;
    Ok(())
}

impl ServiceRegistry {
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
        limit: usize,
    ) -> Result<Vec<(ProjectBinding, u64, JobKind)>> {
        ensure!(
            (1..=32).contains(&limit),
            "invalid background continuation page bound"
        );
        let mut statement=self.connection.prepare("SELECT b.project,b.revision,b.source_pending,EXISTS(SELECT 1 FROM service_setup_pending s WHERE s.project=b.project) FROM service_background b JOIN projects p ON p.binding=b.project WHERE b.project>?1 AND p.paused=0 AND p.registered=1 AND (b.source_pending=1 OR b.semantic_pending=1 OR EXISTS(SELECT 1 FROM service_setup_pending s WHERE s.project=b.project)) AND NOT EXISTS(SELECT 1 FROM service_jobs j WHERE j.project=b.project AND j.state IN('pending','running') AND NOT EXISTS(SELECT 1 FROM service_job_recovery r WHERE r.job=j.id AND r.waiting=1 AND j.kind='model_setup')) ORDER BY b.project LIMIT ?2")?;
        let rows = statement.query_map(
            params![
                after.map(|id| id.to_string()).unwrap_or_default(),
                limit as i64
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
        service::{JobOutcome, JobState, ProjectRegistration},
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
        assert!(registry.due_background_jobs(None, 32)?.is_empty());
        registry.pause(project, false)?;
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(2))?;
        let due = registry.due_background_jobs(None, 32)?;
        assert_eq!(due.len(), 1);
        let (bound, revision, kind) = due[0];
        assert_eq!((bound, kind), (project, JobKind::ModelSetup));
        let setup = registry.enqueue_background_job(project, revision, kind, nonce(3), nonce(4))?;
        assert_eq!(
            registry
                .enqueue_background_job(project, revision, kind, nonce(3), nonce(4))?
                .id,
            setup.id
        );
        assert!(registry.due_background_jobs(None, 32)?.is_empty());
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
        let due = registry.due_background_jobs(None, 32)?;
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
        assert!(registry.due_background_jobs(None, 32)?.is_empty());
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        drop(registry);
        let mut registry = ServiceRegistry::open(root.path(), nonce(4))?;
        let [(bound, revision, kind)] = registry
            .due_background_jobs(None, 32)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("missing saved continuation"))?;
        assert_eq!((bound, kind), (project, JobKind::Vectors));
        let job = registry.enqueue_background_job(project, revision, kind, nonce(2), nonce(5))?;
        assert_eq!(job.state, JobState::Pending);
        assert!(!registry.has_background_work()?);
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        let replay =
            registry.enqueue_background_job(project, revision, kind, nonce(2), nonce(5))?;
        assert_eq!(replay.id, job.id);
        assert!(registry.has_background_work()?);
        Ok(())
    }
    #[test]
    fn stale_pages_pause_and_other_accepted_work_preserve_newer_intents() -> Result<()> {
        let (_root, mut registry, lease) = fixture()?;
        let project = lease.identity().0;
        registry.set_background_pending(&lease, JobKind::Index, true)?;
        let stale = registry.due_background_jobs(None, 1)?[0];
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        assert!(
            registry
                .enqueue_background_job(project, stale.1, stale.2, nonce(2), nonce(3))
                .is_err()
        );
        assert!(registry.pending_projects(32)?.is_empty());
        registry.pause(project, true)?;
        assert!(registry.due_background_jobs(None, 32)?.is_empty());
        assert!(!registry.has_background_work()?);
        registry.pause(project, false)?;
        let due = registry.due_background_jobs(None, 1)?[0];
        assert_eq!(due.2, JobKind::Index);
        let foreign = registry.enqueue_internal(project, nonce(2), JobKind::Verify, nonce(4))?;
        assert!(
            registry
                .enqueue_background_job(project, due.1, due.2, nonce(2), nonce(5))
                .is_err()
        );
        registry.claim_job_id(&lease, foreign.id)?;
        registry.complete_job(&lease, foreign.id, JobOutcome::Succeeded { revision: None })?;
        let source = registry.enqueue_background_job(project, due.1, due.2, nonce(2), nonce(6))?;
        registry.claim_job_id(&lease, source.id)?;
        registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
        let due = registry.due_background_jobs(None, 1)?[0];
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
        assert!(registry.due_background_jobs(None, 33).is_err());
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
            registry.due_background_jobs(None, 1)?[0].2,
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
        assert_eq!(registry.due_background_jobs(None, 1)?[0].2, JobKind::Index);
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
        let due = registry.due_background_jobs(None, 1)?[0];
        assert!(
            registry
                .enqueue_background_job(project, due.1, due.2, nonce(2), nonce(3))
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
            .enqueue_background_job(project, due.1, due.2, nonce(2), nonce(5))
            .is_err()
        {
            assert!(registry.has_background_work()?);
            registry.enqueue_background_job(project, due.1, due.2, nonce(2), nonce(6))?;
        }
        assert!(!registry.has_background_work()?);
        Ok(())
    }
}
