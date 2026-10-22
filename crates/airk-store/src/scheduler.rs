// SPDX-License-Identifier: Apache-2.0
//! Durable, bounded per-data-root indexing intents with monotonic event watermarks.
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::Duration,
};

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
    directory: PathBuf,
    _lifecycle: File,
}

struct QueueConnection {
    connection: Connection,
    _reservation: Option<crate::quota::Reservation>,
    _lifecycle: File,
}
impl std::ops::Deref for QueueConnection {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.connection
    }
}
impl std::ops::DerefMut for QueueConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }
}

impl Queue {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    fn open(&self) -> Result<QueueConnection> {
        crate::quota::ensure_current_root(&self.directory)?;
        fs::create_dir_all(&self.directory)?;
        let lifecycle = crate::quota::lifecycle_lease(&self.directory, false)?;
        crate::quota::ensure_current_root(&self.directory)?;
        let path = self.directory.join("scheduler-1.2.sqlite3");
        let existing = match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_file() && !metadata.file_type().is_symlink(),
                    "invalid scheduler database path"
                );
                metadata.len()
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        const MAX_DATABASE: u64 = 128 * 1024 * 1024;
        ensure!(
            existing <= MAX_DATABASE,
            "scheduler database exceeds 128 MiB"
        );
        let quota = crate::quota::Quota::open(&self.directory)?;
        let capacity = quota.status()?;
        let free = capacity
            .root_limit_bytes
            .saturating_sub(capacity.used_bytes)
            .saturating_sub(capacity.reserved_bytes);
        let target = (existing + 16 * 1024 * 1024).min(MAX_DATABASE);
        // Reserve new pages plus a full rollback journal and sector/page framing.
        // No cache spill or on-disk temporary tables may bypass this allocation.
        let allocation = (3 * target - existing + 65_536).min(free);
        let allowed = allocation
            .checked_add(existing)
            .and_then(|bytes| bytes.checked_sub(65_536))
            .map_or(0, |bytes| (bytes / 3 / 4096) * 4096)
            .min(MAX_DATABASE);
        ensure!(
            allowed >= existing.max(16_384),
            "root quota cannot reserve scheduler database and rollback journal"
        );
        let reservation = quota.reserve_root(allocation)?;
        reservation.charge(allocation)?;
        let mut db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(2))?;
        let page_size =
            u64::try_from(db.pragma_query_value(None, "page_size", |row| row.get::<_, i64>(0))?)?;
        let journal: String = db.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        ensure!(
            page_size == 4096 && journal.eq_ignore_ascii_case("delete"),
            "unsupported scheduler page size or journal mode"
        );
        let pages = allowed / 4096;
        let actual = u64::try_from(db.query_row(
            &format!("PRAGMA max_page_count={pages}"),
            [],
            |row| row.get::<_, i64>(0),
        )?)?;
        ensure!(
            actual <= pages,
            "scheduler grew beyond its storage reservation"
        );
        db.execute_batch("PRAGMA synchronous=FULL; PRAGMA temp_store=MEMORY; PRAGMA cache_spill=OFF; PRAGMA journal_size_limit=0;
            CREATE TABLE IF NOT EXISTS jobs (
                collection TEXT PRIMARY KEY, root TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 0,
                completed INTEGER NOT NULL DEFAULT 0, running INTEGER NOT NULL DEFAULT 0,
                paused INTEGER NOT NULL DEFAULT 0, blocked INTEGER NOT NULL DEFAULT 0,
                full_revision INTEGER NOT NULL DEFAULT 0, manual_revision INTEGER NOT NULL DEFAULT 0,
                first_pending INTEGER NOT NULL DEFAULT 0, last_event INTEGER NOT NULL DEFAULT 0,
                last_completed INTEGER NOT NULL DEFAULT 0, error TEXT, report TEXT,
                scan_started INTEGER NOT NULL DEFAULT 0, scan_completed INTEGER NOT NULL DEFAULT 0, scan_error TEXT);")?;
        // Upgrade earlier queue state without losing pending revisions or pause.
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let columns = tx
            .prepare("PRAGMA table_info(jobs)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (name, declaration) in [
            ("scan_started", "INTEGER NOT NULL DEFAULT 0"),
            ("scan_completed", "INTEGER NOT NULL DEFAULT 0"),
            ("scan_error", "TEXT"),
            ("path_hints", "TEXT NOT NULL DEFAULT '{}'"),
            ("inventory_revision", "INTEGER NOT NULL DEFAULT 0"),
            ("debounced_revision", "INTEGER NOT NULL DEFAULT 0"),
            ("inventory_completed", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            if !columns.iter().any(|column| column == name) {
                tx.execute_batch(&format!("ALTER TABLE jobs ADD COLUMN {name} {declaration}"))?;
            }
        }
        tx.commit()?;
        Ok(QueueConnection {
            connection: db,
            _reservation: Some(reservation),
            _lifecycle: lifecycle,
        })
    }

    fn open_read(&self) -> Result<QueueConnection> {
        crate::quota::ensure_current_root(&self.directory)?;
        let lifecycle = crate::quota::lifecycle_lease(&self.directory, false)?;
        crate::quota::ensure_current_root(&self.directory)?;
        let path = self.directory.join("scheduler-1.2.sqlite3");
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= 128 * 1024 * 1024,
            "invalid or oversized scheduler database"
        );
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.busy_timeout(Duration::from_secs(2))?;
        Ok(QueueConnection {
            connection: db,
            _reservation: None,
            _lifecycle: lifecycle,
        })
    }

    pub fn status(&self, collection: &str, root: &str) -> Result<Option<ScheduledJob>> {
        crate::validate_collection(collection)?;
        crate::quota::ensure_current_root(&self.directory)?;
        if !self.directory.join("scheduler-1.2.sqlite3").exists() {
            return Ok(None);
        }
        let db = self.open_read()?;
        let job = db
            .query_row(
                "SELECT * FROM jobs WHERE collection=?1",
                [collection],
                decode,
            )
            .optional()?;
        if let Some(job) = &job {
            ensure!(
                job.root == root,
                "scheduled collection belongs to another root"
            );
        }
        Ok(job)
    }

    /// Read only registered bindings; never discover or register arbitrary roots.
    pub fn list(&self) -> Result<Vec<ScheduledJob>> {
        crate::quota::ensure_current_root(&self.directory)?;
        if !self.directory.join("scheduler-1.2.sqlite3").exists() {
            return Ok(Vec::new());
        }
        let db = self.open_read()?;
        // Event routing needs bindings and policy, never retained index reports.
        // The legacy-width projection also works before a writer upgrades scans.
        let jobs = db
            .prepare(
                "SELECT collection,root,revision,completed,running,paused,blocked,
            full_revision,manual_revision,first_pending,last_event,last_completed,error,NULL
            FROM jobs ORDER BY collection LIMIT 1025",
            )?
            .query_map([], decode)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(jobs.len() <= 1024, "scheduler registration limit exceeded");
        for job in &jobs {
            crate::validate_collection(&job.collection)?;
            ensure!(
                job.root.len() <= 32768,
                "scheduler root exceeds path budget"
            );
        }
        Ok(jobs)
    }

    /// Event requests increment revisions. Explicit clicks can join the current
    /// pending pass; a newly requested full rebuild never joins an incremental pass.
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
        ensure!(root.len() <= 32768, "scheduler root exceeds path budget");
        let mut db = self.open()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior = tx
            .query_row(
                "SELECT * FROM jobs WHERE collection=?1",
                [collection],
                decode,
            )
            .optional()?;
        if let Some(prior) = &prior {
            ensure!(
                prior.root == root,
                "scheduled collection belongs to another root"
            );
        } else {
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))?;
            ensure!(count < 1024, "scheduler registration limit reached");
            tx.execute(
                "INSERT INTO jobs(collection,root) VALUES(?1,?2)",
                params![collection, root],
            )?;
        }
        let keep = prior.as_ref().is_some_and(|job| {
            coalesce
                && job.requested_revision > job.completed_revision
                && (manual || job.running_revision == 0)
                && (paths.is_none() || job.running_revision == 0)
                && !(paths.is_none()
                    && job.paths.is_some()
                    && job.running_revision == job.requested_revision)
                && (!full || job.full)
        });
        let revision = if keep {
            prior.as_ref().expect("pending job").requested_revision
        } else {
            prior
                .as_ref()
                .map_or(0, |job| job.requested_revision)
                .checked_add(1)
                .context("scheduler revision overflow")?
        };
        let mut hints: std::collections::BTreeMap<String, u64> = tx
            .query_row(
                "SELECT path_hints FROM jobs WHERE collection=?1",
                [collection],
                |row| row.get::<_, String>(0),
            )
            .map(|bytes| serde_json::from_str(&bytes))??;
        hints.retain(|_, rev| *rev > prior.as_ref().map_or(0, |job| job.completed_revision));
        if let Some(paths) = paths {
            for path in paths {
                hints.insert(path.clone(), revision);
            }
        } else {
            hints.clear();
        }
        // Overflow safely promotes to a complete reconciliation rather than losing events.
        let hints_bytes = serde_json::to_string(&hints)?;
        let use_paths = paths.is_some()
            && hints.len() <= 1024
            && hints_bytes.len() <= 1 << 20
            && prior.as_ref().is_none_or(|job| {
                job.requested_revision <= job.completed_revision || job.paths.is_some()
            });
        let hints_bytes = if use_paths { hints_bytes } else { "{}".into() };
        let revision = i64::try_from(revision)?;
        let now = i64::try_from(now)?;
        tx.execute(
            "UPDATE jobs SET revision=?2, blocked=0, error=NULL,
            first_pending=CASE WHEN completed=revision THEN ?3 ELSE first_pending END,
            last_event=CASE WHEN ?6 THEN last_event ELSE ?3 END,
            full_revision=CASE WHEN ?4 THEN ?2 ELSE full_revision END,
            manual_revision=CASE WHEN ?5 THEN ?2 ELSE manual_revision END,
            path_hints=?7, inventory_revision=CASE WHEN ?8 THEN inventory_revision ELSE ?2 END,
            debounced_revision=CASE WHEN ?9 THEN ?2 ELSE 0 END WHERE collection=?1",
            params![
                collection,
                revision,
                now,
                full,
                manual,
                keep,
                hints_bytes,
                use_paths,
                debounced
            ],
        )?;
        let job = tx.query_row(
            "SELECT * FROM jobs WHERE collection=?1",
            [collection],
            decode,
        )?;
        tx.commit()?;
        Ok(job)
    }

    pub fn pause(
        &self,
        collection: &str,
        root: &str,
        paused: bool,
    ) -> Result<Option<ScheduledJob>> {
        let prior = self.status(collection, root)?;
        let mut db = self.open()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if prior.is_none() {
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))?;
            ensure!(count < 1024, "scheduler registration limit reached");
            tx.execute(
                "INSERT OR IGNORE INTO jobs(collection,root) VALUES(?1,?2)",
                params![collection, root],
            )?;
        }
        let existing: String = tx.query_row(
            "SELECT root FROM jobs WHERE collection=?1",
            [collection],
            |row| row.get(0),
        )?;
        ensure!(
            existing == root,
            "scheduled collection belongs to another root"
        );
        // A new Pause action revokes pending manual admission too. A later
        // explicit manual request can override the persisted automatic pause.
        // Keep its revision queued so Resume rediscovers unfinished source work.
        tx.execute("UPDATE jobs SET paused=?2, manual_revision=CASE WHEN ?2 THEN completed ELSE manual_revision END, blocked=CASE WHEN ?2 THEN blocked ELSE 0 END, error=CASE WHEN ?2 THEN error ELSE NULL END WHERE collection=?1 AND root=?3", params![collection, paused, root])?;
        tx.commit()?;
        self.status(collection, root)
    }

    /// Stable advisory lock file is never unlinked, preventing lock-inode races.
    /// The operating system releases ownership when the worker exits or crashes.
    pub fn worker_lock(&self) -> Result<Option<WorkerLock>> {
        crate::quota::ensure_current_root(&self.directory)?;
        fs::create_dir_all(&self.directory)?;
        let lifecycle = crate::quota::lifecycle_lease(&self.directory, false)?;
        crate::quota::ensure_current_root(&self.directory)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("scheduler-1.2-worker.lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(WorkerLock {
                _file: file,
                directory: self.directory.clone(),
                _lifecycle: lifecycle,
            })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Caller must hold worker_lock for the entire claim/index/finish operation.
    pub fn claim(&self, lock: &WorkerLock, now: u64) -> Result<Option<ScheduledJob>> {
        ensure!(
            lock.directory == self.directory,
            "scheduler worker belongs to another data root"
        );
        let mut db = self.open()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Owning the OS lock proves every persisted running claim is abandoned.
        tx.execute("UPDATE jobs SET running=0 WHERE running<>0", [])?;
        let mut query = tx.prepare("SELECT * FROM jobs WHERE revision>completed AND blocked=0 ORDER BY first_pending,collection")?;
        let jobs: Vec<ScheduledJob> = query
            .query_map([], decode)?
            .collect::<rusqlite::Result<_>>()?;
        drop(query);
        let selected = jobs.into_iter().find(|job| {
            job.manual
                || (!job.paused && job.debounced)
                || (!job.paused
                    && (job.last_event_unix_ms > now
                        || job.last_completed_unix_ms > now
                        || now
                            >= job
                                .last_event_unix_ms
                                .saturating_add(2000)
                                .min(job.first_pending_unix_ms.saturating_add(30000))
                                .max(job.last_completed_unix_ms.saturating_add(5000))))
        });
        if let Some(job) = &selected {
            tx.execute(
                "UPDATE jobs SET running=revision WHERE collection=?1",
                [&job.collection],
            )?;
        }
        tx.commit()?;
        Ok(selected)
    }

    pub fn finish(
        &self,
        lock: &WorkerLock,
        job: &ScheduledJob,
        now: u64,
        report: Result<&str, &str>,
    ) -> Result<()> {
        ensure!(
            lock.directory == self.directory,
            "scheduler worker belongs to another data root"
        );
        let (completed, error, encoded) = match report {
            Ok(report) => {
                ensure!(
                    report.len() <= 8 * 1024 * 1024,
                    "scheduler report exceeds 8 MiB budget"
                );
                (true, None, Some(report))
            }
            Err(error) => {
                let mut end = error.len().min(4096);
                while !error.is_char_boundary(end) {
                    end -= 1;
                }
                (false, Some(error[..end].to_string()), None)
            }
        };
        let inventory_complete = encoded
            .and_then(|bytes| serde_json::from_str::<serde_json::Value>(bytes).ok())
            .is_some_and(|report| {
                report
                    .get("path_scoped")
                    .and_then(serde_json::Value::as_bool)
                    == Some(false)
            });
        let db = self.open()?;
        let changed = db.execute("UPDATE jobs SET completed=CASE WHEN ?4 THEN ?2 ELSE completed END,
            running=0, blocked=NOT ?4, error=?5, report=CASE WHEN ?4 THEN ?6 ELSE report END,
            first_pending=CASE WHEN ?4 AND revision>?2 THEN last_event ELSE first_pending END,
            last_completed=CASE WHEN ?4 THEN ?3 ELSE last_completed END,
            inventory_completed=CASE WHEN ?8 THEN ?3 ELSE inventory_completed END WHERE collection=?1 AND running=?2 AND root=?7",
            params![job.collection, i64::try_from(job.requested_revision)?, i64::try_from(now)?, completed, error, encoded, job.root, inventory_complete])?;
        ensure!(changed == 1, "scheduler claim changed before completion");
        Ok(())
    }

    /// Release a claim blocked by a live collection writer without treating
    /// contention as an indexing failure or completing the queued revision.
    pub fn defer(&self, lock: &WorkerLock, job: &ScheduledJob) -> Result<()> {
        ensure!(
            lock.directory == self.directory,
            "scheduler worker belongs to another data root"
        );
        let changed = self.open()?.execute(
            "UPDATE jobs SET running=0 WHERE collection=?1 AND root=?2 AND running=?3",
            params![
                job.collection,
                job.root,
                i64::try_from(job.requested_revision)?
            ],
        )?;
        ensure!(changed == 1, "scheduler claim changed before deferral");
        Ok(())
    }

    /// Stop automatic semantic retries after a failed attempt. A new explicit
    /// request or Resume clears this block without losing committed evidence.
    pub fn block_semantic(&self, lock: &WorkerLock, job: &ScheduledJob, error: &str) -> Result<()> {
        ensure!(
            lock.directory == self.directory,
            "scheduler worker belongs to another data root"
        );
        let mut end = error.len().min(4096);
        while !error.is_char_boundary(end) {
            end -= 1;
        }
        let changed = self.open()?.execute(
            "UPDATE jobs SET blocked=1,error=?3 WHERE collection=?1 AND root=?2 AND running=0 AND revision=completed",
            params![job.collection, job.root, &error[..end]],
        )?;
        ensure!(
            changed == 1,
            "semantic scheduler binding changed before failure"
        );
        Ok(())
    }

    pub fn forget(&self, collection: &str, root: &str) -> Result<()> {
        let lock = self
            .worker_lock()?
            .context("scheduler worker active; cannot forget collection")?;
        self.forget_locked(&lock, collection, root)
    }

    /// Select a registered idle collection for bounded periodic reconciliation.
    /// Record the start before scanning so crashes do not produce a tight retry loop.
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
        let mut db = self.open()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let job = tx
            .query_row(
                "SELECT * FROM jobs WHERE paused=0 AND blocked=0 AND revision=completed
            AND (scan_started=0 OR scan_started<=?1 OR scan_started>?2) ORDER BY scan_started,collection LIMIT 1",
                params![i64::try_from(now.saturating_sub(interval_ms))?, i64::try_from(now)?],
                decode,
            )
            .optional()?;
        if let Some(job) = &job {
            tx.execute(
                "UPDATE jobs SET scan_started=?2 WHERE collection=?1",
                params![job.collection, i64::try_from(now)?],
            )?;
        }
        tx.commit()?;
        Ok(job)
    }

    pub fn finish_scan(
        &self,
        lock: &WorkerLock,
        job: &ScheduledJob,
        now: u64,
        error: Option<&str>,
    ) -> Result<()> {
        ensure!(
            lock.directory == self.directory,
            "scheduler worker belongs to another data root"
        );
        let error = error.map(|error| error.chars().take(1000).collect::<String>());
        self.open()?.execute(
            "UPDATE jobs SET scan_completed=?2,scan_error=?3 WHERE collection=?1 AND root=?4",
            params![job.collection, i64::try_from(now)?, error, job.root],
        )?;
        Ok(())
    }
    pub fn forget_locked(&self, lock: &WorkerLock, collection: &str, root: &str) -> Result<()> {
        ensure!(
            lock.directory == self.directory,
            "scheduler worker belongs to another data root"
        );
        self.status(collection, root)?;
        let db = self.open()?;
        db.execute(
            "DELETE FROM jobs WHERE collection=?1 AND root=?2",
            params![collection, root],
        )?;
        Ok(())
    }

    pub fn data_dir(&self) -> &Path {
        &self.directory
    }

    /// Observe queued source work without materializing reports for other roots.
    pub fn has_pending_source(&self) -> Result<bool> {
        if !self.directory.join("scheduler-1.2.sqlite3").exists() {
            return Ok(false);
        }
        Ok(self.open_read()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs WHERE revision>completed AND blocked=0 AND (paused=0 OR manual_revision>completed))",
            [], |row| row.get(0),
        )?)
    }
}

fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledJob> {
    let revision = nonnegative(row, 2)?;
    let completed = nonnegative(row, 3)?;
    // A read-only status request must work before the next writer upgrades an
    // older queue. Partial or otherwise malformed schemas still fail closed.
    let legacy = row.as_ref().column_count() == 14;
    let selected = row.as_ref().column_count() >= 20;
    let (paths, recent_paths) = if selected {
        let encoded: String = row.get(17)?;
        let parsed = (|| -> Result<(Option<Vec<String>>, Vec<String>)> {
            ensure!(
                encoded.len() <= 1 << 20,
                "scheduler path hints exceed byte limit"
            );
            let hints: std::collections::BTreeMap<String, u64> = serde_json::from_str(&encoded)?;
            ensure!(
                hints.len() <= 1024 && hints.values().all(|value| *value <= revision),
                "invalid scheduler hint revisions"
            );
            if !hints.is_empty() {
                validate_paths(&hints.keys().cloned().collect::<Vec<_>>())?;
            }
            let recent = hints
                .iter()
                .filter(|(_, value)| **value == completed)
                .map(|(path, _)| path.clone())
                .collect();
            let paths: Vec<_> = hints
                .into_iter()
                .filter(|(_, revision)| *revision > completed)
                .map(|(path, _)| path)
                .collect();
            if paths.is_empty() || nonnegative(row, 18)? > completed {
                return Ok((None, recent));
            }
            validate_paths(&paths)?;
            Ok((Some(paths), recent))
        })();
        parsed.map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(17, rusqlite::types::Type::Text, error.into())
        })?
    } else {
        (None, Vec::new())
    };
    Ok(ScheduledJob {
        collection: row.get(0)?,
        root: row.get(1)?,
        requested_revision: revision,
        completed_revision: completed,
        running_revision: nonnegative(row, 4)?,
        paused: row.get(5)?,
        blocked: row.get(6)?,
        full: nonnegative(row, 7)? > completed,
        manual: nonnegative(row, 8)? > completed,
        first_pending_unix_ms: nonnegative(row, 9)?,
        last_event_unix_ms: nonnegative(row, 10)?,
        last_completed_unix_ms: nonnegative(row, 11)?,
        error: row.get(12)?,
        last_report: row.get(13)?,
        scan_started_unix_ms: if legacy { 0 } else { nonnegative(row, 14)? },
        scan_completed_unix_ms: if legacy { 0 } else { nonnegative(row, 15)? },
        inventory_completed_unix_ms: if row.as_ref().column_count() >= 21 {
            nonnegative(row, 20)?
        } else {
            0
        },
        scan_error: if legacy { None } else { row.get(16)? },
        paths,
        debounced: selected && nonnegative(row, 19)? >= revision && revision > completed,
        recent_paths,
    })
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

fn nonnegative(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    u64::try_from(row.get::<_, i64>(index)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}
