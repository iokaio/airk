// SPDX-License-Identifier: Apache-2.0
use super::*;

fn identity(n: u8) -> WorkerIdentity {
    WorkerIdentity {
        service: Incarnation::from_bytes([1; 16]),
        worker: Incarnation::from_bytes([n; 16]),
        project: airk_core::service::ProjectBinding::from_bytes([n; 32]),
        fencing_generation: 1,
    }
}

#[test]
fn resource_owner_requires_both_child_and_provider_completion() -> Result<()> {
    let worker = identity(2);
    let helper = Incarnation::from_bytes([3; 16]);
    let mut owners = WorkerResourceDrains::default();
    owners.reserve_history(worker, vec![helper])?;
    assert_eq!(
        owners.poll(worker, None, |_, _, _| Ok(false))?,
        DrainProgress::ChildPending
    );
    assert!(owners.mark_released(worker).is_err());
    owners.child_reaped(worker)?;
    assert_eq!(
        owners.poll(worker, None, |_, _, _| Ok(false))?,
        DrainProgress::ProviderPending,
        "missing/replaced helper is not engine completion"
    );
    assert!(owners.mark_released(worker).is_err());
    // Exercise exact native-proof routing; actual engine/native-reap witnesses
    // are qualified separately by the helper process tests.
    assert_eq!(
        owners.poll(worker, None, |service, generation, in_process| Ok(service
            == worker.service
            && generation == helper
            && !in_process))?,
        DrainProgress::Ready
    );
    owners.mark_released(worker)?;
    assert_eq!(owners.for_project(worker.project), vec![worker]);
    owners.remove_completed(worker)?;
    assert!(owners.is_empty());
    Ok(())
}

#[test]
fn never_authorized_still_requires_actual_child_reap_and_exact_identity() -> Result<()> {
    let worker = identity(4);
    let mut owners = WorkerResourceDrains::default();
    owners.reserve_history(worker, vec![])?;
    let mut foreign = worker;
    foreign.fencing_generation += 1;
    assert!(owners.child_reaped(foreign).is_err());
    assert_eq!(
        owners.poll(worker, None, |_, _, _| unreachable!())?,
        DrainProgress::ChildPending
    );
    owners.child_reaped(worker)?;
    assert_eq!(
        owners.poll(worker, None, |_, _, _| unreachable!())?,
        DrainProgress::Ready
    );
    assert!(owners.remove_completed(worker).is_err());
    owners.mark_released(worker)?;
    owners.remove_completed(worker)?;
    Ok(())
}

#[test]
fn bounded_owner_refusal_preserves_existing_obligations() -> Result<()> {
    let mut owners = WorkerResourceDrains::default();
    for n in 1..=128 {
        owners.reserve_history(identity(n), vec![])?;
    }
    assert!(owners.reserve_history(identity(129), vec![]).is_err());
    assert_eq!(owners.identities().len(), 128);
    owners.reserve_history(identity(1), vec![])?;
    assert!(
        owners
            .reserve_history(identity(1), vec![Incarnation::from_bytes([9; 16])])
            .is_err()
    );
    assert_eq!(owners.identities().len(), 128);
    Ok(())
}
#[test]
fn inprocess_history_never_routes_through_legacy_helper_proof() -> Result<()> {
    let worker = identity(6);
    let generation = Incarnation::from_bytes([7; 16]);
    let mut owners = WorkerResourceDrains::default();
    owners.reserve_history_kind(worker, vec![generation], true)?;
    assert!(owners.reserve_history(worker, vec![generation]).is_err());
    owners.child_reaped(worker)?;
    assert_eq!(
        owners.poll(worker, None, |_, _, in_process| {
            assert!(in_process);
            Ok(false)
        })?,
        DrainProgress::ProviderPending
    );
    assert_eq!(
        owners.poll(worker, None, |service, observed, in_process| Ok(in_process
            && service == worker.service
            && observed == generation))?,
        DrainProgress::Ready
    );
    Ok(())
}

#[test]
fn completed_authorizations_require_native_anchor_even_without_live_generations() -> Result<()> {
    let worker = identity(8);
    let anchor = airk_core::inference_provider::CompletionLedgerObservation {
        schema: 1,
        namespace: Incarnation::from_bytes([2; 16]),
        service_instance: worker.service,
        approved_execution_sha256: "ab".repeat(32).parse()?,
        factory_generation: Incarnation::from_bytes([3; 16]),
        completed_owners: 40,
        admitted_high_water: 80,
        head_sha256: "cd".repeat(32).parse()?,
    };
    let mut owners = WorkerResourceDrains::default();
    owners.reserve_history_completed(worker, Vec::new(), true, Some(anchor.clone()))?;
    owners.child_reaped(worker)?;
    assert_eq!(
        owners.poll(worker, None, |_, _, _| unreachable!())?,
        DrainProgress::ProviderPending,
        "parsed completion metadata is not a native witness"
    );
    assert!(owners.mark_released(worker).is_err());
    assert_eq!(
        owners.poll_completed(
            worker,
            None,
            |_, _, _| unreachable!(),
            |expected| Ok(expected == &anchor)
        )?,
        DrainProgress::Ready
    );
    owners.mark_released(worker)?;
    Ok(())
}
