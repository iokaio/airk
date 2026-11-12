// SPDX-License-Identifier: Apache-2.0
use super::{InstallationBinding, digest, private_file, read_private, write_private};
pub use airk_core::service_http_credentials::AuthenticatedCredential;
use airk_core::service_http_credentials::{
    Ledger, MAX_CREDENTIALS, MAX_STORE, MAX_TOKEN, Record, RevocationReason, STORE, filename,
    validate_attachment,
};
use airk_core::usage::ClientFamily;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_http::HttpSelection,
    service_http_cleanup::{
        HttpCleanupOwner, HttpCleanupOwnership, MAX_HTTP_CLEANUP_BYTES, MAX_HTTP_CLEANUP_OWNERS,
    },
};
use anyhow::{Result, ensure};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use subtle::ConstantTimeEq;

const MAX_RETIRED: usize = 2048;
const MAX_CLEANUP_DIRECTORY_ENTRIES: usize = 2 * MAX_CREDENTIALS;
const DEFAULT_LIFETIME_MS: u64 = 365 * 86_400_000;

/// Stored at the original token path only after native revocation commits.
/// This is issuance/retirement proof, unlike an orphan token from failed setup.
#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Retired {
    schema: u32,
    binding: InstallationBinding,
    id: Incarnation,
    revoked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retirement: Option<RetirementMetadata>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetirementMetadata {
    reason: RevocationReason,
    client: ClientFamily,
    expires_unix_ms: u64,
    explorer_descriptor_sha256: Option<Sha256Digest>,
    authority_sha256: Sha256Digest,
}
/// Positive native-owned attachment retirement evidence, never new grant authority.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentRetirementProof {
    pub schema: u32,
    pub connection_id: Incarnation,
    pub installation: Sha256Digest,
    pub namespace: Incarnation,
    pub client: ClientFamily,
    pub expires_unix_ms: u64,
    pub explorer_descriptor_sha256: Sha256Digest,
    pub authority_sha256: Sha256Digest,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionOwnership {
    NotOwned,
    Active,
    Revoked,
    Expired,
}

fn cleanup_credential_files(root: &Path) -> Result<Vec<(Incarnation, String, u64)>> {
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
        let text = name
            .strip_prefix("credential-")
            .and_then(|name| name.strip_suffix(".token"))
            .ok_or_else(|| anyhow::anyhow!("HTTP cleanup credential filename is unobserved"))?;
        let id: Incarnation = text
            .parse()
            .map_err(|_| anyhow::anyhow!("HTTP cleanup credential identity is unobserved"))?;
        ensure!(
            filename(id) == name,
            "HTTP cleanup credential filename is not canonical"
        );
        private_file(&entry.path())?;
        let size = std::fs::symlink_metadata(entry.path())?.len();
        files.push((id, name.to_owned(), size));
        ensure!(
            files.len() <= MAX_HTTP_CLEANUP_OWNERS,
            "HTTP cleanup inventory exceeds bound"
        );
    }
    files.sort_by_key(|(id, _, _)| *id);
    Ok(files)
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
pub fn read_client_token(root: &Path, id: Incarnation) -> Result<String> {
    let bytes = read_private(root, &filename(id), MAX_TOKEN)?
        .ok_or_else(|| anyhow::anyhow!("HTTP client credential unavailable"))?;
    airk_core::service_http_credentials::client_token(bytes, id)
}
impl CredentialStore {
    pub fn open(root: &Path, binding: InstallationBinding) -> Result<Self> {
        crate::http_endpoint::validate_binding(&binding)?;
        crate::local_ipc::prepare_root(root)?;
        let ledger: Ledger = match read_private(root, STORE, MAX_STORE)? {
            Some(bytes) => Ledger::parse(&bytes)?,
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
        crate::http_endpoint::validate_binding(&binding)?;
        crate::local_ipc::require_existing_root(root)?;
        let raw = read_private(root, STORE, MAX_STORE)?
            .ok_or_else(|| anyhow::anyhow!("HTTP credential ledger is unobserved"))?;
        let ledger = Ledger::parse(&raw)?;
        Self::validated(root, binding, ledger)
    }
    /// Establish the first durable empty ledger only from the actual service
    /// owner, before publishing an endpoint. This is not a missing-ledger repair
    /// API: any published endpoint or credential file preserves unknown history.
    pub fn initialize_empty_owned(
        &mut self,
        owner: &airk_store::service::ServiceLease,
    ) -> Result<()> {
        let control = owner.root();
        let root = self.root.canonicalize()?;
        ensure!(
            root == control.join("http-1"),
            "HTTP ledger initializer belongs to another service owner"
        );
        let namespace =
            airk_store::service::registry::ServiceRegistry::open_saved(control)?.incarnation()?;
        let binding = super::installation_binding(control, namespace)?;
        ensure!(
            self.ledger.binding == binding,
            "HTTP ledger initializer namespace or installation changed"
        );
        match std::fs::symlink_metadata(root.join(STORE)) {
            Ok(_) => {
                *self = Self::read_existing(&root, binding)?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        ensure!(
            self.ledger.records.is_empty(),
            "HTTP ledger disappeared after issuance"
        );
        ensure!(
            super::load_descriptor(&root, &binding)?.is_none(),
            "published HTTP endpoint has an unobserved credential ledger"
        );
        ensure!(
            cleanup_credential_files(&root)?.is_empty(),
            "HTTP credential history exists without its ledger"
        );
        ensure!(
            std::fs::read_dir(&root)?.next().is_none(),
            "HTTP initialization history exists without its ledger"
        );
        let ledger = Ledger {
            schema: 1,
            binding: binding.clone(),
            records: Vec::new(),
        };
        write_private(
            &root,
            STORE,
            &serde_json::to_vec(&ledger)?,
            MAX_STORE,
            false,
        )?;
        *self = Self::read_existing(&root, binding)?;
        Ok(())
    }
    fn validated(root: &Path, binding: InstallationBinding, ledger: Ledger) -> Result<Self> {
        ledger.validate(&binding)?;
        Ok(Self {
            root: root.to_owned(),
            ledger,
        })
    }
    pub fn ownership(&self, id: Incarnation, now_unix_ms: u64) -> Result<ConnectionOwnership> {
        let current = Self::read_existing(&self.root, self.ledger.binding.clone())?;
        Ok(
            match current.ledger.records.iter().find(|record| record.id == id) {
                None => match current.retired(id)? {
                    true => ConnectionOwnership::Revoked,
                    false => ConnectionOwnership::NotOwned,
                },
                Some(record) if record.revoked => ConnectionOwnership::Revoked,
                Some(record) if record.expires_unix_ms <= now_unix_ms => {
                    ConnectionOwnership::Expired
                }
                Some(_) => ConnectionOwnership::Active,
            },
        )
    }
    /// Complete disposal-only ownership metadata. The caller retains original
    /// native mutation exclusion through capture. This never reads active token
    /// bodies, authenticates a credential, creates a ledger or opens a socket.
    /// A positive ledger record keeps ownership even if its token is missing.
    pub fn cleanup_owners(&self, now_unix_ms: u64) -> Result<Vec<HttpCleanupOwner>> {
        let current = Self::read_existing(&self.root, self.ledger.binding.clone())?;
        let mut owners = std::collections::BTreeMap::new();
        for record in &current.ledger.records {
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
        let files = cleanup_credential_files(&current.root)?;
        for (id, _, size) in &files {
            if let Some(record) = current
                .ledger
                .records
                .iter()
                .find(|record| record.id == *id)
            {
                if *size == MAX_TOKEN as u64 {
                    continue; // The existing verifier is sufficient; no secret read.
                }
                ensure!(
                    record.revoked && *size > MAX_TOKEN as u64,
                    "HTTP cleanup credential file state is unobserved"
                );
                ensure!(
                    current.read_retirement(*id)?.is_some(),
                    "HTTP cleanup retirement metadata is unobserved"
                );
            } else {
                // Native credentials are exactly 103 bytes. Without a committed
                // record those bytes are an orphan, never an ownership marker.
                ensure!(
                    *size > MAX_TOKEN as u64 && *size <= MAX_RETIRED as u64,
                    "HTTP cleanup orphan credential ownership is unobserved"
                );
                ensure!(
                    current.read_retirement(*id)?.is_some(),
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
        let checked = Self::read_existing(&self.root, self.ledger.binding.clone())?;
        ensure!(
            serde_json::to_vec(&checked.ledger)? == serde_json::to_vec(&current.ledger)?
                && cleanup_credential_files(&current.root)? == files,
            "HTTP cleanup ownership changed during capture"
        );
        let owners: Vec<_> = owners.into_values().collect();
        ensure!(
            owners.len() <= MAX_HTTP_CLEANUP_OWNERS,
            "HTTP cleanup inventory exceeds bound"
        );
        airk_core::index_catalog::encoded_size(&owners, MAX_HTTP_CLEANUP_BYTES)?;
        Ok(owners)
    }
    fn retired(&self, id: Incarnation) -> Result<bool> {
        Ok(self.read_retirement(id)?.is_some())
    }
    fn read_retirement(&self, id: Incarnation) -> Result<Option<Retired>> {
        let Some(bytes) = read_private(&self.root, &filename(id), MAX_RETIRED)? else {
            return Ok(None);
        };
        let retired: Retired = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("HTTP credential issuance ownership is unobserved"))?;
        ensure!(
            retired.binding == self.ledger.binding
                && retired.id == id
                && retired.revoked
                && ((retired.schema == 1 && retired.retirement.is_none())
                    || (retired.schema == 2 && retired.retirement.is_some())),
            "HTTP retired credential binding mismatch"
        );
        Ok(Some(retired))
    }
    fn retirement(&self, record: &Record) -> Result<Retired> {
        Ok(Retired {
            schema: 2,
            binding: self.ledger.binding.clone(),
            id: record.id,
            revoked: true,
            retirement: Some(Self::retirement_metadata(
                record,
                record
                    .revocation_reason
                    .unwrap_or(RevocationReason::Unknown),
            )?),
        })
    }
    fn retirement_metadata(
        record: &Record,
        reason: RevocationReason,
    ) -> Result<RetirementMetadata> {
        Ok(RetirementMetadata {
            reason,
            client: record.client,
            expires_unix_ms: record.expires_unix_ms,
            explorer_descriptor_sha256: record
                .explorer_descriptor
                .as_ref()
                .map(|path| {
                    Ok::<_, anyhow::Error>(airk_core::identity::framed_digest(
                        b"airk-http-explorer-attachment/1",
                        &[&serde_json::to_vec(path)?],
                    ))
                })
                .transpose()?,
            authority_sha256: authority_digest(&record.selection)?,
        })
    }
    pub fn attachment_retirement(
        &self,
        id: Incarnation,
        now_unix_ms: u64,
    ) -> Result<Option<AttachmentRetirementProof>> {
        let current = Self::read_existing(&self.root, self.ledger.binding.clone())?;
        let metadata = match current.ledger.records.iter().find(|record| record.id == id) {
            Some(record) if record.expires_unix_ms <= now_unix_ms => return Ok(None),
            Some(record) if record.revoked => Self::retirement_metadata(
                record,
                record
                    .revocation_reason
                    .unwrap_or(RevocationReason::Unknown),
            )?,
            Some(record) => {
                let Some(path) = record.explorer_descriptor.as_deref() else {
                    return Ok(None);
                };
                if !crate::explorer_assistance::window_retired(path, now_unix_ms).unwrap_or(false) {
                    return Ok(None);
                }
                Self::retirement_metadata(record, RevocationReason::AttachmentRetired)?
            }
            None => match current
                .read_retirement(id)?
                .and_then(|marker| marker.retirement)
            {
                Some(metadata) => metadata,
                None => return Ok(None),
            },
        };
        let Some(descriptor) = metadata.explorer_descriptor_sha256 else {
            return Ok(None);
        };
        if metadata.reason != RevocationReason::AttachmentRetired
            || metadata.expires_unix_ms <= now_unix_ms
        {
            return Ok(None);
        }
        Ok(Some(AttachmentRetirementProof {
            schema: 1,
            connection_id: id,
            installation: current.ledger.binding.installation,
            namespace: current.ledger.binding.namespace,
            client: metadata.client,
            expires_unix_ms: metadata.expires_unix_ms,
            explorer_descriptor_sha256: descriptor,
            authority_sha256: metadata.authority_sha256,
        }))
    }
    /// Recheck under the service's credential mutex immediately before issuance.
    /// A read-only lookup is insufficient: explicit revocation can race it.
    pub(crate) fn validate_reattachment(
        &self,
        previous: Incarnation,
        next: Incarnation,
        selection: &HttpSelection,
        client: ClientFamily,
        explorer_descriptor: Option<&Path>,
        now_unix_ms: u64,
    ) -> Result<()> {
        ensure!(
            previous != next,
            "HTTP reattachment cannot reuse retired identity"
        );
        ensure!(
            matches!(selection, HttpSelection::ImportedLegacy { selection } if matches!(selection.grant.provenance, airk_core::grant_provenance::GrantProvenance::ImportedStandaloneAdoption { .. })),
            "HTTP reattachment requires original standalone adoption"
        );
        let proof = self
            .attachment_retirement(previous, now_unix_ms)?
            .ok_or_else(|| anyhow::anyhow!("HTTP attachment retirement unconfirmed"))?;
        let descriptor = explorer_descriptor.ok_or_else(|| {
            anyhow::anyhow!("HTTP reattachment requires a new live Explorer attachment")
        })?;
        validate_attachment(Some(descriptor))?;
        ensure!(
            !crate::explorer_assistance::window_retired(descriptor, now_unix_ms)?,
            "HTTP replacement attachment is retired"
        );
        let descriptor_sha256 = airk_core::identity::framed_digest(
            b"airk-http-explorer-attachment/1",
            &[&serde_json::to_vec(descriptor)?],
        );
        ensure!(
            proof.client == client
                && proof.authority_sha256 == authority_digest(selection)?
                && proof.explorer_descriptor_sha256 != descriptor_sha256,
            "HTTP reattachment authority changed"
        );
        Ok(())
    }
    /// The caller must notify the gateway for every returned ID, including an
    /// uncertain commit error, before compacting or returning to enrollment.
    pub(crate) fn revoke_inactive(&mut self, now_unix_ms: u64) -> (Vec<Incarnation>, Result<()>) {
        let result = Self::open(&self.root, self.ledger.binding.clone());
        match result {
            Ok(current) => *self = current,
            Err(error) => return (Vec::new(), Err(error)),
        }
        let ids: Vec<_> = self
            .ledger
            .records
            .iter()
            .filter(|record| {
                record.revoked
                    || record.expires_unix_ms <= now_unix_ms
                    || record.explorer_descriptor.as_deref().is_some_and(|path| {
                        crate::explorer_assistance::window_retired(path, now_unix_ms)
                            .unwrap_or(false)
                    })
            })
            .map(|record| record.id)
            .collect();
        let result = (|| {
            let mut next = self.ledger.clone();
            let mut changed = false;
            for record in &mut next.records {
                if !record.revoked && ids.contains(&record.id) {
                    record.generation = record
                        .generation
                        .checked_add(1)
                        .ok_or_else(|| anyhow::anyhow!("HTTP credential generation exhausted"))?;
                    record.revoked = true;
                    record.revocation_reason = Some(if record.expires_unix_ms <= now_unix_ms {
                        RevocationReason::Expired
                    } else {
                        RevocationReason::AttachmentRetired
                    });
                    changed = true;
                }
            }
            if changed {
                self.commit(next)?;
            }
            Ok(())
        })();
        (ids, result)
    }
    /// Reclaim active-ledger capacity without discarding issuance ownership or
    /// allowing a retired ID to be enrolled again. Each historical token path
    /// becomes one bounded, secret-free retirement proof; no directory scan.
    pub(crate) fn compact_revoked(&mut self) -> Result<()> {
        *self = Self::open(&self.root, self.ledger.binding.clone())?;
        let mut next = self.ledger.clone();
        for record in self.ledger.records.iter().filter(|record| record.revoked) {
            let name = filename(record.id);
            let existing = read_private(&self.root, &name, MAX_RETIRED)?;
            if let Some(bytes) = existing.as_ref() {
                let token_matches = bytes.len() == MAX_TOKEN
                    && bool::from(record.verifier.as_bytes().ct_eq(digest(bytes).as_bytes()));
                let expected = self.retirement(record)?;
                let marker_matches =
                    serde_json::from_slice::<Retired>(bytes).is_ok_and(|mut marker| {
                        if marker.schema == 1
                            && marker.retirement.is_none()
                            && matches!(
                                record.revocation_reason,
                                None | Some(RevocationReason::Explicit)
                            )
                        {
                            // Full-record ownership can retain unknown or
                            // explicit denial, never infer automatic retirement.
                            marker.schema = 2;
                            marker.retirement = expected.retirement.clone();
                        }
                        if record.revocation_reason == Some(RevocationReason::Explicit)
                            && let Some(metadata) = marker.retirement.as_mut()
                        {
                            // Explicit revocation can commit after marker publication
                            // but before the corresponding full record compacts.
                            metadata.reason = RevocationReason::Explicit;
                        }
                        marker == expected
                    });
                ensure!(
                    token_matches || marker_matches,
                    "HTTP retirement token ownership mismatch"
                );
            }
            write_private(
                &self.root,
                &name,
                &serde_json::to_vec(&self.retirement(record)?)?,
                MAX_RETIRED,
                existing.is_some(),
            )?;
            next.records.retain(|candidate| candidate.id != record.id);
        }
        if next.records.len() != self.ledger.records.len() {
            self.commit(next)?;
        }
        Ok(())
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
            revocation_reason: None,
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
        current.ledger.authenticate(token, now_unix_ms)
    }
    pub fn revoke(&mut self, id: Incarnation) -> Result<()> {
        *self = Self::open(&self.root, self.ledger.binding.clone())?;
        if !self.ledger.records.iter().any(|record| record.id == id)
            && let Some(mut marker) = self.read_retirement(id)?
        {
            if let Some(metadata) = marker.retirement.as_mut() {
                metadata.reason = RevocationReason::Explicit;
                write_private(
                    &self.root,
                    &filename(id),
                    &serde_json::to_vec(&marker)?,
                    MAX_RETIRED,
                    true,
                )?;
            }
            return Ok(());
        }
        let mut next = self.ledger.clone();
        let record = next
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| anyhow::anyhow!("HTTP connection not found"))?;
        if record.revoked && record.revocation_reason == Some(RevocationReason::Explicit) {
            return Ok(());
        }
        if !record.revoked {
            record.generation = record
                .generation
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("HTTP credential generation exhausted"))?;
        }
        record.revoked = true;
        record.revocation_reason = Some(RevocationReason::Explicit);
        self.commit(next)
    }
}

/// Domain-separated digest of the exact reviewed selection, except its saved
/// minimum catalog clock. Object keys are sorted recursively; array order and
/// all original grant, policy, registration and adoption fields remain exact.
pub fn authority_digest(selection: &HttpSelection) -> Result<Sha256Digest> {
    selection.validate()?;
    let mut normalized = selection.clone();
    match &mut normalized {
        HttpSelection::Scoped { selection } => {
            selection.expected_catalog_revision = airk_core::index_catalog::CatalogRevision::INITIAL
        }
        HttpSelection::Combined { selection } => {
            selection.expected_catalog_revision = airk_core::index_catalog::CatalogRevision::INITIAL
        }
        HttpSelection::ImportedLegacy { selection } => {
            selection.expected_catalog_revision = airk_core::index_catalog::CatalogRevision::INITIAL
        }
        HttpSelection::ImportedLegacyUnion { selection } => {
            for member in &mut selection.selections {
                member.expected_catalog_revision =
                    airk_core::index_catalog::CatalogRevision::INITIAL;
            }
        }
    }
    fn sorted(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(values) => serde_json::Value::Object(
                values
                    .into_iter()
                    .collect::<std::collections::BTreeMap<_, _>>()
                    .into_iter()
                    .map(|(key, value)| (key, sorted(value)))
                    .collect(),
            ),
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sorted).collect())
            }
            value => value,
        }
    }
    Ok(airk_core::identity::framed_digest(
        b"airk-http-authority/1",
        &[&serde_json::to_vec(&sorted(serde_json::to_value(
            normalized,
        )?))?],
    ))
}

/// A saved minimum catalog clock may advance without changing the reviewed
/// authority. Every other selection field, including each union member's
/// original lineage, must remain equal.
pub(crate) fn same_authority(previous: &HttpSelection, next: &HttpSelection) -> bool {
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
