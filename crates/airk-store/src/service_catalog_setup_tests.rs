// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::service::{ServiceLease, registry::ServiceRegistry};
use airk_core::index_catalog::IndexId;

fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, IndexSetupRequest)> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    for name in ["control", "source", "alias", "other"] {
        std::fs::create_dir(base.join(name))?;
    }
    let control = base.join("control");
    let mut genesis = super::super::genesis::CatalogGenesis::begin(
        ServiceLease::acquire(&control)?,
        Sha256Digest::from_bytes([71; 32]),
        Incarnation::from_bytes([72; 16]),
    )?;
    genesis.backup()?;
    let receipt = genesis.migrate()?;
    // This fixture enables only isolated Store metadata tests. It does not
    // qualify or invoke production activation/readiness or native preparation.
    let db = Connection::open(control.join("service-1.4.sqlite3"))?;
    db.execute("UPDATE index_catalog_meta SET activated=1 WHERE id=1", [])?;
    drop(db);
    let mut owner = super::super::super::owner_record(&control, true, receipt.namespace)?;
    owner.catalog_upgrade = None;
    owner.complete(&control)?;
    drop(genesis);
    let registry = ServiceRegistry::open(&control, receipt.namespace)?;
    let request = IndexSetupRequest {
        caller: Incarnation::from_bytes([73; 16]),
        key: RequestKey {
            namespace: receipt.namespace,
            epoch: 0,
            nonce: Incarnation::from_bytes([74; 16]),
        },
        expected_catalog_revision: CatalogRevision::INITIAL,
        name: "Reviewed index".into(),
        registration: ProjectRegistration::new(
            base.join("source").to_str().unwrap().into(),
            base.join("data").to_str().unwrap().into(),
            "fixture".into(),
        )?,
        sources: vec![
            SetupSource {
                canonical_path: base.join("source").to_str().unwrap().into(),
                aliases: vec![],
                owner: true,
            },
            SetupSource {
                canonical_path: base.join("alias").to_str().unwrap().into(),
                aliases: vec!["support".into()],
                owner: false,
            },
        ],
        setup_watch: true,
    };
    Ok((temp, registry, request))
}
fn native(
    receipt: &IndexSetupReceipt,
    revision: CatalogRevision,
) -> Result<NativeIndexPreparation> {
    // Trusted adapter stand-in: these unit tests verify metadata admission only.
    // The CLI integration must validate actual storage/source identities first.
    NativeIndexPreparation::from_validated_native_review(
        receipt.request.registration.clone(),
        receipt.request.sources.clone(),
        Sha256Digest::from_bytes([75; 32]),
        Sha256Digest::from_bytes([76; 32]),
        SetupPreparationReview {
            expected_catalog_revision: revision,
            expected_source_revision: receipt.source_revision,
        },
    )
}
fn no_work(registry: &ServiceRegistry) -> Result<()> {
    let counts:(i64,i64,i64,i64,i64) = registry.connection.query_row("SELECT (SELECT count(*) FROM service_watch),(SELECT count(*) FROM service_background),(SELECT count(*) FROM service_setup_pending),(SELECT count(*) FROM service_jobs),(SELECT count(*) FROM query_projects)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
    assert_eq!(counts, (0, 0, 0, 0, 0));
    Ok(())
}
fn completion(receipt: &IndexSetupReceipt, revision: CatalogRevision) -> SetupCompletion {
    SetupCompletion {
        prepared_digest: receipt.preparation.as_ref().unwrap().digest,
        expected_catalog_revision: revision,
        expected_source_revision: receipt.source_revision,
    }
}

#[test]
fn reservation_and_native_preparation_remain_pending_until_exact_once_ready() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let reserved = registry.reserve_index_setup(&request)?;
    assert_eq!(reserved.stage, SetupStage::Reserved);
    assert_eq!(
        registry.get(request.registration.binding)?.registration,
        request.registration
    );
    assert!(registry.index_setup_pending(request.registration.binding)?);
    assert_eq!(reserved.source_ids.len(), request.sources.len());
    assert_ne!(reserved.source_ids[0], reserved.source_ids[1]);
    no_work(&registry)?;
    assert!(
        !Path::new(&request.registration.data_root).exists(),
        "Store reservation must not create storage"
    );
    assert!(
        registry
            .register_with_setup(&request.registration, Some(true))
            .is_err()
    );
    assert!(
        registry
            .register_missing_with_setup(&request.registration, Some(true))
            .is_err()
    );
    no_work(&registry)?;
    let proof = native(&reserved, reserved.catalog_revision)?;
    let prepared = registry.record_index_preparation(
        request.caller,
        &request.key,
        reserved.request_digest,
        &proof,
    )?;
    assert_eq!(prepared.stage, SetupStage::Prepared);
    assert!(registry.index_setup_pending(request.registration.binding)?);
    no_work(&registry)?;
    let finish = completion(&prepared, prepared.catalog_revision);
    let ready = registry.complete_index_setup(
        request.caller,
        &request.key,
        reserved.request_digest,
        &finish,
    )?;
    assert_eq!(ready.stage, SetupStage::Ready);
    assert_eq!(ready.source_revision, CatalogRevision::INITIAL.next()?);
    assert!(!registry.index_setup_pending(request.registration.binding)?);
    let state:(bool,i64,i64,bool,i64)=registry.connection.query_row("SELECT w.enabled,w.configuration_epoch,w.requested,b.source_pending,(SELECT count(*) FROM service_setup_pending) FROM service_watch w JOIN service_background b ON b.project=w.project",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
    assert_eq!(state, (true, 1, 1, true, 1));
    assert_eq!(
        registry.complete_index_setup(
            request.caller,
            &request.key,
            reserved.request_digest,
            &finish
        )?,
        ready
    );
    assert_eq!(registry.reserve_index_setup(&request)?, ready);
    assert_eq!(
        registry.record_index_preparation(
            request.caller,
            &request.key,
            reserved.request_digest,
            &proof
        )?,
        ready
    );
    let after: (i64, i64) = registry.connection.query_row(
        "SELECT configuration_epoch,requested FROM service_watch",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(after, (1, 1), "replay must not schedule another setup");
    assert_eq!(
        registry.connection.query_row(
            "SELECT count(*) FROM project_index_memberships",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        0
    );
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_access_grants", [], |row| row
                .get::<_, i64>(0))?,
        0
    );
    Ok(())
}

#[test]
fn accepted_setup_replays_before_epoch_and_revision_but_reused_body_refuses() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let receipt = registry.reserve_index_setup(&request)?;
    registry.connection.execute(
        "UPDATE service_meta SET request_epoch=request_epoch+1 WHERE id=1",
        [],
    )?;
    registry.connection.execute(
        "UPDATE index_catalog_meta SET revision=revision+1 WHERE id=1",
        [],
    )?;
    assert_eq!(registry.reserve_index_setup(&request)?, receipt);
    assert_eq!(
        registry.index_setup_receipt(request.caller, &request.key)?,
        Some(receipt.clone())
    );
    let mut changed = request.clone();
    changed.name = "another reviewed name".into();
    assert!(registry.reserve_index_setup(&changed).is_err());
    let mut new = request.clone();
    new.key.nonce = Incarnation::from_bytes([77; 16]);
    assert!(registry.reserve_index_setup(&new).is_err());
    let revision = registry.catalog_state()?.unwrap().revision;
    let proof = native(&receipt, revision)?;
    let prepared = registry.record_index_preparation(
        request.caller,
        &request.key,
        receipt.request_digest,
        &proof,
    )?;
    let finish = completion(&prepared, revision);
    let ready = registry.complete_index_setup(
        request.caller,
        &request.key,
        receipt.request_digest,
        &finish,
    )?;
    registry.connection.execute(
        "UPDATE index_catalog_meta SET revision=revision+1 WHERE id=1",
        [],
    )?;
    assert_eq!(
        registry.complete_index_setup(
            request.caller,
            &request.key,
            receipt.request_digest,
            &finish
        )?,
        ready
    );
    let mut different = finish.clone();
    different.expected_catalog_revision = different.expected_catalog_revision.next()?;
    assert!(
        registry
            .complete_index_setup(
                request.caller,
                &request.key,
                receipt.request_digest,
                &different
            )
            .is_err()
    );
    Ok(())
}

#[test]
fn live_authority_refuses_legacy_only_registration_and_keeps_ready_reopen() -> Result<()> {
    let (temp, mut registry, request) = fixture()?;
    let reserved = registry.reserve_index_setup(&request)?;
    let proof = native(&reserved, reserved.catalog_revision)?;
    let prepared = registry.record_index_preparation(
        request.caller,
        &request.key,
        reserved.request_digest,
        &proof,
    )?;
    registry.complete_index_setup(
        request.caller,
        &request.key,
        reserved.request_digest,
        &completion(&prepared, prepared.catalog_revision),
    )?;
    // Exercise the private registration admission guard without advertising or
    // opening an unsupported production contract in this inactive F source slice.
    registry.connection.execute(
        "UPDATE index_catalog_meta SET authority_contract=1 WHERE id=1",
        [],
    )?;
    let base = temp.path().canonicalize()?;
    let other = ProjectRegistration::new(
        base.join("other").to_str().unwrap().into(),
        base.join("other-data").to_str().unwrap().into(),
        "other".into(),
    )?;
    assert!(registry.register(&other).is_err());
    assert!(registry.register_with_setup(&other, Some(true)).is_err());
    assert!(
        registry
            .register_missing_with_setup(&other, Some(true))
            .is_err()
    );
    assert!(!registry.is_registered(other.binding)?);
    registry.register(&request.registration)?;
    registry.register_with_setup(&request.registration, Some(true))?;
    assert_eq!(
        registry.connection.query_row(
            "SELECT configuration_epoch FROM service_watch",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        1
    );
    registry.connection.execute(
        "UPDATE index_definitions SET tombstone=1 WHERE id=?1",
        [reserved.index_id.to_string()],
    )?;
    assert!(registry.register(&request.registration).is_err());
    registry.connection.execute("UPDATE index_definitions SET tombstone=0,source_import_state='pending_metadata' WHERE id=?1",[reserved.index_id.to_string()])?;
    assert!(registry.register(&request.registration).is_err());
    registry.connection.execute("UPDATE index_definitions SET source_import_state='complete',active_registration=NULL WHERE id=?1",[reserved.index_id.to_string()])?;
    assert!(registry.register(&request.registration).is_err());
    Ok(())
}

#[test]
fn pending_aliases_and_retained_backings_fence_legacy_reservations() -> Result<()> {
    let (temp, mut registry, request) = fixture()?;
    let reserved = registry.reserve_index_setup(&request)?;
    no_work(&registry)?;
    let base = temp.path().canonicalize()?;
    let alias_data = ProjectRegistration::new(
        base.join("other").to_str().unwrap().into(),
        base.join("alias/new-data").to_str().unwrap().into(),
        "overlap".into(),
    )?;
    assert!(registry.register(&alias_data).is_err());
    assert!(!base.join("alias/new-data").exists());
    assert!(!registry.is_registered(alias_data.binding)?);
    // A published/native watch roster cannot remove the independent reservation.
    registry.connection.execute("INSERT INTO service_watch(project,enabled,requested,indexed,configuration_epoch,roots,roots_observed,native_roots,backend,attempts,next_ms) VALUES(?1,0,0,0,0,x'5b5d',0,0,'disabled',0,0)",[request.registration.binding.to_string()])?;
    assert!(registry.register(&alias_data).is_err());
    registry.connection.execute(
        "DELETE FROM service_watch WHERE project=?1",
        [request.registration.binding.to_string()],
    )?;
    registry.connection.execute("PRAGMA foreign_keys=ON", [])?;
    registry.connection.execute(
        "DELETE FROM projects WHERE binding=?1",
        [request.registration.binding.to_string()],
    )?;
    let source_in_retained_data = ProjectRegistration::new(
        base.join("data/another-source").to_str().unwrap().into(),
        base.join("other-data").to_str().unwrap().into(),
        "retained".into(),
    )?;
    assert!(registry.register(&source_in_retained_data).is_err());
    let same_backing = ProjectRegistration::new(
        base.join("other").to_str().unwrap().into(),
        request.registration.data_root.clone(),
        request.registration.collection.clone(),
    )?;
    assert!(registry.register(&same_backing).is_err());
    let proof = native(&reserved, reserved.catalog_revision)?;
    assert!(
        registry
            .record_index_preparation(
                request.caller,
                &request.key,
                reserved.request_digest,
                &proof
            )
            .is_err(),
        "preparation cannot resurrect removed registration"
    );
    Ok(())
}

#[test]
fn stale_or_different_native_proof_never_makes_pending_metadata_ready() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let reserved = registry.reserve_index_setup(&request)?;
    let mut wrong = native(&reserved, reserved.catalog_revision)?;
    wrong.sources[1].aliases = vec!["different".into()];
    assert!(
        registry
            .record_index_preparation(
                request.caller,
                &request.key,
                reserved.request_digest,
                &wrong
            )
            .is_err()
    );
    let mut stale = native(&reserved, CatalogRevision::INITIAL)?;
    assert!(
        registry
            .record_index_preparation(
                request.caller,
                &request.key,
                reserved.request_digest,
                &stale
            )
            .is_err()
    );
    stale.review.expected_catalog_revision = reserved.catalog_revision;
    let prepared = registry.record_index_preparation(
        request.caller,
        &request.key,
        reserved.request_digest,
        &stale,
    )?;
    wrong = native(&reserved, reserved.catalog_revision)?;
    wrong.storage_identity = Sha256Digest::from_bytes([78; 32]);
    assert!(
        registry
            .record_index_preparation(
                request.caller,
                &request.key,
                reserved.request_digest,
                &wrong
            )
            .is_err()
    );
    let mut finish = completion(&prepared, prepared.catalog_revision);
    finish.expected_source_revision = finish.expected_source_revision.next()?;
    assert!(
        registry
            .complete_index_setup(
                request.caller,
                &request.key,
                reserved.request_digest,
                &finish
            )
            .is_err()
    );
    finish.expected_source_revision = reserved.source_revision;
    registry.connection.execute(
        "DELETE FROM index_source_aliases WHERE index_id=?1",
        [reserved.index_id.to_string()],
    )?;
    assert!(
        registry
            .complete_index_setup(
                request.caller,
                &request.key,
                reserved.request_digest,
                &finish
            )
            .is_err()
    );
    assert!(registry.index_setup_pending(request.registration.binding)?);
    no_work(&registry)?;
    Ok(())
}

#[test]
fn full_receipt_capacity_refuses_before_any_registration_or_index() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    for n in 1..=1024u128 {
        registry.connection.execute(
            "INSERT INTO index_operation_receipts VALUES(?1,'fixture','saved',1,x'7b7d')",
            [Incarnation::from_bytes(n.to_be_bytes()).to_string()],
        )?;
    }
    assert!(registry.reserve_index_setup(&request).is_err());
    assert!(!registry.is_registered(request.registration.binding)?);
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_definitions", [], |row| row
                .get::<_, i64>(0))?,
        0
    );
    no_work(&registry)?;
    Ok(())
}

#[test]
fn index_capacity_failure_rolls_back_accepted_receipt_and_legacy_registration() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    for n in 1..=1024u128 {
        let index = IndexId::from_identity(Incarnation::from_bytes(n.to_be_bytes()));
        let registration = ProjectRegistration::new(
            format!("/fictional/source-{n}"),
            format!("/fictional/data-{n}"),
            "fixture".into(),
        )?;
        registry.connection.execute("INSERT INTO index_definitions VALUES(?1,?2,NULL,?3,?4,'collections-1.4','fixture','fixture',1,1,0,NULL,0,'pending_metadata')",params![index.to_string(),registration.binding.to_string(),registration.root,registration.data_root])?;
    }
    assert!(registry.reserve_index_setup(&request).is_err());
    assert!(!registry.is_registered(request.registration.binding)?);
    assert!(
        registry
            .index_setup_receipt(request.caller, &request.key)?
            .is_none()
    );
    assert_eq!(
        registry.connection.query_row(
            "SELECT count(*) FROM index_operation_receipts",
            [],
            |row| row.get::<_, i64>(0)
        )?,
        0
    );
    no_work(&registry)?;
    Ok(())
}

#[test]
fn pending_native_fence_blocks_new_setup_stages_but_not_exact_receipt_replay() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let reserved = registry.reserve_index_setup(&request)?;
    let id = Incarnation::from_bytes([79; 16]);
    registry.connection.execute("INSERT INTO index_operation_receipts VALUES(?1,'catalog_management_fenced','prepared',1,x'7b7d')",[id.to_string()])?;
    assert_eq!(registry.reserve_index_setup(&request)?, reserved);
    let proof = native(&reserved, reserved.catalog_revision)?;
    assert!(
        registry
            .record_index_preparation(
                request.caller,
                &request.key,
                reserved.request_digest,
                &proof
            )
            .is_err()
    );
    no_work(&registry)?;
    registry.connection.execute(
        "DELETE FROM index_operation_receipts WHERE id=?1",
        [id.to_string()],
    )?;
    let prepared = registry.record_index_preparation(
        request.caller,
        &request.key,
        reserved.request_digest,
        &proof,
    )?;
    registry.connection.execute("INSERT INTO index_operation_receipts VALUES(?1,'catalog_management_fenced','prepared',1,x'7b7d')",[id.to_string()])?;
    assert!(
        registry
            .complete_index_setup(
                request.caller,
                &request.key,
                reserved.request_digest,
                &completion(&prepared, prepared.catalog_revision)
            )
            .is_err()
    );
    no_work(&registry)?;
    Ok(())
}

#[test]
fn terminal_receipt_budget_is_reserved_before_setup_acceptance() -> Result<()> {
    let (temp, mut registry, mut request) = fixture()?;
    let base = temp.path().canonicalize()?;
    for n in 0..5 {
        request.sources.push(SetupSource {
            canonical_path: format!("{}/budget-{n}/{}", base.display(), "p".repeat(2800)),
            aliases: vec![format!("budget{n}")],
            owner: false,
        });
    }
    let length = serde_json::to_vec(&request)?.len();
    assert!(length < 15800);
    request
        .sources
        .last_mut()
        .unwrap()
        .canonical_path
        .push_str(&"p".repeat(15800 - length));
    assert_eq!(serde_json::to_vec(&request)?.len(), 15800);
    request.validate()?;
    assert!(registry.reserve_index_setup(&request).is_err());
    assert!(!registry.is_registered(request.registration.binding)?);
    assert!(
        registry
            .index_setup_receipt(request.caller, &request.key)?
            .is_none()
    );
    no_work(&registry)?;
    let mut json = serde_json::to_value(&request)?;
    json["prepared"] = serde_json::json!(true);
    assert!(
        serde_json::from_value::<IndexSetupRequest>(json).is_err(),
        "frontend preparation assertions are not accepted"
    );
    Ok(())
}

#[test]
fn unsupported_native_alias_shapes_refuse_before_any_setup_reservation() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let before = registry.connection.total_changes();
    let revision = registry.catalog_state()?.unwrap().revision;
    for primary in [true, false] {
        let mut unsupported = request.clone();
        if primary {
            unsupported.sources[0].aliases.push("primary_alias".into());
        } else {
            unsupported.sources[1].aliases.push("second_alias".into());
        }
        assert!(registry.reserve_index_setup(&unsupported).is_err());
        assert!(!registry.is_registered(request.registration.binding)?);
        assert!(
            registry
                .index_setup_receipt(request.caller, &request.key)?
                .is_none()
        );
        assert_eq!(registry.catalog_state()?.unwrap().revision, revision);
        let counts:(i64,i64,i64)=registry.connection.query_row("SELECT (SELECT count(*) FROM index_definitions),(SELECT count(*) FROM index_sources),(SELECT count(*) FROM index_operation_receipts)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?;
        assert_eq!(counts, (0, 0, 0));
        no_work(&registry)?;
        assert!(!Path::new(&request.registration.data_root).exists());
    }
    assert_eq!(registry.connection.total_changes(), before);
    Ok(())
}
