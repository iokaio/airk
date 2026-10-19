// SPDX-License-Identifier: Apache-2.0
//! Exact current vector identity and bounded citation occurrence pointers.
//! These are authenticated map values, not ANN search results or evidence bytes.
use super::{CurrentHead, map};
use airk_core::identity::Sha256Digest;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_VECTOR_OCCURRENCES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorBinding {
    pub version: u32,
    pub embedding_key: Sha256Digest,
    pub graph_scope: Sha256Digest,
    pub graph_id: u32,
    /// Sorted, unique IDs. Empty means a retained but non-live graph tombstone.
    pub occurrence_ids: Vec<Sha256Digest>,
}

impl VectorBinding {
    pub fn validate(&self, head: &CurrentHead) -> Result<()> {
        head.validate()?;
        ensure!(
            self.version == 1
                && self.graph_scope == head.graph_head.scope.digest()
                && self.graph_id != 0
                && u64::from(self.graph_id) < head.graph_head.allocated_count
                && self.occurrence_ids.len() <= MAX_VECTOR_OCCURRENCES
                && self.occurrence_ids.windows(2).all(|pair| pair[0] < pair[1]),
            "invalid current vector binding"
        );
        Ok(())
    }

    pub fn canonical_bytes(&self, head: &CurrentHead) -> Result<Vec<u8>> {
        self.validate(head)?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= map::MAX_VALUE_BYTES,
            "vector binding exceeds map value limit"
        );
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8], head: &CurrentHead, expected: Sha256Digest) -> Result<Self> {
        ensure!(
            bytes.len() <= map::MAX_VALUE_BYTES,
            "oversized vector binding"
        );
        let binding: Self = serde_json::from_slice(bytes)?;
        ensure!(
            binding.embedding_key == expected && binding.canonical_bytes(head)? == bytes,
            "current vector binding key or encoding mismatch"
        );
        Ok(binding)
    }
}

/// The occurrence payload is a retained, digest-addressed object. A caller must
/// verify that object and decode its citation body before returning evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrencePointer {
    pub version: u32,
    pub embedding_key: Sha256Digest,
    pub occurrence_id: Sha256Digest,
    pub path: String,
    pub source_hash: Sha256Digest,
    pub payload_digest: Sha256Digest,
    pub object: Sha256Digest,
}

impl OccurrencePointer {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && !self.path.is_empty()
                && self.path.len() <= 8192
                && !self.path.contains(['\\', ':', '\0'])
                && self
                    .path
                    .split('/')
                    .all(|part| !matches!(part, "" | "." | "..")),
            "invalid current occurrence pointer"
        );
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= map::MAX_VALUE_BYTES,
            "occurrence pointer exceeds map value limit"
        );
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8], expected: Sha256Digest) -> Result<Self> {
        ensure!(
            bytes.len() <= map::MAX_VALUE_BYTES,
            "oversized occurrence pointer"
        );
        let pointer: Self = serde_json::from_slice(bytes)?;
        ensure!(
            pointer.occurrence_id == expected && pointer.canonical_bytes()? == bytes,
            "current occurrence pointer key or encoding mismatch"
        );
        Ok(pointer)
    }
}

#[cfg(test)]
mod tests;
