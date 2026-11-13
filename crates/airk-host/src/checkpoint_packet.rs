// SPDX-License-Identifier: Apache-2.0
//! The native upgrade checkpoint packet and fence files: the persisted format,
//! its bounded reader and digest, the immutable writer, and the installer's
//! fence writer (`arm_recovery`) and remover (`settle`). Moved from
//! `service_runtime::checkpoint` and `source_supervisor::checkpoint` (split
//! step 2b) so the installer reads them without the service runtime.
use crate::checkpoint_fence::{FENCE, Fence, FenceOrigin, bytes, fence};
use crate::source_protocol as protocol;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::ProjectBinding,
};
use airk_store::service::registry::checkpoint::CheckpointSnapshot;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Write, path::Path};

pub const MAX_PACKET: u64 = 512 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Packet {
    pub schema: u32,
    pub captured_unix_ms: u64,
    pub operation_id: Incarnation,
    pub service_instance: Incarnation,
    pub execution_sha256: Sha256Digest,
    pub candidate_execution_sha256: Sha256Digest,
    pub state: CheckpointSnapshot,
    pub workers: Vec<crate::project_protocol::WorkerCheckpointObservation>,
    pub source: SourceCheckpointObservation,
    pub provider: airk_core::inference_provider::CompletionObservation,
}

pub fn packet_name(operation: Incarnation) -> String {
    format!("native-checkpoint-{operation}.json")
}
pub fn packet_digest(control: &Path, operation: Incarnation) -> Result<Sha256Digest> {
    Ok(Sha256Digest::from_bytes(
        Sha256::digest(bytes(&control.join(packet_name(operation)), MAX_PACKET)?).into(),
    ))
}

pub fn immutable(control: &Path, name: &str, data: &[u8]) -> Result<()> {
    ensure!(
        data.len() as u64 <= MAX_PACKET,
        "complete checkpoint packet exceeds capacity"
    );
    let path = control.join(name);
    if path.try_exists()? {
        ensure!(
            bytes(&path, MAX_PACKET)? == data,
            "immutable checkpoint packet changed"
        );
        return Ok(());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(control)?;
    temporary.write_all(data)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(&path)?;
    #[cfg(unix)]
    std::fs::File::open(control)?.sync_all()?;
    Ok(())
}

/// The installer clears only the exact operation after verified commit/rollback.
pub fn settle(control: &Path, operation: Incarnation) -> Result<()> {
    let Some(saved) = fence(control)? else {
        return Ok(());
    };
    ensure!(
        saved.operation_id == operation,
        "another checkpoint owns admission"
    );
    crate::install_files::safe_file(&control.join(FENCE))?;
    std::fs::remove_file(control.join(FENCE))?;
    #[cfg(unix)]
    std::fs::File::open(control)?.sync_all()?;
    Ok(())
}

/// Called only under installer-owned native exclusion, after committed-state
/// capture. An installer fence invents no historical service incarnation.
pub fn arm_recovery(control: &Path, operation: Incarnation, candidate: Sha256Digest) -> Result<()> {
    if let Some(saved) = fence(control)? {
        ensure!(
            saved.operation_id == operation && saved.candidate_execution_sha256 == candidate,
            "another upgrade owns recovery admission"
        );
        return Ok(());
    }
    let saved = Fence {
        schema: 1,
        control: control.canonicalize()?,
        operation_id: operation,
        service_instance: None,
        origin: FenceOrigin::InstallerAfterVerifiedExit,
        execution_sha256: crate::runtime_pair::current_executable_sha256()?.parse()?,
        candidate_execution_sha256: candidate,
    };
    immutable(control, FENCE, &serde_json::to_vec(&saved)?)
}

pub fn read_packet(control: &Path, operation: Incarnation, sha256: Sha256Digest) -> Result<Packet> {
    let path = control.join(packet_name(operation));
    let data = bytes(&path, MAX_PACKET)?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&data).into()) == sha256,
        "native checkpoint packet digest changed"
    );
    let packet: Packet = serde_json::from_slice(&data)?;
    packet.provider.validate()?;
    packet.source.validate()?;
    ensure!(
        packet.schema == 1
            && packet.operation_id == operation
            && packet.provider.operation_id == operation
            && packet.source.operation_id == operation
            && packet.source.service == packet.service_instance
            && packet.provider.provider.service_instance == packet.service_instance
            && packet.provider.provider.approved_execution_sha256 == packet.execution_sha256
            && packet.provider.state == airk_core::inference_provider::CompletionState::Complete
            && packet
                .workers
                .iter()
                .all(|worker| worker.operation_id == operation
                    && worker.identity.service == packet.service_instance
                    && worker.sealed
                    && worker.durable_boundary),
        "native checkpoint packet is incomplete or crossed"
    );
    ensure!(
        packet.provider.captured_workers
            == packet
                .workers
                .iter()
                .map(|worker| worker.identity)
                .collect::<Vec<_>>(),
        "native checkpoint packet worker roster changed"
    );
    packet.state.digest()?;
    Ok(packet)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileReference {
    pub project: ProjectBinding,
    pub configuration_epoch: u64,
    pub requested_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitBoundary {
    GracefulChildExit,
    NoActiveWatcherAfterNativeExclusion,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCheckpointObservation {
    pub schema: u32,
    pub operation_id: Incarnation,
    pub service: Incarnation,
    pub worker: Option<Incarnation>,
    pub process_id: Option<u32>,
    pub exit_boundary: ExitBoundary,
    pub source_lease_excluded: bool,
    pub targets: Vec<protocol::CheckpointTarget>,
    pub retained_reconciliations: Vec<ReconcileReference>,
}
impl SourceCheckpointObservation {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.source_lease_excluded
                && self.targets.len() <= 1024
                && self.retained_reconciliations.len() <= 1024,
            "source checkpoint barrier incomplete or oversized"
        );
        ensure!(
            match self.exit_boundary {
                ExitBoundary::GracefulChildExit =>
                    self.worker.is_some() && self.process_id.is_some_and(|pid| pid > 0),
                ExitBoundary::NoActiveWatcherAfterNativeExclusion =>
                    self.worker.is_none() && self.process_id.is_none() && self.targets.is_empty(),
            },
            "source checkpoint exit identity changed"
        );
        let mut projects = BTreeSet::new();
        for target in &self.targets {
            ensure!(
                target.sequence > 0
                    && target.sequence <= i64::MAX as u64
                    && projects.insert(target.project),
                "source checkpoint target duplicated or invalid"
            );
        }
        projects.clear();
        for target in &self.retained_reconciliations {
            ensure!(
                projects.insert(target.project),
                "source checkpoint reconciliation duplicated"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 256 * 1024,
            "source checkpoint receipt capacity exceeded"
        );
        Ok(())
    }
}
