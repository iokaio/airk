// SPDX-License-Identifier: Apache-2.0
//! Fixed native final-publication capacity; no document/evidence access.
use super::*;
use crate::service::registry::catalog::setup::storage::{
    SetupDirectory, SetupFileMode, file_identity,
};
use std::{
    ffi::OsStr,
    io::{Seek, SeekFrom},
};
const CAPSULE_BYTES: u64 = airk_core::catalog_management::MAX_PAGE_BYTES as u64;
const OWNER_BYTES: u64 = MAX_METADATA as u64;
const INTENT_BYTES: u64 = 16384;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalSlot {
    pub schema: u32,
    pub operation: Incarnation,
    pub request: Sha256Digest,
    pub index: airk_core::index_catalog::IndexId,
    pub registration: ProjectRegistration,
    pub directory: (u64, u64),
    pub capsule_file: (u64, u64),
    pub owner_file: (u64, u64),
}
impl TerminalSlot {
    pub(crate) fn identity(&self) -> Result<Sha256Digest> {
        ensure!(self.schema == 1, "unsupported split terminal allocation");
        Ok(framed_digest(
            b"airk/split-terminal-allocation/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
    fn names(&self) -> (String, String, String) {
        names(self.operation)
    }
    pub(crate) fn validate_for(&self, state: &NativeSplitState) -> Result<()> {
        let value = &state.0;
        value.validate()?;
        ensure!(
            self.operation == value.receipt.operation_id
                && self.request == value.receipt.request_digest
                && ((self.index == value.request.original_index
                    && self.registration == value.request.original_registration)
                    || (self.index == value.request.destination.index_id
                        && self.registration == value.request.destination_registration)),
            "split terminal slot request/owner differs"
        );
        Ok(())
    }
}
fn names(operation: Incarnation) -> (String, String, String) {
    (
        format!(".split-final-{operation}-capsule.part"),
        format!(".split-final-{operation}-owner.part"),
        format!(".split-final-{operation}.json"),
    )
}
fn absent(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}
fn open_initial(directory: &SetupDirectory, name: &str) -> Result<File> {
    match directory.open_file(OsStr::new(name), SetupFileMode::ReadWrite) {
        Ok(file) => {
            ensure!(
                file.metadata()?.len() == 0,
                "unrecorded nonempty terminal allocation is preserved"
            );
            Ok(file)
        }
        Err(error) if absent(&error) => {
            directory.open_file(OsStr::new(name), SetupFileMode::CreateNew)
        }
        Err(error) => Err(error),
    }
}
fn allocate(file: &mut File, bytes: u64) -> Result<()> {
    ensure!(
        file.metadata()?.len() <= bytes,
        "split terminal allocation length exceeded"
    );
    file.seek(SeekFrom::Start(0))?;
    let zero = [0u8; 8192];
    let mut left = bytes;
    while left > 0 {
        let count = left.min(zero.len() as u64) as usize;
        file.write_all(&zero[..count])?;
        left -= count as u64;
    }
    file.set_len(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn fill(file: &mut File, bytes: &[u8], limit: u64) -> Result<()> {
    ensure!(
        bytes.len() as u64 <= limit && file.metadata()?.len() >= bytes.len() as u64,
        "terminal publication would require unreserved growth"
    );
    file.seek(SeekFrom::Start(0))?;
    file.write_all(bytes)?;
    file.set_len(bytes.len() as u64)?;
    file.sync_all()?;
    Ok(())
}
impl Collection {
    pub(crate) fn prepare_split_terminal_slot(
        &self,
        state: &NativeSplitState,
        expected: &crate::quota::funding::FundingReceipt,
    ) -> Result<(TerminalSlot, crate::quota::funding::FundingReceipt)> {
        let value = &state.0;
        value.validate()?;
        expected.validate_for(&value.request)?;
        ensure!(
            value.receipt.physical_cleanup_authorization.is_some()
                && (value.receipt.stage == SplitStage::CleanupPending
                    || matches!(
                        value.receipt.state,
                        SplitRunState::CanceledBeforeFence {
                            cleanup_pending: true
                        }
                    )),
            "terminal allocation precedes native physical cleanup"
        );
        let registration = owner_registration(&self.owner)?;
        let index = if registration == value.request.original_registration {
            value.request.original_index
        } else {
            ensure!(
                registration == value.request.destination_registration,
                "terminal allocation owner differs"
            );
            value.request.destination.index_id
        };
        ensure!(
            value.receipt.funding_receipts.get(&registration.data_root)
                == Some(&expected.identity()?)
                && expected.data_root == Path::new(&registration.data_root),
            "terminal allocation funding was not admitted"
        );
        let quota = Quota::open_service(&expected.data_root)?;
        quota.validate_funding(expected)?;
        let participant = expected
            .participants
            .iter()
            .find(|participant| participant.index_id == index)
            .context("terminal allocation participant missing")?;
        let phase = participant
            .phase
            .as_ref()
            .context("terminal allocation lacks admitted phase")?;
        let funding = quota.adopt_funding_phase(expected, index, phase)?;
        let quota = quota.with_funding(funding.clone())?;
        let mut store = self.select_for_diagnostics()?;
        let writer = store.lock()?;
        ensure!(
            self.select_for_diagnostics()?.directory() == store.directory(),
            "terminal allocation selected slot changed"
        );
        store.funding = Some(funding);
        let directory = SetupDirectory::open_canonical(&self.directory)?;
        let owner: Owner = serde_json::from_slice(
            &directory.read_file(OsStr::new("service-owner.json"), MAX_METADATA as u64)?,
        )?;
        let pointer = owner
            .required_split
            .as_ref()
            .context("terminal allocation lacks split owner")?;
        let previous = capsule(&self.directory, pointer)?.snapshot;
        ensure!(
            previous.request == value.request
                && previous.plan == value.plan
                && pointer.revision <= value.receipt.revision,
            "terminal allocation lifecycle differs"
        );
        let (capsule_name, owner_name, intent_name) = names(value.receipt.operation_id);
        let (intent, mut capsule_file, mut owner_file) =
            match directory.read_file(OsStr::new(&intent_name), INTENT_BYTES) {
                Ok(bytes) => {
                    let intent: TerminalSlot = serde_json::from_slice(&bytes)?;
                    intent.validate_for(state)?;
                    ensure!(
                        serde_json::to_vec(&intent)? == bytes
                            && intent.directory == directory.identity()?,
                        "terminal allocation record differs"
                    );
                    let capsule_file =
                        directory.open_file(OsStr::new(&capsule_name), SetupFileMode::ReadWrite)?;
                    let owner_file =
                        directory.open_file(OsStr::new(&owner_name), SetupFileMode::ReadWrite)?;
                    ensure!(
                        file_identity(&capsule_file)? == intent.capsule_file
                            && file_identity(&owner_file)? == intent.owner_file,
                        "terminal allocated file identity changed"
                    );
                    (intent, capsule_file, owner_file)
                }
                Err(error) if absent(&error) => {
                    let capsule_file = open_initial(&directory, &capsule_name)?;
                    let owner_file = open_initial(&directory, &owner_name)?;
                    let intent = TerminalSlot {
                        schema: 1,
                        operation: value.receipt.operation_id,
                        request: value.receipt.request_digest,
                        index,
                        registration: registration.clone(),
                        directory: directory.identity()?,
                        capsule_file: file_identity(&capsule_file)?,
                        owner_file: file_identity(&owner_file)?,
                    };
                    (intent, capsule_file, owner_file)
                }
                Err(error) => return Err(error),
            };
        if capsule_file.metadata()?.len() == CAPSULE_BYTES
            && owner_file.metadata()?.len() == OWNER_BYTES
        {
            let receipt = store.reconcile_native_split_metadata(&writer, &quota)?;
            return Ok((intent, receipt));
        }
        let reservation = quota.reserve(
            &registration.collection,
            CAPSULE_BYTES + OWNER_BYTES + INTENT_BYTES,
        )?;
        let bytes = serde_json::to_vec(&intent)?;
        ensure!(
            bytes.len() as u64 <= INTENT_BYTES,
            "terminal allocation intent exceeds bound"
        );
        // The final name must never identify an interrupted JSON prefix.
        // Existing protected publication resumes only this exact native scratch
        // prefix, syncs complete bytes, then atomically renames without replacing
        // a foreign final intent. No preallocation grows before that publication.
        crate::service::registry::catalog::setup::storage::write_exact(
            &directory,
            &intent_name,
            &bytes,
        )?;
        allocate(&mut capsule_file, CAPSULE_BYTES)?;
        allocate(&mut owner_file, OWNER_BYTES)?;
        directory.sync()?;
        reservation.charge(CAPSULE_BYTES + OWNER_BYTES + bytes.len() as u64)?;
        drop(reservation);
        let receipt = store.reconcile_native_split_metadata(&writer, &quota)?;
        Ok((intent, receipt))
    }
    pub(crate) fn publish_split_terminal_slot(
        &self,
        state: &NativeSplitState,
        intent: &TerminalSlot,
    ) -> Result<NativeTerminalPublication> {
        intent.validate_for(state)?;
        let value = &state.0;
        ensure!(
            matches!(
                value.receipt.state,
                SplitRunState::Complete
                    | SplitRunState::CanceledBeforeFence {
                        cleanup_pending: false
                    }
            ),
            "terminal publication target is not terminal"
        );
        ensure!(
            value.receipt.terminal_publication_slots.get(&intent.index)
                == Some(&intent.identity()?),
            "terminal publication allocation differs"
        );
        let store = FileStore::new(&self.directory);
        let writer = store.existing_writer_exclusion()?;
        let directory = SetupDirectory::open_canonical(&self.directory)?;
        ensure!(
            directory.identity()? == intent.directory,
            "terminal publication directory changed"
        );
        let mut old_owner_file =
            directory.open_file(OsStr::new("service-owner.json"), SetupFileMode::Read)?;
        let mut old_bytes = Vec::new();
        (&mut old_owner_file)
            .take(MAX_METADATA as u64 + 1)
            .read_to_end(&mut old_bytes)?;
        ensure!(
            old_bytes.len() <= MAX_METADATA,
            "terminal previous owner exceeds bound"
        );
        let mut owner: Owner = serde_json::from_slice(&old_bytes)?;
        ensure!(
            owner_registration(&owner)? == intent.registration
                && owner_digest(&owner)? == self.binding,
            "terminal publication previous owner differs"
        );
        let old = owner
            .required_split
            .as_ref()
            .context("terminal publication previous split owner missing")?;
        let previous = capsule(&self.directory, old)?.snapshot;
        ensure!(
            previous.request == value.request && previous.plan == value.plan,
            "terminal publication previous request differs"
        );
        let capsule_bytes = serde_json::to_vec(&Capsule {
            schema: 1,
            snapshot: value.clone(),
        })?;
        ensure!(
            capsule_bytes.len() as u64 <= CAPSULE_BYTES,
            "terminal capsule exceeds prepared allocation"
        );
        let digest = framed_digest(b"airk/stable-split-state/1", &[&capsule_bytes]);
        let target = filename(digest);
        let (capsule_name, owner_name, intent_name) = intent.names();
        let capsule_file = match directory.open_file(OsStr::new(&target), SetupFileMode::Read) {
            Ok(file) => {
                ensure!(
                    file_identity(&file)? == intent.capsule_file
                        && directory.read_file(OsStr::new(&target), CAPSULE_BYTES)?
                            == capsule_bytes,
                    "terminal capsule target differs"
                );
                file
            }
            Err(error) if absent(&error) => {
                let mut file =
                    directory.open_file(OsStr::new(&capsule_name), SetupFileMode::ReadWrite)?;
                ensure!(
                    file_identity(&file)? == intent.capsule_file,
                    "terminal capsule allocation changed"
                );
                fill(&mut file, &capsule_bytes, CAPSULE_BYTES)?;
                directory.publish_no_replace(
                    OsStr::new(&capsule_name),
                    &file,
                    &directory,
                    OsStr::new(&target),
                )?;
                file
            }
            Err(error) => return Err(error),
        };
        let role = if intent.index == value.request.original_index {
            SplitOwnerRole::OriginalRemainder
        } else {
            SplitOwnerRole::Destination
        };
        let reference = SplitOwnerReference {
            required_contract: airk_core::catalog_split::SPLIT_CONTRACT,
            operation_id: intent.operation,
            request_digest: intent.request,
            index_id: intent.index,
            role,
            activation_receipt: value.receipt.membership_receipt,
        };
        reference.validate_for(&value.request, &value.receipt)?;
        owner.required_split = Some(SplitOwnerPointer {
            reference,
            revision: value.receipt.revision,
            digest,
        });
        let owner_bytes = serde_json::to_vec(&owner)?;
        ensure!(
            owner_bytes.len() <= MAX_METADATA,
            "terminal owner exceeds prepared allocation"
        );
        let owner_file = if old_bytes == owner_bytes {
            ensure!(
                file_identity(&old_owner_file)? == intent.owner_file,
                "terminal owner target allocation differs"
            );
            old_owner_file
        } else {
            let mut file =
                directory.open_file(OsStr::new(&owner_name), SetupFileMode::ReadWrite)?;
            ensure!(
                file_identity(&file)? == intent.owner_file,
                "terminal owner allocation changed"
            );
            fill(&mut file, &owner_bytes, OWNER_BYTES)?;
            directory.replace_file(
                OsStr::new(&owner_name),
                &file,
                OsStr::new("service-owner.json"),
                &old_owner_file,
            )?;
            file
        };
        match directory.open_file(OsStr::new(&intent_name), SetupFileMode::Read) {
            Ok(file) => directory.unlink_file_if_identity(OsStr::new(&intent_name), &file)?,
            Err(error) if absent(&error) => (),
            Err(error) => return Err(error),
        };
        directory.sync()?;
        Ok(NativeTerminalPublication {
            intent: intent.clone(),
            receipt: value.receipt_digest,
            directory,
            capsule_file,
            owner_file,
            _writer: writer,
            capsule_name: target,
            capsule_bytes,
            owner_bytes,
        })
    }
}
pub(crate) struct NativeTerminalPublication {
    pub intent: TerminalSlot,
    pub receipt: Sha256Digest,
    directory: SetupDirectory,
    capsule_file: File,
    owner_file: File,
    _writer: crate::ExistingWriterExclusion,
    capsule_name: String,
    capsule_bytes: Vec<u8>,
    owner_bytes: Vec<u8>,
}
impl NativeTerminalPublication {
    pub(crate) fn revalidate(&self) -> Result<()> {
        ensure!(
            self.directory.identity()? == self.intent.directory
                && file_identity(&self.capsule_file)? == self.intent.capsule_file
                && file_identity(&self.owner_file)? == self.intent.owner_file,
            "terminal publication physical identity differs"
        );
        ensure!(
            self.directory
                .read_file(OsStr::new(&self.capsule_name), CAPSULE_BYTES)?
                == self.capsule_bytes
                && self
                    .directory
                    .read_file(OsStr::new("service-owner.json"), MAX_METADATA as u64)?
                    == self.owner_bytes,
            "terminal publication bytes changed"
        );
        Ok(())
    }
}
