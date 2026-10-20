// SPDX-License-Identifier: Apache-2.0
//! Original command fixtures for exact retained reads and explicit history controls.
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn run(root: &Path, data: &Path, args: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_airk"))
        .arg("--root")
        .arg(root)
        .arg("--data-dir")
        .arg(data)
        .env("AIRK_USAGE_MODE", "disabled")
        .args(args)
        .output()?)
}
fn call(root: &Path, data: &Path, args: &[&str]) -> Result<Value> {
    let output = run(root, data, args)?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn cli_current_revision_expiry_and_unsupported_history_leave_sources_untouched() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("original.py"),
        "def original():\n    return 17\n",
    )?;
    call(root.path(), data.path(), &["index"])?;
    let first = call(root.path(), data.path(), &["info", "original.py"])?;
    let old = first["read"]["publication"]["token"].as_str().unwrap();
    fs::write(
        root.path().join("original.py"),
        "def original():\n    return 29\n",
    )?;
    call(root.path(), data.path(), &["index"])?;
    let current = call(root.path(), data.path(), &["info", "original.py"])?;
    let current = current["read"]["publication"]["token"].as_str().unwrap();
    fs::write(root.path().join("original.py"), "LIVE_UNINDEXED_SENTINEL\n")?;
    for args in [
        vec![
            "read",
            "original.py",
            "--start-line",
            "1",
            "--end-line",
            "2",
        ],
        vec!["context", "original.py", "2"],
        vec!["info", "original.py"],
        vec!["outline", "original.py"],
    ] {
        let mut exact = args.clone();
        exact.extend(["--revision", old]);
        let output = run(root.path(), data.path(), &exact)?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("expired"));
        let mut exact = args;
        exact.extend(["--revision", current]);
        let response = call(root.path(), data.path(), &exact)?;
        assert_eq!(response["read"]["publication"]["token"], current);
        assert_eq!(response["read"]["historical"], false);
        assert!(!response.to_string().contains("LIVE_UNINDEXED"));
    }
    for args in [
        vec!["collections", "history", "show"],
        vec!["collections", "history", "available"],
        vec!["collections", "checkpoints", "list"],
    ] {
        assert!(!run(root.path(), data.path(), &args)?.status.success());
    }
    assert_eq!(
        fs::read_to_string(root.path().join("original.py"))?,
        "LIVE_UNINDEXED_SENTINEL\n"
    );
    Ok(())
}
