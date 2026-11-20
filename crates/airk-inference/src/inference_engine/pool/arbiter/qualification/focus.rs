// SPDX-License-Identifier: Apache-2.0
//! Explicit private Focus qualification; no serialized fixture supplies compute authority.
use super::{peak_rss, private_control};
use airk_core::{
    arbiter::{Asset, AssetRole, ModelChoice, Profile, Readout, Stage, Task},
    identity::{Sha256Digest, framed_digest},
};
use airk_retrieval::arbiter::{
    compiler::{Candidate, CompiledInput, focus},
    runtime::{AssetSource, ExecutionProfile, Scorer},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusInput {
    profile: PathBuf,
    files: BTreeMap<String, PathBuf>,
    fixtures: PathBuf,
}
struct FocusFiles(BTreeMap<String, PathBuf>);
impl AssetSource for FocusFiles {
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>> {
        let path = self
            .0
            .get(&asset.name)
            .context("asset outside authentic prepared file map")?;
        let file = std::fs::File::open(path)?;
        ensure!(
            file.metadata()?.len() == asset.bytes,
            "mapped native asset length differs"
        );
        let mut bytes = Vec::new();
        file.take(asset.bytes + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == asset.bytes,
            "mapped native asset changed while reading"
        );
        Ok(bytes)
    }
}

type LoadedInputs = (FocusInput, Profile, Vec<u8>, Vec<u8>, Value);

#[derive(Clone, Copy)]
struct Width {
    tokens: usize,
    rows: usize,
    resource_id: &'static str,
    minimum_occupied: i64,
}

const STATIC512: Width = Width {
    tokens: 512,
    rows: 12,
    resource_id: "long-maintenance-synthetic",
    minimum_occupied: 480,
};
const STATIC1024: Width = Width {
    tokens: 1024,
    rows: 14,
    resource_id: "synthetic-1024",
    minimum_occupied: 1024,
};

fn inputs(width: Width) -> Result<LoadedInputs> {
    let input_bytes = std::fs::read(
        std::env::var_os("AIRK_FOCUS_QUALIFICATION_INPUT")
            .context("missing Focus profile inputs")?,
    )?;
    let expected: Sha256Digest = std::env::var("AIRK_FOCUS_QUALIFICATION_INPUT_SHA256")
        .context("missing Focus input pin")?
        .parse()?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&input_bytes).into()) == expected,
        "Focus input descriptor differs"
    );
    let input: FocusInput = serde_json::from_slice(&input_bytes)?;
    let bytes = std::fs::read(&input.profile)?;
    let profile: Profile = serde_json::from_slice(&bytes)?;
    profile.validate()?;
    ensure!(
        input.files.keys().collect::<BTreeSet<_>>()
            == profile
                .assets
                .iter()
                .map(|asset| &asset.name)
                .collect::<BTreeSet<_>>(),
        "exact Rust-prepared asset overlay required"
    );
    ensure!(
        profile.model == ModelChoice::FocusQwenReranker06B
            && profile.stage == Some(Stage::Focus)
            && profile.tasks == std::collections::BTreeSet::from([Task::FocusRank])
            && profile.task_policies.get(&Task::FocusRank) == Some(&focus::policy_digest()?)
            && profile.abi.readout == Readout::FocusYesNo
            && profile.max_tokens == width.tokens
            && profile.max_candidates == 4
            && profile.qualification.is_none()
            && profile.qualified_targets.is_empty()
            && profile.downloads.is_empty(),
        "actual unqualified selected Focus profile required"
    );
    let fit = profile
        .cpu_fit
        .as_ref()
        .context("explicit experimental CPU tier required")?;
    fit.validate()?;
    ensure!(
        fit.minimum_physical_cores == 4 && fit.intra_threads == 4,
        "frozen experimental CPU4 tier required"
    );
    let fixture_bytes = std::fs::read(&input.fixtures)?;
    ensure!(
        profile
            .assets
            .iter()
            .any(|asset| asset.role == AssetRole::Fixture
                && asset.bytes == fixture_bytes.len() as u64
                && asset.sha256 == Sha256Digest::from_bytes(Sha256::digest(&fixture_bytes).into())),
        "independent Focus fixture is not an authenticated asset"
    );
    let fixture: Value = serde_json::from_slice(&fixture_bytes)?;
    ensure!(
        fixture["rows"]
            .as_array()
            .is_some_and(|rows| rows.len() == width.rows),
        "complete publisher control roster required"
    );
    if width.tokens == STATIC1024.tokens {
        ensure!(
            fixture["schema"] == "airk.private-independent-focus-publisher1024-fixture/1"
                && fixture["input_pins"]["width"] == 1024
                && profile
                    .assets
                    .iter()
                    .any(|asset| asset.role == AssetRole::Model
                        && fixture["input_pins"]["graph"] == asset.sha256.to_string()),
            "static1024 fixture requires its exact candidate graph"
        );
    }
    Ok((input, profile, bytes, fixture_bytes, fixture))
}
fn verify_assets(source: &FocusFiles, profile: &Profile) -> Result<()> {
    for asset in &profile.assets {
        let bytes = source.read_asset(asset)?;
        ensure!(
            bytes.len() as u64 == asset.bytes
                && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == asset.sha256,
            "native Focus asset differs"
        );
    }
    Ok(())
}
fn original_pair(row: &Value) -> Result<(String, Candidate)> {
    Ok((
        row["original_query"]
            .as_str()
            .context("missing original question")?
            .into(),
        Candidate {
            id: row["id"]
                .as_str()
                .context("missing original candidate identity")?
                .into(),
            text: row["original_document"]
                .as_str()
                .context("missing original source window")?
                .into(),
        },
    ))
}
fn diagnostic_input(row: &Value, fixture: Sha256Digest) -> Result<CompiledInput> {
    let expected = &row["expected"];
    Ok(CompiledInput {
        prompt: expected["prompt"]
            .as_str()
            .context("missing independent prompt")?
            .into(),
        input_ids: serde_json::from_value(expected["input_ids"].clone())?,
        attention_mask: serde_json::from_value(expected["attention_mask"].clone())?,
        position_ids: serde_json::from_value(expected["position_ids"].clone())?,
        decision_position: expected["decision_position"]
            .as_i64()
            .context("missing final decision position")?,
        candidate_positions: serde_json::from_value(expected["candidate_positions"].clone())?,
        option_mask: serde_json::from_value(expected["option_mask"].clone())?,
        input_digest: framed_digest(
            b"airk.private-focus-publisher-diagnostic/1",
            &[fixture.as_bytes(), &serde_json::to_vec(row)?],
        ),
    })
}
fn same_compiler_view(input: &CompiledInput, row: &Value) -> Result<()> {
    let observed = serde_json::to_value(input)?;
    for field in [
        "prompt",
        "input_ids",
        "attention_mask",
        "position_ids",
        "decision_position",
        "candidate_positions",
        "option_mask",
    ] {
        ensure!(
            observed[field] == row["expected"][field],
            "native Focus differs from independent publisher field: {field}"
        );
    }
    Ok(())
}
fn relevance(pair: &[f32]) -> Result<f64> {
    ensure!(
        pair.len() == 2 && pair.iter().all(|v| v.is_finite()),
        "finite exact no/yes pair required"
    );
    let peak = pair[0].max(pair[1]) as f64;
    let no = (f64::from(pair[0]) - peak).exp();
    let yes = (f64::from(pair[1]) - peak).exp();
    Ok(yes / (no + yes))
}
fn raw_error(actual: &[f32], expected: &[f32]) -> Result<(f64, f64, bool)> {
    let score = (relevance(actual)? - relevance(expected)?).abs();
    let raw = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
        .fold(0.0, f64::max);
    Ok((
        raw,
        score,
        (actual[1] > actual[0]) == (expected[1] > expected[0]),
    ))
}

#[test]
#[ignore = "explicit actual selected Focus assets, native CPU4 scope and publisher512 fixture required"]
fn focus_native_weight8_static512_matches_publisher_and_compiled_model_views() -> Result<()> {
    parity(STATIC512)
}

#[test]
#[ignore = "explicit isolated Focus1024 graph and publisher controls required"]
fn focus_native_weight8_static1024_matches_publisher_and_compiled_model_views() -> Result<()> {
    parity(STATIC1024)
}

#[test]
#[ignore = "requires the exact original release catalog CPU assets and static512 input fixture"]
fn legacy_focus_release_cpu_replays_original_compiled_pairs() -> Result<()> {
    let manifest: Sha256Digest =
        "04821edaed45ac10aa5b0b7b2c0b7b28bf5d04a06c8b623cfe6d762f77d5e23b".parse()?;
    let profile = airk_retrieval::arbiter::runtime::release_asset_profile(manifest)?;
    let execution = ExecutionProfile::from_release(manifest)?;
    let record = execution
        .source_transition_record()?
        .context("missing release transition")?;
    ensure!(
        record["legacy_manifest"] == manifest.to_string()
            && record["task_qualification_transferred"] == false,
        "wrong release source transition"
    );
    let directory = PathBuf::from(
        std::env::var_os("AIRK_LEGACY_FOCUS_ASSETS").context("missing original assets")?,
    );
    let metadata = PathBuf::from(
        std::env::var_os("AIRK_LEGACY_FOCUS_METADATA").context("missing original metadata")?,
    );
    let source = FocusFiles(
        profile
            .assets
            .iter()
            .map(|asset| {
                let path = if asset.name == "focus-relevance-metadata.json" {
                    metadata.clone()
                } else {
                    directory.join(&asset.name)
                };
                (asset.name.clone(), path)
            })
            .collect(),
    );
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease)?;
    owner.run(|| -> Result<()> {
        verify_assets(&source, &profile)?;
        let fixture = std::fs::read(directory.join("native-runtime512-fixture.json"))?;
        ensure!(
            format!("{:x}", Sha256::digest(&fixture))
                == "b07dbc3bf8167d54d2cb43a07b1cfa361173e7bf1c5207870dfafdcf5d71b080",
            "original fixture changed"
        );
        let fixture: Value = serde_json::from_slice(&fixture)?;
        let scorer = Scorer::open(execution, &source)?;
        let mut verified = Vec::new();
        for row in fixture["rows"]
            .as_array()
            .context("missing original inputs")?
        {
            if row["native_model_view"] != true {
                continue;
            }
            let (question, option) = original_pair(row)?;
            let compiled = scorer.compile("", &question, &[option])?;
            same_compiler_view(&compiled, row)?;
            let binding = scorer
                .source_transition_input(&compiled)?
                .context("missing original input binding")?;
            ensure!(
                binding["current_compiled_input"] == compiled.input_digest.to_string()
                    && binding["transition"] == record,
                "compiled transition differs"
            );
            let scores = scorer.score(&compiled)?;
            let expected =
                serde_json::from_value::<Vec<f32>>(row["python_weight8_logits"][0].clone())?;
            let (raw, relevance, winner) = raw_error(&scores, &expected)?;
            ensure!(
                raw <= 0.001 && relevance <= 0.00001 && winner,
                "original weight8 CPU math changed"
            );
            verified.push(json!({"id":row["id"],"binding":binding,"raw_scores":scores,
                "reference_scores":expected,"maximum_raw_error":raw}));
        }
        ensure!(verified.len() == 10, "incomplete compiled native fixture");
        println!(
            "{}",
            json!({"profile":manifest,"width":512,"rows":verified})
        );
        Ok(())
    })?
}

fn parity(width: Width) -> Result<()> {
    let (input, profile, profile_bytes, fixture_bytes, fixture) = inputs(width)?;
    let output = PathBuf::from(
        std::env::var_os("AIRK_FOCUS_NATIVE_PARITY_OUTPUT")
            .context("missing fresh native parity output")?,
    );
    ensure!(!output.exists(), "fresh parity output required");
    let source = FocusFiles(input.files);
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease)?;
    owner.run(|| -> Result<()> {
        verify_assets(&source, &profile)?;
        let fixture_hash = Sha256Digest::from_bytes(Sha256::digest(&fixture_bytes).into());
        let mut receipt = json!({"schema":"airk.private-focus-native-parity/1","complete":false,"evaluation":true,"release_qualified":false,"profile":profile.manifest,"profile_source_sha256":format!("{:x}",Sha256::digest(&profile_bytes)),"fixture_sha256":fixture_hash,"runtime":profile.runtime,"policy":focus::policy_digest()?,"rows":[],"scope":"Actual native CPU4 ORT numerical parity; native compiled model-view pairs and two explicitly raw publisher diagnostic pairs, no indexed authority/quality/composed-path qualification"});
        let run = (|| -> Result<()> {
            let started = Instant::now();
            let scorer = Scorer::open(ExecutionProfile::for_qualification(profile.clone())?, &source)?;
            receipt["load_us"] = json!(started.elapsed().as_micros());
            receipt["execution_identity"] = serde_json::to_value(scorer.execution_identity()?)?;
            let rows = fixture["rows"].as_array().context("missing rows")?;
            let mut native_count = 0;
            for row in rows {
                let native = row["native_model_view"].as_bool().context("missing explicit model-view kind")?;
                let compiled = if native {
                    native_count += 1;
                    let (question, option) = original_pair(row)?;
                    let actual = scorer.compile("", &question, &[option])?;
                    same_compiler_view(&actual, row)?;
                    actual
                } else { diagnostic_input(row, fixture_hash)? };
                let pair = |field: &str| -> Result<Vec<f32>> {
                    let value = if width.tokens == STATIC512.tokens { &row[field][0] } else { &row[field] };
                    Ok(serde_json::from_value(value.clone())?)
                };
                let publisher = pair("official_logits")?;
                let python = pair("python_weight8_logits")?;
                let started = Instant::now();
                let actual = scorer.score(&compiled)?;
                let elapsed = started.elapsed().as_micros();
                let (raw, score, argmax) = raw_error(&actual, &publisher)?;
                let (runtime_raw, runtime_score, runtime_argmax) = raw_error(&actual, &python)?;
                receipt["rows"].as_array_mut().unwrap().push(json!({"id":row["id"],"native_model_view":native,"input":compiled,"raw_scores":actual,"publisher_scores":publisher,"python_weight8_scores":python,"publisher_raw_error":raw,"publisher_relevance_error":score,"publisher_argmax_matches":argmax,"runtime_raw_error":runtime_raw,"runtime_relevance_error":runtime_score,"runtime_argmax_matches":runtime_argmax,"inference_us":elapsed}));
                ensure!(raw <= 0.25 && score <= 0.05 && argmax && runtime_raw <= 0.001 && runtime_score <= 0.00001 && runtime_argmax, "frozen actual native Focus numerical tolerance exceeded");
            }
            ensure!(native_count == width.rows - 2, "complete native model-view roster required");
            drop(scorer);
            verify_assets(&source, &profile)?;
            Ok(())
        })();
        receipt["complete"] = json!(run.is_ok());
        if let Err(error) = &run { receipt["error"] = json!(format!("{error:#}")); }
        std::fs::write(&output, serde_json::to_vec_pretty(&receipt)?)?;
        run
    })??;
    Ok(())
}

#[test]
#[ignore = "explicit isolated CPU4 Focus maximum-input twenty-sample native resource campaign"]
fn focus_native_cpu4_tier_records_maximum512_pair_resources() -> Result<()> {
    resource(STATIC512)
}

#[test]
#[ignore = "explicit isolated CPU4 Focus1024 maximum-input twenty-sample resource campaign"]
fn focus_native_cpu4_tier_records_maximum1024_pair_resources() -> Result<()> {
    resource(STATIC1024)
}

fn resource(width: Width) -> Result<()> {
    let (input, profile, profile_bytes, fixture_bytes, fixture) = inputs(width)?;
    let output = PathBuf::from(
        std::env::var_os("AIRK_FOCUS_NATIVE_RESOURCE_OUTPUT")
            .context("missing fresh resource output")?,
    );
    ensure!(!output.exists(), "fresh native resource output required");
    let source = FocusFiles(input.files);
    let (_parent, control) = private_control()?;
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(Arc::new(
        airk_store::service::ServiceLease::acquire(&control)?,
    ))?;
    owner.run(|| -> Result<()> {
        verify_assets(&source, &profile)?;
        let row = fixture["rows"].as_array().context("missing resource source")?.iter().find(|row| row["id"] == width.resource_id).context("missing frozen near-cap source")?;
        let (question, option) = original_pair(row)?;
        let started = Instant::now();
        let scorer = Scorer::open(ExecutionProfile::for_qualification(profile.clone())?, &source)?;
        let load_us = started.elapsed().as_micros();
        let compiled = scorer.compile("", &question, &[option])?;
        same_compiler_view(&compiled, row)?;
        ensure!(compiled.input_ids.len() == width.tokens && compiled.attention_mask.iter().sum::<i64>() >= width.minimum_occupied && compiled.candidate_positions == [2152,9693], "full static original source workload required");
        let warmup = Instant::now();scorer.score(&compiled)?;let warmup_us = warmup.elapsed().as_micros();
        let mut samples = Vec::new();
        for _ in 0..20 { let started = Instant::now();scorer.score(&compiled)?;samples.push(started.elapsed().as_micros()); }
        let mut ordered = samples.clone();ordered.sort_unstable();
        let peak = peak_rss().context("native process lifetime peak unavailable")?;
        let identity = scorer.execution_identity()?;
        drop(scorer);verify_assets(&source, &profile)?;
        let receipt = json!({"schema":"airk.private-focus-native-resource/1","complete":true,"evaluation":true,"release_qualified":false,"profile":profile.manifest,"profile_source_sha256":format!("{:x}",Sha256::digest(&profile_bytes)),"fixture_sha256":format!("{:x}",Sha256::digest(&fixture_bytes)),"execution_identity":identity,"input":compiled,"load_us":load_us,"warmup_us":warmup_us,"samples_us":samples,"nearest_rank_p95_us":ordered[18],"maximum_us":ordered[19],"peak_process_rss_bytes":peak,"scope":"Actual CPU4 native fixed-width pair and20warm measurements under real native lease; no composed-path admission/latency, cancellation/source authority, end-user RAM floor or publication claim"});
        std::fs::write(&output, serde_json::to_vec_pretty(&receipt)?)?;
        Ok(())
    })??;
    Ok(())
}

#[test]
#[ignore = "requires the original pinned DEV_Q002 first Search and unqualified Focus1024 compiler"]
fn focus_static1024_compiles_original_dev_pair_without_dispatch() -> Result<()> {
    use airk_retrieval::arbiter::compiler::Compiler;
    use std::io::Write;

    let (input, profile, _, _, _) = inputs(STATIC1024)?;
    let answer = std::fs::read(
        std::env::var_os("AIRK_FOCUS_NATIVE_ORIGINAL_ANSWER").context("missing original Answer")?,
    )?;
    let answer_sha = Sha256Digest::from_bytes(Sha256::digest(&answer).into());
    ensure!(
        answer_sha.to_string()
            == "9997a58787a9eed1e845b3d1941c499c63ad105549b0e80e377999cea9dbeaa0",
        "original Answer identity differs"
    );
    let packet: Value = serde_json::from_slice(&answer)?;
    let body = &packet["structuredContent"];
    let record = &body["records"][0];
    let result = &record["canonical_result"];
    let receipt = &result["focus"]["receipt"];
    let view = &receipt["presentations"][0];
    let query = record["arguments"]["query"]
        .as_str()
        .context("original query")?;
    let text = result["hits"][0]["text"].as_str().context("first source")?;
    let id = view["candidate"].as_str().context("presented candidate")?;
    ensure!(
        record["operation"] == "search"
            && body["question"] == query
            && query.len() == 1702
            && view["candidate"] == receipt["original_order"][0]
            && view["utf8_start"] == 0
            && view["utf8_end"] == 620
            && view["source_bytes"] == 620
            && text.len() == 620,
        "original untruncated question or source window differs"
    );
    let source = FocusFiles(input.files);
    let bytes = |role| -> Result<Vec<u8>> {
        let asset = profile
            .assets
            .iter()
            .find(|asset| asset.role == role)
            .context("missing compiler asset")?;
        let value = source.read_asset(asset)?;
        ensure!(
            Sha256Digest::from_bytes(Sha256::digest(&value).into()) == asset.sha256,
            "compiler source changed"
        );
        Ok(value)
    };
    let compiler =
        Compiler::from_verified(&bytes(AssetRole::Compiler)?, &bytes(AssetRole::Tokenizer)?)?;
    let pair = compiler.compile(
        "",
        query,
        &[Candidate {
            id: id.to_owned(),
            text: text.to_owned(),
        }],
    )?;
    ensure!(
        pair.input_ids.len() == 1024
            && pair.attention_mask.len() == 1024
            && pair.attention_mask[..166].iter().all(|&v| v == 0)
            && pair.input_ids[..166].iter().all(|&v| v == 151643)
            && pair.attention_mask[166..].iter().all(|&v| v == 1)
            && pair.position_ids.iter().copied().eq(0..1024)
            && pair.decision_position == 1023
            && pair.candidate_positions == [2152, 9693]
            && pair.option_mask == [true, true],
        "native compiler lost original positions, source or padding"
    );
    ensure!(
        format!("{:x}", Sha256::digest(serde_json::to_vec(&pair.input_ids)?))
            == "c2cfbaa85f91e1eba2f972ef77ebb8f5f0f1b7edbef7ca69c19e85b89e09c8bf",
        "compiled IDs differ from the independent publisher reference"
    );
    let output = PathBuf::from(
        std::env::var_os("AIRK_FOCUS_NATIVE_COMPILE_OUTPUT").context("missing compile output")?,
    );
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)?;
    serde_json::to_writer_pretty(
        &mut file,
        &json!({"schema":"airk.private-focus-original-compile/1","complete":true,
        "source_answer":answer_sha,"profile":profile.manifest,"compiler":profile.compiler,"compiled":pair,
        "unpadded_tokens":858,"left_padding":166,"model_invocations":0,"release_qualified":false}),
    )?;
    file.flush()?;
    Ok(())
}
