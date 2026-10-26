// SPDX-License-Identifier: Apache-2.0
use super::{AirkBackend, CACHE_BYTES, FORMAT, tables};
use anyhow::{Result, ensure};
use redb::{Durability, ReadableDatabase};
use std::{fs::File, sync::Arc};

/// A shared worker-owned handle. Snapshots keep it alive through publication/handoff.
pub struct Database {
    inner: Arc<redb::Database>,
}
pub struct Snapshot {
    transaction: redb::ReadTransaction,
    _database: Arc<redb::Database>,
}
impl Database {
    /// The caller owns the admission/ownership locks and supplies a validated file.
    /// `initialize` is allowed only for a newly owned empty file.
    pub fn open(file: File, capacity: u64, initialize: bool) -> Result<Self> {
        let length = file.metadata()?.len();
        ensure!(
            initialize == (length == 0),
            "redb initialization does not match owned file state"
        );
        let mut builder = redb::Builder::new();
        builder.set_cache_size(CACHE_BYTES);
        let database = builder.create_with_backend(AirkBackend::new(file, capacity)?)?;
        if initialize {
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
        Ok(Snapshot {
            transaction: self.inner.begin_read()?,
            _database: self.inner.clone(),
        })
    }
    pub fn write(&self) -> Result<redb::WriteTransaction> {
        let mut transaction = self.inner.begin_write()?;
        transaction.set_durability(Durability::Immediate)?;
        transaction.set_two_phase_commit(true);
        Ok(transaction)
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
