// SPDX-License-Identifier: Apache-2.0
use super::{AirkBackend, CACHE_BYTES, FORMAT, tables};
use anyhow::{Result, ensure};
use redb::{Durability, ReadableDatabase, ReadableTableMetadata, TableHandle};
use std::{
    fs::File,
    io::{self, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub(crate) type CapacityAdmitter = dyn Fn(u64, u64) -> Result<u64> + Send + Sync;

#[derive(Clone)]
enum GrowthPressure {
    Quota(crate::quota::QuotaPressure),
    Physical(crate::capacity::StoragePressure),
}
fn growth_error(
    error: anyhow::Error,
    witness: &Arc<Mutex<Option<GrowthPressure>>>,
) -> anyhow::Error {
    match witness.lock().ok().and_then(|value| value.clone()) {
        Some(GrowthPressure::Quota(value)) => error.context(value),
        Some(GrowthPressure::Physical(value)) => error.context(value),
        None => error,
    }
}

/// A shared worker-owned handle. Snapshots keep it alive through publication/handoff.
pub struct Database {
    inner: Arc<redb::Database>,
    capacity: Arc<AtomicU64>,
    admitter: Arc<Mutex<Option<Arc<CapacityAdmitter>>>>,
    growth_pressure: Arc<Mutex<Option<GrowthPressure>>>,
    poisoned: AtomicBool,
}
pub struct Snapshot {
    transaction: redb::ReadTransaction,
    pub(super) map_buckets: redb::ReadOnlyTable<u16, &'static [u8]>,
    pub(super) map_nodes: redb::ReadOnlyTable<u32, &'static [u8]>,
    _database: Arc<redb::Database>,
}
impl Database {
    /// The caller owns the admission/ownership locks and supplies a validated file.
    /// `initialize` is allowed only for a newly owned empty file.
    pub fn open(file: File, capacity: u64, initialize: bool) -> Result<Self> {
        Self::open_mode(file, capacity, initialize, false, None)
    }
    pub(crate) fn open_with_admitter(
        file: File,
        capacity: u64,
        admitter: Arc<CapacityAdmitter>,
    ) -> Result<Self> {
        Self::open_mode(file, capacity, false, false, Some(admitter))
    }
    /// Requires the owning layer's matching durable, incomplete bootstrap
    /// receipt. This may finish an empty schema, never replace committed state.
    pub(crate) fn resume_bootstrap(file: File, capacity: u64) -> Result<Self> {
        let initialize = file.metadata()?.len() == 0;
        Self::open_mode(file, capacity, initialize, true, None)
    }
    fn open_mode(
        mut file: File,
        capacity: u64,
        initialize: bool,
        resume: bool,
        initial_admitter: Option<Arc<CapacityAdmitter>>,
    ) -> Result<Self> {
        let length = file.metadata()?.len();
        ensure!(
            initialize == (length == 0),
            "redb initialization does not match owned file state"
        );
        if initialize {
            // redb's default empty-file region exceeds small admitted limits.
            // Seed its compact empty image without increasing the user's cap.
            let seed = include_bytes!(concat!(env!("OUT_DIR"), "/empty-ledger.airk"));
            ensure!(
                file.metadata()?.is_file() && capacity >= 65536 && seed.len() as u64 <= capacity,
                "redb bootstrap exceeds admitted capacity"
            );
            file.write_all(seed)?;
            file.sync_all()?;
        }
        let mut builder = redb::Builder::new();
        builder.set_cache_size(CACHE_BYTES);
        let maximum = crate::current::database::MAX_DATABASE_BYTES;
        let current_capacity = Arc::new(AtomicU64::new(capacity));
        let admitter: Arc<Mutex<Option<Arc<CapacityAdmitter>>>> =
            Arc::new(Mutex::new(initial_admitter));
        let callback_admitter = admitter.clone();
        let callback_capacity = current_capacity.clone();
        let growth_pressure = Arc::new(Mutex::new(None));
        let callback_pressure = growth_pressure.clone();
        let backend = AirkBackend::new_growing(
            file,
            capacity,
            maximum,
            Some(Arc::new(move |end, current| {
                let callback = callback_admitter
                    .lock()
                    .map_err(|_| io::Error::other("redb growth callback mutex poisoned"))?
                    .clone()
                    .ok_or_else(|| {
                        io::Error::other(super::CapacityExceeded {
                            requested: end,
                            admitted: current,
                        })
                    })?;
                let admitted = callback(end, current).map_err(|error| {
                    if let Some(pressure) = error.downcast_ref::<crate::quota::QuotaPressure>() {
                        if let Ok(mut witness) = callback_pressure.lock() {
                            *witness = Some(GrowthPressure::Quota(pressure.clone()));
                        }
                        io::Error::other(super::ReservationGrowthDenied(pressure.clone()))
                    } else if let Some(pressure) =
                        error.downcast_ref::<crate::capacity::StoragePressure>()
                    {
                        if let Ok(mut witness) = callback_pressure.lock() {
                            *witness = Some(GrowthPressure::Physical(pressure.clone()));
                        }
                        io::Error::other(error)
                    } else {
                        io::Error::other(error)
                    }
                })?;
                callback_capacity.store(admitted, Ordering::Release);
                Ok(admitted)
            })),
        )?;
        let database = builder
            .create_with_backend(backend)
            .map_err(|error| growth_error(error.into(), &growth_pressure))?;
        let needs_schema = if initialize {
            true
        } else if resume {
            let read = database.begin_read()?;
            match read.open_table(tables::META) {
                Ok(meta) => match meta.get(3)? {
                    Some(value) => {
                        ensure!(
                            value.value() == FORMAT.to_be_bytes(),
                            "foreign bootstrap engine format"
                        );
                        false
                    }
                    None => {
                        ensure!(meta.is_empty()?, "bootstrap metadata has no format marker");
                        true
                    }
                },
                Err(redb::TableError::TableDoesNotExist(_)) => true,
                Err(error) => return Err(error.into()),
            }
        } else {
            false
        };
        if needs_schema {
            let read = database.begin_read()?;
            for table in read.list_tables()? {
                ensure!(
                    matches!(table.name(), "ledger" | "meta"),
                    "bootstrap contains foreign user tables"
                );
            }
            let seed: redb::TableDefinition<&str, &[u8]> = redb::TableDefinition::new("ledger");
            ensure!(
                read.open_table(seed)?.is_empty()?,
                "bootstrap contains foreign ledger state"
            );
            drop(read);
            let mut transaction = database.begin_write()?;
            transaction.set_durability(Durability::Immediate)?;
            transaction.set_two_phase_commit(true);
            {
                let mut meta = transaction.open_table(tables::META)?;
                meta.insert(3, FORMAT.to_be_bytes().as_slice())?;
            }
            transaction.open_table(tables::MAP_BUCKET)?;
            transaction.open_table(tables::MAP_NODE)?;
            transaction.open_table(tables::GRAPH_RECORD)?;
            transaction.open_table(tables::GRAPH_NODE)?;
            transaction.open_table(tables::LEX_TERM)?;
            transaction.open_table(tables::LEX_DIRECTORY)?;
            transaction.open_table(tables::LEX_BLOCK)?;
            transaction.open_table(tables::LEX_DOCUMENT)?;
            transaction.open_table(tables::LEX_PATH)?;
            transaction.open_table(tables::LEX_FORWARD)?;
            transaction.open_table(tables::PENDING)?;
            transaction.commit()?;
        }
        let result = Self {
            inner: Arc::new(database),
            capacity: current_capacity,
            admitter,
            growth_pressure,
            poisoned: AtomicBool::new(false),
        };
        let snapshot = result.snapshot()?;
        let meta = snapshot.transaction.open_table(tables::META)?;
        ensure!(
            meta.get(3)?
                .is_some_and(|value| value.value() == FORMAT.to_be_bytes()),
            "unsupported redb collection format"
        );
        Ok(result)
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        let transaction = self.inner.begin_read()?;
        Ok(Snapshot {
            map_buckets: transaction.open_table(tables::MAP_BUCKET)?,
            map_nodes: transaction.open_table(tables::MAP_NODE)?,
            transaction,
            _database: self.inner.clone(),
        })
    }
    pub(crate) fn admit_capacity(&self, capacity: u64) {
        self.capacity.fetch_max(capacity, Ordering::AcqRel);
    }
    pub(crate) fn capacity(&self) -> u64 {
        self.capacity.load(Ordering::Acquire)
    }
    pub(crate) fn set_capacity_admitter(&self, admitter: Arc<CapacityAdmitter>) -> Result<()> {
        *self
            .admitter
            .lock()
            .map_err(|_| anyhow::anyhow!("redb growth callback mutex poisoned"))? = Some(admitter);
        Ok(())
    }
    pub(crate) fn clear_capacity_admitter(&self) {
        if let Ok(mut admitter) = self.admitter.lock() {
            *admitter = None;
        }
    }
    pub(crate) fn map_growth_error(&self, error: anyhow::Error) -> anyhow::Error {
        if error.chain().any(|cause| {
            cause.is::<std::io::Error>()
                || matches!(
                    cause.downcast_ref::<redb::StorageError>(),
                    Some(redb::StorageError::PreviousIo)
                )
                || matches!(
                    cause.downcast_ref::<redb::Error>(),
                    Some(redb::Error::PreviousIo)
                )
        }) {
            self.poisoned.store(true, Ordering::Release);
        }
        growth_error(error, &self.growth_pressure)
    }
    pub(crate) fn needs_reopen(&self) -> bool {
        self.poisoned.load(Ordering::Acquire)
            || self
                .growth_pressure
                .lock()
                .is_ok_and(|value| value.is_some())
    }
    pub(crate) fn reopen_error(&self) -> anyhow::Error {
        growth_error(
            anyhow::anyhow!(
                "current database requires re-open after failed I/O; drain retained readers"
            ),
            &self.growth_pressure,
        )
    }

    pub fn write(&self) -> Result<redb::WriteTransaction> {
        let mut transaction = self.inner.begin_write()?;
        transaction.set_durability(Durability::Immediate)?;
        transaction.set_two_phase_commit(true);
        Ok(transaction)
    }
    pub(crate) fn idle(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }
    /// Compaction is only possible after every engine wrapper and snapshot has
    /// drained, because redb requires exclusive mutable database ownership.
    pub(crate) fn compact(self) -> Result<bool> {
        let mut database = Arc::try_unwrap(self.inner)
            .map_err(|_| anyhow::anyhow!("redb compaction has retained engine/snapshot handles"))?;
        Ok(database.compact()?)
    }
}
impl Snapshot {
    pub fn transaction(&self) -> &redb::ReadTransaction {
        &self.transaction
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_retains_owner_and_old_values_across_commit() -> Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let database = Database::open(file.reopen()?, 16 << 20, true)?;
        let snapshot = database.snapshot()?;
        let transaction = database.write()?;
        {
            transaction
                .open_table(tables::META)?
                .insert(0, b"next".as_slice())?;
        }
        transaction.commit()?;
        assert!(
            snapshot
                .transaction()
                .open_table(tables::META)?
                .get(0)?
                .is_none()
        );
        let next = database.snapshot()?;
        drop(database);
        assert_eq!(
            next.transaction()
                .open_table(tables::META)?
                .get(0)?
                .unwrap()
                .value(),
            b"next"
        );
        assert!(Database::open(file.reopen()?, 16 << 20, false).is_err());
        drop(next);
        drop(snapshot);
        Database::open(file.reopen()?, 16 << 20, false)?;
        Ok(())
    }
}
