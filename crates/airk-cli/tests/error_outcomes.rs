// SPDX-License-Identifier: Apache-2.0
//! Real CLI gate contention is structured independently of Rust backtrace settings.
use airk_kernel::Kernel;
use airk_store::{FileStore, current::CurrentScope, quota::Quota};
use anyhow::Result;
use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn cli_gate_contention_is_typed_with_backtraces_and_other_errors_are_not_retryable() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("fixture.txt"), "gate contention fixture\n")?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let owner: Value =
        serde_json::from_slice(&fs::read(store.directory().join("current-owner.json"))?)?;
    let scope: CurrentScope = serde_json::from_value(owner["scope"].clone())?;
    let quota = Quota::open(data.path())?;
    let writer = store.lock()?;
    let capacity = store.current_database_capacity(&scope)?;
    let current = store.current_write(&writer, &quota, &scope, capacity)?;
    for (arguments, backtrace) in [
        (vec!["collections", "diagnose"], "0"),
        (vec!["collections", "diagnose"], "1"),
        (vec!["collections", "diagnose"], "full"),
        (vec!["status"], "1"),
        (vec!["semantic", "status"], "full"),
        (vec!["semantic", "run", "--scheduled"], "1"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_airk"))
            .arg("--root")
            .arg(root.path())
            .arg("--data-dir")
            .arg(data.path())
            .args(&arguments)
            .env("AIRK_ERROR_FORMAT", "json")
            .env("RUST_BACKTRACE", backtrace)
            .env("RUST_LIB_BACKTRACE", backtrace)
            .output()?;
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let outcome: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(outcome["airk_error"], 1);
        assert_eq!(
            outcome["code"], "current_gate_busy",
            "{arguments:?}: {outcome}"
        );
        assert_eq!(outcome["retryable"], true);
        assert!(
            outcome["message"]
                .as_str()
                .unwrap()
                .contains("current index is busy applying an update")
        );
        assert!(
            !outcome["message"]
                .as_str()
                .unwrap()
                .contains("stack backtrace")
        );
    }
    drop(current);
    drop(writer);
    // Invalid durable ownership is an operational error, never a busy result.
    fs::write(
        store.directory().join("current-owner.json"),
        "invalid owner",
    )?;
    let output = Command::new(env!("CARGO_BIN_EXE_airk"))
        .arg("--root")
        .arg(root.path())
        .arg("--data-dir")
        .arg(data.path())
        .args(["collections", "diagnose"])
        .env("AIRK_ERROR_FORMAT", "json")
        .env("RUST_LIB_BACKTRACE", "1")
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    let outcome: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(outcome["code"], "operation_failed");
    assert_eq!(outcome["retryable"], false);
    let output = Command::new(env!("CARGO_BIN_EXE_airk"))
        .arg("--root")
        .arg(root.path())
        .arg("--data-dir")
        .arg(data.path())
        .args(["collections", "diagnose"])
        .env_remove("AIRK_ERROR_FORMAT")
        .env("RUST_BACKTRACE", "0")
        .env("RUST_LIB_BACKTRACE", "0")
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("Error: "));
    Ok(())
}
