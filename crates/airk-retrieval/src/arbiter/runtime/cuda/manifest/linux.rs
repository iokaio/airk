// SPDX-License-Identifier: Apache-2.0
//! Authenticated Linux x86-64 CUDA manifest and public build evidence.
use super::*;
use airk_core::arbiter::{Asset, AssetRole};
use sha2::{Digest, Sha256};

#[cfg(feature = "test-support")]
mod assembly;
#[cfg(feature = "test-support")]
pub use assembly::{
    assemble_scientific, encode_build_provenance, encode_empty_product, provenance_build,
};

mod policy;
use policy::{asset_name, required_variant, validate_fits, validate_requirements};
pub use policy::{
    compatibility_policy_digest, product_compatibility_policy_digest, requirement_policy,
};

const TARGET: &str = "x86_64-unknown-linux-gnu";
const SHARED: &str = "libonnxruntime_providers_shared.so";
const PROVIDER: &str = "libonnxruntime_providers_cuda.so";
const DRIVER: &str = "libcuda.so.1";
const ZERO: Sha256Digest = Sha256Digest::from_bytes([0; 32]);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElfFile {
    pub name: String,
    pub soname: Option<String>,
    pub bytes: u64,
    pub sha256: Sha256Digest,
    pub needed: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Closure {
    target: String,
    ort_version: String,
    static_core_build: Sha256Digest,
    provenance: Asset,
    files: Vec<ElfFile>,
    load_order: Vec<String>,
    system_imports: Vec<ElfFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
pub enum Descriptor {
    #[serde(rename = "airk.native.linux-cuda-runtime-manifest/1")]
    Scientific {
        closure: Closure,
        fits: Vec<VariantFit>,
    },
    #[serde(rename = "airk.native.linux-cuda-runtime-manifest/2")]
    Product {
        closure: Closure,
        requirements: Vec<VariantRequirement>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    schema: String,
    target: String,
    static_core_build: Sha256Digest,
    ort_commit: String,
    source_archive_sha256: Sha256Digest,
    source_projection_sha256: Sha256Digest,
    cuda_version: String,
    cudnn_version: String,
    architectures: Vec<[u32; 2]>,
    provider_shared_sha256: Sha256Digest,
    provider_cuda_sha256: Sha256Digest,
    aggregate_static_sha256: Sha256Digest,
    components: BTreeMap<String, Sha256Digest>,
    legal: Vec<OriginalNotice>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalNotice {
    component: String,
    source_sha256: Sha256Digest,
    asset: Asset,
}

fn elf_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.contains("..")
        && s.bytes()
            .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'.' | b'_' | b'-' | b'+'))
        && (s.ends_with(".so")
            || s.split_once(".so.").is_some_and(|(_, v)| {
                !v.is_empty()
                    && v.split('.')
                        .all(|d| !d.is_empty() && d.bytes().all(|c| c.is_ascii_digit()))
            }))
}

// cuDNN and NVRTC resolve additional components at runtime. Keep these
// authenticated and available before the provider's first initialization.
const REQUIRED_PAYLOAD: &[&str] = &[
    SHARED,
    PROVIDER,
    "libcudart.so.12",
    "libcublas.so.12",
    "libcublasLt.so.12",
    "libcufft.so.11",
    "libcurand.so.10",
    "libnvJitLink.so.12",
    "libnvrtc.so.12",
    "libnvrtc-builtins.so.12.8",
    "libcudnn.so.9",
    "libcudnn_adv.so.9",
    "libcudnn_cnn.so.9",
    "libcudnn_engines_precompiled.so.9",
    "libcudnn_engines_runtime_compiled.so.9",
    "libcudnn_graph.so.9",
    "libcudnn_heuristic.so.9",
    "libcudnn_ops.so.9",
];

#[cfg(feature = "test-support")]
pub fn linux_cuda_payload_names() -> &'static [&'static str] {
    REQUIRED_PAYLOAD
}

pub(in super::super) fn payload_name(s: &str) -> bool {
    REQUIRED_PAYLOAD.contains(&s)
}

pub(in super::super) fn system_name(s: &str) -> bool {
    matches!(
        s,
        DRIVER
            | "ld-linux-x86-64.so.2"
            | "libc.so.6"
            | "libdl.so.2"
            | "libm.so.6"
            | "librt.so.1"
            | "libpthread.so.0"
            | "libgcc_s.so.1"
            | "libstdc++.so.6"
            | "libnuma.so.1"
            | "libz.so.1"
            | "libdxcore.so"
            | "libd3d12.so"
            | "libd3d12core.so"
    )
}

impl Closure {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.target == TARGET && self.ort_version == "1.30.0" && self.static_core_build != ZERO,
            "invalid Linux CUDA static build"
        );
        let p = &self.provenance;
        ensure!(
            p.name == "airk-linux-cuda-build-provenance.json"
                && p.role == AssetRole::Notice
                && (1..=1 << 20).contains(&p.bytes)
                && p.sha256 != ZERO,
            "invalid Linux CUDA provenance asset"
        );
        ensure!(
            (2..=64).contains(&self.files.len()) && (1..=64).contains(&self.system_imports.len()),
            "Linux CUDA closure capacity"
        );
        let mut known = BTreeSet::new();
        let mut total = 0u64;
        for (rows, system) in [(&self.files, false), (&self.system_imports, true)] {
            for file in rows {
                let soname = file.soname.as_deref();
                ensure!(
                    elf_name(&file.name)
                        && known.insert(&file.name)
                        && if system {
                            system_name(&file.name)
                        } else {
                            payload_name(&file.name)
                        },
                    "invalid or duplicate Linux CUDA ELF image"
                );
                ensure!(
                    (soname == Some(file.name.as_str())
                        || (system && file.name == "libdxcore.so" && soname.is_none()))
                        && (1..=2u64 << 30).contains(&file.bytes)
                        && file.sha256 != ZERO
                        && file.needed.len() <= 128
                        && file.needed.iter().collect::<BTreeSet<_>>().len() == file.needed.len(),
                    "invalid Linux CUDA ELF metadata"
                );
                total = total
                    .checked_add(file.bytes)
                    .context("CUDA ELF byte overflow")?;
            }
        }
        ensure!(
            total <= 8u64 << 30
                && self.files.len() == REQUIRED_PAYLOAD.len()
                && self.system_imports.iter().any(|row| row.name == DRIVER)
                && self.load_order.last().is_some_and(|name| name == PROVIDER),
            "missing required Linux CUDA images"
        );
        let mut available: BTreeSet<&str> = self
            .system_imports
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        for file in &self.system_imports {
            ensure!(
                file.needed.iter().all(|n| available.contains(n.as_str())),
                "unresolved Linux system dependency"
            );
        }
        ensure!(
            self.files.len() == self.load_order.len(),
            "invalid Linux CUDA load order length"
        );
        for name in &self.load_order {
            let file = self
                .files
                .iter()
                .find(|v| v.name == *name)
                .context("unlisted ELF in load order")?;
            ensure!(
                file.needed.iter().all(|n| available.contains(n.as_str()))
                    && available.insert(name),
                "dependency not pinned and loaded before ELF or duplicate load"
            );
        }
        Ok(())
    }
}

impl Descriptor {
    fn closure(&self) -> &Closure {
        match self {
            Self::Scientific { closure, .. } | Self::Product { closure, .. } => closure,
        }
    }

    pub fn parse(raw: &[u8]) -> Result<Self> {
        ensure!(raw.len() <= 128 << 10, "Linux CUDA descriptor byte cap");
        let value: Self = serde_json::from_slice(raw)?;
        value.closure().validate()?;
        match &value {
            Self::Scientific { fits, .. } => {
                validate_fits(fits, TARGET)?;
                ensure!(
                    fits.iter().all(|f| f.manifest != ZERO
                        && f.execution_identity.len() <= 512
                        && value
                            .system_imports()
                            .iter()
                            .any(|driver| driver.name == DRIVER
                                && driver.sha256 == f.driver_image_sha256)),
                    "invalid Linux CUDA scientific fit"
                );
            }
            Self::Product { requirements, .. } => validate_requirements(requirements, TARGET)?,
        }
        Ok(value)
    }

    pub fn approved(raw: &[u8]) -> Result<(Self, Sha256Digest)> {
        let pin = option_env!("AIRK_APPROVED_LINUX_CUDA_MANIFEST_SHA256").context(
            RuntimeUnavailable::new("Linux CUDA manifest approval not compiled"),
        )?;
        let build = option_env!("AIRK_STATIC_LINUX_CUDA_BUILD_SHA256").context(
            RuntimeUnavailable::new("Linux static CUDA build approval absent"),
        )?;
        ensure!(raw.len() <= 128 << 10, "Linux CUDA descriptor byte cap");
        let digest = Sha256Digest::from_bytes(Sha256::digest(raw).into());
        ensure!(
            digest.to_string() == pin,
            "unapproved Linux CUDA manifest bytes"
        );
        let value = Self::parse(raw)?;
        ensure!(
            value.build().to_string() == build,
            "Linux CUDA static build differs"
        );
        Ok((value, digest))
    }

    pub fn build(&self) -> Sha256Digest {
        self.closure().static_core_build
    }
    pub fn files(&self) -> &[ElfFile] {
        &self.closure().files
    }
    pub fn load_order(&self) -> &[String] {
        &self.closure().load_order
    }
    pub fn system_imports(&self) -> &[ElfFile] {
        &self.closure().system_imports
    }
    pub fn provenance(&self) -> &Asset {
        &self.closure().provenance
    }
    pub fn is_product(&self) -> bool {
        matches!(self, Self::Product { .. })
    }

    pub fn verify_provenance(&self, raw: &[u8]) -> Result<Vec<Asset>> {
        let asset = self.provenance();
        ensure!(
            raw.len() <= 1 << 20
                && raw.len() as u64 == asset.bytes
                && Sha256Digest::from_bytes(Sha256::digest(raw).into()) == asset.sha256,
            "Linux provenance asset mismatch"
        );
        let p: Provenance = serde_json::from_slice(raw)?;
        ensure!(
            p.schema == "airk.native.linux-cuda-build-provenance/1"
                && p.target == TARGET
                && p.static_core_build == self.build()
                && p.ort_commit == "f2c39fe2f838cf35ce7da92824f5a5e3ee6e88a7"
                && p.source_archive_sha256.to_string()
                    == "1b4ec55fa26ee8fe622ea256894dfe9381bdfa99d41489994a1961720f556c48"
                && p.source_projection_sha256 != ZERO
                && p.aggregate_static_sha256 != ZERO
                && p.cuda_version == "12.8.1"
                && p.cudnn_version == "9.10.2.21"
                && p.architectures == [[8, 6]],
            "Linux CUDA public producer identity mismatch"
        );
        for (name, hash) in [
            (SHARED, p.provider_shared_sha256),
            (PROVIDER, p.provider_cuda_sha256),
        ] {
            ensure!(
                self.files()
                    .iter()
                    .any(|f| f.name == name && f.sha256 == hash),
                "Linux same-build provider mismatch"
            );
        }
        let required = [
            "onnxruntime",
            "abseil_cpp",
            "date",
            "eigen",
            "flatbuffers",
            "fp16",
            "json",
            "microsoft_gsl",
            "microsoft_wil",
            "mp11",
            "onnx",
            "protobuf",
            "pytorch_cpuinfo",
            "re2",
            "safeint",
            "cutlass",
            "cudnn_frontend",
            "cuda_cccl",
            "cuda_cudart",
            "cuda_nvcc",
            "cuda_nvrtc",
            "cuda_nvtx",
            "cuda_cupti",
            "cuda_profiler_api",
            "libcublas",
            "libcurand",
            "libcufft",
            "libnvjitlink",
            "cudnn",
        ];
        ensure!(
            p.components.len() == required.len()
                && required
                    .iter()
                    .all(|k| p.components.get(*k).is_some_and(|h| *h != ZERO))
                && p.components["onnxruntime"] == p.source_archive_sha256,
            "incomplete Linux CUDA component/source inventory"
        );
        ensure!(
            (required.len()..=512).contains(&p.legal.len()),
            "Linux CUDA notice count"
        );
        let mut seen = BTreeSet::new();
        let mut covered = BTreeSet::new();
        let mut total = 0u64;
        for notice in &p.legal {
            let asset = &notice.asset;
            ensure!(
                p.components.get(&notice.component) == Some(&notice.source_sha256)
                    && asset.role == AssetRole::Notice
                    && asset_name(&asset.name)
                    && seen.insert(&asset.name)
                    && asset
                        .name
                        .starts_with(&format!("airk-linux-cuda-notice-{}-", notice.component))
                    && [".txt", ".html", ".md"]
                        .iter()
                        .any(|suffix| asset.name.ends_with(suffix))
                    && (1..=2 << 20).contains(&asset.bytes)
                    && asset.sha256 != ZERO,
                "invalid Linux CUDA original legal asset"
            );
            total = total
                .checked_add(asset.bytes)
                .context("Linux CUDA notice overflow")?;
            covered.insert(&notice.component);
        }
        ensure!(
            covered.len() == p.components.len() && total <= 16 << 20,
            "incomplete Linux CUDA legal coverage"
        );
        Ok(p.legal.into_iter().map(|n| n.asset).collect())
    }

    pub fn fits(&self) -> Result<&[VariantFit]> {
        match self {
            Self::Scientific { fits, .. } => Ok(fits),
            Self::Product { .. } => anyhow::bail!("product requirements cannot be scientific fits"),
        }
    }

    pub fn product_has_adapter_class(&self, id: &AdapterIdentity) -> bool {
        matches!(self, Self::Product {requirements,..} if requirements.iter().any(|r| r.matches_adapter_class(id)))
    }

    pub fn requirement(&self, profile: &Profile) -> Result<&VariantRequirement> {
        let Self::Product { requirements, .. } = self else {
            anyhow::bail!("scientific fit cannot grant product eligibility")
        };
        required_variant(requirements, profile)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(in super::super) fn scientific_test_manifest() -> serde_json::Value {
    tests::descriptor(false)
}
