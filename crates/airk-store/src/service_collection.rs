// SPDX-License-Identifier: Apache-2.0
//! Owned incarnation selection and replacement handoff; never rename a live DB.
use super::acquire_file;
use crate::{
    FileStore,
    current::{CurrentToken, gate::Gate},
    quota::Quota,
};
use airk_core::{
    graph_storage::GraphRecordReader,
    identity::{Incarnation, Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_METADATA: usize = 16384;
const MAX_RETIRED: usize = 8;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    version: u32,
    root: String,
    data_root: String,
    collection: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Retired {
    slot: Incarnation,
    incarnation: Option<Incarnation>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    owner: Sha256Digest,
    epoch: u64,
    active: Incarnation,
    retired: Vec<Retired>,
    last_rebuild: Option<RebuildReceipt>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u32,
    owner: Sha256Digest,
    expected: Sha256Digest,
    slot: Incarnation,
    job: Sha256Digest,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Initial {
    version: u32,
    owner: Sha256Digest,
    slot: Incarnation,
}

#[derive(Clone)]
pub struct Collection {
    directory: PathBuf,
    owner: Owner,
    binding: Sha256Digest,
}
pub struct Selection {
    pub store: FileStore,
    _gate: Gate,
}
pub struct Replacement {
    collection: Collection,
    intent: Intent,
    _lease: File,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebuildReceipt {
    pub job: Sha256Digest,
    pub token: CurrentToken,
}
#[derive(Clone, Copy)]
pub(super) enum HandoffStage {
    BeforeSwitch,
    AfterSwitch,
}
#[derive(Default, Clone, serde::Serialize)]
pub struct RetirementReport {
    pub removed_directories: u32,
    pub removed_files: u32,
    pub pending: usize,
}

fn retirement_inventory(directory: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let mut pending = vec![(directory.to_path_buf(), 0usize)];
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut visited = 0usize;
    while let Some((path, depth)) = pending.pop() {
        ensure!(depth <= 4, "retirement directory depth exceeded");
        ensure!(plain(&path)?.is_dir(), "invalid retirement directory");
        directories.push(path.clone());
        for entry in fs::read_dir(&path)? {
            visited += 1;
            ensure!(visited <= 1_000_000, "retirement inventory budget exceeded");
            let path = entry?.path();
            let metadata = plain(&path)?;
            ensure!(
                path.canonicalize()?.starts_with(directory.canonicalize()?),
                "retirement escaped owned incarnation"
            );
            let relative = path.strip_prefix(directory)?;
            let parts = relative
                .iter()
                .map(|part| part.to_str().context("non-UTF-8 retirement entry"))
                .collect::<Result<Vec<_>>>()?;
            let digest_name = |name: &str| {
                name.len() == 64
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            };
            let known = match parts.as_slice() {
                [name] if metadata.is_file() => matches!(
                    *name,
                    "writer.lock"
                        | "current.sqlite3"
                        | "current.sqlite3-wal"
                        | "current.sqlite3-shm"
                        | "current-owner.json"
                        | "current-bootstrap.json"
                        | "current-runtime-bootstrap.json"
                        | "roots.json"
                        | "extraction-policy.json"
                        | "vector-policy.json"
                        | "compression-policy.json"
                        | "scan-limits.json"
                        | "usage-policy.json"
                        | "coverage.json"
                ),
                [name] if metadata.is_dir() => {
                    matches!(*name, "objects" | "current-metadata" | "current-resources")
                }
                ["objects", kind] if metadata.is_dir() => matches!(
                    *kind,
                    "artifacts" | "sources" | "models" | "embedding-packs"
                ),
                ["objects", _, name] if metadata.is_file() => digest_name(name),
                ["current-metadata" | "current-resources", name] if metadata.is_file() => {
                    name.strip_suffix(".json").is_some_and(digest_name)
                }
                _ => false,
            };
            ensure!(
                known,
                "unmanaged retirement entry; preserve it for explicit review"
            );
            if metadata.is_dir() {
                pending.push((path, depth + 1));
            } else {
                files.push(path);
            }
        }
    }
    Ok((files, directories))
}

fn plain(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file() {
        crate::managed_collection::reject_file_aliases(path, &metadata)?;
    }
    ensure!(
        !metadata.file_type().is_symlink(),
        "linked service collection state"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            !metadata.is_file() || metadata.nlink() == 1,
            "hard-linked service collection metadata"
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected service collection state"
        );
    }
    Ok(metadata)
}
fn json<T: serde::de::DeserializeOwned + Serialize>(path: &Path) -> Result<T> {
    let metadata = plain(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_METADATA as u64,
        "invalid service collection metadata"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_METADATA as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_METADATA,
        "service collection metadata too large"
    );
    let value: T = serde_json::from_slice(&bytes)?;
    ensure!(
        serde_json::to_vec(&value)? == bytes,
        "noncanonical service collection metadata"
    );
    Ok(value)
}
fn digest<T: Serialize>(value: &T) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"service-collection-metadata/1.4",
        &[&serde_json::to_vec(value)?],
    ))
}
struct PreparedMetadata {
    temporary: tempfile::NamedTempFile,
    _reservation: crate::quota::Reservation,
}
fn prepare<T: Serialize>(collection: &Collection, value: &T) -> Result<PreparedMetadata> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() <= MAX_METADATA,
        "service collection metadata too large"
    );
    let quota = Quota::open(Path::new(&collection.owner.data_root))?;
    let reservation = quota.reserve(&collection.owner.collection, MAX_METADATA as u64 * 2)?;
    reservation.charge(bytes.len() as u64)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".service-metadata-")
        .tempfile_in(&collection.directory)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    Ok(PreparedMetadata {
        temporary,
        _reservation: reservation,
    })
}
fn write<T: Serialize>(collection: &Collection, name: &str, value: &T) -> Result<()> {
    let prepared = prepare(collection, value)?;
    crate::durability::persist_synced(prepared.temporary, &collection.directory.join(name))
}

impl Collection {
    fn sweep_metadata_scratch(&self) -> Result<()> {
        let mut removed = 0usize;
        for (count, entry) in fs::read_dir(&self.directory)?.enumerate() {
            ensure!(count < 4096, "service metadata inventory bound exceeded");
            let entry = entry?;
            if !entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(".service-metadata-") && name.len() <= 80)
                || !entry.file_type()?.is_file()
                || removed >= 64
            {
                continue;
            }
            let path = entry.path();
            let owned = json::<Manifest>(&path)
                .is_ok_and(|value| value.version == 140 && value.owner == self.binding)
                || json::<Intent>(&path)
                    .is_ok_and(|value| value.version == 140 && value.owner == self.binding)
                || json::<Initial>(&path)
                    .is_ok_and(|value| value.version == 140 && value.owner == self.binding)
                || json::<Owner>(&path)
                    .is_ok_and(|value| digest(&value).is_ok_and(|binding| binding == self.binding));
            if owned {
                fs::remove_file(path)?;
                removed += 1;
            }
        }
        Ok(())
    }
    /// Never hold the selection lock while walking/removing old derived files.
    /// A live old request defers retirement, including after its timer expires.
    pub fn retire(&self) -> Result<RetirementReport> {
        let _lease = acquire_file(&self.directory, "replacement.lock")?;
        let snapshot = {
            let _lock = acquire_file(&self.directory, "manifest.lock")?;
            self.manifest()?
        };
        let mut report = RetirementReport {
            pending: snapshot.retired.len(),
            ..Default::default()
        };
        for retired in snapshot.retired.iter().take(1) {
            let directory = self.slot(retired.slot);
            // Recreate only a receipt-bound empty slot when a crash separated
            // deletion from allowance retirement; never initialize a database.
            let store = FileStore::new(&directory);
            let writer = store.lock()?;
            if directory.try_exists()? {
                if directory.join("current-owner.json").try_exists()? {
                    let scope = store
                        .current_scope(&self.owner.root, &self.owner.collection)?
                        .context("retirement ownership missing")?;
                    ensure!(
                        Some(scope.incarnation) == retired.incarnation,
                        "retirement ownership differs from receipt"
                    );
                }
                let _gate = match Gate::exclusive(&store, &writer) {
                    Ok(gate) => gate,
                    Err(error)
                        if error
                            .downcast_ref::<crate::current::CurrentGateContention>()
                            .is_some() =>
                    {
                        return Ok(report);
                    }
                    Err(error) => return Err(error),
                };
                let (files, mut directories) = retirement_inventory(&directory)?;
                for file in files.iter().take(4096) {
                    fs::remove_file(file)?;
                    report.removed_files += 1;
                }
                if files.len() > 4096 {
                    return Ok(report);
                }
                directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
                for directory in directories {
                    fs::remove_dir(directory)?;
                }
                let quota = Quota::open(Path::new(&self.owner.data_root))?;
                for grant in quota.current_grants(&writer)? {
                    if Some(grant.scope().incarnation) == retired.incarnation {
                        grant.release(&writer)?;
                    }
                }
            }
            // The receipt remains until removal is proven, so interrupted cleanup
            // resumes without adopting an arbitrary surviving directory.
            let _lock = acquire_file(&self.directory, "manifest.lock")?;
            let mut manifest = self.manifest()?;
            ensure!(manifest.active != retired.slot, "active retirement target");
            manifest.retired.retain(|entry| entry.slot != retired.slot);
            write(self, "active.json", &manifest)?;
            report.removed_directories += 1;
            report.pending = manifest.retired.len();
        }
        Ok(report)
    }
    /// Explicit worker activation creates only owned metadata and an empty slot.
    pub fn initialize(
        data_root: &Path,
        root: &str,
        collection: &str,
        initial: Incarnation,
    ) -> Result<Self> {
        crate::validate_collection(collection)?;
        let data_root = data_root.canonicalize()?;
        let directory = data_root.join("collections-1.4").join(collection);
        crate::managed_collection::validate_ancestors(&directory)?;
        fs::create_dir_all(&directory)?;
        for path in [
            &data_root,
            &directory
                .parent()
                .context("namespace missing")?
                .to_path_buf(),
            &directory,
        ] {
            ensure!(
                plain(path)?.is_dir(),
                "invalid service collection directory"
            );
        }
        let owner = Owner {
            version: 140,
            root: root.into(),
            data_root: data_root.to_str().context("non-UTF-8 data root")?.into(),
            collection: collection.into(),
        };
        let selected = Self {
            binding: digest(&owner)?,
            directory,
            owner,
        };
        let _lock = acquire_file(&selected.directory, "manifest.lock")?;
        let owner_path = selected.directory.join("service-owner.json");
        if owner_path.try_exists()? {
            let existing: Owner = json(&owner_path)?;
            ensure!(
                digest(&existing)? == selected.binding,
                "foreign service collection owner"
            );
        } else {
            for entry in fs::read_dir(&selected.directory)? {
                let entry = entry?;
                ensure!(
                    entry.file_name() == "manifest.lock",
                    "unowned service collection state; preserve it and select fresh service storage"
                );
            }
            write(&selected, "service-owner.json", &selected.owner)?;
        }
        let incarnations = selected.directory.join("incarnations");
        fs::create_dir_all(&incarnations)?;
        ensure!(plain(&incarnations)?.is_dir(), "invalid incarnation root");
        if !selected.directory.join("active.json").try_exists()? {
            let initial_path = selected.directory.join("initial-slot.json");
            let initial = if initial_path.try_exists()? {
                let receipt: Initial = json(&initial_path)?;
                ensure!(
                    receipt.version == 140 && receipt.owner == selected.binding,
                    "foreign initial slot receipt"
                );
                receipt.slot
            } else {
                write(
                    &selected,
                    "initial-slot.json",
                    &Initial {
                        version: 140,
                        owner: selected.binding,
                        slot: initial,
                    },
                )?;
                initial
            };
            let slot = selected.slot(initial);
            if !slot.try_exists()? {
                fs::create_dir(&slot)?;
            }
            ensure!(plain(&slot)?.is_dir(), "invalid initial incarnation");
            write(
                &selected,
                "active.json",
                &Manifest {
                    version: 140,
                    owner: selected.binding,
                    epoch: 1,
                    active: initial,
                    retired: vec![],
                    last_rebuild: None,
                },
            )?;
        }
        selected.manifest()?;
        if selected.directory.join("initial-slot.json").try_exists()? {
            let receipt: Initial = json(&selected.directory.join("initial-slot.json"))?;
            ensure!(
                receipt.owner == selected.binding && receipt.version == 140,
                "foreign initial slot receipt"
            );
            fs::remove_file(selected.directory.join("initial-slot.json"))?;
        }
        Ok(selected)
    }
    pub fn existing(data_root: &Path, root: &str, collection: &str) -> Result<Self> {
        crate::validate_collection(collection)?;
        let data_root = data_root.canonicalize()?;
        let directory = data_root.join("collections-1.4").join(collection);
        crate::managed_collection::validate_ancestors(&directory)?;
        ensure!(plain(&directory)?.is_dir(), "service collection missing");
        let owner: Owner = json(&directory.join("service-owner.json"))?;
        ensure!(
            owner.version == 140
                && owner.root == root
                && owner.collection == collection
                && Path::new(&owner.data_root) == data_root,
            "foreign service collection binding"
        );
        Ok(Self {
            binding: digest(&owner)?,
            owner,
            directory,
        })
    }
    fn slot(&self, slot: Incarnation) -> PathBuf {
        self.directory.join("incarnations").join(slot.to_string())
    }
    fn manifest(&self) -> Result<Manifest> {
        let manifest: Manifest = json(&self.directory.join("active.json"))?;
        ensure!(
            manifest.version == 140
                && manifest.owner == self.binding
                && manifest.epoch > 0
                && manifest.retired.len() <= MAX_RETIRED,
            "invalid service selection manifest"
        );
        ensure!(
            manifest
                .retired
                .iter()
                .all(|retired| retired.slot != manifest.active),
            "active incarnation listed for retirement"
        );
        ensure!(
            plain(&self.slot(manifest.active))?.is_dir(),
            "selected service incarnation missing"
        );
        Ok(manifest)
    }
    /// The OS object lease is acquired while manifest replacement is excluded.
    /// Keep it through the whole request, not in an idle kernel/session cache.
    pub fn select(&self) -> Result<Selection> {
        let _lock = acquire_file(&self.directory, "manifest.lock")?;
        let manifest = self.manifest()?;
        let store = FileStore::new(self.slot(manifest.active));
        let gate = Gate::shared(&store)?;
        Ok(Selection { store, _gate: gate })
    }
    pub fn selected_store(&self) -> Result<FileStore> {
        Ok(self.select()?.store)
    }
    pub fn completed_rebuild(&self, job: Sha256Digest) -> Result<bool> {
        let _lock = acquire_file(&self.directory, "manifest.lock")?;
        Ok(self
            .manifest()?
            .last_rebuild
            .is_some_and(|receipt| receipt.job == job))
    }
    pub fn last_rebuild_receipt(&self) -> Result<Option<RebuildReceipt>> {
        let _lock = acquire_file(&self.directory, "manifest.lock")?;
        Ok(self.manifest()?.last_rebuild)
    }
    pub fn begin_replacement(&self, slot: Incarnation, job: Sha256Digest) -> Result<Replacement> {
        let lease = acquire_file(&self.directory, "replacement.lock")?;
        let _lock = acquire_file(&self.directory, "manifest.lock")?;
        ensure!(
            !self.directory.join("replacement.json").try_exists()?,
            "interrupted replacement requires owned recovery"
        );
        let manifest = self.manifest()?;
        ensure!(
            manifest.retired.len() < MAX_RETIRED,
            "retired incarnation capacity; finish reader drain and cleanup before rebuilding"
        );
        let intent = Intent {
            version: 140,
            owner: self.binding,
            expected: digest(&manifest)?,
            slot,
            job,
        };
        crate::managed_collection::validate_ancestors(&self.slot(slot))?;
        ensure!(
            !self.slot(slot).try_exists()?,
            "replacement directory already exists"
        );
        write(self, "replacement.json", &intent)?;
        fs::create_dir(self.slot(slot))?;
        crate::maintenance::sync_parent(&self.directory.join("incarnations"))?;
        Ok(Replacement {
            collection: self.clone(),
            intent,
            _lease: lease,
        })
    }
    /// An available replacement lock proves the old build owner no longer runs.
    /// Selection remains usable even when classification/cleanup needs a retry.
    pub fn recover_abandoned_replacement(&self) -> Result<()> {
        let _lease = acquire_file(&self.directory, "replacement.lock")?;
        let _lock = acquire_file(&self.directory, "manifest.lock")?;
        self.sweep_metadata_scratch()?;
        let path = self.directory.join("replacement.json");
        if !path.try_exists()? {
            return Ok(());
        }
        let intent: Intent = json(&path)?;
        ensure!(
            intent.version == 140 && intent.owner == self.binding,
            "foreign replacement intent"
        );
        let mut manifest = self.manifest()?;
        if manifest.active != intent.slot && self.slot(intent.slot).try_exists()? {
            ensure!(
                manifest.retired.len() < MAX_RETIRED,
                "retired incarnation capacity"
            );
            // Intent was durable before the unique directory was created, and
            // service scope incarnation equals its slot. Classification never
            // opens a half-built database or blocks the verified active index.
            let incarnation = Some(intent.slot);
            if !manifest.retired.iter().any(|old| old.slot == intent.slot) {
                manifest.retired.push(Retired {
                    slot: intent.slot,
                    incarnation,
                });
                write(self, "active.json", &manifest)?;
            }
        }
        fs::remove_file(path)?;
        crate::maintenance::sync_parent(&self.directory)
    }
}

impl Replacement {
    pub fn store(&self) -> FileStore {
        FileStore::new(self.collection.slot(self.intent.slot))
    }
    /// Kernel validation must also verify retained inputs and applied/live source
    /// configuration before this coherent, revision-bound handoff.
    pub fn commit(
        &mut self,
        expected_head: Option<CurrentToken>,
        expected_settings: Sha256Digest,
    ) -> Result<CurrentToken> {
        self.commit_observed(expected_head, expected_settings, &mut |_| Ok(()))
    }
    pub(super) fn commit_observed(
        &mut self,
        expected_head: Option<CurrentToken>,
        expected_settings: Sha256Digest,
        observer: &mut dyn FnMut(HandoffStage) -> Result<()>,
    ) -> Result<CurrentToken> {
        let store = self.store();
        let _writer = store.lock()?;
        let mut manifest = self.collection.manifest()?;
        ensure!(
            digest(&manifest)? == self.intent.expected,
            "replacement selection plan expired"
        );
        let old = FileStore::new(self.collection.slot(manifest.active));
        let old_scope = old.current_scope(
            &self.collection.owner.root,
            &self.collection.owner.collection,
        )?;
        if let Some(expected) = expected_head {
            let scope = old_scope.as_ref().context("replacement base disappeared")?;
            ensure!(
                old.current_read(scope)?.token()? == expected,
                "replacement publication base changed"
            );
        }
        ensure!(
            settings_id(&settings(&old, &self.collection.owner.root)?) == expected_settings
                && settings_id(&settings(&store, &self.collection.owner.root)?)
                    == expected_settings,
            "replacement configuration changed before handoff"
        );
        let scope = store
            .current_scope(
                &self.collection.owner.root,
                &self.collection.owner.collection,
            )?
            .context("replacement has no owned head")?;
        ensure!(
            scope.incarnation == self.intent.slot,
            "replacement scope differs from owned incarnation directory"
        );
        ensure!(
            old_scope
                .as_ref()
                .is_none_or(|old| old.incarnation != scope.incarnation),
            "replacement must have a fresh incarnation"
        );
        let read = store.current_read(&scope)?;
        ensure!(
            !read.has_prepared_update()?,
            "replacement publication is incomplete"
        );
        read.verify_lexical_integrity()?;
        let counts = read.counts()?;
        ensure!(
            counts.allocated_count <= 1_000_000,
            "replacement graph validation count exceeded"
        );
        let mut bytes = 0u64;
        for id in 0..counts.allocated_count {
            bytes = bytes
                .checked_add(read.read(u32::try_from(id)?)?.bytes.len() as u64)
                .context("replacement graph validation overflow")?;
            ensure!(
                bytes <= 1 << 30,
                "replacement graph validation byte budget exceeded"
            );
        }
        read.map_entries(crate::current::map::MapVisitLimits::default())?;
        let token = read.token()?;
        manifest.retired.push(Retired {
            slot: manifest.active,
            incarnation: Some(old_scope.map_or(manifest.active, |scope| scope.incarnation)),
        });
        manifest.active = self.intent.slot;
        manifest.epoch = manifest
            .epoch
            .checked_add(1)
            .context("manifest epoch exhausted")?;
        manifest.last_rebuild = Some(RebuildReceipt {
            job: self.intent.job,
            token,
        });
        // Complete authentication, quota inventory, serialization and sync before
        // excluding new request selection. Only the pointer replacement is gated.
        let prepared = prepare(&self.collection, &manifest)?;
        observer(HandoffStage::BeforeSwitch)?;
        let _lock = acquire_file(&self.collection.directory, "manifest.lock")?;
        ensure!(
            digest(&self.collection.manifest()?)? == self.intent.expected,
            "replacement selection changed before handoff"
        );
        crate::durability::persist_synced(
            prepared.temporary,
            &self.collection.directory.join("active.json"),
        )?;
        observer(HandoffStage::AfterSwitch)?;
        fs::remove_file(self.collection.directory.join("replacement.json"))?;
        crate::maintenance::sync_parent(&self.collection.directory)?;
        Ok(token)
    }
}

#[cfg(test)]
#[path = "service_collection_tests.rs"]
mod tests;

/// Helper capabilities admit only the registered project's explicit store layout.
pub fn validate_location(path: &Path, data_root: &Path, collection: &str) -> Result<()> {
    let store = FileStore::new(path.canonicalize()?);
    let managed = store.managed_path()?;
    ensure!(
        managed.namespace == "collections-1.4"
            && managed.collection == collection
            && managed.root == data_root.canonicalize()?,
        "foreign service collection location"
    );
    Ok(())
}
pub(crate) fn validate_usage_directory(directory: &Path) -> Result<()> {
    let store = FileStore::new(directory);
    let managed = store.managed_path()?;
    ensure!(
        managed.namespace == "collections-1.4" && managed.slot.is_none(),
        "invalid service usage owner"
    );
    let owner: Owner = json(&directory.join("service-owner.json"))?;
    Collection::existing(managed.root, &owner.root, managed.collection)?.manifest()?;
    Ok(())
}

const SETTINGS: [&str; 5] = [
    "roots.json",
    "extraction-policy.json",
    "vector-policy.json",
    "compression-policy.json",
    "scan-limits.json",
];
fn settings(source: &FileStore, root: &str) -> Result<Vec<(&'static str, Vec<u8>)>> {
    let mut result = Vec::new();
    let mut total = 0usize;
    for name in SETTINGS {
        let path = source.directory().join(name);
        if !path.try_exists()? {
            continue;
        }
        let metadata = plain(&path)?;
        ensure!(
            metadata.is_file() && metadata.len() <= 262144,
            "invalid saved service configuration"
        );
        let mut bytes = Vec::new();
        File::open(path)?.take(262145).read_to_end(&mut bytes)?;
        total = total
            .checked_add(bytes.len())
            .context("saved configuration size overflow")?;
        ensure!(
            bytes.len() <= 262144 && total <= 1 << 20,
            "saved configuration budget exceeded"
        );
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        if name == "usage-policy.json" {
            crate::usage::policy(source.directory())?;
        } else {
            ensure!(
                value
                    .get("owner_root")
                    .or_else(|| value.get("root"))
                    .and_then(serde_json::Value::as_str)
                    == Some(root),
                "foreign saved service configuration"
            );
        }
        if let Some(collection) = value.get("collection") {
            ensure!(
                collection.as_str() == Some(source.collection_id()?),
                "foreign configuration collection"
            );
        }
        result.push((name, bytes));
    }
    Ok(result)
}
fn settings_id(settings: &[(&str, Vec<u8>)]) -> Sha256Digest {
    let mut parts = Vec::new();
    for (name, bytes) in settings {
        parts.push(name.as_bytes());
        parts.push(bytes.as_slice());
    }
    framed_digest(b"service-saved-configuration/1.4", &parts)
}
pub fn settings_digest(source: &FileStore, root: &str) -> Result<Sha256Digest> {
    let _writer = source.lock()?;
    Ok(settings_id(&settings(source, root)?))
}
pub fn copy_settings(
    source: &FileStore,
    target: &FileStore,
    root: &str,
    quota: &Quota,
) -> Result<Sha256Digest> {
    let _writer = target.lock()?;
    ensure!(
        source.managed_path()?.logical.canonicalize()?
            == target.managed_path()?.logical.canonicalize()?
            && source.directory() != target.directory(),
        "replacement configuration owner mismatch"
    );
    let settings = settings(source, root)?;
    let reservation = quota.reserve(source.collection_id()?, 1 << 20)?;
    for (name, bytes) in &settings {
        let mut destination = reservation.create_file(target.directory().join(name))?;
        destination.write_all(bytes)?;
        destination.sync_all()?;
    }
    Ok(settings_id(&settings))
}
