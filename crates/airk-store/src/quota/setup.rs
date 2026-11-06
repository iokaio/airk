// SPDX-License-Identifier: Apache-2.0
//! Explicit setup-only quota admission anchored to the captured data-root inode.
//! Inventory never sweeps abandoned state. Ordinary quota/provider paths remain
//! path based; no setup inventory runs during reads or retrieval.
use super::*;
use crate::service::registry::catalog::setup::storage::{
    SetupDirectory, SetupFileMode, file_identity,
};
use airk_core::identity::Incarnation;
use std::ffi::{OsStr, OsString};

#[cfg(windows)]
#[path = "setup_owner.rs"]
mod owner;
#[cfg(windows)]
pub(crate) use owner::{
    InitialCollectionClaim, InitialCollectionClaimState, settle_initial_stage_claims,
};

/// A normal native service reservation, held until staged metadata has moved to
/// its final names. The caller computes the exact encoded peak before admission.
pub(crate) struct SetupMetadataAllowance {
    records: SetupDirectory,
    ledger: File,
    record_name: OsString,
    record: File,
    lease_name: OsString,
    lease: File,
    _lifecycle: File,
}

#[derive(Debug)]
pub(crate) struct SetupAllocationBusy(pub Incarnation);
impl std::fmt::Display for SetupAllocationBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native setup allocation {} is already leased", self.0)
    }
}
impl std::error::Error for SetupAllocationBusy {}

#[derive(Debug)]
pub(crate) struct SetupAllocationMismatch(pub Incarnation);
impl std::fmt::Display for SetupAllocationMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "native setup allocation {} has different saved metadata",
            self.0
        )
    }
}
impl std::error::Error for SetupAllocationMismatch {}

struct PendingLease {
    directory: SetupDirectory,
    name: OsString,
    file: Option<File>,
    created: bool,
}
impl Drop for PendingLease {
    fn drop(&mut self) {
        if self.created
            && let Some(file) = &self.file
        {
            let _ = unlink_owned(&self.directory, &self.name, file);
        }
    }
}

fn missing(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
}

fn existing_or_new(directory: &SetupDirectory, name: &OsStr) -> Result<File> {
    match directory.open_file(name, SetupFileMode::ReadWrite) {
        Ok(file) => Ok(file),
        Err(error) if missing(&error) => {
            match directory.open_file(name, SetupFileMode::CreateNew) {
                Ok(file) => {
                    directory.sync()?;
                    Ok(file)
                }
                Err(error)
                    if error
                        .downcast_ref::<io::Error>()
                        .is_some_and(|error| error.kind() == io::ErrorKind::AlreadyExists) =>
                {
                    directory.open_file(name, SetupFileMode::ReadWrite)
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

fn reject_retired(root: &SetupDirectory) -> Result<()> {
    for name in [
        ".relocated-to.json",
        ".relocation-in-progress.json",
        ".relocation-pending.json",
    ] {
        match root.open_entry(OsStr::new(name)) {
            Ok(_) => anyhow::bail!(
                "data root is relocated or awaiting relocation recovery; inspect {name}"
            ),
            Err(error) if missing(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn control_entries(control: &SetupDirectory) -> Result<()> {
    for (visited, entry) in control.entries()?.enumerate() {
        ensure!(
            visited < MAX_RESERVATIONS * 2 + 4,
            "excessive quota control state"
        );
        let name = entry?;
        if name.to_string_lossy().starts_with("policy-") {
            ensure!(
                control
                    .open_file(&name, SetupFileMode::Read)?
                    .metadata()?
                    .len()
                    <= POLICY_MAX,
                "invalid abandoned quota policy state"
            );
        } else {
            ensure!(
                [
                    "ledger.lock",
                    "policy.json",
                    "reservations",
                    "current-grants",
                    "split-allocations"
                ]
                .iter()
                .any(|allowed| name == *allowed),
                "unexpected quota control entry"
            );
        }
    }
    Ok(())
}

fn policy(control: &SetupDirectory) -> Result<QuotaPolicy> {
    let policy = match control.read_file(OsStr::new("policy.json"), POLICY_MAX) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if missing(&error) => QuotaPolicy::default(),
        Err(error) => return Err(error),
    };
    policy.validate()?;
    Ok(policy)
}

fn outstanding(
    control: &SetupDirectory,
    reservations: &SetupDirectory,
    root: &SetupDirectory,
    logical_root: &Path,
) -> Result<Vec<LiveRecord>> {
    let mut records = Vec::new();
    for (visited, entry) in reservations.entries()?.enumerate() {
        ensure!(
            visited < MAX_RESERVATIONS * 2 + 2,
            "quota ledger contains excessive state"
        );
        let name = entry?;
        let text = name.to_str().context("invalid quota record name")?;
        ensure!(
            text.starts_with("r-") && text.len() <= 128,
            "unexpected quota ledger entry"
        );
        let file = reservations.open_file(&name, SetupFileMode::Read)?;
        if Path::new(&name)
            .extension()
            .is_some_and(|extension| extension == "lease")
        {
            ensure!(file.metadata()?.len() == 0, "invalid quota lease");
            continue;
        }
        let bytes = reservations.read_file(&name, RECORD_MAX)?;
        let record: Record = serde_json::from_slice(&bytes)?;
        if !record.collection.is_empty() {
            crate::validate_collection(&record.collection)?;
        }
        records.push(LiveRecord { record });
    }
    let current = match control.open_dir(OsStr::new("current-grants")) {
        Ok(current) => Some(current),
        Err(error) if missing(&error) => None,
        Err(error) => return Err(error),
    };
    if let Some(current) = current {
        ensure!(
            current.identity()?.0 == control.identity()?.0,
            "current quota grants cross filesystem boundary"
        );
        let mut count = 0;
        for (visited, entry) in current.entries()?.enumerate() {
            ensure!(
                visited < MAX_RESERVATIONS * 2 + 2,
                "excessive current quota grant state"
            );
            let name = entry?;
            let text = name.to_str().context("invalid current quota grant name")?;
            let bytes = current.read_file(&name, RECORD_MAX)?;
            if text.starts_with("grant-") && text.len() <= 128 {
                continue;
            }
            records.push(current::decode_inventory_record(&name, &bytes)?);
            count += 1;
            ensure!(
                count <= MAX_RESERVATIONS,
                "current quota grant count exhausted"
            );
        }
    }
    records.extend(funding::pinned_records(control, root, logical_root)?);
    ensure!(
        records.len() <= MAX_RESERVATIONS,
        "quota reservation count exhausted"
    );
    Ok(records)
}

#[derive(Default)]
struct Usage {
    used: u64,
    collection_used: u64,
}

// Each active DFS frame retains one directory and one native inventory cursor.
// Bound depth as well as entries; width must not consume descriptor capacity.
const MAX_SETUP_INVENTORY_DEPTH: usize = 64;

#[derive(Debug)]
pub(crate) struct SetupInventoryDepthLimit {
    pub depth: usize,
    pub limit: usize,
}
impl std::fmt::Display for SetupInventoryDepthLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "setup quota inventory depth {} exceeds limit {}; refusing allocation",
            self.depth, self.limit
        )
    }
}
impl std::error::Error for SetupInventoryDepthLimit {}

struct Inventory<'a> {
    device: u64,
    collection: &'a str,
    visited: usize,
    usage: Usage,
}
impl Inventory<'_> {
    fn walk(&mut self, directory: &SetupDirectory, relative: &mut Vec<OsString>) -> Result<()> {
        ensure!(
            relative.len() <= MAX_SETUP_INVENTORY_DEPTH,
            SetupInventoryDepthLimit {
                depth: relative.len(),
                limit: MAX_SETUP_INVENTORY_DEPTH
            }
        );
        // Descend immediately while the parent's native iterator remains open.
        // There is only one active child descriptor, never a sibling queue.
        for entry in directory.entries()? {
            let name = entry?;
            self.visited += 1;
            check_inventory_count(self.visited)?;
            if relative.is_empty() && excluded_at_root(&name) {
                continue;
            }
            let file = match directory.open_entry(&name) {
                Ok(file) => file,
                Err(error) if missing(&error) => continue,
                Err(error) => return Err(error),
            };
            let metadata = file.metadata()?;
            ensure!(
                file_identity(&file)?.0 == self.device,
                "quota inventory crosses filesystem boundary"
            );
            relative.push(name);
            if metadata.is_dir() {
                self.walk(&SetupDirectory::from_file(file)?, relative)?;
            } else {
                ensure!(
                    metadata.is_file(),
                    "quota inventory contains nonregular state"
                );
                self.usage.used = checked_add(self.usage.used, metadata.len())?;
                if relative.len() >= 2
                    && relative[0] == Namespace::Service.collections()
                    && relative[1] == self.collection
                {
                    self.usage.collection_used =
                        checked_add(self.usage.collection_used, metadata.len())?;
                }
            }
            relative.pop();
        }
        Ok(())
    }
}

fn excluded_at_root(name: &OsStr) -> bool {
    [
        Namespace::Service.control(),
        "collections-1.2",
        ".quota-1.2",
        ".lifecycle.lock",
    ]
    .iter()
    .any(|excluded| name == *excluded)
}

fn inventory(root: &SetupDirectory, collection: &str) -> Result<Usage> {
    let mut inventory = Inventory {
        device: root.identity()?.0,
        collection,
        visited: 0,
        usage: Usage::default(),
    };
    inventory.walk(root, &mut Vec::new())?;
    Ok(inventory.usage)
}

fn unlink_owned(directory: &SetupDirectory, name: &OsStr, owned: &File) -> Result<()> {
    directory.unlink_file_if_identity(name, owned)
}

impl SetupMetadataAllowance {
    pub(crate) fn reserve_at(
        root: &SetupDirectory,
        logical_root: &Path,
        collection: &str,
        exact_encoded_bytes: u64,
        allocation: Incarnation,
    ) -> Result<Self> {
        if !collection.is_empty() {
            crate::validate_collection(collection)?;
        }
        ensure!(
            logical_root.is_absolute(),
            "setup quota root label must be absolute"
        );
        ensure!(
            exact_encoded_bytes > 0,
            "quota reservation must be positive"
        );
        reject_retired(root)?;
        let lifecycle = existing_or_new(root, OsStr::new(Namespace::Service.lifecycle()))?;
        lifecycle
            .try_lock_shared()
            .context("data root relocation in progress")?;
        reject_retired(root)?;
        let control = root.create_dir(OsStr::new(Namespace::Service.control()))?;
        let reservations = control.create_dir(OsStr::new("reservations"))?;
        root.sync()?;
        control.sync()?;
        let ledger = existing_or_new(&control, OsStr::new("ledger.lock"))?;
        admission::acquire(&ledger, None)?;
        control_entries(&control)?;
        let policy = policy(&control)?;
        // Names derive from the native saved operation and phase. They retain
        // the ordinary r-* ledger format; this is not a new authority record.
        let record_name = OsString::from(format!("r-{allocation}"));
        let lease_name = OsString::from(format!("r-{allocation}.lease"));
        let lease_directory = reservations.try_clone()?;
        let (lease, created) = match reservations.open_file(&lease_name, SetupFileMode::ReadWrite) {
            Ok(file) => (file, false),
            Err(error) if missing(&error) => (
                reservations.open_file(&lease_name, SetupFileMode::CreateNew)?,
                true,
            ),
            Err(error) => return Err(error),
        };
        let mut pending_lease = PendingLease {
            directory: lease_directory,
            name: lease_name.clone(),
            file: Some(lease),
            created,
        };
        let lease = pending_lease
            .file
            .as_ref()
            .context("missing setup allocation lease")?;
        match lease.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(SetupAllocationBusy(allocation).into());
            }
            Err(error) => return Err(error.into()),
        }
        ensure!(
            lease.metadata()?.len() == 0,
            SetupAllocationMismatch(allocation)
        );
        lease.sync_all()?;
        // Acquire the actual lease before inspecting an earlier attempt. Other
        // abandoned allocations are counted conservatively and never retired.
        let existing = match reservations.open_file(&record_name, SetupFileMode::ReadWrite) {
            Ok(file) => {
                let persisted = reservations
                    .read_file(&record_name, RECORD_MAX)
                    .context(SetupAllocationMismatch(allocation))?;
                let saved: Record = serde_json::from_slice(&persisted)
                    .context(SetupAllocationMismatch(allocation))?;
                ensure!(
                    saved.collection == collection && saved.bytes == exact_encoded_bytes,
                    SetupAllocationMismatch(allocation)
                );
                Some(file)
            }
            Err(error) if missing(&error) => None,
            Err(error) => return Err(error),
        };
        let records = outstanding(&control, &reservations, root, logical_root)?;
        ensure!(
            records
                .len()
                .saturating_sub(usize::from(existing.is_some()))
                < MAX_RESERVATIONS,
            "quota reservation count exhausted"
        );
        let usage = inventory(root, collection)?;
        let reserved = records
            .iter()
            .try_fold(0, |total, item| checked_add(total, item.record.bytes))?;
        let collection_reserved = records
            .iter()
            .filter(|item| item.record.collection == collection)
            .try_fold(0, |total, item| checked_add(total, item.record.bytes))?;
        if existing.is_none() {
            admit_logical_bytes(
                &policy,
                (usage.used, reserved),
                checked_add(usage.collection_used, collection_reserved)?,
                collection,
                exact_encoded_bytes,
                false,
            )?;
        }
        // An exact existing allocation already admitted this fixed peak. Like
        // an ordinary live Reservation, using its remaining allowance does not
        // admit that peak again after its own staged bytes have been written.
        // The native stager must keep the same saved-operation/phase byte cap.
        let bytes = serde_json::to_vec(&Record {
            collection: collection.into(),
            bytes: exact_encoded_bytes,
        })?;
        ensure!(
            bytes.len() as u64 <= RECORD_MAX,
            "quota metadata exceeds bounded size"
        );
        root.require_growth(checked_add(
            exact_encoded_bytes,
            if existing.is_none() {
                bytes.len() as u64
            } else {
                0
            },
        )?)?;
        let record = if let Some(record) = existing {
            record
        } else {
            let mut record = reservations.open_file(&record_name, SetupFileMode::CreateNew)?;
            if let Err(error) = record.write_all(&bytes).and_then(|()| record.sync_all()) {
                let _ = unlink_owned(&reservations, &record_name, &record);
                return Err(error.into());
            }
            record
        };
        let lease = pending_lease
            .file
            .take()
            .context("missing setup allocation lease")?;
        let allowance = Self {
            records: reservations,
            ledger,
            record_name,
            record,
            lease_name,
            lease,
            _lifecycle: lifecycle,
        };
        allowance.records.sync()?;
        allowance.ledger.unlock()?;
        Ok(allowance)
    }
}

impl Drop for SetupMetadataAllowance {
    fn drop(&mut self) {
        // Failure to acquire/validate leaves a conservative abandoned reservation;
        // a future normal native recovery owns retirement. Never sweep a tree.
        if self.ledger.try_lock().is_ok() {
            let _ = unlink_owned(&self.records, &self.record_name, &self.record);
            let _ = unlink_owned(&self.records, &self.lease_name, &self.lease);
            let _ = self.records.sync();
            let _ = self.ledger.unlock();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<(tempfile::TempDir, PathBuf, SetupDirectory, Quota)> {
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().canonicalize()?;
        let quota = Quota::open_service(&path)?;
        let root = SetupDirectory::open_canonical(&path)?;
        Ok((temporary, path, root, quota))
    }

    fn allocation() -> Incarnation {
        Incarnation::from_bytes([7; 16])
    }

    #[test]
    fn exact_setup_allowance_shares_native_pressure_and_rolls_back() -> Result<()> {
        let (_temporary, path, root, quota) = fixture()?;
        let used = quota.status()?.used_bytes;
        quota.configure(QuotaPolicy {
            root_bytes: used + 128,
            default_collection_bytes: 64,
            collections: BTreeMap::new(),
        })?;
        let error = SetupMetadataAllowance::reserve_at(&root, &path, "fixture", 65, allocation())
            .err()
            .context("missing collection pressure")?;
        assert!(matches!(
            error.downcast_ref::<QuotaPressure>().map(|p| p.scope),
            Some(PressureScope::Collection)
        ));
        let allowance =
            SetupMetadataAllowance::reserve_at(&root, &path, "fixture", 64, allocation())?;
        assert_eq!(quota.status()?.reserved_bytes, 64);
        let error = quota
            .reserve("fixture", 1)
            .err()
            .context("missing shared ledger pressure")?;
        assert!(error.is::<QuotaPressure>());
        drop(allowance);
        assert_eq!(quota.status()?.reserved_bytes, 0);
        let error = SetupMetadataAllowance::reserve_at(&root, &path, "other", 129, allocation())
            .err()
            .context("missing root pressure")?;
        assert!(matches!(
            error.downcast_ref::<QuotaPressure>().map(|p| p.scope),
            Some(PressureScope::Root)
        ));
        assert_eq!(quota.status()?.reserved_bytes, 0);
        // Initial data-scope ownership is a native root reservation and must
        // not consume a nonexistent collection's default 64-byte allowance.
        let root_claim = SetupMetadataAllowance::reserve_at(&root, &path, "", 100, allocation())?;
        assert_eq!(quota.status()?.reserved_bytes, 100);
        drop(root_claim);
        assert_eq!(quota.status()?.reserved_bytes, 0);
        Ok(())
    }

    #[test]
    fn setup_inventory_uses_pinned_inode_after_root_rename_and_replacement() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let parent = temporary.path().canonicalize()?;
        let original = parent.join("root");
        fs::create_dir(&original)?;
        let quota = Quota::open_service(&original)?;
        quota.configure(QuotaPolicy {
            root_bytes: 1_000_000,
            default_collection_bytes: 1_000_000,
            collections: BTreeMap::new(),
        })?;
        let pinned = SetupDirectory::open_canonical(&original)?;
        let moved = parent.join("moved");
        fs::rename(&original, &moved)?;
        fs::create_dir(&original)?;
        fs::write(original.join("replacement"), b"untouched")?;
        let allowance =
            SetupMetadataAllowance::reserve_at(&pinned, &original, "fixture", 64, allocation())?;
        assert!(!original.join(".quota-1.4").exists());
        assert_eq!(fs::read(original.join("replacement"))?, b"untouched");
        assert_eq!(Quota::open_service(&moved)?.status()?.reserved_bytes, 64);
        drop(allowance);
        assert_eq!(Quota::open_service(&moved)?.status()?.reserved_bytes, 0);
        assert_eq!(fs::read(original.join("replacement"))?, b"untouched");
        Ok(())
    }

    #[test]
    fn setup_inventory_counts_existing_collections_and_excludes_legacy_namespace() -> Result<()> {
        let (_temporary, path, root, quota) = fixture()?;
        fs::create_dir_all(path.join("collections-1.4/fixture"))?;
        fs::write(path.join("collections-1.4/fixture/existing"), [0; 40])?;
        fs::create_dir(path.join("collections-1.2"))?;
        fs::write(path.join("collections-1.2/legacy"), [0; 200])?;
        let usage = inventory(&root, "fixture")?;
        assert_eq!(usage.collection_used, 40);
        assert_eq!(usage.used, quota.status()?.used_bytes);
        quota.configure(QuotaPolicy {
            root_bytes: 1000,
            default_collection_bytes: 64,
            collections: BTreeMap::new(),
        })?;
        assert!(
            SetupMetadataAllowance::reserve_at(&root, &path, "fixture", 25, allocation()).is_err()
        );
        drop(SetupMetadataAllowance::reserve_at(
            &root,
            &path,
            "fixture",
            24,
            allocation(),
        )?);
        Ok(())
    }

    #[test]
    fn setup_inventory_never_recovers_or_deletes_abandoned_and_unknown_entries() -> Result<()> {
        let (_temporary, path, root, _quota) = fixture()?;
        let control = path.join(".quota-1.4");
        let abandoned = control.join("reservations/r-abandoned");
        let bytes = serde_json::to_vec(&Record {
            collection: "fixture".into(),
            bytes: 64,
        })?;
        fs::write(&abandoned, &bytes)?;
        fs::write(control.join("policy-abandoned"), b"scratch")?;
        drop(SetupMetadataAllowance::reserve_at(
            &root,
            &path,
            "fixture",
            64,
            allocation(),
        )?);
        assert_eq!(fs::read(&abandoned)?, bytes);
        assert_eq!(fs::read(control.join("policy-abandoned"))?, b"scratch");
        let unknown = control.join("reservations/foreign");
        fs::write(&unknown, b"do not remove")?;
        assert!(
            SetupMetadataAllowance::reserve_at(&root, &path, "fixture", 64, allocation()).is_err()
        );
        assert_eq!(fs::read(unknown)?, b"do not remove");
        assert_eq!(fs::read(abandoned)?, bytes);
        Ok(())
    }

    #[test]
    fn dropping_allowance_preserves_replaced_record_and_lease() -> Result<()> {
        let (_temporary, path, root, _quota) = fixture()?;
        let allowance =
            SetupMetadataAllowance::reserve_at(&root, &path, "fixture", 64, allocation())?;
        let directory = path.join(".quota-1.4/reservations");
        let record = directory.join(&allowance.record_name);
        let lease = directory.join(&allowance.lease_name);
        fs::rename(&record, directory.join("saved-record"))?;
        fs::rename(&lease, directory.join("saved-lease"))?;
        fs::write(&record, b"foreign record")?;
        fs::write(&lease, b"foreign lease")?;
        drop(allowance);
        assert_eq!(fs::read(record)?, b"foreign record");
        assert_eq!(fs::read(lease)?, b"foreign lease");
        Ok(())
    }

    #[test]
    fn crashed_setup_reuses_exact_allocation_without_replacing_or_double_counting() -> Result<()> {
        let (_temporary, path, root, quota) = fixture()?;
        let used = quota.status()?.used_bytes;
        quota.configure(QuotaPolicy {
            root_bytes: used + 64,
            default_collection_bytes: 64,
            collections: BTreeMap::new(),
        })?;
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "quota::setup::tests::deterministic_setup_allocation_child",
                "--nocapture",
            ])
            .env("AIRK_SETUP_ALLOCATION_TEST_ROOT", &path)
            .status()?;
        ensure!(status.success(), "setup allocation child failed");
        let directory = path.join(".quota-1.4/reservations");
        let record = directory.join(format!("r-{}", allocation()));
        let original = file_identity(&File::open(&record)?)?;
        let bytes = fs::read(&record)?;
        assert_eq!(fs::read_dir(&directory)?.count(), 2);
        let mismatch =
            SetupMetadataAllowance::reserve_at(&root, &path, "fixture", 65, allocation())
                .err()
                .context("missing allocation mismatch")?;
        assert!(mismatch.is::<SetupAllocationMismatch>());
        assert_eq!(fs::read(&record)?, bytes);
        assert_eq!(fs::read_dir(&directory)?.count(), 2);
        let allowance =
            SetupMetadataAllowance::reserve_at(&root, &path, "fixture", 64, allocation())?;
        let resumed = file_identity(&File::open(&record)?)?;
        assert_eq!(original, resumed);
        assert_eq!(quota.status()?.reserved_bytes, 64);
        assert_eq!(fs::read_dir(&directory)?.count(), 2);
        let busy = SetupMetadataAllowance::reserve_at(&root, &path, "fixture", 64, allocation())
            .err()
            .context("missing live allocation refusal")?;
        assert!(busy.is::<SetupAllocationBusy>());
        let other = SetupMetadataAllowance::reserve_at(
            &root,
            &path,
            "fixture",
            1,
            Incarnation::from_bytes([8; 16]),
        )
        .err()
        .context("missing independent allocation pressure")?;
        assert!(other.is::<QuotaPressure>());
        assert_eq!(fs::read(&record)?, bytes);
        assert_eq!(fs::read_dir(&directory)?.count(), 2);
        drop(allowance);
        assert_eq!(quota.status()?.reserved_bytes, 0);
        assert_eq!(fs::read_dir(&directory)?.count(), 0);
        assert_eq!(fs::read(path.join("native-stage-evidence"))?, [b's'; 64]);
        Ok(())
    }

    #[test]
    fn deterministic_setup_allocation_child() -> Result<()> {
        let Some(path) = std::env::var_os("AIRK_SETUP_ALLOCATION_TEST_ROOT") else {
            return Ok(());
        };
        let path = PathBuf::from(path);
        let root = SetupDirectory::open_canonical(&path)?;
        let _allowance =
            SetupMetadataAllowance::reserve_at(&root, &path, "fixture", 64, allocation())?;
        root.create_file(OsStr::new("native-stage-evidence"), &[b's'; 64])?;
        // Exercise real process-death descriptor release without running the
        // allowance destructor or changing any persisted quota bytes.
        std::process::exit(0);
    }

    #[test]
    fn native_setup_inventory_counts_wide_shared_root_without_sibling_handle_queue() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().canonicalize()?;
        let root = SetupDirectory::open_canonical(&path)?;
        let collections = root.create_dir(OsStr::new("collections-1.4"))?;
        for index in 0..512 {
            let collection = collections.create_dir(OsStr::new(&format!("c-{index:04}")))?;
            collection.create_file(OsStr::new("payload"), &[index as u8])?;
        }
        let usage = inventory(&root, "c-0001")?;
        assert_eq!(usage.used, 512);
        assert_eq!(usage.collection_used, 1);
        assert_eq!(root.identity()?, root.try_clone()?.identity()?);
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn wide_setup_inventory_succeeds_with_64_descriptor_child_limit() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().canonicalize()?;
        fs::create_dir(root.join("collections-1.4"))?;
        for index in 0..512 {
            let collection = root.join(format!("collections-1.4/c-{index:04}"));
            fs::create_dir(&collection)?;
            fs::write(collection.join("payload"), [index as u8])?;
        }
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "quota::setup::tests::bounded_setup_inventory_child",
                "--nocapture",
            ])
            .env("AIRK_SETUP_INVENTORY_TEST_ROOT", &root)
            .status()?;
        ensure!(status.success(), "bounded setup inventory child failed");
        for index in 0..512 {
            assert_eq!(
                fs::read(root.join(format!("collections-1.4/c-{index:04}/payload")))?,
                [index as u8]
            );
        }
        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn bounded_setup_inventory_child() -> Result<()> {
        let Some(path) = std::env::var_os("AIRK_SETUP_INVENTORY_TEST_ROOT") else {
            return Ok(());
        };
        let root = SetupDirectory::open_canonical(Path::new(&path))?;
        let mut limit = std::mem::MaybeUninit::<libc::rlimit>::uninit();
        // SAFETY: correctly sized writable output; this is a dedicated child,
        // never a limit change in the parent running concurrent Rust tests.
        ensure!(
            unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, limit.as_mut_ptr()) } == 0,
            io::Error::last_os_error()
        );
        // SAFETY: successful getrlimit initialized both limit fields.
        let original = unsafe { limit.assume_init() };
        ensure!(
            original.rlim_max >= 64,
            "fixture requires at least 64 descriptor hard limit"
        );
        let bounded = libc::rlimit {
            rlim_cur: 64,
            rlim_max: original.rlim_max,
        };
        // SAFETY: valid limit; only this subprocess is affected.
        ensure!(
            unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &bounded) } == 0,
            io::Error::last_os_error()
        );
        let usage = inventory(&root, "c-0001")?;
        assert_eq!(usage.used, 512);
        assert_eq!(usage.collection_used, 1);
        Ok(())
    }

    #[test]
    fn setup_inventory_refuses_excessive_depth_explicitly() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = SetupDirectory::open_canonical(&temporary.path().canonicalize()?)?;
        let mut child = root.try_clone()?;
        for _ in 0..=MAX_SETUP_INVENTORY_DEPTH {
            child = child.create_dir(OsStr::new("nested"))?;
        }
        child.create_file(OsStr::new("payload"), b"unchanged")?;
        let error = inventory(&root, "fixture")
            .err()
            .context("missing depth refusal")?;
        let depth = error
            .downcast_ref::<SetupInventoryDepthLimit>()
            .context("missing typed depth limit")?;
        assert_eq!(depth.depth, MAX_SETUP_INVENTORY_DEPTH + 1);
        assert_eq!(depth.limit, MAX_SETUP_INVENTORY_DEPTH);
        assert_eq!(child.read_file(OsStr::new("payload"), 32)?, b"unchanged");
        Ok(())
    }
}
