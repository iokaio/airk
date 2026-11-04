// SPDX-License-Identifier: Apache-2.0
//! Metadata format/CAS tests only. SQL fixture activation does not qualify the
//! native guardian, process/job ownership, startup permit, or attach proof.
use super::*;

fn owner(number: u128) -> SharedHelperIdentity {
    SharedHelperIdentity {
        service: Incarnation::from_bytes([4; 16]),
        helper: Incarnation::from_bytes(number.to_be_bytes()),
    }
}
fn boot() -> ProviderBoot {
    ProviderBoot {
        boot_id: Some("aaaa".into()),
        process_scope: "fixture-native".into(),
        elapsed_ns: 1,
        monotonic_reset_proves_boot: false,
    }
}
fn unix() -> SharedHelperContainment {
    SharedHelperContainment::Unix {
        guardian_pid: None,
        group: None,
        pin_device: 1,
        pin_inode: 2,
    }
}
fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry)> {
    let root = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    let service = crate::service::ServiceLease::acquire(root.path())?;
    registry.record_service_instance(&service, owner(1).service)?;
    let legacy = super::super::super::catalog::schema_digest(&registry.connection)?;
    let tx = registry.connection.transaction()?;
    super::super::super::catalog::install_schema(&tx)?;
    tx.execute(
        "INSERT INTO index_catalog_meta VALUES(1,1,0,0,0,0,?1,?2,?3)",
        params![
            Incarnation::from_bytes([2; 16]).to_string(),
            legacy.to_string(),
            Sha256Digest::from_bytes([3; 32]).to_string()
        ],
    )?;
    tx.pragma_update(None, "user_version", 12)?;
    super::super::super::catalog::apply_authority_v1_layout(&tx)?;
    tx.execute(
        "UPDATE index_catalog_meta SET authority_contract=1,activated=1 WHERE id=1",
        [],
    )?;
    tx.commit()?;
    Ok((root, registry))
}

#[test]
fn legacy_registry_never_admits_or_infers_shared_provider_coverage() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
    assert!(
        registry
            .begin_shared_helper_drain(owner(1), unix(), boot())
            .is_err()
    );
    assert!(registry.shared_helper_drains().is_err());
    assert!(registry.provider_ownerships()?.is_empty());
    assert!(registry.provider_drains()?.is_empty());
    Ok(())
}

#[test]
fn shared_records_are_strict_typed_and_never_synthesize_worker_identity() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    let record = registry.begin_shared_helper_drain(owner(1), unix(), boot())?;
    let bytes = record.bytes()?;
    let json: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert!(json.get("identity").is_none());
    assert_eq!(json["version"], 2);
    assert_eq!(json["role"], "shared_embedding_helper");
    assert_eq!(
        decode_shared(
            record.id.to_string(),
            bytes.clone(),
            digest(&bytes),
            record.namespace
        )?,
        record
    );
    let mut unknown = json;
    unknown["worker"] = serde_json::json!("invented");
    let unknown = serde_json::to_vec(&unknown)?;
    assert!(
        decode_shared(
            record.id.to_string(),
            unknown.clone(),
            digest(&unknown),
            record.namespace
        )
        .is_err()
    );
    assert!(
        decode_shared(
            record.id.to_string(),
            bytes.clone(),
            "0".repeat(64),
            record.namespace
        )
        .is_err()
    );
    assert!(
        registry
            .shared_helper_drain(SharedHelperIdentity {
                service: Incarnation::from_bytes([9; 16]),
                ..record.owner
            })
            .is_err()
    );
    // A legacy complete-read API must refuse mixed roles rather than quietly
    // presenting a reduced result. The worker-only API is explicitly selected.
    assert!(registry.provider_drains().is_err());
    assert!(registry.worker_provider_drains()?.is_empty());
    assert_eq!(
        registry.provider_ownerships()?,
        vec![ProviderOwnership::SharedHelper(record)]
    );
    Ok(())
}

#[test]
fn exact_binding_and_terminal_rows_are_retained_without_rewriting_old_identity() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    let reserved = registry.begin_shared_helper_drain(owner(1), unix(), boot())?;
    assert!(registry.bind_shared_helper(&reserved, 456).is_err());
    let pending = registry.bind_shared_guardian(&reserved, 123)?;
    assert!(registry.bind_shared_guardian(&reserved, 789).is_err());
    let bound = registry.bind_shared_helper(&pending, 456)?;
    assert!(registry.bind_shared_helper(&pending, 789).is_err());
    assert!(registry.complete_shared_helper_drain(&pending).is_err());
    assert_eq!(
        registry.shared_helper_drain(bound.owner)?,
        Some(bound.clone())
    );
    let done = registry.complete_shared_helper_drain(&bound)?;
    assert_eq!(done.phase, SharedHelperPhase::Drained);
    assert_eq!(done.owner, bound.owner);
    assert_eq!(done.containment, bound.containment);
    assert_eq!(registry.complete_shared_helper_drain(&bound)?, done);
    assert_eq!(registry.shared_helper_drain(done.owner)?, Some(done));
    Ok(())
}

#[test]
fn global_jobs_are_namespaced_by_exact_service_helper_and_native_user() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    let namespace = registry.incarnation()?;
    let current = owner(1);
    let name = shared_helper_job_name(namespace, current, "S-1-5-21-123")?;
    assert!(name.starts_with("Global\\AIRK-provider-v2-"));
    assert_ne!(
        name,
        shared_helper_job_name(namespace, current, "S-1-5-21-124")?
    );
    assert_ne!(
        name,
        shared_helper_job_name(namespace, owner(2), "S-1-5-21-123")?
    );
    assert!(shared_helper_job_name(namespace, current, "S-1-5-evil").is_err());
    let record = registry.begin_shared_helper_drain(
        current,
        SharedHelperContainment::Windows {
            job_name: name,
            user_sid: "S-1-5-21-123".into(),
            session: 7,
            created: false,
            leader_pid: None,
        },
        boot(),
    )?;
    assert!(registry.bind_shared_helper(&record, 456).is_err());
    let created = registry.mark_shared_job_created(&record)?;
    let bound = registry.bind_shared_helper(&created, 456)?;
    assert_eq!(registry.shared_helper_drain(current)?, Some(bound));
    let mut foreign = record;
    if let SharedHelperContainment::Windows { job_name, .. } = &mut foreign.containment {
        *job_name = job_name.replacen("Global\\", "Local\\", 1);
    }
    assert!(foreign.bytes().is_err());
    Ok(())
}

#[test]
fn mixed_roles_share_existing_global_capacity_and_old_worker_bytes() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    let worker = WorkerIdentity {
        service: owner(1).service,
        worker: Incarnation::from_bytes([7; 16]),
        project: Sha256Digest::from_bytes([8; 32]),
        fencing_generation: 1,
    };
    let mut legacy_bytes = Vec::new();
    for number in 0..64u128 {
        let record = ProviderDrain {
            version: 1,
            id: Incarnation::from_bytes((1000 + number).to_be_bytes()),
            namespace: registry.incarnation()?,
            identity: worker,
            guardian_pid: 123,
            group: Some(456),
            boot: boot(),
        };
        let bytes = record.bytes()?;
        registry.connection.execute(
            "INSERT INTO service_provider_drains VALUES(?1,?2,?3)",
            params![record.id.to_string(), bytes, digest(&bytes)],
        )?;
        legacy_bytes.push((record.id, bytes));
        registry.begin_shared_helper_drain(owner(2000 + number), unix(), boot())?;
    }
    assert_eq!(registry.provider_ownerships()?.len(), MAX_PROVIDER_DRAINS);
    assert_eq!(registry.worker_provider_drains()?.len(), 64);
    assert_eq!(registry.shared_helper_drains()?.len(), 64);
    assert!(
        registry
            .begin_shared_helper_drain(owner(3000), unix(), boot())
            .is_err()
    );
    for (id, bytes) in legacy_bytes {
        let persisted: Vec<u8> = registry.connection.query_row(
            "SELECT payload FROM service_provider_drains WHERE id=?1",
            [id.to_string()],
            |row| row.get(0),
        )?;
        assert_eq!(persisted, bytes);
    }
    Ok(())
}

#[test]
fn damaged_digest_cannot_complete_shared_resource_ownership() -> Result<()> {
    let (_root, mut registry) = fixture()?;
    let record = registry.begin_shared_helper_drain(owner(1), unix(), boot())?;
    registry.connection.execute(
        "UPDATE service_provider_drains SET digest=?1 WHERE id=?2",
        params!["0".repeat(64), record.id.to_string()],
    )?;
    assert!(registry.provider_ownerships().is_err());
    assert!(registry.complete_shared_helper_drain(&record).is_err());
    let count: i64 = registry.connection.query_row(
        "SELECT count(*) FROM service_provider_drains",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(count, 1);
    Ok(())
}

#[test]
fn predecessor_gc_requires_native_lease_exact_terminal_rows_and_rolls_back_the_whole_set()
-> Result<()> {
    let (root, mut registry) = fixture()?;
    let first = registry.begin_shared_helper_drain(owner(1), unix(), boot())?;
    let second = registry.begin_shared_helper_drain(owner(2), unix(), boot())?;
    let first = registry.complete_shared_helper_drain(&first)?;
    let second = registry.complete_shared_helper_drain(&second)?;
    let lease = crate::service::ServiceLease::acquire(root.path())?;
    assert!(
        registry
            .prune_drained_shared_helpers(&lease, owner(1).service, std::slice::from_ref(&first))
            .is_err()
    );
    let current = Incarnation::from_bytes([9; 16]);
    registry.record_service_instance(&lease, current)?;
    let mut changed = second.clone();
    changed.boot.elapsed_ns += 1;
    assert!(
        registry
            .prune_drained_shared_helpers(&lease, current, &[first.clone(), changed])
            .is_err()
    );
    assert_eq!(
        registry.shared_helper_drains()?.len(),
        2,
        "earlier deletion must roll back on later mismatch"
    );
    let foreign = tempfile::tempdir()?;
    let foreign_lease = crate::service::ServiceLease::acquire(foreign.path())?;
    assert!(
        registry
            .prune_drained_shared_helpers(&foreign_lease, current, std::slice::from_ref(&first))
            .is_err()
    );
    registry.connection.execute("INSERT INTO index_operation_receipts VALUES(?1,'catalog_management_fenced','prepared',1,?2)",
        params![Incarnation::from_bytes([10;16]).to_string(), b"malformed pending native obligation".as_slice()])?;
    assert!(
        registry
            .prune_drained_shared_helpers(&lease, current, &[first.clone(), second.clone()])
            .is_err()
    );
    assert_eq!(
        registry.shared_helper_drains()?.len(),
        2,
        "unknown pending WDO must retain ownership"
    );
    registry
        .connection
        .execute("DELETE FROM index_operation_receipts", [])?;
    assert_eq!(
        registry.prune_drained_shared_helpers(&lease, current, &[first, second])?,
        2
    );
    assert!(registry.shared_helper_drains()?.is_empty());
    Ok(())
}
