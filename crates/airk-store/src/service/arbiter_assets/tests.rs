// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::arbiter::*;
fn profile() -> Profile {
    let digest = Sha256Digest::from_bytes(Sha256::digest(b"asset-data").into());
    Profile {
        task_policies: Default::default(),
        qualified_targets: vec![],
        cpu_fit: None,
        downloads: vec![],
        model: ModelChoice::SmallIntern,
        profile_id: "fixture".into(),
        manifest: Sha256Digest::from_bytes([3; 32]),
        runtime: RUNTIME.into(),
        compiler: digest,
        calibration: digest,
        policy: digest,
        qualification: None,
        tasks: std::collections::BTreeSet::from([Task::ContextNextRead]),
        abi: ScoreAbi {
            readout: Readout::DecisionSymbol,
            inputs: vec![TensorContract {
                name: "input_ids".into(),
                element: TensorType::Int64,
                axes: vec!["tokens".into()],
            }],
            output: TensorContract {
                name: "scores".into(),
                element: TensorType::Float32,
                axes: vec!["options".into()],
            },
        },
        max_tokens: 128,
        max_candidates: 8,
        memory_reservation_bytes: 1 << 20,
        peak_disk_bytes: 1 << 20,
        assets: [
            AssetRole::Model,
            AssetRole::Tokenizer,
            AssetRole::Compiler,
            AssetRole::Calibration,
            AssetRole::Notice,
            AssetRole::Fixture,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, role)| Asset {
            name: format!("asset-{i}.bin"),
            role,
            bytes: 10,
            sha256: digest,
        })
        .collect(),
    }
}
fn fixture() -> Result<(tempfile::TempDir, Arc<AssetStore>, Profile)> {
    let root = tempfile::tempdir()?;
    let lease = Arc::new(ServiceLease::acquire(root.path())?);
    let store = AssetStore::open(
        lease,
        Sha256Digest::from_bytes([1; 32]),
        Incarnation::from_bytes([2; 16]),
    )?;
    Ok((root, store, profile()))
}
fn populate(store: &Arc<AssetStore>, profile: &Profile) -> Result<()> {
    let transaction = store.begin(profile, 1)?;
    for asset in &profile.assets {
        if let Some(mut writer) = transaction.asset(asset)? {
            writer.write_all(b"asset-data")?;
            transaction.publish(writer)?;
        }
    }
    transaction.finish()
}
#[test]
fn absent_asset_status_does_not_create_optional_directories() -> Result<()> {
    let (root, store, profile) = fixture()?;
    assert_eq!(store.phase(&profile)?, None);
    assert!(!root.path().join("arbiter-v2").exists());
    Ok(())
}
#[test]
fn complete_closure_pins_real_files_and_remove_refuses_until_pin_release() -> Result<()> {
    let (_root, store, profile) = fixture()?;
    populate(&store, &profile)?;
    let pin = store.pin(&profile)?;
    assert_eq!(pin.read(&profile.assets[0])?, b"asset-data");
    assert!(store.remove(&profile, 1).is_err());
    drop(pin);
    store.remove(&profile, 1)?;
    assert_eq!(store.phase(&profile)?, Some(Phase::Removed));
    assert!(store.pin(&profile).is_err());
    Ok(())
}
#[test]
fn partial_or_wrong_digest_never_publishes_and_explicit_recovery_keeps_original_generation()
-> Result<()> {
    let (root, store, profile) = fixture()?;
    {
        let transaction = store.begin(&profile, 1)?;
        assert!(store.begin(&profile, 1).is_err());
        let mut writer = transaction.asset(&profile.assets[0])?.unwrap();
        writer.write_all(b"asset-datx")?;
        assert!(transaction.publish(writer).is_err());
    }
    assert_eq!(store.phase(&profile)?, Some(Phase::Failed));
    assert!(store.pin(&profile).is_err());
    let directory = root
        .path()
        .join("arbiter-v2/assets")
        .join(profile.manifest.to_string());
    assert_eq!(
        std::fs::read(directory.join("stage-1/asset-0.bin"))?,
        b"asset-datx"
    );
    populate(&store, &profile)?;
    assert_eq!(store.phase(&profile)?, Some(Phase::Ready));
    assert_eq!(
        std::fs::read(directory.join("stage-1/asset-0.bin"))?,
        b"asset-datx"
    );
    Ok(())
}
#[test]
fn canceled_transaction_is_not_ready_and_restart_requires_explicit_begin() -> Result<()> {
    let (_root, store, profile) = fixture()?;
    store.begin(&profile, 1)?.cancel()?;
    let reopened = AssetStore::open(store.ownership.clone(), store.installation, store.catalog)?;
    assert_eq!(reopened.phase(&profile)?, Some(Phase::Canceled));
    assert!(reopened.pin(&profile).is_err());
    populate(&reopened, &profile)?;
    assert_eq!(reopened.phase(&profile)?, Some(Phase::Ready));
    Ok(())
}
#[test]
fn foreign_existing_final_file_is_preserved_not_adopted_or_deleted() -> Result<()> {
    let (root, store, profile) = fixture()?;
    let transaction = store.begin(&profile, 1)?;
    let path = root
        .path()
        .join("arbiter-v2/assets")
        .join(profile.manifest.to_string())
        .join(&profile.assets[0].name);
    std::fs::write(&path, b"foreign-data")?;
    assert!(transaction.asset(&profile.assets[0]).is_err());
    drop(transaction);
    assert!(store.remove(&profile, 1).is_err());
    assert_eq!(std::fs::read(path)?, b"foreign-data");
    Ok(())
}
#[cfg(unix)]
#[test]
fn linked_optional_asset_directory_is_refused_without_touching_target() -> Result<()> {
    let (root, store, profile) = fixture()?;
    let outside = tempfile::tempdir()?;
    std::os::unix::fs::symlink(outside.path(), root.path().join("arbiter-v2"))?;
    assert!(store.begin(&profile, 1).is_err());
    assert!(std::fs::read_dir(outside.path())?.next().is_none());
    Ok(())
}

#[test]
fn descriptor_reader_lock_blocks_removal_by_another_store_adapter() -> Result<()> {
    let (_root, store, profile) = fixture()?;
    populate(&store, &profile)?;
    let pin = store.pin(&profile)?;
    let other = AssetStore::open(store.ownership.clone(), store.installation, store.catalog)?;
    assert!(other.remove(&profile, 1).is_err());
    assert_eq!(pin.read(&profile.assets[0])?, b"asset-data");
    drop(pin);
    other.remove(&profile, 1)?;
    Ok(())
}
#[test]
fn interrupted_receipt_stage_is_preserved_and_explicit_recovery_uses_fresh_revision() -> Result<()>
{
    let (root, store, profile) = fixture()?;
    let transaction = store.begin(&profile, 1)?;
    let directory = root
        .path()
        .join("arbiter-v2/assets")
        .join(profile.manifest.to_string());
    std::fs::write(
        directory.join("receipt-2.part"),
        b"retained interrupted receipt",
    )?;
    drop(transaction);
    assert_eq!(
        std::fs::read(directory.join("receipt-2.part"))?,
        b"retained interrupted receipt"
    );
    populate(&store, &profile)?;
    assert_eq!(store.phase(&profile)?, Some(Phase::Ready));
    assert_eq!(
        std::fs::read(directory.join("receipt-2.part"))?,
        b"retained interrupted receipt"
    );
    Ok(())
}
#[test]
fn reserved_transaction_names_refuse_before_any_optional_write() -> Result<()> {
    let (root, store, mut profile) = fixture()?;
    profile.assets[0].name = "receipt.json".into();
    assert!(store.begin(&profile, 1).is_err());
    assert!(!root.path().join("arbiter-v2").exists());
    Ok(())
}

#[test]
fn withdrawn_profile_cleanup_uses_only_owned_receipt_and_real_pin_boundary() -> Result<()> {
    let (_root, store, profile) = fixture()?;
    populate(&store, &profile)?;
    // The storage fixture was never a compiled execution capability.
    let pin = store.pin(&profile)?;
    assert!(store.remove_owned(profile.manifest, 2).is_err());
    drop(pin);
    store.remove_owned(profile.manifest, 2)?;
    assert_eq!(
        store.owned_observation(profile.manifest)?.unwrap().phase,
        Phase::Removed
    );
    assert!(store.pin(&profile).is_err());
    Ok(())
}
#[test]
fn tampered_cleanup_receipt_identity_and_path_are_refused_without_source_mutation() -> Result<()> {
    let (root, store, profile) = fixture()?;
    populate(&store, &profile)?;
    let path = root
        .path()
        .join("arbiter-v2/assets")
        .join(profile.manifest.to_string())
        .join(RECEIPT);
    let original = std::fs::read(&path)?;
    let mut record: Record = serde_json::from_slice(&original)?;
    let source = root.path().join("source.txt");
    std::fs::write(&source, b"source remains read only")?;
    record.assets[0].name = "../../../source.txt".into();
    std::fs::write(&path, canonical(&record)?)?;
    assert!(store.remove_owned(profile.manifest, 2).is_err());
    assert_eq!(std::fs::read(&source)?, b"source remains read only");
    let mut record: Record = serde_json::from_slice(&original)?;
    record.installation = Sha256Digest::from_bytes([9; 32]);
    std::fs::write(&path, canonical(&record)?)?;
    assert!(store.remove_owned(profile.manifest, 2).is_err());
    assert!(store.owned_observation(profile.manifest).is_err());
    assert_eq!(std::fs::read(&source)?, b"source remains read only");
    Ok(())
}
#[test]
fn incomplete_owned_removal_is_discovered_by_original_configuration_without_reprovisioning()
-> Result<()> {
    let (root, store, profile) = fixture()?;
    populate(&store, &profile)?;
    let asset = root
        .path()
        .join("arbiter-v2/assets")
        .join(profile.manifest.to_string())
        .join(&profile.assets[0].name);
    std::fs::write(&asset, b"foreign-data")?;
    assert!(store.remove_owned(profile.manifest, 2).is_err());
    assert_eq!(store.pending_owned_removals(2)?, vec![profile.manifest]);
    assert!(store.pending_owned_removals(3)?.is_empty());
    assert_eq!(std::fs::read(&asset)?, b"foreign-data");
    // An explicit owner repair restores the exact approved bytes; removal then
    // reconciles this operation instead of creating or downloading another set.
    std::fs::write(&asset, b"asset-data")?;
    store.remove_owned(profile.manifest, 2)?;
    assert_eq!(
        store.owned_observation(profile.manifest)?.unwrap().phase,
        Phase::Removed
    );
    assert!(!asset.exists());
    Ok(())
}

#[test]
fn explicit_remove_cleans_only_exact_sealed_canceled_partial_bytes() -> Result<()> {
    let (root, store, profile) = fixture()?;
    let transaction = store.begin(&profile, 1)?;
    let mut writer = transaction.asset(&profile.assets[0])?.unwrap();
    writer.write_all(b"asset")?;
    drop(writer);
    transaction.cancel()?;
    let stage = root
        .path()
        .join("arbiter-v2/assets")
        .join(profile.manifest.to_string())
        .join("stage-1");
    assert_eq!(
        std::fs::read(stage.join(&profile.assets[0].name))?,
        b"asset"
    );
    store.remove_owned(profile.manifest, 2)?;
    assert!(!stage.join(&profile.assets[0].name).exists());
    assert!(
        !stage
            .join(format!("{}.partial.json", profile.assets[0].name))
            .exists()
    );
    assert!(stage.join("operation.json").exists());
    assert_eq!(
        store.owned_observation(profile.manifest)?.unwrap().phase,
        Phase::Removed
    );
    Ok(())
}
#[test]
fn hard_interruption_without_partial_receipt_is_preserved_and_removal_stays_pending() -> Result<()>
{
    let (root, store, profile) = fixture()?;
    let transaction = store.begin(&profile, 1)?;
    let stage = root
        .path()
        .join("arbiter-v2/assets")
        .join(profile.manifest.to_string())
        .join("stage-1");
    std::fs::write(stage.join(&profile.assets[0].name), b"unproven partial")?;
    drop(transaction);
    assert!(store.remove_owned(profile.manifest, 2).is_err());
    assert_eq!(
        std::fs::read(stage.join(&profile.assets[0].name))?,
        b"unproven partial"
    );
    assert_eq!(
        store.owned_observation(profile.manifest)?.unwrap().phase,
        Phase::Removing
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn removal_preserves_foreign_alternate_streams_and_changed_default_bytes() -> Result<()> {
    let (root, store, profile) = fixture()?;
    populate(&store, &profile)?;
    let directory = root
        .path()
        .join("arbiter-v2/assets")
        .join(profile.manifest.to_string());
    let asset = directory.join(&profile.assets[0].name);
    let stream = std::path::PathBuf::from(format!("{}:foreign", asset.display()));
    std::fs::write(&stream, b"foreign alternate stream")?;
    assert!(store.remove_owned(profile.manifest, 1).is_err());
    assert_eq!(std::fs::read(&stream)?, b"foreign alternate stream");
    assert_eq!(std::fs::read(&asset)?, b"asset-data");
    assert_eq!(store.phase(&profile)?, Some(Phase::Removing));
    std::fs::remove_file(&stream)?;
    std::fs::write(&asset, b"asset-edit")?;
    assert!(store.remove_owned(profile.manifest, 1).is_err());
    assert_eq!(std::fs::read(&asset)?, b"asset-edit");
    Ok(())
}

#[cfg(windows)]
#[test]
fn same_removal_handle_blocks_mutation_after_hash_through_disposition() -> Result<()> {
    let (root, store, profile) = fixture()?;
    populate(&store, &profile)?;
    let directory = root
        .path()
        .join("arbiter-v2/assets")
        .join(profile.manifest.to_string());
    let asset = directory.join(&profile.assets[0].name);
    let raw = airk_process::directory::OpenedDirectory::open(&directory)?;
    let mut observed = false;
    remove_verified_with(
        &raw,
        OsStr::new(&profile.assets[0].name),
        &profile.assets[0],
        None,
        |_| {
            observed = true;
            let write = std::fs::OpenOptions::new()
                .write(true)
                .open(&asset)
                .unwrap_err();
            assert_eq!(
                write.raw_os_error(),
                Some(5),
                "pending verified deletion handle must deny new opens"
            );
            let stream = std::path::PathBuf::from(format!("{}:racing", asset.display()));
            let alternate = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&stream)
                .unwrap_err();
            assert_eq!(
                alternate.raw_os_error(),
                Some(5),
                "alternate streams cannot be added after hash"
            );
            Ok(())
        },
    )?;
    assert!(observed);
    assert!(!asset.exists());
    Ok(())
}

#[test]
fn same_bytes_replaced_object_is_not_adopted_pinned_or_removed() -> Result<()> {
    let (root, store, profile) = fixture()?;
    populate(&store, &profile)?;
    let directory = root
        .path()
        .join("arbiter-v2/assets")
        .join(profile.manifest.to_string());
    let target = directory.join(&profile.assets[0].name);
    let preserved = root.path().join("original-retained-object");
    std::fs::rename(&target, &preserved)?;
    std::fs::write(&target, b"asset-data")?;
    assert!(
        store.pin(&profile).is_err(),
        "same-byte foreign inode cannot become a native pin"
    );
    assert!(
        store.begin(&profile, 2).is_err(),
        "recovery cannot adopt same-byte foreign inode"
    );
    assert!(
        store.remove_owned(profile.manifest, 2).is_err(),
        "digest equality is not object custody"
    );
    assert_eq!(std::fs::read(&target)?, b"asset-data");
    assert_eq!(std::fs::read(&preserved)?, b"asset-data");
    Ok(())
}

#[test]
fn final_object_custody_precedes_publication_and_survives_interruption() -> Result<()> {
    let (_root, store, profile) = fixture()?;
    let transaction = store.begin(&profile, 1)?;
    let mut writer = transaction.asset(&profile.assets[0])?.unwrap();
    writer.write_all(b"asset-data")?;
    let original = file_identity(&writer.file)?;
    let mut observed = false;
    assert!(
        transaction
            .publish_with(writer, || {
                observed = true;
                let (record, _) = store.read(&transaction.directory, &profile)?.unwrap();
                assert_eq!(record.phase, Phase::Downloading);
                assert_eq!(
                    record.owned_assets.get(&profile.assets[0].name),
                    Some(&original)
                );
                assert!(missing(
                    &transaction
                        .directory
                        .open_file(OsStr::new(&profile.assets[0].name), SetupFileMode::Read)
                        .unwrap_err()
                ));
                anyhow::bail!("injected interruption after custody before publication")
            })
            .is_err()
    );
    assert!(observed);
    drop(transaction);
    let directory = store.directory(profile.manifest, false)?;
    let (record, _) = store.read(&directory, &profile)?.unwrap();
    assert_eq!(record.phase, Phase::Failed);
    assert_eq!(
        record.owned_assets.get(&profile.assets[0].name),
        Some(&original)
    );
    store.remove_owned(profile.manifest, 1)?;
    assert_eq!(store.phase(&profile)?, Some(Phase::Removed));
    Ok(())
}
