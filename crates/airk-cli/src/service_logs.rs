// SPDX-License-Identifier: Apache-2.0
//! Finite summary ring; explicit local error details use a separate private ring.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_logs::{LogKind, LogPage, LogRecord, LogTelemetry},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
    thread::JoinHandle,
};

const DIRECTORY: &str = "logs-1.4";
const OWNER: &str = "logs-1.4.owner.json";
const FILES: [&str; 21] = [
    "current.jsonl",
    "previous-1.jsonl",
    "previous-2.jsonl",
    "previous-3.jsonl",
    "previous-4.jsonl",
    "previous-5.jsonl",
    "previous-6.jsonl",
    "previous-7.jsonl",
    "previous-8.jsonl",
    "previous-9.jsonl",
    "previous-10.jsonl",
    "previous-11.jsonl",
    "previous-12.jsonl",
    "previous-13.jsonl",
    "previous-14.jsonl",
    "previous-15.jsonl",
    "previous-16.jsonl",
    "previous-17.jsonl",
    "previous-18.jsonl",
    "previous-19.jsonl",
    "previous-20.jsonl",
];
const FILE_BYTES: u64 = 10_000_000;
const HIGH_WATER_BYTES: u64 = 200_000_000;
const LOW_WATER_BYTES: u64 = 100_000_000;
const RECORD_BYTES: usize = 768;
const READ_BYTES: usize = 64 << 10;

#[path = "service_raw_logs.rs"]
mod raw;
pub use raw::export as export_raw;
pub use raw::export_to as export_raw_to;
pub use raw::record_client_error;
pub(crate) use raw::{FailureDetail, JobFailure};

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Owner {
    schema: u32,
    root: Sha256Digest,
}
fn expected_owner(root: &Path) -> Result<Owner> {
    Ok(Owner {
        schema: 1,
        root: airk_core::identity::framed_digest(
            b"airk-diagnostic-log-owner/1.4",
            &[
                crate::user_paths::canonical_location(&std::path::absolute(root)?)?
                    .as_os_str()
                    .as_encoded_bytes(),
            ],
        ),
    })
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn owner(root: &Path, create: bool) -> Result<PathBuf> {
    named_owner(root, create, DIRECTORY, OWNER)
}
fn named_owner(root: &Path, create: bool, name: &str, owner_name: &str) -> Result<PathBuf> {
    let parent = crate::runtime_install::log_parent(root)?;
    if !exists(&parent)? {
        ensure!(create, "diagnostic log location missing");
        crate::local_ipc::prepare_root(&parent)?;
    }
    crate::local_ipc::prepare_root(&parent)?;
    let directory = parent.join(name);
    let receipt = parent.join(owner_name);
    let expected = expected_owner(root)?;
    if !exists(&receipt)? {
        ensure!(
            create && !exists(&directory)?,
            "diagnostic logs have no ownership receipt"
        );
        let mut temporary = tempfile::NamedTempFile::new_in(&parent)?;
        temporary.write_all(&serde_json::to_vec(&expected)?)?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(&receipt) {
            Ok(_) => {}
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    crate::runtime_install::safe_file(&receipt)?;
    let mut bytes = Vec::new();
    File::open(receipt)?.take(4097).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 4096 && serde_json::from_slice::<Owner>(&bytes)? == expected,
        "foreign diagnostic log ownership"
    );
    if !exists(&directory)? {
        ensure!(create, "diagnostic log directory missing");
        crate::local_ipc::prepare_root(&directory)?;
    }
    // Validation never changes an existing directory's permissions.
    crate::local_ipc::prepare_root(&directory)?;
    Ok(directory)
}
fn preflight(directory: &Path) -> Result<u64> {
    let mut bytes = 0;
    for (count, entry) in fs::read_dir(directory)?.enumerate() {
        ensure!(count <= FILES.len(), "unknown diagnostic log entries");
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("non-UTF-8 log entry")?;
        ensure!(
            FILES.contains(&name) || name == "writer.lock",
            "unknown diagnostic log file"
        );
        crate::runtime_install::safe_file(&entry.path())?;
        let length = entry.metadata()?.len();
        ensure!(
            length <= if name == "writer.lock" { 0 } else { FILE_BYTES },
            "oversized diagnostic log file"
        );
        bytes += length;
    }
    Ok(bytes)
}
struct Sink {
    directory: PathBuf,
    lock: File,
    record_bytes: usize,
}
impl Drop for Sink {
    fn drop(&mut self) {
        let _ = self.lock.unlock();
    }
}
impl Sink {
    fn open(root: &Path) -> Result<Self> {
        Self::named(root, DIRECTORY, OWNER, RECORD_BYTES)
    }
    fn named(root: &Path, name: &str, receipt: &str, record_bytes: usize) -> Result<Self> {
        let directory = named_owner(root, true, name, receipt)?;
        preflight(&directory)?;
        let lock = open_file(&directory.join("writer.lock"), true)?;
        crate::runtime_install::safe_file(&directory.join("writer.lock"))?;
        Ok(Self {
            directory,
            lock,
            record_bytes,
        })
    }
    fn rotate(&self) -> Result<()> {
        preflight(&self.directory)?;
        let oldest = self.directory.join(FILES[FILES.len() - 1]);
        if exists(&oldest)? {
            fs::remove_file(oldest)?;
        }
        for n in (0..FILES.len() - 1).rev() {
            let from = self.directory.join(FILES[n]);
            if exists(&from)? {
                fs::rename(from, self.directory.join(FILES[n + 1]))?;
            }
        }
        Ok(())
    }
    fn append(&mut self, record: &LogRecord) -> Result<()> {
        record.validate()?;
        self.append_bytes(serde_json::to_vec(record)?)
    }
    fn append_bytes(&mut self, mut bytes: Vec<u8>) -> Result<()> {
        self.append_retained(&mut bytes, HIGH_WATER_BYTES, LOW_WATER_BYTES, FILE_BYTES)
    }
    fn append_retained(
        &mut self,
        bytes: &mut Vec<u8>,
        high: u64,
        low: u64,
        segment: u64,
    ) -> Result<()> {
        // Service job failures and explicit client failures share one stream.
        struct WriteGuard(File);
        impl Drop for WriteGuard {
            fn drop(&mut self) {
                let _ = self.0.unlock();
            }
        }
        let write = self.lock.try_clone()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            match write.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(error) => return Err(error).context("diagnostic_writer_busy_or_unavailable"),
            }
        }
        let _guard = WriteGuard(write);
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= self.record_bytes,
            "invalid diagnostic log record"
        );
        let mut stored = preflight(&self.directory)?;
        let incoming = bytes.len() as u64 + 1;
        ensure!(
            low > incoming && low < high && high <= (FILES.len() as u64 - 1) * segment,
            "invalid log retention bounds"
        );
        if stored.saturating_add(incoming) > high {
            // Whole finite segments contain the oldest complete entries. This
            // requires no extra copy of a 100 MB log or crash-left temporary file.
            for name in FILES.iter().rev() {
                if stored.saturating_add(incoming) <= low {
                    break;
                }
                let path = self.directory.join(name);
                if exists(&path)? {
                    let length = fs::metadata(&path)?.len();
                    crate::runtime_install::safe_file(&path)?;
                    fs::remove_file(path)?;
                    stored = stored.saturating_sub(length);
                }
            }
        }
        let path = self.directory.join(FILES[0]);
        let length = if exists(&path)? {
            fs::metadata(&path)?.len()
        } else {
            0
        };
        if length + incoming > segment {
            self.rotate()?;
        }
        let mut file = open_file(&path, true)?;
        // A crash can leave one partial line. Preserve those bytes, delimit it,
        // and report it as malformed when exporting; never splice a new record.
        if file.metadata()?.len() > 0 {
            file.seek(SeekFrom::End(-1))?;
            let mut last = [0];
            file.read_exact(&mut last)?;
            if last[0] != b'\n' {
                file.write_all(b"\n")?;
            }
        }
        file.write_all(bytes)?;
        file.sync_data()?;
        Ok(())
    }
}
fn open_file(path: &Path, create: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).append(true).create(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = options.open(path)?;
    crate::runtime_install::safe_file(path)?;
    Ok(file)
}

#[derive(Default)]
struct Counters {
    available: AtomicBool,
    dropped: AtomicU64,
    pending_drops: AtomicU64,
    failures: AtomicU64,
}
impl Counters {
    fn lose(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
        self.pending_drops.fetch_add(1, Ordering::Relaxed);
    }
}
fn make_record(
    instance: Incarnation,
    kind: LogKind,
    project: Option<Sha256Digest>,
    job: Option<Sha256Digest>,
    count: u64,
) -> LogRecord {
    LogRecord {
        schema: 1,
        instance,
        kind,
        project,
        job,
        count,
        code: None,
        observed_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64),
    }
}
enum Message {
    Record(LogRecord),
    Failure(LogRecord, JobFailure),
    Stop,
}
#[derive(Clone)]
pub(crate) struct Log {
    sender: Option<SyncSender<Message>>,
    counters: Arc<Counters>,
    instance: Incarnation,
}
pub(crate) struct Guard {
    sender: Option<SyncSender<Message>>,
    thread: Option<JoinHandle<()>>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Message::Stop);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Log {
    pub fn start(root: PathBuf, instance: Incarnation) -> (Self, Guard) {
        let counters = Arc::new(Counters::default());
        let observed = counters.clone();
        let (sender, receiver) = sync_channel(64);
        let thread = std::thread::Builder::new()
            .name("airk-diagnostic-log".into())
            .spawn(move || {
                let mut sink = Sink::open(&root).ok();
                let mut raw_sink = None;
                observed.available.store(sink.is_some(), Ordering::Release);
                if sink.is_none() {
                    observed.failures.fetch_add(1, Ordering::Relaxed);
                }
                while let Ok(message) = receiver.recv() {
                    let pending = observed.pending_drops.swap(0, Ordering::AcqRel);
                    if pending > 0
                        && let Some(active) = sink.as_mut()
                        && active
                            .append(&make_record(
                                instance,
                                LogKind::RecordsDropped,
                                None,
                                None,
                                pending,
                            ))
                            .is_err()
                    {
                        observed.failures.fetch_add(1, Ordering::Relaxed);
                        observed.available.store(false, Ordering::Release);
                        sink = None;
                    }
                    let record = match message {
                        Message::Stop => break,
                        Message::Record(record) => record,
                        Message::Failure(record, failure) => {
                            let saved = raw::append(&root, &mut raw_sink, &record, &failure);
                            if saved.is_err() {
                                observed.failures.fetch_add(1, Ordering::Relaxed);
                            }
                            record
                        }
                    };
                    if let Some(active) = sink.as_mut() {
                        if active.append(&record).is_err() {
                            observed.failures.fetch_add(1, Ordering::Relaxed);
                            observed.available.store(false, Ordering::Release);
                            observed.lose();
                            sink = None;
                        }
                    } else {
                        observed.lose();
                    }
                }
            })
            .ok();
        let enabled = thread.is_some();
        if !enabled {
            counters.failures.fetch_add(1, Ordering::Relaxed);
        }
        (
            Self {
                sender: enabled.then(|| sender.clone()),
                counters,
                instance,
            },
            Guard {
                sender: enabled.then_some(sender),
                thread,
            },
        )
    }
    pub fn record(&self, kind: LogKind, project: Option<Sha256Digest>, job: Option<Sha256Digest>) {
        self.record_code(kind, project, job, None);
    }
    pub fn record_code(
        &self,
        kind: LogKind,
        project: Option<Sha256Digest>,
        job: Option<Sha256Digest>,
        code: Option<&str>,
    ) {
        let mut record = make_record(self.instance, kind, project, job, 1);
        record.code = code.map(str::to_owned);
        if record.validate().is_err() {
            record.code = Some("failure_code_unavailable".into());
        }
        if self
            .sender
            .as_ref()
            .is_none_or(|sender| sender.try_send(Message::Record(record)).is_err())
        {
            self.counters.lose();
        }
    }
    pub fn telemetry(&self) -> LogTelemetry {
        LogTelemetry {
            available: self.counters.available.load(Ordering::Acquire),
            dropped_records: self.counters.dropped.load(Ordering::Relaxed),
            write_failures: self.counters.failures.load(Ordering::Relaxed),
        }
    }
    pub fn job_failure(&self, project: Sha256Digest, failure: JobFailure) {
        let mut record = make_record(
            self.instance,
            LogKind::JobFailed,
            Some(project),
            Some(failure.job),
            1,
        );
        record.code = Some(failure.code.clone());
        if record.validate().is_err() || failure.validate().is_err() {
            self.counters.lose();
            return;
        }
        if self
            .sender
            .as_ref()
            .is_none_or(|sender| sender.try_send(Message::Failure(record, failure)).is_err())
        {
            self.counters.lose();
        }
    }
}

/// Cached diagnostics work without a live service and never create paths.
pub fn recent(root: &Path, limit: usize) -> Result<LogPage> {
    ensure!(
        (1..=32).contains(&limit),
        "invalid diagnostic log page bound"
    );
    read_retained(root, limit, READ_BYTES, 48 << 10)
}

/// Explicit local export of the entire finite ring; never sent over service IPC.
pub fn export(root: &Path) -> Result<LogPage> {
    read_retained(
        root,
        usize::MAX,
        HIGH_WATER_BYTES as usize,
        HIGH_WATER_BYTES as usize + (1 << 20),
    )
}

fn read_retained(
    root: &Path,
    limit: usize,
    read_bytes: usize,
    output_bytes: usize,
) -> Result<LogPage> {
    let directory = owner(root, false)?;
    let stored_bytes = preflight(&directory)?;
    let mut page = LogPage {
        schema: 1,
        records: Vec::new(),
        stored_bytes,
        malformed_records: 0,
        tail_truncated: false,
    };
    let mut remaining = read_bytes;
    for name in FILES {
        let path = directory.join(name);
        if !exists(&path)? {
            continue;
        }
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        let count = usize::try_from(length.min(remaining as u64))?;
        let start = length - count as u64;
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        file.take(count as u64).read_to_end(&mut bytes)?;
        remaining -= bytes.len();
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
            if line.len() > RECORD_BYTES {
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
        if page.records.len() == limit || remaining == 0 {
            break;
        }
    }
    page.records.reverse();
    ensure!(
        serde_json::to_vec(&page)?.len() <= output_bytes,
        "diagnostic log page exceeds bound"
    );
    Ok(page)
}

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

#[cfg(test)]
#[path = "service_logs_tests.rs"]
mod tests;
