// SPDX-License-Identifier: Apache-2.0
//! Qualified single-current-index storage foundation. Runtime adoption is separate.
mod database;
mod gate;
pub mod graph;
mod journal;
pub mod map;
mod read;

pub use database::{DATABASE_VERSION, PAGE_BYTES, journal_bound};
pub use journal::{
    CapturedInput, CapturedInputs, CurrentCapacityChange, CurrentCommit, CurrentWriter,
    PendingTransaction, TransactionPhase,
};
pub use read::CurrentRead;

use airk_core::{
    graph_storage::GraphHead,
    identity::{Incarnation, Sha256Digest, framed_digest},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_HEAD_BYTES: usize = 65_536;
pub const MAX_PENDING_INPUTS: usize = 65_536;
pub const MAX_INPUT_METADATA_BYTES: usize = 262_144;
pub const MAX_PENDING_METADATA_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentScope {
    pub root: String,
    pub collection: String,
    pub incarnation: Incarnation,
}

impl CurrentScope {
    pub fn validate(&self) -> Result<()> {
        crate::validate_collection(&self.collection)?;
        ensure!(
            !self.root.is_empty() && self.root.len() <= 32_768 && !self.root.contains('\0'),
            "invalid current source root"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentHead {
    pub version: u32,
    pub scope: CurrentScope,
    pub sequence: u64,
    /// Authentication root for per-artifact and keyed current mappings.
    pub artifact_root: Sha256Digest,
    pub lexical_commit: Sha256Digest,
    pub graph_head: GraphHead,
}

impl CurrentHead {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        self.graph_head.validate()?;
        ensure!(
            self.version == 1 && self.sequence <= i64::MAX as u64,
            "unsupported current head"
        );
        let graph = &self.graph_head.scope;
        ensure!(
            graph.collection == self.scope.collection
                && graph.root == self.scope.root
                && graph.incarnation == self.scope.incarnation,
            "current graph belongs to another collection"
        );
        if self.sequence == 0 {
            ensure!(
                self.artifact_root == map::empty_root(&self.scope)?,
                "initial current catalog is not empty"
            );
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_HEAD_BYTES,
            "current head exceeds byte bound"
        );
        Ok(bytes)
    }
    pub fn token(&self) -> Result<CurrentToken> {
        Ok(CurrentToken {
            incarnation: self.scope.incarnation,
            sequence: self.sequence,
            digest: framed_digest(b"current-index-head/1", &[&self.canonical_bytes()?]),
        })
    }
}

/// Equality is the only supported revision relationship. There is no ancestry API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentToken {
    pub incarnation: Incarnation,
    pub sequence: u64,
    pub digest: Sha256Digest,
}

pub(crate) fn decode_head(bytes: &[u8], scope: &CurrentScope) -> Result<CurrentHead> {
    ensure!(
        bytes.len() <= MAX_HEAD_BYTES,
        "current head exceeds byte bound"
    );
    let head: CurrentHead = serde_json::from_slice(bytes)?;
    ensure!(
        head.canonical_bytes()? == bytes && &head.scope == scope,
        "foreign or noncanonical current head"
    );
    Ok(head)
}

#[cfg(test)]
mod tests;
