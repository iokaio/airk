// SPDX-License-Identifier: Apache-2.0
//! Durable removal after unregister: owned files first, minimal historical identity last.
use super::{ServiceRegistry, bump};
use crate::service::ProjectLease;
use airk_core::service::{ProjectBinding, ProjectRegistration};
use anyhow::{Context, Result, ensure};
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
    tx.pragma_update(None, "user_version", version.max(10))?;
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
        super::catalog::source_change::require_retired(&self.connection, project)?;
        ensure!(
            self.catalog_active_split_for_binding(project)?.is_none(),
            "active split recovery owns this retired participant; finish exact split recovery first"
        );
        let registration = self.retained_project(project)?.registration;
        let policy = super::catalog::policy::retired_for_binding(&self.connection, project)?;
        let directory = Path::new(&registration.data_root)
            .join("collections-1.4")
            .join(&registration.collection);
        crate::managed_collection::validate_ancestors(&directory)?;
        let present = match std::fs::symlink_metadata(&directory) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        // Explicit committed retirement, not a source watcher observation, is
        // the authority for logical cleanup. Never recreate a missing data root
        // just to retain a capsule or open its quota ledger.
        let absent_data = if present {
            None
        } else {
            RetiredDataAbsence::capture(Path::new(&registration.data_root))?
        };
        if let Some(snapshot) = &policy {
            let job = self.job(snapshot.job)?;
            ensure!(
                matches!(
                    job.state,
                    airk_core::service::JobState::Succeeded
                        | airk_core::service::JobState::Failed
                        | airk_core::service::JobState::Canceled
                ),
                "retirement must settle the current policy job before cleanup"
            );
        }
        let uninitialized_data = if !present && absent_data.is_none() {
            RetiredUninitializedData::capture(Path::new(&registration.data_root))?
        } else {
            None
        };
        let absent_collection = if !present && absent_data.is_none() && uninitialized_data.is_none()
        {
            match crate::quota::AbsentCollectionSettlement::settle(
                &registration,
                &lease,
                &self.root,
                self.incarnation()?,
            ) {
                Ok(proof) => Some(proof),
                Err(error)
                    if matches!(
                        error.downcast_ref::<std::fs::TryLockError>(),
                        Some(std::fs::TryLockError::WouldBlock)
                    ) =>
                {
                    return Ok(false);
                }
                Err(error) => return Err(error),
            }
        } else {
            None
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
            if let Some(snapshot) = &policy {
                // Stable ownership must also make old readers refuse a reset
                // collection whose inner publication has already been removed.
                let owner_present = directory.join("service-owner.json").try_exists()?;
                let copy = crate::current::policy_capsule::PolicyCapsule::retirement_copy(
                    &directory,
                    &registration,
                    snapshot,
                )?;
                if owner_present || copy.is_some() {
                    let bytes = u64::try_from(
                        copy.as_ref()
                            .map(serde_json::to_vec)
                            .transpose()?
                            .map_or(0, |value| value.len()),
                    )? + 16384;
                    let reservation = quota.reserve(&registration.collection, bytes)?;
                    reservation.charge(bytes)?;
                    if owner_present {
                        crate::service::collection::Collection::existing(
                            Path::new(&registration.data_root),
                            &registration.root,
                            &registration.collection,
                        )?
                        .retain_source_policy(&snapshot.policy)?;
                    }
                    if let Some(copy) = copy {
                        copy.save(&directory)?;
                    }
                }
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
        if let Some(absent) = &absent_data {
            absent.revalidate()?;
        }
        if let Some(absent) = &absent_collection {
            absent.revalidate()?;
        }
        if let Some(uninitialized) = &uninitialized_data {
            uninitialized.revalidate()?;
        }
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
            "service_background_audit",
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
            "DELETE FROM service_jobs WHERE project=?1 AND (?2 IS NULL OR id<>?2)",
            params![
                project.to_string(),
                policy.as_ref().map(|snapshot| snapshot.job.to_string())
            ],
        )?;
        tx.execute(
            "DELETE FROM service_events WHERE project=?1",
            [project.to_string()],
        )?;
        tx.execute(
            "DELETE FROM service_recovery_budget WHERE scope=?1 OR scope=?2 OR scope=?3 OR scope=?4 OR scope=?5",
            params![format!("source:{project}"), format!("work:{project}"), format!("integrity:{project}"), format!("integrity_damage:{project}"), format!("owner_session:{project}")],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO service_removed_projects VALUES(?1,?2,?3)",
            params![
                project.to_string(),
                removed_root_identity(&registration.root).to_string(),
                i64::try_from(super::maintenance::clock_ms()?)?
            ],
        )?;
        let schema = tx.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?;
        let retained_catalog_index = if matches!(
            schema,
            12 | super::runtime_contract::IN_PROCESS_SCHEMA
        ) {
            tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM index_definitions WHERE legacy_binding=?1 AND active_registration=?1 AND tombstone=1)",
                [project.to_string()],
                |row| row.get::<_, bool>(0),
            )?
        } else {
            false
        };
        if policy.is_some() || retained_catalog_index {
            // Keep the exact inactive catalog backing identity after cleanup.
            // Removal suppression history can legitimately be cleared by a new
            // explicit index on this source; it cannot be the sole permanent
            // proof for reconciling the old full retirement fence. Policy jobs
            // additionally retain this registration through their existing FK.
            // Catalog index/tombstone capacity already bounds these identities.
            tx.execute(
                "UPDATE projects SET cleanup_guard=NULL WHERE binding=?1 AND registered=0",
                [project.to_string()],
            )?;
        } else {
            tx.execute(
                "DELETE FROM projects WHERE binding=?1",
                [project.to_string()],
            )?;
        }
        // The held ancestor still names the absent data tree at the native
        // metadata commit boundary. This records no physical disposal claim.
        if let Some(absent) = &absent_data {
            absent.revalidate()?;
        }
        if let Some(absent) = &absent_collection {
            absent.revalidate()?;
        }
        if let Some(uninitialized) = &uninitialized_data {
            uninitialized.revalidate()?;
        }
        bump(&tx)?;
        tx.commit()?;
        Ok(true)
    }
}

pub(super) fn removed_root_identity(root: &str) -> airk_core::identity::Sha256Digest {
    airk_core::identity::framed_digest(b"airk-removed-source/1", &[root.as_bytes()])
}

// Metadata-only registration does not initialize or claim its data root. This
// proof authorizes only dropping already retired registry references: no file,
// allowance, ownership receipt or advisory is created, removed or adopted.
struct RetiredUninitializedData {
    path: std::path::PathBuf,
    root: super::catalog::setup::storage::SetupDirectory,
}
impl RetiredUninitializedData {
    fn capture(data: &Path) -> Result<Option<Self>> {
        let root = super::catalog::setup::storage::SetupDirectory::open_canonical(data)?;
        match root.open_entry(std::ffi::OsStr::new(super::super::data_scope::OWNER)) {
            Ok(_) => return Ok(None),
            Err(error) if missing_entry(&error) => (),
            Err(error) => return Err(error),
        }
        let proof = Self {
            path: data.to_path_buf(),
            root,
        };
        proof.revalidate()?;
        Ok(Some(proof))
    }
    fn revalidate(&self) -> Result<()> {
        use std::ffi::OsStr;
        ensure!(
            super::catalog::setup::storage::SetupDirectory::open_canonical(&self.path)?
                .identity()?
                == self.root.identity()?,
            "uninitialized retired data root changed"
        );
        // Any native ownership/claim/quota state requires the ordinary complete
        // ownership-aware cleanup. Unrelated user/legacy files are untouched.
        for name in [
            super::super::data_scope::OWNER,
            super::super::data_scope::PART,
            super::super::data_scope::LOCK,
            ".quota-1.4",
            ".lifecycle-1.4.lock",
        ] {
            match self.root.open_entry(OsStr::new(name)) {
                Err(error) if missing_entry(&error) => (),
                Err(error) => return Err(error),
                Ok(_) => anyhow::bail!("uninitialized retired data root contains native state"),
            }
        }
        match self.root.open_dir(OsStr::new("collections-1.4")) {
            Err(error) if missing_entry(&error) => (),
            Err(error) => return Err(error),
            Ok(directory) => ensure!(
                directory.entries()?.next().transpose()?.is_none(),
                "unbound retired collection namespace is not empty"
            ),
        }
        Ok(())
    }
}
fn missing_entry(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

// Native-only, nonserializable proof. No source path, quota opener, marker or
// missing directory creation is involved. A present/redirected/unavailable data
// root must use the existing ownership-aware physical cleanup instead.
struct RetiredDataAbsence {
    ancestor_path: std::path::PathBuf,
    ancestor: super::catalog::setup::storage::SetupDirectory,
    missing: std::ffi::OsString,
}
impl RetiredDataAbsence {
    fn capture(data: &Path) -> Result<Option<Self>> {
        match std::fs::symlink_metadata(data) {
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let mut selected = None;
        for candidate in data.ancestors().skip(1) {
            match std::fs::symlink_metadata(candidate) {
                Ok(metadata) => {
                    ensure!(
                        metadata.is_dir() && !metadata.file_type().is_symlink(),
                        "retired data ancestor is not a plain directory"
                    );
                    selected = Some(candidate.to_path_buf());
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        let ancestor_path = selected.context("retired data has no existing ancestor")?;
        let ancestor =
            super::catalog::setup::storage::SetupDirectory::open_canonical(&ancestor_path)?;
        let relative = data.strip_prefix(&ancestor_path)?;
        let missing = match relative
            .components()
            .next()
            .context("retired data missing child absent")?
        {
            std::path::Component::Normal(name) => name.to_os_string(),
            _ => anyhow::bail!("invalid retired data absence path"),
        };
        let proof = Self {
            ancestor_path,
            ancestor,
            missing,
        };
        proof.revalidate()?;
        Ok(Some(proof))
    }
    fn revalidate(&self) -> Result<()> {
        ensure!(
            super::catalog::setup::storage::SetupDirectory::open_canonical(&self.ancestor_path)?
                .identity()?
                == self.ancestor.identity()?,
            "retired data ancestor changed"
        );
        match self.ancestor.open_dir(&self.missing) {
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
            Ok(_) => anyhow::bail!(
                "retired data reappeared; physical cleanup requires ownership validation"
            ),
        }
    }
}

#[cfg(test)]
mod missing_data_tests {
    use super::*;
    #[test]
    fn missing_data_proof_preserves_reappeared_tree() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let path = owner.path().canonicalize()?.join("removed").join("data");
        let proof = RetiredDataAbsence::capture(&path)?.context("absence proof missing")?;
        std::fs::create_dir_all(&path)?;
        std::fs::write(path.join("sentinel"), b"preserved")?;
        assert!(proof.revalidate().is_err());
        assert_eq!(std::fs::read(path.join("sentinel"))?, b"preserved");
        assert!(RetiredDataAbsence::capture(&path)?.is_none());
        Ok(())
    }
    #[test]
    fn metadata_only_registration_retires_without_claiming_uninitialized_storage() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let data = base.join("data");
        let source = base.join("source");
        let control = base.join("control");
        for path in [&data, &source, &control] {
            std::fs::create_dir(path)?;
        }
        std::fs::write(source.join("source.md"), b"preserved source")?;
        std::fs::write(data.join("unrelated-user-data"), b"preserved user data")?;
        std::fs::create_dir(data.join("collections-1.4"))?;
        let registration = ProjectRegistration::new(
            source.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            "never-started".into(),
        )?;
        let mut registry = ServiceRegistry::open(
            &control,
            airk_core::identity::Incarnation::from_bytes([61; 16]),
        )?;
        registry.register(&registration)?;
        std::fs::rename(&source, base.join("offline-source"))?;
        let epoch = registry.get(registration.binding)?.fencing_generation;
        assert!(registry.retire_missing_source(registration.binding, epoch, 0)?);
        assert!(registry.purge_removed_project_turn(registration.binding)?);
        assert!(registry.project_was_removed(registration.binding)?);
        assert!(registry.retained_project(registration.binding).is_err());
        assert_eq!(
            std::fs::read(data.join("unrelated-user-data"))?,
            b"preserved user data"
        );
        assert_eq!(
            std::fs::read(base.join("offline-source/source.md"))?,
            b"preserved source"
        );
        let mut names = std::fs::read_dir(&data)?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?;
        names.sort();
        assert_eq!(
            names,
            vec![
                std::ffi::OsString::from("collections-1.4"),
                std::ffi::OsString::from("unrelated-user-data")
            ]
        );
        assert!(
            std::fs::read_dir(data.join("collections-1.4"))?
                .next()
                .is_none()
        );
        registry.register(&registration)?;
        assert_eq!(
            registry.get(registration.binding)?.registration,
            registration
        );
        // Actual initial binding can precede the first writer/collection namespace.
        // Retirement must not manufacture those missing nodes just to finish.
        std::fs::remove_dir(data.join("collections-1.4"))?;
        let bound = crate::service::data_scope::bind(&data, &control, registry.incarnation()?)?;
        let owner_path = data.join(crate::service::data_scope::OWNER);
        let owner_bytes = std::fs::read(&owner_path)?;
        let names = || -> Result<Vec<std::ffi::OsString>> {
            let mut names = std::fs::read_dir(&data)?
                .map(|e| e.map(|e| e.file_name()))
                .collect::<std::io::Result<Vec<_>>>()?;
            names.sort();
            Ok(names)
        };
        let bound_names = names()?;
        let epoch = registry.get(registration.binding)?.fencing_generation;
        assert!(registry.retire_missing_source(registration.binding, epoch, 0)?);
        assert!(registry.purge_removed_project_turn(registration.binding)?);
        bound.check()?;
        assert_eq!(std::fs::read(owner_path)?, owner_bytes);
        assert_eq!(names()?, bound_names);
        assert!(!data.join("collections-1.4").exists());
        assert_eq!(
            std::fs::read(data.join("unrelated-user-data"))?,
            b"preserved user data"
        );
        Ok(())
    }
    #[test]
    fn uninitialized_retirement_proof_refuses_native_state_and_replaced_root() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let data = base.join("data");
        std::fs::create_dir(&data)?;
        let proof = RetiredUninitializedData::capture(&data)?.context("fresh proof missing")?;
        for name in [
            super::super::super::data_scope::PART,
            super::super::super::data_scope::LOCK,
            ".quota-1.4",
            ".lifecycle-1.4.lock",
        ] {
            std::fs::write(data.join(name), b"unknown native state")?;
            assert!(proof.revalidate().is_err());
            assert!(RetiredUninitializedData::capture(&data).is_err());
            assert_eq!(std::fs::read(data.join(name))?, b"unknown native state");
            std::fs::remove_file(data.join(name))?;
        }
        std::fs::create_dir(data.join("collections-1.4"))?;
        std::fs::write(data.join("collections-1.4/unknown"), b"unknown collection")?;
        assert!(proof.revalidate().is_err());
        std::fs::remove_file(data.join("collections-1.4/unknown"))?;
        std::fs::rename(&data, base.join("old-data"))?;
        std::fs::create_dir(&data)?;
        assert!(proof.revalidate().is_err());
        Ok(())
    }
}
