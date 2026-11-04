// SPDX-License-Identifier: Apache-2.0
use super::*;

fn retire_request(
    registry: &mut ServiceRegistry,
    original: &CatalogRequest,
) -> Result<CatalogRequest> {
    let CatalogOperation::BeginIndexSourceChange { id, .. } = &original.operation else {
        unreachable!()
    };
    let (_, definition, source, _, _) = registration(&registry.connection, *id)?;
    let (namespace, epoch) = registry.request_epoch()?;
    let mut request = original.clone();
    request.caller = Incarnation::from_bytes([105; 16]);
    request.key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([106; 16]),
    };
    request.expected_catalog_revision = super::super::super::state(&registry.connection)?.revision;
    request.operation = CatalogOperation::RetireIndex {
        id: *id,
        expected_definition_revision: definition,
        expected_source_revision: source,
        reason: IndexRetirementReason::Explicit,
    };
    Ok(request)
}

fn id(request: &CatalogRequest) -> IndexId {
    match &request.operation {
        CatalogOperation::RetireIndex { id, .. } => *id,
        _ => unreachable!(),
    }
}

#[test]
fn full_native_retirement_evidence_preserves_pending_source_state_and_current_clock() -> Result<()>
{
    let (_temp, mut registry, original) = fixture()?;
    apply(&mut registry, &original)?;
    let request = retire_request(&mut registry, &original)?;
    assert!(
        registry
            .catalog_index_retirement_evidence(id(&request))?
            .is_none()
    );
    let retired = apply(&mut registry, &request)?;
    // Native retirement invalidates unknown legacy keys by advancing the global
    // request epoch. A new independent mutation must obtain a fresh native key.
    let (namespace, epoch) = registry.request_epoch()?;
    assert!(epoch > request.key.epoch);
    let mut unrelated = request.clone();
    unrelated.key = RequestKey {
        namespace,
        epoch,
        nonce: Incarnation::from_bytes([107; 16]),
    };
    unrelated.expected_catalog_revision = retired.metadata.catalog_revision;
    unrelated.operation = CatalogOperation::CreateProject {
        id: airk_core::index_catalog::QueryProjectId::from_identity(Incarnation::from_bytes(
            [108; 16],
        )),
        name: "Current native clock".into(),
    };
    let preview = registry.catalog_preview(&unrelated)?.identity()?;
    let current = registry.catalog_apply(&unrelated, preview)?;
    let evidence = registry
        .catalog_index_retirement_evidence(id(&request))?
        .context("native proof missing")?;
    evidence.validate()?;
    assert_eq!(evidence.retirement, retired);
    assert_eq!(evidence.catalog_clock.revision, current.catalog_revision);
    assert_eq!(
        evidence.index.source_import,
        airk_core::index_catalog::SourceImportState::PendingMetadata
    );
    assert_ne!(
        evidence.retirement.request.caller, original.caller,
        "actual native caller need not be the source operation's UI caller"
    );
    assert!(
        registry
            .catalog_index_retirement_evidence(IndexId::from_identity(Incarnation::from_bytes(
                [109; 16]
            )))?
            .is_none()
    );
    Ok(())
}

#[test]
fn tombstone_alone_is_not_retirement_evidence_and_corrupt_or_stale_proof_refuses() -> Result<()> {
    let (_temp, mut registry, original) = fixture()?;
    let request = retire_request(&mut registry, &original)?;
    registry.connection.execute(
        "UPDATE index_definitions SET tombstone=1 WHERE id=?1",
        [id(&request).to_string()],
    )?;
    assert!(
        registry
            .catalog_index_retirement_evidence(id(&request))?
            .is_none()
    );
    registry.connection.execute(
        "UPDATE index_definitions SET tombstone=0 WHERE id=?1",
        [id(&request).to_string()],
    )?;
    let retired = apply(&mut registry, &request)?;
    registry.connection.execute(
        "UPDATE index_operation_receipts SET payload=X'7b7d' WHERE id=?1",
        [retired.metadata.operation_id.to_string()],
    )?;
    assert!(
        registry
            .catalog_index_retirement_evidence(id(&request))
            .is_err()
    );
    registry.connection.execute(
        "UPDATE index_operation_receipts SET payload=?1 WHERE id=?2",
        params![
            serde_json::to_vec(&retired)?,
            retired.metadata.operation_id.to_string()
        ],
    )?;
    registry.connection.execute(
        "UPDATE index_definitions SET source_revision=source_revision+1 WHERE id=?1",
        [id(&request).to_string()],
    )?;
    assert!(
        registry
            .catalog_index_retirement_evidence(id(&request))
            .is_err()
    );
    registry.connection.execute(
        "UPDATE index_definitions SET source_revision=source_revision-1 WHERE id=?1",
        [id(&request).to_string()],
    )?;
    registry.connection.execute(
        "UPDATE index_sources SET enabled=1 WHERE index_id=?1",
        [id(&request).to_string()],
    )?;
    assert!(
        registry
            .catalog_index_retirement_evidence(id(&request))
            .is_err()
    );
    registry.connection.execute(
        "UPDATE index_sources SET enabled=0 WHERE index_id=?1",
        [id(&request).to_string()],
    )?;
    registry.connection.execute(
        "UPDATE index_definitions SET active_registration=NULL WHERE id=?1",
        [id(&request).to_string()],
    )?;
    assert!(
        registry
            .catalog_index_retirement_evidence(id(&request))
            .is_err()
    );
    Ok(())
}

#[test]
fn native_retirement_evidence_rejects_ambiguous_full_saved_lineage() -> Result<()> {
    let (_temp, mut registry, original) = fixture()?;
    let request = retire_request(&mut registry, &original)?;
    let mut duplicate = apply(&mut registry, &request)?;
    duplicate.request.key.nonce = Incarnation::from_bytes([110; 16]);
    duplicate.metadata.operation_id =
        catalog_operation_id(duplicate.request.caller, &duplicate.request.key)?;
    duplicate.metadata.key = duplicate.request.key.clone();
    duplicate.metadata.request_digest = duplicate.request.identity()?;
    let plan = FencePlan::new(
        duplicate.proof.service,
        duplicate.proof.gate_generation,
        vec![],
    )?
    .with_worker_obligation(
        duplicate
            .proof
            .worker_obligation
            .clone()
            .context("native census missing")?,
    )?;
    let intent = airk_core::catalog_fences::FenceIntent {
        schema: 1,
        operation_id: duplicate.metadata.operation_id,
        request: duplicate.request.clone(),
        preview_digest: duplicate.preview_digest,
        prepared_digest: Sha256Digest::from_bytes([0; 32]),
        plan,
        stage: airk_core::catalog_fences::FenceStage::Fenced,
        confirmations: vec![],
        worker_confirmations: vec![],
    };
    duplicate.prepared_digest = intent.identity()?;
    duplicate.validate()?;
    registry.connection.execute("INSERT INTO index_operation_receipts VALUES(?1,'catalog_management_fenced','metadata_applied',?2,?3)",
        params![duplicate.metadata.operation_id.to_string(),number(duplicate.metadata.catalog_revision),serde_json::to_vec(&duplicate)?])?;
    assert!(
        registry
            .catalog_index_retirement_evidence(id(&request))
            .is_err()
    );
    Ok(())
}
