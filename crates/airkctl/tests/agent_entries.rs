// SPDX-License-Identifier: Apache-2.0
//! Exact removal through the actual CLI; explicit control roots need no installed pointer.
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// Existing-file edits exercise actual native retention eligibility. These
/// newly created fixture paths never adopt or change a user's configuration.
struct Editable {
    _temp: tempfile::TempDir,
    path: PathBuf,
}

impl Editable {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap().join("private");
        airk_ipc::endpoint::prepare_root(&path).unwrap();
        let path = PathBuf::from(
            path.to_str()
                .unwrap()
                .strip_prefix(r"\\?\")
                .unwrap_or(path.to_str().unwrap()),
        );
        let fixture = Self { _temp: temp, path };
        fixture.git(&["init", "--quiet", "--template="]);
        let excludes = fixture.path.join("empty-excludes");
        std::fs::write(&excludes, b"").unwrap();
        fixture.git(&["config", "core.excludesFile", excludes.to_str().unwrap()]);
        std::fs::write(fixture.path.join(".gitignore"), b".airk/\n").unwrap();
        std::fs::write(fixture.path.join("package.json"), br#"{"name":"airk-exact-entry-fixture","version":"0.0.0","packageManager":"npm@11.12.1","files":["payload"],"main":"payload/main.js"}"#).unwrap();
        std::fs::create_dir(fixture.path.join("payload")).unwrap();
        std::fs::write(fixture.path.join("payload/main.js"), b"fixture\n").unwrap();
        fixture
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn git(&self, arguments: &[&str]) {
        use airk_process::bounded_command::{Invocation, execute_with_timeout};
        let git = std::env::split_paths(&std::env::var_os("PATH").expect("fixture Git PATH"))
            .map(|directory| directory.join(if cfg!(windows) { "git.exe" } else { "git" }))
            .find(|path| path.is_absolute() && path.is_file())
            .expect("retained-edit integration fixtures require Git")
            .canonicalize()
            .unwrap();
        let mut args = vec![
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-C",
            self.path.to_str().unwrap(),
        ];
        args.extend_from_slice(arguments);
        let output =
            execute_with_timeout(&Invocation::new(git, &args), Duration::from_secs(5)).unwrap();
        assert!(
            output.success,
            "isolated fixture Git setup failed: {}",
            output.text
        );
    }
}

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
    let project = Editable::new();
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
    let project = Editable::new();
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
    let project = Editable::new();
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
    let project = Editable::new();
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
    let first = selected(&entries, "airk-first");
    let second = selected(&entries, "airk-second");
    let output = command(project.path(), "remove", &[first.clone(), second.clone()]);
    assert!(
        output.status.success(),
        "existing partial-result transport: {output:?}"
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    let actions = result["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 2);
    assert_eq!(
        result,
        json!({"schema": 1, "actions": [
        {"file": first["file"], "serverKey": "airk-first", "connectionId": null, "changed": true, "error": null},
        {"file": second["file"], "serverKey": "airk-second", "connectionId": "11111111111111111111111111111111", "changed": null,
            "error": "Conflicting AIRK credential input; existing configuration preserved."}
    ], "blockers": []})
    );
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

/// A valid selector is not authority to retain an arbitrary complete file.
/// Shared parents and private targets without proven Git/npm exclusions refuse.
#[test]
fn exact_entry_cli_refuses_shared_or_unreviewed_whole_file_retention() {
    const POLICY: &str = "Automatic config editing was refused; apply the edit manually: Automatic editing cannot safely retain a recovery copy here. Apply the agent configuration edit manually.";
    const SHARED: &str = "Automatic config editing was refused; apply the edit manually: parent grants foreign mutation";
    const FOREIGN_ACCESS: &str = "Automatic config editing was refused; apply the edit manually: private object grants foreign access";
    for private in [false, true] {
        let temporary = tempfile::tempdir().unwrap();
        let project = if private {
            let directory = temporary.path().join("private-without-git-or-package");
            airk_ipc::endpoint::prepare_root(&directory).unwrap();
            directory
        } else {
            temporary.path().to_path_buf()
        };
        let file = project.join(".mcp.json");
        let original = b"{\"mcpServers\":{\"airk-refused\":{\"command\":\"airk\"}},\"unrelatedSecret\":\"fictional-private-field\"}\n";
        std::fs::write(&file, original).unwrap();
        let selector = selected(&listed(&project), "airk-refused");
        let output = command(&project, "remove", std::slice::from_ref(&selector));
        assert!(
            output.status.success(),
            "partial-result transport: {output:?}"
        );
        assert!(output.stderr.is_empty(), "{output:?}");
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        let error = result["actions"][0]["error"]
            .as_str()
            .expect("retention refusal detail");
        if private {
            assert_eq!(error, POLICY);
        } else {
            // D8 checks parent mutation and private object access before Git/npm
            // eligibility. A shared temporary target can refuse at either fence;
            // an already protected temporary target still lacks recovery policy.
            assert!(
                [SHARED, FOREIGN_ACCESS, POLICY].contains(&error),
                "unexpected native refusal: {error}"
            );
        }
        assert_eq!(
            result,
            json!({"schema":1,"actions":[{
            "file":selector["file"],"serverKey":"airk-refused","connectionId":null,
            "changed":null,"error":error
        }],"blockers":[]})
        );
        assert_eq!(std::fs::read(&file).unwrap(), original);
        assert!(!project.join(".airk").exists());
        assert!(!project.join("missing-control").exists());
    }
}
