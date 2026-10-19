// SPDX-License-Identifier: Apache-2.0
//! Exact current vector identity and bounded citation occurrence pointers.
//! These are authenticated map values, not ANN search results or evidence bytes.
use super::{CurrentHead, CurrentScope, journal::valid_current_path, map};
use airk_core::{
    extraction::MAX_EXTRACTION_CHUNKS,
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

mod plan;
pub(crate) use plan::{VectorPlanContext, attach_new_occurrences, retire_changed_paths};

pub const MAX_VECTOR_OCCURRENCES: usize = 128;

/// A new path-local citation and its already allocated DiskANN node. The
/// pointer refers to the changed artifact's verified retained extraction;
/// callers derive citation bodies from that exact captured payload.
#[derive(Clone, Debug)]
pub struct PreparedOccurrence {
    pub pointer: OccurrencePointer,
    pub graph_id: u32,
}

/// A path-local ordinal makes every old occurrence addressable after an edit
/// without a whole-corpus scan or a large list in one authenticated map value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathOccurrences {
    pub version: u32,
    pub path: String,
    pub source_hash: Sha256Digest,
    pub count: u32,
}

impl PathOccurrences {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && valid_current_path(&self.path)
                && usize::try_from(self.count)? <= MAX_EXTRACTION_CHUNKS,
            "invalid current path occurrence index"
        );
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= map::MAX_VALUE_BYTES,
            "path occurrence index too large"
        );
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8], path: &str) -> Result<Self> {
        ensure!(
            bytes.len() <= map::MAX_VALUE_BYTES,
            "path occurrence index too large"
        );
        let index: Self = serde_json::from_slice(bytes)?;
        ensure!(
            index.path == path && index.canonical_bytes()? == bytes,
            "current path occurrence index binding mismatch"
        );
        Ok(index)
    }

    pub fn occurrence_id(&self, scope: &CurrentScope, ordinal: u32) -> Result<Sha256Digest> {
        self.validate()?;
        scope.validate()?;
        ensure!(
            ordinal < self.count,
            "current path occurrence ordinal out of range"
        );
        Ok(framed_digest(
            b"current-path-occurrence/1",
            &[
                scope.root.as_bytes(),
                scope.collection.as_bytes(),
                scope.incarnation.as_bytes(),
                self.path.as_bytes(),
                self.source_hash.as_bytes(),
                &ordinal.to_be_bytes(),
            ],
        ))
    }
}

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
    pub path_ordinal: u32,
    pub source_hash: Sha256Digest,
    pub payload_digest: Sha256Digest,
    pub object: Sha256Digest,
    pub members: Vec<airk_core::vector_evidence::VectorMemberSpan>,
}

impl OccurrencePointer {
    pub fn validate(&self) -> Result<()> {
        airk_core::vector_evidence::validate_members(&self.members)?;
        ensure!(
            self.version == 1
                && !self.path.is_empty()
                && self.path.len() <= 8192
                && usize::try_from(self.path_ordinal)? < MAX_EXTRACTION_CHUNKS
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
