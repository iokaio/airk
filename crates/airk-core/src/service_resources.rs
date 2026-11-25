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
/// Most embedding inference threads (one-thread sessions) a configuration
/// override or a work allowance may name: one per text of a full batch.
pub const MAX_INFERENCE_THREADS: u16 = 32;
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

/// The record kept beside `allow_gpu` now that GPU use is opt-in. Absent when
/// GPU use is off and no notice is pending. The owning decision is "Service-wide
/// GPU use is opt-in" in `arbiter/CONTRACT.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GpuOptIn {
    /// The user turned GPU use on through the service-wide setting.
    UserChoice,
    /// An enabled value stored under the earlier default was reset to off, and
    /// the user has not answered yet.
    DefaultReset,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceConfiguration {
    /// Off unless the user opted in. `true` is in effect only with
    /// `gpu_opt_in: user_choice`.
    #[serde(default)]
    pub allow_gpu: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_opt_in: Option<GpuOptIn>,
    #[serde(default = "default_query_concurrency")]
    pub query_concurrency: usize,
    pub preset: ResourcePreset,
    pub extraction_threads: Option<u16>,
    pub inference_threads: Option<u16>,
    pub memory_bytes: Option<u64>,
    pub background_io_bytes_per_second: Option<u64>,
}
/// Change only the service-wide GPU policy, preserving all other settings.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetGpuAllowed {
    pub allow_gpu: bool,
}

impl Default for ResourceConfiguration {
    fn default() -> Self {
        Self {
            allow_gpu: false,
            gpu_opt_in: None,
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
    /// Reads a stored configuration under the opt-in rule. An enabled value
    /// without the record was written under the earlier default: it becomes off
    /// with the reset notice pending. Every other configuration is unchanged,
    /// so applying this twice gives the same result.
    #[must_use]
    pub fn stored_under_gpu_opt_in(mut self) -> Self {
        if self.allow_gpu && self.gpu_opt_in.is_none() {
            self.allow_gpu = false;
            self.gpu_opt_in = Some(GpuOptIn::DefaultReset);
        }
        self
    }
    /// The user's answer through the service-wide setting. Either answer
    /// clears a pending reset notice.
    #[must_use]
    pub fn with_gpu_choice(mut self, allow_gpu: bool) -> Self {
        self.allow_gpu = allow_gpu;
        self.gpu_opt_in = allow_gpu.then_some(GpuOptIn::UserChoice);
        self
    }
    /// A whole replacement accepted while `previous` is in effect. It takes
    /// `allow_gpu` as requested. The record is the service's own: a request
    /// cannot create a reset notice, and one that leaves GPU use off cannot
    /// clear it.
    #[must_use]
    pub fn replacing(mut self, previous: &Self) -> Self {
        self.gpu_opt_in = if self.allow_gpu {
            Some(GpuOptIn::UserChoice)
        } else {
            previous
                .gpu_opt_in
                .filter(|record| *record == GpuOptIn::DefaultReset)
        };
        self
    }
    pub fn validate(&self) -> Result<()> {
        validate_query_concurrency(self.query_concurrency)?;
        ensure!(
            self.extraction_threads
                .is_none_or(|threads| (1..=64).contains(&threads))
                && self
                    .inference_threads
                    .is_none_or(|threads| (1..=MAX_INFERENCE_THREADS).contains(&threads)),
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
        ensure!(
            match self.gpu_opt_in {
                // Off, or enabled under the earlier default and reset when read.
                None => true,
                Some(GpuOptIn::UserChoice) => self.allow_gpu,
                Some(GpuOptIn::DefaultReset) => !self.allow_gpu,
            },
            "GPU opt-in record contradicts allow_gpu"
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
                && (1..=MAX_INFERENCE_THREADS).contains(&self.inference_threads)
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
mod gpu_opt_in_tests;

#[cfg(test)]
mod concurrency_tests {
    use super::*;
    #[test]
    fn inference_thread_overrides_and_allowances_accept_one_through_thirty_two() {
        for value in 0..=40u16 {
            let configuration = ResourceConfiguration {
                inference_threads: Some(value),
                ..Default::default()
            };
            assert_eq!(
                configuration.validate().is_ok(),
                (1..=32).contains(&value),
                "{value}"
            );
            let allowance = WorkAllowance {
                id: Incarnation::from_bytes([1; 16]),
                identity: WorkerIdentity {
                    service: Incarnation::from_bytes([2; 16]),
                    worker: Incarnation::from_bytes([3; 16]),
                    project: Sha256Digest::from_bytes([5; 32]),
                    fencing_generation: 1,
                },
                job: Sha256Digest::from_bytes([4; 32]),
                extraction_threads: 1,
                inference_threads: value,
                working_memory_bytes: 768 << 20,
                background_io_bytes_per_second: 64 << 20,
                ttl_ms: 300_000,
            };
            assert_eq!(
                allowance.validate().is_ok(),
                (1..=32).contains(&value),
                "{value}"
            );
        }
    }
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
