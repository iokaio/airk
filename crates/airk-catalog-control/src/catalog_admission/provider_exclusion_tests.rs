// SPDX-License-Identifier: Apache-2.0
//! Provider exclusion admission test. Moved from `service_inference::tests`
//! (split step 2b): it drives the catalog Gate, which sits above inference.
use airk_core::{identity::Incarnation, index_catalog::CatalogRevision};
use airk_inference::service_inference::PredecessorExclusionWitness;
use anyhow::Result;
use std::sync::Arc;

#[test]
fn predecessor_exclusion_is_exact_and_cannot_be_used_as_helper_reap() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    crate::local_ipc::prepare_root(&root)?;
    let current = Incarnation::from_bytes([1; 16]);
    let prior = Incarnation::from_bytes([2; 16]);
    let generation = Incarnation::from_bytes([3; 16]);
    let registry = airk_store::service::registry::ServiceRegistry::open(&root, current)?;
    let namespace = registry.incarnation()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    assert!(
        PredecessorExclusionWitness::acquire(lease.clone(), current, current, generation).is_err()
    );
    let witness = PredecessorExclusionWitness::acquire(lease, current, prior, generation)?;
    let gate = crate::catalog_admission::NativeCatalogAdmission::new(
        current,
        namespace,
        CatalogRevision::INITIAL,
        0,
    )?;
    gate.observe_provider_excluded(witness.clone())?;
    assert!(gate.provider_was_completed(prior, generation, true)?);
    assert!(!gate.provider_was_completed(prior, generation, false)?);
    assert!(!gate.provider_was_completed(current, generation, true)?);
    let foreign = crate::catalog_admission::NativeCatalogAdmission::new(
        current,
        Incarnation::from_bytes([4; 16]),
        CatalogRevision::INITIAL,
        0,
    )?;
    assert!(foreign.observe_provider_excluded(witness.clone()).is_err());
    let changed = crate::catalog_admission::NativeCatalogAdmission::new(
        Incarnation::from_bytes([5; 16]),
        namespace,
        CatalogRevision::INITIAL,
        0,
    )?;
    assert!(changed.observe_provider_excluded(witness).is_err());
    Ok(())
}
