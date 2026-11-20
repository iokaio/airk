// SPDX-License-Identifier: Apache-2.0
//! Scoped Answer dispatch for the reviewed private Compact CUDA recipes.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnswerRecipe {
    OriginalFp32,
    ProtectedQkv,
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
        ensure!(
            description.model == ModelChoice::ContextQwenCompact,
            "private CUDA Answer requires Compact Context"
        );
        let device = description
            .device_execution
            .as_ref()
            .context("CUDA Answer profile missing")?;
        Self::identify(device.precision, device.graph, receipt)
    }

    fn capacity(self) -> Result<cuda::DeviceCapacityPlan> {
        match self {
            Self::OriginalFp32 => cuda::DeviceCapacityPlan::probe(22 << 30, 16 << 30, 2 << 30),
            Self::ProtectedQkv => cuda::DeviceCapacityPlan::probe(16 << 30, 8 << 30, 2 << 30),
        }
    }
}

impl Scorer {
    /// Selects the reviewed phase reservation before the private service probes a device.
    pub fn private_compact_answer_capacity(
        profile: &ExecutionProfile,
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<cuda::DeviceCapacityPlan> {
        AnswerRecipe::for_profile(profile, reviewed_graph_sha)?.capacity()
    }

    /// Opens the exact private Answer graph and retains this session's observed placement
    /// with its original service lease. Normal scoring still checks each operation deadline.
    pub fn open_private_compact_answer(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        overall_deadline: Instant,
    ) -> Result<(Self, CompactBoundedHybridAnswerProvenance)> {
        if AnswerRecipe::for_profile(&profile, reviewed_graph_sha)? == AnswerRecipe::OriginalFp32 {
            return Self::open_compact_bounded_hybrid_answer(
                profile,
                source,
                reviewed_graph_result,
                reviewed_graph_sha,
                overall_deadline,
            );
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
}
