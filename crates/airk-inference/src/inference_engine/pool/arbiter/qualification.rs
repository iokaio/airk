// SPDX-License-Identifier: Apache-2.0
//! Explicit private qualification, never part of release profile admission.
use super::*;
use airk_core::arbiter::{Asset, ModelChoice, Profile};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    profile: PathBuf,
    assets: PathBuf,
    fixtures: PathBuf,
}
struct Files(PathBuf);
impl AssetSource for Files {
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>> {
        let path = self.0.join(&asset.name);
        let file = std::fs::File::open(path)?;
        ensure!(
            file.metadata()?.len() == asset.bytes,
            "qualification asset length differs"
        );
        use std::io::Read;
        let mut bytes = Vec::new();
        file.take(asset.bytes + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == asset.bytes,
            "qualification asset changed while reading"
        );
        Ok(bytes)
    }
}
fn private_control() -> Result<(tempfile::TempDir, PathBuf)> {
    let parent = tempfile::tempdir()?;
    let control = parent.path().join("native-control");
    #[cfg(windows)]
    local_ipc::create_root(&control)?;
    #[cfg(not(windows))]
    local_ipc::prepare_root(&control)?;
    // Revalidate actual native ownership; an existing inherited temp directory
    // is never modified or accepted as a fabricated private boundary.
    local_ipc::prepare_root(&control)?;
    Ok((parent, control))
}
#[test]
fn qualification_control_child_has_real_native_private_ownership_without_model_load() -> Result<()>
{
    let (parent, control) = private_control()?;
    assert_eq!(control.parent(), Some(parent.path()));
    assert_ne!(control, parent.path());
    local_ipc::prepare_root(&control)?;
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    assert_eq!(lease.root().canonicalize()?, control.canonicalize()?);
    #[cfg(windows)]
    assert!(
        local_ipc::create_root(&control).is_err(),
        "exclusive private creation must not adopt an existing directory"
    );
    Ok(())
}
fn vector<T: serde::de::DeserializeOwned>(case: &Value, name: &str) -> Result<Vec<T>> {
    serde_json::from_value(case[name][0].clone())
        .with_context(|| format!("missing reference {name}"))
}

#[test]
#[ignore = "explicit four-model native qualification requires AIRK_ARBITER_QUALIFICATION_INPUT and OUTPUT"]
fn four_complete_trained_models_match_independent_compilers_and_raw_scores() -> Result<()> {
    let input = std::env::var_os("AIRK_ARBITER_QUALIFICATION_INPUT")
        .context("missing qualification input")?;
    let output = std::env::var_os("AIRK_ARBITER_QUALIFICATION_OUTPUT")
        .context("missing qualification output")?;
    let inputs: Vec<Input> = serde_json::from_slice(&std::fs::read(input)?)?;
    ensure!(
        inputs.len() == 4,
        "qualification requires all four candidate slots"
    );
    let (_control_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease)?;
    let receipt = owner.run(|| -> Result<Value> {
        let mut seen = std::collections::BTreeSet::new();
        let mut records = Vec::new();
        for input in inputs {
            let profile_bytes = std::fs::read(&input.profile)?;
            let profile: Profile = serde_json::from_slice(&profile_bytes)?;
            ensure!(seen.insert(profile.model), "duplicate candidate slot");
            let source = Files(input.assets);
            let fixture_bytes = std::fs::read(&input.fixtures)?;
            let fixture_hash = Sha256Digest::from_bytes(Sha256::digest(&fixture_bytes).into());
            ensure!(profile.assets.iter().any(|asset| asset.role == airk_core::arbiter::AssetRole::Fixture && asset.bytes == fixture_bytes.len() as u64 && asset.sha256 == fixture_hash),"independent fixture bytes differ from authenticated profile asset");
            let fixture: Value = serde_json::from_slice(&fixture_bytes)?;
            let cases = fixture["cases"].as_array().context("missing independent compiler cases")?;
            ensure!(cases.len() == 3, "all ASCII, Unicode and hostile-control cases required");
            records.push(json!({"model":profile.model,"profile_manifest":profile.manifest,"profile_source_sha256":format!("{:x}",Sha256::digest(&profile_bytes)),"fixture_sha256":fixture_hash,"runtime":profile.runtime,"evaluation":true,"release_qualified":false,"status":"loading","cases":[]}));
            std::fs::write(&output,serde_json::to_vec_pretty(&json!({"schema":"airk.arbiter.native-model-parity/1","complete":false,"models":records}))?)?;
            let start = Instant::now();
            let scorer = Scorer::open(ExecutionProfile::for_qualification(profile.clone())?, &source).with_context(||format!("native model load failed for {:?} manifest {}",profile.model,profile.manifest))?;
            let load_ms = start.elapsed().as_millis();
            records.last_mut().unwrap()["status"]="loaded".into();
            records.last_mut().unwrap()["load_ms"]=serde_json::to_value(load_ms)?;
            std::fs::write(&output,serde_json::to_vec_pretty(&json!({"schema":"airk.arbiter.native-model-parity/1","complete":false,"models":records}))?)?;
            let mut results = Vec::new();
            for case in cases {
                let candidates: Vec<Candidate> = serde_json::from_value(case["options"].clone())?;
                let compiled = scorer.compile(case["state"].as_str().context("missing state")?, case["question"].as_str().context("missing question")?, &candidates)?;
                assert_eq!(compiled.prompt,case["prompt"].as_str().unwrap());
                assert_eq!(compiled.input_ids,vector::<i64>(case,"input_ids")?);
                assert_eq!(compiled.attention_mask,vector::<i64>(case,"attention_mask")?);
                assert_eq!(compiled.position_ids,vector::<i64>(case,"position_ids")?);
                assert_eq!(compiled.decision_position,case["decision_positions"][0].as_i64().unwrap());
                assert_eq!(compiled.candidate_positions,vector::<i64>(case,"candidate_positions")?);
                assert_eq!(compiled.option_mask,vector::<bool>(case,"option_mask")?);
                let expected = vector::<f32>(case,"raw_scores")?;
                let started = Instant::now();
                let actual = scorer.score(&compiled)?;
                ensure!(actual.len() == expected.len(),"trained score count differs");
                let error = actual.iter().zip(&expected).map(|(a,b)| (a-b).abs()).fold(0f32,f32::max);
                results.push(json!({"name":case["name"],"tokens":compiled.input_ids.len(),"raw_scores":actual,"expected":expected,"max_absolute_error":error,"inference_ms":started.elapsed().as_millis()}));
                records.last_mut().unwrap()["cases"]=serde_json::to_value(&results)?;
                std::fs::write(&output,serde_json::to_vec_pretty(&json!({"schema":"airk.arbiter.native-model-parity/1","complete":false,"models":records}))?)?;
                ensure!(error <= 0.00025,"native trained raw-score parity failed: model={:?}, case={}, error={error}, actual={actual:?}, expected={expected:?}",profile.model,case["name"]);
            }
            *records.last_mut().unwrap()=json!({"model":profile.model,"profile_manifest":profile.manifest,"profile_source_sha256":format!("{:x}",Sha256::digest(&profile_bytes)),"fixture_sha256":fixture_hash,"runtime":profile.runtime,"execution_identity":scorer.execution_identity()?,"evaluation":true,"release_qualified":false,"status":"passed","load_ms":load_ms,"cases":results});
            std::fs::write(&output,serde_json::to_vec_pretty(&json!({"schema":"airk.arbiter.native-model-parity/1","complete":false,"models":records}))?)?;
            // Explicit drop before the next full candidate, never concurrent
            // 4-model memory inflation or borrowed native-session reuse.
            drop(scorer);
        }
        assert_eq!(seen,std::collections::BTreeSet::from([ModelChoice::SmallIntern,ModelChoice::SmallStrands,ModelChoice::LargeIntern,ModelChoice::LargeKev]));
        Ok(json!({"schema":"airk.arbiter.native-model-parity/1","complete":true,"scope":"complete trained models and independent compiler developer fixtures; no human quality or task calibration claim","models":records}))
    })??;
    std::fs::write(output, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(())
}

#[cfg(windows)]
fn peak_rss() -> Option<u64> {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    let mut memory: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&memory) as u32;
    memory.cb = size;
    // SAFETY: current-process pseudo handle and correctly sized output storage.
    (unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut memory, size) } != 0)
        .then_some(memory.PeakWorkingSetSize as u64)
}
#[cfg(unix)]
fn peak_rss() -> Option<u64> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: actual current-process observation with correctly sized storage.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    let bytes = u64::try_from(usage.ru_maxrss).ok()?;
    #[cfg(target_os = "macos")]
    {
        Some(bytes)
    }
    #[cfg(not(target_os = "macos"))]
    {
        bytes.checked_mul(1024)
    }
}
#[test]
#[ignore = "explicit isolated model resource campaign; no concurrent compiler/model workloads"]
fn complete_trained_model_maximum_context_has_twenty_native_warm_measurements() -> Result<()> {
    native_resource_measurements(false)
}

#[test]
#[ignore = "explicit large CPU16 developer diagnostic only; no resource qualification"]
fn large_cpu16_maximum_context_developer_probe_has_one_warmup_and_three_observations() -> Result<()>
{
    native_resource_measurements(true)
}

#[test]
#[ignore = "explicit large CPU24 developer diagnostic only; no resource qualification"]
fn large_cpu24_maximum_context_developer_probe_has_one_warmup_and_three_observations() -> Result<()>
{
    native_resource_measurements_for_tier(Some(24))
}

fn native_resource_measurements(developer_probe: bool) -> Result<()> {
    native_resource_measurements_for_tier(developer_probe.then_some(16))
}
fn native_resource_measurements_for_tier(probe_tier: Option<usize>) -> Result<()> {
    let developer_probe = probe_tier.is_some();
    let inputs: Vec<Input> = serde_json::from_slice(&std::fs::read(
        std::env::var_os("AIRK_ARBITER_QUALIFICATION_INPUT").context("missing resource inputs")?,
    )?)?;
    let selected = std::env::var("AIRK_ARBITER_RESOURCE_MODEL")
        .context("select exactly one model per fresh test process")?;
    let output =
        std::env::var_os("AIRK_ARBITER_RESOURCE_OUTPUT").context("missing resource receipt")?;
    let mut matches = Vec::new();
    for input in inputs {
        let bytes = std::fs::read(&input.profile)?;
        let profile: Profile = serde_json::from_slice(&bytes)?;
        if serde_json::to_value(profile.model)?.as_str() == Some(selected.as_str()) {
            matches.push((input, profile, bytes));
        }
    }
    ensure!(
        matches.len() == 1,
        "resource model selection must identify one exact profile"
    );
    let (input, profile, profile_bytes) = matches.pop().unwrap();
    if let Some(required_tier) = probe_tier {
        ensure!(
            matches!(required_tier, 16 | 24),
            "closed developer tier required"
        );
        let fit = profile
            .cpu_fit
            .as_ref()
            .context("explicit developer execution tier required")?;
        ensure!(
            matches!(
                profile.model,
                ModelChoice::LargeIntern | ModelChoice::LargeKev
            ) && fit.minimum_physical_cores == required_tier
                && fit.intra_threads == required_tier,
            "developer diagnostic requires the exact explicit large tier"
        );
    }

    let (_control_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease)?;
    owner.run(|| -> Result<()> {
        ensure!(profile.max_tokens == 2048 && profile.max_candidates >= 8,"resource campaign requires frozen full input limits");
        let started=Instant::now();
        let scorer=Scorer::open(ExecutionProfile::for_qualification(profile.clone())?, &Files(input.assets))?;
        let load_us=started.elapsed().as_micros();
        let options:Vec<Candidate>=(0..8).map(|i|Candidate{id:format!("read-{i}"),text:format!("Read verified indexed source section {i}.")}).collect();
        let question="Which verified additional indexed read is most useful?";
        let mut low=0usize; let mut high=4096usize;
        let mut maximum=None;
        while low<=high {
            let middle=low+(high-low)/2;
            let state=" source".repeat(middle);
            match scorer.compile(&state,question,&options) {
                Ok(compiled)=> {maximum=Some(compiled);low=middle+1;},
                Err(_)=> {if middle==0 {break;} high=middle-1;},
            }
        }
        let compiled=maximum.context("no bounded maximum-input resource case")?;
        ensure!(compiled.input_ids.len()==2048 && compiled.candidate_positions.len()==8,"resource input must exactly hit T2048/N8; no shortened substitution");
        let warmup=Instant::now(); scorer.score(&compiled)?; let warmup_us=warmup.elapsed().as_micros();
        let mut samples=Vec::new();
        for _ in 0..(if developer_probe {3}else{20}) {let started=Instant::now();scorer.score(&compiled)?;samples.push(started.elapsed().as_micros());}
        let mut ordered=samples.clone();ordered.sort_unstable();
        let peak=peak_rss().context("native peak RSS unavailable")?;
        let ceiling=if matches!(profile.model,ModelChoice::SmallIntern|ModelChoice::SmallStrands){12u64<<30}else{24u64<<30};
        let p95=if developer_probe {None}else{Some(ordered[18])};
        let maximum=*ordered.last().unwrap();
        let passed=!developer_probe && ordered[18]<=30_000_000 && peak<=ceiling;
        let receipt=json!({"schema":if developer_probe {"airk.arbiter.native-resource-probe/1"}else{"airk.arbiter.native-resource/1"},"developer_probe":developer_probe,"evaluation":true,"release_qualified":false,"model":profile.model,"profile":profile.manifest,"profile_source_sha256":format!("{:x}",Sha256::digest(&profile_bytes)),"execution_identity":scorer.execution_identity()?,"input":compiled,"load_us":load_us,"warmup_us":warmup_us,"samples_us":samples,"nearest_rank_p95_us":p95,"maximum_us":maximum,"peak_process_rss_bytes":peak,"rss_ceiling_bytes":ceiling,"passed":passed,"scope":if developer_probe {"one warmup plus three native maximum-input developer observations; NOT a twenty-sample p95 or resource qualification"}else{"authenticated native maximum-input load and twenty warm measurements; process lifetime peak, no human quality or cancellation/recovery claim"}});
        std::fs::write(output,serde_json::to_vec_pretty(&receipt)?)?;
        ensure!(developer_probe || passed,"frozen resource floor refused; retained exact native receipt");
        Ok(())
    })??;
    Ok(())
}

#[test]
#[ignore = "one explicit model; actual native task transcript and pinned assets; calibration partition only"]
fn actual_native_calibration_task_vectors_produce_raw_scores_without_release_admission()
-> Result<()> {
    let transcripts = std::fs::read(
        std::env::var_os("AIRK_ARBITER_CALIBRATION_SCORE_TRANSCRIPT")
            .context("missing native task transcript")?,
    )?;
    let transcript_hash = Sha256Digest::from_bytes(Sha256::digest(&transcripts).into());
    let expected: Sha256Digest = std::env::var("AIRK_ARBITER_CALIBRATION_SCORE_TRANSCRIPT_SHA256")
        .context("missing transcript pin")?
        .parse()?;
    ensure!(
        transcript_hash == expected,
        "native task transcript bytes changed"
    );
    let transcript: Value = serde_json::from_slice(&transcripts)?;
    ensure!(
        transcript["schema"] == "airk.arbiter.native-calibration-transcripts/1"
            && transcript["evaluation"] == true
            && transcript["release_qualified"] == false,
        "actual native evaluation transcript required"
    );
    let input_bytes = std::fs::read(
        std::env::var_os("AIRK_ARBITER_CALIBRATION_SCORE_PROFILES")
            .context("missing exact four model inputs")?,
    )?;
    let input_hash = Sha256Digest::from_bytes(Sha256::digest(&input_bytes).into());
    let expected: Sha256Digest = std::env::var("AIRK_ARBITER_CALIBRATION_SCORE_PROFILES_SHA256")
        .context("missing profile input pin")?
        .parse()?;
    ensure!(input_hash == expected, "native profile input bytes changed");
    ensure!(
        transcript["profile_inputs_sha256"] == input_hash.to_string(),
        "transcript consumed a different profile input file"
    );
    let inputs: Vec<Input> = serde_json::from_slice(&input_bytes)?;
    ensure!(
        inputs.len() == 4,
        "exactly four candidate model inputs required"
    );
    let selected = std::env::var("AIRK_ARBITER_CALIBRATION_SCORE_MODEL")
        .context("select exactly one native model")?;
    let output = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_CALIBRATION_SCORE_OUTPUT")
            .context("missing fresh score output")?,
    );
    ensure!(
        !output.try_exists()?,
        "native calibration score output must be fresh"
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut matched = None;
    for input in inputs {
        let profile_bytes = std::fs::read(&input.profile)?;
        let profile: Profile = serde_json::from_slice(&profile_bytes)?;
        profile.validate()?;
        ensure!(
            seen.insert(profile.model),
            "duplicate native calibration model slot"
        );
        if serde_json::to_value(profile.model)?.as_str() == Some(selected.as_str()) {
            matched = Some((input, profile, profile_bytes));
        }
    }
    ensure!(
        seen == std::collections::BTreeSet::from([
            ModelChoice::SmallIntern,
            ModelChoice::SmallStrands,
            ModelChoice::LargeIntern,
            ModelChoice::LargeKev
        ]),
        "all four closed model slots required"
    );
    let (input, profile, profile_bytes) = matched.context("unknown calibration model")?;
    let profile_hash = Sha256Digest::from_bytes(Sha256::digest(&profile_bytes).into());
    let policy = airk_retrieval::arbiter::tasks::policy_digest()?;
    let profiles = transcript["profiles"]
        .as_array()
        .context("missing native transcript profiles")?;
    let candidates: Vec<_> = profiles
        .iter()
        .filter(|item| item["model"] == serde_json::to_value(profile.model).unwrap())
        .collect();
    ensure!(
        candidates.len() == 1,
        "native transcript model slot ambiguous"
    );
    let entry = candidates[0];
    ensure!(
        entry["profile"] == serde_json::to_value(profile.manifest)?
            && entry["profile_source_sha256"] == profile_hash.to_string()
            && entry["compiler"] == serde_json::to_value(profile.compiler)?
            && entry["task_policy"] == serde_json::to_value(policy)?,
        "native transcript/profile/compiler/task-policy binding differs"
    );
    let rows = entry["rows"]
        .as_array()
        .context("missing calibration rows")?;
    ensure!(
        !rows.is_empty() && rows.len() <= 29,
        "calibration partition bound exceeded"
    );
    let mut ids = std::collections::BTreeSet::new();
    for row in rows {
        ensure!(
            ids.insert(row["id"].as_str().context("calibration id missing")?),
            "duplicate calibration id"
        );
    }
    let (_parent, control) = private_control()?;
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(Arc::new(
        airk_store::service::ServiceLease::acquire(&control)?,
    ))?;
    owner.run(||->Result<()> {
        let mut result=json!({"schema":"airk.arbiter.native-calibration-scores/1","complete":false,"evaluation":true,"release_qualified":false,"model":profile.model,"profile":profile.manifest,"profile_source_sha256":profile_hash,"compiler":profile.compiler,"task_policy":policy,"transcript_sha256":transcript_hash,"capture_sha256":transcript["capture_sha256"],"profile_inputs_sha256":input_hash,"runtime":profile.runtime,"binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe()?)?)),"status":"loading","rows":[]});
        std::fs::write(&output,serde_json::to_vec_pretty(&result)?)?;
        let started=Instant::now();
        let scorer=Scorer::open(ExecutionProfile::for_qualification(profile.clone())?,&Files(input.assets)).with_context(||format!("calibration native load failed for {:?}",profile.model))?;
        result["load_us"]=serde_json::to_value(started.elapsed().as_micros())?;
        result["execution_identity"]=serde_json::to_value(scorer.execution_identity()?)?;
        result["status"]="scoring".into();
        let mut scores=Vec::new();
        for row in rows {
            if row["applied"]==false {scores.push(json!({"id":row["id"],"applied":false,"reason":row["reason"]}));}
            else {
                ensure!(row["applied"]==true,"native transcript application state missing");
                let prompt=&row["prompt"];
                let options:Vec<Candidate>=serde_json::from_value(prompt["candidates"].clone())?;
                let compiled=scorer.compile(prompt["state"].as_str().context("native state missing")?,prompt["question"].as_str().context("native question missing")?,&options)?;
                ensure!(serde_json::to_value(&compiled)?==row["compiled"],"actual native calibration vectors differ from exact emitted task transcript");
                ensure!(compiled.input_ids.len()<=2048 && options.len()<=8,"native calibration input budget crossed");
                let started=Instant::now();let raw=scorer.score(&compiled)?;
                ensure!(raw.len()==options.len() && raw.iter().all(|score|score.is_finite()),"invalid native calibration scores");
                scores.push(json!({"id":row["id"],"applied":true,"raw_scores":raw,"input_digest":compiled.input_digest,"input_tokens":compiled.input_ids.len(),"inference_us":started.elapsed().as_micros(),"candidates":options}));
            }
            result["rows"]=serde_json::to_value(&scores)?;
            std::fs::write(&output,serde_json::to_vec_pretty(&result)?)?;
        }
        result["status"]="complete".into();result["complete"]=true.into();
        std::fs::write(&output,serde_json::to_vec_pretty(&result)?)?;
        Ok(())
    })??;
    Ok(())
}

#[derive(serde::Serialize)]
struct EmpiricalNextReadCalibration {
    schema: &'static str,
    task: airk_core::arbiter::Task,
    task_policy: Sha256Digest,
    read_population: &'static str,
    decision_rule: &'static str,
    probability_calibration_claim: bool,
    empirical_summary_sha256: Sha256Digest,
    native_score_sha256: Sha256Digest,
    source_profile_sha256: Sha256Digest,
    counts: std::collections::BTreeMap<String, u64>,
    limitation: &'static str,
}
fn exact_asset_file(path: &std::path::Path, asset: &Asset) -> Result<()> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() == asset.bytes,
        "materialized asset length differs"
    );
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .context("asset byte count overflow")?;
        ensure!(total <= asset.bytes, "materialized asset grew");
        digest.update(&buffer[..count]);
    }
    ensure!(
        total == asset.bytes && Sha256Digest::from_bytes(digest.finalize().into()) == asset.sha256,
        "materialized asset bytes differ from original pin"
    );
    Ok(())
}
fn private_evaluation_identity(profile: &Profile) -> Result<Sha256Digest> {
    let mut preimage = profile.clone();
    preimage.manifest = Sha256Digest::from_bytes([0; 32]);
    preimage.qualification = None;
    preimage.qualified_targets.clear();
    Ok(airk_core::identity::framed_digest(
        b"airk.arbiter.evaluation-profile/1",
        &[&serde_json::to_vec(&preimage)?],
    ))
}

#[test]
#[ignore = "private native profile materialization AFTER reviewed actual hybrid empirical summary; no product qualification"]
fn materialize_private_next_read_profiles_from_actual_native_empirical_evidence() -> Result<()> {
    let input_bytes = std::fs::read(
        std::env::var_os("AIRK_ARBITER_EVALUATION_MATERIALIZE_SOURCES")
            .context("missing original four profile inputs")?,
    )?;
    let input_hash = Sha256Digest::from_bytes(Sha256::digest(&input_bytes).into());
    let expected: Sha256Digest =
        std::env::var("AIRK_ARBITER_EVALUATION_MATERIALIZE_SOURCES_SHA256")
            .context("missing source input pin")?
            .parse()?;
    ensure!(
        input_hash == expected,
        "original source profile input changed"
    );
    let inputs: Vec<Input> = serde_json::from_slice(&input_bytes)?;
    ensure!(
        inputs.len() == 4,
        "all four original model profiles required"
    );
    let summary_bytes = std::fs::read(
        std::env::var_os("AIRK_ARBITER_EVALUATION_MATERIALIZE_SUMMARY")
            .context("missing reviewed empirical summary")?,
    )?;
    let summary_hash = Sha256Digest::from_bytes(Sha256::digest(&summary_bytes).into());
    let expected: Sha256Digest =
        std::env::var("AIRK_ARBITER_EVALUATION_MATERIALIZE_SUMMARY_SHA256")
            .context("missing empirical summary pin")?
            .parse()?;
    ensure!(
        summary_hash == expected,
        "reviewed empirical summary changed"
    );
    let summary: Value = serde_json::from_slice(&summary_bytes)?;
    ensure!(
        summary["schema"] == "airk.private-native-empirical-read-utility/1"
            && summary["evaluation"] == true
            && summary["release_qualified"] == false
            && summary["probability_calibration_claim"] == false
            && summary["capture_read_population"] == "trained-v2-native-default-hybrid",
        "actual hybrid empirical evidence required; no probability/product claim"
    );
    let destination = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_MATERIALIZE_OUTPUT")
            .context("missing fresh private destination")?,
    );
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .context("repository root missing")?;
    let artifacts = repository.join("artifacts").canonicalize()?;
    ensure!(
        destination.is_absolute()
            && !destination.try_exists()?
            && destination
                .parent()
                .context("output parent missing")?
                .canonicalize()?
                .starts_with(&artifacts),
        "fresh private artifact output required"
    );
    std::fs::create_dir(&destination)?;
    let policy = airk_retrieval::arbiter::tasks::policy_digest()?;
    let mut seen = std::collections::BTreeSet::new();
    let mut qualification_inputs = Vec::new();
    let mut evaluation_inputs = Vec::new();
    for input in inputs {
        let original_bytes = std::fs::read(&input.profile)?;
        let original_hash = Sha256Digest::from_bytes(Sha256::digest(&original_bytes).into());
        let mut profile: Profile = serde_json::from_slice(&original_bytes)?;
        profile.validate()?;
        ensure!(
            profile.qualification.is_none() && seen.insert(profile.model),
            "only distinct unqualified source profiles allowed"
        );
        let model = serde_json::to_value(profile.model)?
            .as_str()
            .context("model name missing")?
            .to_owned();
        let artifact = &summary["native_artifacts"][&model];
        let native = &artifact["native_identity"];
        ensure!(
            native["profile"] == serde_json::to_value(profile.manifest)?
                && native["profile_source_sha256"] == original_hash.to_string()
                && native["compiler"] == serde_json::to_value(profile.compiler)?
                && native["task_policy"] == serde_json::to_value(policy)?
                && native["runtime"] == profile.runtime,
            "empirical evidence does not bind original native model/compiler/task"
        );
        let execution = profile
            .cpu_fit
            .clone()
            .unwrap_or_default()
            .runtime_identity()?;
        ensure!(
            native["execution_identity"][0] == execution,
            "empirical native execution tier differs"
        );
        let native_score_hash: Sha256Digest = artifact["sha256"]
            .as_str()
            .context("native score hash missing")?
            .parse()?;
        let report = &summary["models"][&model];
        let mut counts = std::collections::BTreeMap::new();
        for key in [
            "episodes",
            "applied_episodes",
            "refused_episodes",
            "unique_argmax_episodes",
            "exact_tie_abstentions",
            "selected_projected_useful_count",
            "missing_support_opportunity_episodes",
            "missing_support_opportunity_selected_useful",
            "prior_complete_episodes",
        ] {
            counts.insert(
                key.to_owned(),
                report[key].as_u64().context("empirical count missing")?,
            );
        }
        ensure!(
            counts["episodes"] == 19
                && counts["applied_episodes"] + counts["refused_episodes"] == 19
                && counts["unique_argmax_episodes"] + counts["exact_tie_abstentions"]
                    == counts["applied_episodes"]
                && counts["selected_projected_useful_count"] <= counts["unique_argmax_episodes"]
                && counts["missing_support_opportunity_selected_useful"]
                    <= counts["missing_support_opportunity_episodes"]
                && counts["missing_support_opportunity_episodes"] <= 19
                && counts["prior_complete_episodes"] <= 19,
            "inconsistent empirical calibration counts"
        );
        let calibration = EmpiricalNextReadCalibration {
            schema: "airk.arbiter.empirical-next-read/1",
            task: airk_core::arbiter::Task::ContextNextRead,
            task_policy: policy,
            read_population: "trained-v2-native-default-hybrid",
            decision_rule: "unique_finite_argmax_exact_ties_abstain",
            probability_calibration_claim: false,
            empirical_summary_sha256: summary_hash,
            native_score_sha256: native_score_hash,
            source_profile_sha256: original_hash,
            counts,
            limitation: "Nineteen calibration-only mechanically projected human-support utility episodes. No sufficiency, authorization, confidence, final goal quality, resource or release qualification claim.",
        };
        let calibration_bytes = serde_json::to_vec_pretty(&calibration)?;
        let calibration_hash = Sha256Digest::from_bytes(Sha256::digest(&calibration_bytes).into());
        let policy_bytes = serde_json::to_vec_pretty(
            &json!({"schema":"airk.arbiter.next-read-evaluation-policy/1","task_policy":policy,"compiler":profile.compiler,"calibration":calibration_hash,"source_manifest":profile.manifest,"source_profile_sha256":original_hash,"empirical_summary_sha256":summary_hash,"decision_rule":"unique_finite_argmax_exact_ties_abstain","evaluation":true,"release_qualified":false}),
        )?;
        let policy_hash = Sha256Digest::from_bytes(Sha256::digest(&policy_bytes).into());
        let root = destination.join(&model);
        std::fs::create_dir(&root)?;
        profile
            .assets
            .retain(|asset| asset.role != airk_core::arbiter::AssetRole::Calibration);
        for asset in &profile.assets {
            let target = root.join(&asset.name);
            std::fs::hard_link(input.assets.join(&asset.name), &target)?;
            exact_asset_file(&target, asset)?;
        }
        for (name, role, bytes, hash) in [
            (
                "calibration-empirical-next-read.json",
                airk_core::arbiter::AssetRole::Calibration,
                &calibration_bytes,
                calibration_hash,
            ),
            (
                "context-policy-evidence.json",
                airk_core::arbiter::AssetRole::Fixture,
                &policy_bytes,
                policy_hash,
            ),
        ] {
            ensure!(
                !root.join(name).try_exists()?,
                "new metadata name collides with immutable asset"
            );
            std::fs::write(root.join(name), bytes)?;
            profile.assets.push(Asset {
                name: name.into(),
                role,
                bytes: bytes.len() as u64,
                sha256: hash,
            });
        }
        profile.profile_id.push_str("-context-eval-r1");
        profile.tasks =
            std::collections::BTreeSet::from([airk_core::arbiter::Task::ContextNextRead]);
        profile.task_policies =
            std::collections::BTreeMap::from([(airk_core::arbiter::Task::ContextNextRead, policy)]);
        profile.calibration = calibration_hash;
        profile.policy = policy_hash;
        profile.qualification = None;
        profile.qualified_targets.clear();
        profile.downloads.clear();
        profile.manifest = Sha256Digest::from_bytes([0; 32]);
        profile.validate()?;
        profile.manifest = airk_core::identity::framed_digest(
            b"airk.arbiter.evaluation-profile/1",
            &[&serde_json::to_vec(&profile)?],
        );
        let bytes = serde_json::to_vec_pretty(&profile)?;
        let path = root.join("profile.json");
        std::fs::write(&path, &bytes)?;
        let fixtures = root.join(
            input
                .fixtures
                .file_name()
                .context("source fixture name missing")?,
        );
        ensure!(
            profile
                .assets
                .iter()
                .any(|asset| asset.role == airk_core::arbiter::AssetRole::Fixture
                    && fixtures.file_name() == Some(std::ffi::OsStr::new(&asset.name))),
            "original compiler fixture absent from new closure"
        );
        qualification_inputs.push(json!({"profile":path,"assets":root,"fixtures":fixtures}));
        evaluation_inputs.push(json!({"profile":path,"assets":root,"fixtures":fixtures,"profile_sha256":format!("{:x}",Sha256::digest(&bytes))}));
    }
    ensure!(
        seen == std::collections::BTreeSet::from([
            ModelChoice::SmallIntern,
            ModelChoice::SmallStrands,
            ModelChoice::LargeIntern,
            ModelChoice::LargeKev
        ]),
        "four closed model slots required"
    );
    std::fs::write(
        destination.join("qualification-inputs.json"),
        serde_json::to_vec_pretty(&qualification_inputs)?,
    )?;
    std::fs::write(
        destination.join("evaluation-inputs.json"),
        serde_json::to_vec_pretty(&evaluation_inputs)?,
    )?;
    Ok(())
}

#[test]
fn private_evaluation_identity_excludes_self_hash_and_never_relabels_source_manifest() -> Result<()>
{
    let original: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    let original_manifest = original.manifest;
    let baseline = private_evaluation_identity(&original)?;
    let mut self_hash = original.clone();
    self_hash.manifest = Sha256Digest::from_bytes([251; 32]);
    assert_eq!(baseline, private_evaluation_identity(&self_hash)?);
    let mut policy = original.clone();
    policy.policy = Sha256Digest::from_bytes([252; 32]);
    assert_ne!(baseline, private_evaluation_identity(&policy)?);
    assert_eq!(original.manifest, original_manifest);
    assert_ne!(original.manifest, baseline);
    Ok(())
}

#[test]
#[ignore = "native-only metadata retiering; no model load, resource or quality qualification"]
fn emit_distinct_large_cpu16_private_profiles_without_relabeling_cpu8_sources() -> Result<()> {
    retier_private_profiles(16)
}

#[test]
#[ignore = "native-only metadata retiering; no CPU24 resource or quality qualification"]
fn emit_distinct_large_cpu24_private_profiles_without_relabeling_cpu16_sources() -> Result<()> {
    retier_private_profiles(24)
}

fn retier_private_profiles(tier: usize) -> Result<()> {
    ensure!(
        matches!(tier, 16 | 24),
        "closed private retier target required"
    );
    let prior = if tier == 16 { 8 } else { 16 };
    let raw = std::fs::read(
        std::env::var_os("AIRK_ARBITER_RETIER_INPUT")
            .context("missing original four CPU8 inputs")?,
    )?;
    let input_hash = Sha256Digest::from_bytes(Sha256::digest(&raw).into());
    let pin: Sha256Digest = std::env::var("AIRK_ARBITER_RETIER_INPUT_SHA256")
        .context("missing original input pin")?
        .parse()?;
    ensure!(input_hash == pin, "original CPU8 input bytes changed");
    let inputs: Vec<Input> = serde_json::from_slice(&raw)?;
    ensure!(inputs.len() == 4, "four original slots required");
    let output = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_RETIER_OUTPUT")
            .context("missing fresh private retier directory")?,
    );
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .context("repository root missing")?;
    let artifacts = repository.join("artifacts").canonicalize()?;
    ensure!(
        output.is_absolute()
            && !output.try_exists()?
            && output
                .parent()
                .context("retier parent missing")?
                .canonicalize()?
                .starts_with(artifacts),
        "fresh private artifact retier output required"
    );
    std::fs::create_dir(&output)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    let mut lineage = Vec::new();
    for input in inputs {
        let bytes = std::fs::read(&input.profile)?;
        let hash = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
        let mut profile: Profile = serde_json::from_slice(&bytes)?;
        profile.validate()?;
        ensure!(
            seen.insert(profile.model)
                && profile.qualification.is_none()
                && profile.qualified_targets.is_empty(),
            "distinct unqualified source slots required"
        );
        let original = profile.manifest;
        let path = if matches!(
            profile.model,
            ModelChoice::LargeIntern | ModelChoice::LargeKev
        ) {
            let fit = profile
                .cpu_fit
                .as_ref()
                .context("source large CPU8 fit must be explicit")?;
            ensure!(
                fit.minimum_physical_cores == prior && fit.intra_threads == prior,
                "retier requires actual immediately preceding source tier"
            );
            let policy_bytes = serde_json::to_vec_pretty(
                &json!({"schema":format!("airk.arbiter.private-cpu{tier}-policy/1"),"source_profile_sha256":hash,"source_manifest":original,"source_policy":profile.policy,"source_input_sha256":input_hash,"tier":{"minimum_physical_cores":tier,"intra_threads":tier,"inter_threads":1,"sequential":true,"spinning":false},"unchanged_floors":{"tokens":2048,"batch":1,"candidates":8,"p95_us":30000000,"large_rss_bytes":24u64<<30},"evaluation":true,"release_qualified":false,"scope":"new metadata-only execution candidate; no CPU8 identity rewrite or transferred qualification"}),
            )?;
            let model = serde_json::to_value(profile.model)?
                .as_str()
                .unwrap()
                .to_owned();
            std::fs::write(
                output.join(format!("{model}-cpu{tier}-policy.json")),
                &policy_bytes,
            )?;
            profile.cpu_fit = Some(airk_core::arbiter::CpuFit {
                minimum_physical_cores: tier,
                intra_threads: tier,
                inter_threads: 1,
                sequential: true,
                spinning: false,
            });
            profile.profile_id.push_str(&format!("-cpu{tier}-eval-r1"));
            profile.policy = Sha256Digest::from_bytes(Sha256::digest(&policy_bytes).into());
            profile.downloads.clear();
            profile.qualification = None;
            profile.qualified_targets.clear();
            profile.manifest = private_evaluation_identity(&profile)?;
            profile.validate()?;
            let path = output.join(format!("{model}-cpu{tier}-profile.json"));
            std::fs::write(&path, serde_json::to_vec_pretty(&profile)?)?;
            lineage.push(json!({"model":profile.model,"source_profile":input.profile,"source_profile_sha256":hash,"source_manifest":original,"new_profile":path,"new_manifest":profile.manifest,"new_execution":profile.cpu_fit.as_ref().unwrap().runtime_identity()?,"assets_unchanged":true,"qualification_transferred":false}));
            path
        } else {
            lineage.push(json!({"model":profile.model,"source_profile":input.profile,"source_profile_sha256":hash,"source_manifest":original,"unchanged":true}));
            input.profile.clone()
        };
        result.push(json!({"profile":path,"assets":input.assets,"fixtures":input.fixtures}));
        ensure!(
            std::fs::read(&input.profile)? == bytes,
            "original CPU8 source profile mutated"
        );
    }
    ensure!(
        seen == std::collections::BTreeSet::from([
            ModelChoice::SmallIntern,
            ModelChoice::SmallStrands,
            ModelChoice::LargeIntern,
            ModelChoice::LargeKev
        ]),
        "all four exact model slots required"
    );
    std::fs::write(
        output.join("inputs.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    std::fs::write(
        output.join("lineage.json"),
        serde_json::to_vec_pretty(
            &json!({"schema":"airk.arbiter.private-retier/1","evaluation":true,"release_qualified":false,"source_inputs_sha256":input_hash,"profiles":lineage,"scope":"Native metadata emitter only; no model execution or qualification"}),
        )?,
    )?;
    Ok(())
}
