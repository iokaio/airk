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
use std::sync::{Arc, Mutex};

pub(crate) use journal::{apply, cleanup, stage};
pub(crate) use sql::{create, read_record};

const FILE_NAME: &str = "current-graph-overlay.sqlite3";
const MAX_OVERLAY_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_DIRTY_RECORDS: u64 = 1_000_000;
const MAX_OPERATIONS: u64 = 100_000_000;
const MAX_READ_BYTES: u64 = 128 * 1024 * 1024 * 1024;
const PAGE_ROWS: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct GraphOverlayLimits {
    pub maximum_bytes: u64,
    pub maximum_dirty_records: u64,
    pub maximum_operations: u64,
    pub maximum_read_bytes: u64,
}
impl Default for GraphOverlayLimits {
    fn default() -> Self {
        Self {
            maximum_bytes: 256 * 1024 * 1024,
            maximum_dirty_records: 65_536,
            maximum_operations: 10_000_000,
            maximum_read_bytes: 16 * 1024 * 1024 * 1024,
        }
    }
}
impl GraphOverlayLimits {
    fn validate(self) -> Result<()> {
        ensure!(
            (16 * 1024..=MAX_OVERLAY_BYTES).contains(&self.maximum_bytes)
                && (1..=MAX_DIRTY_RECORDS).contains(&self.maximum_dirty_records)
                && (1..=MAX_OPERATIONS).contains(&self.maximum_operations)
                && (1..=MAX_READ_BYTES).contains(&self.maximum_read_bytes),
            "invalid graph overlay limits"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GraphStatistics {
    pub operations: u64,
    pub base_record_reads: u64,
    pub proof_nodes: u64,
    /// Logical fixed-record/proof bytes admitted, not physical device I/O.
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
        let database = reservation.open_private_sqlite(
            reservation.staging_directory()?.join(FILE_NAME),
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
        let result = (|| {
            ensure!(
                state.statistics.operations < self.limits.maximum_operations,
                "graph overlay operation limit exceeded"
            );
            state.statistics.operations += 1;
            operation(&mut state)
        })();
        if result.is_err() {
            state.failed = true;
        }
        result
    }
    fn charge_read(&self, state: &mut State, bytes: u64) -> Result<()> {
        ensure!(
            bytes
                <= self
                    .limits
                    .maximum_read_bytes
                    .saturating_sub(state.statistics.read_bytes),
            "graph overlay read-byte limit exceeded"
        );
        state.statistics.read_bytes += bytes;
        Ok(())
    }
    fn base_proof(&self, state: &mut State, id: u32) -> Result<sql::Proof> {
        self.charge_read(
            state,
            u64::from(self.base_head.scope.record_bytes) + 32 * 32,
        )?;
        match &state.base {
            Some(base) => {
                state.statistics.base_record_reads += 1;
                state.statistics.proof_nodes += 32;
                base.with_connection(|connection| sql::read_proof(connection, &self.base_head, id))
            }
            None => sql::Proof::empty(&self.base_head, id),
        }
    }
    fn read_inner(&self, state: &mut State, id: u32) -> Result<GraphRecord> {
        ensure!(
            u64::from(id) < state.counts.allocated_count,
            "graph ID outside candidate allocation"
        );
        self.charge_read(state, u64::from(self.base_head.scope.record_bytes) + 32)?;
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
