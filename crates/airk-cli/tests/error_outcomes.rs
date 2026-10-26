// SPDX-License-Identifier: Apache-2.0
//! Native clients refuse unavailable services without opening legacy storage.
use airk_kernel::Kernel;
use airk_store::{FileStore, current::CurrentScope, quota::Quota};
use anyhow::Result;
use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn cli_service_refusal_preserves_locked_legacy_storage_and_backtrace_independent_errors()
-> Result<()> {
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
    let current = store.redb_writer(&writer, &quota, &scope)?;
    let before = fs::read(store.directory().join("current-owner.json"))?;
    for (arguments, backtrace) in [
        (vec!["status"], "0"),
        (vec!["status"], "1"),
        (vec!["status"], "full"),
        (vec!["index"], "full"),
        (vec!["outline", "fixture.txt"], "1"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_airk"))
            .arg("--service-install-dir")
            .arg(data.path().join("missing-runtime"))
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
            outcome["code"], "operation_failed",
            "{arguments:?}: {outcome}"
        );
        assert_eq!(outcome["retryable"], false);
        assert!(outcome["message"].as_str().unwrap().contains("runtime"));
        assert!(
            !outcome["message"]
                .as_str()
                .unwrap()
                .contains("stack backtrace")
        );
    }
    assert_eq!(
        fs::read(store.directory().join("current-owner.json"))?,
        before
    );
    assert!(!data.path().join("missing-runtime").exists());
    drop(current);
    drop(writer);
    // Invalid durable ownership is an operational error, never a busy result.
    fs::write(
        store.directory().join("current-owner.json"),
        "invalid owner",
    )?;
    let output = Command::new(env!("CARGO_BIN_EXE_airk"))
        .arg("--service-install-dir")
        .arg(data.path().join("missing-runtime"))
        .arg("--root")
        .arg(root.path())
        .arg("--data-dir")
        .arg(data.path())
        .args(["status"])
        .env("AIRK_ERROR_FORMAT", "json")
        .env("RUST_LIB_BACKTRACE", "1")
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    let outcome: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(outcome["code"], "operation_failed");
    assert_eq!(outcome["retryable"], false);
    let output = Command::new(env!("CARGO_BIN_EXE_airk"))
        .arg("--service-install-dir")
        .arg(data.path().join("missing-runtime"))
        .arg("--root")
        .arg(root.path())
        .arg("--data-dir")
        .arg(data.path())
        .args(["status"])
        .env_remove("AIRK_ERROR_FORMAT")
        .env("RUST_BACKTRACE", "0")
        .env("RUST_LIB_BACKTRACE", "0")
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("Error: "));
    Ok(())
}
