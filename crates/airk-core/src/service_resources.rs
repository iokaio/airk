// SPDX-License-Identifier: Apache-2.0
//! Content-free service resource contracts; observations and admission live in kernel.
use crate::{
    identity::{Incarnation, Sha256Digest},
    service_records::WorkerIdentity,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const DEFAULT_QUERY_CONCURRENCY: usize = 16;
pub const MAX_QUERY_CONCURRENCY: usize = 64;
pub const QUERY_CONCURRENCY_CHOICES: [usize; 5] = [4, 8, 16, 32, 64];
pub const fn default_query_concurrency() -> usize {
    DEFAULT_QUERY_CONCURRENCY
}
pub fn validate_query_concurrency(value: usize) -> Result<()> {
    ensure!(
        QUERY_CONCURRENCY_CHOICES.contains(&value),
        "concurrent evidence requests must be 4, 8, 16, 32 or 64"
    );
    Ok(())
}

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
    BackgroundIoCapacity,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceConfiguration {
    #[serde(default = "default_query_concurrency")]
    pub query_concurrency: usize,
    pub preset: ResourcePreset,
    pub extraction_threads: Option<u16>,
    pub inference_threads: Option<u16>,
    pub memory_bytes: Option<u64>,
    pub background_io_bytes_per_second: Option<u64>,
}
impl Default for ResourceConfiguration {
    fn default() -> Self {
        Self {
            query_concurrency: DEFAULT_QUERY_CONCURRENCY,
            preset: Default::default(),
            extraction_threads: None,
            inference_threads: None,
            memory_bytes: None,
            background_io_bytes_per_second: None,
        }
    }
}
impl ResourceConfiguration {
    pub fn validate(&self) -> Result<()> {
        validate_query_concurrency(self.query_concurrency)?;
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
    pub query_headroom_bytes: u64,
    pub background_io_limit_bytes_per_second: u64,
    pub background_io_reserved_bytes_per_second: u64,
    pub observed_rss_bytes: Option<u64>,
    pub available_memory_bytes: Option<u64>,
    pub telemetry_complete: bool,
    pub pending_provider_drains: usize,
    pub provider_drain_error: bool,
    pub pressure: bool,
    pub running_jobs: usize,
    pub waiting_projects: usize,
}

#[cfg(test)]
mod concurrency_tests {
    use super::*;
    #[test]
    fn saved_legacy_policy_defaults_to_sixteen_and_only_listed_values_are_valid() {
        let mut legacy = serde_json::to_value(ResourceConfiguration::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("query_concurrency");
        assert_eq!(
            serde_json::from_value::<ResourceConfiguration>(legacy)
                .unwrap()
                .query_concurrency,
            16
        );
        for value in 0..=66 {
            let configuration = ResourceConfiguration {
                query_concurrency: value,
                ..Default::default()
            };
            assert_eq!(
                configuration.validate().is_ok(),
                QUERY_CONCURRENCY_CHOICES.contains(&value),
                "{value}"
            );
        }
    }
}
