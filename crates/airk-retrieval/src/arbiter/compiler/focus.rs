// SPDX-License-Identifier: Apache-2.0
//! Publisher pairwise relevance recipe; source text never supplies template controls.
use super::{Candidate, CompiledInput, Compiler};
use airk_core::{
    arbiter::{CompilerProfile, CompilerRecipe},
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Result, ensure};

const INSTRUCTION: &str =
    "Given a web search query, retrieve relevant passages that answer the query";
const PREFIX: &str = "<|im_start|>system\nJudge whether the Document meets the requirements based on the Query and the Instruct provided. Note that the answer can only be \"yes\" or \"no\".<|im_end|>\n<|im_start|>user\n";
const SUFFIX: &str = "<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n";
const CONTROLS: [(&str, u32); 3] = [
    ("<|im_start|>", 151644),
    ("<|im_end|>", 151645),
    ("<|endoftext|>", 151643),
];
const SYMBOLS: [(&str, u32); 2] = [("no", 2152), ("yes", 9693)];

pub(super) fn validate_profile(profile: &CompilerProfile) -> Result<()> {
    ensure!(
        profile.recipe == CompilerRecipe::QwenRerankYesNoV1
            && profile.max_options == 2
            && (1..=8192).contains(&profile.max_tokens)
            && profile
                .controls
                .iter()
                .map(|t| (t.text.as_str(), t.id))
                .eq(CONTROLS)
            && profile
                .symbols
                .iter()
                .map(|t| (t.text.as_str(), t.id))
                .eq(SYMBOLS),
        "Focus requires the exact publisher control and no/yes token catalog"
    );
    Ok(())
}

/// Injective model-view quoting; original question/window bytes are separately bound.
fn model_view(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '<' => output.push_str("\\u003c"),
            '>' => output.push_str("\\u003e"),
            _ => output.push(character),
        }
    }
    output
}

/// Native source policy is portable across checkout line endings, not source-text edits.
pub fn policy_digest() -> Result<Sha256Digest> {
    let parent = std::str::from_utf8(include_bytes!("../compiler.rs"))?.replace("\r\n", "\n");
    let source = std::str::from_utf8(include_bytes!("focus.rs"))?.replace("\r\n", "\n");
    ensure!(
        !parent.contains('\r') && !source.contains('\r'),
        "lone CR in Focus source policy"
    );
    Ok(framed_digest(
        b"airk.arbiter.focus-pair-policy/1",
        &[
            parent.as_bytes(),
            source.as_bytes(),
            airk_core::arbiter::focus::semantic_policy_digest()?.as_bytes(),
        ],
    ))
}

impl Compiler {
    fn focus_input_digest(&self, original: &[u8], input: &CompiledInput) -> Result<Sha256Digest> {
        #[derive(serde::Serialize)]
        struct View<'a> {
            prompt: &'a str,
            input_ids: [&'a [i64]; 1],
            attention_mask: [&'a [i64]; 1],
            position_ids: [&'a [i64]; 1],
            decision_positions: [i64; 1],
            candidate_positions: [&'a [i64]; 1],
            option_mask: [&'a [bool]; 1],
        }
        let view = serde_json::to_vec(&View {
            prompt: &input.prompt,
            input_ids: [&input.input_ids],
            attention_mask: [&input.attention_mask],
            position_ids: [&input.position_ids],
            decision_positions: [input.decision_position],
            candidate_positions: [&input.candidate_positions],
            option_mask: [&input.option_mask],
        })?;
        Ok(framed_digest(
            b"airk.arbiter.focus-compiled-input/1",
            &[
                original,
                &self.profile_bytes,
                policy_digest()?.as_bytes(),
                &view,
            ],
        ))
    }

    pub(super) fn compile_focus(
        &self,
        state: &str,
        question: &str,
        options: &[Candidate],
    ) -> Result<CompiledInput> {
        self.compile_focus_instruction(state, question, options, INSTRUCTION)
    }

    fn compile_focus_instruction(
        &self,
        state: &str,
        question: &str,
        options: &[Candidate],
        instruction: &str,
    ) -> Result<CompiledInput> {
        validate_profile(&self.profile)?;
        ensure!(
            state.is_empty() && options.len() == 1,
            "Focus accepts empty state and one original pair only"
        );
        let option = &options[0];
        ensure!(
            !question.trim().is_empty()
                && question.len() <= 16_384
                && !option.id.is_empty()
                && option.id.len() <= 128
                && !option.id.chars().any(char::is_control)
                && !option.text.trim().is_empty()
                && option.text.len() <= 8192,
            "invalid original Focus question/candidate window"
        );
        let original = serde_json::to_vec(&(state, question, options))?;
        let body = format!(
            "<Instruct>: {instruction}\n<Query>: {}\n<Document>: {}",
            model_view(question),
            model_view(&option.text)
        );
        // The official manual recipe independently tokenizes all three components.
        let mut ids = Vec::new();
        for component in [PREFIX, &body, SUFFIX] {
            ids.extend_from_slice(self.encode(component, false)?.get_ids());
        }
        ensure!(
            !ids.is_empty() && ids.len() <= self.profile.max_tokens,
            "Focus context exceeded; truncation is forbidden"
        );
        for (text, id) in SYMBOLS {
            ensure!(
                self.encode(text, false)?.get_ids() == [id],
                "Focus no/yes readout is not the exact single token"
            );
        }
        let pad = self.profile.max_tokens - ids.len();
        let mut input_ids = vec![151643_i64; pad];
        input_ids.extend(ids.into_iter().map(i64::from));
        let mut attention_mask = vec![0; pad];
        attention_mask.resize(self.profile.max_tokens, 1);
        let mut input = CompiledInput {
            prompt: format!("{PREFIX}{body}{SUFFIX}"),
            input_ids,
            attention_mask,
            position_ids: (0..self.profile.max_tokens)
                .map(|position| position as i64)
                .collect(),
            decision_position: i64::try_from(self.profile.max_tokens - 1)?,
            candidate_positions: vec![2152, 9693],
            option_mask: vec![true, true],
            input_digest: Sha256Digest::from_bytes([0; 32]),
        };
        input.input_digest = self.focus_input_digest(&original, &input)?;
        Ok(input)
    }
}

/// Emit the actual Rust-owned static512 compiler asset for qualification tools.
#[cfg(any(test, feature = "test-support"))]
pub fn compiler_profile(tokenizer_bytes: &[u8]) -> Result<Vec<u8>> {
    use airk_core::arbiter::{BoundToken, COMPILER_SCHEMA};
    use sha2::{Digest, Sha256};
    let tokenizer = tokenizers::Tokenizer::from_bytes(tokenizer_bytes)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let bind = |(text, id): (&str, u32)| -> Result<BoundToken> {
        ensure!(
            tokenizer.token_to_id(text) == Some(id),
            "selected Focus tokenizer binding differs"
        );
        Ok(BoundToken {
            text: text.into(),
            id,
        })
    };
    let profile = CompilerProfile {
        schema: COMPILER_SCHEMA.into(),
        recipe: CompilerRecipe::QwenRerankYesNoV1,
        tokenizer: Sha256Digest::from_bytes(Sha256::digest(tokenizer_bytes).into()),
        max_tokens: 512,
        max_options: 2,
        controls: CONTROLS.into_iter().map(bind).collect::<Result<_>>()?,
        symbols: SYMBOLS.into_iter().map(bind).collect::<Result<_>>()?,
    };
    profile.validate()?;
    validate_profile(&profile)?;
    let mut bytes = serde_json::to_vec_pretty(&profile)?;
    bytes.push(b'\n');
    Compiler::from_verified(&bytes, tokenizer_bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "test-support"))]
pub mod experiment;
