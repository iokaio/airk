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

fn connection_failure_code<'a>(error: &anyhow::Error, fallback: &'a str) -> &'a str {
    if error
        .downcast_ref::<crate::catalog_coordinator::CatalogRevisionChanged>()
        .is_some()
    {
        "catalog_revision_changed"
    } else if error
        .downcast_ref::<crate::catalog_coordinator::CatalogClockPending>()
        .is_some()
    {
        "catalog_clock_pending"
    } else {
        fallback
    }
}

/// Administrative metadata transport. Credentials remain inside Runtime and
/// the complete canonical native value/error is preserved for native clients.
pub async fn administrative_read(
    control: &Path,
    project: airk_core::service::ProjectBinding,
    request: airk_core::native_reads::NativeRead,
    client: airk_core::usage::ClientFamily,
) -> Result<serde_json::Value> {
    let response = crate::console_client::request(
        control,
        ControlMethod::NativeRead {
            project,
            request: request.clone(),
            usage_client: client,
        },
    )
    .await?;
    match response {
        ControlResponse::NativeRead { result } => {
            crate::usage_client::require_native_success(result)
        }
        ControlResponse::Failure { code } if code == "native_administrative_legacy" => {
            let mut connection =
                crate::project_client::Connection::connect_as(control, project, client).await?;
            crate::usage_client::require_native_success(connection.read_native(request).await?)
        }
        response @ ControlResponse::Failure { .. } => Err(
            crate::usage_client::NativeReadFailure::from_response(serde_json::to_value(response)?)
                .into(),
        ),
        // Never retain an unexpected credential-plane response as error data.
        _ => anyhow::bail!("unexpected native administrative response"),
    }
}

struct State {
    instance: airk_core::identity::Incarnation,
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
    catalog_gate: Option<crate::catalog_admission::NativeCatalogAdmission>,
    legacy_connections: bool,
    native_administrative: crate::native_admin_connections::NativeAdministrativePool,
    native_caller: airk_core::identity::Incarnation,
}

async fn connection(mut stream: Stream, state: Arc<State>) -> Result<()> {
    let Some(request): Option<ControlRequest> =
        local_ipc::read_message(&mut stream, Duration::from_secs(5)).await?
    else {
        return Ok(());
    };
    let instance_matches = request
        .expected_instance
        .is_none_or(|expected| expected == state.instance);
    if request.protocol == PROTOCOL_VERSION
        && instance_matches
        && !matches!(request.method, ControlMethod::Health)
        && let Ok(mut activity) = state.last_client_activity.lock()
    {
        *activity = Instant::now();
    }
    let shutdown = request.protocol == PROTOCOL_VERSION
        && instance_matches
        && matches!(request.method, ControlMethod::Shutdown);
    let response = if request.protocol != PROTOCOL_VERSION {
        ControlResponse::Incompatible {
            protocol: PROTOCOL_VERSION,
        }
    } else if !instance_matches {
        ControlResponse::Failure {
            code: "service_instance_changed".into(),
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
                        catalog_clock: state
                            .catalog_gate
                            .as_ref()
                            .and_then(|gate| gate.catalog_clock()),
                        storage_formats: Some(airk_core::storage_format::CURRENT),
                        build: crate::runtime_metadata::process().build,
                        build_error: crate::runtime_metadata::process().metadata_error,
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
            ControlMethod::ConnectScoped { request } => {
                match state.management.request_scoped(request).await {
                    Ok(delivery) => {
                        return delivery.deliver(&mut stream, Duration::from_secs(5)).await;
                    }
                    Err(error) => ControlResponse::Failure {
                        code: connection_failure_code(&error, "scoped_connection_unavailable")
                            .into(),
                    },
                }
            }
            ControlMethod::NativeRead { .. }
                if state.catalog_gate.is_none() && state.legacy_connections =>
            {
                ControlResponse::Failure {
                    code: "native_administrative_legacy".into(),
                }
            }
            ControlMethod::NativeRead {
                project,
                request,
                usage_client,
            } => match state.catalog_gate.as_ref() {
                Some(gate) => match state
                    .native_administrative
                    .read(
                        &state.management,
                        gate,
                        state.native_caller,
                        project,
                        request,
                        usage_client,
                    )
                    .await
                {
                    Ok(result) => ControlResponse::NativeRead { result },
                    Err(_) => ControlResponse::Failure {
                        code: "native_administrative_connection_rebind".into(),
                    },
                },
                None => ControlResponse::Failure {
                    code: "native_administrative_admission_unavailable".into(),
                },
            },
            ControlMethod::Connect { project } => {
                if state.catalog_gate.is_some() {
                    match state.management.request_legacy(project).await {
                        Ok(delivery) => {
                            return delivery.deliver(&mut stream, Duration::from_secs(5)).await;
                        }
                        Err(error) => ControlResponse::Failure {
                            code: connection_failure_code(&error, "project_unavailable_or_busy")
                                .into(),
                        },
                    }
                } else if state.legacy_connections {
                    state.supervisor.connect(project).await
                } else {
                    ControlResponse::Failure {
                        code: "catalog_admission_unavailable".into(),
                    }
                }
            }
            ControlMethod::Workers { projects } => state.supervisor.workers_status(&projects),
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
    if let ControlResponse::Failure { code, .. }
    | ControlResponse::CatalogOutcomeUnknown { code, .. }
    | ControlResponse::SourceChangeBusy { code, .. }
    | ControlResponse::SourceChangeResolutionOutcomeUnknown { code, .. } = &response
    {
        state.log.record_code(
            airk_core::service_logs::LogKind::ControlFailed,
            None,
            None,
            Some(code),
        );
    } else if matches!(response, ControlResponse::Incompatible { .. }) {
        state.log.record_code(
            airk_core::service_logs::LogKind::ControlFailed,
            None,
            None,
            Some("runtime_incompatible"),
        );
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
    let _ownership = Arc::new(airk_store::service::ServiceLease::acquire(root)?);
    crate::runtime_install::record_control_root(&_ownership)?;
    if root.join("service-1.4.sqlite3").try_exists()? {
        match airk_store::service::registry::ServiceRegistry::open(root, new_instance_id()?) {
            Ok(registry) => ensure!(
                !registry.service_explicitly_stopped()?,
                "service_explicitly_stopped"
            ),
            Err(error)
                if error.is::<airk_store::service::registry::catalog::AdmissionPending>() =>
            {
                return Err(error);
            }
            // Preserve the existing diagnostic control path for other damaged or
            // unavailable registries. Pending upgrades must start no supervisors.
            Err(_) => {}
        }
    }
    _ownership.prepare_control_endpoint()?;
    local_ipc::prepare_control_endpoint(&_ownership)?;
    let instance = new_instance_id()?;
    let (log, log_guard) = crate::service_logs::Log::start(root.canonicalize()?, instance);
    log.record(airk_core::service_logs::LogKind::ServiceStarted, None, None);
    let policy = Arc::new(Mutex::new(Broker::new(instance, BrokerLimits::service())?));
    let sources = crate::source_supervisor::SourceSupervisor::start(
        root.canonicalize()?,
        instance,
        log.clone(),
    )?;
    let supervisor = crate::project_supervisor::Supervisor::start(
        root.canonicalize()?,
        instance,
        policy.clone(),
        sources.process_id.clone(),
        sources.telemetry_complete.clone(),
        log.clone(),
        _ownership.clone(),
    )?;
    let mut legacy_connections = false;
    let catalog_gate = match airk_store::service::registry::ServiceRegistry::open(root, instance) {
        Ok(registry) => match registry.catalog_state()? {
            None => {
                legacy_connections = true;
                None
            }
            Some(catalog) => {
                let supported = airk_store::service::registry::catalog::capabilities();
                ensure!(
                    supported.supports(
                        catalog.schema,
                        catalog.authority_contract,
                        catalog.policy_contract
                    ),
                    "catalog_authority_contract_unsupported"
                );
                match catalog.authority_contract {
                    0 => {
                        legacy_connections = true;
                        None
                    }
                    1 => {
                        ensure!(
                            catalog.activated && catalog.policy_contract == 0,
                            "catalog_authority_not_ready"
                        );
                        Some(crate::catalog_admission::NativeCatalogAdmission::new(
                            instance,
                            catalog.namespace,
                            catalog.revision,
                            1,
                        )?)
                    }
                    _ => anyhow::bail!("catalog_authority_contract_unsupported"),
                }
            }
        },
        Err(error) if error.is::<airk_store::service::registry::catalog::AdmissionPending>() => {
            return Err(error);
        }
        Err(_) => None,
    };
    let catalog_binding = match catalog_gate.as_ref() {
        Some(gate) => {
            // This records the actual native owner before synchronous Attach.
            // It is not a predecessor-drain proof: Ready/listener still await
            // all exact worker/helper recovery and durable fence restoration.
            airk_store::service::registry::ServiceRegistry::open(root, instance)?
                .record_service_instance(&_ownership, instance)?;
            Some(crate::service_management::CatalogBinding {
                gate: gate.clone(),
                control: supervisor.catalog_control(gate.clone())?,
            })
        }
        None => None,
    };
    let management = crate::service_management::Management::start_with_catalog(
        root.canonicalize()?,
        instance,
        sources.wake_handle(),
        log.clone(),
        _ownership.clone(),
        catalog_binding,
    )?;
    // Publish no control listener until native pending fences have been restored
    // by the accepted metadata owner and its strict Ready handshake completed.
    sources.set_missing_reporter(management.missing_reporter())?;
    let address = local_ipc::endpoint(root, "control")?;
    let mut listener = Listener::bind(&address)?;
    let state = Arc::new(State {
        instance,
        policy: policy.clone(),
        started: Instant::now(),
        last_client_activity: Mutex::new(Instant::now()),
        user: local_ipc::current_user()?,
        executable_sha256: crate::runtime_pair::current_executable_sha256()?,
        runtime_sha256: crate::runtime_install::installed_identity(&std::env::current_exe()?)?,
        shutdown: Notify::new(),
        management,
        supervisor,
        sources,
        log: log.clone(),
        catalog_gate,
        legacy_connections,
        native_administrative: Default::default(),
        native_caller: local_ipc::caller_identity()?,
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
    request_scoped(root, method, version, None).await
}
pub async fn request_for_instance(
    root: &Path,
    method: ControlMethod,
    expected: airk_core::identity::Incarnation,
) -> Result<ControlResponse> {
    request_scoped(root, method, env!("CARGO_PKG_VERSION"), Some(expected)).await
}
pub async fn request_for_version_and_instance(
    root: &Path,
    method: ControlMethod,
    version: &str,
    expected: airk_core::identity::Incarnation,
) -> Result<ControlResponse> {
    request_scoped(root, method, version, Some(expected)).await
}
async fn request_scoped(
    root: &Path,
    method: ControlMethod,
    version: &str,
    expected_instance: Option<airk_core::identity::Incarnation>,
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
            expected_instance,
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
