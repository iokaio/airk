// SPDX-License-Identifier: Apache-2.0
//! Private native metadata and T32 probe for the reviewed Compact FP16 candidate.
use super::*;
use airk_core::arbiter::device::{DevicePrecision, PlacementPolicy};

const ORIGINAL_GRAPH: &str = "c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4";
const FP32_FIXTURE: &str = "c4360d8a759c68c305fba0df7bd04001a6b043d8a2a57b36f0d8ef0f898519b9";
const FP32_PRODUCER: &str = "b3849443d0d4d737d5c3f88b323b9eaffbd1045500d97f1487c56be639884bbe";
const T32_INPUT: &str = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335";
const GRAPH_BYTES: u64 = 64 << 20;
const PROOF_BYTES: u64 = 16 << 20;
const REVIEWED_FP16_GRAPH_SHA256: &str =
    "47a7cd6f9e0ddf50911cc7a745bcbec4ca2a963772aad5e2e6acc663e26ec104";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProofFile {
    path: String,
    sha256: Sha256Digest,
    bytes: Option<u64>,
}
impl ProofFile {
    fn pin(&self) -> ResourceFilePin {
        ResourceFilePin {
            path: self.path.clone(),
            sha256: self.sha256,
        }
    }

    fn asset(&self, name: &str, role: AssetRole, limit: u64) -> Result<Asset> {
        self.pin().validate()?;
        let bytes = self
            .bytes
            .context("reviewed FP16 artifact length missing")?;
        ensure!(
            Path::new(&self.path).file_name().and_then(|v| v.to_str()) == Some(name)
                && (1..=limit).contains(&bytes),
            "reviewed FP16 artifact name or length differs"
        );
        Ok(Asset {
            name: name.into(),
            role,
            bytes,
            sha256: self.sha256,
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CandidateProof {
    graph: ProofFile,
    weights: ProofFile,
}

const QKV_STAGE_SHA256: &str = "87ec206416eda607ff1585809d085e37fc4f8b99a5fc029308b5c78218cefcc2";
const QKV_GRAPH_SHA256: &str = "e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14";
const QKV_METADATA_SCHEMA: &str = "airk.private.compact-fp32-qkv-probe-metadata/1";
const QKV_GRAPH_SCHEMA: &str = "airk.private.compact-fp32-qkv-graph-diagnostic/1";
const QKV_NATIVE_GRAPH_SHA256: &str =
    "42a273598b5c8678535691426ed725041a5d9565ff1fbaf535a43938894f07a4";
const QKV_T32_SCHEMA: &str = "airk.private.compact-fp32-qkv-t32-component/1";
const QKV_REFERENCE_SHA256: &str =
    "304d9b16922ce88f9db181c1e1956d9c0aa99ab29c5ef4dff26f2c37f5b0fa28";

fn qkv_projection_stage(proof: &IndependentProof) -> Result<(ResourceFilePin, CandidateProof)> {
    let pin = env_pin(
        "AIRK_CUDA_FP16_PROJECTION_STAGE",
        "AIRK_CUDA_FP16_PROJECTION_STAGE_SHA256",
    )?;
    ensure!(
        pin.sha256 == QKV_STAGE_SHA256.parse::<Sha256Digest>()?,
        "reviewed Compact FP32 Q/K/V projection stage required"
    );
    let fact: Value = serde_json::from_slice(&pinned(&pin, 1 << 20)?)?;
    let candidate = CandidateProof {
        graph: serde_json::from_value(fact["candidate_graph"].clone())?,
        weights: serde_json::from_value(fact["candidate_weights"].clone())?,
    };
    let directory = Path::new(&pin.path).parent();
    ensure!(
        fact["schema"] == "airk.private.compact-fp32-qkv-projection-candidate/1"
            && fact["complete"] == true
            && fact["qualification"] == false
            && fact["product_admission"] == false
            && fact["native_load_attempts"] == 0
            && fact["native_forward_attempts"] == 0
            && fact["baseline_graph"] == json!(proof.candidate.graph)
            && fact["baseline_weights"] == json!(proof.candidate.weights)
            && fact["graph_proof"]["projection_count"] == 84
            && fact["graph_proof"]["added_cast_count"] == 252
            && candidate.graph.sha256 == QKV_GRAPH_SHA256.parse::<Sha256Digest>()?
            && candidate.graph.sha256 != proof.candidate.graph.sha256
            && candidate.weights.sha256 == proof.candidate.weights.sha256
            && candidate.weights.bytes == proof.candidate.weights.bytes
            && Path::new(&candidate.graph.path).parent() == directory
            && Path::new(&candidate.weights.path).parent() == directory,
        "Compact FP32 Q/K/V stage graph, baseline or weight lineage differs"
    );
    Ok((pin, candidate))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceProof {
    fixture: ProofFile,
    producer: ProofFile,
    name: String,
    input: CompiledInput,
    model: ModelChoice,
    compiler: Sha256Digest,
    resource_policy: Sha256Digest,
    matching_fp16: [f32; 8],
    original_fp32: [f32; 8],
}

#[derive(Deserialize)]
struct IndependentProof {
    schema: String,
    complete: bool,
    model: ModelChoice,
    reference_device: String,
    model_inference: bool,
    torch_forward_invocations: u64,
    native_execution: bool,
    qualification: bool,
    candidate: CandidateProof,
    original_graph: ProofFile,
    verification: Value,
    reference: ReferenceProof,
}

fn qkv_projection_reference(
    stage: &ResourceFilePin,
    candidate: &CandidateProof,
    baseline: &IndependentProof,
) -> Result<(ResourceFilePin, ReferenceProof)> {
    let pin = env_pin(
        "AIRK_CUDA_FP16_PROJECTION_REFERENCE",
        "AIRK_CUDA_FP16_PROJECTION_REFERENCE_SHA256",
    )?;
    ensure!(
        pin.sha256 == QKV_REFERENCE_SHA256.parse::<Sha256Digest>()?,
        "exact independent Compact FP32 Q/K/V T32 reference required"
    );
    let mut proof: Value = serde_json::from_slice(&pinned(&pin, PROOF_BYTES)?)?;
    ensure!(
        proof["schema"] == "airk.private.compact-fp32-qkv-t32-reference/1"
            && proof["complete"] == true
            && proof["reference_device"] == "cpu"
            && proof["model_inference"] == true
            && proof["torch_forward_invocations"] == 1
            && proof["native_execution"] == false
            && proof["qualification"] == false
            && proof["product_admission"] == false
            && proof["candidate"] == json!(candidate)
            && proof["baseline_candidate"] == json!(baseline.candidate)
            && proof["original_graph"] == json!(baseline.original_graph)
            && proof["stage_receipt"]["path"] == json!(stage.path)
            && proof["stage_receipt"]["sha256"] == json!(stage.sha256)
            && proof["recipe"]["qkv_fp32_routes"]
                .as_array()
                .is_some_and(|routes| routes.len() == 84)
            && proof["recipe"]["context_flash_single_tile"] == true,
        "independent Compact FP32 Q/K/V recipe or source differs"
    );
    let reference = proof["reference"]
        .as_object_mut()
        .context("missing Q/K/V reference row")?;
    let matching = reference
        .remove("matching_qkv_fp32")
        .context("missing matching Q/K/V scores")?;
    ensure!(
        reference.insert("matching_fp16".into(), matching).is_none(),
        "ambiguous matching Q/K/V scores"
    );
    let reference: ReferenceProof = serde_json::from_value(proof["reference"].take())?;
    ensure!(
        serde_json::to_value(&reference.input)? == serde_json::to_value(&baseline.reference.input)?
            && exact_reference_score_words(
                &reference.original_fp32,
                &baseline.reference.original_fp32
            ),
        "independent Compact FP32 Q/K/V original T32 input or score bits differ"
    );
    Ok((pin, reference))
}

fn exact_reference_score_words(actual: &[f32; 8], expected: &[f32; 8]) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, b)| a.to_bits() == b.to_bits())
}

#[test]
fn compact_fp16_qkv_reference_compares_original_binary32_words() -> Result<()> {
    let model_scores: Value = serde_json::from_str("[0.10000000149011612,-0.0,0,1,2,3,4,5]")?;
    let native_scores = json!([0.1, -0.0, 0, 1, 2, 3, 4, 5]);
    assert_ne!(model_scores, native_scores);
    let actual = serde_json::from_value(model_scores)?;
    let mut expected = serde_json::from_value(native_scores)?;
    assert!(exact_reference_score_words(&actual, &expected));
    expected[1] = 0.0;
    assert!(!exact_reference_score_words(&actual, &expected));
    Ok(())
}

fn validate_independent_proof(proof: &IndependentProof) -> Result<()> {
    let graph = &proof.verification["graph"];
    let parameters = proof.verification["parameters"]
        .as_array()
        .context("independent Compact FP16 parameter census missing")?;
    let layers = 28;
    ensure!(
        proof.schema == "airk.private.compact-fp16-independent-proof/1"
            && proof.complete
            && proof.model == ModelChoice::ContextQwenCompact
            && proof.reference_device == "cpu"
            && proof.model_inference
            && proof.torch_forward_invocations == 1
            && !proof.native_execution
            && !proof.qualification
            && proof.original_graph.sha256 == ORIGINAL_GRAPH.parse()?
            && proof.candidate.graph.sha256 != proof.original_graph.sha256
            && proof.verification["qualification"] == false
            && graph["qualification"] == false
            && graph["layers"] == layers
            && graph["nodes"]
                .as_u64()
                .is_some_and(|count| (1..=65_536).contains(&count))
            && graph["all_output_reachable_edges_checked"] == true
            && parameters.len() == 2 + layers * 11,
        "reviewed Compact FP16 graph or learned-parameter census differs"
    );
    let mut names = BTreeSet::new();
    let mut initializers = BTreeSet::new();
    for row in parameters {
        let name = row["parameter"]
            .as_str()
            .context("parameter name missing")?;
        let initializer = row["initializer"]
            .as_str()
            .context("initializer name missing")?;
        let actual: Sha256Digest =
            serde_json::from_value(row["actual_initializer_sha256"].clone())?;
        let expected: Sha256Digest =
            serde_json::from_value(row["independent_expected_sha256"].clone())?;
        ensure!(
            !name.is_empty()
                && !initializer.is_empty()
                && names.insert(name)
                && initializers.insert(initializer)
                && actual == expected,
            "independent Compact FP16 parameter binding differs"
        );
    }
    Ok(())
}

fn validate_reference(
    reference: &ReferenceProof,
    cpu: &Profile,
    policy: &ResourcePolicy,
) -> Result<()> {
    let fixture_pin = reference.fixture.pin();
    let producer_pin = reference.producer.pin();
    ensure!(
        fixture_pin.sha256 == FP32_FIXTURE.parse()?
            && producer_pin.sha256 == FP32_PRODUCER.parse()?
            && reference.name == "diagnostic-all-live-32"
            && reference.input.input_digest == T32_INPUT.parse()?
            && reference.model == cpu.model
            && reference.compiler == cpu.compiler
            && reference.resource_policy == policy.digest()?
            && reference
                .matching_fp16
                .iter()
                .all(|value| value.is_finite()),
        "independent Compact FP16 T32 identity differs"
    );
    let fixture: NumericFixture = serde_json::from_slice(&pinned(&fixture_pin, PROOF_BYTES)?)?;
    pinned(&producer_pin, PROOF_BYTES)?;
    numeric_rows(&fixture.rows, &[(32, 8), (1024, 8)], false)?;
    let row = &fixture.rows[0];
    ensure!(
        fixture.schema == "airk.arbiter.native-numeric/2"
            && fixture.model == reference.model
            && fixture.compiler == reference.compiler
            && fixture.resource_policy == reference.resource_policy
            && fixture.reference_producer.as_ref() == Some(&producer_pin)
            && row.name == reference.name
            && serde_json::to_value(&row.input)? == serde_json::to_value(&reference.input)?
            && row.reference_original_fp32.as_deref() == Some(reference.original_fp32.as_slice()),
        "original Compact FP32 T32 fixture or reference differs"
    );
    Ok(())
}

fn require_initial_bounds(
    bounds: &GpuProbeReservation,
    phases: (u64, u64, u64, u64),
) -> Result<()> {
    ensure!(
        bounds.host_limit_bytes == 32 << 30
            && bounds.device_limit_bytes == 24 << 30
            && bounds.deadline_ms == 120_000
            && phases == (16 << 30, 8 << 30, 2 << 30, 32 << 30),
        "Compact FP16 probe requires the reviewed Windows host, device and deadline bounds"
    );
    Ok(())
}

#[test]
fn compact_fp16_metadata_requires_original_windows_probe_envelope() -> Result<()> {
    let mut bounds = GpuProbeReservation {
        schema: "airk.arbiter.gpu-probe-reservation/1".into(),
        host_limit_bytes: 32 << 30,
        device_limit_bytes: 24 << 30,
        deadline_ms: 120_000,
    };
    let phases = (16 << 30, 8 << 30, 2 << 30, 32 << 30);
    require_initial_bounds(&bounds, phases)?;
    assert!(require_initial_bounds(&bounds, (16 << 30, 8 << 30, 2 << 30, 24 << 30)).is_err());
    bounds.host_limit_bytes = 16 << 30;
    assert!(require_initial_bounds(&bounds, phases).is_err());
    Ok(())
}

#[test]
#[ignore = "data-only metadata for the actual reviewed Compact FP16 artifact and T32 reference"]
fn emit_cuda_compact_fp16_probe_metadata() -> Result<()> {
    emit_compact_fp16_probe_metadata(false)
}

#[test]
#[ignore = "owned unqualified metadata only for the reviewed Compact FP32 Q/K/V projection experiment"]
fn emit_cuda_compact_fp32_qkv_projection_probe_metadata() -> Result<()> {
    emit_compact_fp16_probe_metadata(true)
}

fn emit_compact_fp16_probe_metadata(qkv_projection: bool) -> Result<()> {
    let started = Instant::now();
    let execution_user = airk_ipc::endpoint::current_user()?;
    ensure!(
        execution_user == diagnostic_default_owner()?,
        "native Compact FP16 default owner differs"
    );
    let (request, request_pin) = request()?;
    let Request::Prepare {
        schema,
        cpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    } = request
    else {
        anyhow::bail!("reviewed Compact CPU Prepare request required");
    };
    ensure!(
        schema == SCHEMA,
        "unknown Compact FP16 metadata request schema"
    );
    require_initial_bounds(
        &bounds,
        (cold_bytes, warm_bytes, headroom_bytes, job_limit_bytes),
    )?;
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let (mut input, cpu, cpu_files, _) = inputs_from(&cpu_input)?;
    let capacity = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &cpu,
    )?;
    let cpu_graph = cpu
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Model)
        .context("authenticated Compact CPU graph missing")?;
    ensure!(
        cpu.model == ModelChoice::ContextQwenCompact
            && cpu.cpu_fit.is_some()
            && cpu.device_execution.is_none()
            && cpu_graph.sha256 == ORIGINAL_GRAPH.parse()?,
        "current native Compact CPU profile and original graph required"
    );
    let approved_manifest: Sha256Digest = option_env!("AIRK_APPROVED_CUDA_MANIFEST_SHA256")
        .context("CUDA manifest approval must be compiled")?
        .parse()?;
    let approved_build: Sha256Digest = option_env!("AIRK_STATIC_CUDA_BUILD_SHA256")
        .context("static CUDA build approval must be compiled")?
        .parse()?;
    ensure!(
        runtime_manifest.sha256 == approved_manifest,
        "Compact FP16 runtime manifest differs from the compiled approval"
    );
    let runtime = pinned(&runtime_manifest, 128 << 10)?;
    let proof_pin = env_pin(
        "AIRK_CUDA_FP16_INDEPENDENT_PROOF",
        "AIRK_CUDA_FP16_INDEPENDENT_PROOF_SHA256",
    )?;
    let proof: IndependentProof = serde_json::from_slice(&pinned(&proof_pin, PROOF_BYTES)?)?;
    validate_independent_proof(&proof)?;
    validate_reference(&proof.reference, &cpu, &input.policy)?;
    let original_bytes = pinned(&proof.original_graph.pin(), GRAPH_BYTES)?;
    ensure!(
        proof.original_graph.bytes == Some(cpu_graph.bytes)
            && original_bytes.len() as u64 == cpu_graph.bytes,
        "independently verified original Compact graph does not bind the CPU profile"
    );
    verify_files(&cpu_files, &cpu)?;
    ensure!(
        Instant::now() < deadline,
        "original Compact FP16 metadata expiry reached"
    );
    let projection = qkv_projection
        .then(|| qkv_projection_stage(&proof))
        .transpose()?;
    let candidate = projection
        .as_ref()
        .map_or(&proof.candidate, |(_, candidate)| candidate);
    ensure!(
        Path::new(&candidate.graph.path).parent() == Path::new(&candidate.weights.path).parent(),
        "reviewed Compact FP16 graph and weights must share their original directory"
    );
    let model = candidate
        .graph
        .asset("model.onnx", AssetRole::Model, GRAPH_BYTES)?;
    let weights = candidate
        .weights
        .asset("weights.bin", AssetRole::ExternalWeights, 16 << 30)?;
    for asset in &cpu.assets {
        if matches!(asset.role, AssetRole::Model | AssetRole::ExternalWeights) {
            input.files.remove(&asset.name);
        }
    }
    ensure!(
        input
            .files
            .insert(model.name.clone(), candidate.graph.path.clone())
            .is_none()
            && input
                .files
                .insert(weights.name.clone(), candidate.weights.path.clone())
                .is_none(),
        "reviewed Compact FP16 model paths collide with existing assets"
    );
    let prepared = cuda::prepare_fp16_probe_profile(
        &cpu,
        &Files(input.files.clone()),
        vec![model, weights],
        proof.original_graph.sha256,
        &runtime,
        runtime_manifest.sha256,
        capacity,
    )?;
    let device = prepared
        .profile
        .device_execution
        .as_ref()
        .context("Compact FP16 descriptor missing")?;
    ensure!(
        device.runtime_build == approved_build
            && device.graph == candidate.graph.sha256
            && device.precision == DevicePrecision::Float16
            && device.placement == PlacementPolicy::StrictGpu
            && prepared.original_graph == cpu_graph.sha256
            && prepared.profile.qualification.is_none()
            && prepared.profile.qualified_targets.is_empty(),
        "prepared Compact FP16 profile does not bind the approved build and strict candidate"
    );
    ensure!(
        Instant::now() < deadline,
        "original Compact FP16 metadata expiry reached"
    );
    let directory = PathBuf::from(text("AIRK_CUDA_METADATA_DIRECTORY")?);
    ensure!(directory.is_absolute(), "absolute guarded output required");
    std::fs::create_dir(&directory)?;
    let mut source_manifest = json!({
        "schema": "airk.private.compact-fp16-source/1",
        "parent_source": input.source_manifest, "parent_input": cpu_input,
        "parent_profile": cpu.manifest, "original_graph": proof.original_graph,
        "candidate": candidate, "independent_proof": proof_pin,
        "attention_plan": prepared.attention_plan, "assets": prepared.profile.assets,
    });
    if let Some((stage, _)) = &projection {
        source_manifest["schema"] = json!("airk.private.compact-fp32-qkv-source/1");
        source_manifest["projection_stage"] = json!(stage);
    }
    input.source_manifest = write_json(&directory.join("source-manifest.json"), &source_manifest)?;
    let calibration = write_new(
        &directory.join(&prepared.calibration_name),
        &prepared.calibration_bytes,
    )?;
    input
        .files
        .insert(prepared.calibration_name, calibration.path.clone());
    input.profile = write_json(&directory.join("profile.json"), &prepared.profile)?;
    input.numeric_fixture = None;
    input.validate()?;
    let native_input = write_json(&directory.join("native-input.json"), &input)?;
    let probe = write_json(
        &directory.join("probe-request.json"),
        &Request::Probe {
            schema,
            gpu_input: native_input.clone(),
            runtime_manifest: runtime_manifest.clone(),
            bounds,
            cold_bytes,
            warm_bytes,
            headroom_bytes,
            job_limit_bytes,
        },
    )?;
    ensure!(
        Instant::now() < deadline,
        "original Compact FP16 metadata expiry reached"
    );
    let mut receipt = json!({
            "schema": "airk.private.compact-fp16-probe-metadata/1",
            "request": request_pin, "cpu_input": cpu_input, "runtime_manifest": runtime_manifest,
            "independent_proof": proof_pin, "reference_input_digest": proof.reference.input.input_digest,
            "source_manifest": input.source_manifest, "profile": prepared.profile.manifest,
            "profile_pin": input.profile, "calibration": calibration, "native_input": native_input,
            "probe_request": probe, "execution_user": execution_user, "qualified": false,
            "model_requests_dispatched": false, "runtime_admission_dispatched": false,
            "forward_count_observed": null, "elapsed_us": started.elapsed().as_micros(),
    });
    if let Some((stage, _)) = &projection {
        receipt["schema"] = json!(QKV_METADATA_SCHEMA);
        receipt["projection_stage"] = json!(stage);
    }
    write_json(&directory.join("metadata-receipt.json"), &receipt)?;
    ensure!(
        Instant::now() < deadline,
        "original Compact FP16 expiry reached during retention"
    );
    Ok(())
}

fn bind_t32_candidate(
    input: &ResourceRunInput,
    profile: &Profile,
    metadata: &Value,
    reference: &ReferenceProof,
    candidate: &CandidateProof,
) -> Result<()> {
    let device = profile
        .device_execution
        .as_ref()
        .context("reviewed Compact FP16 CUDA descriptor missing")?;
    ensure!(
        profile.model == ModelChoice::ContextQwenCompact
            && profile.profile_id == "private-cuda-fp16-compact-efficient-probe-v1"
            && profile.cpu_fit.is_none()
            && input.numeric_fixture.is_none()
            && device.precision == DevicePrecision::Float16
            && device.placement == PlacementPolicy::StrictGpu
            && device.graph == candidate.graph.sha256
            && metadata["reference_input_digest"] == json!(reference.input.input_digest),
        "reviewed Compact FP16 profile, graph or T32 reference differs"
    );
    for (asset, fact) in [
        (
            candidate
                .graph
                .asset("model.onnx", AssetRole::Model, GRAPH_BYTES)?,
            &candidate.graph,
        ),
        (
            candidate
                .weights
                .asset("weights.bin", AssetRole::ExternalWeights, 16 << 30)?,
            &candidate.weights,
        ),
    ] {
        ensure!(
            profile.assets.contains(&asset) && input.files.get(&asset.name) == Some(&fact.path),
            "Compact FP16 native input does not bind the independently verified file"
        );
    }
    Ok(())
}

fn observe_t32_input_refusals(
    scorer: &Scorer,
    input: &CompiledInput,
    receipt: &mut Value,
    file: &mut File,
) -> Result<()> {
    scorer.validate_fp16_probe_input(input)?;
    let mut wrong_positions = input.clone();
    wrong_positions.position_ids[0] = 1;
    let mut hole = input.clone();
    hole.attention_mask[16] = 0;
    let mut all_zero = input.clone();
    all_zero.attention_mask.fill(0);
    for (case, bad, expected) in [
        (
            "wrong_native_positions",
            wrong_positions,
            "G5 original positions/decision/input length differ",
        ),
        (
            "mask_hole",
            hole,
            "G5 Context requires original all-live8192/N8 geometry",
        ),
        (
            "mask_all_zero",
            all_zero,
            "G5 Context requires original all-live8192/N8 geometry",
        ),
    ] {
        let observed = scorer.validate_fp16_probe_input(&bad);
        let error = observed.as_ref().err().map(ToString::to_string);
        receipt["input_refusals"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "case": case, "refused": error.as_deref() == Some(expected),
                "error": error, "native_forward_dispatched": false,
            }));
        flush_graph_diagnostic(file, receipt)?;
        ensure!(
            error.as_deref() == Some(expected),
            "Compact FP16 pre-dispatch input control differs: {case}"
        );
    }
    Ok(())
}

fn t32_reference_bounds(
    (matching_raw, matching_mass, matching_argmax): (f64, f64, bool),
    original_mass: f64,
) -> bool {
    matching_raw <= 0.001 && matching_mass <= 0.05 && matching_argmax && original_mass <= 0.05
}

#[test]
fn compact_fp16_t32_limits_matching_parity_and_original_mass() -> Result<()> {
    let actual = [0.02, 0.0];
    let matching = comparison(&actual, &actual)?;
    let (original_raw, original_mass, original_argmax) = comparison(&actual, &[0.0, 0.02])?;
    assert!(original_raw > 0.001);
    assert!(!original_argmax);
    assert!(t32_reference_bounds(matching, original_mass));

    assert!(t32_reference_bounds((0.001, 0.05, true), 0.05));
    assert!(!t32_reference_bounds((0.0011, 0.05, true), 0.05));
    assert!(!t32_reference_bounds((0.001, 0.051, true), 0.05));
    assert!(!t32_reference_bounds((0.001, 0.05, false), 0.05));
    assert!(!t32_reference_bounds((0.001, 0.05, true), 0.051));
    Ok(())
}

fn read_t32_profile(scorer: &Scorer, prefix: &Path) -> Result<(Value, Vec<u8>)> {
    let path = scorer.finish_profiling()?;
    let leaf = prefix
        .file_name()
        .and_then(|value| value.to_str())
        .context("ORT profile prefix missing")?;
    ensure!(
        path.parent() == prefix.parent()
            && path.extension().is_some_and(|part| part == "json")
            && path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with(leaf)),
        "Compact FP16 ORT profile escaped the checked prefix"
    );
    let bytes = small(&path, 64 << 20)?;
    ensure!(!bytes.is_empty(), "Compact FP16 ORT profile is empty");
    Ok((
        json!({"path": path, "bytes": bytes.len(), "sha256": sha(&bytes)}),
        bytes,
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompactFp16Probe {
    StrictT32,
    GraphOnly,
    QkvProjectionGraphOnly,
    QkvProjectionT32,
    BoundedT32,
    Layer0Taps,
    Layer0ProjectionTaps,
}

#[test]
#[ignore = "owned strict Compact FP16 T32 probe with a reviewed artifact, independent reference and actual CUDA runtime"]
fn cuda_compact_fp16_t32_component() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::StrictT32)
}

#[test]
#[ignore = "disposable Windows Compact FP16 final graph with the reviewed T32 Probe; no scorer or forward"]
fn cuda_compact_fp16_load_only_graph_diagnostic() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::GraphOnly)
}

#[test]
#[ignore = "owned load-only CUDA graph observation for the distinct Compact FP32 Q/K/V projection experiment"]
fn cuda_compact_fp32_qkv_projection_load_only_graph_diagnostic() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvProjectionGraphOnly)
}

#[test]
#[ignore = "owned Compact FP16 T32 scoring with exact final graph, bounded CPU controls and native CUDA profiling"]
fn cuda_compact_fp16_bounded_t32_component() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::BoundedT32)
}

#[test]
#[ignore = "disposable Compact FP16 T32 CUDA diagnostic of the first layer half attention tensors"]
fn cuda_compact_fp16_layer0_taps() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::Layer0Taps)
}

#[test]
#[ignore = "disposable Compact FP16 T32 CUDA diagnostic of the first layer half projections"]
fn cuda_compact_fp16_layer0_projection_taps() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::Layer0ProjectionTaps)
}

#[test]
#[ignore = "one bounded Windows CUDA Compact FP32 Q/K/V score against its exact independent T32 reference"]
fn cuda_compact_fp32_qkv_projection_t32_component() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvProjectionT32)
}

fn observe_compact_fp16_t32(mode: CompactFp16Probe) -> Result<()> {
    let qkv_t32 = mode == CompactFp16Probe::QkvProjectionT32;
    let qkv_graph_only = mode == CompactFp16Probe::QkvProjectionGraphOnly;
    let qkv_projection = qkv_graph_only || qkv_t32;
    let graph_only = mode == CompactFp16Probe::GraphOnly || qkv_graph_only;
    let projection_taps = mode == CompactFp16Probe::Layer0ProjectionTaps;
    let layer0_taps = mode == CompactFp16Probe::Layer0Taps || projection_taps;
    let started_ns = airk_process::monotonic::now_ns()?;
    let started = Instant::now();
    let execution_user = airk_ipc::endpoint::current_user()?;
    let default_owner = diagnostic_default_owner()?;
    ensure!(
        execution_user == default_owner,
        "native Compact FP16 default owner differs"
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
        anyhow::bail!("native-emitted Compact FP16 Probe request required");
    };
    ensure!(schema == SCHEMA, "unknown Compact FP16 Probe schema");
    require_initial_bounds(
        &bounds,
        (cold_bytes, warm_bytes, headroom_bytes, job_limit_bytes),
    )?;
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let expires_ns = started_ns
        .checked_add(bounds.deadline_ms * 1_000_000)
        .context("original Compact FP16 monotonic expiry overflow")?;
    let (input, profile, files, _) = inputs_from(&gpu_input)?;
    let capacity = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &profile,
    )?;
    let metadata_pin = env_pin(
        "AIRK_CUDA_FP16_METADATA_RECEIPT",
        "AIRK_CUDA_FP16_METADATA_RECEIPT_SHA256",
    )?;
    let metadata: Value = serde_json::from_slice(&pinned(&metadata_pin, 1 << 20)?)?;
    ensure!(
        metadata["schema"]
            == if qkv_projection {
                QKV_METADATA_SCHEMA
            } else {
                "airk.private.compact-fp16-probe-metadata/1"
            }
            && metadata["probe_request"] == json!(request_pin)
            && metadata["native_input"] == json!(gpu_input)
            && metadata["profile"] == json!(profile.manifest)
            && metadata["profile_pin"] == json!(input.profile)
            && metadata["source_manifest"] == json!(input.source_manifest)
            && metadata["runtime_manifest"] == json!(runtime_manifest)
            && metadata["execution_user"] == execution_user
            && metadata["qualified"] == false
            && metadata["model_requests_dispatched"] == false
            && metadata["runtime_admission_dispatched"] == false,
        "Compact FP16 Probe does not bind the exact native metadata receipt"
    );
    let proof_pin: ResourceFilePin = serde_json::from_value(metadata["independent_proof"].clone())?;
    let proof: IndependentProof = serde_json::from_slice(&pinned(&proof_pin, PROOF_BYTES)?)?;
    validate_independent_proof(&proof)?;
    validate_reference(&proof.reference, &profile, &input.policy)?;
    let projection = qkv_projection
        .then(|| qkv_projection_stage(&proof))
        .transpose()?;
    let candidate = projection
        .as_ref()
        .map_or(&proof.candidate, |(_, candidate)| candidate);
    if let Some((stage, _)) = &projection {
        ensure!(
            metadata["projection_stage"] == json!(stage),
            "Compact FP32 Q/K/V metadata stage differs"
        );
    }
    let matching_projection = if qkv_t32 {
        let (stage, candidate) = projection
            .as_ref()
            .context("missing Q/K/V projection stage")?;
        let matching = qkv_projection_reference(stage, candidate, &proof)?;
        validate_reference(&matching.1, &profile, &input.policy)?;
        Some(matching)
    } else {
        None
    };
    let reference = matching_projection
        .as_ref()
        .map_or(&proof.reference, |(_, reference)| reference);
    bind_t32_candidate(&input, &profile, &metadata, reference, candidate)?;
    let reviewed_graph = if mode == CompactFp16Probe::BoundedT32 || layer0_taps || qkv_t32 {
        let pin = env_pin(
            "AIRK_CUDA_FP16_REVIEWED_GRAPH",
            "AIRK_CUDA_FP16_REVIEWED_GRAPH_SHA256",
        )?;
        let expected = if qkv_t32 {
            QKV_NATIVE_GRAPH_SHA256
        } else {
            REVIEWED_FP16_GRAPH_SHA256
        };
        ensure!(
            pin.sha256 == expected.parse::<Sha256Digest>()?,
            "unreviewed Compact FP16 graph evidence"
        );
        let bytes = pinned(&pin, PROOF_BYTES)?;
        Some((pin, bytes))
    } else {
        None
    };
    pinned(&runtime_manifest, 128 << 10)?;
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    ensure!(
        manifest.as_path() == Path::new(&runtime_manifest.path),
        "Compact FP16 manifest differs from the guarded runtime directory"
    );

    let output = output()?;
    let prefix = if graph_only {
        ensure!(
            output.is_absolute(),
            "absolute Compact FP16 diagnostic output required"
        );
        None
    } else {
        let prefix = PathBuf::from(text("AIRK_CUDA_FP16_PROFILE_PREFIX")?);
        ensure!(
            output.is_absolute()
                && prefix.is_absolute()
                && prefix.parent() == output.parent()
                && prefix.file_name().is_some()
                && prefix != output
                && !prefix.exists(),
            "fresh absolute output and guarded Compact FP16 profile prefix required"
        );
        Some(prefix)
    };
    let binary = file_hash(&std::env::current_exe()?)?;
    let actual_job = job_limits(bounds.host_limit_bytes, job_limit_bytes)?;
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
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let mut receipt = json!({
        "schema": match mode {
            CompactFp16Probe::StrictT32 => "airk.private.compact-fp16-t32-component/2",
            CompactFp16Probe::GraphOnly => "airk.private.compact-fp16-graph-diagnostic/1",
            CompactFp16Probe::QkvProjectionGraphOnly => QKV_GRAPH_SCHEMA,
            CompactFp16Probe::QkvProjectionT32 => QKV_T32_SCHEMA,
            CompactFp16Probe::BoundedT32 => "airk.private.compact-fp16-bounded-t32-component/1",
            CompactFp16Probe::Layer0Taps => "airk.private.compact-fp16-layer0-taps/1",
            CompactFp16Probe::Layer0ProjectionTaps => "airk.private.compact-fp16-layer0-projection-taps/1",
        },
        "complete": false, "qualified": false, "product_admission": false,
        "request": request_pin, "native_input": gpu_input, "profile": profile.manifest,
        "source_manifest": input.source_manifest, "runtime_manifest": runtime_manifest,
        "metadata_receipt": metadata_pin, "independent_proof": proof_pin,
        "pid": std::process::id(), "native_binary_sha256": binary, "execution_user": execution_user,
        "default_owner": default_owner, "bounds": bounds, "actual_job": actual_job,
        "native_host_reservation_bytes": charge.bytes(), "resource_snapshot": resources.snapshot(),
        "original_expires_monotonic_ns": expires_ns, "runtime_profile": null, "load_logs": null, "rows": [],
        "input_refusals": [], "numeric_forward_attempts": 0, "session_teardown_observed": false,
        "device_teardown_observed": false,
        "native_forward_returned": false, "forward_synchronized": false,
        "source_mask": {
            "independent_complete_graph_proof": proof_pin,
            "verified_candidate_graph": candidate.graph.sha256,
            "compact_graph_uses_attention_mask_shape_only": true,
            "mask_values_require_all_live_before_dispatch": true,
        },
        "measured_peak_device_bytes": null, "native_backend_attested": false,
    });
    if graph_only {
        receipt["graph_observation"] = Value::Null;
    }
    if let Some((stage, _)) = &projection {
        receipt["projection_stage"] = json!(stage);
    }
    if let Some((pin, _)) = &matching_projection {
        receipt["projection_reference"] = json!(pin);
    }
    if layer0_taps {
        receipt["layer0_final_graph"] = Value::Null;
        receipt["layer0_taps"] = json!([]);
    }
    if let Some((pin, _)) = &reviewed_graph {
        receipt["reviewed_graph"] = json!(pin);
        receipt["bounded_policy_identity"] = Value::Null;
        receipt["bounded_profile"] = Value::Null;
    }
    flush_graph_diagnostic(&mut file, &receipt)?;
    let mut journal = operation::Journal::create_until(
        &output,
        operation::Identity {
            input_sha256: request_pin.sha256,
            profile: profile.manifest,
            process_id: std::process::id(),
            native_binary_sha256: binary,
        },
        expires_ns,
    )?;
    let run = owner.run(|| -> Result<()> {
        let mut custody = Custody {
            scorer: None, device: None, owner: owner.clone(), lease: lease.clone(), charge: Some(charge),
            control: &mut control_parent, files: Some(files),
        };
        let work = (|| -> Result<()> {
            ensure!(Instant::now() < deadline, "original Compact FP16 deadline expired before admission");
            let files = custody.files.as_ref().context("Compact FP16 native asset custody missing")?;
            verify_files(files, &profile)?;
            let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
            ensure!(runtime.manifest_identity() == runtime_manifest.sha256, "actual Compact FP16 runtime manifest differs");
            let adapters = runtime.adapters(deadline)?;
            ensure!(adapters.len() == 1, "Compact FP16 probe requires exactly one observed adapter");
            let device = runtime.device(adapters[0].uuid, deadline)?;
            custody.device = Some(device.clone());
            receipt["adapter_before"] = json!(device.observation(deadline)?);
            flush_graph_diagnostic(&mut file, &receipt)?;
            let catalog = airk_kernel::inference_owner::evaluation_catalog(lease.clone(), vec![profile.clone()])?;
            let execution = catalog.execution_profile(profile.manifest)?.bind_cuda_probe(device, capacity, deadline)?;
            let Some(profile_prefix) = prefix.as_deref() else {
                let observer = execution.cuda_graph_observer()?;
                let loaded = journal.observe(operation::Kind::Load, 0, None, || {
                    if qkv_projection {
                        Scorer::load_private_compact_fp32_qkv_projection_diagnostic(
                            execution, files, proof.candidate.graph.sha256, candidate.graph.sha256,
                        )
                    } else {
                        Scorer::diagnose_cuda_graph_metadata(execution, files)
                    }
                })?;
                receipt["load_us"] = json!(loaded.elapsed_us);
                receipt["load_native_call_us"] = json!(loaded.native_call_elapsed_us);
                let snapshot = observer.snapshot()?;
                receipt["graph_observation"] = serde_json::to_value(&snapshot)?;
                receipt["session_teardown_observed"] = json!(snapshot
                    .teardown
                    .as_ref()
                    .is_some_and(|teardown| teardown.validate().is_ok()));
                flush_graph_diagnostic(&mut file, &receipt)?;
                loaded.value?;
                ensure!(loaded.within_deadline, "original Compact FP16 graph load expiry exceeded");
                return snapshot.validate_graph_metadata();
            };
            let observer = execution.cuda_compact_fp16_load_observer()?;
            let loaded = journal.observe(operation::Kind::Load, 0, None, || {
                if let Some((pin, graph_bytes)) = &reviewed_graph {
                    if qkv_t32 {
                        Scorer::open_private_compact_fp32_qkv_projection_profiled(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    } else if projection_taps {
                        Scorer::open_compact_fp16_layer0_projection_taps(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    } else if layer0_taps {
                        Scorer::open_compact_fp16_layer0_taps(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    } else {
                        Scorer::open_compact_fp16_bounded_hybrid_profiled(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    }
                } else {
                    Scorer::open_profiled(execution, files, profile_prefix)
                }
            })?;
            receipt["load_us"] = json!(loaded.elapsed_us);
            receipt["load_native_call_us"] = json!(loaded.native_call_elapsed_us);
            let logs = observer.snapshot();
            if let Ok(snapshot) = &logs {
                receipt["load_logs"] = json!(snapshot.logs);
            }
            custody.scorer = Some(loaded.value?);
            logs?;
            ensure!(loaded.within_deadline, "original Compact FP16 load expiry exceeded");
            let scorer = custody.scorer.as_ref().context("Compact FP16 scorer missing")?;
            receipt["execution_identity"] = json!(scorer.execution_identity()?);
            receipt["final_placement"] = json!(scorer.device_placement()?.context("actual Compact FP16 final placement missing")?);
            receipt["attention_plan"] = if reviewed_graph.is_some() {
                let (graph, policy) = scorer.compact_fp16_bounded_hybrid_evidence()?;
                if layer0_taps {
                    receipt["layer0_final_graph"] = serde_json::to_value(graph)?;
                }
                receipt["bounded_policy_identity"] = json!(policy);
                scorer.compact_fp16_bounded_hybrid_attention_plan()?
            } else {
                scorer.attention_backend_plan()?.context("actual Compact FP16 attention plan missing")?
            };
            flush_graph_diagnostic(&mut file, &receipt)?;
            observe_t32_input_refusals(scorer, &reference.input, &mut receipt, &mut file)?;
            receipt["numeric_forward_attempts"] = json!(1);
            flush_graph_diagnostic(&mut file, &receipt)?;
            let scored = journal.observe(operation::Kind::Numeric, 1, Some(reference.input.input_digest), || {
                if qkv_t32 {
                    scorer
                        .score_private_compact_fp32_qkv_projection_t32(&reference.input, deadline)
                        .map(|scores| (scores, None))
                } else if layer0_taps {
                    scorer
                        .score_layer0_taps(&reference.input, deadline)
                        .map(|(scores, taps)| (scores, Some(taps)))
                } else {
                    scorer
                        .score_with_deadline(&reference.input, deadline)
                        .map(|scores| (scores, None))
                }
            })?;
            let (actual, taps) = scored.value?;
            receipt["native_forward_returned"] = json!(true);
            if let Some(taps) = taps {
                let roster = if projection_taps {
                    airk_retrieval::arbiter::runtime::COMPACT_FP16_LAYER0_PROJECTION_TAPS
                } else {
                    airk_retrieval::arbiter::runtime::COMPACT_FP16_LAYER0_TAPS
                };
                ensure!(taps.len() == roster.len(), "Compact FP16 layer-0 tap roster differs");
                receipt["layer0_taps"] = json!(roster
                    .iter()
                    .zip(taps)
                    .map(|(&(name, width), bits)| json!({"name": name, "width": width, "bits": bits}))
                    .collect::<Vec<_>>());
            }
            let (half_raw, half_mass, half_winner) = comparison(&actual, &reference.matching_fp16)?;
            let (original_raw, original_mass, original_winner) = comparison(&actual, &reference.original_fp32)?;
            receipt["rows"].as_array_mut().unwrap().push(json!({
                "name": reference.name, "tokens": 32, "input_digest": reference.input.input_digest,
                "raw_scores": actual, "matching_fp16_raw_error": half_raw, "matching_fp16_mass_error": half_mass,
                "matching_fp16_argmax_matches": half_winner, "original_fp32_raw_error": original_raw,
                "original_fp32_mass_error": original_mass, "original_fp32_argmax_matches": original_winner,
                "elapsed_us": scored.elapsed_us, "native_call_elapsed_us": scored.native_call_elapsed_us,
            }));
            flush_graph_diagnostic(&mut file, &receipt)?;
            if layer0_taps {
                ensure!(scored.within_deadline, "original Compact FP16 numeric expiry exceeded");
            } else {
                ensure!(
                    scored.within_deadline
                        && t32_reference_bounds((half_raw, half_mass, half_winner), original_mass),
                    "Compact FP16 T32 matching parity, original mass or time bound refused"
                );
            }
            Ok(())
        })();

        let mut teardown = Ok(());
        let session_opened = custody.scorer.is_some();
        if let Some(scorer) = custody.scorer.take() {
            let profile_capture = scorer.synchronize(deadline).and_then(|()| {
                ensure!(scorer.device_settled(), "Compact FP16 score remains unsettled");
                receipt["forward_synchronized"] = json!(receipt["native_forward_returned"] == true);
                read_t32_profile(
                    &scorer,
                    prefix.as_deref().context("Compact FP16 scorer profile prefix missing")?,
                )
            });
            match profile_capture {
                Ok((trace, bytes)) => {
                    receipt["runtime_profile"] = trace;
                    if reviewed_graph.is_some() {
                        let profile_summary = scorer.compact_fp16_bounded_hybrid_evidence().and_then(|(graph, _)| {
                            super::hybrid_profile_trace::compact_fp16_t32_profiled_run(
                                &bytes, &graph.rows, if qkv_t32 { 1942 } else { 1746 },
                            )
                        });
                        match profile_summary {
                            Ok(summary) => receipt["bounded_profile"] = json!(summary),
                            Err(error) => {
                                receipt["bounded_profile_error"] = json!(format!("{error:#}"));
                                teardown = Err(error);
                            }
                        }
                    }
                }
                Err(error) => {
                    receipt["runtime_profile_error"] = json!(format!("{error:#}"));
                    teardown = Err(error);
                }
            }
            drop(scorer);
        }
        if let Some(device) = custody.device.as_ref() {
            let release = device.synchronize(deadline).and_then(|()| {
                ensure!(!device.poisoned() && !cuda::process_poisoned(), "Compact FP16 teardown remains unknown");
                if graph_only {
                    ensure!(device.pending_claims() == Some(0), "Compact FP16 graph device claims remain unsettled");
                } else {
                    receipt["session_teardown_observed"] = json!(session_opened);
                }
                receipt["device_teardown_observed"] = json!(true);
                receipt["adapter_after"] = json!(device.observation(deadline)?);
                Ok(())
            });
            if let Err(error) = &release {
                receipt["teardown_error"] = json!(format!("{error:#}"));
            }
            teardown = teardown.and(release);
        }
        if let Some((pin, _)) = &reviewed_graph {
            teardown = teardown.and(pinned(pin, PROOF_BYTES).map(|_| ()));
        }
        if let Some((pin, _)) = &matching_projection {
            teardown = teardown.and(pinned(pin, PROOF_BYTES).map(|_| ()));
        }
        work.and(teardown)?;
        verify_files(custody.files.as_ref().context("Compact FP16 post-input custody missing")?, &profile)?;
        for (pin, maximum) in [
            (&request_pin, 1 << 20), (&gpu_input, 1 << 20), (&input.profile, 1 << 20),
            (&metadata_pin, 1 << 20), (&runtime_manifest, 128 << 10),
            (&input.source_manifest, PROOF_BYTES), (&proof_pin, PROOF_BYTES),
            (&proof.reference.fixture.pin(), PROOF_BYTES), (&proof.reference.producer.pin(), PROOF_BYTES),
        ] {
            pinned(pin, maximum)?;
        }
        ensure!(Instant::now() < deadline, "original Compact FP16 expiry exceeded during post-input proofs");
        Ok(())
    }).and_then(std::convert::identity);

    drop(owner);
    drop(lease);
    let retained = control_parent.is_none();
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
    receipt["native_control_retained_for_unknown_cuda"] = json!(retained);
    receipt["native_control_cleanup_observed"] = json!(cleaned);
    receipt["native_control_cleanup_error"] =
        json!(cleanup.as_ref().err().map(ToString::to_string));
    receipt["native_host_charge_release_observed"] = json!(released);
    receipt["process_cuda_poisoned"] = json!(poisoned);
    receipt["peak_process_rss_bytes"] = json!(peak);
    let result = run.and_then(|()| {
        ensure!(
            receipt["session_teardown_observed"] == true
                && !retained
                && cleaned
                && released
                && !poisoned,
            "Compact FP16 session or host release unproven"
        );
        final_host_peak(peak, bounds.host_limit_bytes)?;
        Ok(())
    });
    receipt["complete"] = json!(result.is_ok());
    receipt["elapsed_us_to_final_report_start"] = json!(started.elapsed().as_micros());
    if let Err(error) = &result {
        receipt["error"] = json!(format!("{error:#}"));
        if let Some(refusal) = error.downcast_ref::<cuda::PlacementRefusal>() {
            receipt["placement_refusal"] = json!(&refusal.observation);
        }
        if let Some(refusal) =
            error.downcast_ref::<airk_retrieval::arbiter::runtime::CompactFp16Layer0GraphRefusal>()
        {
            receipt["layer0_final_graph"] = serde_json::to_value(&refusal.observation)?;
        }
    }
    flush_graph_diagnostic(&mut file, &receipt)?;
    if Instant::now() >= deadline {
        receipt["complete"] = json!(false);
        receipt["final_report_retention_exceeded_original_deadline"] = json!(true);
        flush_graph_diagnostic(&mut file, &receipt)?;
        return result.and_then(|()| {
            anyhow::bail!("original Compact FP16 expiry exceeded during final report retention")
        });
    }
    result
}
