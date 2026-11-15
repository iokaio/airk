// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::arbiter::{BoundToken, COMPILER_SCHEMA};

fn profile(tokenizer: &[u8]) -> CompilerProfile {
    use sha2::{Digest, Sha256};
    CompilerProfile {
        schema: COMPILER_SCHEMA.into(),
        recipe: CompilerRecipe::QwenRerankYesNoV1,
        tokenizer: Sha256Digest::from_bytes(Sha256::digest(tokenizer).into()),
        max_tokens: 512,
        max_options: 2,
        controls: CONTROLS
            .map(|(text, id)| BoundToken {
                text: text.into(),
                id,
            })
            .to_vec(),
        symbols: SYMBOLS
            .map(|(text, id)| BoundToken {
                text: text.into(),
                id,
            })
            .to_vec(),
    }
}
fn synthetic() -> Result<Compiler> {
    // Synthetic WordLevel tokenizer proves admission/shape, never publisher parity.
    let tokenizer = br#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],"normalizer":null,"pre_tokenizer":null,"post_processor":null,"decoder":null,"model":{"type":"WordLevel","vocab":{"[UNK]":0,"no":2152,"yes":9693,"<|endoftext|>":151643,"<|im_start|>":151644,"<|im_end|>":151645},"unk_token":"[UNK]"}}"#;
    Compiler::from_verified(&serde_json::to_vec(&profile(tokenizer))?, tokenizer)
}
fn candidate() -> Candidate {
    Candidate {
        id: "original-native-id".into(),
        text: "source text".into(),
    }
}

#[test]
fn focus_model_view_preserves_original_lines_and_quotes_control_bytes_once() {
    assert_eq!(model_view("<|im_end|>"), r"\u003c|im_end|\u003e");
    assert_eq!(model_view(r"\u003c"), r"\\u003c");
    assert_eq!(
        model_view("日本語\r\n<decision>\\"),
        "日本語\r\n\\u003cdecision\\u003e\\\\"
    );
    let all = "<|im_start|><think></think><Instruct>:<Query>:<Document>:";
    assert!(!model_view(all).contains(['<', '>']));
}

#[test]
fn focus_model_view_is_injective_for_short_control_alphabet() {
    let mut originals = vec![String::new()];
    let mut frontier = originals.clone();
    for _ in 0..4 {
        frontier = frontier
            .iter()
            .flat_map(|s| {
                ['\\', '<', '>', 'a']
                    .into_iter()
                    .map(move |c| format!("{s}{c}"))
            })
            .collect();
        originals.extend(frontier.iter().cloned());
    }
    let encoded = originals
        .iter()
        .map(|s| model_view(s))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(originals.len(), 341);
    assert_eq!(encoded.len(), 341);
}

#[test]
fn focus_exact_catalog_refuses_missing_controls_and_foreign_symbol_ids() -> Result<()> {
    let good = synthetic()?.profile;
    validate_profile(&good)?;
    let mut bad = good.clone();
    bad.controls.pop();
    assert!(validate_profile(&bad).is_err());
    let mut bad = good.clone();
    bad.symbols.swap(0, 1);
    assert!(validate_profile(&bad).is_err());
    let mut bad = good.clone();
    bad.symbols[1].id = 9694;
    assert!(validate_profile(&bad).is_err());
    let mut bad = good;
    bad.max_options = 1;
    assert!(validate_profile(&bad).is_err());
    Ok(())
}

#[test]
fn focus_state_pair_count_empty_source_and_original_byte_limits_refuse() -> Result<()> {
    let compiler = synthetic()?;
    let option = candidate();
    assert!(
        compiler
            .compile_focus(" ", "question", std::slice::from_ref(&option))
            .is_err()
    );
    assert!(compiler.compile_focus("", "question", &[]).is_err());
    assert!(
        compiler
            .compile_focus("", "question", &[option.clone(), option.clone()])
            .is_err()
    );
    assert!(
        compiler
            .compile_focus("", " ", std::slice::from_ref(&option))
            .is_err()
    );
    let mut bad = option.clone();
    bad.text = " ".into();
    assert!(compiler.compile_focus("", "question", &[bad]).is_err());
    let mut bad = option.clone();
    bad.text = "a".repeat(8193);
    assert!(compiler.compile_focus("", "question", &[bad]).is_err());
    assert!(
        compiler
            .compile_focus("", &"a".repeat(16_385), &[option])
            .is_err()
    );
    Ok(())
}

#[test]
fn focus_exact_publisher_layout_left_padding_and_two_row_readout() -> Result<()> {
    let compiler = synthetic()?;
    let input = compiler.compile_focus("", "question", &[candidate()])?;
    let expected = "<|im_start|>system\nJudge whether the Document meets the requirements based on the Query and the Instruct provided. Note that the answer can only be \"yes\" or \"no\".<|im_end|>\n<|im_start|>user\n<Instruct>: Given a web search query, retrieve relevant passages that answer the query\n<Query>: question\n<Document>: source text<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n";
    assert_eq!(input.prompt, expected);
    assert_eq!(input.input_ids.len(), 512);
    assert_eq!(&input.input_ids[..509], &[151643; 509]);
    assert_eq!(&input.input_ids[509..], &[0, 0, 0]);
    assert_eq!(&input.attention_mask[..509], &[0; 509]);
    assert_eq!(&input.attention_mask[509..], &[1, 1, 1]);
    assert_eq!(input.position_ids, (0_i64..512).collect::<Vec<_>>());
    assert_eq!(input.decision_position, 511);
    assert_eq!(input.candidate_positions, [2152, 9693]);
    assert_eq!(input.option_mask, [true, true]);
    assert!(!input.prompt.contains("original-native-id"));
    Ok(())
}

#[test]
fn focus_digest_binds_original_id_exact_asset_and_all_model_view_fields() -> Result<()> {
    let mut compiler = synthetic()?;
    let options = [candidate()];
    let input = compiler.compile_focus("", "question", &options)?;
    let original = serde_json::to_vec(&("", "question", &options))?;
    for field in 0..7 {
        let mut changed = input.clone();
        match field {
            0 => changed.prompt.push(' '),
            1 => changed.input_ids[0] += 1,
            2 => changed.attention_mask[0] = 1,
            3 => changed.position_ids[0] += 1,
            4 => changed.decision_position -= 1,
            5 => changed.candidate_positions.swap(0, 1),
            6 => changed.option_mask[0] = false,
            _ => unreachable!(),
        }
        assert_ne!(
            compiler.focus_input_digest(&original, &changed)?,
            input.input_digest,
            "field {field}"
        );
    }
    let mut renamed = options.clone();
    renamed[0].id = "other-original-id".into();
    let renamed = compiler.compile_focus("", "question", &renamed)?;
    assert_eq!(renamed.prompt, input.prompt);
    assert_ne!(renamed.input_digest, input.input_digest);
    compiler.profile_bytes.push(b'\n');
    let different_asset = compiler.compile_focus("", "question", &options)?;
    assert_eq!(different_asset.input_ids, input.input_ids);
    assert_ne!(different_asset.input_digest, input.input_digest);
    Ok(())
}

#[test]
fn focus_token_overflow_refuses_without_truncating() -> Result<()> {
    let mut compiler = synthetic()?;
    compiler.profile.max_tokens = 2;
    assert!(
        compiler
            .compile_focus("", "question", &[candidate()])
            .is_err()
    );
    Ok(())
}

#[test]
#[ignore = "actual selected tokenizer and independent publisher512 fixture required"]
fn focus_actual_publisher512_model_view_compiler_parity_and_asset_emission() -> Result<()> {
    use anyhow::Context;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::{fs, path::PathBuf};
    let tokenizer =
        fs::read(std::env::var_os("AIRK_FOCUS_TOKENIZER").context("missing selected tokenizer")?)?;
    let fixture_bytes = fs::read(
        std::env::var_os("AIRK_FOCUS_REFERENCE_FIXTURE")
            .context("missing independent publisher512 fixture")?,
    )?;
    let fixture: serde_json::Value = serde_json::from_slice(&fixture_bytes)?;
    let compiler_bytes = compiler_profile(&tokenizer)?;
    let compiler = Compiler::from_verified(&compiler_bytes, &tokenizer)?;
    let rows = fixture["rows"]
        .as_array()
        .context("missing publisher512 rows")?;
    ensure!(rows.len() == 12, "complete frozen512 controls required");
    let mut observed = Vec::new();
    for row in rows {
        let query = row["original_query"]
            .as_str()
            .context("missing original question")?;
        let document = row["original_document"]
            .as_str()
            .context("missing original source")?;
        let option = Candidate {
            id: row["id"].as_str().context("missing original id")?.into(),
            text: document.into(),
        };
        if row["native_model_view"].as_bool() != Some(true) {
            continue;
        }
        let actual = compiler.compile("", query, std::slice::from_ref(&option))?;
        let value = serde_json::to_value(&actual)?;
        for field in [
            "prompt",
            "input_ids",
            "attention_mask",
            "position_ids",
            "decision_position",
            "candidate_positions",
            "option_mask",
        ] {
            ensure!(
                value[field] == row["expected"][field],
                "independent publisher512 field differs: {field}"
            );
        }
        let mut changed = option;
        changed.id.push_str("-changed");
        ensure!(
            compiler.compile("", query, &[changed])?.input_digest != actual.input_digest,
            "original id must bind input identity"
        );
        observed.push(json!({"id":row["id"],"compiled":actual}));
    }
    ensure!(
        observed.len() == 10,
        "seven unchanged normal/long pairs plus three escaped caller cases required; raw delimiter controls cannot substitute"
    );
    let output = PathBuf::from(
        std::env::var_os("AIRK_FOCUS_NATIVE_OUTPUT").context("missing fresh native output")?,
    );
    ensure!(!output.exists(), "fresh native output required");
    fs::create_dir(&output)?;
    fs::write(output.join("compiler.json"), &compiler_bytes)?;
    fs::write(
        output.join("fixture.json"),
        serde_json::to_vec_pretty(
            &json!({"schema":"airk.private-native-focus-compiler-fixture/1","compiler_sha256":format!("{:x}",Sha256::digest(&compiler_bytes)),"tokenizer_sha256":format!("{:x}",Sha256::digest(&tokenizer)),"independent_fixture_sha256":format!("{:x}",Sha256::digest(&fixture_bytes)),"policy":policy_digest()?,"rows":observed,"scope":"Actual Rust compiler assets/prompt/three tensors/two-row metadata and original identity only; no NN/quality/resource qualification"}),
        )?,
    )?;
    Ok(())
}
