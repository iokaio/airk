// SPDX-License-Identifier: Apache-2.0
//! Derived state only. Source workspaces remain authoritative.
use airk_core::{SCHEMA_VERSION, Snapshot, SnapshotStore};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
mod sqlite;

#[derive(Clone, Debug)]
pub struct FileStore {
    directory: PathBuf,
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
    /// One writer; a crash leaves a lock requiring deliberate operator recovery.
    pub fn lock(&self) -> Result<WriterLock> {
        fs::create_dir_all(&self.directory)?;
        let path = self.directory.join("writer.lock");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .context("index writer locked; see docs/operations.md for recovery")?;
        let guard = WriterLock {
            path,
            _file: file.try_clone()?,
        };
        writeln!(file, "pid={}", std::process::id())?;
        Ok(guard)
    }
}

impl SnapshotStore for FileStore {
    fn load(&self) -> Result<Option<Snapshot>> {
        if self.directory.join("index.sqlite3").exists() {
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
        sqlite::save(&self.directory, snapshot)
    }
}

pub struct WriterLock {
    path: PathBuf,
    _file: File,
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
    let collections = data_root.join("collections");
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
        let snapshot = FileStore::new(entry.path()).load();
        let (generation, indexed_files, status) = match snapshot {
            Ok(Some(s)) if s.collection == id => {
                (Some(s.generation), Some(s.artifacts.len()), "indexed")
            }
            Ok(None) => (None, None, "no-snapshot"),
            _ => (None, None, "invalid-snapshot"),
        };
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
    let base = data_root
        .join("collections")
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
    let snapshot = store
        .load()?
        .context("no managed snapshot; refusing collection deletion")?;
    ensure!(
        snapshot.collection == id,
        "collection identity mismatch; refusing deletion"
    );
    let (disk_bytes, files) = directory_usage(&resolved)?;
    // The lock remains held while snapshot and derived artifacts are removed.
    for entry in fs::read_dir(&resolved)? {
        let entry = entry?;
        if entry.file_name() == "writer.lock" {
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
    drop(lock);
    fs::remove_dir(&resolved)?;
    Ok(CollectionUsage {
        collection: id.into(),
        disk_bytes,
        files,
        generation: Some(snapshot.generation),
        indexed_files: Some(snapshot.artifacts.len()),
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
