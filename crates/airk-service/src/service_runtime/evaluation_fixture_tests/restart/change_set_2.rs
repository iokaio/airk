// SPDX-License-Identifier: Apache-2.0
//! Private Answer evaluation, change set 2: Context option B profiles.
//!
//! One option-B variant of each CPU Context roster profile: the roster's
//! weights, compiler and tokenizer assets, the option-B task policy
//! (`airk_retrieval::arbiter::context_wide`) and a provisional calibration
//! bound to that policy (a Read is accepted; terminals stay unsupported). The
//! emitted inputs use change set 1's catalog-only format, so the change-set
//! fixture runs them unchanged; the kernel builds option-B rosters only when
//! the `context_b` layer is bound. Weights are never read or copied.

use super::super::*;
use airk_core::arbiter::{
    AssetRole, Task,
    context_action::calibration::{CALIBRATION_SCHEMA_V2, JointCalibration},
};
use airk_retrieval::arbiter::context_wide;

const OUTPUT: &str = "AIRK_ARBITER_EVALUATION_CHANGE_SET_2_OUTPUT";
const TAG: &str = "cs2-context-b";

fn manifest(source_sha256: Sha256Digest, profile: &Profile) -> Result<Sha256Digest> {
    let mut preimage = profile.clone();
    preimage.manifest = Sha256Digest::from_bytes([0; 32]);
    Ok(airk_core::identity::framed_digest(
        b"airk.private.answer-change-set-2-profile/1",
        &[
            source_sha256.as_bytes(),
            TAG.as_bytes(),
            &serde_json::to_vec(&preimage)?,
        ],
    ))
}

/// Change set 1's provisional rule, bound to the option-B task policy.
fn option_b_calibration(frozen: &JointCalibration) -> Result<JointCalibration> {
    let mut calibration = frozen.clone();
    let reason = "provisional evaluation: terminal not qualified";
    calibration.schema = CALIBRATION_SCHEMA_V2.into();
    calibration.task_policy = context_wide::policy_digest()?;
    calibration.rules = serde_json::from_value(serde_json::json!([{
        "action":"read","allowed_gaps":["unsupported"],"allowed_reasons":["additional_evidence_may_help"],
        "allowed_roles":["unknown"],"allowed_relevance":["uncertain"],"minimum_raw_margin":0.0}]))?;
    calibration.class_support = Some(serde_json::from_value(serde_json::json!([
        {"action":"read","support":{"status":"supported","qualification":null}},
        {"action":"sufficient","support":{"status":"unsupported","reason":reason}},
        {"action":"clarify","support":{"status":"unsupported","reason":reason}},
        {"action":"no_justified_read","support":{"status":"unsupported","reason":reason}}
    ]))?);
    calibration.validate_class_support()?;
    Ok(calibration)
}

/// The option-B variant: new task policy and Calibration asset, new manifest.
fn option_b_profile(source: &Profile, source_sha256: Sha256Digest, calibration: &[u8]) -> Result<Profile> {
    ensure!(
        matches!(source.model, ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge)
            && source.qualification.is_none()
            && source.device_execution.is_none(),
        "option B requires a private CPU Context roster profile"
    );
    let policy = context_wide::policy_digest()?;
    let mut profile = source.clone();
    let asset = profile
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("Context calibration asset missing")?;
    asset.bytes = calibration.len() as u64;
    asset.sha256 = sha(calibration);
    profile.calibration = asset.sha256;
    profile.task_policies = BTreeMap::from([(Task::ContextAction, policy)]);
    profile.profile_id = format!("{}-{TAG}", source.profile_id);
    profile.downloads.clear();
    profile.qualified_targets.clear();
    profile.manifest = manifest(source_sha256, &profile)?;
    profile.validate()?;
    ensure!(
        context_wide::policy_for(&profile)? == policy,
        "option-B profile is not admitted under its policy"
    );
    JointCalibration::parse_and_validate(calibration, &profile, policy)?;
    Ok(profile)
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

fn pretty(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Writes the option-B Context profiles, their calibrations and a catalog-only
/// inputs list (SHA-256 pinned) into a fresh folder under artifacts.
#[test]
#[ignore = "private change-set-2 profile emitter; pinned CPU roster and a fresh output folder required"]
fn emit_private_change_set_2_context_b_profiles() -> Result<()> {
    let input = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_INPUTS")
            .context("AIRK_ARBITER_EVALUATION_INPUTS missing")?,
    );
    let expected: Sha256Digest = std::env::var("AIRK_ARBITER_EVALUATION_INPUTS_SHA256")?.parse()?;
    let output = PathBuf::from(std::env::var_os(OUTPUT).context("change-set-2 output missing")?);
    let artifacts = artifact_root()?;
    ensure!(
        output.is_absolute()
            && !output.try_exists()?
            && output
                .parent()
                .context("output parent missing")?
                .canonicalize()?
                .starts_with(&artifacts),
        "change-set-2 output must be a fresh folder under artifacts"
    );
    let (_, mut raw) = source_file(&input, &artifacts)?;
    let raw = bounded(&mut raw)?;
    ensure!(sha(&raw) == expected, "CPU roster inputs hash differs");
    let roster: Vec<Input> = serde_json::from_slice(&raw)?;
    std::fs::create_dir(&output)?;
    let mut extras = Vec::new();
    let mut receipt = Vec::new();
    for entry in &roster {
        let (_, mut file) = source_file(&entry.profile, &artifacts)?;
        let bytes = bounded(&mut file)?;
        ensure!(sha(&bytes) == entry.profile_sha256, "roster profile bytes differ");
        let source: Profile = serde_json::from_slice(&bytes)?;
        source.validate()?;
        if !matches!(source.model, ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge) {
            continue;
        }
        let asset = source
            .assets
            .iter()
            .find(|asset| asset.role == AssetRole::Calibration)
            .context("roster calibration asset missing")?;
        let (_, mut file) = source_file(&entry.assets.join(&asset.name), &artifacts)?;
        let frozen_bytes = bounded(&mut file)?;
        ensure!(sha(&frozen_bytes) == asset.sha256, "roster calibration bytes differ");
        let frozen = JointCalibration::parse_and_validate(
            &frozen_bytes,
            &source,
            *source
                .task_policies
                .get(&Task::ContextAction)
                .context("roster Context policy missing")?,
        )?;
        let calibration = pretty(&option_b_calibration(&frozen)?)?;
        let name = format!("{:?}-{TAG}-calibration.json", source.model).to_lowercase();
        let (calibration_path, calibration_sha256) = write_new(&output, &name, &calibration)?;
        let profile = option_b_profile(&source, entry.profile_sha256, &calibration)?;
        let (path, digest) = write_new(&output, &format!("{}.json", profile.profile_id), &pretty(&profile)?)?;
        let overrides = serde_json::json!({ asset.name.clone(): {"path": calibration_path, "sha256": calibration_sha256} });
        receipt.push(serde_json::json!({
            "variant": TAG, "model": profile.model, "profile": path, "profile_sha256": digest,
            "manifest": profile.manifest, "source_profile": entry.profile, "source_profile_sha256": entry.profile_sha256,
            "source_manifest": source.manifest, "task_policies": profile.task_policies,
            "calibration": profile.calibration, "overrides": overrides, "weights_copied": false,
        }));
        // Change set 1's catalog-only input format (`change_set::ExtraInput`).
        extras.push(serde_json::json!({
            "profile": path, "assets": entry.assets, "fixtures": entry.fixtures,
            "profile_sha256": digest, "overrides": overrides,
        }));
    }
    ensure!(!extras.is_empty(), "CPU roster has no Context profile");
    let (inputs_path, inputs_sha256) =
        write_new(&output, "change-set-2-context-b-inputs.json", &pretty(&extras)?)?;
    let report = serde_json::json!({
        "schema": "airk.private.answer-change-set-2-profiles/1",
        "evaluation": true, "release_qualified": false,
        "cpu_inputs": input, "cpu_inputs_sha256": expected,
        "change_set_inputs": inputs_path, "change_set_inputs_sha256": inputs_sha256,
        "context_b_policy": context_wide::policy_digest()?,
        "context_product_policy": airk_retrieval::arbiter::context_action::policy_digest()?,
        "roster_units": context_wide::UNITS, "roster_reads": context_wide::READS,
        "profiles": receipt,
    });
    write_new(&output, "change-set-2-profiles.json", &pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[test]
fn option_b_calibration_binds_its_own_policy_and_admits_only_read() -> Result<()> {
    use airk_core::arbiter::context_action::calibration::{ClassSupport, ContextActionClass};
    let frozen: JointCalibration = serde_json::from_value(serde_json::json!({
        "schema":"airk.arbiter.context-action.calibration/2","model":"arbiter-context-qwen3-17b",
        "compiler":Sha256Digest::from_bytes([1;32]),"task_policy":Sha256Digest::from_bytes([2;32]),
        "score_abi":{"readout":"decision_symbol","inputs":[{"name":"input_ids","element":"int64","axes":["batch","tokens"]}],
            "output":{"name":"scores","element":"float32","axes":["batch","options"]}},
        "assets":{},"rules":[],
        "class_support":[
            {"action":"read","support":{"status":"unsupported","reason":"probe"}},
            {"action":"sufficient","support":{"status":"unsupported","reason":"probe"}},
            {"action":"clarify","support":{"status":"unsupported","reason":"probe"}},
            {"action":"no_justified_read","support":{"status":"unsupported","reason":"probe"}}]}))?;
    let option_b = option_b_calibration(&frozen)?;
    assert_eq!(option_b.task_policy, context_wide::policy_digest()?);
    assert_ne!(
        option_b.task_policy,
        airk_retrieval::arbiter::context_action::policy_digest()?
    );
    assert_eq!(option_b.compiler, frozen.compiler);
    assert_eq!(option_b.rules.len(), 1);
    assert_eq!(option_b.rules[0].action, ContextActionClass::Read);
    assert!(option_b.class_support.as_ref().unwrap().iter().all(|class| match class.action {
        ContextActionClass::Read => matches!(class.support, ClassSupport::Supported { qualification: None }),
        _ => matches!(class.support, ClassSupport::Unsupported { .. }),
    }));
    Ok(())
}

#[test]
fn option_b_manifests_are_new_identities() -> Result<()> {
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.model = ModelChoice::ContextQwenCompact;
    let source = Sha256Digest::from_bytes([3; 32]);
    let first = manifest(source, &profile)?;
    assert_eq!(first, manifest(source, &profile)?);
    assert_ne!(first, manifest(Sha256Digest::from_bytes([4; 32]), &profile)?);
    assert_ne!(first, profile.manifest);
    Ok(())
}
