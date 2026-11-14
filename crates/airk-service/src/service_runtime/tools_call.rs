// SPDX-License-Identifier: Apache-2.0
//! The M3 `ToolsCall` handler and the combined-session M2 catalog (split steps
//! 3a.2b and 3a.2d; `docs/split/contracts.md` §2.2): each resolves the
//! selection as `--service-project`, `--catalog-session` or
//! `--combined-session` with `--service-runtime-dir` did, then runs the shared
//! CLI call path (`airk_frontend::tool_call`). It lives with the service dispatch
//! because it uses the same project clients and front-end resolution the
//! dispatch already uses.
use airk_core::{
    identity::Incarnation,
    service::{
        CombinedConnectRequest, ControlResponse, ToolResponseMode, ToolSelection,
        ToolsCatalogDetail, WindowLease,
    },
};
use airk_frontend::tool_call::{
    call_with_context, is_local_help, local_help, project_backend, require_envelope, with_defaults,
};
use airk_mcp::{read_attempt::ReadAttemptContext, remote::RemoteTools, response::ResponseMode};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value};
use std::{path::Path, sync::Arc};

/// Decode a supplied original parent before any scope/backend work. None is
/// the current standalone default, minted once here rather than after selection.
pub(super) fn read_context(
    attempt: Option<airk_core::read_attempt::ReadAttemptBudget>,
) -> Result<ReadAttemptContext> {
    let context = match attempt {
        Some(attempt) => airk_host::read_attempt::decode(attempt)?,
        None => ReadAttemptContext::new(
            std::time::Instant::now() + airk_core::read_attempt::MAX_READ_BUDGET,
            Default::default(),
        ),
    };
    require_live(&context)?;
    Ok(context)
}
pub(super) fn require_live(context: &ReadAttemptContext) -> Result<()> {
    ensure!(
        !context.cancellation().is_cancelled() && std::time::Instant::now() < context.deadline(),
        "Original canonical read deadline or cancellation before dispatch/delivery"
    );
    Ok(())
}
/// Capture a known Answer's narrower absolute carrier before any local Kernel
/// dispatch. Host owns conversion; repeated ingress preserves captured expiry.
pub(super) fn admit_context(
    tool: &str,
    arguments: &Map<String, Value>,
    context: ReadAttemptContext,
) -> Result<ReadAttemptContext> {
    let context = airk_mcp::read_attempt::admit_tool_context(tool, arguments, context);
    require_live(&context)?;
    if !matches!(tool, "answer" | "airk_answer") {
        return Ok(context);
    }
    let id = match context.request_id() {
        Some(id) => id,
        None => airk_kernel::service_broker::new_instance_id()?,
    };
    let context = context.with_request_id(id)?;
    let wire = airk_host::read_attempt::encode(&context, id)?;
    context.capture_transport_attempt(wire)
}

/// The longest M2/M3 failure message, as the CLI's error line bounded it.
const MAX_MESSAGE_CHARS: usize = 4096;

fn response_mode(mode: ToolResponseMode) -> ResponseMode {
    match mode {
        ToolResponseMode::Terse => ResponseMode::Terse,
        ToolResponseMode::Full => ResponseMode::Full,
    }
}

/// The failure message: the full error chain, bounded as the CLI's error line
/// bounded it.
pub(super) fn failure_message(error: &anyhow::Error) -> String {
    format!("{error:#}")
        .chars()
        .take(MAX_MESSAGE_CHARS)
        .collect()
}

pub(super) fn failed(error: &anyhow::Error) -> ControlResponse {
    ControlResponse::ToolCallFailed {
        message: failure_message(error),
    }
}

/// A one-shot CLI process released its connections by exiting; the service
/// releases them explicitly. A cleanup failure never replaces a result.
async fn released<T>(backend: &dyn RemoteTools, result: Result<T>) -> Result<T> {
    backend.close();
    let cleanup = backend.shutdown().await;
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => {
            Err(error.context(format!("tool session cleanup also failed: {cleanup:#}")))
        }
        (result, _) => result,
    }
}

/// The session backend `--combined-session` built: the resolved combined
/// client behind the combined unified facade.
async fn combined_backend(
    control: &Path,
    request: &CombinedConnectRequest,
    client: airk_core::usage::ClientFamily,
) -> Result<Arc<dyn RemoteTools>> {
    let inner = airk_frontend::combined_frontend::resolve(None, Some(control), request).await?;
    inner.client(client);
    Ok(Arc::new(airk_mcp::unified::Unified::combined(inner, None)?))
}

/// Answer M2 for a combined session: its catalog, after it resolves.
pub(super) async fn combined_catalog(
    control: &Path,
    detail: &ToolsCatalogDetail,
    request: &CombinedConnectRequest,
) -> ControlResponse {
    let backend =
        match combined_backend(control, request, airk_core::usage::ClientFamily::Cli).await {
            Ok(backend) => backend,
            Err(error) => return failed(&error),
        };
    let catalog = backend.catalog();
    let result = airk_frontend::tool_catalog::response_from(detail, catalog);
    match released(backend.as_ref(), result).await {
        Ok(response) => response,
        Err(error) => failed(&error),
    }
}

/// Answer M3 for the service whose control root is `control`.
pub(super) async fn response(
    control: &Path,
    tool: String,
    arguments: Map<String, Value>,
    selection: ToolSelection,
    mode: ToolResponseMode,
    client: airk_core::usage::ClientFamily,
    context: ReadAttemptContext,
) -> ControlResponse {
    let context = match admit_context(&tool, &arguments, context) {
        Ok(context) => context,
        Err(error) => return failed(&error),
    };
    if let Err(error) = require_live(&context) {
        return failed(&error);
    }
    // The A2 grant belongs to an admin session; the pipe has none.
    if selection == ToolSelection::Admin {
        return ControlResponse::Failure {
            code: "admin_scope_not_granted".into(),
        };
    }
    // Open reads are dispatched to `open_reads::response`, which needs the
    // service's agent plane; a call that reaches here bypassed the dispatch.
    if matches!(selection, ToolSelection::Open { .. }) {
        return ControlResponse::Failure {
            code: super::scope_grant::SCOPE_UNAVAILABLE.into(),
        };
    }
    match run(control, tool, arguments, selection, mode, client, context).await {
        Ok(result) => ControlResponse::ToolResult { result },
        Err(error) => failed(&error),
    }
}

/// M3 under a window's W4 grant (selection authority §3.2; U99 Q3, Q5):
/// sweep and resolve the grant, run the full catalog check with the windows
/// lock released, then read through the in-process selection connection and
/// the canonical handlers. A successful read renews the window. Refusals are
/// the existing codes: `window_unknown`, `window_lease_invalid`,
/// `window_scope_not_granted`, `scoped_read_unavailable`.
pub(super) async fn window_response(
    state: &Arc<super::State>,
    window: (Incarnation, WindowLease),
    tool: String,
    arguments: Map<String, Value>,
    mode: ToolResponseMode,
    client: airk_core::usage::ClientFamily,
    context: ReadAttemptContext,
) -> ControlResponse {
    let (window_id, lease) = window;
    let context = match admit_context(&tool, &arguments, context) {
        Ok(context) => context,
        Err(error) => return failed(&error),
    };
    if let Err(error) = require_live(&context) {
        return failed(&error);
    }
    let control = state.control_root.clone();
    let (resolved, dropped) =
        super::window_plane::read_unlocked(&state.windows, &window_id, &lease, |selection| {
            super::blocking_scope_check(control, selection)
        })
        .await;
    super::revoke_dropped(state, dropped);
    let read = match resolved {
        Ok(read) => read,
        Err(code) => return ControlResponse::Failure { code },
    };
    let grant = SelectionGrant {
        selection: read.grant.selection.clone(),
        scope_identity: read.grant.scope_identity,
        selection_ref: read.grant.selection_ref,
    };
    let result = match selection_read(state, &grant, tool, arguments, mode, client, context).await {
        Ok(result) => result,
        Err(SelectionRead::Refused(code)) => {
            return ControlResponse::Failure { code: code.into() };
        }
        Err(SelectionRead::Failed(error)) => return failed(&error),
    };
    if result["isError"].as_bool() != Some(true)
        && let Ok(mut windows) = state.windows.lock()
    {
        windows.renew_after_read(&read);
    }
    ControlResponse::ToolResult { result }
}

/// An in-memory selection grant (a window's W4 or an admin session's A2) as
/// its owning plane resolved and rechecked it. The nonce inside
/// `selection_ref` reaches only the in-process admission and gate calls.
pub(super) struct SelectionGrant {
    pub(super) selection: airk_core::service::WindowScopeSelection,
    pub(super) scope_identity: airk_core::identity::Sha256Digest,
    pub(super) selection_ref: airk_core::scoped_reads::SelectionScopeRef,
}

pub(super) enum SelectionRead {
    Refused(&'static str),
    Failed(anyhow::Error),
}

/// One read under a selection grant, through the canonical handlers. One
/// index reads through one in-process selection connection whose every
/// connect re-runs admission (exact revision, live nonce, authority build).
/// Several indexes read through the combined facade (§3.6), whose members
/// are such connections and whose final receipt is issued in process.
pub(super) async fn selection_read(
    state: &Arc<super::State>,
    grant: &SelectionGrant,
    tool: String,
    arguments: Map<String, Value>,
    mode: ToolResponseMode,
    client: airk_core::usage::ClientFamily,
    context: ReadAttemptContext,
) -> std::result::Result<Value, SelectionRead> {
    let context = admit_context(&tool, &arguments, context).map_err(SelectionRead::Failed)?;
    require_live(&context).map_err(SelectionRead::Failed)?;
    if matches!(tool.as_str(), "answer" | "airk_answer") {
        super::source_delivery::retain_selection(state, &context, grant.selection_ref)
            .map_err(SelectionRead::Failed)?;
    }
    let (backend, arguments) = match grant.selection.index_ids.as_slice() {
        [index_id] => {
            let arguments = match single_index_arguments(arguments) {
                Ok(arguments) => arguments,
                Err(refusal) => return Ok(refusal),
            };
            (
                single_selection_backend(state, grant, *index_id, client).await?,
                arguments,
            )
        }
        _ => (
            combined_selection_backend(state, grant, client).await?,
            arguments,
        ),
    };
    read_through(backend, tool, arguments, mode, context).await
}

/// U107: a one-index selection or open read applies `limit_per_index` as
/// its member's `limit` (the combined rule with one member: the per-index
/// count under the overall cap); its native tool never sees the control. A
/// refusal is the tool error a combined read gives.
pub(super) fn single_index_arguments(
    mut arguments: Map<String, Value>,
) -> std::result::Result<Map<String, Value>, Value> {
    use airk_core::combined_reads::{LIMIT_PER_INDEX_TOO_LARGE, per_index_cap};
    let Some(value) = arguments.remove("limit_per_index") else {
        return Ok(arguments);
    };
    let invalid = || {
        airk_mcp::unified::failure(
            "invalid_native_arguments",
            "limit_per_index must be an integer 1..100 and limit an integer 1..100.",
        )
    };
    let per_index = value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| (1..=100).contains(value))
        .ok_or_else(invalid)?;
    let limit = match arguments.get("limit") {
        None => None,
        Some(limit) => Some(
            limit
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(invalid)?,
        ),
    };
    if per_index_cap(1, limit, per_index).is_err() {
        return Err(airk_mcp::unified::failure(
            LIMIT_PER_INDEX_TOO_LARGE,
            "limit_per_index exceeds limit or 100.",
        ));
    }
    arguments.insert("limit".into(), per_index.into());
    Ok(arguments)
}

/// One call through a selection or open backend and the canonical
/// handlers, then the backend's release (a cleanup failure never replaces
/// a result).
pub(super) async fn read_through(
    backend: Arc<dyn RemoteTools>,
    tool: String,
    arguments: Map<String, Value>,
    mode: ToolResponseMode,
    context: ReadAttemptContext,
) -> std::result::Result<Value, SelectionRead> {
    require_live(&context).map_err(SelectionRead::Failed)?;
    let result = async {
        let result = call_with_context(
            backend.as_ref(),
            tool,
            arguments,
            response_mode(mode),
            context.clone(),
        )
        .await?;
        require_envelope(&result)?;
        Ok(result)
    }
    .await;
    let result = released(backend.as_ref(), result)
        .await
        .map_err(SelectionRead::Failed);
    require_live(&context).map_err(SelectionRead::Failed)?;
    result
}

/// The service's in-process admission of one selection member: a fresh
/// `connect_selection` then `take_in_process` on every call.
fn selection_connect(
    state: &Arc<super::State>,
) -> airk_frontend::combined_frontend::SelectionConnect {
    use crate::{catalog_authority::SelectionConnectRequest, project_client::SelectionGrantFuture};
    let owner = Arc::clone(state);
    Arc::new(
        move |request: SelectionConnectRequest| -> SelectionGrantFuture {
            let owner = Arc::clone(&owner);
            Box::pin(async move {
                owner
                    .management
                    .connect_selection(request)
                    .await?
                    .take_in_process()
            })
        },
    )
}

async fn single_selection_backend(
    state: &Arc<super::State>,
    grant: &SelectionGrant,
    index_id: airk_core::index_catalog::IndexId,
    client: airk_core::usage::ClientFamily,
) -> std::result::Result<Arc<dyn RemoteTools>, SelectionRead> {
    let registration = selection_backing(
        &state.control_root,
        grant.selection.namespace,
        grant.selection_ref.catalog_revision,
        index_id,
    )
    .await
    .map_err(|_| SelectionRead::Refused(super::scope_grant::SCOPE_UNAVAILABLE))?;
    let request = airk_catalog_control::catalog_authority::SelectionConnectRequest {
        selection_ref: grant.selection_ref,
        window_selection: grant.selection.clone(),
        index_id,
    };
    let admit = selection_connect(state);
    let connect: airk_frontend::project_client::SelectionConnector = Arc::new(
        move || -> airk_frontend::project_client::SelectionGrantFuture { admit(request.clone()) },
    );
    let inner = airk_frontend::project_client::selection_tools(
        state.control_root.clone(),
        registration.binding,
        airk_frontend::project_client::SelectionSession {
            window_selection: grant.selection.clone(),
            scope_identity: grant.scope_identity,
            index_id,
            connect,
        },
    )
    .map_err(SelectionRead::Failed)?;
    inner.client(client);
    project_backend(
        inner,
        registration.binding.to_string(),
        &registration.collection,
    )
    .map_err(SelectionRead::Failed)
}

/// U105/U106(d): one whole index of an open agent read through the same
/// selection backend. `selection_ref` is the agent plane's live lease; every
/// connect and reconnect re-runs the in-process open admission (exact
/// revision, live agent nonce, whole-index authority build). The session's
/// selection names the one index and no project.
pub(super) async fn open_single_backend(
    state: &Arc<super::State>,
    selection_ref: airk_core::scoped_reads::SelectionScopeRef,
    namespace: airk_core::identity::Incarnation,
    index_id: airk_core::index_catalog::IndexId,
    client: airk_core::usage::ClientFamily,
) -> std::result::Result<Arc<dyn RemoteTools>, SelectionRead> {
    let registration = selection_backing(
        &state.control_root,
        namespace,
        selection_ref.catalog_revision,
        index_id,
    )
    .await
    .map_err(|_| SelectionRead::Refused(super::scope_grant::SCOPE_UNAVAILABLE))?;
    let request = airk_catalog_control::catalog_authority::OpenConnectRequest {
        selection_ref,
        namespace,
        index_id,
    };
    let owner = Arc::clone(state);
    let connect: airk_frontend::project_client::SelectionConnector = Arc::new(
        move || -> airk_frontend::project_client::SelectionGrantFuture {
            let owner = Arc::clone(&owner);
            let request = request.clone();
            Box::pin(async move {
                owner
                    .management
                    .connect_open(request)
                    .await?
                    .take_in_process()
            })
        },
    );
    let inner = airk_frontend::project_client::selection_tools(
        state.control_root.clone(),
        registration.binding,
        airk_frontend::project_client::SelectionSession {
            window_selection: airk_core::service::WindowScopeSelection {
                namespace,
                expected_catalog_revision: selection_ref.catalog_revision,
                index_ids: vec![index_id],
                project_ids: Vec::new(),
            },
            scope_identity: selection_ref.scope_identity,
            index_id,
            connect,
        },
    )
    .map_err(SelectionRead::Failed)?;
    inner.client(client);
    project_backend(
        inner,
        registration.binding.to_string(),
        &registration.collection,
    )
    .map_err(SelectionRead::Failed)
}

/// Several indexes: the combined facade over the selection (§3.6), the same
/// one `--combined-session` reads through. Its current snapshot must be
/// authorized at exactly the grant's revision, or the read refuses as a
/// whole (`scoped_read_unavailable`).
async fn combined_selection_backend(
    state: &Arc<super::State>,
    grant: &SelectionGrant,
    client: airk_core::usage::ClientFamily,
) -> std::result::Result<Arc<dyn RemoteTools>, SelectionRead> {
    let Some(gate) = state.catalog_gate.clone() else {
        return Err(SelectionRead::Refused(
            super::scope_grant::SCOPE_UNAVAILABLE,
        ));
    };
    let scope = airk_frontend::combined_frontend::SelectionScope {
        selection_ref: grant.selection_ref,
        window_selection: grant.selection.clone(),
        connect: selection_connect(state),
        issue: selection_issue(state, gate, state.instance),
    };
    let inner =
        airk_frontend::combined_frontend::resolve_selection(state.control_root.clone(), scope)
            .await
            .map_err(|_| SelectionRead::Refused(super::scope_grant::SCOPE_UNAVAILABLE))?;
    inner.client(client);
    match airk_mcp::unified::Unified::combined(Arc::clone(&inner), None) {
        Ok(backend) => Ok(Arc::new(backend)),
        Err(error) => {
            let _ = released(inner.as_ref(), Ok(())).await;
            Err(SelectionRead::Failed(error))
        }
    }
}

/// U105/U106(d): several whole indexes of an open agent read through the
/// combined facade's open basis (no Project, its own snapshot and cursor
/// domain). Members connect through the in-process open admission and the
/// result is delivered under the live agent nonce, as for a selection.
pub(super) async fn open_combined_backend(
    state: &Arc<super::State>,
    selection_ref: airk_core::scoped_reads::SelectionScopeRef,
    namespace: airk_core::identity::Incarnation,
    index_ids: Vec<airk_core::index_catalog::IndexId>,
    client: airk_core::usage::ClientFamily,
) -> std::result::Result<Arc<dyn RemoteTools>, SelectionRead> {
    let Some(gate) = state.catalog_gate.clone() else {
        return Err(SelectionRead::Refused(
            super::scope_grant::SCOPE_UNAVAILABLE,
        ));
    };
    let owner = Arc::clone(state);
    let scope = airk_frontend::combined_frontend::OpenScope {
        selection_ref,
        namespace,
        index_ids,
        connect: Arc::new(
            move |request: airk_catalog_control::catalog_authority::OpenConnectRequest|
                  -> airk_frontend::project_client::SelectionGrantFuture {
                let owner = Arc::clone(&owner);
                Box::pin(async move {
                    owner
                        .management
                        .connect_open(request)
                        .await?
                        .take_in_process()
                })
            },
        ),
        issue: selection_issue(state,gate, state.instance),
    };
    let inner = airk_frontend::combined_frontend::resolve_open(state.control_root.clone(), scope)
        .await
        .map_err(|_| SelectionRead::Refused(super::scope_grant::SCOPE_UNAVAILABLE))?;
    inner.client(client);
    match airk_mcp::unified::Unified::combined(Arc::clone(&inner), None) {
        Ok(backend) => Ok(Arc::new(backend)),
        Err(error) => {
            let _ = released(inner.as_ref(), Ok(())).await;
            Err(SelectionRead::Failed(error))
        }
    }
}

/// The in-process final-delivery receipt of a combined selection read: the
/// gate's clock at exactly the selection's revision, then an issue ticket
/// and the live nonce checked together under the gate lock.
fn selection_issue(
    state: &Arc<super::State>,
    gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    instance: airk_core::identity::Incarnation,
) -> airk_frontend::combined_frontend::SelectionIssue {
    use airk_core::scoped_reads::ScopeReadRefused;
    let factory = super::arbiter::policy_delivery::factory(state);
    Arc::new(
        move |selection: &airk_core::scoped_reads::SelectionScopeRef,
              scope_identity: airk_core::identity::Sha256Digest,
              result_digest: airk_core::identity::Sha256Digest,
              expected: &[airk_core::combined_context::ContextDeliveryBinding],
              context: Option<&ReadAttemptContext>|
              -> Result<airk_core::service::CombinedDeliveryReceipt> {
            let clock = gate.catalog_clock().ok_or(ScopeReadRefused)?;
            ensure!(
                clock.revision == selection.catalog_revision,
                ScopeReadRefused
            );
            let ticket = gate.begin_issue(clock.namespace, clock.revision)?;
            let receipt = || {
                gate.with_live_selection(ticket, selection, || {
                    airk_core::service::CombinedDeliveryReceipt {
                        schema: 1,
                        service_instance: instance,
                        namespace: clock.namespace,
                        catalog_revision: clock.revision,
                        scope_identity,
                        result_digest,
                    }
                })
            };
            if let Some(context) = context {
                let guard = factory(expected, context)?;
                let mut delivered = None;
                let result = guard.poll(&mut || match receipt() {
                    Ok(value) => {
                        delivered = Some(value);
                        std::task::Poll::Ready(Ok(0))
                    }
                    Err(_) => std::task::Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "native selection delivery revoked",
                    ))),
                });
                ensure!(
                    matches!(result, std::task::Poll::Ready(Ok(_))),
                    "context delivery policy stale; original work must not replay"
                );
                context.retain_delivery_guard(guard)?;
                delivered.context("native selection delivery receipt unavailable")
            } else {
                ensure!(
                    expected.is_empty(),
                    "context delivery requires original attempt"
                );
                receipt()
            }
        },
    )
}

/// The selected index's backing project, read at exactly the grant's
/// catalog revision (never rolled forward): present, not retired, import
/// complete.
async fn selection_backing(
    control: &Path,
    namespace: airk_core::identity::Incarnation,
    catalog_revision: airk_core::index_catalog::CatalogRevision,
    index_id: airk_core::index_catalog::IndexId,
) -> Result<airk_core::service::ProjectRegistration> {
    use airk_core::{
        catalog_management::{CatalogEntity, CatalogItem},
        index_catalog::SourceImportState,
        scoped_reads::ScopeReadRefused,
        service::ControlMethod,
    };
    let response = super::request(
        control,
        ControlMethod::CatalogDetail {
            entity: CatalogEntity::Index { id: index_id },
            expected_catalog_revision: Some(catalog_revision),
        },
    )
    .await?;
    let ControlResponse::CatalogDetail { detail, .. } = response else {
        return Err(ScopeReadRefused.into());
    };
    ensure!(
        detail.namespace == namespace && detail.revision == catalog_revision,
        ScopeReadRefused
    );
    let Some(CatalogItem::Index(index)) = detail.item else {
        return Err(ScopeReadRefused.into());
    };
    index.validate()?;
    ensure!(
        index.id == index_id
            && !index.tombstone
            && index.source_import == SourceImportState::Complete,
        ScopeReadRefused
    );
    Ok(index.backing)
}

/// The narrowing defaults `--combined-session` added to every call.
fn combined_defaults(
    project_ids: Option<Vec<airk_core::index_catalog::QueryProjectId>>,
    index_ids: Option<Vec<airk_core::index_catalog::IndexId>>,
    member_availability: Option<airk_core::combined_reads::MemberAvailability>,
) -> Result<Map<String, Value>> {
    ensure!(
        project_ids.is_some() || index_ids.is_none() && member_availability.is_none(),
        "index_ids and member_availability narrow an explicit project_ids selection"
    );
    let mut defaults = Map::new();
    if let Some(ids) = project_ids {
        defaults.insert("project_ids".into(), serde_json::to_value(ids)?);
    }
    if let Some(ids) = index_ids {
        defaults.insert("index_ids".into(), serde_json::to_value(ids)?);
    }
    if let Some(policy) = member_availability {
        defaults.insert("member_availability".into(), serde_json::to_value(policy)?);
    }
    Ok(defaults)
}

async fn run(
    control: &Path,
    tool: String,
    arguments: Map<String, Value>,
    selection: ToolSelection,
    mode: ToolResponseMode,
    client: airk_core::usage::ClientFamily,
    context: ReadAttemptContext,
) -> Result<Value> {
    require_live(&context)?;
    if matches!(
        selection,
        ToolSelection::Unselected | ToolSelection::Project { .. }
    ) && is_local_help(&tool)
    {
        return local_help(arguments);
    }
    // The same resolution as the CLI's selections with `--service-runtime-dir`:
    // read-only, never starting or electing a service.
    let (backend, arguments): (Arc<dyn RemoteTools>, _) = match selection {
        ToolSelection::Unselected => {
            anyhow::bail!("project binding required")
        }
        // Dispatched to `window_response` before this point; it needs the
        // service's window plane and in-process admission.
        ToolSelection::Window { .. } => {
            anyhow::bail!("window scope reads are served by the service dispatch")
        }
        // Answered by `response` before this point.
        ToolSelection::Admin => {
            anyhow::bail!("admin scope reads are served only by the admin plane")
        }
        // Dispatched to `open_reads::response` before this point; it needs
        // the service's agent plane and in-process open admission.
        ToolSelection::Open { .. } => {
            anyhow::bail!("open agent reads are served by the service dispatch")
        }
        ToolSelection::Project { binding } => {
            let project =
                airk_frontend::service_frontend::resolve_control(control, binding).await?;
            let registration = project.registration;
            let inner = airk_frontend::project_client::tools(project.control, registration.binding);
            inner.client(client);
            let backend = project_backend(
                inner,
                registration.binding.to_string(),
                &registration.collection,
            )?;
            (backend, arguments)
        }
        ToolSelection::Catalog { request } => {
            let selected =
                airk_frontend::service_frontend::resolve_scoped(None, Some(control), &request)
                    .await?;
            let registration = selected.project.registration;
            let inner = airk_frontend::project_client::scoped_tools(
                selected.project.control,
                registration.binding,
                selected.selection,
            )?;
            inner.client(client);
            let backend = project_backend(
                inner,
                registration.binding.to_string(),
                &registration.collection,
            )?;
            (backend, arguments)
        }
        ToolSelection::Combined {
            request,
            project_ids,
            index_ids,
            member_availability,
        } => {
            let defaults = combined_defaults(project_ids, index_ids, member_availability)?;
            let backend = combined_backend(control, &request, client).await?;
            match with_defaults(arguments, &defaults) {
                Ok(arguments) => (backend, arguments),
                Err(error) => return released(backend.as_ref(), Err(error)).await,
            }
        }
    };
    let result = async {
        let result = call_with_context(
            backend.as_ref(),
            tool,
            arguments,
            response_mode(mode),
            context.clone(),
        )
        .await?;
        require_envelope(&result)?;
        Ok(result)
    }
    .await;
    let result = released(backend.as_ref(), result).await;
    require_live(&context)?;
    result
}

#[cfg(test)]
mod tests;
