// SPDX-License-Identifier: Apache-2.0
//! The M3 `ToolsCall` handler and the combined-session M2 catalog (split steps
//! 3a.2b and 3a.2d; `docs/split/contracts.md` §2.2): each resolves the
//! selection as `--service-project`, `--catalog-session` or
//! `--combined-session` with `--service-runtime-dir` did, then runs the shared
//! CLI call path (`crate::tool_call`). It lives with the service dispatch
//! because it uses the same project clients and front-end resolution the
//! dispatch already uses.
use crate::tool_call::{
    call, is_local_help, local_help, project_backend, require_envelope, with_defaults,
};
use airk_core::{
    identity::Incarnation,
    service::{
        CombinedConnectRequest, ControlResponse, ToolResponseMode, ToolSelection,
        ToolsCatalogDetail, WindowLease,
    },
};
use airk_mcp::{remote::RemoteTools, response::ResponseMode};
use anyhow::{Result, ensure};
use serde_json::{Map, Value};
use std::{path::Path, sync::Arc};

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

fn failed(error: &anyhow::Error) -> ControlResponse {
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
    let inner = crate::combined_frontend::resolve(None, Some(control), request).await?;
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
    let result = crate::tool_catalog::response_from(detail, catalog);
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
) -> ControlResponse {
    // The A2 grant belongs to an admin session; the pipe has none.
    if selection == ToolSelection::Admin {
        return ControlResponse::Failure {
            code: "admin_scope_not_granted".into(),
        };
    }
    match run(control, tool, arguments, selection, mode, client).await {
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
    window_id: Incarnation,
    lease: WindowLease,
    tool: String,
    arguments: Map<String, Value>,
    mode: ToolResponseMode,
    client: airk_core::usage::ClientFamily,
) -> ControlResponse {
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
    let result = match window_read(state, &read.grant, tool, arguments, mode, client).await {
        Ok(result) => result,
        Err(WindowRead::Refused(code)) => {
            return ControlResponse::Failure { code: code.into() };
        }
        Err(WindowRead::Failed(error)) => return failed(&error),
    };
    if result["isError"].as_bool() != Some(true)
        && let Ok(mut windows) = state.windows.lock()
    {
        windows.renew_after_read(&read);
    }
    ControlResponse::ToolResult { result }
}

enum WindowRead {
    Refused(&'static str),
    Failed(anyhow::Error),
}

/// One read under the grant. One index reads through one in-process
/// selection connection whose every connect re-runs admission (exact
/// revision, live nonce, authority build). Several indexes need the
/// combined fan-out (§3.6), which is not served yet.
async fn window_read(
    state: &Arc<super::State>,
    grant: &super::window_plane::WindowGrant,
    tool: String,
    arguments: Map<String, Value>,
    mode: ToolResponseMode,
    client: airk_core::usage::ClientFamily,
) -> std::result::Result<Value, WindowRead> {
    let [index_id] = grant.selection.index_ids.as_slice() else {
        return Err(WindowRead::Refused("not_yet_served"));
    };
    let registration = selection_backing(&state.control_root, grant, *index_id)
        .await
        .map_err(|_| WindowRead::Refused(super::scope_grant::SCOPE_UNAVAILABLE))?;
    let request = crate::catalog_authority::SelectionConnectRequest {
        selection_ref: grant.selection_ref,
        window_selection: grant.selection.clone(),
        index_id: *index_id,
    };
    let owner = Arc::clone(state);
    let connect: crate::project_client::SelectionConnector =
        Arc::new(move || -> crate::project_client::SelectionGrantFuture {
            let (owner, request) = (Arc::clone(&owner), request.clone());
            Box::pin(async move {
                owner
                    .management
                    .connect_selection(request)
                    .await?
                    .take_in_process()
            })
        });
    let inner = crate::project_client::selection_tools(
        state.control_root.clone(),
        registration.binding,
        crate::project_client::SelectionSession {
            window_selection: grant.selection.clone(),
            scope_identity: grant.scope_identity,
            index_id: *index_id,
            connect,
        },
    )
    .map_err(WindowRead::Failed)?;
    inner.client(client);
    let backend = project_backend(
        inner,
        registration.binding.to_string(),
        &registration.collection,
    )
    .map_err(WindowRead::Failed)?;
    let result = async {
        let result = call(backend.as_ref(), tool, arguments, response_mode(mode)).await?;
        require_envelope(&result)?;
        Ok(result)
    }
    .await;
    released(backend.as_ref(), result)
        .await
        .map_err(WindowRead::Failed)
}

/// The selected index's backing project, read at exactly the grant's
/// catalog revision (never rolled forward): present, not retired, import
/// complete.
async fn selection_backing(
    control: &Path,
    grant: &super::window_plane::WindowGrant,
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
            expected_catalog_revision: Some(grant.catalog_revision),
        },
    )
    .await?;
    let ControlResponse::CatalogDetail { detail, .. } = response else {
        return Err(ScopeReadRefused.into());
    };
    ensure!(
        detail.namespace == grant.selection.namespace && detail.revision == grant.catalog_revision,
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
) -> Result<Value> {
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
        ToolSelection::Project { binding } => {
            let project = crate::service_frontend::resolve_control(control, binding).await?;
            let registration = project.registration;
            let inner = crate::project_client::tools(project.control, registration.binding);
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
                crate::service_frontend::resolve_scoped(None, Some(control), &request).await?;
            let registration = selected.project.registration;
            let inner = crate::project_client::scoped_tools(
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
        let result = call(backend.as_ref(), tool, arguments, response_mode(mode)).await?;
        require_envelope(&result)?;
        Ok(result)
    }
    .await;
    released(backend.as_ref(), result).await
}

#[cfg(test)]
mod tests;
