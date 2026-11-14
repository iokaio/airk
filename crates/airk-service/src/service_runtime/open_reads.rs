// SPDX-License-Identifier: Apache-2.0
//! U105/U106 open agent reads (`docs/split/open-agent-access.md` step 5,
//! contracts §2.9): M3 `ToolSelection::Open`, the `list_indexes` tool, the
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
    catalog_management::{CatalogItem, CatalogKind, CatalogPageRequest, MAX_PAGE_BYTES},
    identity::Incarnation,
    index_catalog::MAX_SELECTION,
    service::{ControlResponse, ToolResponseMode, ToolSelection, ToolsCatalogDetail},
    service_http::open_refusal,
    service_http_credentials::{AgentChoice, OpenConnectionState},
};
use airk_endpoint::http_endpoint::{OpenDefaultsChange, OpenDefaultsRefused};
use serde_json::{Map, Value};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
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
    let selection = ToolSelection::Open {
        index_ids: call.index_names.clone(),
        index_set_ids: call.set_names.clone(),
        connection_id: call.connection_id,
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
    // caller sees (the local owner sees the choice-on catalog).
    if airk_frontend::tool_call::is_local_help(&call.tool) {
        let choice = connection
            .as_ref()
            .map_or(AgentChoice::On, |open| open.agent_choice);
        let answer = airk_mcp::help::call_with_catalog(
            Value::Object(call.arguments),
            airk_mcp::open::catalog(choice),
        );
        return match serde_json::to_value(answer) {
            Ok(result) => ControlResponse::ToolResult { result },
            Err(error) => tools_call::failed(&error.into()),
        };
    }
    if call.tool == airk_mcp::open::LIST_INDEXES {
        return list(state, connection.as_ref(), &call).await;
    }
    let requested =
        match agent_scope::requested(&call.index_names, &call.set_names, connection.as_ref()) {
            Ok(requested) => requested,
            Err(code) => return refused(code),
        };
    let listing = match listing(state, false).await {
        Ok(listing) => listing,
        Err(code) => return refused(code),
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
    let mut arguments = call.arguments;
    if index_ids.len() == 1 {
        arguments = match tools_call::single_index_arguments(arguments) {
            Ok(arguments) => arguments,
            Err(result) => return ControlResponse::ToolResult { result },
        };
    }
    let backend = match index_ids.as_slice() {
        [index_id] => {
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
        Ok(backend) => tools_call::read_through(backend, call.tool, arguments, call.mode).await,
        Err(refusal) => Err(refusal),
    };
    match read {
        Ok(result) => ControlResponse::ToolResult { result },
        Err(tools_call::SelectionRead::Refused(code)) => refused(code),
        Err(tools_call::SelectionRead::Failed(error)) => tools_call::failed(&error),
    }
}

/// `list_indexes`: refused for a connection with choice off; its only
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
                "list_indexes takes no index or index set selection.",
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
                    "list_indexes accepts only cursor (a next_cursor string).",
                ),
            };
        }
    };
    let listing = match listing(state, true).await {
        Ok(listing) => listing,
        Err(code) => return refused(code),
    };
    match agent_scope::list_page(&listing, cursor.as_deref(), MAX_PAGE_BYTES) {
        Ok(body) => ControlResponse::ToolResult {
            result: airk_mcp::unified::success(body),
        },
        Err(code) => refused(code),
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
