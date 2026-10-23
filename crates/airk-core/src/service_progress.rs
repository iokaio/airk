// SPDX-License-Identifier: Apache-2.0
//! Content-free work observations. Liveness is deliberately not work advancement.
use crate::{
    identity::{Incarnation, RevisionToken, Sha256Digest},
    service::{JobRecord, WorkerIdentity},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkPhase {
    Starting,
    Discovering,
    Extracting,
    Preparing,
    Publishing,
    Embedding,
    Verifying,
    Recovering,
    Retiring,
    Finished,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkProgress {
    pub job: Sha256Digest,
    pub grant: Incarnation,
    pub sequence: u64,
    pub phase: WorkPhase,
    pub paused: bool,
    pub stalled: bool,
    pub elapsed_ms: u64,
    pub phase_elapsed_ms: u64,
    pub phase_totals_ms: std::collections::BTreeMap<WorkPhase, u64>,
    pub last_advance_age_ms: Option<u64>,
    /// Time without actual work advancement, excluding explicit project pause.
    pub unpaused_idle_ms: u64,
    pub captured_files: u64,
    pub extracted_files: u64,
    pub prepared_changes: u64,
    pub committed_changes: u64,
    pub publications: u64,
    pub verified_objects: u64,
    pub indexed_files: Option<u64>,
    pub total_files: Option<u64>,
    pub completed_semantic_units: Option<u64>,
    pub publication: Option<RevisionToken>,
}
impl WorkProgress {
    pub fn validate(&self) -> Result<()> {
        let phase_time = self
            .phase_totals_ms
            .values()
            .try_fold(0u64, |total, ms| total.checked_add(*ms))
            .context("service phase timing overflow")?;
        ensure!(
            self.phase_totals_ms.len() <= 11
                && phase_time <= self.elapsed_ms
                && self.phase_elapsed_ms <= self.elapsed_ms
                && self.unpaused_idle_ms <= self.elapsed_ms
                && self
                    .last_advance_age_ms
                    .is_none_or(|age| age <= self.elapsed_ms)
                && self.extracted_files <= self.captured_files
                && self.committed_changes <= self.prepared_changes,
            "incoherent service progress observation"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressObservation {
    pub worker: WorkerIdentity,
    pub current_fencing_generation: u64,
    pub progress: WorkProgress,
    pub observed_unix_ms: u64,
    pub job: JobRecord,
}
