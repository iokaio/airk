// SPDX-License-Identifier: Apache-2.0
//! Scoped Answer dispatch for the reviewed private Arbiter (Compact) CUDA recipes.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnswerRecipe {
    OriginalFp32,
    ProtectedQkv,
    /// A protected efficient-MHA FP16 graph (Compact 147be075)
    /// under the evaluation-only bounded admission; its own bounded policy digest is
    /// the review pin and its capacity is the profile's measured device memory.
    ProtectedMha,
}

impl AnswerRecipe {
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
            _ => anyhow::bail!("private CUDA Answer requires the Arbiter Context model"),
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
        }
    }
}

/// The protected MHA Answer admission: the original Context limits, a
/// Context task, and this profile's own bounded policy pin.
fn protected_mha_admits(
    description: &Profile,
    resource: &ResourcePolicy,
    policy: Sha256Digest,
) -> bool {
    description.max_tokens == resource.context_input_tokens
        && description.max_candidates == resource.maximum_context_candidates
        && declares_context_task(description)
        && matches!(
            description.device_execution.as_ref().map(|d| &d.placement),
            Some(airk_core::arbiter::device::PlacementPolicy::BoundedHybrid { policy: p }) if *p == policy
        )
}

impl Scorer {
    /// Verifies the protected recipe of an unqualified loop-calibration base before
    /// the private fixture attaches it. This check grants no open or GPU authority.
    #[cfg(all(feature = "test-support", windows, target_arch = "x86_64"))]
    pub fn validate_private_protected_mha_calibration_base(base: &Profile) -> Result<()> {
        ensure!(
            base.model == ModelChoice::ContextQwenCompact
                && crate::arbiter::runtime::loop_tasks::declares_card_tasks(base)
                && cuda::evaluation_admits(base, true)?,
            "private loop calibration requires the reviewed Protected MHA Compact route"
        );
        Ok(())
    }

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
            AnswerRecipe::ProtectedMha => {
                ensure!(
                    reviewed_graph_result.is_empty(),
                    "the protected MHA Answer recipe admits no external graph result"
                );
                if let Some(base) = calibration_base.as_ref() {
                    ensure!(
                        crate::arbiter::runtime::loop_tasks::declares_card_tasks(base),
                        "the protected MHA calibration variant requires the loop card tasks"
                    );
                    require_calibration_only_variant(base, profile.description())?;
                }
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
                && declares_context_task(description),
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
            protected_mha_admits(description, &resource, policy),
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

    /// Change log entry 51: the service refused to warm an FP16 loop profile
    /// ("protected MHA Answer requires the original Context limits and its
    /// own policy pin") because the check asked for the one-turn task. A
    /// loop profile is admitted; the limits (8,192 tokens, 8 candidates) and
    /// the policy pin stay as strict, and a profile with neither task set is
    /// still refused.
    #[test]
    fn protected_mha_answer_admits_a_loop_profile_under_the_same_limits_and_pin() -> Result<()> {
        use airk_core::arbiter::device::{
            DEVICE_EXECUTION_SCHEMA, DeviceExecution, DeviceMemory, DevicePrecision, GpuBackend,
            PlacementPolicy,
        };
        let pin = Sha256Digest::from_bytes([5; 32]);
        let mut one_turn = crate::arbiter::runtime::catalog_profiles()?
            .into_iter()
            .find(|p| p.model == ModelChoice::ContextQwenCompact && p.cpu_fit.is_some())
            .context("Compact fixture profile missing")?;
        one_turn.max_tokens = 8192;
        one_turn.max_candidates = 8;
        one_turn.device_execution = Some(DeviceExecution {
            schema: DEVICE_EXECUTION_SCHEMA.into(),
            backend: GpuBackend::Cuda,
            runtime_build: Sha256Digest::from_bytes([1; 32]),
            compatibility_policy: Sha256Digest::from_bytes([2; 32]),
            provider_options: Sha256Digest::from_bytes([3; 32]),
            math_policy: Sha256Digest::from_bytes([4; 32]),
            precision: DevicePrecision::Float16,
            tf32: false,
            placement: PlacementPolicy::BoundedHybrid { policy: pin },
            graph: Sha256Digest::from_bytes([6; 32]),
            host_fit: one_turn.cpu_fit.clone().context("fit")?,
            memory: DeviceMemory::Dedicated {
                required_peak_bytes: 8 << 30,
                headroom_bytes: 2 << 30,
            },
        });
        assert_eq!(one_turn.tasks, BTreeSet::from([Task::ContextAction]));
        let mut looped = one_turn.clone();
        crate::arbiter::runtime::admit_loop_tasks(&mut looped)?;
        assert_eq!(looped.tasks.len(), 4);
        assert!(!looped.tasks.contains(&Task::ContextAction));
        let resource = ResourcePolicy::focus_static1024();
        assert!(protected_mha_admits(&one_turn, &resource, pin), "one-turn");
        assert!(protected_mha_admits(&looped, &resource, pin), "loop");
        // Still refused: another pin, other limits, no Context task.
        let other = Sha256Digest::from_bytes([7; 32]);
        assert!(!protected_mha_admits(&looped, &resource, other), "wrong pin");
        let mut short = looped.clone();
        short.max_tokens = 2048;
        assert!(!protected_mha_admits(&short, &resource, pin), "2,048 tokens");
        let mut wide = looped.clone();
        wide.max_candidates = 16;
        assert!(!protected_mha_admits(&wide, &resource, pin), "16 candidates");
        let mut unpinned = looped.clone();
        unpinned.device_execution.as_mut().context("device")?.placement =
            PlacementPolicy::StrictGpu;
        assert!(!protected_mha_admits(&unpinned, &resource, pin), "no bounded pin");
        for tasks in [vec![], vec![Task::FocusRank]] {
            let mut neither = looped.clone();
            neither.tasks = tasks.iter().copied().collect();
            assert!(!protected_mha_admits(&neither, &resource, pin), "{tasks:?}");
            assert!(!declares_context_task(&neither));
        }
        assert!(declares_context_task(&one_turn) && declares_context_task(&looped));
        Ok(())
    }
}