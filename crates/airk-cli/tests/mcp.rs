// SPDX-License-Identifier: Apache-2.0
//! Real stdio protocol lifecycle and collection-scoped tools through the shipped binary.
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
        let mut child = Command::new(env!("CARGO_BIN_EXE_airk"))
            .args([
                "--root",
                root.to_str().expect("test root"),
                "--data-dir",
                state.to_str().expect("test state"),
                "--collection",
                "project-a",
                "mcp",
            ])
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
        for _ in 0..10 {
            let line = self.output.recv_timeout(Duration::from_secs(10))?;
            let value: Value = serde_json::from_str(&line)?;
            if value["id"] == id {
                return Ok(value);
            }
        }
        anyhow::bail!("response not received")
    }
    fn call(&mut self, id: u64, name: &str, arguments: Value) -> anyhow::Result<Value> {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}}))?;
        self.receive(id)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn default_search_uses_vectors_and_live_mcp_connections_observe_panel_opt_out() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
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
        let output = Command::new(env!("CARGO_BIN_EXE_airk"))
            .arg("--root")
            .arg(root.path())
            .arg("--data-dir")
            .arg(state.path())
            .args(["--collection", "project-a"])
            .args(args)
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    cli(&["index"])?;
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
    let first = evidence(session.call(2, "airk_search", json!({"query":"login"}))?)?;
    assert_eq!(first["hits"][0]["citation"]["path"], "auth.txt");
    cli(&["collections", "vectors", "set", "--enabled", "false"])?;
    let disabled = evidence(session.call(3, "airk_search", json!({"query":"login"}))?)?;
    assert_eq!(disabled["hits"], json!([]));
    assert_eq!(first["publication"], disabled["publication"]);
    assert_eq!(
        session.call(4, "airk_search", json!({"query":"login","mode":"vector"}))?["result"]["isError"],
        true
    );
    cli(&["collections", "vectors", "set", "--enabled", "true"])?;
    assert_eq!(
        evidence(session.call(5, "airk_search", json!({"query":"login"}))?)?["hits"][0]["citation"]
            ["path"],
        "auth.txt"
    );
    Ok(())
}

#[test]
fn durable_key_reads_match_cli_and_mcp_and_reject_filter_combinations() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(
        root.path().join("original.rs"),
        "pub fn original_evidence() -> u32 { 7 }\n",
    )?;
    fs::write(
        root.path().join("original.md"),
        "# Original evidence\n\nThe original client must retain its local evidence.\n",
    )?;
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_airk"))
            .arg("--root")
            .arg(root.path())
            .arg("--data-dir")
            .arg(state.path())
            .args(["--collection", "project-a"])
            .env("AIRK_USAGE_MODE", "disabled")
            .args(args)
            .output()
    };
    let cli = |args: &[&str]| -> anyhow::Result<Value> {
        let output = run(args)?;
        anyhow::ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    cli(&["collections", "extraction", "detail", "detailed"])?;
    cli(&["index"])?;
    let code = cli(&["code-facts", "--name", "original_evidence"])?;
    let symbol_key = code["facts"][0]["continuity_key"]["key"]
        .as_str()
        .expect("qualified symbol key");
    let documents = cli(&["facts", "--kind", "section"])?;
    let document_key = documents["facts"][0]["continuity_key"]["key"]
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
        16
    );
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
        let mut expected = cli(&[command, option, key, "--limit", "10"])?;
        let result = session.call(id, tool, json!({(field):key,"limit":10}))?;
        assert_ne!(result["result"]["isError"], true, "{result}");
        let mut actual: Value = serde_json::from_str(
            result["result"]["content"][0]["text"]
                .as_str()
                .expect("tool payload"),
        )?;
        for response in [&mut expected, &mut actual] {
            response.as_object_mut().unwrap().remove("request_id");
        }
        assert_eq!(actual, expected);
        assert!(!actual["facts"].as_array().unwrap().is_empty());
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
    fs::write(
        root.path().join("aurora.py"),
        "def northern_observation():\n    return 71\n",
    )?;
    let cli = |args: &[&str]| -> anyhow::Result<Value> {
        let output = Command::new(env!("CARGO_BIN_EXE_airk"))
            .arg("--root")
            .arg(root.path())
            .arg("--data-dir")
            .arg(state.path())
            .args(["--collection", "project-a"])
            .args(args)
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
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
    let mut expected = cli(&["changes", "--path", "aurora.py"])?;
    let mut actual = payload(session.call(
        2,
        "airk_changes",
        json!({"selectors":[{"kind":"path","path":"aurora.py"}]}),
    )?)?;
    for value in [&mut expected, &mut actual] {
        value.as_object_mut().unwrap().remove("request_id");
        value["freshness"]
            .as_object_mut()
            .unwrap()
            .remove("observed_unix_ms");
    }
    assert_eq!(actual, expected);
    let info = cli(&["info", "aurora.py"])?;
    let validator = info["read"]["validator"].as_str().expect("validator");
    let unchanged = payload(session.call(
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
    let old_text = "def authenticate():\n    return 'revisionone'\n";
    let new_text = "def authenticate():\n    return 'revisiontwo'\n";
    fs::write(root.path().join("auth.py"), old_text)?;
    let index = || -> anyhow::Result<Value> {
        let output = Command::new(env!("CARGO_BIN_EXE_airk"))
            .args([
                "--root",
                root.path().to_str().expect("root"),
                "--data-dir",
                state.path().to_str().expect("state"),
                "--collection",
                "project-a",
                "index",
            ])
            .output()?;
        anyhow::ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
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
    let lock_path = state.path().join("collections-1.2/project-a/writer.lock");
    let writer_lock = fs::File::create_new(&lock_path)?;
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| -> anyhow::Result<()> {
        let readers: Vec<_> = agents
            .iter_mut()
            .map(|agent| {
                scope.spawn(|| -> anyhow::Result<()> {
                    barrier.wait();
                    let payload = evidence(agent.call(
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
    fs::remove_file(&lock_path)?;

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
                                result.to_string().contains("busy")
                                    || result.to_string().contains("active readers"),
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
        let payload = evidence(agent.call(
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
    fs::write(root.path().join("a.txt"), "fixture")?;
    for client in [
        "codex",
        "vscode",
        "cursor",
        "opencode",
        "opencode-v2",
        "copilot-cli",
        "kilo",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_airk"))
            .args([
                "--root",
                root.path().to_str().expect("root"),
                "--data-dir",
                state.path().to_str().expect("state"),
                "--collection",
                "project-a",
                "setup",
                "--client",
                client,
            ])
            .output()?;
        assert!(
            output.status.success(),
            "{client}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout)?;
        if client == "codex" {
            assert!(text.starts_with("[mcp_servers.airk]"));
            assert!(text.contains("project-a"));
            continue;
        }
        let config: Value = serde_json::from_str(&text)?;
        let server = match client {
            "vscode" => &config["servers"]["airk"],
            "opencode" | "kilo" => &config["mcp"]["airk"],
            "opencode-v2" => &config["mcp"]["servers"]["airk"],
            _ => &config["mcpServers"]["airk"],
        };
        let args = server
            .get("args")
            .unwrap_or(&server["command"])
            .as_array()
            .expect("separate arguments");
        assert!(args.contains(&json!("project-a")));
        assert_eq!(args.last(), Some(&json!("mcp")));
        if client == "vscode" || client == "cursor" {
            assert_eq!(server["type"], "stdio");
        }
    }
    Ok(())
}

#[test]
fn setup_outputs_client_config_and_mcp_exposes_specialized_tools() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
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
    let configured = Command::new(env!("CARGO_BIN_EXE_airk"))
        .arg("--root")
        .arg(root.path())
        .arg("--data-dir")
        .arg(state.path())
        .args([
            "--collection",
            "project-a",
            "collections",
            "extraction",
            "detail",
            "balanced",
        ])
        .output()?;
    anyhow::ensure!(
        configured.status.success(),
        "{}",
        String::from_utf8_lossy(&configured.stderr)
    );
    let setup = Command::new(env!("CARGO_BIN_EXE_airk"))
        .args([
            "--root",
            root.path().to_str().expect("root"),
            "--data-dir",
            state.path().to_str().expect("state"),
            "--collection",
            "project-a",
            "setup",
        ])
        .output()?;
    assert!(
        setup.status.success(),
        "{}",
        String::from_utf8_lossy(&setup.stderr)
    );
    let config: Value = serde_json::from_slice(&setup.stdout)?;
    assert!(
        config["mcpServers"]["airk"]["args"]
            .as_array()
            .expect("args")
            .contains(&json!("project-a"))
    );
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
    let mut advertised = tools.clone();
    advertised.sort_by_key(|tool| tool["name"].as_str().unwrap().to_owned());
    assert_eq!(catalog["tools"], json!(advertised));
    assert_eq!(tools.len(), 16);
    assert!(
        tools
            .iter()
            .all(|t| t["annotations"]["readOnlyHint"] == true)
    );
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
            json!({"name":"authenticate","kind":"definition"}),
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
        let result = session.call(i, name, args.clone())?;
        assert!(result.get("error").is_none(), "{name}: {result}");
        assert_ne!(result["result"]["isError"], true, "{name}: {result}");
        let payload: Value = serde_json::from_str(
            result["result"]["content"][0]["text"]
                .as_str()
                .expect("JSON payload"),
        )?;
        let direct = Command::new(env!("CARGO_BIN_EXE_airk"))
            .arg("--root")
            .arg(root.path())
            .arg("--data-dir")
            .arg(state.path())
            .args([
                "--collection",
                "project-a",
                "tools",
                "call",
                name,
                "--args",
                &args.to_string(),
            ])
            .env("AIRK_USAGE_MODE", "disabled")
            .output()?;
        assert!(
            direct.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&direct.stderr)
        );
        let direct: Value = serde_json::from_slice(&direct.stdout)?;
        let mut direct: Value =
            serde_json::from_str(direct["content"][0]["text"].as_str().unwrap())?;
        if name != "airk_status" {
            // Status includes changing quota/usage observations.
            let mut expected = payload.clone();
            expected.as_object_mut().unwrap().remove("request_id");
            if direct.get("freshness").is_some() {
                // Each call observes scheduler freshness at its own wall-clock instant.
                assert!(direct["freshness"]["observed_unix_ms"].is_number());
                direct["freshness"]["observed_unix_ms"] = Value::Null;
                expected["freshness"]["observed_unix_ms"] = Value::Null;
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
            assert_eq!(payload["facts"][0]["name"], "authenticate");
        }
        if name == "airk_document_facts" {
            assert_eq!(payload["facts"][0]["name"], "id=logo");
        }
        if name == "airk_code_facts" {
            assert_eq!(payload["facts"][0]["code"]["basis"], "syntax");
            assert!(
                payload["facts"][0]["code"]["signature"]
                    .as_str()
                    .expect("signature")
                    .contains("authenticate")
            );
        }
        if name == "airk_relationships" {
            assert_eq!(
                payload["relationships"][0]["resolution"],
                "syntax_candidate"
            );
        }
    }
    let unsupported = session.call(18, "airk_history", json!({"limit":2,"max_bytes":4096}))?;
    assert_eq!(unsupported["result"]["isError"], true);
    assert!(unsupported.to_string().contains("unsupported"));
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
    let requirements = session.call(
        30,
        "airk_document_facts",
        json!({"kind":"requirement","path_prefix":"requirements"}),
    )?;
    assert_ne!(requirements["result"]["isError"], true);
    let payload: Value = serde_json::from_str(
        requirements["result"]["content"][0]["text"]
            .as_str()
            .expect("JSON"),
    )?;
    assert_eq!(payload["facts"].as_array().expect("facts").len(), 1);
    assert_eq!(payload["facts"][0]["document"]["requirement_id"], "REQ-1");
    assert_eq!(
        payload["facts"][0]["document"]["quantities"][0]["comparator"],
        "lte"
    );
    assert_eq!(
        payload["facts"][0]["document"]["dates"][0]["normalized"],
        "2026-10-13"
    );
    assert_eq!(payload["facts"][0]["citation"]["start_line"], 5);
    Ok(())
}

#[path = "support/current_recovery.rs"]
mod current_recovery;
