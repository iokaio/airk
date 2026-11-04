// SPDX-License-Identifier: Apache-2.0
//! Component metadata tests; native filesystem proof remains separately owned.
use super::*;

fn index_id(request: &CatalogRequest) -> IndexId {
    let CatalogOperation::BeginIndexSourceChange { id, .. } = &request.operation else {
        unreachable!()
    };
    *id
}

#[test]
fn explicit_index_source_recovery_survives_restart_with_full_failed_job_and_result() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let index = index_id(&request);
    assert!(registry.catalog_pending_source_change(index)?.is_none());
    assert!(
        registry
            .catalog_pending_source_change(IndexId::from_identity(Incarnation::from_bytes(
                [100; 16]
            )))?
            .is_none()
    );
    let begin = apply(&mut registry, &request)?;
    let plan = source_plan(&request);
    let mut lease = ProjectLease::acquire(&registry.root, plan.registration.binding)?;
    registry.admit_worker(&mut lease)?;
    registry
        .claim_job_id(&lease, plan.job)?
        .context("source job missing")?;
    let actual = native(&request)?;
    let result = registry.qualify_catalog_source_result(
        &lease,
        plan.job,
        &actual,
        &serde_json::json!({"updated":true,"native_extra":{"preserved":true}}),
    )?;
    registry.record_configuration_result(&lease, plan.job, &result)?;
    registry.complete_job(
        &lease,
        plan.job,
        JobOutcome::Failed {
            code: "fixture_after_effect".into(),
        },
    )?;
    let expected = registry.catalog_source_change_observation(begin.metadata.operation_id)?;
    let root = registry.root.clone();
    drop(lease);
    drop(registry);
    let registry = ServiceRegistry::open(&root, request.key.namespace)?;
    let discovered = registry
        .catalog_pending_source_change(index)?
        .context("pending source work lost")?;
    assert_eq!(
        serde_json::to_value(&discovered)?,
        serde_json::to_value(&expected)?
    );
    assert_eq!(
        discovered.job.context("failed job missing")?.state,
        JobState::Failed
    );
    assert_eq!(discovered.configuration_result, Some(result));
    assert!(discovered.completion.is_none());
    Ok(())
}

#[test]
fn explicit_index_source_recovery_rejects_missing_wrong_kind_and_corrupt_begin() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let index = index_id(&request);
    let begin = apply(&mut registry, &request)?;
    let operation = begin.metadata.operation_id.to_string();
    registry.connection.execute(
        "UPDATE index_operation_receipts SET kind='wrong_source_lane' WHERE id=?1",
        [&operation],
    )?;
    assert!(
        registry.catalog_pending_source_change(index).is_err(),
        "unknown lineage is not absence"
    );
    registry.connection.execute("UPDATE index_operation_receipts SET kind='catalog_management_fenced',payload=X'7b7d' WHERE id=?1", [&operation])?;
    assert!(registry.catalog_pending_source_change(index).is_err());
    registry.connection.execute(
        "DELETE FROM index_operation_receipts WHERE id=?1",
        [&operation],
    )?;
    assert!(registry.catalog_pending_source_change(index).is_err());
    Ok(())
}

#[test]
fn explicit_index_source_recovery_refuses_ambiguous_current_revision_lineage() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let index = index_id(&request);
    let begin = apply(&mut registry, &request)?;
    // Deliberately inject a second structurally valid saved Begin at the same
    // source revision. Native discovery must refuse rather than choose one.
    let mut duplicate = begin.clone();
    duplicate.request.key.nonce = Incarnation::from_bytes([101; 16]);
    let CatalogOperation::BeginIndexSourceChange { plan, .. } = &mut duplicate.request.operation
    else {
        unreachable!()
    };
    plan.job = airk_core::catalog_source_change::source_job_id(
        plan.registration.binding,
        duplicate.request.caller,
        &duplicate.request.key,
    )?;
    duplicate.metadata.operation_id =
        catalog_operation_id(duplicate.request.caller, &duplicate.request.key)?;
    duplicate.metadata.key = duplicate.request.key.clone();
    duplicate.metadata.request_digest = duplicate.request.identity()?;
    let intent = airk_core::catalog_fences::FenceIntent {
        schema: 1,
        operation_id: duplicate.metadata.operation_id,
        request: duplicate.request.clone(),
        preview_digest: duplicate.preview_digest,
        prepared_digest: Sha256Digest::from_bytes([0; 32]),
        plan: airk_core::catalog_fences::FencePlan::new(
            duplicate.proof.service,
            duplicate.proof.gate_generation,
            vec![],
        )?,
        stage: airk_core::catalog_fences::FenceStage::Fenced,
        confirmations: vec![],
        worker_confirmations: vec![],
    };
    duplicate.prepared_digest = intent.identity()?;
    duplicate.validate()?;
    registry.connection.execute("INSERT INTO index_operation_receipts VALUES(?1,'catalog_management_fenced','metadata_applied',?2,?3)",
        params![duplicate.metadata.operation_id.to_string(),number(duplicate.metadata.catalog_revision),serde_json::to_vec(&duplicate)?])?;
    assert!(registry.catalog_pending_source_change(index).is_err());
    Ok(())
}

#[test]
fn pending_without_begin_is_absent_only_for_current_canonical_native_setup() -> Result<()> {
    use airk_core::catalog_setup::{IndexSetupRequest, SetupSource};
    let (temp, mut registry, request) = fixture()?;
    let primary = temp.path().canonicalize()?.join("setup-source");
    std::fs::create_dir(&primary)?;
    let registration = ProjectRegistration::new(
        primary.to_str().unwrap().into(),
        temp.path()
            .canonicalize()?
            .join("setup-data")
            .to_str()
            .unwrap()
            .into(),
        "setup-discovery".into(),
    )?;
    let setup = IndexSetupRequest {
        caller: request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([102; 16]),
            ..request.key.clone()
        },
        expected_catalog_revision: request.expected_catalog_revision,
        name: "Known pending setup".into(),
        sources: vec![SetupSource {
            canonical_path: registration.root.clone(),
            aliases: vec![],
            owner: true,
        }],
        registration,
        setup_watch: false,
    };
    let reserved = registry.reserve_index_setup(&setup)?;
    assert!(
        registry
            .catalog_pending_source_change(reserved.index_id)?
            .is_none()
    );
    registry.connection.execute(
        "UPDATE index_sources SET enabled=1 WHERE index_id=?1",
        [reserved.index_id.to_string()],
    )?;
    assert!(
        registry
            .catalog_pending_source_change(reserved.index_id)
            .is_err(),
        "known setup still requires exact disabled source metadata"
    );
    registry.connection.execute(
        "UPDATE index_definitions SET source_import_state='pending_metadata' WHERE id=?1",
        [index_id(&request).to_string()],
    )?;
    assert!(
        registry
            .catalog_pending_source_change(index_id(&request))
            .is_err(),
        "unexplained pending state is unknown, not absent"
    );
    Ok(())
}
