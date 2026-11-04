// SPDX-License-Identifier: Apache-2.0
//! Settle only a saved setup's dead initial allocations before publishing its
//! Windows root. This is not general quota recovery or admission.
use super::*;

#[derive(Clone, Copy)]
pub(crate) enum InitialCollectionClaimState {
    PreserveAdmission,
    CompletePublishedMetadata,
}
pub(crate) struct InitialCollectionClaim<'a> {
    pub allocation: Incarnation,
    pub collection: &'a str,
    pub bytes: u64,
    /// Derived only by the native saved-operation metadata validator.
    pub state: InitialCollectionClaimState,
}
struct Claim {
    allocation: Incarnation,
    expected: Vec<u8>,
    record_name: OsString,
    lease_name: OsString,
    record: Option<File>,
    lease: Option<PendingLease>,
    settle: bool,
}
impl Claim {
    fn new(allocation: Incarnation, collection: &str, bytes: u64, settle: bool) -> Result<Self> {
        ensure!(bytes > 0, SetupAllocationMismatch(allocation));
        let expected = serde_json::to_vec(&Record {
            collection: collection.into(),
            bytes,
        })?;
        ensure!(
            expected.len() as u64 <= RECORD_MAX,
            SetupAllocationMismatch(allocation)
        );
        Ok(Self {
            allocation,
            expected,
            record_name: format!("r-{allocation}").into(),
            lease_name: format!("r-{allocation}.lease").into(),
            record: None,
            lease: None,
            settle,
        })
    }
}

/// The caller retains the exact saved reservation, ProjectLease and data claim,
/// has verified the final-path OWNER bytes on the private stage, and has not
/// created a collection. All child handles here close before root publication.
#[cfg(test)]
fn settle_initial_owner_claim(
    root: &SetupDirectory,
    allocation: Incarnation,
    expected_bytes: u64,
) -> Result<()> {
    settle_initial_stage_claims(root, allocation, expected_bytes, None)
}

/// Recognize only the optional collection phase of this SAME saved setup.
/// Partial metadata keeps its admitted cap; only validated final collection
/// metadata can settle that phase. Missing allocations are never invented.
pub(crate) fn settle_initial_stage_claims(
    root: &SetupDirectory,
    allocation: Incarnation,
    expected_bytes: u64,
    collection: Option<InitialCollectionClaim<'_>>,
) -> Result<()> {
    ensure!(
        (1..=16384).contains(&expected_bytes),
        SetupAllocationMismatch(allocation)
    );
    let mut claims = vec![Claim::new(allocation, "", expected_bytes, true)?];
    if let Some(collection) = collection {
        crate::validate_collection(collection.collection)?;
        ensure!(
            collection.allocation != allocation,
            SetupAllocationMismatch(allocation)
        );
        claims.push(Claim::new(
            collection.allocation,
            collection.collection,
            collection.bytes,
            matches!(
                collection.state,
                InitialCollectionClaimState::CompletePublishedMetadata
            ),
        )?);
    }
    reject_retired(root)?;
    let lifecycle = match root.open_file(
        OsStr::new(Namespace::Service.lifecycle()),
        SetupFileMode::ReadWrite,
    ) {
        Ok(file) => {
            ensure!(
                file.metadata()?.len() == 0,
                "unknown setup lifecycle contents"
            );
            Some(file)
        }
        Err(error) if missing(&error) => None,
        Err(error) => return Err(error),
    };
    let control = match root.open_dir(OsStr::new(Namespace::Service.control())) {
        Ok(directory) => directory,
        Err(error) if missing(&error) => return Ok(()),
        Err(error) => return Err(error),
    };
    ensure!(
        control.identity()?.0 == root.identity()?.0,
        "initial quota control crosses filesystem boundary"
    );
    let mut ledger = None;
    let mut reservations = None;
    for (number, name) in control.entries()?.enumerate() {
        ensure!(number < 2, "unknown initial owner quota metadata");
        let name = name?;
        if name == "ledger.lock" {
            let file = control.open_file(&name, SetupFileMode::ReadWrite)?;
            ensure!(
                file.metadata()?.len() == 0,
                "unknown initial owner quota ledger"
            );
            ledger = Some(file);
        } else if name == "reservations" {
            let directory = control.open_dir(&name)?;
            ensure!(
                directory.identity()?.0 == control.identity()?.0,
                "initial quota reservations cross filesystem boundary"
            );
            reservations = Some(directory);
        } else {
            anyhow::bail!("unknown initial owner quota metadata");
        }
    }
    let Some(reservations) = reservations else {
        return Ok(());
    };
    if let Some(lifecycle) = &lifecycle {
        lifecycle
            .try_lock_shared()
            .context("initial owner quota lifecycle is busy")?;
    }
    if let Some(ledger) = &ledger {
        admission::acquire(ledger, None)?;
    }
    // Inspect the complete bounded prefix before creating a recovery lease or
    // deleting anything. Never claim an arbitrary abandoned root reservation.
    for (number, name) in reservations.entries()?.enumerate() {
        ensure!(
            number < claims.len() * 2,
            "unknown initial owner quota allocations"
        );
        let name = name?;
        let claim = claims
            .iter_mut()
            .find(|claim| name == claim.record_name || name == claim.lease_name)
            .context("foreign initial owner quota allocation preserved")?;
        if name == claim.record_name {
            let file = reservations.open_file(&name, SetupFileMode::Read)?;
            let bytes = reservations.read_file(&name, RECORD_MAX)?;
            ensure!(
                bytes == claim.expected,
                SetupAllocationMismatch(claim.allocation)
            );
            claim.record = Some(file);
        } else {
            let file = reservations.open_file(&name, SetupFileMode::ReadWrite)?;
            ensure!(
                file.metadata()?.len() == 0,
                SetupAllocationMismatch(claim.allocation)
            );
            claim.lease = Some(PendingLease {
                directory: reservations.try_clone()?,
                name: name.clone(),
                file: Some(file),
                created: false,
            });
        }
    }
    if claims
        .iter()
        .all(|claim| claim.record.is_none() && claim.lease.is_none())
    {
        return Ok(());
    }
    let _lifecycle = lifecycle.context("initial owner allocation lacks lifecycle")?;
    let _ledger = ledger.context("initial owner allocation lacks quota ledger")?;
    // Created recovery leases must roll back before the ledger guard closes.
    let mut claims = claims;
    // Validate the named record again under the canonical ledger, including its
    // retained native file identity. A record-only crash may create a normal
    // empty lease only after this complete validation.
    for claim in &claims {
        if let Some(record) = &claim.record {
            let current = reservations.open_file(&claim.record_name, SetupFileMode::Read)?;
            ensure!(
                file_identity(&current)? == file_identity(record)?,
                SetupAllocationMismatch(claim.allocation)
            );
            ensure!(
                reservations.read_file(&claim.record_name, RECORD_MAX)? == claim.expected,
                SetupAllocationMismatch(claim.allocation)
            );
        }
    }
    // Lock ALL existing leases first. A live collection phase refuses without
    // deleting the owner phase or creating a previously absent recovery lease.
    for claim in &claims {
        if let Some(lease) = &claim.lease {
            let current = reservations.open_file(&claim.lease_name, SetupFileMode::Read)?;
            ensure!(
                file_identity(&current)?
                    == file_identity(lease.file.as_ref().context("missing initial quota lease")?)?,
                SetupAllocationMismatch(claim.allocation)
            );
            lock_lease(
                lease.file.as_ref().context("missing initial quota lease")?,
                claim.allocation,
            )?;
        }
    }
    for claim in &mut claims {
        if claim.settle && claim.record.is_some() && claim.lease.is_none() {
            let file = reservations.open_file(&claim.lease_name, SetupFileMode::CreateNew)?;
            claim.lease = Some(PendingLease {
                directory: reservations.try_clone()?,
                name: claim.lease_name.clone(),
                file: Some(file),
                created: true,
            });
            lock_lease(
                claim
                    .lease
                    .as_ref()
                    .context("missing created quota lease")?
                    .file
                    .as_ref()
                    .context("missing created quota lease handle")?,
                claim.allocation,
            )?;
        }
    }
    for claim in &mut claims {
        if !claim.settle {
            continue;
        }
        if let Some(record) = &claim.record {
            reservations.unlink_file_if_identity(&claim.record_name, record)?;
        }
        if let Some(lease) = &mut claim.lease {
            reservations.unlink_file_if_identity(
                &claim.lease_name,
                lease.file.as_ref().context("missing initial quota lease")?,
            )?;
            lease.created = false;
        }
    }
    reservations.sync()?;
    Ok(())
}

fn lock_lease(lease: &File, allocation: Incarnation) -> Result<()> {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<(tempfile::TempDir, PathBuf, SetupDirectory)> {
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().canonicalize()?;
        let root = SetupDirectory::open_canonical(&path)?;
        Ok((temporary, path, root))
    }
    fn allocation() -> Incarnation {
        Incarnation::from_bytes([83; 16])
    }
    fn collection_allocation() -> Incarnation {
        Incarnation::from_bytes([84; 16])
    }
    fn paths(path: &Path) -> (PathBuf, PathBuf) {
        let records = path.join(Namespace::Service.control()).join("reservations");
        (
            records.join(format!("r-{}", allocation())),
            records.join(format!("r-{}.lease", allocation())),
        )
    }
    fn crashed_claim(path: &Path) -> Result<()> {
        crashed_claim_with_collection(path, false)
    }
    fn crashed_claim_with_collection(path: &Path, collection: bool) -> Result<()> {
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "quota::setup::owner::tests::initial_owner_claim_crash_child",
                "--nocapture",
            ])
            .env("AIRK_INITIAL_OWNER_CLAIM_TEST_ROOT", path)
            .env(
                "AIRK_INITIAL_COLLECTION_CLAIM_TEST",
                if collection { "1" } else { "0" },
            )
            .status()?;
        ensure!(status.success(), "initial owner allocation child failed");
        let (record, lease) = paths(path);
        assert_eq!(
            fs::read(record)?,
            serde_json::to_vec(&Record {
                collection: String::new(),
                bytes: 64
            })?
        );
        assert_eq!(fs::metadata(lease)?.len(), 0);
        Ok(())
    }

    #[test]
    fn initial_owner_claim_crash_child() -> Result<()> {
        let Some(path) = std::env::var_os("AIRK_INITIAL_OWNER_CLAIM_TEST_ROOT") else {
            return Ok(());
        };
        let path = PathBuf::from(path);
        let root = SetupDirectory::open_canonical(&path)?;
        let _claim = SetupMetadataAllowance::reserve_at(&root, &path, "", 64, allocation())?;
        if std::env::var_os("AIRK_INITIAL_COLLECTION_CLAIM_TEST").is_some_and(|value| value == "1")
        {
            let _collection = SetupMetadataAllowance::reserve_at(
                &root,
                &path,
                "fixture",
                128,
                collection_allocation(),
            )?;
            std::process::exit(0);
        }
        std::process::exit(0);
    }

    #[test]
    fn partial_collection_keeps_exact_admitted_record_and_complete_collection_settles_it()
    -> Result<()> {
        for state in [
            InitialCollectionClaimState::PreserveAdmission,
            InitialCollectionClaimState::CompletePublishedMetadata,
        ] {
            let (_temporary, path, root) = fixture()?;
            crashed_claim_with_collection(&path, true)?;
            let (owner_record, owner_lease) = paths(&path);
            let records = owner_record.parent().unwrap();
            let collection_record = records.join(format!("r-{}", collection_allocation()));
            let collection_lease = records.join(format!("r-{}.lease", collection_allocation()));
            let original = fs::read(&collection_record)?;
            let identity = file_identity(&File::open(&collection_record)?)?;
            settle_initial_stage_claims(
                &root,
                allocation(),
                64,
                Some(InitialCollectionClaim {
                    allocation: collection_allocation(),
                    collection: "fixture",
                    bytes: 128,
                    state,
                }),
            )?;
            assert!(!owner_record.exists());
            assert!(!owner_lease.exists());
            match state {
                InitialCollectionClaimState::PreserveAdmission => {
                    assert_eq!(fs::read(&collection_record)?, original);
                    assert_eq!(file_identity(&File::open(&collection_record)?)?, identity);
                    assert_eq!(fs::metadata(&collection_lease)?.len(), 0);
                    let resumed = SetupMetadataAllowance::reserve_at(
                        &root,
                        &path,
                        "fixture",
                        128,
                        collection_allocation(),
                    )?;
                    assert_eq!(file_identity(&File::open(&collection_record)?)?, identity);
                    assert_eq!(Quota::open_service(&path)?.status()?.reserved_bytes, 128);
                    drop(resumed);
                }
                InitialCollectionClaimState::CompletePublishedMetadata => {}
            }
            assert!(!collection_record.exists());
            assert!(!collection_lease.exists());
        }
        Ok(())
    }

    #[test]
    fn live_or_mismatched_collection_claim_refuses_before_owner_cleanup() -> Result<()> {
        let (_temporary, path, root) = fixture()?;
        crashed_claim(&path)?;
        let (owner_record, owner_lease) = paths(&path);
        let original = fs::read(&owner_record)?;
        let live = SetupMetadataAllowance::reserve_at(
            &root,
            &path,
            "fixture",
            128,
            collection_allocation(),
        )?;
        let error = settle_initial_stage_claims(
            &root,
            allocation(),
            64,
            Some(InitialCollectionClaim {
                allocation: collection_allocation(),
                collection: "fixture",
                bytes: 128,
                state: InitialCollectionClaimState::PreserveAdmission,
            }),
        )
        .err()
        .context("missing live collection refusal")?;
        assert!(error.is::<SetupAllocationBusy>());
        assert_eq!(fs::read(&owner_record)?, original);
        assert!(owner_lease.exists());
        assert!(
            settle_initial_stage_claims(
                &root,
                allocation(),
                64,
                Some(InitialCollectionClaim {
                    allocation: collection_allocation(),
                    collection: "fixture",
                    bytes: 129,
                    state: InitialCollectionClaimState::CompletePublishedMetadata
                })
            )
            .is_err()
        );
        assert_eq!(fs::read(&owner_record)?, original);
        assert!(owner_lease.exists());
        drop(live);
        settle_initial_owner_claim(&root, allocation(), 64)?;
        Ok(())
    }

    #[test]
    fn exact_dead_owner_allocation_settles_pair_record_only_and_lease_only_without_new_admission()
    -> Result<()> {
        for remaining in ["pair", "record", "lease"] {
            let (_temporary, path, root) = fixture()?;
            crashed_claim(&path)?;
            let (record, lease) = paths(&path);
            if remaining == "record" {
                fs::remove_file(&lease)?;
            }
            if remaining == "lease" {
                fs::remove_file(&record)?;
            }
            settle_initial_owner_claim(&root, allocation(), 64)?;
            assert!(!record.exists());
            assert!(!lease.exists());
            settle_initial_owner_claim(&root, allocation(), 64)?;
            assert_eq!(fs::read_dir(record.parent().unwrap())?.count(), 0);
        }
        Ok(())
    }

    #[test]
    fn live_initial_owner_allocation_is_preserved_until_actual_lease_release() -> Result<()> {
        let (_temporary, path, root) = fixture()?;
        let claim = SetupMetadataAllowance::reserve_at(&root, &path, "", 64, allocation())?;
        let (record, lease) = paths(&path);
        let before = fs::read(&record)?;
        let error = settle_initial_owner_claim(&root, allocation(), 64)
            .err()
            .context("missing live owner allocation refusal")?;
        assert!(error.is::<SetupAllocationBusy>());
        assert_eq!(fs::read(&record)?, before);
        assert_eq!(fs::metadata(&lease)?.len(), 0);
        drop(claim);
        assert!(!record.exists());
        assert!(!lease.exists());
        Ok(())
    }

    #[test]
    fn unknown_or_mismatched_initial_quota_bytes_are_preserved_before_any_cleanup() -> Result<()> {
        let (_temporary, path, root) = fixture()?;
        crashed_claim(&path)?;
        let (record, lease) = paths(&path);
        let before = fs::read(&record)?;
        assert!(settle_initial_owner_claim(&root, allocation(), 65).is_err());
        assert_eq!(fs::read(&record)?, before);
        assert!(lease.exists());
        let foreign = record.parent().unwrap().join("r-foreign");
        fs::write(&foreign, b"foreign allocation")?;
        assert!(settle_initial_owner_claim(&root, allocation(), 64).is_err());
        assert_eq!(fs::read(&foreign)?, b"foreign allocation");
        assert_eq!(fs::read(&record)?, before);
        assert!(lease.exists());
        fs::remove_file(&foreign)?;
        fs::write(&record, b"{\"collection\":\"foreign\",\"bytes\":64}")?;
        let unknown = fs::read(&record)?;
        assert!(settle_initial_owner_claim(&root, allocation(), 64).is_err());
        assert_eq!(fs::read(&record)?, unknown);
        assert!(lease.exists());
        Ok(())
    }
}
