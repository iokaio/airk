// SPDX-License-Identifier: Apache-2.0
//! Explicit local-only failure evidence, separate from content-free reports/IPC.
use super::*;
use std::fmt::Write as _;

const DIRECTORY: &str = "job-errors-1";
const OWNER: &str = "job-errors-1.owner.json";
// Keep worst-case JSON escaping plus progress inside the 64 KiB worker frame.
const DETAIL_BYTES: usize = 8 << 10;
// JSON escaping can use six bytes for one byte of diagnostic text.
const RECORD_BYTES: usize = (DETAIL_BYTES * 6) + 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FailureDetail {
    pub chain: String,
    pub truncated: bool,
}
impl FailureDetail {
    pub fn capture(error: &anyhow::Error) -> Self {
        let mut output = BoundedDetail {
            text: String::new(),
            truncated: false,
        };
        for (position, cause) in error.chain().enumerate() {
            if position == 32 {
                output.truncated = true;
                break;
            }
            if writeln!(&mut output, "{position}: {cause}").is_err() {
                break;
            }
        }
        Self {
            chain: output.text,
            truncated: output.truncated,
        }
    }
}
struct BoundedDetail {
    text: String,
    truncated: bool,
}
impl std::fmt::Write for BoundedDetail {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let remaining = DETAIL_BYTES.saturating_sub(self.text.len());
        let mut end = remaining.min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&text[..end]);
        if end < text.len() {
            self.truncated = true;
            return Err(std::fmt::Error);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JobFailure {
    pub job: Sha256Digest,
    pub grant: Incarnation,
    pub worker: Incarnation,
    pub operation: airk_core::service::JobKind,
    pub code: String,
    pub detail: FailureDetail,
}
impl JobFailure {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.detail.chain.len() <= DETAIL_BYTES && !self.detail.chain.is_empty(),
            "invalid raw diagnostic detail"
        );
        ensure!(
            !self.code.is_empty()
                && self.code.len() <= 128
                && self
                    .code
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
            "invalid raw diagnostic code"
        );
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRecord {
    summary: LogRecord,
    failure: JobFailure,
}
pub(super) fn append(
    root: &Path,
    sink: &mut Option<Sink>,
    summary: &LogRecord,
    failure: &JobFailure,
) -> Result<()> {
    failure.validate()?;
    summary.validate()?;
    ensure!(
        summary.kind == LogKind::JobFailed
            && summary.job == Some(failure.job)
            && summary.code.as_deref() == Some(&failure.code),
        "raw diagnostic identity mismatch"
    );
    if sink.is_none() {
        *sink = Some(Sink::named(root, DIRECTORY, OWNER, RECORD_BYTES)?);
    }
    let value = RawRecord {
        summary: summary.clone(),
        failure: failure.clone(),
    };
    sink.as_mut()
        .context("raw diagnostic writer unavailable")?
        .append_bytes(serde_json::to_vec(&value)?)
}

/// Records written by the extension share retention with indexing-job errors.
pub fn record_client_error(root: &Path, operation: &str, input: &[u8]) -> Result<()> {
    ensure!(
        !operation.is_empty()
            && operation.len() <= 64
            && operation
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_'),
        "invalid diagnostic operation"
    );
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ClientError {
        code: String,
        detail: FailureDetail,
    }
    let failure: ClientError = serde_json::from_slice(input)?;
    ensure!(
        failure.detail.chain.len() <= DETAIL_BYTES
            && !failure.detail.chain.is_empty()
            && !failure.code.is_empty()
            && failure.code.len() <= 128
            && failure
                .code
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
        "invalid client diagnostic detail"
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .min(u64::MAX as u128) as u64;
    let record = serde_json::to_vec(
        &serde_json::json!({"schema":1,"kind":"extension_error","observed_unix_ms":now,
        "native_version":env!("CARGO_PKG_VERSION"),"operation":operation,"failure":failure}),
    )?;
    Sink::named(root, DIRECTORY, OWNER, RECORD_BYTES)?.append_bytes(record)
}

/// Exact retained JSONL, oldest first. This explicit local CLI path never uses
/// service RPC or the redacted diagnostic report. No source files are read.
pub fn export(root: &Path) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    export_to(root, &mut output)?;
    Ok(output)
}
/// Stream a finite capture to a caller-owned file; large logs never need an IPC frame.
pub fn export_to(root: &Path, output: &mut impl Write) -> Result<u64> {
    let parent = crate::runtime_install::log_parent(root)?;
    if !exists(&parent.join(OWNER))? && !exists(&parent.join(DIRECTORY))? {
        return Ok(0);
    }
    let directory = named_owner(root, false, DIRECTORY, OWNER)?;
    preflight(&directory)?;
    let mut total = 0u64;
    for name in FILES.into_iter().rev() {
        let path = directory.join(name);
        if !exists(&path)? {
            continue;
        }
        crate::runtime_install::safe_file(&path)?;
        let mut file = File::open(&path)?;
        let mut remaining = file.metadata()?.len();
        ensure!(remaining <= FILE_BYTES, "raw diagnostic file exceeds bound");
        let mut block = [0u8; 65536];
        let mut last = None;
        while remaining > 0 {
            let length = remaining.min(block.len() as u64) as usize;
            let count = file.read(&mut block[..length])?;
            if count == 0 {
                break;
            }
            total += count as u64;
            ensure!(
                total <= HIGH_WATER_BYTES + FILES.len() as u64,
                "raw diagnostic capture exceeds retention bound"
            );
            output.write_all(&block[..count])?;
            last = block.get(count - 1).copied();
            remaining -= count as u64;
        }
        // Preserve an interrupted record's exact bytes and delimit files.
        if last.is_some_and(|byte| byte != b'\n') {
            output.write_all(b"\n")?;
            total += 1;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn client_errors_share_retention_with_a_live_job_error_writer_and_require_no_job() -> Result<()>
    {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("control");
        crate::local_ipc::prepare_root(&root)?;
        let mut job_writer = Sink::named(&root, DIRECTORY, OWNER, RECORD_BYTES)?;
        job_writer.append_bytes(serde_json::to_vec(
            &serde_json::json!({"kind":"fixture_before"}),
        )?)?;
        record_client_error(&root,"open_raw_logs",br#"{"code":"status_poll_timed_out","detail":{"chain":"A local status connection timed out","truncated":false}}"#)?;
        job_writer.append_bytes(serde_json::to_vec(
            &serde_json::json!({"kind":"fixture_after"}),
        )?)?;
        let bytes = export(&root)?;
        let records = std::str::from_utf8(&bytes)?
            .lines()
            .map(serde_json::from_str::<serde_json::Value>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        assert_eq!(records.len(), 3);
        assert_eq!(records[1]["kind"], "extension_error");
        assert_eq!(records[1]["operation"], "open_raw_logs");
        assert_eq!(records[1]["failure"]["code"], "status_poll_timed_out");
        assert!(record_client_error(&root, "../../foreign", b"{}").is_err());
        assert_eq!(export(&root)?, bytes);
        let mut streamed = Vec::new();
        assert_eq!(export_to(&root, &mut streamed)?, bytes.len() as u64);
        assert_eq!(streamed, bytes);
        Ok(())
    }
    #[test]
    fn raw_failure_survives_while_summary_stays_content_free() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("private-control");
        crate::local_ipc::prepare_root(&root)?;
        let instance = Incarnation::from_bytes([1; 16]);
        let project = Sha256Digest::from_bytes([2; 32]);
        let job = Sha256Digest::from_bytes([3; 32]);
        let error = anyhow::anyhow!("private path D:\\source\\example: unexpected invariant")
            .context("prepare extraction");
        let (log, guard) = Log::start(root.clone(), instance);
        log.job_failure(
            project,
            JobFailure {
                job,
                grant: Incarnation::from_bytes([4; 16]),
                worker: Incarnation::from_bytes([5; 16]),
                operation: airk_core::service::JobKind::Reconcile,
                code: "project_operation_failed".into(),
                detail: FailureDetail::capture(&error),
            },
        );
        drop(guard);
        let summary = serde_json::to_string(&super::super::export(&root)?)?;
        assert!(summary.contains(&job.to_string()));
        assert!(!summary.contains("private path"));
        let raw = export(&root)?;
        let saved: RawRecord = serde_json::from_slice(&raw)?;
        assert_eq!(saved.summary.job, Some(job));
        assert!(saved.failure.detail.chain.contains("prepare extraction"));
        assert!(saved.failure.detail.chain.contains("unexpected invariant"));
        assert!(!saved.failure.detail.truncated);
        // Export still works if a crash left part of a UTF-8 codepoint at EOF.
        let directory = named_owner(&root, false, DIRECTORY, OWNER)?;
        OpenOptions::new()
            .append(true)
            .open(directory.join(FILES[0]))?
            .write_all(&[0xc3])?;
        assert!(export(&root)?.ends_with(&[0xc3, b'\n']));
        Ok(())
    }
    #[test]
    fn raw_detail_is_bounded_utf8_and_missing_logs_stay_missing() -> Result<()> {
        let detail = FailureDetail::capture(&anyhow::anyhow!("é".repeat(DETAIL_BYTES)));
        assert!(detail.truncated);
        assert!(detail.chain.len() <= DETAIL_BYTES);
        let root = tempfile::tempdir()?;
        assert!(export(root.path())?.is_empty());
        assert!(!root.path().join(DIRECTORY).exists());
        Ok(())
    }

    #[test]
    fn fully_escaped_error_fits_private_worker_frame() -> Result<()> {
        let detail = FailureDetail::capture(&anyhow::anyhow!("\u{0001}".repeat(DETAIL_BYTES + 1)));
        assert!(detail.truncated);
        let reply = crate::project_protocol::Reply::WorkStatus {
            observation: crate::project_protocol::WorkObservation {
                active: None,
                finished: Some(Incarnation::from_bytes([1; 16])),
                maintenance: None,
                progress: None,
                waiting_for: None,
                failure: Some(JobFailure {
                    job: Sha256Digest::from_bytes([2; 32]),
                    grant: Incarnation::from_bytes([1; 16]),
                    worker: Incarnation::from_bytes([3; 16]),
                    operation: airk_core::service::JobKind::Reconcile,
                    code: "project_operation_failed".into(),
                    detail,
                }),
            },
        };
        let mut frame = Vec::new();
        crate::service_ipc::write_frame(&mut frame, &reply)?;
        assert!(
            frame.len() < 60 * 1024,
            "reserve space for bounded progress metadata"
        );
        Ok(())
    }
}
