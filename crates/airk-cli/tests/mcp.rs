// SPDX-License-Identifier: Apache-2.0
//! Real stdio protocol lifecycle and collection-scoped tools through the shipped binary.
mod common;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::Duration,
};

struct Session {
    child: Child,
    input: ChildStdin,
    output: Receiver<String>,
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
        let mut command = common::command(root, state, "project-a");
        if let Some(mode) = mode {
            command.env("AIRK_MCP_RESPONSE_MODE", mode);
        } else {
            command.env_remove("AIRK_MCP_RESPONSE_MODE");
        }
        let mut child = command
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (sender, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            output,
        })
    }
    fn send(&mut self, value: Value) -> anyhow::Result<()> {
        writeln!(self.input, "{value}")?;
        self.input.flush()?;
        Ok(())
    }
    fn receive(&self, id: u64) -> anyhow::Result<Value> {
        self.receive_until(id, std::time::Instant::now() + Duration::from_secs(60))
    }
    fn receive_until(&self, id: u64, deadline: std::time::Instant) -> anyhow::Result<Value> {
        for _ in 0..10 {
            let remaining = deadline
                .checked_duration_since(std::time::Instant::now())
                .ok_or_else(|| anyhow::anyhow!("stdio observation deadline exhausted"))?;
            let line = self.output.recv_timeout(remaining)?;
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "stdio observation deadline exhausted"
            );
            let value: Value = serde_json::from_str(&line)?;
            if value["id"] == id {
                return Ok(value);
            }
        }
        anyhow::bail!("response not received")
    }
    fn call(&mut self, id: u64, name: &str, arguments: Value) -> anyhow::Result<Value> {
        let name = name.strip_prefix("airk_").unwrap_or(name);
        self.send(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}}))?;
        self.receive(id)
    }
    fn published_status(&mut self, id: u64, project: &str) -> anyhow::Result<Value> {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "publication observation deadline exhausted"
            );
            self.send(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"airk_diagnostics","arguments":{"response_mode":"full"}}}))?;
            let reply = self.receive_until(id, deadline)?;
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
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    fn read_retry(&mut self, id: u64, name: &str, arguments: Value) -> anyhow::Result<Value> {
        let until = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let reply = self.call(id, name, arguments.clone())?;
            let Some(retry) = common::read_retry::within(&reply["result"], until) else {
                return Ok(reply);
            };
            std::thread::sleep(retry.delay);
        }
    }
    fn wait_source_idle(&mut self, id: u64) -> anyhow::Result<()> {
        self.wait_source_state(id, "idle")
    }
    fn wait_source_state(&mut self, id: u64, state: &str) -> anyhow::Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let reply = self.read_retry(id, "status", json!({"response_mode":"full"}))?;
            anyhow::ensure!(
                reply.get("error").is_none() && reply["result"]["isError"] != true,
                "{reply}"
            );
            let observed: Value = serde_json::from_str(
                reply["result"]["content"][0]["text"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing status: {reply}"))?,
            )?;
            if observed["freshness"]["state"] == state {
                return Ok(());
            }
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "fixture source work did not settle: {observed}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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
    let service = common::Service::start(root.path(), state.path(), "project-a");
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
    common::call(root.path(), state.path(), "project-a", &["index"])?;
    let mut session = Session::start(root.path(), state.path())?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"verification-parity-fixture","version":"1"}}}))?;
    assert!(session.receive(1)?.get("error").is_none());
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    session.wait_source_idle(2)?;
    let args = json!({"path":"policy.md","verification":"report","verification_max_sources":25,"verification_max_bytes":16777216,"verification_deadline_ms":2000,"response_mode":"full"});
    let mcp = session.read_retry(3, "file_info", args.clone())?;
    assert_ne!(mcp["result"]["isError"], true, "{mcp}");
    let native = payload(&mcp["result"])?;
    assert_eq!(native["source_verification"]["status"], "verified");
    assert_eq!(
        native["source_verification"]["counts"]["bytes_read"],
        source.len()
    );
    let alias = common::call(
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
    let tools = common::call(
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
    let mut expected = native.clone();
    stable(&mut expected);
    for mut actual in [alias, payload(&tools)?] {
        stable(&mut actual);
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
    let mut default_cli = common::call(
        root.path(),
        state.path(),
        "project-a",
        &["info", "policy.md"],
    )?;
    stable(&mut default_cli);
    assert_eq!(default_cli, default_expected);
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
    let output = common::read_retry::output(
        common::command(root.path(), state.path(), "project-a").args([
            "tools",
            "call",
            "airk_parse",
            "--args",
            &serde_json::to_string(&required_plan)?,
        ]),
    )?;
    assert!(!output.status.success());
    let output: Value = serde_json::from_slice(&output.stdout)?;
    let mut actual_plan = payload(&output)?;
    stable(&mut actual_plan);
    assert_eq!(actual_plan, expected_plan);
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
            common::command(root.path(), state.path(), "project-a").args(&invocation),
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
        let mut expected = expected_failure.clone();
        stable(&mut actual);
        stable(&mut expected);
        assert_eq!(actual, expected);
    }
    let mut shell = common::command(root.path(), state.path(), "project-a")
        .args(["tools", "shell"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    writeln!(
        shell.stdin.take().unwrap(),
        "call airk_file_info {}\nquit",
        serde_json::to_string(&require)?
    )?;
    let shell = shell.wait_with_output()?;
    assert!(shell.status.success());
    let shell_text = String::from_utf8(shell.stdout)?;
    assert!(
        shell_text.contains("verification_required_unmet")
            && shell_text.contains("source_verification")
            && shell_text.contains("mismatch")
    );
    let off = common::call(
        root.path(),
        state.path(),
        "project-a",
        &["info", "policy.md", "--verification", "off"],
    )?;
    assert!(off.get("source_verification").is_none());
    assert_eq!(off["publication"], native["publication"]);
    assert_eq!(off["content_hash"], native["content_hash"]);
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
    let service = common::Service::start(root.path(), state.path(), "project-a");
    let source = "pub struct QueueClient;\n";
    fs::write(root.path().join("client.rs"), source)?;
    let passage = format!("{} ScopeMarker\n", "background ".repeat(700));
    fs::write(root.path().join("passage.txt"), &passage)?;
    common::call(root.path(), state.path(), "project-a", &["index"])?;
    let prepared = common::call(
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
    session.wait_source_idle(19)?;
    assert!(matches!(
        service.request(airk_core::service::ControlMethod::Pause {
            project: service.project.binding,
            paused: true,
        }),
        airk_core::service::ControlResponse::Registered { project } if project.paused
    ));
    session.wait_source_state(19, "paused")?;
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
    let cli = common::call(
        root.path(),
        state.path(),
        "project-a",
        &[
            "parse",
            "--request",
            file.to_str().unwrap(),
            "--execute",
            "--project",
            &service.project.binding.to_string(),
        ],
    )?;
    assert_eq!(cli["plan"], mcp["plan"]);
    assert_eq!(cli["evidence"], mcp["evidence"]);
    assert!(!cli["evidence"].as_array().unwrap().is_empty());
    session.send(json!({"jsonrpc":"2.0","id":18,"method":"tools/list","params":{}}))?;
    let catalog = session.receive(18)?;
    let parse = catalog["result"]["tools"]
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
            assert!(
                mcp["steps"][0]["result"]["guidance"]["observations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["reason"] == "reply_omitted")
            );
        }
        fs::write(&file, serde_json::to_vec(&request)?)?;
        let cli = common::call(
            root.path(),
            state.path(),
            "project-a",
            &[
                "parse",
                "--request",
                file.to_str().unwrap(),
                "--project",
                &service.project.binding.to_string(),
            ],
        )?;
        let tool_reply = common::call(
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
        for actual in [&cli, &tool] {
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
            let alias = common::call(
                root.path(),
                state.path(),
                "project-a",
                &["symbol", "QueueClient", "--reply-detail", detail],
            )?;
            compare_native(alias, mcp["steps"][0]["result"].clone())?;
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
            assert!(
                mcp["guidance"]["observations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["reason"] == "truncated")
            );
        }
        let tool = common::call(
            root.path(),
            state.path(),
            "project-a",
            &[
                "tools",
                "call",
                "airk_search",
                "--args",
                &serde_json::to_string(&args)?,
            ],
        )?;
        let tool: Value = serde_json::from_str(tool["content"][0]["text"].as_str().unwrap())?;
        compare_native(mcp.clone(), tool)?;
        if !full {
            let alias = common::call(
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
            compare_native(mcp.clone(), alias)?;
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
        assert!(
            mcp["guidance"]["observations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["reason"] == reason)
        );
        let cli = common::call(
            root.path(),
            state.path(),
            "project-a",
            &[
                "tools",
                "call",
                tool,
                "--args",
                &serde_json::to_string(&args)?,
            ],
        )?;
        let mut cli: Value = serde_json::from_str(cli["content"][0]["text"].as_str().unwrap())?;
        stable(&mut mcp);
        stable(&mut cli);
        assert_eq!(mcp, cli);
    }
    let direct = decode(session.read_retry(4, "status", json!({"response_mode":"full"}))?)?;
    assert_eq!(direct["query_concurrency"], 16);
    // native read: only this single refused status read can repeat with its explicit receipt.
    // The evidence plan above still executes once, without whole-plan replay.
    let status_request = json!({"schema":"airk.parse.request/1","mode":"execute","text":"/status","response_mode":"full"});
    let parsed =
        status_plan_with_retry(|| decode(session.call(5, "parse", status_request.clone())?))?;
    let nested = &parsed["steps"][0]["result"];
    assert_eq!(parsed["steps"][0]["status"], "complete", "{parsed}");
    for field in direct.as_object().unwrap().keys() {
        assert!(
            nested.get(field).is_some(),
            "Workbench status lost MCP field {field}: {nested}"
        );
    }
    assert_eq!(nested["query_concurrency"], 16);
    assert_eq!(nested["publication"], direct["publication"]);
    assert_eq!(nested["files"], direct["files"]);
    let cli_status = common::call(root.path(), state.path(), "project-a", &["status"])?;
    assert_eq!(cli_status["query_concurrency"], 16);
    fs::write(
        &file,
        serde_json::to_vec(
            &json!({"schema":"airk.parse.request/1","mode":"execute","text":"/status"}),
        )?,
    )?;
    // native read: the same single-status request; additional/completed steps cannot repeat.
    let cli_plan = status_plan_with_retry(|| {
        common::call(
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
fn default_terse_stdio_supports_full_and_requested_status_without_changing_revision()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let _service = common::Service::start(root.path(), state.path(), "project-a");
    fs::write(
        root.path().join("one.rs"),
        "fn response_fixture() -> bool { true }\n",
    )?;
    common::call(root.path(), state.path(), "project-a", &["index"])?;
    let mut session = Session::with_mode(root.path(), state.path(), None)?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"response-mode-fixture","version":"1"}}}))?;
    let initialized = session.receive(1)?;
    let instructions = initialized["result"]["instructions"].as_str().unwrap();
    assert!(instructions.len() <= 512 && instructions.contains("response_mode=full"));
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    let mut values: Vec<Value> = Vec::new();
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
#[cfg(feature = "embedding-worker")]
fn default_search_uses_vectors_and_live_mcp_connections_observe_panel_opt_out() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let service = common::Service::with_helper(root.path(), state.path(), "project-a");
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
        common::call(root.path(), state.path(), "project-a", args)
    };
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
    let service = common::Service::start(root.path(), state.path(), "project-a");
    fs::write(
        root.path().join("original.rs"),
        "pub fn original_evidence() -> u32 { 7 }\n",
    )?;
    fs::write(
        root.path().join("original.md"),
        "# Original evidence\n\nThe original client must retain its local evidence.\n",
    )?;
    let run = |args: &[&str]| {
        common::command(root.path(), state.path(), "project-a")
            .args(args)
            .output()
    };
    let cli = |args: &[&str]| common::call(root.path(), state.path(), "project-a", args);
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
    assert_eq!(
        session.receive(2)?["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        28
    );
    session.wait_source_idle(30)?;
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
        let mut expected = cli(&[command, option, key, "--compact", "false", "--limit", "10"])?;
        let result =
            session.read_retry(id, tool, json!({(field):key,"compact":false,"limit":10}))?;
        assert_ne!(result["result"]["isError"], true, "{result}");
        let mut actual: Value = serde_json::from_str(
            result["result"]["content"][0]["text"]
                .as_str()
                .expect("tool payload"),
        )?;
        common::read_retry::normalize_dated_readiness_pair(&mut expected, &mut actual, 2)?;
        for response in [&mut expected, &mut actual] {
            common::read_retry::evidence_for_comparison(response)?;
        }
        assert_eq!(actual, expected);
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
    let _service = common::Service::start(root.path(), state.path(), "project-a");
    fs::write(
        root.path().join("aurora.py"),
        "def northern_observation():\n    return 71\n",
    )?;
    let cli = |args: &[&str]| -> anyhow::Result<Value> {
        common::call(root.path(), state.path(), "project-a", args)
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
    session.wait_source_idle(30)?;
    let mut expected = cli(&["changes", "--path", "aurora.py"])?;
    let mut actual = payload(session.read_retry(
        2,
        "airk_changes",
        json!({"selectors":[{"kind":"path","path":"aurora.py"}]}),
    )?)?;
    common::read_retry::normalize_dated_readiness_pair(&mut expected, &mut actual, 1)?;
    for value in [&mut expected, &mut actual] {
        assert!(
            value.get("freshness").is_some(),
            "changes omitted freshness"
        );
        common::read_retry::evidence_for_comparison(value)?;
    }
    assert_eq!(actual, expected);
    let info = cli(&["info", "aurora.py"])?;
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
    let _service = common::Service::start(root.path(), state.path(), "project-a");
    let old_text = "def authenticate():\n    return 'revisionone'\n";
    let new_text = "def authenticate():\n    return 'revisiontwo'\n";
    fs::write(root.path().join("auth.py"), old_text)?;
    let index = || -> anyhow::Result<Value> {
        // Index admission retains one exact request key across bounded metadata replies.
        _service.job(airk_core::service::JobKind::Index);
        common::published_status(root.path(), state.path(), "project-a")
    };
    let old_generation = index()?["generation"].as_u64().expect("generation");
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
    for agent in &mut agents {
        let ready = evidence(agent.read_retry(
            90,
            "airk_search",
            json!({"query":"authenticate", "mode":"lexical"}),
        )?)?;
        assert_eq!(ready["hits"][0]["text"], old_text);
    }
    let collection = airk_store::service::collection::Collection::existing(
        state.path(),
        &_service.project.root,
        "project-a",
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
    let barrier = std::sync::Barrier::new(3);
    let published = std::thread::scope(|scope| -> anyhow::Result<Value> {
        let readers: Vec<_> = agents
            .iter_mut()
            .map(|agent| {
                scope.spawn(|| -> anyhow::Result<()> {
                    barrier.wait();
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
                        assert!(generation == old_generation || generation == old_generation + 1);
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
                    }
                    Ok(())
                })
            })
            .collect();
        barrier.wait();
        let started = std::time::Instant::now();
        let published = loop {
            match index() {
                Ok(result) => break result,
                Err(error) => {
                    assert!(format!("{error:#}").contains("active readers"), "{error:#}");
                    assert!(started.elapsed().as_secs() < 30);
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        };
        for reader in readers {
            reader.join().expect("reader panicked")?;
        }
        Ok(published)
    })?;
    assert_eq!(published["generation"], old_generation + 1);
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
fn client_setup_formats_preserve_collection_scope_and_command_arguments() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let _service = common::Service::start(root.path(), state.path(), "project-a");
    fs::write(root.path().join("a.txt"), "fixture")?;
    for client in [
        "claude",
        "codex",
        "vscode",
        "cursor",
        "opencode",
        "opencode-v2",
        "copilot-cli",
        "kilo",
    ] {
        let output = common::command(root.path(), state.path(), "project-a")
            .args(["setup", "--client", client])
            .output()?;
        assert!(
            !output.status.success(),
            "raw service invented an installed launcher for {client}"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("installed stable launcher"));
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
    let service = common::Service::start(root.path(), state.path(), "project-a");
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
    service.configure(airk_cli::service_configuration::Draft::Detail {
        level: airk_core::extraction::IndexDetail::Balanced,
    });
    common::call(root.path(), state.path(), "project-a", &["index"])?;
    let mut session = Session::start(root.path(), state.path())?;
    session.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"airk-test","version":"1"}}}))?;
    assert_eq!(session.receive(1)?["result"]["serverInfo"]["name"], "airk");
    session.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
    session.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))?;
    let listed = session.receive(2)?;
    let tools = listed["result"]["tools"].as_array().expect("tools");
    let catalog = Command::new(env!("CARGO_BIN_EXE_airk"))
        .args(["tools", "list", "--json"])
        .output()?;
    assert!(catalog.status.success());
    let catalog: Value = serde_json::from_slice(&catalog.stdout)?;
    assert_eq!(
        catalog["tools"].as_array().unwrap().len(),
        18,
        "terminal evidence adapter retains compatibility aliases"
    );
    assert_eq!(tools.len(), 28);
    let names: std::collections::BTreeSet<_> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 28);
    for tool in catalog["tools"].as_array().unwrap() {
        let name = tool["name"]
            .as_str()
            .unwrap()
            .strip_prefix("airk_")
            .unwrap();
        assert!(names.contains(name), "missing public evidence tool {name}");
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
    session.wait_source_idle(30)?;
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
            let envelope = common::call(
                root.path(),
                state.path(),
                "project-a",
                &["tools", "call", name, "--args", &args.to_string()],
            )?;
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

/// Pipe framing tests use metadata-only MCP startup and owned temporary state.
/// No service installation, index, or read replay is involved.
fn frame_pipe(bytes: Vec<u8>, chunk: usize) -> anyhow::Result<std::process::Output> {
    use std::io::Read;
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let mut child = common::command(root.path(), state.path(), "frame-bound")
        .arg("mcp")
        .env_remove("AIRK_PROCESS_DIAGNOSTICS")
        .env_remove("AIRK_ERROR_FORMAT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    // Keep independent drains so neither output pipe can block the process.
    let drain = |reader: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            reader
                .take(1024 * 1024)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
    };
    let out = drain(Box::new(stdout));
    let err = drain(Box::new(stderr));
    let mut input = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for part in bytes.chunks(chunk) {
            if let Err(error) = input.write_all(part) {
                if error.kind() == std::io::ErrorKind::BrokenPipe {
                    return Ok(());
                }
                return Err(error);
            }
            input.flush()?;
        }
        Ok(()) // drop stdin: real EOF
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            anyhow::bail!("bounded MCP process did not exit before deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    writer.join().expect("pipe writer")?;
    let stdout = out.join().expect("stdout drain")?;
    let stderr = err.join().expect("stderr drain")?;
    anyhow::ensure!(
        stdout.len() < 1024 * 1024 && stderr.len() < 1024 * 1024,
        "MCP fixture output exceeded its bound"
    );
    for line in stdout
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value: Value = serde_json::from_slice(line)?;
        anyhow::ensure!(
            value["jsonrpc"] == "2.0",
            "stdout must contain protocol frames only"
        );
    }
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

fn frame_initialize() -> Vec<u8> {
    format!("{}\n", json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"frame-fixture","version":"1"}
    }})).into_bytes()
}

#[test]
fn mcp_pipe_frame_bound_counts_newline_and_resets_per_frame() -> anyhow::Result<()> {
    const LIMIT: usize = airk_core::service::MAX_FRAME_BYTES;
    // JSON trailing whitespace is valid, and counts toward the physical line cap.
    for chunk in [1, 8192, LIMIT + 1] {
        let mut bytes = frame_initialize();
        bytes.pop();
        bytes.resize(LIMIT - 1, b' ');
        bytes.push(b'\n');
        bytes.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n");
        let output = frame_pipe(bytes, chunk)?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let replies: Vec<Value> = String::from_utf8(output.stdout)?
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        assert!(replies.iter().any(|reply| reply["id"] == 1));
        assert!(replies.iter().any(|reply| reply["id"] == 2));
    }
    Ok(())
}

#[test]
fn mcp_pipe_overflow_is_nonzero_bounded_and_content_free() -> anyhow::Result<()> {
    const LIMIT: usize = airk_core::service::MAX_FRAME_BYTES;
    for (newline, chunk) in [(true, 1), (true, 8192), (false, LIMIT + 1)] {
        let mut bytes = frame_initialize();
        bytes.pop();
        bytes.resize(LIMIT, b' ');
        if newline {
            bytes.push(b'\n');
        } else {
            bytes.push(b'x');
        }
        bytes.extend_from_slice(b"PRIVATE_REJECTED_PAYLOAD\n");
        let output = frame_pipe(bytes, chunk)?;
        assert!(!output.status.success());
        assert_eq!(
            String::from_utf8(output.stderr)?,
            "Error: MCP request exceeds frame bound\n"
        );
        assert!(!String::from_utf8(output.stdout)?.contains("PRIVATE_REJECTED_PAYLOAD"));
    }
    Ok(())
}

#[test]
fn mcp_pipe_clean_eof_without_newline_and_malformed_input_keep_sdk_semantics() -> anyhow::Result<()>
{
    let mut bytes = frame_initialize();
    bytes.pop();
    let output = frame_pipe(bytes, 17)?;
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let replies: Vec<Value> = String::from_utf8(output.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert!(replies.iter().any(|reply| reply["id"] == 1));

    let mut bytes = frame_initialize();
    bytes.pop();
    bytes.resize(airk_core::service::MAX_FRAME_BYTES, b' ');
    let output = frame_pipe(bytes, 8192)?;
    assert!(
        output.status.success(),
        "exact-limit unterminated JSON line is allowed"
    );
    assert!(output.stderr.is_empty());
    let replies: Vec<Value> = String::from_utf8(output.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert!(replies.iter().any(|reply| reply["id"] == 1));

    // Preserve the SDK's existing behavior: ignore unparsable JSON, then
    // handle the next complete line. Add no custom parser/resynchronization
    // and never reflect rejected bytes into stderr/stdout.
    let output = frame_pipe(
        b"PRIVATE_MALFORMED\n{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"ping\"}\n".to_vec(),
        8192,
    )?;
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let replies: Vec<Value> = String::from_utf8(output.stdout)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0]["id"], 99);
    assert!(replies[0]["result"].is_object());
    Ok(())
}
