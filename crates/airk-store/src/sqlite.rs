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
    conn: Connection,
    _lease: Option<std::fs::File>,
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

fn connect_root(directory: &Path, write: bool) -> Result<Connection> {
    let flags = if write {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let conn = Connection::open_with_flags(directory.join("index.sqlite3"), flags)?;
    conn.busy_timeout(Duration::from_secs(2))?;
    if !write {
        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        ensure!((1..=2).contains(&version), "unsupported SQLite schema");
        let deadline = Instant::now() + Duration::from_millis(250);
        conn.progress_handler(1000, Some(move || Instant::now() > deadline))?;
    }
    if write {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA max_page_count=262144;",
        )?;
    }
    Ok(conn)
}

fn connect(directory: &Path, write: bool) -> Result<PinnedConnection> {
    let conn = connect_root(directory, write)?;
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
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
            conn,
            _lease: Some(lease),
        });
    }
    Ok(PinnedConnection { conn, _lease: None })
}

/// Select legacy rows or an immutable generation while the active pointer is
/// pinned by a read transaction. Migration cannot combine new metadata/old facts.
fn read_transaction(directory: &Path) -> Result<PinnedConnection> {
    for _ in 0..3 {
        let conn = connect_root(directory, false)?;
        conn.execute_batch("BEGIN DEFERRED")?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 1 {
            return Ok(PinnedConnection { conn, _lease: None });
        }
        let payload: String = conn.query_row("SELECT payload FROM index_components WHERE id=1 AND length(CAST(payload AS BLOB)) <= 131072", [], |r| r.get(0))?;
        let header: PersistentRevision = serde_json::from_str(&payload)?;
        ensure!(
            header.storage_format == 1,
            "unsupported active catalog format"
        );
        let open = || -> Result<PinnedConnection> {
            let lease = std::fs::File::open(crate::blobs::reader_lease_path(
                directory,
                &header.revision,
            )?)?;
            lease.lock_shared()?;
            let inner = Connection::open_with_flags(
                directory
                    .join("generations")
                    .join(&header.revision)
                    .join("catalog.sqlite"),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            inner.execute_batch("BEGIN DEFERRED")?;
            ensure!(
                identity(&inner, &header.root, &header.collection)? == header.generation,
                "catalog generation mismatch"
            );
            let deadline = Instant::now() + Duration::from_millis(250);
            inner.progress_handler(1000, Some(move || Instant::now() > deadline))?;
            Ok(PinnedConnection {
                conn: inner,
                _lease: Some(lease),
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
        snapshot.schema_version == SCHEMA_VERSION,
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
    let conn = connect_root(directory, false)?;
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
                schema == SCHEMA_VERSION
                    && header.root == root
                    && header.collection == collection
                    && header.generation == u64::try_from(generation)?,
                "component header does not match active revision"
            );
            Ok(header)
        })
        .transpose()
}

pub(super) fn save(directory: &Path, snapshot: &Snapshot, revision: Option<&str>) -> Result<()> {
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
        let mut database =
            reservation.open_private_sqlite(directory.join("catalog.sqlite"), maximum)?;
        save_connection(database.connection_mut(), snapshot, Some(revision), source)?;
        return database.finish();
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
    let existing_frames = file_bytes("index.sqlite3-wal")? / (page_size + 24);
    let shared_growth = existing_frames
        .checked_add(maximum)
        .and_then(|frames| frames.checked_mul(8))
        .and_then(|bytes| bytes.checked_add(65_536))
        .context("pointer shared-memory bound overflow")?
        .saturating_sub(file_bytes("index.sqlite3-shm")?);
    let allowance = maximum
        .checked_mul(page_size + 32)
        .and_then(|value| value.checked_add(database_growth))
        .and_then(|value| value.checked_add(shared_growth))
        .and_then(|value| value.checked_add(32))
        .context("pointer journal bound overflow")?;
    if let Some(reservation) = reservation {
        reservation.charge(allowance)?;
    }
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
    save_connection(&mut conn, snapshot, revision, source)
}

fn save_connection(
    conn: &mut Connection,
    snapshot: &Snapshot,
    revision: Option<&str>,
    source: &dyn ArtifactSource,
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
    tx.execute_batch("CREATE TABLE IF NOT EXISTS revision(id INTEGER PRIMARY KEY CHECK(id=1), root TEXT NOT NULL, collection TEXT NOT NULL, generation INTEGER NOT NULL, schema_version INTEGER NOT NULL, payload TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS facts(path TEXT NOT NULL, name TEXT NOT NULL, folded_name TEXT NOT NULL, qualified_name TEXT NOT NULL, symbol_id TEXT NOT NULL, kind TEXT NOT NULL, language TEXT NOT NULL, is_code INTEGER NOT NULL, is_document INTEGER NOT NULL, line INTEGER NOT NULL, payload TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS facts_name ON facts(name,kind);
        CREATE INDEX IF NOT EXISTS facts_qualified ON facts(qualified_name);
        CREATE INDEX IF NOT EXISTS facts_symbol ON facts(symbol_id);
        CREATE INDEX IF NOT EXISTS facts_path ON facts(path,line);
        CREATE TABLE IF NOT EXISTS relationships(source TEXT NOT NULL,target TEXT NOT NULL,kind TEXT NOT NULL,path TEXT NOT NULL,line INTEGER NOT NULL,payload TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS edges_source ON relationships(source,kind);
        CREATE INDEX IF NOT EXISTS edges_target ON relationships(target,kind);
        CREATE TABLE IF NOT EXISTS index_components(id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL);
        DELETE FROM index_components;
        DELETE FROM facts; DELETE FROM relationships; PRAGMA user_version=1;")?;
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
        let mut facts =
            tx.prepare("INSERT INTO facts VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)")?;
        let mut edges = tx.prepare("INSERT INTO relationships VALUES(?1,?2,?3,?4,?5,?6)")?;
        let mut insert = |hit: &FactHit| -> Result<()> {
            let code = hit.code.as_ref();
            facts.execute(params![
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
            if let Some(c) = code {
                if let Some(owner) = &c.owner_id {
                    insert_edge(
                        &mut edges,
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
                    )?;
                }
                if let Some(target) = &c.target {
                    insert_edge(
                        &mut edges,
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
                    )?;
                }
            }
            Ok(())
        };
        for path in snapshot.artifacts.keys() {
            let a = source.load_artifact(path)?;
            for f in &a.facts {
                insert(&FactHit {
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
                })?;
            }
        }
        for f in &snapshot.imported_facts {
            insert(f)?;
        }

        for edge in &snapshot.imported_relationships {
            insert_edge(&mut edges, edge)?;
        }
    }
    tx.commit()?;
    Ok(())
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
    ensure!(schema == SCHEMA_VERSION, "unsupported snapshot schema");
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
            generation,
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
        let mut response = RelationshipsResponse {generation,collection:collection.into(),relationships:Vec::new(),truncated:false,coverage:"Selected syntax patterns and optional manifest-bound producer assertions. External targets are identifiers only; impact is incomplete.".into()};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileStore;

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
