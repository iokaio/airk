// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::fs;

fn registry_bytes(root: &Path) -> Result<Vec<u8>> {
    Ok(fs::read(root.join(REGISTRY))?)
}

#[test]
fn an_admitted_first_start_gives_a_fresh_root_an_activated_catalog_once() -> Result<()> {
    let root = tempfile::tempdir()?;
    let lease = ServiceLease::acquire(root.path())?;
    let control = lease.root().to_path_buf();
    let lease = ensure_catalog(lease)?;
    assert_eq!(lease.root(), control, "the same lease comes back");
    let catalog = ServiceRegistry::open_saved(&control)?
        .catalog_state()?
        .context("fresh Genesis created no catalog")?;
    assert_eq!(catalog.authority_contract, 1);
    assert!(catalog.activated, "the catalog is activated");
    assert_eq!(
        genesis::pending(&control)?.map(|receipt| receipt.stage),
        Some(GenesisStage::Settled)
    );
    // A later start leaves an existing catalog untouched.
    let before = registry_bytes(&control)?;
    let lease = ensure_catalog(lease)?;
    assert_eq!(registry_bytes(&control)?, before);
    drop(lease);
    Ok(())
}

#[test]
fn a_used_root_without_a_catalog_is_refused_without_writing() -> Result<()> {
    let root = tempfile::tempdir()?;
    let lease = ServiceLease::acquire(root.path())?;
    let control = lease.root().to_path_buf();
    let instance = airk_kernel::service_broker::new_instance_id()?;
    // A service that ran on this root without a catalog recorded its instance.
    ServiceRegistry::open(&control, instance)?.record_service_instance(&lease, instance)?;
    let before = registry_bytes(&control)?;
    let Err(error) = ensure_catalog(lease) else {
        panic!("a used root must be refused");
    };
    assert!(
        error
            .chain()
            .any(|cause| cause.to_string().starts_with(REFUSED)),
        "{error:#}"
    );
    assert_eq!(registry_bytes(&control)?, before);
    assert!(
        ServiceRegistry::open_saved(&control)?
            .catalog_state()?
            .is_none()
    );
    Ok(())
}
