// SPDX-License-Identifier: Apache-2.0
//! Compact physical rows with the original SQL evidence views and MCP payloads.
use airk_core::{FactHit, RelationshipHit, identity::Sha256Digest};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(crate) fn present(conn: &Connection) -> Result<bool> {
    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='normalized_evidence' AND type='table')", [], |r| r.get(0))?;
    if exists {
        let (count, version): (i64, i64) = conn.query_row(
            "SELECT count(*),coalesce(max(version),0) FROM normalized_evidence",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        ensure!(
            count == 1 && version == 1,
            "unsupported normalized evidence schema"
        );
    }
    Ok(exists)
}

/// Query projection adds a generation-local row identity without changing persisted schemas.
pub(crate) fn fact_projection(with_identity: bool) -> String {
    let common = "'$.name',f.name,'$.kind',f.kind,'$.language',f.language,'$.citation.path',ef.path,'$.citation.content_hash',lower(hex(ef.source_hash)),'$.citation.start_line',f.line";
    let identity = if with_identity {
        "f.rowid AS discovery_id,"
    } else {
        ""
    };
    format!("SELECT {identity}ef.path AS path,f.name,f.folded_name,f.qualified_name,f.symbol_id,f.kind,f.language,f.is_code,f.is_document,f.line,
        CASE WHEN f.is_code=1 THEN json_set(f.payload,{common},'$.code.qualified_name',f.qualified_name,'$.code.symbol_id',f.symbol_id,'$.code.basis',json_extract(p.payload,'$.basis'),'$.code.producer',json_extract(p.payload,'$.producer'),'$.code.producer_version',json_extract(p.payload,'$.producer_version'),'$.code.coverage',json_extract(p.payload,'$.coverage'))
        ELSE json_set(f.payload,{common}) END AS payload
        FROM fact_rows f JOIN evidence_files ef ON ef.id=f.file_id LEFT JOIN evidence_producers p ON p.id=f.producer_id")
}

pub(crate) fn create(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE normalized_evidence(version INTEGER NOT NULL); INSERT INTO normalized_evidence VALUES(1);
        CREATE TABLE evidence_files(id INTEGER PRIMARY KEY,path TEXT NOT NULL,source_hash BLOB NOT NULL CHECK(length(source_hash)=32),UNIQUE(path,source_hash));
        CREATE TABLE evidence_producers(id INTEGER PRIMARY KEY,payload TEXT NOT NULL UNIQUE);
        CREATE TABLE fact_rows(file_id INTEGER NOT NULL,name TEXT NOT NULL,folded_name TEXT NOT NULL,qualified_name TEXT NOT NULL,symbol_id TEXT NOT NULL,kind TEXT NOT NULL,language TEXT NOT NULL,is_code INTEGER NOT NULL,is_document INTEGER NOT NULL,line INTEGER NOT NULL,producer_id INTEGER,payload TEXT NOT NULL,imported INTEGER NOT NULL);
        CREATE TABLE edge_rows(file_id INTEGER NOT NULL,source TEXT NOT NULL,target TEXT NOT NULL,kind TEXT NOT NULL,line INTEGER NOT NULL,producer_id INTEGER NOT NULL,payload TEXT NOT NULL,imported INTEGER NOT NULL);")?;
    tx.execute_batch(&format!("CREATE VIEW facts AS {};
        CREATE VIEW relationships AS SELECT e.source,e.target,e.kind,ef.path AS path,e.line,
        json_set(e.payload,'$.source',e.source,'$.target',e.target,'$.kind',e.kind,'$.citation.path',ef.path,'$.citation.content_hash',lower(hex(ef.source_hash)),'$.citation.start_line',e.line,'$.basis',json_extract(p.payload,'$.basis'),'$.producer',json_extract(p.payload,'$.producer'),'$.producer_version',json_extract(p.payload,'$.producer_version')) AS payload
        FROM edge_rows e JOIN evidence_files ef ON ef.id=e.file_id JOIN evidence_producers p ON p.id=e.producer_id;", fact_projection(false)))?;
    Ok(())
}

pub(crate) fn indexes(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE INDEX facts_name ON fact_rows(name,kind); CREATE INDEX facts_qualified ON fact_rows(qualified_name); CREATE INDEX facts_symbol ON fact_rows(symbol_id); CREATE INDEX facts_path ON fact_rows(file_id,line); CREATE INDEX edges_source ON edge_rows(source,kind); CREATE INDEX edges_target ON edge_rows(target,kind); CREATE INDEX edges_file ON edge_rows(file_id);")?;
    Ok(())
}

pub(crate) struct Writer<'a> {
    tx: &'a Transaction<'a>,
    last_file: Option<(String, String, i64)>,
    producers: BTreeMap<String, i64>,
    producer_bytes: usize,
}

impl<'a> Writer<'a> {
    pub(crate) fn new(tx: &'a Transaction<'a>) -> Self {
        Self {
            tx,
            last_file: None,
            producers: BTreeMap::new(),
            producer_bytes: 0,
        }
    }

    pub(crate) fn file(&mut self, path: &str, hash: &str) -> Result<i64> {
        if let Some((p, h, id)) = &self.last_file
            && p == path
            && h == hash
        {
            return Ok(*id);
        }
        let digest: Sha256Digest = hash.parse()?;
        let id = self
            .tx
            .query_row(
                "SELECT id FROM evidence_files WHERE path=?1 AND source_hash=?2",
                params![path, digest.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        let id = match id {
            Some(id) => id,
            None => {
                self.tx.execute(
                    "INSERT INTO evidence_files(path,source_hash) VALUES(?1,?2)",
                    params![path, digest.as_bytes().as_slice()],
                )?;
                self.tx.last_insert_rowid()
            }
        };
        self.last_file = Some((path.into(), hash.into(), id));
        Ok(id)
    }

    fn producer(&mut self, payload: &str) -> Result<i64> {
        if let Some(id) = self.producers.get(payload) {
            return Ok(*id);
        }
        let id = self
            .tx
            .query_row(
                "SELECT id FROM evidence_producers WHERE payload=?1",
                [payload],
                |r| r.get(0),
            )
            .optional()?;
        let id = match id {
            Some(id) => id,
            None => {
                self.tx.execute(
                    "INSERT INTO evidence_producers(payload) VALUES(?1)",
                    [payload],
                )?;
                self.tx.last_insert_rowid()
            }
        };
        // The dictionary itself is disk backed; only this bounded lookup cache is resident.
        if self.producer_bytes + payload.len() > 1024 * 1024 || self.producers.len() == 1024 {
            self.producers.clear();
            self.producer_bytes = 0;
        }
        if payload.len() <= 1024 * 1024 {
            self.producer_bytes += payload.len();
            self.producers.insert(payload.into(), id);
        }
        Ok(id)
    }

    pub(crate) fn fact(&mut self, hit: &FactHit, imported: bool) -> Result<()> {
        let file = self.file(&hit.citation.path, &hit.citation.content_hash)?;
        let mut payload = serde_json::to_value(hit)?;
        let object = payload.as_object_mut().context("fact object missing")?;
        for key in ["name", "kind", "language"] {
            object.remove(key);
        }
        strip_citation(object)?;
        let producer = if let Some(code) = object.get_mut("code").and_then(Value::as_object_mut) {
            code.remove("qualified_name");
            code.remove("symbol_id");
            Some(self.producer(&take_producer(code, true)?)?)
        } else {
            None
        };
        let code = hit.code.as_ref();
        self.tx
            .prepare_cached(
                "INSERT INTO fact_rows VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            )?
            .execute(params![
                file,
                hit.name,
                hit.name.to_lowercase(),
                code.map_or("", |c| c.qualified_name.as_str()),
                code.map_or("", |c| c.symbol_id.as_str()),
                hit.kind,
                hit.language,
                code.is_some(),
                code.is_none() && !matches!(hit.kind.as_str(), "definition" | "import"),
                i64::try_from(hit.citation.start_line)?,
                producer,
                serde_json::to_string(&payload)?,
                imported
            ])?;
        Ok(())
    }

    pub(crate) fn edge(&mut self, edge: &RelationshipHit, imported: bool) -> Result<()> {
        let file = self.file(&edge.citation.path, &edge.citation.content_hash)?;
        let mut payload = serde_json::to_value(edge)?;
        let object = payload.as_object_mut().context("edge object missing")?;
        for key in ["source", "target", "kind"] {
            object.remove(key);
        }
        strip_citation(object)?;
        let producer = self.producer(&take_producer(object, false)?)?;
        self.tx
            .prepare_cached("INSERT INTO edge_rows VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")?
            .execute(params![
                file,
                edge.source,
                edge.target,
                edge.kind,
                i64::try_from(edge.citation.start_line)?,
                producer,
                serde_json::to_string(&payload)?,
                imported
            ])?;
        Ok(())
    }

    /// Reuse native rows only. Provider assertions are rebuilt independently.
    /// The caller authenticates the pinned base and exact per-file input metadata.
    pub(crate) fn copy_native(&mut self, base: &Connection, path: &str, hash: &str) -> Result<()> {
        let digest: Sha256Digest = hash.parse()?;
        let source: Option<i64> = base
            .query_row(
                "SELECT id FROM evidence_files WHERE path=?1 AND source_hash=?2",
                params![path, digest.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        let file = self.file(path, hash)?;
        let Some(source) = source else {
            return Ok(());
        };
        let mut stmt = base.prepare("SELECT f.name,f.folded_name,f.qualified_name,f.symbol_id,f.kind,f.language,f.is_code,f.is_document,f.line,p.payload,f.payload FROM fact_rows f LEFT JOIN evidence_producers p ON p.id=f.producer_id WHERE f.file_id=?1 AND f.imported=0 ORDER BY f.rowid")?;
        let mut rows = stmt.query([source])?;
        while let Some(row) = rows.next()? {
            let producer: Option<String> = row.get(9)?;
            let producer = producer.as_deref().map(|p| self.producer(p)).transpose()?;
            let mut values = vec![rusqlite::types::Value::Integer(file)];
            for col in 0..9 {
                values.push(row.get::<_, rusqlite::types::Value>(col)?);
            }
            values.push(producer.map_or(
                rusqlite::types::Value::Null,
                rusqlite::types::Value::Integer,
            ));
            values.push(row.get(10)?);
            values.push(rusqlite::types::Value::Integer(0));
            self.tx
                .prepare_cached(
                    "INSERT INTO fact_rows VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                )?
                .execute(rusqlite::params_from_iter(values))?;
        }
        let mut stmt = base.prepare("SELECT e.source,e.target,e.kind,e.line,p.payload,e.payload FROM edge_rows e JOIN evidence_producers p ON p.id=e.producer_id WHERE e.file_id=?1 AND e.imported=0 ORDER BY e.rowid")?;
        let mut rows = stmt.query([source])?;
        while let Some(row) = rows.next()? {
            let producer = self.producer(&row.get::<_, String>(4)?)?;
            let mut values = vec![rusqlite::types::Value::Integer(file)];
            for col in 0..4 {
                values.push(row.get::<_, rusqlite::types::Value>(col)?);
            }
            values.push(rusqlite::types::Value::Integer(producer));
            values.push(row.get(5)?);
            values.push(rusqlite::types::Value::Integer(0));
            self.tx
                .prepare_cached("INSERT INTO edge_rows VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")?
                .execute(rusqlite::params_from_iter(values))?;
        }
        Ok(())
    }
}

fn strip_citation(object: &mut Map<String, Value>) -> Result<()> {
    let citation = object
        .get_mut("citation")
        .and_then(Value::as_object_mut)
        .context("citation missing")?;
    for key in ["path", "content_hash", "start_line"] {
        citation.remove(key);
    }
    Ok(())
}

fn take_producer(object: &mut Map<String, Value>, coverage: bool) -> Result<String> {
    let mut producer = Map::new();
    for key in ["basis", "producer", "producer_version"]
        .into_iter()
        .chain(coverage.then_some("coverage"))
    {
        producer.insert(
            key.into(),
            object.remove(key).context("producer field missing")?,
        );
    }
    Ok(serde_json::to_string(&producer)?)
}
