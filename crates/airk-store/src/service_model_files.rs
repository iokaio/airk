// SPDX-License-Identifier: Apache-2.0
//! Bounded immutable per-service model files, independent of collection incarnations.
use crate::{
    managed_collection::reject_file_aliases,
    quota::{Quota, QuotaPolicy},
};
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    model_objects::{ModelAssetName, ModelAssetSource, ModelObjectRef, ModelObjectSet},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Weak},
};

const MAX_SETS: usize = 16;
const MAX_BYTES: u64 = 1024 * 1024 * 1024;
const OWNER: &[u8] = b"airk-service-model-files-v1\n";

/// A verified installed-set locator. It carries no lifetime claim on an
/// evictable cache entry: installed sets are retained independently of sessions.
pub struct InstalledModel {
    directory: PathBuf,
    set: ModelObjectSet,
}
pub fn model_scope(service_root: &Path) -> Result<Sha256Digest> {
    let root = service_root.canonicalize()?;
    directories(&root, false)?;
    Ok(framed_digest(
        b"airk-shared-model-owner/1.4",
        &[root
            .to_str()
            .context("shared model runtime must be UTF-8")?
            .as_bytes()],
    ))
}
impl InstalledModel {
    pub fn open(service_root: &Path, objects: &ModelObjectSet) -> Result<Self> {
        objects.validate()?;
        let shared = objects
            .shared
            .as_ref()
            .context("model is not a shared installed reference")?;
        ensure!(
            shared.owner == model_scope(service_root)?,
            "shared model belongs to another runtime"
        );
        let root = service_root.canonicalize()?.join("model-files-1.4");
        directories(&root, false)?;
        verify_bytes(&root.join("owner.txt"), OWNER)?;
        let directory = root.join("sets").join(shared.set.to_string());
        directories(&directory, false)?;
        let bytes = read_receipt(&directory.join("set.json"))?;
        ensure!(
            Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == shared.set,
            "shared installed set identity mismatch"
        );
        let set = ModelObjectSet::from_bytes(&bytes)?;
        ensure!(
            set == objects.local_set(),
            "shared installed set differs from captured objects"
        );
        verify_bytes(&directory.join("installed.json"), &installed_bytes(&set)?)?;
        Ok(Self { directory, set })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn verify(&self, maximum: u64) -> Result<()> {
        let mut remaining = maximum;
        for (name, asset) in &self.set.assets {
            remaining = remaining
                .checked_sub(asset.bytes)
                .context("shared model verification budget exceeded")?;
            verify_asset(&self.directory.join(name.filename()), asset)?;
        }
        Ok(())
    }
}
impl ModelAssetSource for InstalledModel {
    fn read_asset(&self, name: ModelAssetName, maximum: u64) -> Result<Vec<u8>> {
        let asset = self
            .set
            .assets
            .get(&name)
            .context("shared model asset absent")?;
        ensure!(
            asset.bytes <= maximum.min(name.maximum_bytes()),
            "shared model asset exceeds read budget"
        );
        directories(&self.directory, false)?;
        let path = self.directory.join(name.filename());
        let metadata = fs::symlink_metadata(&path)?;
        reject_file_aliases(&path, &metadata)?;
        ensure!(
            metadata.len() == asset.bytes,
            "shared model asset length changed"
        );
        let mut bytes = Vec::new();
        File::open(path)?
            .take(asset.bytes + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == asset.bytes
                && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == asset.sha256,
            "shared model asset digest changed"
        );
        Ok(bytes)
    }
}

pub struct ModelFileLease {
    path: PathBuf,
    // Keep exclusion from another cache owner until the final loaded model or
    // session releases its files, even if its owning cache adapter was dropped.
    _owner: Arc<File>,
}
impl ModelFileLease {
    pub fn directory(&self) -> &Path {
        &self.path
    }
}
pub struct ServiceModelFiles {
    root: PathBuf,
    quota: Quota,
    owner: Arc<File>,
    pins: BTreeMap<Sha256Digest, Weak<ModelFileLease>>,
}
impl ServiceModelFiles {
    /// The native adapter establishes the same-user private service directory.
    pub fn open(service_root: &Path) -> Result<Self> {
        let root = service_root.canonicalize()?.join("model-files-1.4");
        directories(&root, true)?;
        fs::create_dir_all(&root)?;
        directories(&root, false)?;
        let lock = root.join("owner.lock");
        if let Ok(meta) = fs::symlink_metadata(&lock) {
            reject_file_aliases(&lock, &meta)?;
        }
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock)?;
        reject_file_aliases(&lock, &owner.metadata()?)?;
        ensure!(
            owner.metadata()?.len() == 0,
            "invalid shared model ownership file"
        );
        owner
            .try_lock()
            .context("shared model files still have an owner")?;
        let receipt = root.join("owner.txt");
        if !receipt.try_exists()? {
            ensure!(
                fs::read_dir(&root)?.count() == 1,
                "unowned shared model directory"
            );
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&receipt)?;
            file.write_all(OWNER)?;
            file.sync_all()?;
            sync_directory(&root)?;
        }
        verify_bytes(&receipt, OWNER)?;
        let sets = root.join("sets");
        directories(&sets, true)?;
        fs::create_dir_all(&sets)?;
        let quota = Quota::open_service(&root)?;
        quota.configure(QuotaPolicy {
            root_bytes: MAX_BYTES,
            default_collection_bytes: MAX_BYTES,
            collections: BTreeMap::new(),
        })?;
        let value = Self {
            root,
            quota,
            owner: Arc::new(owner),
            pins: BTreeMap::new(),
        };
        value.inventory()?;
        value.recover_retirement()?;
        Ok(value)
    }

    pub fn publish_default(&self, set: &ModelObjectSet) -> Result<ModelObjectSet> {
        let objects = self.installed_reference(set)?;
        let key = objects.shared.as_ref().unwrap().set;
        let path = self.root.join(format!("default-{key}.json"));
        let bytes = objects.canonical_bytes()?;
        if path.try_exists()? {
            verify_bytes(&path, &bytes)?;
            return Ok(objects);
        }
        let reservation = self.quota.reserve_root(bytes.len() as u64)?;
        let partial = self.root.join(format!("default-{key}.part"));
        if partial.try_exists()? {
            verify_prefix(&partial, &bytes)?;
        }
        reservation.charge(bytes.len() as u64)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&partial)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(partial, path)?;
        sync_directory(&self.root)?;
        Ok(objects)
    }
    pub fn default_reference(
        runtime: &Path,
        plan: &ModelObjectSet,
    ) -> Result<Option<ModelObjectSet>> {
        ensure!(
            plan.shared.is_none(),
            "default asset plan must be local metadata"
        );
        let expected = plan.shared_with(model_scope(runtime)?)?;
        let key = expected.shared.as_ref().unwrap().set;
        let path = runtime
            .canonicalize()?
            .join("model-files-1.4")
            .join(format!("default-{key}.json"));
        if !path.try_exists()? {
            return Ok(None);
        }
        let objects = ModelObjectSet::from_bytes(&read_receipt(&path)?)?;
        ensure!(
            objects == expected,
            "default installed model reference changed"
        );
        InstalledModel::open(runtime, &objects)?;
        Ok(Some(objects))
    }

    pub fn installed_reference(&self, set: &ModelObjectSet) -> Result<ModelObjectSet> {
        let runtime = self.root.parent().context("model runtime parent missing")?;
        let objects = set.shared_with(model_scope(runtime)?)?;
        let installed = InstalledModel::open(runtime, &objects)?;
        validate_set(installed.directory(), &objects.local_set(), false)?;
        Ok(objects)
    }
    pub fn installed(&mut self, objects: &ModelObjectSet) -> Result<Arc<ModelFileLease>> {
        let runtime = self.root.parent().context("model runtime parent missing")?;
        let installed = InstalledModel::open(runtime, objects)?;
        validate_set(installed.directory(), &objects.local_set(), false)?;
        let digest = objects.shared.as_ref().unwrap().set;
        Ok(self.pin(digest, installed.directory))
    }

    /// The caller verifies project scope and holds any collection object pin
    /// through this copy. Only fixed asset names from the exact set are read.
    pub fn retain(
        &mut self,
        set: &ModelObjectSet,
        source: impl Fn(ModelAssetName) -> PathBuf,
    ) -> Result<Arc<ModelFileLease>> {
        self.retain_readers(set, |name| {
            let input_path = source(name);
            directories(
                input_path.parent().context("model source parent missing")?,
                false,
            )?;
            verify_asset(&input_path, &set.assets[&name])?;
            Ok(Box::new(File::open(input_path)?) as Box<dyn Read>)
        })
    }

    /// Provision directly from verified byte streams (including embedded assets).
    /// No temporary input directory or collection-local weight copy is required.
    pub fn install_readers(
        &mut self,
        set: &ModelObjectSet,
        source: impl Fn(ModelAssetName) -> Result<Box<dyn Read>>,
    ) -> Result<Arc<ModelFileLease>> {
        let lease = self.retain_readers(set, source)?;
        let marker = installed_bytes(set)?;
        let directory = lease.directory();
        let installed = directory.join("installed.json");
        if installed.try_exists()? {
            verify_bytes(&installed, &marker)?;
            return Ok(lease);
        }
        let reservation = self.quota.reserve_root(marker.len() as u64)?;
        let partial = directory.join("installed.part");
        if partial.try_exists()? {
            verify_prefix(&partial, &marker)?;
        }
        reservation.charge(marker.len() as u64)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&partial)?;
        file.write_all(&marker)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&partial, installed)?;
        sync_directory(directory)?;
        Ok(lease)
    }

    fn retain_readers(
        &mut self,
        set: &ModelObjectSet,
        source: impl Fn(ModelAssetName) -> Result<Box<dyn Read>>,
    ) -> Result<Arc<ModelFileLease>> {
        ensure!(
            set.shared.is_none(),
            "shared provisioning requires the local asset description"
        );
        let receipt = set.canonical_bytes()?;
        let digest = Sha256Digest::from_bytes(Sha256::digest(&receipt).into());
        let destination = self.root.join("sets").join(digest.to_string());
        if destination.try_exists()? {
            validate_set(&destination, set, false)?;
            return Ok(self.pin(digest, destination));
        }
        let mut entries = self.inventory()?;
        let stage = self.root.join("sets").join(format!("stage-{digest}"));
        let required: u64 =
            set.assets.values().map(|asset| asset.bytes).sum::<u64>() + receipt.len() as u64;
        if entries.len() >= MAX_SETS || self.quota.reserve_root(required).is_err() {
            self.retire_unused()?;
            entries = self.inventory()?;
        }
        ensure!(
            entries.len() < MAX_SETS || entries.contains(&stage),
            "shared model set limit reached"
        );
        let reservation = self.quota.reserve_root(required)?;
        if !stage.try_exists()? {
            fs::create_dir(&stage)?;
            reservation.charge(receipt.len() as u64)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(stage.join("set.json"))?;
            file.write_all(&receipt)?;
            file.sync_all()?;
            sync_directory(&stage)?;
            sync_directory(stage.parent().unwrap())?;
        }
        validate_set(&stage, set, true)?;
        for (name, reference) in &set.assets {
            let target = stage.join(name.filename());
            if target.try_exists()? {
                verify_asset(&target, reference)?;
                continue;
            }
            let partial = stage.join(format!("{}.part", name.filename()));
            if let Ok(meta) = fs::symlink_metadata(&partial) {
                reject_file_aliases(&partial, &meta)?;
                ensure!(
                    meta.len() <= reference.bytes,
                    "oversized owned model staging file"
                );
                fs::remove_file(&partial)?;
            }
            reservation.charge(reference.bytes)?;
            let mut input = source(*name)?.take(reference.bytes + 1);
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&partial)?;
            let mut hash = Sha256::new();
            let mut bytes = 0u64;
            let mut buffer = [0; 64 * 1024];
            loop {
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                bytes += count as u64;
                ensure!(bytes <= reference.bytes, "model source grew during copy");
                output.write_all(&buffer[..count])?;
                hash.update(&buffer[..count]);
            }
            ensure!(
                bytes == reference.bytes
                    && Sha256Digest::from_bytes(hash.finalize().into()) == reference.sha256,
                "model source changed during copy"
            );
            output.sync_all()?;
            drop(output);
            fs::rename(&partial, &target)?;
            sync_directory(&stage)?;
        }
        validate_set(&stage, set, false)?;
        fs::rename(&stage, &destination)?;
        sync_directory(destination.parent().unwrap())?;
        Ok(self.pin(digest, destination))
    }

    fn pin(&mut self, digest: Sha256Digest, path: PathBuf) -> Arc<ModelFileLease> {
        if let Some(existing) = self.pins.get(&digest).and_then(Weak::upgrade) {
            return existing;
        }
        let lease = Arc::new(ModelFileLease {
            path,
            _owner: self.owner.clone(),
        });
        self.pins.insert(digest, Arc::downgrade(&lease));
        lease
    }

    /// Bounded cache eviction. Collection evidence is not deleted. Unknown or
    /// modified state is preserved; active sessions/loaded models keep their pins.
    pub fn retire_unused(&mut self) -> Result<usize> {
        self.recover_retirement()?;
        let entries = self.inventory()?;
        let mut removed = 0;
        for directory in entries {
            let name = directory
                .file_name()
                .and_then(|value| value.to_str())
                .context("invalid model set name")?;
            if name.starts_with("stage-") || name.starts_with("retire-") {
                continue;
            }
            let digest: Sha256Digest = name.parse()?;
            if self.pins.get(&digest).and_then(Weak::upgrade).is_some() {
                continue;
            }
            if directory.join("installed.json").try_exists()?
                || directory.join("installed.part").try_exists()?
            {
                let set = ModelObjectSet::from_bytes(&read_receipt(&directory.join("set.json"))?)?;
                validate_set(&directory, &set, false)?;
                continue; // Installed assets are persistent, not an inference-session cache.
            }
            let bytes = read_receipt(&directory.join("set.json"))?;
            ensure!(
                Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == digest,
                "model set ownership mismatch"
            );
            let set = ModelObjectSet::from_bytes(&bytes)?;
            validate_set(&directory, &set, false)?;
            let retiring = self.root.join("sets").join(format!("retire-{digest}"));
            fs::rename(&directory, &retiring)?;
            sync_directory(&self.root.join("sets"))?;
            finish_retirement(&retiring, digest)?;
            self.pins.remove(&digest);
            removed += 1;
        }
        sync_directory(&self.root.join("sets"))?;
        Ok(removed)
    }

    fn recover_retirement(&self) -> Result<()> {
        for path in self.inventory()? {
            if let Some(key) = path
                .file_name()
                .and_then(|value| value.to_str())
                .and_then(|value| value.strip_prefix("retire-"))
            {
                finish_retirement(&path, key.parse()?)?;
            }
        }
        sync_directory(&self.root.join("sets"))
    }

    fn inventory(&self) -> Result<Vec<PathBuf>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(self.root.join("sets"))? {
            ensure!(
                entries.len() < MAX_SETS,
                "shared model directory limit exceeded"
            );
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().context("invalid shared model filename")?;
            let key = name
                .strip_prefix("stage-")
                .or_else(|| name.strip_prefix("retire-"))
                .unwrap_or(name);
            ensure!(
                key.len() == 64
                    && key
                        .bytes()
                        .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value)),
                "unknown shared model directory"
            );
            directories(&entry.path(), false)?;
            entries.push(entry.path());
        }
        Ok(entries)
    }
}

fn finish_retirement(directory: &Path, digest: Sha256Digest) -> Result<()> {
    ensure!(
        !directory.join("installed.json").try_exists()?
            && !directory.join("installed.part").try_exists()?,
        "installed model cannot be retired as cache state"
    );
    // The atomic rename records retirement intent before the first unlink.
    // Remove the receipt last, so every partially removed set remains provable.
    if fs::read_dir(directory)?.next().is_none() {
        fs::remove_dir(directory)?;
        return Ok(());
    }
    let receipt = read_receipt(&directory.join("set.json"))?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&receipt).into()) == digest,
        "retired model ownership mismatch"
    );
    let set = ModelObjectSet::from_bytes(&receipt)?;
    validate_set(directory, &set, true)?;
    for entry in fs::read_dir(directory)? {
        let name = entry?.file_name();
        ensure!(
            name == "set.json" || set.assets.keys().any(|key| name == key.filename()),
            "unknown retired model file"
        );
    }
    for name in set.assets.keys() {
        let path = directory.join(name.filename());
        if path.try_exists()? {
            fs::remove_file(path)?;
        }
    }
    sync_directory(directory)?;
    fs::remove_file(directory.join("set.json"))?;
    sync_directory(directory)?;
    fs::remove_dir(directory)?;
    Ok(())
}

fn directories(path: &Path, missing: bool) -> Result<()> {
    for path in path.ancestors().filter(|path| !path.as_os_str().is_empty()) {
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                ensure!(
                    meta.is_dir() && !meta.file_type().is_symlink(),
                    "redirected model directory"
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        meta.file_attributes() & 0x400 == 0,
                        "reparse model directory"
                    );
                }
            }
            Err(error) if missing && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn verify_bytes(path: &Path, expected: &[u8]) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    reject_file_aliases(path, &meta)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        meta.len() == expected.len() as u64 && bytes == expected,
        "shared model receipt mismatch"
    );
    Ok(())
}
fn read_receipt(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    reject_file_aliases(path, &meta)?;
    ensure!(
        meta.len() <= airk_core::model_objects::MAX_MODEL_OBJECT_SET_BYTES as u64,
        "oversized model receipt"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(airk_core::model_objects::MAX_MODEL_OBJECT_SET_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= airk_core::model_objects::MAX_MODEL_OBJECT_SET_BYTES,
        "growing model receipt"
    );
    Ok(bytes)
}
fn verify_asset(path: &Path, reference: &ModelObjectRef) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    reject_file_aliases(path, &meta)?;
    ensure!(meta.len() == reference.bytes, "shared model size mismatch");
    let mut file = File::open(path)?.take(reference.bytes + 1);
    let mut hash = Sha256::new();
    let mut length = 0u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        length += count as u64;
        hash.update(&buffer[..count]);
    }
    ensure!(
        length == reference.bytes
            && Sha256Digest::from_bytes(hash.finalize().into()) == reference.sha256,
        "shared model digest mismatch"
    );
    Ok(())
}
fn installed_bytes(set: &ModelObjectSet) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(
        &serde_json::json!({"format":1,"set":Sha256Digest::from_bytes(Sha256::digest(set.canonical_bytes()?).into())}),
    )?)
}
fn verify_prefix(path: &Path, expected: &[u8]) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    reject_file_aliases(path, &metadata)?;
    ensure!(
        metadata.len() <= expected.len() as u64,
        "oversized installed model intent"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        expected.starts_with(&bytes),
        "unknown installed model intent preserved"
    );
    Ok(())
}

fn validate_set(directory: &Path, set: &ModelObjectSet, partial: bool) -> Result<()> {
    directories(directory, false)?;
    verify_bytes(&directory.join("set.json"), &set.canonical_bytes()?)?;
    let mut count = 0;
    for entry in fs::read_dir(directory)? {
        count += 1;
        ensure!(count <= 11, "shared model file count exceeded");
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("invalid shared model asset name")?;
        if name == "set.json" {
            continue;
        }
        if name == "installed.json" {
            verify_bytes(&entry.path(), &installed_bytes(set)?)?;
            continue;
        }
        if name == "installed.part" {
            verify_prefix(&entry.path(), &installed_bytes(set)?)?;
            continue;
        }
        let asset = set
            .assets
            .iter()
            .find(|(key, _)| {
                name == key.filename() || (partial && name == format!("{}.part", key.filename()))
            })
            .context("unknown shared model file")?;
        reject_file_aliases(&entry.path(), &fs::symlink_metadata(entry.path())?)?;
        if name == asset.0.filename() {
            verify_asset(&entry.path(), asset.1)?;
        } else {
            ensure!(
                entry.metadata()?.len() <= asset.1.bytes,
                "oversized model staging file"
            );
        }
    }
    if !partial {
        for (name, reference) in &set.assets {
            verify_asset(&directory.join(name.filename()), reference)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "service_model_files_tests.rs"]
mod tests;
