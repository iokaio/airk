// SPDX-License-Identifier: Apache-2.0
//! Content-free desired source revisions and watcher observations; never live-source proof.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchBackend {
    #[default]
    Disabled,
    Native,
    PeriodicFallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchFailure {
    BackendUnavailable,
    RootUnavailable,
    RootLimit,
    EventOverflow,
    MetadataUnavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchObservation {
    pub enabled: bool,
    pub requested_revision: u64,
    /// Completed source-intent revision, not a publication token or a promise of full extraction coverage.
    pub completed_revision: u64,
    pub automatic_retries_exhausted: bool,
    pub automatic_retry_canceled: bool,
    pub configuration_observed: bool,
    pub configured_roots: u16,
    pub native_roots: u16,
    pub backend: WatchBackend,
    pub failure: Option<WatchFailure>,
    pub observed_unix_ms: Option<u64>,
    pub last_reconciled_unix_ms: Option<u64>,
}

impl WatchObservation {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.completed_revision <= self.requested_revision
                && self.configured_roots <= 17
                && self.native_roots <= self.configured_roots,
            "invalid source watcher observation"
        );
        ensure!(
            (self.enabled || (self.backend == WatchBackend::Disabled && self.native_roots == 0))
                && (self.backend != WatchBackend::Native
                    || (self.configuration_observed && self.native_roots == self.configured_roots)),
            "watch backend disagrees with configured coverage"
        );
        Ok(())
    }
}
