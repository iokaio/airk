// SPDX-License-Identifier: Apache-2.0
//! Scoped Answer dispatch for the reviewed private Compact and Large CUDA recipes.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnswerRecipe {
    OriginalFp32,
    ProtectedQkv,
    /// The corrected Large FP32 batch-view graph with its reviewed placement.
    LargeBatchView,
    /// A protected efficient-MHA FP16 graph (Compact 147be075 or Large c1b57761)
    /// under the evaluation-only bounded admission; its own bounded policy digest is
    /// the review pin and its capacity is the profile's measured device memory.
    ProtectedMha,
}

impl AnswerRecipe {
    fn identify_large(
        precision: airk_core::arbiter::device::DevicePrecision,
        graph: Sha256Digest,
        receipt: Sha256Digest,
    ) -> Result<Self> {
        ensure!(
            precision == airk_core::arbiter::device::DevicePrecision::Float32
                && graph == BATCH_VIEW_GRAPH.parse()?
                && receipt == BATCH_VIEW_PLACEMENT.parse()?,
            "Large CUDA Answer requires the reviewed FP32 batch-view graph and placement"
        );
        Ok(Self::LargeBatchView)
    }

    fn identify(
        precision: airk_core::arbiter::device::DevicePrecision,
        graph: Sha256Digest,
        receipt: Sha256Digest,
    ) -> Result<Self> {
        use airk_core::arbiter::device::DevicePrecision;
        match precision {
            DevicePrecision::Float32 => {
                ensure!(
                    graph != REVIEWED_QKV.graph_sha.parse()?
                        && receipt != REVIEWED_QKV.load_sha.parse()?,
                    "protected QKV evidence cannot authorize the original FP32 graph"
                );
                Ok(Self::OriginalFp32)
            }
            DevicePrecision::Float16 => {
                ensure!(
                    graph == REVIEWED_QKV.graph_sha.parse()?
                        && receipt == REVIEWED_QKV.load_sha.parse()?,
                    "Compact FP16 Answer requires the reviewed protected QKV graph and load receipt"
                );
                Ok(Self::ProtectedQkv)
            }
        }
    }

    fn for_profile(profile: &ExecutionProfile, receipt: Sha256Digest) -> Result<Self> {
        let description = profile.description();
        let device = description
            .device_execution
            .as_ref()
            .context("CUDA Answer profile missing")?;
        if matches!(
            device.placement,
            airk_core::arbiter::device::PlacementPolicy::BoundedHybrid { .. }
        ) {
            ensure!(
                cuda::evaluation_bounded(description)?,
                "bounded CUDA Answer requires the protected evaluation admission"
            );
            return Ok(Self::ProtectedMha);
        }
        match description.model {
            ModelChoice::ContextQwenCompact => {
                Self::identify(device.precision, device.graph, receipt)
            }
            ModelChoice::ContextQwenLarge => {
                Self::identify_large(device.precision, device.graph, receipt)
            }
            _ => anyhow::bail!("private CUDA Answer requires Compact or Large Context"),
        }
    }

    fn capacity_for(self, description: &Profile) -> Result<cuda::DeviceCapacityPlan> {
        match self {
            // The profile's own measured cold peak; warm extra is bounded by the cold peak.
            Self::ProtectedMha => match description
                .device_execution
                .as_ref()
                .map(|device| &device.memory)
            {
                Some(airk_core::arbiter::device::DeviceMemory::Dedicated {
                    required_peak_bytes,
                    headroom_bytes,
                }) => cuda::DeviceCapacityPlan::probe(
                    *required_peak_bytes,
                    *required_peak_bytes,
                    *headroom_bytes,
                ),
                _ => anyhow::bail!("protected MHA profile has no measured dedicated memory"),
            },
            _ => self.capacity(),
        }
    }

    fn capacity(self) -> Result<cuda::DeviceCapacityPlan> {
        match self {
            Self::ProtectedMha => {
                anyhow::bail!("protected MHA capacity comes from its measured profile")
            }
            Self::OriginalFp32 => cuda::DeviceCapacityPlan::probe(22 << 30, 16 << 30, 2 << 30),
            Self::ProtectedQkv => cuda::DeviceCapacityPlan::probe(16 << 30, 8 << 30, 2 << 30),
            // The reservation the corrected Large graph ran its four widths under.
            Self::LargeBatchView => cuda::DeviceCapacityPlan::probe(40 << 30, 8 << 30, 4 << 30),
        }
    }
}

impl Scorer {
    /// Selects the reviewed phase reservation before the private service probes a device.
    pub fn private_compact_answer_capacity(
        profile: &ExecutionProfile,
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<cuda::DeviceCapacityPlan> {
        AnswerRecipe::for_profile(profile, reviewed_graph_sha)?.capacity_for(profile.description())
    }

    /// Opens the exact private Answer graph and retains this session's observed placement
    /// with its original service lease. Normal scoring still checks each operation deadline.
    pub fn open_private_compact_answer(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        overall_deadline: Instant,
        calibration_base: Option<Profile>,
    ) -> Result<(Self, CompactBoundedHybridAnswerProvenance)> {
        match AnswerRecipe::for_profile(&profile, reviewed_graph_sha)? {
            AnswerRecipe::OriginalFp32 => {
                return Self::open_compact_bounded_hybrid_answer_variant(
                    profile,
                    source,
                    reviewed_graph_result,
                    reviewed_graph_sha,
                    overall_deadline,
                    calibration_base,
                );
            }
            AnswerRecipe::LargeBatchView => {
                return Self::open_private_large_answer(
                    profile,
                    source,
                    reviewed_graph_result,
                    reviewed_graph_sha,
                    overall_deadline,
                    calibration_base,
                );
            }
            AnswerRecipe::ProtectedMha => {
                ensure!(
                    calibration_base.is_none() && reviewed_graph_result.is_empty(),
                    "the protected MHA Answer recipe admits no graph result or calibration variant"
                );
                return Self::open_private_protected_mha_answer(
                    profile,
                    source,
                    reviewed_graph_sha,
                    overall_deadline,
                );
            }
            AnswerRecipe::ProtectedQkv => ensure!(
                calibration_base.is_none(),
                "the protected QKV Answer recipe admits no calibration-only variant"
            ),
        }
        let policy = ResourcePolicy::focus_static1024();
        let description = profile.description();
        ensure!(
            description.max_tokens == policy.context_input_tokens
                && description.max_candidates == policy.maximum_context_candidates
                && description.tasks.contains(&Task::ContextAction),
            "protected QKV Answer requires the original Context action limits"
        );
        let maximum_operation = Duration::from_millis(policy.deadline_ms);
        let load_deadline = profile
            .gpu
            .as_ref()
            .context("CUDA Answer binding missing")?
            .load_deadline;
        require_answer_deadline(
            Instant::now(),
            load_deadline,
            overall_deadline,
            maximum_operation,
        )?;
        let mut scorer = Self::open_private_compact_fp32_qkv_unprofiled(
            profile,
            source,
            reviewed_graph_result,
            reviewed_graph_sha,
        )?;
        let mut evidence = scorer
            .bounded_hybrid
            .take()
            .context("same-session QKV evidence missing")?;
        evidence.policy_identity = framed_digest(
            b"airk.private.compact-fp16-qkv-answer/1",
            &[
                evidence.policy_identity.as_bytes(),
                policy.digest()?.as_bytes(),
            ],
        );
        cuda::in_time(load_deadline)?;
        Ok((
            scorer,
            CompactBoundedHybridAnswerProvenance {
                evidence,
                overall_deadline,
                maximum_operation,
            },
        ))
    }

    /// Opens the unprofiled corrected Large FP32 session for the private Answer
    /// actor through the reviewed Large resource route, under the same Answer
    /// limits and original deadline checks as Compact.
    fn open_private_large_answer(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        overall_deadline: Instant,
        calibration_base: Option<Profile>,
    ) -> Result<(Self, CompactBoundedHybridAnswerProvenance)> {
        let policy = ResourcePolicy::software_v2();
        let description = profile.description();
        ensure!(
            description.max_tokens == policy.context_input_tokens
                && description.max_candidates == policy.maximum_context_candidates
                && description.tasks.contains(&Task::ContextAction),
            "Large Answer requires the original Context action limits"
        );
        let maximum_operation = Duration::from_millis(policy.deadline_ms);
        let load_deadline = profile
            .gpu
            .as_ref()
            .context("Large CUDA Answer binding missing")?
            .load_deadline;
        require_answer_deadline(
            Instant::now(),
            load_deadline,
            overall_deadline,
            maximum_operation,
        )?;
        let opened = Self::open_large_bounded_hybrid_resource_variant(
            profile,
            source,
            reviewed_graph_result,
            reviewed_graph_sha,
            overall_deadline,
            calibration_base,
        )?;
        let BoundedHybridScorer {
            scorer,
            mut evidence,
            ..
        } = opened;
        evidence.policy_identity = framed_digest(
            b"airk.private.large-cuda-batch-view-answer/1",
            &[
                evidence.policy_identity.as_bytes(),
                policy.digest()?.as_bytes(),
            ],
        );
        cuda::in_time(load_deadline)?;
        Ok((
            scorer,
            CompactBoundedHybridAnswerProvenance {
                evidence,
                overall_deadline,
                maximum_operation,
            },
        ))
    }

    /// Opens a protected efficient-MHA Context profile through plain execution under the
    /// evaluation admission. `policy` must equal the profile's bounded policy digest,
    /// which the release verifier has matched to the live session before this returns.
    fn open_private_protected_mha_answer(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        policy: Sha256Digest,
        overall_deadline: Instant,
    ) -> Result<(Self, CompactBoundedHybridAnswerProvenance)> {
        let resource = ResourcePolicy::focus_static1024();
        let description = profile.description();
        ensure!(
            description.max_tokens == resource.context_input_tokens
                && description.max_candidates == resource.maximum_context_candidates
                && description.tasks.contains(&Task::ContextAction)
                && matches!(
                    description.device_execution.as_ref().map(|d| &d.placement),
                    Some(airk_core::arbiter::device::PlacementPolicy::BoundedHybrid { policy: p }) if *p == policy
                ),
            "protected MHA Answer requires the original Context limits and its own policy pin"
        );
        let maximum_operation = Duration::from_millis(resource.deadline_ms);
        let load_deadline = profile
            .gpu
            .as_ref()
            .context("protected MHA CUDA binding missing")?
            .load_deadline;
        require_answer_deadline(
            Instant::now(),
            load_deadline,
            overall_deadline,
            maximum_operation,
        )?;
        let mut scorer = Self::open(profile, source)?;
        let mut evidence = scorer
            .bounded_hybrid
            .take()
            .context("verified protected MHA session evidence missing")?;
        ensure!(
            evidence.policy_identity == policy,
            "verified protected MHA policy differs"
        );
        evidence.policy_identity = framed_digest(
            b"airk.private.cuda-protected-mha-answer/1",
            &[
                evidence.policy_identity.as_bytes(),
                resource.digest()?.as_bytes(),
            ],
        );
        cuda::in_time(load_deadline)?;
        Ok((
            scorer,
            CompactBoundedHybridAnswerProvenance {
                evidence,
                overall_deadline,
                maximum_operation,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_answer_reservation_requires_matching_qkv_graph_and_receipt() -> Result<()> {
        use airk_core::arbiter::device::DevicePrecision::{Float16, Float32};
        let graph = "e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14".parse()?;
        let receipt = "42a273598b5c8678535691426ed725041a5d9565ff1fbaf535a43938894f07a4".parse()?;
        let other = Sha256Digest::from_bytes([0; 32]);
        let qkv = AnswerRecipe::identify(Float16, graph, receipt)?.capacity()?;
        assert_eq!(
            (
                qkv.cold_peak_bytes(),
                qkv.warm_extra_bytes(),
                qkv.headroom_bytes()
            ),
            (16 << 30, 8 << 30, 2 << 30)
        );
        let original = AnswerRecipe::identify(Float32, other, other)?.capacity()?;
        assert_eq!(
            (
                original.cold_peak_bytes(),
                original.warm_extra_bytes(),
                original.headroom_bytes()
            ),
            (22 << 30, 16 << 30, 2 << 30)
        );
        assert!(AnswerRecipe::identify(Float16, other, receipt).is_err());
        assert!(AnswerRecipe::identify(Float16, graph, other).is_err());
        assert!(AnswerRecipe::identify(Float32, graph, receipt).is_err());
        Ok(())
    }

    #[test]
    fn private_large_answer_requires_the_reviewed_batch_view_graph_and_placement() -> Result<()> {
        use airk_core::arbiter::device::DevicePrecision::{Float16, Float32};
        // Literal pins of the corrected Large FP32 graph and its retained placement result.
        let graph = "5c9a26693eec8522bbfb4d5c4445fa0b60684449a5ea2e1098ec71cca96405b1".parse()?;
        let placement =
            "d4ccefff117a11c68b92c3f9c08b145a4fbb5a470114af5a23107c73a26bc431".parse()?;
        let other = Sha256Digest::from_bytes([0; 32]);
        let large = AnswerRecipe::identify_large(Float32, graph, placement)?;
        assert_eq!(large, AnswerRecipe::LargeBatchView);
        let plan = large.capacity()?;
        assert_eq!(
            (
                plan.cold_peak_bytes(),
                plan.warm_extra_bytes(),
                plan.headroom_bytes()
            ),
            (40 << 30, 8 << 30, 4 << 30)
        );
        assert!(AnswerRecipe::identify_large(Float16, graph, placement).is_err());
        assert!(AnswerRecipe::identify_large(Float32, other, placement).is_err());
        assert!(AnswerRecipe::identify_large(Float32, graph, other).is_err());
        // The original unfused Large graph and its placement are not admitted.
        let original =
            "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94".parse()?;
        let original_placement =
            "1892e5b040d8c51969f1d3361d0929950fa98f0ef4079060e48d00458635488a".parse()?;
        assert!(AnswerRecipe::identify_large(Float32, original, original_placement).is_err());
        Ok(())
    }

    #[test]
    fn protected_mha_answer_capacity_is_the_profile_measured_memory() -> Result<()> {
        use airk_core::arbiter::device::{
            DEVICE_EXECUTION_SCHEMA, DeviceExecution, DeviceMemory, DevicePrecision, GpuBackend,
            PlacementPolicy,
        };
        let mut profile = crate::arbiter::runtime::catalog_profiles()?
            .into_iter()
            .find(|p| p.model == ModelChoice::ContextQwenCompact && p.cpu_fit.is_some())
            .context("Compact fixture profile missing")?;
        assert!(AnswerRecipe::ProtectedMha.capacity_for(&profile).is_err());
        profile.device_execution = Some(DeviceExecution {
            schema: DEVICE_EXECUTION_SCHEMA.into(),
            backend: GpuBackend::Cuda,
            runtime_build: Sha256Digest::from_bytes([1; 32]),
            compatibility_policy: Sha256Digest::from_bytes([2; 32]),
            provider_options: Sha256Digest::from_bytes([3; 32]),
            math_policy: Sha256Digest::from_bytes([4; 32]),
            precision: DevicePrecision::Float16,
            tf32: false,
            placement: PlacementPolicy::BoundedHybrid {
                policy: Sha256Digest::from_bytes([5; 32]),
            },
            graph: Sha256Digest::from_bytes([6; 32]),
            host_fit: profile.cpu_fit.clone().context("fit")?,
            memory: DeviceMemory::Dedicated {
                required_peak_bytes: 8 << 30,
                headroom_bytes: 2 << 30,
            },
        });
        let plan = AnswerRecipe::ProtectedMha.capacity_for(&profile)?;
        assert_eq!(
            (plan.cold_peak_bytes(), plan.warm_extra_bytes(), plan.headroom_bytes()),
            (8 << 30, 8 << 30, 2 << 30)
        );
        assert!(AnswerRecipe::ProtectedMha.capacity().is_err());
        // The literal recipes keep their reviewed tuples through capacity_for.
        let qkv = AnswerRecipe::ProtectedQkv.capacity_for(&profile)?;
        assert_eq!(qkv.cold_peak_bytes(), 16 << 30);
        Ok(())
    }
}