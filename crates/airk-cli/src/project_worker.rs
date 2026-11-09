// SPDX-License-Identifier: Apache-2.0
//! One lock-bearing project owner, with direct bounded read-only query admission.
use crate::{
    local_ipc::{self, Listener, Stream},
    project_protocol::{self as protocol, Command, Reply},
    service_ipc,
};
use airk_core::service::{ConnectionGrant, JobKind, JobOutcome, MAX_FRAME_BYTES, WorkerIdentity};
use airk_kernel::{
    Kernel,
    service_broker::{Broker, BrokerLimits, SessionPurpose},
};
use airk_store::service::{ProjectLease, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
use std::{
    path::Path,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

// All accepted blocking kernel work finishes before the process exits. Retain the OS
// ownership handle until that exit, including error paths through the async runtime.
static PROCESS_LEASE: OnceLock<Arc<ProjectLease>> = OnceLock::new();

fn retry_semantic_turn<T>(mut turn: impl FnMut() -> Result<T>, budget: Duration) -> Result<T> {
    let deadline = Instant::now() + budget;
    loop {
        if let Err(error) = airk_kernel::service_execution::checkpoint() {
            airk_kernel::service_execution::progress_waiting(None);
            return Err(error);
        }
        match turn() {
            Err(error)
                if Kernel::service_enrichment_contention(&error) && Instant::now() < deadline =>
            {
                airk_kernel::service_execution::progress_waiting(
                    Kernel::service_enrichment_wait_reason(&error),
                );
                // Retained work is fenced/idempotent. Keep the same accepted job
                // and charged grant through temporary read/publication exclusion.
                std::thread::sleep(
                    Duration::from_millis(25)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            result => {
                airk_kernel::service_execution::progress_waiting(None);
                return result;
            }
        }
    }
}

fn semantic_publication<T: PartialEq, R>(
    mut turn: impl FnMut() -> Result<()>,
    mut observe: impl FnMut() -> Result<(T, R)>,
    budget: Duration,
) -> Result<R> {
    let before = retry_semantic_turn(&mut observe, budget)?.0;
    let mut inspect = false;
    let mut contention = None;
    retry_semantic_turn(
        || {
            if !inspect {
                match turn() {
                    Ok(()) => {}
                    Err(error) if Kernel::service_enrichment_contention(&error) => {
                        contention = Some(error)
                    }
                    Err(error) => return Err(error),
                }
                inspect = true;
            }
            // Once publication advanced, no mutation is repeated. This worker is
            // the sole writer, and temporary exclusion never publishes a failure.
            let (publication, result) = observe()?;
            if publication == before
                && let Some(error) = contention.take()
            {
                inspect = false;
                return Err(error);
            }
            Ok(result)
        },
        budget,
    )
}
struct StopOnDrop(Arc<AtomicBool>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
use tokio::{
    sync::{Notify, Semaphore, mpsc},
    task::JoinSet,
};

mod admin;
mod admission;
mod scopes;

struct State {
    control: std::path::PathBuf,
    identity: WorkerIdentity,
    storage: airk_store::service::data_scope::BoundDataRoot,
    broker: Mutex<Broker>,
    tools: airk_mcp::local::LocalTools,
    closing: Arc<AtomicBool>,
    admission: Arc<admission::QueryAdmission>,
    scopes: Arc<Mutex<scopes::Scopes>>,
}

#[derive(Default)]
struct WorkState {
    active: Option<airk_core::identity::Incarnation>,
    finished: Option<airk_core::identity::Incarnation>,
    maintenance: Option<airk_core::service::MaintenanceObservation>,
    failure: Option<crate::service_logs::JobFailure>,
    integrity_observation_failure: Option<String>,
}

fn integrity_observation_code(error: &anyhow::Error) -> &'static str {
    if error.is::<airk_store::service::collection::integrity::native_owner::NativeOwnerRecoveryRequired>() {
        "native_owner_recovery_required"
    } else {
        "integrity_maintenance_observation_unavailable"
    }
}

fn verify_turn_bytes(blocked_large: bool, working_memory_bytes: u64) -> u64 {
    // Existing native work grants reserve at least 768 MiB. An indivisible
    // component may use the existing 512-MiB audit ceiling only after refusal
    // by the ordinary 64-MiB turn; smaller supplied grants retain that limit.
    if blocked_large && working_memory_bytes >= 768 << 20 {
        512 << 20
    } else {
        64 << 20
    }
}

fn observe_native_integrity(
    kernel: &Kernel,
    registry: &mut ServiceRegistry,
    lease: Arc<ProjectLease>,
    bound: &airk_store::service::data_scope::BoundDataRoot,
    identity: WorkerIdentity,
    max_verify_bytes: Option<u64>,
) -> Result<bool> {
    match max_verify_bytes {
        Some(max_bytes) => {
            kernel.observe_service_audit_index_with_limit(registry, &lease, max_bytes)?
        }
        None => kernel.observe_service_audit_index(registry, &lease)?,
    }
    kernel.try_arm_native_owner(registry, lease, bound, identity)
}
fn record_watch_roots(kernel: &Kernel, registry: &mut ServiceRegistry, lease: &ProjectLease) {
    let roots = kernel
        .refresh_service_snapshot()
        .and_then(|selected| selected.configured_roots());
    let recorded = roots.and_then(|roots| {
        registry.record_watch_roots(lease, &roots.into_values().collect::<Vec<_>>())
    });
    if recorded.is_err() {
        let _ = registry.invalidate_watch_roots(lease);
    }
}

fn observe_maintenance(
    kernel: &Kernel,
) -> (
    airk_core::service::MaintenanceObservation,
    Option<crate::service_logs::FailureDetail>,
) {
    let result = kernel.maintain_service_collection();
    let detail = result
        .as_ref()
        .err()
        .map(crate::service_logs::FailureDetail::capture);
    (
        airk_core::service::MaintenanceObservation {
            pending_incarnations: result.as_ref().ok().map(|report| report.pending),
            pending_objects: result.as_ref().is_ok_and(|report| report.pending_objects),
            objects_checked: true,
            removed_files: result.as_ref().map_or(0, |report| report.removed_files),
            failed: result.is_err(),
            observed_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64),
        },
        detail,
    )
}
async fn query(mut stream: Stream, state: Arc<State>) -> Result<()> {
    let Some(grant): Option<ConnectionGrant> =
        local_ipc::read_message(&mut stream, Duration::from_secs(5)).await?
    else {
        return Ok(());
    };
    ensure!(
        !state.closing.load(Ordering::Acquire),
        "project ownership is draining"
    );
    let (session, purpose, authority) = admin::redeem(&state, &grant)?;
    let session_tools = match authority {
        Some(authority) => {
            state
                .tools
                .scoped_session_with_client(authority, grant.client)
                .await
        }
        None => state.tools.session_with_client(grant.client).await,
    };
    let mut tools = match session_tools {
        Ok(tools) => tools,
        Err(error) => {
            state
                .broker
                .lock()
                .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
                .disconnect(state.identity, session, Instant::now())?;
            return Err(error);
        }
    };
    let _session_fence = scopes::SessionFence {
        scopes: state.scopes.clone(),
        reference: grant.scope,
        session,
    };
    let response_kernel = tools.scoped_read_revocation_handle();
    let result = async {
        if let Some(scope) = grant.scope {
            state.scopes.lock().map_err(|_| anyhow::anyhow!("scope policy unavailable"))?
                .attach(scope, session, tools.scoped_read_revocation_handle())?;
        }
        local_ipc::write_message(
            &mut stream,
            &protocol::Accepted {
                identity: state.identity,
            },
            Duration::from_secs(5),
        )
        .await?;
        let authorized_until = Instant::now() + Duration::from_secs(60);
        loop {
            let Some(request): Option<protocol::Query> =
                local_ipc::read_message(&mut stream, Duration::from_secs(30)).await?
            else {
                return Ok(());
            };
            ensure!(
                !state.closing.load(Ordering::Acquire) && Instant::now() < authorized_until,
                "project authorization expired"
            );
            ensure!(
                if request.native.is_some() { request.tool.is_empty() && request.arguments.is_empty() } else { !request.tool.is_empty() && request.tool.len() <= 128 },
                "invalid tool name"
            );
            admin::validate_request(purpose, &request)?;
            state.storage.check()?;
            {
                let scopes=state.scopes.lock().map_err(|_| anyhow::anyhow!("scope policy unavailable"))?;
                admin::check_scope(&scopes,grant.scope,purpose)?;
            }
            // This one received request has not been accepted by the broker.
            // Wait briefly for its slot rather than making the agent replay it;
            // authorization and ownership are rechecked on every attempt.
            let received = tokio::time::Instant::now();
            let wire_attempt = request.read_attempt;
            let retry_budget_complete = request.native.is_none()
                || wire_attempt.is_some_and(|budget| budget.native_retry_expires_monotonic_ns.is_some());
            let read_context = wire_attempt.map(crate::read_attempt::decode).transpose()?
                .map(|context| context.narrowed(authorized_until));
            // Accepted blocking work keeps the existing hard reap boundary; a
            // shorter caller deadline must not kill this shared worker early.
            let query_deadline = received + Duration::from_secs(125);
            let admission_deadline = (received + Duration::from_secs(2))
                .min(tokio::time::Instant::from_std(authorized_until))
                .min(read_context.as_ref().map_or(query_deadline, |context| tokio::time::Instant::from_std(context.deadline())));
            let Some(admission) = state.admission.while_open(&mut stream, admission_deadline, || {
                use airk_core::service::AdmissionError;
                if state.closing.load(Ordering::Acquire) {
                    return Err(AdmissionError::Draining);
                }
                if Instant::now() >= authorized_until {
                    return Err(AdmissionError::Expired);
                }
                if state.storage.check().is_err() {
                    return Err(AdmissionError::OwnershipLost);
                }
                let scope_allowed = {
                    let scopes=state.scopes.lock().map_err(|_| AdmissionError::OwnershipLost)?;
                    admin::check_scope(&scopes,grant.scope,purpose)
                };
                if scope_allowed.is_err() {
                    return Err(AdmissionError::InvalidGrant);
                }
                let mut policy = state.broker.lock().map_err(|_| AdmissionError::OwnershipLost)?;
                let now = Instant::now();
                if tokio::time::Instant::from_std(now) >= admission_deadline {
                    return Err(AdmissionError::Busy);
                }
                policy.begin_query_with_control(state.identity, session, MAX_FRAME_BYTES, now, matches!(request.tool.as_str(), "airk_status" | "status"))
            }).await? else { return Ok(()); };
            let admission = if admission == Err(airk_core::service::AdmissionError::Busy)
                && Instant::now() >= authorized_until {
                Err(airk_core::service::AdmissionError::Expired)
            } else { admission };
            let request_id = match admission {
                Ok(request) => request,
                Err(error) => {
                    let mut reply = admission_rejection_observed(error, &response_kernel)?;
                    reply.status_receipt = tools.status_receipt();
                    deliver(&mut stream, &reply, &state, grant.scope, purpose).await?;
                    if reply.rebind { return Ok(()); }
                    continue;
                }
            };
            tools.restore_status_receipt(request.status_receipt);
            // The permit and revision materialization survive transport cancellation.
            let native_read = request.native.is_some();
            let response = match tokio::time::timeout_at(query_deadline, async {
                match request.native {
                    Some(native) => match &read_context {
                        Some(context) => tools.read_native_with_context(native, state.identity.project, context.clone()).await,
                        None => tools.read_native(native, state.identity.project).await,
                    },
                    None => {
                        if request.tool == "airk_diagnostics" && grant.scope.is_none() {
                            tools.set_runtime(crate::runtime_metadata::bundle(&state.control, state.identity, request.frontend).await);
                            tools.set_work(crate::runtime_metadata::work(&state.control, state.identity).await);
                        }
                        let result = match &read_context {
                            Some(context) => tools.call_with_read_context(request.tool, request.arguments, context.clone()).await?,
                            None => tools.call(request.tool, request.arguments).await?,
                        };
                        Ok(serde_json::to_value(result)?)
                    },
                }
            }).await {
                Ok(response) => response,
                Err(_) => {
                    // Cancelling a protocol future does not stop accepted native
                    // work or release its object lease. Exit this lock-bearing
                    // worker; only the supervisor's confirmed reap frees capacity.
                    std::process::exit(75);
                }
            };
            let value = response.unwrap_or_else(|error| {
                if response_kernel.scope_observation().is_some()
                    || error.is::<airk_kernel::SourcePolicyReadUnavailable>() {
                    return scoped_worker_error(&response_kernel, &error, native_read);
                }
                if native_read { serde_json::json!({"error":{"code":"native_read_failed","message":format!("{error:#}").chars().take(1024).collect::<String>()}}) }
                else { unscoped_worker_error(&error) }
            });
            let read_admission = wire_attempt.zip(read_context.as_ref()).filter(|(_, context)| retry_budget_complete && context.selection_admission_refused())
                .map(|(budget, _)| protocol::SelectionReadAdmission {
                    request_id: budget.request_id, identity: state.identity, scope: grant.scope,
                });
            let reply = protocol::QueryResult { read_admission, result: value, status_receipt: tools.status_receipt(), rebind: false };
            let result = deliver(&mut stream, &reply, &state, grant.scope, purpose).await;
            state
                .broker
                .lock()
                .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
                .finish_query(state.identity, request_id)?;
            state.admission.completed();
            result?;
        }
    }
    .await;
    state
        .broker
        .lock()
        .map_err(|_| anyhow::anyhow!("query policy unavailable"))?
        .disconnect(state.identity, session, Instant::now())?;
    tools.close().await?;
    result
}

/// Serialize first, then fence immediately before delivery. A result completed
/// before revocation cannot be delivered from a retained serialized response.
async fn deliver(
    stream: &mut Stream,
    reply: &protocol::QueryResult,
    state: &State,
    scope: Option<airk_core::service::ScopedReadRef>,
    purpose: SessionPurpose,
) -> Result<()> {
    let mut frame = Vec::new();
    service_ipc::write_frame_limited(&mut frame, reply, MAX_FRAME_BYTES)?;
    deliver_frame(stream, &frame, || {
        let guard = state
            .scopes
            .lock()
            .map_err(|_| anyhow::anyhow!("scope policy unavailable"))?;
        ensure!(
            !state.closing.load(Ordering::Acquire),
            "project ownership is draining"
        );
        admin::check_scope(&guard, scope, purpose)?;
        Ok(guard)
    })
    .await
}

async fn deliver_frame<G>(
    stream: &mut (impl tokio::io::AsyncWrite + Unpin),
    frame: &[u8],
    fence: impl Fn() -> Result<G>,
) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut offset = 0;
        while offset < frame.len() {
            // A backpressured stream may yield before writing any bytes. Fence
            // every resumed write poll, not only creation of the write future.
            let count = std::future::poll_fn(|cx| {
                let _guard = match fence() {
                    Ok(guard) => guard,
                    Err(error) => return std::task::Poll::Ready(Err(std::io::Error::other(error))),
                };
                tokio::io::AsyncWrite::poll_write(
                    std::pin::Pin::new(&mut *stream),
                    cx,
                    &frame[offset..],
                )
            })
            .await?;
            ensure!(count > 0, "native frame writer closed");
            offset += count;
        }
        std::future::poll_fn(|cx| {
            let _guard = match fence() {
                Ok(guard) => guard,
                Err(error) => return std::task::Poll::Ready(Err(std::io::Error::other(error))),
            };
            tokio::io::AsyncWrite::poll_flush(std::pin::Pin::new(&mut *stream), cx)
        })
        .await?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("native frame write deadline exceeded")??;
    Ok(())
}

fn admission_rejection(error: airk_core::service::AdmissionError) -> protocol::QueryResult {
    let busy = error == airk_core::service::AdmissionError::Busy;
    let detail = serde_json::json!({"error":{"code":"direct_query_admission", "reason":error,
        "query_submitted":false,"retryable":busy,"retry_after_ms":if busy {Some(25)} else {None},
        "fresh_brokerage_required":!busy}});
    protocol::QueryResult {
        read_admission: None,
        result: serde_json::json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true}),
        status_receipt: Default::default(),
        rebind: !busy,
    }
}

/// This worker boundary previously exposed only a fixed public failure. Keep
/// unknown internal paths/content private while retaining recognized receipts.
fn unscoped_worker_error(error: &anyhow::Error) -> serde_json::Value {
    if let Some(reply) = airk_mcp::response::typed_error_result(error) {
        return serde_json::to_value(reply).expect("typed worker failure receipt");
    }
    let body = serde_json::json!({"error":{"code":"read_failed",
        "message":"AIRK read-only tool failed or is unavailable.","retryable":false}});
    serde_json::json!({"content":[{"type":"text","text":body.to_string()}],"structuredContent":body,"isError":true})
}

/// Preserve the direct transport's native/MCP envelope distinction while using
/// the same bounded, typed refusal factory as canonical MCP reads.
fn scoped_worker_error(kernel: &Kernel, error: &anyhow::Error, native: bool) -> serde_json::Value {
    let reply = airk_mcp::response::scoped_error_result(kernel, error);
    if native {
        // The canonical factory always supplies JSON for a scoped response,
        // including generic failures and frame-budget refusals.
        reply
            .content
            .iter()
            .find_map(|block| {
                block
                    .as_text()
                    .and_then(|text| serde_json::from_str(&text.text).ok())
            })
            .expect("canonical scoped error JSON")
    } else {
        serde_json::to_value(reply).expect("canonical scoped error serialization")
    }
}

fn admission_rejection_observed(
    error: airk_core::service::AdmissionError,
    kernel: &Kernel,
) -> Result<protocol::QueryResult> {
    let mut reply = admission_rejection(error);
    if kernel.scope_observation().is_some() {
        let text = reply.result["content"][0]["text"]
            .as_str()
            .context("direct admission receipt missing")?;
        let mut detail: serde_json::Value = serde_json::from_str(text)?;
        match kernel.annotate_scoped_read(&mut detail) {
            Ok(()) => reply.result["content"][0]["text"] = detail.to_string().into(),
            Err(error) => reply.result = scoped_worker_error(kernel, &error, false),
        }
    }
    Ok(reply)
}

fn execute(
    kernel: &Kernel,
    kind: JobKind,
    job: airk_core::identity::Sha256Digest,
) -> Result<JobOutcome> {
    use airk_core::service_progress::WorkPhase;
    airk_kernel::service_execution::progress_phase(match kind {
        JobKind::Index | JobKind::Reconcile | JobKind::Rebuild => WorkPhase::Discovering,
        JobKind::Vectors | JobKind::ModelSetup => WorkPhase::Embedding,
        JobKind::Configure => WorkPhase::Starting,
        JobKind::Inspect | JobKind::PlanInspect | JobKind::Repair | JobKind::Verify => {
            WorkPhase::Verifying
        }
        JobKind::Recover => WorkPhase::Recovering,
        JobKind::Maintain => WorkPhase::Retiring,
    });
    if kind == JobKind::Recover {
        return Ok(JobOutcome::Succeeded {
            revision: kernel.recover_service_publication()?,
        });
    }
    if kind == JobKind::Maintain {
        return Ok(JobOutcome::Succeeded { revision: None });
    }
    if kind == JobKind::Vectors {
        let selected = retry_semantic_turn(
            || kernel.refresh_service_snapshot(),
            Duration::from_secs(30),
        )?;
        return semantic_publication(
            || selected.service_enrichment_turn().map(|_| ()),
            || {
                let publication = selected.status()?.publication;
                Ok((
                    publication.clone(),
                    JobOutcome::Succeeded {
                        revision: publication.map(|value| value.token),
                    },
                ))
            },
            Duration::from_secs(30),
        );
    }
    let selected =
        kernel
            .refresh_service_snapshot()
            .context(airk_kernel::diagnostics::IndexFailureStage(
                "select_snapshot",
            ))?;
    let kernel = &selected;
    match kind {
        JobKind::Index | JobKind::Reconcile => {
            kernel.index_service_sources()?;
        }
        JobKind::Vectors => unreachable!("vector turns use publication-aware retry"),
        JobKind::ModelSetup => {
            // Upgrade existing semantic state from retained evidence without a
            // source rescan. Disabled projects retire vectors and stay text-only.
            let retained = kernel.upgrade_service_semantics()?;
            return Ok(JobOutcome::Succeeded {
                revision: if retained {
                    kernel
                        .status()?
                        .publication
                        .map(|publication| publication.token)
                } else {
                    None
                },
            });
        }
        JobKind::Verify => unreachable!("service Verify owns its durable continuation"),
        JobKind::Recover
        | JobKind::Configure
        | JobKind::Maintain
        | JobKind::Inspect
        | JobKind::PlanInspect
        | JobKind::Repair => {
            unreachable!("handled outside ordinary execution")
        }
        JobKind::Rebuild => {
            return Ok(JobOutcome::Succeeded {
                revision: Some(kernel.rebuild_service(job)?),
            });
        }
    }
    Ok(JobOutcome::Succeeded {
        revision: kernel
            .status()?
            .publication
            .map(|publication| publication.token),
    })
}

fn reconcile_semantic_setup(
    kernel: &Kernel,
    registry: &mut airk_store::service::registry::ServiceRegistry,
    lease: &airk_store::service::ProjectLease,
    engine: airk_core::identity::Sha256Digest,
) -> Result<()> {
    let project = lease.identity().0;
    let snapshot = kernel.inspect_service(project)?;
    if snapshot.vectors_enabled == Some(true)
        && snapshot
            .semantic
            .as_ref()
            .is_some_and(|s| s.state == airk_core::readiness::SemanticState::Disabled)
        && let Some(settings) = snapshot.settings_digest
    {
        registry.reconcile_semantic_setup(lease, &format!("{engine}:{settings}"))?;
    }
    Ok(())
}

pub async fn run() -> Result<()> {
    let init: protocol::Init = service_ipc::read_frame(&mut std::io::stdin().lock())?
        .context("missing private project initialization")?;
    local_ipc::prepare_root(&init.runtime)?;
    let mut lease = ProjectLease::acquire(&init.runtime, init.project)?;
    let mut registry = ServiceRegistry::open(&init.runtime, init.service)?;
    // Native persisted authority, pinned before serving. No wire omission or
    // frontend assertion may select the legacy unrestricted branch. The native
    // coordinator must fence/restart all legacy workers BEFORE authority promotion;
    // this startup pin alone cannot fence a later live catalog upgrade.
    let scopes_required = registry
        .catalog_state()?
        .is_some_and(|state| state.authority_contract >= 1);
    let registration = registry.get(init.project)?.registration;
    airk_kernel::service_projects::validate_retained(&registration)?;
    let epoch = registry.admit_worker(&mut lease)?;
    let lease = Arc::new(lease);
    ensure!(
        PROCESS_LEASE.set(lease.clone()).is_ok(),
        "project role already owns a process lease"
    );
    let identity = WorkerIdentity {
        service: init.service,
        worker: init.worker,
        project: init.project,
        fencing_generation: epoch,
    };
    let role = protocol::role(init.worker);
    let mut listener = Listener::bind(&local_ipc::endpoint(&init.runtime, &role)?)?;
    service_ipc::write_frame(
        &mut std::io::stdout().lock(),
        &Reply::Ready { identity, role },
    )?;
    // A late child may receive a buffered Init after its parent died. It cannot touch
    // collection state until the service accounts its lock-bearing generation.
    let Some(Command::Activate {
        identity: accepted,
        allowance,
        pool,
        pool_pending,
    }) = service_ipc::read_frame(&mut std::io::stdin().lock())?
    else {
        anyhow::bail!("project owner was not activated")
    };
    ensure!(
        accepted == identity
            && (1..=65).contains(&allowance.concurrent_queries)
            && allowance.response_bytes == allowance.concurrent_queries * MAX_FRAME_BYTES,
        "project activation identity or allowance mismatch"
    );
    crate::embedding_pool_client::install(
        &init.runtime,
        identity,
        pool.map(|access| *access),
        pool_pending,
    )?;
    let storage = airk_store::service::data_scope::bind(
        Path::new(&registration.data_root),
        &init.runtime,
        registry.incarnation()?,
    )?;
    #[cfg(unix)]
    crate::unix_guardian::install(
        &init.runtime,
        registry.incarnation()?,
        identity,
        &lease,
        &std::env::current_exe()?,
    )?;
    // A staged participant starts as an unprivileged metadata observer. Only
    // its actually claimed native job can attach a private split permit later.
    let refresh_pending = registry.pending_reference_refresh(init.project)?;
    let split_observer = registry
        .catalog_active_split_for_binding(init.project)?
        .map(|_| Kernel::open_existing_service_observer(&registration))
        .transpose()?;
    // Evidence activation does not release the participant's prepaid writer
    // allocation. Generic startup work must wait for the same native barrier
    // used by job admission, including a terminal receipt awaiting settlement.
    let split_write_fenced = registry.catalog_split_write_fenced(init.project)?
        || split_observer
            .as_ref()
            .is_some_and(|observer| observer.require_unrestricted_read().is_err());
    let kernel = if split_write_fenced {
        split_observer.context("staged participant observer missing")?
    } else if refresh_pending {
        Kernel::open_existing_service_observer(&registration)?
    } else {
        Kernel::open_service_collection_for_diagnostics(
            Path::new(&registration.root),
            Some((&registration.data_root).into()),
            &registration.collection,
        )?
    }
    .with_expected_source_policy(if scopes_required {
        registry
            .catalog_index_for_binding(init.project)?
            .map(|index| registry.catalog_source_policy(index.id))
            .transpose()?
            .flatten()
    } else {
        None
    })?
    .with_service_freshness(identity)?;
    // Bind immutable bundled model descriptors only. This creates no model,
    // reserves no quota and grants no staged evidence access. Keeping this
    // metadata on the ordinary Kernel also supports reads after activation.
    let rebuild_control = init.runtime.clone();
    let rebuild_project = init.project;
    let kernel = crate::bundled_assets::bind_service_models(kernel, &init.runtime)?
        .with_rebuild_observer(rebuild_project, move || {
            crate::runtime_install::rebuild_observation_for_control(
                &rebuild_control,
                rebuild_project,
            )
        });
    airk_kernel::service_execution::install(Path::new(&registration.data_root), identity)?;
    let receipt = if split_write_fenced {
        None
    } else {
        kernel.service_rebuild_receipt()?
    };
    registry.recover_jobs_with_rebuild_receipt(&lease, receipt.as_ref())?;
    if !split_write_fenced && !refresh_pending {
        source_configuration::recover(&kernel, &mut registry, &lease, init.engine)?;
    }
    // A crash after the accepted Configure result settles but before catalog
    // bookkeeping must not require replaying a policy change or clearing its deny.
    if !split_write_fenced
        && scopes_required
        && let Some(index) = registry.catalog_index_for_binding(init.project)?
        && let Some(snapshot) = registry.catalog_policy_status(index.id)?
        && snapshot
            .application
            .as_ref()
            .is_none_or(|report| report.physical_cleanup_pending)
        && registry.job(snapshot.job)?.state == airk_core::service::JobState::Succeeded
    {
        registry.catalog_record_policy_job(index.id, snapshot.operation_id)?;
    }
    // Only an actually opened retained index can become an automatic audit
    // candidate. A fresh/unprepared registration remains ordinary setup work.
    let initial_integrity_failure = if split_write_fenced {
        None
    } else {
        observe_native_integrity(
            &kernel,
            &mut registry,
            lease.clone(),
            &storage,
            identity,
            None,
        )
        .err()
        .map(|error| integrity_observation_code(&error).to_owned())
    };
    if !split_write_fenced {
        record_watch_roots(&kernel, &mut registry, &lease);
        let _ = reconcile_semantic_setup(&kernel, &mut registry, &lease, init.engine);
    }
    // Recovery is durable desired work, admitted by the service rather than an
    // unaccounted startup replay. Failed recovery leaves the owner reachable.
    if !split_write_fenced
        && !refresh_pending
        && kernel.service_recovery_required().unwrap_or(false)
        && !registry.source_failure_blocked(init.project)?
    {
        registry.enqueue_internal(
            init.project,
            init.service,
            JobKind::Recover,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
    }
    let pending_retirements = if split_write_fenced {
        0
    } else {
        kernel.pending_service_retirements()?
    };
    let previous_maintenance = registry.maintenance_observation(init.project)?;
    if !split_write_fenced
        && (pending_retirements > 0
            || previous_maintenance
                .as_ref()
                .is_none_or(|value| !value.objects_checked))
    {
        registry.record_maintenance(
            &lease,
            &airk_core::service::MaintenanceObservation {
                pending_incarnations: Some(pending_retirements),
                pending_objects: true,
                objects_checked: false,
                removed_files: 0,
                failed: previous_maintenance
                    .as_ref()
                    .is_some_and(|value| value.failed),
                observed_unix_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64),
            },
        )?;
    }
    let mut broker = Broker::new(
        init.service,
        BrokerLimits {
            reserved_status: allowance.concurrent_queries > 1,
            ..BrokerLimits::service()
        },
    )?;
    broker.register(
        identity,
        allowance,
        Instant::now(),
        Duration::from_secs(300),
    )?;
    let closing = Arc::new(AtomicBool::new(false));
    let _stop = StopOnDrop(closing.clone());
    let activity = Arc::new(Notify::new());
    let busy = Arc::new(AtomicBool::new(false));
    let maintenance_waiting = Arc::new(AtomicBool::new(false));
    let job_storage = storage.clone();
    let state = Arc::new(State {
        control: init.runtime.clone(),
        identity,
        storage,
        broker: Mutex::new(broker),
        tools: airk_mcp::local::LocalTools::with_client(
            kernel.clone(),
            airk_core::usage::ClientFamily::Unknown,
        )
        .await?,
        closing: closing.clone(),
        admission: Default::default(),
        scopes: Arc::new(Mutex::new(scopes::Scopes::new(identity, scopes_required))),
    });
    service_ipc::write_frame(&mut std::io::stdout().lock(), &Reply::Activated)?;
    let (sender, mut commands) = mpsc::channel(8);
    let control_closing = closing.clone();
    let control_activity = activity.clone();
    let control_admission = state.admission.clone();
    let control_scopes = state.scopes.clone();
    std::thread::Builder::new()
        .name("airk-project-control".into())
        .spawn(move || {
            let mut input = std::io::stdin().lock();
            while let Ok(Some(command)) = service_ipc::read_frame::<_, Command>(&mut input) {
                if sender.blocking_send(command).is_err() {
                    break;
                }
            }
            crate::embedding_pool_client::close();
            control_closing.store(true, Ordering::Release);
            if let Ok(mut scopes) = control_scopes.lock() {
                scopes.close();
            }
            control_admission.close();
            control_activity.notify_one();
        })?;
    let job_closing = closing.clone();
    let job_activity = activity.clone();
    let job_busy = busy.clone();
    let job_lease = lease.clone();
    let (work_sender, work_receiver) =
        std::sync::mpsc::sync_channel::<airk_core::service_resources::WorkAllowance>(1);
    let work_status = Arc::new(Mutex::new(WorkState {
        integrity_observation_failure: initial_integrity_failure.clone(),
        ..Default::default()
    }));
    let job_status = work_status.clone();
    let freshness_kernel = kernel.clone();
    let jobs = tokio::task::spawn_blocking(move || -> Result<ServiceRegistry> {
        let mut last_integrity_failure = initial_integrity_failure;
        while !job_closing.load(Ordering::Acquire) {
            let allowance = match work_receiver.recv_timeout(Duration::from_millis(200)) {
                Ok(allowance) => allowance,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            // Unregister can commit between supervisor observation and dispatch.
            // Never claim inactive cleanup with a query/index owner.
            registry.get(identity.project)?;
            ensure!(
                !registry
                    .configuration_work(allowance.job)?
                    .is_some_and(|work| work.request.configuration.is_derived_cleanup()),
                "cleanup requires a distinct inactive owner"
            );
            if let Some(job) = registry.claim_job_id(&job_lease, allowance.job)? {
                let split_job = registry
                    .catalog_active_split_for_binding(identity.project)?
                    .is_some_and(|snapshot| {
                        snapshot
                            .receipt
                            .jobs
                            .iter()
                            .any(|reference| reference.job == job.id)
                    });
                let refresh_job=registry.configuration_work(job.id)?.is_some_and(|work|matches!(work.request.configuration,airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}));
                let mut automatic_settled = false;
                let mut verify_admission = None;
                let mut completed_maintenance = None;
                let mut failure_detail = None;
                let mut audit_drain = None;
                let mut audit_completion = None;
                let mut query_completion = None;
                let mut source_completion = None;
                use airk_store::service::registry::RecoveryCondition;
                let mut recovery_condition = RecoveryCondition::Delay;
                let mut outcome = if registry.cancellation_requested(job.id)? {
                    if matches!(job.kind, JobKind::Verify | JobKind::Repair) {
                        audit_drain = registry
                            .drained_verify_before_execution(&job_lease, &job_storage, job.id)
                            .ok();
                    }
                    Ok(JobOutcome::Canceled)
                } else {
                    airk_kernel::service_execution::run(allowance.clone(), || {
                        job_storage.check()?;
                        airk_kernel::service_execution::checkpoint()?;
                        if job.kind == JobKind::Configure {
                            let mut work = registry
                                .configuration_work(job.id)?
                                .context("configuration work missing")?;
                            ensure!(
                                work.engine == init.engine,
                                airk_kernel::service_configuration::ConfigurationChanged
                            );
                            if split_job {
                                return crate::project_worker_split::execute(
                                    &kernel, &mut registry, &job_lease,
                                    job.id, init.engine, &init.runtime,
                                );
                            }
                            if let Some(source_work) = registry.catalog_source_work(&job_lease, job.id)? {
                                let airk_core::catalog_management::CatalogOperation::BeginIndexSourceChange { plan, .. } = &source_work.begin.operation else {
                                    anyhow::bail!("source work lacks its accepted plan");
                                };
                                registry.record_watch_roots(&job_lease, &plan.after.aliases.values().cloned().collect::<Vec<_>>())?;
                                let prepared = source_configuration::prepare(
                                    &kernel, &source_work, &work, init.engine)?;
                                let result = registry.qualify_catalog_source_result(
                                    &job_lease, job.id, prepared.native(),
                                    &serde_json::json!({"updated":true,"source_pass_requested":true}))?;
                                registry.record_configuration_result(&job_lease, job.id, &result)?;
                                source_completion = Some(prepared);
                                return Ok(JobOutcome::Succeeded { revision: None });
                            }
                            if matches!(work.request.configuration,airk_core::service_configuration::ProjectConfiguration::RefreshReferences {..}) {
                                return crate::reference_refresh_control::execute(&kernel,&mut registry,&job_lease,job.id);
                            }
                            if work.request.prepare {
                                work.request.validate()?;
                                let observed = kernel.inspect_service(identity.project)?;
                                work.request.incarnation = observed.incarnation;
                                work.request.settings = observed
                                    .settings_digest
                                    .context("settings inspection unavailable")?;
                                work.request.prepare = false;
                            }
                            kernel
                                .validate_service_configuration(identity.project, &work.request)?;
                            if let Some(roots) =
                                kernel.service_configuration_roots(&work.request)?
                            {
                                registry.record_watch_roots(&job_lease, &roots)?;
                            }
                            let result = kernel
                                .configure_service_project(identity.project, &work.request)?;
                            registry.record_configuration_result(&job_lease, job.id, &result)?;
                            if let Ok(snapshot) = kernel.inspect_service(identity.project) {
                                let observed = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)?
                                    .as_millis()
                                    .try_into()?;
                                let _ = registry.record_inspection(
                                    &job_lease, identity, job.id, observed, &snapshot,
                                );
                            }
                            return Ok(JobOutcome::Succeeded { revision: None });
                        }
                        let kind = if job.kind == JobKind::Repair {
                            use airk_core::service_maintenance::{PlanError, RepairOperation};
                            let approved = registry.repair_work(&job_lease, job.id, init.engine)?;
                            let observed = kernel.inspect_service(identity.project)?;
                            ensure!(
                                observed.guard_digest()? == approved.expected.guard_digest()?,
                                PlanError::Stale
                            );
                            if matches!(
                                approved.operation,
                                RepairOperation::Reconcile | RepairOperation::Rebuild
                            ) {
                                ensure!(
                                    observed.source_state
                                        == airk_core::service_inspection::SourceState::Available,
                                    PlanError::SourceUnavailable
                                );
                                kernel
                                    .refresh_service_snapshot()?
                                    .require_service_sources()
                                    .map_err(|_| PlanError::SourceUnavailable)?;
                            }
                            if approved.operation == RepairOperation::Vectors {
                                // The approved regeneration supersedes an older
                                // continuation. A crash/failure before its success
                                // must not silently resume that old chain.
                                registry.set_background_pending(
                                    &job_lease,
                                    JobKind::Vectors,
                                    false,
                                )?;
                                kernel
                                    .refresh_service_snapshot()?
                                    .begin_service_vector_regeneration(job.id)?;
                            }
                            approved
                                .operation
                                .job_kind()
                                .context("unresolved approved repair operation")?
                        } else {
                            job.kind
                        };
                        if matches!(kind, JobKind::Index | JobKind::Reconcile | JobKind::Rebuild) {
                            let selected = kernel.refresh_service_snapshot()?;
                            selected.require_service_sources()?;
                            let roots = selected
                                .configured_roots()?
                                .into_values()
                                .collect::<Vec<_>>();
                            registry.record_watch_roots(&job_lease, &roots)?;
                        }
                        if matches!(kind, JobKind::Inspect | JobKind::PlanInspect) {
                            airk_kernel::service_execution::progress_phase(
                                airk_core::service_progress::WorkPhase::Verifying,
                            );
                            let snapshot = kernel.inspect_service(identity.project)?;
                            let observed_unix_ms = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)?
                                .as_millis()
                                .try_into()?;
                            registry.record_inspection(
                                &job_lease,
                                identity,
                                job.id,
                                observed_unix_ms,
                                &snapshot,
                            )?;
                            return Ok(JobOutcome::Succeeded {
                                revision: snapshot.publication,
                            });
                        }
                        if kind == JobKind::Verify {
                            verify_admission =
                                Some(verify_turn_bytes(true, allowance.working_memory_bytes));
                            airk_kernel::service_execution::progress_phase(
                                airk_core::service_progress::WorkPhase::Verifying,
                            );
                            let recovery_epoch = if last_integrity_failure.as_deref()
                                == Some("native_owner_recovery_required")
                                || kernel.native_owner_requires_recovery()?
                            {
                                Some(kernel.begin_service_native_recovery(
                                    &registry,
                                    &job_lease,
                                    &job_storage,
                                    identity,
                                    job.id,
                                )?)
                            } else {
                                None
                            };
                            loop {
                                airk_kernel::service_execution::checkpoint()?;
                                let run_turn = |max_bytes| match recovery_epoch.as_ref() {
                                    Some(epoch) => kernel.verify_native_recovery_service_turn(
                                        &registry,
                                        &job_lease,
                                        &job_storage,
                                        epoch,
                                        max_bytes,
                                    ),
                                    None => kernel.verify_service_turn_with_limit(
                                        &registry,
                                        &job_lease,
                                        &job_storage,
                                        job.id,
                                        max_bytes,
                                    ),
                                };
                                let mut turn = run_turn(64 << 20)?;
                                let expanded = verify_turn_bytes(
                                    turn.blocked_large,
                                    allowance.working_memory_bytes,
                                );
                                if expanded > 64 << 20 {
                                    // A later sibling component may exceed the first
                                    // blocked object's requirement. Admit the fixed
                                    // exceptional profile once, so its no-progress
                                    // witness describes the actual available ceiling.
                                    turn = run_turn(expanded)?;
                                }
                                if registry.cancellation_requested(job.id)? {
                                    audit_drain = turn.drained;
                                    return Ok(JobOutcome::Canceled);
                                }
                                if let Some(proof) = turn.recovery_completed.as_ref() {
                                    kernel.recover_service_native_owner(
                                        &mut registry,
                                        &job_lease,
                                        &job_storage,
                                        identity,
                                        job.id,
                                        proof,
                                    )?;
                                }
                                if let Some(proof) = turn.retained_completed.as_ref() {
                                    registry.record_retained_audit(&job_lease, proof)?;
                                }
                                if registry
                                    .automatic_audit_context(&job_lease, job.id)?
                                    .is_some_and(|context| !context.manual())
                                    && !(turn.progress.requires_repair() && turn.work_complete())
                                {
                                    let now_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)?
                                        .as_millis()
                                        .try_into()?;
                                    let handoff = if let Some(proof) = turn.query_completed.as_ref()
                                    {
                                        registry.complete_automatic_audit(&job_lease, proof, now_ms)
                                    } else {
                                        let drained = turn
                                            .drained
                                            .as_ref()
                                            .context("automatic audit turn lacks drain proof")?;
                                        ensure!(
                                            !turn.blocked_large || turn.blocked_input.is_some(),
                                            "blocked automatic audit lacks retry witness"
                                        );
                                        registry.park_automatic_audit(
                                            &job_lease,
                                            job.id,
                                            drained,
                                            turn.blocked_input.as_ref(),
                                            now_ms,
                                        )
                                    };
                                    if let Err(error) = handoff {
                                        return Err(match turn.drained {
                                            Some(drained) => error.context(drained),
                                            None => error,
                                        });
                                    }
                                    automatic_settled = true;
                                    // The accepted job has already completed or parked
                                    // transactionally. This value never settles its guard.
                                    return Ok(JobOutcome::Canceled);
                                }
                                if turn.blocked_large
                                    || (turn.progress.requires_repair() && turn.work_complete())
                                    || (turn.work_complete()
                                        && !turn.projection_coverage_complete())
                                {
                                    query_completion = turn.query_completed;
                                    audit_drain = turn.drained;
                                    return Ok(JobOutcome::Failed {
                                        code: "integrity_coverage_incomplete".into(),
                                    });
                                }
                                if turn.work_complete() && turn.progress.complete() {
                                    query_completion = turn.query_completed;
                                    let complete = turn
                                        .completed
                                        .context("complete audit lacks physical proof")?;
                                    let revision = complete.revision()?;
                                    audit_completion = Some(complete);
                                    return Ok(JobOutcome::Succeeded {
                                        revision: Some(revision),
                                    });
                                }
                                // Existing job admission/cancellation owns this work;
                                // no parallel scheduler or query-time audit is added.
                                std::thread::yield_now();
                            }
                        }
                        let outcome = execute(&kernel, kind, job.id)?;
                        // Successful changes refresh console metadata while this exact job
                        // still owns its fenced lease. Refreshing the UI must not require
                        // a second Inspect job to see new counts or semantic readiness.
                        if matches!(
                            kind,
                            JobKind::Index
                                | JobKind::Reconcile
                                | JobKind::Vectors
                                | JobKind::ModelSetup
                                | JobKind::Rebuild
                                | JobKind::Repair
                                | JobKind::Recover
                        ) && matches!(outcome, JobOutcome::Succeeded { .. })
                            && let Ok(snapshot) = kernel.inspect_service(identity.project)
                        {
                            let observed = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)?
                                .as_millis()
                                .try_into()?;
                            let _ = registry.record_inspection(
                                &job_lease, identity, job.id, observed, &snapshot,
                            );
                        }
                        // Inspection and plan preview must not turn a status
                        // request into a complete current-object census. A
                        // dedicated admitted maintenance job continues cleanup.
                        if !matches!(
                            kind,
                            JobKind::Maintain
                                | JobKind::Index
                                | JobKind::Reconcile
                                | JobKind::Rebuild
                                | JobKind::Repair
                                | JobKind::Recover
                                | JobKind::Vectors
                                | JobKind::ModelSetup
                        ) {
                            return Ok(outcome);
                        }
                        airk_kernel::service_execution::progress_phase(
                            airk_core::service_progress::WorkPhase::Retiring,
                        );
                        let (maintenance, maintenance_detail) = observe_maintenance(&kernel);
                        let _ = registry.record_maintenance(&job_lease, &maintenance);
                        let failed = kind == JobKind::Maintain && maintenance.failed;
                        if failed {
                            failure_detail = maintenance_detail;
                        }
                        completed_maintenance = Some(maintenance.clone());
                        job_status
                            .lock()
                            .map_err(|_| anyhow::anyhow!("work status unavailable"))?
                            .maintenance = Some(maintenance);
                        if failed {
                            Ok(JobOutcome::Failed {
                                code: "owned_retirement_failed".into(),
                            })
                        } else {
                            Ok(outcome)
                        }
                    })
                }
                .unwrap_or_else(|error| {
                    audit_drain =
                        airk_store::service::registry::AuditDrainProof::from_error(&error);
                    if audit_drain.is_some()
                        && registry.cancellation_requested(job.id).unwrap_or(false)
                    {
                        return JobOutcome::Canceled;
                    }
                    if audit_drain.is_none()
                        && error.is::<airk_kernel::service_execution::Canceled>()
                        && matches!(job.kind, JobKind::Verify | JobKind::Repair)
                    {
                        // This synchronous accepted job stack has fully unwound;
                        // no read/turn task is detached. Revoked/failed persistence
                        // cannot mint this exact-job safe-drain proof.
                        audit_drain = registry
                            .drained_verify_before_execution(&job_lease, &job_storage, job.id)
                            .ok();
                    }
                    failure_detail = Some(crate::service_logs::FailureDetail::capture(&error));
                    for cause in error.chain() {
                        if let Some(pressure) =
                            cause.downcast_ref::<airk_store::quota::QuotaPressure>()
                        {
                            recovery_condition = RecoveryCondition::Quota {
                                required: pressure.required_bytes,
                                scope: pressure.scope,
                            };
                            break;
                        }
                        if let Some(pressure) =
                            cause.downcast_ref::<airk_store::capacity::StoragePressure>()
                        {
                            recovery_condition = RecoveryCondition::Capacity {
                                required: pressure.required_bytes,
                            };
                            break;
                        }
                    }
                    if error.is::<airk_kernel::service_projects::SourceUnavailable>() {
                        recovery_condition = RecoveryCondition::Source;
                    } else if error.is::<crate::embedding_pool_client::HelperUnavailable>() {
                        recovery_condition = RecoveryCondition::Helper;
                    } else if matches!(
                        airk_store::capacity::storage_pressure_code(&error),
                        Some("filesystem_storage_full" | "sqlite_storage_full")
                    ) {
                        recovery_condition = RecoveryCondition::Capacity {
                            required: 16 * 1024 * 1024,
                        };
                    }
                    if error
                        .downcast_ref::<airk_kernel::service_execution::Canceled>()
                        .is_some()
                        || error.downcast_ref::<airk_core::service_maintenance::PlanError>()
                            == Some(&airk_core::service_maintenance::PlanError::Canceled)
                    {
                        JobOutcome::Canceled
                    } else {
                        JobOutcome::Failed {
                            code: if let Some(code) =
                                error.downcast_ref::<airk_core::service_maintenance::PlanError>()
                            {
                                return JobOutcome::Failed {
                                    code: code.to_string(),
                                };
                            } else if let Some(code) =
                                airk_store::capacity::storage_pressure_code(&error)
                            {
                                code
                            } else if error.is::<crate::embedding_pool_client::HelperUnavailable>()
                            {
                                "helper_unavailable"
                            } else if job.kind == JobKind::Vectors
                                && Kernel::service_enrichment_contention(&error)
                            {
                                "semantic_contention_retry_exhausted"
                            } else if error.is::<airk_store::current::CurrentReadContention>()
                                || error.is::<airk_store::current::CurrentOpenContention>()
                            {
                                "current_database_admission_busy"
                            } else if error.is::<airk_store::quota::QuotaContention>() {
                                "quota_ledger_busy"
                            } else if error.is::<airk_kernel::service_execution::WorkStalled>() {
                                "work_stalled"
                            } else if error.is::<airk_kernel::service_projects::SourceUnavailable>()
                            {
                                "source_unavailable"
                            } else if error.is::<airk_store::service::registry::SourceBoundary>() {
                                "protected_source_boundary"
                            } else if error
                                .is::<airk_kernel::service_inspection::InspectionChanged>()
                            {
                                if job.kind == JobKind::Repair {
                                    "maintenance_plan_stale"
                                } else {
                                    "inspection_changed"
                                }
                            } else if error
                                .is::<airk_kernel::service_configuration::ConfigurationChanged>()
                            {
                                "configuration_settings_or_engine_changed"
                            } else if job.kind == JobKind::Configure {
                                "configuration_failed_reinspect"
                            } else {
                                let cause = airk_kernel::diagnostics::indexing_failure_code(&error);
                                return JobOutcome::Failed {
                                    code: match error
                                        .downcast_ref::<airk_kernel::current::RecoveryFailure>()
                                    {
                                        Some(stage) => format!(
                                            "{}_{}",
                                            stage.0,
                                            cause.as_deref().unwrap_or("failed")
                                        ),
                                        None => cause
                                            .unwrap_or_else(|| "project_operation_failed".into()),
                                    },
                                };
                            }
                            .into(),
                        }
                    }
                });
                if let JobOutcome::Failed { code } = &outcome {
                    let detail = failure_detail.unwrap_or_else(|| {
                        crate::service_logs::FailureDetail::capture(&anyhow::anyhow!(
                            "operation returned failure code: {code}"
                        ))
                    });
                    job_status
                        .lock()
                        .map_err(|_| anyhow::anyhow!("work status unavailable"))?
                        .failure = Some(crate::service_logs::JobFailure {
                        job: job.id,
                        grant: allowance.id,
                        worker: identity.worker,
                        operation: job.kind,
                        code: code.clone(),
                        detail,
                    });
                }
                // Persist the next phase before acknowledging this one. These
                // bounded flags survive public queue saturation and owner exit.
                if matches!(outcome, JobOutcome::Succeeded { .. }) {
                    if job.kind == JobKind::ModelSetup {
                        let readiness =
                            if matches!(outcome, JobOutcome::Succeeded { revision: Some(_) }) {
                                Some(airk_kernel::service_execution::with_database_admission(
                                    || kernel.refresh_service_snapshot()?.status(),
                                )?)
                            } else {
                                None
                            };
                        if readiness.is_none() {
                            registry.set_background_pending(&job_lease, JobKind::Index, true)?;
                        }
                        registry.set_background_pending(
                            &job_lease,
                            JobKind::Vectors,
                            readiness.is_some_and(|s| {
                                matches!(
                                    s.semantic_readiness.state,
                                    airk_core::readiness::SemanticState::Pending
                                        | airk_core::readiness::SemanticState::Partial
                                )
                            }),
                        )?;
                    } else if matches!(
                        job.kind,
                        JobKind::Index
                            | JobKind::Reconcile
                            | JobKind::Rebuild
                            | JobKind::Repair
                            | JobKind::Recover
                            | JobKind::Vectors
                    ) {
                        let readiness =
                            airk_kernel::service_execution::with_database_admission(|| {
                                Ok(kernel
                                    .refresh_service_snapshot()?
                                    .status()?
                                    .semantic_readiness)
                            })?;
                        registry.set_background_pending(
                            &job_lease,
                            JobKind::Vectors,
                            matches!(
                                readiness.state,
                                airk_core::readiness::SemanticState::Pending
                                    | airk_core::readiness::SemanticState::Partial
                            ),
                        )?;
                    }
                } else if job.kind == JobKind::Vectors
                    || matches!(outcome, JobOutcome::Canceled)
                        && matches!(
                            job.kind,
                            JobKind::Index
                                | JobKind::Reconcile
                                | JobKind::Rebuild
                                | JobKind::Repair
                                | JobKind::Recover
                        )
                {
                    // A failed/canceled semantic phase requires explicit retry.
                    // Source failures leave independent older enrichment intact.
                    registry.set_background_pending(&job_lease, JobKind::Vectors, false)?;
                }
                record_watch_roots(&kernel, &mut registry, &job_lease);
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis()
                    .try_into()?;
                if matches!(outcome, JobOutcome::Failed { .. })
                    && matches!(
                        job.kind,
                        JobKind::Index
                            | JobKind::Reconcile
                            | JobKind::Rebuild
                            | JobKind::Repair
                            | JobKind::Vectors
                            | JobKind::ModelSetup
                    )
                {
                    // Failed captures can leave unreferenced objects even if
                    // the preceding cleanup was complete. Schedule a bounded
                    // check without resetting cleanup cancellation/backoff or
                    // replacing this job's original failure with cleanup work.
                    let mut pending = registry
                        .maintenance_observation(identity.project)?
                        .unwrap_or(airk_core::service::MaintenanceObservation {
                            pending_incarnations: Some(0),
                            pending_objects: true,
                            objects_checked: false,
                            removed_files: 0,
                            failed: false,
                            observed_unix_ms: now_ms,
                        });
                    pending.pending_objects = true;
                    pending.objects_checked = false;
                    pending.removed_files = 0;
                    pending.observed_unix_ms = now_ms;
                    registry.record_maintenance(&job_lease, &pending)?;
                }
                let deferred = if let JobOutcome::Failed { code } = &outcome {
                    registry.defer_job_failure(
                        &job_lease,
                        job.id,
                        code,
                        &recovery_condition,
                        now_ms,
                    )?
                } else {
                    false
                };
                if !deferred && !automatic_settled {
                    let completed = registry.complete_job_with_query_audit_at(
                        &job_lease,
                        job.id,
                        outcome.clone(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)?
                            .as_millis()
                            .try_into()?,
                        completed_maintenance.as_ref(),
                        (
                            if matches!(outcome, JobOutcome::Succeeded { .. }) {
                                audit_completion
                                    .as_ref()
                                    .map(airk_store::service::registry::AuditEvidence::Complete)
                            } else {
                                audit_drain
                                    .as_ref()
                                    .map(airk_store::service::registry::AuditEvidence::Drained)
                            },
                            query_completion.as_ref(),
                        ),
                    )?;
                    outcome = completed.outcome.context("settled job outcome missing")?;
                    if matches!(outcome, JobOutcome::Succeeded { .. })
                        && let Some(prepared) = source_completion.take()
                    {
                        prepared.complete(&mut registry)?;
                    }
                    drop(source_completion.take());
                    if job.kind == JobKind::Configure {
                        if matches!(outcome, JobOutcome::Succeeded { .. })
                            && let Some(work) = registry.configuration_work(job.id)?
                            && let airk_core::service_configuration::ProjectConfiguration::ApplySourcePolicy { policy, operation_id } = work.request.configuration
                        {
                            registry.catalog_record_policy_job(policy.index_id, operation_id)?;
                        }
                        if !split_job && !refresh_job {
                            let _ = reconcile_semantic_setup(
                                &kernel,
                                &mut registry,
                                &job_lease,
                                init.engine,
                            );
                        }
                    }
                }
                let integrity_failure = if split_job {
                    last_integrity_failure.clone()
                } else {
                    observe_native_integrity(
                        &kernel,
                        &mut registry,
                        job_lease.clone(),
                        &job_storage,
                        identity,
                        verify_admission,
                    )
                    .err()
                    .map(|error| integrity_observation_code(&error).to_owned())
                };
                if integrity_failure != last_integrity_failure {
                    job_status
                        .lock()
                        .map_err(|_| anyhow::anyhow!("work status unavailable"))?
                        .integrity_observation_failure = integrity_failure.clone();
                    last_integrity_failure = integrity_failure;
                }
                if matches!(outcome, JobOutcome::Succeeded { revision: Some(_) }) {
                    registry.reset_recovery_budget(&identity.project.to_string())?;
                    registry.reset_recovery_budget(&format!("work:{}", identity.project))?;
                    if job.kind == JobKind::Vectors {
                        registry.reset_recovery_budget("helper")?;
                    }
                    let _ =
                        reconcile_semantic_setup(&kernel, &mut registry, &job_lease, init.engine);
                }
                airk_kernel::service_execution::progress_phase(
                    airk_core::service_progress::WorkPhase::Finished,
                );
                if let Some(progress) = airk_kernel::service_execution::progress() {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .and_then(|time| u64::try_from(time.as_millis()).ok());
                    if let Some(now_ms) = now_ms {
                        let _ = registry.observe_progress(identity, &progress, now_ms);
                    }
                }
            }
            {
                let mut status = job_status
                    .lock()
                    .map_err(|_| anyhow::anyhow!("work status unavailable"))?;
                status.active = None;
                status.finished = Some(allowance.id);
            }
            job_busy.store(false, Ordering::Release);
            job_activity.notify_one();
        }
        Ok(registry)
    });
    let permits = Arc::new(Semaphore::new(1024));
    let mut connections = JoinSet::new();
    let mut idle = Instant::now();
    let mut checkpoint_sealed = false;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        if closing.load(Ordering::Acquire) {
            break;
        }
        tokio::select! {
            command=commands.recv()=>{
                match command {
                    Some(Command::Checkpoint { identity: expected, operation_id, seal }) => {
                        ensure!(expected == identity, "checkpoint worker identity changed");
                        let active_queries = {
                            let mut broker = state.broker.lock().map_err(|_| anyhow::anyhow!("query policy unavailable"))?;
                            broker.pause_evidence();
                            broker.status(Instant::now()).active_queries
                        };
                        state.admission.close();
                        let status = work_status.lock().map_err(|_| anyhow::anyhow!("work status unavailable"))?;
                        let durable_boundary = !busy.load(Ordering::Acquire) && status.active.is_none() && active_queries == 0;
                        if seal && durable_boundary {
                            checkpoint_sealed = true;
                            crate::embedding_pool_client::close();
                        }
                        let observation = protocol::WorkerCheckpointObservation {
                            schema:1, identity, operation_id, engine_sha256:init.engine,
                            admission_closed:true, sealed:checkpoint_sealed, active_work_grant:status.active,
                            active_queries,
                            durable_boundary,
                        };
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &Reply::Checkpoint { observation })?;
                        idle = Instant::now();
                    },
                    Some(Command::GrantAdmin { identity: expected, caller, nonce }) => {
                        let grant = if expected == identity && !closing.load(Ordering::Acquire) {
                            state.broker.lock().map_err(|_| anyhow::anyhow!("query policy unavailable"))?
                                .issue_native_admin(identity,caller,Instant::now(),Duration::from_secs(15)).map_err(anyhow::Error::from)
                        } else { Err(anyhow::anyhow!("foreign native administrative worker")) };
                        let reply = Reply::AdminGrant {identity,nonce,grant:grant.ok()};
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &reply)?;
                        idle=Instant::now();
                    },
                    Some(Command::ResolveSourceChange {identity:expected,nonce,begin_operation}) => {
                        let admitted=expected==identity && busy.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_ok();
                        let observation=if admitted {
                            let resolved=(||->Result<_>{
                                let mut registry=ServiceRegistry::open(&state.control,identity.service)?;
                                source_configuration::resolve(&freshness_kernel,&mut registry,&lease,begin_operation,init.engine)
                            })();
                            busy.store(false,Ordering::Release);
                            resolved.ok().map(Box::new)
                        } else {None};
                        state.admission.completed();
                        service_ipc::write_frame(&mut std::io::stdout().lock(),&Reply::SourceChangeResolution {identity,nonce,observation})?;
                        idle=Instant::now();
                    },
                    Some(Command::InstallScope { identity: expected, authority, lease_generation, expires_unix_ms }) => {
                        let installed = state.scopes.lock().map_err(|_| anyhow::anyhow!("scope policy unavailable"))?
                            .install(expected, *authority, lease_generation, expires_unix_ms);
                        let reply = match installed { Ok(scope) => Reply::ScopeInstalled { identity, scope }, Err(_) => Reply::Failed };
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &reply)?;
                    },
                    Some(Command::RenewScopeLease { identity: expected, authority, previous, lease_generation, expires_unix_ms }) => {
                        let installed = state.scopes.lock().map_err(|_| anyhow::anyhow!("scope policy unavailable"))?
                            .renew_scope_lease(expected, *authority, previous, lease_generation, expires_unix_ms);
                        let reply = match installed { Ok(scope) => Reply::ScopeInstalled { identity, scope }, Err(_) => Reply::Failed };
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &reply)?;
                    },
                    Some(Command::RevokeScope { identity: expected, scope }) => {
                        let revoked = state.scopes.lock().map_err(|_| anyhow::anyhow!("scope policy unavailable"))?.revoke(expected, scope);
                        state.admission.completed();
                        let reply = if revoked.is_ok() { Reply::ScopeRevoked { identity, scope } } else { Reply::Failed };
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &reply)?;
                    },
                    Some(Command::GrantScoped { identity: expected, caller, scope }) => {
                        let grant = (|| -> Result<ConnectionGrant> {
                            ensure!(expected == identity, "scope identity mismatch");
                            let mut scopes = state.scopes.lock().map_err(|_| anyhow::anyhow!("scope policy unavailable"))?;
                            scopes.check(Some(scope))?;
                            let mut grant = state.broker.lock().map_err(|_| anyhow::anyhow!("query policy unavailable"))?
                                .issue(identity, caller, Instant::now(), Duration::from_secs(15))?;
                            scopes.bind(&mut grant, scope)?;
                            Ok(grant)
                        })();
                        let reply = match grant { Ok(grant) => Reply::Grant { grant }, Err(_) => Reply::Failed };
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &reply)?;
                        idle = Instant::now();
                    },
                    Some(Command::ConfigureQueries { identity:expected, concurrency }) => {
                        let accepted = expected == identity && state.broker.lock().map_err(|_| anyhow::anyhow!("query policy unavailable"))?
                            .configure_queries(identity, concurrency, Instant::now()).is_ok();
                        state.admission.completed();
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &if accepted { Reply::QueriesConfigured } else { Reply::Failed })?;
                    },
                    Some(Command::Renew{identity:expected})=>{
                        let renewed=expected==identity&&airk_kernel::service_execution::renew(identity).is_ok()&&state.broker.lock().map_err(|_|anyhow::anyhow!("query policy unavailable"))?.renew(identity,Instant::now(),Duration::from_secs(300)).is_ok();
                        service_ipc::write_frame(&mut std::io::stdout().lock(),&if renewed{Reply::Renewed}else{Reply::Failed})?;
                    },
                    Some(Command::ConfigurePool{identity:expected,pool,pool_pending})=>{
                        let accepted=expected==identity && (if pool_pending{
                            crate::embedding_pool_client::update_pending(pool.map(|access| *access),true)
                        }else{crate::embedding_pool_client::update(pool.map(|access| *access))}).is_ok();
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &if accepted{Reply::PoolConfigured}else{Reply::Failed})?;
                    },
                    Some(Command::RunJob{identity:expected, allowance})=>{
                        let accepted=!checkpoint_sealed && expected==identity && allowance.identity==identity && allowance.validate().is_ok()
                            && !busy.swap(true, Ordering::AcqRel);
                        let accepted=if accepted {
                            airk_kernel::service_execution::activate(&allowance)?;
                            {let mut status=work_status.lock().map_err(|_|anyhow::anyhow!("work status unavailable"))?;status.active=Some(allowance.id);status.finished=None;status.failure=None;}
                            work_sender.try_send(allowance).is_ok()
                        }else{false};
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &if accepted{Reply::WorkAccepted}else{Reply::Failed})?;
                        idle=Instant::now();
                    },
                    Some(Command::WorkStatus{identity:expected,control,keep_for_maintenance,freshness})=>{
                        if expected==identity {freshness_kernel.observe_service_freshness(identity, freshness.map(|sample| *sample))?;}
                        if expected==identity {maintenance_waiting.store(keep_for_maintenance,Ordering::Release);}
                        if expected==identity && let Some(control)=control {airk_kernel::service_execution::control(identity,control.grant,control.paused,control.canceled,control.yield_to_source)?;}
                        let mut status=work_status.lock().map_err(|_|anyhow::anyhow!("work status unavailable"))?;
                        let reply=if expected==identity{Reply::WorkStatus{observation:protocol::WorkObservation{active:status.active,finished:status.finished,maintenance:status.maintenance.clone(),failure:status.failure.clone(),integrity_observation_failure:status.integrity_observation_failure.take(),progress:airk_kernel::service_execution::progress().map(Box::new),waiting_for:status.active.and_then(|_|airk_kernel::service_execution::waiting_for())}}}else{Reply::Failed};
                        service_ipc::write_frame(&mut std::io::stdout().lock(), &reply)?;
                    },
                    Some(Command::Grant{caller})=>{
                        let legacy = state.scopes.lock().map_err(|_|anyhow::anyhow!("scope policy unavailable"))?.legacy_allowed();
                        let grant=if legacy {state.broker.lock().map_err(|_|anyhow::anyhow!("query policy unavailable"))?.issue(identity,caller,Instant::now(),Duration::from_secs(15))} else {Err(airk_core::service::AdmissionError::InvalidGrant)};
                        let reply=match grant{Ok(grant)=>Reply::Grant{grant},Err(_)=>Reply::Failed};
                        service_ipc::write_frame(&mut std::io::stdout().lock(),&reply)?;
                        idle=Instant::now();
                    },
                    Some(Command::Stop|Command::Activate{..})|None=>break,
                }
            },
            stream=listener.accept()=>{
                let stream=match stream{Ok(stream)=>stream,Err(error) if error.downcast_ref::<local_ipc::PeerRejected>().is_some()=>continue,Err(error)=>return Err(error)};
                if let Ok(permit)=permits.clone().try_acquire_owned(){let state=state.clone();connections.spawn(async move{let _permit=permit;query(stream,state).await});idle=Instant::now();}
            },
            _=activity.notified()=>{idle=Instant::now();},
            _=connections.join_next(),if !connections.is_empty()=>{idle=Instant::now();},
            _=tick.tick()=>{
                state.scopes.lock().map_err(|_| anyhow::anyhow!("scope policy unavailable"))?.expire();
                if busy.load(Ordering::Acquire) && airk_kernel::service_execution::watchdog() {
                    // A native call that never reaches a safe boundary cannot be
                    // canceled by dropping its future. The parent retains the
                    // work allowance until this process actually exits.
                    std::process::exit(76);
                }
                if jobs.is_finished(){break;}
                if busy.load(Ordering::Acquire)||maintenance_waiting.load(Ordering::Acquire){idle=Instant::now();}
                if connections.is_empty()&&idle.elapsed()>Duration::from_secs(15){break;}
                // A generation has a bounded lifetime; replacement requires fresh brokerage.
                if state.broker.lock().map_err(|_|anyhow::anyhow!("query policy unavailable"))?.status(Instant::now()).draining_projects>0 {break;}
            }
        }
    }
    crate::embedding_pool_client::close();
    closing.store(true, Ordering::Release);
    state
        .scopes
        .lock()
        .map_err(|_| anyhow::anyhow!("scope policy unavailable"))?
        .close();
    state.admission.close();
    drop(work_sender);
    // Idle client reads must not hold an intentionally stopped owner open.
    // Blocking materialization retains its inner admission permit until actual
    // completion; process/job ownership is still held through the native drain.
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    let mut registry = jobs.await??;
    let mut state = Arc::try_unwrap(state)
        .map_err(|_| anyhow::anyhow!("project query handlers survived drain"))?;
    state.tools.close().await?;
    // Only the explicit positive capability API arms a session. Strict owners
    // take the no-op path; never clear based solely on child exit or socket EOF.
    freshness_kernel.settle_native_owner_after_owned_drain(&mut registry)?;
    drop(lease);
    Ok(())
}

#[cfg(test)]
mod response_tests;

mod source_configuration;

#[cfg(test)]
mod admission_tests {
    #[tokio::test]
    async fn delivery_rechecks_fence_after_backpressure_before_any_reply_byte() -> anyhow::Result<()>
    {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        struct CheckedWriter {
            inner: tokio::io::DuplexStream,
            fence: std::sync::Arc<std::sync::Mutex<bool>>,
            polls: usize,
        }
        impl tokio::io::AsyncWrite for CheckedWriter {
            fn poll_write(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                bytes: &[u8],
            ) -> std::task::Poll<std::io::Result<usize>> {
                assert!(
                    matches!(
                        self.fence.try_lock(),
                        Err(std::sync::TryLockError::WouldBlock)
                    ),
                    "scope guard released before write poll"
                );
                self.polls += 1;
                tokio::io::AsyncWrite::poll_write(std::pin::Pin::new(&mut self.inner), cx, bytes)
            }
            fn poll_flush(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                assert!(matches!(
                    self.fence.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
                tokio::io::AsyncWrite::poll_flush(std::pin::Pin::new(&mut self.inner), cx)
            }
            fn poll_shutdown(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                tokio::io::AsyncWrite::poll_shutdown(std::pin::Pin::new(&mut self.inner), cx)
            }
        }
        let (mut inner, mut receiver) = tokio::io::duplex(1);
        inner.write_all(b"x").await?;
        let live = std::sync::Arc::new(std::sync::Mutex::new(true));
        let mut sender = CheckedWriter {
            inner,
            fence: live.clone(),
            polls: 0,
        };
        let mut delivery = Box::pin(super::deliver_frame(&mut sender, b"protected", || {
            let guard = live.lock().unwrap();
            anyhow::ensure!(*guard, "scope revoked");
            Ok(guard)
        }));
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        use std::future::Future;
        assert!(delivery.as_mut().poll(&mut cx).is_pending());
        // Acquiring this lock is the same linearization as revoke+ack. The
        // earlier pending write retained no guard across its yield.
        *live.lock().unwrap() = false;
        let mut previous = [0];
        receiver.read_exact(&mut previous).await?;
        assert_eq!(&previous, b"x");
        assert!(delivery.await.is_err());
        assert_eq!(
            sender.polls, 1,
            "write was polled after revocation acknowledged"
        );
        drop(sender);
        let mut bytes = Vec::new();
        receiver.read_to_end(&mut bytes).await?;
        assert!(bytes.is_empty());
        Ok(())
    }

    #[test]
    fn verify_exceptional_admission_requires_actual_refusal_and_existing_full_work_grant() {
        assert_eq!(super::verify_turn_bytes(false, 768 << 20), 64 << 20);
        assert_eq!(super::verify_turn_bytes(true, (768 << 20) - 1), 64 << 20);
        assert_eq!(super::verify_turn_bytes(true, 768 << 20), 512 << 20);
        assert_eq!(super::verify_turn_bytes(true, 4 << 30), 512 << 20);
    }

    #[test]
    fn semantic_retry_after_commit_only_repeats_observation() -> anyhow::Result<()> {
        let turns = std::cell::Cell::new(0);
        let reads = std::cell::Cell::new(0);
        let result = super::semantic_publication(
            || {
                turns.set(turns.get() + 1);
                // The batch committed, but its follow-up report was excluded.
                Err(airk_store::current::CurrentReadContention.into())
            },
            || {
                reads.set(reads.get() + 1);
                match reads.get() {
                    1 => Ok((0, 0)),
                    2 => Err(airk_store::current::CurrentReadContention.into()),
                    _ => Ok((1, 42)),
                }
            },
            std::time::Duration::from_secs(1),
        )?;
        assert_eq!(result, 42);
        assert_eq!(turns.get(), 1);
        assert_eq!(reads.get(), 3);
        Ok(())
    }
    #[test]
    fn semantic_turn_waits_for_native_lock_release_and_bounds_persistent_contention()
    -> anyhow::Result<()> {
        use std::{
            fs::OpenOptions,
            time::{Duration, Instant},
        };
        let owner = tempfile::tempdir()?;
        let path = owner.path().join("owned-semantic-turn.lock");
        let held = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        held.try_lock()?;
        let (seen, waiting) = std::sync::mpsc::sync_channel(1);
        let release = std::thread::spawn(move || {
            let _ = waiting.recv();
            std::thread::sleep(Duration::from_millis(100));
            drop(held);
        });
        let mut attempts = 0;
        let result = super::retry_semantic_turn(
            || {
                attempts += 1;
                let lock = OpenOptions::new().read(true).write(true).open(&path)?;
                if let Err(error) = lock.try_lock() {
                    let _ = seen.try_send(());
                    return Err(error.into());
                }
                Ok(42)
            },
            Duration::from_secs(2),
        )?;
        release.join().unwrap();
        assert_eq!(result, 42);
        assert!(attempts >= 2);
        let held = OpenOptions::new().read(true).write(true).open(&path)?;
        held.try_lock()?;
        let started = Instant::now();
        let result = super::retry_semantic_turn(
            || {
                let lock = OpenOptions::new().read(true).write(true).open(&path)?;
                lock.try_lock()?;
                Ok(())
            },
            Duration::from_millis(50),
        );
        assert!(result.unwrap_err().is::<std::fs::TryLockError>());
        assert!(started.elapsed() < Duration::from_secs(2));
        let mut failed = 0;
        let result: anyhow::Result<()> = super::retry_semantic_turn(
            || {
                failed += 1;
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into())
            },
            Duration::from_secs(1),
        );
        assert!(result.is_err());
        assert_eq!(failed, 1);
        Ok(())
    }
    #[test]
    fn temporary_admission_keeps_the_session_and_expired_authority_requires_rebinding() {
        use airk_core::service::AdmissionError;
        for reason in [
            AdmissionError::Busy,
            AdmissionError::Expired,
            AdmissionError::StaleIdentity,
        ] {
            let reply = super::admission_rejection(reason);
            let detail: serde_json::Value =
                serde_json::from_str(reply.result["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(detail["error"]["query_submitted"], false);
            assert_eq!(
                detail["error"]["reason"],
                serde_json::to_value(reason).unwrap()
            );
            assert_eq!(reply.rebind, reason != AdmissionError::Busy);
            assert_eq!(detail["error"]["retryable"], reason == AdmissionError::Busy);
            assert_eq!(
                detail["error"]["retry_after_ms"],
                if reason == AdmissionError::Busy {
                    serde_json::json!(25)
                } else {
                    serde_json::Value::Null
                }
            );
        }
    }
}
