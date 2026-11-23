// SPDX-License-Identifier: Apache-2.0
use super::*;
fn candidate(id: u8, gpu: bool, rejection: Option<SelectionReason>) -> AdvisoryCandidate {
    AdvisoryCandidate {
        manifest: Sha256Digest::from_bytes([id; 32]),
        model: SelectedModel::Arbiter,
        execution_identity: if gpu {
            "literal-cuda-execution"
        } else {
            "literal-cpu-execution"
        }
        .into(),
        backend: gpu.then_some(GpuBackend::Cuda),
        precision: gpu.then_some(DevicePrecision::Float32),
        adapter_key: Some(if gpu {
            format!("adapter-{id}")
        } else {
            "cpu".into()
        }),
        vendor: gpu.then_some(AdapterVendor::Nvidia),
        capability_epoch: 7,
        max_tokens: 2048,
        cpu_cores: 4,
        rejection,
    }
}
#[test]
fn auto_prefers_gpu_and_forced_cpu_is_explicit() -> Result<()> {
    let rows = [candidate(2, false, None), candidate(1, true, None)];
    let auto = select(
        SelectedModel::Arbiter,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        7,
        120_000_000,
        &rows,
    )?;
    assert_eq!(
        auto.selected.unwrap().manifest,
        Sha256Digest::from_bytes([1; 32])
    );
    let cpu = select(
        SelectedModel::Arbiter,
        DevicePolicy::Cpu,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        7,
        120_000_000,
        &rows,
    )?;
    assert_eq!(
        cpu.selected.unwrap().manifest,
        Sha256Digest::from_bytes([2; 32])
    );
    assert_eq!(cpu.reasons[0].reason, SelectionReason::ForcedCpu);
    Ok(())
}
#[test]
fn all_gpu_pre_dispatch_failures_use_same_model_cpu_with_exact_reason() -> Result<()> {
    for reason in [
        SelectionReason::Unqualified,
        SelectionReason::TargetUnsupported,
        SelectionReason::RuntimeUnavailable,
        SelectionReason::DriverUnsupported,
        SelectionReason::DeviceUnsupported,
        SelectionReason::GraphUnsupported,
        SelectionReason::ProviderInitializationFailed,
        SelectionReason::AllocationFailed,
        SelectionReason::DeviceMemoryUnknown,
        SelectionReason::DeviceMemoryInsufficient,
        SelectionReason::HostMemoryUnknown,
        SelectionReason::HostMemoryInsufficient,
        SelectionReason::CircuitOpen,
    ] {
        let rows = [candidate(1, true, Some(reason)), candidate(2, false, None)];
        let result = select(
            SelectedModel::Arbiter,
            DevicePolicy::Auto,
            GpuPrecisionPreference::Auto,
            GpuUse::OptedIn,
            7,
            100,
            &rows,
        )?;
        assert_eq!(
            result.selected.unwrap().manifest,
            Sha256Digest::from_bytes([2; 32])
        );
        assert_eq!(result.reasons[0].reason, reason);
    }
    Ok(())
}
#[test]
fn cpu_fit_failure_and_absent_cpu_never_substitute_another_model() -> Result<()> {
    let rows = [
        candidate(1, true, Some(SelectionReason::GraphUnsupported)),
        candidate(2, false, Some(SelectionReason::CpuFitInsufficient)),
    ];
    let result = select(
        SelectedModel::Arbiter,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        7,
        100,
        &rows,
    )?;
    assert!(result.selected.is_none());
    assert_eq!(
        result.reasons[1].reason,
        SelectionReason::CpuFitInsufficient
    );
    let result = select(
        SelectedModel::Arbiter,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        7,
        100,
        &rows[..1],
    )?;
    assert!(result.selected.is_none());
    assert_eq!(
        result.reasons.last().unwrap().reason,
        SelectionReason::NoQualifiedCpu
    );
    let mut foreign = candidate(3, false, None);
    foreign.model = SelectedModel::Focus;
    assert!(
        select(
            SelectedModel::Arbiter,
            DevicePolicy::Auto,
            GpuPrecisionPreference::Auto,
            GpuUse::OptedIn,
            7,
            100,
            &[foreign]
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn deterministic_adapter_order_and_generation_revalidation() -> Result<()> {
    let rows = [
        candidate(2, true, None),
        candidate(1, true, None),
        candidate(3, false, None),
    ];
    assert_eq!(
        select(
            SelectedModel::Arbiter,
            DevicePolicy::Auto,
            GpuPrecisionPreference::Auto,
            GpuUse::OptedIn,
            7,
            100,
            &rows
        )?
        .selected
        .unwrap()
        .manifest,
        Sha256Digest::from_bytes([1; 32])
    );
    let result = select(
        SelectedModel::Arbiter,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        8,
        100,
        &rows,
    )?;
    assert!(result.selected.is_none());
    assert!(
        result
            .reasons
            .iter()
            .all(|row| row.reason == SelectionReason::ObservationStale)
    );
    assert!(
        select(
            SelectedModel::Arbiter,
            DevicePolicy::Auto,
            GpuPrecisionPreference::Auto,
            GpuUse::OptedIn,
            7,
            100,
            &[candidate(1, true, None), candidate(1, true, None)]
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn original_deadline_and_uncertain_settlement_forbid_replay() -> Result<()> {
    let result = select(
        SelectedModel::Arbiter,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        7,
        0,
        &[candidate(1, true, None)],
    )?;
    assert!(result.selected.is_none());
    assert_eq!(result.reasons[0].reason, SelectionReason::DeadlineExpired);
    assert_eq!(
        fallback_permitted(true, false, true, 100),
        Err(SelectionReason::SettlementUnknown)
    );
    assert_eq!(
        fallback_permitted(true, true, true, 0),
        Err(SelectionReason::DeadlineExpired)
    );
    assert_eq!(
        fallback_permitted(false, false, false, 100),
        Err(SelectionReason::AuthorityChanged)
    );
    assert_eq!(fallback_permitted(true, true, true, 100), Ok(()));
    Ok(())
}
#[test]
fn device_memory_is_checked_joint_capacity_not_arena_limit() -> Result<()> {
    assert_eq!(
        DeviceMemory::Dedicated {
            required_peak_bytes: 10,
            headroom_bytes: 2
        }
        .required_bytes()?,
        12
    );
    assert_eq!(
        DeviceMemory::Unified {
            required_joint_peak_bytes: 20,
            headroom_bytes: 3
        }
        .required_bytes()?,
        23
    );
    assert!(
        DeviceMemory::Dedicated {
            required_peak_bytes: u64::MAX,
            headroom_bytes: 1
        }
        .required_bytes()
        .is_err()
    );
    assert!(
        DeviceMemory::Unified {
            required_joint_peak_bytes: 0,
            headroom_bytes: 1
        }
        .required_bytes()
        .is_err()
    );
    Ok(())
}
#[test]
fn default_auto_and_unknown_wire_fields_refuse() -> Result<()> {
    assert_eq!(DevicePolicy::default(), DevicePolicy::Auto);
    assert_eq!(
        serde_json::from_str::<PlacementPolicy>(r#"{"mode":"strict_gpu"}"#)?,
        PlacementPolicy::StrictGpu
    );
    assert!(serde_json::from_str::<PlacementPolicy>(
        r#"{"mode":"strict_gpu","policy":"1111111111111111111111111111111111111111111111111111111111111111"}"#
    ).is_err());
    assert!(serde_json::from_str::<PlacementPolicy>(
        r#"{"mode":"bounded_hybrid","policy":"1111111111111111111111111111111111111111111111111111111111111111","allow_cpu":true}"#
    ).is_err());

    assert!(serde_json::from_str::<DevicePolicy>(r#""gpu_only""#).is_err());
    assert!(
        serde_json::from_str::<PlacementPolicy>(r#"{"mode":"strict_gpu","allow_cpu":true}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<DeviceMemory>(
            r#"{"kind":"dedicated","required_peak_bytes":1,"headroom_bytes":1,"gpu_mem_limit":2}"#
        )
        .is_err()
    );
    assert!(serde_json::from_str::<DeviceCatalog>(r#"{"schema":"airk.arbiter.device-catalog/2","focus":[],"arbiter":[],"context_large":[]}"#).is_err());
    Ok(())
}

#[test]
fn capacity_unknown_race_and_unified_accounting_are_explicit() -> Result<()> {
    let requirement = DeviceMemory::Dedicated {
        required_peak_bytes: 10,
        headroom_bytes: 2,
    };
    assert_eq!(
        device_capacity_refusal(&requirement, None, 0)?,
        Some(SelectionReason::DeviceMemoryUnknown)
    );
    assert_eq!(device_capacity_refusal(&requirement, Some(12), 0)?, None);
    assert_eq!(
        device_capacity_refusal(&requirement, Some(12), 1)?,
        Some(SelectionReason::DeviceMemoryInsufficient)
    );
    assert_eq!(
        device_capacity_refusal(&requirement, Some(1), u64::MAX)?,
        Some(SelectionReason::DeviceMemoryInsufficient)
    );
    let unified = DeviceMemory::Unified {
        required_joint_peak_bytes: 10,
        headroom_bytes: 2,
    };
    assert_eq!(device_capacity_refusal(&unified, Some(12), 0)?, None);
    Ok(())
}
#[test]
fn cpu_profile_omission_roundtrip_and_explicit_disabled_migration() -> Result<()> {
    let profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.validate()?;
    assert!(profile.device_execution.is_none());
    let serialized = serde_json::to_value(&profile)?;
    assert!(serialized.get("device_execution").is_none());
    assert_eq!(
        profile.device_execution_identity()?,
        profile
            .cpu_fit
            .clone()
            .unwrap_or_default()
            .runtime_identity()?
    );
    let legacy: Configuration = serde_json::from_str(
        r#"{"schema":"airk.arbiter.configuration/3","installation":"0101010101010101010101010101010101010101010101010101010101010101","catalog":"02020202020202020202020202020202","revision":1,"focus":{"enabled":false,"selected_profile":null,"indexes":[]},"context":{"enabled":false,"selected_profile":null,"indexes":[]}}"#,
    )?;
    let before = legacy.canonical_bytes()?;
    let migrated = DeviceConfiguration::migrate_legacy(&legacy, test_binding(), |_| None)?;
    assert_eq!(migrated.focus.device_policy, DevicePolicy::Auto);
    assert_eq!(migrated.context.device_policy, DevicePolicy::Auto);
    assert!(!migrated.focus.enabled && !migrated.context.enabled);
    assert_eq!(migrated.revision, legacy.revision + 1);
    assert_eq!(legacy.canonical_bytes()?, before);
    assert_eq!(
        DeviceStageConfiguration::disabled().device_policy,
        DevicePolicy::Auto
    );
    Ok(())
}

#[test]
fn gpu_execution_identity_binds_every_policy_and_cpu_runtime_cannot_impersonate_it() -> Result<()> {
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    let pin = Sha256Digest::from_bytes([7; 32]);
    let execution = DeviceExecution {
        schema: DEVICE_EXECUTION_SCHEMA.into(),
        backend: GpuBackend::Cuda,
        runtime_build: pin,
        compatibility_policy: pin,
        provider_options: pin,
        math_policy: pin,
        precision: DevicePrecision::Float32,
        tf32: false,
        placement: PlacementPolicy::StrictGpu,
        graph: profile
            .assets
            .iter()
            .find(|asset| asset.role == AssetRole::Model)
            .unwrap()
            .sha256,
        host_fit: CpuFit::default(),
        memory: DeviceMemory::Dedicated {
            required_peak_bytes: 10,
            headroom_bytes: 2,
        },
    };
    let identity = execution.execution_identity()?;
    let mut tf32 = execution.clone();
    tf32.tf32 = true;
    assert!(tf32.execution_identity().is_err());
    assert!(identity.starts_with("onnxruntime-cuda/1.30.0/device-execution/"));
    let mut changed = execution.clone();
    changed.math_policy = Sha256Digest::from_bytes([8; 32]);
    assert_ne!(identity, changed.execution_identity()?);
    changed = execution.clone();
    changed.provider_options = Sha256Digest::from_bytes([8; 32]);
    assert_ne!(identity, changed.execution_identity()?);
    changed = execution.clone();
    changed.placement = PlacementPolicy::BoundedHybrid { policy: pin };
    assert_ne!(identity, changed.execution_identity()?);
    profile.device_execution = Some(execution);
    assert!(profile.validate().is_err()); // legacy CPU identity is never reinterpreted
    profile.device_execution = None;
    profile.runtime = "onnxruntime-cuda/1.30.0".into();
    assert!(profile.validate().is_err());
    Ok(())
}
#[test]
fn two_slot_catalog_requires_every_slot_and_canonical_resolution() -> Result<()> {
    let empty = DeviceCatalog {
        schema: DEVICE_CATALOG_SCHEMA.into(),
        focus: vec![],
        arbiter: vec![],
    };
    assert!(empty.validate(|_| None).is_err());
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.cpu_fit = Some(CpuFit::default());
    let row = VariantReference {
        manifest: profile.manifest,
        execution_identity: profile.device_execution_identity()?,
    };
    let bad = DeviceCatalog {
        schema: DEVICE_CATALOG_SCHEMA.into(),
        focus: vec![row.clone()],
        arbiter: vec![row],
    };
    assert!(bad.validate(|_| Some(&profile)).is_err()); // historical/foreign model cannot acquire Focus
    assert!(bad.validate(|_| None).is_err());
    Ok(())
}

#[test]
fn operation_wide_failures_never_fall_through_to_another_device() -> Result<()> {
    for reason in [
        SelectionReason::AuthorityChanged,
        SelectionReason::SettlementUnknown,
        SelectionReason::DeadlineExpired,
    ] {
        for policy in [DevicePolicy::Auto, DevicePolicy::Cpu] {
            let rows = [
                candidate(1, true, Some(reason)),
                candidate(2, false, None),
                candidate(3, true, None),
            ];
            let result = select(
                SelectedModel::Arbiter,
                policy,
                GpuPrecisionPreference::Auto,
                GpuUse::OptedIn,
                7,
                100,
                &rows,
            )?;
            assert!(result.selected.is_none());
            assert_eq!(result.operation_failure, Some(reason));
            assert_eq!(result.reasons.len(), 1);
            assert_eq!(result.reasons[0].reason, reason);
        }
    }
    Ok(())
}
pub(super) fn literal_profile(slot: SelectedModel, id: u8, gpu: bool) -> Result<Profile> {
    let mut profile = crate::arbiter::stage_tests::current_profile(slot.stage())?;
    profile.model = slot.model();
    profile.manifest = Sha256Digest::from_bytes([id; 32]);
    profile.cpu_fit = Some(CpuFit::default());
    if gpu {
        let pin = Sha256Digest::from_bytes([7; 32]);
        profile.cpu_fit = None;
        profile.runtime = "onnxruntime-cuda/1.30.0".into();
        profile.device_execution = Some(DeviceExecution {
            schema: DEVICE_EXECUTION_SCHEMA.into(),
            backend: GpuBackend::Cuda,
            runtime_build: pin,
            compatibility_policy: pin,
            provider_options: pin,
            math_policy: pin,
            precision: DevicePrecision::Float32,
            tf32: false,
            placement: PlacementPolicy::StrictGpu,
            graph: profile
                .assets
                .iter()
                .find(|asset| asset.role == AssetRole::Model)
                .unwrap()
                .sha256,
            host_fit: CpuFit::default(),
            memory: DeviceMemory::Dedicated {
                required_peak_bytes: 10,
                headroom_bytes: 2,
            },
        });
    }
    profile.validate()?;
    Ok(profile)
}
#[test]
fn exact_two_slots_materialize_distinct_cpu_gpu_and_refuse_cross_model() -> Result<()> {
    let profiles = [
        literal_profile(SelectedModel::Focus, 1, false)?,
        literal_profile(SelectedModel::Arbiter, 2, false)?,
        literal_profile(SelectedModel::Arbiter, 3, true)?,
    ];
    let row = |i: usize| -> Result<VariantReference> {
        Ok(VariantReference {
            manifest: profiles[i].manifest,
            execution_identity: profiles[i].device_execution_identity()?,
        })
    };
    let catalog = DeviceCatalog {
        schema: DEVICE_CATALOG_SCHEMA.into(),
        focus: vec![row(0)?],
        arbiter: vec![row(1)?, row(2)?],
    };
    let resolve = |pin| profiles.iter().find(|profile| profile.manifest == pin);
    catalog.validate(resolve)?;
    assert_eq!(
        catalog
            .materialize(SelectedModel::Arbiter, profiles[2].manifest, resolve)?
            .runtime,
        "onnxruntime-cuda/1.30.0"
    );
    assert!(
        catalog
            .materialize(SelectedModel::Focus, profiles[2].manifest, resolve)
            .is_err()
    );
    let mut stale = catalog.clone();
    stale.arbiter[1].execution_identity = "onnxruntime-cpu/1.30.0".into();
    assert!(stale.validate(resolve).is_err());
    let mut duplicate = catalog.clone();
    duplicate.arbiter.push(row(1)?);
    assert!(duplicate.validate(resolve).is_err());
    assert!(
        catalog
            .materialize(
                SelectedModel::Arbiter,
                Sha256Digest::from_bytes([99; 32]),
                resolve
            )
            .is_err()
    );
    Ok(())
}
#[test]
fn enabled_legacy_migrates_auto_without_changing_model_enrollment_or_consent() -> Result<()> {
    let focus = literal_profile(SelectedModel::Focus, 7, false)?;
    let compact = literal_profile(SelectedModel::Arbiter, 8, false)?;
    let legacy = crate::arbiter::stage_tests::configuration(true, true);
    let before = legacy.canonical_bytes()?;
    let migrated = DeviceConfiguration::migrate_legacy(&legacy, test_binding(), |pin| {
        if pin == focus.manifest {
            Some(&focus)
        } else if pin == compact.manifest {
            Some(&compact)
        } else {
            None
        }
    })?;
    assert!(migrated.focus.enabled && migrated.context.enabled);
    assert_eq!(migrated.focus.selected_model, Some(SelectedModel::Focus));
    assert_eq!(
        migrated.context.selected_model,
        Some(SelectedModel::Arbiter)
    );
    assert_eq!(migrated.focus.device_policy, DevicePolicy::Auto);
    assert_eq!(migrated.context.device_policy, DevicePolicy::Auto);
    assert_eq!(
        serde_json::to_value(&migrated.focus.indexes)?,
        serde_json::to_value(&legacy.focus.indexes)?
    );
    assert_eq!(
        serde_json::to_value(&migrated.context.indexes)?,
        serde_json::to_value(&legacy.context.indexes)?
    );
    assert_eq!(legacy.canonical_bytes()?, before);
    let mut explicit_cpu = migrated.clone();
    explicit_cpu.context.device_policy = DevicePolicy::Cpu;
    let bytes = explicit_cpu.canonical_bytes()?;
    let decoded: DeviceConfiguration = serde_json::from_slice(&bytes)?;
    assert_eq!(decoded.context.device_policy, DevicePolicy::Cpu);
    assert_eq!(decoded.canonical_bytes()?, bytes);
    assert!(DeviceConfiguration::migrate_legacy(&legacy, test_binding(), |_| None).is_err());
    assert!(
        DeviceConfiguration::migrate_legacy(&legacy, test_binding(), |_| Some(&compact)).is_err()
    );
    Ok(())
}
#[test]
fn exact_execution_canonical_bytes_and_framed_digest_are_literal() -> Result<()> {
    const CANONICAL: &str = r#"{"schema":"airk.arbiter.device-execution/1","backend":"cuda","runtime_build":"0707070707070707070707070707070707070707070707070707070707070707","compatibility_policy":"0707070707070707070707070707070707070707070707070707070707070707","provider_options":"0707070707070707070707070707070707070707070707070707070707070707","math_policy":"0707070707070707070707070707070707070707070707070707070707070707","precision":"float32","tf32":false,"placement":{"mode":"strict_gpu"},"graph":"0707070707070707070707070707070707070707070707070707070707070707","host_fit":{"minimum_physical_cores":1,"intra_threads":1,"inter_threads":1,"sequential":true,"spinning":false},"memory":{"kind":"dedicated","required_peak_bytes":10,"headroom_bytes":2}}"#;
    let execution: DeviceExecution = serde_json::from_str(CANONICAL)?;
    assert_eq!(serde_json::to_vec(&execution)?, CANONICAL.as_bytes());
    assert_eq!(
        execution.execution_identity()?,
        "onnxruntime-cuda/1.30.0/device-execution/75e7dcb0dc85d782bbd317e604c3cb3f1000622b04ee585eca4e76cd62b77f61"
    );
    let encoded = serde_json::to_vec(&execution)?;
    let decoded: DeviceExecution = serde_json::from_slice(&encoded)?;
    assert_eq!(decoded, execution);
    let mut duplicate=CANONICAL.replacen("\"runtime_build\":","\"runtime_build\":\"0707070707070707070707070707070707070707070707070707070707070707\",\"runtime_build\":",1);
    assert!(serde_json::from_str::<DeviceExecution>(&duplicate).is_err());
    duplicate = CANONICAL.replacen(
        "\"schema\":\"airk.arbiter.device-execution/1\"",
        "\"schema\":\"airk.arbiter.device-execution/9\"",
        1,
    );
    assert!(
        serde_json::from_str::<DeviceExecution>(&duplicate)?
            .execution_identity()
            .is_err()
    );
    Ok(())
}

#[test]
fn materialization_never_returns_a_changed_unverified_resolver_reference() -> Result<()> {
    use std::cell::Cell;
    let profiles = [
        literal_profile(SelectedModel::Focus, 1, false)?,
        literal_profile(SelectedModel::Arbiter, 2, false)?,
        literal_profile(SelectedModel::Focus, 3, false)?,
    ];
    let row = |i: usize| -> Result<VariantReference> {
        Ok(VariantReference {
            manifest: profiles[i].manifest,
            execution_identity: profiles[i].device_execution_identity()?,
        })
    };
    let catalog = DeviceCatalog {
        schema: DEVICE_CATALOG_SCHEMA.into(),
        focus: vec![row(0)?],
        arbiter: vec![row(1)?],
    };
    let calls = Cell::new(0usize);
    let resolve = |pin| {
        if pin == profiles[1].manifest {
            calls.set(calls.get() + 1);
            Some(if calls.get() == 1 {
                &profiles[1]
            } else {
                &profiles[2]
            })
        } else {
            profiles.iter().find(|profile| profile.manifest == pin)
        }
    };
    let selected = catalog.materialize(SelectedModel::Arbiter, profiles[1].manifest, resolve)?;
    assert_eq!(calls.get(), 1);
    assert!(std::ptr::eq(selected, &profiles[1]));
    assert_ne!(selected.model, ModelChoice::ContextQwenLarge);
    // A resolver giving the wrong object first is refused, never retried into success.
    let calls = Cell::new(0usize);
    let wrong_first = |pin| {
        if pin == profiles[1].manifest {
            calls.set(calls.get() + 1);
            Some(if calls.get() == 1 {
                &profiles[2]
            } else {
                &profiles[1]
            })
        } else {
            profiles.iter().find(|profile| profile.manifest == pin)
        }
    };
    assert!(
        catalog
            .materialize(SelectedModel::Arbiter, profiles[1].manifest, wrong_first)
            .is_err()
    );
    assert_eq!(calls.get(), 1);
    Ok(())
}

#[test]
fn explicit_fp32_fp16_scope_and_honest_probe_bounds_cannot_become_qualification() -> Result<()> {
    for precision in ["bfloat16", "mixed"] {
        assert!(serde_json::from_str::<DevicePrecision>(&format!("\"{precision}\"")).is_err());
    }
    assert_eq!(
        serde_json::from_str::<DevicePrecision>("\"float32\"")?,
        DevicePrecision::Float32
    );
    let probe = GpuProbeReservation {
        schema: "airk.arbiter.gpu-probe-reservation/1".into(),
        host_limit_bytes: 32 << 30,
        device_limit_bytes: 24 << 30,
        deadline_ms: 120000,
    };
    probe.validate()?;
    let mut expired = probe.clone();
    expired.deadline_ms = 120001;
    assert!(expired.validate().is_err());
    let mut zero = probe.clone();
    zero.device_limit_bytes = 0;
    assert!(zero.validate().is_err());
    let bytes = serde_json::to_vec(&probe)?;
    assert!(serde_json::from_slice::<DeviceExecution>(&bytes).is_err());
    assert!(serde_json::from_slice::<DeviceMemory>(&bytes).is_err());
    Ok(())
}

#[test]
fn gpu_precision_preference_is_independent_of_device_and_reports_cpu_fallback() -> Result<()> {
    let fp32 = candidate(1, true, None);
    let mut fp16 = candidate(2, true, None);
    fp16.precision = Some(DevicePrecision::Float16);
    let cpu = candidate(3, false, None);
    let rows = [fp16.clone(), cpu.clone(), fp32.clone()];
    let auto = select(
        SelectedModel::Arbiter,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        7,
        100,
        &rows,
    )?;
    assert_eq!(
        auto.selected.unwrap().precision,
        Some(DevicePrecision::Float32)
    );
    let selected = select(
        SelectedModel::Arbiter,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Fp16,
        GpuUse::OptedIn,
        7,
        100,
        &rows,
    )?;
    assert_eq!(
        selected.selected.unwrap().precision,
        Some(DevicePrecision::Float16)
    );
    let mut low = fp32;
    low.rejection = Some(SelectionReason::DeviceMemoryInsufficient);
    let automatic = select(
        SelectedModel::Arbiter,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        7,
        100,
        &[low.clone(), fp16, cpu.clone()],
    )?;
    assert_eq!(
        automatic.selected.unwrap().precision,
        Some(DevicePrecision::Float16)
    );
    let explicit = select(
        SelectedModel::Arbiter,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Fp32,
        GpuUse::OptedIn,
        7,
        100,
        &[low, cpu],
    )?;
    let selected = explicit.selected.unwrap();
    assert_eq!(selected.backend, None);
    assert_eq!(selected.precision, None);
    assert_eq!(
        explicit.reasons[0].reason,
        SelectionReason::DeviceMemoryInsufficient
    );
    assert_eq!(
        serde_json::from_str::<GpuPrecisionPreference>("\"fp16\"")?,
        GpuPrecisionPreference::Fp16
    );
    assert!(serde_json::from_str::<GpuPrecisionPreference>("\"bf16\"").is_err());
    Ok(())
}

fn test_binding() -> DeviceCatalogBinding {
    DeviceCatalogBinding {
        catalog: Sha256Digest::from_bytes([31; 32]),
        source: Sha256Digest::from_bytes([32; 32]),
    }
}
