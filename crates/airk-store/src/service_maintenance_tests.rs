// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::service::ProjectLease;
use airk_core::{
    service::{JobKind, JobOutcome, JobState, ProjectRegistration, WorkerIdentity},
    service_inspection::{IndexState, InspectionSnapshot, SourceState},
    service_maintenance::RepairOperation,
};

#[test]
fn dispatch_turns_are_bounded_and_independent_plans_do_not_share_jobs() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    for n in 1..=8 {
        register(&mut registry, n)?;
    }
    let a = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Verify,
        6,
    )?;
    let b = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Verify,
        7,
    )?;
    registry.advance_maintenance_plans(1100)?;
    let a_page = registry.maintenance_plan_page(a.id, None, 32)?;
    let b_page = registry.maintenance_plan_page(b.id, None, 32)?;
    assert_eq!(
        a_page
            .items
            .iter()
            .filter(|item| item.phase == ItemPhase::InspectQueued)
            .count(),
        4
    );
    assert_eq!(
        b_page
            .items
            .iter()
            .filter(|item| item.phase == ItemPhase::InspectQueued)
            .count(),
        4
    );
    for (a, b) in a_page.items.iter().zip(&b_page.items) {
        if a.inspection_job.is_some() {
            assert_ne!(a.inspection_job, b.inspection_job);
        }
    }
    registry.pause_maintenance_plan(a.id, true, 1200)?;
    registry.advance_maintenance_plans(1300)?;
    let b_page = registry.maintenance_plan_page(b.id, None, 32)?;
    assert!(
        b_page
            .items
            .iter()
            .all(|item| item.phase == ItemPhase::InspectQueued)
    );
    registry.cancel_maintenance_plan(a.id, 1400)?;
    registry.advance_maintenance_plans(1500)?;
    for item in b_page.items {
        assert_eq!(
            registry.job(item.inspection_job.unwrap())?.state,
            JobState::Pending
        );
    }
    assert_eq!(
        registry.maintenance_plan(a.id)?.outcome,
        Some(PlanOutcome::Canceled)
    );
    Ok(())
}

fn register(registry: &mut ServiceRegistry, n: u8) -> Result<ProjectBinding> {
    let project = ProjectRegistration::new(
        format!("/source-{n}"),
        format!("/state-{n}"),
        format!("fixture-{n}"),
    )?;
    registry.register(&project)?;
    Ok(project.binding)
}
fn request(
    registry: &mut ServiceRegistry,
    selection: RepairSelection,
    operation: RepairOperation,
    n: u8,
) -> Result<MaintenancePlan> {
    let (namespace, epoch) = registry.request_epoch()?;
    registry.create_maintenance_plan(
        Incarnation::from_bytes([1; 16]),
        &RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([n; 16]),
        },
        &RepairRequest {
            selection,
            operation,
        },
        1000,
        Sha256Digest::from_bytes([2; 32]),
    )
}
fn owner(
    registry: &mut ServiceRegistry,
    root: &std::path::Path,
    project: ProjectBinding,
) -> Result<ProjectLease> {
    let mut lease = ProjectLease::acquire(root, project)?;
    registry.admit_worker(&mut lease)?;
    Ok(lease)
}
fn snapshot(project: ProjectBinding) -> InspectionSnapshot {
    InspectionSnapshot {
        quota_policy_digest: None,
        project,
        incarnation: Incarnation::from_bytes([3; 16]),
        publication: None,
        settings_digest: Some(Sha256Digest::from_bytes([4; 32])),
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
    }
}
fn inspect(
    registry: &mut ServiceRegistry,
    lease: &ProjectLease,
    snapshot: &InspectionSnapshot,
) -> Result<()> {
    let job = registry
        .claim_job(lease)?
        .context("inspection not admitted")?;
    assert_eq!(job.kind, JobKind::PlanInspect);
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([1; 16]),
        worker: Incarnation::from_bytes([5; 16]),
        project: snapshot.project,
        fencing_generation: job.fencing_generation,
    };
    registry.record_inspection(lease, worker, job.id, 1200, snapshot)?;
    registry.complete_job_at(
        lease,
        job.id,
        JobOutcome::Succeeded {
            revision: snapshot.publication,
        },
        1300,
    )?;
    Ok(())
}

#[test]
fn approved_plan_is_durable_fenced_private_and_preserves_partial_failure() -> Result<()> {
    let root = tempfile::tempdir()?;
    let instance = Incarnation::from_bytes([1; 16]);
    let mut registry = ServiceRegistry::open(root.path(), instance)?;
    let a = register(&mut registry, 1)?;
    let b = register(&mut registry, 2)?;
    let plan = request(
        &mut registry,
        RepairSelection::Selected(vec![b, a]),
        RepairOperation::Verify,
        6,
    )?;
    assert!(
        registry
            .approve_maintenance_plan(plan.id, Sha256Digest::from_bytes([0; 32]), false, 1001)
            .is_err()
    );
    registry.advance_maintenance_plans(1100)?;
    let a_lease = owner(&mut registry, root.path(), a)?;
    let b_lease = owner(&mut registry, root.path(), b)?;
    inspect(&mut registry, &a_lease, &snapshot(a))?;
    assert_eq!(
        registry.maintenance_plan(plan.id)?.phase,
        PlanPhase::Planning
    );
    inspect(&mut registry, &b_lease, &snapshot(b))?;
    let ready = registry.maintenance_plan(plan.id)?;
    assert_eq!(ready.phase, PlanPhase::Ready);
    let preview = ready.preview_digest.unwrap();
    assert!(registry.next_pending_job(a)?.is_none());
    drop(registry);
    let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([7; 16]))?;
    registry.approve_maintenance_plan(plan.id, preview, false, 1400)?;
    registry.advance_maintenance_plans(1500)?;
    let a_job = registry.claim_job(&a_lease)?.unwrap();
    assert_eq!(a_job.kind, JobKind::Repair);
    let progress = airk_core::service_progress::WorkProgress {
        job: a_job.id,
        grant: Incarnation::from_bytes([8; 16]),
        sequence: 1,
        phase: airk_core::service_progress::WorkPhase::Verifying,
        paused: false,
        stalled: false,
        elapsed_ms: 5,
        phase_elapsed_ms: 5,
        phase_totals_ms: Default::default(),
        last_advance_age_ms: Some(0),
        unpaused_idle_ms: 0,
        captured_files: 0,
        extracted_files: 0,
        prepared_changes: 0,
        committed_changes: 0,
        publications: 0,
        verified_objects: 1,
        indexed_files: None,
        total_files: None,
        completed_semantic_units: None,
        publication: None,
    };
    registry.observe_progress(
        WorkerIdentity {
            service: instance,
            worker: Incarnation::from_bytes([9; 16]),
            project: a,
            fencing_generation: a_job.fencing_generation,
        },
        &progress,
        1501,
    )?;
    assert!(
        registry
            .repair_work(&a_lease, a_job.id, Sha256Digest::from_bytes([9; 32]))
            .is_err()
    );
    assert!(
        registry
            .repair_work(&b_lease, a_job.id, plan.engine)
            .is_err()
    );
    assert_eq!(
        registry
            .repair_work(&a_lease, a_job.id, plan.engine)?
            .operation,
        RepairOperation::Verify
    );
    registry.pause_maintenance_plan(plan.id, true, 1600)?;
    assert_eq!(registry.maintenance_job_control(a_job.id)?, (true, false));
    assert!(registry.claim_job(&b_lease)?.is_none());
    registry.pause_maintenance_plan(plan.id, false, 1700)?;
    registry.complete_job_at(
        &a_lease,
        a_job.id,
        JobOutcome::Succeeded { revision: None },
        1800,
    )?;
    let b_job = registry.claim_job(&b_lease)?.unwrap();
    registry.complete_job_at(
        &b_lease,
        b_job.id,
        JobOutcome::Failed {
            code: "maintenance_plan_stale".into(),
        },
        1900,
    )?;
    let completed = registry.maintenance_plan(plan.id)?;
    assert_eq!(completed.phase, PlanPhase::Finished);
    assert_eq!(completed.outcome, Some(PlanOutcome::PartialFailure));
    assert_eq!((completed.counts.succeeded, completed.counts.stale), (1, 1));
    assert_eq!(
        registry.maintenance_plan_page(plan.id, None, 1)?.after,
        Some(a.min(b))
    );
    let again = registry.approve_maintenance_plan(plan.id, preview, true, 2000)?;
    assert_eq!(again.phase, PlanPhase::Finished);
    assert!(registry.claim_job(&a_lease)?.is_none());
    registry.connection.execute(
        "DELETE FROM service_jobs WHERE id=?1",
        [a_job.id.to_string()],
    )?;
    drop(a_lease);
    let replacement = owner(&mut registry, root.path(), a)?;
    let retained = registry
        .maintenance_plan_page(plan.id, None, 32)?
        .items
        .into_iter()
        .find(|item| item.project == a)
        .unwrap()
        .progress
        .unwrap();
    assert_eq!(retained.progress.job, a_job.id);
    assert_eq!(retained.progress.verified_objects, 1);
    assert!(retained.current_fencing_generation > retained.worker.fencing_generation);
    drop(replacement);
    Ok(())
}

#[test]
fn all_membership_is_frozen_and_cancel_does_not_touch_other_requests() -> Result<()> {
    let root = tempfile::tempdir()?;
    let instance = Incarnation::from_bytes([1; 16]);
    let mut registry = ServiceRegistry::open(root.path(), instance)?;
    let a = register(&mut registry, 1)?;
    let plan = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Auto,
        6,
    )?;
    let b = register(&mut registry, 2)?;
    assert_eq!(registry.maintenance_plan(plan.id)?.counts.selected, 1);
    let unrelated = registry.enqueue_internal(
        a,
        instance,
        JobKind::Verify,
        Incarnation::from_bytes([8; 16]),
    )?;
    assert!(
        registry
            .enqueue_internal(
                a,
                instance,
                JobKind::Repair,
                Incarnation::from_bytes([9; 16])
            )
            .is_err()
    );
    registry.advance_maintenance_plans(1100)?;
    registry.cancel_maintenance_plan(plan.id, 1200)?;
    registry.advance_maintenance_plans(1300)?;
    assert_eq!(registry.job(unrelated.id)?.state, JobState::Pending);
    let completed = registry.maintenance_plan(plan.id)?;
    assert_eq!(completed.outcome, Some(PlanOutcome::Canceled));
    assert_eq!(completed.counts.canceled, 1);
    assert!(
        registry
            .maintenance_plan_page(plan.id, None, 32)?
            .items
            .iter()
            .all(|item| item.project != b)
    );
    let repeat = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Auto,
        6,
    )?;
    assert_eq!(repeat.id, plan.id);
    assert_eq!(repeat.counts.selected, 1);
    assert!(
        request(
            &mut registry,
            RepairSelection::All,
            RepairOperation::Verify,
            6
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn failed_inspections_have_finite_backoff_and_pause_freezes_the_deadline() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    let a = register(&mut registry, 1)?;
    let plan = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Verify,
        6,
    )?;
    let lease = owner(&mut registry, root.path(), a)?;
    let mut now = 1100;
    for attempt in 1..=3 {
        registry.advance_maintenance_plans(now)?;
        let job = registry.claim_job(&lease)?.unwrap();
        registry.complete_job_at(
            &lease,
            job.id,
            JobOutcome::Failed {
                code: "inspection_changed".into(),
            },
            now + 100,
        )?;
        if attempt < 3 {
            registry.advance_maintenance_plans(now + 101)?;
            assert!(registry.claim_job(&lease)?.is_none());
            now += 100 + (1000 << attempt);
        }
    }
    let ready = registry.maintenance_plan(plan.id)?;
    assert_eq!(ready.phase, PlanPhase::Ready);
    assert_eq!(ready.counts.skipped, 1);
    registry.approve_maintenance_plan(plan.id, ready.preview_digest.unwrap(), false, now + 200)?;
    registry.advance_maintenance_plans(now + 201)?;
    assert_eq!(
        registry.maintenance_plan(plan.id)?.outcome,
        Some(PlanOutcome::PartialFailure)
    );
    let plan = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Verify,
        7,
    )?;
    registry.advance_maintenance_plans(10_000)?;
    registry.pause_maintenance_plan(plan.id, true, 11_000)?;
    registry.advance_maintenance_plans(900_000)?;
    registry.pause_maintenance_plan(plan.id, false, 911_000)?;
    registry.advance_maintenance_plans(912_000)?;
    assert!(
        registry.claim_job(&lease)?.is_some(),
        "pause consumed the queued probe deadline"
    );
    Ok(())
}

#[test]
fn destructive_fallback_requires_approval_and_recovered_publication_is_not_cleanup_proof()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    let a = register(&mut registry, 1)?;
    let plan = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Auto,
        6,
    )?;
    registry.advance_maintenance_plans(1100)?;
    let lease = owner(&mut registry, root.path(), a)?;
    let mut unavailable = snapshot(a);
    unavailable.index_state = IndexState::Unavailable;
    unavailable.failure =
        Some(airk_core::service_inspection::InspectionFailure::IndexObservationFailed);
    inspect(&mut registry, &lease, &unavailable)?;
    let ready = registry.maintenance_plan(plan.id)?;
    assert!(
        registry
            .approve_maintenance_plan(plan.id, ready.preview_digest.unwrap(), false, 1400)
            .is_err()
    );
    registry.approve_maintenance_plan(plan.id, ready.preview_digest.unwrap(), true, 1400)?;
    registry.advance_maintenance_plans(1500)?;
    let job = registry.claim_job(&lease)?.unwrap();
    registry.complete_job_at(
        &lease,
        job.id,
        JobOutcome::Succeeded { revision: None },
        1600,
    )?;
    let completed = registry.maintenance_plan(plan.id)?;
    assert_eq!(completed.counts.cleanup_pending, 1);
    assert_eq!(completed.outcome, Some(PlanOutcome::PartialFailure));
    Ok(())
}

#[test]
fn plan_retention_expires_old_keys_and_never_repeats_a_retired_receipt() -> Result<()> {
    let root = tempfile::tempdir()?;
    let instance = Incarnation::from_bytes([1; 16]);
    let mut registry = ServiceRegistry::open(root.path(), instance)?;
    register(&mut registry, 1)?;
    let (namespace, epoch) = registry.request_epoch()?;
    let first = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Verify,
        6,
    )?;
    registry.cancel_maintenance_plan(first.id, 1100)?;
    registry.advance_maintenance_plans(1101)?;
    for n in 7..=9 {
        let plan = request(
            &mut registry,
            RepairSelection::All,
            RepairOperation::Verify,
            n,
        )?;
        registry.cancel_maintenance_plan(plan.id, 1200)?;
        registry.advance_maintenance_plans(1201)?;
    }
    assert!(
        request(
            &mut registry,
            RepairSelection::All,
            RepairOperation::Verify,
            10
        )
        .unwrap_err()
        .is::<PlanError>()
    );
    let (new_namespace, new_epoch) = registry.request_epoch()?;
    assert_eq!(new_namespace, namespace);
    assert!(new_epoch > epoch);
    assert!(registry.maintenance_plan(first.id).is_err());
    let old = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([6; 16]),
    };
    let result = registry.create_maintenance_plan(
        instance,
        &old,
        &RepairRequest {
            selection: RepairSelection::All,
            operation: RepairOperation::Verify,
        },
        1300,
        first.engine,
    );
    assert_eq!(
        result.unwrap_err().downcast_ref::<PlanError>(),
        Some(&PlanError::KeyExpired)
    );
    let fresh = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Verify,
        10,
    )?;
    assert_ne!(fresh.id, first.id);
    Ok(())
}

#[test]
fn disk_pressure_retries_are_finite_and_recovery_does_not_replay_ambiguous_repairs() -> Result<()> {
    let root = tempfile::tempdir()?;
    let instance = Incarnation::from_bytes([1; 16]);
    let mut registry = ServiceRegistry::open(root.path(), instance)?;
    let project = register(&mut registry, 1)?;
    let plan = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Reconcile,
        6,
    )?;
    registry.advance_maintenance_plans(1100)?;
    let lease = owner(&mut registry, root.path(), project)?;
    inspect(&mut registry, &lease, &snapshot(project))?;
    let ready = registry.maintenance_plan(plan.id)?;
    registry.approve_maintenance_plan(plan.id, ready.preview_digest.unwrap(), false, 1400)?;
    let mut now = 1500;
    for attempt in 1..=3 {
        registry.advance_maintenance_plans(now)?;
        let job = registry.claim_job(&lease)?.unwrap();
        registry.complete_job_at(
            &lease,
            job.id,
            JobOutcome::Failed {
                code: "storage_pressure".into(),
            },
            now + 100,
        )?;
        if attempt < 3 {
            registry.advance_maintenance_plans(now + 101)?;
            assert!(registry.claim_job(&lease)?.is_none());
            now += 100 + (10_000 << (attempt - 1));
        }
    }
    let completed = registry.maintenance_plan(plan.id)?;
    assert_eq!(completed.outcome, Some(PlanOutcome::PartialFailure));
    assert_eq!(completed.counts.failed, 1);
    let next = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Reconcile,
        7,
    )?;
    registry.advance_maintenance_plans(now + 1000)?;
    inspect(&mut registry, &lease, &snapshot(project))?;
    let ready = registry.maintenance_plan(next.id)?;
    registry.approve_maintenance_plan(next.id, ready.preview_digest.unwrap(), false, now + 1100)?;
    registry.advance_maintenance_plans(now + 1200)?;
    let interrupted = registry.claim_job(&lease)?.unwrap();
    drop(lease);
    let replacement = owner(&mut registry, root.path(), project)?;
    registry.recover_jobs(&replacement)?;
    assert_eq!(registry.job(interrupted.id)?.state, JobState::Failed);
    assert!(registry.claim_job(&replacement)?.is_none());
    assert_eq!(
        registry.maintenance_plan(next.id)?.outcome,
        Some(PlanOutcome::PartialFailure)
    );
    Ok(())
}

#[test]
fn unavailable_sources_are_skipped_and_unadmitted_probes_expire() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    let project = register(&mut registry, 1)?;
    let plan = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Reconcile,
        6,
    )?;
    registry.advance_maintenance_plans(1100)?;
    let lease = owner(&mut registry, root.path(), project)?;
    let mut unavailable = snapshot(project);
    unavailable.source_state = SourceState::Unavailable;
    inspect(&mut registry, &lease, &unavailable)?;
    let page = registry.maintenance_plan_page(plan.id, None, 32)?;
    assert_eq!(page.items[0].phase, ItemPhase::Skipped);
    assert_eq!(page.items[0].code.as_deref(), Some("source_unavailable"));
    let expired = request(
        &mut registry,
        RepairSelection::All,
        RepairOperation::Verify,
        7,
    )?;
    registry.advance_maintenance_plans(700_000)?;
    let page = registry.maintenance_plan_page(expired.id, None, 32)?;
    assert_eq!(page.plan.phase, PlanPhase::Ready);
    assert_eq!(
        page.items[0].code.as_deref(),
        Some("inspection_admission_timeout")
    );
    assert!(registry.claim_job(&lease)?.is_none());
    Ok(())
}
