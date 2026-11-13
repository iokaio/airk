// SPDX-License-Identifier: Apache-2.0
//! Inert loopback connection descriptions, never proof of native authority.
use crate::{
    identity::{Incarnation, Sha256Digest},
    service::{CombinedConnectRequest, ScopedConnectRequest},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

pub const HTTP_CONTRACT_VERSION: u32 = 1;
pub const PREFERRED_HTTP_PORT: u16 = 57421;
pub const MAX_ENDPOINT_BYTES: usize = 4096;
pub const MAX_HTTP_SELECTION_BYTES: usize = 48 * 1024;
pub const MAX_CONNECTION_BYTES: usize = 60 * 1024;
pub const MAX_IMPORTED_HTTP_MEMBER_BYTES: usize = 16 * 1024;

/// Exactly one canonical IPv4 loopback MCP URL. No DNS, redirects or userinfo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LoopbackMcpUrl(u16);

impl LoopbackMcpUrl {
    pub fn new(port: u16) -> Result<Self> {
        ensure!(
            port != 0,
            "an endpoint needs an actually bound nonzero port"
        );
        Ok(Self(port))
    }

    pub fn port(self) -> u16 {
        self.0
    }

    pub fn origin(self) -> String {
        format!("https://127.0.0.1:{}", self.0)
    }
}

impl fmt::Display for LoopbackMcpUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "https://127.0.0.1:{}/mcp", self.0)
    }
}

impl FromStr for LoopbackMcpUrl {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        ensure!(value.len() <= 32, "loopback URL exceeds its bound");
        let port = value
            .strip_prefix("https://127.0.0.1:")
            .and_then(|value| value.strip_suffix("/mcp"))
            .ok_or_else(|| anyhow::anyhow!("expected a canonical loopback HTTPS MCP URL"))?;
        ensure!(
            !port.is_empty()
                && !port.starts_with('0')
                && port.bytes().all(|byte| byte.is_ascii_digit()),
            "noncanonical loopback port"
        );
        Self::new(port.parse()?)
    }
}

impl TryFrom<String> for LoopbackMcpUrl {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}

impl From<LoopbackMcpUrl> for String {
    fn from(value: LoopbackMcpUrl) -> Self {
        value.to_string()
    }
}

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

/// Existing reviewed grant selection, not a URL-derived or ambient workspace scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HttpSelection {
    Scoped {
        selection: ScopedConnectRequest,
    },
    Combined {
        selection: CombinedConnectRequest,
    },
    ImportedLegacy {
        selection: Box<NativeImportedHttpSelection>,
    },
    ImportedLegacyUnion {
        selection: NativeImportedHttpUnion,
    },
    /// U105/U106(b): an open agent connection. The bearer admits the agent
    /// to the MCP endpoint without a reviewed grant; the record's `open`
    /// state (default scope, agent choice) decides what each call reads.
    /// Served by enrollment, connection info and the HTTPS open backend.
    Open {
        selection: OpenHttpSelection,
    },
}

/// Open-read refusal codes (U105). Stable machine codes, no detail.
pub mod open_refusal {
    /// Agent choice is off and the call named indexes or index sets.
    pub const AGENT_SCOPE_CHOICE_DISABLED: &str = "agent_scope_choice_disabled";
    /// Agent choice is off and the connection has no default scope.
    pub const AGENT_DEFAULT_SCOPE_UNSET: &str = "agent_default_scope_unset";
    /// A named index or set matches more than one entry by exact name.
    pub const SCOPE_NAME_AMBIGUOUS: &str = "scope_name_ambiguous";
    /// A named index or set matches no id and no exact name.
    pub const SCOPE_NAME_UNKNOWN: &str = "scope_name_unknown";
    /// The resolved scope (sets expanded) names more than 32 indexes.
    pub const AGENT_SCOPE_TOO_LARGE: &str = "agent_scope_too_large";
    /// The connection is not an open connection of this installation, or it
    /// is revoked or expired.
    pub const AGENT_CONNECTION_NOT_OPEN: &str = "agent_connection_not_open";
    /// `expected_defaults_revision` is not the stored defaults revision.
    pub const AGENT_DEFAULTS_CONFLICT: &str = "agent_defaults_conflict";
    /// A `list_indexes` cursor of another catalog revision.
    pub const LIST_CURSOR_EXPIRED: &str = "list_cursor_expired";
}

/// U105: the open connection's namespace only. It names no index, grant or
/// project; the service-stored default scope lives in the credential ledger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenHttpSelection {
    pub namespace: Incarnation,
}

/// Original multi-project server routing, not an authorization union or a new
/// reviewed grant. Every original binding still uses its own imported authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeImportedHttpUnion {
    pub namespace: Incarnation,
    pub selections: Vec<NativeImportedHttpSelection>,
    pub no_default_project: bool,
}
impl NativeImportedHttpUnion {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (2..=32).contains(&self.selections.len()) && self.no_default_project,
            "imported multi-project server requires 2..32 explicit bindings and no default project"
        );
        let mut bindings = std::collections::BTreeSet::new();
        for member in &self.selections {
            member.validate()?;
            ensure!(
                member.namespace == self.namespace && bindings.insert(member.registration.binding),
                "crossed or duplicate imported server binding"
            );
        }
        crate::index_catalog::encoded_size(self, MAX_HTTP_SELECTION_BYTES)?;
        Ok(())
    }
}

/// Exact imported predecessor scope, never a new reviewed grant. The config
/// receipt identifies the client's pre-migration bytes, not evidence authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeImportedHttpSelection {
    pub namespace: Incarnation,
    pub expected_catalog_revision: crate::index_catalog::CatalogRevision,
    pub registration: crate::service_records::ProjectRegistration,
    pub grant: crate::grant_provenance::PersistedGrant,
    pub configuration_receipt: Sha256Digest,
}

impl NativeImportedHttpSelection {
    pub fn validate(&self) -> Result<()> {
        self.registration.validate()?;
        self.grant.validate()?;
        let legacy_binding = self.grant.provenance.imported_binding().ok_or_else(|| {
            anyhow::anyhow!("HTTP predecessor selection requires imported provenance")
        })?;
        if let crate::grant_provenance::GrantProvenance::ImportedStandaloneAdoption { adoption } =
            &self.grant.provenance
        {
            ensure!(
                adoption.plan.setup.key.namespace == self.namespace
                    && adoption.plan.setup.registration == self.registration
                    && adoption.admits_configuration(self.configuration_receipt),
                "standalone HTTP selector differs from original configuration or adopted registration"
            );
        }
        ensure!(
            legacy_binding == self.registration.binding
                && !self.grant.grant.revoked
                && self.grant.grant.id
                    == crate::grant_provenance::native_imported_grant_id(
                        self.namespace,
                        self.grant.grant.host,
                        &self.grant.provenance
                    )?,
            "crossed imported HTTP scope or lineage"
        );
        crate::index_catalog::encoded_size(self, MAX_IMPORTED_HTTP_MEMBER_BYTES)?;
        Ok(())
    }
}

impl HttpSelection {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Scoped { selection } => selection.validate(),
            Self::Combined { selection } => selection.validate(),
            Self::ImportedLegacy { selection } => selection.validate(),
            Self::ImportedLegacyUnion { selection } => selection.validate(),
            Self::Open { .. } => Ok(()),
        }
    }

    pub fn namespace(&self) -> Incarnation {
        match self {
            Self::Scoped { selection } => selection.namespace,
            Self::Combined { selection } => selection.namespace,
            Self::ImportedLegacy { selection } => selection.namespace,
            Self::ImportedLegacyUnion { selection } => selection.namespace,
            Self::Open { selection } => selection.namespace,
        }
    }
}

/// Native setup returns protected file references, never a printable bearer token.
/// The adapter must validate ownership and safe-file identity before reading them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectionInfo {
    pub schema: u32,
    pub connection_id: Incarnation,
    pub endpoint: EndpointDescriptor,
    pub selection: HttpSelection,
    pub ca_certificate_file: String,
    pub credential_file: String,
    /// U105: an open connection's stored default scope and agent choice,
    /// present exactly when `selection` is `Open`. Absent, the encoding is
    /// the pre-U105 one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<crate::agent_connection::OpenConnectionState>,
}

impl HttpConnectionInfo {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == HTTP_CONTRACT_VERSION,
            "unsupported connection schema"
        );
        self.endpoint.validate()?;
        self.selection.validate()?;
        ensure!(
            self.selection.namespace() == self.endpoint.namespace,
            "crossed HTTP connection namespace"
        );
        if let Some(open) = &self.open {
            open.validate()?;
        }
        ensure!(
            matches!(self.selection, HttpSelection::Open { .. }) == self.open.is_some(),
            "open connection state does not match its selection"
        );
        for reference in [&self.ca_certificate_file, &self.credential_file] {
            ensure!(
                !reference.is_empty()
                    && reference.len() <= 4096
                    && !reference.chars().any(char::is_control),
                "invalid private file reference"
            );
        }
        crate::index_catalog::encoded_size(self, MAX_CONNECTION_BYTES)?;
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_CONNECTION_BYTES,
            "connection record exceeds bound"
        );
        let value: Self = serde_json::from_slice(bytes)?;
        value.validate()?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
