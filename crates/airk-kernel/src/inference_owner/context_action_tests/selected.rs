// SPDX-License-Identifier: Apache-2.0
//! Selected two-model CAL-only raw scoring; never qualification or actuation.
use super::*;
use airk_core::arbiter::Task;
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StagedInput {
    profile: PathBuf,
    assets: PathBuf,
    fixtures: PathBuf,
    profile_sha256: Sha256Digest,
}

fn selected_models() -> BTreeSet<ModelChoice> {
    BTreeSet::from([
        ModelChoice::ContextQwenCompact,
        ModelChoice::ContextQwenLarge,
    ])
}
#[test]
fn selected_context_task_policy_is_emitted_by_native_owner() -> Result<()> {
    println!(
        "{}",
        json!({"task":"airk.context_action.v1","task_policy":context_action::policy_digest()?,"models":selected_models(),"NN_invocations":0})
    );
    Ok(())
}
fn selected_preflight() -> Result<(Value, Sha256Digest)> {
    let expected: Sha256Digest =
        std::env::var("AIRK_SELECTED_CONTEXT_PREFLIGHT_SHA256")?.parse()?;
    let raw = pinned(&env_path("AIRK_SELECTED_CONTEXT_PREFLIGHT")?, expected)?;
    let value: Value = serde_json::from_slice(&raw)?;
    ensure!(
        value["schema"] == "airk.private-selected-Context-CAL-reference-preflight/1"
            && value["complete"] == true
            && value["human_semantic_gold_count"] == 0
            && value["Focus_configuration"] == "ON"
            && value["corpus_sha256"] == std::env::var("AIRK_SELECTED_CONTEXT_CORPUS_SHA256")?
            && value["task_policy"] == context_action::policy_digest()?.to_string()
            && array(&value, "rows")?.len() == 100,
        "foreign selected CAL reference preflight"
    );
    let mut ids = BTreeSet::new();
    for row in array(&value, "rows")? {
        ensure!(
            ids.insert(text(row, "id")?) && row["split"] == "calibration",
            "duplicate/nonCAL selected episode"
        );
        if !row["request"].is_null() {
            let request: NativeContextActionRequest =
                serde_json::from_value(row["request"].clone())?;
            request.roster.validate(&request.records)?;
            let eligible = request
                .roster
                .basis
                .read_choices_scored
                .iter()
                .copied()
                .collect();
            let rebuilt = context_action::build_roster_with_eligible(
                &request.question,
                &request.scope,
                &request.records,
                &request.choices,
                &eligible,
            )?;
            ensure!(
                serde_json::to_value(rebuilt)? == row["request"]["roster"],
                "captured roster differs from actual owning constructor"
            );
            ensure!(
                request.question == text(row, "question")?
                    && request.scope == request.roster.scope
                    && request
                        .records
                        .iter()
                        .all(|record| record.scope == request.scope),
                "captured source/question isolation differs"
            );
        }
    }
    Ok((value, expected))
}
#[test]
#[ignore = "selected two-Context native CAL raw scores; Root reviewed no-outcome scope and owned resource release required"]
fn selected_context_cal_complete_readout_raw_scores() -> Result<()> {
    let (preflight, preflight_sha) = selected_preflight()?;
    let profiles_sha: Sha256Digest =
        std::env::var("AIRK_SELECTED_CONTEXT_PROFILES_SHA256")?.parse()?;
    let profiles_bytes = pinned(&env_path("AIRK_SELECTED_CONTEXT_PROFILES")?, profiles_sha)?;
    let inputs: Vec<StagedInput> = serde_json::from_slice(&profiles_bytes)?;
    ensure!(
        inputs.len() == 3,
        "exact three selected staged profiles required"
    );
    let mut profiles = BTreeMap::new();
    for input in inputs {
        let raw = std::fs::read(&input.profile)?;
        let profile: Profile = serde_json::from_slice(&raw)?;
        profile.validate()?;
        ensure!(
            sha(&raw) == input.profile_sha256,
            "staged native profile bytes changed"
        );
        if profile.model == ModelChoice::FocusQwenReranker06B {
            ensure!(
                preflight["Focus_profile"] == profile.manifest.to_string(),
                "actual captured Focus profile differs from staged master"
            );
            continue;
        }
        ensure!(
            selected_models().contains(&profile.model)
                && profile.qualification.is_none()
                && profile.tasks == BTreeSet::from([Task::ContextAction])
                && profile.task_policies.get(&Task::ContextAction)
                    == Some(&context_action::policy_digest()?),
            "selected native numeric/raw profile differs"
        );
        ensure!(
            profiles
                .insert(profile.model, (input, profile, sha(&raw)))
                .is_none(),
            "duplicate Context model"
        );
    }
    ensure!(
        profiles.keys().copied().collect::<BTreeSet<_>>() == selected_models(),
        "selected Context roster differs"
    );
    let selected: ModelChoice =
        serde_json::from_value(Value::String(std::env::var("AIRK_SELECTED_CONTEXT_MODEL")?))?;
    let (input, profile, profile_sha) = profiles
        .remove(&selected)
        .context("unselected Context model")?;
    ensure!(
        profile_sha == input.profile_sha256,
        "selected profile bytes changed from pre-outcome freeze"
    );
    let fixture_sha = sha(&std::fs::read(&input.fixtures)?);
    let output = env_path("AIRK_SELECTED_CONTEXT_RAW_OUTPUT")?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let binary_sha = sha(&std::fs::read(std::env::current_exe()?)?);
    let mut result = Vec::new();
    let report = |complete, rows: &Vec<Value>| {
        json!({"schema":"airk.arbiter.context-action.selected-cal-raw/1","complete":complete,
        "model":selected,"profile":profile.manifest,"profile_source_sha256":profile_sha,"profiles_sha256":profiles_sha,
        "preflight_sha256":preflight_sha,"capture_sha256":preflight["capture_sha256"],"corpus_sha256":preflight["corpus_sha256"],
        "task_policy":context_action::policy_digest().ok(),"test_binary_sha256":binary_sha,"task_qualified":false,"release_qualified":false,
        "human_semantic_gold_count":0,"Focus_state":"ON; actual service admission and captured combined allowance","compiler_fixture_sha256":fixture_sha,"rows":rows})
    };
    std::fs::write(&output, serde_json::to_vec_pretty(&report(false, &result))?)?;
    ensure!(
        array(&preflight, "rows")?
            .iter()
            .any(|row| !row["request"].is_null()),
        "no native requests; model must not load"
    );
    let owner = NativeInferenceOwner::admit_private_fixture(&env_path(
        "AIRK_CONTEXT_ACTION_PRIVATE_CONTROL",
    )?)?;
    owner.run(||->Result<()> {
        let census_for=|row:&Value| -> Result<Value> {
            let matches:Vec<_>=array(row,"compiler_census")?.iter().filter(|value|value["profile"]==profile.manifest.to_string()).collect();
            ensure!(matches.len()==1,"captured native per-profile compiler census missing/duplicate");
            Ok(matches[0].clone())
        };
        let any_eligible=array(&preflight,"rows")?.iter().filter(|row|!row["request"].is_null())
            .map(|row|Ok(census_for(row)?["fits_remaining_allowance"]==true)).collect::<Result<Vec<_>>>()?.into_iter().any(|value|value);
        let scorer=if any_eligible {Some(Scorer::open(ExecutionProfile::for_qualification(profile.clone())?,&Files(input.assets))?)}else{None};
        for row in array(&preflight,"rows")? {
            if row["request"].is_null() {
                result.push(json!({"id":row["id"],"split":"calibration","source_group":row["source_group"],"status":"no_native_assessment_request","scores":null}));
            } else {
                let request:NativeContextActionRequest=serde_json::from_value(row["request"].clone())?;
                let census=census_for(row)?;
                if census["fits_remaining_allowance"]!=true || row["remaining_ms_at_observation"].as_u64().unwrap_or(0)==0 {
                    result.push(json!({"id":row["id"],"split":"calibration","source_group":row["source_group"],"status":"compiler_refused_remaining_allowance","roster":request.roster,"census":census}));
                    std::fs::write(&output,serde_json::to_vec_pretty(&report(false,&result))?)?;
                    continue;
                }
                let scorer=scorer.as_ref().context("eligible row has no actual scorer")?;
                let prompt=context_action::prompt(&request)?;
                let at=Instant::now();
                let compiled=scorer.compile(&prompt.state,&prompt.question,&prompt.candidates);
                let compile_us=at.elapsed().as_micros();
                let observation=match compiled {
                    Err(error)=>json!({"id":row["id"],"split":"calibration","source_group":row["source_group"],"status":"compiler_refused","error":format!("{error:#}"),"compile_us":compile_us,"roster":request.roster}),
                    Ok(compiled)=>{
                        ensure!(serde_json::to_value(&compiled)?==census["compiled"],"actual scoring compiler differs from captured no-NN census");
                        ensure!(compiled.input_ids.len()<=request.remaining_input_tokens && compiled.candidate_positions.len()<=8,"original token/card bound exceeded");
                        let inference=Instant::now();let scores=scorer.score(&compiled)?;
                        ensure!(scores.len()==request.roster.cards.len() && scores.iter().all(|score|score.is_finite()),"invalid raw scores");
                        ensure!(at.elapsed().as_millis()<=u128::from(request.remaining_ms.min(row["remaining_ms_at_observation"].as_u64().context("actual remaining allowance missing")?)),"original native compile+inference deadline exceeded");
                        json!({"id":row["id"],"split":"calibration","source_group":row["source_group"],"status":"raw_scores_only","compile_us":compile_us,"inference_us":inference.elapsed().as_micros(),"compiled":compiled,"scores":scores,"roster":request.roster})
                    }
                };
                result.push(observation);
            }
            std::fs::write(&output,serde_json::to_vec_pretty(&report(false,&result))?)?;
        }
        Ok(())
    })??;
    std::fs::write(output, serde_json::to_vec_pretty(&report(true, &result))?)?;
    Ok(())
}
