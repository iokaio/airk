// SPDX-License-Identifier: Apache-2.0
//! Pure native policy/metadata cases. No environment mutation or model invocation.
use super::*;
#[cfg(feature = "test-support")]
#[test]
fn private_context_probe_calibration_names_the_converted_assets() {
    use airk_core::arbiter::{Asset, AssetRole};
    let assets = [
        ("model.onnx", AssetRole::Model, 1),
        ("weights.bin", AssetRole::ExternalWeights, 2),
        ("tokenizer.json", AssetRole::Tokenizer, 3),
        ("calibration.json", AssetRole::Calibration, 4),
    ]
    .map(|(name, role, byte)| Asset {
        name: name.into(),
        role,
        bytes: 100,
        sha256: Sha256Digest::from_bytes([byte; 32]),
    });
    let bindings = context_probe_asset_bindings(&assets);
    assert_eq!(
        bindings.keys().map(String::as_str).collect::<Vec<_>>(),
        ["model.onnx", "tokenizer.json", "weights.bin"]
    );
    assert_eq!(bindings["model.onnx"].role, AssetRole::Model);
    assert_eq!(bindings["weights.bin"].role, AssetRole::ExternalWeights);
    assert_eq!(bindings["weights.bin"].sha256.as_bytes(), &[2; 32]);
    assert_eq!(bindings["tokenizer.json"].sha256.as_bytes(), &[3; 32]);
}

fn env(gemm: Option<&str>, debug: Option<&str>) -> MathEnvironment {
    MathEnvironment {
        gemm: gemm.map(str::to_owned),
        debug: debug.map(str::to_owned),
        unicode: true,
    }
}
#[test]
fn inherited_exact_settings_pass_and_late_setter_or_drift_refuses() {
    let allowed = env(Some("2"), Some("1"));
    assert!(check_environment(&allowed, &allowed, true).is_ok());
    for initial in [
        env(None, None),
        env(Some("0"), Some("1")),
        env(Some("2"), None),
    ] {
        assert!(check_environment(&initial, &allowed, true).is_err());
    }
    for current in [
        env(Some("3"), Some("1")),
        env(Some("2"), Some("0")),
        MathEnvironment {
            unicode: false,
            ..allowed.clone()
        },
    ] {
        assert!(check_environment(&allowed, &current, true).is_err());
    }
}
#[test]
fn fp32_policy_remains_distinct_from_each_model_fp16_policy() -> Result<()> {
    let f32 = math_policy_digest();
    let f32provider = provider_options_digest();
    let mut identities = BTreeSet::new();
    for model in [
        ModelChoice::FocusQwenReranker06B,
        ModelChoice::ContextQwenCompact,
        ModelChoice::ContextQwenLarge,
    ] {
        let half = fp16_math_policy_digest(model)?;
        assert_ne!(f32, half);
        assert!(identities.insert(half));
        assert_ne!(f32provider, fp16_provider_options_digest(model)?);
    }
    assert!(fp16_math_policy_digest(ModelChoice::SmallIntern).is_err());
    Ok(())
}
fn input(tokens: usize) -> super::super::super::CompiledInput {
    super::super::super::CompiledInput {
        prompt: "literal synthetic".into(),
        input_ids: vec![1; tokens],
        attention_mask: vec![1; tokens],
        position_ids: (0..tokens as i64).collect(),
        decision_position: tokens as i64 - 1,
        candidate_positions: vec![32, 33],
        option_mask: vec![true, true],
        input_digest: Sha256Digest::from_bytes([7; 32]),
    }
}
#[test]
fn context_boundaries_and_exact_original_mask_positions_refuse_unsupported_input() -> Result<()> {
    for tokens in [1, 17, 128, 8192] {
        validate_geometry(ModelChoice::ContextQwenCompact, &input(tokens))?;
        validate_geometry(ModelChoice::ContextQwenLarge, &input(tokens))?;
    }
    for tokens in [0, 8193] {
        assert!(validate_geometry(ModelChoice::ContextQwenCompact, &input(tokens)).is_err());
    }
    let mut bad = input(17);
    bad.attention_mask[0] = 0;
    assert!(validate_geometry(ModelChoice::ContextQwenCompact, &bad).is_err());
    bad.attention_mask[0] = 1;
    bad.position_ids[0] = 1;
    assert!(validate_geometry(ModelChoice::ContextQwenCompact, &bad).is_err());
    Ok(())
}
#[test]
fn focus_all_live_and_extreme_left_pad_pass_without_renumbering() -> Result<()> {
    for prefix in [0, 1, 511] {
        let mut row = input(512);
        row.attention_mask[..prefix].fill(0);
        validate_geometry(ModelChoice::FocusQwenReranker06B, &row)?;
        assert_eq!(row.position_ids[511], 511);
    }
    let mut bad = input(512);
    bad.attention_mask[111] = 0;
    assert!(validate_geometry(ModelChoice::FocusQwenReranker06B, &bad).is_err());
    bad.attention_mask.fill(0);
    assert!(validate_geometry(ModelChoice::FocusQwenReranker06B, &bad).is_err());
    assert!(validate_geometry(ModelChoice::FocusQwenReranker06B, &input(128)).is_err());
    Ok(())
}
#[test]
fn half_initializer_bit_loading_preserves_half_values_without_rounding() {
    let bits = [0x0000u16, 0x8000, 0x0001, 0x3c00, 0x7bff];
    let bytes = bits
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect::<Vec<_>>();
    let values = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|x| half::f16::from_bits(u16::from_le_bytes(*x)))
        .collect::<Vec<_>>();
    assert_eq!(values.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), bits);
}

#[test]
fn actual_final_placement_needs_exact_complete_attention_roster() -> Result<()> {
    let model = ModelChoice::ContextQwenCompact;
    let nodes = (0..28)
        .map(|i| crate::arbiter::external_data::g5::AttentionNode {
            name: format!("/airk_g5/attention/{i}/efficient"),
            operator: "GroupQueryAttention".into(),
            domain: "com.microsoft".into(),
        })
        .collect::<Vec<_>>();
    let plan = AttentionPlan {
        schema: "airk.native.cuda-fp16-attention-plan/1",
        graph: Sha256Digest::from_bytes([3; 32]),
        model,
        operator: "GroupQueryAttention",
        domain: "com.microsoft",
        dtype: "fp16",
        backend: "FLASH_ATTENTION",
        math_policy: fp16_math_policy_digest(model)?,
        provider_options: fp16_provider_options_digest(model)?,
        nodes,
    };
    let base = PlacementObservation {
        scope: "literal synthetic final SessionState",
        complete: true,
        shim_status: 0,
        ort_error_code: 0,
        reported_nodes: 28,
        reported_string_bytes: 1024,
        nodes: (0..28)
            .map(|i| PlacedNode {
                subgraph_index: 0,
                name: format!("/airk_g5/attention/{i}/efficient"),
                domain: "com.microsoft".into(),
                op: "GroupQueryAttention".into(),
                ep: "CUDAExecutionProvider".into(),
            })
            .collect(),
    };
    plan.require_placement(&base)?;
    for case in 0..7 {
        let mut bad = base.clone();
        match case {
            0 => bad.nodes.clear(),
            1 => {
                bad.nodes.pop();
            }
            2 => bad.nodes[1].name = bad.nodes[0].name.clone(),
            3 => bad.nodes[1].name = "foreign-attention".into(),
            4 => bad.nodes[1].ep = "CPUExecutionProvider".into(),
            5 => bad.nodes[1].subgraph_index = 1,
            _ => bad.complete = false,
        };
        assert!(plan.require_placement(&bad).is_err(), "literal case{case}");
    }
    Ok(())
}

#[test]
fn qualified_product_math_does_not_force_attention_debug_logging() {
    let allowed = env(Some("2"), None);
    assert!(check_environment(&allowed, &allowed, false).is_ok());
    assert!(check_environment(&allowed, &allowed, true).is_err());
}

/// Compare the two existing admission decisions using a retained, unqualified native profile.
#[cfg(all(
    target_arch = "x86_64",
    any(windows, all(target_os = "linux", target_env = "gnu"))
))]
#[test]
#[ignore = "campaign original Compact descriptor required; no CUDA call"]
fn original_compact_bounded_options_refused_by_strict_input_plan() -> Result<()> {
    let root = std::env::var_os("AIRK_PRIVATE_CAMPAIGN").context("original campaign required")?;
    let path = std::path::Path::new(&root)
        .join("compact-fp16-projection-baseline-metadata-r5/metadata/profile.json");
    let raw = std::fs::read(path)?;
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "2d37b06bb7ee6c5813acedd14b0b98ca305153466a6d467609b37373290da211"
    );
    let mut profile: Profile = serde_json::from_slice(&raw)?;
    assert_eq!(profile.model, ModelChoice::ContextQwenCompact);
    let model = profile.model;
    let device = profile.device_execution.as_mut().unwrap();
    device.provider_options = fp16_provider_options_digest(model)?;
    device.math_policy = fp16_math_policy_digest(model)?;
    let strict = AttentionPlan::from_graph(&profile, &[], &BTreeMap::new()).unwrap_err();
    assert_eq!(
        strict.to_string(),
        "G5 actual graph differs from immutable descriptor"
    );

    let bounded = super::super::BoundedContextOptimizer::ProtectedProjections;
    let device = profile.device_execution.as_mut().unwrap();
    device.placement = PlacementPolicy::BoundedHybrid {
        policy: Sha256Digest::from_bytes([1; 32]),
    };
    device.provider_options = bounded.provider_options(model)?;
    device.math_policy = bounded.math_policy(model)?;
    assert_eq!(
        super::super::BoundedContextOptimizer::from_profile(&profile)?,
        bounded
    );
    let refused = AttentionPlan::from_graph(&profile, &[], &BTreeMap::new()).unwrap_err();
    assert_eq!(
        refused.to_string(),
        "G5 profile math/provider identity differs"
    );
    Ok(())
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn protected_efficient_mha_binds_the_model_graph_and_half_weight_identity() -> Result<()> {
    use airk_core::arbiter::{Asset, AssetRole};
    for (model, graph, weights, weight_bytes) in [
        (
            ModelChoice::ContextQwenCompact,
            "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9",
            "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3",
            3_441_397_760,
        ),
        (
            ModelChoice::ContextQwenLarge,
            "c1b5776144c6d9af07d08552cae40e491d8f866e89c53bab567adeeb32ca75e4",
            "5b053f02795e72ff88123b28f89d3f2c89da055e9ed2228bc60809dd0a2b4270",
            8_045_328_384,
        ),
    ] {
        let mut profile = super::super::super::catalog_profiles()?
            .into_iter()
            .find(|profile| profile.model == model && profile.cpu_fit.is_some())
            .context("Context CPU fixture profile missing")?;
        let asset = Asset {
            name: "weights.bin".into(),
            role: AssetRole::ExternalWeights,
            bytes: weight_bytes,
            sha256: weights.parse()?,
        };
        profile
            .assets
            .retain(|asset| asset.role != AssetRole::ExternalWeights);
        profile.assets.push(asset.clone());
        let recipe = BoundedContextOptimizer::ProtectedEfficientMha;
        profile.device_execution = Some(DeviceExecution {
            schema: DEVICE_EXECUTION_SCHEMA.into(),
            backend: GpuBackend::Cuda,
            runtime_build: Sha256Digest::from_bytes([1; 32]),
            compatibility_policy: Sha256Digest::from_bytes([2; 32]),
            provider_options: recipe.provider_options(model)?,
            math_policy: recipe.math_policy(model)?,
            precision: DevicePrecision::Float16,
            tf32: false,
            placement: PlacementPolicy::BoundedHybrid {
                policy: Sha256Digest::from_bytes([3; 32]),
            },
            graph: graph.parse()?,
            host_fit: profile.cpu_fit.take().context("Context CPU fit missing")?,
            memory: DeviceMemory::Dedicated {
                required_peak_bytes: 1,
                headroom_bytes: 1,
            },
        });
        assert!(is_protected_efficient_mha(&profile)?);
        assert_ne!(
            recipe.provider_options(model)?,
            BoundedContextOptimizer::ProtectedProjections.provider_options(model)?
        );
        let check = |profile: &Profile| {
            AttentionPlan::from_graph(profile, b"foreign graph", &BTreeMap::new())
                .unwrap_err()
                .to_string()
        };
        assert_eq!(
            check(&profile),
            "protected efficient MHA graph artifact differs"
        );
        profile.assets.last_mut().unwrap().sha256 = Sha256Digest::from_bytes([9; 32]);
        assert_eq!(
            check(&profile),
            "protected efficient MHA weight artifact differs"
        );
        profile.assets.pop();
        profile.assets.extend([asset.clone(), asset]);
        assert_eq!(
            check(&profile),
            "protected efficient MHA weight artifact differs"
        );
        let device = profile.device_execution.as_mut().unwrap();
        device.placement = PlacementPolicy::StrictGpu;
        assert!(!is_protected_efficient_mha(&profile)?);
        assert_eq!(check(&profile), "G5 profile math/provider identity differs");
    }
    Ok(())
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn protected_efficient_mha_requires_every_model_specific_cuda_attention_and_projection()
-> Result<()> {
    for (model, prefix, layers) in [
        (ModelChoice::ContextQwenCompact, "airk_compact_cutlass", 28),
        (ModelChoice::ContextQwenLarge, "airk_large_cutlass", 36),
    ] {
        let candidate = ProtectedContextMha::for_model(model)?;
        let plan = AttentionPlan {
            schema: "airk.native.cuda-fp16-attention-plan/1",
            graph: candidate.graph.parse()?,
            model,
            operator: "MultiHeadAttention",
            domain: "com.microsoft",
            dtype: "fp32",
            backend: "EFFICIENT_ATTENTION",
            math_policy: Sha256Digest::from_bytes([1; 32]),
            provider_options: Sha256Digest::from_bytes([2; 32]),
            nodes: candidate.attention_nodes(),
        };
        let mut nodes = Vec::new();
        for layer in 0..layers {
            nodes.push(PlacedNode {
                subgraph_index: 0,
                name: format!("/{prefix}/{layer}/efficient"),
                domain: "com.microsoft".into(),
                op: "MultiHeadAttention".into(),
                ep: "CUDAExecutionProvider".into(),
            });
            for site in [
                "self_attn/q_proj",
                "self_attn/k_proj",
                "self_attn/v_proj",
                "self_attn/o_proj",
                "mlp/gate_proj",
                "mlp/up_proj",
                "mlp/down_proj",
            ] {
                nodes.push(PlacedNode {
                    subgraph_index: 0,
                    name: format!("/scorer/layers.{layer}/{site}/MatMul"),
                    domain: String::new(),
                    op: "MatMul".into(),
                    ep: "CUDAExecutionProvider".into(),
                });
            }
        }
        let placement = PlacementObservation {
            scope: "literal protected Context assignment",
            complete: true,
            shim_status: 0,
            ort_error_code: 0,
            reported_nodes: nodes.len() as u64,
            reported_string_bytes: 1024,
            nodes,
        };
        plan.require_attention_placement(&placement)?;
        let mut bad = placement.clone();
        bad.nodes[0].op = "GroupQueryAttention".into();
        assert!(plan.require_attention_placement(&bad).is_err());
        let mut bad = placement.clone();
        bad.nodes.last_mut().unwrap().ep = "CPUExecutionProvider".into();
        assert!(plan.require_attention_placement(&bad).is_err());
        let mut bad = placement.clone();
        bad.nodes.pop();
        assert!(plan.require_attention_placement(&bad).is_err());
        let mut bad = placement;
        bad.complete = false;
        assert!(plan.require_attention_placement(&bad).is_err());
    }
    Ok(())
}
