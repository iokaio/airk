// SPDX-License-Identifier: Apache-2.0
//! Legacy and service allowances must never share ownership or cleanup.
use super::*;
use crate::FileStore;
use airk_core::identity::{Incarnation, Sha256Digest};

#[test]
fn service_grants_and_staging_survive_legacy_cleanup_with_the_same_collection_id() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let legacy = Quota::open(owner.path())?;
    let service = Quota::open_service(owner.path())?;
    let old_store = FileStore::new(owner.path().join("collections-1.2/fixture"));
    let new_store = FileStore::new(owner.path().join("collections-1.4/fixture"));
    let old_writer = old_store.lock()?;
    let new_writer = new_store.lock()?;
    let scope = CurrentGrantScope {
        collection: "fixture".into(),
        incarnation: Incarnation::from_bytes([8; 16]),
        transaction: Sha256Digest::from_bytes([9; 32]),
    };
    let old_grant = legacy.reserve_current(&old_writer, scope.clone(), 12345)?;
    let new_grant = service.reserve_current(&new_writer, scope.clone(), 54321)?;
    drop(new_grant); // Durable allowance survives loss of the process handle.
    assert!(legacy.current_grants(&new_writer).is_err());
    assert!(service.current_grants(&old_writer).is_err());
    let old = legacy.reserve("fixture", 4096)?;
    let new = service.reserve("fixture", 8192)?;
    let old_stage = old.staging_directory()?;
    let new_stage = new.staging_directory()?;
    assert!(old_stage.starts_with(old_store.directory().canonicalize()?));
    assert!(new_stage.starts_with(new_store.directory().canonicalize()?));
    old.create_file(old_stage.join("old.bin"))?
        .write_all(b"legacy")?;
    new.create_file(new_stage.join("new.bin"))?
        .write_all(b"service")?;
    assert!(old.create_file(new_stage.join("cross.bin")).is_err());
    assert!(new.create_file(old_stage.join("cross.bin")).is_err());
    let grant_path = fs::read_dir(service.control_path().join("current-grants"))?
        .next()
        .context("grant missing")??
        .path();
    let record = fs::read(&grant_path)?;
    old_grant.release(&old_writer)?;
    drop(old);
    assert!(!old_stage.exists());
    let old_status = legacy.status()?;
    assert_eq!(old_status.active_reservations, 0);
    assert_eq!(old_status.reserved_bytes, 0);
    assert_eq!(fs::read(new_stage.join("new.bin"))?, b"service");
    assert_eq!(fs::read(&grant_path)?, record);
    let status = service.status()?;
    assert_eq!(status.active_reservations, 2);
    assert_eq!(status.reserved_bytes, 54321 + 8192);
    service
        .current_grant(&new_writer, &scope)?
        .context("service allowance lost")?
        .release(&new_writer)?;
    Ok(())
}

#[test]
fn service_policy_inventory_and_lifecycle_are_independent_of_legacy_state() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let legacy = Quota::open(owner.path())?;
    let service = Quota::open_service(owner.path())?;
    fs::create_dir_all(owner.path().join("collections-1.2/fixture"))?;
    fs::create_dir_all(owner.path().join("collections-1.4/fixture"))?;
    fs::write(
        owner.path().join("collections-1.2/fixture/source"),
        b"legacy bytes",
    )?;
    fs::write(owner.path().join("collections-1.4/fixture/source"), b"new")?;
    let policy = QuotaPolicy {
        root_bytes: 1 << 20,
        default_collection_bytes: 1 << 19,
        collections: Default::default(),
    };
    service.configure(policy.clone())?;
    assert_ne!(legacy.policy()?, policy);
    assert_eq!(service.policy()?, policy);
    assert_eq!(legacy.status()?.collections["fixture"].used_bytes, 12);
    assert_eq!(service.status()?.collections["fixture"].used_bytes, 3);
    let destination = owner.path().join("moved");
    assert!(crate::relocation::plan(owner.path(), &destination).is_err());
    assert!(crate::relocation::relocate(owner.path(), &destination).is_err());
    assert!(!destination.exists());
    assert!(!owner.path().join(".relocation-in-progress.json").exists());
    assert!(!owner.path().join(".relocated-to.json").exists());
    let old_exclusive = lifecycle_lease(owner.path(), true)?;
    let new_store = FileStore::new(owner.path().join("collections-1.4/fixture"));
    let new_shared = new_store
        .lifecycle(false)?
        .context("service lease missing")?;
    assert!(service_lifecycle_lease(owner.path(), true).is_err());
    drop(new_shared);
    drop(old_exclusive);
    let new_exclusive = service_lifecycle_lease(owner.path(), true)?;
    let old_shared = lifecycle_lease(owner.path(), false)?;
    drop(old_shared);
    drop(new_exclusive);
    fs::write(
        legacy.control_path().join("unknown"),
        b"preserve original state",
    )?;
    assert!(legacy.status().is_err());
    assert_eq!(service.status()?.collections["fixture"].used_bytes, 3);
    assert_eq!(
        fs::read(legacy.control_path().join("unknown"))?,
        b"preserve original state"
    );
    Ok(())
}
