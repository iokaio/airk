// SPDX-License-Identifier: Apache-2.0
//! Existing service-payload inventory contract; pure bounded raw-byte decoder.
use super::{AdminAssets, RECORD_FILE, Signing};
use crate::identity::Sha256Digest;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const INVENTORY_SCHEMA: &str = "airk-service-payload/1";
pub const INVENTORY_FILE: &str = "payload-inventory.json";
pub const MAX_INVENTORY_BYTES: usize = 256 * 1024;
const MAX_INVENTORY_FILES: usize = 4096;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryFile {
    pub sha256: Sha256Digest,
    pub bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryExecutables {
    pub airkctl: String,
    pub airk: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadInventory {
    pub schema: String,
    pub version: String,
    pub channel: String,
    pub target: String,
    pub executables: InventoryExecutables,
    /// Program-relative `/`-separated path → digest and size.
    pub files: BTreeMap<String, InventoryFile>,
    pub admin_assets: AdminAssets,
    /// `None` for a payload whose images were not signed.
    pub signing: Option<Signing>,
}

impl PayloadInventory {
    pub fn decode(bytes: &[u8], expected: Sha256Digest) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_INVENTORY_BYTES,
            "payload_inventory_malformed: too many bytes"
        );
        ensure!(
            Sha256Digest::from_bytes(Sha256::digest(bytes).into()) == expected,
            "payload_inventory_mismatch: inventory differs from the operation's payload inventory"
        );
        let inventory: Self =
            serde_json::from_slice(bytes).context("payload_inventory_malformed")?;
        ensure!(
            inventory.schema == INVENTORY_SCHEMA,
            "payload_inventory_malformed: unknown schema"
        );
        ensure!(
            inventory.files.len() <= MAX_INVENTORY_FILES,
            "payload_inventory_malformed: too many files"
        );
        for name in inventory.files.keys() {
            ensure!(
                relative_inside(name) && name != INVENTORY_FILE && name != RECORD_FILE,
                "payload_inventory_malformed: invalid path {name:?}"
            );
        }
        for executable in [&inventory.executables.airk, &inventory.executables.airkctl] {
            ensure!(
                inventory.files.contains_key(executable),
                "payload_inventory_malformed: executable {executable:?} is not listed"
            );
        }
        Ok(inventory)
    }
}

pub fn relative_inside(relative: &str) -> bool {
    !relative.is_empty()
        && !relative.starts_with('/')
        && !relative.contains('\\')
        && !relative.contains(':')
        && relative
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests;
