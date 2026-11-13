// SPDX-License-Identifier: Apache-2.0
//! Native cumulative completion authority. Disk observations are audit evidence,
//! never constructors for a native witness. All IO runs on the owner actor.
use crate::inference_engine::NativeCompletionWitness;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    inference_provider::{
        CompletionBasis, CompletionLedgerObservation, CompletionObservation, CompletionState,
        ProviderIdentity,
    },
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_RECORD: usize = airk_core::inference_provider::MAX_COMPLETION_BYTES + 4096;
const VERIFY_RECORDS: u64 = 128;
const VERIFY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    namespace: Incarnation,
    factory: ProviderIdentity,
    sequence: u64,
    previous: Option<Sha256Digest>,
    completion: CompletionObservation,
}

/// Only a completed native owner can issue this witness. Keeping the actual
/// service lease also prevents an excluded owner from being mistaken for live.
#[derive(Clone)]
pub struct NativeLedgerWitness {
    observation: CompletionLedgerObservation,
    _ownership: Arc<airk_store::service::ServiceLease>,
}
impl NativeLedgerWitness {
    pub fn observation(&self) -> &CompletionLedgerObservation {
        &self.observation
    }
    pub fn matches(&self, namespace: Incarnation, service: Incarnation) -> bool {
        self.observation.namespace == namespace && self.observation.service_instance == service
    }
}

struct Verification {
    sequence: u64,
    expected: Sha256Digest,
    admitted: u64,
}
pub(crate) struct NativeLedger {
    ownership: Arc<airk_store::service::ServiceLease>,
    namespace: Incarnation,
    factory: ProviderIdentity,
    directory: PathBuf,
    head: Option<CompletionLedgerObservation>,
    verification: Option<Verification>,
    verified: bool,
}

fn hash(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn read(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    airk_host::install_files::safe_file(path)?;
    ensure!(
        fs::metadata(path)?.len() <= maximum as u64,
        "native completion evidence exceeds capacity"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= maximum,
        "native completion evidence grew beyond capacity"
    );
    Ok(bytes)
}
fn sync(directory: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(directory)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}
fn immutable(directory: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    if path.try_exists()? {
        ensure!(
            read(path, MAX_RECORD)? == bytes,
            "immutable native completion evidence changed"
        );
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?
            .sync_all()?;
    } else {
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(path)?;
    }
    sync(directory)
}
impl NativeLedger {
    pub(crate) fn new(
        ownership: Arc<airk_store::service::ServiceLease>,
        factory: ProviderIdentity,
    ) -> Result<Self> {
        factory.validate()?;
        let namespace = airk_store::service::registry::retained_namespace(ownership.root())?;
        let parent = ownership.root().join("provider-completions");
        crate::local_ipc::prepare_root(&parent)?;
        let directory = parent.join(format!(
            "{}-{}",
            factory.service_instance, factory.provider_generation
        ));
        ensure!(
            !directory.try_exists()?,
            "native factory completion journal already exists"
        );
        crate::local_ipc::prepare_root(&directory)?;
        sync(&parent)?;
        Ok(Self {
            ownership,
            namespace,
            factory,
            directory,
            head: None,
            verification: None,
            verified: false,
        })
    }
    pub(crate) fn observation(&self) -> Option<&CompletionLedgerObservation> {
        self.head.as_ref()
    }
    pub(crate) fn append(
        &mut self,
        witness: &NativeCompletionWitness,
    ) -> Result<NativeLedgerWitness> {
        let completion = witness.observation();
        completion.validate()?;
        ensure!(
            witness.namespace() == self.namespace
                && witness.matches(
                    self.factory.service_instance,
                    completion.provider.provider_generation
                )
                && completion.provider.approved_execution_sha256
                    == self.factory.approved_execution_sha256
                && completion.provider.provider_generation != self.factory.provider_generation
                && completion.completion_basis == CompletionBasis::NativeEngineBarrier
                && completion.state == CompletionState::Complete,
            "crossed native completion journal witness"
        );
        crate::local_ipc::require_existing_root(&self.directory)?;
        ensure!(
            airk_store::service::registry::retained_namespace(self.ownership.root())?
                == self.namespace,
            "native completion namespace changed"
        );
        let sequence = self.head.as_ref().map_or(Ok(1), |head| {
            head.completed_owners
                .checked_add(1)
                .context("native completion sequence overflow")
        })?;
        let admitted = self
            .head
            .as_ref()
            .map_or(0, |head| head.admitted_high_water)
            .checked_add(completion.admitted_high_water)
            .context("native completion work count overflow")?;
        let record = Record {
            schema: 1,
            namespace: self.namespace,
            factory: self.factory,
            sequence,
            previous: self.head.as_ref().map(|head| head.head_sha256),
            completion: completion.clone(),
        };
        let bytes = serde_json::to_vec(&record)?;
        ensure!(
            bytes.len() <= MAX_RECORD,
            "native completion record exceeds capacity"
        );
        let path = self.directory.join(format!("{sequence:020}.json"));
        let digest = hash(&bytes);
        // A completed generation can appear once only. This index is evidence,
        // not authority: verification checks it against the native hash chain.
        let index = self.directory.join(format!(
            "generation-{}.json",
            completion.provider.provider_generation
        ));
        let index_bytes = serde_json::to_vec(&(sequence, digest))?;
        let observation = CompletionLedgerObservation {
            schema: 1,
            namespace: self.namespace,
            service_instance: self.factory.service_instance,
            approved_execution_sha256: self.factory.approved_execution_sha256,
            factory_generation: self.factory.provider_generation,
            completed_owners: sequence,
            admitted_high_water: admitted,
            head_sha256: digest,
        };
        observation.validate()?;
        let desired_head = serde_json::to_vec(&observation)?;
        let head_path = self.directory.join("head.json");
        if head_path.try_exists()? {
            let existing = read(&head_path, 2048)?;
            let prior = self.head.as_ref().map(serde_json::to_vec).transpose()?;
            ensure!(
                prior.as_deref() == Some(existing.as_slice()) || existing == desired_head,
                "native completion head changed before append"
            );
        } else {
            ensure!(self.head.is_none(), "native completion head was removed");
        }
        immutable(&self.directory, &path, &bytes)?;
        immutable(&self.directory, &index, &index_bytes)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        temporary.write_all(&desired_head)?;
        temporary.as_file().sync_all()?;
        if head_path.try_exists()? {
            airk_host::install_files::safe_file(&head_path)?;
        }
        temporary.persist(&head_path)?;
        sync(&self.directory)?;
        self.head = Some(observation.clone());
        self.verification = None;
        self.verified = false;
        Ok(NativeLedgerWitness {
            observation,
            _ownership: self.ownership.clone(),
        })
    }
    /// Verify the immutable chain in bounded actor turns, never on a query.
    /// A large history makes progress without growing a checkpoint response.
    pub(crate) fn verify(&mut self) -> Result<bool> {
        let head = self
            .head
            .as_ref()
            .context("native completion journal is empty")?;
        ensure!(
            airk_store::service::registry::retained_namespace(self.ownership.root())?
                == self.namespace,
            "native completion namespace changed"
        );
        crate::local_ipc::require_existing_root(&self.directory)?;
        ensure!(
            read(&self.directory.join("head.json"), 2048)? == serde_json::to_vec(head)?,
            "native completion journal head changed"
        );
        if self.verified {
            return Ok(true);
        }
        let state = self.verification.get_or_insert(Verification {
            sequence: head.completed_owners,
            expected: head.head_sha256,
            admitted: 0,
        });
        let mut bytes_used = 0usize;
        for _ in 0..VERIFY_RECORDS {
            if VERIFY_BYTES.saturating_sub(bytes_used) < MAX_RECORD {
                break;
            }
            let bytes = read(
                &self.directory.join(format!("{:020}.json", state.sequence)),
                MAX_RECORD,
            )?;
            bytes_used += bytes.len();
            ensure!(
                hash(&bytes) == state.expected,
                "native completion record digest changed"
            );
            let record: Record = serde_json::from_slice(&bytes)?;
            record.completion.validate()?;
            ensure!(
                record.schema == 1
                    && record.namespace == self.namespace
                    && record.factory == self.factory
                    && record.sequence == state.sequence
                    && record.completion.completion_basis == CompletionBasis::NativeEngineBarrier
                    && record.completion.state == CompletionState::Complete
                    && record.completion.provider.service_instance == self.factory.service_instance
                    && record.completion.provider.approved_execution_sha256
                        == self.factory.approved_execution_sha256,
                "crossed native completion record"
            );
            let expected_index = serde_json::to_vec(&(record.sequence, state.expected))?;
            ensure!(
                read(
                    &self.directory.join(format!(
                        "generation-{}.json",
                        record.completion.provider.provider_generation
                    )),
                    128
                )? == expected_index,
                "native completion generation index changed"
            );
            let admitted = state
                .admitted
                .checked_add(record.completion.admitted_high_water)
                .context("native completion verification count overflow")?;
            if state.sequence == 1 {
                ensure!(
                    record.previous.is_none() && admitted == head.admitted_high_water,
                    "native completion chain totals changed"
                );
                self.verified = true;
                self.verification = None;
                return Ok(true);
            }
            state.expected = record
                .previous
                .context("native completion chain was truncated")?;
            state.sequence -= 1;
            state.admitted = admitted;
            if bytes_used >= VERIFY_BYTES {
                break;
            }
        }
        Ok(false)
    }
}
