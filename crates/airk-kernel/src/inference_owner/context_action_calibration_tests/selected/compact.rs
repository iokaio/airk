// SPDX-License-Identifier: Apache-2.0
//! Private per-artifact CAL fit; the full selected two-model gate is separate.
use super::*;

use airk_retrieval::arbiter::scientific_cal::{self, CompactArtifact};
const ORIGINAL_MANIFEST: &str = scientific_cal::ORIGINAL.manifest;
const CORPUS: &str = "c51a9aa5985f2c505a2839d7b426d1614390b35639b89d9aae93681878d6ffe4";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    path: PathBuf,
    sha256: Sha256Digest,
}

impl Pin {
    fn json(&self) -> Result<Value> {
        Ok(serde_json::from_slice(&read_pinned(
            &self.path,
            self.sha256,
        )?)?)
    }
}

fn original_census(row: &Value) -> Result<&Value> {
    let mut matches = rows(row, "compiler_census")?.iter().filter(|entry| {
        entry["profile"] == ORIGINAL_MANIFEST && entry["model"] == "arbiter-context-qwen3-17b"
    });
    let entry = matches
        .next()
        .context("missing captured Compact compiler census")?;
    ensure!(
        matches.next().is_none() && entry["NN_invocations"] == 0,
        "foreign compiler census"
    );
    Ok(entry)
}

fn aligned(row: &Value, raw: &Value) -> Result<()> {
    ensure!(
        row["id"].is_string()
            && raw["id"] == row["id"]
            && raw["source_group"] == row["source_group"]
            && raw["split"] == "calibration",
        "raw case order or source differs"
    );
    let request = &row["request"];
    if request.is_null() {
        ensure!(
            raw["status"] == "no_native_assessment_request" && raw["scores"].is_null(),
            "unavailable request relabeled"
        );
        return Ok(());
    }
    let census = original_census(row)?;
    let remaining = row["remaining_ms_at_observation"]
        .as_u64()
        .context("remaining allowance missing")?
        .min(
            request["remaining_ms"]
                .as_u64()
                .context("request deadline missing")?,
        );
    ensure!(raw["roster"] == request["roster"], "raw roster differs");
    if !census["fits_remaining_allowance"]
        .as_bool()
        .context("invalid captured allowance")?
        || remaining == 0
    {
        ensure!(
            raw["status"] == "compiler_refused_remaining_allowance"
                && raw["scores"].is_null()
                && raw["census"] == *census,
            "original compiler refusal changed"
        );
        return Ok(());
    }
    if raw["status"] == "compiler_refused" {
        ensure!(
            raw["error"].as_str().is_some_and(|msg| !msg.is_empty()) && raw["scores"].is_null(),
            "compiler refusal lacks original error"
        );
        return Ok(());
    }
    let scores: Vec<f32> = serde_json::from_value(raw["scores"].clone())?;
    let count = rows(&request["roster"], "cards")?.len();
    let compile = raw["compile_us"].as_u64().context("missing compile time")?;
    let inference = raw["inference_us"]
        .as_u64()
        .context("missing inference time")?;
    ensure!(
        raw["status"] == "raw_scores_only"
            && raw["compiled"].is_object()
            && raw["compiled"] == census["compiled"]
            && (1..=8).contains(&count)
            && scores.len() == count
            && scores.iter().all(|s| s.is_finite())
            && compile
                .checked_add(inference)
                .context("invalid scoring time")?
                / 1000
                <= remaining,
        "exact captured compiler, live score cardinality, or original deadline differs"
    );
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScientificInput {
    preflight: Pin,
    launch_audit: Pin,
    model: ModelInput,
    producer_binary: Pin,
    scoring_closure: Pin,
}

fn focus_epoch(input: &ScientificInput, pre: &Value) -> Result<Sha256Digest> {
    let audit = input.launch_audit.json()?;
    let master = &audit["inputs"]["inputs"];
    let master_sha: Sha256Digest = serde_json::from_value(master["sha256"].clone())?;
    let descriptors: Vec<Value> =
        serde_json::from_slice(&read_pinned(Path::new(field(master, "path")?), master_sha)?)?;
    ensure!(
        pre["launch_audit_sha256"] == json!(input.launch_audit.sha256)
            && pre["Focus_profile"] == audit["profiles"]["arbiter-focus-qwen3-reranker-06b"]
            && audit["inputs"]["corpus"]["sha256"] == CORPUS,
        "capture provenance differs from audit"
    );
    let mut found = false;
    for row in descriptors {
        let digest: Sha256Digest = serde_json::from_value(row["profile_sha256"].clone())?;
        let profile: Profile =
            serde_json::from_slice(&read_pinned(Path::new(field(&row, "profile")?), digest)?)?;
        if profile.model == ModelChoice::FocusQwenReranker06B {
            profile.validate()?;
            ensure!(
                !found && pre["Focus_profile"] == json!(profile.manifest),
                "ambiguous Focus epoch"
            );
            let compiler = profile
                .assets
                .iter()
                .find(|asset| asset.role == AssetRole::Compiler)
                .context("Focus compiler missing")?;
            ensure!(
                compiler.sha256 == profile.compiler && compiler.name == "compiler.json",
                "wrong compiler asset"
            );
            let definition: Value = serde_json::from_slice(&read_pinned(
                &Path::new(field(&row, "assets")?).join(&compiler.name),
                compiler.sha256,
            )?)?;
            ensure!(
                definition["max_tokens"] == 1024,
                "active static-1024 Focus compiler required"
            );
            found = true;
        }
    }
    ensure!(found, "missing Focus epoch");
    Ok(master_sha)
}

fn verify_producer(
    evidence: &Value,
    raw: &Value,
    input: &ScientificInput,
    artifact: &CompactArtifact,
) -> Result<()> {
    ensure!(
        evidence["schema"] == "airk.private.compact-cal-raw-closure/1"
            && evidence["complete"] == true
            && evidence["raw_sha256"] == json!(input.model.raw_sha256)
            && evidence["profile_sha256"] == json!(input.model.profile_sha256)
            && evidence["preflight_sha256"] == json!(input.preflight.sha256)
            && evidence["test_binary_sha256"] == json!(input.producer_binary.sha256)
            && raw["test_binary_sha256"] == evidence["test_binary_sha256"],
        "scoring closure identities differ"
    );
    let jobs: Vec<Pin> = serde_json::from_value(evidence["owned_jobs"].clone())?;
    ensure!(!jobs.is_empty(), "no closed native scoring Job");
    for pin in jobs {
        let job = pin.json()?;
        ensure!(
            job["exit_code"] == 0
                && job["timed_out"] == false
                && job["log_oversized"] == false
                && job["child_assigned_before_resume"] == true
                && job["owned_descendants_settled"] == true
                && job["owned_job_forced_termination"] == false
                && job["owned_job_final_active_processes"] == 0
                && job["owned_job_final_member_pids"] == json!([])
                && job.get("cleanup_refusal").is_none(),
            "native scoring Job incomplete"
        );
    }
    ensure!(
        evidence["origin"] == artifact.origin,
        "scientific scorer origin differs"
    );
    if let Some(basis) = artifact.matching_basis {
        ensure!(
            evidence["returned_synchronized"] == true,
            "live candidate CUDA completion required"
        );
        let pin: Pin = serde_json::from_value(evidence["independent_references"].clone())?;
        let independent = pin.json()?;
        ensure!(
            independent["schema"] == "airk.private.compact-cal-independent-references/1"
                && independent["complete"] == true
                && independent["candidate_profile_sha256"] == artifact.profile_file
                && independent["candidate_graph_sha256"] == artifact.graph
                && independent["candidate_weights_sha256"] == artifact.weights
                && independent["matching_reference_basis_sha256"] == basis
                && independent["preflight_sha256"] == json!(input.preflight.sha256),
            "foreign independent references"
        );
        let scored: Vec<_> = rows(raw, "rows")?
            .iter()
            .filter(|v| v["status"] == "raw_scores_only")
            .collect();
        let refs = rows(&independent, "rows")?;
        ensure!(scored.len() == refs.len(), "missing independent scored row");
        for (actual, reference) in scored.into_iter().zip(refs) {
            ensure!(
                actual["id"] == reference["id"] && actual["compiled"] == reference["compiled"],
                "independent reference compiler or order differs"
            );
            for name in ["matching_fp16", "original_fp32"] {
                let values: Vec<f32> = serde_json::from_value(reference[name].clone())?;
                ensure!(
                    values.len() == rows(actual, "scores")?.len()
                        && values.iter().all(|v| v.is_finite()),
                    "missing finite per-card independent reference"
                );
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "closed CAL100 and exact native Compact raw-score evidence; scientific fit only"]
fn fit_compact_artifact_generic_read_scientific() -> Result<()> {
    let expected: Sha256Digest = std::env::var("AIRK_CONTEXT_ACTION_FIT_INPUT_SHA256")?.parse()?;
    let path = PathBuf::from(
        std::env::var_os("AIRK_CONTEXT_ACTION_FIT_INPUT").context("missing fit input")?,
    );
    let input: ScientificInput = serde_json::from_slice(&read_pinned(&path, expected)?)?;
    let pre = input.preflight.json()?;
    let profile: Profile = serde_json::from_slice(&read_pinned(
        &input.model.profile,
        input.model.profile_sha256,
    )?)?;
    profile.validate()?;
    ensure!(
        profile.model == ModelChoice::ContextQwenCompact && profile.qualification.is_none(),
        "foreign Compact profile"
    );
    let raw: Value =
        serde_json::from_slice(&read_pinned(&input.model.raw, input.model.raw_sha256)?)?;
    read_pinned(&input.producer_binary.path, input.producer_binary.sha256)?;
    let master = focus_epoch(&input, &pre)?;
    let artifact = CompactArtifact::registered(input.model.profile_sha256, &profile)?;
    verify_producer(&input.scoring_closure.json()?, &raw, &input, artifact)?;
    ensure!(
        pre["qualified_profiles"] == json!([])
            && pre["corpus_sha256"] == CORPUS
            && raw["schema"] == "airk.arbiter.context-action.selected-cal-raw/1"
            && raw["corpus_sha256"] == CORPUS
            && raw["model"] == json!(profile.model)
            && raw["profile"] == json!(profile.manifest)
            && raw["profile_source_sha256"] == json!(input.model.profile_sha256)
            && raw["profiles_sha256"] == json!(master)
            && raw["preflight_sha256"] == json!(input.preflight.sha256)
            && raw["capture_sha256"] == pre["capture_sha256"]
            && !pre["capture_sha256"].is_null()
            && raw["task_policy"] == json!(context_action::policy_digest()?)
            && raw["task_policy"] == pre["task_policy"],
        "native raw source, model, capture or policy binding changed"
    );
    let (cases, uncaptured) = selected_cases(&pre, &raw)?;
    for (row, scored) in rows(&pre, "rows")?.iter().zip(rows(&raw, "rows")?) {
        aligned(row, scored)?;
    }
    let out = PathBuf::from(
        std::env::var_os("AIRK_CONTEXT_ACTION_FIT_OUTPUT").context("missing output directory")?,
    );
    ensure!(
        out.is_absolute(),
        "absolute private report directory required"
    );
    std::fs::create_dir(&out)?;
    let report = fit_artifact(&out.join("model"), input.model, profile, cases, uncaptured)?;
    std::fs::write(
        out.join("fit-report.private.json"),
        serde_json::to_vec_pretty(&json!({
            "schema":"airk.private.compact-artifact-scientific-fit/1","complete":true,"fit_input_sha256":expected,
            "preflight_sha256":input.preflight.sha256,"launch_audit_sha256":input.launch_audit.sha256,
            "Focus_profile":pre["Focus_profile"],"focus_tokens":1024,"scoring_closure_sha256":input.scoring_closure.sha256,
            "scientific_fit":report,"cal_questions":100,"two_model_gate":false,"numeric_qualification":false,
            "task_qualified":false,"release_qualified":false,"human_semantic_gold_count":0
        }))?,
    )?;
    Ok(())
}

#[test]
fn compact_fit_requires_original_compiler_card_order_and_allowance() -> Result<()> {
    let compiled = json!({"input_digest":"captured","input_ids":[1,2],"candidate_positions":[0,1]});
    let roster = json!({"cards":[{"id":"a"},{"id":"b"}]});
    let row = json!({"id":"q","source_group":"g","remaining_ms_at_observation":3,
        "request":{"roster":roster,"remaining_ms":5},"compiler_census":[{
            "profile":ORIGINAL_MANIFEST,"model":"arbiter-context-qwen3-17b","NN_invocations":0,
            "fits_remaining_allowance":true,"compiled":compiled}]});
    let scored = json!({"id":"q","split":"calibration","source_group":"g","roster":roster,
        "status":"raw_scores_only","scores":[-0.25,3.0],"compiled":compiled,"compile_us":220,"inference_us":2780});
    aligned(&row, &scored)?;
    for (field, value) in [
        ("id", json!("other")),
        ("source_group", json!("elsewhere")),
        ("roster", json!({"cards":[]})),
        (
            "compiled",
            json!({"input_digest":"captured","input_ids":[1,2],"candidate_positions":[1,0]}),
        ),
        ("scores", json!([3.0])),
        ("inference_us", json!(5000)),
    ] {
        let mut changed = scored.clone();
        changed[field] = value;
        assert!(aligned(&row, &changed).is_err(), "accepted altered {field}");
    }
    let mut over = row.clone();
    over["compiler_census"][0]["fits_remaining_allowance"] = json!(false);
    assert!(aligned(&over, &scored).is_err());
    let refused = json!({"id":"q","split":"calibration","source_group":"g","roster":roster,
        "status":"compiler_refused_remaining_allowance","census":over["compiler_census"][0]});
    aligned(&over, &refused)?;
    let mut unavailable = row;
    unavailable["request"] = Value::Null;
    assert!(aligned(&unavailable, &scored).is_err());
    aligned(
        &unavailable,
        &json!({"id":"q","split":"calibration","source_group":"g",
        "status":"no_native_assessment_request","scores":null}),
    )?;
    Ok(())
}
