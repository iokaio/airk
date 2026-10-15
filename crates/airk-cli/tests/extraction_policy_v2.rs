// SPDX-License-Identifier: Apache-2.0
//! Original command fixtures: configured policy is distinct from published evidence.
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
    let output = run(root, state, args)?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn configured_policy_waits_for_publication_and_validation_does_not_mutate() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let source = root.path().join("original.rs");
    let bytes = b"pub fn original_fixture() -> u32 { 7 }\n";
    fs::write(&source, bytes)?;
    call(root.path(), state.path(), &["index"])?;
    let original = call(
        root.path(),
        state.path(),
        &["collections", "extraction", "show"],
    )?;
    assert_eq!(original["pending_reindex"], false);
    let policy = json!({"version":1,"rules":[{"selector":{"kind":"exact","path":"original.rs"},"case_sensitive":true,"adapter_id":"text"}]}).to_string();
    let preview = call(
        root.path(),
        state.path(),
        &["collections", "extraction", "validate", "--json", &policy],
    )?;
    assert_eq!(preview["origin"], "preview");
    assert_eq!(
        call(
            root.path(),
            state.path(),
            &["collections", "extraction", "show"]
        )?["digest"],
        original["digest"]
    );
    let configured = call(
        root.path(),
        state.path(),
        &["collections", "extraction", "set", "--json", &policy],
    )?;
    assert_eq!(configured["pending_reindex"], true);
    assert_eq!(configured["applied_digest"], original["applied_digest"]);
    assert_eq!(configured["publication"], original["publication"]);
    call(root.path(), state.path(), &["index"])?;
    let applied = call(
        root.path(),
        state.path(),
        &["collections", "extraction", "show"],
    )?;
    assert_eq!(applied["pending_reindex"], false);
    assert_eq!(applied["applied_digest"], configured["digest"]);
    assert_eq!(fs::read(&source)?, bytes);
    Ok(())
}

#[test]
fn policy_inputs_are_bounded_validated_and_scope_checked() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let fixture = tempfile::tempdir()?;
    fs::write(root.path().join("original.rs"), "fn original() {}\n")?;
    call(root.path(), state.path(), &["index"])?;
    let file = fixture.path().join("policy.json");
    fs::write(&file, br#"{"version":1,"rules":[]}"#)?;
    let file = file.to_str().unwrap();
    call(
        root.path(),
        state.path(),
        &["collections", "extraction", "set", "--file", file],
    )?;
    for action in ["validate", "set"] {
        assert!(
            !run(
                root.path(),
                state.path(),
                &["collections", "extraction", action]
            )?
            .status
            .success()
        );
        assert!(
            !run(
                root.path(),
                state.path(),
                &[
                    "collections",
                    "extraction",
                    action,
                    "--file",
                    file,
                    "--json",
                    "{} "
                ]
            )?
            .status
            .success()
        );
        assert!(
            !run(
                root.path(),
                state.path(),
                &[
                    "collections",
                    "extraction",
                    action,
                    "--json",
                    r#"{"version":2,"rules":[]}"#
                ]
            )?
            .status
            .success()
        );
        assert!(
            !run(
                root.path(),
                state.path(),
                &[
                    "collections",
                    "extraction",
                    action,
                    "--json",
                    r#"{"version":1,"rules":[],"unknown":true}"#
                ]
            )?
            .status
            .success()
        );
    }
    fs::write(file, vec![b' '; 131_073])?;
    assert!(
        !run(
            root.path(),
            state.path(),
            &["collections", "extraction", "set", "--file", file]
        )?
        .status
        .success()
    );
    let wrong = tempfile::tempdir()?;
    for action in [
        vec!["show"],
        vec!["validate", "--json", r#"{"version":1,"rules":[]}"#],
        vec!["set", "--json", r#"{"version":1,"rules":[]}"#],
    ] {
        let mut args = vec!["collections", "extraction"];
        args.extend(action);
        assert!(!run(wrong.path(), state.path(), &args)?.status.success());
    }
    Ok(())
}
