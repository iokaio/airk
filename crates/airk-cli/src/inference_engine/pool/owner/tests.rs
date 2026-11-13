// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn actual_owner_failure_remains_visible_after_native_join_and_is_not_reap() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("control");
    local_ipc::prepare_root(&root)?;
    let service = airk_kernel::service_broker::new_instance_id()?;
    let registry = airk_store::service::registry::ServiceRegistry::open(&root, service)?;
    let namespace = registry.incarnation()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    let mut owner = InferenceOwner::start(lease.clone(), service, 1)?;
    // Terminate the actual owned listener/compute lanes without requesting a
    // public stop. The next observation must retain this unexpected exit.
    owner.state.closing.store(true, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !owner.failed() && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(owner.failed());
    let status = owner.provider_status()?;
    assert!(!status.admission_open);
    assert_eq!(
        status.failure,
        Some(airk_core::inference_provider::ProviderFailure::OwnerFailed)
    );
    assert!(owner.stop()?);
    assert!(
        owner.failed(),
        "joining the thread must not erase its earlier failure"
    );
    let operation = airk_kernel::service_broker::new_instance_id()?;
    let witness = owner
        .checkpoint_drain(operation, Vec::new())?
        .context("joined compute owner has no native barrier")?;
    assert_eq!(witness.namespace(), namespace);
    assert!(witness.matches(service, witness.observation().provider.provider_generation));
    let proof = witness.clone();
    drop(owner);
    drop(lease);
    assert!(
        airk_store::service::ServiceLease::acquire(&root).is_err(),
        "opaque native witness must retain its exact lease"
    );
    drop(proof);
    assert!(airk_store::service::ServiceLease::acquire(&root).is_ok());
    Ok(())
}
#[test]
fn established_native_connection_cannot_admit_after_its_grant_expires() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let runtime = fixture.path().join("runtime");
    let source = fixture.path().join("source");
    let data = fixture.path().join("data");
    std::fs::create_dir(&source)?;
    std::fs::create_dir(&data)?;
    local_ipc::prepare_root(&runtime)?;
    let service = airk_kernel::service_broker::new_instance_id()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&runtime)?);
    let owner = InferenceOwner::start(lease, service, 1)?;
    let project =
        airk_kernel::service_projects::registration(&source, &data, "expired-capability")?;
    let worker = WorkerIdentity {
        service,
        worker: airk_kernel::service_broker::new_instance_id()?,
        project: project.binding,
        fencing_generation: 1,
    };
    let grant = protocol::Grant {
        service,
        pool: owner.generation(),
        project,
        worker: worker.worker,
        token: Sha256Digest::from_bytes([7; 32]),
    };
    owner.register(grant.clone(), worker)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let mut stream = local_ipc::connect(&local_ipc::endpoint(
                &runtime,
                &protocol::role(owner.generation()),
            )?)
            .await?;
            local_ipc::write_message(&mut stream, &grant, Duration::from_secs(2)).await?;
            let ready: protocol::Observation =
                local_ipc::read_message(&mut stream, Duration::from_secs(2))
                    .await?
                    .context("native authorization handshake missing")?;
            ensure!(
                matches!(ready,protocol::Observation::Ready{pool,..} if pool==owner.generation()),
                "crossed native authorization handshake"
            );
            ensure!(
                owner.state.grant_live(&grant)?,
                "fresh established grant is not live"
            );
            owner
                .state
                .grants
                .lock()
                .unwrap()
                .get_mut(&grant.token)
                .context("registered grant disappeared")?
                .expires = Instant::now() - Duration::from_secs(1);
            // The per-request check is authoritative even before timer pruning;
            // a renewal cannot resurrect an already expired capability.
            ensure!(
                !owner.state.grant_live(&grant)?,
                "expired grant survived without a prune turn"
            );
            ensure!(
                !owner.renew(worker.worker)?,
                "renewal resurrected an expired grant"
            );
            let operation = protocol::Operation::Close;
            let envelope = protocol::Envelope {
                project: worker.project,
                worker: worker.worker,
                session: Incarnation::from_bytes([8; 16]),
                request_id: Incarnation::from_bytes([9; 16]),
                input_digest: protocol::input_digest(&operation)?,
                operation,
            };
            let sent =
                local_ipc::write_message(&mut stream, &envelope, Duration::from_secs(2)).await;
            let reply: Result<Option<protocol::ResultEnvelope>> = match sent {
                Ok(()) => local_ipc::read_message(&mut stream, Duration::from_secs(2)).await,
                Err(error) => Err(error),
            };
            match reply {
                Ok(None) => {}
                Ok(Some(_)) => {
                    anyhow::bail!("expired established capability reached the native engine")
                }
                Err(error) => {
                    let transport = error
                        .downcast_ref::<std::io::Error>()
                        .context("expired transport did not close within its bounded deadline")?;
                    ensure!(
                        matches!(
                            transport.kind(),
                            std::io::ErrorKind::UnexpectedEof
                                | std::io::ErrorKind::BrokenPipe
                                | std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::ConnectionAborted
                        ),
                        "unexpected expired transport failure: {error:#}"
                    );
                }
            }
            ensure!(
                !owner.failed(),
                "rejecting an expired peer failed the shared owner"
            );
            Ok::<(), anyhow::Error>(())
        })?;
    Ok(())
}
