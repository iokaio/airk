// SPDX-License-Identifier: Apache-2.0
//! `agents endpoint`, `agents info` and `agents revoke` (split step 3e.2):
//! the local HTTPS endpoint and connections, as `airk runtime http-endpoint`,
//! `http-connection-info` and `http-revoke` printed them. None returns a
//! credential except `agents credential` (3e.3), the offline header helper.
//! `config` (3e.6) classifies and edits agent MCP configuration files.
use crate::contract::{ErrorLine, ExitClass};

pub(crate) mod config;
pub(crate) mod manage;
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse},
};
use serde_json::Value;
use std::path::Path;

fn refused(message: &str) -> ErrorLine {
    ErrorLine::new(ExitClass::Refused, "operation_failed", false, message)
}

/// `airk`'s refusal for an unexpected native answer.
fn native_refusal(response: &ControlResponse) -> ErrorLine {
    let mut line = refused(&format!(
        "native HTTP setup refused: {}",
        serde_json::to_string(response).unwrap_or_default()
    ));
    line.native = serde_json::to_value(response).ok();
    line
}

fn answer(response: ControlResponse) -> Result<Value, ErrorLine> {
    match response {
        ControlResponse::AgentEndpoint { endpoint } => {
            serde_json::to_value(endpoint).map_err(|error| refused(&error.to_string()))
        }
        ControlResponse::AgentConnectionInfo { connection } => {
            serde_json::to_value(connection).map_err(|error| refused(&error.to_string()))
        }
        ControlResponse::AgentConnectionOwnership { result }
        | ControlResponse::AgentConnectionVerified { result } => Ok(result),
        ControlResponse::AgentRefused { message } => Err(refused(&message)),
        other => Err(crate::service::refusal(&other)),
    }
}

/// M15a: the checked endpoint descriptor.
pub fn endpoint(control: &Path, version: &str) -> Result<Value, ErrorLine> {
    answer(crate::service::request(
        control,
        ControlMethod::AgentEndpoint {},
        version,
    )?)
}

/// M15: an existing connection's references.
pub fn info(control: &Path, version: &str, connection_id: Incarnation) -> Result<Value, ErrorLine> {
    answer(crate::service::request(
        control,
        ControlMethod::AgentConnectionInfo { connection_id },
        version,
    )?)
}

/// M16 (`agents list --connection-id`): whether this installation owns the
/// connection, as `airk runtime lookup-http-connection` answered without its
/// selected-runtime field `active`.
pub fn ownership(
    control: &Path,
    version: &str,
    connection_id: Incarnation,
) -> Result<Value, ErrorLine> {
    answer(crate::service::request(
        control,
        ControlMethod::AgentConnectionOwnership { connection_id },
        version,
    )?)
}

/// V1 (`agents verify`): the service probes its own `/mcp` with the stored
/// credential; the token never reaches this process.
pub fn verify(
    control: &Path,
    version: &str,
    connection_id: Incarnation,
) -> Result<Value, ErrorLine> {
    answer(crate::service::request(
        control,
        ControlMethod::AgentConnectionVerify { connection_id },
        version,
    )?)
}

/// `agents default` inputs (U105).
pub struct DefaultsRequest {
    /// Ids or exact names; with `sets`, the whole new default scope.
    pub indexes: Vec<String>,
    pub sets: Vec<String>,
    /// Empty the default scope.
    pub clear: bool,
    pub agent_choice: Option<airk_core::service_http_credentials::AgentChoice>,
    /// The stored defaults revision to replace; read from `agents info`
    /// when absent.
    pub expected_revision: Option<u64>,
}

/// The current index and index set pages at one catalog revision, for name
/// resolution by the shared rule (`airk_core::agent_scope`).
fn catalog_listing(
    control: &Path,
    version: &str,
) -> Result<airk_core::agent_scope::CatalogListing, ErrorLine> {
    use airk_core::catalog_management::{
        CatalogItem, CatalogKind, CatalogPageRequest, MAX_PAGE_BYTES,
    };
    let mut listing: Option<airk_core::agent_scope::CatalogListing> = None;
    for kind in [CatalogKind::Indexes, CatalogKind::Projects] {
        let mut after = None;
        loop {
            let expected = listing.as_ref().and_then(|listing| listing.revision);
            let response = crate::service::request(
                control,
                ControlMethod::CatalogPage {
                    request: CatalogPageRequest {
                        kind,
                        index_id: None,
                        project_id: None,
                        after: after.take(),
                        expected_catalog_revision: expected,
                        limit: airk_core::index_catalog::MAX_SELECTION,
                        max_bytes: MAX_PAGE_BYTES,
                    },
                },
                version,
            )?;
            let ControlResponse::CatalogPage { page, .. } = response else {
                return Err(crate::service::refusal(&response));
            };
            let listing = listing.get_or_insert_with(|| airk_core::agent_scope::CatalogListing {
                namespace: page.namespace,
                revision: Some(page.revision),
                indexes: Vec::new(),
                projects: Vec::new(),
                memberships: Vec::new(),
                sources: Vec::new(),
            });
            if listing.revision != Some(page.revision) {
                return Err(refused(
                    "the catalog changed while it was read; run the command again",
                ));
            }
            for item in page.items {
                match item {
                    CatalogItem::Index(index) => listing.indexes.push(index),
                    CatalogItem::Project(project) => listing.projects.push(project),
                    _ => return Err(refused("unexpected catalog page item")),
                }
            }
            match page.continuation {
                Some(next) => after = Some(next),
                None => break,
            }
        }
    }
    listing.ok_or_else(|| refused("catalog unavailable"))
}

fn name_refused(code: &str, name: &str) -> ErrorLine {
    ErrorLine::new(ExitClass::Refused, code, false, &format!("{code}: {name}"))
}

/// `agents default` (U105): resolve names by the shared rule, read the
/// current defaults revision when none is given, then
/// `AgentConnectionDefaults`. Prints `{"schema":1,"connectionId","open"}`.
pub fn default_scope(
    control: &Path,
    version: &str,
    connection_id: Incarnation,
    request: DefaultsRequest,
) -> Result<Value, ErrorLine> {
    let (index_ids, index_set_ids) = if request.clear {
        (Some(Vec::new()), Some(Vec::new()))
    } else if request.indexes.is_empty() && request.sets.is_empty() {
        (None, None)
    } else {
        let listing = catalog_listing(control, version)?;
        let mut indexes = Vec::new();
        for name in &request.indexes {
            let id = airk_core::agent_scope::index_by_name(&listing, name)
                .map_err(|code| name_refused(code, name))?;
            if !indexes.contains(&id) {
                indexes.push(id);
            }
        }
        let mut sets = Vec::new();
        for name in &request.sets {
            let id = airk_core::agent_scope::set_by_name(&listing, name)
                .map_err(|code| name_refused(code, name))?;
            if !sets.contains(&id) {
                sets.push(id);
            }
        }
        (Some(indexes), Some(sets))
    };
    let expected_defaults_revision = match request.expected_revision {
        Some(revision) => revision,
        None => {
            let connection: airk_core::service_http::HttpConnectionInfo =
                serde_json::from_value(info(control, version, connection_id)?)
                    .map_err(|error| refused(&error.to_string()))?;
            connection
                .open
                .ok_or_else(|| {
                    ErrorLine::new(
                        ExitClass::Refused,
                        airk_core::service_http::open_refusal::AGENT_CONNECTION_NOT_OPEN,
                        false,
                        "this connection is not an open agent connection",
                    )
                })?
                .defaults_revision
        }
    };
    let response = crate::service::request(
        control,
        ControlMethod::AgentConnectionDefaults {
            connection_id,
            expected_defaults_revision,
            index_ids,
            index_set_ids,
            agent_choice: request.agent_choice,
        },
        version,
    )?;
    match response {
        ControlResponse::AgentConnectionDefaults {
            connection_id: changed,
            open,
        } if changed == connection_id => Ok(serde_json::json!({
            "schema": 1,
            "connectionId": changed,
            "open": open,
        })),
        other => Err(crate::service::refusal(&other)),
    }
}

/// `http-revoke`: the service must accept requests; the revoke is fenced to
/// the observed instance and must name the connection asked for.
pub fn revoke(
    control: &Path,
    version: &str,
    connection_id: Incarnation,
) -> Result<Value, ErrorLine> {
    let health = match crate::service::request(control, ControlMethod::Health, version)? {
        ControlResponse::Health { observation } => observation,
        other => return Err(native_refusal(&other)),
    };
    if !health.policy.accepting {
        return Err(refused("native service is draining"));
    }
    let response = crate::service::request_fenced(
        control,
        ControlMethod::HttpRevoke { connection_id },
        version,
        Some(health.instance),
    )?;
    match &response {
        ControlResponse::HttpRevoked {
            connection_id: revoked,
        } if *revoked == connection_id => {
            serde_json::to_value(&response).map_err(|error| refused(&error.to_string()))
        }
        other => Err(native_refusal(other)),
    }
}

/// `agents credential` (U75, U90, U92): the bearer header of an existing,
/// unrevoked connection for an agent's header helper. It reads only the
/// owner-private ledger and token under `<control>/http-1`, needs no running
/// service, never enrolls, rotates or writes, and no error contains the token.
/// The ledger must be bound to this control root, this user and its own
/// namespace; the service still checks the token against its live namespace.
pub fn credential(control: &Path, connection_id: Incarnation) -> Result<Value, ErrorLine> {
    use airk_core::service_http_credentials::{self as credentials, InstallationBinding, Ledger};
    let fail = |error: String| refused(&error);
    let private = control.join("http-1");
    airk_ipc::endpoint::require_existing_root(&private)
        .map_err(|error| fail(format!("{error:#}")))?;
    let ledger = private_file(&private.join(credentials::STORE), credentials::MAX_STORE)
        .map_err(fail)?
        .ok_or_else(|| fail("HTTP credential ledger is unobserved".into()))?;
    let ledger = Ledger::parse(&ledger).map_err(|error| fail(format!("{error:#}")))?;
    let user = airk_ipc::endpoint::current_user().map_err(|error| fail(format!("{error:#}")))?;
    let canonical = control
        .canonicalize()
        .map_err(|error| fail(error.to_string()))?;
    let binding = InstallationBinding::derive(&canonical, &user, ledger.binding.namespace)
        .map_err(|error| fail(format!("{error:#}")))?;
    ledger
        .validate(&binding)
        .map_err(|error| fail(format!("{error:#}")))?;
    let bytes = private_file(
        &private.join(credentials::filename(connection_id)),
        credentials::MAX_TOKEN,
    )
    .map_err(fail)?
    .ok_or_else(|| fail("HTTP client credential unavailable".into()))?;
    let token = credentials::client_token(bytes, connection_id)
        .map_err(|error| fail(format!("{error:#}")))?;
    let now: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| fail(error.to_string()))?
        .as_millis()
        .try_into()
        .map_err(|_| fail("clock out of range".into()))?;
    let authenticated = ledger
        .authenticate(&token, now)
        .map_err(|error| fail(format!("{error:#}")))?;
    if authenticated.connection_id != connection_id {
        return Err(fail("native HTTP connection identity changed".into()));
    }
    Ok(serde_json::json!({ "Authorization": format!("Bearer {token}") }))
}

/// An owner-private file within `limit` bytes, `None` when absent, with the
/// messages `airk`'s HTTPS private-record reader gives.
pub(crate) fn private_file(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, String> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(metadata) if metadata.is_file() && metadata.len() > limit as u64 => {
            return Err("HTTP private record exceeds bound".into());
        }
        Ok(_) => {}
    }
    crate::install::owned_bytes(path, limit).map(Some)
}

/// The qualified clients (U91).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Client {
    Claude,
    Codex,
    Vscode,
}

impl Client {
    pub(crate) fn executable_name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Vscode => "code",
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Vscode => "vscode",
        }
    }
}
