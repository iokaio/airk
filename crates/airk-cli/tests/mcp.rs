// SPDX-License-Identifier: Apache-2.0
//! Real HTTPS MCP lifecycle and canonical collection-scoped tool parity.
mod common;
#[path = "mcp/source_runtime.rs"]
mod source_runtime;
use serde_json::{Value, json};
use std::{
    fs,
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

struct Session {
    transport: Mutex<common::http::HttpMcpSession>,
    _service: Arc<common::Service>,
    /// The session's response mode, so a CLI fallback reads alike.
    mode: Option<String>,
}
fn cli_call(
    root: &std::path::Path,
    data: &std::path::Path,
    collection: &str,
    args: &[&str],
) -> anyhow::Result<Value> {
    if args.first() == Some(&"index") {
        common::call(root, data, collection, args)
    } else {
        let result = common::scoped_call(root, data, collection, args)?;
        // Match the existing unscoped fixture adapter's convenience contract;
        // tools call retains its complete wire envelope for explicit comparison.
        if args.first() != Some(&"tools") && result.get("content").is_some() {
            native_payload(&result)
        } else {
            Ok(result)
        }
    }
}
/// U110: `airkctl tools call <tool> --index <id>`, the open read of the
/// fixture's one index: the service path every tokenless HTTPS MCP read
/// takes, so complete MCP results compare with it directly. Reads through
/// the reviewed scoped session (`cli_call`) carry that session's own scope
/// observation and compare with the other scoped CLI forms.
fn open_command(
    root: &std::path::Path,
    data: &std::path::Path,
    collection: &str,
    tool: &str,
    args: &Value,
) -> common::ctl::Ctl {
    let service = common::http_fixture(root, data, collection);
    let mut command = common::ctl::Ctl::new();
    command
        .arg("--service-runtime-dir")
        .arg(&service.control)
        .args(["tools", "call", tool])
        .args(service.open_scope().cli_arguments())
        .args(["--args".to_owned(), args.to_string()])
        .env("AIRK_MCP_RESPONSE_MODE", "full")
        .env("AIRK_USAGE_MODE", "disabled");
    command
}
/// The complete envelope of one open `tools call` (a tool error exits 1).
fn open_call(
    root: &std::path::Path,
    data: &std::path::Path,
    collection: &str,
    tool: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let output = common::read_retry::output(&mut open_command(root, data, collection, tool, args))?;
    anyhow::ensure!(
        output.status.success() || output.status.code() == Some(1),
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}
/// The JSON body of a scoped `tools call` through the reviewed session.
fn scoped_tool_payload(
    root: &std::path::Path,
    data: &std::path::Path,
    collection: &str,
    tool: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let envelope = cli_call(
        root,
        data,
        collection,
        &["tools", "call", tool, "--args", &args.to_string()],
    )?;
    Ok(serde_json::from_str(
        envelope["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing native JSON: {envelope}"))?,
    )?)
}
fn native_payload(envelope: &Value) -> anyhow::Result<Value> {
    anyhow::ensure!(
        envelope["isError"] != true,
        "native read refused: {envelope}"
    );
    let content = envelope["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing native content: {envelope}"))?;
    anyhow::ensure!(
        content.len() == 1 && content[0]["type"] == "text",
        "unexpected native content inventory: {envelope}"
    );
    let body: Value = serde_json::from_str(
        content[0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing native JSON: {envelope}"))?,
    )?;
    if let Some(structured) = envelope.get("structuredContent") {
        anyhow::ensure!(
            structured == &body,
            "native text and structured results disagree: {envelope}"
        );
    }
    Ok(body)
}
fn assert_query_scope_metadata_tool(tools: &[Value], name: &str, transport_project: bool) {
    let tool = tools
        .iter()
        .find(|tool| tool["name"] == name)
        .expect("query scope metadata tool");
    let canonical_catalog = if transport_project {
        airk_mcp::unified::tools()
    } else {
        airk_mcp::local::tools()
    };
    let canonical = canonical_catalog
        .iter()
        .find(|tool| tool.name == name)
        .expect("canonical query scope metadata tool");
    let canonical = serde_json::to_value(canonical).unwrap();
    assert_eq!(tool["inputSchema"], canonical["inputSchema"]);
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    let mut expected_schema = airk_core::combined_reads::query_scope::schema();
    expected_schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    if transport_project {
        let project = &tool["inputSchema"]["properties"]["project"];
        assert_eq!(project["type"], "string");
        assert_eq!(project["pattern"], "^[0-9a-f]{64}$");
        assert!(
            !tool["inputSchema"]["required"]
                .as_array()
                .is_some_and(|fields| fields.iter().any(|field| field == "project")),
            "the configured transport binding must remain optional"
        );
        expected_schema["properties"]["project"] = project.clone();
    } else {
        assert!(tool["inputSchema"]["properties"].get("project").is_none());
    }
    // The optional configured transport binding is the complete public delta.
    assert_eq!(tool["inputSchema"], expected_schema);
    for field in ["project_ids", "index_ids", "verification", "response_mode"] {
        assert!(
            tool["inputSchema"]["properties"].get(field).is_none(),
            "discovery must not advertise {field}"
        );
    }
}

/// The tokenless local agent session's `tools/list` (user decisions of 27
/// November 2026): only the seven core retrieval tools. Independently listed.
fn assert_local_mcp_catalog_names(tools: &[Value]) {
    let expected = [
        "find_definition",
        "find_identifier_occurrences",
        "gather_context",
        "list_indexed_files",
        "outline_file",
        "read_indexed_range",
        "search_passages",
    ];
    let mut actual = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("public tool name"))
        .collect::<Vec<_>>();
    actual.sort_unstable();
    assert_eq!(actual, expected, "complete local agent session catalog");
}

impl Session {
    fn start(root: &std::path::Path, state: &std::path::Path) -> anyhow::Result<Self> {
        // Existing rich-payload parity and publication tests explicitly use Full.
        Self::with_mode(root, state, Some("full"))
    }
    fn with_mode(
        root: &std::path::Path,
        state: &std::path::Path,
        mode: Option<&str>,
    ) -> anyhow::Result<Self> {
        let service = common::http_fixture(root, state, "project-a");
        // U110: tokenless HTTPS; every read names the fixture's one index.
        let transport = common::http::HttpMcpSession::start_with_mode(
            &service.control,
            service.open_scope(),
            mode,
        )?;
        Ok(Self {
            transport: Mutex::new(transport),
            _service: service,
            mode: mode.map(str::to_owned),
        })
    }
    fn send(&mut self, value: Value) -> anyhow::Result<()> {
        self.transport.get_mut().unwrap().send(value)
    }
    fn receive(&self, id: u64) -> anyhow::Result<Value> {
        self.receive_until(id, std::time::Instant::now() + Duration::from_secs(60))
    }
    fn receive_until(&self, id: u64, deadline: std::time::Instant) -> anyhow::Result<Value> {
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "HTTPS observation deadline exhausted"
        );
        self.transport
            .lock()
            .unwrap()
            .receive_until(i64::try_from(id)?, deadline)
    }
    fn call(&mut self, id: u64, name: &str, arguments: Value) -> anyhow::Result<Value> {
        self.call_until(
            id,
            name,
            arguments,
            std::time::Instant::now() + Duration::from_secs(60),
        )
    }
    fn call_until(
        &mut self,
        id: u64,
        name: &str,
        arguments: Value,
        deadline: std::time::Instant,
    ) -> anyhow::Result<Value> {
        let native = if name.starts_with("airk_") {
            name.to_owned()
        } else {
            airk_core::tool_names::native_name_of_public(name)
                .ok_or_else(|| anyhow::anyhow!("no native operation for {name}"))?
        };
        let name = airk_core::tool_names::public_name_of_native(name).unwrap_or(name);
        let reply = self.transport.get_mut().unwrap().request_until(
            i64::try_from(id)?,
            "tools/call",
            json!({"name":name,"arguments":arguments}),
            deadline,
        )?;
        if airk_core::tool_names::on_mcp(name) {
            return Ok(reply);
        }
        // MCP offers only the core retrieval tools (27 November 2026). It
        // refuses this operation, which the CLI keeps: the same open read
        // through `tools call` stands in for the MCP reply.
        anyhow::ensure!(
            reply["result"]["structuredContent"]["error"]["code"] == "UNKNOWN_TOOL",
            "MCP must refuse {name}: {reply}"
        );
        let mut command = common::ctl::Ctl::new();
        command
            .arg("--service-runtime-dir")
            .arg(&self._service.control)
            .args(["tools", "call", &native])
            .args(self._service.open_scope().cli_arguments())
            .args(["--args".to_owned(), arguments.to_string()])
            .env(
                "AIRK_MCP_RESPONSE_MODE",
                self.mode.as_deref().unwrap_or("terse"),
            )
            .env("AIRK_USAGE_MODE", "disabled");
        let output = common::read_retry::output(&mut command)?;
        anyhow::ensure!(
            output.status.success() || output.status.code() == Some(1),
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        let envelope: Value = serde_json::from_slice(&output.stdout)?;
        Ok(json!({"jsonrpc":"2.0","id":id,"result":envelope}))
    }
    fn published_status(&mut self, id: u64, project: &str) -> anyhow::Result<Value> {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "publication observation deadline exhausted"
            );
            let reply = self.call_until(
                id,
                "airk_diagnostics",
                json!({"response_mode":"full"}),
                deadline,
            )?;
            if let Some(retry) = common::read_retry::within(&reply["result"], deadline) {
                std::thread::sleep(retry.delay);
                continue;
            }
            anyhow::ensure!(
                reply.get("error").is_none() && reply["result"]["isError"] != true,
                "status read rejected"
            );
            let value: Value = serde_json::from_str(
                reply["result"]["content"][0]["text"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing status payload"))?,
            )?;
            if !common::read_retry::publication_pending(&value, "project-a", project)? {
                return Ok(reply);
            }
            std::thread::sleep(
                Duration::from_millis(25)
                    .min(deadline.saturating_duration_since(std::time::Instant::now())),
            );
        }
    }
    fn read_retry(&mut self, id: u64, name: &str, arguments: Value) -> anyhow::Result<Value> {
        let until = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let reply = self.call_until(id, name, arguments.clone(), until)?;
            let Some(retry) = common::read_retry::within(&reply["result"], until) else {
                return Ok(reply);
            };
            std::thread::sleep(retry.delay);
        }
    }
    fn wait_source_idle(&self) -> anyhow::Result<()> {
        self.wait_source_state("idle")
    }
    fn wait_source_state(&self, state: &str) -> anyhow::Result<()> {
        source_runtime::wait(&self._service, state)
    }
}

#[test]
fn saved_source_verification_preserves_complete_results_through_cli_and_public_mcp()
-> anyhow::Result<()> {
    fn payload(envelope: &Value) -> anyhow::Result<Value> {
        Ok(serde_json::from_str(
            envelope["content"][0]["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing native JSON: {envelope}"))?,
        )?)
    }
    fn stable(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                // These alone vary by transport observation. Source hashes,
                // scope/selection digests, watermarks and all outcomes remain.
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
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let source = "# Saved evidence\nQueue records preserve exact source identity.\n";
    fs::write(root.path().join("policy.md"), source)?;
    let service = common::Service::reviewed(root.path(), state.path(), "project-a");
    assert!(matches!(
        common::rpc(
            &service.control,
            airk_core::service::ControlMethod::ConfigureWatch {
                project: service.project.binding,
                enabled: false
            }
        ),
        airk_core::service::ControlResponse::Watch { .. }
    ));
    cli_call(root.path(), state.path(), "project-a", &["index"])?;
    let mut session = Session::start(root.path(), state.path())?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"verification-parity-fixture","version":"1"}}}))?;
    assert!(session.receive(1)?.get("error").is_none());
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    source_runtime::pause_and_wait(&service)?;
    let args = json!({"path":"policy.md","verification":"report","verification_max_sources":25,"verification_max_bytes":16777216,"verification_deadline_ms":2000,"response_mode":"full"});
    let mcp = session.read_retry(3, "file_info", args.clone())?;
    assert_ne!(mcp["result"]["isError"], true, "{mcp}");
    let native = payload(&mcp["result"])?;
    assert_eq!(native["source_verification"]["status"], "verified");
    assert_eq!(
        native["source_verification"]["counts"]["bytes_read"],
        source.len()
    );
    let alias = common::scoped_call(
        root.path(),
        state.path(),
        "project-a",
        &[
            "info",
            "policy.md",
            "--verification",
            "report",
            "--verification-max-sources",
            "25",
            "--verification-max-bytes",
            "16777216",
            "--verification-deadline-ms",
            "2000",
        ],
    )?;
    let tools = cli_call(
        root.path(),
        state.path(),
        "project-a",
        &[
            "tools",
            "call",
            "airk_file_info",
            "--args",
            &serde_json::to_string(&args)?,
        ],
    )?;
    // Compare the entire native envelopes as well as their complete bodies.
    // JSON text is normalized with the same dated-field policy as structured
    // content. Unknown envelope/content fields remain in the equality check.
    fn normalize_envelope(envelope: &mut Value) -> anyhow::Result<()> {
        if let Some(discriminator) = envelope
            .as_object_mut()
            .and_then(|fields| fields.remove("resultType"))
        {
            anyhow::ensure!(
                discriminator == "complete",
                "unexpected native result discriminator"
            );
        }
        if let Some(content) = envelope.get_mut("content").and_then(Value::as_array_mut) {
            for row in content {
                if row["type"] == "text"
                    && let Some(text) = row["text"].as_str()
                    && let Ok(mut body) = serde_json::from_str::<Value>(text)
                {
                    stable(&mut body);
                    row["text"] = serde_json::to_string(&body)?.into();
                }
            }
        }
        stable(envelope);
        Ok(())
    }
    // U110: HTTPS MCP is an open read of the fixture's index; it compares
    // with the open `tools call`. The scoped convenience form compares with
    // the scoped `tools call` of the same session.
    let open = open_call(
        root.path(),
        state.path(),
        "project-a",
        "airk_file_info",
        &args,
    )?;
    for (mut actual, reference, label) in [
        (open.clone(), mcp["result"].clone(), "open tools/HTTP"),
        (alias.clone(), tools.clone(), "convenience/tools"),
    ] {
        let mut expected_envelope = reference;
        normalize_envelope(&mut expected_envelope)?;
        normalize_envelope(&mut actual)?;
        assert_eq!(
            actual, expected_envelope,
            "{label} must retain the complete envelope"
        );
    }
    for (mut actual, mut expected) in [
        (payload(&open)?, native.clone()),
        (native_payload(&alias)?, payload(&tools)?),
    ] {
        stable(&mut actual);
        stable(&mut expected);
        assert_eq!(actual, expected);
    }
    let default_args = json!({"path":"policy.md","response_mode":"full"});
    let default_mcp = session.read_retry(30, "file_info", default_args.clone())?;
    let mut default_expected = payload(&default_mcp["result"])?;
    assert_eq!(default_expected["source_verification"]["mode"], "report");
    assert_eq!(
        default_expected["source_verification"]["limits"]["max_sources"],
        8
    );
    stable(&mut default_expected);
    let mut default_open = payload(&open_call(
        root.path(),
        state.path(),
        "project-a",
        "airk_file_info",
        &default_args,
    )?)?;
    stable(&mut default_open);
    assert_eq!(default_open, default_expected);
    let mut default_cli = cli_call(
        root.path(),
        state.path(),
        "project-a",
        &["info", "policy.md"],
    )?;
    stable(&mut default_cli);
    let mut default_scoped = scoped_tool_payload(
        root.path(),
        state.path(),
        "project-a",
        "airk_file_info",
        &default_args,
    )?;
    stable(&mut default_scoped);
    assert_eq!(default_cli, default_scoped);
    // Disable watching before the fixture edit, so this checks the returned old
    // evidence rather than racing a background publication of the new bytes.
    fs::write(root.path().join("policy.md"), "changed saved bytes\n")?;
    let mut require = args.clone();
    require["verification"] = json!("require");
    let failed = session.call(4, "file_info", require.clone())?;
    assert_eq!(failed["result"]["isError"], true, "{failed}");
    let expected_failure = payload(&failed["result"])?;
    assert_eq!(
        failed["result"]["structuredContent"], expected_failure,
        "the public transport must expose the complete required-check failure"
    );
    assert_eq!(
        expected_failure["error"]["code"],
        "verification_required_unmet"
    );
    assert_eq!(
        expected_failure["source_verification"]["sources"][0]["state"],
        "mismatch"
    );
    assert_eq!(expected_failure["publication"], native["publication"]);
    assert!(common::read_retry::classify(&failed["result"]).is_none());
    // native read: a required-check plan executes once and is never replayed.
    let required_plan = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"require",
        "plan":{"steps":[{"id":"check","tool":"file_info","args":{"path":"policy.md"}}]},"response_mode":"full"});
    let failed_plan = session.call(31, "parse", required_plan.clone())?;
    assert_eq!(failed_plan["result"]["isError"], true, "{failed_plan}");
    let mut expected_plan = payload(&failed_plan["result"])?;
    assert_eq!(
        expected_plan["source_verification"]["status"],
        "requirements_unmet"
    );
    stable(&mut expected_plan);
    let output = common::read_retry::output(&mut open_command(
        root.path(),
        state.path(),
        "project-a",
        "airk_parse",
        &required_plan,
    ))?;
    assert!(!output.status.success());
    let output: Value = serde_json::from_slice(&output.stdout)?;
    let mut actual_plan = payload(&output)?;
    stable(&mut actual_plan);
    assert_eq!(actual_plan, expected_plan);
    // The open `tools call` and HTTPS MCP return the same complete failure.
    let open_failure = open_call(
        root.path(),
        state.path(),
        "project-a",
        "airk_file_info",
        &require,
    )?;
    assert_eq!(open_failure["isError"], true);
    let mut open_failure = payload(&open_failure)?;
    let mut mcp_failure = expected_failure.clone();
    stable(&mut open_failure);
    stable(&mut mcp_failure);
    assert_eq!(open_failure, mcp_failure);
    // The scoped convenience form and scoped `tools call` agree with each other.
    let mut scoped_failure: Option<Value> = None;
    for invocation in [
        vec![
            "info",
            "policy.md",
            "--verification",
            "require",
            "--verification-max-sources",
            "25",
            "--verification-max-bytes",
            "16777216",
            "--verification-deadline-ms",
            "2000",
        ],
        vec!["tools", "call", "airk_file_info", "--args", "{}"],
    ] {
        let mut invocation: Vec<String> = invocation.into_iter().map(str::to_owned).collect();
        if invocation[0] == "tools" {
            *invocation.last_mut().unwrap() = serde_json::to_string(&require)?;
        }
        let output = common::read_retry::output(
            common::scoped_command(root.path(), state.path(), "project-a").args(&invocation),
        )?;
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let reply: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(reply["isError"], true);
        let mut actual = payload(&reply)?;
        stable(&mut actual);
        assert_eq!(actual["error"]["code"], "verification_required_unmet");
        assert_eq!(actual["publication"], native["publication"]);
        match &scoped_failure {
            Some(expected) => assert_eq!(&actual, expected),
            None => scoped_failure = Some(actual),
        }
    }
    // `tools shell` has only its project form now; the reviewed scope's
    // session (window) route serves `tools call`, not the console.
    let off = cli_call(
        root.path(),
        state.path(),
        "project-a",
        &["info", "policy.md", "--verification", "off"],
    )?;
    assert!(off.get("source_verification").is_none());
    assert_eq!(off["publication"], native["publication"]);
    assert_eq!(off["content_hash"], native["content_hash"]);
    // The actual CLI and public MCP paths share the same explicit terse
    // capability projection. Full mode retains the canonical Workbench schemas.
    for (ordinal, mode) in ["terse", "full"].into_iter().enumerate() {
        let args = json!({"adapter":"markdown","response_mode":mode,"include_status":true});
        let wire = session.read_retry(9000 + ordinal as u64, "capabilities", args.clone())?;
        anyhow::ensure!(wire["result"]["isError"] != true, "capabilities: {wire}");
        let mut native = payload(&wire["result"])?;
        let tools = open_call(
            root.path(),
            state.path(),
            "project-a",
            "airk_capabilities",
            &args,
        )?;
        let mut cli = payload(&tools)?;
        if mode == "terse" {
            assert!(native["workbench"].get("operations").is_none());
            assert_eq!(native["workbench"]["operation_schemas"]["included"], false);
            assert!(
                native["workbench"]["operation_names"]
                    .as_array()
                    .is_some_and(|names| !names.is_empty())
            );
        } else {
            assert_eq!(
                native["workbench"],
                airk_kernel::workbench::operation_catalog()
            );
        }
        stable(&mut native);
        stable(&mut cli);
        assert_eq!(
            cli, native,
            "complete {mode} capabilities differ by transport"
        );
    }
    Ok(())
}

#[test]
fn workbench_prepared_plans_execute_through_cli_and_public_mcp_without_source_writes()
-> anyhow::Result<()> {
    fn stable(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                for key in [
                    "request_id",
                    "observed_unix_ms",
                    "cache_age_ms",
                    "cached",
                    "elapsed_ms",
                    "observed_from_unix_ms",
                    "observed_to_unix_ms",
                ] {
                    fields.remove(key);
                }
                fields.values_mut().for_each(stable);
            }
            Value::Array(rows) => rows.iter_mut().for_each(stable),
            _ => {}
        }
    }
    fn compare_native(mut a: Value, mut b: Value) -> anyhow::Result<()> {
        if a.get("result").is_some() && b.get("result").is_some() {
            compare_native(a["result"].clone(), b["result"].clone())?;
            a.as_object_mut().unwrap().remove("result");
            b.as_object_mut().unwrap().remove("result");
        }
        common::read_retry::normalize_dated_readiness_pair(&mut a, &mut b, 2)?;
        stable(&mut a);
        stable(&mut b);
        assert_eq!(a, b);
        Ok(())
    }
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "project-a");
    let source = "pub struct QueueClient;\n";
    fs::write(root.path().join("client.rs"), source)?;
    let passage = format!("{} ScopeMarker\n", "background ".repeat(700));
    fs::write(root.path().join("passage.txt"), &passage)?;
    cli_call(root.path(), state.path(), "project-a", &["index"])?;
    let prepared = cli_call(
        root.path(),
        state.path(),
        "project-a",
        &["parse", "/search QueueClient --mode lexical"],
    )?;
    assert_eq!(prepared["status"], "prepared", "{prepared}");
    assert_eq!(prepared["plan"]["tier"], "command");
    assert_eq!(prepared["executed"], false);
    let mut session = Session::start(root.path(), state.path())?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"airk-workbench","version":"1"}}}))?;
    assert!(session.receive(1)?.get("error").is_none());
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    let decode = |reply: Value| -> anyhow::Result<Value> {
        anyhow::ensure!(
            reply.get("error").is_none() && reply["result"]["isError"] != true,
            "{reply}"
        );
        Ok(serde_json::from_str(
            reply["result"]["content"][0]["text"].as_str().unwrap(),
        )?)
    };
    // These are single-execution dependent plans, so a writer between their steps
    // correctly produces a partial result rather than authorizing whole-plan replay.
    // Freeze this parity fixture's completed source work through the public control
    // contract; source/update and publication-busy behavior have separate fixtures.
    session.wait_source_idle()?;
    assert!(matches!(
        service.request(airk_core::service::ControlMethod::Pause {
            project: service.project.binding,
            paused: true,
        }),
        airk_core::service::ControlResponse::Registered { project } if project.paused
    ));
    session.wait_source_state("paused")?;
    let request = json!({"schema":"airk.parse.request/1","plan":prepared["plan"]});
    let mcp_prepared = decode(session.read_retry(2, "parse", request.clone())?)?;
    assert_eq!(mcp_prepared["plan"], prepared["plan"]);
    let mut execution = request.clone();
    execution["mode"] = json!("execute");
    // native read: a whole evidence plan executes exactly once, without automatic replay.
    let mcp = decode(session.call(3, "parse", execution)?)?;
    assert_eq!(mcp["status"], "complete", "{mcp}");
    assert_eq!(mcp["executed"], true);
    let file = state.path().join("workbench-request.json");
    fs::write(&file, serde_json::to_vec(&request)?)?;
    let cli = cli_call(
        root.path(),
        state.path(),
        "project-a",
        &["parse", "--request", file.to_str().unwrap(), "--execute"],
    )?;
    assert_eq!(cli["plan"], mcp["plan"]);
    assert_eq!(cli["evidence"], mcp["evidence"]);
    assert!(!cli["evidence"].as_array().unwrap().is_empty());
    // Parse is a CLI operation only (27 November 2026): read its schema from
    // the complete catalog the CLI lists.
    let catalog = serde_json::to_value(airk_mcp::unified::tools())?;
    let parse = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "parse")
        .unwrap();
    for (offset, key) in ["plan", "recipe", "budgets", "output"]
        .into_iter()
        .enumerate()
    {
        assert!(
            parse["inputSchema"]["properties"][key]["type"]
                .as_array()
                .unwrap()
                .contains(&json!("object"))
        );
        let mut invalid = json!({"schema":"airk.parse.request/1"});
        invalid[key] = json!("{}");
        // native read: deliberate malformed nested argument; rejection executes exactly once.
        let rejected = session.call(40 + offset as u64, "parse", invalid)?;
        assert!(
            rejected.get("error").is_some() || rejected["result"]["isError"] == true,
            "{rejected}"
        );
    }
    for (offset, (compact, detail)) in [
        (Some(true), None),
        (Some(false), None),
        (None, Some("lean")),
        (None, Some("rich")),
    ]
    .into_iter()
    .enumerate()
    {
        let mut discovery = json!({"name":"QueueClient"});
        if let Some(compact) = compact {
            discovery["compact"] = json!(compact);
        }
        let mut request = json!({"schema":"airk.parse.request/1","mode":"execute","response_mode":"full",
        "plan":{"verification_scope":"steps","steps":[
            {"id":"find","tool":"symbols","args":discovery},
            {"id":"structure","tool":"file_info","args":{},"bind":[{"arg":"path","from":"find","select":"unique_definition_path"}]},
            {"id":"excerpt","tool":"read_file","args":{},"bind":[{"arg":"location","from":"find","select":"hit_locations"}]}
        ]}});
        if let Some(detail) = detail {
            request["reply_detail"] = json!(detail);
        }
        // native read: a dependent evidence plan executes once; never retry the whole plan.
        let mcp = decode(session.call(20 + offset as u64, "parse", request.clone())?)?;
        assert_eq!(mcp["status"], "complete", "{mcp}");
        assert_eq!(mcp["calls"], 4);
        if compact == Some(true) || detail == Some("lean") {
            assert!(mcp["steps"][0]["result"].get("guidance").is_none(), "{mcp}");
        }
        fs::write(&file, serde_json::to_vec(&request)?)?;
        let cli = cli_call(
            root.path(),
            state.path(),
            "project-a",
            &["parse", "--request", file.to_str().unwrap()],
        )?;
        let tool_reply = cli_call(
            root.path(),
            state.path(),
            "project-a",
            &[
                "tools",
                "call",
                "airk_parse",
                "--args",
                &serde_json::to_string(&request)?,
            ],
        )?;
        let tool: Value = serde_json::from_str(tool_reply["content"][0]["text"].as_str().unwrap())?;
        // U110: HTTPS MCP compares with the open `tools call` (one service
        // path); the scoped convenience form with the scoped `tools call`.
        let open = native_payload(&open_call(
            root.path(),
            state.path(),
            "project-a",
            "airk_parse",
            &request,
        )?)?;
        for (actual, mcp) in [(&open, &mcp), (&cli, &tool)] {
            assert_eq!(actual["status"], "complete", "{actual}");
            assert_eq!(actual["plan"], mcp["plan"]);
            assert_eq!(actual["evidence"], mcp["evidence"]);
            assert_eq!(actual["completeness"], mcp["completeness"]);
            for field in [
                "schema",
                "collection",
                "publication",
                "requires_input",
                "warnings",
                "truncated",
                "calls",
                "executed",
                "reply_detail",
            ] {
                assert_eq!(
                    actual[field], mcp[field],
                    "lost or changed envelope field {field}"
                );
            }
            assert_eq!(
                actual["steps"].as_array().unwrap().len(),
                mcp["steps"].as_array().unwrap().len()
            );
            for (a, b) in actual["steps"]
                .as_array()
                .unwrap()
                .iter()
                .zip(mcp["steps"].as_array().unwrap())
            {
                assert_eq!(a["status"], b["status"]);
                assert_eq!(a["arguments"], b["arguments"]);
                // Compare complete native read data; only transport/cache observations vary.
                compare_native(a.clone(), b.clone())?;
            }
        }
        if let Some(detail) = detail {
            let alias = cli_call(
                root.path(),
                state.path(),
                "project-a",
                &["symbol", "QueueClient", "--reply-detail", detail],
            )?;
            compare_native(alias, tool["steps"][0]["result"].clone())?;
        }
    }
    for (offset, (detail, full)) in [
        ("lean", false),
        ("rich", false),
        ("lean", true),
        ("rich", true),
    ]
    .into_iter()
    .enumerate()
    {
        let args = json!({"query":"ScopeMarker","mode":"lexical","reply_detail":detail,"full_chunks":full,"limit":10,"max_bytes":8192,"response_mode":"full"});
        let mcp = decode(session.read_retry(60 + offset as u64, "airk_search", args.clone())?)?;
        let returned = mcp["hits"][0]["text"].as_str().unwrap();
        if full {
            assert_eq!(returned, passage);
        } else {
            assert!(returned.len() <= 768);
            assert!(mcp.get("guidance").is_none(), "{}: {mcp}", "truncated");
        }
        let open = native_payload(&open_call(
            root.path(),
            state.path(),
            "project-a",
            "airk_search",
            &args,
        )?)?;
        compare_native(mcp.clone(), open)?;
        if !full {
            let tool =
                scoped_tool_payload(root.path(), state.path(), "project-a", "airk_search", &args)?;
            let alias = cli_call(
                root.path(),
                state.path(),
                "project-a",
                &[
                    "search",
                    "ScopeMarker",
                    "--mode",
                    "lexical",
                    "--reply-detail",
                    detail,
                ],
            )?;
            compare_native(tool, alias)?;
        }
    }
    assert_eq!(
        fs::read_to_string(root.path().join("passage.txt"))?,
        passage
    );
    for (offset, (tool, args, reason)) in [
        (
            "airk_search",
            json!({"query":"AbsentIdentifier","mode":"lexical","response_mode":"full"}),
            "unknown",
        ),
        (
            "airk_capabilities",
            json!({"adapter":"text","response_mode":"full"}),
            "unsupported",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut mcp = decode(session.read_retry(70 + offset as u64, tool, args.clone())?)?;
        assert!(mcp.get("guidance").is_none(), "{}: {mcp}", reason);
        let mut cli = native_payload(&open_call(
            root.path(),
            state.path(),
            "project-a",
            tool,
            &args,
        )?)?;
        stable(&mut mcp);
        stable(&mut cli);
        assert_eq!(mcp, cli);
    }
    let direct = decode(session.read_retry(4, "index_status", json!({"response_mode":"full"}))?)?;
    assert_eq!(direct["query_concurrency"], 16);
    // native read: only this single refused status read can repeat with its explicit receipt.
    // The evidence plan above still executes once, without whole-plan replay.
    let status_request = json!({"schema":"airk.parse.request/1","mode":"execute","text":"/status","response_mode":"full"});
    let parsed =
        status_plan_with_retry(|| decode(session.call(5, "parse", status_request.clone())?))?;
    let nested = &parsed["steps"][0]["result"];
    assert_eq!(parsed["steps"][0]["status"], "complete", "{parsed}");
    // U110 (contracts §2.11): `scope` reports how the outer read's scope was
    // chosen; it belongs to the open read result, not to a plan step's result.
    assert_eq!(direct["read_scope"]["source"], "named", "{direct}");
    for field in direct
        .as_object()
        .unwrap()
        .keys()
        .filter(|field| *field != "read_scope")
    {
        assert!(
            nested.get(field).is_some(),
            "Workbench status lost MCP field {field}: {nested}"
        );
    }
    assert_eq!(nested["query_concurrency"], 16);
    assert_eq!(nested["publication"], direct["publication"]);
    assert_eq!(nested["files"], direct["files"]);
    let cli_status = cli_call(root.path(), state.path(), "project-a", &["status"])?;
    assert_eq!(cli_status["query_concurrency"], 16);
    fs::write(
        &file,
        serde_json::to_vec(
            &json!({"schema":"airk.parse.request/1","mode":"execute","text":"/status"}),
        )?,
    )?;
    // native read: the same single-status request; additional/completed steps cannot repeat.
    let cli_plan = status_plan_with_retry(|| {
        cli_call(
            root.path(),
            state.path(),
            "project-a",
            &["parse", "--request", file.to_str().unwrap()],
        )
    })?;
    assert_eq!(
        cli_plan["steps"][0]["result"]["query_concurrency"], 16,
        "{cli_plan}"
    );
    assert_eq!(fs::read_to_string(root.path().join("client.rs"))?, source);
    Ok(())
}

// A fixture may repeat one explicitly refused status read: at most 12 attempts,
// with further attempts admitted only within 30 seconds. Other plans execute once.
fn status_plan_with_retry(
    mut read: impl FnMut() -> anyhow::Result<Value>,
) -> anyhow::Result<Value> {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut attempts = 1;
    let mut reply = read()?;
    loop {
        let single_status = reply["executed"] == true
            && reply["status"] == "failed"
            && reply["calls"] == 1
            && reply["plan"]["steps"].as_array().is_some_and(|steps| {
                steps.len() == 1
                    && steps[0]["tool"] == "status"
                    && steps[0]["bind"].as_array().is_some_and(Vec::is_empty)
            })
            && reply["steps"].as_array().is_some_and(|steps| {
                steps.len() == 1 && steps[0]["tool"] == "status" && steps[0]["status"] == "failed"
            });
        let retry = single_status
            .then(|| common::read_retry::within(&reply["steps"][0]["result"], deadline))
            .flatten()
            .filter(|_| attempts < 12);
        if let Some(retry) = retry {
            eprintln!("status plan refusal {attempts}: {reply}");
            std::thread::sleep(retry.delay);
            if std::time::Instant::now() >= deadline {
                return Ok(reply);
            }
            attempts += 1;
            reply = read()?;
        } else {
            return Ok(reply);
        }
    }
}

#[test]
fn workbench_prepared_plans_retry_only_single_explicit_status_refusals() -> anyhow::Result<()> {
    let receipt = json!({"isError":true,"content":[{"type":"text","text":json!({"status":"retry","retryable":true,"code":"publication_busy","retry_after_ms":1,"action":"repeat_same_request"}).to_string()}]});
    let refused = json!({"executed":true,"status":"failed","calls":1,"plan":{"steps":[{"tool":"status","bind":[]}]},"steps":[{"tool":"status","status":"failed","result":receipt}]});
    let complete = json!({"status":"complete","steps":[{"tool":"status","status":"complete","result":{"query_concurrency":16}}]});
    let mut calls = 0;
    assert_eq!(
        status_plan_with_retry(|| {
            calls += 1;
            Ok(if calls == 1 {
                refused.clone()
            } else {
                complete.clone()
            })
        })?,
        complete
    );
    assert_eq!(calls, 2);
    let mut cases = vec![];
    let mut value = refused.clone();
    value["calls"] = json!(2);
    cases.push(value);
    let mut value = refused.clone();
    value["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"tool":"search","status":"complete"}));
    cases.push(value);
    for result in [
        json!({"isError":true,"content":[{"type":"text","text":"unknown failure"}]}),
        json!({"isError":true,"content":[{"type":"text","text":json!({"error":{"code":"direct_query_admission","reason":"busy","query_submitted":true,"retryable":true,"fresh_brokerage_required":false,"retry_after_ms":1}}).to_string()}]}),
    ] {
        let mut value = refused.clone();
        value["steps"][0]["result"] = result;
        cases.push(value);
    }
    for value in cases {
        let mut calls = 0;
        assert_eq!(
            status_plan_with_retry(|| {
                calls += 1;
                Ok(value.clone())
            })?,
            value
        );
        assert_eq!(calls, 1, "ineligible plan was replayed");
    }
    Ok(())
}

#[test]
fn default_terse_https_supports_full_and_requested_status_without_changing_revision()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let _service = common::Service::reviewed(root.path(), state.path(), "project-a");
    fs::write(
        root.path().join("one.rs"),
        "fn response_fixture() -> bool { true }\n",
    )?;
    cli_call(root.path(), state.path(), "project-a", &["index"])?;
    let mut session = Session::with_mode(root.path(), state.path(), None)?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"response-mode-fixture","version":"1"}}}))?;
    let initialized = session.receive(1)?;
    let instructions = initialized["result"]["instructions"].as_str().unwrap();
    assert!(instructions.len() <= 512 && instructions.contains("response_mode=full"));
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    // MCP offers no help tool (27 November 2026): `Session::call` asserts the
    // MCP refusal under either negotiated protocol and returns the CLI's
    // answer, which keeps help with its own structured refusal.
    let invalid_help = json!({"topic":"workbench","detail":"full"});
    let refused = session.call(200, "help", invalid_help.clone())?;
    assert_eq!(refused["result"]["isError"], true);
    let command = open_command(
        root.path(),
        state.path(),
        "project-a",
        "help",
        &invalid_help,
    );
    let output = common::read_retry::once_until(
        &command,
        std::time::Instant::now() + Duration::from_secs(60),
    )?;
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let canonical: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(canonical["isError"], true);
    assert_eq!(canonical["resultType"], "complete");
    // 2026-07-28 uses per-request metadata rather than initialize. Asking for
    // it through this legacy handshake negotiates the supported 2025 fallback.
    let mut fallback = Session::with_mode(root.path(), state.path(), None)?;
    fallback.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"future-initialize-fallback","version":"1"}}}))?;
    let initialized = fallback.receive(1)?;
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    fallback.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    let fallback_refused = fallback.call(2, "help", invalid_help)?;
    assert_eq!(fallback_refused["result"]["isError"], true);
    drop(fallback);    let mut values: Vec<Value> = Vec::new();
    for (id, args) in [
        (2, json!({"query":"response_fixture","mode":"lexical"})),
        (3, json!({"query":"response_fixture","mode":"lexical"})),
        (
            4,
            json!({"query":"response_fixture","mode":"lexical","response_mode":"full"}),
        ),
        (
            5,
            json!({"query":"response_fixture","mode":"lexical","include_status":true}),
        ),
    ] {
        let response = session.read_retry(id, "airk_search", args)?;
        anyhow::ensure!(response["result"]["isError"] != true, "{response}");
        let mut value = serde_json::from_str::<Value>(
            response["result"]["content"][0]["text"].as_str().unwrap(),
        )?;
        if id == 3 {
            // Freshness observations can settle after the index job completes
            // without changing its publication. Only unchanged status is omitted.
            let mut prior = values[0]["freshness"].clone();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut next_id = 6;
            while let Some(freshness) = value.get("freshness") {
                let mut before = prior.clone();
                let mut after = freshness.clone();
                before.as_object_mut().unwrap().remove("observed_unix_ms");
                after.as_object_mut().unwrap().remove("observed_unix_ms");
                assert_ne!(before, after, "unchanged freshness was repeated");
                assert_eq!(
                    value["publication"]["token"],
                    values[0]["publication"]["token"]
                );
                assert_eq!(value["hits"], values[0]["hits"]);
                prior = freshness.clone();
                anyhow::ensure!(
                    std::time::Instant::now() < deadline,
                    "freshness observations never settled: {value}"
                );
                let response = session.read_retry(
                    next_id,
                    "airk_search",
                    json!({"query":"response_fixture","mode":"lexical"}),
                )?;
                anyhow::ensure!(response["result"]["isError"] != true, "{response}");
                value = serde_json::from_str(
                    response["result"]["content"][0]["text"].as_str().unwrap(),
                )?;
                next_id += 1;
            }
        }
        values.push(value);
    }
    assert!(values[0].get("generation").is_none());
    assert!(values[0].get("freshness").is_some());
    assert!(values[1].get("freshness").is_none());
    assert!(values[2]["publication"]["generation"].is_u64());
    assert!(values[3].get("freshness").is_some());
    for value in &values[1..] {
        assert_eq!(
            value["publication"]["token"],
            values[0]["publication"]["token"]
        );
        assert_eq!(value["hits"], values[0]["hits"]);
    }
    Ok(())
}

#[test]
#[cfg(feature = "local-inference")]
fn default_search_uses_vectors_and_live_mcp_connections_observe_panel_opt_out() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let service = common::Service::reviewed(root.path(), state.path(), "project-a");
    fs::write(root.path().join("auth.txt"), "authenticate")?;
    let model = state.path().join("models/local");
    fs::create_dir_all(&model)?;
    fs::write(
        model.join("vocabulary.json"),
        br#"{"authenticate":[1.0,0.0],"login":[0.99,0.01]}"#,
    )?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(&json!({
            "schema":1,"name":"original-mcp-default-fixture","license":"Apache-2.0","dimensions":2,
            "sha256":"44c954367402ad93265c92c015fbfa80eaeb8dd7b2ec44b64650d7071df4d2f5"
        }))?,
    )?;
    let cli = |args: &[&str]| -> anyhow::Result<Value> {
        cli_call(root.path(), state.path(), "project-a", args)
    };
    // Initial native setup may still update the model policy. Complete that
    // accepted work before capturing this fixture's first configuration CAS.
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + Duration::from_secs(90),
    )?;
    service.configure(airk_cli::service_configuration::Draft::InstallModel { directory: model });
    cli(&["index"])?;
    service.wait_semantic(1)?;
    assert_eq!(
        cli(&["search", "login"])?["hits"][0]["citation"]["path"],
        "auth.txt"
    );
    let direct = cli(&[
        "tools",
        "call",
        "airk_search",
        "--args",
        r#"{"query":"login","mode":"vector"}"#,
    ])?;
    let direct: Value = serde_json::from_str(direct["content"][0]["text"].as_str().unwrap())?;
    assert_eq!(direct["hits"][0]["citation"]["path"], "auth.txt");
    let mut session = Session::start(root.path(), state.path())?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"vector-default-test","version":"1"}}}))?;
    session.receive(1)?;
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    let evidence = |reply: Value| -> anyhow::Result<Value> {
        anyhow::ensure!(reply["result"]["isError"] != true, "{reply}");
        Ok(serde_json::from_str(
            reply["result"]["content"][0]["text"].as_str().unwrap(),
        )?)
    };
    let first = evidence(session.read_retry(2, "airk_search", json!({"query":"login"}))?)?;
    assert_eq!(first["hits"][0]["citation"]["path"], "auth.txt");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    let disabled = evidence(session.read_retry(3, "airk_search", json!({"query":"login"}))?)?;
    assert_eq!(disabled["hits"], json!([]));
    // Native source turns may publish after the policy job; the live session
    // must observe opt-out without relying on a race against that publication.
    assert_eq!(
        first["publication"]["incarnation"],
        disabled["publication"]["incarnation"]
    );
    assert_eq!(
        session.call(4, "airk_search", json!({"query":"login","mode":"vector"}))?["result"]["isError"],
        true
    );
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: true });
    cli(&["index"])?;
    service.wait_semantic(1)?;
    assert_eq!(
        evidence(session.read_retry(5, "airk_search", json!({"query":"login"}))?)?["hits"][0]["citation"]
            ["path"],
        "auth.txt"
    );
    Ok(())
}

#[test]
fn durable_key_reads_match_cli_and_mcp_and_reject_filter_combinations() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "project-a");
    fs::write(
        root.path().join("original.rs"),
        "pub fn original_evidence() -> u32 { 7 }\n",
    )?;
    fs::write(
        root.path().join("original.md"),
        "# Original evidence\n\nThe original client must retain its local evidence.\n",
    )?;
    let run = |args: &[&str]| {
        common::scoped_command(root.path(), state.path(), "project-a")
            .args(args)
            .output()
    };
    let cli = |args: &[&str]| cli_call(root.path(), state.path(), "project-a", args);
    // Initial native setup may still update the model policy. Complete that
    // accepted work before capturing this fixture's first configuration CAS.
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + Duration::from_secs(90),
    )?;
    service.configure(airk_cli::service_configuration::Draft::Detail {
        level: airk_core::extraction::IndexDetail::Detailed,
    });
    cli(&["index"])?;
    let code = cli(&[
        "code-facts",
        "--compact",
        "false",
        "--name",
        "original_evidence",
    ])?;
    let symbol_key = code["items"][0]["continuity_key"]["key"]
        .as_str()
        .expect("qualified symbol key");
    let documents = cli(&["facts", "--compact", "false", "--kind", "section"])?;
    let document_key = documents["items"][0]["continuity_key"]["key"]
        .as_str()
        .expect("qualified document key");
    let mut session = Session::start(root.path(), state.path())?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"original-key-parity","version":"1"}}}))?;
    session.receive(1)?;
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    session.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}))?;
    let catalog = session.receive(2)?;
    let tools = catalog["result"]["tools"].as_array().unwrap();
    assert_local_mcp_catalog_names(tools);
    // MCP offers no query_scope (27 November 2026); the CLI keeps it.
    assert!(!tools.iter().any(|tool| tool["name"] == "query_scope"));
    session.wait_source_idle()?;
    for (id, command, option, tool, field, key) in [
        (
            3,
            "code-facts",
            "--symbol-key",
            "airk_code_facts",
            "symbol_key",
            symbol_key,
        ),
        (
            4,
            "facts",
            "--document-key",
            "airk_document_facts",
            "document_key",
            document_key,
        ),
    ] {
        let arguments = json!({(field):key,"compact":false,"limit":10});
        // The scoped convenience form and the scoped `tools call` agree.
        let mut convenience = cli(&[command, option, key, "--compact", "false", "--limit", "10"])?;
        let mut scoped =
            scoped_tool_payload(root.path(), state.path(), "project-a", tool, &arguments)?;
        // U110: HTTPS MCP and the open `tools call` take one service path.
        let mut expected = native_payload(&open_call(
            root.path(),
            state.path(),
            "project-a",
            tool,
            &arguments,
        )?)?;
        let result = session.read_retry(id, tool, arguments.clone())?;
        assert_ne!(result["result"]["isError"], true, "{result}");
        let mut actual: Value = serde_json::from_str(
            result["result"]["content"][0]["text"]
                .as_str()
                .expect("tool payload"),
        )?;
        for (left, right) in [
            (&mut expected, &mut actual),
            (&mut convenience, &mut scoped),
        ] {
            common::read_retry::normalize_dated_readiness_pair(left, right, 2)?;
            for response in [&mut *left, &mut *right] {
                common::read_retry::evidence_for_comparison(response)?;
            }
            assert_eq!(left, right);
        }
        assert!(!actual["items"].as_array().unwrap().is_empty());
        assert!(
            !run(&[command, option, key, "--name", "original"])?
                .status
                .success()
        );
        let invalid = session.call(id + 10, tool, json!({(field):key,"name":"original"}))?;
        assert!(invalid["result"]["isError"] == true || invalid.get("error").is_some());
    }
    Ok(())
}

#[test]
fn changes_cli_and_mcp_share_results_and_conditional_validators() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let _service = common::Service::reviewed(root.path(), state.path(), "project-a");
    fs::write(
        root.path().join("aurora.py"),
        "def northern_observation():\n    return 71\n",
    )?;
    let cli = |args: &[&str]| -> anyhow::Result<Value> {
        cli_call(root.path(), state.path(), "project-a", args)
    };
    cli(&["index"])?;
    let mut session = Session::start(root.path(), state.path())?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"original-parity-test","version":"1"}}}))?;
    let initialized = session.receive(1)?;
    assert!(
        initialized["result"]["instructions"]
            .as_str()
            .expect("instructions")
            .len()
            <= 512
    );
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    let payload = |result: Value| -> anyhow::Result<Value> {
        anyhow::ensure!(
            result["result"]["isError"] != true && result.get("error").is_none(),
            "{result}"
        );
        Ok(serde_json::from_str(
            result["result"]["content"][0]["text"]
                .as_str()
                .expect("tool JSON"),
        )?)
    };
    session.wait_source_idle()?;
    let changes = json!({"selectors":[{"kind":"path","path":"aurora.py"}]});
    // The scoped convenience form and the scoped `tools call` agree; HTTPS
    // MCP and the open `tools call` (one service path, U110) agree.
    let mut convenience = cli(&["changes", "--path", "aurora.py"])?;
    let mut scoped = scoped_tool_payload(
        root.path(),
        state.path(),
        "project-a",
        "airk_changes",
        &changes,
    )?;
    let mut expected = payload(json!({"result": open_call(
        root.path(),
        state.path(),
        "project-a",
        "airk_changes",
        &changes,
    )?}))?;
    let mut actual = payload(session.read_retry(2, "airk_changes", changes.clone())?)?;
    for (left, right) in [
        (&mut expected, &mut actual),
        (&mut convenience, &mut scoped),
    ] {
        common::read_retry::normalize_dated_readiness_pair(left, right, 1)?;
        for value in [&mut *left, &mut *right] {
            assert!(
                value.get("freshness").is_some(),
                "changes omitted freshness"
            );
            common::read_retry::evidence_for_comparison(value)?;
        }
        assert_eq!(left, right);
    }
    // The validator of the open read's own evidence.
    let info = native_payload(&open_call(
        root.path(),
        state.path(),
        "project-a",
        "airk_file_info",
        &json!({"path":"aurora.py"}),
    )?)?;
    let validator = info["read"]["validator"].as_str().expect("validator");
    let unchanged = payload(session.read_retry(
        3,
        "airk_file_info",
        json!({"path":"aurora.py","if_evidence":validator}),
    )?)?;
    assert_eq!(unchanged["read"]["status"], "not_modified");
    assert_eq!(
        unchanged["read"]["publication"],
        info["read"]["publication"]
    );
    for (id, args) in [
        (
            4,
            json!({"selectors":[{"kind":"path","path":"../outside"}]}),
        ),
        (
            5,
            json!({"selectors":vec![json!({"kind":"path","path":"aurora.py"});65]}),
        ),
        (
            6,
            json!({"selectors":[{"kind":"path","path":"aurora.py"}],"max_bytes":1}),
        ),
    ] {
        let result = session.call(id, "airk_changes", args)?;
        assert!(
            result["result"]["isError"] == true || result.get("error").is_some(),
            "{result}"
        );
    }
    Ok(())
}

#[test]
fn concurrent_mcp_agents_share_one_index_and_refresh_after_publication() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let _service = common::Service::reviewed(root.path(), state.path(), "project-a");
    let old_text = "def authenticate():\n    return 'revisionone'\n";
    let new_text = "def authenticate():\n    return 'revisiontwo'\n";
    fs::write(root.path().join("auth.py"), old_text)?;
    let index = || -> anyhow::Result<Value> {
        // Index admission retains one exact request key across bounded metadata replies.
        _service.job(airk_core::service::JobKind::Index);
        _service.settle_reviewed_index(
            root.path(),
            state.path(),
            std::time::Instant::now() + Duration::from_secs(90),
        )?;
        common::published_status(root.path(), state.path(), "project-a")
    };
    index()?;
    source_runtime::pause_and_wait(&_service)?;
    let baseline = common::published_status(root.path(), state.path(), "project-a")?;
    let old_generation = baseline["generation"].as_u64().expect("generation");
    let old_publication = baseline["publication"].clone();
    assert!(old_publication["token"].is_string());
    assert!(baseline["scope_observation"].is_object());
    let mut agents = [
        Session::start(root.path(), state.path())?,
        Session::start(root.path(), state.path())?,
    ];
    for agent in &mut agents {
        agent.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"shared-index-test","version":"1"}}}))?;
        anyhow::ensure!(
            agent.receive(1)?.get("error").is_none(),
            "initialization failed"
        );
        agent.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    }
    let evidence = |response: Value| -> anyhow::Result<Value> {
        anyhow::ensure!(
            response.get("error").is_none() && response["result"]["isError"] != true,
            "{response}"
        );
        Ok(serde_json::from_str(
            response["result"]["content"][0]["text"]
                .as_str()
                .expect("text"),
        )?)
    };
    // Queries do not acquire or wait for the collection's exclusive writer lock.
    // Activate the native worker first; startup recovery has its own writer gate.
    // U110: both tokenless sessions name the same index, so they observe one
    // open scope; every later read keeps it across the publication.
    let mut expected_scope = Value::Null;
    for agent in &mut agents {
        let ready = evidence(agent.read_retry(
            90,
            "airk_search",
            json!({"query":"authenticate", "mode":"lexical"}),
        )?)?;
        assert_eq!(ready["hits"][0]["text"], old_text);
        assert_eq!(ready["publication"], old_publication);
        assert!(ready["scope_observation"].is_object(), "{ready}");
        if expected_scope.is_null() {
            expected_scope = ready["scope_observation"].clone();
        }
        assert_eq!(ready["scope_observation"], expected_scope);
    }
    let expected_scope = expected_scope;
    let collection = airk_store::service::collection::Collection::existing(
        std::path::Path::new(&_service.project.data_root),
        &_service.project.root,
        &_service.project.collection,
    )?;
    let store = collection.selected_store()?;
    // Read readiness does not promise that native maintenance has released its
    // short writer turn. Wait only for typed OS contention, then retain the
    // actual lock throughout both concurrent reads on every host platform.
    let writer_deadline = std::time::Instant::now() + Duration::from_secs(30);
    let writer_lock = loop {
        match store.lock() {
            Ok(lock) => break lock,
            Err(error)
                if matches!(
                    error.downcast_ref::<std::fs::TryLockError>(),
                    Some(std::fs::TryLockError::WouldBlock)
                ) && std::time::Instant::now() < writer_deadline =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error.context("acquiring concurrent-reader fixture lock")),
        }
    };
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| -> anyhow::Result<()> {
        let readers: Vec<_> = agents
            .iter_mut()
            .map(|agent| {
                scope.spawn(|| -> anyhow::Result<()> {
                    barrier.wait();
                    let payload = evidence(agent.read_retry(
                        2,
                        "airk_search",
                        json!({"query":"authenticate", "mode":"lexical"}),
                    )?)?;
                    assert_eq!(payload["generation"], old_generation);
                    assert_eq!(payload["publication"], old_publication);
                    assert_eq!(payload["collection"], "project-a");
                    assert_eq!(payload["hits"][0]["text"], old_text);
                    Ok(())
                })
            })
            .collect();
        for reader in readers {
            reader.join().expect("reader panicked")?;
        }
        Ok(())
    })?;
    drop(writer_lock);

    fs::write(root.path().join("auth.py"), new_text)?;
    assert!(
        matches!(_service.request(airk_core::service::ControlMethod::Pause{project:_service.project.binding,paused:false}),airk_core::service::ControlResponse::Registered{project} if !project.paused)
    );
    let observations = Mutex::new(Vec::<Value>::new());
    let barrier = std::sync::Barrier::new(3);
    let published = std::thread::scope(|scope| -> anyhow::Result<Value> {
        let readers: Vec<_> = agents
            .iter_mut()
            .map(|agent| {
                scope.spawn(|| -> anyhow::Result<()> {
                    barrier.wait();
                    let mut last_generation = old_generation;
                    for id in 10..42 {
                        let result = agent.call(
                            id,
                            "airk_search",
                            json!({"query":"authenticate", "mode":"lexical"}),
                        )?;
                        if result["result"]["isError"] == true {
                            assert!(
                                common::read_retry::classify(&result["result"]).is_some(),
                                "{result}"
                            );
                            continue;
                        }
                        let payload = evidence(result)?;
                        let generation = payload["generation"].as_u64().expect("generation");
                        assert!(
                            generation >= last_generation,
                            "one reader regressed publication"
                        );
                        last_generation = generation;
                        assert_eq!(payload["scope_observation"], expected_scope);
                        assert_eq!(payload["publication"]["generation"], generation);
                        assert_eq!(
                            payload["publication"]["incarnation"],
                            old_publication["incarnation"]
                        );
                        assert!(payload["publication"]["token"].is_string());
                        assert_eq!(payload["collection"], "project-a");
                        assert_eq!(payload["hits"][0]["citation"]["path"], "auth.py");
                        assert_eq!(
                            payload["hits"][0]["text"],
                            if generation == old_generation {
                                old_text
                            } else {
                                new_text
                            }
                        );
                        observations.lock().unwrap().push(payload);
                    }
                    Ok(())
                })
            })
            .collect();
        barrier.wait();
        // Submit the update once; later observation failure cannot authorize
        // replaying an already accepted native mutation.
        let published = index()?;
        for reader in readers {
            reader.join().expect("reader panicked")?;
        }
        Ok(published)
    })?;
    // One accepted Index may publish lexical evidence and then semantic
    // enrichment. Validate every exact revision, not an obsolete one-job/one-
    // generation assumption. No model or background work is disabled.
    let final_generation = published["generation"].as_u64().expect("final generation");
    assert!(final_generation > old_generation);
    let mut revisions = std::collections::BTreeMap::new();
    revisions.insert(old_generation, old_publication);
    revisions.insert(final_generation, published["publication"].clone());
    let observations = observations.into_inner().unwrap();
    assert!(
        !observations.is_empty(),
        "fanout returned no completed reads"
    );
    for observed in observations {
        let generation = observed["generation"].as_u64().unwrap();
        assert!(
            generation <= final_generation,
            "read exceeded settled final publication"
        );
        if let Some(previous) = revisions.insert(generation, observed["publication"].clone()) {
            assert_eq!(
                previous, observed["publication"],
                "one generation changed exact publication identity"
            );
        }
    }
    let mut shared_hash = None;
    for agent in &mut agents {
        let payload = evidence(agent.read_retry(
            100,
            "airk_search",
            json!({"query":"authenticate", "mode":"lexical"}),
        )?)?;
        assert_eq!(payload["generation"], published["generation"]);
        assert_eq!(payload["hits"][0]["text"], new_text);
        let hash = payload["hits"][0]["citation"]["content_hash"].clone();
        assert!(hash.as_str().is_some());
        if let Some(expected) = &shared_hash {
            assert_eq!(&hash, expected);
        }
        shared_hash = Some(hash);
    }
    Ok(())
}

#[test]
fn setup_outputs_client_config_and_mcp_exposes_specialized_tools() -> anyhow::Result<()> {
    fn stable(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                for key in [
                    "request_id",
                    "observed_unix_ms",
                    "cache_age_ms",
                    "cached",
                    "observed_from_unix_ms",
                    "observed_to_unix_ms",
                ] {
                    fields.remove(key);
                }
                fields.values_mut().for_each(stable);
            }
            Value::Array(rows) => rows.iter_mut().for_each(stable),
            _ => {}
        }
    }
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::reviewed(root.path(), state.path(), "project-a");
    fs::write(
        root.path().join("auth.py"),
        "import os\ndef authenticate():\n    return True\n",
    )?;
    fs::write(
        root.path().join("notes.md"),
        "# Authentication\nUse the authenticate function.\n",
    )?;
    fs::write(
        root.path().join("logo.svg"),
        "<svg viewBox=\"0 0 10 10\"><path id=\"logo\"/></svg>",
    )?;
    fs::write(
        root.path().join("requirements.md"),
        "# Architecture\n\n## Requirements\n\nREQ-1: The interface must be accessible in at most 200 ms by 13 October 2026.\n",
    )?;
    // This fixture exercises broad document facts retained by Rich Structure.
    // Initial native setup may still update the model policy. Complete that
    // accepted work before capturing this fixture's first configuration CAS.
    service.settle_reviewed_index(
        root.path(),
        state.path(),
        std::time::Instant::now() + Duration::from_secs(90),
    )?;
    service.configure(airk_cli::service_configuration::Draft::Detail {
        level: airk_core::extraction::IndexDetail::Balanced,
    });
    cli_call(root.path(), state.path(), "project-a", &["index"])?;
    let mut session = Session::start(root.path(), state.path())?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"airk-test","version":"1"}}}))?;
    assert_eq!(session.receive(1)?["result"]["serverInfo"]["name"], "airk");
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    session.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))?;
    let listed = session.receive(2)?;
    let tools = listed["result"]["tools"].as_array().expect("tools");
    let catalog = common::ctl::Ctl::new()
        .arg("--service-runtime-dir")
        .arg(&service.control)
        .args(["tools", "list", "--json"])
        .output()?;
    assert!(catalog.status.success());
    let catalog: Value = serde_json::from_slice(&catalog.stdout)?;
    assert_eq!(
        catalog["tools"],
        serde_json::to_value(airk_mcp::unified::tools())?
    );
    assert_query_scope_metadata_tool(catalog["tools"].as_array().unwrap(), "query_scope", true);
    // Every tokenless HTTPS MCP session is the local agent session, which
    // lists only the core retrieval tools, without scope arguments.
    assert_eq!(
        listed["result"]["tools"],
        serde_json::to_value(airk_mcp::open::agent_catalog())?,
        "actual MCP must list exactly the agent catalog"
    );
    assert_local_mcp_catalog_names(tools);
    let names: std::collections::BTreeSet<_> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    // MCP offers no query_scope (27 November 2026); the CLI keeps it.
    assert!(!tools.iter().any(|tool| tool["name"] == "query_scope"));
    // MCP lists a subset of the CLI's complete catalog: exactly the tools it
    // offers (27 November 2026). The CLI keeps every other operation.
    let cli_names: std::collections::BTreeSet<_> = catalog["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    for name in &names {
        assert!(cli_names.contains(name), "MCP tool {name} missing from the CLI catalog");
        assert!(airk_core::tool_names::on_mcp(name), "MCP lists {name}");
    }
    let drafts = [
        "draft_search",
        "draft_strategy",
        "draft_plan",
        "suggest_view",
    ];
    for tool in tools {
        assert_eq!(
            tool["annotations"]["readOnlyHint"],
            json!(!drafts.contains(&tool["name"].as_str().unwrap()))
        );
    }
    source_runtime::pause_and_wait(&session._service)?;
    for (i, name, args) in [
        (
            3,
            "airk_symbols",
            json!({"name":"authenticate","language":"python"}),
        ),
        (4, "airk_imports", json!({"path_prefix":"auth"})),
        (5, "airk_outline", json!({"path":"auth.py"})),
        (
            6,
            "airk_document_facts",
            json!({"language":"svg","name":"id=logo"}),
        ),
        (7, "airk_file_info", json!({"path":"auth.py"})),
        (8, "airk_context", json!({"path":"auth.py","line":2})),
        (9, "airk_references", json!({"name":"authenticate"})),
        (
            10,
            "airk_search",
            json!({"query":"authenticate", "mode":"lexical"}),
        ),
        (11, "airk_list_files", json!({})),
        (12, "airk_status", json!({})),
        (13, "airk_capabilities", json!({})),
        (
            14,
            "airk_read_file",
            json!({"path":"auth.py","start_line":2,"end_line":3}),
        ),
        (
            15,
            "airk_code_facts",
            json!({"name":"authenticate","kind":"definition","compact":false}),
        ),
        (
            16,
            "airk_relationships",
            json!({"symbol":"file:auth.py","direction":"outgoing","kind":"import"}),
        ),
        (
            17,
            "airk_changes",
            json!({"selectors":[{"kind":"path","path":"auth.py"}]}),
        ),
    ] {
        let result = if name == "airk_status" {
            let project = airk_kernel::service_projects::registration(
                root.path(),
                state.path(),
                "project-a",
            )?;
            session.published_status(i, &project.binding.to_string())?
        } else {
            session.read_retry(i, name, args.clone())?
        };
        assert!(result.get("error").is_none(), "{name}: {result}");
        assert_ne!(result["result"]["isError"], true, "{name}: {result}");
        let payload: Value = serde_json::from_str(
            result["result"]["content"][0]["text"]
                .as_str()
                .expect("JSON payload"),
        )?;
        let mut direct: Value = if name == "airk_status" {
            common::published_status(root.path(), state.path(), "project-a")?
        } else {
            // U110: the open `tools call`, the service path HTTPS MCP takes.
            let envelope = open_call(root.path(), state.path(), "project-a", name, &args)?;
            serde_json::from_str(envelope["content"][0]["text"].as_str().unwrap())?
        };
        if name != "airk_status" {
            // Status includes changing quota/usage observations.
            let mut expected = payload.clone();
            for value in [&mut direct, &mut expected] {
                stable(value);
            }
            assert_eq!(direct, expected, "CLI/MCP result mismatch for {name}");
        }
        if name != "airk_capabilities" {
            assert_eq!(
                payload
                    .get("collection")
                    .or_else(|| payload["evidence"].get("collection"))
                    .or_else(|| payload["publication"].get("collection"))
                    .or_else(|| payload["current_publication"].get("collection")),
                Some(&json!("project-a"))
            );
        }
        if name == "airk_symbols" {
            assert_eq!(payload["items"][0]["name"], "authenticate");
        }
        if name == "airk_document_facts" {
            assert_eq!(payload["items"][0]["name"], "id=logo");
        }
        if name == "airk_code_facts" {
            assert_eq!(payload["items"][0]["code"]["basis"], "syntax");
            assert!(
                payload["items"][0]["code"]["signature"]
                    .as_str()
                    .expect("signature")
                    .contains("authenticate")
            );
        }
        if name == "airk_relationships" {
            assert_eq!(payload["items"][0]["resolution"], "syntax_candidate");
        }
    }
    let budget = session.call(
        20,
        "airk_search",
        json!({"query":"authenticate","max_bytes":999999}),
    )?;
    assert_eq!(budget["result"]["isError"], true);
    let escape = session.call(
        21,
        "airk_read_file",
        json!({"path":"../outside","start_line":1,"end_line":1}),
    )?;
    assert_eq!(escape["result"]["isError"], true);
    let unknown = session.call(
        22,
        "airk_symbols",
        json!({"name":"authenticate","unsupported":true}),
    )?;
    assert!(unknown.get("error").is_some() || unknown["result"]["isError"] == true);
    let requirements = session.read_retry(
        30,
        "airk_document_facts",
        json!({"kind":"requirement","path_prefix":"requirements","compact":false}),
    )?;
    assert_ne!(requirements["result"]["isError"], true);
    let payload: Value = serde_json::from_str(
        requirements["result"]["content"][0]["text"]
            .as_str()
            .expect("JSON"),
    )?;
    assert_eq!(payload["items"].as_array().expect("facts").len(), 1);
    assert_eq!(payload["items"][0]["document"]["requirement_id"], "REQ-1");
    assert_eq!(
        payload["items"][0]["document"]["quantities"][0]["comparator"],
        "lte"
    );
    assert_eq!(
        payload["items"][0]["document"]["dates"][0]["normalized"],
        "2026-10-13"
    );
    assert_eq!(payload["items"][0]["citation"]["start_line"], 5);
    Ok(())
}

#[path = "support/current_recovery.rs"]
mod current_recovery;

#[test]
fn held_quota_ledger_preserves_same_read_hint_through_native_cli_and_mcp() -> anyhow::Result<()> {
    fn hint(reply: &Value, expected_scope: &Value) -> anyhow::Result<Value> {
        open_hint(reply, expected_scope, None)
    }
    /// U110: an open read also reports how its scope was named (`scope`);
    /// when `open_scope` is given that exact report is required, then the
    /// remaining hint is the same complete retry contract.
    fn open_hint(
        reply: &Value,
        expected_scope: &Value,
        open_scope: Option<&Value>,
    ) -> anyhow::Result<Value> {
        anyhow::ensure!(reply.is_object() && reply["isError"] == true, "{reply}");
        let blocks = reply["content"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("{reply}"))?;
        anyhow::ensure!(blocks.len() == 1 && blocks[0]["type"] == "text", "{reply}");
        let mut value: Value = serde_json::from_str(
            blocks[0]["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("{reply}"))?,
        )?;
        if let Some(open_scope) = open_scope
            && let Some(scope) = value
                .as_object_mut()
                .and_then(|fields| fields.remove("read_scope"))
        {
            anyhow::ensure!(&scope == open_scope, "{reply}");
        }
        anyhow::ensure!(
            value
                == json!({"status":"retry","retryable":true,"code":"selection_busy",
            "retry_after_ms":25,"action":"repeat_same_request", "scope_observation":expected_scope}),
            "{reply}"
        );
        Ok(value)
    }
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original = "QuotaLedgerSentinel original retained source.\n";
    fs::write(root.path().join("one.txt"), original)?;
    let service = common::Service::reviewed(root.path(), data.path(), "project-a");
    // Initial native setup may still update the model policy. Complete that
    // accepted work before capturing this fixture's first configuration CAS.
    service.settle_reviewed_index(
        root.path(),
        data.path(),
        std::time::Instant::now() + Duration::from_secs(90),
    )?;
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(airk_core::service::JobKind::Index);
    assert!(
        matches!(service.request(airk_core::service::ControlMethod::Pause {
        project: service.project.binding, paused: true
    }), airk_core::service::ControlResponse::Registered { project } if project.paused)
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    // The reviewed fixture's window selection, read in process through the
    // pipe and through the session-routed CLI (`scoped_command`).
    let direct = common::pipe::PipeTools::window(&service.control, service.window()?);
    let args = json!({"response_mode":"full"});
    let baseline = runtime.block_on(airk_mcp::remote::call_observed(
        direct.as_ref(),
        "airk_diagnostics".into(),
        args.as_object().unwrap().clone(),
    ))?;
    anyhow::ensure!(baseline["isError"] != true, "{baseline}");
    let baseline: Value = serde_json::from_str(baseline["content"][0]["text"].as_str().unwrap())?;
    let expected_scope = baseline["scope_observation"].clone();
    anyhow::ensure!(expected_scope.is_object(), "{baseline}");
    let publication = baseline["publication"]["token"].clone();
    anyhow::ensure!(publication.is_string(), "{baseline}");
    let mut session = Session::start(root.path(), data.path())?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"quota-read-admission","version":"1"}}}))?;
    anyhow::ensure!(session.receive(1)?.get("error").is_none());
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    let warm = session.call(2, "airk_diagnostics", args)?;
    anyhow::ensure!(
        warm.get("error").is_none() && warm["result"]["isError"] != true,
        "{warm}"
    );
    // U110: HTTPS MCP reads the fixture's index through the open path; its
    // scope observation is the open selection's, which the open `tools call`
    // shares. The scoped native and CLI reads keep the reviewed session's.
    let warm = native_payload(&warm["result"])?;
    let open_observation = warm["scope_observation"].clone();
    anyhow::ensure!(open_observation.is_object(), "{warm}");
    let open_scope = warm["read_scope"].clone();
    anyhow::ensure!(open_scope["source"] == "named", "{warm}");
    assert_eq!(warm["publication"]["token"], publication);
    let ledger = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(std::path::Path::new(&service.project.data_root).join(".quota-1.4/ledger.lock"))?;
    let until = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match ledger.try_lock() {
            Ok(()) => break,
            Err(fs::TryLockError::WouldBlock) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) => return Err(error.into()),
        }
    }
    for (id, mode) in [(3, "terse"), (4, "full")] {
        let args = json!({"response_mode":mode});
        // native read: exactly once while holding the real ledger, so a typed
        // complete rejection is observed rather than hidden by fixture retries.
        let native = runtime.block_on(airk_mcp::remote::call_observed(
            direct.as_ref(),
            "airk_diagnostics".into(),
            args.as_object().unwrap().clone(),
        ))?;
        hint(&native, &expected_scope)?;
        let encoded = args.to_string();
        let mut command = common::scoped_command(root.path(), data.path(), "project-a");
        command.args(["tools", "call", "airk_diagnostics", "--args", &encoded]);
        // native read: single complete refusal, never auto-repeat under held lock.
        let output = common::read_retry::once_until(
            &command,
            std::time::Instant::now() + Duration::from_secs(60),
        )?;
        anyhow::ensure!(
            output.status.code() == Some(1),
            "stdout={};stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let cli: Value = serde_json::from_slice(&output.stdout)?;
        hint(&cli, &expected_scope)?;
        assert_eq!(native, cli, "complete native/CLI refusal differs");
        // native read: the open `tools call`, once, under the held lock.
        let open_read = open_command(
            root.path(),
            data.path(),
            "project-a",
            "airk_diagnostics",
            &args,
        );
        let output = common::read_retry::once_until(
            &open_read,
            std::time::Instant::now() + Duration::from_secs(60),
        )?;
        anyhow::ensure!(
            output.status.code() == Some(1),
            "stdout={};stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let open: Value = serde_json::from_slice(&output.stdout)?;
        open_hint(&open, &open_observation, Some(&open_scope))?;
        // native read: single MCP response has the same complete error contract.
        let wire = session.call(id, "airk_diagnostics", args)?;
        anyhow::ensure!(wire.get("error").is_none(), "{wire}");
        open_hint(&wire["result"], &open_observation, Some(&open_scope))?;
        let mut legacy_wire = open.clone();
        if let Some(kind) = legacy_wire.as_object_mut().unwrap().remove("resultType") {
            assert_eq!(
                kind, "complete",
                "only the SDK complete-result discriminator may vary"
            );
        }
        assert_eq!(
            legacy_wire, wire["result"],
            "complete open CLI/MCP refusal differs"
        );
    }
    ledger.unlock()?;
    drop(ledger);
    let args = json!({"response_mode":"full"});
    let recovered = runtime.block_on(airk_mcp::remote::call_observed(
        direct.as_ref(),
        "airk_diagnostics".into(),
        args.as_object().unwrap().clone(),
    ))?;
    anyhow::ensure!(recovered["isError"] != true, "{recovered}");
    let recovered: Value = serde_json::from_str(recovered["content"][0]["text"].as_str().unwrap())?;
    assert_eq!(recovered["publication"]["token"], publication);
    assert_eq!(fs::read_to_string(root.path().join("one.txt"))?, original);
    Ok(())
}
