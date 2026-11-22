// SPDX-License-Identifier: Apache-2.0
//! Change set 1 (change 5): SoftwareEvidenceV1 as a separate Focus pair policy.
use super::*;
use airk_core::arbiter::{BoundToken, COMPILER_SCHEMA, CompilerProfile, CompilerRecipe};

fn synthetic(max_tokens: usize) -> Result<Compiler> {
    // Synthetic WordLevel tokenizer proves admission/shape, never publisher parity.
    let tokenizer = br#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],"normalizer":null,"pre_tokenizer":null,"post_processor":null,"decoder":null,"model":{"type":"WordLevel","vocab":{"[UNK]":0,"no":2152,"yes":9693,"<|endoftext|>":151643,"<|im_start|>":151644,"<|im_end|>":151645},"unk_token":"[UNK]"}}"#;
    let bind = |(text, id): (&str, u32)| BoundToken {
        text: text.into(),
        id,
    };
    let profile = CompilerProfile {
        schema: COMPILER_SCHEMA.into(),
        recipe: CompilerRecipe::QwenRerankYesNoV1,
        tokenizer: Sha256Digest::from_bytes(Sha256::digest(tokenizer).into()),
        max_tokens,
        max_options: 2,
        controls: [
            ("<|im_start|>", 151644),
            ("<|im_end|>", 151645),
            ("<|endoftext|>", 151643),
        ]
        .map(bind)
        .to_vec(),
        symbols: [("no", 2152), ("yes", 9693)].map(bind).to_vec(),
    };
    Compiler::from_verified(&serde_json::to_vec(&profile)?, tokenizer)
}

#[test]
fn software_policy_is_distinct_stable_and_leaves_the_product_policy_unchanged() -> Result<()> {
    let product = super::super::policy_digest()?;
    let software = software_policy_digest()?;
    assert_ne!(software, product);
    assert_eq!(software, software_policy_digest()?);
    assert_eq!(product, super::super::policy_digest()?);
    Ok(())
}

#[test]
fn software_compile_uses_the_software_instruction_and_binds_its_policy() -> Result<()> {
    for width in [512, 1024] {
        let compiler = synthetic(width)?;
        let option = Candidate {
            id: "original-native-id".into(),
            text: "source text".into(),
        };
        let product = compiler.compile("", "question", std::slice::from_ref(&option))?;
        let software = compile_software(&compiler, "", "question", std::slice::from_ref(&option))?;
        assert!(software.prompt.contains(&format!("<Instruct>: {SOFTWARE}\n")));
        assert!(product.prompt.contains(&format!("<Instruct>: {INSTRUCTION}\n")));
        assert_ne!(software.input_digest, product.input_digest);
        assert_eq!(software.input_ids.len(), width);
        assert_eq!(
            software.input_digest,
            compile_software(&compiler, "", "question", std::slice::from_ref(&option))?
                .input_digest
        );
        // The same admission as product compilation: one pair, empty state.
        assert!(compile_software(&compiler, " ", "question", std::slice::from_ref(&option)).is_err());
        assert!(compile_software(&compiler, "", "question", &[]).is_err());
    }
    Ok(())
}
