// SPDX-License-Identifier: Apache-2.0
//! Native-only service behavior fixtures; no live registry or source state.
use super::*;
use airk_core::{
    changes::FreshnessState,
    service_configuration::{ConfigurationRequest, ProjectConfiguration},
    service_events::EventKind,
};
fn nonce(value: u8) -> Incarnation {
    Incarnation::from_bytes([value; 16])
}
fn fixture() -> Result<(
    tempfile::TempDir,
    NativeServiceRegistry,
    ProjectRegistration,
    ProjectLease,
)> {
    let base = tempfile::tempdir()?;
    let control = base.path().join("control");
    let source = base.path().join("source");
    fs::create_dir(&control)?;
    fs::create_dir(&source)?;
    let mut registry = NativeServiceRegistry::open(&control, nonce(1))?;
    let registration = ProjectRegistration::new(
        source.to_string_lossy().into_owned(),
        base.path().join("data").to_string_lossy().into_owned(),
        "flow".into(),
    )?;
    registry.register(&registration)?;
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut lease)?;
    Ok((base, registry, registration, lease))
}
#[test]
fn continuations_debit_atomically_and_late_cancellation_preserves_committed_success() -> Result<()>
{
    let (_base, mut registry, project, lease) = fixture()?;
    let source = registry.enqueue_internal(project.binding, nonce(2), JobKind::Index, nonce(3))?;
    registry.claim_job_id(&lease, source.id)?;
    registry.set_background_pending(&lease, JobKind::Vectors, true)?;
    assert!(registry.due_background_jobs(None, 32)?.is_empty());
    registry.complete_job(&lease, source.id, JobOutcome::Succeeded { revision: None })?;
    let due = registry.due_background_jobs(None, 1)?[0];
    let job = registry.enqueue_background_job(due.0, due.1, due.2, nonce(2), nonce(4))?;
    assert!(!registry.has_background_work()?);
    registry.set_background_pending(&lease, JobKind::Vectors, true)?;
    assert_eq!(
        registry
            .enqueue_background_job(due.0, due.1, due.2, nonce(2), nonce(4))?
            .id,
        job.id
    );
    assert!(registry.has_background_work()?);
    registry.claim_job_id(&lease, job.id)?;
    registry.request_cancellation(job.id)?;
    let completed =
        registry.complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })?;
    assert_eq!(completed.state, JobState::Succeeded);
    assert!(!registry.has_background_work()?);
    Ok(())
}
#[test]
fn source_watermarks_keep_events_arriving_during_a_pass_for_later_reconciliation() -> Result<()> {
    let (_base, mut registry, project, lease) = fixture()?;
    registry.configure_watch(project.binding, true)?;
    registry.record_watch_roots(&lease, &[])?;
    let (_, revision) = registry.due_source_projects(None, 100, 1)?[0];
    let job =
        registry.enqueue_source_reconcile(project.binding, nonce(2), nonce(3), revision, 100)?;
    registry.claim_job_id(&lease, job.id)?;
    let epoch = registry.watch(project.binding)?.configuration_epoch;
    registry.source_event(project.binding, epoch, nonce(4), 1)?;
    registry.complete_job_at(
        &lease,
        job.id,
        JobOutcome::Succeeded { revision: None },
        150,
    )?;
    let watched = registry.watch(project.binding)?;
    assert_eq!(watched.observation.completed_revision, revision);
    assert!(watched.observation.requested_revision > revision);
    assert_eq!(
        registry.source_freshness(project.binding, 160)?.state,
        FreshnessState::Pending
    );
    assert_eq!(registry.due_source_projects(None, 160, 1)?.len(), 1);
    Ok(())
}
#[test]
fn configuration_results_recover_unregister_while_paused_and_replay_exactly() -> Result<()> {
    let (base, mut registry, project, lease) = fixture()?;
    registry.pause(project.binding, true)?;
    let (namespace, epoch) = registry.request_epoch()?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: nonce(2),
    };
    let request = ConfigurationRequest {
        incarnation: nonce(3),
        settings: Sha256Digest::from_bytes([4; 32]),
        configuration: ProjectConfiguration::Unregister,
    };
    let engine = Sha256Digest::from_bytes([5; 32]);
    let job = registry.enqueue_configuration(project.binding, nonce(6), &key, &request, engine)?;
    assert!(registry.job_runs_while_paused(job.id)?);
    registry
        .claim_job_id(&lease, job.id)?
        .context("paused unregister was not claimed")?;
    registry.record_configuration_result(
        &lease,
        job.id,
        &serde_json::json!({"unregistration_requested": true}),
    )?;
    drop(registry);
    drop(lease);
    let mut registry = NativeServiceRegistry::open(&base.path().join("control"), nonce(7))?;
    let mut replacement = ProjectLease::acquire(&base.path().join("control"), project.binding)?;
    registry.admit_worker(&mut replacement)?;
    registry.recover_jobs(&replacement)?;
    assert!(!registry.is_registered(project.binding)?);
    assert_eq!(registry.job(job.id)?.state, JobState::Succeeded);
    assert_eq!(
        registry
            .enqueue_configuration(
                project.binding,
                nonce(6),
                &key,
                &request,
                Sha256Digest::from_bytes([8; 32])
            )?
            .id,
        job.id
    );
    assert_eq!(registry.configuration_work(job.id)?.unwrap().engine, engine);
    assert_eq!(
        registry.configuration_result(job.id)?.unwrap()["unregistered"],
        true
    );
    drop(replacement);
    registry.register(&project)?;
    assert!(registry.is_registered(project.binding)?);
    Ok(())
}
#[test]
fn event_pages_pin_their_through_position_and_replay_does_not_emit_transitions() -> Result<()> {
    let (_base, mut registry, project, lease) = fixture()?;
    let job = registry.enqueue_internal(project.binding, nonce(2), JobKind::Verify, nonce(3))?;
    let first = registry.events(Some(project.binding), None, 1)?;
    let through = first.snapshot_through;
    registry.claim_job_id(&lease, job.id)?;
    let next = registry.events(Some(project.binding), first.next.as_ref(), 32)?;
    assert_eq!(next.snapshot_through, through);
    assert!(next.events.iter().all(|event| event.sequence <= through));
    let tail = registry.events(Some(project.binding), Some(&first.tail), 32)?;
    assert_eq!(tail.events.len(), 1);
    assert_eq!(tail.events[0].kind, EventKind::JobRunning);
    assert!(registry.events(None, Some(&first.tail), 32).is_err());
    Ok(())
}

#[test]
fn queue_rotation_preserves_the_named_failure_and_all_active_receipts() -> Result<()> {
    let (_base, mut registry, project, lease) = fixture()?;
    let original =
        registry.enqueue_internal(project.binding, nonce(2), JobKind::Verify, nonce(3))?;
    registry.claim_job_id(&lease, original.id)?;
    registry.complete_job(
        &lease,
        original.id,
        JobOutcome::Failed {
            code: "verification_failed".into(),
        },
    )?;
    let pending = Sha256Digest::from_bytes([91; 32]);
    registry.write(|state| {
        for number in 0u32..1023 {
            let id = if number == 1022 {
                pending
            } else {
                framed_digest(b"native-capacity-fixture", &[&number.to_le_bytes()])
            };
            state.next_job_sequence += 1;
            let terminal = number != 1022;
            state.jobs.insert(
                id.to_string(),
                NativeJob {
                    sequence: state.next_job_sequence,
                    caller: nonce(4),
                    key: RequestKey {
                        namespace: state.incarnation,
                        epoch: state.request_epoch,
                        nonce: nonce(5),
                    },
                    record: JobRecord {
                        id,
                        project: project.binding,
                        kind: JobKind::Verify,
                        state: if terminal {
                            JobState::Succeeded
                        } else {
                            JobState::Pending
                        },
                        fencing_generation: 0,
                        attempts: 0,
                        outcome: terminal.then_some(JobOutcome::Succeeded { revision: None }),
                    },
                    cancellation_requested: false,
                    source_revision: None,
                    configuration: None,
                    plan: None,
                },
            );
        }
        Ok(())
    })?;
    let (namespace, epoch) = registry.request_epoch()?;
    assert_eq!(registry.job(original.id)?.state, JobState::Failed);
    let stale = RequestKey {
        namespace,
        epoch,
        nonce: nonce(6),
    };
    assert!(matches!(
        registry
            .retry_job(original.id, nonce(7), &stale)
            .unwrap_err()
            .downcast_ref::<JobError>(),
        Some(JobError::KeyExpired)
    ));
    assert_eq!(registry.job(original.id)?.state, JobState::Failed);
    assert_eq!(registry.job(pending)?.state, JobState::Pending);
    let (namespace, epoch) = registry.request_epoch()?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: nonce(8),
    };
    let retry = registry.retry_job(original.id, nonce(7), &key)?;
    assert_eq!(retry.id, pending);
    assert_eq!(
        registry.retry_job(original.id, nonce(7), &key)?.id,
        retry.id
    );
    let mut cursor = None;
    let mut retry_events = 0;
    loop {
        let page = registry.events(Some(project.binding), cursor.as_ref(), 32)?;
        retry_events += page
            .events
            .iter()
            .filter(|event| {
                event.job == Some(retry.id) && event.kind == EventKind::JobRetryRequested
            })
            .count();
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        retry_events, 1,
        "exact retry replay duplicated or lost its durable event"
    );
    assert!(
        registry
            .enqueue(project.binding, nonce(7), &key, JobKind::Verify)
            .is_err()
    );
    Ok(())
}
#[test]
fn old_registry_handles_cannot_read_or_mutate_a_replacement_incarnation() -> Result<()> {
    let (base, mut stale, project, lease) = fixture()?;
    let root = base.path().join("control");
    drop(lease);
    // Simulate an independently completed namespace replacement inside this
    // isolated fixture. Registry handles intentionally do not pin database files.
    for name in [FILE, "service-1.4.owner.json"] {
        let original = root.join(name);
        let retained = root.join(format!("retained-{name}"));
        assert!(
            original.is_absolute()
                && retained.is_absolute()
                && original.parent() == Some(root.as_path())
                && retained.parent() == Some(root.as_path())
        );
        fs::rename(original, retained)?;
    }
    let mut fresh = NativeServiceRegistry::open(&root, nonce(90))?;
    fresh.register(&project)?;
    let before = fresh.list(None, None, 32)?;
    assert!(stale.list(None, None, 32).is_err());
    assert!(stale.get(project.binding).is_err());
    assert!(stale.pause(project.binding, true).is_err());
    assert!(stale.request_epoch().is_err());
    let after = fresh.list(None, None, 32)?;
    assert_eq!(before.revision, after.revision);
    assert!(!fresh.get(project.binding)?.paused);
    Ok(())
}

#[test]
fn same_install_reset_fences_old_keys_event_cursors_and_handles() -> Result<()> {
    let (base, mut stale, project, lease) = fixture()?;
    drop(lease);
    let root = base.path().join("control");
    let ownership = stale.incarnation();
    let (namespace, epoch) = stale.request_epoch()?;
    let key = RequestKey {
        namespace,
        epoch,
        nonce: nonce(93),
    };
    let cursor = stale.events(None, None, 1)?.tail;
    fs::rename(root.join(FILE), root.join("retained-native.airk"))?;
    for name in LEGACY_FILES {
        fs::write(root.join(name), b"original owned reset fixture")?;
    }
    let lease = crate::service::ServiceLease::acquire(&root)?;
    assert!(reset_owned_sqlite(&root, &lease, ownership).is_err());
    reset_owned_sqlite(&root, &lease, nonce(94))?;
    let mut fresh = NativeServiceRegistry::open(&root, ownership)?;
    assert_eq!(fresh.incarnation(), ownership);
    assert_ne!(fresh.request_epoch()?.0, namespace);
    fresh.register(&project)?;
    assert!(
        fresh
            .enqueue(project.binding, nonce(95), &key, JobKind::Index)
            .is_err()
    );
    assert!(fresh.events(None, Some(&cursor), 1).is_err());
    assert!(stale.list(None, None, 1).is_err());
    assert!(stale.pause(project.binding, true).is_err());
    assert!(!fresh.get(project.binding)?.paused);
    Ok(())
}

#[test]
fn inactive_dispatch_observes_only_typed_cleanup_and_allows_fenced_claim() -> Result<()> {
    let (base, mut registry, project, lease) = fixture()?;
    let (namespace, epoch) = registry.request_epoch()?;
    let request = ConfigurationRequest {
        incarnation: nonce(3),
        settings: Sha256Digest::from_bytes([4; 32]),
        configuration: ProjectConfiguration::Unregister,
    };
    let job = registry.enqueue_configuration(
        project.binding,
        nonce(4),
        &RequestKey {
            namespace,
            epoch,
            nonce: nonce(5),
        },
        &request,
        Sha256Digest::from_bytes([6; 32]),
    )?;
    registry.claim_job_id(&lease, job.id)?;
    registry.record_configuration_result(
        &lease,
        job.id,
        &serde_json::json!({"unregistration_requested":true}),
    )?;
    registry.complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })?;
    drop(lease);
    let (namespace, epoch) = registry.request_epoch()?;
    let request = ConfigurationRequest {
        incarnation: namespace,
        settings: project.binding,
        configuration: ProjectConfiguration::PrepareDerivedCleanup,
    };
    let cleanup = registry.enqueue_configuration(
        project.binding,
        nonce(7),
        &RequestKey {
            namespace,
            epoch,
            nonce: nonce(8),
        },
        &request,
        Sha256Digest::from_bytes([6; 32]),
    )?;
    assert_eq!(
        registry.next_pending_job(project.binding)?.unwrap().id,
        cleanup.id
    );
    assert_eq!(
        registry
            .pending_job_observation(project.binding)?
            .unwrap()
            .id,
        cleanup.id
    );
    assert_eq!(
        registry.pending_project_page(None, 32)?,
        vec![project.binding]
    );
    assert!(
        registry
            .enqueue(
                project.binding,
                nonce(9),
                &RequestKey {
                    namespace,
                    epoch,
                    nonce: nonce(10)
                },
                JobKind::Index
            )
            .is_err()
    );
    let mut owner = ProjectLease::acquire(&base.path().join("control"), project.binding)?;
    registry.admit_derived_worker(&mut owner)?;
    assert_eq!(registry.claim_job(&owner)?.unwrap().id, cleanup.id);
    Ok(())
}
