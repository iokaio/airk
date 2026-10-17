// SPDX-License-Identifier: Apache-2.0
//! Embedded transactional catalog. Read tools never create/migrate a database.
use crate::{FileStore, PersistentRevision};
use airk_core::*;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

struct PinnedConnection {
    detail: airk_core::extraction::IndexDetail,
    conn: Connection,
    _lease: Option<std::fs::File>,
    publication: Option<airk_core::evidence::RevisionEnvelope>,
}

fn is_v2_root(version: u32) -> bool {
    matches!(version, 3..=5)
}

fn publication_root_version(conn: &Connection) -> Result<u32> {
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let administrative = table_exists(conn, "history_admin")?;
    ensure!(
        version < 5 || administrative,
        "history administrative authority missing"
    );
    Ok(if administrative { 5 } else { 4 })
}

fn history_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS history_bindings(revision TEXT PRIMARY KEY,incarnation TEXT NOT NULL,generation INTEGER NOT NULL,payload TEXT NOT NULL); CREATE INDEX IF NOT EXISTS history_scope ON history_bindings(incarnation,generation);")?;
    Ok(())
}

pub(crate) fn history_binding(
    directory: &Path,
    token: &airk_core::identity::RevisionToken,
) -> Result<Option<crate::history::HistoryBinding>> {
    retry_wal_protocol(|| {
        let conn = connect_root_once(directory, false)?;
        if !table_exists(&conn, "history_bindings")? {
            return Ok(None);
        }
        let encoded: Option<String> = conn.query_row("SELECT CAST(substr(CAST(payload AS BLOB),1,4097) AS TEXT) FROM history_bindings WHERE revision=?1", [token.revision.to_string()], |row| row.get(0)).optional()?;
        encoded
            .map(|encoded| {
                let binding: crate::history::HistoryBinding = serde_json::from_str(&encoded)?;
                binding.validate()?;
                ensure!(
                    binding.publication.token == *token,
                    "foreign historical binding"
                );
                Ok(binding)
            })
            .transpose()
    })
}

pub(crate) fn history_bindings(directory: &Path) -> Result<Vec<crate::history::HistoryBinding>> {
    retry_wal_protocol(|| {
        let conn = connect_root_once(directory, false)?;
        if !table_exists(&conn, "history_bindings")? {
            return Ok(vec![]);
        }
        let mut statement = conn.prepare("SELECT CAST(substr(CAST(payload AS BLOB),1,4097) AS TEXT) FROM history_bindings ORDER BY generation,revision LIMIT 257")?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(rows.len() <= 256, "historical binding count limit exceeded");
        rows.into_iter()
            .map(|encoded| {
                ensure!(
                    encoded.len() <= 4096,
                    "historical binding byte limit exceeded"
                );
                let binding: crate::history::HistoryBinding = serde_json::from_str(&encoded)?;
                binding.validate()?;
                ensure!(
                    receipt_connection(&conn, &binding.publication.token)?.as_ref()
                        == Some(&binding.publication),
                    "historical publication receipt changed"
                );
                Ok(binding)
            })
            .collect()
    })
}

pub(crate) fn commit_history_binding(
    directory: &Path,
    binding: &crate::history::HistoryBinding,
    expected_head: &airk_core::identity::RevisionToken,
    reservation: &crate::quota::Reservation,
) -> Result<()> {
    binding.validate()?;
    let (maximum, allowance) = pointer_budget(directory)?;
    reservation.charge(allowance)?;
    let mut conn = connect_root(directory, true)?;
    conn.execute_batch(&format!(
        "PRAGMA max_page_count={maximum}; PRAGMA cache_spill=OFF; PRAGMA temp_store=MEMORY;"
    ))?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let version: u32 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    ensure!(
        is_v2_root(version),
        "history requires a committed v2 publication"
    );
    ensure!(
        expected_head_connection(directory, &tx)? == crate::ExpectedHead::V2(*expected_head),
        "pointer changed before historical binding commit"
    );
    binding.publication.token.validate_against(expected_head)?;
    ensure!(
        binding.publication.token != *expected_head,
        "active publication cannot become historical"
    );
    ensure!(
        receipt_connection(&tx, &binding.publication.token)?.as_ref() == Some(&binding.publication),
        "history conversion publication receipt changed"
    );
    history_schema(&tx)?;
    let encoded = serde_json::to_string(binding)?;
    ensure!(encoded.len() <= 4096, "historical binding too large");
    let existing: Option<String> = tx.query_row("SELECT CAST(substr(CAST(payload AS BLOB),1,4097) AS TEXT) FROM history_bindings WHERE revision=?1", [binding.publication.token.revision.to_string()], |row| row.get(0)).optional()?;
    if let Some(existing) = existing {
        ensure!(
            existing == encoded,
            "conflicting historical representation binding"
        );
    } else {
        let count: i64 = tx.query_row("SELECT count(*) FROM history_bindings", [], |row| {
            row.get(0)
        })?;
        ensure!(count < 256, "historical binding count limit reached");
        tx.execute(
            "INSERT INTO history_bindings VALUES(?1,?2,?3,?4)",
            params![
                binding.publication.token.revision.to_string(),
                binding.publication.token.incarnation.to_string(),
                i64::try_from(binding.publication.token.generation)?,
                encoded
            ],
        )?;
    }
    tx.pragma_update(None, "user_version", publication_root_version(&tx)?)?;
    tx.commit()?;
    Ok(())
}

/// Percent-encode the entire native UTF-8 filename, including separators. SQLite
/// decodes it after URI parsing, preserving Windows extended paths without treating
/// UNC prefixes as URI authorities or filename '?'/'#' bytes as URI syntax.
fn open_immutable_catalog(path: &Path) -> Result<Connection> {
    use std::fmt::Write;
    let path = path.canonicalize()?;
    let native = path.to_str().context("non-UTF-8 catalog path")?;
    let mut uri = String::from("file:");
    for byte in native.bytes() {
        write!(&mut uri, "%{byte:02X}").expect("string formatting is infallible");
    }
    uri.push_str("?mode=ro&immutable=1");
    Ok(Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?)
}
impl std::ops::Deref for PinnedConnection {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.conn
    }
}
impl std::ops::DerefMut for PinnedConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

/// WAL read-lock negotiation can report SQLITE_PROTOCOL under concurrent
/// connection churn. Restart only a read or pre-transaction connection setup;
/// never replay a mutation whose commit result might be ambiguous.
fn retry_wal_protocol<T>(mut read: impl FnMut() -> Result<T>) -> Result<T> {
    for attempt in 0..3 {
        match read() {
            Err(error)
                if attempt < 2
                    && error
                        .downcast_ref::<rusqlite::Error>()
                        .is_some_and(|error| {
                            matches!(error, rusqlite::Error::SqliteFailure(code, _)
                            if code.extended_code & 0xff == rusqlite::ffi::SQLITE_PROTOCOL)
                        }) =>
            {
                std::thread::sleep(Duration::from_millis(10 * (attempt + 1)));
            }
            result => return result,
        }
    }
    unreachable!("last attempt returns its result")
}

fn connect_root(directory: &Path, write: bool) -> Result<Connection> {
    retry_wal_protocol(|| connect_root_once(directory, write))
}

fn connect_root_once(directory: &Path, write: bool) -> Result<Connection> {
    let flags = if write {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let conn = Connection::open_with_flags(directory.join("index.sqlite3"), flags)?;
    conn.busy_timeout(Duration::from_secs(2))?;
    if !write {
        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        ensure!(
            (1..=crate::ROOT_POINTER_VERSION).contains(&version),
            "unsupported SQLite schema"
        );
        let deadline = Instant::now() + Duration::from_millis(250);
        conn.progress_handler(1000, Some(move || Instant::now() > deadline))?;
    }
    if write {
        let journal: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        if !journal.eq_ignore_ascii_case("wal") {
            conn.execute_batch("PRAGMA journal_mode=WAL")?;
        }
        conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA max_page_count=262144;")?;
    }
    Ok(conn)
}

fn connect(directory: &Path, write: bool) -> Result<PinnedConnection> {
    let conn = connect_root(directory, write)?;
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if !write && is_v2_root(version) {
        drop(conn);
        return read_transaction(directory);
    }
    if !write && version == 2 {
        let header = persistent_revision(directory)?.context("active catalog pointer missing")?;
        ensure!(
            header.storage_format == 1,
            "unsupported catalog storage format"
        );
        let lease = std::fs::File::open(crate::blobs::reader_lease_path(
            directory,
            &header.revision,
        )?)?;
        lease.lock_shared()?;
        let conn = Connection::open_with_flags(
            directory
                .join("generations")
                .join(&header.revision)
                .join("catalog.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        identity(&conn, &header.root, &header.collection).and_then(|generation| {
            ensure!(
                generation == header.generation,
                "catalog generation mismatch"
            );
            Ok(())
        })?;
        let deadline = Instant::now() + Duration::from_millis(250);
        conn.progress_handler(1000, Some(move || Instant::now() > deadline))?;
        return Ok(PinnedConnection {
            detail: Default::default(),
            conn,
            _lease: Some(lease),
            publication: None,
        });
    }
    Ok(PinnedConnection {
        detail: Default::default(),
        conn,
        _lease: None,
        publication: None,
    })
}

/// Select legacy rows or an immutable generation while the active pointer is
/// pinned by a read transaction. Migration cannot combine new metadata/old facts.
fn read_transaction(directory: &Path) -> Result<PinnedConnection> {
    retry_wal_protocol(|| read_transaction_once(directory))
}

fn read_transaction_once(directory: &Path) -> Result<PinnedConnection> {
    for _ in 0..3 {
        let conn = connect_root_once(directory, false)?;
        conn.execute_batch("BEGIN DEFERRED")?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 1 {
            return Ok(PinnedConnection {
                detail: Default::default(),
                conn,
                _lease: None,
                publication: None,
            });
        }
        let payload: String = conn.query_row("SELECT payload FROM index_components WHERE id=1 AND length(CAST(payload AS BLOB)) <= 131072", [], |r| r.get(0))?;
        let header: PersistentRevision = serde_json::from_str(&payload)?;
        ensure!(
            matches!(header.storage_format, 1 | 2),
            "unsupported active catalog format"
        );
        let open = || -> Result<PinnedConnection> {
            let lease = std::fs::File::open(crate::blobs::reader_lease_path(
                directory,
                &header.revision,
            )?)?;
            lease.lock_shared()?;
            let catalog_path = directory
                .join("generations")
                .join(&header.revision)
                .join("catalog.sqlite");
            let inner = if is_v2_root(version) {
                open_immutable_catalog(&catalog_path)?
            } else {
                Connection::open_with_flags(catalog_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?
            };
            let mut detail = airk_core::extraction::IndexDetail::Detailed;
            if is_v2_root(version) {
                let token = header.token.context("v2 catalog token missing")?;
                let receipt = receipt_connection(&conn, &token)?.context("v2 receipt missing")?;
                detail = crate::blobs::verify_v2_for_query(directory, &receipt)?;
                validate_v2_catalog(&inner, &token, &receipt.inventory_digest)?;
            }
            inner.execute_batch("BEGIN DEFERRED")?;
            ensure!(
                identity(&inner, &header.root, &header.collection)? == header.generation,
                "catalog generation mismatch"
            );
            let deadline = Instant::now() + Duration::from_millis(250);
            inner.progress_handler(1000, Some(move || Instant::now() > deadline))?;
            Ok(PinnedConnection {
                detail,
                conn: inner,
                _lease: Some(lease),
                publication: header.token.filter(|_| is_v2_root(version)).map(|token| {
                    airk_core::evidence::RevisionEnvelope::new(header.collection.clone(), token)
                }),
            })
        };
        let result = open();
        drop(conn);
        match result {
            Ok(value) => return Ok(value),
            Err(error) => {
                if persistent_revision(directory)?
                    .is_none_or(|current| current.revision == header.revision)
                {
                    return Err(error);
                }
            }
        }
    }
    anyhow::bail!("catalog changed repeatedly while opening exact-fact transaction")
}

pub(super) fn load(directory: &Path) -> Result<Option<Snapshot>> {
    let conn = connect(directory, false)?;
    let payload: Option<String> = conn
        .query_row("SELECT CASE WHEN length(CAST(payload AS BLOB)) <= 536870912 THEN payload ELSE NULL END FROM revision WHERE id=1", [], |r| r.get(0))
        .context("invalid SQLite catalog; rebuild with a compatible binary")?;
    let payload = payload.context("snapshot exceeds 512 MiB limit")?;
    let snapshot: Snapshot = serde_json::from_str(&payload)?;
    ensure!(
        snapshot.schema_version == SCHEMA_VERSION
            || (snapshot.schema_version == 4 && snapshot.storage_format == 2),
        "unsupported snapshot schema"
    );
    Ok(Some(snapshot))
}

pub(super) fn legacy_payload_bytes(directory: &Path) -> Result<u64> {
    let conn = connect_root(directory, false)?;
    let bytes: i64 = conn.query_row(
        "SELECT length(CAST(payload AS BLOB)) FROM revision WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    Ok(u64::try_from(bytes)?)
}

pub(super) fn persistent_revision(directory: &Path) -> Result<Option<PersistentRevision>> {
    retry_wal_protocol(|| persistent_revision_once(directory))
}

fn persistent_revision_once(directory: &Path) -> Result<Option<PersistentRevision>> {
    let conn = connect_root_once(directory, false)?;
    conn.execute_batch("BEGIN DEFERRED")?;
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='index_components')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let record: Option<(String, String, String, i64, u32)> = conn.query_row(
        "SELECT c.payload,r.root,r.collection,r.generation,r.schema_version FROM index_components c JOIN revision r ON c.id=r.id WHERE c.id=1 AND length(CAST(c.payload AS BLOB)) <= 131072",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
    record
        .map(|(payload, root, collection, generation, schema)| {
            let header: PersistentRevision = serde_json::from_str(&payload)?;
            ensure!(
                ((schema == SCHEMA_VERSION && header.storage_format <= 1 && version <= 2)
                    || (schema == 4 && header.storage_format == 2 && is_v2_root(version)))
                    && header.root == root
                    && header.collection == collection
                    && header.generation == u64::try_from(generation)?,
                "component header does not match active revision"
            );
            if is_v2_root(version) {
                let token = header.token.context("v2 header missing token")?;
                ensure!(
                    token.generation == header.generation
                        && token.revision.to_string() == header.revision,
                    "v2 token header mismatch"
                );
                let receipt = receipt_connection(&conn, &token)?
                    .context("active publication receipt missing")?;
                ensure!(
                    header.inventory_sha256.as_deref()
                        == Some(receipt.inventory_digest.to_string().as_str())
                        && header.manifest_sha256.as_deref()
                            == Some(receipt.manifest_digest.to_string().as_str()),
                    "active receipt binding mismatch"
                );
            } else {
                ensure!(
                    header.token.is_none() && header.manifest_sha256.is_none(),
                    "legacy header contains v2 identity"
                );
            }
            Ok(header)
        })
        .transpose()
}

pub(super) fn save(directory: &Path, snapshot: &Snapshot, revision: Option<&str>) -> Result<()> {
    ensure!(
        snapshot.schema_version == SCHEMA_VERSION && snapshot.storage_format <= 1,
        "v2 evidence requires transactional publication finalizer"
    );
    save_inner(directory, snapshot, revision, snapshot, false)
}

pub(super) fn save_catalog(
    directory: &Path,
    snapshot: &Snapshot,
    revision: &str,
    source: &dyn ArtifactSource,
    reservation: Option<&crate::quota::Reservation>,
    maximum: u64,
) -> Result<()> {
    ensure!(snapshot.storage_format == 1, "compact catalog required");
    if let Some(reservation) = reservation {
        let mut database = reservation
            .open_private_sqlite(directory.join("catalog.sqlite"), maximum)
            .with_context(|| format!("admit private fact catalog with {maximum}-byte quota cap"))?;
        save_connection(database.connection_mut(), snapshot, Some(revision), source, None, None)
            .with_context(|| format!("build private fact catalog with {maximum}-byte quota cap; previous generation retained"))?;
        return database.finish().context("finish private fact catalog");
    }
    save_inner(directory, snapshot, Some(revision), source, true)?;
    std::fs::rename(
        directory.join("index.sqlite3"),
        directory.join("catalog.sqlite"),
    )?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(directory.join("catalog.sqlite"))?
        .sync_all()?;
    Ok(())
}

fn header(snapshot: &Snapshot, revision: &str) -> PersistentRevision {
    PersistentRevision {
        token: None,
        manifest_sha256: None,
        storage_format: snapshot.storage_format,
        roots: snapshot.roots.clone(),
        inventory_sha256: (snapshot.storage_format == 1).then(|| {
            use sha2::Digest;
            format!(
                "{:x}",
                sha2::Sha256::digest(
                    serde_json::to_vec(snapshot).expect("snapshot serialization is infallible")
                )
            )
        }),
        revision: revision.into(),
        root: snapshot.root.clone(),
        collection: snapshot.collection.clone(),
        generation: snapshot.generation,
        files: snapshot.artifacts.len(),
        chunks: snapshot.artifacts.values().map(Artifact::chunk_count).sum(),
        source_bytes: snapshot.artifacts.values().map(|a| a.bytes).sum(),
        extractor_version: snapshot.extractor_version.clone(),
        semantic_producer: snapshot.analysis.as_ref().map(|a| a.producer.clone()),
        imported_facts: snapshot.imported_facts.len(),
        imported_relationships: snapshot.imported_relationships.len(),
    }
}

fn raw_digest(bytes: &[u8]) -> airk_core::identity::Sha256Digest {
    use sha2::Digest;
    airk_core::identity::Sha256Digest::from_bytes(sha2::Sha256::digest(bytes).into())
}

const MAX_FILE_METADATA_BYTES: usize = airk_core::continuity::MAX_CONTINUITY_BYTES + 131_072;

fn continuity_schema(conn: &Connection) -> Result<bool> {
    let tables: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('continuity_schema','continuity_records')",
        [], |row| row.get(0))?;
    if tables == 0 {
        return Ok(false);
    }
    ensure!(tables == 2, "incomplete continuity catalog extension");
    let (count, version): (i64, i64) = conn.query_row(
        "SELECT count(*),coalesce(min(version),0) FROM continuity_schema",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        count == 1 && version == 1,
        "unsupported continuity catalog version"
    );
    Ok(true)
}

fn validate_v2_catalog(
    conn: &Connection,
    token: &airk_core::identity::RevisionToken,
    inventory_digest: &airk_core::identity::Sha256Digest,
) -> Result<()> {
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    ensure!(
        version == crate::IMMUTABLE_CATALOG_VERSION,
        "unsupported immutable catalog version"
    );
    let (stored_token, digest): (String, String) = conn.query_row(
        "SELECT token,inventory_digest FROM v2_catalog WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        stored_token == token.to_string() && digest == inventory_digest.to_string(),
        "immutable catalog identity mismatch"
    );
    continuity_schema(conn)?;
    Ok(())
}

pub(crate) fn verify_prepared_catalog(
    directory: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
) -> Result<()> {
    let conn = open_immutable_catalog(&directory.join("catalog.sqlite"))?;
    validate_v2_catalog(&conn, &receipt.token, &receipt.inventory_digest)?;
    let integrity: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    ensure!(
        integrity == "ok",
        "immutable catalog integrity check failed"
    );
    Ok(())
}

pub(crate) fn file_in_catalog(
    directory: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
    path: &str,
) -> Result<Option<airk_core::publication::InventoryFile>> {
    ensure!(path.len() <= 4096, "metadata path exceeds bound");
    let conn = open_immutable_catalog(
        &directory
            .join("generations")
            .join(receipt.token.revision.to_string())
            .join("catalog.sqlite"),
    )?;
    validate_v2_catalog(&conn, &receipt.token, &receipt.inventory_digest)?;
    let payload: Option<Option<String>> = conn
        .query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB)) <= ?2 THEN payload END FROM files WHERE path=?1",
            params![path, MAX_FILE_METADATA_BYTES as i64],
            |row| row.get(0),
        )
        .optional()?;
    payload
        .map(|payload| {
            Ok(serde_json::from_str(
                &payload.context("file metadata exceeds byte bound")?,
            )?)
        })
        .transpose()
}

pub(crate) fn file_by_artifact_in_catalog(
    directory: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
    id: airk_core::identity::ArtifactId,
) -> Result<Option<(String, airk_core::publication::InventoryFile)>> {
    let conn = open_immutable_catalog(
        &directory
            .join("generations")
            .join(receipt.token.revision.to_string())
            .join("catalog.sqlite"),
    )?;
    validate_v2_catalog(&conn, &receipt.token, &receipt.inventory_digest)?;
    let row: Option<(Option<String>, Option<String>)> = conn.query_row(
        "SELECT CASE WHEN length(CAST(path AS BLOB))<=4096 THEN path END,CASE WHEN length(CAST(payload AS BLOB))<=?2 THEN payload END FROM files WHERE artifact_id=?1",
        params![id.to_string(), MAX_FILE_METADATA_BYTES as i64], |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
    row.map(|(path, payload)| {
        Ok((
            path.context("file path exceeds byte bound")?,
            serde_json::from_str(&payload.context("file metadata exceeds byte bound")?)?,
        ))
    })
    .transpose()
}

pub(crate) fn continuity_in_catalog(
    directory: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
    key: airk_core::continuity::ContinuityKey,
) -> Result<Option<(String, airk_core::continuity::ContinuityRecord)>> {
    let conn = open_immutable_catalog(
        &directory
            .join("generations")
            .join(receipt.token.revision.to_string())
            .join("catalog.sqlite"),
    )?;
    validate_v2_catalog(&conn, &receipt.token, &receipt.inventory_digest)?;
    if !continuity_schema(&conn)? {
        return Ok(None);
    }
    let row: Option<(Option<String>, Option<String>)> = conn.query_row(
        "SELECT CASE WHEN length(CAST(path AS BLOB))<=4096 THEN path END,CASE WHEN length(CAST(record AS BLOB))<=?2 THEN record END FROM continuity_records WHERE key=?1",
        params![serde_json::to_string(&key)?, airk_core::continuity::MAX_CONTINUITY_BYTES as i64],
        |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
    row.map(|(path, record)| {
        let record: airk_core::continuity::ContinuityRecord =
            serde_json::from_str(&record.context("continuity record exceeds byte bound")?)?;
        ensure!(record.key == key, "continuity index key mismatch");
        Ok((path.context("continuity path exceeds byte bound")?, record))
    })
    .transpose()
}

fn legacy_json_head(directory: &Path) -> Result<crate::ExpectedHead> {
    use std::io::Read;
    let path = directory.join("snapshot.json");
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(crate::ExpectedHead::Empty);
        }
        Err(error) => return Err(error.into()),
    };
    ensure!(
        file.metadata()?.len() <= 536_870_912,
        "legacy snapshot exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take(536_870_913).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 536_870_912, "legacy snapshot exceeds bound");
    Ok(crate::ExpectedHead::Legacy(
        airk_core::identity::framed_digest(b"legacy-json-head/1", &[&bytes]),
    ))
}

fn expected_head_connection(directory: &Path, conn: &Connection) -> Result<crate::ExpectedHead> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='revision')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return legacy_json_head(directory);
    }
    let payload: Option<String> = conn.query_row("SELECT payload FROM revision WHERE id=1 AND length(CAST(payload AS BLOB)) <= 536870912", [], |row| row.get(0)).optional()?;
    let Some(payload) = payload else {
        anyhow::bail!("invalid or oversized committed revision");
    };
    let has_components: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='index_components')",
        [],
        |row| row.get(0),
    )?;
    let components: Option<String> = if has_components {
        conn.query_row("SELECT payload FROM index_components WHERE id=1 AND length(CAST(payload AS BLOB))<=131072", [], |row| row.get(0)).optional()?
    } else {
        None
    };
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if is_v2_root(version) {
        let header: PersistentRevision =
            serde_json::from_str(components.as_deref().context("v2 head missing")?)?;
        return Ok(crate::ExpectedHead::V2(
            header.token.context("v2 token missing")?,
        ));
    }
    ensure!(version <= 2, "unsupported root version");
    Ok(crate::ExpectedHead::Legacy(
        airk_core::identity::framed_digest(
            b"legacy-sqlite-head/1",
            &[
                &version.to_be_bytes(),
                payload.as_bytes(),
                components.as_deref().unwrap_or("").as_bytes(),
            ],
        ),
    ))
}

pub(super) fn expected_head(directory: &Path) -> Result<crate::ExpectedHead> {
    if !directory.join("index.sqlite3").exists() {
        return legacy_json_head(directory);
    }
    let conn = Connection::open_with_flags(
        directory.join("index.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    conn.execute_batch("BEGIN DEFERRED")?;
    expected_head_connection(directory, &conn)
}

fn receipt_connection(
    conn: &Connection,
    token: &airk_core::identity::RevisionToken,
) -> Result<Option<airk_core::identity::PublicationReceipt>> {
    let value: Option<String> = conn.query_row("SELECT payload FROM publication_receipts WHERE revision=?1 AND incarnation=?2 AND generation=?3 AND length(CAST(payload AS BLOB))<=4096", params![token.revision.to_string(),token.incarnation.to_string(),i64::try_from(token.generation)?], |row| row.get(0)).optional()?;
    value
        .map(|value| {
            let receipt: airk_core::identity::PublicationReceipt = serde_json::from_str(&value)?;
            ensure!(
                receipt.token == *token,
                "retained receipt identity mismatch"
            );
            Ok(receipt)
        })
        .transpose()
}

pub(super) fn receipt(
    directory: &Path,
    token: &airk_core::identity::RevisionToken,
) -> Result<Option<airk_core::identity::PublicationReceipt>> {
    retry_wal_protocol(|| receipt_once(directory, token))
}

pub(crate) fn validate_retained_token(
    directory: &Path,
    token: &airk_core::identity::RevisionToken,
) -> Result<()> {
    retry_wal_protocol(|| {
        let conn = connect_root_once(directory, false)?;
        let mut statement = conn.prepare("SELECT revision FROM publication_receipts WHERE incarnation=?1 AND generation=?2 LIMIT 2")?;
        let revisions = statement
            .query_map(
                params![
                    token.incarnation.to_string(),
                    i64::try_from(token.generation)?
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(
            revisions.len() <= 1
                && revisions
                    .first()
                    .is_none_or(|revision| revision == &token.revision.to_string()),
            "forked retained revision token"
        );
        if table_exists(&conn, "retired_representations")? {
            let mut statement=conn.prepare("SELECT revision FROM retired_representations WHERE incarnation=?1 AND generation=?2 LIMIT 2")?;
            let retired = statement
                .query_map(
                    params![
                        token.incarnation.to_string(),
                        i64::try_from(token.generation)?
                    ],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ensure!(
                retired.len() <= 1
                    && retired
                        .first()
                        .is_none_or(|revision| revision == &token.revision.to_string()),
                "forked retired revision token"
            );
        }
        Ok(())
    })
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |row| row.get(0),
    )?)
}

pub(crate) fn maintenance_receipts(
    directory: &Path,
    after_generation: Option<u64>,
    limit: usize,
) -> Result<Vec<airk_core::identity::PublicationReceipt>> {
    let conn = connect_root(directory, false)?;
    let mut statement = conn.prepare(
        "SELECT payload FROM publication_receipts WHERE ?1 IS NULL OR generation>?1 ORDER BY generation ASC,revision ASC LIMIT ?2",
    )?;
    let rows = statement
        .query_map(
            params![
                after_generation.map(i64::try_from).transpose()?,
                i64::try_from(limit)?
            ],
            |row| row.get::<_, String>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|row| {
            ensure!(row.len() <= 16384, "oversized publication receipt");
            Ok(serde_json::from_str(&row)?)
        })
        .collect()
}

pub(crate) fn retirement_proof(
    directory: &Path,
    token: &airk_core::identity::RevisionToken,
) -> Result<Option<crate::maintenance::RetirementProof>> {
    let conn = connect_root(directory, false)?;
    if !table_exists(&conn, "retired_representations")? {
        return Ok(None);
    }
    let row: Option<String> = conn
        .query_row(
            "SELECT proof FROM retired_representations WHERE revision=?1",
            [token.revision.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    row.map(|row| {
        ensure!(row.len() <= 16384, "oversized retirement proof");
        let proof: crate::maintenance::RetirementProof = serde_json::from_str(&row)?;
        ensure!(proof.token == *token, "foreign retirement proof");
        Ok(proof)
    })
    .transpose()
}

pub(crate) fn authorize_retirement(
    directory: &Path,
    proof: &crate::maintenance::RetirementProof,
    expected: &airk_core::identity::PublicationReceipt,
    reservation: &crate::quota::Reservation,
) -> Result<()> {
    authorize_representation_retirement(directory, proof, expected, reservation, None)
}

pub(crate) fn retire_history_binding(
    directory: &Path,
    proof: &crate::maintenance::RetirementProof,
    binding: &crate::history::HistoryBinding,
    reservation: &crate::quota::Reservation,
) -> Result<()> {
    authorize_representation_retirement(
        directory,
        proof,
        &binding.publication,
        reservation,
        Some(binding),
    )
}

fn authorize_representation_retirement(
    directory: &Path,
    proof: &crate::maintenance::RetirementProof,
    expected: &airk_core::identity::PublicationReceipt,
    reservation: &crate::quota::Reservation,
    history: Option<&crate::history::HistoryBinding>,
) -> Result<()> {
    let (maximum, allowance) = pointer_budget(directory)?;
    reservation.charge(allowance)?;
    let mut conn = connect_root(directory, true)?;
    conn.execute_batch(&format!(
        "PRAGMA max_page_count={maximum}; PRAGMA cache_spill=OFF; PRAGMA temp_store=MEMORY;"
    ))?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    ensure!(
        !history_admin_connection(directory, &tx)?
            .checkpoints
            .values()
            .any(|pin| pin.publication.token == proof.token),
        "checkpoint publication cannot be retired"
    );
    ensure!(
        expected_head_connection(directory, &tx)? == crate::ExpectedHead::V2(proof.authorized_head),
        "pointer changed before retirement authorization"
    );
    ensure!(
        proof.token != proof.authorized_head
            && proof.token.incarnation == proof.authorized_head.incarnation,
        "invalid retirement scope"
    );
    ensure!(
        receipt_connection(&tx, &proof.token)?.as_ref() == Some(expected),
        "retirement receipt changed"
    );
    if table_exists(&tx, "history_bindings")? {
        let retained: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM history_bindings WHERE revision=?1)",
            [proof.token.revision.to_string()],
            |row| row.get(0),
        )?;
        if let Some(binding) = history {
            ensure!(retained, "historical binding missing before retirement");
            let encoded: String = tx.query_row("SELECT CAST(substr(CAST(payload AS BLOB),1,4097) AS TEXT) FROM history_bindings WHERE revision=?1", [proof.token.revision.to_string()], |row| row.get(0))?;
            ensure!(
                encoded == serde_json::to_string(binding)?,
                "historical binding changed before retirement"
            );
            tx.execute(
                "DELETE FROM history_bindings WHERE revision=?1",
                [proof.token.revision.to_string()],
            )?;
        } else {
            ensure!(
                !retained,
                "historical publication receipt cannot be retired by full-generation cleanup"
            );
        }
    } else {
        ensure!(history.is_none(), "historical binding table missing");
    }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS retired_representations(revision TEXT PRIMARY KEY,incarnation TEXT NOT NULL,generation INTEGER NOT NULL,proof TEXT NOT NULL); CREATE INDEX IF NOT EXISTS retired_scope ON retired_representations(incarnation,generation); CREATE TABLE IF NOT EXISTS retirement_floors(incarnation TEXT PRIMARY KEY,generation INTEGER NOT NULL);")?;
    let encoded = serde_json::to_string(proof)?;
    ensure!(encoded.len() <= 16384, "retirement proof exceeds bound");
    tx.execute("INSERT INTO retired_representations(revision,incarnation,generation,proof) VALUES(?1,?2,?3,?4)",params![proof.token.revision.to_string(),proof.token.incarnation.to_string(),i64::try_from(proof.token.generation)?,encoded])?;
    tx.execute(
        "DELETE FROM publication_receipts WHERE revision=?1",
        [proof.token.revision.to_string()],
    )?;
    // Proofs for unfinished intents cannot be evicted. Select only completed
    // rows whose intent file is absent; old identities then become unverifiable.
    loop {
        let (count, bytes): (i64, i64) = tx.query_row(
            "SELECT count(*),coalesce(sum(length(proof)),0) FROM retired_representations",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        ensure!(count >= 0 && bytes >= 0, "negative retirement accounting");
        if count <= 10_000 && bytes <= 16 * 1024 * 1024 {
            break;
        }
        let oldest:Option<(String,String,i64)>=tx.query_row("SELECT revision,incarnation,generation FROM retired_representations ORDER BY generation,revision LIMIT 1",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
        let (revision, incarnation, generation) =
            oldest.context("retirement proof accounting mismatch")?;
        ensure!(
            !directory
                .join("maintenance/intents")
                .join(format!("{revision}.json"))
                .exists()
                && !directory
                    .join("maintenance/history-retirements")
                    .join(format!("{revision}.json"))
                    .exists(),
            "retirement proof cap protected by unfinished maintenance"
        );
        tx.execute("INSERT INTO retirement_floors(incarnation,generation) VALUES(?1,?2) ON CONFLICT(incarnation) DO UPDATE SET generation=max(generation,excluded.generation)",params![incarnation,generation])?;
        tx.execute(
            "DELETE FROM retired_representations WHERE revision=?1",
            [revision],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn receipt_once(
    directory: &Path,
    token: &airk_core::identity::RevisionToken,
) -> Result<Option<airk_core::identity::PublicationReceipt>> {
    let conn = connect_root_once(directory, false)?;
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if !is_v2_root(version) {
        return Ok(None);
    }
    receipt_connection(&conn, token)
}

/// Legacy pruning has no v2 receipt authority. Only durable retirement may remove it.
pub(crate) fn retire_unrepresented_receipts(directory: &Path) -> Result<usize> {
    let conn = connect_root(directory, false)?;
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    ensure!(
        !is_v2_root(version),
        "v2 receipt retirement requires durable maintenance authorization"
    );
    Ok(0)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn save_catalog_v2(
    directory: &Path,
    inventory: &airk_core::publication::InventoryV2,
    lineage: &airk_core::changes::LineageEnvelope,
    token: &airk_core::identity::RevisionToken,
    source: &dyn ArtifactSource,
    reservation: Option<&crate::quota::Reservation>,
    maximum: u64,
    base_directory: &Path,
    previous: Option<&crate::blobs::Catalog>,
) -> Result<()> {
    use std::io::Write;
    inventory.validate()?;
    lineage.validate()?;
    let bytes = inventory.canonical_bytes()?;
    ensure!(
        token.incarnation == inventory.incarnation
            && token.generation == inventory.generation
            && token.revision
                == airk_core::identity::revision_digest(
                    inventory.retrieval_specification.as_bytes(),
                    &bytes
                ),
        "catalog token mismatch"
    );
    let lineage_bytes = serde_json::to_vec(lineage)?;
    ensure!(
        raw_digest(&lineage_bytes) == inventory.lineage_digest
            && lineage.incarnation == inventory.incarnation
            && lineage.observation.generation == inventory.generation,
        "catalog lineage mismatch"
    );
    ensure!(
        reservation.is_some(),
        "v2 catalog requires a charged reservation"
    );
    let reservation = reservation.context("reservation missing")?;
    let mut lineage_file = reservation.create_file(directory.join("lineage.json"))?;
    lineage_file.write_all(&lineage_bytes)?;
    lineage_file.sync_all()?;
    drop(lineage_file);
    let mut database =
        reservation.open_private_sqlite(directory.join("catalog.sqlite"), maximum)?;
    let reuse = reuse_catalog(base_directory, previous, inventory)?;
    let written = (|| -> Result<()> {
        let snapshot = inventory.projection()?;
        save_connection(
            database.connection_mut(),
            &snapshot,
            Some(&token.revision.to_string()),
            source,
            Some(inventory),
            reuse
                .as_ref()
                .zip(previous.and_then(|c| c.inventory.as_ref())),
        )?;
        let conn = database.connection_mut();
        let tx = conn.transaction()?;
        tx.execute_batch("CREATE TABLE v2_catalog(id INTEGER PRIMARY KEY CHECK(id=1),token TEXT NOT NULL,inventory_digest TEXT NOT NULL,lineage_digest TEXT NOT NULL); CREATE TABLE files(path TEXT PRIMARY KEY,artifact_id TEXT NOT NULL UNIQUE,source_hash TEXT NOT NULL,profile_id TEXT NOT NULL,payload TEXT NOT NULL); CREATE INDEX files_source_hash ON files(source_hash); CREATE INDEX files_profile ON files(profile_id); CREATE TABLE change_events(generation INTEGER NOT NULL,event_id INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(generation,event_id)); CREATE TABLE revision_headers(generation INTEGER PRIMARY KEY,token TEXT NOT NULL,payload TEXT NOT NULL); CREATE TABLE path_resolutions(artifact_id TEXT NOT NULL,path TEXT NOT NULL,payload TEXT NOT NULL); CREATE INDEX resolutions_path ON path_resolutions(path); CREATE INDEX resolutions_artifact ON path_resolutions(artifact_id);")?;
        tx.execute_batch("CREATE TABLE continuity_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL); INSERT INTO continuity_schema VALUES(1,1); CREATE TABLE continuity_records(key TEXT PRIMARY KEY,path TEXT NOT NULL,record TEXT NOT NULL); CREATE INDEX continuity_path ON continuity_records(path);")?;
        tx.execute(
            "INSERT INTO v2_catalog VALUES(1,?1,?2,?3)",
            params![
                token.to_string(),
                raw_digest(&bytes).to_string(),
                inventory.lineage_digest.to_string()
            ],
        )?;
        for (path, file) in &inventory.files {
            let payload = serde_json::to_string(file)?;
            ensure!(
                payload.len() <= MAX_FILE_METADATA_BYTES,
                "file metadata exceeds byte bound"
            );
            tx.execute(
                "INSERT INTO files VALUES(?1,?2,?3,?4,?5)",
                params![
                    path,
                    file.state.artifact_id.to_string(),
                    file.state.source_hash.to_string(),
                    file.state.profile_id.to_string(),
                    payload
                ],
            )?;
            for record in &file.continuity {
                tx.execute(
                    "INSERT INTO continuity_records VALUES(?1,?2,?3)",
                    params![
                        serde_json::to_string(&record.key)?,
                        path,
                        serde_json::to_string(record)?
                    ],
                )?;
            }
        }
        for event in &lineage.events {
            tx.execute(
                "INSERT INTO change_events VALUES(?1,?2,?3)",
                params![
                    i64::try_from(event.generation)?,
                    i64::try_from(event.event_id)?,
                    serde_json::to_string(event)?
                ],
            )?;
        }
        for header in &lineage.ancestry {
            tx.execute(
                "INSERT INTO revision_headers VALUES(?1,?2,?3)",
                params![
                    i64::try_from(header.token.generation)?,
                    header.token.to_string(),
                    serde_json::to_string(header)?
                ],
            )?;
        }
        for resolver in &lineage.resolvers {
            tx.execute(
                "INSERT INTO path_resolutions VALUES(?1,?2,?3)",
                params![
                    resolver.artifact_id.to_string(),
                    resolver.from.path,
                    serde_json::to_string(resolver)?
                ],
            )?;
        }
        tx.pragma_update(None, "user_version", crate::IMMUTABLE_CATALOG_VERSION)?;
        tx.commit()?;
        Ok(())
    })();
    if let Err(error) = written {
        return Err(database.write_error(error));
    }
    // Authenticate again after row reuse; do not publish a copy of changed base bytes.
    if reuse.is_some() {
        crate::blobs::verify_v2_for_query(
            base_directory,
            previous
                .and_then(|c| c.receipt.as_ref())
                .context("reuse receipt missing")?,
        )?;
    }
    database.finish()?;
    Ok(())
}

pub(super) fn publish_catalog_v2(
    directory: &Path,
    inventory: &airk_core::publication::InventoryV2,
    receipt: &airk_core::identity::PublicationReceipt,
    expected: &crate::ExpectedHead,
    reservation: &crate::quota::Reservation,
) -> Result<()> {
    inventory.validate()?;
    let bytes = inventory.canonical_bytes()?;
    let token = receipt.token;
    ensure!(
        token.incarnation == inventory.incarnation
            && token.generation == inventory.generation
            && token.revision
                == airk_core::identity::revision_digest(
                    inventory.retrieval_specification.as_bytes(),
                    &bytes
                )
            && receipt.inventory_digest == raw_digest(&bytes),
        "publication receipt does not bind inventory"
    );
    let prepared = crate::blobs::verify_v2_generation(directory, receipt)?;
    ensure!(
        prepared.canonical_bytes()? == bytes,
        "prepared inventory differs from publication candidate"
    );
    let lineage_bytes = crate::blobs::read_verified(
        &directory
            .join("generations")
            .join(token.revision.to_string())
            .join("lineage.json"),
        &inventory.lineage_digest.to_string(),
        airk_core::changes::MAX_LINEAGE_BYTES as u64,
    )?;
    let lineage: airk_core::changes::LineageEnvelope = serde_json::from_slice(&lineage_bytes)?;
    lineage.validate()?;
    ensure!(
        lineage.incarnation == token.incarnation
            && lineage.observation.generation == token.generation,
        "prepared lineage identity mismatch"
    );
    let (maximum, allowance) = pointer_budget(directory)?;
    reservation.charge(allowance)?;
    // The first SQLite pointer is prepared privately. Creating an empty root DB
    // would otherwise shadow a valid legacy JSON snapshot after a failed transaction.
    let private_root = if !directory.join("index.sqlite3").exists() {
        Some(
            tempfile::Builder::new()
                .prefix("pointer-")
                .tempdir_in(reservation.staging_directory()?)?,
        )
    } else {
        None
    };
    let pointer_directory = private_root
        .as_ref()
        .map_or(directory, |temporary| temporary.path());
    let mut conn = connect_root(pointer_directory, true)?;
    conn.execute_batch(&format!(
        "PRAGMA max_page_count={maximum}; PRAGMA cache_spill=OFF; PRAGMA temp_store=MEMORY;"
    ))?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    ensure!(
        &expected_head_connection(directory, &tx)? == expected,
        "publication parent changed"
    );
    if let crate::ExpectedHead::V2(parent) = expected {
        ensure!(
            parent.incarnation == token.incarnation
                && parent.next_generation()? == token.generation,
            "publication sequence/incarnation mismatch"
        );
        ensure!(
            lineage.observation.parent.as_ref() == Some(parent),
            "prepared lineage parent does not match committed parent"
        );
    } else {
        ensure!(
            lineage.observation.parent.is_none(),
            "first v2 observation cannot invent an earlier v2 parent"
        );
    }
    if !matches!(expected, crate::ExpectedHead::Empty) {
        let has_revision: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='revision')",
            [],
            |row| row.get(0),
        )?;
        let (old_root, old_collection, old_generation) = if has_revision {
            let (root, collection, generation): (String, String, i64) = tx.query_row(
                "SELECT root,collection,generation FROM revision WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            (root, collection, u64::try_from(generation)?)
        } else {
            let snapshot: Snapshot =
                serde_json::from_reader(std::fs::File::open(directory.join("snapshot.json"))?)?;
            (snapshot.root, snapshot.collection, snapshot.generation)
        };
        ensure!(
            old_root == inventory.root
                && old_collection == inventory.collection
                && old_generation.checked_add(1) == Some(token.generation),
            "publication changes collection binding or skips sequence"
        );
    }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS revision(id INTEGER PRIMARY KEY CHECK(id=1),root TEXT NOT NULL,collection TEXT NOT NULL,generation INTEGER NOT NULL,schema_version INTEGER NOT NULL,payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS index_components(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS publication_receipts(revision TEXT PRIMARY KEY,incarnation TEXT NOT NULL,generation INTEGER NOT NULL,payload TEXT NOT NULL); CREATE INDEX IF NOT EXISTS receipts_scope ON publication_receipts(incarnation,generation);")?;
    let mut header = header(&inventory.projection()?, &token.revision.to_string());
    header.token = Some(token);
    header.inventory_sha256 = Some(receipt.inventory_digest.to_string());
    header.manifest_sha256 = Some(receipt.manifest_digest.to_string());
    let header_payload = serde_json::to_string(&header)?;
    ensure!(
        header_payload.len() <= 131_072,
        "active v2 pointer exceeds 128 KiB"
    );
    tx.execute(
        "INSERT INTO publication_receipts VALUES(?1,?2,?3,?4)",
        params![
            token.revision.to_string(),
            token.incarnation.to_string(),
            i64::try_from(token.generation)?,
            serde_json::to_string(receipt)?
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO revision VALUES(1,?1,?2,?3,4,?4)",
        params![
            inventory.root,
            inventory.collection,
            i64::try_from(inventory.generation)?,
            header_payload
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO index_components VALUES(1,?1)",
        [&header_payload],
    )?;
    history_schema(&tx)?;
    tx.pragma_update(None, "user_version", publication_root_version(&tx)?)?;
    tx.commit()?;
    if let Some(private_root) = private_root {
        let (busy, _, _): (i64, i64, i64) =
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        ensure!(busy == 0, "private pointer checkpoint remained busy");
        conn.close().map_err(|(_, error)| error)?;
        let staged = private_root.path().join("index.sqlite3").canonicalize()?;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&staged)?
            .sync_all()?;
        let target = directory.canonicalize()?.join("index.sqlite3");
        tempfile::TempPath::try_from_path(staged)?.persist_noclobber(target)?;
    }
    Ok(())
}

pub(super) fn catalog_page_budget(reserved: u64, physical: u64, pointer: u64) -> Result<u64> {
    let required = pointer
        .checked_add(16_384)
        .context("minimum catalog and pointer allowance overflow")?;
    if reserved < required {
        return Err(crate::quota::QuotaPressure {
            scope: crate::quota::PressureScope::Reservation,
            required_bytes: required,
            available_bytes: reserved,
            protected_bytes: None,
        }
        .into());
    }
    // SQLite's maximum representable page count is a format limit, not an
    // application catalog ceiling. Logical policy and observed capacity govern
    // every smaller admission.
    // This observation precedes lineage writes and the quota charge's fresh
    // capacity probe. Do not admit virtually all free space: concurrent writes
    // on the same filesystem would make even a tiny catalog fail admission.
    // Keep 10% of observed growth available in addition to OS safety headroom.
    // It remains a point-in-time bound, not a physical reservation.
    let physical_admission = physical.saturating_sub(physical / 10);
    let maximum = (reserved.min(physical_admission).saturating_sub(pointer) / 4096)
        .min(u64::from(u32::MAX) - 1)
        * 4096;
    ensure!(
        maximum >= 16_384,
        "private catalog admission exhausted: {reserved} reserved bytes and {physical} physical growth bytes remain, {pointer} needed for active pointer publication"
    );
    Ok(maximum)
}

pub(super) fn pointer_budget(directory: &Path) -> Result<(u64, u64)> {
    let (page_size, pages) = if directory.join("index.sqlite3").exists() {
        let conn = connect_root(directory, false)?;
        let page_size: u32 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let pages: u32 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        (u64::from(page_size), u64::from(pages))
    } else {
        (4096, 0)
    };
    let maximum = pages
        .checked_add(128)
        .context("pointer page bound overflow")?
        .max(128);
    // page_count includes pages which may exist only in the WAL. A checkpoint
    // can grow the main file by that entire difference, including during legacy
    // migration. Count physical file lengths separately from logical page count.
    let file_bytes = |name: &str| -> Result<u64> {
        match std::fs::metadata(directory.join(name)) {
            Ok(metadata) => Ok(metadata.len()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(error) => Err(error.into()),
        }
    };
    let database_growth = maximum
        .checked_mul(page_size)
        .context("pointer database bound overflow")?
        .saturating_sub(file_bytes("index.sqlite3")?);
    // With spill disabled, one transaction appends each dirty page at most once.
    // WAL frames use a 24-byte header; allow 32 for conservative framing. The
    // shared-memory index uses 32 KiB per roughly 4096 frames (8 bytes/frame),
    // plus headers and allocation rounding. Existing WAL frames matter too.
    // Reserve the full SHM allocation: a reader can close and remove sidecars
    // between admission and publication, requiring their recreation.
    let existing_frames = file_bytes("index.sqlite3-wal")? / (page_size + 24);
    let shared_growth = existing_frames
        .checked_add(maximum)
        .and_then(|frames| frames.checked_mul(8))
        .and_then(|bytes| bytes.checked_add(65_536))
        .context("pointer shared-memory bound overflow")?;
    let allowance = maximum
        .checked_mul(page_size + 32)
        .and_then(|value| value.checked_add(database_growth))
        .and_then(|value| value.checked_add(shared_growth))
        .and_then(|value| value.checked_add(32))
        .context("pointer journal bound overflow")?;
    Ok((maximum, allowance))
}

pub(super) fn publish_catalog(
    directory: &Path,
    snapshot: &Snapshot,
    revision: &str,
    reservation: Option<&crate::quota::Reservation>,
) -> Result<()> {
    ensure!(
        snapshot.storage_format == 1 && crate::blobs::valid_digest(revision),
        "invalid compact catalog publication"
    );
    let target = directory
        .join("generations")
        .join(revision)
        .join("catalog.sqlite");
    ensure!(target.is_file(), "prepared immutable catalog missing");
    let (maximum, allowance) = pointer_budget(directory)?;
    if let Some(reservation) = reservation {
        reservation.charge(allowance).with_context(|| {
            format!("admit active catalog pointer with {allowance}-byte main/WAL/SHM allowance")
        })?;
    }
    publish_pointer(directory, snapshot, revision, maximum)
        .with_context(|| format!("publish active catalog pointer with {maximum}-page cap and {allowance}-byte write allowance"))
}

fn publish_pointer(
    directory: &Path,
    snapshot: &Snapshot,
    revision: &str,
    maximum: u64,
) -> Result<()> {
    let mut conn = connect_root(directory, true)?;
    conn.execute_batch(&format!(
        "PRAGMA max_page_count={maximum}; PRAGMA cache_spill=OFF; PRAGMA temp_store=MEMORY;"
    ))?;
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(version <= 2, "unsupported SQLite schema");
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS revision(id INTEGER PRIMARY KEY CHECK(id=1),root TEXT NOT NULL,collection TEXT NOT NULL,generation INTEGER NOT NULL,schema_version INTEGER NOT NULL,payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS index_components(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);")?;
    let mut pointer = Snapshot::empty(snapshot.root.clone());
    pointer.collection = snapshot.collection.clone();
    pointer.roots = snapshot.roots.clone();
    pointer.generation = snapshot.generation;
    pointer.storage_format = 1;
    pointer.persistent_indexes = true;
    pointer.embedding_model = snapshot.embedding_model.clone();
    pointer.extractor_version = snapshot.extractor_version.clone();
    let payload = serde_json::to_string(&pointer)?;
    ensure!(
        payload.len() <= 131_072,
        "active pointer metadata exceeds 128 KiB"
    );
    tx.execute(
        "INSERT OR REPLACE INTO revision VALUES(1,?1,?2,?3,?4,?5)",
        params![
            snapshot.root,
            snapshot.collection,
            i64::try_from(snapshot.generation)?,
            snapshot.schema_version,
            payload
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO index_components VALUES(1,?1)",
        [serde_json::to_string(&header(snapshot, revision))?],
    )?;
    tx.execute_batch("PRAGMA user_version=2;")?;
    tx.commit()?;
    Ok(())
}

fn save_inner(
    directory: &Path,
    snapshot: &Snapshot,
    revision: Option<&str>,
    source: &dyn ArtifactSource,
    private: bool,
) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    let mut conn = connect(directory, true)?;
    if private {
        conn.execute_batch(
            "PRAGMA journal_mode=OFF; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-16384; PRAGMA cache_spill=ON;",
        )?;
    }
    save_connection(&mut conn, snapshot, revision, source, None, None)
}

fn save_connection(
    conn: &mut Connection,
    snapshot: &Snapshot,
    revision: Option<&str>,
    source: &dyn ArtifactSource,
    inventory: Option<&airk_core::publication::InventoryV2>,
    reuse: Option<(&Connection, &airk_core::publication::InventoryV2)>,
) -> Result<()> {
    let payload = serde_json::to_string(snapshot)?;
    ensure!(
        payload.len()
            <= if snapshot.storage_format == 1 {
                crate::blobs::MAX_CATALOG_BYTES as usize
            } else {
                536_870_912
            },
        "catalog payload exceeds byte bound; previous generation retained"
    );
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(version <= 2, "unsupported SQLite schema");
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS revision(id INTEGER PRIMARY KEY CHECK(id=1), root TEXT NOT NULL, collection TEXT NOT NULL, generation INTEGER NOT NULL, schema_version INTEGER NOT NULL, payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS index_components(id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL); DELETE FROM index_components; PRAGMA user_version=1;")?;
    if inventory.is_some() {
        crate::normalized::create(&tx)?;
    } else {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS facts(path TEXT NOT NULL,name TEXT NOT NULL,folded_name TEXT NOT NULL,qualified_name TEXT NOT NULL,symbol_id TEXT NOT NULL,kind TEXT NOT NULL,language TEXT NOT NULL,is_code INTEGER NOT NULL,is_document INTEGER NOT NULL,line INTEGER NOT NULL,payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS relationships(source TEXT NOT NULL,target TEXT NOT NULL,kind TEXT NOT NULL,path TEXT NOT NULL,line INTEGER NOT NULL,payload TEXT NOT NULL); DELETE FROM facts; DELETE FROM relationships;")?;
    }
    if let Some(revision) = revision {
        let header = header(snapshot, revision);
        tx.execute(
            "INSERT INTO index_components VALUES(1,?1)",
            [serde_json::to_string(&header)?],
        )?;
    }
    tx.execute(
        "INSERT OR REPLACE INTO revision VALUES(1,?1,?2,?3,?4,?5)",
        params![
            snapshot.root,
            snapshot.collection,
            i64::try_from(snapshot.generation)?,
            snapshot.schema_version,
            payload
        ],
    )?;
    {
        let mut writer = inventory.map(|_| crate::normalized::Writer::new(&tx));
        for path in snapshot.artifacts.keys() {
            if let (Some((base, old)), Some(next), Some(writer)) =
                (reuse, inventory, writer.as_mut())
                && old.files.get(path) == next.files.get(path)
                && old.files.contains_key(path)
            {
                writer.copy_native(base, path, &next.files[path].state.source_hash.to_string())?;
                continue;
            }
            let a = source.load_artifact(path)?;
            let keys = airk_core::continuity::FactKeyIndex::new(
                inventory
                    .and_then(|inventory| inventory.files.get(path))
                    .map_or(&[], |file| file.continuity.as_slice()),
            );
            for (ordinal, f) in a.facts.iter().enumerate() {
                insert_hit(
                    &tx,
                    &mut writer,
                    &FactHit {
                        continuity_key: keys.key(ordinal, f),
                        name: f.name.clone(),
                        kind: f.kind.clone(),
                        syntax_kind: f.syntax_kind.clone(),
                        language: a.language.clone(),
                        document: f.document.clone(),
                        code: f.code.clone(),
                        citation: Citation {
                            path: a.path.clone(),
                            content_hash: a.content_hash.clone(),
                            start_line: f.start_line,
                            end_line: f.end_line,
                            location: f.location.clone(),
                        },
                    },
                    false,
                )?;
            }
        }
        for f in &snapshot.imported_facts {
            let mut imported = f.clone();
            imported.continuity_key = None;
            insert_hit(&tx, &mut writer, &imported, true)?;
        }

        for edge in &snapshot.imported_relationships {
            write_edge(&tx, &mut writer, edge, true)?;
        }
    }
    if inventory.is_some() {
        crate::normalized::indexes(&tx)?;
    } else {
        // Build lookup B-trees once after the private bulk load; existing legacy
        // indexes are retained. All indexes commit with the same evidence rows.
        tx.execute_batch("CREATE INDEX IF NOT EXISTS facts_name ON facts(name,kind); CREATE INDEX IF NOT EXISTS facts_qualified ON facts(qualified_name); CREATE INDEX IF NOT EXISTS facts_symbol ON facts(symbol_id); CREATE INDEX IF NOT EXISTS facts_path ON facts(path,line); CREATE INDEX IF NOT EXISTS edges_source ON relationships(source,kind); CREATE INDEX IF NOT EXISTS edges_target ON relationships(target,kind);")?;
    }
    tx.commit()?;
    Ok(())
}

fn reuse_catalog(
    directory: &Path,
    previous: Option<&crate::blobs::Catalog>,
    next: &airk_core::publication::InventoryV2,
) -> Result<Option<Connection>> {
    let Some(previous) = previous else {
        return Ok(None);
    };
    let (Some(inventory), Some(receipt)) = (&previous.inventory, &previous.receipt) else {
        return Ok(None);
    };
    ensure!(
        receipt_connection(&connect_root(directory, false)?, &receipt.token)?.as_ref()
            == Some(receipt),
        "reuse requires a committed local receipt"
    );
    ensure!(
        raw_digest(&inventory.canonical_bytes()?) == receipt.inventory_digest,
        "reuse inventory changed after authentication"
    );
    if inventory.incarnation != next.incarnation
        || inventory.collection != next.collection
        || inventory.root != next.root
        || inventory.roots != next.roots
        || inventory.extractor_version != next.extractor_version
    {
        return Ok(None);
    }
    crate::blobs::verify_v2_for_query(directory, receipt)?;
    let conn = open_immutable_catalog(
        &directory
            .join("generations")
            .join(receipt.token.revision.to_string())
            .join("catalog.sqlite"),
    )?;
    if !crate::normalized::present(&conn)? {
        return Ok(None);
    }
    validate_v2_catalog(&conn, &receipt.token, &receipt.inventory_digest)?;
    Ok(Some(conn))
}

fn insert_hit(
    tx: &rusqlite::Transaction<'_>,
    writer: &mut Option<crate::normalized::Writer<'_>>,
    hit: &FactHit,
    imported: bool,
) -> Result<()> {
    let code = hit.code.as_ref();
    if let Some(writer) = writer {
        writer.fact(hit, imported)?;
    } else {
        tx.prepare_cached("INSERT INTO facts VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)")?
            .execute(params![
                hit.citation.path,
                hit.name,
                hit.name.to_lowercase(),
                code.map_or("", |c| c.qualified_name.as_str()),
                code.map_or("", |c| c.symbol_id.as_str()),
                hit.kind,
                hit.language,
                code.is_some(),
                code.is_none() && !matches!(hit.kind.as_str(), "definition" | "import"),
                i64::try_from(hit.citation.start_line)?,
                serde_json::to_string(hit)?
            ])?;
    }
    if let Some(c) = code {
        if let Some(owner) = &c.owner_id {
            write_edge(
                tx,
                writer,
                &RelationshipHit {
                    source: owner.clone(),
                    target: c.symbol_id.clone(),
                    kind: "contains".into(),
                    resolution: "syntax_candidate".into(),
                    basis: EvidenceBasis::Syntax,
                    producer: "airk-syntax-containment".into(),
                    producer_version: "native-facts/4".into(),
                    citation: hit.citation.clone(),
                },
                imported,
            )?;
        }
        if let Some(target) = &c.target {
            write_edge(
                tx,
                writer,
                &RelationshipHit {
                    source: c
                        .owner_id
                        .clone()
                        .unwrap_or_else(|| format!("file:{}", hit.citation.path)),
                    target: target.clone(),
                    kind: hit.kind.clone(),
                    resolution: "syntax_candidate".into(),
                    basis: c.basis.clone(),
                    producer: c.producer.clone(),
                    producer_version: c.producer_version.clone(),
                    citation: hit.citation.clone(),
                },
                imported,
            )?;
        }
    }
    Ok(())
}

fn write_edge(
    tx: &rusqlite::Transaction<'_>,
    writer: &mut Option<crate::normalized::Writer<'_>>,
    edge: &RelationshipHit,
    imported: bool,
) -> Result<()> {
    if let Some(writer) = writer {
        writer.edge(edge, imported)
    } else {
        let mut statement =
            tx.prepare_cached("INSERT INTO relationships VALUES(?1,?2,?3,?4,?5,?6)")?;
        insert_edge(&mut statement, edge)
    }
}

fn insert_edge(stmt: &mut rusqlite::Statement<'_>, edge: &RelationshipHit) -> Result<()> {
    stmt.execute(params![
        edge.source,
        edge.target,
        edge.kind,
        edge.citation.path,
        i64::try_from(edge.citation.start_line)?,
        serde_json::to_string(edge)?
    ])?;
    Ok(())
}

fn identity(conn: &Connection, root: &str, collection: &str) -> Result<u64> {
    let (stored_root, stored_collection, generation, schema): (String, String, i64, u32) = conn
        .query_row(
            "SELECT root,collection,generation,schema_version FROM revision WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
    ensure!(
        stored_root == root && stored_collection == collection,
        "index belongs to another root or collection"
    );
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    ensure!(
        schema == SCHEMA_VERSION || (schema == 4 && version == crate::IMMUTABLE_CATALOG_VERSION),
        "unsupported snapshot schema"
    );
    Ok(u64::try_from(generation)?)
}

impl FileStore {
    /// None means a legacy JSON snapshot; the kernel retains its compatibility path.
    pub fn facts(
        &self,
        root: &str,
        collection: &str,
        q: &FactQuery,
        code_only: bool,
    ) -> Result<Option<FactsResponse>> {
        let _lifecycle = self.lifecycle(false)?;
        if !self.directory.join("index.sqlite3").exists() {
            return Ok(None);
        }
        let conn = read_transaction(&self.directory)?;
        let tx = &conn;
        let generation = identity(tx, root, collection)?;
        let mut response = FactsResponse {
            indexing: conn.detail.into(),
            generation,
            publication: conn.publication.clone(),
            collection: collection.into(),
            facts: Vec::new(),
            truncated: false,
        };
        let name_filter = if q.name.is_none() {
            "1"
        } else if q.exact_name {
            "(name=?7 OR qualified_name=?7 OR symbol_id=?7)"
        } else {
            "instr(folded_name,?9)>0"
        };
        let mut stmt = tx.prepare(&format!(
            "SELECT payload FROM facts WHERE
            (?1=0 OR is_code=1) AND (?2=0 OR is_document=1)
            AND (?3 IS NULL OR kind=?3) AND (?4 IS NULL OR language=?4)
            AND ((?5=1 AND path=?6) OR (?5=0 AND substr(path,1,length(?6))=?6))
            AND {name_filter}
            ORDER BY path COLLATE BINARY,line,kind,name LIMIT ?10"
        ))?;
        let mut rows = stmt.query(params![
            code_only,
            q.document_only,
            q.kind,
            q.language,
            q.path_exact,
            q.path_prefix,
            q.name,
            q.exact_name,
            q.name.as_ref().map(|n| n.to_lowercase()),
            i64::try_from(q.limit + 1)?
        ])?;
        let mut remaining = q.max_bytes;
        while let Some(row) = rows.next()? {
            let payload: String = row.get(0)?;
            let hit: FactHit = serde_json::from_str(&payload)?;
            let bytes = hit.name.len()
                + hit.citation.path.len()
                + hit
                    .document
                    .as_ref()
                    .map(serde_json::to_vec)
                    .transpose()?
                    .map_or(0, |v| v.len())
                + hit
                    .code
                    .as_ref()
                    .map(serde_json::to_vec)
                    .transpose()?
                    .map_or(0, |v| v.len());
            if response.facts.len() == q.limit || bytes > remaining {
                response.truncated = true;
                break;
            }
            remaining -= bytes;
            response.facts.push(hit);
        }
        Ok(Some(response))
    }

    pub fn relationships(
        &self,
        root: &str,
        collection: &str,
        q: &RelationshipQuery,
    ) -> Result<RelationshipsResponse> {
        let _lifecycle = self.lifecycle(false)?;
        let conn = read_transaction(&self.directory)?;
        let tx = &conn;
        let generation = identity(tx, root, collection)?;
        let mut response = RelationshipsResponse {indexing:conn.detail.into(),generation,publication:conn.publication.clone(),collection:collection.into(),relationships:Vec::new(),truncated:false,coverage:"Selected syntax patterns and optional manifest-bound producer assertions. External targets are identifiers only; impact is incomplete.".into()};
        let mut pending = BTreeSet::from([q.symbol.clone()]);
        let mut visited = BTreeSet::new();
        let mut returned = BTreeSet::new();
        let deadline = Instant::now() + Duration::from_millis(250);
        let mut remaining = q.max_bytes;
        for _ in 0..q.hops {
            let mut next = BTreeSet::new();
            for symbol in pending {
                if !visited.insert(symbol.clone()) {
                    continue;
                }
                if visited.len() > 100 || Instant::now() > deadline {
                    response.truncated = true;
                    return Ok(response);
                }
                // Separate indexed branches prevent scanning the whole graph per hop.
                let sql = if q.direction == "incoming" {
                    "SELECT payload FROM relationships WHERE target=?1 AND (?2 IS NULL OR kind=?2) AND substr(path,1,length(?3))=?3 ORDER BY path,line,source,target LIMIT ?4"
                } else {
                    "SELECT payload FROM relationships WHERE source=?1 AND (?2 IS NULL OR kind=?2) AND substr(path,1,length(?3))=?3 ORDER BY path,line,source,target LIMIT ?4"
                };
                let mut stmt = tx.prepare(sql)?;
                let mut rows = stmt.query(params![
                    symbol,
                    q.kind,
                    q.path_prefix,
                    i64::try_from(q.limit + 1)?
                ])?;
                while let Some(row) = rows.next()? {
                    let payload: String = row.get(0)?;
                    if !returned.insert(payload.clone()) {
                        continue;
                    }
                    if response.relationships.len() == q.limit
                        || payload.len() > remaining
                        || Instant::now() > deadline
                    {
                        response.truncated = true;
                        return Ok(response);
                    }
                    let edge: RelationshipHit = serde_json::from_str(&payload)?;
                    remaining -= payload.len();
                    next.insert(if q.direction == "incoming" {
                        edge.source.clone()
                    } else {
                        edge.target.clone()
                    });
                    response.relationships.push(edge);
                }
            }
            pending = next;
            if pending.is_empty() {
                break;
            }
        }
        Ok(response)
    }
}

fn history_admin_connection(
    directory: &Path,
    conn: &Connection,
) -> Result<crate::history_policy::AdminState> {
    let crate::ExpectedHead::V2(head) = expected_head_connection(directory, conn)? else {
        anyhow::bail!("history controls require v2 migration");
    };
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    ensure!(
        is_v2_root(version),
        "history controls require supported publication root"
    );
    if !table_exists(conn, "history_admin")? {
        ensure!(version < 5, "history administrative authority missing");
        return crate::history_policy::AdminState::empty(head.incarnation);
    }
    ensure!(
        version == 5,
        "administrative table has unsupported root version"
    );
    let encoded: Option<Vec<u8>> = conn
        .query_row(
            "SELECT substr(CAST(payload AS BLOB),1,65537) FROM history_admin WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let encoded = encoded.context("history administrative row missing")?;
    ensure!(
        encoded.len() <= crate::history_policy::MAX_HISTORY_ADMIN_BYTES,
        "oversized history administrative state"
    );
    let state: crate::history_policy::AdminState = serde_json::from_slice(&encoded)?;
    state.validate()?;
    ensure!(
        state.incarnation == head.incarnation,
        "foreign administrative incarnation"
    );
    for checkpoint in state.checkpoints.values() {
        ensure!(
            receipt_connection(conn, &checkpoint.publication.token)?.as_ref()
                == Some(&checkpoint.publication),
            "checkpoint publication receipt missing or changed"
        );
    }
    Ok(state)
}

pub(crate) fn history_admin(directory: &Path) -> Result<crate::history_policy::AdminState> {
    retry_wal_protocol(|| {
        history_admin_connection(directory, &connect_root_once(directory, false)?)
    })
}

pub(crate) fn commit_history_admin(
    directory: &Path,
    expected: &crate::history_policy::AdminState,
    next: &crate::history_policy::AdminState,
    expected_head: &airk_core::identity::RevisionToken,
    reservation: &crate::quota::Reservation,
) -> Result<()> {
    expected.validate()?;
    next.validate()?;
    ensure!(
        next.incarnation == expected.incarnation
            && next.sequence
                == expected
                    .sequence
                    .checked_add(1)
                    .context("operator sequence exhausted")?,
        "invalid operator mutation sequence"
    );
    let (maximum, allowance) = pointer_budget(directory)?;
    reservation.charge(allowance)?;
    let mut conn = connect_root(directory, true)?;
    conn.execute_batch(&format!(
        "PRAGMA max_page_count={maximum}; PRAGMA cache_spill=OFF; PRAGMA temp_store=MEMORY;"
    ))?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    ensure!(
        expected_head_connection(directory, &tx)? == crate::ExpectedHead::V2(*expected_head),
        "publication changed before operator mutation"
    );
    ensure!(
        &history_admin_connection(directory, &tx)? == expected,
        "history administrative state changed"
    );
    for checkpoint in next.checkpoints.values() {
        ensure!(
            receipt_connection(&tx, &checkpoint.publication.token)?.as_ref()
                == Some(&checkpoint.publication),
            "checkpoint admission receipt changed"
        );
    }
    let encoded = serde_json::to_vec(next)?;
    ensure!(
        encoded.len() <= crate::history_policy::MAX_HISTORY_ADMIN_BYTES,
        "history administrative state exceeds bound"
    );
    tx.execute_batch("CREATE TABLE IF NOT EXISTS history_admin(id INTEGER PRIMARY KEY CHECK(id=1),payload BLOB NOT NULL);")?;
    tx.execute("INSERT INTO history_admin VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [encoded])?;
    tx.pragma_update(None, "user_version", 5u32)?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn history_receipt_count(directory: &Path) -> Result<u64> {
    let conn = connect_root(directory, false)?;
    let count: i64 = conn.query_row("SELECT count(*) FROM publication_receipts", [], |row| {
        row.get(0)
    })?;
    Ok(u64::try_from(count)?)
}
pub(crate) fn history_selection(
    directory: &Path,
) -> Result<Option<crate::history_policy::SelectionScan>> {
    let conn = connect_root(directory, false)?;
    if !table_exists(&conn, "history_selection")? {
        return Ok(None);
    }
    let bytes: Option<Vec<u8>> = conn
        .query_row(
            "SELECT substr(CAST(payload AS BLOB),1,262145) FROM history_selection WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    bytes
        .map(|bytes| {
            ensure!(
                bytes.len() <= 262144,
                "history selection cache exceeds bound"
            );
            let scan: crate::history_policy::SelectionScan = serde_json::from_slice(&bytes)?;
            scan.validate()?;
            Ok(scan)
        })
        .transpose()
}
pub(crate) fn history_scan_receipts(
    directory: &Path,
    before: Option<u64>,
    limit: usize,
) -> Result<Vec<airk_core::identity::PublicationReceipt>> {
    ensure!(limit <= 256, "history scan page exceeds bound");
    let conn = connect_root(directory, false)?;
    let mut stmt = conn.prepare("SELECT generation,revision,substr(CAST(payload AS BLOB),1,4097) FROM publication_receipts WHERE ?1 IS NULL OR generation<?1 ORDER BY generation DESC,revision LIMIT ?2")?;
    let rows = stmt.query_map(
        params![
            before.map(i64::try_from).transpose()?,
            i64::try_from(limit)?
        ],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        },
    )?;
    rows.map(|row| {
        let (generation, revision, bytes) = row?;
        ensure!(bytes.len() <= 4096, "oversized selection receipt");
        let receipt: airk_core::identity::PublicationReceipt = serde_json::from_slice(&bytes)?;
        ensure!(
            receipt.token.generation == u64::try_from(generation)?
                && receipt.token.revision.to_string() == revision,
            "selection receipt index mismatch"
        );
        Ok(receipt)
    })
    .collect()
}
pub(crate) fn commit_history_selection(
    directory: &Path,
    scan: &crate::history_policy::SelectionScan,
    reservation: &crate::quota::Reservation,
) -> Result<()> {
    scan.validate()?;
    let (maximum, allowance) = pointer_budget(directory)?;
    reservation.charge(allowance)?;
    let mut conn = connect_root(directory, true)?;
    conn.execute_batch(&format!(
        "PRAGMA max_page_count={maximum}; PRAGMA cache_spill=OFF; PRAGMA temp_store=MEMORY;"
    ))?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    ensure!(
        expected_head_connection(directory, &tx)? == crate::ExpectedHead::V2(scan.head),
        "head changed during historical selection"
    );
    let state = history_admin_connection(directory, &tx)?;
    ensure!(
        state.sequence == scan.operator_sequence && state.policy.digest == scan.policy_digest,
        "operator policy changed during selection"
    );
    let count: i64 = tx.query_row("SELECT count(*) FROM publication_receipts", [], |row| {
        row.get(0)
    })?;
    ensure!(
        u64::try_from(count)? == scan.receipt_count,
        "retained receipt set changed during selection"
    );
    tx.execute_batch("CREATE TABLE IF NOT EXISTS history_selection(id INTEGER PRIMARY KEY CHECK(id=1),payload BLOB NOT NULL);")?;
    tx.execute("INSERT INTO history_selection VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [serde_json::to_vec(scan)?])?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn retained_receipts(
    directory: &Path,
    before: Option<u64>,
    limit: usize,
) -> Result<crate::history_policy::RetainedReceiptPage> {
    ensure!(
        (1..=64).contains(&limit),
        "retained receipt page limit must be 1..64"
    );
    retry_wal_protocol(|| {
        let mut conn = connect_root(directory, false)?;
        let tx = conn.transaction()?;
        let crate::ExpectedHead::V2(head) = expected_head_connection(directory, &tx)? else {
            anyhow::bail!("retained receipt listing requires v2 migration");
        };
        ensure!(
            before.is_none_or(|generation| generation <= head.generation),
            "receipt cursor exceeds current generation"
        );
        let mut stmt = tx.prepare("SELECT generation,revision,substr(CAST(payload AS BLOB),1,4097) FROM publication_receipts WHERE ?1 IS NULL OR generation<?1 ORDER BY generation DESC,revision LIMIT ?2")?;
        let rows = stmt.query_map(
            params![
                before.map(i64::try_from).transpose()?,
                i64::try_from(limit + 1)?
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )?;
        let mut receipts = vec![];
        for row in rows {
            let (generation, revision, bytes) = row?;
            ensure!(bytes.len() <= 4096, "retained receipt exceeds row bound");
            let receipt: airk_core::identity::PublicationReceipt = serde_json::from_slice(&bytes)?;
            receipt.token.validate_against(&head)?;
            ensure!(
                receipt.token.generation == u64::try_from(generation)?
                    && receipt.token.revision.to_string() == revision,
                "retained receipt index mismatch"
            );
            receipts.push(receipt);
        }
        let complete = receipts.len() <= limit;
        receipts.truncate(limit);
        let next_before_generation = if complete {
            None
        } else {
            receipts.last().map(|receipt| receipt.token.generation)
        };
        Ok(crate::history_policy::RetainedReceiptPage {
            head,
            receipts,
            next_before_generation,
            complete,
        })
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn maintenance_pages_include_generation_zero_and_advance_past_prefix() -> anyhow::Result<()> {
        use airk_core::identity::{Incarnation, PublicationReceipt, RevisionToken, Sha256Digest};
        let root = tempfile::tempdir()?;
        let connection = super::connect_root(root.path(), true)?;
        connection.pragma_update(None, "user_version", crate::ROOT_POINTER_VERSION)?;
        connection.execute_batch(
            "CREATE TABLE publication_receipts(generation INTEGER, revision TEXT, payload TEXT);",
        )?;
        for generation in 0..6u64 {
            let digest = Sha256Digest::from_bytes([generation as u8; 32]);
            let receipt = PublicationReceipt {
                token: RevisionToken {
                    incarnation: Incarnation::from_bytes([7; 16]),
                    generation,
                    revision: digest,
                },
                inventory_digest: digest,
                manifest_digest: digest,
            };
            connection.execute(
                "INSERT INTO publication_receipts VALUES(?1,?2,?3)",
                rusqlite::params![
                    generation as i64,
                    digest.to_string(),
                    serde_json::to_string(&receipt)?
                ],
            )?;
        }
        let page = super::maintenance_receipts(root.path(), None, 2)?;
        assert_eq!(
            page.iter().map(|r| r.token.generation).collect::<Vec<_>>(),
            vec![0, 1]
        );
        let next = super::maintenance_receipts(root.path(), Some(1), 2)?;
        assert_eq!(
            next.iter().map(|r| r.token.generation).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(
            super::maintenance_receipts(root.path(), Some(0), 1)?[0]
                .token
                .generation,
            1
        );
        assert!(super::maintenance_receipts(root.path(), Some(5), 2)?.is_empty());
        assert_eq!(
            super::maintenance_receipts(root.path(), None, 1)?[0]
                .token
                .generation,
            0
        );
        Ok(())
    }
    use super::*;
    use crate::FileStore;

    #[test]
    fn wal_retry_is_bounded_and_only_retries_protocol_failures() -> Result<()> {
        let mut attempts = 0;
        let value = retry_wal_protocol(|| {
            attempts += 1;
            if attempts < 3 {
                return Err(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_PROTOCOL),
                    None,
                )
                .into());
            }
            Ok(7)
        })?;
        assert_eq!((attempts, value), (3, 7));
        for (code, expected_attempts) in [
            (rusqlite::ffi::SQLITE_PROTOCOL, 3),
            (rusqlite::ffi::SQLITE_CORRUPT, 1),
            (rusqlite::ffi::SQLITE_IOERR, 1),
        ] {
            let mut attempts = 0;
            let result: Result<()> = retry_wal_protocol(|| {
                attempts += 1;
                Err(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None).into())
            });
            assert!(result.is_err());
            assert_eq!(attempts, expected_attempts);
        }
        Ok(())
    }

    #[test]
    fn catalog_admission_leaves_pointer_room_with_smaller_physical_capacity() -> Result<()> {
        let logical = 4 * 1024 * 1024 * 1024;
        let physical = 112 * 1024 * 1024; // 128 MiB volume after safety headroom.
        let pointer = 1_200_123;
        let maximum = catalog_page_budget(logical, physical, pointer)?;
        let admission = physical - physical / 10;
        assert!(maximum + pointer <= admission);
        assert!(admission - maximum - pointer < 4096);
        assert!(physical - maximum - pointer >= physical / 10);
        // Hosted Mac runners can lose tens of MiB between admission and the
        // fresh charge probe even for a tiny fixture on a mostly empty volume.
        let hosted_physical = 32 * 1024 * 1024 * 1024;
        let hosted_maximum =
            catalog_page_budget(100 * 1024 * 1024 * 1024, hosted_physical, pointer)?;
        assert!(hosted_maximum + pointer <= hosted_physical - 16 * 1024 * 1024);
        assert_eq!(catalog_page_budget(32_768, u64::MAX, 16_384)?, 16_384);
        let physical_error = catalog_page_budget(logical, pointer + 16_383, pointer).unwrap_err();
        assert!(
            physical_error
                .downcast_ref::<crate::quota::QuotaPressure>()
                .is_none()
        );
        let logical_error = catalog_page_budget(pointer + 16_383, physical, pointer).unwrap_err();
        let pressure = logical_error
            .downcast_ref::<crate::quota::QuotaPressure>()
            .expect("logical admission retains the typed quota cause");
        assert!(matches!(
            pressure.scope,
            crate::quota::PressureScope::Reservation
        ));
        assert_eq!(pressure.required_bytes, pointer + 16_384);
        assert_eq!(pressure.available_bytes, pointer + 16_383);
        assert!(pressure.protected_bytes.is_none());
        assert!(
            catalog_page_budget(8 * 1024 * 1024 * 1024, u64::MAX, pointer)?
                > 4 * 1024 * 1024 * 1024
        );
        assert_eq!(
            catalog_page_budget(u64::MAX, u64::MAX, 0)?,
            (u64::from(u32::MAX) - 1) * 4096
        );
        Ok(())
    }

    #[test]
    fn catalog_quota_accounts_for_repeated_long_citations_without_json_multiplier() -> Result<()> {
        let root = tempfile::tempdir()?;
        let directory = root.path().join("collections/default");
        let store = FileStore::new(&directory);
        let _writer = store.lock()?;
        let previous = Snapshot::empty("fixture".into());
        store.save_with_components(&previous, &"a".repeat(64))?;
        let path = format!("{}fixture.rs", "long_component/".repeat(100));
        let artifact = Artifact {
            path: path.clone(),
            content_hash: "b".repeat(64),
            bytes: 2000,
            chunks: Vec::new(),
            language: "rust".into(),
            extraction_method: "fixture".into(),
            parse_status: "parsed".into(),
            retained: None,
            facts: (1..=1000)
                .map(|line| Fact {
                    name: format!("item{line}"),
                    kind: "definition".into(),
                    syntax_kind: "function".into(),
                    start_line: line,
                    end_line: line,
                    location: DocumentLocation::Lines {
                        start: line,
                        end: line,
                    },
                    document: None,
                    code: None,
                })
                .collect(),
        };
        let artifact_bytes = serde_json::to_vec(&artifact)?.len() as u64;
        let mut source = Snapshot::empty("fixture".into());
        source.artifacts.insert(path.clone(), artifact.clone());
        let mut snapshot = source.clone();
        snapshot.storage_format = 1;
        snapshot.generation = 1;
        snapshot
            .artifacts
            .insert(path, store.retain_artifact(&artifact, None)?);
        let old_estimate =
            artifact_bytes * 3 + serde_json::to_vec(&snapshot)?.len() as u64 * 3 + 1024 * 1024;
        let quota = crate::quota::Quota::open(root.path())?;
        {
            let reservation = quota.reserve("default", old_estimate)?;
            let staging = reservation.staging_directory()?;
            let error = store
                .prepare_catalog(
                    &staging,
                    &snapshot,
                    &"c".repeat(64),
                    &source,
                    Some(&reservation),
                    old_estimate,
                )
                .expect_err("repeated long citation/index keys exceed the former JSON multiplier");
            assert!(format!("{error:#}").contains("private fact catalog"));
            assert_eq!(store.persistent_revision()?.unwrap().generation, 0);
        }
        let reservation = quota.reserve("default", 32 * 1024 * 1024)?;
        let before = reservation.remaining();
        let (_, pointer_allowance) = pointer_budget(&directory)?;
        let maximum = store.catalog_budget(&reservation)?;
        assert_eq!(
            reservation.remaining(),
            before,
            "planning does not charge the pointer twice"
        );
        assert!(maximum + pointer_allowance <= before);
        let staging = reservation.staging_directory()?;
        let revision = "d".repeat(64);
        store.prepare_catalog(
            &staging,
            &snapshot,
            &revision,
            &source,
            Some(&reservation),
            maximum,
        )?;
        let bytes = std::fs::metadata(staging.join("catalog.sqlite"))?.len();
        assert!(bytes > old_estimate && bytes <= maximum);
        let conn = Connection::open_with_flags(
            staging.join("catalog.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        assert_eq!(
            conn.query_row("SELECT count(*) FROM facts", [], |r| r.get::<_, i64>(0))?,
            1000
        );
        drop(conn);
        std::fs::create_dir_all(directory.join("generations"))?;
        std::fs::rename(&staging, directory.join("generations").join(&revision))?;
        store.publish_catalog(&snapshot, &revision, Some(&reservation))?;
        assert_eq!(store.persistent_revision()?.unwrap().generation, 1);
        Ok(())
    }

    #[test]
    fn wal_readers_keep_one_revision_while_another_writer_publishes() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let store = FileStore::new(dir.path());
        let mut snapshot = Snapshot::empty("fixture".into());
        store.save(&snapshot)?;
        let mut reader = connect(dir.path(), false)?;
        let tx = reader.transaction()?;
        assert_eq!(identity(&tx, "fixture", "default")?, 0);
        snapshot.generation = 1;
        store.save(&snapshot)?;
        assert_eq!(
            identity(&tx, "fixture", "default")?,
            0,
            "reader retains its transaction"
        );
        assert_eq!(store.load()?.expect("new snapshot").generation, 1);
        Ok(())
    }

    #[test]
    fn failed_transaction_retains_metadata_facts_and_previous_revision() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let store = FileStore::new(dir.path());
        let mut snapshot = Snapshot::empty("fixture".into());
        store.save_with_components(&snapshot, &"a".repeat(64))?;
        let conn = connect(dir.path(), true)?;
        conn.execute_batch("CREATE TRIGGER reject_revision BEFORE INSERT ON revision BEGIN SELECT RAISE(ABORT,'fixture rejection'); END;")?;
        snapshot.generation = 1;
        assert!(
            store
                .save_with_components(&snapshot, &"b".repeat(64))
                .is_err()
        );
        assert_eq!(store.load()?.expect("prior snapshot").generation, 0);
        assert_eq!(
            store
                .persistent_revision()?
                .expect("prior components")
                .revision,
            "a".repeat(64)
        );
        conn.execute_batch("DROP TRIGGER reject_revision;")?;
        store.save_with_components(&snapshot, &"b".repeat(64))?;
        assert_eq!(store.load()?.expect("new snapshot").generation, 1);
        assert_eq!(
            store
                .persistent_revision()?
                .expect("new components")
                .revision,
            "b".repeat(64)
        );
        Ok(())
    }

    #[test]
    fn pointer_quota_reserves_checkpoint_growth_of_wal_only_legacy_pages() -> Result<()> {
        let root = tempfile::tempdir()?;
        let directory = root.path().join("collections/default");
        let store = FileStore::new(&directory);
        let mut snapshot = Snapshot::empty("fixture".into());
        store.save(&snapshot)?;
        let conn = connect_root(&directory, true)?;
        conn.pragma_update(None, "wal_autocheckpoint", 0)?;
        conn.execute_batch("CREATE TABLE legacy_payload(value BLOB); INSERT INTO legacy_payload VALUES(zeroblob(1048576));")?;
        let page_size: u64 =
            u64::from(conn.pragma_query_value(None, "page_size", |row| row.get::<_, u32>(0))?);
        let pages: u64 =
            u64::from(conn.pragma_query_value(None, "page_count", |row| row.get::<_, u32>(0))?);
        assert!(
            pages * page_size
                > std::fs::metadata(directory.join("index.sqlite3"))?.len() + 128 * page_size,
            "fixture requires large committed pages still resident only in WAL"
        );
        let old_allowance = (pages + 128).max(128) * (page_size + 32) + 128 * page_size + 65_536;
        let reservation =
            crate::quota::Quota::open(root.path())?.reserve("default", old_allowance)?;
        let revision = "d".repeat(64);
        let prepared = directory.join("generations").join(&revision);
        std::fs::create_dir_all(&prepared)?;
        std::fs::write(prepared.join("catalog.sqlite"), b"unpublished fixture")?;
        snapshot.storage_format = 1;
        snapshot.generation = 1;
        assert!(
            store
                .publish_catalog(&snapshot, &revision, Some(&reservation))
                .is_err(),
            "WAL-only database growth must exhaust an allowance that counts only 128 new main-file pages"
        );
        assert_eq!(store.load()?.expect("prior legacy revision").generation, 0);
        Ok(())
    }

    #[test]
    fn legacy_snapshot_migrates_and_sqlite_corruption_never_falls_back() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let store = FileStore::new(dir.path());
        let snapshot = Snapshot::empty("fixture".into());
        std::fs::write(
            dir.path().join("snapshot.json"),
            serde_json::to_vec(&snapshot)?,
        )?;
        assert!(store.load()?.is_some());
        store.save(&snapshot)?;
        assert!(dir.path().join("index.sqlite3").is_file());
        assert!(
            dir.path().join("snapshot.json").is_file(),
            "legacy retained for recovery"
        );
        std::fs::write(dir.path().join("index.sqlite3"), b"corrupt fixture")?;
        assert!(store.load().is_err());
        Ok(())
    }
}
