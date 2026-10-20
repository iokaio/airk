// SPDX-License-Identifier: Apache-2.0
//! Authenticated graph records and bounded private afterimages. Algorithms belong
//! to retrieval; transactions/gates belong to current. No authoritative connection
//! is opened here and no current reader survives sealing.
mod digest;
mod journal;
mod merkle;
mod sql;
mod staging;
#[cfg(test)]
mod tests;

use crate::quota::{PrivateDatabase, Reservation};
use airk_core::graph_storage::*;
use anyhow::{Context, Result, ensure};
use rusqlite::params;
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

pub(crate) use journal::{apply, cleanup, stage};
pub(crate) use sql::{create, read_record};

static NEXT_OVERLAY: AtomicU64 = AtomicU64::new(0);
const MAX_OVERLAY_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_DIRTY_RECORDS: u64 = 1_000_000;
const PAGE_ROWS: usize = 64;
const MAX_BASE_PROOFS: usize = 256;
const MAX_BASE_PROOF_CACHE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct GraphOverlayLimits {
    pub maximum_bytes: u64,
    pub maximum_dirty_records: u64,
}
impl Default for GraphOverlayLimits {
    fn default() -> Self {
        Self {
            maximum_bytes: 256 * 1024 * 1024,
            maximum_dirty_records: MAX_DIRTY_RECORDS,
        }
    }
}
impl GraphOverlayLimits {
    /// Leave at least three quarters of the remaining preparation allowance for
    /// retained objects, cache packs and lexical staging. SQLite charges this
    /// capacity before opening; it does not allocate the whole amount on disk.
    pub fn for_indexing(reservation: &Reservation) -> Self {
        Self {
            maximum_bytes: (reservation.remaining() / 4).clamp(16 * 1024, MAX_OVERLAY_BYTES),
            ..Self::default()
        }
    }

    fn validate(self) -> Result<()> {
        ensure!(
            (16 * 1024..=MAX_OVERLAY_BYTES).contains(&self.maximum_bytes)
                && (1..=MAX_DIRTY_RECORDS).contains(&self.maximum_dirty_records),
            "invalid graph overlay limits"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GraphStatistics {
    pub operations: u64,
    pub base_record_reads: u64,
    pub base_proof_cache_hits: u64,
    pub base_proof_cache_evictions: u64,
    pub proof_nodes: u64,
    /// Logical fixed-record/proof bytes processed, not physical device I/O.
    pub read_bytes: u64,
    pub dirty_reads: u64,
    pub write_operations: u64,
    pub dirty_records: u64,
    pub merkle_afterimages: u64,
    pub sealed_records: u64,
    pub private_page_bytes: u64,
}
struct State {
    database: PrivateDatabase,
    base: Option<Arc<super::CurrentRead>>,
    counts: GraphCounts,
    statistics: GraphStatistics,
    base_proofs: BTreeMap<u32, sql::Proof>,
    base_proof_order: VecDeque<u32>,
    failed: bool,
}
pub struct GraphOverlay {
    base_head: GraphHead,
    limits: GraphOverlayLimits,
    state: Mutex<State>,
}
impl GraphOverlay {
    pub fn new(
        reservation: &Reservation,
        base: Arc<super::CurrentRead>,
        limits: GraphOverlayLimits,
    ) -> Result<Self> {
        let head = base.head().graph_head.clone();
        Self::build(reservation, head, Some(base), limits)
    }
    /// Fresh deltas begin at zero allocation. Append the non-live reserved seed
    /// before opening the DiskANN provider or inserting ordinary nodes.
    pub fn empty(
        reservation: &Reservation,
        scope: GraphScope,
        limits: GraphOverlayLimits,
    ) -> Result<Self> {
        Self::build(reservation, GraphHead::empty(scope)?, None, limits)
    }
    fn build(
        reservation: &Reservation,
        base_head: GraphHead,
        base: Option<Arc<super::CurrentRead>>,
        limits: GraphOverlayLimits,
    ) -> Result<Self> {
        limits.validate()?;
        base_head.validate()?;
        // One reservation can publish several progressive batches. Each sealed
        // overlay remains private until that batch's journal has consumed it.
        let ordinal = NEXT_OVERLAY.fetch_add(1, Ordering::Relaxed);
        let database = reservation.open_private_sqlite(
            reservation
                .staging_directory()?
                .join(format!("current-graph-overlay-{ordinal}.sqlite3")),
            limits.maximum_bytes,
        )?;
        database
            .connection()
            .execute_batch(sql::OVERLAY_SCHEMA)
            .map_err(|error| database.write_error(error.into()))?;
        Ok(Self {
            limits,
            state: Mutex::new(State {
                database,
                base,
                counts: base_head.counts(),
                statistics: GraphStatistics::default(),
                base_proofs: BTreeMap::new(),
                base_proof_order: VecDeque::new(),
                failed: false,
            }),
            base_head,
        })
    }
    fn with_state<T>(&self, operation: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("graph overlay lock poisoned"))?;
        ensure!(!state.failed, "graph overlay failed; discard candidate");
        // Diagnostic only: repeated DiskANN neighbor visits are legitimate
        // work, not additional resident memory or retained storage.
        state.statistics.operations = state.statistics.operations.saturating_add(1);
        let result = operation(&mut state);
        if result.is_err() {
            state.failed = true;
        }
        result
    }
    fn record_read(&self, state: &mut State, bytes: u64) {
        state.statistics.read_bytes = state.statistics.read_bytes.saturating_add(bytes);
    }
    fn base_proof(&self, state: &mut State, id: u32) -> Result<sql::Proof> {
        self.record_read(
            state,
            u64::from(self.base_head.scope.record_bytes) + 32 * 32,
        );
        match &state.base {
            Some(base) => {
                if let Some(proof) = state.base_proofs.get(&id) {
                    state.statistics.base_proof_cache_hits += 1;
                    return Ok(proof.clone());
                }
                state.statistics.base_record_reads += 1;
                state.statistics.proof_nodes += 32;
                let proof = base.with_connection(|connection| {
                    sql::read_proof(connection, &self.base_head, id)
                })?;
                let bytes = usize::try_from(self.base_head.scope.record_bytes)? + 32 * 32;
                let capacity = (MAX_BASE_PROOF_CACHE_BYTES / bytes).clamp(1, MAX_BASE_PROOFS);
                if state.base_proof_order.len() == capacity {
                    let old = state
                        .base_proof_order
                        .pop_front()
                        .context("base proof cache order missing")?;
                    state.base_proofs.remove(&old);
                    state.statistics.base_proof_cache_evictions += 1;
                }
                state.base_proof_order.push_back(id);
                state.base_proofs.insert(id, proof.clone());
                Ok(proof)
            }
            None => sql::Proof::empty(&self.base_head, id),
        }
    }
    fn read_inner(&self, state: &mut State, id: u32) -> Result<GraphRecord> {
        ensure!(
            u64::from(id) < state.counts.allocated_count,
            "graph ID outside candidate allocation"
        );
        self.record_read(state, u64::from(self.base_head.scope.record_bytes) + 32);
        if let Some(record) =
            sql::dirty_record(state.database.connection(), &self.base_head.scope, id)?
        {
            state.statistics.dirty_reads += 1;
            return Ok(record);
        }
        self.base_proof(state, id)?
            .record
            .context("allocated graph record missing")
    }
    fn write_inner(&self, state: &mut State, id: u32, record: &GraphRecord) -> Result<()> {
        record.validate(&self.base_head.scope, id)?;
        let exists: bool = state.database.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM dirty_records WHERE id=?1)",
            [i64::from(id)],
            |row| row.get(0),
        )?;
        ensure!(
            exists || state.statistics.dirty_records < self.limits.maximum_dirty_records,
            "graph overlay dirty-record limit exceeded"
        );
        let checksum = leaf_hash(&self.base_head.scope, id, record)?;
        state.database.connection().execute(
            "INSERT INTO dirty_records(id,embedding_key,live,record,checksum) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET embedding_key=excluded.embedding_key,live=excluded.live,
             record=excluded.record,checksum=excluded.checksum",
            params![i64::from(id), record.embedding_key.map(|key| key.as_bytes().to_vec()),
                i64::from(record.live), &record.bytes, checksum.as_bytes().as_slice()])
            .map_err(|error| state.database.write_error(error.into()))?;
        state.statistics.write_operations += 1;
        state.statistics.dirty_records += u64::from(!exists);
        Ok(())
    }
    pub fn statistics(&self) -> Result<GraphStatistics> {
        self.with_state(|state| {
            let mut statistics = state.statistics.clone();
            statistics.private_page_bytes = sql::page_bytes(state.database.connection())?;
            Ok(statistics)
        })
    }
    /// Drop all provider clones and unwrap the owning Arc first. The returned
    /// delta owns no base reader, shared gate or callback into current state.
    pub fn seal(self) -> Result<SealedGraph> {
        let mut state = self
            .state
            .into_inner()
            .map_err(|_| anyhow::anyhow!("graph overlay lock poisoned"))?;
        ensure!(!state.failed, "graph overlay failed; discard candidate");
        let result = staging::seal(&self.base_head, self.limits, &mut state);
        state.base.take();
        let descriptor = result?;
        state
            .database
            .connection()
            .execute_batch("COMMIT;")
            .map_err(|error| state.database.write_error(error.into()))?;
        state.statistics.private_page_bytes = sql::page_bytes(state.database.connection())?;
        Ok(SealedGraph {
            database: state.database,
            descriptor,
            statistics: state.statistics,
        })
    }
}
impl GraphRecordReader for GraphOverlay {
    fn scope(&self) -> &GraphScope {
        &self.base_head.scope
    }
    fn counts(&self) -> Result<GraphCounts> {
        self.with_state(|state| Ok(state.counts))
    }
    fn read(&self, id: u32) -> Result<GraphRecord> {
        self.with_state(|state| self.read_inner(state, id))
    }
}
impl GraphRecordOverlay for GraphOverlay {
    fn append(&self, expected_id: u32, record: GraphRecord) -> Result<()> {
        self.with_state(|state| {
            ensure!(
                u64::from(expected_id) == state.counts.allocated_count
                    && state.counts.allocated_count < GRAPH_ID_CAPACITY,
                "graph append ID is not next allocation"
            );
            self.write_inner(state, expected_id, &record)?;
            state.counts.allocated_count += 1;
            state.counts.live_nodes += u64::from(record.live);
            Ok(())
        })
    }
    fn put(&self, id: u32, record: GraphRecord) -> Result<()> {
        self.with_state(|state| {
            let existing = self.read_inner(state, id)?;
            ensure!(
                existing.embedding_key == record.embedding_key && existing.live == record.live,
                "graph put cannot change key or live state"
            );
            self.write_inner(state, id, &record)
        })
    }
    fn set_live(&self, id: u32, live: bool) -> Result<()> {
        self.with_state(|state| {
            ensure!(id != 0, "reserved graph seed cannot change live state");
            let mut record = self.read_inner(state, id)?;
            if record.live != live {
                record.live = live;
                self.write_inner(state, id, &record)?;
                if live {
                    state.counts.live_nodes += 1;
                } else {
                    state.counts.live_nodes -= 1;
                }
            }
            Ok(())
        })
    }
    fn poison(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.failed = true;
        }
    }
}

pub struct SealedGraph {
    database: PrivateDatabase,
    descriptor: GraphDeltaDescriptor,
    statistics: GraphStatistics,
}
impl SealedGraph {
    /// Bind a complete independently built graph to the old head that the
    /// current journal will replace atomically. No old row is reused.
    pub fn replacing(mut self, base: GraphHead) -> Result<Self> {
        ensure!(
            self.descriptor.base.allocated_count == 0 && base.scope != self.descriptor.target.scope,
            "replacement graph must be built from empty"
        );
        let mut next = self.descriptor.clone();
        next.base = base;
        next.validate()?;
        let mut records = digest::Stream::new(true, &next.base, &next.target)?;
        self.visit_records(&mut |id, record| records.record(&next.target.scope, id, record))?;
        let mut nodes = digest::Stream::new(false, &next.base, &next.target)?;
        self.visit_nodes(&mut |node| nodes.node(node))?;
        next.records_digest = records.finish();
        next.nodes_digest = nodes.finish();
        self.descriptor = next;
        Ok(self)
    }

    pub fn statistics(&self) -> &GraphStatistics {
        &self.statistics
    }
}
impl PreparedGraphDelta for SealedGraph {
    fn descriptor(&self) -> &GraphDeltaDescriptor {
        &self.descriptor
    }
    fn visit_records(
        &self,
        visitor: &mut dyn FnMut(u32, &GraphRecord) -> Result<()>,
    ) -> Result<()> {
        staging::visit_records(self.database.connection(), &self.descriptor, visitor)
    }
    fn visit_nodes(&self, visitor: &mut dyn FnMut(GraphMerkleNode) -> Result<()>) -> Result<()> {
        staging::visit_nodes(self.database.connection(), &self.descriptor, visitor)
    }
}
