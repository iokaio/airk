// SPDX-License-Identifier: Apache-2.0
//! Capability selection works offline before any workspace/model state exists.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::fs;
mod common;

#[test]
fn model_catalog_never_opens_collection_state_and_selects_shared_fp16() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let missing_root = directory.path().join("not-a-workspace");
    let missing_state = directory.path().join("not-created");
    // airkctl asks the service for the catalog (M10); the service has no project.
    let service = common::BareService::start();
    let output = common::ctl::Ctl::new()
        .arg("--service-runtime-dir")
        .arg(&service.control)
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
    assert_eq!(value["automatic_int8"], false);
    assert_eq!(
        value["capabilities"]["architecture"],
        std::env::consts::ARCH
    );
    assert_eq!(
        value["capabilities"]["operating_system"],
        std::env::consts::OS
    );
    assert!(value["capabilities"]["logical_cpus"].as_u64().unwrap() >= 1);
    let id = "minilm-l6-v2-fp16-v1";
    let runtime = "onnxruntime/1.30.0/airk-minilm-fp16-shared/1";
    let [profile] = value["catalog"]["profiles"].as_array().unwrap().as_slice() else {
        anyhow::bail!("expected exactly one bundled profile");
    };
    assert_eq!(value["catalog"]["schema"], 2);
    assert_eq!(value["catalog"]["default_profile"], id);
    assert_eq!(value["selection"]["profile_id"], id);
    assert_eq!(value["selection"]["reason"], "shared_fp16_model");
    assert_eq!(value["selection"]["platform_qualified"], false);
    assert_eq!(value["selection"]["short_input_batch_size"], 1);
    // The capability field retains its original runtime-family label.
    assert_eq!(
        value["capabilities"]["runtime"],
        "onnxruntime-cpu/1.30.0/airk-trained-mixed-int8-b1-v2"
    );
    let model = "f3c3a4eed373650ba1e2f4d3d53437f1ecf92d4c50e5994e07e163751923ee4a";
    let tokenizer = "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037";
    let license = "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30";
    assert_eq!(profile["id"], id);
    assert_eq!(profile["precision"], "float16");
    assert_eq!(profile["runtime"], runtime);
    assert_eq!(profile["manifest"]["model_sha256"], model);
    assert_eq!(profile["manifest"]["tokenizer_sha256"], tokenizer);
    assert_eq!(profile["manifest"]["license_sha256"], license);
    assert_eq!(value["bundled_model"]["manifest"], profile["manifest"]);
    assert!(profile.get("quantization").is_none_or(Value::is_null));
    assert_eq!(
        profile["assets"],
        json!([
            {"name":"model.onnx","bytes":45469884,"sha256":model,"url":null},
            {"name":"tokenizer.json","bytes":466247,"sha256":tokenizer,"url":null},
            {"name":"LICENSE","bytes":11358,"sha256":license,"url":null}
        ])
    );
    assert_eq!(fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}
