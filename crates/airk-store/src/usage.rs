// SPDX-License-Identifier: Apache-2.0
//! Small transactional daily aggregates, independent of the index writer lock.
//! A separate lifecycle lock prevents deletion/late-writer resurrection.
use airk_core::usage::*;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod wait;

pub const DAY_MS: u64 = 86_400_000;
const MAX_DEDUP: usize = 4096;
const MAX_DIAGNOSTICS: usize = 128;
const MAX_BUCKET_BYTES: usize = 65_536;
const CONTENTION_BUDGET: Duration = Duration::from_secs(2);

pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

/// Validate ownership using indexed metadata, without loading source payloads.
pub fn validate_binding(directory: &Path, root: &str, collection: &str) -> Result<()> {
    let store = crate::FileStore::new(directory);
    if let Some(scope) = store.current_owned_scope()? {
        ensure!(
            scope.root == root && scope.collection == collection,
            "usage collection/root binding mismatch"
        );
        store.current_read(&scope)?;
        return Ok(());
    }
    let db = Connection::open_with_flags(
        directory.join("index.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    db.busy_timeout(Duration::from_millis(2))?;
    let (saved_root, saved_collection): (String, String) = db.query_row(
        "SELECT root,collection FROM revision WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        saved_root == root && saved_collection == collection,
        "usage collection/root binding mismatch"
    );
    Ok(())
}

/// Optional comparison accounting never loads an oversized retained source payload.
pub fn comparison_snapshot(
    directory: &Path,
    root: &str,
    collection: &str,
) -> Result<Option<airk_core::Snapshot>> {
    let db = Connection::open_with_flags(
        directory.join("index.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    db.busy_timeout(Duration::from_millis(2))?;
    let payload: Option<String> = db.query_row(
        "SELECT CASE WHEN length(CAST(payload AS BLOB)) <= 8388608 THEN payload ELSE NULL END FROM revision WHERE id=1 AND root=?1 AND collection=?2",
        params![root, collection], |row| row.get(0)).optional()?.flatten();
    payload
        .map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}

pub struct UsageLock {
    // Release the OS lock before waking the next local waiter (field drop order).
    _file: File,
    _turn: Option<wait::Turn>,
}

fn lock(path: PathBuf) -> Result<UsageLock> {
    lock_with_budget(path, Duration::ZERO)
}

fn lock_with_budget(path: PathBuf, budget: Duration) -> Result<UsageLock> {
    let started = Instant::now();
    // Polling the OS lock alone lets a busy thread repeatedly overtake sleepers.
    // Queue local callers before contending with other processes. Both waits use
    // the original deadline; deletion's zero-budget guard remains fail-fast.
    let turn = if budget.is_zero() {
        None
    } else {
        Some(wait::acquire(&path, started, budget)?)
    };
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(!metadata.file_type().is_symlink(), "linked usage lock");
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .context("usage state is busy; observation may be dropped")?;
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) if started.elapsed() < budget => {
                std::thread::sleep(
                    Duration::from_millis(2).min(budget.saturating_sub(started.elapsed())),
                );
            }
            Err(error) => {
                return Err(anyhow::Error::from(error)
                    .context("usage state is busy; observation may be dropped"));
            }
        }
    }
    Ok(UsageLock {
        _file: file,
        _turn: turn,
    })
}

fn reserve_write(directory: &Path, bytes: u64) -> Result<crate::quota::Reservation> {
    let root = directory
        .parent()
        .and_then(Path::parent)
        .context("invalid usage data root")?;
    let collection = directory
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid usage collection")?;
    let reservation = crate::quota::Quota::open(root)?.reserve(collection, bytes)?;
    reservation.charge(bytes)?;
    Ok(reservation)
}

fn reserve_ledger(
    directory: &Path,
    policy: &UsagePolicy,
    best_effort: bool,
) -> Result<crate::quota::Reservation> {
    let current =
        fs::metadata(directory.join("usage.sqlite3")).map_or(0, |metadata| metadata.len());
    // Local max_page_count already splits the cap between DB and rollback journal.
    // Charge their maximum additional footprint before allowing SQLite to write.
    let bytes = policy
        .max_collection_bytes
        .saturating_sub(current)
        .max(16_384);
    if best_effort {
        let root = directory
            .parent()
            .and_then(Path::parent)
            .context("invalid usage data root")?;
        let collection = directory
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid usage collection")?;
        let quota = crate::quota::Quota::open(root)?;
        let started = Instant::now();
        let reservation = loop {
            match quota.try_reserve(
                collection,
                bytes,
                CONTENTION_BUDGET.saturating_sub(started.elapsed()),
            ) {
                Ok(reservation) => break reservation,
                Err(error)
                    if matches!(
                        error.downcast_ref::<std::fs::TryLockError>(),
                        Some(std::fs::TryLockError::WouldBlock)
                    ) && started.elapsed() < CONTENTION_BUDGET =>
                {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => return Err(error),
            }
        };
        reservation.charge(bytes)?;
        Ok(reservation)
    } else {
        reserve_write(directory, bytes)
    }
}

pub fn deletion_guard(collection: &Path) -> Result<UsageLock> {
    lock(collection.join("usage.lock"))
}

fn operation_guard(collection: &Path) -> Result<UsageLock> {
    lock_with_budget(collection.join("usage.lock"), CONTENTION_BUDGET)
}

fn managed(directory: &Path) -> Result<()> {
    ensure!(
        !fs::symlink_metadata(directory)?.file_type().is_symlink(),
        "linked collection usage state"
    );
    ensure!(
        directory.join("current.sqlite3").is_file()
            || directory.join("index.sqlite3").is_file()
            || directory.join("snapshot.json").is_file(),
        "collection no longer exists"
    );
    for name in ["usage.sqlite3", "usage-policy.json"] {
        let path = directory.join(name);
        if let Ok(meta) = fs::symlink_metadata(path) {
            ensure!(!meta.file_type().is_symlink(), "linked usage file");
        }
    }
    Ok(())
}

pub fn policy(directory: &Path) -> Result<UsagePolicy> {
    let path = directory.join("usage-policy.json");
    if !path.exists() {
        return Ok(UsagePolicy::default());
    }
    ensure!(
        !fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "linked usage policy"
    );
    ensure!(fs::metadata(&path)?.len() <= 4096, "usage policy too large");
    let value: UsagePolicy = serde_json::from_slice(&fs::read(path)?)?;
    validate_policy(&value)?;
    Ok(value)
}

fn validate_policy(policy: &UsagePolicy) -> Result<()> {
    ensure!(
        (1..=30).contains(&policy.aggregate_retention_days),
        "retention must be 1..30 days"
    );
    ensure!(
        policy.diagnostic_retention_hours <= 24,
        "diagnostic retention exceeds 24 hours"
    );
    ensure!(
        (65_536..=1_048_576).contains(&policy.max_collection_bytes),
        "usage collection cap must be 64 KiB..1 MiB"
    );
    ensure!(
        (policy.max_collection_bytes..=33_554_432).contains(&policy.max_data_root_bytes),
        "invalid usage data-root cap"
    );
    Ok(())
}

pub fn configure(directory: &Path, value: &UsagePolicy) -> Result<()> {
    validate_policy(value)?;
    let _guard = operation_guard(directory)?;
    managed(directory)?;
    let _quota = reserve_write(directory, 4096)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer(file.as_file_mut(), value)?;
    file.as_file_mut().sync_all()?;
    file.persist(directory.join("usage-policy.json"))?;
    Ok(())
}

fn connect(directory: &Path, create: bool) -> Result<Connection> {
    if let Ok(metadata) = fs::metadata(directory.join("usage.sqlite3")) {
        ensure!(
            metadata.len() <= 1_048_576,
            "usage database exceeds absolute cap"
        );
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | if create {
            OpenFlags::SQLITE_OPEN_CREATE
        } else {
            OpenFlags::empty()
        };
    let db = Connection::open_with_flags(directory.join("usage.sqlite3"), flags)?;
    db.busy_timeout(Duration::from_millis(2))?;
    let limit = policy(directory)?.max_collection_bytes;
    db.pragma_update(None, "max_page_count", ((limit - 16_384) / 2 / 4096) as i64)?;
    // DELETE journaling bounds transient storage and leaves no persistent WAL.
    db.execute_batch(
        "PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA trusted_schema=OFF;",
    )?;
    Ok(db)
}

fn prune_supplements(tx: &rusqlite::Transaction<'_>, oldest_day: u64) -> Result<()> {
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='supplemental_daily')",
        [],
        |row| row.get::<_, bool>(0),
    )? {
        tx.execute(
            "DELETE FROM supplemental_daily WHERE day<?1",
            [oldest_day as i64],
        )?;
        tx.execute(
            "DELETE FROM supplemental_seen WHERE timestamp<?1",
            [(oldest_day * DAY_MS) as i64],
        )?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct UsageStore {
    directory: PathBuf,
    scope: UsageScope,
    incarnation: String,
    best_effort: bool,
}

impl UsageRecorder for UsageStore {
    fn record(&self, observation: &UsageObservation) -> Result<()> {
        UsageStore::record(self, observation)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RecordingHealth {
    pub duplicate_observations: u64,
    #[serde(default)]
    pub supplemental_duplicate_observations: u64,
    pub earliest_retained_unix_ms: Option<u64>,
    pub latest_retained_unix_ms: Option<u64>,
    pub retained_days: usize,
    pub database_bytes: u64,
    /// Failures before a transaction commits cannot be durably counted by a failed store.
    pub dropped_observations: Option<u64>,
    pub deduplication_capacity: usize,
    pub complete_history: bool,
    pub useful_feedback: u64,
    pub partly_useful_feedback: u64,
    pub not_useful_feedback: u64,
}

#[derive(Serialize)]
pub struct StoredUsage {
    pub buckets: Vec<UsageReport>,
    pub lifetime: Option<UsageReport>,
    pub supplemental: Vec<SupplementalBucket>,
    pub health: RecordingHealth,
}

impl UsageStore {
    pub fn open(
        directory: &Path,
        scope: &UsageScope,
        policy: &UsagePolicy,
    ) -> Result<Option<Self>> {
        Self::open_mode(directory, scope, policy, false)
    }

    /// Adapter path: bounded contention waits and quota inventory work.
    pub fn open_best_effort(
        directory: &Path,
        scope: &UsageScope,
        policy: &UsagePolicy,
    ) -> Result<Option<Self>> {
        Self::open_mode(directory, scope, policy, true)
    }

    fn open_mode(
        directory: &Path,
        scope: &UsageScope,
        policy: &UsagePolicy,
        best_effort: bool,
    ) -> Result<Option<Self>> {
        if policy.mode == RecordingMode::Disabled {
            return Ok(None);
        }
        validate_policy(policy)?;
        let _guard = operation_guard(directory)?;
        managed(directory)?;
        crate::validate_collection(&scope.collection)?;
        if directory.join("usage.sqlite3").exists() {
            ensure!(
                fs::metadata(directory.join("usage.sqlite3"))?.len() <= 1_048_576,
                "usage database exceeds absolute cap"
            );
            // Opening an established ledger does not grow it. Avoid quota scans,
            // schema writes and journal setup until there is an observation to commit.
            let db = Connection::open_with_flags(
                directory.join("usage.sqlite3"),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            db.busy_timeout(Duration::from_millis(2))?;
            let saved: String =
                db.query_row("SELECT value FROM meta WHERE key='scope'", [], |r| r.get(0))?;
            ensure!(
                serde_json::from_str::<UsageScope>(&saved)? == *scope,
                "usage history belongs to another root binding"
            );
            let incarnation =
                db.query_row("SELECT value FROM meta WHERE key='incarnation'", [], |r| {
                    r.get(0)
                })?;
            return Ok(Some(Self {
                directory: directory.to_path_buf(),
                scope: scope.clone(),
                incarnation,
                best_effort,
            }));
        }
        let root = directory
            .parent()
            .and_then(Path::parent)
            .context("invalid collection directory")?;
        let _quota = if !directory.join("usage.sqlite3").exists() {
            Some(lock_with_budget(
                root.join("usage-quota.lock"),
                CONTENTION_BUDGET,
            )?)
        } else {
            None
        };
        // Conservative reservations include each database's maximum rollback journal.
        // New collections cannot race each other to overcommit the data-root cap.
        if !directory.join("usage.sqlite3").exists() {
            let mut reservations = 1u64;
            for (count, entry) in fs::read_dir(root.join("collections-1.2"))?.enumerate() {
                ensure!(count < 4096, "usage collection inventory exceeds bound");
                let entry = entry?;
                if entry.file_type()?.is_dir() && entry.path().join("usage.sqlite3").exists() {
                    reservations += 1;
                }
            }
            ensure!(
                reservations.saturating_mul(1_048_576) <= policy.max_data_root_bytes,
                "usage data-root capacity reached"
            );
        }
        let _managed_quota = reserve_ledger(directory, policy, best_effort)?;
        let mut db = connect(directory, true)?;
        let pages = ((policy.max_collection_bytes - 16_384) / 2 / 4096).max(6);
        db.pragma_update(None, "max_page_count", pages as i64)?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS daily (day INTEGER PRIMARY KEY, summary TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS seen (sequence INTEGER PRIMARY KEY AUTOINCREMENT, id BLOB UNIQUE NOT NULL, timestamp INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS diagnostics (id BLOB PRIMARY KEY, timestamp INTEGER NOT NULL, event TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS feedback (id BLOB PRIMARY KEY, usefulness TEXT NOT NULL);")?;
        let tx = db.transaction()?;
        let scope_json = serde_json::to_string(scope)?;
        let existing: Option<String> = tx
            .query_row("SELECT value FROM meta WHERE key='scope'", [], |r| r.get(0))
            .optional()?;
        if let Some(existing) = existing {
            ensure!(
                existing == scope_json,
                "usage history belongs to another root binding"
            );
        } else {
            // Random SQLite bytes identify this collection-history incarnation without paths.
            tx.execute("INSERT INTO meta VALUES ('scope',?1)", [&scope_json])?;
            tx.execute(
                "INSERT INTO meta VALUES ('incarnation',lower(hex(randomblob(16))))",
                [],
            )?;
        }
        let incarnation =
            tx.query_row("SELECT value FROM meta WHERE key='incarnation'", [], |r| {
                r.get(0)
            })?;
        tx.commit()?;
        Ok(Some(Self {
            directory: directory.to_path_buf(),
            scope: scope.clone(),
            incarnation,
            best_effort,
        }))
    }

    fn verify(&self, db: &Connection) -> Result<()> {
        let identity: String =
            db.query_row("SELECT value FROM meta WHERE key='incarnation'", [], |r| {
                r.get(0)
            })?;
        ensure!(
            identity == self.incarnation,
            "usage history was cleared or collection recreated"
        );
        Ok(())
    }

    pub fn record(&self, event: &UsageObservation) -> Result<()> {
        ensure!(
            event.scope == self.scope,
            "usage observation scope mismatch"
        );
        let _guard = operation_guard(&self.directory)?;
        managed(&self.directory)?;
        let current_policy = policy(&self.directory)?;
        if current_policy.mode == RecordingMode::Disabled {
            return Ok(());
        }
        let _quota = reserve_ledger(&self.directory, &current_policy, self.best_effort)?;
        let mut db = connect(&self.directory, false)?;
        self.verify(&db)?;
        let now = unix_ms();
        ensure!(
            event.unix_ms <= now.saturating_add(60_000),
            "usage timestamp is in the future"
        );
        let oldest_day =
            (now / DAY_MS).saturating_sub(u64::from(current_policy.aggregate_retention_days) - 1);
        ensure!(
            event.unix_ms / DAY_MS >= oldest_day,
            "usage observation outside retention window"
        );
        let tx = db.transaction()?;
        tx.execute("DELETE FROM daily WHERE day < ?1", [oldest_day as i64])?;
        prune_supplements(&tx, oldest_day)?;
        tx.execute(
            "DELETE FROM seen WHERE timestamp < ?1",
            [(oldest_day * DAY_MS) as i64],
        )?;
        tx.execute(
            "DELETE FROM diagnostics WHERE timestamp < ?1",
            [
                now.saturating_sub(u64::from(current_policy.diagnostic_retention_hours) * 3_600_000)
                    as i64,
            ],
        )?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO seen(id,timestamp) VALUES (?1,?2)",
            params![event.event_id.as_slice(), event.unix_ms as i64],
        )?;
        if inserted == 0 {
            tx.execute("INSERT INTO meta VALUES ('duplicates','1') ON CONFLICT(key) DO UPDATE SET value=CAST(value AS INTEGER)+1", [])?;
            tx.commit()?;
            return Ok(());
        }
        let replay_floor: i64 = tx
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key='replay_floor'",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(-1);
        ensure!(
            event.unix_ms as i64 > replay_floor,
            "observation older than bounded replay protection; dropped rather than counted twice"
        );
        let day = event.unix_ms / DAY_MS;
        let mut aggregate = summarize(
            &self.scope,
            day * DAY_MS,
            (day + 1) * DAY_MS,
            [event],
            ByteTokenHeuristic::default(),
        )?;
        // A single bounded rollup starts with new observations, never by replaying
        // retained daily buckets. It participates in the same deduplicated transaction.
        let saved: Option<String> = tx
            .query_row("SELECT value FROM meta WHERE key='lifetime'", [], |r| {
                r.get(0)
            })
            .optional()?;
        let mut lifetime = summarize(
            &self.scope,
            day * DAY_MS,
            (day + 1) * DAY_MS,
            [],
            ByteTokenHeuristic::default(),
        )?;
        if let Some(saved) = saved {
            ensure!(
                saved.len() <= MAX_BUCKET_BYTES,
                "oversize lifetime aggregate"
            );
            lifetime = serde_json::from_str(&saved)?;
        }
        lifetime.start_unix_ms = lifetime.start_unix_ms.min(day * DAY_MS);
        lifetime.end_unix_ms_exclusive = lifetime.end_unix_ms_exclusive.max((day + 1) * DAY_MS);
        lifetime.merge(&aggregate)?;
        let saved = serde_json::to_string(&lifetime)?;
        ensure!(
            saved.len() <= MAX_BUCKET_BYTES,
            "lifetime dimensions exceed cap"
        );
        tx.execute("INSERT INTO meta VALUES ('lifetime',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [saved])?;
        let old: Option<String> = tx
            .query_row(
                "SELECT summary FROM daily WHERE day=?1",
                [day as i64],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            ensure!(old.len() <= MAX_BUCKET_BYTES, "oversize usage aggregate");
            aggregate.merge(&serde_json::from_str(&old)?)?;
        }
        let encoded = serde_json::to_string(&aggregate)?;
        ensure!(
            encoded.len() <= MAX_BUCKET_BYTES,
            "usage dimensions exceed bucket cap"
        );
        tx.execute("INSERT INTO daily VALUES (?1,?2) ON CONFLICT(day) DO UPDATE SET summary=excluded.summary", params![day as i64, encoded])?;
        let evicted: Option<i64> = tx.query_row("SELECT MAX(timestamp) FROM seen WHERE sequence NOT IN (SELECT sequence FROM seen ORDER BY sequence DESC LIMIT ?1)", [MAX_DEDUP as i64], |r| r.get(0))?;
        if let Some(floor) = evicted {
            tx.execute("INSERT INTO meta VALUES ('replay_floor',?1) ON CONFLICT(key) DO UPDATE SET value=MAX(CAST(value AS INTEGER),CAST(excluded.value AS INTEGER))", [floor])?;
        }
        tx.execute("DELETE FROM seen WHERE sequence NOT IN (SELECT sequence FROM seen ORDER BY sequence DESC LIMIT ?1)", [MAX_DEDUP as i64])?;
        tx.execute(
            "DELETE FROM feedback WHERE id NOT IN (SELECT id FROM seen)",
            [],
        )?;
        if current_policy.mode == RecordingMode::DiagnosticEvents {
            let encoded = serde_json::to_string(event)?;
            ensure!(encoded.len() <= 4096, "diagnostic observation too large");
            tx.execute(
                "INSERT OR IGNORE INTO diagnostics VALUES (?1,?2,?3)",
                params![event.event_id.as_slice(), event.unix_ms as i64, encoded],
            )?;
            tx.execute("DELETE FROM diagnostics WHERE id NOT IN (SELECT id FROM diagnostics ORDER BY timestamp DESC LIMIT ?1)", [MAX_DIAGNOSTICS as i64])?;
        } else {
            tx.execute("DELETE FROM diagnostics", [])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn read(directory: &Path, scope: &UsageScope, start: u64, end: u64) -> Result<StoredUsage> {
        let mut result = StoredUsage {
            buckets: Vec::new(),
            lifetime: None,
            supplemental: Vec::new(),
            health: RecordingHealth {
                deduplication_capacity: MAX_DEDUP,
                ..Default::default()
            },
        };
        if !directory.join("usage.sqlite3").exists() {
            return Ok(result);
        }
        let _guard = operation_guard(directory)?;
        managed(directory)?;
        let db = connect(directory, false)?;
        let saved: String =
            db.query_row("SELECT value FROM meta WHERE key='scope'", [], |r| r.get(0))?;
        ensure!(
            serde_json::from_str::<UsageScope>(&saved)? == *scope,
            "usage history belongs to another root binding"
        );
        let lifetime: Option<String> = db
            .query_row("SELECT value FROM meta WHERE key='lifetime'", [], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(saved) = lifetime {
            ensure!(
                saved.len() <= MAX_BUCKET_BYTES,
                "oversize lifetime aggregate"
            );
            let aggregate: UsageReport = serde_json::from_str(&saved)?;
            ensure!(aggregate.scope == *scope, "lifetime scope mismatch");
            result.lifetime = Some(aggregate);
        }
        let current_policy = policy(directory)?;
        let oldest = (unix_ms() / DAY_MS)
            .saturating_sub(u64::from(current_policy.aggregate_retention_days) - 1)
            * DAY_MS;
        let mut query = db.prepare("SELECT summary FROM daily WHERE day*86400000 >= ?1 AND day*86400000 < ?2 ORDER BY day LIMIT 31")?;
        for encoded in query.query_map(
            params![i64::try_from(start.max(oldest))?, i64::try_from(end)?],
            |row| row.get::<_, String>(0),
        )? {
            let encoded = encoded?;
            ensure!(encoded.len() <= MAX_BUCKET_BYTES, "oversize usage bucket");
            result.buckets.push(serde_json::from_str(&encoded)?);
        }
        result.health.duplicate_observations = db
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key='duplicates'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0)
            .max(0) as u64;
        result.health.earliest_retained_unix_ms = result.buckets.first().map(|b| b.start_unix_ms);
        result.health.latest_retained_unix_ms =
            result.buckets.last().map(|b| b.end_unix_ms_exclusive);
        result.health.retained_days = result.buckets.len();
        result.health.database_bytes = fs::metadata(directory.join("usage.sqlite3"))?.len();
        for row in db.prepare("SELECT usefulness,COUNT(*) FROM feedback JOIN seen USING(id) WHERE timestamp>=?1 AND timestamp<?2 GROUP BY usefulness")?
            .query_map(params![i64::try_from(start.max(oldest))?,i64::try_from(end)?], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))? {
            let (label, count) = row?;
            match label.as_str() {
                "useful" => result.health.useful_feedback = count.max(0) as u64,
                "partly_useful" => result.health.partly_useful_feedback = count.max(0) as u64,
                "not_useful" => result.health.not_useful_feedback = count.max(0) as u64,
                _ => anyhow::bail!("invalid stored feedback category"),
            }
        }
        if db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='supplemental_daily')",
            [],
            |row| row.get::<_, bool>(0),
        )? {
            for encoded in db.prepare("SELECT summary FROM supplemental_daily WHERE day*86400000>=?1 AND day*86400000<?2 ORDER BY day LIMIT 31")?
                .query_map(params![i64::try_from(start.max(oldest))?,i64::try_from(end)?], |row| row.get::<_, String>(0))? {
                let encoded = encoded?;
                ensure!(encoded.len() <= MAX_BUCKET_BYTES, "oversize supplemental bucket");
                result.supplemental.push(serde_json::from_str(&encoded)?);
            }
            result.health.supplemental_duplicate_observations = db
                .query_row(
                    "SELECT CAST(value AS INTEGER) FROM meta WHERE key='supplemental_duplicates'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0)
                .max(0) as u64;
            let days: std::collections::BTreeSet<u64> = result
                .buckets
                .iter()
                .map(|bucket| bucket.start_unix_ms / DAY_MS)
                .chain(result.supplemental.iter().map(|bucket| bucket.day))
                .collect();
            result.health.retained_days = days.len();
            result.health.earliest_retained_unix_ms = days.first().map(|day| day * DAY_MS);
            result.health.latest_retained_unix_ms = days.last().map(|day| (day + 1) * DAY_MS);
        }
        Ok(result)
    }

    /// Explicit numeric imports share the same quota, lifecycle, policy and atomicity.
    /// Only aggregate strata and a bounded opaque deduplication window are retained.
    pub fn record_supplement(&self, event: &SupplementalMeasurement) -> Result<bool> {
        event.validate()?;
        let _guard = operation_guard(&self.directory)?;
        managed(&self.directory)?;
        let current = policy(&self.directory)?;
        ensure!(
            current.mode != RecordingMode::Disabled,
            "usage recording is disabled"
        );
        let _quota = reserve_ledger(&self.directory, &current, false)?;
        let now = unix_ms();
        let oldest = (now / DAY_MS).saturating_sub(u64::from(current.aggregate_retention_days) - 1);
        ensure!(
            event.unix_ms <= now.saturating_add(60_000) && event.unix_ms / DAY_MS >= oldest,
            "supplemental timestamp outside retention window"
        );
        let mut db = connect(&self.directory, false)?;
        self.verify(&db)?;
        let tx = db.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS supplemental_daily (day INTEGER PRIMARY KEY, summary TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS session_links (id BLOB PRIMARY KEY);
            CREATE TABLE IF NOT EXISTS supplemental_seen (sequence INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT UNIQUE NOT NULL, timestamp INTEGER NOT NULL);")?;
        tx.execute("DELETE FROM daily WHERE day<?1", [oldest as i64])?;
        tx.execute(
            "DELETE FROM seen WHERE timestamp<?1",
            [(oldest * DAY_MS) as i64],
        )?;
        tx.execute(
            "DELETE FROM feedback WHERE id NOT IN (SELECT id FROM seen)",
            [],
        )?;
        if current.mode == RecordingMode::DiagnosticEvents {
            tx.execute(
                "DELETE FROM diagnostics WHERE timestamp<?1",
                [
                    now.saturating_sub(u64::from(current.diagnostic_retention_hours) * 3_600_000)
                        as i64,
                ],
            )?;
        } else {
            tx.execute("DELETE FROM diagnostics", [])?;
        }
        tx.execute(
            "DELETE FROM supplemental_daily WHERE day<?1",
            [oldest as i64],
        )?;
        tx.execute(
            "DELETE FROM supplemental_seen WHERE timestamp<?1",
            [(oldest * DAY_MS) as i64],
        )?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO supplemental_seen(id,timestamp) VALUES (?1,?2)",
            params![event.event_id.to_ascii_lowercase(), event.unix_ms as i64],
        )?;
        if inserted == 0 {
            tx.execute("INSERT INTO meta VALUES ('supplemental_duplicates','1') ON CONFLICT(key) DO UPDATE SET value=CAST(value AS INTEGER)+1", [])?;
            tx.commit()?;
            return Ok(false);
        }
        let floor: i64 = tx
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key='supplemental_replay_floor'",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(-1);
        ensure!(
            event.unix_ms as i64 > floor,
            "supplemental observation older than bounded replay protection"
        );
        tx.execute(
            "DELETE FROM session_links WHERE id NOT IN (SELECT id FROM seen)",
            [],
        )?;
        for request in &event.airk_request_ids {
            let id: Vec<u8> = (0..32)
                .step_by(2)
                .map(|i| u8::from_str_radix(&request[i..i + 2], 16))
                .collect::<std::result::Result<_, _>>()?;
            let retained = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM seen WHERE id=?1)",
                [&id],
                |r| r.get::<_, bool>(0),
            )?;
            ensure!(
                retained,
                "linked AIRK request is unknown, expired or belongs to another collection"
            );
            tx.execute("INSERT INTO session_links VALUES (?1)", [&id])
                .context("AIRK request already linked to another session import")?;
        }
        let day = event.unix_ms / DAY_MS;
        let encoded: Option<String> = tx
            .query_row(
                "SELECT summary FROM supplemental_daily WHERE day=?1",
                [day as i64],
                |row| row.get(0),
            )
            .optional()?;
        let mut bucket = if let Some(encoded) = encoded {
            ensure!(
                encoded.len() <= MAX_BUCKET_BYTES,
                "oversize supplemental bucket"
            );
            serde_json::from_str::<SupplementalBucket>(&encoded)?
        } else {
            SupplementalBucket {
                day,
                summaries: Vec::new(),
            }
        };
        let new = SupplementSummary::new(event);
        if let Some(existing) = bucket
            .summaries
            .iter_mut()
            .find(|s| s.stratum == new.stratum)
        {
            existing.observe(event)?;
        } else {
            ensure!(
                bucket.summaries.len() < 16,
                "supplemental dimensions reached daily cap of 16; input was not recorded"
            );
            let mut new = new;
            new.observe(event)?;
            bucket.summaries.push(new);
        }
        let encoded = serde_json::to_string(&bucket)?;
        ensure!(
            encoded.len() <= MAX_BUCKET_BYTES,
            "supplemental aggregate exceeds bucket cap"
        );
        tx.execute("INSERT INTO supplemental_daily VALUES (?1,?2) ON CONFLICT(day) DO UPDATE SET summary=excluded.summary", params![day as i64,encoded])?;
        let evicted: Option<i64> = tx.query_row("SELECT MAX(timestamp) FROM supplemental_seen WHERE sequence NOT IN (SELECT sequence FROM supplemental_seen ORDER BY sequence DESC LIMIT 256)", [], |row| row.get(0))?;
        if let Some(floor) = evicted {
            tx.execute("INSERT INTO meta VALUES ('supplemental_replay_floor',?1) ON CONFLICT(key) DO UPDATE SET value=MAX(CAST(value AS INTEGER),CAST(excluded.value AS INTEGER))", [floor])?;
        }
        tx.execute("DELETE FROM supplemental_seen WHERE sequence NOT IN (SELECT sequence FROM supplemental_seen ORDER BY sequence DESC LIMIT 256)", [])?;
        tx.commit()?;
        Ok(true)
    }
}

pub fn feedback(
    directory: &Path,
    scope: &UsageScope,
    request_id: &str,
    usefulness: Usefulness,
) -> Result<()> {
    ensure!(
        request_id.len() == 32 && request_id.bytes().all(|b| b.is_ascii_hexdigit()),
        "request ID must be 32 hex digits"
    );
    let id: Vec<u8> = (0..32)
        .step_by(2)
        .map(|i| u8::from_str_radix(&request_id[i..i + 2], 16))
        .collect::<std::result::Result<_, _>>()?;
    let _guard = operation_guard(directory)?;
    managed(directory)?;
    ensure!(
        policy(directory)?.mode != RecordingMode::Disabled,
        "usage recording is disabled"
    );
    let _quota = reserve_ledger(directory, &policy(directory)?, false)?;
    let db = connect(directory, false)?;
    let saved: String =
        db.query_row("SELECT value FROM meta WHERE key='scope'", [], |r| r.get(0))?;
    ensure!(
        serde_json::from_str::<UsageScope>(&saved)? == *scope,
        "feedback belongs to another collection/root"
    );
    let label = serde_json::to_string(&usefulness)?;
    let updated = db.execute("INSERT INTO feedback SELECT id,?2 FROM seen WHERE id=?1 ON CONFLICT(id) DO UPDATE SET usefulness=excluded.usefulness", params![id,label.trim_matches('"')])?;
    ensure!(
        updated == 1,
        "request ID is unknown or outside retained feedback window"
    );
    Ok(())
}

pub fn clear(directory: &Path) -> Result<()> {
    let _guard = operation_guard(directory)?;
    managed(directory)?;
    // Reclaim without a temporary DB/journal or quota reservation when storage is full.
    // Old writers cannot open a deleted ledger; a recreated ledger has a new incarnation.
    for name in ["usage.sqlite3", "usage.sqlite3-journal"] {
        let path = directory.join(name);
        match fs::remove_file(path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
