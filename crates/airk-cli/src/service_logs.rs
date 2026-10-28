// SPDX-License-Identifier: Apache-2.0
//! Finite diagnostic ring. No source/query text, paths, raw stderr or exception strings.
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
const FILES: [&str; 4] = [
    "current.jsonl",
    "previous-1.jsonl",
    "previous-2.jsonl",
    "previous-3.jsonl",
];
const FILE_BYTES: u64 = 1 << 20;
const RECORD_BYTES: usize = 768;
const READ_BYTES: usize = 64 << 10;

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
    let parent = crate::runtime_install::log_parent(root)?;
    if !exists(&parent)? {
        ensure!(create, "diagnostic log location missing");
        crate::local_ipc::prepare_root(&parent)?;
    }
    crate::local_ipc::prepare_root(&parent)?;
    let directory = parent.join(DIRECTORY);
    let receipt = parent.join(OWNER);
    let expected = expected_owner(root)?;
    if !exists(&receipt)? {
        ensure!(
            create && !exists(&directory)?,
            "diagnostic logs have no ownership receipt"
        );
        let mut temporary = tempfile::NamedTempFile::new_in(&parent)?;
        temporary.write_all(&serde_json::to_vec(&expected)?)?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&receipt)?;
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
        ensure!(count < 5, "unknown diagnostic log entries");
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
}
impl Drop for Sink {
    fn drop(&mut self) {
        let _ = self.lock.unlock();
    }
}
impl Sink {
    fn open(root: &Path) -> Result<Self> {
        let directory = owner(root, true)?;
        preflight(&directory)?;
        let lock = open_file(&directory.join("writer.lock"), true)?;
        crate::runtime_install::safe_file(&directory.join("writer.lock"))?;
        lock.try_lock()?;
        Ok(Self { directory, lock })
    }
    fn rotate(&self) -> Result<()> {
        preflight(&self.directory)?;
        let oldest = self.directory.join(FILES[3]);
        if exists(&oldest)? {
            fs::remove_file(oldest)?;
        }
        for n in (0..3).rev() {
            let from = self.directory.join(FILES[n]);
            if exists(&from)? {
                fs::rename(from, self.directory.join(FILES[n + 1]))?;
            }
        }
        Ok(())
    }
    fn append(&mut self, record: &LogRecord) -> Result<()> {
        record.validate()?;
        let mut bytes = serde_json::to_vec(record)?;
        bytes.push(b'\n');
        ensure!(
            record.schema == 1 && bytes.len() <= RECORD_BYTES,
            "invalid diagnostic log record"
        );
        preflight(&self.directory)?;
        let path = self.directory.join(FILES[0]);
        let length = if exists(&path)? {
            fs::metadata(&path)?.len()
        } else {
            0
        };
        if length + bytes.len() as u64 + 1 > FILE_BYTES {
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
        file.write_all(&bytes)?;
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
                    let Message::Record(record) = message else {
                        break;
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
    read_retained(root, usize::MAX, FILE_BYTES as usize * FILES.len(), 8 << 20)
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
