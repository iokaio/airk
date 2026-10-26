// SPDX-License-Identifier: Apache-2.0
//! Durable redb indexing intents with isolated worker ownership and bounded reports.
use anyhow::{Context, Result, ensure};
use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const FILE: &str = "scheduler.airk";
const KIND: &str = "airk-scheduler/2";
const CAPACITY: u64 = 128 << 20;
const MAXIMUM: usize = 128 << 20;
const REPORTS: TableDefinition<u64, &[u8]> = TableDefinition::new("scheduler_reports");

#[derive(Clone, Debug, Serialize)]
pub struct ScheduledJob {
    pub collection: String,
    pub root: String,
    pub requested_revision: u64,
    pub completed_revision: u64,
    pub running_revision: u64,
    pub paused: bool,
    pub blocked: bool,
    pub full: bool,
    pub manual: bool,
    pub first_pending_unix_ms: u64,
    pub last_event_unix_ms: u64,
    pub last_completed_unix_ms: u64,
    pub error: Option<String>,
    pub scan_started_unix_ms: u64,
    pub scan_completed_unix_ms: u64,
    pub inventory_completed_unix_ms: u64,
    pub scan_error: Option<String>,
    pub paths: Option<Vec<String>>,
    pub debounced: bool,
    #[serde(skip)]
    pub recent_paths: Vec<String>,
    #[serde(skip)]
    pub last_report: Option<String>,
}

#[derive(Clone)]
pub struct Queue {
    directory: PathBuf,
}
pub struct WorkerLock {
    _file: File,
    _compatibility: File,
    directory: PathBuf,
    collection: String,
    _lifecycle: File,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    collection: String,
    root: String,
    revision: u64,
    completed: u64,
    running: u64,
    paused: bool,
    blocked: bool,
    full_revision: u64,
    manual_revision: u64,
    first_pending: u64,
    last_event: u64,
    last_completed: u64,
    error: Option<String>,
    report: Option<u64>,
    scan_started: u64,
    scan_completed: u64,
    scan_error: Option<String>,
    hints: BTreeMap<String, u64>,
    inventory_revision: u64,
    debounced_revision: u64,
    inventory_completed: u64,
}
impl Row {
    fn new(collection: &str, root: &str) -> Self {
        Self {
            collection: collection.into(),
            root: root.into(),
            revision: 0,
            completed: 0,
            running: 0,
            paused: false,
            blocked: false,
            full_revision: 0,
            manual_revision: 0,
            first_pending: 0,
            last_event: 0,
            last_completed: 0,
            error: None,
            report: None,
            scan_started: 0,
            scan_completed: 0,
            scan_error: None,
            hints: BTreeMap::new(),
            inventory_revision: 0,
            debounced_revision: 0,
            inventory_completed: 0,
        }
    }
    fn snapshot(&self, report: Option<String>) -> Result<ScheduledJob> {
        crate::validate_collection(&self.collection)?;
        ensure!(
            self.root.len() <= 32768
                && self.completed <= self.revision
                && self.running <= self.revision
                && self.hints.len() <= 1024
                && self.hints.values().all(|v| *v <= self.revision),
            "invalid scheduler binding/revision/hints"
        );
        if !self.hints.is_empty() {
            validate_paths(&self.hints.keys().cloned().collect::<Vec<_>>())?;
        }
        for value in [
            self.revision,
            self.completed,
            self.running,
            self.full_revision,
            self.manual_revision,
            self.first_pending,
            self.last_event,
            self.last_completed,
            self.scan_started,
            self.scan_completed,
            self.inventory_revision,
            self.debounced_revision,
            self.inventory_completed,
        ] {
            i64::try_from(value)?;
        }
        let paths: Vec<_> = self
            .hints
            .iter()
            .filter(|(_, rev)| **rev > self.completed)
            .map(|(path, _)| path.clone())
            .collect();
        Ok(ScheduledJob {
            collection: self.collection.clone(),
            root: self.root.clone(),
            requested_revision: self.revision,
            completed_revision: self.completed,
            running_revision: self.running,
            paused: self.paused,
            blocked: self.blocked,
            full: self.full_revision > self.completed,
            manual: self.manual_revision > self.completed,
            first_pending_unix_ms: self.first_pending,
            last_event_unix_ms: self.last_event,
            last_completed_unix_ms: self.last_completed,
            error: self.error.clone(),
            last_report: report,
            scan_started_unix_ms: self.scan_started,
            scan_completed_unix_ms: self.scan_completed,
            scan_error: self.scan_error.clone(),
            inventory_completed_unix_ms: self.inventory_completed,
            paths: if paths.is_empty() || self.inventory_revision > self.completed {
                None
            } else {
                Some(paths)
            },
            debounced: self.debounced_revision >= self.revision && self.revision > self.completed,
            recent_paths: self
                .hints
                .iter()
                .filter(|(_, rev)| **rev == self.completed)
                .map(|(path, _)| path.clone())
                .collect(),
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    id: u64,
    collection: String,
    root: String,
    first_revision: u64,
    revision: u64,
    bytes: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    jobs: BTreeMap<String, Row>,
    reports: Vec<Report>,
    report_sequence: u64,
}
impl State {
    fn empty() -> Self {
        Self {
            version: 2,
            jobs: BTreeMap::new(),
            reports: Vec::new(),
            report_sequence: 0,
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 2 && self.jobs.len() <= 1024 && self.reports.len() <= 128,
            "scheduler format/count bound exceeded"
        );
        for (collection, row) in &self.jobs {
            ensure!(
                *collection == row.collection,
                "scheduler collection key mismatch"
            );
            row.snapshot(None)?;
        }
        let mut prior = 0;
        let mut total = 0usize;
        for report in &self.reports {
            ensure!(
                report.id > prior
                    && report.id <= self.report_sequence
                    && report.first_revision <= report.revision
                    && report.bytes <= 8 << 20,
                "invalid scheduler report identity"
            );
            prior = report.id;
            total = total
                .checked_add(report.bytes)
                .context("report bytes overflow")?;
        }
        ensure!(
            total <= 16 << 20,
            "scheduler report history byte budget exceeded"
        );
        Ok(())
    }
    fn row(&mut self, collection: &str, root: &str) -> Result<&mut Row> {
        if !self.jobs.contains_key(collection) {
            ensure!(
                self.jobs.len() < 1024,
                "scheduler registration limit reached"
            );
            self.jobs
                .insert(collection.into(), Row::new(collection, root));
        }
        let row = self
            .jobs
            .get_mut(collection)
            .context("scheduler row missing")?;
        ensure!(
            row.root == root,
            "scheduled collection belongs to another root"
        );
        Ok(row)
    }
}
fn report_bytes(bytes: &[u8]) -> Result<String> {
    ensure!(
        bytes.len() <= 8 << 20,
        "scheduler report exceeds 8 MiB budget"
    );
    Ok(std::str::from_utf8(bytes)?.into())
}
fn read_report(transaction: &redb::ReadTransaction, id: Option<u64>) -> Result<Option<String>> {
    id.map(|id| {
        let table = transaction.open_table(REPORTS)?;
        let value = table.get(id)?.context("scheduler report missing")?;
        report_bytes(value.value())
    })
    .transpose()
}
fn write_report(transaction: &redb::WriteTransaction, id: Option<u64>) -> Result<Option<String>> {
    id.map(|id| {
        let table = transaction.open_table(REPORTS)?;
        let value = table.get(id)?.context("scheduler report missing")?;
        report_bytes(value.value())
    })
    .transpose()
}
fn gc(
    state: &State,
    transaction: &redb::WriteTransaction,
    candidates: impl IntoIterator<Item = u64>,
) -> Result<()> {
    let pinned: BTreeSet<_> = state
        .jobs
        .values()
        .filter_map(|row| row.report)
        .chain(state.reports.iter().map(|report| report.id))
        .collect();
    let mut table = transaction.open_table(REPORTS)?;
    for id in candidates {
        if !pinned.contains(&id) {
            table.remove(id)?;
        }
    }
    Ok(())
}
fn brief(error: &str) -> String {
    let mut end = error.len().min(4096);
    while !error.is_char_boundary(end) {
        end -= 1;
    }
    error[..end].into()
}

impl Queue {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    fn guard(&self) -> Result<(File, File)> {
        crate::quota::ensure_current_root(&self.directory)?;
        let lifecycle = crate::quota::lifecycle_lease(&self.directory, false)?;
        crate::quota::ensure_current_root(&self.directory)?;
        let path = self.directory.join("scheduler-kv.lock");
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "linked scheduler metadata lock"
            );
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok((file, lifecycle))
    }
    fn read<R>(
        &self,
        inspect: impl FnOnce(State, &redb::ReadTransaction) -> Result<R>,
    ) -> Result<R> {
        let _guard = self.guard()?;
        crate::ledger::read_with(
            &self.directory.join(FILE),
            KIND,
            CAPACITY,
            MAXIMUM,
            |state: State, transaction| {
                state.validate()?;
                inspect(state, transaction)
            },
        )
    }
    fn exists(&self) -> Result<bool> {
        crate::quota::ensure_current_root(&self.directory)?;
        if self.directory.join(FILE).try_exists()? {
            return Ok(true);
        }
        ensure!(
            !self.directory.join("scheduler-1.2.sqlite3").try_exists()?,
            "existing scheduler requires upgrade import; queued work preserved"
        );
        Ok(false)
    }
    fn write<R>(
        &self,
        mutate: impl FnOnce(&mut State, &redb::WriteTransaction) -> Result<R>,
    ) -> Result<R> {
        crate::quota::ensure_current_root(&self.directory)?;
        fs::create_dir_all(&self.directory)?;
        let _guard = self.guard()?;
        let path = self.directory.join(FILE);
        let existing = if path.try_exists()? {
            fs::metadata(&path)?.len()
        } else {
            ensure!(
                !self.directory.join("scheduler-1.2.sqlite3").try_exists()?,
                "existing scheduler requires upgrade import; queued work preserved"
            );
            0
        };
        ensure!(existing <= CAPACITY, "scheduler database exceeds 128 MiB");
        let quota = crate::quota::Quota::open(&self.directory)?;
        let status = quota.status()?;
        let free = status
            .root_limit_bytes
            .saturating_sub(status.used_bytes)
            .saturating_sub(status.reserved_bytes);
        let allocation = (16u64 << 20)
            .min(CAPACITY.saturating_sub(existing))
            .saturating_add(65536)
            .min(free);
        ensure!(
            allocation >= 65536,
            "root quota cannot reserve scheduler database growth"
        );
        let reservation = quota.reserve_root(allocation)?;
        if existing == 0 {
            crate::ledger::create(&path, KIND, CAPACITY, MAXIMUM, &reservation, State::empty())?;
        }
        crate::ledger::write_with(
            &path,
            KIND,
            CAPACITY,
            MAXIMUM,
            &reservation,
            |state: &mut State, transaction| {
                state.validate()?;
                let result = mutate(state, transaction)?;
                state.validate()?;
                Ok(result)
            },
        )
    }
    pub fn status(&self, collection: &str, root: &str) -> Result<Option<ScheduledJob>> {
        crate::validate_collection(collection)?;
        if !self.exists()? {
            return Ok(None);
        }
        self.read(|state, transaction| {
            state
                .jobs
                .get(collection)
                .map(|row| {
                    ensure!(
                        row.root == root,
                        "scheduled collection belongs to another root"
                    );
                    row.snapshot(read_report(transaction, row.report)?)
                })
                .transpose()
        })
    }
    pub fn list(&self) -> Result<Vec<ScheduledJob>> {
        if !self.exists()? {
            return Ok(Vec::new());
        }
        self.read(|state, _| state.jobs.values().map(|row| row.snapshot(None)).collect())
    }
    pub fn enqueue(
        &self,
        collection: &str,
        root: &str,
        full: bool,
        manual: bool,
        coalesce: bool,
        now: u64,
    ) -> Result<ScheduledJob> {
        self.enqueue_selected(collection, root, full, manual, coalesce, now, None, false)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn enqueue_selected(
        &self,
        collection: &str,
        root: &str,
        full: bool,
        manual: bool,
        coalesce: bool,
        now: u64,
        paths: Option<&[String]>,
        debounced: bool,
    ) -> Result<ScheduledJob> {
        crate::validate_collection(collection)?;
        ensure!(root.len() <= 32768, "scheduler root exceeds path budget");
        i64::try_from(now)?;
        let normalized = paths.map(|paths| {
            paths
                .iter()
                .map(|path| path.replace('\\', "/"))
                .collect::<Vec<_>>()
        });
        let paths = normalized.as_deref();
        if let Some(paths) = paths {
            validate_paths(paths)?;
            ensure!(!full, "full rebuild cannot use path hints");
        }
        self.write(|state, transaction| {
            let row = state.row(collection, root)?;
            let prior = row.snapshot(None)?;
            let keep = coalesce
                && prior.requested_revision > prior.completed_revision
                && (manual || prior.running_revision == 0)
                && (paths.is_none() || prior.running_revision == 0)
                && !(paths.is_none()
                    && prior.paths.is_some()
                    && prior.running_revision == prior.requested_revision)
                && (!full || prior.full);
            let revision = if keep {
                row.revision
            } else {
                row.revision
                    .checked_add(1)
                    .context("scheduler revision overflow")?
            };
            i64::try_from(revision)?;
            row.hints.retain(|_, rev| *rev > row.completed);
            if let Some(paths) = paths {
                for path in paths {
                    row.hints.insert(path.clone(), revision);
                }
            } else {
                row.hints.clear();
            }
            let use_paths = paths.is_some()
                && row.hints.len() <= 1024
                && serde_json::to_vec(&row.hints)?.len() <= 1 << 20
                && (prior.requested_revision <= prior.completed_revision || prior.paths.is_some());
            if !use_paths {
                row.hints.clear();
                row.inventory_revision = revision;
            }
            if row.completed == row.revision {
                row.first_pending = now;
            }
            row.revision = revision;
            row.blocked = false;
            row.error = None;
            if !keep {
                row.last_event = now;
            }
            if full {
                row.full_revision = revision;
            }
            if manual {
                row.manual_revision = revision;
            }
            row.debounced_revision = if debounced { revision } else { 0 };
            row.snapshot(write_report(transaction, row.report)?)
        })
    }
    pub fn pause(
        &self,
        collection: &str,
        root: &str,
        paused: bool,
    ) -> Result<Option<ScheduledJob>> {
        crate::validate_collection(collection)?;
        ensure!(root.len() <= 32768, "scheduler root exceeds path budget");
        self.write(|state, transaction| {
            let row = state.row(collection, root)?;
            row.paused = paused;
            if paused {
                row.manual_revision = row.completed;
            } else {
                row.blocked = false;
                row.error = None;
            }
            Ok(Some(row.snapshot(write_report(transaction, row.report)?)?))
        })
    }

    pub fn worker_lock(&self, collection: &str) -> Result<Option<WorkerLock>> {
        crate::validate_collection(collection)?;
        crate::quota::ensure_current_root(&self.directory)?;
        fs::create_dir_all(&self.directory)?;
        let lifecycle = crate::quota::lifecycle_lease(&self.directory, false)?;
        crate::quota::ensure_current_root(&self.directory)?;
        // Old helpers hold this file exclusively for their whole worker turn.
        // Shared ownership admits new collection workers but excludes old claim
        // logic, which resets all running rows. Keep it for the entire turn.
        let compatibility = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("scheduler-1.2-worker.lock"))?;
        match compatibility.try_lock_shared() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => anyhow::bail!(
                "An older AIRK indexing worker is active. Close or restart older AIRK editor/watch workers, then retry with AIRK 1.3.1 or newer."
            ),
            Err(error) => return Err(error.into()),
        }
        let locks = self.directory.join("scheduler-1.3-workers");
        fs::create_dir_all(&locks)?;
        ensure!(
            !fs::symlink_metadata(&locks)?.file_type().is_symlink(),
            "linked scheduler lock directory"
        );
        let path = locks.join(format!("{collection}.lock"));
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "linked scheduler worker lock"
            );
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(WorkerLock {
                _file: file,
                _compatibility: compatibility,
                directory: self.directory.clone(),
                collection: collection.into(),
                _lifecycle: lifecycle,
            })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn claim(&self, lock: &WorkerLock, now: u64) -> Result<Option<ScheduledJob>> {
        ensure!(
            lock.directory == self.directory,
            "scheduler worker belongs to another data root"
        );
        i64::try_from(now)?;
        self.write(|state, transaction| {
            let Some(row) = state.jobs.get_mut(&lock.collection) else {
                return Ok(None);
            };
            row.running = 0;
            if row.revision <= row.completed || row.blocked {
                return Ok(None);
            }
            let job = row.snapshot(write_report(transaction, row.report)?)?;
            let eligible = job.manual
                || (!job.paused && job.debounced)
                || (!job.paused
                    && (job.last_event_unix_ms > now
                        || job.last_completed_unix_ms > now
                        || now
                            >= job
                                .last_event_unix_ms
                                .saturating_add(2000)
                                .min(job.first_pending_unix_ms.saturating_add(30000))
                                .max(job.last_completed_unix_ms.saturating_add(5000))));
            if !eligible {
                return Ok(None);
            }
            row.running = row.revision;
            Ok(Some(job))
        })
    }
    pub fn finish(
        &self,
        lock: &WorkerLock,
        job: &ScheduledJob,
        now: u64,
        report: Result<&str, &str>,
    ) -> Result<()> {
        ensure!(
            lock.directory == self.directory && lock.collection == job.collection,
            "scheduler worker belongs to another data root"
        );
        i64::try_from(now)?;
        let (completed, error, encoded) = match report {
            Ok(report) => {
                ensure!(
                    report.len() <= 8 << 20,
                    "scheduler report exceeds 8 MiB budget"
                );
                (true, None, Some(report))
            }
            Err(error) => (false, Some(brief(error)), None),
        };
        let inventory_complete = encoded
            .and_then(|bytes| serde_json::from_str::<serde_json::Value>(bytes).ok())
            .is_some_and(|report| {
                report
                    .get("path_scoped")
                    .and_then(serde_json::Value::as_bool)
                    == Some(false)
            });
        self.write(|state, transaction| {
            let row = state
                .jobs
                .get_mut(&job.collection)
                .context("scheduler claim missing")?;
            ensure!(
                row.running == job.requested_revision && row.root == job.root,
                "scheduler claim changed before completion"
            );
            row.running = 0;
            row.blocked = !completed;
            row.error = error;
            if completed {
                row.completed = job.requested_revision;
                row.last_completed = now;
                if row.revision > job.requested_revision {
                    row.first_pending = row.last_event;
                }
            }
            if inventory_complete {
                row.inventory_completed = now;
            }
            if let Some(encoded) = encoded {
                let old = row.report;
                state.report_sequence = state
                    .report_sequence
                    .checked_add(1)
                    .context("scheduler report sequence overflow")?;
                let id = state.report_sequence;
                row.report = Some(id);
                transaction
                    .open_table(REPORTS)?
                    .insert(id, encoded.as_bytes())?;
                state.reports.push(Report {
                    id,
                    collection: job.collection.clone(),
                    root: job.root.clone(),
                    first_revision: job
                        .completed_revision
                        .checked_add(1)
                        .context("report revision overflow")?,
                    revision: job.requested_revision,
                    bytes: encoded.len(),
                });
                let keep: BTreeSet<_> = state
                    .reports
                    .iter()
                    .rev()
                    .filter(|report| report.collection == job.collection && report.root == job.root)
                    .take(4)
                    .map(|report| report.id)
                    .collect();
                let mut removed: Vec<_> = old.into_iter().collect();
                state.reports.retain(|report| {
                    let preserve = report.collection != job.collection
                        || report.root != job.root
                        || keep.contains(&report.id);
                    if !preserve {
                        removed.push(report.id);
                    }
                    preserve
                });
                let mut bytes = 0usize;
                let mut position = 0usize;
                let mut preserve = BTreeSet::new();
                for report in state.reports.iter().rev() {
                    bytes = bytes
                        .checked_add(report.bytes)
                        .context("report history bytes overflow")?;
                    position += 1;
                    if bytes <= 16 << 20 && position <= 128 {
                        preserve.insert(report.id);
                    } else {
                        removed.push(report.id);
                    }
                }
                state.reports.retain(|report| preserve.contains(&report.id));
                gc(state, transaction, removed)?;
            }
            Ok(())
        })
    }
    pub fn completed_report(
        &self,
        collection: &str,
        root: &str,
        revision: u64,
    ) -> Result<Option<String>> {
        i64::try_from(revision)?;
        self.read(|state, transaction| {
            let id = state
                .reports
                .iter()
                .filter(|report| {
                    report.collection == collection
                        && report.root == root
                        && report.first_revision <= revision
                        && report.revision >= revision
                })
                .min_by_key(|report| report.revision)
                .map(|report| report.id);
            read_report(transaction, id)
        })
    }
    pub fn defer(&self, lock: &WorkerLock, job: &ScheduledJob) -> Result<()> {
        ensure!(
            lock.directory == self.directory && lock.collection == job.collection,
            "scheduler worker belongs to another data root"
        );
        self.write(|state, _| {
            let row = state
                .jobs
                .get_mut(&job.collection)
                .context("scheduler claim missing")?;
            ensure!(
                row.root == job.root && row.running == job.requested_revision,
                "scheduler claim changed before deferral"
            );
            row.running = 0;
            Ok(())
        })
    }
    pub fn block_semantic(&self, lock: &WorkerLock, job: &ScheduledJob, error: &str) -> Result<()> {
        ensure!(
            lock.directory == self.directory && lock.collection == job.collection,
            "scheduler worker belongs to another data root"
        );
        self.write(|state, _| {
            let row = state
                .jobs
                .get_mut(&job.collection)
                .context("semantic scheduler binding missing")?;
            ensure!(
                row.root == job.root && row.running == 0 && row.revision == row.completed,
                "semantic scheduler binding changed before failure"
            );
            row.blocked = true;
            row.error = Some(brief(error));
            Ok(())
        })
    }
    pub fn forget(&self, collection: &str, root: &str) -> Result<()> {
        let lock = self
            .worker_lock(collection)?
            .context("this collection is already indexing; cannot forget collection")?;
        self.forget_locked(&lock, collection, root)
    }
    pub fn claim_scan(
        &self,
        lock: &WorkerLock,
        now: u64,
        interval_ms: u64,
    ) -> Result<Option<ScheduledJob>> {
        ensure!(
            lock.directory == self.directory,
            "scheduler worker belongs to another data root"
        );
        i64::try_from(now)?;
        self.write(|state, transaction| {
            let Some(row) = state.jobs.get_mut(&lock.collection) else {
                return Ok(None);
            };
            if row.paused
                || row.blocked
                || row.revision != row.completed
                || !(row.scan_started == 0
                    || row.scan_started <= now.saturating_sub(interval_ms)
                    || row.scan_started > now)
            {
                return Ok(None);
            }
            let job = row.snapshot(write_report(transaction, row.report)?)?;
            row.scan_started = now;
            Ok(Some(job))
        })
    }
    pub fn finish_scan(
        &self,
        lock: &WorkerLock,
        job: &ScheduledJob,
        now: u64,
        error: Option<&str>,
    ) -> Result<()> {
        ensure!(
            lock.directory == self.directory && lock.collection == job.collection,
            "scheduler worker belongs to another data root"
        );
        i64::try_from(now)?;
        self.write(|state, _| {
            if let Some(row) = state.jobs.get_mut(&job.collection)
                && row.root == job.root
            {
                row.scan_completed = now;
                row.scan_error = error.map(|error| error.chars().take(1000).collect());
            }
            Ok(())
        })
    }
    pub fn forget_locked(&self, lock: &WorkerLock, collection: &str, root: &str) -> Result<()> {
        ensure!(
            lock.directory == self.directory && lock.collection == collection,
            "scheduler worker belongs to another data root"
        );
        self.write(|state, transaction| {
            if let Some(row) = state.jobs.get(collection) {
                ensure!(
                    row.root == root,
                    "scheduled collection belongs to another root"
                );
            }
            let mut removed = Vec::new();
            if let Some(row) = state.jobs.remove(collection) {
                removed.extend(row.report);
            }
            state.reports.retain(|report| {
                if report.collection == collection && report.root == root {
                    removed.push(report.id);
                    false
                } else {
                    true
                }
            });
            if !removed.is_empty() {
                gc(state, transaction, removed)?;
            }
            Ok(())
        })
    }
    pub fn data_dir(&self) -> &Path {
        &self.directory
    }
    pub fn has_pending_source(&self) -> Result<bool> {
        if !self.exists()? {
            return Ok(false);
        }
        self.read(|state, _| {
            Ok(state.jobs.values().any(|row| {
                row.revision > row.completed
                    && !row.blocked
                    && (!row.paused || row.manual_revision > row.completed)
            }))
        })
    }
}

fn validate_paths(paths: &[String]) -> Result<()> {
    ensure!(
        !paths.is_empty()
            && paths.len() <= 1024
            && paths.iter().map(String::len).sum::<usize>() <= 1 << 20,
        "scheduler path hints exceed admission limits"
    );
    ensure!(
        paths.iter().all(|path| path.len() <= 4096
            && !path.contains(['\\', ':', '\0'])
            && path
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..")),
        "invalid scheduler path hint"
    );
    Ok(())
}
