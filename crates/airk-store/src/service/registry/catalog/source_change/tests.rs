// SPDX-License-Identifier: Apache-2.0
//! Isolated Store metadata fixtures. Native RootConfig ownership and filesystem
//! review are supplied by the native integration, never inferred from this SQL.
use super::*;
use crate::service::ProjectLease;
use airk_core::{
    catalog_fences::{FencePlan, FencePreparation},
    service::JobOutcome,
    source_policy::SourceRoot,
};

mod discovery;

mod retirement_evidence;

fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, CatalogRequest)> {
    fixture_for_authority(1)
}
fn fixture_for_authority(
    authority: u32,
) -> Result<(tempfile::TempDir, ServiceRegistry, CatalogRequest)> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    for folder in ["control", "source", "extra"] {
        std::fs::create_dir(base.join(folder))?;
    }
    let mut registry =
        ServiceRegistry::open(&base.join("control"), Incarnation::from_bytes([1; 16]))?;
    let registration = ProjectRegistration::new(
        base.join("source").to_str().unwrap().into(),
        base.join("data").to_str().unwrap().into(),
        "fixture".into(),
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
    registry.connection.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,'Source fixture',1,1,0,NULL,0,'complete')",params![id.to_string(),registration.binding.to_string(),registration.root,registration.data_root,registration.collection])?;
    let owner =
        airk_core::index_catalog::SourceRootId::from_identity(Incarnation::from_bytes([5; 16]));
    registry.connection.execute(
        "INSERT INTO index_sources VALUES(?1,?2,?3,1,1)",
        params![owner.to_string(), id.to_string(), registration.root],
    )?;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let caller = Incarnation::from_bytes([6; 16]);
    let key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([7; 16]),
    };
    let operation = catalog_operation_id(caller, &key)?;
    let extra = base.join("extra").to_str().unwrap().to_owned();
    let before = SourceConfigurationSnapshot {
        primary_root: registration.root.clone(),
        aliases: BTreeMap::new(),
    };
    let after = SourceConfigurationSnapshot {
        aliases: BTreeMap::from([("extra".into(), extra.clone())]),
        ..before.clone()
    };
    let plan = SourceChangePlan {
        registration: registration.clone(),
        engine_identity: Sha256Digest::from_bytes([8; 32]),
        job: airk_core::catalog_source_change::source_job_id(registration.binding, caller, &key)?,
        configuration: ConfigurationRequest {
            prepare: false,
            incarnation: namespace,
            settings: registration.binding,
            configuration: ProjectConfiguration::AddRoot {
                alias: "extra".into(),
                path: extra.clone(),
            },
        },
        before,
        after,
        sources: vec![
            SourceRoot {
                id: owner,
                canonical_path: registration.root,
                aliases: vec![],
                owner: true,
            },
            SourceRoot {
                id: new_source_id(id, operation, &extra)?,
                canonical_path: extra,
                aliases: vec!["extra".into()],
                owner: false,
            },
        ],
    };
    let request = CatalogRequest {
        caller,
        key,
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation: CatalogOperation::BeginIndexSourceChange {
            id,
            expected_source_revision: CatalogRevision::INITIAL,
            plan: Box::new(plan),
        },
    };
    request.validate()?;
    // Exact compiled selected authority format; this unit fixture does not
    // claim held-owner activation or filesystem review qualification.
    let tx = registry.connection.transaction()?;
    if authority == 1 {
        super::super::apply_authority_v1_layout(&tx)?;
        tx.execute(
            "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
            [],
        )?;
    }
    tx.commit()?;
    Ok((temp, registry, request))
}
fn apply(registry: &mut ServiceRegistry, request: &CatalogRequest) -> Result<FencedReceipt> {
    let digest = registry.catalog_preview(request)?.identity()?;
    let mut plan = FencePlan::new(Incarnation::from_bytes([9; 16]), 1, vec![])?;
    if let CatalogOperation::RetireIndex { id, .. } = &request.operation {
        let backing = registration(&registry.connection, *id)?.0.binding;
        plan = plan.with_worker_obligation(airk_core::catalog_fences::WorkerDrainObligation {
            index_id: *id,
            backing,
            workers: vec![],
            providers: vec![],
        })?;
    }
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(request, digest, &plan)?
    else {
        anyhow::bail!("fresh native fence required");
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
    Ok(receipt)
}
fn source_plan(request: &CatalogRequest) -> &SourceChangePlan {
    let CatalogOperation::BeginIndexSourceChange { plan, .. } = &request.operation else {
        unreachable!()
    };
    plan
}
fn native(request: &CatalogRequest) -> Result<NativeSourceConfiguration> {
    let plan = source_plan(request);
    NativeSourceConfiguration::from_validated_native_owner(
        plan.registration.clone(),
        plan.engine_identity,
        plan.after.clone(),
    )
}
#[test]
fn begin_is_atomic_disabled_and_exact_job_replay_survives_new_admission_guards() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let plan = source_plan(&request);
    let before: i64 =
        registry
            .connection
            .query_row("SELECT count(*) FROM service_jobs", [], |r| r.get(0))?;
    assert!(
        registry
            .catalog_apply(&request, Sha256Digest::from_bytes([0; 32]))
            .is_err()
    );
    let receipt = apply(&mut registry, &request)?;
    let state:(String,i64,i64)=registry.connection.query_row("SELECT source_import_state,source_revision,(SELECT count(*) FROM index_sources WHERE enabled=1) FROM index_definitions",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    assert_eq!(state, ("pending_metadata".into(), 2, 0));
    let after: i64 =
        registry
            .connection
            .query_row("SELECT count(*) FROM service_jobs", [], |r| r.get(0))?;
    assert_eq!(after, before + 1);
    assert_eq!(
        registry.catalog_apply(&request, receipt.preview_digest)?,
        receipt.metadata
    );
    let replay = registry.enqueue_configuration(
        plan.registration.binding,
        request.caller,
        &request.key,
        &plan.configuration,
        plan.engine_identity,
    )?;
    assert_eq!(replay.id, plan.job);
    assert_eq!(replay.state, JobState::Pending);
    let observation = registry.catalog_source_change_observation(receipt.metadata.operation_id)?;
    assert_eq!(observation.begin, receipt);
    assert_eq!(observation.job.unwrap().id, plan.job);
    assert!(observation.completion.is_none());
    Ok(())
}
#[test]
fn actual_saved_result_completes_with_fresh_catalog_revision_and_replays_before_review()
-> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let begin = apply(&mut registry, &request)?;
    let plan = source_plan(&request);
    let actual = native(&request)?;
    assert!(
        registry
            .complete_catalog_source_change(begin.metadata.operation_id, &actual)
            .is_err()
    );
    let mut lease = ProjectLease::acquire(&registry.root, plan.registration.binding)?;
    registry.admit_worker(&mut lease)?;
    registry
        .claim_job_id(&lease, plan.job)?
        .context("source job not claimed")?;
    assert!(
        registry
            .record_configuration_result(&lease, plan.job, &serde_json::json!({"updated":true}))
            .is_err()
    );
    assert!(
        registry
            .complete_job(&lease, plan.job, JobOutcome::Succeeded { revision: None })
            .is_err()
    );
    let result = registry.qualify_catalog_source_result(
        &lease,
        plan.job,
        &actual,
        &serde_json::json!({"updated":true}),
    )?;
    registry.record_configuration_result(&lease, plan.job, &result)?;
    registry.complete_job(&lease, plan.job, JobOutcome::Succeeded { revision: None })?;
    let CatalogOperation::BeginIndexSourceChange { id, .. } = &request.operation else {
        unreachable!()
    };
    let rename = CatalogRequest {
        key: RequestKey {
            nonce: Incarnation::from_bytes([10; 16]),
            ..request.key.clone()
        },
        expected_catalog_revision: begin.metadata.catalog_revision,
        operation: CatalogOperation::RenameIndex {
            id: *id,
            name: "Renamed while native work ran".into(),
            expected_definition_revision: CatalogRevision::INITIAL,
        },
        ..request.clone()
    };
    let preview = registry.catalog_preview(&rename)?;
    registry.catalog_apply(&rename, preview.identity()?)?;
    registry.connection.execute(
        "UPDATE service_meta SET request_epoch=request_epoch+1 WHERE id=1",
        [],
    )?;
    let receipt = registry.complete_catalog_source_change(begin.metadata.operation_id, &actual)?;
    assert_eq!(receipt.catalog_revision.get(), 4);
    let state: (String, String, i64) = registry.connection.query_row(
        "SELECT name,source_import_state,source_revision FROM index_definitions",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    assert_eq!(
        state,
        ("Renamed while native work ran".into(), "complete".into(), 2)
    );
    let wrong = NativeSourceConfiguration::from_validated_native_owner(
        plan.registration.clone(),
        Sha256Digest::from_bytes([11; 32]),
        plan.before.clone(),
    )?;
    assert_eq!(
        registry.complete_catalog_source_change(begin.metadata.operation_id, &wrong)?,
        receipt
    );
    Ok(())
}
#[test]
fn source_capacity_refusal_has_no_job_source_or_receipt_side_effects() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    for n in 1u128..=1023 {
        registry.connection.execute("INSERT INTO index_operation_receipts VALUES(?1,'fixture','metadata_applied',1,X'7b7d')",[Incarnation::from_bytes(n.to_be_bytes()).to_string()])?;
    }
    assert!(registry.catalog_preview(&request).is_err());
    let state:(i64,i64,i64)=registry.connection.query_row("SELECT (SELECT count(*) FROM service_jobs),(SELECT count(*) FROM index_sources WHERE enabled=1),(SELECT count(*) FROM index_operation_receipts)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    assert_eq!(state, (0, 1, 1023));
    Ok(())
}
#[test]
fn canonical_retirement_cancels_pending_source_work_and_saved_begin_remains_replayable()
-> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let begin = apply(&mut registry, &request)?;
    let CatalogOperation::BeginIndexSourceChange { id, .. } = &request.operation else {
        unreachable!()
    };
    let retire = CatalogRequest {
        key: RequestKey {
            nonce: Incarnation::from_bytes([12; 16]),
            ..request.key.clone()
        },
        expected_catalog_revision: begin.metadata.catalog_revision,
        operation: CatalogOperation::RetireIndex {
            id: *id,
            expected_definition_revision: CatalogRevision::INITIAL,
            expected_source_revision: CatalogRevision::INITIAL.next()?,
            reason: IndexRetirementReason::Explicit,
        },
        ..request.clone()
    };
    apply(&mut registry, &retire)?;
    assert_eq!(
        registry.job(source_plan(&request).job)?.state,
        JobState::Canceled
    );
    let state:(bool,bool,i64)=registry.connection.query_row("SELECT tombstone,(SELECT registered FROM projects),(SELECT count(*) FROM index_sources WHERE enabled=1) FROM index_definitions",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    assert_eq!(state, (true, false, 0));
    assert!(
        registry
            .complete_catalog_source_change(begin.metadata.operation_id, &native(&request)?)
            .is_err()
    );
    assert_eq!(
        registry.catalog_apply_fenced(&request, begin.preview_digest, begin.prepared_digest)?,
        begin
    );
    Ok(())
}
#[test]
fn authority_one_blocks_legacy_source_and_derived_side_effects_before_canonical_review()
-> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let plan = source_plan(&request);
    super::super::validate_catalog_layout(&registry.connection)?;
    let error = require_configuration_admission(&registry.connection, &plan.configuration)
        .expect_err("legacy source changes require native catalog review");
    assert!(error.is::<SourceChangeReviewRequired>());
    assert!(require_retired(&registry.connection, plan.registration.binding).is_err());
    assert!(
        registry
            .retire_missing_source(plan.registration.binding, 0, 0)
            .is_err()
    );
    let state: bool =
        registry
            .connection
            .query_row("SELECT registered FROM projects", [], |r| r.get(0))?;
    assert!(state);
    Ok(())
}

#[test]
fn pending_source_completion_survives_reopen_and_new_worker_epoch() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let begin = apply(&mut registry, &request)?;
    let plan = source_plan(&request);
    let actual = native(&request)?;
    let root = registry.root.clone();
    let mut lease = ProjectLease::acquire(&root, plan.registration.binding)?;
    registry.admit_worker(&mut lease)?;
    assert!(registry.pending_source_completions(&lease)?.is_empty());
    registry
        .claim_job_id(&lease, plan.job)?
        .context("missing source job")?;
    assert!(registry.pending_source_completions(&lease)?.is_empty());
    let result = registry.qualify_catalog_source_result(
        &lease,
        plan.job,
        &actual,
        &serde_json::json!({"updated":true}),
    )?;
    registry.record_configuration_result(&lease, plan.job, &result)?;
    registry.complete_job(&lease, plan.job, JobOutcome::Succeeded { revision: None })?;
    drop(lease);
    drop(registry);
    let mut registry = ServiceRegistry::open(&root, request.key.namespace)?;
    let mut replacement = ProjectLease::acquire(&root, plan.registration.binding)?;
    registry.admit_worker(&mut replacement)?;
    registry.recover_jobs(&replacement)?;
    let pending = registry.pending_source_completions(&replacement)?;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].begin, request);
    assert_eq!(pending[0].job, plan.job);
    let foreign = ProjectLease::acquire(&root, Sha256Digest::from_bytes([99; 32]))?;
    assert!(registry.pending_source_completions(&foreign).is_err());
    registry.complete_catalog_source_change(begin.metadata.operation_id, &actual)?;
    assert!(
        registry
            .pending_source_completions(&replacement)?
            .is_empty()
    );
    Ok(())
}
#[test]
fn interrupted_exact_source_job_requeues_without_reinterpreting_accepted_work() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    apply(&mut registry, &request)?;
    let plan = source_plan(&request);
    let root = registry.root.clone();
    let mut lease = ProjectLease::acquire(&root, plan.registration.binding)?;
    registry.admit_worker(&mut lease)?;
    registry
        .claim_job_id(&lease, plan.job)?
        .context("missing source job")?;
    let original = registry
        .configuration_work(plan.job)?
        .context("missing configuration")?;
    drop(lease);
    let mut replacement = ProjectLease::acquire(&root, plan.registration.binding)?;
    registry.admit_worker(&mut replacement)?;
    registry.recover_jobs(&replacement)?;
    assert_eq!(registry.job(plan.job)?.state, JobState::Pending);
    assert_eq!(registry.configuration_work(plan.job)?, Some(original));
    assert!(registry.claim_job_id(&replacement, plan.job)?.is_some());
    assert!(
        registry
            .catalog_source_work(&replacement, plan.job)?
            .is_some()
    );
    Ok(())
}
#[test]
fn queued_accepted_work_refuses_source_change_before_fence_then_allows_after_settlement()
-> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let plan = source_plan(&request);
    let preview = registry.catalog_preview(&request)?.identity()?;
    let key = RequestKey {
        nonce: Incarnation::from_bytes([66; 16]),
        ..request.key.clone()
    };
    let pending = registry.enqueue(
        plan.registration.binding,
        request.caller,
        &key,
        JobKind::Index,
    )?;
    let failure = registry.catalog_preview(&request).unwrap_err();
    let busy = failure
        .downcast_ref::<SourceChangeBusy>()
        .context("typed source busy details missing")?;
    assert_eq!((busy.pending_jobs, busy.running_jobs), (1, 0));
    assert!(
        failure
            .downcast_ref::<airk_core::catalog_fences::FencePrepareRejected>()
            .is_none(),
        "ordinary preview is not a native preinsert rejection proof"
    );
    let fence_plan = FencePlan::new(Incarnation::from_bytes([9; 16]), 1, vec![])?;
    let rejected = registry
        .catalog_prepare_fence(&request, preview, &fence_plan)
        .unwrap_err();
    assert!(rejected.is::<SourceChangeBusy>());
    assert_eq!(
        rejected.downcast_ref::<airk_core::catalog_fences::FencePrepareRejected>(),
        Some(&airk_core::catalog_fences::FencePrepareRejected::SourceBusy)
    );
    assert!(registry.catalog_pending_fence()?.is_none());
    assert_eq!(registry.job(pending.id)?.state, JobState::Pending);
    registry.cancel_pending(pending.id)?;
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(&request, preview, &fence_plan)?
    else {
        anyhow::bail!("fresh native fence required");
    };
    registry.catalog_record_fence_confirmations(
        request.caller,
        &request.key,
        intent.prepared_digest,
        &[],
    )?;
    // Exercise the reserved-apply boundary with accepted work becoming pending
    // after preparation. This adversarial SQL fixture is not normal admission.
    registry.connection.execute(
        "UPDATE service_jobs SET state='pending',outcome=NULL WHERE id=?1",
        [pending.id.to_string()],
    )?;
    let failure = registry
        .catalog_apply_fenced(&request, preview, intent.prepared_digest)
        .unwrap_err();
    assert!(failure.is::<SourceChangeBusy>());
    assert!(
        failure
            .downcast_ref::<airk_core::catalog_fences::FencePrepareRejected>()
            .is_none(),
        "an already reserved fence cannot be classified as a preinsert rejection"
    );
    assert!(registry.catalog_pending_fence()?.is_some());
    assert_eq!(registry.job(pending.id)?.state, JobState::Pending);
    registry.cancel_pending(pending.id)?;
    registry.catalog_apply_fenced(&request, preview, intent.prepared_digest)?;
    Ok(())
}

#[test]
fn removal_reuses_source_identities_at_the_seventeen_retained_source_bound() -> Result<()> {
    let (_temp, mut registry, mut request) = fixture()?;
    let CatalogOperation::BeginIndexSourceChange { id, plan, .. } = &mut request.operation else {
        unreachable!()
    };
    let owner = plan.sources[0].clone();
    let mut before = BTreeMap::new();
    let mut sources = vec![owner];
    for n in 0u8..16 {
        let alias = format!("r{n}");
        let path = Path::new(&plan.registration.root)
            .parent()
            .unwrap()
            .join(&alias)
            .to_str()
            .unwrap()
            .to_owned();
        let source = airk_core::index_catalog::SourceRootId::from_identity(
            Incarnation::from_bytes([n + 20; 16]),
        );
        registry.connection.execute(
            "INSERT INTO index_sources VALUES(?1,?2,?3,0,1)",
            params![source.to_string(), id.to_string(), path],
        )?;
        registry.connection.execute(
            "INSERT INTO index_source_aliases VALUES(?1,?2,?3)",
            params![id.to_string(), source.to_string(), alias],
        )?;
        before.insert(alias.clone(), path.clone());
        if n < 15 {
            sources.push(SourceRoot {
                id: source,
                canonical_path: path,
                aliases: vec![alias],
                owner: false,
            });
        }
    }
    plan.before.aliases = before.clone();
    before.remove("r15");
    plan.after.aliases = before;
    plan.sources = sources;
    plan.configuration.configuration = ProjectConfiguration::RemoveRoot {
        alias: "r15".into(),
    };
    request.validate()?;
    apply(&mut registry, &request)?;
    let count: i64 =
        registry
            .connection
            .query_row("SELECT count(*) FROM index_sources", [], |r| r.get(0))?;
    assert_eq!(count, 17);
    Ok(())
}
#[test]
fn linked_source_claim_and_completion_wait_for_independent_prepared_authority_fence() -> Result<()>
{
    let (_temp, mut registry, request) = fixture()?;
    let begin = apply(&mut registry, &request)?;
    let plan = source_plan(&request);
    let project =
        airk_core::index_catalog::QueryProjectId::from_identity(Incarnation::from_bytes([89; 16]));
    registry.connection.execute(
        "INSERT INTO query_projects VALUES(?1,'Other project',1,1,1)",
        [project.to_string()],
    )?;
    let unrelated = CatalogRequest {
        key: RequestKey {
            nonce: Incarnation::from_bytes([90; 16]),
            ..request.key.clone()
        },
        expected_catalog_revision: begin.metadata.catalog_revision,
        operation: CatalogOperation::DeleteProject {
            id: project,
            expected_definition_revision: CatalogRevision::INITIAL,
            expected_scope_revision: CatalogRevision::INITIAL,
            expected_defaults_revision: CatalogRevision::INITIAL,
        },
        ..request.clone()
    };
    let digest = registry.catalog_preview(&unrelated)?.identity()?;
    let FencePreparation::Prepared(intent) = registry.catalog_prepare_fence(
        &unrelated,
        digest,
        &FencePlan::new(Incarnation::from_bytes([91; 16]), 1, vec![])?,
    )?
    else {
        anyhow::bail!("expected fresh fence");
    };
    let mut lease = ProjectLease::acquire(&registry.root, plan.registration.binding)?;
    registry.admit_worker(&mut lease)?;
    assert!(registry.claim_job_id(&lease, plan.job).is_err());
    assert_eq!(registry.job(plan.job)?.state, JobState::Pending);
    registry.catalog_record_fence_confirmations(
        unrelated.caller,
        &unrelated.key,
        intent.prepared_digest,
        &[],
    )?;
    registry.catalog_apply_fenced(&unrelated, digest, intent.prepared_digest)?;
    registry
        .claim_job_id(&lease, plan.job)?
        .context("source claim did not resume")?;
    let actual = native(&request)?;
    let result = registry.qualify_catalog_source_result(
        &lease,
        plan.job,
        &actual,
        &serde_json::json!({"updated":true}),
    )?;
    registry.record_configuration_result(&lease, plan.job, &result)?;
    registry.complete_job(&lease, plan.job, JobOutcome::Succeeded { revision: None })?;
    registry.complete_catalog_source_change(begin.metadata.operation_id, &actual)?;
    Ok(())
}
#[test]
fn native_failed_desired_resolution_preserves_failed_job_and_all_prior_result_fields() -> Result<()>
{
    let (_temp, mut registry, mut request) = fixture()?;
    let CatalogOperation::BeginIndexSourceChange { plan, .. } = &mut request.operation else {
        unreachable!()
    };
    let data = Path::new(&plan.registration.data_root);
    let _bound = crate::service::data_scope::bind(data, &registry.root, request.key.namespace)?;
    let collection = crate::service::collection::Collection::initialize(
        data,
        &plan.registration.root,
        &plan.registration.collection,
        Incarnation::from_bytes([92; 16]),
    )?;
    let selection = collection.select()?;
    plan.configuration.incarnation = selection
        .store
        .service_incarnation()?
        .context("missing native collection incarnation")?;
    plan.configuration.settings =
        crate::service::collection::settings_digest(&selection.store, &plan.registration.root)?;
    let begin = apply(&mut registry, &request)?;
    let plan = source_plan(&request);
    let mut lease = ProjectLease::acquire(&registry.root, plan.registration.binding)?;
    registry.admit_worker(&mut lease)?;
    registry
        .claim_job_id(&lease, plan.job)?
        .context("source job not claimed")?;
    let writer = selection.store.lock()?;
    let actual = native(&request)?;
    assert!(
        NativeDesiredSourceResolution::from_validated_native_owner(
            &actual,
            &selection.store,
            &writer
        )
        .is_err()
    );
    let mut roots =
        crate::roots::RootConfig::load(selection.store.directory(), &plan.registration.root)?;
    roots.roots = plan.after.aliases.clone();
    selection
        .store
        .invalidate_owned_state_before_mutation(&writer)?;
    roots.save(selection.store.directory())?;
    let forged = serde_json::json!({"updated":true,(RESOLUTION_FIELD):{"schema":1}});
    assert!(
        registry
            .qualify_catalog_source_result(&lease, plan.job, &actual, &forged)
            .is_err()
    );
    assert!(
        registry
            .record_configuration_result(&lease, plan.job, &forged)
            .is_err()
    );
    let result = registry.qualify_catalog_source_result(
        &lease,
        plan.job,
        &actual,
        &serde_json::json!({"updated":true,"prior_detail":"preserved"}),
    )?;
    registry.record_configuration_result(&lease, plan.job, &result)?;
    let failed = registry.complete_job(
        &lease,
        plan.job,
        JobOutcome::Failed {
            code: "source_post_effect_failure".into(),
        },
    )?;
    assert!(
        registry
            .complete_catalog_source_change(begin.metadata.operation_id, &actual)
            .is_err()
    );
    let proof = NativeDesiredSourceResolution::from_validated_native_owner(
        &actual,
        &selection.store,
        &writer,
    )?;
    let receipt =
        registry.reconcile_failed_source_change(&lease, begin.metadata.operation_id, &proof)?;
    assert_eq!(
        serde_json::to_vec(&registry.job(plan.job)?)?,
        serde_json::to_vec(&failed)?
    );
    let saved = registry
        .configuration_result(plan.job)?
        .context("missing failed resolution result")?;
    assert_eq!(saved["prior_detail"], "preserved");
    assert!(saved.get(RESOLUTION_FIELD).is_some());
    let observation = registry.catalog_source_change_observation(begin.metadata.operation_id)?;
    assert_eq!(
        serde_json::to_vec(&observation.job)?,
        serde_json::to_vec(&Some(failed))?
    );
    assert_eq!(observation.completion, Some(receipt.clone()));
    assert_eq!(
        registry.reconcile_failed_source_change(&lease, begin.metadata.operation_id, &proof)?,
        receipt
    );
    let mut corrupt = receipt;
    corrupt.request_digest = Sha256Digest::from_bytes([0; 32]);
    registry.connection.execute(
        "UPDATE index_operation_receipts SET payload=?1 WHERE id=?2",
        params![
            serde_json::to_vec(&corrupt)?,
            corrupt.operation_id.to_string()
        ],
    )?;
    assert!(
        registry
            .catalog_source_change_observation(begin.metadata.operation_id)
            .is_err()
    );
    Ok(())
}

#[test]
fn binding_lookup_uses_complete_canonical_index_and_refuses_foreign_active_payload() -> Result<()> {
    use airk_core::catalog_management::{CatalogEntity, CatalogItem};
    let (_temp, mut registry, request) = fixture()?;
    let plan = source_plan(&request);
    let CatalogOperation::BeginIndexSourceChange { id, .. } = request.operation else {
        unreachable!()
    };
    let Some(CatalogItem::Index(expected)) = registry
        .catalog_detail(&CatalogEntity::Index { id }, None)?
        .item
    else {
        anyhow::bail!("index missing");
    };
    assert_eq!(
        registry.catalog_index_for_binding(plan.registration.binding)?,
        Some(expected)
    );
    assert!(
        registry
            .catalog_index_for_binding(Sha256Digest::from_bytes([93; 32]))?
            .is_none()
    );
    let foreign = ProjectRegistration::new(
        "/foreign/source".into(),
        "/foreign/data".into(),
        "foreign".into(),
    )?;
    registry.connection.execute(
        "UPDATE projects SET payload=?1 WHERE binding=?2",
        params![
            serde_json::to_vec(&foreign)?,
            plan.registration.binding.to_string()
        ],
    )?;
    assert!(
        registry
            .catalog_index_for_binding(plan.registration.binding)
            .is_err()
    );
    Ok(())
}
#[test]
fn real_p5_inactive_backfill_retirement_finalizes_sources_and_preserves_legacy_state() -> Result<()>
{
    use super::super::upgrade::{BackfillIndex, CatalogUpgrade, UpgradePlan};
    use airk_core::index_catalog::{QueryProjectId, SourceImportState, SourceRootId};
    let temp = tempfile::tempdir()?;
    let control = temp.path().canonicalize()?;
    let namespace = Incarnation::from_bytes([94; 16]);
    let registration = ProjectRegistration::new(
        "/offline/historical-source".into(),
        "/offline/historical-data".into(),
        "historical".into(),
    )?;
    let mut legacy = ServiceRegistry::open(&control, namespace)?;
    legacy.register(&registration)?;
    assert!(legacy.retire_missing_source(registration.binding, 0, 0)?);
    let legacy_digest = super::super::backup::legacy_data_digest(&legacy.connection)?;
    drop(legacy);
    let upgrade = CatalogUpgrade::acquire(&control)?;
    let index = IndexId::from_identity(Incarnation::from_bytes([95; 16]));
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([96; 16]));
    let plan = UpgradePlan {
        schema: 1,
        id: Incarnation::from_bytes([97; 16]),
        control: upgrade.control().into(),
        namespace: upgrade.namespace(),
        installer_identity: Sha256Digest::from_bytes([98; 32]),
        legacy_schema_digest: upgrade.source_schema_digest()?,
        registrations_digest: upgrade.registration_digest()?,
        indexes: vec![BackfillIndex {
            registration: registration.clone(),
            index_id: index,
            project_id: project,
            sources: vec![SourceRoot {
                id: SourceRootId::from_identity(Incarnation::from_bytes([99; 16])),
                canonical_path: registration.root.clone(),
                aliases: vec![],
                owner: true,
            }],
            source_import: SourceImportState::PendingMetadata,
            configuration_digest: None,
        }],
    };
    upgrade.admit(plan)?;
    upgrade.migrate()?;
    // Real held-owner backup/migration above. The remaining fixture enables
    // isolated native metadata tests; it does not qualify production activation.
    let mut db = Connection::open(control.join("service-1.4.sqlite3"))?;
    let tx = db.transaction()?;
    super::super::apply_authority_v1_layout(&tx)?;
    tx.execute(
        "UPDATE index_catalog_meta SET authority_contract=1,activated=1 WHERE id=1",
        [],
    )?;
    tx.commit()?;
    drop(db);
    let mut owner = super::super::super::owner_record(&control, true, namespace)?;
    owner.catalog_upgrade = None;
    owner.complete(&control)?;
    drop(upgrade);
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    assert_eq!(
        super::super::backup::legacy_data_digest(&registry.connection)?,
        legacy_digest
    );
    assert!(require_retired(&registry.connection, registration.binding).is_err());
    let request = CatalogRequest {
        caller: Incarnation::from_bytes([100; 16]),
        key: RequestKey {
            namespace,
            epoch: registry.saved_request_epoch()?.1,
            nonce: Incarnation::from_bytes([101; 16]),
        },
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation: CatalogOperation::RetireIndex {
            id: index,
            expected_definition_revision: CatalogRevision::INITIAL,
            expected_source_revision: CatalogRevision::INITIAL,
            reason: IndexRetirementReason::Explicit,
        },
    };
    let receipt = apply(&mut registry, &request)?;
    assert_eq!(
        super::super::backup::legacy_data_digest(&registry.connection)?,
        legacy_digest,
        "historical metadata retirement must preserve every legacy row/job/watch/cleanup byte"
    );
    let evidence = registry
        .catalog_index_retirement_evidence(index)?
        .context("historical native retirement proof missing")?;
    assert_eq!(evidence.retirement, receipt);
    evidence.validate()?;
    require_retired(&registry.connection, registration.binding)?;
    assert!(
        super::super::super::unregistration::retire(
            &registry.connection,
            registration.binding,
            None
        )
        .is_err(),
        "old already-unregistered behavior must remain an error"
    );
    let mut repeat = request.clone();
    repeat.key.nonce = Incarnation::from_bytes([102; 16]);
    repeat.expected_catalog_revision = receipt.metadata.catalog_revision;
    repeat.operation = CatalogOperation::RetireIndex {
        id: index,
        expected_definition_revision: CatalogRevision::INITIAL.next()?,
        expected_source_revision: CatalogRevision::INITIAL.next()?,
        reason: IndexRetirementReason::Explicit,
    };
    assert!(registry.catalog_preview(&repeat).is_err());
    let cleanup = ConfigurationRequest {
        prepare: false,
        incarnation: namespace,
        settings: registration.binding,
        configuration: ProjectConfiguration::PrepareDerivedCleanup,
    };
    let key = RequestKey {
        namespace,
        epoch: registry.saved_request_epoch()?.1,
        nonce: Incarnation::from_bytes([103; 16]),
    };
    let accepted = registry.enqueue_configuration(
        registration.binding,
        request.caller,
        &key,
        &cleanup,
        Sha256Digest::from_bytes([104; 32]),
    )?;
    assert_eq!(accepted.kind, JobKind::Configure);
    assert_eq!(accepted.state, JobState::Pending);
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_derived_worker(&mut lease)?;
    assert!(registry.claim_job_id(&lease, accepted.id)?.is_some());
    // No cleanup execution and no source/data filesystem operation occur.
    Ok(())
}
#[test]
fn nonhistorical_tombstone_cannot_adopt_inactive_backfill_finalization() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let plan = source_plan(&request);
    let CatalogOperation::BeginIndexSourceChange { id, .. } = request.operation else {
        unreachable!()
    };
    registry.connection.execute(
        "UPDATE projects SET registered=0,paused=1 WHERE binding=?1",
        [plan.registration.binding.to_string()],
    )?;
    registry.connection.execute(
        "UPDATE index_definitions SET tombstone=1 WHERE id=?1",
        [id.to_string()],
    )?;
    let retire = CatalogRequest {
        operation: CatalogOperation::RetireIndex {
            id,
            expected_definition_revision: CatalogRevision::INITIAL,
            expected_source_revision: CatalogRevision::INITIAL,
            reason: IndexRetirementReason::Explicit,
        },
        ..request.clone()
    };
    assert!(registry.catalog_preview(&retire).is_err());
    assert!(require_retired(&registry.connection, plan.registration.binding).is_err());
    Ok(())
}

#[test]
fn schema_eleven_configuration_claim_and_completion_never_query_catalog_tables() -> Result<()> {
    let root = tempfile::tempdir()?;
    let base = root.path().canonicalize()?;
    let control = base.join("control");
    std::fs::create_dir(&control)?;
    std::fs::create_dir(base.join("source"))?;
    let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([111; 16]))?;
    let registration = ProjectRegistration::new(
        base.join("source").to_str().unwrap().into(),
        base.join("data").to_str().unwrap().into(),
        "legacy-configure".into(),
    )?;
    registry.register(&registration)?;
    let tables: u32 = registry.connection.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE name='index_operation_receipts'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        tables, 0,
        "regression requires the actual legacy physical format"
    );
    let (namespace, epoch) = registry.request_epoch()?;
    let request = ConfigurationRequest {
        prepare: false,
        incarnation: namespace,
        settings: registration.binding,
        configuration: ProjectConfiguration::Usage {
            mode: airk_core::usage::RecordingMode::Disabled,
        },
    };
    let job = registry.enqueue_configuration(
        registration.binding,
        Incarnation::from_bytes([112; 16]),
        &RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([113; 16]),
        },
        &request,
        Sha256Digest::from_bytes([114; 32]),
    )?;
    let mut lease = ProjectLease::acquire(&control, registration.binding)?;
    registry.admit_worker(&mut lease)?;
    assert!(registry.claim_job_id(&lease, job.id)?.is_some());
    assert!(registry.catalog_source_work(&lease, job.id)?.is_none());
    let result = serde_json::json!({"legacy_configuration_applied": true});
    registry.record_configuration_result(&lease, job.id, &result)?;
    registry.complete_job(&lease, job.id, JobOutcome::Succeeded { revision: None })?;
    assert_eq!(registry.configuration_result(job.id)?, Some(result));
    Ok(())
}

#[test]
fn source_work_legacy_mode_requires_supported_activated_format_and_owner_admission() -> Result<()> {
    let (_root, registry, request) = fixture_for_authority(0)?;
    let plan = source_plan(&request);
    let lease = ProjectLease::acquire(&registry.root, plan.registration.binding)?;
    assert!(!live(&registry.connection)?);
    assert!(registry.catalog_source_work(&lease, plan.job)?.is_none());
    for mutation in [
        "UPDATE index_catalog_meta SET activated=0 WHERE id=1",
        "UPDATE index_catalog_meta SET authority_contract=2 WHERE id=1",
        "UPDATE index_catalog_meta SET policy_contract=1 WHERE id=1",
        "CREATE TABLE unexpected_source_guard(value)",
        "DROP TABLE index_operation_receipts",
        "PRAGMA user_version=10",
    ] {
        let (_root, registry, request) = fixture()?;
        let plan = source_plan(&request);
        let lease = ProjectLease::acquire(&registry.root, plan.registration.binding)?;
        registry.connection.execute_batch(mutation)?;
        assert!(
            registry.catalog_source_work(&lease, plan.job).is_err(),
            "{mutation}"
        );
    }
    let owner_path = registry.root.join("service-1.4.owner.json");
    let mut owner: serde_json::Value = serde_json::from_slice(&std::fs::read(&owner_path)?)?;
    // Simulate an exact native pending owner receipt without granting admission.
    owner["catalog_upgrade"] = serde_json::to_value(Incarnation::from_bytes([115; 16]))?;
    std::fs::write(owner_path, serde_json::to_vec(&owner)?)?;
    let error = registry
        .catalog_source_work(&lease, plan.job)
        .err()
        .context("pending owner admitted")?;
    assert!(error.is::<super::super::AdmissionPending>());
    Ok(())
}

fn retirement_reference_fixture(
    registry: &ServiceRegistry,
    request: &CatalogRequest,
) -> Result<(
    Vec<airk_core::index_catalog::QueryProjectId>,
    airk_core::index_catalog::ContextId,
    airk_core::index_catalog::GrantId,
)> {
    use airk_core::index_catalog::{ContextId, GrantId, QueryProjectId};
    let plan = source_plan(request);
    let index = plan.sources[0].id;
    let id = match &request.operation {
        CatalogOperation::BeginIndexSourceChange { id, .. } => *id,
        _ => unreachable!(),
    };
    let projects = vec![
        QueryProjectId::from_identity(Incarnation::from_bytes([21; 16])),
        QueryProjectId::from_identity(Incarnation::from_bytes([22; 16])),
    ];
    let context = ContextId::from_identity(Incarnation::from_bytes([23; 16]));
    let grant = GrantId::from_identity(Incarnation::from_bytes([24; 16]));
    registry.connection.execute(
        "INSERT INTO index_access_grants(id,host,revision,revoked) VALUES(?1,?2,1,0)",
        params![
            grant.to_string(),
            Sha256Digest::from_bytes([25; 32]).to_string()
        ],
    )?;
    for project in &projects {
        registry.connection.execute(
            "INSERT INTO query_projects VALUES(?1,'Reviewed Project',1,1,1)",
            [project.to_string()],
        )?;
        registry.connection.execute(
            "INSERT INTO project_index_memberships VALUES(?1,?2,1)",
            params![project.to_string(), id.to_string()],
        )?;
        registry.connection.execute(
            "INSERT INTO project_index_scopes VALUES(?1,?2,?3,'subtree','')",
            params![project.to_string(), id.to_string(), index.to_string()],
        )?;
        registry.connection.execute(
            "INSERT INTO project_default_indexes VALUES(?1,?2)",
            params![project.to_string(), id.to_string()],
        )?;
        registry.connection.execute(
            "INSERT INTO index_access_memberships VALUES(?1,?2,?3,1,1,0,NULL)",
            params![grant.to_string(), project.to_string(), id.to_string()],
        )?;
        registry.connection.execute(
            "INSERT INTO index_access_scopes VALUES(?1,?2,?3,?4,'subtree','')",
            params![
                grant.to_string(),
                project.to_string(),
                id.to_string(),
                index.to_string()
            ],
        )?;
    }
    registry.connection.execute(
        "INSERT INTO index_contexts VALUES(?1,?2,?3,'workspace','Reviewed context',1)",
        params![
            context.to_string(),
            Sha256Digest::from_bytes([26; 32]).to_string(),
            Sha256Digest::from_bytes([27; 32]).to_string()
        ],
    )?;
    for project in &projects {
        registry.connection.execute(
            "INSERT INTO context_projects VALUES(?1,?2)",
            params![context.to_string(), project.to_string()],
        )?;
    }
    registry.connection.execute(
        "INSERT INTO context_default_indexes VALUES(?1,?2)",
        params![context.to_string(), id.to_string()],
    )?;
    Ok((projects, context, grant))
}

#[test]
fn retirement_discloses_and_removes_current_references_without_rewriting_frozen_grants()
-> Result<()> {
    use airk_core::catalog_management::{CatalogEntity, CatalogItem};
    let (_owner, mut registry, begin) = fixture()?;
    let (projects, context, grant) = retirement_reference_fixture(&registry, &begin)?;
    let frozen = super::super::operations::read_persisted_grant(&registry.connection, grant)?;
    let mut request = begin.clone();
    let id = match &begin.operation {
        CatalogOperation::BeginIndexSourceChange { id, .. } => *id,
        _ => unreachable!(),
    };
    request.operation = CatalogOperation::RetireIndex {
        id,
        expected_definition_revision: CatalogRevision::INITIAL,
        expected_source_revision: CatalogRevision::INITIAL,
        reason: IndexRetirementReason::Explicit,
    };
    let preview = registry.catalog_preview(&request)?;
    let expected_revision = CatalogRevision::INITIAL.next()?;
    for project in &projects {
        assert!(preview.before.iter().any(|item| matches!(item,CatalogItem::Membership(member) if member.project_id==*project && member.index_id==id)));
        let after = preview
            .after
            .iter()
            .find_map(|item| match item {
                CatalogItem::Project(value) if value.id == *project => Some(value),
                _ => None,
            })
            .context("affected Project missing from review")?;
        assert!(after.default_indexes.is_empty());
        assert_eq!(after.scope_revision, CatalogRevision::INITIAL.next()?);
        assert_eq!(after.defaults_revision, CatalogRevision::INITIAL.next()?);
    }
    assert!(preview.before.iter().any(|item| matches!(item,CatalogItem::Context(value) if value.id==context && value.index_ids==vec![id])));
    assert!(preview.after.iter().any(|item| matches!(item,CatalogItem::Context(value) if value.id==context && value.index_ids.is_empty() && value.revision==expected_revision)));
    // Existing metadata-fixture fence helper; actual native drain/transport proof
    // remains the combined process regression, never inferred from this unit.
    apply(&mut registry, &request)?;
    require_no_retired_references(&registry.connection, id)?;
    assert_eq!(
        super::super::operations::read_persisted_grant(&registry.connection, grant)?,
        frozen
    );
    assert_eq!(
        registry.catalog_state()?.unwrap().revision,
        request.expected_catalog_revision.next()?
    );
    for project in projects {
        let detail = registry.catalog_detail(&CatalogEntity::Project { id: project }, None)?;
        assert!(
            matches!(detail.item,Some(CatalogItem::Project(value)) if value.default_indexes.is_empty())
        );
    }
    Ok(())
}

#[test]
fn retirement_reference_revision_exhaustion_refuses_complete_preview_without_effects() -> Result<()>
{
    let (_owner, mut registry, begin) = fixture()?;
    let (projects, _, grant) = retirement_reference_fixture(&registry, &begin)?;
    registry.connection.execute(
        "UPDATE query_projects SET scope_revision=9223372036854775807 WHERE id=?1",
        [projects[1].to_string()],
    )?;
    let frozen = super::super::operations::read_persisted_grant(&registry.connection, grant)?;
    let id = match &begin.operation {
        CatalogOperation::BeginIndexSourceChange { id, .. } => *id,
        _ => unreachable!(),
    };
    let request = CatalogRequest {
        operation: CatalogOperation::RetireIndex {
            id,
            expected_definition_revision: CatalogRevision::INITIAL,
            expected_source_revision: CatalogRevision::INITIAL,
            reason: IndexRetirementReason::Explicit,
        },
        ..begin
    };
    assert!(registry.catalog_preview(&request).is_err());
    assert_eq!(
        registry.connection.query_row(
            "SELECT count(*) FROM project_index_memberships WHERE index_id=?1",
            [id.to_string()],
            |row| row.get::<_, i64>(0)
        )?,
        2
    );
    assert!(!registry.connection.query_row(
        "SELECT tombstone FROM index_definitions WHERE id=?1",
        [id.to_string()],
        |row| row.get::<_, bool>(0)
    )?);
    assert_eq!(
        registry.connection.query_row(
            "SELECT scope_revision FROM query_projects WHERE id=?1",
            [projects[0].to_string()],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    assert_eq!(
        super::super::operations::read_persisted_grant(&registry.connection, grant)?,
        frozen
    );
    assert_eq!(
        registry.catalog_state()?.unwrap().revision,
        request.expected_catalog_revision
    );
    Ok(())
}
