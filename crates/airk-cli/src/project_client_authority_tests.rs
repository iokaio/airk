// SPDX-License-Identifier: Apache-2.0
//! Actual same-user IPC fixtures for authority shared by independent frontend channels.
use super::*;
use airk_core::{
    identity::Incarnation,
    service::{
        ConnectionGrant, ControlRequest, ProjectRegistration, ServiceHealth, WorkerIdentity,
    },
};
use airk_store::service::registry::ServiceRegistry;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::{Notify, watch};

#[derive(Default)]
struct Gate {
    entered: Notify,
    release: Notify,
}
impl Gate {
    async fn hold(&self) {
        self.entered.notify_one();
        self.release.notified().await;
    }
}
#[derive(Default)]
struct Controls {
    health: AtomicUsize,
    connect: AtomicUsize,
    hold_health: Mutex<Option<Arc<Gate>>>,
    hold_grant: Mutex<Option<Arc<Gate>>>,
}
#[derive(Default)]
struct Queries {
    count: AtomicUsize,
    receipts: Mutex<Vec<airk_mcp::response::StatusReceipt>>,
    hold_reply: Mutex<Option<Arc<Gate>>>,
    barrier: Mutex<Option<Arc<tokio::sync::Barrier>>>,
}
struct OwnedIpc {
    route: Arc<Mutex<WorkerIdentity>>,
    controls: Arc<Controls>,
    stop: watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<Result<()>>>,
}
impl Drop for OwnedIpc {
    fn drop(&mut self) {
        // Test-only socket peers own no native jobs or source writes. On an
        // assertion panic, stop these actual fixture tasks rather than detach them.
        self.stop.send_replace(true);
        for task in &self.tasks {
            task.abort();
        }
    }
}
fn test_identity(project: ProjectBinding, generation: u64, byte: u8) -> WorkerIdentity {
    WorkerIdentity {
        service: Incarnation::from_bytes([102; 16]),
        worker: Incarnation::from_bytes([byte; 16]),
        project,
        fencing_generation: generation,
    }
}
fn worker_receipt(identity: WorkerIdentity) -> airk_mcp::response::StatusReceipt {
    serde_json::from_value(
        serde_json::json!({"freshness":format!("{:064x}",identity.fencing_generation)}),
    )
    .unwrap()
}
fn worker_publication(identity: WorkerIdentity) -> String {
    format!("worker-{}-{}", identity.worker, identity.fencing_generation)
}
fn response_body(value: &serde_json::Value) -> Result<serde_json::Value> {
    Ok(serde_json::from_str(
        value["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing fixture result body"))?,
    )?)
}
impl OwnedIpc {
    fn new(root: &Path, initial: WorkerIdentity) -> Result<Self> {
        local_ipc::prepare_root(root)?;
        let mut listener = local_ipc::Listener::bind(&local_ipc::endpoint(root, "control")?)?;
        let route = Arc::new(Mutex::new(initial));
        let controls = Arc::new(Controls::default());
        let (stop, mut stopped) = watch::channel(false);
        let chosen = route.clone();
        let observed = controls.clone();
        let task = tokio::spawn(async move {
            let mut handlers = tokio::task::JoinSet::new();
            loop {
                let stream = tokio::select! {
                    _=stopped.changed()=>break,
                    accepted=listener.accept()=>accepted?,
                };
                let chosen = chosen.clone();
                let observed = observed.clone();
                handlers.spawn(async move {
                    let mut stream = stream;
                    let request: ControlRequest =
                        local_ipc::read_message(&mut stream, Duration::from_secs(5))
                            .await?
                            .ok_or_else(|| anyhow::anyhow!("missing control request"))?;
                    // Bind the actual response before the hold, so changing the
                    // broker route cannot relabel a delayed old grant.
                    let identity = *chosen.lock().unwrap();
                    let response = match request.method {
                        ControlMethod::Health => {
                            observed.health.fetch_add(1, Ordering::SeqCst);
                            let gate = observed.hold_health.lock().unwrap().take();
                            if let Some(gate) = gate {
                                gate.hold().await;
                            }
                            ControlResponse::Health {
                                observation: Box::new(ServiceHealth {
                                    storage_formats: None,
                                    build: None,
                                    build_error: None,
                                    logs: airk_core::service_logs::LogTelemetry {
                                        available: false,
                                        dropped_records: 0,
                                        write_failures: 0,
                                    },
                                    version: env!("CARGO_PKG_VERSION").into(),
                                    protocol: PROTOCOL_VERSION,
                                    instance: identity.service,
                                    process_id: std::process::id(),
                                    executable_sha256: "00".repeat(32),
                                    runtime_sha256: None,
                                    user: local_ipc::current_user()?,
                                    uptime_ms: 0,
                                    policy: airk_kernel::service_broker::Broker::new(
                                        identity.service,
                                        Default::default(),
                                    )?
                                    .status(Instant::now()),
                                }),
                            }
                        }
                        ControlMethod::Connect { project } => {
                            ensure!(project == identity.project, "foreign fixture project");
                            observed.connect.fetch_add(1, Ordering::SeqCst);
                            let gate = observed.hold_grant.lock().unwrap().take();
                            if let Some(gate) = gate {
                                gate.hold().await;
                            }
                            ControlResponse::Connection {
                                role: protocol::role(identity.worker),
                                grant: ConnectionGrant {
                                    scope: None,
                                    protocol: PROTOCOL_VERSION,
                                    identity,
                                    caller: local_ipc::caller_identity()?,
                                    token: "cd".repeat(32).parse()?,
                                    ttl_ms: 5000,
                                    client: airk_core::usage::ClientFamily::Unknown,
                                },
                            }
                        }
                        _ => anyhow::bail!("unexpected fixture control method"),
                    };
                    local_ipc::write_message(&mut stream, &response, Duration::from_secs(5)).await
                });
            }
            while let Some(result) = handlers.join_next().await {
                result??;
            }
            Ok(())
        });
        Ok(Self {
            route,
            controls,
            stop,
            tasks: vec![task],
        })
    }
    fn add_worker(&mut self, root: &Path, identity: WorkerIdentity) -> Result<Arc<Queries>> {
        let mut listener = local_ipc::Listener::bind(&local_ipc::endpoint(
            root,
            &protocol::role(identity.worker),
        )?)?;
        let records = Arc::new(Queries::default());
        let observed = records.clone();
        let mut stopped = self.stop.subscribe();
        self.tasks.push(tokio::spawn(async move {
            let mut handlers=tokio::task::JoinSet::new();
            loop {
                let stream=tokio::select! { _=stopped.changed()=>break,accepted=listener.accept()=>accepted?, };
                let observed=observed.clone();
                handlers.spawn(async move {
                    let mut stream=stream;
                    let grant:ConnectionGrant=local_ipc::read_message(&mut stream,Duration::from_secs(5)).await?.ok_or_else(||anyhow::anyhow!("missing fixture grant"))?;
                    ensure!(grant.identity==identity,"fixture grant changed worker");
                    local_ipc::write_message(&mut stream,&Accepted{identity},Duration::from_secs(5)).await?;
                    while let Some(query)=local_ipc::read_message::<_,Query>(&mut stream,Duration::from_secs(5)).await? {
                        observed.count.fetch_add(1,Ordering::SeqCst);
                        observed.receipts.lock().unwrap().push(query.status_receipt);
                        let barrier = observed.barrier.lock().unwrap().clone();
                        if let Some(barrier) = barrier { barrier.wait().await; }
                        let gate=observed.hold_reply.lock().unwrap().take();
                        if let Some(gate)=gate { gate.hold().await; }
                        local_ipc::write_message(&mut stream,&protocol::QueryResult {
                            result:airk_mcp::unified::success(serde_json::json!({"index_state":"available","publication":{"token":worker_publication(identity)}})),
                            status_receipt:worker_receipt(identity),rebind:false,
                        },Duration::from_secs(5)).await?;
                    }
                    Ok::<_,anyhow::Error>(())
                });
            }
            while let Some(result)=handlers.join_next().await { result??; }
            Ok(())
        }));
        Ok(records)
    }
    async fn finish(mut self) -> Result<()> {
        self.stop.send_replace(true);
        // Keep each handle owned if the watchdog expires; Drop aborts only our
        // socket fixtures. Production frontend owners have already drained.
        for task in &mut self.tasks {
            tokio::time::timeout(Duration::from_secs(10), task).await???;
        }
        self.tasks.clear();
        Ok(())
    }
}
fn fixture(pinned: bool) -> Result<(tempfile::TempDir, PathBuf, SourceFence, Arc<Backend>)> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    local_ipc::prepare_root(&root)?;
    let project = ProjectRegistration::new(
        owner.path().join("source").to_string_lossy().into_owned(),
        owner.path().join("data").to_string_lossy().into_owned(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&root, Incarnation::from_bytes([101; 16]))?;
    registry.register(&project)?;
    drop(registry);
    let Some(CacheScope::Registered(observed)) = status_scope(&root, project.binding)? else {
        anyhow::bail!("fixture registration missing")
    };
    assert_eq!(observed.fencing_generation, 0);
    let backend = source_tools(root.clone(), project.binding);
    if !pinned {
        backend.status.source_scope(observed);
    }
    Ok((owner, root, observed, backend))
}
async fn call(
    backend: &Backend,
    pinned: bool,
    scope: SourceFence,
    tool: &str,
) -> Result<serde_json::Value> {
    if pinned {
        airk_mcp::remote::RemoteTools::call(backend, tool.into(), Default::default()).await
    } else {
        backend
            .call_observed(tool.into(), Default::default(), scope)
            .await
    }
}
async fn drained(backend: Arc<Backend>, ipc: OwnedIpc) -> Result<()> {
    backend.status.shutdown().await?;
    drop(backend);
    ipc.finish().await
}

#[tokio::test]
async fn actual_newer_grants_fence_both_channel_directions_without_renewing_healthy_connections()
-> Result<()> {
    for pinned in [false, true] {
        for status_first in [false, true] {
            let (_owner, root, scope, backend) = fixture(pinned)?;
            let old = test_identity(scope.project, 1, 111);
            let newer = test_identity(scope.project, 2, 112);
            let mut ipc = OwnedIpc::new(&root, old)?;
            let old_queries = ipc.add_worker(&root, old)?;
            let new_queries = ipc.add_worker(&root, newer)?;
            let first = if status_first {
                "airk_status"
            } else {
                "airk_read_file"
            };
            let second = if status_first {
                "airk_read_file"
            } else {
                "airk_status"
            };
            assert_eq!(
                response_body(&call(&backend, pinned, scope, first).await?)?["publication"]["token"],
                worker_publication(old)
            );
            *ipc.route.lock().unwrap() = newer;
            assert_eq!(
                response_body(&call(&backend, pinned, scope, second).await?)?["publication"]["token"],
                worker_publication(newer)
            );
            // Saved registry generation remains zero: only the actual accepted
            // grant, shared across the two transports, can reject worker one.
            assert!(status_scope(&root, scope.project)? == Some(CacheScope::Registered(scope)));
            assert_eq!(
                response_body(&call(&backend, pinned, scope, first).await?)?["publication"]["token"],
                worker_publication(newer)
            );
            assert_eq!(
                old_queries.count.load(Ordering::SeqCst),
                1,
                "old live channel received an extra Query"
            );
            assert_eq!(new_queries.count.load(Ordering::SeqCst), 2);
            let handshakes = ipc.controls.connect.load(Ordering::SeqCst);
            assert_eq!(handshakes, 3);
            call(&backend, pinned, scope, first).await?;
            assert_eq!(
                ipc.controls.connect.load(Ordering::SeqCst),
                handshakes,
                "scalar invalidation alone renewed a healthy channel"
            );
            assert_eq!(old_queries.count.load(Ordering::SeqCst), 1);
            drained(backend, ipc).await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn delayed_old_grant_cannot_replace_newer_authority_or_acknowledgment() -> Result<()> {
    for pinned in [false, true] {
        let (_owner, root, scope, backend) = fixture(pinned)?;
        let old = test_identity(scope.project, 1, 113);
        let newer = test_identity(scope.project, 2, 114);
        let mut ipc = OwnedIpc::new(&root, old)?;
        let old_queries = ipc.add_worker(&root, old)?;
        let new_queries = ipc.add_worker(&root, newer)?;
        let gate = Arc::new(Gate::default());
        *ipc.controls.hold_grant.lock().unwrap() = Some(gate.clone());
        let mut pending = Box::pin(call(&backend, pinned, scope, "airk_read_file"));
        tokio::select! { _=gate.entered.notified()=>{},result=&mut pending=>anyhow::bail!("grant never held: {result:?}"), }
        *ipc.route.lock().unwrap() = newer;
        assert_eq!(
            response_body(&call(&backend, pinned, scope, "airk_status").await?)?["publication"]["token"],
            worker_publication(newer)
        );
        gate.release.notify_one();
        let old_result = tokio::time::timeout(Duration::from_secs(10), &mut pending).await?;
        assert!(
            old_result.is_err()
                || old_result
                    .as_ref()
                    .is_ok_and(|value| value["isError"] == true)
        );
        assert_eq!(
            old_queries.count.load(Ordering::SeqCst),
            0,
            "late grant submitted old evidence"
        );
        assert_eq!(backend.status.receipt(), worker_receipt(newer));
        assert_eq!(
            response_body(&call(&backend, pinned, scope, "airk_read_file").await?)?["publication"]
                ["token"],
            worker_publication(newer)
        );
        assert_eq!(
            new_queries.receipts.lock().unwrap().last().copied(),
            Some(worker_receipt(newer))
        );
        drop(pending);
        drained(backend, ipc).await?;
    }
    Ok(())
}

#[tokio::test]
async fn accepted_old_reply_keeps_its_result_without_restoring_old_worker_receipt() -> Result<()> {
    for pinned in [false, true] {
        let (_owner, root, scope, backend) = fixture(pinned)?;
        let old = test_identity(scope.project, 1, 115);
        let newer = test_identity(scope.project, 2, 116);
        let mut ipc = OwnedIpc::new(&root, old)?;
        let old_queries = ipc.add_worker(&root, old)?;
        let new_queries = ipc.add_worker(&root, newer)?;
        let gate = Arc::new(Gate::default());
        *old_queries.hold_reply.lock().unwrap() = Some(gate.clone());
        let mut pending = Box::pin(call(&backend, pinned, scope, "airk_read_file"));
        tokio::select! { _=gate.entered.notified()=>{},result=&mut pending=>anyhow::bail!("query never held: {result:?}"), }
        *ipc.route.lock().unwrap() = newer;
        assert_eq!(
            response_body(&call(&backend, pinned, scope, "airk_status").await?)?["publication"]["token"],
            worker_publication(newer)
        );
        gate.release.notify_one();
        let original = tokio::time::timeout(Duration::from_secs(10), &mut pending).await??;
        assert_eq!(
            response_body(&original)?["publication"]["token"],
            worker_publication(old)
        );
        assert_eq!(backend.status.receipt(), worker_receipt(newer));
        assert_eq!(
            old_queries.count.load(Ordering::SeqCst),
            1,
            "accepted original query was replayed"
        );
        assert_eq!(
            response_body(&call(&backend, pinned, scope, "airk_status").await?)?["publication"]["token"],
            worker_publication(newer)
        );
        assert_eq!(
            new_queries.receipts.lock().unwrap().last().copied(),
            Some(worker_receipt(newer))
        );
        drop(pending);
        drained(backend, ipc).await?;
    }
    Ok(())
}

#[tokio::test]
async fn closed_or_replaced_frontend_while_health_waits_never_submits_connect() -> Result<()> {
    for pinned in [false, true] {
        for close in [false, true] {
            let (_owner, root, scope, backend) = fixture(pinned)?;
            let identity = test_identity(scope.project, 1, 117);
            let mut ipc = OwnedIpc::new(&root, identity)?;
            let queries = ipc.add_worker(&root, identity)?;
            let gate = Arc::new(Gate::default());
            *ipc.controls.hold_health.lock().unwrap() = Some(gate.clone());
            let mut pending = Box::pin(call(&backend, pinned, scope, "airk_read_file"));
            tokio::select! { _=gate.entered.notified()=>{},result=&mut pending=>anyhow::bail!("Health never held: {result:?}"), }
            if close {
                airk_mcp::remote::RemoteTools::close(&*backend);
            } else {
                let mut newer = scope;
                newer.namespace = Incarnation::from_bytes([118; 16]);
                if pinned {
                    backend.status.seed(CachedStatus::new(CacheScope::Registered(newer),&airk_mcp::unified::success(serde_json::json!({"index_state":"available","publication":{"token":"replacement"}}))).unwrap());
                } else {
                    backend.status.source_scope(newer);
                }
            }
            gate.release.notify_one();
            let refused = tokio::time::timeout(Duration::from_secs(10), &mut pending).await?;
            assert!(
                refused.is_err() || refused.as_ref().is_ok_and(|value| value["isError"] == true)
            );
            assert_eq!(ipc.controls.health.load(Ordering::SeqCst), 1);
            assert_eq!(
                ipc.controls.connect.load(Ordering::SeqCst),
                0,
                "Connect crossed known close/replacement after Health"
            );
            assert_eq!(queries.count.load(Ordering::SeqCst), 0);
            drop(pending);
            drained(backend, ipc).await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn same_generation_different_worker_is_refused_without_poisoning_known_authority()
-> Result<()> {
    for pinned in [false, true] {
        let (_owner, root, scope, backend) = fixture(pinned)?;
        let original = test_identity(scope.project, 1, 119);
        let conflicting = test_identity(scope.project, 1, 120);
        let mut ipc = OwnedIpc::new(&root, original)?;
        let original_queries = ipc.add_worker(&root, original)?;
        let conflicting_queries = ipc.add_worker(&root, conflicting)?;
        call(&backend, pinned, scope, "airk_read_file").await?;
        *ipc.route.lock().unwrap() = conflicting;
        let refused = call(&backend, pinned, scope, "airk_status").await;
        if let Ok(value) = refused {
            assert_ne!(response_body(&value)?["index_state"], "available");
        }
        assert_eq!(conflicting_queries.count.load(Ordering::SeqCst), 0);
        assert_eq!(backend.status.receipt(), worker_receipt(original));
        *ipc.route.lock().unwrap() = original;
        let connects = ipc.controls.connect.load(Ordering::SeqCst);
        assert_eq!(
            response_body(&call(&backend, pinned, scope, "airk_read_file").await?)?["publication"]
                ["token"],
            worker_publication(original)
        );
        assert_eq!(ipc.controls.connect.load(Ordering::SeqCst), connects);
        assert_eq!(original_queries.count.load(Ordering::SeqCst), 2);
        drained(backend, ipc).await?;
    }
    Ok(())
}

#[tokio::test]
async fn original_source_expiry_during_health_prevents_connect() -> Result<()> {
    let (_owner, root, scope, backend) = fixture(false)?;
    let identity = test_identity(scope.project, 1, 121);
    let mut ipc = OwnedIpc::new(&root, identity)?;
    let queries = ipc.add_worker(&root, identity)?;
    let gate = Arc::new(Gate::default());
    *ipc.controls.hold_health.lock().unwrap() = Some(gate.clone());
    // An actual immutable std::Instant deadline, with ample room to release
    // before the transport's five-second Health read timeout. This is an expiry
    // boundary fixture, not a wall-time performance assertion or clock mock.
    let expires = Instant::now() + Duration::from_secs(2);
    let mut pending = Box::pin(backend.call_observed_until(
        "airk_read_file".into(),
        Default::default(),
        scope,
        expires,
    ));
    tokio::select! { _=gate.entered.notified()=>{},result=&mut pending=>anyhow::bail!("Health never held: {result:?}"), }
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        expires + Duration::from_millis(1),
    ))
    .await;
    gate.release.notify_one();
    let refused = tokio::time::timeout(Duration::from_secs(10), &mut pending).await?;
    assert!(refused.is_err() || refused.as_ref().is_ok_and(|value| value["isError"] == true));
    assert_eq!(ipc.controls.health.load(Ordering::SeqCst), 1);
    assert_eq!(
        ipc.controls.connect.load(Ordering::SeqCst),
        0,
        "Connect renewed an expired source observation after Health"
    );
    assert_eq!(queries.count.load(Ordering::SeqCst), 0);
    drop(pending);
    drained(backend, ipc).await?;
    Ok(())
}

#[tokio::test]
async fn simultaneous_cold_evidence_calls_share_the_first_authenticated_binding() -> Result<()> {
    for pinned in [false, true] {
        let (_owner, root, scope, backend) = fixture(pinned)?;
        let identity = test_identity(scope.project, 1, 123);
        let mut ipc = OwnedIpc::new(&root, identity)?;
        let queries = ipc.add_worker(&root, identity)?;
        let gate = Arc::new(Gate::default());
        *ipc.controls.hold_health.lock().unwrap() = Some(gate.clone());
        let mut first = Box::pin(call(&backend, pinned, scope, "airk_read_file"));
        tokio::select! { _=gate.entered.notified()=>{}, result=&mut first=>anyhow::bail!("cold Health never held: {result:?}") }
        // Poll both siblings while the first owns setup, before ANY worker
        // authority is known. No sleep or scheduling assumption creates the race.
        let mut second = Box::pin(call(&backend, pinned, scope, "airk_read_file"));
        let mut third = Box::pin(call(&backend, pinned, scope, "airk_read_file"));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(second.as_mut().poll(&mut context).is_pending());
        assert!(third.as_mut().poll(&mut context).is_pending());
        assert_eq!(backend.state_waiters.available_permits(), 128);
        gate.release.notify_one();
        let (first, second, third) = tokio::join!(first, second, third);
        for reply in [first?, second?, third?] {
            assert_ne!(reply["isError"], true, "{reply}");
            assert_eq!(
                response_body(&reply)?["publication"]["token"],
                worker_publication(identity)
            );
        }
        assert_eq!(queries.count.load(Ordering::SeqCst), 3);
        assert_eq!(ipc.controls.health.load(Ordering::SeqCst), 3);
        assert_eq!(ipc.controls.connect.load(Ordering::SeqCst), 3);
        assert_eq!(backend.state_waiters.available_permits(), 128);
        drained(backend, ipc).await?;
    }
    Ok(())
}

#[tokio::test]
async fn queued_known_authority_is_not_promoted_across_a_worker_replacement() -> Result<()> {
    for pinned in [false, true] {
        let (_owner, root, scope, backend) = fixture(pinned)?;
        let old = test_identity(scope.project, 1, 124);
        let newer = test_identity(scope.project, 2, 125);
        let mut ipc = OwnedIpc::new(&root, old)?;
        let old_queries = ipc.add_worker(&root, old)?;
        let new_queries = ipc.add_worker(&root, newer)?;
        call(&backend, pinned, scope, "airk_read_file").await?;
        let gate = Arc::new(Gate::default());
        *old_queries.hold_reply.lock().unwrap() = Some(gate.clone());
        let mut accepted = Box::pin(call(&backend, pinned, scope, "airk_read_file"));
        tokio::select! { _=gate.entered.notified()=>{}, result=&mut accepted=>anyhow::bail!("old query never held: {result:?}") }
        let mut spare = Vec::new();
        for _ in 0..15 {
            spare.push(backend.state.lock().await);
        }
        let mut queued = Box::pin(call(&backend, pinned, scope, "airk_read_file"));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(queued.as_mut().poll(&mut context).is_pending());
        *ipc.route.lock().unwrap() = newer;
        call(&backend, pinned, scope, "airk_status").await?;
        gate.release.notify_one();
        let (original, refused) = tokio::join!(accepted, queued);
        assert_eq!(
            response_body(&original?)?["publication"]["token"],
            worker_publication(old)
        );
        let refused = refused?;
        assert_eq!(
            response_body(&refused)?["error"]["query_submitted"],
            false,
            "{refused}"
        );
        assert_eq!(old_queries.count.load(Ordering::SeqCst), 2);
        assert_eq!(new_queries.count.load(Ordering::SeqCst), 1);
        drop(spare);
        drained(backend, ipc).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn configured_parallel_channels_submit_all_requests_before_any_reply() -> Result<()> {
    for concurrency in [4, 8, 16, 32, 64] {
        for pinned in [false, true] {
            let (_owner, root, scope, backend) = fixture(pinned)?;
            crate::service_resource_policy::save(
                &root,
                &airk_core::service_resources::ResourceConfiguration {
                    query_concurrency: concurrency,
                    ..Default::default()
                },
            )?;
            let identity = test_identity(scope.project, 1, 127);
            let mut ipc = OwnedIpc::new(&root, identity)?;
            let queries = ipc.add_worker(&root, identity)?;
            *queries.barrier.lock().unwrap() =
                Some(Arc::new(tokio::sync::Barrier::new(concurrency)));
            let mut requests = tokio::task::JoinSet::new();
            for _ in 0..concurrency {
                let backend = backend.clone();
                requests
                    .spawn(async move { call(&backend, pinned, scope, "airk_read_file").await });
            }
            while let Some(reply) =
                tokio::time::timeout(Duration::from_secs(10), requests.join_next()).await?
            {
                let reply = reply??;
                assert_ne!(reply["isError"], true, "{concurrency}: {reply}");
                assert_eq!(
                    response_body(&reply)?["publication"]["token"],
                    worker_publication(identity)
                );
            }
            assert_eq!(queries.count.load(Ordering::SeqCst), concurrency);
            assert_eq!(ipc.controls.connect.load(Ordering::SeqCst), concurrency);
            drained(backend, ipc).await?;
        }
    }
    Ok(())
}
