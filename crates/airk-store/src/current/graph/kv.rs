// SPDX-License-Identifier: Apache-2.0
//! Typed redb graph records using the existing DiskANN codec and authentication domains.
use super::{GraphStorage, digest, merkle, proof, staging};
use crate::current::kv::tables;
use airk_core::{graph_storage::*, identity::Sha256Digest};
use anyhow::{Result, ensure};
use redb::{ReadableTable, ReadableTableMetadata};

pub(super) fn encode(record: &GraphRecord) -> Vec<u8> {
    let mut bytes = vec![
        u8::from(record.embedding_key.is_some()),
        u8::from(record.live),
    ];
    if let Some(key) = record.embedding_key {
        bytes.extend_from_slice(key.as_bytes());
    }
    bytes.extend_from_slice(&record.bytes);
    bytes
}
pub(super) fn decode(scope: &GraphScope, id: u32, bytes: &[u8]) -> Result<GraphRecord> {
    ensure!(
        bytes.len() >= 2 && bytes[0] <= 1 && bytes[1] <= 1,
        "invalid redb graph record markers"
    );
    let offset = 2 + usize::from(bytes[0]) * 32;
    ensure!(
        bytes.len() == offset + scope.record_bytes as usize,
        "invalid redb graph record length"
    );
    let record = GraphRecord {
        embedding_key: if bytes[0] == 1 {
            Some(Sha256Digest::from_bytes(bytes[2..34].try_into()?))
        } else {
            None
        },
        live: bytes[1] == 1,
        bytes: bytes[offset..].to_vec(),
    };
    record.validate(scope, id)?;
    Ok(record)
}
fn coordinate(height: u8, index: u32) -> Result<u64> {
    validate_coordinate(height, index)?;
    Ok((u64::from(height) << 32) | u64::from(index))
}

/// Capture typed afterimages in canonical journal order. Replacements also
/// retire old rows absent from the new graph; no old-instance node may leak
/// into a target proof. The caller binds both descriptor heads to publication.
pub fn visit_operations(
    base: &redb::ReadTransaction,
    delta: &dyn PreparedGraphDelta,
    visitor: &mut dyn FnMut(&crate::current::kv::journal::Operation) -> Result<()>,
) -> Result<()> {
    use crate::current::kv::journal::{Operation, Table};
    let descriptor = delta.descriptor();
    digest::descriptor_bounds(descriptor)?;
    let mut emitted = 0usize;
    let mut emit = |operation: &Operation| {
        ensure!(
            emitted < crate::current::MAX_PENDING_MAP_CHANGES,
            "graph capture operation bound exceeded"
        );
        emitted += 1;
        visitor(operation)
    };
    let replacing = descriptor.base.scope != descriptor.target.scope;
    let old = base.open_table(tables::GRAPH_RECORD)?;
    let mut previous = old.iter()?;
    let mut next = if replacing {
        previous.next().transpose()?.map(|(k, _)| k.value())
    } else {
        None
    };
    let mut records = digest::Stream::new(true, &descriptor.base, &descriptor.target)?;
    delta.visit_records(&mut |id, record| {
        ensure!(
            u64::from(id) < descriptor.target.allocated_count,
            "captured graph record outside target allocation"
        );
        records.record(&descriptor.target.scope, id, record)?;
        while next.is_some_and(|old| old < id) {
            emit(&Operation {
                table: Table::GraphRecord,
                key: next.unwrap().to_be_bytes().to_vec(),
                value: None,
            })?;
            next = previous.next().transpose()?.map(|(k, _)| k.value());
        }
        if next == Some(id) {
            next = previous.next().transpose()?.map(|(k, _)| k.value());
        }
        emit(&Operation {
            table: Table::GraphRecord,
            key: id.to_be_bytes().to_vec(),
            value: Some(encode(record)),
        })
    })?;
    while let Some(id) = next {
        emit(&Operation {
            table: Table::GraphRecord,
            key: id.to_be_bytes().to_vec(),
            value: None,
        })?;
        next = previous.next().transpose()?.map(|(k, _)| k.value());
    }
    ensure!(
        records.count == descriptor.record_count
            && records.bytes == descriptor.record_bytes
            && records.finish() == descriptor.records_digest,
        "captured graph record stream mismatch"
    );
    let old = base.open_table(tables::GRAPH_NODE)?;
    let mut previous = old.iter()?;
    let mut next = if replacing {
        previous.next().transpose()?.map(|(k, _)| k.value())
    } else {
        None
    };
    let mut nodes = digest::Stream::new(false, &descriptor.base, &descriptor.target)?;
    delta.visit_nodes(&mut |node| {
        nodes.node(node)?;
        let key = coordinate(node.height, node.index)?;
        while next.is_some_and(|old| old < key) {
            emit(&Operation {
                table: Table::GraphNode,
                key: next.unwrap().to_be_bytes().to_vec(),
                value: None,
            })?;
            next = previous.next().transpose()?.map(|(k, _)| k.value());
        }
        if next == Some(key) {
            next = previous.next().transpose()?.map(|(k, _)| k.value());
        }
        emit(&Operation {
            table: Table::GraphNode,
            key: key.to_be_bytes().to_vec(),
            value: (node.digest != empty_hash(&descriptor.target.scope, node.height)?)
                .then(|| node.digest.as_bytes().to_vec()),
        })
    })?;
    while let Some(key) = next {
        emit(&Operation {
            table: Table::GraphNode,
            key: key.to_be_bytes().to_vec(),
            value: None,
        })?;
        next = previous.next().transpose()?.map(|(k, _)| k.value());
    }
    ensure!(
        nodes.count == descriptor.node_count && nodes.finish() == descriptor.nodes_digest,
        "captured graph node stream mismatch"
    );
    Ok(())
}
macro_rules! graph_storage {
    ($transaction:ty) => {
        impl GraphStorage for $transaction {
            fn graph_record(&self, scope: &GraphScope, id: u32) -> Result<Option<GraphRecord>> {
                self.open_table(tables::GRAPH_RECORD)?
                    .get(id)?
                    .map(|value| decode(scope, id, value.value()))
                    .transpose()
            }
            fn graph_node(&self, height: u8, index: u32) -> Result<Option<Sha256Digest>> {
                self.open_table(tables::GRAPH_NODE)?
                    .get(coordinate(height, index)?)?
                    .map(|value| {
                        ensure!(value.value().len() == 32, "invalid redb graph node length");
                        Ok(Sha256Digest::from_bytes(value.value().try_into()?))
                    })
                    .transpose()
            }
        }
    };
}
graph_storage!(redb::ReadTransaction);
graph_storage!(redb::WriteTransaction);

pub fn read(transaction: &redb::ReadTransaction, head: &GraphHead, id: u32) -> Result<GraphRecord> {
    super::proof::read_record(transaction, head, id)
}

/// Apply a validated, bounded retained delta inside the caller's atomic transaction.
/// Any error requires aborting that transaction, including a final descriptor mismatch.
pub fn apply(
    transaction: &redb::WriteTransaction,
    delta: &dyn PreparedGraphDelta,
) -> Result<GraphHead> {
    let descriptor = delta.descriptor();
    digest::descriptor_bounds(descriptor)?;
    let replacing = descriptor.base.scope != descriptor.target.scope;
    let mut head = if replacing {
        transaction
            .open_table(tables::GRAPH_RECORD)?
            .retain(|_, _| false)?;
        transaction
            .open_table(tables::GRAPH_NODE)?
            .retain(|_, _| false)?;
        GraphHead::empty(descriptor.target.scope.clone())?
    } else {
        descriptor.base.clone()
    };
    let base = head.clone();
    let mut records = digest::Stream::new(true, &descriptor.base, &descriptor.target)?;
    delta.visit_records(&mut |id, record| {
        ensure!(
            records.count < descriptor.record_count,
            "extra redb graph afterimage"
        );
        records.record(&head.scope, id, record)?;
        let proof = proof::read_proof(transaction, &head, id)?;
        let mut counts = head.counts();
        staging::update_counts(&mut counts, &base, id, proof.record.as_ref(), record)?;
        let root = merkle::update_path(
            &head.scope,
            id,
            proof.record.as_ref(),
            record,
            (&proof.siblings, head.merkle_root),
            |_, _| Ok(None),
            |node| {
                let key = coordinate(node.height, node.index)?;
                let mut table = transaction.open_table(tables::GRAPH_NODE)?;
                if node.digest == empty_hash(&head.scope, node.height)? {
                    table.remove(key)?;
                } else {
                    table.insert(key, node.digest.as_bytes().as_slice())?;
                }
                Ok(())
            },
        )?;
        transaction
            .open_table(tables::GRAPH_RECORD)?
            .insert(id, encode(record).as_slice())?;
        head.allocated_count = counts.allocated_count;
        head.live_nodes = counts.live_nodes;
        head.merkle_root = root;
        Ok(())
    })?;
    ensure!(
        records.count == descriptor.record_count
            && records.bytes == descriptor.record_bytes
            && records.finish() == descriptor.records_digest,
        "redb graph record stream mismatch"
    );
    ensure!(
        head == descriptor.target,
        "redb graph target root/count mismatch"
    );
    let mut nodes = digest::Stream::new(false, &descriptor.base, &descriptor.target)?;
    delta.visit_nodes(&mut |node| {
        ensure!(
            nodes.count < descriptor.node_count,
            "extra redb graph node afterimage"
        );
        nodes.node(node)?;
        let stored = transaction
            .graph_node(node.height, node.index)?
            .unwrap_or(empty_hash(&head.scope, node.height)?);
        ensure!(
            stored == node.digest,
            "redb graph node differs from derived record proof"
        );
        Ok(())
    })?;
    ensure!(
        nodes.count == descriptor.node_count && nodes.finish() == descriptor.nodes_digest,
        "redb graph node stream mismatch"
    );
    Ok(head)
}

pub fn verify_all(
    transaction: &redb::ReadTransaction,
    head: &GraphHead,
    maximum_records: u64,
) -> Result<()> {
    let records = transaction.open_table(tables::GRAPH_RECORD)?.len()?;
    verify_storage(transaction, head, maximum_records, records)?;
    verify_node_rows(&transaction.open_table(tables::GRAPH_NODE)?, head)
}
pub fn verify_write(
    transaction: &redb::WriteTransaction,
    head: &GraphHead,
    maximum_records: u64,
) -> Result<()> {
    let records = transaction.open_table(tables::GRAPH_RECORD)?.len()?;
    verify_storage(transaction, head, maximum_records, records)?;
    verify_node_rows(&transaction.open_table(tables::GRAPH_NODE)?, head)
}
fn verify_storage(
    transaction: &dyn GraphStorage,
    head: &GraphHead,
    maximum_records: u64,
    records: u64,
) -> Result<()> {
    head.validate()?;
    ensure!(
        head.allocated_count <= maximum_records,
        "graph verification budget exceeded"
    );
    let mut live = 0u64;
    for id in 0..head.allocated_count {
        let record = proof::read_record(transaction, head, u32::try_from(id)?)?;
        record.validate(&head.scope, u32::try_from(id)?)?;
        live += u64::from(record.live);
        ensure!(
            transaction.graph_node(0, id as u32)?
                == Some(leaf_hash(&head.scope, id as u32, &record)?),
            "redb graph leaf node mismatch"
        );
        for height in 1..=32u8 {
            if id % (1u64 << height) != 0 {
                continue;
            }
            let index = (id >> height) as u32;
            let left = transaction
                .graph_node(height - 1, index * 2)?
                .unwrap_or(empty_hash(&head.scope, height - 1)?);
            let right = transaction
                .graph_node(height - 1, index * 2 + 1)?
                .unwrap_or(empty_hash(&head.scope, height - 1)?);
            let expected = branch_hash(&head.scope, height, index, left, right)?;
            ensure!(
                transaction.graph_node(height, index)? == Some(expected),
                "redb graph branch node mismatch"
            );
            if height == 32 {
                ensure!(expected == head.merkle_root, "redb graph root mismatch");
            }
        }
    }
    ensure!(live == head.live_nodes, "redb graph live count mismatch");
    ensure!(
        records == head.allocated_count,
        "unexpected redb graph record count"
    );
    Ok(())
}
fn verify_node_rows(
    nodes: &impl ReadableTable<u64, &'static [u8]>,
    head: &GraphHead,
) -> Result<()> {
    ensure!(
        nodes.len()? <= head.allocated_count.saturating_mul(33),
        "too many redb graph nodes"
    );
    for row in nodes.iter()? {
        let (key, value) = row?;
        let height = u8::try_from(key.value() >> 32)?;
        let index = key.value() as u32;
        validate_coordinate(height, index)?;
        ensure!(
            (u64::from(index) << height) < head.allocated_count
                && value.value().len() == 32
                && value.value() != empty_hash(&head.scope, height)?.as_bytes(),
            "invalid/outside redb graph node"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{current::kv::Database, quota::Quota};
    use airk_core::identity::Incarnation;

    #[test]
    fn sealed_diskann_delta_retains_codec_root_and_rejects_tampering() -> Result<()> {
        let scope = GraphScope {
            collection: "graph-fixture".into(),
            root: "original-graph-source".into(),
            incarnation: Incarnation::from_bytes([1; 16]),
            graph_instance: Incarnation::from_bytes([2; 16]),
            specification: Sha256Digest::from_bytes([3; 32]),
            record_bytes: 32,
        };
        let directory = tempfile::tempdir()?;
        let quota = Quota::open(directory.path().join("data"))?;
        let reservation = quota.reserve(&scope.collection, 8 << 20)?;
        let overlay = super::super::GraphOverlay::empty(
            &reservation,
            scope.clone(),
            super::super::GraphOverlayLimits {
                maximum_bytes: 1 << 20,
                maximum_dirty_records: 100,
            },
        )?;
        for id in 0u32..12 {
            overlay.append(
                id,
                GraphRecord {
                    embedding_key: (id != 0).then(|| Sha256Digest::from_bytes([id as u8; 32])),
                    live: id != 0,
                    bytes: vec![id as u8; 32],
                },
            )?;
        }
        let delta = overlay.seal()?;
        let file = tempfile::NamedTempFile::new()?;
        let database = Database::open(file.reopen()?, 16 << 20, true)?;
        let transaction = database.write()?;
        let head = apply(&transaction, &delta)?;
        assert_eq!(head, delta.descriptor().target);
        transaction.commit()?;
        let snapshot = database.snapshot()?;
        verify_all(snapshot.transaction(), &head, 12)?;
        for id in 0..12 {
            assert_eq!(
                read(snapshot.transaction(), &head, id)?.bytes,
                vec![id as u8; 32]
            );
        }
        let transaction = database.write()?;
        transaction.open_table(tables::GRAPH_RECORD)?.remove(4)?;
        transaction.commit()?;
        assert!(read(database.snapshot()?.transaction(), &head, 4).is_err());
        assert_eq!(read(snapshot.transaction(), &head, 4)?.bytes, vec![4; 32]);
        Ok(())
    }
}
