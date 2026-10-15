// SPDX-License-Identifier: Apache-2.0
//! Original command-level fixtures for explicit retention policy and maintenance.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
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
fn policy(mode: &str) -> String {
    json!({"version":1,"mode":mode,"searchable_predecessors":1}).to_string()
}
fn generations(state: &Path) -> Result<usize> {
    Ok(fs::read_dir(state.join("collections/default/generations"))?
        .collect::<std::io::Result<Vec<_>>>()?
        .len())
}

#[test]
fn retention_preview_set_and_apply_preserve_source_and_manual_prune_semantics() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let source = root.path().join("original.py");
    fs::write(&source, "def original_aurora():\n    return 1\n")?;
    call(root.path(), state.path(), &["index"])?;
    let manual = policy("manual");
    let receipt = call(
        root.path(),
        state.path(),
        &["collections", "retention", "set", "--json", &manual],
    )?;
    assert_eq!(receipt["origin"], "explicit");
    assert_eq!(receipt["policy"]["mode"], "manual");
    for value in 2..=4 {
        fs::write(
            &source,
            format!("def original_aurora():\n    return {value}\n"),
        )?;
        call(root.path(), state.path(), &["index"])?;
    }
    assert_eq!(generations(state.path())?, 4);
    let source_bytes = fs::read(&source)?;
    let status = call(
        root.path(),
        state.path(),
        &["collections", "retention", "show"],
    )?;
    assert_eq!(status["history_supported"], false);
    assert_eq!(status["checkpoints_supported"], false);
    assert_eq!(status["policy"]["digest"], receipt["digest"]);
    let automatic = policy("automatic");
    let preview = call(
        root.path(),
        state.path(),
        &["collections", "retention", "preview", "--json", &automatic],
    )?;
    assert_eq!(preview["status"]["policy"]["policy"]["mode"], "automatic");
    assert_eq!(generations(state.path())?, 4);
    let after = call(
        root.path(),
        state.path(),
        &["collections", "retention", "show"],
    )?;
    assert_eq!(after["policy"], status["policy"]);
    assert_eq!(after["active"], status["active"]);
    let dry = call(
        root.path(),
        state.path(),
        &[
            "collections",
            "retention",
            "run",
            "--max-removal-bytes",
            "1073741824",
            "--max-duration-ms",
            "10000",
            "--max-metadata-bytes",
            "33554432",
        ],
    )?;
    assert_eq!(dry["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(generations(state.path())?, 4);
    let mut complete = false;
    for _ in 0..16 {
        let applied = call(
            root.path(),
            state.path(),
            &["collections", "retention", "run", "--apply"],
        )?;
        if applied["receipt"]["state"] == "complete" {
            complete = true;
            break;
        }
        assert_eq!(applied["receipt"]["state"], "pending", "{applied}");
        assert_eq!(applied["receipt"]["pending"], true);
    }
    assert!(
        complete,
        "bounded explicit passes must finish this four-revision fixture"
    );
    assert_eq!(generations(state.path())?, 2);
    assert_eq!(fs::read(&source)?, source_bytes);
    // Legacy manual prune is separately explicit and keeps only active evidence.
    call(
        root.path(),
        state.path(),
        &["collections", "prune", "--apply"],
    )?;
    assert_eq!(generations(state.path())?, 1);
    assert_eq!(fs::read(&source)?, source_bytes);
    Ok(())
}

#[test]
fn policy_file_and_inline_inputs_are_bounded_and_invalid_updates_leave_policy_unchanged()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(
        root.path().join("original.txt"),
        "Original retention validation fixture.\n",
    )?;
    call(root.path(), state.path(), &["index"])?;
    let input = tempfile::tempdir()?;
    let file = input.path().join("policy.json");
    fs::write(&file, policy("manual"))?;
    let file_name = file.to_str().unwrap();
    let receipt = call(
        root.path(),
        state.path(),
        &["collections", "retention", "set", file_name],
    )?;
    let invalid = json!({"version":1,"mode":"manual","searchable_predecessors":0}).to_string();
    for args in [
        vec!["collections", "retention", "set"],
        vec!["collections", "retention", "set", "--json", "{}"],
        vec!["collections", "retention", "set", "--json", &invalid],
        vec![
            "collections",
            "retention",
            "run",
            "--max-metadata-bytes",
            "16383",
        ],
        vec![
            "collections",
            "retention",
            "run",
            "--apply",
            "--max-metadata-bytes",
            "67108865",
        ],
        vec![
            "collections",
            "retention",
            "run",
            "--max-duration-ms",
            "60001",
        ],
        vec![
            "collections",
            "retention",
            "run",
            "--apply",
            "--max-removal-bytes",
            "68719476737",
        ],
        vec![
            "collections",
            "retention",
            "run",
            "--max-removal-bytes",
            "0",
        ],
        vec![
            "collections",
            "retention",
            "set",
            file_name,
            "--json",
            &policy("manual"),
        ],
    ] {
        assert!(!run(root.path(), state.path(), &args)?.status.success());
    }
    fs::write(&file, vec![b' '; 65_537])?;
    assert!(
        !run(
            root.path(),
            state.path(),
            &["collections", "retention", "preview", "--file", file_name]
        )?
        .status
        .success()
    );
    let status = call(
        root.path(),
        state.path(),
        &["collections", "retention", "show"],
    )?;
    assert_eq!(status["policy"], receipt);
    let foreign_root = tempfile::tempdir()?;
    for args in [
        vec!["collections", "retention", "show"],
        vec!["collections", "retention", "preview"],
        vec![
            "collections",
            "retention",
            "set",
            "--json",
            &policy("automatic"),
        ],
        vec!["collections", "retention", "run", "--apply"],
    ] {
        assert!(
            !run(foreign_root.path(), state.path(), &args)?
                .status
                .success(),
            "foreign root must not operate on bound collection"
        );
    }
    Ok(())
}
