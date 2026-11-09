// SPDX-License-Identifier: Apache-2.0
//! Metadata admission tests, with explicit native-proof stand-ins. Actual
//! archive/Current and native process exclusion are separate integration gates.
use super::*;
use airk_core::catalog_legacy_adoption::source_inventory_digest;

fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, LegacyAdoptionPlan)> {
    let (temp, mut registry, mut setup) = super::super::tests::fixture()?;
    // This fixture qualifies metadata admission only. Match the complete
    // production authority layout before changing its marker; a marker alone
    // cannot turn the authority-zero physical schema into authority one.
    let tx = registry.connection.transaction()?;
    super::super::super::apply_authority_v1_layout(&tx)?;
    tx.execute(
        "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
        [],
    )?;
    tx.commit()?;
    setup.setup_watch = false;
    let original_registration = ProjectRegistration::new(
        setup.registration.root.clone(),
        temp.path()
            .canonicalize()?
            .join("original-data")
            .to_str()
            .unwrap()
            .into(),
        setup.registration.collection.clone(),
    )?;
    let aliases = setup
        .sources
        .iter()
        .flat_map(|source| {
            source
                .aliases
                .iter()
                .map(|alias| (alias.clone(), source.canonical_path.clone()))
        })
        .collect();
    let digest = Sha256Digest::from_bytes([81; 32]);
    let source_inventory_sha256 = source_inventory_digest(
        &original_registration.root,
        &original_registration.collection,
        &aliases,
    )?;
    let plan = LegacyAdoptionPlan {
        schema: 1,
        setup,
        original_registration,
        origin_sha256: digest,
        config_sha256: digest,
        executable_sha256: digest,
        quiescence_sha256: digest,
        archive_inventory_sha256: digest,
        original_publication_sha256: digest,
        source_inventory_sha256,
    };
    plan.validate()?;
    Ok((temp, registry, plan))
}
fn no_background_or_authority(registry: &ServiceRegistry) -> Result<()> {
    let counts:(i64,i64,i64,i64,i64,i64)=registry.connection.query_row("SELECT (SELECT count(*) FROM service_watch),(SELECT count(*) FROM service_background),(SELECT count(*) FROM service_setup_pending),(SELECT count(*) FROM service_jobs),(SELECT count(*) FROM query_projects),(SELECT count(*) FROM index_access_grants)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)))?;
    assert_eq!(counts, (0, 0, 0, 0, 0, 0));
    Ok(())
}

// Real native metadata encodings, but no claim of a converted Current head.
fn checkpoint_capsule(
    legacy: bool,
) -> Result<(tempfile::TempDir, ServiceRegistry, LegacyAdoptionPlan)> {
    let (temp, mut registry, plan) = fixture()?;
    let reserved = if legacy {
        registry.reserve_legacy_adoption(&plan)?
    } else {
        registry.reserve_index_setup(&plan.setup)?
    };
    let control = registry.root.canonicalize()?;
    let metadata = super::super::metadata::Metadata::format_request(&control, &plan.setup, legacy)?;
    let storage_identity = framed_digest(
        b"airk/catalog-native-setup-storage/1",
        &[
            control.to_str().unwrap().as_bytes(),
            plan.setup.key.namespace.as_bytes(),
            plan.setup.registration.binding.as_bytes(),
            reserved.operation_id.as_bytes(),
        ],
    );
    let mut native = NativeIndexPreparation::from_validated_native_review(
        plan.setup.registration.clone(),
        plan.setup.sources.clone(),
        storage_identity,
        metadata.configuration,
        SetupPreparationReview {
            expected_catalog_revision: reserved.catalog_revision,
            expected_source_revision: reserved.source_revision,
        },
    )?;
    native.legacy_adoption = legacy.then(|| plan.identity()).transpose()?;
    registry.record_index_preparation(
        plan.setup.caller,
        &plan.setup.key,
        plan.setup.identity()?,
        &native,
    )?;
    let data = Path::new(&plan.setup.registration.data_root);
    let logical = data
        .join("collections-1.4")
        .join(&plan.setup.registration.collection);
    let slot = logical.join("incarnations").join(&metadata.slot);
    std::fs::create_dir_all(&slot)?;
    std::fs::write(
        data.join(crate::service::data_scope::OWNER),
        &metadata.owner,
    )?;
    for (name, bytes) in metadata.collection_files() {
        std::fs::write(logical.join(name), bytes)?;
    }
    std::fs::write(slot.join("roots.json"), &metadata.roots)?;
    Ok((temp, registry, plan))
}

#[test]
fn checkpoint_exact_prepared_adoption_without_current_gate_is_not_initialized() -> Result<()> {
    let (_temp, registry, plan) = checkpoint_capsule(true)?;
    let data = Path::new(&plan.setup.registration.data_root);
    let gate = data
        .join("collections-1.4")
        .join(&plan.setup.registration.collection)
        .join(".current-gates");
    assert!(!gate.exists());
    let first = crate::service::registry::checkpoint::snapshot(&registry.root)?;
    let publication = first
        .publications
        .iter()
        .find(|p| p.binding == plan.setup.registration.binding)
        .unwrap();
    assert!(matches!(
        publication.publication,
        crate::service::registry::checkpoint::PublicationState::NotInitialized
    ));
    assert!(
        first
            .indexes
            .iter()
            .any(|i| i.binding == plan.setup.registration.binding
                && i.source_import_state == "pending_metadata")
    );
    let second = crate::service::registry::checkpoint::snapshot(&registry.root)?;
    assert_eq!(first.digest()?, second.digest()?);
    assert!(!gate.exists(), "observation must not create a Current gate");
    no_background_or_authority(&registry)?;
    Ok(())
}

#[test]
fn checkpoint_ordinary_preparation_never_gains_adoption_empty_storage_exception() -> Result<()> {
    let (_temp, registry, _plan) = checkpoint_capsule(false)?;
    assert!(crate::service::registry::checkpoint::snapshot(&registry.root).is_err());
    Ok(())
}

#[test]
fn checkpoint_ready_adoption_missing_current_files_remains_an_error() -> Result<()> {
    let (_temp, mut registry, plan) = checkpoint_capsule(true)?;
    let native = NativeLegacyAdoptionCompletion {
        publication: publication(&plan)?,
        plan,
        pin: PublicationPin::MetadataOnly,
    };
    registry.complete_legacy_adoption(&native)?;
    assert!(crate::service::registry::checkpoint::snapshot(&registry.root).is_err());
    Ok(())
}

#[test]
fn checkpoint_prepared_adoption_refuses_changed_capsule_and_unknown_slot_files() -> Result<()> {
    for name in [
        "roots.json",
        "unexpected.bin",
        "current.sqlite3",
        "current-owner.json",
    ] {
        let (_temp, registry, plan) = checkpoint_capsule(true)?;
        let receipt = registry
            .index_setup_receipt(plan.setup.caller, &plan.setup.key)?
            .unwrap();
        let slot = Path::new(&plan.setup.registration.data_root)
            .join("collections-1.4")
            .join(&plan.setup.registration.collection)
            .join("incarnations")
            .join(receipt.operation_id.to_string());
        std::fs::write(slot.join(name), b"changed or incomplete publication")?;
        assert!(
            crate::service::registry::checkpoint::snapshot(&registry.root).is_err(),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn checkpoint_prepared_adoption_refuses_crossed_native_storage_proof() -> Result<()> {
    let (_temp, registry, plan) = checkpoint_capsule(true)?;
    let mut receipt = registry
        .index_setup_receipt(plan.setup.caller, &plan.setup.key)?
        .unwrap();
    receipt.preparation.as_mut().unwrap().storage_identity = Sha256Digest::from_bytes([0; 32]);
    registry.connection.execute(
        "UPDATE index_operation_receipts SET payload=?1 WHERE id=?2",
        params![
            serde_json::to_vec(&receipt)?,
            receipt.operation_id.to_string()
        ],
    )?;
    assert!(crate::service::registry::checkpoint::snapshot(&registry.root).is_err());
    Ok(())
}

#[test]
fn adoption_checkpoint_history_matches_actual_reserved_prepared_and_ready_transitions() -> Result<()>
{
    let (_temp, mut registry, plan) = fixture()?;
    let reserved = registry.reserve_legacy_adoption(&plan)?;
    let prepared = prepared(&mut registry, &plan)?;
    let expected = vec![reserved, prepared];
    assert_eq!(
        super::super::legacy_adoption_checkpoint_history(&expected[1])?,
        expected
    );
    let native = NativeLegacyAdoptionCompletion {
        publication: publication(&plan)?,
        plan,
        pin: PublicationPin::MetadataOnly,
    };
    let ready = registry.complete_legacy_adoption(&native)?.setup;
    assert_eq!(
        super::super::legacy_adoption_checkpoint_history(&ready)?,
        expected
    );
    Ok(())
}

#[test]
fn adoption_checkpoint_history_refuses_ordinary_and_impossible_native_ancestors() -> Result<()> {
    let (_temp, mut registry, plan) = fixture()?;
    let prepared = prepared(&mut registry, &plan)?;
    let mut ordinary = prepared.clone();
    ordinary.legacy_adoption = None;
    ordinary.preparation.as_mut().unwrap().legacy_adoption = None;
    assert!(super::super::legacy_adoption_checkpoint_history(&ordinary).is_err());
    let mut crossed = prepared;
    crossed.catalog_revision = crossed.catalog_revision.next()?;
    assert!(super::super::legacy_adoption_checkpoint_history(&crossed).is_err());
    let native = NativeLegacyAdoptionCompletion {
        publication: publication(&plan)?,
        plan,
        pin: PublicationPin::MetadataOnly,
    };
    let mut ready = registry.complete_legacy_adoption(&native)?.setup;
    ready.completion.as_mut().unwrap().expected_catalog_revision = ready.catalog_revision;
    assert!(super::super::legacy_adoption_checkpoint_history(&ready).is_err());
    Ok(())
}
fn prepared(
    registry: &mut ServiceRegistry,
    plan: &LegacyAdoptionPlan,
) -> Result<IndexSetupReceipt> {
    let before = registry.reserve_legacy_adoption(plan)?;
    let mut native = NativeIndexPreparation::from_validated_native_review(
        plan.setup.registration.clone(),
        plan.setup.sources.clone(),
        Sha256Digest::from_bytes([82; 32]),
        Sha256Digest::from_bytes([83; 32]),
        SetupPreparationReview {
            expected_catalog_revision: before.catalog_revision,
            expected_source_revision: before.source_revision,
        },
    )?;
    native.legacy_adoption = Some(plan.identity()?);
    registry.record_index_preparation(
        plan.setup.caller,
        &plan.setup.key,
        plan.setup.identity()?,
        &native,
    )
}
fn publication(plan: &LegacyAdoptionPlan) -> Result<LegacyAdoptionPublication> {
    Ok(LegacyAdoptionPublication {
        schema: 1,
        plan_digest: plan.identity()?,
        archive_inventory_sha256: plan.archive_inventory_sha256,
        original_publication_sha256: plan.original_publication_sha256,
        current: airk_core::identity::RevisionToken {
            incarnation: Incarnation::from_bytes([84; 16]),
            generation: 2,
            revision: Sha256Digest::from_bytes([85; 32]),
        },
        resource_sha256: Sha256Digest::from_bytes([86; 32]),
        compatibility_sha256: Sha256Digest::from_bytes([87; 32]),
    })
}
pub(super) fn completed_authority_fixture() -> Result<(
    tempfile::TempDir,
    ServiceRegistry,
    NativeLegacyAdoptionCompletion,
    LegacyAdoptionReceipt,
)> {
    let (temp, mut registry, plan) = fixture()?;
    prepared(&mut registry, &plan)?;
    let native = NativeLegacyAdoptionCompletion {
        publication: publication(&plan)?,
        plan,
        pin: PublicationPin::MetadataOnly,
    };
    let receipt = registry.complete_legacy_adoption(&native)?;
    Ok((temp, registry, native, receipt))
}
#[test]
fn adoption_reservation_arms_rollback_fence_before_any_filesystem_effect() -> Result<()> {
    let (_temp, mut registry, plan) = fixture()?;
    let receipt = registry.reserve_legacy_adoption(&plan)?;
    assert_eq!(receipt.legacy_adoption, Some(plan.identity()?));
    assert_eq!(
        registry
            .connection
            .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))?,
        13
    );
    assert!(!Path::new(&plan.setup.registration.data_root).exists());
    no_background_or_authority(&registry)?;
    assert!(registry.reserve_index_setup(&plan.setup).is_err());
    let mut changed = plan.clone();
    changed.config_sha256 = Sha256Digest::from_bytes([88; 32]);
    assert!(registry.reserve_legacy_adoption(&changed).is_err());
    assert_eq!(registry.reserve_legacy_adoption(&plan)?, receipt);
    Ok(())
}
#[test]
fn adoption_only_completion_is_exact_once_and_never_schedules_or_grants() -> Result<()> {
    let (_temp, mut registry, plan) = fixture()?;
    let prepared = prepared(&mut registry, &plan)?;
    let ordinary = SetupCompletion {
        prepared_digest: prepared.preparation.as_ref().unwrap().digest,
        expected_catalog_revision: prepared.catalog_revision,
        expected_source_revision: prepared.source_revision,
        legacy_adoption: None,
    };
    assert!(
        registry
            .complete_index_setup(
                plan.setup.caller,
                &plan.setup.key,
                plan.setup.identity()?,
                &ordinary
            )
            .is_err()
    );
    no_background_or_authority(&registry)?;
    // This explicit metadata stand-in cannot be constructed by a frontend:
    // production uses the Store-authenticated Current publication capability.
    let native = NativeLegacyAdoptionCompletion {
        plan: plan.clone(),
        publication: publication(&plan)?,
        pin: PublicationPin::MetadataOnly,
    };
    let ready = registry.complete_legacy_adoption(&native)?;
    ready.validate()?;
    assert_eq!(ready.setup.stage, SetupStage::Ready);
    no_background_or_authority(&registry)?;
    let changes = registry.connection.total_changes();
    assert_eq!(registry.complete_legacy_adoption(&native)?, ready);
    assert_eq!(registry.connection.total_changes(), changes);
    let mut changed = native;
    changed.publication.resource_sha256 = Sha256Digest::from_bytes([89; 32]);
    assert!(registry.complete_legacy_adoption(&changed).is_err());
    no_background_or_authority(&registry)?;
    Ok(())
}
#[test]
fn ordinary_native_preparation_cannot_consume_an_adoption_reservation() -> Result<()> {
    let (_temp, mut registry, plan) = fixture()?;
    let reserved = registry.reserve_legacy_adoption(&plan)?;
    let ordinary = NativeIndexPreparation::from_validated_native_review(
        plan.setup.registration.clone(),
        plan.setup.sources.clone(),
        Sha256Digest::from_bytes([82; 32]),
        Sha256Digest::from_bytes([83; 32]),
        SetupPreparationReview {
            expected_catalog_revision: reserved.catalog_revision,
            expected_source_revision: reserved.source_revision,
        },
    )?;
    assert!(
        registry
            .record_index_preparation(
                plan.setup.caller,
                &plan.setup.key,
                plan.setup.identity()?,
                &ordinary
            )
            .is_err()
    );
    assert_eq!(
        registry.index_setup_receipt(plan.setup.caller, &plan.setup.key)?,
        Some(reserved)
    );
    no_background_or_authority(&registry)?;
    Ok(())
}
#[test]
fn saved_archive_preparation_preserves_missing_source_labels_without_live_source_pins() -> Result<()>
{
    use super::super::storage::{NativeSetupPins, prepare_saved_adoption_storage};
    use crate::legacy_adoption::{
        CaptureLimits, LegacyScope, VerifiedLegacyArchive, capture_tree, copy_tree,
    };
    use airk_core::SnapshotStore;
    let (temp, mut registry, mut plan) = fixture()?;
    let original = Path::new(&plan.original_registration.data_root)
        .join("collections-1.2")
        .join(&plan.original_registration.collection);
    let store = crate::FileStore::new(&original);
    let mut snapshot = airk_core::Snapshot::empty(plan.original_registration.root.clone());
    snapshot.collection = plan.original_registration.collection.clone();
    snapshot.generation = 1;
    snapshot.roots = plan
        .setup
        .sources
        .iter()
        .flat_map(|s| {
            s.aliases
                .iter()
                .map(|a| (a.clone(), s.canonical_path.clone()))
        })
        .collect();
    let writer = store.lock()?;
    store.save(&snapshot)?;
    drop(writer);
    let limits = CaptureLimits {
        max_files: 128,
        max_bytes: 16 << 20,
        max_file_bytes: 8 << 20,
        max_depth: 12,
        max_snapshot_bytes: 8 << 20,
    };
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let inventory = capture_tree(&original, limits, until)?;
    let validation = temp.path().canonicalize()?.join("private-validation");
    std::fs::create_dir(&validation)?;
    copy_tree(
        &original,
        &validation,
        &inventory,
        limits,
        until,
        &mut || Ok(()),
    )?;
    let archive = VerifiedLegacyArchive::open_validation_copy(
        &validation,
        LegacyScope {
            root: snapshot.root.clone(),
            collection: snapshot.collection.clone(),
            roots: snapshot.roots.clone(),
        },
        &inventory,
        limits,
        until,
    )?;
    plan.archive_inventory_sha256 = archive.identity().archive_inventory_sha256;
    plan.original_publication_sha256 = archive.identity().original_root_sha256;
    for source in &plan.setup.sources {
        std::fs::remove_dir(&source.canonical_path)?;
    }
    assert!(super::super::validate_setup_metadata_plan(&registry.root, &plan.setup).is_err());
    let saved = SavedAdoptionSources::from_verified_archive(&plan, &archive)?;
    super::super::validate_saved_adoption_metadata_plan(&registry.root, &saved)?;
    let reserved = registry.reserve_legacy_adoption(&plan)?;
    let data = Path::new(&plan.setup.registration.data_root);
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .access_mode(0x80 | 0x20)
            .share_mode(3)
            .custom_flags(0x02000000 | 0x00200000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let pins =
        NativeSetupPins::from_native_review(options.open(data.parent().unwrap())?, None, vec![]);
    let prepared =
        prepare_saved_adoption_storage(&registry, &registry.root, &reserved, pins, saved)?
            .record_legacy_adoption(&mut registry, &plan)?;
    assert_eq!(
        prepared.preparation.unwrap().legacy_adoption,
        Some(plan.identity()?)
    );
    let slot = data
        .join("collections-1.4")
        .join(&plan.setup.registration.collection)
        .join("incarnations")
        .join(reserved.operation_id.to_string());
    let roots = crate::roots::RootConfig::load(&slot, &plan.original_registration.root)?;
    assert_eq!(roots.roots, snapshot.roots);
    assert!(
        plan.setup
            .sources
            .iter()
            .all(|s| !Path::new(&s.canonical_path).exists())
    );
    assert_eq!(capture_tree(&original, limits, until)?, inventory);
    no_background_or_authority(&registry)?;
    Ok(())
}

pub(super) fn complete_metadata_authority_fixture(
    registry: &mut ServiceRegistry,
    plan: LegacyAdoptionPlan,
) -> Result<(NativeLegacyAdoptionCompletion, LegacyAdoptionReceipt)> {
    prepared(registry, &plan)?;
    let native = NativeLegacyAdoptionCompletion {
        publication: publication(&plan)?,
        plan,
        pin: PublicationPin::MetadataOnly,
    };
    let receipt = registry.complete_legacy_adoption(&native)?;
    Ok((native, receipt))
}

/// Real native owner exclusion and the same reservation/preparation/completion
/// sequence used by the coordinator. Publication is explicitly metadata-only;
/// authenticated historical archive conversion remains an integration gate.
#[test]
fn adoption_existing_maintenance_writer_reserves_and_completes_without_bootstrap() -> Result<()> {
    let (_temp, registry, plan) = fixture()?;
    let control = registry.root.clone();
    let namespace = registry.incarnation()?;
    drop(registry);
    let owner_before = std::fs::read(control.join("service-1.4.owner.json"))?;
    let mut readonly = ServiceRegistry::open_saved(&control)?;
    assert!(readonly.reserve_legacy_adoption(&plan).is_err());
    assert!(
        readonly
            .index_setup_receipt(plan.setup.caller, &plan.setup.key)?
            .is_none()
    );
    drop(readonly);
    let lease = crate::service::ServiceLease::acquire(&control)?;
    assert!(
        ServiceRegistry::with_existing_maintenance(
            &lease,
            Incarnation::from_bytes([199; 16]),
            |_| Ok(())
        )
        .is_err()
    );
    let receipt = ServiceRegistry::with_existing_maintenance(&lease, namespace, |registry| {
        prepared(registry, &plan)?;
        let native = NativeLegacyAdoptionCompletion {
            publication: publication(&plan)?,
            plan: plan.clone(),
            pin: PublicationPin::MetadataOnly,
        };
        let receipt = registry.complete_legacy_adoption(&native)?;
        assert_eq!(registry.complete_legacy_adoption(&native)?, receipt);
        no_background_or_authority(registry)?;
        Ok(receipt)
    })?;
    assert_eq!(receipt.setup.stage, SetupStage::Ready);
    assert_eq!(
        std::fs::read(control.join("service-1.4.owner.json"))?,
        owner_before
    );
    let saved = ServiceRegistry::open_saved(&control)?;
    assert_eq!(
        saved
            .index_setup_receipt(plan.setup.caller, &plan.setup.key)?
            .unwrap(),
        receipt.setup
    );
    drop(saved);
    let other = tempfile::tempdir()?;
    let other_lease = crate::service::ServiceLease::acquire(other.path())?;
    assert!(
        ServiceRegistry::with_existing_maintenance(&other_lease, namespace, |_| Ok(())).is_err()
    );
    assert!(!other.path().join("service-1.4.sqlite3").exists());
    Ok(())
}

#[test]
fn adoption_runtime_layout_refuses_state_descendants_and_accepts_separate_data() -> Result<()> {
    let (state_owner, mut registry, mut plan) = fixture()?;
    let separate = tempfile::tempdir()?;
    let separate = separate.path().canonicalize()?;
    let state = state_owner.path().canonicalize()?;
    let install = separate.join("install");
    let logs = separate.join("logs");
    std::fs::create_dir(&install)?;
    std::fs::create_dir(&logs)?;
    let layout = serde_json::json!({"schema":1,"install":install,"state":state,"logs":logs,"channel":"stable","ready":true});
    std::fs::write(
        install.join("runtime-layout.json"),
        serde_json::to_vec(&layout)?,
    )?;
    std::fs::write(
        state.join("airk-state-1.4.owner.json"),
        serde_json::to_vec(&layout)?,
    )?;
    let source = separate.join("source").to_str().unwrap().to_owned();
    plan.original_registration = ProjectRegistration::new(
        source.clone(),
        separate.join("original").to_str().unwrap().into(),
        "fixture".into(),
    )?;
    plan.setup.sources = vec![SetupSource {
        canonical_path: source.clone(),
        aliases: vec![],
        owner: true,
    }];
    plan.source_inventory_sha256 =
        source_inventory_digest(&source, "fixture", &std::collections::BTreeMap::new())?;
    plan.setup.registration = ProjectRegistration::new(
        source.clone(),
        state
            .join("legacy-adoption/indexes/rejected")
            .to_str()
            .unwrap()
            .into(),
        "fixture".into(),
    )?;
    let readonly = ServiceRegistry::open_saved(&registry.root)?;
    assert!(
        readonly
            .preflight_source_isolation(&plan.setup.registration, &[])
            .is_err()
    );
    let remembered: i64 =
        readonly
            .connection
            .query_row("SELECT count(*) FROM service_private_paths", [], |row| {
                row.get(0)
            })?;
    assert_eq!(remembered, 0, "read-only preflight wrote protected paths");
    drop(readonly);
    assert!(
        registry
            .reserve_legacy_adoption(&plan)
            .unwrap_err()
            .is::<crate::service::registry::source_isolation::SourceBoundary>()
    );
    assert!(
        registry
            .index_setup_receipt(plan.setup.caller, &plan.setup.key)?
            .is_none()
    );
    assert!(!Path::new(&plan.setup.registration.data_root).exists());
    plan.setup.registration = ProjectRegistration::new(
        source,
        separate.join("adopted/index").to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let control = registry.root.clone();
    let namespace = registry.incarnation()?;
    drop(registry);
    let lease = crate::service::ServiceLease::acquire(&control)?;
    ServiceRegistry::with_existing_maintenance(&lease, namespace, |registry| {
        prepared(registry, &plan)?;
        let native = NativeLegacyAdoptionCompletion {
            publication: publication(&plan)?,
            plan: plan.clone(),
            pin: PublicationPin::MetadataOnly,
        };
        assert_eq!(
            registry.complete_legacy_adoption(&native)?.setup.stage,
            SetupStage::Ready
        );
        let proposed = ProjectRegistration::new(
            separate.join("other-source").to_str().unwrap().into(),
            plan.setup.registration.root.clone(),
            "other".into(),
        )?;
        let readonly = ServiceRegistry::open_saved(&control)?;
        assert!(
            readonly.preflight_source_isolation(&proposed, &[]).is_err(),
            "missing registered source accepted as a new data container"
        );
        assert!(!Path::new(&proposed.data_root).exists());
        Ok(())
    })
}
