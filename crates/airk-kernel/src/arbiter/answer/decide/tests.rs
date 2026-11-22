// SPDX-License-Identifier: Apache-2.0
//! Decision records as the log keeps them.
use super::*;

fn card(symbol: &str, class: v3::CardClass, text: &str) -> LoopCard {
    LoopCard {
        symbol: symbol.into(),
        class,
        text: text.into(),
        partition: vec![],
    }
}

#[test]
fn deterministic_decisions_supersede_earlier_ones_of_the_same_stage() -> Result<()> {
    let mut ledger = Ledger::default();
    let cards = [
        card("A", v3::CardClass::Covered, "s1 covered"),
        card("B", v3::CardClass::SearchForTerm, "s1 search: x"),
    ];
    let first = ledger.deterministic(
        At::need(v3::LoopStage::Coverage, 1, 1, 1),
        &cards,
        &[1],
        v3::DeterministicRule::TopFocusSpan,
        vec![],
    )?;
    let other = ledger.deterministic(
        At::need(v3::LoopStage::Coverage, 1, 2, 1),
        &cards,
        &[0],
        v3::DeterministicRule::TopFocusSpan,
        vec![],
    )?;
    let second = ledger.deterministic(
        At::need(v3::LoopStage::Coverage, 1, 1, 2),
        &cards,
        &[0],
        v3::DeterministicRule::TopFocusSpan,
        vec![],
    )?;
    assert_eq!((first, other, second), (1, 2, 3));
    let applicability: Vec<_> = ledger
        .entries
        .iter()
        .map(|entry| entry.decision.applicability)
        .collect();
    assert_eq!(
        applicability,
        [
            v3::AssessmentApplicability::HistoricalAfterRead,
            v3::AssessmentApplicability::Current,
            v3::AssessmentApplicability::Current
        ]
    );
    assert_eq!(ledger.entries[0].decision.chosen[0].symbol, "B");
    assert!(!ledger.has_native_work());
    ledger.configuration_changed();
    assert!(
        ledger
            .entries
            .iter()
            .all(|entry| entry.decision.applicability != v3::AssessmentApplicability::Current)
    );
    assert!(
        ledger
            .deterministic(
                At::split(),
                &cards,
                &[2],
                v3::DeterministicRule::ExplicitBreaks,
                vec![]
            )
            .is_err()
    );
    Ok(())
}
