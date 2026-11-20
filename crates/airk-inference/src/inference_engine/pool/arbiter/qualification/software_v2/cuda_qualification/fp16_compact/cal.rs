// SPDX-License-Identifier: Apache-2.0
//! Original-input census for the Windows scientific Compact CAL readout.
use super::*;
use airk_core::native_context_action::NativeContextActionRequest;
use airk_retrieval::arbiter::{compiler::Compiler, context_action};

const AUDIT: &str = "87731282ecd121eaf08d5074d1bc421c5753bc65f7126f9578b31c6f9303d661";
use airk_retrieval::arbiter::scientific_cal::{self, CompactArtifact};
const ORIGINAL: &str = scientific_cal::ORIGINAL.manifest;
const MODEL: &str = "arbiter-context-qwen3-17b";
const MAX_CERTIFICATE: u64 = 256 << 20;

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value[field]
        .as_array()
        .with_context(|| format!("CAL {field} array missing"))
}

fn pin(value: &Value) -> Result<ResourceFilePin> {
    Ok(ResourceFilePin {
        path: value["path"]
            .as_str()
            .context("CAL input path missing")?
            .into(),
        sha256: serde_json::from_value(value["sha256"].clone())?,
    })
}

fn json_file(pin: &ResourceFilePin, limit: u64) -> Result<Value> {
    Ok(serde_json::from_slice(&pinned(pin, limit)?)?)
}

fn closed_population(pre: &Value, audit: &Value, ids: &[Value]) -> Result<()> {
    ensure!(
        pre["schema"] == "airk.private-selected-Context-CAL-reference-preflight/1"
            && pre["complete"] == true
            && pre["focus_tokens"] == 1024
            && pre["launch_audit_sha256"] == AUDIT
            && pre["Focus_configuration"] == "ON"
            && pre["qualified_profiles"] == json!([])
            && pre["human_semantic_gold_count"] == 0
            && pre["Focus_profile"] == audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
            && pre["MiniLM_pipeline"] == audit["pipeline"]
            && pre["source_manifest_sha256"] == audit["inputs"]["manifest"]["sha256"]
            && pre["corpus_sha256"] == audit["inputs"]["corpus"]["sha256"]
            && audit["profiles"][MODEL] == ORIGINAL
            && pre["task_policy"] == json!(context_action::policy_digest()?),
        "foreign or incomplete scientific CAL100 certificate"
    );
    let rows = array(pre, "rows")?;
    ensure!(
        rows.len() == 100 && ids.len() == rows.len(),
        "incomplete CAL population"
    );
    let mut seen = BTreeSet::new();
    for (row, id) in rows.iter().zip(ids) {
        ensure!(
            row["id"] == *id
                && seen.insert(id.as_str().context("missing question ID")?)
                && row["split"] == "calibration"
                && row["source_group"].is_string(),
            "CAL case missing, reordered, duplicated or foreign"
        );
    }
    Ok(())
}

fn compiled_row(row: &Value, compiler: &Compiler, model: &str, manifest: &str) -> Result<Value> {
    let base = json!({"id":row["id"],"source_group":row["source_group"],"split":"calibration"});
    let mut result = base;
    let request = row.get("request").context("missing CAL request status")?;
    if request.is_null() {
        result["status"] = json!("no_native_assessment_request");
        return Ok(result);
    }
    let request: NativeContextActionRequest = serde_json::from_value(request.clone())?;
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
        serde_json::to_value(rebuilt)? == row["request"]["roster"]
            && row["question"] == request.question
            && request.scope == request.roster.scope
            && request
                .records
                .iter()
                .all(|record| record.scope == request.scope),
        "CAL original roster or source scope changed"
    );
    let mut censuses = array(row, "compiler_census")?
        .iter()
        .filter(|entry| entry["model"] == model && entry["profile"] == manifest);
    let census = censuses.next().context("original model census missing")?;
    ensure!(
        censuses.next().is_none() && census["NN_invocations"] == 0,
        "invalid original no-forward compiler census"
    );
    result["roster"] = json!(request.roster);
    let remaining = row["remaining_ms_at_observation"]
        .as_u64()
        .context("CAL remaining budget missing")?
        .min(request.remaining_ms);
    if !census["fits_remaining_allowance"]
        .as_bool()
        .context("CAL compiler allowance missing")?
        || remaining == 0
    {
        result["status"] = json!("compiler_refused_remaining_allowance");
        result["census"] = census.clone();
        return Ok(result);
    }
    let prompt = context_action::prompt(&request)?;
    let compiled = compiler.compile(&prompt.state, &prompt.question, &prompt.candidates)?;
    let value = serde_json::to_value(&compiled)?;
    ensure!(
        value == census["compiled"]
            && compiled.input_ids.len() <= request.remaining_input_tokens
            && compiled.candidate_positions.len() == request.roster.cards.len()
            && (1..=8).contains(&request.roster.cards.len()),
        "original compiled tensors/card order differ from native CAL census"
    );
    result["compiled"] = value;
    result["remaining_ms"] = json!(remaining);
    result["status"] = json!("compiled_original_input");
    Ok(result)
}

#[test]
#[ignore = "complete certified original CAL100 and pinned CPU profile; model-free"]
fn compact_cal100_original_compiled_input_census() -> Result<()> {
    let audit_pin = env_pin(
        "AIRK_CUDA_CAL_LAUNCH_AUDIT",
        "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256",
    )?;
    ensure!(
        audit_pin.sha256.to_string() == AUDIT,
        "unreviewed CAL launch epoch"
    );
    let audit = json_file(&audit_pin, 2 << 20)?;
    let pre_pin = env_pin("AIRK_CUDA_CAL_PREFLIGHT", "AIRK_CUDA_CAL_PREFLIGHT_SHA256")?;
    let pre = json_file(&pre_pin, MAX_CERTIFICATE)?;
    let census = json_file(&pin(&audit["census"])?, 2 << 20)?;
    closed_population(&pre, &audit, array(&census, "question_ids")?)?;
    let (original, profile, compiler) = original_compiler(&audit, ModelChoice::ContextQwenCompact)?;
    let rows = array(&pre, "rows")?
        .iter()
        .map(|row| compiled_row(row, &compiler, MODEL, ORIGINAL))
        .collect::<Result<Vec<_>>>()?;
    let value = json!({"schema":"airk.private.compact-cal-original-compiled-inputs/1","complete":true,
        "launch_audit":audit_pin,"preflight":pre_pin,"original_profile":original,"model":profile.model,
        "manifest":profile.manifest,"test_binary_sha256":file_hash(&std::env::current_exe()?)?,
        "NN_invocations":0,"capture_sha256":pre["capture_sha256"],"rows":rows});
    let path = output()?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer(&mut file, &value)?;
    file.sync_all()?;
    Ok(())
}

fn original_compiler(
    audit: &Value,
    model: ModelChoice,
) -> Result<(ResourceFilePin, Profile, Compiler)> {
    let (profile_file, model_name, manifest) = match model {
        ModelChoice::ContextQwenCompact => (scientific_cal::ORIGINAL.profile_file, MODEL, ORIGINAL),
        ModelChoice::ContextQwenLarge => (
            "7a444c76d135e374239cd351245458f11479438b9eae7b4e228f3b0b6f3eb192",
            "arbiter-context-qwen3-4b",
            "d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307",
        ),
        _ => anyhow::bail!("original Context compiler required"),
    };
    ensure!(
        audit["profiles"][model_name] == manifest,
        "original launch profile differs"
    );
    let master = json_file(&pin(&audit["inputs"]["inputs"])?, 2 << 20)?;
    let mut originals = master
        .as_array()
        .context("missing launch master profiles")?
        .iter()
        .filter(|item| item["profile_sha256"] == profile_file);
    let staged = originals.next().context("original CPU profile missing")?;
    ensure!(originals.next().is_none(), "repeated original profile");
    let original = ResourceFilePin {
        path: staged["profile"]
            .as_str()
            .context("missing CPU profile path")?
            .into(),
        sha256: serde_json::from_value(staged["profile_sha256"].clone())?,
    };
    let profile: Profile = serde_json::from_slice(&pinned(&original, 1 << 20)?)?;
    profile.validate()?;
    ensure!(
        profile.model == model && profile.manifest.to_string() == manifest,
        "foreign original compiler profile"
    );
    let root = Path::new(
        staged["assets"]
            .as_str()
            .context("original compiler root missing")?,
    );
    let files = Files(
        profile
            .assets
            .iter()
            .map(|asset| {
                (
                    asset.name.clone(),
                    root.join(&asset.name).to_string_lossy().into_owned(),
                )
            })
            .collect(),
    );
    let compiler = observed::compiler(&profile, &files)?;
    Ok((original, profile, compiler))
}

#[test]
#[ignore = "checked chronological CAL input and authentic original compilers; no model dispatch"]
fn chronological_original_context_compiler_row() -> Result<()> {
    let audit_pin = env_pin(
        "AIRK_CUDA_CAL_LAUNCH_AUDIT",
        "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256",
    )?;
    ensure!(audit_pin.sha256.to_string() == AUDIT, "foreign CAL audit");
    let audit = json_file(&audit_pin, 2 << 20)?;
    let input = env_pin(
        "AIRK_CUDA_CAL_CHRONOLOGICAL_INPUT",
        "AIRK_CUDA_CAL_CHRONOLOGICAL_INPUT_SHA256",
    )?;
    let projection = json_file(&input, 16 << 20)?;
    verify_chronological_projection(&audit, &projection, false)?;
    Ok(())
}

#[test]
#[ignore = "closed chronological CAL prefix for both original compilers; no model dispatch"]
fn chronological_original_context_compiler_batch() -> Result<()> {
    let audit_pin = env_pin(
        "AIRK_CUDA_CAL_LAUNCH_AUDIT",
        "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256",
    )?;
    ensure!(audit_pin.sha256.to_string() == AUDIT, "foreign CAL audit");
    let audit = json_file(&audit_pin, 2 << 20)?;
    let mut phases = None;
    for (key, model) in [
        ("COMPACT", ModelChoice::ContextQwenCompact),
        ("LARGE", ModelChoice::ContextQwenLarge),
    ] {
        let name = format!("AIRK_CUDA_CAL_CHRONOLOGICAL_{key}");
        let input = env_pin(&name, &format!("{name}_SHA256"))?;
        let projection = json_file(&input, 128 << 20)?;
        ensure!(projection["model"] == json!(model), "wrong model batch");
        if let Some(previous) = &phases {
            ensure!(*previous == projection["phases"], "model prefixes differ");
        }
        phases = Some(projection["phases"].clone());
        verify_chronological_projection(&audit, &projection, true)?;
    }
    Ok(())
}

fn verify_chronological_projection(audit: &Value, projection: &Value, batch: bool) -> Result<()> {
    let model: ModelChoice = serde_json::from_value(projection["model"].clone())?;
    let (_, profile, compiler) = original_compiler(audit, model)?;
    let census = json_file(&pin(&audit["census"])?, 2 << 20)?;
    let ids = array(&census, "question_ids")?;
    ensure!(
        ids.len() == 100
            && projection["launch_audit_sha256"] == AUDIT
            && projection["NN_invocations"] == 0
            && projection["original_manifest"] == profile.manifest.to_string(),
        "foreign chronological projection"
    );
    if !batch {
        return verify_chronological_row(projection, &projection["selected"], &compiler, ids);
    }
    let rows = array(projection, "rows")?;
    let phases = array(projection, "phases")?;
    ensure!(
        (1..=13).contains(&phases.len())
            && rows.len() == (8 * phases.len()).min(100)
            && projection["original_requests"] == rows.len(),
        "incomplete original prefix"
    );
    let mut compiled = 0;
    for (ordinal, row) in rows.iter().enumerate() {
        ensure!(
            row["ordinal"] == ordinal && ids[ordinal] == row["id"],
            "nonchronological request"
        );
        match row["status"].as_str() {
            Some("eligible") => {
                verify_chronological_row(projection, row, &compiler, ids)?;
                compiled += 1;
            }
            Some("no_native_assessment_request" | "compiler_refused_remaining_allowance") => {
                ensure!(
                    row.get("compiled").is_none(),
                    "refusal contains delivered compiled input"
                );
            }
            _ => anyhow::bail!("unknown original request status"),
        }
    }
    ensure!(
        projection["statuses"]["eligible"]
            .as_u64()
            .unwrap_or_default()
            == compiled,
        "eligible count differs"
    );
    println!(
        "{model:?} original_requests={} native_recompiled={compiled} retained_refusals={}",
        rows.len(),
        rows.len() as u64 - compiled
    );
    Ok(())
}

fn verify_chronological_row(
    projection: &Value,
    selected: &Value,
    compiler: &Compiler,
    ids: &[Value],
) -> Result<()> {
    let ordinal = usize::try_from(
        selected["ordinal"]
            .as_u64()
            .context("missing original ordinal")?,
    )?;
    ensure!(
        ids.get(ordinal) == Some(&selected["id"]),
        "wrong original CAL ordinal"
    );
    ensure!(
        selected["status"] == "eligible",
        "foreign chronological projection"
    );
    let [capture] = array(selected, "captures")?.as_slice() else {
        anyhow::bail!("one original assessment capture required");
    };
    let record = json_file(&pin(capture)?, 16 << 20)?;
    ensure!(
        record["Context_NN_dispatch"] == false
            && record["Focus_configuration"] == "ON"
            && record["actual_Context_profile"]["manifest"] == ORIGINAL,
        "foreign native capture"
    );
    let request = &record["request"];
    let row = json!({"id":selected["id"], "question":request["question"], "request":request,
        "compiler_census":record["compiler_census"], "remaining_ms_at_observation":
        record["remaining_ns_at_observation"].as_u64().context("missing native deadline")? / 1_000_000});
    let original = compiled_row(
        &row,
        compiler,
        projection["model"].as_str().context("model ID")?,
        projection["original_manifest"]
            .as_str()
            .context("original manifest")?,
    )?;
    ensure!(
        original["status"] == "compiled_original_input"
            && original["compiled"] == selected["compiled"]
            && original["roster"] == selected["roster"]
            && original["remaining_ms"] == selected["remaining_ms"]
            && request["remaining_input_tokens"] == selected["remaining_input_tokens"],
        "changed CAL tensors, roster or budget"
    );
    Ok(())
}

/// One preregistered question in a complete certified population. One native
/// charge/lease per invocation preserves the original finite operation journal.
pub(super) struct Selected {
    preflight: ResourceFilePin,
    census: ResourceFilePin,
    references: ResourceFilePin,
    candidate_profile: ResourceFilePin,
    ordinal: usize,
    captured: Value,
    expected: Value,
}

impl Selected {
    pub(super) fn load(
        candidate: &CandidateProof,
        profile: &Profile,
        profile_pin: &ResourceFilePin,
    ) -> Result<Self> {
        let artifact = CompactArtifact::registered(profile_pin.sha256, profile)?;
        ensure!(
            artifact.profile_file == scientific_cal::WINDOWS_QKV.profile_file
                && candidate.graph.sha256.to_string() == artifact.graph
                && candidate.weights.sha256.to_string() == artifact.weights,
            "unregistered Windows QKV scorer or consumed asset"
        );
        let preflight = env_pin("AIRK_CUDA_CAL_PREFLIGHT", "AIRK_CUDA_CAL_PREFLIGHT_SHA256")?;
        let census = env_pin(
            "AIRK_CUDA_CAL_ORIGINAL_COMPILED",
            "AIRK_CUDA_CAL_ORIGINAL_COMPILED_SHA256",
        )?;
        let references = env_pin(
            "AIRK_CUDA_CAL_REFERENCES",
            "AIRK_CUDA_CAL_REFERENCES_SHA256",
        )?;
        let original = json_file(&census, MAX_CERTIFICATE)?;
        let pre = json_file(&preflight, MAX_CERTIFICATE)?;
        let audit_pin = pin(&original["launch_audit"])?;
        ensure!(
            audit_pin.sha256.to_string() == AUDIT,
            "foreign CAL input launch"
        );
        let audit = json_file(&audit_pin, 2 << 20)?;
        let ids = json_file(&pin(&audit["census"])?, 2 << 20)?;
        closed_population(&pre, &audit, array(&ids, "question_ids")?)?;
        ensure!(
            original["schema"] == "airk.private.compact-cal-original-compiled-inputs/1"
                && original["complete"] == true
                && original["preflight"] == json!(preflight)
                && original["manifest"] == ORIGINAL
                && original["capture_sha256"] == pre["capture_sha256"]
                && original["NN_invocations"] == 0
                && array(&original, "rows")?.len() == 100,
            "foreign CAL original compile census"
        );
        let independent = json_file(&references, MAX_CERTIFICATE)?;
        ensure!(
            independent["schema"] == "airk.private.compact-cal-independent-references/1"
                && independent["complete"] == true
                && independent["preflight_sha256"] == json!(preflight.sha256)
                && independent["candidate_profile_sha256"] == json!(profile_pin.sha256)
                && independent["matching_reference_basis_sha256"] == json!(artifact.matching_basis)
                && independent["candidate_graph_sha256"] == json!(candidate.graph.sha256)
                && independent["candidate_weights_sha256"] == json!(candidate.weights.sha256)
                && independent["original_compiled_sha256"] == json!(census.sha256),
            "independent CAL reference archive belongs to a different candidate or input"
        );
        let prepared = array(&original, "rows")?;
        let certs = array(&pre, "rows")?;
        let mut refs = array(&independent, "rows")?.iter();
        for (compiled, cert) in prepared.iter().zip(certs) {
            ensure!(
                compiled["id"] == cert["id"]
                    && compiled["source_group"] == cert["source_group"]
                    && compiled["split"] == "calibration",
                "CAL original input order differs"
            );
            if compiled["status"] == "compiled_original_input" {
                let reference = refs.next().context("missing independent CAL row")?;
                ensure!(
                    reference["id"] == compiled["id"]
                        && reference["compiled"] == compiled["compiled"]
                        && reference["roster"] == compiled["roster"],
                    "independent CAL reference row differs"
                );
                let n = array(&compiled["roster"], "cards")?.len();
                for key in ["matching_fp16", "original_fp32"] {
                    let values: Vec<f32> = serde_json::from_value(reference[key].clone())?;
                    ensure!(
                        (1..=8).contains(&n)
                            && values.len() == n
                            && values.iter().all(|v| v.is_finite()),
                        "incomplete CAL reference vector"
                    );
                }
            } else {
                ensure!(
                    matches!(
                        compiled["status"].as_str(),
                        Some(
                            "no_native_assessment_request" | "compiler_refused_remaining_allowance"
                        )
                    ),
                    "unknown CAL row class"
                );
            }
        }
        ensure!(refs.next().is_none(), "extra independent CAL row");
        let ordinal = usize::try_from(number("AIRK_CUDA_CAL_ORDINAL")?)?;
        let expected = prepared
            .get(ordinal)
            .context("CAL ordinal outside original 100")?
            .clone();
        ensure!(
            expected["status"] == "compiled_original_input",
            "unavailable CAL case cannot dispatch"
        );
        Ok(Self {
            preflight,
            census,
            references,
            candidate_profile: profile_pin.clone(),
            ordinal,
            captured: certs[ordinal].clone(),
            expected,
        })
    }

    pub(super) fn score(
        &self,
        scorer: &Scorer,
        compiler: &Compiler,
        journal: &mut operation::Journal,
        receipt: &mut Value,
        file: &mut File,
        deadline: Instant,
    ) -> Result<()> {
        let start = Instant::now();
        let rebuilt = compiled_row(&self.captured, compiler, MODEL, ORIGINAL)?;
        ensure!(
            rebuilt == self.expected,
            "actual candidate compiler differs from independently rebuilt original input"
        );
        let input: airk_retrieval::arbiter::compiler::CompiledInput =
            serde_json::from_value(rebuilt["compiled"].clone())?;
        let compile_us = start.elapsed().as_micros();
        let remaining = rebuilt["remaining_ms"]
            .as_u64()
            .context("native remaining time missing")?;
        let expiry = deadline.min(
            start
                .checked_add(Duration::from_millis(remaining))
                .context("CAL deadline overflow")?,
        );
        ensure!(
            Instant::now() < expiry,
            "CAL compile exhausted original allowance"
        );
        receipt["numeric_forward_attempts"] = json!(1);
        receipt["scientific_cal"] = json!({"ordinal":self.ordinal,"preflight":self.preflight,"original_compiled":self.census,"independent_references":self.references,"candidate_profile":self.candidate_profile});
        flush_graph_diagnostic(file, receipt)?;
        let at = Instant::now();
        let timed = journal.observe(
            operation::Kind::Numeric,
            1,
            Some(input.input_digest),
            || scorer.score_with_deadline(&input, expiry),
        )?;
        let scores = timed.value?;
        scorer.synchronize(expiry)?;
        ensure!(
            Instant::now() < expiry
                && timed.within_deadline
                && scorer.device_settled()
                && scores.len() == array(&rebuilt["roster"], "cards")?.len()
                && scores.iter().all(|v| v.is_finite()),
            "incomplete native CAL forward or original deadline exceeded"
        );
        receipt["native_forward_returned"] = json!(true);
        receipt["rows"] = json!([{"input_digest":input.input_digest,"tokens":input.input_ids.len(),
            "native_call_elapsed_us":timed.native_call_elapsed_us,"elapsed_us":timed.elapsed_us}]);
        receipt["cal_row"] = json!({"id":rebuilt["id"],"source_group":rebuilt["source_group"],"split":"calibration","status":"raw_scores_only","compiled":rebuilt["compiled"],"roster":rebuilt["roster"],"scores":scores,"compile_us":compile_us,"inference_us":at.elapsed().as_micros()});
        flush_graph_diagnostic(file, receipt)?;
        Ok(())
    }

    pub(super) fn recheck(&self) -> Result<()> {
        for input in [&self.preflight, &self.census, &self.references] {
            pinned(input, MAX_CERTIFICATE)?;
        }
        Ok(())
    }
}
#[test]
#[ignore = "one eligible preregistered CAL100 row with complete independently authenticated source and reference archives"]
fn compact_qkv_scientific_cal_original_readout() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvScientificCal)
}

#[test]
fn compact_cal_population_rejects_reordered_and_duplicate_ordinals() -> Result<()> {
    let ids: Vec<_> = (0..100).map(|i| json!(format!("CAL-{i}"))).collect();
    let audit = json!({"profiles":{"arbiter-context-qwen3-17b":ORIGINAL,
        "arbiter-focus-qwen3-reranker-06b":"focus"},"pipeline":"minilm",
        "inputs":{"manifest":{"sha256":"manifest"},"corpus":{"sha256":"corpus"}}});
    let rows: Vec<_> = ids
        .iter()
        .map(|id| json!({"id":id,"source_group":"test","split":"calibration"}))
        .collect();
    let pre = json!({"schema":"airk.private-selected-Context-CAL-reference-preflight/1",
        "complete":true,"focus_tokens":1024,"launch_audit_sha256":AUDIT,"Focus_configuration":"ON","qualified_profiles":[],
        "human_semantic_gold_count":0,"Focus_profile":"focus","MiniLM_pipeline":"minilm","source_manifest_sha256":"manifest",
        "corpus_sha256":"corpus","task_policy":context_action::policy_digest()?,"rows":rows});
    closed_population(&pre, &audit, &ids)?;
    let mut reordered = ids.clone();
    reordered.swap(1, 7);
    assert!(closed_population(&pre, &audit, &reordered).is_err());
    let mut duplicate = pre.clone();
    duplicate["rows"][1]["id"] = ids[0].clone();
    let mut duplicate_ids = ids.clone();
    duplicate_ids[1] = ids[0].clone();
    assert!(closed_population(&duplicate, &audit, &duplicate_ids).is_err());
    let mut truncated = pre;
    truncated["rows"].as_array_mut().unwrap().pop();
    assert!(closed_population(&truncated, &audit, &ids).is_err());
    Ok(())
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod protected;
