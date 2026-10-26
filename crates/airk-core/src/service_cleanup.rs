// SPDX-License-Identifier: Apache-2.0
//! Bounded metadata for explicit, source-preserving derived-state cleanup.
use crate::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_CLEANUP_FILES: u64 = 1_000_000;
pub const MAX_CLEANUP_BYTES: u64 = 1 << 40;
pub const MAX_PRESERVED_SETTINGS_BYTES: u64 = 1 << 20;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DerivedCleanupPreview {
    pub version: u32,
    pub owner: Sha256Digest,
    pub guard: Sha256Digest,
    pub active: Incarnation,
    pub known_incarnations: u8,
    pub owned_files: u64,
    pub owned_bytes: u64,
    /// False means counts are bounded lower bounds; the owner-bound scope is exact.
    pub inventory_complete: bool,
    pub preserved_settings_files: u8,
    pub preserved_settings_bytes: u64,
    pub resuming: bool,
}
impl DerivedCleanupPreview {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && (1..=10).contains(&self.known_incarnations)
                && self.owned_files <= MAX_CLEANUP_FILES
                && self.owned_bytes <= MAX_CLEANUP_BYTES
                && self.preserved_settings_files <= 5
                && self.preserved_settings_bytes <= MAX_PRESERVED_SETTINGS_BYTES,
            "invalid derived cleanup preview"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DerivedCleanupReport {
    pub version: u32,
    pub owner: Sha256Digest,
    pub guard: Sha256Digest,
    pub complete: bool,
    pub pending_incarnations: u8,
    /// Original owned files inventoried before their first deletion turn.
    pub removed_known_files: u64,
    pub removed_known_bytes: u64,
    pub preserved_settings_files: u8,
    pub preserved_settings_bytes: u64,
}
impl DerivedCleanupReport {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && self.pending_incarnations <= 10
                && self.complete == (self.pending_incarnations == 0)
                && self.removed_known_files <= MAX_CLEANUP_FILES.saturating_mul(10)
                && self.removed_known_bytes <= MAX_CLEANUP_BYTES
                && self.preserved_settings_files <= 5
                && self.preserved_settings_bytes <= MAX_PRESERVED_SETTINGS_BYTES,
            "invalid derived cleanup report"
        );
        Ok(())
    }
}
