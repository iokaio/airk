// SPDX-License-Identifier: Apache-2.0
//! U105/U106: the catalog of an open agent connection
//! (`docs/split/open-agent-access.md` §5). The connection's agent choice
//! decides the catalog: off offers the public tools with no scope selector
//! and no `list_scopes`; on adds `index_ids`/`index_set_ids` (ids or exact
//! names, at most 32 each) to every native read tool and offers
//! `list_scopes`. The service resolves and refuses the selectors; this
//! module owns only the schemas.
use airk_core::service_http_credentials::AgentChoice;
use rmcp::model::Tool;
use serde_json::{Value, json};
use std::sync::Arc;

/// The list tool's public name.
pub use airk_core::agent_scope::LIST_SCOPES;
/// The open selectors a read tool accepts with agent choice on.
pub const SELECTORS: [&str; 2] = ["index_ids", "index_set_ids"];

/// `list_scopes`: every index and index set of this installation with its
/// id, name, description and indexed paths.
pub fn list_scopes_tool() -> Tool {
    serde_json::from_value(json!({
        "name": LIST_SCOPES,
        "description": "List every index and index set this AIRK installation can read: id, name, description (stored, or generated from the name and paths), indexed source paths and catalog readiness; a set lists its member indexes. The first page also lists workspace_defaults: the search scope applied in the AIRK Explorer for each workspace folder. Pass ids or exact names as index_ids/index_set_ids on read tools. Pages follow next_cursor at one catalog revision. Never reads evidence or opens sources.",
        "inputSchema": {"type":"object","additionalProperties":false,"properties":{
            "cursor":{"type":"string","minLength":1,"maxLength":64,"description":"next_cursor of the previous page; expires when the catalog changes."}}},
        "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
    }))
    .expect("static list_scopes definition")
}

fn selector(what: &str, choice: AgentChoice) -> Value {
    let description = match choice {
        AgentChoice::On => format!(
            "{what} to read whole: each an id, exact name, or a unique case-insensitive name from list_scopes. Union of index_ids and index_set_ids; omit both to read this connection's default scope."
        ),
        // U109: narrowing only, never outside the assigned set.
        AgentChoice::Off => format!(
            "Optional: narrow this request to some {} of the assigned set; never outside it. Each is an id, exact name, or a unique case-insensitive name. Omit to read the whole assigned set.",
            what.to_lowercase()
        ),
    };
    json!({"type":"array","items":{"type":"string","minLength":1,"maxLength":256},"maxItems":32,"uniqueItems":true,
        "description":description})
}

/// Whether `name` is a native evidence read (a public alias of a native
/// tool other than `help`), which takes the open selectors.
fn native_read(name: &str, native: &[Tool]) -> bool {
    name != "help"
        && native
            .iter()
            .any(|tool| tool.name.strip_prefix("airk_") == Some(name))
}

/// U110: the `workspace` argument of the local agent session.
fn workspace_schema(list: bool) -> Value {
    let description = if list {
        "Optional absolute path of your working folder: adds workspace_default, the default scope that applies to it (or null). Filled from your MCP roots when your client provides them."
    } else {
        "Absolute path of your working folder. With no index_ids/index_set_ids, reads the search scope applied for this folder (or the nearest containing folder) in the AIRK Explorer. Filled from your MCP roots when your client provides them."
    };
    json!({"type":"string","minLength":1,"maxLength":4096,"description":description})
}

/// U110: the scope a local read must name.
fn required_selector(what: &str) -> Value {
    json!({"type":"array","items":{"type":"string","minLength":1,"maxLength":256},"maxItems":32,"uniqueItems":true,
    "description":format!(
        "{what} to read whole: each an id, exact name, or a unique case-insensitive name from list_scopes. Union of index_ids and index_set_ids. A scope is required: name indexes or index sets, or pass workspace to read the default applied for that folder."
    )})
}

/// U110: the `tools/list` of the built-in local agent session every
/// tokenless MCP session uses: the choice-on catalog whose reads require a
/// scope (`index_ids`, `index_set_ids` or `workspace`) and whose list tool
/// takes an optional `workspace`.
pub fn local_catalog() -> Vec<Tool> {
    let native = crate::local::tools();
    catalog(AgentChoice::On)
        .into_iter()
        .map(|mut tool| {
            let read = native_read(&tool.name, &native);
            let list = tool.name == LIST_SCOPES;
            let schema = Arc::make_mut(&mut tool.input_schema);
            if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
                if read {
                    properties.insert("index_ids".into(), required_selector("Indexes"));
                    properties.insert("index_set_ids".into(), required_selector("Index sets"));
                }
                if read || list {
                    properties.insert("workspace".into(), workspace_schema(list));
                }
            }
            tool
        })
        .collect()
}

/// U110: whether a local-session tool takes `workspace`.
pub fn takes_workspace(tool: &str) -> bool {
    static TOOLS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    TOOLS
        .get_or_init(|| {
            local_catalog()
                .into_iter()
                .filter(|tool| {
                    tool.input_schema
                        .get("properties")
                        .and_then(Value::as_object)
                        .is_some_and(|properties| properties.contains_key("workspace"))
                })
                .map(|tool| tool.name.to_string())
                .collect()
        })
        .iter()
        .any(|name| name == tool)
}

/// The open connection's `tools/list` for `choice`. Tools keep their public
/// definitions except the legacy `project` selector (an open connection has
/// no binding) and `projects` (replaced by `list_scopes`).
pub fn catalog(choice: AgentChoice) -> Vec<Tool> {
    let native = crate::local::tools();
    let mut tools: Vec<Tool> = crate::unified::tools()
        .into_iter()
        .filter(|tool| tool.name != "projects")
        .map(|mut tool| {
            let read = native_read(&tool.name, &native);
            let schema = Arc::make_mut(&mut tool.input_schema);
            if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
                properties.remove("project");
                // U107: an open read may span several indexes in any choice.
                if read && properties.contains_key("limit") {
                    properties.insert(
                        "limit_per_index".into(),
                        crate::combined::limit_per_index_schema(),
                    );
                }
                if read {
                    properties.insert("index_ids".into(), selector("Indexes", choice));
                    properties.insert("index_set_ids".into(), selector("Index sets", choice));
                }
            }
            tool
        })
        .collect();
    if choice == AgentChoice::On {
        tools.push(list_scopes_tool());
    }
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tools: &[Tool]) -> Vec<String> {
        tools.iter().map(|tool| tool.name.to_string()).collect()
    }
    fn properties(tool: &Tool) -> Vec<String> {
        tool.input_schema["properties"]
            .as_object()
            .map(|properties| properties.keys().cloned().collect())
            .unwrap_or_default()
    }

    #[test]
    fn choice_off_offers_narrowing_selectors_and_no_list_tool() {
        let off = catalog(AgentChoice::Off);
        assert!(!names(&off).contains(&LIST_SCOPES.to_owned()));
        assert!(!names(&off).contains(&"projects".to_owned()));
        let native = crate::local::tools();
        for tool in &off {
            let keys = properties(tool);
            assert!(!keys.contains(&"project".into()), "{}", tool.name);
            // U109: reads may narrow within the assigned set.
            let selected = SELECTORS.iter().all(|key| keys.contains(&(*key).into()));
            assert_eq!(selected, native_read(&tool.name, &native), "{}", tool.name);
        }
        let search = off.iter().find(|tool| tool.name == "search").unwrap();
        let description = search.input_schema["properties"]["index_ids"]["description"]
            .as_str()
            .unwrap();
        assert!(description.contains("of the assigned set"), "{description}");
        assert!(description.contains("never outside it"), "{description}");
        // U107: a default scope may span several indexes in either choice.
        assert!(properties(search).contains(&"limit_per_index".into()));
    }

    #[test]
    fn choice_on_adds_selectors_to_reads_and_the_list_tool() {
        let (off, on) = (catalog(AgentChoice::Off), catalog(AgentChoice::On));
        let mut expected = names(&off);
        expected.push(LIST_SCOPES.into());
        expected.sort();
        assert_eq!(names(&on), expected);
        let native = crate::local::tools();
        for tool in &on {
            let keys = properties(tool);
            let selected = SELECTORS.iter().all(|key| keys.contains(&(*key).into()));
            assert_eq!(selected, native_read(&tool.name, &native), "{}", tool.name);
        }
        let search = on.iter().find(|tool| tool.name == "search").unwrap();
        assert_eq!(
            search.input_schema["properties"]["index_ids"]["maxItems"],
            32
        );
        // Every other part of a read tool's definition is the public one.
        let public = crate::unified::tools();
        let mut stripped = search.clone();
        let schema = Arc::make_mut(&mut stripped.input_schema);
        for key in SELECTORS.into_iter().chain(["limit_per_index"]) {
            schema["properties"].as_object_mut().unwrap().remove(key);
        }
        let mut original = public
            .into_iter()
            .find(|tool| tool.name == "search")
            .unwrap();
        Arc::make_mut(&mut original.input_schema)["properties"]
            .as_object_mut()
            .unwrap()
            .remove("project");
        assert_eq!(
            serde_json::to_value(stripped).unwrap(),
            serde_json::to_value(original).unwrap()
        );
    }

    #[test]
    fn the_local_catalog_requires_a_scope_and_takes_a_workspace() {
        let local = catalog_names_with_workspace();
        // Literal expectations: the evidence reads and the list tool.
        for name in ["search", "read_file", "symbols", LIST_SCOPES] {
            assert!(local.contains(&name.to_owned()), "{name}: {local:?}");
            assert!(takes_workspace(name), "{name}");
        }
        assert!(!local.contains(&"help".to_owned()));
        assert!(!takes_workspace("help"));
        let search = local_catalog()
            .into_iter()
            .find(|tool| tool.name == "search")
            .unwrap();
        let described = |key: &str| {
            search.input_schema["properties"][key]["description"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        assert!(described("index_ids").contains("A scope is required"));
        assert!(described("workspace").contains("AIRK Explorer"));
        let list = local_catalog()
            .into_iter()
            .find(|tool| tool.name == LIST_SCOPES)
            .unwrap();
        assert!(properties(&list).contains(&"cursor".into()));
        assert!(!properties(&list).contains(&"index_ids".into()));
    }

    fn catalog_names_with_workspace() -> Vec<String> {
        local_catalog()
            .iter()
            .filter(|tool| properties(tool).contains(&"workspace".into()))
            .map(|tool| tool.name.to_string())
            .collect()
    }
}
