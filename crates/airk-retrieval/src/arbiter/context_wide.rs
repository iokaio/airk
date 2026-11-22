// SPDX-License-Identifier: Apache-2.0
//! Private evaluation change set 2, Context option B: a wider roster, and the
//! best Read is chosen even when a terminal card scores highest.
//!
//! The product task is `context_action`, whose task policy hashes that whole
//! source file; it stays byte-identical. Option B has its own task policy,
//! binding the product policy and this module's source. Only a profile that
//! declares exactly this policy selects it, and only test-support builds admit
//! such a profile (`policy_for`). The kernel builds a B roster only when the
//! private fixture binds the `context_b` layer. The prompt rendering is the
//! product's compact view, proven equal on product rosters by a parity test.
use super::{compiler::Candidate, context_action, tasks::Prompt};
use airk_core::{
    Hit,
    arbiter::context_action::{
        ContextAction, ContextActionRoster, MAX_ACTION_CARDS, calibration::JointCalibration,
    },
    arbiter::{EvidenceRecord, EvidenceRole, Gap, NativeRead, ReadChoice, ScopeBinding},
    identity::{Sha256Digest, framed_digest},
    native_context_action::NativeContextActionRequest,
};
use anyhow::{Result, ensure};
use serde_json::json;

/// Source units shown (product: 4); the Core roster bound is 8.
pub const UNITS: usize = 8;
/// Read cards offered (product: 5). With Sufficient and Clarify this fills the
/// Core bound of 8 cards; NoJustifiedRead joins only while a card slot is free.
pub const READS: usize = 6;

pub fn policy_digest() -> Result<Sha256Digest> {
    let source = std::str::from_utf8(include_bytes!("context_wide.rs"))?.replace("\r\n", "\n");
    ensure!(!source.contains('\r'), "lone CR in option-B task policy");
    Ok(framed_digest(
        b"airk.private.context-action-task-policy/option-b1",
        &[
            context_action::policy_digest()?.as_bytes(),
            source.as_bytes(),
        ],
    ))
}

/// The Context task policy a profile is scored under: the product policy, or
/// (test-support builds only) option B when the profile declares exactly it.
pub fn policy_for(profile: &airk_core::arbiter::Profile) -> Result<Sha256Digest> {
    #[cfg(any(test, feature = "test-support"))]
    {
        let wide = policy_digest()?;
        if profile
            .task_policies
            .get(&airk_core::arbiter::Task::ContextAction)
            == Some(&wide)
        {
            return Ok(wide);
        }
    }
    let _ = profile;
    context_action::policy_digest()
}

fn is_wide(policy: Sha256Digest) -> Result<bool> {
    #[cfg(any(test, feature = "test-support"))]
    let wide = policy == policy_digest()?;
    #[cfg(not(any(test, feature = "test-support")))]
    let wide = {
        let _ = policy;
        false
    };
    Ok(wide)
}

/// The owner's prompt for the roster's policy (product unless option B).
pub fn prompt_for(request: &NativeContextActionRequest) -> Result<Prompt> {
    if is_wide(request.roster.policy)? {
        prompt(request)
    } else {
        context_action::prompt(request)
    }
}

/// The owner's selection for the roster's policy (product unless option B).
pub fn selected_action_for(
    scores: &[f32],
    roster: &ContextActionRoster,
    calibration: &JointCalibration,
) -> Result<Option<Sha256Digest>> {
    if is_wide(roster.policy)? {
        selected_action(scores, roster, calibration)
    } else {
        context_action::selected_action(scores, roster, calibration)
    }
}

/// The product constructor with option B's widths and policy.
pub fn build_roster_with_eligible(
    question: &str,
    scope: &ScopeBinding,
    records: &[EvidenceRecord],
    choices: &[ReadChoice],
    eligible: &std::collections::BTreeSet<Sha256Digest>,
) -> Result<ContextActionRoster> {
    use airk_core::arbiter::context_action::*;
    let policy = policy_digest()?;
    let mut units = Vec::new();
    let mut available = 0usize;
    let mut partial = 0usize;
    for record in records.iter().rev() {
        ensure!(
            &record.scope == scope,
            "foreign joint construction evidence"
        );
        if !matches!(
            record.operation,
            NativeRead::Search | NativeRead::Context | NativeRead::ReadEvidence
        ) {
            continue;
        }
        if let Some(hits) = record
            .canonical_result
            .get("hits")
            .and_then(serde_json::Value::as_array)
        {
            for (index, value) in hits.iter().enumerate() {
                let hit: Hit = serde_json::from_value(value.clone())?;
                if hit.text.is_empty() {
                    continue;
                }
                available += 1;
                if units.len() >= UNITS {
                    continue;
                }
                let mut end = hit.text.len().min(192);
                while !hit.text.is_char_boundary(end) {
                    end -= 1;
                }
                let unit = EvidenceRef {
                    record: record.id,
                    kind: SourceUnitKind::Hit,
                    result_pointer: format!("/hits/{index}"),
                    utf8_start: 0,
                    utf8_end: end,
                };
                partial += usize::from(unit.validate(records, scope)?);
                units.push(unit);
            }
        }
    }
    ensure!(!units.is_empty(), "joint assessment has no source units");
    let offered: Vec<_> = choices
        .iter()
        .filter(|choice| eligible.contains(&choice.id))
        .take(READS)
        .collect();
    let scored: Vec<_> = offered.iter().map(|choice| choice.id).collect();
    let basis = AssessmentBasis {
        source_units_available: available,
        source_units_omitted: available - units.len(),
        source_units_presented: units.clone(),
        source_units_partial: partial,
        read_choices_available: choices.len(),
        read_choices_omitted: choices.len() - scored.len(),
        read_choices_scored: scored,
    };
    let input_digest = framed_digest(
        b"airk.arbiter.context-action-basis/1",
        &[&serde_json::to_vec(&(
            question, scope, records, choices, &basis, policy,
        ))?],
    );
    let support: Vec<_> = units
        .iter()
        .map(|unit| EvidenceAssessment {
            evidence: unit.clone(),
            role: EvidenceRole::Document,
            relevance: RelevanceHypothesis::Useful,
        })
        .collect();
    // Reads, Sufficient and Clarify; NoJustifiedRead only into a free slot.
    let free_slot = offered.len() + 2 < MAX_ACTION_CARDS;
    let mut cards = Vec::new();
    let mut add = |action, gap, reason, assessments: Vec<EvidenceAssessment>| -> Result<()> {
        let id = framed_digest(
            b"airk.arbiter.context-action-card/1",
            &[
                input_digest.as_bytes(),
                policy.as_bytes(),
                &serde_json::to_vec(&(action, gap, reason, &assessments))?,
            ],
        );
        cards.push(ContextActionCard {
            id,
            action,
            gap,
            reason,
            assessments,
        });
        Ok(())
    };
    for choice in offered {
        let hypotheses = support
            .iter()
            .cloned()
            .map(|mut unit| {
                unit.role = EvidenceRole::Unknown;
                unit.relevance = RelevanceHypothesis::Uncertain;
                unit
            })
            .collect();
        add(
            ContextAction::Read { choice: choice.id },
            Gap::Unsupported,
            ContextReason::AdditionalEvidenceMayHelp,
            hypotheses,
        )?;
    }
    add(
        ContextAction::Sufficient,
        Gap::Sufficient,
        ContextReason::RetainedEvidenceCoversQuestion,
        support,
    )?;
    add(
        ContextAction::Clarify,
        Gap::Ambiguous,
        ContextReason::AmbiguousQuestion,
        vec![],
    )?;
    if basis.read_choices_omitted == 0 && free_slot {
        add(
            ContextAction::NoJustifiedRead,
            Gap::Unsupported,
            ContextReason::NoVerifiedUsefulRead,
            vec![],
        )?;
    }
    let roster = ContextActionRoster {
        schema: ACTION_ROSTER_SCHEMA.into(),
        scope: scope.clone(),
        input_digest,
        policy,
        basis,
        cards,
    };
    roster.validate(records)?;
    Ok(roster)
}

/// Option B's prompt: the same request checks and rebuild proof as the
/// product's `prompt`, then the product's compact rendering.
pub fn prompt(request: &NativeContextActionRequest) -> Result<Prompt> {
    let roster = &request.roster;
    roster.validate(&request.records)?;
    ensure!(
        request.scope == roster.scope
            && roster.policy == policy_digest()?
            && !request.question.trim().is_empty()
            && request.question.len() <= 16_384,
        "joint task input/policy mismatch"
    );
    let eligible = roster.basis.read_choices_scored.iter().copied().collect();
    let rebuilt = build_roster_with_eligible(
        &request.question,
        &roster.scope,
        &request.records,
        &request.choices,
        &eligible,
    )?;
    ensure!(
        serde_json::to_value(rebuilt)? == serde_json::to_value(roster)?,
        "joint roster differs from actual native question/menu constructor"
    );
    render(&request.question, &request.records, &request.choices, roster)
}

/// First-use ordered table of distinct file paths.
#[derive(Default)]
struct PathTable(Vec<String>);
impl PathTable {
    fn index(&mut self, path: &str) -> usize {
        if let Some(index) = self.0.iter().position(|known| known == path) {
            return index;
        }
        self.0.push(path.to_owned());
        self.0.len() - 1
    }
}

fn tag<T: serde::Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(text) => Ok(text),
        _ => anyhow::bail!("closed joint label is not a plain tag"),
    }
}

/// The product's compact, lossless roster view (`context_action::compact_prompt`,
/// private there), for any roster; no policy check. The kernel parity test
/// `option_b_rendering_equals_the_product_prompt_on_a_product_roster` holds
/// the two equal.
pub fn render(
    question: &str,
    records: &[EvidenceRecord],
    choices: &[ReadChoice],
    roster: &ContextActionRoster,
) -> Result<Prompt> {
    use airk_core::arbiter::literal::SourceLiteral;
    let mut paths = PathTable::default();
    let mut evidence = Vec::new();
    for unit in &roster.basis.source_units_presented {
        let record = records
            .iter()
            .find(|record| record.id == unit.record)
            .ok_or_else(|| anyhow::anyhow!("joint task source missing"))?;
        let hit: Hit = serde_json::from_value(
            record
                .canonical_result
                .pointer(&unit.result_pointer)
                .ok_or_else(|| anyhow::anyhow!("joint task source unit missing"))?
                .clone(),
        )?;
        let excerpt = hit
            .text
            .get(unit.utf8_start..unit.utf8_end)
            .ok_or_else(|| anyhow::anyhow!("joint task source window invalid"))?;
        evidence.push(json!([
            paths.index(&hit.citation.path),
            hit.citation.start_line,
            hit.citation.end_line,
            unit.utf8_start,
            unit.utf8_end,
            hit.text.len(),
            hit.truncated,
            excerpt
        ]));
    }
    let mut reads = Vec::new();
    for id in &roster.basis.read_choices_scored {
        let choice = choices
            .iter()
            .find(|choice| choice.id == *id)
            .ok_or_else(|| anyhow::anyhow!("joint verified read not captured"))?;
        let operation = tag(&choice.operation)?;
        reads.push(match SourceLiteral::parse(&choice.target)? {
            Some(source) => json!([
                "literal",
                operation,
                paths.index(&source.path),
                source.line,
                source.returned_utf8_start,
                source.returned_utf8_end,
                source.query
            ]),
            None => json!(["target", operation, choice.target]),
        });
    }
    let mut sets: Vec<Vec<serde_json::Value>> = Vec::new();
    let mut candidates = Vec::new();
    for (index, card) in roster.cards.iter().enumerate() {
        let mut hypotheses = Vec::new();
        for assessment in &card.assessments {
            let position = roster
                .basis
                .source_units_presented
                .iter()
                .position(|unit| unit == &assessment.evidence)
                .ok_or_else(|| anyhow::anyhow!("joint hypothesis unit not presented"))?;
            hypotheses.push(json!([
                position + 1,
                tag(&assessment.role)?,
                tag(&assessment.relevance)?
            ]));
        }
        let hypotheses = if hypotheses.is_empty() {
            "none".to_owned()
        } else {
            let known = sets.iter().position(|set| *set == hypotheses);
            let set = known.unwrap_or_else(|| {
                sets.push(hypotheses);
                sets.len() - 1
            });
            format!("h{}", set + 1)
        };
        let action = match card.action {
            ContextAction::Read { choice } => {
                let position = roster
                    .basis
                    .read_choices_scored
                    .iter()
                    .position(|id| *id == choice)
                    .ok_or_else(|| anyhow::anyhow!("joint card read not scored"))?;
                format!("read r{}", position + 1)
            }
            ContextAction::Sufficient => "sufficient".to_owned(),
            ContextAction::Clarify => "clarify".to_owned(),
            ContextAction::NoJustifiedRead => "no_justified_read".to_owned(),
        };
        candidates.push(Candidate {
            id: format!("a{}", index + 1),
            text: format!(
                "{action}; gap {}; reason {}; hypotheses {hypotheses}",
                tag(&card.gap)?,
                tag(&card.reason)?
            ),
        });
    }
    let hypothesis_sets: serde_json::Map<String, serde_json::Value> = sets
        .into_iter()
        .enumerate()
        .map(|(index, set)| (format!("h{}", index + 1), serde_json::Value::Array(set)))
        .collect();
    let state = json!({"user_question":question,
        "format":"Rows are positional. paths: file paths, named elsewhere by 0-based index. indexed_evidence[i] is e(i+1) = [path, start_line, end_line, utf8_start, utf8_end, original_utf8_bytes, native_hit_truncated, text_excerpt]. verified_reads[i] is r(i+1) = [\"literal\", operation, path, line, utf8_start, utf8_end, query] or [\"target\", operation, native_target]. hypothesis_sets: h(k) lists [evidence number, role, relevance] and is shared by every action naming it; none means no hypotheses.",
        "paths":paths.0,"indexed_evidence":evidence,"verified_reads":reads,
        "hypothesis_sets":hypothesis_sets,
        "source_units_available":roster.basis.source_units_available,
        "source_units_omitted":roster.basis.source_units_omitted,
        "source_units_partial":roster.basis.source_units_partial,
        "read_choices_available":roster.basis.read_choices_available,
        "read_choices_omitted":roster.basis.read_choices_omitted,
        "qualification":"Whole-card hypotheses are fallible; omitted source units and unscored reads are not negative evidence."}).to_string();
    ensure!(
        state.len() <= 65_536,
        "joint evidence does not fit unchanged compiler state bound"
    );
    Ok(Prompt {
        state,
        question: context_action::ACTION_QUESTION.into(),
        candidates,
        aliases: vec![],
        policy: roster.policy,
    })
}

/// Option B selection: the highest-scoring Read card, whatever the terminal
/// cards score. Its margin is over the next Read (0 when it is the only Read);
/// a tie between Reads abstains. The calibration still decides acceptance.
pub fn selected_action(
    scores: &[f32],
    roster: &ContextActionRoster,
    calibration: &JointCalibration,
) -> Result<Option<Sha256Digest>> {
    ensure!(
        !scores.is_empty()
            && scores.len() <= MAX_ACTION_CARDS
            && scores.len() == roster.cards.len()
            && scores.iter().all(|score| score.is_finite())
            && calibration.task_policy == roster.policy
            && roster.policy == policy_digest()?,
        "invalid joint task score shape/policy"
    );
    let reads: Vec<usize> = (0..roster.cards.len())
        .filter(|index| matches!(roster.cards[*index].action, ContextAction::Read { .. }))
        .collect();
    let Some(best) = reads
        .iter()
        .copied()
        .reduce(|left, right| if scores[right] > scores[left] { right } else { left })
    else {
        return Ok(None);
    };
    let runner = reads
        .iter()
        .filter(|index| **index != best)
        .map(|index| scores[*index])
        .reduce(f32::max);
    let margin = match runner {
        Some(runner) if runner == scores[best] => return Ok(None),
        Some(runner) => scores[best] - runner,
        None => 0.0,
    };
    Ok((margin.is_finite() && calibration.accepts(&roster.cards[best], margin))
        .then_some(roster.cards[best].id))
}
