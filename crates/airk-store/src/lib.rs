// SPDX-License-Identifier: Apache-2.0
//! Derived state only. Source workspaces remain authoritative.
use airk_core::{Snapshot, SnapshotStore};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
pub mod blobs;
pub mod capacity;
pub mod checkpoints;
pub mod compression;
pub mod current;
mod durability;
pub mod extraction_policy;
pub mod history;
pub mod history_policy;
mod ledger;
pub mod maintenance;
mod managed_collection;
pub mod model_objects;
pub mod quota;
pub mod relocation;
pub mod reset;
pub use reset::IndexFormat;
pub mod retention;
pub mod roots;
pub mod scheduler;
pub mod service;
pub mod usage;
pub mod vector_cache;
pub mod vector_policy;
pub mod vector_registry;

#[derive(Clone, Debug)]
pub struct FileStore {
    directory: PathBuf,
    shared_model_runtime: Option<PathBuf>,
    shared_model_default: Option<airk_core::model_objects::ModelObjectSet>,
    current_engine: current::kv::owner::Cache,
}

pub enum UsageSourceSizes {
    Available(Vec<airk_core::usage::SourceContextSize>),
    /// The retained generation predates compact source measurements.
    Legacy,
    Unavailable(airk_core::usage::BaselineUnavailable),
}

/// Logical reads of authenticated bytes, not physical device I/O. Lookup time
/// includes binding/lease work and excludes the separately timed verifier.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UsageReadMetrics {
    pub verification_bytes: u64,
    pub verification_us: u64,
    pub lookup_us: u64,
}

impl FileStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            shared_model_runtime: None,
            shared_model_default: None,
            current_engine: Default::default(),
        }
    }
    pub fn with_service_models(
        mut self,
        runtime: &Path,
        default: airk_core::model_objects::ModelObjectSet,
    ) -> Result<Self> {
        anyhow::ensure!(
            self.collection_namespace()? == "collections-1.4" && default.shared.is_none(),
            "shared models require service collection ownership"
        );
        default.validate()?;
        self.shared_model_runtime = Some(runtime.canonicalize()?);
        self.shared_model_default = Some(default);
        Ok(self)
    }
    pub fn shared_model_runtime(&self) -> Option<&Path> {
        self.shared_model_runtime.as_deref()
    }
    pub fn shared_model_default(&self) -> Option<&airk_core::model_objects::ModelObjectSet> {
        self.shared_model_default.as_ref()
    }
    pub fn installed_model(
        &self,
        set: &airk_core::model_objects::ModelObjectSet,
    ) -> Result<service::model_files::InstalledModel> {
        service::model_files::InstalledModel::open(
            self.shared_model_runtime()
                .context("shared model runtime is not bound")?,
            set,
        )
    }
    pub fn validate_model_reference(
        &self,
        set: &airk_core::model_objects::ModelObjectSet,
        maximum_local_bytes: u64,
    ) -> Result<()> {
        if set.shared.is_some() {
            use airk_core::model_objects::ModelAssetSource;
            // Publication captures installed identity, not live model availability.
            // Inference and explicit verification hash their actual asset bytes.
            self.installed_model(set)?
                .read_asset(airk_core::model_objects::ModelAssetName::ModelJson, 16384)?;
        } else {
            model_objects::ModelObjects::new(self.directory()).verify(set, maximum_local_bytes)?;
        }
        Ok(())
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    /// Exact owned layout, shared by gates and quota adapters. A sibling is not
    /// admitted merely because its collection name happens to match.
    pub(crate) fn collection_namespace(&self) -> Result<&str> {
        Ok(self.managed_path()?.namespace)
    }
    pub(crate) fn managed_path(&self) -> Result<managed_collection::ManagedPath<'_>> {
        managed_collection::ManagedPath::parse(&self.directory)
    }
    pub(crate) fn collection_id(&self) -> Result<&str> {
        Ok(self.managed_path()?.collection)
    }
    pub fn service_incarnation(&self) -> Result<Option<airk_core::identity::Incarnation>> {
        self.managed_path()?.slot.map(str::parse).transpose()
    }
    pub fn managed_collection_directory(&self) -> Result<&Path> {
        Ok(self.managed_path()?.logical)
    }
    pub(crate) fn lifecycle(&self, create: bool) -> Result<Option<Arc<File>>> {
        let Ok(managed) = self.managed_path() else {
            return Ok(None);
        };
        let root = managed.root;
        quota::ensure_current_root(root)?;
        if !root.exists() {
            if !create {
                return Ok(None);
            }
            fs::create_dir_all(root)?;
        }
        let lease = if managed.namespace == "collections-1.4" {
            quota::service_lifecycle_lease(root, false)?
        } else {
            quota::lifecycle_lease(root, false)?
        };
        quota::ensure_current_root(root)?;
        Ok(Some(Arc::new(lease)))
    }
    /// OS-backed exclusivity releases after process death. Legacy lock markers
    /// still fail closed until an operator establishes that their writer exited.
    pub fn lock(&self) -> Result<WriterLock> {
        if self.managed_path().is_ok() {
            managed_collection::validate_ancestors(&self.directory)?;
        }
        let lifecycle = self.lifecycle(true)?;
        let physical_parent = self
            .directory
            .parent()
            .context("index directory has no parent")?;
        fs::create_dir_all(physical_parent)?;
        let logical = self
            .managed_path()
            .ok()
            .map_or(self.directory.as_path(), |path| path.logical);
        let parent = logical
            .parent()
            .context("logical collection parent missing")?;
        let name = logical.file_name().context("index directory has no name")?;
        fs::create_dir_all(parent)?;
        ensure!(
            !fs::symlink_metadata(parent)?.file_type().is_symlink(),
            "refusing linked collection parent"
        );
        let locks = parent.join(".writer-locks");
        fs::create_dir_all(&locks)?;
        ensure!(
            !fs::symlink_metadata(&locks)?.file_type().is_symlink(),
            "refusing linked writer lock directory"
        );
        if locks.join(name).exists() {
            ensure!(
                !fs::symlink_metadata(locks.join(name))?
                    .file_type()
                    .is_symlink(),
                "refusing linked writer lock"
            );
        }
        let advisory = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(locks.join(name))?;
        advisory.try_lock().context("index writer locked")?;
        let bound_slot = match self.managed_path() {
            Ok(managed) if managed.namespace == "collections-1.4" && managed.slot.is_some() => {
                service::collection::has_container_metadata(managed.logical)?
            }
            _ => false,
        };
        // Bound slots are created only by authenticated container setup/builds.
        // A stale handle after retirement must not recreate its old directory.
        if !bound_slot {
            fs::create_dir_all(&self.directory)?;
        }
        ensure!(
            !fs::symlink_metadata(&self.directory)?
                .file_type()
                .is_symlink(),
            "refusing linked collection directory"
        );
        // tempfile persistence uses native Win32 paths; give both operands the
        // canonical extended prefix even for direct FileStore callers.
        let directory = self.directory.canonicalize()?;
        let path = directory.join("writer.lock");
        if path.exists() {
            ensure!(
                !fs::symlink_metadata(&path)?.file_type().is_symlink(),
                "refusing linked writer marker"
            );
            let mut marker = String::new();
            File::open(&path)?.take(128).read_to_string(&mut marker)?;
            ensure!(
                marker.starts_with("airk-advisory-v1\n"),
                "legacy index writer locked; see docs/operations.md for recovery"
            );
            fs::remove_file(&path)?;
        }
        let mut file = tempfile::NamedTempFile::new_in(&directory)?;
        writeln!(file, "airk-advisory-v1\npid={}", std::process::id())?;
        file.as_file().sync_all()?;
        // Older binaries only use create_new markers; never replace one that
        // appeared after our check while preparing the new marker.
        file.persist_noclobber(&path)
            .context("write collection writer marker; another writer may be active")?;
        let guard = WriterLock {
            path,
            _file: Arc::new(advisory),
            _lifecycle: lifecycle,
            object_captures: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        };
        Ok(guard)
    }
}

#[derive(Clone)]
pub struct WriterLock {
    path: PathBuf,
    _file: Arc<File>,
    _lifecycle: Option<Arc<File>>,
    object_captures: Arc<std::sync::atomic::AtomicUsize>,
}

/// Keeps unpublished captured objects and prior scan reuse inputs alive across
/// intermediate publications. The collection writer excludes other processes;
/// after a crash, only durable current/journal ownership needs to survive.
#[must_use]
pub struct ObjectCaptureGuard<'a> {
    writer: &'a WriterLock,
}

impl WriterLock {
    pub fn capture_objects(&self) -> ObjectCaptureGuard<'_> {
        self.object_captures
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        ObjectCaptureGuard { writer: self }
    }

    pub fn has_object_captures(&self) -> bool {
        self.object_captures
            .load(std::sync::atomic::Ordering::Acquire)
            != 0
    }
}

impl Drop for ObjectCaptureGuard<'_> {
    fn drop(&mut self) {
        self.writer
            .object_captures
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[derive(Debug, Serialize)]
pub struct CollectionUsage {
    pub collection: String,
    pub disk_bytes: u64,
    pub files: usize,
    pub generation: Option<u64>,
    pub indexed_files: Option<usize>,
    pub status: String,
}

pub fn validate_collection(collection: &str) -> Result<()> {
    ensure!(
        !collection.is_empty()
            && collection.len() <= 64
            && collection
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_'),
        "collection must be 1..64 lowercase ASCII letters/digits/hyphens/underscores"
    );
    ensure!(
        ![
            "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
            "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"
        ]
        .contains(&collection),
        "collection is a reserved filesystem name"
    );
    Ok(())
}

pub fn list_collections(data_root: &Path) -> Result<Vec<CollectionUsage>> {
    let collections = data_root.join("collections-1.2");
    if !collections.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(&collections)? {
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if validate_collection(&id).is_err() || !entry.file_type()?.is_dir() {
            continue;
        }
        let (disk_bytes, files) = directory_usage(&entry.path())?;
        let store = FileStore::new(entry.path());
        let observed = (|| -> Result<_> {
            let Some(scope) = store.current_owned_scope()? else {
                return Ok((None, None, "unindexed"));
            };
            let read = store.current_read(&scope)?;
            let state = read
                .collection_state()?
                .context("current collection metadata missing")?;
            Ok((
                Some(read.head().sequence),
                Some(usize::try_from(state.counts.files)?),
                "indexed",
            ))
        })();
        let (generation, indexed_files, status) =
            observed.unwrap_or((None, None, "unavailable-current-index"));
        result.push(CollectionUsage {
            collection: id,
            disk_bytes,
            files,
            generation,
            indexed_files,
            status: status.into(),
        });
    }
    result.sort_by(|a, b| a.collection.cmp(&b.collection));
    Ok(result)
}

fn directory_usage(root: &Path) -> Result<(u64, usize)> {
    let mut bytes = 0u64;
    let mut files = 0;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "collection contains a link; refusing maintenance"
            );
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                bytes = bytes
                    .checked_add(metadata.len())
                    .context("disk usage overflow")?;
                files += 1;
            }
        }
    }
    Ok((bytes, files))
}

pub fn remove_collection(data_root: &Path, id: &str) -> Result<CollectionUsage> {
    validate_collection(id)?;
    let scheduler = scheduler::Queue::new(data_root);
    let worker = scheduler
        .worker_lock(id)?
        .context("this collection is already indexing; wait before removing it")?;
    let base = data_root
        .join("collections-1.2")
        .canonicalize()
        .context("no collection storage exists")?;
    let path = base.join(id);
    ensure!(
        !fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "refusing linked collection directory"
    );
    let resolved = path.canonicalize()?;
    ensure!(
        resolved.parent() == Some(base.as_path()),
        "collection escaped data root"
    );
    let store = FileStore::new(&resolved);
    let lock = store.lock()?;
    let usage_deletion = usage::deletion_guard(&resolved)?;
    let scope = store
        .current_owned_scope()?
        .context("no current collection owner; refusing deletion")?;
    let read = store.current_read(&scope)?;
    let generation = read.head().sequence;
    let indexed_files = usize::try_from(
        read.collection_state()?
            .context("current collection metadata missing")?
            .counts
            .files,
    )?;
    let stored_root = scope.root;
    ensure!(
        scope.collection == id,
        "collection identity mismatch; refusing deletion"
    );
    drop(read);
    let _current_gate = store.current_deletion_gate(&lock)?;
    current::release_lexical_cache();
    let (disk_bytes, files) = directory_usage(&resolved)?;
    scheduler.forget_locked(&worker, id, &stored_root)?;
    // The lock remains held while snapshot and derived artifacts are removed.
    for entry in fs::read_dir(&resolved)? {
        let entry = entry?;
        if entry.file_name() == "writer.lock" || entry.file_name() == "usage.lock" {
            continue;
        }
        let target = entry.path().canonicalize()?;
        ensure!(
            target.starts_with(&resolved),
            "maintenance target escaped collection"
        );
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(&target)?;
        } else {
            fs::remove_file(&target)?;
        }
    }
    drop(usage_deletion);
    if resolved.join("usage.lock").exists() {
        fs::remove_file(resolved.join("usage.lock"))?;
    }
    fs::remove_file(&lock.path)?;
    fs::remove_dir(&resolved)?;
    drop(lock);
    Ok(CollectionUsage {
        collection: id.into(),
        disk_bytes,
        files,
        generation: Some(generation),
        indexed_files: Some(indexed_files),
        status: "removed-derived-index".into(),
    })
}
impl Drop for WriterLock {
    fn drop(&mut self) {
        if Arc::strong_count(&self._file) != 1 {
            return;
        }
        let _ = fs::remove_file(&self.path);
        // A concurrent fork can temporarily inherit this open file description
        // until exec. Release logical ownership explicitly rather than letting
        // the inherited descriptor extend it beyond this guard's lifetime.
        let _ = self._file.unlock();
    }
}

#[cfg(all(test, unix))]
mod writer_lock_tests {
    use super::*;

    #[test]
    fn writer_release_is_not_extended_by_an_inherited_open_file_description() -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = FileStore::new(root.path().join("collection"));
        let writer = store.lock()?;
        // dup shares the same open file description as a descriptor inherited
        // through fork, without introducing a fork into the threaded test host.
        let inherited = writer._file.try_clone()?;
        assert!(store.lock().is_err());
        drop(writer);
        let replacement = store.lock()?;
        drop(inherited);
        assert!(store.lock().is_err());
        drop(replacement);
        drop(store.lock()?);
        Ok(())
    }
}

#[derive(Default)]
pub struct MemoryStore(Mutex<Option<Snapshot>>);
impl SnapshotStore for MemoryStore {
    fn load(&self) -> Result<Option<Snapshot>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("memory store poisoned"))?
            .clone())
    }
    fn save(&self, snapshot: &Snapshot) -> Result<()> {
        *self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("memory store poisoned"))? = Some(snapshot.clone());
        Ok(())
    }
}
