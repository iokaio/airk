// SPDX-License-Identifier: Apache-2.0
//! Native foreground service control loop, independent of project stores and queries.
use crate::local_ipc::{self, Listener, Stream};
use airk_core::service::{
    ControlMethod, ControlRequest, ControlResponse, PROTOCOL_VERSION, ServiceHealth,
};
use airk_kernel::service_broker::{Broker, BrokerLimits, new_instance_id};
use anyhow::{Result, ensure};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    sync::{Notify, Semaphore},
    task::JoinSet,
};

struct State {
    policy: Arc<Mutex<Broker>>,
    started: Instant,
    last_client_activity: Mutex<Instant>,
    user: String,
    executable_sha256: String,
    runtime_sha256: Option<String>,
    shutdown: Notify,
    management: crate::service_management::Management,
    supervisor: crate::project_supervisor::Supervisor,
    sources: crate::source_supervisor::SourceSupervisor,
    log: crate::service_logs::Log,
}

async fn connection(mut stream: Stream, state: Arc<State>) -> Result<()> {
    let Some(request): Option<ControlRequest> =
        local_ipc::read_message(&mut stream, Duration::from_secs(5)).await?
    else {
        return Ok(());
    };
    if request.protocol == PROTOCOL_VERSION
        && !matches!(request.method, ControlMethod::Health)
        && let Ok(mut activity) = state.last_client_activity.lock()
    {
        *activity = Instant::now();
    }
    let shutdown =
        request.protocol == PROTOCOL_VERSION && matches!(request.method, ControlMethod::Shutdown);
    let response = if request.protocol != PROTOCOL_VERSION {
        ControlResponse::Incompatible {
            protocol: PROTOCOL_VERSION,
        }
    } else {
        match request.method {
            ControlMethod::Health => {
                let now = Instant::now();
                let policy = state
                    .policy
                    .lock()
                    .map_err(|_| anyhow::anyhow!("service policy unavailable"))?
                    .status(now);
                ControlResponse::Health {
                    observation: Box::new(ServiceHealth {
                        logs: state.log.telemetry(),
                        version: env!("CARGO_PKG_VERSION").into(),
                        protocol: PROTOCOL_VERSION,
                        instance: policy.service,
                        process_id: std::process::id(),
                        executable_sha256: state.executable_sha256.clone(),
                        runtime_sha256: state.runtime_sha256.clone(),
                        user: state.user.clone(),
                        uptime_ms: state.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                        policy,
                    }),
                }
            }
            ControlMethod::Shutdown => ControlResponse::Stopping,
            ControlMethod::Connect { project } => state.supervisor.connect(project).await,
            ControlMethod::Worker { project } => state.supervisor.worker_status(project),
            ControlMethod::RestartProject { project, expected } => {
                state.supervisor.restart_project(project, expected).await
            }
            ControlMethod::StartEmbedding => state.supervisor.start_embedding().await,
            ControlMethod::EmbeddingStatus => state.supervisor.embedding_status().await,
            ControlMethod::StopEmbedding { worker, force } => {
                state.supervisor.stop_embedding(worker, force).await
            }
            ControlMethod::SourceStatus => state.sources.status(),
            ControlMethod::Resources => state.supervisor.resource_status(),
            ControlMethod::ConfigureResources { configuration } => {
                state.supervisor.configure_resources(configuration).await
            }
            method => state.management.request(method).await,
        }
    };
    if matches!(
        response,
        ControlResponse::Failure { .. } | ControlResponse::Incompatible { .. }
    ) {
        state
            .log
            .record(airk_core::service_logs::LogKind::ControlFailed, None, None);
    }
    local_ipc::write_message(&mut stream, &response, Duration::from_secs(5)).await?;
    if shutdown {
        state.shutdown.notify_one();
    }
    Ok(())
}

pub async fn serve(root: &Path) -> Result<()> {
    serve_with_idle(root, None).await
}

pub async fn serve_with_idle(root: &Path, idle_grace: Option<Duration>) -> Result<()> {
    crate::runtime_install::validate_control_root(root)?;
    local_ipc::prepare_root(root)?;
    let _ownership = airk_store::service::ServiceLease::acquire(root)?;
    crate::runtime_install::record_control_root(&_ownership)?;
    _ownership.prepare_control_endpoint()?;
    local_ipc::prepare_control_endpoint(&_ownership)?;
    let address = local_ipc::endpoint(root, "control")?;
    let mut listener = Listener::bind(&address)?;
    let instance = new_instance_id()?;
    let (log, log_guard) = crate::service_logs::Log::start(root.canonicalize()?, instance);
    log.record(airk_core::service_logs::LogKind::ServiceStarted, None, None);
    let policy = Arc::new(Mutex::new(Broker::new(instance, BrokerLimits::default())?));
    let sources = crate::source_supervisor::SourceSupervisor::start(
        root.canonicalize()?,
        instance,
        log.clone(),
    )?;
    let state = Arc::new(State {
        policy: policy.clone(),
        started: Instant::now(),
        last_client_activity: Mutex::new(Instant::now()),
        user: local_ipc::current_user()?,
        executable_sha256: crate::runtime_pair::digest_file(&std::env::current_exe()?)?,
        runtime_sha256: crate::runtime_install::installed_identity(&std::env::current_exe()?)?,
        shutdown: Notify::new(),
        management: crate::service_management::Management::start(
            root.canonicalize()?,
            instance,
            sources.wake_handle(),
            log.clone(),
        )?,
        supervisor: crate::project_supervisor::Supervisor::start(
            root.canonicalize()?,
            instance,
            policy,
            sources.process_id.clone(),
            sources.telemetry_complete.clone(),
            log.clone(),
        )?,
        sources,
        log: log.clone(),
    });
    // Initialization hashes the executable and opens native adapters. None of
    // that time is idle service time: begin the grace only when control can run.
    *state
        .last_client_activity
        .lock()
        .map_err(|_| anyhow::anyhow!("service activity unavailable"))? = Instant::now();
    let admission = Arc::new(Semaphore::new(64));
    let mut connections = JoinSet::new();
    let mut idle_tick = tokio::time::interval(Duration::from_secs(1));
    idle_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            stream = listener.accept() => {
                let stream = match stream {
                    Ok(stream) => stream,
                    Err(error) if error.downcast_ref::<local_ipc::PeerRejected>().is_some() => {
                        log.record(airk_core::service_logs::LogKind::PeerRejected,None,None);continue;
                    },
                    Err(error) => return Err(error),
                };
                if let Ok(permit) = admission.clone().try_acquire_owned() {
                    let state = state.clone();
                    let connection_log=log.clone();
                    connections.spawn(async move { let _permit = permit;
                        let result=connection(stream, state).await;
                        if result.is_err(){connection_log.record(airk_core::service_logs::LogKind::ConnectionFailed,None,None);}
                        result
                    });
                } else {log.record(airk_core::service_logs::LogKind::ControlCapacity,None,None);}
            },
            _ = state.shutdown.notified() => {log.record(airk_core::service_logs::LogKind::ShutdownRequested,None,None);break},
            _ = idle_tick.tick(), if idle_grace.is_some() => {
                if !state.management.can_idle_exit() || !state.supervisor.can_idle_exit() || !state.sources.can_idle_exit() {
                    if let Ok(mut activity)=state.last_client_activity.lock() { *activity=Instant::now(); }
                    continue;
                }
                if state.management.can_idle_exit() && state.supervisor.can_idle_exit() && state.sources.can_idle_exit() && connections.is_empty()
                    && state.last_client_activity.lock().is_ok_and(|activity| activity.elapsed() >= idle_grace.unwrap_or(Duration::MAX)) {
                    log.record(airk_core::service_logs::LogKind::IdleExit,None,None);
                    break;
                }
            },
            signal = tokio::signal::ctrl_c() => { signal?; log.record(airk_core::service_logs::LogKind::SignalExit,None,None);break; },
            _ = connections.join_next(), if !connections.is_empty() => {},
        }
    }
    // All handlers have finite read/write deadlines and hold no project publication state.
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    connections.shutdown().await;
    drop(state);
    log.record(airk_core::service_logs::LogKind::ServiceStopped, None, None);
    drop(log_guard);
    Ok(())
}

pub async fn request(root: &Path, method: ControlMethod) -> Result<ControlResponse> {
    request_for_version(root, method, env!("CARGO_PKG_VERSION")).await
}

pub async fn request_for_version(
    root: &Path,
    method: ControlMethod,
    version: &str,
) -> Result<ControlResponse> {
    let response_deadline = if matches!(method, ControlMethod::Connect { .. }) {
        Duration::from_secs(20)
    } else {
        Duration::from_secs(5)
    };
    let address = local_ipc::endpoint(root, "control")?;
    let mut stream =
        tokio::time::timeout(Duration::from_secs(5), local_ipc::connect(&address)).await??;
    local_ipc::write_message(
        &mut stream,
        &ControlRequest {
            protocol: PROTOCOL_VERSION,
            method,
        },
        Duration::from_secs(5),
    )
    .await?;
    let response = local_ipc::read_message(&mut stream, response_deadline)
        .await?
        .ok_or_else(|| anyhow::anyhow!("service disconnected before control response"))?;
    if let ControlResponse::Health { observation } = &response {
        ensure!(
            observation.protocol == PROTOCOL_VERSION
                && observation.version == version
                && observation.user == local_ipc::current_user()?,
            "incompatible native service identity"
        );
    }
    Ok(response)
}
