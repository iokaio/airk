// SPDX-License-Identifier: Apache-2.0
//! FocusON selected-CAL scope. Reuses the unchanged owner fit/replay/selector.
use super::*;

fn selected_cases(preflight: &Value, raw: &Value) -> Result<(Vec<Case>, Vec<Value>)> {
    ensure!(
        preflight["schema"] == "airk.private-selected-Context-CAL-reference-preflight/1"
            && preflight["Focus_configuration"] == "ON"
            && preflight["complete"] == true
            && preflight["human_semantic_gold_count"] == 0
            && raw["complete"] == true
            && raw["task_qualified"] == false
            && raw["release_qualified"] == false,
        "selected FocusON raw/preflight required"
    );
    let pre: BTreeMap<_, _> = rows(preflight, "rows")?
        .iter()
        .map(|row| Ok((field(row, "id")?, row)))
        .collect::<Result<_>>()?;
    ensure!(pre.len() == 100, "independent CAL100 required");
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    let mut uncaptured = Vec::new();
    for row in rows(raw, "rows")? {
        let id = field(row, "id")?;
        ensure!(seen.insert(id), "duplicate raw CAL question");
        let original = pre.get(id).context("foreign raw CAL question")?;
        ensure!(
            row["split"] == "calibration" && row["source_group"] == original["source_group"],
            "CAL split/component changed"
        );
        if original["request"].is_null() {
            ensure!(
                row["status"] == "no_native_assessment_request",
                "missing native request relabeled"
            );
            uncaptured.push(json!({"id":id,"classification":"no_native_assessment_request","semantic_negative_label":false}));
            continue;
        }
        let request: NativeContextActionRequest =
            serde_json::from_value(original["request"].clone())?;
        let roster: ContextActionRoster = serde_json::from_value(row["roster"].clone())?;
        ensure!(
            serde_json::to_value(&roster)? == serde_json::to_value(&request.roster)?
                && roster.policy == context_action::policy_digest()?,
            "whole-card input/policy changed"
        );
        roster.validate(&request.records)?;
        let mut certified = BTreeSet::new();
        for certificate in rows(
            &original["reference_proxies"],
            "valid_reference_proxy_whole_cards",
        )? {
            ensure!(
                certificate["proxy"] == "generic_read_original_reference_complement",
                "unsupported semantic certificate"
            );
            let card: Sha256Digest = serde_json::from_value(certificate["card"].clone())?;
            ensure!(
                roster
                    .cards
                    .iter()
                    .any(|value| value.id == card && generic_rule(0.0).supports(value)),
                "certificate is not exact generic Read whole card"
            );
            certified.insert(card);
        }
        let status = field(row, "status")?;
        let scores = match status {
            "raw_scores_only" => Some(serde_json::from_value::<Vec<f32>>(row["scores"].clone())?),
            "compiler_refused" | "compiler_refused_remaining_allowance" => None,
            _ => anyhow::bail!("unexpected selected CAL raw status"),
        };
        result.push(Case {
            id: id.into(),
            family: field(row, "source_group")?.into(),
            roster,
            scores,
            certified,
            raw_status: status.into(),
            raw_error: row.get("error").cloned().unwrap_or(Value::Null),
        });
    }
    ensure!(
        seen.len() == 100 && seen == pre.keys().copied().collect(),
        "CAL100 rows silently omitted"
    );
    Ok((result, uncaptured))
}
#[test]
#[ignore = "explicit selected FocusON CAL100 fit; no NN or promotion; all fixed source/model pins required"]
fn fit_selected_context_generic_read_calibration() -> Result<()> {
    let input_bytes = std::fs::read(
        std::env::var_os("AIRK_CONTEXT_ACTION_FIT_INPUT").context("fit input missing")?,
    )?;
    ensure!(
        sha(&input_bytes) == std::env::var("AIRK_CONTEXT_ACTION_FIT_INPUT_SHA256")?.parse()?,
        "fit input drift"
    );
    let input: Input = serde_json::from_slice(&input_bytes)?;
    ensure!(
        input.models.len() == 2,
        "exact two selected Context models required"
    );
    let bytes = read_pinned(&input.preflight, input.preflight_sha256)?;
    let preflight: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        preflight["complete"] == true
            && preflight["qualified_profiles"] == json!([])
            && preflight["Focus_configuration"] == "ON"
            && preflight["corpus_sha256"] == std::env::var("AIRK_SELECTED_CONTEXT_CORPUS_SHA256")?,
        "foreign selected CAL scope"
    );
    let model_master: Sha256Digest =
        std::env::var("AIRK_SELECTED_CONTEXT_PROFILES_SHA256")?.parse()?;
    let mut prepared = BTreeMap::new();
    for model in input.models {
        let profile_raw = read_pinned(&model.profile, model.profile_sha256)?;
        let profile: Profile = serde_json::from_slice(&profile_raw)?;
        profile.validate()?;
        ensure!(
            matches!(
                profile.model,
                ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge
            ) && profile.qualification.is_none(),
            "unselected/qualified profile in private fit"
        );
        let raw: Value = serde_json::from_slice(&read_pinned(&model.raw, model.raw_sha256)?)?;
        ensure!(
            raw["model"] == serde_json::to_value(profile.model)?
                && raw["profile"] == serde_json::to_value(profile.manifest)?
                && raw["profile_source_sha256"] == serde_json::to_value(model.profile_sha256)?
                && raw["profiles_sha256"] == serde_json::to_value(model_master)?
                && raw["preflight_sha256"] == serde_json::to_value(input.preflight_sha256)?
                && raw["capture_sha256"] == preflight["capture_sha256"]
                && raw["task_policy"] == serde_json::to_value(context_action::policy_digest()?)?,
            "actual raw profile/preflight/capture/policy binding changed"
        );
        let (cases, uncaptured) = selected_cases(&preflight, &raw)?;
        ensure!(
            prepared
                .insert(profile.model, (model, profile, cases, uncaptured))
                .is_none(),
            "duplicate selected model"
        );
    }
    ensure!(
        prepared.keys().copied().collect::<BTreeSet<_>>()
            == BTreeSet::from([
                ModelChoice::ContextQwenCompact,
                ModelChoice::ContextQwenLarge
            ]),
        "selected two-model roster changed"
    );
    let output = PathBuf::from(
        std::env::var_os("AIRK_CONTEXT_ACTION_FIT_OUTPUT").context("fresh fit output missing")?,
    );
    ensure!(output.is_absolute(), "absolute private fit output required");
    std::fs::create_dir(&output)?;
    let mut reports = Vec::new();
    for (model, (input, original, cases, uncaptured)) in prepared {
        let directory = output.join(
            serde_json::to_value(model)?
                .as_str()
                .context("model name")?,
        );
        reports.push(fit_artifact(
            &directory, input, original, cases, uncaptured,
        )?);
    }
    std::fs::write(
        output.join("fit-report.private.json"),
        serde_json::to_vec_pretty(&json!({
        "schema":"airk.private-selected-Context-generic-read-CAL-fit/1","complete":true,"fit_input_sha256":sha(&input_bytes),
        "preflight_sha256":input.preflight_sha256,"profiles_sha256":model_master,"cal_questions":100,
        "Focus_configuration":"ON","criterion":"UNCHANGED owner fit/replay: max certified admissions, ZERO UNKNOWN admitted, greatest preserving finite cutoff; global winner, ties and whole-card semantics unchanged",
        "human_semantic_gold_count":0,"task_qualified":false,"release_qualified":false,"qualified_profiles":[],
        "unsupported":["Sufficient","Clarify","NoJustifiedRead","semantic_need_roles_relevance"],"models":reports}))?,
    )?;
    Ok(())
}

fn fit_artifact(
    directory: &Path,
    input: ModelInput,
    original: Profile,
    cases: Vec<Case>,
    uncaptured: Vec<Value>,
) -> Result<Value> {
    let model = original.model;
    let policy = context_action::policy_digest()?;
    let mut binding = original.clone();
    binding.qualification = None;
    binding.qualified_targets.clear();
    binding.downloads.clear();
    let template = calibration(&binding, policy, 0.0);
    template.validate(&binding, policy)?;
    std::fs::create_dir(directory)?;
    let report = if let Some((margin, outcomes)) = fit(&template, &cases)? {
        let mut value = template;
        value.rules = vec![generic_rule(margin)];
        let mut bytes = serde_json::to_vec_pretty(&value)?;
        bytes.push(b'\n');
        let asset_sha = sha(&bytes);
        let name = "context-action-calibration.json";
        binding
            .assets
            .retain(|asset| asset.role != AssetRole::Calibration);
        binding.assets.push(Asset {
            name: name.into(),
            role: AssetRole::Calibration,
            bytes: bytes.len() as u64,
            sha256: asset_sha,
        });
        binding.calibration = asset_sha;
        binding.profile_id = format!(
            "private-selected-generic-read-{}",
            serde_json::to_value(model)?.as_str().context("model")?
        );
        binding.manifest = Sha256Digest::from_bytes([0; 32]);
        binding.manifest = framed_digest(
            b"airk.private.context-action-evaluation-profile/1",
            &[
                input.profile_sha256.as_bytes(),
                &serde_json::to_vec(&binding)?,
            ],
        );
        binding.validate()?;
        let decoded = JointCalibration::parse_and_validate(&bytes, &binding, policy)?;
        let (positive, unknown, replayed) = replay(&decoded, &cases)?;
        ensure!(
            positive > 0 && unknown == 0 && replayed == outcomes,
            "serialized same-owner safe replay changed"
        );
        std::fs::write(directory.join(name), &bytes)?;
        std::fs::write(
            directory.join("experimental-profile.json"),
            serde_json::to_vec_pretty(&binding)?,
        )?;
        json!({"model":model,"status":"independent_reference_utility_rule_fitted","minimum_raw_margin":margin,
            "certified_admitted_questions":positive,"uncertified_admitted_questions":unknown,"calibration_sha256":asset_sha,
            "experimental_profile":binding,"asset_overlay":{"base":input.base_assets,"generated":directory},"outcomes":outcomes})
    } else {
        let (_, _, diagnostic) = replay(&template, &cases)?;
        json!({"model":model,"status":"unsupported_no_positive_safe_margin","calibration":null,"experimental_profile":null,
            "diagnostic_zero_margin_replay":diagnostic})
    };
    Ok(
        json!({"result":report,"cal_population":100,"native_rosters":cases.len(),"uncaptured_outcomes":uncaptured,
        "human_semantic_gold":false,"task_qualified":false,"release_qualified":false}),
    )
}

mod compact;
mod cutlass_study;
