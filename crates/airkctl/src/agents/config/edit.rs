// SPDX-License-Identifier: Apache-2.0
//! Pure edits of recognized AIRK entries (U110): `connect` inserts the HTTPS
//! entry with only the endpoint URL and the `X-AIRK-Response-Mode` header;
//! `repair` points a recognized HTTPS entry at the live endpoint and strips
//! its legacy credential fields (`Authorization`, `headersHelper`,
//! `http_headers_helper`, `bearer_token_env_var`) and, for VS Code, the
//! credential input nothing references any more; `remove` deletes a
//! recognized entry and, for VS Code, that input (agentConfig.ts
//! `syncVscodeHttpInput`). Anything AIRK did not provably write is refused, so
//! other servers, comments, disabled states and tool/approval policies are
//! never rewritten.
use super::jsonc::{self, Kind, Out};
use super::recognize::{self, Declaration, MAX_BYTES, Parser, Recognized};
use super::{Agent, Scope, toml};
use airk_core::identity::Incarnation;
use serde_json::Value;
use std::collections::HashSet;

const MODE: &str = "X-AIRK-Response-Mode";
/// The VS Code command the extension registered to resolve legacy inputs.
const VSCODE_COMMAND: &str = "airk.httpCredential";

/// One server declaration in one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) container_path: Vec<String>,
    pub(crate) server_key: String,
}

fn input_id(id: Incarnation) -> String {
    format!("airkToken_{id}")
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

/// The legacy credential input the extension wrote for connection `id`.
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

fn checked(url: &str) -> Result<(), String> {
    if recognize::http_url(url) {
        Ok(())
    } else {
        Err(format!("invalid AIRK HTTPS endpoint {url:?}"))
    }
}

/// The U110 JSON server fields: the endpoint URL and the response-mode
/// header, no `Authorization` and no `headersHelper`.
fn json_fields(agent: Agent, url: &str) -> Result<Out, String> {
    match agent {
        Agent::Claude | Agent::Copilot => Ok(object(vec![
            ("type", text("http")),
            ("url", text(url)),
            ("headers", object(vec![(MODE, text("terse"))])),
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

/// Whether the Codex entry at `target` asks for less than
/// `CODEX_TOOL_TIMEOUT_SEC` (or nothing, Codex's 60 s default), so `agents
/// repair` raises it even when the entry is otherwise healthy.
pub(crate) fn codex_timeout_short(agent: Agent, source: &str, target: &Target) -> bool {
    if agent.parser() != Parser::Codex {
        return false;
    }
    let Ok(rows) = recognize::declarations(agent.parser(), source) else {
        return false;
    };
    find(&rows, target).is_some_and(|row| {
        row.fields
            .get("tool_timeout_sec")
            .and_then(Value::as_f64)
            .is_none_or(|seconds| seconds < super::CODEX_TOOL_TIMEOUT_SEC as f64)
    })
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

/// Whether the declaration's `table` already names the response mode.
fn has_mode(row: &Declaration, table: &str) -> bool {
    row.fields
        .get(table)
        .and_then(Value::as_object)
        .is_some_and(|headers| headers.keys().any(|key| key.eq_ignore_ascii_case(MODE)))
}

/// Write the HTTPS entry `server_key` for the endpoint `url` into the
/// agent's top-level container. `source` is `None` for an absent file. An
/// existing recognized entry is repaired in place; any other entry with that
/// key is refused.
pub(crate) fn connect(
    agent: Agent,
    scope: Scope,
    source: Option<&str>,
    server_key: &str,
    url: &str,
) -> Result<String, String> {
    if !agent.connectable() {
        return Err(unverified(agent));
    }
    if !recognize::airk_key(server_key) {
        return Err(format!(
            "AIRK server keys start with airk; {server_key:?} is not one."
        ));
    }
    checked(url)?;
    let target = Target {
        container_path: vec![agent.container().to_string()],
        server_key: server_key.to_string(),
    };
    let empty = if agent == Agent::Codex { "" } else { "{}\n" };
    let source = source.unwrap_or(empty);
    let rows = recognize::declarations(agent.parser(), source)?;
    if find(&rows, &target).is_some() {
        return repair(agent, scope, source, &target, url);
    }
    let result = if agent == Agent::Codex {
        toml::append_server(source, server_key, url)
    } else {
        let fields = json_fields(agent, url)?;
        jsonc::insert_member(source, &target.container_path, server_key, &fields)?
    };
    bounded(result)
}

/// Point a recognized HTTPS entry at the endpoint `url` and strip its legacy
/// credential fields (U110); add the response-mode header when the entry has
/// none. Other fields stay.
pub(crate) fn repair(
    agent: Agent,
    scope: Scope,
    source: &str,
    target: &Target,
    url: &str,
) -> Result<String, String> {
    if !agent.connectable() {
        return Err(unverified(agent));
    }
    checked(url)?;
    let (row, recognized) = owned(agent, scope, source, target)?;
    let Recognized::Https { id, stale, .. } = recognized else {
        return Err(
            "Retired stdio AIRK entries are not repaired; remove the entry and connect again."
                .into(),
        );
    };
    let result = if agent.parser() == Parser::Codex {
        if target.container_path != ["mcp_servers"] {
            return Err("AIRK edits Codex servers only in the top-level mcp_servers table.".into());
        }
        let key = &target.server_key;
        let mut result = toml::set_string(source, key, None, "url", url)?;
        for field in &stale {
            result = match field.as_slice() {
                [name] => toml::remove_field(&result, key, None, name)?,
                [table, name] => toml::remove_field(&result, key, Some(table.as_str()), name)?,
                _ => return Err("Unexpected AIRK credential field; review it manually.".into()),
            };
        }
        // Codex cuts a tool call off after 60 s by default; Answer may run to
        // its 605 s transport ceiling. A longer setting the user chose stays.
        let timeout = row.fields.get("tool_timeout_sec").and_then(Value::as_f64);
        if timeout.is_none_or(|seconds| seconds < super::CODEX_TOOL_TIMEOUT_SEC as f64) {
            result = toml::set_integer(
                &result,
                key,
                "tool_timeout_sec",
                super::CODEX_TOOL_TIMEOUT_SEC,
            )?;
        }
        if !has_mode(&row, "http_headers") {
            result = toml::set_string(&result, key, Some("http_headers"), MODE, "terse")?;
        }
        result
    } else {
        let mut server = target.container_path.clone();
        server.push(target.server_key.clone());
        let at = |fields: &[&str]| -> Vec<String> {
            server
                .iter()
                .cloned()
                .chain(fields.iter().map(|f| f.to_string()))
                .collect()
        };
        let mut result = jsonc::set_string(source, &at(&["url"]), url)?;
        for field in &stale {
            let (name, parent) = field
                .split_last()
                .ok_or("Unexpected AIRK credential field; review it manually.")?;
            let parent: Vec<String> = server.iter().chain(parent).cloned().collect();
            result = jsonc::remove_member(&result, &parent, name)?;
        }
        if !has_mode(&row, "headers") {
            result = jsonc::set_string(&result, &at(&["headers", MODE]), "terse")?;
        }
        match id {
            Some(id) if agent == Agent::Copilot => drop_input(&result, id)?,
            _ => result,
        }
    };
    bounded(result)
}

/// Delete a recognized entry (HTTPS or retired stdio) and, for VS Code, its
/// legacy credential input once nothing references it.
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
        Recognized::Https { id: Some(id), .. } if agent == Agent::Copilot => {
            drop_input(&result, id)
        }
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

/// `syncCredentialInput` for removal: drop the legacy input `airkToken_<id>`
/// once nothing references it. An ambiguous or conflicting `inputs` array is
/// refused.
fn drop_input(source: &str, id: Incarnation) -> Result<String, String> {
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
    match existing {
        Some((index, _)) if count == 0 => jsonc::remove_element(source, &inputs_path, index),
        _ => Ok(source.to_string()),
    }
}
