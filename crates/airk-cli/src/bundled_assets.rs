// SPDX-License-Identifier: Apache-2.0
//! Explicit offline extraction of verified embedded model assets.
use anyhow::Result;
#[cfg(feature = "bundled-model")]
use anyhow::ensure;
#[cfg(feature = "bundled-model")]
use sha2::{Digest, Sha256};
#[cfg(feature = "bundled-model")]
use std::{fs, io::Write};
#[cfg(feature = "bundled-model")]
include!(concat!(env!("OUT_DIR"), "/bundled_model.rs"));

pub fn manifest() -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/model.json"
    )))?)
}

#[cfg(feature = "bundled-model")]
pub fn extract(directory: &std::path::Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(directory)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "invalid model extraction directory"
    );
    let report = serde_json::to_value(airk_retrieval::model_catalog::inspect_catalog()?)?;
    let profile = &report["catalog"]["profiles"][0];
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
            .open(directory.join(name))?;
        output.write_all(bytes)?;
        output.sync_all()?;
    }
    Ok(())
}
