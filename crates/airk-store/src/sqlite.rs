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

mod publication;
mod queries;
pub(super) use publication::{
    catalog_page_budget, pointer_budget, publish_catalog, publish_catalog_v2, save, save_catalog,
    save_catalog_v2,
};
mod usage_sizes;

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
    crate::reset::ensure_ready(directory)?;
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
