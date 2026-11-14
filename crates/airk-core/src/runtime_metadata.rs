// SPDX-License-Identifier: Apache-2.0
//! Strict C7 schema-three metadata for a service and CLI in one native payload.
//! Pure claims only: authenticity, filesystem checks and interface admission are adapters.
use crate::identity::Sha256Digest;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const SPLIT_RUNTIME_SCHEMA: u32 = 3;
pub const MAX_RUNTIME_METADATA_BYTES: usize = 4096;

/// Full existing C7 identity. The declaration order is its canonical compact encoding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Wire")]
pub struct SplitRuntimeMetadata {
    pub schema: u32,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub airk_sha256: Sha256Digest,
    pub airkctl_sha256: Sha256Digest,
    pub i1_protocol: u32,
    pub i2_contract: u32,
    pub admin_inventory_sha256: Sha256Digest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    schema: u32,
    version: String,
    os: String,
    arch: String,
    airk_sha256: Sha256Digest,
    airkctl_sha256: Sha256Digest,
    i1_protocol: u32,
    i2_contract: u32,
    admin_inventory_sha256: Sha256Digest,
}

impl TryFrom<Wire> for SplitRuntimeMetadata {
    type Error = anyhow::Error;
    fn try_from(wire: Wire) -> Result<Self> {
        let value = Self {
            schema: wire.schema,
            version: wire.version,
            os: wire.os,
            arch: wire.arch,
            airk_sha256: wire.airk_sha256,
            airkctl_sha256: wire.airkctl_sha256,
            i1_protocol: wire.i1_protocol,
            i2_contract: wire.i2_contract,
            admin_inventory_sha256: wire.admin_inventory_sha256,
        };
        value.validate()?;
        Ok(value)
    }
}

impl SplitRuntimeMetadata {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SPLIT_RUNTIME_SCHEMA,
            "unsupported split runtime schema"
        );
        let parts: Vec<_> = self.version.split('.').collect();
        ensure!(
            self.version.len() <= 4096
                && parts.len() == 3
                && parts.iter().all(|part| {
                    !part.is_empty()
                        && (part.len() == 1 || !part.starts_with('0'))
                        && part.bytes().all(|byte| byte.is_ascii_digit())
                }),
            "invalid split runtime version"
        );
        ensure!(
            matches!(
                (self.os.as_str(), self.arch.as_str()),
                ("windows" | "linux" | "macos", "x86_64") | ("macos", "aarch64")
            ),
            "unsupported split runtime target"
        );
        ensure!(
            (1..=65_535).contains(&self.i1_protocol) && (1..=65_535).contains(&self.i2_contract),
            "invalid split runtime interface version"
        );
        ensure!(
            self.airk_sha256 != self.airkctl_sha256,
            "split runtime images must be distinct"
        );
        Ok(())
    }

    pub fn validate_target(&self, version: &str, os: &str, arch: &str) -> Result<()> {
        self.validate()?;
        ensure!(
            self.version == version && self.os == os && self.arch == arch,
            "incompatible split runtime target"
        );
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_RUNTIME_METADATA_BYTES,
            "split runtime metadata exceeds bound"
        );
        Ok(serde_json::from_slice(bytes)?)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_RUNTIME_METADATA_BYTES,
            "split runtime metadata exceeds bound"
        );
        Ok(bytes)
    }

    /// Payload-relative names, including the native macOS bundle topology.
    pub fn service_relative_path(&self) -> Result<&'static str> {
        self.validate()?;
        Ok(match self.os.as_str() {
            "windows" => "airk.exe",
            "macos" => "AIRK.app/Contents/MacOS/airk",
            _ => "airk",
        })
    }

    pub fn cli_relative_path(&self) -> Result<&'static str> {
        self.validate()?;
        Ok(match self.os.as_str() {
            "windows" => "airkctl.exe",
            "macos" => "AIRK.app/Contents/MacOS/airkctl",
            _ => "airkctl",
        })
    }
}

#[cfg(test)]
mod tests;
