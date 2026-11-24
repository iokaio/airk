// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{index_catalog::*, scoped_reads::SourceDescriptor, service::ProjectBinding};

fn trim_request(namespace: Incarnation, authority: &Base) -> Result<CatalogRequest> {
    use airk_core::{
        service_configuration::{ConfigurationRequest, ProjectConfiguration},
        source_policy::{
            DenyReason, DenyRule, POLICY_SCHEMA, PolicyChange, SourcePolicy, SourceRoot,
        },
    };
    let mut request = request(namespace, authority);
    let root = &authority.sources[0];
    let rule_id = DenyId::from_identity(Incarnation::from_bytes([52; 16]));
    let selector = ScopeAtom {
        source_root_id: root.source_root_id,
        kind: PathScope::Subtree,
        path: "private".into(),
    };
    let policy = SourcePolicy {
        schema: POLICY_SCHEMA.into(),
        index_id: authority.index_id,
        owner_binding: authority.backing.to_string().parse()?,
        namespace: "collections-1.4".into(),
        collection: "fixture".into(),
        epoch: CatalogRevision::INITIAL,
        roots: vec![SourceRoot {
            id: root.source_root_id,
            canonical_path: root.original_root.clone(),
            aliases: vec![],
            owner: true,
        }],
        positive_includes: None,
        denies: vec![DenyRule {
            source_root_id: selector.source_root_id,
            kind: selector.kind,
            path: selector.path.clone(),
            rule_id,
            reason: DenyReason::Trim,
            created_epoch: CatalogRevision::INITIAL,
            inherited_from: None,
        }],
    };
    let operation_id =
        airk_core::catalog_management::catalog_operation_id(request.caller, &request.key)?;
    request.operation = CatalogOperation::ChangeIndexPolicy {
        id: authority.index_id,
        expected_source_revision: CatalogRevision::INITIAL,
        expected_policy_epoch: 0,
        expected_policy_digest: None,
        change: PolicyChange::Trim { rule_id, selector },
        configuration: Box::new(ConfigurationRequest {
            prepare: false,
            incarnation: Incarnation::from_bytes([53; 16]),
            settings: Sha256Digest::from_bytes([54; 32]),
            configuration: ProjectConfiguration::ApplySourcePolicy {
                policy: Box::new(policy),
                operation_id,
            },
        }),
        engine_identity: Sha256Digest::from_bytes([55; 32]),
    };
    request.validate()?;
    Ok(request)
}

#[test]
fn permanent_trim_requires_unscoped_worker_census_and_provider_drain_before_commit() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(directory.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let request = trim_request(namespace, &authority)?;
    let plan = gate.close_retirement_admission(&request, worker.project)?;
    assert!(
        plan.targets.is_empty(),
        "unscoped worker has no Project scope entries"
    );
    assert!(gate.require_worker_start(worker.project).is_err());
    assert!(
        gate.with_administrative_worker(namespace, worker, || ())
            .is_err()
    );
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    let plan = gate.capture_retirement_workers(
        &plan.ticket,
        worker.project,
        vec![worker],
        vec![native_provider_obligation(worker, vec![])],
    )?;
    assert_eq!(plan.drain_workers, vec![worker]);
    gate.begin_worker_drain(&plan.ticket, worker)?;
    gate.observe_worker_reaped(worker)?;
    assert!(
        gate.require_commit_ready(&plan.ticket).is_err(),
        "process exit alone cannot settle helper/resource work"
    );
    gate.observe_worker_resources_drained(worker)?;
    gate.require_commit_ready(&plan.ticket)?;
    Ok(())
}

#[test]
fn permanent_trim_closes_pre_persistence_combined_and_administrative_delivery() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(directory.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let mut delivered = 0;
    gate.with_issue(issue, || delivered += 1)?;
    gate.with_administrative_worker(namespace, worker, || delivered += 1)?;
    let request = trim_request(namespace, &authority)?;
    let _closed = gate.close_retirement_admission(&request, worker.project)?;
    assert!(gate.with_issue(issue, || delivered += 1).is_err());
    assert!(
        gate.with_administrative_worker(namespace, worker, || delivered += 1)
            .is_err()
    );
    assert_eq!(
        delivered, 2,
        "neither retained delivery can cross the native policy close"
    );
    Ok(())
}

#[test]
fn combined_delivery_poll_refuses_the_pre_persistence_pending_fence() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (gate, _, authority, namespace) = fixture(directory.path().to_str().unwrap());
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let mut delivered = 0;
    gate.with_issue(issue, || delivered += 1)?;
    assert_eq!(delivered, 1);
    // This native close precedes any durable registry mutation. A persisted
    // metadata snapshot alone would still see the original catalog revision.
    let _pending = gate.close_request_admission(&request(namespace, &authority))?;
    assert!(gate.with_issue(issue, || delivered += 1).is_err());
    assert_eq!(delivered, 1, "revoked delivery effect must never be polled");
    assert!(
        gate.begin_issue(namespace, CatalogRevision::INITIAL)
            .is_err()
    );
    Ok(())
}

/// Change-log entry 67: on the begin side, a fence in flight, a moved
/// revision and a clock still behind are typed and retryable; another
/// namespace refuses, and delivery of an issued ticket still refuses.
#[test]
fn begin_side_transients_are_typed_and_delivery_still_refuses() -> Result<()> {
    use crate::catalog_coordinator::{CatalogClockPending, CatalogRevisionChanged};
    let directory = tempfile::tempdir()?;
    let (gate, _, authority, namespace) = fixture(directory.path().to_str().unwrap());
    let foreign = Incarnation::from_bytes([12; 16]);
    let next = CatalogRevision::INITIAL.next()?;
    let error = gate.begin_issue(namespace, next).unwrap_err();
    assert!(error.is::<CatalogRevisionChanged>(), "{error:#}");
    let error = gate.capture_current_issue(namespace, next).unwrap_err();
    assert!(error.is::<CatalogClockPending>(), "{error:#}");
    for error in [
        gate.begin_issue(foreign, CatalogRevision::INITIAL)
            .unwrap_err(),
        gate.capture_current_issue(foreign, CatalogRevision::INITIAL)
            .unwrap_err(),
    ] {
        assert!(error.is::<ScopeReadRefused>(), "{error:#}");
    }
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let _pending = gate.close_request_admission(&request(namespace, &authority))?;
    for error in [
        gate.begin_issue(namespace, CatalogRevision::INITIAL)
            .unwrap_err(),
        gate.capture_current_issue(namespace, CatalogRevision::INITIAL)
            .unwrap_err(),
    ] {
        assert!(error.is::<CatalogAdmissionPending>(), "{error:#}");
        assert!(!error.is::<ScopeReadRefused>());
    }
    let mut delivered = 0;
    let error = gate.with_issue(issue, || delivered += 1).unwrap_err();
    assert!(error.is::<ScopeReadRefused>(), "{error:#}");
    assert!(
        gate.check_issue(issue)
            .unwrap_err()
            .is::<ScopeReadRefused>()
    );
    assert_eq!(delivered, 0);
    Ok(())
}

/// Change-log entry 67: a worker start held back by a retirement census in
/// flight is the retryable `CatalogAdmissionPending`, not a refusal.
#[test]
fn worker_start_during_a_census_fence_is_admission_pending() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(directory.path().to_str().unwrap());
    gate.register_worker(worker)?;
    gate.require_worker_start(worker.project)?;
    let request = trim_request(namespace, &authority)?;
    let _plan = gate.close_retirement_admission(&request, worker.project)?;
    let error = gate.require_worker_start(worker.project).unwrap_err();
    assert!(error.is::<CatalogAdmissionPending>(), "{error:#}");
    assert!(!error.is::<ScopeReadRefused>());
    Ok(())
}

#[tokio::test]
async fn native_delivery_stops_an_actual_partial_native_write_after_admission_close() -> Result<()>
{
    use tokio::io::AsyncReadExt;
    let directory = tempfile::tempdir()?;
    let (gate, _, authority, namespace) = fixture(directory.path().to_str().unwrap());
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let response = airk_core::service::ControlResponse::NativeRead {
        result: serde_json::json!({"retained": "a native administrative page"}),
    };
    let (mut reader, mut writer) = tokio::io::duplex(1);
    let owner = gate.clone();
    let writing = tokio::spawn(async move {
        owner
            .deliver_issue(issue, &mut writer, &response, Duration::from_secs(5))
            .await
    });
    let mut prefix = [0; 1];
    reader.read_exact(&mut prefix).await?;
    let _pending = gate.close_request_admission(&request(namespace, &authority))?;
    // Wake a writer already waiting for buffer capacity; its next actual poll
    // must observe the close even though the registry clock is unchanged.
    let mut buffered = [0; 1];
    let buffered_bytes = reader.read(&mut buffered).await?;
    let error = writing.await?.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("combined_delivery_scope_expired"),
        "{error:#}"
    );
    let mut remainder = Vec::new();
    reader.read_to_end(&mut remainder).await?;
    assert!(
        buffered_bytes + remainder.len() < 4,
        "a partial native frame cannot become a complete receipt"
    );
    Ok(())
}

#[test]
fn retirement_captures_unscoped_worker_and_reopens_only_after_exact_reap_proof() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let mut request = request(namespace, &authority);
    request.operation = CatalogOperation::RetireIndex {
        id: authority.index_id,
        expected_definition_revision: CatalogRevision::INITIAL,
        expected_source_revision: CatalogRevision::INITIAL,
        reason: airk_core::catalog_source_change::IndexRetirementReason::Explicit,
    };
    let closed = gate.close_request_admission(&request)?;
    assert!(closed.targets.is_empty(), "fixture has no evidence scopes");
    assert!(gate.require_commit_ready(&closed.ticket).is_err());
    assert!(gate.require_worker_start(worker.project).is_err());
    gate.bind_retirement_backing(&closed.ticket, authority.index_id, worker.project)?;
    let captured = gate.capture_retirement_workers(
        &closed.ticket,
        worker.project,
        vec![worker],
        vec![native_provider_obligation(worker, Vec::new())],
    )?;
    assert_eq!(captured.drain_workers, vec![worker]);
    assert_eq!(
        captured.durable.worker_obligation.as_ref().unwrap().workers,
        vec![worker]
    );
    assert!(
        gate.require_worker_start(ProjectBinding::from_bytes([33; 32]))
            .is_ok()
    );
    // Native owner exclusion / actual child reap is supplied at this ledger
    // boundary; this unit test does not claim an OS process was exercised.
    let mut wrong = worker;
    wrong.worker = Incarnation::from_bytes([45; 16]);
    assert!(gate.observe_worker_reaped(wrong).is_err());
    assert!(gate.require_commit_ready(&captured.ticket).is_err());
    gate.begin_worker_drain(&captured.ticket, worker)?;
    gate.observe_worker_reaped(worker)?;
    assert!(
        gate.require_commit_ready(&captured.ticket).is_err(),
        "child exit cannot acknowledge accepted provider work"
    );
    gate.observe_worker_resources_drained(worker)?;
    gate.require_commit_ready(&captured.ticket)?;
    assert_eq!(
        gate.worker_confirmations(&captured.ticket)?,
        vec![WorkerFenceConfirmation {
            worker,
            method: WorkerFenceConfirmationMethod::NativeWorkerReap
        }]
    );
    let (_, receipt) = durable_fixture(&request, &captured.durable)?;
    gate.finish_commit(
        &captured.ticket,
        &request,
        receipt.preview_digest,
        receipt.prepared_digest,
        &receipt,
    )?;
    gate.begin_issue(namespace, receipt.metadata.catalog_revision)?;
    Ok(())
}

#[test]
fn retirement_restart_keeps_exact_census_and_cannot_replace_old_worker_with_empty_capture()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, authority, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let mut request = request(namespace, &authority);
    request.operation = CatalogOperation::RetireIndex {
        id: authority.index_id,
        expected_definition_revision: CatalogRevision::INITIAL,
        expected_source_revision: CatalogRevision::INITIAL,
        reason: airk_core::catalog_source_change::IndexRetirementReason::Explicit,
    };
    let closed = gate.close_request_admission(&request)?;
    gate.bind_retirement_backing(&closed.ticket, authority.index_id, worker.project)?;
    let captured = gate.capture_retirement_workers(
        &closed.ticket,
        worker.project,
        vec![worker],
        vec![native_provider_obligation(worker, Vec::new())],
    )?;
    let repeated =
        gate.capture_retirement_workers(&closed.ticket, worker.project, vec![], vec![])?;
    assert_eq!(
        repeated.durable, captured.durable,
        "uncertain census retry must reuse original obligation"
    );
    let (intent, _) = durable_fixture(&request, &captured.durable)?;
    let replacement = NativeCatalogAdmission::new(
        Incarnation::from_bytes([50; 16]),
        namespace,
        CatalogRevision::INITIAL,
        1,
    )?;
    let restored = replacement.restore_pending_fence(&intent)?;
    assert_eq!(restored.drain_workers, vec![worker]);
    assert_eq!(restored.durable, captured.durable);
    assert!(replacement.require_worker_start(worker.project).is_err());
    assert!(replacement.require_commit_ready(&restored.ticket).is_err());
    replacement.observe_worker_reaped(worker)?;
    assert!(replacement.require_commit_ready(&restored.ticket).is_err());
    replacement.observe_worker_resources_drained(worker)?;
    replacement.require_commit_ready(&restored.ticket)?;
    Ok(())
}

fn durable_fixture(
    request: &CatalogRequest,
    plan: &StoreFencePlan,
) -> Result<(FenceIntent, FencedReceipt)> {
    use airk_store::service::registry::catalog::operations::{CatalogFenceProof, FenceStage};
    let preview_digest = Sha256Digest::from_bytes([19; 32]);
    let prepared_digest = airk_core::identity::framed_digest(
        b"airk/catalog-fence-prepare/1",
        &[
            &serde_json::to_vec(request)?,
            &serde_json::to_vec(&preview_digest)?,
            &serde_json::to_vec(plan)?,
        ],
    );
    let key = FenceKey::new(request)?;
    let intent = FenceIntent {
        schema: 1,
        operation_id: key.operation_id,
        request: request.clone(),
        preview_digest,
        prepared_digest,
        plan: plan.clone(),
        stage: FenceStage::Prepared,
        confirmations: Vec::new(),
        worker_confirmations: Vec::new(),
    };
    intent.validate()?;
    let receipt = FencedReceipt {
        request: request.clone(),
        metadata: airk_core::catalog_management::CatalogReceipt {
            schema: 1,
            operation_id: key.operation_id,
            caller: request.caller,
            key: request.key.clone(),
            request_digest: request.identity()?,
            catalog_revision: request.expected_catalog_revision.next()?,
            authority_fence_required: true,
        },
        preview_digest,
        prepared_digest,
        proof: CatalogFenceProof {
            plan_digest: plan.identity()?,
            service: plan.service,
            gate_generation: plan.gate_generation,
            confirmations: plan.targets.iter().cloned().map(|target| airk_store::service::registry::catalog::operations::FenceConfirmation { target, method: airk_store::service::registry::catalog::operations::FenceConfirmationMethod::NativeScopeAck }).collect(),
            worker_obligation: plan.worker_obligation.clone(),
            worker_confirmations: plan.worker_obligation.as_ref().map(|o| o.workers.iter().map(|worker| WorkerFenceConfirmation {worker: *worker, method:WorkerFenceConfirmationMethod::NativeWorkerReap}).collect()).unwrap_or_default(),
        },
    };
    receipt.validate_for(request, preview_digest, prepared_digest, plan)?;
    Ok((intent, receipt))
}

/// The index one test reads: its backing, projects, scope and sources.
#[derive(Clone)]
struct Base {
    backing: ProjectBinding,
    index_id: IndexId,
    project_ids: Vec<QueryProjectId>,
    project_id: QueryProjectId,
    atoms: Vec<ScopeAtom>,
    sources: Vec<SourceDescriptor>,
}
fn fixture(root: &str) -> (NativeCatalogAdmission, WorkerIdentity, Base, Incarnation) {
    let namespace = Incarnation::from_bytes([1; 16]);
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([2; 16]),
        worker: Incarnation::from_bytes([3; 16]),
        project: ProjectBinding::from_bytes([4; 32]),
        fencing_generation: 1,
    };
    let index_id = IndexId::from_identity(Incarnation::from_bytes([5; 16]));
    let project_id = QueryProjectId::from_identity(Incarnation::from_bytes([6; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([7; 16]));
    let authority = Base {
        backing: worker.project,
        index_id,
        project_ids: vec![project_id],
        project_id,
        atoms: vec![ScopeAtom {
            source_root_id: source,
            kind: PathScope::Subtree,
            path: String::new(),
        }],
        sources: vec![SourceDescriptor {
            source_root_id: source,
            original_root: root.into(),
        }],
    };
    let gate = NativeCatalogAdmission::new(worker.service, namespace, CatalogRevision::INITIAL, 1)
        .unwrap();
    (gate, worker, authority, namespace)
}
fn expiry() -> Instant {
    Instant::now() + Duration::from_secs(60)
}
fn request(namespace: Incarnation, authority: &Base) -> CatalogRequest {
    CatalogRequest {
        caller: Incarnation::from_bytes([10; 16]),
        key: RequestKey {
            namespace,
            epoch: 1,
            nonce: Incarnation::from_bytes([11; 16]),
        },
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation: CatalogOperation::DeleteProject {
            id: authority.project_id,
            expected_definition_revision: CatalogRevision::INITIAL,
            expected_scope_revision: CatalogRevision::INITIAL,
            expected_defaults_revision: CatalogRevision::INITIAL,
        },
    }
}
// Selection read authority (spec §3.3, §3.4): in-memory W4/A2 grants.

fn selection_ref(service: Incarnation, nonce: u8) -> SelectionScopeRef {
    SelectionScopeRef {
        owner: SelectionOwnerKind::Window,
        grant_nonce: Incarnation::from_bytes([nonce; 16]),
        service_instance: service,
        scope_identity: Sha256Digest::from_bytes([0x33; 32]),
        catalog_revision: CatalogRevision::INITIAL,
    }
}
/// The selection authority over the catalog fixture's current membership.
fn selection_authority(authority: &Base, selection: SelectionScopeRef) -> SelectionReadAuthority {
    SelectionReadAuthority {
        schema: 1,
        backing: authority.backing,
        index_id: authority.index_id,
        project_ids: authority.project_ids.clone(),
        selection,
        atoms: authority
            .atoms
            .iter()
            .map(|atom| (authority.project_id, atom.clone()))
            .collect(),
        sources: authority.sources.clone(),
        membership_revisions: vec![(authority.project_id, CatalogRevision::INITIAL)],
        source_revision: CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    }
}
fn install_selection(
    gate: &NativeCatalogAdmission,
    worker: WorkerIdentity,
    authority: &SelectionReadAuthority,
    namespace: Incarnation,
) -> Result<(IssueTicket, InstallTicket)> {
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let reservation = gate.reserve_selection_scope(issue, worker, authority, expiry())?;
    gate.acknowledge_install(reservation, worker, reservation.scope)?;
    Ok((issue, reservation))
}
fn selection_live(gate: &NativeCatalogAdmission, nonce: u8) -> bool {
    gate.0
        .lock()
        .unwrap()
        .live_selections
        .contains_key(&Incarnation::from_bytes([nonce; 16]))
}

#[test]
fn selection_admission_requires_a_live_nonce_of_this_service_and_prunes_after_ack() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let selection = selection_ref(worker.service, 0x5c);
    let authority = selection_authority(&catalog, selection);
    // Not registered: nothing admits it.
    assert!(install_selection(&gate, worker, &authority, namespace).is_err());
    // Another service's ref, or a second registration of a nonce, is refused.
    assert!(
        gate.register_selection(&selection_ref(Incarnation::from_bytes([42; 16]), 0x5c))
            .is_err()
    );
    gate.register_selection(&selection)?;
    assert!(gate.register_selection(&selection).is_err());
    let (issue, reservation) = install_selection(&gate, worker, &authority, namespace)?;
    let scope = reservation.scope;
    assert_eq!(scope.selection, selection);
    gate.check_scope_issue(issue, worker, scope)?;
    // Owner revocation: nothing persisted, no generation bump.
    let generation = gate.0.lock().unwrap().generation;
    let pairs = gate.revoke_selection(&[selection.grant_nonce])?;
    assert_eq!(pairs, vec![(worker, scope)]);
    assert_eq!(gate.0.lock().unwrap().generation, generation);
    assert!(gate.check_scope_issue(issue, worker, scope).is_err());
    assert!(
        gate.reserve_selection_scope(issue, worker, &authority, expiry())
            .is_err()
    );
    assert!(gate.selection_revocation_pending(worker, scope)?);
    // The worker's acknowledgement prunes the entry and its high-water.
    gate.record_selection_revoked(worker, scope)?;
    {
        let state = gate.0.lock().unwrap();
        assert_eq!((state.installed.len(), state.highwater.len()), (0, 0));
    }
    assert!(!gate.selection_revocation_pending(worker, scope)?);
    gate.record_selection_revoked(worker, scope)?;
    Ok(())
}

#[test]
fn owner_revocation_races_install_in_either_order_and_never_admits() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    // Revoke before install: the reservation is refused.
    let early = selection_ref(worker.service, 0x61);
    gate.register_selection(&early)?;
    assert!(gate.revoke_selection(&[early.grant_nonce])?.is_empty());
    assert!(
        gate.reserve_selection_scope(
            issue,
            worker,
            &selection_authority(&catalog, early),
            expiry()
        )
        .is_err()
    );
    // Install in flight, then revoke: the acknowledgement cannot settle it
    // as installed, and the reply is classified as a revoked selection.
    let late = selection_ref(worker.service, 0x62);
    gate.register_selection(&late)?;
    let reservation = gate.reserve_selection_scope(
        issue,
        worker,
        &selection_authority(&catalog, late),
        expiry(),
    )?;
    assert!(!gate.selection_install_revoked(&reservation));
    assert_eq!(
        gate.revoke_selection(&[late.grant_nonce])?,
        vec![(worker, reservation.scope)]
    );
    assert!(
        gate.acknowledge_install(reservation, worker, reservation.scope)
            .is_err()
    );
    assert!(gate.selection_install_revoked(&reservation));
    assert!(
        gate.check_scope_issue(issue, worker, reservation.scope)
            .is_err()
    );
    Ok(())
}

#[test]
fn index_fence_revokes_selection_locally_and_persists_no_targets() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let selection = selection_ref(worker.service, 0x5c);
    gate.register_selection(&selection)?;
    let (_, selected) = install_selection(
        &gate,
        worker,
        &selection_authority(&catalog, selection),
        namespace,
    )?;
    let plan = gate.close_admission(
        FenceKey::new(&request(namespace, &catalog))?,
        ScopeTarget::Index(catalog.index_id),
    )?;
    // Selection scopes are never durable fence targets.
    assert!(plan.targets.is_empty() && plan.durable.targets.is_empty());
    assert!(!selection_live(&gate, 0x5c));
    assert_eq!(
        gate.unsettled_selection_revocations(&plan.ticket)?,
        vec![(worker, selected.scope)]
    );
    // The local revocation holds the commit until the worker acknowledges.
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    gate.record_selection_revoked(worker, selected.scope)?;
    assert!(
        gate.unsettled_selection_revocations(&plan.ticket)?
            .is_empty()
    );
    assert!(gate.require_commit_ready(&plan.ticket)?.is_empty());
    Ok(())
}

#[test]
fn selection_uncertain_renewal_current_ack_retains_the_previous_revocation() -> Result<()> {
    selection_uncertain_renewal_fence(false)
}

#[test]
fn selection_uncertain_renewal_previous_ack_retains_the_current_revocation() -> Result<()> {
    selection_uncertain_renewal_fence(true)
}

fn uncertain_selection_renewal(
    gate: &NativeCatalogAdmission,
    worker: WorkerIdentity,
    catalog: &Base,
    namespace: Incarnation,
) -> Result<(SelectionReadAuthority, InstallTicket, InstallTicket)> {
    let selection = selection_ref(worker.service, 0x5c);
    let authority = selection_authority(catalog, selection);
    gate.register_selection(&selection)?;
    let (issue, initial) = install_selection(gate, worker, &authority, namespace)?;
    gate.expire_install_for_test(initial)?;
    let renewal = gate.reserve_selection_scope(issue, worker, &authority, expiry())?;
    assert_eq!(renewal.previous(), Some(initial.scope));
    assert_ne!(
        renewal.scope.lease_generation,
        initial.scope.lease_generation
    );
    // No private installation ACK: either generation may still be at the worker.
    Ok((authority, initial, renewal))
}

fn selection_uncertain_renewal_fence(previous_first: bool) -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let (_, initial, renewal) = uncertain_selection_renewal(&gate, worker, &catalog, namespace)?;
    let plan = gate.close_admission(
        FenceKey::new(&request(namespace, &catalog))?,
        ScopeTarget::Index(catalog.index_id),
    )?;
    assert!(plan.targets.is_empty());
    assert!(
        plan.durable.targets.is_empty(),
        "selection leases stay in memory"
    );
    assert_eq!(
        gate.unsettled_selection_revocations(&plan.ticket)?,
        vec![(worker, renewal.scope), (worker, initial.scope)]
    );
    let [first, second] = if previous_first {
        [initial.scope, renewal.scope]
    } else {
        [renewal.scope, initial.scope]
    };
    gate.record_selection_revoked(worker, first)?;
    assert!(!gate.selection_revocation_pending(worker, first)?);
    assert!(gate.selection_revocation_pending(worker, second)?);
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    assert_eq!(
        gate.unsettled_selection_revocations(&plan.ticket)?,
        vec![(worker, second)]
    );
    if !previous_first {
        // A duplicate current ACK must neither fail nor settle its sibling.
        gate.record_selection_revoked(worker, first)?;
        assert!(gate.selection_revocation_pending(worker, second)?);
        assert!(gate.require_commit_ready(&plan.ticket).is_err());
    }
    gate.record_selection_revoked(worker, second)?;
    assert!(gate.require_commit_ready(&plan.ticket)?.is_empty());
    assert!(
        gate.unsettled_selection_revocations(&plan.ticket)?
            .is_empty()
    );
    assert!(!gate.selection_revocation_pending(worker, first)?);
    assert!(!gate.selection_revocation_pending(worker, second)?);
    let state = gate.0.lock().unwrap();
    assert!(state.installed.is_empty());
    assert!(state.highwater.is_empty());
    Ok(())
}

#[test]
fn selection_uncertain_renewal_owner_revocation_retains_both_exact_acks() -> Result<()> {
    for previous_first in [false, true] {
        let root = tempfile::tempdir()?;
        let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
        let (authority, initial, renewal) =
            uncertain_selection_renewal(&gate, worker, &catalog, namespace)?;
        assert_eq!(
            gate.revoke_selection(&[authority.selection.grant_nonce])?,
            vec![(worker, renewal.scope), (worker, initial.scope)]
        );
        let [first, second] = if previous_first {
            [initial.scope, renewal.scope]
        } else {
            [renewal.scope, initial.scope]
        };
        gate.record_selection_revoked(worker, first)?;
        if !previous_first {
            gate.record_selection_revoked(worker, first)?;
        }
        assert!(!gate.selection_revocation_pending(worker, first)?);
        assert!(gate.selection_revocation_pending(worker, second)?);
        assert_eq!(
            gate.revoke_selection(&[authority.selection.grant_nonce])?,
            vec![(worker, second)]
        );
        assert_eq!(gate.0.lock().unwrap().installed.len(), 1);
        gate.record_selection_revoked(worker, second)?;
        assert!(gate.0.lock().unwrap().installed.is_empty());
        assert!(gate.0.lock().unwrap().highwater.is_empty());
    }
    Ok(())
}

#[test]
fn worker_reap_settles_local_selection_revocations() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let selection = selection_ref(worker.service, 0x5c);
    gate.register_selection(&selection)?;
    install_selection(
        &gate,
        worker,
        &selection_authority(&catalog, selection),
        namespace,
    )?;
    let plan = gate.close_admission(
        FenceKey::new(&request(namespace, &catalog))?,
        ScopeTarget::Index(catalog.index_id),
    )?;
    assert!(plan.targets.is_empty());
    assert!(gate.require_commit_ready(&plan.ticket).is_err());
    gate.observe_worker_reaped(worker)?;
    assert!(
        gate.unsettled_selection_revocations(&plan.ticket)?
            .is_empty()
    );
    assert!(gate.require_commit_ready(&plan.ticket)?.is_empty());
    Ok(())
}

#[test]
fn refused_close_revokes_no_selection_scope() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let selection = selection_ref(worker.service, 0x5c);
    gate.register_selection(&selection)?;
    let (issue, selected) = install_selection(
        &gate,
        worker,
        &selection_authority(&catalog, selection),
        namespace,
    )?;
    // Another namespace, then another expected revision: both refused
    // before any local revocation.
    let foreign = request(Incarnation::from_bytes([42; 16]), &catalog);
    assert!(
        gate.close_admission(
            FenceKey::new(&foreign)?,
            ScopeTarget::Index(catalog.index_id)
        )
        .is_err()
    );
    let mut stale = request(namespace, &catalog);
    stale.expected_catalog_revision = CatalogRevision::INITIAL.next()?;
    assert!(
        gate.close_admission(FenceKey::new(&stale)?, ScopeTarget::Index(catalog.index_id))
            .is_err()
    );
    assert!(selection_live(&gate, 0x5c));
    gate.check_scope_issue(issue, worker, selected.scope)?;
    Ok(())
}

#[test]
fn restored_pending_fence_holds_no_targets_and_no_local_revocation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    let selection = selection_ref(worker.service, 0x5c);
    gate.register_selection(&selection)?;
    install_selection(
        &gate,
        worker,
        &selection_authority(&catalog, selection),
        namespace,
    )?;
    let request = request(namespace, &catalog);
    let plan = gate.close_request_admission(&request)?;
    assert!(plan.durable.targets.is_empty());
    let (intent, _) = durable_fixture(&request, &plan.durable)?;
    let restored = NativeCatalogAdmission::new(
        Incarnation::from_bytes([21; 16]),
        namespace,
        CatalogRevision::INITIAL,
        1,
    )?;
    let restored_plan = restored.restore_pending_fence(&intent)?;
    assert_eq!(restored_plan.durable, plan.durable);
    assert!(
        restored
            .unsettled_selection_revocations(&restored_plan.ticket)?
            .is_empty()
    );
    let state = restored.0.lock().unwrap();
    assert!(state.live_selections.is_empty());
    assert!(
        state
            .pending
            .as_ref()
            .unwrap()
            .local_selection_revocations
            .is_empty()
    );
    Ok(())
}

fn agent_ref(service: Incarnation, nonce: u8) -> SelectionScopeRef {
    SelectionScopeRef {
        owner: SelectionOwnerKind::Agent,
        ..selection_ref(service, nonce)
    }
}
/// U105: the open authority of the catalog fixture's whole index.
fn open_authority(authority: &Base, selection: SelectionScopeRef) -> OpenReadAuthority {
    OpenReadAuthority {
        schema: 1,
        backing: authority.backing,
        index_id: authority.index_id,
        selection,
        sources: authority.sources.clone(),
        source_revision: CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    }
}

/// U105: agent nonces have their own budget; a full agent budget leaves
/// window/admin registration open and the reverse.
#[test]
fn agent_nonces_have_their_own_budget() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, _, _) = fixture(root.path().to_str().unwrap());
    for nonce in 0..MAX_AGENT_SELECTIONS as u8 {
        gate.register_selection(&agent_ref(worker.service, nonce))?;
    }
    assert!(
        gate.register_selection(&agent_ref(worker.service, 0xe5))
            .is_err(),
        "the agent budget is full"
    );
    for nonce in 0x40..0x40 + 160u8 {
        gate.register_selection(&selection_ref(worker.service, nonce))?;
    }
    assert!(
        gate.register_selection(&selection_ref(worker.service, 0xf0))
            .is_err()
    );
    // Revoking (pruning) one agent nonce frees exactly one agent slot.
    assert!(
        gate.revoke_selection(&[Incarnation::from_bytes([3; 16])])?
            .is_empty()
    );
    gate.register_selection(&agent_ref(worker.service, 0xe5))?;
    assert!(
        gate.register_selection(&agent_ref(worker.service, 0xe6))
            .is_err()
    );
    Ok(())
}

/// U105: an open read installs only under a live agent nonce, never under a
/// window or admin owner, and revokes like any selection.
#[test]
fn open_scope_requires_a_live_agent_nonce_and_revokes_like_a_selection() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    let selection = agent_ref(worker.service, 0x71);
    let authority = open_authority(&catalog, selection);
    assert!(
        gate.reserve_open_scope(issue, worker, &authority, expiry())
            .is_err(),
        "unregistered nonce"
    );
    // A window nonce of the same bytes never admits an agent authority.
    gate.register_selection(&selection_ref(worker.service, 0x71))?;
    assert!(
        gate.reserve_open_scope(issue, worker, &authority, expiry())
            .is_err()
    );
    assert!(gate.revoke_selection(&[selection.grant_nonce])?.is_empty());
    gate.register_selection(&selection)?;
    let reservation = gate.reserve_open_scope(issue, worker, &authority, expiry())?;
    assert!(reservation.requires_install);
    assert_eq!(reservation.scope.selection, selection);
    gate.acknowledge_install(reservation, worker, reservation.scope)?;
    gate.check_scope_issue(issue, worker, reservation.scope)?;
    let mut window = authority.clone();
    window.selection.owner = SelectionOwnerKind::Window;
    assert!(
        gate.reserve_open_scope(issue, worker, &window, expiry())
            .is_err()
    );
    assert_eq!(
        gate.revoke_selection(&[selection.grant_nonce])?,
        vec![(worker, reservation.scope)]
    );
    assert!(
        gate.check_scope_issue(issue, worker, reservation.scope)
            .is_err()
    );
    gate.record_selection_revoked(worker, reservation.scope)?;
    assert_eq!(gate.0.lock().unwrap().installed.len(), 0);
    Ok(())
}

/// U105: agent installations are counted apart from W4/A2 selections.
#[test]
fn agent_installations_do_not_consume_window_selection_capacity() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, worker, catalog, namespace) = fixture(root.path().to_str().unwrap());
    gate.register_worker(worker)?;
    let issue = gate.begin_issue(namespace, CatalogRevision::INITIAL)?;
    for nonce in 0..MAX_AGENT_SELECTIONS as u8 {
        let selection = agent_ref(worker.service, nonce);
        gate.register_selection(&selection)?;
        let reservation = gate.reserve_open_scope(
            issue,
            worker,
            &open_authority(&catalog, selection),
            expiry(),
        )?;
        gate.acknowledge_install(reservation, worker, reservation.scope)?;
    }
    for nonce in 0x40..0x40 + 80u8 {
        let selection = selection_ref(worker.service, nonce);
        gate.register_selection(&selection)?;
        let reservation = gate.reserve_selection_scope(
            issue,
            worker,
            &selection_authority(&catalog, selection),
            expiry(),
        )?;
        gate.acknowledge_install(reservation, worker, reservation.scope)?;
    }
    let overflow = selection_ref(worker.service, 0xf0);
    gate.register_selection(&overflow)?;
    assert!(
        gate.reserve_selection_scope(
            issue,
            worker,
            &selection_authority(&catalog, overflow),
            expiry()
        )
        .is_err(),
        "the window bound is unchanged by agent entries"
    );
    Ok(())
}
