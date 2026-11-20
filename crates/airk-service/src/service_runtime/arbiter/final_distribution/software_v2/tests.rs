// SPDX-License-Identifier: Apache-2.0
//! Independent synthetic controls. These are not actual resources or qualification.
use super::*;
use airk_core::arbiter::resource_policy::ResourceSample;
fn profile() -> Result<Profile> {
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.model = ModelChoice::ContextQwenCompact;
    profile.stage = Some(Stage::Context);
    profile.max_tokens = 8192;
    profile.memory_reservation_bytes = 1000;
    Ok(profile)
}
fn resource_capture() -> Result<(Profile, ResourcePolicy, Value)> {
    let profile = profile()?;
    let policy = ResourcePolicy::software_v2();
    let mut series = ResourceSeries::new(1000)?;
    for index in 0..20 {
        series.begin_sample(&policy)?;
        series.finish_sample(
            ResourceSample {
                elapsed_us: index + 1,
                peak_process_rss_bytes: 100 + index,
            },
            &policy,
        )?;
    }
    let input = CompiledInput {
        prompt: "SYNTHETIC; NO MODEL EXECUTION".into(),
        input_ids: vec![1; 8192],
        attention_mask: vec![1; 8192],
        position_ids: (0..8192).collect(),
        decision_position: 8191,
        candidate_positions: (1..=8).collect(),
        option_mask: vec![true; 8],
        input_digest: Sha256Digest::from_bytes([71; 32]),
    };
    let capture = json!({"schema":"airk.arbiter.native-resource/2","candidate_resource_passed":true,
        "numeric_status":"unavailable_in_resource_only_run","input":input,"resource":series,
        "load_us":4,"load_peak_process_rss_bytes":80,
        "warmup":{"returned":true,"success":true,"elapsed_us":5,"peak_process_rss_bytes":100}});
    Ok((profile, policy, capture))
}
#[test]
fn complete_software_v2_resources_recompute_twenty_samples_and_real_shape() -> Result<()> {
    let (profile, policy, capture) = resource_capture()?;
    resource(&capture, &profile, &policy)?;
    assert_eq!(capture["resource"]["nearest_rank_p95_us"], 19);
    assert_eq!(capture["resource"]["maximum_us"], 20);
    let mut focus = profile;
    focus.model = ModelChoice::FocusQwenReranker06B;
    focus.stage = Some(Stage::Focus);
    focus.max_tokens = 512;
    let mut input: CompiledInput = serde_json::from_value(capture["input"].clone())?;
    input.input_ids = vec![0; 512];
    input.attention_mask = vec![1; 512];
    input.position_ids = (0..512).collect();
    input.decision_position = 511;
    input.candidate_positions = vec![2152, 9693];
    input.option_mask = vec![true, true];
    compiled_shape(&input, &focus, &policy)?;
    input.attention_mask[0..33].fill(0);
    assert!(compiled_shape(&input, &focus, &policy).is_err());
    let current = ResourcePolicy::focus_static1024();
    focus.max_tokens = 1024;
    assert!(compiled_shape(&input, &focus, &current).is_err());
    input.input_ids.resize(1024, 1);
    input.attention_mask = vec![1; 1024];
    input.position_ids = (0..1024).collect();
    input.decision_position = 1023;
    compiled_shape(&input, &focus, &current)?;
    input.attention_mask[0] = 0;
    assert!(compiled_shape(&input, &focus, &current).is_err());
    Ok(())
}
#[test]
fn native_numeric_comparison_uses_arrays_and_full_argmax() -> Result<()> {
    assert_eq!(comparison(&[3.0, 1.0], &[3.0, 1.0])?, (0.0, 0.0, true));
    let (raw, mass, same) = comparison(&[3.0, 1.0], &[1.0, 3.0])?;
    assert_eq!(raw, 2.0);
    assert!(mass > 0.5);
    assert!(!same);
    assert!(comparison(&[f32::NAN, 1.0], &[3.0, 1.0]).is_err());
    assert!(comparison(&[3.0, 1.0], &[3.0]).is_err());
    Ok(())
}
#[test]
fn historical_policy_is_absent_and_cannot_satisfy_wide_context() -> Result<()> {
    let profile = profile()?;
    let digest = Sha256Digest::from_bytes([72; 32]);
    let binding = Binding {
        model: profile.model,
        candidate_profile_sha256: digest,
        assets_sha256: digest,
        compiler: digest,
        calibration: digest,
        task_policy: digest,
        runtime: RUNTIME.into(),
        execution_identity: "synthetic".into(),
        target: "x86_64-pc-windows-msvc".into(),
        task_scope: "generic_read_unknown_uncertain".into(),
    };
    let raw = json!({"schema":"airk.private-final-stage-qualification/1","binding":binding,"gates":{},"component_target_provenance":{"path":"synthetic-unused","bytes":1,"sha256":digest}});
    let mut proof: FinalProof = serde_json::from_value(raw)?;
    let decision: RecordedDecision = serde_json::from_value(
        json!({"schema":"airk.private-recorded-stage-promotion/1","decision":"synthetic-only","approved_final_proofs":[]}),
    )?;
    assert!(policy(&profile, &proof, &decision).is_err());
    let mut historical = profile;
    historical.max_tokens = 2048;
    assert!(policy(&historical, &proof, &decision)?.is_none());
    assert!(
        serde_json::to_value(&proof)?
            .get("resource_policy")
            .is_none()
    );
    proof.resource_policy = Some(PinnedFile {
        path: PathBuf::from("synthetic-never-read"),
        bytes: 1,
        sha256: digest,
    });
    assert!(
        policy(&historical, &proof, &decision).is_err(),
        "unapproved source must refuse before path access"
    );
    Ok(())
}
pub(super) fn refusal_cases() -> Result<Vec<Value>> {
    let (profile, policy, capture) = resource_capture()?;
    let mut cases = Vec::new();
    for (id, mutated) in [
        ("software_v2_partial_sample_population", {
            let mut value = capture.clone();
            value["resource"]["samples"].as_array_mut().unwrap().pop();
            value
        }),
        ("software_v2_forged_p95", {
            let mut value = capture.clone();
            value["resource"]["nearest_rank_p95_us"] = json!(1);
            value
        }),
        ("software_v2_first_deadline_overrun_retained", {
            let mut value = capture.clone();
            value["resource"]["samples"][0]["elapsed_us"] = json!(120_000_001_u64);
            value
        }),
        ("software_v2_rss_overrun", {
            let mut value = capture.clone();
            value["resource"]["samples"][19]["peak_process_rss_bytes"] = json!(1001);
            value
        }),
        ("software_v2_lost_actual_rss", {
            let mut value = capture.clone();
            value["resource"]["samples"][19]["peak_process_rss_bytes"] = json!(1);
            value
        }),
        ("software_v2_old2048_not8192", {
            let mut value = capture.clone();
            value["input"]["input_ids"] = json!(vec![1; 2048]);
            value
        }),
        ("software_v2_hidden_option_omission", {
            let mut value = capture.clone();
            value["input"]["option_mask"][7] = json!(false);
            value
        }),
        ("software_v2_failed_warmup", {
            let mut value = capture.clone();
            value["warmup"]["success"] = json!(false);
            value
        }),
    ] {
        ensure!(
            resource(&mutated, &profile, &policy).is_err(),
            "synthetic /2 refusal failed: {id}"
        );
        cases.push(json!({"id":id,"input":mutated,"accepted":false,"synthetic_only":true}));
    }
    let root = std::env::current_dir()?;
    let path = root.join("crates/synthetic-source.rs");
    let digest = Sha256Digest::from_bytes([91; 32]);
    let group = Sha256Digest::from_bytes([92; 32]);
    let manifest = json!({"repository_root":root,"source_files":[{"path":path,"sha256":digest}]});
    let build = json!({"schema":"airk-readiness-command/1","exit_code":0,"inputs_unchanged":true,"changed_groups":[],"runner_unchanged":true,"runtime_unchanged":true,"command_inputs_unchanged":true,"command":["cargo","test"],"groups":{"native":{"sha256":group,"files":{"crates/synthetic-source.rs":digest}}}});
    let receipt = Sha256Digest::from_bytes([95; 32]);
    let receipts = BTreeSet::from([receipt]);
    source_identity(
        &manifest,
        &build,
        &BTreeSet::from([group]),
        receipt,
        &receipts,
    )?;
    let mut wrong = build.clone();
    wrong["groups"]["native"]["sha256"] = json!(Sha256Digest::from_bytes([93; 32]));
    ensure!(
        source_identity(
            &manifest,
            &wrong,
            &BTreeSet::from([group]),
            receipt,
            &receipts
        )
        .is_err(),
        "unreviewed actual-image source group accepted"
    );
    cases.push(json!({"id":"software_v2_unreviewed_native_source_group","input":wrong,"accepted":false,"synthetic_only":true}));
    let mut wrong = build;
    wrong["groups"]["native"]["files"]["crates/synthetic-source.rs"] =
        json!(Sha256Digest::from_bytes([94; 32]));
    ensure!(
        source_identity(
            &manifest,
            &wrong,
            &BTreeSet::from([group]),
            receipt,
            &receipts
        )
        .is_err(),
        "mismatched original source file accepted"
    );
    cases.push(json!({"id":"software_v2_changed_declared_native_source","input":wrong,"accepted":false,"synthetic_only":true}));
    ensure!(
        seal(&json!({"input_seals":{}}), &path, digest).is_err(),
        "missing actual owned input seal accepted"
    );
    cases.push(json!({"id":"software_v2_missing_actual_owned_request_seal","input":{"input_seals":{}},"accepted":false,"synthetic_only":true}));
    let mut joint_manifest = manifest.clone();
    joint_manifest["source_files"][0]["sha256"] = json!(Sha256Digest::from_bytes([97; 32]));
    let mut joint_build = wrong.clone();
    joint_build["groups"]["native"]["files"]["crates/synthetic-source.rs"] =
        json!(Sha256Digest::from_bytes([97; 32]));
    let unreviewed = Sha256Digest::from_bytes([96; 32]);
    joint_manifest["native_build_receipt"] =
        json!({"path":"synthetic-unreviewed","sha256":unreviewed});
    ensure!(
        source_identity(
            &joint_manifest,
            &joint_build,
            &BTreeSet::from([group]),
            unreviewed,
            &receipts
        )
        .is_err(),
        "jointly changed receipt/manifest copied a reviewed native group label"
    );
    cases.push(json!({"id":"software_v2_joint_forged_build_manifest_with_copied_group","input":{"manifest":joint_manifest,"build":joint_build},"accepted":false,"synthetic_only":true}));
    let mut padded: CompiledInput = serde_json::from_value(capture["input"].clone())?;
    padded.attention_mask[0] = 0;
    ensure!(
        numeric_shape(&padded, 8192, 8, false).is_err(),
        "padded Context8192 claimed all-live"
    );
    cases.push(json!({"id":"software_v2_padded8192_is_not_all_live","input":padded,"accepted":false,"synthetic_only":true}));
    Ok(cases)
}
#[test]
fn software_v2_promotion_refusals_preserve_original_partial_failures() -> Result<()> {
    assert_eq!(refusal_cases()?.len(), 13);
    Ok(())
}

#[test]
fn software_v2_numeric_shape_requires_all_context_tokens_but_preserves_focus_padding() -> Result<()>
{
    let (_, _, capture) = resource_capture()?;
    let mut input: CompiledInput = serde_json::from_value(capture["input"].clone())?;
    numeric_shape(&input, 8192, 8, false)?;
    input.attention_mask[0] = 0;
    assert!(numeric_shape(&input, 8192, 8, false).is_err());
    input.input_ids = vec![1; 512];
    input.attention_mask = vec![0; 512];
    input.attention_mask[32..].fill(1);
    input.position_ids = (0..512).collect();
    input.decision_position = 511;
    input.candidate_positions = vec![2152, 9693];
    input.option_mask = vec![true, true];
    numeric_shape(&input, 512, 2, true)?;
    input.attention_mask[100] = 0;
    assert!(numeric_shape(&input, 512, 2, true).is_err());
    Ok(())
}
