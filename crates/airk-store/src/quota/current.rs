// SPDX-License-Identifier: Apache-2.0
//! Crash-surviving current-publication allowances. Never recover a collection under ledger.lock.
use super::*;
use airk_core::identity::{Incarnation, Sha256Digest, framed_digest};
use std::io::Write;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentGrantScope {
    pub collection: String,
    pub incarnation: Incarnation,
    pub transaction: Sha256Digest,
}

impl CurrentGrantScope {
    fn filename(&self) -> String {
        let digest = framed_digest(
            b"current-quota-grant/1",
            &[
                self.collection.as_bytes(),
                self.incarnation.as_bytes(),
                self.transaction.as_bytes(),
            ],
        );
        format!("g-{digest}.json")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableRecord {
    version: u32,
    scope: CurrentGrantScope,
    bytes: u64,
}

/// Dropping a handle intentionally preserves its allowance. Only the owning
/// collection writer, after checking its journal under the exclusive gate, retires it.
pub struct CurrentGrant {
    quota: Quota,
    record: DurableRecord,
}

impl CurrentGrant {
    pub fn scope(&self) -> &CurrentGrantScope {
        &self.record.scope
    }
    pub fn capacity(&self) -> u64 {
        self.record.bytes
    }

    /// Replace the outstanding phase allowance. Already written database/object
    /// bytes are measured separately; callers supply remaining growth plus scratch.
    pub fn resize(&mut self, writer: &crate::WriterLock, bytes: u64) -> Result<()> {
        validate_writer(&self.quota, writer, &self.record.scope.collection)?;
        ensure!(bytes > 0, "current quota grant must be positive");
        let _lock = self.quota.lock()?;
        self.require_current()?;
        if bytes > self.record.bytes {
            admit(
                &self.quota,
                &self.record.scope.collection,
                bytes - self.record.bytes,
                false,
            )?;
        }
        let mut next = self.record.clone();
        next.bytes = bytes;
        persist(&self.quota, &next, false)?;
        self.record = next;
        Ok(())
    }

    /// Requires the caller's exclusive current gate and a settled/absent pending
    /// transaction. No active SQLite transaction or connection mutex may be held here.
    pub fn release(self, writer: &crate::WriterLock) -> Result<()> {
        validate_writer(&self.quota, writer, &self.record.scope.collection)?;
        let _lock = self.quota.lock()?;
        self.require_current()?;
        let directory = self.quota.root.join(".quota-1.2/current-grants");
        fs::remove_file(directory.join(self.record.scope.filename()))?;
        crate::maintenance::sync_parent(&directory)?;
        Ok(())
    }

    fn require_current(&self) -> Result<()> {
        let path = self
            .quota
            .root
            .join(".quota-1.2/current-grants")
            .join(self.record.scope.filename());
        let current = read_record(&path)?;
        ensure!(
            current == self.record,
            "current quota grant changed; reacquire it under the writer"
        );
        Ok(())
    }
}

impl Quota {
    pub(crate) fn verify_current_allowance(
        &self,
        scope: &CurrentGrantScope,
        minimum: u64,
    ) -> Result<()> {
        let _lock = self.lock()?;
        let record = read_record(
            &self
                .root
                .join(".quota-1.2/current-grants")
                .join(scope.filename()),
        )?;
        ensure!(
            record.scope == *scope && record.bytes >= minimum,
            "missing or insufficient service WAL recovery allowance"
        );
        Ok(())
    }
    /// Call under lifecycle -> collection writer -> exclusive current gate. The
    /// ledger lock is released before callers start any SQLite transaction.
    pub fn reserve_current(
        &self,
        writer: &crate::WriterLock,
        scope: CurrentGrantScope,
        bytes: u64,
    ) -> Result<CurrentGrant> {
        validate_writer(self, writer, &scope.collection)?;
        ensure!(bytes > 0, "current quota grant must be positive");
        let _lock = self.lock()?;
        let path = self
            .root
            .join(".quota-1.2/current-grants")
            .join(scope.filename());
        ensure!(
            !path.try_exists()?,
            "current quota grant already exists; adopt it for recovery"
        );
        admit(self, &scope.collection, bytes, true)?;
        let record = DurableRecord {
            version: 1,
            scope,
            bytes,
        };
        persist(self, &record, true)?;
        Ok(CurrentGrant {
            quota: self.clone(),
            record,
        })
    }

    pub fn current_grant(
        &self,
        writer: &crate::WriterLock,
        scope: &CurrentGrantScope,
    ) -> Result<Option<CurrentGrant>> {
        validate_writer(self, writer, &scope.collection)?;
        let _lock = self.lock()?;
        let path = self
            .root
            .join(".quota-1.2/current-grants")
            .join(scope.filename());
        if !path.try_exists()? {
            return Ok(None);
        }
        let record = read_record(&path)?;
        ensure!(&record.scope == scope, "current quota grant scope mismatch");
        Ok(Some(CurrentGrant {
            quota: self.clone(),
            record,
        }))
    }

    /// Conservative orphan discovery. The collection journal, inspected outside
    /// the ledger lock, decides whether each grant is adopted or explicitly retired.
    pub fn current_grants(&self, writer: &crate::WriterLock) -> Result<Vec<CurrentGrant>> {
        let directory = writer
            .path
            .parent()
            .context("invalid current grant writer")?;
        let managed = crate::managed_collection::ManagedPath::parse(directory)?;
        let collection = managed.collection;
        validate_writer(self, writer, collection)?;
        let _lock = self.lock()?;
        Ok(read_records(self, true, None)?
            .into_iter()
            .filter(|record| record.scope.collection == collection)
            .map(|record| CurrentGrant {
                quota: self.clone(),
                record,
            })
            .collect())
    }
}

fn validate_writer(quota: &Quota, writer: &crate::WriterLock, collection: &str) -> Result<()> {
    crate::validate_collection(collection)?;
    let directory = writer.path.parent().context("writer directory missing")?;
    let store = crate::FileStore::new(directory);
    let expected = quota
        .root
        .join(store.collection_namespace()?)
        .join(collection);
    unlinked(&expected)?;
    ensure!(
        store.managed_path()?.logical.canonicalize()? == expected.canonicalize()?
            && store.managed_path()?.collection == collection,
        "current quota grant requires its collection writer"
    );
    Ok(())
}

fn admit(quota: &Quota, collection: &str, additional: u64, new_record: bool) -> Result<()> {
    let records = quota.live_records()?;
    ensure!(
        !new_record || records.len() < MAX_RESERVATIONS,
        "quota reservation count exhausted"
    );
    let policy = quota.read_policy()?;
    let report = quota.measure(&policy, &records, None)?;
    let root_available = policy
        .root_bytes
        .saturating_sub(checked_add(report.used_bytes, report.reserved_bytes)?);
    if additional > root_available {
        return Err(QuotaPressure {
            scope: PressureScope::Root,
            required_bytes: additional,
            available_bytes: root_available,
            protected_bytes: None,
        }
        .into());
    }
    let used = report.collections.get(collection).map_or(Ok(0), |item| {
        checked_add(item.used_bytes, item.reserved_bytes)
    })?;
    let collection_available = policy.limit(collection).saturating_sub(used);
    if additional > collection_available {
        return Err(QuotaPressure {
            scope: PressureScope::Collection,
            required_bytes: additional,
            available_bytes: collection_available,
            protected_bytes: None,
        }
        .into());
    }
    crate::capacity::require_growth(&quota.root, additional)?;
    Ok(())
}

fn read_record(path: &Path) -> Result<DurableRecord> {
    unlinked(
        path.parent()
            .context("current quota grant has no directory")?,
    )?;
    let record: DurableRecord = json_read(path, RECORD_MAX)?;
    crate::validate_collection(&record.scope.collection)?;
    ensure!(
        record.version == 1
            && record.bytes > 0
            && path.file_name().and_then(|name| name.to_str())
                == Some(record.scope.filename().as_str()),
        "invalid durable current quota grant"
    );
    Ok(record)
}

fn read_records(
    quota: &Quota,
    cleanup: bool,
    deadline: Option<Instant>,
) -> Result<Vec<DurableRecord>> {
    let directory = quota.root.join(".quota-1.2/current-grants");
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error.into()),
    };
    unlinked(&directory)?;
    let mut result = vec![];
    for (visited, entry) in entries.enumerate() {
        within_deadline(deadline)?;
        ensure!(
            visited < MAX_RESERVATIONS * 2 + 2,
            "excessive current quota grant state"
        );
        let entry = entry?;
        let path = entry.path();
        unlinked(&path)?;
        ensure!(
            entry.file_type()?.is_file(),
            "invalid current quota grant entry"
        );
        let name = entry.file_name();
        let name = name.to_str().context("invalid current quota grant name")?;
        if name.starts_with("grant-") && name.len() <= 128 {
            // Atomic-write scratch exists only under this ledger lock. A leftover
            // cannot own a pending transaction: its grant was never published.
            ensure!(
                entry.metadata()?.len() <= RECORD_MAX,
                "oversized current grant scratch"
            );
            if cleanup {
                fs::remove_file(&path)?;
            }
            continue;
        }
        result.push(read_record(&path)?);
        ensure!(
            result.len() <= MAX_RESERVATIONS,
            "current quota grant count exhausted"
        );
    }
    Ok(result)
}

pub(super) fn records(
    quota: &Quota,
    cleanup: bool,
    deadline: Option<Instant>,
) -> Result<Vec<LiveRecord>> {
    Ok(read_records(quota, cleanup, deadline)?
        .into_iter()
        .map(|record| LiveRecord {
            record: Record {
                collection: record.scope.collection,
                bytes: record.bytes,
            },
        })
        .collect())
}

fn persist(quota: &Quota, record: &DurableRecord, create: bool) -> Result<()> {
    let directory = quota.root.join(".quota-1.2/current-grants");
    fs::create_dir_all(&directory)?;
    unlinked(&directory)?;
    crate::maintenance::sync_parent(&quota.root.join(".quota-1.2"))?;
    let path = directory.join(record.scope.filename());
    if create {
        ensure!(!path.try_exists()?, "current quota grant already exists");
    } else {
        unlinked(&path)?;
    }
    let bytes = serde_json::to_vec(record)?;
    ensure!(
        bytes.len() as u64 <= RECORD_MAX,
        "current quota grant exceeds metadata bound"
    );
    crate::capacity::require_growth(&quota.root, RECORD_MAX)?;
    let mut temporary = tempfile::Builder::new()
        .prefix("grant-")
        .tempfile_in(&directory)?;
    temporary.write_all(&bytes)?;
    crate::durability::persist_synced(temporary, &path)?;
    crate::maintenance::sync_parent(&directory)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileStore;

    fn scope() -> CurrentGrantScope {
        CurrentGrantScope {
            collection: "fixture".into(),
            incarnation: Incarnation::from_bytes([1; 16]),
            transaction: Sha256Digest::from_bytes([2; 32]),
        }
    }

    #[test]
    fn dropped_grants_survive_accounting_and_resize_without_accumulating_scratch() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let store = FileStore::new(quota.root.join("collections-1.2/fixture"));
        let writer = store.lock()?;
        let grant = quota.reserve_current(&writer, scope(), 64 * 1024)?;
        assert_eq!(grant.capacity(), 64 * 1024);
        drop(grant);
        assert_eq!(quota.status()?.reserved_bytes, 64 * 1024);
        assert_eq!(quota.current_grants(&writer)?.len(), 1);
        let mut grant = quota
            .current_grant(&writer, &scope())?
            .context("missing durable grant")?;
        grant.resize(&writer, 128 * 1024)?;
        assert_eq!(quota.status()?.reserved_bytes, 128 * 1024);
        grant.resize(&writer, 32 * 1024)?;
        assert_eq!(quota.status()?.reserved_bytes, 32 * 1024);
        let normal = quota.reserve("other", 16 * 1024)?;
        assert_eq!(quota.status()?.reserved_bytes, 48 * 1024);
        drop(normal);
        assert_eq!(quota.status()?.reserved_bytes, 32 * 1024);
        grant.release(&writer)?;
        assert_eq!(quota.status()?.reserved_bytes, 0);
        Ok(())
    }

    #[test]
    fn grant_quota_and_writer_isolation_fail_without_retiring_recovery_capacity() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let store = FileStore::new(quota.root.join("collections-1.2/fixture"));
        let writer = store.lock()?;
        let other_store = FileStore::new(quota.root.join("collections-1.2/other"));
        let other = other_store.lock()?;
        let mut grant = quota.reserve_current(&writer, scope(), 64 * 1024)?;
        assert!(grant.resize(&other, 128 * 1024).is_err());
        assert!(quota.current_grant(&other, &scope()).is_err());
        let used = quota.status()?.used_bytes;
        quota.configure(QuotaPolicy {
            root_bytes: used + 64 * 1024 + 4096,
            default_collection_bytes: 128 * 1024,
            collections: BTreeMap::new(),
        })?;
        assert!(
            grant
                .resize(&writer, 128 * 1024)
                .unwrap_err()
                .is::<QuotaPressure>()
        );
        assert!(
            quota
                .reserve("other", 8192)
                .err()
                .context("missing quota pressure")?
                .is::<QuotaPressure>()
        );
        assert_eq!(quota.status()?.reserved_bytes, 64 * 1024);
        assert_eq!(grant.capacity(), 64 * 1024);
        grant.release(&writer)?;
        Ok(())
    }

    #[test]
    fn malformed_or_rebound_durable_metadata_fails_closed() -> Result<()> {
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let store = FileStore::new(quota.root.join("collections-1.2/fixture"));
        let writer = store.lock()?;
        let grant = quota.reserve_current(&writer, scope(), 64 * 1024)?;
        let path = quota
            .root
            .join(".quota-1.2/current-grants")
            .join(scope().filename());
        let mut record = grant.record.clone();
        record.scope.incarnation = Incarnation::from_bytes([9; 16]);
        fs::write(&path, serde_json::to_vec(&record)?)?;
        assert!(quota.status().is_err());
        assert!(quota.current_grant(&writer, &scope()).is_err());
        Ok(())
    }

    #[test]
    fn process_exit_retains_grant_until_the_collection_writer_retires_it() -> Result<()> {
        let root = tempfile::tempdir()?;
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "quota::current::tests::durable_grant_child",
                "--nocapture",
            ])
            .env("AIRK_CURRENT_GRANT_TEST_ROOT", root.path())
            .status()?;
        ensure!(status.success(), "grant child failed");
        let quota = Quota::open(root.path())?;
        assert_eq!(quota.status()?.reserved_bytes, 64 * 1024);
        let store = FileStore::new(quota.root.join("collections-1.2/fixture"));
        let writer = store.lock()?;
        quota
            .current_grant(&writer, &scope())?
            .context("lost crash recovery allowance")?
            .release(&writer)?;
        assert_eq!(quota.status()?.reserved_bytes, 0);
        Ok(())
    }

    #[test]
    fn durable_grant_child() -> Result<()> {
        let Some(root) = std::env::var_os("AIRK_CURRENT_GRANT_TEST_ROOT") else {
            return Ok(());
        };
        let quota = Quota::open(Path::new(&root))?;
        let store = FileStore::new(quota.root.join("collections-1.2/fixture"));
        let writer = store.lock()?;
        let _grant = quota.reserve_current(&writer, scope(), 64 * 1024)?;
        std::process::exit(0);
    }
}
