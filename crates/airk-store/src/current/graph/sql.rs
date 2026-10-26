// SPDX-License-Identifier: Apache-2.0
use super::merkle;
use airk_core::{graph_storage::*, identity::Sha256Digest};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Row, params};

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

pub(super) fn stored_record(
    connection: &Connection,
    scope: &GraphScope,
    id: u32,
) -> Result<Option<GraphRecord>> {
    let raw = connection
        .prepare_cached(
            "SELECT embedding_key,live,
        CASE WHEN typeof(record)='blob' AND length(record)=?2 THEN record END
        FROM graph_records WHERE id=?1",
        )?
        .query_row(params![i64::from(id), scope.record_bytes], |row| {
            row_record(row, 0)
        })
        .optional()?;
    raw.map(|raw| decode_record(scope, id, raw)).transpose()
}

pub(super) fn read_proof(
    connection: &dyn super::GraphStorage,
    head: &GraphHead,
    id: u32,
) -> Result<Proof> {
    head.validate()?;
    let record = connection.graph_record(&head.scope, id)?;
    let mut siblings = [Sha256Digest::from_bytes([0; 32]); 32];
    let mut index = id;
    for (height, sibling) in siblings.iter_mut().enumerate() {
        *sibling = connection
            .graph_node(height as u8, index ^ 1)?
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
    connection: &dyn super::GraphStorage,
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
