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
    let lock_path = state.path().join("collections/project-a/writer.lock");
    let writer_lock = fs::File::create_new(&lock_path)?;
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| -> anyhow::Result<()> {
        let readers: Vec<_> = agents
            .iter_mut()
            .map(|agent| {
                scope.spawn(|| -> anyhow::Result<()> {
                    barrier.wait();
                    let payload =
                        evidence(agent.call(2, "airk_search", json!({"query":"authenticate"}))?)?;
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
                        let payload = evidence(agent.call(
                            id,
                            "airk_search",
                            json!({"query":"authenticate"}),
                        )?)?;
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
        let published = index()?;
        for reader in readers {
            reader.join().expect("reader panicked")?;
        }
        Ok(published)
    })?;
    assert_eq!(published["generation"], old_generation + 1);
    let mut shared_hash = None;
    for agent in &mut agents {
        let payload = evidence(agent.call(100, "airk_search", json!({"query":"authenticate"}))?)?;
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
    assert_eq!(tools.len(), 14);
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
        (10, "airk_search", json!({"query":"authenticate"})),
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
    ] {
        let result = session.call(i, name, args)?;
        assert!(result.get("error").is_none(), "{name}: {result}");
        assert_ne!(result["result"]["isError"], true, "{name}: {result}");
        let payload: Value = serde_json::from_str(
            result["result"]["content"][0]["text"]
                .as_str()
                .expect("JSON payload"),
        )?;
        if name != "airk_capabilities" {
            assert_eq!(
                payload
                    .get("collection")
                    .or_else(|| payload["evidence"].get("collection")),
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
