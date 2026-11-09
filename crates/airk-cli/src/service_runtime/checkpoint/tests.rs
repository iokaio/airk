// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn durable_recovery_fence_is_closed_in_memory_and_external_removal_does_not_reopen_it() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let control = owner.path().canonicalize()?;
    let operation = Incarnation::from_bytes([7; 16]);
    arm_recovery(&control, operation, Sha256Digest::from_bytes([3; 32]))?;
    let coordinator = Arc::new(Coordinator::new(&control)?);
    assert!(coordinator.admission_closed()?);
    assert!(coordinator.admit().is_err());
    assert_eq!(coordinator.active.load(Ordering::Acquire), 0);
    assert!(settle(&control, Incarnation::from_bytes([8; 16])).is_err());
    assert!(coordinator.admission_closed()?);
    let saved = fence(&control)?.context("fence absent")?;
    assert!(saved.service_instance.is_none()); // No fabricated predecessor identity.
    assert!(saved.origin == FenceOrigin::InstallerAfterVerifiedExit);
    settle(&control, operation)?;
    // Only the validated private native Release transition may reopen memory.
    assert!(coordinator.admission_closed()?);
    assert!(coordinator.admit().is_err());
    assert_eq!(coordinator.active.load(Ordering::Acquire), 0);
    Ok(())
}

#[test]
fn memory_quiescence_retains_previously_accepted_request_charge() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let coordinator = Arc::new(Coordinator::new(owner.path())?);
    let accepted = coordinator.admit()?;
    coordinator.closed.store(true, Ordering::Release);
    assert!(coordinator.admit().is_err());
    assert_eq!(coordinator.active.load(Ordering::Acquire), 1);
    drop(accepted);
    assert_eq!(coordinator.active.load(Ordering::Acquire), 0);
    Ok(())
}

#[test]
fn immutable_packet_reconciliation_preserves_changed_bytes() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let control = owner.path().canonicalize()?;
    let name = packet_name(Incarnation::from_bytes([4; 16]));
    immutable(&control, &name, b"original complete captured packet")?;
    immutable(&control, &name, b"original complete captured packet")?;
    assert!(immutable(&control, &name, b"changed state").is_err());
    assert_eq!(
        bytes(&control.join(name), MAX_PACKET)?,
        b"original complete captured packet"
    );
    Ok(())
}
