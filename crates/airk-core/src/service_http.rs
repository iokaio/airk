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

/// Local trust plan C: the parts of an endpoint record that a build serving
/// loopback over HTTPS wrote which an upgrade carries forward. The service
/// keeps the port, publishes the next generation over plain HTTP and chains
/// it to this record's digest. Nothing else of the old record (its CA and
/// leaf digests) is read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyEndpoint {
    pub installation: Sha256Digest,
    pub user: String,
    pub namespace: Incarnation,
    pub generation: u64,
    pub port: u16,
    /// The digest an HTTPS build gave the record: its framed bytes, which
    /// that build wrote as the canonical encoding of the record.
    pub digest: Sha256Digest,
}

impl LegacyEndpoint {
    /// Decode an old record tolerantly: unknown fields (the CA and leaf
    /// digests, the state) are ignored, but the schema, the binding fields,
    /// a positive generation and a canonical `https://127.0.0.1:<port>/mcp`
    /// URL are required.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        #[derive(Deserialize)]
        struct Record {
            schema: u32,
            installation: Sha256Digest,
            user: String,
            namespace: Incarnation,
            generation: u64,
            url: String,
        }
        ensure!(
            bytes.len() <= MAX_ENDPOINT_BYTES,
            "endpoint record exceeds bound"
        );
        let record: Record = serde_json::from_slice(bytes)?;
        ensure!(
            record.schema == HTTP_CONTRACT_VERSION,
            "unsupported endpoint schema"
        );
        ensure!(record.generation > 0, "invalid endpoint generation");
        ensure!(
            !record.user.is_empty()
                && record.user.len() <= 256
                && record.user.bytes().all(|byte| byte.is_ascii_graphic()),
            "invalid native user identity"
        );
        let port = LoopbackMcpUrl::legacy_https_port(&record.url)
            .ok_or_else(|| anyhow::anyhow!("not a legacy HTTPS endpoint record"))?;
        Ok(Self {
            installation: record.installation,
            user: record.user,
            namespace: record.namespace,
            generation: record.generation,
            port,
            digest: crate::identity::framed_digest(b"airk-http-endpoint/1", &[bytes]),
        })
    }
}

/// A saved endpoint record: the current shape, or one an HTTPS build wrote
/// (local trust plan C), which only an upgrade reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavedEndpoint {
    Current(EndpointDescriptor),
    Legacy(LegacyEndpoint),
}

impl SavedEndpoint {
    /// The current shape first; an old HTTPS record only when that fails.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        match EndpointDescriptor::decode(bytes) {
            Ok(current) => Ok(Self::Current(current)),
            Err(current) => LegacyEndpoint::decode(bytes)
                .map(Self::Legacy)
                .map_err(|_| current),
        }
    }

    pub fn port(&self) -> u16 {
        match self {
            Self::Current(value) => value.url.port(),
            Self::Legacy(value) => value.port,
        }
    }

    pub fn generation(&self) -> u64 {
        match self {
            Self::Current(value) => value.generation,
            Self::Legacy(value) => value.generation,
        }
    }

    pub fn digest(&self) -> Result<Sha256Digest> {
        match self {
            Self::Current(value) => value.digest(),
            Self::Legacy(value) => Ok(value.digest),
        }
    }

    /// The installation, user and namespace the record names.
    pub fn binding(&self) -> (Sha256Digest, &str, Incarnation) {
        match self {
            Self::Current(value) => (value.installation, &value.user, value.namespace),
            Self::Legacy(value) => (value.installation, &value.user, value.namespace),
        }
    }

    pub fn current(&self) -> Option<&EndpointDescriptor> {
        match self {
            Self::Current(value) => Some(value),
            Self::Legacy(_) => None,
        }
    }

    pub fn into_current(self) -> Option<EndpointDescriptor> {
        match self {
            Self::Current(value) => Some(value),
            Self::Legacy(_) => None,
        }
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
