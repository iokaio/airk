// SPDX-License-Identifier: Apache-2.0
//! Descriptor-relative, explicit optional-model assets under the real service lease.
use super::ServiceLease;
use crate::store_fs::{SetupDirectory, SetupFileMode, file_identity};
use airk_core::{
    arbiter::{Asset, Profile},
    identity::{Incarnation, Sha256Digest},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    sync::{Arc, Mutex, Weak},
};

const RECEIPT: &str = "receipt.json";
fn checked_assets(assets: &[Asset]) -> Result<()> {
    ensure!(
        !assets.is_empty() && assets.len() <= 128,
        "Arbiter stored asset roster bound"
    );
    let mut names = std::collections::BTreeSet::new();
    for asset in assets {
        ensure!(
            !asset.name.is_empty()
                && asset.name.len() <= 128
                && asset
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
                && !matches!(
                    asset.name.as_str(),
                    "." | ".." | "receipt.json" | "writer.lock" | "readers.lock" | "operation.json"
                )
                && !asset.name.starts_with("receipt-")
                && !asset.name.starts_with("stage-")
                && !asset.name.ends_with(".partial.json")
                && names.insert(&asset.name)
                && asset.bytes > 0
                && asset.bytes <= 16_u64 << 30,
            "Arbiter stored asset name/length collides with native metadata or leaves its fixed directory"
        );
    }
    Ok(())
}
fn checked_profile(profile: &Profile) -> Result<()> {
    profile.validate()?;
    checked_assets(&profile.assets)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Downloading,
    Ready,
    Canceled,
    Failed,
    Removing,
    Removed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    pub phase: Phase,
    pub configuration_revision: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: String,
    installation: Sha256Digest,
    catalog: Incarnation,
    manifest: Sha256Digest,
    revision: u64,
    configuration_revision: u64,
    phase: Phase,
    assets: Vec<Asset>,
    owned_assets: BTreeMap<String, (u64, u64)>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialRecord {
    schema: String,
    operation: Record,
    asset: Asset,
    written_bytes: u64,
    sha256: Sha256Digest,
    file_identity: (u64, u64),
}
fn missing(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
}
fn canonical<T: Serialize>(record: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(record)?;
    bytes.push(b'\n');
    ensure!(bytes.len() <= 131072, "Arbiter asset receipt bound");
    Ok(bytes)
}
fn next_revision(directory: &SetupDirectory, previous: u64) -> Result<u64> {
    let mut revision = previous;
    for _ in 0..1024 {
        revision = revision
            .checked_add(1)
            .context("Arbiter asset revision exhausted")?;
        let name = format!("receipt-{revision}.part");
        match directory.open_file(OsStr::new(&name), SetupFileMode::Read) {
            Err(error) if missing(&error) => return Ok(revision),
            Ok(_) => {}
            Err(error) => return Err(error),
        }
    }
    anyhow::bail!("Arbiter retained receipt recovery capacity exhausted")
}
fn hash(file: &mut File, asset: &Asset) -> Result<()> {
    ensure!(
        file.metadata()?.len() == asset.bytes,
        "Arbiter asset length differs"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut remaining = asset.bytes;
    let mut buffer = [0_u8; 65536];
    while remaining > 0 {
        let limit = usize::try_from(remaining.min(buffer.len() as u64))?;
        let n = file.read(&mut buffer[..limit])?;
        ensure!(n > 0, "Arbiter asset shortened");
        digest.update(&buffer[..n]);
        remaining -= u64::try_from(n)?;
    }
    let mut extra = [0];
    ensure!(
        file.read(&mut extra)? == 0
            && Sha256Digest::from_bytes(digest.finalize().into()) == asset.sha256,
        "Arbiter asset bytes changed"
    );
    file.seek(SeekFrom::Start(0))?;
    Ok(())
}

#[cfg(windows)]
fn same_directory(
    raw: &airk_process::directory::OpenedDirectory,
    retained: &SetupDirectory,
) -> Result<()> {
    ensure!(
        raw.identity()? == airk_process::directory::identity(&retained.file)?,
        "Arbiter removal directory changed; bytes preserved"
    );
    Ok(())
}

#[cfg(windows)]
fn remove_verified(
    directory: &airk_process::directory::OpenedDirectory,
    name: &OsStr,
    expected: &Asset,
    identity: Option<(u64, u64)>,
) -> Result<()> {
    remove_verified_with(directory, name, expected, identity, |_| Ok(()))
}

#[cfg(windows)]
fn remove_verified_with(
    directory: &airk_process::directory::OpenedDirectory,
    name: &OsStr,
    expected: &Asset,
    identity: Option<(u64, u64)>,
    before_dispose: impl FnOnce(&File) -> Result<()>,
) -> Result<()> {
    directory.remove_file_if(name, |file, proof| {
        proof.validate(file)?;
        directory.require_default_stream(name, file)?;
        let result = (|| -> Result<()> {
            if let Some(identity) = identity {
                ensure!(
                    file_identity(file)? == identity,
                    "Arbiter removal identity changed; bytes preserved"
                );
            }
            hash(file, expected)?;
            before_dispose(file)
        })();
        result.map_err(std::io::Error::other)?;
        Ok(true)
    })?;
    Ok(())
}

pub struct AssetStore {
    root: SetupDirectory,
    installation: Sha256Digest,
    catalog: Incarnation,
    ownership: Arc<ServiceLease>,
    pins: Mutex<BTreeMap<Sha256Digest, Weak<()>>>,
}
impl AssetStore {
    pub fn open(
        ownership: Arc<ServiceLease>,
        installation: Sha256Digest,
        catalog: Incarnation,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            root: SetupDirectory::open_canonical(ownership.root())?,
            installation,
            catalog,
            ownership,
            pins: Mutex::new(BTreeMap::new()),
        }))
    }
    fn parent(&self, create: bool) -> Result<SetupDirectory> {
        let optional = if create {
            self.root.create_dir(OsStr::new("arbiter-v2"))?
        } else {
            self.root.open_dir(OsStr::new("arbiter-v2"))?
        };
        if create {
            optional.create_dir(OsStr::new("assets"))
        } else {
            optional.open_dir(OsStr::new("assets"))
        }
    }
    fn directory(&self, manifest: Sha256Digest, create: bool) -> Result<SetupDirectory> {
        let parent = self.parent(create)?;
        let name = manifest.to_string();
        if create {
            parent.create_dir(OsStr::new(&name))
        } else {
            parent.open_dir(OsStr::new(&name))
        }
    }
    fn read_owned(
        &self,
        directory: &SetupDirectory,
        manifest: Sha256Digest,
    ) -> Result<Option<(Record, File)>> {
        let file = match directory.open_file(OsStr::new(RECEIPT), SetupFileMode::Read) {
            Ok(file) => file,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        (&file).take(131073).read_to_end(&mut bytes)?;
        let record: Record = serde_json::from_slice(&bytes)?;
        ensure!(
            canonical(&record)? == bytes
                && record.schema == "airk.arbiter.assets/2"
                && record.installation == self.installation
                && record.catalog == self.catalog
                && record.manifest == manifest
                && record.revision > 0
                && record.configuration_revision > 0,
            "Arbiter owned asset receipt identity differs"
        );
        checked_assets(&record.assets)?;
        ensure!(
            record
                .owned_assets
                .keys()
                .all(|name| record.assets.iter().any(|asset| &asset.name == name)),
            "Arbiter owned asset identities leave the declared closure"
        );
        ensure!(
            record.phase != Phase::Ready || record.owned_assets.len() == record.assets.len(),
            "Arbiter ready receipt lacks original object custody"
        );
        Ok(Some((record, file)))
    }
    fn read(
        &self,
        directory: &SetupDirectory,
        profile: &Profile,
    ) -> Result<Option<(Record, File)>> {
        let record = self.read_owned(directory, profile.manifest)?;
        if let Some((record, _)) = &record {
            ensure!(
                record.assets == profile.assets,
                "Arbiter assets differ from current compiled profile"
            );
        }
        Ok(record)
    }
    /// Cleanup observation is not a catalog, profile or execution capability.
    pub fn owned_observation(&self, manifest: Sha256Digest) -> Result<Option<Observation>> {
        let directory = match self.directory(manifest, false) {
            Ok(value) => value,
            Err(error) if missing(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(self
            .read_owned(&directory, manifest)?
            .map(|(record, _)| Observation {
                phase: record.phase,
                configuration_revision: record.configuration_revision,
            }))
    }
    /// Explicit recovery may find an old removal without qualifying its model.
    pub fn pending_owned_removals(&self, configuration_revision: u64) -> Result<Vec<Sha256Digest>> {
        let parent = match self.parent(false) {
            Ok(value) => value,
            Err(error) if missing(&error) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut result = Vec::new();
        let mut count = 0_usize;
        for name in parent.entries()? {
            count = count
                .checked_add(1)
                .context("Arbiter owned profile census overflow")?;
            ensure!(count <= 1024, "Arbiter owned profile recovery census bound");
            let name = name?;
            let text = name
                .to_str()
                .context("Arbiter owned profile identity is not UTF-8")?;
            let manifest: Sha256Digest = text.parse()?;
            ensure!(
                manifest.to_string() == text,
                "Arbiter owned profile identity is not canonical"
            );
            let directory = parent.open_dir(&name)?;
            if let Some((record, _)) = self.read_owned(&directory, manifest)?
                && record.phase == Phase::Removing
                && record.configuration_revision == configuration_revision
            {
                result.push(manifest);
            }
        }
        Ok(result)
    }
    fn write(
        &self,
        directory: &SetupDirectory,
        record: &Record,
        prior: Option<&File>,
    ) -> Result<()> {
        let bytes = canonical(record)?;
        let name = format!("receipt-{}.part", record.revision);
        let mut stage = directory.open_file(OsStr::new(&name), SetupFileMode::CreateNew)?;
        stage.write_all(&bytes)?;
        stage.sync_all()?;
        if let Some(prior) = prior {
            directory.replace_file(OsStr::new(&name), &stage, OsStr::new(RECEIPT), prior)?;
        } else {
            directory.publish_no_replace(
                OsStr::new(&name),
                &stage,
                directory,
                OsStr::new(RECEIPT),
            )?;
        }
        directory.sync()
    }
    pub fn phase(&self, profile: &Profile) -> Result<Option<Phase>> {
        Ok(self.observation(profile)?.map(|record| record.phase))
    }
    pub fn observation(&self, profile: &Profile) -> Result<Option<Observation>> {
        checked_profile(profile)?;
        let directory = match self.directory(profile.manifest, false) {
            Ok(d) => d,
            Err(e) if missing(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        Ok(self
            .read(&directory, profile)?
            .map(|(record, _)| Observation {
                phase: record.phase,
                configuration_revision: record.configuration_revision,
            }))
    }
    /// Explicit operation only; a fresh stage never adopts interrupted bytes.
    pub fn begin(
        self: &Arc<Self>,
        profile: &Profile,
        configuration_revision: u64,
    ) -> Result<Transaction> {
        checked_profile(profile)?;
        ensure!(
            configuration_revision > 0,
            "Arbiter configuration revision missing"
        );
        let total = profile.assets.iter().try_fold(0_u64, |total, asset| {
            total
                .checked_add(asset.bytes)
                .context("Arbiter asset closure overflow")
        })?;
        ensure!(
            profile.peak_disk_bytes >= total,
            "Arbiter resource profile understates logical asset closure"
        );
        let directory = self.directory(profile.manifest, true)?;
        let lock = match directory.open_file(OsStr::new("writer.lock"), SetupFileMode::CreateNew) {
            Ok(f) => f,
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists) =>
            {
                directory.open_file(OsStr::new("writer.lock"), SetupFileMode::ReadWrite)?
            }
            Err(e) => return Err(e),
        };
        ensure!(
            lock.metadata()?.len() == 0,
            "invalid Arbiter asset writer lock"
        );
        lock.try_lock()
            .context("Arbiter asset operation already active")?;
        let readers =
            match directory.open_file(OsStr::new("readers.lock"), SetupFileMode::CreateNew) {
                Ok(file) => file,
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) =>
                {
                    directory.open_file(OsStr::new("readers.lock"), SetupFileMode::ReadWrite)?
                }
                Err(error) => return Err(error),
            };
        ensure!(
            readers.metadata()?.len() == 0,
            "invalid Arbiter asset reader lock"
        );
        let prior = self.read(&directory, profile)?;
        if prior.is_none() {
            let mut count = 0_usize;
            for name in directory.entries()? {
                count = count
                    .checked_add(1)
                    .context("Arbiter unowned directory count overflow")?;
                let name = name?;
                ensure!(
                    count <= 2
                        && (name == OsStr::new("writer.lock")
                            || name == OsStr::new("readers.lock")),
                    "Arbiter profile directory has unowned entries; they were preserved"
                );
            }
        }
        let revision = next_revision(
            &directory,
            prior.as_ref().map_or(0, |(record, _)| record.revision),
        )?;
        let record = Record {
            schema: "airk.arbiter.assets/2".into(),
            installation: self.installation,
            catalog: self.catalog,
            manifest: profile.manifest,
            revision,
            configuration_revision,
            phase: Phase::Downloading,
            assets: profile.assets.clone(),
            owned_assets: prior
                .as_ref()
                .map_or_else(BTreeMap::new, |(record, _)| record.owned_assets.clone()),
        };
        self.write(&directory, &record, prior.as_ref().map(|(_, file)| file))?;
        let stage_name = format!("stage-{revision}");
        match directory.open_dir(OsStr::new(&stage_name)) {
            Err(e) if missing(&e) => {}
            Ok(_) => anyhow::bail!("Arbiter staging generation already exists"),
            Err(e) => return Err(e),
        };
        let mut remaining = 131072_u64;
        for asset in &profile.assets {
            match directory.open_file(OsStr::new(&asset.name), SetupFileMode::Read) {
                Ok(mut file) => {
                    ensure!(
                        record.owned_assets.get(&asset.name) == Some(&file_identity(&file)?),
                        "Arbiter existing asset lacks original object custody; bytes preserved"
                    );
                    hash(&mut file, asset)?;
                }
                Err(error) if missing(&error) => {
                    remaining = remaining
                        .checked_add(asset.bytes)
                        .context("Arbiter remaining asset disk overflow")?;
                }
                Err(error) => return Err(error),
            }
        }
        directory.require_growth(remaining)?;
        let stage = directory.create_dir(OsStr::new(&stage_name))?;
        let mut intent = stage.open_file(OsStr::new("operation.json"), SetupFileMode::CreateNew)?;
        intent.write_all(&canonical(&record)?)?;
        intent.sync_all()?;
        stage.sync()?;
        Ok(Transaction {
            store: self.clone(),
            directory,
            stage,
            profile: profile.clone(),
            record,
            _lock: lock,
            settled: false,
        })
    }
    pub fn pin(&self, profile: &Profile) -> Result<PinnedAssets> {
        checked_profile(profile)?;
        let mut pins = self
            .pins
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter asset pins unavailable"))?;
        let directory = self.directory(profile.manifest, false)?;
        let readers = directory.open_file(OsStr::new("readers.lock"), SetupFileMode::ReadWrite)?;
        ensure!(
            readers.metadata()?.len() == 0,
            "invalid Arbiter asset reader lock"
        );
        readers
            .try_lock_shared()
            .context("Arbiter asset removal is active")?;
        let (record, _) = self
            .read(&directory, profile)?
            .context("Arbiter asset receipt missing")?;
        ensure!(
            record.phase == Phase::Ready,
            "Arbiter assets require explicit provisioning or recovery"
        );
        let mut files = BTreeMap::new();
        for asset in &profile.assets {
            let mut file = directory.open_file(OsStr::new(&asset.name), SetupFileMode::Read)?;
            ensure!(
                record.owned_assets.get(&asset.name) == Some(&file_identity(&file)?),
                "Arbiter pinned asset identity changed; foreign bytes preserved"
            );
            hash(&mut file, asset)?;
            files.insert(asset.name.clone(), (asset.clone(), Mutex::new(file)));
        }
        let pin = pins
            .get(&profile.manifest)
            .and_then(Weak::upgrade)
            .unwrap_or_else(|| {
                let pin = Arc::new(());
                pins.insert(profile.manifest, Arc::downgrade(&pin));
                pin
            });
        Ok(PinnedAssets {
            files,
            _pin: pin,
            _ownership: self.ownership.clone(),
            _readers: readers,
        })
    }
    pub fn remove(&self, profile: &Profile, configuration_revision: u64) -> Result<()> {
        checked_profile(profile)?;
        let directory = self.directory(profile.manifest, false)?;
        self.read(&directory, profile)?
            .context("Arbiter asset receipt missing")?;
        self.remove_owned(profile.manifest, configuration_revision)
    }
    /// Explicit cleanup only. This receipt can never qualify execution.
    pub fn remove_owned(&self, manifest: Sha256Digest, configuration_revision: u64) -> Result<()> {
        ensure!(
            configuration_revision > 0,
            "Arbiter removal configuration revision missing"
        );
        let pins = self
            .pins
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter asset pins unavailable"))?;
        ensure!(
            !pins
                .get(&manifest)
                .is_some_and(|pin| pin.strong_count() > 0),
            "Arbiter assets remain pinned by native work"
        );
        let directory = self.directory(manifest, false)?;
        #[cfg(windows)]
        let raw = {
            use airk_process::directory::OpenedDirectory;
            let root = OpenedDirectory::open(self.ownership.root())?;
            same_directory(&root, &self.root)?;
            let raw = root
                .open_child(OsStr::new("arbiter-v2"))?
                .open_child(OsStr::new("assets"))?
                .open_child(OsStr::new(&manifest.to_string()))?;
            same_directory(&raw, &directory)?;
            raw
        };
        let lock = directory.open_file(OsStr::new("writer.lock"), SetupFileMode::ReadWrite)?;
        lock.try_lock()
            .context("Arbiter asset operation remains active")?;
        let readers = directory.open_file(OsStr::new("readers.lock"), SetupFileMode::ReadWrite)?;
        ensure!(
            readers.metadata()?.len() == 0,
            "invalid Arbiter asset reader lock"
        );
        readers
            .try_lock()
            .context("Arbiter asset descriptors remain pinned")?;
        let (mut record, prior) = self
            .read_owned(&directory, manifest)?
            .context("Arbiter asset receipt missing")?;
        record.revision = next_revision(&directory, record.revision)?;
        record.phase = Phase::Removing;
        record.configuration_revision = configuration_revision;
        self.write(&directory, &record, Some(&prior))?;
        for asset in &record.assets {
            if !record.owned_assets.contains_key(&asset.name) {
                match directory.open_file(OsStr::new(&asset.name), SetupFileMode::Read) {
                    Err(error) if missing(&error) => continue,
                    Err(error) => return Err(error),
                    Ok(_) => anyhow::bail!(
                        "Arbiter removal lacks original object custody; bytes preserved"
                    ),
                }
            }
            #[cfg(windows)]
            match remove_verified(
                &raw,
                OsStr::new(&asset.name),
                asset,
                record.owned_assets.get(&asset.name).copied(),
            ) {
                Ok(()) => {}
                Err(error) if missing(&error) => {}
                Err(error) => return Err(error),
            }
            #[cfg(not(windows))]
            match directory.open_file(OsStr::new(&asset.name), SetupFileMode::Read) {
                Ok(mut file) => {
                    ensure!(
                        record.owned_assets.get(&asset.name) == Some(&file_identity(&file)?),
                        "Arbiter removal lacks original object custody; bytes preserved"
                    );
                    hash(&mut file, asset)?;
                    directory.unlink_file_if_identity(OsStr::new(&asset.name), &file)?;
                }
                Err(e) if missing(&e) => {}
                Err(e) => return Err(e),
            }
        }
        #[cfg(not(windows))]
        cleanup_stages(&directory, &record)?;
        #[cfg(windows)]
        cleanup_stages(&directory, &record, &raw)?;
        record.revision = next_revision(&directory, record.revision)?;
        record.phase = Phase::Removed;
        let (_, removing) = self
            .read_owned(&directory, manifest)?
            .context("Arbiter removal receipt disappeared")?;
        self.write(&directory, &record, Some(&removing))?;
        drop(pins);
        Ok(())
    }
}

fn cleanup_stages(
    directory: &SetupDirectory,
    record: &Record,
    #[cfg(windows)] raw: &airk_process::directory::OpenedDirectory,
) -> Result<()> {
    let mut names = Vec::new();
    for name in directory.entries()? {
        ensure!(names.len() < 2048, "Arbiter cleanup entry bound");
        names.push(name?);
    }
    for name in names {
        let text = name
            .to_str()
            .context("Arbiter cleanup entry is not UTF-8")?;
        if matches!(text, RECEIPT | "writer.lock" | "readers.lock")
            || record.assets.iter().any(|asset| asset.name == text)
        {
            continue;
        }
        if text.starts_with("receipt-") && text.ends_with(".part") {
            continue;
        }
        let Some(revision) = text.strip_prefix("stage-") else {
            anyhow::bail!("Arbiter cleanup found an unproven foreign entry; it was preserved");
        };
        let revision = revision.parse::<u64>()?;
        ensure!(
            text == format!("stage-{revision}"),
            "Arbiter stage identity differs"
        );
        let stage = directory.open_dir(&name)?;
        #[cfg(windows)]
        let raw_stage = raw.open_child(&name)?;
        #[cfg(windows)]
        same_directory(&raw_stage, &stage)?;
        let operation = stage.read_file(OsStr::new("operation.json"), 131072)?;
        let intent: Record = serde_json::from_slice(&operation)?;
        ensure!(
            canonical(&intent)? == operation
                && intent.schema == record.schema
                && intent.installation == record.installation
                && intent.catalog == record.catalog
                && intent.manifest == record.manifest
                && intent.assets == record.assets
                && intent.revision == revision
                && intent.phase == Phase::Downloading,
            "Arbiter retained stage operation identity differs"
        );
        let mut entries = Vec::new();
        for entry in stage.entries()? {
            ensure!(entries.len() < 257, "Arbiter staged closure bound");
            entries.push(entry?);
        }
        for entry in &entries {
            let text = entry
                .to_str()
                .context("Arbiter staged entry is not UTF-8")?;
            ensure!(
                text == "operation.json"
                    || record.assets.iter().any(|asset| text == asset.name
                        || text == format!("{}.partial.json", asset.name)),
                "Arbiter staged foreign entry was preserved"
            );
        }
        for asset in &record.assets {
            let proof_name = format!("{}.partial.json", asset.name);
            let proof = match stage.open_file(OsStr::new(&proof_name), SetupFileMode::Read) {
                Ok(file) => Some(file),
                Err(error) if missing(&error) => None,
                Err(error) => return Err(error),
            };
            let file = match stage.open_file(OsStr::new(&asset.name), SetupFileMode::Read) {
                Ok(file) => Some(file),
                Err(error) if missing(&error) => None,
                Err(error) => return Err(error),
            };
            if file.is_none() && proof.is_none() {
                continue;
            }
            let proof=proof.context("Unsealed interrupted Arbiter asset was preserved; explicit manual recovery is required")?;
            let mut bytes = Vec::new();
            (&proof).take(131073).read_to_end(&mut bytes)?;
            let partial: PartialRecord = serde_json::from_slice(&bytes)?;
            ensure!(
                canonical(&partial)? == bytes
                    && partial.schema == "airk.arbiter.partial/2"
                    && partial.operation == intent
                    && partial.asset == *asset
                    && partial.written_bytes <= asset.bytes,
                "Arbiter partial ownership receipt differs"
            );
            if let Some(file) = file {
                ensure!(
                    file_identity(&file)? == partial.file_identity,
                    "Arbiter partial identity was replaced; bytes preserved"
                );
                let mut observed = asset.clone();
                observed.bytes = partial.written_bytes;
                observed.sha256 = partial.sha256;
                #[cfg(not(windows))]
                {
                    let mut file = file;
                    hash(&mut file, &observed)?;
                    stage.unlink_file_if_identity(OsStr::new(&asset.name), &file)?;
                }
                #[cfg(windows)]
                remove_verified(
                    &raw_stage,
                    OsStr::new(&asset.name),
                    &observed,
                    Some(partial.file_identity),
                )?;
            }
            #[cfg(not(windows))]
            stage.unlink_file_if_identity(OsStr::new(&proof_name), &proof)?;
            #[cfg(windows)]
            {
                let mut expected = asset.clone();
                expected.bytes = u64::try_from(bytes.len())?;
                expected.sha256 = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
                remove_verified(
                    &raw_stage,
                    OsStr::new(&proof_name),
                    &expected,
                    Some(file_identity(&proof)?),
                )?;
            }
        }
    }
    Ok(())
}

pub struct Transaction {
    store: Arc<AssetStore>,
    directory: SetupDirectory,
    stage: SetupDirectory,
    profile: Profile,
    record: Record,
    _lock: File,
    settled: bool,
}
impl Transaction {
    pub fn asset(&self, asset: &Asset) -> Result<Option<AssetWriter>> {
        ensure!(self.profile.assets.contains(asset), "foreign Arbiter asset");
        match self
            .directory
            .open_file(OsStr::new(&asset.name), SetupFileMode::Read)
        {
            Ok(mut file) => {
                let (record, _) = self
                    .store
                    .read(&self.directory, &self.profile)?
                    .context("Arbiter original asset custody is missing")?;
                ensure!(
                    record.owned_assets.get(&asset.name) == Some(&file_identity(&file)?),
                    "Arbiter existing asset identity changed; foreign bytes preserved"
                );
                hash(&mut file, asset)?;
                return Ok(None);
            }
            Err(e) if missing(&e) => {}
            Err(e) => return Err(e),
        }
        let file = self
            .stage
            .open_file(OsStr::new(&asset.name), SetupFileMode::CreateNew)?;
        Ok(Some(AssetWriter {
            file,
            asset: asset.clone(),
            bytes: 0,
            hash: Sha256::new(),
            _ownership: self.store.ownership.clone(),
            stage: self.stage.try_clone()?,
            operation: self.record.clone(),
            published: false,
        }))
    }
    pub fn publish(&self, writer: AssetWriter) -> Result<()> {
        self.publish_with(writer, || Ok(()))
    }
    fn publish_with(
        &self,
        mut writer: AssetWriter,
        before_publish: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            writer.operation == self.record && self.profile.assets.contains(&writer.asset),
            "Arbiter publication belongs to another original operation"
        );
        ensure!(
            writer.bytes == writer.asset.bytes
                && Sha256Digest::from_bytes(writer.hash.clone().finalize().into())
                    == writer.asset.sha256,
            "Arbiter logical asset digest differs"
        );
        writer.file.flush()?;
        writer.file.sync_all()?;
        let source = self
            .stage
            .open_file(OsStr::new(&writer.asset.name), SetupFileMode::Read)?;
        ensure!(
            file_identity(&source)? == file_identity(&writer.file)?,
            "Arbiter original staged object changed"
        );
        let (mut custody, prior) = self
            .store
            .read(&self.directory, &self.profile)?
            .context("Arbiter original publication receipt missing")?;
        ensure!(
            custody.revision == self.record.revision && custody.phase == Phase::Downloading,
            "Arbiter original publication revision changed"
        );
        custody
            .owned_assets
            .insert(writer.asset.name.clone(), file_identity(&writer.file)?);
        self.store.write(&self.directory, &custody, Some(&prior))?;
        before_publish()?;
        self.stage.publish_no_replace(
            OsStr::new(&writer.asset.name),
            &writer.file,
            &self.directory,
            OsStr::new(&writer.asset.name),
        )?;
        writer.published = true;
        self.directory.sync()
    }
    pub fn finish(mut self) -> Result<()> {
        let (custody, _) = self
            .store
            .read(&self.directory, &self.profile)?
            .context("Arbiter final custody receipt missing")?;
        for asset in &self.profile.assets {
            let mut file = self
                .directory
                .open_file(OsStr::new(&asset.name), SetupFileMode::Read)?;
            ensure!(
                custody.owned_assets.get(&asset.name) == Some(&file_identity(&file)?),
                "Arbiter final asset object differs from original publication"
            );
            hash(&mut file, asset)?;
        }
        self.settle(Phase::Ready)
    }
    pub fn cancel(mut self) -> Result<()> {
        self.settle(Phase::Canceled)
    }
    fn settle(&mut self, phase: Phase) -> Result<()> {
        let (prior, file) = self
            .store
            .read(&self.directory, &self.profile)?
            .context("Arbiter asset transaction disappeared")?;
        ensure!(
            prior.revision == self.record.revision
                && prior.configuration_revision == self.record.configuration_revision,
            "Arbiter asset transaction revision changed"
        );
        self.record.revision = next_revision(&self.directory, self.record.revision)?;
        self.record.owned_assets = prior.owned_assets;
        self.record.phase = phase;
        self.store
            .write(&self.directory, &self.record, Some(&file))?;
        self.settled = true;
        Ok(())
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.settled {
            let _ = self.settle(Phase::Failed);
        }
    }
}
pub struct AssetWriter {
    _ownership: Arc<ServiceLease>,
    file: File,
    asset: Asset,
    bytes: u64,
    hash: Sha256,
    stage: SetupDirectory,
    operation: Record,
    published: bool,
}
impl AssetWriter {
    fn seal_partial(&mut self) -> Result<()> {
        if self.published {
            return Ok(());
        }
        self.file.sync_all()?;
        let mut actual = self
            .stage
            .open_file(OsStr::new(&self.asset.name), SetupFileMode::Read)?;
        ensure!(
            file_identity(&actual)? == file_identity(&self.file)?
                && actual.metadata()?.len() == self.bytes,
            "Arbiter partial object changed"
        );
        let sha256 = Sha256Digest::from_bytes(self.hash.clone().finalize().into());
        let mut partial = self.asset.clone();
        partial.bytes = self.bytes;
        partial.sha256 = sha256;
        hash(&mut actual, &partial)?;
        let record = PartialRecord {
            schema: "airk.arbiter.partial/2".into(),
            operation: self.operation.clone(),
            asset: self.asset.clone(),
            written_bytes: self.bytes,
            sha256,
            file_identity: file_identity(&actual)?,
        };
        let name = format!("{}.partial.json", self.asset.name);
        let mut receipt = self
            .stage
            .open_file(OsStr::new(&name), SetupFileMode::CreateNew)?;
        receipt.write_all(&canonical(&record)?)?;
        receipt.sync_all()?;
        self.stage.sync()
    }
}
impl Drop for AssetWriter {
    fn drop(&mut self) {
        let _ = self.seal_partial();
    }
}
impl Write for AssetWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let size = u64::try_from(bytes.len()).map_err(std::io::Error::other)?;
        if self
            .bytes
            .checked_add(size)
            .is_none_or(|n| n > self.asset.bytes)
        {
            return Err(std::io::Error::other(
                "Arbiter logical asset exceeds approved length",
            ));
        }
        let n = self.file.write(bytes)?;
        self.hash.update(&bytes[..n]);
        self.bytes += u64::try_from(n).map_err(std::io::Error::other)?;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
pub struct PinnedAssets {
    _readers: File,
    files: BTreeMap<String, (Asset, Mutex<File>)>,
    _pin: Arc<()>,
    _ownership: Arc<ServiceLease>,
}
impl PinnedAssets {
    pub fn read(&self, asset: &Asset) -> Result<Vec<u8>> {
        let (declared, file) = self
            .files
            .get(&asset.name)
            .context("Arbiter asset absent from pinned closure")?;
        ensure!(declared == asset, "Arbiter asset declaration changed");
        let mut file = file
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter asset descriptor unavailable"))?;
        hash(&mut file, asset)?;
        let length = usize::try_from(asset.bytes)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .context("Arbiter asset allocation unavailable")?;
        (&mut *file).take(asset.bytes + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() == length
                && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == asset.sha256,
            "Arbiter pinned asset changed during read"
        );
        Ok(bytes)
    }
}
#[cfg(test)]
mod tests;
