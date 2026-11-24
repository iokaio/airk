// SPDX-License-Identifier: Apache-2.0
//! The installation binding of the HTTPS identity, and the names and bounds
//! of the credential ledger an installation from before local trust plan B
//! (23 November 2026) retains. Credentials are no longer issued, read or
//! authenticated; `airk-endpoint`'s read-only `RetainedLedger` reads the
//! retained ledger for predecessor cleanup only.
use crate::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The ledger file inside the HTTPS private folder.
pub const STORE: &str = "credentials.json";
/// The ledger read bound.
pub const MAX_STORE: usize = 1_048_576;
/// The ledger record bound.
pub const MAX_CREDENTIALS: usize = 128;
/// A retained native credential token is exactly this long.
pub const MAX_TOKEN: usize = 103;

/// The installation, user and namespace an HTTPS identity belongs to.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationBinding {
    pub installation: Sha256Digest,
    pub user: String,
    pub namespace: Incarnation,
}

impl InstallationBinding {
    /// Reconstruct from a canonical control root, the current user and the
    /// native namespace, never from untrusted JSON.
    pub fn derive(canonical_control: &Path, user: &str, namespace: Incarnation) -> Result<Self> {
        let bytes = serde_json::to_vec(canonical_control)?;
        Ok(Self {
            installation: crate::identity::framed_digest(
                b"airk-http-installation/1",
                &[&bytes, user.as_bytes(), namespace.as_bytes()],
            ),
            user: user.to_owned(),
            namespace,
        })
    }
    /// The binding belongs to `user`.
    pub fn require_user(&self, user: &str) -> Result<()> {
        ensure!(self.user == user, "HTTP identity owner mismatch");
        Ok(())
    }
}

/// The token file (or, after revocation, retirement marker) of connection
/// `id` in the retained ledger's folder.
pub fn filename(id: Incarnation) -> String {
    format!("credential-{id}.token")
}
