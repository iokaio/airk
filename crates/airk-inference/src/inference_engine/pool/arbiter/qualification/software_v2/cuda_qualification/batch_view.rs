// SPDX-License-Identifier: Apache-2.0
//! Private data-only metadata for the reviewed Large FP32 batch-view candidate.
use super::*;
use airk_core::arbiter::{Task, context_action::calibration::JointCalibration};

const ORIGINAL_REQUEST: &str = "9a7daee2c118030c979852efc6780f011356af320e0c3fe0a6d1d9a2a0ab33b0";
const ORIGINAL_GRAPH: &str = "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94";
const BATCH_VIEW_GRAPH: &str = "5c9a26693eec8522bbfb4d5c4445fa0b60684449a5ea2e1098ec71cca96405b1";
const PREPARATION: &str = "ea6ebdadacf0b776e00281172edc4c756ee396ae03eb8db0596c420218315039";

#[test]
#[ignore = "data-only registration of the exact reviewed Large FP32 batch-view candidate"]
fn emit_cuda_large_fp32_batch_view_metadata() -> Result<()> {
    let started = Instant::now();
    let execution_user = airk_ipc::endpoint::current_user()?;
    ensure!(
        execution_user == diagnostic_default_owner()?,
        "native batch-view default owner differs"
    );
    let (request, parent_request) = request()?;
    ensure!(
        parent_request.sha256 == ORIGINAL_REQUEST.parse()?,
        "the original observed Large request is required"
    );
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
        anyhow::bail!("original native Large Probe required");
    };
    let (mut input, mut profile, files, _) = inputs_from(&gpu_input)?;
    ensure!(
        schema == SCHEMA
            && profile.model == ModelChoice::ContextQwenLarge
            && profile.cpu_fit.is_none()
            && input.numeric_fixture.is_none(),
        "expected the exact unqualified Large GPU input"
    );
    profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &profile,
    )?;
    let previous_profile = profile.manifest;
    let graph = env_pin(
        "AIRK_CUDA_BATCH_VIEW_MODEL",
        "AIRK_CUDA_BATCH_VIEW_MODEL_SHA256",
    )?;
    let preparation = env_pin(
        "AIRK_CUDA_BATCH_VIEW_PREPARATION",
        "AIRK_CUDA_BATCH_VIEW_PREPARATION_SHA256",
    )?;
    ensure!(
        graph.sha256 == BATCH_VIEW_GRAPH.parse()? && preparation.sha256 == PREPARATION.parse()?,
        "reviewed Large batch-view graph or preparation receipt differs"
    );
    let graph_bytes = pinned(&graph, 16 << 20)?;
    let prepared: Value = serde_json::from_slice(&pinned(&preparation, 1 << 20)?)?;
    ensure!(
        prepared["result"]["source_sha256"] == ORIGINAL_GRAPH
            && prepared["result"]["candidate_sha256"] == BATCH_VIEW_GRAPH
            && prepared["result"]["candidate_bytes"] == graph_bytes.len()
            && prepared["result"]["mask_adds"] == 36
            && prepared["result"]["native_scoring_dispatched"] == false,
        "batch-view preparation does not bind this original and candidate graph"
    );
    pinned(&runtime_manifest, 128 << 10)?;
    let calibration_asset = profile
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("original Large calibration asset missing")?;
    let calibration_name = calibration_asset.name.clone();
    let task_policy = profile.task_policies[&Task::ContextAction];
    let mut calibration = JointCalibration::parse_and_validate(
        &files.read_asset(calibration_asset)?,
        &profile,
        task_policy,
    )?;
    ensure!(
        calibration.rules.is_empty(),
        "expected raw-only Large calibration"
    );
    let model = profile
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Model)
        .context("original Large model asset missing")?;
    let device = profile
        .device_execution
        .as_mut()
        .context("original Large CUDA descriptor missing")?;
    ensure!(
        model.sha256 == ORIGINAL_GRAPH.parse()? && device.graph == model.sha256,
        "original Large model and device graph differ"
    );
    model.bytes = graph_bytes.len() as u64;
    model.sha256 = graph.sha256;
    device.graph = graph.sha256;
    calibration
        .assets
        .get_mut(&model.name)
        .context("calibration model binding missing")?
        .sha256 = graph.sha256;
    input.files.insert(model.name.clone(), graph.path.clone());
    let mut cal_bytes = serde_json::to_vec_pretty(&calibration)?;
    cal_bytes.push(b'\n');
    profile.calibration = sha(&cal_bytes);
    let asset = profile
        .assets
        .iter_mut()
        .find(|asset| asset.name == calibration_name)
        .context("original calibration asset missing")?;
    asset.sha256 = profile.calibration;
    asset.bytes = cal_bytes.len() as u64;
    profile.profile_id = "private-cuda-fp32-context-large-batch-view-v1".into();
    profile.manifest = Sha256Digest::from_bytes([0; 32]);
    let profile_bytes = serde_json::to_vec(&profile)?;
    profile.manifest = framed_digest(
        b"airk.private.large-fp32-batch-view-profile/1",
        &[
            previous_profile.as_bytes(),
            parent_request.sha256.as_bytes(),
            preparation.sha256.as_bytes(),
            &profile_bytes,
        ],
    );
    profile.validate()?;
    calibration.validate(&profile, task_policy)?;

    let directory = PathBuf::from(text("AIRK_CUDA_METADATA_DIRECTORY")?);
    ensure!(directory.is_absolute(), "absolute guarded output required");
    std::fs::create_dir(&directory)?;
    input.source_manifest = write_json(
        &directory.join("source-manifest.json"),
        &json!({
            "schema": "airk.private.large-fp32-batch-view-source/1",
            "parent_source": input.source_manifest,
            "parent_input": gpu_input,
            "parent_profile": previous_profile,
            "preparation": preparation,
            "graph": graph,
            "assets": profile.assets,
        }),
    )?;
    let calibration_pin = write_new(&directory.join(&calibration_name), &cal_bytes)?;
    input.files.insert(calibration_name, calibration_pin.path);
    input.profile = write_json(&directory.join("profile.json"), &profile)?;
    input.validate()?;
    let native_input = write_json(&directory.join("native-input.json"), &input)?;
    let probe = Request::Probe {
        schema,
        gpu_input: native_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    };
    let probe_pin = write_json(&directory.join("probe-request.json"), &probe)?;
    ensure!(
        started.elapsed() < Duration::from_secs(120),
        "metadata deadline exceeded"
    );
    write_json(
        &directory.join("metadata-receipt.json"),
        &json!({
            "schema": "airk.private.large-fp32-batch-view-metadata/1",
            "parent_request": parent_request, "source_manifest": input.source_manifest,
            "profile": profile.manifest, "profile_pin": input.profile, "probe_request": probe_pin,
            "graph": graph, "preparation": preparation, "execution_user": execution_user,
            "model_requests_dispatched": false, "qualified": false,
        }),
    )?;
    Ok(())
}
