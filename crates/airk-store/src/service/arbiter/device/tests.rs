// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::arbiter::{device::*, *};
fn binding() -> DeviceCatalogBinding {
    DeviceCatalogBinding {
        catalog: Sha256Digest::from_bytes([10; 32]),
        source: Sha256Digest::from_bytes([11; 32]),
    }
}
fn fixture() -> Result<(tempfile::TempDir, ConfigurationStore)> {
    let root = tempfile::tempdir()?;
    let lease = Arc::new(ServiceLease::acquire(root.path())?);
    let store = ConfigurationStore::open(
        lease,
        Sha256Digest::from_bytes([1; 32]),
        Incarnation::from_bytes([2; 16]),
    )?;
    Ok((root, store))
}
fn profile() -> Result<Profile> {
    let mut value: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    value.stage = Some(Stage::Focus);
    value.model = ModelChoice::FocusQwenReranker06B;
    value.manifest = Sha256Digest::from_bytes([3; 32]);
    value.cpu_fit = Some(CpuFit::default());
    value.qualification = None;
    value.qualified_targets.clear();
    value.tasks = std::collections::BTreeSet::from([Task::FocusRank]);
    value.task_policies =
        std::collections::BTreeMap::from([(Task::FocusRank, Sha256Digest::from_bytes([9; 32]))]);
    value.abi = ScoreAbi {
        readout: Readout::FocusYesNo,
        inputs: ["input_ids", "attention_mask", "position_ids"]
            .into_iter()
            .map(|name| TensorContract {
                name: name.into(),
                element: TensorType::Int64,
                axes: vec!["batch".into(), "tokens".into()],
            })
            .collect(),
        output: TensorContract {
            name: "logits".into(),
            element: TensorType::Float32,
            axes: vec!["batch".into(), "labels".into()],
        },
    };
    value.max_tokens = 512;
    value.validate()?;
    Ok(value)
}
fn enabled(store: &ConfigurationStore) -> Result<Configuration> {
    let mut value = store.load()?;
    value.revision += 1;
    value.focus.enabled = true;
    value.focus.selected_profile = Some(Sha256Digest::from_bytes([3; 32]));
    value.focus.indexes = vec![Enrollment {
        index_id: airk_core::index_catalog::IndexId::from_identity(Incarnation::from_bytes(
            [4; 16],
        )),
        tasks: std::collections::BTreeSet::from([Task::FocusRank]),
    }];
    store.apply(value.revision - 1, &value)
}
#[test]
fn absent_device_load_is_disabled_without_creating_state() -> Result<()> {
    let (root, store) = fixture()?;
    let StoredConfiguration::Device(value) = store.device(binding()).load()? else {
        panic!("unexpected legacy");
    };
    assert!(!value.focus.enabled);
    assert_eq!(value.focus.device_policy, DevicePolicy::Auto);
    assert!(!root.path().join(DIRECTORY).exists());
    Ok(())
}
#[test]
fn auto_migration_archives_exact_three_bytes_and_legacy_manager_refuses_four() -> Result<()> {
    let (root, store) = fixture()?;
    let old = enabled(&store)?;
    let raw = old.canonical_bytes()?;
    let profile = profile()?;
    let migrated = store.device(binding()).load_or_migrate(
        |pin| (pin == profile.manifest).then_some(&profile),
        || Ok(()),
    )?;
    assert!(migrated.focus.enabled);
    assert_eq!(migrated.focus.selected_model, Some(SelectedModel::Focus));
    assert_eq!(migrated.focus.device_policy, DevicePolicy::Auto);
    assert_eq!(
        serde_json::to_value(&migrated.focus.indexes)?,
        serde_json::to_value(&old.focus.indexes)?
    );
    assert_eq!(
        std::fs::read(
            root.path()
                .join(DIRECTORY)
                .join(format!("configuration-v3-{}.retained", digest(&raw)))
        )?,
        raw
    );
    assert!(store.load().is_err());
    assert!(store.apply(old.revision, &old).is_err());
    assert_eq!(
        store.device(binding()).migrate_checked(
            old.revision,
            digest(&raw),
            |pin| (pin == profile.manifest).then_some(&profile),
            || Ok(())
        )?,
        migrated
    );
    Ok(())
}
#[test]
fn wrong_hash_expiry_and_conflicting_archive_preserve_active_legacy_bytes() -> Result<()> {
    let (root, store) = fixture()?;
    let old = enabled(&store)?;
    let raw = old.canonical_bytes()?;
    let profile = profile()?;
    assert!(
        store
            .device(binding())
            .migrate_checked(
                old.revision,
                Sha256Digest::from_bytes([99; 32]),
                |_| Some(&profile),
                || Ok(())
            )
            .is_err()
    );
    assert!(
        store
            .device(binding())
            .migrate_checked(
                old.revision,
                digest(&raw),
                |_| Some(&profile),
                || anyhow::bail!("original deadline expired")
            )
            .is_err()
    );
    let archive = root
        .path()
        .join(DIRECTORY)
        .join(format!("configuration-v3-{}.retained", digest(&raw)));
    std::fs::write(&archive, b"conflicting retained bytes")?;
    assert!(
        store
            .device(binding())
            .migrate_checked(old.revision, digest(&raw), |_| Some(&profile), || Ok(()))
            .is_err()
    );
    assert_eq!(
        std::fs::read(root.path().join(DIRECTORY).join(CONFIGURATION))?,
        raw
    );
    assert_eq!(std::fs::read(archive)?, b"conflicting retained bytes");
    Ok(())
}
#[test]
fn current_cpu_preference_cas_and_catalog_source_binding_are_preserved() -> Result<()> {
    let (_root, store) = fixture()?;
    let initial = store
        .device(binding())
        .initialize_checked(None, || Ok(()))?;
    let mut candidate = initial.clone();
    candidate.revision += 1;
    candidate.context.device_policy = DevicePolicy::Cpu;
    candidate.context.gpu_precision = GpuPrecisionPreference::Fp16;
    assert_eq!(
        store
            .device(binding())
            .apply_checked(initial.revision, &candidate, || Ok(()))?,
        candidate
    );
    assert!(
        store
            .device(binding())
            .apply_checked(initial.revision, &candidate, || Ok(()))
            .is_err()
    );
    let mut wrong = binding();
    wrong.source = Sha256Digest::from_bytes([12; 32]);
    assert!(store.device(wrong).load().is_err());
    let observed = store
        .device(binding())
        .with_current(|| Ok(()), |current| Ok(current.clone()))?;
    assert_eq!(observed.context.device_policy, DevicePolicy::Cpu);
    assert_eq!(observed.context.gpu_precision, GpuPrecisionPreference::Fp16);
    Ok(())
}
#[test]
fn old_v2_initialization_remains_explicit_disabled_and_preserves_bytes() -> Result<()> {
    let (root, store) = fixture()?;
    let old = root.path().join("arbiter-v2");
    std::fs::create_dir(&old)?;
    let bytes = include_bytes!("../../../../../../fixtures/arbiter/configuration-disabled-v2.json");
    std::fs::write(old.join(CONFIGURATION), bytes)?;
    assert!(store.device(binding()).load().is_err());
    assert!(
        store
            .device(binding())
            .initialize_checked(None, || Ok(()))
            .is_err()
    );
    let current = store
        .device(binding())
        .initialize_checked(Some(digest(bytes)), || Ok(()))?;
    assert!(!current.focus.enabled && !current.context.enabled);
    assert_eq!(std::fs::read(old.join(CONFIGURATION))?, bytes);
    Ok(())
}

#[test]
fn expiry_after_synced_archive_and_staged_four_keeps_three_then_explicit_recovery() -> Result<()> {
    use std::cell::Cell;
    let (root, store) = fixture()?;
    let old = enabled(&store)?;
    let raw = old.canonical_bytes()?;
    let profile = profile()?;
    let calls = Cell::new(0u32);
    let expired = || {
        calls.set(calls.get() + 1);
        if calls.get() == 6 {
            anyhow::bail!("original management clock expired before publish");
        }
        Ok(())
    };
    assert!(
        store
            .device(binding())
            .migrate_checked(
                old.revision,
                digest(&raw),
                |pin| (pin == profile.manifest).then_some(&profile),
                expired
            )
            .is_err()
    );
    assert_eq!(calls.get(), 6);
    assert_eq!(
        std::fs::read(root.path().join(DIRECTORY).join(CONFIGURATION))?,
        raw
    );
    assert_eq!(
        std::fs::read(
            root.path()
                .join(DIRECTORY)
                .join(format!("configuration-v3-{}.retained", digest(&raw)))
        )?,
        raw
    );
    let current = store.device(binding()).migrate_checked(
        old.revision,
        digest(&raw),
        |pin| (pin == profile.manifest).then_some(&profile),
        || Ok(()),
    )?;
    assert_eq!(current.revision, old.revision + 1);
    assert!(current.focus.enabled);
    Ok(())
}

#[test]
fn recovered_archive_and_stage_require_real_fsync_and_preserve_original_custody() -> Result<()> {
    use std::cell::Cell;
    let (root, store) = fixture()?;
    let old = enabled(&store)?;
    let raw = old.canonical_bytes()?;
    let profile = profile()?;
    let directory = store.root.open_dir(OsStr::new(DIRECTORY))?;
    let archive_name = format!("configuration-v3-{}.retained", digest(&raw));
    let stage_name = format!("device-configuration-{}.part", old.revision + 1);
    let calls = Cell::new(0u64);
    let fail_archive = |_: &File| -> Result<()> {
        calls.set(calls.get() + 1);
        anyhow::bail!("injected fsync failure after complete bytes");
    };
    assert!(
        store
            .device(binding())
            .migrate_synced(
                old.revision,
                digest(&raw),
                |pin| (pin == profile.manifest).then_some(&profile),
                || Ok(()),
                &fail_archive
            )
            .is_err()
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(
        std::fs::read(root.path().join(DIRECTORY).join(CONFIGURATION))?,
        raw
    );
    let archive = directory.open_file(OsStr::new(&archive_name), SetupFileMode::Read)?;
    let archive_id = crate::store_fs::file_identity(&archive)?;
    assert_eq!(bounded(&archive)?, raw);
    drop(archive);
    let calls = Cell::new(0u64);
    let fail_stage = |file: &File| -> Result<()> {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            anyhow::bail!("injected stage fsync failure");
        }
        file.sync_all()?;
        Ok(())
    };
    assert!(
        store
            .device(binding())
            .migrate_synced(
                old.revision,
                digest(&raw),
                |pin| (pin == profile.manifest).then_some(&profile),
                || Ok(()),
                &fail_stage
            )
            .is_err()
    );
    assert_eq!(calls.get(), 2);
    let stage = directory.open_file(OsStr::new(&stage_name), SetupFileMode::Read)?;
    let stage_id = crate::store_fs::file_identity(&stage)?;
    let stage_bytes = bounded(&stage)?;
    drop(stage);
    // Both branches now recover matching complete files. A fake bytes==durable
    // implementation would skip these sync callbacks and publish incorrectly.
    let calls = Cell::new(0u64);
    let fail_recovered_archive = |_: &File| -> Result<()> {
        calls.set(calls.get() + 1);
        anyhow::bail!("recovered archive is not durable");
    };
    assert!(
        store
            .device(binding())
            .migrate_synced(
                old.revision,
                digest(&raw),
                |pin| (pin == profile.manifest).then_some(&profile),
                || Ok(()),
                &fail_recovered_archive
            )
            .is_err()
    );
    assert_eq!(calls.get(), 1);
    let calls = Cell::new(0u64);
    let fail_recovered_stage = |file: &File| -> Result<()> {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            anyhow::bail!("recovered stage is not durable");
        }
        file.sync_all()?;
        Ok(())
    };
    assert!(
        store
            .device(binding())
            .migrate_synced(
                old.revision,
                digest(&raw),
                |pin| (pin == profile.manifest).then_some(&profile),
                || Ok(()),
                &fail_recovered_stage
            )
            .is_err()
    );
    assert_eq!(calls.get(), 2);
    assert_eq!(
        std::fs::read(root.path().join(DIRECTORY).join(CONFIGURATION))?,
        raw
    );
    let archive = directory.open_file(OsStr::new(&archive_name), SetupFileMode::Read)?;
    assert_eq!(crate::store_fs::file_identity(&archive)?, archive_id);
    drop(archive);
    let stage = directory.open_file(OsStr::new(&stage_name), SetupFileMode::Read)?;
    assert_eq!(crate::store_fs::file_identity(&stage)?, stage_id);
    assert_eq!(bounded(&stage)?, stage_bytes);
    drop(stage);
    let current = store.device(binding()).migrate_checked(
        old.revision,
        digest(&raw),
        |pin| (pin == profile.manifest).then_some(&profile),
        || Ok(()),
    )?;
    assert_eq!(current.revision, old.revision + 1);
    let published = directory.open_file(OsStr::new(CONFIGURATION), SetupFileMode::Read)?;
    assert_eq!(crate::store_fs::file_identity(&published)?, stage_id);
    Ok(())
}

fn catalog_profiles() -> Result<Vec<Profile>> {
    let focus = profile()?;
    let mut compact = focus.clone();
    compact.model = ModelChoice::ContextQwenCompact;
    compact.stage = Some(Stage::Context);
    compact.manifest = Sha256Digest::from_bytes([4; 32]);
    compact.tasks = std::collections::BTreeSet::from([Task::ContextAction]);
    compact.task_policies = std::collections::BTreeMap::from([(
        Task::ContextAction,
        Sha256Digest::from_bytes([9; 32]),
    )]);
    compact.abi = ScoreAbi {
        readout: Readout::DecisionSymbol,
        inputs: vec![
            TensorContract {
                name: "input_ids".into(),
                element: TensorType::Int64,
                axes: vec!["batch_1".into(), "tokens".into()],
            },
            TensorContract {
                name: "attention_mask".into(),
                element: TensorType::Int64,
                axes: vec!["batch_1".into(), "tokens".into()],
            },
            TensorContract {
                name: "position_ids".into(),
                element: TensorType::Int64,
                axes: vec!["batch_1".into(), "tokens".into()],
            },
            TensorContract {
                name: "decision_positions".into(),
                element: TensorType::Int64,
                axes: vec!["batch_1".into()],
            },
            TensorContract {
                name: "candidate_positions".into(),
                element: TensorType::Int64,
                axes: vec!["batch_1".into(), "options".into()],
            },
            TensorContract {
                name: "option_mask".into(),
                element: TensorType::Bool,
                axes: vec!["batch_1".into(), "options".into()],
            },
        ],
        output: TensorContract {
            name: "scores".into(),
            element: TensorType::Float32,
            axes: vec!["batch_1".into(), "options".into()],
        },
    };
    for value in [&focus, &compact] {
        value.validate()?;
    }
    Ok(vec![focus, compact])
}
fn catalog(profiles: &[Profile]) -> Result<DeviceCatalog> {
    use airk_core::arbiter::device::*;
    let row = |i: usize| -> Result<VariantReference> {
        Ok(VariantReference {
            manifest: profiles[i].manifest,
            execution_identity: profiles[i].device_execution_identity()?,
        })
    };
    Ok(DeviceCatalog {
        schema: DEVICE_CATALOG_SCHEMA.into(),
        focus: vec![row(0)?],
        arbiter: vec![row(1)?],
    })
}
#[test]
fn native_four_to_four_rebind_preserves_settings_and_rejects_unknown_or_stale_authority()
-> Result<()> {
    let (root, store) = fixture()?;
    let profiles = catalog_profiles()?;
    let old_catalog = catalog(&profiles)?;
    let new_catalog = old_catalog.clone();
    let resolve = |pin| profiles.iter().find(|profile| profile.manifest == pin);
    let old_binding = old_catalog.binding(Sha256Digest::from_bytes([40; 32]), resolve)?;
    let new_binding = new_catalog.binding(Sha256Digest::from_bytes([41; 32]), resolve)?;
    let original = store
        .device(old_binding.clone())
        .initialize_checked(None, || Ok(()))?;
    let mut settings = original.clone();
    settings.revision += 1;
    settings.focus.enabled = true;
    settings.focus.selected_model = Some(SelectedModel::Focus);
    settings.focus.device_policy = DevicePolicy::Cpu;
    settings.focus.gpu_precision = GpuPrecisionPreference::Fp16;
    settings.focus.indexes = vec![Enrollment {
        index_id: airk_core::index_catalog::IndexId::from_identity(Incarnation::from_bytes(
            [4; 16],
        )),
        tasks: std::collections::BTreeSet::from([Task::FocusRank]),
    }];
    store
        .device(old_binding.clone())
        .apply_checked(original.revision, &settings, || Ok(()))?;
    let raw = settings.canonical_bytes()?;
    let observed = store
        .device(new_binding.clone())
        .inspect_device_binding()?
        .unwrap();
    assert_eq!(observed.binding, old_binding);
    assert_eq!(observed.configuration_sha256, digest(&raw));
    assert!(store.device(new_binding.clone()).load().is_err()); // strict ordinary decoding is not relaxed
    let authorize = |old: &DeviceCatalogBinding, new: &DeviceCatalogBinding| -> Result<()> {
        ensure!(
            old == &old_binding && new == &new_binding,
            "native compiled transition owner mismatch"
        );
        Ok(())
    };
    let denied = |_: &DeviceCatalogBinding, _: &DeviceCatalogBinding| -> Result<()> {
        anyhow::bail!("native transition revoked")
    };
    assert!(
        store
            .device(new_binding.clone())
            .rebind_checked(
                CatalogRebindRequest {
                    expected_revision: settings.revision,
                    expected_configuration: digest(&raw),
                    expected_old: &old_binding,
                    old_catalog: &old_catalog,
                    new_catalog: &new_catalog,
                    resolve_old: &resolve,
                    resolve_new: &resolve,
                    authorize: &denied
                },
                || Ok(())
            )
            .is_err()
    );
    assert!(
        store
            .device(new_binding.clone())
            .rebind_checked(
                CatalogRebindRequest {
                    expected_revision: settings.revision,
                    expected_configuration: Sha256Digest::from_bytes([99; 32]),
                    expected_old: &old_binding,
                    old_catalog: &old_catalog,
                    new_catalog: &new_catalog,
                    resolve_old: &resolve,
                    resolve_new: &resolve,
                    authorize: &authorize
                },
                || Ok(())
            )
            .is_err()
    );
    assert_eq!(
        std::fs::read(root.path().join(DIRECTORY).join(CONFIGURATION))?,
        raw
    );
    let request = || CatalogRebindRequest {
        expected_revision: settings.revision,
        expected_configuration: digest(&raw),
        expected_old: &old_binding,
        old_catalog: &old_catalog,
        new_catalog: &new_catalog,
        resolve_old: &resolve,
        resolve_new: &resolve,
        authorize: &authorize,
    };
    let current = store
        .device(new_binding.clone())
        .rebind_checked(request(), || Ok(()))?;
    assert_eq!(current.focus, settings.focus);
    assert_eq!(current.context, settings.context);
    assert_eq!(current.revision, settings.revision + 1);
    assert_eq!(current.device_catalog, new_binding);
    assert_eq!(current.catalog, settings.catalog);
    assert_eq!(current.installation, settings.installation);
    assert_eq!(
        std::fs::read(
            root.path()
                .join(DIRECTORY)
                .join(format!("configuration-v4-{}.retained", digest(&raw)))
        )?,
        raw
    );
    assert!(store.device(old_binding.clone()).load().is_err());
    assert!(
        store
            .device(new_binding.clone())
            .rebind_checked(request(), || Ok(()))
            .is_err()
    );
    let StoredConfiguration::Device(reloaded) = store.device(new_binding).load()? else {
        panic!("expected current4");
    };
    assert_eq!(reloaded, current);
    Ok(())
}

#[test]
fn idempotent_migration_rechecks_liveness_after_all_retained_reads() -> Result<()> {
    use std::cell::Cell;
    let (root, store) = fixture()?;
    let old = enabled(&store)?;
    let raw = old.canonical_bytes()?;
    let profile = profile()?;
    store.device(binding()).migrate_checked(
        old.revision,
        digest(&raw),
        |pin| (pin == profile.manifest).then_some(&profile),
        || Ok(()),
    )?;
    let before = std::fs::read(root.path().join(DIRECTORY).join(CONFIGURATION))?;
    let checks = Cell::new(0u32);
    let revoke = || {
        checks.set(checks.get() + 1);
        if checks.get() == 3 {
            anyhow::bail!("original authority revoked after idempotent retained reads");
        }
        Ok(())
    };
    assert!(
        store
            .device(binding())
            .migrate_checked(
                old.revision,
                digest(&raw),
                |pin| (pin == profile.manifest).then_some(&profile),
                revoke
            )
            .is_err()
    );
    assert_eq!(checks.get(), 3);
    assert_eq!(
        std::fs::read(root.path().join(DIRECTORY).join(CONFIGURATION))?,
        before
    );
    Ok(())
}
#[test]
fn already_initialized_device_read_rechecks_liveness_before_success() -> Result<()> {
    use std::cell::Cell;
    let (root, store) = fixture()?;
    store
        .device(binding())
        .initialize_checked(None, || Ok(()))?;
    let before = std::fs::read(root.path().join(DIRECTORY).join(CONFIGURATION))?;
    let checks = Cell::new(0u32);
    let revoke = || {
        checks.set(checks.get() + 1);
        if checks.get() == 2 {
            anyhow::bail!("original authority revoked after existing configuration read");
        }
        Ok(())
    };
    assert!(
        store
            .device(binding())
            .initialize_checked(None, revoke)
            .is_err()
    );
    assert_eq!(checks.get(), 2);
    assert_eq!(
        std::fs::read(root.path().join(DIRECTORY).join(CONFIGURATION))?,
        before
    );
    Ok(())
}

#[test]
fn initialization_observation_requires_actual_absence_and_never_hides_current_corruption()
-> Result<()> {
    let (root, store) = fixture()?;
    assert!(
        store
            .device(binding())
            .initialization_status(|| Ok(()))?
            .is_none()
    );
    let legacy = root.path().join("arbiter-v2");
    std::fs::create_dir(&legacy)?;
    let bytes = include_bytes!("../../../../../../fixtures/arbiter/configuration-disabled-v2.json");
    std::fs::write(legacy.join(CONFIGURATION), bytes)?;
    let observed = store
        .device(binding())
        .initialization_status(|| Ok(()))?
        .unwrap();
    assert_eq!(observed.actual_legacy_digest, Some(digest(bytes)));
    let current = root.path().join(DIRECTORY);
    std::fs::create_dir(&current)?;
    std::fs::write(
        current.join(CONFIGURATION),
        b"corrupt current configuration3",
    )?;
    assert!(
        store
            .device(binding())
            .initialization_status(|| Ok(()))?
            .is_none()
    );
    assert!(store.device(binding()).load().is_err());
    assert_eq!(
        std::fs::read(current.join(CONFIGURATION))?,
        b"corrupt current configuration3"
    );
    Ok(())
}
#[test]
fn ambiguous_legacy_or_revoked_initialization_observation_refuses() -> Result<()> {
    let (root, store) = fixture()?;
    for name in ["arbiter-v1", "arbiter-v2"] {
        let directory = root.path().join(name);
        std::fs::create_dir(&directory)?;
        std::fs::write(directory.join(CONFIGURATION), b"original retained bytes")?;
    }
    assert!(
        store
            .device(binding())
            .initialization_status(|| Ok(()))
            .is_err()
    );
    assert!(
        store
            .device(binding())
            .initialization_status(|| anyhow::bail!("original observation authority revoked"))
            .is_err()
    );
    assert!(!root.path().join(DIRECTORY).exists());
    Ok(())
}

/// The retained configuration/4 document with Large selected (LH), written by
/// the build before Large was retired for this fixture's installation `01…`
/// and source catalog `02…`.
const RETAINED_LARGE: &[u8] =
    include_bytes!("../../../../../../fixtures/arbiter/configuration4/large-enabled.json");
const RETAINED_BINDING: &[u8] =
    include_bytes!("../../../../../../fixtures/arbiter/configuration4/binding.json");

#[test]
fn configuration4_is_migrated_only_under_the_compiled_transition_and_its_bytes_retained()
-> Result<()> {
    let (root, store) = fixture()?;
    let old: DeviceCatalogBinding = serde_json::from_slice(RETAINED_BINDING)?;
    let directory = root.path().join("arbiter-v3");
    std::fs::create_dir(&directory)?;
    std::fs::write(directory.join("configuration.json"), RETAINED_LARGE)?;
    let device = store.device(binding());
    // Ordinary decoding refuses configuration/4; the binding is observable.
    assert!(device.load().is_err());
    let observed = device
        .inspect_device_binding()?
        .context("binding observation")?;
    assert!(observed.configuration4);
    assert_eq!(
        (observed.binding.clone(), observed.revision),
        (old.clone(), 2)
    );
    // A transition from another binding, or to another, is refused and
    // leaves the bytes as they were.
    let foreign = Configuration4Transition {
        old: binding(),
        new: binding(),
    };
    assert!(device.migrate_v4_checked(&foreign, || Ok(())).is_err());
    let elsewhere = Configuration4Transition {
        old: old.clone(),
        new: DeviceCatalogBinding {
            catalog: Sha256Digest::from_bytes([12; 32]),
            source: Sha256Digest::from_bytes([13; 32]),
        },
    };
    assert!(device.migrate_v4_checked(&elsewhere, || Ok(())).is_err());
    assert_eq!(
        std::fs::read(directory.join("configuration.json"))?,
        RETAINED_LARGE
    );
    let transition = Configuration4Transition {
        old,
        new: binding(),
    };
    let migrated = device.migrate_v4_checked(&transition, || Ok(()))?;
    assert_eq!(migrated.schema, "airk.arbiter.configuration/5");
    assert_eq!(migrated.revision, 3);
    assert_eq!(migrated.device_catalog, binding());
    assert!(!migrated.context.enabled);
    assert_eq!(migrated.context.selected_model, None);
    assert_eq!(migrated.context.indexes.len(), 1);
    assert_eq!(migrated.context_retired, Some(RetiredModel::ContextLarge));
    // The /5 document is now current and the exact /4 bytes are retained.
    let StoredConfiguration::Device(current) = device.load()? else {
        anyhow::bail!("migrated configuration is not current");
    };
    assert_eq!(current, migrated);
    let retained = std::fs::read_dir(&directory)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("configuration-v4-")
        })
        .map(|entry| std::fs::read(entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    assert_eq!(retained, vec![RETAINED_LARGE.to_vec()]);
    // Running it again changes nothing.
    assert_eq!(device.migrate_v4_checked(&transition, || Ok(()))?, migrated);
    Ok(())
}
