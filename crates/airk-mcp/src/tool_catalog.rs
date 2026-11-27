// SPDX-License-Identifier: Apache-2.0
//! Shared native schema and selector decorators for advertised tools and admission.
use rmcp::model::Tool;
use serde_json::{Map, Value, json};
use std::sync::Arc;

pub(crate) fn native_tools() -> Vec<Tool> {
    static TOOLS: std::sync::OnceLock<Vec<Tool>> = std::sync::OnceLock::new();
    TOOLS.get_or_init(build_tools).clone()
}

// Exact bundled definitions only; per-session projects/admission/runtime state stays outside.
pub(super) fn build_tools() -> Vec<Tool> {
    let mut tools = crate::AirkServer::shared_tool_router().list_all();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

fn project_schema() -> Value {
    json!({"type":"string","pattern":"^[0-9a-f]{64}$","description":"Configured project binding. Needed only when this connection has several and no default; the projects operation lists them."})
}
/// Scope selectors an agent never needs (user decision, 27 November 2026):
/// a call reads its connection's own scope, so MCP leaves these out of the
/// schemas it advertises. The CLI catalog keeps them.
const CONNECTION_SCOPE_FIELDS: [&str; 8] = [
    "project",
    "project_ids",
    "index_ids",
    "index_set_ids",
    "workspace",
    "member_availability",
    "origin_publication",
    crate::open::CONFIGURED_SCOPE,
];

/// The schema an MCP agent sees: the tool without connection-scope selectors.
pub(crate) fn agent_view(mut tool: Tool) -> Tool {
    let schema = Arc::make_mut(&mut tool.input_schema);
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        for field in CONNECTION_SCOPE_FIELDS {
            properties.remove(field);
        }
    }
    if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|field| !CONNECTION_SCOPE_FIELDS.iter().any(|scope| field == scope));
    }
    if let Some(dependent) = schema.get_mut("dependentRequired").and_then(Value::as_object_mut) {
        dependent.retain(|field, _| !CONNECTION_SCOPE_FIELDS.contains(&field.as_str()));
        if dependent.is_empty() {
            schema.remove("dependentRequired");
        }
    }
    tool
}

pub(crate) fn scoped_tool(mut tool: Tool) -> Tool {
    let schema = Arc::make_mut(&mut tool.input_schema);
    schema.entry("properties").or_insert_with(|| json!({}))["project"] = project_schema();
    tool
}
/// U107: the per-index count selector of a multi-index read tool (one
/// that takes `limit`).
pub fn limit_per_index_schema() -> Value {
    json!({"type":"integer","minimum":1,"maximum":100,"description":"Results each selected index returns, exactly; results stay grouped by index in members. limit becomes the overall cap and defaults to the number of indexes x limit_per_index; refused (combined_limit_per_index_too_large) when that exceeds limit or 100. Byte, deadline and verification budgets stay shared evenly across indexes. Omit to split limit evenly."})
}

pub(crate) fn combined_tool(mut tool: Tool) -> Tool {
    let schema = Arc::make_mut(&mut tool.input_schema);
    let properties = schema.entry("properties").or_insert_with(|| json!({}));
    // A selected combined session cannot switch into a legacy binding.
    properties
        .as_object_mut()
        .expect("tool properties")
        .remove("project");
    properties["project_ids"] = json!({"type":"array","items":{"type":"string","pattern":"^prj_[0-9a-f]{32}$"},"minItems":1,"maxItems":32,"uniqueItems":true,"description":"Projects to read, within this connection's reviewed grant; the query_scope operation lists them. Omit to read the whole captured selection."});
    properties["index_ids"] = json!({"type":"array","items":{"type":"string","pattern":"^idx_[0-9a-f]{32}$"},"minItems":1,"maxItems":32,"uniqueItems":true,"description":"Optional explicit index subset of the selected Projects. Qualified follow-up reads require one originating index."});
    if properties.get("limit").is_some() {
        properties["limit_per_index"] = limit_per_index_schema();
    }
    properties["member_availability"] = json!({"type":"string","enum":["report","require"],"default":"report","description":"Report retains usable data and complete member failures. Require returns non-success if any selected member cannot answer; independent of saved-source verification."});
    if !matches!(
        tool.name.as_ref(),
        "help" | "airk_help" | "parse" | "airk_parse"
    ) {
        properties["origin_publication"] = json!({"type":"string","minLength":1,"maxLength":256,"description":"Exact originating member publication token. Requires one selected index; a different member publication expires the follow-up rather than substituting newer evidence."});
    }
    if properties.get("cursor").is_some() {
        properties["cursor"] = json!({"type":"string","minLength":1,"maxLength":32768,"description":"Combined continuation bound to all selected member publications, grant/scope identity, filters, representation and shared budgets. Repeat the same request inputs; any changed member expires this cursor."});
    }
    // Keep composition keywords out of tools' root schema for clients that
    // require an object-shaped catalog; the dependent selectors remain explicit.
    schema.insert(
        "dependentRequired".into(),
        json!({
            "index_ids":["project_ids"], "member_availability":["project_ids"]
        }),
    );
    tool
}

/// Project the canonical Answer payload for budget admission without changing
/// the original selector or the router's exact error behavior.
pub(crate) fn native_answer_arguments(
    arguments: &Map<String, Value>,
) -> Option<Map<String, Value>> {
    let native = native_tools()
        .into_iter()
        .find(|entry| entry.name == "airk_answer")?;
    let native_properties = native.input_schema.get("properties")?.as_object()?;
    let public = scoped_tool(native.clone());
    let combined = combined_tool(native.clone());
    let public_properties = public.input_schema.get("properties")?.as_object()?;
    let combined_properties = combined.input_schema.get("properties")?.as_object()?;
    if arguments.keys().any(|key| {
        !native_properties.contains_key(key)
            && !public_properties.contains_key(key)
            && !combined_properties.contains_key(key)
    }) {
        return None;
    }
    Some(
        arguments
            .iter()
            .filter(|(key, _)| native_properties.contains_key(*key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}
