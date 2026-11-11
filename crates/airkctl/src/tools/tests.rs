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
fn arguments_take_positionals_json_and_one_runtime_directory() {
    let parsed = arguments(&["search", "--json", "--runtime-dir", "C:/x"]).unwrap();
    assert_eq!(parsed.positionals, ["search"]);
    assert!(parsed.json);
    assert_eq!(parsed.runtime_dir, Some(PathBuf::from("C:/x")));
    let parsed = arguments(&["--runtime-dir=/x"]).unwrap();
    assert_eq!(parsed.runtime_dir, Some(PathBuf::from("/x")));
    assert!(!parsed.json && parsed.positionals.is_empty());
    for invalid in [
        &["--runtime-dir"][..],
        &["--runtime-dir="],
        &["--runtime-dir", "a", "--runtime-dir", "b"],
        &["--schemas"],
        &["-j"],
    ] {
        assert!(arguments(invalid).is_err(), "{invalid:?}");
    }
}
