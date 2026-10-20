// SPDX-License-Identifier: Apache-2.0
//! Disposable exact-input graph lookup, owned by one publication reservation.
//!
//! DiskANN remains the vector engine. This private SQLite table only remembers
//! which inputs this build has already evaluated; it is never a published index
//! or a reusable cache. Closing the reservation removes it, including after a
//! crashed owner. A single uncommitted transaction avoids per-input commits.
use crate::quota::{PressureScope, PrivateDatabase, QuotaPressure, Reservation};
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    vector_selection::{PreparedVectorNode, VectorNodeRegistry},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use std::sync::Mutex;

pub const MIN_REGISTRY_BYTES: u64 = 16 * 1024;
pub const MAX_REGISTRY_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_REGISTRY_ROWS: u64 = 1_000_000;
const FILE_NAME: &str = "vector-registry.sqlite3";

#[derive(Clone, Copy, Debug, Serialize)]
pub struct VectorRegistryStatistics {
    pub rows: u64,
    /// Allocated SQLite pages, including dirty pages; not physical device I/O.
    pub bytes: u64,
    pub maximum_bytes: u64,
}

struct State {
    database: PrivateDatabase,
    rows: u64,
    failed: bool,
}

pub struct VectorRegistry {
    state: Mutex<State>,
    scope: Sha256Digest,
    maximum_bytes: u64,
    maximum_rows: u64,
}

impl VectorRegistry {
    /// The caller holds the collection writer and chooses a share of its
    /// publication reservation. All pages are charged before the file is opened;
    /// the existing private-database adapter bounds its page cache to 16 MiB.
    /// A reservation can own only one registry. Existing files are never adopted.
    pub fn new(reservation: &Reservation, maximum_bytes: u64) -> Result<Self> {
        Self::with_limits(reservation, maximum_bytes, MAX_REGISTRY_ROWS)
    }

    fn with_limits(
        reservation: &Reservation,
        maximum_bytes: u64,
        maximum_rows: u64,
    ) -> Result<Self> {
        ensure!(
            maximum_bytes <= MAX_REGISTRY_BYTES && (1..=MAX_REGISTRY_ROWS).contains(&maximum_rows),
            "vector registry allocation exceeds its bound"
        );
        if maximum_bytes < MIN_REGISTRY_BYTES {
            return Err(QuotaPressure {
                scope: PressureScope::Reservation,
                required_bytes: MIN_REGISTRY_BYTES,
                available_bytes: maximum_bytes.min(reservation.remaining()),
                protected_bytes: None,
            }
            .into());
        }
        let maximum_bytes = maximum_bytes / 4096 * 4096;
        let path = reservation.staging_directory()?.join(FILE_NAME);
        let scope = framed_digest(
            b"private-vector-registry-scope/1",
            &[path.to_str().context("non-UTF-8 registry path")?.as_bytes()],
        );
        let database = reservation.open_private_sqlite(path, maximum_bytes)?;
        database
            .connection()
            .execute_batch(
                "CREATE TABLE inputs (
                    key BLOB PRIMARY KEY CHECK(typeof(key)='blob' AND length(key)=32),
                    graph_id INTEGER UNIQUE CHECK(graph_id IS NULL OR
                        (typeof(graph_id)='integer' AND graph_id BETWEEN 1 AND 4294967295)),
                    checksum BLOB NOT NULL CHECK(typeof(checksum)='blob' AND length(checksum)=32)
                ) WITHOUT ROWID;
                BEGIN IMMEDIATE;",
            )
            .map_err(|error| database.write_error(error.into()))?;
        Ok(Self {
            state: Mutex::new(State {
                database,
                rows: 0,
                failed: false,
            }),
            scope,
            maximum_bytes,
            maximum_rows,
        })
    }

    fn checksum(&self, key: Sha256Digest, node: PreparedVectorNode) -> Sha256Digest {
        framed_digest(
            b"private-vector-registry-entry/1",
            &[
                self.scope.as_bytes(),
                key.as_bytes(),
                &node.graph_id.unwrap_or(0).to_be_bytes(),
            ],
        )
    }

    fn with_state<T>(&self, operation: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("vector registry lock poisoned"))?;
        ensure!(!state.failed, "vector registry failed; discard this build");
        let result = operation(&mut state);
        if result.is_err() {
            // Journal-free private writes need not be recoverable after failure.
            // Never turn a damaged registry into cache misses or continue writes.
            state.failed = true;
        }
        result
    }

    fn read(&self, state: &State, key: Sha256Digest) -> Result<Option<PreparedVectorNode>> {
        let stored = state
            .database
            .connection()
            .prepare_cached("SELECT graph_id,checksum FROM inputs WHERE key=?1")?
            .query_row([key.as_bytes().as_slice()], |row| {
                Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, [u8; 32]>(1)?))
            })
            .optional()?;
        let Some((graph_id, checksum)) = stored else {
            return Ok(None);
        };
        let node = PreparedVectorNode {
            graph_id: graph_id.map(u32::try_from).transpose()?,
        };
        ensure!(
            node.graph_id != Some(0),
            "invalid vector registry node zero"
        );
        ensure!(
            self.checksum(key, node).as_bytes() == &checksum,
            "vector registry entry checksum mismatch"
        );
        Ok(Some(node))
    }

    pub fn statistics(&self) -> Result<VectorRegistryStatistics> {
        self.with_state(|state| {
            let pages: u32 =
                state
                    .database
                    .connection()
                    .pragma_query_value(None, "page_count", |row| row.get(0))?;
            let bytes = u64::from(pages) * 4096;
            ensure!(
                bytes <= self.maximum_bytes,
                "vector registry exceeded page bound"
            );
            Ok(VectorRegistryStatistics {
                rows: state.rows,
                bytes,
                maximum_bytes: self.maximum_bytes,
            })
        })
    }
}

impl VectorNodeRegistry for VectorRegistry {
    fn lookup(&self, key: Sha256Digest) -> Result<Option<PreparedVectorNode>> {
        self.with_state(|state| self.read(state, key))
    }

    fn remember(&self, key: Sha256Digest, node: PreparedVectorNode) -> Result<()> {
        ensure!(
            node.graph_id != Some(0),
            "vector registry node zero is reserved"
        );
        self.with_state(|state| {
            if let Some(previous) = self.read(state, key)? {
                ensure!(previous == node, "vector registry input outcome changed");
                return Ok(());
            }
            ensure!(
                state.rows < self.maximum_rows,
                "vector registry row budget exceeded"
            );
            let checksum = self.checksum(key, node);
            state
                .database
                .connection()
                .execute(
                    "INSERT INTO inputs(key,graph_id,checksum) VALUES(?1,?2,?3)",
                    params![
                        key.as_bytes().as_slice(),
                        node.graph_id.map(i64::from),
                        checksum.as_bytes().as_slice(),
                    ],
                )
                .map_err(|error| state.database.write_error(error.into()))?;
            state.rows += 1;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::Quota;
    use std::fs;

    fn key(value: u32) -> Sha256Digest {
        framed_digest(b"registry-original-fixture/1", &[&value.to_be_bytes()])
    }

    #[test]
    fn exact_keys_keep_positive_and_no_vector_outcomes_and_reject_changes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let reservation = quota.reserve("fixture", 128 * 1024)?;
        let registry = VectorRegistry::new(&reservation, 128 * 1024)?;
        let positive = PreparedVectorNode { graph_id: Some(1) };
        let empty = PreparedVectorNode { graph_id: None };
        assert_eq!(registry.lookup(key(1))?, None);
        registry.remember(key(1), positive)?;
        registry.remember(key(2), empty)?;
        registry.remember(key(3), empty)?;
        registry.remember(key(1), positive)?;
        registry.remember(key(2), empty)?;
        assert_eq!(registry.lookup(key(1))?, Some(positive));
        assert_eq!(registry.lookup(key(2))?, Some(empty));
        assert_eq!(registry.lookup(key(4))?, None);
        assert_eq!(registry.statistics()?.rows, 3);
        assert_eq!(reservation.charged(), 128 * 1024);
        assert!(registry.remember(key(1), empty).is_err());
        assert!(registry.lookup(key(1)).is_err());
        Ok(())
    }

    #[test]
    fn zero_and_reusing_a_graph_id_for_another_key_are_rejected() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let reservation = quota.reserve("fixture", 128 * 1024)?;
        let registry = VectorRegistry::new(&reservation, 128 * 1024)?;
        assert!(
            registry
                .remember(key(1), PreparedVectorNode { graph_id: Some(0) })
                .is_err()
        );
        let positive = PreparedVectorNode {
            graph_id: Some(u32::MAX),
        };
        registry.remember(key(1), positive)?;
        assert_eq!(registry.lookup(key(1))?, Some(positive));
        assert!(registry.remember(key(2), positive).is_err());
        assert!(registry.lookup(key(1)).is_err());
        Ok(())
    }

    #[test]
    fn row_limits_disk_limits_and_quota_fail_closed() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let reservation = quota.reserve("rows", 128 * 1024)?;
        let registry = VectorRegistry::with_limits(&reservation, 128 * 1024, 2)?;
        let empty = PreparedVectorNode { graph_id: None };
        registry.remember(key(1), empty)?;
        registry.remember(key(2), empty)?;
        registry.remember(key(2), empty)?;
        assert_eq!(registry.statistics()?.rows, 2);
        assert!(registry.remember(key(3), empty).is_err());
        assert!(registry.lookup(key(1)).is_err());

        let reservation = quota.reserve("pages", MIN_REGISTRY_BYTES)?;
        let registry = VectorRegistry::new(&reservation, MIN_REGISTRY_BYTES)?;
        let error = (0..1000)
            .find_map(|value| registry.remember(key(value), empty).err())
            .context("tiny page cap should be exhausted")?;
        assert!(
            error.downcast_ref::<QuotaPressure>().is_some()
                || matches!(
                    error.downcast_ref::<rusqlite::Error>(),
                    Some(rusqlite::Error::SqliteFailure(failure, _))
                        if failure.code == rusqlite::ErrorCode::DiskFull
                ),
            "{error:#}"
        );
        assert!(registry.lookup(key(0)).is_err());
        assert!(
            fs::metadata(reservation.staging_directory()?.join(FILE_NAME))?.len()
                <= MIN_REGISTRY_BYTES
        );
        let insufficient = quota.reserve("denied", MIN_REGISTRY_BYTES - 1)?;
        let error = VectorRegistry::new(&insufficient, MIN_REGISTRY_BYTES)
            .err()
            .context("quota rejection missing")?;
        assert!(error.downcast_ref::<QuotaPressure>().is_some());
        assert!(!insufficient.staging_directory()?.join(FILE_NAME).exists());
        assert!(VectorRegistry::new(&insufficient, MAX_REGISTRY_BYTES + 1).is_err());
        Ok(())
    }

    #[test]
    fn corrupt_records_are_errors_without_unbounded_blob_decoding() -> Result<()> {
        for oversized in [false, true] {
            let root = tempfile::tempdir()?;
            let quota = Quota::open(root.path())?;
            let reservation = quota.reserve("fixture", 128 * 1024)?;
            let registry = VectorRegistry::new(&reservation, 128 * 1024)?;
            registry.remember(key(1), PreparedVectorNode { graph_id: Some(1) })?;
            {
                let state = registry.state.lock().unwrap();
                let connection = state.database.connection();
                if oversized {
                    // Replace the private table using the owning connection to
                    // simulate corrupt storage bypassing its normal constraints.
                    connection.execute_batch("DROP TABLE inputs; CREATE TABLE inputs(key BLOB,graph_id INTEGER,checksum BLOB);")?;
                    connection.execute(
                        "INSERT INTO inputs VALUES(?1,1,zeroblob(65536))",
                        [key(1).as_bytes().as_slice()],
                    )?;
                } else {
                    connection.execute(
                        "UPDATE inputs SET checksum=zeroblob(32) WHERE key=?1",
                        [key(1).as_bytes().as_slice()],
                    )?;
                }
            }
            assert!(registry.lookup(key(1)).is_err());
            assert!(registry.lookup(key(2)).is_err());
        }
        Ok(())
    }

    #[test]
    fn reservations_isolate_collections_and_keep_staging_until_handles_close() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let first = quota.reserve("one", 128 * 1024)?;
        let second = quota.reserve("two", 128 * 1024)?;
        let first_stage = first.staging_directory()?;
        let second_stage = second.staging_directory()?;
        let one = VectorRegistry::new(&first, 128 * 1024)?;
        let two = VectorRegistry::new(&second, 128 * 1024)?;
        one.remember(key(1), PreparedVectorNode { graph_id: Some(7) })?;
        assert_eq!(two.lookup(key(1))?, None);
        two.remember(key(1), PreparedVectorNode { graph_id: Some(8) })?;
        assert_eq!(one.lookup(key(1))?.unwrap().graph_id, Some(7));
        assert_eq!(two.lookup(key(1))?.unwrap().graph_id, Some(8));
        drop(first);
        assert!(
            first_stage.exists(),
            "open registry keeps its reservation alive"
        );
        drop(one);
        assert!(!first_stage.exists());
        assert!(second_stage.exists());
        drop(two);
        drop(second);
        assert!(!second_stage.exists());
        assert_eq!(quota.status()?.active_reservations, 0);
        Ok(())
    }

    #[test]
    fn transaction_stays_private_and_cache_page_escape_is_blocked() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let reservation = quota.reserve("fixture", 128 * 1024)?;
        let registry = VectorRegistry::new(&reservation, 128 * 1024)?;
        registry.remember(key(1), PreparedVectorNode { graph_id: Some(1) })?;
        let state = registry.state.lock().unwrap();
        let connection = state.database.connection();
        assert!(!connection.is_autocommit());
        let cache: i64 = connection.pragma_query_value(None, "cache_size", |row| row.get(0))?;
        assert_eq!(cache, -16384);
        assert!(
            connection
                .execute_batch("PRAGMA max_page_count=1000000")
                .is_err()
        );
        assert!(
            connection
                .execute_batch("ATTACH ':memory:' AS escaped")
                .is_err()
        );
        let other = rusqlite::Connection::open(reservation.staging_directory()?.join(FILE_NAME))?;
        other.busy_timeout(std::time::Duration::ZERO)?;
        assert!(other.execute("DELETE FROM inputs", []).is_err());
        assert_eq!(
            other.query_row("SELECT COUNT(*) FROM inputs", [], |row| row
                .get::<_, i64>(0))?,
            0
        );
        Ok(())
    }

    #[test]
    fn dead_owner_registry_is_reclaimed_without_touching_published_files() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let collection = root.path().join("collections-1.2/fixture");
        fs::create_dir_all(&collection)?;
        fs::write(collection.join("published-evidence"), b"original evidence")?;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "vector_registry::tests::crash_registry_child",
                "--nocapture",
            ])
            .env("AIRK_VECTOR_REGISTRY_CRASH_ROOT", root.path())
            .output()?;
        ensure!(
            output.status.success(),
            "registry child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stages = fs::read_dir(&collection)?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".quota-stage-")
            })
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        assert_eq!(stages.len(), 1);
        assert!(stages[0].join(FILE_NAME).exists());
        assert_eq!(quota.status()?.active_reservations, 0);
        assert!(!stages[0].exists());
        assert_eq!(
            fs::read(collection.join("published-evidence"))?,
            b"original evidence"
        );
        Ok(())
    }

    #[test]
    fn crash_registry_child() -> Result<()> {
        let Some(root) = std::env::var_os("AIRK_VECTOR_REGISTRY_CRASH_ROOT") else {
            return Ok(());
        };
        let quota = Quota::open(root)?;
        let reservation = quota.reserve("fixture", 128 * 1024)?;
        let registry = VectorRegistry::new(&reservation, 128 * 1024)?;
        registry.remember(key(1), PreparedVectorNode { graph_id: Some(1) })?;
        assert_eq!(registry.lookup(key(1))?.unwrap().graph_id, Some(1));
        // A process exit must not require rolling this private transaction forward.
        std::process::exit(0);
    }
}
