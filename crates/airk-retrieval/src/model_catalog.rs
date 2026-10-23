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
    /// OS-usable CPU instructions, not a claim about selected tract kernels.
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
    pub url: String,
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
    pub manifest: OnnxManifest,
    pub assets: Vec<ModelAsset>,
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
            self.schema == 1 && self.profiles.len() == 1,
            "unsupported model catalog"
        );
        let profile = &self.profiles[0];
        // An ISA flag must never promote an unqualified future precision/export.
        ensure!(
            profile.id == self.default_profile
                && profile.precision == "fp32"
                && profile.runtime == ONNX_RUNTIME
                && profile.manifest.schema == 2,
            "model catalog has no approved FP32 default"
        );
        ensure!(
            profile.assets.len() == 3,
            "model catalog asset inventory differs"
        );
        let mut names = BTreeSet::new();
        for asset in &profile.assets {
            let (expected, maximum, url) = match asset.name.as_str() {
                "model.onnx" => (
                    &profile.manifest.model_sha256,
                    MAX_MODEL_BYTES,
                    format!(
                        "{}/resolve/{}/onnx/model.onnx",
                        profile.manifest.source, profile.manifest.revision
                    ),
                ),
                "tokenizer.json" => (
                    &profile.manifest.tokenizer_sha256,
                    MAX_TOKENIZER_BYTES,
                    format!(
                        "{}/resolve/{}/tokenizer.json",
                        profile.manifest.source, profile.manifest.revision
                    ),
                ),
                "LICENSE" => (
                    &profile.manifest.license_sha256,
                    262_144,
                    "https://www.apache.org/licenses/LICENSE-2.0.txt".to_owned(),
                ),
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
                    && asset.url == url
                    && asset.url.starts_with("https://"),
                "invalid model catalog asset binding"
            );
        }
        Ok(())
    }
}

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
    let profile = &catalog.profiles[0];
    let platform = format!("{}-{}", hardware.operating_system, hardware.architecture);
    let platform_qualified = profile.qualification.verified_platforms.contains(&platform);
    Ok(ModelSelection {
        profile_id: profile.id.clone(),
        reason: if platform_qualified {
            "qualified_fp32_default"
        } else {
            "portable_fp32_fallback_unqualified_platform"
        },
        platform_qualified,
        short_input_batch_size: if platform_qualified
            && crate::model::qualified_batch_four(&profile.manifest)
        {
            4
        } else {
            1
        },
    })
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
    fn instruction_flags_do_not_enable_an_unqualified_precision() -> Result<()> {
        let mut report = inspect_catalog()?;
        let id = report.selection.profile_id;
        for features in [
            vec![],
            vec!["avx2"],
            vec!["avx512vnni", "avxvnni"],
            vec!["neon", "dotprod"],
        ] {
            report.capabilities.cpu_features = features;
            assert_eq!(
                select_profile(&report.catalog, &report.capabilities)?.profile_id,
                id
            );
        }
        report.capabilities.operating_system = "unknown";
        report.capabilities.architecture = "unknown";
        let fallback = select_profile(&report.catalog, &report.capabilities)?;
        assert!(!fallback.platform_qualified);
        assert_eq!(fallback.short_input_batch_size, 1);
        assert_eq!(fallback.profile_id, id);
        report.catalog.profiles[0].precision = "int8".into();
        assert!(select_profile(&report.catalog, &report.capabilities).is_err());
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
