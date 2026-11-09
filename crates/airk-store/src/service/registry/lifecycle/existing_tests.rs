// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{identity::Incarnation, service::RequestKey};
use std::fs;

#[test]
fn existing_lifecycle_write_keeps_saved_reads_readonly_and_namespace_fenced() -> Result<()> {
    let root = tempfile::tempdir()?;
    let namespace = Incarnation::from_bytes([71; 16]);
    let native = ServiceRegistry::open(root.path(), namespace)?;
    let (_, epoch) = native.saved_request_epoch()?;
    drop(native);
    let database = root.path().join("service-1.4.sqlite3");
    let before = fs::read(&database)?;
    assert!(
        ServiceRegistry::open_saved(root.path())?
            .clear_service_stop()
            .is_err()
    );
    assert_eq!(fs::read(&database)?, before);
    let mut observed = false;
    assert!(
        ServiceRegistry::with_existing_lifecycle(
            root.path(),
            Incarnation::from_bytes([72; 16]),
            |_| {
                observed = true;
                Ok(())
            }
        )
        .is_err()
    );
    assert!(!observed);
    assert_eq!(fs::read(&database)?, before);
    let request = LifecycleRequest {
        caller: Incarnation::from_bytes([73; 16]),
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([74; 16]),
        },
        action: LifecycleAction::Restart,
        expected_instance: Incarnation::from_bytes([75; 16]),
        runtime: Sha256Digest::from_bytes([76; 32]),
        manager: false,
    };
    let record = ServiceRegistry::with_existing_lifecycle(root.path(), namespace, |ledger| {
        ledger.begin_lifecycle(&request)
    })?;
    assert_eq!(record.request, request);
    let starting = ServiceRegistry::with_existing_lifecycle(root.path(), namespace, |ledger| {
        ledger.advance_lifecycle(&record, LifecyclePhase::Starting)
    })?;
    let complete = ServiceRegistry::with_existing_lifecycle(root.path(), namespace, |ledger| {
        ledger.advance_lifecycle(&starting, LifecyclePhase::Complete)
    })?;
    assert_eq!(complete.request, request);
    assert_eq!(complete.phase, LifecyclePhase::Complete);
    assert!(!ServiceRegistry::open_saved(root.path())?.service_explicitly_stopped()?);
    ServiceRegistry::with_existing_lifecycle(root.path(), namespace, |ledger| {
        ledger.clear_service_stop()
    })?;
    assert_eq!(
        serde_json::to_value(
            ServiceRegistry::open_saved(root.path())?.lifecycle_by_id(record.id)?
        )?,
        serde_json::to_value(Some(complete))?
    );
    Ok(())
}

#[test]
fn existing_lifecycle_write_refuses_unknown_schema11_shape_without_mutation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let namespace = Incarnation::from_bytes([77; 16]);
    drop(ServiceRegistry::open(root.path(), namespace)?);
    let database = root.path().join("service-1.4.sqlite3");
    rusqlite::Connection::open(&database)?.execute_batch(
        "CREATE TRIGGER foreign_lifecycle_effect AFTER UPDATE ON service_lifecycle_state BEGIN SELECT 1; END;")?;
    let before = fs::read(&database)?;
    let mut observed = false;
    assert!(
        ServiceRegistry::with_existing_lifecycle(root.path(), namespace, |_| {
            observed = true;
            Ok(())
        })
        .is_err()
    );
    assert!(!observed);
    assert_eq!(fs::read(&database)?, before);
    Ok(())
}

#[test]
fn existing_lifecycle_write_refuses_real_schema8_without_migration() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let control = owner.path().join("control");
    fs::create_dir(&control)?;
    let namespace = Incarnation::from_bytes([78; 16]);
    let registration = airk_core::service::ProjectRegistration::new(
        owner.path().join("source").to_string_lossy().into_owned(),
        owner.path().join("data").to_string_lossy().into_owned(),
        "original".into(),
    )?;
    super::super::predecessor_schema8_tests::schema8(&control, namespace, &registration)?;
    let database = control.join("service-1.4.sqlite3");
    let before = fs::read(&database)?;
    let owner_before = fs::read(control.join("service-1.4.owner.json"))?;
    let mut observed = false;
    let error = ServiceRegistry::with_existing_lifecycle(&control, namespace, |_| {
        observed = true;
        Ok(())
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("unsupported_existing_registry_write"));
    assert!(!observed);
    assert_eq!(
        ServiceRegistry::open_saved(&control)?.incarnation()?,
        namespace
    );
    assert_eq!(fs::read(&database)?, before);
    assert_eq!(
        fs::read(control.join("service-1.4.owner.json"))?,
        owner_before
    );
    Ok(())
}

#[test]
fn existing_lifecycle_write_does_not_create_a_missing_registry() -> Result<()> {
    let root = tempfile::tempdir()?;
    let missing = root.path().join("missing");
    assert!(
        ServiceRegistry::with_existing_lifecycle(
            &missing,
            Incarnation::from_bytes([79; 16]),
            |_| Ok(())
        )
        .is_err()
    );
    assert!(!missing.exists());
    Ok(())
}
