// SPDX-License-Identifier: Apache-2.0
//! Private test-support prompt experiments. No product recipe/admission is added.
use super::{Candidate, CompiledInput, Compiler, INSTRUCTION};
use airk_core::{
    arbiter::{Readout, Stage},
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, time::Instant};

pub const SCHEMA: &str = "airk.private.focus-instruction-census/1";
const SOFTWARE: &str = "Given a software question, retrieve relevant code, documentation, or diagnostic evidence that helps answer the question.";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Instruction {
    Publisher,
    SoftwareEvidenceV1,
}
impl Instruction {
    pub fn text(self) -> &'static str {
        match self {
            Self::Publisher => INSTRUCTION,
            Self::SoftwareEvidenceV1 => SOFTWARE,
        }
    }
    pub fn digest(self) -> Result<Sha256Digest> {
        let source = std::str::from_utf8(include_bytes!("experiment.rs"))?.replace("\r\n", "\n");
        ensure!(!source.contains('\r'), "lone CR in experiment source");
        Ok(framed_digest(
            b"airk.private.focus-instruction-recipe/1",
            &[
                &serde_json::to_vec(&self)?,
                self.text().as_bytes(),
                source.as_bytes(),
                super::policy_digest()?.as_bytes(),
            ],
        ))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    pub id: String,
    pub question: String,
    pub document: String,
    pub question_sha256: Sha256Digest,
    pub document_sha256: Sha256Digest,
    pub label: u8,
    pub role: String,
    pub rights_scope: String,
}
impl Pair {
    fn validate(&self) -> Result<()> {
        ensure!(self.label <= 1, "unknown relevance is not a negative");
        ensure!(
            Sha256Digest::from_bytes(Sha256::digest(self.question.as_bytes()).into())
                == self.question_sha256
                && Sha256Digest::from_bytes(Sha256::digest(self.document.as_bytes()).into())
                    == self.document_sha256,
            "original human question/window hash differs"
        );
        ensure!(
            [
                "TRAIN",
                "DEV",
                "CAL_RESERVED",
                "CONFIRMATION_RESERVED",
                "RESEARCH_TEST_RESERVED",
                "QUARANTINE_SOURCE_PARSE_OR_EXPOSURE",
                "NO_ADMITTED_COMPONENT",
                "REPLAY"
            ]
            .contains(&self.role.as_str()),
            "unknown frozen experiment role"
        );
        ensure!(
            !self.rights_scope.is_empty() && self.rights_scope.len() <= 128,
            "rights scope absent"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Compiled { input: CompiledInput },
    Refused { reason: String },
}
impl Outcome {
    pub fn input(&self) -> Option<&CompiledInput> {
        match self {
            Self::Compiled { input } => Some(input),
            Self::Refused { .. } => None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub pair: Pair,
    pub publisher: Outcome,
    pub software_evidence_v1: Outcome,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Census {
    pub schema: String,
    pub publisher_recipe: Sha256Digest,
    pub software_recipe: Sha256Digest,
    pub compiler_sha256: Sha256Digest,
    pub tokenizer_sha256: Sha256Digest,
    pub original_denominator: usize,
    pub rows: Vec<Observation>,
    pub qualification: bool,
    pub model_requests_dispatched: bool,
}

pub fn compile_pair(
    compiler: &Compiler,
    instruction: Instruction,
    pair: &Pair,
) -> Result<CompiledInput> {
    pair.validate()?;
    ensure!(
        compiler.limits() == (512, 2),
        "experiment requires exact native512 profile"
    );
    compiler.compile_focus_instruction(
        "",
        &pair.question,
        &[Candidate {
            id: pair.id.clone(),
            text: pair.document.clone(),
        }],
        instruction.text(),
    )
}

pub fn census(compiler: &Compiler, pairs: Vec<Pair>, deadline: Instant) -> Result<Census> {
    ensure!(
        !pairs.is_empty() && pairs.len() <= 4000,
        "full bounded original population required"
    );
    let mut ids = BTreeSet::new();
    for pair in &pairs {
        ensure!(
            ids.insert(pair.id.clone()),
            "duplicate original experiment identity"
        );
        pair.validate()?;
    }
    let observe = |pair: &Pair, recipe| -> Result<Outcome> {
        ensure!(
            Instant::now() < deadline,
            "original census deadline expired"
        );
        let result = match compile_pair(compiler, recipe, pair) {
            Ok(input) => Outcome::Compiled { input },
            Err(error) => Outcome::Refused {
                reason: format!("{error:#}"),
            },
        };
        ensure!(
            Instant::now() < deadline,
            "original census deadline expired after compilation"
        );
        Ok(result)
    };
    let mut rows = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let publisher = observe(&pair, Instruction::Publisher)?;
        let software_evidence_v1 = observe(&pair, Instruction::SoftwareEvidenceV1)?;
        rows.push(Observation {
            pair,
            publisher,
            software_evidence_v1,
        });
    }
    let result = Census {
        schema: SCHEMA.into(),
        publisher_recipe: Instruction::Publisher.digest()?,
        software_recipe: Instruction::SoftwareEvidenceV1.digest()?,
        compiler_sha256: Sha256Digest::from_bytes(Sha256::digest(&compiler.profile_bytes).into()),
        tokenizer_sha256: compiler.profile.tokenizer,
        original_denominator: rows.len(),
        rows,
        qualification: false,
        model_requests_dispatched: false,
    };
    ensure!(
        Instant::now() < deadline,
        "original census deadline expired retaining recipe identity"
    );
    Ok(result)
}

#[derive(Clone, Debug, Serialize)]
pub struct RawScores {
    pub input_digest: Sha256Digest,
    pub instruction: Instruction,
    pub recipe: Sha256Digest,
    pub no_logit: f32,
    pub yes_logit: f32,
    pub no_bits: u32,
    pub yes_bits: u32,
    pub elapsed_us: u64,
}
fn raw_scores(
    input: &CompiledInput,
    instruction: Instruction,
    scores: &[f32],
    elapsed_us: u64,
) -> Result<RawScores> {
    ensure!(
        input.input_ids.len() == 512
            && input.candidate_positions == [2152, 9693]
            && input.option_mask == [true, true]
            && input.decision_position == 511,
        "raw experiment readout differs from native512 no/yes ABI"
    );
    ensure!(
        scores.len() == 2 && scores.iter().all(|x| x.is_finite()),
        "raw finite no/yes logits required"
    );
    Ok(RawScores {
        input_digest: input.input_digest,
        instruction,
        recipe: instruction.digest()?,
        no_logit: scores[0],
        yes_logit: scores[1],
        no_bits: scores[0].to_bits(),
        yes_bits: scores[1].to_bits(),
        elapsed_us,
    })
}
/// Caller retains actual native ServiceComputeScope and the admitted Scorer.
/// No error is retried and no serialized record constructs runtime authority.
pub fn score_raw(
    scorer: &crate::arbiter::runtime::Scorer,
    compiler: &Compiler,
    pair: &Pair,
    instruction: Instruction,
    deadline: Instant,
) -> Result<RawScores> {
    ensure!(
        scorer.profile().stage == Some(Stage::Focus)
            && scorer.profile().abi.readout == Readout::FocusYesNo,
        "actual Focus scorer required"
    );
    ensure!(Instant::now() < deadline, "original score deadline expired");
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&compiler.profile_bytes).into())
            == scorer.profile().compiler,
        "experiment compiler differs from actual scorer compiler asset"
    );
    let input = compile_pair(compiler, instruction, pair)?;
    ensure!(
        Instant::now() < deadline,
        "original score deadline expired during compilation"
    );
    let started = Instant::now();
    let scores = scorer.score_with_deadline(&input, deadline)?;
    ensure!(
        Instant::now() < deadline,
        "original score deadline expired after submission"
    );
    let result = raw_scores(
        &input,
        instruction,
        &scores,
        u64::try_from(started.elapsed().as_micros())?,
    )?;
    ensure!(
        Instant::now() < deadline,
        "original score deadline expired during observation"
    );
    Ok(result)
}

#[cfg(test)]
mod tests;
