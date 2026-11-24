// SPDX-License-Identifier: Apache-2.0
//! Inert loopback connection descriptions, never proof of native authority.
use crate::identity::{Incarnation, Sha256Digest};
pub use crate::service_records::LoopbackMcpUrl;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const HTTP_CONTRACT_VERSION: u32 = 1;
pub const PREFERRED_HTTP_PORT: u16 = 57421;
pub const MAX_ENDPOINT_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointState {
    Recovering,
    Ready,
    Stopping,
}

/// Public metadata obtained through protected native control/storage.
/// A digest or deserialized descriptor alone authenticates neither file nor peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointDescriptor {
    pub schema: u32,
    pub installation: Sha256Digest,
    pub user: String,
    pub namespace: Incarnation,
    pub generation: u64,
    pub service_instance: Incarnation,
    pub executable_sha256: Sha256Digest,
    pub url: LoopbackMcpUrl,
    pub ca_sha256: Sha256Digest,
    pub certificate_sha256: Sha256Digest,
    pub state: EndpointState,
    pub predecessor_digest: Option<Sha256Digest>,
}

impl EndpointDescriptor {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == HTTP_CONTRACT_VERSION,
            "unsupported endpoint schema"
        );
        ensure!(self.generation > 0, "invalid endpoint generation");
        ensure!(
            !self.user.is_empty()
                && self.user.len() <= 256
                && self.user.bytes().all(|byte| byte.is_ascii_graphic()),
            "invalid native user identity"
        );
        crate::index_catalog::encoded_size(self, MAX_ENDPOINT_BYTES)?;
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_ENDPOINT_BYTES,
            "endpoint record exceeds bound"
        );
        let value: Self = serde_json::from_slice(bytes)?;
        value.validate()?;
        Ok(value)
    }

    pub fn digest(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(crate::identity::framed_digest(
            b"airk-http-endpoint/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

/// The local agent session's open selection, not a URL-derived or ambient
/// workspace scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HttpSelection {
    /// U105/U110: the open selection of the built-in local agent session.
    Open { selection: OpenHttpSelection },
}

/// Open-read refusal codes (U105). Stable machine codes, no detail.
pub mod open_refusal {
    /// Agent choice is off and the connection has no default scope.
    pub const AGENT_DEFAULT_SCOPE_UNSET: &str = "agent_default_scope_unset";
    /// A named index or set matches more than one entry by exact name.
    pub const SCOPE_NAME_AMBIGUOUS: &str = "scope_name_ambiguous";
    /// A named index or set matches no id and no exact name.
    pub const SCOPE_NAME_UNKNOWN: &str = "scope_name_unknown";
    /// The resolved scope (sets expanded) names more than 32 indexes.
    pub const AGENT_SCOPE_TOO_LARGE: &str = "agent_scope_too_large";
    /// U110: a read named no index, no index set and no workspace with a
    /// stored default.
    pub const SCOPE_REQUIRED: &str = "scope_required";
    /// A `list_scopes` cursor of another catalog revision.
    pub const LIST_CURSOR_EXPIRED: &str = "list_cursor_expired";
}

/// U105: the open connection's namespace only. It names no index, grant or
/// project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenHttpSelection {
    pub namespace: Incarnation,
}

impl HttpSelection {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Open { .. } => Ok(()),
        }
    }

    pub fn namespace(&self) -> Incarnation {
        match self {
            Self::Open { selection } => selection.namespace,
        }
    }
}

#[cfg(test)]
mod tests;
