// SPDX-License-Identifier: Apache-2.0
//! Native feature observations and explicitly qualified automatic model profiles.
//! Inspection is independent of collection state and never opens model assets.
use crate::model::{MAX_MODEL_BYTES, MAX_TOKENIZER_BYTES, ONNX_RUNTIME, OnnxManifest};
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

impl ModelCatalog {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 2
                && self.profiles.len() == 2
                && self.default_profile == "minilm-l6-v2-u8-v2",
            "unsupported model catalog"
        );
        let mut ids = BTreeSet::new();
        for profile in &self.profiles {
            ensure!(
                ids.insert(&profile.id)
                    && profile.runtime == ONNX_RUNTIME
                    && profile.manifest.schema == 2,
                "invalid INT8 profile"
            );
            let (approved_hash, approved_size, signed) =
                match (profile.id.as_str(), profile.precision.as_str()) {
                    ("minilm-l6-v2-u8-v2", "mixed-u8") => (
                        "e7399e81737d3f9707a304be116a229983bc9fff0974d1b087293789021d2d7b",
                        69_325_926,
                        false,
                    ),
                    ("minilm-l6-v2-s8-v2", "mixed-s8") => (
                        "4788597b6d26061b778ac6255723d4bb004e4927446c480ecaebe19e4ea7f47c",
                        69_355_444,
                        true,
                    ),
                    _ => anyhow::bail!("FP32 and unknown model precisions are forbidden"),
                };
            profile
                .quantization
                .as_ref()
                .context("trained quantization policy missing")?
                .validate(signed)?;
            let mut expected = legacy_catalog()?
                .profiles
                .into_iter()
                .find(|old| old.precision == if signed { "int8" } else { "uint8" })
                .context("retained base profile missing")?
                .manifest;
            expected.model_sha256 = approved_hash.into();
            expected.license_sha256 =
                "9cc7fbd69f78ff19a188025976df20d5c17b0cec5ed79ad5d7e16d2094e46abd".into();
            ensure!(
                profile.manifest == expected,
                "trained model base/tokenizer/ABI metadata differs"
            );
            ensure!(
                profile.manifest.model_sha256 == approved_hash,
                "unapproved weight hash"
            );
            ensure!(
                profile.assets.len() == 3,
                "model catalog asset inventory differs"
            );
            let mut names = BTreeSet::new();
            for asset in &profile.assets {
                let (expected, maximum, exact_size) = match asset.name.as_str() {
                    "model.onnx" => (
                        &profile.manifest.model_sha256,
                        MAX_MODEL_BYTES,
                        approved_size,
                    ),
                    "tokenizer.json" => (
                        &profile.manifest.tokenizer_sha256,
                        MAX_TOKENIZER_BYTES,
                        466_247,
                    ),
                    "LICENSE" => (&profile.manifest.license_sha256, 262_144, 34_889),
                    _ => anyhow::bail!("unsupported model catalog asset"),
                };
                ensure!(
                    names.insert(&asset.name)
                        && asset.bytes > 0
                        && asset.bytes <= maximum
                        && asset.sha256 == *expected
                        && asset.sha256.len() == 64
                        && asset
                            .sha256
                            .bytes()
                            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                        && asset.bytes == exact_size
                        && asset.url.is_none(),
                    "invalid model catalog asset binding"
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
    catalog.validate()?;
    let (id, reason) = match hardware.architecture {
        "aarch64" => ("minilm-l6-v2-s8-v2", "arm64_signed_int8"),
        "x86_64"
            if hardware
                .cpu_features
                .iter()
                .any(|feature| matches!(*feature, "avxvnni" | "avx512vnni")) =>
        {
            ("minilm-l6-v2-s8-v2", "vnni_signed_int8")
        }
        "x86_64" => ("minilm-l6-v2-u8-v2", "portable_unsigned_int8"),
        _ => anyhow::bail!("unsupported INT8 architecture"),
    };
    let profile = catalog
        .profiles
        .iter()
        .find(|profile| profile.id == id)
        .context("INT8 profile missing")?;
    let platform = format!("{}-{}", hardware.operating_system, hardware.architecture);
    Ok(ModelSelection {
        profile_id: profile.id.clone(),
        reason,
        platform_qualified: profile.qualification.verified_platforms.contains(&platform),
        // Dynamic activation quantization is sensitive to other rows in a batch.
        // Keep per-text cache keys independent of unrelated inputs on every CPU.
        short_input_batch_size: 1,
    })
}

/// Only these exact pinned exports can execute; arbitrary/FP32 graphs cannot.
pub fn approved_manifest(manifest: &OnnxManifest) -> Result<()> {
    let catalog: ModelCatalog = serde_json::from_str(include_str!("model_catalog.json"))?;
    catalog.validate()?;
    ensure!(
        catalog.profiles.iter().any(|p| p.manifest == *manifest),
        "semantic_model_upgrade_required: AIRK 1.6 requires its bundled INT8 model"
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
        .context("selected INT8 profile missing")
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
        automatic_int8: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_covers_non_vnni_vnni_and_arm_without_fp32_fallback() -> Result<()> {
        let mut report = inspect_catalog()?;
        for (architecture, features, expected) in [
            ("x86_64", vec![], "minilm-l6-v2-u8-v2"),
            ("x86_64", vec!["avx2", "avx512f"], "minilm-l6-v2-u8-v2"),
            ("x86_64", vec!["avxvnni"], "minilm-l6-v2-s8-v2"),
            ("x86_64", vec!["avx512vnni"], "minilm-l6-v2-s8-v2"),
            ("aarch64", vec!["neon"], "minilm-l6-v2-s8-v2"),
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
        assert!(report.catalog.validate().is_err());
        let mut report = inspect_catalog()?;
        report.catalog.profiles[0].assets[1] = report.catalog.profiles[0].assets[0].clone();
        assert!(report.catalog.validate().is_err());
        Ok(())
    }
}
