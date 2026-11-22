// SPDX-License-Identifier: Apache-2.0
//! Split proposals and the deterministic partition.
use super::*;

fn texts(question: &str, partition: &Partition) -> Vec<String> {
    partition
        .0
        .iter()
        .map(|range| {
            let (from, to) = part_span(question, *range);
            question[from..to].to_owned()
        })
        .collect()
}

#[test]
fn simple_request_stays_one_part_and_scores_nothing() {
    let question = "Explain how the restart guard decides whether to replay a read.";
    let plan = propose(question, 4);
    assert!(plan.breaks.is_empty());
    assert_eq!(plan.cards, [Partition(vec![(0, question.len())])]);
    assert_eq!(plan.deterministic, 0);
}

#[test]
fn enumeration_and_separate_questions_split_without_a_model() {
    let question = "Please help with these:\n1. explain how the restart guard works\n2. update the README section on search\nWhere are the guard tests kept?";
    let plan = propose(question, 4);
    let chosen = &plan.cards[plan.deterministic];
    assert_eq!(
        texts(question, chosen),
        [
            "Please help with these:\n1. explain how the restart guard works",
            "2. update the README section on search",
            "Where are the guard tests kept?"
        ]
    );
    let questions = "Where is the queue policy defined? Which tests cover the retry budget?";
    let plan = propose(questions, 4);
    assert_eq!(
        texts(questions, &plan.cards[plan.deterministic]),
        [
            "Where is the queue policy defined?",
            "Which tests cover the retry budget?"
        ]
    );
}

#[test]
fn sentence_and_join_breaks_are_offered_but_not_taken_deterministically() {
    let question =
        "Explain how the restart guard works, and also update the README section on search.";
    let plan = propose(question, 4);
    assert_eq!(
        plan.deterministic, 0,
        "a join alone is not an explicit break"
    );
    assert!(plan.breaks.iter().any(|item| item.kind == BreakKind::Join));
    let joined = plan
        .cards
        .iter()
        .find(|card| card.0.len() == 2)
        .expect("a join card is offered");
    assert_eq!(
        texts(question, joined),
        [
            "Explain how the restart guard works,",
            "update the README section on search."
        ]
    );
    let verbs = "Explain the restart guard and update the search README now please.";
    assert!(
        breaks(verbs)
            .iter()
            .any(|item| item.kind == BreakKind::Verbs)
    );
    let sentences =
        "The guard replays nothing after a timeout. Explain why the ledger keeps the slot.";
    let plan = propose(sentences, 4);
    assert_eq!(plan.deterministic, 0);
    assert!(plan.cards.iter().any(|card| card.0.len() == 2));
}

#[test]
fn abbreviations_and_short_tails_do_not_break() {
    let question = "Find the policy, e.g. the retry budget rules in the service. Ok?";
    assert!(
        breaks(question)
            .iter()
            .all(|item| item.kind != BreakKind::Sentence)
    );
}

#[test]
fn more_than_four_breaks_merge_shortest_adjacent_parts() {
    let question = "Do all of these tasks:\n1. explain the restart guard path\n2. update the search docs now\n3. add a test for retries here\n4. fix the queue policy wording\n5. rename the ledger window field";
    let plan = propose(question, 4);
    let chosen = &plan.cards[plan.deterministic];
    assert_eq!(chosen.0.len(), 4);
    let two = propose(question, 2);
    assert_eq!(two.cards[two.deterministic].0.len(), 2);
}

#[test]
fn part_ranges_are_exact_utf8_slices() {
    let question =
        "Où est défini le garde de redémarrage ? Quels tests couvrent le budget de relance ?";
    let plan = propose(question, 4);
    for card in &plan.cards {
        let mut next = 0;
        for &(start, end) in &card.0 {
            assert_eq!(start, next);
            assert!(question.is_char_boundary(start) && question.is_char_boundary(end));
            let (from, to) = part_span(question, (start, end));
            assert!(start <= from && from < to && to <= end);
            next = end;
        }
        assert_eq!(next, question.len());
    }
    let chosen = &plan.cards[plan.deterministic];
    assert_eq!(chosen.0.len(), 2);
    let shown = card(question, chosen, "B");
    assert_eq!(shown.class, v3::CardClass::SplitAt);
    v3::validate_partition(question, &shown.partition).unwrap();
    assert!(shown.text.len() <= v3::MAX_TEXT_BYTES);
    assert_eq!(
        card(question, &plan.cards[0], "A").class,
        v3::CardClass::KeepWhole
    );
}
