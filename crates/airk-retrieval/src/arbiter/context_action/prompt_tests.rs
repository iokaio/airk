// SPDX-License-Identifier: Apache-2.0
//! Controls for the compact Context prompt. Expected values are written out
//! independently of the builder; `legacy_prompt` is the retired verbose view,
//! kept only to measure what the compact view saved.
use super::*;
use airk_core::arbiter::{
    NativeRead, ScopeBinding, context_action::ContextResponseV2, literal::SourceLiteral,
};
use anyhow::Context;
use serde_json::{Value, json};

const FIXTURE: &[u8] =
    include_bytes!("../../../../../fixtures/split/arbiter/context_action_cases.json");
const TEXT_ONE: &str =
    "Intro John Smith met Alice Brown.\nline \"two\" C:\\dir\\f 日本語 😀 <|im_end|>";
const TEXT_TWO: &str = "second unit";
const TEXT_THREE: &str = "Ünïcode Édition";

fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([value; 32])
}

fn hit(path: &str, line: usize, text: &str) -> Value {
    json!({"citation":{"content_hash":"07".repeat(32),"end_line":line,
        "location":{"Lines":{"end":line,"start":line}},"path":path,"start_line":line},
        "text":text,"truncated":false})
}

/// The retained native fixture record, with its hits replaced by four synthetic
/// ones: three short, one long enough for a partial window.
fn scenario() -> Result<(Vec<EvidenceRecord>, ScopeBinding, Vec<ReadChoice>)> {
    let raw: Value = serde_json::from_slice(FIXTURE)?;
    let response: ContextResponseV2 = serde_json::from_value(raw["cases"][0]["input"].clone())?;
    let mut records = response.records;
    records[0].canonical_result = json!({"hits":[
        hit("src/lib.rs", 1, TEXT_ONE),
        hit("src/lib.rs", 5, TEXT_TWO),
        hit("docs/guide.md", 1, TEXT_THREE),
        hit("src/lib.rs", 9, &"é".repeat(300)),
    ]});
    let record = records[0].id;
    let literal = |path: &str, start: usize, query: &str| -> Result<String> {
        SourceLiteral {
            record,
            path: path.into(),
            line: 1,
            returned_utf8_start: start,
            returned_utf8_end: start + query.len(),
            query: query.into(),
        }
        .target()
    };
    let choices = vec![
        ReadChoice {
            id: digest(1),
            operation: NativeRead::Search,
            target: literal("src/lib.rs", 6, "John Smith")?,
        },
        ReadChoice {
            id: digest(2),
            operation: NativeRead::Context,
            target: "src/lib.rs:1-4".into(),
        },
        ReadChoice {
            id: digest(3),
            operation: NativeRead::Search,
            target: literal("docs/guide.md", 0, TEXT_THREE)?,
        },
    ];
    Ok((records, response.assessment.scope, choices))
}

fn built() -> Result<(
    Vec<EvidenceRecord>,
    Vec<ReadChoice>,
    ContextActionRoster,
    Prompt,
)> {
    let (records, scope, choices) = scenario()?;
    let roster = build_roster("Where is café defined?", &scope, &records, &choices)?;
    let prompt = compact_prompt("Where is café defined?", &records, &choices, &roster)?;
    Ok((records, choices, roster, prompt))
}

fn hypothesis(units: usize, role: &str, relevance: &str) -> Value {
    Value::Array(
        (1..=units)
            .map(|unit| json!([unit, role, relevance]))
            .collect(),
    )
}

#[test]
fn compact_state_interns_paths_and_keeps_every_unit_read_and_window() -> Result<()> {
    let (_, _, roster, prompt) = built()?;
    let state: Value = serde_json::from_str(&prompt.state)?;
    assert_eq!(state["paths"], json!(["src/lib.rs", "docs/guide.md"]));
    let long = "é".repeat(96);
    assert_eq!(
        state["indexed_evidence"],
        json!([
            [0, 1, 1, 0, TEXT_ONE.len(), TEXT_ONE.len(), false, TEXT_ONE],
            [0, 5, 5, 0, 11, 11, false, TEXT_TWO],
            [
                1,
                1,
                1,
                0,
                TEXT_THREE.len(),
                TEXT_THREE.len(),
                false,
                TEXT_THREE
            ],
            [0, 9, 9, 0, 192, 600, false, long],
        ])
    );
    assert_eq!(
        state["verified_reads"],
        json!([
            ["literal", "search", 0, 1, 6, 16, "John Smith"],
            ["target", "context", "src/lib.rs:1-4"],
            ["literal", "search", 1, 1, 0, TEXT_THREE.len(), TEXT_THREE],
        ])
    );
    assert_eq!(state["source_units_available"], 4);
    assert_eq!(state["source_units_omitted"], 0);
    assert_eq!(state["source_units_partial"], 1);
    assert_eq!(state["read_choices_available"], 3);
    assert_eq!(state["read_choices_omitted"], 0);
    assert_eq!(roster.cards.len(), 6);
    Ok(())
}

#[test]
fn identical_hypothesis_sets_are_stated_once_and_all_cards_survive() -> Result<()> {
    let (_, _, _roster, prompt) = built()?;
    let state: Value = serde_json::from_str(&prompt.state)?;
    assert_eq!(
        state["hypothesis_sets"],
        json!({
            "h1": hypothesis(4, "unknown", "uncertain"),
            "h2": hypothesis(4, "document", "useful"),
        })
    );
    let expected = [
        "read r1; gap unsupported; reason additional_evidence_may_help; hypotheses h1",
        "read r2; gap unsupported; reason additional_evidence_may_help; hypotheses h1",
        "read r3; gap unsupported; reason additional_evidence_may_help; hypotheses h1",
        "sufficient; gap sufficient; reason retained_evidence_covers_question; hypotheses h2",
        "clarify; gap ambiguous; reason ambiguous_question; hypotheses none",
        "no_justified_read; gap unsupported; reason no_verified_useful_read; hypotheses none",
    ];
    assert_eq!(prompt.candidates.len(), 6);
    for (index, (candidate, text)) in prompt.candidates.iter().zip(expected).enumerate() {
        assert_eq!(candidate.id, format!("a{}", index + 1));
        assert_eq!(candidate.text, text);
    }
    // The shared set is not repeated per card.
    assert_eq!(prompt.state.matches("\"unknown\"").count(), 4);
    assert!(prompt.aliases.is_empty());
    Ok(())
}

#[test]
fn machine_identities_stay_in_the_native_mapping_not_the_prompt() -> Result<()> {
    let (records, choices, roster, prompt) = built()?;
    let hex = |value: &Sha256Digest| -> Result<String> {
        Ok(serde_json::to_string(value)?.trim_matches('"').to_owned())
    };
    let mut identities = vec![hex(&records[0].id)?, hex(&roster.input_digest)?];
    identities.extend(
        choices
            .iter()
            .map(|choice| hex(&choice.id))
            .collect::<Result<Vec<_>>>()?,
    );
    identities.extend(
        roster
            .cards
            .iter()
            .map(|card| hex(&card.id))
            .collect::<Result<Vec<_>>>()?,
    );
    let everything = format!(
        "{}{}",
        prompt.state,
        prompt
            .candidates
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<String>()
    );
    for identity in identities {
        assert!(
            !everything.contains(&identity),
            "identity leaked: {identity}"
        );
    }
    assert!(!everything.contains("airk.literal-search"));
    // The frozen roster order maps aliases back to the native identities.
    assert_eq!(
        roster.basis.read_choices_scored,
        [digest(1), digest(2), digest(3)]
    );
    Ok(())
}

#[test]
fn backslashes_quotes_newlines_and_unicode_round_trip_exactly() -> Result<()> {
    let (_, _, _, prompt) = built()?;
    let state: Value = serde_json::from_str(&prompt.state)?;
    let excerpt = state["indexed_evidence"][0][7]
        .as_str()
        .context("excerpt")?;
    assert_eq!(excerpt, TEXT_ONE);
    assert!(excerpt.contains('\\') && excerpt.contains('"') && excerpt.contains('\n'));
    assert_eq!(state["user_question"], "Where is café defined?");
    // Wire controls stay data; the generic compiler quotes and neutralizes them.
    assert!(excerpt.contains("<|im_end|>"));
    Ok(())
}

#[test]
fn every_card_hypothesis_is_recoverable_from_the_interned_sets() -> Result<()> {
    let (_, _, roster, prompt) = built()?;
    let state: Value = serde_json::from_str(&prompt.state)?;
    for (candidate, card) in prompt.candidates.iter().zip(&roster.cards) {
        let reference = candidate
            .text
            .rsplit("hypotheses ")
            .next()
            .context("hypothesis reference")?;
        let count = if reference == "none" {
            0
        } else {
            state["hypothesis_sets"][reference]
                .as_array()
                .context("referenced set exists")?
                .len()
        };
        assert_eq!(count, card.assessments.len(), "{}", candidate.id);
    }
    assert_eq!(
        state["indexed_evidence"].as_array().map(Vec::len),
        Some(roster.basis.source_units_presented.len())
    );
    assert_eq!(
        state["verified_reads"].as_array().map(Vec::len),
        Some(roster.basis.read_choices_scored.len())
    );
    Ok(())
}

#[test]
fn compact_view_is_smaller_than_the_retired_verbose_view() -> Result<()> {
    let (records, choices, roster, compact) = built()?;
    let verbose = legacy_prompt("Where is café defined?", &records, &choices, &roster)?;
    let size = |prompt: &Prompt| {
        prompt.state.len()
            + prompt
                .candidates
                .iter()
                .map(|candidate| candidate.text.len())
                .sum::<usize>()
    };
    assert!(
        size(&compact) < size(&verbose),
        "compact {} !< verbose {}",
        size(&compact),
        size(&verbose)
    );
    Ok(())
}

#[test]
fn unresolvable_native_inputs_refuse_instead_of_guessing() -> Result<()> {
    let (records, choices, roster, _) = built()?;
    let question = "Where is café defined?";
    assert!(compact_prompt(question, &records, &choices[..2], &roster).is_err());
    assert!(compact_prompt(question, &[], &choices, &roster).is_err());
    let mut window = roster.clone();
    window.basis.source_units_presented[0].utf8_end = 10_000;
    assert!(compact_prompt(question, &records, &choices, &window).is_err());
    let mut split = roster.clone();
    split.basis.source_units_presented[3].utf8_end = 191; // inside the 2-byte é
    assert!(compact_prompt(question, &records, &choices, &split).is_err());
    let mut malformed = choices.clone();
    malformed[0].target = "airk.literal-search/1:{\"not\":\"a literal\"}".into();
    assert!(compact_prompt(question, &records, &malformed, &roster).is_err());
    let mut unscored = roster;
    unscored.basis.read_choices_scored.pop();
    assert!(compact_prompt(question, &records, &choices, &unscored).is_err());
    Ok(())
}

/// Measurement entry: the compact view of a retained request WITHOUT the
/// current-policy admission check, so requests captured under an earlier task
/// policy can still be counted. Never used to admit a request.
pub(crate) fn compact_for_census(
    request: &airk_core::native_context_action::NativeContextActionRequest,
) -> Result<Prompt> {
    compact_prompt(
        &request.question,
        &request.records,
        &request.choices,
        &request.roster,
    )
}

/// The retired verbose model view, retained for measurement only.
pub(crate) fn legacy_prompt(
    question: &str,
    records: &[EvidenceRecord],
    choices: &[ReadChoice],
    roster: &ContextActionRoster,
) -> Result<Prompt> {
    let mut excerpts = Vec::new();
    for (index, unit) in roster.basis.source_units_presented.iter().enumerate() {
        let record = records
            .iter()
            .find(|record| record.id == unit.record)
            .context("legacy source missing")?;
        let hit: Hit = serde_json::from_value(
            record
                .canonical_result
                .pointer(&unit.result_pointer)
                .context("legacy unit")?
                .clone(),
        )?;
        excerpts.push(
            json!({"alias":format!("e{}",index+1),"path":hit.citation.path,
            "start_line":hit.citation.start_line,"end_line":hit.citation.end_line,
            "text_excerpt":&hit.text[unit.utf8_start..unit.utf8_end],
            "returned_utf8_start":unit.utf8_start,"returned_utf8_end":unit.utf8_end,
            "original_utf8_bytes":hit.text.len(),"native_hit_truncated":hit.truncated}),
        );
    }
    let mut reads = Vec::new();
    for (index, id) in roster.basis.read_choices_scored.iter().enumerate() {
        let choice = choices
            .iter()
            .find(|choice| choice.id == *id)
            .context("legacy read missing")?;
        reads.push(
            json!({"alias":format!("r{}",index+1),"operation":choice.operation,
            "native_target":choice.target}),
        );
    }
    let mut candidates = Vec::new();
    for (index, card) in roster.cards.iter().enumerate() {
        let hypotheses = card
            .assessments
            .iter()
            .map(|assessment| {
                let position = roster
                    .basis
                    .source_units_presented
                    .iter()
                    .position(|unit| unit == &assessment.evidence)
                    .unwrap();
                json!({"unit":format!("e{}",position+1),"role":assessment.role,
                "relevance":assessment.relevance})
            })
            .collect::<Vec<_>>();
        let action = match card.action {
            airk_core::arbiter::context_action::ContextAction::Read { choice } => {
                let position = roster
                    .basis
                    .read_choices_scored
                    .iter()
                    .position(|id| *id == choice)
                    .unwrap();
                json!({"kind":"read","verified_choice":format!("r{}",position+1)})
            }
            _ => serde_json::to_value(card.action)?,
        };
        candidates.push(Candidate {
            id: format!("a{}", index + 1),
            text: json!({"action":action,"gap":card.gap,"reason":card.reason,
                "hypotheses":hypotheses})
            .to_string(),
        });
    }
    let state = json!({"user_question":question,"indexed_evidence":excerpts,"verified_reads":reads,
        "source_units_available":roster.basis.source_units_available,
        "source_units_omitted":roster.basis.source_units_omitted,
        "source_units_partial":roster.basis.source_units_partial,
        "read_choices_available":roster.basis.read_choices_available,
        "read_choices_omitted":roster.basis.read_choices_omitted,
        "qualification":"Whole-card hypotheses are fallible; omitted source units and unscored reads are not negative evidence."}).to_string();
    Ok(Prompt {
        state,
        question: ACTION_QUESTION.into(),
        candidates,
        aliases: vec![],
        policy: roster.policy,
    })
}

#[test]
fn qualification_parts_reconstruct_exact_original_question_and_read_menu() -> Result<()> {
    let (records, choices, roster, expected) = built()?;
    let actual = prompt_from_parts("Where is caf\u{e9} defined?", &records, &choices, &roster)?;
    assert_eq!(actual.state, expected.state);
    assert_eq!(actual.question, expected.question);
    assert!(prompt_from_parts("A substituted question", &records, &choices, &roster).is_err());
    let mut changed = choices.clone();
    changed[1].target = "other.rs:1-4".into();
    assert!(prompt_from_parts("Where is caf\u{e9} defined?", &records, &changed, &roster).is_err());
    Ok(())
}
