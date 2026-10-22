// SPDX-License-Identifier: Apache-2.0
//! Offline model preparation is an explicit write command, never a read side effect.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{fs, io::Write};

include!(concat!(env!("OUT_DIR"), "/bundled_model.rs"));

pub fn manifest() -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(
        ASSETS
            .iter()
            .find(|(name, _)| *name == "model.json")
            .unwrap()
            .1,
    )?)
}

pub fn install(kernel: &airk_kernel::Kernel) -> Result<airk_kernel::vectors::VectorStatus> {
    ensure!(
        kernel.vectors_enabled()?,
        "vector search is disabled for this collection"
    );
    let report = airk_kernel::inspect_model_catalog()?;
    let report = serde_json::to_value(report)?;
    let profile = &report["catalog"]["profiles"][0];
    ensure!(
        manifest()? == profile["manifest"],
        "bundled model manifest differs from helper catalog"
    );
    let source = tempfile::Builder::new()
        .prefix("airk-bundled-model-")
        .tempdir_in(std::env::temp_dir().canonicalize()?)?;
    for &(name, bytes) in ASSETS {
        if name != "model.json" {
            let asset = profile["assets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|asset| asset["name"] == name)
                .unwrap();
            ensure!(
                bytes.len() as u64 == asset["bytes"].as_u64().unwrap()
                    && format!("{:x}", Sha256::digest(bytes)) == asset["sha256"].as_str().unwrap(),
                "bundled asset size or digest mismatch: {name}"
            );
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(source.path().join(name))?;
        output.write_all(bytes)?;
        output.sync_all()?;
    }
    kernel.install_default_vector_model(source.path())
}
