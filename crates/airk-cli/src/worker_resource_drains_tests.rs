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
        owners.poll(worker, None, |_, _| Ok(false))?,
        DrainProgress::ChildPending
    );
    assert!(owners.mark_released(worker).is_err());
    owners.child_reaped(worker)?;
    assert_eq!(
        owners.poll(worker, None, |_, _| Ok(false))?,
        DrainProgress::ProviderPending,
        "missing/replaced helper is not engine completion"
    );
    assert!(owners.mark_released(worker).is_err());
    // Exercise exact native-proof routing; actual engine/native-reap witnesses
    // are qualified separately by the helper process tests.
    assert_eq!(
        owners.poll(worker, None, |service, generation| Ok(service
            == worker.service
            && generation == helper))?,
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
        owners.poll(worker, None, |_, _| unreachable!())?,
        DrainProgress::ChildPending
    );
    owners.child_reaped(worker)?;
    assert_eq!(
        owners.poll(worker, None, |_, _| unreachable!())?,
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
