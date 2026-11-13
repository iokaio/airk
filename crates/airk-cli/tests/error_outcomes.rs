// SPDX-License-Identifier: Apache-2.0
//! Native clients refuse unavailable services without opening legacy storage.
use airk_kernel::Kernel;
use airk_store::{FileStore, current::CurrentScope, quota::Quota};
use anyhow::Result;
use serde_json::Value;
use std::fs;
mod common;

#[test]
fn cli_service_refusal_preserves_locked_legacy_storage() -> Result<()> {
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
    let before = fs::read(store.directory().join("current-owner.json"))?;
    // Backtrace environment/exit coverage lives in airkctl/tests/error_outcomes.rs,
    // where the actual executable receives the environment in a child process.
    for arguments in [
        vec!["status"],
        vec!["index"],
        vec!["outline", "fixture.txt"],
    ] {
        let output = common::ctl::Ctl::new()
            .arg("--service-runtime-dir")
            .arg(data.path().join("missing-runtime"))
            .arg("--root")
            .arg(root.path())
            .arg("--data-dir")
            .arg(data.path())
            .args(&arguments)
            .env("AIRK_ERROR_FORMAT", "json")
            .output()?;
        // airkctl's I2 error line (contracts §3.4): a missing service is class
        // `service_missing`, exit 3, retryable; it starts and opens nothing.
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stdout.is_empty());
        let outcome: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(outcome["airkctl_error"], 1);
        assert_eq!(
            outcome["class"], "service_missing",
            "{arguments:?}: {outcome}"
        );
        assert_eq!(
            outcome["code"], "service_unavailable",
            "{arguments:?}: {outcome}"
        );
        assert_eq!(outcome["retryable"], true);
        assert!(outcome["message"].as_str().unwrap().contains("not running"));
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
    let output = common::ctl::Ctl::new()
        .arg("--service-runtime-dir")
        .arg(data.path().join("missing-runtime"))
        .arg("--root")
        .arg(root.path())
        .arg("--data-dir")
        .arg(data.path())
        .args(["status"])
        .env("AIRK_ERROR_FORMAT", "json")
        .output()?;
    // airkctl never opens project storage, so an invalid owner file cannot
    // become a busy or storage result: the answer is the missing service.
    assert_eq!(output.status.code(), Some(3));
    let outcome: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(outcome["class"], "service_missing");
    assert_eq!(outcome["code"], "service_unavailable");
    let output = common::ctl::Ctl::new()
        .arg("--service-runtime-dir")
        .arg(data.path().join("missing-runtime"))
        .arg("--root")
        .arg(root.path())
        .arg("--data-dir")
        .arg(data.path())
        .args(["status"])
        .env_remove("AIRK_ERROR_FORMAT")
        .output()?;
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("airkctl: "));
    Ok(())
}
