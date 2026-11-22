// SPDX-License-Identifier: Apache-2.0
//! The Context ladder of plan section 12.5: Large on the GPU, then Compact on
//! the GPU, then Compact on the CPU. Large never runs on the CPU, in production
//! or as a fallback. Each step down carries its typed reasons.
//!
//! Advisory only, like [`select`](super::select): it reads two finished
//! per-model selections and grants nothing. The caller rechecks consent,
//! qualification, capacity, epoch and the original deadline before dispatch.

use super::*;

/// The rung a Context selection landed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextRung {
    LargeGpu,
    CompactGpu,
    CompactCpu,
    /// No rung is runnable: Answer runs the no-model loop.
    None,
}

/// The ladder's result: the rung, the chosen model's selection (with its own
/// variant) and every reason collected on the way down, in order.
#[derive(Clone, Debug)]
pub struct LadderOutcome {
    pub rung: ContextRung,
    pub selected: Option<SelectedVariant>,
    pub model: Option<SelectedModel>,
    pub reasons: Vec<CandidateRejection>,
}

fn global(reason: SelectionReason) -> CandidateRejection {
    CandidateRejection {
        manifest: None,
        adapter_key: None,
        reason,
    }
}

/// Walks the ladder. `large` is the Large selection when the user asked for
/// Large (None when Compact was asked for, so the ladder starts at Compact);
/// `compact` is the Compact selection.
///
/// - A Large selection on a GPU is taken.
/// - A Large selection on the CPU is refused with `NoQualifiedGpuRoute`: Large
///   never runs on the CPU.
/// - Otherwise Compact is taken, on the GPU first by its own order, with a
///   `SmallerModelSelected` row when Large was asked for.
/// - With neither, the outcome is `None` and keeps every reason.
///
/// At most [`MAX_SELECTION_REASONS`] reasons are kept; the closing row of a
/// step down always survives.
pub fn context_ladder(
    large: Option<&DeviceSelection>,
    compact: &DeviceSelection,
) -> Result<LadderOutcome> {
    ensure!(
        compact.model == SelectedModel::ContextCompact
            && large.is_none_or(|large| large.model == SelectedModel::ContextLarge),
        "Context ladder requires the Large and Compact selections"
    );
    let mut reasons = Vec::new();
    if let Some(large) = large {
        reasons.extend(large.reasons.iter().cloned());
        match &large.selected {
            Some(variant) if variant.backend.is_some() => {
                return Ok(LadderOutcome {
                    rung: ContextRung::LargeGpu,
                    selected: Some(variant.clone()),
                    model: Some(SelectedModel::ContextLarge),
                    reasons: bounded(reasons),
                });
            }
            Some(variant) => reasons.push(CandidateRejection {
                manifest: Some(variant.manifest),
                adapter_key: Some(variant.adapter_key.clone()),
                reason: SelectionReason::NoQualifiedGpuRoute,
            }),
            None => {}
        }
        reasons.push(global(SelectionReason::SmallerModelSelected));
    }
    reasons.extend(compact.reasons.iter().cloned());
    let rung = match &compact.selected {
        Some(variant) if variant.backend.is_some() => ContextRung::CompactGpu,
        Some(_) => ContextRung::CompactCpu,
        None => ContextRung::None,
    };
    Ok(LadderOutcome {
        rung,
        model: compact.selected.as_ref().map(|_| SelectedModel::ContextCompact),
        selected: compact.selected.clone(),
        reasons: bounded(reasons),
    })
}

/// Keeps the first reasons and the step-down row within the status limit.
fn bounded(mut reasons: Vec<CandidateRejection>) -> Vec<CandidateRejection> {
    if reasons.len() <= MAX_SELECTION_REASONS {
        return reasons;
    }
    let step = reasons
        .iter()
        .position(|row| row.reason == SelectionReason::SmallerModelSelected);
    let mut kept: Vec<_> = reasons.drain(..MAX_SELECTION_REASONS - 1).collect();
    match step {
        Some(index) if index >= MAX_SELECTION_REASONS - 1 => {
            kept.push(global(SelectionReason::SmallerModelSelected));
        }
        _ => kept.push(reasons.remove(0)),
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: u8) -> Sha256Digest {
        Sha256Digest::from_bytes([byte; 32])
    }
    fn variant(byte: u8, gpu: bool) -> SelectedVariant {
        SelectedVariant {
            manifest: digest(byte),
            execution_identity: format!("variant-{byte}"),
            backend: gpu.then_some(GpuBackend::Cuda),
            precision: gpu.then_some(DevicePrecision::Float16),
            adapter_key: if gpu { "gpu-0".into() } else { "cpu".into() },
            capability_epoch: 1,
        }
    }
    fn selection(
        model: SelectedModel,
        selected: Option<SelectedVariant>,
        reasons: Vec<CandidateRejection>,
    ) -> DeviceSelection {
        DeviceSelection {
            schema: DEVICE_STATUS_SCHEMA.into(),
            model,
            policy: DevicePolicy::Auto,
            gpu_precision: GpuPrecisionPreference::Auto,
            selected,
            reasons,
            operation_failure: None,
        }
    }
    fn row(byte: u8, reason: SelectionReason) -> CandidateRejection {
        CandidateRejection {
            manifest: Some(digest(byte)),
            adapter_key: Some("gpu-0".into()),
            reason,
        }
    }

    #[test]
    fn large_on_the_gpu_is_the_top_rung() -> Result<()> {
        let large = selection(SelectedModel::ContextLarge, Some(variant(1, true)), vec![]);
        let compact = selection(SelectedModel::ContextCompact, Some(variant(2, true)), vec![]);
        let outcome = context_ladder(Some(&large), &compact)?;
        assert_eq!(outcome.rung, ContextRung::LargeGpu);
        assert_eq!(outcome.model, Some(SelectedModel::ContextLarge));
        assert_eq!(outcome.selected.map(|v| v.manifest), Some(digest(1)));
        assert!(outcome.reasons.is_empty());
        Ok(())
    }

    #[test]
    fn large_without_gpu_steps_down_to_compact_gpu_with_typed_reasons() -> Result<()> {
        let large = selection(
            SelectedModel::ContextLarge,
            None,
            vec![row(1, SelectionReason::DeviceMemoryInsufficient)],
        );
        let compact = selection(SelectedModel::ContextCompact, Some(variant(2, true)), vec![]);
        let outcome = context_ladder(Some(&large), &compact)?;
        assert_eq!(outcome.rung, ContextRung::CompactGpu);
        assert_eq!(outcome.model, Some(SelectedModel::ContextCompact));
        assert_eq!(
            outcome.reasons.iter().map(|r| r.reason).collect::<Vec<_>>(),
            vec![
                SelectionReason::DeviceMemoryInsufficient,
                SelectionReason::SmallerModelSelected
            ]
        );
        Ok(())
    }

    #[test]
    fn large_never_runs_on_the_cpu() -> Result<()> {
        let large = selection(SelectedModel::ContextLarge, Some(variant(1, false)), vec![]);
        let compact = selection(
            SelectedModel::ContextCompact,
            Some(variant(3, false)),
            vec![row(2, SelectionReason::GpuNotOptedIn)],
        );
        let outcome = context_ladder(Some(&large), &compact)?;
        assert_eq!(outcome.rung, ContextRung::CompactCpu);
        assert_eq!(outcome.selected.map(|v| v.manifest), Some(digest(3)));
        assert_eq!(
            outcome.reasons,
            vec![
                CandidateRejection {
                    manifest: Some(digest(1)),
                    adapter_key: Some("cpu".into()),
                    reason: SelectionReason::NoQualifiedGpuRoute,
                },
                global(SelectionReason::SmallerModelSelected),
                row(2, SelectionReason::GpuNotOptedIn),
            ]
        );
        Ok(())
    }

    #[test]
    fn compact_requested_starts_at_compact_without_a_step_down_row() -> Result<()> {
        let compact = selection(SelectedModel::ContextCompact, Some(variant(2, true)), vec![]);
        let outcome = context_ladder(None, &compact)?;
        assert_eq!(outcome.rung, ContextRung::CompactGpu);
        assert!(outcome.reasons.is_empty());
        let none = selection(
            SelectedModel::ContextCompact,
            None,
            vec![global(SelectionReason::NoQualifiedCpu)],
        );
        let outcome = context_ladder(None, &none)?;
        assert_eq!(outcome.rung, ContextRung::None);
        assert_eq!(outcome.model, None);
        assert_eq!(outcome.reasons, vec![global(SelectionReason::NoQualifiedCpu)]);
        Ok(())
    }

    #[test]
    fn the_step_down_row_survives_the_reason_limit() -> Result<()> {
        let many: Vec<_> = (0..MAX_SELECTION_REASONS as u8)
            .map(|byte| row(byte, SelectionReason::AllocationFailed))
            .collect();
        let large = selection(SelectedModel::ContextLarge, None, many);
        let compact = selection(SelectedModel::ContextCompact, Some(variant(40, true)), vec![]);
        let outcome = context_ladder(Some(&large), &compact)?;
        assert_eq!(outcome.reasons.len(), MAX_SELECTION_REASONS);
        assert_eq!(
            outcome.reasons.last().map(|r| r.reason),
            Some(SelectionReason::SmallerModelSelected)
        );
        Ok(())
    }

    #[test]
    fn mismatched_models_are_refused() {
        let compact = selection(SelectedModel::ContextCompact, None, vec![]);
        let focus = selection(SelectedModel::Focus, None, vec![]);
        assert!(context_ladder(Some(&compact), &compact).is_err());
        assert!(context_ladder(None, &focus).is_err());
    }
}
