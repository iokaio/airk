// SPDX-License-Identifier: Apache-2.0
//! Qualified single-current-index storage foundation. Runtime adoption is separate.
mod database;
mod gate;
pub mod graph;
mod journal;
pub mod map;
mod read;
pub mod vector_map;

pub use database::{DATABASE_VERSION, PAGE_BYTES, journal_bound};
pub use journal::{
    ArtifactChange, CapturedInput, CapturedInputs, CurrentCapacityChange, CurrentCommit,
    CurrentWriter, PendingTransaction, TransactionPhase,
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
pub const MAX_LEXICAL_MANIFEST_BYTES: usize = 1024 * 1024;

pub(crate) fn verify_lexical_manifest(bytes: &[u8], expected: Sha256Digest) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_LEXICAL_MANIFEST_BYTES,
        "current lexical manifest byte bound exceeded"
    );
    serde_json::from_slice::<serde_json::Value>(bytes)?;
    ensure!(
        framed_digest(b"current-lexical-manifest/1", &[bytes]) == expected,
        "current lexical manifest digest mismatch"
    );
    Ok(())
}

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
    /// Sequence whose immutable Tantivy directory is selected by this head.
    /// Graph-only commits retain the prior lexical sequence.
    pub lexical_sequence: u64,
    pub lexical_commit: Sha256Digest,
    pub graph_head: GraphHead,
}

impl CurrentHead {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        self.graph_head.validate()?;
        ensure!(
            self.version == 2
                && self.sequence <= i64::MAX as u64
                && self.lexical_sequence <= self.sequence,
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
                self.artifact_root == map::empty_root(&self.scope)? && self.lexical_sequence == 0,
                "initial current catalog is not empty"
            );
        }
        Ok(())
    }
    pub fn lexical_directory_name(&self) -> Result<String> {
        self.validate()?;
        Ok(format!("lexical-{}", self.lexical_sequence))
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
            digest: framed_digest(b"current-index-head/2", &[&self.canonical_bytes()?]),
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

impl crate::FileStore {
    /// Read the captured capacity bound before opening a recovering writer.
    pub fn current_database_capacity(&self, scope: &CurrentScope) -> Result<u64> {
        scope.validate()?;
        ensure!(
            self.directory().file_name().and_then(|name| name.to_str())
                == Some(scope.collection.as_str()),
            "foreign current capacity collection"
        );
        database::check_owner(self.directory(), scope)
    }
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
