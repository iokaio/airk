// SPDX-License-Identifier: Apache-2.0
//! Original command-level fixtures for explicit retention policy and maintenance.
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn run(root: &Path, state: &Path, args: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_airk"))
        .arg("--root")
        .arg(root)
        .arg("--data-dir")
        .arg(state)
        .env("AIRK_USAGE_MODE", "disabled")
        .args(args)
        .output()?)
}
fn call(root: &Path, state: &Path, args: &[&str]) -> Result<Value> {
    let result = run(root, state, args)?;
    ensure!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(serde_json::from_slice(&result.stdout)?)
}
#[test]
fn current_maintenance_replaces_retention_and_preserves_sources_and_token() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(root.path().join("note.txt"), "original source")?;
    call(root.path(), state.path(), &["index"])?;
    let before = call(root.path(), state.path(), &["status"])?;
    let clean = call(root.path(), state.path(), &["collections", "maintain"])?;
    assert_eq!(clean["removed_files"], 0);
    assert_eq!(
        call(root.path(), state.path(), &["status"])?["publication"],
        before["publication"]
    );
    for args in [
        vec!["collections", "retention", "show"],
        vec!["collections", "retention", "run", "--apply"],
        vec!["collections", "prune", "--apply"],
    ] {
        let output = run(root.path(), state.path(), &args)?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported"));
    }
    assert_eq!(
        fs::read_to_string(root.path().join("note.txt"))?,
        "original source"
    );
    Ok(())
}
