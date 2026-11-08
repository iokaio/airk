// SPDX-License-Identifier: Apache-2.0
use super::{InstallationBinding, digest, read_private, write_private};
use airk_core::usage::ClientFamily;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_http::HttpSelection,
};
use anyhow::{Result, ensure};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};
use subtle::ConstantTimeEq;

fn unknown_client() -> ClientFamily {
    ClientFamily::Unknown
}

const MAX_STORE: usize = 1_048_576;
const MAX_CREDENTIALS: usize = 128;
const MAX_TOKEN: usize = 103;
const STORE: &str = "credentials.json";
const DEFAULT_LIFETIME_MS: u64 = 365 * 86_400_000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    #[serde(default = "unknown_client")]
    client: ClientFamily,
    id: Incarnation,
    generation: u64,
    selection: HttpSelection,
    #[serde(default)]
    explorer_descriptor: Option<PathBuf>,
    expires_unix_ms: u64,
    verifier: Sha256Digest,
    revoked: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    schema: u32,
    binding: InstallationBinding,
    records: Vec<Record>,
}

/// Callers serialize native mutations. Request authentication must re-read this
/// store and revalidate the selected native grant before every operation.
pub struct CredentialStore {
    root: PathBuf,
    ledger: Ledger,
}
pub struct IssuedCredential {
    pub connection_id: Incarnation,
    pub generation: u64,
    pub credential_file: PathBuf,
    pub explorer_descriptor: Option<PathBuf>,
}
#[derive(Clone)]
pub struct AuthenticatedCredential {
    pub client: ClientFamily,
    pub connection_id: Incarnation,
    pub generation: u64,
    pub selection: HttpSelection,
    pub expires_unix_ms: u64,
    pub explorer_descriptor: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionOwnership {
    NotOwned,
    Active,
    Revoked,
    Expired,
}

fn filename(id: Incarnation) -> String {
    format!("credential-{id}.token")
}
fn random_token(id: Incarnation) -> Result<String> {
    let mut random = [0u8; 32];
    SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| anyhow::anyhow!("HTTP credential randomness unavailable"))?;
    let mut token = format!("airk1.{id}.");
    use std::fmt::Write;
    for byte in random {
        write!(token, "{byte:02x}")?;
    }
    Ok(token)
}
fn token_id(token: &str) -> Result<Incarnation> {
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
pub fn read_client_token(root: &Path, id: Incarnation) -> Result<String> {
    let bytes = read_private(root, &filename(id), MAX_TOKEN)?
        .ok_or_else(|| anyhow::anyhow!("HTTP client credential unavailable"))?;
    let token =
        String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("HTTP client credential invalid"))?;
    ensure!(
        token_id(&token)? == id,
        "HTTP client credential identity mismatch"
    );
    Ok(token)
}
impl CredentialStore {
    pub fn open(root: &Path, binding: InstallationBinding) -> Result<Self> {
        binding.validate()?;
        crate::local_ipc::prepare_root(root)?;
        let ledger: Ledger = match read_private(root, STORE, MAX_STORE)? {
            Some(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| anyhow::anyhow!("invalid HTTP credential ledger"))?,
            None => Ledger {
                schema: 1,
                binding: binding.clone(),
                records: Vec::new(),
            },
        };
        Self::validated(root, binding, ledger)
    }
    /// Ownership lookup never creates a ledger or tightens filesystem ACLs. A
    /// missing/unreadable ledger is unknown, never proof that an ID is foreign.
    pub fn read_existing(root: &Path, binding: InstallationBinding) -> Result<Self> {
        binding.validate()?;
        crate::local_ipc::require_existing_root(root)?;
        let raw = read_private(root, STORE, MAX_STORE)?
            .ok_or_else(|| anyhow::anyhow!("HTTP credential ledger is unobserved"))?;
        let ledger: Ledger = serde_json::from_slice(&raw)
            .map_err(|_| anyhow::anyhow!("invalid HTTP credential ledger"))?;
        Self::validated(root, binding, ledger)
    }
    fn validated(root: &Path, binding: InstallationBinding, ledger: Ledger) -> Result<Self> {
        ensure!(
            ledger.schema == 1
                && ledger.binding == binding
                && ledger.records.len() <= MAX_CREDENTIALS,
            "HTTP credential ledger binding or bound mismatch"
        );
        let mut ids = std::collections::BTreeSet::new();
        for record in &ledger.records {
            record.selection.validate()?;
            validate_attachment(record.explorer_descriptor.as_deref())?;
            ensure!(
                record.selection.namespace() == binding.namespace
                    && record.generation > 0
                    && ids.insert(record.id),
                "invalid HTTP credential ledger record"
            );
        }
        Ok(Self {
            root: root.to_owned(),
            ledger,
        })
    }
    pub fn ownership(&self, id: Incarnation, now_unix_ms: u64) -> Result<ConnectionOwnership> {
        let current = Self::read_existing(&self.root, self.ledger.binding.clone())?;
        Ok(
            match current.ledger.records.iter().find(|record| record.id == id) {
                None => ConnectionOwnership::NotOwned,
                Some(record) if record.revoked => ConnectionOwnership::Revoked,
                Some(record) if record.expires_unix_ms <= now_unix_ms => {
                    ConnectionOwnership::Expired
                }
                Some(_) => ConnectionOwnership::Active,
            },
        )
    }
    fn commit(&mut self, ledger: Ledger) -> Result<()> {
        write_private(
            &self.root,
            STORE,
            &serde_json::to_vec(&ledger)?,
            MAX_STORE,
            true,
        )?;
        self.ledger = ledger;
        Ok(())
    }
    /// Called only after native control revalidates the live grant. Metadata
    /// revision advancement cannot widen the immutable credential authority.
    pub fn enroll_default(
        &mut self,
        id: Incarnation,
        selection: HttpSelection,
        now_unix_ms: u64,
    ) -> Result<IssuedCredential> {
        self.enroll_default_attached(id, selection, None, now_unix_ms)
    }
    /// Native control validates exact window ownership/session/expiry before
    /// enrollment; request-time attachment lookup repeats those checks.
    pub fn enroll_default_attached(
        &mut self,
        id: Incarnation,
        selection: HttpSelection,
        explorer_descriptor: Option<PathBuf>,
        now_unix_ms: u64,
    ) -> Result<IssuedCredential> {
        self.enroll_default_for_client(
            id,
            selection,
            explorer_descriptor,
            ClientFamily::Unknown,
            now_unix_ms,
        )
    }
    pub fn enroll_default_for_client(
        &mut self,
        id: Incarnation,
        selection: HttpSelection,
        explorer_descriptor: Option<PathBuf>,
        client: ClientFamily,
        now_unix_ms: u64,
    ) -> Result<IssuedCredential> {
        selection.validate()?;
        validate_attachment(explorer_descriptor.as_deref())?;
        *self = Self::open(&self.root, self.ledger.binding.clone())?;
        if let Some(record) = self.ledger.records.iter().find(|record| record.id == id) {
            ensure!(
                !record.revoked && record.expires_unix_ms > now_unix_ms,
                "expired or revoked HTTP credential requires a new connection ID"
            );
            ensure!(
                same_authority(&record.selection, &selection)
                    && record.client == client
                    && record.explorer_descriptor == explorer_descriptor,
                "HTTP connection authority changed; use a new connection ID"
            );
            let token = read_client_token(&self.root, id)?;
            ensure!(
                bool::from(
                    record
                        .verifier
                        .as_bytes()
                        .ct_eq(digest(token.as_bytes()).as_bytes())
                ),
                "HTTP enrollment credential mismatch"
            );
            let generation = record.generation;
            if record.selection != selection {
                let mut next = self.ledger.clone();
                next.records
                    .iter_mut()
                    .find(|record| record.id == id)
                    .unwrap()
                    .selection = selection;
                self.commit(next)?;
            }
            return Ok(IssuedCredential {
                connection_id: id,
                generation,
                credential_file: self.root.join(filename(id)),
                explorer_descriptor,
            });
        }
        let expires = now_unix_ms
            .checked_add(DEFAULT_LIFETIME_MS)
            .ok_or_else(|| anyhow::anyhow!("HTTP credential expiry overflow"))?;
        self.enroll_for_client(
            id,
            selection,
            explorer_descriptor,
            client,
            expires,
            now_unix_ms,
        )
    }
    /// Native control has already reviewed the live grant. Same-ID recovery
    /// requires the exact original intent and matching immutable client file.
    pub fn enroll(
        &mut self,
        id: Incarnation,
        selection: HttpSelection,
        expires_unix_ms: u64,
        now_unix_ms: u64,
    ) -> Result<IssuedCredential> {
        self.enroll_attached(id, selection, None, expires_unix_ms, now_unix_ms)
    }
    pub fn enroll_attached(
        &mut self,
        id: Incarnation,
        selection: HttpSelection,
        explorer_descriptor: Option<PathBuf>,
        expires_unix_ms: u64,
        now_unix_ms: u64,
    ) -> Result<IssuedCredential> {
        self.enroll_for_client(
            id,
            selection,
            explorer_descriptor,
            ClientFamily::Unknown,
            expires_unix_ms,
            now_unix_ms,
        )
    }
    fn enroll_for_client(
        &mut self,
        id: Incarnation,
        selection: HttpSelection,
        explorer_descriptor: Option<PathBuf>,
        client: ClientFamily,
        expires_unix_ms: u64,
        now_unix_ms: u64,
    ) -> Result<IssuedCredential> {
        selection.validate()?;
        validate_attachment(explorer_descriptor.as_deref())?;
        ensure!(
            selection.namespace() == self.ledger.binding.namespace && expires_unix_ms > now_unix_ms,
            "invalid HTTP credential scope or expiry"
        );
        if let Some(record) = self.ledger.records.iter().find(|record| record.id == id) {
            ensure!(
                !record.revoked
                    && record.client == client
                    && record.selection == selection
                    && record.explorer_descriptor == explorer_descriptor
                    && record.expires_unix_ms == expires_unix_ms,
                "HTTP enrollment intent mismatch or revoked credential"
            );
            let token = read_client_token(&self.root, id)?;
            ensure!(
                bool::from(
                    record
                        .verifier
                        .as_bytes()
                        .ct_eq(digest(token.as_bytes()).as_bytes())
                ),
                "HTTP enrollment credential mismatch"
            );
            return Ok(IssuedCredential {
                connection_id: id,
                generation: record.generation,
                credential_file: self.root.join(filename(id)),
                explorer_descriptor,
            });
        }
        ensure!(
            self.ledger.records.len() < MAX_CREDENTIALS,
            "HTTP credential storage limit reached; explicit cleanup required"
        );
        let token = random_token(id)?;
        let mut next = self.ledger.clone();
        next.records.push(Record {
            client,
            id,
            generation: 1,
            selection,
            explorer_descriptor: explorer_descriptor.clone(),
            expires_unix_ms,
            verifier: digest(token.as_bytes()),
            revoked: false,
        });
        // Refuse predictable aggregate-capacity failures before publishing the
        // immutable token. Uncertain I/O failures still preserve orphan evidence.
        airk_core::index_catalog::encoded_size(&next, MAX_STORE)?;
        write_private(
            &self.root,
            &filename(id),
            token.as_bytes(),
            MAX_TOKEN,
            false,
        )?;
        self.commit(next)?;
        Ok(IssuedCredential {
            connection_id: id,
            generation: 1,
            credential_file: self.root.join(filename(id)),
            explorer_descriptor,
        })
    }
    pub fn authenticate(&self, token: &str, now_unix_ms: u64) -> Result<AuthenticatedCredential> {
        // A previously opened reader cannot retain authority after native
        // revocation has durably replaced the ledger.
        let current = Self::read_existing(&self.root, self.ledger.binding.clone())?;
        let id = token_id(token)?;
        let record = current
            .ledger
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
    pub fn revoke(&mut self, id: Incarnation) -> Result<()> {
        let mut next = self.ledger.clone();
        let record = next
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| anyhow::anyhow!("HTTP connection not found"))?;
        if record.revoked {
            return Ok(());
        }
        record.generation = record
            .generation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("HTTP credential generation exhausted"))?;
        record.revoked = true;
        self.commit(next)
    }
}

fn validate_attachment(path: Option<&Path>) -> Result<()> {
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

fn same_authority(previous: &HttpSelection, next: &HttpSelection) -> bool {
    let mut normalized = previous.clone();
    match (&mut normalized, next) {
        (HttpSelection::Scoped { selection: old }, HttpSelection::Scoped { selection: new }) => {
            if new.expected_catalog_revision < old.expected_catalog_revision {
                return false;
            }
            old.expected_catalog_revision = new.expected_catalog_revision;
        }
        (
            HttpSelection::Combined { selection: old },
            HttpSelection::Combined { selection: new },
        ) => {
            if new.expected_catalog_revision < old.expected_catalog_revision {
                return false;
            }
            old.expected_catalog_revision = new.expected_catalog_revision;
        }
        (
            HttpSelection::ImportedLegacy { selection: old },
            HttpSelection::ImportedLegacy { selection: new },
        ) => {
            if new.expected_catalog_revision < old.expected_catalog_revision {
                return false;
            }
            old.expected_catalog_revision = new.expected_catalog_revision;
        }
        (
            HttpSelection::ImportedLegacyUnion { selection: old },
            HttpSelection::ImportedLegacyUnion { selection: new },
        ) => {
            if old.selections.len() != new.selections.len() {
                return false;
            }
            for (old, new) in old.selections.iter_mut().zip(&new.selections) {
                if new.expected_catalog_revision < old.expected_catalog_revision {
                    return false;
                }
                old.expected_catalog_revision = new.expected_catalog_revision;
            }
        }
        _ => return false,
    }
    normalized == *next
}
