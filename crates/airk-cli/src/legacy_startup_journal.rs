// SPDX-License-Identifier: Apache-2.0
//! Bounded control-owned retirement receipts. Loss never resets an initialized epoch.
use super::*;
use serde::de::DeserializeOwned;
use std::{
    fs::{File, OpenOptions},
    io::Write,
};
const MAX_ENTRIES: usize = 16;
const MAX_BYTES: u64 = 16384;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    Planned,
    Disabled,
    DefinitionRemoved,
    ReceiptRemoved,
    Complete,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    pub version: u32,
    pub review: Review,
    pub phase: Phase,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    version: u32,
    namespace: Incarnation,
    epoch: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ready {
    version: u32,
    namespace: Incarnation,
}
pub(super) struct Journal {
    root: PathBuf,
    directory: PathBuf,
    pub namespace: Incarnation,
    owner: Owner,
    _lock: Option<File>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    version: u32,
    value: T,
    sha256: Sha256Digest,
}
fn checksum(value: &impl Serialize) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"legacy-retirement-metadata/1.4",
        &[&serde_json::to_vec(value)?],
    ))
}
fn read<T: DeserializeOwned + Serialize>(path: &Path) -> Result<T> {
    crate::runtime_install::safe_file(path)?;
    ensure!(
        fs::metadata(path)?.len() <= MAX_BYTES,
        "legacy receipt exceeds bound"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "legacy receipt grew beyond bound"
    );
    let envelope: Envelope<T> = serde_json::from_slice(&bytes)?;
    ensure!(
        envelope.version == 1
            && envelope.sha256 == checksum(&envelope.value)?
            && serde_json::to_vec(&envelope)? == bytes,
        "legacy receipt checksum or representation changed"
    );
    Ok(envelope.value)
}
fn sync(directory: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(directory)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}
fn write<T: Serialize>(directory: &Path, name: &str, value: &T) -> Result<()> {
    write_at(directory, directory, name, value)
}
fn write_at<T: Serialize>(staging: &Path, directory: &Path, name: &str, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(&Envelope {
        version: 1,
        value,
        sha256: checksum(value)?,
    })?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "legacy receipt exceeds bound"
    );
    let mut temporary = tempfile::Builder::new()
        .prefix(".airk-legacy-")
        .tempfile_in(staging)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(directory.join(name))?;
    sync(staging)?;
    sync(directory)
}
impl Journal {
    pub fn saved(control: &Path) -> Result<Self> {
        let root = control.canonicalize()?;
        crate::runtime_install::validate_control_root(&root)?;
        let namespace =
            airk_store::service::registry::ServiceRegistry::open_saved(&root)?.incarnation()?;
        let directory = root.join("legacy-retirement-1.4");
        let ready = root.join("legacy-retirement-ready-1.4.json");
        let initialized = exists(&ready)?;
        if initialized {
            let ready: Ready = read(&ready)?;
            ensure!(
                ready.version == 1 && ready.namespace == namespace,
                "foreign legacy journal ready receipt"
            );
        }
        let owner = if exists(&directory)? {
            let metadata = fs::symlink_metadata(&directory)?;
            ensure!(
                metadata.is_dir()
                    && !metadata.file_type().is_symlink()
                    && directory.canonicalize()? == directory,
                "redirected legacy receipt directory"
            );
            if exists(&directory.join("owner.json"))? {
                let owner: Owner = read(&directory.join("owner.json"))?;
                ensure!(
                    owner.version == 1 && owner.namespace == namespace,
                    "foreign legacy receipt namespace"
                );
                owner
            } else {
                ensure!(
                    !initialized,
                    "initialized legacy journal owner is missing; preserve state"
                );
                Owner {
                    version: 1,
                    namespace,
                    epoch: 0,
                }
            }
        } else {
            ensure!(
                !initialized,
                "initialized legacy journal directory is missing; preserve state"
            );
            Owner {
                version: 1,
                namespace,
                epoch: 0,
            }
        };
        let journal = Self {
            root,
            directory,
            namespace,
            owner,
            _lock: None,
        };
        if exists(&journal.directory)? {
            let entries = journal.entries()?;
            ensure!(
                initialized || entries.is_empty(),
                "legacy initialization is incomplete; preserve existing receipts"
            );
            if !initialized {
                for entry in fs::read_dir(&journal.directory)? {
                    let entry = entry?;
                    if entry.file_name() == "owner.json" {
                        continue;
                    }
                    let path = entry.path();
                    let owned =
                        entry.file_name().to_str().is_some_and(|name| {
                            name.starts_with(".airk-legacy-") && name.len() <= 80
                        }) && (read::<Owner>(&path)
                            .is_ok_and(|owner| owner.version == 1 && owner.namespace == namespace)
                            || read::<Ready>(&path).is_ok_and(|ready| {
                                ready.version == 1 && ready.namespace == namespace
                            }));
                    ensure!(owned, "unknown legacy initialization file preserved");
                }
            }
        }
        Ok(journal)
    }
    pub fn locked(control: &Path) -> Result<Self> {
        let mut journal = Self::saved(control)?;
        let path = journal.root.join("legacy-retirement-1.4.lock");
        if exists(&path)? {
            crate::runtime_install::safe_file(&path)?;
            ensure!(
                fs::metadata(&path)?.len() == 0,
                "changed legacy lock preserved"
            );
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        crate::runtime_install::safe_file(&path)?;
        file.try_lock()
            .context("another legacy removal operation is active")?;
        journal = Self::saved(&journal.root)?;
        journal._lock = Some(file);
        if !exists(&journal.directory)? {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&journal.directory)?;
            }
            #[cfg(not(unix))]
            fs::create_dir(&journal.directory)?;
            sync(&journal.root)?;
        }
        journal.sweep()?;
        if !exists(&journal.directory.join("owner.json"))? {
            write(&journal.directory, "owner.json", &journal.owner)?;
        }
        if !exists(&journal.root.join("legacy-retirement-ready-1.4.json"))? {
            write_at(
                &journal.directory,
                &journal.root,
                "legacy-retirement-ready-1.4.json",
                &Ready {
                    version: 1,
                    namespace: journal.namespace,
                },
            )?;
        }
        Ok(journal)
    }
    pub fn epoch(&self) -> u64 {
        self.owner.epoch
    }
    fn entries(&self) -> Result<Vec<Sha256Digest>> {
        if !exists(&self.directory)? {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        let mut scratch = 0;
        for (count, entry) in fs::read_dir(&self.directory)?.enumerate() {
            ensure!(
                count < MAX_ENTRIES + 65,
                "legacy journal directory budget exceeded"
            );
            let entry = entry?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .context("unknown legacy journal name preserved")?;
            if name == "owner.json" {
                continue;
            }
            if name.starts_with(".airk-legacy-") && name.len() <= 80 {
                scratch += 1;
                ensure!(scratch <= 64, "legacy scratch budget exceeded");
                continue;
            }
            let approval: Sha256Digest = name
                .strip_suffix(".json")
                .context("unknown legacy journal file preserved")?
                .parse()?;
            ensure!(
                format!("{approval}.json") == name,
                "noncanonical legacy journal file preserved"
            );
            entries.push(approval);
            ensure!(
                entries.len() <= MAX_ENTRIES,
                "legacy receipt capacity exceeded"
            );
        }
        entries.sort();
        Ok(entries)
    }
    fn sweep(&self) -> Result<()> {
        for entry in fs::read_dir(&self.directory)?.take(MAX_ENTRIES + 65) {
            let entry = entry?;
            let name = entry.file_name();
            if !name
                .to_str()
                .is_some_and(|name| name.starts_with(".airk-legacy-") && name.len() <= 80)
            {
                continue;
            }
            let path = entry.path();
            let owned = read::<Intent>(&path).is_ok_and(|intent| {
                intent.version == 1
                    && intent.review.approval.is_some()
                    && intent.review.validate(self.namespace).is_ok()
            }) || read::<Owner>(&path)
                .is_ok_and(|owner| owner.version == 1 && owner.namespace == self.namespace)
                || read::<Ready>(&path)
                    .is_ok_and(|ready| ready.version == 1 && ready.namespace == self.namespace);
            if owned {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
    pub fn get(&self, approval: Sha256Digest) -> Result<Option<Intent>> {
        let path = self.directory.join(format!("{approval}.json"));
        if !exists(&path)? {
            return Ok(None);
        }
        let intent: Intent = read(&path)?;
        intent.review.validate(self.namespace)?;
        ensure!(
            intent.version == 1
                && intent.review.approval == Some(approval)
                && intent.review.preview.epoch <= self.owner.epoch,
            "foreign or changed legacy intent"
        );
        Ok(Some(intent))
    }
    pub fn save(&mut self, intent: &Intent) -> Result<()> {
        ensure!(intent.version == 1, "unsupported legacy receipt version");
        ensure!(self._lock.is_some(), "legacy mutation lock required");
        intent.review.validate(self.namespace)?;
        let approval = intent
            .review
            .approval
            .context("legacy removal was not approved")?;
        if let Some(prior) = self.get(approval)? {
            ensure!(
                prior.review == intent.review && prior.phase <= intent.phase,
                "legacy receipt changed or regressed"
            );
        } else {
            ensure!(
                intent.phase == Phase::Planned
                    && intent.review.preview.epoch == self.owner.epoch
                    && self.entries()?.len() < MAX_ENTRIES,
                "legacy receipt capacity or epoch expired"
            );
        }
        write(&self.directory, &format!("{approval}.json"), intent)
    }
    pub fn advance_epoch(&mut self, minimum: u64) -> Result<()> {
        ensure!(self._lock.is_some(), "legacy mutation lock required");
        if self.owner.epoch < minimum {
            self.owner.epoch = minimum;
            write(&self.directory, "owner.json", &self.owner)?;
        }
        Ok(())
    }
    pub fn forget(&mut self, approval: Sha256Digest) -> Result<()> {
        let intent = self.get(approval)?.context("legacy receipt not retained")?;
        ensure!(
            intent.phase == Phase::Complete,
            "pending legacy removal cannot be forgotten"
        );
        self.advance_epoch(
            self.owner
                .epoch
                .checked_add(1)
                .context("legacy epoch exhausted")?,
        )?;
        fs::remove_file(self.directory.join(format!("{approval}.json")))?;
        sync(&self.directory)
    }
    pub fn list(&self) -> Result<serde_json::Value> {
        let items=self.entries()?.into_iter().map(|approval|->Result<_>{let intent=self.get(approval)?.context("legacy receipt disappeared")?;Ok(serde_json::json!({"approval":approval,"service_id":intent.review.preview.spec.id,"phase":intent.phase}))}).collect::<Result<Vec<_>>>()?;
        Ok(
            serde_json::json!({"namespace":self.namespace,"epoch":self.owner.epoch,"items":items,"limit":MAX_ENTRIES}),
        )
    }
}
