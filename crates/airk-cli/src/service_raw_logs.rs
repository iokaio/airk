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

/// Exact retained JSONL, oldest first. This explicit local CLI path never uses
/// service RPC or the redacted diagnostic report. No source files are read.
pub fn export(root: &Path) -> Result<Vec<u8>> {
    let parent = crate::runtime_install::log_parent(root)?;
    if !exists(&parent.join(OWNER))? && !exists(&parent.join(DIRECTORY))? {
        return Ok(Vec::new());
    }
    let directory = named_owner(root, false, DIRECTORY, OWNER)?;
    preflight(&directory)?;
    let mut output = Vec::new();
    for name in FILES.into_iter().rev() {
        let path = directory.join(name);
        if !exists(&path)? {
            continue;
        }
        crate::runtime_install::safe_file(&path)?;
        let mut bytes = Vec::new();
        File::open(&path)?
            .take(FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= FILE_BYTES as usize,
            "raw diagnostic file exceeds bound"
        );
        output.extend(bytes);
        // Preserve an interrupted record's exact bytes and delimit files.
        if output.last().is_some_and(|byte| *byte != b'\n') {
            output.push(b'\n');
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
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
