// SPDX-License-Identifier: Apache-2.0
//! Native feature observations and explicitly qualified automatic model profiles.
//! Inspection is independent of collection state and never opens model assets.
use crate::model::{ONNX_RUNTIME, OnnxManifest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Serialize)]
pub struct ModelCapabilities {
    pub operating_system: &'static str,
    pub architecture: &'static str,
    pub logical_cpus: usize,
    pub feature_detection: &'static str,
    /// OS-usable CPU instructions, not a claim about selected runtime kernels.
    pub cpu_features: Vec<&'static str>,
    pub runtime: &'static str,
}

pub fn detect_capabilities() -> ModelCapabilities {
    #[allow(unused_mut)]
    let mut features = Vec::new();
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        for (name, available) in [
            ("sse2", std::is_x86_feature_detected!("sse2")),
            ("fma", std::is_x86_feature_detected!("fma")),
            ("avx2", std::is_x86_feature_detected!("avx2")),
            ("avx512f", std::is_x86_feature_detected!("avx512f")),
            ("avx512bw", std::is_x86_feature_detected!("avx512bw")),
            ("avx512vl", std::is_x86_feature_detected!("avx512vl")),
            ("avx512vnni", std::is_x86_feature_detected!("avx512vnni")),
            ("avxvnni", std::is_x86_feature_detected!("avxvnni")),
        ] {
            if available {
                features.push(name);
            }
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        for (name, available) in [
            ("neon", std::arch::is_aarch64_feature_detected!("neon")),
            (
                "dotprod",
                std::arch::is_aarch64_feature_detected!("dotprod"),
            ),
            ("fp16", std::arch::is_aarch64_feature_detected!("fp16")),
            ("i8mm", std::arch::is_aarch64_feature_detected!("i8mm")),
        ] {
            if available {
                features.push(name);
            }
        }
    }
    features.sort_unstable();
    ModelCapabilities {
        operating_system: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
        feature_detection: if cfg!(any(
            target_arch = "x86",
            target_arch = "x86_64",
            target_arch = "aarch64"
        )) {
            "native_runtime"
        } else {
            "unavailable_for_architecture"
        },
        cpu_features: features,
        runtime: ONNX_RUNTIME,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAsset {
    pub name: String,
    pub url: Option<String>,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Qualification {
    pub verified_platforms: Vec<String>,
    pub evidence: String,
    pub scope: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelProfile {
    pub id: String,
    pub precision: String,
    pub runtime: String,
    pub qualification: Qualification,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantization: Option<QuantizationPolicy>,
    pub manifest: OnnxManifest,
    pub assets: Vec<ModelAsset>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuantizationPolicy {
    pub schema: String,
    pub activation_precision: String,
    pub signed_lowering: Option<IntegerLowering>,
    pub fp32_embeddings_retained: bool,
    pub protected_fp32_nodes: Vec<String>,
    pub constant_linear_parameters: u64,
    pub quantized_linear_parameters: u64,
    pub trained_checkpoint_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegerLowering {
    pub split_matmul_nodes: usize,
    pub shared_activation_splits: usize,
    pub maximum_inner_dimension: usize,
    pub identity: String,
    pub converted_weight_parameters: usize,
    pub weight_identity: String,
}
#[cfg(test)]
impl QuantizationPolicy {
    fn validate(&self, signed: bool) -> Result<()> {
        ensure!(
            self.schema == "airk.minilm.quantization/2"
                && self.activation_precision
                    == if signed {
                        "unsigned asymmetric8bit split into exact low7/high1 MatMulInteger passes"
                    } else {
                        "unsigned asymmetric INT8"
                    }
                && self.fp32_embeddings_retained
                && self.constant_linear_parameters == 10_616_832
                && self.quantized_linear_parameters == 7_077_888
                && self.trained_checkpoint_sha256
                    == "51517cd30b9318cfabfffcf844d6312486a4d84a5027ff88e32ce73da523b906"
                && self.protected_fp32_nodes
                    == (0..6)
                        .map(|layer| format!("/inner/encoder/layer.{layer}/output/dense/MatMul"))
                        .collect::<Vec<_>>(),
            "unapproved trained mixed-precision policy"
        );
        match (&self.signed_lowering, signed) {
            (None, false) => {}
            (Some(lowering), true) => ensure!(
                lowering.split_matmul_nodes == 30
                    && lowering.shared_activation_splits == 18
                    && lowering.maximum_inner_dimension == 32768
                    && lowering.converted_weight_parameters == 60
                    && lowering.identity
                        == "(Alo-Zlo)(B-Zb)+128(Ahi-Zhi)(B-Zb)=(A-Za)(B-Zb); Alo,Zlo<=127, Ahi,Zhi<=1, signed weights/zero-points[-128,127]"
                    && lowering.weight_identity
                        == "Qs=Qu-128; Zs=Zu-128; identical scale bytes, so S(Qs-Zs)=S(Qu-Zu) exactly",
                "unapproved signed integer lowering"
            ),
            _ => anyhow::bail!("mixed-precision signed lowering differs"),
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCatalog {
    pub schema: u32,
    pub default_profile: String,
    pub profiles: Vec<ModelProfile>,
}

pub const FP16_PROFILE: &str = "minilm-l6-v2-fp16-v1";
pub const FP16_MODEL_SHA256: &str =
    "f3c3a4eed373650ba1e2f4d3d53437f1ecf92d4c50e5994e07e163751923ee4a";

impl ModelCatalog {
    fn validate_fp16(&self) -> Result<()> {
        ensure!(
            self.schema == 2 && self.default_profile == FP16_PROFILE && self.profiles.len() == 1,
            "invalid shared MiniLM catalog"
        );
        let profile = &self.profiles[0];
        let retained = legacy_catalog()?;
        let mut expected = retained
            .profiles
            .iter()
            .find(|old| old.precision == "uint8")
            .context("original publisher ABI missing")?
            .manifest
            .clone();
        expected.model_sha256 = FP16_MODEL_SHA256.into();
        ensure!(
            profile.id == FP16_PROFILE
                && profile.runtime == crate::model::ONNX_FP16_RUNTIME
                && profile.precision == "float16"
                && profile.quantization.is_none()
                && profile.manifest == expected
                && profile.assets.len() == 3,
            "invalid shared FP16 MiniLM profile"
        );
        let mut names = BTreeSet::new();
        for asset in &profile.assets {
            let (digest, bytes) = match asset.name.as_str() {
                "model.onnx" => (&expected.model_sha256, 45_469_884),
                "tokenizer.json" => (&expected.tokenizer_sha256, 466_247),
                "LICENSE" => (&expected.license_sha256, 11_358),
                _ => anyhow::bail!("unexpected shared FP16 MiniLM asset"),
            };
            ensure!(
                names.insert(&asset.name)
                    && asset.url.is_none()
                    && asset.sha256 == *digest
                    && asset.bytes == bytes,
                "shared FP16 MiniLM asset differs from its pin"
            );
        }
        Ok(())
    }

    #[cfg(test)]
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 2
                && self.profiles.len() == 2
                && self.default_profile == "minilm-l6-v2-u8-v3",
            "unsupported original baseline catalog"
        );
        let retained = legacy_catalog()?;
        let mut ids = BTreeSet::new();
        for profile in &self.profiles {
            let (hash, size, precision) = match (profile.id.as_str(), profile.precision.as_str()) {
                ("minilm-l6-v2-u8-v3", "uint8") => (
                    "b941bf19f1f1283680f449fa6a7336bb5600bdcd5f84d10ddc5cd72218a0fd21",
                    23046789,
                    "uint8",
                ),
                ("minilm-l6-v2-s8-v3", "int8") => (
                    "4278337fd0ff3c68bfb6291042cad8ab363e1d9fbc43dcb499fe91c871902474",
                    23026053,
                    "int8",
                ),
                _ => anyhow::bail!("only exact original CPU profiles are active"),
            };
            ensure!(
                ids.insert(&profile.id)
                    && profile.runtime == ONNX_RUNTIME
                    && profile.quantization.is_none(),
                "invalid original baseline profile/policy"
            );
            let original = retained
                .profiles
                .iter()
                .find(|old| old.precision == precision)
                .context("original baseline metadata missing")?;
            ensure!(
                profile.manifest == original.manifest && profile.manifest.model_sha256 == hash,
                "original publisher manifest/ABI differs"
            );
            ensure!(
                profile.assets.len() == 3,
                "original asset inventory differs"
            );
            let mut names = BTreeSet::new();
            for asset in &profile.assets {
                let (pin, length) = match asset.name.as_str() {
                    "model.onnx" => (&profile.manifest.model_sha256, size),
                    "tokenizer.json" => (&profile.manifest.tokenizer_sha256, 466247),
                    "LICENSE" => (&profile.manifest.license_sha256, 11358),
                    _ => anyhow::bail!("unsupported original model asset"),
                };
                ensure!(
                    names.insert(&asset.name)
                        && asset.sha256 == *pin
                        && asset.bytes == length
                        && asset.url.is_none(),
                    "original asset identity/length/download policy differs"
                );
            }
        }
        Ok(())
    }
}
fn legacy_catalog() -> Result<ModelCatalog> {
    use sha2::{Digest, Sha256};
    let bytes = include_bytes!("model_catalog_v1.json");
    ensure!(
        format!("{:x}", Sha256::digest(bytes))
            == "5250cecb29775ac45d8d813f83403c011541d172f6829866e9c3ec76a9888a67",
        "retained v1 catalog identity changed"
    );
    Ok(serde_json::from_slice(bytes)?)
}

#[cfg(test)]
mod baseline_emitter;
#[cfg(test)]
mod policy_cases;

#[derive(Debug, Serialize)]
pub struct ModelSelection {
    pub profile_id: String,
    pub reason: &'static str,
    pub platform_qualified: bool,
    pub short_input_batch_size: usize,
}

#[derive(Debug, Serialize)]
pub struct ModelCatalogReport {
    pub schema: u32,
    pub capabilities: ModelCapabilities,
    pub catalog: ModelCatalog,
    pub selection: ModelSelection,
    pub automatic_int8: bool,
}

fn select_profile(catalog: &ModelCatalog, hardware: &ModelCapabilities) -> Result<ModelSelection> {
    catalog.validate_fp16()?;
    ensure!(
        matches!(hardware.architecture, "aarch64" | "x86_64"),
        "unsupported MiniLM architecture"
    );
    let profile = &catalog.profiles[0];
    let platform = format!("{}-{}", hardware.operating_system, hardware.architecture);
    Ok(ModelSelection {
        profile_id: profile.id.clone(),
        reason: "shared_fp16_model",
        platform_qualified: profile.qualification.verified_platforms.contains(&platform),
        // Dynamic activation quantization is sensitive to other rows in a batch.
        // Keep per-text cache keys independent of unrelated inputs on every CPU.
        short_input_batch_size: 1,
    })
}

/// Only these exact pinned exports can execute; arbitrary/FP32 graphs cannot.
pub(crate) fn contains_active_manifest(manifest: &OnnxManifest) -> Result<bool> {
    let catalog: ModelCatalog = serde_json::from_str(include_str!("model_catalog.json"))?;
    catalog.validate_fp16()?;
    Ok(catalog
        .profiles
        .iter()
        .any(|profile| profile.manifest == *manifest))
}

pub(crate) fn contains_active_asset_references(
    set: &airk_core::model_objects::ModelObjectSet,
) -> Result<bool> {
    use airk_core::model_objects::ModelAssetName;
    set.validate()?;
    let catalog: ModelCatalog = serde_json::from_str(include_str!("model_catalog.json"))?;
    catalog.validate_fp16()?;
    Ok(catalog.profiles.iter().any(|profile| {
        [
            (ModelAssetName::ModelOnnx, "model.onnx"),
            (ModelAssetName::TokenizerJson, "tokenizer.json"),
            (ModelAssetName::License, "LICENSE"),
        ]
        .into_iter()
        .all(|(name, file)| {
            let Some(reference) = set.assets.get(&name) else {
                return false;
            };
            profile.assets.iter().any(|asset| {
                asset.name == file
                    && reference.sha256.to_string() == asset.sha256
                    && reference.bytes == asset.bytes
            })
        })
    }))
}

pub fn approved_manifest(manifest: &OnnxManifest) -> Result<()> {
    let catalog: ModelCatalog = serde_json::from_str(include_str!("model_catalog.json"))?;
    catalog.validate_fp16()?;
    ensure!(
        catalog.profiles.iter().any(|p| p.manifest == *manifest),
        "semantic_model_upgrade_required: AIRK requires its bundled MiniLM model"
    );
    Ok(())
}

pub fn selected_profile() -> Result<ModelProfile> {
    let report = inspect_catalog()?;
    report
        .catalog
        .profiles
        .into_iter()
        .find(|p| p.id == report.selection.profile_id)
        .context("selected MiniLM profile missing")
}

pub fn inspect_catalog() -> Result<ModelCatalogReport> {
    let catalog: ModelCatalog = serde_json::from_str(include_str!("model_catalog.json"))
        .context("parse embedded model catalog")?;
    let capabilities = detect_capabilities();
    let selection = select_profile(&catalog, &capabilities)?;
    Ok(ModelCatalogReport {
        schema: 1,
        capabilities,
        catalog,
        selection,
        automatic_int8: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_uses_the_shared_weights_on_each_supported_cpu() -> Result<()> {
        let mut report = inspect_catalog()?;
        for (architecture, features, expected) in [
            ("x86_64", vec![], FP16_PROFILE),
            ("x86_64", vec!["avx2", "avx512f"], FP16_PROFILE),
            ("x86_64", vec!["avxvnni"], FP16_PROFILE),
            ("x86_64", vec!["avx512vnni"], FP16_PROFILE),
            ("aarch64", vec!["neon"], FP16_PROFILE),
        ] {
            report.capabilities.architecture = architecture;
            report.capabilities.cpu_features = features;
            let selection = select_profile(&report.catalog, &report.capabilities)?;
            assert_eq!(selection.profile_id, expected);
            assert_eq!(selection.short_input_batch_size, 1);
        }
        report.capabilities.architecture = "unknown";
        assert!(select_profile(&report.catalog, &report.capabilities).is_err());
        let mut old = report.catalog.profiles[0].manifest.clone();
        old.model_sha256 =
            "6fd5d72fe4589f189f8ebc006442dbb529bb7ce38f8082112682524616046452".into();
        assert!(approved_manifest(&old).is_err());
        Ok(())
    }

    #[test]
    fn catalog_rejects_corrupt_asset_bindings_and_duplicate_paths() -> Result<()> {
        let mut report = inspect_catalog()?;
        report.catalog.profiles[0].assets[0].sha256 = "00".repeat(32);
        assert!(report.catalog.validate_fp16().is_err());
        let mut report = inspect_catalog()?;
        report.catalog.profiles[0].assets[1] = report.catalog.profiles[0].assets[0].clone();
        assert!(report.catalog.validate_fp16().is_err());
        Ok(())
    }
}
