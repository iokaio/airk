// SPDX-License-Identifier: Apache-2.0
//! Content-free desired source revisions and watcher observations; never live-source proof.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Includes finite native buffers for up to 32,768 directory registrations.
pub const WATCHER_MEMORY_BYTES: u64 = 768 << 20;
pub const MAX_WATCH_DIRECTORIES: u16 = 32_768;
pub const MAX_PROJECT_WATCH_DIRECTORIES: u16 = 8_192;

pub fn watcher_memory_bytes(machine_memory: Option<u64>) -> u64 {
    (machine_memory.unwrap_or(8 << 30) / 64).clamp(128 << 20, WATCHER_MEMORY_BYTES)
}

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
    ProjectDirectoryLimit,
    GlobalDirectoryLimit,
    ProjectPathBytesLimit,
    GlobalPathBytesLimit,
    ScanEntryLimit,
    DepthLimit,
    PathLengthLimit,
    EventOverflow,
    MetadataUnavailable,
}

/// Bounded metadata only. A failed root walk stops at its first error, so this
/// is an observed failure list, not an exhaustive filesystem inventory.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchTraversal {
    pub unreadable: Vec<UnreadableDirectory>,
    pub incomplete_roots: Vec<u16>,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnreadableDirectory {
    pub root: u16,
    pub path: String,
    /// A bounded error category, never raw OS exception text.
    pub reason: String,
}

impl WatchTraversal {
    pub fn unavailable(&self) -> bool {
        !self.unreadable.is_empty() || self.truncated
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.unreadable.len() <= 8
                && self.incomplete_roots.len() <= 17
                && self.incomplete_roots.iter().all(|root| *root < 17)
                && self.unreadable.iter().all(|entry| entry.root < 17
                    && self.incomplete_roots.contains(&entry.root)
                    && !entry.path.is_empty()
                    && entry.path.len() <= 4096
                    && !entry.path.chars().any(char::is_control)
                    && matches!(
                        entry.reason.as_str(),
                        "not_found" | "permission_denied" | "io_error" | "redirected"
                    ))
                && serde_json::to_vec(self)?.len() <= 8192,
            "watch traversal diagnostics exceed bounds"
        );
        Ok(())
    }
    pub fn push(&mut self, entry: UnreadableDirectory) {
        if let Some(old) = self
            .unreadable
            .iter_mut()
            .find(|old| old.root == entry.root && old.path == entry.path)
        {
            if matches!(
                entry.reason.as_str(),
                "not_found" | "permission_denied" | "io_error" | "redirected"
            ) {
                old.reason = entry.reason;
            }
            return;
        }
        self.unreadable.push(entry);
        // Leave space for the final incomplete-root list and changed categories.
        if self.validate().is_err()
            || serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() > 7168)
        {
            self.unreadable.pop();
            self.truncated = true;
        }
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traversal: Option<WatchTraversal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traversal_observed_unix_ms: Option<u64>,
}

impl WatchObservation {
    pub fn validate(&self) -> Result<()> {
        if let Some(traversal) = &self.traversal {
            traversal.validate()?;
            ensure!(
                traversal
                    .incomplete_roots
                    .iter()
                    .all(|root| *root < self.configured_roots),
                "foreign watch diagnostic root"
            );
        }
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
