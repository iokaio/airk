// SPDX-License-Identifier: Apache-2.0
//! Truthful identity of saved predecessor evidence, not a native extraction claim.
use crate::identity::{Sha256Digest, framed_digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacySavedOrigin {
    pub schema: u32,
    pub archive_inventory_sha256: Sha256Digest,
    pub original_root_sha256: Sha256Digest,
    pub original_path: String,
    pub source_available: bool,
    pub profile_available: bool,
}
impl LegacySavedOrigin {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && !self.original_path.is_empty()
                && self.original_path.len() <= 8192
                && !self.original_path.contains(['\\', ':', '\0'])
                && self
                    .original_path
                    .split('/')
                    .all(|part| !matches!(part, "" | "." | "..")),
            "invalid saved legacy evidence origin"
        );
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/legacy-saved-evidence/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}
