// SPDX-License-Identifier: Apache-2.0
//! Checksummed private graph rows over a reservation-owned redb transaction.
use super::{PAGE_ROWS, kv};
use crate::quota::{PrivateKv, PrivateTable};
use airk_core::{graph_storage::*, identity::Sha256Digest};
use anyhow::{Result, ensure};

fn decode(scope: &GraphScope, id: u32, bytes: &[u8]) -> Result<GraphRecord> {
    ensure!(bytes.len() >= 32, "private graph row truncated");
    let record = kv::decode(scope, id, &bytes[32..])?;
    ensure!(
        leaf_hash(scope, id, &record)?.as_bytes().as_slice() == &bytes[..32],
        "private graph row checksum mismatch"
    );
    Ok(record)
}
pub(super) fn dirty_record(
    database: &PrivateKv,
    scope: &GraphScope,
    id: u32,
) -> Result<Option<GraphRecord>> {
    database
        .get(PrivateTable::Records, &id.to_be_bytes())?
        .map(|bytes| decode(scope, id, &bytes))
        .transpose()
}
pub(super) fn put_record(
    database: &PrivateKv,
    scope: &GraphScope,
    id: u32,
    record: &GraphRecord,
) -> Result<bool> {
    let mut bytes = leaf_hash(scope, id, record)?.as_bytes().to_vec();
    bytes.extend_from_slice(&kv::encode(record));
    database.put(PrivateTable::Records, &id.to_be_bytes(), &bytes)
}
fn node_key(height: u8, index: u32) -> Result<[u8; 5]> {
    validate_coordinate(height, index)?;
    let mut key = [0; 5];
    key[0] = height;
    key[1..].copy_from_slice(&index.to_be_bytes());
    Ok(key)
}
pub(super) fn node(database: &PrivateKv, height: u8, index: u32) -> Result<Option<Sha256Digest>> {
    database
        .get(PrivateTable::Nodes, &node_key(height, index)?)?
        .map(|bytes| Ok(Sha256Digest::from_bytes(bytes.as_slice().try_into()?)))
        .transpose()
}
pub(super) fn put_node(database: &PrivateKv, node: GraphMerkleNode) -> Result<()> {
    database.put(
        PrivateTable::Nodes,
        &node_key(node.height, node.index)?,
        node.digest.as_bytes(),
    )?;
    Ok(())
}
pub(super) fn record_page(
    database: &PrivateKv,
    scope: &GraphScope,
    after: Option<u32>,
) -> Result<Vec<(u32, GraphRecord)>> {
    let after = after.map(u32::to_be_bytes);
    database
        .page(
            PrivateTable::Records,
            after.as_ref().map(|key| key.as_slice()),
            PAGE_ROWS,
        )?
        .into_iter()
        .map(|(key, value)| {
            let id = u32::from_be_bytes(key.as_slice().try_into()?);
            Ok((id, decode(scope, id, &value)?))
        })
        .collect()
}
pub(super) fn node_page(
    database: &PrivateKv,
    after: Option<(u8, u32)>,
) -> Result<Vec<GraphMerkleNode>> {
    let after = after
        .map(|(height, index)| node_key(height, index))
        .transpose()?;
    database
        .page(
            PrivateTable::Nodes,
            after.as_ref().map(|key| key.as_slice()),
            PAGE_ROWS,
        )?
        .into_iter()
        .map(|(key, value)| {
            let node = GraphMerkleNode {
                height: key[0],
                index: u32::from_be_bytes(key[1..].try_into()?),
                digest: Sha256Digest::from_bytes(value.as_slice().try_into()?),
            };
            node.validate()?;
            Ok(node)
        })
        .collect()
}
