// SPDX-License-Identifier: Apache-2.0
//! Small bounded metadata ledgers. Callers own lifecycle locks and quota admission.
use anyhow::{Context, Result, ensure};
use redb::{Durability, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};

const RECORDS: TableDefinition<&str, &[u8]> = TableDefinition::new("ledger");

fn validate_file(path: &Path, capacity: u64) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() > 0
            && metadata.len() <= capacity,
        "invalid ledger file or capacity"
    );
    crate::managed_collection::reject_file_aliases(path, &metadata)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected ledger file"
        );
    }
    Ok(())
}

struct Bounded {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other(
                "ledger serialization byte budget exceeded",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode<T: Serialize>(state: &T, maximum: usize) -> Result<Vec<u8>> {
    let mut output = Bounded {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut output, state)?;
    Ok(lz4_flex::compress_prepend_size(&output.bytes))
}
fn decode<T: DeserializeOwned>(bytes: &[u8], maximum: usize) -> Result<T> {
    ensure!(bytes.len() >= 4, "truncated ledger payload");
    let size = u32::from_le_bytes(bytes[..4].try_into()?) as usize;
    ensure!(size <= maximum, "ledger decoded byte budget exceeded");
    Ok(serde_json::from_slice(
        &lz4_flex::decompress_size_prepended(bytes)?,
    )?)
}
pub(crate) fn read<T: DeserializeOwned>(
    path: &Path,
    kind: &str,
    capacity: u64,
    maximum: usize,
) -> Result<T> {
    read_with(path, kind, capacity, maximum, |state, _| Ok(state))
}
pub(crate) fn read_with<T: DeserializeOwned, R>(
    path: &Path,
    kind: &str,
    capacity: u64,
    maximum: usize,
    inspect: impl FnOnce(T, &redb::ReadTransaction) -> Result<R>,
) -> Result<R> {
    validate_file(path, capacity)?;
    let mut builder = redb::Builder::new();
    builder.set_cache_size(1 << 20);
    let database = builder.open_read_only(path)?;
    let transaction = database.begin_read()?;
    let records = transaction.open_table(RECORDS)?;
    ensure!(
        records
            .get("kind")?
            .is_some_and(|v| v.value() == kind.as_bytes()),
        "incompatible ledger kind"
    );
    let state = decode(
        records
            .get("state")?
            .context("ledger state missing")?
            .value(),
        maximum,
    )?;
    inspect(state, &transaction)
}

fn update<T: Serialize + DeserializeOwned, R>(
    file: std::fs::File,
    kind: &str,
    capacity: u64,
    maximum: usize,
    initial: Option<T>,
    mutate: impl FnOnce(&mut T, &redb::WriteTransaction) -> Result<R>,
) -> Result<R> {
    let mut builder = redb::Builder::new();
    builder.set_cache_size(1 << 20);
    let recovery_file = file.try_clone()?;
    let database =
        builder.create_with_backend(crate::current::kv::AirkBackend::new(file, capacity)?)?;
    let outcome = (|| -> Result<R> {
        let mut transaction = database.begin_write()?;
        transaction.set_durability(Durability::Immediate)?;
        transaction.set_two_phase_commit(true);
        let result;
        {
            let mut records = transaction.open_table(RECORDS)?;
            let mut state = if let Some(state) = initial {
                ensure!(
                    records.is_empty()?,
                    "ledger initialization would replace existing state"
                );
                records.insert("kind", kind.as_bytes())?;
                state
            } else {
                ensure!(
                    records
                        .get("kind")?
                        .is_some_and(|v| v.value() == kind.as_bytes()),
                    "incompatible ledger kind"
                );
                decode(
                    records
                        .get("state")?
                        .context("ledger state missing")?
                        .value(),
                    maximum,
                )?
            };
            result = mutate(&mut state, &transaction)?;
            let encoded = encode(&state, maximum)?;
            ensure!(
                encoded.len() as u64 <= capacity,
                "ledger payload exceeds admitted capacity"
            );
            records.insert("state", encoded.as_slice())?;
        }
        transaction.commit()?;
        Ok(result)
    })();
    drop(database);
    if let Err(error) = outcome {
        // redb marks the open image as requiring repair after an I/O failure.
        // Reconcile under the same caller-held lifecycle lock and admission so
        // retained read-only opens are usable after rejected growth.
        let recovery = builder.create_with_backend(crate::current::kv::AirkBackend::new(
            recovery_file,
            capacity,
        )?);
        return match recovery {
            Ok(recovered) => {
                drop(recovered);
                Err(error)
            }
            Err(recovery_error) => {
                Err(error.context(format!("ledger recovery remains pending: {recovery_error}")))
            }
        };
    }
    outcome
}

pub(crate) fn write<T: Serialize + DeserializeOwned, R>(
    path: &Path,
    kind: &str,
    capacity: u64,
    maximum: usize,
    mutate: impl FnOnce(&mut T) -> Result<R>,
) -> Result<R> {
    validate_file(path, capacity)?;
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    update(file, kind, capacity, maximum, None, |state, _| {
        mutate(state)
    })
}

pub(crate) fn write_with<T: Serialize + DeserializeOwned, R>(
    path: &Path,
    kind: &str,
    capacity: u64,
    maximum: usize,
    mutate: impl FnOnce(&mut T, &redb::WriteTransaction) -> Result<R>,
) -> Result<R> {
    validate_file(path, capacity)?;
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    update(file, kind, capacity, maximum, None, mutate)
}

pub(crate) fn create<T: Serialize + DeserializeOwned>(
    path: &Path,
    kind: &str,
    capacity: u64,
    maximum: usize,
    state: T,
) -> Result<()> {
    ensure!(!path.try_exists()?, "ledger already exists");
    let parent = path.parent().context("ledger parent missing")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    // redb initially grows to >1 MiB. Use a build-generated compact empty image
    // so small metadata ledgers never exceed their admitted ceiling at creation.
    let empty = include_bytes!(concat!(env!("OUT_DIR"), "/empty-ledger.airk"));
    ensure!(
        empty.len() as u64 <= capacity,
        "empty ledger exceeds admitted capacity"
    );
    temporary.as_file_mut().write_all(empty)?;
    temporary.as_file().sync_all()?;
    update(
        temporary.reopen()?,
        kind,
        capacity,
        maximum,
        Some(state),
        |_, _| Ok(()),
    )?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(path)?;
    crate::maintenance::sync_parent(parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn failed_update_preserves_committed_state_and_read_is_scoped() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("fixture.airk");
        create(
            &path,
            "fixture/1",
            1 << 20,
            65536,
            BTreeMap::from([("value".to_string(), 7u64)]),
        )?;
        let result: Result<()> =
            write::<BTreeMap<String, u64>, _>(&path, "fixture/1", 1 << 20, 65536, |state| {
                state.insert("value".into(), 8);
                anyhow::bail!("fixture abort")
            });
        assert!(result.is_err());
        assert_eq!(
            read::<BTreeMap<String, u64>>(&path, "fixture/1", 1 << 20, 65536)?["value"],
            7
        );
        assert!(read::<BTreeMap<String, u64>>(&path, "foreign/1", 1 << 20, 65536).is_err());
        Ok(())
    }
}
