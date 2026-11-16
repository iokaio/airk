// SPDX-License-Identifier: Apache-2.0
use super::*;
#[test]
fn original_human_character_offsets_require_exact_utf8_answer() -> Result<()> {
    assert_eq!(
        char_span("a東京\n", 1, 3, "東京")?,
        LocalSpan { start: 1, end: 7 }
    );
    assert!(char_span("a東京\n", 1, 3, "京").is_err());
    assert!(char_span("a東京\n", 1, 99, "東京").is_err());
    Ok(())
}
#[test]
fn native_lf_lines_allow_only_explicit_crlf_outer_newline_equivalence() -> Result<()> {
    let annotation = json!({"start_line":2,"end_line":2,"content":"東京"});
    assert_eq!(
        context_span("a\r\n東京\r\nx", &annotation)?,
        LocalSpan { start: 3, end: 11 }
    );
    assert!(
        context_span(
            "one\u{2028}two\nthree",
            &json!({"start_line":2,"end_line":2,"content":"two"})
        )
        .is_err()
    );
    assert!(
        context_span(
            "one\rtwo\nthree",
            &json!({"start_line":2,"end_line":2,"content":"two"})
        )
        .is_err()
    );
    assert_eq!(equivalent_lines("\rgold\r"), "\rgold\r");
    assert!(
        context_span(
            "x\ngold\rREST\n",
            &json!({"start_line":99,"end_line":99,"content":"gold"})
        )
        .is_err()
    );
    assert!(
        context_span(
            "x\ngold\r\nREST\n",
            &json!({"start_line":99,"end_line":99,"content":"gold\r"})
        )
        .is_err()
    );
    assert_eq!(
        context_span(
            "x\ngold\r\nREST\n",
            &json!({"start_line":99,"end_line":99,"content":"gold"})
        )?,
        LocalSpan { start: 2, end: 6 }
    );
    Ok(())
}
#[test]
fn entire_raw_gold_may_relocate_but_numbered_formatter_may_not() -> Result<()> {
    let source = "a\nb\nc\n";
    assert_eq!(
        context_span(
            source,
            &json!({"start_line":2,"end_line":3,"content":"2\tb\n3\tc"})
        )?,
        LocalSpan { start: 2, end: 6 }
    );
    assert_eq!(
        context_span(
            source,
            &json!({"start_line":99,"end_line":100,"content":"b\nc\n"})
        )?,
        LocalSpan { start: 2, end: 6 }
    );
    assert!(
        context_span(
            source,
            &json!({"start_line":2,"end_line":3,"content":"3\tb\n4\tc"})
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn repeated_overlapping_or_blank_gold_cannot_become_source_evidence() {
    for (source, content) in [("aaa\n", "aa"), ("one\none\n", "one\n"), ("a\n", "")] {
        assert!(
            context_span(
                source,
                &json!({"start_line":99,"end_line":99,"content":content})
            )
            .is_err()
        );
    }
}
#[test]
fn full_gold_bag_cannot_drop_unknown_missing_or_fragmented_members() {
    assert_eq!(complete_bag(&[Some(true), Some(true)]), Some(true));
    assert_eq!(complete_bag(&[Some(true), None]), None);
    assert_eq!(complete_bag(&[Some(true), Some(false)]), Some(false));
    assert_eq!(complete_bag(&[]), None);
}
#[test]
fn checkout_paths_and_same_revision_file_membership_cannot_change() -> Result<()> {
    assert_eq!(
        relative_annotation("/workspace/owner__repo__0.1/src/a.rs", "owner/repo")?,
        "src/a.rs"
    );
    assert!(relative_annotation("/workspace/other__repo__0.1/src/a.rs", "owner/repo").is_err());
    assert!(relative_annotation("../a.rs", "owner/repo").is_err());
    let mut group = Group {
        suffix: PathBuf::new(),
        files: BTreeMap::new(),
    };
    add_file(
        &mut group,
        "a.rs",
        ResourceFilePin {
            path: "original".into(),
            sha256: Sha256Digest::from_bytes([1; 32]),
        },
    )?;
    assert!(
        add_file(
            &mut group,
            "a.rs",
            ResourceFilePin {
                path: "substitute".into(),
                sha256: Sha256Digest::from_bytes([2; 32])
            }
        )
        .is_err()
    );
    let rights = ResourceFilePin {
        path: std::env::temp_dir()
            .join("rights-input.json")
            .to_string_lossy()
            .into(),
        sha256: Sha256Digest::from_bytes([1; 32]),
    };
    assert!(rights_map(&[rights.clone(), rights]).is_err());
    assert!(context_rights(&json!({"rows":[{"rights_pins":[]}]})).is_err());
    Ok(())
}

#[test]
fn actual_verified_focus_compiler_rejects_wider_valid_native_profile() -> Result<()> {
    let tokenizer = serde_json::to_vec(
        &json!({"version":"1.0","truncation":null,"padding":null,"added_tokens":[
        {"id":151644,"content":"<|im_start|>","single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true},
        {"id":151645,"content":"<|im_end|>","single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true},
        {"id":151643,"content":"<|endoftext|>","single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true}],
        "normalizer":null,"pre_tokenizer":{"type":"Whitespace"},"post_processor":null,"decoder":null,
        "model":{"type":"WordLevel","vocab":{"[UNK]":0,"word":1,"no":2152,"yes":9693,"<|endoftext|>":151643,"<|im_start|>":151644,"<|im_end|>":151645},"unk_token":"[UNK]"}}),
    )?;
    let bytes = airk_retrieval::arbiter::compiler::focus::compiler_profile(&tokenizer)?;
    assert_eq!(
        verified_focus_compiler(&bytes, &tokenizer)?.recipe(),
        airk_core::arbiter::CompilerRecipe::QwenRerankYesNoV1
    );
    let mut wide: airk_core::arbiter::CompilerProfile = serde_json::from_slice(&bytes)?;
    wide.max_tokens = 8192;
    let bytes = serde_json::to_vec(&wide)?;
    assert!(Compiler::from_verified(&bytes, &tokenizer).is_ok());
    assert!(verified_focus_compiler(&bytes, &tokenizer).is_err());
    Ok(())
}
