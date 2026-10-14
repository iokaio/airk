// SPDX-License-Identifier: Apache-2.0
//! Explicit, quiescent relocation. Old managed data and source workspaces remain intact.
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MANIFEST_MAX: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;
const PROGRESS: &str = ".relocation-in-progress.json";
const PENDING: &str = ".relocation-pending.json";
const RETIRED: &str = ".relocated-to.json";
const MANIFEST: &str = ".relocation-manifest.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    bytes: u64,
    sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    source: PathBuf,
    destination: PathBuf,
    files: Vec<Entry>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    schema: u32,
    source: PathBuf,
    destination: PathBuf,
    transaction: String,
    manifest_sha256: String,
}

#[derive(Debug, Serialize)]
pub struct RelocationPlan {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub required_copy_headroom: u64,
    pub collections: Vec<String>,
    pub old_state_retained: bool,
}
#[derive(Debug, Serialize)]
pub struct RelocationResult {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub files: usize,
    pub copied_bytes: u64,
    pub old_state_retained: bool,
}

pub fn check_root(root: &Path) -> Result<()> {
    crate::quota::ensure_current_root(root)
}
fn plain(path: &Path) -> Result<()> {
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "linked relocation path: {}",
        path.display()
    );
    Ok(())
}
fn canonical_source(source: &Path) -> Result<PathBuf> {
    let root = source.canonicalize()?;
    ensure!(
        root.to_str().is_some() && root.is_dir(),
        "invalid managed source directory"
    );
    plain(&root.join("collections"))?;
    ensure!(
        root.join("collections").is_dir(),
        "relocation requires a managed collections directory"
    );
    Ok(root)
}
fn canonical_destination(source: &Path, destination: &Path) -> Result<PathBuf> {
    let name = destination
        .file_name()
        .context("relocation destination requires a new directory name")?;
    let parent = destination
        .parent()
        .context("relocation destination has no parent")?
        .canonicalize()?;
    let target = parent.join(name);
    ensure!(
        target.to_str().is_some()
            && target != source
            && !target.starts_with(source)
            && !source.starts_with(&target),
        "source and destination data roots must be separate"
    );
    Ok(target)
}
fn collections(source: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(source.join("collections"))? {
        let entry = entry?;
        plain(&entry.path())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("non-UTF-8 collection state"))?;
        if name == ".writer-locks" || name == ".reader-locks" {
            continue;
        }
        crate::validate_collection(&name)?;
        ensure!(entry.file_type()?.is_dir(), "invalid collection state");
        names.push(name);
    }
    names.sort();
    ensure!(
        !names.is_empty(),
        "relocation requires at least one indexed collection"
    );
    Ok(names)
}

struct Freeze {
    _lifecycle: File,
    _worker: File,
    _writers: Vec<File>,
    _usage: Vec<crate::usage::UsageLock>,
    _readers: Vec<File>,
}
fn freeze(source: &Path) -> Result<(Freeze, Vec<String>)> {
    let lifecycle = crate::quota::relocation_lifecycle_lease(source)?;
    // Enumerate under the root lease: a collection published between discovery
    // and freezing must not escape reader protection or boundary validation.
    let names = collections(source)?;
    let worker_path = source.join("scheduler-worker.lock");
    if worker_path.exists() {
        plain(&worker_path)?;
    }
    let worker = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(worker_path)?;
    worker
        .try_lock()
        .context("scheduler worker active; stop it before relocation")?;
    let writer_base = source.join("collections/.writer-locks");
    fs::create_dir_all(&writer_base)?;
    plain(&writer_base)?;
    let mut writers = Vec::new();
    let mut usage = Vec::new();
    let mut readers = Vec::new();
    for name in &names {
        let collection = source.join("collections").join(name);
        ensure!(
            !collection.join("writer.lock").exists(),
            "collection writer marker remains; finish/recover indexing before relocation"
        );
        let path = writer_base.join(name);
        if path.exists() {
            plain(&path)?;
        }
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        writer
            .try_lock()
            .context("collection writer active; stop it before relocation")?;
        writers.push(writer);
        usage.push(crate::usage::deletion_guard(&collection)?);
        readers.extend(crate::retention::exclusive_collection_leases(&collection)?);
    }
    Ok((
        Freeze {
            _lifecycle: lifecycle,
            _worker: worker,
            _writers: writers,
            _usage: usage,
            _readers: readers,
        },
        names,
    ))
}

fn validate_workspace_boundaries(
    source: &Path,
    destination: &Path,
    names: &[String],
) -> Result<()> {
    for name in names {
        let directory = source.join("collections").join(name);
        let database = directory.join("index.sqlite3");
        ensure!(
            database.exists(),
            "relocation requires current SQLite collection metadata"
        );
        plain(&database)?;
        let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let root: String =
            connection.query_row("SELECT root FROM revision WHERE id=1", [], |row| row.get(0))?;
        let mut roots = crate::roots::RootConfig::load(&directory, &root)?.roots;
        roots.insert(String::new(), root);
        // The published pointer can retain aliases removed from pending config.
        if let Ok(payload) = connection.query_row::<String, _, _>("SELECT payload FROM index_components WHERE id=1 AND length(CAST(payload AS BLOB))<=131072", [], |row| row.get(0)) {
            let value: serde_json::Value = serde_json::from_str(&payload)?;
            if let Some(published) = value.get("roots").and_then(serde_json::Value::as_object) {
                for (alias, path) in published { if let Some(path) = path.as_str() { roots.insert(format!("published:{alias}"), path.into()); } }
            }
        }
        for root in roots.values() {
            let root = Path::new(root);
            ensure!(
                !root.starts_with(source) && !root.starts_with(destination),
                "data relocation target must not contain a source workspace"
            );
        }
    }
    Ok(())
}

fn excluded(relative: &Path) -> bool {
    let value = relative.to_string_lossy().replace('\\', "/");
    if [
        PROGRESS,
        PENDING,
        RETIRED,
        MANIFEST,
        ".lifecycle.lock",
        "scheduler-worker.lock",
        ".airkignore",
    ]
    .contains(&value.as_str())
    {
        return true;
    }
    if value == ".quota" {
        return false;
    }
    if value.starts_with(".quota/") {
        return value != ".quota/policy.json";
    }
    if value.starts_with("collections/.writer-locks")
        || value.starts_with("collections/.reader-locks")
    {
        return true;
    }
    relative.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name == "writer.lock"
            || name == "usage.lock"
            || name.ends_with("-shm")
            || name.starts_with(".quota-stage-")
            || name.starts_with("building-")
            || name.starts_with(".coverage-")
            || name.starts_with(".relocation-marker-")
    })
}
fn digest(path: &Path) -> Result<(u64, String)> {
    plain(path)?;
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    ensure!(before.is_file(), "relocation only copies regular files");
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut bytes = 0u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        bytes = bytes
            .checked_add(count as u64)
            .context("relocation byte overflow")?;
    }
    let after = file.metadata()?;
    ensure!(
        bytes == before.len()
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "managed file changed during relocation inventory"
    );
    Ok((bytes, format!("{:x}", hash.finalize())))
}
fn inventory(source: &Path, destination: &Path) -> Result<Manifest> {
    let mut pending = vec![source.to_path_buf()];
    let mut files = Vec::new();
    let mut visited = 0usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            visited += 1;
            ensure!(
                visited <= MAX_ENTRIES,
                "relocation inventory exceeds 100000 entries"
            );
            let path = entry.path();
            plain(&path)?;
            let relative = path.strip_prefix(source)?;
            if excluded(relative) {
                continue;
            }
            if entry.file_type()?.is_dir() {
                pending.push(path);
                continue;
            }
            let (bytes, sha256) = digest(&path)?;
            files.push(Entry {
                path: relative
                    .to_str()
                    .context("non-UTF-8 managed path")?
                    .replace('\\', "/"),
                bytes,
                sha256,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        schema: 1,
        source: source.into(),
        destination: destination.into(),
        files,
    };
    ensure!(
        serde_json::to_vec(&manifest)?.len() as u64 <= MANIFEST_MAX,
        "relocation manifest exceeds 32 MiB"
    );
    Ok(manifest)
}
fn copy_requirements(manifest: &Manifest) -> Result<(u64, u64, BTreeMap<String, u64>)> {
    let mut bytes = 0u64;
    let mut collections = BTreeMap::<String, u64>::new();
    for entry in &manifest.files {
        bytes = bytes
            .checked_add(entry.bytes)
            .context("relocation size overflow")?;
        let parts: Vec<_> = entry.path.split('/').collect();
        if parts.len() >= 3
            && parts[0] == "collections"
            && crate::validate_collection(parts[1]).is_ok()
        {
            let value = collections.entry(parts[1].into()).or_default();
            *value = value
                .checked_add(entry.bytes.checked_mul(2).context("copy size overflow")?)
                .context("copy size overflow")?;
        }
    }
    let metadata = (serde_json::to_vec(manifest)?.len() as u64)
        .checked_mul(2)
        .context("manifest size overflow")?;
    let peak = bytes
        .checked_mul(2)
        .and_then(|value| value.checked_add(metadata))
        .and_then(|value| value.checked_add(131072))
        .context("relocation peak size overflow")?;
    Ok((bytes, peak, collections))
}

pub fn plan(source: &Path, destination: &Path) -> Result<RelocationPlan> {
    check_root(source)?;
    let source = canonical_source(source)?;
    let destination = canonical_destination(&source, destination)?;
    ensure!(
        !destination.exists(),
        "relocation destination already exists"
    );
    let (_freeze, names) = freeze(&source)?;
    validate_workspace_boundaries(&source, &destination, &names)?;
    let manifest = inventory(&source, &destination)?;
    let (bytes, headroom, copied) = copy_requirements(&manifest)?;
    crate::quota::Quota::open(&source)?.relocation_capacity(headroom, &copied)?;
    Ok(RelocationPlan {
        source,
        destination,
        files: manifest.files.len(),
        bytes,
        required_copy_headroom: headroom,
        collections: names,
        old_state_retained: true,
    })
}
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if path.exists() {
        plain(path)?;
    }
    let parent = path.parent().context("marker parent missing")?;
    plain(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".relocation-marker-")
        .tempfile_in(parent)?;
    serde_json::to_writer(temporary.as_file_mut(), value)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    sync_directory(parent)
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn load_json<T: serde::de::DeserializeOwned>(path: &Path, maximum: u64) -> Result<T> {
    plain(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "relocation metadata exceeds budget"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn relocate(source: &Path, destination: &Path) -> Result<RelocationResult> {
    check_root(source)?;
    let source = canonical_source(source)?;
    let destination = canonical_destination(&source, destination)?;
    ensure!(
        !destination.exists(),
        "relocation destination already exists"
    );
    let (_freeze, names) = freeze(&source)?;
    validate_workspace_boundaries(&source, &destination, &names)?;
    let manifest = inventory(&source, &destination)?;
    let (_, headroom, copied) = copy_requirements(&manifest)?;
    crate::quota::Quota::open(&source)?.relocation_capacity(headroom, &copied)?;
    let bytes = serde_json::to_vec(&manifest)?;
    let marker = Marker {
        schema: 1,
        source: source.clone(),
        destination,
        transaction: format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ),
        manifest_sha256: format!("{:x}", Sha256::digest(&bytes)),
    };
    atomic_json(&source.join(MANIFEST), &manifest)?;
    atomic_json(&source.join(PROGRESS), &marker)?;
    #[cfg(test)]
    if std::env::var_os("AIRK_RELOCATION_TEST_CRASH").is_some() {
        std::process::exit(0);
    }
    finish(&marker, &manifest)
}

fn checked_path(root: &Path, relative: &str, create: bool) -> Result<PathBuf> {
    let relative = Path::new(relative);
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "relocation manifest path escapes target"
    );
    let path = root.join(relative);
    let mut parent = root.to_path_buf();
    for part in relative
        .parent()
        .context("manifest parent missing")?
        .components()
    {
        parent.push(part);
        if create && !parent.exists() {
            fs::create_dir(&parent)?;
        }
        plain(&parent)?;
    }
    if path.exists() {
        plain(&path)?;
    }
    Ok(path)
}
fn checked_target(root: &Path, relative: &str) -> Result<PathBuf> {
    checked_path(root, relative, true)
}
fn matching_marker(path: &Path, marker: &Marker) -> Result<()> {
    let other: Marker = load_json(path, 65536)?;
    ensure!(
        other.schema == 1
            && other.transaction == marker.transaction
            && other.source == marker.source
            && other.destination == marker.destination
            && other.manifest_sha256 == marker.manifest_sha256,
        "relocation transaction markers disagree"
    );
    Ok(())
}
fn finish(marker: &Marker, manifest: &Manifest) -> Result<RelocationResult> {
    let destination = &marker.destination;
    if !destination.exists() {
        fs::create_dir(destination)?;
        atomic_json(&destination.join(PENDING), marker)?;
    } else {
        plain(destination)?;
        if !destination.join(PENDING).exists() {
            ensure!(
                fs::read_dir(destination)?.next().is_none(),
                "destination has unowned state; refusing relocation recovery"
            );
            atomic_json(&destination.join(PENDING), marker)?;
        } else {
            matching_marker(&destination.join(PENDING), marker)?;
        }
    }
    // Active readers/writers cannot open a pending destination; hold its lifecycle
    // too so activation and validation cannot race a client configured early.
    let _destination_lease = crate::quota::lifecycle_lease(destination, true)?;
    atomic_json(&destination.join(MANIFEST), manifest)?;
    let mut copied = 0u64;
    for entry in &manifest.files {
        ensure!(
            crate::blobs::valid_digest(&entry.sha256),
            "invalid relocation file digest"
        );
        let source = checked_path(&marker.source, &entry.path, false)?;
        ensure!(
            digest(&source)? == (entry.bytes, entry.sha256.clone()),
            "source changed since relocation was prepared"
        );
        let target = checked_target(destination, &entry.path)?;
        if target.exists() && digest(&target)? == (entry.bytes, entry.sha256.clone()) {
            copied += entry.bytes;
            continue;
        }
        let parent = target.parent().context("copy target parent missing")?;
        let prefix = format!(".relocation-copy-{}-", marker.transaction);
        // A previous interrupted attempt can leave only our specifically named
        // temporary in this verified, transaction-owned destination directory.
        for temporary in fs::read_dir(parent)? {
            let temporary = temporary?;
            if temporary.file_name().to_string_lossy().starts_with(&prefix) {
                plain(&temporary.path())?;
                ensure!(
                    temporary.file_type()?.is_file(),
                    "invalid relocation temporary"
                );
                fs::remove_file(temporary.path())?;
            }
        }
        let mut output = tempfile::Builder::new()
            .prefix(&prefix)
            .tempfile_in(parent)?;
        let mut input = File::open(&source)?;
        let length = std::io::copy(&mut (&mut input).take(entry.bytes), &mut output)?;
        let mut trailing = [0u8; 1];
        ensure!(
            length == entry.bytes && input.read(&mut trailing)? == 0,
            "source size changed during relocation copy"
        );
        output.as_file().sync_all()?;
        ensure!(
            digest(output.path())? == (entry.bytes, entry.sha256.clone()),
            "relocation copy verification failed"
        );
        output.persist(&target)?;
        sync_directory(parent)?;
        copied = copied
            .checked_add(length)
            .context("relocation copy count overflow")?;
    }
    // Recreate empty lease files with fresh OS ownership; never copy an active lock.
    for entry in &manifest.files {
        let parts: Vec<_> = entry.path.split('/').collect();
        if parts.len() >= 5
            && parts[0] == "collections"
            && parts[2] == "generations"
            && crate::blobs::valid_digest(parts[3])
        {
            let lease = checked_target(
                destination,
                &format!("collections/.reader-locks/{}/{}", parts[1], parts[3]),
            )?;
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(lease)?;
        }
    }
    // The old custom data directory may live inside a source workspace. Retiring
    // it must not cause its derived files to enter the next source inventory.
    for root in [&marker.source, destination] {
        let path = root.join(".airkignore");
        if path.exists() {
            plain(&path)?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.write_all(b"*\n")?;
        file.sync_all()?;
    }
    atomic_json(&marker.source.join(RETIRED), marker)?;
    matching_marker(&destination.join(PENDING), marker)?;
    fs::remove_file(destination.join(PENDING))?;
    sync_directory(destination)?;
    if marker.source.join(PROGRESS).exists() {
        fs::remove_file(marker.source.join(PROGRESS))?;
    }
    sync_directory(&marker.source)?;
    Ok(RelocationResult {
        source: marker.source.clone(),
        destination: destination.clone(),
        files: manifest.files.len(),
        copied_bytes: copied,
        old_state_retained: true,
    })
}

pub fn recover(source: &Path) -> Result<RelocationResult> {
    let source = canonical_source(source)?;
    let marker_path = if source.join(PROGRESS).exists() {
        source.join(PROGRESS)
    } else {
        source.join(RETIRED)
    };
    let marker: Marker = load_json(&marker_path, 65536)?;
    ensure!(
        marker.schema == 1
            && marker.source == source
            && marker
                .transaction
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'-'),
        "invalid relocation transaction binding"
    );
    ensure!(
        canonical_destination(&source, &marker.destination)? == marker.destination,
        "relocation destination identity changed"
    );
    let (_freeze, names) = freeze(&source)?;
    validate_workspace_boundaries(&source, &marker.destination, &names)?;
    let manifest: Manifest = load_json(&source.join(MANIFEST), MANIFEST_MAX)?;
    ensure!(
        manifest.schema == 1
            && manifest.source == source
            && manifest.destination == marker.destination
            && format!("{:x}", Sha256::digest(serde_json::to_vec(&manifest)?))
                == marker.manifest_sha256,
        "relocation manifest binding/digest mismatch"
    );
    // If activation already completed, a repeated recovery is read-only and does
    // not rewrite a destination which may now have newer published generations.
    if source.join(RETIRED).exists() && !marker.destination.join(PENDING).exists() {
        matching_marker(&source.join(RETIRED), &marker)?;
        let completed: Manifest = load_json(&marker.destination.join(MANIFEST), MANIFEST_MAX)?;
        ensure!(
            format!("{:x}", Sha256::digest(serde_json::to_vec(&completed)?))
                == marker.manifest_sha256,
            "completed relocation destination receipt is missing or changed"
        );
        let bytes = manifest.files.iter().try_fold(0u64, |total, entry| {
            total
                .checked_add(entry.bytes)
                .context("relocation size overflow")
        })?;
        return Ok(RelocationResult {
            source,
            destination: marker.destination,
            files: manifest.files.len(),
            copied_bytes: bytes,
            old_state_retained: true,
        });
    }
    finish(&marker, &manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{Snapshot, SnapshotStore};

    fn fixture(base: &Path, workspace: &Path) -> Result<PathBuf> {
        let root = base.join("state");
        let store = crate::FileStore::new(root.join("collections/default"));
        let mut snapshot =
            Snapshot::empty(workspace.canonicalize()?.to_string_lossy().into_owned());
        snapshot.generation = 7;
        store.save(&snapshot)?;
        Ok(root)
    }
    #[test]
    fn relocation_preserves_evidence_sources_and_newer_destination_state() -> Result<()> {
        let base = tempfile::tempdir()?;
        let workspace = tempfile::tempdir()?;
        fs::write(workspace.path().join("source.txt"), "original source")?;
        let source = fixture(base.path(), workspace.path())?;
        let destination = base.path().join("moved");
        let preview = plan(&source, &destination)?;
        assert!(preview.files > 0 && preview.bytes > 0 && preview.old_state_retained);
        let moved = relocate(&source, &destination)?;
        assert_eq!(moved.files, preview.files);
        assert!(check_root(&source).is_err());
        check_root(&destination)?;
        assert_eq!(
            crate::FileStore::new(destination.join("collections/default"))
                .load()?
                .expect("copied collection")
                .generation,
            7
        );
        assert!(source.join("collections/default/index.sqlite3").exists());
        assert_eq!(fs::read_to_string(source.join(".airkignore"))?, "*\n");
        assert_eq!(
            fs::read_to_string(workspace.path().join("source.txt"))?,
            "original source"
        );
        fs::write(
            destination.join("newer-state"),
            "new publication after relocation",
        )?;
        recover(&source)?;
        assert!(
            destination.join("newer-state").exists(),
            "idempotent recovery must not overwrite a live destination"
        );
        Ok(())
    }
    #[test]
    fn active_reservations_and_generation_readers_prevent_relocation() -> Result<()> {
        let base = tempfile::tempdir()?;
        let workspace = tempfile::tempdir()?;
        let source = fixture(base.path(), workspace.path())?;
        let destination = base.path().join("moved");
        let reservation = crate::quota::Quota::open(&source)?.reserve("default", 1)?;
        assert!(plan(&source, &destination).is_err());
        drop(reservation);
        let revision = "a".repeat(64);
        fs::create_dir_all(
            source
                .join("collections/default/generations")
                .join(&revision),
        )?;
        let locks = source.join("collections/.reader-locks/default");
        fs::create_dir_all(&locks)?;
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(locks.join(revision))?;
        lease.lock_shared()?;
        assert!(plan(&source, &destination).is_err());
        drop(lease);
        assert!(plan(&source, &destination).is_ok());
        Ok(())
    }
    #[test]
    fn insufficient_copy_headroom_preserves_current_root() -> Result<()> {
        let base = tempfile::tempdir()?;
        let workspace = tempfile::tempdir()?;
        let source = fixture(base.path(), workspace.path())?;
        let destination = base.path().join("moved");
        let preview = plan(&source, &destination)?;
        let quota = crate::quota::Quota::open(&source)?;
        let used = quota.status()?.used_bytes;
        quota.configure(crate::quota::QuotaPolicy {
            root_bytes: used + preview.required_copy_headroom - 1,
            default_collection_bytes: u64::MAX,
            collections: BTreeMap::new(),
        })?;
        assert!(relocate(&source, &destination).is_err());
        check_root(&source)?;
        assert!(!destination.exists());
        assert_eq!(
            crate::FileStore::new(source.join("collections/default"))
                .load()?
                .expect("old publication")
                .generation,
            7
        );
        Ok(())
    }
    #[test]
    fn process_exit_after_prepare_is_recoverable_without_source_loss() -> Result<()> {
        let base = tempfile::tempdir()?;
        let workspace = tempfile::tempdir()?;
        let source = fixture(base.path(), workspace.path())?;
        let destination = base.path().join("moved");
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "relocation::tests::relocation_crash_child",
                "--nocapture",
            ])
            .env("AIRK_RELOCATION_TEST_CRASH", "1")
            .env("AIRK_RELOCATION_TEST_SOURCE", &source)
            .env("AIRK_RELOCATION_TEST_DESTINATION", &destination)
            .status()?;
        assert!(status.success());
        assert!(check_root(&source).is_err());
        assert!(source.join("collections/default/index.sqlite3").exists());
        recover(&source)?;
        check_root(&destination)?;
        assert_eq!(
            crate::FileStore::new(destination.join("collections/default"))
                .load()?
                .expect("recovered publication")
                .generation,
            7
        );
        Ok(())
    }
    #[test]
    fn relocation_crash_child() -> Result<()> {
        let Some(source) = std::env::var_os("AIRK_RELOCATION_TEST_SOURCE") else {
            return Ok(());
        };
        let destination =
            std::env::var_os("AIRK_RELOCATION_TEST_DESTINATION").context("child destination")?;
        relocate(Path::new(&source), Path::new(&destination))?;
        anyhow::bail!("child crash hook was not reached")
    }
}
