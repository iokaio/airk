// SPDX-License-Identifier: Apache-2.0
//! Bounded worker observations for console display and maintenance guards.
use crate::{
    identity::{Incarnation, RevisionToken, Sha256Digest, framed_digest},
    readiness::{DiscoveryState, SemanticState},
    service::{ProjectBinding, WorkerIdentity},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_INSPECTION_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexState {
    Empty,
    Ready,
    RecoveryRequired,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Available,
    Unavailable,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionFailure {
    IndexObservationFailed,
    IndexRequiresRecovery,
    SettingsObservationFailed,
    RecoveryObservationFailed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSummary {
    pub state: SemanticState,
    pub selected_artifacts: u64,
    pub completed_artifacts: u64,
    pub selected_units: Option<u64>,
    pub completed_units: u64,
    pub failed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoverySummary {
    pub state: DiscoveryState,
    pub processed_artifacts: u64,
    pub committed_artifacts: u64,
    pub total_artifacts: Option<u64>,
    pub failed: bool,
}

/// Every field is content-free. Registration paths are a separate trusted UI input.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionSnapshot {
    pub project: ProjectBinding,
    pub incarnation: Incarnation,
    pub publication: Option<RevisionToken>,
    pub settings_digest: Option<Sha256Digest>,
    pub index_state: IndexState,
    pub source_state: SourceState,
    pub recovery_required: Option<bool>,
    pub indexed_files: Option<u64>,
    pub indexed_vectors: Option<u64>,
    pub vectors_enabled: Option<bool>,
    pub semantic: Option<SemanticSummary>,
    pub discovery: Option<DiscoverySummary>,
    pub failure: Option<InspectionFailure>,
}
impl InspectionSnapshot {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.publication
                .is_none_or(|token| token.incarnation == self.incarnation),
            "inspection publication differs from incarnation"
        );
        ensure!(
            match self.index_state {
                IndexState::Empty =>
                    self.publication.is_none()
                        && self.failure.is_none()
                        && self.settings_digest.is_some()
                        && self.recovery_required.is_some(),
                IndexState::Ready =>
                    self.publication.is_some()
                        && self.failure.is_none()
                        && self.settings_digest.is_some()
                        && self.recovery_required.is_some(),
                IndexState::RecoveryRequired => self.recovery_required == Some(true),
                IndexState::Unavailable => self.failure.is_some(),
            },
            "incoherent inspection state"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_INSPECTION_BYTES,
            "inspection exceeds byte bound"
        );
        Ok(())
    }
    /// Observation time/worker changes do not invalidate the inspected index state.
    /// Execution must recapture this guard; a cached observation is never authority.
    pub fn guard_digest(&self) -> Result<Sha256Digest> {
        self.validate()?;
        let bytes = serde_json::to_vec(&(
            self.project,
            self.incarnation,
            self.publication,
            self.settings_digest,
            self.index_state,
            self.recovery_required,
        ))?;
        Ok(framed_digest(b"airk-maintenance-inspection/1.4", &[&bytes]))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionObservation {
    pub worker: WorkerIdentity,
    pub current_fencing_generation: u64,
    pub job: Sha256Digest,
    pub observed_unix_ms: u64,
    pub snapshot: InspectionSnapshot,
}
