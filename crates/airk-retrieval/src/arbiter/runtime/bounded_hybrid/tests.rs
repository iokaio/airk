// SPDX-License-Identifier: Apache-2.0
use super::*;

fn sample() -> Vec<GraphMetadataRow> {
    use GraphMetadataRow::*;
    vec![
        Graph {
            graph: 0,
            parent_graph: None,
            parent_node: None,
            attribute: String::new(),
            outer_bindings: "not_applicable".into(),
        },
        Node {
            graph: 0,
            node: 0,
            name: String::new(),
            domain: String::new(),
            op: "Shape".into(),
            ep: CPU.into(),
            version: 1,
        },
        Node {
            graph: 0,
            node: 1,
            name: String::new(),
            domain: String::new(),
            op: "Other".into(),
            ep: CUDA.into(),
            version: 1,
        },
        Value {
            graph: 0,
            value: 1,
            name: "control".into(),
            element_type: Some(7),
            rank: Some(1),
            type_state: "tensor".into(),
            provenance: "node_output".into(),
            producer: Some(0),
        },
        Dimension {
            graph: 0,
            value: 1,
            index: 0,
            number: Some(5),
            symbol: None,
        },
        Edge {
            graph: 0,
            node: Some(0),
            value: Some(1),
            role: 2,
            slot: 0,
        },
        Edge {
            graph: 0,
            node: Some(1),
            value: Some(1),
            role: 0,
            slot: 0,
        },
        End { graph: 0 },
    ]
}

#[test]
fn bounded_hybrid_cpu_value_gate_refuses_unprofiled_or_non_control_tensors() -> Result<()> {
    use GraphMetadataRow::*;
    let rows = sample();
    validate_cpu_values(&rows, CpuValueBounds::fp32(COMPACT_CPU_PAYLOAD_BYTES))?;

    let rejects = [
        (
            4,
            Dimension {
                graph: 0,
                value: 1,
                index: 0,
                number: Some(6),
                symbol: None,
            },
        ),
        (
            4,
            Dimension {
                graph: 0,
                value: 1,
                index: 0,
                number: None,
                symbol: Some("dynamic".into()),
            },
        ),
        (
            5,
            Edge {
                graph: 0,
                node: Some(0),
                value: Some(1),
                role: 1,
                slot: 0,
            },
        ),
        (
            5,
            Edge {
                graph: 0,
                node: Some(0),
                value: Some(2),
                role: 2,
                slot: 0,
            },
        ),
    ];
    for (index, replacement) in rejects {
        let mut rows = rows.clone();
        rows[index] = replacement;
        assert!(
            validate_cpu_values(&rows, CpuValueBounds::fp32(COMPACT_CPU_PAYLOAD_BYTES)).is_err()
        );
    }
    let mut rows = rows;
    let Value { element_type, .. } = &mut rows[3] else {
        unreachable!()
    };
    *element_type = Some(1);
    assert!(validate_cpu_values(&rows, CpuValueBounds::fp32(COMPACT_CPU_PAYLOAD_BYTES)).is_err());
    Ok(())
}

#[test]
fn compact_answer_uses_original_operation_and_fixture_expiries() -> Result<()> {
    let load_end = Instant::now();
    let now = load_end + Duration::from_secs(1);
    let expiry = now + Duration::from_secs(120);
    let whole_expiry = expiry + Duration::from_secs(1);
    let limit = Duration::from_secs(120);
    require_answer_deadline(now, expiry, whole_expiry, limit)?;
    for (deadline, overall) in [
        (now, whole_expiry),
        (load_end, whole_expiry),
        (expiry + Duration::from_nanos(1), whole_expiry),
        (expiry, expiry - Duration::from_nanos(1)),
    ] {
        assert!(require_answer_deadline(now, deadline, overall, limit).is_err());
    }
    Ok(())
}

#[test]
fn compact_resource_operations_keep_independent_deadlines_and_exact_width() -> Result<()> {
    let load_expiry = Instant::now() + Duration::from_secs(120);
    let now = load_expiry + Duration::from_secs(1);
    let operation_expiry = now + Duration::from_secs(120);
    let overall_expiry = now + Duration::from_secs(3600);
    let mut input = CompiledInput {
        prompt: String::new(),
        input_ids: vec![1; 8192],
        attention_mask: vec![1; 8192],
        position_ids: (0..8192).collect(),
        decision_position: 8191,
        candidate_positions: (0..8).collect(),
        option_mask: vec![true; 8],
        input_digest: Sha256Digest::from_bytes([1; 32]),
    };
    require_resource_operation(&input, now, operation_expiry, overall_expiry)?;
    assert!(require_resource_operation(&input, now, now, overall_expiry).is_err());
    assert!(
        require_resource_operation(
            &input,
            now,
            operation_expiry + Duration::from_nanos(1),
            overall_expiry,
        )
        .is_err()
    );
    assert!(
        require_resource_operation(
            &input,
            now,
            operation_expiry,
            operation_expiry - Duration::from_nanos(1),
        )
        .is_err()
    );
    input.attention_mask[0] = 0;
    assert!(require_resource_operation(&input, now, operation_expiry, overall_expiry).is_err());
    input.attention_mask[0] = 1;
    input.option_mask[0] = false;
    assert!(require_resource_operation(&input, now, operation_expiry, overall_expiry).is_err());
    input.option_mask[0] = true;
    input.input_ids.truncate(1024);
    assert!(require_resource_operation(&input, now, operation_expiry, overall_expiry).is_err());
    Ok(())
}

#[test]
fn compact_fp16_bounded_cpu_values_admit_only_measured_control_geometry() -> Result<()> {
    use GraphMetadataRow::*;
    let mut rows = sample();
    assert!(validate_cpu_values(&rows, CpuValueBounds::COMPACT_FP16).is_err());
    let Dimension { number, .. } = &mut rows[4] else {
        unreachable!()
    };
    *number = Some(4);
    validate_cpu_values(&rows, CpuValueBounds::COMPACT_FP16)?;
    let Value { element_type, .. } = &mut rows[3] else {
        unreachable!()
    };
    *element_type = Some(6);
    assert!(validate_cpu_values(&rows, CpuValueBounds::fp32(COMPACT_CPU_PAYLOAD_BYTES)).is_err());
    validate_cpu_values(&rows, CpuValueBounds::COMPACT_FP16)?;
    let bounds = CpuValueBounds {
        maximum_payload: 15,
        ..CpuValueBounds::COMPACT_FP16
    };
    assert!(validate_cpu_values(&rows, bounds).is_err());
    let bounds = CpuValueBounds {
        maximum_payload: 16,
        ..CpuValueBounds::COMPACT_FP16
    };
    let Edge { node, .. } = &mut rows[6] else {
        unreachable!()
    };
    *node = Some(0);
    validate_cpu_values(&rows, bounds)?;
    let Edge { role, .. } = &mut rows[6] else {
        unreachable!()
    };
    *role = 1;
    assert!(validate_cpu_values(&rows, bounds).is_err());
    Ok(())
}

#[test]
fn compact_fp16_bounded_attention_requires_the_exact_canonical_cuda_roster() -> Result<()> {
    let names = (0..28)
        .map(|i| format!("attention-{i}"))
        .collect::<Vec<_>>();
    let expected = names
        .iter()
        .map(|name| {
            serde_json::json!({
                "name": name, "operator": "GroupQueryAttention", "domain": "com.microsoft"
            })
        })
        .collect::<Vec<_>>();
    let plan = serde_json::json!({
        "nodes": expected, "operator": "GroupQueryAttention", "domain": "com.microsoft",
        "dtype": "fp16", "backend": "FLASH_ATTENTION"
    });
    let mut rows = names
        .into_iter()
        .enumerate()
        .map(|(i, name)| GraphMetadataRow::Node {
            graph: 0,
            node: i as u64,
            name,
            domain: "com.microsoft".into(),
            op: "GroupQueryAttention".into(),
            ep: "CUDAExecutionProvider".into(),
            version: 1,
        })
        .collect::<Vec<_>>();
    validate_compact_fp16_attention(&rows, &plan)?;
    let mut altered = rows.clone();
    altered.pop();
    assert!(validate_compact_fp16_attention(&altered, &plan).is_err());
    let GraphMetadataRow::Node { ep, .. } = &mut rows[0] else {
        unreachable!()
    };
    *ep = "CPUExecutionProvider".into();
    assert!(validate_compact_fp16_attention(&rows, &plan).is_err());
    Ok(())
}

#[test]
fn compact_fp16_bounded_graph_refuses_caller_selected_evidence() {
    let raw = b"{}";
    let sha = Sha256Digest::from_bytes(Sha256::digest(raw).into());
    assert!(GraphGate::new_compact_fp16(raw, sha).is_err());
    assert!(GraphGate::new_compact_fp16(raw, COMPACT_FP16_GRAPH_SHA.parse().unwrap()).is_err());
}

fn reviewed_cuda_base() -> Result<Profile> {
    use airk_core::arbiter::device::{
        DEVICE_EXECUTION_SCHEMA, DeviceExecution, DeviceMemory, DevicePrecision, GpuBackend,
        PlacementPolicy,
    };
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.model = ModelChoice::ContextQwenCompact;
    profile.qualification = None;
    profile.qualified_targets.clear();
    profile.downloads.clear();
    profile.profile_id = "private-cuda-fp32-context-compact-calibration-base-v1".into();
    profile.manifest = Sha256Digest::from_bytes([1; 32]);
    let digest = Sha256Digest::from_bytes([9; 32]);
    profile.device_execution = Some(DeviceExecution {
        schema: DEVICE_EXECUTION_SCHEMA.into(),
        backend: GpuBackend::Cuda,
        runtime_build: digest,
        compatibility_policy: digest,
        provider_options: digest,
        math_policy: digest,
        precision: DevicePrecision::Float32,
        tf32: false,
        placement: PlacementPolicy::StrictGpu,
        graph: Sha256Digest::from_bytes([7; 32]),
        host_fit: Default::default(),
        memory: DeviceMemory::Dedicated {
            required_peak_bytes: 40 << 30,
            headroom_bytes: 4 << 30,
        },
    });
    if !profile
        .assets
        .iter()
        .any(|asset| asset.role == AssetRole::Calibration)
    {
        profile.assets.push(Asset {
            name: "context-action-calibration.json".into(),
            role: AssetRole::Calibration,
            bytes: 10,
            sha256: Sha256Digest::from_bytes([2; 32]),
        });
    }
    let calibration = profile
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("calibration asset")?;
    calibration.sha256 = Sha256Digest::from_bytes([2; 32]);
    calibration.bytes = 10;
    profile.calibration = calibration.sha256;
    profile.assets.retain(|asset| {
        asset.role != AssetRole::Calibration || asset.sha256 == Sha256Digest::from_bytes([2; 32])
    });
    Ok(profile)
}

fn provisional_variant(base: &Profile) -> Result<Profile> {
    let mut variant = base.clone();
    let calibration = variant
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("calibration asset")?;
    calibration.sha256 = Sha256Digest::from_bytes([3; 32]);
    calibration.bytes = 42;
    variant.calibration = calibration.sha256;
    variant.profile_id = format!("{}-cs1-provisional-read", base.profile_id);
    variant.manifest = Sha256Digest::from_bytes([4; 32]);
    Ok(variant)
}

#[test]
fn calibration_only_cuda_variant_is_checked_against_its_reviewed_base() -> Result<()> {
    let base = reviewed_cuda_base()?;
    let variant = provisional_variant(&base)?;
    // Accepted: the reviewed result names the base; checks then run on the base.
    let reviewed = reviewed_profile(base.manifest, Some(&base), &variant)?;
    assert_eq!(reviewed.manifest, base.manifest);
    // Without a base the opened profile itself is checked, unchanged.
    assert_eq!(reviewed_profile(variant.manifest, None, &variant)?.manifest, variant.manifest);
    let mut compact = base.clone();
    compact.model = ModelChoice::ContextQwenCompact;
    let compact_variant = Profile {
        model: ModelChoice::ContextQwenCompact,
        ..variant.clone()
    };
    require_calibration_only_variant(&compact, &compact_variant)?;
    Ok(())
}

#[test]
fn calibration_only_cuda_variant_refuses_every_other_difference() -> Result<()> {
    let base = reviewed_cuda_base()?;
    let variant = provisional_variant(&base)?;
    let refused = |changed: Profile| require_calibration_only_variant(&base, &changed).is_err();
    // A graph change.
    let mut graph = variant.clone();
    graph.device_execution.as_mut().context("device")?.graph = Sha256Digest::from_bytes([5; 32]);
    assert!(refused(graph));
    // A non-calibration asset change.
    let mut asset = variant.clone();
    let other = asset
        .assets
        .iter_mut()
        .find(|asset| asset.role != AssetRole::Calibration)
        .context("non-calibration asset")?;
    other.sha256 = Sha256Digest::from_bytes([6; 32]);
    assert!(refused(asset));
    // Any other field, a qualified profile, a public id, a CPU profile.
    let mut tokens = variant.clone();
    tokens.max_tokens += 1;
    assert!(refused(tokens));
    let mut qualified = variant.clone();
    qualified.qualification = base_qualification()?;
    assert!(refused(qualified));
    let mut public = variant.clone();
    public.profile_id = "arbiter-context".into();
    assert!(refused(public));
    let mut cpu = variant.clone();
    cpu.device_execution = None;
    assert!(refused(cpu));
    // An identity alias: same calibration, or the base's own manifest.
    let mut alias = base.clone();
    alias.manifest = Sha256Digest::from_bytes([4; 32]);
    alias.profile_id = variant.profile_id.clone();
    assert!(refused(alias));
    let mut same_manifest = variant.clone();
    same_manifest.manifest = base.manifest;
    assert!(refused(same_manifest));
    // An unreviewed base: the reviewed result names another profile.
    assert!(reviewed_profile(Sha256Digest::from_bytes([8; 32]), Some(&base), &variant).is_err());
    // A base that is itself not a reviewed private CUDA profile.
    let mut public_base = base.clone();
    public_base.profile_id = "arbiter-context".into();
    assert!(require_calibration_only_variant(&public_base, &variant).is_err());
    Ok(())
}

fn base_qualification() -> Result<Option<Sha256Digest>> {
    let profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    Ok(profile.qualification)
}