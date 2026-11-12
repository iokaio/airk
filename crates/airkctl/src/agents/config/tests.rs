// SPDX-License-Identifier: Apache-2.0
//! The shared cases (`fixtures/split/agents/config_cases.json`, hand-reviewed
//! expectations) and focused checks of the parsers, the U92 helper and the
//! file layer.
use super::*;
use serde_json::{Value, json};
use std::collections::HashMap;

const CASES: &str = include_str!("../../../../../fixtures/split/agents/config_cases.json");
const A: &str = "0123456789abcdef0123456789abcdef";

fn id(value: &str) -> Incarnation {
    value.parse().expect("connection id")
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

fn expect(
    name: &str,
    operation: &str,
    source: Option<&str>,
    result: Result<String, String>,
    expected: &Value,
) {
    let label = format!("{name}: {operation}");
    if expected.get("refused").is_some() {
        assert!(
            result.is_err(),
            "{label}: expected a refusal, got {result:?}"
        );
        return;
    }
    let text = result.unwrap_or_else(|error| panic!("{label}: {error}"));
    if expected.get("unchanged").is_some() {
        assert_eq!(Some(text.as_str()), source, "{label}: expected no change");
    } else {
        let wanted = expected["text"].as_str().expect("expected text");
        assert_eq!(text, wanted, "{label}");
    }
}

fn connection(spec: &Value) -> Connection<'_> {
    Connection {
        id: id(spec["connectionId"].as_str().expect("connectionId")),
        url: spec["url"].as_str().expect("url"),
        airkctl: spec["airkctl"].as_str().expect("airkctl"),
    }
}

fn run(case: &Value) {
    let name = case["name"].as_str().expect("name");
    let agent: Agent = serde_json::from_value(case["agent"].clone()).expect("agent");
    let scope: Scope = serde_json::from_value(case["scope"].clone()).expect("scope");
    let file = case["file"].as_str().expect("file");
    let source = case["source"].as_str();
    let facts = &case["observations"];
    let connections: HashMap<String, Ownership> = facts["connections"]
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(key, value)| {
                    let ownership = match value.get("owned").and_then(Value::as_str) {
                        Some(url) => Ownership::Owned {
                            url: url.to_string(),
                        },
                        None if value == "not_owned" => Ownership::NotOwned,
                        None => panic!("{name}: unknown ownership {value}"),
                    };
                    (key.clone(), ownership)
                })
                .collect()
        })
        .unwrap_or_default();
    let paths: HashMap<String, Option<bool>> = facts["paths"]
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(key, value)| (key.clone(), value.as_bool()))
                .collect()
        })
        .unwrap_or_default();
    let verified = facts
        .get("verifiedLaunchers")
        .map(strings)
        .unwrap_or_default();
    let ownership = |connection: Incarnation| {
        connections
            .get(&connection.to_string())
            .cloned()
            .unwrap_or(Ownership::Unavailable)
    };
    let exists = |path: &str| paths.get(path).copied().unwrap_or(Some(true));
    let observations = Observations {
        endpoint_url: facts["endpointUrl"].as_str(),
        trust_installed: facts["trustInstalled"].as_bool(),
        ownership: &ownership,
        path_exists: &exists,
        verified_launchers: &verified,
        windows: false,
    };
    if let Some(source) = source {
        let report = Classifier::new(&observations).file(agent, scope, file, source);
        let mut findings = serde_json::to_value(&report.findings).expect("findings");
        for finding in findings.as_array_mut().expect("array") {
            finding.as_object_mut().expect("object").remove("detail");
        }
        assert_eq!(findings, case["findings"], "{name}: findings");
        let entries: Vec<Value> = report
            .entries
            .iter()
            .map(|entry| json!({"containerPath": entry.container_path, "serverKey": entry.server_key, "status": entry.status}))
            .collect();
        assert_eq!(Value::Array(entries), case["entries"], "{name}: entries");
    }
    let target = |spec: &Value| Target {
        container_path: strings(&spec["containerPath"]),
        server_key: spec["serverKey"].as_str().expect("serverKey").to_string(),
    };
    if let Some(spec) = case.get("connect") {
        let key = spec["serverKey"].as_str().expect("serverKey");
        let result = connect(agent, scope, source, key, &connection(spec));
        expect(name, "connect", source, result, &spec["expect"]);
    }
    for operation in ["repair", "remove"] {
        let Some(spec) = case.get(operation) else {
            continue;
        };
        let text = source.unwrap_or_else(|| panic!("{name}: {operation} needs a source"));
        let result = if operation == "repair" {
            repair(agent, scope, text, &target(spec), &connection(spec))
        } else {
            remove(agent, scope, text, &target(spec))
        };
        expect(name, operation, source, result, &spec["expect"]);
    }
}

#[test]
fn shared_cases() {
    let document: Value = serde_json::from_str(CASES).expect("config_cases.json");
    assert_eq!(document["schema"], "airk.split.agents.config_cases/1");
    let cases = document["cases"].as_array().expect("cases");
    assert!(!cases.is_empty(), "no shared cases selected");
    for case in cases {
        run(case);
    }
}

#[test]
fn helper_quotes_paths_a_shell_would_split() {
    let windows = r"C:\Program Files\AIRK\airkctl.exe";
    let command = recognize::helper_command(windows, id(A)).expect("helper");
    assert_eq!(
        command,
        format!("\"{windows}\" agents credential --connection-id {A} --format header-json")
    );
    assert_eq!(
        recognize::helper_connection(&command),
        Some((windows.to_string(), id(A)))
    );
    for refused in [
        "airkctl",
        "/opt/airk/airk",
        "/opt/$HOME/airkctl",
        r"\\server\share\airkctl.exe",
    ] {
        assert!(
            recognize::helper_command(refused, id(A)).is_err(),
            "{refused}"
        );
    }
}

#[test]
fn only_the_u92_helper_form_is_recognized() {
    let ok =
        format!("/opt/airk/bin/airkctl agents credential --connection-id {A} --format header-json");
    assert!(recognize::helper_connection(&ok).is_some());
    for other in [
        format!(
            "/opt/airk/airk runtime http-credential --install-dir /opt/airk --connection-id {A} --format header-json"
        ),
        format!("airkctl agents credential --connection-id {A} --format header-json"),
        format!(
            "/opt/airk/bin/airkctl agents credential --connection-id {} --format header-json",
            A.to_uppercase()
        ),
        format!(
            "/opt/airk/bin/airkctl  agents credential --connection-id {A} --format header-json"
        ),
        format!(
            "/opt/airk/bin/airkctl agents credential --connection-id {A} --format header-json --x"
        ),
        format!("/opt/airk/bin/other agents credential --connection-id {A} --format header-json"),
    ] {
        assert_eq!(recognize::helper_connection(&other), None, "{other}");
    }
}

#[test]
fn endpoint_urls_are_loopback_https_mcp_only() {
    for ok in ["https://127.0.0.1:1/mcp", "https://127.0.0.1:65535/mcp"] {
        assert!(recognize::http_url(ok), "{ok}");
    }
    for refused in [
        "https://127.0.0.1:0/mcp",
        "https://127.0.0.1:65536/mcp",
        "https://127.0.0.1:08/mcp",
        "http://127.0.0.1:4433/mcp",
        "https://localhost:4433/mcp",
        "https://127.0.0.1:4433/mcp/",
    ] {
        assert!(!recognize::http_url(refused), "{refused}");
    }
}

#[test]
fn jsonc_accepts_comments_and_trailing_commas_only() {
    let tree = jsonc::parse(
        "\u{feff}{ /* a */ \"a\": [1, 2,], // b\n \"b\": {\"c\": \"\\u00e9\\ud83d\\ude00\"}, }",
    )
    .expect("jsonc");
    assert_eq!(
        tree.value(),
        json!({"a": [1, 2], "b": {"c": "\u{e9}\u{1f600}"}})
    );
    for refused in [
        "[]",
        "{\"a\": 1,,}",
        "{\"a\": 1} x",
        "{\"a\": tru}",
        "{'a': 1}",
        "{\"a\": \"\n\"}",
        "{\"a\": 1 /* x",
    ] {
        assert!(jsonc::parse(refused).is_err(), "{refused:?}");
    }
}

#[test]
fn toml_strings_cannot_manufacture_headers() {
    let source = "[tool]\nnote = \"\"\"\n[mcp_servers.airk]\nurl = \"x\"\n\"\"\"\n[mcp_servers.real]\ncommand = 'a # not a comment'\n";
    let rows = toml::declarations(source).expect("toml");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].server_key, "real");
    assert_eq!(rows[0].fields["command"], "a # not a comment");
    for refused in [
        "[mcp_servers.a]\n[mcp_servers.a]\n",
        "[[mcp_servers.a]]\n",
        "[mcp_servers.a]\nx = 1\nx = 2\n",
        "[mcp_servers]\na = { url = \"u\" }\n[mcp_servers.a]\ncommand = \"c\"\n",
        "x = [\n",
    ] {
        assert!(toml::declarations(refused).is_err(), "{refused:?}");
    }
}

#[test]
fn toml_inline_airk_entries_need_manual_review() {
    let source = format!(
        "[mcp_servers]\nairk = {{ url = \"https://127.0.0.1:4433/mcp\", http_headers_helper = \"/opt/airk/bin/airkctl agents credential --connection-id {A} --format header-json\" }}\n"
    );
    let target = Target {
        container_path: vec!["mcp_servers".into()],
        server_key: "airk".into(),
    };
    assert!(remove(Agent::Codex, Scope::Project, &source, &target).is_err());
}

#[test]
fn user_locations_follow_the_platform() {
    let env = UserEnv {
        home: Some(r"C:\Users\u".into()),
        appdata: Some(r"D:\Roaming".into()),
        codex_home: Some("relative".into()),
        ..UserEnv::default()
    };
    let found = user_locations(Platform::Windows, &env);
    let files: Vec<(Agent, &str)> = found
        .files
        .iter()
        .map(|(agent, path)| (*agent, path.as_str()))
        .collect();
    assert_eq!(
        files,
        [
            (Agent::Codex, r"C:\Users\u\.codex\config.toml"),
            (Agent::Claude, r"C:\Users\u\.claude.json"),
            (Agent::Copilot, r"D:\Roaming\Code\User\mcp.json"),
            (Agent::Copilot, r"D:\Roaming\Code - Insiders\User\mcp.json"),
        ]
    );
    assert_eq!(found.blockers, ["invalid_codex_home"]);
    let linux = user_locations(
        Platform::Linux,
        &UserEnv {
            home: Some("/home/u".into()),
            ..UserEnv::default()
        },
    );
    assert_eq!(
        linux.files[2],
        (
            Agent::Copilot,
            "/home/u/.config/Code/User/mcp.json".to_string()
        )
    );
}

#[test]
fn edit_file_creates_project_folders_and_replaces_atomically() {
    let root = tempfile::tempdir().expect("tempdir");
    let root = root.path().canonicalize().expect("canonical root");
    let file = root.join(".vscode").join("mcp.json");
    let wrote = edit_file(&file, Some(root.as_path()), true, |before| {
        assert_eq!(before, None);
        Ok("{}\n".into())
    })
    .expect("create");
    assert!(wrote);
    let unchanged = edit_file(&file, Some(root.as_path()), false, |before| {
        Ok(before.unwrap_or_default().to_string())
    })
    .expect("unchanged");
    assert!(!unchanged);
    let replaced = edit_file(&file, Some(root.as_path()), false, |_| {
        Ok("{ \"servers\": {} }\n".into())
    })
    .expect("replace");
    assert!(replaced);
    assert_eq!(
        std::fs::read_to_string(&file).expect("read"),
        "{ \"servers\": {} }\n"
    );
    let user = root.join("missing").join("config.toml");
    assert!(edit_file(&user, None, true, |_| Ok(String::new())).is_err());
}

#[cfg(unix)]
#[test]
fn linked_configuration_files_are_refused() {
    let root = tempfile::tempdir().expect("tempdir");
    let target = root.path().join("real.json");
    std::fs::write(&target, "{}\n").expect("write");
    let link = root.path().join(".mcp.json");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    assert!(edit_file(&link, None, false, |_| Ok("{\"x\": 1}\n".into())).is_err());
    assert_eq!(std::fs::read_to_string(&target).expect("read"), "{}\n");
}
