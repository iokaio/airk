// SPDX-License-Identifier: Apache-2.0
//! Native inference owner status over the actual HTTPS gateway. Moved from
//! `inference_engine::pool::owner::tests` (split step 2b): it drives the
//! gateway, which sits above inference.
use crate::local_ipc;
use airk_core::{
    identity::Incarnation, inference_provider::CompletionState, service::WorkerIdentity,
};
use airk_host::embedding_pool_protocol as protocol;
use airk_inference::inference_engine::pool::{InferenceOwner, Work, accounting};
use airk_retrieval::embedding_worker::{Request, Response};
use anyhow::{Context, Result, ensure};
use std::{
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

#[test]
fn held_inference_keeps_native_status_responsive_and_checkpoint_charged() -> Result<()> {
    // Hold the canonical compute lane at its existing pre-compute hook. This
    // exercises real owner threads, charges, model lifetime and the native stop
    // barrier. HTTPS uses the actual gateway and this exact provider's status;
    // this component fixture does not claim installed-editor qualification.
    fn enqueue(
        owner: &InferenceOwner,
        grant: &protocol::Grant,
        worker: WorkerIdentity,
        session: Incarnation,
        alive: &Arc<AtomicBool>,
        request: Request,
    ) -> Result<(
        oneshot::Receiver<protocol::ResultEnvelope>,
        accounting::DeliveryCharge,
    )> {
        let operation = protocol::Operation::Call {
            request: Box::new(request),
        };
        let envelope = protocol::Envelope {
            project: worker.project,
            worker: worker.worker,
            session,
            request_id: airk_kernel::service_broker::new_instance_id()?,
            input_digest: protocol::input_digest(&operation)?,
            operation,
        };
        let (charge, delivery) = owner
            .state
            .accounting
            .as_ref()
            .unwrap()
            .admit(worker.worker)?;
        let (reply, response) = oneshot::channel();
        owner
            .state
            .sender
            .blocking_send(Work::Request {
                grant: Box::new(grant.clone()),
                envelope: Box::new(envelope),
                alive: alive.clone(),
                worker_alive: alive.clone(),
                reply,
                charge: Some(charge),
                authorization: Some(owner.state.clone()),
                native_worker: Some(worker),
            })
            .map_err(|_| anyhow::anyhow!("owned inference queue closed"))?;
        Ok((response, delivery))
    }

    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("control");
    local_ipc::prepare_root(&root)?;
    let service = airk_kernel::service_broker::new_instance_id()?;
    let registry = airk_store::service::registry::ServiceRegistry::open(&root, service)?;
    let namespace = registry.incarnation()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    let owner = InferenceOwner::start(lease.clone(), service, 1)?;
    let (mut grant, location, identity) = airk_inference::inference_engine::pool::tests::fixture(
        fixture.path(),
        "held",
        "held model",
    )?;
    grant.service = service;
    grant.pool = owner.generation();
    let worker = WorkerIdentity {
        service,
        worker: grant.worker,
        project: grant.project.binding,
        fencing_generation: 1,
    };
    owner.register(grant.clone(), worker)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let authenticated = runtime.block_on(async {
        let mut stream = local_ipc::connect(&local_ipc::endpoint(
            &root,
            &protocol::role(owner.generation()),
        )?)
        .await?;
        local_ipc::write_message(&mut stream, &grant, Duration::from_secs(2)).await?;
        let ready: protocol::Observation =
            local_ipc::read_message(&mut stream, Duration::from_secs(2))
                .await?
                .context("native authorization handshake missing")?;
        ensure!(
            matches!(ready, protocol::Observation::Ready { pool, .. } if pool == owner.generation()),
            "crossed native authorization handshake: {}",
            serde_json::to_string(&ready)?
        );
        Ok::<_, anyhow::Error>(stream)
    })?;
    let session = airk_kernel::service_broker::new_instance_id()?;
    let alive = Arc::new(AtomicBool::new(true));
    let (opened, delivery) = enqueue(
        &owner,
        &grant,
        worker,
        session,
        &alive,
        airk_inference::inference_engine::pool::tests::open(location, identity),
    )?;
    let opened = opened.blocking_recv()?;
    assert!(
        matches!(opened.response, Response::Opened { .. }),
        "native model Open failed: {}",
        serde_json::to_string(&opened)?
    );
    drop(delivery);

    let (entered, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release, release_rx) = std::sync::mpsc::sync_channel(1);
    // Dropping this sender also releases a failed test, before owner Drop joins
    // its compute thread. No detached native work survives a failed assertion.
    let (configured, configured_rx) = oneshot::channel();
    owner
        .state
        .sender
        .blocking_send(Work::ComputeHook {
            hook: Box::new(move |context, request| {
                assert_eq!(context.unwrap().worker, Some(worker));
                assert!(matches!(request, Request::Embed { .. }));
                entered.send(()).unwrap();
                let _ = release_rx.recv_timeout(Duration::from_secs(10));
            }),
            reply: configured,
        })
        .map_err(|_| anyhow::anyhow!("owned inference queue closed"))?;
    configured_rx.blocking_recv()?;
    let (result, delivery) = enqueue(
        &owner,
        &grant,
        worker,
        session,
        &alive,
        Request::Embed {
            texts: vec!["cobalt".into()],
            workers: 1,
            memory_bytes: 8 << 20,
        },
    )?;
    entered_rx.recv_timeout(Duration::from_secs(5))?;
    let status = owner.provider_status()?;
    assert!(status.admission_open);
    assert_eq!(status.running_outstanding, 1);
    assert_eq!(status.callback_outstanding, 1);
    assert_eq!(status.delivery_outstanding, 1);
    assert!(status.model_references_outstanding >= 1);
    assert!(matches!(
        owner.status()?,
        protocol::Observation::Status { .. }
    ));

    struct Status(Arc<Mutex<InferenceOwner>>);
    impl airk_mcp::remote::RemoteTools for Status {
        fn call(
            &self,
            name: String,
            _args: serde_json::Map<String, serde_json::Value>,
        ) -> airk_mcp::remote::RemoteFuture<'_> {
            Box::pin(async move {
                ensure!(
                    matches!(name.as_str(), "status" | "airk_status"),
                    "unexpected native status operation: {name}"
                );
                let owner = self.0.lock().unwrap();
                Ok(serde_json::json!({
                    "content": [],
                    "isError": false,
                    "structuredContent": owner.provider_status()?,
                }))
            })
        }
    }
    let owner = Arc::new(Mutex::new(owner));
    let https = runtime.block_on(crate::http_gateway::tests::live_https_status(Arc::new(
        Status(owner.clone()),
    )))?;
    let mut expected = serde_json::to_value(&status)?;
    expected["query_concurrency"] = airk_core::service_resources::DEFAULT_QUERY_CONCURRENCY.into();
    assert_eq!(https["structuredContent"], expected, "{https}");
    let mut owner = Arc::try_unwrap(owner)
        .map_err(|_| anyhow::anyhow!("HTTPS retained native status owner"))?
        .into_inner()
        .map_err(|_| anyhow::anyhow!("native status owner poisoned"))?;

    let operation = airk_kernel::service_broker::new_instance_id()?;
    assert!(owner.checkpoint_drain(operation, vec![worker])?.is_none());
    let pending = owner.checkpoint_context()?.unwrap();
    assert_eq!(pending.state, CompletionState::Pending);
    assert_eq!(pending.running_outstanding, 1);
    assert_eq!(pending.captured_workers, vec![worker]);
    assert!(!owner.provider_status()?.admission_open);
    assert!(owner.register(grant.clone(), worker).is_err());
    assert!(
        owner
            .state
            .accounting
            .as_ref()
            .unwrap()
            .admit(worker.worker)
            .is_err()
    );
    assert!(!owner.worker_settled(worker.worker)?);
    assert!(
        !owner.stop()?,
        "held compute must not manufacture native exit"
    );
    assert!(owner.completion().is_none());
    assert!(airk_store::service::ServiceLease::acquire(&root).is_err());

    release.send(())?;
    let Response::Embedded { vectors, .. } = result.blocking_recv()?.response else {
        anyhow::bail!("held accepted inference did not complete")
    };
    assert_eq!(vectors, vec![Some(vec![1.0, 0.0])]);
    // Even completed compute cannot certify drain until delivery is released.
    assert!(owner.checkpoint_drain(operation, vec![worker])?.is_none());
    assert_eq!(owner.checkpoint_context()?.unwrap().delivery_outstanding, 1);
    drop(delivery);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !owner.stop()? {
        ensure!(
            Instant::now() < deadline,
            "native owner did not settle after release"
        );
        std::thread::yield_now();
    }
    let witness = owner
        .checkpoint_drain(operation, vec![worker])?
        .context("settled native owner has no completion witness")?
        .clone();
    assert_eq!(witness.namespace(), namespace);
    assert_eq!(witness.observation().state, CompletionState::Complete);
    assert_eq!(witness.observation().running_outstanding, 0);
    assert_eq!(witness.observation().model_references_outstanding, 0);
    assert_eq!(witness.observation().delivery_outstanding, 0);
    drop(owner);
    drop(authenticated);
    drop(lease);
    assert!(airk_store::service::ServiceLease::acquire(&root).is_err());
    drop(witness);
    assert!(airk_store::service::ServiceLease::acquire(&root).is_ok());
    let next_service = airk_kernel::service_broker::new_instance_id()?;
    let next_lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
    let next_owner = InferenceOwner::start(next_lease, next_service, 1)?;
    assert!(next_owner.register(grant.clone(), worker).is_err());
    runtime.block_on(async {
        let mut stream = local_ipc::connect(&local_ipc::endpoint(
            &root,
            &protocol::role(next_owner.generation()),
        )?)
        .await?;
        local_ipc::write_message(&mut stream, &grant, Duration::from_secs(2)).await?;
        let response: Option<protocol::Observation> =
            local_ipc::read_message(&mut stream, Duration::from_secs(2)).await?;
        ensure!(
            response.is_none(),
            "restarted owner accepted an old engine grant"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}
