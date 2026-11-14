// SPDX-License-Identifier: Apache-2.0
//! Private native Admin custody and bounded public chunk semantics.
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An A1 launch code (contracts §2.7): 32 random bytes as unpadded
/// base64url (43 characters, canonical final character). `Debug` never shows
/// it; the service stores only its SHA-256.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AdminLaunchSecret(String);

impl AdminLaunchSecret {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for AdminLaunchSecret {
    type Error = String;
    fn try_from(value: String) -> std::result::Result<Self, String> {
        let bytes = value.as_bytes();
        // 43 characters carry 258 bits; the final two must be zero.
        if bytes.len() == 43
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'_')
            && b"AEIMQUYcgkosw048".contains(&bytes[42])
        {
            Ok(Self(value))
        } else {
            Err("invalid admin launch code".into())
        }
    }
}

impl From<AdminLaunchSecret> for String {
    fn from(code: AdminLaunchSecret) -> String {
        code.0
    }
}

impl std::fmt::Debug for AdminLaunchSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AdminLaunchSecret(<redacted>)")
    }
}

pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;
pub const CHUNK_FRAME_BYTES: usize = 512 * 1024;
/// Existing I2 120-second ceiling, expressed in the same-host native domain.
pub const MAX_DEADLINE_NS: u64 = 120_000_000_000;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAdminSessionSecret {
    cookie: AdminLaunchSecret,
    csrf: AdminLaunchSecret,
}
impl NativeAdminSessionSecret {
    pub fn new(cookie: String, csrf: String) -> Result<Self> {
        Ok(Self {
            cookie: cookie.try_into().map_err(anyhow::Error::msg)?,
            csrf: csrf.try_into().map_err(anyhow::Error::msg)?,
        })
    }
    pub fn cookie(&self) -> &str {
        self.cookie.as_str()
    }
    pub fn csrf(&self) -> &str {
        self.csrf.as_str()
    }
}
impl std::fmt::Debug for NativeAdminSessionSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeAdminSessionSecret([REDACTED])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeAdminChunk {
    #[schemars(range(min = 100, max = 599))]
    pub http_status: u16,
    #[schemars(range(min = 1, max = 33554432))]
    pub total_bytes: u64,
    #[schemars(range(max = 33554432))]
    pub offset: u64,
    #[schemars(length(min = 1, max = 65536))]
    pub data: Vec<u8>,
    pub done: bool,
}
impl NativeAdminChunk {
    pub fn validate(&self) -> Result<u64> {
        ensure!(
            (100..=599).contains(&self.http_status),
            "invalid native Admin HTTP status"
        );
        ensure!(
            (1..=MAX_RESPONSE_BYTES).contains(&self.total_bytes),
            "native Admin response size refused"
        );
        ensure!(
            !self.data.is_empty() && self.data.len() <= MAX_CHUNK_BYTES,
            "native Admin chunk size refused"
        );
        let end = self
            .offset
            .checked_add(self.data.len() as u64)
            .ok_or_else(|| anyhow::anyhow!("native Admin offset overflow"))?;
        ensure!(
            end <= self.total_bytes && self.done == (end == self.total_bytes),
            "inconsistent native Admin chunk"
        );
        Ok(end)
    }
}

/// Reference semantic owner for the frontend's bounded assembly; no secrets.
#[derive(Default)]
pub struct NativeAdminAssembly {
    status: Option<u16>,
    total: Option<u64>,
    bytes: Vec<u8>,
    finished: bool,
    failed: bool,
}
impl NativeAdminAssembly {
    pub fn push(&mut self, chunk: &NativeAdminChunk) -> Result<Option<serde_json::Value>> {
        ensure!(!self.failed, "native Admin stream already refused");
        let result = self.push_valid(chunk);
        if result.is_err() {
            self.failed = true;
            self.bytes.clear();
        }
        result
    }
    fn push_valid(&mut self, chunk: &NativeAdminChunk) -> Result<Option<serde_json::Value>> {
        ensure!(!self.finished, "native Admin stream already complete");
        chunk.validate()?;
        ensure!(
            chunk.offset == self.bytes.len() as u64,
            "native Admin stream offset changed"
        );
        ensure!(
            self.status.is_none_or(|status| status == chunk.http_status)
                && self.total.is_none_or(|total| total == chunk.total_bytes),
            "native Admin stream identity changed"
        );
        self.status = Some(chunk.http_status);
        self.total = Some(chunk.total_bytes);
        self.bytes.extend_from_slice(&chunk.data);
        if chunk.done {
            let value = serde_json::from_slice(&self.bytes)?;
            self.finished = true;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }
    pub fn complete(&self) -> bool {
        self.finished
    }
}

#[cfg(test)]
mod tests;
