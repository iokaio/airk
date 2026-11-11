// SPDX-License-Identifier: Apache-2.0
//! Bounded content-free process diagnostics; log records are not indexed evidence.
use crate::identity::{Incarnation, Sha256Digest};
use anyhow::{Result, ensure};
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
    ModelRecovery,
    ModelStopped,
    SourceWorkerFailed,
    SourceTraversalUnavailable,
    SourceTraversalRecovered,
    SourceProjectRemoved,
    ProjectRemovalCompleted,
    ProjectRemovalFailed,
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

/// The largest retained log record (U86; moved from `airk-cli`).
pub const MAX_RECORD_BYTES: usize = 768;

/// Add one retained file's tail to `page`, newest record first. Files are
/// read newest first; `started_mid_file` is true when the read began after
/// the file's start. Returns `true` once `limit` records are held (U86).
pub fn read_tail(page: &mut LogPage, bytes: &[u8], started_mid_file: bool, limit: usize) -> bool {
    let start = u64::from(started_mid_file);
    let complete_tail = bytes.last() == Some(&b'\n');
    page.tail_truncated |= start > 0 || !complete_tail;
    let mut lines: Vec<_> = bytes.split(|byte| *byte == b'\n').collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    if !lines.is_empty() {
        lines.pop();
    } // empty terminator or unfinished last record
    for line in lines.into_iter().rev() {
        if line.len() > MAX_RECORD_BYTES {
            page.malformed_records += 1;
            continue;
        }
        match serde_json::from_slice::<LogRecord>(line) {
            Ok(record) if record.validate().is_ok() => page.records.push(record),
            _ => page.malformed_records += 1,
        }
        if page.records.len() == limit {
            // The recent page cannot certify that older records were absent.
            page.tail_truncated = true;
            break;
        }
    }
    page.records.len() == limit
}

/// Put the records oldest first and hold the page to its encoded bound (U86).
pub fn finish(mut page: LogPage, output_bytes: usize) -> Result<LogPage> {
    page.records.reverse();
    ensure!(
        serde_json::to_vec(&page)?.len() <= output_bytes,
        "diagnostic log page exceeds bound"
    );
    Ok(page)
}

/// The text form of a recent page (U86; moved from `airk-cli`).
pub fn render(page: &LogPage) -> Result<String> {
    let mut text = format!(
        "Cached AIRK process log; not index freshness.\nStored bytes: {}; malformed records: {}; tail limited/incomplete: {}\n",
        page.stored_bytes, page.malformed_records, page.tail_truncated
    );
    ensure!(
        page.schema == 1 && page.records.len() <= 32,
        "invalid diagnostic log page"
    );
    for record in &page.records {
        record.validate()?;
        text.push_str(&format!(
            "{} {} {:?} count={}",
            record.observed_unix_ms, record.instance, record.kind, record.count
        ));
        if let Some(project) = record.project {
            text.push_str(&format!(" project={project}"));
        }
        if let Some(job) = record.job {
            text.push_str(&format!(" job={job}"));
        }
        if let Some(code) = &record.code {
            text.push_str(&format!(" code={code}"));
        }
        text.push('\n');
    }
    ensure!(text.len() <= 48 << 10, "diagnostic summary exceeds bound");
    Ok(text)
}
