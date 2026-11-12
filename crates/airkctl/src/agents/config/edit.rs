// SPDX-License-Identifier: Apache-2.0
//! Pure edits of recognized AIRK entries: `connect` inserts the HTTPS entry
//! `httpServerFields` (model.ts) renders, with the U92 helper; `repair`
//! rewrites the URL, helper and credential reference of a recognized HTTPS
//! entry; `remove` deletes a recognized entry and, for VS Code, its
//! credential input (agentConfig.ts `syncVscodeHttpInput`). Anything AIRK
//! did not provably write is refused, so other servers, comments, disabled
//! states and tool/approval policies are never rewritten.
use super::jsonc::{self, Kind, Out};
use super::recognize::{self, Declaration, MAX_BYTES, Parser, Recognized};
use super::{Agent, Scope, toml};
use airk_core::identity::Incarnation;
use serde_json::Value;
use std::collections::HashSet;

const MODE: &str = "X-AIRK-Response-Mode";
/// The VS Code command the extension registers to resolve the input.
const VSCODE_COMMAND: &str = "airk.httpCredential";

/// The connection an entry should name.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Connection<'a> {
    pub(crate) id: Incarnation,
    /// The live endpoint, `https://127.0.0.1:<port>/mcp`.
    pub(crate) url: &'a str,
    /// The installed `airkctl` the helper names.
    pub(crate) airkctl: &'a str,
}

/// One server declaration in one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) container_path: Vec<String>,
    pub(crate) server_key: String,
}

fn env_header(id: Incarnation) -> String {
    format!(
        "Bearer ${{AIRK_MCP_TOKEN_{}}}",
        id.to_string().to_ascii_uppercase()
    )
}

fn input_id(id: Incarnation) -> String {
    format!("airkToken_{id}")
}

fn input_header(id: Incarnation) -> String {
    format!("Bearer ${{input:{}}}", input_id(id))
}

fn text(value: &str) -> Out {
    Out::Str(value.to_string())
}

fn object(fields: Vec<(&str, Out)>) -> Out {
    Out::Obj(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

fn input(id: Incarnation) -> Out {
    object(vec![
        ("id", Out::Str(input_id(id))),
        ("type", text("command")),
        ("command", text(VSCODE_COMMAND)),
        (
            "args",
            object(vec![("connection_id", Out::Str(id.to_string()))]),
        ),
    ])
}

fn unverified(agent: Agent) -> String {
    format!("http_client_compatibility_unverified: {}", agent.name())
}

fn checked(connection: &Connection<'_>) -> Result<(), String> {
    if recognize::http_url(connection.url) {
        Ok(())
    } else {
        Err(format!("invalid AIRK HTTPS endpoint {:?}", connection.url))
    }
}

/// The JSON server fields of `httpServerFields`, helper per U92.
fn json_fields(agent: Agent, connection: &Connection<'_>) -> Result<Out, String> {
    let url = text(connection.url);
    match agent {
        Agent::Claude => Ok(object(vec![
            ("type", text("http")),
            ("url", url),
            (
                "headers",
                object(vec![
                    ("Authorization", Out::Str(env_header(connection.id))),
                    (MODE, text("terse")),
                ]),
            ),
            (
                "headersHelper",
                Out::Str(recognize::helper_command(
                    connection.airkctl,
                    connection.id,
                )?),
            ),
        ])),
        Agent::Copilot => Ok(object(vec![
            ("type", text("http")),
            ("url", url),
            (
                "headers",
                object(vec![
                    ("Authorization", Out::Str(input_header(connection.id))),
                    (MODE, text("terse")),
                ]),
            ),
        ])),
        other => Err(unverified(other)),
    }
}

fn bounded(result: String) -> Result<String, String> {
    if result.len() > MAX_BYTES {
        return Err("Updated configuration would exceed 1 MiB.".into());
    }
    Ok(result)
}

fn find<'a>(rows: &'a [Declaration], target: &Target) -> Option<&'a Declaration> {
    rows.iter()
        .find(|row| row.container == target.container_path && row.server_key == target.server_key)
}

/// The recognized declaration at `target`, or a refusal.
fn owned(
    agent: Agent,
    scope: Scope,
    source: &str,
    target: &Target,
) -> Result<(Declaration, Recognized), String> {
    let rows = recognize::declarations(agent.parser(), source)?;
    let key = &target.server_key;
    let row = find(&rows, target)
        .ok_or_else(|| format!("No server {key} in {}.", target.container_path.join(".")))?;
    if scope == Scope::Project && row.container != [agent.container()] {
        return Err(format!(
            "AIRK edits project servers only under {}.",
            agent.container()
        ));
    }
    let recognized = recognize::recognize(agent, scope, row, &[], false).ok_or_else(|| {
        format!(
            "The server {key} is not an entry AIRK wrote; nothing was changed. Review it manually."
        )
    })?;
    Ok((row.clone(), recognized))
}

/// Write the HTTPS entry `server_key` into the agent's top-level container.
/// `source` is `None` for an absent file. An existing recognized entry is
/// repaired in place; any other entry with that key is refused.
pub(crate) fn connect(
    agent: Agent,
    scope: Scope,
    source: Option<&str>,
    server_key: &str,
    connection: &Connection<'_>,
) -> Result<String, String> {
    if !agent.connectable() {
        return Err(unverified(agent));
    }
    if !recognize::airk_key(server_key) {
        return Err(format!(
            "AIRK server keys start with airk; {server_key:?} is not one."
        ));
    }
    checked(connection)?;
    let target = Target {
        container_path: vec![agent.container().to_string()],
        server_key: server_key.to_string(),
    };
    let empty = if agent == Agent::Codex { "" } else { "{}\n" };
    let source = source.unwrap_or(empty);
    let rows = recognize::declarations(agent.parser(), source)?;
    if find(&rows, &target).is_some() {
        return repair(agent, scope, source, &target, connection);
    }
    let result = if agent == Agent::Codex {
        let helper = recognize::helper_command(connection.airkctl, connection.id)?;
        toml::append_server(source, server_key, connection.url, &helper)
    } else {
        let fields = json_fields(agent, connection)?;
        let result = jsonc::insert_member(source, &target.container_path, server_key, &fields)?;
        if agent == Agent::Copilot {
            sync_input(&result, connection.id, false)?
        } else {
            result
        }
    };
    bounded(result)
}

/// Point a recognized HTTPS entry at `connection`: its URL, its helper
/// (Claude Code, Codex) and its credential reference. Other fields stay.
pub(crate) fn repair(
    agent: Agent,
    scope: Scope,
    source: &str,
    target: &Target,
    connection: &Connection<'_>,
) -> Result<String, String> {
    if !agent.connectable() {
        return Err(unverified(agent));
    }
    checked(connection)?;
    let (row, recognized) = owned(agent, scope, source, target)?;
    let Recognized::Https { id: previous, .. } = recognized else {
        return Err(
            "Retired stdio AIRK entries are not repaired; remove the entry and connect again."
                .into(),
        );
    };
    let mut path = target.container_path.clone();
    path.push(target.server_key.clone());
    let at = |fields: &[&str]| -> Vec<String> {
        path.iter()
            .cloned()
            .chain(fields.iter().map(|f| f.to_string()))
            .collect()
    };
    let result = match agent {
        Agent::Claude => {
            let helper = recognize::helper_command(connection.airkctl, connection.id)?;
            let mut result = jsonc::set_string(source, &at(&["url"]), connection.url)?;
            result = jsonc::set_string(&result, &at(&["headersHelper"]), &helper)?;
            let authorization = row
                .fields
                .get("headers")
                .and_then(|headers| headers.get("Authorization"));
            if authorization.is_some() {
                result = jsonc::set_string(
                    &result,
                    &at(&["headers", "Authorization"]),
                    &env_header(connection.id),
                )?;
            }
            result
        }
        Agent::Copilot => {
            let mut result = jsonc::set_string(source, &at(&["url"]), connection.url)?;
            result = jsonc::set_string(
                &result,
                &at(&["headers", "Authorization"]),
                &input_header(connection.id),
            )?;
            if previous != connection.id {
                result = sync_input(&result, previous, true)?;
            }
            sync_input(&result, connection.id, false)?
        }
        Agent::Codex => {
            if target.container_path != ["mcp_servers"] {
                return Err(
                    "AIRK edits Codex servers only in the top-level mcp_servers table.".into(),
                );
            }
            let helper = recognize::helper_command(connection.airkctl, connection.id)?;
            let result = toml::set_string(source, &target.server_key, "url", connection.url)?;
            toml::set_string(&result, &target.server_key, "http_headers_helper", &helper)?
        }
        other => return Err(unverified(other)),
    };
    bounded(result)
}

/// Delete a recognized entry (HTTPS or retired stdio) and, for VS Code, its
/// credential input once nothing references it.
pub(crate) fn remove(
    agent: Agent,
    scope: Scope,
    source: &str,
    target: &Target,
) -> Result<String, String> {
    let (_, recognized) = owned(agent, scope, source, target)?;
    if agent.parser() == Parser::Codex {
        if target.container_path != ["mcp_servers"] {
            return Err("AIRK edits Codex servers only in the top-level mcp_servers table.".into());
        }
        return toml::remove_server(source, &target.server_key);
    }
    let result = jsonc::remove_member(source, &target.container_path, &target.server_key)?;
    match recognized {
        Recognized::Https { id, .. } if agent == Agent::Copilot => sync_input(&result, id, true),
        _ => Ok(result),
    }
}

fn references(value: &Value, marker: &str) -> usize {
    match value {
        Value::String(text) => text.matches(marker).count(),
        Value::Array(items) => items.iter().map(|item| references(item, marker)).sum(),
        Value::Object(map) => map
            .iter()
            .map(|(key, item)| key.matches(marker).count() + references(item, marker))
            .sum(),
        _ => 0,
    }
}

/// `syncCredentialInput`: add the connection's input when exactly one
/// Authorization field references it, or remove it once none does. An
/// ambiguous or conflicting `inputs` array is refused.
fn sync_input(source: &str, id: Incarnation, remove: bool) -> Result<String, String> {
    let tree = jsonc::parse(source)?;
    let inputs_path = ["inputs".to_string()];
    let items = match tree.at(&inputs_path) {
        None => &[][..],
        Some(node) => match &node.kind {
            Kind::Array(items) => &items[..],
            _ => {
                return Err(
                    "VS Code inputs must be an array; existing configuration preserved.".into(),
                );
            }
        },
    };
    let wanted_id = input_id(id);
    let mut seen = HashSet::new();
    let mut existing = None;
    for (index, item) in items.iter().enumerate() {
        let value = item.value();
        let Some(name) = value
            .get("id")
            .and_then(Value::as_str)
            .filter(|_| value.is_object())
        else {
            return Err("Ambiguous VS Code inputs; existing configuration preserved.".into());
        };
        if !seen.insert(name.to_string()) {
            return Err("Ambiguous VS Code inputs; existing configuration preserved.".into());
        }
        if name == wanted_id {
            existing = Some((index, value.clone()));
        }
    }
    if let Some((_, value)) = &existing
        && *value != input(id).to_value()
    {
        return Err("Conflicting AIRK credential input; existing configuration preserved.".into());
    }
    let count = references(&tree.value(), &format!("${{input:{wanted_id}}}"));
    if remove {
        return match existing {
            Some((index, _)) if count == 0 => jsonc::remove_element(source, &inputs_path, index),
            _ => Ok(source.to_string()),
        };
    }
    if count != 1 {
        return Err(
            "AIRK credential input must belong to exactly one HTTP Authorization field.".into(),
        );
    }
    if existing.is_some() {
        return Ok(source.to_string());
    }
    jsonc::append_element(source, &inputs_path, &input(id))
}
