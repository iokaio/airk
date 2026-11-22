// SPDX-License-Identifier: Apache-2.0
//! Spans, deterministic verdicts and coverage rosters.
use super::super::gather::{Cause, Gathering, Step};
use super::*;
use crate::arbiter::answer::tests::{hit, reply, scope};

fn need(kind: NeedKind, subject: Option<&str>, query: &str) -> Need {
    Need {
        id: 1,
        kind,
        subject: subject.map(str::to_owned),
        query: query.into(),
    }
}

fn retain(
    gather: &mut Gathering,
    (tool, operation): (&'static str, v3::LoopRead),
    arguments: Value,
    need: Option<u8>,
    kind: FollowUp,
    body: Value,
) -> Result<usize> {
    let dispatch = gather.dispatch(
        (tool, operation),
        arguments,
        (1, need, 1),
        if need.is_some() {
            Cause::Structural
        } else {
            Cause::PartSearch
        },
        kind,
        Step::Leaf,
        "subject",
    )?;
    gather.offer(dispatch);
    let taken = gather.take(1, 1);
    match gather.observe(taken[0].seq, Ok(body))? {
        super::super::gather::Observed::Retained(index) => Ok(index),
        other => anyhow::bail!("unexpected {other:?}"),
    }
}

fn located(scope: &ScopeBinding, path: &str) -> Value {
    json!({"publication":scope.publication,"facts":[{"text":"restart_guard",
        "citation":{"path":path,"start_line":1,"end_line":9,"content_hash":"abc"}}]})
}

#[test]
fn structural_verdict_rules_per_kind() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let definition = need(NeedKind::Definition, Some("restart_guard"), "restart_guard");
    retain(
        &mut gather,
        ("symbols", v3::LoopRead::Symbols),
        json!({"name":"restart_guard"}),
        Some(1),
        FollowUp::Definition,
        located(&scope, "src/guard.rs"),
    )?;
    assert_eq!(
        verdict(&gather, 1, &definition, &[], 1),
        v3::Coverage::Partial,
        "a location list alone"
    );
    retain(
        &mut gather,
        ("read_file", v3::LoopRead::ReadFile),
        json!({"path":"src/guard.rs","start_line":1,"end_line":9}),
        Some(1),
        FollowUp::Excerpt,
        reply(
            &scope,
            vec![hit("src/guard.rs", 1, 9, "fn restart_guard() {}")],
        ),
    )?;
    let found = spans(&gather, 1, 1, 1);
    assert_eq!(found.len(), 1);
    assert_eq!(
        verdict(&gather, 1, &definition, &found, 2),
        v3::Coverage::Covered
    );

    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let contract = need(NeedKind::Contract, None, "guard contract rules");
    retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"guard contract rules"}),
        Some(1),
        FollowUp::Keyword,
        reply(&scope, vec![hit("src/guard.rs", 1, 9, "x")]),
    )?;
    let found = spans(&gather, 1, 1, 1);
    assert_eq!(
        verdict(&gather, 1, &contract, &found, 1),
        v3::Coverage::Partial
    );
    retain(
        &mut gather,
        ("read_file", v3::LoopRead::ReadFile),
        json!({"path":"src/CONTRACT.md","start_line":1,"end_line":40}),
        Some(1),
        FollowUp::Governing,
        reply(&scope, vec![hit("src/CONTRACT.md", 1, 40, "Rules")]),
    )?;
    let found = spans(&gather, 1, 1, 1);
    assert_eq!(
        verdict(&gather, 1, &contract, &found, 2),
        v3::Coverage::Covered
    );
    let tests = need(NeedKind::Tests, Some("restart_guard"), "restart_guard test");
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"restart_guard test"}),
        Some(1),
        FollowUp::Keyword,
        reply(
            &scope,
            vec![hit("tests/guard.rs", 1, 9, "restart_guard();")],
        ),
    )?;
    let found = spans(&gather, 1, 1, 1);
    assert_eq!(
        verdict(&gather, 1, &tests, &found, 1),
        v3::Coverage::Covered
    );
    assert_eq!(
        verdict(&gather, 1, &need(NeedKind::Tests, None, "x"), &[], 0),
        v3::Coverage::Missing
    );
    Ok(())
}

#[test]
fn focus_absent_uses_canonical_order_and_never_lexically_overclaims() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let overview = need(NeedKind::Implementation, None, "restart guard replay");
    retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"restart guard replay"}),
        Some(1),
        FollowUp::Keyword,
        reply(
            &scope,
            vec![
                hit("src/a.rs", 1, 3, "the restart guard"),
                hit("src/b.rs", 1, 3, "restart guard and replay rules"),
            ],
        ),
    )?;
    let found = spans(&gather, 1, 1, 1);
    assert_eq!(
        found[0].hit.citation.path, "src/a.rs",
        "canonical order without Focus"
    );
    assert_eq!(
        verdict(&gather, 1, &overview, &found, 1),
        v3::Coverage::Partial
    );
    let mut scored = found.clone();
    scored[1].focus = Some(0.9);
    scored[0].focus = Some(0.05);
    rank(&mut scored);
    assert_eq!(scored[0].hit.citation.path, "src/b.rs");
    assert_eq!(
        verdict(&gather, 1, &overview, &scored, 1),
        v3::Coverage::Partial,
        "a high Focus score alone needs its whole chunk read"
    );
    assert_eq!(from_card(false, &scored[1..], 1), v3::Coverage::Missing);
    assert_eq!(from_card(true, &scored, 0), v3::Coverage::Missing);
    Ok(())
}

#[test]
fn spans_prefer_the_sub_needs_own_reads_then_distinct_files() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"part"}),
        None,
        FollowUp::Initial,
        reply(&scope, vec![hit("src/part.rs", 1, 2, "p")]),
    )?;
    retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"need"}),
        Some(1),
        FollowUp::Keyword,
        reply(
            &scope,
            vec![
                hit("src/a.rs", 1, 2, "a1"),
                hit("src/a.rs", 5, 6, "a2"),
                hit("src/b.rs", 1, 2, "b1"),
            ],
        ),
    )?;
    let found = spans(&gather, 1, 1, 1);
    let paths: Vec<_> = found
        .iter()
        .map(|span| {
            (
                span.hit.citation.path.as_str(),
                span.hit.citation.start_line,
            )
        })
        .collect();
    assert_eq!(
        paths,
        [
            ("src/a.rs", 1),
            ("src/b.rs", 1),
            ("src/a.rs", 5),
            ("src/part.rs", 1)
        ]
    );
    assert_eq!(found[0].pointer, "/hits/0");
    Ok(())
}

#[test]
fn coverage_roster_cites_top_spans_and_offers_alternatives() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"need"}),
        Some(1),
        FollowUp::Keyword,
        reply(
            &scope,
            vec![hit("src/a.rs", 1, 2, "a1"), hit("src/b.rs", 3, 4, "b1")],
        ),
    )?;
    let found = spans(&gather, 1, 1, 1);
    let roster = cards(
        2,
        &found,
        &[
            Alternative {
                class: v3::CardClass::ReadMoreHere,
                text: "s2 read more: src/a.rs:1-40".into(),
            },
            Alternative {
                class: v3::CardClass::SearchForTerm,
                text: "s2 search: restart_guard".into(),
            },
        ],
    );
    assert_eq!(roster[0].text, "s2 covered: src/a.rs:1-2, src/b.rs:3-4");
    assert_eq!(roster.len(), 3);
    assert_eq!(roster[2].class, v3::CardClass::SearchForTerm);
    let needs = [
        need(NeedKind::Overview, None, "one"),
        need(NeedKind::Overview, None, "one"),
    ];
    let ready = [
        (1, &needs[0], 1, found.as_slice()),
        (2, &needs[1], 1, found.as_slice()),
    ];
    let (questions, owners, batch) = batch(&ready, 32);
    assert_eq!(
        questions,
        ["one"],
        "equal sub-need queries share a question"
    );
    assert_eq!(owners, [(0, 0), (0, 1), (1, 0), (1, 1)]);
    assert_eq!(
        batch.iter().map(|span| span.part).collect::<Vec<_>>(),
        [1, 1, 2, 2]
    );
    Ok(())
}
