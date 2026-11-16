// SPDX-License-Identifier: Apache-2.0
//! Complete native tokenizer diagnostic; no graph execution or qualification.
use crate::arbiter::{
    compiler::{Compiler, qwen},
    context_action::prompt_tests::{compact_for_census, legacy_prompt},
    tasks::Prompt,
};
use airk_core::{arbiter::CompilerProfile, native_context_action::NativeContextActionRequest};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::PathBuf};

const DIAGNOSTIC_TOKEN_CEILING: usize = 8192;

fn sha(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}

#[derive(Serialize)]
struct ViewMeasurement {
    status: &'static str,
    input_tokens: Option<usize>,
    compiled_input_digest: Option<String>,
    fits_original_remaining: Option<bool>,
    fits_2048: Option<bool>,
    fits_diagnostic_8192: Option<bool>,
    raw_refusal_detail: Option<String>,
}
impl ViewMeasurement {
    fn refused(status: &'static str, error: &anyhow::Error) -> Self {
        Self {
            status,
            input_tokens: None,
            compiled_input_digest: None,
            fits_original_remaining: None,
            fits_2048: None,
            fits_diagnostic_8192: None,
            raw_refusal_detail: Some(format!("{error:#}")),
        }
    }
}

fn measure(compiler: &Compiler, prompt: Result<Prompt>, remaining: usize) -> ViewMeasurement {
    let prompt = match prompt {
        Ok(prompt) => prompt,
        Err(error) => return ViewMeasurement::refused("prompt_refused", &error),
    };
    match compiler.compile(&prompt.state, &prompt.question, &prompt.candidates) {
        Ok(input) => {
            let tokens = input.input_ids.len();
            ViewMeasurement {
                status: "compiled",
                input_tokens: Some(tokens),
                compiled_input_digest: Some(input.input_digest.to_string()),
                fits_original_remaining: Some(tokens <= remaining),
                fits_2048: Some(tokens <= 2048),
                fits_diagnostic_8192: Some(tokens <= DIAGNOSTIC_TOKEN_CEILING),
                raw_refusal_detail: None,
            }
        }
        Err(error) => ViewMeasurement::refused("compiler_refused", &error),
    }
}

#[derive(Default, Serialize)]
struct Counts {
    selected: usize,
    decoded: usize,
    decode_refused: usize,
    structural_refused: usize,
    old_compiled: usize,
    old_refused: usize,
    compact_compiled: usize,
    compact_refused: usize,
    comparable: usize,
    compact_grew: usize,
    compact_equal: usize,
    compact_shrank: usize,
}

/// Retain all rows, including growth and missing counts from any refusal. The
/// unchanged older census asserts shrinkage and remains a separate regression.
#[test]
#[ignore = "exact pinned native requests, publisher tokenizer and fresh private output required"]
fn complete_context_prompt_census_records_growth_and_refusals() -> Result<()> {
    let input = PathBuf::from(std::env::var("AIRK_CONTEXT_CENSUS_REQUESTS")?);
    let tokenizer_path = PathBuf::from(std::env::var("AIRK_QWEN_TOKENIZER")?);
    let output = PathBuf::from(std::env::var("AIRK_CONTEXT_CENSUS_OUTPUT")?);
    ensure!(
        output.is_absolute() && !output.exists() && output != input && output != tokenizer_path,
        "fresh absolute private census output required"
    );
    let raw_requests = fs::read(&input)?;
    let tokenizer = fs::read(&tokenizer_path)?;
    ensure!(
        raw_requests.len() <= 64 << 20 && tokenizer.len() <= 64 << 20,
        "census input byte bound"
    );
    let request_sha = sha(&raw_requests);
    let tokenizer_sha = sha(&tokenizer);
    ensure!(
        request_sha == std::env::var("AIRK_CONTEXT_CENSUS_REQUESTS_SHA256")?
            && tokenizer_sha == std::env::var("AIRK_QWEN_TOKENIZER_SHA256")?,
        "census input/tokenizer differs from the reviewed exact identity"
    );
    let requests: Vec<Value> = serde_json::from_slice(&raw_requests)?;
    ensure!(
        !requests.is_empty() && requests.len() <= 10_000,
        "bounded nonempty native request population required"
    );
    let original_profile = qwen::compiler_profile(&tokenizer)?;
    let mut diagnostic_profile: CompilerProfile = serde_json::from_slice(&original_profile)?;
    diagnostic_profile.max_tokens = DIAGNOSTIC_TOKEN_CEILING;
    diagnostic_profile.validate()?;
    let diagnostic_profile = serde_json::to_vec(&diagnostic_profile)?;
    let compiler = Compiler::from_verified(&diagnostic_profile, &tokenizer)?;
    let mut counts = Counts::default();
    let mut rows = Vec::with_capacity(requests.len());
    for (position, original_row) in requests.iter().enumerate() {
        counts.selected += 1;
        let original_request = original_row.get("request").unwrap_or(original_row);
        let request: NativeContextActionRequest =
            match serde_json::from_value(original_request.clone()) {
                Ok(request) => request,
                Err(error) => {
                    counts.decode_refused += 1;
                    counts.old_refused += 1;
                    counts.compact_refused += 1;
                    let error = anyhow::Error::from(error);
                    rows.push(json!({"position":position,"original_row":original_row,
                    "old":ViewMeasurement::refused("decode_refused",&error),
                    "compact":ViewMeasurement::refused("decode_refused",&error),
                    "compact_minus_old_tokens":null,"compact_grew":null}));
                    continue;
                }
            };
        counts.decoded += 1;
        // The legacy helper slices exact source windows. Validate those windows
        // first so malformed rows become explicit refusals instead of panics.
        let structure = request.roster.validate(&request.records);
        let (old, compact) = match structure {
            Err(error) => {
                counts.structural_refused += 1;
                (
                    ViewMeasurement::refused("structural_refused", &error),
                    ViewMeasurement::refused("structural_refused", &error),
                )
            }
            Ok(()) => (
                measure(
                    &compiler,
                    legacy_prompt(
                        &request.question,
                        &request.records,
                        &request.choices,
                        &request.roster,
                    ),
                    request.remaining_input_tokens,
                ),
                measure(
                    &compiler,
                    compact_for_census(&request),
                    request.remaining_input_tokens,
                ),
            ),
        };
        counts.old_compiled += usize::from(old.input_tokens.is_some());
        counts.old_refused += usize::from(old.input_tokens.is_none());
        counts.compact_compiled += usize::from(compact.input_tokens.is_some());
        counts.compact_refused += usize::from(compact.input_tokens.is_none());
        let delta = match (old.input_tokens, compact.input_tokens) {
            (Some(old), Some(compact)) => {
                counts.comparable += 1;
                counts.compact_grew += usize::from(compact > old);
                counts.compact_equal += usize::from(compact == old);
                counts.compact_shrank += usize::from(compact < old);
                Some(i64::try_from(compact)? - i64::try_from(old)?)
            }
            _ => None,
        };
        rows.push(json!({"position":position,"original_row":original_row,
            "original_parent_budget":request.parent_budget,
            "original_remaining_input_tokens":request.remaining_input_tokens,
            "original_remaining_ms":request.remaining_ms,
            "original_roster_cards":request.roster.cards.len(),
            "old":old,"compact":compact,"compact_minus_old_tokens":delta,
            "compact_grew":delta.map(|delta|delta>0)}));
    }
    ensure!(
        counts.selected == requests.len()
            && rows.len() == requests.len()
            && counts.old_compiled + counts.old_refused == requests.len()
            && counts.compact_compiled + counts.compact_refused == requests.len()
            && counts.compact_grew + counts.compact_equal + counts.compact_shrank
                == counts.comparable,
        "census population accounting mismatch"
    );
    let inputs_unchanged =
        sha(&fs::read(&input)?) == request_sha && sha(&fs::read(&tokenizer_path)?) == tokenizer_sha;
    let report = json!({"schema":"airk.private-complete-native-context-census/1",
        "complete":true,"inputs_unchanged":inputs_unchanged,
        "requests_file":input,"requests_sha256":request_sha,
        "tokenizer_file":tokenizer_path,"tokenizer_sha256":tokenizer_sha,
        "original_emitted_compiler_sha256":sha(&original_profile),
        "diagnostic_compiler_sha256":sha(&diagnostic_profile),
        "diagnostic_compiler_token_ceiling":DIAGNOSTIC_TOKEN_CEILING,
        "native_compiler_policy":qwen::policy_digest()?,
        "counts":counts,"rows":rows,
        "graph_executions":0,"training_steps":0,"qualified":false,
        "scope":"Complete retained native request population, unchanged original budgets, current native prompt helpers and compiler. Diagnostic metadata width8192 measures successful compiles only; no graph/profile/resource fit or current request authority is certified. Structural/prompt/token refusals have null counts, never guessed counts, cropping or omitted rows. Compact growth is an observation, not a failed measurement.",
        "coverage":"Token comparisons cover only rows where both native views compile; refused rows remain in the original selected denominator. Public Compiler does not expose over-cap raw token counts; null is explicit lower measurement coverage, not zero tokens or a fit."});
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .context("create fresh private census output")?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    println!("complete native Context census: {}", report["counts"]);
    ensure!(
        inputs_unchanged,
        "census inputs changed; complete report retained"
    );
    Ok(())
}
