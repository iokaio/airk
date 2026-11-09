// SPDX-License-Identifier: Apache-2.0
//! Real producer replay, custody and cross-root funding boundaries.
use super::*;

#[test]
fn clean_break_storage_empty_namespace_requires_actual_empty_saved_registry_and_exact_namespace()
-> Result<()> {
    let temporary = tempfile::tempdir()?;
    let control = temporary.path().canonicalize()?.join("control");
    fs::create_dir(&control)?;
    let namespace = Incarnation::from_bytes([111; 16]);
    let service = ServiceLease::acquire(&control)?;
    drop(ServiceRegistry::open(&control, namespace)?);
    let database = control.join("service-1.4.sqlite3");
    let before = fs::read(&database)?;
    assert!(
        CleanBreakNamespaceOwner::acquire(&service, Incarnation::from_bytes([112; 16]), &[])
            .is_err()
    );
    assert!(!control.join("source-watch-1.4.lock").exists());
    assert!(!control.join("embedding-1.4.lock").exists());
    let owner = CleanBreakNamespaceOwner::acquire(&service, namespace, &[])?;
    assert!(
        owner.registrations().is_empty() && owner.roots.is_empty() && owner.projects.is_empty()
    );
    drop(owner);
    assert_eq!(fs::read(&database)?, before);

    let fixture = Fixture::new()?;
    let before = fs::read(fixture.data.join(OWNER))?;
    assert!(CleanBreakNamespaceOwner::acquire(&fixture.service, fixture.namespace, &[]).is_err());
    assert!(
        CleanBreakNamespaceOwner::acquire(
            &fixture.service,
            fixture.namespace,
            &fixture.registrations[..1]
        )
        .is_err()
    );
    assert_eq!(fs::read(fixture.data.join(OWNER))?, before);
    fixture.preserved()
}

#[test]
fn clean_break_storage_missing_root_without_admitted_receipt_is_not_success() -> Result<()> {
    let fixture = Fixture::new()?;
    let displaced = fixture.data.parent().unwrap().join("externally-moved-data");
    fs::rename(&fixture.data, &displaced)?;
    assert!(
        validate_clean_break_storage(
            &fixture.data,
            fixture.service.root(),
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert!(displaced.join(OWNER).is_file());
    fixture.preserved()
}

#[test]
fn clean_break_storage_requires_existing_lifecycle_without_creating_a_replacement() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.purge()?;
    let lifecycle = fixture.data.join(".lifecycle-1.4.lock");
    fs::remove_file(&lifecycle)?;
    assert!(
        validate_clean_break_storage(
            &fixture.data,
            fixture.service.root(),
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert!(!lifecycle.exists());
    assert!(fixture.data.join(OWNER).is_file());
    fixture.preserved()
}

#[test]
fn clean_break_storage_exact_receipt_resumes_a_pair_split_at_the_64_file_boundary() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.purge()?;
    let quota = Quota::open_service(&fixture.data)?;
    let mut reservations = Vec::new();
    for _ in 0..33 {
        reservations.push(quota.reserve_root(4096)?);
    }
    // A real failed destructor leaves the producer's exact row/lease pair after
    // process custody closes. No row bytes, publication or ownership is invented.
    let ledger = OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.data.join(".quota-1.4/ledger.lock"))?;
    ledger.try_lock()?;
    crate::quota::with_lock_admission(
        std::time::Duration::from_millis(1),
        || Ok(()),
        |_| {},
        || {
            drop(reservations);
            Ok(())
        },
    )?;
    drop(ledger);
    let records = fixture.data.join(".quota-1.4/reservations");
    assert_eq!(fs::read_dir(&records)?.count(), 66);
    assert!(!retire_clean_break_storage_turn(
        &fixture.data,
        &fixture.service,
        fixture.namespace,
        &fixture.registrations
    )?);
    let remaining = fs::read_dir(&records)?.collect::<std::io::Result<Vec<_>>>()?;
    assert!(
        remaining.iter().any(|entry| entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "lease")
            && !entry.path().with_extension("").exists()),
        "64-file turn must exercise an interrupted row/lease pair"
    );
    validate_clean_break_storage(
        &fixture.data,
        fixture.service.root(),
        fixture.namespace,
        &fixture.registrations,
    )?;
    fixture.finish()?;
    fixture.preserved()
}

#[test]
fn clean_break_storage_read_census_resumes_after_owner_unlink_before_root_unlink() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.purge()?;
    assert!(
        retire_with_hook(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations,
            |effect| {
                if effect && !fixture.data.join(OWNER).exists() {
                    anyhow::bail!("interrupted after original owner unlink")
                }
                Ok(())
            }
        )
        .is_err()
    );
    assert!(fixture.data.is_dir());
    assert!(!fixture.data.join(OWNER).exists());
    assert!(fixture.data.join(".lifecycle-1.4.lock").is_file());
    validate_clean_break_storage(
        &fixture.data,
        fixture.service.root(),
        fixture.namespace,
        &fixture.registrations,
    )?;
    fixture.finish()?;
    validate_clean_break_storage(
        &fixture.data,
        fixture.service.root(),
        fixture.namespace,
        &fixture.registrations,
    )?;
    fixture.preserved()
}

#[test]
fn clean_break_storage_marker_rejects_changed_known_metadata_bytes_readonly() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.purge()?;
    assert!(
        retire_with_hook(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations,
            |_| anyhow::bail!("interrupted after durable roster")
        )
        .is_err()
    );
    fs::write(fixture.data.join(OWNER), b"changed retained owner bytes")?;
    assert!(
        validate_clean_break_storage(
            &fixture.data,
            fixture.service.root(),
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert_eq!(
        fs::read(fixture.data.join(OWNER))?,
        b"changed retained owner bytes"
    );
    fixture.preserved()
}

fn pending_funding(
    fixture: &Fixture,
    destination: ProjectRegistration,
) -> Result<(Quota, crate::quota::funding::FundingRequest)> {
    use crate::quota::funding::{FundingBudget, FundingRequest};
    use airk_core::index_catalog::IndexId;
    let quota = Quota::open_service(&fixture.data)?;
    let original_index = IndexId::from_identity(Incarnation::from_bytes([101; 16]));
    let destination_index = IndexId::from_identity(Incarnation::from_bytes([102; 16]));
    let mut quota_digests = std::collections::BTreeMap::new();
    for registration in [&fixture.registrations[0], &destination] {
        let root_quota = Quota::open_service(&registration.data_root)?;
        quota_digests.insert(
            registration.data_root.clone(),
            root_quota.funding_policy_digest()?,
        );
    }
    Ok((
        quota,
        FundingRequest {
            operation_id: Incarnation::from_bytes([103; 16]),
            request_digest: Sha256Digest::from_bytes([104; 32]),
            namespace: fixture.namespace,
            original_index,
            original: fixture.registrations[0].clone(),
            destination_index,
            destination,
            budgets: vec![
                FundingBudget {
                    index_id: original_index,
                    bytes: 1024 * 1024,
                },
                FundingBudget {
                    index_id: destination_index,
                    bytes: 1024 * 1024,
                },
            ],
            quota_digests,
        },
    ))
}

#[test]
fn clean_break_storage_pending_funding_is_discarded_only_after_actual_lease_closes() -> Result<()> {
    use crate::quota::funding::FundingPhase;
    use airk_core::catalog_split::SplitJobPurpose;
    let fixture = Fixture::new()?;
    fixture.purge()?;
    let (quota, request) = pending_funding(&fixture, fixture.registrations[1].clone())?;
    let receipt = quota.reserve_funding(&request)?;
    assert!(!receipt.complete);
    let owner = quota.begin_funding_phase(
        &receipt,
        request.destination_index,
        FundingPhase {
            revision: 2,
            purpose: SplitJobPurpose::DestinationBuild,
            job: Sha256Digest::from_bytes([105; 32]),
        },
    )?;
    let funded = quota.clone().with_funding(owner)?;
    let reservation = funded.reserve(&request.destination.collection, 4096)?;
    let before = fs::read(fixture.data.join(OWNER))?;
    validate_clean_break_storage(
        &fixture.data,
        fixture.service.root(),
        fixture.namespace,
        &fixture.registrations,
    )?;
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert_eq!(fs::read(fixture.data.join(OWNER))?, before);
    drop(reservation);
    let closed = quota.observe_funding(request.operation_id)?;
    assert!(!closed.complete && closed.children.iter().all(|child| child.closed));
    fixture.finish()?;
    fixture.preserved()
}

#[test]
fn clean_break_storage_refuses_foreign_remote_funding_request_and_orphan_lease() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.purge()?;
    let remote = fixture.data.parent().unwrap().join("remote-data");
    bind(&remote, fixture.service.root(), fixture.namespace)?;
    let foreign = ProjectRegistration::new(
        fixture
            .source
            .join("unregistered-remote")
            .to_str()
            .unwrap()
            .into(),
        remote.to_str().unwrap().into(),
        "remote".into(),
    )?;
    let (quota, request) = pending_funding(&fixture, foreign)?;
    let receipt = quota.reserve_funding(&request)?;
    assert_eq!(
        receipt.participants.len(),
        1,
        "foreign remote request is not a local participant"
    );
    let before = fs::read(fixture.data.join(OWNER))?;
    assert!(
        validate_clean_break_storage(
            &fixture.data,
            fixture.service.root(),
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert_eq!(fs::read(fixture.data.join(OWNER))?, before);
    assert!(remote.join(OWNER).is_file());
    fixture.preserved()?;

    let fixture = Fixture::new()?;
    fixture.purge()?;
    let (quota, request) = pending_funding(&fixture, fixture.registrations[1].clone())?;
    quota.reserve_funding(&request)?;
    let leases = fixture.data.join(".quota-1.4/split-allocations/leases");
    fs::create_dir(&leases)?;
    let foreign_lease = leases.join(format!("l-{}.lease", Sha256Digest::from_bytes([106; 32])));
    fs::write(&foreign_lease, [])?;
    assert!(
        validate_clean_break_storage(
            &fixture.data,
            fixture.service.root(),
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert!(
        retire_clean_break_storage_turn(
            &fixture.data,
            &fixture.service,
            fixture.namespace,
            &fixture.registrations
        )
        .is_err()
    );
    assert!(foreign_lease.is_file() && fixture.data.join(OWNER).is_file());
    fixture.preserved()
}

#[test]
fn clean_break_storage_known_cross_root_pending_funding_keeps_remote_storage_untouched()
-> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.purge()?;
    let remote = fixture.data.parent().unwrap().join("known-remote-data");
    let source = fixture.source.join("known-remote-source");
    fs::create_dir(&source)?;
    fs::write(source.join("saved.txt"), b"saved source")?;
    bind(&remote, fixture.service.root(), fixture.namespace)?;
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        remote.to_str().unwrap().into(),
        "remote".into(),
    )?;
    let mut registry = ServiceRegistry::open(fixture.service.root(), fixture.namespace)?;
    registry.register(&registration)?;
    let mut project = ProjectLease::acquire(fixture.service.root(), registration.binding)?;
    registry.admit_worker(&mut project)?;
    registry.record_watch_roots(&project, &[])?;
    drop(project);
    drop(registry);
    fixture.registrations.push(registration.clone());
    let (quota, request) = pending_funding(&fixture, registration)?;
    let funding = quota.reserve_funding(&request)?;
    assert_eq!(funding.participants.len(), 1);
    assert!(!funding.complete);
    let before = fs::read(remote.join(OWNER))?;
    validate_clean_break_storage(
        &fixture.data,
        fixture.service.root(),
        fixture.namespace,
        &fixture.registrations,
    )?;
    fixture.finish()?;
    assert_eq!(fs::read(remote.join(OWNER))?, before);
    fixture.preserved()
}

#[test]
fn clean_break_storage_pre_admission_budget_observation_refuses_aggregate_and_encoding_overflow()
-> Result<()> {
    let fixture = Fixture::new()?;
    let (mut observation, _) = capture_retirement(
        &fixture.data,
        fixture.service.root(),
        fixture.namespace,
        &fixture.registrations,
        false,
        false,
    )?;
    prospective_budget(&observation, &fixture.data, &fixture.registrations)?;
    // Pure sizing projections are not native identities or deletion authority.
    // Exercise aggregate/encoding decisions cheaply without creating thousands
    // of roots or publishing a synthetic retirement receipt.
    let base = observation.files.len();
    for index in 0..2000 {
        observation.files.push(RetiredFile {
            path: vec![
                ".quota-1.4".into(),
                "reservations".into(),
                format!("r-{index:04}{}", "x".repeat(120)),
            ],
            identity: (0, 0),
            bytes: 0,
            digest: Sha256Digest::from_bytes([0; 32]),
            lock: false,
        });
    }
    assert!(observation.files.len() + observation.directories.len() < MAX_FILES);
    assert!(serde_json::to_vec(&observation)?.len() as u64 > MAX_RECEIPT);
    assert!(prospective_budget(&observation, &fixture.data, &fixture.registrations).is_err());
    observation.files.truncate(base);
    while observation.files.len() + observation.directories.len() <= MAX_FILES {
        observation.files.push(RetiredFile {
            path: vec![format!("budget-{}", observation.files.len())],
            identity: (0, 0),
            bytes: 0,
            digest: Sha256Digest::from_bytes([0; 32]),
            lock: false,
        });
    }
    assert!(prospective_budget(&observation, &fixture.data, &fixture.registrations).is_err());
    assert!(
        !fixture
            .service
            .root()
            .join(marker(&fixture.data, fixture.namespace)?)
            .exists()
    );
    validate_clean_break_storage(
        &fixture.data,
        fixture.service.root(),
        fixture.namespace,
        &fixture.registrations,
    )?;
    fixture.preserved()
}
