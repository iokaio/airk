// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::service::ServiceLease;

/// Retain the exact real SQLite crash image after a spilled uncommitted write.
/// No handwritten journal/header or production recovery operation constructs it.
fn interrupted_transaction(root: &Path) -> Result<()> {
    let database = root.join("service-1.4.sqlite3");
    let journal = root.join("service-1.4.sqlite3-journal");
    let connection = Connection::open(&database)?;
    connection.execute_batch("PRAGMA cache_size=1; BEGIN IMMEDIATE; CREATE TABLE interrupted_recovery_fixture(payload BLOB); INSERT INTO interrupted_recovery_fixture VALUES(zeroblob(131072));")?;
    let database_bytes = fs::read(&database)?;
    let journal_bytes = fs::read(&journal)?;
    assert_eq!(
        &journal_bytes[..8],
        &[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]
    );
    connection.execute_batch("ROLLBACK;")?;
    drop(connection);
    fs::write(database, database_bytes)?;
    fs::write(journal, journal_bytes)?;
    Ok(())
}

#[test]
fn stopped_registry_recovers_actual_hot_journal_without_changing_readonly_observation() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let namespace = Incarnation::from_bytes([161; 16]);
    drop(ServiceRegistry::open(root.path(), namespace)?);
    let before = fs::read(root.path().join("service-1.4.sqlite3"))?;
    let lease = ServiceLease::acquire(root.path())?;
    interrupted_transaction(root.path())?;
    let failure = ServiceRegistry::open_saved(root.path())
        .err()
        .context("hot readonly observation must refuse")?;
    assert!(
        matches!(failure.downcast_ref::<rusqlite::Error>(), Some(rusqlite::Error::SqliteFailure(error, _)) if error.extended_code == 776),
        "{failure:#}"
    );
    ServiceRegistry::recover_existing_after_stop(&lease, namespace)?;
    assert_eq!(fs::read(root.path().join("service-1.4.sqlite3"))?, before);
    assert!(!root.path().join("service-1.4.sqlite3-journal").exists());
    assert_eq!(
        ServiceRegistry::open_saved(root.path())?.incarnation()?,
        namespace
    );
    Ok(())
}

#[test]
fn stopped_registry_recovery_without_journal_is_byte_preserving() -> Result<()> {
    let root = tempfile::tempdir()?;
    let namespace = Incarnation::from_bytes([162; 16]);
    drop(ServiceRegistry::open(root.path(), namespace)?);
    let before = fs::read(root.path().join("service-1.4.sqlite3"))?;
    let lease = ServiceLease::acquire(root.path())?;
    ServiceRegistry::recover_existing_after_stop(&lease, namespace)?;
    assert_eq!(fs::read(root.path().join("service-1.4.sqlite3"))?, before);
    assert!(!root.path().join("service-1.4.sqlite3-journal").exists());
    Ok(())
}

#[test]
fn stopped_registry_recovery_refuses_missing_or_foreign_namespace_before_write() -> Result<()> {
    let root = tempfile::tempdir()?;
    let namespace = Incarnation::from_bytes([163; 16]);
    let lease = ServiceLease::acquire(root.path())?;
    assert!(ServiceRegistry::recover_existing_after_stop(&lease, namespace).is_err());
    assert!(!root.path().join("service-1.4.sqlite3").exists());
    drop(ServiceRegistry::open(root.path(), namespace)?);
    interrupted_transaction(root.path())?;
    let database = root.path().join("service-1.4.sqlite3");
    let journal = root.path().join("service-1.4.sqlite3-journal");
    let before = (fs::read(&database)?, fs::read(&journal)?);
    assert!(
        ServiceRegistry::recover_existing_after_stop(&lease, Incarnation::from_bytes([164; 16]))
            .is_err()
    );
    assert_eq!((fs::read(database)?, fs::read(journal)?), before);
    Ok(())
}

#[test]
fn stopped_registry_recovery_refuses_aliased_journal_before_write() -> Result<()> {
    let root = tempfile::tempdir()?;
    let namespace = Incarnation::from_bytes([165; 16]);
    drop(ServiceRegistry::open(root.path(), namespace)?);
    let lease = ServiceLease::acquire(root.path())?;
    interrupted_transaction(root.path())?;
    let database = root.path().join("service-1.4.sqlite3");
    let journal = root.path().join("service-1.4.sqlite3-journal");
    fs::hard_link(&journal, root.path().join("foreign-journal-alias"))?;
    let before = (fs::read(&database)?, fs::read(&journal)?);
    assert!(ServiceRegistry::recover_existing_after_stop(&lease, namespace).is_err());
    assert_eq!((fs::read(database)?, fs::read(journal)?), before);
    Ok(())
}

#[test]
fn stopped_registry_recovery_refuses_foreign_hot_database_under_matching_owner() -> Result<()> {
    let root = tempfile::tempdir()?;
    let foreign = tempfile::tempdir()?;
    let namespace = Incarnation::from_bytes([166; 16]);
    drop(ServiceRegistry::open(root.path(), namespace)?);
    drop(ServiceRegistry::open(
        foreign.path(),
        Incarnation::from_bytes([167; 16]),
    )?);
    interrupted_transaction(foreign.path())?;
    let database = root.path().join("service-1.4.sqlite3");
    let journal = root.path().join("service-1.4.sqlite3-journal");
    fs::copy(foreign.path().join("service-1.4.sqlite3"), &database)?;
    fs::copy(foreign.path().join("service-1.4.sqlite3-journal"), &journal)?;
    let lease = ServiceLease::acquire(root.path())?;
    assert_eq!(
        retained_namespace(root.path())?,
        namespace,
        "the pathname owner alone is insufficient"
    );
    let before = (fs::read(&database)?, fs::read(&journal)?);
    let error = ServiceRegistry::recover_existing_after_stop(&lease, namespace).unwrap_err();
    assert!(format!("{error:#}").contains("service registry ownership mismatch"));
    assert_eq!((fs::read(database)?, fs::read(journal)?), before);
    Ok(())
}
