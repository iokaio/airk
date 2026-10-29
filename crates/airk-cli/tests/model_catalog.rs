// SPDX-License-Identifier: Apache-2.0
//! Capability selection works offline before any workspace/model state exists.
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{fs, process::Command};

#[test]
fn model_catalog_never_opens_collection_state_and_only_selects_int8() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let missing_root = directory.path().join("not-a-workspace");
    let missing_state = directory.path().join("not-created");
    let output = Command::new(env!("CARGO_BIN_EXE_airk"))
        .args([
            "--root",
            missing_root.to_str().unwrap(),
            "--data-dir",
            missing_state.to_str().unwrap(),
            "model-catalog",
        ])
        .env("AIRK_USAGE_MODE", "diagnostic")
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(
        output.stdout.len() < 32768,
        "catalog exceeds protocol bound"
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value["schema"], 1);
    assert_eq!(value["automatic_int8"], true);
    assert_eq!(
        value["capabilities"]["architecture"],
        std::env::consts::ARCH
    );
    assert_eq!(
        value["capabilities"]["operating_system"],
        std::env::consts::OS
    );
    assert!(value["capabilities"]["logical_cpus"].as_u64().unwrap() >= 1);
    let profiles = value["catalog"]["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 2);
    assert!(
        profiles
            .iter()
            .any(|p| p["id"] == value["selection"]["profile_id"])
    );
    assert_eq!(profiles[0]["precision"], "uint8");
    assert_eq!(profiles[1]["precision"], "int8");
    assert_eq!(
        profiles[0]["manifest"]["model_sha256"],
        "b941bf19f1f1283680f449fa6a7336bb5600bdcd5f84d10ddc5cd72218a0fd21"
    );
    assert_eq!(fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}
