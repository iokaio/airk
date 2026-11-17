// SPDX-License-Identifier: Apache-2.0
//! Additive native software-v2 observations; historical /1 owners stay unchanged.
use super::{peak_rss, private_control};
use airk_core::{
    arbiter::{Asset, AssetRole, ModelChoice, Profile, Stage, resource_policy::*},
    identity::{Sha256Digest, framed_digest},
};
use airk_retrieval::arbiter::{
    compiler::{Candidate, CompiledInput, Compiler},
    runtime::{AssetSource, ExecutionProfile, Scorer, current_target},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

struct Files(BTreeMap<String, String>);
impl AssetSource for Files {
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>> {
        let path = self
            .0
            .get(&asset.name)
            .context("unmapped software-v2 native asset")?;
        let bytes = small(Path::new(path), asset.bytes)?;
        ensure!(
            bytes.len() as u64 == asset.bytes && sha(&bytes) == asset.sha256,
            "consumed native resource asset bytes differ"
        );
        Ok(bytes)
    }
}
fn sha(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn small(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= maximum,
        "native resource file type/size differs"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "resource reparse point refused"
        );
    }
    let limit = maximum.checked_add(1).context("file bound overflow")?;
    let length = usize::try_from(metadata.len()).context("native resource length overflow")?;
    let mut bytes = Vec::new();
    // Match production PinnedAssets::read: reserve the actual authenticated
    // metadata length, not geometric growth and not the larger caller ceiling.
    bytes
        .try_reserve_exact(length)
        .context("native resource allocation unavailable")?;
    File::open(path)?.take(limit).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "native resource file grew beyond bound"
    );
    Ok(bytes)
}
fn pinned(pin: &ResourceFilePin, maximum: u64) -> Result<Vec<u8>> {
    pin.validate()?;
    let bytes = small(Path::new(&pin.path), maximum)?;
    ensure!(
        sha(&bytes) == pin.sha256,
        "pinned software-v2 metadata changed"
    );
    Ok(bytes)
}
fn file_hash(path: &Path) -> Result<Sha256Digest> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 4 << 20];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}
fn verify_files(files: &Files, profile: &Profile) -> Result<()> {
    for asset in &profile.assets {
        let path = Path::new(
            files
                .0
                .get(&asset.name)
                .context("native asset map incomplete")?,
        );
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == asset.bytes,
            "native resource asset type/length differs"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "native resource reparse asset refused"
            );
        }
        ensure!(
            file_hash(path)? == asset.sha256,
            "native resource asset hash differs"
        );
    }
    Ok(())
}
fn inputs() -> Result<(ResourceRunInput, Profile, Files, Sha256Digest)> {
    let path = PathBuf::from(std::env::var("AIRK_SOFTWARE_V2_NATIVE_INPUT")?);
    let expected: Sha256Digest = std::env::var("AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256")?.parse()?;
    inputs_from(&ResourceFilePin {
        path: path.to_str().context("native request path")?.into(),
        sha256: expected,
    })
}
fn inputs_from(pin: &ResourceFilePin) -> Result<(ResourceRunInput, Profile, Files, Sha256Digest)> {
    let raw = pinned(pin, 1 << 20)?;
    let expected = pin.sha256;
    let input: ResourceRunInput = serde_json::from_slice(&raw)?;
    input.validate()?;
    pinned(&input.source_manifest, 16 << 20)?;
    let raw_profile = pinned(&input.profile, 1 << 20)?;
    let profile: Profile = serde_json::from_slice(&raw_profile)?;
    profile.validate()?;
    let stage = profile
        .model
        .stage()
        .context("only selected native stage models")?;
    let (width, reservation) = match profile.model {
        ModelChoice::FocusQwenReranker06B => (512, 4u64 << 30),
        ModelChoice::ContextQwenCompact => (8192, 16u64 << 30),
        ModelChoice::ContextQwenLarge => (8192, 40u64 << 30),
        _ => anyhow::bail!("historical model cannot inherit software-v2 evidence"),
    };
    ensure!(
        profile.max_tokens == width
            && profile.memory_reservation_bytes == reservation
            && profile.stage == Some(stage)
            && profile.qualification.is_none()
            && profile.qualified_targets.is_empty()
            && profile.downloads.is_empty()
            && (stage == Stage::Focus || profile.max_candidates == 8),
        "resource profile is not the exact unqualified software-v2 candidate"
    );
    ensure!(
        input.files.keys().collect::<BTreeSet<_>>()
            == profile
                .assets
                .iter()
                .map(|asset| &asset.name)
                .collect::<BTreeSet<_>>(),
        "exact native resource asset roster required"
    );
    let files = Files(input.files.clone());
    Ok((input, profile, files, expected))
}
#[derive(Debug)]
struct ObservedNativeFailure {
    elapsed_us: u64,
    peak_process_rss_bytes: u64,
}
impl std::fmt::Display for ObservedNativeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native score returned a known failure after measured work")
    }
}
impl std::error::Error for ObservedNativeFailure {}
fn flush(file: &mut File, receipt: &Value) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(receipt)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.set_len(bytes.len() as u64)?;
    file.flush()?;
    file.sync_data()?;
    Ok(())
}
fn isa() -> Value {
    #[cfg(target_arch = "x86_64")]
    {
        json!({"architecture":"x86_64",
        "avx":std::arch::is_x86_feature_detected!("avx"),"avx2":std::arch::is_x86_feature_detected!("avx2"),
        "fma":std::arch::is_x86_feature_detected!("fma")})
    }
    #[cfg(target_arch = "aarch64")]
    {
        json!({"architecture":"aarch64",
        "neon":std::arch::is_aarch64_feature_detected!("neon")})
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        json!({"architecture":std::env::consts::ARCH})
    }
}
fn resource_input(
    profile: &Profile,
    files: &Files,
    mut compile: impl FnMut(&str, &str, &[Candidate]) -> Result<CompiledInput>,
) -> Result<CompiledInput> {
    if profile.model == ModelChoice::FocusQwenReranker06B {
        for asset in profile
            .assets
            .iter()
            .filter(|asset| asset.role == AssetRole::Fixture)
        {
            ensure!(
                asset.bytes <= 4 << 20,
                "bounded resource source fixture required"
            );
            let fixture: Value = serde_json::from_slice(&files.read_asset(asset)?)?;
            if let Some(row) = fixture
                .get("rows")
                .and_then(Value::as_array)
                .and_then(|rows| {
                    rows.iter()
                        .find(|row| row["id"] == "long-maintenance-synthetic")
                })
            {
                let option = Candidate {
                    id: row["id"]
                        .as_str()
                        .context("Focus resource source id")?
                        .into(),
                    text: row["original_document"]
                        .as_str()
                        .context("Focus resource source document")?
                        .into(),
                };
                let input = compile(
                    "",
                    row["original_query"]
                        .as_str()
                        .context("Focus resource source query")?,
                    &[option],
                )?;
                ensure!(
                    input.input_ids.len() == 512
                        && input.attention_mask.iter().sum::<i64>() >= 480
                        && input.candidate_positions == [2152, 9693],
                    "actual full static512 near-cap Focus source required"
                );
                return Ok(input);
            }
        }
        anyhow::bail!("authenticated near-cap Focus source fixture unavailable");
    }
    let options = (0..8)
        .map(|index| Candidate {
            id: format!("resource-option-{index}"),
            text: format!("Synthetic whole option {index}; no source or task-truth authority."),
        })
        .collect::<Vec<_>>();
    let (mut low, mut high) = (0usize, 16384usize);
    let mut maximum = None;
    while low <= high {
        let middle = low + (high - low) / 2;
        match compile(
            &" source".repeat(middle),
            "Select one complete synthetic option.",
            &options,
        ) {
            Ok(input) => {
                maximum = Some(input);
                low = middle + 1;
            }
            Err(_) => {
                if middle == 0 {
                    break;
                }
                high = middle - 1;
            }
        }
    }
    let input = maximum.context("native maximum-input compiler could not admit a case")?;
    ensure!(
        input.input_ids.len() == 8192
            && input.candidate_positions.len() == 8
            && input.option_mask == [true; 8]
            && input.attention_mask == vec![1; 8192],
        "exact native T8192/N8/all-live input required; no shortened substitution"
    );
    Ok(input)
}
fn observe_series(
    series: &mut ResourceSeries,
    policy: &ResourcePolicy,
    mut observe: impl FnMut() -> Result<ResourceSample>,
    mut retain: impl FnMut(&ResourceSeries) -> Result<()>,
) -> Result<()> {
    for _ in 0..policy.required_warm_samples {
        series.begin_sample(policy)?;
        match observe() {
            Ok(sample) => {
                let outcome = series.finish_sample(sample, policy);
                retain(series)?;
                outcome?;
            }
            Err(error) => {
                series.fail_native(policy)?;
                retain(series)?;
                return Err(error);
            }
        }
    }
    series.require_complete(policy)
}

#[test]
#[ignore = "actual selected assets, frozen software-v2 request and exclusive native resource window required"]
fn software_v2_selected_native_resources() -> Result<()> {
    let (input, profile, files, input_hash) = inputs()?;
    let output = PathBuf::from(std::env::var("AIRK_SOFTWARE_V2_NATIVE_OUTPUT")?);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let binary_sha256 = file_hash(&std::env::current_exe()?)?;
    let mut receipt = json!({"schema":"airk.arbiter.native-resource/2","complete":false,
        "evaluation":true,"release_qualified":false,"candidate_resource_passed":false,
        "input_sha256":input_hash,"source_manifest":input.source_manifest,
        "profile":profile.manifest,"profile_source_sha256":input.profile.sha256,
        "resource_policy":input.policy,"resource_policy_digest":input.policy_digest,
        "model":profile.model,"target":current_target(),"isa":isa(),"cpu_fit":profile.cpu_fit,
        "process_id":std::process::id(),"execution_user":airk_ipc::endpoint::current_user()?,
        "native_binary_sha256":binary_sha256,"resource":ResourceSeries::new(profile.memory_reservation_bytes)?,
        "operation_journal":output.with_extension("operations.jsonl"),
        "measurement_identity":"native-original-operation-start-to-post-end-retention/1",
        "numeric_status":"unavailable_in_resource_only_run",
        "scope":"Actual service-lease native score-return/clock/process-lifetime-RSS observations only. No task quality, composed service admission, cold end-to-end, calibrated action, platform transfer or publication claim."});
    flush(&mut file, &receipt)?;
    let mut operations = operation::Journal::create(
        &output,
        operation::Identity {
            input_sha256: input_hash,
            profile: profile.manifest,
            process_id: std::process::id(),
            native_binary_sha256: binary_sha256,
        },
        input.policy.deadline_ms,
    )?;
    let (_parent, control) = private_control()?;
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(Arc::new(
        airk_store::service::ServiceLease::acquire(&control)?,
    ))?;
    let run = owner.run(|| -> Result<()> {
        verify_files(&files, &profile)?;
        let loaded = operations.observe(operation::Kind::Load, 0, None, || {
            Scorer::open(
                ExecutionProfile::for_qualification(profile.clone())?,
                &files,
            )
        })?;
        receipt["load_us"] = json!(loaded.elapsed_us);
        receipt["load_native_call_us"] = json!(loaded.native_call_elapsed_us);
        receipt["load_peak_process_rss_bytes"] = json!(peak_rss());
        flush(&mut file, &receipt)?;
        ensure!(
            loaded.within_deadline,
            "original native load deadline exceeded"
        );
        let scorer = loaded.value?;
        receipt["execution_identity"] = serde_json::to_value(scorer.execution_identity()?)?;
        let compiled = resource_input(&profile, &files, |state, question, candidates| {
            scorer.compile(state, question, candidates)
        })?;
        receipt["input"] = serde_json::to_value(&compiled)?;
        let warmed = operations.observe(
            operation::Kind::Warmup,
            0,
            Some(compiled.input_digest),
            || scorer.score(&compiled),
        )?;
        let peak = peak_rss().context("actual native process peak RSS unavailable")?;
        receipt["warmup"] = json!({"elapsed_us":warmed.elapsed_us,
            "native_call_elapsed_us":warmed.native_call_elapsed_us,"peak_process_rss_bytes":peak,
            "returned":true,"success":warmed.value.is_ok()});
        flush(&mut file, &receipt)?;
        ensure!(
            warmed.within_deadline,
            "original native warmup deadline exceeded"
        );
        warmed.value?;
        ensure!(
            warmed.elapsed_us <= input.policy.p95_limit_us
                && peak <= profile.memory_reservation_bytes,
            "first warmup exceeded frozen native resource bound; no measured forward admitted"
        );
        let mut series = ResourceSeries::new(profile.memory_reservation_bytes)?;
        let mut ordinal = 0;
        let result = observe_series(
            &mut series,
            &input.policy,
            || {
                ordinal += 1;
                let scored = operations.observe(
                    operation::Kind::Sample,
                    ordinal,
                    Some(compiled.input_digest),
                    || scorer.score(&compiled),
                )?;
                let elapsed_us = scored.elapsed_us;
                let peak_process_rss_bytes = peak_rss().context("actual native RSS unavailable")?;
                scored.value.map_err(|error| {
                    error.context(ObservedNativeFailure {
                        elapsed_us,
                        peak_process_rss_bytes,
                    })
                })?;
                // Original operation time includes both durable observer publications.
                // A late return therefore enters the real partial series as overrun,
                // and Core refuses the next forward without inventing a zero sample.
                Ok(ResourceSample {
                    elapsed_us,
                    peak_process_rss_bytes,
                })
            },
            |series| {
                receipt["resource"] = serde_json::to_value(series)?;
                flush(&mut file, &receipt)
            },
        );
        receipt["resource"] = serde_json::to_value(&series)?;
        result?;
        drop(scorer);
        verify_files(&files, &profile)?;
        pinned(&input.source_manifest, 16 << 20)?;
        receipt["candidate_resource_passed"] = true.into();
        receipt["complete"] = true.into();
        Ok(())
    });
    let result = run.and_then(std::convert::identity);
    if let Err(error) = &result {
        receipt["error"] = format!("{error:#}").into();
        if let Some(observed) = error.downcast_ref::<ObservedNativeFailure>() {
            receipt["native_failure_observation"] = json!({"native_return_observed":true,
                "elapsed_us":observed.elapsed_us,"peak_process_rss_bytes":observed.peak_process_rss_bytes});
        }
    }
    flush(&mut file, &receipt)?;
    result
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NumericCase {
    name: String,
    input: CompiledInput,
    reference_same_artifact: Option<Vec<f32>>,
    reference_original_fp32: Option<Vec<f32>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NumericFixture {
    schema: String,
    model: ModelChoice,
    compiler: Sha256Digest,
    resource_policy: Sha256Digest,
    reference_producer: Option<ResourceFilePin>,
    original_reference_assets: Vec<Asset>,
    rows: Vec<NumericCase>,
}
fn diagnostic(row: &Value, fixture: Sha256Digest) -> Result<CompiledInput> {
    let input = &row["input"];
    Ok(CompiledInput {
        prompt: String::new(),
        input_ids: serde_json::from_value(input["input_ids"].clone())?,
        attention_mask: serde_json::from_value(input["attention_mask"].clone())?,
        position_ids: serde_json::from_value(input["position_ids"].clone())?,
        decision_position: input["decision_position"]
            .as_i64()
            .context("native diagnostic decision")?,
        candidate_positions: serde_json::from_value(input["candidate_positions"].clone())?,
        option_mask: serde_json::from_value(input["option_mask"].clone())?,
        input_digest: framed_digest(
            b"airk.private-context-numeric-probe/2",
            &[fixture.as_bytes(), &serde_json::to_vec(row)?],
        ),
    })
}
fn softmax(values: &[f32]) -> Result<Vec<f64>> {
    ensure!(
        !values.is_empty() && values.iter().all(|value| value.is_finite()),
        "finite native numeric scores required"
    );
    let peak = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let values = values
        .iter()
        .map(|value| (f64::from(*value) - f64::from(peak)).exp())
        .collect::<Vec<_>>();
    let total: f64 = values.iter().sum();
    Ok(values.into_iter().map(|value| value / total).collect())
}
fn comparison(actual: &[f32], expected: &[f32]) -> Result<(f64, f64, bool)> {
    ensure!(
        actual.len() == expected.len(),
        "native numeric reference cardinality differs"
    );
    let a = softmax(actual)?;
    let b = softmax(expected)?;
    let winner = |scores: &[f32]| {
        scores.iter().enumerate().fold(
            0,
            |best, (index, value)| if *value > scores[best] { index } else { best },
        )
    };
    Ok((
        actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
            .fold(0f64, f64::max),
        a.iter()
            .zip(b)
            .map(|(a, b)| (a - b).abs())
            .fold(0f64, f64::max),
        winner(actual) == winner(expected),
    ))
}
fn numeric_shape(
    fixture: &NumericFixture,
    profile: &Profile,
    policy: &ResourcePolicy,
) -> Result<()> {
    ensure!(
        fixture.schema == "airk.arbiter.native-numeric/2"
            && fixture.model == profile.model
            && fixture.compiler == profile.compiler
            && fixture.resource_policy == policy.digest()?,
        "numeric fixture is not the exact current compiler/model/resource identity"
    );
    let focus = profile.model == ModelChoice::FocusQwenReranker06B;
    ensure!(
        fixture.rows.len() == if focus { 12 } else { 3 },
        "complete independent native numeric roster required"
    );
    ensure!(
        fixture
            .rows
            .iter()
            .map(|row| &row.name)
            .collect::<BTreeSet<_>>()
            .len()
            == fixture.rows.len(),
        "numeric case identities must be distinct"
    );
    for (index, row) in fixture.rows.iter().enumerate() {
        let (tokens, options) = if focus {
            (512, 2)
        } else {
            [(17, 3), (128, 8), (8192, 8)][index]
        };
        ensure!(
            !row.name.is_empty()
                && row.name.len() <= 128
                && row.input.input_ids.len() == tokens
                && row.input.input_ids.iter().all(|token| *token >= 0)
                && row.input.attention_mask.len() == tokens
                && row.input.position_ids.len() == tokens
                && row.input.position_ids.iter().copied().eq(0..tokens as i64)
                && row.input.decision_position == tokens as i64 - 1
                && row.input.candidate_positions.len() == options
                && row
                    .input
                    .candidate_positions
                    .iter()
                    .all(|token| *token >= 0)
                && row.input.option_mask == vec![true; options]
                && if focus {
                    // Focus keeps its genuine static-width left padding. The
                    // Context resource claim instead requires every token live.
                    row.input
                        .attention_mask
                        .iter()
                        .all(|mask| matches!(mask, 0 | 1))
                        && row.input.attention_mask.last() == Some(&1)
                        && row
                            .input
                            .attention_mask
                            .windows(2)
                            .all(|pair| pair[0] <= pair[1])
                } else {
                    row.input.attention_mask.iter().all(|mask| *mask == 1)
                },
            "current all-live numeric input shape differs; historical2048 cannot satisfy8192"
        );
    }
    Ok(())
}

#[test]
#[ignore = "independently produced same-artifact/original-FP32 references at current8192 width required"]
fn software_v2_selected_native_numeric() -> Result<()> {
    let (input, profile, files, input_hash) = inputs()?;
    let pin = input
        .numeric_fixture
        .as_ref()
        .context("current independent numeric reference unavailable")?;
    let raw = pinned(pin, 16 << 20)?;
    let fixture: NumericFixture = serde_json::from_slice(&raw)?;
    numeric_shape(&fixture, &profile, &input.policy)?;
    let producer = fixture
        .reference_producer
        .as_ref()
        .context("independent reference producer pin required")?;
    pinned(producer, 16 << 20)?;
    ensure!(
        !fixture.original_reference_assets.is_empty(),
        "original-reference artifact identities required"
    );
    for row in &fixture.rows {
        for reference in [&row.reference_same_artifact, &row.reference_original_fp32] {
            let reference = reference
                .as_ref()
                .context("current independent numeric reference unavailable")?;
            ensure!(
                reference.len() == row.input.candidate_positions.len()
                    && reference.iter().all(|value| value.is_finite()),
                "complete finite independent reference required before native load"
            );
        }
    }
    let output = PathBuf::from(std::env::var("AIRK_SOFTWARE_V2_NATIVE_OUTPUT")?);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let binary_sha256 = file_hash(&std::env::current_exe()?)?;
    let mut receipt = json!({"schema":"airk.arbiter.native-numeric-result/2","complete":false,"evaluation":true,
        "release_qualified":false,"input_sha256":input_hash,"profile":profile.manifest,
        "profile_source_sha256":input.profile.sha256,"fixture":pin,"source_manifest":input.source_manifest,
        "reference_producer":producer,"original_reference_assets":fixture.original_reference_assets,
        "resource_policy_digest":input.policy_digest,"resource_policy":input.policy,
        "model":profile.model,"target":current_target(),"isa":isa(),"cpu_fit":profile.cpu_fit,
        "process_id":std::process::id(),"execution_user":airk_ipc::endpoint::current_user()?,
        "native_binary_sha256":binary_sha256,"operation_journal":output.with_extension("operations.jsonl"),
        "measurement_identity":"native-original-operation-start-to-post-end-retention/1",
        "rows":[],"scope":"Actual native raw-logit/mass/argmax comparisons with separately pinned independent references; no semantic/calibration/composed-path/publication qualification."});
    flush(&mut file, &receipt)?;
    let mut operations = operation::Journal::create(
        &output,
        operation::Identity {
            input_sha256: input_hash,
            profile: profile.manifest,
            process_id: std::process::id(),
            native_binary_sha256: binary_sha256,
        },
        input.policy.deadline_ms,
    )?;
    let (_parent, control) = private_control()?;
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(Arc::new(
        airk_store::service::ServiceLease::acquire(&control)?,
    ))?;
    let run=owner.run(||->Result<()>{verify_files(&files,&profile)?;
        let loaded=operations.observe(operation::Kind::Load,0,None,|| {
            Scorer::open(ExecutionProfile::for_qualification(profile.clone())?,&files)
        })?;
        receipt["load_us"]=json!(loaded.elapsed_us);
        receipt["load_native_call_us"]=json!(loaded.native_call_elapsed_us);
        receipt["load_peak_process_rss_bytes"]=json!(peak_rss());
        flush(&mut file,&receipt)?;
        ensure!(loaded.within_deadline,"original native numeric load deadline exceeded");
        let scorer=loaded.value?;
        receipt["execution_identity"]=serde_json::to_value(scorer.execution_identity()?)?;
        for (ordinal,row) in fixture.rows.iter().enumerate() {
            let scored=operations.observe(operation::Kind::Numeric,ordinal+1,Some(row.input.input_digest),||scorer.score(&row.input))?;
            let elapsed_us=scored.elapsed_us;
            let native_call_elapsed_us=scored.native_call_elapsed_us;
            let within_deadline=scored.within_deadline;
            let actual=scored.value?;
            let(same_raw,same_mass,same_argmax)=comparison(&actual,row.reference_same_artifact.as_ref().context("same-artifact reference")?)?;
            let(original_raw,original_mass,original_argmax)=comparison(&actual,row.reference_original_fp32.as_ref().context("original FP32 reference")?)?;
            receipt["rows"].as_array_mut().context("numeric result rows")?.push(json!({"name":row.name,
                "input_digest":row.input.input_digest,"tokens":row.input.input_ids.len(),"options":row.input.candidate_positions.len(),
                "raw_scores":actual,"elapsed_us":elapsed_us,"native_call_elapsed_us":native_call_elapsed_us,"same_artifact_raw_error":same_raw,"same_artifact_mass_error":same_mass,
                "original_fp32_raw_error":original_raw,"original_fp32_mass_error":original_mass,"argmax_matches":same_argmax&&original_argmax}));
            flush(&mut file,&receipt)?;
            ensure!(within_deadline,"original native numeric score deadline exceeded; exact returned partial rows retained");
            let focus=profile.model==ModelChoice::FocusQwenReranker06B;
            ensure!(same_raw<=0.001&&same_mass<=if focus{0.00001}else{0.05}
                &&original_raw<=if focus{0.25}else{0.001}&&original_mass<=0.05&&same_argmax&&original_argmax,
                "frozen native numeric tolerance refused; exact observed partial rows retained");
        } drop(scorer);verify_files(&files,&profile)?;Ok(())
    });
    let result = run.and_then(std::convert::identity);
    receipt["complete"] = json!(result.is_ok());
    if let Err(error) = &result {
        receipt["error"] = format!("{error:#}").into();
    }
    flush(&mut file, &receipt)?;
    result
}

#[test]
#[ignore = "native current compiler/assets and fresh private numeric-template output required; no NN"]
fn emit_software_v2_native_numeric_template() -> Result<()> {
    let (input, profile, files, _) = inputs()?;
    let compiler_asset = profile
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Compiler)
        .context("native compiler asset")?;
    let tokenizer_asset = profile
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Tokenizer)
        .context("native tokenizer asset")?;
    let compiler = Compiler::from_verified(
        &files.read_asset(compiler_asset)?,
        &files.read_asset(tokenizer_asset)?,
    )?;
    let mut rows = Vec::new();
    for asset in profile
        .assets
        .iter()
        .filter(|asset| asset.role == AssetRole::Fixture)
    {
        ensure!(
            asset.bytes <= 4 << 20,
            "bounded original native fixture required"
        );
        let raw = files.read_asset(asset)?;
        let fixture: Value = serde_json::from_slice(&raw)?;
        if let Some(old) = fixture.get("rows").and_then(Value::as_array) {
            if profile.model == ModelChoice::FocusQwenReranker06B {
                for row in old {
                    let expected = &row["expected"];
                    let input = CompiledInput {
                        prompt: expected["prompt"].as_str().context("Focus prompt")?.into(),
                        input_ids: serde_json::from_value(expected["input_ids"].clone())?,
                        attention_mask: serde_json::from_value(expected["attention_mask"].clone())?,
                        position_ids: serde_json::from_value(expected["position_ids"].clone())?,
                        decision_position: expected["decision_position"]
                            .as_i64()
                            .context("Focus decision")?,
                        candidate_positions: serde_json::from_value(
                            expected["candidate_positions"].clone(),
                        )?,
                        option_mask: serde_json::from_value(expected["option_mask"].clone())?,
                        input_digest: framed_digest(
                            b"airk.private-focus-publisher-diagnostic/2",
                            &[asset.sha256.as_bytes(), &serde_json::to_vec(row)?],
                        ),
                    };
                    rows.push(NumericCase {
                        name: row["id"].as_str().context("Focus case id")?.into(),
                        input,
                        reference_same_artifact: None,
                        reference_original_fp32: None,
                    });
                }
            } else if old.len() >= 2 {
                for (index, row) in old.iter().take(2).enumerate() {
                    rows.push(NumericCase {
                        name: format!("all-live-{index}"),
                        input: diagnostic(row, asset.sha256)?,
                        reference_same_artifact: None,
                        reference_original_fp32: None,
                    });
                }
                rows.push(NumericCase {
                    name: "native-all-live-8192".into(),
                    input: resource_input(&profile, &files, |state, question, options| {
                        compiler.compile(state, question, options)
                    })?,
                    reference_same_artifact: None,
                    reference_original_fp32: None,
                });
            }
            break;
        }
    }
    let fixture = NumericFixture {
        schema: "airk.arbiter.native-numeric/2".into(),
        model: profile.model,
        compiler: profile.compiler,
        resource_policy: input.policy_digest,
        reference_producer: None,
        original_reference_assets: vec![],
        rows,
    };
    numeric_shape(&fixture, &profile, &input.policy)?;
    let output = PathBuf::from(std::env::var("AIRK_SOFTWARE_V2_NATIVE_OUTPUT")?);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&serde_json::to_vec_pretty(&fixture)?)?;
    file.sync_all()?;
    Ok(())
}

mod operation;
mod original_sources;

#[cfg(test)]
mod tests;

mod attention_diagnostic;

#[cfg(feature = "gpu-cuda")]
mod cuda_qualification;
