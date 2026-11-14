// SPDX-License-Identifier: Apache-2.0
//! Explicit offline extraction of verified embedded model assets.
/// Exercise the embedded graph inside an exact private native service fixture.
/// This does not register projects, publish an index, or touch the user's runtime.
#[cfg(all(feature = "bundled-model", feature = "local-inference"))]
pub fn verify_model_smoke() -> Result<serde_json::Value> {
    use anyhow::Context;
    let mut fixture_builder = tempfile::Builder::new();
    fixture_builder.prefix("airk-model-smoke-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fixture_builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    let fixture = fixture_builder.tempdir()?;
    let runtime = fixture.path().join("runtime");
    crate::local_ipc::prepare_root(&runtime)?;
    let ownership = std::sync::Arc::new(airk_store::service::ServiceLease::acquire(&runtime)?);
    let report = airk_kernel::inference_owner::run_service_inference(
        ownership,
        || -> Result<serde_json::Value> {
            let plan = EmbeddedAssets::plan()?;
            let model =
                airk_retrieval::model::EmbeddingModel::open_native_assets(&EmbeddedAssets, &plan)?;
            let first = model
                .embed_text("cobalt harbor")?
                .context("embedded model produced no vector")?;
            let second = model
                .embed_text("cobalt harbor")?
                .context("embedded model produced no repeat vector")?;
            ensure!(
                first.len() == 384
                    && first.len() == model.dimensions()
                    && first.iter().all(|value| value.is_finite())
                    && first == second,
                "embedded model smoke output differs from approved contract"
            );
            Ok(
                serde_json::json!({"schema":1,"source":"embedded","engine":airk_retrieval::model::ONNX_RUNTIME_ENGINE,"profile":airk_retrieval::model_catalog::selected_profile()?.id,"legacy_identity":plan.legacy_identity,"pipeline_digest":plan.pipeline_digest,"dimensions":first.len(),"repeat_equal":true}),
            )
        },
    )??;
    fixture.close()?;
    Ok(report)
}
use anyhow::Result;
#[cfg(feature = "bundled-model")]
use anyhow::ensure;
#[cfg(feature = "bundled-model")]
use sha2::{Digest, Sha256};
#[cfg(feature = "bundled-model")]
use std::{fs, io::Write};
#[cfg(feature = "bundled-model")]
include!(concat!(env!("OUT_DIR"), "/bundled_model.rs"));

fn manifest_bytes() -> Result<&'static [u8]> {
    Ok(
        match airk_retrieval::model_catalog::selected_profile()?
            .id
            .as_str()
        {
            "minilm-l6-v2-u8-v2" => {
                include_bytes!(concat!(env!("OUT_DIR"), "/minilm-l6-v2-u8-v2/model.json"))
            }
            "minilm-l6-v2-s8-v2" => {
                include_bytes!(concat!(env!("OUT_DIR"), "/minilm-l6-v2-s8-v2/model.json"))
            }
            _ => anyhow::bail!("unsupported bundled model"),
        },
    )
}
pub fn manifest() -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(manifest_bytes()?)?)
}

/// Bind only authenticated model metadata to a worker/observer. Native model
/// initialization remains owned by the service's inference capability.
pub fn bind_service_models(
    kernel: airk_kernel::Kernel,
    runtime: &std::path::Path,
) -> Result<airk_kernel::Kernel> {
    #[cfg(feature = "bundled-model")]
    {
        kernel.with_embedded_service_models(
            runtime,
            default_plan()?,
            std::sync::Arc::new(EmbeddedAssets),
        )
    }
    #[cfg(not(feature = "bundled-model"))]
    {
        kernel.with_service_models(runtime, default_plan()?)
    }
}
#[cfg(feature = "bundled-model")]
fn assets() -> Result<&'static [(&'static str, &'static [u8])]> {
    let id = airk_retrieval::model_catalog::selected_profile()?.id;
    PROFILES
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, assets)| *assets)
        .ok_or_else(|| anyhow::anyhow!("selected embedded model absent"))
}

#[cfg(feature = "bundled-model")]
pub fn extract(directory: &std::path::Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(directory)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "invalid model extraction directory"
    );
    let profile = serde_json::to_value(airk_retrieval::model_catalog::selected_profile()?)?;
    for &(name, bytes) in assets()? {
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
        assets()?
            .iter()
            .find(|(key, _)| *key == name.filename())
            .map(|(_, bytes)| *bytes)
            .ok_or_else(|| anyhow::anyhow!("embedded model asset absent"))
    }
    fn metadata_plan() -> Result<airk_core::model_objects::ModelObjectSet> {
        let set = default_plan()?;
        for &(name, bytes) in assets()? {
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
    fn borrowed_asset(
        &self,
        name: airk_core::model_objects::ModelAssetName,
        maximum: u64,
    ) -> Result<Option<&[u8]>> {
        let bytes = Self::bytes(name)?;
        ensure!(
            bytes.len() as u64 <= maximum.min(name.maximum_bytes()),
            "embedded model asset exceeds budget"
        );
        Ok(Some(bytes))
    }
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
            let bytes = manifest_bytes()?;
            let profile = serde_json::to_value(airk_retrieval::model_catalog::selected_profile()?)?;
            let assets = profile["assets"]
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

/// Packaging checks actual embedded bytes, not just the main executable's metadata.
#[cfg(feature = "bundled-model")]
pub fn verify_inventory() -> Result<serde_json::Value> {
    let catalog = airk_retrieval::model_catalog::inspect_catalog()?.catalog;
    ensure!(
        PROFILES.len() == 2,
        "exactly two INT8 profiles must be embedded"
    );
    let mut inventory = Vec::new();
    for profile in catalog.profiles {
        let embedded = PROFILES
            .iter()
            .find(|(id, _)| *id == profile.id)
            .ok_or_else(|| anyhow::anyhow!("embedded INT8 profile absent"))?
            .1;
        ensure!(embedded.len() == 4, "unexpected embedded model inventory");
        for asset in &profile.assets {
            let bytes = embedded
                .iter()
                .find(|(name, _)| *name == asset.name)
                .ok_or_else(|| anyhow::anyhow!("embedded INT8 asset absent"))?
                .1;
            ensure!(
                bytes.len() as u64 == asset.bytes
                    && format!("{:x}", Sha256::digest(bytes)) == asset.sha256,
                "embedded INT8 asset failed verification"
            );
        }
        let manifest = embedded
            .iter()
            .find(|(name, _)| *name == "model.json")
            .ok_or_else(|| anyhow::anyhow!("embedded manifest absent"))?
            .1;
        ensure!(
            serde_json::from_slice::<serde_json::Value>(manifest)?
                == serde_json::to_value(&profile.manifest)?,
            "embedded manifest differs"
        );
        inventory.push(serde_json::json!({"id":profile.id,"precision":profile.precision,"manifest":profile.manifest,"assets":profile.assets}));
    }
    Ok(serde_json::json!({"schema":1,"fp32_included":false,"profiles":inventory}))
}
