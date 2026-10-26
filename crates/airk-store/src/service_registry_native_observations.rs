// SPDX-License-Identifier: Apache-2.0
//! Fenced bounded observations in one native metadata snapshot.
use super::{NativeServiceRegistry, State};
use crate::service::ProjectLease;
use airk_core::{
    identity::Sha256Digest,
    service::{JobError, JobKind, JobState, ProjectBinding, WorkerIdentity},
    service_inspection::{InspectionObservation, InspectionSnapshot, MAX_INSPECTION_BYTES},
    service_progress::{ProgressObservation, WorkProgress},
};
use anyhow::{Context, Result, ensure};

pub(super) fn validate(state: &State) -> Result<()> {
    ensure!(
        state.progress.len() <= super::MAX_PROJECTS
            && state.inspections.len() <= super::MAX_PROJECTS,
        "observation count exceeds budget"
    );
    for (binding, observation) in &state.progress {
        observation.progress.validate()?;
        ensure!(
            binding == &observation.worker.project.to_string()
                && state.projects.contains_key(binding)
                && state
                    .jobs
                    .contains_key(&observation.progress.job.to_string())
                && serde_json::to_vec(observation)?.len() <= 4096,
            "invalid native progress observation"
        );
    }
    for (binding, observation) in &state.inspections {
        observation.snapshot.validate()?;
        ensure!(
            binding == &observation.worker.project.to_string()
                && observation.snapshot.project == observation.worker.project
                && state.projects.contains_key(binding)
                && serde_json::to_vec(observation)?.len() <= MAX_INSPECTION_BYTES + 1024,
            "invalid native inspection observation"
        );
    }
    Ok(())
}

pub(super) fn worker_epoch(
    state: &State,
    root: &std::path::Path,
    lease: &ProjectLease,
) -> Result<u64> {
    ensure!(lease.root == root, JobError::StaleWorker);
    let epoch = lease.epoch.context(JobError::StaleWorker)?;
    ensure!(
        state
            .projects
            .get(&lease.binding.to_string())
            .is_some_and(|project| project.epoch == epoch),
        JobError::StaleWorker
    );
    Ok(epoch)
}
impl NativeServiceRegistry {
    pub fn observe_progress(
        &mut self,
        identity: WorkerIdentity,
        progress: &WorkProgress,
        now_ms: u64,
    ) -> Result<()> {
        progress.validate()?;
        self.write(|state| {
            let job = state
                .jobs
                .get(&progress.job.to_string())
                .context(JobError::UnknownJob)?;
            ensure!(
                job.record.project == identity.project
                    && job.record.fencing_generation == identity.fencing_generation
                    && job.record.state != JobState::Pending
                    && state
                        .projects
                        .get(&identity.project.to_string())
                        .is_some_and(|project| project.epoch == identity.fencing_generation),
                "foreign or stale work observation"
            );
            if let Some(prior) = state.progress.get(&identity.project.to_string()) {
                if prior.progress.job == progress.job
                    && prior.worker == identity
                    && prior.progress.grant == progress.grant
                {
                    ensure!(
                        progress.sequence >= prior.progress.sequence
                            && progress.elapsed_ms >= prior.progress.elapsed_ms
                            && progress.captured_files >= prior.progress.captured_files
                            && progress.extracted_files >= prior.progress.extracted_files
                            && progress.prepared_changes >= prior.progress.prepared_changes
                            && progress.committed_changes >= prior.progress.committed_changes
                            && progress.publications >= prior.progress.publications
                            && progress.verified_objects >= prior.progress.verified_objects
                            && (!prior.progress.stalled || progress.stalled),
                        "out of order work observation"
                    );
                } else if prior.progress.job == progress.job {
                    ensure!(
                        identity.fencing_generation > prior.worker.fencing_generation,
                        "old grant cannot replace current progress"
                    );
                } else {
                    let previous = state
                        .jobs
                        .get(&prior.progress.job.to_string())
                        .context("prior progress lost its job")?;
                    ensure!(
                        job.sequence > previous.sequence
                            || job.record.fencing_generation > previous.record.fencing_generation,
                        "old job cannot replace newer progress"
                    );
                }
            }
            let observation = ProgressObservation {
                worker: identity,
                current_fencing_generation: identity.fencing_generation,
                progress: progress.clone(),
                observed_unix_ms: now_ms,
                job: job.record.clone(),
            };
            ensure!(
                serde_json::to_vec(&observation)?.len() <= 4096,
                "work observation exceeds byte budget"
            );
            super::native_maintenance::progress(state, &observation)?;
            state
                .progress
                .insert(identity.project.to_string(), observation);
            state.bump()
        })
    }
    pub fn progress_observation(
        &self,
        project: ProjectBinding,
    ) -> Result<Option<ProgressObservation>> {
        self.read(|state| {
            let registered = state
                .projects
                .get(&project.to_string())
                .filter(|entry| entry.registered)
                .context("service project is not registered")?;
            state
                .progress
                .get(&project.to_string())
                .cloned()
                .map(|mut observation| {
                    observation.current_fencing_generation = registered.epoch;
                    observation.job = state
                        .jobs
                        .get(&observation.progress.job.to_string())
                        .context("progress job missing")?
                        .record
                        .clone();
                    ensure!(
                        observation.worker.project == project && observation.job.project == project,
                        "work observation binding changed"
                    );
                    Ok(observation)
                })
                .transpose()
        })
    }
    pub fn record_inspection(
        &mut self,
        lease: &ProjectLease,
        worker: WorkerIdentity,
        job: Sha256Digest,
        observed_unix_ms: u64,
        snapshot: &InspectionSnapshot,
    ) -> Result<()> {
        snapshot.validate()?;
        self.write(|state| {
            let epoch = worker_epoch(state, &self.root, lease)?;
            ensure!(
                worker.project == lease.binding
                    && worker.fencing_generation == epoch
                    && snapshot.project == worker.project,
                JobError::StaleWorker
            );
            let record = &state
                .jobs
                .get(&job.to_string())
                .context(JobError::UnknownJob)?
                .record;
            ensure!(
                record.project == worker.project
                    && matches!(
                        record.kind,
                        JobKind::Inspect | JobKind::PlanInspect | JobKind::Configure
                    )
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
            ensure!(
                serde_json::to_vec(&observation)?.len() <= MAX_INSPECTION_BYTES + 1024,
                "inspection record exceeds byte bound"
            );
            super::native_maintenance::inspection(state, &observation)?;
            state
                .inspections
                .insert(worker.project.to_string(), observation);
            state.bump()
        })
    }
    pub fn inspection(&self, project: ProjectBinding) -> Result<Option<InspectionObservation>> {
        self.read(|state| {
            let epoch = state
                .projects
                .get(&project.to_string())
                .context("service project is not registered")?
                .epoch;
            state
                .inspections
                .get(&project.to_string())
                .cloned()
                .map(|mut observation| {
                    observation.current_fencing_generation = epoch;
                    ensure!(
                        observation.worker.fencing_generation > 0
                            && observation.worker.fencing_generation <= epoch,
                        "cached inspection has an impossible fencing generation"
                    );
                    Ok(observation)
                })
                .transpose()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::Incarnation, service::ProjectRegistration, service_progress::WorkPhase,
    };
    fn nonce(value: u8) -> Incarnation {
        Incarnation::from_bytes([value; 16])
    }
    fn progress(job: Sha256Digest) -> WorkProgress {
        WorkProgress {
            job,
            grant: nonce(3),
            sequence: 1,
            phase: WorkPhase::Extracting,
            paused: false,
            stalled: false,
            elapsed_ms: 5,
            phase_elapsed_ms: 5,
            phase_totals_ms: Default::default(),
            last_advance_age_ms: Some(0),
            unpaused_idle_ms: 0,
            captured_files: 1,
            extracted_files: 1,
            prepared_changes: 0,
            committed_changes: 0,
            publications: 0,
            verified_objects: 0,
            indexed_files: None,
            total_files: None,
            completed_semantic_units: None,
            publication: None,
        }
    }
    #[test]
    fn native_progress_is_monotonic_fenced_and_keeps_completion_separate() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = tempfile::tempdir()?;
        let mut registry = NativeServiceRegistry::open(root.path(), nonce(1))?;
        let registration = ProjectRegistration::new(
            source.path().to_string_lossy().into_owned(),
            root.path()
                .with_extension("airk-data")
                .to_string_lossy()
                .into_owned(),
            "progress".into(),
        )?;
        registry.register(&registration)?;
        let job =
            registry.enqueue_internal(registration.binding, nonce(1), JobKind::Index, nonce(2))?;
        let mut lease = ProjectLease::acquire(root.path(), registration.binding)?;
        let epoch = registry.admit_worker(&mut lease)?;
        let identity = WorkerIdentity {
            service: nonce(1),
            worker: nonce(4),
            project: registration.binding,
            fencing_generation: epoch,
        };
        let sample = progress(job.id);
        assert!(registry.observe_progress(identity, &sample, 1000).is_err());
        registry.claim_job_id(&lease, job.id)?;
        registry.observe_progress(identity, &sample, 1000)?;
        let mut stale = sample.clone();
        stale.elapsed_ms = 4;
        stale.phase_elapsed_ms = 4;
        assert!(registry.observe_progress(identity, &stale, 1001).is_err());
        let mut regressed = sample.clone();
        regressed.sequence += 1;
        regressed.captured_files = 0;
        assert!(
            registry
                .observe_progress(identity, &regressed, 1001)
                .is_err()
        );
        let mut wrong_grant = sample.clone();
        wrong_grant.grant = nonce(5);
        assert!(
            registry
                .observe_progress(identity, &wrong_grant, 1001)
                .is_err()
        );
        registry.complete_job(
            &lease,
            job.id,
            airk_core::service::JobOutcome::Succeeded { revision: None },
        )?;
        let observed = registry
            .progress_observation(registration.binding)?
            .unwrap();
        assert_eq!(observed.job.state, JobState::Succeeded);
        assert_eq!(observed.progress.phase, WorkPhase::Extracting);
        drop(registry);
        drop(lease);
        let mut registry = NativeServiceRegistry::open(root.path(), nonce(6))?;
        let mut replacement = ProjectLease::acquire(root.path(), registration.binding)?;
        let generation = registry.admit_worker(&mut replacement)?;
        let cached = registry
            .progress_observation(registration.binding)?
            .unwrap();
        assert_eq!(cached.current_fencing_generation, generation);
        assert!(cached.current_fencing_generation > cached.worker.fencing_generation);
        assert!(registry.observe_progress(identity, &sample, 2000).is_err());
        Ok(())
    }
}
