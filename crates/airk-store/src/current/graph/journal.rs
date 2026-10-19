// SPDX-License-Identifier: Apache-2.0
//! Durable delta import/apply inside the current owner's transaction. Failed
//! callbacks or validation require the caller to roll back the entire transaction.
use super::{digest, merkle, sql, staging};
use airk_core::{graph_storage::*, identity::Sha256Digest};
use anyhow::{Result, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

fn pending_node(
    connection: &Connection,
    txn: Sha256Digest,
    height: u8,
    index: u32,
) -> Result<Option<Sha256Digest>> {
    validate_coordinate(height, index)?;
    Ok(connection
        .prepare_cached(
            "SELECT digest FROM pending_graph_merkle WHERE txn=?1 AND height=?2 AND idx=?3",
        )?
        .query_row(
            params![txn.as_bytes().as_slice(), height, i64::from(index)],
            |row| row.get::<_, [u8; 32]>(0),
        )
        .optional()?
        .map(Sha256Digest::from_bytes))
}

fn put_pending_node(
    connection: &Connection,
    txn: Sha256Digest,
    node: GraphMerkleNode,
) -> Result<()> {
    let values = params![
        txn.as_bytes().as_slice(),
        node.height,
        i64::from(node.index),
        node.digest.as_bytes().as_slice()
    ];
    if connection.execute("UPDATE OR ROLLBACK pending_graph_merkle SET digest=?4 WHERE txn=?1 AND height=?2 AND idx=?3",values)?==0 {
        connection.execute("INSERT OR ROLLBACK INTO pending_graph_merkle(txn,height,idx,digest) VALUES(?1,?2,?3,?4)",values)?;
    }
    Ok(())
}

pub(crate) fn stage(
    transaction: &Transaction<'_>,
    delta: &dyn PreparedGraphDelta,
    txn: Sha256Digest,
) -> Result<GraphDeltaDescriptor> {
    let descriptor = delta.descriptor().clone();
    digest::descriptor_bounds(&descriptor)?;
    let record_count: u32 = transaction.query_row(
        "SELECT COUNT(*) FROM pending_graph_records WHERE txn=?1",
        [txn.as_bytes().as_slice()],
        |row| row.get(0),
    )?;
    let node_count: u32 = transaction.query_row(
        "SELECT COUNT(*) FROM pending_graph_merkle WHERE txn=?1",
        [txn.as_bytes().as_slice()],
        |row| row.get(0),
    )?;
    ensure!(
        record_count == 0 && node_count == 0,
        "graph transaction ID already staged"
    );
    let mut counts = descriptor.base.counts();
    let mut root = descriptor.base.merkle_root;
    let mut records = digest::Stream::new(true, &descriptor.base, &descriptor.target)?;
    delta.visit_records(&mut|id,record|{
        ensure!(records.count<descriptor.record_count,"extra staged graph record");
        records.record(&descriptor.target.scope,id,record)?;
        let proof=sql::read_proof(transaction,&descriptor.base,id)?;
        staging::update_counts(&mut counts,&descriptor.base,id,proof.record.as_ref(),record)?;
        root=merkle::update_path(&descriptor.base.scope,id,proof.record.as_ref(),record,(&proof.siblings,root),
            |height,index|pending_node(transaction,txn,height,index),
            |node|{
                put_pending_node(transaction,txn,node)?;
                Ok(())
            })?;
        transaction.execute("INSERT OR ROLLBACK INTO pending_graph_records(txn,id,embedding_key,live,record) VALUES(?1,?2,?3,?4,?5)",
            params![txn.as_bytes().as_slice(),i64::from(id),record.embedding_key.map(|key|key.as_bytes().to_vec()),i64::from(record.live),&record.bytes])?;
        Ok(())
    })?;
    ensure!(
        records.count == descriptor.record_count
            && records.bytes == descriptor.record_bytes
            && records.finish() == descriptor.records_digest,
        "staged graph record digest mismatch"
    );
    ensure!(
        counts == descriptor.target.counts() && root == descriptor.target.merkle_root,
        "staged graph target root/count mismatch"
    );
    let computed: u32 = transaction.query_row(
        "SELECT COUNT(*) FROM pending_graph_merkle WHERE txn=?1",
        [txn.as_bytes().as_slice()],
        |row| row.get(0),
    )?;
    ensure!(
        u64::from(computed) == descriptor.node_count,
        "staged graph node count mismatch"
    );
    let mut nodes = digest::Stream::new(false, &descriptor.base, &descriptor.target)?;
    delta.visit_nodes(&mut |node| {
        ensure!(
            nodes.count < descriptor.node_count,
            "extra staged graph node"
        );
        nodes.node(node)?;
        ensure!(
            pending_node(transaction, txn, node.height, node.index)? == Some(node.digest),
            "staged graph node differs from record-derived proof"
        );
        Ok(())
    })?;
    ensure!(
        nodes.count == descriptor.node_count && nodes.finish() == descriptor.nodes_digest,
        "staged graph node digest mismatch"
    );
    Ok(descriptor)
}

fn validate_pending(
    connection: &Connection,
    txn: Sha256Digest,
    descriptor: &GraphDeltaDescriptor,
) -> Result<()> {
    digest::descriptor_bounds(descriptor)?;
    let mut records = digest::Stream::new(true, &descriptor.base, &descriptor.target)?;
    let mut statement = connection.prepare_cached(
        "SELECT id,embedding_key,live,
        CASE WHEN typeof(record)='blob' AND length(record)=?2 THEN record END
        FROM pending_graph_records WHERE txn=?1 ORDER BY id",
    )?;
    let mut rows = statement.query(params![
        txn.as_bytes().as_slice(),
        descriptor.target.scope.record_bytes
    ])?;
    while let Some(row) = rows.next()? {
        ensure!(
            records.count < descriptor.record_count,
            "extra pending graph record"
        );
        let id = u32::try_from(row.get::<_, i64>(0)?)?;
        let record = sql::decode_record(
            &descriptor.target.scope,
            id,
            (row.get(1)?, row.get(2)?, row.get(3)?),
        )?;
        ensure!(
            u64::from(id) < descriptor.target.allocated_count,
            "pending graph ID exceeds target allocation"
        );
        records.record(&descriptor.target.scope, id, &record)?;
    }
    ensure!(
        records.count == descriptor.record_count
            && records.bytes == descriptor.record_bytes
            && records.finish() == descriptor.records_digest,
        "pending graph record digest mismatch"
    );
    let mut nodes = digest::Stream::new(false, &descriptor.base, &descriptor.target)?;
    let mut statement = connection.prepare_cached(
        "SELECT height,idx,digest FROM pending_graph_merkle WHERE txn=?1 ORDER BY height,idx",
    )?;
    let mut rows = statement.query([txn.as_bytes().as_slice()])?;
    while let Some(row) = rows.next()? {
        ensure!(
            nodes.count < descriptor.node_count,
            "extra pending graph node"
        );
        nodes.node(GraphMerkleNode {
            height: u8::try_from(row.get::<_, i64>(0)?)?,
            index: u32::try_from(row.get::<_, i64>(1)?)?,
            digest: Sha256Digest::from_bytes(row.get(2)?),
        })?;
    }
    ensure!(
        nodes.count == descriptor.node_count && nodes.finish() == descriptor.nodes_digest,
        "pending graph node digest mismatch"
    );
    if descriptor.record_count > 0 {
        ensure!(
            pending_node(connection, txn, 32, 0)? == Some(descriptor.target.merkle_root),
            "pending graph root mismatch"
        );
    } else {
        ensure!(
            descriptor.base == descriptor.target && descriptor.node_count == 0,
            "empty graph delta changes head"
        );
    }
    Ok(())
}

pub(crate) fn apply(
    transaction: &Transaction<'_>,
    txn: Sha256Digest,
    descriptor: &GraphDeltaDescriptor,
) -> Result<()> {
    validate_pending(transaction, txn, descriptor)?;
    // OR ROLLBACK avoids statement journals outside the current owner's reserved
    // rollback-journal allowance. SQLite UPSERT's DO UPDATE uses ABORT regardless
    // of the INSERT conflict clause, so use explicit update/insert pairs instead.
    let mut statement = transaction.prepare_cached(
        "SELECT id,embedding_key,live,
        CASE WHEN typeof(record)='blob' AND length(record)=?2 THEN record END
        FROM pending_graph_records WHERE txn=?1 ORDER BY id",
    )?;
    let mut rows = statement.query(params![
        txn.as_bytes().as_slice(),
        descriptor.target.scope.record_bytes
    ])?;
    while let Some(row) = rows.next()? {
        let id = u32::try_from(row.get::<_, i64>(0)?)?;
        let record = sql::decode_record(
            &descriptor.target.scope,
            id,
            (row.get(1)?, row.get(2)?, row.get(3)?),
        )?;
        let key = record.embedding_key.map(|key| key.as_bytes().to_vec());
        let values = params![i64::from(id), key, i64::from(record.live), record.bytes];
        if transaction.execute(
            "UPDATE OR ROLLBACK graph_records SET embedding_key=?2,live=?3,record=?4 WHERE id=?1",
            values,
        )? == 0
        {
            transaction.execute("INSERT OR ROLLBACK INTO graph_records(id,embedding_key,live,record) VALUES(?1,?2,?3,?4)",values)?;
        }
    }
    drop(rows);
    drop(statement);
    let mut statement = transaction.prepare_cached(
        "SELECT height,idx,digest FROM pending_graph_merkle WHERE txn=?1 ORDER BY height,idx",
    )?;
    let mut rows = statement.query([txn.as_bytes().as_slice()])?;
    while let Some(row) = rows.next()? {
        let height = row.get::<_, u8>(0)?;
        let index = row.get::<_, u32>(1)?;
        let digest = row.get::<_, [u8; 32]>(2)?;
        let values = params![height, index, digest.as_slice()];
        if transaction.execute(
            "UPDATE OR ROLLBACK graph_merkle SET digest=?3 WHERE height=?1 AND idx=?2",
            values,
        )? == 0
        {
            transaction.execute(
                "INSERT OR ROLLBACK INTO graph_merkle(height,idx,digest) VALUES(?1,?2,?3)",
                values,
            )?;
        }
    }
    drop(rows);
    drop(statement);
    cleanup(transaction, txn)
}

pub(crate) fn cleanup(transaction: &Transaction<'_>, txn: Sha256Digest) -> Result<()> {
    transaction.execute(
        "DELETE FROM pending_graph_records WHERE txn=?1",
        [txn.as_bytes().as_slice()],
    )?;
    transaction.execute(
        "DELETE FROM pending_graph_merkle WHERE txn=?1",
        [txn.as_bytes().as_slice()],
    )?;
    Ok(())
}
