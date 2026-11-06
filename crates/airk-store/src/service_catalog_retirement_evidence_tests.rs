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

#[test]
fn completed_native_purge_preserves_retirement_evidence_across_new_same_root_setup() -> Result<()> {
    let (temp, mut registry, original) = fixture()?;
    let request = retire_request(&mut registry, &original)?;
    let index = id(&request);
    let retired = apply(&mut registry, &request)?;
    let before = registry
        .catalog_index_retirement_evidence(index)?
        .context("pre-cleanup retirement proof missing")?;
    let backing = before.index.backing.clone();
    let source = temp.path().join("source").join("preserved.txt");
    std::fs::write(&source, b"source is not cleanup state")?;
    assert!(registry.catalog_policy_status(index)?.is_none());
    assert!(!std::path::Path::new(&backing.data_root).exists());

    // Actual owning cleanup, including its native ancestor-absence proof and
    // committed removed-project identity. No fabricated null FK or cleanup row.
    assert!(registry.purge_removed_project_turn(backing.binding)?);
    let pointer: Option<String> = registry.connection.query_row(
        "SELECT active_registration FROM index_definitions WHERE id=?1",
        [index.to_string()],
        |row| row.get(0),
    )?;
    assert_eq!(pointer, Some(backing.binding.to_string()));
    assert!(registry.pending_project_removals(None, 1)?.is_empty());
    let after = registry
        .catalog_index_retirement_evidence(index)?
        .context("completed cleanup lost canonical retirement proof")?;
    assert_eq!(before.index, after.index);
    assert_eq!(after.retirement, retired);
    assert_eq!(after.retirement, before.retirement);
    after.validate()?;
    assert_eq!(std::fs::read(&source)?, b"source is not cleanup state");
    assert!(!std::path::Path::new(&backing.data_root).exists());

    // Canonical native setup of a different backing on the same source clears
    // the mutable old suppression marker. The old exact retained identity must
    // still authenticate its unchanged full fence, without touching source bytes.
    use super::super::super::setup::storage::{NativeSetupPins, prepare_index_storage};
    use airk_core::catalog_setup::{IndexSetupRequest, SetupCompletion, SetupSource, SetupStage};
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let setup = IndexSetupRequest {
        caller: Incarnation::from_bytes([115; 16]),
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([116; 16]),
        },
        expected_catalog_revision: registry
            .catalog_state()?
            .context("catalog missing")?
            .revision,
        name: "New same-source index".into(),
        registration: ProjectRegistration::new(
            backing.root.clone(),
            temp.path()
                .canonicalize()?
                .join("new-data")
                .to_str()
                .unwrap()
                .into(),
            "replacement".into(),
        )?,
        sources: vec![SetupSource {
            canonical_path: backing.root.clone(),
            aliases: vec![],
            owner: true,
        }],
        setup_watch: false,
    };
    let reserved = registry.reserve_index_setup(&setup)?;
    assert!(!registry.project_was_removed(backing.binding)?);
    fn directory_pin(path: &std::path::Path) -> Result<std::fs::File> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
                FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE,
            };
            options
                .access_mode(FILE_TRAVERSE | FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
        }
        Ok(options.open(path)?)
    }
    let data = std::path::Path::new(&setup.registration.data_root);
    let pins = NativeSetupPins::from_native_review(
        directory_pin(data.parent().unwrap())?,
        None,
        vec![directory_pin(std::path::Path::new(&backing.root))?],
    );
    let control = registry.root.clone();
    let prepared =
        prepare_index_storage(&registry, &control, &reserved, pins)?.record(&mut registry)?;
    let ready = registry.complete_index_setup(
        setup.caller,
        &setup.key,
        prepared.request_digest,
        &SetupCompletion {
            prepared_digest: prepared.preparation.as_ref().unwrap().digest,
            expected_catalog_revision: prepared.catalog_revision,
            expected_source_revision: prepared.source_revision,
        },
    )?;
    assert_eq!(ready.stage, SetupStage::Ready);
    assert_eq!(
        registry.get(setup.registration.binding)?.registration,
        setup.registration
    );
    assert_eq!(
        registry
            .catalog_index_retirement_evidence(index)?
            .context("new setup lost old proof")?
            .retirement,
        retired
    );
    assert_eq!(std::fs::read(&source)?, b"source is not cleanup state");
    assert!(
        registry.register_with_setup(&backing, None).is_err(),
        "retained tombstone became registration authority"
    );
    Ok(())
}

#[test]
fn policy_retirement_cleanup_keeps_strict_retained_registration_evidence() -> Result<()> {
    use airk_core::{
        index_catalog::{DenyId, PathScope, ScopeAtom},
        source_policy::{PolicyChange, SourcePolicy},
    };
    let (_temp, mut registry, original) = fixture()?;
    let index = match &original.operation {
        CatalogOperation::BeginIndexSourceChange { id, .. } => *id,
        _ => unreachable!(),
    };
    let definition = super::super::super::operations::index_record(&registry.connection, index)?;
    let roots = super::super::super::policy::source_roots(&registry.connection, index)?;
    let selector = ScopeAtom {
        source_root_id: roots[0].id,
        kind: PathScope::Exact,
        path: "private.rs".into(),
    };
    let rule_id = DenyId::from_identity(Incarnation::from_bytes([112; 16]));
    let policy = SourcePolicy::initial_trim(&definition, roots, rule_id, selector.clone())?;
    let operation = catalog_operation_id(original.caller, &original.key)?;
    let change = CatalogRequest {
        operation: CatalogOperation::ChangeIndexPolicy {
            id: index,
            expected_source_revision: definition.source_revision,
            expected_policy_epoch: 0,
            expected_policy_digest: None,
            change: PolicyChange::Trim { rule_id, selector },
            engine_identity: Sha256Digest::from_bytes([113; 32]),
            configuration: Box::new(ConfigurationRequest {
                prepare: false,
                incarnation: original.key.namespace,
                settings: definition.backing.binding,
                configuration: ProjectConfiguration::ApplySourcePolicy {
                    policy: Box::new(policy),
                    operation_id: operation,
                },
            }),
        },
        ..original.clone()
    };
    let preview = registry.catalog_preview(&change)?.identity()?;
    let plan = FencePlan::new(Incarnation::from_bytes([114; 16]), 1, vec![])?
        .with_worker_obligation(airk_core::catalog_fences::WorkerDrainObligation {
            index_id: index,
            backing: definition.backing.binding,
            workers: vec![],
            providers: vec![],
        })?;
    let FencePreparation::Prepared(intent) =
        registry.catalog_prepare_fence(&change, preview, &plan)?
    else {
        anyhow::bail!("fresh policy fence required");
    };
    registry.catalog_record_fence_confirmations(
        change.caller,
        &change.key,
        intent.prepared_digest,
        &[],
    )?;
    registry.catalog_apply_fenced(&change, preview, intent.prepared_digest)?;
    let request = retire_request(&mut registry, &original)?;
    let retired = apply(&mut registry, &request)?;
    assert!(registry.purge_removed_project_turn(definition.backing.binding)?);
    let evidence = registry
        .catalog_index_retirement_evidence(index)?
        .context("retained policy proof missing")?;
    assert_eq!(evidence.retirement, retired);
    assert!(registry.catalog_policy_status(index)?.is_some());
    let pointer: Option<String> = registry.connection.query_row(
        "SELECT active_registration FROM index_definitions WHERE id=?1",
        [index.to_string()],
        |row| row.get(0),
    )?;
    assert_eq!(pointer, Some(definition.backing.binding.to_string()));
    // A cleanup record is not permission to ignore inconsistent retained state.
    registry.connection.execute(
        "UPDATE projects SET paused=0 WHERE binding=?1",
        [definition.backing.binding.to_string()],
    )?;
    assert!(registry.catalog_index_retirement_evidence(index).is_err());
    Ok(())
}

#[test]
fn historical_purged_registration_requires_exact_surviving_cleanup_marker() -> Result<()> {
    let (_temp, mut registry, original) = fixture()?;
    let request = retire_request(&mut registry, &original)?;
    let index = id(&request);
    let retired = apply(&mut registry, &request)?;
    let backing = registry
        .catalog_index_retirement_evidence(index)?
        .unwrap()
        .index
        .backing;
    assert!(registry.purge_removed_project_turn(backing.binding)?);
    // Model only the old implementation's final SQL deletion after actual purge;
    // the positive cleanup marker was produced by the real owning transaction.
    registry.connection.execute(
        "DELETE FROM projects WHERE binding=?1",
        [backing.binding.to_string()],
    )?;
    assert_eq!(
        registry
            .catalog_index_retirement_evidence(index)?
            .context("historical proof missing")?
            .retirement,
        retired
    );
    let removed: (String, i64) = registry.connection.query_row(
        "SELECT root_identity,removed_ms FROM service_removed_projects WHERE binding=?1",
        [backing.binding.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    registry.connection.execute(
        "DELETE FROM service_removed_projects WHERE binding=?1",
        [backing.binding.to_string()],
    )?;
    assert!(registry.catalog_index_retirement_evidence(index).is_err());
    registry.connection.execute(
        "INSERT INTO service_removed_projects VALUES(?1,?2,?3)",
        params![
            backing.binding.to_string(),
            Sha256Digest::from_bytes([111; 32]).to_string(),
            removed.1
        ],
    )?;
    assert!(registry.catalog_index_retirement_evidence(index).is_err());
    registry.connection.execute(
        "UPDATE service_removed_projects SET root_identity=?1 WHERE binding=?2",
        params![removed.0, backing.binding.to_string()],
    )?;
    assert_eq!(
        registry
            .catalog_index_retirement_evidence(index)?
            .unwrap()
            .retirement,
        retired
    );
    Ok(())
}
