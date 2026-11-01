// SPDX-License-Identifier: Apache-2.0
//! A durable veto admitted with an existing Verify job, not a byte certificate.
//! Reuses the bounded recovery row and existing claim/completion transactions.
use super::*;
use airk_core::service::{JobKind, JobOutcome, JobRecord, JobState};

#[derive(Debug, PartialEq, Eq)]
pub enum IntegrityAuditState {
    Clear,
    LiveAudit { job: Sha256Digest },
    Unresolved { job: Sha256Digest },
}

/// Exact-job drain evidence, not a physical coverage certificate.
#[derive(Debug)]
pub struct AuditDrainProof {
    job: Sha256Digest,
    project: ProjectBinding,
    epoch: u64,
    namespace: Incarnation,
    live: crate::service::data_scope::LiveIntegrity,
}
impl std::fmt::Display for AuditDrainProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("retained audit safely drained without detected damage")
    }
}
impl std::error::Error for AuditDrainProof {}
impl AuditDrainProof {
    pub(crate) fn job(&self) -> Sha256Digest {
        self.job
    }

    pub(crate) fn new(
        registry: &ServiceRegistry,
        lease: &ProjectLease,
        bound: &crate::service::data_scope::BoundDataRoot,
        job: Sha256Digest,
    ) -> Result<Self> {
        let epoch = super::jobs::worker_epoch(&registry.connection, &registry.root, lease)?;
        let record = registry.job(job)?;
        ensure!(
            physical_audit(&registry.connection, &record)?
                && record.project == lease.identity().0
                && record.state == JobState::Running
                && record.fencing_generation == epoch
                && registry.integrity_audit_veto(lease)? == Some(job),
            "foreign audit drain"
        );
        let namespace = registry.incarnation()?;
        let registration = registry.get(record.project)?.registration;
        bound.require_identity(
            Path::new(&registration.data_root),
            &registry.root,
            namespace,
        )?;
        let live = bound.live_integrity();
        ensure!(
            !live.revoked(),
            "damaged audit owner cannot prove safe cancellation"
        );
        Ok(Self {
            job,
            project: record.project,
            epoch,
            namespace,
            live,
        })
    }
    pub fn from_error(error: &anyhow::Error) -> Option<Self> {
        error.downcast_ref::<Self>().map(|proof| Self {
            job: proof.job,
            project: proof.project,
            epoch: proof.epoch,
            namespace: proof.namespace,
            live: proof.live.clone(),
        })
    }
    pub(crate) fn matches(&self, connection: &Connection, job: &JobRecord) -> Result<bool> {
        let namespace: String = connection.query_row(
            "SELECT incarnation FROM service_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        Ok(!self.live.revoked()
            && self.job == job.id
            && self.project == job.project
            && self.epoch == job.fencing_generation
            && self.namespace == namespace.parse()?)
    }
}

pub enum AuditEvidence<'a> {
    Drained(&'a AuditDrainProof),
    Complete(&'a crate::service::collection::integrity::verify::AuditCompletionProof),
}
fn physical_audit(connection: &Connection, job: &JobRecord) -> Result<bool> {
    Ok(job.kind == JobKind::Verify
        || (job.kind == JobKind::Repair
            && super::maintenance::job_is_physical_verify(connection, job.id, job.project)?))
}
fn scope(project: ProjectBinding) -> String {
    format!("integrity:{project}")
}
fn damage_scope(project: ProjectBinding) -> String {
    format!("integrity_damage:{project}")
}
const MAX_NATIVE_DAMAGE: usize = 32;
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DamageVeto {
    version: u32,
    entries: Vec<crate::service::collection::integrity::KnownDamage>,
}
fn damage(connection: &Connection, project: ProjectBinding) -> Result<DamageVeto> {
    let row: Option<String> = connection
        .query_row(
            "SELECT substr(engine,1,16385) FROM service_recovery_budget WHERE scope=?1",
            [damage_scope(project)],
            |row| row.get(0),
        )
        .optional()?;
    let Some(row) = row else {
        return Ok(DamageVeto {
            version: 1,
            entries: Vec::new(),
        });
    };
    ensure!(row.len() <= 16384, "integrity damage veto exceeds bound");
    let state: DamageVeto = serde_json::from_str(&row)?;
    ensure!(
        state.version == 1 && state.entries.len() <= MAX_NATIVE_DAMAGE,
        "unsupported integrity damage veto"
    );
    Ok(state)
}

/// Same bounded parser/policy as native_no_damage, inside owning Clean CAS.
pub(super) fn require_no_owner_damage(
    connection: &Connection,
    project: ProjectBinding,
    incarnation: Incarnation,
) -> Result<()> {
    let recorded = damage(connection, project)?;
    ensure!(
        recorded.entries.len() < MAX_NATIVE_DAMAGE
            && !recorded
                .entries
                .iter()
                .any(|entry| entry.incarnation == incarnation),
        "native owner has durable known damage"
    );
    Ok(())
}

pub(super) fn claimed(transaction: &Connection, job: &JobRecord) -> Result<()> {
    if !physical_audit(transaction, job)? {
        return Ok(());
    }
    // This reserved scope is NOT accepted by public reset_recovery_budget;
    // source retry/engine changes cannot silently erase an integrity veto.
    transaction.execute(
        "INSERT INTO service_recovery_budget VALUES(?1,?2,0,0,0,1)
        ON CONFLICT(scope) DO UPDATE SET engine=excluded.engine,failures=3,next_ms=0,suppressed=1",
        params![scope(job.project), job.id.to_string()],
    )?;
    Ok(())
}
pub(super) fn completed(
    transaction: &Connection,
    job: &JobRecord,
    outcome: &JobOutcome,
    evidence: Option<AuditEvidence<'_>>,
) -> Result<()> {
    if !physical_audit(transaction, job)? {
        return Ok(());
    }
    if matches!(outcome, JobOutcome::Succeeded { .. }) {
        let Some(AuditEvidence::Complete(proof)) = evidence else {
            anyhow::bail!("physical audit success requires actual complete proof")
        };
        ensure!(
            proof.matches(transaction, job)?,
            "foreign or revoked audit completion"
        );
        let JobOutcome::Succeeded {
            revision: Some(revision),
        } = outcome
        else {
            anyhow::bail!("physical audit success requires exact audited publication")
        };
        ensure!(
            *revision == proof.revision()?,
            "physical audit publication mismatch"
        );
        // Damage may be recorded by a sibling read after physical completion
        // but before this SQL settlement, without revoking this proof's Arc.
        require_no_owner_damage(transaction, job.project, revision.incarnation)?;
        // Only this exact admitted job may settle its veto. The service Verify
        // producer must require full physical completion and no sticky damage.
        // Partial/canceled work follows the distinct owned-drain branch below;
        // interruption or an unproved failure cannot settle this guard.
        transaction.execute(
            "DELETE FROM service_recovery_budget WHERE scope=?1 AND engine=?2",
            params![scope(job.project), job.id.to_string()],
        )?;
    } else {
        let clean_terminal = matches!(outcome, JobOutcome::Canceled)
            || matches!(outcome,JobOutcome::Failed{code} if code=="integrity_coverage_incomplete");
        let drained = match evidence {
            Some(AuditEvidence::Drained(proof)) => Some(proof),
            _ => None,
        };
        let safe_cancel = clean_terminal
            && drained
                .map(|proof| proof.matches(transaction, job))
                .transpose()?
                .unwrap_or(false);
        if safe_cancel {
            // A prior unresolved audit is not healed by partial/canceled work.
            transaction.execute(
                "DELETE FROM service_recovery_budget WHERE scope=?1 AND engine=?2 AND failures=0",
                params![scope(job.project), job.id.to_string()],
            )?;
        } else {
            transaction.execute(
                "UPDATE service_recovery_budget SET failures=3 WHERE scope=?1 AND engine=?2",
                params![scope(job.project), job.id.to_string()],
            )?;
        }
    }
    Ok(())
}
impl ServiceRegistry {
    /// Test-only fault on this connection: prove claim rollback when its
    /// physical repair veto cannot be persisted, without exposing raw SQL.
    #[cfg(test)]
    pub(crate) fn refuse_physical_repair_guard_for_test(&self, refuse: bool) -> Result<()> {
        self.connection.execute_batch(if refuse {
            "CREATE TEMP TRIGGER refuse_physical_repair_guard BEFORE INSERT ON service_recovery_budget WHEN NEW.scope LIKE 'integrity:%' BEGIN SELECT RAISE(ABORT,'injected'); END;"
        } else {
            "DROP TRIGGER refuse_physical_repair_guard;"
        })?;
        Ok(())
    }

    /// Failure-only durability in the existing control store. No ordinary
    /// successful query prewrite/fsync and no component eviction at saturation.
    pub fn persist_integrity_damage(
        &self,
        lease: &ProjectLease,
        incarnation: Incarnation,
        component: Sha256Digest,
        observed_unix_ms: u64,
    ) -> Result<()> {
        super::jobs::worker_epoch(&self.connection, &self.root, lease)?;
        let transaction = self.connection.unchecked_transaction()?;
        let mut state = damage(&transaction, lease.identity().0)?;
        if state
            .entries
            .iter()
            .any(|old| old.incarnation == incarnation && old.component == component)
        {
            return Ok(());
        }
        ensure!(
            state.entries.len() < MAX_NATIVE_DAMAGE,
            "integrity damage veto saturated; strict fallback required"
        );
        state
            .entries
            .push(crate::service::collection::integrity::KnownDamage {
                incarnation,
                component,
                observed_unix_ms,
            });
        let bytes = serde_json::to_string(&state)?;
        ensure!(bytes.len() <= 16384, "integrity damage veto exceeds bound");
        transaction.execute(
            "INSERT INTO service_recovery_budget VALUES(?1,?2,3,0,0,1)
            ON CONFLICT(scope) DO UPDATE SET engine=excluded.engine,failures=3,suppressed=1",
            params![damage_scope(lease.identity().0), bytes],
        )?;
        transaction.commit()?;
        Ok(())
    }
    pub fn recorded_integrity_damage(
        &self,
        lease: &ProjectLease,
    ) -> Result<Vec<crate::service::collection::integrity::KnownDamage>> {
        super::jobs::worker_epoch(&self.connection, &self.root, lease)?;
        Ok(damage(&self.connection, lease.identity().0)?.entries)
    }
    /// Absence is merely no pending audit veto, never proof of object bytes.
    /// Requires the actual admitted native worker; unsupported owners cannot use
    /// this metadata to construct a restart capability.
    pub fn integrity_audit_veto(&self, lease: &ProjectLease) -> Result<Option<Sha256Digest>> {
        super::jobs::worker_epoch(&self.connection, &self.root, lease)?;
        let row: Option<(String, i64, bool)> = self
            .connection
            .query_row(
                "SELECT engine,failures,suppressed FROM service_recovery_budget WHERE scope=?1",
                [scope(lease.identity().0)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        row.map(|(job, failures, suppressed)| {
            ensure!(
                matches!(failures, 0 | 3) && suppressed,
                "malformed integrity audit veto"
            );
            job.parse().context("malformed integrity audit job")
        })
        .transpose()
    }
    pub fn integrity_audit_state(&self, lease: &ProjectLease) -> Result<IntegrityAuditState> {
        let epoch = super::jobs::worker_epoch(&self.connection, &self.root, lease)?;
        let Some(job) = self.integrity_audit_veto(lease)? else {
            return Ok(IntegrityAuditState::Clear);
        };
        let failures: i64 = self.connection.query_row(
            "SELECT failures FROM service_recovery_budget WHERE scope=?1",
            [scope(lease.identity().0)],
            |row| row.get(0),
        )?;
        let live = self.job(job).is_ok_and(|record| {
            record.project == lease.identity().0
                && physical_audit(&self.connection, &record).unwrap_or(false)
                && record.state == JobState::Running
                && record.fencing_generation == epoch
        });
        Ok(if failures == 0 && live {
            IntegrityAuditState::LiveAudit { job }
        } else {
            IntegrityAuditState::Unresolved { job }
        })
    }
    /// Called before execution or after the accepted synchronous canceled stack
    /// has unwound. The actual bound owner must still prove safe drained state.
    pub fn drained_verify_before_execution(
        &self,
        lease: &ProjectLease,
        bound: &crate::service::data_scope::BoundDataRoot,
        job: Sha256Digest,
    ) -> Result<AuditDrainProof> {
        AuditDrainProof::new(self, lease, bound, job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::service::{JobState, RequestKey};

    #[test]
    fn actual_verify_claim_arms_veto_atomically_and_failed_completion_preserves_it() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([91; 16]))?;
        let project = ProjectRegistration::new(
            "/integrity-source".into(),
            "/integrity-data".into(),
            "fixture".into(),
        )?;
        registry.register(&project)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        let (namespace, epoch) = registry.request_epoch()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([92; 16]),
        };
        let job = registry.enqueue(project.binding, namespace, &key, JobKind::Verify)?;
        registry.connection.execute_batch("CREATE TEMP TRIGGER refuse_integrity_veto BEFORE INSERT ON service_recovery_budget BEGIN SELECT RAISE(ABORT,'injected veto persistence failure'); END;")?;
        assert!(registry.claim_job_id(&lease, job.id).is_err());
        assert_eq!(
            registry.job(job.id)?.state,
            JobState::Pending,
            "physical audit admission must roll back if its veto could not persist"
        );
        registry
            .connection
            .execute_batch("DROP TRIGGER refuse_integrity_veto;")?;
        let claimed = registry
            .claim_job_id(&lease, job.id)?
            .context("Verify not claimed")?;
        assert_eq!(registry.integrity_audit_veto(&lease)?, Some(claimed.id));
        // Collection metadata persistence is unavailable after physical work.
        // The already committed control veto does not depend on a second write.
        registry.complete_job(
            &lease,
            job.id,
            JobOutcome::Failed {
                code: "integrity_observation_unpersisted".into(),
            },
        )?;
        assert_eq!(registry.integrity_audit_veto(&lease)?, Some(job.id));
        assert!(
            registry
                .reset_recovery_budget(&scope(project.binding))
                .is_err()
        );
        drop(registry);
        drop(lease);
        let mut registry = ServiceRegistry::open(root.path(), namespace)?;
        let mut lease = ProjectLease::acquire(root.path(), project.binding)?;
        registry.admit_worker(&mut lease)?;
        assert_eq!(registry.integrity_audit_veto(&lease)?, Some(job.id));
        Ok(())
    }
}
