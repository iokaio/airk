// SPDX-License-Identifier: Apache-2.0
//! Bounded content-free process diagnostics; log records are not indexed evidence.
use crate::identity::{Incarnation, Sha256Digest};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogKind {
    ServiceStarted,
    ShutdownRequested,
    IdleExit,
    SignalExit,
    ServiceStopped,
    ControlFailed,
    ControlCapacity,
    PeerRejected,
    ConnectionFailed,
    WorkerFailed,
    JobFailed,
    WorkerStopped,
    ModelFailed,
    ModelStopped,
    SourceWorkerFailed,
    SourceTraversalUnavailable,
    SourceTraversalRecovered,
    MetadataUnavailable,
    RecordsDropped,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogRecord {
    pub schema: u32,
    pub instance: Incarnation,
    pub observed_unix_ms: u64,
    pub kind: LogKind,
    pub project: Option<Sha256Digest>,
    pub job: Option<Sha256Digest>,
    pub count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}
impl LogRecord {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema == 1
                && self.count > 0
                && (self.kind == LogKind::RecordsDropped || self.count == 1)
                && (self.job.is_none() || self.project.is_some()),
            "invalid diagnostic log record"
        );
        anyhow::ensure!(
            self.code.as_ref().is_none_or(|code| !code.is_empty()
                && code.len() <= 128
                && code
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')),
            "invalid diagnostic failure code"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogPage {
    pub schema: u32,
    /// Oldest first within the bounded recent selection. Always cached records.
    pub records: Vec<LogRecord>,
    pub stored_bytes: u64,
    pub malformed_records: u32,
    pub tail_truncated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogTelemetry {
    pub available: bool,
    pub dropped_records: u64,
    pub write_failures: u64,
}
