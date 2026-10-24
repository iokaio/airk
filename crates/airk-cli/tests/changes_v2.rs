// SPDX-License-Identifier: Apache-2.0
//! Original CLI fixtures for exact remembered evidence and conditional reads.
use anyhow::{Context, Result};
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
    let output = run(root, state, args)?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn four_conditional_reads_and_remembered_paths_preserve_exact_identity() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(
        root.path().join("weather.py"),
        "def aurora_temperature():\n    return 17\n",
    )?;
    call(root.path(), state.path(), &["index"])?;
    let info = call(root.path(), state.path(), &["info", "weather.py"])?;
    let source_hash = info["content_hash"].as_str().context("source hash")?;
    let source_only = call(
        root.path(),
        state.path(),
        &["info", "weather.py", "--known-hash", source_hash],
    )?;
    assert_eq!(source_only["read"]["status"], "content");
    let first_line = call(
        root.path(),
        state.path(),
        &["read", "weather.py", "--start-line", "1", "--end-line", "1"],
    )?;
    let validator = first_line["read"]["validator"]
        .as_str()
        .context("line validator")?;
    let changed_span = call(
        root.path(),
        state.path(),
        &[
            "read",
            "weather.py",
            "--start-line",
            "2",
            "--end-line",
            "2",
            "--if-evidence",
            validator,
        ],
    )?;
    assert_eq!(changed_span["read"]["status"], "content");
    assert_ne!(changed_span["read"]["validator"], validator);
    for args in [
        vec!["info", "weather.py"],
        vec!["read", "weather.py", "--start-line", "1", "--end-line", "2"],
        vec!["outline", "weather.py"],
        vec!["context", "weather.py", "1"],
    ] {
        let first = call(root.path(), state.path(), &args)?;
        assert_eq!(first["read"]["status"], "content");
        let validator = first["read"]["validator"].as_str().context("validator")?;
        let mut conditional = args.clone();
        conditional.extend(["--if-evidence", validator]);
        let second = call(root.path(), state.path(), &conditional)?;
        assert_eq!(second["read"]["status"], "not_modified");
        assert_eq!(second["read"]["publication"], first["read"]["publication"]);
        assert!(second.get("hits").is_none() && second.get("facts").is_none());
    }
    let remembered = call(
        root.path(),
        state.path(),
        &["changes", "--path", "weather.py"],
    )?;
    assert_eq!(remembered["status"], "ready");
    assert_eq!(remembered["baseline_status"], "not_requested");
    assert_eq!(remembered["validations"][0]["requested_path"], "weather.py");
    assert_eq!(remembered["freshness"]["live_source_verified"], false);
    let token = remembered["to"].as_str().context("target token")?;
    assert_eq!(remembered["publication"]["token"], token);
    let unchanged = call(
        root.path(),
        state.path(),
        &["changes", "--since", token, "--path", "weather.py"],
    )?;
    assert_eq!(unchanged["baseline_status"], "verified");
    assert_eq!(unchanged["validations"][0]["outcome"], "index-current");
    assert!(
        unchanged["changes"]
            .as_array()
            .context("changes")?
            .is_empty()
    );
    // A live edit is not evidence until a successful publication observes it.
    fs::write(
        root.path().join("weather.py"),
        "def aurora_temperature():\n    return 29\n",
    )?;
    let still_indexed = call(root.path(), state.path(), &["changes", "--since", token])?;
    assert_eq!(still_indexed["to"], token);
    call(root.path(), state.path(), &["index"])?;
    let changed = call(
        root.path(),
        state.path(),
        &["changes", "--since", token, "--path", "weather.py"],
    )?;
    assert_ne!(changed["to"], token);
    assert_eq!(changed["baseline_status"], "verified");
    assert!(!changed["changes"].as_array().unwrap().is_empty());
    assert_eq!(changed["validations"][0]["outcome"], "index-current");
    Ok(())
}

#[test]
fn changes_rejects_invalid_scope_and_bounds_and_preserves_selector_ordinals() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::write(
        root.path().join("scope.txt"),
        "Original northern observatory evidence.\n",
    )?;
    call(root.path(), state.path(), &["index"])?;
    for args in [
        vec!["changes"],
        vec!["changes", "--path", "../outside"],
        vec!["changes", "--path", "/outside"],
        vec!["changes", "--since", "malformed"],
        vec!["changes", "--path", "scope.txt", "--limit", "0"],
        vec!["changes", "--path", "scope.txt", "--max-bytes", "1"],
    ] {
        assert!(
            !run(root.path(), state.path(), &args)?.status.success(),
            "{args:?}"
        );
    }
    let selector = json!({"kind":"path","path":"scope.txt"}).to_string();
    let response = call(
        root.path(),
        state.path(),
        &["changes", "--selector", &selector, "--selector", &selector],
    )?;
    assert_eq!(response["validations"][0]["ordinal"], 0);
    assert_eq!(response["validations"][1]["ordinal"], 1);
    let mut args = vec!["changes"];
    for _ in 0..65 {
        args.extend(["--selector", selector.as_str()]);
    }
    assert!(!run(root.path(), state.path(), &args)?.status.success());
    Ok(())
}

#[test]
fn equal_generation_foreign_and_forked_tokens_are_rejected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let first_state = tempfile::tempdir()?;
    let second_state = tempfile::tempdir()?;
    fs::write(
        root.path().join("identity.txt"),
        "Independent original polar evidence.\n",
    )?;
    call(root.path(), first_state.path(), &["index"])?;
    call(root.path(), second_state.path(), &["index"])?;
    let first = call(
        root.path(),
        first_state.path(),
        &["changes", "--path", "identity.txt"],
    )?;
    let second = call(
        root.path(),
        second_state.path(),
        &["changes", "--path", "identity.txt"],
    )?;
    assert_eq!(
        first["publication"]["generation"],
        second["publication"]["generation"]
    );
    assert_ne!(
        first["publication"]["incarnation"],
        second["publication"]["incarnation"]
    );
    let token = first["to"].as_str().context("token")?;
    for (state, candidate) in [
        (second_state.path(), token.to_owned()),
        (first_state.path(), {
            let mut t: airk_core::identity::RevisionToken = token.parse()?;
            t.revision = airk_core::identity::Sha256Digest::from_bytes([0x5a; 32]);
            t.to_string()
        }),
        (first_state.path(), {
            let mut t: airk_core::identity::RevisionToken = token.parse()?;
            t.generation += 1;
            t.to_string()
        }),
    ] {
        assert_eq!(
            call(root.path(), state, &["changes", "--since", &candidate])?["baseline_status"],
            "baseline_unavailable"
        );
        assert!(
            !run(
                root.path(),
                state,
                &["info", "identity.txt", "--revision", &candidate]
            )?
            .status
            .success()
        );
    }
    Ok(())
}
