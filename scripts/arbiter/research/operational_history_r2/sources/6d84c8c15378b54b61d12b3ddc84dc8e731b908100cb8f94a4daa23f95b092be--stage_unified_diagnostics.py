# SPDX-License-Identifier: Apache-2.0
"""Prepare narrow public diagnostic naming fixes outside frozen source inputs."""
from pathlib import Path
import difflib

ROOT = Path(__file__).resolve().parents[2]
PATH = 'crates/airk-mcp/src/unified.rs'
source = (ROOT / PATH).read_text(encoding='utf-8')
updated = source.replace('.replace("airk_explorer_ui_status", "status.explorer")', '.replace("airk_explorer_ui_status", "diagnostics.explorer.ui")')
updated = updated.replace('json!({"type":"string","enum":["summary","steps","full"]})',
    'json!({"type":"string","enum":["summary","steps","full"],"description":"Explorer guidance detail; valid only with query or topic_id. Omit for native topic, command and tool help."})')
updated = updated.replace('''    async fn help(&self, mut args: Map<String, Value>) -> Value {
        let native_selector''', '''    async fn help(&self, mut args: Map<String, Value>) -> Value {
        if let Err(error) = crate::help::validate_detail_selector(&args) {
            return serde_json::to_value(crate::response::error_result(&error))
                .expect("typed help argument refusal");
        }
        let native_selector''')
updated = updated.replace('fn project_schema() -> Value {', '''fn replace_diagnostic_tool(text: &str, old: &str, public: &str) -> String {
    let identifier = |character: char| character.is_ascii_alphanumeric() || character == '_';
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, _) in text.match_indices(old) {
        let end = start + old.len();
        if text[..start].chars().next_back().is_some_and(identifier)
            || text[end..].chars().next().is_some_and(identifier)
        { continue; }
        result.push_str(&text[cursor..start]);
        result.push_str(public);
        cursor = end;
    }
    result.push_str(&text[cursor..]);
    result
}

fn public_diagnostic_text(text: &str) -> String {
    let mut text = text.to_owned();
    for (public, old) in UI_NAMES {
        text = replace_diagnostic_tool(&text, old, public);
    }
    for (old, public) in [
        ("airk_explorer_ui_status", "diagnostics (explorer.ui)"),
        ("airk_explorer_help_search", "help"),
        ("airk_explorer_help_get", "help"),
        ("airk_explorer_suggest_navigation", "suggest_view"),
        ("airk_explorer_suggest_review", "suggest_view"),
        ("airk_explorer_suggest_management", "suggest_view"),
        ("airk_help", "help"),
    ] {
        text = replace_diagnostic_tool(&text, old, public);
    }
    text
}

/// Rewrite only adapter-authored diagnostics. Strategy operators, template
/// bodies, shared handoff content, and evidence retain their exact bytes.
fn public_ui_response(tool: &str, mut result: Value) -> Value {
    let Some(mut value) = body(&result) else { return result; };
    if let Some(error) = value.get_mut("error").and_then(Value::as_object_mut) {
        for field in ["message", "detail"] {
            if let Some(Value::String(text)) = error.get_mut(field) {
                *text = public_diagnostic_text(text);
            }
        }
    }
    if value.get("valid") == Some(&Value::Bool(false)) {
        if let Some(errors) = value.get_mut("errors").and_then(Value::as_array_mut) {
            for error in errors {
                if let Some(Value::String(text)) = error.get_mut("message") {
                    *text = public_diagnostic_text(text);
                }
            }
        }
    }
    if matches!(tool, "airk_explorer_help_get" | "airk_explorer_help_search" | "airk_explorer_ui_status") {
        if let Some(notes) = value.get_mut("notes").and_then(Value::as_array_mut) {
            for note in notes {
                if let Value::String(text) = note { *text = public_diagnostic_text(text); }
            }
        }
    }
    // Both MCP representations must carry the same projected diagnostics.
    if result.get("structuredContent").is_some() {
        result["structuredContent"] = value.clone();
    }
    if let Some(Value::String(text)) = result.get_mut("content")
        .and_then(Value::as_array_mut).and_then(|items| items.first_mut())
        .and_then(|item| item.get_mut("text"))
    {
        *text = value.to_string();
    }
    result
}

fn project_schema() -> Value {''')
updated = updated.replace('    schema["properties"]\n        .as_object_mut()\n        .unwrap()\n        .remove("project");', '''    schema["properties"]
        .as_object_mut()
        .unwrap()
        .remove("project");
    schema["properties"]["tool"]["description"] = json!(
        "Exact public MCP tool name, for example search; returns its current description and input schema."
    );''')
old = '''        explorer.call(project, name.into(), args, client).await.unwrap_or_else(|_| failure("UI_UNAVAILABLE", "The verified Explorer adapter is unavailable. Check status and the installed extension; native retrieval remains available."))'''
new = '''        let result = explorer.call(project, name.into(), args, client).await.unwrap_or_else(|_| failure("UI_UNAVAILABLE", "The verified Explorer adapter is unavailable. Check diagnostics and the installed extension; native retrieval remains available."));
        public_ui_response(name, result)'''
if old not in updated:
    raise ValueError('UI adapter result boundary changed; review before staging')
updated = updated.replace(old, new)
tests = '''
    #[tokio::test]
    async fn native_help_detail_refuses_its_exact_field_before_native_or_ui_dispatch() {
        let (u, native, _, ui) = fixture(false, true);
        let reply = call(&u, "help", json!({"topic":"workbench","detail":"full"})).await;
        assert_eq!(reply["isError"], true);
        let value = body(&reply).unwrap();
        assert_eq!(value["error"]["code"], "invalid_help_arguments");
        assert_eq!(value["error"]["field"], "detail");
        assert!(value["error"]["message"].as_str().unwrap().contains("query or topic_id"));
        assert!(native.calls.lock().unwrap().is_empty());
        assert!(ui.calls.lock().unwrap().is_empty());
        assert_ne!(call(&u, "help", json!({"topic":"workbench","response_mode":"full"})).await["isError"], true);
        call(&u, "help", json!({"query":"workbench","detail":"full"})).await;
        assert_eq!(ui.calls.lock().unwrap().last().unwrap().2["detail"], "full");
        let schema = serde_json::to_value(&tools().into_iter().find(|tool| tool.name == "help").unwrap().input_schema).unwrap();
        assert!(schema["properties"]["detail"]["description"].as_str().unwrap().contains("query or topic_id"));
    }

    #[test]
    fn public_ui_diagnostics_use_advertised_tools_without_rewriting_user_content() {
        let original = json!({
            "valid":false,"errors":[{"field":"workspace_binding","message":"Read airk_explorer_ui_status again."}],
            "strategy":{"instructions":["Call airk_help"],"steps":[{"op":"airk.status"}]},
            "handoff":{"content":"User-authored airk_explorer_ui_status text"}
        });
        let projected = public_ui_response("airk_explorer_validate_draft", success(original.clone()));
        let value = body(&projected).unwrap();
        assert_eq!(value["errors"][0]["message"], "Read diagnostics (explorer.ui) again.");
        assert_eq!(value["strategy"], original["strategy"]);
        assert_eq!(value["handoff"], original["handoff"]);
        assert_eq!(serde_json::from_str::<Value>(projected["content"][0]["text"].as_str().unwrap()).unwrap(), projected["structuredContent"]);
        let error = body(&public_ui_response("airk_explorer_suggest_search", failure("STALE_BINDING", "Call airk_explorer_ui_status or airk_explorer_templates_list."))).unwrap();
        assert_eq!(error["error"]["message"], "Call diagnostics (explorer.ui) or list_templates.");
        assert_eq!(public_diagnostic_text("Unknown field custom_airk_help_suffix"), "Unknown field custom_airk_help_suffix");
        let help = json!({"notes":["Call airk_help for CLI guidance."],"body":"Use airk_search in the native CLI."});
        let public_help = body(&public_ui_response("airk_explorer_help_get", success(help.clone()))).unwrap();
        assert_eq!(public_help["notes"][0], "Call help for CLI guidance.");
        assert_eq!(public_help["body"], help["body"]);
        let catalog = tools();
        let help = catalog.iter().find(|tool| tool.name == "help").unwrap();
        let schema = serde_json::to_value(&help.input_schema).unwrap();
        assert!(!schema["properties"]["tool"]["description"].as_str().unwrap().contains("search or search"));
        let draft = catalog.iter().find(|tool| tool.name == "draft_search").unwrap();
        let schema = serde_json::to_value(&draft.input_schema).unwrap();
        assert!(schema["properties"]["workspace_binding"]["description"].as_str().unwrap().contains("diagnostics.explorer.ui"));
    }
'''
before, final = updated.rsplit('\n}', 1)
updated = before + '\n' + tests + '\n}' + final
changes = {PATH: (source, updated)}

HELP = 'crates/airk-mcp/src/help.rs'
source = (ROOT / HELP).read_text(encoding='utf-8')
updated = source.replace('pub fn call(arguments: Value) -> CallToolResult {', '''/// `detail` belongs to Explorer guidance, not native topic/CLI/tool selectors.
/// This validation is shared by public unified dispatch and native CLI help.
pub(crate) fn validate_detail_selector(arguments: &serde_json::Map<String, Value>) -> anyhow::Result<()> {
    if arguments.contains_key("detail") && !arguments.contains_key("query") && !arguments.contains_key("topic_id") {
        return Err(airk_core::read_arguments::ReadArgumentError::new(
            "invalid_help_arguments", "detail",
            "detail is valid only with Explorer query or topic_id help; omit it for native topic, command or tool help, which already returns full bounded content",
        ).into());
    }
    Ok(())
}
fn detail_selector_refusal(arguments: &Value) -> Option<CallToolResult> {
    let arguments = arguments.as_object()?;
    validate_detail_selector(arguments).err().map(|error| crate::response::error_result(&error))
}
pub fn call(arguments: Value) -> CallToolResult {
    if let Some(refusal) = detail_selector_refusal(&arguments) { return refusal; }''')
updated = updated.replace('pub fn call_with_catalog(arguments: Value, tools: Vec<rmcp::model::Tool>) -> CallToolResult {',
    '''pub fn call_with_catalog(arguments: Value, tools: Vec<rmcp::model::Tool>) -> CallToolResult {
    if let Some(refusal) = detail_selector_refusal(&arguments) { return refusal; }''')
changes[HELP] = (source, updated)

TOOLS_TEST = 'crates/airk-cli/tests/tools.rs'
source = (ROOT / TOOLS_TEST).read_text(encoding='utf-8')
updated = source + '''
#[test]
fn native_help_cli_reports_detail_selector_field_without_opening_a_workspace() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("missing root");
    let state = base.path().join("missing state");
    let args = json!({"topic":"workbench","detail":"full"});
    let output = command(&root, &state).args(["tools", "call", "airk_help", "--args", &args.to_string()]).output().unwrap();
    assert!(!output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result, serde_json::to_value(airk_mcp::help::call(args)).unwrap());
    let value: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["error"]["code"], "invalid_help_arguments");
    assert_eq!(value["error"]["field"], "detail");
    assert!(value["error"]["message"].as_str().unwrap().contains("query or topic_id"));
    assert!(!root.exists() && !state.exists());
}
'''
changes[TOOLS_TEST] = (source, updated)

MCP_TEST = 'crates/airk-cli/tests/mcp.rs'
source = (ROOT / MCP_TEST).read_text(encoding='utf-8')
updated = source.replace('    let mut values: Vec<Value> = Vec::new();', '''    // Exercise unified stdio help without sending Explorer-only detail into the
    // native selector; the canonical CLI helper retains the identical refusal.
    let invalid_help = json!({"topic":"workbench","detail":"full"});
    let refused = session.call(200, "help", invalid_help.clone())?;
    assert_eq!(refused["result"], serde_json::to_value(airk_mcp::help::call(invalid_help))?);
    assert_eq!(refused["result"]["isError"], true);
    let valid_help = session.call(201, "help", json!({"topic":"workbench","response_mode":"full"}))?;
    assert_ne!(valid_help["result"]["isError"], true);
    let mut values: Vec<Value> = Vec::new();''')
if updated == source:
    raise ValueError('Actual MCP help regression insertion point changed')
changes[MCP_TEST] = (source, updated)

patch = ''.join(f'diff --git a/{path} b/{path}\n' + ''.join(difflib.unified_diff(
    before.splitlines(keepends=True), after.splitlines(keepends=True), fromfile=f'a/{path}', tofile=f'b/{path}'))
    for path, (before, after) in changes.items())
destination = Path(__file__).with_name('unified-diagnostic-names.patch')
destination.write_text(patch, encoding='utf-8')
print(destination)
