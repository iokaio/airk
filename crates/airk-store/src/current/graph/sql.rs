// SPDX-License-Identifier: Apache-2.0
use super::{PAGE_ROWS, merkle};
use airk_core::{graph_storage::*, identity::Sha256Digest};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Row, params};

pub(super) const OVERLAY_SCHEMA: &str = "
CREATE TABLE dirty_records(
 id INTEGER PRIMARY KEY CHECK(id BETWEEN 0 AND 4294967295),
 embedding_key BLOB CHECK(embedding_key IS NULL OR (typeof(embedding_key)='blob' AND length(embedding_key)=32)),
 live INTEGER NOT NULL CHECK(live IN (0,1)), record BLOB NOT NULL CHECK(typeof(record)='blob' AND length(record)<=16384),
 checksum BLOB NOT NULL CHECK(typeof(checksum)='blob' AND length(checksum)=32));
CREATE TABLE dirty_nodes(height INTEGER NOT NULL CHECK(height BETWEEN 0 AND 32),
 idx INTEGER NOT NULL CHECK(idx BETWEEN 0 AND 4294967295),
 digest BLOB NOT NULL CHECK(typeof(digest)='blob' AND length(digest)=32), PRIMARY KEY(height,idx)) WITHOUT ROWID;
BEGIN IMMEDIATE;";

pub(crate) fn create(connection: &Connection) -> Result<()> {
    connection.execute_batch("CREATE TABLE graph_records(
 id INTEGER PRIMARY KEY CHECK(id BETWEEN 0 AND 4294967295),
 embedding_key BLOB CHECK(embedding_key IS NULL OR (typeof(embedding_key)='blob' AND length(embedding_key)=32)),
 live INTEGER NOT NULL CHECK(live IN (0,1)), record BLOB NOT NULL CHECK(typeof(record)='blob' AND length(record)<=16384));
 CREATE TABLE graph_merkle(height INTEGER NOT NULL CHECK(height BETWEEN 0 AND 32),
 idx INTEGER NOT NULL CHECK(idx BETWEEN 0 AND 4294967295),
 digest BLOB NOT NULL CHECK(typeof(digest)='blob' AND length(digest)=32), PRIMARY KEY(height,idx)) WITHOUT ROWID;
 CREATE TABLE pending_graph_records(txn BLOB NOT NULL CHECK(typeof(txn)='blob' AND length(txn)=32),
 id INTEGER NOT NULL CHECK(id BETWEEN 0 AND 4294967295), embedding_key BLOB,
 live INTEGER NOT NULL CHECK(live IN (0,1)),record BLOB NOT NULL CHECK(typeof(record)='blob' AND length(record)<=16384),
 PRIMARY KEY(txn,id)) WITHOUT ROWID;
 CREATE TABLE pending_graph_merkle(txn BLOB NOT NULL CHECK(typeof(txn)='blob' AND length(txn)=32),
 height INTEGER NOT NULL CHECK(height BETWEEN 0 AND 32),idx INTEGER NOT NULL CHECK(idx BETWEEN 0 AND 4294967295),
 digest BLOB NOT NULL CHECK(typeof(digest)='blob' AND length(digest)=32), PRIMARY KEY(txn,height,idx)) WITHOUT ROWID;")?;
    Ok(())
}

type RawRecord = (Option<[u8; 32]>, i64, Option<Vec<u8>>);
fn row_record(row: &Row<'_>, offset: usize) -> rusqlite::Result<RawRecord> {
    Ok((row.get(offset)?, row.get(offset + 1)?, row.get(offset + 2)?))
}
pub(super) fn decode_record(scope: &GraphScope, id: u32, raw: RawRecord) -> Result<GraphRecord> {
    ensure!(matches!(raw.1, 0 | 1), "invalid graph live marker");
    let record = GraphRecord {
        embedding_key: raw.0.map(Sha256Digest::from_bytes),
        live: raw.1 == 1,
        bytes: raw.2.context("graph record exceeds fixed byte bound")?,
    };
    record.validate(scope, id)?;
    Ok(record)
}

pub(super) fn dirty_record(
    connection: &Connection,
    scope: &GraphScope,
    id: u32,
) -> Result<Option<GraphRecord>> {
    let row = connection
        .query_row(
            "SELECT embedding_key,live,
        CASE WHEN typeof(record)='blob' AND length(record)=?2 THEN record END, checksum
        FROM dirty_records WHERE id=?1",
            params![i64::from(id), scope.record_bytes],
            |row| Ok((row_record(row, 0)?, row.get::<_, [u8; 32]>(3)?)),
        )
        .optional()?;
    row.map(|(raw, checksum)| {
        let record = decode_record(scope, id, raw)?;
        ensure!(
            leaf_hash(scope, id, &record)?.as_bytes() == &checksum,
            "private graph row checksum mismatch"
        );
        Ok(record)
    })
    .transpose()
}

#[derive(Clone)]
pub(super) struct Proof {
    pub record: Option<GraphRecord>,
    pub siblings: merkle::Siblings,
}
impl Proof {
    pub fn empty(head: &GraphHead, id: u32) -> Result<Self> {
        ensure!(
            head.allocated_count == 0,
            "missing base for allocated graph"
        );
        let mut siblings = [Sha256Digest::from_bytes([0; 32]); 32];
        for (height, sibling) in siblings.iter_mut().enumerate() {
            *sibling = empty_hash(&head.scope, height as u8)?;
        }
        merkle::verify_absent(head, id, &siblings)?;
        Ok(Self {
            record: None,
            siblings,
        })
    }
}

pub(super) fn node(
    connection: &Connection,
    table: &str,
    height: u8,
    index: u32,
) -> Result<Option<Sha256Digest>> {
    validate_coordinate(height, index)?;
    let query = match table {
        "graph_merkle" => "SELECT digest FROM graph_merkle WHERE height=?1 AND idx=?2",
        "dirty_nodes" => "SELECT digest FROM dirty_nodes WHERE height=?1 AND idx=?2",
        _ => anyhow::bail!("unknown private graph node table"),
    };
    let digest = connection
        .prepare_cached(query)?
        .query_row(params![height, i64::from(index)], |row| {
            row.get::<_, [u8; 32]>(0)
        })
        .optional()?;
    Ok(digest.map(Sha256Digest::from_bytes))
}

pub(super) fn read_proof(connection: &Connection, head: &GraphHead, id: u32) -> Result<Proof> {
    head.validate()?;
    let raw = connection
        .prepare_cached(
            "SELECT embedding_key,live,
        CASE WHEN typeof(record)='blob' AND length(record)=?2 THEN record END
        FROM graph_records WHERE id=?1",
        )?
        .query_row(params![i64::from(id), head.scope.record_bytes], |row| {
            row_record(row, 0)
        })
        .optional()?;
    let record = raw
        .map(|raw| decode_record(&head.scope, id, raw))
        .transpose()?;
    let mut siblings = [Sha256Digest::from_bytes([0; 32]); 32];
    let mut index = id;
    for (height, sibling) in siblings.iter_mut().enumerate() {
        *sibling = node(connection, "graph_merkle", height as u8, index ^ 1)?
            .unwrap_or(empty_hash(&head.scope, height as u8)?);
        index >>= 1;
    }
    match &record {
        Some(record) => verify_record(head, id, record, &siblings)?,
        None => merkle::verify_absent(head, id, &siblings)?,
    }
    Ok(Proof { record, siblings })
}

pub(crate) fn read_record(
    connection: &Connection,
    head: &GraphHead,
    id: u32,
) -> Result<GraphRecord> {
    ensure!(
        u64::from(id) < head.allocated_count,
        "graph ID outside committed allocation"
    );
    read_proof(connection, head, id)?
        .record
        .context("allocated graph record missing")
}

pub(super) fn record_page(
    connection: &Connection,
    scope: &GraphScope,
    after: Option<u32>,
) -> Result<Vec<(u32, GraphRecord)>> {
    let mut statement = connection.prepare_cached(
        "SELECT id,embedding_key,live,
        CASE WHEN typeof(record)='blob' AND length(record)=?2 THEN record END,checksum
        FROM dirty_records WHERE id>?1 ORDER BY id LIMIT ?3",
    )?;
    let mut rows = statement.query(params![
        after.map_or(-1, i64::from),
        scope.record_bytes,
        PAGE_ROWS as u32
    ])?;
    let mut page = Vec::with_capacity(PAGE_ROWS);
    while let Some(row) = rows.next()? {
        let id = u32::try_from(row.get::<_, i64>(0)?)?;
        let record = decode_record(scope, id, row_record(row, 1)?)?;
        ensure!(
            leaf_hash(scope, id, &record)?.as_bytes() == &row.get::<_, [u8; 32]>(4)?,
            "private graph row checksum mismatch"
        );
        page.push((id, record));
    }
    Ok(page)
}

pub(super) fn node_page(
    connection: &Connection,
    after: Option<(u8, u32)>,
) -> Result<Vec<GraphMerkleNode>> {
    let (height, index) = after.map_or((-1, 0), |(height, index)| {
        (i64::from(height), i64::from(index))
    });
    let mut statement = connection.prepare_cached(
        "SELECT height,idx,digest FROM dirty_nodes
        WHERE (height,idx)>(?1,?2) ORDER BY height,idx LIMIT ?3",
    )?;
    let mut rows = statement.query(params![height, index, PAGE_ROWS as u32])?;
    let mut page = Vec::with_capacity(PAGE_ROWS);
    while let Some(row) = rows.next()? {
        let node = GraphMerkleNode {
            height: u8::try_from(row.get::<_, i64>(0)?)?,
            index: u32::try_from(row.get::<_, i64>(1)?)?,
            digest: Sha256Digest::from_bytes(row.get(2)?),
        };
        node.validate()?;
        page.push(node);
    }
    Ok(page)
}

pub(super) fn page_bytes(connection: &Connection) -> Result<u64> {
    let count: u32 = connection.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    Ok(u64::from(count) * 4096)
}
