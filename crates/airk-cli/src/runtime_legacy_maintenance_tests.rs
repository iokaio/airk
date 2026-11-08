// SPDX-License-Identifier: Apache-2.0
//! Production decision regressions; actual process qualification is separate.
use super::*;
use airk_core::{
    catalog_legacy_adoption::source_inventory_digest,
    catalog_setup::{IndexSetupRequest, SetupSource},
    index_catalog::CatalogRevision,
    service::{ProjectRegistration, RequestKey},
};

#[test]
fn manager_changes_preserve_every_pending_maintenance_phase_and_unknown_journal() -> Result<()> {
    let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
    local_ipc::prepare_root(&root.join("control"))?;
    require_manager_change_settled(&root)?;
    let operation = Incarnation::from_bytes([71; 16]);
    let mut journal = Journal {
        schema: 1,
        install: root.clone(),
        control: layout::control(&root)?,
        operation,
        selected: Selection {
            schema: 1,
            active: identity,
            previous: None,
        },
        execution_sha256: Sha256Digest::from_bytes([72; 32]),
        original_running: false,
        original_explicit_user_stop: true,
        original_recorded_instance: None,
        original_health: None,
        cycle: 0,
        cycle_health: None,
        cycle_recorded_instance: None,
        checkpoint_operation: checkpoint_operation(operation, 0, None),
        cycle_state_sha256: None,
        manager_owned: false,
        manager_definition_sha256: Sha256Digest::from_bytes([73; 32]),
        phase: Phase::Admitted,
        checkpoint: None,
        boundary: None,
        quiescent_state_sha256: None,
        adoption_plans: vec![],
        adoption_plan: None,
        restoration_state_sha256: None,
        restoration: None,
        restore_failure: None,
    };
    for phase in [
        Phase::Admitted,
        Phase::Checkpointed,
        Phase::Quiescent,
        Phase::Restoring,
        Phase::RestoreFailed,
    ] {
        journal.phase = phase;
        save(&journal)?;
        let before = fs::read(root.join(JOURNAL))?;
        assert!(require_manager_change_settled(&root).is_err());
        assert_eq!(fs::read(root.join(JOURNAL))?, before);
    }
    journal.phase = Phase::Restored;
    save(&journal)?;
    require_manager_change_settled(&root)?;
    local_ipc::prepare_root(&journal.control)?;
    let snapshot = store::snapshot(&journal.control)?;
    let name = restoration_snapshot_name(journal.operation, journal.cycle);
    assert_eq!(
        name,
        format!(
            "legacy-maintenance-restoration-{}-0.json",
            journal.operation
        )
    );
    assert_ne!(name, restoration_snapshot_name(journal.operation, 1));
    save_snapshot(&journal, &name, &snapshot)?;
    let saved = fs::read(root.join(&name))?;
    assert_eq!(
        snapshot.digest()?,
        framed_digest(b"airk/checkpoint-native-state/1", &[&saved])
    );
    save_snapshot(&journal, &name, &snapshot)?;
    let mut changed = snapshot.clone();
    changed.complete = false;
    assert!(save_snapshot(&journal, &name, &changed).is_err());
    assert_eq!(
        fs::read(root.join(&name))?,
        saved,
        "crossed snapshot must remain untouched"
    );
    let mut oversized = snapshot.clone();
    oversized.project_bindings = vec![adoption_plan(3)?.setup.registration.binding; 20_000];
    assert!(save_snapshot(&journal, "oversized-restoration-fixture.json", &oversized).is_err());
    assert!(!root.join("oversized-restoration-fixture.json").exists());
    // Earlier journals recorded the hash without a file; read compatibility
    // remains unchanged. New observers require the file before claiming facts.
    journal.restoration_state_sha256 = Some(snapshot.digest()?);
    save(&journal)?;
    fs::remove_file(root.join(&name))?;
    assert_eq!(
        read(&root)?.unwrap().restoration_state_sha256,
        journal.restoration_state_sha256
    );
    fs::write(root.join(JOURNAL), b"{unknown")?;
    assert!(require_manager_change_settled(&root).is_err());
    assert_eq!(fs::read(root.join(JOURNAL))?, b"{unknown");
    #[cfg(unix)]
    {
        fs::remove_file(root.join(JOURNAL))?;
        std::os::unix::fs::symlink(root.join("missing-original-journal"), root.join(JOURNAL))?;
        assert!(require_manager_change_settled(&root).is_err());
    }
    Ok(())
}

fn adoption_plan(group: u8) -> Result<LegacyAdoptionPlan> {
    let root = format!("/source-{group}");
    let old = ProjectRegistration::new(root.clone(), format!("/old-{group}"), "saved".into())?;
    let new = ProjectRegistration::new(root.clone(), format!("/new-{group}"), "saved".into())?;
    let digest = Sha256Digest::from_bytes([13; 32]);
    let plan = LegacyAdoptionPlan {
        schema: 1,
        setup: IndexSetupRequest {
            caller: Incarnation::from_bytes([14; 16]),
            key: RequestKey {
                namespace: Incarnation::from_bytes([15; 16]),
                epoch: 0,
                nonce: Incarnation::from_bytes([group; 16]),
            },
            expected_catalog_revision: CatalogRevision::INITIAL,
            name: format!("Saved group {group}"),
            registration: new,
            sources: vec![SetupSource {
                canonical_path: root.clone(),
                aliases: vec![],
                owner: true,
            }],
            setup_watch: false,
        },
        original_registration: old,
        origin_sha256: digest,
        config_sha256: digest,
        executable_sha256: digest,
        quiescence_sha256: digest,
        archive_inventory_sha256: digest,
        original_publication_sha256: digest,
        source_inventory_sha256: source_inventory_digest(&root, "saved", &BTreeMap::new())?,
    };
    plan.validate()?;
    Ok(plan)
}

#[test]
fn two_native_groups_share_original_quiescence_but_capture_each_current_catalog_revision()
-> Result<()> {
    let first = adoption_plan(20)?;
    let mut second = adoption_plan(21)?;
    require_plan_context(
        &first,
        Some(first.setup.key.namespace),
        CatalogRevision::INITIAL,
        first.quiescence_sha256,
    )?;
    second.setup.expected_catalog_revision = CatalogRevision::INITIAL.next()?;
    require_plan_context(
        &second,
        Some(first.setup.key.namespace),
        second.setup.expected_catalog_revision,
        first.quiescence_sha256,
    )?;
    assert!(!plan_already_bound(std::slice::from_ref(&first), &second)?);
    assert_eq!(first.quiescence_sha256, second.quiescence_sha256);
    assert!(
        require_plan_context(
            &second,
            Some(first.setup.key.namespace),
            CatalogRevision::INITIAL,
            first.quiescence_sha256
        )
        .is_err()
    );
    assert!(
        require_plan_context(
            &second,
            Some(Incarnation::from_bytes([16; 16])),
            second.setup.expected_catalog_revision,
            first.quiescence_sha256
        )
        .is_err()
    );
    assert!(
        require_plan_context(
            &second,
            Some(first.setup.key.namespace),
            second.setup.expected_catalog_revision,
            Sha256Digest::from_bytes([17; 32])
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn batched_group_retry_is_exact_and_crossed_request_or_backing_never_adds_a_plan() -> Result<()> {
    let first = adoption_plan(22)?;
    let plans = vec![first.clone()];
    assert!(plan_already_bound(&plans, &first)?);
    let mut changed = first.clone();
    changed.config_sha256 = Sha256Digest::from_bytes([18; 32]);
    assert!(plan_already_bound(&plans, &changed).is_err());
    let mut crossed = adoption_plan(23)?;
    crossed.setup.key = first.setup.key.clone();
    assert!(plan_already_bound(&plans, &crossed).is_err());
    let mut duplicate_backing = first.clone();
    duplicate_backing.setup.key.nonce = Incarnation::from_bytes([24; 16]);
    duplicate_backing.original_registration = ProjectRegistration::new(
        first.original_registration.root.clone(),
        "/different-old".into(),
        "saved".into(),
    )?;
    assert!(plan_already_bound(&plans, &duplicate_backing).is_err());
    let mut duplicate_origin = first.clone();
    duplicate_origin.setup.key.nonce = Incarnation::from_bytes([25; 16]);
    duplicate_origin.setup.registration = ProjectRegistration::new(
        first.original_registration.root.clone(),
        "/different-new".into(),
        "saved".into(),
    )?;
    assert!(plan_already_bound(&plans, &duplicate_origin).is_err());
    let mut crossed_root = adoption_plan(26)?;
    crossed_root.setup.registration =
        ProjectRegistration::new("/unrelated-root".into(), "/new-26".into(), "saved".into())?;
    assert!(plan_already_bound(&plans, &crossed_root).is_err());
    assert_eq!(plans, vec![first]);
    Ok(())
}

#[test]
fn bounded_plan_roster_keeps_exact_retry_and_never_discards_an_existing_group() -> Result<()> {
    let mut plans = Vec::new();
    for group in 1..=u8::try_from(MAX_ADOPTION_PLANS)? {
        let plan = adoption_plan(group)?;
        assert!(!plan_already_bound(&plans, &plan)?);
        plans.push(plan);
    }
    assert!(plan_already_bound(&plans, &plans[0])?);
    assert!(plan_already_bound(&plans, &adoption_plan(33)?).is_err());
    assert_eq!(plans.len(), MAX_ADOPTION_PLANS);
    assert_eq!(plans[0], adoption_plan(1)?);
    Ok(())
}

#[test]
fn bound_plans_without_actual_store_receipts_never_admit_introduced_native_indexes() -> Result<()> {
    for plan in [adoption_plan(27)?, adoption_plan(28)?] {
        assert!(checked_native_receipt(&plan, None)?.is_none());
    }
    Ok(())
}

fn continuation_receipt() -> Result<airk_core::catalog_setup::IndexSetupReceipt> {
    use airk_core::{catalog_management::catalog_operation_id, catalog_setup::*};
    let plan = adoption_plan(29)?;
    let request = plan.setup.clone();
    let revision = request.expected_catalog_revision.next()?;
    let marker = plan.identity()?;
    let digest = Sha256Digest::from_bytes([65; 32]);
    let receipt = IndexSetupReceipt {
        schema: 1,
        operation_id: catalog_operation_id(request.caller, &request.key)?,
        request_digest: request.identity()?,
        index_id: setup_index_id(&request)?,
        source_ids: setup_source_ids(&request)?,
        request,
        stage: SetupStage::Prepared,
        catalog_revision: revision,
        source_revision: CatalogRevision::INITIAL,
        preparation: Some(PreparationEvidence {
            digest,
            storage_identity: digest,
            source_configuration_digest: digest,
            review: SetupPreparationReview {
                expected_catalog_revision: revision,
                expected_source_revision: CatalogRevision::INITIAL,
            },
            legacy_adoption: Some(marker),
        }),
        completion: None,
        legacy_adoption: Some(marker),
    };
    receipt.validate()?;
    Ok(receipt)
}

#[test]
fn exact_planned_setup_continuations_preserve_foreign_work_and_native_stage_changes() -> Result<()>
{
    let prepared = continuation_receipt()?;
    let current = setup_checkpoint_work(&prepared)?.unwrap();
    let reserved =
        airk_store::service::registry::catalog::setup::legacy_adoption_checkpoint_history(
            &prepared,
        )?
        .remove(0);
    let prior = setup_checkpoint_work(&reserved)?.unwrap();
    let foreign = store::UnfinishedWork {
        owner: "service_jobs".into(),
        id: "prior-job".into(),
        kind: "Index".into(),
        state: "Pending".into(),
        original_engine: None,
        payload_digest: None,
    };
    require_setup_continuations(
        std::slice::from_ref(&foreign),
        &[foreign.clone(), current.clone()],
        &[&prepared],
    )?;
    require_setup_continuations(
        &[foreign.clone(), prior],
        &[foreign.clone(), current.clone()],
        &[&prepared],
    )?;
    require_setup_continuations(
        &[foreign.clone(), current.clone()],
        &[foreign.clone(), current.clone()],
        &[&prepared],
    )?;
    let mut changed = foreign.clone();
    changed.state = "Running".into();
    assert!(
        require_setup_continuations(
            std::slice::from_ref(&foreign),
            &[changed, current.clone()],
            &[&prepared]
        )
        .is_err()
    );
    assert!(
        require_setup_continuations(
            std::slice::from_ref(&foreign),
            std::slice::from_ref(&current),
            &[&prepared]
        )
        .is_err()
    );
    assert!(require_setup_continuations(&[], &[foreign, current.clone()], &[&prepared]).is_err());
    assert!(require_setup_continuations(&[], &[], &[&prepared]).is_err());
    assert!(require_setup_continuations(&[], &[current.clone(), current], &[&prepared]).is_err());
    Ok(())
}

#[test]
fn planned_setup_before_digest_and_complete_current_receipt_are_not_replaceable() -> Result<()> {
    let prepared = continuation_receipt()?;
    let current = setup_checkpoint_work(&prepared)?.unwrap();
    for changed in [
        store::UnfinishedWork {
            payload_digest: Some(Sha256Digest::from_bytes([0; 32])),
            ..current.clone()
        },
        store::UnfinishedWork {
            kind: "other-owner-operation".into(),
            ..current.clone()
        },
        store::UnfinishedWork {
            original_engine: Some(Sha256Digest::from_bytes([0; 32])),
            ..current.clone()
        },
    ] {
        assert!(
            require_setup_continuations(
                std::slice::from_ref(&changed),
                std::slice::from_ref(&current),
                &[&prepared]
            )
            .is_err()
        );
        assert!(
            require_setup_continuations(&[], std::slice::from_ref(&changed), &[&prepared]).is_err()
        );
    }
    let mut crossed = prepared.clone();
    crossed.request_digest = Sha256Digest::from_bytes([0; 32]);
    assert!(require_setup_continuations(&[], std::slice::from_ref(&current), &[&crossed]).is_err());
    let mut ordinary = prepared;
    ordinary.legacy_adoption = None;
    ordinary.preparation.as_mut().unwrap().legacy_adoption = None;
    assert!(
        require_setup_continuations(&[], std::slice::from_ref(&current), &[&ordinary]).is_err()
    );
    Ok(())
}

#[test]
fn ready_planned_setup_consumes_only_its_exact_prepared_continuation() -> Result<()> {
    use airk_core::catalog_setup::{SetupCompletion, SetupStage};
    let prepared = continuation_receipt()?;
    let prior = setup_checkpoint_work(&prepared)?.unwrap();
    let digest = Sha256Digest::from_bytes([66; 32]);
    let mut ready = prepared.clone();
    ready.stage = SetupStage::Ready;
    ready.completion = Some(SetupCompletion {
        prepared_digest: prepared.preparation.as_ref().unwrap().digest,
        expected_catalog_revision: prepared.catalog_revision,
        expected_source_revision: prepared.source_revision,
        legacy_adoption: Some(
            airk_core::catalog_legacy_adoption::LegacyAdoptionPublication {
                schema: 1,
                plan_digest: prepared.legacy_adoption.unwrap(),
                archive_inventory_sha256: digest,
                original_publication_sha256: digest,
                current: airk_core::identity::RevisionToken {
                    incarnation: Incarnation::from_bytes([67; 16]),
                    generation: 1,
                    revision: digest,
                },
                resource_sha256: digest,
                compatibility_sha256: digest,
            },
        ),
    });
    ready.catalog_revision = prepared.catalog_revision.next()?;
    ready.source_revision = prepared.source_revision.next()?;
    ready.validate()?;
    require_setup_continuations(std::slice::from_ref(&prior), &[], &[&ready])?;
    assert!(
        require_setup_continuations(
            std::slice::from_ref(&prior),
            std::slice::from_ref(&prior),
            &[&ready]
        )
        .is_err()
    );
    let changed = store::UnfinishedWork {
        payload_digest: Some(digest),
        ..prior
    };
    assert!(require_setup_continuations(std::slice::from_ref(&changed), &[], &[&ready]).is_err());
    Ok(())
}

#[test]
fn restoration_phase_preserves_user_stop_and_idle_policy() {
    for phase in [
        Phase::Admitted,
        Phase::Checkpointed,
        Phase::Quiescent,
        Phase::Restored,
    ] {
        assert!(!restoration_needs_reconcile(phase, true, false));
    }
    for phase in [Phase::Restoring, Phase::RestoreFailed] {
        assert!(restoration_needs_reconcile(phase, true, false));
        assert!(!restoration_needs_reconcile(phase, false, false));
        assert!(!restoration_needs_reconcile(phase, true, true));
    }
    assert!(restart_allowed(true, false));
    assert!(!restart_allowed(false, false));
    assert!(!restart_allowed(true, true));
    assert!(!restart_allowed(false, true));
}

#[test]
fn either_user_stop_change_refuses_restoration() -> Result<()> {
    unchanged_user_stop(false, false)?;
    unchanged_user_stop(true, true)?;
    assert!(unchanged_user_stop(false, true).is_err());
    assert!(unchanged_user_stop(true, false).is_err());
    Ok(())
}

#[test]
fn checkpoint_cycles_preserve_parent_operation_and_exact_native_owner() {
    let operation = Incarnation::from_bytes([1; 16]);
    let original = Some(Incarnation::from_bytes([2; 16]));
    let replacement = Some(Incarnation::from_bytes([3; 16]));
    let first = checkpoint_operation(operation, 0, original);
    assert_eq!(first, checkpoint_operation(operation, 0, original));
    assert_ne!(first, checkpoint_operation(operation, 1, original));
    assert_ne!(first, checkpoint_operation(operation, 0, replacement));
    assert_ne!(
        first,
        checkpoint_operation(Incarnation::from_bytes([4; 16]), 0, original)
    );
}

#[test]
fn pending_retry_requires_original_operation_and_restored_operation_can_reenter() -> Result<()> {
    let original = Incarnation::from_bytes([5; 16]);
    let other = Incarnation::from_bytes([6; 16]);
    for phase in [
        Phase::Admitted,
        Phase::Checkpointed,
        Phase::Quiescent,
        Phase::Restoring,
        Phase::RestoreFailed,
    ] {
        require_pending_operation(phase, original, original)?;
        assert!(require_pending_operation(phase, original, other).is_err());
    }
    require_pending_operation(Phase::Restored, original, original)?;
    require_pending_operation(Phase::Restored, original, other)?;
    Ok(())
}

#[test]
fn restored_successor_requires_preservation_settlement_and_actual_recorded_owner() -> Result<()> {
    let original = Incarnation::from_bytes([7; 16]);
    let successor = Incarnation::from_bytes([8; 16]);
    assert!(!restored_owner_is_fresh(
        Some(original),
        original,
        Some(original),
        false,
        true
    )?);
    assert!(restored_owner_is_fresh(
        Some(original),
        successor,
        Some(successor),
        true,
        false
    )?);
    assert!(
        restored_owner_is_fresh(Some(original), successor, Some(successor), false, false).is_err()
    );
    assert!(
        restored_owner_is_fresh(Some(original), successor, Some(successor), true, true).is_err()
    );
    assert!(
        restored_owner_is_fresh(Some(original), successor, Some(original), true, false).is_err()
    );
    Ok(())
}

#[test]
fn unavailable_successor_dto_keeps_operation_and_explicit_recovery_guidance() -> Result<()> {
    let error = RestoreNeedsNativeRecovery {
        code: "maintenance_restored_successor_health_unavailable",
        operation: Incarnation::from_bytes([9; 16]),
        checkpoint_owner: Some(Incarnation::from_bytes([10; 16])),
        recorded_successor: Some(Incarnation::from_bytes([11; 16])),
        guidance: "Retain the exact journal; unavailable health does not prove exit.",
    };
    let dto = serde_json::to_value(&error)?;
    assert_eq!(dto["operation"], error.operation.to_string());
    assert_eq!(
        dto["code"],
        "maintenance_restored_successor_health_unavailable"
    );
    assert!(error.to_string().contains("does not prove exit"));
    Ok(())
}
