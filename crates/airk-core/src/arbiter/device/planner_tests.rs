// SPDX-License-Identifier: Apache-2.0
//! Literal tables for the Auto route order. Every expected index and reason is
//! written by hand from the contract tables, never computed by the planner.
use super::*;

const MODEL: SelectedModel = SelectedModel::Arbiter;
/// A Fast tier (2,048 tokens) and a Full tier (8,192 tokens) of one model.
const FAST: usize = 2048;
const FULL: usize = 8192;

fn id(value: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([value; 32])
}
fn cpu(manifest: u8, max_tokens: usize, cores: usize) -> AdvisoryCandidate {
    AdvisoryCandidate {
        manifest: id(manifest),
        model: MODEL,
        execution_identity: "literal-cpu-execution".into(),
        backend: None,
        precision: None,
        adapter_key: Some("cpu".into()),
        vendor: None,
        capability_epoch: 1,
        max_tokens,
        cpu_cores: cores,
        rejection: None,
    }
}
fn gpu(manifest: u8, adapter: &str, vendor: AdapterVendor) -> AdvisoryCandidate {
    AdvisoryCandidate {
        manifest: id(manifest),
        model: MODEL,
        execution_identity: "literal-cuda-execution".into(),
        backend: Some(GpuBackend::Cuda),
        precision: Some(DevicePrecision::Float16),
        adapter_key: Some(adapter.into()),
        vendor: Some(vendor),
        capability_epoch: 1,
        max_tokens: FULL,
        cpu_cores: 1,
        rejection: None,
    }
}
/// A GPU variant that was never bound to an adapter.
fn unbound(manifest: u8, rejection: Option<SelectionReason>) -> AdvisoryCandidate {
    let mut value = gpu(manifest, "unused", AdapterVendor::Nvidia);
    value.adapter_key = None;
    value.vendor = None;
    value.rejection = rejection;
    value
}
fn rejected(mut value: AdvisoryCandidate, reason: SelectionReason) -> AdvisoryCandidate {
    value.rejection = Some(reason);
    value
}
fn global(reason: SelectionReason) -> CandidateRejection {
    CandidateRejection {
        manifest: None,
        adapter_key: None,
        reason,
    }
}
fn row(manifest: u8, adapter: Option<&str>, reason: SelectionReason) -> CandidateRejection {
    CandidateRejection {
        manifest: Some(id(manifest)),
        adapter_key: adapter.map(str::to_owned),
        reason,
    }
}
fn plan(
    policy: DevicePolicy,
    precision: GpuPrecisionPreference,
    gpu_use: GpuUse,
    remaining_us: u64,
    candidates: &[AdvisoryCandidate],
) -> Result<AttemptPlan> {
    attempt_order(MODEL, policy, precision, gpu_use, remaining_us, candidates)
}
/// One GPU variant on an NVIDIA adapter, then a Fast and a Full CPU tier.
fn roster(vendor: AdapterVendor) -> [AdvisoryCandidate; 3] {
    [
        gpu(1, "adapter-a", vendor),
        cpu(2, FAST, 8),
        cpu(3, FULL, 8),
    ]
}

#[test]
fn order_table_gpu_off_or_policy_cpu_takes_the_cpu_ladder() -> Result<()> {
    let rows = roster(AdapterVendor::Nvidia);
    for (policy, gpu_use, reason) in [
        (
            DevicePolicy::Auto,
            GpuUse::NotOptedIn,
            SelectionReason::GpuNotOptedIn,
        ),
        (
            DevicePolicy::Cpu,
            GpuUse::OptedIn,
            SelectionReason::ForcedCpu,
        ),
        (
            DevicePolicy::Cpu,
            GpuUse::NotOptedIn,
            SelectionReason::ForcedCpu,
        ),
        (
            DevicePolicy::Cpu,
            GpuUse::NoAdapter,
            SelectionReason::ForcedCpu,
        ),
    ] {
        let value = plan(policy, GpuPrecisionPreference::Auto, gpu_use, 1, &rows)?;
        assert_eq!(value.order, [2, 1], "{policy:?}/{gpu_use:?}");
        assert_eq!(value.reasons, [global(reason)], "{policy:?}/{gpu_use:?}");
        assert_eq!(value.operation_failure, None);
        assert_eq!(value.no_fit, None);
    }
    Ok(())
}

#[test]
fn order_table_gpu_on_nvidia_takes_cuda_then_the_ladder() -> Result<()> {
    let rows = [
        cpu(2, FAST, 8),
        gpu(1, "adapter-a", AdapterVendor::Nvidia),
        cpu(3, FULL, 8),
    ];
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        1,
        &rows,
    )?;
    assert_eq!(value.order, [1, 2, 0]);
    assert_eq!(value.reasons, [] as [CandidateRejection; 0]);
    assert_eq!(value.operation_failure, None);
    assert_eq!(value.no_fit, None);
    Ok(())
}

#[test]
fn order_table_gpu_on_other_adapter_reports_no_qualified_route_and_takes_the_ladder() -> Result<()>
{
    // A CUDA variant observed on an adapter that is not NVIDIA has no route.
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        1,
        &roster(AdapterVendor::Other),
    )?;
    assert_eq!(value.order, [2, 1]);
    assert_eq!(
        value.reasons,
        [row(
            1,
            Some("adapter-a"),
            SelectionReason::NoQualifiedGpuRoute
        )]
    );
    assert_eq!(value.no_fit, None);
    // GPU use is on and the model has no GPU candidate at all.
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        1,
        &[cpu(2, FAST, 8), cpu(3, FULL, 8)],
    )?;
    assert_eq!(value.order, [1, 0]);
    assert_eq!(
        value.reasons,
        [global(SelectionReason::NoQualifiedGpuRoute)]
    );
    // Neither DirectML nor Core ML is a qualified route for any vendor.
    for backend in [GpuBackend::DirectMl, GpuBackend::CoreMl] {
        for vendor in [AdapterVendor::Nvidia, AdapterVendor::Other] {
            let mut other = gpu(1, "adapter-a", vendor);
            other.backend = Some(backend);
            let value = plan(
                DevicePolicy::Auto,
                GpuPrecisionPreference::Auto,
                GpuUse::OptedIn,
                1,
                &[other, cpu(2, FAST, 8)],
            )?;
            assert_eq!(value.order, [1]);
            assert_eq!(
                value.reasons,
                [row(
                    1,
                    Some("adapter-a"),
                    SelectionReason::NoQualifiedGpuRoute
                )]
            );
        }
    }
    Ok(())
}

#[test]
fn order_table_gpu_on_no_adapter_reports_no_gpu_and_takes_the_ladder() -> Result<()> {
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::NoAdapter,
        1,
        &[cpu(2, FAST, 8), cpu(3, FULL, 8)],
    )?;
    assert_eq!(value.order, [1, 0]);
    assert_eq!(value.reasons, [global(SelectionReason::NoGpu)]);
    assert_eq!(value.no_fit, None);
    // A variant that was never bound keeps its own row beside no_gpu.
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Fp32,
        GpuUse::NoAdapter,
        1,
        &[unbound(1, None), cpu(3, FULL, 8)],
    )?;
    assert_eq!(value.order, [1]);
    assert_eq!(
        value.reasons,
        [
            global(SelectionReason::NoGpu),
            row(1, None, SelectionReason::GpuPrecisionExcluded),
        ]
    );
    // A bound GPU candidate contradicts an enumeration that found no adapter.
    assert!(
        plan(
            DevicePolicy::Auto,
            GpuPrecisionPreference::Auto,
            GpuUse::NoAdapter,
            1,
            &roster(AdapterVendor::Nvidia),
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn cpu_ladder_orders_by_larger_token_budget_then_larger_cpu_class_not_by_manifest() -> Result<()> {
    // Manifest order is 1, 2, 3, 4. The ladder order is Full/8, Full/4, Fast/8, Fast/4.
    let rows = [
        cpu(1, FAST, 4),
        cpu(2, FULL, 4),
        cpu(3, FAST, 8),
        cpu(4, FULL, 8),
    ];
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::NotOptedIn,
        1,
        &rows,
    )?;
    assert_eq!(value.order, [3, 1, 2, 0]);
    // Two tiers that differ only by manifest keep a deterministic order.
    let rows = [cpu(9, FULL, 8), cpu(4, FULL, 8)];
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::NotOptedIn,
        1,
        &rows,
    )?;
    assert_eq!(value.order, [1, 0]);
    Ok(())
}

#[test]
fn core_count_below_a_class_refuses_that_tier_and_the_next_tier_is_tried() -> Result<()> {
    // A 4-core host: the caller marks both 8-core tiers, the 4-core tiers stay.
    let rows = [
        rejected(cpu(1, FULL, 8), SelectionReason::CpuFitInsufficient),
        cpu(2, FULL, 4),
        rejected(cpu(3, FAST, 8), SelectionReason::CpuFitInsufficient),
        cpu(4, FAST, 4),
    ];
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::NotOptedIn,
        1,
        &rows,
    )?;
    assert_eq!(value.order, [1, 3]);
    assert_eq!(
        value.reasons,
        [
            global(SelectionReason::GpuNotOptedIn),
            row(1, Some("cpu"), SelectionReason::CpuFitInsufficient),
            row(3, Some("cpu"), SelectionReason::CpuFitInsufficient),
        ]
    );
    assert_eq!(value.no_fit, None);
    // A 2-core host: no tier is left. Each refused tier keeps its own row, so
    // the order needs no closing row.
    let rows = [
        rejected(cpu(1, FULL, 8), SelectionReason::CpuFitInsufficient),
        rejected(cpu(2, FAST, 4), SelectionReason::CpuFitInsufficient),
    ];
    let value = plan(
        DevicePolicy::Cpu,
        GpuPrecisionPreference::Auto,
        GpuUse::NotOptedIn,
        1,
        &rows,
    )?;
    assert_eq!(value.order, [] as [usize; 0]);
    assert_eq!(
        value.reasons,
        [
            global(SelectionReason::ForcedCpu),
            row(1, Some("cpu"), SelectionReason::CpuFitInsufficient),
            row(2, Some("cpu"), SelectionReason::CpuFitInsufficient),
        ]
    );
    assert_eq!(value.no_fit, None);
    assert_eq!(value.operation_failure, None);
    Ok(())
}

#[test]
fn reason_table_provider_and_memory_causes_keep_their_exact_reason_and_fall_to_the_ladder()
-> Result<()> {
    for reason in [
        SelectionReason::RuntimeUnavailable,
        SelectionReason::ProviderInitializationFailed,
        SelectionReason::DriverUnsupported,
        SelectionReason::DeviceMemoryInsufficient,
        SelectionReason::DeviceMemoryUnknown,
        SelectionReason::HostMemoryInsufficient,
        SelectionReason::HostMemoryUnknown,
    ] {
        let rows = [
            rejected(gpu(1, "adapter-a", AdapterVendor::Nvidia), reason),
            cpu(2, FAST, 8),
            cpu(3, FULL, 8),
        ];
        let value = plan(
            DevicePolicy::Auto,
            GpuPrecisionPreference::Auto,
            GpuUse::OptedIn,
            1,
            &rows,
        )?;
        assert_eq!(value.order, [2, 1], "{reason:?}");
        assert_eq!(value.reasons, [row(1, Some("adapter-a"), reason)]);
        assert_eq!(value.operation_failure, None);
    }
    // A variant refused before any adapter was bound keeps an absent adapter key.
    let rows = [
        unbound(1, Some(SelectionReason::RuntimeUnavailable)),
        cpu(2, FULL, 8),
    ];
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        1,
        &rows,
    )?;
    assert_eq!(value.order, [1]);
    assert_eq!(
        value.reasons,
        [row(1, None, SelectionReason::RuntimeUnavailable)]
    );
    // An unbound variant with nothing against it can never be offered.
    assert!(
        plan(
            DevicePolicy::Auto,
            GpuPrecisionPreference::Auto,
            GpuUse::OptedIn,
            1,
            &[unbound(1, None), cpu(2, FULL, 8)],
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn reason_table_precision_preference_records_a_row_for_each_excluded_variant() -> Result<()> {
    let mut wide = gpu(1, "adapter-a", AdapterVendor::Nvidia);
    wide.precision = Some(DevicePrecision::Float32);
    let half = gpu(2, "adapter-a", AdapterVendor::Nvidia);
    let rows = [half.clone(), wide.clone(), cpu(3, FULL, 8)];
    // Auto keeps the existing GPU order: FP32 before FP16, then the ladder.
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        1,
        &rows,
    )?;
    assert_eq!(value.order, [1, 0, 2]);
    assert_eq!(value.reasons, [] as [CandidateRejection; 0]);
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Fp32,
        GpuUse::OptedIn,
        1,
        &rows,
    )?;
    assert_eq!(value.order, [1, 2]);
    assert_eq!(
        value.reasons,
        [row(
            2,
            Some("adapter-a"),
            SelectionReason::GpuPrecisionExcluded
        )]
    );
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Fp16,
        GpuUse::OptedIn,
        1,
        &rows,
    )?;
    assert_eq!(value.order, [0, 2]);
    assert_eq!(
        value.reasons,
        [row(
            1,
            Some("adapter-a"),
            SelectionReason::GpuPrecisionExcluded
        )]
    );
    // Excluded variants that were never probed are recorded, not dropped, and
    // the model is not reported as having no GPU route.
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Fp32,
        GpuUse::OptedIn,
        1,
        &[unbound(2, None), cpu(3, FULL, 8)],
    )?;
    assert_eq!(value.order, [1]);
    assert_eq!(
        value.reasons,
        [row(2, None, SelectionReason::GpuPrecisionExcluded)]
    );
    // The preference never restricts a CPU tier.
    assert!(GpuPrecisionPreference::Fp32.admits(None));
    assert!(GpuPrecisionPreference::Fp16.admits(None));
    assert!(!GpuPrecisionPreference::Fp32.admits(Some(DevicePrecision::Float16)));
    assert!(!GpuPrecisionPreference::Fp16.admits(Some(DevicePrecision::Float32)));
    Ok(())
}

#[test]
fn reason_table_deadline_authority_and_settlement_stop_the_operation_with_no_order() -> Result<()> {
    // remaining_us 0: deadline_expired, nothing ordered, even with fitting tiers.
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        0,
        &roster(AdapterVendor::Nvidia),
    )?;
    assert_eq!(value.order, [] as [usize; 0]);
    assert_eq!(value.reasons, [global(SelectionReason::DeadlineExpired)]);
    assert_eq!(
        value.operation_failure,
        Some(SelectionReason::DeadlineExpired)
    );
    assert_eq!(value.no_fit, None);
    for reason in [
        SelectionReason::AuthorityChanged,
        SelectionReason::SettlementUnknown,
        SelectionReason::DeadlineExpired,
    ] {
        for policy in [DevicePolicy::Auto, DevicePolicy::Cpu] {
            let rows = [
                rejected(gpu(1, "adapter-a", AdapterVendor::Nvidia), reason),
                cpu(2, FULL, 8),
            ];
            let value = plan(
                policy,
                GpuPrecisionPreference::Auto,
                GpuUse::NotOptedIn,
                1,
                &rows,
            )?;
            assert_eq!(value.order, [] as [usize; 0]);
            assert_eq!(value.reasons, [global(reason)]);
            assert_eq!(value.operation_failure, Some(reason));
        }
    }
    Ok(())
}

#[test]
fn no_cpu_tier_closes_a_gpu_only_order_and_a_candidate_of_another_model_is_refused() -> Result<()> {
    let rows = [gpu(1, "adapter-a", AdapterVendor::Nvidia)];
    let value = plan(
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        1,
        &rows,
    )?;
    assert_eq!(value.order, [0]);
    assert_eq!(value.reasons, [] as [CandidateRejection; 0]);
    assert_eq!(value.no_fit, Some(global(SelectionReason::NoQualifiedCpu)));
    Ok(())
}

#[test]
fn nothing_emits_smaller_model_selected() -> Result<()> {
    // Every policy, setting, precision preference and candidate shape of the
    // tables above. The value is in the vocabulary; no plan or selection holds it.
    let mut wide = gpu(4, "adapter-b", AdapterVendor::Nvidia);
    wide.precision = Some(DevicePrecision::Float32);
    let shapes: [Vec<AdvisoryCandidate>; 5] = [
        roster(AdapterVendor::Nvidia).to_vec(),
        roster(AdapterVendor::Other).to_vec(),
        vec![cpu(2, FAST, 8), cpu(3, FULL, 8)],
        vec![
            rejected(wide, SelectionReason::DeviceMemoryInsufficient),
            unbound(5, Some(SelectionReason::RuntimeUnavailable)),
            rejected(cpu(6, FULL, 8), SelectionReason::CpuFitInsufficient),
        ],
        vec![],
    ];
    let mut plans = 0;
    for candidates in &shapes {
        let gpu_candidates = candidates.iter().any(|row| row.backend.is_some());
        for policy in [DevicePolicy::Auto, DevicePolicy::Cpu] {
            for gpu_use in [GpuUse::NotOptedIn, GpuUse::NoAdapter, GpuUse::OptedIn] {
                if gpu_candidates && policy == DevicePolicy::Auto && gpu_use == GpuUse::NoAdapter {
                    continue;
                }
                for precision in [
                    GpuPrecisionPreference::Auto,
                    GpuPrecisionPreference::Fp32,
                    GpuPrecisionPreference::Fp16,
                ] {
                    for remaining_us in [0, 1] {
                        let value = plan(policy, precision, gpu_use, remaining_us, candidates)?;
                        let selection = select(
                            MODEL,
                            policy,
                            precision,
                            gpu_use,
                            1,
                            remaining_us,
                            candidates,
                        )?;
                        for row in value
                            .reasons
                            .iter()
                            .chain(&value.no_fit)
                            .chain(&selection.reasons)
                        {
                            assert_ne!(row.reason, SelectionReason::SmallerModelSelected);
                        }
                        assert_ne!(
                            value.operation_failure,
                            Some(SelectionReason::SmallerModelSelected)
                        );
                        assert_ne!(
                            selection.operation_failure,
                            Some(SelectionReason::SmallerModelSelected)
                        );
                        assert!(selection.selected.as_ref().is_none_or(|row| {
                            candidates.iter().any(|candidate| {
                                candidate.manifest == row.manifest && candidate.model == MODEL
                            })
                        }));
                        plans += 1;
                    }
                }
            }
        }
    }
    // Three shapes with a GPU candidate give 30 plans each, two without give 36.
    assert_eq!(plans, 162);
    Ok(())
}

#[test]
fn new_reasons_have_literal_wire_names_and_the_row_cap_is_twenty() -> Result<()> {
    for (reason, wire) in [
        (SelectionReason::GpuNotOptedIn, "\"gpu_not_opted_in\""),
        (
            SelectionReason::NoQualifiedGpuRoute,
            "\"no_qualified_gpu_route\"",
        ),
        (
            SelectionReason::SmallerModelSelected,
            "\"smaller_model_selected\"",
        ),
        (SelectionReason::ForcedCpu, "\"forced_cpu\""),
        (SelectionReason::NoGpu, "\"no_gpu\""),
    ] {
        assert_eq!(serde_json::to_string(&reason)?, wire);
        assert_eq!(serde_json::from_str::<SelectionReason>(wire)?, reason);
    }
    assert!(serde_json::from_str::<SelectionReason>("\"gpu_disabled\"").is_err());
    assert_eq!(MAX_SELECTION_REASONS, 20);
    Ok(())
}

#[test]
fn select_walks_the_plan_and_closes_a_no_fit_selection() -> Result<()> {
    // GPU use off: the Full tier is selected and the GPU candidate has no row.
    let rows = roster(AdapterVendor::Nvidia);
    let value = select(
        MODEL,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::NotOptedIn,
        1,
        1,
        &rows,
    )?;
    let selected = value.selected.context("Full tier not selected")?;
    assert_eq!(selected.manifest, id(3));
    assert_eq!(selected.backend, None);
    assert_eq!(selected.adapter_key, "cpu");
    assert_eq!(value.reasons, [global(SelectionReason::GpuNotOptedIn)]);
    assert_eq!(value.operation_failure, None);
    // Nothing fits: selected is absent and each tier has its row.
    let rows = [
        rejected(cpu(2, FAST, 8), SelectionReason::CpuFitInsufficient),
        rejected(cpu(3, FULL, 8), SelectionReason::HostMemoryInsufficient),
    ];
    let value = select(
        MODEL,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::NotOptedIn,
        1,
        1,
        &rows,
    )?;
    assert!(value.selected.is_none());
    assert_eq!(
        value.reasons,
        [
            global(SelectionReason::GpuNotOptedIn),
            row(3, Some("cpu"), SelectionReason::HostMemoryInsufficient),
            row(2, Some("cpu"), SelectionReason::CpuFitInsufficient),
        ]
    );
    assert_eq!(value.operation_failure, None);
    // A model with no CPU candidate closes its no-fit selection with one row.
    let rows = [rejected(
        gpu(1, "adapter-a", AdapterVendor::Nvidia),
        SelectionReason::ProviderInitializationFailed,
    )];
    let value = select(
        MODEL,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        1,
        1,
        &rows,
    )?;
    assert!(value.selected.is_none());
    assert_eq!(
        value.reasons,
        [
            row(
                1,
                Some("adapter-a"),
                SelectionReason::ProviderInitializationFailed
            ),
            global(SelectionReason::NoQualifiedCpu),
        ]
    );
    Ok(())
}
