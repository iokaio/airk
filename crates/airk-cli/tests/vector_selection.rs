// SPDX-License-Identifier: Apache-2.0
//! Explicit vector selection stays separate from extraction and the enable switch.
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
fn selection_cli_preserves_opt_out_and_applies_only_on_indexing() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let source = root.path().join("original.rs");
    let bytes = b"pub fn original_selection_fixture() -> u32 { 7 }\n";
    fs::write(&source, bytes)?;
    call(
        root.path(),
        state.path(),
        &["collections", "vectors", "set", "--enabled", "false"],
    )?;
    call(root.path(), state.path(), &["index"])?;
    let initial = call(
        root.path(),
        state.path(),
        &["collections", "vectors", "selection", "show"],
    )?;
    assert_eq!(initial["origin"], "default");
    assert_eq!(initial["pending_reindex"], false);
    let policy = json!({"version":1,"exclusions":[{"id":"selected-rust","basename":"original.rs","language":"rust"}]}).to_string();
    let preview = call(
        root.path(),
        state.path(),
        &[
            "collections",
            "vectors",
            "selection",
            "validate",
            "--json",
            &policy,
        ],
    )?;
    assert_eq!(preview["origin"], "preview");
    assert_eq!(
        call(
            root.path(),
            state.path(),
            &["collections", "vectors", "selection", "show"]
        )?["digest"],
        initial["digest"]
    );
    let configured = call(
        root.path(),
        state.path(),
        &[
            "collections",
            "vectors",
            "selection",
            "set",
            "--json",
            &policy,
        ],
    )?;
    assert_eq!(configured["pending_reindex"], true);
    let vectors = call(
        root.path(),
        state.path(),
        &["collections", "vectors", "show"],
    )?;
    assert_eq!(vectors["enabled"], false);
    assert_eq!(vectors["selection_pending_reindex"], true);
    assert_eq!(
        call(root.path(), state.path(), &["info", "original.rs"])?["vector_selection"]["eligible"],
        true
    );
    call(root.path(), state.path(), &["index"])?;
    let applied = call(
        root.path(),
        state.path(),
        &["collections", "vectors", "selection", "show"],
    )?;
    assert_eq!(applied["pending_reindex"], false);
    assert_eq!(applied["applied_digest"], configured["digest"]);
    let info = call(root.path(), state.path(), &["info", "original.rs"])?;
    assert_eq!(info["vector_selection"]["excluded_by"], "selected-rust");
    call(
        root.path(),
        state.path(),
        &["collections", "vectors", "set", "--enabled", "true"],
    )?;
    assert_eq!(
        call(
            root.path(),
            state.path(),
            &["collections", "vectors", "selection", "show"]
        )?["digest"],
        configured["digest"]
    );
    assert_eq!(fs::read(source)?, bytes);
    Ok(())
}

#[test]
fn selection_cli_rejects_invalid_bounded_inputs_without_changing_configuration() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let input = tempfile::tempdir()?;
    let policy_file = input.path().join("selection.json");
    let valid = r#"{"version":1,"exclusions":[{"id":"text","suffix":".txt"}]}"#;
    fs::write(&policy_file, valid)?;
    let file = policy_file.to_str().unwrap();
    let saved = call(
        root.path(),
        state.path(),
        &["collections", "vectors", "selection", "set", "--file", file],
    )?;
    for action in ["validate", "set"] {
        assert!(
            !run(
                root.path(),
                state.path(),
                &["collections", "vectors", "selection", action]
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
                    "vectors",
                    "selection",
                    action,
                    "--file",
                    file,
                    "--json",
                    valid
                ]
            )?
            .status
            .success()
        );
        for invalid in [
            r#"{"version":2,"exclusions":[]}"#,
            r#"{"version":1,"exclusions":[],"unknown":true}"#,
            r#"{"version":1,"exclusions":[{"id":"all"}]}"#,
            r#"{"version":1,"exclusions":[{"id":"same","suffix":".txt"},{"id":"same","suffix":".rs"}]}"#,
            r#"{"version":1,"exclusions":[{"id":"outside","path_prefix":"../"}]}"#,
        ] {
            assert!(
                !run(
                    root.path(),
                    state.path(),
                    &[
                        "collections",
                        "vectors",
                        "selection",
                        action,
                        "--json",
                        invalid
                    ]
                )?
                .status
                .success(),
                "accepted {invalid}"
            );
        }
    }
    fs::write(&policy_file, vec![b' '; 131_073])?;
    assert!(
        !run(
            root.path(),
            state.path(),
            &["collections", "vectors", "selection", "set", "--file", file]
        )?
        .status
        .success()
    );
    assert_eq!(
        call(
            root.path(),
            state.path(),
            &["collections", "vectors", "selection", "show"]
        )?["digest"],
        saved["digest"]
    );
    let foreign = tempfile::tempdir()?;
    for args in [
        vec!["show"],
        vec!["validate", "--json", valid],
        vec!["set", "--json", valid],
    ] {
        let mut command = vec!["collections", "vectors", "selection"];
        command.extend(args);
        assert!(
            !run(foreign.path(), state.path(), &command)?
                .status
                .success()
        );
    }
    let reset = call(
        root.path(),
        state.path(),
        &[
            "collections",
            "vectors",
            "selection",
            "set",
            "--json",
            r#"{"version":1,"exclusions":[]}"#,
        ],
    )?;
    assert_eq!(reset["policy"]["exclusions"], json!([]));
    Ok(())
}
