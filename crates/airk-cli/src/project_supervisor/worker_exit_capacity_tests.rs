// SPDX-License-Identifier: Apache-2.0
//! Private Supervisor admission regression with an owned inert fixture child.
use super::*;

#[test]
fn resource_exit_fixture_child() {
    if std::env::var_os("AIRK_TEST_EXIT_CAPACITY_CHILD").is_some() {
        use std::io::Read;
        let _ = std::io::stdin().read_to_end(&mut Vec::new());
    }
}

#[test]
fn exit_capacity_refusal_retains_child_and_closes_all_fresh_admission() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    local_ipc::prepare_root(&root)?;
    let instance = Incarnation::from_bytes([1; 16]);
    let registry = ServiceRegistry::open(&root, instance)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg(format!("{}::resource_exit_fixture_child", module_path!()))
        .env("AIRK_TEST_EXIT_CAPACITY_CHILD", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = crate::native_process::spawn(&mut command)?;
    let containment =
        crate::native_process::attach_owned(&mut child, crate::child_job::ChildJob::attach)?;
    let (_sender, receiver) = mpsc::sync_channel(1);
    let identity = WorkerIdentity {
        service: instance,
        worker: Incarnation::from_bytes([200; 16]),
        project: ProjectBinding::from_bytes([200; 32]),
        fencing_generation: 1,
    };
    let worker = ChildWorker {
        exiting: false,
        read_settlement: None,
        catalog_gate: None,
        catalog_pending: None,
        catalog_quarantined: false,
        derived: false,
        _containment: Some(containment),
        scope: None,
        input: child
            .stdin
            .take()
            .map(|input| Box::new(StreamFrames(input)) as Box<dyn FrameSink<WorkerCommand>>),
        child: WorkerHost::Child(child),
        output: receiver,
        reader: None,
        stderr: None,
        identity: Some(identity),
        role: protocol::role(identity.worker),
        runtime: root,
        last_renewal: Instant::now(),
        pool_binding: None,
        provider_helpers: Vec::new(),
        provider_in_process: false,
        #[cfg(feature = "local-inference")]
        provider_completed: None,
        pool_pending: false,
        last_status: Instant::now(),
        waiting: None,
    };
    let mut workers = BTreeMap::from([(identity.project, worker)]);
    let mut drains = WorkerResourceDrains::default();
    for n in 1..=128 {
        drains.reserve_history(
            WorkerIdentity {
                service: instance,
                worker: Incarnation::from_bytes([n; 16]),
                project: ProjectBinding::from_bytes([n; 32]),
                fencing_generation: 1,
            },
            vec![],
        )?;
    }
    assert!(begin_worker_exit(&mut workers, identity.project, &mut None, &mut drains).is_err());
    let retained = workers
        .get_mut(&identity.project)
        .context("capacity refusal lost child owner")?;
    assert!(retained.exiting);
    assert_eq!(retained.identity, Some(identity));
    assert_eq!(drains.identities().len(), 128);
    assert!(
        retained
            .grant()
            .err()
            .context("exiting worker accepted a grant")?
            .to_string()
            .contains("draining")
    );
    let allowance = airk_core::service_resources::WorkAllowance {
        id: instance,
        identity,
        job: identity.project,
        extraction_threads: 1,
        inference_threads: 1,
        working_memory_bytes: 128 << 20,
        background_io_bytes_per_second: 1 << 20,
        ttl_ms: 1000,
    };
    assert!(
        retained
            .run_job(allowance)
            .unwrap_err()
            .to_string()
            .contains("draining")
    );
    assert!(
        retained
            .prepare_pool_binding(None, &registry, false)
            .err()
            .context("exiting worker accepted provider registration")?
            .to_string()
            .contains("draining")
    );
    for in_process in [false, true] {
        assert!(
            retained
                .record_provider_delivery(Some(instance), in_process)
                .unwrap_err()
                .to_string()
                .contains("draining")
        );
    }
    assert!(retained.provider_helpers.is_empty());
    assert!(!retained.provider_in_process);
    Ok(())
}
