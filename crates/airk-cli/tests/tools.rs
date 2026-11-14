// SPDX-License-Identifier: Apache-2.0
use serde_json::{Value, json};
mod common;
use common::parity::{CLOCK_FIELDS, Parity};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn command(root: &Path, state: &Path) -> common::ctl::Ctl {
    common::evidence_command(root, state, "tools-test")
}

/// A read with no project fixture. `airkctl` asks a service for the catalog
/// and the help tool (the pre-split CLI answered them in process), so a
/// project-less service stands in for the absent installation.
fn offline(root: &Path, state: &Path, service: &common::BareService) -> common::ctl::Ctl {
    let mut command = common::ctl::Ctl::new();
    command
        .arg("--root")
        .arg(root)
        .arg("--data-dir")
        .arg(state)
        .args(["--collection", "tools-test", "--service-runtime-dir"])
        .arg(&service.control)
        .env("AIRK_MCP_RESPONSE_MODE", "full");
    command
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
    let service = common::BareService::start();
    let command = |root: &Path, state: &Path| offline(root, state, &service);
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
            String::from_utf8_lossy(&discovery.stderr).contains("project_unresolved"),
            "{}",
            String::from_utf8_lossy(&discovery.stderr)
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
                r#"{"command":"airkctl agents connect"}"#,
            ])
            .output()
            .unwrap(),
    );
    assert!(
        help["content"]
            .as_str()
            .unwrap()
            .contains("--selection-file")
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
        common::read_retry::once_until(&*common::ctl::Ctl::new().arg("--version"), deadline)
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
    // Interactive failure must not end the session.
    let output = command(root.path(), state.path())
        .args(["tools", "shell"])
        .input(format!("help\nhelp airk_search\ncall not_a_tool\ncall airk_search []\ncall airk_capabilities --args-file {}\nexit\n", query.file_name().unwrap().to_str().unwrap()))
        .output()
        .unwrap();
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
    let service = common::BareService::start();
    let command = |root: &Path, state: &Path| offline(root, state, &service);
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

/// The control answer in canonical space, as the recorded `airk` output.
fn control_outcome(
    parity: &Parity,
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
                read_attempt: None,
                tool: tool.into(),
                arguments: arguments.as_object().unwrap().clone(),
                selection,
                response_mode: airk_core::service::ToolResponseMode::Full,
                usage_client: airk_core::usage::ClientFamily::Cli,
            },
        ))
        .unwrap();
    match response {
        airk_core::service::ControlResponse::ToolResult { result } => Outcome::Envelope(
            comparable(serde_json::from_str(&parity.canonical(&result.to_string())).unwrap()),
        ),
        airk_core::service::ControlResponse::ToolCallFailed { message } => {
            Outcome::Failed(parity.canonical(&message))
        }
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
    let mut parity = Parity::new("control_tool_calls_answer_what_the_cli_printed");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    // The receipt budget depends on the fixture paths' length.
    parity.mask_json_fields(&["receipt_bytes_budget"]);
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
            parity
                .old(&format!("catalog {tool}"), || {
                    common::legacy_airk_scoped_command(root.path(), state.path(), "tools-call")
                        .args(["tools", "call", tool, "--args", &arguments_text])
                        .output()
                        .unwrap()
                })
                .output(),
        );
        let control = control_outcome(
            &parity,
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
            parity
                .old(&format!("project {tool}"), || {
                    common::legacy_airk_command(root.path(), state.path(), "tools-call")
                        .args(["tools", "call", tool, "--args", &arguments_text])
                        .output()
                        .unwrap()
                })
                .output(),
        );
        let control = control_outcome(
            &parity,
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
            parity
                .old(&format!("unselected help {text}"), || {
                    Command::new(env!("CARGO_BIN_EXE_airk"))
                        .args(["tools", "call", "help", "--args", &text])
                        .env("AIRK_MCP_RESPONSE_MODE", "full")
                        .output()
                        .unwrap()
                })
                .output(),
        );
        let control = control_outcome(
            &parity,
            &service.control,
            "help",
            &arguments,
            airk_core::service::ToolSelection::Unselected,
        );
        assert_eq!(control, cli, "unselected help {text}");
    }
    parity.finish();
    Ok(())
}

/// What a command line produced: exit code, comparable stdout and the final
/// error message without its program prefix.
fn line_outcome(
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    prefix: &str,
) -> (Option<i32>, Option<Value>, String) {
    let stdout = (!stdout.is_empty()).then(|| comparable(serde_json::from_slice(stdout).unwrap()));
    let stderr = String::from_utf8_lossy(stderr);
    let message = stderr
        .lines()
        .last()
        .unwrap_or_default()
        .strip_prefix(prefix)
        .unwrap_or_default()
        .to_owned();
    (code, stdout, message)
}

/// The recorded `airk` run's outcome (split U95).
fn airk_outcome(recorded: common::parity::Recorded) -> (Option<i32>, Option<Value>, String) {
    line_outcome(
        recorded.code,
        recorded.stdout.as_bytes(),
        recorded.stderr.as_bytes(),
        "Error: ",
    )
}

/// The `airkctl` command line run in process (`airkctl::cli::run`, the code its
/// `main` runs) with `AIRK_MCP_RESPONSE_MODE=full`, as the fixture commands set.
/// Its output is compared in canonical space, as the recorded `airk` output.
fn airkctl_outcome(parity: &Parity, arguments: &[&str]) -> (Option<i32>, Option<Value>, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut io = airkctl::cli::Io {
        read_attempt: None,
        stdin: &mut std::io::empty(),
        stdout: &mut stdout,
        stderr: &mut stderr,
        full_responses: true,
        window: None,
    };
    let line = std::iter::once("airkctl")
        .chain(arguments.iter().copied())
        .map(std::ffi::OsString::from)
        .collect();
    let code = airkctl::cli::run(line, &mut io);
    let stdout = parity.canonical(&String::from_utf8_lossy(&stdout));
    let stderr = parity.canonical(&String::from_utf8_lossy(&stderr));
    line_outcome(
        Some(i32::from(code)),
        stdout.as_bytes(),
        stderr.as_bytes(),
        "airkctl: ",
    )
}

/// `airkctl` evidence reads and tool calls print what `airk` printed, for both
/// selections, on the reviewed indexed fixture (split step 3a.2c).
#[test]
fn airkctl_reads_print_what_airk_printed() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-reads");
    fs::write(
        root.path().join("recovery.md"),
        "# Recovery\n\nA forgotten password is recovered from the account page.\n",
    )?;
    fs::write(
        root.path().join("account.rs"),
        "use std::fmt;\n\npub fn recover_password(user: &str) -> String {\n    format!(\"reset {user}\")\n}\n",
    )?;
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    common::call(root.path(), state.path(), "ctl-reads", &["index"])?;
    let airk_core::service_http::HttpSelection::Scoped { selection } = service.http_selection()
    else {
        panic!("the reviewed fixture selects one scoped index");
    };
    let arguments_file = state.path().join("ctl-args.json");
    fs::write(&arguments_file, r#"{"query":"recover","limit":2}"#)?;
    let arguments_file = arguments_file.to_str().unwrap().to_owned();
    let reads: Vec<Vec<&str>> = vec![
        vec!["status"],
        vec!["parse", "where is the password recovered"],
        vec!["search", "forgotten password", "--limit", "3"],
        vec![
            "search",
            "recover",
            "--mode",
            "lexical",
            "--language",
            "rust",
        ],
        vec!["files"],
        vec![
            "read",
            "recovery.md",
            "--start-line",
            "1",
            "--end-line",
            "2",
        ],
        vec!["info", "account.rs"],
        vec!["outline", "account.rs"],
        vec!["symbol", "recover_password"],
        vec!["imports"],
        vec!["facts"],
        vec!["code-facts", "--name", "recover_password"],
        vec!["relationships", "recover_password"],
        vec!["refs", "recover_password"],
        vec![
            "context",
            "account.rs",
            "3",
            "--before",
            "1",
            "--after",
            "1",
        ],
        vec!["changes"],
        vec!["read", "missing.md", "--start-line", "1", "--end-line", "2"],
        vec!["tools", "call", "airk_status"],
        vec![
            "tools",
            "call",
            "airk_search",
            "--args-file",
            &arguments_file,
        ],
        vec![
            "tools",
            "call",
            "help",
            "--args",
            r#"{"topic":"workbench"}"#,
        ],
    ];
    let catalog = serde_json::to_string(&selection)?;
    let binding = service.project.binding.to_string();
    let control = service.control.to_str().unwrap().to_owned();
    let mut parity = Parity::new("airkctl_reads_print_what_airk_printed");
    parity.substitute_path(Path::new(&arguments_file), "<ARGS_FILE>");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    // The receipt budget depends on the fixture paths' length.
    parity.mask_json_fields(&["receipt_bytes_budget"]);
    let mut compared = 0;
    for read in &reads {
        let name = parity.name(&read.join(" "));
        let old = airk_outcome(parity.old(&format!("catalog {name}"), || {
            common::legacy_airk_scoped_command(root.path(), state.path(), "ctl-reads")
                .args(read)
                .output()
                .unwrap()
        }));
        let mut line = vec![
            "--service-runtime-dir",
            &control,
            "--catalog-session",
            &catalog,
        ];
        line.extend(read.iter().copied());
        assert_eq!(
            airkctl_outcome(&parity, &line),
            old,
            "catalog selection {read:?}"
        );
        let old = airk_outcome(parity.old(&format!("project {name}"), || {
            common::legacy_airk_command(root.path(), state.path(), "ctl-reads")
                .args(read)
                .output()
                .unwrap()
        }));
        let mut line = vec![
            "--service-runtime-dir",
            &control,
            "--service-project",
            &binding,
        ];
        line.extend(read.iter().copied());
        assert_eq!(
            airkctl_outcome(&parity, &line),
            old,
            "project selection {read:?}"
        );
        compared += 1;
    }
    assert_eq!(compared, reads.len());
    parity.finish();
    Ok(())
}

/// Without a binding or session, `airkctl` resolves the project its
/// `--root`/`--data-dir`/`--collection` name through M1 and reads it as
/// `airk` read the bound project (split step 3c.1).
#[test]
fn airkctl_resolves_the_project_its_folder_names() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-resolve");
    fs::write(
        root.path().join("notes.md"),
        "# Notes\n\nResolved by folder.\n",
    )?;
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    common::call(root.path(), state.path(), "ctl-resolve", &["index"])?;
    let control = service.control.to_str().unwrap().to_owned();
    let project_root = service.project.root.clone();
    let data_root = service.project.data_root.clone();
    let mut parity = Parity::new("airkctl_resolves_the_project_its_folder_names");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    for read in [
        vec!["status"],
        vec!["files"],
        vec!["read", "notes.md", "--start-line", "1", "--end-line", "3"],
    ] {
        let old = airk_outcome(parity.old(&format!("resolved {}", read.join(" ")), || {
            common::legacy_airk_command(root.path(), state.path(), "ctl-resolve")
                .args(&read)
                .output()
                .unwrap()
        }));
        let mut line = vec![
            "--service-runtime-dir",
            &control,
            "--root",
            &project_root,
            "--data-dir",
            &data_root,
            "--collection",
            "ctl-resolve",
        ];
        line.extend(read.iter().copied());
        assert_eq!(airkctl_outcome(&parity, &line), old, "resolved {read:?}");
    }
    // An existing folder, disjoint from the data root, that is not registered
    // (a missing or overlapping folder is refused before the registry lookup).
    let elsewhere = tempfile::tempdir()?;
    let missing = elsewhere.path().to_str().unwrap();
    let (code, _, message) = airkctl_outcome(
        &parity,
        &[
            "--service-runtime-dir",
            &control,
            "--root",
            missing,
            "--data-dir",
            &data_root,
            "status",
        ],
    );
    assert_eq!(code, Some(1));
    assert!(
        message.starts_with("project is not registered"),
        "{message}"
    );
    parity.finish();
    Ok(())
}

/// `airkctl pause|resume|watch|projects` print what `airk` printed for the
/// same project (split step 3c.2). Each `airk` call precedes the `airkctl`
/// one, so both observe the same state.
#[test]
fn airkctl_project_commands_print_what_airk_printed() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-admin");
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let control = service.control.to_str().unwrap().to_owned();
    let binding = service.project.binding.to_string();
    let mut parity = Parity::new("airkctl_project_commands_print_what_airk_printed");
    // The default project name is the folder's name.
    parity.substitute(
        root.path().file_name().unwrap().to_string_lossy(),
        "<ROOT_NAME>",
    );
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    for command in [
        vec!["pause"],
        vec!["resume"],
        vec!["watch", "off"],
        vec!["watch", "on"],
        vec!["projects", "--runtime-dir", &control],
    ] {
        let name = parity.name(&command.join(" "));
        let old = parity.old(&name, || {
            common::legacy_airk_command(root.path(), state.path(), "ctl-admin")
                .args(&command)
                .output()
                .unwrap()
        });
        let mut line = vec![
            "airkctl",
            "--service-runtime-dir",
            &control,
            "--service-project",
            &binding,
        ];
        line.extend(command.iter().copied());
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: true,
            window: None,
        };
        let code = airkctl::cli::run(line.into_iter().map(Into::into).collect(), &mut io);
        assert_eq!(
            (
                Some(i32::from(code)),
                parity.canonical(&String::from_utf8(stdout)?)
            ),
            (old.code, old.stdout),
            "{command:?}: {}",
            String::from_utf8_lossy(&stderr)
        );
    }
    parity.finish();
    Ok(())
}

/// M5-M8 on a live service follow the journal rules of contracts §2.4
/// (split step 3c.3b): duplicate, conflict, expiry, key mismatch and capacity
/// for M5; listing; key-idempotent submission; forgetting.
#[test]
fn the_request_journal_follows_its_contract_rules() -> anyhow::Result<()> {
    use airk_core::{
        identity::Incarnation,
        service::{ControlMethod as Method, ControlResponse as Response, JobKind, RequestKey},
    };
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-journal");
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let project = service.project.binding;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let request = |method| {
        runtime
            .block_on(airk_cli::service_runtime::request(&service.control, method))
            .unwrap()
    };
    let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch) else {
        panic!("no request epoch");
    };
    let id = |byte: u8| Incarnation::from_bytes([byte; 16]);
    let key = |nonce: Incarnation, epoch: u64| RequestKey {
        namespace,
        epoch,
        nonce,
    };
    let prepare =
        |caller: Incarnation, key: RequestKey, kind: JobKind| Method::ClientRequestPrepare {
            caller,
            key: key.clone(),
            request: Box::new(Method::Enqueue {
                project,
                caller,
                key,
                kind,
            }),
        };
    let code = |response: Response| match response {
        Response::Failure { code } => code,
        other => panic!(
            "expected a refusal: {}",
            serde_json::to_string(&other).unwrap()
        ),
    };
    let (caller, nonce) = (id(0x0c), id(0x0b));
    let first = request(prepare(caller, key(nonce, epoch), JobKind::Index));
    assert!(matches!(
        first,
        Response::ClientRequestPrepared {
            duplicate: false,
            ..
        }
    ));
    let again = request(prepare(caller, key(nonce, epoch), JobKind::Index));
    assert!(
        matches!(again, Response::ClientRequestPrepared { duplicate: true, nonce: n, .. } if n == nonce)
    );
    assert_eq!(
        code(request(prepare(
            caller,
            key(nonce, epoch),
            JobKind::Inspect
        ))),
        "client_request_key_conflict"
    );
    assert_eq!(
        code(request(prepare(
            caller,
            key(id(0x1b), epoch + 1000),
            JobKind::Index
        ))),
        "client_request_key_expired"
    );
    let mismatched = Method::ClientRequestPrepare {
        caller: id(0x0d),
        key: key(id(0x2b), epoch),
        request: Box::new(Method::Enqueue {
            project,
            caller,
            key: key(id(0x2b), epoch),
            kind: JobKind::Index,
        }),
    };
    assert_eq!(code(request(mismatched)), "client_request_key_mismatch");
    let Response::ClientRequests { pending, capacity } = request(Method::ClientRequestList {})
    else {
        panic!("no journal list");
    };
    assert_eq!(capacity, 64);
    assert_eq!(pending.len(), 1);
    assert_eq!((pending[0].nonce, pending[0].kind), (nonce, JobKind::Index));
    let Response::Job { job } = request(Method::ClientRequestSubmit { nonce }) else {
        panic!("submission refused");
    };
    let Response::Job { job: replayed } = request(Method::ClientRequestSubmit { nonce }) else {
        panic!("resubmission refused");
    };
    assert_eq!((job.id, job.kind), (replayed.id, JobKind::Index));
    for filler in 1..64u8 {
        let filled = request(prepare(
            caller,
            key(id(0x40 + filler), epoch),
            JobKind::Inspect,
        ));
        assert!(
            matches!(
                filled,
                Response::ClientRequestPrepared {
                    duplicate: false,
                    ..
                }
            ),
            "{filler}"
        );
    }
    assert_eq!(
        code(request(prepare(
            caller,
            key(id(0x3f), epoch),
            JobKind::Inspect
        ))),
        "client_request_journal_full"
    );
    assert!(matches!(
        request(Method::ClientRequestForget { nonce }),
        Response::ClientRequestForgotten { existed: true, .. }
    ));
    assert!(matches!(
        request(Method::ClientRequestForget { nonce }),
        Response::ClientRequestForgotten { existed: false, .. }
    ));
    Ok(())
}

/// `airkctl index|inspect` and `service requests …` run the journal as
/// `airk` did (split step 3c.3c): the same job kind and project, the same
/// guidance line (nonce normalized, `airk` → `airkctl`), the same listing.
#[test]
fn airkctl_jobs_and_requests_match_airk() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-jobs");
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let control = service.control.to_str().unwrap().to_owned();
    let binding = service.project.binding.to_string();
    let ctl = |arguments: &[&str]| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = std::iter::once("airkctl")
            .chain(arguments.iter().copied())
            .map(std::ffi::OsString::from)
            .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let mut parity = Parity::new("airkctl_jobs_and_requests_match_airk");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    // A saved request's caller and key carry this run's journal namespace and
    // epoch (the recorded run also spent epochs on its own `airk` jobs).
    parity.mask_json_fields(&["caller", "namespace", "epoch"]);
    for command in ["inspect", "index"] {
        let old = parity.old(command, || {
            common::legacy_airk_command(root.path(), state.path(), "ctl-jobs")
                .arg(command)
                .output()
                .unwrap()
        });
        assert!(old.success, "{}", old.stderr);
        let (code, stdout, stderr) = ctl(&[
            "--service-runtime-dir",
            &control,
            "--service-project",
            &binding,
            command,
        ]);
        assert_eq!(code, 0, "{stderr}");
        let old_job: Value = serde_json::from_str(&old.stdout)?;
        let new_job: Value = serde_json::from_str(&parity.canonical(&stdout))?;
        assert_eq!(new_job["status"], "job");
        assert_eq!(
            (&new_job["job"]["project"], &new_job["job"]["kind"]),
            (&old_job["job"]["project"], &old_job["job"]["kind"]),
            "{command}"
        );
        // The request nonce is a renamed hex token in canonical space.
        let old_line = old
            .stderr
            .lines()
            .last()
            .unwrap_or_default()
            .replace("'airk service", "'airkctl service");
        assert_eq!(
            parity.canonical(&stderr).lines().last().unwrap_or_default(),
            old_line,
            "{command}"
        );
    }
    let (code, prepared, stderr) = ctl(&[
        "service",
        "requests",
        "--runtime-dir",
        &control,
        "prepare",
        "--project",
        &binding,
        "--kind",
        "inspect",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let prepared: Value = serde_json::from_str(&prepared)?;
    assert_eq!(prepared["submitted"], false);
    let saved = prepared["prepared"].as_str().unwrap().to_owned();
    parity.substitute(&saved, "<PREPARED>");
    let old_list = parity.old("requests list", || {
        command_in(
            &control,
            &["service", "requests", "--runtime-dir", &control, "list"],
        )
    });
    assert!(old_list.success, "{}", old_list.stderr);
    let old_list = old_list.stdout;
    let (code, new_list, _) = ctl(&["service", "requests", "--runtime-dir", &control, "list"]);
    assert_eq!(
        (code, parity.canonical(&new_list).as_str()),
        (0, old_list.as_str())
    );
    assert!(new_list.contains(&saved), "{new_list}");
    let (code, retried, stderr) = ctl(&[
        "service",
        "requests",
        "--runtime-dir",
        &control,
        "retry",
        &saved,
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        serde_json::from_str::<Value>(&retried)?["job"]["kind"],
        "inspect"
    );
    let (code, forgotten, _) = ctl(&[
        "service",
        "requests",
        "--runtime-dir",
        &control,
        "forget",
        &saved,
    ]);
    assert_eq!(code, 0);
    assert_eq!(
        serde_json::from_str::<Value>(&forgotten)?,
        json!({"forgotten": saved, "native_work_canceled": false})
    );
    let (_, emptied, _) = ctl(&["service", "requests", "--runtime-dir", &control, "list"]);
    assert_eq!(emptied.trim(), "[]");
    parity.finish();
    Ok(())
}

/// `airk` with only these arguments.
fn command_in(_control: &str, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_airk"))
        .args(arguments)
        .output()
        .unwrap()
}

/// The last stderr line without its program prefix (`Error: ` for `airk`,
/// `airkctl: ` for `airkctl`): the message both print for a refusal.
fn refusal_message(stderr: &[u8], prefix: &str) -> String {
    let text = String::from_utf8_lossy(stderr);
    let line = text.lines().last().unwrap_or_default();
    line.strip_prefix(prefix).unwrap_or(line).to_owned()
}

/// `init`, `service register`, `service prepare-registration` and `service
/// prepare-storage` (split step 3c.4b): `airkctl` prints what `airk` printed,
/// through M4a/M4b and the existing `ReserveProject` and `Register`.
#[test]
fn airkctl_registration_matches_airk() -> anyhow::Result<()> {
    use airkctl::registration::Init;
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        source.path().join("original.md"),
        b"# Registration parity\n",
    )?;
    let service = common::Service::reviewed(source.path(), data.path(), "ctl-init");
    let control = service.control.to_str().unwrap().to_owned();
    let version = env!("CARGO_PKG_VERSION");
    let ctl = |arguments: &[&str]| -> (u8, String, Vec<u8>) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = std::iter::once("airkctl")
            .chain(arguments.iter().copied())
            .map(std::ffi::OsString::from)
            .collect();
        let code = airkctl::cli::run(line, &mut io);
        (code, String::from_utf8(stdout).unwrap(), stderr)
    };
    let old = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_airk"))
            .args(arguments)
            .output()
            .unwrap()
    };
    let unreviewed = tempfile::tempdir()?;
    let absent = tempfile::tempdir()?;
    let fresh = tempfile::tempdir()?;
    let fresh_parent = tempfile::tempdir()?;
    let other = tempfile::tempdir()?;
    let other_data = tempfile::tempdir()?;
    let mut parity = Parity::new("airkctl_registration_matches_airk");
    parity.substitute_path(source.path(), "<SOURCE>");
    parity.substitute_path(data.path(), "<DATA>");
    parity.substitute_path(unreviewed.path(), "<UNREVIEWED>");
    parity.substitute_path(absent.path(), "<ABSENT>");
    parity.substitute_path(fresh.path(), "<FRESH>");
    parity.substitute_path(fresh_parent.path(), "<FRESH_PARENT>");
    parity.substitute_path(other.path(), "<OTHER>");
    parity.substitute_path(other_data.path(), "<OTHER_DATA>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);

    // init on the reviewed mapping: the same acknowledged project.
    let initialized = parity.old("init reviewed", || {
        service
            .installed_command(source.path(), data.path(), "ctl-init")
            .arg("init")
            .output()
            .unwrap()
    });
    assert!(initialized.success, "{}", initialized.stderr);
    let Init::Registered(project) = airkctl::registration::init(
        &service.control,
        version,
        source.path(),
        data.path(),
        "ctl-init",
        None,
    )
    .map_err(|line| anyhow::anyhow!(line.message))?
    else {
        anyhow::bail!("airkctl init was not acknowledged");
    };
    assert_eq!(
        parity.canonical(&(serde_json::to_string(&project)? + "\n")),
        initialized.stdout
    );
    assert_eq!(project.registration, service.project);

    // init on an unmapped folder: the same refusal; nothing is created.
    let absent_data = absent.path().join("not-created/nested");
    fs::write(unreviewed.path().join("original.md"), b"# Unreviewed\n")?;
    let rejected = parity.old("init unmapped", || {
        service
            .installed_command(unreviewed.path(), &absent_data, "ctl-unmapped")
            .arg("init")
            .output()
            .unwrap()
    });
    assert!(!rejected.success);
    match airkctl::registration::init(
        &service.control,
        version,
        unreviewed.path(),
        &absent_data,
        "ctl-unmapped",
        None,
    ) {
        Ok(Init::NotAcknowledged(response)) => assert_eq!(
            parity.canonical(&(serde_json::to_string(&response)? + "\n")),
            rejected.stdout
        ),
        Ok(Init::Registered(_)) => anyhow::bail!("airkctl init registered an unmapped folder"),
        Err(line) => assert_eq!(
            parity.canonical(&line.message),
            refusal_message(rejected.stderr.as_bytes(), "Error: ")
        ),
    }
    assert!(!absent.path().join("not-created").exists());

    // prepare-registration: offline in `airk`, M4a in `airkctl`.
    let planned = fresh_parent.path().join("planned");
    let inner = fresh.path().join("inner");
    for (case, data_dir, collection) in [
        ("planned", Some(planned.to_str().unwrap()), "ctl-plan"),
        ("inner", Some(inner.to_str().unwrap()), "ctl-plan"),
        ("default", None, "ctl-plan"),
    ] {
        let mut arguments = vec!["--root", fresh.path().to_str().unwrap()];
        if let Some(data_dir) = data_dir {
            arguments.extend(["--data-dir", data_dir]);
        }
        arguments.extend([
            "--collection",
            collection,
            "service",
            "prepare-registration",
        ]);
        let before = parity.old(&format!("prepare-registration {case}"), || old(&arguments));
        let (code, stdout, stderr) = ctl(&[
            &["--service-runtime-dir", control.as_str()][..],
            &arguments[..],
        ]
        .concat());
        assert_eq!(
            (code == 0, parity.canonical(&stdout)),
            (before.success, before.stdout),
            "{arguments:?}"
        );
        if code != 0 {
            assert_eq!(
                refusal_message(
                    parity
                        .canonical(&String::from_utf8_lossy(&stderr))
                        .as_bytes(),
                    "airkctl: "
                ),
                refusal_message(before.stderr.as_bytes(), "Error: "),
                "{arguments:?}"
            );
        }
    }
    assert!(!planned.exists());

    // register: storage must exist; the service's answer is printed alike.
    let register = [
        "--root",
        other.path().to_str().unwrap(),
        "--data-dir",
        other_data.path().to_str().unwrap(),
        "--collection",
        "ctl-register",
        "service",
        "register",
        "--runtime-dir",
        &control,
    ];
    let before = parity.old("register", || old(&register));
    let (code, stdout, stderr) = ctl(&register);
    assert_eq!(
        (code == 0, parity.canonical(&stdout)),
        (before.success, before.stdout),
        "{}",
        String::from_utf8_lossy(&stderr)
    );

    // prepare-storage: the registry's registration, or the same refusal.
    let unknown = airkctl::registration::plan(
        &service.control,
        version,
        fresh.path(),
        &planned,
        "ctl-plan",
        airk_core::service::RegistrationStorage::Planned,
    )
    .map_err(|line| anyhow::anyhow!(line.message))?;
    parity.substitute(unknown.binding, "<PLANNED_BINDING>");
    for (case, binding) in [
        ("reviewed", service.project.binding),
        ("planned", unknown.binding),
    ] {
        let binding = binding.to_string();
        let arguments = [
            "service",
            "prepare-storage",
            "--runtime-dir",
            &control,
            "--project",
            &binding,
        ];
        let before = parity.old(&format!("prepare-storage {case}"), || old(&arguments));
        let (code, stdout, stderr) = ctl(&arguments);
        assert_eq!(
            (code == 0, parity.canonical(&stdout)),
            (before.success, before.stdout),
            "{binding}"
        );
        if code != 0 {
            assert_eq!(
                refusal_message(
                    parity
                        .canonical(&String::from_utf8_lossy(&stderr))
                        .as_bytes(),
                    "airkctl: "
                ),
                refusal_message(before.stderr.as_bytes(), "Error: "),
                "{binding}"
            );
        }
    }
    assert!(!planned.exists());
    service.shutdown_reviewed()?;
    parity.finish();
    Ok(())
}

/// A console transcript with the varying observation fields `stable` drops
/// replaced in place (pretty JSON keeps one field per line).
fn stable_transcript(text: &str) -> String {
    const VARYING: [&str; 7] = [
        "request_id",
        "observed_unix_ms",
        "observed_from_unix_ms",
        "observed_to_unix_ms",
        "cache_age_ms",
        "cached",
        "elapsed_ms",
    ];
    text.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            match VARYING
                .iter()
                .find(|key| trimmed.starts_with(&format!("\"{key}\":")))
            {
                Some(key) => format!(
                    "{}\"{key}\": _{}",
                    &line[..line.len() - trimmed.len()],
                    if line.ends_with(',') { "," } else { "" }
                ),
                None => line.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `tools shell` (split step 3c.4c): the same console input through `airk`
/// and `airkctl`, by project binding and by catalog session, prints the same
/// banner, help, prompts, results and `Error:` lines.
#[test]
fn airkctl_tools_shell_matches_airk() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-shell");
    fs::write(
        root.path().join("recovery.md"),
        "# Recovery\n\nRecover a forgotten password.\n",
    )?;
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let arguments_file = state.path().join("shell-args.json");
    fs::write(&arguments_file, r#"{"query":"recover","limit":2}"#)?;
    let script = format!(
        "list\nhelp airk_search\nhelp nope\ncall airk_status\ncall airk_search {{\"query\":\"recover\",\"limit\":2}}\ncall airk_search --args-file \"{}\"\ncall airk_search [1]\ncall nope\nbogus\n\nhelp\nexit\ncall airk_status\n",
        arguments_file.display()
    );
    let airkctl_shell = |arguments: &[&str]| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut stdin = script.as_bytes();
        // `common::command` and `scoped_command` run `airk` with
        // `AIRK_MCP_RESPONSE_MODE=full`; `airkctl` reads the same setting.
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut stdin,
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: true,
            window: None,
        };
        let line = std::iter::once("airkctl")
            .chain(arguments.iter().copied())
            .chain(["tools", "shell"])
            .map(std::ffi::OsString::from)
            .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let airk_shell = |mut command: Command| -> Output {
        let mut child = command
            .args(["tools", "shell"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let airk_core::service_http::HttpSelection::Scoped { selection } = service.http_selection()
    else {
        panic!("the reviewed fixture selects one scoped index");
    };
    let catalog = serde_json::to_string(&selection)?;
    let binding = service.project.binding.to_string();
    let control = service.control.to_str().unwrap().to_owned();
    let mut parity = Parity::new("airkctl_tools_shell_matches_airk");
    parity.substitute_path(&arguments_file, "<ARGS_FILE>");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    for (name, old, new) in [
        (
            "project",
            common::legacy_airk_command(root.path(), state.path(), "ctl-shell"),
            vec![
                "--service-runtime-dir",
                &control,
                "--service-project",
                &binding,
            ],
        ),
        (
            "catalog",
            common::legacy_airk_scoped_command(root.path(), state.path(), "ctl-shell"),
            vec![
                "--service-runtime-dir",
                &control,
                "--catalog-session",
                &catalog,
            ],
        ),
    ] {
        let recorded = parity.old(name, || airk_shell(old));
        // The documented program rename (`airk tools …` → `airkctl tools …`),
        // as the golden comparison applies it.
        let (succeeded, old_stdout, old_stderr) = (
            recorded.success,
            recorded.stdout.replace("airk tools ", "airkctl tools "),
            recorded
                .stderr
                .replace("'airk tools list'", "'airkctl tools list'"),
        );
        let (code, new_stdout, new_stderr) = airkctl_shell(&new);
        let (new_stdout, new_stderr) =
            (parity.canonical(&new_stdout), parity.canonical(&new_stderr));
        assert!(succeeded, "{name}: {old_stderr}");
        assert_eq!(code, 0, "{name}: {new_stderr}");
        assert!(
            old_stdout.starts_with("AIRK Tools\n"),
            "{name}: {old_stdout}"
        );
        assert_eq!(
            stable_transcript(&new_stdout),
            stable_transcript(&old_stdout),
            "{name}"
        );
        assert_eq!(new_stderr, old_stderr, "{name}");
        assert!(
            new_stderr.contains("Error: Unknown command. Type help for instructions."),
            "{name}: {new_stderr}"
        );
    }
    parity.finish();
    Ok(())
}

/// `configure`, the `usage` mutations, `usage import --preview` and both
/// `prepare-configuration` commands (split step 3c.5b): `airkctl` prints what
/// `airk` printed through M9 and the journal (jobs compared by project and
/// kind; nonces normalized).
#[test]
fn airkctl_configuration_matches_airk() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-config");
    // Configuration requires the project's first inspection: wait for the
    // fixture's initial Index work (observed, deadline-bounded) before the
    // first prepare-configuration.
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let control = service.control.to_str().unwrap().to_owned();
    let binding = service.project.binding.to_string();
    let ctl = |arguments: &[&str], stdin: &str| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut input = stdin.as_bytes();
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut input,
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: true,
            window: None,
        };
        let line = std::iter::once("airkctl")
            .chain([
                "--service-runtime-dir",
                &control,
                "--service-project",
                &binding,
            ])
            .chain(arguments.iter().copied())
            .map(std::ffi::OsString::from)
            .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let measurement = state.path().join("measurement.json");
    let draft_file = state.path().join("draft.json");
    let mut parity = Parity::new("airkctl_configuration_matches_airk");
    parity.substitute_path(&measurement, "<MEASUREMENT>");
    parity.substitute_path(&draft_file, "<DRAFT_FILE>");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    // The recorded `airk` run (split U95): new output is masked alike.
    let old = |parity: &mut Parity,
               case: &str,
               arguments: &[&str],
               stdin: &str|
     -> anyhow::Result<(bool, String, String)> {
        let recorded = parity.old(case, || {
            let mut child = common::legacy_airk_command(root.path(), state.path(), "ctl-config")
                .args(arguments)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin.as_bytes())
                .unwrap();
            child.wait_with_output().unwrap()
        });
        Ok((recorded.success, recorded.stdout, recorded.stderr))
    };
    let message = |stderr: &str, prefix: &str| {
        let line = stderr.lines().last().unwrap_or_default();
        line.strip_prefix(prefix).unwrap_or(line).to_owned()
    };

    // A prepared request is a read: the same request for the same state.
    let draft = r#"{"operation":"vectors","enabled":false}"#;
    let prepare = [
        "service",
        "prepare-configuration",
        "--runtime-dir",
        &control,
        "--project",
        &binding,
    ];
    let (succeeded, before, stderr) = old(&mut parity, "prepare-configuration", &prepare, draft)?;
    assert!(succeeded, "{stderr}");
    let (code, after, stderr) = ctl(&prepare, draft);
    assert_eq!(
        (code, parity.canonical(&after).as_str()),
        (0, before.as_str()),
        "{stderr}"
    );

    // Refusals print the same message: a malformed draft, an unknown mode
    // value, a measurement that does not parse.
    fs::write(&measurement, "{}")?;
    let measurement = measurement.to_str().unwrap().to_owned();
    for (case, arguments, stdin) in [
        (
            "configure bogus",
            vec!["configure", "--stdin"],
            "{\"operation\":\"bogus\"}",
        ),
        (
            "usage import preview",
            vec!["usage", "import", &measurement, "--preview"],
            "",
        ),
    ] {
        let (succeeded, stdout, old_stderr) = old(&mut parity, case, &arguments, stdin)?;
        let (code, new_stdout, new_stderr) = ctl(&arguments, stdin);
        assert!(!succeeded && code != 0, "{arguments:?}");
        assert_eq!(parity.canonical(&new_stdout), stdout, "{arguments:?}");
        assert_eq!(
            message(&parity.canonical(&new_stderr), "airkctl: "),
            message(&old_stderr, "Error: "),
            "{arguments:?}"
        );
    }

    // Mutations: the same job, and the same saved-request line.
    fs::write(&draft_file, draft)?;
    let draft_file = draft_file.to_str().unwrap().to_owned();
    for (case, arguments) in [
        ("configure file", vec!["configure", "--file", &draft_file]),
        ("usage clear", vec!["usage", "clear"]),
        (
            "usage set aggregates_only",
            vec!["usage", "set", "--mode", "aggregates_only"],
        ),
    ] {
        let (succeeded, stdout, old_stderr) = old(&mut parity, case, &arguments, "")?;
        let (code, new_stdout, new_stderr) = ctl(&arguments, "");
        assert!(succeeded, "{arguments:?}: {old_stderr}");
        assert_eq!(code, 0, "{arguments:?}: {new_stderr}");
        let (before, after): (Value, Value) = (
            serde_json::from_str(&stdout)?,
            serde_json::from_str(&parity.canonical(&new_stdout))?,
        );
        assert_eq!(after["status"], "job", "{arguments:?}");
        assert_eq!(
            (&after["job"]["project"], &after["job"]["kind"]),
            (&before["job"]["project"], &before["job"]["kind"]),
            "{arguments:?}"
        );
        assert_eq!(
            regex_free_configuration_nonce(&parity.canonical(&new_stderr)),
            regex_free_configuration_nonce(&old_stderr),
            "{arguments:?}"
        );
    }

    // A saved configuration request: prepared, listed, forgotten.
    let saved = [
        "service",
        "requests",
        "--runtime-dir",
        &control,
        "prepare-configuration",
        "--project",
        &binding,
        "--file",
        &draft_file,
    ];
    let (code, prepared, stderr) = ctl(&saved, "");
    assert_eq!(code, 0, "{stderr}");
    let prepared: Value = serde_json::from_str(&prepared)?;
    assert_eq!(prepared["submitted"], false);
    let nonce = prepared["prepared"].as_str().unwrap().to_owned();
    let (_, listed, _) = ctl(
        &["service", "requests", "--runtime-dir", &control, "list"],
        "",
    );
    assert!(
        listed.contains(&nonce) && listed.contains("\"kind\":\"configure\""),
        "{listed}"
    );
    let (code, _, _) = ctl(
        &[
            "service",
            "requests",
            "--runtime-dir",
            &control,
            "forget",
            &nonce,
        ],
        "",
    );
    assert_eq!(code, 0);
    parity.finish();
    Ok(())
}

/// The saved-configuration-request lines with their nonces replaced.
fn regex_free_configuration_nonce(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|line| line.starts_with("Saved CLI configuration request "))
        .map(|line| {
            let rest = &line["Saved CLI configuration request ".len()..];
            let end = rest.find(';').unwrap_or(rest.len());
            format!("Saved CLI configuration request <nonce>{}", &rest[end..])
        })
        .collect()
}

/// `settings` and `usage report|policy` (split step 3c.5c): the document
/// `airk` printed, by project binding and by catalog session (N1). JSON
/// documents compare after `stable`; text reports compare as printed.
#[test]
fn airkctl_usage_documents_match_airk() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-usage");
    // Readiness fields move while the first index settles; compare after it.
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + std::time::Duration::from_secs(90),
    )?;
    let airk_core::service_http::HttpSelection::Scoped { selection } = service.http_selection()
    else {
        panic!("the reviewed fixture selects one scoped index");
    };
    let catalog = serde_json::to_string(&selection)?;
    let binding = service.project.binding.to_string();
    let control = service.control.to_str().unwrap().to_owned();
    let ctl = |selection: &[&str], arguments: &[&str]| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: true,
            window: None,
        };
        let line = std::iter::once("airkctl")
            .chain(selection.iter().copied())
            .chain(arguments.iter().copied())
            .map(std::ffi::OsString::from)
            .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let comparable_text = |text: &str| match serde_json::from_str::<Value>(text) {
        Ok(mut value) => {
            stable(&mut value);
            value.to_string()
        }
        Err(_) => text.to_owned(),
    };
    let reads: Vec<Vec<&str>> = vec![
        vec!["settings", "roots"],
        vec!["settings", "vector_status"],
        vec!["usage", "policy"],
        vec!["usage", "report"],
        vec!["usage", "report", "--format", "markdown"],
        vec!["usage", "report", "--days", "0"],
    ];
    let mut parity = Parity::new("airkctl_usage_documents_match_airk");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    // Measured resource figures (timings, memory, OS counters) vary per run.
    parity.mask_json_fields(&[
        "*_us",
        "peak_rss_bytes",
        "value",
        "*disk_growth_bytes",
        "returned_utf8_bytes",
        "measured_disk_bytes",
    ]);
    parity.mask_text("measured_usage_text", mask_measured_usage_text);
    for read in &reads {
        for (name, old, selection) in [
            (
                "project",
                common::legacy_airk_command(root.path(), state.path(), "ctl-usage"),
                vec![
                    "--service-runtime-dir",
                    control.as_str(),
                    "--service-project",
                    binding.as_str(),
                ],
            ),
            (
                "catalog",
                common::legacy_airk_scoped_command(root.path(), state.path(), "ctl-usage"),
                vec![
                    "--service-runtime-dir",
                    control.as_str(),
                    "--catalog-session",
                    catalog.as_str(),
                ],
            ),
        ] {
            let mut old = old;
            let before = parity.old(&format!("{name} {}", read.join(" ")), || {
                old.args(read).output().unwrap()
            });
            let (code, stdout, stderr) = ctl(&selection, read);
            assert_eq!(code == 0, before.success, "{name} {read:?}: {stderr}");
            assert_eq!(
                comparable_text(&parity.canonical(&stdout)),
                comparable_text(&before.stdout),
                "{name} {read:?}"
            );
            if code != 0 {
                assert_eq!(
                    parity
                        .canonical(&stderr)
                        .lines()
                        .last()
                        .unwrap_or_default()
                        .strip_prefix("airkctl: "),
                    before
                        .stderr
                        .lines()
                        .last()
                        .unwrap_or_default()
                        .strip_prefix("Error: "),
                    "{name} {read:?}"
                );
            }
        }
    }
    parity.finish();
    Ok(())
}

/// `model-catalog` (split step 3c.5d): `airkctl` prints the report `airk`
/// printed, from the service (M10).
#[test]
fn airkctl_model_catalog_matches_airk() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-models");
    let control = service.control.to_str().unwrap().to_owned();
    let mut parity = Parity::new("airkctl_model_catalog_matches_airk");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    let before = parity.old("model-catalog", || {
        Command::new(env!("CARGO_BIN_EXE_airk"))
            .arg("model-catalog")
            .output()
            .unwrap()
    });
    assert!(before.success);
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut io = airkctl::cli::Io {
        read_attempt: None,
        stdin: &mut std::io::empty(),
        stdout: &mut stdout,
        stderr: &mut stderr,
        full_responses: false,
        window: None,
    };
    let line = [
        "airkctl",
        "--service-runtime-dir",
        &control,
        "model-catalog",
    ]
    .map(std::ffi::OsString::from)
    .to_vec();
    let code = airkctl::cli::run(line, &mut io);
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
    assert_eq!(parity.canonical(&String::from_utf8(stdout)?), before.stdout);
    parity.finish();
    Ok(())
}

/// `explorer …` (split step 3d): the same operation through `airk` (into
/// `--output`) and `airkctl` (into `--output-file`, U79, through M11) writes
/// the same result and prints the same `{written, ok}` line, apart from the
/// file name and the drafts' clock fields.
#[test]
fn airkctl_explorer_matches_airk() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-explorer");
    let control = service.control.to_str().unwrap().to_owned();
    let out = tempfile::tempdir()?;
    let ctl = |arguments: &[&str]| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = ["airkctl", "--service-runtime-dir", control.as_str()]
            .into_iter()
            .chain(arguments.iter().copied())
            .map(std::ffi::OsString::from)
            .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    // Drafts carry the clock that wrote them.
    fn timeless(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                fields.retain(|key, _| !key.ends_with("_ms") && !key.ends_with("_at"));
                fields.values_mut().for_each(timeless);
            }
            Value::Array(rows) => rows.iter_mut().for_each(timeless),
            _ => {}
        }
    }
    // A written result (canonical text) without its clock fields.
    fn read(text: &str) -> anyhow::Result<Value> {
        let mut value: Value = serde_json::from_str(text)?;
        timeless(&mut value);
        Ok(value)
    }
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("list-templates", vec![]),
        (
            "get-template",
            vec!["--args", r#"{"id":"no-such-template"}"#],
        ),
        (
            "draft-search",
            vec!["--args", r#"{"query":"recover a password"}"#],
        ),
        ("draft-search", vec!["--args", r#"{"query":1}"#]),
        (
            "validate-draft",
            vec!["--args", r#"{"kind":"search","draft":{}}"#],
        ),
        ("draft-plan", vec!["--args", "[1]"]),
    ];
    let mut parity = Parity::new("airkctl_explorer_matches_airk");
    parity.mask_json_fields(CLOCK_FIELDS);
    parity.substitute_path(out.path(), "<OUT>");
    parity.substitute_path(root.path(), "<ROOT>");
    parity.substitute_path(state.path(), "<STATE>");
    parity.service(&service);
    for (index, (command, extra)) in cases.iter().enumerate() {
        let old_path = out.path().join(format!("old-{index}.json"));
        let new_path = out.path().join(format!("new-{index}.json"));
        let before = parity.old(&format!("{index} {command}"), || {
            Command::new(env!("CARGO_BIN_EXE_airk"))
                .args(["explorer", command])
                .args(extra)
                .arg("--output")
                .arg(&old_path)
                .output()
                .unwrap()
        });
        let written = parity.old_file(&format!("{index} {command} output"), &old_path);
        let mut line = vec!["explorer", command];
        line.extend(extra.iter().copied());
        line.extend(["--output-file", new_path.to_str().unwrap()]);
        let (code, stdout, stderr) = ctl(&line);
        assert_eq!(code == 0, before.success, "{command} {extra:?}: {stderr}");
        assert_eq!(
            parity.canonical(&stdout.replace(&format!("new-{index}"), &format!("old-{index}"))),
            before.stdout,
            "{command} {extra:?}"
        );
        assert_eq!(written.success, new_path.exists(), "{command} {extra:?}");
        if new_path.exists() {
            assert_eq!(
                read(&parity.canonical(&fs::read_to_string(&new_path)?))?,
                read(&written.stdout)?,
                "{command} {extra:?}"
            );
        } else {
            assert_eq!(
                parity
                    .canonical(&stderr)
                    .lines()
                    .last()
                    .unwrap_or_default()
                    .strip_prefix("airkctl: "),
                before
                    .stderr
                    .lines()
                    .last()
                    .unwrap_or_default()
                    .strip_prefix("Error: "),
                "{command} {extra:?}"
            );
        }
    }
    // proposal-status reads a written draft: the one `airk` wrote while
    // capturing; in replay (no `airk` run) the one airkctl wrote.
    let draft = out.path().join(if parity.capturing() {
        "old-2.json"
    } else {
        "new-2.json"
    });
    let old_status = out.path().join("old-status.json");
    let new_status = out.path().join("new-status.json");
    let before = parity.old("proposal-status", || {
        Command::new(env!("CARGO_BIN_EXE_airk"))
            .args(["explorer", "proposal-status", "--draft"])
            .arg(&draft)
            .arg("--output")
            .arg(&old_status)
            .output()
            .unwrap()
    });
    let old_status = parity.old_file("proposal-status output", &old_status);
    let (code, _, stderr) = ctl(&[
        "explorer",
        "proposal-status",
        "--draft",
        draft.to_str().unwrap(),
        "--output-file",
        new_status.to_str().unwrap(),
    ]);
    assert_eq!(code == 0, before.success, "{stderr}");
    let status = |parity: &Parity| -> anyhow::Result<Value> {
        read(&parity.canonical(&fs::read_to_string(&new_status)?))
    };
    assert_eq!(status(&parity)?, read(&old_status.stdout)?);
    // An existing output file is refused alike and left unchanged.
    let (code, _, _) = ctl(&[
        "explorer",
        "list-templates",
        "--output-file",
        new_status.to_str().unwrap(),
    ]);
    assert_ne!(code, 0);
    assert_eq!(status(&parity)?, read(&old_status.stdout)?);
    parity.finish();
    Ok(())
}

/// `agents endpoint`, `agents info` and `agents revoke` (split step 3e.2):
/// `airkctl` prints what `airk runtime http-endpoint`, `http-connection-info`
/// and `http-revoke` printed for the same installed service and connection.
#[test]
fn airkctl_agent_connections_match_airk() -> anyhow::Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let service = common::Service::reviewed(source.path(), data.path(), "ctl-agents");
    let control = service.control.to_str().unwrap().to_owned();
    let inputs = tempfile::tempdir()?;
    let private = inputs.path().join("private");
    airk_cli::local_ipc::prepare_root(&private)?;
    let selection = private.join("selection.json");
    fs::write(&selection, serde_json::to_vec(&service.http_selection())?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&selection, fs::Permissions::from_mode(0o600))?;
    }
    let runtime = |role: &str, extra: &[&str]| -> anyhow::Result<std::process::Output> {
        Ok(service
            .installed_command(source.path(), data.path(), "ctl-agents")
            .args(["runtime", role, "--install-dir"])
            .arg(service.install_dir())
            .args(extra)
            .output()?)
    };
    let ctl = |arguments: &[&str]| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = [
            "airkctl",
            "--service-runtime-dir",
            control.as_str(),
            "agents",
        ]
        .into_iter()
        .chain(arguments.iter().copied())
        .map(std::ffi::OsString::from)
        .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let same =
        |parity: &Parity, name: &str, old: common::parity::Recorded, new: (u8, String, String)| {
            assert_eq!(new.0 == 0, old.success, "{name}: {}", new.2);
            assert_eq!(parity.canonical(&new.1), old.stdout, "{name}");
            if new.0 != 0 {
                assert_eq!(
                    parity
                        .canonical(&new.2)
                        .lines()
                        .last()
                        .unwrap_or_default()
                        .strip_prefix("airkctl: "),
                    old.stderr
                        .lines()
                        .last()
                        .unwrap_or_default()
                        .strip_prefix("Error: "),
                    "{name}"
                );
            }
        };
    let id = airk_kernel::service_broker::new_instance_id()?.to_string();
    let unknown = airk_kernel::service_broker::new_instance_id()?.to_string();
    let mut parity = Parity::new("airkctl_agent_connections_match_airk");
    parity.substitute(&id, "<CONNECTION>");
    parity.substitute(&unknown, "<UNKNOWN_CONNECTION>");
    parity.substitute_path(source.path(), "<SOURCE>");
    parity.substitute_path(data.path(), "<DATA>");
    parity.substitute_path(inputs.path(), "<INPUTS>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    // The bearer token is a secret and differs per run; `user` names the
    // account that captured the fixture.
    parity.mask_json_fields(&["Authorization", "user"]);
    // The endpoint's identities and loopback URL are this run's.
    let endpoint = ctl(&["endpoint"]);
    if let Ok(value) = serde_json::from_str::<Value>(&endpoint.1) {
        parity.substitute_identifiers(&value, "ENDPOINT");
    }
    let old = parity.old("endpoint", || runtime("http-endpoint", &[]).unwrap());
    same(&parity, "endpoint", old, endpoint);
    let old = parity.old("info of an unknown connection", || {
        runtime("http-connection-info", &["--connection-id", &unknown]).unwrap()
    });
    same(
        &parity,
        "info of an unknown connection",
        old,
        ctl(&["info", "--connection-id", &unknown]),
    );
    let enrolled = parity.old("enroll", || {
        runtime(
            "http-connection",
            &[
                "--selection-file",
                selection.to_str().unwrap(),
                "--connection-id",
                &id,
                "--client",
                "codex",
            ],
        )
        .unwrap()
    });
    assert!(enrolled.success, "{}", enrolled.stderr);
    if !parity.capturing() {
        // The recorded run enrolled through `airk runtime http-connection`;
        // this run makes the same enrollment through its control method.
        enroll_codex_connection(&service, &id)?;
    }
    let old = parity.old("info", || {
        runtime("http-connection-info", &["--connection-id", &id]).unwrap()
    });
    same(&parity, "info", old, ctl(&["info", "--connection-id", &id]));
    let (code, stdout, stderr) = ctl(&["info", "--connection-id", &id]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        serde_json::from_str::<Value>(&stdout)?
            .get("token")
            .is_none(),
        "{stdout}"
    );
    // The offline header helper (3e.3): same header, and the same refusals
    // for an unknown connection and, below, a revoked one.
    let credential = |parity: &mut Parity, case: &str, id: &str| -> anyhow::Result<_> {
        Ok((
            parity.old(case, || {
                runtime(
                    "http-credential",
                    &["--connection-id", id, "--format", "header-json"],
                )
                .unwrap()
            }),
            ctl(&[
                "credential",
                "--connection-id",
                id,
                "--format",
                "header-json",
            ]),
        ))
    };
    let (old, new) = credential(&mut parity, "credential", &id)?;
    assert!(
        new.1.contains("\"Authorization\":\"Bearer airk1."),
        "{}",
        new.2
    );
    same(&parity, "credential", old, new);
    let (old, new) = credential(&mut parity, "credential of an unknown connection", &unknown)?;
    same(&parity, "credential of an unknown connection", old, new);
    // M16 (3e.4): `agents list --connection-id` answers what
    // `lookup-http-connection` answers, without its selected-runtime field.
    let airk_core::service::ControlResponse::Health {
        observation: health,
    } = service.request(airk_core::service::ControlMethod::Health)
    else {
        anyhow::bail!("missing service health");
    };
    let active = health.runtime_sha256.clone().unwrap_or_default();
    parity.substitute(&active, "<ACTIVE>");
    for (name, target) in [("owned", id.as_str()), ("not owned", unknown.as_str())] {
        let old = parity.old(&format!("lookup {name}"), || {
            runtime(
                "lookup-http-connection",
                &["--expected-active", &active, "--connection-id", target],
            )
            .unwrap()
        });
        assert!(old.success, "{}", old.stderr);
        let mut old: Value = serde_json::from_str(&old.stdout)?;
        // The run's own active runtime digest, in canonical space.
        assert_eq!(old["active"], parity.canonical(&active).as_str());
        old.as_object_mut().unwrap().remove("active");
        let (code, new, stderr) = ctl(&["list", "--connection-id", target]);
        assert_eq!(code, 0, "{name}: {stderr}");
        assert_eq!(
            serde_json::from_str::<Value>(&parity.canonical(&new))?,
            old,
            "{name}"
        );
    }
    // V1 (3e.9): the service verifies its own MCP endpoint with the stored
    // credential, and no token reaches the output.
    let (code, verified, stderr) = ctl(&["verify", "--connection-id", &id]);
    assert_eq!(code, 0, "{stderr}");
    let verified: Value = serde_json::from_str(&verified)?;
    assert_eq!(verified["verified"], true, "{verified}");
    assert_eq!(verified["http_status"], 200);
    assert!(!verified.to_string().contains("airk1."), "{verified}");
    // Revoke through airkctl, then airk sees the same revoked connection.
    let (code, revoked, stderr) = ctl(&["revoke", "--connection-id", &id]);
    assert_eq!(code, 0, "{stderr}");
    let revoked: Value = serde_json::from_str(&revoked)?;
    assert_eq!(revoked["status"], "http_revoked");
    assert_eq!(revoked["connection_id"], id);
    let old = parity.old("info after revoke", || {
        runtime("http-connection-info", &["--connection-id", &id]).unwrap()
    });
    same(
        &parity,
        "info after revoke",
        old,
        ctl(&["info", "--connection-id", &id]),
    );
    let old = parity.old("revoke again", || {
        runtime("http-revoke", &["--connection-id", &id]).unwrap()
    });
    same(
        &parity,
        "revoke again",
        old,
        ctl(&["revoke", "--connection-id", &id]),
    );
    // A revoked connection is owned-but-unavailable and does not verify.
    let (code, after, stderr) = ctl(&["list", "--connection-id", &id]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        serde_json::from_str::<Value>(&after)?["code"],
        "owned_connection_revoked"
    );
    let (code, _, _) = ctl(&["verify", "--connection-id", &id]);
    assert_ne!(code, 0);
    let (old, new) = credential(&mut parity, "credential after revoke", &id)?;
    assert_ne!(new.0, 0);
    same(&parity, "credential after revoke", old, new);
    parity.finish();
    Ok(())
}

/// The enrollment `airk runtime http-connection --client codex` made, through
/// the same control method (HttpEnroll at the live instance), for a run that
/// replays the recorded `airk` outputs (split U95).
fn enroll_codex_connection(service: &common::Service, id: &str) -> anyhow::Result<()> {
    use airk_core::service::{ControlMethod, ControlResponse};
    let ControlResponse::Health { observation } = service.request(ControlMethod::Health) else {
        anyhow::bail!("missing service health");
    };
    let response = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(airk_cli::service_runtime::request_for_instance(
            &service.control,
            ControlMethod::HttpEnroll {
                connection_id: id.parse()?,
                selection: Box::new(service.http_selection()),
                client: airk_core::usage::ClientFamily::Codex,
                explorer_descriptor: None,
                reattach_connection_id: None,
            },
            observation.instance,
        ))?;
    anyhow::ensure!(
        matches!(response, ControlResponse::HttpConnection { .. }),
        "enrollment refused: {}",
        serde_json::to_string(&response)?
    );
    Ok(())
}

/// `service trust inspect` and `service issuer-rotation status` (split
/// 3e.5) print what `airk runtime http-trust` and `http-issuer-rotation`
/// print; install without the reviewed digest is refused by both.
#[test]
fn airkctl_trust_and_issuer_rotation_match_airk() -> anyhow::Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let service = common::Service::reviewed(source.path(), data.path(), "ctl-trust");
    let control = service.control.to_str().unwrap().to_owned();
    let runtime = |role: &str, extra: &[&str]| -> anyhow::Result<std::process::Output> {
        Ok(service
            .installed_command(source.path(), data.path(), "ctl-trust")
            .args(["runtime", role, "--install-dir"])
            .arg(service.install_dir())
            .args(extra)
            .output()?)
    };
    let ctl = |arguments: &[&str]| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = [
            "airkctl",
            "--service-runtime-dir",
            control.as_str(),
            "service",
        ]
        .into_iter()
        .chain(arguments.iter().copied())
        .map(std::ffi::OsString::from)
        .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let mut parity = Parity::new("airkctl_trust_and_issuer_rotation_match_airk");
    parity.substitute_path(source.path(), "<SOURCE>");
    parity.substitute_path(data.path(), "<DATA>");
    parity.service(&service);
    parity.mask_json_fields(CLOCK_FIELDS);
    // `user` names the account that captured the fixture.
    parity.mask_json_fields(&["user"]);
    // The endpoint's issuer and certificate identities are this run's.
    if let airk_core::service::ControlResponse::HttpEndpoint {
        endpoint: Some(endpoint),
    } = service.request(airk_core::service::ControlMethod::HttpEndpoint)
    {
        parity.substitute_identifiers(&endpoint, "ENDPOINT");
    }
    for (name, old, new) in [
        (
            "trust inspect",
            parity.old("trust inspect", || {
                runtime("http-trust", &["--action", "inspect"]).unwrap()
            }),
            ctl(&["trust", "inspect"]),
        ),
        (
            "issuer-rotation status",
            parity.old("issuer-rotation status", || {
                runtime("http-issuer-rotation", &["--action", "status"]).unwrap()
            }),
            ctl(&["issuer-rotation", "status"]),
        ),
    ] {
        assert!(old.success, "{name}: {}", old.stderr);
        assert_eq!(new.0, 0, "{name}: {}", new.2);
        assert_eq!(
            serde_json::from_str::<Value>(&parity.canonical(&new.1))?,
            serde_json::from_str::<Value>(&old.stdout)?,
            "{name}"
        );
    }
    let (code, _, stderr) = ctl(&["trust", "install"]);
    assert_ne!(code, 0);
    assert!(
        stderr.contains("install/remove requires the reviewed issuer SHA256"),
        "{stderr}"
    );
    assert!(
        !parity
            .old("trust install", || {
                runtime("http-trust", &["--action", "install"]).unwrap()
            })
            .success
    );
    parity.finish();
    Ok(())
}

/// `agents connect/list/repair/remove` (split 3e.6–3e.8) on a live service:
/// enroll a reviewed selection, write Claude Code and Codex project entries
/// naming the U92 helper, classify them, repair a missing helper and remove
/// them, keeping the other server.
#[test]
fn airkctl_agents_connect_list_repair_remove_on_a_live_service() -> anyhow::Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let service = common::Service::reviewed(source.path(), data.path(), "ctl-agentcfg");
    let control = service.control.to_str().unwrap().to_owned();
    let inputs = tempfile::tempdir()?;
    let private = inputs.path().join("private");
    airk_cli::local_ipc::prepare_root(&private)?;
    let selection = private.join("selection.json");
    fs::write(&selection, serde_json::to_vec(&service.http_selection())?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&selection, fs::Permissions::from_mode(0o600))?;
    }
    let project = tempfile::tempdir()?;
    let project = project.path().canonicalize()?;
    fs::write(
        project.join(".mcp.json"),
        "{\n  \"mcpServers\": {\n    \"other\": {\"command\": \"node\", \"args\": [\"x\"]}\n  }\n}\n",
    )?;
    let tools = inputs.path().canonicalize()?.join("bin");
    fs::create_dir(&tools)?;
    let name = if cfg!(windows) {
        "airkctl.exe"
    } else {
        "airkctl"
    };
    let helper = tools.join(name);
    fs::write(&helper, b"fictional airkctl")?;
    let ctl = |arguments: &[&str]| -> (u8, Value, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = [
            "airkctl",
            "--service-runtime-dir",
            control.as_str(),
            "agents",
        ]
        .into_iter()
        .chain(arguments.iter().copied())
        .map(std::ffi::OsString::from)
        .collect();
        let code = airkctl::cli::run(line, &mut io);
        let stdout = String::from_utf8(stdout).unwrap();
        (
            code,
            serde_json::from_str(&stdout).unwrap_or(Value::Null),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let id = airk_kernel::service_broker::new_instance_id()?.to_string();
    let folder = project.to_str().unwrap();
    let (code, connected, stderr) = ctl(&[
        "connect",
        "--client",
        "claude",
        "--connection-id",
        &id,
        "--selection-file",
        selection.to_str().unwrap(),
        "--project",
        folder,
        "--airkctl",
        helper.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(connected["changed"], true, "{connected}");
    let claude = fs::read_to_string(project.join(".mcp.json"))?;
    assert!(claude.contains("\"other\""), "{claude}");
    assert!(
        claude.contains(&format!(
            "agents credential --connection-id {id} --format header-json"
        )),
        "{claude}"
    );
    assert!(!claude.contains("airk1."), "{claude}");
    let (code, _, stderr) = ctl(&[
        "connect",
        "--client",
        "codex",
        "--connection-id",
        &id,
        "--project",
        folder,
        "--airkctl",
        helper.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    let list = || ctl(&["list", "--project", folder, "--no-user"]);
    let (code, listed, stderr) = list();
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(listed["findings"], serde_json::json!([]), "{listed}");
    let entries = listed["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "{listed}");
    assert!(
        entries
            .iter()
            .all(|entry| entry["connectionId"] == id.as_str()),
        "{listed}"
    );
    // The helper disappears: both entries report it, and repair rewrites them.
    fs::remove_file(&helper)?;
    let (_, listed, _) = list();
    let problems: Vec<_> = listed["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding["problem"].clone())
        .collect();
    assert_eq!(problems, vec!["https_helper_missing"; 2], "{listed}");
    let replacement = tools.join("new");
    fs::create_dir(&replacement)?;
    let moved = replacement.join(name);
    fs::write(&moved, b"fictional airkctl")?;
    let (code, repaired, stderr) = ctl(&[
        "repair",
        "--project",
        folder,
        "--no-user",
        "--airkctl",
        moved.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    let actions = repaired["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 2, "{repaired}");
    assert!(
        actions.iter().all(|action| action["changed"] == true),
        "{repaired}"
    );
    let (_, listed, _) = list();
    assert_eq!(listed["findings"], serde_json::json!([]), "{listed}");
    let (code, removed, stderr) = ctl(&["remove", "--project", folder, "--no-user"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(removed["actions"].as_array().unwrap().len(), 2, "{removed}");
    let claude = fs::read_to_string(project.join(".mcp.json"))?;
    assert!(
        claude.contains("\"other\"") && !claude.contains("agents credential"),
        "{claude}"
    );
    let (_, listed, _) = list();
    assert_eq!(listed["entries"], serde_json::json!([]), "{listed}");
    Ok(())
}

/// `airkctl explorer get-handoff` (M12): a handoff a window published is
/// written to a new file; an unknown id is refused and writes nothing.
#[test]
fn airkctl_explorer_get_handoff_reads_a_published_handoff() -> anyhow::Result<()> {
    use airk_core::service::{ControlMethod, ControlResponse};
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let service = common::Service::reviewed(source.path(), data.path(), "ctl-handoff");
    let control = service.control.to_str().unwrap().to_owned();
    let ControlResponse::WindowOpened {
        window_id, lease, ..
    } = service.request(ControlMethod::WindowOpen {
        label: "handoff test".into(),
        owner_pid: std::process::id(),
    })
    else {
        anyhow::bail!("window did not open");
    };
    let handoff = serde_json::json!({"kind": "search", "query": "parity"});
    let ControlResponse::HandoffPublished { handoff_id, .. } =
        service.request(ControlMethod::HandoffPublish {
            window_id,
            lease,
            handoff: handoff.clone(),
            ttl_ms: 60_000,
        })
    else {
        anyhow::bail!("handoff was not published");
    };
    let out = tempfile::tempdir()?;
    let get = |id: String, file: &std::path::Path| -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: false,
            window: None,
        };
        let line = [
            "airkctl",
            "--service-runtime-dir",
            control.as_str(),
            "explorer",
            "get-handoff",
            "--handoff-id",
            id.as_str(),
            "--output-file",
            file.to_str().unwrap(),
        ]
        .into_iter()
        .map(std::ffi::OsString::from)
        .collect();
        let code = airkctl::cli::run(line, &mut io);
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    };
    let written = out.path().join("handoff.json");
    let (code, stdout, stderr) = get(handoff_id.to_string(), &written);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(serde_json::from_str::<Value>(&stdout)?["ok"], true);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&written)?)?,
        handoff
    );
    let missing = out.path().join("missing.json");
    let unknown = airk_kernel::service_broker::new_instance_id()?.to_string();
    let (code, _, _) = get(unknown, &missing);
    assert_ne!(code, 0);
    assert!(!missing.exists());
    Ok(())
}

/// `airkctl session` on a real service (split step 3f.3): `hello` opens a
/// window the service lists (W1, W5), an `invoke` prints what the one-shot
/// command printed, and end of input closes the window (W3). No frame carries
/// the lease.
#[test]
fn airkctl_session_opens_and_closes_its_window_on_a_live_service() -> anyhow::Result<()> {
    use std::sync::{Arc, Mutex};
    struct Shared(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Shared {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let frame = |value: Value| {
        let body = serde_json::to_vec(&value).unwrap();
        let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
        bytes.extend(body);
        bytes
    };
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "ctl-session");
    let airk_core::service::ControlResponse::Health { observation } =
        service.request(airk_core::service::ControlMethod::Health)
    else {
        anyhow::bail!("missing service health");
    };
    let mut input = frame(json!({"id": "01".repeat(16), "request": {"hello": {
        "protocol": 2, "client": {"name": "airk-vscode", "version": "1.9.0"}, "i2_contracts": [1]}}}));
    input.extend(frame(json!({"id": "02".repeat(16), "request": {"invoke": {
        "argv": ["projects"], "deadline_ms": 30000}}})));
    let output = Arc::new(Mutex::new(Vec::new()));
    let mut options =
        airkctl::cli::session::Options::new(env!("CARGO_PKG_VERSION"), "0".repeat(64));
    options.control_root = Some(service.control.clone());
    options.label = "live window".into();
    let code = airkctl::cli::session::serve_streams(
        std::io::Cursor::new(input),
        Shared(output.clone()),
        options,
    );
    assert_eq!(code, 0);
    let bytes = output.lock().unwrap().clone();
    let mut frames = Vec::new();
    let mut rest = &bytes[..];
    while rest.len() >= 4 {
        let length = u32::from_be_bytes(rest[..4].try_into()?) as usize;
        frames.push(serde_json::from_slice::<Value>(&rest[4..4 + length])?);
        rest = &rest[4 + length..];
    }
    let reply = |id: &str| {
        frames
            .iter()
            .find(|frame| frame["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("no reply {id}: {frames:?}"))
    };
    let hello = reply(&"01".repeat(16));
    assert_eq!(hello["outcome"]["status"], "hello", "{hello}");
    assert_eq!(
        hello["outcome"]["service_instance"],
        observation.instance.to_string()
    );
    assert!(hello["outcome"]["window_id"].is_string(), "{hello}");
    let invoked = reply(&"02".repeat(16));
    let control = service.control.to_str().unwrap().to_owned();
    let one_shot = common::ctl::Ctl::new()
        .args(["projects", "--runtime-dir", &control])
        .output()?;
    assert_eq!(invoked["outcome"]["exit_code"], 0, "{invoked}");
    assert_eq!(
        invoked["outcome"]["stdout"],
        String::from_utf8(one_shot.stdout)?
    );
    assert!(!String::from_utf8_lossy(&bytes).contains("lease"));
    // End of input closed the window.
    let airk_core::service::ControlResponse::AssistanceWindows { windows } =
        service.request(airk_core::service::ControlMethod::AssistanceWindows {})
    else {
        anyhow::bail!("missing window listing");
    };
    assert!(windows.is_empty(), "{windows:?}");
    Ok(())
}

/// Usage-report text that varies per run: the root binding's digest bytes
/// and measured figures (`…Us: n`, `…Bytes: n`, `RSS: Some(n)`), at any JSON
/// escape level.
fn mask_measured_usage_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'scan: while !rest.is_empty() {
        for (marker, close) in [
            ("root_binding", Some(']')),
            ("latency_buckets", Some(']')),
            ("Us: ", None),
            ("Bytes: ", None),
            ("RSS: Some(", None),
        ] {
            if let Some(after) = rest.strip_prefix(marker) {
                out.push_str(marker);
                match close {
                    Some(end) => {
                        // Skip the quote/escape run and `:[`, then the array body.
                        let open = after.find('[').map_or(0, |at| at + 1);
                        let body = after[open..].find(end).map_or(after.len(), |at| open + at);
                        out.push_str(&after[..open]);
                        out.push_str("<MASKED>");
                        rest = &after[body..];
                    }
                    None => {
                        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
                        if digits > 0 {
                            out.push_str("<MASKED>");
                        }
                        rest = &after[digits..];
                    }
                }
                continue 'scan;
            }
        }
        // A Markdown table row's last cell (the measured column, e.g.
        // "Returned bytes"), escaped or not.
        if out.ends_with("| ") {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0
                && (rest[digits..].starts_with(" |\\n") || rest[digits..].starts_with(" |\n"))
            {
                out.push_str("<MASKED>");
                rest = &rest[digits..];
                continue 'scan;
            }
        }
        let next = rest.chars().next().unwrap();
        out.push(next);
        rest = &rest[next.len_utf8()..];
    }
    out
}
