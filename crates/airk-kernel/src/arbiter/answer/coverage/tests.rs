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
fn excerpt_cuts_share_the_allowance_equally_then_in_call_order() {
    let (long, short) = ("x".repeat(100), "y".repeat(10));
    let (long, short) = (long.as_str(), short.as_str());
    // Everything fits: nothing is cut, however large the allowance.
    assert_eq!(excerpt_cuts(&[short, long], 110), [10, 100]);
    assert_eq!(excerpt_cuts(&[short, long], 6144), [10, 100]);
    // 90 bytes over three spans is 30 each. The short span needs only 10,
    // so 20 are left, and they go to the first span that was cut.
    assert_eq!(excerpt_cuts(&[long, short, long], 90), [50, 10, 30]);
    assert_eq!(excerpt_cuts(&[short, long, long], 90), [10, 50, 30]);
    // 40 left over two short spans: the one long span takes them all.
    assert_eq!(excerpt_cuts(&[short, short, long], 90), [10, 10, 70]);
    // 250 over three long spans is 83 each and 1 over, to the first.
    assert_eq!(excerpt_cuts(&[long, long, long], 250), [84, 83, 83]);
    // Fewer bytes than spans: no share, the bytes go to the first span.
    assert_eq!(excerpt_cuts(&[long, long, long], 2), [2, 0, 0]);
    assert_eq!(excerpt_cuts(&[long, short], 0), [0, 0]);
    assert!(excerpt_cuts(&[], 6144).is_empty());
}

#[test]
fn excerpt_cuts_end_on_character_boundaries() {
    // "é" is the two bytes 1..3 of "héllo".
    assert_eq!(excerpt_cuts(&["héllo"], 2), [1]);
    assert_eq!(excerpt_cuts(&["héllo"], 3), [3]);
    // The emoji is the four bytes 1..5 of "a😀b".
    assert_eq!(excerpt_cuts(&["a😀b"], 4), [1]);
    assert_eq!(excerpt_cuts(&["a😀b"], 5), [5]);
    // Not even one three-byte character fits in two bytes.
    assert_eq!(excerpt_cuts(&["日本語"], 2), [0]);
}

fn unit(gather: &Gathering, record: usize, hit: usize, range: (usize, usize)) -> EvidenceRef {
    EvidenceRef {
        record: gather.records[record].id,
        kind: SourceUnitKind::Hit,
        result_pointer: format!("/hits/{hit}"),
        utf8_start: range.0,
        utf8_end: range.1,
    }
}

fn excerpt(
    path: &str,
    lines: (usize, usize),
    focus: Option<f32>,
    text: &str,
    cut_bytes: usize,
) -> CardEvidence {
    CardEvidence {
        path: path.into(),
        start_line: lines.0,
        end_line: lines.1,
        focus,
        excerpt: text.into(),
        cut_bytes,
    }
}

/// A covered, a partly covered and a missing sub-need of one part, and what
/// one Coverage card call shows of each.
#[test]
fn a_covered_a_partial_and_a_missing_sub_need_show_their_own_spans() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let definition = Need {
        id: 1,
        kind: NeedKind::Definition,
        subject: Some("restart_guard".into()),
        query: "restart_guard".into(),
    };
    let contract = Need {
        id: 2,
        kind: NeedKind::Contract,
        subject: None,
        query: "guard contract rules".into(),
    };
    let tests = Need {
        id: 3,
        kind: NeedKind::Tests,
        subject: None,
        query: "guard test".into(),
    };
    // Sub-need 1: the definition's location and its excerpt.
    retain(
        &mut gather,
        ("symbols", v3::LoopRead::Symbols),
        json!({"name":"restart_guard"}),
        Some(1),
        FollowUp::Definition,
        located(&scope, "src/guard.rs"),
    )?;
    let body = retain(
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
    // Sub-need 2: a search hit, and no governing document.
    let notes = retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"guard contract rules"}),
        Some(2),
        FollowUp::Keyword,
        reply(
            &scope,
            vec![hit("docs/notes.md", 3, 7, "The guard has rules.")],
        ),
    )?;
    // Sub-need 3: nothing was read.
    let found = [
        spans(&gather, 1, 1, 1),
        spans(&gather, 1, 2, 1),
        spans(&gather, 1, 3, 1),
    ];
    assert_eq!(
        [
            verdict(&gather, 1, &definition, &found[0], 2),
            verdict(&gather, 1, &contract, &found[1], 1),
            verdict(&gather, 1, &tests, &found[2], 0),
        ],
        [
            v3::Coverage::Covered,
            v3::Coverage::Partial,
            v3::Coverage::Missing
        ]
    );
    let needs: Vec<&[Span]> = found.iter().map(Vec::as_slice).collect();
    let shown = present(&gather, &needs, 6144);
    assert_eq!(
        shown,
        [
            Shown {
                evidence: vec![excerpt(
                    "src/guard.rs",
                    (1, 9),
                    None,
                    "fn restart_guard() {}",
                    0
                )],
                presented: vec![unit(&gather, body, 0, (0, 21))],
                omitted: vec![],
            },
            Shown {
                evidence: vec![excerpt(
                    "docs/notes.md",
                    (3, 7),
                    None,
                    "The guard has rules.",
                    0
                )],
                presented: vec![unit(&gather, notes, 0, (0, 20))],
                omitted: vec![],
            },
            Shown::default(),
        ]
    );
    Ok(())
}

#[test]
fn excerpts_follow_the_focus_rank_and_disclose_every_cut() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let (a, b, c) = ("a".repeat(40), "b".repeat(10), "c".repeat(40));
    let record = retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"need"}),
        Some(1),
        FollowUp::Keyword,
        reply(
            &scope,
            vec![
                hit("src/a.rs", 1, 2, &a),
                hit("src/b.rs", 3, 4, &b),
                hit("src/c.rs", 5, 6, &c),
            ],
        ),
    )?;
    let mut ranked = spans(&gather, 1, 1, 1);
    // Focus scored the first and the third hit and left the second unscored.
    ranked[0].focus = Some(0.2);
    ranked[2].focus = Some(0.9);
    rank(&mut ranked);
    let unit = |hit: usize, range: (usize, usize)| unit(&gather, record, hit, range);
    // 60 bytes over three spans is 20 each. The 10-byte span leaves 10,
    // which go to the first span in rank order.
    let shown = present(&gather, &[&ranked[..]], 60);
    assert_eq!(
        shown,
        [Shown {
            evidence: vec![
                excerpt("src/c.rs", (5, 6), Some(0.9), &"c".repeat(30), 10),
                excerpt("src/a.rs", (1, 2), Some(0.2), &"a".repeat(20), 20),
                excerpt("src/b.rs", (3, 4), None, &"b".repeat(10), 0),
            ],
            presented: vec![unit(2, (0, 30)), unit(0, (0, 20)), unit(1, (0, 10))],
            omitted: vec![unit(2, (30, 40)), unit(0, (20, 40))],
        }]
    );
    // The allowance is the call's: the same spans over two sub-needs of one
    // call are cut the same way.
    let split = present(&gather, &[&ranked[..1], &ranked[1..]], 60);
    assert_eq!(split[0].evidence, shown[0].evidence[..1]);
    assert_eq!(split[1].evidence, shown[0].evidence[1..]);
    assert_eq!(split[0].omitted, [unit(2, (30, 40))]);
    assert_eq!(split[1].omitted, [unit(0, (20, 40))]);
    // Two bytes for three spans: only the top span shows anything, and the
    // spans not shown at all are logged whole as omitted.
    assert_eq!(
        present(&gather, &[&ranked[..]], 2),
        [Shown {
            evidence: vec![excerpt("src/c.rs", (5, 6), Some(0.9), "cc", 38)],
            presented: vec![unit(2, (0, 2))],
            omitted: vec![unit(2, (2, 40)), unit(0, (0, 40)), unit(1, (0, 10))],
        }]
    );
    // No allowance: no excerpt, and every span is logged as omitted.
    assert_eq!(
        present(&gather, &[&ranked[..]], 0),
        [Shown {
            evidence: vec![],
            presented: vec![],
            omitted: vec![unit(2, (0, 40)), unit(0, (0, 40)), unit(1, (0, 10))],
        }]
    );
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
