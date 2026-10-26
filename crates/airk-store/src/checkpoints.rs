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
