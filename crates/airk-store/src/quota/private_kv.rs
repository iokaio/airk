// SPDX-License-Identifier: Apache-2.0
//! Reservation-owned disposable redb stores. No published index is opened here.
use super::{PressureScope, QuotaPressure, Reservation};
use anyhow::{Context, Result, ensure};
use redb::{Durability, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    ops::Bound,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy)]
pub(crate) enum PrivateTable {
    Records,
    Nodes,
    Registry,
    GraphIds,
    LexicalPages,
    LexicalDocuments,
    LexicalBlocks,
}
impl PrivateTable {
    pub(crate) fn definition(self) -> TableDefinition<'static, &'static [u8], &'static [u8]> {
        TableDefinition::new(match self {
            Self::Records => "private_records",
            Self::Nodes => "private_nodes",
            Self::Registry => "private_registry",
            Self::GraphIds => "private_graph_ids",
            Self::LexicalPages => "private_lexical_pages",
            Self::LexicalDocuments => "private_lexical_documents",
            Self::LexicalBlocks => "private_lexical_blocks",
        })
    }
    fn key_length(self) -> usize {
        match self {
            Self::Records | Self::GraphIds => 4,
            Self::Nodes => 5,
            Self::Registry | Self::LexicalPages => 32,
            Self::LexicalDocuments => 8,
            Self::LexicalBlocks => 40,
        }
    }
    fn maximum(self) -> usize {
        match self {
            Self::Records => 16384 + 66,
            Self::Nodes | Self::GraphIds => 32,
            Self::Registry => 36,
            Self::LexicalPages | Self::LexicalDocuments | Self::LexicalBlocks => 65537,
        }
    }
    fn validate(self, key: &[u8], value: &[u8]) -> Result<()> {
        ensure!(
            key.len() == self.key_length() && value.len() <= self.maximum(),
            "invalid private redb key/value size"
        );
        Ok(())
    }
}
pub(crate) struct PrivateKv {
    transaction: Option<redb::WriteTransaction>,
    database: redb::Database,
    path: PathBuf,
    maximum: u64,
    denied: std::sync::Arc<std::sync::atomic::AtomicU64>,
    _reservation: Reservation,
}
impl Reservation {
    pub(crate) fn open_private_kv(
        &self,
        path: impl AsRef<Path>,
        maximum: u64,
    ) -> Result<PrivateKv> {
        // Physical allocator overhead grows under the caller's remaining job
        // reservation. The input estimate is not a per-file growth ceiling.
        let physical_maximum = self.remaining().min(16u64 << 40);
        if maximum < 65536 {
            return Err(QuotaPressure {
                scope: PressureScope::Reservation,
                required_bytes: 65536,
                available_bytes: maximum.min(self.remaining()),
                protected_bytes: None,
            }
            .into());
        }
        let path = self.checked_destination(path.as_ref())?;
        let initial = 65536u64;
        self.charge(initial)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        let empty = include_bytes!(concat!(env!("OUT_DIR"), "/empty-ledger.airk"));
        ensure!(
            empty.len() as u64 <= maximum,
            "private bootstrap exceeds reservation"
        );
        file.write_all(empty)?;
        let mut builder = redb::Builder::new();
        builder.set_cache_size(16 << 20);
        let reservation = self.clone();
        let backend = crate::current::kv::AirkBackend::new_growing(
            file,
            initial,
            physical_maximum,
            Some(std::sync::Arc::new(move |requested, admitted| {
                let required = requested.saturating_sub(admitted);
                const GRANULE: u64 = 1 << 20;
                let rounded = required
                    .checked_add(GRANULE - 1)
                    .map(|value| value / GRANULE * GRANULE)
                    .unwrap_or(required);
                let charge = rounded
                    .min(reservation.remaining())
                    .min(physical_maximum.saturating_sub(admitted))
                    .max(required);
                reservation
                    .charge(charge)
                    .map(|()| admitted.saturating_add(charge))
                    .map_err(|error| {
                        if let Some(pressure) = error.downcast_ref::<QuotaPressure>() {
                            std::io::Error::other(crate::current::kv::ReservationGrowthDenied(
                                pressure.clone(),
                            ))
                        } else {
                            std::io::Error::other(error)
                        }
                    })
            })),
        )?;
        let denied = backend.denial_witness();
        let database = builder.create_with_backend(backend)?;
        let mut transaction = database.begin_write()?;
        transaction.set_durability(Durability::None)?;
        for table in [
            PrivateTable::Records,
            PrivateTable::Nodes,
            PrivateTable::Registry,
            PrivateTable::GraphIds,
            PrivateTable::LexicalPages,
            PrivateTable::LexicalDocuments,
            PrivateTable::LexicalBlocks,
        ] {
            transaction.open_table(table.definition())?;
        }
        Ok(PrivateKv {
            transaction: Some(transaction),
            database,
            path,
            maximum: physical_maximum,
            denied,
            _reservation: self.clone(),
        })
    }
}
fn get_from(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    role: PrivateTable,
    key: &[u8],
) -> Result<Option<Vec<u8>>> {
    table
        .get(key)?
        .map(|value| {
            role.validate(key, value.value())?;
            Ok(value.value().to_vec())
        })
        .transpose()
}
fn page_from(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    role: PrivateTable,
    after: Option<&[u8]>,
    limit: usize,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    ensure!(
        (1..=64).contains(&limit),
        "private redb page bound exceeded"
    );
    let range = (
        after.map_or(Bound::Unbounded, Bound::Excluded),
        Bound::<&[u8]>::Unbounded,
    );
    let mut result = Vec::new();
    for row in table.range::<&[u8]>(range)?.take(limit) {
        let (key, value) = row?;
        role.validate(key.value(), value.value())?;
        result.push((key.value().to_vec(), value.value().to_vec()));
    }
    Ok(result)
}
impl PrivateKv {
    pub(crate) fn remove(&self, table: PrivateTable, key: &[u8]) -> Result<()> {
        ensure!(
            key.len() == table.key_length(),
            "invalid private redb key size"
        );
        self.denied.store(0, std::sync::atomic::Ordering::Relaxed);
        self.transaction
            .as_ref()
            .context("private redb store is sealed")?
            .open_table(table.definition())?
            .remove(key)
            .map_err(|e| self.write_error(e.into()))?;
        Ok(())
    }
    pub(crate) fn get(&self, table: PrivateTable, key: &[u8]) -> Result<Option<Vec<u8>>> {
        ensure!(
            key.len() == table.key_length(),
            "invalid private redb key size"
        );
        match &self.transaction {
            Some(transaction) => get_from(&transaction.open_table(table.definition())?, table, key),
            None => get_from(
                &self.database.begin_read()?.open_table(table.definition())?,
                table,
                key,
            ),
        }
    }
    pub(crate) fn page(
        &self,
        table: PrivateTable,
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        match &self.transaction {
            Some(transaction) => page_from(
                &transaction.open_table(table.definition())?,
                table,
                after,
                limit,
            ),
            None => page_from(
                &self.database.begin_read()?.open_table(table.definition())?,
                table,
                after,
                limit,
            ),
        }
    }
    pub(crate) fn len(&self, table: PrivateTable) -> Result<u64> {
        Ok(match &self.transaction {
            Some(transaction) => transaction.open_table(table.definition())?.len()?,
            None => self
                .database
                .begin_read()?
                .open_table(table.definition())?
                .len()?,
        })
    }
    pub(crate) fn put(&self, table: PrivateTable, key: &[u8], value: &[u8]) -> Result<bool> {
        table.validate(key, value)?;
        self.denied.store(0, std::sync::atomic::Ordering::Relaxed);
        let transaction = self
            .transaction
            .as_ref()
            .context("private redb store is sealed")?;
        let mut rows = transaction.open_table(table.definition())?;
        let inserted = rows
            .insert(key, value)
            .map_err(|error| self.write_error(error.into()))?
            .is_none();
        Ok(inserted)
    }
    pub(crate) fn seal(&mut self) -> Result<()> {
        let mut transaction = self
            .transaction
            .take()
            .context("private redb store already sealed")?;
        transaction.set_durability(Durability::Immediate)?;
        transaction.set_two_phase_commit(true);
        transaction
            .commit()
            .map_err(|error| self.write_error(error.into()))?;
        Ok(())
    }
    pub(crate) fn bytes(&self) -> Result<u64> {
        let bytes = fs::metadata(&self.path)?.len();
        ensure!(
            bytes <= self.maximum,
            "private redb exceeded admitted capacity"
        );
        Ok(bytes)
    }
    pub(crate) fn write_error(&self, error: anyhow::Error) -> anyhow::Error {
        let admitted = error.chain().any(|cause| {
            cause
                .downcast_ref::<std::io::Error>()
                .and_then(|io| io.get_ref())
                .is_some_and(|inner| inner.is::<crate::current::kv::CapacityExceeded>())
                || cause.is::<crate::current::kv::CapacityExceeded>()
        });
        let denied = self.denied.load(std::sync::atomic::Ordering::Relaxed);
        if denied & (1u64 << 63) != 0 {
            error.context(QuotaPressure {
                scope: PressureScope::Reservation,
                required_bytes: denied & !(1u64 << 63),
                available_bytes: self._reservation.remaining(),
                protected_bytes: None,
            })
        } else if error.downcast_ref::<QuotaPressure>().is_some() {
            error
        } else if admitted || denied > self.maximum {
            error.context(QuotaPressure {
                scope: PressureScope::Reservation,
                required_bytes: denied.max(self.maximum.saturating_add(4096)),
                available_bytes: self.maximum,
                protected_bytes: None,
            })
        } else {
            error
        }
    }
    #[cfg(test)]
    pub(crate) fn physical_maximum(&self) -> u64 {
        self.maximum
    }
    #[cfg(test)]
    pub(crate) fn transaction(&self) -> &redb::WriteTransaction {
        self.transaction.as_ref().expect("private transaction open")
    }
    #[cfg(test)]
    pub(crate) fn database(&self) -> &redb::Database {
        &self.database
    }
}
