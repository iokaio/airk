// SPDX-License-Identifier: Apache-2.0
//! Fenced cached inspection metadata; reads never open a project index.
use super::ServiceRegistry;
use crate::service::ProjectLease;
use airk_core::{
    identity::Sha256Digest,
    service::{JobError, JobKind, JobState, ProjectBinding, WorkerIdentity},
    service_inspection::{InspectionObservation, InspectionSnapshot, MAX_INSPECTION_BYTES},
};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

const MAX_RECORD: usize = MAX_INSPECTION_BYTES + 1024;
pub(super) const SCHEMA:&str="CREATE TABLE IF NOT EXISTS service_inspections(
    project TEXT PRIMARY KEY REFERENCES projects(binding),payload BLOB NOT NULL CHECK(length(payload)<=9216)) WITHOUT ROWID;";

impl ServiceRegistry {
    pub fn record_inspection(
        &mut self,
        lease: &ProjectLease,
        worker: WorkerIdentity,
        job: Sha256Digest,
        observed_unix_ms: u64,
        snapshot: &InspectionSnapshot,
    ) -> Result<()> {
        snapshot.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let epoch = super::jobs::worker_epoch(&tx, &self.root, lease)?;
        ensure!(
            worker.project == lease.identity().0
                && worker.fencing_generation == epoch
                && snapshot.project == worker.project,
            JobError::StaleWorker
        );
        let record = super::jobs::record(&tx, job)?;
        ensure!(
            record.project == worker.project
                && matches!(record.kind, JobKind::Inspect | JobKind::PlanInspect)
                && record.state == JobState::Running
                && record.fencing_generation == epoch,
            JobError::StaleWorker
        );
        let observation = InspectionObservation {
            worker,
            current_fencing_generation: epoch,
            job,
            observed_unix_ms,
            snapshot: snapshot.clone(),
        };
        let bytes = serde_json::to_vec(&observation)?;
        ensure!(
            bytes.len() <= MAX_RECORD,
            "inspection record exceeds byte bound"
        );
        tx.execute(
            "INSERT INTO service_inspections(project,payload) VALUES(?1,?2)
            ON CONFLICT(project) DO UPDATE SET payload=excluded.payload",
            params![worker.project.to_string(), bytes],
        )?;
        super::maintenance::record_inspection(&tx, &observation)?;
        tx.commit()?;
        Ok(())
    }
    pub fn inspection(&self, project: ProjectBinding) -> Result<Option<InspectionObservation>> {
        let tx = self.connection.unchecked_transaction()?;
        let epoch: i64 = tx.query_row(
            "SELECT epoch FROM projects WHERE binding=?1",
            [project.to_string()],
            |row| row.get(0),
        )?;
        let bytes: Option<Vec<u8>> = tx
            .query_row(
                "SELECT substr(payload,1,9217) FROM service_inspections WHERE project=?1",
                [project.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        bytes
            .map(|bytes| {
                ensure!(
                    bytes.len() <= MAX_RECORD,
                    "inspection record exceeds byte bound"
                );
                let mut observation: InspectionObservation = serde_json::from_slice(&bytes)?;
                observation.snapshot.validate()?;
                ensure!(
                    observation.snapshot.project == project
                        && observation.worker.project == project,
                    "foreign cached inspection"
                );
                observation.current_fencing_generation = u64::try_from(epoch)?;
                ensure!(
                    observation.worker.fencing_generation > 0
                        && observation.worker.fencing_generation
                            <= observation.current_fencing_generation,
                    "cached inspection has an impossible fencing generation"
                );
                Ok(observation)
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::Incarnation,
        service::ProjectRegistration,
        service_inspection::{IndexState, SourceState},
    };

    #[test]
    fn cached_inspection_is_fenced_survives_restart_and_does_not_resume_paused_source_work()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let instance = Incarnation::from_bytes([3; 16]);
        let mut registry = ServiceRegistry::open(owner.path(), instance)?;
        let project =
            ProjectRegistration::new("/source".into(), "/state".into(), "fixture".into())?;
        registry.register(&project)?;
        assert!(registry.inspection(project.binding)?.is_none());
        registry.pause(project.binding, true)?;
        let source = registry.enqueue_internal(
            project.binding,
            instance,
            JobKind::Index,
            Incarnation::from_bytes([4; 16]),
        )?;
        let inspect = registry.enqueue_internal(
            project.binding,
            instance,
            JobKind::Inspect,
            Incarnation::from_bytes([5; 16]),
        )?;
        assert_eq!(
            registry.next_pending_job(project.binding)?.unwrap().id,
            inspect.id
        );
        assert_eq!(registry.pending_projects(32)?, vec![project.binding]);
        assert_eq!(
            registry.pending_project_page(None, 32)?,
            vec![project.binding]
        );
        let mut lease = ProjectLease::acquire(owner.path(), project.binding)?;
        let epoch = registry.admit_worker(&mut lease)?;
        let worker = WorkerIdentity {
            service: instance,
            worker: Incarnation::from_bytes([6; 16]),
            project: project.binding,
            fencing_generation: epoch,
        };
        let snapshot = InspectionSnapshot {
            project: project.binding,
            incarnation: Incarnation::from_bytes([7; 16]),
            publication: None,
            settings_digest: Some(Sha256Digest::from_bytes([8; 32])),
            index_state: IndexState::Empty,
            source_state: SourceState::Available,
            recovery_required: Some(false),
            indexed_files: Some(0),
            indexed_vectors: Some(0),
            vectors_enabled: Some(false),
            semantic: None,
            discovery: None,
            failure: None,
            storage: None,
        };
        assert!(
            registry
                .record_inspection(&lease, worker, inspect.id, 1000, &snapshot)
                .is_err()
        );
        assert!(registry.claim_job_id(&lease, source.id)?.is_none());
        assert_eq!(registry.claim_job(&lease)?.unwrap().id, inspect.id);
        registry.record_inspection(&lease, worker, inspect.id, 1000, &snapshot)?;
        let mut foreign = snapshot.clone();
        foreign.project = Sha256Digest::from_bytes([9; 32]);
        assert!(
            registry
                .record_inspection(&lease, worker, inspect.id, 1001, &foreign)
                .is_err()
        );
        registry.complete_job(
            &lease,
            inspect.id,
            airk_core::service::JobOutcome::Succeeded { revision: None },
        )?;
        assert!(registry.get(project.binding)?.paused);
        assert_eq!(registry.job(source.id)?.state, JobState::Pending);
        assert!(registry.claim_job(&lease)?.is_none());
        assert!(
            registry
                .record_inspection(&lease, worker, inspect.id, 1002, &snapshot)
                .is_err()
        );
        drop(registry);
        drop(lease);
        let mut registry = ServiceRegistry::open(owner.path(), Incarnation::from_bytes([10; 16]))?;
        let cached = registry.inspection(project.binding)?.unwrap();
        assert_eq!(cached.job, inspect.id);
        assert_eq!(cached.observed_unix_ms, 1000);
        let mut next = ProjectLease::acquire(owner.path(), project.binding)?;
        let epoch = registry.admit_worker(&mut next)?;
        let stale = registry.inspection(project.binding)?.unwrap();
        assert_eq!(stale.current_fencing_generation, epoch);
        assert!(stale.worker.fencing_generation < epoch);
        assert!(
            registry
                .record_inspection(&next, worker, inspect.id, 1003, &snapshot)
                .is_err()
        );
        let mut invalid = serde_json::to_value(snapshot)?;
        invalid["failure"] = serde_json::json!("private/path or source text");
        assert!(serde_json::from_value::<InspectionSnapshot>(invalid).is_err());
        assert_eq!(
            registry
                .inspection(project.binding)?
                .unwrap()
                .observed_unix_ms,
            1000
        );
        let original: Vec<u8> = registry.connection.query_row(
            "SELECT payload FROM service_inspections WHERE project=?1",
            [project.binding.to_string()],
            |row| row.get(0),
        )?;
        let mut future: serde_json::Value = serde_json::from_slice(&original)?;
        future["worker"]["fencing_generation"] = serde_json::json!(epoch + 1);
        let changed = serde_json::to_vec(&future)?;
        registry.connection.execute(
            "UPDATE service_inspections SET payload=?1 WHERE project=?2",
            params![changed, project.binding.to_string()],
        )?;
        assert!(registry.inspection(project.binding).is_err());
        let preserved: Vec<u8> = registry.connection.query_row(
            "SELECT payload FROM service_inspections WHERE project=?1",
            [project.binding.to_string()],
            |row| row.get(0),
        )?;
        assert_eq!(
            preserved, changed,
            "invalid cached metadata was rewritten by a read"
        );
        Ok(())
    }
}
