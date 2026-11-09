// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::service::ProjectLease;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{JobKind, JobOutcome, ProjectRegistration},
    service_progress::WorkPhase,
};
fn nonce(n: u8) -> Incarnation {
    Incarnation::from_bytes([n; 16])
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
fn cached_progress_is_fenced_monotonic_and_does_not_invent_completion_after_restart() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(root.path(), nonce(1))?;
    let registration =
        ProjectRegistration::new("/source".into(), "/derived".into(), "fixture".into())?;
    let project = registration.binding;
    registry.register(&registration)?;
    let job = registry.enqueue_internal(project, nonce(1), JobKind::Index, nonce(2))?;
    let mut lease = ProjectLease::acquire(root.path(), project)?;
    let epoch = registry.admit_worker(&mut lease)?;
    let identity = WorkerIdentity {
        service: nonce(1),
        worker: nonce(4),
        project,
        fencing_generation: epoch,
    };
    let snapshot = progress(job.id);
    assert!(
        registry
            .observe_progress(identity, &snapshot, 1000)
            .is_err()
    );
    registry.claim_job_id(&lease, job.id)?;
    registry.observe_progress(identity, &snapshot, 1000)?;
    let mut stale = snapshot.clone();
    stale.elapsed_ms = 4;
    stale.phase_elapsed_ms = 4;
    assert!(registry.observe_progress(identity, &stale, 1001).is_err());
    let mut wrong = snapshot.clone();
    wrong.grant = nonce(5);
    assert!(registry.observe_progress(identity, &wrong, 1001).is_err());
    let mut regressed = snapshot.clone();
    regressed.sequence += 1;
    regressed.captured_files = 0;
    regressed.extracted_files = 0;
    assert!(
        registry
            .observe_progress(identity, &regressed, 1001)
            .is_err()
    );
    registry.complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })?;
    let cached = registry.progress_observation(project)?.unwrap();
    assert_eq!(cached.job.state, JobState::Succeeded);
    assert_eq!(
        cached.progress.phase,
        WorkPhase::Extracting,
        "terminal receipt fabricated a completed progress report"
    );
    assert_eq!(cached.progress.total_files, None);
    drop(registry);
    drop(lease);
    let mut registry = ServiceRegistry::open(root.path(), nonce(6))?;
    let mut replacement = ProjectLease::acquire(root.path(), project)?;
    let epoch = registry.admit_worker(&mut replacement)?;
    let cached = registry.progress_observation(project)?.unwrap();
    assert_eq!(cached.current_fencing_generation, epoch);
    assert_eq!(cached.worker, identity);
    assert!(epoch > cached.worker.fencing_generation);
    assert!(
        registry
            .observe_progress(identity, &snapshot, 2000)
            .is_err()
    );
    let next = registry.enqueue_internal(project, nonce(6), JobKind::Index, nonce(7))?;
    registry.claim_job_id(&replacement, next.id)?;
    let newer = WorkerIdentity {
        service: nonce(6),
        worker: nonce(8),
        project,
        fencing_generation: epoch,
    };
    let second = progress(next.id);
    registry.observe_progress(newer, &second, 2000)?;
    drop(replacement);
    let mut replacement = ProjectLease::acquire(root.path(), project)?;
    let epoch = registry.admit_worker(&mut replacement)?;
    registry.recover_jobs(&replacement)?;
    registry.claim_job_id(&replacement, next.id)?;
    let resumed = WorkerIdentity {
        worker: nonce(9),
        fencing_generation: epoch,
        ..newer
    };
    let mut restarted = progress(next.id);
    restarted.grant = nonce(10);
    restarted.sequence = 0;
    restarted.elapsed_ms = 0;
    restarted.phase_elapsed_ms = 0;
    registry.observe_progress(resumed, &restarted, 3000)?;
    assert_eq!(
        registry.progress_observation(project)?.unwrap().worker,
        resumed
    );
    Ok(())
}
