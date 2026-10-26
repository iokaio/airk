// SPDX-License-Identifier: Apache-2.0
//! Original command-level fixtures for explicit retention policy and maintenance.
use anyhow::Result;
mod common;
use serde_json::Value;
use std::{fs, path::Path, process::Output};

fn run(root: &Path, state: &Path, args: &[&str]) -> Result<Output> {
    Ok(common::command(root, state, "default")
        .args(args)
        .output()?)
}
fn call(root: &Path, state: &Path, args: &[&str]) -> Result<Value> {
    common::call(root, state, "default", args)
}
#[test]
fn current_maintenance_replaces_retention_and_preserves_sources_and_token() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let service = common::Service::start(root.path(), state.path(), "default");
    fs::write(root.path().join("note.txt"), "original source")?;
    call(root.path(), state.path(), &["index"])?;
    let before = common::published_status(root.path(), state.path(), "default")?;
    service.repair(airk_core::service_maintenance::RepairOperation::Auto);
    assert_eq!(
        common::published_status(root.path(), state.path(), "default")?["publication"],
        before["publication"]
    );
    for args in [
        vec!["collections", "retention", "show"],
        vec!["collections", "retention", "run", "--apply"],
        vec!["collections", "prune", "--apply"],
    ] {
        let output = run(root.path(), state.path(), &args)?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized"));
    }
    assert_eq!(
        fs::read_to_string(root.path().join("note.txt"))?,
        "original source"
    );
    Ok(())
}
