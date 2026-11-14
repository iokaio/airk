// SPDX-License-Identifier: Apache-2.0
//! Private setup recovery metadata. This module performs no filesystem work
//! and never relaxes ordinary I4 pointer/record verification.
pub mod active;
use super::{
    IndexDeletion, InstallPointer, InstallRecord, OperationKind, Refusal, SETUP_RECEIPT_SCHEMA,
    SetupOperation, SetupPhase, SetupReceipt, canonical, decode,
};
use crate::identity::{Incarnation, Sha256Digest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PUBLICATION_SCHEMA: &str = "airk-install-publication/1";
pub const RETAINED_SCHEMA: &str = "airk-install-retained/1";
pub const MAX_RECOVERY_BYTES: usize = 256 * 1024;

/// A complete pair already accepted by the unchanged I4 decoders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationImage {
    pub pointer: InstallPointer,
    pub record: InstallRecord,
}

impl InstallationImage {
    pub fn validate(&self) -> Result<(), Refusal> {
        InstallPointer::decode(&canonical(&self.pointer))?;
        InstallRecord::decode(&canonical(&self.record), &self.pointer)?;
        Ok(())
    }
}

/// Prepared durably before either publication write. Candidate timestamps and
/// identities stay fixed through every resumption of this exact operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationReceipt {
    pub schema: String,
    pub operation_id: Incarnation,
    pub operation_sha256: Sha256Digest,
    pub payload_inventory_sha256: Sha256Digest,
    pub prior: Option<InstallationImage>,
    pub candidate: InstallationImage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationPosition {
    BeforeRecord,
    RecordPublished,
    PairPublished,
}

fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

impl PublicationReceipt {
    pub fn decode(bytes: &[u8], operation: &SetupOperation) -> Result<Self, Refusal> {
        let receipt: Self = decode(bytes, MAX_RECOVERY_BYTES, PUBLICATION_SCHEMA, |r: &Self| {
            &r.schema
        })?;
        receipt.validate_for(operation)?;
        Ok(receipt)
    }

    pub fn validate_for(&self, operation: &SetupOperation) -> Result<(), Refusal> {
        SetupOperation::decode(&canonical(operation), operation.created_unix_ms)?;
        if self.schema != PUBLICATION_SCHEMA {
            return Err(Refusal::UnknownSchema);
        }
        if !matches!(
            operation.kind,
            OperationKind::Install | OperationKind::Repair | OperationKind::Upgrade
        ) || self.operation_id != operation.operation_id
            || self.operation_sha256 != sha256(&canonical(operation))
            || self.payload_inventory_sha256 != operation.payload_inventory_sha256
            || self.candidate.record.locations != operation.locations
            || self.candidate.record.installed_by.operation_id != operation.operation_id
        {
            return Err(Refusal::Malformed);
        }
        self.candidate.validate()?;
        if let Some(prior) = &self.prior {
            prior.validate()?;
        }
        let expected_install_id = self
            .prior
            .as_ref()
            .filter(|prior| prior.pointer.program == self.candidate.pointer.program)
            .map_or_else(
                || {
                    let digest = crate::identity::framed_digest(
                        b"airk/install-id/1",
                        &[operation.operation_id.as_bytes()],
                    );
                    let mut bytes = [0; 16];
                    bytes.copy_from_slice(&digest.as_bytes()[..16]);
                    Incarnation::from_bytes(bytes)
                },
                |prior| prior.record.install_id,
            );
        if self.candidate.record.install_id != expected_install_id {
            return Err(Refusal::InstallIdMismatch);
        }
        Ok(())
    }

    /// Classify exact live bytes observed under the install lock. For a moved
    /// program, `prior_record` is the record at the original program path;
    /// otherwise it must be absent because `candidate_record` covers that path.
    /// The caller first validates this receipt against the exact admitted
    /// operation and independently verifies candidate fields against the
    /// actual pinned payload inventory. Any third state refuses; this function
    /// grants no write authority.
    pub fn observe(
        &self,
        pointer: Option<&[u8]>,
        candidate_record: Option<&[u8]>,
        prior_record: Option<&[u8]>,
    ) -> Result<PublicationPosition, Refusal> {
        if self.schema != PUBLICATION_SCHEMA {
            return Err(Refusal::UnknownSchema);
        }
        self.candidate.validate()?;
        if let Some(prior) = &self.prior {
            prior.validate()?;
        }
        let moved = self
            .prior
            .as_ref()
            .is_some_and(|prior| prior.pointer.program != self.candidate.pointer.program);
        if moved {
            let prior = self.prior.as_ref().ok_or(Refusal::Malformed)?;
            if prior_record != Some(canonical(&prior.record).as_slice()) {
                return Err(Refusal::RecordDigestMismatch);
            }
        } else if prior_record.is_some() {
            return Err(Refusal::Malformed);
        }
        let candidate_pointer = canonical(&self.candidate.pointer);
        let candidate_bytes = canonical(&self.candidate.record);
        if pointer == Some(candidate_pointer.as_slice())
            && candidate_record == Some(candidate_bytes.as_slice())
        {
            return Ok(PublicationPosition::PairPublished);
        }
        let prior_pointer = self.prior.as_ref().map(|prior| canonical(&prior.pointer));
        if pointer != prior_pointer.as_deref() {
            return Err(Refusal::RecordDigestMismatch);
        }
        if candidate_record == Some(candidate_bytes.as_slice()) {
            return Ok(PublicationPosition::RecordPublished);
        }
        let before_record = self
            .prior
            .as_ref()
            .filter(|_| !moved)
            .map(|prior| canonical(&prior.record));
        if candidate_record == before_record.as_deref() {
            return Ok(PublicationPosition::BeforeRecord);
        }
        Err(Refusal::RecordDigestMismatch)
    }
}

/// Stable object identity, observed through the platform's opened directory
/// handle. The adapter must also recheck current owner/privacy and redirects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "platform", rename_all = "snake_case", deny_unknown_fields)]
pub enum RootIdentity {
    Windows { volume_serial: u64, file_id: String },
    Unix { device: u64, inode: u64 },
}

impl RootIdentity {
    fn validate(&self) -> Result<(), Refusal> {
        let valid = match self {
            Self::Windows { file_id, .. } => {
                file_id.len() == 32
                    && file_id
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    && file_id.bytes().any(|byte| byte != b'0')
            }
            Self::Unix { inode, .. } => *inode != 0,
        };
        if valid {
            Ok(())
        } else {
            Err(Refusal::Malformed)
        }
    }
}

/// A missing root stays missing; a newly created folder is never adopted
/// through an earlier absence observation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum RootObservation {
    Absent,
    Present { identity: RootIdentity },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedRoots {
    pub data: RootObservation,
    pub state: RootObservation,
    pub cache: RootObservation,
    pub logs: RootObservation,
}

impl RetainedRoots {
    fn validate(&self, windows: bool) -> Result<(), Refusal> {
        let mut identities = Vec::new();
        for root in [&self.data, &self.state, &self.cache, &self.logs] {
            if let RootObservation::Present { identity } = root {
                identity.validate()?;
                if matches!(identity, RootIdentity::Windows { .. }) != windows {
                    return Err(Refusal::Malformed);
                }
                if identities.contains(&identity) {
                    return Err(Refusal::Malformed);
                }
                identities.push(identity);
            }
        }
        Ok(())
    }
}

/// Private ownership evidence retained before pointer/record removal. It is
/// usable only with the matching successful closed keep-data removal receipt
/// and the adapter's live private-root identity checks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedInstallation {
    pub schema: String,
    pub operation_sha256: Sha256Digest,
    pub operation: SetupOperation,
    pub installation: InstallationImage,
    pub roots: RetainedRoots,
}

impl RetainedInstallation {
    pub fn decode(bytes: &[u8]) -> Result<Self, Refusal> {
        let retained: Self = decode(bytes, MAX_RECOVERY_BYTES, RETAINED_SCHEMA, |r: &Self| {
            &r.schema
        })?;
        retained.validate()?;
        Ok(retained)
    }

    pub fn validate(&self) -> Result<(), Refusal> {
        if self.schema != RETAINED_SCHEMA {
            return Err(Refusal::UnknownSchema);
        }
        SetupOperation::decode(&canonical(&self.operation), self.operation.created_unix_ms)?;
        self.installation.validate()?;
        self.roots.validate(matches!(
            self.installation.record.signing,
            super::Signing::Windows { .. }
        ))?;
        if self.operation.kind != OperationKind::Uninstall
            || self.operation.options.delete_indexes == IndexDeletion::Purge
            || self.operation.locations != self.installation.record.locations
            || self.operation_sha256 != sha256(&canonical(&self.operation))
        {
            return Err(Refusal::Malformed);
        }
        Ok(())
    }

    /// A prepared or failed removal is not proof that the installation retired.
    /// A keep-data receipt never authorizes following a different current root.
    pub fn validate_completion(&self, receipt: &SetupReceipt) -> Result<(), Refusal> {
        self.validate()?;
        SetupReceipt::decode(&canonical(receipt))?;
        if receipt.schema != SETUP_RECEIPT_SCHEMA
            || receipt.operation_id != self.operation.operation_id
            || receipt.operation_sha256 != self.operation_sha256
            || receipt.kind != OperationKind::Uninstall
            || receipt.expires_unix_ms != self.operation.expires_unix_ms
            || !receipt.closed
            || receipt.failure.is_some()
            || receipt
                .steps
                .iter()
                .any(|step| step.phase == SetupPhase::DataPurged)
        {
            return Err(Refusal::Malformed);
        }
        let removed = |phase| {
            receipt.steps.iter().position(|step| {
                step.phase == phase && matches!(step.outcome.as_str(), "removed" | "absent")
            })
        };
        let pointer = removed(SetupPhase::PointerRemoved).ok_or(Refusal::Malformed)?;
        let record = removed(SetupPhase::RecordRemoved).ok_or(Refusal::Malformed)?;
        let complete = receipt
            .steps
            .iter()
            .position(|step| step.phase == SetupPhase::Complete && step.outcome == "closed")
            .ok_or(Refusal::Malformed)?;
        if pointer >= record || record >= complete || complete + 1 != receipt.steps.len() {
            return Err(Refusal::Malformed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
