// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::identity::Sha256Digest;
use rusqlite::params;

// Format-only qualified layout fixture, not a claim of actual OS/native authority.
fn qualified() -> Result<(tempfile::TempDir, ServiceRegistry)> {
    let root = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    let old = catalog::schema_digest(&registry.connection)?;
    let tx = registry.connection.transaction()?;
    catalog::install_schema(&tx)?;
    tx.execute(
        "INSERT INTO index_catalog_meta VALUES(1,1,1,0,0,0,?1,?2,?3)",
        params![
            Incarnation::from_bytes([2; 16]).to_string(),
            old.to_string(),
            Sha256Digest::from_bytes([3; 32]).to_string()
        ],
    )?;
    tx.pragma_update(None, "user_version", 12)?;
    catalog::apply_authority_v1_layout(&tx)?;
    tx.execute(
        "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
        [],
    )?;
    tx.commit()?;
    Ok((root, registry))
}

#[test]
fn failed_native_transaction_keeps_predecessor_compatible_and_commit_rejects_old_contract()
-> Result<()> {
    let (root, mut registry) = qualified()?;
    let mut old = catalog::capabilities();
    old.maximum_registry_schema = 12;
    required_capabilities(root.path())?.require_supported(&old)?;
    {
        let tx = registry.connection.transaction()?;
        arm_in_transaction(&tx)?;
        assert_eq!(
            tx.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
            13
        );
        // An initial failed candidate/effect rolls back the existing marker too.
    }
    assert_eq!(
        registry
            .connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        12
    );
    required_capabilities(root.path())?.require_supported(&old)?;
    {
        let tx = registry.connection.transaction()?;
        arm_in_transaction(&tx)?;
        tx.commit()?;
    }
    let required = required_capabilities(root.path())?;
    let error = required.require_supported(&old).unwrap_err();
    assert!(error.is::<RuntimeContractUnsupported>());
    required.require_supported(&catalog::capabilities())?;
    assert_eq!(
        ServiceRegistry::open_saved(root.path())?
            .catalog_state()?
            .unwrap()
            .schema,
        13
    );
    // Current ordinary startup can reopen exactly the same registry; it does not
    // downgrade the required marker or reinterpret the saved provider history.
    let reopened = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    assert_eq!(reopened.catalog_state()?.unwrap().schema, 13);
    Ok(())
}

#[test]
fn embedded_mutation_guard_binds_namespace_and_never_initializes_unowned_roots() -> Result<()> {
    let empty = tempfile::tempdir()?;
    assert!(require_embedded_default_write(empty.path()).is_err());
    assert_eq!(std::fs::read_dir(empty.path())?.count(), 0);
    let (root, registry) = qualified()?;
    assert!(
        require_embedded_default_write_for_namespace(root.path(), Incarnation::from_bytes([9; 16]))
            .is_err()
    );
    assert_eq!(
        registry
            .connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        12
    );
    require_embedded_default_write_for_namespace(root.path(), Incarnation::from_bytes([1; 16]))?;
    assert_eq!(registry.catalog_state()?.unwrap().schema, 13);
    assert!(registry.catalog_state()?.unwrap().accepted_new_writes);
    Ok(())
}

#[test]
fn concurrent_embedded_mutation_guards_wait_for_existing_fence_then_both_complete() -> Result<()> {
    let (root, registry) = qualified()?;
    let held = crate::store_fs::acquire_existing_file(root.path(), "service-1.4.initialize.lock")?;
    let ready = std::sync::Arc::new(std::sync::Barrier::new(3));
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| -> Result<()> {
        for _ in 0..2 {
            let ready = ready.clone();
            let sender = sender.clone();
            let path = root.path();
            scope.spawn(move || {
                ready.wait();
                let result = require_embedded_default_write_for_namespace(
                    path,
                    Incarnation::from_bytes([1; 16]),
                );
                sender.send(result).unwrap();
            });
        }
        ready.wait();
        // A refused job would return immediately. Both submissions must remain
        // pending while the actual native ownership file is held elsewhere.
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(50)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        assert_eq!(registry.catalog_state()?.unwrap().schema, 12);
        drop(held);
        receiver.recv_timeout(Duration::from_secs(3))??;
        receiver.recv_timeout(Duration::from_secs(3))??;
        Ok(())
    })?;
    assert_eq!(registry.catalog_state()?.unwrap().schema, 13);
    assert!(registry.catalog_state()?.unwrap().accepted_new_writes);
    assert!(std::fs::read(root.path().join("service-1.4.initialize.lock"))?.is_empty());
    Ok(())
}

#[test]
fn embedded_mutation_fence_deadline_preserves_typed_contention_and_database_bytes() -> Result<()> {
    let (root, registry) = qualified()?;
    let held = crate::store_fs::acquire_existing_file(root.path(), "service-1.4.initialize.lock")?;
    let database = root.path().join("service-1.4.sqlite3");
    let before = std::fs::read(&database)?;
    let started = std::time::Instant::now();
    let error =
        require_embedded_default_write_for_namespace(root.path(), Incarnation::from_bytes([1; 16]))
            .unwrap_err();
    assert!(started.elapsed() >= Duration::from_secs(2));
    assert!(matches!(
        error
            .chain()
            .find_map(|cause| cause.downcast_ref::<std::fs::TryLockError>()),
        Some(std::fs::TryLockError::WouldBlock)
    ));
    assert_eq!(std::fs::read(&database)?, before);
    assert_eq!(registry.catalog_state()?.unwrap().schema, 12);
    // The ordinary existing-file primitive still refuses immediately; its
    // writer/ledger/manifest callers have not inherited this bounded wait.
    assert!(
        crate::store_fs::acquire_existing_file(root.path(), "service-1.4.initialize.lock").is_err()
    );
    drop(held);
    Ok(())
}

#[test]
fn embedded_mutation_wait_never_creates_or_repairs_missing_or_foreign_ownership() -> Result<()> {
    let (root, registry) = qualified()?;
    let database = root.path().join("service-1.4.sqlite3");
    let before = std::fs::read(&database)?;
    let lock = root.path().join("service-1.4.initialize.lock");
    std::fs::remove_file(&lock)?;
    assert!(
        require_embedded_default_write_for_namespace(root.path(), Incarnation::from_bytes([1; 16]))
            .is_err()
    );
    assert!(!lock.exists());
    assert_eq!(std::fs::read(&database)?, before);
    std::fs::write(&lock, b"foreign ownership marker")?;
    assert!(
        require_embedded_default_write_for_namespace(root.path(), Incarnation::from_bytes([1; 16]))
            .is_err()
    );
    assert_eq!(std::fs::read(&lock)?, b"foreign ownership marker");
    assert_eq!(std::fs::read(&database)?, before);
    assert_eq!(registry.catalog_state()?.unwrap().schema, 12);
    Ok(())
}
