// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn list_text_names_each_tool_then_the_guidance() {
    let document = json!({"tools": [
        {"name": "status", "description": "Quick check."},
        {"name": "bare"}
    ]});
    assert_eq!(
        list_text(&document).unwrap(),
        "status\n  Quick check.\nbare\n  \n\nUse: airkctl tools help <tool>\nOr in the tools console: help <tool>\n"
    );
    assert_eq!(list_text(&json!({"tools": [{"description": "x"}]})), None);
    assert_eq!(list_text(&json!({})), None);
}

#[test]
fn the_template_uses_defaults_then_typed_placeholders_for_required_fields() {
    let schema = json!({
        "type": "object",
        "required": ["a", "b", "c", "d", "e", "f", "g"],
        "properties": {
            "a": {"type": "integer"},
            "b": {"type": "number"},
            "c": {"type": "boolean"},
            "d": {"type": "array"},
            "e": {"type": "object"},
            "f": {"type": "string"},
            "g": {"type": "integer", "default": 7},
            "optional": {"type": "string"},
            "optional_default": {"type": "string", "default": "x"}
        }
    });
    assert_eq!(
        Value::Object(template(&schema)),
        json!({"a": 1, "b": 1, "c": false, "d": [], "e": {}, "f": "<f>", "g": 7,
               "optional_default": "x"})
    );
    assert!(template(&json!({"type": "object"})).is_empty());
}

#[test]
fn help_text_echoes_the_typed_name_and_adds_the_search_note_only_for_search() {
    let tool = json!({
        "name": "search",
        "description": "Find things.",
        "inputSchema": {"type": "object", "required": ["query"],
                        "properties": {"query": {"type": "string"}}}
    });
    let text = help_text(&tool, "airk_search").unwrap();
    assert!(text.starts_with(
        "search\nFind things.\n\nInput schema (required arguments and defaults):\n{\n"
    ));
    assert!(text.contains(
        "\n  airkctl tools call airk_search --args-file query.json\nInside the tools console:\n  call airk_search --args-file query.json\n  call airk_search {\"query\":\"<query>\"}\n"
    ));
    assert!(text.ends_with("Omit mode for the saved collection search policy.\n"));
    let other = help_text(&json!({"name": "files", "inputSchema": {}}), "files").unwrap();
    assert!(other.starts_with("files\n\n\nInput schema"));
    assert!(other.ends_with("  call files {}\n"));
    assert_eq!(help_text(&json!({"name": "files"}), "files"), None);
}

#[test]
fn arguments_are_one_bounded_json_object() {
    assert_eq!(read_arguments(None, None).unwrap(), Map::new());
    assert_eq!(
        Value::Object(read_arguments(Some(r#"{"query":"q"}"#), None).unwrap()),
        json!({"query": "q"})
    );
    for (text, message) in [
        ("[1]", "MCP arguments must be a JSON object"),
        (
            "{",
            "Expected a UTF-8 JSON object; use --args-file to avoid shell quoting",
        ),
    ] {
        assert_eq!(
            read_arguments(Some(text), None).unwrap_err().to_string(),
            message
        );
    }
    let largest = format!(r#"{{"q":"{}"}}"#, "x".repeat(MAX_ARGUMENT_BYTES - 8));
    assert_eq!(largest.len(), MAX_ARGUMENT_BYTES);
    assert!(read_arguments(Some(&largest), None).is_ok());
    let over = format!(r#"{{"q":"{}"}}"#, "x".repeat(MAX_ARGUMENT_BYTES - 7));
    assert_eq!(
        read_arguments(Some(&over), None).unwrap_err().to_string(),
        "MCP arguments exceed 64 KiB"
    );
}

#[test]
fn argument_files_must_be_regular_and_bounded() {
    let folder = tempfile::tempdir().unwrap();
    assert_eq!(
        read_arguments(None, Some(folder.path()))
            .unwrap_err()
            .to_string(),
        "Arguments must be a regular file"
    );
    let file = folder.path().join("query.json");
    std::fs::write(&file, r#"{"query":"q"}"#).unwrap();
    assert_eq!(
        Value::Object(read_arguments(None, Some(&file)).unwrap()),
        json!({"query": "q"})
    );
    std::fs::write(&file, "x".repeat(MAX_ARGUMENT_BYTES + 1)).unwrap();
    assert_eq!(
        read_arguments(None, Some(&file)).unwrap_err().to_string(),
        "MCP arguments exceed 64 KiB"
    );
}
