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
mod agent_plane;
mod agents;
pub(crate) mod checkpoint;
mod configuration;
mod explorer;
mod first_start;
mod folders;
mod http;
mod journal;
mod lifecycle;
mod login;
mod open_reads;
mod registration;
mod resolve;
mod rotation;
mod scope_grant;
mod storage;
mod tools_call;
mod usage;
mod window_plane;

fn connection_failure_code<'a>(error: &anyhow::Error, fallback: &'a str) -> &'a str {
    if error
        .downcast_ref::<airk_catalog_control::catalog_coordinator::CatalogRevisionChanged>()
        .is_some()
    {
        "catalog_revision_changed"
    } else if error
        .downcast_ref::<airk_catalog_control::catalog_coordinator::CatalogClockPending>()
        .is_some()
    {
        "catalog_clock_pending"
    } else if let Some(pending) =
        error.downcast_ref::<airk_supervisor::project_supervisor::ConnectionSetupPending>()
    {
        pending.code()
    } else if error.is::<airk_core::scoped_reads::ScopeReadRefused>() {
        "scoped_read_unavailable"
    } else {
        fallback
    }
}

pub use airk_frontend::usage_client::administrative_read;

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
    supervisor: airk_supervisor::project_supervisor::Supervisor,
    sources: airk_supervisor::source_supervisor::SourceSupervisor,
    log: airk_host::service_logs::Log,
    catalog_gate: Option<airk_catalog_control::catalog_admission::NativeCatalogAdmission>,
    legacy_connections: bool,
    native_administrative: crate::native_admin_connections::NativeAdministrativePool,
    native_caller: airk_core::identity::Incarnation,
    /// The M19 record this instance accepted; advanced after the drain.
    lifecycle: Mutex<Option<airk_core::service_lifecycle::LifecycleRecord>>,
    /// W1-W8 and M12 (split step 3f.2): this instance's windows, in memory.
    windows: Mutex<window_plane::WindowPlane>,
    /// Sends `RevokeScope` for the selection scopes of dropped window grants
    /// (selection authority §3.3); present with the catalog gate.
    selection_control: Option<airk_supervisor::project_supervisor::CatalogControlHandle>,
    /// A1 launch codes and admin sessions (split Phase 5), in memory.
    admin: Arc<airk_https::admin_plane::AdminPlane>,
    /// U105/U106(d): open agent selection leases, in memory.
    agents: Mutex<agent_plane::AgentPlane>,
}

/// The system clock in Unix milliseconds, as the planes read it.
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

/// The agent lease plane with the system clock, OS-random nonces and, with
/// a catalog gate, live registration of each lease's nonce (nested inside
/// the plane's lock, as for windows). Without a gate every lease refuses.
fn agents(
    instance: airk_core::identity::Incarnation,
    gate: Option<airk_catalog_control::catalog_admission::NativeCatalogAdmission>,
) -> agent_plane::AgentPlane {
    agent_plane::AgentPlane::new(
        instance,
        Box::new(unix_ms),
        Box::new(|| new_instance_id().expect("the OS random source is available")),
        Box::new(
            move |selection: &airk_core::scoped_reads::SelectionScopeRef| match &gate {
                Some(gate) => gate.register_selection(selection).map_err(|_| ()),
                None => Err(()),
            },
        ),
    )
}

/// The window plane with the system clock (Unix milliseconds) and the OS
/// random source for window, draft and handoff ids, 256-bit leases and
/// selection grant nonces. W4 is checked by the catalog check it shares with
/// the admin grant (`scope_grant`); the dispatch runs that check outside the
/// `windows` lock (`window_plane::grant_unlocked`), so the inline validator
/// below is reached only by a plane call that bypasses the dispatch. With a
/// catalog gate, a stored grant's nonce is registered live with it, nested
/// inside the windows lock (selection authority §3.7).
fn windows(
    instance: airk_core::identity::Incarnation,
    control: std::path::PathBuf,
    gate: Option<airk_catalog_control::catalog_admission::NativeCatalogAdmission>,
) -> window_plane::WindowPlane {
    let random = || new_instance_id().expect("the OS random source is available");
    let registrar = gate.map(|gate| -> window_plane::SelectionRegistrar {
        Box::new(
            move |selection: &airk_core::scoped_reads::SelectionScopeRef| {
                gate.register_selection(selection).map_err(|_| ())
            },
        )
    });
    window_plane::WindowPlane::new(
        instance,
        Box::new(unix_ms),
        Box::new(move |_| random()),
        Box::new(move || {
            let mut bytes = [0u8; 32];
            bytes[..16].copy_from_slice(random().as_bytes());
            bytes[16..].copy_from_slice(random().as_bytes());
            airk_core::service::WindowLease::from_bytes(bytes)
        }),
        Box::new(move |selection| scope_grant::check(&control, selection)),
    )
    .with_selection_authority(Box::new(random), registrar)
}

/// Owner revocation of dropped window grants (selection authority §3.3,
/// N4): remove each nonce from the gate's live set and mark its
/// installations revoked, under the gate lock only. Returns the pairs that
/// still need a worker `RevokeScope`. Call with no owner lock held (§3.7).
fn revoke_in_gate(
    gate: Option<&airk_catalog_control::catalog_admission::NativeCatalogAdmission>,
    dropped: window_plane::Dropped,
) -> Vec<(
    airk_core::service::WorkerIdentity,
    airk_core::service::ScopedReadRef,
)> {
    revoke_nonces_in_gate(gate, dropped.into_nonces())
}

/// [`revoke_in_gate`] for nonces of any owner plane.
fn revoke_nonces_in_gate(
    gate: Option<&airk_catalog_control::catalog_admission::NativeCatalogAdmission>,
    nonces: Vec<airk_core::identity::Incarnation>,
) -> Vec<(
    airk_core::service::WorkerIdentity,
    airk_core::service::ScopedReadRef,
)> {
    match gate {
        Some(gate) if !nonces.is_empty() => gate.revoke_selection(&nonces).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// [`revoke_in_gate`], then `RevokeScope` to each affected worker on a
/// blocking thread (it waits for each acknowledgement). A revoked nonce is
/// no longer live, so its scope is unusable even before the worker answers.
fn revoke_dropped(state: &State, dropped: window_plane::Dropped) {
    revoke_scopes(state, revoke_in_gate(state.catalog_gate.as_ref(), dropped));
}

/// [`revoke_dropped`] for the agent plane's dropped leases.
fn revoke_agent_dropped(state: &State, dropped: agent_plane::Dropped) {
    revoke_scopes(
        state,
        revoke_nonces_in_gate(state.catalog_gate.as_ref(), dropped.into_nonces()),
    );
}

/// Drop every agent lease of `connection` (its defaults changed or it was
/// revoked) and revoke their nonces with the plane's lock released.
fn revoke_agent_connection(state: &State, connection: airk_core::identity::Incarnation) {
    let dropped = match state.agents.lock() {
        Ok(mut agents) => agents.revoke_connection(connection),
        Err(_) => return,
    };
    revoke_agent_dropped(state, dropped);
}

/// `RevokeScope` for each pair on a blocking thread.
fn revoke_scopes(
    state: &State,
    pairs: Vec<(
        airk_core::service::WorkerIdentity,
        airk_core::service::ScopedReadRef,
    )>,
) {
    if let (false, Some(control)) = (pairs.is_empty(), state.selection_control.clone()) {
        drop(tokio::task::spawn_blocking(move || {
            control.revoke_selection_scopes(pairs)
        }));
    }
}

/// The A2 grants' side of the scoped-read admission gate (selection
/// authority §3.1, §3.3): a stored grant's nonce is registered live (nested
/// inside the admin plane's lock), and the nonces of dropped grants are
/// revoked after that lock is released, with `RevokeScope` to each affected
/// worker on a blocking thread, as for windows.
fn admin_selection_authority(
    gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    control: Option<airk_supervisor::project_supervisor::CatalogControlHandle>,
) -> airk_https::admin_plane::SelectionAuthority {
    let register = gate.clone();
    airk_https::admin_plane::SelectionAuthority {
        register: Arc::new(
            move |selection: &airk_core::scoped_reads::SelectionScopeRef| {
                register.register_selection(selection).is_ok()
            },
        ),
        revoke: Arc::new(move |nonces: Vec<airk_core::identity::Incarnation>| {
            let pairs = gate.revoke_selection(&nonces).unwrap_or_default();
            if let (false, Some(control)) = (pairs.is_empty(), control.clone())
                && let Ok(runtime) = tokio::runtime::Handle::try_current()
            {
                drop(runtime.spawn_blocking(move || control.revoke_selection_scopes(pairs)));
            }
        }),
    }
}

/// The W4 catalog check off the async workers.
async fn blocking_scope_check(
    control: std::path::PathBuf,
    selection: airk_core::service::WindowScopeSelection,
) -> window_plane::CheckResult {
    tokio::task::spawn_blocking(move || scope_grant::check(&control, &selection))
        .await
        .unwrap_or_else(|_| Err(scope_grant::SCOPE_UNAVAILABLE.to_owned()))
}

/// The admin plane's A2 read (selection authority S2): the W4 selection read
/// path under the session's grant, in process.
fn admin_selection_reader(
    state: Arc<std::sync::OnceLock<std::sync::Weak<State>>>,
) -> airk_https::admin_plane::SelectionReader {
    Arc::new(move |read: airk_https::admin_plane::SelectedRead| {
        let state = state.clone();
        Box::pin(async move {
            let Some(state) = state.get().and_then(std::sync::Weak::upgrade) else {
                return ControlResponse::Failure {
                    code: "service_draining".into(),
                };
            };
            let grant = tools_call::SelectionGrant {
                selection: read.selection,
                scope_identity: read.scope_identity,
                selection_ref: read.selection_ref,
            };
            match tools_call::selection_read(
                &state,
                &grant,
                read.tool,
                read.arguments,
                read.mode,
                read.client,
            )
            .await
            {
                Ok(result) => ControlResponse::ToolResult { result },
                Err(tools_call::SelectionRead::Refused(code)) => {
                    ControlResponse::Failure { code: code.into() }
                }
                Err(tools_call::SelectionRead::Failed(error)) => tools_call::failed(&error),
            }
        })
    })
}

/// The admin plane's A2 catalog check: the shared `scope_grant` check, off
/// the async workers.
fn admin_scope_check(control: std::path::PathBuf) -> airk_https::admin_plane::ScopeCheck {
    Arc::new(move |selection: airk_core::service::WindowScopeSelection| {
        let control = control.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || scope_grant::check(&control, &selection))
                .await
                .unwrap_or_else(|_| Err(scope_grant::SCOPE_UNAVAILABLE.to_owned()))
        })
    })
}

/// The admin plane's `https.trust_status`: the status `airk runtime
/// http-trust --action inspect` printed, through the same callers
/// (`http_endpoint::trust::saved_identity` and `manage(Inspect)`), which
/// read the saved endpoint and the OS trust store and change nothing.
fn admin_trust_inspect(control: std::path::PathBuf) -> airk_https::admin_plane::TrustInspect {
    Arc::new(move || {
        let control = control.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
                let (binding, endpoint) =
                    airk_endpoint::http_endpoint::trust::saved_identity(&control)?;
                let status = airk_endpoint::http_endpoint::trust::manage(
                    &control.join("http-1"),
                    binding,
                    endpoint.ca_sha256,
                    airk_endpoint::http_endpoint::trust::Action::Inspect,
                )?;
                Ok(serde_json::to_value(status)?)
            })
            .await?
        })
    })
}

/// The real outer NativeRead seam. A fixture may supply the producer for race
/// tests, but admission, framing and byte delivery use the native gate. Only a
/// failure before protected bytes may return to the ordinary failure writer.
async fn deliver_native_read<S, F>(
    stream: &mut S,
    gate: &airk_catalog_control::catalog_admission::NativeCatalogAdmission,
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
                    Ok(()) => {
                        revoke_agent_connection(&state, connection_id);
                        ControlResponse::HttpRevoked { connection_id }
                    }
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
                    airk_frontend::combined_frontend::validate_delivery(
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
                        build: airk_supervisor::runtime_metadata::process().build,
                        build_error: airk_supervisor::runtime_metadata::process().metadata_error,
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
                connection_id: None,
            } => airk_frontend::tool_catalog::response(&detail)?,
            ControlMethod::ToolsCatalog {
                detail,
                combined: Some(request),
                connection_id: None,
            } => tools_call::combined_catalog(&state.control_root, &detail, &request).await,
            // U105: an open connection's catalog follows its agent choice.
            ControlMethod::ToolsCatalog {
                detail,
                combined: None,
                connection_id: Some(connection_id),
            } => open_reads::catalog(&state, &detail, connection_id).await,
            ControlMethod::ToolsCatalog {
                combined: Some(_),
                connection_id: Some(_),
                ..
            } => ControlResponse::Failure {
                code: "invalid_tool_catalog_selection".into(),
            },
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
            // M10: the report `model-catalog` printed, unchanged.
            ControlMethod::ModelCatalog {} => {
                match airk_kernel::inspect_model_catalog().and_then(|catalog| {
                    let mut document = serde_json::to_value(catalog)?;
                    document["bundled_model"] =
                        serde_json::json!({"manifest": airk_host::bundled_assets::manifest()?});
                    Ok(document)
                }) {
                    Ok(document) => ControlResponse::ModelCatalog { document },
                    Err(error) => ControlResponse::UsageReadFailed {
                        message: tools_call::failure_message(&error),
                        native: None,
                    },
                }
            }
            // W4 (U77, selection authority S1b): the catalog check runs with
            // the windows lock released; the stored grant's nonce is
            // registered live, and every dropped nonce is revoked after the
            // lock is released (§3.7).
            ControlMethod::WindowScopeGrant {
                window_id,
                lease,
                selection,
            } => {
                let control = state.control_root.clone();
                let (response, dropped) = window_plane::grant_unlocked(
                    &state.windows,
                    &window_id,
                    &lease,
                    &selection,
                    |selection| blocking_scope_check(control, selection),
                )
                .await;
                revoke_dropped(&state, dropped);
                response
            }
            // A2 belongs to an admin session, which the pipe never has; the
            // admin plane serves it without this endpoint.
            ControlMethod::AdminScopeGrant { .. } => ControlResponse::Failure {
                code: "admin_session_required".into(),
            },
            // W1-W3, W4b, W5-W8 and M12 (split step 3f.2): in memory.
            method @ (ControlMethod::WindowOpen { .. }
            | ControlMethod::WindowRenew { .. }
            | ControlMethod::WindowClose { .. }
            | ControlMethod::WindowScopeRelease { .. }
            | ControlMethod::AssistanceWindows {}
            | ControlMethod::AssistanceSubmit { .. }
            | ControlMethod::AssistanceDrain { .. }
            | ControlMethod::HandoffPublish { .. }
            | ControlMethod::ExplorerHandoff { .. }) => {
                let dispatched = match state.windows.lock() {
                    Ok(mut windows) => windows.dispatch(&method),
                    Err(_) => None,
                };
                match dispatched {
                    Some((response, dropped)) => {
                        revoke_dropped(&state, dropped);
                        response
                    }
                    None => ControlResponse::Failure {
                        code: "window_plane_unavailable".into(),
                    },
                }
            }
            ControlMethod::ExplorerOffline {
                operation,
                arguments,
                document,
                project,
            } => explorer::run(operation, arguments, document, project),
            ControlMethod::UsageDocument {
                project,
                catalog,
                read,
            } => usage::document(&state.control_root, project, catalog, read).await,
            ControlMethod::PrepareConfiguration { project, draft } => {
                configuration::prepare(&state.control_root, project, draft).await
            }
            ControlMethod::PrepareStorage { project } => {
                registration::prepare_storage(&state.control_root, project)
            }
            ControlMethod::ResolveProject { target } => {
                resolve::response(&state.control_root, target).await
            }
            ControlMethod::RecordClientError { operation, input } => {
                match airk_host::service_logs::record_client_error(
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
            ControlMethod::AgentEndpoint {} => agents::endpoint(&state.control_root).await,
            ControlMethod::AgentConnectionInfo { connection_id } => {
                agents::connection(&state.control_root, connection_id).await
            }
            ControlMethod::AgentConnectionOwnership { connection_id } => {
                agents::ownership(&state.control_root, connection_id).await
            }
            ControlMethod::AgentConnectionVerify { connection_id } => {
                agents::verify(&state.control_root, connection_id).await
            }
            ControlMethod::AgentConnectionDefaults {
                connection_id,
                expected_defaults_revision,
                index_ids,
                index_set_ids,
                agent_choice,
            } => {
                open_reads::defaults(
                    &state,
                    connection_id,
                    airk_endpoint::http_endpoint::OpenDefaultsChange {
                        expected_defaults_revision,
                        index_ids,
                        index_set_ids,
                        agent_choice,
                    },
                )
                .await
            }
            ControlMethod::HttpIssuerRotation { action } => {
                rotation::response(&state.control_root, action).await
            }
            // A1 (contracts §2.7): owner pipe only; the code is in this reply only.
            ControlMethod::AdminLaunchCode {} => {
                match state.http.get().map(|owner| owner.descriptor()) {
                    Some(Ok(descriptor)) => state.admin.launch_code(descriptor.url.port()),
                    _ => ControlResponse::Failure {
                        code: "admin_listener_unavailable".into(),
                    },
                }
            }
            // M22/M23 (U100) and M21 (U101): file system and OS work off the
            // async workers; a failure carries its leading code.
            ControlMethod::StorageInventory {} => {
                let control = state.control_root.clone();
                let answer = tokio::task::spawn_blocking(move || {
                    storage::inventory(&storage::served_locations(&control)?)
                })
                .await;
                match answer {
                    Ok(Ok(inventory)) => ControlResponse::StorageInventory { inventory },
                    Ok(Err(error)) => ControlResponse::Failure {
                        code: storage::failure_code(&error, "storage_unavailable"),
                    },
                    Err(_) => ControlResponse::Failure {
                        code: "storage_unavailable".into(),
                    },
                }
            }
            ControlMethod::StoragePurgePlan { kind } => {
                let (control, log) = (state.control_root.clone(), state.log.clone());
                let answer = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
                    let plan =
                        storage::purge_plan(kind, &storage::served_locations(&control)?, &log)?;
                    let digest = plan.digest()?;
                    Ok((plan, digest))
                })
                .await;
                match answer {
                    Ok(Ok((plan, plan_digest))) => {
                        ControlResponse::StoragePurgePlan { plan, plan_digest }
                    }
                    Ok(Err(error)) => ControlResponse::Failure {
                        code: storage::failure_code(&error, "storage_unavailable"),
                    },
                    Err(_) => ControlResponse::Failure {
                        code: "storage_unavailable".into(),
                    },
                }
            }
            ControlMethod::StoragePurge { kind, plan_digest } => {
                let (control, log) = (state.control_root.clone(), state.log.clone());
                let answer = tokio::task::spawn_blocking(move || {
                    storage::purge(
                        kind,
                        plan_digest,
                        &storage::served_locations(&control)?,
                        &log,
                    )
                })
                .await;
                match answer {
                    Ok(Ok(receipt)) => ControlResponse::StoragePurged { receipt },
                    Ok(Err(error)) => ControlResponse::Failure {
                        code: storage::failure_code(&error, "storage_unavailable"),
                    },
                    Err(_) => ControlResponse::Failure {
                        code: "storage_unavailable".into(),
                    },
                }
            }
            ControlMethod::LoginStartup { action } => {
                let control = state.control_root.clone();
                let answer =
                    tokio::task::spawn_blocking(move || login::login_startup(action, &control))
                        .await;
                match answer {
                    Ok(Ok(login)) => ControlResponse::LoginStartup { login },
                    Ok(Err(error)) => ControlResponse::Failure {
                        code: storage::failure_code(&error, login::UNAVAILABLE),
                    },
                    Err(_) => ControlResponse::Failure {
                        code: login::UNAVAILABLE.into(),
                    },
                }
            }
            // M24: folders only, off the async workers; a failure carries its
            // leading code (`folder_not_found`, `folder_link_not_followed`, …).
            ControlMethod::FolderBrowse {
                path,
                show_hidden,
                after,
            } => {
                let control = state.control_root.clone();
                let answer = tokio::task::spawn_blocking(move || {
                    folders::browse(&control, path.as_deref(), show_hidden, after.as_deref())
                })
                .await;
                match answer {
                    Ok(Ok(listing)) => ControlResponse::FolderListing { listing },
                    Ok(Err(error)) => ControlResponse::Failure {
                        code: storage::failure_code(&error, "folder_unavailable"),
                    },
                    Err(_) => ControlResponse::Failure {
                        code: "folder_unavailable".into(),
                    },
                }
            }
            ControlMethod::AbandonLifecycle { id } => {
                match lifecycle::abandon(&state.control_root, state.instance, id) {
                    Ok(record) => ControlResponse::LifecycleAbandoned { record },
                    Err(error) => ControlResponse::Failure {
                        code: lifecycle::failure_code(&error),
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
            // M3 under a window's W4 grant (selection authority §3.2).
            ControlMethod::ToolsCall {
                tool,
                arguments,
                selection: airk_core::service::ToolSelection::Window { window_id, lease },
                response_mode,
                usage_client,
            } => {
                tools_call::window_response(
                    &state,
                    window_id,
                    lease,
                    tool,
                    arguments,
                    response_mode,
                    usage_client,
                )
                .await
            }
            // U105: an open read (a connection's default scope, or named
            // indexes and sets), leased through the agent plane.
            ControlMethod::ToolsCall {
                tool,
                arguments,
                selection:
                    airk_core::service::ToolSelection::Open {
                        index_ids,
                        index_set_ids,
                        connection_id,
                    },
                response_mode,
                usage_client,
            } => {
                open_reads::response(
                    &state,
                    open_reads::OpenCall {
                        connection_id,
                        index_names: index_ids,
                        set_names: index_set_ids,
                        tool,
                        arguments,
                        mode: response_mode,
                        client: usage_client,
                    },
                )
                .await
            }
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
    deliver_control_response(
        &state.shutdown,
        shutdown,
        local_ipc::write_message_limited(
            &mut stream,
            &response,
            Duration::from_secs(5),
            response_budget,
        ),
    )
    .await
}

/// Accepted shutdown belongs to the service, independently of reply delivery.
async fn deliver_control_response(
    shutdown: &Notify,
    requested_shutdown: bool,
    write: impl std::future::Future<Output = Result<()>>,
) -> Result<()> {
    if requested_shutdown {
        shutdown.notify_one();
    }
    write.await
}

/// The admin plane's calls reach this instance's own control endpoint, so
/// they run the same canonical handlers as every pipe client.
fn admin_dispatch(
    control: std::path::PathBuf,
    instance: airk_core::identity::Incarnation,
) -> airk_https::admin_plane::Dispatch {
    Arc::new(move |method: ControlMethod| {
        let control = control.clone();
        Box::pin(async move { request_for_instance(&control, method, instance).await })
    })
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
    airk_host::install_layout::validate_control_root(root)?;
    airk_installer::runtime_install::require_baseline_service(root)?;
    local_ipc::prepare_root(root)?;
    let lease = acquire_single_instance(root)?;
    airk_endpoint::http_endpoint::rotation::require_start_allowed(root)?;
    airk_host::install_layout::record_control_root(&lease)?;
    // An installed (admitted) start gives a control root without a catalog its
    // fresh Genesis first: before the lifecycle and stop checks open the
    // registry (Genesis fences ordinary admission until it settles, so an
    // interrupted one is only completed here) and before any supervisor, log
    // or registry actor starts. Development `serve` roots keep running
    // without one.
    let lease = if admission.is_some() {
        first_start::ensure_catalog(lease)?
    } else {
        lease
    };
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
    let _ownership = Arc::new(lease);
    _ownership.prepare_control_endpoint()?;
    local_ipc::prepare_control_endpoint(_ownership.root())?;
    let instance = new_instance_id()?;
    let (log, log_guard) = airk_host::service_logs::Log::start(root.canonicalize()?, instance);
    log.record(airk_core::service_logs::LogKind::ServiceStarted, None, None);
    let policy = Arc::new(Mutex::new(Broker::new(instance, BrokerLimits::service())?));
    let sources = airk_supervisor::source_supervisor::SourceSupervisor::start(
        root.canonicalize()?,
        instance,
        log.clone(),
    )?;
    let supervisor = airk_supervisor::project_supervisor::Supervisor::start(
        root.canonicalize()?,
        instance,
        policy.clone(),
        sources.process_id.clone(),
        sources.telemetry_complete.clone(),
        log.clone(),
        _ownership.clone(),
        airk_supervisor::runtime_metadata::project_worker_hooks(),
    )?;
    let mut legacy_connections = false;
    let catalog_gate = match airk_store::service::registry::ServiceRegistry::open(root, instance) {
        Ok(registry) => {
            match registry.catalog_state()? {
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
                            Some(airk_catalog_control::catalog_admission::NativeCatalogAdmission::new(
                            instance,
                            catalog.namespace,
                            catalog.revision,
                            1,
                        )?)
                        }
                        _ => anyhow::bail!("catalog_authority_contract_unsupported"),
                    }
                }
            }
        }
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
    // The same attached port, for owner revocation of window grants.
    let selection_control = catalog_binding
        .as_ref()
        .map(|binding| binding.control.clone());
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
    let plane = windows(instance, root.canonicalize()?, catalog_gate.clone());
    let catalog_gate_for_agents = catalog_gate.clone();
    // The A2 read runs in process; `State` is bound once it exists.
    let reader_state: Arc<std::sync::OnceLock<std::sync::Weak<State>>> =
        Arc::new(std::sync::OnceLock::new());
    let admin = airk_https::admin_plane::AdminPlane::new(
        admin_dispatch(root.canonicalize()?, instance),
        instance,
    )
    .with_scope_check(admin_scope_check(root.canonicalize()?))
    .with_trust_inspect(admin_trust_inspect(root.canonicalize()?));
    let admin = match catalog_gate.clone() {
        Some(gate) => admin
            .with_selection_authority(admin_selection_authority(gate, selection_control.clone()))
            .with_selection_reader(admin_selection_reader(reader_state.clone())),
        None => admin,
    };
    let state = Arc::new(State {
        http: std::sync::OnceLock::new(),
        checkpoint: Arc::new(checkpoint::Coordinator::new(root)?),
        control_root: root.canonicalize()?,
        instance,
        policy: policy.clone(),
        started: Instant::now(),
        last_client_activity: Mutex::new(Instant::now()),
        user: local_ipc::current_user()?,
        executable_sha256: airk_host::runtime_pair::current_executable_sha256()?,
        runtime_sha256: airk_installer::runtime_install::installed_identity(
            &std::env::current_exe()?,
        )?,
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
        windows: Mutex::new(plane),
        selection_control,
        admin: Arc::new(admin),
        agents: Mutex::new(agents(instance, catalog_gate_for_agents)),
    });
    let _ = reader_state.set(Arc::downgrade(&state));
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
    // Idle agent leases are dropped by every lease call and by this sweep,
    // so an unused nonce never outlives its idle bound by more than a tick.
    let mut agent_sweep = tokio::time::interval(Duration::from_secs(30));
    agent_sweep.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
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
            _ = agent_sweep.tick() => {
                let dropped = state.agents.lock().map(|mut agents| agents.sweep());
                if let Ok(dropped) = dropped {
                    revoke_agent_dropped(&state, dropped);
                }
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
    // Service end (selection authority §3.3): revoke every window grant's
    // nonce before the workers drain; no handler holds the windows lock now.
    let ended = match state.windows.lock() {
        Ok(mut windows) => Some(windows.end()),
        Err(_) => None,
    };
    // Every agent lease ends with the service, as window grants do.
    let agents_ended = state
        .agents
        .lock()
        .map(|mut agents| agents.close().into_nonces())
        .unwrap_or_default();
    // Every admin session and its A2 grant ends with the service; the plane
    // revokes their nonces once its lock is released.
    state.admin.end();
    if let Some(ended) = ended {
        let mut nonces = ended.into_nonces();
        nonces.extend(agents_ended);
        let pairs = revoke_nonces_in_gate(state.catalog_gate.as_ref(), nonces);
        if let (false, Some(control)) = (pairs.is_empty(), state.selection_control.clone()) {
            let _ = tokio::time::timeout(
                Duration::from_secs(5),
                tokio::task::spawn_blocking(move || control.revoke_selection_scopes(pairs)),
            )
            .await;
        }
    }
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

#[cfg(test)]
use crate::ipc::control::registry_busy_retry;
#[cfg(test)]
use airk_installer::control_client::prebaseline_metadata_method;
pub use airk_installer::control_client::{
    request, request_for_instance, request_for_version, request_for_version_and_instance,
};

#[cfg(test)]
mod registry_retry_tests;

#[cfg(test)]
mod response_delivery_tests;
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
        airk_catalog_control::catalog_admission::NativeCatalogAdmission,
        Incarnation,
    )> {
        let namespace = Incarnation::from_bytes([61; 16]);
        let gate = airk_catalog_control::catalog_admission::NativeCatalogAdmission::new(
            Incarnation::from_bytes([62; 16]),
            namespace,
            CatalogRevision::INITIAL,
            1,
        )?;
        Ok((gate, namespace))
    }

    fn close_native_read_gate(
        gate: &airk_catalog_control::catalog_admission::NativeCatalogAdmission,
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
mod window_selection_revocation_tests {
    use super::*;
    use airk_core::{
        identity::{Incarnation, Sha256Digest},
        index_catalog::CatalogRevision,
        scoped_reads::SelectionScopeRef,
        service::{WindowLease, WindowScopeSelection},
    };

    const INSTANCE: [u8; 16] = [0xe1; 16];

    fn gate() -> airk_catalog_control::catalog_admission::NativeCatalogAdmission {
        airk_catalog_control::catalog_admission::NativeCatalogAdmission::new(
            Incarnation::from_bytes(INSTANCE),
            Incarnation::from_bytes([0x0a; 16]),
            CatalogRevision::try_from(12u64).unwrap(),
            1,
        )
        .unwrap()
    }

    /// A plane whose stored grants register with `gate`, as `windows`
    /// wires it, under a fixed clock and fixed identifiers.
    fn plane(
        gate: &airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    ) -> window_plane::WindowPlane {
        let registrar = gate.clone();
        let mut nonces = 0u8;
        window_plane::WindowPlane::new(
            Incarnation::from_bytes(INSTANCE),
            Box::new(|| 0),
            Box::new(|_| Incarnation::from_bytes([0xa0; 16])),
            Box::new(|| WindowLease::from_bytes([0x1a; 32])),
            Box::new(|_: &WindowScopeSelection| Err("unused".into())),
        )
        .with_selection_authority(
            Box::new(move || {
                nonces += 1;
                Incarnation::from_bytes([0x90 + nonces; 16])
            }),
            Some(Box::new(move |selection: &SelectionScopeRef| {
                registrar.register_selection(selection).map_err(|_| ())
            })),
        )
    }

    fn selection() -> WindowScopeSelection {
        serde_json::from_value(serde_json::json!({
            "namespace": "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",
            "expected_catalog_revision": 12,
            "index_ids": ["idx_01010101010101010101010101010101"],
            "project_ids": ["prj_02020202020202020202020202020202"],
        }))
        .unwrap()
    }

    fn selection_ref(nonce: u8) -> SelectionScopeRef {
        SelectionScopeRef {
            owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
            grant_nonce: Incarnation::from_bytes([nonce; 16]),
            service_instance: Incarnation::from_bytes(INSTANCE),
            scope_identity: Sha256Digest::from_bytes([0x5c; 32]),
            catalog_revision: CatalogRevision::try_from(12u64).unwrap(),
        }
    }

    /// A stored grant's nonce is live in the gate (a second registration of
    /// it is refused); replacing and closing revoke the dropped nonces
    /// there, so each can be registered again only after its revocation.
    #[tokio::test]
    async fn dropped_window_grants_are_revoked_in_the_gate() {
        let gate = gate();
        let windows = Mutex::new(plane(&gate));
        let opened = windows
            .lock()
            .unwrap()
            .dispatch(&ControlMethod::WindowOpen {
                label: "a".into(),
                owner_pid: 1,
            });
        assert!(revoke_in_gate(Some(&gate), opened.unwrap().1).is_empty());
        let (id, lease) = (
            Incarnation::from_bytes([0xa0; 16]),
            WindowLease::from_bytes([0x1a; 32]),
        );
        let check = |selection: WindowScopeSelection| {
            std::future::ready(Ok((
                Sha256Digest::from_bytes([0x5c; 32]),
                selection.expected_catalog_revision,
            )))
        };
        let (granted, dropped) =
            window_plane::grant_unlocked(&windows, &id, &lease, &selection(), check).await;
        assert!(matches!(
            granted,
            ControlResponse::WindowScopeGranted { .. }
        ));
        assert!(revoke_in_gate(Some(&gate), dropped).is_empty());
        assert!(
            gate.register_selection(&selection_ref(0x91)).is_err(),
            "live"
        );
        // Replace: the first nonce is dropped and revoked.
        let (_, replaced) =
            window_plane::grant_unlocked(&windows, &id, &lease, &selection(), check).await;
        assert!(gate.register_selection(&selection_ref(0x91)).is_err());
        assert!(revoke_in_gate(Some(&gate), replaced).is_empty());
        assert!(gate.register_selection(&selection_ref(0x91)).is_ok());
        // Close: the second nonce.
        let close = ControlMethod::WindowClose {
            window_id: id,
            lease: lease.clone(),
        };
        let (closed, dropped) = windows.lock().unwrap().dispatch(&close).unwrap();
        assert!(matches!(closed, ControlResponse::WindowClosed {}));
        assert!(gate.register_selection(&selection_ref(0x92)).is_err());
        assert!(revoke_in_gate(Some(&gate), dropped).is_empty());
        assert!(gate.register_selection(&selection_ref(0x92)).is_ok());
        // Without a gate nothing is revoked or sent.
        assert!(revoke_in_gate(None, windows.lock().unwrap().end()).is_empty());
    }
}

#[cfg(test)]
mod connection_setup_contract_tests {
    use super::*;
    #[test]
    fn only_known_typed_pre_query_pending_errors_become_busy_codes() {
        use airk_supervisor::project_supervisor::ConnectionSetupPending as Pending;
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
