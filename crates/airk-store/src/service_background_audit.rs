// SPDX-License-Identifier: Apache-2.0
//! Bounded integrity-maintenance state inside the existing service registry.
//! Rows and dated observations confer no query or repair authority.
use super::super::{ServiceRegistry, jobs};
use crate::service::{
    ProjectLease,
    collection::integrity::verify::{RetainedAuditCompletionProof, RetainedAuditObservation},
};
use airk_core::service::{JobKind, JobState, ProjectBinding};
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
const MAX_STATE: usize = 4096;
pub(super) const SCHEMA: &str = "CREATE TABLE service_background_audit(
 project TEXT PRIMARY KEY REFERENCES projects(binding) ON DELETE CASCADE,
 revision INTEGER NOT NULL CHECK(revision>=0),
 campaign INTEGER NOT NULL CHECK(campaign>=0),
 job TEXT UNIQUE REFERENCES service_jobs(id) ON DELETE SET NULL,
 parked INTEGER NOT NULL CHECK(parked IN(0,1)),
 manual INTEGER NOT NULL CHECK(manual IN(0,1)),
 intent BLOB CHECK(intent IS NULL OR length(intent)<=4096),
 coverage BLOB CHECK(coverage IS NULL OR length(coverage)<=4096),
 blocked BLOB CHECK(blocked IS NULL OR length(blocked)<=4096),
 next_ms INTEGER NOT NULL CHECK(next_ms>=0),
 CHECK(parked=0 OR job IS NOT NULL)) WITHOUT ROWID;";

pub(crate) fn validate_existing(connection: &Connection, required: bool) -> Result<bool> {
    let sql: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='service_background_audit'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(sql) = sql {
        ensure!(
            sql == SCHEMA.trim_end_matches(';'),
            "unexpected background audit schema"
        );
        return Ok(true);
    }
    ensure!(!required, "initialized background audit state is missing");
    Ok(false)
}
pub(crate) fn migrate(connection: &mut Connection, version: i64) -> Result<()> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !validate_existing(&tx, version >= 11)? {
        tx.execute_batch(SCHEMA)?;
    }
    let current: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current < 11 {
        tx.pragma_update(None, "user_version", 11)?;
    }
    tx.commit()?;
    Ok(())
}
impl ServiceRegistry {
    /// Record actual retained-data coverage while the same fenced Verify still
    /// owns its live drain evidence. This observation cannot clear an audit veto,
    /// mark whole optional coverage complete or authorize private fast readers.
    pub fn record_retained_audit(
        &mut self,
        lease: &ProjectLease,
        proof: &RetainedAuditCompletionProof,
    ) -> Result<()> {
        let observation = proof.observation();
        observation.validate()?;
        let bytes = serde_json::to_vec(observation)?;
        ensure!(
            bytes.len() <= MAX_STATE,
            "retained audit observation exceeds bound"
        );
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = jobs::worker_epoch(&tx, &self.root, lease)?;
        // The opaque proof selects its own exact admitted job; no caller flag or
        // serialized coverage record can substitute for physical evidence.
        let job = proof.job();
        let record = jobs::record(&tx, job)?;
        ensure!(
            record.project == lease.identity().0
                && matches!(record.kind, JobKind::Verify | JobKind::Repair)
                && record.state == JobState::Running
                && record.fencing_generation == epoch
                && proof.matches(&tx, &record)?,
            "foreign retained audit observation"
        );
        super::super::integrity_guard::require_no_owner_damage(
            &tx,
            record.project,
            observation.head.incarnation,
        )?;
        tx.execute("INSERT OR IGNORE INTO service_background_audit(project,revision,campaign,job,parked,manual,intent,coverage,blocked,next_ms) VALUES(?1,0,0,NULL,0,0,NULL,NULL,NULL,0)", [record.project.to_string()])?;
        let changed = tx.execute("UPDATE service_background_audit SET revision=revision+1,coverage=?1 WHERE project=?2 AND coverage IS NOT ?1 AND revision<9223372036854775807", params![bytes, record.project.to_string()])?;
        if changed == 0 {
            let equal: bool = tx.query_row(
                "SELECT coverage IS ?1 FROM service_background_audit WHERE project=?2",
                params![bytes, record.project.to_string()],
                |row| row.get(0),
            )?;
            ensure!(equal, "background audit revision exhausted");
        }
        tx.commit()?;
        Ok(())
    }
    /// Saved observations never initialize or migrate an older registry.
    pub fn retained_audit_observation(
        &self,
        project: ProjectBinding,
    ) -> Result<Option<RetainedAuditObservation>> {
        let exists: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='service_background_audit' AND type='table')", [], |row| row.get(0))?;
        if !exists {
            return Ok(None);
        }
        let bytes: Option<Option<Vec<u8>>> = self
            .connection
            .query_row(
                "SELECT substr(coverage,1,4097) FROM service_background_audit WHERE project=?1",
                [project.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        bytes
            .flatten()
            .map(|bytes| {
                ensure!(
                    bytes.len() <= MAX_STATE,
                    "retained audit observation exceeds bound"
                );
                let observation: RetainedAuditObservation = serde_json::from_slice(&bytes)?;
                observation.validate()?;
                Ok(observation)
            })
            .transpose()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::identity::Incarnation;
    #[test]
    fn registry_audit_migration_gates_older_state_and_never_recreates_missing_initialized_table()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([81; 16]))?;
        registry
            .connection
            .execute_batch("DROP TABLE service_background_audit; PRAGMA user_version=10;")?;
        drop(registry);
        let saved = ServiceRegistry::open_saved(root.path())?;
        assert!(
            saved
                .retained_audit_observation(ProjectBinding::from_bytes([1; 32]))?
                .is_none()
        );
        let version: i64 = saved
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        assert_eq!(version, 10);
        drop(saved);
        let registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([82; 16]))?;
        let version: i64 = registry
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        assert_eq!(version, 11);
        registry
            .connection
            .execute_batch("DROP TABLE service_background_audit")?;
        drop(registry);
        let path = root.path().join("service-1.4.sqlite3");
        let missing = std::fs::read(&path)?;
        assert!(ServiceRegistry::open(root.path(), Incarnation::from_bytes([83; 16])).is_err());
        assert!(ServiceRegistry::open_saved(root.path()).is_err());
        assert_eq!(
            std::fs::read(&path)?,
            missing,
            "missing initialized schema changed before refusal"
        );
        let connection = Connection::open(&path)?;
        connection.execute_batch(
            "CREATE TABLE service_background_audit(unknown TEXT); PRAGMA user_version=10;",
        )?;
        drop(connection);
        let unknown = std::fs::read(&path)?;
        assert!(ServiceRegistry::open(root.path(), Incarnation::from_bytes([84; 16])).is_err());
        assert_eq!(
            std::fs::read(&path)?,
            unknown,
            "foreign audit schema was rewritten"
        );
        Ok(())
    }
}
