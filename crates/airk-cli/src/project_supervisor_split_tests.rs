// SPDX-License-Identifier: Apache-2.0
//! Real native empty-census ownership plus private Kernel jobs. This does not
//! qualify live process/provider drains or managed installation activation.
use super::*;
use airk_core::{
    catalog_management::{
        CatalogEntity, CatalogItem, CatalogKind, CatalogPageRequest, MAX_PAGE_BYTES,
    },
    catalog_split::{SplitFinalizationProgress, SplitJobPurpose, SplitRunState, SplitStage},
    catalog_split_draft::SplitMembershipSelection,
    service::{ControlMethod, JobOutcome, JobState},
};
use airk_kernel::Kernel;

fn page(registry: &mut ServiceRegistry, kind: CatalogKind) -> Result<Vec<CatalogItem>> {
    let page = registry.catalog_page(&CatalogPageRequest {
        kind,
        index_id: None,
        project_id: None,
        after: None,
        expected_catalog_revision: None,
        limit: 32,
        max_bytes: MAX_PAGE_BYTES,
    })?;
    ensure!(
        page.continuation.is_none(),
        "fixture exceeds complete single-page census"
    );
    Ok(page.items)
}

#[test]
fn real_empty_native_owner_completes_witness_fence_remainder_membership_and_funding_ack()
-> Result<()> {
    let mut fixture = crate::catalog_split_control::tests::native_fixture(false)?;
    ensure!(
        !fixture.selection.projects.is_empty(),
        "native import needs affected Project census"
    );
    for project in &mut fixture.selection.projects {
        project.membership = SplitMembershipSelection::PreserveReviewedScope;
    }
    let grants_before = page(&mut fixture.registry, CatalogKind::Grants)?;
    let contexts_before = page(&mut fixture.registry, CatalogKind::Contexts)?;
    let projects_before = page(&mut fixture.registry, CatalogKind::Projects)?;
    let source = Path::new(&fixture.original.root);
    let selected_bytes = std::fs::read(source.join("docs/selected.md"))?;
    let remainder_bytes = std::fs::read(source.join("remainder.md"))?;
    let original_before = Kernel::open_existing_service_observer(&fixture.original)?
        .status()?
        .publication
        .context("original publication absent")?
        .token;

    // Reuse the production native owner and its exact control/drain implementation.
    // There are genuinely no child workers, helper deliveries, or scoped clients;
    // private job leases below are dropped before native census and cutover.
    let instance = new_instance_id()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(
        &fixture.control,
    )?);
    crate::runtime_install::record_control_root(&lease)?;
    fixture.registry.record_service_instance(&lease, instance)?;
    fixture.registry = ServiceRegistry::open(&fixture.control, instance)?;
    let state = fixture
        .registry
        .catalog_state()?
        .context("catalog absent")?;
    let gate = NativeCatalogAdmission::new(instance, state.namespace, state.revision, 1)?;
    let (log, log_guard) = crate::service_logs::Log::start(fixture.control.clone(), instance);
    let sources = crate::source_supervisor::SourceSupervisor::start(
        fixture.control.clone(),
        instance,
        log.clone(),
    )?;
    let source_wake = sources.wake_handle();
    let owner = Rc::new(RefCell::new(NativeOwner {
        root: fixture.control.clone(),
        instance,
        gate: gate.clone(),
        bound: None,
        workers: BTreeMap::new(),
        registry: Some(ServiceRegistry::open(&fixture.control, instance)?),
        pool: None,
        recovery: PoolRecovery::new(Instant::now()),
        resources: airk_kernel::service_resources::Resources::detect(
            airk_core::service_resources::ResourceConfiguration {
                memory_bytes: Some(4 << 30),
                query_concurrency: 4,
                ..Default::default()
            },
        )?,
        drains: WorkerResourceDrains::default(),
        policy: Mutex::new(Broker::new(
            instance,
            airk_kernel::service_broker::BrokerLimits::service(),
        )?),
        log,
        _log_guard: log_guard,
        backoff: BTreeMap::new(),
        _lease: lease,
    }));
    ensure!(
        matches!(
            owner.borrow_mut().execute(catalog::Operation::Attach)?,
            catalog::Response::Attached
        ),
        "native owner failed attachment"
    );
    let control = NativeControl {
        owner,
        gate: gate.clone(),
        source: Some(source_wake.clone()),
    };
    let mut coordinator =
        crate::catalog_coordinator::CatalogCoordinator::new(gate.clone(), control.clone())?;

    let Some(ControlResponse::CatalogSplitDraft { draft }) = crate::catalog_split_control::execute(
        &mut fixture.registry,
        &fixture.control,
        &ControlMethod::CatalogSplitDraft {
            selection: Box::new(fixture.selection.clone()),
        },
    )?
    else {
        anyhow::bail!("canonical split draft missing");
    };
    ensure!(
        draft
            .request
            .projects
            .iter()
            .all(|effect| effect.destination_scope.is_some()),
        "review omitted requested Project destination"
    );
    let Some(ControlResponse::CatalogSplitAccepted { snapshot: accepted }) =
        crate::catalog_split_control::execute(
            &mut fixture.registry,
            &fixture.control,
            &ControlMethod::CatalogSplitApply { draft },
        )?
    else {
        anyhow::bail!("canonical split apply missing");
    };
    let operation = accepted.receipt.operation_id;
    let mut witnessed = false;
    let mut fenced = false;
    let mut removed = false;
    let mut complete = None;
    let mut resumed_metadata = 0;
    let mut refreshed_witness = false;
    for _ in 0..80 {
        let current = fixture
            .registry
            .catalog_split_status(operation)?
            .context("split disappeared")?;
        ensure!(
            !matches!(current.receipt.state, SplitRunState::NeedsAttention { .. }),
            "split needs attention: {:?}",
            current.receipt
        );
        if current.finalization.as_ref().is_some_and(|finalization| {
            finalization.progress == SplitFinalizationProgress::Complete
        }) {
            complete = Some(current);
            break;
        }
        if current.receipt.stage == SplitStage::RemainderPublished {
            let policy = fixture
                .registry
                .catalog_policy_status(current.request.original_index)?
                .context("remainder policy absent")?;
            ensure!(
                policy
                    .application
                    .as_ref()
                    .is_some_and(|report| report.removal_publication
                        == current.receipt.remainder_publication),
                "split advanced before canonical P9 completion acknowledgment"
            );
        }
        if let Some(child) = current.receipt.jobs.last()
            && fixture.registry.job(child.job)?.state == JobState::Pending
        {
            let registration = if matches!(
                child.purpose,
                SplitJobPurpose::DestinationBuild | SplitJobPurpose::DestinationCleanup
            ) {
                &current.request.destination_registration
            } else {
                &current.request.original_registration
            };
            let mut project_lease = ProjectLease::acquire(&fixture.control, registration.binding)?;
            fixture.registry.admit_worker(&mut project_lease)?;
            fixture
                .registry
                .claim_job_id(&project_lease, child.job)?
                .context("native exact child not claimable")?;
            let kernel = Kernel::open_existing_service_observer(registration)?;
            let work = fixture
                .registry
                .configuration_work(child.job)?
                .context("accepted native configuration absent")?;
            let outcome = crate::project_worker_split::execute(
                &kernel,
                &mut fixture.registry,
                &project_lease,
                child.job,
                work.engine,
                &fixture.control,
            )?;
            ensure!(
                matches!(outcome, JobOutcome::Succeeded { .. }),
                "actual native child failed"
            );
            fixture
                .registry
                .complete_job(&project_lease, child.job, outcome)?;
            if child.purpose == SplitJobPurpose::OriginalRemoval {
                // Deliberately omit production's post-Succeeded P9 tail. This
                // is the real crash/interleaving boundary the metadata owner
                // must reconcile from the exact retained native job result.
                ensure!(
                    fixture
                        .registry
                        .catalog_policy_status(current.request.original_index)?
                        .context("original policy absent after actual removal")?
                        .application
                        .is_none(),
                    "fixture unexpectedly ran the P9 completion tail"
                );
            }
            witnessed |= child.purpose == SplitJobPurpose::OriginalWitness;
            removed |= child.purpose == SplitJobPurpose::OriginalRemoval;
            // These actual ownership guards end before the next metadata turn.
            drop(kernel);
            drop(project_lease);
            if child.purpose == SplitJobPurpose::OriginalRemoval
                || child.purpose == SplitJobPurpose::OriginalWitness && !refreshed_witness
            {
                let observed = fixture
                    .registry
                    .catalog_split_status(operation)?
                    .context("split absent")?;
                let attention = fixture.registry.catalog_note_split_failure(
                    operation,
                    observed.receipt_digest,
                    "native_fixture_metadata_ack_interrupted",
                )?;
                let request = airk_core::catalog_split::SplitRecoveryRequest {
                    caller: attention.request.caller,
                    key: airk_core::service::RequestKey {
                        nonce: Incarnation::from_bytes(
                            [if child.purpose == SplitJobPurpose::OriginalWitness {
                                121
                            } else {
                                122
                            }; 16],
                        ),
                        ..attention.request.key.clone()
                    },
                    operation_id: operation,
                    request_digest: attention.receipt.request_digest,
                    expected_receipt_digest: attention.receipt_digest,
                    expected_finalization_revision: None,
                    action: airk_core::catalog_split::SplitRecoveryAction::Resume,
                };
                let sql = rusqlite::Connection::open(fixture.control.join("service-1.4.sqlite3"))?;
                let count: i64 =
                    sql.query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
                // Omit retained proof temporarily to exercise unknown/pruned
                // result refusal. Restore only the exact genuine native bytes.
                let bytes: Vec<u8> = sql.query_row(
                    "SELECT result FROM service_configuration WHERE job=?1",
                    [child.job.to_string()],
                    |row| row.get(0),
                )?;
                sql.execute(
                    "UPDATE service_configuration SET result=zeroblob(16392) WHERE job=?1",
                    [child.job.to_string()],
                )?;
                let method = ControlMethod::CatalogSplitRecover {
                    request: Box::new(request.clone()),
                };
                ensure!(
                    crate::catalog_split_control::execute(
                        &mut fixture.registry,
                        &fixture.control,
                        &method
                    )
                    .is_err(),
                    "missing successful result became retry authority"
                );
                ensure!(
                    fixture
                        .registry
                        .catalog_split_recovery_receipt(request.caller, &request.key)?
                        .is_none()
                        && fixture.registry.catalog_split_status(operation)?
                            == Some(attention.clone()),
                    "unknown result mutated acknowledgment or current split"
                );
                sql.execute(
                    "UPDATE service_configuration SET result=?1 WHERE job=?2",
                    rusqlite::params![bytes, child.job.to_string()],
                )?;
                let Some(ControlResponse::CatalogSplitRecovered { receipt, snapshot }) =
                    crate::catalog_split_control::execute(
                        &mut fixture.registry,
                        &fixture.control,
                        &method,
                    )?
                else {
                    anyhow::bail!("metadata Resume response absent");
                };
                ensure!(
                    matches!(
                        receipt.outcome,
                        airk_core::catalog_split::SplitRecoveryOutcome::ContinuationAccepted { .. }
                    ) && snapshot.receipt.jobs == attention.receipt.jobs
                        && snapshot.receipt.stage == attention.receipt.stage
                        && snapshot.receipt.state == SplitRunState::Pending,
                    "successful child Resume reran work instead of exact metadata continuation"
                );
                // The canonical owning methods must replay this historical ack
                // before observing the now-newer current receipt or config.
                let replay = if child.purpose == SplitJobPurpose::OriginalRemoval {
                    fixture
                        .registry
                        .catalog_resume_split_policy_action(&request)?
                } else {
                    fixture.registry.catalog_resume_split_job_action(
                        &request,
                        work.engine,
                        &work.request,
                    )?
                };
                ensure!(
                    replay == *receipt,
                    "exact native Resume key changed its accepted acknowledgment"
                );
                ensure!(
                    sql.query_row("SELECT count(*) FROM service_jobs", [], |row| row
                        .get::<_, i64>(0))?
                        == count,
                    "metadata-only Resume grew the native work queue"
                );
                resumed_metadata += 1;
            }
        } else {
            match crate::catalog_split_control::advance_pending(
                &mut fixture.registry,
                &fixture.control,
                fixture.engine,
                operation,
            )? {
                crate::catalog_split_control::SplitAdvance::NeedOriginalFence(snapshot) => {
                    ensure!(
                        witnessed && snapshot.receipt.remainder_witness.is_some(),
                        "native witness not retained"
                    );
                    if !refreshed_witness {
                        // The successful proof is now retained. Preserve the
                        // distinct explicit fresh-witness branch used after a
                        // stale cutover observation; this test requests fresh
                        // requalification without claiming the source changed.
                        let attention = fixture.registry.catalog_note_split_failure(
                            operation,
                            snapshot.receipt_digest,
                            "native_fixture_retained_witness_refresh",
                        )?;
                        let request = airk_core::catalog_split::SplitRecoveryRequest {
                            caller: attention.request.caller,
                            key: airk_core::service::RequestKey {
                                nonce: Incarnation::from_bytes([123; 16]),
                                ..attention.request.key.clone()
                            },
                            operation_id: operation,
                            request_digest: attention.receipt.request_digest,
                            expected_receipt_digest: attention.receipt_digest,
                            expected_finalization_revision: None,
                            action: airk_core::catalog_split::SplitRecoveryAction::Resume,
                        };
                        let Some(ControlResponse::CatalogSplitRecovered {
                            receipt,
                            snapshot: next,
                        }) = crate::catalog_split_control::execute(
                            &mut fixture.registry,
                            &fixture.control,
                            &ControlMethod::CatalogSplitRecover {
                                request: Box::new(request),
                            },
                        )?
                        else {
                            anyhow::bail!("stale witness recovery absent");
                        };
                        ensure!(
                            matches!(
                                receipt.outcome,
                                airk_core::catalog_split::SplitRecoveryOutcome::ResumeAccepted { .. }
                            ) && next.receipt.jobs.len() == attention.receipt.jobs.len() + 1
                                && next.receipt.jobs.last().is_some_and(
                                    |job| job.purpose == SplitJobPurpose::OriginalWitness
                                ),
                            "explicit stale retained witness Resume did not admit one fresh witness job"
                        );
                        refreshed_witness = true;
                        continue;
                    }
                    crate::catalog_coordinator::refresh_catalog_clock(&gate, &fixture.registry)?;
                    let (request, preview) =
                        crate::catalog_source_change_control::split_cutover_for_native_fixture(
                            &mut fixture.registry,
                            &fixture.control,
                            &snapshot,
                            fixture.engine,
                        )?;
                    let receipt =
                        coordinator.apply_fenced(&mut fixture.registry, &request, preview)?;
                    let obligation = receipt
                        .proof
                        .worker_obligation
                        .as_ref()
                        .context("real native worker census missing")?;
                    ensure!(
                        obligation.workers.is_empty(),
                        "private fixture unexpectedly owns child workers"
                    );
                    ensure!(
                        control.owner.borrow().workers.is_empty(),
                        "native owner census changed"
                    );
                    ensure!(
                        fixture
                            .registry
                            .catalog_split_status(operation)?
                            .context("fenced split absent")?
                            .receipt
                            .stage
                            == SplitStage::OriginalFenced,
                        "native fence did not retain original cutover"
                    );
                    fenced = true;
                }
                crate::catalog_split_control::SplitAdvance::Advanced(_)
                | crate::catalog_split_control::SplitAdvance::Waiting(_) => {}
            }
        }
    }
    let complete = complete.context("bounded native split did not complete")?;
    ensure!(
        resumed_metadata == 2 && refreshed_witness,
        "native happy path omitted metadata recovery or stale witness refresh"
    );
    ensure!(
        witnessed && fenced && removed && complete.receipt.stage == SplitStage::Complete,
        "happy path omitted actual native work"
    );
    ensure!(
        complete.receipt.funding_release_receipts.len() == 2,
        "two native root releases absent"
    );
    ensure!(
        complete
            .finalization
            .as_ref()
            .context("finalization absent")?
            .funding_acknowledged
            .len()
            == 2,
        "two native funding acknowledgments absent"
    );
    ensure!(
        !fixture
            .registry
            .catalog_split_write_fenced(fixture.original.binding)?
            && !fixture
                .registry
                .catalog_split_write_fenced(complete.request.destination_registration.binding)?,
        "fully acknowledged participants remain write-fenced"
    );
    let original = Kernel::open_existing_service_observer(&fixture.original)?;
    let destination =
        Kernel::open_existing_service_observer(&complete.request.destination_registration)?;
    original.require_unrestricted_read()?;
    destination.require_unrestricted_read()?;
    let remainder_files = original.list_files("", 32, MAX_PAGE_BYTES)?;
    let destination_files = destination.list_files("", 32, MAX_PAGE_BYTES)?;
    ensure!(
        !remainder_files.truncated && !destination_files.truncated,
        "native file census truncated"
    );
    ensure!(
        remainder_files.paths == vec!["remainder.md".to_string()]
            && destination_files.paths == vec!["docs/selected.md".to_string()],
        "actual retained evidence does not match disjoint accepted partition"
    );
    ensure!(
        original
            .status()?
            .publication
            .context("remainder publication absent")?
            .token
            != original_before,
        "original publication never changed"
    );
    for effect in &complete.request.projects {
        let detail = fixture.registry.catalog_detail(
            &CatalogEntity::Membership {
                project_id: effect.project_id,
                index_id: complete.request.destination.index_id,
            },
            None,
        )?;
        ensure!(
            matches!(detail.item, Some(CatalogItem::Membership(ref membership)) if Some(&membership.scope) == effect.destination_scope.as_ref()),
            "actual destination membership differs from accepted review"
        );
        let original_membership = fixture.registry.catalog_detail(
            &CatalogEntity::Membership {
                project_id: effect.project_id,
                index_id: complete.request.original_index,
            },
            None,
        )?;
        ensure!(
            original_membership.item.is_some() == effect.retain_original,
            "original membership differs from accepted choice"
        );
    }
    let projects_after = page(&mut fixture.registry, CatalogKind::Projects)?;
    for before in projects_before {
        let CatalogItem::Project(before) = before else {
            anyhow::bail!("unexpected Project page item");
        };
        let after = projects_after
            .iter()
            .find_map(|item| match item {
                CatalogItem::Project(project) if project.id == before.id => Some(project),
                _ => None,
            })
            .context("affected Project disappeared")?;
        ensure!(
            after.default_indexes == before.default_indexes
                && after.defaults_revision == before.defaults_revision,
            "split invented default selection"
        );
    }
    ensure!(
        page(&mut fixture.registry, CatalogKind::Grants)? == grants_before,
        "split invented or rewrote a grant"
    );
    ensure!(
        page(&mut fixture.registry, CatalogKind::Contexts)? == contexts_before,
        "split invented context selection"
    );
    ensure!(
        std::fs::read(source.join("docs/selected.md"))? == selected_bytes
            && std::fs::read(source.join("remainder.md"))? == remainder_bytes,
        "native split modified source files"
    );
    drop(coordinator);
    drop(control);
    drop(sources);
    Ok(())
}
