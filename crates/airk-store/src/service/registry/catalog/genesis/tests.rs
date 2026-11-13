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

// ---- S3: retained airk-state/1 registry schema (compare-only) ----
// Fixtures: fixtures/schema/airk-state-1, captured from the 1.8.16 baseline
// (43d7005) by scripts/dev/s3_capture_schema.py. Never rewritten by tests.

const S3_REGISTRY_FIXTURES: &[(&str, &str, &str)] = &[
    (
        "service-1.4.schema11",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.schema11.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.schema11.meta.json"
        )),
    ),
    (
        "catalog-schema11-backup",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/catalog-schema11-backup.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/catalog-schema11-backup.meta.json"
        )),
    ),
    (
        "service-1.4.authority0",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.authority0.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.authority0.meta.json"
        )),
    ),
    (
        "service-1.4.authority1",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.authority1.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.authority1.meta.json"
        )),
    ),
    (
        "service-1.4.policy1",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy1.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy1.meta.json"
        )),
    ),
    (
        "service-1.4.policy2",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy2.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy2.meta.json"
        )),
    ),
    (
        "service-1.4.policy3",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy3.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy3.meta.json"
        )),
    ),
    (
        "service-1.4.authority1-armed",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.authority1-armed.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.authority1-armed.meta.json"
        )),
    ),
    (
        "service-1.4.policy1-armed",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy1-armed.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy1-armed.meta.json"
        )),
    ),
    (
        "service-1.4.policy2-armed",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy2-armed.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy2-armed.meta.json"
        )),
    ),
    (
        "service-1.4.policy3-armed",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy3-armed.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.policy3-armed.meta.json"
        )),
    ),
    (
        "service-1.4.adopted-policy0",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.adopted-policy0.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.adopted-policy0.meta.json"
        )),
    ),
    (
        "service-1.4.adopted-policy1",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.adopted-policy1.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.adopted-policy1.meta.json"
        )),
    ),
    (
        "service-1.4.adopted-policy2",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.adopted-policy2.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.adopted-policy2.meta.json"
        )),
    ),
    (
        "service-1.4.adopted-policy3",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.adopted-policy3.objects.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/schema/airk-state-1/service-1.4.adopted-policy3.meta.json"
        )),
    ),
];

fn s3_fixture(
    table: &[(&str, &str, &str)],
    label: &str,
) -> anyhow::Result<(serde_json::Value, serde_json::Value)> {
    use anyhow::Context as _;
    let (_, objects, meta) = table
        .iter()
        .find(|(name, _, _)| *name == label)
        .with_context(|| format!("retained schema fixture {label} is not listed"))?;
    Ok((serde_json::from_str(objects)?, serde_json::from_str(meta)?))
}

/// Same rendering as scripts/dev/s3_capture_schema.py: rows sorted by
/// (type, name, tbl_name), CRLF -> LF in sql only, rootpage omitted.
fn s3_observed(
    connection: &rusqlite::Connection,
) -> anyhow::Result<(serde_json::Value, serde_json::Value)> {
    let mut statement = connection.prepare("SELECT type,name,tbl_name,sql FROM sqlite_master")?;
    let mut rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
    let objects = rows
        .into_iter()
        .map(|(kind, name, table, sql)| {
            serde_json::json!({
                "type": kind,
                "name": name,
                "tbl_name": table,
                "sql": sql.map(|sql| sql.replace("\r\n", "\n")),
            })
        })
        .collect();
    let meta = serde_json::json!({
        "user_version": connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
        "application_id": connection.pragma_query_value(None, "application_id", |row| row.get::<_, i64>(0))?,
    });
    Ok((serde_json::Value::Array(objects), meta))
}

fn s3_assert(
    table: &[(&str, &str, &str)],
    connection: &rusqlite::Connection,
    label: &str,
) -> anyhow::Result<()> {
    let (objects, meta) = s3_fixture(table, label)?;
    let observed = s3_observed(connection)?;
    assert_eq!(
        observed.0, objects,
        "{label}: sqlite_master differs from the retained 1.8.16 fixture"
    );
    assert_eq!(
        observed.1, meta,
        "{label}: user_version/application_id differ from the retained 1.8.16 fixture"
    );
    Ok(())
}

struct S3Producer {
    root: PathBuf,
    operation: Incarnation,
}

impl activation::AuthorityV1ReadinessProducer for S3Producer {
    fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
        Ok(Vec::new())
    }
    fn qualify(
        &self,
        expected: &activation::AuthorityV1CandidateObservation,
    ) -> Result<activation::AuthorityV1NativeQualification> {
        Ok(activation::AuthorityV1NativeQualification {
            candidate: observe_authority_v1(&self.root, self.operation, expected.nonce)?,
            predecessor_refusals: Vec::new(),
        })
    }
}

/// Current Genesis through settlement, asserting each intermediate stage when
/// `check_stages`. Mirrors the capture driver's sequence exactly.
fn s3_settled(root: &Path, check_stages: bool) -> Result<super::super::super::ServiceRegistry> {
    let mut genesis = begin(root)?;
    let receipt = genesis.receipt()?;
    if check_stages {
        s3_assert(
            S3_REGISTRY_FIXTURES,
            &registry(&receipt.root, false)?,
            "service-1.4.schema11",
        )?;
    }
    let backed = genesis.backup()?;
    if check_stages {
        let attempt = backed
            .backups
            .iter()
            .find(|attempt| attempt.complete)
            .context("qualified genesis backup missing")?;
        let path = backup::validate(&backed.root, backed.operation, attempt)?;
        let saved = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        s3_assert(S3_REGISTRY_FIXTURES, &saved, "catalog-schema11-backup")?;
    }
    genesis.migrate()?;
    if check_stages {
        s3_assert(
            S3_REGISTRY_FIXTURES,
            &registry(&receipt.root, false)?,
            "service-1.4.authority0",
        )?;
    }
    genesis.promote_authority_v1()?;
    genesis.record_authority_v1_readiness(
        Incarnation::from_bytes([119; 16]),
        &S3Producer {
            root: receipt.root.clone(),
            operation: receipt.operation,
        },
    )?;
    assert_eq!(genesis.settle_authority_v1()?.stage, GenesisStage::Settled);
    drop(genesis);
    super::super::super::ServiceRegistry::open(root, receipt.namespace)
}

type S3Layer = (&'static str, fn(&Connection) -> Result<()>);

#[test]
fn current_genesis_and_layers_reproduce_retained_airk_state_1_registry_schema() -> Result<()> {
    let layers: [S3Layer; 3] = [
        ("policy1", super::super::apply_policy_v1_layout),
        ("policy2", super::super::apply_split_v2_layout),
        ("policy3", super::super::apply_split_extraction_v3_layout),
    ];
    for armed in [false, true] {
        let temp = tempfile::tempdir()?;
        let mut saved = s3_settled(temp.path(), !armed)?;
        let suffix = if armed { "-armed" } else { "" };
        if armed {
            let tx = saved.connection.transaction()?;
            crate::service::registry::runtime_contract::arm_in_transaction(&tx)?;
            tx.commit()?;
        }
        s3_assert(
            S3_REGISTRY_FIXTURES,
            &saved.connection,
            &format!("service-1.4.authority1{suffix}"),
        )?;
        for (name, apply) in layers {
            let tx = saved.connection.transaction()?;
            apply(&tx)?;
            tx.commit()?;
            s3_assert(
                S3_REGISTRY_FIXTURES,
                &saved.connection,
                &format!("service-1.4.{name}{suffix}"),
            )?;
        }
    }
    Ok(())
}

/// Reader acceptance: the current compiled layout recognizer accepts every
/// retained 1.8.16 registry shape, including adopted (schema 13) layouts that
/// current code no longer produces. The layout/version pairs are reviewed
/// values, not derived from the code under test.
#[test]
fn current_reader_recognizes_every_retained_airk_state_1_registry_layout() -> Result<()> {
    use super::super::CatalogLayout::*;
    let cases = [
        ("service-1.4.schema11", None, 11),
        ("catalog-schema11-backup", None, 11),
        ("service-1.4.authority0", Some(Schema12Authority0), 12),
        ("service-1.4.authority1", Some(Schema12Authority1), 12),
        ("service-1.4.policy1", Some(Schema12Authority1Policy1), 12),
        ("service-1.4.policy2", Some(Schema12Authority1Policy2), 12),
        ("service-1.4.policy3", Some(Schema12Authority1Policy3), 12),
        ("service-1.4.authority1-armed", Some(Schema12Authority1), 13),
        (
            "service-1.4.policy1-armed",
            Some(Schema12Authority1Policy1),
            13,
        ),
        (
            "service-1.4.policy2-armed",
            Some(Schema12Authority1Policy2),
            13,
        ),
        (
            "service-1.4.policy3-armed",
            Some(Schema12Authority1Policy3),
            13,
        ),
        ("service-1.4.adopted-policy0", Some(Schema12Authority1), 13),
        (
            "service-1.4.adopted-policy1",
            Some(Schema12Authority1Policy1),
            13,
        ),
        (
            "service-1.4.adopted-policy2",
            Some(Schema12Authority1Policy2),
            13,
        ),
        (
            "service-1.4.adopted-policy3",
            Some(Schema12Authority1Policy3),
            13,
        ),
    ];
    assert_eq!(cases.len(), S3_REGISTRY_FIXTURES.len());
    for (label, layout, version) in cases {
        let (objects, meta) = s3_fixture(S3_REGISTRY_FIXTURES, label)?;
        let text = |object: &serde_json::Value, field: &str| -> Result<String> {
            Ok(object[field]
                .as_str()
                .with_context(|| format!("{label}: {field} is not text"))?
                .to_owned())
        };
        let objects = objects
            .as_array()
            .context("fixture objects must be an array")?
            .iter()
            .map(|object| {
                Ok(super::super::SchemaObject {
                    kind: text(object, "type")?,
                    name: text(object, "name")?,
                    table: text(object, "tbl_name")?,
                    // schema_objects() reads NULL sql as an empty string.
                    sql: object["sql"].as_str().unwrap_or_default().to_owned(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        match layout {
            None => assert_eq!(&objects, super::super::expected(11)?, "{label}"),
            Some(layout) => assert_eq!(
                super::super::physical_catalog_layout(&objects)?,
                layout,
                "{label}"
            ),
        }
        assert_eq!(
            meta,
            serde_json::json!({
                "user_version": version,
                "application_id": super::super::super::APPLICATION,
            }),
            "{label}"
        );
    }
    Ok(())
}

// ---- U105: the lazily installed description layer ----
// Fixture: fixtures/split/agents/registry-policy3-descriptions.*, the retained
// policy3 objects plus the layer DDL rendered by Python sqlite3 (independent
// of this Rust path). Retained fixtures above never carry the layer.
const U105_DESCRIPTION_FIXTURE: &[(&str, &str, &str)] = &[(
    "registry-policy3-descriptions",
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/split/agents/registry-policy3-descriptions.objects.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/split/agents/registry-policy3-descriptions.meta.json"
    )),
)];

#[test]
fn description_layer_over_policy3_reproduces_its_reviewed_fixture() -> Result<()> {
    for (label, objects, _) in S3_REGISTRY_FIXTURES {
        assert!(
            !objects.contains("catalog_descriptions"),
            "{label}: a retained airk-state/1 fixture carries the U105 layer"
        );
    }
    let temp = tempfile::tempdir()?;
    let mut saved = s3_settled(temp.path(), false)?;
    for apply in [
        super::super::apply_policy_v1_layout,
        super::super::apply_split_v2_layout,
        super::super::apply_split_extraction_v3_layout,
    ] {
        let tx = saved.connection.transaction()?;
        apply(&tx)?;
        tx.commit()?;
    }
    s3_assert(
        S3_REGISTRY_FIXTURES,
        &saved.connection,
        "service-1.4.policy3",
    )?;
    let tx = saved.connection.transaction()?;
    super::super::descriptions::apply_layout(&tx)?;
    // Idempotent: a second install is a no-op over the same identity.
    super::super::descriptions::apply_layout(&tx)?;
    tx.commit()?;
    s3_assert(
        U105_DESCRIPTION_FIXTURE,
        &saved.connection,
        "registry-policy3-descriptions",
    )?;
    assert_eq!(
        super::super::validate_catalog_layout(&saved.connection)?,
        super::super::CatalogLayout::Schema12Authority1Policy3
    );
    super::super::validate_existing(&saved.connection)?;
    Ok(())
}
