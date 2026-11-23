// SPDX-License-Identifier: Apache-2.0
//! `agents endpoint` (split step 3e.2): the local HTTPS endpoint, as `airk
//! runtime http-endpoint` printed it, and `scope default|defaults` (U110).
//! `config` (3e.6) classifies and edits agent MCP configuration files. The
//! connection commands (`info`, `credential`, `revoke`, `verify`, `launch`,
//! `default`) were removed with the credential (local trust plan B).
use crate::contract::{ErrorLine, ExitClass};

pub(crate) mod config;
pub(crate) mod manage;
use airk_core::service::{ControlMethod, ControlResponse};
use serde_json::Value;
use std::path::Path;

fn refused(message: &str) -> ErrorLine {
    ErrorLine::new(ExitClass::Refused, "operation_failed", false, message)
}

fn answer(response: ControlResponse) -> Result<Value, ErrorLine> {
    match response {
        ControlResponse::AgentEndpoint { endpoint, manual } => {
            let mut value =
                serde_json::to_value(endpoint).map_err(|error| refused(&error.to_string()))?;
            value["manual"] =
                serde_json::to_value(manual).map_err(|error| refused(&error.to_string()))?;
            Ok(value)
        }
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

/// U110: `{"schema":1,"entries":[...]}` from a workspace-defaults answer.
fn workspace_document(response: ControlResponse) -> Result<Value, ErrorLine> {
    match response {
        ControlResponse::WorkspaceScopeDefaults { entries } => {
            Ok(serde_json::json!({"schema": 1, "entries": entries}))
        }
        other => Err(crate::service::refusal(&other)),
    }
}

/// `scope default` (U110): resolve names by the shared rule, then set (or,
/// naming nothing, clear) the default of each workspace folder.
pub fn workspace_default(
    control: &Path,
    version: &str,
    folders: Vec<String>,
    index_names: &[String],
    set_names: &[String],
) -> Result<Value, ErrorLine> {
    for folder in &folders {
        airk_core::agent_workspace::validate_path(folder)
            .map_err(|error| refused(&format!("{error:#}: {folder}")))?;
    }
    let (mut indexes, mut sets) = (Vec::new(), Vec::new());
    if !index_names.is_empty() || !set_names.is_empty() {
        let listing = catalog_listing(control, version)?;
        for name in index_names {
            let id = airk_core::agent_scope::index_by_name(&listing, name)
                .map_err(|code| name_refused(code, name))?;
            if !indexes.contains(&id) {
                indexes.push(id);
            }
        }
        for name in set_names {
            let id = airk_core::agent_scope::set_by_name(&listing, name)
                .map_err(|code| name_refused(code, name))?;
            if !sets.contains(&id) {
                sets.push(id);
            }
        }
    }
    workspace_document(crate::service::request(
        control,
        ControlMethod::WorkspaceScopeDefault {
            folders,
            index_ids: indexes,
            index_set_ids: sets,
        },
        version,
    )?)
}

/// `scope defaults` (U110): every stored workspace default.
pub fn workspace_defaults(control: &Path, version: &str) -> Result<Value, ErrorLine> {
    workspace_document(crate::service::request(
        control,
        ControlMethod::WorkspaceScopeDefaults {},
        version,
    )?)
}

/// The qualified clients (U91).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Client {
    Claude,
    Codex,
    Vscode,
}
