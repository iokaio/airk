// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::service::registry::ServiceRegistry;
use airk_core::index_catalog::SourceRootId;
use rusqlite::params;

#[test]
fn unsupported_source_schema_retains_typed_contract_details_without_authorizing_rebuild()
-> Result<()> {
    let (root, _) = fixture()?;
    let connection = Connection::open(root.path().join(DATABASE))?;
    // A version marker alone cannot qualify this relabeled schema as an
    // authentic predecessor. The diagnostic reports incompatibility only.
    connection.pragma_update(None, "user_version", 8)?;
    drop(connection);
    let error = registry_read(root.path())
        .err()
        .context("old schema accepted")?;
    let rejection = error
        .downcast_ref::<CatalogMigrationSourceUnsupported>()
        .context("source incompatibility lost its typed identity")?;
    assert_eq!(rejection.observed_registry_schema, 8);
    assert_eq!(rejection.minimum_registry_schema, 11);
    assert_eq!(rejection.maximum_registry_schema, 13);
    assert_eq!(
        error.to_string(),
        "unsupported catalog upgrade source format"
    );

    let connection = Connection::open(root.path().join(DATABASE))?;
    connection.pragma_update(None, "application_id", 0)?;
    drop(connection);
    let foreign = registry_read(root.path())
        .err()
        .context("foreign registry accepted")?;
    assert!(
        foreign
            .downcast_ref::<CatalogMigrationSourceUnsupported>()
            .is_none()
    );
    Ok(())
}

pub(super) fn fixture() -> Result<(tempfile::TempDir, ProjectRegistration)> {
    let root = tempfile::tempdir()?;
    let registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    let registration = ProjectRegistration::new(
        "/fictional/unavailable-source".into(),
        "/fictional/unavailable-index".into(),
        "fixture".into(),
    )?;
    registry.connection.execute(
        "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,7,1,1)",
        params![
            registration.binding.to_string(),
            serde_json::to_vec(&registration)?
        ],
    )?;
    Ok((root, registration))
}

#[test]
fn migration_and_restore_preserve_jobs_receipts_watermarks_and_removal_history() -> Result<()> {
    use airk_core::service::{JobOutcome, RequestKey};
    let (root, registration) = fixture()?;
    let caller = Incarnation::from_bytes([51; 16]);
    let connection = Connection::open(root.path().join(DATABASE))?;
    connection.execute_batch("PRAGMA foreign_keys=ON;")?;
    let epoch: i64 = connection.query_row("SELECT request_epoch FROM service_meta", [], |row| {
        row.get(0)
    })?;
    let mut keys = Vec::new();
    for number in 1..=3u8 {
        let job = Sha256Digest::from_bytes([number + 50; 32]);
        connection.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,?3,'pending',7,0)",params![job.to_string(),registration.binding.to_string(),if number==2 {"inspect"} else {"index"}])?;
        let key = RequestKey {
            namespace: Incarnation::from_bytes([1; 16]),
            epoch: u64::try_from(epoch)?,
            nonce: Incarnation::from_bytes([number + 60; 16]),
        };
        connection.execute(
            "INSERT INTO service_requests VALUES(?1,?2,?3,?4,?5)",
            params![
                key.namespace.to_string(),
                epoch,
                key.nonce.to_string(),
                caller.to_string(),
                job.to_string()
            ],
        )?;
        keys.push((job, key));
    }
    connection.execute(
        "UPDATE service_jobs SET state='running',attempts=1 WHERE id=?1",
        [keys[1].0.to_string()],
    )?;
    connection.execute(
        "INSERT INTO service_job_source VALUES(?1,4)",
        [keys[2].0.to_string()],
    )?;
    connection.execute(
        "UPDATE service_jobs SET state='failed',attempts=2,outcome=?1 WHERE id=?2",
        params![
            serde_json::to_vec(&JobOutcome::Failed {
                code: "fixture_parser_unavailable".into()
            })?,
            keys[2].0.to_string()
        ],
    )?;
    connection.execute(
        "INSERT INTO service_job_cancellation VALUES(?1)",
        [keys[1].0.to_string()],
    )?;
    connection.execute(
        "INSERT INTO service_watch VALUES(?1,0,9,4,3,?2,1,0,'disabled',NULL,100,90,0,0,NULL)",
        params![
            registration.binding.to_string(),
            serde_json::to_vec(&vec!["/fictional/saved-alias"])?
        ],
    )?;
    connection.execute(
        "INSERT INTO service_removed_projects VALUES(?1,?2,55)",
        params![
            Sha256Digest::from_bytes([71; 32]).to_string(),
            Sha256Digest::from_bytes([72; 32]).to_string()
        ],
    )?;
    super::super::backup::integrity(&connection)?;
    let original = super::super::backup::legacy_data_digest(&connection)?;
    drop(connection);
    let registry = ServiceRegistry::open_saved(root.path())?;
    let jobs = keys
        .iter()
        .map(|(job, _)| -> Result<_> { Ok(serde_json::to_value(registry.job(*job)?)?) })
        .collect::<Result<Vec<_>>>()?;
    drop(registry);
    let owner = CatalogUpgrade::acquire(root.path())?;
    owner.admit(plan(&owner, registration.clone())?)?;
    let receipt = owner.migrate()?;
    assert_eq!(receipt.legacy_data_digest, Some(original));
    assert_eq!(
        super::super::backup::legacy_data_digest(&registry_read(root.path())?)?,
        original
    );
    owner.restore()?;
    let restored = ServiceRegistry::open_saved(root.path())?;
    for ((id, key), expected) in keys.iter().zip(jobs) {
        assert_eq!(serde_json::to_value(restored.job(*id)?)?, expected);
        let (receipt, expired) = restored.request_receipt(caller, key)?;
        assert!(!expired);
        assert_eq!(serde_json::to_value(receipt.unwrap())?, expected);
    }
    assert!(restored.get(registration.binding)?.paused);
    assert_eq!(
        super::super::backup::legacy_data_digest(&restored.connection)?,
        original
    );
    Ok(())
}

#[test]
fn known_private_source_configuration_survives_offline_sources_without_initialization() -> Result<()>
{
    use crate::{roots::RootConfig, service::collection::Collection};
    let owner = tempfile::tempdir()?;
    let canonical = owner.path().canonicalize()?;
    let control = canonical.join("control");
    let data = canonical.join("data");
    let source = canonical.join("source");
    let additional = canonical.join("additional");
    for directory in [&control, &data, &source, &additional] {
        fs::create_dir(directory)?;
    }
    fs::write(
        source.join("source-sentinel"),
        b"source bytes are read-only",
    )?;
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let registry = ServiceRegistry::open(&control, Incarnation::from_bytes([1; 16]))?;
    registry.connection.execute(
        "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,7,1,1)",
        params![
            registration.binding.to_string(),
            serde_json::to_vec(&registration)?
        ],
    )?;
    drop(registry);
    let collection = Collection::initialize(
        &data,
        &registration.root,
        &registration.collection,
        Incarnation::from_bytes([80; 16]),
    )?;
    let selected = collection.select()?;
    let writer = selected.store.lock()?;
    let directory = selected.store.directory().to_path_buf();
    let mut config = RootConfig::load(&directory, &registration.root)?;
    config
        .roots
        .insert("specs".into(), additional.to_str().unwrap().into());
    config.save(&directory)?;
    let policy = directory.join("extraction-policy.json");
    let opaque = br#"{"schema":99,"retained_policy_fixture":"must not be rewritten"}"#;
    fs::write(&policy, opaque)?;
    let before = fs::read(directory.join("roots.json"))?;
    drop(writer);
    drop(selected);
    let moved = canonical.join("offline-source");
    let moved_additional = canonical.join("offline-additional");
    ensure!(
        source.starts_with(&canonical)
            && additional.starts_with(&canonical)
            && moved.starts_with(&canonical)
            && moved_additional.starts_with(&canonical),
        "fixture move escaped its owned root"
    );
    fs::rename(&source, &moved)?;
    fs::rename(&additional, &moved_additional)?;
    let upgrade = CatalogUpgrade::acquire(&control)?;
    let configuration = upgrade.source_configuration(&registration)?;
    assert_eq!(configuration.source_import, SourceImportState::Complete);
    assert!(configuration.digest.is_some());
    assert_eq!(
        configuration.aliases.get("specs"),
        Some(&additional.to_str().unwrap().to_owned())
    );
    let mut complete = plan(&upgrade, registration.clone())?;
    complete.indexes[0].source_import = configuration.source_import;
    complete.indexes[0].configuration_digest = configuration.digest;
    complete.indexes[0].sources.push(SourceRoot {
        id: SourceRootId::from_identity(Incarnation::from_bytes([81; 16])),
        canonical_path: additional.to_str().unwrap().into(),
        aliases: vec!["specs".into()],
        owner: false,
    });
    upgrade.admit(complete)?;
    upgrade.migrate()?;
    let read = registry_read(&control)?;
    assert_eq!(
        read.query_row("SELECT count(*) FROM index_sources", [], |row| row
            .get::<_, i64>(0))?,
        2
    );
    assert_eq!(
        read.query_row("SELECT alias FROM index_source_aliases", [], |row| row
            .get::<_, String>(
            0
        ))?,
        "specs"
    );
    drop(read);
    upgrade.restore()?;
    assert_eq!(fs::read(directory.join("roots.json"))?, before);
    assert_eq!(fs::read(&policy)?, opaque);
    assert_eq!(
        fs::read(moved.join("source-sentinel"))?,
        b"source bytes are read-only"
    );
    assert!(
        !source.exists() && !additional.exists(),
        "metadata read must not recreate source directories"
    );
    Ok(())
}

#[test]
fn service_lease_transfer_keeps_exclusion_through_migration_without_reacquisition() -> Result<()> {
    let (root, registration) = fixture()?;
    let service = ServiceLease::acquire(root.path())?;
    let owner = CatalogUpgrade::acquire_with_service(service)?;
    let plan = plan(&owner, registration)?;
    owner.admit(plan)?;
    owner.migrate()?;
    assert!(ServiceLease::acquire(root.path()).is_err());
    let service = owner.into_service_lease();
    assert!(ServiceLease::acquire(root.path()).is_err());
    assert!(
        ServiceRegistry::open_saved(root.path()).is_err(),
        "lease transfer cannot bypass settlement"
    );
    // Migration-only leases are released, while the single service owner stays.
    let embedding = EmbeddingLease::acquire(root.path())?;
    let watcher = SourceWatchLease::acquire(root.path())?;
    drop((embedding, watcher, service));
    CatalogUpgrade::acquire(root.path())?;
    Ok(())
}
pub(super) fn plan(
    owner: &CatalogUpgrade,
    registration: ProjectRegistration,
) -> Result<UpgradePlan> {
    Ok(UpgradePlan {
        schema: 1,
        id: Incarnation::from_bytes([2; 16]),
        control: owner.control().into(),
        namespace: owner.namespace(),
        installer_identity: Sha256Digest::from_bytes([3; 32]),
        legacy_schema_digest: owner.source_schema_digest()?,
        registrations_digest: owner.registration_digest()?,
        indexes: vec![BackfillIndex {
            sources: vec![SourceRoot {
                id: SourceRootId::from_identity(Incarnation::from_bytes([4; 16])),
                canonical_path: registration.root.clone(),
                aliases: vec![],
                owner: true,
            }],
            registration,
            index_id: IndexId::from_identity(Incarnation::from_bytes([5; 16])),
            project_id: QueryProjectId::from_identity(Incarnation::from_bytes([6; 16])),
            source_import: SourceImportState::PendingMetadata,
            configuration_digest: None,
        }],
    })
}

#[test]
fn admission_fences_ordinary_readers_and_resumes_the_exact_saved_ids() -> Result<()> {
    let (root, registration) = fixture()?;
    let database = root.path().join(DATABASE);
    let before = fs::read(&database)?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let plan = plan(&owner, registration)?;
    let receipt = owner.admit(plan.clone())?;
    assert_eq!(receipt.stage, UpgradeStage::Admission);
    assert!(
        ServiceRegistry::open_saved(root.path())
            .err()
            .context("expected pending admission")?
            .to_string()
            .contains("upgrade is pending")
    );
    assert!(ServiceRegistry::open(root.path(), owner.namespace()).is_err());
    assert_eq!(
        fs::read(&database)?,
        before,
        "admission cannot mutate the registry"
    );
    drop(owner);
    let owner = CatalogUpgrade::acquire(root.path())?;
    let resumed = owner.admit(plan.clone())?;
    assert_eq!(resumed.plan, plan);
    assert_eq!(resumed.plan_digest, receipt.plan_digest);
    assert_eq!(fs::read(&database)?, before);
    Ok(())
}

#[test]
fn interrupted_intent_before_owner_fence_can_arm_on_resume() -> Result<()> {
    let (root, registration) = fixture()?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let plan = plan(&owner, registration)?;
    let receipt = UpgradeReceipt {
        schema: 1,
        plan_digest: plan.identity()?,
        plan: plan.clone(),
        stage: UpgradeStage::Admission,
        legacy_data_digest: None,
        wal: None,
        backups: vec![],
        readiness: None,
        authority_v1_host: None,
        authority_v1: None,
        retirement_recovery: None,
    };
    persist(
        &owner.control().join(INTENT),
        &serde_json::to_vec(&receipt)?,
    )?;
    assert!(
        ServiceRegistry::open_saved(root.path()).is_ok(),
        "no data migration happened before the fence"
    );
    drop(owner);
    let before = fs::read(root.path().join(DATABASE))?;
    assert!(
        CatalogUpgrade::resume_for_installer(root.path(), Sha256Digest::from_bytes([198; 32]))
            .is_err()
    );
    assert!(fs::read(root.path().join(DATABASE))? == before);
    let owner = CatalogUpgrade::resume_for_installer(root.path(), plan.installer_identity)?;
    owner.admit(plan)?;
    assert!(ServiceRegistry::open_saved(root.path()).is_err());
    Ok(())
}

#[test]
fn unknown_worker_ownership_blocks_without_an_intent_or_registry_changes() -> Result<()> {
    let (root, _) = fixture()?;
    let namespace = super::super::super::retained_namespace(root.path())?;
    let before = fs::read(root.path().join(DATABASE))?;
    let worker = ProjectLease::acquire(root.path(), Sha256Digest::from_bytes([19; 32]))?;
    assert!(CatalogUpgrade::acquire(root.path()).is_err());
    assert!(pending(root.path())?.is_none());
    assert_eq!(fs::read(root.path().join(DATABASE))?, before);
    assert_eq!(
        super::super::super::retained_namespace(root.path())?,
        namespace
    );
    drop(worker);
    CatalogUpgrade::acquire(root.path())?;
    Ok(())
}

#[test]
fn stopped_or_pending_lifecycle_refuses_initial_and_pre_fence_catalog_admission() -> Result<()> {
    use airk_core::service::RequestKey;
    use airk_core::service_lifecycle::{LifecycleAction, LifecyclePhase, LifecycleRequest};
    for completed_stop in [false, true] {
        for saved_intent in [false, true] {
            let (root, registration) = fixture()?;
            let mut registry =
                ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
            let (namespace, epoch) = registry.request_epoch()?;
            let request = LifecycleRequest {
                caller: Incarnation::from_bytes([184; 16]),
                key: RequestKey {
                    namespace,
                    epoch,
                    nonce: Incarnation::from_bytes([185; 16]),
                },
                action: if completed_stop {
                    LifecycleAction::Stop
                } else {
                    LifecycleAction::Restart
                },
                expected_instance: Incarnation::from_bytes([186; 16]),
                runtime: Sha256Digest::from_bytes([187; 32]),
                manager: false,
            };
            let lifecycle = registry.begin_lifecycle(&request)?;
            registry.advance_lifecycle(
                &lifecycle,
                if completed_stop {
                    LifecyclePhase::Complete
                } else {
                    LifecyclePhase::Starting
                },
            )?;
            drop(registry);
            let owner = CatalogUpgrade::acquire(root.path())?;
            let plan = plan(&owner, registration)?;
            if saved_intent {
                let receipt = UpgradeReceipt {
                    schema: 1,
                    plan_digest: plan.identity()?,
                    plan: plan.clone(),
                    stage: UpgradeStage::Admission,
                    legacy_data_digest: None,
                    wal: None,
                    backups: vec![],
                    readiness: None,
                    authority_v1_host: None,
                    authority_v1: None,
                    retirement_recovery: None,
                };
                persist(
                    &owner.control().join(INTENT),
                    &serde_json::to_vec(&receipt)?,
                )?;
            }
            let before = fs::read(root.path().join(DATABASE))?;
            let error = owner.admit(plan).unwrap_err();
            assert!(
                error.to_string().contains(if completed_stop {
                    "service_explicitly_stopped"
                } else {
                    "resume_or_abandon_pending_lifecycle_first"
                }),
                "{error:#}"
            );
            assert!(fs::read(root.path().join(DATABASE))? == before);
            assert_eq!(pending(root.path())?.is_some(), saved_intent);
            let saved = ServiceRegistry::open_saved(root.path())?;
            assert_eq!(saved.service_explicitly_stopped()?, completed_stop);
            assert_eq!(saved.pending_lifecycle()?.is_some(), !completed_stop);
        }
    }
    Ok(())
}

#[test]
fn pending_source_metadata_cannot_invent_extra_roots_or_replace_the_original() -> Result<()> {
    let (root, registration) = fixture()?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let mut plan = plan(&owner, registration)?;
    let mut extra = plan.indexes[0].sources[0].clone();
    extra.id = SourceRootId::from_identity(Incarnation::from_bytes([8; 16]));
    extra.canonical_path = "/fictional/unrequested".into();
    extra.owner = false;
    plan.indexes[0].sources.push(extra);
    assert!(owner.admit(plan).is_err());
    assert!(pending(root.path())?.is_none());
    assert!(ServiceRegistry::open_saved(root.path()).is_ok());
    Ok(())
}

#[test]
fn qualified_backup_preserves_all_legacy_rows_and_refuses_a_changed_digest() -> Result<()> {
    let (root, registration) = fixture()?;
    let original = fs::read(root.path().join(DATABASE))?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    owner.admit(plan(&owner, registration)?)?;
    let receipt = owner.backup()?;
    assert_eq!(receipt.stage, UpgradeStage::Backup);
    assert_eq!(receipt.backups.len(), 1);
    let attempt = &receipt.backups[0];
    assert!(attempt.complete);
    let path = super::super::backup::validate(owner.control(), receipt.plan.id, attempt)?;
    let snapshot = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    assert_eq!(
        super::super::backup::legacy_data_digest(&snapshot)?,
        receipt.legacy_data_digest.unwrap()
    );
    assert_eq!(
        snapshot.query_row("SELECT epoch,paused FROM projects", [], |row| Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, bool>(1)?
        )))?,
        (7, true)
    );
    drop(snapshot);
    assert_eq!(fs::read(root.path().join(DATABASE))?, original);
    let mut bytes = fs::read(&path)?;
    bytes[100] ^= 1;
    fs::write(&path, &bytes)?;
    assert!(owner.backup().is_err());
    assert_eq!(
        fs::read(&path)?,
        bytes,
        "qualified backups are never silently overwritten"
    );
    assert_eq!(fs::read(root.path().join(DATABASE))?, original);
    Ok(())
}

#[test]
fn unclaimed_backup_slot_is_preserved_and_a_bounded_alternative_is_qualified() -> Result<()> {
    let (root, registration) = fixture()?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let receipt = owner.admit(plan(&owner, registration)?)?;
    let unclaimed = BackupAttempt::reserved(0).path(owner.control(), receipt.plan.id);
    fs::write(&unclaimed, b"Unclaimed fixture metadata must survive")?;
    let completed = owner.backup()?;
    assert_eq!(completed.backups.len(), 2);
    assert!(completed.backups[0].abandoned);
    assert!(completed.backups[1].complete);
    assert_eq!(
        fs::read(&unclaimed)?,
        b"Unclaimed fixture metadata must survive"
    );
    drop(owner);
    let owner = CatalogUpgrade::acquire(root.path())?;
    let resumed = owner.backup()?;
    assert_eq!(resumed.backups[1].sha256, completed.backups[1].sha256);
    Ok(())
}

#[test]
fn migration_is_atomic_and_resumes_both_sides_of_commit_without_reallocating_ids() -> Result<()> {
    for fault in [
        "migration_intent",
        "before_migration_commit",
        "after_migration_commit",
    ] {
        let (root, registration) = fixture()?;
        let owner = CatalogUpgrade::acquire(root.path())?;
        let plan = plan(&owner, registration)?;
        owner.admit(plan.clone())?;
        assert!(
            owner
                .migrate_at(|stage, _| {
                    ensure!(stage != fault, "injected migration interruption");
                    Ok(())
                })
                .is_err()
        );
        let interrupted = registry_read(owner.control())?;
        let version: u32 =
            interrupted.pragma_query_value(None, "user_version", |row| row.get(0))?;
        assert_eq!(
            version,
            if fault == "after_migration_commit" {
                12
            } else {
                11
            }
        );
        super::super::backup::integrity(&interrupted)?;
        drop(interrupted);
        drop(owner);
        let owner = CatalogUpgrade::acquire(root.path())?;
        let receipt = owner.migrate()?;
        assert_eq!(receipt.stage, UpgradeStage::Imports);
        assert_eq!(receipt.plan, plan);
        let read = registry_read(owner.control())?;
        assert_eq!(
            read.query_row("SELECT id FROM index_definitions", [], |row| row
                .get::<_, String>(0))?,
            plan.indexes[0].index_id.to_string()
        );
        assert_eq!(
            read.query_row("SELECT id FROM query_projects", [], |row| row
                .get::<_, String>(0))?,
            plan.indexes[0].project_id.to_string()
        );
        assert_eq!(
            read.query_row("SELECT epoch,paused FROM projects", [], |row| Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, bool>(1)?
            )))?,
            (7, true)
        );
        assert!(
            ServiceRegistry::open_saved(owner.control()).is_err(),
            "unqualified migration stays fenced"
        );
        assert_eq!(owner.migrate()?.plan_digest, receipt.plan_digest);
    }
    Ok(())
}

// Invoked only by the process-level interruption tests below. exit deliberately
// skips Rust/SQLite destructors, leaving the actual committed WAL or hot journal.
#[test]
fn catalog_crash_fixture_child() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_CATALOG_CRASH_FIXTURE") else {
        return Ok(());
    };
    let root = PathBuf::from(root);
    match std::env::var("AIRK_CATALOG_CRASH_KIND")?.as_str() {
        "wal" => {
            let connection = Connection::open(root.join(DATABASE))?;
            connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA wal_autocheckpoint=0; UPDATE projects SET epoch=9;")?;
            std::process::exit(73);
        }
        "journal" => {
            let owner = CatalogUpgrade::acquire(&root)?;
            owner.migrate_at(|stage, connection| {
                if stage == "before_migration_commit" {
                    connection
                        .context("transaction checkpoint missing")?
                        .cache_flush()?;
                    std::process::exit(73);
                }
                Ok(())
            })?;
            anyhow::bail!("migration did not reach the interruption point")
        }
        _ => anyhow::bail!("unknown crash fixture"),
    }
}

fn crash(root: &Path, kind: &str) -> Result<()> {
    let output = std::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "service::registry::catalog::upgrade::tests::catalog_crash_fixture_child",
            "--nocapture",
        ])
        .env("AIRK_CATALOG_CRASH_FIXTURE", root)
        .env("AIRK_CATALOG_CRASH_KIND", kind)
        .output()?;
    ensure!(
        output.status.code() == Some(73),
        "crash fixture failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn committed_wal_survives_process_exit_and_is_included_in_the_qualified_backup() -> Result<()> {
    let (root, registration) = fixture()?;
    crash(root.path(), "wal")?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    owner.admit(plan(&owner, registration)?)?;
    let receipt = owner.backup()?;
    assert!(
        receipt
            .wal
            .as_ref()
            .is_some_and(|wal| wal.valid_frames > wal.uncommitted_frames)
    );
    let snapshot =
        super::super::backup::validate(owner.control(), receipt.plan.id, &receipt.backups[0])?;
    let snapshot = Connection::open_with_flags(snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    assert_eq!(
        snapshot.query_row("SELECT epoch FROM projects", [], |row| row.get::<_, i64>(0))?,
        9
    );
    drop(snapshot);
    owner.migrate()?;
    assert_eq!(
        registry_read(owner.control())?
            .query_row("SELECT epoch FROM projects", [], |row| row.get::<_, i64>(0))?,
        9
    );
    Ok(())
}

#[test]
fn owned_hot_journal_recovers_only_after_saved_worker_exclusion_and_resumes() -> Result<()> {
    let (root, registration) = fixture()?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let plan = plan(&owner, registration)?;
    owner.admit(plan.clone())?;
    owner.backup()?;
    drop(owner);
    crash(root.path(), "journal")?;
    let journal = root.path().join(format!("{DATABASE}-journal"));
    let before = fs::read(&journal)?;
    let database = fs::read(root.path().join(DATABASE))?;
    let exclusion = PendingCatalogExclusion::acquire(root.path())?;
    assert_eq!(exclusion.installer_identity(), plan.installer_identity);
    assert_eq!(
        fs::read(&journal)?,
        before,
        "process exclusion must not recover SQLite"
    );
    assert_eq!(fs::read(root.path().join(DATABASE))?, database);
    assert!(
        CatalogUpgrade::acquire(root.path()).is_err(),
        "exclusion retains all ownership leases"
    );
    drop(exclusion);
    assert!(before.len() > 512);
    assert!(
        registry_read(root.path()).is_err(),
        "real hot journal requires writable SQLite recovery"
    );
    let worker = ProjectLease::acquire(root.path(), plan.indexes[0].registration.binding)?;
    assert!(CatalogUpgrade::acquire(root.path()).is_err());
    assert_eq!(
        fs::read(&journal)?,
        before,
        "failed exclusion must not recover the journal"
    );
    drop(worker);
    let wrong_installer = Sha256Digest::from_bytes([199; 32]);
    let mismatch = CatalogUpgrade::resume_for_installer(root.path(), wrong_installer)
        .err()
        .context("crossed installer must refuse before recovering SQLite")?;
    assert!(format!("{mismatch:#}").contains("another verified installer payload"));
    assert_eq!(fs::read(&journal)?, before);
    assert!(fs::read(root.path().join(DATABASE))? == database);
    let service = ServiceLease::acquire(root.path())?;
    assert!(CatalogUpgrade::acquire_with_service_for_installer(service, wrong_installer).is_err());
    assert_eq!(fs::read(&journal)?, before);
    assert!(fs::read(root.path().join(DATABASE))? == database);
    let owner =
        PendingCatalogExclusion::acquire(root.path())?.into_upgrade(plan.installer_identity)?;
    assert!(ServiceLease::acquire(root.path()).is_err());
    assert_eq!(
        registry_read(root.path())?
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        11
    );
    assert_eq!(owner.migrate()?.plan, plan);
    Ok(())
}

#[test]
fn partial_backup_preserves_sidecars_and_refuses_qualified_overlays() -> Result<()> {
    let (root, registration) = fixture()?;
    let owner = CatalogUpgrade::acquire(root.path())?;
    let receipt = owner.admit(plan(&owner, registration)?)?;
    let source = registry_read(root.path())?;
    let rows = super::super::backup::legacy_data_digest(&source)?;
    let mut attempt = BackupAttempt::reserved(0);
    assert!(
        super::super::backup::capture(
            owner.control(),
            receipt.plan.id,
            &source,
            rows,
            &mut attempt,
            |_| anyhow::bail!("interrupted after claiming the empty file")
        )
        .is_err()
    );
    let path = attempt.path(owner.control(), receipt.plan.id);
    let overlay = path.with_file_name(format!(
        "{}-journal",
        path.file_name().unwrap().to_string_lossy()
    ));
    let sentinel = b"unclaimed sidecar must survive";
    fs::write(&overlay, sentinel)?;
    assert!(!super::super::backup::capture(
        owner.control(),
        receipt.plan.id,
        &source,
        rows,
        &mut attempt,
        |_| Ok(())
    )?);
    assert_eq!(fs::metadata(&path)?.len(), 0);
    assert_eq!(fs::read(&overlay)?, sentinel);
    let mut receipt = owner.receipt()?;
    attempt.abandoned = true;
    receipt.stage = UpgradeStage::Backup;
    receipt.legacy_data_digest = Some(rows);
    receipt.backups.push(attempt);
    owner.save(&receipt)?;
    drop(source);
    let completed = owner.backup()?;
    assert_eq!(completed.backups.len(), 2);
    assert!(completed.backups[1].complete);
    assert_eq!(fs::read(&overlay)?, sentinel);
    let path = completed.backups[1].path(owner.control(), completed.plan.id);
    let wal = path.with_file_name(format!(
        "{}-wal",
        path.file_name().unwrap().to_string_lossy()
    ));
    fs::write(&wal, sentinel)?;
    assert!(owner.backup().is_err());
    assert_eq!(fs::read(&wal)?, sentinel);
    Ok(())
}

#[test]
fn hot_journal_recovery_refuses_unproven_ownership_without_changing_bytes() -> Result<()> {
    for invalid in ["fence", "stage", "snapshot"] {
        let (root, registration) = fixture()?;
        let owner = CatalogUpgrade::acquire(root.path())?;
        let receipt = owner.admit(plan(&owner, registration)?)?;
        owner.backup()?;
        drop(owner);
        crash(root.path(), "journal")?;
        match invalid {
            "fence" => {
                let path = root.path().join("service-1.4.owner.json");
                let mut record: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
                record.as_object_mut().unwrap().remove("catalog_upgrade");
                fs::write(path, serde_json::to_vec(&record)?)?;
            }
            "stage" => {
                let mut record = pending(root.path())?.unwrap();
                record.stage = UpgradeStage::Backup;
                persist(
                    &record.plan.control.join(INTENT),
                    &serde_json::to_vec(&record)?,
                )?;
            }
            "snapshot" => {
                let record = pending(root.path())?.unwrap();
                let path = record.backups[0].path(&receipt.plan.control, record.plan.id);
                let mut bytes = fs::read(&path)?;
                bytes[100] ^= 1;
                fs::write(path, bytes)?;
            }
            _ => unreachable!(),
        }
        let database = fs::read(root.path().join(DATABASE))?;
        let journal = root.path().join(format!("{DATABASE}-journal"));
        let before = fs::read(&journal)?;
        assert!(CatalogUpgrade::acquire(root.path()).is_err());
        assert_eq!(fs::read(&journal)?, before);
        assert_eq!(fs::read(root.path().join(DATABASE))?, database);
    }
    Ok(())
}

#[test]
fn interrupted_migration_rejects_changed_backfill_authority_instead_of_accepting_counts()
-> Result<()> {
    for change in [
        "UPDATE project_index_scopes SET kind='exact',path='unexpected.txt'",
        "UPDATE index_sources SET enabled=0",
        "UPDATE project_index_memberships SET revision=2",
        "DELETE FROM project_default_indexes",
    ] {
        let (root, registration) = fixture()?;
        let owner = CatalogUpgrade::acquire(root.path())?;
        owner.admit(plan(&owner, registration)?)?;
        owner.migrate()?;
        let connection = Connection::open(root.path().join(DATABASE))?;
        connection.execute_batch(change)?;
        drop(connection);
        let before = fs::read(root.path().join(DATABASE))?;
        assert!(owner.migrate().is_err());
        assert_eq!(fs::read(root.path().join(DATABASE))?, before);
    }
    Ok(())
}
