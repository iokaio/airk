// SPDX-License-Identifier: Apache-2.0
//! Path-free same-build runtime provenance, not runtime or model authority.
use super::*;
use airk_core::arbiter::{Asset, AssetRole};
use serde::{Deserialize, Serialize};

#[cfg(feature = "test-support")]
pub(super) mod projection;
#[cfg(test)]
mod tests;

pub(super) const NAME: &str = "airk-cuda-build-provenance.json";
const MAX_JSON: usize = 262_144;
const CUDA_ARCHITECTURES_OPTION: &str = "CMAKE_CUDA_ARCHITECTURES";

fn configured_architectures(value: &str) -> Option<&'static [[u32; 2]]> {
    match value {
        "86" => Some(&[[8, 6]]),
        "86;89;120" => Some(&[[8, 6], [8, 9], [12, 0]]),
        _ => None,
    }
}

fn safe_build_option(name: &str, value: &str) -> bool {
    token(name, 128)
        && if name == CUDA_ARCHITECTURES_OPTION {
            configured_architectures(value).is_some()
        } else {
            value.is_empty() || token(value, 256)
        }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: Sha256Digest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SourceKind {
    Archive,
    InstalledToolkitInventory,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Component {
    pub name: String,
    pub version: Option<String>,
    pub upstream_sha1: Option<String>,
    pub source_revision: Option<String>,
    pub original_url: Option<String>,
    pub source_kind: SourceKind,
    pub source_sha256: Sha256Digest,
    pub inventory_sha256: Option<Sha256Digest>,
    pub source_changes: Vec<Sha256Digest>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Tool {
    pub name: String,
    pub version: Option<String>,
    pub version_kind: Option<String>,
    pub bytes: u64,
    pub sha256: Sha256Digest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BuildOption {
    pub name: String,
    pub value: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LegalNotice {
    pub component: String,
    pub file: PublicFile,
    pub original_source_sha256: Sha256Digest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Aggregate {
    pub file: PublicFile,
    pub proof_sha256: Sha256Digest,
    pub receipt_sha256: Sha256Digest,
    pub plan_sha256: Sha256Digest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckpointSectionHistory {
    pub section_id: String,
    pub section_index: u64,
    pub receipt_sha256: Sha256Digest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckpointHistory {
    pub mode: String,
    pub cold_build_qualified: bool,
    pub qualification: bool,
    pub approved_plan_sha256: Sha256Digest,
    pub failed_ancestry: Vec<Sha256Digest>,
    pub sections: Vec<CheckpointSectionHistory>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IncrementalHistory {
    pub mode: String,
    pub parent_producer_sha256: Sha256Digest,
    pub parent_build_proof_sha256: Sha256Digest,
    pub parent_aggregate_sha256: Sha256Digest,
    pub recipe_sha256: Sha256Digest,
    pub source_before_sha256: Sha256Digest,
    pub source_after_sha256: Sha256Digest,
    pub patch_sha256: Sha256Digest,
    pub new_object_sha256: Sha256Digest,
    pub new_session_sha256: Sha256Digest,
    pub aggregate_sha256: Sha256Digest,
    pub qualification: bool,
    pub cold_build_qualified: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicRuntimeProvenance {
    pub schema: String,
    pub target: String,
    pub ort_version: String,
    pub static_core_build: Sha256Digest,
    pub producer_script_sha256: Sha256Digest,
    pub producer_lock_sha256: Sha256Digest,
    pub verifier_sha256: Sha256Digest,
    pub build_proof_sha256: Sha256Digest,
    pub configure_argv_sha256: Sha256Digest,
    pub legal_inventory_sha256: Sha256Digest,
    pub legal_owner_sha256: Sha256Digest,
    pub historical_failed_parent: Option<Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_history: Option<CheckpointHistory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incremental_history: Option<IncrementalHistory>,
    pub aggregate: Aggregate,
    pub components: Vec<Component>,
    pub tools: Vec<Tool>,
    pub options: Vec<BuildOption>,
    pub files: Vec<PublicFile>,
    pub legal: Vec<LegalNotice>,
}
fn nonzero(value: Sha256Digest) -> bool {
    value.as_bytes().iter().any(|b| *b != 0)
}
fn token(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && !value.contains("..")
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'+'))
}
fn basename(value: &str) -> bool {
    if !token(value, 128) || value != value.to_ascii_lowercase() {
        return false;
    }
    let stem = value.split('.').next().unwrap_or_default();
    !matches!(stem, "con" | "prn" | "aux" | "nul" | "clock$")
        && !(stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.as_bytes()[3].is_ascii_digit())
}
fn public_source_url(value: &str) -> bool {
    let Some(tail) = value.strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = tail.split_once('/') else {
        return false;
    };
    value.len() <= 1024
        && matches!(
            host,
            "github.com" | "codeload.github.com" | "developer.download.nvidia.com"
        )
        && !path.is_empty()
        && !path.contains("..")
        && value
            .bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b'\\' | b'@' | b'?' | b'#'))
        && !path.contains(':')
}
fn component_for(name: &str) -> Option<&'static str> {
    match name {
        "onnxruntime_providers_shared.dll" | "onnxruntime_providers_cuda.dll" => {
            Some("onnxruntime")
        }
        "cudnn64_9.dll"
        | "cudnn_adv64_9.dll"
        | "cudnn_cnn64_9.dll"
        | "cudnn_engines_precompiled64_9.dll"
        | "cudnn_engines_runtime_compiled64_9.dll"
        | "cudnn_graph64_9.dll"
        | "cudnn_heuristic64_9.dll"
        | "cudnn_ops64_9.dll" => Some("cudnn"),
        "cublas64_12.dll"
        | "cublaslt64_12.dll"
        | "cudart64_12.dll"
        | "cufft64_11.dll"
        | "curand64_10.dll"
        | "cusolver64_11.dll"
        | "cusparse64_12.dll"
        | "nvjitlink_120_0.dll"
        | "nvrtc64_120_0.dll"
        | "nvrtc-builtins64_128.dll"
        | "nvfatbin_120_0.dll" => Some("cuda"),
        _ => None,
    }
}
fn validate_description(
    value: &PublicRuntimeProvenance,
    build: Sha256Digest,
    files: &[manifest::RuntimeFile],
) -> Result<()> {
    ensure!(
        value.schema == "airk.native.cuda-runtime-provenance/1"
            && value.target == "x86_64-pc-windows-msvc"
            && value.ort_version == "1.30.0"
            && value.static_core_build == build
            && nonzero(build),
        "unknown/mismatched public CUDA provenance"
    );
    ensure!(
        [
            value.producer_script_sha256,
            value.producer_lock_sha256,
            value.verifier_sha256,
            value.build_proof_sha256,
            value.configure_argv_sha256,
            value.legal_inventory_sha256,
            value.legal_owner_sha256,
            value.aggregate.proof_sha256,
            value.aggregate.receipt_sha256,
            value.aggregate.plan_sha256
        ]
        .into_iter()
        .all(nonzero)
            && value.historical_failed_parent.is_none_or(nonzero),
        "missing public build/source identity"
    );
    if let Some(history) = &value.incremental_history {
        ensure!(
            history.mode == "incremental-finalgraph"
                && !history.qualification
                && !history.cold_build_qualified
                && history.parent_producer_sha256.to_string()
                    == "f9fac8eb2fe1a1cb5191118124503782792ca6d3aff14d02326167cd7276b178"
                && history.parent_aggregate_sha256.to_string()
                    == "042c970193bf3d2fc7c80f53b055f74130c653d561f7735142fc01b01cebac00"
                && value.static_core_build != history.parent_producer_sha256
                && history.aggregate_sha256 == value.aggregate.file.sha256
                && history.aggregate_sha256 != history.parent_aggregate_sha256
                && history.source_before_sha256 != history.source_after_sha256
                && value.checkpoint_history.is_some()
                && value
                    .components
                    .iter()
                    .any(|component| component.name == "onnxruntime"
                        && component.source_changes.contains(&history.patch_sha256)
                        && component
                            .source_changes
                            .contains(&history.source_after_sha256))
                && [
                    history.parent_build_proof_sha256,
                    history.recipe_sha256,
                    history.source_before_sha256,
                    history.source_after_sha256,
                    history.patch_sha256,
                    history.new_object_sha256,
                    history.new_session_sha256,
                ]
                .into_iter()
                .all(nonzero),
            "invalid public incremental history"
        );
    }
    if let Some(history) = &value.checkpoint_history {
        ensure!(
            history.mode == "checkpointed-warm"
                && !history.cold_build_qualified
                && !history.qualification
                && history.approved_plan_sha256.to_string()
                    == "6bee6b8b95a01fea28adf04117260d12503012ea8120ae225afa232b6589df97"
                && !history.failed_ancestry.is_empty()
                && history.failed_ancestry.len() <= 32
                && history.failed_ancestry.iter().copied().all(nonzero)
                && history
                    .failed_ancestry
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>()
                    .len()
                    == history.failed_ancestry.len()
                && history.failed_ancestry.iter().any(|x| x.to_string()
                    == "3a4c18932dbafbcc719cb77b30f616372b220cc315a1acdba82df24a9734c026")
                && value.historical_failed_parent.is_some_and(|x| x.to_string()
                    == "3a4c18932dbafbcc719cb77b30f616372b220cc315a1acdba82df24a9734c026")
                && history.sections.len() == 3,
            "invalid public checkpoint history"
        );
        for (index, row) in history.sections.iter().enumerate() {
            ensure!(
                row.section_index == index as u64
                    && row.section_id
                        == [
                            "flash-remainder",
                            "llm-remainder",
                            "final-provider-all-closure"
                        ][index]
                    && nonzero(row.receipt_sha256),
                "public checkpoint section identity differs"
            );
        }
    }
    ensure!(
        value.aggregate.file.name == "onnxruntime.lib"
            && value.aggregate.file.bytes > 65_536
            && value.aggregate.file.bytes <= 2u64 << 30
            && nonzero(value.aggregate.file.sha256),
        "invalid static aggregate identity"
    );
    ensure!(
        (3..=64).contains(&value.components.len())
            && (3..=16).contains(&value.tools.len())
            && (1..=128).contains(&value.options.len())
            && (2..=64).contains(&value.files.len())
            && (3..=512).contains(&value.legal.len()),
        "public provenance capacity"
    );
    let mut components = BTreeSet::new();
    for row in &value.components {
        ensure!(
            token(&row.name, 64)
                && row.name == row.name.to_ascii_lowercase()
                && row.version.as_ref().is_none_or(|v| token(v, 128))
                && row.upstream_sha1.as_ref().is_none_or(|v| v.len() == 40
                    && v.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
                && row.source_revision.as_ref().is_none_or(|v| token(v, 128))
                && row
                    .original_url
                    .as_ref()
                    .is_none_or(|v| public_source_url(v))
                && nonzero(row.source_sha256)
                && row.inventory_sha256.is_none_or(nonzero)
                && row.source_changes.len() <= 16
                && row.source_changes.iter().copied().all(nonzero)
                && components.insert(row.name.clone()),
            "invalid/duplicate source component"
        );
    }
    for (name, version) in [
        ("onnxruntime", "1.30.0"),
        ("cuda", "12.8.1"),
        ("cudnn", "9.10.2.21"),
    ] {
        ensure!(
            value.components.iter().any(|v| v.name == name
                && v.version.as_deref() == Some(version)
                && if name == "cuda" {
                    matches!(v.source_kind, SourceKind::InstalledToolkitInventory)
                        && v.inventory_sha256 == Some(v.source_sha256)
                } else {
                    matches!(v.source_kind, SourceKind::Archive)
                }),
            "required source version missing"
        );
    }
    let mut tools = BTreeSet::new();
    for row in &value.tools {
        ensure!(
            token(&row.name, 32)
                && matches!(
                    row.name.as_str(),
                    "compiler"
                        | "cmake"
                        | "ninja"
                        | "python"
                        | "patch"
                        | "vcvars"
                        | "archiver"
                        | "nvcc"
                )
                && row.version.as_ref().is_none_or(|v| token(v, 128))
                && row.version.is_some() == row.version_kind.is_some()
                && row
                    .version_kind
                    .as_deref()
                    .is_none_or(|v| matches!(v, "msvc-toolset" | "reported-tool-version"))
                && row.bytes > 0
                && row.bytes <= 1u64 << 30
                && nonzero(row.sha256)
                && tools.insert(row.name.clone()),
            "invalid/duplicate tool identity"
        );
    }
    ensure!(
        ["compiler", "cmake", "ninja"]
            .iter()
            .all(|n| tools.contains(*n)),
        "compiler tool closure missing"
    );
    let mut options = BTreeSet::new();
    for row in &value.options {
        ensure!(
            safe_build_option(&row.name, &row.value) && options.insert(row.name.as_str()),
            "unsafe/duplicate semantic build option"
        );
    }
    for (name, expected) in [
        ("onnxruntime_USE_CUDA", "ON"),
        ("onnxruntime_BUILD_SHARED_LIB", "OFF"),
        ("onnxruntime_BUILD_CUDA_EP_AS_PLUGIN", "OFF"),
    ] {
        ensure!(
            value
                .options
                .iter()
                .any(|v| v.name == name && v.value == expected),
            "same static-core build option missing"
        );
    }
    value.architectures()?;
    let mut payload = BTreeMap::new();
    let mut total = 0u64;
    for row in &value.files {
        ensure!(
            basename(&row.name)
                && component_for(&row.name).is_some()
                && row.bytes > 0
                && row.bytes <= 2u64 << 30
                && nonzero(row.sha256)
                && payload
                    .insert(row.name.clone(), (row.bytes, row.sha256))
                    .is_none(),
            "unapproved/duplicate runtime payload"
        );
        total = total.checked_add(row.bytes).context("payload overflow")?;
        ensure!(total <= 8u64 << 30, "payload capacity");
    }
    ensure!(
        [
            "onnxruntime_providers_shared.dll",
            "onnxruntime_providers_cuda.dll"
        ]
        .iter()
        .all(|n| payload.contains_key(*n)),
        "provider pair missing"
    );
    let observed: BTreeMap<_, _> = files
        .iter()
        .map(|r| (r.name.clone(), (r.bytes, r.sha256)))
        .collect();
    ensure!(
        observed.len() == files.len() && observed == payload,
        "public/native DLL closure mismatch"
    );
    let mut legal = BTreeSet::new();
    let mut covered = BTreeSet::new();
    let mut notice_bytes = 0u64;
    for row in &value.legal {
        let prefix = format!("airk-cuda-notice-{}-", row.component);
        let leaf = row
            .file
            .name
            .strip_prefix(&prefix)
            .context("legal package root differs")?;
        ensure!(
            components.contains(&row.component)
                && value
                    .components
                    .iter()
                    .any(|v| v.name == row.component
                        && v.source_sha256 == row.original_source_sha256)
                && basename(&row.file.name)
                && basename(leaf)
                && [".txt", ".html", ".md"]
                    .iter()
                    .any(|ext| row.file.name.ends_with(ext))
                && row.file.bytes > 0
                && row.file.bytes <= 2 << 20
                && nonzero(row.file.sha256)
                && nonzero(row.original_source_sha256)
                && legal.insert(row.file.name.clone()),
            "invalid/duplicate original legal notice"
        );
        notice_bytes = notice_bytes
            .checked_add(row.file.bytes)
            .context("notice overflow")?;
        ensure!(notice_bytes <= 16 << 20, "notice closure capacity");
        covered.insert(row.component.clone());
    }
    ensure!(
        covered == components,
        "complete original component notice coverage required"
    );
    Ok(())
}
/// The caller retains protected metadata and actual PE/held-file custody. This
/// decoder describes the build; it cannot construct runtime/device eligibility.
pub(super) fn validate(
    raw: &[u8],
    asset: &Asset,
    expected_static_build: Sha256Digest,
    files: &[manifest::RuntimeFile],
) -> Result<PublicRuntimeProvenance> {
    ensure!(
        asset.name == NAME
            && asset.role == AssetRole::Notice
            && raw.len() <= MAX_JSON
            && raw.len() as u64 == asset.bytes
            && nonzero(asset.sha256)
            && Sha256Digest::from_bytes(Sha256::digest(raw).into()) == asset.sha256,
        "public CUDA provenance bytes/asset mismatch"
    );
    let value: PublicRuntimeProvenance = serde_json::from_slice(raw)?;
    validate_description(&value, expected_static_build, files)?;
    Ok(value)
}

/// Load and packaging use the same held provenance/requirement architecture binding.
pub(super) fn validate_product(
    raw: &[u8],
    product: &manifest::ProductRuntimeManifest,
) -> Result<PublicRuntimeProvenance> {
    let value = validate(
        raw,
        &product.provenance,
        product.static_core_build,
        &product.files,
    )?;
    product.validate_architectures(value.architectures()?)?;
    Ok(value)
}

impl PublicRuntimeProvenance {
    fn architectures(&self) -> Result<&'static [[u32; 2]]> {
        self.options
            .iter()
            .find(|row| row.name == CUDA_ARCHITECTURES_OPTION)
            .and_then(|row| configured_architectures(&row.value))
            .context("authenticated CUDA build architectures missing")
    }

    /// Exact direct-child assets for the loader's original held-file custody.
    pub(super) fn legal_assets(&self) -> Vec<Asset> {
        self.legal
            .iter()
            .map(|v| Asset {
                name: v.file.name.clone(),
                role: AssetRole::Notice,
                bytes: v.file.bytes,
                sha256: v.file.sha256,
            })
            .collect()
    }
}
