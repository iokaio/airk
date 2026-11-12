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
pub(crate) mod checkpoint;
mod http;
mod journal;
mod lifecycle;
mod registration;
mod resolve;
mod tools_call;

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
    } else if let Some(pending) =
        error.downcast_ref::<crate::project_supervisor::ConnectionSetupPending>()
    {
        pending.code()
    } else if error.is::<airk_core::scoped_reads::ScopeReadRefused>() {
        "scoped_read_unavailable"
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
    http: std::sync::OnceLock<Arc<http::Owner>>,
    checkpoint: Arc<checkpoint::Coordinator>,
    control_root: std::path::PathBuf,
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
    /// The M19 record this instance accepted; advanced after the drain.
    lifecycle: Mutex<Option<airk_core::service_lifecycle::LifecycleRecord>>,
}

/// The real outer NativeRead seam. A fixture may supply the producer for race
/// tests, but admission, framing and byte delivery use the native gate. Only a
/// failure before protected bytes may return to the ordinary failure writer.
async fn deliver_native_read<S, F>(
    stream: &mut S,
    gate: &crate::catalog_admission::NativeCatalogAdmission,
    produce: F,
) -> Result<Option<ControlResponse>>
where
    S: tokio::io::AsyncWrite + Unpin,
    F: std::future::Future<Output = Result<serde_json::Value>>,
{
    let unavailable = || {
        Some(ControlResponse::Failure {
            code: "native_administrative_connection_rebind".into(),
        })
    };
    let Some(ticket) = gate
        .catalog_clock()
        .and_then(|clock| gate.begin_issue(clock.namespace, clock.revision).ok())
    else {
        return Ok(unavailable());
    };
    let Ok(result) = produce.await else {
        return Ok(unavailable());
    };
    // A completed worker reply may be retained here while a policy fence
    // begins. Each resumed write rechecks the original admission clock.
    gate.deliver_issue(
        ticket,
        stream,
        &ControlResponse::NativeRead { result },
        Duration::from_secs(5),
    )
    .await?;
    Ok(None)
}

async fn connection(mut stream: Stream, state: Arc<State>) -> Result<()> {
    let Some((request, length)): Option<(ControlRequest, usize)> = local_ipc::read_message_sized(
        &mut stream,
        Duration::from_secs(5),
        airk_core::service::MAX_CONTROL_REQUEST_BYTES,
    )
    .await?
    else {
        return Ok(());
    };
    // Each method keeps its own request budget (contracts §2.8); a larger
    // frame is dropped unanswered, as an oversize frame always was.
    anyhow::ensure!(
        length <= request.method.request_frame_budget(),
        "invalid native frame length"
    );
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
    let mut shutdown = request.protocol == PROTOCOL_VERSION
        && instance_matches
        && matches!(request.method, ControlMethod::Shutdown);
    let response_budget = request.method.response_frame_budget();
    let _checkpoint_admission = if request.protocol == PROTOCOL_VERSION
        && instance_matches
        && !matches!(
            request.method,
            ControlMethod::Health
                | ControlMethod::HttpEndpoint
                | ControlMethod::Shutdown
                | ControlMethod::PrepareUpgradeCheckpoint { .. }
                | ControlMethod::ObserveUpgradeCheckpoint { .. }
                | ControlMethod::UpgradeRecoveryProbe { .. }
                | ControlMethod::ReleaseUpgradeAdmission { .. }
        )
        && !(matches!(request.method, ControlMethod::RequestEpoch)
            && state.checkpoint.recovery_probe(state.instance)?)
    {
        match state.checkpoint.admit() {
            Ok(guard) => Some(guard),
            Err(_) => {
                local_ipc::write_message(
                    &mut stream,
                    &ControlResponse::Failure {
                        code: "runtime_upgrade_in_progress".into(),
                    },
                    Duration::from_secs(5),
                )
                .await?;
                return Ok(());
            }
        }
    } else {
        None
    };
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
            ControlMethod::HttpEndpoint => ControlResponse::HttpEndpoint {
                endpoint: state
                    .http
                    .get()
                    .map(|owner| owner.descriptor().map(Box::new))
                    .transpose()?,
            },
            ControlMethod::HttpEnroll {
                connection_id,
                selection,
                client,
                explorer_descriptor,
                reattach_connection_id,
            } => match state.http.get() {
                Some(owner) => match owner
                    .enroll(
                        &state.control_root,
                        connection_id,
                        *selection,
                        explorer_descriptor.map(std::path::PathBuf::from),
                        client,
                        reattach_connection_id,
                    )
                    .await
                {
                    Ok(connection) => ControlResponse::HttpConnection {
                        connection: Box::new(connection),
                    },
                    Err(_) => ControlResponse::Failure {
                        code: "http_enrollment_refused".into(),
                    },
                },
                None => ControlResponse::Failure {
                    code: "http_catalog_not_ready".into(),
                },
            },
            ControlMethod::HttpRevoke { connection_id } => match state.http.get() {
                Some(owner) => match owner.revoke(connection_id).await {
                    Ok(()) => ControlResponse::HttpRevoked { connection_id },
                    Err(_) => ControlResponse::Failure {
                        code: "http_revocation_refused".into(),
                    },
                },
                None => ControlResponse::Failure {
                    code: "http_catalog_not_ready".into(),
                },
            },
            ControlMethod::UpgradeRecoveryProbe { operation_id } => {
                state.checkpoint.validate_recovery(&state, operation_id)?
            }
            ControlMethod::ReleaseUpgradeAdmission { operation_id } => {
                state
                    .checkpoint
                    .release(state.clone(), operation_id)
                    .await?;
                ControlResponse::UpgradeAdmissionReleased { operation_id }
            }
            ControlMethod::PrepareUpgradeCheckpoint {
                operation_id,
                candidate_execution_sha256,
            } => {
                state
                    .checkpoint
                    .prepare(state.clone(), operation_id, candidate_execution_sha256)
                    .await?;
                if let Some(owner) = state.http.get() {
                    owner.update_state()?;
                }
                ControlResponse::UpgradeCheckpoint {
                    observation: Box::new(state.checkpoint.prepared(&state, operation_id)?),
                }
            }
            ControlMethod::ObserveUpgradeCheckpoint { operation_id } => {
                ControlResponse::UpgradeCheckpoint {
                    observation: Box::new(state.checkpoint.observe(&state, operation_id).await?),
                }
            }
            ControlMethod::ValidateCombinedDelivery {
                selection,
                query_selection,
                scope_identity,
                result_digest,
            } => {
                let Some(gate) = state.catalog_gate.as_ref() else {
                    return local_ipc::write_message(
                        &mut stream,
                        &ControlResponse::Failure {
                            code: "combined_delivery_scope_unavailable".into(),
                        },
                        Duration::from_secs(5),
                    )
                    .await;
                };
                let Some(clock) = gate.catalog_clock() else {
                    return local_ipc::write_message(
                        &mut stream,
                        &ControlResponse::Failure {
                            code: "combined_delivery_scope_unavailable".into(),
                        },
                        Duration::from_secs(5),
                    )
                    .await;
                };
                let control = state.control_root.clone();
                let expected_revision = clock.revision;
                let observed = tokio::task::spawn_blocking(move || {
                    crate::combined_frontend::validate_delivery(
                        &control,
                        &selection,
                        &query_selection,
                        scope_identity,
                        expected_revision,
                    )
                })
                .await;
                let admitted = match observed {
                    Ok(Ok(())) => gate.begin_issue(clock.namespace, clock.revision),
                    _ => Err(airk_core::scoped_reads::ScopeReadRefused.into()),
                };
                match admitted {
                    Ok(ticket) => {
                        let response = ControlResponse::CombinedDeliveryValidated {
                            receipt: airk_core::service::CombinedDeliveryReceipt {
                                schema: 1,
                                service_instance: state.instance,
                                namespace: clock.namespace,
                                catalog_revision: clock.revision,
                                scope_identity,
                                result_digest,
                            },
                        };
                        // A partial refused write closes this exchange. Never
                        // append an unguarded fallback after protected bytes.
                        return gate
                            .deliver_issue(ticket, &mut stream, &response, Duration::from_secs(5))
                            .await;
                    }
                    Err(_) => ControlResponse::Failure {
                        code: "combined_delivery_scope_expired".into(),
                    },
                }
            }
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
                Some(gate) => {
                    match deliver_native_read(
                        &mut stream,
                        gate,
                        state.native_administrative.read(
                            &state.management,
                            gate,
                            state.native_caller,
                            project,
                            request,
                            usage_client,
                        ),
                    )
                    .await?
                    {
                        None => return Ok(()),
                        Some(failure) => failure,
                    }
                }
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
            ControlMethod::ToolsCatalog {
                detail,
                combined: None,
            } => crate::tool_catalog::response(&detail)?,
            ControlMethod::ToolsCatalog {
                detail,
                combined: Some(request),
            } => tools_call::combined_catalog(&state.control_root, &detail, &request).await,
            ControlMethod::ClientRequestPrepare {
                caller,
                key,
                request,
            } => journal::prepare(&state.control_root, caller, key, *request),
            ControlMethod::ClientRequestList {} => journal::list(&state.control_root),
            ControlMethod::ClientRequestSubmit { nonce } => {
                journal::submit(&state.control_root, nonce).await
            }
            ControlMethod::ClientRequestForget { nonce } => {
                journal::forget(&state.control_root, nonce)
            }
            ControlMethod::PlanRegistration {
                root,
                data_root,
                collection,
                storage,
            } => registration::plan(&root, &data_root, &collection, storage),
            // M9 lands its handler in step 3c.5b; until then the typed refusal.
            ControlMethod::PrepareConfiguration { .. } => ControlResponse::Failure {
                code: "not_yet_served".into(),
            },
            ControlMethod::PrepareStorage { project } => {
                registration::prepare_storage(&state.control_root, project)
            }
            ControlMethod::ResolveProject { target } => {
                resolve::response(&state.control_root, target).await
            }
            ControlMethod::RecordClientError { operation, input } => {
                match crate::service_logs::record_client_error(
                    &state.control_root,
                    &operation,
                    input.as_bytes(),
                ) {
                    Ok(()) => ControlResponse::ClientErrorRecorded {},
                    Err(_) => ControlResponse::Failure {
                        code: "invalid_client_error".into(),
                    },
                }
            }
            ControlMethod::LifecycleStatus => match lifecycle::status(&state.control_root) {
                Ok((explicitly_stopped, pending)) => ControlResponse::LifecycleStatus {
                    explicitly_stopped,
                    pending,
                },
                Err(error) => ControlResponse::Failure {
                    code: lifecycle::failure_code(&error),
                },
            },
            ControlMethod::Lifecycle {
                action,
                caller,
                key,
            } => match lifecycle::accept(
                &state.control_root,
                state.instance,
                &state.executable_sha256,
                action,
                caller,
                key,
            ) {
                Ok(record) => {
                    // A replay for an earlier instance answers its record only.
                    if record.phase == airk_core::service_lifecycle::LifecyclePhase::Draining
                        && record.request.expected_instance == state.instance
                    {
                        *state
                            .lifecycle
                            .lock()
                            .map_err(|_| anyhow::anyhow!("lifecycle state unavailable"))? =
                            Some(record.clone());
                        shutdown = true;
                    }
                    ControlResponse::LifecycleAccepted {
                        record,
                        service_instance: state.instance,
                        process_id: std::process::id(),
                    }
                }
                Err(error) => ControlResponse::Failure {
                    code: lifecycle::failure_code(&error),
                },
            },
            ControlMethod::ToolsCall {
                tool,
                arguments,
                selection,
                response_mode,
                usage_client,
            } => {
                tools_call::response(
                    &state.control_root,
                    tool,
                    arguments,
                    selection,
                    response_mode,
                    usage_client,
                )
                .await
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
    local_ipc::write_message_limited(
        &mut stream,
        &response,
        Duration::from_secs(5),
        response_budget,
    )
    .await?;
    if shutdown {
        state.shutdown.notify_one();
    }
    Ok(())
}

pub async fn serve(root: &Path) -> Result<()> {
    serve_with_idle(root, None).await
}

/// Serve as an admitted private role (split step 3b.1e-2): once this instance
/// holds the single-instance lease, a predecessor's pending lifecycle record
/// is completed and an explicit start clears the saved Stop.
pub async fn serve_admitted(root: &Path, mode: airk_core::install_record::StartMode) -> Result<()> {
    serve_with_admission(root, None, Some(mode)).await
}

/// Exactly one service owns a control root. While a live service holds the
/// single-instance lease, a second `serve`, `managed-serve`, `foreground` or
/// `host` start is refused with the typed `service_already_running` code; the
/// lease and its exclusion are unchanged.
fn acquire_single_instance(root: &Path) -> Result<airk_store::service::ServiceLease> {
    airk_store::service::ServiceLease::acquire(root).map_err(|error| {
        let held = error.chain().any(|cause| {
            matches!(
                cause.downcast_ref::<std::fs::TryLockError>(),
                Some(std::fs::TryLockError::WouldBlock)
            )
        });
        if held {
            error.context(
                "service_already_running: another AIRK service owns this control directory; use the running service (`airk runtime ensure`) instead of starting a second one",
            )
        } else {
            error
        }
    })
}

pub async fn serve_with_idle(root: &Path, idle_grace: Option<Duration>) -> Result<()> {
    serve_with_admission(root, idle_grace, None).await
}

async fn serve_with_admission(
    root: &Path,
    idle_grace: Option<Duration>,
    admission: Option<airk_core::install_record::StartMode>,
) -> Result<()> {
    crate::install_layout::validate_control_root(root)?;
    crate::runtime_install::require_baseline_service(root)?;
    local_ipc::prepare_root(root)?;
    let _ownership = Arc::new(acquire_single_instance(root)?);
    crate::http_endpoint::rotation::require_start_allowed(root)?;
    crate::install_layout::record_control_root(&_ownership)?;
    if let Some(mode) = admission {
        lifecycle::admit_start(root, mode)?;
    }
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
    local_ipc::prepare_control_endpoint(_ownership.root())?;
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
        crate::runtime_metadata::project_worker_hooks(),
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
                        ensure!(
                            catalog.policy_contract == 0,
                            "catalog_authority_contract_unsupported"
                        );
                        legacy_connections = true;
                        None
                    }
                    1 => {
                        ensure!(
                            catalog.activated && catalog.policy_contract <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT,
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
        http: std::sync::OnceLock::new(),
        checkpoint: Arc::new(checkpoint::Coordinator::new(root)?),
        control_root: root.canonicalize()?,
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
        lifecycle: Mutex::new(None),
    });
    let mut http_task = if let Some((owner, task)) = http::Owner::start(&state, _ownership.clone())?
    {
        state
            .http
            .set(owner)
            .map_err(|_| anyhow::anyhow!("HTTP owner already initialized"))?;
        Some(task)
    } else {
        None
    };
    let mut http_maintenance = tokio::time::interval(Duration::from_secs(60));
    http_maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
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
            _ = http_maintenance.tick(), if http_task.is_some() => {
                if let Some(owner) = state.http.get() {
                    let owner = owner.clone();
                    tokio::task::spawn_blocking(move || owner.refresh()).await??;
                }
            },
            outcome = async { http_task.as_mut().expect("guarded HTTP task").await }, if http_task.is_some() => {
                outcome??;
                anyhow::bail!("HTTP owner exited before service shutdown");
            },
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
                if state.checkpoint.has_active_requests() || !state.management.can_idle_exit() || !state.supervisor.can_idle_exit() || !state.sources.can_idle_exit() {
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
    if let Some(owner) = state.http.get() {
        owner.stop()?;
    }
    if let Some(task) = http_task {
        task.await??;
    }
    // All handlers have finite read/write deadlines and hold no project publication state.
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    connections.shutdown().await;
    let accepted = state
        .lifecycle
        .lock()
        .map_err(|_| anyhow::anyhow!("lifecycle state unavailable"))?
        .take();
    drop(state);
    // Supervisors and workers have drained: the accepted record advances.
    if let Some(record) = accepted {
        lifecycle::finish_drain(root, &record)?;
    }
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
fn prebaseline_metadata_method(method: &ControlMethod) -> Result<bool> {
    let encoded = serde_json::to_value(method)?;
    let name = encoded
        .as_str()
        .or_else(|| {
            encoded
                .as_object()
                .and_then(|object| object.keys().next().map(String::as_str))
        })
        .unwrap_or("");
    Ok(matches!(
        name,
        "health" | "shutdown" | "resources" | "projects" | "inactive_projects" | "service_events"
    ))
}

use crate::ipc::control::{registry_busy_retry, request_once};

async fn request_scoped(
    root: &Path,
    method: ControlMethod,
    version: &str,
    expected_instance: Option<airk_core::identity::Incarnation>,
) -> Result<ControlResponse> {
    if !prebaseline_metadata_method(&method)? {
        crate::runtime_install::require_baseline_control(root)?;
    }
    // registry_busy means either an unaccepted metadata queue entry or a
    // classified read-only contention failure. Retain every request identity;
    // uncertain outcomes and other refusals never enter this retry contract.
    registry_busy_retry(|| request_once(root, method.clone(), version, expected_instance)).await
}

/// This transport accepts only the opaque installer proof and a fixed read.
/// Ordinary RequestEpoch, source reads and enrollment retain their baseline gate.
pub(crate) async fn request_initialized_probe(
    permit: &crate::runtime_install::InitializedProbe,
) -> Result<ControlResponse> {
    let (control, version, instance, namespace) = permit.request_identity()?;
    let response = registry_busy_retry(|| {
        request_once(
            control,
            ControlMethod::RequestEpoch,
            version,
            Some(instance),
        )
    })
    .await?;
    ensure!(
        matches!(&response, ControlResponse::RequestEpoch { namespace: observed, .. } if *observed == namespace),
        "clean-break bootstrap registry differs from initialized namespace"
    );
    Ok(response)
}

#[cfg(test)]
mod registry_retry_tests;
#[cfg(test)]
mod single_instance_tests;

#[cfg(test)]
mod connection_failure_tests {
    use super::*;
    use airk_core::{
        catalog_management::{CatalogOperation, CatalogRequest},
        identity::Incarnation,
        index_catalog::{CatalogRevision, GrantId},
        service::RequestKey,
    };
    use tokio::io::AsyncReadExt;

    fn native_read_gate() -> Result<(
        crate::catalog_admission::NativeCatalogAdmission,
        Incarnation,
    )> {
        let namespace = Incarnation::from_bytes([61; 16]);
        let gate = crate::catalog_admission::NativeCatalogAdmission::new(
            Incarnation::from_bytes([62; 16]),
            namespace,
            CatalogRevision::INITIAL,
            1,
        )?;
        Ok((gate, namespace))
    }

    fn close_native_read_gate(
        gate: &crate::catalog_admission::NativeCatalogAdmission,
        namespace: Incarnation,
    ) -> Result<()> {
        // Close the real admission gate before persistence. This test focuses
        // on the shared delivery boundary; subprocess tests cover trim itself.
        gate.close_request_admission(&CatalogRequest {
            caller: Incarnation::from_bytes([63; 16]),
            key: RequestKey {
                namespace,
                epoch: 1,
                nonce: Incarnation::from_bytes([64; 16]),
            },
            expected_catalog_revision: CatalogRevision::INITIAL,
            operation: CatalogOperation::RevokeGrant {
                id: GrantId::from_identity(Incarnation::from_bytes([65; 16])),
                expected_revision: CatalogRevision::INITIAL,
            },
        })?;
        Ok(())
    }

    #[tokio::test]
    async fn outer_native_read_preserves_the_complete_native_envelope() -> Result<()> {
        let (gate, _) = native_read_gate()?;
        let result = serde_json::json!({
            "value":"original café source",
            "publication":{"token":"fixture-revision"},
            "coverage":{"complete":false,"unknown":["physical_cleanup"]},
            "cursor":"fixture-continuation",
            "policy":{"epoch":4,"physical_cleanup_pending":true},
            "retry":{"code":"fixture","details":{"retained":true}}
        });
        let mut expected = Vec::new();
        crate::service_ipc::write_frame(
            &mut expected,
            &ControlResponse::NativeRead {
                result: result.clone(),
            },
        )?;
        let (mut reader, mut writer) = tokio::io::duplex(16);
        let writing = tokio::spawn(async move {
            let outcome = deliver_native_read(&mut writer, &gate, async { Ok(result) }).await;
            drop(writer);
            outcome
        });
        let mut actual = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), reader.read_to_end(&mut actual)).await??;
        assert!(writing.await??.is_none());
        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn outer_native_read_refuses_a_completed_producer_after_native_admission_close()
    -> Result<()> {
        let (gate, namespace) = native_read_gate()?;
        let (started, ready) = tokio::sync::oneshot::channel();
        let (complete, released) = tokio::sync::oneshot::channel();
        let (mut reader, mut writer) = tokio::io::duplex(16);
        let owner = gate.clone();
        let writing = tokio::spawn(async move {
            // The producer is controlled fixture work; the outer handler seam,
            // native ticket, framing and byte transport are production code.
            let outcome = deliver_native_read(&mut writer, &owner, async {
                let _ = started.send(());
                released.await?;
                Ok(serde_json::json!({"forbidden":"old worker result"}))
            })
            .await;
            drop(writer);
            outcome
        });
        ready.await?;
        close_native_read_gate(&gate, namespace)?;
        complete
            .send(())
            .map_err(|_| anyhow::anyhow!("producer fixture disappeared"))?;
        let outcome = writing.await?;
        assert!(
            outcome.is_err(),
            "a post-production fence cannot become an unguarded fallback"
        );
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        assert!(bytes.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn outer_native_read_stops_backpressured_bytes_without_fallback_after_gate_close()
    -> Result<()> {
        let (gate, namespace) = native_read_gate()?;
        let (mut reader, mut writer) = tokio::io::duplex(1);
        let owner = gate.clone();
        let writing = tokio::spawn(async move {
            let outcome = deliver_native_read(&mut writer, &owner, async {
                Ok(serde_json::json!({"forbidden":"old worker result".repeat(64)}))
            })
            .await;
            drop(writer);
            outcome
        });
        let mut prefix = [0; 1];
        reader.read_exact(&mut prefix).await?;
        close_native_read_gate(&gate, namespace)?;
        // Releasing actual channel capacity wakes the pending write. Its next
        // poll must recheck the close without appending a fallback frame.
        let mut remainder = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), reader.read_to_end(&mut remainder)).await??;
        assert!(writing.await?.is_err());
        assert!(
            prefix.len() + remainder.len() < 4,
            "even the old frame header must remain incomplete"
        );
        Ok(())
    }

    #[tokio::test]
    async fn outer_native_read_does_not_poll_a_producer_under_closed_admission() -> Result<()> {
        let (gate, namespace) = native_read_gate()?;
        close_native_read_gate(&gate, namespace)?;
        let (mut reader, mut writer) = tokio::io::duplex(16);
        let called = std::sync::atomic::AtomicBool::new(false);
        let fallback = deliver_native_read(&mut writer, &gate, async {
            called.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(serde_json::json!({"forbidden":"unadmitted result"}))
        })
        .await?;
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
        assert!(
            matches!(fallback, Some(ControlResponse::Failure { code }) if code == "native_administrative_connection_rebind")
        );
        drop(writer);
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        assert!(bytes.is_empty());
        Ok(())
    }

    #[test]
    fn authority_refusal_remains_permanent_across_both_connection_routes() {
        let error = anyhow::Error::new(airk_core::scoped_reads::ScopeReadRefused)
            .context("native connection admission");
        for fallback in [
            "project_unavailable_or_busy",
            "scoped_connection_unavailable",
        ] {
            assert_eq!(
                super::connection_failure_code(&error, fallback),
                "scoped_read_unavailable"
            );
            assert_eq!(
                super::connection_failure_code(&anyhow::anyhow!("busy"), fallback),
                fallback
            );
        }
    }
}

#[cfg(test)]
mod connection_setup_contract_tests {
    use super::*;
    #[test]
    fn only_known_typed_pre_query_pending_errors_become_busy_codes() {
        use crate::project_supervisor::ConnectionSetupPending as Pending;
        for pending in [
            Pending::Queue,
            Pending::Draining,
            Pending::ResidentCapacity,
            Pending::RestartBackoff,
        ] {
            let expected = pending.code();
            let error = anyhow::Error::new(pending).context("connection admission");
            assert_eq!(
                connection_failure_code(&error, "scoped_connection_unavailable"),
                expected
            );
            assert_eq!(
                connection_failure_code(
                    &anyhow::anyhow!(expected),
                    "scoped_connection_unavailable"
                ),
                "scoped_connection_unavailable"
            );
        }
        for message in [
            "catalog native outcome unknown",
            "native catalog exchange outcome unknown",
            "project owner role is draining",
            "project restart circuit exhausted",
            "native scoped worker role changed",
        ] {
            assert_eq!(
                connection_failure_code(&anyhow::anyhow!(message), "scoped_connection_unavailable"),
                "scoped_connection_unavailable"
            );
        }
    }
}
