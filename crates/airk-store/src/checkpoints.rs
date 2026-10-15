// SPDX-License-Identifier: Apache-2.0
//! Named pins of already committed evidence; labels are data, never paths.
use airk_core::identity::{PublicationReceipt, RevisionToken, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_CHECKPOINTS: usize = 4;
pub const MAX_CHECKPOINT_LABEL_BYTES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub version: u32,
    pub label: String,
    pub publication: PublicationReceipt,
    pub sequence: u64,
    pub admitted_dependency_bytes: u64,
    pub dependency_digest: Sha256Digest,
}
impl Checkpoint {
    pub fn validate(&self) -> Result<()> {
        validate_label(&self.label)?;
        ensure!(
            self.version == 1 && self.sequence > 0,
            "invalid checkpoint version or sequence"
        );
        Ok(())
    }
}
pub fn validate_label(label: &str) -> Result<()> {
    ensure!(
        !label.is_empty()
            && label.len() <= MAX_CHECKPOINT_LABEL_BYTES
            && !label.chars().any(char::is_control),
        "checkpoint label must contain 1..=128 UTF-8 bytes without controls"
    );
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCheckpoint {
    pub label: String,
    pub revision: RevisionToken,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveCheckpoint {
    pub label: String,
    pub expected_revision: RevisionToken,
}
#[derive(Clone, Debug, Serialize)]
pub struct CheckpointMutationReceipt {
    pub mutation: crate::history_policy::OperatorReceipt,
    pub checkpoint: Checkpoint,
    pub removed: bool,
}

pub(crate) fn protected(store: &crate::FileStore, token: &RevisionToken) -> Result<bool> {
    Ok(crate::sqlite::history_admin(store.directory())?
        .checkpoints
        .values()
        .any(|pin| pin.publication.token == *token))
}

fn metadata_bytes(descriptor: &crate::history::HistoryDescriptor) -> Result<u64> {
    descriptor.metadata.values().try_fold(
        descriptor.canonical_bytes()?.len() as u64,
        |total, item| {
            total
                .checked_add(item.bytes)
                .ok_or_else(|| anyhow::anyhow!("checkpoint metadata overflow"))
        },
    )
}

pub(crate) fn footprint(descriptors: &[crate::history::HistoryDescriptor]) -> Result<u64> {
    let mut revisions = std::collections::BTreeSet::new();
    let mut dependencies = std::collections::BTreeMap::new();
    let mut bytes = 0u64;
    for descriptor in descriptors {
        if !revisions.insert(descriptor.publication.token.revision) {
            continue;
        }
        bytes = bytes
            .checked_add(metadata_bytes(descriptor)?)
            .ok_or_else(|| anyhow::anyhow!("checkpoint footprint overflow"))?;
        for dependency in &descriptor.dependencies {
            let key = (dependency.kind, dependency.digest);
            if let Some(old) = dependencies.insert(key, dependency.bytes) {
                ensure!(
                    old == dependency.bytes,
                    "inconsistent checkpoint dependency length"
                );
            }
        }
    }
    for size in dependencies.values() {
        bytes = bytes
            .checked_add(*size)
            .ok_or_else(|| anyhow::anyhow!("checkpoint dependency total overflow"))?;
    }
    Ok(bytes)
}

impl crate::FileStore {
    pub fn checkpoints(&self) -> Result<Vec<Checkpoint>> {
        let _lifecycle = self.lifecycle(false)?;
        Ok(crate::sqlite::history_admin(self.directory())?
            .checkpoints
            .into_values()
            .collect())
    }

    pub fn create_checkpoint(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        request: &CreateCheckpoint,
        budget: crate::history_policy::HistoryWorkBudget,
    ) -> Result<CheckpointMutationReceipt> {
        use crate::history_policy::{OperatorAction, digest};
        use anyhow::Context;
        crate::maintenance::check_writer(self, writer, quota)?;
        validate_label(&request.label)?;
        let mut work = crate::maintenance::WorkBudget::history(budget)?;
        let current = self
            .catalog()?
            .context("checkpoint requires indexed collection")?;
        let head = current
            .receipt
            .as_ref()
            .context("checkpoint requires v2 migration")?
            .token;
        request.revision.validate_against(&head)?;
        crate::sqlite::validate_retained_token(self.directory(), &request.revision)?;
        let expected = crate::sqlite::history_admin(self.directory())?;
        ensure!(
            !expected.checkpoints.contains_key(&request.label),
            "checkpoint label already exists; remove it explicitly before reuse"
        );
        ensure!(
            expected.checkpoints.len() < MAX_CHECKPOINTS,
            "checkpoint count limit reached"
        );
        ensure!(
            !crate::maintenance::retirement_pending(self, &request.revision)?
                && !crate::history::removal_intents(self)?
                    .iter()
                    .any(|intent| intent.binding.publication.token == request.revision),
            "checkpoint target is being retired"
        );
        let lease = std::fs::File::open(crate::blobs::reader_lease_path(
            self.directory(),
            &request.revision.revision.to_string(),
        )?)?;
        lease.lock_shared()?;
        let target = crate::history::describe_revision(self, &request.revision, &mut work)?;
        let mut descriptors = vec![target.clone()];
        for checkpoint in expected.checkpoints.values() {
            descriptors.push(crate::history::describe_revision(
                self,
                &checkpoint.publication.token,
                &mut work,
            )?);
        }
        let status = quota.status()?;
        let allocation = status
            .collections
            .get(&current.snapshot.collection)
            .context("collection quota accounting missing")?;
        let projected = footprint(&descriptors)?;
        let cap = expected.policy.policy.byte_limit(allocation.limit_bytes);
        ensure!(
            projected <= cap,
            "checkpoint eventual history footprint exceeds history byte admission limit"
        );
        let distinct = descriptors
            .iter()
            .map(|descriptor| descriptor.publication.token.revision)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        ensure!(
            distinct <= usize::from(expected.policy.policy.max_packages),
            "checkpoint packages exceed history count admission limit"
        );
        // Metadata copies must be affordable even when an active target is still full.
        let future_metadata = descriptors
            .iter()
            .filter(|descriptor| {
                self.directory()
                    .join("generations")
                    .join(descriptor.publication.token.revision.to_string())
                    .exists()
            })
            .try_fold(0u64, |sum, descriptor| {
                sum.checked_add(metadata_bytes(descriptor)?)
                    .context("checkpoint copy admission overflow")
            })?;
        let root_available = status
            .root_limit_bytes
            .saturating_sub(status.used_bytes)
            .saturating_sub(status.reserved_bytes);
        let collection_available = allocation
            .limit_bytes
            .saturating_sub(allocation.used_bytes)
            .saturating_sub(allocation.reserved_bytes);
        ensure!(
            future_metadata <= root_available.min(collection_available),
            "checkpoint future metadata copies exceed quota headroom"
        );
        let reservation = quota.reserve_available(
            &current.snapshot.collection,
            budget.maintenance.max_metadata_bytes,
        )?;
        let mut next = expected.clone();
        let mutation = next.record(head, OperatorAction::CreateCheckpoint, request)?;
        let checkpoint = Checkpoint {
            version: 1,
            label: request.label.clone(),
            publication: target.publication.clone(),
            sequence: mutation.sequence,
            admitted_dependency_bytes: footprint(std::slice::from_ref(&target))?,
            dependency_digest: digest(&target)?,
        };
        next.checkpoints
            .insert(request.label.clone(), checkpoint.clone());
        crate::sqlite::commit_history_admin(
            self.directory(),
            &expected,
            &next,
            &head,
            &reservation,
        )?;
        Ok(CheckpointMutationReceipt {
            mutation,
            checkpoint,
            removed: false,
        })
    }

    pub fn remove_checkpoint(
        &self,
        writer: &crate::WriterLock,
        quota: &crate::quota::Quota,
        request: &RemoveCheckpoint,
        budget: crate::history_policy::HistoryWorkBudget,
    ) -> Result<CheckpointMutationReceipt> {
        use anyhow::Context;
        crate::maintenance::check_writer(self, writer, quota)?;
        budget.validate()?;
        validate_label(&request.label)?;
        let current = self
            .catalog()?
            .context("checkpoint requires indexed collection")?;
        let head = current
            .receipt
            .as_ref()
            .context("checkpoint requires v2 migration")?
            .token;
        let expected = crate::sqlite::history_admin(self.directory())?;
        let checkpoint = expected
            .checkpoints
            .get(&request.label)
            .context("checkpoint label not found")?
            .clone();
        ensure!(
            checkpoint.publication.token == request.expected_revision,
            "checkpoint revision changed"
        );
        let mut next = expected.clone();
        next.checkpoints.remove(&request.label);
        let mutation = next.record(
            head,
            crate::history_policy::OperatorAction::RemoveCheckpoint,
            request,
        )?;
        let reservation = quota.reserve_available(
            &current.snapshot.collection,
            budget.maintenance.max_metadata_bytes,
        )?;
        crate::sqlite::commit_history_admin(
            self.directory(),
            &expected,
            &next,
            &head,
            &reservation,
        )?;
        Ok(CheckpointMutationReceipt {
            mutation,
            checkpoint,
            removed: true,
        })
    }
}
