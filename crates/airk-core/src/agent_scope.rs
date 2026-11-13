// SPDX-License-Identifier: Apache-2.0
//! U105/U106: the scope of an open agent read and the `list_indexes` body,
//! decided purely over catalog observations (`docs/split/open-agent-access.md`
//! §5-§6, contracts §2.9). The service gathers a [`CatalogListing`] at one
//! catalog revision; these functions never read storage, a clock or the
//! credential ledger.
//!
//! Rules: a call naming nothing reads the connection's stored default scope;
//! with agent choice off a call naming anything is refused; a name is an
//! `idx_`/`prj_` id, else an exact name, and an ambiguous or unknown name is
//! refused; an index set expands to its live member indexes, each read whole
//! (U106(a)); the union is deduplicated and bounded to
//! [`MAX_SELECTION`] indexes.
use crate::{
    catalog_management::CatalogSource,
    identity::Incarnation,
    index_catalog::{
        CatalogRevision, IndexDefinition, IndexId, MAX_DESCRIPTION_BYTES, MAX_SELECTION,
        Membership, QueryProject, QueryProjectId, SourceImportState,
    },
    service_http::open_refusal,
    service_http_credentials::{AgentChoice, OpenConnectionState},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// The open list tool's public name (served by the service, offered in an
/// open connection's catalog only with agent choice on).
pub const LIST_INDEXES: &str = "list_indexes";

/// The refusal of a scope that resolved to no index (an empty index set).
pub const SCOPE_UNAVAILABLE: &str = "scoped_read_unavailable";

/// The catalog at one revision, as the service read it. `sources` may be
/// empty when only resolution (not `list_indexes`) needs the listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogListing {
    pub namespace: Incarnation,
    pub revision: Option<CatalogRevision>,
    pub indexes: Vec<IndexDefinition>,
    pub projects: Vec<QueryProject>,
    pub memberships: Vec<Membership>,
    pub sources: Vec<CatalogSource>,
}

/// What one call asks to read before names are resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requested<'a> {
    /// Ids or exact names the call named.
    Named {
        indexes: &'a [String],
        sets: &'a [String],
    },
    /// The connection's stored default scope.
    Defaults {
        indexes: &'a [IndexId],
        sets: &'a [QueryProjectId],
    },
}

/// The choice rules of one read. `connection` is the open connection's
/// stored state; `None` is the local owner (`airkctl tools call --index`
/// without `--connection`), who has no default and no choice rule.
pub fn requested<'a>(
    index_names: &'a [String],
    set_names: &'a [String],
    connection: Option<&'a OpenConnectionState>,
) -> Result<Requested<'a>, &'static str> {
    let named = !index_names.is_empty() || !set_names.is_empty();
    match connection {
        _ if named && connection.is_some_and(|open| open.agent_choice == AgentChoice::Off) => {
            Err(open_refusal::AGENT_SCOPE_CHOICE_DISABLED)
        }
        _ if named => Ok(Requested::Named {
            indexes: index_names,
            sets: set_names,
        }),
        Some(open)
            if !open.default_index_ids.is_empty() || !open.default_index_set_ids.is_empty() =>
        {
            Ok(Requested::Defaults {
                indexes: &open.default_index_ids,
                sets: &open.default_index_set_ids,
            })
        }
        _ => Err(open_refusal::AGENT_DEFAULT_SCOPE_UNSET),
    }
}

/// Whether `list_indexes` is served: always for the local owner, only with
/// agent choice on for a connection.
pub fn list_allowed(connection: Option<&OpenConnectionState>) -> Result<(), &'static str> {
    match connection {
        Some(open) if open.agent_choice == AgentChoice::Off => {
            Err(open_refusal::AGENT_SCOPE_CHOICE_DISABLED)
        }
        _ => Ok(()),
    }
}

fn live_index<'a>(listing: &'a CatalogListing, id: &IndexId) -> Option<&'a IndexDefinition> {
    listing
        .indexes
        .iter()
        .find(|index| index.id == *id && !index.tombstone)
}

/// One index name: an `idx_` id of a live index, else the exact name of
/// exactly one live index.
pub fn index_by_name(listing: &CatalogListing, name: &str) -> Result<IndexId, &'static str> {
    if let Ok(id) = name.parse::<IndexId>() {
        return live_index(listing, &id)
            .map(|index| index.id)
            .ok_or(open_refusal::SCOPE_NAME_UNKNOWN);
    }
    let mut found = listing
        .indexes
        .iter()
        .filter(|index| !index.tombstone && index.name == name);
    match (found.next(), found.next()) {
        (Some(index), None) => Ok(index.id),
        (Some(_), Some(_)) => Err(open_refusal::SCOPE_NAME_AMBIGUOUS),
        (None, _) => Err(open_refusal::SCOPE_NAME_UNKNOWN),
    }
}

/// One index set name: a `prj_` id of a present set, else the exact name of
/// exactly one set.
pub fn set_by_name(listing: &CatalogListing, name: &str) -> Result<QueryProjectId, &'static str> {
    if let Ok(id) = name.parse::<QueryProjectId>() {
        return listing
            .projects
            .iter()
            .find(|project| project.id == id)
            .map(|project| project.id)
            .ok_or(open_refusal::SCOPE_NAME_UNKNOWN);
    }
    let mut found = listing
        .projects
        .iter()
        .filter(|project| project.name == name);
    match (found.next(), found.next()) {
        (Some(project), None) => Ok(project.id),
        (Some(_), Some(_)) => Err(open_refusal::SCOPE_NAME_AMBIGUOUS),
        (None, _) => Err(open_refusal::SCOPE_NAME_UNKNOWN),
    }
}

/// The live member indexes of a set, in id order.
fn members(listing: &CatalogListing, set: &QueryProjectId) -> BTreeSet<IndexId> {
    listing
        .memberships
        .iter()
        .filter(|membership| membership.project_id == *set)
        .filter(|membership| live_index(listing, &membership.index_id).is_some())
        .map(|membership| membership.index_id)
        .collect()
}

/// Resolve a request to the sorted, deduplicated indexes it reads whole.
pub fn resolve(
    listing: &CatalogListing,
    requested: Requested<'_>,
) -> Result<Vec<IndexId>, &'static str> {
    let mut indexes = BTreeSet::new();
    let mut sets = Vec::new();
    match requested {
        Requested::Named {
            indexes: names,
            sets: set_names,
        } => {
            for name in names {
                indexes.insert(index_by_name(listing, name)?);
            }
            for name in set_names {
                sets.push(set_by_name(listing, name)?);
            }
        }
        Requested::Defaults {
            indexes: ids,
            sets: set_ids,
        } => {
            for id in ids {
                indexes.insert(
                    live_index(listing, id)
                        .map(|index| index.id)
                        .ok_or(open_refusal::SCOPE_NAME_UNKNOWN)?,
                );
            }
            for id in set_ids {
                if !listing.projects.iter().any(|project| project.id == *id) {
                    return Err(open_refusal::SCOPE_NAME_UNKNOWN);
                }
                sets.push(*id);
            }
        }
    }
    for set in &sets {
        indexes.extend(members(listing, set));
    }
    if indexes.is_empty() {
        return Err(SCOPE_UNAVAILABLE);
    }
    if indexes.len() > MAX_SELECTION {
        return Err(open_refusal::AGENT_SCOPE_TOO_LARGE);
    }
    Ok(indexes.into_iter().collect())
}

/// Check stored defaults against the catalog: every index live, every set
/// present, each list unique and bounded. Sets are not expanded here, so a
/// set may later grow past the read bound and refuse at read time.
pub fn validate_defaults(
    listing: &CatalogListing,
    indexes: &[IndexId],
    sets: &[QueryProjectId],
) -> Result<(), &'static str> {
    crate::index_catalog::unique_bounded(indexes, MAX_SELECTION)
        .map_err(|_| open_refusal::AGENT_SCOPE_TOO_LARGE)?;
    crate::index_catalog::unique_bounded(sets, MAX_SELECTION)
        .map_err(|_| open_refusal::AGENT_SCOPE_TOO_LARGE)?;
    for id in indexes {
        live_index(listing, id).ok_or(open_refusal::SCOPE_NAME_UNKNOWN)?;
    }
    for id in sets {
        if !listing.projects.iter().any(|project| project.id == *id) {
            return Err(open_refusal::SCOPE_NAME_UNKNOWN);
        }
    }
    Ok(())
}

/// The catalog readiness `list_indexes` reports for an index: its source
/// import and enabled sources only. It is not publication, policy-removal or
/// semantic readiness (`status` reports those); an index without a saved
/// policy reads through the metadata-only path and is not pending.
fn readiness(index: &IndexDefinition, paths: &[&str]) -> &'static str {
    if index.source_import != SourceImportState::Complete {
        "import_pending"
    } else if paths.is_empty() {
        "no_enabled_source"
    } else {
        "ready"
    }
}

/// `text` bounded to [`MAX_DESCRIPTION_BYTES`] at a character boundary.
fn bounded(text: String) -> String {
    if text.len() <= MAX_DESCRIPTION_BYTES {
        return text;
    }
    let mut end = MAX_DESCRIPTION_BYTES - '…'.len_utf8();
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

fn index_entry(listing: &CatalogListing, index: &IndexDefinition) -> Value {
    let paths: Vec<&str> = listing
        .sources
        .iter()
        .filter(|source| source.index_id == index.id && source.enabled)
        .map(|source| source.canonical_path.as_str())
        .collect();
    let (description, generated) = match &index.description {
        Some(text) => (text.clone(), false),
        None if paths.is_empty() => (format!("{}: indexes no enabled source", index.name), true),
        None => (
            bounded(format!("{}: indexes {}", index.name, paths.join(", "))),
            true,
        ),
    };
    json!({
        "id": index.id,
        "name": index.name,
        "description": description,
        "description_generated": generated,
        "paths": paths,
        "readiness": readiness(index, &paths),
    })
}

fn set_entry(listing: &CatalogListing, project: &QueryProject) -> Value {
    let ids: Vec<IndexId> = members(listing, &project.id).into_iter().collect();
    let names: Vec<&str> = ids
        .iter()
        .filter_map(|id| live_index(listing, id).map(|index| index.name.as_str()))
        .collect();
    let (description, generated) = match &project.description {
        Some(text) => (text.clone(), false),
        None if names.is_empty() => (format!("{}: empty index set", project.name), true),
        None => (
            bounded(format!(
                "{}: index set of {}",
                project.name,
                names.join(", ")
            )),
            true,
        ),
    };
    json!({
        "id": project.id,
        "name": project.name,
        "description": description,
        "description_generated": generated,
        "index_ids": ids,
    })
}

const CURSOR_PREFIX: &str = "lix1";

fn cursor(revision: CatalogRevision, offset: usize) -> String {
    format!("{CURSOR_PREFIX}.{}.{offset}", revision.get())
}

/// The entry offset a cursor names, refused unless it was issued at
/// `revision`.
fn resume(revision: CatalogRevision, text: &str) -> Result<usize, &'static str> {
    let mut parts = text.split('.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(CURSOR_PREFIX), Some(at), Some(offset), None) => {
            if at.parse::<u64>() != Ok(revision.get()) {
                return Err(open_refusal::LIST_CURSOR_EXPIRED);
            }
            offset
                .parse::<usize>()
                .map_err(|_| open_refusal::LIST_CURSOR_EXPIRED)
        }
        _ => Err(open_refusal::LIST_CURSOR_EXPIRED),
    }
}

/// One `list_indexes` page: every live index (id order), then every index
/// set (id order), from the cursor's offset while the encoded body stays
/// within `max_bytes`. A page always carries at least one remaining entry;
/// `next_cursor` is present while entries remain and is bound to the
/// listing's revision.
pub fn list_page(
    listing: &CatalogListing,
    after: Option<&str>,
    max_bytes: usize,
) -> Result<Value, &'static str> {
    let revision = listing.revision.ok_or(SCOPE_UNAVAILABLE)?;
    let start = match after {
        Some(text) => resume(revision, text)?,
        None => 0,
    };
    let mut live: Vec<&IndexDefinition> = listing
        .indexes
        .iter()
        .filter(|index| !index.tombstone)
        .collect();
    live.sort_by_key(|index| index.id);
    let mut sets: Vec<&QueryProject> = listing.projects.iter().collect();
    sets.sort_by_key(|project| project.id);
    let total = live.len() + sets.len();
    if start > total {
        return Err(open_refusal::LIST_CURSOR_EXPIRED);
    }
    let mut body = json!({
        "schema": 1,
        "namespace": listing.namespace,
        "catalog_revision": revision.get(),
        "indexes": [],
        "index_sets": [],
    });
    let mut next = start;
    while next < total {
        let (field, entry) = match live.get(next) {
            Some(index) => ("indexes", index_entry(listing, index)),
            None => ("index_sets", set_entry(listing, sets[next - live.len()])),
        };
        body[field].as_array_mut().expect("entry list").push(entry);
        let mut probe = body.clone();
        if next + 1 < total {
            probe["next_cursor"] = json!(cursor(revision, next + 1));
        }
        let size = serde_json::to_vec(&probe).map_or(usize::MAX, |bytes| bytes.len());
        if size > max_bytes && next > start {
            body[field].as_array_mut().expect("entry list").pop();
            break;
        }
        next += 1;
    }
    if next < total {
        body["next_cursor"] = json!(cursor(revision, next));
    }
    Ok(body)
}

#[cfg(test)]
mod tests;
