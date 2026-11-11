// SPDX-License-Identifier: Apache-2.0
//! Native ownership/Attach component regression, staged for the next test window.
//! This does not qualify installers, a live helper descendant, or Runtime Ready.
use super::*;
use anyhow::Context;

#[test]
fn owner_marker_enables_shared_recovery_but_attach_still_requires_real_project_exclusion()
-> Result<()> {
    use airk_core::service::ProjectRegistration;
    use airk_kernel::service_broker::BrokerLimits;
    use rusqlite::{Connection, params};
    use std::sync::atomic::{AtomicBool, AtomicU32};
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    local_ipc::prepare_root(&root)?;
    let root = root.canonicalize()?;
    let instance = Incarnation::from_bytes([93; 16]);
    let registry = crate::catalog_test_fixture::authority_one(&root, instance)?;
    let namespace = registry.incarnation()?;
    let registration = ProjectRegistration::new(
        owner.path().join("source").to_str().unwrap().into(),
        owner.path().join("data").to_str().unwrap().into(),
        "startup-order".into(),
    )?;
    drop(registry);
    // Physical-format fixture only. It does not bypass production activation
    // or manufacture a native reap receipt from metadata.
    let sql = Connection::open(root.join("service-1.4.sqlite3"))?;
    let tx = sql.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,1,0,1)",
        params![
            registration.binding.to_string(),
            serde_json::to_vec(&registration)?
        ],
    )?;
    tx.commit()?;
    drop(sql);
    let ownership = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    let exclusion = ProjectLease::acquire(&root, registration.binding)?;
    let policy = Arc::new(Mutex::new(Broker::new(instance, BrokerLimits::service())?));
    let (log, _log_guard) = crate::service_logs::Log::start(root.clone(), instance);
    let supervisor = Supervisor::start(
        root.clone(),
        instance,
        policy,
        Arc::new(AtomicU32::new(0)),
        Arc::new(AtomicBool::new(true)),
        log,
        ownership.clone(),
        crate::runtime_metadata::project_worker_hooks(),
    )?;
    assert!(
        crate::embedding_pool_process::recover_predecessor_shared_helpers(&root, instance).is_err(),
        "an unrecorded owner received positive shared coverage"
    );
    ServiceRegistry::open(&root, instance)?.record_service_instance(&ownership, instance)?;
    let proof = crate::embedding_pool_process::recover_predecessor_shared_helpers(&root, instance)?
        .context("native owner marker did not permit shared recovery")?;
    assert!(proof.matches(namespace, instance));
    let gate = NativeCatalogAdmission::new(
        instance,
        namespace,
        airk_core::index_catalog::CatalogRevision::INITIAL,
        1,
    )?;
    let handle = CatalogControlHandle {
        sender: Arc::downgrade(supervisor.sender.as_ref().unwrap()),
        gate: gate.clone(),
        source: None,
    };
    let error = handle
        .call(Operation::Attach)
        .err()
        .context("held native project exclusion was ignored")?;
    assert!(error.is::<NativePredecessorsPending>());
    drop(exclusion);
    handle.attach_before(Instant::now() + Duration::from_secs(5))?;
    gate.begin_issue(
        namespace,
        airk_core::index_catalog::CatalogRevision::INITIAL,
    )?;
    drop(handle);
    drop(supervisor);
    Ok(())
}
