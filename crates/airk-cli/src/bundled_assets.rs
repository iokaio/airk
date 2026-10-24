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

#[cfg(feature = "bundled-model")]
pub struct EmbeddedAssets;
#[cfg(feature = "bundled-model")]
impl EmbeddedAssets {
    pub fn bytes(name: airk_core::model_objects::ModelAssetName) -> Result<&'static [u8]> {
        ASSETS
            .iter()
            .find(|(key, _)| *key == name.filename())
            .map(|(_, bytes)| *bytes)
            .ok_or_else(|| anyhow::anyhow!("embedded model asset absent"))
    }
    fn metadata_plan() -> Result<airk_core::model_objects::ModelObjectSet> {
        let set = default_plan()?;
        for &(name, bytes) in ASSETS {
            let asset: airk_core::model_objects::ModelAssetName =
                serde_json::from_value(serde_json::Value::String(name.into()))?;
            ensure!(
                set.assets[&asset].bytes == bytes.len() as u64,
                "embedded asset length differs from metadata"
            );
        }
        Ok(set)
    }
    pub fn plan() -> Result<airk_core::model_objects::ModelObjectSet> {
        static PLAN: std::sync::OnceLock<
            std::result::Result<airk_core::model_objects::ModelObjectSet, String>,
        > = std::sync::OnceLock::new();
        PLAN.get_or_init(|| Self::metadata_plan().map_err(|e| e.to_string()))
            .as_ref()
            .cloned()
            .map_err(|_| anyhow::anyhow!("embedded model assets failed verification"))
    }
}
#[cfg(feature = "bundled-model")]
impl airk_core::model_objects::ModelAssetSource for EmbeddedAssets {
    fn read_asset(
        &self,
        name: airk_core::model_objects::ModelAssetName,
        maximum: u64,
    ) -> Result<Vec<u8>> {
        let bytes = Self::bytes(name)?;
        ensure!(
            bytes.len() as u64 <= maximum.min(name.maximum_bytes()),
            "embedded model asset exceeds budget"
        );
        Ok(bytes.to_vec())
    }
}

/// Tiny compiled metadata is available in the inference-free main executable.
pub fn default_plan() -> Result<airk_core::model_objects::ModelObjectSet> {
    use airk_core::model_objects::{ModelAssetName, ModelObjectRef};
    use anyhow::{Context, ensure};
    use sha2::{Digest, Sha256};
    static PLAN: std::sync::OnceLock<
        std::result::Result<airk_core::model_objects::ModelObjectSet, String>,
    > = std::sync::OnceLock::new();
    PLAN.get_or_init(|| {
        (|| -> Result<_> {
            let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/model.json"));
            let report = serde_json::to_value(airk_retrieval::model_catalog::inspect_catalog()?)?;
            let assets = report["catalog"]["profiles"][0]["assets"]
                .as_array()
                .context("model catalog assets missing")?;
            let mut references = std::collections::BTreeMap::new();
            references.insert(
                ModelAssetName::ModelJson,
                ModelObjectRef {
                    sha256: airk_core::identity::Sha256Digest::from_bytes(
                        Sha256::digest(bytes).into(),
                    ),
                    bytes: bytes.len() as u64,
                },
            );
            for asset in assets {
                let name: ModelAssetName = serde_json::from_value(asset["name"].clone())?;
                ensure!(
                    name != ModelAssetName::ModelJson,
                    "duplicate manifest catalog entry"
                );
                references.insert(
                    name,
                    ModelObjectRef {
                        sha256: asset["sha256"]
                            .as_str()
                            .context("catalog digest missing")?
                            .parse()?,
                        bytes: asset["bytes"].as_u64().context("catalog length missing")?,
                    },
                );
            }
            airk_retrieval::model::plan_from_asset_references(bytes, references)
        })()
        .map_err(|e| e.to_string())
    })
    .as_ref()
    .cloned()
    .map_err(|_| anyhow::anyhow!("default model metadata is unavailable"))
}
