// SPDX-License-Identifier: Apache-2.0
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuntimeFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: Sha256Digest,
    pub imports: BTreeSet<String>,
    pub delay_imports: BTreeSet<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VariantFit {
    pub manifest: Sha256Digest,
    pub execution_identity: String,
    pub cold_peak_bytes: u64,
    pub warm_extra_bytes: u64,
    pub headroom_bytes: u64,
    pub host_warm_extra_bytes: u64,
    pub evidence: DeviceEvidence,
    pub adapter_uuid: [u8; 16],
    pub driver_api_version: i32,
    pub driver_image_sha256: Sha256Digest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuntimeManifest {
    pub schema: String,
    pub ort_version: String,
    pub static_core_build: Sha256Digest,
    pub producer_name: String,
    pub producer_bytes: u64,
    pub producer_sha256: Sha256Digest,
    pub files: Vec<RuntimeFile>,
    pub load_order: Vec<String>,
    pub system_imports: BTreeSet<String>,
    pub fits: Vec<VariantFit>,
}
#[cfg(any(test, feature = "test-support", all(windows, target_arch = "x86_64")))]
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value == value.to_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        && !value.contains("..")
}
impl RuntimeManifest {
    #[cfg(any(test, feature = "test-support", all(windows, target_arch = "x86_64")))]
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "airk.native.cuda-runtime-manifest/1" && self.ort_version == "1.30.0",
            "unknown CUDA runtime manifest"
        );
        ensure!(
            self.static_core_build == self.producer_sha256
                && name(&self.producer_name)
                && self.producer_bytes > 0
                && self.producer_bytes <= 16 << 20,
            "producer/static-core closure mismatch"
        );
        ensure!(
            (2..=64).contains(&self.files.len())
                && self.fits.len() <= 16
                && self.system_imports.len() <= 256,
            "CUDA closure capacity"
        );
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        for file in &self.files {
            ensure!(
                name(&file.name)
                    && file.name.ends_with(".dll")
                    && file.name != "onnxruntime.dll"
                    && file.name != "nvcuda.dll"
                    && file.bytes > 0
                    && file.bytes <= 2u64 << 30
                    && names.insert(file.name.clone()),
                "invalid or duplicate CUDA payload"
            );
            total = total
                .checked_add(file.bytes)
                .context("CUDA closure overflow")?;
            ensure!(
                total <= 8u64 << 30 && file.imports.len() <= 256 && file.delay_imports.len() <= 256,
                "CUDA closure byte/import capacity"
            );
        }
        ensure!(
            names.contains("onnxruntime_providers_shared.dll")
                && names.contains("onnxruntime_providers_cuda.dll"),
            "same-build provider pair missing"
        );
        ensure!(
            self.load_order.len() == names.len()
                && self.load_order.iter().cloned().collect::<BTreeSet<_>>() == names,
            "invalid CUDA load order"
        );
        ensure!(
            self.system_imports.iter().all(|v| name(v)
                && v.ends_with(".dll")
                && !names.contains(v)
                && v != "onnxruntime.dll"),
            "invalid system import closure"
        );
        for file in &self.files {
            ensure!(
                file.imports
                    .iter()
                    .chain(&file.delay_imports)
                    .all(|v| names.contains(v) || self.system_imports.contains(v)),
                "unresolved native DLL import"
            );
        }
        let mut manifests = BTreeSet::new();
        for fit in &self.fits {
            ensure!(
                manifests.insert(fit.manifest) && !fit.execution_identity.is_empty(),
                "duplicate CUDA fit"
            );
            DeviceCapacityPlan::new(
                fit.cold_peak_bytes,
                fit.warm_extra_bytes,
                fit.headroom_bytes,
            )?;
            ensure!(
                fit.adapter_uuid != [0; 16]
                    && fit.driver_api_version >= 12080
                    && fit.evidence.numerical != fit.evidence.resources
                    && fit.evidence.numerical != fit.evidence.placement
                    && fit.evidence.resources != fit.evidence.placement
                    && [
                        fit.evidence.numerical,
                        fit.evidence.resources,
                        fit.evidence.placement,
                        fit.driver_image_sha256
                    ]
                    .iter()
                    .all(|v| *v != Sha256Digest::from_bytes([0; 32]))
                    && fit.evidence.target == super::super::current_target()
                    && fit.evidence.execution_identity == fit.execution_identity,
                "invalid native GPU evidence/host phase fit"
            );
        }
        Ok(())
    }
    #[cfg(any(test, all(windows, target_arch = "x86_64")))]
    pub fn approved(bytes: &[u8]) -> Result<(Self, Sha256Digest)> {
        let approved = option_env!("AIRK_APPROVED_CUDA_MANIFEST_SHA256")
            .context(RuntimeUnavailable::new("manifest approval not compiled"))?;
        let static_build = option_env!("AIRK_STATIC_CUDA_BUILD_SHA256")
            .context(RuntimeUnavailable::new("static build provenance absent"))?;
        let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
        ensure!(
            digest.to_string() == approved,
            "unapproved runtime manifest bytes"
        );
        let value: Self = serde_json::from_slice(bytes)?;
        value.validate()?;
        ensure!(
            value.static_core_build.to_string() == static_build,
            "GPU DLLs differ from linked static runtime build"
        );
        Ok((value, digest))
    }
}
pub fn compatibility_policy_digest() -> Sha256Digest {
    airk_core::identity::framed_digest(b"airk.native.cuda.compatibility/1",&[b"windows-x64;ort1.30;single-static-core;api22;cuda-driver-primary-context;sm=86;driver-api>=12080"])
}
pub fn provider_options_digest() -> Sha256Digest {
    airk_core::identity::framed_digest(b"airk.native.cuda.provider-options/1",&[b"cuda-only;error-on-failure;disable-cpu-fallback=1;tf32=0;cuda-graph=0;arena=declared-cold-peak;cpu-score-output;synchronize=ORT-default+primary-context"])
}
pub fn math_policy_digest() -> Sha256Digest {
    airk_core::identity::framed_digest(
        b"airk.native.cuda.math-policy/1",
        &[b"fp32;tf32=0;original-six-input-and-scores-abi;unchanged-numeric-limits"],
    )
}
