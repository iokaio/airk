// SPDX-License-Identifier: Apache-2.0
//! Collection-local immutable embedding packs. The caller holds the collection writer.
//! Exact input keys and numerical checksums belong to core; this adapter only changes storage.
mod format;
#[cfg(test)]
mod tests;
mod update;

use airk_core::{embedding::MAX_EMBEDDING_DIMENSIONS, identity::Sha256Digest};
use anyhow::{Result, ensure};
pub use format::{
    PACK_HEADER_BYTES, pack_path, record_bytes, verification_read_bound, verify_pack,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub use update::VectorCache;

pub const MAX_CACHE_REFERENCES: usize = 1_000_000;
pub const MAX_CACHE_READ_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub const MAX_CACHE_MANIFEST_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_PACK_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_PACK_RECORDS: u32 = 4096;
pub const MAX_PACKS: usize = 4096;
pub const MAX_INDEX_CACHE_BYTES: u64 = 4 * 1024 * 1024;
/// Six retained readers leave room for the private writer and one verification reader.
pub const MAX_OPEN_PACK_HANDLES: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackReference {
    pub sha256: Sha256Digest,
    pub bytes: u64,
    /// SHA-256 of the complete fixed header followed by the sorted record index.
    pub index_sha256: Sha256Digest,
    pub pipeline_digest: Sha256Digest,
    pub dimensions: u32,
    pub records: u32,
}

impl PackReference {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=MAX_PACK_RECORDS).contains(&self.records),
            "invalid embedding pack record count"
        );
        ensure!(
            self.bytes == format::pack_bytes(self.dimensions, self.records)?
                && self.bytes <= MAX_PACK_BYTES,
            "invalid embedding pack byte count"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorCacheReference {
    pub key: Sha256Digest,
    pub pack: Sha256Digest,
    /// Physical record ordinal, independent of the index's key order.
    pub record: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorCacheManifest {
    pub version: u32,
    pub pipeline_digest: Sha256Digest,
    pub dimensions: u32,
    pub packs: Vec<PackReference>,
    /// Only records used by this publication own their containing pack.
    pub references: Vec<VectorCacheReference>,
}

impl VectorCacheManifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 2,
            "obsolete embedding cache format; run airk index --full"
        );
        ensure!(
            (1..=MAX_EMBEDDING_DIMENSIONS as u32).contains(&self.dimensions)
                && self.references.len() <= MAX_CACHE_REFERENCES
                && self.packs.len() <= MAX_PACKS,
            "embedding cache manifest exceeds bounds"
        );
        ensure!(
            self.packs
                .windows(2)
                .all(|pair| pair[0].sha256 < pair[1].sha256)
                && self
                    .references
                    .windows(2)
                    .all(|pair| pair[0].key < pair[1].key),
            "embedding packs and references must be sorted and unique"
        );
        let mut packs = BTreeMap::new();
        for pack in &self.packs {
            pack.validate()?;
            ensure!(
                pack.pipeline_digest == self.pipeline_digest && pack.dimensions == self.dimensions,
                "embedding pack pipeline differs from manifest"
            );
            packs.insert(pack.sha256, (pack, 0usize));
        }
        let mut physical = BTreeSet::new();
        for reference in &self.references {
            let (pack, count) = packs
                .get_mut(&reference.pack)
                .ok_or_else(|| anyhow::anyhow!("embedding reference has no owned pack"))?;
            ensure!(
                reference.record < pack.records
                    && physical.insert((reference.pack, reference.record)),
                "invalid or duplicate embedding record reference"
            );
            *count += 1;
        }
        ensure!(
            packs.values().all(|(_, count)| *count > 0),
            "unreferenced embedding pack"
        );
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_CACHE_MANIFEST_BYTES,
            "embedding cache manifest exceeds byte bound"
        );
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_CACHE_MANIFEST_BYTES,
            "embedding cache manifest exceeds byte bound"
        );
        #[derive(Deserialize)]
        struct Version {
            version: u32,
        }
        ensure!(
            serde_json::from_slice::<Version>(bytes)?.version == 2,
            "obsolete embedding cache format; run airk index --full"
        );
        let manifest: Self = serde_json::from_slice(bytes)?;
        ensure!(
            manifest.canonical_bytes()? == bytes,
            "noncanonical embedding cache manifest"
        );
        Ok(manifest)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct VectorCacheStatistics {
    pub file_opens: u64,
    /// Application bytes requested from files; includes verification reads.
    pub read_bytes: u64,
    /// Application writes, including the final fixed-header rewrite.
    pub written_bytes: u64,
    pub record_verification_bytes: u64,
    pub index_verification_bytes: u64,
    /// Full-stream hashing reads; overlaps read_bytes and record verification.
    pub full_pack_verification_bytes: u64,
    pub syncs: u64,
    pub index_cache_hits: u64,
    /// Declared retained decoded-index allocation, not process RSS.
    pub index_cache_peak_bytes: u64,
    pub packs_written: u64,
    pub packs_reused: u64,
    pub compacted_records: u64,
}
