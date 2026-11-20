// SPDX-License-Identifier: Apache-2.0
//! Private four-width Context FP32 CUDA profiling with exact reference inputs.
use super::*;
use serde_json::json;

const PROOF_BYTES: u64 = 16 << 20;
const PROFILE_BYTES: u64 = 64 << 20;
const WIDTHS: [usize; 4] = [32, 128, 1024, 8192];
const GRAPH_SHA: &str = "2f4d3211e0bbf097c702acf90dcce046552a3e2d75dc49b2f5945d7988c76fee";
const RETAINED_SHA: &str = "3962186640b42648b345fa200516ed44a3de4ccab0d783e671c9c22c3b8b1bbd";
const SUPPLEMENTAL_SHA: &str = "c4360d8a759c68c305fba0df7bd04001a6b043d8a2a57b36f0d8ef0f898519b9";
const CAPACITY_SHA: &str = "071891ef57f12067bd386ccb0230f5018eb2a1143109d6aeeb0db731eab50586";
const FAILED_CAPACITY_SHA: &str =
    "e3fd8d525bd0f7abc7eae08b154dd9410c02a6ee6d27d475bce7af3b4814ef4f";

struct Experiment {
    model: ModelChoice,
    host_gib: u64,
    device_gib: u64,
    graph: &'static str,
    retained: &'static str,
    supplemental: &'static str,
    receipt_schema: &'static str,
    equivalent_predecessor: bool,
    fp16_t32: bool,
    projection_capture: bool,
}

const COMPACT: Experiment = Experiment {
    model: ModelChoice::ContextQwenCompact,
    host_gib: 32,
    device_gib: 24,
    graph: GRAPH_SHA,
    projection_capture: false,
    retained: RETAINED_SHA,
    supplemental: SUPPLEMENTAL_SHA,
    receipt_schema: "airk.private.cuda-compact-bounded-hybrid-profile/1",
    fp16_t32: false,
    equivalent_predecessor: false,
};

const LARGE: Experiment = Experiment {
    model: ModelChoice::ContextQwenLarge,
    host_gib: 48,
    device_gib: 44,
    graph: "1892e5b040d8c51969f1d3361d0929950fa98f0ef4079060e48d00458635488a",
    projection_capture: false,
    retained: "c87c4edcdec2ab5e98b41bfc186c3bb7d60791e7b9efd0aea186b731eecc3dd2",
    supplemental: "020669f0c44a959a0d95798eb3cc52925513e7e549496368017e0827a398aaef",
    receipt_schema: "airk.private.cuda-large-bounded-hybrid-profile/1",
    fp16_t32: false,
    equivalent_predecessor: false,
};

const BATCH_VIEW: Experiment = Experiment {
    graph: "d4ccefff117a11c68b92c3f9c08b145a4fbb5a470114af5a23107c73a26bc431",
    receipt_schema: "airk.private.cuda-large-batch-view-bounded-hybrid-profile/1",
    equivalent_predecessor: true,
    ..LARGE
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capacity {
    schema: String,
    failed_native_result_sha256: Sha256Digest,
    device_limit_bytes: u64,
    arena_limit_bytes: u64,
    warm_extra_bytes: u64,
    headroom_bytes: u64,
}

fn capacity(raw: &[u8], approved_device_bytes: u64) -> Result<(Capacity, DeviceCapacityPlan)> {
    let value: Capacity = serde_json::from_slice(raw)?;
    ensure!(
        value.schema == "airk.private.compact-cuda-arena-policy/1"
            && value.failed_native_result_sha256 == FAILED_CAPACITY_SHA.parse()?
            && value.device_limit_bytes == approved_device_bytes
            && value.device_limit_bytes == 24 << 30
            && value.arena_limit_bytes == 22 << 30
            && value.warm_extra_bytes == 16 << 30
            && value.headroom_bytes == 2 << 30
            && value.arena_limit_bytes.checked_add(value.headroom_bytes)
                == Some(approved_device_bytes),
        "private Compact arena policy differs from the reviewed 24 GiB envelope"
    );
    let plan = DeviceCapacityPlan::probe(
        value.arena_limit_bytes,
        value.warm_extra_bytes,
        value.headroom_bytes,
    )?;
    Ok((value, plan))
}

fn exact_pin(key: &str, expected: &str) -> Result<ResourceFilePin> {
    let pin = env_pin(key, &format!("{key}_SHA256"))?;
    ensure!(
        pin.sha256 == expected.parse()?,
        "reviewed input differs: {key}"
    );
    Ok(pin)
}

fn reference_arrays(row: &NumericCase) -> Result<[&[f32]; 2]> {
    let finite = |values: &[f32]| {
        values.len() == row.input.candidate_positions.len()
            && values.iter().all(|value| value.is_finite())
    };
    match (
        row.reference_same_artifact.as_deref(),
        row.reference_original_fp32.as_deref(),
    ) {
        (Some(same), Some(original)) if finite(same) && finite(original) => Ok([same, original]),
        _ => anyhow::bail!("both complete, finite independent Context references required"),
    }
}

fn validate_reference_producer(
    fixture: &NumericFixture,
    profile: &Profile,
) -> Result<ResourceFilePin> {
    let pin = fixture
        .reference_producer
        .clone()
        .context("independent producer missing")?;
    let producer: Value = serde_json::from_slice(&pinned(&pin, PROOF_BYTES)?)?;
    let rows: Vec<NumericCase> = serde_json::from_value(producer["rows"].clone())?;
    let assets = profile
        .assets
        .iter()
        .filter(|asset| {
            matches!(
                asset.role,
                AssetRole::Model | AssetRole::ExternalWeights | AssetRole::Tokenizer
            )
        })
        .collect::<Vec<_>>();
    ensure!(
        producer["schema"] == "airk.arbiter.independent-native-numeric-reference/2"
            && producer["qualification"] == false
            && producer["model"] == json!(fixture.model)
            && producer["compiler"] == json!(fixture.compiler)
            && producer["resource_policy_digest"] == json!(fixture.resource_policy)
            && producer["candidate_assets"] == json!(assets)
            && producer["original_reference_assets"] == json!(fixture.original_reference_assets)
            && !fixture.original_reference_assets.is_empty()
            && json!(rows) == json!(fixture.rows),
        "independent reference producer differs from exact current neural assets or tensors"
    );
    Ok(pin)
}

struct ReferenceLineage {
    request: ResourceFilePin,
    equivalence: ResourceFilePin,
    native_input: ResourceFilePin,
    profile_pin: ResourceFilePin,
    profile: Profile,
}

fn equivalent_predecessor(current: &Profile, input: &ResourceRunInput) -> Result<ReferenceLineage> {
    const ORIGINAL_GRAPH: &str = "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94";
    const CORRECTED_GRAPH: &str =
        "5c9a26693eec8522bbfb4d5c4445fa0b60684449a5ea2e1098ec71cca96405b1";
    let request = exact_pin(
        "AIRK_CUDA_HYBRID_REFERENCE_REQUEST",
        "9a7daee2c118030c979852efc6780f011356af320e0c3fe0a6d1d9a2a0ab33b0",
    )?;
    let equivalence = exact_pin(
        "AIRK_CUDA_HYBRID_REFERENCE_EQUIVALENCE",
        "404a486500b1c3955cc5616554ccf2f7c55cd4855981d4477265688db41278d1",
    )?;
    let proof: Value = serde_json::from_slice(&pinned(&equivalence, 4096)?)?;
    let predecessor: Request = serde_json::from_slice(&pinned(&request, 1 << 20)?)?;
    let Request::Probe { gpu_input, .. } = predecessor else {
        anyhow::bail!("the retained original Large reference Probe is required");
    };
    let (old_input, original, _, _) = inputs_from(&gpu_input)?;
    ensure!(
        proof["schema"] == "airk.private.large-fp32-batch-view-reference-lineage/1"
            && proof["predecessor_graph"] == ORIGINAL_GRAPH
            && proof["candidate_graph"] == CORRECTED_GRAPH
            && proof["comparison_basis"] == "equivalent_predecessor"
            && proof["exact_inverse"] == true
            && proof["operand_order_unchanged"] == true
            && proof["verified_mask_adds"] == 36
            && proof["native_scoring_dispatched"] == false
            && original.manifest.to_string()
                == "eb467966e528b3b34f0c95d26bdcb0e3f404267b5dcaa673cc6c56755b85905b"
            && current.manifest.to_string()
                == "7e4481caaf7034a70779702edad73a0eb08321418d1b785a4b7f4f891522bdac"
            && original.model == current.model
            && original.compiler == current.compiler
            && original.abi == current.abi
            && old_input.policy_digest == input.policy_digest,
        "batch-view input, original reference identity or exact inverse differs"
    );
    let preserved = |profile: &Profile| {
        profile
            .assets
            .iter()
            .filter(|asset| !matches!(asset.role, AssetRole::Model | AssetRole::Calibration))
            .cloned()
            .collect::<Vec<_>>()
    };
    ensure!(
        preserved(&original) == preserved(current)
            && original
                .device_execution
                .as_ref()
                .map(|d| d.graph.to_string())
                .as_deref()
                == Some(ORIGINAL_GRAPH)
            && current
                .device_execution
                .as_ref()
                .map(|d| d.graph.to_string())
                .as_deref()
                == Some(CORRECTED_GRAPH),
        "batch-view comparison requires unchanged external weights, tokenizer and other assets"
    );
    Ok(ReferenceLineage {
        request,
        equivalence,
        native_input: gpu_input,
        profile_pin: old_input.profile,
        profile: original,
    })
}

pub(super) struct ProfileReferences {
    pub(super) rows: Vec<NumericCase>,
    pub(super) provenance: Value,
    pub(super) pins: Vec<(ResourceFilePin, u64)>,
}

fn fp32_references(
    experiment: &Experiment,
    profile: &Profile,
    input: &ResourceRunInput,
) -> Result<ProfileReferences> {
    let retained_pin = exact_pin("AIRK_CUDA_HYBRID_RETAINED", experiment.retained)?;
    let supplementary_pin = exact_pin("AIRK_CUDA_HYBRID_SUPPLEMENTAL", experiment.supplemental)?;
    let retained: NumericFixture = serde_json::from_slice(&pinned(&retained_pin, PROOF_BYTES)?)?;
    let supplementary: NumericFixture =
        serde_json::from_slice(&pinned(&supplementary_pin, PROOF_BYTES)?)?;
    numeric_shape(&retained, profile, &input.policy)?;
    ensure!(
        supplementary.schema == retained.schema
            && supplementary.model == retained.model
            && supplementary.compiler == retained.compiler
            && supplementary.resource_policy == retained.resource_policy,
        "supplemental Context references use a different native scoring identity"
    );
    numeric_rows(&supplementary.rows, &[(32, 8), (1024, 8)], false)?;
    let lineage = if experiment.equivalent_predecessor {
        Some(equivalent_predecessor(profile, input)?)
    } else {
        None
    };
    let reference_profile = lineage.as_ref().map_or(profile, |proof| &proof.profile);
    let retained_producer = validate_reference_producer(&retained, reference_profile)?;
    let supplementary_producer = validate_reference_producer(&supplementary, reference_profile)?;
    let rows = vec![
        supplementary.rows[0].clone(),
        retained.rows[1].clone(),
        supplementary.rows[1].clone(),
        retained.rows[2].clone(),
    ];
    let mut provenance = json!({"retained_fixture": retained_pin, "supplemental_fixture": supplementary_pin,
        "retained_producer": retained_producer, "supplemental_producer": supplementary_producer});
    let mut pins = vec![
        (retained_pin, PROOF_BYTES),
        (supplementary_pin, PROOF_BYTES),
        (retained_producer, PROOF_BYTES),
        (supplementary_producer, PROOF_BYTES),
    ];
    if let Some(lineage) = lineage {
        provenance["reference_basis"] = json!("equivalent_predecessor");
        provenance["reference_request"] = json!(lineage.request);
        provenance["reference_equivalence"] = json!(lineage.equivalence);
        pins.extend(
            [
                lineage.request,
                lineage.equivalence,
                lineage.native_input,
                lineage.profile_pin,
            ]
            .map(|pin| (pin, 1 << 20)),
        );
    }
    Ok(ProfileReferences {
        rows,
        provenance,
        pins,
    })
}

fn score_rows(
    scorer: &mut airk_retrieval::arbiter::runtime::BoundedHybridScorer,
    rows: &[NumericCase],
    operations: &mut operation::Journal,
    receipt: &mut Value,
    file: &mut File,
    deadline: Instant,
    experiment: &Experiment,
) -> Result<()> {
    for (index, row) in rows.iter().enumerate() {
        let [same, original] = reference_arrays(row)?;
        let timed = operations.observe(
            operation::Kind::Numeric,
            index + 1,
            Some(row.input.input_digest),
            || {
                if experiment.projection_capture {
                    scorer.score_large_projection_taps(&row.input, deadline).map(|(s,t)| (s,Some(t)))
                } else {
                    scorer.score_with_deadline(&row.input, deadline).map(|s| (s,None))
                }
            },
        )?;
        let (actual, taps) = timed.value?;
        if let Some(taps) = taps {
            if matches!(experiment.receipt_schema, ATTENTION17_SCHEMA | DENSE17_SCHEMA) {
                receipt["layer17_taps"] = large_block17_words(experiment.receipt_schema,taps)?;
            } else if experiment.graph == "0d00e2fabdb5fe54474041fb1f2787502349b78ce40436b955cd4433633a4462" {
                receipt["hidden_taps"] = large_hidden_words(taps)?;
            } else {
                receipt["layer0_taps"] = large_projection_words(taps)?;
            }
            receipt["layer0_final_graph"] = json!(scorer.graph_metadata());
        }
        let (same_raw, same_mass, same_argmax) = comparison(&actual, same)?;
        let (original_raw, original_mass, original_argmax) = comparison(&actual, original)?;
        let mut observed = json!({
            "name": row.name, "tokens": row.input.input_ids.len(), "input_digest": row.input.input_digest,
            "raw_scores": actual, "elapsed_us": timed.elapsed_us, "native_call_elapsed_us": timed.native_call_elapsed_us,
            "original_fp32_raw_error": original_raw, "original_fp32_mass_error": original_mass,
            "argmax_matches": same_argmax && original_argmax,
        });
        let (raw, mass) = if experiment.fp16_t32 {
            observed["development_screen_pass"] = json!(
                same_raw <= 0.07 && same_mass <= 0.05 && original_mass <= 0.05 && same_argmax
            );
            observed["matching_fp16_argmax_matches"] = json!(same_argmax);
            observed["original_fp32_argmax_matches"] = json!(original_argmax);
            observed["matching_reference_0_001"] = json!(same_raw <= 0.001);
            ("matching_fp16_raw_error", "matching_fp16_mass_error")
        } else if experiment.equivalent_predecessor {
            (
                "equivalent_predecessor_raw_error",
                "equivalent_predecessor_mass_error",
            )
        } else {
            ("same_artifact_raw_error", "same_artifact_mass_error")
        };
        observed[raw] = json!(same_raw);
        observed[mass] = json!(same_mass);
        receipt["rows"].as_array_mut().unwrap().push(observed);
        flush_graph_diagnostic(file, receipt)?;
        ensure!(
            timed.within_deadline
                && (experiment.fp16_t32
                    || (same_raw <= 0.001
                        && original_raw <= 0.001
                        && same_mass <= 0.05
                        && original_mass <= 0.05
                        && same_argmax
                        && original_argmax)),
            "Context scientific forward or original deadline refused at T{}",
            row.input.input_ids.len()
        );
    }
    Ok(())
}

#[test]
#[ignore = "owned Windows CUDA job, reviewed complete graph, and exact independent Compact references"]
fn cuda_compact_fp32_bounded_hybrid_profile() -> Result<()> {
    profile_context(COMPACT)
}

#[test]
#[ignore = "owned Windows 48 GiB job, reviewed Large graph, and exact independent references"]
fn cuda_large_fp32_bounded_hybrid_profile() -> Result<()> {
    profile_context(LARGE)
}

#[test]
#[ignore = "owned Windows 48 GiB job, reviewed corrected graph and independent predecessor references"]
fn cuda_large_fp32_batch_view_bounded_hybrid_profile() -> Result<()> {
    profile_context(BATCH_VIEW)
}

#[test]
#[ignore = "owned Windows Large FP16 T32 engineering comparison against independent CPU"]
fn cuda_large_fp16_scientific_t32() -> Result<()> {
    profile_context(Experiment {
        fp16_t32: true,
        graph: "fff6dbe2f0c2fac4b9ba215da7d5029421d2706ee48c01c2485b9f157ee15ca6",
        receipt_schema: "airk.private.large-fp16-scientific-t32/1",
        ..LARGE
    })
}

const BODY_GRAPH_OBSERVATION: &str = "96641f4cf15ddd6cf1209402eae4ed366b9f77f00bfc28f1c894e67143ca4a16";

#[test]
#[ignore = "owned Windows Large all-body FP32 accumulation against original Flash T32 references"]
fn cuda_large_body_fp32_scientific_t32() -> Result<()> {
    profile_context(Experiment {
        fp16_t32: true,
        graph: BODY_GRAPH_OBSERVATION,
        receipt_schema: "airk.private.large-body252-fp32-scientific-t32/1",
        ..LARGE
    })
}

#[test]
#[ignore = "owned original Windows Large protected Q/K/V FP32 T32 comparison"]
fn cuda_large_qkv_fp32_scientific_t32() -> Result<()> {
    profile_context(Experiment {
        fp16_t32: true,
        graph: "0d00e2fabdb5fe54474041fb1f2787502349b78ce40436b955cd4433633a4462",
        receipt_schema: "airk.private.large-qkv-fp32-scientific-t32/1",
        ..LARGE
    })
}

#[test]
#[ignore = "owned Windows Large protected hidden/readout operand capture"]
fn cuda_large_qkv_hidden_t32() -> Result<()> {
    profile_context(Experiment {
        fp16_t32: true, projection_capture: true, graph: "0d00e2fabdb5fe54474041fb1f2787502349b78ce40436b955cd4433633a4462",
        receipt_schema: "airk.private.large-qkv-hidden-t32/1",
        ..LARGE
    })
}

const ATTENTION17_SCHEMA: &str = "airk.private.large-qkv-attention17-t32/1";
const DENSE17_SCHEMA: &str = "airk.private.large-qkv-dense17-t32/1";

fn large_block17_words(schema: &str, words: Vec<Vec<u16>>) -> Result<Value> {
    let attention = [("q",4096,32),("k",1024,32),("v",1024,32),("attention",4096,32)];
    let dense = [("o_input",4096,1),("o_output",2560,1),("down_input",9728,1),("down_output",2560,1)];
    let roster = match schema {
        ATTENTION17_SCHEMA => attention,
        DENSE17_SCHEMA => dense,
        _ => anyhow::bail!("unknown Large layer17 operand set"),
    };
    ensure!(words.len() == roster.len(), "four Large layer17 operands required");
    let mut result = Vec::new();
    for ((name,width,tokens), bits) in roster.into_iter().zip(words) {
        ensure!(bits.len() == width*tokens && bits.iter().all(|v| v & 0x7c00 != 0x7c00),
                "Large layer17 operand size or binary16 finiteness differs");
        result.push(json!({"name":name,"width":width,"tokens":tokens,"bits":bits}));
    }
    Ok(json!(result))
}

#[test]
fn large_layer17_roster_refuses_truncated_key_history() {
    assert!(large_block17_words(ATTENTION17_SCHEMA,
        vec![vec![0;32*4096],vec![0;1024],vec![0;32*1024],vec![0;32*4096]]).is_err());
}

#[test]
#[ignore = "owned Windows Large protected Q/K/V layer17 Flash operands"]
fn cuda_large_qkv_attention17_t32() -> Result<()> {
    profile_context(Experiment { fp16_t32: true, projection_capture: true,
        graph: "0d00e2fabdb5fe54474041fb1f2787502349b78ce40436b955cd4433633a4462",receipt_schema:ATTENTION17_SCHEMA,..LARGE })
}

#[test]
#[ignore = "owned Windows Large protected Q/K/V layer17 dense operands"]
fn cuda_large_qkv_dense17_t32() -> Result<()> {
    profile_context(Experiment { fp16_t32: true, projection_capture: true,
        graph: "0d00e2fabdb5fe54474041fb1f2787502349b78ce40436b955cd4433633a4462",receipt_schema:DENSE17_SCHEMA,..LARGE })
}

fn large_hidden_words(words: Vec<Vec<u16>>) -> Result<Value> {
    let names = ["/scorer/layers.0/input_layernorm/Mul_1_output_0",
        "/scorer/layers.17/input_layernorm/Mul_1_output_0", "/scorer/layers.35/Add_1_output_0",
        "/scorer/norm/Mul_1_output_0"];
    ensure!(words.len() == names.len(), "four original FP32 decision rows required");
    let mut result = Vec::new();
    for (name, bits) in names.into_iter().zip(words) {
        ensure!(bits.len() == 5120 && bits.as_chunks::<2>().0.iter().all(|w|
            f32::from_bits(u32::from(w[0]) | (u32::from(w[1]) << 16)).is_finite()),
            "original Large hidden row requires 2560 finite FP32 words");
        result.push(json!({"name":name, "token":31, "width":2560, "bits":bits}));
    }
    Ok(json!(result))
}

#[test]
fn large_hidden_rows_require_finite_fp32_pairs() {
    let mut words = vec![vec![0; 5120]; 4];
    assert!(large_hidden_words(words.clone()).is_ok());
    words[2][1] = 0x7fc0;
    assert!(large_hidden_words(words).is_err());
}

fn large_projection_words(words: Vec<Vec<u16>>) -> Result<Value> {
    const ROSTER: [(&str, usize); 4] = [
        ("/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/input16", 2560),
        ("/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/output16", 1024),
        ("/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16", 1024),
        ("/airk_g5/weight//scorer/layers.0/self_attn/q_proj/MatMul/output16", 4096),
    ];
    ensure!(words.len() == ROSTER.len(), "four original Large projection tensors required");
    let mut taps = Vec::with_capacity(ROSTER.len());
    for ((name, width), bits) in ROSTER.into_iter().zip(words) {
        ensure!(bits.len() == 32 * width && bits.iter().all(|word| *word & 0x7c00 != 0x7c00),
            "original finite Large T32 tap dimensions differ: {name}");
        taps.push(json!({"name":name,"width":width,"bits":bits}));
    }
    Ok(json!(taps))
}

#[test]
fn large_projection_taps_reject_malformed_native_words() {
    assert!(large_projection_words(vec![]).is_err());
    let mut rows: Vec<_> = [2560,1024,1024,4096].map(|width| vec![0; 32*width]).into();
    rows[3][31*4096] = 0x7e00;
    assert!(large_projection_words(rows).is_err());
}

#[test]
#[ignore = "owned Windows original Large T32 projection operands and live CUDA edges"]
fn cuda_large_fp16_projection_taps_t32() -> Result<()> {
    profile_context(Experiment {
        projection_capture: true, fp16_t32: true,
        graph: "fff6dbe2f0c2fac4b9ba215da7d5029421d2706ee48c01c2485b9f157ee15ca6",
        receipt_schema: "airk.private.large-fp16-layer0-projection-t32/1",
        ..LARGE
    })
}

fn profile_context(experiment: Experiment) -> Result<()> {
    let started_ns = airk_process::monotonic::now_ns()?;
    let started = Instant::now();
    let execution_user = airk_ipc::endpoint::current_user()?;
    let default_owner = diagnostic_default_owner()?;
    ensure!(
        execution_user == default_owner,
        "native default owner differs"
    );
    let (request, request_pin) = request()?;
    let Request::Probe {
        schema,
        gpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    } = request
    else {
        anyhow::bail!("reviewed Context graph Probe request required");
    };
    ensure!(
        schema == SCHEMA
            && bounds.host_limit_bytes == experiment.host_gib << 30
            && job_limit_bytes == experiment.host_gib << 30
            && bounds.deadline_ms == 120_000
            && bounds.device_limit_bytes == experiment.device_gib << 30,
        "approved original Context host, device or time bound differs"
    );
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let expires_ns = started_ns
        .checked_add(bounds.deadline_ms * 1_000_000)
        .context("original expiry overflow")?;
    let (input, profile, files, _) = inputs_from(&gpu_input)?;
    ensure!(
        profile.model == experiment.model
            && profile.cpu_fit.is_none()
            && profile.device_execution.is_some(),
        "frozen Context GPU profile required"
    );
    let request_capacity = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &profile,
    )?;
    let (capacity_pin, capacity_plan) = if experiment.model == ModelChoice::ContextQwenCompact {
        let pin = exact_pin("AIRK_CUDA_HYBRID_CAPACITY", CAPACITY_SHA)?;
        let (_, plan) = capacity(&pinned(&pin, 4096)?, bounds.device_limit_bytes)?;
        (Some(pin), plan)
    } else {
        ensure!(
            (cold_bytes, warm_bytes, headroom_bytes) == (40 << 30, 8 << 30, 4 << 30),
            "Large profiling requires the reviewed load-only device reservation"
        );
        (None, request_capacity)
    };
    let graph_pin = exact_pin("AIRK_CUDA_HYBRID_GRAPH", experiment.graph)?;
    let graph_bytes = pinned(&graph_pin, PROOF_BYTES)?;
    let graph: Value = serde_json::from_slice(&graph_bytes)?;
    ensure!(
        graph["request"] == json!(request_pin)
            && graph["native_input"] == json!(gpu_input)
            && graph["runtime_manifest"] == json!(runtime_manifest),
        "reviewed graph did not consume the exact current request, input and runtime"
    );
    let references = if experiment.fp16_t32 {
        super::fp16_large::t32_references(&profile, &input)?
    } else {
        fp32_references(&experiment, &profile, &input)?
    };
    for row in &references.rows {
        reference_arrays(row)?;
    }
    let out = output()?;
    let prefix = PathBuf::from(text("AIRK_CUDA_HYBRID_ORT_PROFILE_PREFIX")?);
    ensure!(
        out.is_absolute()
            && prefix.is_absolute()
            && out.parent() == prefix.parent()
            && prefix.file_name().is_some()
            && out != prefix
            && !prefix.exists(),
        "fresh guarded ORT profiling prefix required beside native output"
    );
    pinned(&runtime_manifest, 128 << 10)?;
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    let installed_runtime = if experiment.projection_capture {
        let expected = out.parent().and_then(Path::parent).context("private campaign directory missing")?
            .join("windows-cu15-scientific-provider-stage-r1/airk-cuda-runtime.json");
        ensure!(manifest == expected, "original Large capture requires its guarded D provider stage");
        let installed = ResourceFilePin { path: manifest.to_string_lossy().into_owned(), sha256: runtime_manifest.sha256 };
        ensure!(pinned(&installed, 128 << 10)? == pinned(&runtime_manifest, 128 << 10)?,
            "installed runtime differs from the original Large CUDA provider");
        Some(installed)
    } else {
        ensure!(manifest.as_path() == Path::new(&runtime_manifest.path), "runtime locator pin differs");
        None
    };
    let actual_job = if matches!(experiment.graph, "0d00e2fabdb5fe54474041fb1f2787502349b78ce40436b955cd4433633a4462" | BODY_GRAPH_OBSERVATION) {
        job_limits(22 << 30, 22 << 30)?
    } else {
        job_limits(bounds.host_limit_bytes, job_limit_bytes)?
    };
    let mut resources = airk_kernel::service_resources::Resources::detect(ResourceConfiguration {
        preset: ResourcePreset::Throughput,
        ..ResourceConfiguration::default()
    })?;
    let available = airk_kernel::service_resource_observations::available_memory();
    resources.observe(
        airk_kernel::service_resource_observations::rss(std::process::id()),
        available,
        true,
    );
    let incarnation = Incarnation::from_bytes(request_pin.sha256.as_bytes()[..16].try_into()?);
    let charge =
        Arc::new(resources.reserve_optional(incarnation, bounds.host_limit_bytes, available)?);
    let release = charge.release_observer();
    let (parent, control) = private_control()?;
    let mut control_parent = Some(parent);
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = Arc::new(airk_kernel::inference_owner::NativeInferenceOwner::admit(
        lease.clone(),
    )?);
    let binary = file_hash(&std::env::current_exe()?)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(&out)?;
    let mut receipt = json!({
        "schema": experiment.receipt_schema, "complete": false,
        "qualified": false, "product_admission": false, "request": request_pin, "native_input": gpu_input,
        "profile": profile.manifest, "source_manifest": input.source_manifest, "runtime_manifest": runtime_manifest,
        "reviewed_graph": graph_pin,
        "capacity_policy": capacity_pin,
        "applied_capacity": {
            "arena_limit_bytes": capacity_plan.cold_peak_bytes(),
            "warm_extra_bytes": capacity_plan.warm_extra_bytes(),
            "headroom_bytes": capacity_plan.headroom_bytes(),
        },
        "pid": std::process::id(), "native_binary_sha256": binary, "execution_user": execution_user,
        "default_owner": default_owner, "bounds": bounds, "actual_job": actual_job,
        "native_host_reservation_bytes": charge.bytes(), "resource_snapshot": resources.snapshot(),
        "original_expires_monotonic_ns": expires_ns, "runtime_profile": null, "profiled_runs": [],
        "rows": [], "session_teardown_observed": false, "measured_peak_device_bytes": null,
        "measured_transfer_bytes": null,
    });
    if let Some(pin) = &installed_runtime {
        receipt["installed_runtime_manifest"] = json!(pin);
    }
    receipt.as_object_mut().unwrap().extend(
        references
            .provenance
            .as_object()
            .context("scoring reference provenance missing")?
            .clone(),
    );
    flush_graph_diagnostic(&mut file, &receipt)?;
    let mut operations = operation::Journal::create_until(
        &out,
        operation::Identity {
            input_sha256: request_pin.sha256,
            profile: profile.manifest,
            process_id: std::process::id(),
            native_binary_sha256: binary,
        },
        expires_ns,
    )?;
    let run = owner
        .run(|| -> Result<()> {
            let mut custody = Custody {
                scorer: None,
                device: None,
                owner: owner.clone(),
                lease: lease.clone(),
                charge: Some(charge),
                control: &mut control_parent,
                files: Some(files),
            };
            ensure!(Instant::now() < deadline, "expired before CUDA admission");
            let files = custody.files.as_ref().unwrap();
            verify_files(files, &profile)?;
            let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
            ensure!(
                runtime.manifest_identity() == runtime_manifest.sha256,
                "admitted runtime differs"
            );
            let adapters = runtime.adapters(deadline)?;
            ensure!(
                adapters.len() == 1,
                "Context profile requires one observed CUDA adapter"
            );
            let device = runtime.device(adapters[0].uuid, deadline)?;
            custody.device = Some(device.clone());
            receipt["adapter_before"] = json!(device.observation(deadline)?);
            let catalog = airk_kernel::inference_owner::evaluation_catalog(
                lease.clone(),
                vec![profile.clone()],
            )?;
            let execution = catalog
                .execution_profile(profile.manifest)?
                .bind_cuda_probe(device.clone(), capacity_plan, deadline)?;
            let loaded = operations.observe(operation::Kind::Load, 0, None, || {
                let open = if experiment.model == ModelChoice::ContextQwenCompact {
                    Scorer::open_compact_bounded_hybrid_profiled
                } else if experiment.receipt_schema == ATTENTION17_SCHEMA {
                    Scorer::open_large_attention17_taps
                } else if experiment.receipt_schema == DENSE17_SCHEMA {
                    Scorer::open_large_dense17_taps
                } else if experiment.projection_capture {
                    Scorer::open_large_fp16_projection_taps
                } else {
                    Scorer::open_large_bounded_hybrid_profiled
                };
                open(execution, files, &prefix, &graph_bytes, graph_pin.sha256)
            })?;
            receipt["load_us"] = json!(loaded.elapsed_us);
            let mut scorer = loaded.value?;
            ensure!(
                loaded.within_deadline,
                "original Context session load expiry exceeded"
            );
            let policy_identity = match &capacity_pin {
                Some(pin) => airk_core::identity::framed_digest(
                    b"airk.private.compact-cuda-bounded-hybrid-capacity/2",
                    &[scorer.policy_identity().as_bytes(), pin.sha256.as_bytes()],
                ),
                None => airk_core::identity::framed_digest(
                    b"airk.private.large-cuda-bounded-hybrid-capacity/1",
                    &[
                        scorer.policy_identity().as_bytes(),
                        request_pin.sha256.as_bytes(),
                    ],
                ),
            };
            receipt["policy_identity"] = json!(policy_identity);
            receipt["adapter_loaded"] = json!(device.observation(deadline)?);
            flush_graph_diagnostic(&mut file, &receipt)?;
            score_rows(
                &mut scorer,
                &references.rows,
                &mut operations,
                &mut receipt,
                &mut file,
                deadline,
                &experiment,
            )?;
            scorer.synchronize(deadline)?;
            ensure!(
                scorer.device_settled(),
                "Context CUDA work remains unsettled"
            );
            let path = scorer.finish_profiling()?;
            ensure!(
                path.parent() == prefix.parent()
                    && path.extension().is_some_and(|ext| ext == "json")
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| prefix
                            .file_name()
                            .and_then(|s| s.to_str())
                            .is_some_and(|start| name.starts_with(start))),
                "ORT profile escaped its guarded prefix"
            );
            let bytes = small(&path, PROFILE_BYTES)?;
            receipt["runtime_profile"] = json!({
                "path": path, "sha256": sha(&bytes), "bytes": bytes.len(),
            });
            receipt["profiled_runs"] = if experiment.fp16_t32 {
                json!([super::hybrid_profile_trace::large_fp16_t32_profiled_run(
                    &bytes,
                    &scorer.graph_metadata().rows,
                    experiment.graph
                )?])
            } else {
                json!(super::hybrid_profile_trace::profiled_runs(
                    &bytes,
                    &scorer.graph_metadata().rows,
                    &WIDTHS
                )?)
            };
            receipt["adapter_after"] = json!(device.observation(deadline)?);
            drop(scorer);
            device.synchronize(deadline)?;
            ensure!(
                !device.poisoned() && !cuda::process_poisoned(),
                "native teardown settlement unknown"
            );
            receipt["session_teardown_observed"] = json!(true);
            verify_files(files, &profile)?;
            for (pin, max) in [
                (&request_pin, 1 << 20),
                (&gpu_input, 1 << 20),
                (&input.profile, 1 << 20),
                (&runtime_manifest, 128 << 10),
                (&input.source_manifest, PROOF_BYTES),
                (&graph_pin, PROOF_BYTES),
            ] {
                pinned(pin, max)?;
            }
            if let Some(pin) = &capacity_pin {
                pinned(pin, 4096)?;
            }
            if let Some(pin) = &installed_runtime {
                pinned(pin, 128 << 10)?;
            }
            for (pin, max) in &references.pins {
                pinned(pin, *max)?;
            }
            ensure!(
                Instant::now() < deadline,
                "original Context expiry exceeded during postpins"
            );
            Ok(())
        })
        .and_then(std::convert::identity);
    drop(owner);
    drop(lease);
    let retained_control = control_parent.is_none();
    let cleanup = control_parent
        .take()
        .map(tempfile::TempDir::close)
        .transpose();
    let cleaned = matches!(&cleanup, Ok(Some(())));
    let mut wait = std::pin::pin!(release.wait());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let released = matches!(
        std::future::Future::poll(wait.as_mut(), &mut cx),
        std::task::Poll::Ready(Ok(()))
    );
    let poisoned = cuda::process_poisoned();
    let peak = peak_rss();
    receipt["native_control_retained_for_unknown_cuda"] = json!(retained_control);
    receipt["native_control_cleanup_observed"] = json!(cleaned);
    receipt["native_control_cleanup_error"] =
        json!(cleanup.as_ref().err().map(ToString::to_string));
    receipt["native_host_charge_release_observed"] = json!(released);
    receipt["process_cuda_poisoned"] = json!(poisoned);
    receipt["peak_process_rss_bytes"] = json!(peak);
    let result = run.and_then(|()| {
        ensure!(
            receipt["session_teardown_observed"] == true
                && !retained_control
                && cleaned
                && released
                && !poisoned,
            "Context CUDA session or host release unproven"
        );
        final_host_peak(peak, bounds.host_limit_bytes)?;
        Ok(())
    });
    receipt["complete"] = json!(result.is_ok());
    receipt["error"] = json!(result.as_ref().err().map(|e| format!("{e:#}")));
    receipt["elapsed_us_to_final_report_start"] = json!(started.elapsed().as_micros());
    flush_graph_diagnostic(&mut file, &receipt)?;
    if Instant::now() >= deadline {
        receipt["complete"] = json!(false);
        receipt["final_report_retention_exceeded_original_deadline"] = json!(true);
        flush_graph_diagnostic(&mut file, &receipt)?;
        anyhow::bail!("original Context profiling expiry during result retention");
    }
    result
}

#[test]
fn compact_capacity_revision_stays_within_approved_device_envelope() -> Result<()> {
    let input = json!({
        "schema": "airk.private.compact-cuda-arena-policy/1",
        "failed_native_result_sha256": "e3fd8d525bd0f7abc7eae08b154dd9410c02a6ee6d27d475bce7af3b4814ef4f",
        "device_limit_bytes": 25769803776_u64,
        "arena_limit_bytes": 23622320128_u64,
        "warm_extra_bytes": 17179869184_u64,
        "headroom_bytes": 2147483648_u64,
    });
    let bytes = serde_json::to_vec(&input)?;
    let (_, plan) = capacity(&bytes, 24 << 30)?;
    assert_eq!(plan.cold_peak_bytes(), 22 << 30);
    assert_eq!(plan.warm_extra_bytes(), 16 << 30);
    assert_eq!(plan.headroom_bytes(), 2 << 30);
    assert!(capacity(&bytes, 32 << 30).is_err());
    for (key, drift) in [
        ("arena_limit_bytes", json!(25_u64 << 30)),
        ("warm_extra_bytes", json!(8_u64 << 30)),
        ("headroom_bytes", json!(1_u64 << 30)),
        ("failed_native_result_sha256", json!("00".repeat(32))),
    ] {
        let mut changed = input.clone();
        changed[key] = drift;
        assert!(capacity(&serde_json::to_vec(&changed)?, 24 << 30).is_err());
    }
    Ok(())
}

mod resource20;

#[test]
#[ignore = "owned exclusive Windows CUDA Job and original monotonic resource envelope required"]
fn cuda_compact_fp32_bounded_hybrid_resource20() -> Result<()> {
    resource20::run(COMPACT)
}

#[test]
#[ignore = "requires the owned Windows CUDA Large Resource20 launcher"]
fn cuda_large_fp32_batch_view_bounded_hybrid_resource20() -> Result<()> {
    resource20::run(BATCH_VIEW)
}
