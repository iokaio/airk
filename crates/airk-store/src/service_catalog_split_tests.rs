// SPDX-License-Identifier: Apache-2.0
//! Metadata admission/refusal fixtures. Native publication qualification lives
//! in process tests; these fixtures do not assert filesystem/build readiness.
use super::*;
use airk_core::{
    index_catalog::{PathScope, ScopeAtom, SourceRootId},
    source_policy::SplitDestination,
};
pub(in crate::service::registry::catalog) fn fixture() -> Result<(
    tempfile::TempDir,
    ServiceRegistry,
    SplitRequest,
    SourcePolicy,
    SplitPolicyPlan,
)> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    let root = base.join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("original.md"), "original remainder")?;
    std::fs::create_dir(base.join("control"))?;
    let mut registry =
        ServiceRegistry::open(&base.join("control"), Incarnation::from_bytes([1; 16]))?;
    let registration = ProjectRegistration::new(
        root.to_str().unwrap().into(),
        base.join("original-data").to_str().unwrap().into(),
        "original".into(),
    )?;
    registry.register(&registration)?;
    let old = super::super::schema_digest(&registry.connection)?;
    super::super::install_schema(&registry.connection)?;
    registry.connection.execute(
        "INSERT INTO index_catalog_meta VALUES(1,1,1,0,0,0,?1,?2,?3)",
        params![
            Incarnation::from_bytes([2; 16]).to_string(),
            old.to_string(),
            Sha256Digest::from_bytes([3; 32]).to_string()
        ],
    )?;
    registry
        .connection
        .pragma_update(None, "user_version", 12)?;
    let id = IndexId::from_identity(Incarnation::from_bytes([4; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([5; 16]));
    registry.connection.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,'Original',1,1,0,NULL,0,'complete')",params![id.to_string(),registration.binding.to_string(),registration.root,registration.data_root,registration.collection])?;
    registry.connection.execute(
        "INSERT INTO index_sources VALUES(?1,?2,?3,1,1)",
        params![source.to_string(), id.to_string(), registration.root],
    )?;
    let tx = registry.connection.transaction()?;
    super::super::apply_authority_v1_layout(&tx)?;
    tx.execute(
        "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
        [],
    )?;
    tx.commit()?;
    let observed = registry.catalog_split_source_state(id, CatalogRevision::INITIAL)?;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let caller = Incarnation::from_bytes([6; 16]);
    let key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([7; 16]),
    };
    let destination_registration = ProjectRegistration::new(
        registration.root.clone(),
        base.join("uncreated-destination").to_str().unwrap().into(),
        "destination".into(),
    )?;
    let target = SplitDestination {
        index_id: IndexId::from_identity(Incarnation::from_bytes([8; 16])),
        owner_binding: destination_registration.binding,
        collection: destination_registration.collection.clone(),
        source_ids: BTreeMap::from([(
            source,
            SourceRootId::from_identity(Incarnation::from_bytes([9; 16])),
        )]),
    };
    let selectors = vec![ScopeAtom {
        source_root_id: source,
        kind: PathScope::Subtree,
        path: "docs".into(),
    }];
    let plan = observed.policy.compile_split(
        observed.policy.identity()?,
        airk_core::catalog_management::catalog_operation_id(caller, &key)?,
        &selectors,
        target.clone(),
    )?;
    let request = SplitRequest {
        extraction_policy: None,
        schema: 1,
        required_policy_contract: SPLIT_CONTRACT,
        caller,
        key,
        expected_catalog_revision: observed.catalog_revision,
        original_index: id,
        original_registration: registration,
        expected_definition_revision: observed.index.definition_revision,
        expected_source_revision: observed.index.source_revision,
        original_incarnation: Incarnation::from_bytes([10; 16]),
        original_settings_digest: Sha256Digest::from_bytes([11; 32]),
        source_configuration: observed.source_configuration,
        baseline: observed.baseline,
        before_policy_digest: observed.policy.identity()?,
        selectors,
        destination_name: "Destination".into(),
        destination_registration,
        destination: target,
        compiled_plan_digest: framed_digest(
            b"airk/catalog-split-plan/1",
            &[&serde_json::to_vec(&plan)?],
        ),
        projects: vec![],
        capacity: SplitCapacityReview {
            destination_growth: 1 << 20,
            destination_temporary: 1 << 20,
            original_rewrite: 1 << 20,
            cleanup_allowance: 2 << 20,
            original_quota_digest: Sha256Digest::from_bytes([12; 32]),
            destination_quota_digest: Sha256Digest::from_bytes([13; 32]),
        },
        require_semantic_ready: false,
        original_watch_digest: observed.watch_digest,
        destination_watch_enabled: false,
    };
    let mut request = request;
    request.capacity.original_quota_digest = crate::quota::Quota::review_service_funding_policy(
        Path::new(&request.original_registration.data_root),
    )?
    .digest;
    request.capacity.destination_quota_digest = crate::quota::Quota::review_service_funding_policy(
        Path::new(&request.destination_registration.data_root),
    )?
    .digest;
    request.validate_plan(&observed.policy, &plan)?;
    Ok((temp, registry, request, observed.policy, plan))
}
#[test]
fn accepted_split_is_durable_before_any_funding_or_destination_effect() -> Result<()> {
    let (temp, mut registry, request, before, plan) = fixture()?;
    let original = std::fs::read(temp.path().join("source/original.md"))?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    assert_eq!(accepted.receipt.stage, SplitStage::Accepted);
    assert!(
        accepted.receipt.capacity_receipt.is_none() && accepted.receipt.funding_receipts.is_empty()
    );
    assert!(!Path::new(&request.destination_registration.data_root).exists());
    assert_eq!(
        registry.catalog_split_receipt(request.caller, &request.key)?,
        Some(accepted.clone())
    );
    assert_eq!(
        registry.catalog_reserve_split(&request, &before, &plan)?,
        accepted
    );
    assert_eq!(
        registry.catalog_state()?.unwrap().policy_contract,
        SPLIT_CONTRACT
    );
    assert_eq!(
        index_record(&registry.connection, request.original_index)?.policy,
        None
    );
    assert_eq!(
        std::fs::read(temp.path().join("source/original.md"))?,
        original
    );
    let reopened = ServiceRegistry::open_saved(&registry.root)?;
    assert_eq!(
        reopened.catalog_split_status(request.operation_id()?)?,
        Some(accepted)
    );
    Ok(())
}
#[test]
fn stale_or_conflicting_split_admission_rolls_back_before_capability_or_storage() -> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let mut stale = request.clone();
    stale.expected_source_revision = stale.expected_source_revision.next()?;
    assert!(
        registry
            .catalog_reserve_split(&stale, &before, &plan)
            .is_err()
    );
    assert_eq!(registry.catalog_state()?.unwrap().policy_contract, 0);
    let saved = registry.catalog_reserve_split(&request, &before, &plan)?;
    let mut crossed = request.clone();
    crossed.destination_name = "Different request".into();
    assert!(
        registry
            .catalog_reserve_split(&crossed, &before, &plan)
            .is_err()
    );
    assert_eq!(
        registry.catalog_split_status(request.operation_id()?)?,
        Some(saved)
    );
    Ok(())
}
#[test]
fn split_receipt_cas_and_pre_fence_cancel_preserve_original_and_do_not_claim_cleanup() -> Result<()>
{
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    let canceled =
        registry.catalog_cancel_split(request.operation_id()?, accepted.receipt_digest)?;
    assert!(matches!(
        canceled.receipt.state,
        SplitRunState::CanceledBeforeFence {
            cleanup_pending: true
        }
    ));
    assert!(
        canceled.receipt.original_fence.is_none()
            && canceled.receipt.cancellation_cleanup_receipt.is_none()
    );
    assert!(
        registry
            .catalog_cancel_split(request.operation_id()?, accepted.receipt_digest)
            .is_err()
    );
    assert!(
        registry
            .catalog_finish_split(request.operation_id()?, canceled.receipt_digest)
            .is_err()
    );
    assert!(
        registry
            .catalog_split_funding_terminal(request.operation_id()?)
            .is_err()
    );
    assert_eq!(
        index_record(&registry.connection, request.original_index)?.policy,
        None
    );
    Ok(())
}
#[test]
fn staged_owner_role_and_complete_recovery_acknowledgments_reject_tampering() -> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let state = registry.catalog_reserve_split(&request, &before, &plan)?;
    let original = SplitOwnerReference {
        required_contract: SPLIT_CONTRACT,
        operation_id: state.receipt.operation_id,
        request_digest: state.receipt.request_digest,
        index_id: request.original_index,
        role: SplitOwnerRole::OriginalRemainder,
        activation_receipt: None,
    };
    let destination = SplitOwnerReference {
        index_id: request.destination.index_id,
        role: SplitOwnerRole::Destination,
        ..original.clone()
    };
    assert_eq!(
        original.evidence_constraint(&request, &state.receipt)?,
        SplitEvidenceConstraint::PreserveOriginalAdmission
    );
    assert_eq!(
        destination.evidence_constraint(&request, &state.receipt)?,
        SplitEvidenceConstraint::SealedDestination
    );
    let mut crossed = original.clone();
    crossed.role = SplitOwnerRole::Destination;
    assert!(crossed.validate_for(&request, &state.receipt).is_err());
    let mut early = original;
    early.activation_receipt = Some(Sha256Digest::from_bytes([99; 32]));
    assert!(early.validate_for(&request, &state.receipt).is_err());
    let recovery = SplitRecoveryRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([20; 16]),
            ..request.key.clone()
        },
        operation_id: state.receipt.operation_id,
        request_digest: state.receipt.request_digest,
        expected_receipt_digest: state.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Cancel,
    };
    let invalid = SplitRecoveryReceipt {
        request_digest: framed_digest(
            b"airk/catalog-split-recovery/1",
            &[&serde_json::to_vec(&recovery)?],
        ),
        request: recovery,
        outcome: SplitRecoveryOutcome::AlreadyComplete {
            receipt_digest: Sha256Digest::from_bytes([99; 32]),
        },
    };
    assert!(invalid.validate_for(&request, &state.receipt).is_err());
    Ok(())
}

#[test]
fn canceled_never_prepared_destination_uses_pinned_absence_and_finishes_without_storage()
-> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    let canceled =
        registry.catalog_cancel_split(request.operation_id()?, accepted.receipt_digest)?;
    let state = registry.catalog_split_state(request.operation_id()?)?;
    let proof = NativeSplitAbsentDestination::capture(&state)?;
    let lease = crate::service::ProjectLease::acquire(
        &registry.root,
        request.destination_registration.binding,
    )?;
    let authorized = registry.catalog_authorize_split_absent_cleanup(&lease, proof)?;
    assert!(authorized.receipt.physical_cleanup_authorization.is_some());
    assert_ne!(authorized.receipt_digest, canceled.receipt_digest);
    let complete =
        registry.catalog_finish_split(request.operation_id()?, authorized.receipt_digest)?;
    assert_eq!(
        complete.receipt.state,
        SplitRunState::CanceledBeforeFence {
            cleanup_pending: false
        }
    );
    assert!(complete.receipt.cancellation_cleanup_receipt.is_some());
    assert!(!Path::new(&request.destination_registration.data_root).exists());
    assert_eq!(
        index_record(&registry.connection, request.original_index)?.policy,
        None
    );
    Ok(())
}
#[test]
fn canceled_absence_refuses_new_or_unowned_destination_without_deleting_it() -> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    registry.catalog_cancel_split(request.operation_id()?, accepted.receipt_digest)?;
    let state = registry.catalog_split_state(request.operation_id()?)?;
    let destination = Path::new(&request.destination_registration.data_root);
    std::fs::create_dir(destination)?;
    let proof = NativeSplitAbsentDestination::capture(&state)?;
    std::fs::create_dir(destination.join("collections-1.4"))?;
    let lease = crate::service::ProjectLease::acquire(
        &registry.root,
        request.destination_registration.binding,
    )?;
    assert!(
        registry
            .catalog_authorize_split_absent_cleanup(&lease, proof)
            .is_err()
    );
    let unexpected = destination
        .join("collections-1.4")
        .join(&request.destination_registration.collection);
    std::fs::create_dir(&unexpected)?;
    std::fs::write(unexpected.join("user.txt"), "preserve")?;
    assert!(NativeSplitAbsentDestination::capture(&state).is_err());
    assert_eq!(std::fs::read(unexpected.join("user.txt"))?, b"preserve");
    assert!(
        registry
            .catalog_split_status(request.operation_id()?)?
            .unwrap()
            .receipt
            .physical_cleanup_authorization
            .is_none()
    );
    Ok(())
}

#[test]
fn split_refuses_setup_job_plan_and_saved_recovery_action_key_owners() -> Result<()> {
    // Key ownership is checked before any effect or key reinterpretation. These
    // isolated metadata rows exercise each schema's independent owning key.
    for kind in ["setup", "job", "plan"] {
        let (_temp, mut registry, request, before, plan) = fixture()?;
        match kind {
            "setup" => {
                registry.connection.execute("INSERT INTO index_operation_receipts VALUES(?1,'catalog_index_setup','reserved',1,?2)",params![request.operation_id()?.to_string(),b"{}".as_slice()])?;
            }
            "job" => {
                registry.enqueue(
                    request.original_registration.binding,
                    request.caller,
                    &request.key,
                    JobKind::Inspect,
                )?;
            }
            "plan" => {
                let id = Sha256Digest::from_bytes([30; 32]);
                registry.connection.execute("INSERT INTO service_plans(id,request,engine,phase,paused,paused_at,canceled,fallback_approved,inspection_until,created_ms,updated_ms) VALUES(?1,?2,?1,'complete',0,0,0,0,0,1,1)",params![id.to_string(),b"{}".as_slice()])?;
                registry.connection.execute(
                    "INSERT INTO service_plan_requests VALUES(?1,?2,?3,?4,?5)",
                    params![
                        request.key.namespace.to_string(),
                        i64::try_from(request.key.epoch)?,
                        request.key.nonce.to_string(),
                        request.caller.to_string(),
                        id.to_string()
                    ],
                )?;
            }
            _ => unreachable!(),
        }
        assert!(
            registry
                .catalog_reserve_split(&request, &before, &plan)
                .is_err(),
            "{kind} key reused"
        );
        assert_eq!(registry.catalog_state()?.unwrap().policy_contract, 0);
        assert!(!Path::new(&request.destination_registration.data_root).exists());
    }
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    let canceled =
        registry.catalog_cancel_split(request.operation_id()?, accepted.receipt_digest)?;
    let recovery_key = RequestKey {
        nonce: Incarnation::from_bytes([31; 16]),
        ..request.key.clone()
    };
    let recovery = SplitRecoveryRequest {
        caller: request.caller,
        key: recovery_key.clone(),
        operation_id: request.operation_id()?,
        request_digest: request.identity()?,
        expected_receipt_digest: accepted.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Cancel,
    };
    let receipt = SplitRecoveryReceipt {
        request_digest: framed_digest(
            b"airk/catalog-split-recovery/1",
            &[&serde_json::to_vec(&recovery)?],
        ),
        request: recovery,
        outcome: SplitRecoveryOutcome::CancelBeforeFenceAccepted {
            receipt_digest: canceled.receipt_digest,
            cleanup_pending: true,
            resulting_receipt: Box::new(canceled.receipt.clone()),
        },
    };
    registry.catalog_record_split_recovery(&receipt)?;
    let mut crossed = request.clone();
    crossed.key = recovery_key;
    let crossed_plan = before.compile_split(
        before.identity()?,
        crossed.operation_id()?,
        &crossed.selectors,
        crossed.destination.clone(),
    )?;
    crossed.compiled_plan_digest = framed_digest(
        b"airk/catalog-split-plan/1",
        &[&serde_json::to_vec(&crossed_plan)?],
    );
    assert!(
        registry
            .catalog_reserve_split(&crossed, &before, &crossed_plan)
            .is_err()
    );
    assert_eq!(
        registry.catalog_split_recovery_receipt(receipt.request.caller, &receipt.request.key)?,
        Some(receipt)
    );
    assert_eq!(
        registry.catalog_reserve_split(&request, &before, &plan)?,
        canceled
    );
    Ok(())
}

#[test]
fn explicit_metadata_resume_is_atomic_exact_and_independent_of_later_failure() -> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    let stopped = registry.catalog_note_split_failure(
        request.operation_id()?,
        accepted.receipt_digest,
        "capacity_pressure",
    )?;
    let recovery = SplitRecoveryRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([40; 16]),
            ..request.key.clone()
        },
        operation_id: request.operation_id()?,
        request_digest: request.identity()?,
        expected_receipt_digest: stopped.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Resume,
    };
    let acknowledgment = registry.catalog_resume_split_continuation(&recovery)?;
    let SplitRecoveryOutcome::ContinuationAccepted {
        receipt_digest,
        resulting_receipt,
    } = &acknowledgment.outcome
    else {
        anyhow::bail!("metadata continuation required")
    };
    assert_eq!(resulting_receipt.state, SplitRunState::Pending);
    assert_eq!(resulting_receipt.stage, SplitStage::Accepted);
    assert_eq!(resulting_receipt.jobs, stopped.receipt.jobs);
    assert_eq!(
        resulting_receipt.previous_receipt_digest,
        Some(stopped.receipt_digest)
    );
    acknowledgment.validate_for(&request, &stopped.receipt)?;
    let mut tampered = acknowledgment.clone();
    if let SplitRecoveryOutcome::ContinuationAccepted {
        receipt_digest,
        resulting_receipt,
    } = &mut tampered.outcome
    {
        resulting_receipt.destination_storage = Some(Sha256Digest::from_bytes([41; 32]));
        *receipt_digest = resulting_receipt.identity(&request)?;
    }
    assert!(
        tampered.validate_for(&request, &stopped.receipt).is_err(),
        "metadata resume must not introduce a proof"
    );
    let mut wrong_action = acknowledgment.clone();
    wrong_action.request.action = SplitRecoveryAction::Cancel;
    wrong_action.request_digest = framed_digest(
        b"airk/catalog-split-recovery/1",
        &[&serde_json::to_vec(&wrong_action.request)?],
    );
    assert!(
        wrong_action
            .validate_for(&request, &stopped.receipt)
            .is_err()
    );
    registry.catalog_note_split_failure(
        request.operation_id()?,
        *receipt_digest,
        "capacity_pressure",
    )?;
    assert_eq!(
        registry.catalog_resume_split_continuation(&recovery)?,
        acknowledgment
    );
    let reopened = ServiceRegistry::open_saved(&registry.root)?;
    assert_eq!(
        reopened.catalog_split_recovery_receipt(recovery.caller, &recovery.key)?,
        Some(acknowledgment)
    );
    assert!(matches!(
        reopened
            .catalog_split_status(request.operation_id()?)?
            .unwrap()
            .receipt
            .state,
        SplitRunState::NeedsAttention { .. }
    ));
    Ok(())
}
#[test]
fn metadata_continuation_phase_matrix_preserves_nonterminal_phases_and_refuses_canceled_terminal()
-> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    for (stage, allowed) in [
        (SplitStage::Accepted, true),
        (SplitStage::DestinationPrepared, true),
        (SplitStage::Building, true),
        (SplitStage::DestinationValidated, true),
        (SplitStage::OriginalFenced, true),
        (SplitStage::RemainderPublished, true),
        (SplitStage::MembershipApplied, true),
        (SplitStage::CleanupPending, true),
        (SplitStage::Complete, false),
    ] {
        let mut receipt = accepted.receipt.clone();
        receipt.stage = stage;
        assert_eq!(
            receipt.supports_metadata_continuation(),
            allowed,
            "{stage:?}"
        );
        receipt.state = SplitRunState::CanceledBeforeFence {
            cleanup_pending: true,
        };
        assert_eq!(
            receipt.supports_metadata_continuation(),
            stage != SplitStage::Complete
        );
    }
    let canceled =
        registry.catalog_cancel_split(request.operation_id()?, accepted.receipt_digest)?;
    let recovery = SplitRecoveryRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([42; 16]),
            ..request.key.clone()
        },
        operation_id: request.operation_id()?,
        request_digest: request.identity()?,
        expected_receipt_digest: canceled.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Resume,
    };
    let ack = registry.catalog_resume_split_continuation(&recovery)?;
    let SplitRecoveryOutcome::ContinuationAccepted {
        resulting_receipt, ..
    } = ack.outcome
    else {
        anyhow::bail!("cleanup metadata continuation required")
    };
    assert_eq!(resulting_receipt.state, canceled.receipt.state);
    assert!(resulting_receipt.jobs.is_empty());
    Ok(())
}

#[test]
fn cancel_action_journals_exact_ack_and_wake_never_retries_attention_or_terminal() -> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    assert_eq!(registry.catalog_pending_splits(8)?, vec![accepted.clone()]);
    let stopped = registry.catalog_note_split_failure(
        request.operation_id()?,
        accepted.receipt_digest,
        "capacity_pressure",
    )?;
    assert!(registry.catalog_pending_splits(8)?.is_empty());
    let action = SplitRecoveryRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([50; 16]),
            ..request.key.clone()
        },
        operation_id: request.operation_id()?,
        request_digest: request.identity()?,
        expected_receipt_digest: stopped.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Cancel,
    };
    let acknowledgment = registry.catalog_cancel_split_action(&action)?;
    let current = registry
        .catalog_split_status(request.operation_id()?)?
        .unwrap();
    assert_eq!(registry.catalog_pending_splits(8)?, vec![current.clone()]);
    assert_eq!(
        registry.catalog_split_recovery_receipt(action.caller, &action.key)?,
        Some(acknowledgment.clone())
    );
    let lease = crate::service::ProjectLease::acquire(
        &registry.root,
        request.destination_registration.binding,
    )?;
    let proof = NativeSplitAbsentDestination::capture(
        &registry.catalog_split_state(request.operation_id()?)?,
    )?;
    let authorized = registry.catalog_authorize_split_absent_cleanup(&lease, proof)?;
    let terminal =
        registry.catalog_finish_split(request.operation_id()?, authorized.receipt_digest)?;
    assert!(registry.catalog_pending_splits(8)?.is_empty());
    assert_eq!(
        registry.catalog_cancel_split_action(&action)?,
        acknowledgment
    );
    assert!(
        registry
            .catalog_cancel_split(request.operation_id()?, terminal.receipt_digest)
            .is_err()
    );
    assert!(registry.catalog_pending_splits(0).is_err());
    assert!(registry.catalog_pending_splits(9).is_err());
    Ok(())
}
#[test]
fn cleanup_job_recovery_is_atomic_native_claimed_and_runs_while_paused() -> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    let reserved = registry
        .catalog_reserve_split_destination(request.operation_id()?, accepted.receipt_digest)?;
    let canceled =
        registry.catalog_cancel_split(request.operation_id()?, reserved.receipt_digest)?;
    let action = SplitRecoveryRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([51; 16]),
            ..request.key.clone()
        },
        operation_id: request.operation_id()?,
        request_digest: request.identity()?,
        expected_receipt_digest: canceled.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Resume,
    };
    let engine = Sha256Digest::from_bytes([52; 32]);
    let configuration = ConfigurationRequest {
        prepare: false,
        incarnation: request.operation_id()?,
        settings: request.original_settings_digest,
        configuration: ProjectConfiguration::SplitWork {
            operation_id: request.operation_id()?,
            request_digest: request.identity()?,
            expected_receipt_digest: canceled.receipt_digest,
            purpose: SplitJobPurpose::DestinationCleanup,
            require_semantic_ready: false,
        },
    };
    assert!(
        registry
            .enqueue_configuration(
                request.destination_registration.binding,
                request.caller,
                &action.key,
                &configuration,
                engine
            )
            .is_err()
    );
    let acknowledgment =
        registry.catalog_resume_split_job_action(&action, engine, &configuration)?;
    let SplitRecoveryOutcome::ResumeAccepted { accepted_job, .. } = acknowledgment.outcome else {
        anyhow::bail!("native cleanup child required")
    };
    assert_eq!(
        registry.catalog_resume_split_job_action(&action, engine, &configuration)?,
        acknowledgment
    );
    registry.pause(request.destination_registration.binding, true)?;
    let mut lease = crate::service::ProjectLease::acquire(
        &registry.root,
        request.destination_registration.binding,
    )?;
    registry.admit_worker(&mut lease)?;
    assert_eq!(
        registry.claim_job_id(&lease, accepted_job)?.unwrap().id,
        accepted_job
    );
    let claimed = registry.claimed_split_work(&lease, accepted_job, engine)?;
    assert_eq!(claimed.job.purpose, SplitJobPurpose::DestinationCleanup);
    assert!(
        registry
            .record_configuration_result(
                &lease,
                accepted_job,
                &serde_json::json!({"cleanup_complete":true})
            )
            .is_err()
    );
    registry.complete_job(
        &lease,
        accepted_job,
        airk_core::service::JobOutcome::Canceled,
    )?;
    assert!(
        registry
            .claimed_split_work(&lease, accepted_job, engine)
            .is_err()
    );
    assert!(registry.catalog_pending_splits(8)?.is_empty());
    let generic = RequestKey {
        nonce: Incarnation::from_bytes([53; 16]),
        ..request.key.clone()
    };
    assert!(
        registry
            .retry_job(accepted_job, request.caller, &generic)
            .is_err()
    );
    assert_eq!(
        registry.catalog_split_recovery_receipt(action.caller, &action.key)?,
        Some(acknowledgment)
    );
    Ok(())
}

#[test]
fn canceled_cleanup_failure_latches_attention_until_explicit_exact_continuation() -> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    let canceled =
        registry.catalog_cancel_split(request.operation_id()?, accepted.receipt_digest)?;
    let stopped = registry.catalog_note_split_failure(
        request.operation_id()?,
        canceled.receipt_digest,
        "cleanup_unavailable",
    )?;
    assert_eq!(stopped.receipt.state, canceled.receipt.state);
    assert_eq!(
        stopped.receipt.cleanup_attention.as_deref(),
        Some("cleanup_unavailable")
    );
    assert!(registry.catalog_pending_splits(8)?.is_empty());
    let action = SplitRecoveryRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([54; 16]),
            ..request.key.clone()
        },
        operation_id: request.operation_id()?,
        request_digest: request.identity()?,
        expected_receipt_digest: stopped.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Resume,
    };
    let acknowledgment = registry.catalog_resume_split_continuation(&action)?;
    acknowledgment.validate_for(&request, &stopped.receipt)?;
    let current = registry
        .catalog_split_status(request.operation_id()?)?
        .unwrap();
    assert_eq!(current.receipt.state, canceled.receipt.state);
    assert!(current.receipt.cleanup_attention.is_none());
    assert_eq!(registry.catalog_pending_splits(8)?, vec![current]);
    Ok(())
}

#[test]
fn native_minimum_metadata_capacity_refuses_before_intent_and_keeps_key_reusable() -> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let minimum = native_metadata_capacity(&request, &before, &plan)?;
    assert!(minimum.total > 2 * (49152 + 16384));
    let mut insufficient = request.clone();
    insufficient.capacity.cleanup_allowance = 1;
    assert!(
        registry
            .catalog_reserve_split(&insufficient, &before, &plan)
            .is_err()
    );
    assert_eq!(registry.catalog_state()?.unwrap().policy_contract, 0);
    assert!(!Path::new(&request.destination_registration.data_root).exists());
    assert!(
        registry
            .catalog_split_receipt(request.caller, &request.key)?
            .is_none()
    );
    registry.catalog_reserve_split(&request, &before, &plan)?;
    Ok(())
}
#[test]
fn frozen_finalization_attention_has_separate_cas_and_old_ack_cannot_clear_later_failure()
-> Result<()> {
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    let canceled =
        registry.catalog_cancel_split(request.operation_id()?, accepted.receipt_digest)?;
    let lease = crate::service::ProjectLease::acquire(
        &registry.root,
        request.destination_registration.binding,
    )?;
    let absent = NativeSplitAbsentDestination::capture(
        &registry.catalog_split_state(request.operation_id()?)?,
    )?;
    let authorized = registry.catalog_authorize_split_absent_cleanup(&lease, absent)?;
    let frozen = registry
        .catalog_advance_split_finalization(request.operation_id()?, authorized.receipt_digest)?;
    assert_eq!(frozen.receipt_digest, authorized.receipt_digest);
    assert_eq!(
        frozen.finalization.as_ref().unwrap().progress,
        SplitFinalizationProgress::Prepared
    );
    let stopped = registry.catalog_note_split_failure(
        request.operation_id()?,
        frozen.receipt_digest,
        "terminal_storage_unavailable",
    )?;
    assert_eq!(stopped.receipt_digest, frozen.receipt_digest);
    assert!(stopped.finalization.as_ref().unwrap().attention.is_some());
    assert!(registry.catalog_pending_splits(8)?.is_empty());
    let action = SplitRecoveryRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([70; 16]),
            ..request.key.clone()
        },
        operation_id: request.operation_id()?,
        request_digest: request.identity()?,
        expected_receipt_digest: stopped.receipt_digest,
        expected_finalization_revision: Some(stopped.finalization.as_ref().unwrap().revision),
        action: SplitRecoveryAction::Resume,
    };
    let ack = registry.catalog_resume_split_finalization(&action)?;
    ack.validate_for(&request, &stopped.receipt)?;
    let second = registry.catalog_note_split_failure(
        request.operation_id()?,
        stopped.receipt_digest,
        "later_terminal_failure",
    )?;
    assert_eq!(registry.catalog_resume_split_finalization(&action)?, ack);
    assert_eq!(
        registry
            .catalog_split_status(request.operation_id()?)?
            .unwrap(),
        second
    );
    let mut stale = action.clone();
    stale.key.nonce = Incarnation::from_bytes([71; 16]);
    assert!(registry.catalog_resume_split_finalization(&stale).is_err());
    let fresh = SplitRecoveryRequest {
        key: RequestKey {
            nonce: Incarnation::from_bytes([72; 16]),
            ..action.key.clone()
        },
        expected_finalization_revision: Some(second.finalization.as_ref().unwrap().revision),
        ..action.clone()
    };
    registry.catalog_resume_split_finalization(&fresh)?;
    let published = registry
        .catalog_advance_split_finalization(request.operation_id()?, stopped.receipt_digest)?;
    assert_eq!(
        published.receipt.state,
        SplitRunState::CanceledBeforeFence {
            cleanup_pending: false
        }
    );
    assert_eq!(
        published.finalization.as_ref().unwrap().progress,
        SplitFinalizationProgress::Acknowledging
    );
    assert_eq!(registry.catalog_pending_splits(8)?, vec![published.clone()]);
    let complete = registry
        .catalog_advance_split_finalization(request.operation_id()?, published.receipt_digest)?;
    assert_eq!(
        complete.finalization.as_ref().unwrap().progress,
        SplitFinalizationProgress::Complete
    );
    assert!(registry.catalog_pending_splits(8)?.is_empty());
    assert_eq!(
        registry.catalog_split_recovery_receipt(action.caller, &action.key)?,
        Some(ack)
    );
    assert_eq!(
        index_record(&registry.connection, request.original_index)?.policy,
        None
    );
    assert_eq!(
        canceled.receipt.state,
        SplitRunState::CanceledBeforeFence {
            cleanup_pending: true
        }
    );
    Ok(())
}

#[test]
fn real_claimed_cancellation_disposes_paid_shell_then_publishes_and_acknowledges_fixed_slots()
-> Result<()> {
    use crate::{
        quota::{
            Quota,
            funding::{FundingBudget, FundingPhase, FundingRequest},
        },
        service::collection::Collection,
    };
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let source = std::fs::read(Path::new(&request.original_registration.root).join("original.md"))?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    registry.catalog_reserve_split_destination(request.operation_id()?, accepted.receipt_digest)?;
    let state = registry.catalog_split_state(request.operation_id()?)?;
    let prepared = super::super::setup::storage::prepare_split_test_storage(&registry, &state)?;
    let mut current = prepared.record(&mut registry)?;
    crate::service::data_scope::bind(
        Path::new(&request.original_registration.data_root),
        &registry.root,
        request.key.namespace,
    )?;
    let funding_request = FundingRequest::for_split(
        &request,
        vec![
            FundingBudget {
                index_id: request.original_index,
                bytes: request.capacity.original_rewrite + request.capacity.cleanup_allowance / 2,
            },
            FundingBudget {
                index_id: request.destination.index_id,
                bytes: request.capacity.destination_growth
                    + request.capacity.destination_temporary
                    + request.capacity.cleanup_allowance
                    - request.capacity.cleanup_allowance / 2,
            },
        ],
    )?;
    for root in [
        &request.original_registration.data_root,
        &request.destination_registration.data_root,
    ] {
        let quota = Quota::open_service(Path::new(root))?;
        let receipt = quota.reserve_funding(&funding_request)?;
        current = registry.catalog_record_split_funding(
            request.operation_id()?,
            current.receipt_digest,
            &receipt,
        )?;
    }
    assert_eq!(current.receipt.stage, SplitStage::DestinationPrepared);
    assert!(registry.catalog_split_write_fenced(request.destination_registration.binding)?);
    assert!(!registry.catalog_split_write_fenced(request.original_registration.binding)?);
    assert!(
        registry
            .watch_projects(None, 32)?
            .iter()
            .all(|item| item.project.registration.binding
                != request.destination_registration.binding)
    );
    let collection = Collection::existing(
        Path::new(&request.destination_registration.data_root),
        &request.destination_registration.root,
        &request.destination_registration.collection,
    )?;
    let configuration = collection.split_work_configuration(
        &registry.catalog_split_state(request.operation_id()?)?,
        SplitJobPurpose::DestinationBuild,
    )?;
    let engine = Sha256Digest::from_bytes([80; 32]);
    let build_key = RequestKey {
        nonce: Incarnation::from_bytes([80; 16]),
        ..request.key.clone()
    };
    current = registry.enqueue_split_job(
        request.operation_id()?,
        current.receipt_digest,
        request.caller,
        &build_key,
        SplitJobPurpose::DestinationBuild,
        &ConfigurationWork {
            engine,
            request: configuration,
        },
    )?;
    let build = current.receipt.jobs.last().unwrap().job;
    let cancel = SplitRecoveryRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([81; 16]),
            ..request.key.clone()
        },
        operation_id: request.operation_id()?,
        request_digest: request.identity()?,
        expected_receipt_digest: current.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Cancel,
    };
    registry.catalog_cancel_split_action(&cancel)?;
    assert_eq!(
        registry.job(build)?.state,
        JobState::Canceled,
        "pending split work must become terminal without needing a forbidden claim"
    );
    current = registry
        .catalog_split_status(request.operation_id()?)?
        .unwrap();
    let configuration = collection.split_work_configuration(
        &registry.catalog_split_state(request.operation_id()?)?,
        SplitJobPurpose::DestinationCleanup,
    )?;
    let resume = SplitRecoveryRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([82; 16]),
            ..request.key.clone()
        },
        operation_id: request.operation_id()?,
        request_digest: request.identity()?,
        expected_receipt_digest: current.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Resume,
    };
    let ack = registry.catalog_resume_split_job_action(&resume, engine, &configuration)?;
    let SplitRecoveryOutcome::ResumeAccepted {
        accepted_job: job, ..
    } = ack.outcome
    else {
        anyhow::bail!("cleanup job required")
    };
    let mut lease = crate::service::ProjectLease::acquire(
        &registry.root,
        request.destination_registration.binding,
    )?;
    registry.admit_worker(&mut lease)?;
    registry
        .claim_job_id(&lease, job)?
        .context("native cleanup claim missing")?;
    let work = registry.claimed_split_work(&lease, job, engine)?;
    let quota = Quota::open_service(Path::new(&request.destination_registration.data_root))?;
    let funding = quota.observe_funding(request.operation_id()?)?;
    let phase = FundingPhase {
        revision: work.snapshot.receipt.revision.get(),
        purpose: SplitJobPurpose::DestinationCleanup,
        job,
    };
    let funding_use =
        quota.begin_funding_phase(&funding, request.destination.index_id, phase.clone())?;
    collection.retain_split_state(
        &NativeSplitState(work.snapshot.clone()),
        &quota.clone().with_funding(funding_use.clone())?,
    )?;
    let store = collection
        .clone()
        .with_split_access(work.permit.clone())?
        .selected_store()?
        .with_operation_funding(funding_use.clone())?;
    let writer = store.lock()?;
    let fresh = quota.observe_funding(request.operation_id()?)?;
    let mut wrong = phase.clone();
    wrong.job = Sha256Digest::from_bytes([83; 32]);
    assert!(
        store
            .takeover_operation_funding_for_cleanup(&writer, &quota, &fresh, wrong)
            .is_err()
    );
    assert_eq!(quota.observe_funding(request.operation_id()?)?, fresh);
    store.reconcile_operation_funding_without_current(&writer, &Quota::for_store(&store)?)?;
    drop(writer);
    drop(store);
    let mut proof = None;
    for _ in 0..4 {
        if let Some(result) =
            registry.cleanup_split_destination(&lease, job, engine, Some(&funding_use))?
        {
            proof = Some(result);
            break;
        }
    }
    registry.record_split_job_result(
        &lease,
        job,
        proof.context("native bounded disposal did not finish")?,
    )?;
    registry.complete_job(
        &lease,
        job,
        airk_core::service::JobOutcome::Succeeded { revision: None },
    )?;
    current = registry
        .catalog_split_status(request.operation_id()?)?
        .unwrap();
    current = registry.catalog_complete_split_job(
        request.operation_id()?,
        current.receipt_digest,
        job,
    )?;
    assert!(current.receipt.physical_cleanup_authorization.is_some());
    let mut saw_slot = false;
    let mut saw_ack = false;
    for _ in 0..12 {
        current = registry
            .catalog_advance_split_finalization(request.operation_id()?, current.receipt_digest)?;
        saw_slot |= current
            .receipt
            .terminal_publication_slots
            .contains_key(&request.destination.index_id);
        if current
            .finalization
            .as_ref()
            .is_some_and(|state| state.progress == SplitFinalizationProgress::Acknowledging)
        {
            saw_ack = true;
            assert!(registry.catalog_split_write_fenced(request.destination_registration.binding)?);
            assert!(
                registry
                    .catalog_active_split_for_binding(request.destination_registration.binding)?
                    .is_some()
            );
        }
        if current
            .finalization
            .as_ref()
            .is_some_and(|state| state.progress == SplitFinalizationProgress::Complete)
        {
            break;
        }
    }
    assert!(saw_slot && saw_ack);
    assert_eq!(
        current.finalization.as_ref().unwrap().progress,
        SplitFinalizationProgress::Complete
    );
    assert!(!registry.catalog_split_write_fenced(request.destination_registration.binding)?);
    assert!(
        registry
            .catalog_active_split_for_binding(request.destination_registration.binding)?
            .is_none()
    );
    let directory = Path::new(&request.destination_registration.data_root)
        .join("collections-1.4")
        .join(&request.destination_registration.collection);
    assert!(!directory.join("incarnations").exists());
    assert!(directory.join("service-owner.json").exists());
    assert!(
        !std::fs::read_dir(&directory)?.any(|entry| entry.is_ok_and(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with(".split-final-")))
    );
    assert_eq!(
        std::fs::read(Path::new(&request.original_registration.root).join("original.md"))?,
        source
    );
    Ok(())
}

#[test]
fn final_fence_configuration_rechecks_real_metadata_after_worker_drain_without_creating_current()
-> Result<()> {
    use crate::{
        quota::Quota,
        service::{
            ProjectLease,
            collection::{Collection, inspect_settings_digest},
        },
    };
    let (temporary, registry, mut request, _before, _plan) = fixture()?;
    let registration = &request.original_registration;
    crate::service::data_scope::bind(
        Path::new(&registration.data_root),
        &registry.root,
        request.key.namespace,
    )?;
    let collection = Collection::initialize(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
        request.original_incarnation,
    )?;
    let store = collection.select_for_diagnostics()?;
    let marker = store.lock()?;
    drop(marker);
    request.original_settings_digest = inspect_settings_digest(&store, &registration.root)?;
    let old_worker = ProjectLease::acquire(&registry.root, registration.binding)?;
    assert!(
        ProjectLease::acquire(&registry.root, registration.binding).is_err(),
        "undrained native worker must retain exclusive project authority"
    );
    drop(old_worker);
    let fenced = ProjectLease::acquire(&registry.root, registration.binding)?;
    verify_fenced_original_configuration(&store, &request, &fenced)?;
    drop(fenced);
    let writer = store.lock()?;
    let quota = Quota::for_store(&store)?;
    store.configure_extraction_policy_bytes(
        &writer,
        &quota,
        &registration.root,
        b"{\"detail\":\"full\"}",
    )?;
    drop(writer);
    let fenced = ProjectLease::acquire(&registry.root, registration.binding)?;
    assert!(verify_fenced_original_configuration(&store, &request, &fenced).is_err());
    drop(fenced);
    request.original_settings_digest = inspect_settings_digest(&store, &registration.root)?;
    let extra = temporary.path().join("extra-source");
    std::fs::create_dir(&extra)?;
    let writer = store.lock()?;
    let mut roots = crate::roots::RootConfig::load(store.directory(), &registration.root)?;
    roots.add(
        "extra",
        &extra,
        Path::new(&registration.root),
        Path::new(&registration.data_root),
    )?;
    roots.save(store.directory())?;
    drop(writer);
    request.original_settings_digest = inspect_settings_digest(&store, &registration.root)?;
    let fenced = ProjectLease::acquire(&registry.root, registration.binding)?;
    assert!(
        verify_fenced_original_configuration(&store, &request, &fenced).is_err(),
        "matching changed settings must not hide a changed root roster"
    );
    assert!(!store.directory().join("current.sqlite3").exists());
    assert_eq!(
        std::fs::read(Path::new(&registration.root).join("original.md"))?,
        b"original remainder"
    );
    Ok(())
}

#[test]
fn pending_native_split_fence_holds_same_canonical_job_and_watch_barrier_before_sql_projection()
-> Result<()> {
    use airk_core::catalog_fences::{FenceIntent, FencePlan, FenceStage, WorkerDrainObligation};
    let (_temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    assert!(!registry.catalog_split_write_fenced(request.original_registration.binding)?);
    assert!(
        registry
            .watch_projects(None, 32)?
            .iter()
            .any(|item| item.project.registration.binding == request.original_registration.binding)
    );
    let inspect_key = RequestKey {
        nonce: Incarnation::from_bytes([90; 16]),
        ..request.key.clone()
    };
    let inspect = registry.enqueue(
        request.original_registration.binding,
        request.caller,
        &inspect_key,
        JobKind::Inspect,
    )?;
    assert_eq!(
        registry
            .next_pending_job(request.original_registration.binding)?
            .unwrap()
            .id,
        inspect.id
    );
    let key = RequestKey {
        nonce: Incarnation::from_bytes([91; 16]),
        ..request.key.clone()
    };
    let operation = airk_core::catalog_management::catalog_operation_id(request.caller, &key)?;
    let change = CatalogRequest {
        caller: request.caller,
        key,
        expected_catalog_revision: accepted.receipt.catalog_revision,
        operation: CatalogOperation::ChangeIndexPolicy {
            id: request.original_index,
            expected_source_revision: request.expected_source_revision,
            expected_policy_epoch: 0,
            expected_policy_digest: None,
            change: PolicyChange::SplitRemainder {
                split_operation: accepted.receipt.operation_id,
                expected_split_receipt: accepted.receipt_digest,
            },
            engine_identity: Sha256Digest::from_bytes([92; 32]),
            configuration: Box::new(ConfigurationRequest {
                prepare: false,
                incarnation: request.original_incarnation,
                settings: request.original_settings_digest,
                configuration: ProjectConfiguration::ApplySourcePolicy {
                    policy: Box::new(plan.remainder.clone()),
                    operation_id: operation,
                },
            }),
        },
    };
    let fence_plan = FencePlan::new(Incarnation::from_bytes([93; 16]), 1, vec![])?
        .with_worker_obligation(WorkerDrainObligation {
            index_id: request.original_index,
            backing: request.original_registration.binding,
            workers: vec![],
            providers: vec![],
        })?;
    let mut intent = FenceIntent {
        schema: 1,
        operation_id: operation,
        request: change,
        preview_digest: Sha256Digest::from_bytes([94; 32]),
        prepared_digest: Sha256Digest::from_bytes([0; 32]),
        plan: fence_plan,
        stage: FenceStage::Prepared,
        confirmations: vec![],
        worker_confirmations: vec![],
    };
    intent.prepared_digest = intent.identity()?;
    intent.validate()?;
    // This isolated fixture projects the actual typed durable fence envelope;
    // OS drain/publication qualification is covered by separate native tests.
    registry.connection.execute("INSERT INTO index_operation_receipts VALUES(?1,'catalog_management_fenced','prepared',1,?2)",params![operation.to_string(),serde_json::to_vec(&intent)?])?;
    assert_eq!(
        registry
            .catalog_split_status(request.operation_id()?)?
            .unwrap()
            .receipt
            .stage,
        SplitStage::Accepted
    );
    assert!(
        registry.catalog_split_write_fenced(request.original_registration.binding)?,
        "pending native cutover precedes split SQL projection"
    );
    assert!(
        registry
            .next_pending_job(request.original_registration.binding)?
            .is_none()
    );
    assert!(
        registry
            .watch_projects(None, 32)?
            .iter()
            .all(|item| item.project.registration.binding != request.original_registration.binding)
    );
    // A completed fence row stops being the pending override; the actual split
    // phase then keeps the same barrier. No read authority is granted here.
    let tx = registry.connection.transaction()?;
    let old = exact(&tx, request.operation_id()?, accepted.receipt_digest)?;
    let mut next = old.receipt.clone();
    next.stage = SplitStage::OriginalFenced;
    next.funding_receipts.insert(
        request.original_registration.data_root.clone(),
        Sha256Digest::from_bytes([95; 32]),
    );
    next.funding_receipts.insert(
        request.destination_registration.data_root.clone(),
        Sha256Digest::from_bytes([96; 32]),
    );
    next.capacity_receipt = Some(Sha256Digest::from_bytes([97; 32]));
    next.destination_storage = Some(Sha256Digest::from_bytes([98; 32]));
    let token = airk_core::identity::RevisionToken {
        incarnation: request.original_incarnation,
        generation: 1,
        revision: Sha256Digest::from_bytes([99; 32]),
    };
    next.destination_publication = Some(token);
    next.destination_validation = Some(Sha256Digest::from_bytes([100; 32]));
    next.original_fence = Some(Sha256Digest::from_bytes([101; 32]));
    next.remainder_witness = Some(SplitRemainderWitness {
        job: None,
        index_id: request.original_index,
        before_policy_digest: request.before_policy_digest,
        publication: token,
        source_root_id: before.roots[0].id,
        logical_path: "original.md".into(),
        source_relative_path: "original.md".into(),
        expected_hash: Sha256Digest::from_bytes([102; 32]),
        observed_from_unix_ms: 1,
        observed_to_unix_ms: 1,
        bytes_read: 0,
    });
    replace(&tx, &old, next)?;
    tx.execute(
        "UPDATE index_operation_receipts SET stage='metadata_applied' WHERE id=?1",
        [operation.to_string()],
    )?;
    tx.commit()?;
    assert!(registry.catalog_split_write_fenced(request.original_registration.binding)?);
    assert!(
        registry
            .next_pending_job(request.original_registration.binding)?
            .is_none()
    );
    assert_eq!(
        registry.job(inspect.id)?.state,
        JobState::Pending,
        "barrier preserves already accepted unrelated work"
    );
    Ok(())
}

#[test]
fn split_remainder_preview_rolls_back_without_claiming_native_fence_and_direct_commit_refuses()
-> Result<()> {
    let (temp, mut registry, request, before, plan) = fixture()?;
    let accepted = registry.catalog_reserve_split(&request, &before, &plan)?;
    // Metadata-only projection for preview/refusal testing. These typed values
    // never qualify real storage, a publication, a source witness, or a drain.
    let tx = registry.connection.transaction()?;
    let mut next = accepted.receipt.clone();
    next.stage = SplitStage::DestinationValidated;
    for (root, byte) in [
        (&request.original_registration.data_root, 111),
        (&request.destination_registration.data_root, 112),
    ] {
        next.funding_receipts
            .insert(root.clone(), Sha256Digest::from_bytes([byte; 32]));
    }
    next.capacity_receipt = Some(Sha256Digest::from_bytes([113; 32]));
    next.destination_storage = Some(Sha256Digest::from_bytes([114; 32]));
    next.destination_publication = Some(airk_core::identity::RevisionToken {
        incarnation: Incarnation::from_bytes([115; 16]),
        generation: 1,
        revision: Sha256Digest::from_bytes([116; 32]),
    });
    next.destination_validation = Some(Sha256Digest::from_bytes([117; 32]));
    let validated = replace(&tx, &accepted, next)?;
    tx.commit()?;
    let key = RequestKey {
        nonce: Incarnation::from_bytes([118; 16]),
        ..request.key.clone()
    };
    let operation = airk_core::catalog_management::catalog_operation_id(request.caller, &key)?;
    let change = CatalogRequest {
        caller: request.caller,
        key,
        expected_catalog_revision: validated.receipt.catalog_revision,
        operation: CatalogOperation::ChangeIndexPolicy {
            id: request.original_index,
            expected_source_revision: request.expected_source_revision,
            expected_policy_epoch: 0,
            expected_policy_digest: None,
            change: PolicyChange::SplitRemainder {
                split_operation: validated.receipt.operation_id,
                expected_split_receipt: validated.receipt_digest,
            },
            engine_identity: Sha256Digest::from_bytes([119; 32]),
            configuration: Box::new(ConfigurationRequest {
                prepare: false,
                incarnation: request.original_incarnation,
                settings: request.original_settings_digest,
                configuration: ProjectConfiguration::ApplySourcePolicy {
                    policy: Box::new(plan.remainder.clone()),
                    operation_id: operation,
                },
            }),
        },
    };
    let state_before = registry.catalog_state()?;
    let source_before = std::fs::read(temp.path().join("source/original.md"))?;
    let counts = |registry: &ServiceRegistry| -> Result<Vec<i64>> {
        [
            "service_jobs",
            "service_requests",
            "index_operation_receipts",
            "index_split_jobs",
            "index_split_history",
        ]
        .iter()
        .map(|table| {
            Ok(registry.connection.query_row(
                &format!("SELECT count(*) FROM {table}"),
                [],
                |row| row.get(0),
            )?)
        })
        .collect()
    };
    let counts_before = counts(&registry)?;
    let watch = registry.watch(request.original_registration.binding)?;
    let watch_before = (
        watch.configuration_epoch,
        watch.additional_roots,
        serde_json::to_value(watch.observation)?,
    );
    let preview = registry.catalog_preview(&change)?;
    assert!(preview.authority_fence_required && preview.file_data_changes);
    assert_eq!(preview.operation, change.operation);
    let remainder_digest = plan.remainder.identity()?;
    assert!(
        matches!(preview.after.as_slice(), [airk_core::catalog_management::CatalogItem::Index(index)]
        if index.policy.as_ref().is_some_and(|policy| policy.digest == remainder_digest))
    );
    assert_eq!(registry.catalog_state()?, state_before);
    assert_eq!(
        registry.catalog_split_status(request.operation_id()?)?,
        Some(validated.clone())
    );
    assert_eq!(counts(&registry)?, counts_before);
    let watch = registry.watch(request.original_registration.binding)?;
    assert_eq!(
        (
            watch.configuration_epoch,
            watch.additional_roots,
            serde_json::to_value(watch.observation)?
        ),
        watch_before
    );
    assert!(
        registry
            .catalog_source_policy(request.original_index)?
            .is_none()
    );
    assert!(registry.catalog_pending_fence()?.is_none());
    assert!(!registry.catalog_split_write_fenced(request.original_registration.binding)?);
    assert_eq!(registry.catalog_preview(&change)?, preview);
    assert!(
        registry
            .catalog_apply(&change, preview.identity()?)
            .is_err()
    );
    let runtime = registry.root.clone();
    let tx = registry.connection.transaction()?;
    assert!(super::super::operations::mutate(&tx, &change, &runtime).is_err());
    tx.rollback()?;
    assert!(
        registry
            .catalog_apply_fenced(
                &change,
                preview.identity()?,
                Sha256Digest::from_bytes([120; 32])
            )
            .is_err()
    );
    assert_eq!(
        registry.catalog_split_status(request.operation_id()?)?,
        Some(validated)
    );
    assert_eq!(registry.catalog_state()?, state_before);
    assert_eq!(counts(&registry)?, counts_before);
    assert!(registry.catalog_pending_fence()?.is_none());
    assert_eq!(
        std::fs::read(temp.path().join("source/original.md"))?,
        source_before
    );
    Ok(())
}
