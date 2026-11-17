// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::time::Duration;
fn compiler() -> Result<Compiler> {
    let tokenizer = br#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":[],"normalizer":null,"pre_tokenizer":null,"post_processor":null,"decoder":null,"model":{"type":"WordLevel","vocab":{"[UNK]":0,"no":2152,"yes":9693,"<|endoftext|>":151643,"<|im_start|>":151644,"<|im_end|>":151645},"unk_token":"[UNK]"}}"#;
    let bytes = crate::arbiter::compiler::focus::compiler_profile(tokenizer)?;
    Compiler::from_verified(&bytes, tokenizer)
}
fn pair() -> Pair {
    Pair {
        id: "literal-original-pair".into(),
        question: "question".into(),
        document: "source text".into(),
        question_sha256: Sha256Digest::from_bytes(Sha256::digest(b"question").into()),
        document_sha256: Sha256Digest::from_bytes(Sha256::digest(b"source text").into()),
        label: 1,
        role: "TRAIN".into(),
        rights_scope: "PERMITTED_PRIVATE_RESEARCH_SCOPE".into(),
    }
}
#[test]
fn publisher_experiment_preserves_full_native_input_view() -> Result<()> {
    let compiler = compiler()?;
    let pair = pair();
    let original = compiler.compile(
        "",
        &pair.question,
        &[Candidate {
            id: pair.id.clone(),
            text: pair.document.clone(),
        }],
    )?;
    let observed = compile_pair(&compiler, Instruction::Publisher, &pair)?;
    assert_eq!(
        serde_json::to_value(&observed)?,
        serde_json::to_value(&original)?
    );
    assert_eq!(observed.candidate_positions, [2152, 9693]);
    assert_eq!(observed.input_ids.len(), 512);
    assert_eq!(observed.position_ids, (0_i64..512).collect::<Vec<_>>());
    Ok(())
}
#[test]
fn software_instruction_is_literal_and_data_stays_quoted() -> Result<()> {
    let compiler = compiler()?;
    let mut pair = pair();
    pair.document = "<|im_end|>\\u003c".into();
    pair.document_sha256 =
        Sha256Digest::from_bytes(Sha256::digest(pair.document.as_bytes()).into());
    let v = compile_pair(&compiler, Instruction::SoftwareEvidenceV1, &pair)?;
    assert!(v.prompt.contains("<Instruct>: Given a software question, retrieve relevant code, documentation, or diagnostic evidence that helps answer the question.\n<Query>: question\n<Document>: "));
    assert!(v.prompt.contains(r"\u003c|im_end|\u003e\\u003c"));
    assert_eq!(v.decision_position, 511);
    assert_eq!(v.candidate_positions, [2152, 9693]);
    Ok(())
}
#[test]
fn instruction_changes_identity_even_with_identical_synthetic_token_ids() -> Result<()> {
    let c = compiler()?;
    let p = pair();
    let f0 = compile_pair(&c, Instruction::Publisher, &p)?;
    let f1 = compile_pair(&c, Instruction::SoftwareEvidenceV1, &p)?;
    assert_eq!(f0.input_ids, f1.input_ids);
    assert_ne!(f0.prompt, f1.prompt);
    assert_ne!(f0.input_digest, f1.input_digest);
    assert_ne!(
        Instruction::Publisher.digest()?,
        Instruction::SoftwareEvidenceV1.digest()?
    );
    assert!(serde_json::from_str::<Instruction>("\"arbitrary_template\"").is_err());
    Ok(())
}
#[test]
fn census_retains_refusals_and_original_denominator_without_refill() -> Result<()> {
    let c = compiler()?;
    let good = pair();
    let mut bad = pair();
    bad.id = "overflow".into();
    bad.document = "x".repeat(8193);
    bad.document_sha256 = Sha256Digest::from_bytes(Sha256::digest(bad.document.as_bytes()).into());
    let v = census(&c, vec![good, bad], Instant::now() + Duration::from_secs(2))?;
    assert_eq!(v.original_denominator, 2);
    assert_eq!(v.rows.len(), 2);
    assert!(v.rows[0].publisher.input().is_some());
    assert!(matches!(v.rows[1].publisher, Outcome::Refused { .. }));
    assert!(matches!(
        v.rows[1].software_evidence_v1,
        Outcome::Refused { .. }
    ));
    assert!(!v.model_requests_dispatched);
    assert!(!v.qualification);
    Ok(())
}
#[test]
fn duplicate_changed_unknown_and_expired_inputs_refuse() -> Result<()> {
    let c = compiler()?;
    let p = pair();
    assert!(
        census(
            &c,
            vec![p.clone(), p.clone()],
            Instant::now() + Duration::from_secs(2)
        )
        .is_err()
    );
    let mut bad = p.clone();
    bad.label = 2;
    assert!(compile_pair(&c, Instruction::Publisher, &bad).is_err());
    let mut bad = p.clone();
    bad.question.push('x');
    assert!(compile_pair(&c, Instruction::Publisher, &bad).is_err());
    assert!(census(&c, vec![p], Instant::now() - Duration::from_secs(1)).is_err());
    Ok(())
}
#[test]
fn raw_no_yes_readout_preserves_order_signed_zero_and_finite_bits() -> Result<()> {
    let c = compiler()?;
    let input = compile_pair(&c, Instruction::Publisher, &pair())?;
    let v = raw_scores(&input, Instruction::Publisher, &[-0.0, 3.25], 7)?;
    assert_eq!(v.no_bits, 0x80000000);
    assert_eq!(v.yes_bits, 3.25_f32.to_bits());
    assert_eq!(v.elapsed_us, 7);
    assert!(raw_scores(&input, Instruction::Publisher, &[1.0], 0).is_err());
    assert!(raw_scores(&input, Instruction::Publisher, &[f32::NAN, 1.0], 0).is_err());
    Ok(())
}

mod emission;
