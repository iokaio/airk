// SPDX-License-Identifier: Apache-2.0
//! Disposable exact-input graph lookup, owned by one publication reservation.
//!
//! DiskANN remains the vector engine. This private redb store only remembers
//! which inputs this build has already evaluated; it is never a published index
//! or a reusable cache. Closing the reservation removes it, including after a
//! crashed owner. A single uncommitted transaction avoids per-input commits.
use crate::quota::{PressureScope, PrivateKv, PrivateTable, QuotaPressure, Reservation};
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    vector_selection::{PreparedVectorNode, VectorNodeRegistry},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::sync::Mutex;

pub const MIN_REGISTRY_BYTES: u64 = 64 * 1024;
pub const MAX_REGISTRY_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_REGISTRY_ROWS: u64 = 1_000_000;
const FILE_NAME: &str = "vector-registry.airk";

#[derive(Clone, Copy, Debug, Serialize)]
pub struct VectorRegistryStatistics {
    pub rows: u64,
    /// Private file bytes, not physical device I/O.
    pub bytes: u64,
    pub maximum_bytes: u64,
}

struct State {
    database: PrivateKv,
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
        let database = reservation.open_private_kv(path, maximum_bytes)?;
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
        let stored = state.database.get(PrivateTable::Registry, key.as_bytes())?;
        let Some(bytes) = stored else {
            return Ok(None);
        };
        ensure!(bytes.len() == 36, "invalid vector registry record length");
        let graph_id = u32::from_be_bytes(bytes[..4].try_into()?);
        let checksum: [u8; 32] = bytes[4..].try_into()?;
        let node = PreparedVectorNode {
            graph_id: (graph_id != 0).then_some(graph_id),
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
            let bytes = state.database.bytes()?;
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
            if let Some(id) = node.graph_id {
                ensure!(
                    state
                        .database
                        .get(PrivateTable::GraphIds, &id.to_be_bytes())?
                        .is_none(),
                    "vector graph ID already belongs to another input"
                );
                state
                    .database
                    .put(PrivateTable::GraphIds, &id.to_be_bytes(), key.as_bytes())?;
            }
            let mut bytes = node.graph_id.unwrap_or(0).to_be_bytes().to_vec();
            bytes.extend_from_slice(checksum.as_bytes());
            state
                .database
                .put(PrivateTable::Registry, key.as_bytes(), &bytes)?;
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
        assert!(
            reservation.charged()
                >= fs::metadata(reservation.staging_directory()?.join(FILE_NAME))?.len()
        );
        assert!(reservation.charged() <= reservation.limit());
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
        assert!(error.downcast_ref::<QuotaPressure>().is_some(), "{error:#}");
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
                let mut rows = state
                    .database
                    .transaction()
                    .open_table(PrivateTable::Registry.definition())?;
                let mut bytes = 1u32.to_be_bytes().to_vec();
                bytes.extend_from_slice(&[0; 32]);
                if oversized {
                    bytes.resize(4096, 0);
                }
                rows.insert(key(1).as_bytes().as_slice(), bytes.as_slice())?;
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
    fn uncommitted_registry_is_private_and_independent_openers_are_excluded() -> Result<()> {
        use redb::ReadableDatabase;
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let reservation = quota.reserve("fixture", 128 * 1024)?;
        let registry = VectorRegistry::new(&reservation, 128 * 1024)?;
        registry.remember(key(1), PreparedVectorNode { graph_id: Some(1) })?;
        let state = registry.state.lock().unwrap();
        assert!(
            state
                .database
                .database()
                .begin_read()?
                .open_table(PrivateTable::Registry.definition())
                .is_err()
        );
        let path = reservation.staging_directory()?.join(FILE_NAME);
        let backend = crate::current::kv::AirkBackend::new(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)?,
            128 * 1024,
        )?;
        assert!(redb::Builder::new().create_with_backend(backend).is_err());
        let observer = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)?;
        assert!(matches!(
            observer.try_lock_shared(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert_eq!(registry.read(&state, key(1))?.unwrap().graph_id, Some(1));
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
