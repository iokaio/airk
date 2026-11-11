// SPDX-License-Identifier: Apache-2.0
use serde_json::{Value, json};
mod common;
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn command(root: &Path, state: &Path) -> Command {
    common::evidence_command(root, state, "tools-test")
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn evidence(output: Output) -> Value {
    let envelope = success(output);
    assert_ne!(envelope["isError"], true);
    serde_json::from_str(envelope["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn tools_help_works_without_a_workspace_and_exposes_live_schemas() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("missing root");
    let state = base.path().join("missing state");
    let list = success(
        command(&root, &state)
            .args(["tools", "list", "--json"])
            .output()
            .unwrap(),
    );
    let tools = list["tools"].as_array().unwrap();
    assert_eq!(tools.len(), airk_mcp::unified::tools().len());
    assert_eq!(
        list["tools"],
        serde_json::to_value(airk_mcp::unified::tools()).unwrap()
    );
    assert!(tools.iter().any(|tool| tool["name"] == "help"));
    let query_scope = tools
        .iter()
        .find(|tool| tool["name"] == "query_scope")
        .expect("query scope metadata tool");
    assert_eq!(query_scope["annotations"]["readOnlyHint"], true);
    let mut discovery_properties =
        airk_core::combined_reads::query_scope::schema()["properties"].clone();
    // Public routing permits only the already configured project binding;
    // it is consumed by Unified before native grant-bound discovery.
    discovery_properties["project"] = query_scope["inputSchema"]["properties"]["project"].clone();
    assert_eq!(discovery_properties["project"]["pattern"], "^[0-9a-f]{64}$");
    assert_eq!(
        query_scope["inputSchema"]["properties"],
        discovery_properties
    );
    assert_eq!(query_scope["inputSchema"]["additionalProperties"], false);
    for field in ["project_ids", "index_ids", "verification", "response_mode"] {
        assert!(
            query_scope["inputSchema"]["properties"]
                .get(field)
                .is_none(),
            "discovery must not advertise {field}"
        );
    }
    for alias in ["query_scope", "airk_query_scope"] {
        let discovery = command(&root, &state)
            .args(["tools", "call", alias, "--args", r#"{"view":"projects"}"#])
            .output()
            .unwrap();
        assert_eq!(discovery.status.code(), Some(1));
        assert!(
            discovery.stdout.is_empty(),
            "missing authority must not produce successful discovery"
        );
        assert!(
            String::from_utf8_lossy(&discovery.stderr).contains("runtime installation is missing")
        );
        assert!(
            !root.exists() && !state.exists(),
            "scope discovery created source/private state"
        );
    }
    let help = evidence(
        command(&root, &state)
            .args(["tools", "call", "airk_help"])
            .output()
            .unwrap(),
    );
    assert!(help["topics"].as_array().unwrap().len() >= 25);
    let help = evidence(
        command(&root, &state)
            .args([
                "tools",
                "call",
                "airk_help",
                "--args",
                r#"{"command":"runtime retire-previous"}"#,
            ])
            .output()
            .unwrap(),
    );
    assert!(
        help["content"]
            .as_str()
            .unwrap()
            .contains("--expected-active")
    );
    assert!(!root.exists() && !state.exists());
    let status = tools.iter().find(|tool| tool["name"] == "status").unwrap();
    assert!(
        status["description"]
            .as_str()
            .unwrap()
            .contains("Quick version, health and project-index check")
    );
    for tool in tools {
        let help = success(
            command(&root, &state)
                .args(["tools", "help", tool["name"].as_str().unwrap(), "--json"])
                .output()
                .unwrap(),
        );
        assert_eq!(&help, tool);
    }
    let search = tools.iter().find(|t| t["name"] == "search").unwrap();
    assert_eq!(search["inputSchema"]["properties"]["limit"]["default"], 10);
    let output = command(&root, &state)
        .args(["tools", "help", "airk_search"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--args-file query.json") && help.contains("Explicit vector search"));
    assert!(
        !command(&root, &state)
            .args(["tools", "help", "not_a_tool"])
            .status()
            .unwrap()
            .success()
    );
    assert!(!root.exists() && !state.exists());
}

#[test]
fn tool_calls_preserve_evidence_scope_errors_and_argument_budgets() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("README.md"),
        "# Fixture\nAuthenticate the local account.\n",
    )
    .unwrap();
    let service = common::Service::reviewed(root.path(), state.path(), "tools-test");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    let accepted = success(
        common::command(root.path(), state.path(), "tools-test")
            .arg("index")
            .output()
            .unwrap(),
    );
    service.wait(serde_json::from_value(accepted["job"]["id"].clone()).unwrap());
    let query = state.path().join("query with spaces.json");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    // An accepted Index job can finish before its setup/source continuations.
    // Settle the owned native work before asserting running-worker diagnostics.
    service
        .settle_reviewed_index(root.path(), state.path(), deadline)
        .unwrap();
    let status = evidence(
        common::read_retry::output_until(
            command(root.path(), state.path()).args(["tools", "call", "airk_diagnostics"]),
            deadline,
        )
        .unwrap(),
    );
    common::read_retry::publication_pending(
        &status,
        "tools-test",
        &service.project.binding.to_string(),
    )
    .unwrap();
    // A reviewed index grant intentionally cannot disclose project-wide jobs,
    // process inventory, or watcher state. Observe those using the fixture's
    // native administrative authority, never widen its evidence grant.
    assert_eq!(
        status["runtime"],
        json!({"schema":1,"error":"native_runtime_context_unavailable"})
    );
    assert_eq!(
        status["work"],
        json!({"error":"native_work_context_unavailable"})
    );
    assert!(status["scope_observation"]["fingerprint"].is_string());
    use airk_core::service::{ControlMethod, ControlResponse};
    let health = match service
        .request_until(ControlMethod::Health, deadline)
        .unwrap()
    {
        ControlResponse::Health { observation } => observation,
        _ => panic!("native health unavailable"),
    };
    let worker = match service
        .request_until(
            ControlMethod::Worker {
                project: service.project.binding,
            },
            deadline,
        )
        .unwrap()
    {
        ControlResponse::Worker {
            project,
            identity: Some(identity),
            ..
        } => {
            assert_eq!(project, service.project.binding);
            identity
        }
        _ => panic!("native worker identity unavailable"),
    };
    assert_eq!(worker.project, service.project.binding);
    assert_eq!(worker.service, health.instance);
    assert_eq!(health.version, env!("CARGO_PKG_VERSION"));
    // The current single executable is the actual service image. Do not invent
    // a scoped worker/front-end process report that the native API withholds.
    use sha2::{Digest, Sha256};
    let expected_image = format!(
        "{:x}",
        Sha256::digest(fs::read(env!("CARGO_BIN_EXE_airk")).unwrap())
    );
    assert_eq!(
        serde_json::to_value(&health).unwrap()["executable_sha256"],
        expected_image
    );
    let version =
        common::read_retry::once_until(common::native_command().arg("--version"), deadline)
            .unwrap();
    assert!(version.status.success());
    assert!(
        String::from_utf8(version.stdout)
            .unwrap()
            .contains(env!("CARGO_PKG_VERSION"))
    );
    match service
        .request_until(
            ControlMethod::Progress {
                project: worker.project,
            },
            deadline,
        )
        .unwrap()
    {
        ControlResponse::Progress {
            project,
            pending_job,
            queued_jobs,
            observation,
            ..
        } => {
            assert_eq!(project, worker.project);
            assert!(pending_job.is_none_or(|job| job.project == worker.project));
            assert!(queued_jobs.iter().all(|job| job.project == worker.project));
            assert!(
                observation.is_none_or(|sample| sample.worker.project == worker.project
                    && sample.job.project == worker.project)
            );
        }
        _ => panic!("native work observation unavailable"),
    }
    match service
        .request_until(
            ControlMethod::Watch {
                project: worker.project,
            },
            deadline,
        )
        .unwrap()
    {
        ControlResponse::Watch {
            project,
            observation,
        } => {
            assert_eq!(project, worker.project);
            observation.validate().unwrap();
            assert!(serde_json::to_value(observation).unwrap()["enabled"].is_boolean());
        }
        _ => panic!("native watch observation unavailable"),
    }
    fs::write(
        &query,
        br#"{"query":"Authenticate","mode":"lexical","limit":1}"#,
    )
    .unwrap();
    let found = evidence(
        common::read_retry::output(
            command(root.path(), state.path())
                .args(["tools", "call", "airk_search", "--args-file"])
                .arg(&query),
        )
        .unwrap(),
    );
    assert_eq!(found["collection"], "tools-test");
    assert_eq!(found["hits"][0]["citation"]["path"], "README.md");
    let read = json!({"path":"README.md","start_line":1,"end_line":2,"revision":found["publication"]["token"]});
    let read = evidence(
        common::read_retry::output(command(root.path(), state.path()).args([
            "tools",
            "call",
            "airk_read_file",
            "--args",
            &read.to_string(),
        ]))
        .unwrap(),
    );
    assert_eq!(read["publication"], found["publication"]);
    assert_eq!(
        read["hits"][0]["citation"]["content_hash"],
        found["hits"][0]["citation"]["content_hash"]
    );
    for (tool, args) in [
        ("airk_search", "[]"),
        ("airk_search", "{"),
        ("not_a_tool", "{}"),
        (
            "airk_search",
            r#"{"query":"Authenticate","collection":"other"}"#,
        ),
        ("airk_search", r#"{"query":"Authenticate","mode":"vector"}"#),
        ("airk_search", r#"{"query":42}"#),
        (
            "airk_read_file",
            r#"{"path":"../secret","start_line":1,"end_line":1}"#,
        ),
    ] {
        let output = command(root.path(), state.path())
            .args(["tools", "call", tool, "--args", args])
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {tool}: {args}");
        if !output.stdout.is_empty() {
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["isError"], true);
        }
    }
    fs::write(&query, vec![b' '; 65537]).unwrap();
    let output = command(root.path(), state.path())
        .args(["tools", "call", "airk_search", "--args-file"])
        .arg(&query)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("64 KiB"));
    assert!(
        !command(root.path(), state.path())
            .args([
                "tools",
                "call",
                "airk_search",
                "--args",
                "{}",
                "--args-file"
            ])
            .arg(&query)
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn interactive_tools_show_help_and_recover_from_invalid_requests() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let _service = common::Service::reviewed(root.path(), state.path(), "tools-test");
    let query = root.path().join("arguments with spaces.json");
    fs::write(&query, r#"{"reply_detail":"rich","adapter":"text"}"#).unwrap();
    let mut child = command(root.path(), state.path())
        .args(["tools", "shell"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Small input/output avoids pipe backpressure; interactive failure must not end the session.
    writeln!(child.stdin.take().unwrap(), "help\nhelp airk_search\ncall not_a_tool\ncall airk_search []\ncall airk_capabilities --args-file {}\nexit", query.file_name().unwrap().to_str().unwrap()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("AIRK Tools")
            && text.contains("tools-test")
            && text.contains("Explicit vector search")
    );
    assert!(text.contains("Result:") && text.contains("\"search_modes\": ["));
    assert!(text.contains("\"reply_detail\": \"rich\""), "{text}");
    assert!(
        text.contains("\"subject\": \"selected_adapter\"") && text.contains("\"read_guidance\""),
        "{text}"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("JSON object"));
}

#[test]
fn native_help_cli_reports_detail_selector_field_without_opening_a_workspace() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("missing root");
    let state = base.path().join("missing state");
    let args = json!({"topic":"workbench","detail":"full"});
    let output = command(&root, &state)
        .args(["tools", "call", "airk_help", "--args", &args.to_string()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result,
        serde_json::to_value(airk_mcp::help::call(args)).unwrap()
    );
    let value: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["error"]["code"], "invalid_help_arguments");
    assert_eq!(value["error"]["field"], "detail");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("query or topic_id")
    );
    assert!(!root.exists() && !state.exists());
}

/// Fields that vary by observation, as the CLI/MCP parity tests strip them.
fn stable(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for key in [
                "request_id",
                "observed_unix_ms",
                "observed_from_unix_ms",
                "observed_to_unix_ms",
                "cache_age_ms",
                "cached",
                "elapsed_ms",
            ] {
                fields.remove(key);
            }
            fields.values_mut().for_each(stable);
        }
        Value::Array(rows) => rows.iter_mut().for_each(stable),
        _ => {}
    }
}

/// An envelope with its JSON text content parsed, then stabilized.
fn comparable(mut envelope: Value) -> Value {
    if let Some(content) = envelope["content"].as_array_mut() {
        for item in content {
            if let Some(text) = item["text"].as_str()
                && let Ok(parsed) = serde_json::from_str::<Value>(text)
            {
                item["text"] = parsed;
            }
        }
    }
    stable(&mut envelope);
    envelope
}

/// What one call produced: an envelope, or the failure message the CLI's
/// error line and M3's `ToolCallFailed` carry.
#[derive(Debug, PartialEq)]
enum Outcome {
    Envelope(Value),
    Failed(String),
}

fn cli_outcome(output: Output) -> Outcome {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.stdout.is_empty() {
        let line = stderr.lines().last().unwrap_or_default();
        return Outcome::Failed(line.strip_prefix("Error: ").unwrap_or(line).to_owned());
    }
    Outcome::Envelope(comparable(serde_json::from_slice(&output.stdout).unwrap()))
}

fn control_outcome(
    control: &Path,
    tool: &str,
    arguments: &Value,
    selection: airk_core::service::ToolSelection,
) -> Outcome {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let response = runtime
        .block_on(airk_cli::service_runtime::request(
            control,
            airk_core::service::ControlMethod::ToolsCall {
                tool: tool.into(),
                arguments: arguments.as_object().unwrap().clone(),
                selection,
                response_mode: airk_core::service::ToolResponseMode::Full,
                usage_client: airk_core::usage::ClientFamily::Cli,
            },
        ))
        .unwrap();
    match response {
        airk_core::service::ControlResponse::ToolResult { result } => {
            Outcome::Envelope(comparable(result))
        }
        airk_core::service::ControlResponse::ToolCallFailed { message } => Outcome::Failed(message),
        other => panic!(
            "unexpected M3 response: {}",
            serde_json::to_string(&other).unwrap()
        ),
    }
}

/// M3 runs the CLI's own call path in the service: for each selection the
/// control answer equals what `airk tools call` printed (split step 3a.2b).
#[test]
fn control_tool_calls_answer_what_the_cli_printed() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "tools-call");
    fs::write(
        root.path().join("recovery.md"),
        "# Recovery\n\nA forgotten password is recovered from the account page.\n",
    )?;
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    common::call(root.path(), state.path(), "tools-call", &["index"])?;
    let airk_core::service_http::HttpSelection::Scoped { selection } = service.http_selection()
    else {
        panic!("the reviewed fixture selects one scoped index");
    };
    let calls = [
        ("airk_read_file", json!({"path": "recovery.md"})),
        ("airk_file_info", json!({"path": "recovery.md"})),
        ("airk_list_files", json!({})),
        (
            "airk_search",
            json!({"query": "forgotten password", "limit": 3}),
        ),
        ("airk_outline", json!({"path": "missing.md"})),
        ("help", json!({"topic": "workbench"})),
    ];
    let mut envelopes = 0;
    for (tool, arguments) in &calls {
        let arguments_text = arguments.to_string();
        let scoped = cli_outcome(
            common::scoped_command(root.path(), state.path(), "tools-call")
                .args(["tools", "call", tool, "--args", &arguments_text])
                .output()?,
        );
        let control = control_outcome(
            &service.control,
            tool,
            arguments,
            airk_core::service::ToolSelection::Catalog {
                request: selection.clone(),
            },
        );
        assert_eq!(control, scoped, "catalog selection {tool}");
        envelopes += usize::from(matches!(control, Outcome::Envelope(_)));
        let project = cli_outcome(
            common::command(root.path(), state.path(), "tools-call")
                .args(["tools", "call", tool, "--args", &arguments_text])
                .output()?,
        );
        let control = control_outcome(
            &service.control,
            tool,
            arguments,
            airk_core::service::ToolSelection::Project {
                binding: service.project.binding,
            },
        );
        assert_eq!(control, project, "project selection {tool}");
    }
    assert!(envelopes >= 4, "the catalog selection must read evidence");
    // Without any selection the CLI answers its help tool in process.
    for arguments in [
        json!({"topic": "workbench"}),
        json!({"topic": "workbench", "detail": "full"}),
    ] {
        let text = arguments.to_string();
        let cli = cli_outcome(
            Command::new(env!("CARGO_BIN_EXE_airk"))
                .args(["tools", "call", "help", "--args", &text])
                .env("AIRK_MCP_RESPONSE_MODE", "full")
                .output()?,
        );
        let control = control_outcome(
            &service.control,
            "help",
            &arguments,
            airk_core::service::ToolSelection::Unselected,
        );
        assert_eq!(control, cli, "unselected help {text}");
    }
    Ok(())
}
