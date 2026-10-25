// SPDX-License-Identifier: Apache-2.0
//! Content-free service resource contracts; observations and admission live in kernel.
use crate::{
    identity::{Incarnation, Sha256Digest},
    service::WorkerIdentity,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Observed native scheduling/execution exclusion, never inferred work progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    WorkerStarting,
    WorkerRoleChange,
    ResidentCapacity,
    MemoryBudget,
    MemoryPressure,
    ExtractionCapacity,
    Scheduler,
    ResourcePolicy,
    ProviderDrain,
    EmbeddingRecovery,
    IndexCapacity,
    ReaderLease,
    Publication,
    CollectionSelection,
    NativeLock,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaitObservation {
    pub job: Sha256Digest,
    pub reason: WaitReason,
    pub age_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePreset {
    #[default]
    Balanced,
    Responsive,
    Throughput,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceConfiguration {
    pub preset: ResourcePreset,
    pub extraction_threads: Option<u16>,
    pub inference_threads: Option<u16>,
    pub memory_bytes: Option<u64>,
    pub background_io_bytes_per_second: Option<u64>,
}
impl ResourceConfiguration {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.extraction_threads
                .is_none_or(|threads| (1..=64).contains(&threads))
                && self
                    .inference_threads
                    .is_none_or(|threads| (1..=4).contains(&threads)),
            "invalid service thread override"
        );
        ensure!(
            self.memory_bytes
                .is_none_or(|bytes| (1 << 30..=64 << 30).contains(&bytes)),
            "service memory override must be 1..64 GiB"
        );
        ensure!(
            self.background_io_bytes_per_second
                .is_none_or(|bytes| (1 << 20..=1 << 30).contains(&bytes)),
            "invalid background I/O override"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkAllowance {
    pub id: Incarnation,
    pub identity: WorkerIdentity,
    pub job: Sha256Digest,
    pub extraction_threads: u16,
    pub inference_threads: u16,
    pub working_memory_bytes: u64,
    pub background_io_bytes_per_second: u64,
    pub ttl_ms: u64,
}
impl WorkAllowance {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=4).contains(&self.extraction_threads)
                && (1..=4).contains(&self.inference_threads)
                && (128 << 20..=4 << 30).contains(&self.working_memory_bytes)
                && (1 << 20..=1 << 30).contains(&self.background_io_bytes_per_second)
                && (1000..=300_000).contains(&self.ttl_ms),
            "invalid service work allowance"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSnapshot {
    pub configuration: ResourceConfiguration,
    pub logical_cpus: usize,
    pub detected_memory_bytes: Option<u64>,
    pub resident_limit: usize,
    pub resident_workers: usize,
    pub extraction_threads_limit: u16,
    pub inference_threads_limit: u16,
    pub extraction_threads_reserved: u16,
    pub memory_limit_bytes: u64,
    pub memory_reserved_bytes: u64,
    pub observed_rss_bytes: Option<u64>,
    pub available_memory_bytes: Option<u64>,
    pub telemetry_complete: bool,
    pub pending_provider_drains: usize,
    pub provider_drain_error: bool,
    pub pressure: bool,
    pub running_jobs: usize,
    pub waiting_projects: usize,
}
