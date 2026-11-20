// SPDX-License-Identifier: Apache-2.0
//! Private Large baseline metadata and disposable final-graph diagnostics.
use super::*;
use airk_core::arbiter::device::{DevicePrecision, PlacementPolicy};

const REFERENCE: &str = "264a3f5a9ddfb9f43a22328c88ce6497624dec87c6c8659fd5b5a7b3a6464ec3";
const PREPARE: &str = "8e69df4aedb85c556330262051d46c610ab48cec3c2a71aba60039458b6db348";
const ORIGINAL: &str = "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94";
const GRAPH: &str = "c00371ad9407c25bd52999d87327115025b7c6f55a4e585645ca112d858de5bd";

#[derive(Deserialize)]
struct Reference {
    graph: ReferenceFile,
    weights: ReferenceFile,
    input_digest: Sha256Digest,
    source_inputs: ReferenceFile,
    matching_fp16: Vec<f32>,
    original_fp32: Vec<f32>,
}

#[derive(Deserialize)]
struct ReferenceFile {
    path: String,
    bytes: u64,
    sha256: Sha256Digest,
}

impl ReferenceFile {
    fn asset(&self, name: &str, role: AssetRole) -> Result<Asset> {
        ensure!(
            Path::new(&self.path).is_absolute()
                && Path::new(&self.path).file_name().and_then(|n| n.to_str()) == Some(name),
            "Large independently authenticated file location differs"
        );
        Ok(Asset {
            name: name.into(),
            role,
            bytes: self.bytes,
            sha256: self.sha256,
        })
    }
}

fn select_reference_row(rows: Vec<NumericCase>, digest: Sha256Digest) -> Result<NumericCase> {
    let mut matches = rows
        .into_iter()
        .filter(|row| row.input.input_digest == digest);
    let row = matches
        .next()
        .context("independent Large T32 input missing")?;
    ensure!(
        matches.next().is_none(),
        "independent Large T32 input digest repeated"
    );
    Ok(row)
}

#[test]
fn unique_large_t32_reference_among_multiple_widths() -> Result<()> {
    fn row(name: &str, width: usize, digest: Sha256Digest) -> Result<NumericCase> {
        Ok(serde_json::from_value(json!({
            "name":name,"input":{"prompt":"","input_ids":vec![1; width],"attention_mask":vec![1; width],
            "position_ids":(0..width).collect::<Vec<_>>(),"candidate_positions":[],"option_mask":vec![false;8],
            "decision_position":0,"input_digest":digest},"reference_same_artifact":null,
            "reference_original_fp32":null
        }))?)
    }
    let t32 = sha(b"selected T32");
    let alternate = sha(b"different width");
    let target = row("32", 32, t32)?;
    let other = row("128", 128, alternate)?;
    assert_eq!(
        select_reference_row(vec![other.clone(), target.clone()], t32)?.name,
        "32"
    );
    assert!(select_reference_row(vec![other], t32).is_err());
    assert!(select_reference_row(vec![target.clone(), target], t32).is_err());
    Ok(())
}

pub(super) fn t32_references(
    profile: &Profile,
    input: &ResourceRunInput,
) -> Result<super::bounded_hybrid::ProfileReferences> {
    let proof = env_pin(
        "AIRK_CUDA_FP16_INDEPENDENT_PROOF",
        "AIRK_CUDA_FP16_INDEPENDENT_PROOF_SHA256",
    )?;
    ensure!(
        proof.sha256 == REFERENCE.parse()?
            && profile.model == ModelChoice::ContextQwenLarge
            && profile
                .device_execution
                .as_ref()
                .is_some_and(|d| d.precision == DevicePrecision::Float16
                    && d.placement == PlacementPolicy::StrictGpu)
            && profile.qualification.is_none()
            && profile.qualified_targets.is_empty(),
        "Large T32 requires the exact independent reference and unqualified FP16 descriptor"
    );
    let reference: Reference = serde_json::from_slice(&pinned(&proof, 1 << 20)?)?;
    for (key, artifact) in [
        ("model.onnx", &reference.graph),
        ("weights.bin", &reference.weights),
    ] {
        ensure!(
            input.files.get(key) == Some(&artifact.path)
                && profile.assets.iter().any(|asset| asset.name == key
                    && asset.sha256 == artifact.sha256
                    && asset.bytes == artifact.bytes),
            "Large FP16 reference and current asset differ"
        );
    }
    let inputs = ResourceFilePin {
        path: reference.source_inputs.path,
        sha256: reference.source_inputs.sha256,
    };
    let data: Value = serde_json::from_slice(&pinned(&inputs, 1 << 20)?)?;
    let rows: Vec<NumericCase> = serde_json::from_value(data["rows"].clone())?;
    let mut row = select_reference_row(rows, reference.input_digest)?;
    ensure!(
        row.input.input_digest == reference.input_digest
            && row.input.input_ids.len() == 32
            && row.input.attention_mask == [1; 32]
            && row.input.option_mask == [true; 8]
            && row.reference_original_fp32.as_ref() == Some(&reference.original_fp32)
            && reference.matching_fp16.len() == 8
            && reference.matching_fp16.iter().all(|v| v.is_finite()),
        "Large independent reference is paired to a different input or original vector"
    );
    row.reference_same_artifact = Some(reference.matching_fp16);
    let provenance = json!({"reference_basis":"independent_large_baseline_half", "independent_proof":proof, "reference_inputs":inputs});
    Ok(super::bounded_hybrid::ProfileReferences {
        rows: vec![row],
        provenance,
        pins: vec![(proof, 1 << 20), (inputs, 1 << 20)],
    })
}

#[test]
#[ignore = "owned Windows metadata for the independently attested Large FP16 baseline"]
fn emit_cuda_large_fp16_probe_metadata() -> Result<()> {
    let started = Instant::now();
    let user = airk_ipc::endpoint::current_user()?;
    ensure!(
        user == diagnostic_default_owner()?,
        "Large metadata owner differs"
    );
    let (request, request_pin) = request()?;
    ensure!(
        request_pin.sha256 == PREPARE.parse()?,
        "Large prepared CPU lineage or bounds differ"
    );
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
        anyhow::bail!("Large CPU prepare input required");
    };
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let job = job_limits(bounds.host_limit_bytes, job_limit_bytes)?;
    let (mut input, cpu, cpu_files, _) = inputs_from(&cpu_input)?;
    let capacity = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &cpu,
    )?;
    let original = ORIGINAL.parse()?;
    ensure!(
        schema == SCHEMA
            && cpu.model == ModelChoice::ContextQwenLarge
            && cpu.cpu_fit.is_some()
            && cpu.device_execution.is_none()
            && cpu
                .assets
                .iter()
                .any(|a| a.role == AssetRole::Model && a.sha256 == original),
        "Large authenticated CPU profile or original graph differs"
    );
    let approved_manifest: Sha256Digest = option_env!("AIRK_APPROVED_CUDA_MANIFEST_SHA256")
        .context("compiled manifest approval missing")?
        .parse()?;
    let approved_build: Sha256Digest = option_env!("AIRK_STATIC_CUDA_BUILD_SHA256")
        .context("compiled build approval missing")?
        .parse()?;
    ensure!(
        runtime_manifest.sha256 == approved_manifest,
        "Large runtime approval differs"
    );
    let runtime = pinned(&runtime_manifest, 128 << 10)?;
    let proof = env_pin(
        "AIRK_CUDA_FP16_INDEPENDENT_PROOF",
        "AIRK_CUDA_FP16_INDEPENDENT_PROOF_SHA256",
    )?;
    ensure!(
        proof.sha256 == REFERENCE.parse()?,
        "Large independent reference differs"
    );
    let reference: Reference = serde_json::from_slice(&pinned(&proof, 1 << 20)?)?;
    verify_files(&cpu_files, &cpu)?;
    ensure!(
        Instant::now() < deadline,
        "Large metadata expired during source verification"
    );
    let graph = reference.graph.asset("model.onnx", AssetRole::Model)?;
    let weights = reference
        .weights
        .asset("weights.bin", AssetRole::ExternalWeights)?;
    ensure!(
        graph.sha256 == GRAPH.parse()?
            && Path::new(&reference.graph.path).parent()
                == Path::new(&reference.weights.path).parent(),
        "Large candidate location or graph differs"
    );
    for asset in &cpu.assets {
        if matches!(asset.role, AssetRole::Model | AssetRole::ExternalWeights) {
            input.files.remove(&asset.name);
        }
    }
    for (asset, path) in [
        (&graph, &reference.graph.path),
        (&weights, &reference.weights.path),
    ] {
        ensure!(
            input
                .files
                .insert(asset.name.clone(), path.clone())
                .is_none(),
            "Large candidate file collision"
        );
    }
    let prepared = cuda::prepare_fp16_probe_profile(
        &cpu,
        &Files(input.files.clone()),
        vec![graph, weights],
        original,
        &runtime,
        runtime_manifest.sha256,
        capacity,
    )?;
    let device = prepared
        .profile
        .device_execution
        .as_ref()
        .context("Large CUDA descriptor missing")?;
    ensure!(
        device.graph == reference.graph.sha256
            && device.runtime_build == approved_build
            && device.precision == DevicePrecision::Float16
            && device.placement == PlacementPolicy::StrictGpu
            && prepared.original_graph == original
            && prepared.profile.qualification.is_none()
            && prepared.profile.qualified_targets.is_empty()
            && Instant::now() < deadline,
        "Large strict private descriptor or deadline differs"
    );
    let directory = PathBuf::from(text("AIRK_CUDA_METADATA_DIRECTORY")?);
    ensure!(
        directory.is_absolute(),
        "absolute Large metadata output required"
    );
    std::fs::create_dir(&directory)?;
    input.source_manifest = write_json(
        &directory.join("source-manifest.json"),
        &json!({
            "schema":"airk.private.large-fp16-source/1","parent_source":input.source_manifest,"parent_input":cpu_input,
            "parent_profile":cpu.manifest,"original_graph":original,"independent_proof":proof,"attention_plan":prepared.attention_plan,"assets":prepared.profile.assets
        }),
    )?;
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
        "Large metadata expired before receipt"
    );
    write_json(
        &output()?,
        &json!({
            "schema":"airk.private.large-fp16-probe-metadata/1","request":request_pin,"cpu_input":cpu_input,"runtime_manifest":runtime_manifest,
            "independent_proof":proof,"reference_input_digest":reference.input_digest,"source_manifest":input.source_manifest,
            "profile":prepared.profile.manifest,"profile_pin":input.profile,"calibration":calibration,"native_input":native_input,"probe_request":probe,
            "graph":reference.graph.sha256,"weights":reference.weights.sha256,"actual_job":job,"execution_user":user,"pid":std::process::id(),
            "native_binary_sha256":file_hash(&std::env::current_exe()?)?,"model_requests_dispatched":false,"qualified":false
        }),
    )?;
    Ok(())
}

#[test]
#[ignore = "owned disposable Windows Large FP16 graph observation without a scorer"]
fn cuda_large_fp16_load_only_graph_diagnostic() -> Result<()> {
    observe_cuda_graph_metadata(ModelChoice::ContextQwenLarge, DevicePrecision::Float16)
}
