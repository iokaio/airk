// SPDX-License-Identifier: Apache-2.0
//! Checkpoint observation of a prepared adoption reservation retained from an
//! earlier release. Explicit native-proof stand-ins; no adoption writer exists.
use super::*;
use airk_core::catalog_legacy_adoption::source_inventory_digest;

fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, LegacyAdoptionPlan)> {
    let (temp, mut registry, mut setup) = super::super::tests::fixture()?;
    // This fixture qualifies metadata admission only. Match the complete
    // production authority layout before changing its marker; a marker alone
    // cannot turn the authority-zero physical schema into authority one.
    let tx = registry.connection.transaction()?;
    super::super::super::apply_authority_v1_layout(&tx)?;
    tx.execute(
        "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
        [],
    )?;
    tx.commit()?;
    setup.setup_watch = false;
    let original_registration = ProjectRegistration::new(
        setup.registration.root.clone(),
        temp.path()
            .canonicalize()?
            .join("original-data")
            .to_str()
            .unwrap()
            .into(),
        setup.registration.collection.clone(),
    )?;
    let aliases = setup
        .sources
        .iter()
        .flat_map(|source| {
            source
                .aliases
                .iter()
                .map(|alias| (alias.clone(), source.canonical_path.clone()))
        })
        .collect();
    let digest = Sha256Digest::from_bytes([81; 32]);
    let source_inventory_sha256 = source_inventory_digest(
        &original_registration.root,
        &original_registration.collection,
        &aliases,
    )?;
    let plan = LegacyAdoptionPlan {
        schema: 1,
        setup,
        original_registration,
        origin_sha256: digest,
        config_sha256: digest,
        executable_sha256: digest,
        quiescence_sha256: digest,
        archive_inventory_sha256: digest,
        original_publication_sha256: digest,
        source_inventory_sha256,
    };
    plan.validate()?;
    Ok((temp, registry, plan))
}
fn no_background_or_authority(registry: &ServiceRegistry) -> Result<()> {
    let counts:(i64,i64,i64,i64,i64,i64)=registry.connection.query_row("SELECT (SELECT count(*) FROM service_watch),(SELECT count(*) FROM service_background),(SELECT count(*) FROM service_setup_pending),(SELECT count(*) FROM service_jobs),(SELECT count(*) FROM query_projects),(SELECT count(*) FROM index_access_grants)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)))?;
    assert_eq!(counts, (0, 0, 0, 0, 0, 0));
    Ok(())
}

// Real native metadata encodings, but no claim of a converted Current head.
fn checkpoint_capsule(
    legacy: bool,
) -> Result<(tempfile::TempDir, ServiceRegistry, LegacyAdoptionPlan)> {
    let (temp, mut registry, plan) = fixture()?;
    let reserved = if legacy {
        registry.reserve_setup_inner(&plan.setup, Some(plan.identity()?))?
    } else {
        registry.reserve_index_setup(&plan.setup)?
    };
    let control = registry.root.canonicalize()?;
    let metadata = super::super::metadata::Metadata::format_request(&control, &plan.setup, legacy)?;
    let storage_identity = framed_digest(
        b"airk/catalog-native-setup-storage/1",
        &[
            control.to_str().unwrap().as_bytes(),
            plan.setup.key.namespace.as_bytes(),
            plan.setup.registration.binding.as_bytes(),
            reserved.operation_id.as_bytes(),
        ],
    );
    let mut native = NativeIndexPreparation::from_validated_native_review(
        plan.setup.registration.clone(),
        plan.setup.sources.clone(),
        storage_identity,
        metadata.configuration,
        SetupPreparationReview {
            expected_catalog_revision: reserved.catalog_revision,
            expected_source_revision: reserved.source_revision,
        },
    )?;
    native.legacy_adoption = legacy.then(|| plan.identity()).transpose()?;
    registry.record_index_preparation(
        plan.setup.caller,
        &plan.setup.key,
        plan.setup.identity()?,
        &native,
    )?;
    let data = Path::new(&plan.setup.registration.data_root);
    let logical = data
        .join("collections-1.4")
        .join(&plan.setup.registration.collection);
    let slot = logical.join("incarnations").join(&metadata.slot);
    std::fs::create_dir_all(&slot)?;
    std::fs::write(
        data.join(crate::service::data_scope::OWNER),
        &metadata.owner,
    )?;
    for (name, bytes) in metadata.collection_files() {
        std::fs::write(logical.join(name), bytes)?;
    }
    std::fs::write(slot.join("roots.json"), &metadata.roots)?;
    Ok((temp, registry, plan))
}

#[test]
fn checkpoint_exact_prepared_adoption_without_current_gate_is_not_initialized() -> Result<()> {
    let (_temp, registry, plan) = checkpoint_capsule(true)?;
    let data = Path::new(&plan.setup.registration.data_root);
    let gate = data
        .join("collections-1.4")
        .join(&plan.setup.registration.collection)
        .join(".current-gates");
    assert!(!gate.exists());
    let first = crate::service::registry::checkpoint::snapshot(&registry.root)?;
    let publication = first
        .publications
        .iter()
        .find(|p| p.binding == plan.setup.registration.binding)
        .unwrap();
    assert!(matches!(
        publication.publication,
        crate::service::registry::checkpoint::PublicationState::NotInitialized
    ));
    assert!(
        first
            .indexes
            .iter()
            .any(|i| i.binding == plan.setup.registration.binding
                && i.source_import_state == "pending_metadata")
    );
    let second = crate::service::registry::checkpoint::snapshot(&registry.root)?;
    assert_eq!(first.digest()?, second.digest()?);
    assert!(!gate.exists(), "observation must not create a Current gate");
    no_background_or_authority(&registry)?;
    Ok(())
}

#[test]
fn checkpoint_ordinary_preparation_never_gains_adoption_empty_storage_exception() -> Result<()> {
    let (_temp, registry, _plan) = checkpoint_capsule(false)?;
    assert!(crate::service::registry::checkpoint::snapshot(&registry.root).is_err());
    Ok(())
}

#[test]
fn checkpoint_prepared_adoption_refuses_changed_capsule_and_unknown_slot_files() -> Result<()> {
    for name in [
        "roots.json",
        "unexpected.bin",
        "current.sqlite3",
        "current-owner.json",
    ] {
        let (_temp, registry, plan) = checkpoint_capsule(true)?;
        let receipt = registry
            .index_setup_receipt(plan.setup.caller, &plan.setup.key)?
            .unwrap();
        let slot = Path::new(&plan.setup.registration.data_root)
            .join("collections-1.4")
            .join(&plan.setup.registration.collection)
            .join("incarnations")
            .join(receipt.operation_id.to_string());
        std::fs::write(slot.join(name), b"changed or incomplete publication")?;
        assert!(
            crate::service::registry::checkpoint::snapshot(&registry.root).is_err(),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn checkpoint_prepared_adoption_refuses_crossed_native_storage_proof() -> Result<()> {
    let (_temp, registry, plan) = checkpoint_capsule(true)?;
    let mut receipt = registry
        .index_setup_receipt(plan.setup.caller, &plan.setup.key)?
        .unwrap();
    receipt.preparation.as_mut().unwrap().storage_identity = Sha256Digest::from_bytes([0; 32]);
    registry.connection.execute(
        "UPDATE index_operation_receipts SET payload=?1 WHERE id=?2",
        params![
            serde_json::to_vec(&receipt)?,
            receipt.operation_id.to_string()
        ],
    )?;
    assert!(crate::service::registry::checkpoint::snapshot(&registry.root).is_err());
    Ok(())
}
