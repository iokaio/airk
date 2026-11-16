// SPDX-License-Identifier: Apache-2.0
//! U105/U106 open agent reads (`docs/split/open-agent-access.md` step 5,
//! contracts §2.9): M3 `ToolSelection::Open`, the `list_scopes` tool, the
//! open connection catalog (M2 with `connection_id`) and
//! `AgentConnectionDefaults`. `airkctl tools call --index/--set/--connection`
//! reaches [`response`] over the pipe; the HTTPS open connection reaches the
//! same function in process (step 6), so both read through one path.
//!
//! The scope decision is `airk_core::agent_scope` over a catalog listing read
//! here at one revision; the connection's defaults and choice are reread
//! from the durable credential ledger on every call, so a defaults change
//! takes effect on the next call. A resolved scope is leased through the
//! agent plane, then read through `tools_call::open_single_backend` (one
//! index) or `tools_call::open_combined_backend` (several).
use super::{State, agent_plane, revoke_agent_connection, revoke_agent_dropped, tools_call};
use airk_core::{
    agent_scope::{self, CatalogListing},
    agent_workspace::{self, WorkspaceDefaults},
    catalog_management::{CatalogItem, CatalogKind, CatalogPageRequest, MAX_PAGE_BYTES},
    identity::Incarnation,
    index_catalog::MAX_SELECTION,
    service::{ControlResponse, ToolResponseMode, ToolSelection, ToolsCatalogDetail},
    service_http::open_refusal,
    service_http_credentials::OpenConnectionState,
};
use airk_endpoint::http_endpoint::{OpenDefaultsChange, OpenDefaultsRefused};
use serde_json::{Map, Value, json};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// How long a listing waits for the registry, as the grant check waits.
const REGISTRY_WAIT: Duration = Duration::from_secs(2);
/// Bounds the page loops of one listing (a defensive stop, not a budget).
const MAX_LISTING_PAGES: usize = 4096;
/// The agent budget is full of other connections' live leases.
const AGENT_SELECTION_CAPACITY: &str = "agent_selection_capacity";

/// One open read as M3 (or the HTTPS open backend) asked for it.
pub(super) struct OpenCall {
    /// The open connection whose defaults and choice apply; `None` is the
    /// local owner, who must name a scope.
    pub(super) connection_id: Option<Incarnation>,
    pub(super) index_names: Vec<String>,
    pub(super) set_names: Vec<String>,
    /// U110: the caller's workspace folder, whose stored default applies
    /// when no index or set is named (local caller only).
    pub(super) workspace: Option<String>,
    pub(super) tool: String,
    pub(super) arguments: Map<String, Value>,
    pub(super) mode: ToolResponseMode,
    pub(super) client: airk_core::usage::ClientFamily,
}

fn refused(code: &str) -> ControlResponse {
    ControlResponse::Failure { code: code.into() }
}

/// The connection's stored open state, reread from the ledger.
async fn connection_state(
    state: &State,
    id: Incarnation,
) -> Result<OpenConnectionState, Box<ControlResponse>> {
    let Some(owner) = state.http.get() else {
        return Err(Box::new(refused("http_catalog_not_ready")));
    };
    match owner.open_state(id).await {
        Ok(Ok(open)) => Ok(open),
        Ok(Err(OpenDefaultsRefused::NotOpen | OpenDefaultsRefused::Conflict)) => {
            Err(Box::new(refused(open_refusal::AGENT_CONNECTION_NOT_OPEN)))
        }
        Err(error) => Err(Box::new(tools_call::failed(&error))),
    }
}

/// The catalog at one revision: every index, index set and membership and,
/// with `sources`, every live index's sources. Blocking; refused unless the
/// catalog is activated, of a supported contract and not fenced.
fn read_listing(control: &Path, sources: bool) -> Result<CatalogListing, &'static str> {
    let unavailable = |_| agent_scope::SCOPE_UNAVAILABLE;
    let mut registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
        control,
        Some(Instant::now() + REGISTRY_WAIT),
    )
    .map_err(unavailable)?;
    let catalog = registry
        .catalog_state()
        .map_err(unavailable)?
        .ok_or(agent_scope::SCOPE_UNAVAILABLE)?;
    let pending = registry
        .catalog_pending_fence()
        .map_err(unavailable)?
        .is_some();
    if pending
        || !catalog.activated
        || catalog.authority_contract != 1
        || catalog.policy_contract
            > airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT
    {
        return Err(agent_scope::SCOPE_UNAVAILABLE);
    }
    let mut page = |kind, index_id| -> Result<Vec<CatalogItem>, &'static str> {
        let mut items = Vec::new();
        let mut after = None;
        for _ in 0..MAX_LISTING_PAGES {
            let page = registry
                .catalog_page(&CatalogPageRequest {
                    kind,
                    index_id,
                    project_id: None,
                    after: after.take(),
                    expected_catalog_revision: Some(catalog.revision),
                    limit: MAX_SELECTION,
                    max_bytes: MAX_PAGE_BYTES,
                })
                .map_err(unavailable)?;
            if page.namespace != catalog.namespace || page.revision != catalog.revision {
                return Err(agent_scope::SCOPE_UNAVAILABLE);
            }
            items.extend(page.items);
            match page.continuation {
                Some(next) => after = Some(next),
                None => return Ok(items),
            }
        }
        Err(agent_scope::SCOPE_UNAVAILABLE)
    };
    let mut listing = CatalogListing {
        namespace: catalog.namespace,
        revision: Some(catalog.revision),
        indexes: Vec::new(),
        projects: Vec::new(),
        memberships: Vec::new(),
        sources: Vec::new(),
    };
    for kind in [
        CatalogKind::Indexes,
        CatalogKind::Projects,
        CatalogKind::Memberships,
    ] {
        for item in page(kind, None)? {
            match item {
                CatalogItem::Index(index) => listing.indexes.push(index),
                CatalogItem::Project(project) => listing.projects.push(project),
                CatalogItem::Membership(membership) => listing.memberships.push(membership),
                _ => return Err(agent_scope::SCOPE_UNAVAILABLE),
            }
        }
    }
    if sources {
        let live: Vec<_> = listing
            .indexes
            .iter()
            .filter(|index| !index.tombstone)
            .map(|index| index.id)
            .collect();
        for id in live {
            for item in page(CatalogKind::Sources, Some(id))? {
                let CatalogItem::Source(source) = item else {
                    return Err(agent_scope::SCOPE_UNAVAILABLE);
                };
                listing.sources.push(source);
            }
        }
    }
    Ok(listing)
}

async fn listing(state: &State, sources: bool) -> Result<CatalogListing, &'static str> {
    let control = state.control_root.clone();
    tokio::task::spawn_blocking(move || read_listing(&control, sources))
        .await
        .unwrap_or(Err(agent_scope::SCOPE_UNAVAILABLE))
}

/// Answer one open read (M3 `ToolSelection::Open`).
pub(super) async fn response(state: &Arc<State>, call: OpenCall) -> ControlResponse {
    match tools_call::read_context(None) {
        Ok(context) => response_with_context(state, call, context).await,
        Err(error) => tools_call::failed(&error),
    }
}
pub(super) async fn response_with_context(
    state: &Arc<State>,
    call: OpenCall,
    context: airk_mcp::read_attempt::ReadAttemptContext,
) -> ControlResponse {
    let context = match tools_call::admit_context(&call.tool, &call.arguments, context) {
        Ok(context) => context,
        Err(error) => return tools_call::failed(&error),
    };
    if let Err(error) = tools_call::require_live(&context) {
        return tools_call::failed(&error);
    }
    let result = response_inner(state, call, context.clone()).await;
    if let Err(error) = tools_call::require_live(&context) {
        return tools_call::failed(&error);
    }
    result
}
async fn response_inner(
    state: &Arc<State>,
    call: OpenCall,
    context: airk_mcp::read_attempt::ReadAttemptContext,
) -> ControlResponse {
    let selection = ToolSelection::Open {
        index_ids: call.index_names.clone(),
        index_set_ids: call.set_names.clone(),
        connection_id: call.connection_id,
        workspace: call.workspace.clone(),
    };
    if let Err(error) = selection.validate() {
        return tools_call::failed(&error);
    }
    let connection = match call.connection_id {
        Some(id) => match connection_state(state, id).await {
            Ok(open) => Some(open),
            Err(response) => return *response,
        },
        None => None,
    };
    // Help reads no index: answered in process against the open catalog this
    // caller sees (the local caller sees the U110 local catalog).
    if airk_frontend::tool_call::is_local_help(&call.tool) {
        let catalog = connection
            .as_ref()
            .map_or_else(airk_mcp::open::local_catalog, |open| {
                airk_mcp::open::catalog(open.agent_choice)
            });
        let answer = airk_mcp::help::call_with_catalog(Value::Object(call.arguments), catalog);
        return match serde_json::to_value(answer) {
            Ok(mut result) => {
                complete_help_envelope(&mut result);
                ControlResponse::ToolResult { result }
            }
            Err(error) => tools_call::failed(&error.into()),
        };
    }
    if call.tool == airk_mcp::open::LIST_SCOPES {
        return list(state, connection.as_ref(), &call).await;
    }
    let listing = match listing(state, false).await {
        Ok(listing) => listing,
        Err(code) => return refused(code),
    };
    let named = !call.index_names.is_empty() || !call.set_names.is_empty();
    // U110: the local caller names its scope, or names a workspace whose
    // stored default applies; nothing else is ambient.
    let stored_default;
    let mut scope = json!({"source": if named { "named" } else { "connection_default" }});
    let requested = if connection.is_none() && !named {
        let Some(workspace) = call.workspace.as_deref() else {
            return scope_required(None);
        };
        let defaults = match load_workspace_defaults(state, listing.namespace).await {
            Ok(defaults) => defaults,
            Err(error) => return tools_call::failed(&error),
        };
        let Some(entry) = defaults.matching(workspace) else {
            return scope_required(Some(workspace));
        };
        stored_default = entry.clone();
        scope = json!({"source": "workspace_default", "workspace_folder": stored_default.folder});
        agent_scope::Requested::Defaults {
            indexes: &stored_default.index_ids,
            sets: &stored_default.index_set_ids,
        }
    } else {
        match agent_scope::requested(&call.index_names, &call.set_names, connection.as_ref()) {
            Ok(requested) => requested,
            Err(code) => return refused(code),
        }
    };
    let index_ids = match agent_scope::resolve(&listing, requested) {
        Ok(ids) => ids,
        // U109: a choice-off call outside the assigned set names what is
        // outside it, in the tool error the CLI and MCP both receive.
        Err(code) if code == open_refusal::AGENT_SCOPE_CHOICE_DISABLED => {
            let outside =
                agent_scope::outside_assigned_set(&listing, requested).unwrap_or_default();
            return ControlResponse::ToolResult {
                result: airk_mcp::unified::failure(
                    code,
                    &agent_scope::outside_message(&listing, &outside),
                ),
            };
        }
        Err(code) => return refused(code),
    };
    let Some(revision) = listing.revision else {
        return refused(agent_scope::SCOPE_UNAVAILABLE);
    };
    let index_ids_text: Vec<String> = index_ids.iter().map(ToString::to_string).collect();
    let identity = match agent_plane::open_scope_identity(listing.namespace, revision, &index_ids) {
        Ok(identity) => identity,
        Err(error) => return tools_call::failed(&error),
    };
    let caller = call.connection_id.unwrap_or(state.native_caller);
    let (leased, dropped) = match state.agents.lock() {
        Ok(mut agents) => agents.lease(caller, identity, revision),
        Err(_) => return refused(agent_scope::SCOPE_UNAVAILABLE),
    };
    revoke_agent_dropped(state, dropped);
    let selection_ref = match leased {
        Ok(selection_ref) => selection_ref,
        Err(agent_plane::LeaseRefused::Capacity) => return refused(AGENT_SELECTION_CAPACITY),
        Err(agent_plane::LeaseRefused::Stale | agent_plane::LeaseRefused::Unavailable) => {
            return refused(agent_scope::SCOPE_UNAVAILABLE);
        }
    };
    let mut context = context;
    let mut arguments = call.arguments;
    if matches!(
        call.tool.as_str(),
        "answer" | "airk_answer" | "search" | "airk_search"
    ) && let Err(error) =
        super::source_delivery::retain_selection(state, &context, selection_ref)
    {
        return tools_call::failed(&error);
    }
    let explicit_subset = tools_call::uses_combined_arguments(&arguments);
    if index_ids.len() == 1 && !explicit_subset {
        arguments = match tools_call::single_index_arguments(arguments) {
            Ok(arguments) => arguments,
            Err(result) => return ControlResponse::ToolResult { result },
        };
        context = match tools_call::singleton_focus::admit(
            state, &call.tool, &arguments, context, identity, &index_ids,
        ) {
            Ok(context) => context,
            Err(error) => return tools_call::failed(&error),
        };
    }
    let backend = match index_ids.as_slice() {
        [index_id] if !explicit_subset => {
            tools_call::open_single_backend(
                state,
                selection_ref,
                listing.namespace,
                *index_id,
                call.client,
            )
            .await
        }
        _ => {
            tools_call::open_combined_backend(
                state,
                selection_ref,
                listing.namespace,
                index_ids,
                call.client,
            )
            .await
        }
    };
    let read = match backend {
        Ok(backend) => {
            tools_call::read_through(backend, call.tool, arguments, call.mode, context).await
        }
        Err(refusal) => Err(refusal),
    };
    match read {
        Ok(mut result) => {
            // U110: every open read reports how its scope was chosen.
            scope["index_ids"] = json!(index_ids_text);
            with_scope(&mut result, scope);
            ControlResponse::ToolResult { result }
        }
        Err(tools_call::SelectionRead::Refused(code)) => refused(code),
        Err(tools_call::SelectionRead::Failed(error)) => tools_call::failed(&error),
    }
}

/// `list_scopes`: refused for a connection with choice off; its only
/// argument is `cursor` and it takes no scope.
async fn list(
    state: &State,
    connection: Option<&OpenConnectionState>,
    call: &OpenCall,
) -> ControlResponse {
    if let Err(code) = agent_scope::list_allowed(connection) {
        return refused(code);
    }
    if !call.index_names.is_empty() || !call.set_names.is_empty() {
        return ControlResponse::ToolResult {
            result: airk_mcp::unified::failure(
                "INVALID_SCHEMA",
                "list_scopes takes no index or index set selection.",
            ),
        };
    }
    let cursor = match (call.arguments.len(), call.arguments.get("cursor")) {
        (0, None) => None,
        (1, Some(Value::String(cursor))) if !cursor.is_empty() && cursor.len() <= 64 => {
            Some(cursor.clone())
        }
        _ => {
            return ControlResponse::ToolResult {
                result: airk_mcp::unified::failure(
                    "INVALID_SCHEMA",
                    "list_scopes accepts only cursor (a next_cursor string).",
                ),
            };
        }
    };
    let listing = match listing(state, true).await {
        Ok(listing) => listing,
        Err(code) => return refused(code),
    };
    let mut body = match agent_scope::list_page(&listing, cursor.as_deref(), MAX_PAGE_BYTES) {
        Ok(body) => body,
        Err(code) => return refused(code),
    };
    // U110: the first page also lists the stored workspace defaults and,
    // for a named workspace, the one that applies to it.
    if cursor.is_none() {
        let defaults = match load_workspace_defaults(state, listing.namespace).await {
            Ok(defaults) => defaults,
            Err(error) => return tools_call::failed(&error),
        };
        if let Some(object) = body.as_object_mut() {
            if let Some(workspace) = call.workspace.as_deref() {
                object.insert(
                    "workspace_default".into(),
                    json!(defaults.matching(workspace)),
                );
            }
            object.insert("workspace_defaults".into(), json!(defaults.entries));
        }
    }
    ControlResponse::ToolResult {
        result: airk_mcp::unified::success(body),
    }
}

/// M2 for an open connection: the catalog of its current agent choice.
pub(super) async fn catalog(
    state: &State,
    detail: &ToolsCatalogDetail,
    connection_id: Incarnation,
) -> ControlResponse {
    let open = match connection_state(state, connection_id).await {
        Ok(open) => open,
        Err(response) => return *response,
    };
    match airk_frontend::tool_catalog::response_from(
        detail,
        airk_mcp::open::catalog(open.agent_choice),
    ) {
        Ok(response) => response,
        Err(error) => tools_call::failed(&error),
    }
}

/// `AgentConnectionDefaults`: check the named ids against the current
/// catalog, compare-and-swap through the credential store (the credential
/// generation never changes), then drop the connection's agent leases when
/// the defaults changed.
pub(super) async fn defaults(
    state: &Arc<State>,
    connection_id: Incarnation,
    change: OpenDefaultsChange,
) -> ControlResponse {
    let Some(owner) = state.http.get() else {
        return refused("http_catalog_not_ready");
    };
    if change.index_ids.is_some() || change.index_set_ids.is_some() {
        let listing = match listing(state, false).await {
            Ok(listing) => listing,
            Err(code) => return refused(code),
        };
        if let Err(code) = agent_scope::validate_defaults(
            &listing,
            change.index_ids.as_deref().unwrap_or_default(),
            change.index_set_ids.as_deref().unwrap_or_default(),
        ) {
            return refused(code);
        }
    }
    let expected = change.expected_defaults_revision;
    match owner.set_open_defaults(connection_id, change).await {
        Ok(Ok(open)) => {
            if open.defaults_revision != expected {
                revoke_agent_connection(state, connection_id);
            }
            ControlResponse::AgentConnectionDefaults {
                connection_id,
                open,
            }
        }
        Ok(Err(OpenDefaultsRefused::NotOpen)) => refused(open_refusal::AGENT_CONNECTION_NOT_OPEN),
        Ok(Err(OpenDefaultsRefused::Conflict)) => refused(open_refusal::AGENT_DEFAULTS_CONFLICT),
        Err(_) => refused("agent_defaults_unavailable"),
    }
}

/// U110: add `read_scope` to a read result's structured copy and, when its text
/// copy is a JSON object (the full document, or a terse rendering of it), to
/// the text copy too, so every copy of the one result names its scope (not
/// on a receipt-bound multi-index body). A
/// distinct key: some results already carry their own top-level `scope`.
fn with_scope(result: &mut Value, scope: Value) {
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return;
    }
    let text = result
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok());
    // A delivered multi-index body is bound byte for byte by its delivery
    // receipt (no transport may add a field); it names its open scope in
    // `selection.scope_identity` instead.
    let receipt_bound =
        |body: Option<&Value>| body.is_some_and(|body| body.get("delivery_observation").is_some());
    if receipt_bound(result.get("structuredContent")) || receipt_bound(text.as_ref()) {
        return;
    }
    if let Some(structured) = result
        .get_mut("structuredContent")
        .and_then(Value::as_object_mut)
    {
        structured.insert("read_scope".into(), scope.clone());
    }
    // The text copy is annotated whether or not a structured copy exists
    // (native worker replies may carry only a JSON text body).
    if let Some(Value::Object(mut text)) = text {
        text.insert("read_scope".into(), scope);
        result["content"][0]["text"] = Value::String(Value::Object(text).to_string());
    }
}

/// U110: an open Help answer carries the same structured copy as every other
/// public envelope: when only a JSON text body is present (help refusals),
/// it is also exposed as `structuredContent`.
fn complete_help_envelope(result: &mut Value) {
    if result.get("structuredContent").is_some() {
        return;
    }
    let body = result
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .filter(Value::is_object);
    if let (Some(body), Some(envelope)) = (body, result.as_object_mut()) {
        envelope.insert("structuredContent".into(), body);
    }
}

/// U110: the refusal of a read that named no scope it could use.
fn scope_required(workspace: Option<&str>) -> ControlResponse {
    let message = match workspace {
        Some(workspace) => format!(
            "No default scope is stored for workspace {workspace:?} or any folder containing it. Name the indexes or index sets to read (index_ids, index_set_ids), or apply a search scope in the AIRK Explorer for that folder. Call list_scopes to see indexes, index sets and workspace defaults."
        ),
        None => "Name the indexes or index sets to read (index_ids, index_set_ids), or pass workspace (your working folder) to use the default scope applied for it in the AIRK Explorer. Call list_scopes to see indexes, index sets and workspace defaults.".to_owned(),
    };
    ControlResponse::ToolResult {
        result: airk_mcp::unified::failure(open_refusal::SCOPE_REQUIRED, &message),
    }
}

/// The service's private HTTP directory, which holds the defaults file.
fn workspace_root(state: &State) -> anyhow::Result<std::path::PathBuf> {
    let root = state.control_root.join("http-1");
    airk_ipc::endpoint::prepare_root(&root)?;
    Ok(root)
}

/// Serializes read-modify-write of the defaults file within the service.
static WORKSPACE_DEFAULTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn read_workspace_defaults(
    root: &Path,
    namespace: Incarnation,
) -> anyhow::Result<WorkspaceDefaults> {
    let Some(bytes) = airk_ipc::private_record::read_private(
        root,
        agent_workspace::FILE,
        agent_workspace::MAX_FILE_BYTES,
    )?
    else {
        return Ok(WorkspaceDefaults::empty(namespace));
    };
    let stored: WorkspaceDefaults = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid workspace defaults file"))?;
    stored.validate()?;
    // Defaults of another catalog namespace (a recreated catalog) never apply.
    Ok(if stored.namespace == namespace {
        stored
    } else {
        WorkspaceDefaults::empty(namespace)
    })
}

/// The stored workspace defaults of `namespace`.
async fn load_workspace_defaults(
    state: &State,
    namespace: Incarnation,
) -> anyhow::Result<WorkspaceDefaults> {
    let root = workspace_root(state)?;
    let _serial = WORKSPACE_DEFAULTS.lock().await;
    tokio::task::spawn_blocking(move || read_workspace_defaults(&root, namespace)).await?
}

/// `WorkspaceScopeDefault`: check the ids against the current catalog, then
/// set or clear the default of each folder and answer every stored default.
pub(super) async fn set_workspace_default(
    state: &Arc<State>,
    folders: Vec<String>,
    index_ids: Vec<airk_core::index_catalog::IndexId>,
    index_set_ids: Vec<airk_core::index_catalog::QueryProjectId>,
) -> ControlResponse {
    let listing = match listing(state, false).await {
        Ok(listing) => listing,
        Err(code) => return refused(code),
    };
    if (!index_ids.is_empty() || !index_set_ids.is_empty())
        && let Err(code) = agent_scope::validate_defaults(&listing, &index_ids, &index_set_ids)
    {
        return refused(code);
    }
    let root = match workspace_root(state) {
        Ok(root) => root,
        Err(error) => return tools_call::failed(&error),
    };
    let _serial = WORKSPACE_DEFAULTS.lock().await;
    let namespace = listing.namespace;
    let written = tokio::task::spawn_blocking(move || -> anyhow::Result<WorkspaceDefaults> {
        let mut defaults = read_workspace_defaults(&root, namespace)?;
        let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        defaults.apply(&folders, &index_ids, &index_set_ids, now)?;
        airk_ipc::private_record::write_private(
            &root,
            agent_workspace::FILE,
            &serde_json::to_vec(&defaults)?,
            agent_workspace::MAX_FILE_BYTES,
            true,
        )?;
        Ok(defaults)
    })
    .await;
    match written {
        Ok(Ok(defaults)) => ControlResponse::WorkspaceScopeDefaults {
            entries: defaults.entries,
        },
        Ok(Err(error)) => tools_call::failed(&error),
        Err(error) => tools_call::failed(&error.into()),
    }
}

/// `WorkspaceScopeDefaults`: every stored default of the current catalog.
pub(super) async fn workspace_defaults_response(state: &Arc<State>) -> ControlResponse {
    let listing = match listing(state, false).await {
        Ok(listing) => listing,
        Err(code) => return refused(code),
    };
    match load_workspace_defaults(state, listing.namespace).await {
        Ok(defaults) => ControlResponse::WorkspaceScopeDefaults {
            entries: defaults.entries,
        },
        Err(error) => tools_call::failed(&error),
    }
}

#[cfg(test)]
mod retained_source_tests {
    use super::*;
    use airk_core::{
        index_catalog::{IndexId, SourceRootId},
        service::ProjectRegistration,
    };

    #[test]
    fn historical_retained_sources_refuse_the_real_open_catalog_listing() -> anyhow::Result<()> {
        for variant in ["ordinary", "source", "root"] {
            let owner = tempfile::tempdir()?;
            let lease = airk_store::service::ServiceLease::acquire(owner.path())?;
            let lease = super::super::first_start::ensure_catalog(lease)?;
            let control = lease.root();
            let connection = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
            let index = IndexId::from_identity(Incarnation::from_bytes([91; 16]));
            let source = SourceRootId::from_identity(Incarnation::from_bytes([92; 16]));
            let archive = "/fixture/.airk/agent-edits/v1/private-transaction";
            let root = if variant == "root" {
                archive
            } else {
                "/fixture/source"
            };
            let source_path = if variant == "source" {
                archive
            } else {
                "/fixture/source"
            };
            let backing = ProjectRegistration::new(
                root.into(),
                "/fixture/.airk/data".into(),
                "archive-read".into(),
            )?;
            connection.execute("INSERT INTO index_definitions VALUES(?1,?2,NULL,?3,?4,'collections-1.4',?5,'Fixture',1,1,0,NULL,0,'complete')",
                rusqlite::params![index.to_string(), backing.binding.to_string(), backing.root, backing.data_root, backing.collection])?;
            connection.execute(
                "INSERT INTO index_sources VALUES(?1,?2,?3,1,1)",
                rusqlite::params![source.to_string(), index.to_string(), source_path],
            )?;
            if variant == "ordinary" {
                let ordinary = read_listing(control, true).map_err(anyhow::Error::msg)?;
                assert_eq!(ordinary.sources.len(), 1);
                assert_eq!(ordinary.sources[0].canonical_path, "/fixture/source");
            } else {
                assert!(
                    matches!(
                        read_listing(control, variant == "source"),
                        Err(agent_scope::SCOPE_UNAVAILABLE)
                    ),
                    "historical {variant} must refuse"
                );
            }
            assert_eq!(
                connection.query_row(
                    "SELECT backing_root FROM index_definitions WHERE id=?1",
                    [index.to_string()],
                    |row| row.get::<_, String>(0)
                )?,
                root
            );
            assert_eq!(
                connection.query_row(
                    "SELECT canonical_path FROM index_sources WHERE id=?1",
                    [source.to_string()],
                    |row| row.get::<_, String>(0)
                )?,
                source_path
            );
        }
        Ok(())
    }
}
