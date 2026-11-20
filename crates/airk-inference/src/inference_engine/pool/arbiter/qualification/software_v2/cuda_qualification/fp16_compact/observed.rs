// SPDX-License-Identifier: Apache-2.0
//! Unprofiled engineering replay of one original, independently scored service admission.
use super::*;
use airk_core::native_context_action::NativeContextActionRequest;
use airk_retrieval::arbiter::{compiler::Compiler, context_action};

const ORIGINAL_COMPACT: &str = "6e7520aa29076f0958353a158e9be3390caef48675188e14e53660d035c1068f";

pub(super) struct Observed {
    pub(super) reference_pin: ResourceFilePin,
    pub(super) precision_reference: Option<ResourceFilePin>,
    pub(super) capture_pin: ResourceFilePin,
    pub(super) census_pin: ResourceFilePin,
    pub(super) case: String,
    pub(super) ordinal: usize,
    request: NativeContextActionRequest,
    compiled: Value,
    remaining_ns: u64,
    matching: Vec<f32>,
    original: Vec<f32>,
}

impl Observed {
    pub(super) fn load(
        candidate: &CandidateProof,
        precision: Option<&ProjectionSpec>,
    ) -> Result<Self> {
        let reference_pin = env_pin(
            "AIRK_CUDA_OBSERVED_REFERENCE",
            "AIRK_CUDA_OBSERVED_REFERENCE_SHA256",
        )?;
        ensure!(
            matches!(
                reference_pin.sha256.to_string().as_str(),
                "f657bbcdc9dac88829376c639bf2f20b6e4ae4c40f796f6305c1ec044bb59b91"
                    | "89b5a289b66ce93ac314c583cadff2112fda185081152d3f6a7886bae8b8d169"
            ),
            "reference outside the reviewed engineering population"
        );
        let owned: Value = serde_json::from_slice(&pinned(&reference_pin, PROOF_BYTES)?)?;
        let run = &owned["owned"];
        let reference = &owned["result"];
        ensure!(
            owned["complete"] == true
                && run["exit_code"] == 0
                && run["owned_descendants_settled"] == true
                && run["owned_job_final_active_processes"] == 0
                && reference["schema"] == "airk.private.compact-observed-development-reference/1"
                && reference["profile"] == ORIGINAL_COMPACT
                && reference["qualification"] == false
                && reference["native_scoring"] == false
                && reference["matching_t128_score_words_equal"] == true
                && reference["ragged_constant_value_control"] == true
                && reference["reference_basis"]["sha256"] == DEVELOPMENT_REFERENCE_SHA
                && (if let Some(spec) = precision {
                    spec.artifact
                        .is_some_and(|(_, graph)| graph == candidate.graph.sha256.to_string())
                        && QKV
                            .artifact
                            .is_some_and(|(_, graph)| reference["candidate_graph_sha256"] == graph)
                } else {
                    reference["candidate_graph_sha256"] == json!(candidate.graph.sha256)
                }),
            "authenticated completed publisher reference for this engineering graph required"
        );
        let capture_pin = serde_json::from_value::<ProofFile>(reference["capture"].clone())?.pin();
        let census_pin = serde_json::from_value::<ProofFile>(reference["census"].clone())?.pin();
        let capture: Value = serde_json::from_slice(&pinned(&capture_pin, PROOF_BYTES)?)?;
        let census: Value = serde_json::from_slice(&pinned(&census_pin, PROOF_BYTES)?)?;
        let ordinal: usize = serde_json::from_value(reference["ordinal"].clone())?;
        let case = reference["case"]
            .as_str()
            .context("observed case ID missing")?
            .to_owned();
        let source_name = format!("record-{ordinal:04}.json");
        ensure!(
            if case == "ENGINEERING_FIXED" {
                census["case_id"] == case
                    && census["schema"] == "airk.private.techqa-predispatch-capture/1"
                    && census["native_observations"]
                        .as_array()
                        .is_some_and(|rows| {
                            rows.iter().any(|row| {
                                row["file"] == source_name
                                    && row["sha256"] == json!(capture_pin.sha256)
                                    && row["context_forwards"] == 0
                            })
                        })
            } else {
                case.starts_with("TRAIN_Q")
                    && census["case"] == case
                    && census["split"] == "TRAIN"
                    && census["schema"] == "airk.private.compact-development-reconciled-admission/1"
                    && census["fixture_settled"] == true
                    && census["context"] == ORIGINAL_COMPACT
                    && census["source_proofs"]
                        [format!("fixture/context-cal-admissions/{source_name}")]
                        == json!(capture_pin.sha256)
            },
            "development census does not bind captured source"
        );
        ensure!(
            capture["schema"] == "airk.private-selected-Context-native-admission-observation/1"
                && capture["ordinal_in_private_run"] == ordinal
                && capture["actual_Focus_forward_observed"] == true
                && capture["actual_Focus_profile"] == reference["capture_focus"]
                && capture["request"]["roster"] == reference["roster"]
                && capture["request"]["choices"] == reference["offered_choices"],
            "original Focus, roster or choices differ"
        );
        let request: NativeContextActionRequest =
            serde_json::from_value(capture["request"].clone())?;
        let count = request.roster.cards.len();
        let remaining_ns = capture["remaining_ns_at_observation"]
            .as_u64()
            .context("remaining native deadline absent")?;
        let matches = capture["compiler_census"]
            .as_array()
            .context("missing compiler census")?
            .iter()
            .filter(|row| row["model"] == json!(ModelChoice::ContextQwenCompact))
            .collect::<Vec<_>>();
        let [actual] = matches.as_slice() else {
            anyhow::bail!("exactly one original Compact compiler row required")
        };
        ensure!(
            actual["profile"] == ORIGINAL_COMPACT
                && actual["NN_invocations"] == 0
                && actual["fits_remaining_allowance"] == true
                && actual["compiled"] == reference["compiled"]
                && (2..=8).contains(&count)
                && request.remaining_input_tokens
                    == reference["remaining_input_tokens"].as_u64().unwrap_or(0) as usize
                && remaining_ns > 0
                && request.remaining_ms > 0,
            "original compiler or allowance differs"
        );
        let (matching, precision_reference): (Vec<f32>, _) = if let Some(spec) = precision {
            let pin = env_pin(
                "AIRK_CUDA_FP16_PROJECTION_REFERENCE",
                "AIRK_CUDA_FP16_PROJECTION_REFERENCE_SHA256",
            )?;
            ensure!(
                spec.reference_sha
                    .is_some_and(|sha| sha == pin.sha256.to_string()),
                "unreviewed down-projection independent score proof"
            );
            let closure: Value = serde_json::from_slice(&pinned(&pin, PROOF_BYTES)?)?;
            let scored = &closure["result"];
            let (recipe, prior, matching) = if spec.stage_schema == full::SPEC.stage_schema {
                (
                    "c073724d2f98ac90a944718a055d8326691e569c29731243cf305eb41fb9bd5f",
                    &scored["sealed_qkv"],
                    &scored["matching_fp16"],
                )
            } else {
                (
                    "762f3da4ab62a3b52a528e7aa238c3749aed4af5db7318d777746947eaec58b0",
                    &scored["baseline"]["scores"],
                    &scored["all_down_fp32"]["scores"],
                )
            };
            ensure!(
                closure["complete"] == true
                    && closure["owned"]["exit_code"] == 0
                    && closure["owned"]["owned_descendants_settled"] == true
                    && closure["owned"]["owned_job_final_active_processes"] == 0
                    && scored["schema"] == spec.reference_schema
                    && scored["qualification"] == false
                    && scored["native_candidate_scoring"] == false
                    && scored["case"] == case
                    && scored["recipe_sha256"] == recipe
                    && scored["original_cpu"]["sha256"] == json!(reference_pin.sha256)
                    && prior == &reference["matching_fp16"]
                    && scored["original_fp32"] == reference["original_fp32"]
                    && scored["compiled"] == reference["compiled"]
                    && scored["roster"] == reference["roster"]
                    && scored["capture"] == reference["capture"]
                    && scored["census"] == reference["census"],
                "protected candidate independent source or compiler input differs"
            );
            (serde_json::from_value(matching.clone())?, Some(pin))
        } else {
            (
                serde_json::from_value(reference["matching_fp16"].clone())?,
                None,
            )
        };
        let original: Vec<f32> = serde_json::from_value(reference["original_fp32"].clone())?;
        ensure!(
            matching.len() == count
                && original.len() == count
                && matching.iter().chain(&original).all(|v| v.is_finite()),
            "observed input reference card count or finite scores differ"
        );
        Ok(Self {
            reference_pin,
            precision_reference,
            capture_pin,
            census_pin,
            case,
            ordinal,
            request,
            compiled: actual["compiled"].clone(),
            remaining_ns,
            matching,
            original,
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
        let budget = Duration::from_nanos(self.remaining_ns)
            .min(Duration::from_millis(self.request.remaining_ms));
        let call_deadline = deadline.min(
            start
                .checked_add(budget)
                .context("observed remaining duration overflow")?,
        );
        let prompt = context_action::prompt(&self.request)?;
        let input = compiler.compile(&prompt.state, &prompt.question, &prompt.candidates)?;
        let compiled = serde_json::to_value(&input)?;
        validate_rebuild(
            &compiled,
            &self.compiled,
            input.input_ids.len(),
            self.request.remaining_input_tokens,
            input.candidate_positions.len(),
            self.request.roster.cards.len(),
        )?;
        ensure!(
            Instant::now() < call_deadline,
            "original native allowance expired"
        );
        let compile_us = start.elapsed().as_micros();
        receipt["numeric_forward_attempts"] = json!(1);
        flush_graph_diagnostic(file, receipt)?;
        let cut = receipt["schema"] == "airk.private.compact-full-t1014-cut/1";
        let timed = journal.observe(
            operation::Kind::Numeric,
            1,
            Some(input.input_digest),
            || {
                if cut {
                    scorer
                        .score_layer0_taps(&input, call_deadline)
                        .map(|(scores, rows)| (scores, Some(rows)))
                } else {
                    scorer
                        .score_with_deadline(&input, call_deadline)
                        .map(|scores| (scores, None))
                }
            },
        )?;
        let (scores, taps) = timed.value?;
        scorer.synchronize(call_deadline)?;
        ensure!(
            scorer.device_settled()
                && timed.within_deadline
                && Instant::now() < call_deadline
                && scores.len() == self.matching.len()
                && scores.iter().all(|s| s.is_finite()),
            "observed input score, deadline or settlement differs"
        );
        if let Some(taps) = taps {
            full::record_cuts(receipt, &scores, taps)?;
        }
        let (raw, mass, winner) = comparison(&scores, &self.matching)?;
        let (original_raw, original_mass, original_winner) = comparison(&scores, &self.original)?;
        let margin = development_signed_margin(&self.matching, development_winner(&self.matching));
        let passed = raw <= DEVELOPMENT_RAW_LIMIT
            && winner
            && mass <= DEVELOPMENT_MASS_LIMIT
            && original_mass <= DEVELOPMENT_MASS_LIMIT;
        receipt["native_forward_returned"] = json!(true);
        receipt["release_raw_limit"] = json!(RELEASE_RAW_LIMIT);
        receipt["release_raw_pass"] = json!(raw <= RELEASE_RAW_LIMIT);
        receipt["rows"] = json!([{"name":self.case,"ordinal":self.ordinal,"input_digest":input.input_digest,
            "tokens":input.input_ids.len(),"options":scores.len(),"compiled":compiled,"roster":self.request.roster,
            "raw_scores":scores,"matching_fp16":self.matching,"original_fp32":self.original,
            "matching_raw_error":raw,"matching_mass_error":mass,"matching_winner_agrees":winner,
            "original_raw_error":original_raw,"original_mass_error":original_mass,"original_winner_agrees":original_winner,
            "matching_margin":margin,"near_tie_ambiguous":margin<=DEVELOPMENT_NEAR_TIE_LIMIT,
            "raw_ceiling_winner_at_risk":margin<=2.0*DEVELOPMENT_RAW_LIMIT,"development_screen_pass":passed,
            "compile_us":compile_us,"elapsed_us":timed.elapsed_us,"native_call_elapsed_us":timed.native_call_elapsed_us,
            "original_remaining_ms":self.request.remaining_ms,"original_remaining_ns":self.remaining_ns,
            "forward_synchronized":true,"within_deadline":true}]);
        flush_graph_diagnostic(file, receipt)?;
        ensure!(
            mass <= DEVELOPMENT_MASS_LIMIT && original_mass <= DEVELOPMENT_MASS_LIMIT,
            "representative mass control refused"
        );
        Ok(())
    }
}

fn validate_rebuild(
    rebuilt: &Value,
    original: &Value,
    tokens: usize,
    allowance: usize,
    slots: usize,
    cards: usize,
) -> Result<()> {
    ensure!(
        rebuilt == original && tokens <= allowance && slots == cards,
        "rebuilt input or original native allowance differs"
    );
    Ok(())
}

#[test]
fn rebuilt_input_includes_order_cardinality_and_allowance() {
    let original = json!({"candidate_positions":[1,3],"attention_mask":[1,1,1,1]});
    assert!(validate_rebuild(&original, &original, 4, 4, 2, 2).is_ok());
    let reordered = json!({"candidate_positions":[3,1],"attention_mask":[1,1,1,1]});
    assert!(validate_rebuild(&reordered, &original, 4, 4, 2, 2).is_err());
    assert!(validate_rebuild(&original, &original, 4, 3, 2, 2).is_err());
    assert!(validate_rebuild(&original, &original, 4, 4, 2, 3).is_err());
}

pub(super) fn compiler(profile: &Profile, files: &Files) -> Result<Compiler> {
    let asset = |role| {
        profile
            .assets
            .iter()
            .find(|asset| asset.role == role)
            .context("original verified compiler asset missing")
    };
    Compiler::from_verified(
        &files.read_asset(asset(AssetRole::Compiler)?)?,
        &files.read_asset(asset(AssetRole::Tokenizer)?)?,
    )
}
