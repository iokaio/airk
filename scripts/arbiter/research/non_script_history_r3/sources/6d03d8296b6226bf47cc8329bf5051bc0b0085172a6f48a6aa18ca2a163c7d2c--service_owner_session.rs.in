// SPDX-License-Identifier: Apache-2.0
//! Private bounded native owner lifecycle state; not physical-audit evidence.
use super::*;
use serde::{Deserialize, Serialize};

pub(crate) const POLICY: u32 = 1;
const MAX: usize = 4096;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionState {
    Clean,
    Active,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EnrollmentBasis {
    LegacyMetadataUnknown,
    OwnedDrain,
    PhysicalVerify,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerSession {
    pub version: u32,
    pub namespace: Incarnation,
    pub project: ProjectBinding,
    pub worker: Incarnation,
    pub epoch: u64,
    pub nonce: Incarnation,
    pub incarnation: Incarnation,
    pub root: Sha256Digest,
    pub selected: Sha256Digest,
    pub policy: u32,
    pub head: crate::current::CurrentToken,
    pub state: SessionState,
    pub basis: EnrollmentBasis,
}
fn scope(project: ProjectBinding) -> String {
    format!("owner_session:{project}")
}
fn load(
    connection: &Connection,
    project: ProjectBinding,
) -> Result<Option<(String, OwnerSession)>> {
    let row: Option<String> = connection
        .query_row(
            "SELECT substr(engine,1,4097) FROM service_recovery_budget WHERE scope=?1",
            [scope(project)],
            |row| row.get(0),
        )
        .optional()?;
    row.map(|bytes| {
        ensure!(bytes.len() <= MAX, "owner session exceeds bounded encoding");
        let state: OwnerSession = serde_json::from_str(&bytes)?;
        ensure!(
            state.version == 1
                && state.policy == POLICY
                && state.project == project
                && state.epoch > 0,
            "unsupported owner session"
        );
        ensure!(
            state.head.incarnation == state.incarnation,
            "owner session head differs from selected incarnation"
        );
        Ok((bytes, state))
    })
    .transpose()
}
impl ServiceRegistry {
    pub(crate) fn load_owner_session(
        &self,
        lease: &ProjectLease,
    ) -> Result<Option<(String, OwnerSession)>> {
        jobs::worker_epoch(&self.connection, &self.root, lease)?;
        let current = load(&self.connection, lease.identity().0)?;
        if let Some((_, state)) = &current {
            ensure!(
                state.namespace == self.incarnation()?,
                "foreign owner session namespace"
            );
        }
        Ok(current)
    }
    pub(crate) fn compare_recovered_owner_session(
        &mut self,
        lease: &ProjectLease,
        expected: &str,
        next: &OwnerSession,
        job: Sha256Digest,
        proof: &crate::service::collection::integrity::verify::EnabledComponentAuditProof,
    ) -> Result<()> {
        self.compare_owner_session_checked(lease, Some(expected), next, Some((job, proof)))
    }

    /// Caller holds noncreating writer + manifest/root proof. This transaction
    /// performs SQL-only fencing, never filesystem lock acquisition.
    pub(crate) fn compare_owner_session(
        &mut self,
        lease: &ProjectLease,
        expected: Option<&str>,
        next: &OwnerSession,
    ) -> Result<()> {
        self.compare_owner_session_checked(lease, expected, next, None)
    }
    fn compare_owner_session_checked(
        &mut self,
        lease: &ProjectLease,
        expected: Option<&str>,
        next: &OwnerSession,
        audit: Option<(
            Sha256Digest,
            &crate::service::collection::integrity::verify::EnabledComponentAuditProof,
        )>,
    ) -> Result<()> {
        let encoded = serde_json::to_string(next)?;
        ensure!(
            encoded.len() <= MAX && next.version == 1 && next.policy == POLICY,
            "unsupported owner session encoding"
        );

        let registration_state = self.registration_state;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = jobs::worker_epoch(&transaction, &self.root, lease)?;
        let namespace: String = transaction.query_row(
            "SELECT incarnation FROM service_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        let namespace: Incarnation = namespace.parse()?;
        if let Some((job, proof)) = audit {
            let record = jobs::record(&transaction, job)?;
            ensure!(
                record.project == next.project
                    && proof.matches(&transaction, &record)?
                    && proof.exact_head().token()? == next.head,
                "foreign or changed physical owner recovery proof"
            );
        }

        ensure!(
            next.namespace == namespace
                && next.project == lease.identity().0
                && next.epoch == epoch,
            "foreign owner session writer"
        );
        let registered: bool = transaction.query_row(
            if registration_state {
                "SELECT EXISTS(SELECT 1 FROM projects WHERE binding=?1 AND registered=1)"
            } else {
                "SELECT EXISTS(SELECT 1 FROM projects WHERE binding=?1)"
            },
            [next.project.to_string()],
            |row| row.get(0),
        )?;
        ensure!(registered, "native owner registration removed");
        let observed = load(&transaction, next.project)?;
        ensure!(
            observed.as_ref().map(|(raw, _)| raw.as_str()) == expected,
            "owner session changed during observation"
        );
        if next.state == SessionState::Clean {
            super::integrity_guard::require_no_owner_damage(
                &transaction,
                next.project,
                next.incarnation,
            )?;
        }
        if let Some((job, _)) = audit {
            // Separate exact enabled-policy recovery transition; generic success
            // and clean partial drain still cannot clear an inherited veto.
            transaction.execute(
                "DELETE FROM service_recovery_budget WHERE scope=?1 AND engine=?2",
                params![format!("integrity:{}", next.project), job.to_string()],
            )?;
        }
        transaction.execute("INSERT INTO service_recovery_budget VALUES(?1,?2,0,0,0,1) ON CONFLICT(scope) DO UPDATE SET engine=excluded.engine,failures=0,started_ms=0,next_ms=0,suppressed=1",params![scope(next.project),encoded])?;
        transaction.commit()?;
        Ok(())
    }
}

pub(crate) fn recovery_matches(
    connection: &Connection,
    epoch: &crate::service::collection::integrity::verify::RecoveryContext,
) -> Result<bool> {
    let Some((raw, _)) = load(connection, epoch.project)? else {
        return Ok(false);
    };
    let inherited = airk_core::identity::framed_digest(
        b"native-recovery-owner/1",
        &[&serde_json::to_vec(&raw)?],
    );
    let namespace: String = connection.query_row(
        "SELECT incarnation FROM service_meta WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    Ok(inherited == epoch.inherited
        && namespace.parse::<Incarnation>()? == epoch.namespace
        && epoch.profile == crate::service::collection::integrity::verify::NATIVE_RECOVERY_PROFILE)
}
impl ServiceRegistry {
    pub(crate) fn require_recovery_epoch(
        &self,
        lease: &ProjectLease,
        epoch: &crate::service::collection::integrity::verify::RecoveryContext,
    ) -> Result<()> {
        let admitted = jobs::worker_epoch(&self.connection, &self.root, lease)?;
        let job = self.job(epoch.job)?;
        ensure!(
            epoch.project == lease.identity().0
                && epoch.worker_epoch == admitted
                && job.project == epoch.project
                && job.state == airk_core::service::JobState::Running
                && job.fencing_generation == admitted
                && self.integrity_audit_veto(lease)? == Some(epoch.job)
                && recovery_matches(&self.connection, epoch)?,
            "stale native recovery epoch"
        );
        Ok(())
    }
}

/// Fixed owned-fixture failure only; does not expose registry SQL authority.
#[cfg(test)]
impl ServiceRegistry {
    pub(crate) fn test_refuse_integrity_damage(&self, refuse: bool) -> Result<()> {
        if refuse {
            self.connection.execute_batch("CREATE TEMP TRIGGER refuse_recovery_damage BEFORE INSERT ON service_recovery_budget WHEN NEW.scope LIKE 'integrity_damage:%' BEGIN SELECT RAISE(ABORT,'owned damage failure'); END")?;
        } else {
            self.connection
                .execute_batch("DROP TRIGGER refuse_recovery_damage")?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_session_parser_refuses_oversize_unknown_and_partial_records() -> Result<()> {
        let db = Connection::open_in_memory()?;
        db.execute_batch("CREATE TABLE service_recovery_budget(scope TEXT PRIMARY KEY,engine TEXT,failures INTEGER,started_ms INTEGER,next_ms INTEGER,suppressed INTEGER)")?;
        let project = ProjectBinding::from_bytes([7; 32]);
        for bytes in [
            "{}".to_owned(),
            "x".repeat(MAX + 1),
            "{\"version\":2}".to_owned(),
        ] {
            db.execute(
                "INSERT OR REPLACE INTO service_recovery_budget VALUES(?1,?2,0,0,0,1)",
                params![scope(project), bytes],
            )?;
            assert!(load(&db, project).is_err());
        }
        Ok(())
    }
}
