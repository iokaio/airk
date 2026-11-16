// SPDX-License-Identifier: Apache-2.0
//! Root-approved private component equivalence. Never final fitted RAM/A6 authority.
use super::*;
use airk_core::{
    arbiter::context_action::{ContextActionRoster, calibration::JointCalibration},
    native_context_action::NativeContextActionRequest,
};
use airk_retrieval::arbiter::context_action::selected_action;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Compatibility {
    schema: String,
    original_profile: PinnedFile,
    fitted_profile: PinnedFile,
    corrected_profile_sha256: Sha256Digest,
    fitter_receipt: PinnedFile,
    fit_input: PinnedFile,
    reference_certificates: PinnedFile,
    reference_inputs: BTreeMap<String, PinnedFile>,
    raw_observations: PinnedFile,
    raw_execution: PinnedFile,
    raw_packet: PinnedFile,
    raw_release: PinnedFile,
    raw_request: PinnedFile,
    #[serde(default)]
    source_formula: Option<PinnedFile>,
}
fn compared_profiles(original: &Profile, fitted: &Profile, candidate: &Profile) -> Result<()> {
    ensure!(
        original.declared_stage() == Some(Stage::Context)
            && fitted.declared_stage() == Some(Stage::Context)
            && candidate.declared_stage() == Some(Stage::Context),
        "calibration equivalence is Context only"
    );
    let noncal = |profile: &Profile| {
        profile
            .assets
            .iter()
            .filter(|asset| asset.role != AssetRole::Calibration)
            .map(|asset| (asset.name.clone(), asset.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    ensure!(
        noncal(original) == noncal(fitted) && noncal(fitted) == noncal(candidate),
        "calibration equivalence changes neural/non-calibration assets"
    );
    let model = serde_json::to_value(fitted.model)?;
    ensure!(
        fitted.profile_id
            == format!(
                "private-selected-generic-read-{}",
                model.as_str().context("model")?
            ),
        "fitted identity not emitted by selected native fitter"
    );
    let mut normalized = original.clone();
    normalized.assets = fitted.assets.clone();
    normalized.calibration = fitted.calibration;
    normalized.profile_id = fitted.profile_id.clone();
    normalized.manifest = fitted.manifest;
    ensure!(
        serde_json::to_value(normalized)? == serde_json::to_value(fitted)?,
        "calibration equivalence changes non-calibration Profile fields"
    );
    let mut normalized = fitted.clone();
    normalized.peak_disk_bytes = candidate.peak_disk_bytes;
    normalized.manifest = candidate.manifest;
    ensure!(
        serde_json::to_value(normalized)? == serde_json::to_value(candidate)?,
        "fitted candidate changes more than approved disk metadata/derived identity"
    );
    Ok(())
}
fn replay_certificates(
    original: &Profile,
    fitted: &Profile,
    calibration: &JointCalibration,
    preflight: &Value,
    raw: &Value,
    fit: &Value,
) -> Result<()> {
    ensure!(
        preflight["schema"] == "airk.private-selected-Context-CAL-reference-preflight/1"
            && preflight["complete"] == true
            && preflight["Focus_configuration"] == "ON"
            && preflight["human_semantic_gold_count"] == 0,
        "actual independent FocusON CAL certificates absent"
    );
    ensure!(
        raw["schema"] == "airk.arbiter.context-action.selected-cal-raw/1"
            && raw["complete"] == true
            && raw["model"] == json!(original.model)
            && raw["profile"] == json!(original.manifest)
            && raw["task_qualified"] == false
            && raw["release_qualified"] == false
            && raw["human_semantic_gold_count"] == 0,
        "actual original CAL raw observations absent/foreign"
    );
    let pre = preflight["rows"]
        .as_array()
        .context("CAL certificates rows")?;
    let rows = raw["rows"].as_array().context("CAL original raw rows")?;
    ensure!(
        pre.len() == 100 && rows.len() == 100,
        "actual fixed100 CAL population absent"
    );
    let mut references = BTreeMap::new();
    for row in pre {
        let id = row["id"].as_str().context("certificate id")?;
        ensure!(
            references.insert(id, row).is_none(),
            "duplicate certificate question"
        );
    }
    let outcomes = fit["outcomes"]
        .as_array()
        .context("actual serialized native fitter outcomes absent")?;
    let mut reported = BTreeMap::new();
    for row in outcomes {
        let id = row["id"].as_str().context("fitter outcome id")?;
        ensure!(
            reported.insert(id, row).is_none(),
            "duplicate fitter outcome"
        );
    }
    let mut seen = BTreeSet::new();
    let mut captured = BTreeSet::new();
    let mut positive = 0;
    for row in rows {
        let id = row["id"].as_str().context("raw question id")?;
        ensure!(seen.insert(id), "duplicate raw CAL question");
        let certificate = references.get(id).context("foreign raw CAL question")?;
        ensure!(
            row["split"] == "calibration" && row["source_group"] == certificate["source_group"],
            "CAL split/source component changed"
        );
        let original_request = certificate
            .get("request")
            .context("required nullable native request observation absent")?;
        if original_request.is_null() {
            ensure!(
                row["status"] == "no_native_assessment_request",
                "missing actual native request relabelled"
            );
            continue;
        }
        captured.insert(id);
        let request: NativeContextActionRequest = serde_json::from_value(original_request.clone())?;
        let roster: ContextActionRoster = serde_json::from_value(row["roster"].clone())?;
        ensure!(
            serde_json::to_value(&roster)? == serde_json::to_value(&request.roster)?
                && roster.policy == fitted.task_policies[&Task::ContextAction],
            "actual whole-card roster/policy changed"
        );
        roster.validate(&request.records)?;
        let selected = match row["status"].as_str().context("raw status")? {
            "raw_scores_only" => {
                let scores = numeric_values(&row["scores"], roster.cards.len())?;
                selected_action(&scores, &roster, calibration)?
            }
            "compiler_refused" | "compiler_refused_remaining_allowance" => None,
            _ => anyhow::bail!("unknown raw CAL outcome"),
        };
        let declared = reported
            .get(id)
            .context("native fitter silently omitted captured question")?;
        ensure!(
            declared["selected_card"] == json!(selected)
                && declared["source_group"] == row["source_group"]
                && declared["semantic_negative_label"] == false,
            "native fitted replay differs from original raw whole-card selection"
        );
        ensure!(
            declared["raw_status"] == row["status"]
                && declared["raw_error"] == row.get("error").cloned().unwrap_or(Value::Null),
            "actual raw refusal/error relabelled by fitter"
        );
        if let Some(card) = selected {
            let certs = certificate["reference_proxies"]["valid_reference_proxy_whole_cards"]
                .as_array()
                .context("independent reference certificates absent")?;
            ensure!(
                certs
                    .iter()
                    .all(|value| value["proxy"] == "generic_read_original_reference_complement"),
                "unsupported semantic certificate"
            );
            ensure!(
                certs.iter().any(|value| value["card"] == json!(card))
                    && declared["selected_has_reference_certificate"] == true,
                "UNKNOWN/uncertified whole-card admission"
            );
            ensure!(
                roster.cards.iter().any(|value| value.id == card
                    && calibration.rules.iter().any(|rule| rule.supports(value))),
                "selected card outside calibrated generic Read scope"
            );
            positive += 1;
        } else {
            ensure!(
                declared.get("selected_has_reference_certificate") == Some(&Value::Null),
                "abstention relabelled certified"
            );
        }
    }
    ensure!(
        seen.len() == references.len()
            && reported.keys().copied().collect::<BTreeSet<_>>() == captured
            && positive > 0
            && fit["certified_admitted_questions"] == positive
            && fit["uncertified_admitted_questions"] == 0,
        "no certified positive or accepted UNKNOWN/count mismatch"
    );
    Ok(())
}

fn raw_owned_execution(input: &Compatibility, original: &Profile, raw: &Value) -> Result<()> {
    let execution: Value = serde_json::from_slice(&pinned(&input.raw_execution)?)?;
    let packet: Value = serde_json::from_slice(&pinned(&input.raw_packet)?)?;
    let release: Value = serde_json::from_slice(&pinned(&input.raw_release)?)?;
    let request: Value = serde_json::from_slice(&pinned(&input.raw_request)?)?;
    ensure!(
        execution["schema"] == "airk.private-selected-context-cal-raw-owned-execution/1"
            && execution["complete"] == true
            && execution.get("failure") == Some(&Value::Null)
            && execution["post_input_drift"] == json!([])
            && execution["whole_phase_deadline_not_refreshed"] == true
            && execution["whole_phase_expired"] == false
            && execution["human_semantic_gold_count"] == 0
            && execution["no_automatic_retry"] == true
            && execution["task_or_runtime_RAM_composed_A6_or_release_qualified"] == false
            && execution["model"] == json!(original.model)
            && execution["capture_sha256"] == json!(input.raw_observations.sha256),
        "actual original CAL owned execution incomplete/foreign/qualified"
    );
    natural_owned_process(&execution["owned"])?;
    ensure!(
        packet["schema"] == "airk.private-selected-context-cal-raw-packet/1"
            && packet["model"] == json!(original.model)
            && packet["profile_manifest"] == json!(original.manifest)
            && packet["profile_source_sha256"] == json!(input.original_profile.sha256)
            && packet["candidate_memory_reservation_bytes"] == original.memory_reservation_bytes
            && packet["cpu_fit"] == serde_json::to_value(&original.cpu_fit)?,
        "original CAL released model/profile/CPU scope differs"
    );
    ensure!(
        execution["packet_sha256"] == json!(input.raw_packet.sha256)
            && execution["decision_sha256"] == json!(input.raw_release.sha256)
            && release["schema"] == "airk.private-selected-context-cal-raw-decision/1"
            && release["approved"] == true
            && release["packet_sha256"] == json!(input.raw_packet.sha256)
            && release["FocusON_CAL_raw_only"] == true,
        "original taskNN release is absent/foreign, not final qualification"
    );
    let seals = execution["input_seals"]
        .as_object()
        .context("original CAL execution seals absent")?;
    ensure!(
        request["input_seals"] == execution["input_seals"]
            && request["packet_sha256"] == execution["packet_sha256"]
            && request["decision_sha256"] == execution["decision_sha256"],
        "actual native CAL request/owned closure differs"
    );
    let command = request["command"]
        .as_array()
        .context("CAL original command absent")?;
    ensure!(command.iter().any(|value|value=="--exact")
        &&command.iter().any(|value|value=="inference_owner::context_action_tests::selected::selected_context_cal_complete_readout_raw_scores"),"wrong original CAL taskNN selector");
    let binary = &packet["binary"];
    let image = &execution["owned"]["suspended_assignment"]["image"];
    let executable = command
        .first()
        .and_then(|value| value.as_str())
        .context("original CAL executable absent")?;
    ensure!(
        binary["path"] == executable
            && image["path"] == executable
            && image["sha256"] == binary["sha256"]
            && raw["test_binary_sha256"] == binary["sha256"]
            && seals.get(executable) == binary.get("sha256"),
        "actual CAL image proof differs"
    );
    let required = [
        input.original_profile.sha256,
        input.reference_certificates.sha256,
        input.raw_packet.sha256,
        input.raw_release.sha256,
    ];
    for pin in required {
        ensure!(
            seals.values().any(|value| value == &json!(pin)),
            "original CAL required input absent from settled closure"
        );
    }
    for asset in &original.assets {
        ensure!(
            seals.values().any(|value| value == &json!(asset.sha256)),
            "original CAL neural/logical asset not sealed"
        );
    }
    for pin in input.reference_inputs.values() {
        ensure!(
            seals.values().any(|value| value == &json!(pin.sha256)),
            "independent reference input not sealed by actual raw NN owner"
        );
    }
    let provenance = execution["actual_build_provenance"]
        .as_array()
        .context("actual native build provenance absent")?;
    ensure!(
        !provenance.is_empty()
            && execution["actual_build_provenance"] == packet["actual_build_provenance"],
        "actual CAL build closure absent/changed"
    );
    for row in provenance {
        let path = row["path"].as_str().context("build provenance path")?;
        ensure!(
            seals.get(path) == row.get("sha256"),
            "actual build evidence not in original closed seals"
        );
    }
    let header = execution["raw_header"]
        .as_object()
        .context("actual CAL raw header absent")?;
    ensure!(
        execution["raw_header"] == packet["raw_header"] && !header.is_empty(),
        "actual frozen raw header differs"
    );
    for (key, value) in header {
        ensure!(&raw[key] == value, "actual raw header field differs: {key}");
    }
    let limit = request["limits"]["host_committed_bytes"]
        .as_u64()
        .context("original CAL process limit absent")?;
    ensure!(
        request["limits"] == packet["limits"]
            && limit >= original.memory_reservation_bytes
            && execution["owned"]["peak_host_committed_bytes"]
                .as_u64()
                .is_some_and(|peak| peak > 0 && peak <= limit),
        "actual CAL owner resource scope exceeded"
    );
    Ok(())
}

pub(super) fn validate(
    profile: &Profile,
    candidate: &Candidate,
    decision: &RecordedDecision,
    pin: &PinnedFile,
    target: &str,
    component_targets: &BTreeMap<Sha256Digest, String>,
) -> Result<(Profile, Binding)> {
    ensure!(
        decision
            .approved_calibration_component_equivalences
            .contains(&pin.sha256),
        "calibration-only component equivalence not explicitly approved"
    );
    let input: Compatibility = serde_json::from_slice(&pinned(pin)?)?;
    ensure!(
        input.schema == "airk.private-context-calibration-component-equivalence/1"
            && input.corrected_profile_sha256 == candidate.profile.sha256,
        "foreign calibration equivalence"
    );
    let original: Profile = serde_json::from_slice(&pinned(&input.original_profile)?)?;
    let fitted: Profile = serde_json::from_slice(&pinned(&input.fitted_profile)?)?;
    original.validate()?;
    fitted.validate()?;
    compared_profiles(&original, &fitted, profile)?;
    let mut identity = fitted.clone();
    identity.manifest = Sha256Digest::from_bytes([0; 32]);
    ensure!(
        airk_core::identity::framed_digest(
            b"airk.private.context-action-evaluation-profile/1",
            &[
                input.original_profile.sha256.as_bytes(),
                &serde_json::to_vec(&identity)?
            ]
        ) == fitted.manifest,
        "actual fitted private manifest recipe differs"
    );
    if fitted.peak_disk_bytes != profile.peak_disk_bytes {
        let formula = input
            .source_formula
            .as_ref()
            .context("disk-only formula evidence required")?;
        let formula: Value = serde_json::from_slice(&pinned(formula)?)?;
        let minimum = profile
            .assets
            .iter()
            .try_fold(4 * 131072 + (16u64 << 20), |sum, asset| {
                sum.checked_add(asset.bytes)
            })
            .context("disk sum overflow")?;
        ensure!(
            formula["schema"] == "airk.private-source-disk-accounting/1"
                && profile.peak_disk_bytes >= minimum
                && profile.peak_disk_bytes > fitted.peak_disk_bytes,
            "foreign/insufficient disk-only correction"
        );
    }
    let report: Value = serde_json::from_slice(&pinned(&input.fitter_receipt)?)?;
    let fit_input: Value = serde_json::from_slice(&pinned(&input.fit_input)?)?;
    ensure!(
        report["fit_input_sha256"] == json!(input.fit_input.sha256)
            && fit_input["preflight_sha256"] == json!(input.reference_certificates.sha256),
        "actual fitter input closure differs"
    );
    let fit_models = fit_input["models"]
        .as_array()
        .context("actual fitter model inputs absent")?;
    ensure!(
        fit_models.len() == 2
            && fit_models
                .iter()
                .filter(
                    |value| value["profile_sha256"] == json!(input.original_profile.sha256)
                        && value["raw_sha256"] == json!(input.raw_observations.sha256)
                )
                .count()
                == 1,
        "actual fitter original Profile/raw input tuple absent"
    );
    let preflight: Value = serde_json::from_slice(&pinned(&input.reference_certificates)?)?;
    let raw: Value = serde_json::from_slice(&pinned(&input.raw_observations)?)?;
    let expected_inputs = BTreeSet::from(["corpus", "capture", "source_manifest", "rubric"]);
    ensure!(
        input
            .reference_inputs
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == expected_inputs,
        "independent certificate four-input closure absent"
    );
    for (name, pin) in &input.reference_inputs {
        pinned(pin)?;
        ensure!(
            preflight[format!("{name}_sha256")] == json!(pin.sha256),
            "independent certificate original input pin differs"
        );
    }
    ensure!(
        preflight["task_policy"] == json!(profile.task_policies[&Task::ContextAction])
            && !preflight["Focus_profile"].is_null()
            && !preflight["MiniLM_pipeline"].is_null()
            && preflight["qualified_profiles"] == json!([]),
        "independent certificate actual Focus/MiniLM/policy closure absent"
    );
    ensure!(
        report["schema"] == "airk.private-selected-Context-generic-read-CAL-fit/1"
            && report["complete"] == true
            && report["Focus_configuration"] == "ON"
            && report["cal_questions"] == 100
            && report["human_semantic_gold_count"] == 0
            && report["task_qualified"] == false
            && report["release_qualified"] == false
            && report["qualified_profiles"] == json!([])
            && report["preflight_sha256"] == json!(input.reference_certificates.sha256)
            && raw["preflight_sha256"] == report["preflight_sha256"]
            && raw["profiles_sha256"] == report["profiles_sha256"]
            && raw["capture_sha256"] == preflight["capture_sha256"]
            && raw["corpus_sha256"] == preflight["corpus_sha256"]
            && raw["profile_source_sha256"] == json!(input.original_profile.sha256)
            && raw["task_policy"] == json!(profile.task_policies[&Task::ContextAction]),
        "native fitter/raw/independent certificates closure differs"
    );
    let models = report["models"]
        .as_array()
        .context("actual native fitter models absent")?;
    let rows = models
        .iter()
        .filter(|row| row["result"]["model"] == json!(profile.model))
        .collect::<Vec<_>>();
    ensure!(
        models.len() == 2
            && rows.len() == 1
            && rows[0]["cal_population"] == 100
            && rows[0]["human_semantic_gold"] == false
            && rows[0]["task_qualified"] == false
            && rows[0]["release_qualified"] == false,
        "actual selected native fitter roster differs"
    );
    let result = &rows[0]["result"];
    ensure!(
        result["status"] == "independent_reference_utility_rule_fitted"
            && result["experimental_profile"] == serde_json::to_value(&fitted)?
            && result["calibration_sha256"] == json!(fitted.calibration),
        "actual fitted Profile differs from native fitter output"
    );
    validate_task_scope(profile, &candidate.files)?;
    let asset = profile
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("fitted calibration asset absent")?;
    let calibration = JointCalibration::parse_and_validate(
        &pinned(&PinnedFile {
            path: candidate
                .files
                .get(&asset.name)
                .context("fitted calibration source absent")?
                .clone(),
            bytes: asset.bytes,
            sha256: asset.sha256,
        })?,
        profile,
        profile.task_policies[&Task::ContextAction],
    )?;
    ensure!(
        calibration.rules.len() == 1
            && result["minimum_raw_margin"]
                .as_f64()
                .is_some_and(|margin| margin.is_finite()
                    && margin >= 0.0
                    && margin as f32 == calibration.rules[0].minimum_raw_margin),
        "actual fitted cutoff differs from native fitter output"
    );
    replay_certificates(&original, &fitted, &calibration, &preflight, &raw, result)?;
    raw_owned_execution(&input, &original, &raw)?;
    let raw_image: Sha256Digest = serde_json::from_value(raw["test_binary_sha256"].clone())?;
    ensure!(
        component_targets
            .get(&raw_image)
            .is_some_and(|actual| actual == target),
        "actual CAL raw image has no reviewed build target provenance"
    );
    let binding = binding(&original, input.original_profile.sha256, target)?;
    Ok((original, binding))
}

#[cfg(test)]
pub(super) fn refusal_cases() -> Result<Vec<Value>> {
    let mut original: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    original.model = ModelChoice::ContextQwenCompact;
    original.stage = Some(Stage::Context);
    original.cpu_fit = Some(CpuFit {
        minimum_physical_cores: 4,
        intra_threads: 4,
        inter_threads: 1,
        sequential: true,
        spinning: false,
    });
    let mut fitted = original.clone();
    fitted.profile_id = "private-selected-generic-read-arbiter-context-qwen3-17b".into();
    fitted.calibration = Sha256Digest::from_bytes([2; 32]);
    fitted.manifest = Sha256Digest::from_bytes([3; 32]);
    let asset = fitted
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("synthetic calibration asset")?;
    asset.sha256 = fitted.calibration;
    asset.name = "context-action-calibration.json".into();
    let mut candidate = fitted.clone();
    candidate.peak_disk_bytes += 1;
    candidate.manifest = Sha256Digest::from_bytes([4; 32]);
    compared_profiles(&original, &fitted, &candidate)?;
    let mut cases = Vec::new();
    for field in [
        "memory",
        "cpu",
        "compiler",
        "neural_asset",
        "task",
        "calibration",
    ] {
        let mut changed = fitted.clone();
        match field {
            "memory" => changed.memory_reservation_bytes += 1,
            "cpu" => {
                let fit = changed.cpu_fit.as_mut().context("synthetic CPU")?;
                fit.minimum_physical_cores = 8;
                fit.intra_threads = 8;
            }
            "compiler" => changed.compiler = Sha256Digest::from_bytes([9; 32]),
            "neural_asset" => {
                changed
                    .assets
                    .iter_mut()
                    .find(|asset| asset.role == AssetRole::Model)
                    .context("synthetic model")?
                    .sha256 = Sha256Digest::from_bytes([9; 32])
            }
            "task" => {
                changed.tasks.insert(Task::FocusRank);
            }
            _ => changed.calibration = Sha256Digest::from_bytes([8; 32]),
        }
        ensure!(
            compared_profiles(&original, &changed, &candidate).is_err(),
            "synthetic equivalence refusal failed: {field}"
        );
        cases.push(
            json!({"id":format!("calibration_equivalence_{field}_refused"),"accepted":false}),
        );
    }

    use airk_core::arbiter::context_action::{
        ContextReason, RelevanceHypothesis,
        calibration::{CALIBRATION_SCHEMA, ContextActionClass, JointRule},
    };
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/split/arbiter/native_context_action_cases.json"
    ))?;
    let mut request: NativeContextActionRequest =
        serde_json::from_value(fixture["cases"][0]["input"].clone())?;
    request.choices = vec![ReadChoice {
        id: Sha256Digest::from_bytes([91; 32]),
        operation: NativeRead::Context,
        target: "synthetic metadata context".into(),
    }];
    request.roster = airk_retrieval::arbiter::context_action::build_roster(
        &request.question,
        &request.scope,
        &request.records,
        &request.choices,
    )?;
    let rule = JointRule {
        action: ContextActionClass::Read,
        allowed_gaps: vec![Gap::Unsupported],
        allowed_reasons: vec![ContextReason::AdditionalEvidenceMayHelp],
        allowed_roles: vec![EvidenceRole::Unknown],
        allowed_relevance: vec![RelevanceHypothesis::Uncertain],
        minimum_raw_margin: 1.0,
    };
    let index = request
        .roster
        .cards
        .iter()
        .position(|card| rule.supports(card))
        .context("native generic Read fixture changed")?;
    let card = request.roster.cards[index].id;
    let calibration = JointCalibration {
        schema: CALIBRATION_SCHEMA.into(),
        class_support: None,
        model: fitted.model,
        compiler: fitted.compiler,
        task_policy: request.roster.policy,
        score_abi: fitted.abi.clone(),
        assets: BTreeMap::new(),
        rules: vec![rule],
    };
    fitted
        .task_policies
        .insert(Task::ContextAction, request.roster.policy);
    let mut scores = vec![0.0_f32; request.roster.cards.len()];
    scores[index] = 2.0;
    let mut preflight = json!({"schema":"airk.private-selected-Context-CAL-reference-preflight/1","complete":true,"Focus_configuration":"ON","human_semantic_gold_count":0,"rows":[]});
    let mut raw = json!({"schema":"airk.arbiter.context-action.selected-cal-raw/1","complete":true,"model":original.model,"profile":original.manifest,"task_qualified":false,"release_qualified":false,"human_semantic_gold_count":0,"rows":[]});
    let mut fit = json!({"outcomes":[],"certified_admitted_questions":100,"uncertified_admitted_questions":0});
    for id in 0..100 {
        let id = format!("synthetic-{id}");
        preflight["rows"].as_array_mut().context("test certificates")?.push(json!({"id":id,"source_group":"synthetic","request":request,"reference_proxies":{"valid_reference_proxy_whole_cards":[{"proxy":"generic_read_original_reference_complement","card":card}]}}));
        raw["rows"].as_array_mut().context("test raw")?.push(json!({"id":id,"source_group":"synthetic","split":"calibration","status":"raw_scores_only","roster":request.roster,"scores":scores}));
        fit["outcomes"].as_array_mut().context("test outcomes")?.push(json!({"id":id,"source_group":"synthetic","raw_status":"raw_scores_only","raw_error":null,"selected_card":card,"selected_has_reference_certificate":true,"semantic_negative_label":false}));
    }
    replay_certificates(&original, &fitted, &calibration, &preflight, &raw, &fit)?;
    let mut unknown = preflight.clone();
    unknown["rows"][0]["reference_proxies"]["valid_reference_proxy_whole_cards"] = json!([]);
    ensure!(
        replay_certificates(&original, &fitted, &calibration, &unknown, &raw, &fit).is_err(),
        "uncertified UNKNOWN was accepted"
    );
    cases.push(json!({"id":"calibration_unknown_admission_refused","accepted":false}));
    let mut changed = raw.clone();
    changed["rows"].as_array_mut().context("test duplicate")?[1]["id"] = json!("synthetic-0");
    ensure!(
        replay_certificates(&original, &fitted, &calibration, &preflight, &changed, &fit).is_err(),
        "duplicate CAL question accepted"
    );
    cases.push(json!({"id":"calibration_population_refused","accepted":false}));
    let mut zero_raw = raw;
    let mut zero_fit = fit;
    for row in zero_raw["rows"].as_array_mut().context("zero raw")? {
        row["scores"] = json!(vec![0.0; request.roster.cards.len()]);
    }
    for row in zero_fit["outcomes"]
        .as_array_mut()
        .context("zero outcomes")?
    {
        row["selected_card"] = Value::Null;
        row["selected_has_reference_certificate"] = Value::Null;
    }
    zero_fit["certified_admitted_questions"] = json!(0);
    ensure!(
        replay_certificates(
            &original,
            &fitted,
            &calibration,
            &preflight,
            &zero_raw,
            &zero_fit
        )
        .is_err(),
        "zero positive CAL accepted"
    );
    cases.push(json!({"id":"calibration_zero_positive_refused","accepted":false}));
    Ok(cases)
}
