// SPDX-License-Identifier: Apache-2.0
//! Real quota APIs and OS leases; no synthesized journal/publication/Ready state.
use super::*;
use crate::FileStore;
use airk_core::identity::Sha256Digest;

struct Fixture {
    quota: Quota,
    registration: ProjectRegistration,
    healthy: ProjectRegistration,
    control: PathBuf,
    namespace: Incarnation,
    _owner: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Result<Self> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let control = base.join("control");
        fs::create_dir(&control)?;
        let root = base.join("data");
        let namespace = Incarnation::from_bytes([71; 16]);
        crate::service::data_scope::bind(&root, &control, namespace)?;
        let quota = Quota::open_service(&root)?;
        quota.configure(QuotaPolicy {
            root_bytes: 64 * 1024 * 1024,
            default_collection_bytes: 32 * 1024 * 1024,
            collections: BTreeMap::new(),
        })?;
        let mut registrations = Vec::new();
        for name in ["retired-a", "healthy-b"] {
            let source = base.join(name);
            fs::create_dir(&source)?;
            fs::write(source.join("source.md"), b"source preserved")?;
            let registration = ProjectRegistration::new(
                source.to_str().unwrap().into(),
                root.to_str().unwrap().into(),
                name.into(),
            )?;
            let directory = root.join("collections-1.4").join(name);
            let store = FileStore::new(&directory);
            let writer = store.lock()?;
            fs::write(directory.join("payload"), name)?;
            for n in [1, 2] {
                // Actual separately admitted durable preparation credit. It is
                // deliberately preserved by Drop, exactly as after process exit.
                drop(quota.reserve_current(
                    &writer,
                    CurrentGrantScope {
                        collection: name.into(),
                        incarnation: Incarnation::from_bytes([n; 16]),
                        transaction: Sha256Digest::from_bytes([n; 32]),
                    },
                    4096,
                )?);
            }
            drop(writer);
            registrations.push(registration);
        }
        Ok(Self {
            quota,
            registration: registrations.remove(0),
            healthy: registrations.remove(0),
            control,
            namespace,
            _owner: owner,
        })
    }
    fn directory(&self) -> PathBuf {
        self.quota
            .root
            .join("collections-1.4")
            .join(&self.registration.collection)
    }
    fn grants(&self, collection: &str) -> Result<Vec<(PathBuf, Vec<u8>)>> {
        let mut result = Vec::new();
        for entry in fs::read_dir(self.quota.control_path().join("current-grants"))? {
            let path = entry?.path();
            let bytes = fs::read(&path)?;
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            if value["scope"]["collection"] == collection {
                result.push((path, bytes));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }
    fn lease(&self) -> Result<ProjectLease> {
        ProjectLease::acquire(&self.control, self.registration.binding)
    }
    fn assert_sources(&self) -> Result<()> {
        for registration in [&self.registration, &self.healthy] {
            assert_eq!(
                fs::read(Path::new(&registration.root).join("source.md"))?,
                b"source preserved"
            );
        }
        Ok(())
    }
}
#[test]
fn absent_collection_releases_only_its_disposed_ordinary_credit_and_allows_shared_root_writer()
-> Result<()> {
    let f = Fixture::new()?;
    let healthy = f.grants(&f.healthy.collection)?;
    assert_eq!(f.grants(&f.registration.collection)?.len(), 2);
    let before = f.quota.status()?;
    f.quota.configure(QuotaPolicy {
        root_bytes: before.used_bytes + before.reserved_bytes + 128,
        default_collection_bytes: 32 * 1024 * 1024,
        collections: BTreeMap::new(),
    })?;
    assert!(f.quota.reserve(&f.healthy.collection, 4096).is_err());
    fs::remove_dir_all(f.directory())?;
    let lease = f.lease()?;
    let proof =
        AbsentCollectionSettlement::settle(&f.registration, &lease, &f.control, f.namespace)?;
    assert!(f.grants(&f.registration.collection)?.is_empty());
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    proof.revalidate()?;
    drop(proof);
    let healthy_credit = f.quota.reserve(&f.healthy.collection, 4096)?;
    healthy_credit.charge(4096)?;
    assert!(!f.directory().exists());
    f.assert_sources()?;
    Ok(())
}
#[test]
fn live_old_ordinary_lease_refuses_all_selected_grant_release() -> Result<()> {
    let f = Fixture::new()?;
    let held = f.quota.reserve(&f.registration.collection, 4096)?;
    let original = f.grants(&f.registration.collection)?;
    let healthy = f.grants(&f.healthy.collection)?;
    fs::remove_dir_all(f.directory())?;
    let lease = f.lease()?;
    assert!(
        AbsentCollectionSettlement::settle(&f.registration, &lease, &f.control, f.namespace)
            .is_err()
    );
    assert_eq!(f.grants(&f.registration.collection)?, original);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    drop(held);
    drop(AbsentCollectionSettlement::settle(
        &f.registration,
        &lease,
        &f.control,
        f.namespace,
    )?);
    assert!(f.grants(&f.registration.collection)?.is_empty());
    f.assert_sources()?;
    Ok(())
}
#[test]
fn interrupted_disposal_is_idempotent_without_releasing_other_collection_or_source() -> Result<()> {
    let f = Fixture::new()?;
    let original = f.grants(&f.registration.collection)?;
    let healthy = f.grants(&f.healthy.collection)?;
    let ordinary = f.quota.reserve(&f.registration.collection, 4096)?;
    let healthy_ordinary = f.quota.reserve(&f.healthy.collection, 4096)?;
    let ordinary_record = ordinary.0.record_path.clone();
    let ordinary_lease = ordinary.0.lease_path.clone();
    let healthy_record = healthy_ordinary.0.record_path.clone();
    let healthy_record_bytes = fs::read(&healthy_record)?;
    // Actual admission created this pair. A held real ledger prevents Drop from
    // cleaning it; field Drop then releases its OS lease as after process exit.
    let ledger = f.quota.lock()?;
    drop(ordinary);
    drop(ledger);
    assert!(ordinary_record.exists() && ordinary_lease.exists());
    fs::remove_dir_all(f.directory())?;
    let lease = f.lease()?;
    assert!(
        AbsentCollectionSettlement::settle_with_hook(
            &f.registration,
            &lease,
            &f.control,
            f.namespace,
            |point| {
                ensure!(
                    point != SettlementPoint::BeforeUnlink,
                    "injected before effect"
                );
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(f.grants(&f.registration.collection)?, original);
    assert!(
        AbsentCollectionSettlement::settle_with_hook(
            &f.registration,
            &lease,
            &f.control,
            f.namespace,
            |point| {
                ensure!(
                    point != SettlementPoint::AfterGrant,
                    "injected after actual disposed grant unlink"
                );
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(f.grants(&f.registration.collection)?.len(), 1);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    let mut reached = false;
    assert!(
        AbsentCollectionSettlement::settle_with_hook(
            &f.registration,
            &lease,
            &f.control,
            f.namespace,
            |point| {
                if point == SettlementPoint::AfterReservationRecord {
                    reached = true;
                    anyhow::bail!("interrupted after actual ordinary record unlink");
                }
                Ok(())
            },
        )
        .is_err()
    );
    assert!(
        reached,
        "failed before the selected ordinary-disposal checkpoint"
    );
    assert!(!ordinary_record.exists() && ordinary_lease.exists());
    assert_eq!(fs::read(&healthy_record)?, healthy_record_bytes);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    drop(AbsentCollectionSettlement::settle(
        &f.registration,
        &lease,
        &f.control,
        f.namespace,
    )?);
    drop(AbsentCollectionSettlement::settle(
        &f.registration,
        &lease,
        &f.control,
        f.namespace,
    )?);
    assert!(f.grants(&f.registration.collection)?.is_empty());
    assert!(!ordinary_record.exists() && ordinary_lease.exists());
    // Native retirement never guesses ownership of a now-unattributed lease.
    // Existing ordinary accounting reaps it under ledger + actual OS lock proof.
    f.quota.status()?;
    assert!(!ordinary_lease.exists());
    let entries = fs::read_dir(f.quota.control_path().join("reservations"))?
        .map(|entry| Ok(entry?.path()))
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(
        entries.len(),
        2,
        "ordinary reaper retained an orphan that consumes the bounded ledger inventory"
    );
    assert!(entries.contains(&healthy_record) && entries.contains(&healthy_ordinary.0.lease_path));
    assert_eq!(fs::read(&healthy_record)?, healthy_record_bytes);
    drop(f.quota.reserve(&f.healthy.collection, 1024)?);
    assert_eq!(fs::read(&healthy_record)?, healthy_record_bytes);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    f.assert_sources()?;
    Ok(())
}
#[test]
fn missing_old_advisory_or_reappeared_collection_never_creates_custody_or_releases_credit()
-> Result<()> {
    let f = Fixture::new()?;
    let original = f.grants(&f.registration.collection)?;
    let healthy = f.grants(&f.healthy.collection)?;
    fs::remove_dir_all(f.directory())?;
    let lease = f.lease()?;
    assert!(
        AbsentCollectionSettlement::settle_with_hook(
            &f.registration,
            &lease,
            &f.control,
            f.namespace,
            |point| {
                if point == SettlementPoint::BeforeUnlink {
                    fs::create_dir(f.directory())?;
                    fs::write(f.directory().join("foreign"), b"preserved")?;
                }
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(fs::read(f.directory().join("foreign"))?, b"preserved");
    assert_eq!(f.grants(&f.registration.collection)?, original);
    fs::remove_dir_all(f.directory())?;
    let advisory = f
        .quota
        .root
        .join("collections-1.4/.writer-locks")
        .join(&f.registration.collection);
    fs::remove_file(&advisory)?;
    assert!(
        AbsentCollectionSettlement::settle(&f.registration, &lease, &f.control, f.namespace)
            .is_err()
    );
    assert!(!advisory.exists() && !f.directory().exists());
    assert_eq!(f.grants(&f.registration.collection)?, original);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    f.assert_sources()?;
    Ok(())
}
#[test]
fn admitted_funding_is_preserved_instead_of_implicitly_adopted_for_retirement() -> Result<()> {
    use super::super::funding::{FundingBudget, FundingRequest};
    use airk_core::index_catalog::IndexId;
    let f = Fixture::new()?;
    let first = IndexId::from_identity(Incarnation::from_bytes([81; 16]));
    let second = IndexId::from_identity(Incarnation::from_bytes([82; 16]));
    let data = f.quota.root.to_str().unwrap().to_owned();
    let request = FundingRequest {
        operation_id: Incarnation::from_bytes([83; 16]),
        request_digest: Sha256Digest::from_bytes([84; 32]),
        namespace: f.namespace,
        original_index: first,
        original: f.registration.clone(),
        destination_index: second,
        destination: f.healthy.clone(),
        budgets: vec![
            FundingBudget {
                index_id: first,
                bytes: 2 * 1024 * 1024,
            },
            FundingBudget {
                index_id: second,
                bytes: 2 * 1024 * 1024,
            },
        ],
        quota_digests: BTreeMap::from([(data, f.quota.funding_policy_digest()?)]),
    };
    let funded = f.quota.reserve_funding(&request)?;
    let original = f.grants(&f.registration.collection)?;
    let healthy = f.grants(&f.healthy.collection)?;
    fs::remove_dir_all(f.directory())?;
    let lease = f.lease()?;
    assert!(
        AbsentCollectionSettlement::settle(&f.registration, &lease, &f.control, f.namespace)
            .is_err()
    );
    assert_eq!(f.quota.observe_funding(request.operation_id)?, funded);
    assert_eq!(f.grants(&f.registration.collection)?, original);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    f.assert_sources()?;
    Ok(())
}

#[test]
fn held_existing_collection_advisory_refuses_disposed_current_grant_release() -> Result<()> {
    let f = Fixture::new()?;
    let original = f.grants(&f.registration.collection)?;
    let healthy = f.grants(&f.healthy.collection)?;
    let old_writer = crate::service::acquire_existing_file(
        &f.quota.root.join("collections-1.4/.writer-locks"),
        &f.registration.collection,
    )?;
    fs::remove_dir_all(f.directory())?;
    let lease = f.lease()?;
    assert!(
        AbsentCollectionSettlement::settle(&f.registration, &lease, &f.control, f.namespace)
            .is_err()
    );
    assert_eq!(f.grants(&f.registration.collection)?, original);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    drop(old_writer);
    drop(AbsentCollectionSettlement::settle(
        &f.registration,
        &lease,
        &f.control,
        f.namespace,
    )?);
    assert!(f.grants(&f.registration.collection)?.is_empty());
    f.assert_sources()?;
    Ok(())
}

#[test]
fn metadata_only_shared_root_retirement_never_disposes_credit_without_advisory() -> Result<()> {
    let f = Fixture::new()?;
    let healthy = f.grants(&f.healthy.collection)?;
    let selected = f.grants(&f.registration.collection)?;
    fs::remove_dir_all(f.directory())?;
    let advisory = f
        .quota
        .root
        .join("collections-1.4/.writer-locks")
        .join(&f.registration.collection);
    fs::remove_file(&advisory)?;
    let lease = f.lease()?;
    // Actual selected native credits still require actual advisory custody.
    assert!(
        AbsentCollectionSettlement::settle(&f.registration, &lease, &f.control, f.namespace)
            .is_err()
    );
    assert_eq!(f.grants(&f.registration.collection)?, selected);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    drop(lease);
    // Populate an actual B-only live operation through the production quota API.
    // Its presence must be fully validated and preserved, not mistaken for A effects.
    let first = airk_core::index_catalog::IndexId::from_identity(Incarnation::from_bytes([11; 16]));
    let second =
        airk_core::index_catalog::IndexId::from_identity(Incarnation::from_bytes([12; 16]));
    let third = ProjectRegistration::new(
        f.control
            .parent()
            .unwrap()
            .join("third-source")
            .to_str()
            .unwrap()
            .into(),
        f.registration.data_root.clone(),
        "funded-c".into(),
    )?;
    let funding = f.quota.reserve_funding(&funding::FundingRequest {
        operation_id: Incarnation::from_bytes([13; 16]),
        request_digest: Sha256Digest::from_bytes([14; 32]),
        namespace: f.namespace,
        original_index: first,
        original: f.healthy.clone(),
        destination_index: second,
        destination: third,
        budgets: vec![
            funding::FundingBudget {
                index_id: first,
                bytes: 4096,
            },
            funding::FundingBudget {
                index_id: second,
                bytes: 4096,
            },
        ],
        quota_digests: BTreeMap::from([(
            f.registration.data_root.clone(),
            f.quota.funding_policy_digest()?,
        )]),
    })?;
    // A genuinely never-activated sibling has neither advisory nor credits.
    let mut unstarted = f.registration.clone();
    unstarted.collection = "never-activated".into();
    unstarted =
        ProjectRegistration::new(unstarted.root, unstarted.data_root, unstarted.collection)?;
    let lease = ProjectLease::acquire(&f.control, unstarted.binding)?;
    let proof = AbsentCollectionSettlement::settle(&unstarted, &lease, &f.control, f.namespace)?;
    proof.revalidate()?;
    assert!(
        !f.quota
            .root
            .join("collections-1.4/.writer-locks/never-activated")
            .exists()
    );
    assert_eq!(f.grants(&f.registration.collection)?, selected);
    assert_eq!(f.grants(&f.healthy.collection)?, healthy);
    drop(proof);
    assert_eq!(
        f.quota.observe_funding(funding.request.operation_id)?,
        funding
    );
    f.assert_sources()?;
    Ok(())
}
