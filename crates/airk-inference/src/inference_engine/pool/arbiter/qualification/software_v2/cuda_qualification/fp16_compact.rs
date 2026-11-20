// SPDX-License-Identifier: Apache-2.0
//! Private native metadata and T32 probe for the reviewed Compact FP16 candidate.
use super::*;
mod observed;
mod timing;
use airk_core::arbiter::device::{DevicePrecision, PlacementPolicy};

const ORIGINAL_GRAPH: &str = "c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4";
const FP32_FIXTURE: &str = "c4360d8a759c68c305fba0df7bd04001a6b043d8a2a57b36f0d8ef0f898519b9";
const FP32_PRODUCER: &str = "b3849443d0d4d737d5c3f88b323b9eaffbd1045500d97f1487c56be639884bbe";
const T32_INPUT: &str = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335";
const GRAPH_BYTES: u64 = 64 << 20;
const PROOF_BYTES: u64 = 16 << 20;
const REVIEWED_FP16_GRAPH_SHA256: &str =
    "47a7cd6f9e0ddf50911cc7a745bcbec4ca2a963772aad5e2e6acc663e26ec104";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProofFile {
    path: String,
    sha256: Sha256Digest,
    bytes: Option<u64>,
}
impl ProofFile {
    fn pin(&self) -> ResourceFilePin {
        ResourceFilePin {
            path: self.path.clone(),
            sha256: self.sha256,
        }
    }

    fn asset(&self, name: &str, role: AssetRole, limit: u64) -> Result<Asset> {
        self.pin().validate()?;
        let bytes = self
            .bytes
            .context("reviewed FP16 artifact length missing")?;
        ensure!(
            Path::new(&self.path).file_name().and_then(|v| v.to_str()) == Some(name)
                && (1..=limit).contains(&bytes),
            "reviewed FP16 artifact name or length differs"
        );
        Ok(Asset {
            name: name.into(),
            role,
            bytes,
            sha256: self.sha256,
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CandidateProof {
    graph: ProofFile,
    weights: ProofFile,
}

struct ProjectionSpec {
    artifact: Option<(&'static str, &'static str)>,
    stage_schema: &'static str,
    source_schema: &'static str,
    metadata_schema: &'static str,
    graph_schema: &'static str,
    native_graph_sha: Option<&'static str>,
    t32_schema: &'static str,
    reference_schema: &'static str,
    reference_sha: Option<&'static str>,
    recipe_routes: &'static str,
    matching_scores: &'static str,
    projections: usize,
}

const QKV: ProjectionSpec = ProjectionSpec {
    artifact: Some((
        "87ec206416eda607ff1585809d085e37fc4f8b99a5fc029308b5c78218cefcc2",
        "e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14",
    )),
    stage_schema: "airk.private.compact-fp32-qkv-projection-candidate/1",
    source_schema: "airk.private.compact-fp32-qkv-source/1",
    metadata_schema: "airk.private.compact-fp32-qkv-probe-metadata/1",
    graph_schema: "airk.private.compact-fp32-qkv-graph-diagnostic/1",
    native_graph_sha: Some("42a273598b5c8678535691426ed725041a5d9565ff1fbaf535a43938894f07a4"),
    t32_schema: "airk.private.compact-fp32-qkv-t32-component/1",
    reference_schema: "airk.private.compact-fp32-qkv-t32-reference/1",
    reference_sha: Some("304d9b16922ce88f9db181c1e1956d9c0aa99ab29c5ef4dff26f2c37f5b0fa28"),
    recipe_routes: "qkv_fp32_routes",
    matching_scores: "matching_qkv_fp32",
    projections: 84,
};

const QKVO: ProjectionSpec = ProjectionSpec {
    artifact: Some((
        "dbf3a746b8ec66d81ba68798988cdc4068d99841c538cbb2963a37125bd3fb5b",
        "0f2407345000cb244674238eead6178bc711238f2a4ddc5b61eef603d87be0b1",
    )),
    stage_schema: "airk.private.compact-fp32-qkvo-projection-candidate/1",
    source_schema: "airk.private.compact-fp32-qkvo-source/1",
    metadata_schema: "airk.private.compact-fp32-qkvo-probe-metadata/1",
    graph_schema: "airk.private.compact-fp32-qkvo-graph-diagnostic/1",
    native_graph_sha: Some("ef46663525337868d8a47fe440b9199bc927cf1625e4b4caa3abeb764e93a1f1"),
    t32_schema: "airk.private.compact-fp32-qkvo-t32-component/1",
    reference_schema: "airk.private.compact-fp32-qkvo-t32-reference/1",
    reference_sha: Some("768ef122dff10af1cddb887650d6231f65817a8c7138e4294953cb4a95a044a6"),
    recipe_routes: "qkvo_fp32_routes",
    matching_scores: "matching_qkvo_fp32",
    projections: 112,
};

fn projection_stage(
    spec: &ProjectionSpec,
    proof: &IndependentProof,
) -> Result<(ResourceFilePin, CandidateProof)> {
    let (stage_sha, graph_sha) = spec
        .artifact
        .context("reviewed Compact projection artifact pins unavailable")?;
    let pin = env_pin(
        "AIRK_CUDA_FP16_PROJECTION_STAGE",
        "AIRK_CUDA_FP16_PROJECTION_STAGE_SHA256",
    )?;
    ensure!(
        pin.sha256 == stage_sha.parse::<Sha256Digest>()?,
        "reviewed Compact FP32 Q/K/V projection stage required"
    );
    let fact: Value = serde_json::from_slice(&pinned(&pin, 1 << 20)?)?;
    let candidate = CandidateProof {
        graph: serde_json::from_value(fact["candidate_graph"].clone())?,
        weights: serde_json::from_value(fact["candidate_weights"].clone())?,
    };
    let directory = Path::new(&pin.path).parent();
    ensure!(
        fact["schema"] == spec.stage_schema
            && fact["complete"] == true
            && fact["qualification"] == false
            && fact["product_admission"] == false
            && fact["native_load_attempts"] == 0
            && fact["native_forward_attempts"] == 0
            && fact["baseline_graph"] == json!(proof.candidate.graph)
            && fact["baseline_weights"] == json!(proof.candidate.weights)
            && fact["graph_proof"]["projection_count"] == spec.projections
            && fact["graph_proof"]["added_cast_count"] == spec.projections * 3
            && candidate.graph.sha256 == graph_sha.parse::<Sha256Digest>()?
            && candidate.graph.sha256 != proof.candidate.graph.sha256
            && candidate.weights.sha256 == proof.candidate.weights.sha256
            && candidate.weights.bytes == proof.candidate.weights.bytes
            && Path::new(&candidate.graph.path).parent() == directory
            && Path::new(&candidate.weights.path).parent() == directory,
        "Compact FP32 Q/K/V stage graph, baseline or weight lineage differs"
    );
    Ok((pin, candidate))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceProof {
    fixture: ProofFile,
    producer: ProofFile,
    name: String,
    input: CompiledInput,
    model: ModelChoice,
    compiler: Sha256Digest,
    resource_policy: Sha256Digest,
    matching_fp16: [f32; 8],
    original_fp32: [f32; 8],
}

#[derive(Deserialize)]
struct IndependentProof {
    schema: String,
    complete: bool,
    model: ModelChoice,
    reference_device: String,
    model_inference: bool,
    torch_forward_invocations: u64,
    native_execution: bool,
    qualification: bool,
    candidate: CandidateProof,
    original_graph: ProofFile,
    verification: Value,
    reference: ReferenceProof,
}

fn projection_reference(
    spec: &ProjectionSpec,
    stage: &ResourceFilePin,
    candidate: &CandidateProof,
    baseline: &IndependentProof,
) -> Result<(ResourceFilePin, ReferenceProof)> {
    let pin = env_pin(
        "AIRK_CUDA_FP16_PROJECTION_REFERENCE",
        "AIRK_CUDA_FP16_PROJECTION_REFERENCE_SHA256",
    )?;
    ensure!(
        pin.sha256
            == spec
                .reference_sha
                .context("reviewed Compact projection reference pin unavailable")?
                .parse::<Sha256Digest>()?,
        "exact independent Compact FP32 Q/K/V T32 reference required"
    );
    let mut proof: Value = serde_json::from_slice(&pinned(&pin, PROOF_BYTES)?)?;
    ensure!(
        proof["schema"] == spec.reference_schema
            && proof["complete"] == true
            && proof["reference_device"] == "cpu"
            && proof["model_inference"] == true
            && proof["torch_forward_invocations"] == 1
            && proof["native_execution"] == false
            && proof["qualification"] == false
            && proof["product_admission"] == false
            && proof["candidate"] == json!(candidate)
            && proof["baseline_candidate"] == json!(baseline.candidate)
            && proof["original_graph"] == json!(baseline.original_graph)
            && proof["stage_receipt"]["path"] == json!(stage.path)
            && proof["stage_receipt"]["sha256"] == json!(stage.sha256)
            && proof["recipe"][spec.recipe_routes]
                .as_array()
                .is_some_and(|routes| routes.len() == spec.projections)
            && proof["recipe"]["context_flash_single_tile"] == true,
        "independent Compact FP32 Q/K/V recipe or source differs"
    );
    let reference = proof["reference"]
        .as_object_mut()
        .context("missing Q/K/V reference row")?;
    let matching = reference
        .remove(spec.matching_scores)
        .context("missing matching Q/K/V scores")?;
    ensure!(
        reference.insert("matching_fp16".into(), matching).is_none(),
        "ambiguous matching Q/K/V scores"
    );
    let reference: ReferenceProof = serde_json::from_value(proof["reference"].take())?;
    ensure!(
        serde_json::to_value(&reference.input)? == serde_json::to_value(&baseline.reference.input)?
            && exact_reference_score_words(
                &reference.original_fp32,
                &baseline.reference.original_fp32
            ),
        "independent Compact FP32 Q/K/V original T32 input or score bits differ"
    );
    Ok((pin, reference))
}

const DEVELOPMENT_REFERENCE_SHA: &str =
    "f0d0e01fa28e7a876f1c57bc9894dff7e2a915ac8bf8150885b2aac2fdff2f14";
// Provisional Compact 1.8.207 development-only limit. Qualification retains 0.001.
const DEVELOPMENT_RAW_LIMIT: f64 = 0.070;
const DEVELOPMENT_MASS_LIMIT: f64 = 0.05;
const DEVELOPMENT_NEAR_TIE_LIMIT: f64 = 0.020;
const RETAINED_DEVELOPMENT_FIXTURE: &str =
    "3962186640b42648b345fa200516ed44a3de4ccab0d783e671c9c22c3b8b1bbd";
const RETAINED_DEVELOPMENT_PRODUCER: &str =
    "c48e88b5d6bd8dff20b052bedebf0a4a48354d57caa1d2944b2dda16281805b2";
const DEVELOPMENT_CASES: [(usize, &str, &str); 3] = [
    (
        128,
        "all-live-1",
        "791067affc1eeba53660a0044369f22f1f463535656419c5da9a486517222c7a",
    ),
    (
        1024,
        "native-all-live-1024",
        "948a46156a129e61b5beadce977e6314b6e96c9570a439724813fe9592fa6357",
    ),
    (
        8192,
        "native-all-live-8192",
        "ca9d34d962e308e6e53af8e14101058e8f02fbb4151bcc65eda2ad88be3de1f0",
    ),
];

struct DevelopmentReferences {
    pin: ResourceFilePin,
    recipe: Value,
    rows: [ReferenceProof; 3],
}

fn development_references(
    stage: &ResourceFilePin,
    candidate: &CandidateProof,
    baseline: &IndependentProof,
    profile: &Profile,
    policy: &ResourcePolicy,
) -> Result<DevelopmentReferences> {
    let pin = env_pin(
        "AIRK_CUDA_FP16_PROJECTION_REFERENCE",
        "AIRK_CUDA_FP16_PROJECTION_REFERENCE_SHA256",
    )?;
    ensure!(
        pin.sha256 == DEVELOPMENT_REFERENCE_SHA.parse::<Sha256Digest>()?,
        "exact independent Compact wider Q/K/V reference required"
    );
    let mut proof: Value = serde_json::from_slice(&pinned(&pin, PROOF_BYTES)?)?;
    let recipe = proof["recipe"].clone();
    ensure!(
        proof["schema"] == "airk.private.compact-fp32-qkv-wider-reference/1"
            && proof["complete"] == true
            && proof["model"] == json!(profile.model)
            && proof["reference_device"] == "cpu"
            && proof["model_inference"] == true
            && proof["torch_forward_invocations"] == 3
            && proof["native_execution"] == false
            && proof["qualification"] == false
            && proof["product_admission"] == false
            && proof["candidate"] == json!(candidate)
            && proof["baseline_candidate"] == json!(baseline.candidate)
            && proof["original_graph"] == json!(baseline.original_graph)
            && proof["stage_receipt"]["path"] == json!(stage.path)
            && proof["stage_receipt"]["sha256"] == json!(stage.sha256)
            && recipe["qkv_fp32_routes"]
                .as_array()
                .is_some_and(|routes| routes.len() == 84)
            && recipe["context_flash_single_tile"] == false
            && recipe["torch_cpu_threads"] == 8
            && recipe["wider_attention"]
                == json!({
                    "algorithm": "causal-reverse-tile-fp16-exponent",
                    "query_block": 64, "key_block": 64, "split_kv": false,
                }),
        "independent wider Q/K/V reference lineage or recipe differs"
    );
    let mut rows = proof["references"].take();
    let rows = rows
        .as_array_mut()
        .context("independent wider Q/K/V rows missing")?;
    ensure!(
        rows.len() == DEVELOPMENT_CASES.len(),
        "wider Q/K/V reference roster differs"
    );
    let mut verified = Vec::new();
    for (row, (width, name, input_sha)) in rows.iter_mut().zip(DEVELOPMENT_CASES) {
        let source = row
            .as_object_mut()
            .context("wider Q/K/V reference row missing")?;
        let matching = source
            .remove("matching_qkv_fp32")
            .context("wider Q/K/V scores missing")?;
        ensure!(
            source.insert("matching_fp16".into(), matching).is_none(),
            "ambiguous wider matching scores"
        );
        let reference: ReferenceProof = serde_json::from_value(row.take())?;
        let fixture_pin = reference.fixture.pin();
        let producer_pin = reference.producer.pin();
        let (fixture_sha, producer_sha) = if width == 1024 {
            (FP32_FIXTURE, FP32_PRODUCER)
        } else {
            (RETAINED_DEVELOPMENT_FIXTURE, RETAINED_DEVELOPMENT_PRODUCER)
        };
        ensure!(
            fixture_pin.sha256 == fixture_sha.parse()?
                && producer_pin.sha256 == producer_sha.parse()?
                && reference.name == name
                && reference.input.input_digest == input_sha.parse()?
                && reference.input.input_ids.len() == width
                && reference.model == profile.model
                && reference.compiler == profile.compiler
                && reference.resource_policy == policy.digest()?
                && reference.matching_fp16.iter().all(|v| v.is_finite())
                && reference.original_fp32.iter().all(|v| v.is_finite()),
            "independent wider Compact Q/K/V reference row differs at T{width}"
        );
        let fixture: NumericFixture = serde_json::from_slice(&pinned(&fixture_pin, PROOF_BYTES)?)?;
        pinned(&producer_pin, PROOF_BYTES)?;
        let source = fixture
            .rows
            .iter()
            .find(|row| row.name == name)
            .context("original FP32 wider fixture row missing")?;
        let original: &[f32; 8] = source
            .reference_original_fp32
            .as_deref()
            .context("original FP32 wider fixture scores missing")?
            .try_into()?;
        ensure!(
            fixture.schema == "airk.arbiter.native-numeric/2"
                && fixture.model == reference.model
                && fixture.compiler == reference.compiler
                && fixture.resource_policy == reference.resource_policy
                && fixture.reference_producer.as_ref() == Some(&producer_pin)
                && serde_json::to_value(&source.input)? == serde_json::to_value(&reference.input)?
                && reference
                    .input
                    .attention_mask
                    .iter()
                    .all(|value| *value == 1)
                && reference.input.option_mask == [true; 8]
                && exact_reference_score_words(original, &reference.original_fp32),
            "original FP32 wider fixture inputs or score words differ at T{width}"
        );
        verified.push(reference);
    }
    Ok(DevelopmentReferences {
        pin,
        recipe,
        rows: verified
            .try_into()
            .map_err(|_| anyhow::anyhow!("wider Q/K/V reference count differs"))?,
    })
}

fn development_winner(scores: &[f32]) -> usize {
    scores.iter().enumerate().fold(
        0,
        |best, (i, score)| {
            if *score > scores[best] { i } else { best }
        },
    )
}

fn development_signed_margin(scores: &[f32], slot: usize) -> f64 {
    let runner_up = scores
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != slot)
        .map(|(_, score)| f64::from(*score))
        .fold(f64::NEG_INFINITY, f64::max);
    f64::from(scores[slot]) - runner_up
}

fn development_metrics(row: &mut Value, actual: &[f32], reference: &ReferenceProof) -> Result<()> {
    let (raw, mass, matched) = comparison(actual, &reference.matching_fp16)?;
    let (original_raw, original_mass, original_matched) =
        comparison(actual, &reference.original_fp32)?;
    let actual_winner = development_winner(actual);
    let matching_winner = development_winner(&reference.matching_fp16);
    let original_winner = development_winner(&reference.original_fp32);
    let matching_margin = development_signed_margin(&reference.matching_fp16, matching_winner);
    let near_tie = matching_margin <= DEVELOPMENT_NEAR_TIE_LIMIT;
    row["matching_qkv_raw_error"] = json!(raw);
    row["matching_qkv_mass_error"] = json!(mass);
    row["matching_qkv_argmax_matches"] = json!(matched);
    row["original_fp32_raw_error"] = json!(original_raw);
    row["original_fp32_mass_error"] = json!(original_mass);
    row["original_fp32_argmax_matches"] = json!(original_matched);
    row["native_winner"] = json!(actual_winner);
    row["matching_qkv_winner"] = json!(matching_winner);
    row["original_fp32_winner"] = json!(original_winner);
    row["native_winner_margin"] = json!(development_signed_margin(actual, actual_winner));
    row["matching_qkv_reference_margin"] = json!(matching_margin);
    row["original_fp32_reference_margin"] = json!(development_signed_margin(
        &reference.original_fp32,
        original_winner
    ));
    row["native_margin_for_matching_qkv_winner"] =
        json!(development_signed_margin(actual, matching_winner));
    row["native_margin_for_original_fp32_winner"] =
        json!(development_signed_margin(actual, original_winner));
    row["near_tie_ambiguous"] = json!(near_tie);
    row["raw_ceiling_winner_at_risk"] = json!(matching_margin <= 2.0 * DEVELOPMENT_RAW_LIMIT);
    row["development_screen_pass"] = json!(
        raw <= DEVELOPMENT_RAW_LIMIT
            && matched
            && mass <= DEVELOPMENT_MASS_LIMIT
            && original_mass <= DEVELOPMENT_MASS_LIMIT
    );
    ensure!(
        mass <= DEVELOPMENT_MASS_LIMIT && original_mass <= DEVELOPMENT_MASS_LIMIT,
        "matching Q/K/V or original FP32 mass bound refused at T{}",
        reference.input.input_ids.len()
    );
    Ok(())
}

#[test]
fn compact_qkv_development_preserves_signed_near_tie_and_mass_refusal() -> Result<()> {
    let sha = Sha256Digest::from_bytes([1; 32]);
    let pin = || ProofFile {
        path: "literal".into(),
        sha256: sha,
        bytes: None,
    };
    let expected = [1.0, 0.99, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let reference = ReferenceProof {
        fixture: pin(),
        producer: pin(),
        name: "literal".into(),
        input: CompiledInput {
            prompt: String::new(),
            input_ids: vec![1; 128],
            attention_mask: vec![1; 128],
            position_ids: (0..128).collect(),
            decision_position: 127,
            candidate_positions: (0..8).collect(),
            option_mask: vec![true; 8],
            input_digest: sha,
        },
        model: ModelChoice::ContextQwenCompact,
        compiler: sha,
        resource_policy: sha,
        matching_fp16: expected,
        original_fp32: expected,
    };
    let mut row = json!({});
    let mut actual = [0.994, 0.996, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    development_metrics(&mut row, &actual, &reference)?;
    assert_eq!(row["native_winner"], 1);
    assert_eq!(row["matching_qkv_winner"], 0);
    assert_eq!(row["matching_qkv_argmax_matches"], false);
    assert_eq!(row["near_tie_ambiguous"], true);
    assert_eq!(row["raw_ceiling_winner_at_risk"], true);
    assert_eq!(row["development_screen_pass"], false);
    assert!(
        row["native_margin_for_matching_qkv_winner"]
            .as_f64()
            .unwrap()
            < 0.0
    );
    actual = expected;
    actual[2] = 0.069;
    development_metrics(&mut row, &actual, &reference)?;
    assert_eq!(row["development_screen_pass"], true);
    actual[2] = 0.071;
    development_metrics(&mut row, &actual, &reference)?;
    assert_eq!(row["development_screen_pass"], false);
    let mut reference = reference;
    reference.matching_fp16[1] = 0.96875;
    actual[2] = 0.0;
    actual[0] = 0.98;
    development_metrics(&mut row, &actual, &reference)?;
    assert_eq!(row["near_tie_ambiguous"], false);
    assert_eq!(row["raw_ceiling_winner_at_risk"], true);
    assert_eq!(row["development_screen_pass"], false);
    actual[0] = 15.0;
    assert!(development_metrics(&mut row, &actual, &reference).is_err());
    assert!(row["matching_qkv_mass_error"].as_f64().unwrap() > 0.05);
    Ok(())
}

fn score_development_rows(
    scorer: &Scorer,
    device: &cuda::CudaDevice,
    run: timing::Run<'_>,
    journal: &mut operation::Journal,
    receipt: &mut Value,
    file: &mut File,
    deadline: Instant,
) -> Result<()> {
    let population = run.schedule();
    for (index, sample) in population.iter().enumerate() {
        let reference = sample.reference;
        let input = &reference.input;
        let width = input.input_ids.len();
        let before = device.observation(deadline)?;
        let host_before = airk_kernel::service_resource_observations::rss(std::process::id());
        receipt["numeric_forward_attempts"] = json!(index + 1);
        flush_graph_diagnostic(file, receipt)?;
        {
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            writeln!(
                stdout,
                "AIRK_CUDA_QKV_DEVELOPMENT_FORWARD {width} {}",
                input.input_digest
            )?;
            stdout.flush()?;
        }
        let (kind, ordinal) = run
            .population
            .map_or((operation::Kind::Numeric, index + 1), |(population, _)| {
                population.journal_operation(index)
            });
        let timed = journal.observe(kind, ordinal, Some(input.input_digest), || {
            scorer.score_with_deadline(input, deadline)
        })?;
        let actual = timed.value?;
        receipt["native_forward_returned"] = json!(true);
        let mut row = json!({
            "name": reference.name, "tokens": width, "input_digest": input.input_digest,
            "raw_scores": actual, "elapsed_us": timed.elapsed_us, "native_call_elapsed_us": timed.native_call_elapsed_us,
            "within_deadline": timed.within_deadline, "forward_synchronized": false,
            "adapter_before": before, "host_rss_before_bytes": host_before,
        });
        let checked = scorer.synchronize(deadline).and_then(|()| {
            ensure!(
                timed.within_deadline && scorer.device_settled(),
                "wider Compact Q/K/V forward expired or remains unsettled at T{width}"
            );
            row["forward_synchronized"] = json!(true);
            row["adapter_after"] = json!(device.observation(deadline)?);
            row["host_rss_after_bytes"] = json!(airk_kernel::service_resource_observations::rss(
                std::process::id()
            ));
            if let Some(sealed) = sample.sealed {
                row["sample_kind"] = json!(sample.kind);
                row["cycle"] = json!(sample.cycle);
                ensure!(
                    actual
                        .iter()
                        .map(|v| v.to_bits())
                        .eq(sealed.iter().map(|v| v.to_bits())),
                    "unprofiled output differs from the sealed candidate at T{width}"
                );
                Ok(())
            } else {
                development_metrics(&mut row, &actual, reference)
            }
        });
        if let Err(error) = &checked {
            row["error"] = json!(format!("{error:#}"));
        }
        receipt["rows"].as_array_mut().unwrap().push(row);
        flush_graph_diagnostic(file, receipt)?;
        checked?;
    }
    if run.population.is_some() {
        return Ok(());
    }
    receipt["development_screen_pass"] = json!(receipt["rows"].as_array().is_some_and(|rows| {
        rows.len() == DEVELOPMENT_CASES.len()
            && rows
                .iter()
                .all(|row| row["development_screen_pass"] == true)
    }));
    Ok(())
}

fn exact_reference_score_words(actual: &[f32; 8], expected: &[f32; 8]) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, b)| a.to_bits() == b.to_bits())
}

#[test]
fn compact_fp16_qkvo_projection_scope_has_distinct_native_and_reference_contracts() {
    assert_eq!(QKV.projections, 84);
    assert_eq!(QKVO.projections, 112);
    assert_ne!(QKV.stage_schema, QKVO.stage_schema);
    assert_ne!(QKV.graph_schema, QKVO.graph_schema);
    assert_ne!(QKV.t32_schema, QKVO.t32_schema);
    assert_ne!(QKV.reference_schema, QKVO.reference_schema);
    assert_eq!(QKVO.recipe_routes, "qkvo_fp32_routes");
    assert_eq!(QKVO.matching_scores, "matching_qkvo_fp32");
}

#[test]
fn compact_fp16_qkv_reference_compares_original_binary32_words() -> Result<()> {
    let model_scores: Value = serde_json::from_str("[0.10000000149011612,-0.0,0,1,2,3,4,5]")?;
    let native_scores = json!([0.1, -0.0, 0, 1, 2, 3, 4, 5]);
    assert_ne!(model_scores, native_scores);
    let actual = serde_json::from_value(model_scores)?;
    let mut expected = serde_json::from_value(native_scores)?;
    assert!(exact_reference_score_words(&actual, &expected));
    expected[1] = 0.0;
    assert!(!exact_reference_score_words(&actual, &expected));
    Ok(())
}

fn validate_independent_proof(proof: &IndependentProof) -> Result<()> {
    let graph = &proof.verification["graph"];
    let parameters = proof.verification["parameters"]
        .as_array()
        .context("independent Compact FP16 parameter census missing")?;
    let layers = 28;
    ensure!(
        proof.schema == "airk.private.compact-fp16-independent-proof/1"
            && proof.complete
            && proof.model == ModelChoice::ContextQwenCompact
            && proof.reference_device == "cpu"
            && proof.model_inference
            && proof.torch_forward_invocations == 1
            && !proof.native_execution
            && !proof.qualification
            && proof.original_graph.sha256 == ORIGINAL_GRAPH.parse()?
            && proof.candidate.graph.sha256 != proof.original_graph.sha256
            && proof.verification["qualification"] == false
            && graph["qualification"] == false
            && graph["layers"] == layers
            && graph["nodes"]
                .as_u64()
                .is_some_and(|count| (1..=65_536).contains(&count))
            && graph["all_output_reachable_edges_checked"] == true
            && parameters.len() == 2 + layers * 11,
        "reviewed Compact FP16 graph or learned-parameter census differs"
    );
    let mut names = BTreeSet::new();
    let mut initializers = BTreeSet::new();
    for row in parameters {
        let name = row["parameter"]
            .as_str()
            .context("parameter name missing")?;
        let initializer = row["initializer"]
            .as_str()
            .context("initializer name missing")?;
        let actual: Sha256Digest =
            serde_json::from_value(row["actual_initializer_sha256"].clone())?;
        let expected: Sha256Digest =
            serde_json::from_value(row["independent_expected_sha256"].clone())?;
        ensure!(
            !name.is_empty()
                && !initializer.is_empty()
                && names.insert(name)
                && initializers.insert(initializer)
                && actual == expected,
            "independent Compact FP16 parameter binding differs"
        );
    }
    Ok(())
}

fn validate_reference(
    reference: &ReferenceProof,
    cpu: &Profile,
    policy: &ResourcePolicy,
) -> Result<()> {
    let fixture_pin = reference.fixture.pin();
    let producer_pin = reference.producer.pin();
    ensure!(
        fixture_pin.sha256 == FP32_FIXTURE.parse()?
            && producer_pin.sha256 == FP32_PRODUCER.parse()?
            && reference.name == "diagnostic-all-live-32"
            && reference.input.input_digest == T32_INPUT.parse()?
            && reference.model == cpu.model
            && reference.compiler == cpu.compiler
            && reference.resource_policy == policy.digest()?
            && reference
                .matching_fp16
                .iter()
                .all(|value| value.is_finite()),
        "independent Compact FP16 T32 identity differs"
    );
    let fixture: NumericFixture = serde_json::from_slice(&pinned(&fixture_pin, PROOF_BYTES)?)?;
    pinned(&producer_pin, PROOF_BYTES)?;
    numeric_rows(&fixture.rows, &[(32, 8), (1024, 8)], false)?;
    let row = &fixture.rows[0];
    ensure!(
        fixture.schema == "airk.arbiter.native-numeric/2"
            && fixture.model == reference.model
            && fixture.compiler == reference.compiler
            && fixture.resource_policy == reference.resource_policy
            && fixture.reference_producer.as_ref() == Some(&producer_pin)
            && row.name == reference.name
            && serde_json::to_value(&row.input)? == serde_json::to_value(&reference.input)?
            && row.reference_original_fp32.as_deref() == Some(reference.original_fp32.as_slice()),
        "original Compact FP32 T32 fixture or reference differs"
    );
    Ok(())
}

fn require_initial_bounds(
    bounds: &GpuProbeReservation,
    phases: (u64, u64, u64, u64),
) -> Result<()> {
    ensure!(
        bounds.host_limit_bytes == 32 << 30
            && bounds.device_limit_bytes == 24 << 30
            && bounds.deadline_ms == 120_000
            && phases == (16 << 30, 8 << 30, 2 << 30, 32 << 30),
        "Compact FP16 probe requires the reviewed Windows host, device and deadline bounds"
    );
    Ok(())
}

#[test]
fn compact_fp16_metadata_requires_original_windows_probe_envelope() -> Result<()> {
    let mut bounds = GpuProbeReservation {
        schema: "airk.arbiter.gpu-probe-reservation/1".into(),
        host_limit_bytes: 32 << 30,
        device_limit_bytes: 24 << 30,
        deadline_ms: 120_000,
    };
    let phases = (16 << 30, 8 << 30, 2 << 30, 32 << 30);
    require_initial_bounds(&bounds, phases)?;
    assert!(require_initial_bounds(&bounds, (16 << 30, 8 << 30, 2 << 30, 24 << 30)).is_err());
    bounds.host_limit_bytes = 16 << 30;
    assert!(require_initial_bounds(&bounds, phases).is_err());
    Ok(())
}

#[test]
#[ignore = "data-only metadata for the actual reviewed Compact FP16 artifact and T32 reference"]
fn emit_cuda_compact_fp16_probe_metadata() -> Result<()> {
    emit_compact_fp16_probe_metadata(None)
}

#[test]
#[ignore = "owned unqualified metadata only for the reviewed Compact FP32 Q/K/V projection experiment"]
fn emit_cuda_compact_fp32_qkv_projection_probe_metadata() -> Result<()> {
    emit_compact_fp16_probe_metadata(Some(&QKV))
}

#[test]
#[ignore = "owned unqualified metadata only for the reviewed Compact FP32 Q/K/V/O projection experiment"]
fn emit_cuda_compact_fp32_qkvo_projection_probe_metadata() -> Result<()> {
    emit_compact_fp16_probe_metadata(Some(&QKVO))
}

fn emit_compact_fp16_probe_metadata(projection_spec: Option<&ProjectionSpec>) -> Result<()> {
    let started = Instant::now();
    let execution_user = airk_ipc::endpoint::current_user()?;
    ensure!(
        execution_user == diagnostic_default_owner()?,
        "native Compact FP16 default owner differs"
    );
    let (request, request_pin) = request()?;
    let Request::Prepare {
        schema,
        cpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    } = request
    else {
        anyhow::bail!("reviewed Compact CPU Prepare request required");
    };
    ensure!(
        schema == SCHEMA,
        "unknown Compact FP16 metadata request schema"
    );
    require_initial_bounds(
        &bounds,
        (cold_bytes, warm_bytes, headroom_bytes, job_limit_bytes),
    )?;
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let (mut input, cpu, cpu_files, _) = inputs_from(&cpu_input)?;
    let capacity = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &cpu,
    )?;
    let cpu_graph = cpu
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Model)
        .context("authenticated Compact CPU graph missing")?;
    ensure!(
        cpu.model == ModelChoice::ContextQwenCompact
            && cpu.cpu_fit.is_some()
            && cpu.device_execution.is_none()
            && cpu_graph.sha256 == ORIGINAL_GRAPH.parse()?,
        "current native Compact CPU profile and original graph required"
    );
    let approved_manifest: Sha256Digest = option_env!("AIRK_APPROVED_CUDA_MANIFEST_SHA256")
        .context("CUDA manifest approval must be compiled")?
        .parse()?;
    let approved_build: Sha256Digest = option_env!("AIRK_STATIC_CUDA_BUILD_SHA256")
        .context("static CUDA build approval must be compiled")?
        .parse()?;
    ensure!(
        runtime_manifest.sha256 == approved_manifest,
        "Compact FP16 runtime manifest differs from the compiled approval"
    );
    let runtime = pinned(&runtime_manifest, 128 << 10)?;
    let proof_pin = env_pin(
        "AIRK_CUDA_FP16_INDEPENDENT_PROOF",
        "AIRK_CUDA_FP16_INDEPENDENT_PROOF_SHA256",
    )?;
    let proof: IndependentProof = serde_json::from_slice(&pinned(&proof_pin, PROOF_BYTES)?)?;
    validate_independent_proof(&proof)?;
    validate_reference(&proof.reference, &cpu, &input.policy)?;
    let original_bytes = pinned(&proof.original_graph.pin(), GRAPH_BYTES)?;
    ensure!(
        proof.original_graph.bytes == Some(cpu_graph.bytes)
            && original_bytes.len() as u64 == cpu_graph.bytes,
        "independently verified original Compact graph does not bind the CPU profile"
    );
    verify_files(&cpu_files, &cpu)?;
    ensure!(
        Instant::now() < deadline,
        "original Compact FP16 metadata expiry reached"
    );
    let projection = projection_spec
        .map(|spec| projection_stage(spec, &proof))
        .transpose()?;
    let candidate = projection
        .as_ref()
        .map_or(&proof.candidate, |(_, candidate)| candidate);
    ensure!(
        Path::new(&candidate.graph.path).parent() == Path::new(&candidate.weights.path).parent(),
        "reviewed Compact FP16 graph and weights must share their original directory"
    );
    let model = candidate
        .graph
        .asset("model.onnx", AssetRole::Model, GRAPH_BYTES)?;
    let weights = candidate
        .weights
        .asset("weights.bin", AssetRole::ExternalWeights, 16 << 30)?;
    for asset in &cpu.assets {
        if matches!(asset.role, AssetRole::Model | AssetRole::ExternalWeights) {
            input.files.remove(&asset.name);
        }
    }
    ensure!(
        input
            .files
            .insert(model.name.clone(), candidate.graph.path.clone())
            .is_none()
            && input
                .files
                .insert(weights.name.clone(), candidate.weights.path.clone())
                .is_none(),
        "reviewed Compact FP16 model paths collide with existing assets"
    );
    let prepared = cuda::prepare_fp16_probe_profile(
        &cpu,
        &Files(input.files.clone()),
        vec![model, weights],
        proof.original_graph.sha256,
        &runtime,
        runtime_manifest.sha256,
        capacity,
    )?;
    let device = prepared
        .profile
        .device_execution
        .as_ref()
        .context("Compact FP16 descriptor missing")?;
    ensure!(
        device.runtime_build == approved_build
            && device.graph == candidate.graph.sha256
            && device.precision == DevicePrecision::Float16
            && device.placement == PlacementPolicy::StrictGpu
            && prepared.original_graph == cpu_graph.sha256
            && prepared.profile.qualification.is_none()
            && prepared.profile.qualified_targets.is_empty(),
        "prepared Compact FP16 profile does not bind the approved build and strict candidate"
    );
    ensure!(
        Instant::now() < deadline,
        "original Compact FP16 metadata expiry reached"
    );
    let directory = PathBuf::from(text("AIRK_CUDA_METADATA_DIRECTORY")?);
    ensure!(directory.is_absolute(), "absolute guarded output required");
    std::fs::create_dir(&directory)?;
    let mut source_manifest = json!({
        "schema": "airk.private.compact-fp16-source/1",
        "parent_source": input.source_manifest, "parent_input": cpu_input,
        "parent_profile": cpu.manifest, "original_graph": proof.original_graph,
        "candidate": candidate, "independent_proof": proof_pin,
        "attention_plan": prepared.attention_plan, "assets": prepared.profile.assets,
    });
    if let (Some(spec), Some((stage, _))) = (projection_spec, &projection) {
        source_manifest["schema"] = json!(spec.source_schema);
        source_manifest["projection_stage"] = json!(stage);
    }
    input.source_manifest = write_json(&directory.join("source-manifest.json"), &source_manifest)?;
    let calibration = write_new(
        &directory.join(&prepared.calibration_name),
        &prepared.calibration_bytes,
    )?;
    input
        .files
        .insert(prepared.calibration_name, calibration.path.clone());
    input.profile = write_json(&directory.join("profile.json"), &prepared.profile)?;
    input.numeric_fixture = None;
    input.validate()?;
    let native_input = write_json(&directory.join("native-input.json"), &input)?;
    let probe = write_json(
        &directory.join("probe-request.json"),
        &Request::Probe {
            schema,
            gpu_input: native_input.clone(),
            runtime_manifest: runtime_manifest.clone(),
            bounds,
            cold_bytes,
            warm_bytes,
            headroom_bytes,
            job_limit_bytes,
        },
    )?;
    ensure!(
        Instant::now() < deadline,
        "original Compact FP16 metadata expiry reached"
    );
    let mut receipt = json!({
            "schema": "airk.private.compact-fp16-probe-metadata/1",
            "request": request_pin, "cpu_input": cpu_input, "runtime_manifest": runtime_manifest,
            "independent_proof": proof_pin, "reference_input_digest": proof.reference.input.input_digest,
            "source_manifest": input.source_manifest, "profile": prepared.profile.manifest,
            "profile_pin": input.profile, "calibration": calibration, "native_input": native_input,
            "probe_request": probe, "execution_user": execution_user, "qualified": false,
            "model_requests_dispatched": false, "runtime_admission_dispatched": false,
            "forward_count_observed": null, "elapsed_us": started.elapsed().as_micros(),
    });
    if let (Some(spec), Some((stage, _))) = (projection_spec, &projection) {
        receipt["schema"] = json!(spec.metadata_schema);
        receipt["projection_stage"] = json!(stage);
    }
    write_json(&directory.join("metadata-receipt.json"), &receipt)?;
    ensure!(
        Instant::now() < deadline,
        "original Compact FP16 expiry reached during retention"
    );
    Ok(())
}

fn bind_t32_candidate(
    input: &ResourceRunInput,
    profile: &Profile,
    metadata: &Value,
    reference: &ReferenceProof,
    candidate: &CandidateProof,
) -> Result<()> {
    let device = profile
        .device_execution
        .as_ref()
        .context("reviewed Compact FP16 CUDA descriptor missing")?;
    ensure!(
        profile.model == ModelChoice::ContextQwenCompact
            && profile.profile_id == "private-cuda-fp16-compact-efficient-probe-v1"
            && profile.cpu_fit.is_none()
            && input.numeric_fixture.is_none()
            && device.precision == DevicePrecision::Float16
            && device.placement == PlacementPolicy::StrictGpu
            && device.graph == candidate.graph.sha256
            && metadata["reference_input_digest"] == json!(reference.input.input_digest),
        "reviewed Compact FP16 profile, graph or T32 reference differs"
    );
    for (asset, fact) in [
        (
            candidate
                .graph
                .asset("model.onnx", AssetRole::Model, GRAPH_BYTES)?,
            &candidate.graph,
        ),
        (
            candidate
                .weights
                .asset("weights.bin", AssetRole::ExternalWeights, 16 << 30)?,
            &candidate.weights,
        ),
    ] {
        ensure!(
            profile.assets.contains(&asset) && input.files.get(&asset.name) == Some(&fact.path),
            "Compact FP16 native input does not bind the independently verified file"
        );
    }
    Ok(())
}

fn observe_input_refusals(
    scorer: &Scorer,
    input: &CompiledInput,
    receipt: &mut Value,
    file: &mut File,
) -> Result<()> {
    scorer.validate_fp16_probe_input(input)?;
    let mut wrong_positions = input.clone();
    wrong_positions.position_ids[0] = 1;
    let mut hole = input.clone();
    hole.attention_mask[16] = 0;
    let mut all_zero = input.clone();
    all_zero.attention_mask.fill(0);
    for (case, bad, expected) in [
        (
            "wrong_native_positions",
            wrong_positions,
            "G5 original positions/decision/input length differ",
        ),
        (
            "mask_hole",
            hole,
            "G5 Context requires original all-live8192/N8 geometry",
        ),
        (
            "mask_all_zero",
            all_zero,
            "G5 Context requires original all-live8192/N8 geometry",
        ),
    ] {
        let observed = scorer.validate_fp16_probe_input(&bad);
        let error = observed.as_ref().err().map(ToString::to_string);
        receipt["input_refusals"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "case": case, "refused": error.as_deref() == Some(expected),
                "error": error, "native_forward_dispatched": false,
            }));
        flush_graph_diagnostic(file, receipt)?;
        ensure!(
            error.as_deref() == Some(expected),
            "Compact FP16 pre-dispatch input control differs: {case}"
        );
    }
    Ok(())
}

fn t32_reference_bounds(
    (matching_raw, matching_mass, matching_argmax): (f64, f64, bool),
    original_mass: f64,
) -> bool {
    matching_raw <= 0.001 && matching_mass <= 0.05 && matching_argmax && original_mass <= 0.05
}

#[test]
fn compact_fp16_t32_limits_matching_parity_and_original_mass() -> Result<()> {
    let actual = [0.02, 0.0];
    let matching = comparison(&actual, &actual)?;
    let (original_raw, original_mass, original_argmax) = comparison(&actual, &[0.0, 0.02])?;
    assert!(original_raw > 0.001);
    assert!(!original_argmax);
    assert!(t32_reference_bounds(matching, original_mass));

    assert!(t32_reference_bounds((0.001, 0.05, true), 0.05));
    assert!(!t32_reference_bounds((0.0011, 0.05, true), 0.05));
    assert!(!t32_reference_bounds((0.001, 0.051, true), 0.05));
    assert!(!t32_reference_bounds((0.001, 0.05, false), 0.05));
    assert!(!t32_reference_bounds((0.001, 0.05, true), 0.051));
    Ok(())
}

fn read_t32_profile(scorer: &Scorer, prefix: &Path) -> Result<(Value, Vec<u8>)> {
    let path = scorer.finish_profiling()?;
    let leaf = prefix
        .file_name()
        .and_then(|value| value.to_str())
        .context("ORT profile prefix missing")?;
    ensure!(
        path.parent() == prefix.parent()
            && path.extension().is_some_and(|part| part == "json")
            && path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with(leaf)),
        "Compact FP16 ORT profile escaped the checked prefix"
    );
    let bytes = small(&path, 64 << 20)?;
    ensure!(!bytes.is_empty(), "Compact FP16 ORT profile is empty");
    Ok((
        json!({"path": path, "bytes": bytes.len(), "sha256": sha(&bytes)}),
        bytes,
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompactFp16Probe {
    StrictT32,
    GraphOnly,
    QkvProjectionGraphOnly,
    QkvProjectionT32,
    QkvDevelopmentBatch,
    QkvResource20,
    QkvObservedDevelopment,
    QkvFitProbe,
    QkvMixedTiming,
    QkvLayer0T1024,
    QkvDecisionT1024,
    QkvoProjectionGraphOnly,
    QkvoProjectionT32,
    BoundedT32,
    Layer0Taps,
    Layer0ProjectionTaps,
}

#[test]
#[ignore = "owned strict Compact FP16 T32 probe with a reviewed artifact, independent reference and actual CUDA runtime"]
fn cuda_compact_fp16_t32_component() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::StrictT32)
}

#[test]
#[ignore = "disposable Windows Compact FP16 final graph with the reviewed T32 Probe; no scorer or forward"]
fn cuda_compact_fp16_load_only_graph_diagnostic() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::GraphOnly)
}

#[test]
#[ignore = "owned load-only CUDA graph observation for the distinct Compact FP32 Q/K/V projection experiment"]
fn cuda_compact_fp32_qkv_projection_load_only_graph_diagnostic() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvProjectionGraphOnly)
}

#[test]
#[ignore = "owned Compact FP16 T32 scoring with exact final graph, bounded CPU controls and native CUDA profiling"]
fn cuda_compact_fp16_bounded_t32_component() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::BoundedT32)
}

#[test]
#[ignore = "disposable Compact FP16 T32 CUDA diagnostic of the first layer half attention tensors"]
fn cuda_compact_fp16_layer0_taps() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::Layer0Taps)
}

#[test]
#[ignore = "disposable Compact FP16 T32 CUDA diagnostic of the first layer half projections"]
fn cuda_compact_fp16_layer0_projection_taps() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::Layer0ProjectionTaps)
}

#[test]
#[ignore = "one bounded Windows CUDA Compact FP32 Q/K/V score against its exact independent T32 reference"]
fn cuda_compact_fp32_qkv_projection_t32_component() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvProjectionT32)
}

#[test]
#[ignore = "owned load-only CUDA graph observation for the distinct Compact FP32 Q/K/V/O projection experiment"]
fn cuda_compact_fp32_qkvo_projection_load_only_graph_diagnostic() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvoProjectionGraphOnly)
}

#[test]
#[ignore = "owned Compact FP32 Q/K/V/O T32 scoring with exact typed graph and native CUDA profiling"]
fn cuda_compact_fp32_qkvo_projection_t32_component() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvoProjectionT32)
}

#[test]
#[ignore = "owned development-only Compact Q/K/V CUDA comparison at T128/T1024/T8192"]
fn cuda_compact_fp32_qkv_projection_development_batch() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvDevelopmentBatch)
}

#[test]
#[ignore = "owned unprofiled Compact CUDA engineering admission"]
fn cuda_compact_fp16_observed_development() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvObservedDevelopment)
}

#[test]
#[ignore = "owned scientific fit probe: one sealed Compact T8192 call after cold load"]
fn cuda_compact_fp16_fit_probe() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvFitProbe)
}

#[test]
#[ignore = "owned Compact FP16 unprofiled cold load, one warmup and twenty T8192 samples"]
fn cuda_compact_fp16_resource20() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvResource20)
}

#[test]
#[ignore = "owned Compact FP16 unprofiled mixed-width cycles in a single bounded session"]
fn cuda_compact_fp16_mixed_timing() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvMixedTiming)
}

#[test]
#[ignore = "one owned Compact Q/K/V T1024 CUDA forward with exact first-layer FP16 attention taps"]
fn cuda_compact_fp32_qkv_layer0_t1024() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvLayer0T1024)
}

#[test]
#[ignore = "owned normalized decision-row capture on the sealed Compact Q/K/V T1024 input"]
fn cuda_compact_fp32_qkv_decision_t1024() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::QkvDecisionT1024)
}

#[derive(Serialize)]
struct Layer0Tap {
    name: &'static str,
    width: usize,
    bits: Vec<u16>,
}

fn flush_compact_result(
    file: &mut File,
    receipt: &Value,
    wide_taps: Option<&[Layer0Tap]>,
) -> Result<()> {
    if let Some(layer0_taps) = wide_taps {
        #[derive(Serialize)]
        struct Receipt<'a> {
            #[serde(flatten)]
            native: &'a Value,
            layer0_taps: &'a [Layer0Tap],
        }
        flush_graph_diagnostic_with_cap(
            file,
            &Receipt {
                native: receipt,
                layer0_taps,
            },
            LAYER0_T1024_DIAGNOSTIC_BYTES,
        )
    } else {
        flush_graph_diagnostic(file, receipt)
    }
}

fn observe_compact_fp16_t32(mode: CompactFp16Probe) -> Result<()> {
    let (projection_spec, projection_t32, graph_only) = match mode {
        CompactFp16Probe::QkvProjectionGraphOnly => (Some(&QKV), false, true),
        CompactFp16Probe::QkvProjectionT32 => (Some(&QKV), true, false),
        CompactFp16Probe::QkvDevelopmentBatch
        | CompactFp16Probe::QkvObservedDevelopment
        | CompactFp16Probe::QkvFitProbe
        | CompactFp16Probe::QkvResource20
        | CompactFp16Probe::QkvMixedTiming
        | CompactFp16Probe::QkvLayer0T1024
        | CompactFp16Probe::QkvDecisionT1024 => (Some(&QKV), false, false),
        CompactFp16Probe::QkvoProjectionGraphOnly => (Some(&QKVO), false, true),
        CompactFp16Probe::QkvoProjectionT32 => (Some(&QKVO), true, false),
        CompactFp16Probe::GraphOnly => (None, false, true),
        _ => (None, false, false),
    };
    let timing = match mode {
        CompactFp16Probe::QkvFitProbe => Some(timing::Population::Fit),
        CompactFp16Probe::QkvResource20 => Some(timing::Population::Fixed),
        CompactFp16Probe::QkvMixedTiming => Some(timing::Population::Mixed),
        _ => None,
    };
    let representative = mode == CompactFp16Probe::QkvObservedDevelopment;
    let unprofiled = representative || timing.is_some();
    let development_batch = mode == CompactFp16Probe::QkvDevelopmentBatch;
    let decision_rows = mode == CompactFp16Probe::QkvDecisionT1024;
    let qkv_layer0_t1024 = decision_rows || mode == CompactFp16Probe::QkvLayer0T1024;
    let projection_taps = mode == CompactFp16Probe::Layer0ProjectionTaps;
    let layer0_taps = mode == CompactFp16Probe::Layer0Taps || projection_taps || qkv_layer0_t1024;
    let started_ns = airk_process::monotonic::now_ns()?;
    let started = Instant::now();
    let execution_user = airk_ipc::endpoint::current_user()?;
    let default_owner = diagnostic_default_owner()?;
    ensure!(
        execution_user == default_owner,
        "native Compact FP16 default owner differs"
    );
    let (request, request_pin) = request()?;
    let Request::Probe {
        schema,
        gpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    } = request
    else {
        anyhow::bail!("native-emitted Compact FP16 Probe request required");
    };
    ensure!(schema == SCHEMA, "unknown Compact FP16 Probe schema");
    require_initial_bounds(
        &bounds,
        (cold_bytes, warm_bytes, headroom_bytes, job_limit_bytes),
    )?;
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let expires_ns = started_ns
        .checked_add(bounds.deadline_ms * 1_000_000)
        .context("original Compact FP16 monotonic expiry overflow")?;
    let (input, profile, files, _) = inputs_from(&gpu_input)?;
    let capacity = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &profile,
    )?;
    let metadata_pin = env_pin(
        "AIRK_CUDA_FP16_METADATA_RECEIPT",
        "AIRK_CUDA_FP16_METADATA_RECEIPT_SHA256",
    )?;
    let metadata: Value = serde_json::from_slice(&pinned(&metadata_pin, 1 << 20)?)?;
    ensure!(
        metadata["schema"]
            == projection_spec.map_or("airk.private.compact-fp16-probe-metadata/1", |spec| spec
                .metadata_schema)
            && metadata["probe_request"] == json!(request_pin)
            && metadata["native_input"] == json!(gpu_input)
            && metadata["profile"] == json!(profile.manifest)
            && metadata["profile_pin"] == json!(input.profile)
            && metadata["source_manifest"] == json!(input.source_manifest)
            && metadata["runtime_manifest"] == json!(runtime_manifest)
            && metadata["execution_user"] == execution_user
            && metadata["qualified"] == false
            && metadata["model_requests_dispatched"] == false
            && metadata["runtime_admission_dispatched"] == false,
        "Compact FP16 Probe does not bind the exact native metadata receipt"
    );
    let proof_pin: ResourceFilePin = serde_json::from_value(metadata["independent_proof"].clone())?;
    let proof: IndependentProof = serde_json::from_slice(&pinned(&proof_pin, PROOF_BYTES)?)?;
    validate_independent_proof(&proof)?;
    validate_reference(&proof.reference, &profile, &input.policy)?;
    let projection = projection_spec
        .map(|spec| projection_stage(spec, &proof))
        .transpose()?;
    let candidate = projection
        .as_ref()
        .map_or(&proof.candidate, |(_, candidate)| candidate);
    if let Some((stage, _)) = &projection {
        ensure!(
            metadata["projection_stage"] == json!(stage),
            "Compact FP32 Q/K/V metadata stage differs"
        );
    }
    let matching_projection = if projection_t32 {
        let spec = projection_spec.context("missing Compact projection scope")?;
        let (stage, candidate) = projection
            .as_ref()
            .context("missing Compact projection stage")?;
        let matching = projection_reference(spec, stage, candidate, &proof)?;
        validate_reference(&matching.1, &profile, &input.policy)?;
        Some(matching)
    } else {
        None
    };
    let development = if development_batch || qkv_layer0_t1024 || unprofiled {
        let (stage, candidate) = projection
            .as_ref()
            .context("missing development Q/K/V stage")?;
        Some(development_references(
            stage,
            candidate,
            &proof,
            &profile,
            &input.policy,
        )?)
    } else {
        None
    };
    let reference = matching_projection
        .as_ref()
        .map_or(&proof.reference, |(_, reference)| reference);
    bind_t32_candidate(&input, &profile, &metadata, reference, candidate)?;
    let reviewed_graph = if mode == CompactFp16Probe::BoundedT32
        || layer0_taps
        || projection_t32
        || development_batch
        || unprofiled
    {
        let pin = env_pin(
            "AIRK_CUDA_FP16_REVIEWED_GRAPH",
            "AIRK_CUDA_FP16_REVIEWED_GRAPH_SHA256",
        )?;
        let expected = if projection_t32 || development_batch || qkv_layer0_t1024 || unprofiled {
            projection_spec
                .context("missing Compact projection scope")?
                .native_graph_sha
                .context("reviewed Compact projection native graph pin unavailable")?
        } else {
            REVIEWED_FP16_GRAPH_SHA256
        };
        ensure!(
            pin.sha256 == expected.parse::<Sha256Digest>()?,
            "unreviewed Compact FP16 graph evidence"
        );
        let bytes = pinned(&pin, PROOF_BYTES)?;
        Some((pin, bytes))
    } else {
        None
    };
    pinned(&runtime_manifest, 128 << 10)?;
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    ensure!(
        manifest.as_path() == Path::new(&runtime_manifest.path),
        "Compact FP16 manifest differs from the guarded runtime directory"
    );

    let output = output()?;
    let observed_proof = representative
        .then(|| observed::Observed::load(candidate))
        .transpose()?;
    let timing_proof = timing.map(|_| timing::Sealed::load()).transpose()?;
    let prefix = if graph_only || unprofiled {
        ensure!(
            output.is_absolute(),
            "absolute Compact FP16 diagnostic output required"
        );
        None
    } else {
        let prefix = PathBuf::from(text("AIRK_CUDA_FP16_PROFILE_PREFIX")?);
        ensure!(
            output.is_absolute()
                && prefix.is_absolute()
                && prefix.parent() == output.parent()
                && prefix.file_name().is_some()
                && prefix != output
                && !prefix.exists(),
            "fresh absolute output and guarded Compact FP16 profile prefix required"
        );
        Some(prefix)
    };
    let binary = file_hash(&std::env::current_exe()?)?;
    let actual_job = job_limits(bounds.host_limit_bytes, job_limit_bytes)?;
    let mut resources = airk_kernel::service_resources::Resources::detect(ResourceConfiguration {
        preset: ResourcePreset::Throughput,
        ..ResourceConfiguration::default()
    })?;
    let available = airk_kernel::service_resource_observations::available_memory();
    resources.observe(
        airk_kernel::service_resource_observations::rss(std::process::id()),
        available,
        true,
    );
    let incarnation = Incarnation::from_bytes(request_pin.sha256.as_bytes()[..16].try_into()?);
    let charge =
        Arc::new(resources.reserve_optional(incarnation, bounds.host_limit_bytes, available)?);
    let release = charge.release_observer();
    let (parent, control) = private_control()?;
    let mut control_parent = Some(parent);
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = Arc::new(airk_kernel::inference_owner::NativeInferenceOwner::admit(
        lease.clone(),
    )?);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let mut receipt = json!({
        "schema": match mode {
            CompactFp16Probe::StrictT32 => "airk.private.compact-fp16-t32-component/2",
            CompactFp16Probe::GraphOnly => "airk.private.compact-fp16-graph-diagnostic/1",
            CompactFp16Probe::QkvProjectionGraphOnly => QKV.graph_schema,
            CompactFp16Probe::QkvProjectionT32 => QKV.t32_schema,
            CompactFp16Probe::QkvDevelopmentBatch => "airk.private.compact-fp32-qkv-development-batch/1",
            CompactFp16Probe::QkvObservedDevelopment => "airk.private.compact-qkv-observed-development/1",
            CompactFp16Probe::QkvResource20 => "airk.private.compact-fp16-unprofiled-resource20/1",
            CompactFp16Probe::QkvFitProbe => "airk.private.compact-fp16-unprofiled-fit/1",
            CompactFp16Probe::QkvMixedTiming => "airk.private.compact-fp16-unprofiled-mixed/1",
            CompactFp16Probe::QkvDecisionT1024 => "airk.private.compact-fp32-qkv-decision-t1024/1",
            CompactFp16Probe::QkvLayer0T1024 => "airk.private.compact-fp32-qkv-layer0-t1024/1",
            CompactFp16Probe::QkvoProjectionGraphOnly => QKVO.graph_schema,
            CompactFp16Probe::QkvoProjectionT32 => QKVO.t32_schema,
            CompactFp16Probe::BoundedT32 => "airk.private.compact-fp16-bounded-t32-component/1",
            CompactFp16Probe::Layer0Taps => "airk.private.compact-fp16-layer0-taps/1",
            CompactFp16Probe::Layer0ProjectionTaps => "airk.private.compact-fp16-layer0-projection-taps/1",
        },
        "complete": false, "qualified": false, "product_admission": false,
        "request": request_pin, "native_input": gpu_input, "profile": profile.manifest,
        "source_manifest": input.source_manifest, "runtime_manifest": runtime_manifest,
        "metadata_receipt": metadata_pin, "independent_proof": proof_pin,
        "pid": std::process::id(), "native_binary_sha256": binary, "execution_user": execution_user,
        "default_owner": default_owner, "bounds": bounds, "actual_job": actual_job,
        "native_host_reservation_bytes": charge.bytes(), "resource_snapshot": resources.snapshot(),
        "original_expires_monotonic_ns": expires_ns, "runtime_profile": null, "load_logs": null, "rows": [],
        "input_refusals": [], "numeric_forward_attempts": 0, "session_teardown_observed": false,
        "device_teardown_observed": false,
        "native_forward_returned": false, "forward_synchronized": false,
        "source_mask": {
            "independent_complete_graph_proof": proof_pin,
            "verified_candidate_graph": candidate.graph.sha256,
            "compact_graph_uses_attention_mask_shape_only": true,
            "mask_values_require_all_live_before_dispatch": true,
        },
        "measured_peak_device_bytes": null, "native_backend_attested": false,
    });
    if let Some(row) = &observed_proof {
        receipt["profiling_enabled"] = json!(false);
        receipt["observed_reference"] = json!(row.reference_pin);
        receipt["observed_capture"] = json!(row.capture_pin);
        receipt["observed_census"] = json!(row.census_pin);
    }
    if let Some(sealed) = &timing_proof {
        receipt["profiling_enabled"] = json!(false);
        receipt["timing_score_sources"] = json!(sealed.pins);
    }
    if graph_only {
        receipt["graph_observation"] = Value::Null;
    }
    if let Some((stage, _)) = &projection {
        receipt["projection_stage"] = json!(stage);
    }
    if let Some((pin, _)) = &matching_projection {
        receipt["projection_reference"] = json!(pin);
    }
    if let Some(references) = &development {
        receipt["projection_reference"] = json!(references.pin);
        receipt["reference_basis"] = json!("independent_wider_qkv_cpu_approximation");
        receipt["reference_recipe"] = references.recipe.clone();
        receipt["development_only"] = json!(true);
    }
    if development_batch {
        receipt["qualification_cutoff_raw"] = json!(0.001);
        receipt["development_raw_limit"] = json!(DEVELOPMENT_RAW_LIMIT);
        receipt["matching_qkv_mass_limit"] = json!(DEVELOPMENT_MASS_LIMIT);
        receipt["original_fp32_mass_limit"] = json!(DEVELOPMENT_MASS_LIMIT);
        receipt["near_tie_margin_limit"] = json!(DEVELOPMENT_NEAR_TIE_LIMIT);
        receipt["development_screen_pass"] = json!(false);
        receipt["profiled_runs"] = json!([]);
    }
    if layer0_taps {
        receipt["layer0_final_graph"] = Value::Null;
        receipt["layer0_taps"] = json!([]);
    }
    if let Some((pin, _)) = &reviewed_graph {
        receipt["reviewed_graph"] = json!(pin);
        receipt["bounded_policy_identity"] = Value::Null;
        receipt["bounded_profile"] = Value::Null;
    }
    flush_graph_diagnostic(&mut file, &receipt)?;
    let mut journal = operation::Journal::create_until(
        &output,
        operation::Identity {
            input_sha256: request_pin.sha256,
            profile: profile.manifest,
            process_id: std::process::id(),
            native_binary_sha256: binary,
        },
        expires_ns,
    )?;
    let mut wide_taps = Vec::new();
    let run = owner.run(|| -> Result<()> {
        let mut custody = Custody {
            scorer: None, device: None, owner: owner.clone(), lease: lease.clone(), charge: Some(charge),
            control: &mut control_parent, files: Some(files),
        };
        let work = (|| -> Result<()> {
            ensure!(Instant::now() < deadline, "original Compact FP16 deadline expired before admission");
            let files = custody.files.as_ref().context("Compact FP16 native asset custody missing")?;
            verify_files(files, &profile)?;
            let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
            ensure!(runtime.manifest_identity() == runtime_manifest.sha256, "actual Compact FP16 runtime manifest differs");
            let adapters = runtime.adapters(deadline)?;
            ensure!(adapters.len() == 1, "Compact FP16 probe requires exactly one observed adapter");
            let device = runtime.device(adapters[0].uuid, deadline)?;
            custody.device = Some(device.clone());
            receipt["adapter_before"] = json!(device.observation(deadline)?);
            flush_graph_diagnostic(&mut file, &receipt)?;
            let catalog = airk_kernel::inference_owner::evaluation_catalog(lease.clone(), vec![profile.clone()])?;
            let execution = catalog.execution_profile(profile.manifest)?.bind_cuda_probe(device, capacity, deadline)?;
            if graph_only {
                let observer = execution.cuda_graph_observer()?;
                let loaded = journal.observe(operation::Kind::Load, 0, None, || {
                    if projection_spec.is_some() {
                        Scorer::load_private_compact_fp32_qkv_projection_diagnostic(
                            execution, files, proof.candidate.graph.sha256, candidate.graph.sha256,
                        )
                    } else {
                        Scorer::diagnose_cuda_graph_metadata(execution, files)
                    }
                })?;
                receipt["load_us"] = json!(loaded.elapsed_us);
                receipt["load_native_call_us"] = json!(loaded.native_call_elapsed_us);
                let snapshot = observer.snapshot()?;
                receipt["graph_observation"] = serde_json::to_value(&snapshot)?;
                receipt["session_teardown_observed"] = json!(snapshot
                    .teardown
                    .as_ref()
                    .is_some_and(|teardown| teardown.validate().is_ok()));
                flush_graph_diagnostic(&mut file, &receipt)?;
                loaded.value?;
                ensure!(loaded.within_deadline, "original Compact FP16 graph load expiry exceeded");
                return snapshot.validate_graph_metadata();
            };
            let observer = execution.cuda_compact_fp16_load_observer()?;
            let loaded = journal.observe(operation::Kind::Load, 0, None, || {
                if unprofiled {
                    let (pin, graph_bytes) = reviewed_graph.as_ref().context("missing timing graph custody")?;
                    return Scorer::open_private_compact_fp32_qkv_unprofiled(execution, files, graph_bytes, pin.sha256);
                }
                let profile_prefix = prefix.as_deref().context("scoring profile prefix missing")?;
                if let Some((pin, graph_bytes)) = &reviewed_graph {
                    if mode == CompactFp16Probe::QkvoProjectionT32 {
                        Scorer::open_private_compact_fp32_qkvo_projection_profiled(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    } else if decision_rows {
                        Scorer::open_private_compact_fp32_qkv_decision_t1024(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    } else if qkv_layer0_t1024 {
                        Scorer::open_private_compact_fp32_qkv_layer0_t1024(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    } else if mode == CompactFp16Probe::QkvProjectionT32 || development_batch {
                        Scorer::open_private_compact_fp32_qkv_projection_profiled(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    } else if projection_taps {
                        Scorer::open_compact_fp16_layer0_projection_taps(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    } else if layer0_taps {
                        Scorer::open_compact_fp16_layer0_taps(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    } else {
                        Scorer::open_compact_fp16_bounded_hybrid_profiled(
                            execution, files, profile_prefix, graph_bytes, pin.sha256,
                        )
                    }
                } else {
                    Scorer::open_profiled(execution, files, profile_prefix)
                }
            })?;
            receipt["load_us"] = json!(loaded.elapsed_us);
            receipt["load_native_call_us"] = json!(loaded.native_call_elapsed_us);
            let logs = observer.snapshot();
            if let Ok(snapshot) = &logs {
                receipt["load_logs"] = json!(snapshot.logs);
            }
            custody.scorer = Some(loaded.value?);
            logs?;
            ensure!(loaded.within_deadline, "original Compact FP16 load expiry exceeded");
            let scorer = custody.scorer.as_ref().context("Compact FP16 scorer missing")?;
            receipt["execution_identity"] = json!(scorer.execution_identity()?);
            receipt["final_placement"] = json!(scorer.device_placement()?.context("actual Compact FP16 final placement missing")?);
            receipt["attention_plan"] = if reviewed_graph.is_some() {
                let (graph, policy) = scorer.compact_fp16_bounded_hybrid_evidence()?;
                if layer0_taps {
                    receipt["layer0_final_graph"] = serde_json::to_value(graph)?;
                }
                receipt["bounded_policy_identity"] = json!(policy);
                scorer.compact_fp16_bounded_hybrid_attention_plan()?
            } else {
                scorer.attention_backend_plan()?.context("actual Compact FP16 attention plan missing")?
            };
            flush_graph_diagnostic(&mut file, &receipt)?;
            if let Some(observation) = &observed_proof {
                let compiler = observed::compiler(&profile, files)?;
                return observation.score(scorer, &compiler, &mut journal, &mut receipt, &mut file, deadline);
            }
            if development_batch || timing.is_some() {
                let references = development.as_ref().context("development reference custody missing")?;
                let device = custody.device.as_ref().context("development CUDA device custody missing")?;
                return score_development_rows(
                    scorer, device, timing::Run { references, short: &proof.reference,
                        population: timing.zip(timing_proof.as_ref()) },
                    &mut journal, &mut receipt, &mut file, deadline,
                );
            }
            let reference = if qkv_layer0_t1024 {
                &development.as_ref().context("T1024 reference custody missing")?.rows[1]
            } else {
                reference
            };
            observe_input_refusals(scorer, &reference.input, &mut receipt, &mut file)?;
            receipt["numeric_forward_attempts"] = json!(1);
            flush_graph_diagnostic(&mut file, &receipt)?;
            let scored = journal.observe(operation::Kind::Numeric, 1, Some(reference.input.input_digest), || {
                if mode == CompactFp16Probe::QkvoProjectionT32 {
                    scorer
                        .score_private_compact_fp32_qkvo_projection_t32(&reference.input, deadline)
                        .map(|scores| (scores, None))
                } else if mode == CompactFp16Probe::QkvProjectionT32 {
                    scorer
                        .score_private_compact_fp32_qkv_projection_t32(&reference.input, deadline)
                        .map(|scores| (scores, None))
                } else if layer0_taps {
                    scorer
                        .score_layer0_taps(&reference.input, deadline)
                        .map(|(scores, taps)| (scores, Some(taps)))
                } else {
                    scorer
                        .score_with_deadline(&reference.input, deadline)
                        .map(|scores| (scores, None))
                }
            })?;
            let (actual, taps) = scored.value?;
            receipt["native_forward_returned"] = json!(true);
            if qkv_layer0_t1024 {
                let (roster, retained_rows) = if decision_rows {
                    (Scorer::COMPACT_QKV_DECISION_TAPS, 1)
                } else {
                    (airk_retrieval::arbiter::runtime::COMPACT_FP16_LAYER0_TAPS, 1024)
                };
                let taps = taps.context("Compact Q/K/V T1024 attention outputs missing")?;
                ensure!(
                    taps.len() == roster.len()
                        && taps.iter().zip(roster).all(|(bits, &(_, width))| bits.len() == retained_rows * width)
                        && actual.len() == 8
                        && actual.iter().all(|score| score.is_finite()),
                    "Compact Q/K/V T1024 outputs differ"
                );
                wide_taps = roster.iter().zip(taps)
                    .map(|(&(name, width), bits)| Layer0Tap { name, width, bits })
                    .collect();
                receipt["rows"].as_array_mut().unwrap().push(json!({
                    "name": reference.name, "tokens": 1024, "input_digest": reference.input.input_digest,
                    "raw_scores": actual, "elapsed_us": scored.elapsed_us,
                    "native_call_elapsed_us": scored.native_call_elapsed_us,
                }));
                flush_graph_diagnostic(&mut file, &receipt)?;
                ensure!(scored.within_deadline, "original Compact Q/K/V T1024 numeric expiry exceeded");
                return Ok(());
            }
            if let Some(taps) = taps {
                let roster = if projection_taps {
                    airk_retrieval::arbiter::runtime::COMPACT_FP16_LAYER0_PROJECTION_TAPS
                } else {
                    airk_retrieval::arbiter::runtime::COMPACT_FP16_LAYER0_TAPS
                };
                ensure!(taps.len() == roster.len(), "Compact FP16 layer-0 tap roster differs");
                receipt["layer0_taps"] = json!(roster
                    .iter()
                    .zip(taps)
                    .map(|(&(name, width), bits)| json!({"name": name, "width": width, "bits": bits}))
                    .collect::<Vec<_>>());
            }
            let (half_raw, half_mass, half_winner) = comparison(&actual, &reference.matching_fp16)?;
            let (original_raw, original_mass, original_winner) = comparison(&actual, &reference.original_fp32)?;
            receipt["rows"].as_array_mut().unwrap().push(json!({
                "name": reference.name, "tokens": 32, "input_digest": reference.input.input_digest,
                "raw_scores": actual, "matching_fp16_raw_error": half_raw, "matching_fp16_mass_error": half_mass,
                "matching_fp16_argmax_matches": half_winner, "original_fp32_raw_error": original_raw,
                "original_fp32_mass_error": original_mass, "original_fp32_argmax_matches": original_winner,
                "elapsed_us": scored.elapsed_us, "native_call_elapsed_us": scored.native_call_elapsed_us,
            }));
            flush_graph_diagnostic(&mut file, &receipt)?;
            if layer0_taps {
                ensure!(scored.within_deadline, "original Compact FP16 numeric expiry exceeded");
            } else {
                ensure!(
                    scored.within_deadline
                        && t32_reference_bounds((half_raw, half_mass, half_winner), original_mass),
                    "Compact FP16 T32 matching parity, original mass or time bound refused"
                );
            }
            Ok(())
        })();

        let mut teardown = Ok(());
        let session_opened = custody.scorer.is_some();
        if unprofiled && let Some(scorer) = custody.scorer.take() {
            teardown = scorer.synchronize(deadline).and_then(|()| {
                ensure!(scorer.device_settled(), "unprofiled session has pending GPU work");
                receipt["forward_synchronized"] = json!(receipt["native_forward_returned"] == true);
                Ok(())
            });
        }
        if let Some(scorer) = custody.scorer.take() {
            let profile_capture = scorer.synchronize(deadline).and_then(|()| {
                ensure!(scorer.device_settled(), "Compact FP16 score remains unsettled");
                receipt["forward_synchronized"] = json!(receipt["native_forward_returned"] == true);
                read_t32_profile(
                    &scorer,
                    prefix.as_deref().context("Compact FP16 scorer profile prefix missing")?,
                )
            });
            match profile_capture {
                Ok((trace, bytes)) => {
                    receipt["runtime_profile"] = trace;
                    if reviewed_graph.is_some() {
                        let profile_summary = scorer.compact_fp16_bounded_hybrid_evidence().and_then(|(graph, _)| {
                            if development_batch {
                                let rows = receipt["rows"].as_array().context("development row receipt missing")?;
                                let widths = rows.iter().map(|row| {
                                    row["tokens"].as_u64().and_then(|n| usize::try_from(n).ok())
                                        .context("development row width missing")
                                }).collect::<Result<Vec<_>>>()?;
                                let summary = super::hybrid_profile_trace::compact_fp16_development_profiled_runs(
                                    &bytes, &graph.rows, &widths,
                                )?;
                                return Ok((true, serde_json::to_value(summary)?));
                            }
                            if qkv_layer0_t1024 {
                                let summary = super::hybrid_profile_trace::compact_qkv_layer0_t1024_profiled_run(
                                    &bytes, &graph.rows,
                                )?;
                                return Ok((false, serde_json::to_value(summary)?));
                            }
                            let expected_nodes = match mode {
                                CompactFp16Probe::QkvProjectionT32 => 1942,
                                // Native admission already matched every typed row and counter
                                // to the exact reviewed load-only receipt.
                                CompactFp16Probe::QkvoProjectionT32 => graph.rows.iter()
                                    .filter(|row| matches!(row, cuda::GraphMetadataRow::Node { .. }))
                                    .count(),
                                _ => 1746,
                            };
                            let summary = super::hybrid_profile_trace::compact_fp16_t32_profiled_run(
                                &bytes, &graph.rows, expected_nodes,
                            )?;
                            Ok((false, serde_json::to_value(summary)?))
                        });
                        match profile_summary {
                            Ok((true, summary)) => receipt["profiled_runs"] = summary,
                            Ok((false, summary)) => receipt["bounded_profile"] = summary,
                            Err(error) => {
                                receipt["bounded_profile_error"] = json!(format!("{error:#}"));
                                teardown = Err(error);
                            }
                        }
                    }
                }
                Err(error) => {
                    receipt["runtime_profile_error"] = json!(format!("{error:#}"));
                    teardown = Err(error);
                }
            }
            drop(scorer);
        }
        if let Some(device) = custody.device.as_ref() {
            let release = device.synchronize(deadline).and_then(|()| {
                ensure!(!device.poisoned() && !cuda::process_poisoned(), "Compact FP16 teardown remains unknown");
                if graph_only {
                    ensure!(device.pending_claims() == Some(0), "Compact FP16 graph device claims remain unsettled");
                } else {
                    receipt["session_teardown_observed"] = json!(session_opened);
                }
                receipt["device_teardown_observed"] = json!(true);
                receipt["adapter_after"] = json!(device.observation(deadline)?);
                Ok(())
            });
            if let Err(error) = &release {
                receipt["teardown_error"] = json!(format!("{error:#}"));
            }
            teardown = teardown.and(release);
        }
        if let Some((pin, _)) = &reviewed_graph {
            teardown = teardown.and(pinned(pin, PROOF_BYTES).map(|_| ()));
        }
        if let Some((pin, _)) = &matching_projection {
            teardown = teardown.and(pinned(pin, PROOF_BYTES).map(|_| ()));
        }
        if let Some(references) = &development {
            teardown = teardown.and(pinned(&references.pin, PROOF_BYTES).map(|_| ()));
            for row in &references.rows {
                for pin in [row.fixture.pin(), row.producer.pin()] {
                    teardown = teardown.and(pinned(&pin, PROOF_BYTES).map(|_| ()));
                }
            }
        }
        if let Some(sealed) = &timing_proof {
            for pin in &sealed.pins { teardown = teardown.and(pinned(pin, PROOF_BYTES).map(|_| ())); }
        }
        if let Some(row) = &observed_proof {
            for pin in [&row.reference_pin, &row.capture_pin, &row.census_pin] {
                teardown = teardown.and(pinned(pin, PROOF_BYTES).map(|_| ()));
            }
        }
        work.and(teardown)?;
        verify_files(custody.files.as_ref().context("Compact FP16 post-input custody missing")?, &profile)?;
        for (pin, maximum) in [
            (&request_pin, 1 << 20), (&gpu_input, 1 << 20), (&input.profile, 1 << 20),
            (&metadata_pin, 1 << 20), (&runtime_manifest, 128 << 10),
            (&input.source_manifest, PROOF_BYTES), (&proof_pin, PROOF_BYTES),
            (&proof.reference.fixture.pin(), PROOF_BYTES), (&proof.reference.producer.pin(), PROOF_BYTES),
        ] {
            pinned(pin, maximum)?;
        }
        ensure!(Instant::now() < deadline, "original Compact FP16 expiry exceeded during post-input proofs");
        Ok(())
    }).and_then(std::convert::identity);

    drop(owner);
    drop(lease);
    let retained = control_parent.is_none();
    let cleanup = control_parent
        .take()
        .map(tempfile::TempDir::close)
        .transpose();
    let cleaned = matches!(&cleanup, Ok(Some(())));
    let mut wait = std::pin::pin!(release.wait());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let released = matches!(
        std::future::Future::poll(wait.as_mut(), &mut cx),
        std::task::Poll::Ready(Ok(()))
    );
    let poisoned = cuda::process_poisoned();
    let peak = peak_rss();
    receipt["native_control_retained_for_unknown_cuda"] = json!(retained);
    receipt["native_control_cleanup_observed"] = json!(cleaned);
    receipt["native_control_cleanup_error"] =
        json!(cleanup.as_ref().err().map(ToString::to_string));
    receipt["native_host_charge_release_observed"] = json!(released);
    receipt["process_cuda_poisoned"] = json!(poisoned);
    receipt["peak_process_rss_bytes"] = json!(peak);
    let result = run.and_then(|()| {
        ensure!(
            receipt["session_teardown_observed"] == true
                && !retained
                && cleaned
                && released
                && !poisoned,
            "Compact FP16 session or host release unproven"
        );
        final_host_peak(peak, bounds.host_limit_bytes)?;
        Ok(())
    });
    receipt["complete"] = json!(result.is_ok());
    receipt["elapsed_us_to_final_report_start"] = json!(started.elapsed().as_micros());
    if let Err(error) = &result {
        receipt["error"] = json!(format!("{error:#}"));
        if let Some(refusal) = error.downcast_ref::<cuda::PlacementRefusal>() {
            receipt["placement_refusal"] = json!(&refusal.observation);
        }
        if let Some(refusal) =
            error.downcast_ref::<airk_retrieval::arbiter::runtime::CompactFp16Layer0GraphRefusal>()
        {
            receipt["layer0_final_graph"] = serde_json::to_value(&refusal.observation)?;
        }
    }
    let wide_taps = if qkv_layer0_t1024 {
        receipt.as_object_mut().unwrap().remove("layer0_taps");
        Some(wide_taps.as_slice())
    } else {
        None
    };
    flush_compact_result(&mut file, &receipt, wide_taps)?;
    if Instant::now() >= deadline {
        receipt["complete"] = json!(false);
        receipt["final_report_retention_exceeded_original_deadline"] = json!(true);
        flush_compact_result(&mut file, &receipt, wide_taps)?;
        return result.and_then(|()| {
            anyhow::bail!("original Compact FP16 expiry exceeded during final report retention")
        });
    }
    result
}
