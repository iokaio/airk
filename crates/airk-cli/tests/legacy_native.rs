// SPDX-License-Identifier: Apache-2.0
//! Unadopted old commands remain data during actual native startup inspection.
mod common;
use anyhow::{Result, ensure};
use std::fs;
#[test]
fn legacy_review_never_executes_the_selected_old_binary_or_creates_project_state() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let old = tempfile::tempdir()?;
    fs::write(
        source.path().join("original.txt"),
        b"unchanged review source",
    )?;
    let service = common::Service::start(source.path(), data.path(), "legacy-readonly");
    let old_data = old.path().join("old-data");
    fs::create_dir(&old_data)?;
    fs::write(
        old_data.join("saved-settings.json"),
        b"preserved old settings",
    )?;
    fs::write(old_data.join("index.dat"), b"preserved old index")?;
    let executable = old
        .path()
        .join(if cfg!(windows) { "airk.exe" } else { "airk" });
    fs::write(
        &executable,
        b"This is deliberately not executable; never launch it.",
    )?;
    let output = common::native_command()
        .args(["service", "legacy", "review", "--runtime-dir"])
        .arg(&service.control)
        .arg("--root")
        .arg(source.path())
        .arg("--data-dir")
        .arg(&old_data)
        .args(["--collection", "old-review", "--executable"])
        .arg(&executable)
        .output()?;
    ensure!(
        output.status.success(),
        "legacy native review failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let review: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(review["approval"], serde_json::Value::Null);
    assert_eq!(review["preview"]["removal_available"], false);
    assert!(!service.control.join("legacy-retirement-1.4").try_exists()?);
    assert_eq!(
        fs::read(source.path().join("original.txt"))?,
        b"unchanged review source"
    );
    assert_eq!(
        fs::read(old_data.join("saved-settings.json"))?,
        b"preserved old settings"
    );
    assert_eq!(
        fs::read(old_data.join("index.dat"))?,
        b"preserved old index"
    );
    assert_eq!(fs::read_dir(old_data)?.count(), 2);
    let list = common::native_command()
        .args(["service", "legacy", "list", "--runtime-dir"])
        .arg(&service.control)
        .output()?;
    ensure!(list.status.success(), "native legacy receipt list failed");
    assert!(
        serde_json::from_slice::<serde_json::Value>(&list.stdout)?["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!service.control.join("legacy-retirement-1.4").try_exists()?);
    Ok(())
}
