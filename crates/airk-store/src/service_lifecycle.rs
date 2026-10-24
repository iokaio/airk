// SPDX-License-Identifier: Apache-2.0
//! Registry-bound lifecycle receipts and persistent explicit-stop policy.
use super::ServiceRegistry;
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    service_lifecycle::{LifecycleAction, LifecyclePhase, LifecycleRecord, LifecycleRequest},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

pub(super) const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS service_lifecycle_state(
        id INTEGER PRIMARY KEY CHECK(id=1), stopped INTEGER NOT NULL CHECK(stopped IN(0,1)),
        instance TEXT CHECK(instance IS NULL OR length(instance)=32));
    INSERT OR IGNORE INTO service_lifecycle_state VALUES(1,0,NULL);
    CREATE TABLE IF NOT EXISTS service_lifecycle(
        seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
        request BLOB NOT NULL CHECK(length(request)<=2048),
        phase TEXT NOT NULL CHECK(phase IN('draining','starting','complete','abandoned')));
    CREATE UNIQUE INDEX IF NOT EXISTS service_lifecycle_active ON service_lifecycle((1)) WHERE phase NOT IN('complete','abandoned');";

fn id(request: &LifecycleRequest) -> Sha256Digest {
    framed_digest(
        b"airk/lifecycle-request/1",
        &[
            request.key.namespace.as_bytes(),
            &request.key.epoch.to_be_bytes(),
            request.key.nonce.as_bytes(),
            request.caller.as_bytes(),
        ],
    )
}
fn decode(id: Sha256Digest, bytes: Vec<u8>, phase: String) -> Result<LifecycleRecord> {
    ensure!(bytes.len() <= 2048, "lifecycle record exceeds budget");
    let request: LifecycleRequest = serde_json::from_slice(&bytes)?;
    ensure!(
        self::id(&request) == id,
        "lifecycle record identity mismatch"
    );
    let phase = match phase.as_str() {
        "draining" => LifecyclePhase::Draining,
        "starting" => LifecyclePhase::Starting,
        "complete" => LifecyclePhase::Complete,
        "abandoned" => LifecyclePhase::Abandoned,
        _ => anyhow::bail!("invalid lifecycle phase"),
    };
    Ok(LifecycleRecord { id, request, phase })
}
impl ServiceRegistry {
    pub fn lifecycle_by_id(&self, identity: Sha256Digest) -> Result<Option<LifecycleRecord>> {
        self.connection
            .query_row(
                "SELECT request,phase FROM service_lifecycle WHERE id=?1",
                [identity.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .map(|(bytes, phase)| decode(identity, bytes, phase))
            .transpose()
    }
    pub fn record_service_instance(
        &mut self,
        lease: &crate::service::ServiceLease,
        instance: airk_core::identity::Incarnation,
    ) -> Result<()> {
        ensure!(
            lease.root() == self.root,
            "service instance ownership differs"
        );
        ensure!(
            !self.service_explicitly_stopped()?,
            "service_explicitly_stopped"
        );
        self.connection.execute(
            "UPDATE service_lifecycle_state SET instance=?1 WHERE id=1",
            [instance.to_string()],
        )?;
        Ok(())
    }
    pub fn recorded_service_instance(&self) -> Result<Option<airk_core::identity::Incarnation>> {
        let value: Option<String> = self.connection.query_row(
            "SELECT instance FROM service_lifecycle_state WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        value.map(|text| text.parse()).transpose()
    }
    pub fn lifecycle_record(&self, request: &LifecycleRequest) -> Result<Option<LifecycleRecord>> {
        let identity = id(request);
        let row = self
            .connection
            .query_row(
                "SELECT request,phase FROM service_lifecycle WHERE id=?1",
                [identity.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(bytes, phase)| {
            let record = decode(identity, bytes, phase)?;
            ensure!(record.request == *request, "lifecycle_request_key_conflict");
            Ok(record)
        })
        .transpose()
    }
    pub fn pending_lifecycle(&self) -> Result<Option<LifecycleRecord>> {
        let row = self
            .connection
            .query_row(
                "SELECT id,request,phase FROM service_lifecycle WHERE phase NOT IN('complete','abandoned')",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        row.map(|(identity, bytes, phase)| decode(identity.parse()?, bytes, phase))
            .transpose()
    }
    /// The caller holds the installation lock and has checked the concrete native instance.
    pub fn begin_lifecycle(&mut self, request: &LifecycleRequest) -> Result<LifecycleRecord> {
        let identity = id(request);
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some((bytes, phase)) = transaction
            .query_row(
                "SELECT request,phase FROM service_lifecycle WHERE id=?1",
                [identity.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
        {
            let record = decode(identity, bytes, phase)?;
            ensure!(record.request == *request, "lifecycle_request_key_conflict");
            return Ok(record);
        }
        let (namespace, epoch): (String, i64) = transaction.query_row(
            "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        ensure!(
            namespace == request.key.namespace.to_string()
                && u64::try_from(epoch)? == request.key.epoch,
            "lifecycle_request_key_expired"
        );
        let active: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_lifecycle WHERE phase NOT IN('complete','abandoned'))",
            [],
            |row| row.get(0),
        )?;
        ensure!(!active, "another_lifecycle_request_is_pending");
        let count: i64 =
            transaction.query_row("SELECT count(*) FROM service_lifecycle", [], |row| {
                row.get(0)
            })?;
        if count >= 64 {
            transaction.execute("DELETE FROM service_lifecycle WHERE seq IN(SELECT seq FROM service_lifecycle WHERE phase IN('complete','abandoned') ORDER BY seq LIMIT 32)",[])?;
            transaction.execute(
                "UPDATE service_meta SET request_epoch=?1 WHERE id=1",
                [epoch
                    .checked_add(1)
                    .context("service request epoch exhausted")?],
            )?;
        }
        let bytes = serde_json::to_vec(request)?;
        ensure!(bytes.len() <= 2048, "lifecycle request exceeds budget");
        transaction.execute(
            "INSERT INTO service_lifecycle(id,request,phase) VALUES(?1,?2,'draining')",
            params![identity.to_string(), bytes],
        )?;
        transaction.execute(
            "UPDATE service_lifecycle_state SET stopped=1 WHERE id=1",
            [],
        )?;
        transaction.commit()?;
        Ok(LifecycleRecord {
            id: identity,
            request: request.clone(),
            phase: LifecyclePhase::Draining,
        })
    }
    pub fn advance_lifecycle(
        &mut self,
        record: &LifecycleRecord,
        next: LifecyclePhase,
    ) -> Result<LifecycleRecord> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (bytes, phase) = transaction.query_row(
            "SELECT request,phase FROM service_lifecycle WHERE id=?1",
            [record.id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let current = decode(record.id, bytes, phase)?;
        ensure!(
            current.request == record.request,
            "lifecycle request changed"
        );
        if current.phase == next {
            return Ok(current);
        }
        ensure!(current.phase == record.phase, "lifecycle phase changed");
        let valid = matches!(
            (current.request.action, current.phase, next),
            (
                LifecycleAction::Stop,
                LifecyclePhase::Draining,
                LifecyclePhase::Complete
            ) | (
                LifecycleAction::Restart,
                LifecyclePhase::Draining,
                LifecyclePhase::Starting
            ) | (
                LifecycleAction::Restart,
                LifecyclePhase::Starting,
                LifecyclePhase::Complete
            )
        );
        let abandon = next == LifecyclePhase::Abandoned
            && matches!(
                current.phase,
                LifecyclePhase::Draining | LifecyclePhase::Starting
            );
        ensure!(valid || abandon, "invalid lifecycle transition");
        let phase = match next {
            LifecyclePhase::Starting => "starting",
            LifecyclePhase::Abandoned => "abandoned",
            _ => "complete",
        };
        transaction.execute(
            "UPDATE service_lifecycle SET phase=?1 WHERE id=?2",
            params![phase, record.id.to_string()],
        )?;
        if abandon {
            transaction.execute(
                "UPDATE service_lifecycle_state SET stopped=1 WHERE id=1",
                [],
            )?;
        } else if current.request.action == LifecycleAction::Restart {
            transaction.execute(
                "UPDATE service_lifecycle_state SET stopped=0 WHERE id=1",
                [],
            )?;
        }
        transaction.commit()?;
        Ok(LifecycleRecord {
            phase: next,
            ..current
        })
    }
    pub fn service_explicitly_stopped(&self) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT stopped FROM service_lifecycle_state WHERE id=1",
            [],
            |row| row.get(0),
        )?)
    }
    pub fn clear_service_stop(&mut self) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_lifecycle WHERE phase NOT IN('complete','abandoned'))",
            [],
            |row| row.get(0),
        )?;
        ensure!(!active, "resume pending lifecycle before explicit start");
        transaction.execute(
            "UPDATE service_lifecycle_state SET stopped=0 WHERE id=1",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{identity::Incarnation, service::RequestKey};
    fn request(
        registry: &mut ServiceRegistry,
        nonce: u8,
        action: LifecycleAction,
    ) -> Result<LifecycleRequest> {
        let (namespace, epoch) = registry.request_epoch()?;
        Ok(LifecycleRequest {
            caller: Incarnation::from_bytes([2; 16]),
            key: RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes([nonce; 16]),
            },
            action,
            expected_instance: Incarnation::from_bytes([3; 16]),
            runtime: Sha256Digest::from_bytes([4; 32]),
            manager: false,
        })
    }
    #[test]
    fn accepted_stop_survives_reopen_replays_exactly_and_blocks_automatic_start() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let instance = Incarnation::from_bytes([1; 16]);
        let mut registry = ServiceRegistry::open(directory.path(), instance)?;
        let input = request(&mut registry, 1, LifecycleAction::Stop)?;
        let record = registry.begin_lifecycle(&input)?;
        assert!(registry.service_explicitly_stopped()?);
        assert_eq!(registry.begin_lifecycle(&input)?.id, record.id);
        let mut conflicting = input.clone();
        conflicting.action = LifecycleAction::Restart;
        assert!(registry.begin_lifecycle(&conflicting).is_err());
        assert!(
            registry
                .advance_lifecycle(&record, LifecyclePhase::Starting)
                .is_err()
        );
        assert!(registry.clear_service_stop().is_err());
        drop(registry);
        let mut registry =
            ServiceRegistry::open(directory.path(), Incarnation::from_bytes([9; 16]))?;
        let pending = registry.pending_lifecycle()?.unwrap();
        assert_eq!(pending.id, record.id);
        let completed = registry.advance_lifecycle(&pending, LifecyclePhase::Complete)?;
        assert_eq!(
            registry.begin_lifecycle(&input)?.phase,
            LifecyclePhase::Complete
        );
        assert_eq!(
            registry
                .advance_lifecycle(&completed, LifecyclePhase::Complete)?
                .id,
            record.id
        );
        assert!(registry.service_explicitly_stopped()?);
        registry.clear_service_stop()?;
        assert!(!registry.service_explicitly_stopped()?);
        Ok(())
    }
    #[test]
    fn restart_phase_recovery_preserves_owner_and_prevents_overlapping_requests() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry =
            ServiceRegistry::open(directory.path(), Incarnation::from_bytes([1; 16]))?;
        let input = request(&mut registry, 1, LifecycleAction::Restart)?;
        let other = request(&mut registry, 2, LifecycleAction::Stop)?;
        let record = registry.begin_lifecycle(&input)?;
        assert!(registry.begin_lifecycle(&other).is_err());
        assert!(
            registry
                .advance_lifecycle(&record, LifecyclePhase::Complete)
                .is_err()
        );
        let starting = registry.advance_lifecycle(&record, LifecyclePhase::Starting)?;
        assert!(!registry.service_explicitly_stopped()?);
        drop(registry);
        let mut registry =
            ServiceRegistry::open(directory.path(), Incarnation::from_bytes([2; 16]))?;
        assert_eq!(
            registry.pending_lifecycle()?.unwrap().phase,
            LifecyclePhase::Starting
        );
        registry.advance_lifecycle(&starting, LifecyclePhase::Complete)?;
        registry.begin_lifecycle(&other)?;
        Ok(())
    }
    #[test]
    fn retired_lifecycle_keys_expire_without_repeating_old_work() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry =
            ServiceRegistry::open(directory.path(), Incarnation::from_bytes([1; 16]))?;
        let first = request(&mut registry, 0, LifecycleAction::Stop)?;
        for nonce in 0..70 {
            let input = request(&mut registry, nonce, LifecycleAction::Stop)?;
            let record = registry.begin_lifecycle(&input)?;
            registry.advance_lifecycle(&record, LifecyclePhase::Complete)?;
        }
        assert!(registry.lifecycle_record(&first)?.is_none());
        assert!(registry.begin_lifecycle(&first).is_err());
        let count: i64 =
            registry
                .connection
                .query_row("SELECT count(*) FROM service_lifecycle", [], |row| {
                    row.get(0)
                })?;
        assert!(count <= 64);
        Ok(())
    }
    #[test]
    fn missing_lifecycle_table_or_policy_row_is_not_recreated() -> Result<()> {
        for sql in [
            "DROP TABLE service_lifecycle",
            "DELETE FROM service_lifecycle_state",
        ] {
            let directory = tempfile::tempdir()?;
            let instance = Incarnation::from_bytes([1; 16]);
            let registry = ServiceRegistry::open(directory.path(), instance)?;
            registry.connection.execute_batch(sql)?;
            drop(registry);
            assert!(ServiceRegistry::open(directory.path(), instance).is_err());
        }
        Ok(())
    }
    #[test]
    fn abandoning_recovery_preserves_stop_policy_and_never_replays_the_request() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut registry =
            ServiceRegistry::open(directory.path(), Incarnation::from_bytes([1; 16]))?;
        let input = request(&mut registry, 1, LifecycleAction::Restart)?;
        let record = registry.begin_lifecycle(&input)?;
        let starting = registry.advance_lifecycle(&record, LifecyclePhase::Starting)?;
        let abandoned = registry.advance_lifecycle(&starting, LifecyclePhase::Abandoned)?;
        assert!(registry.service_explicitly_stopped()?);
        assert!(registry.pending_lifecycle()?.is_none());
        assert_eq!(
            registry.begin_lifecycle(&input)?.phase,
            LifecyclePhase::Abandoned
        );
        assert!(
            registry
                .advance_lifecycle(&abandoned, LifecyclePhase::Starting)
                .is_err()
        );
        registry.clear_service_stop()?;
        assert!(!registry.service_explicitly_stopped()?);
        Ok(())
    }
    #[test]
    fn version_one_upgrade_preserves_accepted_keys_projects_and_fences() -> Result<()> {
        use airk_core::service::{JobKind, ProjectRegistration};
        let directory = tempfile::tempdir()?;
        let first = Incarnation::from_bytes([1; 16]);
        let mut registry = ServiceRegistry::open(directory.path(), first)?;
        let project = ProjectRegistration::new(
            "/original/migration".into(),
            "/derived/migration".into(),
            "migration".into(),
        )?;
        registry.register(&project)?;
        let mut lease = crate::service::ProjectLease::acquire(directory.path(), project.binding)?;
        let fence = registry.admit_worker(&mut lease)?;
        drop(lease);
        let input = request(&mut registry, 1, LifecycleAction::Stop)?;
        let job = registry.enqueue(project.binding, input.caller, &input.key, JobKind::Index)?;
        registry.connection.execute_batch("DROP TABLE service_lifecycle; DROP TABLE service_lifecycle_state; PRAGMA user_version=1;")?;
        drop(registry);
        let mut registry =
            ServiceRegistry::open(directory.path(), Incarnation::from_bytes([9; 16]))?;
        assert_eq!(registry.incarnation()?, first);
        assert_eq!(registry.get(project.binding)?.fencing_generation, fence);
        assert_eq!(
            registry
                .enqueue(project.binding, input.caller, &input.key, JobKind::Index)?
                .id,
            job.id
        );
        assert!(!registry.service_explicitly_stopped()?);
        Ok(())
    }
}
