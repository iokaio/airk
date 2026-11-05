// SPDX-License-Identifier: Apache-2.0
//! Isolated metadata/failure fixtures. Fake worker census here exercises Store
//! invariants only; actual native admission is covered by process integrations.
use super::*;
use airk_core::{
    catalog_fences::{FencePlan, FencePreparation, WorkerDrainObligation},
    catalog_management::CatalogRequest,
    identity::RevisionToken,
    index_catalog::{CatalogRevision, DenyId, PathScope, ScopeAtom, SourceRootId},
    service::{ProjectRegistration, RequestKey},
    service_configuration::ConfigurationRequest,
};

fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, CatalogRequest)> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    let source = base.join("source");
    std::fs::create_dir(&source)?;
    std::fs::create_dir(base.join("control"))?;
    std::fs::write(source.join("private.rs"), "pub struct Private;\n")?;
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        base.join("data").to_str().unwrap().into(),
        "policy-unit".into(),
    )?;
    let registry = ServiceRegistry::open(&base.join("control"), Incarnation::from_bytes([1; 16]))?;
    let mut registry = registry;
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
    registry.connection.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,'Policy fixture',1,1,0,NULL,0,'complete')",params![id.to_string(),registration.binding.to_string(),registration.root,registration.data_root,registration.collection])?;
    let root = SourceRootId::from_identity(Incarnation::from_bytes([5; 16]));
    registry.connection.execute(
        "INSERT INTO index_sources VALUES(?1,?2,?3,1,1)",
        params![root.to_string(), id.to_string(), registration.root],
    )?;
    let tx = registry.connection.transaction()?;
    super::super::apply_authority_v1_layout(&tx)?;
    tx.execute(
        "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
        [],
    )?;
    tx.commit()?;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let caller = Incarnation::from_bytes([6; 16]);
    let key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([7; 16]),
    };
    let operation_id = airk_core::catalog_management::catalog_operation_id(caller, &key)?;
    let selector = ScopeAtom {
        source_root_id: root,
        kind: PathScope::Exact,
        path: "private.rs".into(),
    };
    let rule_id = DenyId::from_identity(Incarnation::from_bytes([8; 16]));
    let policy = SourcePolicy::initial_trim(
        &index_record(&registry.connection, id)?,
        source_roots(&registry.connection, id)?,
        rule_id,
        selector.clone(),
    )?;
    let request = CatalogRequest {
        caller,
        key,
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation: CatalogOperation::ChangeIndexPolicy {
            id,
            expected_source_revision: CatalogRevision::INITIAL,
            expected_policy_epoch: 0,
            expected_policy_digest: None,
            change: PolicyChange::Trim { rule_id, selector },
            engine_identity: Sha256Digest::from_bytes([9; 32]),
            configuration: Box::new(ConfigurationRequest {
                prepare: false,
                incarnation: namespace,
                settings: registration.binding,
                configuration: ProjectConfiguration::ApplySourcePolicy {
                    policy: Box::new(policy),
                    operation_id,
                },
            }),
        },
    };
    request.validate()?;
    Ok((temp, registry, request))
}
fn index(request: &CatalogRequest) -> IndexId {
    match &request.operation {
        CatalogOperation::ChangeIndexPolicy { id, .. }
        | CatalogOperation::RetireIndex { id, .. } => *id,
        _ => unreachable!(),
    }
}
fn apply(registry: &mut ServiceRegistry, request: &CatalogRequest) -> Result<()> {
    let digest = registry.catalog_preview(request)?.identity()?;
    let id = index(request);
    let backing = index_record(&registry.connection, id)?.backing.binding;
    let plan = FencePlan::new(Incarnation::from_bytes([10; 16]), 1, vec![])?
        .with_worker_obligation(WorkerDrainObligation {
            index_id: id,
            backing,
            workers: vec![],
            providers: vec![],
        })?;
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(request, digest, &plan)?
    else {
        anyhow::bail!("fresh fence expected");
    };
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    let receipt = registry.catalog_apply_fenced(request, digest, intent.prepared_digest)?;
    assert_eq!(
        registry.catalog_apply_fenced(request, digest, intent.prepared_digest)?,
        receipt
    );
    Ok(())
}

fn next_policy_request(
    registry: &ServiceRegistry,
    original: &CatalogRequest,
    change: PolicyChange,
    nonce: u8,
) -> Result<CatalogRequest> {
    let id = index(original);
    let current = registry
        .catalog_policy_status(id)?
        .context("current policy unavailable")?;
    let after = current
        .policy
        .preview_change(current.policy.identity()?, change.clone())?
        .after;
    let CatalogOperation::ChangeIndexPolicy {
        configuration,
        engine_identity,
        ..
    } = &original.operation
    else {
        unreachable!()
    };
    let key = RequestKey {
        nonce: Incarnation::from_bytes([nonce; 16]),
        ..original.key.clone()
    };
    let operation_id = airk_core::catalog_management::catalog_operation_id(original.caller, &key)?;
    let mut configuration = configuration.clone();
    configuration.configuration = ProjectConfiguration::ApplySourcePolicy {
        policy: Box::new(after),
        operation_id,
    };
    let request = CatalogRequest {
        caller: original.caller,
        key,
        expected_catalog_revision: super::super::state(&registry.connection)?.revision,
        operation: CatalogOperation::ChangeIndexPolicy {
            id,
            expected_source_revision: index_record(&registry.connection, id)?.source_revision,
            expected_policy_epoch: current.policy.epoch.get(),
            expected_policy_digest: Some(current.policy.identity()?),
            change,
            configuration,
            engine_identity: *engine_identity,
        },
    };
    request.validate()?;
    Ok(request)
}

#[test]
fn preview_rolls_back_layout_policy_and_job_and_apply_commits_together() -> Result<()> {
    let (temp, mut registry, request) = fixture()?;
    let id = index(&request);
    let source = temp.path().join("source/private.rs");
    let original = std::fs::read(&source)?;
    let before = super::super::schema_digest(&registry.connection)?;
    let preview = registry.catalog_preview(&request)?;
    assert!(preview.authority_fence_required && preview.file_data_changes);
    let impact = preview.source_policy_impact.as_ref().unwrap();
    assert_eq!(impact.target.path, "private.rs");
    assert_eq!(impact.target.kind, PathScope::Exact);
    assert!(
        impact.affected_artifacts.is_none()
            && impact.affected_vectors.is_none()
            && impact.affected_facts.is_none()
            && impact.affected_relationships.is_none()
    );
    assert_eq!(super::super::schema_digest(&registry.connection)?, before);
    assert_eq!(
        super::super::state(&registry.connection)?.policy_contract,
        0
    );
    assert!(registry.catalog_policy_status(id)?.is_none());
    let jobs: i64 =
        registry
            .connection
            .query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
    assert_eq!(jobs, 0);
    assert!(
        registry
            .catalog_apply(&request, preview.identity()?)
            .is_err()
    );
    apply(&mut registry, &request)?;
    let snapshot = registry.catalog_policy_status(id)?.unwrap();
    assert_eq!(
        super::super::state(&registry.connection)?.policy_contract,
        1
    );
    assert_eq!(snapshot.policy.epoch, CatalogRevision::INITIAL);
    assert!(snapshot.application.is_none());
    assert_eq!(registry.job(snapshot.job)?.state, JobState::Pending);
    assert!(
        !snapshot
            .policy
            .allows_canonical(snapshot.policy.roots[0].id, "private.rs")
    );
    assert_eq!(
        registry
            .configuration_work(snapshot.job)?
            .unwrap()
            .request
            .configuration,
        ProjectConfiguration::ApplySourcePolicy {
            policy: Box::new(snapshot.policy.clone()),
            operation_id: snapshot.operation_id
        }
    );
    assert!(
        registry
            .catalog_record_policy_job(id, snapshot.operation_id)
            .is_err()
    );
    assert!(
        registry
            .watch(snapshot.policy.owner_binding)?
            .configuration_epoch
            > 0
    );
    assert_eq!(std::fs::read(source)?, original);
    Ok(())
}

#[test]
fn malformed_work_and_stale_epoch_never_commit_a_policy_or_queue_job() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let mut changed = request.clone();
    if let CatalogOperation::ChangeIndexPolicy { configuration, .. } = &mut changed.operation {
        let ProjectConfiguration::ApplySourcePolicy { policy, .. } =
            &mut configuration.configuration
        else {
            unreachable!()
        };
        policy.denies[0].path = "different.rs".into();
    }
    assert!(registry.catalog_preview(&changed).is_err());
    changed = request.clone();
    if let CatalogOperation::ChangeIndexPolicy {
        expected_policy_epoch,
        ..
    } = &mut changed.operation
    {
        *expected_policy_epoch = 1;
    }
    assert!(registry.catalog_preview(&changed).is_err());
    assert!(registry.catalog_source_policy(index(&request))?.is_none());
    assert_eq!(
        super::super::state(&registry.connection)?.policy_contract,
        0
    );
    Ok(())
}

#[test]
fn reviewed_unblock_can_supersede_only_a_terminal_failed_pending_removal() -> Result<()> {
    for outcome in [
        JobOutcome::Failed {
            code: "unsupported_source_case_mode".into(),
        },
        JobOutcome::Canceled,
    ] {
        let (temp, mut registry, original) = fixture()?;
        let source = temp.path().join("source/private.rs");
        let source_before = std::fs::read(&source)?;
        apply(&mut registry, &original)?;
        let id = index(&original);
        let pending = registry.catalog_policy_status(id)?.unwrap();
        assert!(pending.application.is_none());
        let unblock = next_policy_request(
            &registry,
            &original,
            PolicyChange::Unblock {
                rule_id: pending.policy.denies[0].rule_id,
            },
            50,
        )?;
        assert!(
            registry.catalog_preview(&unblock).is_err(),
            "an accepted pending removal cannot be unblocked while it is still pending or running"
        );
        let state = match &outcome {
            JobOutcome::Failed { .. } => "failed",
            JobOutcome::Canceled => "canceled",
            _ => unreachable!(),
        };
        registry.connection.execute(
            "UPDATE service_jobs SET state=?1,outcome=?2 WHERE id=?3",
            params![
                state,
                serde_json::to_vec(&outcome)?,
                pending.job.to_string()
            ],
        )?;
        let additional_trim = next_policy_request(
            &registry,
            &original,
            PolicyChange::Trim {
                rule_id: DenyId::from_identity(Incarnation::from_bytes([51; 16])),
                selector: ScopeAtom {
                    source_root_id: pending.policy.roots[0].id,
                    kind: PathScope::Exact,
                    path: "other.rs".into(),
                },
            },
            51,
        )?;
        assert!(
            registry.catalog_preview(&additional_trim).is_err(),
            "terminal failure permits only the separately reviewed unblock escape"
        );
        let ordinary_key = RequestKey {
            nonce: Incarnation::from_bytes([52; 16]),
            ..original.key.clone()
        };
        assert!(
            registry
                .enqueue(
                    pending.policy.owner_binding,
                    original.caller,
                    &ordinary_key,
                    JobKind::Index
                )
                .is_err(),
            "ordinary indexing cannot implicitly unblock a failed removal"
        );
        let review = registry.catalog_preview(&unblock)?;
        assert_eq!(
            review.source_policy_impact.as_ref().unwrap().target.path,
            "private.rs"
        );
        assert!(review.file_data_changes);
        assert_eq!(
            registry.catalog_policy_status(id)?.unwrap().policy,
            pending.policy,
            "preview never lifts the deny"
        );
        apply(&mut registry, &unblock)?;
        let current = registry.catalog_policy_status(id)?.unwrap();
        assert_eq!(current.policy.epoch, CatalogRevision::INITIAL.next()?);
        assert!(current.policy.denies.is_empty());
        assert_eq!(current.policy.roots, pending.policy.roots);
        assert!(
            current.application.is_none(),
            "the new epoch still needs its own native attestation; unblock is not source reindexing"
        );
        assert_ne!(current.operation_id, pending.operation_id);
        assert_ne!(current.job, pending.job);
        assert_eq!(registry.job(current.job)?.state, JobState::Pending);
        assert!(
            require_claim(&registry.connection, pending.job).is_err(),
            "the superseded old operation can no longer claim native work"
        );
        let retry_key = RequestKey {
            nonce: Incarnation::from_bytes([53; 16]),
            ..original.key.clone()
        };
        assert!(
            registry
                .catalog_resume_policy_job(id, pending.operation_id, original.caller, &retry_key)
                .is_err()
        );
        let original_receipt = registry
            .request_receipt(original.caller, &original.key)?
            .0
            .unwrap();
        assert_eq!(original_receipt.id, pending.job);
        assert_eq!(original_receipt.outcome, Some(outcome));
        assert_eq!(registry.catalog_source_policy(id)?.unwrap(), current.policy);
        assert_eq!(std::fs::read(source)?, source_before);
    }
    Ok(())
}

#[test]
fn native_policy_job_runs_paused_and_public_configuration_cannot_bypass_review() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let CatalogOperation::ChangeIndexPolicy {
        configuration,
        engine_identity,
        ..
    } = &request.operation
    else {
        unreachable!()
    };
    let backing = index_record(&registry.connection, index(&request))?
        .backing
        .binding;
    registry.pause(backing, true)?;
    assert!(
        registry
            .enqueue_configuration(
                backing,
                request.caller,
                &request.key,
                configuration,
                *engine_identity
            )
            .is_err()
    );
    apply(&mut registry, &request)?;
    let snapshot = registry.catalog_policy_status(index(&request))?.unwrap();
    assert!(registry.job_runs_while_paused(snapshot.job)?);
    assert!(registry.pending_projects(16)?.contains(&backing));
    assert!(require_claim(&registry.connection, snapshot.job)?);
    Ok(())
}

#[test]
fn policy_job_retry_is_atomic_keyed_and_cannot_reinterpret_the_saved_configuration() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    apply(&mut registry, &request)?;
    let id = index(&request);
    let snapshot = registry.catalog_policy_status(id)?.unwrap();
    let failed = serde_json::to_vec(&JobOutcome::Failed {
        code: "fixture_io_failure".into(),
    })?;
    registry.connection.execute(
        "UPDATE service_jobs SET state='failed',outcome=?1 WHERE id=?2",
        params![failed, snapshot.job.to_string()],
    )?;
    assert_eq!(
        registry
            .catalog_policy_status(id)?
            .unwrap()
            .last_error
            .as_deref(),
        Some("fixture_io_failure")
    );
    let key = RequestKey {
        nonce: Incarnation::from_bytes([11; 16]),
        ..request.key.clone()
    };
    let retry =
        registry.catalog_resume_policy_job(id, snapshot.operation_id, request.caller, &key)?;
    assert_ne!(retry.job, snapshot.job);
    assert_eq!(retry.policy, snapshot.policy);
    assert!(retry.last_error.is_none());
    assert_eq!(
        registry.catalog_resume_policy_job(id, snapshot.operation_id, request.caller, &key)?,
        retry
    );
    assert_eq!(
        registry.configuration_work(retry.job)?,
        registry.configuration_work(snapshot.job)?
    );
    assert!(require_claim(&registry.connection, retry.job)?);
    assert!(
        !super::super::source_change::pinned_jobs(&registry.connection)?.contains(&snapshot.job)
    );
    assert!(super::super::source_change::pinned_jobs(&registry.connection)?.contains(&retry.job));
    Ok(())
}

#[test]
fn replayed_resume_preserves_its_accepted_job_receipt_while_current_status_advances() -> Result<()>
{
    let (_temp, mut registry, request) = fixture()?;
    apply(&mut registry, &request)?;
    let id = index(&request);
    let original = registry.catalog_policy_status(id)?.unwrap();
    let fail = |registry: &ServiceRegistry, job: Sha256Digest| -> Result<()> {
        registry.connection.execute(
            "UPDATE service_jobs SET state='failed',outcome=?1 WHERE id=?2",
            params![
                serde_json::to_vec(&JobOutcome::Failed {
                    code: "fixture_io_failure".into()
                })?,
                job.to_string()
            ],
        )?;
        Ok(())
    };
    fail(&registry, original.job)?;
    let first_key = RequestKey {
        nonce: Incarnation::from_bytes([41; 16]),
        ..request.key.clone()
    };
    let first = registry.catalog_resume_policy_job(
        id,
        original.operation_id,
        request.caller,
        &first_key,
    )?;
    assert_eq!(
        registry
            .request_receipt(request.caller, &first_key)?
            .0
            .unwrap()
            .id,
        first.job
    );
    fail(&registry, first.job)?;
    let second_key = RequestKey {
        nonce: Incarnation::from_bytes([42; 16]),
        ..request.key.clone()
    };
    let second = registry.catalog_resume_policy_job(
        id,
        original.operation_id,
        request.caller,
        &second_key,
    )?;
    assert_ne!(second.job, first.job);
    let before: i64 =
        registry
            .connection
            .query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
    let replay = registry.catalog_resume_policy_job(
        id,
        original.operation_id,
        request.caller,
        &first_key,
    )?;
    assert_eq!(
        replay, second,
        "resume status must describe the current policy job, without rolling back its mapping"
    );
    assert_eq!(
        registry
            .request_receipt(request.caller, &first_key)?
            .0
            .unwrap()
            .id,
        first.job,
        "the original accepted request receipt remains bound to its original job"
    );
    assert_eq!(
        registry
            .request_receipt(request.caller, &second_key)?
            .0
            .unwrap()
            .id,
        second.job
    );
    let after: i64 =
        registry
            .connection
            .query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
    assert_eq!(
        after, before,
        "an accepted-key replay must not enqueue new work"
    );
    Ok(())
}

#[test]
fn report_identity_and_cleanup_progress_are_validated_before_recording() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    apply(&mut registry, &request)?;
    let current = registry.catalog_policy_status(index(&request))?.unwrap();
    let mut report = SourcePolicyApplyReport {
        observed_unix_ms: 1,
        policy_digest: current.policy.identity()?,
        operation_id: current.operation_id,
        removal_publication: Some(RevisionToken {
            incarnation: Incarnation::from_bytes([12; 16]),
            generation: 2,
            revision: Sha256Digest::from_bytes([13; 32]),
        }),
        no_index: false,
        removed_artifacts: 1,
        retired_components: vec![],
        physical_cleanup_pending: true,
        reclaimed_files: 0,
        reclaimed_bytes: None,
        cleanup_error: Some("fixture_capacity".into()),
    };
    validate_result(
        &registry.connection,
        current.job,
        &serde_json::to_value(&report)?,
    )?;
    report.operation_id = Incarnation::from_bytes([14; 16]);
    assert!(
        validate_result(
            &registry.connection,
            current.job,
            &serde_json::to_value(&report)?
        )
        .is_err()
    );
    report.operation_id = current.operation_id;
    report.physical_cleanup_pending = false;
    assert!(
        validate_result(
            &registry.connection,
            current.job,
            &serde_json::to_value(&report)?
        )
        .is_err()
    );
    assert!(
        registry
            .catalog_policy_status(index(&request))?
            .unwrap()
            .application
            .is_none()
    );
    Ok(())
}

#[test]
fn pending_policy_retains_source_and_vector_jobs_without_selecting_or_charging_attempts()
-> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    apply(&mut registry, &request)?;
    let current = registry.catalog_policy_status(index(&request))?.unwrap();
    let source = Sha256Digest::from_bytes([30; 32]);
    let vector = Sha256Digest::from_bytes([31; 32]);
    for (id, kind) in [(source, "index"), (vector, "vectors")] {
        registry.connection.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,?3,'pending',0,0)",params![id.to_string(),current.policy.owner_binding.to_string(),kind])?;
    }
    // Model an older accepted FIFO position to prove the policy selector, rather
    // than incidental insertion order, owns admission while removal is pending.
    registry.connection.execute(
        "UPDATE service_jobs SET seq=10000 WHERE id=?1",
        [current.job.to_string()],
    )?;
    assert_eq!(
        registry
            .next_pending_job(current.policy.owner_binding)?
            .unwrap()
            .id,
        current.job
    );
    registry.connection.execute(
        "UPDATE service_jobs SET state='failed',outcome=?1 WHERE id=?2",
        params![
            serde_json::to_vec(&JobOutcome::Failed {
                code: "fixture_interrupted".into()
            })?,
            current.job.to_string()
        ],
    )?;
    assert!(
        registry
            .next_pending_job(current.policy.owner_binding)?
            .is_none()
    );
    assert!(
        !registry
            .pending_projects(16)?
            .contains(&current.policy.owner_binding)
    );
    for id in [source, vector] {
        let job = registry.job(id)?;
        assert_eq!(job.state, JobState::Pending);
        assert_eq!(job.attempts, 0);
    }
    Ok(())
}

#[test]
fn policy_snapshot_survives_reopen_and_disabled_roots_without_predecessor_shape_fallback()
-> Result<()> {
    let (temp, mut registry, request) = fixture()?;
    apply(&mut registry, &request)?;
    let id = index(&request);
    let expected = registry.catalog_policy_status(id)?.unwrap();
    assert!(
        super::super::validate_shape(&registry.connection, 12).is_err(),
        "policy-1 must not match the compiled predecessor layout"
    );
    registry.connection.execute(
        "UPDATE index_sources SET enabled=0 WHERE index_id=?1",
        [id.to_string()],
    )?;
    assert_eq!(registry.catalog_policy_status(id)?.unwrap(), expected);
    drop(registry);
    let registry = ServiceRegistry::open(
        &temp.path().join("control"),
        Incarnation::from_bytes([1; 16]),
    )?;
    assert_eq!(registry.catalog_policy_status(id)?.unwrap(), expected);
    assert_eq!(
        std::fs::read(temp.path().join("source/private.rs"))?,
        b"pub struct Private;\n"
    );
    Ok(())
}

#[test]
fn policy_completion_reserve_absorbs_reports_and_releases_only_after_physical_completion()
-> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    apply(&mut registry, &request)?;
    let id = index(&request);
    let mut snapshot = registry.catalog_policy_status(id)?.unwrap();
    let record_bytes = |registry: &ServiceRegistry| -> Result<i64> {
        Ok(registry.connection.query_row("SELECT length(payload)+length(completion_reserve) FROM index_source_policies WHERE index_id=?1",[id.to_string()],|row|row.get(0))?)
    };
    let reserved = record_bytes(&registry)?;
    snapshot.application = Some(SourcePolicyApplyReport {
        observed_unix_ms: 1,
        policy_digest: snapshot.policy.identity()?,
        operation_id: snapshot.operation_id,
        removal_publication: None,
        no_index: true,
        removed_artifacts: 0,
        retired_components: vec![],
        physical_cleanup_pending: true,
        reclaimed_files: 0,
        reclaimed_bytes: None,
        cleanup_error: Some("e".repeat(1024)),
    });
    snapshot.application_job = Some(snapshot.job);
    write_snapshot(&registry.connection, &snapshot)?;
    assert_eq!(record_bytes(&registry)?, reserved);
    let report = snapshot.application.as_mut().unwrap();
    report.physical_cleanup_pending = false;
    report.cleanup_error = None;
    report.observed_unix_ms = 2;
    write_snapshot(&registry.connection, &snapshot)?;
    assert!(record_bytes(&registry)? < reserved);
    let remaining: i64 = registry.connection.query_row(
        "SELECT length(completion_reserve) FROM index_source_policies WHERE index_id=?1",
        [id.to_string()],
        |row| row.get(0),
    )?;
    assert_eq!(remaining, 0);
    Ok(())
}

#[test]
fn saved_native_results_reconcile_after_restart_and_physical_continuation_observes_once()
-> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    assert!(registry.catalog_reconcile_policy_completions(0).is_err());
    assert!(
        registry
            .catalog_reconcile_policy_completions(16)?
            .is_empty()
    );
    apply(&mut registry, &request)?;
    let id = index(&request);
    let snapshot = registry.catalog_policy_status(id)?.unwrap();
    let mut lease =
        crate::service::ProjectLease::acquire(&registry.root, snapshot.policy.owner_binding)?;
    registry.admit_worker(&mut lease)?;
    registry
        .claim_job_id(&lease, snapshot.job)?
        .context("policy job was not claimed")?;
    let mut report = SourcePolicyApplyReport {
        observed_unix_ms: 1,
        policy_digest: snapshot.policy.identity()?,
        operation_id: snapshot.operation_id,
        removal_publication: None,
        no_index: true,
        removed_artifacts: 0,
        retired_components: vec![],
        physical_cleanup_pending: true,
        reclaimed_files: 0,
        reclaimed_bytes: None,
        cleanup_error: None,
    };
    registry.record_configuration_result(&lease, snapshot.job, &serde_json::to_value(&report)?)?;
    registry.complete_job(
        &lease,
        snapshot.job,
        JobOutcome::Succeeded { revision: None },
    )?;
    // Model loss of the worker after successful result settlement but before its
    // final catalog metadata observation. No source/evidence read repairs this.
    assert!(registry.pending_source_completions(&lease)?.is_empty());
    assert!(
        registry
            .catalog_policy_status(id)?
            .unwrap()
            .application
            .is_none()
    );
    let completed = registry.catalog_reconcile_policy_completions(1)?;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].application_job, Some(snapshot.job));
    assert!(
        completed[0]
            .application
            .as_ref()
            .unwrap()
            .physical_cleanup_pending
    );
    assert!(
        registry.catalog_reconcile_policy_completions(1)?.is_empty(),
        "an unchanged pending physical receipt must not cause maintenance spin"
    );
    let key = RequestKey {
        nonce: Incarnation::from_bytes([40; 16]),
        ..request.key.clone()
    };
    let continuation =
        registry.catalog_resume_policy_job(id, snapshot.operation_id, request.caller, &key)?;
    assert_ne!(continuation.job, snapshot.job);
    assert_eq!(continuation.application_job, Some(snapshot.job));
    registry
        .claim_job_id(&lease, continuation.job)?
        .context("physical continuation was not claimed")?;
    report.observed_unix_ms = 2;
    report.reclaimed_files = 1;
    report.physical_cleanup_pending = false;
    registry.record_configuration_result(
        &lease,
        continuation.job,
        &serde_json::to_value(&report)?,
    )?;
    registry.complete_job(
        &lease,
        continuation.job,
        JobOutcome::Succeeded { revision: None },
    )?;
    let completed = registry.catalog_reconcile_policy_completions(1)?;
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].application_job, Some(continuation.job));
    assert!(
        !completed[0]
            .application
            .as_ref()
            .unwrap()
            .physical_cleanup_pending
    );
    assert!(registry.catalog_reconcile_policy_completions(1)?.is_empty());
    assert!(registry.pending_source_completions(&lease)?.is_empty());
    assert!(
        super::super::source_change::pinned_jobs(&registry.connection)?.contains(&continuation.job),
        "current policy status still needs its completed job after cleanup"
    );
    Ok(())
}

#[test]
fn retirement_materializes_a_pending_deny_and_preserves_its_exact_status_job() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    apply(&mut registry, &request)?;
    let id = index(&request);
    let before = registry.catalog_policy_status(id)?.unwrap();
    let definition = index_record(&registry.connection, id)?;
    let registration = definition.backing.clone();
    let data = std::path::Path::new(&registration.data_root);
    std::fs::create_dir_all(data)?;
    crate::service::data_scope::bind(data, &registry.root, registry.incarnation()?)?;
    let collection = crate::service::collection::Collection::initialize(
        data,
        &registration.root,
        &registration.collection,
        Incarnation::from_bytes([46; 16]),
    )?;
    let directory = data.join("collections-1.4").join(&registration.collection);
    assert!(crate::current::policy_capsule::PolicyCapsule::load(&directory)?.is_none());
    drop(collection);
    let original_source =
        std::fs::read(std::path::Path::new(&registration.root).join("private.rs"))?;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let retire = CatalogRequest {
        caller: request.caller,
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([47; 16]),
        },
        expected_catalog_revision: super::super::state(&registry.connection)?.revision,
        operation: CatalogOperation::RetireIndex {
            id,
            expected_definition_revision: definition.definition_revision,
            expected_source_revision: definition.source_revision,
            reason: airk_core::catalog_source_change::IndexRetirementReason::Explicit,
        },
    };
    apply(&mut registry, &retire)?;
    assert_eq!(registry.job(before.job)?.state, JobState::Canceled);
    let mut complete = false;
    for _ in 0..8 {
        if registry.purge_removed_project_turn(registration.binding)? {
            complete = true;
            break;
        }
    }
    assert!(complete, "admitted policy stranded native retirement");
    let retained = crate::current::policy_capsule::PolicyCapsule::load(&directory)?
        .context("retirement discarded permanent policy")?;
    assert_eq!(retained.policy, before.policy);
    assert!(
        retained.application.is_none(),
        "registry status cannot fabricate native application"
    );
    let observed = registry.catalog_policy_status(id)?.unwrap();
    assert_eq!(observed.policy, before.policy);
    assert_eq!(observed.job, before.job);
    assert_eq!(registry.job(observed.job)?.state, JobState::Canceled);
    assert!(registry.project_was_removed(registration.binding)?);
    assert!(registry.pending_project_removals(None, 32)?.is_empty());
    assert!(registry.purge_removed_project_turn(registration.binding)?);
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let fresh_key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([48; 16]),
    };
    assert!(
        registry
            .catalog_resume_policy_job(id, before.operation_id, request.caller, &fresh_key)
            .is_err()
    );
    assert!(
        registry.register(&registration).is_err(),
        "retired backing must not be recreated without its policy"
    );
    assert_eq!(
        std::fs::read(std::path::Path::new(&registration.root).join("private.rs"))?,
        original_source
    );
    let control = registry.root.clone();
    drop(registry);
    let reopened = ServiceRegistry::open_saved(&control)?;
    assert_eq!(
        reopened.catalog_policy_status(id)?.unwrap().policy,
        before.policy
    );
    assert_eq!(reopened.job(before.job)?.state, JobState::Canceled);
    Ok(())
}

#[test]
fn exact_resume_receipt_survives_policy_supersession_without_replaying_work() -> Result<()> {
    for outcome in [
        JobOutcome::Failed {
            code: "fixture_io_failure".into(),
        },
        JobOutcome::Canceled,
        JobOutcome::Succeeded { revision: None },
    ] {
        let (_temp, mut registry, original) = fixture()?;
        apply(&mut registry, &original)?;
        let id = index(&original);
        let before = registry.catalog_policy_status(id)?.unwrap();
        registry.connection.execute(
            "UPDATE service_jobs SET state='failed',outcome=?1 WHERE id=?2",
            params![
                serde_json::to_vec(&JobOutcome::Failed {
                    code: "fixture_io_failure".into()
                })?,
                before.job.to_string()
            ],
        )?;
        let key = RequestKey {
            nonce: Incarnation::from_bytes([101; 16]),
            ..original.key.clone()
        };
        let retry =
            registry.catalog_resume_policy_job(id, before.operation_id, original.caller, &key)?;
        let state = match &outcome {
            JobOutcome::Failed { .. } => "failed",
            JobOutcome::Canceled => "canceled",
            _ => "succeeded",
        };
        registry.connection.execute(
            "UPDATE service_jobs SET state=?1,outcome=?2 WHERE id=?3",
            params![state, serde_json::to_vec(&outcome)?, retry.job.to_string()],
        )?;
        if state == "succeeded" {
            let report = SourcePolicyApplyReport {
                observed_unix_ms: 1,
                policy_digest: before.policy.identity()?,
                operation_id: before.operation_id,
                removal_publication: None,
                no_index: true,
                removed_artifacts: 0,
                retired_components: vec![],
                physical_cleanup_pending: false,
                reclaimed_files: 0,
                reclaimed_bytes: Some(0),
                cleanup_error: None,
            };
            super::super::super::configuration::write_result(
                &registry.connection,
                retry.job,
                &serde_json::to_vec(&report)?,
            )?;
            registry.catalog_record_policy_job(id, before.operation_id)?;
        }
        let change = if state == "succeeded" {
            PolicyChange::Trim {
                rule_id: DenyId::from_identity(Incarnation::from_bytes([105; 16])),
                selector: ScopeAtom {
                    source_root_id: before.policy.roots[0].id,
                    kind: PathScope::Exact,
                    path: "other.rs".into(),
                },
            }
        } else {
            PolicyChange::Unblock {
                rule_id: before.policy.denies[0].rule_id,
            }
        };
        let successor = next_policy_request(&registry, &original, change, 102)?;
        apply(&mut registry, &successor)?;
        let current = registry.catalog_policy_status(id)?.unwrap();
        assert_ne!(current.operation_id, before.operation_id);
        let jobs: i64 =
            registry
                .connection
                .query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
        let (receipt, expired) = registry.catalog_policy_resume_receipt(
            id,
            before.operation_id,
            original.caller,
            &key,
        )?;
        assert!(!expired);
        let receipt = receipt.unwrap();
        assert_eq!(receipt.job.id, retry.job);
        assert_eq!(receipt.job.state, registry.job(retry.job)?.state);
        assert_eq!(
            receipt.configuration,
            registry.configuration_work(retry.job)?.unwrap()
        );
        assert_eq!(receipt.source_job, before.job);
        assert_eq!(receipt.result.is_some(), state == "succeeded");
        assert_eq!(
            registry
                .connection
                .query_row("SELECT count(*) FROM service_jobs", [], |row| row
                    .get::<_, i64>(0))?,
            jobs
        );
        assert_eq!(
            registry.catalog_policy_status(id)?.unwrap().operation_id,
            current.operation_id
        );
        assert!(
            registry
                .catalog_policy_resume_receipt(id, current.operation_id, original.caller, &key)
                .is_err()
        );
        assert!(
            registry
                .catalog_policy_resume_receipt(
                    airk_core::index_catalog::IndexId::from_identity(Incarnation::from_bytes(
                        [104; 16]
                    )),
                    before.operation_id,
                    original.caller,
                    &key
                )
                .is_err()
        );
        assert!(
            registry
                .catalog_policy_resume_receipt(
                    id,
                    before.operation_id,
                    Incarnation::from_bytes([104; 16]),
                    &key
                )?
                .0
                .is_none()
        );
        assert!(
            registry
                .catalog_policy_resume_receipt(
                    id,
                    before.operation_id,
                    original.caller,
                    &original.key
                )
                .is_err(),
            "an original admission key is not a retry receipt"
        );
        let missing = RequestKey {
            nonce: Incarnation::from_bytes([103; 16]),
            ..key.clone()
        };
        assert!(
            registry
                .catalog_policy_resume_receipt(id, before.operation_id, original.caller, &missing)?
                .0
                .is_none()
        );
        let expired_key = RequestKey {
            epoch: key.epoch + 1,
            ..missing.clone()
        };
        let (absent, expired) = registry.catalog_policy_resume_receipt(
            id,
            before.operation_id,
            original.caller,
            &expired_key,
        )?;
        assert!(absent.is_none() && expired);
        registry.connection.execute(
            "DELETE FROM service_jobs WHERE id=?1",
            [retry.job.to_string()],
        )?;
        let (pruned, _) = registry.catalog_policy_resume_receipt(
            id,
            before.operation_id,
            original.caller,
            &key,
        )?;
        assert!(
            pruned.is_none(),
            "pruned evidence never establishes completion or known no-effect"
        );
    }
    Ok(())
}
