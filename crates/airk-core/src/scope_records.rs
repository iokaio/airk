// SPDX-License-Identifier: Apache-2.0
//! Shared inert scope identities used by service frames and native requests.
use crate::identity::Sha256Digest;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// The owner plane of an in-memory selection grant (W4 window, A2 admin,
/// or a U105 open agent connection).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionOwnerKind {
    Window,
    Admin,
    /// U105/U106(d): an open agent read. A named index is read whole, so
    /// its observation may carry no project memberships.
    Agent,
}

/// In-memory selection grant identity. Never a bearer: it carries no lease,
/// cookie or session id, and possession of it admits nothing. It is never
/// carried by a client-visible frame. `Debug` prints `grant_nonce` redacted.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionScopeRef {
    pub owner: SelectionOwnerKind,
    /// Random identity drawn by the service when the grant is stored; a new
    /// grant (re-select) gets a new nonce.
    pub grant_nonce: crate::identity::Incarnation,
    pub service_instance: crate::identity::Incarnation,
    /// `scope_grant::decide`'s identity of the stored selection.
    pub scope_identity: Sha256Digest,
    pub catalog_revision: crate::index_catalog::CatalogRevision,
}
impl SelectionScopeRef {
    /// The public part only: never the nonce or the service instance.
    pub fn observation(&self) -> ScopeSelectionObservation {
        ScopeSelectionObservation {
            owner: self.owner,
            scope_identity: self.scope_identity,
            catalog_revision: self.catalog_revision,
        }
    }
}
impl std::fmt::Debug for SelectionScopeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectionScopeRef")
            .field("owner", &self.owner)
            .field("grant_nonce", &"[REDACTED]")
            .field("service_instance", &self.service_instance)
            .field("scope_identity", &self.scope_identity)
            .field("catalog_revision", &self.catalog_revision)
            .finish()
    }
}

/// Public selection identity; deliberately omits the nonce and service instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeSelectionObservation {
    pub owner: SelectionOwnerKind,
    pub scope_identity: Sha256Digest,
    pub catalog_revision: crate::index_catalog::CatalogRevision,
}

/// Frozen native scope identity. This metadata is never a bearer credential.
/// Its one authority source is an in-memory selection (`selection`) of a
/// window, the Admin session or an open agent read; it serializes exactly as
/// the former optional field did. A selection ref is never a durable fence
/// target and never leaves the private host/worker path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedReadRef {
    pub schema: u32,
    /// Native-reserved, worker-monotonic installation lease; never a bearer.
    pub lease_generation: u64,
    pub index_id: crate::index_catalog::IndexId,
    pub selection: SelectionScopeRef,
    pub fingerprint: Sha256Digest,
}
impl ScopedReadRef {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && self.lease_generation > 0,
            "unsupported scoped read contract"
        );
        Ok(())
    }
}
