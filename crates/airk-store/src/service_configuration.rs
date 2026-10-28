// SPDX-License-Identifier: Apache-2.0
//! Per-job immutable configuration, within the bounded service registry.
use super::ServiceRegistry;
use airk_core::{
    identity::Sha256Digest,
    service::JobError,
    service_configuration::{ConfigurationWork, MAX_CONFIGURATION_BYTES},
};
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) const SCHEMA: &str = "CREATE TABLE service_configuration(
    job TEXT PRIMARY KEY REFERENCES service_jobs(id) ON DELETE CASCADE,
    payload BLOB NOT NULL CHECK(length(payload)<=16640),
    result BLOB NOT NULL CHECK(length(result)=16392));";

pub(super) fn write_result(connection: &Connection, job: Sha256Digest, bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() <= MAX_CONFIGURATION_BYTES,
        "configuration result exceeds budget"
    );
    let row: i64 = connection.query_row(
        "SELECT rowid FROM service_configuration WHERE job=?1",
        [job.to_string()],
        |row| row.get(0),
    )?;
    let mut blob = connection.blob_open("main", "service_configuration", "result", row, false)?;
    ensure!(
        blob.len() == MAX_CONFIGURATION_BYTES + 8,
        "configuration result reservation changed"
    );
    use std::io::Write;
    blob.write_all(b"ACR1")?;
    blob.write_all(&u32::try_from(bytes.len())?.to_be_bytes())?;
    blob.write_all(bytes)?;
    blob.close()?;
    Ok(())
}

pub(super) fn migrate_v3(connection: &Connection) -> Result<()> {
    connection
        .execute_batch("ALTER TABLE service_configuration RENAME TO service_configuration_v3;")?;
    connection.execute_batch(SCHEMA)?;
    let mut statement=connection.prepare("SELECT job,substr(payload,1,16641),substr(result,1,16385) FROM service_configuration_v3 LIMIT 1025")?;
    let mut rows = statement.query([])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        ensure!(count <= 1024, "configuration migration exceeds job budget");
        let job: String = row.get(0)?;
        let payload: Vec<u8> = row.get(1)?;
        let result: Option<Vec<u8>> = row.get(2)?;
        ensure!(payload.len() <= 16640, "configuration work exceeds budget");
        let work: ConfigurationWork = serde_json::from_slice(&payload)?;
        work.request.validate()?;
        connection.execute(
            "INSERT INTO service_configuration VALUES(?1,?2,zeroblob(16392))",
            params![job, payload],
        )?;
        if let Some(result) = result {
            let _: serde_json::Value = serde_json::from_slice(&result)?;
            write_result(connection, job.parse()?, &result)?;
        }
    }
    drop(rows);
    drop(statement);
    connection.execute_batch("DROP TABLE service_configuration_v3;")?;
    Ok(())
}

pub(super) fn work(
    connection: &Connection,
    job: Sha256Digest,
) -> Result<Option<ConfigurationWork>> {
    let bytes: Option<Vec<u8>> = connection
        .query_row(
            "SELECT substr(payload,1,16641) FROM service_configuration WHERE job=?1",
            [job.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    bytes
        .map(|bytes| {
            ensure!(
                bytes.len() <= MAX_CONFIGURATION_BYTES + 256,
                "configuration work exceeds job budget"
            );
            let work: ConfigurationWork = serde_json::from_slice(&bytes)?;
            work.request.validate()?;
            Ok(work)
        })
        .transpose()
}
pub(super) fn same_request(
    connection: &Connection,
    job: Sha256Digest,
    requested: Option<&ConfigurationWork>,
) -> Result<()> {
    let saved = work(connection, job)?;
    // Service-engine identity is stamped at first acceptance, not chosen by a
    // client. A replay after upgrade still returns its original accepted job.
    ensure!(
        saved.as_ref().map(|work| &work.request) == requested.map(|work| &work.request),
        JobError::KeyConflict
    );
    Ok(())
}
pub(super) fn insert(
    connection: &Connection,
    job: Sha256Digest,
    work: &ConfigurationWork,
) -> Result<()> {
    work.request.validate()?;
    let bytes = serde_json::to_vec(work)?;
    ensure!(
        bytes.len() <= MAX_CONFIGURATION_BYTES + 256,
        "configuration work exceeds job budget"
    );
    connection.execute(
        "INSERT INTO service_configuration(job,payload,result) VALUES(?1,?2,zeroblob(16392))",
        params![job.to_string(), bytes],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO service_background VALUES(?1,0,0,0)",
        [super::jobs::record(connection, job)?.project.to_string()],
    )?;
    Ok(())
}

pub(super) fn result(
    connection: &Connection,
    job: Sha256Digest,
) -> Result<Option<serde_json::Value>> {
    let bytes: Vec<u8> = connection.query_row(
        "SELECT substr(result,1,16393) FROM service_configuration WHERE job=?1",
        [job.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        bytes.len() == MAX_CONFIGURATION_BYTES + 8,
        "configuration result reservation changed"
    );
    if bytes[..8] == [0; 8] {
        return Ok(None);
    }
    ensure!(
        &bytes[..4] == b"ACR1",
        "invalid configuration result header"
    );
    let length = u32::from_be_bytes(bytes[4..8].try_into()?) as usize;
    ensure!(
        length <= MAX_CONFIGURATION_BYTES,
        "configuration result exceeds budget"
    );
    Ok(Some(serde_json::from_slice(&bytes[8..8 + length])?))
}
pub(super) fn recover_results(
    connection: &Connection,
    lease: &crate::service::ProjectLease,
    epoch: u64,
) -> Result<()> {
    let mut statement=connection.prepare("SELECT id FROM service_jobs WHERE project=?1 AND kind='configure' AND state='running' AND epoch<>?2 ORDER BY seq LIMIT 1025")?;
    let ids = statement
        .query_map(
            params![lease.binding.to_string(), i64::try_from(epoch)?],
            |row| row.get::<_, String>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        ids.len() <= 1024,
        "configuration recovery exceeds job budget"
    );
    drop(statement);
    for id in ids {
        if matches!(result(connection, id.parse()?), Ok(Some(_))) {
            connection.execute(
                "UPDATE service_jobs SET state='succeeded',outcome=?1 WHERE id=?2",
                params![
                    serde_json::to_vec(&airk_core::service::JobOutcome::Succeeded {
                        revision: None
                    })?,
                    id
                ],
            )?;
            super::unregistration::finish_if_requested(connection, lease.binding, id.parse()?)?;
            super::cleanup::finish_if_requested(connection, lease.binding, id.parse()?)?;
        }
    }
    Ok(())
}
impl ServiceRegistry {
    pub fn latest_configuration(
        &self,
        project: airk_core::service::ProjectBinding,
    ) -> Result<Option<airk_core::service::JobRecord>> {
        let id: Option<String> = self.connection.query_row("SELECT id FROM service_jobs WHERE project=?1 AND kind='configure' ORDER BY seq DESC LIMIT 1",
            [project.to_string()], |row| row.get(0)).optional()?;
        id.map(|id| super::jobs::record(&self.connection, id.parse()?))
            .transpose()
    }
    pub fn configuration_result(&self, job: Sha256Digest) -> Result<Option<serde_json::Value>> {
        result(&self.connection, job)
    }
    pub fn record_configuration_result(
        &mut self,
        lease: &crate::service::ProjectLease,
        job: Sha256Digest,
        result: &serde_json::Value,
    ) -> Result<()> {
        let bytes = serde_json::to_vec(result)?;
        ensure!(
            bytes.len() <= MAX_CONFIGURATION_BYTES,
            "configuration result exceeds budget"
        );
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let epoch = super::jobs::worker_epoch(&transaction, &self.root, lease)?;
        let record = super::jobs::record(&transaction, job)?;
        ensure!(
            record.project == lease.binding
                && record.kind == airk_core::service::JobKind::Configure
                && record.state == airk_core::service::JobState::Running
                && record.fencing_generation == epoch,
            JobError::StaleWorker
        );
        let unrecorded: bool = transaction.query_row(
            "SELECT substr(result,1,8)=zeroblob(8) FROM service_configuration WHERE job=?1",
            [job.to_string()],
            |row| row.get(0),
        )?;
        ensure!(unrecorded, "configuration result was already recorded");
        write_result(&transaction, job, &bytes)?;
        let work = work(&transaction, job)?.ok_or(JobError::UnknownJob)?;
        if work.request.configuration.requests_source_pass() {
            super::background::set(
                &transaction,
                lease.binding,
                airk_core::service::JobKind::Index,
                true,
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn configuration_work(&self, job: Sha256Digest) -> Result<Option<ConfigurationWork>> {
        work(&self.connection, job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::ProjectLease;
    use airk_core::{
        identity::Incarnation,
        service::{JobKind, JobState, ProjectRegistration, RequestKey},
        service_configuration::{ConfigurationRequest, ProjectConfiguration},
    };
    use anyhow::Context;

    fn open(base: &std::path::Path, caller: Incarnation) -> Result<ServiceRegistry> {
        let control = base.join("control");
        std::fs::create_dir_all(&control)?;
        ServiceRegistry::open(&control, caller)
    }
    fn request(enabled: bool) -> ConfigurationRequest {
        ConfigurationRequest {
            prepare: false,
            incarnation: Incarnation::from_bytes([2; 16]),
            settings: Sha256Digest::from_bytes([3; 32]),
            configuration: ProjectConfiguration::Vectors { enabled },
        }
    }
    fn key(registry: &mut ServiceRegistry, nonce: u8) -> Result<RequestKey> {
        let (namespace, epoch) = registry.request_epoch()?;
        Ok(RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([nonce; 16]),
        })
    }
    fn project(base: &std::path::Path) -> Result<ProjectRegistration> {
        ProjectRegistration::new(
            base.join("source").to_str().unwrap().into(),
            base.join("data").to_str().unwrap().into(),
            "configuration".into(),
        )
    }
    #[test]
    fn unregister_is_atomic_preserves_other_projects_and_old_request_replay_cannot_unregister_a_reregistration()
    -> Result<()> {
        use airk_core::service::JobOutcome;
        let temporary = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([1; 16]);
        let engine = Sha256Digest::from_bytes([4; 32]);
        let mut registry = open(temporary.path(), caller)?;
        let selected = project(temporary.path())?;
        let other = ProjectRegistration::new(
            selected.root.clone(),
            selected.data_root.clone(),
            "other".into(),
        )?;
        registry.register(&selected)?;
        registry.register(&other)?;
        registry.configure_watch(selected.binding, true)?;
        let mut request = request(false);
        request.configuration = ProjectConfiguration::Unregister;
        let accepted_key = key(&mut registry, 1)?;
        let job = registry.enqueue_configuration(
            selected.binding,
            caller,
            &accepted_key,
            &request,
            engine,
        )?;
        let pending_key = key(&mut registry, 2)?;
        let pending = registry.enqueue(selected.binding, caller, &pending_key, JobKind::Index)?;
        let stale_key = key(&mut registry, 3)?;
        let ordinary = ConfigurationRequest {
            configuration: ProjectConfiguration::Vectors { enabled: true },
            ..request.clone()
        };
        let ordinary_key = key(&mut registry, 4)?;
        let ordinary_job = registry.enqueue_configuration(
            selected.binding,
            caller,
            &ordinary_key,
            &ordinary,
            engine,
        )?;
        registry.pause(selected.binding, true)?;
        assert!(registry.job_runs_while_paused(job.id)?);
        assert!(!registry.job_runs_while_paused(ordinary_job.id)?);
        assert_eq!(
            registry.next_pending_job(selected.binding)?.unwrap().id,
            job.id
        );
        assert_eq!(registry.pending_projects(32)?, vec![selected.binding]);
        assert_eq!(
            registry.pending_project_page(None, 32)?,
            vec![selected.binding]
        );
        let mut lease = ProjectLease::acquire(&registry.root, selected.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.set_background_pending(&lease, JobKind::Vectors, true)?;
        assert!(
            registry.claim_job_id(&lease, pending.id)?.is_none(),
            "unregister must not resume queued source work"
        );
        registry
            .claim_job_id(&lease, job.id)?
            .context("unregister not claimed")?;
        assert!(
            registry
                .complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })
                .is_err(),
            "an absent preparation result cannot unregister or commit success"
        );
        assert!(registry.is_registered(selected.binding)?);
        assert_eq!(registry.job(job.id)?.state, JobState::Running);
        registry.record_configuration_result(
            &lease,
            job.id,
            &serde_json::json!({"unregistration_requested":true}),
        )?;
        registry.complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })?;
        assert!(!registry.is_registered(selected.binding)?);
        assert!(registry.get(selected.binding).is_err());
        assert!(registry.pause(selected.binding, false).is_err());
        assert!(registry.configure_watch(selected.binding, true).is_err());
        assert_eq!(registry.job(pending.id)?.state, JobState::Canceled);
        assert_eq!(registry.job(ordinary_job.id)?.state, JobState::Canceled);
        assert!(!registry.has_background_work()?);
        let mut retained = vec![selected.binding, other.binding];
        retained.sort();
        assert_eq!(
            registry.bindings()?,
            retained,
            "ownership audits must retain inactive bindings"
        );
        assert_eq!(registry.list(None, None, 32)?.projects.len(), 1);
        let plan_key = key(&mut registry, 5)?;
        let plan = registry.create_maintenance_plan(
            caller,
            &plan_key,
            &airk_core::service_maintenance::RepairRequest {
                selection: airk_core::service_maintenance::RepairSelection::All,
                operation: airk_core::service_maintenance::RepairOperation::Verify,
            },
            0,
            engine,
        )?;
        assert_eq!(
            plan.counts.selected, 1,
            "all-project repair excludes inactive registrations"
        );
        let invalid_plan_key = key(&mut registry, 6)?;
        assert!(
            registry
                .create_maintenance_plan(
                    caller,
                    &invalid_plan_key,
                    &airk_core::service_maintenance::RepairRequest {
                        selection: airk_core::service_maintenance::RepairSelection::Selected(vec![
                            selected.binding
                        ]),
                        operation: airk_core::service_maintenance::RepairOperation::Verify,
                    },
                    0,
                    engine
                )
                .is_err()
        );
        assert!(
            registry
                .enqueue(other.binding, caller, &stale_key, JobKind::Index)
                .is_err()
        );
        assert_eq!(
            registry.configuration_result(job.id)?.unwrap()["unregistered"],
            true
        );
        assert_eq!(
            registry
                .enqueue_configuration(selected.binding, caller, &accepted_key, &request, engine)?
                .id,
            job.id
        );
        drop(lease);
        let mut saved = ServiceRegistry::open_saved(&registry.root)?;
        assert!(!saved.is_registered(selected.binding)?);
        assert_eq!(saved.list(None, None, 32)?.projects[0].registration, other);
        registry.register(&selected)?;
        assert!(!registry.get(selected.binding)?.paused);
        assert_eq!(
            registry
                .enqueue_configuration(selected.binding, caller, &accepted_key, &request, engine)?
                .id,
            job.id
        );
        assert!(
            registry.is_registered(selected.binding)?,
            "replay must preserve explicit re-registration"
        );
        Ok(())
    }

    #[test]
    fn captured_unregister_preparation_recovers_to_one_atomic_terminal_receipt() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([1; 16]);
        let mut registry = open(temporary.path(), caller)?;
        let selected = project(temporary.path())?;
        registry.register(&selected)?;
        let mut request = request(false);
        request.configuration = ProjectConfiguration::Unregister;
        let accepted = key(&mut registry, 1)?;
        let job = registry.enqueue_configuration(
            selected.binding,
            caller,
            &accepted,
            &request,
            Sha256Digest::from_bytes([4; 32]),
        )?;
        let mut lease = ProjectLease::acquire(&registry.root, selected.binding)?;
        registry.admit_worker(&mut lease)?;
        registry
            .claim_job_id(&lease, job.id)?
            .context("unregister not claimed")?;
        registry.record_configuration_result(
            &lease,
            job.id,
            &serde_json::json!({"unregistration_requested":true}),
        )?;
        assert!(
            registry.is_registered(selected.binding)?,
            "preparation alone is not an unregister receipt"
        );
        drop(lease);
        drop(registry);
        let mut registry = open(temporary.path(), Incarnation::from_bytes([9; 16]))?;
        let mut replacement = ProjectLease::acquire(&registry.root, selected.binding)?;
        registry.admit_worker(&mut replacement)?;
        registry.recover_jobs(&replacement)?;
        assert!(!registry.is_registered(selected.binding)?);
        assert_eq!(registry.job(job.id)?.state, JobState::Succeeded);
        let receipt = registry.configuration_result(job.id)?.unwrap();
        assert_eq!(receipt["unregistered"], true);
        assert_eq!(receipt["derived_state_retained"], true);
        assert_eq!(
            registry.bindings()?,
            vec![selected.binding],
            "recovery must retain ownership-audit identity"
        );
        Ok(())
    }
    #[test]
    fn configuration_keys_bind_complete_requests_and_pending_join_preserves_engine() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([1; 16]);
        let engine = Sha256Digest::from_bytes([4; 32]);
        let mut registry = open(temporary.path(), caller)?;
        let project = project(temporary.path())?;
        registry.register(&project)?;
        let first = key(&mut registry, 1)?;
        let job = registry.enqueue_configuration(
            project.binding,
            caller,
            &first,
            &request(false),
            engine,
        )?;
        assert_eq!(
            registry
                .enqueue_configuration(
                    project.binding,
                    caller,
                    &first,
                    &request(false),
                    Sha256Digest::from_bytes([9; 32])
                )?
                .id,
            job.id
        );
        assert_eq!(registry.configuration_work(job.id)?.unwrap().engine, engine);
        assert!(
            registry
                .enqueue_configuration(project.binding, caller, &first, &request(true), engine)
                .unwrap_err()
                .is::<JobError>()
        );
        let second = key(&mut registry, 2)?;
        assert_eq!(
            registry
                .enqueue_configuration(project.binding, caller, &second, &request(false), engine)?
                .id,
            job.id
        );
        let third = key(&mut registry, 3)?;
        assert_ne!(
            registry
                .enqueue_configuration(project.binding, caller, &third, &request(true), engine)?
                .id,
            job.id
        );
        let fourth = key(&mut registry, 4)?;
        assert!(
            registry
                .enqueue(project.binding, caller, &fourth, JobKind::Configure)
                .is_err()
        );
        drop(registry);
        let mut registry = open(temporary.path(), Incarnation::from_bytes([8; 16]))?;
        assert_eq!(
            registry
                .enqueue_configuration(project.binding, caller, &first, &request(false), engine)?
                .id,
            job.id
        );
        Ok(())
    }
    #[test]
    fn canonical_policy_order_does_not_change_an_accepted_configuration_request() -> Result<()> {
        use airk_core::vector_selection::{VectorExclusionRule, VectorSelectionPolicy};
        let temporary = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([1; 16]);
        let mut registry = open(temporary.path(), caller)?;
        let project = project(temporary.path())?;
        registry.register(&project)?;
        let first = VectorExclusionRule {
            id: "zeta".into(),
            path_prefix: Some("src/zeta/".into()),
            ..Default::default()
        };
        let second = VectorExclusionRule {
            id: "alpha".into(),
            path_prefix: Some("src/alpha/".into()),
            ..Default::default()
        };
        let mut request = request(false);
        request.configuration = ProjectConfiguration::VectorSelection {
            policy: VectorSelectionPolicy {
                version: 1,
                exclusions: vec![first, second],
                inclusions: vec![],
                agent_defaults: false,
            },
        };
        let key = key(&mut registry, 1)?;
        let engine = Sha256Digest::from_bytes([4; 32]);
        let accepted =
            registry.enqueue_configuration(project.binding, caller, &key, &request, engine)?;
        assert_eq!(
            registry
                .enqueue_configuration(project.binding, caller, &key, &request, engine)?
                .id,
            accepted.id
        );
        if let ProjectConfiguration::VectorSelection { policy } = &mut request.configuration {
            policy.exclusions.reverse();
        }
        assert_eq!(
            registry
                .enqueue_configuration(project.binding, caller, &key, &request, engine)?
                .id,
            accepted.id
        );
        Ok(())
    }
    #[test]
    fn result_space_is_reserved_before_work_can_start() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([1; 16]);
        let mut registry = open(temporary.path(), caller)?;
        let project = project(temporary.path())?;
        registry.register(&project)?;
        let key = key(&mut registry, 1)?;
        let job = registry.enqueue_configuration(
            project.binding,
            caller,
            &key,
            &request(false),
            Sha256Digest::from_bytes([4; 32]),
        )?;
        assert!(registry.configuration_result(job.id)?.is_none());
        let reserved: i64 = registry.connection.query_row(
            "SELECT length(result) FROM service_configuration WHERE job=?1",
            [job.id.to_string()],
            |row| row.get(0),
        )?;
        assert_eq!(reserved, MAX_CONFIGURATION_BYTES as i64 + 8);
        let mut lease = ProjectLease::acquire(&temporary.path().join("control"), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.claim_job_id(&lease, job.id)?.unwrap();
        let pages: i64 = registry
            .connection
            .pragma_query_value(None, "page_count", |row| row.get(0))?;
        registry
            .connection
            .pragma_update(None, "max_page_count", pages)?;
        registry.record_configuration_result(
            &lease,
            job.id,
            &serde_json::json!({"metadata":"x".repeat(16_000)}),
        )?;
        assert!(registry.configuration_result(job.id)?.is_some());
        Ok(())
    }
    #[test]
    fn interrupted_configuration_recovers_a_result_or_fails_unknown_without_replaying() -> Result<()>
    {
        for completed in [false, true] {
            let temporary = tempfile::tempdir()?;
            let caller = Incarnation::from_bytes([1; 16]);
            let mut registry = open(temporary.path(), caller)?;
            let project = project(temporary.path())?;
            registry.register(&project)?;
            let key = key(&mut registry, 1)?;
            let job = registry.enqueue_configuration(
                project.binding,
                caller,
                &key,
                &request(false),
                Sha256Digest::from_bytes([4; 32]),
            )?;
            let mut owner =
                ProjectLease::acquire(&temporary.path().join("control"), project.binding)?;
            registry.admit_worker(&mut owner)?;
            registry.claim_job_id(&owner, job.id)?.unwrap();
            if completed {
                registry.record_configuration_result(
                    &owner,
                    job.id,
                    &serde_json::json!({"enabled":false}),
                )?;
            }
            drop(owner);
            let mut replacement =
                ProjectLease::acquire(&temporary.path().join("control"), project.binding)?;
            registry.admit_worker(&mut replacement)?;
            registry.recover_jobs(&replacement)?;
            assert_eq!(
                registry.job(job.id)?.state,
                if completed {
                    JobState::Succeeded
                } else {
                    JobState::Failed
                }
            );
            assert!(registry.next_pending_job(project.binding)?.is_none());
            assert_eq!(registry.configuration_result(job.id)?.is_some(), completed);
            if !completed {
                assert_eq!(
                    serde_json::to_value(registry.job(job.id)?.outcome)?["code"],
                    "configuration_outcome_unknown_reinspect"
                );
            }
            assert!(
                registry
                    .record_configuration_result(&replacement, job.id, &serde_json::json!({}))
                    .is_err()
            );
        }
        Ok(())
    }
    #[test]
    fn version_three_migration_preserves_requests_results_and_reserved_capacity() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([1; 16]);
        let mut registry = open(temporary.path(), caller)?;
        let project = project(temporary.path())?;
        registry.register(&project)?;
        let accepted_key = key(&mut registry, 1)?;
        let engine = Sha256Digest::from_bytes([4; 32]);
        let accepted = registry.enqueue_configuration(
            project.binding,
            caller,
            &accepted_key,
            &request(false),
            engine,
        )?;
        let mut lease = ProjectLease::acquire(&temporary.path().join("control"), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.claim_job_id(&lease, accepted.id)?.unwrap();
        let result = serde_json::json!({"enabled":false});
        registry.record_configuration_result(&lease, accepted.id, &result)?;
        let pending_key = key(&mut registry, 2)?;
        let pending = registry.enqueue_configuration(
            project.binding,
            caller,
            &pending_key,
            &request(true),
            engine,
        )?;
        let namespace = registry.saved_request_epoch()?;
        registry.connection.execute_batch(
            "ALTER TABLE service_configuration RENAME TO configuration_v4;
             CREATE TABLE service_configuration(
                 job TEXT PRIMARY KEY REFERENCES service_jobs(id) ON DELETE CASCADE,
                 payload BLOB NOT NULL CHECK(length(payload)<=16640),
                 result BLOB CHECK(result IS NULL OR length(result)<=16384)) WITHOUT ROWID;
             INSERT INTO service_configuration SELECT job,payload,NULL FROM configuration_v4;
             DROP TABLE configuration_v4;
             DROP TABLE service_watch_traversal; ALTER TABLE projects DROP COLUMN cleanup_guard; ALTER TABLE projects DROP COLUMN registered;
             PRAGMA user_version=3;",
        )?;
        registry.connection.execute(
            "UPDATE service_configuration SET result=?1 WHERE job=?2",
            params![serde_json::to_vec(&result)?, accepted.id.to_string()],
        )?;
        drop(lease);
        drop(registry);
        let mut registry = open(temporary.path(), Incarnation::from_bytes([9; 16]))?;
        assert_eq!(registry.saved_request_epoch()?, namespace);
        assert_eq!(registry.configuration_result(accepted.id)?, Some(result));
        assert!(registry.configuration_result(pending.id)?.is_none());
        assert_eq!(
            registry.configuration_work(pending.id)?.unwrap().engine,
            engine
        );
        assert_eq!(
            registry
                .enqueue_configuration(
                    project.binding,
                    caller,
                    &accepted_key,
                    &request(false),
                    engine,
                )?
                .id,
            accepted.id
        );
        let reserved: i64 = registry.connection.query_row(
            "SELECT min(length(result)) FROM service_configuration",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(reserved, 16392);
        let mut lease = ProjectLease::acquire(&temporary.path().join("control"), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.recover_jobs(&lease)?;
        assert_eq!(registry.job(accepted.id)?.state, JobState::Succeeded);
        assert_eq!(registry.job(pending.id)?.state, JobState::Pending);
        Ok(())
    }
    #[test]
    fn damaged_configuration_result_is_preserved_and_never_replayed() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([1; 16]);
        let mut registry = open(temporary.path(), caller)?;
        let project = project(temporary.path())?;
        registry.register(&project)?;
        let accepted_key = key(&mut registry, 1)?;
        let job = registry.enqueue_configuration(
            project.binding,
            caller,
            &accepted_key,
            &request(false),
            Sha256Digest::from_bytes([4; 32]),
        )?;
        let mut lease = ProjectLease::acquire(&temporary.path().join("control"), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.claim_job_id(&lease, job.id)?.unwrap();
        // A valid marker with invalid JSON is not proof of a completed operation.
        write_result(&registry.connection, job.id, b"{")?;
        let before: Vec<u8> = registry.connection.query_row(
            "SELECT result FROM service_configuration WHERE job=?1",
            [job.id.to_string()],
            |row| row.get(0),
        )?;
        drop(lease);
        let mut lease = ProjectLease::acquire(&temporary.path().join("control"), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.recover_jobs(&lease)?;
        assert_eq!(registry.job(job.id)?.state, JobState::Failed);
        assert_eq!(
            serde_json::to_value(registry.job(job.id)?.outcome)?["code"],
            "configuration_outcome_unknown_reinspect"
        );
        assert!(registry.next_pending_job(project.binding)?.is_none());
        assert!(registry.configuration_result(job.id).is_err());
        let after: Vec<u8> = registry.connection.query_row(
            "SELECT result FROM service_configuration WHERE job=?1",
            [job.id.to_string()],
            |row| row.get(0),
        )?;
        assert_eq!(after, before);
        Ok(())
    }
    #[test]
    fn metadata_pressure_retires_terminal_receipts_but_preserves_pending_work() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([1; 16]);
        let mut registry = open(temporary.path(), caller)?;
        let project = project(temporary.path())?;
        registry.register(&project)?;
        let first_key = key(&mut registry, 1)?;
        let first = registry.enqueue_configuration(
            project.binding,
            caller,
            &first_key,
            &request(false),
            Sha256Digest::from_bytes([4; 32]),
        )?;
        let mut lease = ProjectLease::acquire(&temporary.path().join("control"), project.binding)?;
        registry.admit_worker(&mut lease)?;
        registry.claim_job_id(&lease, first.id)?.unwrap();
        registry.record_configuration_result(
            &lease,
            first.id,
            &serde_json::json!({"enabled":false}),
        )?;
        registry.complete_job(
            &lease,
            first.id,
            airk_core::service::JobOutcome::Succeeded { revision: None },
        )?;
        let next_key = key(&mut registry, 2)?;
        let pending = registry.enqueue_configuration(
            project.binding,
            caller,
            &next_key,
            &request(true),
            Sha256Digest::from_bytes([4; 32]),
        )?;
        let pages: i64 = registry
            .connection
            .pragma_query_value(None, "page_count", |row| row.get(0))?;
        registry
            .connection
            .pragma_update(None, "max_page_count", pages)?;
        let (_, epoch) = registry.request_epoch()?;
        assert!(epoch > first_key.epoch);
        assert!(registry.job(first.id).is_err());
        assert_eq!(registry.job(pending.id)?.state, JobState::Pending);
        assert_eq!(
            registry
                .enqueue_configuration(
                    project.binding,
                    caller,
                    &next_key,
                    &request(true),
                    Sha256Digest::from_bytes([4; 32]),
                )?
                .id,
            pending.id
        );
        assert!(
            registry
                .enqueue_configuration(
                    project.binding,
                    caller,
                    &first_key,
                    &request(false),
                    Sha256Digest::from_bytes([4; 32]),
                )
                .unwrap_err()
                .is::<JobError>()
        );
        Ok(())
    }
    #[test]
    fn version_two_upgrade_preserves_authority_and_missing_completed_table_is_never_recreated()
    -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let caller = Incarnation::from_bytes([1; 16]);
        let mut registry = open(temporary.path(), caller)?;
        let project = project(temporary.path())?;
        registry.register(&project)?;
        let key = key(&mut registry, 1)?;
        let job = registry.enqueue(project.binding, caller, &key, JobKind::Index)?;
        registry
            .connection
            .execute_batch("DROP TABLE service_configuration; DROP TABLE service_watch_traversal; ALTER TABLE projects DROP COLUMN cleanup_guard; ALTER TABLE projects DROP COLUMN registered; PRAGMA user_version=2;")?;
        drop(registry);
        let mut registry = open(temporary.path(), Incarnation::from_bytes([9; 16]))?;
        assert_eq!(registry.incarnation()?, caller);
        assert_eq!(
            registry
                .enqueue(project.binding, caller, &key, JobKind::Index)?
                .id,
            job.id
        );
        registry
            .connection
            .execute_batch("DROP TABLE service_configuration;")?;
        drop(registry);
        let database = temporary.path().join("control/service-1.4.sqlite3");
        let before = std::fs::read(&database)?;
        assert!(open(temporary.path(), caller).is_err());
        assert_eq!(std::fs::read(database)?, before);
        Ok(())
    }
}
