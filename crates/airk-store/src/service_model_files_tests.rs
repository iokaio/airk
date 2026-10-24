// SPDX-License-Identifier: Apache-2.0
use super::*;

fn fixture(root: &Path) -> Result<(PathBuf, ModelObjectSet)> {
    let source = root.canonicalize()?.join("original");
    fs::create_dir(&source)?;
    let assets = [
        (ModelAssetName::ModelJson, b"model fixture".as_slice()),
        (
            ModelAssetName::VocabularyJson,
            b"vocabulary fixture".as_slice(),
        ),
    ];
    let mut set = ModelObjectSet {
        version: 1,
        shared: None,
        pipeline_digest: Sha256Digest::from_bytes([2; 32]),
        legacy_identity: Sha256Digest::from_bytes([3; 32]).to_string(),
        assets: BTreeMap::new(),
    };
    for (name, bytes) in assets {
        fs::write(source.join(name.filename()), bytes)?;
        set.assets.insert(
            name,
            ModelObjectRef {
                sha256: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
                bytes: bytes.len() as u64,
            },
        );
    }
    Ok((source, set))
}

#[test]
fn shared_files_reuse_exact_sets_outlive_adapter_and_preserve_active_or_unknown_files() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let (source, set) = fixture(root.path())?;
    let mut cache = ServiceModelFiles::open(root.path())?;
    let first = cache.retain(&set, |name| source.join(name.filename()))?;
    let second = cache.retain(&set, |_| panic!("verified shared model copied twice"))?;
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(cache.inventory()?.len(), 1);
    assert_eq!(cache.retire_unused()?, 0);
    drop(cache);
    assert!(
        ServiceModelFiles::open(root.path()).is_err(),
        "live model lost its global ownership pin"
    );
    drop(second);
    let directory = first.directory().to_path_buf();
    drop(first);
    let mut cache = ServiceModelFiles::open(root.path())?;
    fs::write(directory.join("unknown.txt"), b"user fixture")?;
    assert!(cache.retire_unused().is_err());
    assert_eq!(fs::read(directory.join("unknown.txt"))?, b"user fixture");
    assert_eq!(fs::read(source.join("model.json"))?, b"model fixture");
    fs::remove_file(directory.join("unknown.txt"))?;
    assert_eq!(cache.retire_unused()?, 1);
    assert!(!directory.exists());
    Ok(())
}

#[test]
fn interrupted_owned_copy_resumes_and_corrupt_or_aliased_assets_are_preserved() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (source, set) = fixture(root.path())?;
    let mut cache = ServiceModelFiles::open(root.path())?;
    let receipt = set.canonical_bytes()?;
    let digest = Sha256Digest::from_bytes(Sha256::digest(&receipt).into());
    let stage = cache.root.join("sets").join(format!("stage-{digest}"));
    fs::create_dir(&stage)?;
    fs::write(stage.join("set.json"), &receipt)?;
    fs::write(stage.join("model.json.part"), b"model")?;
    let lease = cache.retain(&set, |name| source.join(name.filename()))?;
    assert!(!stage.exists());
    let directory = lease.directory().to_path_buf();
    let target = directory.join("model.json");
    drop(lease);
    fs::write(&target, b"other fixture")?;
    assert!(
        cache
            .retain(&set, |name| source.join(name.filename()))
            .is_err()
    );
    assert!(cache.retire_unused().is_err());
    assert_eq!(fs::read(&target)?, b"other fixture");
    fs::write(&target, b"model fixture")?;
    let link = root.path().join("linked-model.json");
    fs::hard_link(&target, &link)?;
    assert!(
        cache
            .retain(&set, |name| source.join(name.filename()))
            .is_err()
    );
    assert!(cache.retire_unused().is_err());
    assert_eq!(fs::read(link)?, b"model fixture");
    Ok(())
}

#[test]
fn retirement_intent_resumes_after_partial_delete_without_touching_sources() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (source, set) = fixture(root.path())?;
    let mut cache = ServiceModelFiles::open(root.path())?;
    let lease = cache.retain(&set, |name| source.join(name.filename()))?;
    let path = lease.directory().to_path_buf();
    let name = path.file_name().unwrap().to_str().unwrap();
    let retiring = path.parent().unwrap().join(format!("retire-{name}"));
    drop(lease);
    fs::rename(&path, &retiring)?;
    fs::remove_file(retiring.join("model.json"))?;
    drop(cache);
    let cache = ServiceModelFiles::open(root.path())?;
    assert!(cache.inventory()?.is_empty());
    assert!(!retiring.exists());
    assert_eq!(fs::read(source.join("model.json"))?, b"model fixture");
    Ok(())
}

#[test]
fn streamed_install_reuses_persistent_assets_after_sessions_and_owner_restart() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (source, set) = fixture(root.path())?;
    let assets = set
        .assets
        .keys()
        .map(|name| Ok((*name, fs::read(source.join(name.filename()))?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut cache = ServiceModelFiles::open(root.path())?;
    let calls = std::cell::Cell::new(0);
    let first = cache.install_readers(&set, |name| {
        calls.set(calls.get() + 1);
        Ok(Box::new(std::io::Cursor::new(assets[&name].clone())))
    })?;
    assert_eq!(calls.get(), 2);
    let directory = first.directory().to_path_buf();
    assert!(directory.join("installed.json").is_file());
    drop(first);
    assert_eq!(cache.retire_unused()?, 0);
    drop(cache);
    let mut cache = ServiceModelFiles::open(root.path())?;
    let second = cache.install_readers(&set, |_| panic!("installed assets were copied again"))?;
    assert_eq!(second.directory(), directory);
    drop(second);
    assert_eq!(cache.retire_unused()?, 0);
    assert_eq!(
        fs::read(source.join("model.json"))?,
        assets[&ModelAssetName::ModelJson]
    );
    Ok(())
}

#[test]
fn installed_intent_resumes_only_an_owned_prefix_and_never_becomes_cache_eviction() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (source, set) = fixture(root.path())?;
    let mut cache = ServiceModelFiles::open(root.path())?;
    let lease = cache.retain(&set, |name| source.join(name.filename()))?;
    let path = lease.directory().to_path_buf();
    drop(lease);
    let expected = installed_bytes(&set)?;
    fs::write(path.join("installed.part"), &expected[..expected.len() / 2])?;
    assert_eq!(cache.retire_unused()?, 0);
    let lease = cache.install_readers(&set, |_| panic!("resumed install recopied assets"))?;
    assert_eq!(fs::read(path.join("installed.json"))?, expected);
    assert!(!path.join("installed.part").exists());
    drop(lease);
    fs::write(path.join("installed.json"), b"Unknown owner preserved.")?;
    assert!(cache.retire_unused().is_err());
    assert!(
        cache
            .install_readers(&set, |_| panic!("unknown receipt triggered a copy"))
            .is_err()
    );
    assert_eq!(
        fs::read(path.join("installed.json"))?,
        b"Unknown owner preserved."
    );
    Ok(())
}

#[test]
fn installed_readers_bind_runtime_exact_objects_and_budgets_without_local_fallback() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (source, set) = fixture(root.path())?;
    let mut cache = ServiceModelFiles::open(root.path())?;
    let lease = cache.install_readers(&set, |name| {
        Ok(Box::new(File::open(source.join(name.filename()))?))
    })?;
    let objects = cache.installed_reference(&set)?;
    let reader = InstalledModel::open(root.path(), &objects)?;
    assert_eq!(
        reader.read_asset(ModelAssetName::ModelJson, 16384)?,
        b"model fixture"
    );
    assert!(reader.read_asset(ModelAssetName::ModelJson, 1).is_err());
    assert!(reader.verify(0).is_err());
    reader.verify(1 << 20)?;
    let foreign = tempfile::tempdir()?;
    assert!(InstalledModel::open(foreign.path(), &objects).is_err());
    assert!(InstalledModel::open(root.path(), &set).is_err());
    assert!(
        crate::model_objects::ModelObjects::new(source.as_path())
            .verify(&objects, 1 << 20)
            .is_err()
    );
    drop(lease);
    let global = cache.installed(&objects)?;
    assert_eq!(global.directory(), reader.directory());
    fs::write(global.directory().join("model.json"), b"other fixture")?;
    assert!(reader.read_asset(ModelAssetName::ModelJson, 16384).is_err());
    assert!(cache.installed(&objects).is_err());
    assert_eq!(fs::read(source.join("model.json"))?, b"model fixture");
    Ok(())
}

#[test]
fn default_pointer_resumes_exact_intent_and_preserves_unknown_bytes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (source, set) = fixture(root.path())?;
    assert!(ServiceModelFiles::default_reference(root.path(), &set)?.is_none());
    let mut cache = ServiceModelFiles::open(root.path())?;
    let lease = cache.install_readers(&set, |name| {
        Ok(Box::new(File::open(source.join(name.filename()))?))
    })?;
    let objects = cache.installed_reference(&set)?;
    let key = objects.shared.as_ref().unwrap().set;
    let bytes = objects.canonical_bytes()?;
    let partial = cache.root.join(format!("default-{key}.part"));
    let target = cache.root.join(format!("default-{key}.json"));
    fs::write(&partial, &bytes[..bytes.len() / 2])?;
    assert!(ServiceModelFiles::default_reference(root.path(), &set)?.is_none());
    assert_eq!(cache.publish_default(&set)?, objects);
    assert!(!partial.exists());
    assert_eq!(cache.publish_default(&set)?, objects);
    drop(lease);
    drop(cache);
    assert_eq!(
        ServiceModelFiles::default_reference(root.path(), &set)?,
        Some(objects)
    );
    let cache = ServiceModelFiles::open(root.path())?;
    fs::write(&target, b"preserve unknown default")?;
    assert!(cache.publish_default(&set).is_err());
    assert!(ServiceModelFiles::default_reference(root.path(), &set).is_err());
    assert_eq!(fs::read(&target)?, b"preserve unknown default");
    fs::remove_file(&target)?;
    fs::write(&partial, b"preserve unknown intent")?;
    assert!(cache.publish_default(&set).is_err());
    assert_eq!(fs::read(partial)?, b"preserve unknown intent");
    Ok(())
}

#[test]
fn installed_set_limit_refuses_new_assets_without_evicting_installed_inputs() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (source, mut set) = fixture(root.path())?;
    let mut cache = ServiceModelFiles::open(root.path())?;
    for index in 1..=MAX_SETS {
        set.pipeline_digest = Sha256Digest::from_bytes([index as u8; 32]);
        drop(cache.install_readers(&set, |name| {
            Ok(Box::new(File::open(source.join(name.filename()))?))
        })?);
    }
    assert_eq!(cache.inventory()?.len(), MAX_SETS);
    set.pipeline_digest = Sha256Digest::from_bytes([99; 32]);
    assert!(
        cache
            .install_readers(&set, |_| panic!("copy started beyond set limit"))
            .is_err()
    );
    assert_eq!(cache.retire_unused()?, 0);
    assert_eq!(cache.inventory()?.len(), MAX_SETS);
    assert_eq!(fs::read(source.join("model.json"))?, b"model fixture");
    Ok(())
}
