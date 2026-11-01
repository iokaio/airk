// SPDX-License-Identifier: Apache-2.0
//! Low-priority same-job physical audit continuations. Metadata is not authority.
use super::{MAX_STATE, jobs};
use crate::{
    current::{CurrentRead, CurrentToken, audit_witness::AuditInputWitness},
    service::{
        ProjectLease,
        registry::{AuditDrainProof, ServiceRegistry},
    },
};
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    service::{JobKind, JobState, ProjectBinding},
};
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
const TURN_DELAY_MS: u64 = 1000;
const AUDIT_PERIOD_MS: u64 = 86_400_000;
type SavedAuditInputs = (Option<Vec<u8>>, Option<Vec<u8>>);

fn next_audit_due(
    observation: &crate::service::collection::integrity::verify::RetainedAuditObservation,
) -> u64 {
    let mut oldest = observation
        .lexical_unix_ms
        .min(observation.source_first_unix_ms);
    for observed in [
        observation.scalars.first_observed_unix_ms,
        observation.native_facts.first_observed_unix_ms,
        observation.native_edges.first_observed_unix_ms,
        observation.read_ranges.first_observed_unix_ms,
        observation.imported.first_observed_unix_ms,
        observation.analysis_first_unix_ms,
        observation.recent_unix_ms,
    ]
    .into_iter()
    .flatten()
    {
        oldest = oldest.min(observed);
    }
    oldest.saturating_add(AUDIT_PERIOD_MS)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u32,
    head: CurrentToken,
    allowance: u64,
}
/// Minted only for the actual fenced running cohort; serialized rows are inert.
#[derive(Debug)]
pub struct AutomaticAuditContext {
    job: Sha256Digest,
    campaign: u64,
    epoch: u64,
    manual: bool,
}
impl AutomaticAuditContext {
    pub fn job(&self) -> Sha256Digest {
        self.job
    }
    pub fn campaign(&self) -> u64 {
        self.campaign
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn manual(&self) -> bool {
        self.manual
    }
    pub fn progress_key(&self) -> Sha256Digest {
        framed_digest(
            b"automatic-audit-progress/1",
            &[self.job.as_bytes(), &self.campaign.to_le_bytes()],
        )
    }
}
fn stop_verify_cohort(
    connection: &Connection,
    record: &airk_core::service::JobRecord,
) -> Result<()> {
    connection.execute("UPDATE service_jobs SET state='canceled',outcome=?1 WHERE state='pending' AND id IN(SELECT job FROM service_background_audit WHERE project=?2 AND job<>?3 AND manual=0)", params![serde_json::to_vec(&airk_core::service::JobOutcome::Canceled)?,record.project.to_string(),record.id.to_string()])?;
    connection.execute("INSERT OR IGNORE INTO service_background_audit(project,revision,campaign,job,parked,manual,intent,coverage,blocked,next_ms) VALUES(?1,0,0,NULL,0,0,NULL,NULL,NULL,9223372036854775807)", [record.project.to_string()])?;
    // Stop never cancels or detaches another explicitly promoted manual request.
    // A pending automatic turn is already drained and may be canceled safely.
    connection.execute("UPDATE service_background_audit SET revision=revision+1,intent=NULL,blocked=NULL,next_ms=9223372036854775807,parked=CASE WHEN manual=1 AND job<>?1 THEN parked ELSE 0 END,manual=CASE WHEN (manual=1 AND job<>?1) OR (job=?1 AND ?3) THEN manual ELSE 0 END,job=CASE WHEN (manual=1 AND job<>?1) OR (job=?1 AND ?3) THEN job ELSE NULL END WHERE project=?2", params![record.id.to_string(),record.project.to_string(),record.state==JobState::Running])?;
    Ok(())
}

pub(crate) fn canceled(connection: &Connection, job: Sha256Digest) -> Result<()> {
    let record = jobs::record(connection, job)?;
    if record.kind == JobKind::Verify
        && matches!(record.state, JobState::Pending | JobState::Running)
    {
        stop_verify_cohort(connection, &record)?;
    }
    Ok(())
}

/// Only an explicit user continuation may re-enable a stopped cohort. Native
/// observations and unrelated publications cannot undo Stop.
pub(crate) fn resume(connection: &Connection, project: ProjectBinding) -> Result<()> {
    connection.execute("UPDATE service_background_audit SET revision=revision+1,next_ms=0,blocked=NULL,manual=0,job=NULL,parked=0 WHERE project=?1 AND next_ms=9223372036854775807 AND (job IS NULL OR EXISTS(SELECT 1 FROM service_jobs j WHERE j.id=job AND j.state IN('succeeded','failed','canceled')))", [project.to_string()])?;
    Ok(())
}

pub(crate) fn settle_terminal(connection: &Connection, project: ProjectBinding) -> Result<()> {
    connection.execute("UPDATE service_background_audit SET revision=revision+1,job=NULL,parked=0,manual=0 WHERE project=?1 AND EXISTS(SELECT 1 FROM service_jobs j WHERE j.id=job AND j.state='succeeded')", [project.to_string()])?;
    connection.execute("UPDATE service_background_audit SET revision=revision+1,job=NULL,parked=0,manual=0,intent=NULL,blocked=NULL,next_ms=9223372036854775807 WHERE project=?1 AND EXISTS(SELECT 1 FROM service_jobs j WHERE j.id=job AND j.state IN('failed','canceled'))", [project.to_string()])?;
    Ok(())
}

pub(crate) fn finished(
    connection: &Connection,
    job: Sha256Digest,
    succeeded: bool,
    now_ms: u64,
) -> Result<()> {
    if !succeeded {
        let record = jobs::record(connection, job)?;
        if record.kind == JobKind::Verify {
            // A manual failure/cancel without actual profile completion stops
            // the cohort too, even if it was not the accepted automatic job.
            stop_verify_cohort(connection, &record)?;
            return Ok(());
        }
    }
    let row: Option<(bool, Option<Vec<u8>>)> = connection
        .query_row(
            "SELECT intent IS NOT NULL,coverage FROM service_background_audit WHERE job=?1",
            [job.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((enabled, bytes)) = row else {
        return Ok(());
    };
    let next = if succeeded && enabled {
        bytes.map(|bytes| -> Result<u64> {
            ensure!(bytes.len() <= MAX_STATE, "audit coverage exceeds bound");
            let observed: crate::service::collection::integrity::verify::RetainedAuditObservation = serde_json::from_slice(&bytes)?;
            observed.validate()?;
            Ok(next_audit_due(&observed))
        }).transpose()?.unwrap_or(now_ms).max(now_ms.saturating_add(TURN_DELAY_MS))
    } else {
        i64::MAX as u64
    };
    connection.execute("UPDATE service_background_audit SET job=NULL,parked=0,manual=0,intent=CASE WHEN ?1 THEN intent ELSE NULL END,next_ms=?2,revision=revision+1 WHERE job=?3", params![succeeded,i64::try_from(next)?,job.to_string()])?;
    Ok(())
}

/// Invoked only inside actual job settlement after the owning registry validates
/// this exact sealed profile proof. No generic outcome or saved observation can
/// create this relation between manual work and the maintenance cohort.
pub(crate) fn finished_query(
    connection: &Connection,
    job: &airk_core::service::JobRecord,
    proof: &crate::service::collection::integrity::verify::QueryComponentAuditProof,
    now_ms: u64,
) -> Result<()> {
    let coverage = serde_json::to_vec(proof.observation())?;
    ensure!(coverage.len() <= MAX_STATE, "audit coverage exceeds bound");
    let intent = serde_json::to_vec(&Intent {
        version: 1,
        head: proof.observation().head,
        allowance: 512 << 20,
    })?;
    ensure!(intent.len() <= MAX_STATE, "audit intent exceeds bound");
    connection.execute("INSERT OR IGNORE INTO service_background_audit(project,revision,campaign,job,parked,manual,intent,coverage,blocked,next_ms) VALUES(?1,0,0,NULL,0,0,NULL,NULL,NULL,0)", [job.project.to_string()])?;
    let due = next_audit_due(proof.observation()).max(now_ms.saturating_add(TURN_DELAY_MS));
    // An older parked automatic job remains accepted. Its next turn is deferred
    // to the real physical due date rather than orphaned or silently canceled.
    // Explicit Stop wins a race after the proof; explicit Resume has reset MAX.
    connection.execute("UPDATE service_background_audit SET revision=revision+1,coverage=?1,blocked=NULL,intent=CASE WHEN next_ms<9223372036854775807 THEN ?2 ELSE intent END,next_ms=CASE WHEN next_ms<9223372036854775807 THEN ?3 ELSE next_ms END,parked=CASE WHEN job=?4 THEN 0 ELSE parked END,manual=CASE WHEN job=?4 THEN 0 ELSE manual END,job=CASE WHEN job=?4 THEN NULL ELSE job END WHERE project=?5", params![coverage,intent,i64::try_from(due)?,job.id.to_string(),job.project.to_string()])?;
    Ok(())
}
pub(crate) fn promote(connection: &Connection, job: Sha256Digest) -> Result<()> {
    connection.execute("UPDATE service_background_audit SET manual=1,blocked=NULL,next_ms=0,revision=revision+1 WHERE job=?1 AND manual=0",[job.to_string()])?;
    Ok(())
}
pub(crate) fn recovered(
    connection: &Connection,
    project: ProjectBinding,
    epoch: u64,
) -> Result<()> {
    // Only genuinely interrupted execution gets a new progress namespace.
    connection.execute("UPDATE service_background_audit SET campaign=campaign+1,revision=revision+1,parked=0 WHERE project=?1 AND campaign<9223372036854775807 AND job IN(SELECT id FROM service_jobs WHERE state='running' AND epoch<>?2)",params![project.to_string(),i64::try_from(epoch)?])?;
    Ok(())
}
impl ServiceRegistry {
    /// Native query authority requires an enabled, runnable maintenance owner.
    /// Stored intent is inert metadata and must be bounded and well formed.
    pub(crate) fn private_query_maintenance_enabled(&self, lease: &ProjectLease) -> Result<bool> {
        jobs::worker_epoch(&self.connection, &self.root, lease)?;
        let bytes: Option<Vec<u8>> = self.connection.query_row(
            "SELECT substr(a.intent,1,4097) FROM service_background_audit a JOIN projects p ON p.binding=a.project WHERE a.project=?1 AND a.intent IS NOT NULL AND a.next_ms<9223372036854775807 AND a.blocked IS NULL AND p.registered=1 AND p.paused=0",
            [lease.identity().0.to_string()], |row| row.get(0),
        ).optional()?;
        let Some(bytes) = bytes else {
            return Ok(false);
        };
        ensure!(bytes.len() <= MAX_STATE, "audit intent exceeds bound");
        let intent: Intent = serde_json::from_slice(&bytes)?;
        ensure!(
            intent.version == 1 && (1..=(512 << 20)).contains(&intent.allowance),
            "invalid automatic audit intent"
        );
        Ok(true)
    }

    /// The native owner supplies an actually opened retained index. Registration
    /// alone cannot admit Verify or create a failing initial-index audit.
    pub fn observe_retained_audit_index(
        &mut self,
        lease: &ProjectLease,
        read: &CurrentRead,
        now_ms: u64,
        allowance: u64,
    ) -> Result<()> {
        self.observe_retained_audit_index_inner(lease, read, now_ms, Some(allowance))
    }

    /// Startup has no accepted work grant. It may observe changed inputs, but
    /// cannot guess a larger allowance and repeatedly unblock fixed work.
    pub fn observe_retained_audit_index_before_grant(
        &mut self,
        lease: &ProjectLease,
        read: &CurrentRead,
        now_ms: u64,
    ) -> Result<()> {
        self.observe_retained_audit_index_inner(lease, read, now_ms, None)
    }

    fn observe_retained_audit_index_inner(
        &mut self,
        lease: &ProjectLease,
        read: &CurrentRead,
        now_ms: u64,
        allowance: Option<u64>,
    ) -> Result<()> {
        let head = read.token()?;
        let registration = self.get(lease.identity().0)?.registration;
        let source = registration.root;
        ensure!(
            registration.collection == read.head().scope.collection,
            "automatic audit index belongs to another collection"
        );
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        jobs::worker_epoch(&tx, &self.root, lease)?;
        ensure!(
            source == read.head().scope.root,
            "automatic audit index belongs to another source"
        );
        let saved: Option<SavedAuditInputs> = tx
            .query_row(
                "SELECT blocked,intent FROM service_background_audit WHERE project=?1",
                [lease.identity().0.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (blocked, saved_intent) = saved.unwrap_or_default();
        let previous_intent = saved_intent
            .as_ref()
            .map(|bytes| -> Result<Intent> {
                ensure!(bytes.len() <= MAX_STATE, "audit intent exceeds bound");
                let intent: Intent = serde_json::from_slice(bytes)?;
                ensure!(
                    intent.version == 1 && (1..=(512 << 20)).contains(&intent.allowance),
                    "invalid automatic audit intent"
                );
                Ok(intent)
            })
            .transpose()?;
        let head_changed = previous_intent.is_none_or(|intent| intent.head != head);
        let witness = blocked
            .map(|bytes| -> Result<AuditInputWitness> {
                ensure!(bytes.len() <= MAX_STATE, "audit witness exceeds bound");
                let witness: AuditInputWitness = serde_json::from_slice(&bytes)?;
                witness.validate()?;
                Ok(witness)
            })
            .transpose()?;
        let allowance = allowance.unwrap_or_else(|| {
            witness
                .as_ref()
                .map_or(512 << 20, AuditInputWitness::allowance)
        });
        ensure!(
            (1..=(512 << 20)).contains(&allowance),
            "invalid automatic audit allowance"
        );
        let intent = serde_json::to_vec(&Intent {
            version: 1,
            head,
            allowance,
        })?;
        ensure!(intent.len() <= MAX_STATE, "audit intent exceeds bound");
        tx.execute("INSERT OR IGNORE INTO service_background_audit(project,revision,campaign,job,parked,manual,intent,coverage,blocked,next_ms) VALUES(?1,0,0,NULL,0,0,?2,NULL,NULL,?3)",params![lease.identity().0.to_string(),intent,i64::try_from(now_ms)?])?;
        let changed = witness
            .map(|witness| -> Result<bool> {
                read.audit_inputs_changed(
                    &witness,
                    crate::service::collection::integrity::verify::declared_audit_policy(),
                    allowance,
                )
            })
            .transpose()?
            .unwrap_or(false);
        // Updating head alone never reactivates a blocked input.
        tx.execute("UPDATE service_background_audit SET intent=?1,revision=revision+1,blocked=CASE WHEN ?2 THEN NULL ELSE blocked END,next_ms=CASE WHEN ?2 OR (?5 AND blocked IS NULL AND job IS NULL) THEN ?3 ELSE next_ms END WHERE project=?4 AND next_ms<9223372036854775807 AND (intent IS NOT ?1 OR ?2)",params![intent,changed,i64::try_from(now_ms)?,lease.identity().0.to_string(),head_changed])?;
        tx.commit()?;
        Ok(())
    }
    pub fn due_automatic_audits(
        &self,
        now_ms: u64,
        limit: usize,
    ) -> Result<Vec<(ProjectBinding, u64)>> {
        ensure!((1..=32).contains(&limit), "invalid automatic audit page");
        let mut s=self.connection.prepare("SELECT a.project,a.revision FROM service_background_audit a JOIN projects p ON p.binding=a.project WHERE a.intent IS NOT NULL AND a.blocked IS NULL AND a.job IS NULL AND a.next_ms<=?1 AND p.registered=1 AND p.paused=0 AND NOT EXISTS(SELECT 1 FROM service_jobs j WHERE j.project=a.project AND j.state IN('pending','running')) AND NOT EXISTS(SELECT 1 FROM service_background b WHERE b.project=a.project AND (b.source_pending=1 OR b.semantic_pending=1)) AND NOT EXISTS(SELECT 1 FROM service_setup_pending s WHERE s.project=a.project) ORDER BY a.next_ms,a.project LIMIT ?2")?;
        s.query_map(params![i64::try_from(now_ms)?, limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .map(|r| {
            let (p, v) = r?;
            Ok((p.parse()?, u64::try_from(v)?))
        })
        .collect()
    }
    pub fn automatic_audit_context(
        &self,
        lease: &ProjectLease,
        job: Sha256Digest,
    ) -> Result<Option<AutomaticAuditContext>> {
        let epoch = jobs::worker_epoch(&self.connection, &self.root, lease)?;
        let record = jobs::record(&self.connection, job)?;
        ensure!(
            record.project == lease.identity().0
                && record.state == JobState::Running
                && record.fencing_generation == epoch,
            "stale automatic audit worker"
        );
        self.connection
            .query_row(
                "SELECT campaign,manual FROM service_background_audit WHERE project=?1 AND job=?2",
                params![record.project.to_string(), job.to_string()],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?)),
            )
            .optional()?
            .map(|(campaign, manual)| {
                Ok(AutomaticAuditContext {
                    job,
                    campaign: u64::try_from(campaign)?,
                    epoch,
                    manual,
                })
            })
            .transpose()
    }
    pub fn complete_automatic_audit(
        &mut self,
        lease: &ProjectLease,
        proof: &crate::service::collection::integrity::verify::QueryComponentAuditProof,
        now_ms: u64,
    ) -> Result<()> {
        let bytes = serde_json::to_vec(proof.observation())?;
        ensure!(bytes.len() <= MAX_STATE, "audit coverage exceeds bound");
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = jobs::worker_epoch(&tx, &self.root, lease)?;
        let record = jobs::record(&tx, proof.job())?;
        ensure!(
            record.project == lease.identity().0
                && record.kind == JobKind::Verify
                && record.state == JobState::Running
                && record.fencing_generation == epoch
                && proof.matches(&tx, &record)?,
            "foreign automatic audit completion"
        );
        let revision = proof.revision();
        super::super::super::integrity_guard::require_no_owner_damage(
            &tx,
            record.project,
            revision.incarnation,
        )?;
        ensure!(tx.execute("UPDATE service_background_audit SET job=NULL,parked=0,coverage=?1,blocked=NULL,next_ms=?2,revision=revision+1 WHERE project=?3 AND job=?4 AND manual=0 AND intent IS NOT NULL AND NOT EXISTS(SELECT 1 FROM service_job_cancellation WHERE job=?4)",params![bytes,i64::try_from(next_audit_due(proof.observation()).max(now_ms.saturating_add(TURN_DELAY_MS)))?,record.project.to_string(),record.id.to_string()])?==1,"automatic audit was stopped, promoted or changed");
        tx.execute(
            "DELETE FROM service_recovery_budget WHERE scope=?1 AND engine=?2",
            params![
                format!("integrity:{}", record.project),
                record.id.to_string()
            ],
        )?;
        let outcome = airk_core::service::JobOutcome::Succeeded {
            revision: Some(revision),
        };
        tx.execute(
            "UPDATE service_jobs SET state='succeeded',outcome=?1 WHERE id=?2",
            params![serde_json::to_vec(&outcome)?, record.id.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn park_automatic_audit(
        &mut self,
        lease: &ProjectLease,
        job: Sha256Digest,
        drained: &AuditDrainProof,
        blocked: Option<&AuditInputWitness>,
        now_ms: u64,
    ) -> Result<()> {
        let blocked = blocked
            .map(|w| {
                w.validate()?;
                let bytes = serde_json::to_vec(w)?;
                ensure!(bytes.len() <= MAX_STATE, "audit witness exceeds bound");
                Ok::<_, anyhow::Error>(bytes)
            })
            .transpose()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = jobs::worker_epoch(&tx, &self.root, lease)?;
        let record = jobs::record(&tx, job)?;
        ensure!(
            record.project == lease.identity().0
                && record.kind == JobKind::Verify
                && record.state == JobState::Running
                && record.fencing_generation == epoch
                && drained.matches(&tx, &record)?,
            "foreign audit handoff"
        );
        // Drain removes only a fresh live veto; inherited uncertainty survives.
        super::super::super::integrity_guard::completed(
            &tx,
            &record,
            &airk_core::service::JobOutcome::Canceled,
            Some(crate::service::registry::AuditEvidence::Drained(drained)),
        )?;
        ensure!(tx.execute("UPDATE service_background_audit SET parked=1,blocked=?1,next_ms=?2,revision=revision+1 WHERE project=?3 AND job=?4 AND manual=0 AND intent IS NOT NULL AND NOT EXISTS(SELECT 1 FROM service_job_cancellation WHERE job=?4)",params![blocked,i64::try_from(now_ms.saturating_add(TURN_DELAY_MS))?,record.project.to_string(),job.to_string()])?==1,"automatic audit was stopped, promoted or changed");
        ensure!(
            tx.execute(
                "UPDATE service_jobs SET state='pending' WHERE id=?1 AND state='running'",
                [job.to_string()]
            )? == 1,
            "audit handoff changed"
        );
        tx.execute(
            "DELETE FROM service_progress WHERE job=?1",
            [job.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{identity::Incarnation, service::ProjectRegistration};
    fn fixture() -> Result<(
        tempfile::TempDir,
        ServiceRegistry,
        ProjectLease,
        crate::service::data_scope::BoundDataRoot,
    )> {
        let temp = tempfile::tempdir()?;
        let base = temp.path().canonicalize()?;
        let control = base.join("control");
        let data = base.join("data");
        let source = base.join("source");
        for path in [&control, &data, &source] {
            std::fs::create_dir(path)?;
        }
        let namespace = Incarnation::from_bytes([71; 16]);
        let mut registry = ServiceRegistry::open(&control, namespace)?;
        let project = ProjectRegistration::new(
            source.to_string_lossy().into_owned(),
            data.to_string_lossy().into_owned(),
            "audit".into(),
        )?;
        registry.register(&project)?;
        let mut lease = ProjectLease::acquire(&control, project.binding)?;
        registry.admit_worker(&mut lease)?;
        let bound = crate::service::data_scope::bind(&data, &control, registry.incarnation()?)?;
        registry.connection.execute(
            "INSERT INTO service_background_audit VALUES(?1,0,0,NULL,0,0,?2,NULL,NULL,0)",
            params![
                project.binding.to_string(),
                serde_json::to_vec(&Intent {
                    version: 1,
                    head: CurrentToken {
                        incarnation: Incarnation::from_bytes([91; 16]),
                        sequence: 0,
                        digest: Sha256Digest::from_bytes([92; 32])
                    },
                    allowance: 512 << 20,
                })?
            ],
        )?;
        Ok((temp, registry, lease, bound))
    }
    fn enqueue(
        registry: &mut ServiceRegistry,
        lease: &ProjectLease,
    ) -> Result<airk_core::service::JobRecord> {
        let revision: i64 = registry.connection.query_row(
            "SELECT revision FROM service_background_audit WHERE project=?1",
            [lease.identity().0.to_string()],
            |row| row.get(0),
        )?;
        registry.enqueue_automatic_audit(
            lease.identity().0,
            u64::try_from(revision)?,
            Incarnation::from_bytes([72; 16]),
            Incarnation::from_bytes([73; 16]),
            0,
        )
    }

    fn retained_index(
        registry: &ServiceRegistry,
        lease: &ProjectLease,
    ) -> Result<(
        crate::service::collection::Collection,
        std::sync::Arc<CurrentRead>,
    )> {
        use crate::current::{CurrentHead, CurrentScope, lexical::LexicalManifest, map};
        use airk_core::graph_storage::{GraphHead, GraphScope};
        let registration = registry.get(lease.identity().0)?.registration;
        let incarnation = Incarnation::from_bytes([91; 16]);
        let collection = crate::service::collection::Collection::initialize(
            std::path::Path::new(&registration.data_root),
            &registration.root,
            &registration.collection,
            incarnation,
        )?;
        let store = collection.selected_store()?;
        let quota = crate::quota::Quota::open_service(&registration.data_root)?;
        let scope = CurrentScope {
            root: registration.root,
            collection: registration.collection,
            incarnation,
        };
        let lexical = LexicalManifest::empty()?;
        let head = CurrentHead {
            version: 2,
            scope: scope.clone(),
            sequence: 0,
            artifact_root: map::empty_root(&scope)?,
            lexical_sequence: 0,
            lexical_commit: lexical.digest()?,
            graph_head: GraphHead::empty(GraphScope {
                root: scope.root.clone(),
                collection: scope.collection.clone(),
                incarnation,
                graph_instance: incarnation,
                specification: Sha256Digest::from_bytes([92; 32]),
                record_bytes: 16,
            })?,
        };
        let lock = store.lock()?;
        let mut writer = store.current_write(&lock, &quota, &scope, 8 << 20)?;
        writer.initialize(head.clone())?;
        writer.install_initial_lexical_manifest(&head.token()?, &lexical.canonical_bytes()?)?;
        drop(writer);
        drop(lock);
        Ok((collection, store.current_read(&scope)?))
    }

    #[test]
    fn stopped_audit_stays_stopped_until_explicit_resume_then_observed_index_is_due() -> Result<()>
    {
        let (_temp, mut registry, lease, _bound) = fixture()?;
        registry
            .connection
            .execute("DELETE FROM service_background_audit", [])?;
        assert!(registry.due_automatic_audits(0, 8)?.is_empty());
        let (_collection, read) = retained_index(&registry, &lease)?;
        registry.observe_retained_audit_index(&lease, &read, 0, 512 << 20)?;
        let job = enqueue(&mut registry, &lease)?;
        registry.cancel_pending(job.id)?;
        for allowance in [512 << 20, 256 << 20] {
            registry.observe_retained_audit_index(&lease, &read, 100, allowance)?;
            assert!(registry.due_automatic_audits(100, 8)?.is_empty());
        }
        let row: (Option<String>, Option<Vec<u8>>) = registry.connection.query_row(
            "SELECT job,intent FROM service_background_audit",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(row, (None, None));
        registry.pause(lease.identity().0, true)?;
        registry.pause(lease.identity().0, false)?;
        registry.observe_retained_audit_index(&lease, &read, 0, 512 << 20)?;
        assert_eq!(registry.due_automatic_audits(0, 8)?.len(), 1);
        let next = registry.enqueue_automatic_audit(
            lease.identity().0,
            registry.due_automatic_audits(0, 8)?[0].1,
            Incarnation::from_bytes([93; 16]),
            Incarnation::from_bytes([94; 16]),
            0,
        )?;
        assert_ne!(job.id, next.id);
        Ok(())
    }

    #[test]
    fn unchanged_blocked_component_never_reclaims_but_changed_allowance_resumes_same_job()
    -> Result<()> {
        let (_temp, mut registry, lease, bound) = fixture()?;
        let (_collection, read) = retained_index(&registry, &lease)?;
        registry.observe_retained_audit_index(&lease, &read, 0, 512 << 20)?;
        let job = enqueue(&mut registry, &lease)?;
        registry.claim_job_id(&lease, job.id)?.expect("first turn");
        let witness = read.file_audit_witness(
            "missing.txt",
            crate::service::collection::integrity::verify::declared_audit_policy(),
            512 << 20,
        )?;
        let drained = registry.drained_verify_before_execution(&lease, &bound, job.id)?;
        registry.park_automatic_audit(&lease, job.id, &drained, Some(&witness), 0)?;
        registry.observe_retained_audit_index(&lease, &read, 100, 512 << 20)?;
        assert!(registry.next_pending_job(lease.identity().0)?.is_none());
        registry.observe_retained_audit_index(&lease, &read, 0, 256 << 20)?;
        assert_eq!(
            registry
                .next_pending_job(lease.identity().0)?
                .expect("changed input")
                .id,
            job.id
        );
        let claim = registry
            .claim_job_id(&lease, job.id)?
            .expect("same job resume");
        assert_eq!(claim.attempts, 1);
        Ok(())
    }

    #[test]
    fn lower_memory_turn_keeps_its_actual_blocked_allowance_without_reactivating() -> Result<()> {
        let (_temp, mut registry, lease, bound) = fixture()?;
        let (_collection, read) = retained_index(&registry, &lease)?;
        registry.observe_retained_audit_index(&lease, &read, 0, 512 << 20)?;
        let job = enqueue(&mut registry, &lease)?;
        registry
            .claim_job_id(&lease, job.id)?
            .expect("lower memory turn");
        let witness = read.file_audit_witness(
            "missing.txt",
            crate::service::collection::integrity::verify::declared_audit_policy(),
            64 << 20,
        )?;
        let drained = registry.drained_verify_before_execution(&lease, &bound, job.id)?;
        registry.park_automatic_audit(&lease, job.id, &drained, Some(&witness), 0)?;
        for now in [100, 200, 300] {
            registry.observe_retained_audit_index(&lease, &read, now, 64 << 20)?;
            registry.observe_retained_audit_index_before_grant(&lease, &read, now)?;
            assert!(registry.next_pending_job(lease.identity().0)?.is_none());
            assert!(registry.claim_job_id(&lease, job.id)?.is_none());
        }
        let source = registry.enqueue_internal(
            lease.identity().0,
            Incarnation::from_bytes([97; 16]),
            JobKind::Index,
            Incarnation::from_bytes([98; 16]),
        )?;
        registry
            .claim_job_id(&lease, source.id)?
            .expect("unrelated source work");
        registry.complete_job(
            &lease,
            source.id,
            airk_core::service::JobOutcome::Succeeded { revision: None },
        )?;
        registry.observe_retained_audit_index_before_grant(&lease, &read, 400)?;
        assert!(registry.next_pending_job(lease.identity().0)?.is_none());
        Ok(())
    }

    #[test]
    fn physical_completion_schedules_from_original_component_dates_and_manual_completion_preserves_them()
    -> Result<()> {
        let (_temp, mut registry, lease, bound) = fixture()?;
        let (collection, read) = retained_index(&registry, &lease)?;
        registry.observe_retained_audit_index(&lease, &read, 0, 512 << 20)?;
        let job = enqueue(&mut registry, &lease)?;
        registry
            .claim_job_id(&lease, job.id)?
            .expect("physical turn");
        let context = registry
            .automatic_audit_context(&lease, job.id)?
            .expect("cohort");
        let turn = {
            let owner = collection.integrity_owner(&registry, &lease, &bound)?;
            owner.anchor(&read)?;
            let store = collection.selected_store()?;
            let mut writer = store.lock()?;
            owner.verify_automatic_turn(&mut writer, &context, 32, 512 << 20, 10, || Ok(()))?
        };
        let query = turn
            .query_completed
            .as_ref()
            .expect("actual declared proof");
        registry.complete_automatic_audit(&lease, query, 100)?;
        assert!(
            registry
                .due_automatic_audits(AUDIT_PERIOD_MS + 9, 8)?
                .is_empty()
        );
        assert_eq!(
            registry
                .due_automatic_audits(AUDIT_PERIOD_MS + 10, 8)?
                .len(),
            1
        );
        let observed = registry
            .retained_audit_observation(lease.identity().0)?
            .expect("physical dates");
        assert_eq!(observed.lexical_unix_ms, 10);
        assert_eq!(observed.source_first_unix_ms, 10);

        // A promoted whole-Verify success must use those retained physical dates,
        // rather than postpone its next physical audit by a day from settlement.
        registry.connection.execute(
            "UPDATE service_background_audit SET job=?1,manual=1",
            [job.id.to_string()],
        )?;
        finished(&registry.connection, job.id, true, 100)?;
        settle_terminal(&registry.connection, lease.identity().0)?;
        let (enabled, next): (bool, i64) = registry.connection.query_row(
            "SELECT intent IS NOT NULL,next_ms FROM service_background_audit",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert!(enabled);
        assert_eq!(u64::try_from(next)?, AUDIT_PERIOD_MS + 10);
        assert_eq!(
            serde_json::to_value(
                registry
                    .retained_audit_observation(lease.identity().0)?
                    .expect("dates retained")
            )?,
            serde_json::to_value(observed)?,
        );
        Ok(())
    }

    #[test]
    fn stale_automatic_context_after_real_worker_recovery_cannot_touch_audit_receipts() -> Result<()>
    {
        let (_temp, mut registry, lease, bound) = fixture()?;
        let (collection, read) = retained_index(&registry, &lease)?;
        registry.observe_retained_audit_index(&lease, &read, 0, 512 << 20)?;
        let job = enqueue(&mut registry, &lease)?;
        registry
            .claim_job_id(&lease, job.id)?
            .expect("original claim");
        let stale = registry
            .automatic_audit_context(&lease, job.id)?
            .expect("original context");
        let project = lease.identity().0;
        drop(lease);
        let mut lease = ProjectLease::acquire(&registry.root, project)?;
        registry.admit_worker(&mut lease)?;
        registry.recover_jobs(&lease)?;
        registry
            .claim_job_id(&lease, job.id)?
            .expect("recovered claim");
        let fresh = registry
            .automatic_audit_context(&lease, job.id)?
            .expect("recovered context");
        let registration = registry.get(project)?.registration;
        let directory = std::path::Path::new(&registration.data_root)
            .join("collections-1.4")
            .join(&registration.collection);
        let receipts = || -> Result<Vec<Option<Vec<u8>>>> {
            [
                "integrity-observation.json",
                "integrity-projection.json",
                "integrity-recovery.json",
            ]
            .into_iter()
            .map(|name| match std::fs::read(directory.join(name)) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.into()),
            })
            .collect()
        };
        let owner = collection.integrity_owner(&registry, &lease, &bound)?;
        owner.anchor(&read)?;
        let store = collection.selected_store()?;
        let mut writer = store.lock()?;
        let before = receipts()?;
        let changes = registry.connection.total_changes();
        assert!(
            owner
                .verify_automatic_turn(&mut writer, &stale, 32, 512 << 20, 100, || Ok(()))
                .is_err()
        );
        assert_eq!(receipts()?, before);
        assert_eq!(registry.connection.total_changes(), changes);
        let turn =
            owner.verify_automatic_turn(&mut writer, &fresh, 32, 512 << 20, 100, || Ok(()))?;
        assert!(turn.query_completed.is_some());
        Ok(())
    }

    #[test]
    fn malformed_enabled_audit_intent_refuses_private_queries_and_fixed_blocking_uses_strict_fallback()
    -> Result<()> {
        let (_temp, mut registry, lease, bound) = fixture()?;
        registry.connection.execute(
            "UPDATE service_background_audit SET intent=?1",
            [b"{}".as_slice()],
        )?;
        assert!(registry.private_query_maintenance_enabled(&lease).is_err());
        let (_collection, read) = retained_index(&registry, &lease)?;
        assert!(
            registry
                .observe_retained_audit_index_before_grant(&lease, &read, 0)
                .is_err()
        );
        let preserved: Vec<u8> = registry.connection.query_row(
            "SELECT intent FROM service_background_audit",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(preserved, b"{}");
        registry
            .connection
            .execute("UPDATE service_background_audit SET intent=NULL", [])?;
        registry.observe_retained_audit_index_before_grant(&lease, &read, 0)?;
        assert!(registry.private_query_maintenance_enabled(&lease)?);
        let job = enqueue(&mut registry, &lease)?;
        registry
            .claim_job_id(&lease, job.id)?
            .expect("blocking turn");
        let witness = read.file_audit_witness(
            "missing.txt",
            crate::service::collection::integrity::verify::declared_audit_policy(),
            512 << 20,
        )?;
        let drained = registry.drained_verify_before_execution(&lease, &bound, job.id)?;
        registry.park_automatic_audit(&lease, job.id, &drained, Some(&witness), 0)?;
        assert!(!registry.private_query_maintenance_enabled(&lease)?);
        Ok(())
    }

    #[test]
    fn automatic_turn_yields_to_foreground_in_an_independent_project() -> Result<()> {
        let (_temp, mut registry, lease, _bound) = fixture()?;
        let job = enqueue(&mut registry, &lease)?;
        let other =
            ProjectRegistration::new("/other-source".into(), "/other-data".into(), "other".into())?;
        registry.register(&other)?;
        let foreground = registry.enqueue_internal(
            other.binding,
            Incarnation::from_bytes([95; 16]),
            JobKind::Index,
            Incarnation::from_bytes([96; 16]),
        )?;
        assert!(registry.next_pending_job(lease.identity().0)?.is_none());
        assert!(registry.claim_job_id(&lease, job.id)?.is_none());
        registry.cancel_pending(foreground.id)?;
        assert!(registry.claim_job_id(&lease, job.id)?.is_some());
        Ok(())
    }

    #[test]
    fn unrelated_waiting_helper_does_not_starve_a_runnable_audit() -> Result<()> {
        let (_temp, mut registry, lease, _bound) = fixture()?;
        let job = enqueue(&mut registry, &lease)?;
        let other = ProjectRegistration::new(
            "/waiting-source".into(),
            "/waiting-data".into(),
            "waiting".into(),
        )?;
        registry.register(&other)?;
        let mut other_lease = ProjectLease::acquire(&registry.root, other.binding)?;
        registry.admit_worker(&mut other_lease)?;
        let helper = registry.enqueue_internal(
            other.binding,
            Incarnation::from_bytes([99; 16]),
            JobKind::ModelSetup,
            Incarnation::from_bytes([100; 16]),
        )?;
        registry
            .claim_job_id(&other_lease, helper.id)?
            .expect("helper work");
        assert!(registry.defer_job_failure(
            &other_lease,
            helper.id,
            "helper_unavailable",
            &crate::service::registry::RecoveryCondition::Helper,
            0
        )?);
        assert!(registry.next_pending_job(other.binding)?.is_none());
        assert_eq!(
            registry
                .next_pending_job(lease.identity().0)?
                .expect("audit remains runnable")
                .id,
            job.id
        );
        assert!(registry.claim_job_id(&lease, job.id)?.is_some());
        Ok(())
    }
    #[test]
    fn automatic_clean_turns_resume_same_job_beyond_crash_limit_and_real_interruption_changes_campaign()
    -> Result<()> {
        let (_temp, mut registry, lease, bound) = fixture()?;
        let job = enqueue(&mut registry, &lease)?;
        let mut key = None;
        for _ in 0..6 {
            // Explicit clock advancement in this fixture avoids a wall-clock sleep.
            registry
                .connection
                .execute("UPDATE service_background_audit SET next_ms=0", [])?;
            let claimed = registry
                .claim_job_id(&lease, job.id)?
                .expect("automatic turn eligible");
            assert_eq!(claimed.attempts, 1);
            let context = registry
                .automatic_audit_context(&lease, job.id)?
                .expect("automatic context");
            assert!(key.is_none_or(|key| key == context.progress_key()));
            key = Some(context.progress_key());
            let drain = registry.drained_verify_before_execution(&lease, &bound, job.id)?;
            registry.park_automatic_audit(&lease, job.id, &drain, None, 0)?;
            assert_eq!(registry.job(job.id)?.state, JobState::Pending);
            assert_eq!(
                registry.integrity_audit_state(&lease)?,
                crate::service::registry::IntegrityAuditState::Clear
            );
        }
        registry
            .connection
            .execute("UPDATE service_background_audit SET next_ms=0", [])?;
        registry.claim_job_id(&lease, job.id)?.expect("last claim");
        let project = lease.identity().0;
        drop(lease);
        let mut next = ProjectLease::acquire(&registry.root, project)?;
        registry.admit_worker(&mut next)?;
        registry.recover_jobs(&next)?;
        let claimed = registry
            .claim_job_id(&next, job.id)?
            .expect("interrupted turn retry");
        assert_eq!(claimed.attempts, 2);
        let context = registry
            .automatic_audit_context(&next, job.id)?
            .expect("retry context");
        assert_ne!(Some(context.progress_key()), key);
        assert!(matches!(
            registry.integrity_audit_state(&next)?,
            crate::service::registry::IntegrityAuditState::Unresolved { .. }
        ));
        Ok(())
    }
    #[test]
    fn pending_automatic_audit_yields_to_foreground_and_manual_verify_promotion_preserves_whole_request()
    -> Result<()> {
        let (_temp, mut registry, lease, _bound) = fixture()?;
        let auto = enqueue(&mut registry, &lease)?;
        let source = registry.enqueue_internal(
            lease.identity().0,
            Incarnation::from_bytes([75; 16]),
            JobKind::Index,
            Incarnation::from_bytes([76; 16]),
        )?;
        assert_eq!(
            registry
                .next_pending_job(lease.identity().0)?
                .expect("source first")
                .id,
            source.id
        );
        assert!(registry.claim_job_id(&lease, auto.id)?.is_none());
        registry.cancel_pending(source.id)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let manual = registry.enqueue(
            lease.identity().0,
            namespace,
            &airk_core::service::RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes([77; 16]),
            },
            JobKind::Verify,
        )?;
        assert_eq!(manual.id, auto.id);
        registry
            .claim_job_id(&lease, auto.id)?
            .expect("promoted claim");
        assert!(
            registry
                .automatic_audit_context(&lease, auto.id)?
                .expect("promoted context")
                .manual()
        );
        Ok(())
    }
    #[test]
    fn automatic_intent_is_durable_when_queue_pressure_refuses_admission_and_pause_defers_it()
    -> Result<()> {
        let (_temp, mut registry, lease, _bound) = fixture()?;
        let project = lease.identity().0;
        registry.pause(project, true)?;
        assert!(registry.due_automatic_audits(0, 8)?.is_empty());
        registry.pause(project, false)?;
        assert_eq!(registry.due_automatic_audits(0, 8)?.len(), 1);
        registry.connection.execute_batch("CREATE TEMP TRIGGER refuse_audit_job BEFORE INSERT ON service_jobs BEGIN SELECT RAISE(ABORT,'queue pressure'); END;")?;
        assert!(enqueue(&mut registry, &lease).is_err());
        assert_eq!(registry.due_automatic_audits(0, 8)?.len(), 1);
        registry
            .connection
            .execute_batch("DROP TRIGGER refuse_audit_job;")?;
        let job = enqueue(&mut registry, &lease)?;
        registry.cancel_pending(job.id)?;
        assert!(
            registry
                .due_automatic_audits(i64::MAX as u64, 8)?
                .is_empty()
        );
        Ok(())
    }
    #[test]
    fn actual_manual_verify_proof_schedules_unassociated_cohort_and_resumes_after_stop()
    -> Result<()> {
        for stopped in [false, true] {
            let (_temp, mut registry, lease, bound) = fixture()?;
            let (collection, read) = retained_index(&registry, &lease)?;
            registry.observe_retained_audit_index(&lease, &read, 0, 512 << 20)?;
            if stopped {
                let automatic = enqueue(&mut registry, &lease)?;
                registry.cancel_pending(automatic.id)?;
                assert!(!registry.private_query_maintenance_enabled(&lease)?);
            }
            let (namespace, epoch) = registry.request_epoch()?;
            let manual = registry.enqueue(
                lease.identity().0,
                Incarnation::from_bytes([101; 16]),
                &airk_core::service::RequestKey {
                    namespace,
                    epoch,
                    nonce: Incarnation::from_bytes([102; 16]),
                },
                JobKind::Verify,
            )?;
            registry
                .claim_job_id(&lease, manual.id)?
                .expect("manual claim");
            let turn = {
                let owner = collection.integrity_owner(&registry, &lease, &bound)?;
                owner.anchor(&read)?;
                let store = collection.selected_store()?;
                let mut writer = store.lock()?;
                owner.verify_turn(&mut writer, manual.id, 32, 512 << 20, 100, || Ok(()))?
            };
            let whole = turn.completed.as_ref().expect("whole physical proof");
            let mut foreign_revision = whole.revision()?;
            foreign_revision.generation += 1;
            let control = registry.root.join("service-1.4.sqlite3");
            let before = std::fs::read(&control)?;
            let changes = registry.connection.total_changes();
            assert!(
                registry
                    .complete_job_with_query_audit_at(
                        &lease,
                        manual.id,
                        airk_core::service::JobOutcome::Succeeded {
                            revision: Some(foreign_revision)
                        },
                        200,
                        None,
                        (
                            Some(crate::service::registry::AuditEvidence::Complete(whole)),
                            turn.query_completed.as_ref()
                        )
                    )
                    .is_err()
            );
            assert_eq!(std::fs::read(&control)?, before);
            assert_eq!(registry.connection.total_changes(), changes);
            registry.complete_job_with_query_audit_at(
                &lease,
                manual.id,
                airk_core::service::JobOutcome::Succeeded {
                    revision: Some(whole.revision()?),
                },
                200,
                None,
                (
                    Some(crate::service::registry::AuditEvidence::Complete(whole)),
                    turn.query_completed.as_ref(),
                ),
            )?;
            registry.observe_retained_audit_index(&lease, &read, 300, 64 << 20)?;
            assert_eq!(registry.job(manual.id)?.state, JobState::Succeeded);
            assert!(registry.private_query_maintenance_enabled(&lease)?);
            assert!(registry.due_automatic_audits(200, 8)?.is_empty());
            assert_eq!(
                registry
                    .due_automatic_audits(AUDIT_PERIOD_MS + 100, 8)?
                    .len(),
                1
            );
            assert_eq!(
                registry
                    .retained_audit_observation(lease.identity().0)?
                    .expect("actual dated observation")
                    .source_first_unix_ms,
                100
            );
        }
        Ok(())
    }
    #[test]
    fn actual_proof_then_cancel_never_reenables_any_manual_cohort_shape() -> Result<()> {
        for shape in 0..5 {
            let (_temp, mut registry, lease, bound) = fixture()?;
            let (collection, read) = retained_index(&registry, &lease)?;
            let project = lease.identity().0;
            if shape == 0 {
                registry
                    .connection
                    .execute("DELETE FROM service_background_audit", [])?;
            } else {
                registry.observe_retained_audit_index(&lease, &read, 0, 512 << 20)?;
            }
            let accept_manual =
                |registry: &mut ServiceRegistry, nonce| -> Result<airk_core::service::JobRecord> {
                    let (namespace, epoch) = registry.request_epoch()?;
                    registry.enqueue(
                        project,
                        Incarnation::from_bytes([103; 16]),
                        &airk_core::service::RequestKey {
                            namespace,
                            epoch,
                            nonce: Incarnation::from_bytes([nonce; 16]),
                        },
                        JobKind::Verify,
                    )
                };
            let mut accepted_manual = None;
            let older = if shape >= 2 {
                let automatic = enqueue(&mut registry, &lease)?;
                if shape >= 3 {
                    registry
                        .claim_job_id(&lease, automatic.id)?
                        .expect("older automatic turn");
                    accepted_manual = Some(accept_manual(&mut registry, 104)?);
                    let drain =
                        registry.drained_verify_before_execution(&lease, &bound, automatic.id)?;
                    registry.park_automatic_audit(&lease, automatic.id, &drain, None, 0)?;
                }
                Some(automatic.id)
            } else {
                None
            };
            let manual = match accepted_manual {
                Some(job) => job,
                None => accept_manual(&mut registry, 104)?,
            };
            registry
                .claim_job_id(&lease, manual.id)?
                .expect("manual turn");
            if shape == 4 {
                assert_eq!(
                    accept_manual(&mut registry, 105)?.id,
                    older.expect("older promoted request")
                );
            }
            let turn = {
                let owner = collection.integrity_owner(&registry, &lease, &bound)?;
                owner.anchor(&read)?;
                let store = collection.selected_store()?;
                let mut writer = store.lock()?;
                owner.verify_turn(&mut writer, manual.id, 32, 512 << 20, 100, || Ok(()))?
            };
            let whole = turn
                .completed
                .as_ref()
                .expect("actual physical proof before Stop");
            registry.request_cancellation(manual.id)?;
            let settled = registry.complete_job_with_query_audit_at(
                &lease,
                manual.id,
                airk_core::service::JobOutcome::Succeeded {
                    revision: Some(whole.revision()?),
                },
                200,
                None,
                (
                    Some(crate::service::registry::AuditEvidence::Complete(whole)),
                    turn.query_completed.as_ref(),
                ),
            )?;
            assert_eq!(settled.state, JobState::Canceled);
            assert!(matches!(
                settled.outcome,
                Some(airk_core::service::JobOutcome::Canceled)
            ));
            assert!(!registry.private_query_maintenance_enabled(&lease)?);
            assert!(registry.retained_audit_observation(project)?.is_none());
            registry.observe_retained_audit_index_before_grant(&lease, &read, 300)?;
            assert!(
                registry
                    .due_automatic_audits(i64::MAX as u64, 8)?
                    .is_empty()
            );
            if shape == 4 {
                let other = older.expect("promoted request retained");
                assert_eq!(registry.job(other)?.state, JobState::Pending);
                assert_eq!(
                    registry
                        .next_pending_job(project)?
                        .expect("other manual still runnable")
                        .id,
                    other
                );
            } else {
                if shape == 3 {
                    assert_eq!(
                        registry.job(older.expect("older automatic"))?.state,
                        JobState::Canceled
                    );
                }
                registry.pause(project, true)?;
                registry.pause(project, false)?;
                registry.observe_retained_audit_index_before_grant(&lease, &read, 400)?;
                assert_eq!(registry.due_automatic_audits(400, 8)?.len(), 1);
                registry.request_cancellation(manual.id)?;
                assert!(registry.private_query_maintenance_enabled(&lease)?);
            }
        }
        Ok(())
    }
}
