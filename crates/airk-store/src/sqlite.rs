// SPDX-License-Identifier: Apache-2.0
//! Embedded transactional catalog. Read tools never create/migrate a database.
use crate::FileStore;
use airk_core::*;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, params};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

fn connect(directory: &Path, write: bool) -> Result<Connection> {
    let flags = if write {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let conn = Connection::open_with_flags(directory.join("index.sqlite3"), flags)?;
    conn.busy_timeout(Duration::from_secs(2))?;
    if !write {
        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        ensure!(version == 1, "unsupported SQLite schema");
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

pub(super) fn load(directory: &Path) -> Result<Option<Snapshot>> {
    let conn = connect(directory, false)?;
    let payload: String = conn
        .query_row("SELECT payload FROM revision WHERE id=1", [], |r| r.get(0))
        .context("invalid SQLite catalog; rebuild with a compatible binary")?;
    ensure!(
        payload.len() <= 536_870_912,
        "snapshot exceeds 512 MiB limit"
    );
    let snapshot: Snapshot = serde_json::from_str(&payload)?;
    ensure!(
        snapshot.schema_version == SCHEMA_VERSION,
        "unsupported snapshot schema"
    );
    Ok(Some(snapshot))
}

pub(super) fn save(directory: &Path, snapshot: &Snapshot) -> Result<()> {
    let payload = serde_json::to_string(snapshot)?;
    ensure!(
        payload.len() <= 536_870_912,
        "derived snapshot exceeds 512 MiB; previous generation retained"
    );
    std::fs::create_dir_all(directory)?;
    let mut conn = connect(directory, true)?;
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(version <= 1, "unsupported SQLite schema");
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
        DELETE FROM facts; DELETE FROM relationships; PRAGMA user_version=1;")?;
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
        for a in snapshot.artifacts.values() {
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
        if !self.directory.join("index.sqlite3").exists() {
            return Ok(None);
        }
        let mut conn = connect(&self.directory, false)?;
        let tx = conn.transaction()?;
        let generation = identity(&tx, root, collection)?;
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
        let mut conn = connect(&self.directory, false)?;
        let tx = conn.transaction()?;
        let generation = identity(&tx, root, collection)?;
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
        store.save(&snapshot)?;
        let conn = connect(dir.path(), true)?;
        conn.execute_batch("CREATE TRIGGER reject_revision BEFORE INSERT ON revision BEGIN SELECT RAISE(ABORT,'fixture rejection'); END;")?;
        snapshot.generation = 1;
        assert!(store.save(&snapshot).is_err());
        assert_eq!(store.load()?.expect("prior snapshot").generation, 0);
        conn.execute_batch("DROP TRIGGER reject_revision;")?;
        store.save(&snapshot)?;
        assert_eq!(store.load()?.expect("new snapshot").generation, 1);
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
