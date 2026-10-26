// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Incarnation,
    service::{JobKind, JobOutcome, ProjectRegistration},
};

fn nonce(value: u8) -> Incarnation {
    Incarnation::from_bytes([value; 16])
}

#[test]
fn source_event_receipts_survive_restart_and_fence_configuration_changes() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(owner.path(), nonce(1))?;
    let registration =
        ProjectRegistration::new("/original".into(), "/derived".into(), "fixture".into())?;
    let project = registration.binding;
    registry.register(&registration)?;
    registry.configure_watch(project, true)?;
    let epoch = registry.watch(project)?.configuration_epoch;
    assert!(registry.source_event(project, epoch, nonce(7), 1)?);
    assert!(!registry.source_event(project, epoch, nonce(7), 1)?);
    assert!(registry.source_event(project, epoch, nonce(7), 3)?);
    assert!(!registry.source_event(project, epoch, nonce(7), 2)?);
    assert!(registry.source_event(project, epoch, nonce(7), 0).is_err());
    let revision = registry.watch(project)?.observation.requested_revision;
    drop(registry);
    let mut registry = ServiceRegistry::open(owner.path(), nonce(2))?;
    assert!(!registry.source_event(project, epoch, nonce(7), 3)?);
    assert_eq!(
        registry.watch(project)?.observation.requested_revision,
        revision
    );
    // A new privately authenticated watcher starts its own sequence.
    assert!(registry.source_event(project, epoch, nonce(8), 1)?);
    registry.configure_watch(project, false)?;
    assert!(!registry.source_event(project, epoch, nonce(8), 2)?);
    registry.configure_watch(project, true)?;
    assert!(!registry.source_event(project, epoch, nonce(8), 2)?);
    let current = registry.watch(project)?;
    assert!(registry.source_event(project, current.configuration_epoch, nonce(8), 2)?);
    assert_eq!(
        registry.watch(project)?.observation.requested_revision,
        current.observation.requested_revision + 1
    );
    Ok(())
}

#[test]
fn startup_failure_settlement_requires_exclusive_unadmitted_ownership() -> Result<()> {
    use airk_core::service::JobState;
    let owner = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(owner.path(), nonce(1))?;
    let registration =
        ProjectRegistration::new("/original".into(), "/derived".into(), "fixture".into())?;
    let project = registration.binding;
    registry.register(&registration)?;
    registry.configure_watch(project, true)?;
    let job = registry.enqueue_source_reconcile(project, nonce(1), nonce(2), 1, 1000)?;
    let mut lease = ProjectLease::acquire(owner.path(), project)?;
    registry.admit_worker(&mut lease)?;
    assert!(
        registry
            .fail_pending_start(&lease, "worker_start_failed")
            .is_err()
    );
    assert_eq!(registry.job(job.id)?.state, JobState::Pending);
    registry
        .claim_job_id(&lease, job.id)?
        .context("job not claimable")?;
    drop(lease);
    let unadmitted = ProjectLease::acquire(owner.path(), project)?;
    assert!(
        registry
            .fail_pending_start(&unadmitted, "worker_start_failed")
            .is_err()
    );
    assert_eq!(registry.job(job.id)?.state, JobState::Running);
    drop(unadmitted);
    let mut lease = ProjectLease::acquire(owner.path(), project)?;
    registry.admit_worker(&mut lease)?;
    registry.recover_jobs(&lease)?;
    registry
        .claim_job_id(&lease, job.id)?
        .context("interrupted job not recovered")?;
    registry.complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })?;
    registry.source_changed(project, registry.watch(project)?.configuration_epoch)?;
    let pending = registry.enqueue_source_reconcile(project, nonce(1), nonce(3), 2, 100_000)?;
    drop(lease);
    let unadmitted = ProjectLease::acquire(owner.path(), project)?;
    assert_eq!(
        registry.fail_pending_start(&unadmitted, "worker_start_failed")?,
        1
    );
    assert_eq!(registry.job(pending.id)?.state, JobState::Failed);
    assert_eq!(registry.watch(project)?.observation.completed_revision, 1);
    assert_eq!(
        registry.fail_pending_start(&unadmitted, "worker_start_failed")?,
        0
    );
    Ok(())
}

#[test]
fn source_completion_acknowledges_its_capture_and_preserves_later_events_and_root_fencing()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(owner.path(), nonce(1))?;
    let registration =
        ProjectRegistration::new("/original".into(), "/derived".into(), "fixture".into())?;
    let project = registration.binding;
    registry.register(&registration)?;
    assert!(!registry.watch(project)?.observation.enabled);
    assert!(!registry.watch(project)?.observation.configuration_observed);
    registry.configure_watch(project, true)?;
    registry.configure_watch(project, true)?;
    assert_eq!(registry.watch(project)?.observation.requested_revision, 1);
    let mut lease = ProjectLease::acquire(owner.path(), project)?;
    registry.admit_worker(&mut lease)?;
    registry.record_watch_roots(&lease, &["/additional".into()])?;
    let watched = registry.watch(project)?;
    assert_eq!(watched.observation.requested_revision, 2);
    assert!(
        registry
            .source_changed(project, watched.configuration_epoch - 1)
            .is_err()
    );
    registry.source_changed(project, watched.configuration_epoch)?;
    assert_eq!(
        registry.due_source_projects(None, 1000, 32)?,
        vec![(project, 3)]
    );
    let first = registry.enqueue_source_reconcile(project, nonce(1), nonce(2), 3, 1000)?;
    assert_eq!(
        registry
            .enqueue_source_reconcile(project, nonce(1), nonce(2), 3, 1000)?
            .id,
        first.id
    );
    assert!(
        registry
            .enqueue_source_reconcile(project, nonce(1), nonce(3), 3, 100_000)
            .is_err()
    );
    registry
        .claim_job_id(&lease, first.id)?
        .context("source job was not claimable")?;
    registry.source_changed(project, watched.configuration_epoch)?;
    registry.complete_job_at(
        &lease,
        first.id,
        JobOutcome::Succeeded { revision: None },
        2000,
    )?;
    let state = registry.watch(project)?.observation;
    assert_eq!((state.completed_revision, state.requested_revision), (3, 4));
    assert_eq!(state.last_reconciled_unix_ms, Some(2000));
    let second = registry.enqueue_source_reconcile(project, nonce(1), nonce(4), 4, 100_000)?;
    registry
        .claim_job_id(&lease, second.id)?
        .context("later source job was not claimable")?;
    registry.complete_job_at(
        &lease,
        second.id,
        JobOutcome::Succeeded { revision: None },
        101_000,
    )?;
    assert!(registry.due_source_projects(None, 200_000, 32)?.is_empty());
    registry.record_watch_roots(&lease, &["/additional".into()])?;
    assert_eq!(
        registry.watch(project)?.observation.requested_revision,
        4,
        "identical roots created new desired work"
    );
    drop(registry);
    let mut registry = ServiceRegistry::open(owner.path(), nonce(5))?;
    assert_eq!(registry.watch(project)?.observation.completed_revision, 4);
    drop(lease);
    let mut replacement = ProjectLease::acquire(owner.path(), project)?;
    registry.admit_worker(&mut replacement)?;
    assert_eq!(
        registry.watch(project)?.additional_roots,
        vec!["/additional"]
    );
    Ok(())
}

#[test]
fn source_cancellation_pause_and_retry_exhaustion_survive_restart_without_losing_desired_work()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(owner.path(), nonce(1))?;
    let registration =
        ProjectRegistration::new("/original".into(), "/derived".into(), "fixture".into())?;
    let project = registration.binding;
    registry.register(&registration)?;
    registry.configure_watch(project, true)?;
    let epoch = registry.watch(project)?.configuration_epoch;
    let canceled = registry.enqueue_source_reconcile(project, nonce(1), nonce(2), 1, 1000)?;
    registry.cancel_pending(canceled.id)?;
    assert!(registry.due_source_projects(None, 100_000, 32)?.is_empty());
    registry.source_changed(project, epoch)?;
    registry.pause(project, true)?;
    assert!(registry.due_source_projects(None, 100_000, 32)?.is_empty());
    registry.source_changed(project, epoch)?;
    registry.pause(project, false)?;
    let mut lease = ProjectLease::acquire(owner.path(), project)?;
    registry.admit_worker(&mut lease)?;
    for attempt in 3..=5 {
        assert_eq!(
            registry.due_source_projects(None, u64::from(attempt) * 100_000, 32)?,
            vec![(project, 3)]
        );
        let job = registry.enqueue_source_reconcile(
            project,
            nonce(1),
            nonce(attempt),
            3,
            u64::from(attempt) * 100_000,
        )?;
        registry
            .claim_job_id(&lease, job.id)?
            .context("retry was not claimable")?;
        registry.complete_job_at(
            &lease,
            job.id,
            JobOutcome::Failed {
                code: "source_fixture_failure".into(),
            },
            u64::from(attempt) * 100_000 + 1000,
        )?;
    }
    assert!(
        registry
            .due_source_projects(None, 1_000_000, 32)?
            .is_empty()
    );
    drop(registry);
    let mut registry = ServiceRegistry::open(owner.path(), nonce(6))?;
    assert_eq!(registry.watch(project)?.observation.requested_revision, 3);
    assert_eq!(registry.watch(project)?.observation.completed_revision, 0);
    assert!(
        registry
            .due_source_projects(None, 1_000_000, 32)?
            .is_empty()
    );
    // A deliberate new source event reopens bounded retries; an explicit manual
    // request can also settle the desired revision without depending on auto mode.
    registry.source_changed(project, epoch)?;
    registry.configure_watch(project, false)?;
    let manual = registry.enqueue_internal(project, nonce(6), JobKind::Index, nonce(7))?;
    registry
        .claim_job_id(&lease, manual.id)?
        .context("manual source job was not claimable")?;
    registry.complete_job_at(
        &lease,
        manual.id,
        JobOutcome::Succeeded { revision: None },
        2_000_000,
    )?;
    assert_eq!(registry.watch(project)?.observation.completed_revision, 4);
    Ok(())
}

#[test]
fn watch_root_failure_is_conservative_idempotent_and_recovery_invalidates_native_coverage()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(owner.path(), nonce(1))?;
    let registration =
        ProjectRegistration::new("/original".into(), "/derived".into(), "fixture".into())?;
    let project = registration.binding;
    registry.register(&registration)?;
    registry.configure_watch(project, true)?;
    let mut lease = ProjectLease::acquire(owner.path(), project)?;
    registry.admit_worker(&mut lease)?;
    registry.record_watch_roots(&lease, &[])?;
    let watched = registry.watch(project)?;
    registry.record_watch_observation(
        project,
        watched.configuration_epoch,
        1,
        WatchBackend::Native,
        None,
        1000,
    )?;
    registry.invalidate_watch_roots(&lease)?;
    let failed = registry.watch(project)?;
    assert!(!failed.observation.configuration_observed);
    assert_eq!(
        failed.observation.failure,
        Some(WatchFailure::MetadataUnavailable)
    );
    registry.invalidate_watch_roots(&lease)?;
    assert_eq!(
        registry.watch(project)?.observation.requested_revision,
        failed.observation.requested_revision
    );
    registry.record_watch_roots(&lease, &["/recovered".into()])?;
    let recovered = registry.watch(project)?;
    assert!(recovered.observation.configuration_observed);
    assert_eq!(
        recovered.observation.backend,
        WatchBackend::PeriodicFallback
    );
    assert_eq!(recovered.observation.native_roots, 0);
    assert_eq!(recovered.observation.observed_unix_ms, None);
    assert!(
        registry
            .record_watch_observation(
                project,
                watched.configuration_epoch,
                1,
                WatchBackend::Native,
                None,
                2000
            )
            .is_err()
    );
    Ok(())
}
