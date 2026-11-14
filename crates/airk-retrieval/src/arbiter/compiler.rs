// SPDX-License-Identifier: Apache-2.0
//! Immutable single-choice text recipes; downloaded metadata supplies no code.
use airk_core::{
    arbiter::*,
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use tokenizers::Tokenizer;

const INTERN_SYSTEM: &str = "You are a careful decision assistant. Use the state and decision schema in the user message to make the requested decisions. For every field, choose exactly one answer symbol (e.g. A, B, C, ...) from its listed options and return one valid JSON object mapping each field name to its chosen symbol. Use the field names and symbols exactly as given. Do not include explanations, Markdown, or extra text.";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledInput {
    pub prompt: String,
    pub input_ids: Vec<i64>,
    pub attention_mask: Vec<i64>,
    pub position_ids: Vec<i64>,
    pub decision_position: i64,
    pub candidate_positions: Vec<i64>,
    pub option_mask: Vec<bool>,
    pub input_digest: Sha256Digest,
}

pub struct Compiler {
    profile: CompilerProfile,
    tokenizer: Tokenizer,
}
fn python_space(character: char) -> bool {
    character.is_whitespace() || matches!(character, '\u{1c}'..='\u{1f}')
}

fn escape(text: &str, controls: &[BoundToken]) -> Result<String> {
    ensure!(
        !text.contains("<decision>"),
        "reserved decision marker in Arbiter input"
    );
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<|") {
        output.push_str(&rest[..start]);
        let suffix = &rest[start + 2..];
        if let Some(end) = suffix.find("|>")
            && !suffix[..end].is_empty()
            && suffix[..end]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            output.push_str("<¦");
            output.push_str(&suffix[..end]);
            output.push_str("¦>");
            rest = &suffix[end + 2..];
        } else {
            output.push_str("<|");
            rest = suffix;
        }
    }
    output.push_str(rest);
    for control in controls {
        if control.text == "<decision>" || control.text.starts_with("<|") {
            continue;
        }
        if let Some(inner) = control
            .text
            .strip_prefix('<')
            .and_then(|text| text.strip_suffix('>'))
        {
            output = output.replace(&control.text, &format!("<¦{inner}¦>"));
        }
    }
    Ok(output)
}

impl Compiler {
    pub fn recipe(&self) -> CompilerRecipe {
        self.profile.recipe
    }
    pub fn from_verified(profile_bytes: &[u8], tokenizer_bytes: &[u8]) -> Result<Self> {
        use sha2::{Digest, Sha256};
        ensure!(
            profile_bytes.len() <= 64 << 10 && tokenizer_bytes.len() <= 64 << 20,
            "Arbiter compiler asset limit"
        );
        let profile: CompilerProfile = serde_json::from_slice(profile_bytes)?;
        profile.validate()?;
        ensure!(
            Sha256Digest::from_bytes(Sha256::digest(tokenizer_bytes).into()) == profile.tokenizer,
            "Arbiter tokenizer identity mismatch"
        );
        let mut tokenizer = Tokenizer::from_bytes(tokenizer_bytes)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        tokenizer
            .with_truncation(None)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        tokenizer.with_padding(None);
        for token in profile.controls.iter().chain(&profile.symbols) {
            ensure!(
                tokenizer.token_to_id(&token.text) == Some(token.id),
                "Arbiter compiler token binding mismatch"
            );
        }
        Ok(Self { profile, tokenizer })
    }
    fn encode(&self, text: &str, special: bool) -> Result<tokenizers::Encoding> {
        self.tokenizer
            .encode(text, special)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
    pub fn compile(
        &self,
        state: &str,
        question: &str,
        options: &[Candidate],
    ) -> Result<CompiledInput> {
        ensure!(
            !question.trim().is_empty()
                && question.len() <= 16_384
                && state.len() <= 65_536
                && !options.is_empty()
                && options.len() <= self.profile.max_options,
            "Arbiter compiler input limit"
        );
        let mut ids_seen = BTreeSet::new();
        for option in options {
            ensure!(
                !option.id.is_empty()
                    && option.id.len() <= 128
                    && option.text.len() <= 8192
                    && !option.id.chars().any(char::is_control)
                    && ids_seen.insert(&option.id),
                "invalid Arbiter option"
            );
        }
        let original = serde_json::to_vec(&(state, question, options))?;
        let state = escape(state, &self.profile.controls)?;
        let question = escape(question, &self.profile.controls)?;
        let options = options
            .iter()
            .map(|option| {
                Ok(Candidate {
                    id: escape(&option.id, &self.profile.controls)?,
                    text: escape(&option.text, &self.profile.controls)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let (prompt, ids, decision, candidates) = match self.profile.recipe {
            CompilerRecipe::InternSingleChoiceV1 => {
                let mut lines = vec![format!("result: {question}")];
                for (symbol, option) in self.profile.symbols.iter().zip(&options) {
                    lines.push(format!(
                        "    {} = {}: {}",
                        symbol.text, option.id, option.text
                    ));
                }
                let user = format!(
                    "Return one answer for every field using the supplied answer symbols.\n\n## State\n{}\n## Decision schema\n{}",
                    serde_json::to_string_pretty(&state)?,
                    lines.join("\n")
                );
                let prompt = format!(
                    "<|im_start|>system\n{INTERN_SYSTEM}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n{{\n    \"result\": \"<decision>\"\n}}<|im_end|>\n"
                );
                let ids = self.encode(&prompt, false)?.get_ids().to_vec();
                let marker = self
                    .tokenizer
                    .token_to_id("<decision>")
                    .context("Arbiter decision marker missing")?;
                let positions: Vec<_> = ids
                    .iter()
                    .enumerate()
                    .filter_map(|(position, id)| (*id == marker).then_some(position))
                    .collect();
                ensure!(
                    positions.len() == 1 && positions[0] > 0,
                    "invalid Arbiter decision marker positions"
                );
                let candidates = self
                    .profile
                    .symbols
                    .iter()
                    .take(options.len())
                    .map(|symbol| {
                        let encoded = self.encode(&symbol.text, false)?;
                        ensure!(
                            encoded.get_ids() == [symbol.id],
                            "Arbiter decision symbol is not one token"
                        );
                        Ok(i64::from(symbol.id))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let decision = positions[0] - 1;
                (prompt, ids, decision, candidates)
            }
            CompilerRecipe::StrandsSingleChoiceV1 => {
                let prefix = format!(
                    "<question type=\"choice\">\nSelect exactly one option.\n{}\n<options>\n",
                    question.trim_matches(python_space)
                );
                let mut question_text = prefix;
                let mut spans = Vec::new();
                for (position, option) in options.iter().enumerate() {
                    let description = option
                        .text
                        .split(python_space)
                        .filter(|word| !word.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let line = format!(
                        "{}. {}{}",
                        position + 1,
                        option.id,
                        if description.is_empty() {
                            String::new()
                        } else {
                            format!(" — {description}")
                        }
                    );
                    let start = question_text.len();
                    question_text.push_str(&line);
                    spans.push((start, question_text.len()));
                    question_text.push('\n');
                }
                question_text.push_str("</options>\n</question>\n<answer>");
                let state_text =
                    format!("<state>\n{}\n</state>\n", state.trim_matches(python_space));
                let mut ids = self.encode(&state_text, true)?.get_ids().to_vec();
                let state_tokens = ids.len();
                let encoded = self.encode(&question_text, false)?;
                let mut candidates = Vec::new();
                for (start, end) in spans {
                    let position = encoded
                        .get_offsets()
                        .iter()
                        .enumerate()
                        .filter_map(|(position, (a, b))| {
                            (b > a && *a >= start && *b <= end).then_some(position)
                        })
                        .next_back()
                        .context("Arbiter option span has no token")?;
                    candidates.push(i64::try_from(state_tokens + position)?);
                }
                ids.extend_from_slice(encoded.get_ids());
                let decision = ids.len() - 1;
                (state_text + &question_text, ids, decision, candidates)
            }
            CompilerRecipe::KevSingleChoiceV1 => {
                let control = |name: &str| -> Result<u32> {
                    self.tokenizer
                        .token_to_id(name)
                        .context("Kev compiler delimiter missing")
                };
                let mut ids = vec![control("<|fim_prefix|>")?];
                ids.extend_from_slice(self.encode(&state, false)?.get_ids());
                ids.push(control("<|fim_middle|>")?);
                ids.extend_from_slice(self.encode(&question, false)?.get_ids());
                let mut candidates = Vec::new();
                for option in &options {
                    ids.push(control("<|box_start|>")?);
                    ids.extend_from_slice(
                        self.encode(&format!("{}: {}", option.id, option.text), false)?
                            .get_ids(),
                    );
                    ids.push(control("<|box_end|>")?);
                    candidates.push(i64::try_from(ids.len() - 1)?);
                }
                ids.push(control("<|fim_suffix|>")?);
                let prompt = self
                    .tokenizer
                    .decode(&ids, false)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                let decision = ids.len() - 1;
                (prompt, ids, decision, candidates)
            }
        };
        ensure!(
            !ids.is_empty() && ids.len() <= self.profile.max_tokens,
            "Arbiter compiler context exceeded; truncation is forbidden"
        );
        let count = ids.len();
        Ok(CompiledInput {
            prompt,
            input_ids: ids.into_iter().map(i64::from).collect(),
            attention_mask: vec![1; count],
            position_ids: (0..count).map(|position| position as i64).collect(),
            decision_position: i64::try_from(decision)?,
            candidate_positions: candidates,
            option_mask: vec![true; options.len()],
            input_digest: framed_digest(b"airk.arbiter.compiled-input/1", &[&original]),
        })
    }
}
