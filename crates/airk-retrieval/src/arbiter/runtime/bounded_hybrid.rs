// SPDX-License-Identifier: Apache-2.0
//! Private fixed-graph Context evaluation experiments.

use super::*;
use airk_core::{arbiter::resource_policy::ResourcePolicy, identity::framed_digest};
use cuda::{GraphMetadataObservation, GraphMetadataRow};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

mod layer0;
use layer0::Layer0Cut;
pub use layer0::{
    COMPACT_FP16_LAYER0_PROJECTION_TAPS, COMPACT_FP16_LAYER0_TAPS, CompactFp16Layer0GraphRefusal,
};

const CPU: &str = "CPUExecutionProvider";
const CUDA: &str = "CUDAExecutionProvider";
const MAX_NATIVE_RESULT_BYTES: usize = 16 << 20;
const MAX_CPU_VALUE_BYTES: u64 = 40;
const COMPACT_CPU_PAYLOAD_BYTES: u64 = 34_131;
const COMPACT_FP16_GRAPH_SHA: &str =
    "47a7cd6f9e0ddf50911cc7a745bcbec4ca2a963772aad5e2e6acc663e26ec104";
const COMPACT_FP16_ATTENTION_NODES: usize = 28;
const LARGE_CPU_PAYLOAD_BYTES: u64 = 43_819;
const LARGE_GRAPH_SHA: &str = "1892e5b040d8c51969f1d3361d0929950fa98f0ef4079060e48d00458635488a";
const BATCH_VIEW_GRAPH: &str = "5c9a26693eec8522bbfb4d5c4445fa0b60684449a5ea2e1098ec71cca96405b1";
const BATCH_VIEW_PLACEMENT: &str =
    "d4ccefff117a11c68b92c3f9c08b145a4fbb5a470114af5a23107c73a26bc431";
const APPROVED_WIDTHS: [usize; 4] = [32, 128, 1024, 8192];

#[derive(Clone, Copy)]
enum GraphPolicy {
    ContextFp32(ModelChoice),
    CompactFp16,
    CompactFp16Layer0(Layer0Cut),
}

impl GraphPolicy {
    fn layer0(self) -> Option<Layer0Cut> {
        match self {
            Self::CompactFp16Layer0(cut) => Some(cut),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct CpuValueBounds {
    maximum_payload: u64,
    maximum_vector_elements: i64,
    maximum_value_bytes: u64,
    allow_int32: bool,
}

impl CpuValueBounds {
    const COMPACT_FP16: Self = Self {
        maximum_payload: 9_443,
        maximum_vector_elements: 4,
        maximum_value_bytes: 32,
        allow_int32: true,
    };

    const fn fp32(maximum_payload: u64) -> Self {
        Self {
            maximum_payload,
            maximum_vector_elements: 5,
            maximum_value_bytes: MAX_CPU_VALUE_BYTES,
            allow_int32: false,
        }
    }
}

pub(super) struct GraphGate {
    evidence_sha: Sha256Digest,
    profile: Sha256Digest,
    adapter: Value,
    expected_rows: Value,
    expected_counts: Value,
    policy: GraphPolicy,
}

pub(super) struct GraphEvidence {
    graph: GraphMetadataObservation,
    policy_identity: Sha256Digest,
    policy: GraphPolicy,
}

impl GraphEvidence {
    pub(super) fn is_layer0(&self) -> bool {
        self.policy.layer0().is_some()
    }
}

pub struct BoundedHybridScorer {
    scorer: Scorer,
    evidence: GraphEvidence,
    deadline: Instant,
    profiling: Profiling,
}

/// Custody of the actual graph checked on the paired Answer session.
pub struct CompactBoundedHybridAnswerProvenance {
    evidence: GraphEvidence,
    overall_deadline: Instant,
    maximum_operation: Duration,
}

impl CompactBoundedHybridAnswerProvenance {
    pub fn policy_identity(&self) -> Sha256Digest {
        self.evidence.policy_identity
    }

    pub fn graph_metadata(&self) -> &GraphMetadataObservation {
        &self.evidence.graph
    }

    /// The owning evaluation resident checks this at each actual dispatch.
    pub fn require_operation_deadline(&self, deadline: Instant) -> Result<()> {
        require_answer_deadline(
            Instant::now(),
            deadline,
            self.overall_deadline,
            self.maximum_operation,
        )
    }
}

impl GraphGate {
    pub(super) fn is_layer0(&self) -> bool {
        self.policy.layer0().is_some()
    }

    pub(super) fn promote_model(&self, graph: Vec<u8>) -> Result<Vec<u8>> {
        match self.policy.layer0() {
            Some(cut) => layer0::promote_model(&graph, cut),
            None => Ok(graph),
        }
    }

    fn new(raw: &[u8], evidence_sha: Sha256Digest, model: ModelChoice) -> Result<Self> {
        let result = Self::read_result(raw, evidence_sha)?;
        ensure!(
            result["schema"] == "airk.private.cuda-graph-metadata-diagnostic/1"
                && result["diagnostic_complete"] == true
                && result["outcome"] == "graph_metadata_observed"
                && result["qualification"] == false
                && result["product_admission"] == false
                && result["forward_dispatched"] == false
                && result["native_host_charge_release_observed"] == true
                && result["native_control_cleanup_observed"] == true
                && result["placement"]["complete"] == true,
            "complete reviewed Context graph evidence required"
        );
        Self::from_result(result, evidence_sha, GraphPolicy::ContextFp32(model))
    }

    fn new_compact_fp16(raw: &[u8], evidence_sha: Sha256Digest) -> Result<Self> {
        ensure!(
            evidence_sha == COMPACT_FP16_GRAPH_SHA.parse()?,
            "exact reviewed Compact FP16 graph receipt required"
        );
        let result = Self::read_result(raw, evidence_sha)?;
        let snapshot = &result["graph_observation"];
        let teardown = &snapshot["teardown"];
        ensure!(
            result["schema"] == "airk.private.compact-fp16-graph-diagnostic/1"
                && result["complete"] == true
                && result["qualified"] == false
                && result["product_admission"] == false
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
            "complete reviewed Compact FP16 graph-only evidence required"
        );
        Self::from_result(result, evidence_sha, GraphPolicy::CompactFp16)
    }

    fn read_result(raw: &[u8], evidence_sha: Sha256Digest) -> Result<Value> {
        ensure!(
            !raw.is_empty()
                && raw.len() <= MAX_NATIVE_RESULT_BYTES
                && Sha256Digest::from_bytes(Sha256::digest(raw).into()) == evidence_sha,
            "reviewed Context graph result bytes differ"
        );
        Ok(serde_json::from_slice(raw)?)
    }

    fn from_result(
        mut result: Value,
        evidence_sha: Sha256Digest,
        policy: GraphPolicy,
    ) -> Result<Self> {
        let graph = match policy {
            GraphPolicy::ContextFp32(_) => &mut result["graph_metadata"],
            GraphPolicy::CompactFp16 | GraphPolicy::CompactFp16Layer0(_) => {
                &mut result["graph_observation"]["graph_metadata"]
            }
        };
        ensure!(
            graph["schema"] == "airk.private.ort-final-graph-metadata/1"
                && graph["complete"] == true
                && graph["shim_status"] == 0
                && graph["ort_error_code"] == 0
                && graph.get("failure").is_some_and(Value::is_null)
                && graph["rows"]
                    .as_array()
                    .is_some_and(|rows| !rows.is_empty())
                && graph["reported_counts"]
                    .as_array()
                    .is_some_and(|counts| counts.len() == 6),
            "complete reviewed Context graph rows required"
        );
        let expected_rows = graph["rows"].take();
        let expected_counts = graph["reported_counts"].take();
        Ok(Self {
            evidence_sha,
            profile: serde_json::from_value(result["profile"].take())?,
            adapter: result["adapter_before"].take(),
            expected_rows,
            expected_counts,
            policy,
        })
    }

    fn verify_profile(&self, profile: &Profile, adapter: cuda::AdapterObservation) -> Result<()> {
        let current = serde_json::to_value(adapter)?;
        let model = match self.policy {
            GraphPolicy::ContextFp32(model) => model,
            GraphPolicy::CompactFp16 | GraphPolicy::CompactFp16Layer0(_) => {
                ModelChoice::ContextQwenCompact
            }
        };
        ensure!(
            profile.manifest == self.profile
                && profile.model == model
                && [
                    "uuid",
                    "driver_image_sha256",
                    "driver_api_version",
                    "compute_major",
                    "compute_minor"
                ]
                .into_iter()
                .all(|key| {
                    self.adapter
                        .get(key)
                        .is_some_and(|expected| current.get(key) == Some(expected))
                }),
            "reviewed Context profile, adapter, or driver differs"
        );
        Ok(())
    }

    pub(super) fn verify_session(
        &self,
        binding: &cuda::Binding,
        session: &Session,
    ) -> Result<GraphEvidence> {
        let placement = binding.device.placement(session)?;
        cuda::diagnostic::complete_placement(&placement)?;
        let graph = binding
            .device
            .graph_metadata(session, binding.load_deadline, &placement)?;
        let (bounds, scope): (CpuValueBounds, &[u8]) = match self.policy {
            GraphPolicy::ContextFp32(ModelChoice::ContextQwenCompact) => (
                CpuValueBounds::fp32(COMPACT_CPU_PAYLOAD_BYTES),
                b"airk.private.compact-cuda-bounded-hybrid/1",
            ),
            GraphPolicy::ContextFp32(ModelChoice::ContextQwenLarge) => (
                CpuValueBounds::fp32(LARGE_CPU_PAYLOAD_BYTES),
                b"airk.private.large-cuda-bounded-hybrid/1",
            ),
            GraphPolicy::CompactFp16 => (
                CpuValueBounds::COMPACT_FP16,
                b"airk.private.compact-fp16-cuda-bounded-hybrid/1",
            ),
            GraphPolicy::CompactFp16Layer0(cut) => (CpuValueBounds::COMPACT_FP16, cut.scope()),
            GraphPolicy::ContextFp32(_) => {
                anyhow::bail!("bounded hybrid model outside reviewed Context scopes")
            }
        };
        let verified = (|| -> Result<()> {
            graph.validate_against(&placement)?;
            if let Some(cut) = self.policy.layer0() {
                layer0::validate_graph(&graph.rows, &self.expected_rows, cut)?;
                layer0::validate_output_abi(session, cut)?;
            } else {
                ensure!(
                    serde_json::to_value(&graph.rows)? == self.expected_rows
                        && serde_json::to_value(graph.reported_counts)? == self.expected_counts,
                    "current Context final graph differs from reviewed graph"
                );
            }
            validate_cpu_values(&graph.rows, bounds)?;
            cuda::in_time(binding.load_deadline)
        })();
        if let Err(cause) = verified {
            if self.is_layer0() {
                return Err(CompactFp16Layer0GraphRefusal {
                    observation: Box::new(graph),
                    cause,
                }
                .into());
            }
            return Err(cause);
        }
        let source = Sha256::digest(include_bytes!("bounded_hybrid.rs"));
        let policy_identity = framed_digest(
            scope,
            &[
                self.evidence_sha.as_bytes(),
                self.profile.as_bytes(),
                &source,
            ],
        );
        Ok(GraphEvidence {
            graph,
            policy_identity,
            policy: self.policy,
        })
    }
}

fn validate_cpu_values(rows: &[GraphMetadataRow], bounds: CpuValueBounds) -> Result<()> {
    use GraphMetadataRow::*;
    let mut root_count = 0;
    let mut cpu_nodes = BTreeSet::new();
    let mut cuda_nodes = 0;
    let mut tensors = BTreeMap::new();
    let mut dimensions = BTreeMap::new();
    let mut incident_values = BTreeSet::new();
    for row in rows {
        match row {
            Graph {
                graph,
                parent_graph,
                parent_node,
                outer_bindings,
                ..
            } => {
                ensure!(
                    *graph == 0
                        && parent_graph.is_none()
                        && parent_node.is_none()
                        && outer_bindings == "not_applicable",
                    "bounded hybrid requires one fully resolved root graph"
                );
                root_count += 1;
            }
            Node { node, ep, .. } if ep == CPU => {
                cpu_nodes.insert(*node);
            }
            Node { ep, .. } => {
                ensure!(ep == CUDA, "unexpected bounded hybrid execution provider");
                cuda_nodes += 1;
            }
            Value {
                value,
                element_type,
                rank,
                type_state,
                ..
            } => {
                tensors.insert(*value, (*element_type, *rank, type_state.as_str()));
            }
            Dimension {
                value,
                index: 0,
                number: Some(number),
                symbol: None,
                ..
            } => {
                dimensions.insert(*value, *number);
            }
            Edge {
                node: Some(node),
                value,
                role,
                ..
            } if cpu_nodes.contains(node) => {
                // ORT's kernel input profile omits implicit slots. The frozen
                // Context graphs have none on CPU, so require that exact scope.
                ensure!(*role != 1, "CPU implicit input cannot be profiled");
                incident_values.extend(*value);
            }
            _ => {}
        }
    }
    ensure!(
        root_count == 1 && !cpu_nodes.is_empty() && cuda_nodes > 0 && !incident_values.is_empty(),
        "bounded hybrid requires an observed CPU and CUDA root graph"
    );
    let mut total_bytes = 0_u64;
    for value in incident_values {
        let &(element, rank, state) = tensors.get(&value).context("missing CPU tensor value")?;
        let width = match (element, state) {
            (Some(6), "tensor") if bounds.allow_int32 => 4_u64,
            (Some(7), "tensor") => 8_u64,
            (Some(9), "tensor") => 1,
            _ => anyhow::bail!("CPU tensor element is outside the reviewed control types"),
        };
        let elements = match rank {
            Some(0) => 1,
            Some(1) => match dimensions.get(&value) {
                Some(elements) if (0..=bounds.maximum_vector_elements).contains(elements) => {
                    *elements as u64
                }
                _ => anyhow::bail!(
                    "CPU vector requires a fixed length at most {}",
                    bounds.maximum_vector_elements
                ),
            },
            _ => anyhow::bail!("CPU tensor must be scalar or a fixed vector"),
        };
        let bytes = elements * width;
        ensure!(
            bytes <= bounds.maximum_value_bytes,
            "CPU tensor exceeds observed byte bound"
        );
        total_bytes += bytes;
    }
    ensure!(
        total_bytes <= bounds.maximum_payload,
        "distinct CPU tensor payload exceeds observed aggregate bound"
    );
    Ok(())
}

fn validate_compact_fp16_attention(rows: &[GraphMetadataRow], plan: &Value) -> Result<()> {
    const OP: &str = "GroupQueryAttention";
    const DOMAIN: &str = "com.microsoft";
    let expected = plan["nodes"]
        .as_array()
        .context("G5 canonical attention roster missing")?;
    ensure!(
        plan["operator"] == OP
            && plan["domain"] == DOMAIN
            && plan["dtype"] == "fp16"
            && plan["backend"] == "FLASH_ATTENTION"
            && expected.len() == COMPACT_FP16_ATTENTION_NODES,
        "Compact FP16 canonical attention plan differs"
    );
    let mut expected_names = BTreeSet::new();
    for node in expected {
        let name = node["name"].as_str().context("G5 attention name missing")?;
        ensure!(
            node["operator"] == OP && node["domain"] == DOMAIN && expected_names.insert(name),
            "Compact FP16 canonical attention node differs"
        );
    }

    let mut live_names = BTreeSet::new();
    for row in rows {
        if let GraphMetadataRow::Node {
            graph,
            name,
            domain,
            op,
            ep,
            ..
        } = row
            && matches!(
                op.as_str(),
                "MultiHeadAttention"
                    | OP
                    | "Attention"
                    | "PackedAttention"
                    | "DecoderMaskedMultiHeadAttention"
            )
        {
            ensure!(
                *graph == 0
                    && domain == DOMAIN
                    && op == OP
                    && ep == CUDA
                    && live_names.insert(name.as_str()),
                "Compact FP16 final attention node must be unique root CUDA GroupQueryAttention"
            );
        }
    }
    ensure!(
        live_names == expected_names,
        "Compact FP16 final attention roster differs from canonical G5 plan"
    );
    Ok(())
}

enum OpenExperiment<'a> {
    Profiled(&'a Path),
    Resource(Instant),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Profiling {
    Active,
    Finished,
    Disabled,
}

fn require_resource_operation(
    input: &CompiledInput,
    now: Instant,
    deadline: Instant,
    overall_deadline: Instant,
) -> Result<()> {
    ensure!(
        now < deadline
            && deadline <= overall_deadline
            && deadline.duration_since(now) <= Duration::from_secs(120),
        "Context resource operation exceeded its original deadline"
    );
    ensure!(
        input.input_ids.len() == 8192
            && input.attention_mask.len() == 8192
            && input.attention_mask.iter().all(|&bit| bit == 1)
            && input.candidate_positions.len() == 8
            && input.option_mask == [true; 8],
        "Context resource scoring requires the exact T8192/N8 all-live shape"
    );
    Ok(())
}

fn require_answer_deadline(
    now: Instant,
    deadline: Instant,
    overall_deadline: Instant,
    maximum_operation: Duration,
) -> Result<()> {
    ensure!(
        now < deadline
            && deadline <= overall_deadline
            && deadline.duration_since(now) <= maximum_operation,
        "Compact Answer operation exceeded its original deadline"
    );
    Ok(())
}

fn measurement_capacity_is_reviewed(binding: &cuda::Binding) -> bool {
    (
        binding.plan.cold_peak_bytes(),
        binding.plan.warm_extra_bytes(),
        binding.plan.headroom_bytes(),
    ) == (22 << 30, 16 << 30, 2 << 30)
}

impl Scorer {
    /// Private Windows evaluation open for the exact reviewed Compact FP16 graph.
    pub fn open_compact_fp16_bounded_hybrid_profiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        Self::open_compact_fp16_gated(
            profile,
            source,
            profiling_prefix,
            reviewed_graph_result,
            reviewed_graph_sha,
            None,
        )
    }

    /// Private fixed-T32 diagnostic. Adds four outputs to a verified in-memory graph.
    pub fn open_compact_fp16_layer0_taps(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        Self::open_compact_fp16_gated(
            profile,
            source,
            profiling_prefix,
            reviewed_graph_result,
            reviewed_graph_sha,
            Some(Layer0Cut::Attention),
        )
    }

    /// Private fixed-T32 diagnostic of the common half input and Q/K/V projections.
    pub fn open_compact_fp16_layer0_projection_taps(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        Self::open_compact_fp16_gated(
            profile,
            source,
            profiling_prefix,
            reviewed_graph_result,
            reviewed_graph_sha,
            Some(Layer0Cut::Projection),
        )
    }

    fn open_compact_fp16_gated(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        layer0: Option<Layer0Cut>,
    ) -> Result<Self> {
        use airk_core::arbiter::device::{DevicePrecision, PlacementPolicy};

        cuda::diagnostic::require_graph_diagnostic_profile(&profile)?;
        let description = profile.description();
        let binding = profile
            .gpu
            .as_ref()
            .context("Compact FP16 bounded hybrid CUDA binding missing")?;
        ensure!(
            description.model == ModelChoice::ContextQwenCompact
                && description.device_execution.as_ref().is_some_and(|device| {
                    device.precision == DevicePrecision::Float16
                        && device.placement == PlacementPolicy::StrictGpu
                })
                && (
                    binding.plan.cold_peak_bytes(),
                    binding.plan.warm_extra_bytes(),
                    binding.plan.headroom_bytes(),
                ) == (16 << 30, 8 << 30, 2 << 30),
            "Compact FP16 bounded hybrid requires the exact probe precision and device reservation"
        );
        let load_deadline = binding.load_deadline;
        let mut gate = GraphGate::new_compact_fp16(reviewed_graph_result, reviewed_graph_sha)?;
        if let Some(cut) = layer0 {
            gate.policy = GraphPolicy::CompactFp16Layer0(cut);
        }
        let scorer = Self::open_gated_graph(profile, source, Some(profiling_prefix), gate)?;
        scorer.compact_fp16_bounded_hybrid_attention_plan()?;
        cuda::in_time(load_deadline)?;
        Ok(scorer)
    }

    pub fn compact_fp16_bounded_hybrid_evidence(
        &self,
    ) -> Result<(&GraphMetadataObservation, Sha256Digest)> {
        let evidence = self
            .bounded_hybrid
            .as_ref()
            .filter(|evidence| {
                matches!(
                    evidence.policy,
                    GraphPolicy::CompactFp16 | GraphPolicy::CompactFp16Layer0(_)
                )
            })
            .context("same-session Compact FP16 bounded graph evidence missing")?;
        Ok((&evidence.graph, evidence.policy_identity))
    }

    /// Canonical G5 plan verified against this scorer's bounded final graph.
    pub fn compact_fp16_bounded_hybrid_attention_plan(&self) -> Result<Value> {
        let (graph, _) = self.compact_fp16_bounded_hybrid_evidence()?;
        let plan = self
            .fp16_attention
            .as_ref()
            .context("Compact FP16 canonical attention plan missing")?
            .observation()?;
        validate_compact_fp16_attention(&graph.rows, &plan)?;
        Ok(plan)
    }

    pub fn score_layer0_taps(
        &self,
        input: &CompiledInput,
        deadline: Instant,
    ) -> Result<(Vec<f32>, Vec<Vec<u16>>)> {
        crate::native_inference::require_service_compute()?;
        self.profile.validate_native_source()?;
        let cut = self
            .bounded_hybrid
            .as_ref()
            .and_then(|evidence| evidence.policy.layer0())
            .context("exact admitted Compact FP16 layer-0 diagnostic session required")?;
        let binding = self
            .profile
            .gpu
            .as_ref()
            .context("layer-0 CUDA binding missing")?;
        ensure!(
            deadline <= binding.load_deadline,
            "original layer-0 deadline differs"
        );
        cuda::in_time(deadline)?;
        layer0::validate_input(input)?;
        let description = self.profile.description();
        input::validate(
            input,
            description.abi.readout,
            description.max_tokens,
            description.max_candidates,
        )?;
        self.compiler.validate_qwen_readout(input)?;
        let plan = self
            .fp16_attention
            .as_ref()
            .context("layer-0 FP16 attention plan missing")?;
        plan.validate_input(input)?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter native session poisoned"))?;
        let result = binding.score_with_layer0_taps(&mut session, input, deadline, cut.taps());
        if result.is_ok() {
            plan.validate_input(input)?;
        }
        result
    }

    pub fn open_compact_bounded_hybrid_profiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<BoundedHybridScorer> {
        Self::open_compact_bounded_hybrid(
            profile,
            source,
            reviewed_graph_result,
            reviewed_graph_sha,
            OpenExperiment::Profiled(profiling_prefix),
        )
    }

    pub fn open_large_bounded_hybrid_profiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<BoundedHybridScorer> {
        Self::open_large_bounded_hybrid(
            profile,
            source,
            reviewed_graph_result,
            reviewed_graph_sha,
            OpenExperiment::Profiled(profiling_prefix),
        )
    }

    pub fn open_large_bounded_hybrid_resource(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        overall_deadline: Instant,
    ) -> Result<BoundedHybridScorer> {
        Self::open_large_bounded_hybrid(
            profile,
            source,
            reviewed_graph_result,
            reviewed_graph_sha,
            OpenExperiment::Resource(overall_deadline),
        )
    }

    fn open_large_bounded_hybrid(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        experiment: OpenExperiment<'_>,
    ) -> Result<BoundedHybridScorer> {
        cuda::diagnostic::require_graph_diagnostic_profile(&profile)?;
        let binding = profile
            .gpu
            .as_ref()
            .context("Large bounded hybrid CUDA binding missing")?;
        let candidate = profile
            .description()
            .device_execution
            .as_ref()
            .context("Large bounded hybrid device policy missing")?;
        let placement = if candidate.graph == BATCH_VIEW_GRAPH.parse()? {
            BATCH_VIEW_PLACEMENT
        } else {
            LARGE_GRAPH_SHA
        };
        ensure!(
            profile.description().model == ModelChoice::ContextQwenLarge
                && reviewed_graph_sha == placement.parse()?
                && (
                    binding.plan.cold_peak_bytes(),
                    binding.plan.warm_extra_bytes(),
                    binding.plan.headroom_bytes(),
                ) == (40 << 30, 8 << 30, 4 << 30),
            "Large bounded hybrid requires the exact graph and device reservation"
        );
        let (deadline, prefix, profiling) = match experiment {
            OpenExperiment::Profiled(prefix) => {
                (binding.load_deadline, Some(prefix), Profiling::Active)
            }
            OpenExperiment::Resource(overall_deadline) => {
                ensure!(
                    placement == BATCH_VIEW_PLACEMENT && binding.load_deadline <= overall_deadline,
                    "Large resource measurement requires the corrected graph and original expiry"
                );
                (overall_deadline, None, Profiling::Disabled)
            }
        };
        let (scorer, mut evidence) = Self::open_reviewed_graph(
            profile,
            source,
            reviewed_graph_result,
            reviewed_graph_sha,
            prefix,
        )?;
        if profiling == Profiling::Disabled {
            evidence.policy_identity = framed_digest(
                b"airk.private.large-cuda-batch-view-bounded-hybrid-resource/1",
                &[evidence.policy_identity.as_bytes()],
            );
        }
        Ok(BoundedHybridScorer {
            scorer,
            evidence,
            deadline,
            profiling,
        })
    }

    pub fn open_compact_bounded_hybrid_resource(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        overall_deadline: Instant,
    ) -> Result<BoundedHybridScorer> {
        Self::open_compact_bounded_hybrid(
            profile,
            source,
            reviewed_graph_result,
            reviewed_graph_sha,
            OpenExperiment::Resource(overall_deadline),
        )
    }

    /// Opens the unprofiled scorer for the actual private Context Answer actor.
    /// The caller retains its returned proof with this same resident and uses
    /// the proof's deadline check at each native dispatch.
    pub fn open_compact_bounded_hybrid_answer(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        overall_deadline: Instant,
    ) -> Result<(Self, CompactBoundedHybridAnswerProvenance)> {
        cuda::diagnostic::require_diagnostic_profile(&profile)?;
        let description = profile.description();
        let policy = ResourcePolicy::software_v2();
        ensure!(
            description.max_tokens == policy.context_input_tokens
                && description.max_candidates == policy.maximum_context_candidates
                && description.tasks.contains(&Task::ContextAction),
            "Compact Answer requires the original Context action limits"
        );
        let policy_digest = policy.digest()?;
        let maximum_operation = Duration::from_millis(policy.deadline_ms);
        let binding = profile
            .gpu
            .as_ref()
            .context("bounded hybrid CUDA binding missing")?;
        ensure!(
            measurement_capacity_is_reviewed(binding),
            "Compact Answer capacity differs from the reviewed experiment"
        );
        let load_deadline = binding.load_deadline;
        require_answer_deadline(
            Instant::now(),
            load_deadline,
            overall_deadline,
            maximum_operation,
        )?;
        let (scorer, mut evidence) = Self::open_reviewed_graph(
            profile,
            source,
            reviewed_graph_result,
            reviewed_graph_sha,
            None,
        )?;
        cuda::in_time(load_deadline)?;
        evidence.policy_identity = framed_digest(
            b"airk.private.compact-cuda-bounded-hybrid-answer/1",
            &[
                evidence.policy_identity.as_bytes(),
                policy_digest.as_bytes(),
            ],
        );
        Ok((
            scorer,
            CompactBoundedHybridAnswerProvenance {
                evidence,
                overall_deadline,
                maximum_operation,
            },
        ))
    }

    fn open_compact_bounded_hybrid(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        experiment: OpenExperiment<'_>,
    ) -> Result<BoundedHybridScorer> {
        cuda::diagnostic::require_diagnostic_profile(&profile)?;
        let binding = profile
            .gpu
            .as_ref()
            .context("bounded hybrid CUDA binding missing")?;
        let load_deadline = binding.load_deadline;
        let (deadline, profiling_prefix) = match experiment {
            OpenExperiment::Profiled(prefix) => (load_deadline, Some(prefix)),
            OpenExperiment::Resource(overall_deadline) => {
                ensure!(
                    load_deadline <= overall_deadline,
                    "Compact resource load deadline exceeds the original experiment expiry"
                );
                ensure!(
                    measurement_capacity_is_reviewed(binding),
                    "Compact resource capacity differs from the reviewed experiment"
                );
                (overall_deadline, None)
            }
        };
        let (scorer, mut evidence) = Self::open_reviewed_graph(
            profile,
            source,
            reviewed_graph_result,
            reviewed_graph_sha,
            profiling_prefix,
        )?;
        let profiling = if profiling_prefix.is_some() {
            Profiling::Active
        } else {
            evidence.policy_identity = framed_digest(
                b"airk.private.compact-cuda-bounded-hybrid-resource/1",
                &[evidence.policy_identity.as_bytes()],
            );
            Profiling::Disabled
        };
        Ok(BoundedHybridScorer {
            scorer,
            evidence,
            deadline,
            profiling,
        })
    }

    fn open_reviewed_graph(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        profiling_prefix: Option<&Path>,
    ) -> Result<(Self, GraphEvidence)> {
        let gate = GraphGate::new(
            reviewed_graph_result,
            reviewed_graph_sha,
            profile.description().model,
        )?;
        let mut scorer = Self::open_gated_graph(profile, source, profiling_prefix, gate)?;
        let evidence = scorer
            .bounded_hybrid
            .take()
            .context("same-session graph evidence missing")?;
        Ok((scorer, evidence))
    }

    fn open_gated_graph(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: Option<&Path>,
        gate: GraphGate,
    ) -> Result<Self> {
        let binding = profile
            .gpu
            .as_ref()
            .context("bounded hybrid CUDA binding missing")?;
        gate.verify_profile(
            profile.description(),
            binding.device.observation(binding.load_deadline)?,
        )?;
        Self::open_internal(
            profile,
            source,
            |builder| match profiling_prefix {
                Some(prefix) => Ok(builder.with_profiling(prefix)?),
                None => Ok(builder),
            },
            OpenMode::BoundedHybrid(Box::new(gate)),
        )?
        .context("bounded hybrid scorer missing")
    }
}

impl BoundedHybridScorer {
    pub fn policy_identity(&self) -> Sha256Digest {
        self.evidence.policy_identity
    }

    pub fn graph_metadata(&self) -> &GraphMetadataObservation {
        &self.evidence.graph
    }

    pub fn score_with_deadline(
        &mut self,
        input: &CompiledInput,
        deadline: Instant,
    ) -> Result<Vec<f32>> {
        match self.profiling {
            Profiling::Active => {
                ensure!(
                    deadline <= self.deadline && APPROVED_WIDTHS.contains(&input.input_ids.len()),
                    "bounded hybrid input width or original deadline differs"
                );
            }
            Profiling::Finished => anyhow::bail!("bounded hybrid profiling has ended"),
            Profiling::Disabled => {
                require_resource_operation(input, Instant::now(), deadline, self.deadline)?;
            }
        }
        self.scorer.score_with_deadline(input, deadline)
    }

    pub fn finish_profiling(&mut self) -> Result<PathBuf> {
        // A failed finish can leave the profiler state unknown. Scoring stays closed.
        ensure!(
            self.profiling == Profiling::Active,
            "bounded hybrid profiler is inactive"
        );
        self.profiling = Profiling::Finished;
        cuda::in_time(self.deadline)?;
        let path = self.scorer.finish_profiling()?;
        cuda::in_time(self.deadline)?;
        Ok(path)
    }

    pub fn synchronize(&self, deadline: Instant) -> Result<()> {
        self.scorer.synchronize(deadline.min(self.deadline))
    }

    pub fn device_settled(&self) -> bool {
        self.scorer.device_settled()
    }
}

#[cfg(test)]
mod tests;
