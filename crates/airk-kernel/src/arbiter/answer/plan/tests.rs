// SPDX-License-Identifier: Apache-2.0
//! Sub-need proposals, their wording and the deterministic plan.
use super::*;

fn summary(proposals: &[Proposal]) -> Vec<(NeedKind, String)> {
    proposals
        .iter()
        .map(|proposal| (proposal.kind, proposal.query.clone()))
        .collect()
}

#[test]
fn identifiers_paths_quotes_and_cues_propose_in_priority_order() {
    let part =
        "Fix `restart_guard` in crates/k/src/guard.rs so the \"retry refused\" error has a test";
    let proposals = propose(part, part);
    assert_eq!(
        summary(&proposals),
        [
            (NeedKind::Implementation, "crates/k/src/guard.rs".into()),
            (NeedKind::Contract, "crates/k/src contract rules".into()),
            (NeedKind::Definition, "restart_guard".into()),
            (NeedKind::Implementation, "restart_guard".into()),
            (NeedKind::Errors, "retry refused".into()),
            (NeedKind::Tests, "restart_guard test".into()),
            (NeedKind::Callers, "restart_guard".into()),
        ]
    );
    assert_eq!(proposals[4].subject.as_deref(), Some("retry refused"));
    assert!(proposals.len() <= MAX_PROPOSALS);
}

#[test]
fn deterministic_plan_takes_first_four() {
    let part = "Explain `ReadAttemptBudget` and `ScopeBinding` and `PartitionAllowance`, their tests and the governing contract rules";
    let proposals = propose(part, part);
    assert!(proposals.len() > 4);
    assert_eq!(deterministic(&proposals, 4), [0, 1, 2, 3]);
    assert_eq!(deterministic(&proposals, 2), [0, 1]);
    let needs = needs(&proposals, &deterministic(&proposals, 4));
    assert_eq!(
        needs.iter().map(|need| need.id).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert_eq!(needs[0].kind, NeedKind::Definition);
}

#[test]
fn reference_card_closes_the_roster_with_unique_texts() {
    let part = "Where is `Gathering` defined and who calls it?";
    let proposals = propose(part, part);
    let roster = cards(&proposals);
    assert_eq!(roster.len(), proposals.len() + 1);
    assert_eq!(roster.last().unwrap().text, REFERENCE);
    let texts: BTreeSet<_> = roster.iter().map(|card| card.text.clone()).collect();
    assert_eq!(texts.len(), roster.len());
    assert!(
        roster
            .iter()
            .all(|card| card.class == v3::CardClass::PlanNeed)
    );
}

#[test]
fn need_queries_are_short_and_per_need() {
    let part = "Update the README section on search so it documents the vector fallback setting";
    let proposals = propose(part, part);
    let kinds: Vec<_> = proposals.iter().map(|proposal| proposal.kind).collect();
    assert!(kinds.contains(&NeedKind::Documentation));
    assert!(kinds.contains(&NeedKind::Configuration));
    for proposal in &proposals {
        assert!(
            proposal.query.split_whitespace().count() <= 4,
            "{proposal:?}"
        );
    }
    let plain = propose("how does it work", "how does it work");
    assert_eq!(plain.len(), 1);
    assert_eq!(plain[0].kind, NeedKind::Overview);
}

#[test]
fn inherited_identifiers_come_last_and_are_marked() {
    let request = "Explain `restart_guard`. Then update the docs for it.";
    let part = "Then update the docs for it.";
    let proposals = propose(part, request);
    let inherited = proposals
        .iter()
        .position(|proposal| proposal.inherited)
        .unwrap();
    assert_eq!(proposals[inherited].query, "restart_guard");
    assert!(
        proposals[inherited..]
            .iter()
            .all(|proposal| proposal.inherited || proposal.kind == NeedKind::Overview)
    );
}
