// SPDX-License-Identifier: Apache-2.0
//! Private layer-0 outputs for authenticated Compact FP16 experiments.

mod answer;

use super::*;

pub const COMPACT_FP16_LAYER0_TAPS: &[(&str, usize)] = &[
    ("/airk_g5/attention/0/q16", 2048),
    ("/airk_g5/attention/0/k16", 1024),
    (V_PROJECTION, 1024),
    ("/airk_g5/attention/0/raw16", 2048),
];
macro_rules! decision_taps {
    ($($n:literal),+) => {
        &[$(
            (concat!("/airk_g5/weight//scorer/layers.", $n, "/self_attn/k_proj/MatMul/input16"), 2048),
        )+]
    };
}

const GQA: &str = "/airk_g5/attention/0/efficient";
const V_PROJECTION: &str = "/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16";
const PROJECTION_INPUT: &str = "/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/input16";
pub const COMPACT_FP16_LAYER0_PROJECTION_TAPS: &[(&str, usize)] = &[
    (PROJECTION_INPUT, 2048),
    (
        "/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/output16",
        1024,
    ),
    (V_PROJECTION, 1024),
    (
        "/airk_g5/weight//scorer/layers.0/self_attn/q_proj/MatMul/output16",
        2048,
    ),
];
pub(super) const LARGE_ATTENTION17: &[(&str, usize)] = &[
    ("/airk_g5/attention/17/q16", 4096), ("/airk_g5/attention/17/k16", 1024),
    ("/airk_g5/weight//scorer/layers.17/self_attn/v_proj/MatMul/output16", 1024),
    ("/airk_g5/attention/17/raw16", 4096),
];
pub(super) const LARGE_DENSE17: &[(&str, usize)] = &[
    ("/airk_g5/attention/17/raw16", 4096),
    ("/airk_g5/weight//scorer/layers.17/self_attn/o_proj/MatMul/output16", 2560),
    ("/airk_g5/weight//scorer/layers.17/mlp/down_proj/MatMul/input16", 9728),
    ("/airk_g5/weight//scorer/layers.17/mlp/down_proj/MatMul/output16", 2560),
];

fn fixed_tap_batch(name: &str) -> bool {
    (name.starts_with("/airk_g5/weight/") && !LARGE_ATTENTION17.iter().chain(LARGE_DENSE17).any(|tap| tap.0 == name))
        || full_cut::fixed_batch(name)
}

pub(super) const LARGE_QKV_HIDDEN_TAPS: &[(&str, usize)] = &[
    ("/scorer/layers.0/input_layernorm/Mul_1_output_0", 2560),
    ("/scorer/layers.17/input_layernorm/Mul_1_output_0", 2560),
    ("/scorer/layers.35/Add_1_output_0", 2560),
    ("/scorer/norm/Mul_1_output_0", 2560),
];
pub(super) const LARGE_PROJECTION_TAPS: &[(&str, usize)] = &[
    (PROJECTION_INPUT, 2560),
    ("/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/output16", 1024),
    (V_PROJECTION, 1024),
    ("/airk_g5/weight//scorer/layers.0/self_attn/q_proj/MatMul/output16", 4096),
];
const PROJECTION_NODES: [&str; 3] = [
    "/scorer/layers.0/self_attn/k_proj/MatMul",
    "/scorer/layers.0/self_attn/v_proj/MatMul",
    "/scorer/layers.0/self_attn/q_proj/MatMul",
];

#[derive(Clone, Copy)]
pub(super) enum Layer0Cut {
    FullProjectionT1014(&'static [(&'static str, usize)]),
    Attention,
    Projection,
    LargeProjection,
    LargeProtectedHidden,
    LargeAttention17,
    LargeDense17,
    QkvAttentionT1024,
    QkvDecisionT1024,
}

impl Layer0Cut {
    pub(super) fn taps(self) -> &'static [(&'static str, usize)] {
        match self {
            Self::Attention | Self::QkvAttentionT1024 => COMPACT_FP16_LAYER0_TAPS,
            Self::Projection => COMPACT_FP16_LAYER0_PROJECTION_TAPS,
            Self::LargeProjection => LARGE_PROJECTION_TAPS,
            Self::LargeProtectedHidden => LARGE_QKV_HIDDEN_TAPS,
            Self::LargeAttention17 => LARGE_ATTENTION17,
            Self::LargeDense17 => LARGE_DENSE17,
            Self::FullProjectionT1014(taps) => taps,
            Self::QkvDecisionT1024 => Scorer::COMPACT_QKV_DECISION_TAPS,
        }
    }

    pub(super) fn tokens(self) -> usize {
        match self {
            Self::Attention | Self::Projection | Self::LargeProjection | Self::LargeProtectedHidden | Self::LargeAttention17 | Self::LargeDense17 => 32,
            Self::FullProjectionT1014(_) => 1014,
            Self::QkvAttentionT1024 | Self::QkvDecisionT1024 => 1024,
        }
    }

    pub(super) fn validate_input(self, input: &CompiledInput) -> Result<()> {
        match self {
            Self::Attention | Self::Projection | Self::LargeProjection | Self::LargeProtectedHidden | Self::LargeAttention17 | Self::LargeDense17 => validate_input(input),
            Self::FullProjectionT1014(_) => {
                ensure!(
                    input.input_digest
                        == "14290e4a90a69dee4d60f29b31c3be731f1b0c9c857bd04432f3758a3944fad2"
                            .parse()?
                        && input.input_ids.len() == 1014
                        && input.decision_position == 1013
                        && input.attention_mask == [1; 1014]
                        && input.position_ids.iter().copied().eq(0..1014)
                        && input.candidate_positions.iter().copied().eq(32..39)
                        && input.option_mask == [true; 7],
                    "original full-projection T1014/N7 input required"
                );
                Ok(())
            }
            Self::QkvAttentionT1024 | Self::QkvDecisionT1024 => {
                ensure!(
                    input.input_digest == T1024_INPUT.parse()?
                        && input.input_ids.len() == 1024
                        && input.attention_mask == [1; 1024]
                        && input.position_ids.iter().copied().eq(0..1024)
                        && input.decision_position == 1023
                        && input.candidate_positions.iter().copied().eq(32..40)
                        && input.option_mask == [true; 8],
                    "exact Compact Q/K/V diagnostic T1024 input required"
                );
                Ok(())
            }
        }
    }

    pub(super) fn scope(self) -> &'static [u8] {
        match self {
            Self::Attention => b"airk.private.compact-fp16-cuda-layer0-taps/1",
            Self::Projection => b"airk.private.compact-fp16-cuda-layer0-projection-taps/1",
            Self::LargeProjection => b"airk.private.large-fp16-cuda-layer0-projection-taps/1",
            Self::LargeProtectedHidden => b"airk.private.large-qkv-fp32-cuda-hidden-taps/1",
            Self::LargeAttention17 => b"airk.private.large-qkv-fp32-attention17/1",
            Self::LargeDense17 => b"airk.private.large-qkv-fp32-dense17/1",
            Self::FullProjectionT1014(_) => b"airk.private.compact-full-fp32-cuda-t1014-cuts/1",
            Self::QkvDecisionT1024 => b"airk.private.compact-fp32-qkv-cuda-decision-t1024/1",
            Self::QkvAttentionT1024 => b"airk.private.compact-fp32-qkv-cuda-layer0-t1024/1",
        }
    }
}
mod full_cut;

const T32_INPUT: &str = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335";
const T1024_INPUT: &str = "948a46156a129e61b5beadce977e6314b6e96c9570a439724813fe9592fa6357";

#[derive(Debug)]
pub struct CompactFp16Layer0GraphRefusal {
    pub observation: Box<GraphMetadataObservation>,
    pub(super) cause: anyhow::Error,
}

impl std::fmt::Display for CompactFp16Layer0GraphRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Compact FP16 layer-0 observation refused: {:#}",
            self.cause
        )
    }
}

impl std::error::Error for CompactFp16Layer0GraphRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

#[derive(Clone, Copy)]
struct ReviewedProjection {
    graph_sha: &'static str,
    load_sha: &'static str,
    load_schema: &'static str,
}

const REVIEWED_QKV: ReviewedProjection = ReviewedProjection {
    graph_sha: "e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14",
    load_sha: "42a273598b5c8678535691426ed725041a5d9565ff1fbaf535a43938894f07a4",
    load_schema: "airk.private.compact-fp32-qkv-graph-diagnostic/1",
};
const REVIEWED_QKVO: Option<ReviewedProjection> = Some(ReviewedProjection {
    graph_sha: "0f2407345000cb244674238eead6178bc711238f2a4ddc5b61eef603d87be0b1",
    load_sha: "ef46663525337868d8a47fe440b9199bc927cf1625e4b4caa3abeb764e93a1f1",
    load_schema: "airk.private.compact-fp32-qkvo-graph-diagnostic/1",
});

pub(super) fn qkv_projection_options(
    builder: ort::session::builder::SessionBuilder,
) -> Result<ort::session::builder::SessionBuilder> {
    Ok(builder
        .with_config_entry(
            "optimization.constant_folding_max_output_size_in_bytes",
            "1048576",
        )?
        .with_config_entry("optimization.enable_cast_chain_elimination", "0")?
        .with_config_entry(
            "optimization.disable_specified_optimizers",
            "FuseFp16InitializerToFp32NodeTransformer",
        )?)
}

fn reviewed_projection_gate(
    raw: &[u8],
    evidence_sha: Sha256Digest,
    review: ReviewedProjection,
) -> Result<GraphGate> {
    ensure!(
        evidence_sha == review.load_sha.parse()?,
        "exact reviewed Compact QKV graph receipt required"
    );
    let result = GraphGate::read_result(raw, evidence_sha)?;
    let snapshot = &result["graph_observation"];
    let teardown = &snapshot["teardown"];
    ensure!(
        result["schema"] == review.load_schema
            && result["complete"] == true
            && result["qualified"] == false
            && result["product_admission"] == false
            && result["source_mask"]["verified_candidate_graph"] == review.graph_sha
            && result["numeric_forward_attempts"] == 0
            && result["native_forward_returned"] == false
            && result["forward_synchronized"] == false
            && result.get("runtime_profile").is_some_and(Value::is_null)
            && result["rows"].as_array().is_some_and(Vec::is_empty)
            && result["input_refusals"]
                .as_array()
                .is_some_and(Vec::is_empty)
            && result["session_teardown_observed"] == true
            && result["device_teardown_observed"] == true
            && result["native_host_charge_release_observed"] == true
            && result["native_control_cleanup_observed"] == true
            && result["native_control_retained_for_unknown_cuda"] == false
            && result["process_cuda_poisoned"] == false
            && snapshot["session_created"] == true
            && snapshot["placement"]["complete"] == true
            && teardown["pre_sync_observed"] == true
            && teardown["session_drop_attempted"] == true
            && teardown["session_drop_returned"] == true
            && teardown["post_sync_observed"] == true
            && teardown["pending_claims"] == 0
            && teardown["poisoned"] == false
            && teardown["errors"].as_array().is_some_and(Vec::is_empty),
        "complete reviewed Compact QKV graph-only evidence required"
    );
    GraphGate::from_result(result, evidence_sha, GraphPolicy::CompactFp16)
}

fn require_qkv_projection_profile(
    profile: &ExecutionProfile,
    graph_sha: Sha256Digest,
) -> Result<()> {
    use airk_core::arbiter::device::DevicePrecision;

    cuda::diagnostic::require_graph_diagnostic_profile(profile)?;
    let description = profile.description();
    let device = description
        .device_execution
        .as_ref()
        .context("Compact QKV projection diagnostic CUDA profile missing")?;
    ensure!(
        description.model == ModelChoice::ContextQwenCompact
            && device.precision == DevicePrecision::Float16
            && device.graph == graph_sha,
        "Compact QKV projection diagnostic profile or graph differs"
    );
    Ok(())
}

impl Scorer {
    /// Opens the exact Windows Large protected-Q/K/V graph for a disposable placement observation.
    pub fn diagnose_large_fp32_qkv_graph(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
    ) -> Result<()> {
        super::super::cuda::diagnostic::require_graph_diagnostic_profile(&profile)?;
        let qkv = "1448e83dcd209b120bd22e1dec084092f56adc7e8eec5dcd323e179b800467aa".parse()?;
        let body = "636cea20b5fa4f58251589d01a7ef2e3df87f7f4b57498d2765b0d56f6b87d2d".parse()?;
        ensure!(
            profile.description.model == ModelChoice::ContextQwenLarge
                && profile.description.device_execution.as_ref().is_some_and(|d| d.graph == qkv || d.graph == body),
            "Large protected Q/K/V graph differs"
        );
        let session = Self::open_internal(profile, source, qkv_projection_options, OpenMode::GraphMetadataDiagnostic)?;
        ensure!(session.is_none(), "Large Q/K/V graph observation returned a scorer");
        Ok(())
    }

    /// Frozen normalized FP16 K-projection inputs sampled across Compact.
    pub const COMPACT_QKV_DECISION_TAPS: &'static [(&'static str, usize)] =
        decision_taps!(0, 9, 18, 27);
    /// Captures one disposable observation of a separately reviewed Compact FP16
    /// graph whose Q, K and V projections are under diagnostic review.
    pub fn load_private_compact_fp32_qkv_projection_diagnostic(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        baseline_graph_sha: Sha256Digest,
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<()> {
        ensure!(
            reviewed_graph_sha != baseline_graph_sha,
            "Compact QKV projection diagnostic requires a distinct reviewed graph identity"
        );
        require_qkv_projection_profile(&profile, reviewed_graph_sha)?;
        let session = Self::open_internal(
            profile,
            source,
            qkv_projection_options,
            OpenMode::GraphMetadataDiagnostic,
        )?;
        ensure!(
            session.is_none(),
            "QKV projection diagnostic unexpectedly returned a scorer"
        );
        Ok(())
    }

    /// Opens only the reviewed Compact projection experiment in a bounded CUDA session.
    pub fn open_private_compact_fp32_qkv_projection_profiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        Self::open_private_compact_fp32_projection_profiled(
            profile,
            source,
            Some(profiling_prefix),
            reviewed_graph_result,
            reviewed_graph_sha,
            REVIEWED_QKV,
            None,
        )
    }

    /// Opens the reviewed QKV graph with four private attention outputs for the sealed T1024 row.
    pub fn open_private_compact_fp32_qkv_layer0_t1024(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        Self::open_private_compact_fp32_projection_profiled(
            profile,
            source,
            Some(profiling_prefix),
            reviewed_graph_result,
            reviewed_graph_sha,
            REVIEWED_QKV,
            Some(Layer0Cut::QkvAttentionT1024),
        )
    }

    pub fn open_private_compact_fp32_qkv_decision_t1024(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        Self::open_private_compact_fp32_projection_profiled(
            profile,
            source,
            Some(profiling_prefix),
            reviewed_graph_result,
            reviewed_graph_sha,
            REVIEWED_QKV,
            Some(Layer0Cut::QkvDecisionT1024),
        )
    }

    pub fn open_private_compact_fp32_qkvo_projection_profiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        Self::open_private_compact_fp32_projection_profiled(
            profile,
            source,
            Some(profiling_prefix),
            reviewed_graph_result,
            reviewed_graph_sha,
            REVIEWED_QKVO.context("reviewed Compact Q/K/V/O load receipt unavailable")?,
            None,
        )
    }

    /// Opens the 196 FP32 dense products after their actual finalized CUDA placement was verified.
    pub fn open_private_compact_full_unprofiled(
        profile: ExecutionProfile,
        capture_taps: Option<&'static [(&'static str, usize)]>,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        ensure!(
            capture_taps.is_none_or(Self::is_full_projection_tap_group),
            "unknown four-output Compact projection diagnostic group"
        );
        let review = ReviewedProjection {
            graph_sha: "fe94664a4addf797feaa54235e07f484f4e72051a894a154e5aff7f2c1c11ea6",
            load_sha: "492da42c6b55364d79daa2b20b1d17373039acc5c43995c4df11031fa82e4c54",
            load_schema: "airk.private.compact-fp32-full-graph-diagnostic/1",
        };
        Self::open_private_compact_fp32_projection_profiled(
            profile,
            source,
            None,
            reviewed_graph_result,
            reviewed_graph_sha,
            review,
            capture_taps.map(Layer0Cut::FullProjectionT1014),
        )
    }

    /// Opens the exact protected MLP graph after its original typed placement proof.
    pub fn open_private_compact_down_unprofiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        let review = ReviewedProjection {
            graph_sha: "a583cbd0906a9801461ac0678d1607ecfe73398b637c19c97ea072c5750a33a1",
            load_sha: "295f0c79716cba2ec3da993e4a2659a7a67b6daec8bb473185c5818413d6daa4",
            load_schema: "airk.private.compact-fp32-qkv-down-graph-diagnostic/1",
        };
        Self::open_private_compact_fp32_projection_profiled(
            profile,
            source,
            None,
            reviewed_graph_result,
            reviewed_graph_sha,
            review,
            None,
        )
    }

    /// Opens the reviewed Q/K/V graph with ordinary outputs and no ORT profiler.
    pub fn open_private_compact_fp32_qkv_unprofiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        Self::open_private_compact_fp32_projection_profiled(
            profile,
            source,
            None,
            reviewed_graph_result,
            reviewed_graph_sha,
            REVIEWED_QKV,
            None,
        )
    }

    fn open_private_compact_fp32_projection_profiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: Option<&Path>,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        review: ReviewedProjection,
        layer0: Option<Layer0Cut>,
    ) -> Result<Self> {
        require_qkv_projection_profile(&profile, review.graph_sha.parse()?)?;
        let binding = profile
            .gpu
            .as_ref()
            .context("Compact QKV projection CUDA binding missing")?;
        ensure!(
            (
                binding.plan.cold_peak_bytes(),
                binding.plan.warm_extra_bytes(),
                binding.plan.headroom_bytes(),
            ) == (16 << 30, 8 << 30, 2 << 30),
            "Compact QKV projection CUDA reservation differs"
        );
        let deadline = binding.load_deadline;
        let mut gate = reviewed_projection_gate(reviewed_graph_result, reviewed_graph_sha, review)?;
        if let Some(cut) = layer0 {
            gate.policy = GraphPolicy::CompactFp16Layer0(cut);
        }
        gate.verify_profile(profile.description(), binding.device.observation(deadline)?)?;
        let scorer = Self::open_internal(
            profile,
            source,
            |builder| {
                let builder = qkv_projection_options(builder)?;
                match profiling_prefix {
                    Some(prefix) => Ok(builder.with_profiling(prefix)?),
                    None => Ok(builder),
                }
            },
            OpenMode::BoundedHybrid(Box::new(gate)),
        )?
        .context("Compact QKV projection scorer missing")?;
        scorer.compact_fp16_bounded_hybrid_attention_plan()?;
        cuda::in_time(deadline)?;
        Ok(scorer)
    }

    /// Scores the original independent T32 input through the reviewed QKV projection experiment.
    pub fn score_private_compact_fp32_qkv_projection_t32(
        &self,
        input: &CompiledInput,
        deadline: Instant,
    ) -> Result<Vec<f32>> {
        self.score_private_compact_fp32_projection_t32(input, deadline, REVIEWED_QKV)
    }

    pub fn score_private_compact_fp32_qkvo_projection_t32(
        &self,
        input: &CompiledInput,
        deadline: Instant,
    ) -> Result<Vec<f32>> {
        self.score_private_compact_fp32_projection_t32(
            input,
            deadline,
            REVIEWED_QKVO.context("reviewed Compact Q/K/V/O load receipt unavailable")?,
        )
    }

    fn score_private_compact_fp32_projection_t32(
        &self,
        input: &CompiledInput,
        deadline: Instant,
        review: ReviewedProjection,
    ) -> Result<Vec<f32>> {
        require_qkv_projection_profile(&self.profile, review.graph_sha.parse()?)?;
        let binding = self
            .profile
            .gpu
            .as_ref()
            .context("Compact QKV projection CUDA binding missing")?;
        ensure!(
            self.bounded_hybrid
                .as_ref()
                .is_some_and(|evidence| matches!(evidence.policy, GraphPolicy::CompactFp16))
                && deadline <= binding.load_deadline,
            "reviewed Compact QKV projection scorer and original deadline required"
        );
        validate_input(input)?;
        self.score_with_deadline(input, deadline)
    }
}

pub(super) fn validate_input(input: &CompiledInput) -> Result<()> {
    ensure!(
        input.input_digest == T32_INPUT.parse()?
            && input.prompt.is_empty()
            && input.input_ids.iter().copied().eq(1..=32)
            && input.attention_mask == [1; 32]
            && input.position_ids.iter().copied().eq(0..32)
            && input.decision_position == 31
            && input.candidate_positions.iter().copied().eq(32..40)
            && input.option_mask == [true; 8],
        "exact Compact FP16 diagnostic T32 input required"
    );
    Ok(())
}

fn varint(input: &mut &[u8]) -> Result<u64> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let (&byte, rest) = input
            .split_first()
            .context("truncated tap protobuf integer")?;
        *input = rest;
        ensure!(shift != 63 || byte <= 1, "overflowing tap protobuf integer");
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    anyhow::bail!("overflowing tap protobuf integer")
}

fn take<'a>(input: &mut &'a [u8], size: usize) -> Result<&'a [u8]> {
    let (prefix, rest) = input
        .split_at_checked(size)
        .context("truncated tap protobuf field")?;
    *input = rest;
    Ok(prefix)
}

fn field<'a>(input: &mut &'a [u8]) -> Result<(u64, Option<&'a [u8]>)> {
    let tag = varint(input)?;
    let payload = match tag & 7 {
        0 => {
            varint(input)?;
            None
        }
        1 => {
            take(input, 8)?;
            None
        }
        2 => {
            let size = usize::try_from(varint(input)?)?;
            Some(take(input, size)?)
        }
        5 => {
            take(input, 4)?;
            None
        }
        _ => anyhow::bail!("unsupported tap protobuf wire type"),
    };
    Ok((tag >> 3, payload))
}

fn write_varint(target: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        target.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    target.push(value as u8);
}

fn write_message(target: &mut Vec<u8>, number: u64, body: &[u8]) {
    write_varint(target, number * 8 + 2);
    write_varint(target, body.len() as u64);
    target.extend_from_slice(body);
}

fn write_output(graph: &mut Vec<u8>, name: &str, width: usize) {
    let mut shape = Vec::new();
    // Keep the admitted graph's partial shape. A fixed token count in the
    // declaration allows ORT to fold unrelated downstream shape operations.
    if fixed_tap_batch(name) {
        write_message(&mut shape, 1, &[8, 1]); // batch dim_value
        let mut tokens = Vec::new();
        write_message(&mut tokens, 2, b"tokens"); // dim_param
        write_message(&mut shape, 1, &tokens);
    } else {
        write_message(&mut shape, 1, &[]);
        write_message(&mut shape, 1, &[]);
    }
    let mut value = vec![8]; // TensorShapeProto.Dimension.dim_value
    write_varint(&mut value, width as u64);
    write_message(&mut shape, 1, &value);
    let mut tensor_type = vec![8, if name.starts_with("/scorer/") { 1 } else { 10 }];
    write_message(&mut tensor_type, 2, &shape);
    let mut value_type = Vec::new();
    write_message(&mut value_type, 1, &tensor_type);
    let mut info = Vec::new();
    write_message(&mut info, 1, name.as_bytes());
    write_message(&mut info, 2, &value_type);
    write_message(graph, 12, &info);
}

fn validate_base_graph(mut graph: &[u8], cut: Layer0Cut) -> Result<()> {
    let taps = cut.taps();
    let mut producers = vec![0; taps.len()];
    let mut outputs = 0;
    while !graph.is_empty() {
        match field(&mut graph)? {
            (1, Some(mut node)) => {
                while !node.is_empty() {
                    if let (2, Some(name)) = field(&mut node)?
                        && let Some(index) = taps.iter().position(|(tap, _)| name == tap.as_bytes())
                    {
                        producers[index] += 1;
                    }
                }
            }
            (12, Some(mut output)) => {
                outputs += 1;
                let mut names = 0;
                while !output.is_empty() {
                    if let (1, Some(name)) = field(&mut output)? {
                        ensure!(name == b"scores", "layer-0 base graph output differs");
                        names += 1;
                    }
                }
                ensure!(
                    names == 1,
                    "layer-0 base graph output name missing or repeated"
                );
            }
            _ => {}
        }
    }
    ensure!(
        outputs == 1 && producers.into_iter().all(|count| count == 1),
        "layer-0 requires one existing producer per tap and only the base scores output"
    );
    Ok(())
}

// Accept only after the original model digest and normal G5 audit pass.
pub(super) fn promote_model(model: &[u8], cut: Layer0Cut) -> Result<Vec<u8>> {
    let mut rest = model;
    let mut graph = None;
    while !rest.is_empty() {
        let start = model.len() - rest.len();
        if let (7, Some(payload)) = field(&mut rest)? {
            let end = model.len() - rest.len();
            ensure!(
                graph.replace((start, end, payload)).is_none(),
                "duplicate layer-0 base graph"
            );
        }
    }
    let (start, end, original) = graph.context("layer-0 base graph missing")?;
    validate_base_graph(original, cut)?;
    let mut graph = original.to_vec();
    for &(name, width) in cut.taps() {
        write_output(&mut graph, name, width);
    }
    let mut derived = Vec::with_capacity(model.len() + graph.len() - original.len() + 2);
    derived.extend_from_slice(&model[..start]);
    write_message(&mut derived, 7, &graph);
    derived.extend_from_slice(&model[end..]);
    ensure!(
        derived.len() <= 64 << 20,
        "layer-0 derived graph exceeds native cap"
    );
    Ok(derived)
}

pub(super) fn validate_output_abi(session: &Session, cut: Layer0Cut) -> Result<()> {
    ensure!(
        session.outputs.len() == 1 + cut.taps().len(),
        "layer-0 diagnostic output count differs"
    );
    for &(name, width) in cut.taps() {
        let batch = if fixed_tap_batch(name) {
            1
        } else {
            -1
        };
        ensure!(
            session.outputs.iter().any(|output| output.name == name
                && matches!(&output.output_type, ValueType::Tensor { ty, shape, .. }
                    if *ty == if name.starts_with("/scorer/") { ort::tensor::TensorElementType::Float32 } else { ort::tensor::TensorElementType::Float16 }
                        && shape.as_ref() == [batch, -1, width as i64])),
            "layer-0 diagnostic output ABI differs: {name}"
        );
    }
    Ok(())
}

type Operator<'a> = (u64, &'a str, &'a str, &'a str, &'a str, i64);

fn reviewed_operator(row: &Value) -> Result<Operator<'_>> {
    Ok((
        row["graph"]
            .as_u64()
            .context("reviewed operator graph missing")?,
        row["name"]
            .as_str()
            .context("reviewed operator name missing")?,
        row["domain"]
            .as_str()
            .context("reviewed operator domain missing")?,
        row["op"]
            .as_str()
            .context("reviewed operator type missing")?,
        row["ep"]
            .as_str()
            .context("reviewed operator provider missing")?,
        row["version"]
            .as_i64()
            .context("reviewed operator version missing")?,
    ))
}

pub(super) fn validate_graph(
    rows: &[GraphMetadataRow],
    base: &Value,
    cut: Layer0Cut,
) -> Result<()> {
    use GraphMetadataRow::*;
    let mut base_ops = base
        .as_array()
        .context("reviewed base graph rows missing")?
        .iter()
        .filter(|row| row["kind"] == "node")
        .map(reviewed_operator)
        .collect::<Result<Vec<_>>>()?;
    let mut live_ops = Vec::new();
    let mut cuda_nodes = BTreeSet::new();
    let mut gqa = None;
    let mut projections = [None; PROJECTION_NODES.len()];
    for row in rows {
        if let Node {
            graph,
            node,
            name,
            domain,
            op,
            ep,
            version,
        } = row
        {
            live_ops.push((
                *graph,
                name.as_str(),
                domain.as_str(),
                op.as_str(),
                ep.as_str(),
                *version,
            ));
            if *graph == 0 && ep == CUDA {
                cuda_nodes.insert(*node);
                if name == GQA && op == "GroupQueryAttention" && domain == "com.microsoft" {
                    gqa = Some(*node);
                }
                if op == "MatMul"
                    && domain.is_empty()
                    && let Some(index) = PROJECTION_NODES
                        .iter()
                        .position(|expected| name.as_str() == *expected)
                {
                    projections[index] = Some(*node);
                }
            }
        }
    }
    base_ops.sort_unstable();
    live_ops.sort_unstable();
    ensure!(
        live_ops == base_ops,
        "layer-0 optimized operator placement differs from reviewed base"
    );
    let gqa = gqa.context("layer-0 CUDA GroupQueryAttention missing")?;
    for (index, &(tap, _)) in cut.taps().iter().enumerate() {
        let value = rows
            .iter()
            .find_map(|row| match row {
                Value {
                    graph: 0,
                    value,
                    name,
                    element_type: Some(dtype),
                    rank: Some(3),
                    producer: Some(producer),
                    ..
                } if name == tap
                    && *dtype == if tap.starts_with("/scorer/") { 1 } else { 10 }
                    && cuda_nodes.contains(producer) =>
                {
                    Some(*value)
                }
                _ => None,
            })
            .context("layer-0 half tap producer missing or not CUDA")?;
        let has_edge = |owner, role, slot| {
            rows.iter().any(|row| matches!(row,
                Edge { graph: 0, node, value: Some(edge_value), role: edge_role, slot: edge_slot }
                    if *node == owner && *edge_value == value && *edge_role == role && *edge_slot == slot
            ))
        };
        let assigned = match cut {
            Layer0Cut::QkvDecisionT1024 | Layer0Cut::FullProjectionT1014(_) | Layer0Cut::LargeProtectedHidden | Layer0Cut::LargeAttention17 | Layer0Cut::LargeDense17 => {
                // The original reviewed graph already contains this named half boundary.
                base.as_array().is_some_and(|all| {
                    all.iter().any(|row| {
                        row["kind"] == "value"
                            && row["name"] == tap
                            && row["element_type"]
                                == if tap.starts_with("/scorer/") { 1 } else { 10 }
                            && row["rank"] == 3
                            && row["graph"] == 0
                            && row["producer"].is_u64()
                    })
                })
            }
            Layer0Cut::Attention | Layer0Cut::QkvAttentionT1024 if index == 3 => {
                has_edge(Some(gqa), 2, 0)
            }
            Layer0Cut::Attention | Layer0Cut::QkvAttentionT1024 => {
                has_edge(Some(gqa), 0, index as u64)
            }
            Layer0Cut::Projection | Layer0Cut::LargeProjection if index == 0 => projections
                .iter()
                .all(|&node| node.is_some() && has_edge(node, 0, 0)),
            Layer0Cut::Projection | Layer0Cut::LargeProjection => {
                let node = projections[index - 1];
                node.is_some() && has_edge(node, 2, 0)
            }
        };
        ensure!(
            assigned
                && rows.iter().any(|row| matches!(row,
                    Edge { graph: 0, node: None, value: Some(edge_value), role: 4, .. }
                        if *edge_value == value
                )),
            "layer-0 tap does not preserve its exact CUDA boundary and diagnostic output: {tap}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_taps_cover_four_pinned_normalized_attention_inputs() {
        let taps = Layer0Cut::QkvDecisionT1024.taps();
        assert_eq!(taps.len(), 4);
        for (&(name, width), layer) in taps.iter().zip([0, 9, 18, 27]) {
            assert_eq!(
                name,
                format!("/airk_g5/weight//scorer/layers.{layer}/self_attn/k_proj/MatMul/input16")
            );
            assert_eq!(width, 2048);
        }
        assert_eq!(Layer0Cut::QkvDecisionT1024.tokens(), 1024);
    }

    #[test]
    fn layer0_projection_outputs_retain_the_symbolic_token_axis() -> Result<()> {
        let expected = [
            "/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/input16",
            "/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/output16",
            "/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16",
            "/airk_g5/weight//scorer/layers.0/self_attn/q_proj/MatMul/output16",
        ];
        let mut graph = Vec::new();
        for name in expected {
            let mut node = Vec::new();
            write_message(&mut node, 2, name.as_bytes());
            write_message(&mut graph, 1, &node);
        }
        write_output(&mut graph, "scores", 8);
        let mut model = Vec::new();
        write_message(&mut model, 7, &graph);
        let derived = promote_model(&model, Layer0Cut::Projection)?;
        let (_, body) = field(&mut derived.as_slice())?;
        let mut added = body.unwrap().strip_prefix(graph.as_slice()).unwrap();
        for (name, width_varint) in expected.into_iter().zip([16, 8, 8, 16]) {
            let (number, body) = field(&mut added)?;
            assert_eq!(number, 12);
            let mut info = body.unwrap();
            assert_eq!(field(&mut info)?, (1, Some(name.as_bytes())));
            let (_, ty) = field(&mut info)?;
            let (_, body) = field(&mut ty.unwrap())?;
            let mut tensor = body.unwrap().strip_prefix(&[8, 10]).unwrap();
            // Literal encoded dimensions: 1, symbolic tokens, and known width.
            let mut dims = b"\x0a\x02\x08\x01\x0a\x08\x12\x06tokens\x0a\x03\x08\x80".to_vec();
            dims.push(width_varint);
            assert_eq!(field(&mut tensor)?, (2, Some(dims.as_slice())));
        }
        assert!(added.is_empty());
        Ok(())
    }

    #[test]
    fn layer0_outputs_append_to_the_existing_graph_only() -> Result<()> {
        let mut graph = Vec::new();
        for (tap, _) in COMPACT_FP16_LAYER0_TAPS {
            let mut node = Vec::new();
            write_message(&mut node, 2, tap.as_bytes());
            write_message(&mut graph, 1, &node);
        }
        write_output(&mut graph, "scores", 8);
        let mut model = vec![8, 10];
        write_message(&mut model, 7, &graph);
        write_message(&mut model, 2, b"literal producer");
        let original = model.clone();
        let derived = promote_model(&model, Layer0Cut::Attention)?;
        assert_eq!(
            promote_model(&model, Layer0Cut::QkvAttentionT1024)?,
            derived
        );
        let mut fields = derived.as_slice();
        assert_eq!(field(&mut fields)?, (1, None));
        let (number, body) = field(&mut fields)?;
        assert_eq!(number, 7);
        let added = body.unwrap().strip_prefix(graph.as_slice()).unwrap();
        let mut new_outputs = added;
        let head16 = [10, 0, 10, 0, 10, 3, 8, 128, 16];
        let head8 = [10, 0, 10, 0, 10, 3, 8, 128, 8];
        let projection = [
            10, 2, 8, 1, 10, 8, 18, 6, b't', b'o', b'k', b'e', b'n', b's', 10, 3, 8, 128, 8,
        ];
        for ((name, _), expected_shape) in COMPACT_FP16_LAYER0_TAPS.iter().zip([
            head16.as_slice(),
            head8.as_slice(),
            projection.as_slice(),
            head16.as_slice(),
        ]) {
            let (number, body) = field(&mut new_outputs)?;
            assert_eq!(number, 12);
            let mut info = body.unwrap();
            assert_eq!(field(&mut info)?, (1, Some(name.as_bytes())));
            let (number, body) = field(&mut info)?;
            assert_eq!(number, 2);
            let mut value_type = body.unwrap();
            let (number, body) = field(&mut value_type)?;
            assert_eq!(number, 1);
            let mut tensor = body.unwrap().strip_prefix(&[8, 10]).unwrap();
            assert_eq!(field(&mut tensor)?, (2, Some(expected_shape)));
            assert!(tensor.is_empty() && value_type.is_empty() && info.is_empty());
        }
        assert!(new_outputs.is_empty());
        assert_eq!(
            field(&mut fields)?,
            (2, Some(b"literal producer".as_slice()))
        );
        assert!(fields.is_empty());
        assert_eq!(model, original);
        assert!(promote_model(&derived, Layer0Cut::Attention).is_err());
        assert!(promote_model(&model, Layer0Cut::Projection).is_err());
        Ok(())
    }
}
