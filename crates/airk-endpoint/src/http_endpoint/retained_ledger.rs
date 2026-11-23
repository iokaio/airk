// SPDX-License-Identifier: Apache-2.0
//! The retained credential ledger of an installation from before local trust
//! plan B (23 November 2026). Nothing issues, authenticates, revokes or
//! writes credentials any more; this lenient, read-only reader serves only
//! predecessor cleanup, which must still recognise the connection ids an old
//! installation owned so that their agent entries can be cleaned.
//!
//! It reads only the fields cleanup needs, as untyped JSON: the ledger's
//! `schema` and `binding`, and each record's `id`, `revoked`,
//! `expires_unix_ms` and `verifier`. Any other field (the selection, client,
//! open state, ...) is ignored, so an older or newer record shape is still
//! recognised. It never creates, repairs or rewrites the ledger or a token
//! file: the retained bytes stay identical. A missing or unreadable ledger is
//! unknown, never proof that an id is foreign.
use super::{InstallationBinding, private_file, read_private};
use airk_core::identity::{Incarnation, Sha256Digest};
use airk_core::service_http_cleanup::{
    HttpCleanupOwner, HttpCleanupOwnership, MAX_HTTP_CLEANUP_BYTES, MAX_HTTP_CLEANUP_OWNERS,
};
use airk_core::service_http_credentials::{MAX_CREDENTIALS, MAX_STORE, MAX_TOKEN, STORE, filename};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The largest retirement marker an old installation wrote in place of a
/// revoked token.
const MAX_RETIRED: usize = 2048;
const MAX_CLEANUP_DIRECTORY_ENTRIES: usize = 2 * MAX_CREDENTIALS;

/// One record of the retained ledger, as cleanup reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedRecord {
    id: Incarnation,
    revoked: bool,
    expires_unix_ms: u64,
    verifier: Sha256Digest,
}

/// The read-only view of one retained ledger.
pub struct RetainedLedger {
    root: PathBuf,
    binding: InstallationBinding,
    bytes: Vec<u8>,
    records: Vec<RetainedRecord>,
}

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .with_context(|| format!("retained HTTP ledger field {key} is unobserved"))
}

fn parse(bytes: &[u8], binding: &InstallationBinding) -> Result<Vec<RetainedRecord>> {
    let ledger: Value =
        serde_json::from_slice(bytes).context("retained HTTP ledger is not JSON")?;
    ensure!(
        field(&ledger, "schema")?.as_u64() == Some(1),
        "retained HTTP ledger schema is unobserved"
    );
    ensure!(
        *field(&ledger, "binding")? == serde_json::to_value(binding)?,
        "retained HTTP ledger binding mismatch"
    );
    let records = field(&ledger, "records")?
        .as_array()
        .context("retained HTTP ledger records are unobserved")?;
    ensure!(
        records.len() <= MAX_CREDENTIALS,
        "retained HTTP ledger exceeds its record bound"
    );
    let mut parsed = Vec::with_capacity(records.len());
    let mut ids = std::collections::BTreeSet::new();
    for record in records {
        let id: Incarnation = serde_json::from_value(field(record, "id")?.clone())
            .context("retained HTTP ledger record id is unobserved")?;
        let revoked = field(record, "revoked")?
            .as_bool()
            .context("retained HTTP ledger record state is unobserved")?;
        let expires_unix_ms = field(record, "expires_unix_ms")?
            .as_u64()
            .context("retained HTTP ledger record expiry is unobserved")?;
        let verifier: Sha256Digest = serde_json::from_value(field(record, "verifier")?.clone())
            .context("retained HTTP ledger record verifier is unobserved")?;
        ensure!(ids.insert(id), "retained HTTP ledger repeats a record");
        parsed.push(RetainedRecord {
            id,
            revoked,
            expires_unix_ms,
            verifier,
        });
    }
    Ok(parsed)
}

/// The canonical `credential-<id>.token` files (or retirement markers) of
/// `root`, with their sizes. Any other file is ignored.
fn credential_files(root: &Path) -> Result<Vec<(Incarnation, u64)>> {
    crate::local_ipc::require_existing_root(root)?;
    let mut files = Vec::new();
    for (count, entry) in std::fs::read_dir(root)?.enumerate() {
        ensure!(
            count < MAX_CLEANUP_DIRECTORY_ENTRIES,
            "HTTP cleanup directory census exceeds bound"
        );
        let entry = entry?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("HTTP cleanup filename is unobserved"))?;
        if !name.starts_with("credential-") {
            continue;
        }
        let id: Incarnation = name
            .strip_prefix("credential-")
            .and_then(|name| name.strip_suffix(".token"))
            .ok_or_else(|| anyhow::anyhow!("HTTP cleanup credential filename is unobserved"))?
            .parse()
            .map_err(|_| anyhow::anyhow!("HTTP cleanup credential identity is unobserved"))?;
        ensure!(
            filename(id) == name,
            "HTTP cleanup credential filename is not canonical"
        );
        private_file(&entry.path())?;
        files.push((id, std::fs::symlink_metadata(entry.path())?.len()));
        ensure!(
            files.len() <= MAX_HTTP_CLEANUP_OWNERS,
            "HTTP cleanup inventory exceeds bound"
        );
    }
    files.sort_by_key(|(id, _)| *id);
    Ok(files)
}

/// Whether `root` holds no retained ledger and no credential file: an
/// installation that never issued a connection (every installation since
/// local trust plan B). An unreadable root is not empty.
pub fn holds_no_credentials(root: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(root.join(STORE)) {
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(credential_files(root)?.is_empty())
}

impl RetainedLedger {
    /// Read an existing retained ledger. Never creates a ledger or tightens
    /// filesystem access.
    pub fn read_existing(root: &Path, binding: InstallationBinding) -> Result<Self> {
        super::validate_binding(&binding)?;
        crate::local_ipc::require_existing_root(root)?;
        let bytes = read_private(root, STORE, MAX_STORE)?
            .ok_or_else(|| anyhow::anyhow!("HTTP credential ledger is unobserved"))?;
        let records = parse(&bytes, &binding)?;
        Ok(Self {
            root: root.to_owned(),
            binding,
            bytes,
            records,
        })
    }

    /// A retirement marker written in place of a revoked token: owner-private
    /// JSON naming this binding and id, revoked, schema 1 or 2.
    fn retired(&self, id: Incarnation) -> Result<bool> {
        let Some(bytes) = read_private(&self.root, &filename(id), MAX_RETIRED)? else {
            return Ok(false);
        };
        let marker: Value = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("HTTP credential issuance ownership is unobserved"))?;
        let marker_id: Option<Incarnation> = marker
            .get("id")
            .cloned()
            .and_then(|id| serde_json::from_value(id).ok());
        ensure!(
            marker.get("binding") == Some(&serde_json::to_value(&self.binding)?)
                && marker_id == Some(id)
                && marker.get("revoked").and_then(Value::as_bool) == Some(true)
                && matches!(marker.get("schema").and_then(Value::as_u64), Some(1 | 2)),
            "HTTP retired credential binding mismatch"
        );
        Ok(true)
    }

    /// Complete disposal-only ownership metadata. Never reads an active
    /// token's bytes, authenticates, creates a ledger or opens a socket. A
    /// ledger record keeps ownership even if its token is missing; a
    /// retirement marker without a record is a revoked owner.
    pub fn cleanup_owners(&self, now_unix_ms: u64) -> Result<Vec<HttpCleanupOwner>> {
        let mut owners = std::collections::BTreeMap::new();
        for record in &self.records {
            let ownership = if record.revoked {
                HttpCleanupOwnership::Revoked
            } else if record.expires_unix_ms <= now_unix_ms {
                HttpCleanupOwnership::Expired
            } else {
                HttpCleanupOwnership::Active
            };
            owners.insert(
                record.id,
                HttpCleanupOwner {
                    connection_id: record.id,
                    ownership,
                    credential_sha256: Some(record.verifier),
                },
            );
        }
        let files = credential_files(&self.root)?;
        for (id, size) in &files {
            if let Some(record) = self.records.iter().find(|record| record.id == *id) {
                if *size == MAX_TOKEN as u64 {
                    continue; // The ledger verifier is sufficient; no secret read.
                }
                ensure!(
                    record.revoked && *size > MAX_TOKEN as u64,
                    "HTTP cleanup credential file state is unobserved"
                );
                ensure!(
                    self.retired(*id)?,
                    "HTTP cleanup retirement metadata is unobserved"
                );
            } else {
                // Native credentials are exactly 103 bytes. Without a record
                // those bytes are an orphan, never an ownership marker.
                ensure!(
                    *size > MAX_TOKEN as u64 && *size <= MAX_RETIRED as u64,
                    "HTTP cleanup orphan credential ownership is unobserved"
                );
                ensure!(
                    self.retired(*id)?,
                    "HTTP cleanup retirement metadata is unobserved"
                );
                owners.insert(
                    *id,
                    HttpCleanupOwner {
                        connection_id: *id,
                        ownership: HttpCleanupOwnership::Revoked,
                        credential_sha256: None,
                    },
                );
            }
            ensure!(
                owners.len() <= MAX_HTTP_CLEANUP_OWNERS,
                "HTTP cleanup inventory exceeds bound"
            );
        }
        let again = read_private(&self.root, STORE, MAX_STORE)?;
        ensure!(
            again.as_deref() == Some(self.bytes.as_slice())
                && credential_files(&self.root)? == files,
            "HTTP cleanup ownership changed during capture"
        );
        let owners: Vec<_> = owners.into_values().collect();
        airk_core::index_catalog::encoded_size(&owners, MAX_HTTP_CLEANUP_BYTES)?;
        Ok(owners)
    }
}

#[cfg(test)]
mod tests;
