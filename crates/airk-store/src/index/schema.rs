// SPDX-License-Identifier: Apache-2.0
//! Tables, connection settings and the format check of the plain index store.
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

pub(super) const USER_VERSION: i64 = 1;
/// "AIRX" as a big-endian integer. The old store never sets an application id.
pub(super) const APPLICATION_ID: i64 = 0x4149_5258;

/// The folder holds a database this version cannot read. The caller rebuilds.
#[derive(Clone, Copy, Debug)]
pub struct IncompatibleIndex;
impl std::fmt::Display for IncompatibleIndex {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("index database has an incompatible format; rebuild the index")
    }
}
impl std::error::Error for IncompatibleIndex {}

const TABLES: &str = "
CREATE TABLE meta(key TEXT PRIMARY KEY, value NOT NULL) WITHOUT ROWID;
CREATE TABLE files(
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  source_hash BLOB NOT NULL CHECK(length(source_hash)=32),
  language TEXT NOT NULL,
  record BLOB NOT NULL,
  metadata BLOB NOT NULL,
  payload BLOB NOT NULL CHECK(length(payload)=32),
  stat_size INTEGER,
  stat_modified_ns INTEGER,
  stat_file_id BLOB,
  vector_state INTEGER NOT NULL CHECK(vector_state BETWEEN 0 AND 3),
  changed_sequence INTEGER NOT NULL);
CREATE INDEX files_language ON files(language, path);
CREATE INDEX files_payload ON files(payload);
CREATE INDEX files_pending ON files(path) WHERE vector_state=1;
CREATE TABLE payloads(
  digest BLOB PRIMARY KEY CHECK(length(digest)=32),
  raw_bytes INTEGER NOT NULL,
  encoding INTEGER NOT NULL,
  body BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE chunks(
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL,
  ordinal INTEGER NOT NULL,
  stable_id BLOB NOT NULL CHECK(length(stable_id)=32),
  citation BLOB NOT NULL,
  UNIQUE(file_id, ordinal));
CREATE VIRTUAL TABLE fts USING fts5(terms, content='', contentless_delete=1,
  tokenize='ascii tokenchars ''_''');
CREATE TABLE selectors(
  kind INTEGER NOT NULL,
  value TEXT NOT NULL,
  file_id INTEGER NOT NULL,
  PRIMARY KEY(kind, value, file_id)) WITHOUT ROWID;
CREATE INDEX selectors_file ON selectors(file_id);
CREATE TABLE handles(
  kind INTEGER NOT NULL,
  value BLOB NOT NULL,
  file_id INTEGER NOT NULL,
  PRIMARY KEY(kind, value)) WITHOUT ROWID;
CREATE INDEX handles_file ON handles(file_id);
CREATE TABLE units(
  id INTEGER PRIMARY KEY,
  file_id INTEGER NOT NULL,
  ordinal INTEGER NOT NULL,
  embedding_key BLOB NOT NULL CHECK(length(embedding_key)=32),
  members BLOB NOT NULL,
  UNIQUE(file_id, ordinal));
CREATE INDEX units_key ON units(embedding_key);
CREATE TABLE vector_rows(
  embedding_key BLOB PRIMARY KEY CHECK(length(embedding_key)=32),
  row INTEGER NOT NULL UNIQUE) WITHOUT ROWID;
CREATE TABLE resources(key TEXT PRIMARY KEY, body BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE changes(
  sequence INTEGER NOT NULL,
  ordinal INTEGER NOT NULL,
  body BLOB NOT NULL,
  PRIMARY KEY(sequence, ordinal)) WITHOUT ROWID;
";

const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

fn configure(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "PRAGMA temp_store=MEMORY; PRAGMA cache_size=-65536; PRAGMA foreign_keys=OFF;",
    )?;
    Ok(())
}

pub(super) fn create(path: &Path) -> Result<Connection> {
    ensure!(!path.try_exists()?, "index database already exists");
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("create index database")?;
    connection.execute_batch(
        "PRAGMA page_size=8192; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
    )?;
    connection.pragma_update(None, "application_id", APPLICATION_ID)?;
    connection.execute_batch(TABLES)?;
    connection.pragma_update(None, "user_version", USER_VERSION)?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    configure(&connection)?;
    Ok(connection)
}

fn application_id(connection: &Connection) -> Result<i64> {
    Ok(connection.pragma_query_value(None, "application_id", |row| row.get(0))?)
}

fn check_format(connection: &Connection) -> Result<()> {
    let application = application_id(connection)?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if application != APPLICATION_ID || version != USER_VERSION {
        return Err(anyhow::Error::new(IncompatibleIndex));
    }
    Ok(())
}

pub(super) fn open_writer(path: &Path) -> Result<Connection> {
    open_writer_with_timeout(path, BUSY_TIMEOUT)
}

#[cfg(test)]
pub(super) fn open_writer_nowait(path: &Path) -> Result<Connection> {
    open_writer_with_timeout(path, std::time::Duration::ZERO)
}

fn open_writer_with_timeout(path: &Path, timeout: std::time::Duration) -> Result<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("open index database for writing")?;
    connection.busy_timeout(timeout)?;
    check_format(&connection)?;
    connection.execute_batch("PRAGMA synchronous=FULL;")?;
    configure(&connection)?;
    Ok(connection)
}

fn open_read_only(path: &Path) -> Result<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("open index database for reading")
}

/// Select the reader family. Each reader still validates its own schema and owner.
pub(crate) fn is_legacy_database(path: &Path) -> Result<bool> {
    match application_id(&open_read_only(path)?)? {
        0 => Ok(true),
        APPLICATION_ID => Ok(false),
        _ => Err(IncompatibleIndex.into()),
    }
}

pub(super) fn open_reader(path: &Path) -> Result<Connection> {
    let connection = open_read_only(path)?;
    check_format(&connection)?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    configure(&connection)?;
    Ok(connection)
}

pub(super) fn meta_integer(connection: &Connection, key: &str) -> Result<Option<i64>> {
    use rusqlite::OptionalExtension;
    Ok(connection
        .query_row("SELECT value FROM meta WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .optional()?)
}

pub(super) fn meta_blob(connection: &Connection, key: &str) -> Result<Option<Vec<u8>>> {
    use rusqlite::OptionalExtension;
    Ok(connection
        .query_row("SELECT value FROM meta WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .optional()?)
}

pub(super) fn set_meta(
    connection: &Connection,
    key: &str,
    value: impl rusqlite::ToSql,
) -> Result<()> {
    connection.execute(
        "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}
