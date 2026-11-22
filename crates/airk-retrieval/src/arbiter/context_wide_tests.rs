// SPDX-License-Identifier: Apache-2.0
//! Change set 2: Context option B renders exactly the product prompt. For every
//! case of the shared Context fixture, with literal and stated-target reads,
//! `context_wide::render` of a product roster equals the product owner's
//! `prompt_from_parts`; option-B rosters render with the same row formats.
use super::{context_action, context_wide};
use airk_core::{
    Hit,
    arbiter::{
        EvidenceRecord, NativeRead, ReadChoice, context_action::ContextResponseV2,
        literal::SourceLiteral,
    },
    identity::Sha256Digest,
};
use anyhow::{Context, Result};
use serde_json::Value;

fn cases() -> Result<Vec<(String, ContextResponseV2)>> {
    let raw: Value = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/split/arbiter/context_action_cases.json"
    ))?;
    raw["cases"]
        .as_array()
        .context("fixture cases")?
        .iter()
        .map(|case| {
            Ok((
                case["name"].as_str().unwrap_or_default().to_owned(),
                serde_json::from_value(case["input"].clone())?,
            ))
        })
        .collect()
}

/// One literal read and `targets` stated-target reads anchored on the record's first hit.
fn choices(record: &EvidenceRecord, targets: u8) -> Result<Vec<ReadChoice>> {
    let hit: Hit = serde_json::from_value(record.canonical_result["hits"][0].clone())?;
    let (path, start, end) = (&hit.citation.path, hit.citation.start_line, hit.citation.end_line);
    let literal = SourceLiteral {
        record: record.id,
        path: path.clone(),
        line: start,
        returned_utf8_start: 0,
        returned_utf8_end: 13,
        query: "Native Answer".into(),
    };
    let mut reads = vec![ReadChoice {
        id: Sha256Digest::from_bytes([40; 32]),
        operation: NativeRead::Search,
        target: literal.target()?,
    }];
    for n in 0..targets {
        reads.push(ReadChoice {
            id: Sha256Digest::from_bytes([41 + n; 32]),
            operation: if n % 2 == 0 { NativeRead::Context } else { NativeRead::Definitions },
            target: format!("Read the whole indexed chunk of {path} at {path}:{start}-{}", end + usize::from(n)),
        });
    }
    Ok(reads)
}

#[test]
fn option_b_render_equals_the_product_prompt_on_every_fixture_case() -> Result<()> {
    let mut compared = 0;
    for (name, case) in cases()? {
        let record = case.records.first().context("fixture record")?;
        for targets in [1, 6] {
            let reads = choices(record, targets)?;
            let product =
                context_action::build_roster(&case.question, &record.scope, &case.records, &reads)?;
            let official = context_action::prompt_from_parts(&case.question, &case.records, &reads, &product)?;
            let rendered = context_wide::render(&case.question, &case.records, &reads, &product)?;
            assert_eq!(
                serde_json::to_value(&rendered)?,
                serde_json::to_value(&official)?,
                "{name} with {targets} target reads"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 18, "nine fixture cases, two read menus each");
    Ok(())
}

#[test]
fn option_b_rosters_render_literal_and_target_rows_and_every_card() -> Result<()> {
    for (name, case) in cases()? {
        let record = case.records.first().context("fixture record")?;
        for (targets, reads_offered, cards) in [(1, 2, 5), (6, context_wide::READS, 8)] {
            let reads = choices(record, targets)?;
            let eligible = reads.iter().map(|read| read.id).collect();
            let wide = context_wide::build_roster_with_eligible(
                &case.question,
                &record.scope,
                &case.records,
                &reads,
                &eligible,
            )?;
            assert_eq!(wide.policy, context_wide::policy_digest()?, "{name}");
            assert_eq!(wide.basis.read_choices_scored.len(), reads_offered, "{name}");
            assert_eq!(wide.cards.len(), cards, "{name}");
            let prompt = context_wide::render(&case.question, &case.records, &reads, &wide)?;
            assert_eq!(prompt.candidates.len(), wide.cards.len());
            assert_eq!(prompt.policy, wide.policy);
            assert_eq!(prompt.question, context_action::ACTION_QUESTION);
            let state: Value = serde_json::from_str(&prompt.state)?;
            let rows = state["verified_reads"].as_array().context("verified reads")?;
            assert_eq!(rows.len(), reads_offered);
            assert_eq!(rows[0][0], "literal");
            assert_eq!(rows[0][6], "Native Answer");
            assert!(rows[1..].iter().all(|row| row[0] == "target"));
            // The product owner refuses an option-B roster.
            assert!(context_action::prompt_from_parts(&case.question, &case.records, &reads, &wide).is_err());
        }
    }
    Ok(())
}