// SPDX-License-Identifier: Apache-2.0
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn call(root: &Path, data: &Path, args: &[&str]) -> Result<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_airk"))
        .arg("--root")
        .arg(root)
        .arg("--data-dir")
        .arg(data)
        .env("AIRK_USAGE_MODE", "disabled")
        .args(args)
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn opt_in_similarity_flag_pages_with_shared_budgets_without_forwarding_reads() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original: String = (0..40)
        .map(|i| format!("billing{i} records original invoice entry {i}.\n"))
        .collect();
    fs::write(root.path().join("old.txt"), &original)?;
    call(root.path(), data.path(), &["index"])?;
    let info = call(root.path(), data.path(), &["info", "old.txt"])?;
    let token = info["read"]["publication"]["token"]
        .as_str()
        .context("token")?;
    fs::remove_file(root.path().join("old.txt"))?;
    fs::write(
        root.path().join("new.txt"),
        original.replace("entry 17.", "entry revised-seventeen."),
    )?;
    call(root.path(), data.path(), &["index"])?;
    let default = call(root.path(), data.path(), &["changes", "--since", token])?;
    assert!(default["suggestions"].as_array().unwrap().is_empty());
    assert!(default["suggestion_coverage"].is_null());
    let mut cursor: Option<String> = None;
    let mut suggestions = vec![];
    loop {
        let mut args = vec![
            "changes",
            "--since",
            token,
            "--include-suggestions",
            "--limit",
            "1",
            "--max-bytes",
            "8192",
        ];
        if let Some(cursor) = &cursor {
            args.extend(["--cursor", cursor]);
        }
        let response = call(root.path(), data.path(), &args)?;
        let mut count = 0;
        let mut bytes = 0;
        for key in ["changes", "invalidations", "validations", "suggestions"] {
            for record in response[key].as_array().unwrap() {
                count += 1;
                bytes += serde_json::to_vec(record)?.len();
            }
        }
        assert!(count <= 1 && bytes <= 8192);
        assert_eq!(response["record_bytes"].as_u64(), Some(bytes as u64));
        suggestions.extend(response["suggestions"].as_array().unwrap().iter().cloned());
        cursor = response["cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert!(suggestions.is_empty());
    let unavailable = call(
        root.path(),
        data.path(),
        &["changes", "--since", token, "--include-suggestions"],
    )?;
    assert_eq!(unavailable["suggestion_coverage"]["status"], "unavailable");
    assert_eq!(unavailable["baseline_status"], "baseline_unavailable");
    let read = call(
        root.path(),
        data.path(),
        &[
            "read",
            "old.txt",
            "--follow-moves",
            "--start-line",
            "1",
            "--end-line",
            "1",
        ],
    )?;
    assert_ne!(read["read"]["status"], "content");
    assert!(read.get("hits").is_none());
    Ok(())
}
