// SPDX-License-Identifier: Apache-2.0
//! CAL-only operational reference-proxy fit; never product/task qualification.
use airk_core::{
    arbiter::context_action::{
        ContextActionRoster, ContextReason, RelevanceHypothesis,
        calibration::{
            CALIBRATION_SCHEMA, ContextActionClass, JointAssetBinding, JointCalibration, JointRule,
        },
    },
    arbiter::{Asset, AssetRole, EvidenceRole, Gap, ModelChoice, Profile, ReadChoice, Task},
    identity::{Sha256Digest, framed_digest},
    native_context_action::NativeContextActionRequest,
};
use airk_retrieval::arbiter::context_action::{self, selected_action};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

fn sha(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn read_pinned(path: &Path, expected: Sha256Digest) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    ensure!(sha(&bytes) == expected, "CAL input bytes changed");
    Ok(bytes)
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("missing {key}"))
}
fn rows<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value[key]
        .as_array()
        .with_context(|| format!("missing {key}"))
}
fn generic_rule(margin: f32) -> JointRule {
    JointRule {
        action: ContextActionClass::Read,
        allowed_gaps: vec![Gap::Unsupported],
        allowed_reasons: vec![ContextReason::AdditionalEvidenceMayHelp],
        allowed_roles: vec![EvidenceRole::Unknown],
        allowed_relevance: vec![RelevanceHypothesis::Uncertain],
        minimum_raw_margin: margin,
    }
}
fn calibration(profile: &Profile, policy: Sha256Digest, margin: f32) -> JointCalibration {
    JointCalibration {
        schema: CALIBRATION_SCHEMA.into(),
        model: profile.model,
        compiler: profile.compiler,
        task_policy: policy,
        score_abi: profile.abi.clone(),
        assets: profile
            .assets
            .iter()
            .filter(|asset| {
                matches!(
                    asset.role,
                    AssetRole::Model | AssetRole::ExternalWeights | AssetRole::Tokenizer
                )
            })
            .map(|asset| {
                (
                    asset.name.clone(),
                    JointAssetBinding {
                        role: asset.role.clone(),
                        sha256: asset.sha256,
                    },
                )
            })
            .collect(),
        rules: vec![generic_rule(margin)],
    }
}
#[derive(Clone)]
struct Case {
    id: String,
    family: String,
    roster: ContextActionRoster,
    scores: Option<Vec<f32>>,
    certified: BTreeSet<Sha256Digest>,
    raw_status: String,
    raw_error: Value,
}
fn replay(calibration: &JointCalibration, cases: &[Case]) -> Result<(usize, usize, Vec<Value>)> {
    let mut certified = 0;
    let mut uncertified = 0;
    let mut outcomes = Vec::new();
    for case in cases {
        let selected = case
            .scores
            .as_ref()
            .map(|scores| selected_action(scores, &case.roster, calibration))
            .transpose()?
            .flatten();
        let witnessed = selected.map(|card| case.certified.contains(&card));
        if witnessed == Some(true) {
            certified += 1;
        }
        if witnessed == Some(false) {
            uncertified += 1;
        }
        outcomes.push(json!({"id":case.id,"source_group":case.family,"raw_status":case.raw_status,
            "raw_error":case.raw_error,"selected_card":selected,"selected_has_reference_certificate":witnessed,
            "classification":match witnessed{Some(true)=>"admitted_reference_complement_proxy",Some(false)=>"admitted_uncertified_unknown",None=>"abstained_or_compiler_refused"},
            "semantic_negative_label":false}));
    }
    Ok((certified, uncertified, outcomes))
}
fn fit(template: &JointCalibration, cases: &[Case]) -> Result<Option<(f32, Vec<Value>)>> {
    let mut boundaries = vec![0.0_f32];
    for case in cases {
        if let Some(scores) = &case.scores {
            // Enumerate numeric cutoff boundaries, not another top-selection
            // implementation. Every decision below goes through the owner.
            for left in scores {
                for right in scores {
                    let difference = left - right;
                    if difference.is_finite() && difference >= 0.0 {
                        let difference = if difference == 0.0 { 0.0 } else { difference };
                        boundaries.push(difference);
                        if difference.next_up().is_finite() {
                            boundaries.push(difference.next_up());
                        }
                    }
                }
            }
        }
    }
    boundaries.sort_by(f32::total_cmp);
    boundaries.dedup_by(|a, b| *a == *b);
    let mut best: Option<(usize, f32, Vec<Value>)> = None;
    for margin in boundaries {
        let mut candidate = template.clone();
        candidate.rules = vec![generic_rule(margin)];
        let (positive, unknown, outcomes) = replay(&candidate, cases)?;
        if positive > 0
            && unknown == 0
            && best.as_ref().is_none_or(|(count, cutoff, _)| {
                positive > *count || positive == *count && margin > *cutoff
            })
        {
            best = Some((positive, margin, outcomes));
        }
    }
    Ok(best.map(|(_, margin, outcomes)| (margin, outcomes)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    preflight: PathBuf,
    preflight_sha256: Sha256Digest,
    models: Vec<ModelInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelInput {
    profile: PathBuf,
    profile_sha256: Sha256Digest,
    raw: PathBuf,
    raw_sha256: Sha256Digest,
    base_assets: PathBuf,
}
fn approved(model: ModelChoice) -> Result<Sha256Digest> {
    match model {
        ModelChoice::SmallIntern => {
            "feff8b9d12f179ab9d719134bbedf2fe5d6a789d68761001130283ca082c9044"
        }
        ModelChoice::SmallStrands => {
            "a4e7a8698709c8a5746cdea925d13579bbbec60c21223dfaccde93937c340b13"
        }
        ModelChoice::LargeIntern => {
            "d10876d7b5b3da240fac17228985714f24aabbb6c53df35a8398b28fa04ed7bc"
        }
        ModelChoice::LargeKev => "83e19e51906b2a7024af2432f6815ab6b032cd5945cf50ac5496f149c1a9f5e9",
        ModelChoice::FocusQwenReranker06B
        | ModelChoice::ContextQwenCompact
        | ModelChoice::ContextQwenLarge => anyhow::bail!(
            "selected Qwen stages are outside the historical four-model calibration artifacts"
        ),
    }
    .parse()
}

#[test]
fn historical_approved_artifacts_refuse_every_selected_qwen_stage() -> Result<()> {
    for model in [
        ModelChoice::FocusQwenReranker06B,
        ModelChoice::ContextQwenCompact,
        ModelChoice::ContextQwenLarge,
    ] {
        assert!(approved(model).is_err());
    }
    assert_eq!(
        approved(ModelChoice::SmallIntern)?.to_string(),
        "feff8b9d12f179ab9d719134bbedf2fe5d6a789d68761001130283ca082c9044"
    );
    Ok(())
}
fn cases(preflight: &Value, raw: &Value) -> Result<Vec<Case>> {
    ensure!(
        raw["complete"] == true
            && raw["task_qualified"] == false
            && raw["release_qualified"] == false,
        "all four complete RAW-only artifacts required before fit"
    );
    let pre: BTreeMap<_, _> = rows(preflight, "rows")?
        .iter()
        .map(|row| Ok((field(row, "id")?, row)))
        .collect::<Result<_>>()?;
    let mut seen = BTreeSet::new();
    let mut train_families = BTreeSet::new();
    let mut cal_families = BTreeSet::new();
    let mut result = Vec::new();
    let mut train = 0;
    for row in rows(raw, "rows")? {
        let id = field(row, "id")?;
        ensure!(seen.insert(id), "duplicate raw question");
        let original = pre.get(id).context("nonpublisher/FINAL raw question")?;
        ensure!(
            row["split"] == original["split"] && row["source_group"] == original["source_group"],
            "raw split/family changed"
        );
        let family = field(row, "source_group")?;
        let request: NativeContextActionRequest =
            serde_json::from_value(original["request"].clone())?;
        let roster: ContextActionRoster = serde_json::from_value(row["roster"].clone())?;
        ensure!(
            serde_json::to_value(&request.roster)? == serde_json::to_value(&roster)?,
            "raw whole-card basis changed"
        );
        roster.validate(&request.records)?;
        ensure!(
            roster.policy == context_action::policy_digest()?,
            "task policy changed"
        );
        match field(row, "split")? {
            "train" => {
                train += 1;
                train_families.insert(family.to_owned());
                continue;
            }
            "calibration" => {
                cal_families.insert(family.to_owned());
            }
            _ => anyhow::bail!("only frozen TRAIN/CAL admitted"),
        }
        let mut certified = BTreeSet::new();
        for certificate in rows(
            &original["reference_proxies"],
            "valid_reference_proxy_whole_cards",
        )? {
            ensure!(
                certificate["proxy"]
                    != "one_original_alternative_reference_complete_with_all_document_useful_units",
                "Sufficient remains unsupported in this corpus"
            );
            if certificate["proxy"] == "generic_read_original_reference_complement" {
                let id: Sha256Digest = serde_json::from_value(certificate["card"].clone())?;
                ensure!(
                    roster
                        .cards
                        .iter()
                        .any(|card| card.id == id && generic_rule(0.0).supports(card)),
                    "certificate is not exact generic Read whole card"
                );
                certified.insert(id);
            }
        }
        let status = field(row, "status")?;
        let scores = match status {
            "raw_scores_only" => Some(serde_json::from_value::<Vec<f32>>(row["scores"].clone())?),
            "compiler_refused" => None,
            _ => anyhow::bail!("unexpected CAL raw status"),
        };
        result.push(Case {
            id: id.into(),
            family: family.into(),
            roster,
            scores,
            certified,
            raw_status: status.into(),
            raw_error: row.get("error").cloned().unwrap_or(Value::Null),
        });
    }
    ensure!(
        seen == pre.keys().copied().collect()
            && seen.len() == 126
            && train == 107
            && result.len() == 19
            && train_families.len() == 36
            && cal_families.len() == 9
            && train_families.is_disjoint(&cal_families),
        "frozen 126/107/19/36/9 population changed"
    );
    Ok(result)
}

#[test]
#[ignore = "explicit native CAL-only proxy fit after all four raw artifacts; no NN or task qualification"]
fn fit_and_replay_context_action_generic_read_calibration() -> Result<()> {
    let input_path =
        std::env::var_os("AIRK_CONTEXT_ACTION_FIT_INPUT").context("missing native fit input")?;
    let input_bytes = std::fs::read(input_path)?;
    ensure!(
        sha(&input_bytes) == std::env::var("AIRK_CONTEXT_ACTION_FIT_INPUT_SHA256")?.parse()?,
        "fit input changed"
    );
    let input: Input = serde_json::from_slice(&input_bytes)?;
    ensure!(input.models.len() == 4, "all four raw models required");
    let pre_bytes = read_pinned(&input.preflight, input.preflight_sha256)?;
    ensure!(
        input.preflight_sha256.to_string()
            == "bef6e957646b68d38ddbe3b3dccb7fc3fe4722479d330e87c89e11a866da60ff",
        "foreign preflight population"
    );
    let preflight: Value = serde_json::from_slice(&pre_bytes)?;
    ensure!(
        preflight["complete"] == true && preflight["qualified_profiles"] == json!([]),
        "only reviewed operational proxy preflight admitted"
    );
    let mut prepared = BTreeMap::new();
    for input in input.models {
        let profile_bytes = read_pinned(&input.profile, input.profile_sha256)?;
        let profile: Profile = serde_json::from_slice(&profile_bytes)?;
        profile.validate()?;
        ensure!(
            input.profile_sha256 == approved(profile.model)? && profile.qualification.is_none(),
            "historical unqualified profile binding changed"
        );
        let raw_bytes = read_pinned(&input.raw, input.raw_sha256)?;
        let raw: Value = serde_json::from_slice(&raw_bytes)?;
        ensure!(
            raw["model"] == serde_json::to_value(profile.model)?
                && raw["profile"] == serde_json::to_value(profile.manifest)?
                && raw["profile_source_sha256"] == serde_json::to_value(input.profile_sha256)?
                && raw["task_policy"] == serde_json::to_value(context_action::policy_digest()?)?
                && raw["capture_sha256"] == preflight["capture_sha256"],
            "raw exact model/profile/policy/capture binding changed"
        );
        let observations = cases(&preflight, &raw)?;
        ensure!(
            prepared
                .insert(profile.model, (input, profile, observations))
                .is_none(),
            "duplicate raw model"
        );
    }
    ensure!(
        prepared.keys().copied().collect::<BTreeSet<_>>()
            == BTreeSet::from([
                ModelChoice::SmallIntern,
                ModelChoice::SmallStrands,
                ModelChoice::LargeIntern,
                ModelChoice::LargeKev
            ]),
        "four exact readouts required"
    );
    let output = PathBuf::from(
        std::env::var_os("AIRK_CONTEXT_ACTION_FIT_OUTPUT").context("missing fresh fit output")?,
    );
    ensure!(
        output.is_absolute(),
        "absolute fresh private output required"
    );
    std::fs::create_dir(&output)?;
    let mut reports = Vec::new();
    for (model, (input, original, observations)) in prepared {
        let policy = context_action::policy_digest()?;
        let mut binding = original.clone();
        binding.tasks = BTreeSet::from([Task::ContextAction]);
        binding.task_policies = BTreeMap::from([(Task::ContextAction, policy)]);
        binding.qualification = None;
        binding.qualified_targets.clear();
        binding.downloads.clear();
        let template = calibration(&binding, policy, 0.0);
        template.validate(&binding, policy)?;
        let slug = serde_json::to_value(model)?
            .as_str()
            .context("model name")?
            .to_owned();
        let directory = output.join(slug);
        std::fs::create_dir(&directory)?;
        let fitted = fit(&template, &observations)?;
        let report = if let Some((margin, outcomes)) = fitted {
            let mut value = template;
            value.rules = vec![generic_rule(margin)];
            let mut bytes = serde_json::to_vec_pretty(&value)?;
            bytes.push(b'\n');
            let asset_sha = sha(&bytes);
            let asset_name = "context-action-calibration.json";
            binding
                .assets
                .retain(|asset| asset.role != AssetRole::Calibration);
            binding.assets.push(Asset {
                name: asset_name.into(),
                role: AssetRole::Calibration,
                bytes: bytes.len() as u64,
                sha256: asset_sha,
            });
            binding.calibration = asset_sha;
            binding.profile_id = format!(
                "private-proxy-cal-only-{}",
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
            let (certified, unknown, replayed) = replay(&decoded, &observations)?;
            ensure!(
                certified > 0 && unknown == 0 && replayed == outcomes,
                "serialized native replay changed"
            );
            std::fs::write(directory.join(asset_name), &bytes)?;
            std::fs::write(
                directory.join("experimental-profile.json"),
                serde_json::to_vec_pretty(&binding)?,
            )?;
            json!({"model":model,"status":"developer_proxy_rule_fitted","minimum_raw_margin":margin,
                "certified_admitted_questions":certified,"uncertified_admitted_questions":unknown,
                "calibration_sha256":asset_sha,"experimental_profile":binding,
                "asset_overlay":{"base":input.base_assets,"generated":directory},"outcomes":outcomes})
        } else {
            let (_, _, diagnostic) = replay(&template, &observations)?;
            let outcomes:Vec<_> = diagnostic.into_iter().map(|counterfactual|json!({
                "id":counterfactual["id"],"source_group":counterfactual["source_group"],
                "selected_card":null,"selected_has_reference_certificate":null,
                "classification":"abstained_no_eligible_asset","semantic_negative_label":false,
                "diagnostic_zero_margin_replay":counterfactual
            })).collect();
            json!({"model":model,"status":"unsupported_no_positive_safe_margin","calibration":null,
                "experimental_profile":null,"outcomes":outcomes})
        };
        reports.push(report);
    }
    std::fs::write(
        output.join("fit-report.private.json"),
        serde_json::to_vec_pretty(&json!({
            "schema":"airk.arbiter.context-action.proxy-calibration-report/1","complete":true,
            "fit_input_sha256":sha(&input_bytes),"preflight_sha256":input.preflight_sha256,
            "task_policy":context_action::policy_digest()?,"cal_questions":19,"cal_families":9,
            "criterion":"max certified CAL questions with zero uncertified proxy admissions; greatest cutoff preserving that set",
            "task_qualified":false,"release_qualified":false,"qualified_profiles":[],
            "unsupported":["sufficient_zero_positive","clarify","no_justified_read","semantic_roles","semantic_gaps"],
            "limits":"Operational reference proxy only; uncertified is unknown, not semantic false. A complement need not be necessary/optimal if another original alternative is already complete. No FINAL/goal/product activation.",
            "models":reports
        }))?,
    )?;
    Ok(())
}

fn test_fixture() -> Result<(Profile, ContextActionRoster, JointCalibration)> {
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    let raw: Value = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/split/arbiter/context_action_cases.json"
    ))?;
    let response: airk_core::arbiter::context_action::ContextResponseV2 =
        serde_json::from_value(raw["cases"][0]["input"].clone())?;
    let choices = vec![ReadChoice {
        id: Sha256Digest::from_bytes([91; 32]),
        operation: airk_core::arbiter::NativeRead::Context,
        target: "policy.md:1-33".into(),
    }];
    let roster = context_action::build_roster(
        "source-only fixture",
        &response.assessment.scope,
        &response.records,
        &choices,
    )?;
    profile.tasks = BTreeSet::from([Task::ContextAction]);
    profile.task_policies = BTreeMap::from([(Task::ContextAction, roster.policy)]);
    let calibration = calibration(&profile, roster.policy, 0.0);
    calibration.validate(&profile, roster.policy)?;
    Ok((profile, roster, calibration))
}
fn test_case(roster: ContextActionRoster, scores: Vec<f32>, certified: bool) -> Case {
    let card = roster.cards[0].id;
    Case {
        id: "independent-literal".into(),
        family: "literal-family".into(),
        roster,
        scores: Some(scores),
        certified: if certified {
            BTreeSet::from([card])
        } else {
            BTreeSet::new()
        },
        raw_status: "raw_scores_only".into(),
        raw_error: Value::Null,
    }
}
#[test]
fn native_proxy_fit_uses_owner_global_top_ties_and_whole_class() -> Result<()> {
    let (_, roster, cal) = test_fixture()?;
    let mut scores = vec![-1.0; roster.cards.len()];
    scores[0] = 4.0;
    scores[1] = 1.0;
    assert_eq!(
        replay(&cal, &[test_case(roster.clone(), scores.clone(), true)])?.0,
        1
    );
    scores[1] = 5.0;
    assert_eq!(
        replay(&cal, &[test_case(roster.clone(), scores.clone(), true)])?.0,
        0,
        "unsupported global top must not fall back"
    );
    scores[0] = 5.0;
    assert_eq!(
        replay(&cal, &[test_case(roster.clone(), scores.clone(), true)])?.0,
        0,
        "tied top abstains"
    );
    scores[0] = 6.0;
    let mut foreign = roster;
    foreign.cards[0].assessments[0].role = EvidenceRole::Definition;
    assert_eq!(
        replay(&cal, &[test_case(foreign, scores, true)])?.0,
        0,
        "all whole-card roles must match"
    );
    Ok(())
}
#[test]
fn native_proxy_fit_boundary_unknown_and_no_accept_none_asset() -> Result<()> {
    let (_, roster, cal) = test_fixture()?;
    let mut good = vec![-1.0; roster.cards.len()];
    good[0] = 4.0;
    good[1] = 1.0;
    let mut unknown = good.clone();
    unknown[0] = 3.0;
    let cases = vec![
        test_case(roster.clone(), good, true),
        test_case(roster.clone(), unknown.clone(), false),
    ];
    let (margin, outcomes) = fit(&cal, &cases)?.context("positive safe cutoff")?;
    assert_eq!(margin, 3.0);
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[1]["semantic_negative_label"], false);
    assert!(fit(&cal, &[test_case(roster.clone(), unknown, false)])?.is_none());
    let mut nonfinite = vec![0.0; roster.cards.len()];
    nonfinite[0] = f32::NAN;
    assert!(fit(&cal, &[test_case(roster, nonfinite, true)]).is_err());
    Ok(())
}
#[test]
fn native_proxy_fit_serialized_asset_binds_complete_model_map_policy_and_abi() -> Result<()> {
    let (profile, _, cal) = test_fixture()?;
    let raw = serde_json::to_vec(&cal)?;
    JointCalibration::parse_and_validate(&raw, &profile, cal.task_policy)?;
    let mut changed = cal.clone();
    changed.assets.clear();
    assert!(
        JointCalibration::parse_and_validate(
            &serde_json::to_vec(&changed)?,
            &profile,
            cal.task_policy
        )
        .is_err()
    );
    let mut changed = cal.clone();
    changed.compiler = Sha256Digest::from_bytes([93; 32]);
    assert!(
        JointCalibration::parse_and_validate(
            &serde_json::to_vec(&changed)?,
            &profile,
            cal.task_policy
        )
        .is_err()
    );
    let mut changed = cal;
    changed.score_abi.output.name.push_str("_foreign");
    assert!(
        JointCalibration::parse_and_validate(
            &serde_json::to_vec(&changed)?,
            &profile,
            changed.task_policy
        )
        .is_err()
    );
    Ok(())
}

mod selected;
