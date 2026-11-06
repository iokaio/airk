// SPDX-License-Identifier: Apache-2.0
use super::*;

const BUDGET: u64 = 1024 * 1024;

struct Fixture {
    _temp: tempfile::TempDir,
    quota: Quota,
    request: FundingRequest,
}
impl Fixture {
    fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let base = temp.path().canonicalize()?;
        let data = base.join("data");
        let control = base.join("control");
        fs::create_dir(&control)?;
        let namespace = Incarnation::from_bytes([3; 16]);
        crate::service::data_scope::bind(&data, &control, namespace)?;
        let quota = Quota::open_service(&data)?;
        let first = IndexId::from_identity(Incarnation::from_bytes([1; 16]));
        let second = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
        let data_text = quota.root.to_str().unwrap().to_owned();
        let request = FundingRequest {
            operation_id: Incarnation::from_bytes([4; 16]),
            request_digest: Sha256Digest::from_bytes([5; 32]),
            namespace,
            original_index: first,
            original: ProjectRegistration::new(
                base.join("source-a").to_str().unwrap().into(),
                data_text.clone(),
                "original".into(),
            )?,
            destination_index: second,
            destination: ProjectRegistration::new(
                base.join("source-b").to_str().unwrap().into(),
                data_text.clone(),
                "destination".into(),
            )?,
            budgets: vec![
                FundingBudget {
                    index_id: first,
                    bytes: BUDGET,
                },
                FundingBudget {
                    index_id: second,
                    bytes: BUDGET,
                },
            ],
            quota_digests: BTreeMap::from([(data_text, quota.funding_policy_digest()?)]),
        };
        Ok(Self {
            _temp: temp,
            quota,
            request,
        })
    }
    fn destination(&self) -> Result<(FundingUse, Quota)> {
        let receipt = self.quota.reserve_funding(&self.request)?;
        let owner = self.quota.begin_funding_phase(
            &receipt,
            self.request.destination_index,
            FundingPhase {
                revision: 2,
                purpose: SplitJobPurpose::DestinationBuild,
                job: Sha256Digest::from_bytes([6; 32]),
            },
        )?;
        Ok((owner.clone(), self.quota.clone().with_funding(owner)?))
    }
    fn destination_store(&self) -> crate::FileStore {
        crate::FileStore::new(self.quota.root.join("collections-1.4/destination"))
    }
}

#[test]
fn closed_preallocation_settles_after_publication_faults_and_sibling_progress() -> Result<()> {
    for boundary in [1, 2] {
        for fail_after_replace in [false, true] {
            let fixture = Fixture::new()?;
            let (owner, funded) = fixture.destination()?;
            let before = funded.observe_funding(owner.operation_id)?;
            let id = reservation_identity(&owner, before.revision)?;
            let lease_path = funded
                .control_path()
                .join(DIRECTORY)
                .join("leases")
                .join(format!("l-{id}.lease"));
            let mut publication = 0;
            {
                let _ledger = funded.lock()?;
                let result = allocate_prepared(
                    &funded,
                    &owner,
                    FundingChildKind::Reservation {
                        staging_name: String::new(),
                    },
                    4096,
                    false,
                    None,
                    |point| {
                        if matches!(point, PersistPoint::BeforeReplace) {
                            publication += 1;
                        }
                        if publication == 1 {
                            assert!(
                                !lease_path.exists(),
                                "closed preallocation must precede lease creation"
                            );
                        } else {
                            let lease = OpenOptions::new()
                                .read(true)
                                .write(true)
                                .open(&lease_path)?;
                            assert_eq!(lease.metadata()?.len(), 0);
                            assert!(matches!(
                                lease.try_lock(),
                                Err(std::fs::TryLockError::WouldBlock)
                            ));
                        }
                        if publication == boundary
                            && matches!(point, PersistPoint::AfterReplace) == fail_after_replace
                        {
                            anyhow::bail!("interrupted funded reservation publication");
                        }
                        Ok(())
                    },
                );
                assert!(result.is_err());
            }
            let published = funded.observe_funding(owner.operation_id)?;
            if boundary == 1 && !fail_after_replace {
                assert_eq!(published, before);
                assert!(!lease_path.exists());
                continue;
            }
            assert_eq!(published.children.len(), 1);
            assert_eq!(published.children[0].id, id);
            assert_eq!(
                published.children[0].closed,
                boundary == 1 || !fail_after_replace
            );
            assert_eq!(lease_path.exists(), boundary == 2);
            // Real independent B work advances this same funding record before
            // A recovers. A's recorded child identity cannot become an orphan.
            let peer_owner = funded.begin_funding_phase(
                &published,
                fixture.request.original_index,
                FundingPhase {
                    revision: 2,
                    purpose: SplitJobPurpose::OriginalRemoval,
                    job: Sha256Digest::from_bytes([7; 32]),
                },
            )?;
            let peer_quota = fixture.quota.clone().with_funding(peer_owner)?;
            let peer = peer_quota.reserve("original", 2048)?;
            peer.charge(4)?;
            let peer_stage = peer.staging_directory()?;
            fs::write(peer_stage.join("peer"), b"keep")?;
            let advanced = funded.observe_funding(owner.operation_id)?;
            assert!(advanced.revision > published.revision);
            let peer_children = advanced
                .children
                .iter()
                .filter(|child| child.index_id == fixture.request.original_index)
                .cloned()
                .collect::<Vec<_>>();
            let mut recovery = owner.clone();
            recovery.phase = FundingPhase {
                revision: 3,
                purpose: SplitJobPurpose::DestinationBuild,
                job: Sha256Digest::from_bytes([8; 32]),
            };
            let transferred = funded.transfer_funded_children(
                &advanced,
                &recovery,
                FundingTakeoverKind::FailedWorkRetry,
                |_| Ok(()),
            )?;
            assert_eq!(
                transferred
                    .children
                    .iter()
                    .find(|child| child.index_id == owner.index_id)
                    .unwrap()
                    .id,
                id
            );
            let recovering = fixture.quota.clone().with_funding(recovery)?;
            let recovery_owner = recovering.funding_use().unwrap();
            let closed = recovering.close_abandoned_funded_reservations(recovery_owner)?;
            assert_eq!(closed, usize::from(boundary == 2 && fail_after_replace));
            let store = fixture.destination_store();
            let writer = store.lock()?;
            let settled = recovering.reconcile_funding_phase(&writer, recovery_owner, None)?;
            assert!(
                settled
                    .children
                    .iter()
                    .all(|child| child.index_id != owner.index_id)
            );
            assert_eq!(
                settled
                    .children
                    .iter()
                    .filter(|child| child.index_id == fixture.request.original_index)
                    .cloned()
                    .collect::<Vec<_>>(),
                peer_children
            );
            assert_eq!(settled.participants[0], advanced.participants[0]);
            assert_eq!(
                settled.participants[1].remaining_growth + settled.participants[1].consumed_growth,
                BUDGET
            );
            assert!(!lease_path.exists());
            assert_eq!(fs::read(peer_stage.join("peer"))?, b"keep");
            let retry = recovering.reserve("destination", 2048)?;
            assert_ne!(retry.0.funded.as_ref().unwrap().child, id);
            drop(retry);
            drop(peer);
        }
    }
    Ok(())
}

#[test]
fn lease_setup_failure_preserves_closed_credit_and_foreign_entries() -> Result<()> {
    let fixture = Fixture::new()?;
    let (owner, funded) = fixture.destination()?;
    let before = funded.observe_funding(owner.operation_id)?;
    let lease_directory = funded.control_path().join(DIRECTORY).join("leases");
    {
        let _ledger = funded.lock()?;
        assert!(
            allocate_prepared(
                &funded,
                &owner,
                FundingChildKind::Reservation {
                    staging_name: String::new()
                },
                4096,
                false,
                None,
                |point| {
                    if matches!(point, PersistPoint::AfterReplace) {
                        // An actual filesystem collision makes lease creation fail.
                        fs::write(&lease_directory, b"foreign entry stays intact")?;
                    }
                    Ok(())
                },
            )
            .is_err()
        );
    }
    let prepared = funded.observe_funding(owner.operation_id)?;
    assert_eq!(prepared.children.len(), 1);
    assert!(prepared.children[0].closed);
    assert_eq!(prepared.participants[0], before.participants[0]);
    assert_eq!(
        prepared.participants[1].remaining_growth + prepared.children[0].held_bytes,
        BUDGET
    );
    assert_eq!(fs::read(&lease_directory)?, b"foreign entry stays intact");
    assert_eq!(funded.close_abandoned_funded_reservations(&owner)?, 0);
    let store = fixture.destination_store();
    let writer = store.lock()?;
    assert!(
        funded
            .reconcile_funding_phase(&writer, &owner, None)
            .is_err()
    );
    assert_eq!(funded.observe_funding(owner.operation_id)?, prepared);
    assert_eq!(fs::read(&lease_directory)?, b"foreign entry stays intact");
    // The fixture, not recovery, removes its deliberate foreign obstruction.
    fs::remove_file(&lease_directory)?;
    let settled = funded.reconcile_funding_phase(&writer, &owner, None)?;
    assert!(settled.children.is_empty());
    assert_eq!(
        settled.participants[1].remaining_growth + settled.participants[1].consumed_growth,
        BUDGET
    );
    Ok(())
}

#[test]
fn published_open_missing_lease_refuses_recovery_and_adoption() -> Result<()> {
    let fixture = Fixture::new()?;
    let (owner, funded) = fixture.destination()?;
    let (child, prepared) = {
        let _ledger = funded.lock()?;
        allocate_prepared(
            &funded,
            &owner,
            FundingChildKind::Reservation {
                staging_name: String::new(),
            },
            4096,
            false,
            None,
            |_| Ok(()),
        )?
    };
    drop(prepared.context("prepared lease missing")?.published()?);
    let before = funded.observe_funding(owner.operation_id)?;
    let path = funded
        .control_path()
        .join(DIRECTORY)
        .join("leases")
        .join(format!("l-{}.lease", child.child));
    fs::remove_file(&path)?;
    assert!(funded.close_abandoned_funded_reservations(&owner).is_err());
    assert!(
        funded
            .adopt_funded_reservation(&owner, child.child)
            .is_err()
    );
    assert_eq!(funded.observe_funding(owner.operation_id)?, before);
    assert!(!path.exists());
    // Adoption of a CLOSED preallocation may create custody, but an uncertain
    // OPEN commit must return its error without dropping a Reservation under
    // the ledger (which would recursively lock it), or deleting its disk lease.
    let fixture = Fixture::new()?;
    let (owner, funded) = fixture.destination()?;
    {
        let _ledger = funded.lock()?;
        assert!(
            allocate_prepared(
                &funded,
                &owner,
                FundingChildKind::Reservation {
                    staging_name: String::new()
                },
                4096,
                false,
                None,
                |point| {
                    if matches!(point, PersistPoint::AfterReplace) {
                        anyhow::bail!("closed preallocation committed");
                    }
                    Ok(())
                },
            )
            .is_err()
        );
    }
    let prepared = funded.observe_funding(owner.operation_id)?;
    let child = &prepared.children[0];
    assert!(child.closed);
    assert!(
        funded
            .adopt_funded_reservation_observed(&owner, child.id, |point| {
                if matches!(point, PersistPoint::AfterReplace) {
                    anyhow::bail!("adopted OPEN commit reply lost");
                }
                Ok(())
            })
            .is_err()
    );
    let adopted = funded.observe_funding(owner.operation_id)?;
    assert!(!adopted.children[0].closed);
    assert_eq!(funded.close_abandoned_funded_reservations(&owner)?, 1);
    Ok(())
}

#[test]
fn parent_credit_transfers_atomically_and_cannot_fund_another_collection() -> Result<()> {
    let fixture = Fixture::new()?;
    let (owner, funded) = fixture.destination()?;
    let before = fixture.quota.status()?;
    assert_eq!(before.reserved_bytes, 2 * BUDGET);
    let child = funded.reserve("destination", BUDGET / 4)?;
    assert_eq!(
        fixture.quota.status()?.reserved_bytes,
        before.reserved_bytes
    );
    assert!(funded.reserve("original", 1).is_err());
    assert!(funded.reserve("destination", BUDGET).is_err());
    let receipt = fixture.quota.observe_funding(owner.operation_id)?;
    assert_eq!(receipt.children.len(), 1);
    assert_eq!(receipt.participants[1].remaining_growth, 3 * BUDGET / 4);
    assert_eq!(receipt.children[0].held_bytes, BUDGET / 4);
    drop(child);
    // Drop is neither journal settlement nor permission to reuse this credit.
    assert_eq!(fixture.quota.status()?.reserved_bytes, 2 * BUDGET);
    assert!(fixture.quota.observe_funding(owner.operation_id)?.children[0].closed);
    Ok(())
}

#[test]
fn exact_ordinary_child_adoption_preserves_charge_and_excludes_a_live_owner() -> Result<()> {
    let fixture = Fixture::new()?;
    let (owner, funded) = fixture.destination()?;
    let child = funded.reserve("destination", 4096)?;
    child.charge(32)?;
    let stage = child.staging_directory()?;
    fs::write(stage.join("captured"), [7; 32])?;
    let saved = fixture.quota.funded_children(&owner)?;
    assert_eq!(saved.len(), 1);
    assert!(
        fixture
            .quota
            .adopt_funded_reservation(&owner, saved[0].id)
            .is_err()
    );
    drop(child);
    assert!(stage.join("captured").is_file());
    let reopened = Quota::open_service(&fixture.quota.root)?;
    assert_eq!(reopened.status()?.reserved_bytes, 2 * BUDGET);
    let receipt = reopened.observe_funding(owner.operation_id)?;
    let resumed = reopened.adopt_funding_phase(&receipt, owner.index_id, &owner.phase)?;
    let adopted = reopened.adopt_funded_reservation(&resumed, saved[0].id)?;
    assert_eq!(adopted.charged(), 32);
    assert_eq!(adopted.staging_directory()?, stage);
    let mut foreign = resumed.clone();
    foreign.request_digest = Sha256Digest::from_bytes([8; 32]);
    assert!(
        reopened
            .adopt_funded_reservation(&foreign, saved[0].id)
            .is_err()
    );
    drop(adopted);
    // Fresh captures after adoption use persisted allocation sequence, not a
    // reset process-local counter that could collide with the saved child.
    let extra = reopened
        .with_funding(resumed)?
        .reserve("destination", 2048)?;
    assert_ne!(extra.staging_directory()?, stage);
    drop(extra);
    Ok(())
}

#[test]
fn funded_current_rebind_survives_restart_and_ordinary_orphan_release_is_refused() -> Result<()> {
    let fixture = Fixture::new()?;
    let (owner, funded) = fixture.destination()?;
    let store = fixture.destination_store();
    let writer = store.lock()?;
    let scope = CurrentGrantScope {
        collection: "destination".into(),
        incarnation: Incarnation::from_bytes([7; 16]),
        transaction: Sha256Digest::from_bytes([9; 32]),
    };
    let grant = funded.reserve_current(&writer, scope.clone(), 65536)?;
    let stale = funded.current_grant(&writer, &scope)?.unwrap();
    let mut published = scope.clone();
    published.transaction = Sha256Digest::from_bytes([10; 32]);
    let grant = grant.rebind(&writer, published.clone())?;
    drop(grant);
    assert!(stale.release(&writer).is_err());
    let ordinary = Quota::open_service(&fixture.quota.root)?;
    assert!(ordinary.current_grant(&writer, &scope)?.is_none());
    let orphan = ordinary
        .current_grant(&writer, &published)?
        .context("funded current lost after restart")?;
    assert!(orphan.require_release_authority().is_err());
    assert!(orphan.release(&writer).is_err());
    assert_eq!(ordinary.status()?.reserved_bytes, 2 * BUDGET);
    let receipt = ordinary.observe_funding(owner.operation_id)?;
    let resumed = ordinary.adopt_funding_phase(&receipt, owner.index_id, &owner.phase)?;
    let recovered = ordinary.with_funding(resumed)?;
    recovered
        .current_grant(&writer, &published)?
        .unwrap()
        .release(&writer)?;
    assert_eq!(recovered.status()?.reserved_bytes, 2 * BUDGET);
    let receipt = recovered.observe_funding(owner.operation_id)?;
    assert!(receipt.children[0].closed);
    Ok(())
}

#[test]
fn single_record_fault_boundary_recovers_old_or_new_whole_allocation() -> Result<()> {
    let fixture = Fixture::new()?;
    let before = fixture.quota.reserve_funding(&fixture.request)?;
    let mut after = before.clone();
    after.revision += 1;
    for fail_after_replace in [false, true] {
        let failure = persist_observed(&fixture.quota, &after, |point| {
            if matches!(point, PersistPoint::AfterReplace) == fail_after_replace {
                anyhow::bail!("injected exact funding persistence boundary");
            }
            Ok(())
        });
        assert!(failure.is_err());
        let reopened = Quota::open_service(&fixture.quota.root)?;
        assert_eq!(
            reopened.observe_funding(before.request.operation_id)?,
            if fail_after_replace {
                after.clone()
            } else {
                before.clone()
            }
        );
        assert_eq!(reopened.status()?.reserved_bytes, 2 * BUDGET);
    }
    Ok(())
}

#[test]
fn bound_root_namespace_policy_and_malformed_inventory_fail_closed() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut request = fixture.request.clone();
    request.namespace = Incarnation::from_bytes([11; 16]);
    assert!(fixture.quota.reserve_funding(&request).is_err());
    request = fixture.request.clone();
    request
        .quota_digests
        .values_mut()
        .for_each(|value| *value = Sha256Digest::from_bytes([12; 32]));
    assert!(fixture.quota.reserve_funding(&request).is_err());
    let receipt = fixture.quota.reserve_funding(&fixture.request)?;
    let path = fixture
        .quota
        .control_path()
        .join(DIRECTORY)
        .join(receipt.filename());
    let mut malformed = receipt.clone();
    malformed.participants[0].remaining_growth -= 1;
    fs::write(&path, serde_json::to_vec(&malformed)?)?;
    assert!(fixture.quota.status().is_err());
    assert!(fixture.quota.validate_funding(&receipt).is_err());
    assert!(fixture.quota.reserve("unrelated", 1).is_err());
    assert_eq!(fs::read(&path)?, serde_json::to_vec(&malformed)?);
    Ok(())
}

#[test]
fn pinned_setup_and_deadline_inventory_observe_same_funding_without_sweeping() -> Result<()> {
    use crate::service::registry::catalog::setup::storage::SetupDirectory;
    let fixture = Fixture::new()?;
    let (owner, funded) = fixture.destination()?;
    let capture = funded.reserve("destination", 4096)?;
    drop(capture);
    let control = SetupDirectory::open_canonical(&fixture.quota.control_path())?;
    let root = SetupDirectory::open_canonical(&fixture.quota.root)?;
    let pinned = pinned_records(&control, &root, &fixture.quota.root)?;
    let ordinary = records(&fixture.quota, None)?;
    let total = |rows: &[LiveRecord]| rows.iter().map(|row| row.record.bytes).sum::<u64>();
    assert_eq!(total(&pinned), total(&ordinary));
    assert_eq!(total(&pinned), 2 * BUDGET);
    assert!(
        records(
            &fixture.quota,
            Some(Instant::now() - Duration::from_secs(1))
        )
        .is_err()
    );
    assert_eq!(fixture.quota.funded_children(&owner)?.len(), 1);
    let lease_dir = fixture.quota.control_path().join(DIRECTORY).join("leases");
    fs::write(lease_dir.join("unknown"), [])?;
    assert!(records(&fixture.quota, None).is_err());
    assert!(pinned_records(&control, &root, &fixture.quota.root).is_err());
    Ok(())
}

#[test]
fn settlement_checks_all_live_children_before_cleanup_and_preserves_other_owners() -> Result<()> {
    let fixture = Fixture::new()?;
    let (owner, funded) = fixture.destination()?;
    let store = fixture.destination_store();
    let writer = store.lock()?;
    let source = fixture.quota.root.parent().unwrap().join("source-a");
    fs::create_dir(&source)?;
    fs::write(source.join("original.txt"), b"source remains read only")?;
    let other = fixture.quota.root.join("collections-1.4/unrelated");
    fs::create_dir_all(&other)?;
    fs::write(other.join("retained"), b"another collection")?;
    let first = funded.reserve("destination", 4096)?;
    let first_stage = first.staging_directory()?;
    first.charge(16)?;
    fs::write(first_stage.join("capture"), [1; 16])?;
    drop(first);
    let live = funded.reserve("destination", 4096)?;
    let live_stage = live.staging_directory()?;
    assert!(
        funded
            .reconcile_funding_phase(&writer, &owner, None)
            .is_err()
    );
    assert!(first_stage.join("capture").is_file());
    assert!(live_stage.is_dir());
    // A frontend cannot obtain the native no-current cleanup capability merely
    // by opening the backing path or possessing a syntactically valid receipt.
    assert!(
        store
            .reconcile_operation_funding_without_current(&writer, &funded)
            .is_err()
    );
    assert!(first_stage.join("capture").is_file());
    drop(live);
    let receipt = funded.reconcile_funding_phase(&writer, &owner, None)?;
    assert!(receipt.children.is_empty());
    assert_eq!(
        receipt.participants[1].settled_revision,
        Some(owner.phase.revision)
    );
    assert!(!first_stage.exists() && !live_stage.exists());
    assert_eq!(
        fs::read(source.join("original.txt"))?,
        b"source remains read only"
    );
    assert_eq!(fs::read(other.join("retained"))?, b"another collection");
    let consumed = fs::metadata(store.directory().join("writer.lock"))?.len();
    let participant = &receipt.participants[1];
    assert_eq!(participant.consumed_growth, consumed);
    let status = fixture.quota.status()?;
    assert_eq!(
        status.collections["destination"].used_bytes,
        participant.baseline_used_bytes + consumed
    );
    assert_eq!(status.reserved_bytes + consumed, 2 * BUDGET);
    Ok(())
}

#[test]
fn cross_root_funding_is_independent_and_cannot_be_adopted_by_the_other_root() -> Result<()> {
    let fixture = Fixture::new()?;
    let base = fixture.quota.root.parent().unwrap();
    let second_root = base.join("destination-data");
    crate::service::data_scope::bind(
        &second_root,
        &base.join("control"),
        fixture.request.namespace,
    )?;
    let second = Quota::open_service(&second_root)?;
    let mut request = fixture.request.clone();
    request.destination = ProjectRegistration::new(
        request.destination.root.clone(),
        second.root.to_str().unwrap().into(),
        request.destination.collection.clone(),
    )?;
    request.quota_digests.insert(
        second.root.to_str().unwrap().into(),
        second.funding_policy_digest()?,
    );
    let original = fixture.quota.reserve_funding(&request)?;
    assert_eq!(fixture.quota.status()?.reserved_bytes, BUDGET);
    assert_eq!(second.status()?.reserved_bytes, 0);
    // No distributed atomicity: the coordinator must save and resume the first
    // root receipt if the second allocation fails before all roots are funded.
    assert_eq!(original.participants.len(), 1);
    let destination = second.reserve_funding(&request)?;
    assert_eq!(destination.participants.len(), 1);
    assert_ne!(original.identity()?, destination.identity()?);
    assert_eq!(second.status()?.reserved_bytes, BUDGET);
    let owner = second.begin_funding_phase(
        &destination,
        request.destination_index,
        FundingPhase {
            revision: 2,
            purpose: SplitJobPurpose::DestinationBuild,
            job: Sha256Digest::from_bytes([13; 32]),
        },
    )?;
    assert!(fixture.quota.clone().with_funding(owner).is_err());
    assert!(
        fixture
            .quota
            .service_rename_capacity(0, &BTreeMap::new())
            .is_err()
    );
    assert_eq!(
        fixture.quota.observe_funding(request.operation_id)?,
        original
    );
    Ok(())
}

#[test]
fn cleanup_takeover_refuses_live_capture_and_preserves_credit_and_exact_lineage() -> Result<()> {
    let fixture = Fixture::new()?;
    let (build, funded) = fixture.destination()?;
    let source = fixture.quota.root.parent().unwrap().join("source-a");
    fs::create_dir(&source)?;
    fs::write(source.join("preserved"), b"original source")?;
    let original = fixture.quota.root.join("collections-1.4/original");
    fs::create_dir_all(&original)?;
    fs::write(original.join("retained"), b"healthy index")?;
    let capture = funded.reserve("destination", 8192)?;
    capture.charge(32)?;
    let stage = capture.staging_directory()?;
    fs::write(stage.join("captured"), [4; 32])?;
    let mut cleanup = build.clone();
    cleanup.phase = FundingPhase {
        revision: 9,
        purpose: SplitJobPurpose::DestinationCleanup,
        job: Sha256Digest::from_bytes([21; 32]),
    };
    let before = fixture.quota.observe_funding(build.operation_id)?;
    assert!(
        fixture
            .quota
            .transfer_cleanup_children(&before, &cleanup, |_| Ok(()))
            .is_err()
    );
    assert_eq!(fixture.quota.observe_funding(build.operation_id)?, before);
    drop(capture);
    let before = fixture.quota.observe_funding(build.operation_id)?;
    let mut foreign = cleanup.clone();
    foreign.index_id = fixture.request.original_index;
    assert!(
        fixture
            .quota
            .transfer_cleanup_children(&before, &foreign, |_| Ok(()))
            .is_err()
    );
    let after = fixture
        .quota
        .transfer_cleanup_children(&before, &cleanup, |_| Ok(()))?;
    assert_eq!(after.identity()?, before.identity()?);
    assert_eq!(after.takeovers.len(), 1);
    assert_eq!(
        after.takeovers[0].previous_phase.as_ref(),
        Some(&build.phase)
    );
    assert_eq!(after.takeovers[0].next_phase, cleanup.phase);
    assert_eq!(after.participants[0], before.participants[0]);
    assert_eq!(
        after.participants[1].remaining_growth,
        before.participants[1].remaining_growth
    );
    assert_eq!(after.children[0].capacity, before.children[0].capacity);
    assert_eq!(after.children[0].charged, 32);
    assert_eq!(fixture.quota.status()?.reserved_bytes, 2 * BUDGET);
    assert!(stage.join("captured").is_file());
    assert!(fixture.quota.clone().with_funding(build).is_err());
    let adopted = fixture
        .quota
        .adopt_funded_reservation(&cleanup, after.children[0].id)?;
    assert_eq!(adopted.charged(), 32);
    drop(adopted);
    let store = fixture.destination_store();
    let writer = store.lock()?;
    let settled = fixture
        .quota
        .reconcile_funding_phase(&writer, &cleanup, None)?;
    assert!(!stage.exists());
    let consumed = fs::metadata(store.directory().join("writer.lock"))?.len();
    let participant = &settled.participants[1];
    assert_eq!(participant.consumed_growth, consumed);
    let status = fixture.quota.status()?;
    assert_eq!(
        status.collections["destination"].used_bytes,
        participant.baseline_used_bytes + consumed
    );
    assert_eq!(status.reserved_bytes + consumed, 2 * BUDGET);
    assert_eq!(fs::read(source.join("preserved"))?, b"original source");
    assert_eq!(fs::read(original.join("retained"))?, b"healthy index");
    Ok(())
}

#[test]
fn quota_policy_preview_never_creates_missing_storage_or_control_state() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    let missing = base.join("missing-data");
    let review = Quota::review_service_funding_policy(&missing)?;
    assert_eq!(review.policy, QuotaPolicy::default());
    assert_eq!(review.digest, policy_digest(&QuotaPolicy::default())?);
    assert!(!missing.exists());
    let existing = base.join("existing-data");
    fs::create_dir(&existing)?;
    Quota::review_service_funding_policy(&existing)?;
    assert!(fs::read_dir(&existing)?.next().is_none());
    fs::create_dir(existing.join(".quota-1.4"))?;
    let policy = QuotaPolicy {
        root_bytes: BUDGET * 8,
        default_collection_bytes: BUDGET * 4,
        collections: BTreeMap::new(),
    };
    fs::write(
        existing.join(".quota-1.4/policy.json"),
        serde_json::to_vec(&policy)?,
    )?;
    let review = Quota::review_service_funding_policy(&existing)?;
    assert_eq!(review.policy, policy);
    assert_eq!(fs::read_dir(existing.join(".quota-1.4"))?.count(), 1);
    Ok(())
}

#[test]
fn operation_takeover_faults_recover_whole_old_or_whole_new_child_custody() -> Result<()> {
    for (kind, fail_after_replace) in [
        FundingTakeoverKind::CanceledDestinationCleanup,
        FundingTakeoverKind::FailedWorkRetry,
    ]
    .into_iter()
    .flat_map(|kind| [false, true].into_iter().map(move |after| (kind, after)))
    {
        let fixture = Fixture::new()?;
        let (build, funded) = fixture.destination()?;
        let reservation = funded.reserve("destination", 4096)?;
        drop(reservation);
        let before = fixture.quota.observe_funding(build.operation_id)?;
        let mut cleanup = build.clone();
        cleanup.phase = FundingPhase {
            revision: 10,
            purpose: if kind == FundingTakeoverKind::FailedWorkRetry {
                SplitJobPurpose::DestinationBuild
            } else {
                SplitJobPurpose::DestinationCleanup
            },
            job: Sha256Digest::from_bytes([22; 32]),
        };
        let failure = fixture
            .quota
            .transfer_funded_children(&before, &cleanup, kind, |point| {
                if matches!(point, PersistPoint::AfterReplace) == fail_after_replace {
                    anyhow::bail!("injected exact takeover persistence boundary");
                }
                Ok(())
            });
        assert!(failure.is_err());
        let reopened = Quota::open_service(&fixture.quota.root)?;
        let observed = reopened.observe_funding(build.operation_id)?;
        assert_eq!(observed.identity()?, before.identity()?);
        assert_eq!(reopened.status()?.reserved_bytes, 2 * BUDGET);
        if fail_after_replace {
            assert_eq!(observed.takeovers.len(), 1);
            assert!(
                observed
                    .children
                    .iter()
                    .all(|child| child.phase == cleanup.phase)
            );
            assert!(reopened.clone().with_funding(build).is_err());
            reopened.clone().with_funding(cleanup)?;
        } else {
            assert_eq!(observed, before);
            reopened.clone().with_funding(build)?;
            assert!(reopened.clone().with_funding(cleanup).is_err());
        }
    }
    Ok(())
}

#[test]
fn failed_work_retry_preserves_partial_staging_and_current_grants_for_both_participants()
-> Result<()> {
    for purpose in [
        SplitJobPurpose::DestinationBuild,
        SplitJobPurpose::OriginalRemoval,
    ] {
        let fixture = Fixture::new()?;
        let receipt = fixture.quota.reserve_funding(&fixture.request)?;
        let (index, collection) = if purpose == SplitJobPurpose::DestinationBuild {
            (fixture.request.destination_index, "destination")
        } else {
            (fixture.request.original_index, "original")
        };
        let old = fixture.quota.begin_funding_phase(
            &receipt,
            index,
            FundingPhase {
                revision: 2,
                purpose,
                job: Sha256Digest::from_bytes([41; 32]),
            },
        )?;
        let funded = fixture.quota.clone().with_funding(old.clone())?;
        let store =
            crate::FileStore::new(fixture.quota.root.join("collections-1.4").join(collection));
        let writer = store.lock()?;
        let capture = funded.reserve(collection, 4096)?;
        capture.charge(128)?;
        let stage = capture.staging_directory()?;
        fs::write(stage.join("partial"), [9; 128])?;
        let scope = CurrentGrantScope {
            collection: collection.into(),
            incarnation: Incarnation::from_bytes([42; 16]),
            transaction: Sha256Digest::from_bytes([43; 32]),
        };
        let grant = funded.reserve_current(&writer, scope.clone(), 8192)?;
        drop(grant);
        let mut retry = old.clone();
        retry.phase = FundingPhase {
            revision: 9,
            purpose,
            job: Sha256Digest::from_bytes([44; 32]),
        };
        let before = fixture.quota.observe_funding(old.operation_id)?;
        assert!(
            fixture
                .quota
                .transfer_funded_children(
                    &before,
                    &retry,
                    FundingTakeoverKind::FailedWorkRetry,
                    |_| Ok(())
                )
                .is_err()
        );
        assert_eq!(fixture.quota.observe_funding(old.operation_id)?, before);
        drop(capture);
        let before = fixture.quota.observe_funding(old.operation_id)?;
        let mut crossed = retry.clone();
        crossed.phase.purpose = if purpose == SplitJobPurpose::DestinationBuild {
            SplitJobPurpose::OriginalRemoval
        } else {
            SplitJobPurpose::DestinationBuild
        };
        assert!(
            fixture
                .quota
                .transfer_funded_children(
                    &before,
                    &crossed,
                    FundingTakeoverKind::FailedWorkRetry,
                    |_| Ok(())
                )
                .is_err()
        );
        let after = fixture.quota.transfer_funded_children(
            &before,
            &retry,
            FundingTakeoverKind::FailedWorkRetry,
            |_| Ok(()),
        )?;
        assert_eq!(before.identity()?, after.identity()?);
        assert_eq!(
            before
                .children
                .iter()
                .map(|child| (child.id, child.capacity, child.charged))
                .collect::<Vec<_>>(),
            after
                .children
                .iter()
                .map(|child| (child.id, child.capacity, child.charged))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            after.takeovers.last().unwrap().kind,
            FundingTakeoverKind::FailedWorkRetry
        );
        assert_eq!(fs::read(stage.join("partial"))?, [9; 128]);
        assert!(fixture.quota.clone().with_funding(old).is_err());
        let recovered = fixture.quota.clone().with_funding(retry.clone())?;
        let child = after
            .children
            .iter()
            .find(|child| matches!(child.kind, FundingChildKind::Reservation { .. }))
            .unwrap();
        let adopted = recovered.adopt_funded_reservation(&retry, child.id)?;
        assert_eq!(adopted.charged(), 128);
        drop(adopted);
        recovered
            .current_grant(&writer, &scope)?
            .unwrap()
            .release(&writer)?;
        let settled = recovered.reconcile_funding_phase(&writer, &retry, None)?;
        assert!(settled.children.is_empty() && !stage.exists());
        let consumed = settled
            .participants
            .iter()
            .map(|participant| participant.consumed_growth)
            .sum::<u64>();
        assert_eq!(
            fixture.quota.status()?.reserved_bytes + consumed,
            2 * BUDGET
        );
    }
    Ok(())
}

#[test]
fn funded_growth_headroom_is_usable_when_global_unreserved_headroom_is_zero() -> Result<()> {
    let fixture = Fixture::new()?;
    let (_owner, funded) = fixture.destination()?;
    let status = fixture.quota.status()?;
    fixture.quota.configure(QuotaPolicy {
        root_bytes: status.used_bytes + status.reserved_bytes,
        default_collection_bytes: BUDGET * 4,
        collections: BTreeMap::new(),
    })?;
    let status = fixture.quota.status()?;
    assert_eq!(
        status.root_limit_bytes - status.used_bytes - status.reserved_bytes,
        0
    );
    assert_eq!(
        funded.operation_growth_headroom("destination")?,
        Some(BUDGET)
    );
    assert!(funded.operation_growth_headroom("original").is_err());
    assert_eq!(
        fixture.quota.operation_growth_headroom("destination")?,
        None
    );
    let reservation = funded.reserve_available("destination", BUDGET / 2)?;
    assert_eq!(reservation.limit(), BUDGET / 2);
    assert_eq!(
        funded.operation_growth_headroom("destination")?,
        Some(BUDGET / 2)
    );
    drop(reservation);
    assert_eq!(
        funded.operation_growth_headroom("destination")?,
        Some(BUDGET / 2)
    );
    Ok(())
}

#[test]
fn progressive_checkpoint_preserves_live_credit_and_monotonic_charges_near_full_root() -> Result<()>
{
    let fixture = Fixture::new()?;
    let (owner, funded) = fixture.destination()?;
    let store = fixture.destination_store();
    let writer = store.lock()?;
    let status = fixture.quota.status()?;
    fixture.quota.configure(QuotaPolicy {
        root_bytes: status.used_bytes + status.reserved_bytes + 128,
        default_collection_bytes: BUDGET * 4,
        collections: BTreeMap::new(),
    })?;
    let capture = funded.reserve("destination", BUDGET / 4)?;
    let failure = funded.reserve("destination", 4096)?;
    capture.charge(64)?;
    let stage = capture.staging_directory()?;
    fs::write(stage.join("live"), [7; 64])?;
    let publication = funded.reserve("destination", 8192)?;
    publication.charge(32)?;
    let published_stage = publication.staging_directory()?;
    fs::write(published_stage.join("closed"), [8; 32])?;
    drop(publication);
    assert!(
        funded
            .reconcile_funding_phase_preserving(&writer, &owner, None, &[&capture])
            .is_err()
    );
    assert!(published_stage.exists());
    let before = funded.observe_funding(owner.operation_id)?;
    let settled = funded.reconcile_funding_phase_preserving_observed(
        &writer,
        &owner,
        None,
        &[&capture, &failure],
        || {
            capture.charge(32).unwrap();
            fs::write(stage.join("live"), [7; 96]).unwrap();
        },
    )?;
    assert_eq!(settled.identity()?, before.identity()?);
    assert_eq!(settled.children.len(), 2);
    assert!(settled.children.iter().all(|child| !child.closed));
    assert_eq!(settled.participants[1].settled_revision, None);
    assert_eq!(capture.charged(), 96);
    assert_eq!(capture.limit(), BUDGET / 4);
    assert_eq!(fs::read(stage.join("live"))?, [7; 96]);
    assert!(!published_stage.exists());
    assert_eq!(
        settled
            .children
            .iter()
            .map(|child| child.held_bytes)
            .sum::<u64>(),
        BUDGET / 4 + 4096
    );
    let consumed = settled.participants[1].consumed_growth;
    assert_eq!(
        consumed,
        fs::metadata(store.directory().join("writer.lock"))?.len() + 96
    );
    assert_eq!(
        fixture.quota.status()?.reserved_bytes + consumed,
        2 * BUDGET
    );
    assert!(fixture.quota.reserve("competing", 1024).is_err());
    let prepaid = funded.reserve_available("destination", 4096)?;
    drop(prepaid);
    assert!(
        funded
            .begin_funding_phase(
                &funded.observe_funding(owner.operation_id)?,
                owner.index_id,
                FundingPhase {
                    revision: 12,
                    purpose: SplitJobPurpose::DestinationCleanup,
                    job: Sha256Digest::from_bytes([61; 32])
                }
            )
            .is_err()
    );
    drop(capture);
    drop(failure);
    let final_receipt = funded.reconcile_funding_phase(&writer, &owner, None)?;
    assert!(final_receipt.children.is_empty());
    assert_eq!(
        final_receipt.participants[1].settled_revision,
        Some(owner.phase.revision)
    );
    assert!(!stage.exists());
    Ok(())
}
