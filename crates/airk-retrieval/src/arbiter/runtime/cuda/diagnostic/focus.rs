// SPDX-License-Identifier: Apache-2.0
//! Test-support FP32 CUDA probe metadata for the static-1024 Focus graph.
//!
//! Plan 12.5.2 (L5 phase A): the existing FP32 preparer accepts only the
//! static-512 Focus. This emits the same data-only, unqualified probe shape
//! for the static-1024 CPU profile (graph a16fcaa1, weights 1c819b77), or for
//! the widened full-FP32 graph 794fc0b2 (phase B), under its own manifest
//! domain. This file is not itself a digest input, but the diagnostic module's
//! own source (diagnostic.rs) is pinned by a `cpu_source_transition` test, and
//! phase B moved the FP16 math digest inputs; see cuda/CONTRACT.md "L5
//! digest-input movements". It never mints evaluation, device ownership,
//! execution, fitted calibration or release authority.
use super::super::{
    DeviceCapacityPlan, PreparedFp32ProbeProfile, compatibility_policy_digest, math_policy_digest,
    probe_runtime_identity, provider_options_digest,
};
use airk_core::{
    arbiter::{AssetRole, ModelChoice, Profile, Task, device::*},
    identity::Sha256Digest,
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

/// The independent static-1024 Focus export (weight8 projections, FP32 activations).
const FOCUS1024_GRAPH: &str = "a16fcaa1bf2d67b019b30cda56d76ba23d589c346ef2ae76d75c229ebe866810";
const FOCUS1024_WEIGHTS: &str = "1c819b7781fbf9b811f6ec1453e91d7f32a1bd6e5489168e0612d4fcf015b143";
const FOCUS1024_PROFILE: &str = "private-cuda-fp32-focus1024-probe-v1";
/// FP32 19b20ad4 widened to static 1024 (focus_widen.py): the full-FP32 precision arm.
const FULL_FP32_GRAPH: &str = "794fc0b2027969150c9368432c066fc7158d16600aafd94d0cbdeeb994f4da19";
const FULL_FP32_WEIGHTS: &str = "3a90bdb2e5d970275d5fe517786219a7123d020ba40bda39f1e8c46e249ec098";
const FULL_FP32_PROFILE: &str = "private-cuda-fp32-focus1024-fullfp32-probe-v1";
const FOCUS1024_DOMAIN: &[u8] = b"airk.private.cuda-fp32-focus1024-probe-profile/1";

impl super::super::CudaRuntime {
    /// Data-only, unqualified FP32 CUDA probe metadata for the static-1024 Focus CPU profile.
    /// The replacement swaps in the widened full-FP32 graph and sidecar (the 12.7.3 FP32 arm);
    /// without it the profile keeps the CPU product's weight8 graph.
    pub fn prepare_focus1024_fp32_probe_profile(
        cpu: &Profile,
        source: &dyn super::super::super::AssetSource,
        replacement: Option<Vec<airk_core::arbiter::Asset>>,
        runtime_manifest: &[u8],
        runtime_manifest_sha256: Sha256Digest,
        plan: DeviceCapacityPlan,
    ) -> Result<PreparedFp32ProbeProfile> {
        prepare(
            cpu,
            source,
            replacement,
            runtime_manifest,
            runtime_manifest_sha256,
            plan,
        )
    }
}

fn prepare(
    cpu: &Profile,
    source: &dyn super::super::super::AssetSource,
    replacement: Option<Vec<airk_core::arbiter::Asset>>,
    runtime_manifest: &[u8],
    runtime_manifest_sha256: Sha256Digest,
    plan: DeviceCapacityPlan,
) -> Result<PreparedFp32ProbeProfile> {
    let runtime_build = probe_runtime_identity(runtime_manifest, runtime_manifest_sha256)?;
    cpu.validate()?;
    super::super::super::validate_selected_policies(cpu)?;
    ensure!(
        cpu.model == ModelChoice::FocusQwenReranker06B
            && cpu.device_execution.is_none()
            && cpu.max_tokens == 1024
            && cpu.max_candidates == 4,
        "Focus1024 probe requires the current static-1024 Focus CPU description"
    );
    let mut source_profile = cpu.clone();
    let full = replacement.is_some();
    if let Some(assets) = replacement {
        source_profile
            .assets
            .retain(|a| !matches!(a.role, AssetRole::Model | AssetRole::ExternalWeights));
        source_profile.assets.extend(assets);
    }
    let cpu = &source_profile;
    let models = role(cpu, AssetRole::Model);
    let weights = role(cpu, AssetRole::ExternalWeights);
    let (graph, weight, id) = if full {
        (FULL_FP32_GRAPH, FULL_FP32_WEIGHTS, FULL_FP32_PROFILE)
    } else {
        (FOCUS1024_GRAPH, FOCUS1024_WEIGHTS, FOCUS1024_PROFILE)
    };
    ensure!(
        models.len() == 1
            && models[0].sha256 == graph.parse()?
            && weights.len() == 1
            && weights[0].sha256 == weight.parse()?,
        "Focus1024 probe requires one of the two reviewed static-1024 graphs and its weights"
    );
    // Authenticate the replacement bytes before they enter a profile.
    super::super::super::bytes(source, models[0], 64 << 20)?;
    super::super::super::bytes(source, weights[0], 4u64 << 30)?;
    let calibrations = role(cpu, AssetRole::Calibration);
    ensure!(calibrations.len() == 1, "native raw calibration missing");
    // Authenticate the current bytes; fresh raw metadata replaces them.
    super::super::super::bytes(source, calibrations[0], 64 << 10)?;
    let mut calibration_bytes = super::super::super::qualification::focus_raw_metadata(
        cpu,
        *cpu.task_policies
            .get(&Task::FocusRank)
            .context("Focus task policy missing")?,
    )?;
    calibration_bytes.push(b'\n');
    let calibration_sha = Sha256Digest::from_bytes(Sha256::digest(&calibration_bytes).into());
    let calibration_name = calibrations[0].name.clone();
    let mut profile = cpu.clone();
    let host_fit = profile.cpu_fit.take().context("native host fit required")?;
    ensure!(
        host_fit.intra_threads == 4 && host_fit.minimum_physical_cores == 4,
        "original selected host CPU tier changed"
    );
    profile.profile_id = id.into();
    profile.runtime = GpuBackend::Cuda.runtime().into();
    profile.downloads.clear();
    profile.qualified_targets.clear();
    profile.qualification = None;
    let asset = profile
        .assets
        .iter_mut()
        .find(|a| a.role == AssetRole::Calibration)
        .context("native calibration asset missing")?;
    asset.bytes = calibration_bytes.len() as u64;
    asset.sha256 = calibration_sha;
    profile.calibration = calibration_sha;
    profile.device_execution = Some(DeviceExecution {
        schema: DEVICE_EXECUTION_SCHEMA.into(),
        backend: GpuBackend::Cuda,
        runtime_build,
        compatibility_policy: compatibility_policy_digest(),
        provider_options: provider_options_digest(),
        math_policy: math_policy_digest(),
        precision: DevicePrecision::Float32,
        tf32: false,
        placement: PlacementPolicy::StrictGpu,
        graph: models[0].sha256,
        host_fit,
        memory: DeviceMemory::Dedicated {
            required_peak_bytes: plan.cold_peak_bytes,
            headroom_bytes: plan.headroom_bytes,
        },
    });
    profile.manifest = Sha256Digest::from_bytes([0; 32]);
    let canonical = serde_json::to_vec(&(
        cpu.manifest,
        runtime_manifest_sha256,
        &profile,
        plan.cold_peak_bytes,
        plan.warm_extra_bytes,
        plan.headroom_bytes,
    ))?;
    profile.manifest = airk_core::identity::framed_digest(FOCUS1024_DOMAIN, &[&canonical]);
    profile.validate()?;
    super::super::super::validate_selected_policies(&profile)?;
    Ok(PreparedFp32ProbeProfile {
        profile,
        calibration_name,
        calibration_bytes,
    })
}

fn role(profile: &Profile, role: AssetRole) -> Vec<&airk_core::arbiter::Asset> {
    profile.assets.iter().filter(|a| a.role == role).collect()
}
