// SPDX-License-Identifier: Apache-2.0
//! Read side of the native upgrade admission fence. Leaf: callers that only
//! ask whether admission is closed, or by which operation, need not depend on
//! the service runtime. The writer, recovery and `settle` stay in
//! `service_runtime::checkpoint`. See `CONTRACT.md` beside this file.
use airk_core::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub(crate) const FENCE: &str = "native-upgrade-checkpoint.json";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fence {
    pub(crate) schema: u32,
    pub(crate) control: PathBuf,
    pub(crate) operation_id: Incarnation,
    pub(crate) service_instance: Option<Incarnation>,
    pub(crate) origin: FenceOrigin,
    pub(crate) execution_sha256: Sha256Digest,
    pub(crate) candidate_execution_sha256: Sha256Digest,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FenceOrigin {
    RunningService,
    InstallerAfterVerifiedExit,
}

pub(crate) fn bytes(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    crate::install_files::safe_file(path)?;
    ensure!(
        std::fs::metadata(path)?.len() <= maximum,
        "checkpoint metadata capacity exceeded"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "checkpoint metadata grew beyond capacity"
    );
    Ok(bytes)
}

pub(crate) fn fence(control: &Path) -> Result<Option<Fence>> {
    let path = control.join(FENCE);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let value: Fence = serde_json::from_slice(&bytes(&path, 4096)?)?;
    ensure!(
        value.schema == 1 && value.control == control.canonicalize()?,
        "checkpoint fence control ownership changed"
    );
    ensure!(
        (value.origin == FenceOrigin::RunningService) == value.service_instance.is_some(),
        "checkpoint fence origin changed"
    );
    Ok(Some(value))
}

pub(crate) fn admission_closed(control: &Path) -> Result<bool> {
    Ok(fence(control)?.is_some())
}

/// The operation that owns a closed admission fence, read without changing it.
/// Settlement still goes through `settle`, which refuses any other operation.
pub(crate) fn fence_operation(control: &Path) -> Result<Option<Incarnation>> {
    Ok(fence(control)?.map(|saved| saved.operation_id))
}
