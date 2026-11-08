// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::Arc;

#[test]
fn actual_service_lease_shares_factory_claim_and_monotonic_compute_seal() -> Result<()> {
    let root = tempfile::tempdir()?;
    let lease = Arc::new(ServiceLease::acquire(root.path())?);
    lease.claim_inference_factory()?;
    let other = lease.clone();
    assert!(
        std::thread::spawn(move || other.claim_inference_factory().is_err())
            .join()
            .unwrap()
    );
    lease.seal_unused_inference()?;
    assert!(lease.admit_inference_owner().is_err());
    assert!(lease.claim_inference_factory().is_err());
    assert!(ServiceLease::acquire(root.path()).is_err());
    drop(lease);
    let fresh = ServiceLease::acquire(root.path())?;
    fresh.claim_inference_factory()?;
    fresh.admit_inference_owner()?;
    assert!(
        fresh.seal_unused_inference().is_err(),
        "completed/failed/idle model owner became never admitted"
    );
    Ok(())
}

#[test]
fn sealing_used_inference_reports_every_admission_without_claiming_completion() -> Result<()> {
    let root = tempfile::tempdir()?;
    let lease = ServiceLease::acquire(root.path())?;
    assert!(lease.seal_inference_admission().is_err());
    lease.claim_inference_factory()?;
    lease.admit_inference_owner()?;
    lease.admit_inference_owner()?;
    assert_eq!(lease.seal_inference_admission()?, 2);
    assert!(lease.admit_inference_owner().is_err());
    assert!(lease.seal_unused_inference().is_err());
    assert_eq!(lease.seal_inference_admission()?, 2);
    Ok(())
}
