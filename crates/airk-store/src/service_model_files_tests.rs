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
