// SPDX-License-Identifier: Apache-2.0
//! Pure projection of retained actual owner facts, never an approval or new model run.
use super::*;

fn project(
    raw: &Value,
    capture: &Value,
    capture_sha: Sha256Digest,
    input_sha: Sha256Digest,
    policy: &ResourcePolicy,
    reservation: u64,
) -> Result<Value> {
    ensure!(
        raw["schema"] == "airk.private.owned-software-v2-native-execution/1"
            && raw["measurement_passed"] == true
            && raw["committed_memory_passed"] == true
            && capture["complete"] == true
            && capture["process_id"].as_u64().is_some_and(|pid| pid > 0)
            && capture["execution_user"]
                .as_str()
                .is_some_and(|user| !user.is_empty())
            && serde_json::from_value::<Sha256Digest>(capture["native_binary_sha256"].clone())
                .is_ok()
            && capture["input_sha256"] == json!(input_sha)
            && capture["resource_policy_digest"] == json!(policy.digest()?)
            && capture["resource_policy"] == serde_json::to_value(policy)?
            && capture["process_id"] == raw["owned"]["suspended_assignment"]["pid"]
            && capture["native_binary_sha256"]
                == raw["owned"]["suspended_assignment"]["image"]["sha256"]
            && capture["execution_user"] == raw["execution_sid"],
        "original raw native owner/capture binding absent or refused"
    );
    let owner = json!({
        "schema":"airk.private.software-v2-owned-execution/1",
        "capture_sha256":capture_sha,"input_sha256":input_sha,
        "resource_policy_digest":policy.digest()?,
        "process_id":capture["process_id"],"execution_user":capture["execution_user"],
        "complete":raw["complete"],"failure":raw["failure"],
        "post_input_drift":raw["post_input_drift"],
        "whole_phase_deadline_not_refreshed":raw["whole_phase_deadline_not_refreshed"],
        "whole_phase_expired":raw["whole_phase_expired"],
        "no_automatic_retry":raw["no_automatic_retry"],
        "other_processes_touched":raw["other_processes_touched"],
        "owned":raw["owned"],"command":raw["command"],"stdout":raw["stdout"],
        "input_seals":raw["input_seals"],"operation_watch":raw["operation_watch"],
        "operation_journal":raw["operation_journal_pin"]
    });
    settled_owner(&owner)?;
    committed(&owner["owned"], reservation)?;
    Ok(owner)
}

fn typed_execution(
    profile: &Profile,
    input: &ResourceRunInput,
    input_pin: PinnedFile,
    capture: &Value,
    capture_pin: PinnedFile,
    owner: PinnedFile,
    native_binary: PinnedFile,
) -> Result<Execution> {
    let gate = match capture["schema"].as_str() {
        Some("airk.arbiter.native-resource/2") => Gate::NativeResource,
        Some("airk.arbiter.native-numeric-result/2") => Gate::NativeNumerical,
        _ => anyhow::bail!("retained native primitive schema absent/unknown"),
    };
    ensure!(
        capture["input_sha256"] == json!(input_pin.sha256)
            && capture["native_binary_sha256"] == json!(native_binary.sha256),
        "typed execution original input/image binding differs"
    );
    let target = capture["target"]
        .as_str()
        .context("actual native capture target absent")?;
    ensure!(!target.is_empty(), "actual native capture target empty");
    Ok(Execution {
        schema: "airk.private.software-v2-native-execution/1".into(),
        gate,
        binding: binding(profile, input.profile.sha256, target)?,
        policy_digest: input.policy.digest()?,
        input: input_pin,
        capture: capture_pin,
        owner,
        native_binary,
    })
}
fn output_path(name: &str, raw: &Path) -> Result<PathBuf> {
    let output = PathBuf::from(std::env::var(name)?);
    ensure!(
        output.is_absolute()
            && fs::canonicalize(output.parent().context("projection output parent absent")?)?
                == fs::canonicalize(raw.parent().context("original raw parent absent")?)?,
        "projection outputs must be in the original protected raw-run directory"
    );
    Ok(output)
}
fn retain_output(path: &Path, bytes: &[u8]) -> Result<PinnedFile> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.sync_all()?;
    let pin = PinnedFile {
        path: path.into(),
        bytes: bytes.len() as u64,
        sha256: sha(bytes),
    };
    pinned(&pin)?;
    Ok(pin)
}

#[test]
#[ignore = "explicit retained actual raw owner projection; zero model execution"]
fn emit_retained_software_v2_owned_execution() -> Result<()> {
    let path = PathBuf::from(std::env::var("AIRK_SOFTWARE_V2_RAW_OWNER")?);
    let sha256: Sha256Digest =
        serde_json::from_value(json!(std::env::var("AIRK_SOFTWARE_V2_RAW_OWNER_SHA256")?))?;
    let raw_pin = PinnedFile {
        bytes: fs::symlink_metadata(&path)?.len(),
        path,
        sha256,
    };
    let raw: Value = serde_json::from_slice(&pinned(&raw_pin)?)?;
    let capture_pin: PinnedFile = serde_json::from_value(raw["native_body_pin"].clone())?;
    let capture: Value = serde_json::from_slice(&pinned(&capture_pin)?)?;
    ensure!(
        capture == raw["native_body"],
        "retained original capture differs from raw owner body"
    );
    let plan: PinnedFile = {
        let path = PathBuf::from(
            raw["plan"]["path"]
                .as_str()
                .context("actual plan path absent")?,
        );
        PinnedFile {
            bytes: fs::symlink_metadata(&path)?.len(),
            path,
            sha256: serde_json::from_value(raw["plan"]["sha256"].clone())?,
        }
    };
    let plan: Value = serde_json::from_slice(&pinned(&plan)?)?;
    let input_pin: ResourceFilePin = serde_json::from_value(plan["native_input"].clone())?;
    let input: ResourceRunInput = serde_json::from_slice(&core_pin(&input_pin)?)?;
    input.validate()?;
    let profile: Profile = serde_json::from_slice(&core_pin(&input.profile)?)?;
    let mut owner = project(
        &raw,
        &capture,
        capture_pin.sha256,
        input_pin.sha256,
        &input.policy,
        profile.memory_reservation_bytes,
    )?;
    verify_named_seals(
        owner["input_seals"]
            .as_object()
            .context("actual input seals absent")?,
    )?;
    seal(&owner, Path::new(&input_pin.path), input_pin.sha256)?;
    let stdout: PinnedFile = serde_json::from_value(owner["stdout"].clone())?;
    pinned(&stdout)?;
    let journal: PinnedFile = serde_json::from_value(owner["operation_journal"].clone())?;
    ensure!(!pinned(&journal)?.is_empty(), "retained journal empty");
    owner["original_raw_execution"] = serde_json::to_value(&raw_pin)?;
    // Full source/target/selector/input closure and RecordedDecision approval stay
    // with owned_execution/source_closure. Neither projection grants authority.
    let owner_output = output_path("AIRK_SOFTWARE_V2_CANONICAL_OWNER_OUTPUT", &raw_pin.path)?;
    let execution_output =
        output_path("AIRK_SOFTWARE_V2_CANONICAL_EXECUTION_OUTPUT", &raw_pin.path)?;
    ensure!(
        owner_output != execution_output,
        "distinct canonical output files required"
    );
    let native_binary = PinnedFile {
        path: PathBuf::from(
            raw["native_image"]["path"]
                .as_str()
                .context("preserved native image path absent")?,
        ),
        bytes: raw["native_image"]["bytes"]
            .as_u64()
            .context("preserved native image length absent")?,
        sha256: serde_json::from_value(raw["native_image"]["sha256"].clone())?,
    };
    seal(&owner, &native_binary.path, native_binary.sha256)?;
    let owner_bytes = serde_json::to_vec_pretty(&owner)?;
    let owner_pin = PinnedFile {
        path: owner_output.clone(),
        bytes: owner_bytes.len() as u64,
        sha256: sha(&owner_bytes),
    };
    let input_pin = PinnedFile {
        path: PathBuf::from(&input_pin.path),
        bytes: fs::symlink_metadata(&input_pin.path)?.len(),
        sha256: input_pin.sha256,
    };
    let execution = typed_execution(
        &profile,
        &input,
        input_pin,
        &capture,
        capture_pin,
        owner_pin,
        native_binary,
    )?;
    // Validate both typed projections before creating either immutable output.
    let execution_bytes = serde_json::to_vec_pretty(&execution)?;
    retain_output(&owner_output, &owner_bytes)?;
    retain_output(&execution_output, &execution_bytes)?;
    Ok(())
}

#[test]
fn owned_projection_keeps_committed_reservation_strict_independently_of_rss() -> Result<()> {
    committed(&json!({"peak_host_committed_bytes":1000}), 1000)?;
    for owned in [
        json!({}),
        json!({"peak_host_committed_bytes":0}),
        json!({"peak_host_committed_bytes":1001}),
        json!({"peak_host_committed_bytes":-1}),
    ] {
        assert!(committed(&owned, 1000).is_err());
    }
    Ok(())
}

#[test]
fn owned_projection_refuses_missing_actual_owner_and_failed_measurement() -> Result<()> {
    let policy = ResourcePolicy::software_v2();
    let digest = Sha256Digest::from_bytes([7; 32]);
    for raw in [
        json!({}),
        json!({"schema":"airk.private.owned-software-v2-native-execution/1",
        "measurement_passed":false,"committed_memory_passed":true}),
    ] {
        assert!(project(&raw, &json!({}), digest, digest, &policy, 1000).is_err());
    }
    Ok(())
}

#[test]
fn owned_projection_preserves_actual_facts_and_refuses_proof_mutations() -> Result<()> {
    let policy = ResourcePolicy::software_v2();
    let image = Sha256Digest::from_bytes([8; 32]);
    let input = Sha256Digest::from_bytes([7; 32]);
    let capture_sha = Sha256Digest::from_bytes([9; 32]);
    let capture = json!({"complete":true,"input_sha256":input,"resource_policy_digest":policy.digest()?,
        "resource_policy":policy,"process_id":77,"execution_user":"SYNTHETIC-USER",
        "native_binary_sha256":image});
    // Literal synthetic proof population, never real process/qualification evidence.
    let raw = json!({"schema":"airk.private.owned-software-v2-native-execution/1",
    "complete":true,"measurement_passed":true,"committed_memory_passed":true,
    "failure":null,"post_input_drift":[],"whole_phase_deadline_not_refreshed":true,
    "whole_phase_expired":false,"no_automatic_retry":true,"other_processes_touched":false,
    "execution_sid":"SYNTHETIC-USER","owned":{
        "exit_code":0,"timed_out":false,"log_oversized":false,
        "owned_descendants_settled":true,"owned_job_forced_termination":false,
        "owned_job_final_active_processes":0,"owned_job_final_member_pids":[],
        "parent_assigned_to_job":false,"child_assigned_before_resume":true,
        "owned_total_processes":1,"unexpected_third_process":false,
        "whole_process_deadline_not_refreshed":true,"peak_host_committed_bytes":1000,
        "retained_member_exits":[{"exit_observed":true,"exit_code":0}],
        "handle_close_proof":{"forced":false,"settled":true,"final_active":0,
            "members_before":[],"final_member_pids":[]},
        "suspended_assignment":{"pid":77,"image":{"sha256":image}}
    }});
    let canonical = project(&raw, &capture, capture_sha, input, &policy, 1000)?;
    assert_eq!(
        canonical["schema"],
        "airk.private.software-v2-owned-execution/1"
    );
    assert_eq!(canonical["capture_sha256"], json!(capture_sha));
    assert_eq!(canonical["owned"], raw["owned"]);
    for (path, value) in [
        ("/whole_phase_expired", json!(true)),
        ("/whole_phase_deadline_not_refreshed", json!(false)),
        ("/no_automatic_retry", json!(false)),
        ("/other_processes_touched", json!(true)),
        ("/post_input_drift", json!(["changed"])),
        ("/failure", json!("native error")),
        ("/owned/owned_job_forced_termination", json!(true)),
        ("/owned/peak_host_committed_bytes", json!(1001)),
        ("/owned/suspended_assignment/pid", json!(78)),
        ("/owned/handle_close_proof/settled", json!(false)),
    ] {
        let mut changed = raw.clone();
        *changed
            .pointer_mut(path)
            .context("literal synthetic mutation absent")? = value;
        assert!(
            project(&changed, &capture, capture_sha, input, &policy, 1000).is_err(),
            "{path}"
        );
    }
    let mut changed = capture.clone();
    changed["resource_policy_digest"] = json!(image);
    assert!(project(&raw, &changed, capture_sha, input, &policy, 1000).is_err());
    Ok(())
}

#[test]
fn owned_projection_emits_existing_typed_execution_and_refuses_wrong_scope() -> Result<()> {
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    let digest = Sha256Digest::from_bytes([11; 32]);
    profile.model = ModelChoice::ContextQwenCompact;
    profile.stage = Some(Stage::Context);
    profile.cpu_fit = Some(CpuFit::default());
    profile.task_policies.insert(Task::ContextAction, digest);
    let pin = PinnedFile {
        path: PathBuf::from("SYNTHETIC-ONLY"),
        bytes: 1,
        sha256: digest,
    };
    let input = ResourceRunInput {
        schema: "airk.arbiter.resource-run/2".into(),
        profile: ResourceFilePin {
            path: "SYNTHETIC-ONLY".into(),
            sha256: digest,
        },
        source_manifest: ResourceFilePin {
            path: "SYNTHETIC-ONLY".into(),
            sha256: digest,
        },
        files: BTreeMap::new(),
        policy: ResourcePolicy::software_v2(),
        policy_digest: ResourcePolicy::software_v2().digest()?,
        numeric_fixture: None,
    };
    for (schema, gate) in [
        ("airk.arbiter.native-resource/2", Gate::NativeResource),
        (
            "airk.arbiter.native-numeric-result/2",
            Gate::NativeNumerical,
        ),
    ] {
        let capture = json!({"schema":schema,"input_sha256":digest,
            "native_binary_sha256":digest,"target":"x86_64-pc-windows-msvc"});
        let make = |capture: &Value| {
            typed_execution(
                &profile,
                &input,
                pin.clone(),
                capture,
                pin.clone(),
                pin.clone(),
                pin.clone(),
            )
        };
        let execution = make(&capture)?;
        let encoded = serde_json::to_value(&execution)?;
        let decoded: Execution = serde_json::from_value(encoded.clone())?;
        assert_eq!(decoded.gate, gate);
        assert_eq!(
            decoded.binding,
            binding(&profile, digest, "x86_64-pc-windows-msvc")?
        );
        assert_eq!(
            encoded["schema"],
            "airk.private.software-v2-native-execution/1"
        );
        assert_eq!(encoded["input"], serde_json::to_value(&pin)?);
        for (field, value) in [
            ("schema", json!("airk.arbiter.native-resource/1")),
            ("target", Value::Null),
            ("input_sha256", json!(Sha256Digest::from_bytes([12; 32]))),
            ("native_binary_sha256", Value::Null),
        ] {
            let mut changed = capture.clone();
            changed[field] = value;
            assert!(make(&changed).is_err(), "{field}");
        }
        let mut unknown = encoded;
        unknown["approved"] = json!(true);
        assert!(serde_json::from_value::<Execution>(unknown).is_err());
    }
    Ok(())
}
