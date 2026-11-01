// SPDX-License-Identifier: Apache-2.0
//! Controlled ACL capability tests; not default-installation qualification.
use super::*;
use crate::current::PrivateIntegrityFailure;
use crate::service::collection::integrity::native_owner::NativeOwnerSession;
use std::sync::Arc;

fn identity(
    registry: &crate::service::registry::ServiceRegistry,
    lease: &ProjectLease,
) -> Result<airk_core::service::WorkerIdentity> {
    Ok(airk_core::service::WorkerIdentity {
        service: registry.incarnation()?,
        worker: Incarnation::from_bytes([171; 16]),
        project: lease.identity().0,
        fencing_generation: lease.identity().1.context("worker epoch missing")?,
    })
}

fn arm(
    collection: &Collection,
    registry: &mut crate::service::registry::ServiceRegistry,
    bound: &crate::service::data_scope::BoundDataRoot,
    lease: Arc<ProjectLease>,
) -> Result<NativeOwnerSession> {
    let worker = identity(registry, &lease)?;
    let store = collection.selected_store()?;
    let scope = store
        .current_owned_scope()?
        .context("retained scope missing")?;
    let read = store.current_read(&scope)?;
    registry.observe_retained_audit_index(&lease, &read, 0, 512 << 20)?;
    drop(read);
    collection.arm_native_owner(registry, lease, bound, worker)
}

#[test]
fn private_reader_is_sealed_to_native_session_and_revoked_on_drain() -> Result<()> {
    let (_temp, collection, mut registry, bound, lease, store, scope) = native_state_fixture()?;
    let lease = Arc::new(lease);
    let strict = store.current_read(&scope)?;
    assert!(!strict.private_query_eligible()?);
    let owner = arm(&collection, &mut registry, &bound, lease)?;
    let private = owner.attach_read(store.current_read(&scope)?)?;
    assert!(private.private_query_eligible()?);
    assert_eq!(strict.token()?, private.token()?);
    owner.settle_after_owned_drain(&mut registry)?;
    assert!(private.token().is_err());
    assert!(private.private_query_eligible().is_err());
    assert!(strict.token().is_ok());
    Ok(())
}

#[test]
fn uncertain_private_read_error_never_fabricates_damage_or_clean_settlement() -> Result<()> {
    let (_temp, collection, mut registry, bound, lease, store, scope) = native_state_fixture()?;
    let lease = Arc::new(lease);
    let worker = identity(&registry, &lease)?;
    let owner = arm(&collection, &mut registry, &bound, lease.clone())?;
    let read = owner.attach_read(store.current_read(&scope)?)?;
    let error = std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "controlled uncertain read",
    );
    assert!(
        read.observe_private_integrity::<()>(Err(error.into()))
            .is_err()
    );
    assert!(read.token().is_err());
    assert!(registry.recorded_integrity_damage(&lease)?.is_empty());
    assert!(owner.settle_after_owned_drain(&mut registry).is_err());
    assert!(
        collection
            .arm_native_owner(&mut registry, lease, &bound, worker)
            .is_err()
    );
    Ok(())
}

#[test]
fn private_corruption_damage_stores_fail_independently_and_preserve_active() -> Result<()> {
    for (refuse_control, refuse_local) in
        [(true, false), (false, true), (true, true), (false, false)]
    {
        let (_temp, collection, mut registry, bound, lease, store, scope) = native_state_fixture()?;
        let lease = Arc::new(lease);
        let worker = identity(&registry, &lease)?;
        let owner = arm(&collection, &mut registry, &bound, lease.clone())?;
        let read = owner.attach_read(store.current_read(&scope)?)?;
        let db =
            rusqlite::Connection::open(bound.native_control_root().join("service-1.4.sqlite3"))?;
        if refuse_control {
            db.execute_batch("CREATE TRIGGER refuse_private_damage BEFORE INSERT ON service_recovery_budget WHEN NEW.scope LIKE 'integrity_damage:%' BEGIN SELECT RAISE(ABORT,'controlled control damage failure'); END")?;
        }
        let receipt = collection.directory.join("integrity-observation.json");
        if refuse_local {
            fs::create_dir(&receipt)?;
        }
        let failure = anyhow::anyhow!("controlled authenticated digest mismatch")
            .context(PrivateIntegrityFailure);
        assert!(read.observe_private_integrity::<()>(Err(failure)).is_err());
        assert!(read.token().is_err());
        assert_eq!(
            !registry.recorded_integrity_damage(&lease)?.is_empty(),
            !refuse_control
        );
        if !refuse_local {
            let value: serde_json::Value = serde_json::from_slice(&fs::read(&receipt)?)?;
            assert_eq!(
                value["known_damage"]
                    .as_array()
                    .context("damage list missing")?
                    .len(),
                1
            );
        }
        assert!(owner.settle_after_owned_drain(&mut registry).is_err());
        if refuse_control {
            db.execute_batch("DROP TRIGGER refuse_private_damage")?;
        }
        if refuse_local {
            fs::remove_dir(&receipt)?;
        }
        assert!(
            collection
                .arm_native_owner(&mut registry, lease, &bound, worker)
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn private_reader_busy_does_not_revoke_session() -> Result<()> {
    let (_temp, collection, mut registry, bound, lease, store, scope) = native_state_fixture()?;
    let owner = arm(&collection, &mut registry, &bound, Arc::new(lease))?;
    let read = owner.attach_read(store.current_read(&scope)?)?;
    assert!(
        read.observe_private_integrity::<()>(Err(crate::current::CurrentReadContention.into()))
            .is_err()
    );
    assert!(read.private_query_eligible()?);
    owner.settle_after_owned_drain(&mut registry)?;
    Ok(())
}

#[test]
fn actual_selected_source_digest_failure_records_sticky_damage() -> Result<()> {
    use sha2::{Digest, Sha256};
    let (_temp, collection, mut registry, bound, lease, store, scope) = native_state_fixture()?;
    let quota = Quota::open_service(Path::new(&collection.owner.data_root))?;
    let source = b"original retained bytes";
    {
        let writer = store.lock()?;
        let input = audit_retained_input(
            &store,
            &writer,
            &quota,
            &scope,
            "body.txt",
            std::str::from_utf8(source)?,
        )?;
        let mut current = store.current_write(&writer, &quota, &scope, 8 << 20)?;
        let token = current.head()?.token()?;
        current.commit_artifact(&token, "body.txt", Some(&input))?;
    }
    let lease = Arc::new(lease);
    let owner = arm(&collection, &mut registry, &bound, lease.clone())?;
    let read = owner.attach_read(store.current_read(&scope)?)?;
    let compact = read
        .artifact("body.txt")?
        .context("selected artifact missing")?;
    let path = store
        .directory()
        .join("objects/sources")
        .join(&compact.content_hash);
    let mut damaged = source.to_vec();
    damaged[0] ^= 1;
    fs::write(path, damaged)?;
    let expected = Sha256Digest::from_bytes(Sha256::digest(source).into());
    let error = read
        .observe_private_integrity(store.read_retained_source_block(
            &compact,
            0..source.len(),
            expected,
        ))
        .expect_err("selected source corruption must refuse");
    assert!(error.downcast_ref::<PrivateIntegrityFailure>().is_some());
    assert_eq!(registry.recorded_integrity_damage(&lease)?.len(), 1);
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(
        collection.directory.join("integrity-observation.json"),
    )?)?;
    assert_eq!(
        receipt["known_damage"]
            .as_array()
            .context("damage list missing")?
            .len(),
        1
    );
    assert!(read.token().is_err());
    assert!(owner.settle_after_owned_drain(&mut registry).is_err());
    Ok(())
}

#[test]
fn stopped_maintenance_keeps_new_reads_strict_until_explicit_resume() -> Result<()> {
    let (_temp, collection, mut registry, bound, lease, store, scope) = native_state_fixture()?;
    let lease = Arc::new(lease);
    let owner = arm(&collection, &mut registry, &bound, lease.clone())?;
    let before = owner.attach_read(store.current_read(&scope)?)?;
    assert!(before.private_query_eligible()?);
    let due = registry.due_automatic_audits(0, 8)?;
    assert_eq!(due.len(), 1);
    let job = registry.enqueue_automatic_audit(
        lease.identity().0,
        due[0].1,
        Incarnation::from_bytes([175; 16]),
        Incarnation::from_bytes([176; 16]),
        0,
    )?;
    registry.cancel_pending(job.id)?;
    assert!(
        before.token().is_err(),
        "Stop must fence an admitted private response"
    );
    let stopped_admission = before
        .private_query_eligible()
        .expect_err("stopped maintenance must refuse an old selected-page reader");
    assert!(
        before
            .observe_private_integrity::<()>(Err(stopped_admission))
            .is_err()
    );
    let stopped = owner.attach_read(store.current_read(&scope)?)?;
    assert!(!stopped.private_query_eligible()?);
    // Merely observing the same retained index must not undo the user's Stop.
    registry.observe_retained_audit_index(&lease, &stopped, 1, 512 << 20)?;
    assert!(
        !owner
            .attach_read(store.current_read(&scope)?)?
            .private_query_eligible()?
    );
    assert!(registry.recorded_integrity_damage(&lease)?.is_empty());
    registry.pause(lease.identity().0, true)?;
    registry.pause(lease.identity().0, false)?;
    registry.observe_retained_audit_index(&lease, &stopped, 2, 512 << 20)?;
    let resumed = owner.attach_read(store.current_read(&scope)?)?;
    assert!(resumed.private_query_eligible()?);
    owner.settle_after_owned_drain(&mut registry)?;
    Ok(())
}

#[test]
fn native_owner_without_maintenance_registration_only_attaches_strict_reads() -> Result<()> {
    let (_temp, collection, mut registry, bound, lease, store, scope) = native_state_fixture()?;
    let lease = Arc::new(lease);
    let worker = identity(&registry, &lease)?;
    let owner = collection.arm_native_owner(&mut registry, lease, &bound, worker)?;
    assert!(
        !owner
            .attach_read(store.current_read(&scope)?)?
            .private_query_eligible()?
    );
    owner.settle_after_owned_drain(&mut registry)?;
    Ok(())
}

#[test]
fn expired_private_request_refuses_without_revoking_future_requests() -> Result<()> {
    let (_temp, collection, mut registry, bound, lease, store, scope) = native_state_fixture()?;
    let lease = Arc::new(lease);
    let owner = arm(&collection, &mut registry, &bound, lease.clone())?;
    let mut read = owner.attach_read(store.current_read(&scope)?)?;
    assert!(read.private_query_eligible()?);
    Arc::get_mut(&mut read)
        .context("request reader unexpectedly shared")?
        .expire_request_for_test();
    let expired = read
        .private_query_eligible()
        .expect_err("expired request must refuse");
    assert!(
        expired
            .downcast_ref::<crate::current::CurrentRequestExpired>()
            .is_some()
    );
    assert!(read.observe_private_integrity::<()>(Err(expired)).is_err());
    assert!(read.token().is_err());
    assert!(
        owner
            .attach_read(store.current_read(&scope)?)?
            .private_query_eligible()?
    );
    assert!(registry.recorded_integrity_damage(&lease)?.is_empty());
    owner.settle_after_owned_drain(&mut registry)?;
    Ok(())
}

#[test]
fn attached_private_read_can_bootstrap_and_reuse_anchor_without_recursive_manifest_lock()
-> Result<()> {
    let (_temp, collection, mut registry, bound, lease, store, scope) = native_state_fixture()?;
    let lease = Arc::new(lease);
    let owner = arm(&collection, &mut registry, &bound, lease.clone())?;
    let read = owner.attach_read(store.current_read(&scope)?)?;
    assert!(read.private_query_eligible()?);
    let integrity = collection.integrity_owner(&registry, &lease, &bound)?;
    integrity.anchor_if_missing(&read)?;
    let path = collection.directory.join("integrity-observation.json");
    let saved = fs::read(&path)?;
    integrity.anchor_if_missing(&read)?;
    assert_eq!(fs::read(&path)?, saved);
    // Ordinary metadata publication shares the same lock-held helper and must
    // also authorize the attached read before taking the manifest exclusion.
    integrity.anchor(&read)?;
    assert!(read.private_query_eligible()?);
    owner.settle_after_owned_drain(&mut registry)?;
    Ok(())
}
