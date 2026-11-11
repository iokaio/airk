// SPDX-License-Identifier: Apache-2.0
use super::*;

fn identity() -> (Sha256Digest, Incarnation) {
    (
        Sha256Digest::from_bytes([117; 32]),
        Incarnation::from_bytes([118; 16]),
    )
}

fn begin(root: &Path) -> Result<CatalogGenesis> {
    let (installer, operation) = identity();
    CatalogGenesis::begin(ServiceLease::acquire(root)?, installer, operation)
}

#[test]
fn fresh_genesis_uses_real_backup_and_stays_inactive_across_exact_resume() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut genesis = begin(root.path())?;
    assert_eq!(genesis.receipt()?.stage, GenesisStage::Admitted);
    assert!(super::super::super::ServiceRegistry::open_saved(root.path()).is_err());
    let backed = genesis.backup()?;
    assert_eq!(backed.stage, GenesisStage::BackedUp);
    let selected = backed
        .backups
        .iter()
        .find(|attempt| attempt.complete)
        .unwrap();
    let path = backup::validate(&backed.root, backed.operation, selected)?;
    assert_eq!(Some(backup::digest(&path)?.1), selected.sha256);
    let saved = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    empty_legacy(&saved, backed.namespace)?;
    assert_eq!(
        backup::legacy_data_digest(&saved)?,
        backed.legacy_data_digest
    );
    drop(saved);
    let result = genesis.migrate()?;
    assert_eq!(result.stage, GenesisStage::Migrated);
    let read = registry(&result.root, false)?;
    let state = super::super::state(&read)?;
    assert!(!state.activated && !state.accepted_new_writes);
    assert_eq!((state.authority_contract, state.policy_contract), (0, 0));
    assert_eq!(Some(state.backup_digest), selected.sha256);
    assert!(!result.root.join("index-catalog-upgrade.json").exists());
    drop(read);
    let lease = genesis.into_service_lease();
    assert!(ServiceLease::acquire(root.path()).is_err());
    let (installer, operation) = identity();
    let mut resumed = CatalogGenesis::begin(lease, installer, operation)?;
    assert_eq!(resumed.migrate()?.stage, GenesisStage::Migrated);
    assert!(super::super::super::ServiceRegistry::open_saved(root.path()).is_err());
    Ok(())
}

#[test]
fn populated_history_or_changed_singletons_refuse_genesis_without_writing() -> Result<()> {
    for sql in [
        "UPDATE service_meta SET request_epoch=1",
        "UPDATE service_meta SET revision=1",
        "UPDATE service_lifecycle_state SET stopped=1",
        "UPDATE service_lifecycle_state SET instance='00112233445566778899aabbccddeeff'",
        "INSERT INTO service_private_paths VALUES('/fictional/existing-private-path')",
        "INSERT INTO service_recovery_budget VALUES('helper','0000000000000000000000000000000000000000000000000000000000000000',0,0,0,0)",
        "INSERT INTO service_lifecycle(id,request,phase) VALUES('old',X'7b7d','complete'); DELETE FROM service_lifecycle",
    ] {
        let root = tempfile::tempdir()?;
        let existing = super::super::super::ServiceRegistry::open(root.path(), identity().1)?;
        existing.connection.execute_batch(sql)?;
        drop(existing);
        let before = fs::read(root.path().join(DATABASE))?;
        let owner = fs::read(root.path().join("service-1.4.owner.json"))?;
        assert!(begin(root.path()).is_err(), "must refuse {sql}");
        assert_eq!(fs::read(root.path().join(DATABASE))?, before);
        assert_eq!(fs::read(root.path().join("service-1.4.owner.json"))?, owner);
        assert!(pending(root.path())?.is_none());
    }
    Ok(())
}

#[test]
fn competing_intents_unexplained_fences_and_missing_owned_database_refuse() -> Result<()> {
    let root = tempfile::tempdir()?;
    let existing = super::super::super::ServiceRegistry::open(root.path(), identity().1)?;
    drop(existing);
    let upgrade = root.path().join("index-catalog-upgrade.json");
    fs::write(&upgrade, b"unclaimed upgrade marker")?;
    assert!(begin(root.path()).is_err());
    assert_eq!(fs::read(&upgrade)?, b"unclaimed upgrade marker");
    fs::remove_file(upgrade)?;
    let canonical = root.path().canonicalize()?;
    let mut owner = super::super::super::owner_record(&canonical, true, identity().1)?;
    owner.catalog_upgrade = Some(Incarnation::from_bytes([119; 16]));
    owner.complete(&canonical)?;
    assert!(begin(root.path()).is_err());
    assert!(pending(root.path())?.is_none());

    let missing = tempfile::tempdir()?;
    drop(super::super::super::ServiceRegistry::open(
        missing.path(),
        identity().1,
    )?);
    fs::remove_file(missing.path().join(DATABASE))?;
    let owner = fs::read(missing.path().join("service-1.4.owner.json"))?;
    assert!(begin(missing.path()).is_err());
    assert!(!missing.path().join(DATABASE).exists());
    assert_eq!(
        fs::read(missing.path().join("service-1.4.owner.json"))?,
        owner
    );
    Ok(())
}

#[test]
fn unknown_backup_is_preserved_and_qualified_backup_replacement_refuses() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut genesis = begin(root.path())?;
    let before = genesis.receipt()?;
    let occupied = backup::BackupAttempt::reserved(0).path(&before.root, before.operation);
    fs::write(&occupied, b"unclaimed backup")?;
    let result = genesis.backup()?;
    assert!(result.backups[0].abandoned);
    assert!(result.backups[1].complete);
    assert_eq!(fs::read(occupied)?, b"unclaimed backup");
    let selected = result.backups[1].path(&result.root, result.operation);
    let mut bytes = fs::read(&selected)?;
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&selected, &bytes)?;
    assert!(genesis.migrate().is_err());
    assert_eq!(fs::read(&selected)?, bytes);
    let read = registry(&result.root, false)?;
    assert_eq!(
        read.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        11
    );
    Ok(())
}

#[test]
fn interrupted_admission_and_post_commit_receipt_gap_resume_exactly() -> Result<()> {
    let root = tempfile::tempdir()?;
    let genesis = begin(root.path())?;
    let admitted = genesis.receipt()?;
    drop(genesis);
    // The only allowed unfenced intent is the exact still-empty Admitted gap.
    let mut owner = super::super::super::owner_record(&admitted.root, true, admitted.namespace)?;
    owner.catalog_upgrade = None;
    owner.complete(&admitted.root)?;
    let mut genesis = begin(root.path())?;
    genesis.backup()?;
    let failure = genesis.migrate_at(|_, stage| {
        ensure!(
            stage != "after_commit",
            "simulated interruption before receipt publication"
        );
        Ok(())
    });
    assert!(failure.is_err());
    assert_eq!(genesis.receipt()?.stage, GenesisStage::Migrating);
    drop(genesis);
    let mut resumed = begin(root.path())?;
    assert_eq!(resumed.migrate()?.stage, GenesisStage::Migrated);
    Ok(())
}

// Only invoked with a unique owned fixture root by the interruption test below.
// Exiting deliberately skips Rust/SQLite cleanup and leaves a real hot journal.
#[test]
fn genesis_crash_fixture_child() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_GENESIS_CRASH_FIXTURE") else {
        return Ok(());
    };
    let mut genesis = begin(Path::new(&root))?;
    genesis.migrate_at(|connection, stage| {
        if stage == "before_commit" {
            connection.cache_flush()?;
            std::process::exit(73);
        }
        Ok(())
    })?;
    anyhow::bail!("genesis did not reach crash checkpoint")
}

#[test]
fn own_hot_journal_requires_exact_intent_backup_and_all_process_exclusion() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut genesis = begin(root.path())?;
    genesis.backup()?;
    drop(genesis);
    let output = std::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "service::registry::catalog::genesis::tests::genesis_crash_fixture_child",
            "--nocapture",
        ])
        .env("AIRK_GENESIS_CRASH_FIXTURE", root.path())
        .output()?;
    ensure!(
        output.status.code() == Some(73),
        "genesis crash fixture: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let journal = root.path().join(format!("{DATABASE}-journal"));
    let before = fs::read(&journal)?;
    assert!(before.len() > 512);
    let database = fs::read(root.path().join(DATABASE))?;
    let original_intent = fs::read(root.path().join(INTENT))?;
    let (installer, operation) = identity();
    assert!(
        CatalogGenesis::begin(
            ServiceLease::acquire(root.path())?,
            Sha256Digest::from_bytes([120; 32]),
            operation
        )
        .is_err()
    );
    assert_eq!(fs::read(&journal)?, before);
    assert_eq!(fs::read(root.path().join(DATABASE))?, database);
    let worker = ProjectLease::acquire(root.path(), Sha256Digest::from_bytes([121; 32]))?;
    assert!(begin(root.path()).is_err());
    assert_eq!(fs::read(&journal)?, before);
    drop(worker);

    let mut invalid = pending(root.path())?.unwrap();
    invalid.stage = GenesisStage::BackedUp;
    persist(&invalid, false)?;
    assert!(begin(root.path()).is_err());
    assert_eq!(fs::read(&journal)?, before);
    fs::write(root.path().join(INTENT), &original_intent)?;

    let canonical = root.path().canonicalize()?;
    let original_owner = fs::read(canonical.join("service-1.4.owner.json"))?;
    let mut owner = super::super::super::owner_record(&canonical, true, operation)?;
    owner.catalog_upgrade = Some(Incarnation::from_bytes([122; 16]));
    owner.complete(&canonical)?;
    assert!(begin(root.path()).is_err());
    assert_eq!(fs::read(&journal)?, before);
    fs::write(canonical.join("service-1.4.owner.json"), original_owner)?;

    let receipt = pending(root.path())?.unwrap();
    let snapshot = receipt
        .backups
        .iter()
        .find(|attempt| attempt.complete)
        .unwrap()
        .path(&canonical, operation);
    let original_backup = fs::read(&snapshot)?;
    let mut corrupt = original_backup.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    fs::write(&snapshot, corrupt)?;
    assert!(begin(root.path()).is_err());
    assert_eq!(fs::read(&journal)?, before);
    assert_eq!(fs::read(canonical.join(DATABASE))?, database);
    fs::write(snapshot, original_backup)?;

    let mut resumed =
        CatalogGenesis::begin(ServiceLease::acquire(root.path())?, installer, operation)?;
    assert_eq!(resumed.migrate()?.stage, GenesisStage::Migrated);
    assert!(!journal.exists());
    Ok(())
}

#[test]
fn unexplained_journal_refuses_before_initial_admission() -> Result<()> {
    let root = tempfile::tempdir()?;
    drop(super::super::super::ServiceRegistry::open(
        root.path(),
        identity().1,
    )?);
    let journal = root.path().join(format!("{DATABASE}-journal"));
    fs::write(&journal, b"not an admitted migration journal")?;
    let before = fs::read(root.path().join(DATABASE))?;
    assert!(begin(root.path()).is_err());
    assert_eq!(fs::read(&journal)?, b"not an admitted migration journal");
    assert_eq!(fs::read(root.path().join(DATABASE))?, before);
    assert!(pending(root.path())?.is_none());
    Ok(())
}
