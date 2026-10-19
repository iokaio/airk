// SPDX-License-Identifier: Apache-2.0
//! Explicit format adoption by rebuilding derived state, never reading old evidence.
//!
//! Source roots, saved extraction/vector/scan configuration, model source assets,
//! and root quota configuration survive. Obsolete history, checkpoint bindings,
//! and their SQLite administration are derived state and are deliberately reset.
use crate::{FileStore, WriterLock};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub const CURRENT_RETRIEVAL_ENGINE: &str = "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-9/inventory-4/model-objects-1/source-context-1/vector-units-1/vector-map-1/vector-selection-1/embedding-packs-1/detail-projection-2";
const INTENT: &str = "index-reset.json";
const MAX_INTENT: u64 = 16_384;
const MAX_ENTRIES: usize = 1_000_000;
const DERIVED: &[&str] = &[
    "index.sqlite3",
    "index.sqlite3-wal",
    "index.sqlite3-shm",
    "index.sqlite3-journal",
    "snapshot.json",
    "generations",
    "objects",
    "history",
    "maintenance",
    "checkpoints",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexFormat {
    Empty,
    Current,
    Obsolete,
    ResetRequired,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u32,
    root: String,
    collection: String,
    prior_incarnation: Option<airk_core::identity::Incarnation>,
    ready_for_build: bool,
}

#[derive(Deserialize)]
struct FormatHeader {
    schema: u32,
    engine: String,
}

#[derive(Clone, Copy)]
enum Stage {
    IntentDurable,
    DerivedRemoved,
    ReadyForBuild,
}

fn plain(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "linked index reset state"
    );
    Ok(metadata)
}

fn bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let metadata = plain(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= maximum,
        "index reset metadata exceeds bound"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "index reset metadata exceeds bound"
    );
    Ok(bytes)
}

fn intent(directory: &Path) -> Result<Option<Intent>> {
    let path = directory.join(INTENT);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let intent: Intent = serde_json::from_slice(&bounded(&path, MAX_INTENT)?)?;
    ensure!(
        intent.version == 1 && !intent.root.is_empty() && intent.root.len() <= 32_768,
        "invalid index reset intent"
    );
    crate::validate_collection(&intent.collection)?;
    Ok(Some(intent))
}

pub(crate) fn ensure_ready(directory: &Path) -> Result<()> {
    ensure!(
        intent(directory)?.is_none(),
        "index rebuild required after format reset; run airk index --full"
    );
    Ok(())
}

/// A bounded header probe is not evidence validation and never opens old payloads.
pub(crate) fn current_manifest(bytes: &[u8]) -> Result<bool> {
    ensure!(
        bytes.len() <= 131_072,
        "index manifest exceeds format-probe bound"
    );
    let header: FormatHeader =
        serde_json::from_slice(bytes).context("invalid index manifest format header")?;
    ensure!(
        header.schema > 0 && !header.engine.is_empty() && header.engine.len() <= 1024,
        "invalid index manifest format header"
    );
    Ok(header.schema == 4 && header.engine == CURRENT_RETRIEVAL_ENGINE)
}

fn binding(
    directory: &Path,
    owner_root: &str,
    collection: &str,
) -> Result<Option<crate::PersistentRevision>> {
    crate::validate_collection(collection)?;
    let database = directory.join("index.sqlite3");
    if !database.try_exists()? {
        ensure!(
            !directory.join("snapshot.json").try_exists()?,
            "obsolete index has no bounded SQLite ownership metadata; remove its derived index before rebuilding"
        );
        return Ok(None);
    }
    ensure!(
        plain(&database)?.is_file(),
        "index coordination database is not a file"
    );
    let conn = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(250))?;
    let deadline = std::time::Instant::now() + Duration::from_millis(250);
    conn.progress_handler(1000, Some(move || std::time::Instant::now() > deadline))?;
    let stored: Option<(String, String)> = conn.query_row(
        "SELECT root,collection FROM revision WHERE id=1 AND length(CAST(root AS BLOB))<=32768 AND length(CAST(collection AS BLOB))<=64",
        [], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    let (root, id) = stored.context("index coordination ownership is missing")?;
    ensure!(
        root == owner_root && id == collection,
        "foreign index reset root or collection"
    );
    drop(conn);
    let header = crate::sqlite::persistent_revision(directory)?;
    if let Some(header) = &header {
        ensure!(
            header.root == owner_root && header.collection == collection,
            "foreign index component header binding"
        );
    }
    Ok(header)
}

impl FileStore {
    pub fn reset_required(&self) -> Result<bool> {
        Ok(intent(self.directory())?.is_some())
    }

    pub fn ensure_index_ready(&self) -> Result<()> {
        ensure_ready(self.directory())
    }

    pub fn index_format(&self, owner_root: &str, collection: &str) -> Result<IndexFormat> {
        if let Some(intent) = intent(self.directory())? {
            ensure!(
                intent.root == owner_root && intent.collection == collection,
                "foreign index reset intent"
            );
            return Ok(IndexFormat::ResetRequired);
        }
        let Some(header) = binding(self.directory(), owner_root, collection)? else {
            return Ok(if self.directory().join("index.sqlite3").try_exists()? {
                IndexFormat::Obsolete
            } else {
                IndexFormat::Empty
            });
        };
        // A manifest's format claim cannot upgrade legacy coordination into a
        // receipt-backed publication. Otherwise catalog() could take its legacy
        // snapshot branch after the kernel admitted this as a current index.
        if header.storage_format != 2 {
            return Ok(IndexFormat::Obsolete);
        }
        // binding() already verifies these identities against the committed
        // receipt. Malformed current coordination remains an error, never an
        // obsolete format that can bypass receipt validation during reset.
        ensure!(
            header.token.is_some()
                && header.inventory_sha256.is_some()
                && header.manifest_sha256.is_some(),
            "current index publication receipt identity missing"
        );
        ensure!(
            crate::blobs::valid_digest(&header.revision),
            "invalid index component identity"
        );
        let generations = self.directory().join("generations");
        let directory = generations.join(&header.revision);
        ensure!(
            plain(&generations)?.is_dir() && plain(&directory)?.is_dir(),
            "invalid index component directory"
        );
        let path = directory.join("manifest.json");
        let bytes = if let Some(hash) = &header.manifest_sha256 {
            ensure!(plain(&path)?.is_file(), "invalid index manifest file");
            crate::blobs::read_verified(&path, hash, 131_072)?
        } else {
            bounded(&path, 131_072)?
        };
        Ok(if current_manifest(&bytes)? {
            IndexFormat::Current
        } else {
            IndexFormat::Obsolete
        })
    }

    /// The caller deliberately requested a full rebuild and holds the writer.
    /// Failure after the durable intent leaves reads unavailable until a retry.
    /// History settings and checkpoints in the old coordination database are
    /// discarded with that incompatible derived history, never migrated.
    pub fn reset_obsolete_index(
        &self,
        writer: &WriterLock,
        owner_root: &str,
        collection: &str,
    ) -> Result<()> {
        self.reset_observed(writer, owner_root, collection, &mut |_| Ok(()))
    }

    fn reset_observed(
        &self,
        writer: &WriterLock,
        owner_root: &str,
        collection: &str,
        observe: &mut dyn FnMut(Stage) -> Result<()>,
    ) -> Result<()> {
        let directory = self.directory().canonicalize()?;
        ensure!(
            plain(self.directory())?.is_dir()
                && writer.path == directory.join("writer.lock")
                && directory.file_name().and_then(|name| name.to_str()) == Some(collection),
            "foreign index reset writer or directory"
        );
        let format = self.index_format(owner_root, collection)?;
        ensure!(
            matches!(format, IndexFormat::Obsolete | IndexFormat::ResetRequired),
            "format reset requires an obsolete index or an interrupted reset"
        );
        let mut reset = match intent(&directory)? {
            Some(reset) => reset,
            None => Intent {
                version: 1,
                root: owner_root.into(),
                collection: collection.into(),
                prior_incarnation: binding(&directory, owner_root, collection)?
                    .and_then(|header| header.token.map(|token| token.incarnation)),
                ready_for_build: false,
            },
        };
        let roots = crate::roots::RootConfig::load(&directory, owner_root)?;
        for source in std::iter::once(owner_root).chain(roots.roots.values().map(String::as_str)) {
            let source = Path::new(source);
            let resolved = if source.exists() {
                source.canonicalize()?
            } else {
                std::path::absolute(source)?
            };
            ensure!(
                !resolved.starts_with(&directory),
                "source root lies inside index reset storage"
            );
        }
        let _readers = reader_exclusion(&directory)?;
        let (targets, _reservations) = derived_targets(&directory)?;
        // Validate the entire deletion set before making any destructive change.
        for target in &targets {
            validate_tree(target, &directory)?;
        }
        let data_root = directory
            .parent()
            .and_then(Path::parent)
            .context("index reset data root missing")?;
        ensure!(
            directory
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "collections"),
            "index reset requires managed collection storage"
        );
        let quota = crate::quota::Quota::open(data_root)?;
        let reservation = quota.try_reserve(collection, MAX_INTENT * 2, Duration::from_secs(5))?;
        reset.ready_for_build = false;
        write_intent(&directory, &reset, &reservation)?;
        observe(Stage::IntentDurable)?;
        for target in targets {
            validate_tree(&target, &directory)?;
            if plain(&target)?.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
            crate::maintenance::sync_parent(&directory)?;
            observe(Stage::DerivedRemoved)?;
        }
        reset.ready_for_build = true;
        write_intent(&directory, &reset, &reservation)?;
        observe(Stage::ReadyForBuild)?;
        Ok(())
    }

    /// Clear the reset marker only after a verified new-incarnation publication.
    pub fn finish_index_reset(
        &self,
        writer: &WriterLock,
        owner_root: &str,
        collection: &str,
    ) -> Result<()> {
        let directory = self.directory().canonicalize()?;
        ensure!(
            writer.path == directory.join("writer.lock"),
            "foreign index reset writer"
        );
        let reset = intent(&directory)?.context("index reset intent missing")?;
        ensure!(
            reset.ready_for_build && reset.root == owner_root && reset.collection == collection,
            "index reset is not ready to finish"
        );
        let header =
            binding(&directory, owner_root, collection)?.context("rebuilt index head missing")?;
        let token = header.token.context("rebuilt index revision missing")?;
        ensure!(
            Some(token.incarnation) != reset.prior_incarnation,
            "rebuilt index reused obsolete incarnation"
        );
        let receipt =
            crate::sqlite::receipt(&directory, &token)?.context("rebuilt index receipt missing")?;
        let inventory = crate::blobs::verify_v2_generation(&directory, &receipt)?;
        ensure!(
            inventory.root == owner_root && inventory.collection == collection,
            "rebuilt index binding mismatch"
        );
        let lineage: airk_core::changes::LineageEnvelope =
            serde_json::from_slice(&crate::blobs::read_verified(
                &directory
                    .join("generations")
                    .join(token.revision.to_string())
                    .join("lineage.json"),
                &inventory.lineage_digest.to_string(),
                airk_core::changes::MAX_LINEAGE_BYTES as u64,
            )?)?;
        ensure!(
            lineage.observation.parent.is_none() && lineage.ancestry.is_empty(),
            "rebuilt index retained obsolete ancestry"
        );
        fs::remove_file(directory.join(INTENT))?;
        crate::maintenance::sync_parent(&directory)
    }
}

fn write_intent(
    directory: &Path,
    intent: &Intent,
    reservation: &crate::quota::Reservation,
) -> Result<()> {
    let bytes = serde_json::to_vec(intent)?;
    ensure!(
        bytes.len() as u64 <= MAX_INTENT,
        "index reset intent exceeds bound"
    );
    reservation.charge(bytes.len() as u64)?;
    let mut temporary = tempfile::Builder::new()
        .prefix("reset-")
        .tempfile_in(directory)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(directory.join(INTENT))?;
    crate::maintenance::sync_parent(directory)
}

fn reader_exclusion(directory: &Path) -> Result<Vec<File>> {
    let root = directory
        .parent()
        .context("collection parent missing")?
        .join(".reader-locks");
    let leases = root.join(directory.file_name().context("collection name missing")?);
    if !leases.try_exists()? {
        return Ok(vec![]);
    }
    ensure!(
        plain(&root)?.is_dir() && plain(&leases)?.is_dir(),
        "invalid index reader lease root"
    );
    let mut result = Vec::new();
    for (visited, entry) in fs::read_dir(leases)?.enumerate() {
        ensure!(
            visited < MAX_ENTRIES,
            "index reader lease count exceeds bound"
        );
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("invalid reader lease name"))?;
        ensure!(
            crate::blobs::valid_digest(&name) && plain(&entry.path())?.is_file(),
            "invalid index reader lease"
        );
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(entry.path())?;
        file.try_lock().context(
            "index reset busy: a reader is active; disconnect clients before airk index --full",
        )?;
        result.push(file);
    }
    Ok(result)
}

fn derived_targets(directory: &Path) -> Result<(Vec<PathBuf>, Vec<File>)> {
    let mut targets = Vec::new();
    for name in DERIVED {
        let path = directory.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => targets.push(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut reservations = Vec::new();
    for (visited, entry) in fs::read_dir(directory)?.enumerate() {
        ensure!(
            visited < MAX_ENTRIES,
            "index reset entry count exceeds bound"
        );
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("non-UTF-8 collection entry"))?;
        let Some(record) = name.strip_prefix(".quota-stage-") else {
            continue;
        };
        ensure!(
            record.starts_with("r-")
                && record.len() <= 128
                && record
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid index reset staging name"
        );
        let data_root = directory
            .parent()
            .and_then(Path::parent)
            .context("index reset data root missing")?;
        let ledger = data_root.join(".quota/reservations");
        ensure!(
            plain(&data_root.join(".quota"))?.is_dir() && plain(&ledger)?.is_dir(),
            "invalid reset reservation ledger"
        );
        #[derive(Deserialize)]
        struct Record {
            collection: String,
        }
        let owner: Record = serde_json::from_slice(&bounded(&ledger.join(record), 1024)?)?;
        ensure!(
            directory.file_name().and_then(|name| name.to_str()) == Some(owner.collection.as_str()),
            "foreign reset reservation"
        );
        let lease_path = ledger.join(record).with_extension("lease");
        ensure!(
            plain(&lease_path)?.is_file(),
            "invalid reset reservation lease"
        );
        let lease = OpenOptions::new().read(true).write(true).open(lease_path)?;
        lease
            .try_lock()
            .context("index reset busy: a storage reservation is active")?;
        reservations.push(lease);
        targets.push(entry.path());
    }
    Ok((targets, reservations))
}

fn validate_tree(target: &Path, directory: &Path) -> Result<()> {
    ensure!(
        target.parent() == Some(directory),
        "index reset target escaped collection"
    );
    let mut pending = vec![target.to_path_buf()];
    let mut visited = 0usize;
    while let Some(path) = pending.pop() {
        visited += 1;
        ensure!(visited <= MAX_ENTRIES, "index reset tree exceeds bound");
        let metadata = plain(&path)?;
        ensure!(
            path.canonicalize()?.starts_with(directory),
            "index reset target escaped collection"
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                ensure!(
                    visited + pending.len() < MAX_ENTRIES,
                    "index reset tree exceeds bound"
                );
                pending.push(entry?.path());
            }
        } else {
            ensure!(
                metadata.is_file(),
                "index reset target is not a regular file"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{Snapshot, SnapshotStore};

    struct Fixture {
        _root: tempfile::TempDir,
        source: PathBuf,
        owner: String,
        store: FileStore,
        writer: WriterLock,
    }

    impl Fixture {
        fn old() -> Result<Self> {
            let root = tempfile::tempdir()?;
            let source = root.path().join("source");
            fs::create_dir(&source)?;
            fs::write(source.join("original.txt"), b"authoritative source")?;
            let owner = source.canonicalize()?.to_str().unwrap().to_owned();
            let store = FileStore::new(root.path().join("data/collections/fixture"));
            let writer = store.lock()?;
            let mut snapshot = Snapshot::empty(owner.clone());
            snapshot.collection = "fixture".into();
            store.save(&snapshot)?;
            // The obsolete evidence cannot be decoded. Ownership alone permits
            // an explicit rebuild and must never deserialize this payload.
            Connection::open(store.directory().join("index.sqlite3"))?.execute(
                "UPDATE revision SET payload='incompatible old evidence' WHERE id=1",
                [],
            )?;
            for name in [
                "generations",
                "objects",
                "history",
                "maintenance",
                "checkpoints",
            ] {
                fs::create_dir_all(store.directory().join(name))?;
                fs::write(
                    store.directory().join(name).join("old-derived-state"),
                    b"discard",
                )?;
            }
            for name in [
                "extraction-policy.json",
                "vector-policy.json",
                "history-policy.json",
                "user-notes.txt",
            ] {
                fs::write(store.directory().join(name), name.as_bytes())?;
            }
            // Old per-vector JSON objects are obsolete derived state. They are
            // discarded only by this explicit reset, never adopted into packs.
            fs::create_dir_all(store.directory().join("objects/embeddings"))?;
            fs::write(
                store
                    .directory()
                    .join("objects/embeddings")
                    .join("a".repeat(64)),
                b"obsolete vector JSON",
            )?;
            crate::roots::RootConfig::load(store.directory(), &owner)?.save(store.directory())?;
            Ok(Self {
                _root: root,
                source,
                owner,
                store,
                writer,
            })
        }

        fn manifest(&self, bytes: &[u8]) -> Result<()> {
            let mut snapshot = Snapshot::empty(self.owner.clone());
            snapshot.collection = "fixture".into();
            self.store
                .save_with_components(&snapshot, &"a".repeat(64))?;
            let directory = self
                .store
                .directory()
                .join("generations")
                .join("a".repeat(64));
            fs::create_dir_all(&directory)?;
            fs::write(directory.join("manifest.json"), bytes)?;
            Ok(())
        }
    }

    #[test]
    fn reset_preserves_sources_configuration_and_unknown_files() -> Result<()> {
        let fixture = Fixture::old()?;
        assert_eq!(
            fixture.store.index_format(&fixture.owner, "fixture")?,
            IndexFormat::Obsolete
        );
        let roots = fs::read(fixture.store.directory().join("roots.json"))?;
        fixture
            .store
            .reset_obsolete_index(&fixture.writer, &fixture.owner, "fixture")?;
        for name in DERIVED {
            assert!(
                !fixture.store.directory().join(name).exists(),
                "retained {name}"
            );
        }
        assert_eq!(
            fs::read(fixture.source.join("original.txt"))?,
            b"authoritative source"
        );
        for name in [
            "extraction-policy.json",
            "vector-policy.json",
            "history-policy.json",
            "user-notes.txt",
        ] {
            assert_eq!(
                fs::read(fixture.store.directory().join(name))?,
                name.as_bytes()
            );
        }
        assert_eq!(
            fs::read(fixture.store.directory().join("roots.json"))?,
            roots
        );
        assert_eq!(
            fixture.store.index_format(&fixture.owner, "fixture")?,
            IndexFormat::ResetRequired
        );
        assert!(fixture.store.load().is_err());
        assert!(fixture.store.persistent_revision().is_err());
        assert!(fixture.store.catalog().is_err());
        assert!(
            fixture
                .store
                .finish_index_reset(&fixture.writer, &fixture.owner, "fixture")
                .is_err()
        );
        assert!(fixture.store.reset_required()?);
        Ok(())
    }

    #[test]
    fn active_reader_fails_busy_before_reset_intent() -> Result<()> {
        let fixture = Fixture::old()?;
        let path = crate::blobs::reader_lease_path(fixture.store.directory(), &"a".repeat(64))?;
        fs::create_dir_all(path.parent().unwrap())?;
        let reader = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        reader.lock_shared()?;
        let error = fixture
            .store
            .reset_obsolete_index(&fixture.writer, &fixture.owner, "fixture")
            .unwrap_err();
        assert!(format!("{error:#}").contains("reader is active"));
        assert!(!fixture.store.reset_required()?);
        assert!(fixture.store.directory().join("index.sqlite3").exists());
        drop(reader);
        fixture
            .store
            .reset_obsolete_index(&fixture.writer, &fixture.owner, "fixture")?;
        Ok(())
    }

    #[test]
    fn reset_removes_only_inactive_owned_staging_without_opening_old_publications() -> Result<()> {
        let fixture = Fixture::old()?;
        let data = fixture
            .store
            .directory()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let quota = crate::quota::Quota::open(data)?;
        let active = quota.reserve("fixture", 1024 * 1024)?;
        let stage = active.staging_directory()?;
        fs::write(
            stage.join("prepared-publication.json"),
            b"obsolete prepared format",
        )?;
        assert!(
            fixture
                .store
                .reset_obsolete_index(&fixture.writer, &fixture.owner, "fixture")
                .is_err()
        );
        assert!(!fixture.store.reset_required()?);
        drop(active);
        let abandoned = fixture
            .store
            .directory()
            .join(".quota-stage-r-resetfixture");
        fs::create_dir(&abandoned)?;
        fs::write(
            abandoned.join("prepared-publication.json"),
            b"obsolete prepared format",
        )?;
        let ledger = data.join(".quota/reservations");
        fs::write(
            ledger.join("r-resetfixture"),
            br#"{"collection":"fixture","bytes":1048576}"#,
        )?;
        fs::write(ledger.join("r-resetfixture.lease"), [])?;
        fixture
            .store
            .reset_obsolete_index(&fixture.writer, &fixture.owner, "fixture")?;
        assert!(!abandoned.exists());
        // Subsequent normal admission can recover the orphan ledger record
        // without trying to parse the incompatible prepared publication.
        let _reservation = quota.reserve("fixture", 1024)?;
        Ok(())
    }

    #[test]
    fn interrupted_reset_is_unreadable_and_full_retry_is_idempotent() -> Result<()> {
        for crash_at in [
            Stage::IntentDurable,
            Stage::DerivedRemoved,
            Stage::ReadyForBuild,
        ] {
            let fixture = Fixture::old()?;
            let result = fixture.store.reset_observed(
                &fixture.writer,
                &fixture.owner,
                "fixture",
                &mut |stage| {
                    ensure!(
                        std::mem::discriminant(&stage) != std::mem::discriminant(&crash_at),
                        "injected reset interruption"
                    );
                    Ok(())
                },
            );
            assert!(result.is_err());
            assert!(fixture.store.reset_required()?);
            assert!(fixture.store.load().is_err());
            fixture
                .store
                .reset_obsolete_index(&fixture.writer, &fixture.owner, "fixture")?;
            assert!(intent(fixture.store.directory())?.unwrap().ready_for_build);
            assert!(!fixture.store.directory().join("generations").exists());
            assert_eq!(
                fs::read(fixture.source.join("original.txt"))?,
                b"authoritative source"
            );
        }
        Ok(())
    }

    #[test]
    fn reset_never_treats_foreign_or_corrupt_metadata_as_empty() -> Result<()> {
        let fixture = Fixture::old()?;
        assert!(
            fixture
                .store
                .reset_obsolete_index(&fixture.writer, "/foreign", "fixture")
                .is_err()
        );
        assert!(
            fixture
                .store
                .index_format(&fixture.owner, "foreign")
                .is_err()
        );
        fixture.manifest(b"invalid header")?;
        assert_eq!(
            fixture.store.index_format(&fixture.owner, "fixture")?,
            IndexFormat::Obsolete,
            "legacy coordination does not require decoding obsolete components"
        );
        Connection::open(fixture.store.directory().join("index.sqlite3"))?.execute(
            "UPDATE index_components SET payload='invalid coordination header' WHERE id=1",
            [],
        )?;
        assert!(
            fixture
                .store
                .index_format(&fixture.owner, "fixture")
                .is_err()
        );
        assert!(
            fixture
                .store
                .reset_obsolete_index(&fixture.writer, &fixture.owner, "fixture")
                .is_err()
        );
        assert!(!fixture.store.reset_required()?);
        assert!(fixture.store.directory().join("objects").exists());
        Ok(())
    }

    #[test]
    fn bounded_manifest_probe_identifies_only_format_not_old_evidence() -> Result<()> {
        let fixture = Fixture::old()?;
        fixture
            .manifest(br#"{"schema":3,"engine":"obsolete","old_payload":{"incompatible":true}}"#)?;
        assert_eq!(
            fixture.store.index_format(&fixture.owner, "fixture")?,
            IndexFormat::Obsolete
        );
        let current =
            serde_json::to_vec(&serde_json::json!({"schema":4,"engine":CURRENT_RETRIEVAL_ENGINE}))?;
        assert!(current_manifest(&current)?);
        fixture.manifest(&current)?;
        // Low-level legacy fixture readers remain available until their store
        // replacement. The kernel's format gate must never admit that branch,
        // even when its unauthenticated manifest claims the newest engine.
        assert!(fixture.store.catalog()?.unwrap().receipt.is_none());
        assert_eq!(
            fixture.store.index_format(&fixture.owner, "fixture")?,
            IndexFormat::Obsolete
        );
        fixture
            .store
            .reset_obsolete_index(&fixture.writer, &fixture.owner, "fixture")?;
        assert!(fixture.store.reset_required()?);
        assert!(current_manifest(&vec![b' '; 131_073]).is_err());
        Ok(())
    }
}
