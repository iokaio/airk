// SPDX-License-Identifier: Apache-2.0
//! Unadopted old commands remain data during actual native startup inspection.
mod common;
use anyhow::{Result, ensure};
use std::fs;
#[test]
fn explicit_old_derived_cleanup_uses_actual_cli_and_keeps_originals_and_settings() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let old = tempfile::tempdir()?;
    let service = common::Service::start(source.path(), data.path(), "legacy-cleanup-cli");
    let source = source.path().canonicalize()?;
    fs::write(source.join("original.txt"), b"preserved original source")?;
    let old_data = old.path().canonicalize()?;
    let directory = old_data.join("collections-1.2/old-collection");
    fs::create_dir_all(&directory)?;
    let owner = serde_json::json!({"version":126,"scope":{"root":source.to_str().unwrap(),"collection":"old-collection","incarnation":airk_core::identity::Incarnation::from_bytes([46;16])},"capacity":65536});
    // The ownership header is canonical typed metadata; the payload is deliberately not SQLite.
    #[derive(serde::Serialize)]
    struct Header {
        version: u32,
        scope: airk_store::current::CurrentScope,
        capacity: u64,
    }
    let header = Header {
        version: 126,
        scope: serde_json::from_value(owner["scope"].clone())?,
        capacity: 65536,
    };
    fs::write(
        directory.join("current-owner.json"),
        serde_json::to_vec(&header)?,
    )?;
    fs::write(
        directory.join("current.sqlite3"),
        b"unreadable old database bytes",
    )?;
    fs::write(
        directory.join("scan-limits.json"),
        b"original saved settings",
    )?;
    fs::write(
        directory.join("user-file.txt"),
        b"unrelated preserved bytes",
    )?;
    let invoke = |action: &str, approval: Option<&str>| -> Result<serde_json::Value> {
        let mut command = common::native_command();
        command
            .args(["service", "legacy", action, "--runtime-dir"])
            .arg(&service.control);
        if action != "derived-receipts" {
            command
                .arg("--root")
                .arg(&source)
                .arg("--data-dir")
                .arg(&old_data)
                .args(["--collection", "old-collection"]);
        }
        if let Some(approval) = approval {
            command.args(["--approval", approval]);
        }
        let output = command.output()?;
        ensure!(
            output.status.success(),
            "legacy cleanup CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let preview = invoke("preview-derived", None)?;
    let approval = preview["approval"].as_str().unwrap();
    assert_eq!(preview["files"], 2);
    let result = invoke("remove-derived", Some(approval))?;
    assert_eq!(result["complete"], true);
    assert!(!directory.join("current.sqlite3").exists());
    assert_eq!(
        fs::read(directory.join("scan-limits.json"))?,
        b"original saved settings"
    );
    assert_eq!(
        fs::read(directory.join("user-file.txt"))?,
        b"unrelated preserved bytes"
    );
    assert_eq!(
        fs::read(source.join("original.txt"))?,
        b"preserved original source"
    );
    assert_eq!(
        invoke("derived-receipts", None)?["items"][0]["complete"],
        true
    );
    fs::write(
        directory.join("current.sqlite3"),
        b"later index: completion must not delete",
    )?;
    assert_eq!(invoke("remove-derived", Some(approval))?, result);
    assert_eq!(
        fs::read(directory.join("current.sqlite3"))?,
        b"later index: completion must not delete"
    );
    Ok(())
}
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
    if std::env::var_os("AIRK_PROCESS_DIAGNOSTICS").is_some() {
        eprintln!("airk legacy fixture: review CLI invocation started");
    }
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
    if std::env::var_os("AIRK_PROCESS_DIAGNOSTICS").is_some() {
        eprintln!("airk legacy fixture: review CLI invocation returned");
    }
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
