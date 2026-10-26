// SPDX-License-Identifier: Apache-2.0
//! Bounded redb usage aggregates with independent lifecycle and replay protection.
use airk_core::usage::*;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
mod kv;
mod wait;
pub use kv::{UsageStore, clear, feedback};
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
pub fn validate_binding(directory: &Path, root: &str, collection: &str) -> Result<()> {
    let store = crate::FileStore::new(directory);
    if directory.join("service-owner.json").try_exists()? {
        ensure!(
            store.collection_id()? == collection,
            "service usage collection mismatch"
        );
        crate::service::collection::Collection::existing(
            store.managed_path()?.root,
            root,
            collection,
        )?;
        return crate::service::collection::validate_usage_directory(directory);
    }
    if let Some(scope) = store.current_owned_scope()? {
        ensure!(
            scope.root == root && scope.collection == collection,
            "usage collection/root binding mismatch"
        );
        store.current_read(&scope)?;
        return Ok(());
    }
    anyhow::bail!("usage requires fresh format-150 collection setup")
}
/// Retired SQL comparison snapshots are unavailable without reading live sources.
pub fn comparison_snapshot(
    _directory: &Path,
    _root: &str,
    _collection: &str,
) -> Result<Option<airk_core::Snapshot>> {
    Ok(None)
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
    let store = crate::FileStore::new(directory);
    let reservation =
        crate::quota::Quota::for_store(&store)?.reserve(store.collection_id()?, bytes)?;
    reservation.charge(bytes)?;
    Ok(reservation)
}

fn reserve_ledger(
    directory: &Path,
    policy: &UsagePolicy,
    best_effort: bool,
) -> Result<crate::quota::Reservation> {
    let current = fs::metadata(directory.join("usage.airk")).map_or(0, |metadata| metadata.len());
    // Reserve the configured remaining ledger budget. The redb backend charges
    // each newly admitted high-water extent before extending the file.
    let bytes = policy
        .max_collection_bytes
        .saturating_sub(current)
        .max(16_384);
    if best_effort {
        let store = crate::FileStore::new(directory);
        let collection = store.collection_id()?;
        let quota = crate::quota::Quota::for_store(&store)?;
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
        Ok(reservation)
    } else {
        let store = crate::FileStore::new(directory);
        let collection = store.collection_id()?;
        crate::quota::Quota::for_store(&store)?.reserve(collection, bytes)
    }
}

pub fn deletion_guard(collection: &Path) -> Result<UsageLock> {
    lock(collection.join("usage.lock"))
}

fn operation_guard(collection: &Path) -> Result<UsageLock> {
    lock_with_budget(collection.join("usage.lock"), CONTENTION_BUDGET)
}

fn managed(directory: &Path) -> Result<()> {
    let service = directory.join("service-owner.json").try_exists()?;
    if service {
        crate::service::collection::validate_usage_directory(directory)?;
    }
    ensure!(
        !fs::symlink_metadata(directory)?.file_type().is_symlink(),
        "linked collection usage state"
    );
    ensure!(
        service || directory.join("current.airk").is_file(),
        "collection no longer exists"
    );
    for name in ["usage.airk", "usage-policy.json"] {
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
