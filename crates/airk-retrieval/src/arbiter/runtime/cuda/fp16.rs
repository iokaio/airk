// SPDX-License-Identifier: Apache-2.0
//! Separate G5 math/attention identity. Metadata is never execution authority.
use super::*;
use airk_core::arbiter::ModelChoice;
use ort::execution_providers::cuda::CUDAAttentionBackend;
use std::sync::OnceLock;
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct MathEnvironment {
    gemm: Option<String>,
    debug: Option<String>,
    unicode: bool,
}
static INITIAL: OnceLock<MathEnvironment> = OnceLock::new();
fn environment() -> MathEnvironment {
    let gemm = std::env::var_os("ORT_CUDA_GEMM_OPTIONS");
    let debug = std::env::var_os("ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO");
    MathEnvironment {
        unicode: gemm.as_ref().is_none_or(|x| x.to_str().is_some())
            && debug.as_ref().is_none_or(|x| x.to_str().is_some()),
        gemm: gemm.and_then(|x| x.into_string().ok()),
        debug: debug.and_then(|x| x.into_string().ok()),
    }
}
/// Called once inside the central ORT initialization BEFORE any OrtEnv. It
/// observes inherited settings without rejecting/changing the CPU/FP32 path.
pub(crate) fn capture_at_runtime_init() {
    INITIAL.get_or_init(environment);
}
fn check_environment(
    initial: &MathEnvironment,
    current: &MathEnvironment,
    proof: bool,
) -> Result<()> {
    ensure!(
        initial.unicode
            && initial == current
            && initial.gemm.as_deref() == Some("2")
            && (!proof || initial.debug.as_deref() == Some("1")),
        "G5 requires unchanged inherited GEMM2 and proof-only debug1 captured before ORT initialization"
    );
    Ok(())
}
pub(super) fn require_environment(proof: bool) -> Result<()> {
    let initial = INITIAL
        .get()
        .context("G5 math environment was not captured before ORT initialization")?;
    check_environment(initial, &environment(), proof)
}
fn family(model: ModelChoice) -> Result<(&'static str, &'static str)> {
    match model {
        ModelChoice::FocusQwenReranker06B => {
            Ok(("focus-qwen3-reranker-06b", "EFFICIENT_ATTENTION"))
        }
        ModelChoice::ContextQwenCompact => Ok(("context-qwen3-17b", "FLASH_ATTENTION")),
        _ => anyhow::bail!("G5 supports only selected Qwen model slots"),
    }
}
pub fn fp16_provider_options_digest(model: ModelChoice) -> Result<Sha256Digest> {
    let (model, backend) = family(model)?;
    Ok(airk_core::identity::framed_digest(b"airk.native.cuda.provider-options/2",&[model.as_bytes(),backend.as_bytes(),b"cuda-only;error-on-failure;disable-cpu-fallback=1;tf32=0;cuda-graph=0;arena=declared-cold-peak;cpu-f32-scores;original-deadline;ORT_CUDA_GEMM_OPTIONS=2;attention-debug1=qualification-proof-only;synchronize=ORT-default+primary-context"]))
}
pub fn fp16_math_policy_digest(model: ModelChoice) -> Result<Sha256Digest> {
    let (model, backend) = family(model)?;
    Ok(airk_core::identity::framed_digest(b"airk.native.cuda.math-policy/2",&[model.as_bytes(),backend.as_bytes(),include_bytes!("fp16.rs"),include_bytes!("../../external_data/g5.rs"),include_bytes!("../cuda.rs"),include_bytes!("../../runtime.rs"),include_bytes!("../../external_data.rs"),include_bytes!("../../../onnx.rs"),b"g5-full-fp16-learned-weights-except-rms-scales-and-focus-readout;ieee754-rne-no-clipping;f32-residual-rmsnorm-qk-norm-rope-silu-gate-sparse-readout;epsilon=1e-6;theta=1000000;head128;original-scale-bits=3db504f3;f32-output-abi;tf32=0;half-gemm-compute32f-no-reduced-precision-reduction;unchanged-numeric-limits;actual-backend-and-completion-proof-separate"]))
}
pub(super) fn backend(profile: &Profile) -> Result<CUDAAttentionBackend> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    if is_protected_efficient_mha(profile)? {
        return Ok(CUDAAttentionBackend::EFFICIENT_ATTENTION);
    }
    Ok(if family(profile.model)?.1 == "EFFICIENT_ATTENTION" {
        CUDAAttentionBackend::EFFICIENT_ATTENTION
    } else {
        CUDAAttentionBackend::FLASH_ATTENTION
    })
}
#[cfg(all(windows, target_arch = "x86_64"))]
fn is_protected_efficient_mha(profile: &Profile) -> Result<bool> {
    let Some(device) = &profile.device_execution else {
        return Ok(false);
    };
    Ok(
        matches!(device.placement, PlacementPolicy::BoundedHybrid { .. })
            && BoundedContextOptimizer::from_profile(profile)?
                == BoundedContextOptimizer::ProtectedEfficientMha,
    )
}

/// True when the profile's graph and single weight asset are the pinned protected
/// Context artifacts for its model (evaluation admission, plan 12.5.2).
#[cfg(all(windows, target_arch = "x86_64", feature = "test-support"))]
pub(super) fn is_protected_graph(profile: &Profile) -> Result<bool> {
    let Ok(pin) = ProtectedContextMha::for_model(profile.model) else {
        return Ok(false);
    };
    let Some(device) = &profile.device_execution else {
        return Ok(false);
    };
    Ok(device.graph == pin.graph.parse()? && pin.require_weights(&profile.assets).is_ok())
}

#[cfg(all(windows, target_arch = "x86_64"))]
struct ProtectedContextMha {
    prefix: &'static str,
    layers: usize,
    graph: &'static str,
    graph_bytes: usize,
    weights: &'static str,
    weight_bytes: u64,
}

#[cfg(all(windows, target_arch = "x86_64"))]
impl ProtectedContextMha {
    fn for_model(model: ModelChoice) -> Result<Self> {
        match model {
            ModelChoice::ContextQwenCompact => Ok(Self {
                prefix: "airk_compact_cutlass",
                layers: 28,
                graph: "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9",
                graph_bytes: 854_973,
                weights: "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3",
                weight_bytes: 3_441_397_760,
            }),
            _ => anyhow::bail!("protected efficient MHA requires a Context model"),
        }
    }

    fn require_weights(&self, assets: &[airk_core::arbiter::Asset]) -> Result<()> {
        let mut weights = assets
            .iter()
            .filter(|asset| asset.role == airk_core::arbiter::AssetRole::ExternalWeights);
        let weight = weights
            .next()
            .context("protected efficient MHA weight asset missing")?;
        ensure!(
            weights.next().is_none()
                && weight.name == "weights.bin"
                && weight.bytes == self.weight_bytes
                && weight.sha256 == self.weights.parse()?,
            "protected efficient MHA weight artifact differs"
        );
        Ok(())
    }

    fn attention_nodes(&self) -> Vec<crate::arbiter::external_data::g5::AttentionNode> {
        (0..self.layers)
            .map(|layer| crate::arbiter::external_data::g5::AttentionNode {
                name: format!("/{}/{layer}/efficient", self.prefix),
                operator: "MultiHeadAttention".into(),
                domain: "com.microsoft".into(),
            })
            .collect()
    }

    fn require_projections(&self, placement: &PlacementObservation) -> Result<()> {
        let nodes: BTreeMap<_, _> = placement
            .nodes
            .iter()
            .map(|node| (node.name.as_str(), node))
            .collect();
        ensure!(
            placement.complete && nodes.len() == placement.nodes.len(),
            "protected efficient MHA native placement is incomplete or ambiguous"
        );
        for layer in 0..self.layers {
            for site in [
                "self_attn/q_proj",
                "self_attn/k_proj",
                "self_attn/v_proj",
                "self_attn/o_proj",
                "mlp/gate_proj",
                "mlp/up_proj",
                "mlp/down_proj",
            ] {
                let name = format!("/scorer/layers.{layer}/{site}/MatMul");
                let node = nodes
                    .get(name.as_str())
                    .context("protected learned projection missing")?;
                ensure!(
                    node.subgraph_index == 0
                        && node.domain.is_empty()
                        && node.op == "MatMul"
                        && node.ep == "CUDAExecutionProvider",
                    "protected learned projection differs: {name}"
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub(in crate::arbiter::runtime) struct AttentionPlan {
    schema: &'static str,
    graph: Sha256Digest,
    model: ModelChoice,
    operator: &'static str,
    domain: &'static str,
    dtype: &'static str,
    backend: &'static str,
    math_policy: Sha256Digest,
    provider_options: Sha256Digest,
    nodes: Vec<crate::arbiter::external_data::g5::AttentionNode>,
    /// The static Focus width this plan admits (512 or 1024); unused for Context.
    #[serde(skip)]
    focus_tokens: usize,
}
impl AttentionPlan {
    pub(in crate::arbiter::runtime) fn from_graph(
        profile: &Profile,
        graph: &[u8],
        files: &BTreeMap<String, u64>,
    ) -> Result<Self> {
        let p = profile
            .device_execution
            .as_ref()
            .context("G5 CPU descriptor")?;
        #[cfg(all(windows, target_arch = "x86_64"))]
        if is_protected_efficient_mha(profile)? {
            let recipe = BoundedContextOptimizer::ProtectedEfficientMha;
            let candidate = ProtectedContextMha::for_model(profile.model)?;
            ensure!(
                p.math_policy == recipe.math_policy(profile.model)?,
                "protected efficient MHA math identity differs"
            );
            candidate.require_weights(&profile.assets)?;
            let actual = Sha256Digest::from_bytes(Sha256::digest(graph).into());
            // The independently reviewed graph bytes pin every FP32 Q/K/V edge and causal MHA attribute.
            ensure!(
                graph.len() == candidate.graph_bytes
                    && actual == candidate.graph.parse()?
                    && actual == p.graph,
                "protected efficient MHA graph artifact differs"
            );
            return Ok(Self {
                schema: "airk.native.cuda-fp16-attention-plan/1",
                graph: actual,
                model: profile.model,
                operator: "MultiHeadAttention",
                domain: "com.microsoft",
                dtype: "fp32",
                backend: "EFFICIENT_ATTENTION",
                math_policy: p.math_policy,
                provider_options: p.provider_options,
                nodes: candidate.attention_nodes(),
                focus_tokens: 0,
            });
        }
        ensure!(
            p.precision == DevicePrecision::Float16
                && !p.tf32
                && p.math_policy == fp16_math_policy_digest(profile.model)?
                && p.provider_options == fp16_provider_options_digest(profile.model)?,
            "G5 profile math/provider identity differs"
        );
        let actual = Sha256Digest::from_bytes(Sha256::digest(graph).into());
        ensure!(
            actual == p.graph,
            "G5 actual graph differs from immutable descriptor"
        );
        let nodes = crate::arbiter::external_data::g5::audit(graph, files, profile.model)?;
        Ok(Self {
            schema: "airk.native.cuda-fp16-attention-plan/1",
            graph: actual,
            model: profile.model,
            operator: if profile.model == ModelChoice::FocusQwenReranker06B {
                "MultiHeadAttention"
            } else {
                "GroupQueryAttention"
            },
            domain: "com.microsoft",
            dtype: "fp16",
            backend: family(profile.model)?.1,
            math_policy: p.math_policy,
            provider_options: p.provider_options,
            nodes,
            focus_tokens: profile.max_tokens,
        })
    }
    pub(in crate::arbiter::runtime) fn require_math_environment(&self) -> Result<()> {
        require_environment(false)
    }
    pub(in crate::arbiter::runtime) fn require_placement(
        &self,
        placement: &PlacementObservation,
    ) -> Result<()> {
        placement.clone().require_strict()?;
        self.require_attention_placement(placement)
    }

    pub(in crate::arbiter::runtime) fn require_attention_placement(
        &self,
        placement: &PlacementObservation,
    ) -> Result<()> {
        let actual = placement
            .nodes
            .iter()
            .filter(|n| {
                matches!(
                    n.op.as_str(),
                    "MultiHeadAttention"
                        | "GroupQueryAttention"
                        | "Attention"
                        | "PackedAttention"
                        | "DecoderMaskedMultiHeadAttention"
                )
            })
            .collect::<Vec<_>>();
        ensure!(
            actual.len() == self.nodes.len()
                && self.nodes.iter().all(|expected| actual
                    .iter()
                    .filter(|n| n.subgraph_index == 0
                        && n.name == expected.name
                        && n.op == expected.operator
                        && n.domain == expected.domain
                        && n.ep == "CUDAExecutionProvider")
                    .count()
                    == 1),
            "G5 final SessionState attention roster differs"
        );
        #[cfg(all(windows, target_arch = "x86_64"))]
        if self.dtype == "fp32" {
            ProtectedContextMha::for_model(self.model)?.require_projections(placement)?;
        }
        Ok(())
    }
    pub(in crate::arbiter::runtime) fn validate_input(
        &self,
        input: &super::super::CompiledInput,
    ) -> Result<()> {
        require_environment(false)?;
        validate_geometry(self.model, self.focus_tokens, input)
    }
    pub(in crate::arbiter::runtime) fn observation(&self) -> Result<serde_json::Value> {
        require_environment(true)?;
        let mut result = serde_json::to_value(self)?;
        result["math_environment"] =
            serde_json::json!({"captured_before_ort":INITIAL.get(),"current":environment()});
        result["qualification"] = false.into();
        result["actual_backend_observed"] = false.into();
        result["forward_completion_claimed"] = false.into();
        Ok(result)
    }
}
fn validate_geometry(
    model: ModelChoice,
    focus_tokens: usize,
    input: &super::super::CompiledInput,
) -> Result<()> {
    let tokens = input.input_ids.len();
    ensure!(
        input.attention_mask.len() == tokens
            && input.position_ids.len() == tokens
            && input
                .position_ids
                .iter()
                .enumerate()
                .all(|(i, p)| *p == i as i64)
            && input.decision_position == tokens as i64 - 1,
        "G5 original positions/decision/input length differ"
    );
    if model == ModelChoice::FocusQwenReranker06B {
        ensure!(
            matches!(focus_tokens, 512 | 1024)
                && tokens == focus_tokens
                && input.attention_mask.last() == Some(&1)
                && input.attention_mask.iter().all(|m| matches!(m, 0 | 1))
                && input.attention_mask.windows(2).all(|p| p[0] <= p[1]),
            "G5 Focus requires its static width with a nonempty live suffix"
        );
    } else {
        ensure!(
            (1..=8192).contains(&tokens)
                && input.attention_mask.iter().all(|m| *m == 1)
                && !input.candidate_positions.is_empty()
                && input.candidate_positions.len() <= 8,
            "G5 Context requires original all-live8192/N8 geometry"
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests;

#[cfg(feature = "test-support")]
impl super::super::Scorer {
    /// Checks the exact G5 scorer's input gate without dispatching native work.
    pub fn validate_fp16_probe_input(&self, input: &super::super::CompiledInput) -> Result<()> {
        self.fp16_attention
            .as_ref()
            .context("G5 scorer required")?
            .validate_input(input)
    }
}

#[cfg(feature = "test-support")]
fn context_probe_asset_bindings(
    assets: &[airk_core::arbiter::Asset],
) -> std::collections::BTreeMap<
    String,
    airk_core::arbiter::context_action::calibration::JointAssetBinding,
> {
    use airk_core::arbiter::{AssetRole, context_action::calibration::JointAssetBinding};
    assets
        .iter()
        .filter(|asset| {
            matches!(
                asset.role,
                AssetRole::Model | AssetRole::ExternalWeights | AssetRole::Tokenizer
            )
        })
        .map(|asset| {
            (
                asset.name.clone(),
                JointAssetBinding {
                    role: asset.role.clone(),
                    sha256: asset.sha256,
                },
            )
        })
        .collect()
}

/// Fresh data-only G5 profile. Never fitted CAL, release, device or execution authority.
#[cfg(feature = "test-support")]
#[derive(Serialize)]
pub struct PreparedFp16ProbeProfile {
    pub profile: Profile,
    pub calibration_name: String,
    pub calibration_bytes: Vec<u8>,
    pub original_graph: Sha256Digest,
    pub attention_plan: serde_json::Value,
}
#[cfg(feature = "test-support")]
pub fn prepare_fp16_probe_profile(
    cpu: &Profile,
    source: &dyn super::super::AssetSource,
    assets: Vec<airk_core::arbiter::Asset>,
    original_graph: Sha256Digest,
    runtime_manifest: &[u8],
    runtime_manifest_sha256: Sha256Digest,
    plan: DeviceCapacityPlan,
) -> Result<PreparedFp16ProbeProfile> {
    use airk_core::arbiter::{AssetRole, Task};
    let runtime_build = probe_runtime_identity(runtime_manifest, runtime_manifest_sha256)?;
    cpu.validate()?;
    super::super::validate_selected_policies(cpu)?;
    let (id, original, threads) = match cpu.model {
        ModelChoice::FocusQwenReranker06B if cpu.max_tokens == 1024 => (
            // FP32 19b20ad4 widened to static 1024 (scripts/arbiter-fp16-export/focus_widen.py).
            "private-cuda-fp16-focus1024-efficient-probe-v1",
            "794fc0b2027969150c9368432c066fc7158d16600aafd94d0cbdeeb994f4da19",
            4,
        ),
        ModelChoice::FocusQwenReranker06B => (
            "private-cuda-fp16-focus-efficient-probe-v1",
            "19b20ad4e39e173f6da602094b9c1583726a65fa76742553f97ec6f0f58436a1",
            4,
        ),
        ModelChoice::ContextQwenCompact => (
            "private-cuda-fp16-compact-efficient-probe-v1",
            "c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4",
            4,
        ),
        _ => anyhow::bail!("G5 unsupported model"),
    };
    ensure!(
        original_graph == original.parse()?
            && cpu.device_execution.is_none()
            && if cpu.model == ModelChoice::FocusQwenReranker06B {
                matches!(cpu.max_tokens, 512 | 1024) && cpu.max_candidates == 4
            } else {
                cpu.max_tokens == 8192 && cpu.max_candidates == 8
            },
        "G5 original FP32 lineage/current input contract differs"
    );
    let mut names = BTreeSet::new();
    ensure!(
        (2..=128).contains(&assets.len())
            && assets.iter().all(|a| names.insert(a.name.clone())
                && matches!(a.role, AssetRole::Model | AssetRole::ExternalWeights)),
        "G5 transformed model/weight roster invalid"
    );
    let mut preview = cpu.clone();
    preview
        .assets
        .retain(|a| !matches!(a.role, AssetRole::Model | AssetRole::ExternalWeights));
    preview.assets.extend(assets.iter().cloned());
    preview.downloads.clear();
    preview.qualified_targets.clear();
    preview.qualification = None;
    preview.validate()?;
    let models = assets
        .iter()
        .filter(|a| a.role == AssetRole::Model)
        .collect::<Vec<_>>();
    ensure!(models.len() == 1, "G5 singleton graph required");
    let graph = super::super::bytes(source, models[0], 64 << 20)?;
    let external = assets
        .iter()
        .filter(|a| a.role == AssetRole::ExternalWeights)
        .map(|a| (a.name.clone(), a.bytes))
        .collect();
    crate::arbiter::external_data::g5::audit(&graph, &external, cpu.model)?;

    for a in assets
        .iter()
        .filter(|a| a.role == AssetRole::ExternalWeights)
    {
        super::super::bytes(source, a, 16u64 << 30)?;
    }
    let calibrations = cpu
        .assets
        .iter()
        .filter(|a| a.role == AssetRole::Calibration)
        .collect::<Vec<_>>();
    ensure!(
        calibrations.len() == 1,
        "G5 current native calibration missing"
    );
    let raw = super::super::bytes(source, calibrations[0], 64 << 10)?;
    let mut calibration_bytes = if cpu.model == ModelChoice::FocusQwenReranker06B {
        super::super::qualification::focus_raw_metadata(cpu, cpu.task_policies[&Task::FocusRank])?
    } else {
        let mut c =
            airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
                &raw,
                cpu,
                crate::arbiter::context_action::policy_digest()?,
            )?;
        c.schema = airk_core::arbiter::context_action::calibration::CALIBRATION_SCHEMA_V2.into();
        c.rules.clear();
        c.class_support = Some(unavailable_classes());
        c.assets = context_probe_asset_bindings(&preview.assets);
        serde_json::to_vec_pretty(&c)?
    };
    calibration_bytes.push(b'\n');
    let mut profile = cpu.clone();
    let host_fit = profile
        .cpu_fit
        .take()
        .context("G5 native host fit missing")?;
    ensure!(
        host_fit.intra_threads == threads && host_fit.minimum_physical_cores == threads,
        "G5 unchanged selected host tier required"
    );
    profile.profile_id = id.into();
    profile.runtime = GpuBackend::Cuda.runtime().into();
    profile.downloads.clear();
    profile.qualified_targets.clear();
    profile.qualification = None;
    profile
        .assets
        .retain(|a| !matches!(a.role, AssetRole::Model | AssetRole::ExternalWeights));
    profile.assets.extend(assets);
    let calibration_name = calibrations[0].name.clone();
    let sha = Sha256Digest::from_bytes(Sha256::digest(&calibration_bytes).into());
    profile.calibration = sha;
    let a = profile
        .assets
        .iter_mut()
        .find(|a| a.role == AssetRole::Calibration)
        .context("G5 CAL asset missing")?;
    a.sha256 = sha;
    a.bytes = calibration_bytes.len() as u64;
    let graph_sha = Sha256Digest::from_bytes(Sha256::digest(&graph).into());
    profile.device_execution = Some(DeviceExecution {
        schema: DEVICE_EXECUTION_SCHEMA.into(),
        backend: GpuBackend::Cuda,
        runtime_build,
        compatibility_policy: compatibility_policy_digest(),
        provider_options: fp16_provider_options_digest(cpu.model)?,
        math_policy: fp16_math_policy_digest(cpu.model)?,
        precision: DevicePrecision::Float16,
        tf32: false,
        placement: PlacementPolicy::StrictGpu,
        graph: graph_sha,
        host_fit,
        memory: DeviceMemory::Dedicated {
            required_peak_bytes: plan.cold_peak_bytes,
            headroom_bytes: plan.headroom_bytes,
        },
    });
    let files = profile
        .assets
        .iter()
        .filter(|a| a.role == AssetRole::ExternalWeights)
        .map(|a| (a.name.clone(), a.bytes))
        .collect();
    let attention = AttentionPlan::from_graph(&profile, &graph, &files)?;
    profile.manifest = Sha256Digest::from_bytes([0; 32]);
    let canonical = serde_json::to_vec(&(
        cpu.manifest,
        original_graph,
        runtime_manifest_sha256,
        &profile,
        plan.cold_peak_bytes,
        plan.warm_extra_bytes,
        plan.headroom_bytes,
    ))?;
    profile.manifest = airk_core::identity::framed_digest(
        b"airk.private.cuda-fp16-efficient-probe-profile/1",
        &[&canonical],
    );
    profile.validate()?;
    super::super::validate_selected_policies(&profile)?;
    if cpu.model != ModelChoice::FocusQwenReranker06B {
        airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
            &calibration_bytes,
            &profile,
            crate::arbiter::context_action::policy_digest()?,
        )?;
    }
    Ok(PreparedFp16ProbeProfile {
        profile,
        calibration_name,
        calibration_bytes,
        original_graph,
        attention_plan: serde_json::to_value(attention)?,
    })
}

/// Measured capacity and host fit for one protected Context profile (design L5 1.4).
#[cfg(all(feature = "test-support", windows, target_arch = "x86_64"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedMeasurement {
    pub cold_peak_bytes: u64,
    pub warm_extra_bytes: u64,
    pub headroom_bytes: u64,
    pub memory_reservation_bytes: u64,
    pub intra_threads: usize,
    /// SHA-256 of the retained measurement receipt the values were read from.
    pub receipt_sha256: Option<Sha256Digest>,
}

/// Fresh, unqualified evaluation profile for a protected efficient-MHA Context graph.
#[cfg(all(feature = "test-support", windows, target_arch = "x86_64"))]
#[derive(Serialize)]
pub struct PreparedProtectedMhaProfile {
    pub profile: Profile,
    pub calibration_name: String,
    pub calibration_bytes: Vec<u8>,
    pub attention_plan: serde_json::Value,
    pub measurement: ProtectedMeasurement,
}

/// Sentinel bounded policy of a pass-1 profile. It names no reviewed placement;
/// only the graph-metadata diagnostic may open it, to observe the real policy.
#[cfg(all(feature = "test-support", windows, target_arch = "x86_64"))]
pub fn protected_mha_pass1_policy() -> Sha256Digest {
    airk_core::identity::framed_digest(b"airk.private.cuda-protected-mha-pass1-sentinel/1", &[])
}

/// Two-pass emitter (design L5 1.3 A). Pass 1 (`policy = None`) writes the
/// sentinel policy; the graph-metadata diagnostic then observes the live rows
/// and placement and computes `bounded_context_execution_policy`. Pass 2 writes
/// the same profile with that digest. The digest excludes the manifest, so the
/// pass-2 manifest change does not invalidate it.
#[cfg(all(feature = "test-support", windows, target_arch = "x86_64"))]
#[allow(clippy::too_many_arguments)]
pub fn prepare_protected_mha_profile(
    cpu: &Profile,
    source: &dyn super::super::AssetSource,
    assets: Vec<airk_core::arbiter::Asset>,
    runtime_manifest: &[u8],
    runtime_manifest_sha256: Sha256Digest,
    measurement: ProtectedMeasurement,
    measurement_receipt: Option<&[u8]>,
    policy: Option<Sha256Digest>,
) -> Result<PreparedProtectedMhaProfile> {
    use airk_core::arbiter::{AssetRole, Task};
    ensure!(
        policy.is_none() || measurement.receipt_sha256.is_some(),
        "a pass-2 protected profile requires its measurement receipt"
    );
    if let Some(expected) = measurement.receipt_sha256 {
        verify_measurement_receipt(
            &measurement,
            measurement_receipt.context("measurement receipt bytes missing")?,
            expected,
        )?;
    }
    let runtime_build = probe_runtime_identity(runtime_manifest, runtime_manifest_sha256)?;
    cpu.validate()?;
    super::super::validate_selected_policies(cpu)?;
    let (id, original) = match cpu.model {
        ModelChoice::ContextQwenCompact => (
            "private-cuda-fp16-context-compact-protected-mha-v1",
            "c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4",
        ),
        _ => anyhow::bail!("protected efficient MHA requires a Context model"),
    };
    let cpu_graphs = cpu
        .assets
        .iter()
        .filter(|a| a.role == AssetRole::Model)
        .collect::<Vec<_>>();
    ensure!(
        cpu.device_execution.is_none()
            && cpu.max_tokens == 8192
            && cpu.max_candidates == 8
            && cpu_graphs.len() == 1
            && cpu_graphs[0].sha256 == original.parse()?
            && cpu.tasks.contains(&Task::ContextAction),
        "protected MHA requires the original FP32 Context8192/N8 CPU description"
    );
    let candidate = ProtectedContextMha::for_model(cpu.model)?;
    let pinned_graph: Sha256Digest = candidate.graph.parse()?;
    ensure!(
        assets.len() == 2
            && assets.iter().any(|a| a.role == AssetRole::Model
                && a.name == "model.onnx"
                && a.bytes == candidate.graph_bytes as u64
                && a.sha256 == pinned_graph)
            && candidate.require_weights(&assets).is_ok(),
        "protected MHA assets must be exactly the pinned graph and weights"
    );
    ensure!(
        measurement.cold_peak_bytes > 0
            && measurement.headroom_bytes > 0
            && measurement.warm_extra_bytes <= measurement.cold_peak_bytes
            && (1..=64).contains(&measurement.intra_threads)
            && measurement.memory_reservation_bytes > 0,
        "protected MHA measured capacity invalid"
    );
    let mut preview = cpu.clone();
    preview
        .assets
        .retain(|a| !matches!(a.role, AssetRole::Model | AssetRole::ExternalWeights));
    preview.assets.extend(assets.iter().cloned());
    let model = assets.iter().find(|a| a.role == AssetRole::Model).context("graph")?;
    let graph = super::super::bytes(source, model, 64 << 20)?;
    for weight in assets.iter().filter(|a| a.role == AssetRole::ExternalWeights) {
        super::super::bytes(source, weight, 16u64 << 30)?;
    }
    let calibrations = cpu
        .assets
        .iter()
        .filter(|a| a.role == AssetRole::Calibration)
        .collect::<Vec<_>>();
    ensure!(calibrations.len() == 1, "native calibration missing");
    let raw = super::super::bytes(source, calibrations[0], 64 << 10)?;
    let mut c = airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
        &raw,
        cpu,
        crate::arbiter::context_action::policy_digest()?,
    )?;
    c.schema = airk_core::arbiter::context_action::calibration::CALIBRATION_SCHEMA_V2.into();
    c.rules.clear();
    c.class_support = Some(unavailable_classes());
    c.assets = context_probe_asset_bindings(&preview.assets);
    let mut calibration_bytes = serde_json::to_vec_pretty(&c)?;
    calibration_bytes.push(b'\n');
    let mut profile = preview;
    let mut host_fit = profile.cpu_fit.take().context("native host fit missing")?;
    host_fit.intra_threads = measurement.intra_threads;
    host_fit.minimum_physical_cores = measurement.intra_threads;
    host_fit.inter_threads = 1;
    host_fit.sequential = true;
    host_fit.spinning = false;
    profile.profile_id = id.into();
    profile.runtime = GpuBackend::Cuda.runtime().into();
    profile.downloads.clear();
    profile.qualified_targets.clear();
    profile.qualification = None;
    profile.memory_reservation_bytes = measurement.memory_reservation_bytes;
    let calibration_name = calibrations[0].name.clone();
    let sha = Sha256Digest::from_bytes(Sha256::digest(&calibration_bytes).into());
    profile.calibration = sha;
    let a = profile
        .assets
        .iter_mut()
        .find(|a| a.role == AssetRole::Calibration)
        .context("CAL asset missing")?;
    a.sha256 = sha;
    a.bytes = calibration_bytes.len() as u64;
    let recipe = BoundedContextOptimizer::ProtectedEfficientMha;
    profile.device_execution = Some(DeviceExecution {
        schema: DEVICE_EXECUTION_SCHEMA.into(),
        backend: GpuBackend::Cuda,
        runtime_build,
        compatibility_policy: compatibility_policy_digest(),
        provider_options: recipe.provider_options(cpu.model)?,
        math_policy: recipe.math_policy(cpu.model)?,
        precision: DevicePrecision::Float16,
        tf32: false,
        placement: PlacementPolicy::BoundedHybrid {
            policy: policy.unwrap_or_else(protected_mha_pass1_policy),
        },
        graph: model.sha256,
        host_fit,
        memory: DeviceMemory::Dedicated {
            required_peak_bytes: measurement.cold_peak_bytes,
            headroom_bytes: measurement.headroom_bytes,
        },
    });
    let files = profile
        .assets
        .iter()
        .filter(|a| a.role == AssetRole::ExternalWeights)
        .map(|a| (a.name.clone(), a.bytes))
        .collect();
    let attention = AttentionPlan::from_graph(&profile, &graph, &files)?;
    profile.manifest = Sha256Digest::from_bytes([0; 32]);
    let canonical = serde_json::to_vec(&(
        cpu.manifest,
        runtime_manifest_sha256,
        &profile,
        &measurement,
    ))?;
    profile.manifest = airk_core::identity::framed_digest(
        b"airk.private.cuda-protected-mha-profile/1",
        &[&canonical],
    );
    profile.validate()?;
    super::super::validate_selected_policies(&profile)?;
    airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
        &calibration_bytes,
        &profile,
        crate::arbiter::context_action::policy_digest()?,
    )?;
    ensure!(super::evaluation_bounded(&profile)?, "emitted profile is outside the evaluation admission");
    Ok(PreparedProtectedMhaProfile {
        profile,
        calibration_name,
        calibration_bytes,
        attention_plan: serde_json::to_value(attention)?,
        measurement,
    })
}

/// The measured values must come from, and stay within, their retained receipt
/// (an `airk.private.l5a-gpu/1` strict-run receipt of the same graph and weights).
#[cfg(all(feature = "test-support", windows, target_arch = "x86_64"))]
fn verify_measurement_receipt(
    measurement: &ProtectedMeasurement,
    raw: &[u8],
    expected: Sha256Digest,
) -> Result<()> {
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(raw).into()) == expected,
        "measurement receipt bytes differ from receipt_sha256"
    );
    let receipt: serde_json::Value = serde_json::from_slice(raw)?;
    let free = |value: &serde_json::Value| value["free_bytes"].as_u64();
    let before = free(&receipt["adapter_before"]).context("receipt pre-load adapter free bytes")?;
    let rows = receipt["rows"].as_array().context("receipt width rows")?;
    ensure!(
        receipt["schema"] == "airk.private.l5a-gpu/1"
            && receipt["complete"] == true
            && receipt["step"] == "strict"
            && !rows.is_empty(),
        "measurement receipt is not a complete strict scorer-path run"
    );
    let mut used = before.saturating_sub(
        free(&receipt["adapter_after_open"]).context("receipt post-load adapter")?,
    );
    for row in rows {
        used = used.max(before.saturating_sub(
            free(&row["adapter_after"]).context("receipt width adapter")?,
        ));
    }
    let committed = receipt["host_final"]["peak_pagefile_bytes"]
        .as_u64()
        .context("receipt host committed peak")?;
    ensure!(
        used <= measurement.cold_peak_bytes,
        "measured device use exceeds the declared cold peak"
    );
    ensure!(
        committed <= measurement.memory_reservation_bytes,
        "measured host committed peak exceeds the declared host reservation"
    );
    Ok(())
}