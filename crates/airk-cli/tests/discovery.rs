// SPDX-License-Identifier: Apache-2.0
//! Shipped command parsing and MCP schemas share discovery continuation semantics.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{fs, process::Command};

#[test]
fn cli_and_mcp_resume_the_same_compact_outline_and_fact_pages() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(
        root.path().join("many.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )?;
    fs::write(
        root.path().join("other.txt"),
        "Original discovery fixture.\n",
    )?;
    let cli = |arguments: &[&str]| -> Result<Value> {
        let output = Command::new(env!("CARGO_BIN_EXE_airk"))
            .arg("--root")
            .arg(root.path())
            .arg("--data-dir")
            .arg(state.path())
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let tool = |name: &str, arguments: Value| -> Result<Value> {
        let encoded = serde_json::to_string(&arguments)?;
        let envelope = cli(&["tools", "call", name, "--args", &encoded])?;
        ensure!(envelope["isError"] != true, "{envelope}");
        Ok(serde_json::from_str(
            envelope["content"][0]["text"]
                .as_str()
                .context("tool content")?,
        )?)
    };
    cli(&["index"])?;
    let first = cli(&["files", "--limit", "1"])?;
    let next = tool(
        "airk_list_files",
        json!({"limit":1,"cursor":first["cursor"]}),
    )?;
    assert_eq!(first["paths"], json!(["many.rs"]));
    assert_eq!(next["paths"], json!(["other.txt"]));
    assert_eq!(next["truncated"], false);

    let first = cli(&["outline", "many.rs", "--compact", "--limit", "1"])?;
    let next = tool(
        "airk_outline",
        json!({"path":"many.rs","compact":true,"limit":1,"cursor":first["cursor"]}),
    )?;
    assert_eq!(first["minimal"]["items"][0]["name"], "alpha");
    assert_eq!(next["items"][0]["name"], "beta");
    assert!(next.get("facts").is_none());
    assert_eq!(next["truncated"], false);
    let direct = cli(&[
        "outline",
        "many.rs",
        "--compact",
        "--limit",
        "1",
        "--cursor",
        first["cursor"].as_str().context("outline cursor")?,
    ])?;
    assert_eq!(next["items"], direct["minimal"]["items"]);

    let first = cli(&["code-facts", "--kind", "definition", "--limit", "1"])?;
    let next = tool(
        "airk_code_facts",
        json!({"kind":"definition","compact":false,"limit":1,"cursor":first["cursor"]}),
    )?;
    assert_eq!(first["facts"][0]["name"], "alpha");
    assert_eq!(next["items"][0]["name"], "beta");
    assert_eq!(next["truncated"], false);
    let schema = cli(&["tools", "help", "airk_outline", "--json"])?;
    // The public help schema exposes the additive inputs used above.
    let schema_text = serde_json::to_string(&schema)?;
    assert!(schema_text.contains("compact") && schema_text.contains("cursor"));
    Ok(())
}
