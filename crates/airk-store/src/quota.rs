// SPDX-License-Identifier: Apache-2.0
//! Reservations for application-written logical bytes, with process-death recovery.
//! Every participating writer must charge growth before issuing writes. This is
//! not an operating-system filesystem quota or a cap on allocation-unit overhead.
use anyhow::{Context, Result, ensure};
use rusqlite::{
    Connection, OpenFlags,
    hooks::{AuthAction, AuthContext, Authorization},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const POLICY_MAX: u64 = 65_536;
const RECORD_MAX: u64 = 1024;
const MAX_RESERVATIONS: usize = 1024;
const MAX_ENTRIES: usize = 1_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuotaPolicy {
    pub root_bytes: u64,
    pub default_collection_bytes: u64,
    #[serde(default)]
    pub collections: BTreeMap<String, u64>,
}
impl Default for QuotaPolicy {
    fn default() -> Self {
        Self {
            root_bytes: 20 * 1024 * 1024 * 1024,
            default_collection_bytes: 4 * 1024 * 1024 * 1024,
            collections: BTreeMap::new(),
        }
    }
}
impl QuotaPolicy {
    fn limit(&self, collection: &str) -> u64 {
        self.collections
            .get(collection)
            .copied()
            .unwrap_or(self.default_collection_bytes)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.collections.len() <= MAX_RESERVATIONS,
            "too many collection quota overrides"
        );
        for collection in self.collections.keys() {
            crate::validate_collection(collection)?;
        }
        ensure!(
            serde_json::to_vec(self)?.len() as u64 <= POLICY_MAX,
            "quota policy exceeds metadata budget"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CollectionQuota {
    pub used_bytes: u64,
    pub reserved_bytes: u64,
    pub limit_bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct QuotaStatus {
    pub root: PathBuf,
    pub used_bytes: u64,
    pub reserved_bytes: u64,
    pub root_limit_bytes: u64,
    pub collections: BTreeMap<String, CollectionQuota>,
    pub active_reservations: usize,
    pub accounting: &'static str,
}

#[derive(Clone)]
pub struct Quota {
    root: PathBuf,
}
pub(crate) struct LedgerLock {
    _file: File,
    lifecycle: Arc<File>,
}

/// Current clients must not silently recreate or reuse retired/private roots.
pub fn ensure_current_root(root: &Path) -> Result<()> {
    for name in [
        ".relocated-to.json",
        ".relocation-in-progress.json",
        ".relocation-pending.json",
    ] {
        let marker = root.join(name);
        if fs::symlink_metadata(&marker).is_ok() {
            let state: serde_json::Value = json_read(&marker, POLICY_MAX)?;
            let destination = state
                .get("destination")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("the recorded destination");
            anyhow::bail!(
                "data root is relocated or awaiting relocation recovery; inspect {name} and use {destination}"
            );
        }
    }
    Ok(())
}

pub(crate) fn lifecycle_lease(root: &Path, exclusive: bool) -> Result<File> {
    acquire_lifecycle(root, exclusive, false)
}

/// Only the initial relocation root lock may wait; callers hold no other locks.
pub(crate) fn relocation_lifecycle_lease(root: &Path) -> Result<File> {
    acquire_lifecycle(root, true, true)
}

fn acquire_lifecycle(root: &Path, exclusive: bool, retry_readers: bool) -> Result<File> {
    let path = root.join(".lifecycle.lock");
    if fs::symlink_metadata(&path).is_ok() {
        unlinked(&path)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if exclusive {
        // Briefly waiting here cannot invert collection/worker lock order. On
        // Unix, a concurrently forked child can inherit another thread's reader
        // descriptor until exec closes CLOEXEC descriptors, even after that
        // original reader exits. Shared acquisition below must stay fail-fast.
        let started = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock)
                    if retry_readers && started.elapsed() < Duration::from_millis(250) =>
                {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => {
                    return Err(anyhow::Error::from(error)
                        .context("data root has active allocation writers"));
                }
            }
        }
    } else {
        file.try_lock_shared()
            .context("data root relocation in progress")?;
    }
    Ok(file)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    collection: String,
    bytes: u64,
}
struct LiveRecord {
    record: Record,
}

fn unlinked(path: &Path) -> Result<()> {
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "linked quota-managed path: {}",
        path.display()
    );
    Ok(())
}
fn json_read<T: serde::de::DeserializeOwned>(path: &Path, maximum: u64) -> Result<T> {
    unlinked(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "quota metadata exceeds bounded size"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn checked_add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).context("quota byte count overflow")
}
fn within_deadline(deadline: Option<Instant>) -> Result<()> {
    ensure!(
        deadline.is_none_or(|deadline| Instant::now() < deadline),
        "quota admission deadline exhausted"
    );
    Ok(())
}

impl Quota {
    /// Canonicalizing aliases ensures all cooperating processes share one ledger.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        ensure_current_root(root.as_ref())?;
        fs::create_dir_all(root.as_ref())?;
        let root = root.as_ref().canonicalize()?;
        ensure!(
            root.to_str().is_some(),
            "non-UTF-8 quota root is unsupported"
        );
        ensure!(root.is_dir(), "quota root is not a directory");
        let quota = Self { root };
        quota.control()?;
        Ok(quota)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn control(&self) -> Result<PathBuf> {
        ensure_current_root(&self.root)?;
        ensure!(
            self.root.canonicalize()? == self.root,
            "quota root identity changed"
        );
        let control = self.root.join(".quota");
        fs::create_dir_all(&control)?;
        unlinked(&control)?;
        let records = control.join("reservations");
        fs::create_dir_all(&records)?;
        unlinked(&records)?;
        Ok(control)
    }
    fn lock(&self) -> Result<LedgerLock> {
        self.lock_before(None)
    }
    fn lock_before(&self, deadline: Option<Instant>) -> Result<LedgerLock> {
        within_deadline(deadline)?;
        ensure_current_root(&self.root)?;
        let lifecycle = Arc::new(lifecycle_lease(&self.root, false)?);
        ensure_current_root(&self.root)?;
        let path = self.control()?.join("ledger.lock");
        if fs::symlink_metadata(&path).is_ok() {
            unlinked(&path)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let started = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock)
                    if deadline.is_none() && started.elapsed() < Duration::from_secs(2) =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => return Err(error.into()),
            }
        }
        for entry in fs::read_dir(self.root.join(".quota"))? {
            within_deadline(deadline)?;
            let entry = entry?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("policy-") {
                unlinked(&entry.path())?;
                ensure!(
                    entry.file_type()?.is_file(),
                    "invalid abandoned quota policy state"
                );
                if deadline.is_none() {
                    fs::remove_file(entry.path())?;
                }
            } else {
                ensure!(
                    ["ledger.lock", "policy.json", "reservations"]
                        .iter()
                        .any(|allowed| name == *allowed),
                    "unexpected quota control entry"
                );
            }
        }
        Ok(LedgerLock {
            _file: file,
            lifecycle,
        })
    }
    fn read_policy(&self) -> Result<QuotaPolicy> {
        let path = self.root.join(".quota/policy.json");
        let policy: QuotaPolicy = if fs::symlink_metadata(&path).is_ok() {
            json_read(&path, POLICY_MAX)?
        } else {
            QuotaPolicy::default()
        };
        policy.validate()?;
        Ok(policy)
    }
    pub fn policy(&self) -> Result<QuotaPolicy> {
        let _lock = self.lock()?;
        self.read_policy()
    }
    pub fn configure(&self, policy: QuotaPolicy) -> Result<QuotaStatus> {
        policy.validate()?;
        let _lock = self.lock()?;
        let records = self.live_records()?;
        let report = self.measure(&policy, &records, None)?;
        ensure!(
            checked_add(report.used_bytes, report.reserved_bytes)? <= policy.root_bytes,
            "new root quota cannot revoke existing allocations"
        );
        for item in report.collections.values() {
            ensure!(
                checked_add(item.used_bytes, item.reserved_bytes)? <= item.limit_bytes,
                "new collection quota cannot revoke existing allocations"
            );
        }
        let mut temporary = tempfile::Builder::new()
            .prefix("policy-")
            .tempfile_in(self.root.join(".quota"))?;
        serde_json::to_writer(temporary.as_file_mut(), &policy)?;
        temporary.as_file().sync_all()?;
        temporary.persist(self.root.join(".quota/policy.json"))?;
        Ok(report)
    }
    pub fn status(&self) -> Result<QuotaStatus> {
        let _lock = self.lock()?;
        let records = self.live_records()?;
        self.measure(&self.read_policy()?, &records, None)
    }
    pub fn reserve(&self, collection: &str, bytes: u64) -> Result<Reservation> {
        crate::validate_collection(collection)?;
        self.reserve_before(collection, bytes, None)
    }
    /// Shared scheduler/control databases have no user collection identity.
    /// Root reservations expose charge accounting only, not collection writers.
    pub fn reserve_root(&self, bytes: u64) -> Result<Reservation> {
        self.reserve_before("", bytes, None)
    }
    /// Caller holds lifecycle EX, so no participating allocation can race this
    /// check. Recovery markers retain that exclusion after a crashed relocation.
    pub(crate) fn relocation_capacity(
        &self,
        additional_root_bytes: u64,
        copies: &BTreeMap<String, u64>,
    ) -> Result<()> {
        let policy = self.read_policy()?;
        let records = self.live_records()?;
        ensure!(
            records.is_empty(),
            "active quota reservations prevent relocation"
        );
        let status = self.measure(&policy, &records, None)?;
        ensure!(
            checked_add(status.used_bytes, additional_root_bytes)? <= policy.root_bytes,
            "relocation copy exceeds root quota headroom"
        );
        for (collection, bytes) in copies {
            let existing = status
                .collections
                .get(collection)
                .map_or(0, |item| item.used_bytes);
            ensure!(
                checked_add(existing, *bytes)? <= policy.limit(collection),
                "relocation copy exceeds collection quota headroom"
            );
        }
        Ok(())
    }
    /// Best-effort recording uses no lock wait and a deadline between filesystem
    /// observations. Individual operating-system I/O calls cannot be preempted.
    pub fn try_reserve(
        &self,
        collection: &str,
        bytes: u64,
        maximum_duration: Duration,
    ) -> Result<Reservation> {
        crate::validate_collection(collection)?;
        self.reserve_before(
            collection,
            bytes,
            Some(
                Instant::now()
                    .checked_add(maximum_duration)
                    .context("quota deadline overflow")?,
            ),
        )
    }
    fn reserve_before(
        &self,
        collection: &str,
        bytes: u64,
        deadline: Option<Instant>,
    ) -> Result<Reservation> {
        if !collection.is_empty() {
            crate::validate_collection(collection)?;
        }
        ensure!(bytes > 0, "quota reservation must be positive");
        let _lock = self.lock_before(deadline)?;
        let policy = self.read_policy()?;
        let records = if deadline.is_some() {
            self.records_without_recovery(deadline)?
        } else {
            self.live_records()?
        };
        ensure!(
            records.len() < MAX_RESERVATIONS,
            "quota reservation count exhausted"
        );
        let report = self.measure(&policy, &records, deadline)?;
        ensure!(
            checked_add(
                checked_add(report.used_bytes, report.reserved_bytes)?,
                bytes
            )? <= policy.root_bytes,
            "root storage quota exhausted"
        );
        let existing = report.collections.get(collection);
        let allocated = existing.map_or(Ok(0), |item| {
            checked_add(item.used_bytes, item.reserved_bytes)
        })?;
        ensure!(
            collection.is_empty() || checked_add(allocated, bytes)? <= policy.limit(collection),
            "collection storage quota exhausted"
        );
        within_deadline(deadline)?;
        let mut record_file = tempfile::Builder::new()
            .prefix("r-")
            .tempfile_in(self.root.join(".quota/reservations"))?;
        let record_path = record_file.path().to_path_buf();
        let lease_path = record_path.with_extension("lease");
        let lease = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&lease_path)?;
        lease.try_lock()?;
        serde_json::to_writer(
            record_file.as_file_mut(),
            &Record {
                collection: collection.into(),
                bytes,
            },
        )?;
        record_file.as_file().sync_all()?;
        let (_, record_path) = record_file.keep()?;
        Ok(Reservation(Arc::new(ReservationInner {
            quota: self.clone(),
            collection: collection.into(),
            record_path,
            lease_path,
            lease: Some(lease),
            limit: bytes,
            charged: AtomicU64::new(0),
            best_effort: deadline.is_some(),
            _lifecycle: _lock.lifecycle.clone(),
        })))
    }
    fn records_without_recovery(&self, deadline: Option<Instant>) -> Result<Vec<LiveRecord>> {
        let mut records = Vec::new();
        for (visited, entry) in fs::read_dir(self.root.join(".quota/reservations"))?.enumerate() {
            within_deadline(deadline)?;
            ensure!(
                visited < MAX_RESERVATIONS * 2 + 2,
                "quota ledger contains excessive state"
            );
            let entry = entry?;
            let path = entry.path();
            unlinked(&path)?;
            ensure!(entry.file_type()?.is_file(), "invalid quota record");
            if path
                .extension()
                .is_some_and(|extension| extension == "lease")
            {
                continue;
            }
            let record: Record = json_read(&path, RECORD_MAX)?;
            if !record.collection.is_empty() {
                crate::validate_collection(&record.collection)?;
            }
            // Counting abandoned allowances is conservative and avoids deleting
            // potentially large staging trees during an implicit usage hook.
            records.push(LiveRecord { record });
        }
        Ok(records)
    }
    fn live_records(&self) -> Result<Vec<LiveRecord>> {
        let mut live = Vec::new();
        let base = self.root.join(".quota/reservations");
        let entries = fs::read_dir(&base)?.collect::<std::io::Result<Vec<_>>>()?;
        ensure!(
            entries.len() <= MAX_RESERVATIONS * 2 + 2,
            "quota ledger contains excessive state"
        );
        for entry in entries {
            let path = entry.path();
            match fs::symlink_metadata(&path) {
                Ok(metadata) => ensure!(
                    !metadata.file_type().is_symlink(),
                    "linked quota reservation state"
                ),
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue, // Its dead record already removed this lease.
                Err(error) => return Err(error.into()),
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("invalid quota record name"))?;
            ensure!(
                entry.file_type()?.is_file() && name.starts_with("r-") && name.len() <= 128,
                "unexpected quota ledger entry"
            );
            if path
                .extension()
                .is_some_and(|extension| extension == "lease")
            {
                continue;
            }
            let lease_path = path.with_extension("lease");
            if !lease_path.exists() {
                fs::remove_file(path)?;
                continue;
            }
            unlinked(&lease_path)?;
            let lease = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&lease_path)?;
            match lease.try_lock() {
                Err(std::fs::TryLockError::WouldBlock) => {
                    let record: Record = json_read(&path, RECORD_MAX)?;
                    if !record.collection.is_empty() {
                        crate::validate_collection(&record.collection)?;
                    }
                    live.push(LiveRecord { record });
                }
                Ok(()) => {
                    // No writer survives with this OS lease. Only service-owned
                    // staging names are disposable; published bytes remain counted.
                    if let Ok(record) = json_read::<Record>(&path, RECORD_MAX) {
                        if !record.collection.is_empty() {
                            crate::validate_collection(&record.collection)?;
                        }
                        cleanup_staging(self, &record.collection, &path)?;
                    }
                    fs::remove_file(path)?;
                    drop(lease);
                    fs::remove_file(lease_path)?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        // Empty leases can survive a crash before their record was persisted.
        for entry in fs::read_dir(&base)? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "lease")
                && !path.with_extension("").exists()
            {
                let lease = OpenOptions::new().read(true).write(true).open(&path)?;
                if lease.try_lock().is_ok() {
                    drop(lease);
                    fs::remove_file(path)?;
                }
            }
        }
        Ok(live)
    }
    fn measure(
        &self,
        policy: &QuotaPolicy,
        records: &[LiveRecord],
        deadline: Option<Instant>,
    ) -> Result<QuotaStatus> {
        let mut result = QuotaStatus {
            root: self.root.clone(),
            used_bytes: 0,
            reserved_bytes: 0,
            root_limit_bytes: policy.root_bytes,
            collections: BTreeMap::new(),
            active_reservations: records.len(),
            accounting: "logical managed file lengths plus full outstanding reservations; quota control metadata excluded",
        };
        let mut pending = vec![self.root.clone()];
        let mut visited = 0usize;
        while let Some(directory) = pending.pop() {
            within_deadline(deadline)?;
            for entry in fs::read_dir(directory)? {
                within_deadline(deadline)?;
                let entry = entry?;
                visited += 1;
                ensure!(
                    visited <= MAX_ENTRIES,
                    "quota inventory entry limit exhausted; refusing allocation"
                );
                let path = entry.path();
                if path == self.root.join(".quota") {
                    continue;
                }
                ensure!(
                    !entry.file_type()?.is_symlink(),
                    "quota inventory contains linked state"
                );
                if entry.file_type()?.is_dir() {
                    pending.push(path);
                    continue;
                }
                ensure!(
                    entry.file_type()?.is_file(),
                    "quota inventory contains nonregular state"
                );
                let bytes = entry.metadata()?.len();
                result.used_bytes = checked_add(result.used_bytes, bytes)?;
                if let Ok(relative) = path.strip_prefix(self.root.join("collections"))
                    && let Some(Component::Normal(collection)) = relative.components().next()
                    && let Some(collection) = collection
                        .to_str()
                        .filter(|name| crate::validate_collection(name).is_ok())
                {
                    let item =
                        result
                            .collections
                            .entry(collection.into())
                            .or_insert(CollectionQuota {
                                used_bytes: 0,
                                reserved_bytes: 0,
                                limit_bytes: policy.limit(collection),
                            });
                    item.used_bytes = checked_add(item.used_bytes, bytes)?;
                }
            }
        }
        for record in records {
            result.reserved_bytes = checked_add(result.reserved_bytes, record.record.bytes)?;
            if record.record.collection.is_empty() {
                continue;
            }
            let item = result
                .collections
                .entry(record.record.collection.clone())
                .or_insert(CollectionQuota {
                    used_bytes: 0,
                    reserved_bytes: 0,
                    limit_bytes: policy.limit(&record.record.collection),
                });
            item.reserved_bytes = checked_add(item.reserved_bytes, record.record.bytes)?;
        }
        Ok(result)
    }
}

struct ReservationInner {
    quota: Quota,
    collection: String,
    record_path: PathBuf,
    lease_path: PathBuf,
    lease: Option<File>,
    limit: u64,
    charged: AtomicU64,
    best_effort: bool,
    _lifecycle: Arc<File>,
}
#[derive(Clone)]
pub struct Reservation(Arc<ReservationInner>);
impl Reservation {
    pub fn limit(&self) -> u64 {
        self.0.limit
    }
    pub fn charged(&self) -> u64 {
        self.0.charged.load(Ordering::Acquire)
    }
    pub fn remaining(&self) -> u64 {
        self.limit().saturating_sub(self.charged())
    }
    /// Charge before issuing any externally implemented provider write. Charges
    /// are never refunded; failed writes cannot accidentally create extra credit.
    pub fn charge(&self, bytes: u64) -> Result<()> {
        self.0
            .charged
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |prior| {
                prior
                    .checked_add(bytes)
                    .filter(|next| *next <= self.0.limit)
            })
            .map_err(|_| {
                anyhow::anyhow!(
                    "publication reservation exhausted; previous generation must remain active"
                )
            })?;
        Ok(())
    }
    fn checked_destination(&self, path: &Path) -> Result<PathBuf> {
        crate::validate_collection(&self.0.collection)?;
        self.0.quota.control()?;
        let base = self
            .0
            .quota
            .root
            .join("collections")
            .join(&self.0.collection);
        let relative = path
            .strip_prefix(&base)
            .context("quota writer target is outside its collection")?;
        ensure!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "noncanonical quota writer target"
        );
        ensure!(
            relative.file_name().is_some(),
            "quota writer requires a file target"
        );
        let mut current = self.0.quota.root.clone();
        for part in path
            .strip_prefix(&self.0.quota.root)?
            .parent()
            .context("quota target parent missing")?
            .components()
        {
            current.push(part);
            if !current.exists() {
                match fs::create_dir(&current) {
                    Ok(()) => (),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
                    Err(error) => return Err(error.into()),
                }
            }
            unlinked(&current)?;
            ensure!(current.is_dir(), "quota target parent is not a directory");
        }
        if path.exists() {
            unlinked(path)?;
        }
        Ok(path.into())
    }
    pub fn staging_directory(&self) -> Result<PathBuf> {
        let path = staging_path(&self.0.quota, &self.0.collection, &self.0.record_path)?;
        self.checked_destination(&path.join("ownership-check"))?;
        Ok(path)
    }
    pub fn create_file(&self, path: impl AsRef<Path>) -> Result<QuotaWriter> {
        let path = self.checked_destination(path.as_ref())?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        Ok(QuotaWriter {
            file,
            known_len: 0,
            reservation: self.clone(),
        })
    }
    pub fn copy_file(
        &self,
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<u64> {
        let mut source = File::open(source)?;
        let mut destination = self.create_file(destination)?;
        let copied = io::copy(&mut source, &mut destination)?;
        destination.sync_all()?;
        Ok(copied)
    }
    /// An unpublished SQLite file has no recovery journal: a failed/crashed build
    /// is discarded, while the old published database remains untouched.
    pub fn open_private_sqlite(
        &self,
        path: impl AsRef<Path>,
        maximum: u64,
    ) -> Result<PrivateDatabase> {
        let pages = maximum / 4096;
        ensure!(
            (4..=u32::MAX as u64 - 1).contains(&pages),
            "private SQLite quota must allow at least four pages"
        );
        let bytes = pages * 4096;
        self.charge(bytes)?;
        let path = self.checked_destination(path.as_ref())?;
        drop(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?,
        );
        let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.execute_batch(&format!("PRAGMA page_size=4096; PRAGMA journal_mode=OFF; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-16384; PRAGMA cache_spill=ON; PRAGMA max_page_count={pages};"))?;
        let actual_pages: i64 =
            connection.query_row("PRAGMA max_page_count", [], |row| row.get(0))?;
        ensure!(
            u64::try_from(actual_pages)? == pages,
            "SQLite refused private database byte cap"
        );
        connection.authorizer(Some(|context: AuthContext<'_>| match context.action {
            AuthAction::Attach { .. } | AuthAction::Detach { .. } => Authorization::Deny,
            AuthAction::Pragma {
                pragma_name,
                pragma_value: Some(_),
            } if !["user_version", "application_id"]
                .iter()
                .any(|allowed| pragma_name.eq_ignore_ascii_case(allowed)) =>
            {
                Authorization::Deny
            }
            _ => Authorization::Allow,
        }))?;
        Ok(PrivateDatabase {
            connection: Some(connection),
            path,
            maximum: bytes,
            _reservation: self.clone(),
        })
    }
}

pub struct QuotaWriter {
    file: File,
    known_len: u64,
    reservation: Reservation,
}
impl QuotaWriter {
    pub fn sync_all(&self) -> io::Result<()> {
        self.file.sync_all()
    }
    pub fn set_len(&mut self, length: u64) -> io::Result<()> {
        self.reservation
            .charge(length.saturating_sub(self.known_len))
            .map_err(io::Error::other)?;
        self.file.set_len(length)?;
        self.known_len = self.known_len.max(length);
        Ok(())
    }
}
impl Write for QuotaWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let position = self.file.stream_position()?;
        let end = position
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| io::Error::other("quota offset overflow"))?;
        self.reservation
            .charge(end.saturating_sub(self.known_len))
            .map_err(io::Error::other)?;
        let written = self.file.write(bytes)?;
        self.known_len = self.known_len.max(position + written as u64);
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
impl Seek for QuotaWriter {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file.seek(position)
    }
}

pub struct PrivateDatabase {
    connection: Option<Connection>,
    path: PathBuf,
    maximum: u64,
    _reservation: Reservation,
}
impl PrivateDatabase {
    pub fn connection(&self) -> &Connection {
        self.connection.as_ref().expect("private database open")
    }
    pub fn connection_mut(&mut self) -> &mut Connection {
        self.connection.as_mut().expect("private database open")
    }
    pub fn finish(mut self) -> Result<()> {
        self.connection
            .take()
            .expect("private database open")
            .close()
            .map_err(|(_, error)| error)?;
        let file = OpenOptions::new().read(true).write(true).open(&self.path)?;
        ensure!(
            file.metadata()?.len() <= self.maximum,
            "private SQLite database exceeded allocation"
        );
        file.sync_all()?;
        Ok(())
    }
}
fn staging_path(quota: &Quota, collection: &str, record: &Path) -> Result<PathBuf> {
    crate::validate_collection(collection)?;
    let name = record
        .file_name()
        .and_then(|name| name.to_str())
        .context("quota reservation name invalid")?;
    ensure!(
        name.starts_with("r-")
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
        "invalid reservation staging identity"
    );
    Ok(quota
        .root
        .join("collections")
        .join(collection)
        .join(format!(".quota-stage-{name}")))
}
fn cleanup_staging(quota: &Quota, collection: &str, record: &Path) -> Result<()> {
    if collection.is_empty() {
        return Ok(());
    }
    let stage = staging_path(quota, collection, record)?;
    if !stage.exists() {
        return Ok(());
    }
    for path in [
        quota.root.join("collections"),
        quota.root.join("collections").join(collection),
        stage.clone(),
    ] {
        unlinked(&path)?;
    }
    let mut pending = vec![stage.clone()];
    let mut visited = 0usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            visited += 1;
            ensure!(
                visited <= MAX_ENTRIES,
                "quota cleanup inventory limit exceeded"
            );
            ensure!(
                !entry.file_type()?.is_symlink(),
                "linked private staging state; refusing cleanup"
            );
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    fs::remove_dir_all(stage)?;
    Ok(())
}
impl Drop for ReservationInner {
    fn drop(&mut self) {
        let deadline = self
            .best_effort
            .then(|| Instant::now() + Duration::from_millis(10));
        let Ok(_lock) = self.quota.lock_before(deadline) else {
            return;
        };
        if self.best_effort
            && staging_path(&self.quota, &self.collection, &self.record_path)
                .is_ok_and(|path| path.exists())
        {
            return; // Leave potentially expensive private-tree cleanup to ordinary maintenance.
        }
        if cleanup_staging(&self.quota, &self.collection, &self.record_path).is_err() {
            return;
        }
        if fs::remove_file(&self.record_path).is_ok() {
            drop(self.lease.take());
            let _ = fs::remove_file(&self.lease_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(root_bytes: u64, collection_bytes: u64) -> QuotaPolicy {
        QuotaPolicy {
            root_bytes,
            default_collection_bytes: collection_bytes,
            collections: BTreeMap::new(),
        }
    }

    #[test]
    fn best_effort_admission_does_not_wait_for_a_busy_ledger() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let lock = quota.lock()?;
        let started = Instant::now();
        assert!(
            quota
                .try_reserve("fixture", 1, Duration::from_millis(20))
                .is_err()
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "best effort must not take the normal two-second lock retry path"
        );
        drop(lock);
        assert!(quota.try_reserve("fixture", 1, Duration::ZERO).is_err());
        assert_eq!(quota.status()?.active_reservations, 0);
        Ok(())
    }

    #[test]
    fn exclusive_lifecycle_retries_short_readers_but_shared_entry_fails_fast() -> Result<()> {
        let root = tempfile::tempdir()?;
        let shared = lifecycle_lease(root.path(), false)?;
        let path = root.path().to_owned();
        let started = Arc::new(std::sync::Barrier::new(2));
        let child_started = started.clone();
        let child = std::thread::spawn(move || {
            child_started.wait();
            relocation_lifecycle_lease(&path)
        });
        started.wait();
        // Model the short lifetime of an inherited reader descriptor. No other
        // lock is held by the exclusive waiter while this reader exits.
        std::thread::sleep(Duration::from_millis(25));
        drop(shared);
        let exclusive = child.join().expect("exclusive waiter thread")?;
        let start = Instant::now();
        assert!(lifecycle_lease(root.path(), false).is_err());
        assert!(
            start.elapsed() < Duration::from_millis(200),
            "shared entrants must not use the exclusive retry path"
        );
        drop(exclusive);
        lifecycle_lease(root.path(), false)?;
        Ok(())
    }

    #[test]
    fn root_services_share_global_capacity_without_a_collection_identity() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        quota.configure(policy(100, 10))?;
        let service = quota.reserve_root(90)?;
        service.charge(90)?;
        assert!(service.staging_directory().is_err());
        assert!(
            service
                .create_file(root.path().join("scheduler.sqlite3"))
                .is_err()
        );
        let collection = quota.reserve("fixture", 10)?;
        let report = quota.status()?;
        assert_eq!(report.reserved_bytes, 100);
        assert_eq!(report.collections.len(), 1);
        assert_eq!(report.collections["fixture"].reserved_bytes, 10);
        assert!(quota.reserve_root(1).is_err());
        assert!(quota.configure(policy(99, 10)).is_err());
        drop(service);
        assert_eq!(quota.status()?.reserved_bytes, 10);
        drop(collection);
        Ok(())
    }

    #[test]
    fn reservations_share_root_limits_and_count_retained_and_temporary_bytes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        quota.configure(policy(100, 80))?;
        let old = root.path().join("collections/one/generations/old");
        fs::create_dir_all(&old)?;
        fs::write(old.join("retained"), [0; 20])?;
        fs::write(root.path().join("other-managed-state"), [0; 10])?;
        let first = quota.reserve("one", 50)?;
        assert!(
            Quota::open(root.path().join("."))?
                .reserve("two", 21)
                .is_err()
        );
        assert!(quota.reserve("one", 11).is_err());
        let second = quota.reserve("two", 20)?;
        let report = quota.status()?;
        assert_eq!(
            (
                report.used_bytes,
                report.reserved_bytes,
                report.active_reservations
            ),
            (30, 70, 2)
        );
        assert_eq!(report.collections["one"].used_bytes, 20);
        assert!(
            quota.configure(policy(99, 80)).is_err(),
            "configuration cannot revoke a live allocation"
        );
        drop(first);
        drop(second);
        assert_eq!(quota.status()?.reserved_bytes, 0);
        assert_eq!(
            fs::read(old.join("retained"))?.len(),
            20,
            "retained generations survive reservation cleanup"
        );
        Ok(())
    }

    #[test]
    fn concurrent_provider_growth_cannot_exceed_one_reservation() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        quota.configure(policy(100, 100))?;
        let reservation = quota.reserve("fixture", 10)?;
        let directory = reservation.staging_directory()?;
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let workers: Vec<_> = (0..2)
            .map(|index| {
                let reservation = reservation.clone();
                let destination = directory.join(format!("{index}.bin"));
                let barrier = barrier.clone();
                std::thread::spawn(move || -> Result<bool> {
                    let mut file = reservation.create_file(destination)?;
                    barrier.wait();
                    Ok(file.write_all(&[1; 6]).is_ok())
                })
            })
            .collect();
        barrier.wait();
        let successes = workers
            .into_iter()
            .map(|worker| worker.join().expect("writer thread"))
            .collect::<Result<Vec<_>>>()?;
        assert_eq!(successes.iter().filter(|success| **success).count(), 1);
        assert_eq!(reservation.charged(), 6);
        assert_eq!(quota.status()?.used_bytes, 6);
        drop(reservation);
        assert!(
            !directory.exists(),
            "failed private staging is removed on final guard drop"
        );
        Ok(())
    }

    #[test]
    fn writer_denies_sparse_growth_and_escape_before_touching_source() -> Result<()> {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        fs::write(outside.path().join("source"), b"original")?;
        let quota = Quota::open(root.path())?;
        let reservation = quota.reserve("fixture", 8)?;
        assert!(
            reservation
                .create_file(outside.path().join("source"))
                .is_err()
        );
        let path = reservation.staging_directory()?.join("candidate");
        let mut writer = reservation.create_file(&path)?;
        writer.seek(SeekFrom::Start(8))?;
        assert!(writer.write_all(b"x").is_err());
        assert_eq!(fs::metadata(&path)?.len(), 0);
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(b"accepted")?;
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(b"rewrite!")?;
        assert_eq!(
            reservation.charged(),
            8,
            "overwrites do not consume another file allocation"
        );
        assert_eq!(fs::read(outside.path().join("source"))?, b"original");
        Ok(())
    }

    #[test]
    fn private_sqlite_stops_at_page_cap_and_blocks_budget_escapes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let reservation = quota.reserve("fixture", 16 * 1024)?;
        let path = reservation.staging_directory()?.join("private.sqlite3");
        let database = reservation.open_private_sqlite(&path, 16 * 1024)?;
        let connection = database.connection();
        connection.execute_batch(
            "CREATE TABLE payload(bytes BLOB); INSERT INTO payload VALUES(zeroblob(1000));",
        )?;
        assert!(
            connection
                .execute_batch("INSERT INTO payload VALUES(zeroblob(20000));")
                .is_err()
        );
        assert!(
            connection
                .execute_batch("PRAGMA max_page_count=100000;")
                .is_err()
        );
        assert!(
            connection
                .execute_batch("PRAGMA journal_mode=WAL;")
                .is_err()
        );
        assert!(
            connection
                .execute_batch("ATTACH DATABASE ':memory:' AS escaped;")
                .is_err()
        );
        assert!(fs::metadata(&path)?.len() <= 16 * 1024);
        assert!(!path.with_extension("sqlite3-wal").exists());
        // A SQLITE_FULL private build is disposable; its old published peer is
        // never updated, so no rollback-journal guarantee is assumed here.
        database.finish()?;
        Ok(())
    }

    #[test]
    fn process_exit_releases_reservations_and_cleans_only_owned_staging() -> Result<()> {
        let root = tempfile::tempdir()?;
        let published = root
            .path()
            .join("collections/fixture/generations/published");
        fs::create_dir_all(&published)?;
        fs::write(published.join("evidence"), b"retained")?;
        let result = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "quota::tests::quota_crash_child", "--nocapture"])
            .env("AIRK_QUOTA_CRASH_ROOT", root.path())
            .status()?;
        assert!(result.success());
        let quota = Quota::open(root.path())?;
        let report = quota.status()?;
        assert_eq!(report.active_reservations, 0);
        assert_eq!(report.used_bytes, 8);
        assert_eq!(fs::read(published.join("evidence"))?, b"retained");
        Ok(())
    }

    #[test]
    fn quota_crash_child() -> Result<()> {
        let Some(root) = std::env::var_os("AIRK_QUOTA_CRASH_ROOT") else {
            return Ok(());
        };
        let quota = Quota::open(root)?;
        let reservation = quota.reserve("fixture", 32)?;
        let mut writer =
            reservation.create_file(reservation.staging_directory()?.join("unfinished"))?;
        writer.write_all(b"abandoned build")?;
        writer.sync_all()?;
        std::process::exit(0);
    }

    #[cfg(unix)]
    #[test]
    fn linked_managed_state_and_policy_paths_fail_closed() -> Result<()> {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked"))?;
        assert!(quota.reserve("fixture", 1).is_err());
        fs::remove_file(root.path().join("linked"))?;
        fs::write(
            outside.path().join("policy"),
            serde_json::to_vec(&QuotaPolicy::default())?,
        )?;
        std::os::unix::fs::symlink(
            outside.path().join("policy"),
            root.path().join(".quota/policy.json"),
        )?;
        assert!(quota.policy().is_err());
        fs::remove_file(outside.path().join("policy"))?;
        assert!(
            quota.policy().is_err(),
            "dangling policy links must not select default quotas"
        );
        fs::remove_file(root.path().join(".quota/policy.json"))?;
        fs::remove_file(root.path().join(".quota/ledger.lock"))?;
        let external_lock = outside.path().join("uncreated-lock");
        std::os::unix::fs::symlink(&external_lock, root.path().join(".quota/ledger.lock"))?;
        assert!(quota.status().is_err());
        assert!(
            !external_lock.exists(),
            "ledger locking must not create an external link target"
        );
        Ok(())
    }
}
