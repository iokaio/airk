// SPDX-License-Identifier: Apache-2.0
//! Additive current-policy primitive evidence; never model/release authority alone.
use super::*;
use airk_core::arbiter::resource_policy::{
    ResourceFilePin, ResourcePolicy, ResourceRunInput, ResourceSeries,
};
use airk_retrieval::arbiter::compiler::CompiledInput;
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub(super) struct Evidence<'a> {
    pub kind: Gate,
    pub profile: &'a Profile,
    pub candidate: &'a Candidate,
    pub expected: &'a Binding,
    pub records: &'a [Original],
    pub policy: &'a ResourcePolicy,
    pub targets: &'a BTreeMap<Sha256Digest, String>,
    pub source_groups: &'a BTreeMap<Sha256Digest, BTreeSet<Sha256Digest>>,
    pub build_receipts: &'a BTreeMap<Sha256Digest, BTreeSet<Sha256Digest>>,
    pub decision: &'a RecordedDecision,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Execution {
    schema: String,
    gate: Gate,
    binding: Binding,
    policy_digest: Sha256Digest,
    input: PinnedFile,
    capture: PinnedFile,
    owner: PinnedFile,
    native_binary: PinnedFile,
}
pub(super) fn policy(
    profile: &Profile,
    proof: &FinalProof,
    decision: &RecordedDecision,
) -> Result<Option<ResourcePolicy>> {
    let Some(pin) = &proof.resource_policy else {
        ensure!(
            profile.max_tokens
                == if profile.declared_stage() == Some(Stage::Focus) {
                    512
                } else {
                    2048
                },
            "historical proof cannot establish wider Context qualification"
        );
        return Ok(None);
    };
    ensure!(
        decision.approved_resource_policies.contains(&pin.sha256),
        "software-v2 policy lacks exact recorded approval"
    );
    let policy: ResourcePolicy = serde_json::from_slice(&pinned(pin)?)?;
    policy.validate()?;
    let expected = if profile.declared_stage() == Some(Stage::Focus) {
        policy.focus_pair_input_tokens
    } else {
        policy.context_input_tokens
    };
    ensure!(
        profile.max_tokens == expected,
        "candidate width differs from approved software-v2 policy"
    );
    Ok(Some(policy))
}
fn core_pin(pin: &ResourceFilePin) -> Result<Vec<u8>> {
    pin.validate()?;
    pinned(&PinnedFile {
        path: PathBuf::from(&pin.path),
        bytes: fs::symlink_metadata(&pin.path)?.len(),
        sha256: pin.sha256,
    })
}
fn original(records: &[Original], digest: Sha256Digest) -> Result<&Value> {
    let matches = records
        .iter()
        .filter(|(actual, _)| *actual == digest)
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "exact original software-v2 artifact absent/duplicate"
    );
    Ok(&matches[0].1)
}
fn identity(
    capture: &Value,
    profile: &Profile,
    expected: &Binding,
    policy: &ResourcePolicy,
) -> Result<()> {
    actual_identity(capture, profile, expected)?;
    ensure!(
        capture["complete"] == true
            && capture["evaluation"] == true
            && capture["release_qualified"] == false
            && capture["model"] == json!(profile.model)
            && capture["target"] == expected.target
            && capture["cpu_fit"] == serde_json::to_value(&profile.cpu_fit)?
            && capture["resource_policy"] == serde_json::to_value(policy)?
            && capture["resource_policy_digest"] == json!(policy.digest()?),
        "actual /2 profile/target/policy observations differ"
    );
    let architecture = if expected.target.starts_with("x86_64-") {
        "x86_64"
    } else if expected.target.starts_with("aarch64-") {
        "aarch64"
    } else {
        anyhow::bail!("unsupported actual native target")
    };
    ensure!(
        capture["isa"]["architecture"] == architecture,
        "actual ISA architecture differs"
    );
    let fields: &[&str] = if architecture == "x86_64" {
        &["avx", "avx2", "fma"]
    } else {
        &["neon"]
    };
    ensure!(
        fields
            .iter()
            .all(|field| capture["isa"][*field].is_boolean()),
        "actual ISA observations missing"
    );
    ensure!(
        capture["process_id"].as_u64().is_some_and(|pid| pid > 0)
            && capture["execution_user"]
                .as_str()
                .is_some_and(|user| !user.is_empty()),
        "native process/user identity missing"
    );
    Ok(())
}
fn compiled_shape(input: &CompiledInput, profile: &Profile, policy: &ResourcePolicy) -> Result<()> {
    let focus = profile.declared_stage() == Some(Stage::Focus);
    let (tokens, options) = if focus {
        (policy.focus_pair_input_tokens, 2)
    } else {
        (
            policy.context_input_tokens,
            policy.maximum_context_candidates,
        )
    };
    ensure!(
        input.input_ids.len() == tokens
            && input.attention_mask.len() == tokens
            && input.position_ids.len() == tokens
            && input.candidate_positions.len() == options
            && input
                .candidate_positions
                .iter()
                .all(|position| *position >= 0)
            && input.option_mask == vec![true; options]
            && input.decision_position >= 0
            && usize::try_from(input.decision_position).is_ok_and(|at| at < tokens)
            && input
                .attention_mask
                .iter()
                .all(|value| matches!(value, 0 | 1)),
        "actual full native resource shape differs"
    );
    if focus {
        ensure!(
            input.candidate_positions == [2152, 9693]
                && input
                    .attention_mask
                    .iter()
                    .filter(|value| **value == 1)
                    .count()
                    >= 480,
            "actual Focus near-cap static512 pair absent"
        );
    } else {
        ensure!(
            input.attention_mask == vec![1; tokens],
            "actual Context eight-live full input absent"
        );
    }
    Ok(())
}
fn resource(capture: &Value, profile: &Profile, policy: &ResourcePolicy) -> Result<()> {
    ensure!(
        capture["schema"] == "airk.arbiter.native-resource/2"
            && capture["candidate_resource_passed"] == true
            && capture["numeric_status"] == "unavailable_in_resource_only_run",
        "complete resource-only /2 observation required"
    );
    let input: CompiledInput = serde_json::from_value(capture["input"].clone())?;
    compiled_shape(&input, profile, policy)?;
    let series: ResourceSeries = serde_json::from_value(capture["resource"].clone())?;
    series.require_complete(policy)?;
    ensure!(
        series.reservation_bytes == profile.memory_reservation_bytes,
        "actual resource reservation differs from candidate"
    );
    ensure!(
        capture["load_us"].as_u64().is_some(),
        "actual load timing unavailable"
    );
    let load = capture["load_peak_process_rss_bytes"]
        .as_u64()
        .context("actual load peak unavailable")?;
    let warm = capture["warmup"]["peak_process_rss_bytes"]
        .as_u64()
        .context("actual warmup peak unavailable")?;
    ensure!(
        load > 0
            && load <= series.reservation_bytes
            && warm >= load
            && warm <= series.reservation_bytes
            && capture["warmup"]["returned"] == true
            && capture["warmup"]["success"] == true
            && capture["warmup"]["elapsed_us"]
                .as_u64()
                .is_some_and(|elapsed| elapsed <= policy.p95_limit_us),
        "actual load/warmup exceeds or omits frozen resources"
    );
    let mut prior = warm;
    for sample in &series.samples {
        ensure!(
            sample.peak_process_rss_bytes >= prior,
            "process-lifetime native peak disappeared between observations"
        );
        prior = sample.peak_process_rss_bytes;
    }
    Ok(())
}
fn source_identity(
    manifest: &Value,
    build: &Value,
    groups: &BTreeSet<Sha256Digest>,
    build_receipt: Sha256Digest,
    receipts: &BTreeSet<Sha256Digest>,
) -> Result<Sha256Digest> {
    ensure!(
        receipts.contains(&build_receipt),
        "source manifest native build receipt was not reviewed for this actual image"
    );
    let group: Sha256Digest = serde_json::from_value(build["groups"]["native"]["sha256"].clone())?;
    ensure!(
        groups.contains(&group),
        "source manifest is not a reviewed actual-image native source group"
    );
    build_target::build(build, &json!(group))?;
    let root = Path::new(
        manifest["repository_root"]
            .as_str()
            .context("original source repository root absent")?,
    );
    ensure!(root.is_absolute(), "absolute original source root required");
    let files = build["groups"]["native"]["files"]
        .as_object()
        .context("actual native build source seals absent")?;
    let pins: Vec<ResourceFilePin> = serde_json::from_value(manifest["source_files"].clone())?;
    ensure!(!pins.is_empty(), "source manifest omitted native inputs");
    let mut names = BTreeSet::new();
    for pin in pins {
        pin.validate()?;
        ensure!(
            names.insert(pin.path.clone()),
            "duplicate declared source seal"
        );
        let name = Path::new(&pin.path)
            .strip_prefix(root)
            .context("declared source outside original repository")?
            .to_str()
            .context("native source key UTF-8")?
            .replace('\\', "/");
        ensure!(
            files.get(&name) == Some(&json!(pin.sha256)),
            "declared source file differs from actual reviewed native build input"
        );
    }
    Ok(group)
}
fn seal(owner: &Value, path: &Path, digest: Sha256Digest) -> Result<()> {
    let name = path.to_str().context("actual owned input path UTF-8")?;
    ensure!(
        owner["input_seals"]
            .as_object()
            .and_then(|values| values.get(name))
            == Some(&json!(digest)),
        "actual owned phase omitted or changed an input seal"
    );
    Ok(())
}
fn source_closure(
    context: &Evidence<'_>,
    execution: &Execution,
    owner: &Value,
    image: Sha256Digest,
) -> Result<()> {
    let input: ResourceRunInput = serde_json::from_slice(&pinned(&execution.input)?)?;
    let manifest: Value = serde_json::from_slice(&core_pin(&input.source_manifest)?)?;
    let build_pin: ResourceFilePin =
        serde_json::from_value(manifest["native_build_receipt"].clone())?;
    let build: Value = serde_json::from_slice(&core_pin(&build_pin)?)?;
    let groups = context
        .source_groups
        .get(&image)
        .context("reviewed image native source group absent")?;
    let receipts = context
        .build_receipts
        .get(&image)
        .context("actual image reviewed build-receipt pins absent")?;
    source_identity(&manifest, &build, groups, build_pin.sha256, receipts)?;
    let seals = owner["input_seals"]
        .as_object()
        .context("actual owned input seals absent")?;
    verify_named_seals(seals)?;
    seal(owner, &execution.input.path, execution.input.sha256)?;
    seal(owner, Path::new(&input.profile.path), input.profile.sha256)?;
    seal(
        owner,
        Path::new(&input.source_manifest.path),
        input.source_manifest.sha256,
    )?;
    seal(owner, Path::new(&build_pin.path), build_pin.sha256)?;
    seal(owner, &execution.native_binary.path, image)?;
    let declared: Vec<ResourceFilePin> = serde_json::from_value(manifest["source_files"].clone())?;
    for pin in declared {
        seal(owner, Path::new(&pin.path), pin.sha256)?;
    }
    for asset in &context.profile.assets {
        let path = input
            .files
            .get(&asset.name)
            .context("actual sealed logical asset absent")?;
        seal(owner, Path::new(path), asset.sha256)?;
    }
    if let Some(fixture) = &input.numeric_fixture {
        seal(owner, Path::new(&fixture.path), fixture.sha256)?;
        let value: Value = serde_json::from_slice(&core_pin(fixture)?)?;
        let producer: ResourceFilePin =
            serde_json::from_value(value["reference_producer"].clone())?;
        seal(owner, Path::new(&producer.path), producer.sha256)?;
    }
    Ok(())
}

fn owned_execution(
    context: &Evidence<'_>,
    capture_sha: Sha256Digest,
    capture: &Value,
    input_sha: Sha256Digest,
) -> Result<()> {
    let Evidence {
        kind,
        profile,
        expected,
        records,
        policy,
        targets,
        source_groups: _,
        build_receipts: _,
        decision,
        ..
    } = *context;
    let (projection_sha, projection) =
        original_schema(records, "airk.private.software-v2-native-execution/1")?;
    ensure!(
        decision
            .approved_software_v2_executions
            .contains(projection_sha),
        "software-v2 execution source lacks exact recorded approval"
    );
    let execution: Execution = serde_json::from_value(projection.clone())?;
    ensure!(
        execution.schema == "airk.private.software-v2-native-execution/1"
            && execution.gate == kind
            && execution.binding == *expected
            && execution.policy_digest == policy.digest()?
            && execution.input.sha256 == input_sha
            && execution.capture.sha256 == capture_sha,
        "actual software-v2 execution scope/input/capture differs"
    );
    ensure!(
        serde_json::from_slice::<Value>(&pinned(&execution.capture)?)? == *capture
            && serde_json::from_slice::<Value>(&pinned(&execution.input)?)?
                == *original(records, input_sha)?,
        "execution original file bytes differ"
    );
    let owner: Value = serde_json::from_slice(&pinned(&execution.owner)?)?;
    ensure!(
        owner["schema"] == "airk.private.software-v2-owned-execution/1"
            && owner["capture_sha256"] == json!(capture_sha)
            && owner["input_sha256"] == json!(input_sha)
            && owner["resource_policy_digest"] == json!(policy.digest()?)
            && owner["process_id"] == capture["process_id"]
            && owner["execution_user"] == capture["execution_user"],
        "actual original owned execution binding differs"
    );
    settled_owner(&owner)?;
    let image: Sha256Digest =
        serde_json::from_value(owner["owned"]["suspended_assignment"]["image"]["sha256"].clone())?;
    ensure!(
        image == execution.native_binary.sha256
            && capture["native_binary_sha256"] == json!(image)
            && targets.get(&image) == Some(&expected.target),
        "actual /2 image lacks reviewed native target provenance"
    );
    ensure!(
        owner["owned"]["suspended_assignment"]["pid"] == capture["process_id"],
        "captured PID differs from actual assigned child"
    );
    ensure!(
        execution.native_binary.bytes > 0
            && execution.native_binary.bytes <= 4u64 << 30
            && fs::symlink_metadata(&execution.native_binary.path)?.len()
                == execution.native_binary.bytes,
        "native preserved image length differs"
    );
    verify_named_seals(&serde_json::Map::from_iter([(
        execution
            .native_binary
            .path
            .to_str()
            .context("native image path")?
            .into(),
        json!(image),
    )]))?;
    source_closure(context, &execution, &owner, image)?;
    let selector = match kind {
        Gate::NativeResource => {
            "inference_engine::pool::arbiter::qualification::software_v2::software_v2_selected_native_resources"
        }
        Gate::NativeNumerical => {
            "inference_engine::pool::arbiter::qualification::software_v2::software_v2_selected_native_numeric"
        }
        _ => anyhow::bail!("not a primitive software-v2 gate"),
    };
    let command = owner["command"]
        .as_array()
        .context("original native command absent")?;
    ensure!(
        command
            .iter()
            .filter(|arg| arg.as_str() == Some(selector))
            .count()
            == 1
            && command.iter().any(|arg| arg == "--exact")
            && command.iter().any(|arg| arg == "--ignored"),
        "actual exact native selector absent/ambiguous"
    );
    let stdout: PinnedFile = serde_json::from_value(owner["stdout"].clone())?;
    let stdout = String::from_utf8(pinned(&stdout)?)?;
    ensure!(
        stdout.contains("running 1 test")
            && stdout.contains(selector)
            && stdout.contains("test result: ok. 1 passed; 0 failed;"),
        "actual native one-test success absent"
    );
    ensure!(
        owner["owned"]["peak_host_committed_bytes"]
            .as_u64()
            .is_some_and(|peak| peak > 0 && peak <= profile.memory_reservation_bytes),
        "actual owned committed-memory limit absent/exceeded"
    );
    Ok(())
}
fn mass(values: &[f32]) -> Result<Vec<f64>> {
    ensure!(
        !values.is_empty() && values.iter().all(|value| value.is_finite()),
        "finite numeric vectors required"
    );
    let peak = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let values = values
        .iter()
        .map(|value| (f64::from(*value) - f64::from(peak)).exp())
        .collect::<Vec<_>>();
    let total: f64 = values.iter().sum();
    Ok(values.into_iter().map(|value| value / total).collect())
}
fn comparison(actual: &[f32], reference: &[f32]) -> Result<(f64, f64, bool)> {
    ensure!(
        actual.len() == reference.len(),
        "independent numeric cardinality differs"
    );
    let actual_mass = mass(actual)?;
    let reference_mass = mass(reference)?;
    Ok((
        actual
            .iter()
            .zip(reference)
            .map(|(left, right)| (f64::from(*left) - f64::from(*right)).abs())
            .fold(0f64, f64::max),
        actual_mass
            .iter()
            .zip(reference_mass)
            .map(|(left, right)| (left - right).abs())
            .fold(0f64, f64::max),
        winner(actual) == winner(reference),
    ))
}
fn numeric_shape(input: &CompiledInput, tokens: usize, options: usize, focus: bool) -> Result<()> {
    ensure!(
        input.input_ids.len() == tokens
            && input.attention_mask.len() == tokens
            && input.position_ids.len() == tokens
            && input.input_ids.iter().all(|id| *id >= 0)
            && input.position_ids == (0..tokens).map(|index| index as i64).collect::<Vec<_>>()
            && input.decision_position
                == i64::try_from(tokens)?
                    .checked_sub(1)
                    .context("nonempty numeric input required")?
            && input.candidate_positions.len() == options
            && input
                .candidate_positions
                .iter()
                .all(|position| *position >= 0)
            && input.option_mask == vec![true; options],
        "full actual numeric tensor shape differs"
    );
    if focus {
        ensure!(
            input.candidate_positions == [2152, 9693]
                && input
                    .attention_mask
                    .iter()
                    .all(|mask| matches!(mask, 0 | 1))
                && !input.attention_mask.windows(2).any(|pair| pair == [1, 0])
                && input.attention_mask.contains(&1),
            "genuine Focus left padding required"
        );
    } else {
        ensure!(
            input.attention_mask == vec![1; tokens],
            "Context numeric qualification requires every token live"
        );
    }
    Ok(())
}

fn numeric(
    capture: &Value,
    input: &ResourceRunInput,
    profile: &Profile,
    policy: &ResourcePolicy,
    decision: &RecordedDecision,
) -> Result<()> {
    ensure!(
        capture["schema"] == "airk.arbiter.native-numeric-result/2",
        "current native numeric result required"
    );
    let fixture_pin = input
        .numeric_fixture
        .as_ref()
        .context("current independent numeric fixture absent")?;
    let fixture: Value = serde_json::from_slice(&core_pin(fixture_pin)?)?;
    ensure!(
        capture["fixture"] == serde_json::to_value(fixture_pin)?
            && fixture["schema"] == "airk.arbiter.native-numeric/2"
            && fixture["model"] == json!(profile.model)
            && fixture["compiler"] == json!(profile.compiler)
            && fixture["resource_policy"] == json!(policy.digest()?),
        "actual independent fixture/model/compiler/policy differs"
    );
    let producer: ResourceFilePin = serde_json::from_value(fixture["reference_producer"].clone())?;
    ensure!(
        capture["reference_producer"] == serde_json::to_value(&producer)?
            && decision
                .approved_software_v2_reference_producers
                .contains(&producer.sha256),
        "original independent reference producer is absent/unapproved"
    );
    let producer: Value = serde_json::from_slice(&core_pin(&producer)?)?;
    ensure!(
        producer["schema"] == "airk.arbiter.independent-native-numeric-reference/2"
            && producer["model"] == json!(profile.model)
            && producer["compiler"] == json!(profile.compiler)
            && producer["resource_policy_digest"] == json!(policy.digest()?)
            && producer["rows"] == fixture["rows"]
            && producer["original_reference_assets"] == fixture["original_reference_assets"]
            && capture["original_reference_assets"] == fixture["original_reference_assets"],
        "independent original input/reference closure differs"
    );
    let candidate_assets = profile
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
        producer["candidate_assets"] == serde_json::to_value(&candidate_assets)?,
        "same-artifact reference does not bind complete actual candidate neural assets"
    );
    let originals: Vec<Asset> =
        serde_json::from_value(producer["original_reference_assets"].clone())?;
    let sources: BTreeMap<String, PinnedFile> =
        serde_json::from_value(producer["original_reference_files"].clone())?;
    ensure!(
        !originals.is_empty() && sources.len() == originals.len(),
        "complete original reference asset/files required"
    );
    let mut reference_names = BTreeSet::new();
    ensure!(
        originals
            .iter()
            .any(|asset| matches!(asset.role, AssetRole::Model | AssetRole::ExternalWeights)),
        "original neural reference assets missing"
    );
    for asset in originals {
        ensure!(
            reference_names.insert(asset.name.clone()),
            "duplicate original reference asset name"
        );
        let file = sources
            .get(&asset.name)
            .context("original reference asset file missing")?;
        ensure!(
            file.bytes == asset.bytes
                && file.sha256 == asset.sha256
                && fs::symlink_metadata(&file.path)?.len() == asset.bytes,
            "original reference asset identity differs"
        );
        verify_named_seals(&serde_json::Map::from_iter([(
            file.path
                .to_str()
                .context("original reference path")?
                .into(),
            json!(asset.sha256),
        )]))?;
    }
    let runs: Vec<PinnedFile> = serde_json::from_value(producer["reference_executions"].clone())?;
    ensure!(
        !runs.is_empty() && runs.len() <= 32,
        "actual independent reference executions absent"
    );
    let mut reference_kinds = BTreeSet::new();
    for pin in runs {
        let run: Value = serde_json::from_slice(&pinned(&pin)?)?;
        ensure!(
            run["schema"] == "airk.arbiter.independent-numeric-reference-execution/1"
                && run["model"] == json!(profile.model)
                && run["compiler"] == json!(profile.compiler)
                && run["resource_policy_digest"] == json!(policy.digest()?)
                && run["native_airk_nn_invocations"] == 0,
            "reference execution is not independent of AIRK native scoring"
        );
        settled_owner(&run)?;
        let seals = run["input_seals"]
            .as_object()
            .context("actual independent reference input seals absent")?;
        verify_named_seals(seals)?;
        let kind = run["reference_kind"]
            .as_str()
            .context("independent reference kind absent")?;
        ensure!(
            matches!(kind, "same_artifact" | "original_fp32")
                && reference_kinds.insert(kind.to_owned()),
            "duplicate/foreign reference execution"
        );
        let expected_assets = if kind == "same_artifact" {
            &producer["candidate_assets"]
        } else {
            &producer["original_reference_assets"]
        };
        ensure!(
            run["source_assets"] == *expected_assets,
            "reference execution changed actual neural source roster"
        );
        if kind == "same_artifact" {
            for asset in &candidate_assets {
                let path = input
                    .files
                    .get(&asset.name)
                    .context("same-artifact reference file absent")?;
                ensure!(
                    seals.get(path) == Some(&json!(asset.sha256)),
                    "same-artifact reference did not seal actual candidate file"
                );
            }
        } else {
            for pin in sources.values() {
                ensure!(
                    seals.get(pin.path.to_str().context("reference seal path")?)
                        == Some(&json!(pin.sha256)),
                    "original reference execution did not seal actual original file"
                );
            }
        }
        let actual_rows = run["rows"]
            .as_array()
            .context("actual independent reference rows absent")?;
        let fixtures = fixture["rows"]
            .as_array()
            .context("native numeric fixture rows absent")?;
        ensure!(
            actual_rows.len() == fixtures.len(),
            "reference execution omitted numeric inputs"
        );
        for (observed, expected) in actual_rows.iter().zip(fixtures) {
            ensure!(
                observed["name"] == expected["name"]
                    && observed["input"] == expected["input"]
                    && observed["scores"]
                        == expected[if kind == "same_artifact" {
                            "reference_same_artifact"
                        } else {
                            "reference_original_fp32"
                        }],
                "original reference execution arrays differ"
            );
        }
    }
    ensure!(
        reference_kinds == BTreeSet::from(["same_artifact".to_owned(), "original_fp32".to_owned()]),
        "both independent reference kinds required"
    );
    let compiler_asset = profile
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Compiler)
        .context("actual numeric compiler asset absent")?;
    let compiler_file = input
        .files
        .get(&compiler_asset.name)
        .context("actual numeric compiler file absent")?;
    let compiler: CompilerProfile = serde_json::from_slice(&core_pin(&ResourceFilePin {
        path: compiler_file.clone(),
        sha256: compiler_asset.sha256,
    })?)?;
    compiler.validate()?;
    ensure!(
        compiler.max_tokens == profile.max_tokens,
        "numeric compiler width differs from candidate"
    );
    let fixtures = fixture["rows"].as_array().context("numeric rows absent")?;
    let observed = capture["rows"]
        .as_array()
        .context("actual numeric observations absent")?;
    let focus = profile.declared_stage() == Some(Stage::Focus);
    ensure!(
        fixtures.len() == if focus { 12 } else { 3 } && observed.len() == fixtures.len(),
        "partial numeric population"
    );
    let mut names = BTreeSet::new();
    for (index, (case, row)) in fixtures.iter().zip(observed).enumerate() {
        let name = case["name"].as_str().context("numeric case name absent")?;
        ensure!(
            !name.is_empty() && names.insert(name),
            "duplicate/empty numeric identity"
        );
        let compiled: CompiledInput = serde_json::from_value(case["input"].clone())?;
        let (tokens, options) = if focus {
            (512, 2)
        } else {
            [(17, 3), (128, 8), (8192, 8)][index]
        };
        numeric_shape(&compiled, tokens, options, focus)?;
        ensure!(
            compiled.candidate_positions
                == compiler
                    .symbols
                    .iter()
                    .take(options)
                    .map(|symbol| i64::from(symbol.id))
                    .collect::<Vec<_>>(),
            "numeric readout uses different vocabulary rows than actual native compiler"
        );
        ensure!(
            row["name"] == name
                && row["tokens"] == tokens
                && row["options"] == options
                && row["input_digest"] == json!(compiled.input_digest),
            "actual /2 numeric identity changed"
        );
        let actual = numeric_values(&row["raw_scores"], options)?;
        let same = numeric_values(&case["reference_same_artifact"], options)?;
        let original = numeric_values(&case["reference_original_fp32"], options)?;
        let (same_raw, same_mass, same_winner) = comparison(&actual, &same)?;
        let (original_raw, original_mass, original_winner) = comparison(&actual, &original)?;
        ensure!(
            same_raw <= 0.001
                && same_mass <= if focus { 0.00001 } else { 0.05 }
                && original_raw <= if focus { 0.25 } else { 0.001 }
                && original_mass <= 0.05
                && same_winner
                && original_winner
                && row["argmax_matches"] == true,
            "recomputed original /2 logits/mass/winner refused"
        );
        for (field, value) in [
            ("same_artifact_raw_error", same_raw),
            ("same_artifact_mass_error", same_mass),
            ("original_fp32_raw_error", original_raw),
            ("original_fp32_mass_error", original_mass),
        ] {
            ensure!(
                row[field].as_f64().is_some_and(
                    |reported| reported.is_finite() && (reported - value).abs() <= 1e-12
                ),
                "reported native numeric error differs from actual arrays"
            );
        }
    }
    Ok(())
}
pub(super) fn validate(context: &Evidence<'_>) -> Result<()> {
    let Evidence {
        kind,
        profile,
        candidate,
        expected,
        records,
        policy,
        targets: _,
        source_groups: _,
        build_receipts: _,
        decision,
    } = *context;
    let schema = match kind {
        Gate::NativeResource => "airk.arbiter.native-resource/2",
        Gate::NativeNumerical => "airk.arbiter.native-numeric-result/2",
        _ => anyhow::bail!("not a software-v2 primitive gate"),
    };
    let (capture_sha, capture) = original_schema(records, schema)?;
    identity(capture, profile, expected, policy)?;
    let (input_sha, value) = original_schema(records, "airk.arbiter.resource-run/2")?;
    let input: ResourceRunInput = serde_json::from_value(value.clone())?;
    input.validate()?;
    ensure!(
        input.profile.sha256 == expected.candidate_profile_sha256
            && input.policy == *policy
            && capture["input_sha256"] == json!(input_sha)
            && capture["source_manifest"] == serde_json::to_value(&input.source_manifest)?,
        "actual /2 input/source/profile binding differs"
    );
    let actual_profile: Profile = serde_json::from_slice(&core_pin(&input.profile)?)?;
    ensure!(
        serde_json::to_value(&actual_profile)? == serde_json::to_value(profile)?,
        "original /2 Profile bytes differ"
    );
    core_pin(&input.source_manifest)?;
    ensure!(
        input.files.len() == candidate.files.len()
            && input.files.iter().all(|(name, path)| candidate
                .files
                .get(name)
                .is_some_and(|actual| actual == Path::new(path))),
        "actual /2 asset file map differs"
    );
    owned_execution(context, *capture_sha, capture, *input_sha)?;
    match kind {
        Gate::NativeResource => resource(capture, profile, policy),
        Gate::NativeNumerical => numeric(capture, &input, profile, policy, decision),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) fn refusal_cases() -> Result<Vec<Value>> {
    tests::refusal_cases()
}
