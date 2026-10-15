// SPDX-License-Identifier: Apache-2.0
//! Original command fixtures for exact retained reads and explicit history controls.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
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
fn manual(root: &Path, data: &Path) -> Result<()> {
    call(
        root,
        data,
        &[
            "collections",
            "retention",
            "set",
            "--json",
            r#"{"version":1,"mode":"manual","searchable_predecessors":1}"#,
        ],
    )?;
    Ok(())
}
fn token(root: &Path, data: &Path) -> Result<String> {
    Ok(
        call(root, data, &["info", "original.py"])?["read"]["publication"]["token"]
            .as_str()
            .context("exact token")?
            .into(),
    )
}

#[test]
fn exact_revision_reads_and_availability_reject_ambiguous_or_stale_selectors() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("original.py"),
        "def original_beacon():\n    return 17\n",
    )?;
    call(root.path(), data.path(), &["index"])?;
    manual(root.path(), data.path())?;
    let old = token(root.path(), data.path())?;
    fs::write(
        root.path().join("original.py"),
        "def original_beacon():\n    return 29\n",
    )?;
    call(root.path(), data.path(), &["index"])?;
    let current = token(root.path(), data.path())?;
    fs::write(root.path().join("original.py"), "LIVE_UNINDEXED_SENTINEL\n")?;
    let read = call(
        root.path(),
        data.path(),
        &[
            "read",
            "original.py",
            "--start-line",
            "1",
            "--end-line",
            "2",
            "--revision",
            &old,
        ],
    )?;
    assert_eq!(read["read"]["publication"]["token"], old);
    assert_eq!(read["read"]["current_publication"]["token"], current);
    assert_eq!(read["read"]["historical"], true);
    assert!(read["hits"][0]["text"].as_str().unwrap().contains("17"));
    let validator = read["read"]["validator"].as_str().context("validator")?;
    let unchanged = call(
        root.path(),
        data.path(),
        &[
            "read",
            "original.py",
            "--start-line",
            "1",
            "--end-line",
            "2",
            "--revision",
            &old,
            "--if-evidence",
            validator,
        ],
    )?;
    assert_eq!(unchanged["read"]["status"], "not_modified");
    assert!(unchanged.get("hits").is_none());
    for args in [
        vec!["info", "original.py"],
        vec!["outline", "original.py"],
        vec![
            "context",
            "original.py",
            "2",
            "--before",
            "0",
            "--after",
            "0",
        ],
    ] {
        let mut args = args;
        args.extend(["--revision", old.as_str()]);
        let result = call(root.path(), data.path(), &args)?;
        assert_eq!(result["read"]["publication"]["token"], old);
    }
    assert!(
        !run(
            root.path(),
            data.path(),
            &["info", "original.py", "--revision", "1"]
        )?
        .status
        .success()
    );
    assert!(
        !run(
            root.path(),
            data.path(),
            &["info", "original.py", "--revision", &old, "--follow-moves"]
        )?
        .status
        .success()
    );
    let page = call(
        root.path(),
        data.path(),
        &["collections", "history", "available", "--limit", "1"],
    )?;
    let cursor = page["next_cursor"]
        .as_str()
        .context("availability cursor")?;
    assert!(
        !run(
            root.path(),
            data.path(),
            &[
                "collections",
                "history",
                "available",
                "--limit",
                "2",
                "--cursor",
                cursor
            ]
        )?
        .status
        .success()
    );
    assert!(
        !run(
            root.path(),
            data.path(),
            &["collections", "history", "available", "--limit", "65"]
        )?
        .status
        .success()
    );
    call(
        root.path(),
        data.path(),
        &["collections", "prune", "--apply"],
    )?;
    let missing = call(
        root.path(),
        data.path(),
        &["info", "original.py", "--revision", &old],
    )?;
    assert_eq!(missing["read"]["status"], "history_unavailable");
    assert!(missing.get("content_hash").is_none());
    assert_eq!(
        fs::read_to_string(root.path().join("original.py"))?,
        "LIVE_UNINDEXED_SENTINEL\n"
    );
    Ok(())
}

#[test]
fn checkpoint_commands_and_purge_require_exact_confirmed_plan() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("original.py"),
        "def original_beacon():\n    return 17\n",
    )?;
    call(root.path(), data.path(), &["index"])?;
    manual(root.path(), data.path())?;
    let head = token(root.path(), data.path())?;
    let policy = call(
        root.path(),
        data.path(),
        &["collections", "history", "show"],
    )?;
    assert_eq!(policy["policy"]["enabled"], false);
    let enabled = json!({"version":1,"enabled":true,"recent":3,"hourly_hours":24,"max_packages":12,"max_bytes":268435456,"quota_basis_points":1000}).to_string();
    call(
        root.path(),
        data.path(),
        &["collections", "history", "set", "--json", &enabled],
    )?;
    call(
        root.path(),
        data.path(),
        &["collections", "history", "preview"],
    )?;
    call(root.path(), data.path(), &["collections", "history", "run"])?;
    assert_eq!(token(root.path(), data.path())?, head);
    call(
        root.path(),
        data.path(),
        &[
            "collections",
            "checkpoint",
            "create",
            "original café",
            "--revision",
            &head,
        ],
    )?;
    assert_eq!(
        call(
            root.path(),
            data.path(),
            &["collections", "checkpoint", "list"]
        )?
        .as_array()
        .unwrap()
        .len(),
        1
    );
    assert!(
        !run(
            root.path(),
            data.path(),
            &[
                "collections",
                "checkpoint",
                "create",
                "invalid",
                "--revision",
                "latest"
            ]
        )?
        .status
        .success()
    );
    call(
        root.path(),
        data.path(),
        &[
            "collections",
            "checkpoint",
            "remove",
            "original café",
            "--revision",
            &head,
        ],
    )?;
    assert!(
        call(
            root.path(),
            data.path(),
            &["collections", "checkpoint", "list"]
        )?
        .as_array()
        .unwrap()
        .is_empty()
    );
    let purge = json!({"revisions":[],"checkpoints":[],"clear_lineage":true}).to_string();
    let plan = call(
        root.path(),
        data.path(),
        &["collections", "history", "purge", "--json", &purge],
    )?;
    assert_eq!(token(root.path(), data.path())?, head);
    assert!(
        !run(
            root.path(),
            data.path(),
            &[
                "collections",
                "history",
                "purge",
                "--json",
                &purge,
                "--apply"
            ]
        )?
        .status
        .success()
    );
    let applied = call(
        root.path(),
        data.path(),
        &[
            "collections",
            "history",
            "purge",
            "--json",
            &purge,
            "--apply",
            "--expected-plan",
            plan["digest"].as_str().context("plan digest")?,
        ],
    )?;
    assert!(applied["lineage_publication"].is_object());
    assert_ne!(token(root.path(), data.path())?, head);
    Ok(())
}
