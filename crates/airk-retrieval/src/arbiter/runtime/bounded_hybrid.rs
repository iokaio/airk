// SPDX-License-Identifier: Apache-2.0
//! Private fixed-graph Context FP32 experiments.

use super::*;
use airk_core::{arbiter::resource_policy::ResourcePolicy, identity::framed_digest};
use cuda::{GraphMetadataObservation, GraphMetadataRow};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const CPU: &str = "CPUExecutionProvider";
const CUDA: &str = "CUDAExecutionProvider";
const MAX_NATIVE_RESULT_BYTES: usize = 16 << 20;
const MAX_CPU_VALUE_BYTES: u64 = 40;
const COMPACT_CPU_PAYLOAD_BYTES: u64 = 34_131;
const LARGE_CPU_PAYLOAD_BYTES: u64 = 43_819;
const LARGE_GRAPH_SHA: &str = "1892e5b040d8c51969f1d3361d0929950fa98f0ef4079060e48d00458635488a";
const BATCH_VIEW_GRAPH: &str = "5c9a26693eec8522bbfb4d5c4445fa0b60684449a5ea2e1098ec71cca96405b1";
const BATCH_VIEW_PLACEMENT: &str =
    "d4ccefff117a11c68b92c3f9c08b145a4fbb5a470114af5a23107c73a26bc431";
const APPROVED_WIDTHS: [usize; 4] = [32, 128, 1024, 8192];

pub(super) struct GraphGate {
    evidence_sha: Sha256Digest,
    profile: Sha256Digest,
    adapter: Value,
    expected_rows: Value,
    expected_counts: Value,
    model: ModelChoice,
}

pub(super) struct GraphEvidence {
    graph: GraphMetadataObservation,
    policy_identity: Sha256Digest,
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
    fn new(raw: &[u8], evidence_sha: Sha256Digest, model: ModelChoice) -> Result<Self> {
        ensure!(
            !raw.is_empty()
                && raw.len() <= MAX_NATIVE_RESULT_BYTES
                && Sha256Digest::from_bytes(Sha256::digest(raw).into()) == evidence_sha,
            "reviewed Context graph result bytes differ"
        );
        let mut result: Value = serde_json::from_slice(raw)?;
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
        let graph = &mut result["graph_metadata"];
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
            model,
        })
    }

    fn verify_profile(&self, profile: &Profile, adapter: cuda::AdapterObservation) -> Result<()> {
        let current = serde_json::to_value(adapter)?;
        ensure!(
            profile.manifest == self.profile
                && profile.model == self.model
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
        graph.validate_against(&placement)?;
        ensure!(
            serde_json::to_value(&graph.rows)? == self.expected_rows
                && serde_json::to_value(graph.reported_counts)? == self.expected_counts,
            "current Context final graph differs from reviewed graph"
        );
        let (maximum_payload, scope): (u64, &[u8]) = match self.model {
            ModelChoice::ContextQwenCompact => (
                COMPACT_CPU_PAYLOAD_BYTES,
                b"airk.private.compact-cuda-bounded-hybrid/1",
            ),
            ModelChoice::ContextQwenLarge => (
                LARGE_CPU_PAYLOAD_BYTES,
                b"airk.private.large-cuda-bounded-hybrid/1",
            ),
            _ => anyhow::bail!("bounded hybrid model outside reviewed Context scopes"),
        };
        validate_cpu_values(&graph.rows, maximum_payload)?;
        cuda::in_time(binding.load_deadline)?;
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
        })
    }
}

fn validate_cpu_values(rows: &[GraphMetadataRow], maximum_payload: u64) -> Result<()> {
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
            (Some(7), "tensor") => 8_u64,
            (Some(9), "tensor") => 1,
            _ => anyhow::bail!("CPU tensor element must be INT64 or BOOL"),
        };
        let elements = match rank {
            Some(0) => 1,
            Some(1) => match dimensions.get(&value) {
                Some(elements @ 0..=5) => *elements as u64,
                _ => anyhow::bail!("CPU vector requires a fixed length at most five"),
            },
            _ => anyhow::bail!("CPU tensor must be scalar or a fixed vector"),
        };
        let bytes = elements * width;
        ensure!(
            bytes <= MAX_CPU_VALUE_BYTES,
            "CPU tensor exceeds observed byte bound"
        );
        total_bytes += bytes;
    }
    ensure!(
        total_bytes <= maximum_payload,
        "distinct CPU tensor payload exceeds observed aggregate bound"
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
        let binding = profile
            .gpu
            .as_ref()
            .context("bounded hybrid CUDA binding missing")?;
        let gate = GraphGate::new(
            reviewed_graph_result,
            reviewed_graph_sha,
            profile.description().model,
        )?;
        gate.verify_profile(
            profile.description(),
            binding.device.observation(binding.load_deadline)?,
        )?;
        let mut scorer = Self::open_internal(
            profile,
            source,
            |builder| match profiling_prefix {
                Some(prefix) => Ok(builder.with_profiling(prefix)?),
                None => Ok(builder),
            },
            OpenMode::BoundedHybrid(Box::new(gate)),
        )?
        .context("bounded hybrid scorer missing")?;
        let evidence = scorer
            .bounded_hybrid
            .take()
            .context("same-session graph evidence missing")?;
        Ok((scorer, evidence))
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
