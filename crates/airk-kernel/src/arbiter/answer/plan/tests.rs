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

#[test]
fn an_apostrophe_does_not_hide_a_quoted_term() {
    assert_eq!(quoted("don't touch 'retry_refused'"), ["retry_refused"]);
}

#[test]
fn explicit_addresses_preserve_the_task_path_and_lines_in_the_plan() {
    let part = "Read (old/worker.rs:4-9) and compare `new/worker.rs:27`.";
    let proposals = propose(part, part);
    assert_eq!(
        summary(&proposals)[..4],
        [
            (NeedKind::Implementation, "old/worker.rs:4-9".into()),
            (NeedKind::Contract, "old contract rules".into()),
            (NeedKind::Implementation, "new/worker.rs:27".into()),
            (NeedKind::Contract, "new contract rules".into()),
        ]
    );
    let chosen = needs(&proposals, &[0, 2]);
    assert_eq!(chosen[0].subject.as_deref(), Some("old/worker.rs:4-9"));
    assert_eq!(chosen[1].subject.as_deref(), Some("new/worker.rs:27"));
    assert_eq!(
        known_address(chosen[0].subject.as_deref().unwrap()),
        Some(KnownAddress {
            path: "old/worker.rs",
            start: 4,
            end: 9,
        })
    );
}

#[test]
fn long_addresses_with_the_same_display_prefix_remain_distinct() {
    let folder = "deep/".repeat(50);
    let first = format!("{folder}worker.rs:30");
    let second = format!("{folder}worker.rs:40");
    let part = format!("Read {first} and {second}");
    let proposals = propose(&part, &part);
    let addresses: Vec<_> = proposals
        .iter()
        .filter(|proposal| proposal.kind == NeedKind::Implementation)
        .filter_map(|proposal| proposal.subject.as_deref().and_then(known_address))
        .map(|address| address.start)
        .collect();
    assert_eq!(addresses, [30, 40]);
}

#[test]
fn invalid_or_unsafe_addresses_are_never_path_hints() {
    for token in [
        "/src/worker.rs:4",
        "C:/src/worker.rs:4",
        r"C:\src\worker.rs:4",
        r"src\worker.rs:4",
        "//host/src/worker.rs:4",
        "https://fixture.invalid/src/worker.rs:4",
        "file:///src/worker.rs:4",
        "../src/worker.rs:4",
        "src/../worker.rs:4",
        "./src/worker.rs:4",
        "src//worker.rs:4",
        "src/worker.rs/:4",
        "src/worker.rs:0",
        "src/worker.rs:4-0",
        "src/worker.rs:9-4",
        "src/worker.rs:9-",
        "src/worker.rs:+9",
        "src/worker.rs:9-10-11",
        "src/worker.rs:4294967296",
        "src/worker.rs:5-18446744073709551616",
        "src/worker.rs:９",
    ] {
        assert_eq!(known_address(token), None, "{token}");
        assert!(path_hints(token).is_empty(), "{token}");
    }
}

#[test]
fn plain_paths_keep_the_original_path_hints() {
    assert_eq!(
        path_hints("Look at src/worker.rs, then README.md."),
        [
            ("src/worker.rs", "src/worker.rs"),
            ("README.md", "README.md")
        ]
    );
    assert_eq!(known_address("src/worker.rs"), None);
}
