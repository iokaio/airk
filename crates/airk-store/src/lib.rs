// SPDX-License-Identifier: Apache-2.0
//! Derived state only. Source workspaces remain authoritative.
use airk_core::identity::{PublicationReceipt, RevisionToken, Sha256Digest};
use airk_core::{SCHEMA_VERSION, Snapshot, SnapshotStore};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
pub mod blobs;
pub mod capacity;
pub mod catalog_experiment;
pub mod checkpoints;
pub mod compression;
pub mod current;
mod durability;
pub mod extraction_policy;
pub mod history;
pub mod history_policy;
pub mod maintenance;
pub mod model_objects;
mod normalized;
pub mod quota;
pub mod relocation;
pub mod reset;
pub use reset::IndexFormat;
pub mod retention;
pub mod roots;
pub mod scheduler;
mod sqlite;
pub mod usage;
pub mod vector_cache;
pub mod vector_policy;
pub mod vector_registry;
mod verified_cache;

#[derive(Clone, Debug)]
pub struct FileStore {
    directory: PathBuf,
}

/// Small metadata pin for disk retrieval, committed with the corresponding revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentRevision {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<RevisionToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_sha256: Option<String>,
    #[serde(default)]
    pub storage_format: u32,
    #[serde(default)]
    pub roots: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub inventory_sha256: Option<String>,
    pub revision: String,
    pub root: String,
    pub collection: String,
    pub generation: u64,
    pub files: usize,
    pub chunks: usize,
    pub source_bytes: usize,
    pub extractor_version: String,
    pub semantic_producer: Option<String>,
    pub imported_facts: usize,
    pub imported_relationships: usize,
}

pub const ROOT_POINTER_VERSION: u32 = 5;
pub const IMMUTABLE_CATALOG_VERSION: u32 = 3;

pub enum UsageSourceSizes {
    Available(Vec<airk_core::usage::SourceContextSize>),
    /// The retained generation predates compact source measurements.
    Legacy,
    Unavailable(airk_core::usage::BaselineUnavailable),
}

/// Logical reads of authenticated bytes, not physical device I/O. Lookup time
/// includes binding/lease/SQL work and excludes the separately timed verifier.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UsageReadMetrics {
    pub verification_bytes: u64,
    pub verification_us: u64,
    pub lookup_us: u64,
}

#[derive(Clone, Debug)]
pub enum CommittedRevision {
    Legacy(PersistentRevision),
    V2 {
        header: PersistentRevision,
        receipt: PublicationReceipt,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpectedHead {
    Empty,
    /// Digest of exact legacy root payload/header bytes, never a reserialized Snapshot.
    Legacy(Sha256Digest),
    V2(RevisionToken),
}

impl FileStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn expected_head(&self) -> Result<ExpectedHead> {
        let _lifecycle = self.lifecycle(false)?;
        sqlite::expected_head(&self.directory)
    }
    pub fn committed_revision(&self) -> Result<Option<CommittedRevision>> {
        self.persistent_revision()?
            .map(|header| {
                if let Some(token) = header.token {
                    let receipt = self.receipt(&token)?.context("committed receipt missing")?;
                    Ok(CommittedRevision::V2 { header, receipt })
                } else {
                    Ok(CommittedRevision::Legacy(header))
                }
            })
            .transpose()
    }
    pub fn receipt(&self, token: &RevisionToken) -> Result<Option<PublicationReceipt>> {
        let _lifecycle = self.lifecycle(false)?;
        self.ensure_index_ready()?;
        sqlite::receipt(&self.directory, token)
    }
    /// Reject a token contradicting a retained receipt; absence is not ancestry proof.
    pub fn validate_revision_token(&self, token: &RevisionToken) -> Result<()> {
        let _lifecycle = self.lifecycle(false)?;
        let current = self
            .persistent_revision()?
            .and_then(|header| header.token)
            .context("revision tokens require a v2 collection")?;
        token.validate_against(&current)?;
        sqlite::validate_retained_token(&self.directory, token)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_catalog_v2(
        &self,
        directory: &Path,
        inventory: &airk_core::publication::InventoryV2,
        lineage: &airk_core::changes::LineageEnvelope,
        token: &RevisionToken,
        source: &dyn airk_core::ArtifactSource,
        reservation: Option<&quota::Reservation>,
        maximum: u64,
    ) -> Result<()> {
        self.prepare_catalog_v2_reusing(
            directory,
            inventory,
            lineage,
            token,
            source,
            reservation,
            maximum,
            None,
        )
    }
    /// Reuse native catalog rows only from an authenticated, still-pinned base.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_catalog_v2_reusing(
        &self,
        directory: &Path,
        inventory: &airk_core::publication::InventoryV2,
        lineage: &airk_core::changes::LineageEnvelope,
        token: &RevisionToken,
        source: &dyn airk_core::ArtifactSource,
        reservation: Option<&quota::Reservation>,
        maximum: u64,
        previous: Option<&blobs::Catalog>,
    ) -> Result<()> {
        // Unlike legacy builds, the final v2 manifest follows catalog checksums.
        // Reserve its full bound plus the lineage callback write before page admission.
        let pending = (131_072u64 + quota::PREPARED_PUBLICATION_MAX)
            .checked_add(serde_json::to_vec(lineage)?.len() as u64)
            .context("v2 metadata allowance overflow")?;
        let maximum = maximum.saturating_sub(pending) / 4096 * 4096;
        ensure!(
            maximum >= 16_384,
            "v2 catalog admission cannot preserve final manifest/lineage allowance"
        );
        sqlite::save_catalog_v2(
            directory,
            inventory,
            lineage,
            token,
            source,
            reservation,
            maximum,
            self.directory(),
            previous,
        )
    }
    pub fn publish_catalog_v2(
        &self,
        inventory: &airk_core::publication::InventoryV2,
        receipt: &PublicationReceipt,
        expected: &ExpectedHead,
        reservation: &quota::Reservation,
    ) -> Result<()> {
        sqlite::publish_catalog_v2(&self.directory, inventory, receipt, expected, reservation)
    }
    pub(crate) fn lifecycle(&self, create: bool) -> Result<Option<Arc<File>>> {
        let Some(collections) = self.directory.parent().filter(|path| {
            path.file_name()
                .is_some_and(|name| name == "collections-1.2")
        }) else {
            return Ok(None);
        };
        let root = collections
            .parent()
            .context("collection has no data root")?;
        quota::ensure_current_root(root)?;
        if !root.exists() {
            if !create {
                return Ok(None);
            }
            fs::create_dir_all(root)?;
        }
        let lease = quota::lifecycle_lease(root, false)?;
        quota::ensure_current_root(root)?;
        Ok(Some(Arc::new(lease)))
    }
    pub fn persistent_revision(&self) -> Result<Option<PersistentRevision>> {
        let _lifecycle = self.lifecycle(false)?;
        self.ensure_index_ready()?;
        if !self.directory.join("index.sqlite3").exists() {
            return Ok(None);
        }
        sqlite::persistent_revision(&self.directory)
    }
    pub fn save_with_components(&self, snapshot: &Snapshot, revision: &str) -> Result<()> {
        ensure!(
            revision.len() == 64 && revision.bytes().all(|c| c.is_ascii_hexdigit()),
            "invalid persistent revision identity"
        );
        sqlite::save(&self.directory, snapshot, Some(revision))
    }
    pub fn prepare_catalog(
        &self,
        directory: &Path,
        snapshot: &Snapshot,
        revision: &str,
        source: &dyn airk_core::ArtifactSource,
        reservation: Option<&quota::Reservation>,
        maximum: u64,
    ) -> Result<()> {
        sqlite::save_catalog(directory, snapshot, revision, source, reservation, maximum)
    }
    /// Admit the private catalog from remaining reserved bytes, leaving the
    /// active pointer's main-file, WAL and shared-memory growth uncharged until
    /// publication. Call after all other generation components have been written,
    /// while holding the collection writer lock.
    /// Observed last-generation size is a planning hint, not a bound on future input.
    pub fn active_generation_bytes(&self) -> Result<Option<u64>> {
        let Some(head) = self.persistent_revision()? else {
            return Ok(None);
        };
        let Some(token) = head.token else {
            return Ok(None);
        };
        Ok(Some(
            directory_usage(
                &self
                    .directory
                    .join("generations")
                    .join(token.revision.to_string()),
            )?
            .0,
        ))
    }
    pub fn catalog_budget(&self, reservation: &quota::Reservation) -> Result<u64> {
        let (_, pointer_bytes) = sqlite::pointer_budget(&self.directory)?;
        let available = reservation.remaining();
        let physical = capacity::probe(&self.directory)?.available_growth_bytes;
        sqlite::catalog_page_budget(available, physical, pointer_bytes)
    }
    pub fn publish_catalog(
        &self,
        snapshot: &Snapshot,
        revision: &str,
        reservation: Option<&quota::Reservation>,
    ) -> Result<()> {
        sqlite::publish_catalog(&self.directory, snapshot, revision, reservation)
    }
    /// OS-backed exclusivity releases after process death. Legacy lock markers
    /// still fail closed until an operator establishes that their writer exited.
    pub fn lock(&self) -> Result<WriterLock> {
        let lifecycle = self.lifecycle(true)?;
        let parent = self
            .directory
            .parent()
            .context("index directory has no parent")?;
        let name = self
            .directory
            .file_name()
            .context("index directory has no name")?;
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
        fs::create_dir_all(&self.directory)?;
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
            _file: advisory,
            _lifecycle: lifecycle,
            object_captures: std::sync::atomic::AtomicUsize::new(0),
        };
        Ok(guard)
    }
}

impl SnapshotStore for FileStore {
    fn load(&self) -> Result<Option<Snapshot>> {
        let _lifecycle = self.lifecycle(false)?;
        self.ensure_index_ready()?;
        if self.directory.join("index.sqlite3").exists() {
            if self
                .persistent_revision()?
                .is_some_and(|h| matches!(h.storage_format, 1 | 2))
            {
                return self.catalog()?.map(|c| c.materialize()).transpose();
            }
            return sqlite::load(&self.directory);
        }
        let path = self.directory.join("snapshot.json");
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            file.metadata()?.len() <= 536_870_912,
            "snapshot exceeds 512 MiB limit"
        );
        let snapshot: Snapshot = serde_json::from_reader(BufReader::new(file))
            .context("invalid snapshot; rebuild from source")?;
        ensure!(
            snapshot.schema_version == SCHEMA_VERSION,
            "unsupported snapshot schema; rebuild with a compatible binary"
        );
        Ok(Some(snapshot))
    }

    fn save(&self, snapshot: &Snapshot) -> Result<()> {
        let _lifecycle = self.lifecycle(true)?;
        sqlite::save(&self.directory, snapshot, None)
    }
}

pub struct WriterLock {
    path: PathBuf,
    _file: File,
    _lifecycle: Option<Arc<File>>,
    object_captures: std::sync::atomic::AtomicUsize,
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
        .worker_lock()?
        .context("scheduler worker active; wait before removing collections")?;
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
        let _ = fs::remove_file(&self.path);
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
