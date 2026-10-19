// SPDX-License-Identifier: Apache-2.0
//! Canonical streams: fixed magic, framed domain/base/target JSON, then framed
//! ordered record or node fields and final row/payload-byte counts. These hashes
//! authenticate logical afterimages, not SQLite page layout or insertion history.
use super::{MAX_DIRTY_RECORDS, MAX_OVERLAY_BYTES};
use airk_core::{graph_storage::*, identity::Sha256Digest};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};

pub(super) struct Stream {
    hash: Sha256,
    records: bool,
    last_record: Option<u32>,
    last_node: Option<(u8, u32)>,
    pub count: u64,
    pub bytes: u64,
}
impl Stream {
    pub fn new(records: bool, base: &GraphHead, target: &GraphHead) -> Result<Self> {
        base.validate()?;
        target.validate()?;
        let mut hash = Sha256::new();
        hash.update(b"AIRK-GRAPH-DELTA-1\0");
        frame(
            &mut hash,
            if records {
                b"current-graph-record-afterimages/1"
            } else {
                b"current-graph-node-afterimages/1"
            },
        );
        frame(&mut hash, &serde_json::to_vec(base)?);
        frame(&mut hash, &serde_json::to_vec(target)?);
        Ok(Self {
            hash,
            records,
            last_record: None,
            last_node: None,
            count: 0,
            bytes: 0,
        })
    }
    pub fn record(&mut self, scope: &GraphScope, id: u32, record: &GraphRecord) -> Result<()> {
        ensure!(
            self.records && self.last_record.is_none_or(|previous| previous < id),
            "graph record afterimages unordered or duplicated"
        );
        record.validate(scope, id)?;
        ensure!(
            self.count < MAX_DIRTY_RECORDS
                && record.bytes.len() as u64 <= MAX_OVERLAY_BYTES.saturating_sub(self.bytes),
            "graph record stream exceeds bounds"
        );
        frame(&mut self.hash, &id.to_be_bytes());
        frame(
            &mut self.hash,
            record
                .embedding_key
                .as_ref()
                .map_or(&[][..], |key| key.as_bytes().as_slice()),
        );
        frame(&mut self.hash, &[u8::from(record.live)]);
        frame(&mut self.hash, &record.bytes);
        self.last_record = Some(id);
        self.count += 1;
        self.bytes += record.bytes.len() as u64;
        Ok(())
    }
    pub fn node(&mut self, node: GraphMerkleNode) -> Result<()> {
        node.validate()?;
        let coordinate = (node.height, node.index);
        ensure!(
            !self.records && self.last_node.is_none_or(|previous| previous < coordinate),
            "graph node afterimages unordered or duplicated"
        );
        ensure!(
            self.count < MAX_DIRTY_RECORDS * 33,
            "graph node stream exceeds bound"
        );
        frame(&mut self.hash, &[node.height]);
        frame(&mut self.hash, &node.index.to_be_bytes());
        frame(&mut self.hash, node.digest.as_bytes());
        self.last_node = Some(coordinate);
        self.count += 1;
        self.bytes += 37;
        Ok(())
    }
    pub fn finish(mut self) -> Sha256Digest {
        frame(&mut self.hash, &self.count.to_be_bytes());
        frame(&mut self.hash, &self.bytes.to_be_bytes());
        Sha256Digest::from_bytes(self.hash.finalize().into())
    }
}
fn frame(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
}

pub(super) fn descriptor_bounds(descriptor: &GraphDeltaDescriptor) -> Result<()> {
    descriptor.validate()?;
    ensure!(
        descriptor.record_count <= MAX_DIRTY_RECORDS
            && descriptor.record_bytes <= MAX_OVERLAY_BYTES,
        "graph delta exceeds store bounds"
    );
    Ok(())
}
