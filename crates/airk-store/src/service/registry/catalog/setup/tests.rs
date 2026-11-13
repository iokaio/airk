// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::service::{ServiceLease, registry::ServiceRegistry};
use airk_core::index_catalog::IndexId;

pub(super) fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, IndexSetupRequest)> {
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
        legacy_adoption: None,
        prepared_digest: receipt.preparation.as_ref().unwrap().digest,
        expected_catalog_revision: revision,
        expected_source_revision: receipt.source_revision,
    }
}

#[test]
fn clean_break_source_census_keeps_distinct_indexes_shared_roots_and_excludes_retired_backing()
-> Result<()> {
    let (owner, mut registry, mut request) = fixture()?;
    assert!(
        registry
            .catalog_clean_break_sources(request.expected_catalog_revision)
            .is_err(),
        "the shared authority-0 metadata fixture must not satisfy the authority-1 census guard"
    );
    // This test substitutes only authority-layout activation metadata, as the
    // shared fixture substitutes native storage preparation below. Install the
    // canonical complete authority-1 physical layout; do not relabel authority 0
    // or claim native readiness qualification from this Store-only fixture.
    let promotion = registry.connection.unchecked_transaction()?;
    super::super::apply_authority_v1_layout(&promotion)?;
    promotion.execute("UPDATE index_catalog_meta SET authority_contract=1,revision=revision+1 WHERE id=1 AND authority_contract=0", [])?;
    promotion.commit()?;
    request.expected_catalog_revision = registry.catalog_state()?.unwrap().revision;
    fn complete(registry: &mut ServiceRegistry, request: &IndexSetupRequest) -> Result<()> {
        let reserved = registry.reserve_index_setup(request)?;
        let prepared = registry.record_index_preparation(
            request.caller,
            &request.key,
            reserved.request_digest,
            &native(&reserved, reserved.catalog_revision)?,
        )?;
        registry.complete_index_setup(
            request.caller,
            &request.key,
            reserved.request_digest,
            &completion(&prepared, prepared.catalog_revision),
        )?;
        Ok(())
    }
    complete(&mut registry, &request)?;
    let mut second = request.clone();
    second.key.nonce = Incarnation::from_bytes([91; 16]);
    second.expected_catalog_revision = registry.catalog_state()?.unwrap().revision;
    second.name = "Independent index of shared sources".into();
    second.registration = ProjectRegistration::new(
        request.registration.root.clone(),
        owner.path().join("second-storage").to_str().unwrap().into(),
        "second".into(),
    )?;
    second.sources[1].canonical_path = owner.path().join("other").to_str().unwrap().into();
    second.sources[1].aliases = vec!["other-source".into()];
    complete(&mut registry, &second)?;
    let revision = registry.catalog_state()?.unwrap().revision;
    let sources = registry.catalog_clean_break_sources(revision)?;
    assert_eq!(sources.len(), 2);
    assert_ne!(sources[0].0.id, sources[1].0.id);
    assert_ne!(sources[0].0.backing.binding, sources[1].0.backing.binding);
    for (index, roots, policy) in &sources {
        assert_eq!(index.backing.root, request.registration.root);
        assert_eq!(roots.len(), 2);
        assert!(policy.is_none());
        assert!(
            !Path::new(&index.backing.data_root).exists(),
            "source census must not create or open historical index storage"
        );
    }
    assert_ne!(
        sources[0]
            .1
            .iter()
            .find(|root| !root.owner)
            .unwrap()
            .canonical_path,
        sources[1]
            .1
            .iter()
            .find(|root| !root.owner)
            .unwrap()
            .canonical_path
    );
    registry.connection.execute(
        "UPDATE index_definitions SET tombstone=1 WHERE active_registration=?1",
        [second.registration.binding.to_string()],
    )?;
    registry.connection.execute(
        "UPDATE projects SET registered=0 WHERE binding=?1",
        [second.registration.binding.to_string()],
    )?;
    let sources = registry.catalog_clean_break_sources(revision)?;
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].0.backing, request.registration);
    assert!(
        registry
            .catalog_clean_break_sources(revision.next()?)
            .is_err()
    );
    Ok(())
}

#[test]
fn reservation_and_native_preparation_remain_pending_until_exact_once_ready() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let reserved = registry.reserve_index_setup(&request)?;
    assert_eq!(reserved.stage, SetupStage::Reserved);
    let checkpoint = crate::service::registry::checkpoint::snapshot(&registry.root)?;
    assert!(checkpoint.unfinished.iter().any(|work| work.kind == KIND
        && work.id == reserved.operation_id.to_string()
        && work.state == "reserved"));
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
    assert!(
        crate::service::registry::checkpoint::snapshot(&registry.root)?
            .unfinished
            .iter()
            .any(|work| work.kind == KIND && work.state == "prepared")
    );
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
    let checkpoint = crate::service::registry::checkpoint::snapshot(&registry.root)?;
    assert!(
        checkpoint.unfinished.is_empty(),
        "completed setup is immutable history, not accepted running work"
    );
    assert!(
        checkpoint
            .tables
            .iter()
            .any(|table| table.name == "index_operation_receipts" && table.rows == Some(1)),
        "terminal receipt remains in the full checkpoint digest"
    );
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
fn checkpoint_setup_terminal_requires_exact_valid_owner_and_preserves_unknown_work() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let reserved = registry.reserve_index_setup(&request)?;
    let proof = native(&reserved, reserved.catalog_revision)?;
    let prepared = registry.record_index_preparation(
        request.caller,
        &request.key,
        reserved.request_digest,
        &proof,
    )?;
    let ready = registry.complete_index_setup(
        request.caller,
        &request.key,
        reserved.request_digest,
        &completion(&prepared, prepared.catalog_revision),
    )?;
    let id = ready.operation_id.to_string();
    let original = serde_json::to_vec(&ready)?;
    let observe =
        |registry: &ServiceRegistry| crate::service::registry::checkpoint::snapshot(&registry.root);
    assert!(observe(&registry)?.unfinished.is_empty());
    let before = registry.connection.total_changes();
    let first = observe(&registry)?.digest()?;
    assert_eq!(first, observe(&registry)?.digest()?);
    assert_eq!(
        before,
        registry.connection.total_changes(),
        "checkpoint is a nonmutating read"
    );
    for (kind, stage) in [
        ("unknown_owner", "ready"),
        ("unknown_owner", "complete"),
        (KIND, "unknown_phase"),
    ] {
        registry.connection.execute(
            "UPDATE index_operation_receipts SET kind=?1,stage=?2 WHERE id=?3",
            params![kind, stage, id],
        )?;
        assert!(
            observe(&registry)?
                .unfinished
                .iter()
                .any(|work| work.id == id && work.kind == kind && work.state == stage)
        );
    }
    registry.connection.execute(
        "UPDATE index_operation_receipts SET kind=?1,stage='ready',payload=X'7b7d' WHERE id=?2",
        params![KIND, id],
    )?;
    assert!(
        observe(&registry).is_err(),
        "ready text cannot authorize malformed proof"
    );
    registry.connection.execute(
        "UPDATE index_operation_receipts SET payload=?1,revision=revision+1 WHERE id=?2",
        params![original, id],
    )?;
    assert!(
        observe(&registry).is_err(),
        "crossed SQL revision cannot authorize a terminal receipt"
    );
    registry.connection.execute(
        "UPDATE index_operation_receipts SET revision=revision-1,id=?1 WHERE id=?2",
        params![Incarnation::from_bytes([222; 16]).to_string(), id],
    )?;
    assert!(
        observe(&registry).is_err(),
        "crossed SQL operation cannot authorize a terminal receipt"
    );
    Ok(())
}

#[test]
fn checkpoint_completed_history_at_canonical_receipt_capacity_remains_readable() -> Result<()> {
    // Component capacity fixture: exact typed receipt encoding under the
    // unchanged canonical 1024-row trigger; no claim of mutation admission.
    let (_temp, mut registry, request) = fixture()?;
    let tx = registry.connection.transaction()?;
    for n in 0u128..1024 {
        let key = RequestKey {
            nonce: Incarnation::from_bytes(n.to_be_bytes()),
            ..request.key.clone()
        };
        let saved = airk_core::catalog_management::CatalogReceipt {
            schema: 1,
            operation_id: catalog_operation_id(request.caller, &key)?,
            caller: request.caller,
            key,
            request_digest: Sha256Digest::from_bytes([41; 32]),
            catalog_revision: CatalogRevision::INITIAL,
            authority_fence_required: false,
        };
        tx.execute("INSERT INTO index_operation_receipts VALUES(?1,'catalog_management','metadata_applied',1,?2)",
            params![saved.operation_id.to_string(),serde_json::to_vec(&saved)?])?;
    }
    assert!(
        tx.execute(
            "INSERT INTO index_operation_receipts VALUES(?1,'unknown_owner','prepared',1,X'7b7d')",
            [Incarnation::from_bytes([255; 16]).to_string()]
        )
        .is_err(),
        "checkpoint coverage must preserve the native receipt capacity guard"
    );
    tx.commit()?;
    let changes = registry.connection.total_changes();
    let checkpoint = crate::service::registry::checkpoint::snapshot(&registry.root)?;
    assert!(checkpoint.unfinished.is_empty());
    assert!(
        checkpoint
            .tables
            .iter()
            .any(|table| table.name == "index_operation_receipts" && table.rows == Some(1024))
    );
    assert_eq!(changes, registry.connection.total_changes());
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
    let first_extra = request.sources.len();
    for n in 0..5 {
        request.sources.push(SetupSource {
            canonical_path: base.join(format!("budget-{n}")).to_str().unwrap().into(),
            aliases: vec![format!("budget{n}")],
            owner: false,
        });
    }
    // Real temp-root length and Windows JSON escaping are variable. Pad the
    // measured encoding, distributing ASCII bytes within each path's real
    // 4096-byte allowance instead of assuming five fixed long paths fit.
    for source in first_extra..request.sources.len() {
        let length = serde_json::to_vec(&request)?.len();
        assert!(
            length <= 15800,
            "short fixture paths already exceed the target"
        );
        let capacity = 4096usize
            .checked_sub(request.sources[source].canonical_path.len())
            .unwrap();
        let padding = (15800 - length).min(capacity);
        request.sources[source]
            .canonical_path
            .push_str(&"p".repeat(padding));
    }
    assert_eq!(serde_json::to_vec(&request)?.len(), 15800);
    request.validate()?;
    assert!(serde_json::to_vec(&request)?.len() < MAX_RECEIPT_BYTES);
    // Even the earliest valid Ready shape exceeds the durable receipt budget;
    // the native worst-revision reservation therefore cannot safely accept it.
    let prepared_digest = Sha256Digest::from_bytes([255; 32]);
    let terminal = IndexSetupReceipt {
        legacy_adoption: None,
        schema: 1,
        operation_id: catalog_operation_id(request.caller, &request.key)?,
        request: request.clone(),
        request_digest: request.identity()?,
        index_id: setup_index_id(&request)?,
        source_ids: setup_source_ids(&request)?,
        stage: SetupStage::Ready,
        catalog_revision: CatalogRevision::INITIAL.next()?.next()?,
        source_revision: CatalogRevision::INITIAL.next()?,
        preparation: Some(PreparationEvidence {
            legacy_adoption: None,
            digest: prepared_digest,
            storage_identity: prepared_digest,
            source_configuration_digest: prepared_digest,
            review: SetupPreparationReview {
                expected_catalog_revision: CatalogRevision::INITIAL.next()?,
                expected_source_revision: CatalogRevision::INITIAL,
            },
        }),
        completion: Some(SetupCompletion {
            legacy_adoption: None,
            prepared_digest,
            expected_catalog_revision: CatalogRevision::INITIAL.next()?,
            expected_source_revision: CatalogRevision::INITIAL,
        }),
    };
    assert!(serde_json::to_vec(&terminal)?.len() > MAX_RECEIPT_BYTES);
    let before = registry.connection.total_changes();
    let revision = registry.catalog_state()?.unwrap().revision;
    let error = registry.reserve_index_setup(&request).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("catalog JSON byte budget exceeded"),
        "unexpected setup refusal: {error:#}"
    );
    assert!(!registry.is_registered(request.registration.binding)?);
    assert!(
        registry
            .index_setup_receipt(request.caller, &request.key)?
            .is_none()
    );
    no_work(&registry)?;
    let counts: (i64, i64, i64, i64) = registry.connection.query_row(
        "SELECT (SELECT count(*) FROM projects),(SELECT count(*) FROM index_definitions),(SELECT count(*) FROM index_sources),(SELECT count(*) FROM index_operation_receipts)",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    assert_eq!(counts, (0, 0, 0, 0));
    assert_eq!(registry.connection.total_changes(), before);
    assert_eq!(registry.catalog_state()?.unwrap().revision, revision);
    assert!(!Path::new(&request.registration.data_root).exists());
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

/// U103: setup through `complete_index_setup_with_index_set` leaves the new
/// index readable through one index set named after it, created exactly once.
#[test]
fn setup_with_index_set_creates_one_whole_source_set_exactly_once() -> Result<()> {
    let (_temp, mut registry, request) = fixture()?;
    let reserved = registry.reserve_index_setup(&request)?;
    let prepared = registry.record_index_preparation(
        request.caller,
        &request.key,
        reserved.request_digest,
        &native(&reserved, reserved.catalog_revision)?,
    )?;
    let finish = completion(&prepared, prepared.catalog_revision);
    let ready = registry.complete_index_setup_with_index_set(
        request.caller,
        &request.key,
        reserved.request_digest,
        &finish,
    )?;
    assert_eq!(ready.stage, SetupStage::Ready);
    let expected_set = format!("prj_{}", ready.operation_id);
    type ProjectRow = (String, String, i64, i64, i64);
    let projects = |registry: &ServiceRegistry| -> Result<Vec<ProjectRow>> {
        let mut statement = registry.connection.prepare(
            "SELECT id,name,definition_revision,scope_revision,defaults_revision FROM query_projects ORDER BY id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    };
    assert_eq!(
        projects(&registry)?,
        vec![(expected_set.clone(), "Reviewed index".to_owned(), 1, 2, 1)]
    );
    let membership: (String, String, i64) = registry.connection.query_row(
        "SELECT project_id,index_id,revision FROM project_index_memberships",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(
        membership,
        (expected_set.clone(), ready.index_id.to_string(), 1)
    );
    let mut statement = registry.connection.prepare(
        "SELECT project_id,index_id,source_id,kind,path FROM project_index_scopes ORDER BY source_id",
    )?;
    let scopes = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let mut sources: Vec<String> = ready.source_ids.iter().map(ToString::to_string).collect();
    sources.sort();
    assert_eq!(
        scopes,
        sources
            .iter()
            .map(|source| (
                expected_set.clone(),
                ready.index_id.to_string(),
                source.clone(),
                "subtree".to_owned(),
                String::new()
            ))
            .collect::<Vec<_>>()
    );
    // An exact retry answers the completed receipt and creates nothing more.
    assert_eq!(
        registry.complete_index_setup_with_index_set(
            request.caller,
            &request.key,
            reserved.request_digest,
            &finish,
        )?,
        ready
    );
    assert_eq!(projects(&registry)?.len(), 1);
    assert_eq!(
        registry
            .connection
            .query_row("SELECT count(*) FROM index_access_grants", [], |row| row
                .get::<_, i64>(0))?,
        0,
        "the set grants nothing by itself"
    );
    Ok(())
}
