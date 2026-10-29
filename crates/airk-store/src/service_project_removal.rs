// SPDX-License-Identifier: Apache-2.0
//! Durable removal after unregister: owned files first, minimal historical identity last.
use super::{ServiceRegistry, bump};
use crate::service::ProjectLease;
use airk_core::service::{ProjectBinding, ProjectRegistration};
use anyhow::{Result, ensure};
use rusqlite::{Connection, TransactionBehavior, params};
use std::path::Path;

pub(super) const GUARD: &str = "e0e5e65940513f67a827c7b9d9549e8d277459ae0eb2bd99c271d9d16430ad47";
const SCHEMA: &str = "CREATE TABLE service_removed_projects(binding TEXT PRIMARY KEY CHECK(length(binding)=64),root_identity TEXT NOT NULL CHECK(length(root_identity)=64),removed_ms INTEGER NOT NULL CHECK(removed_ms>=0)) WITHOUT ROWID;";

pub(super) fn migrate(connection: &mut Connection, version: i64) -> Result<()> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if version < 10 {
        tx.execute_batch(&SCHEMA.replace("CREATE TABLE ", "CREATE TABLE IF NOT EXISTS "))?;
        // Previously removed registrations also finish full removal under 1.6.2.
        tx.execute(
            "UPDATE projects SET cleanup_guard=?1 WHERE registered=0",
            [GUARD],
        )?;
    } else {
        let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='service_removed_projects' AND type='table')", [], |row| row.get(0))?;
        ensure!(exists, "initialized removal history is missing");
    }
    let sql: String = tx.query_row(
        "SELECT sql FROM sqlite_schema WHERE name='service_removed_projects'",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        sql == SCHEMA.trim_end_matches(';'),
        "unexpected removal history schema"
    );
    tx.pragma_update(None, "user_version", 10)?;
    tx.commit()?;
    Ok(())
}

impl ServiceRegistry {
    /// Saved pre-removal registries have no history table; observation never migrates them.
    pub fn source_was_removed(&self, root: &str) -> Result<bool> {
        let present: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='service_removed_projects' AND type='table')",
            [],
            |row| row.get(0),
        )?;
        if !present {
            return Ok(false);
        }
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_removed_projects WHERE root_identity=?1)",
            [removed_root_identity(root).to_string()],
            |row| row.get(0),
        )?)
    }

    pub fn project_was_removed(&self, project: ProjectBinding) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_removed_projects WHERE binding=?1)",
            [project.to_string()],
            |row| row.get(0),
        )?)
    }
    pub fn pending_project_removals(
        &self,
        after: Option<ProjectBinding>,
        limit: usize,
    ) -> Result<Vec<ProjectRegistration>> {
        ensure!((1..=32).contains(&limit), "invalid removal page budget");
        let mut statement = self.connection.prepare("SELECT binding FROM projects WHERE registered=0 AND cleanup_guard=?1 AND binding>?2 ORDER BY binding LIMIT ?3")?;
        let ids = statement
            .query_map(
                params![
                    GUARD,
                    after.map(|id| id.to_string()).unwrap_or_default(),
                    limit as i64
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter()
            .map(|id| Ok(self.retained_project(id.parse()?)?.registration))
            .collect()
    }

    /// Runs only in the supervised metadata/source child. The OS lease excludes
    /// all project workers; the durable guard excludes re-registration throughout I/O.
    /// No source path is opened, canonicalized or enumerated.
    pub fn purge_removed_project_turn(&mut self, project: ProjectBinding) -> Result<bool> {
        let lease = match ProjectLease::acquire(&self.root, project) {
            Ok(lease) => lease,
            Err(error)
                if matches!(
                    error.downcast_ref::<std::fs::TryLockError>(),
                    Some(std::fs::TryLockError::WouldBlock)
                ) =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        let eligible: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE binding=?1 AND registered=0 AND cleanup_guard=?2)", params![project.to_string(), GUARD], |row| row.get(0))?;
        if !eligible {
            return Ok(true);
        }
        let providers: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM service_provider_drains WHERE json_extract(CAST(payload AS TEXT),'$.identity.project')=?1)", [project.to_string()], |row| row.get(0))?;
        if providers {
            return Ok(false);
        }
        let registration = self.retained_project(project)?.registration;
        let directory = Path::new(&registration.data_root)
            .join("collections-1.4")
            .join(&registration.collection);
        crate::managed_collection::validate_ancestors(&directory)?;
        let present = match std::fs::symlink_metadata(&directory) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        if present {
            super::super::data_scope::check(
                Path::new(&registration.data_root),
                &self.root,
                self.incarnation()?,
            )?;
            let quota = crate::quota::Quota::open_service(Path::new(&registration.data_root))?;
            let previous = quota.policy()?;
            if previous.collections.contains_key(&registration.collection) {
                let mut policy = previous.clone();
                policy.collections.remove(&registration.collection);
                quota.configure_if_current(policy, Some(&previous))?;
            }
            if !crate::service::collection::Collection::purge_removed_turn(&registration, &lease)? {
                return Ok(false);
            }
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure!(
            tx.query_row(
                "SELECT registered=0 AND cleanup_guard=?1 FROM projects WHERE binding=?2",
                params![GUARD, project.to_string()],
                |row| row.get::<_, bool>(0)
            )?,
            "removal ownership changed"
        );
        // Child metadata is removed before its FK parents. Other projects/plans remain.
        for table in [
            "service_plan_jobs",
            "service_plan_items",
            "service_progress",
            "service_watch_traversal",
            "service_source_outcomes",
            "service_watch_events",
            "service_watch",
            "service_inspections",
            "service_maintenance",
            "service_background",
            "service_setup_pending",
            "service_semantic_review",
        ] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE project=?1"),
                [project.to_string()],
            )?;
        }
        tx.execute(
            "DELETE FROM service_jobs WHERE project=?1",
            [project.to_string()],
        )?;
        tx.execute(
            "DELETE FROM service_events WHERE project=?1",
            [project.to_string()],
        )?;
        tx.execute(
            "DELETE FROM service_recovery_budget WHERE scope=?1 OR scope=?2",
            params![format!("source:{project}"), format!("work:{project}")],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO service_removed_projects VALUES(?1,?2,?3)",
            params![
                project.to_string(),
                removed_root_identity(&registration.root).to_string(),
                i64::try_from(super::maintenance::clock_ms()?)?
            ],
        )?;
        tx.execute(
            "DELETE FROM projects WHERE binding=?1",
            [project.to_string()],
        )?;
        bump(&tx)?;
        tx.commit()?;
        Ok(true)
    }
}

pub(super) fn removed_root_identity(root: &str) -> airk_core::identity::Sha256Digest {
    airk_core::identity::framed_digest(b"airk-removed-source/1", &[root.as_bytes()])
}
