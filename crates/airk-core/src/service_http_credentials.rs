// SPDX-License-Identifier: Apache-2.0
//! The owner-private agent credential ledger and token format (U90).
//!
//! Pure parsing and verification only: callers read the owner-private files
//! (`airk-cli` for the service, `airkctl` for the offline header helper) and
//! pass their bytes here. The on-disk format is unchanged (U26). Nothing here
//! performs I/O, creates a ledger or logs a token.
use crate::{
    identity::{Incarnation, Sha256Digest},
    service_http::HttpSelection,
    usage::ClientFamily,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};
use subtle::ConstantTimeEq;

/// The ledger file inside the HTTPS private folder.
pub const STORE: &str = "credentials.json";
/// The ledger read bound.
pub const MAX_STORE: usize = 1_048_576;
/// The ledger record bound.
pub const MAX_CREDENTIALS: usize = 128;
/// A native credential token is exactly this long.
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

fn unknown_client() -> ClientFamily {
    ClientFamily::Unknown
}

/// One issued connection.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    #[serde(default = "unknown_client")]
    pub client: ClientFamily,
    pub id: Incarnation,
    pub generation: u64,
    pub selection: HttpSelection,
    #[serde(default)]
    pub explorer_descriptor: Option<PathBuf>,
    pub expires_unix_ms: u64,
    pub verifier: Sha256Digest,
    pub revoked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation_reason: Option<RevocationReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevocationReason {
    Explicit,
    AttachmentRetired,
    Expired,
    Unknown,
}

/// `credentials.json`.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    pub schema: u32,
    pub binding: InstallationBinding,
    pub records: Vec<Record>,
}

/// A token that authenticated against an active record.
#[derive(Clone)]
pub struct AuthenticatedCredential {
    pub client: ClientFamily,
    pub connection_id: Incarnation,
    pub generation: u64,
    pub selection: HttpSelection,
    pub expires_unix_ms: u64,
    pub explorer_descriptor: Option<PathBuf>,
}

/// The token file of connection `id`.
pub fn filename(id: Incarnation) -> String {
    format!("credential-{id}.token")
}

/// The connection a well-formed token names.
pub fn token_id(token: &str) -> Result<Incarnation> {
    ensure!(
        token.len() == MAX_TOKEN && token.starts_with("airk1.") && token.as_bytes()[38] == b'.',
        "HTTP credential rejected"
    );
    ensure!(
        token.as_bytes()[6..38]
            .iter()
            .chain(&token.as_bytes()[39..])
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)),
        "HTTP credential rejected"
    );
    Incarnation::from_str(&token[6..38]).map_err(|_| anyhow::anyhow!("HTTP credential rejected"))
}

/// A client token file's bytes, checked to name connection `id`.
pub fn client_token(bytes: Vec<u8>, id: Incarnation) -> Result<String> {
    let token =
        String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("HTTP client credential invalid"))?;
    ensure!(
        token_id(&token)? == id,
        "HTTP client credential identity mismatch"
    );
    Ok(token)
}

/// A protected Explorer attachment reference is an absolute, bounded path.
pub fn validate_attachment(path: Option<&Path>) -> Result<()> {
    if let Some(path) = path {
        ensure!(
            path.is_absolute()
                && path.as_os_str().len() <= 4096
                && !path
                    .as_os_str()
                    .to_string_lossy()
                    .chars()
                    .any(char::is_control),
            "invalid protected Explorer attachment reference"
        );
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> Sha256Digest {
    use sha2::{Digest, Sha256};
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

impl Ledger {
    /// Parse ledger bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|_| anyhow::anyhow!("invalid HTTP credential ledger"))
    }
    /// The ledger belongs to `binding` and every record is well formed.
    pub fn validate(&self, binding: &InstallationBinding) -> Result<()> {
        ensure!(
            self.schema == 1 && self.binding == *binding && self.records.len() <= MAX_CREDENTIALS,
            "HTTP credential ledger binding or bound mismatch"
        );
        let mut ids = std::collections::BTreeSet::new();
        for record in &self.records {
            record.selection.validate()?;
            validate_attachment(record.explorer_descriptor.as_deref())?;
            ensure!(
                record.selection.namespace() == binding.namespace
                    && record.generation > 0
                    && (record.revoked || record.revocation_reason.is_none())
                    && ids.insert(record.id),
                "invalid HTTP credential ledger record"
            );
        }
        Ok(())
    }
    /// Authenticate `token` against an active, unexpired record in constant time.
    pub fn authenticate(&self, token: &str, now_unix_ms: u64) -> Result<AuthenticatedCredential> {
        let id = token_id(token)?;
        let record = self
            .records
            .iter()
            .find(|record| record.id == id)
            .ok_or_else(|| anyhow::anyhow!("HTTP credential rejected"))?;
        ensure!(
            !record.revoked
                && now_unix_ms < record.expires_unix_ms
                && bool::from(
                    record
                        .verifier
                        .as_bytes()
                        .ct_eq(digest(token.as_bytes()).as_bytes())
                ),
            "HTTP credential rejected"
        );
        Ok(AuthenticatedCredential {
            client: record.client,
            connection_id: id,
            generation: record.generation,
            selection: record.selection.clone(),
            expires_unix_ms: record.expires_unix_ms,
            explorer_descriptor: record.explorer_descriptor.clone(),
        })
    }
}
