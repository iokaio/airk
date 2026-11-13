// SPDX-License-Identifier: Apache-2.0
//! Exact removal through the actual CLI; explicit control roots need no installed pointer.
use serde_json::Value;
use std::{path::Path, process::Command};

fn command(project: &Path, action: &str, selectors: &[Value]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_airkctl"));
    command
        .args(["--output", "json", "agents", action, "--runtime-dir"])
        .arg(project.join("missing-control"))
        .arg("--project")
        .arg(project)
        .arg("--no-user")
        .env_remove("AIRK_ERROR_FORMAT");
    for selector in selectors {
        command.arg("--entry").arg(selector.to_string());
    }
    command.output().expect("actual airkctl process")
}

fn listed(project: &Path) -> Vec<Value> {
    let output = command(project, "list", &[]);
    assert!(output.status.success(), "list: {output:?}");
    let report: Value = serde_json::from_slice(&output.stdout).expect("native list report");
    report["entries"]
        .as_array()
        .expect("complete entries")
        .clone()
}

fn selected(entries: &[Value], key: &str) -> Value {
    let matches: Vec<_> = entries
        .iter()
        .filter(|entry| entry["serverKey"] == key)
        .collect();
    assert_eq!(matches.len(), 1, "exact native entry {key}");
    matches[0].clone()
}

#[test]
fn exact_entry_cli_removes_only_selected_rows_across_agent_files() {
    let project = tempfile::tempdir().unwrap();
    let files = [
        (
            ".mcp.json",
            "{\n  \"mcpServers\": {\n    \"airk-retired-claude\": {\"command\":\"airk\"},\n    \"airk-keep\": { \"command\" : \"airk\", \"args\" : [\"keep\"] }\n  },\n  \"policy\": true\n}\n",
        ),
        (
            ".vscode/mcp.json",
            "{\n  \"servers\": {\n    \"airk-retired-copilot\": {\"command\":\"airk\"},\n    \"airk-keep\": { \"command\" : \"airk\", \"args\" : [\"keep\"] }\n  },\n  \"policy\": true\n}\n",
        ),
        (
            ".codex/config.toml",
            "# retained policy\napproval_policy = \"on-request\"\n\n[mcp_servers.airk-retired-codex]\ncommand = \"airk\"\n\n[mcp_servers.airk-keep]\ncommand  =  \"airk\"\nargs = [\"keep\"]\n",
        ),
    ];
    for (name, source) in &files {
        let file = project.path().join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, source).unwrap();
    }
    let entries = listed(project.path());
    assert_eq!(entries.len(), 6);
    let selectors = [
        "airk-retired-claude",
        "airk-retired-copilot",
        "airk-retired-codex",
    ]
    .map(|key| selected(&entries, key));
    let output = command(project.path(), "remove", &selectors);
    assert!(output.status.success(), "selected removal: {output:?}");
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["actions"].as_array().unwrap().len(), 3);
    for action in result["actions"].as_array().unwrap() {
        assert_eq!(action["changed"], true);
        assert!(action["error"].is_null());
    }
    for (name, before) in &files {
        let after = std::fs::read_to_string(project.path().join(name)).unwrap();
        let retained = if name.ends_with("toml") {
            "[mcp_servers.airk-keep]\ncommand  =  \"airk\"\nargs = [\"keep\"]\n"
        } else {
            "\"airk-keep\": { \"command\" : \"airk\", \"args\" : [\"keep\"] }"
        };
        assert!(before.contains(retained));
        assert!(
            after.contains(retained),
            "unselected bytes in {name}: {after}"
        );
        assert!(
            !after.contains("airk-retired-"),
            "selected row in {name}: {after}"
        );
        assert!(after.contains("policy"));
    }
    assert!(!project.path().join("missing-control").exists());
}

#[test]
fn exact_entry_cli_prevalidates_the_whole_batch_before_writing() {
    let project = tempfile::tempdir().unwrap();
    let file = project.path().join(".mcp.json");
    let original =
        "{\"mcpServers\":{\"airk-a\":{\"command\":\"airk\"},\"airk-b\":{\"command\":\"airk\"}}}\n";
    std::fs::write(&file, original).unwrap();
    let entries = listed(project.path());
    let first = selected(&entries, "airk-a");
    let second = selected(&entries, "airk-b");
    let mut missing = second.clone();
    missing["serverKey"] = "airk-absent".into();
    let mut outside = second.clone();
    outside["file"] = project
        .path()
        .join("not-enumerated.json")
        .to_string_lossy()
        .into_owned()
        .into();
    let mut changed = second.clone();
    changed["command"] = "another-airk".into();
    for (name, targets) in [
        ("duplicate", vec![first.clone(), first.clone()]),
        ("missing", vec![first.clone(), missing]),
        ("out of scope", vec![first.clone(), outside]),
        ("changed", vec![first.clone(), changed]),
    ] {
        let output = command(project.path(), "remove", &targets);
        assert!(!output.status.success(), "{name}: {output:?}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original, "{name}");
    }
}

#[test]
fn exact_entry_cli_preserves_a_shared_copilot_credential_input_until_last_reference() {
    let project = tempfile::tempdir().unwrap();
    let file = project.path().join(".vscode/mcp.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let source = r#"{
  "servers": {
    "airk-remove": {"type":"http","url":"https://127.0.0.1:62133/mcp","headers":{"Authorization":"Bearer ${input:airkToken_11111111111111111111111111111111}"}},
    "airk-retain": {"type":"http","url":"https://127.0.0.1:62133/mcp","headers":{"Authorization":"Bearer ${input:airkToken_11111111111111111111111111111111}"}}
  },
  "inputs": [{"id":"airkToken_11111111111111111111111111111111","type":"command","command":"airk.httpCredential","args":{"connection_id":"11111111111111111111111111111111"}}]
}
"#;
    std::fs::write(&file, source).unwrap();
    let entries = listed(project.path());
    assert_eq!(entries.len(), 2);
    let output = command(
        project.path(),
        "remove",
        &[selected(&entries, "airk-remove")],
    );
    assert!(output.status.success(), "first removal: {output:?}");
    let after = std::fs::read_to_string(&file).unwrap();
    assert!(after.contains("airk-retain"));
    assert!(after.contains("\"command\":\"airk.httpCredential\""));
    let entries = listed(project.path());
    let output = command(
        project.path(),
        "remove",
        &[selected(&entries, "airk-retain")],
    );
    assert!(output.status.success(), "last removal: {output:?}");
    assert!(
        !std::fs::read_to_string(&file)
            .unwrap()
            .contains("airkToken_")
    );
}

#[test]
fn exact_entry_cli_keeps_partial_action_receipts_and_refused_source() {
    let project = tempfile::tempdir().unwrap();
    let claude = project.path().join(".mcp.json");
    std::fs::write(
        &claude,
        r#"{"mcpServers":{"airk-first":{"command":"airk"}}}"#,
    )
    .unwrap();
    let copilot = project.path().join(".vscode/mcp.json");
    std::fs::create_dir_all(copilot.parent().unwrap()).unwrap();
    let conflicting = r#"{"servers":{"airk-second":{"type":"http","url":"https://127.0.0.1:62133/mcp","headers":{"Authorization":"Bearer ${input:airkToken_11111111111111111111111111111111}"}}},"inputs":[{"id":"airkToken_11111111111111111111111111111111","type":"promptString"}]}"#;
    std::fs::write(&copilot, conflicting).unwrap();
    let entries = listed(project.path());
    let output = command(
        project.path(),
        "remove",
        &[
            selected(&entries, "airk-first"),
            selected(&entries, "airk-second"),
        ],
    );
    assert!(
        output.status.success(),
        "existing partial-result transport: {output:?}"
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let actions = result["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0]["serverKey"], "airk-first");
    assert_eq!(actions[0]["changed"], true);
    assert!(actions[0]["error"].is_null());
    assert_eq!(actions[1]["serverKey"], "airk-second");
    assert!(actions[1]["changed"].is_null());
    assert!(
        actions[1]["error"]
            .as_str()
            .unwrap()
            .contains("Conflicting AIRK credential input")
    );
    assert!(
        !std::fs::read_to_string(claude)
            .unwrap()
            .contains("airk-first")
    );
    assert_eq!(std::fs::read_to_string(copilot).unwrap(), conflicting);
}
