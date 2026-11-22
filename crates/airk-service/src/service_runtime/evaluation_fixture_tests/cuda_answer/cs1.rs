// SPDX-License-Identifier: Apache-2.0
//! Private Answer evaluation, change set 1 with Context on CUDA.
//!
//! Composes the change-set lifetime (`restart::change_set`: CPU roster as the
//! restart identity, catalog-only change-set extras, kernel layers) with one
//! CUDA Context profile attached through the private CUDA Answer seam. The CUDA
//! profile is a calibration-only variant of a reviewed CUDA base (Compact or
//! Large FP32) that carries the change set's provisional Read rule. Its reviewed
//! final-graph result names the base, and the bounded-hybrid gate checks graph,
//! placement and adapter against that base, never against the variant.

use super::*;
use crate::service_runtime::evaluation_fixture_tests::restart::change_set;
use airk_core::arbiter::{AssetRole, Task};

const VARIANT_INPUTS: &str = "AIRK_ARBITER_EVALUATION_CUDA_INPUTS";
const BASE_PROFILE: &str = "AIRK_ARBITER_EVALUATION_CUDA_BASE_PROFILE";
const BASES: &str = "AIRK_ARBITER_EVALUATION_CUDA_BASES";
const OUTPUT: &str = "AIRK_ARBITER_EVALUATION_CUDA_VARIANT_OUTPUT";
const TAG: &str = "cs1-provisional-read";

/// A calibration-only variant of a reviewed CUDA Context base profile: only the
/// Calibration asset, the calibration field, the profile id and the manifest
/// change; the manifest is a new identity derived from the pinned base bytes.
fn cuda_variant(base: &Profile, base_sha256: Sha256Digest, calibration: &[u8]) -> Result<Profile> {
    ensure!(
        matches!(
            base.model,
            ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge
        ) && base.qualification.is_none()
            && base.profile_id.starts_with("private-")
            && base.device_execution.is_some(),
        "CUDA provisional-read variant requires a private CUDA Context base"
    );
    let mut profile = base.clone();
    let asset = profile
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("CUDA base calibration asset missing")?;
    asset.bytes = calibration.len() as u64;
    asset.sha256 = sha(calibration);
    profile.calibration = asset.sha256;
    profile.profile_id = format!("{}-{TAG}", base.profile_id);
    profile.manifest = Sha256Digest::from_bytes([0; 32]);
    profile.manifest = airk_core::identity::framed_digest(
        b"airk.private.answer-change-set-1-cuda-profile/1",
        &[
            base_sha256.as_bytes(),
            base.manifest.as_bytes(),
            TAG.as_bytes(),
            &serde_json::to_vec(&profile)?,
        ],
    );
    profile.validate()?;
    let policy = *profile
        .task_policies
        .get(&Task::ContextAction)
        .context("CUDA Context task policy missing")?;
    airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
        calibration,
        &profile,
        policy,
    )?;
    Ok(profile)
}

/// The pre-launch identity relation; the bounded-hybrid gate re-checks it
/// byte for byte against the reviewed base when the session opens.
fn require_variant_of(base: &Profile, variant: &Profile) -> Result<()> {
    ensure!(
        variant.model == base.model
            && variant.device_execution == base.device_execution
            && variant.manifest != base.manifest
            && variant.calibration != base.calibration
            && variant.qualification.is_none()
            && variant.profile_id == format!("{}-{TAG}", base.profile_id),
        "CUDA change-set profile is not this base's provisional-read variant"
    );
    Ok(())
}

fn pinned_profile(path: &Path, expected: Sha256Digest) -> Result<(Vec<u8>, Profile)> {
    let (_directory, mut file) = source_file(path, &artifact_root()?)?;
    let raw = bounded(&mut file)?;
    ensure!(sha(&raw) == expected, "pinned CUDA base profile differs");
    let profile: Profile = serde_json::from_slice(&raw)?;
    profile.validate()?;
    Ok((raw, profile))
}

fn pretty(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_new(directory: &Path, name: &str, bytes: &[u8]) -> Result<(PathBuf, Sha256Digest)> {
    use std::io::Write;
    let path = directory.join(name);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok((path, sha(bytes)))
}

/// Writes, for each pinned CUDA base roster entry, its provisional-read
/// calibration (derived from the base's own calibration exactly as the CPU
/// change set derives its own), the variant profile, and a one-entry change-set
/// input (calibration pinned as an override). Weights are never read or copied.
#[test]
#[ignore = "private CUDA change-set profile emitter; pinned CUDA base roster and a fresh output folder required"]
fn emit_private_cuda_provisional_read_variants() -> Result<()> {
    let input = PathBuf::from(std::env::var_os(BASES).context("CUDA base roster missing")?);
    let expected: Sha256Digest = std::env::var(format!("{BASES}_SHA256"))?.parse()?;
    let output = PathBuf::from(std::env::var_os(OUTPUT).context("CUDA variant output missing")?);
    let artifacts = artifact_root()?;
    ensure!(
        output.is_absolute()
            && !output.try_exists()?
            && output
                .parent()
                .context("output parent missing")?
                .canonicalize()?
                .starts_with(&artifacts),
        "CUDA variant output must be a fresh folder under artifacts"
    );
    let (_, mut raw) = source_file(&input, &artifacts)?;
    let raw = bounded(&mut raw)?;
    ensure!(sha(&raw) == expected, "CUDA base roster hash differs");
    let roster: Vec<super::super::Input> = serde_json::from_slice(&raw)?;
    ensure!(!roster.is_empty() && roster.len() <= 2, "one or two CUDA bases");
    std::fs::create_dir(&output)?;
    let mut receipt = Vec::new();
    for entry in &roster {
        let (_, base) = pinned_profile(&entry.profile, entry.profile_sha256)?;
        let asset = base
            .assets
            .iter()
            .find(|asset| asset.role == AssetRole::Calibration)
            .context("CUDA base calibration asset missing")?;
        let (_, mut file) = source_file(&entry.assets.join(&asset.name), &artifacts)?;
        let frozen_bytes = bounded(&mut file)?;
        ensure!(sha(&frozen_bytes) == asset.sha256, "CUDA base calibration bytes differ");
        let policy = *base
            .task_policies
            .get(&Task::ContextAction)
            .context("CUDA base Context policy missing")?;
        let frozen =
            airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
                &frozen_bytes,
                &base,
                policy,
            )?;
        let calibration = pretty(&change_set::provisional_calibration(&frozen)?)?;
        let model = format!("{:?}", base.model).to_lowercase();
        let (calibration_path, calibration_sha) =
            write_new(&output, &format!("cuda-{model}-provisional-read-calibration.json"), &calibration)?;
        let variant = cuda_variant(&base, entry.profile_sha256, &calibration)?;
        require_variant_of(&base, &variant)?;
        let (profile_path, profile_sha) =
            write_new(&output, &format!("{}.json", variant.profile_id), &pretty(&variant)?)?;
        let inputs = serde_json::json!([{
            "profile": profile_path,
            "assets": entry.assets,
            "fixtures": entry.fixtures,
            "profile_sha256": profile_sha,
            "overrides": {asset.name.clone(): {"path": calibration_path, "sha256": calibration_sha}},
        }]);
        let (inputs_path, inputs_sha) =
            write_new(&output, &format!("cuda-{model}-provisional-read-inputs.json"), &pretty(&inputs)?)?;
        receipt.push(serde_json::json!({
            "model": base.model, "base_profile": entry.profile, "base_profile_sha256": entry.profile_sha256,
            "base_manifest": base.manifest, "variant_profile": profile_path, "variant_profile_sha256": profile_sha,
            "variant_manifest": variant.manifest, "calibration": calibration_path, "calibration_sha256": calibration_sha,
            "inputs": inputs_path, "inputs_sha256": inputs_sha, "weights_copied": false,
        }));
    }
    let report = serde_json::json!({
        "schema": "airk.private.answer-change-set-1-cuda-profiles/1",
        "evaluation": true, "release_qualified": false,
        "bases": input, "bases_sha256": expected, "variants": receipt,
    });
    write_new(&output, "cuda-change-set-1-profiles.json", &pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private change-set 1 Answer with one CUDA Context provisional-read variant; exact reviewed inputs required"]
async fn actual_private_arbiter_fixture_change_set_1_cuda() -> Result<()> {
    let consent = std::env::var("AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT")
        .context("CUDA change-set fixture requires explicit resource configuration consent")?;
    let started = Instant::now();
    let seconds: u64 = std::env::var("AIRK_ARBITER_EVALUATION_SECONDS")?.parse()?;
    ensure!(
        (1..=21600).contains(&seconds),
        "private fixture lifetime must be explicit and at most six hours"
    );
    let artifacts = artifact_root()?;
    let variant_input = PathBuf::from(
        std::env::var_os(VARIANT_INPUTS).context("CUDA change-set variant inputs missing")?,
    );
    let variant_expected: Sha256Digest =
        std::env::var(format!("{VARIANT_INPUTS}_SHA256"))?.parse()?;
    let (variant_raw, mut variants, candidates) =
        change_set::prepare_extras(&variant_input, variant_expected)?;
    ensure!(
        variants.len() == 1 && candidates == [false],
        "the CUDA change-set input names exactly one variant and no Focus candidate"
    );
    let variant = variants.remove(0);
    let base_path = PathBuf::from(std::env::var_os(BASE_PROFILE).context("CUDA base profile missing")?);
    let base_expected: Sha256Digest = std::env::var(format!("{BASE_PROFILE}_SHA256"))?.parse()?;
    let (_, base) = pinned_profile(&base_path, base_expected)?;
    require_variant_of(&base, &variant.profile)?;
    let base_manifest = base.manifest;
    let mut graph = Input::read_matching(&artifacts, started + Duration::from_secs(seconds), |manifest| {
        ensure!(
            manifest == base_manifest,
            "reviewed CUDA graph result does not name the variant's base"
        );
        Ok(())
    })?;
    graph.graph = graph.graph.with_calibration_base(base.clone());
    let manifest = variant.profile.manifest;
    let record = serde_json::json!({
        "schema": "airk.private.answer-change-set-1-cuda/1",
        "cuda_variant_inputs_sha256": variant_expected,
        "cuda_variant_inputs_bytes": variant_raw.len(),
        "cuda_variant_manifest": manifest,
        "cuda_variant_profile_id": variant.profile.profile_id,
        "cuda_model": variant.profile.model,
        "cuda_base_profile": base_path,
        "cuda_base_profile_sha256": base_expected,
        "cuda_base_manifest": base_manifest,
        "graph_sha256": std::env::var("AIRK_ARBITER_EVALUATION_CUDA_GRAPH_SHA256")?,
    });
    let adapter = graph.adapter;
    let answer_graph = graph.graph;
    let attach = Box::new(
        move |owner: EvaluationOwner, source: &[Prepared]| -> Result<EvaluationOwner> {
            let entry = source
                .iter()
                .find(|entry| entry.profile.manifest == manifest)
                .context("CUDA change-set variant missing from the catalog")?;
            owner.with_cuda_answer(manifest, adapter, answer_graph, |asset| {
                entry
                    .sources
                    .get(&asset.name)
                    .context("original CUDA variant asset descriptor missing")?
                    .try_clone()
                    .map_err(Into::into)
            })
        },
    );
    change_set::run_with(
        software_v2_answer_template()?,
        cuda_answer_resource_configuration(&consent)?,
        Some(change_set::Extension {
            prepared: vec![variant],
            record,
            attach,
        }),
    )
    .await
}

#[test]
fn cuda_variant_relation_requires_the_same_device_binding_and_a_new_identity() -> Result<()> {
    let mut base: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    base.qualification = None;
    base.profile_id = "private-cuda-base".into();
    base.manifest = Sha256Digest::from_bytes([1; 32]);
    base.calibration = Sha256Digest::from_bytes([2; 32]);
    let mut variant = base.clone();
    variant.manifest = Sha256Digest::from_bytes([3; 32]);
    variant.calibration = Sha256Digest::from_bytes([4; 32]);
    variant.profile_id = format!("private-cuda-base-{TAG}");
    require_variant_of(&base, &variant)?;
    for refused in [
        Profile { manifest: base.manifest, ..variant.clone() },
        Profile { calibration: base.calibration, ..variant.clone() },
        Profile { profile_id: "private-other".into(), ..variant.clone() },
        Profile { model: ModelChoice::FocusQwenReranker06B, ..variant.clone() },
        Profile { qualification: Some(Sha256Digest::from_bytes([5; 32])), ..variant.clone() },
    ] {
        assert!(require_variant_of(&base, &refused).is_err());
    }
    Ok(())
}
