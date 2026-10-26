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
pub(crate) const MAX_FILE_BYTES: u64 = 16 << 40;
type GrowthAdmitter = dyn Fn(u64, u64) -> io::Result<u64> + Send + Sync;
#[derive(Clone)]
enum AdmissionPressure {
    Quota(crate::quota::QuotaPressure),
    Physical(crate::capacity::StoragePressure),
}
impl AdmissionPressure {
    fn attach(self, error: anyhow::Error) -> anyhow::Error {
        match self {
            Self::Quota(pressure) => error.context(pressure),
            Self::Physical(pressure) => error.context(pressure),
        }
    }
}

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
fn validate_encoding(bytes: &[u8], maximum: usize) -> Result<()> {
    ensure!(bytes.len() >= 4, "truncated ledger payload");
    let size = u32::from_le_bytes(bytes[..4].try_into()?) as usize;
    ensure!(size <= maximum, "ledger decoded byte budget exceeded");
    ensure!(
        bytes.len() <= maximum.saturating_add(maximum / 255).saturating_add(20),
        "ledger encoded byte budget exceeded"
    );
    Ok(())
}
pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8], maximum: usize) -> Result<T> {
    validate_encoding(bytes, maximum)?;
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
    read_encoded_with(path, kind, capacity, maximum, |encoded, transaction| {
        let state = decode(encoded, maximum)?;
        inspect(state, transaction)
    })
}
/// The caller may reuse a validated decoded value only when these exact,
/// bounded bytes and the independently checked file identity are unchanged.
pub(crate) fn read_encoded_with<R>(
    path: &Path,
    kind: &str,
    capacity: u64,
    maximum: usize,
    inspect: impl FnOnce(&[u8], &redb::ReadTransaction) -> Result<R>,
) -> Result<R> {
    let mut profile = crate::current::kv::profile_start();
    validate_file(path, capacity)?;
    let mut builder = redb::Builder::new();
    builder.set_cache_size(1 << 20);
    let database = builder.open_read_only(path)?;
    crate::current::kv::profile_tick("ledger.read_open", &mut profile);
    let transaction = database.begin_read()?;
    let records = transaction.open_table(RECORDS)?;
    ensure!(
        records
            .get("kind")?
            .is_some_and(|value| value.value() == kind.as_bytes()),
        "incompatible ledger kind"
    );
    let row = records.get("state")?.context("ledger state missing")?;
    validate_encoding(row.value(), maximum)?;
    crate::current::kv::profile_tick("ledger.read_body", &mut profile);
    inspect(row.value(), &transaction)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MutationMode {
    StateOnly,
    TypedTables,
}
type Admission<'a> = (
    Option<&'a crate::quota::Reservation>,
    Option<(&'a crate::quota::Quota, &'a Path)>,
    MutationMode,
);
fn update<T: Serialize + DeserializeOwned, R>(
    file: std::fs::File,
    kind: &str,
    capacity: u64,
    maximum: usize,
    admission: Admission<'_>,
    initial: Option<T>,
    mutate: impl FnOnce(&mut T, &redb::WriteTransaction) -> Result<R>,
) -> Result<R> {
    let (reservation, root_quota, mode) = admission;
    let mut builder = redb::Builder::new();
    builder.set_cache_size(1 << 20);
    let recovery_file = file.try_clone()?;
    let file_length = file.metadata()?.len();
    let initial_capacity = file_length.max(65536);
    ensure!(
        initial_capacity <= capacity,
        "ledger file exceeds admitted capacity"
    );
    let growth_pressure = std::sync::Arc::new(std::sync::Mutex::new(None));
    let high_water = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(initial_capacity));
    let growth_holds = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let admitter: Option<std::sync::Arc<GrowthAdmitter>> = if reservation.is_some()
        || root_quota.is_some()
    {
        let reservation = reservation.cloned();
        let root_quota = root_quota.map(|(quota, path)| (quota.clone(), path.to_path_buf()));
        let holds = growth_holds.clone();
        let pressure_witness = growth_pressure.clone();
        let admitted_witness = high_water.clone();
        Some(std::sync::Arc::new(move |end, admitted| {
            let required = end.saturating_sub(admitted);
            const GRANULE: u64 = 64 << 10;
            let rounded = required
                .checked_add(GRANULE - 1)
                .map(|value| value / GRANULE * GRANULE)
                .unwrap_or(required);
            let admission = (|| -> Result<u64> {
                if let Some((quota, path)) = &root_quota {
                    let grant = quota.reserve_root_available(
                        rounded.min(capacity.saturating_sub(admitted)).max(required),
                    )?;
                    let charge = grant.limit();
                    if charge < required {
                        return Err(crate::quota::QuotaPressure {
                            scope: crate::quota::PressureScope::Root,
                            required_bytes: required,
                            available_bytes: charge,
                            protected_bytes: None,
                        }
                        .into());
                    }
                    grant.charge(charge)?;
                    grant.bind_root_file_growth(path, admitted)?;
                    holds
                        .lock()
                        .map_err(|_| anyhow::anyhow!("ledger growth grants poisoned"))?
                        .push(grant);
                    Ok(charge)
                } else {
                    let reservation = reservation.as_ref().context("ledger admission missing")?;
                    let charge = rounded
                        .min(reservation.remaining())
                        .min(capacity.saturating_sub(admitted))
                        .max(required);
                    reservation.charge(charge)?;
                    Ok(charge)
                }
            })();
            admission
                .map(|charge| {
                    let next = admitted.saturating_add(charge);
                    admitted_witness.fetch_max(next, std::sync::atomic::Ordering::Release);
                    next
                })
                .map_err(|error| {
                    if let Some(pressure) = error.downcast_ref::<crate::quota::QuotaPressure>() {
                        if let Ok(mut witness) = pressure_witness.lock() {
                            *witness = Some(AdmissionPressure::Quota(pressure.clone()));
                        }
                        std::io::Error::other(crate::current::kv::ReservationGrowthDenied(
                            pressure.clone(),
                        ))
                    } else if let Some(pressure) =
                        error.downcast_ref::<crate::capacity::StoragePressure>()
                    {
                        if let Ok(mut witness) = pressure_witness.lock() {
                            *witness = Some(AdmissionPressure::Physical(pressure.clone()));
                        }
                        std::io::Error::other(error)
                    } else {
                        std::io::Error::other(error)
                    }
                })
        }))
    } else {
        None
    };
    let mut profile = crate::current::kv::profile_start();
    let backend = crate::current::kv::AirkBackend::new_growing(
        file,
        initial_capacity,
        capacity,
        admitter.clone(),
    )?;
    let database = builder.create_with_backend(backend).map_err(|error| {
        let error = anyhow::Error::new(error);
        match growth_pressure
            .lock()
            .ok()
            .and_then(|pressure| pressure.clone())
        {
            Some(pressure) => pressure.attach(error),
            None => error,
        }
    })?;
    crate::current::kv::profile_tick("ledger.write_open", &mut profile);
    let outcome = (|| -> Result<R> {
        let mut transaction = database.begin_write()?;
        transaction.set_durability(Durability::Immediate)?;
        transaction.set_two_phase_commit(true);
        let result;
        let mut previous = None;
        let unchanged;
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
                let row = records.get("state")?.context("ledger state missing")?;
                if mode == MutationMode::StateOnly {
                    previous = Some(row.value().to_vec());
                }
                decode(row.value(), maximum)?
            };
            crate::current::kv::profile_tick("ledger.write_decode", &mut profile);
            result = mutate(&mut state, &transaction)?;
            crate::current::kv::profile_tick("ledger.write_mutate", &mut profile);
            let encoded = encode(&state, maximum)?;
            crate::current::kv::profile_tick("ledger.write_encode", &mut profile);
            ensure!(
                encoded.len() as u64 <= capacity,
                "ledger payload exceeds admitted capacity"
            );
            unchanged = previous.as_deref() == Some(encoded.as_slice());
            if !unchanged {
                records.insert("state", encoded.as_slice())?;
            }
        }
        // Only state-only closures cannot have changed another typed table.
        // Comparing the exact stored encoding retains all validation and bounds.
        if unchanged {
            transaction.abort()?;
        } else {
            transaction.commit()?;
        }
        crate::current::kv::profile_tick("ledger.write_commit", &mut profile);
        Ok(result)
    })();
    drop(database);
    crate::current::kv::profile_tick("ledger.write_close", &mut profile);
    if let Err(error) = outcome {
        let error = if let Ok(mut pressure) = growth_pressure.lock()
            && let Some(pressure) = pressure.take()
        {
            pressure.attach(error)
        } else {
            error
        };
        // redb marks the open image as requiring repair after an I/O failure.
        // Reconcile under the same caller-held lifecycle lock and admission so
        // retained read-only opens are usable after rejected growth.
        let recovered_length = recovery_file
            .metadata()?
            .len()
            .max(high_water.load(std::sync::atomic::Ordering::Acquire))
            .max(65536);
        let recovery = builder.create_with_backend(crate::current::kv::AirkBackend::new_growing(
            recovery_file,
            recovered_length,
            capacity,
            admitter,
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
    reservation: &crate::quota::Reservation,
    mutate: impl FnOnce(&mut T) -> Result<R>,
) -> Result<R> {
    validate_file(path, capacity)?;
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    update(
        file,
        kind,
        capacity,
        maximum,
        (Some(reservation), None, MutationMode::StateOnly),
        None,
        |state, _| mutate(state),
    )
}

pub(crate) fn write_with<T: Serialize + DeserializeOwned, R>(
    path: &Path,
    kind: &str,
    capacity: u64,
    maximum: usize,
    reservation: &crate::quota::Reservation,
    mutate: impl FnOnce(&mut T, &redb::WriteTransaction) -> Result<R>,
) -> Result<R> {
    validate_file(path, capacity)?;
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    update(
        file,
        kind,
        capacity,
        maximum,
        (Some(reservation), None, MutationMode::TypedTables),
        None,
        mutate,
    )
}

/// Shared metadata files admit each physical extension directly against root
/// quota. Their decoded record budget is independent of allocator/CoW overhead.
pub(crate) fn write_root_growing<T: Serialize + DeserializeOwned, R>(
    path: &Path,
    kind: &str,
    maximum: usize,
    quota: &crate::quota::Quota,
    mutate: impl FnOnce(&mut T) -> Result<R>,
) -> Result<R> {
    let capacity = MAX_FILE_BYTES;
    validate_file(path, capacity)?;
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    update(
        file,
        kind,
        capacity,
        maximum,
        (None, Some((quota, path)), MutationMode::StateOnly),
        None,
        |state, _| mutate(state),
    )
}

pub(crate) fn create_root_growing<T: Serialize + DeserializeOwned>(
    path: &Path,
    kind: &str,
    maximum: usize,
    quota: &crate::quota::Quota,
    state: T,
) -> Result<()> {
    let seed = quota.reserve_root(65536)?;
    create_admitted(
        path,
        kind,
        MAX_FILE_BYTES,
        maximum,
        &seed,
        Some(quota),
        state,
    )
}

pub(crate) fn create<T: Serialize + DeserializeOwned>(
    path: &Path,
    kind: &str,
    capacity: u64,
    maximum: usize,
    reservation: &crate::quota::Reservation,
    state: T,
) -> Result<()> {
    create_admitted(path, kind, capacity, maximum, reservation, None, state)
}
fn create_admitted<T: Serialize + DeserializeOwned>(
    path: &Path,
    kind: &str,
    capacity: u64,
    maximum: usize,
    reservation: &crate::quota::Reservation,
    root_quota: Option<&crate::quota::Quota>,
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
    if root_quota.is_some() {
        reservation.bind_root_file_growth(temporary.path(), 0)?;
    }
    reservation.charge((empty.len() as u64).max(65536).min(capacity))?;
    temporary.as_file_mut().write_all(empty)?;
    temporary.as_file().sync_all()?;
    update(
        temporary.reopen()?,
        kind,
        capacity,
        maximum,
        (
            if root_quota.is_none() {
                Some(reservation)
            } else {
                None
            },
            root_quota.map(|quota| (quota, temporary.path())),
            MutationMode::StateOnly,
        ),
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
    use crate::quota::Quota;
    use std::collections::BTreeMap;
    #[test]
    fn unchanged_state_does_not_discard_typed_table_mutations() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let reservation = quota.reserve_root(4 << 20)?;
        let path = root.path().join("typed-row.airk");
        create(
            &path,
            "typed-row/1",
            4 << 20,
            1 << 20,
            &reservation,
            Vec::<String>::new(),
        )?;
        const ROWS: TableDefinition<u64, u64> = TableDefinition::new("typed_rows");
        write_with(
            &path,
            "typed-row/1",
            4 << 20,
            1 << 20,
            &reservation,
            |state: &mut Vec<String>, transaction| {
                assert!(state.is_empty());
                transaction.open_table(ROWS)?.insert(7, 9)?;
                Ok(())
            },
        )?;
        read_with(
            &path,
            "typed-row/1",
            4 << 20,
            1 << 20,
            |state: Vec<String>, transaction| {
                assert!(state.is_empty());
                assert_eq!(transaction.open_table(ROWS)?.get(7)?.unwrap().value(), 9);
                Ok(())
            },
        )
    }
    #[test]
    fn physical_headroom_failure_keeps_its_typed_code_across_redb_recovery() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let path = root.path().join("headroom.airk");
        create_root_growing(
            &path,
            "headroom-fixture/1",
            1 << 20,
            &quota,
            Vec::<String>::new(),
        )?;
        let mock = crate::capacity::test_growth_headroom(root.path(), 4096)?;
        let denied = write_root_growing(
            &path,
            "headroom-fixture/1",
            1 << 20,
            &quota,
            |state: &mut Vec<String>| {
                let mut seed = 9u32;
                let text = (0..262144)
                    .map(|_| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        char::from(b'a' + ((seed >> 24) % 26) as u8)
                    })
                    .collect::<String>();
                state.push(text);
                Ok(())
            },
        );
        let error = denied.expect_err("mocked physical headroom must reject real growth");
        assert_eq!(
            crate::capacity::storage_pressure_code(&error),
            Some("physical_storage_headroom"),
            "{error:#}"
        );
        assert!(
            error
                .downcast_ref::<crate::capacity::StoragePressure>()
                .is_some()
        );
        drop(mock);
        let retained: Vec<String> = read(&path, "headroom-fixture/1", MAX_FILE_BYTES, 1 << 20)?;
        assert!(retained.is_empty());
        Ok(())
    }
    #[test]
    fn root_ledger_growth_retries_preserve_state_under_actual_root_limits() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let quota = Quota::open(directory.path())?;
        let mut policy = quota.policy()?;
        policy.root_bytes = 5 << 20;
        quota.configure(policy)?;
        let path = directory.path().join("growing-root.airk");
        create_root_growing(
            &path,
            "root-growth-fixture/1",
            4 << 20,
            &quota,
            Vec::<String>::new(),
        )?;
        let mut seed = 7u32;
        for count in [16, 40, 72] {
            let mut values = Vec::new();
            for _ in 0..count {
                let mut value = String::with_capacity(32768);
                for _ in 0..32768 {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    value.push(char::from(b'a' + ((seed >> 24) % 26) as u8));
                }
                values.push(value);
            }
            let attempt = write_root_growing(
                &path,
                "root-growth-fixture/1",
                4 << 20,
                &quota,
                |state: &mut Vec<String>| {
                    *state = values.clone();
                    Ok(())
                },
            );
            if let Err(error) = attempt {
                let pressure = error
                    .downcast_ref::<crate::quota::QuotaPressure>()
                    .context("expected typed root admission failure")?;
                assert!(matches!(pressure.scope, crate::quota::PressureScope::Root));
                let retained: Vec<String> =
                    read(&path, "root-growth-fixture/1", MAX_FILE_BYTES, 4 << 20)?;
                assert_eq!(
                    retained.len(),
                    match count {
                        16 => 0,
                        40 => 16,
                        _ => 40,
                    }
                );
                let mut policy = quota.policy()?;
                policy.root_bytes += pressure
                    .required_bytes
                    .saturating_sub(pressure.available_bytes)
                    + 65536;
                quota.configure(policy)?;
                write_root_growing(
                    &path,
                    "root-growth-fixture/1",
                    4 << 20,
                    &quota,
                    |state: &mut Vec<String>| {
                        *state = values;
                        Ok(())
                    },
                )?;
            }
            let saved: Vec<String> = read(&path, "root-growth-fixture/1", MAX_FILE_BYTES, 4 << 20)?;
            assert_eq!(saved.len(), count);
            let status = quota.status()?;
            assert_eq!(status.reserved_bytes, 0);
            assert!(status.used_bytes <= status.root_limit_bytes);
        }
        assert!(fs::metadata(&path)?.len() > 2 << 20);
        Ok(())
    }
    #[test]
    fn failed_update_preserves_committed_state_and_read_is_scoped() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("fixture.airk");
        create(
            &path,
            "fixture/1",
            1 << 20,
            65536,
            &Quota::open(directory.path())?.reserve_root(1 << 20)?,
            BTreeMap::from([("value".to_string(), 7u64)]),
        )?;
        let reservation = Quota::open(directory.path())?.reserve_root(1 << 20)?;
        let result: Result<()> = write::<BTreeMap<String, u64>, _>(
            &path,
            "fixture/1",
            1 << 20,
            65536,
            &reservation,
            |state| {
                state.insert("value".into(), 8);
                anyhow::bail!("fixture abort")
            },
        );
        assert!(result.is_err());
        assert_eq!(
            read::<BTreeMap<String, u64>>(&path, "fixture/1", 1 << 20, 65536)?["value"],
            7
        );
        assert!(read::<BTreeMap<String, u64>>(&path, "foreign/1", 1 << 20, 65536).is_err());
        Ok(())
    }

    #[test]
    fn ledger_growth_charges_actual_high_water_within_root_reservation() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let quota = Quota::open(directory.path())?;
        let reservation = quota.reserve_root(1 << 20)?;
        let path = directory.path().join("fixture.airk");
        create(
            &path,
            "growing-fixture/1",
            1 << 20,
            1 << 20,
            &reservation,
            BTreeMap::<String, String>::new(),
        )?;
        let before = fs::metadata(&path)?.len();
        let large = (0..80)
            .map(|index| {
                let mut value = String::with_capacity(2048);
                let mut seed = index as u32 + 1;
                for _ in 0..1024 {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    value.push(char::from(b'a' + ((seed >> 24) % 26) as u8));
                }
                (index.to_string(), value)
            })
            .collect::<BTreeMap<_, _>>();
        write(
            &path,
            "growing-fixture/1",
            1 << 20,
            1 << 20,
            &reservation,
            |state: &mut BTreeMap<String, String>| {
                *state = large;
                Ok(())
            },
        )?;
        let after = fs::metadata(&path)?.len();
        assert!(after > before);
        assert!(after <= 1 << 20);
        assert!(reservation.charged() >= after - before);
        assert!(reservation.charged() < reservation.limit());
        Ok(())
    }
}
