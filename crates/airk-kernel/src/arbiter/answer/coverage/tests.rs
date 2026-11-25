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
fn covered_card_cannot_replace_structural_evidence() {
    for kind in [NeedKind::Definition, NeedKind::Callers, NeedKind::Contract] {
        assert!(!may_offer_covered(kind, v3::Coverage::Missing));
        assert!(!may_offer_covered(kind, v3::Coverage::Partial));
        assert!(may_offer_covered(kind, v3::Coverage::Covered));
    }
    for kind in [
        NeedKind::Tests,
        NeedKind::Documentation,
        NeedKind::Configuration,
        NeedKind::Implementation,
        NeedKind::Interfaces,
        NeedKind::Errors,
        NeedKind::Overview,
    ] {
        assert!(may_offer_covered(kind, v3::Coverage::Partial));
    }
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
fn excerpt_cuts_give_every_span_the_same_level_or_all_it_has() {
    let (long, mid, short) = ("x".repeat(100), "z".repeat(60), "y".repeat(10));
    let (long, mid, short) = (long.as_str(), mid.as_str(), short.as_str());
    // Everything fits: nothing is cut, however large the allowance.
    assert_eq!(excerpt_cuts(&[short, long], 110), [10, 100]);
    assert_eq!(excerpt_cuts(&[short, long], 7680), [10, 100]);
    // 90 bytes for two long spans and a short one: the short one takes its
    // 10 and the long ones share the other 80, wherever they stand.
    assert_eq!(excerpt_cuts(&[long, short, long], 90), [40, 10, 40]);
    assert_eq!(excerpt_cuts(&[short, long, long], 90), [10, 40, 40]);
    // Two short spans leave 70 to the one long span.
    assert_eq!(excerpt_cuts(&[short, short, long], 90), [10, 10, 70]);
    // 150 bytes: the short span takes 10 and the other three get 46 each;
    // the 60-byte span is cut like the 100-byte ones.
    assert_eq!(
        excerpt_cuts(&[long, mid, short, long], 150),
        [46, 46, 10, 46]
    );
    // 250 over three long spans is 83 each; the odd byte is not used.
    assert_eq!(excerpt_cuts(&[long, long, long], 250), [83, 83, 83]);
    // Fewer bytes than spans, or none: nothing is shown.
    assert_eq!(excerpt_cuts(&[long, long, long], 2), [0, 0, 0]);
    assert_eq!(excerpt_cuts(&[long, short], 0), [0, 0]);
    assert!(excerpt_cuts(&[], 7680).is_empty());
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

/// A row that shows `text`, with `hidden` bytes of its span before and
/// after it.
fn excerpt(
    path: &str,
    lines: (usize, usize),
    focus: Option<f32>,
    text: &str,
    hidden: (usize, usize),
) -> CardEvidence {
    CardEvidence {
        path: path.into(),
        start_line: lines.0,
        end_line: lines.1,
        focus,
        excerpt: text.into(),
        skipped_bytes: hidden.0,
        cut_bytes: hidden.1,
        truncated: false,
        same_as: None,
    }
}

/// A row that repeats the text the call's row at position `shown` shows.
fn repeated(path: &str, lines: (usize, usize), focus: Option<f32>, shown: usize) -> CardEvidence {
    CardEvidence {
        same_as: Some(shown),
        ..excerpt(path, lines, focus, "", (0, 0))
    }
}

/// A covered, a partly covered and a missing sub-need of one part, and what
/// one card call shows of each.
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
    let shown = present(&gather, &needs, 7680).context("short spans are shown whole")?;
    assert_eq!(
        shown,
        [
            Shown {
                evidence: vec![excerpt(
                    "src/guard.rs",
                    (1, 9),
                    None,
                    "fn restart_guard() {}",
                    (0, 0)
                )],
                unshown: 0,
                presented: vec![unit(&gather, body, 0, (0, 21))],
                omitted: vec![],
            },
            Shown {
                evidence: vec![excerpt(
                    "docs/notes.md",
                    (3, 7),
                    None,
                    "The guard has rules.",
                    (0, 0)
                )],
                unshown: 0,
                presented: vec![unit(&gather, notes, 0, (0, 20))],
                omitted: vec![],
            },
            Shown::default(),
        ]
    );
    // Spans shorter than the minimum are shown whole or the call is not
    // made: 41 bytes show both, 40 would cut one.
    assert_eq!(present(&gather, &needs, 41), Some(shown));
    assert_eq!(present(&gather, &needs, 40), None);
    // A call with nothing gathered is made without evidence.
    assert_eq!(
        present(&gather, &needs[2..], 0),
        Some(vec![Shown::default()])
    );
    Ok(())
}

/// Three spans of one sub-need: `a` (1,000 bytes), `b` (300) and `c`
/// (1,000), in the order the search returned them.
fn three_spans() -> Result<(Gathering, usize, Vec<Span>)> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let (a, b, c) = ("a".repeat(1000), "b".repeat(300), "c".repeat(1000));
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
    let found = spans(&gather, 1, 1, 1);
    Ok((gather, record, found))
}

#[test]
fn excerpts_follow_the_focus_rank_and_disclose_every_cut() -> Result<()> {
    let (gather, record, mut ranked) = three_spans()?;
    // Before Focus, the gathered order.
    assert_eq!(
        ranked
            .iter()
            .map(|span| span.hit.citation.path.as_str())
            .collect::<Vec<_>>(),
        ["src/a.rs", "src/b.rs", "src/c.rs"]
    );
    // Focus scored the first and the third hit and left the second unscored.
    ranked[0].focus = Some(0.2);
    ranked[2].focus = Some(0.9);
    rank(&mut ranked);
    let unit = |hit: usize, range: (usize, usize)| unit(&gather, record, hit, range);
    // 1,500 bytes: the 300-byte span is shown whole and the two long ones
    // get 600 each.
    let shown = present(&gather, &[&ranked[..]], 1500).context("room for every span")?;
    assert_eq!(
        shown,
        [Shown {
            evidence: vec![
                excerpt("src/c.rs", (5, 6), Some(0.9), &"c".repeat(600), (0, 400)),
                excerpt("src/a.rs", (1, 2), Some(0.2), &"a".repeat(600), (0, 400)),
                excerpt("src/b.rs", (3, 4), None, &"b".repeat(300), (0, 0)),
            ],
            unshown: 0,
            presented: vec![unit(2, (0, 600)), unit(0, (0, 600)), unit(1, (0, 300))],
            omitted: vec![unit(2, (600, 1000)), unit(0, (600, 1000))],
        }]
    );
    // The allowance is the call's: the same spans over two sub-needs of one
    // call are cut the same way.
    let split =
        present(&gather, &[&ranked[..1], &ranked[1..]], 1500).context("room for every span")?;
    assert_eq!(split[0].evidence, shown[0].evidence[..1]);
    assert_eq!(split[1].evidence, shown[0].evidence[1..]);
    assert_eq!(split[0].omitted, [unit(2, (600, 1000))]);
    assert_eq!(split[1].omitted, [unit(0, (600, 1000))]);
    Ok(())
}

#[test]
fn a_call_that_cannot_show_every_span_the_minimum_is_not_made() -> Result<()> {
    let (gather, _, ranked) = three_spans()?;
    // 768 bytes are 256 for each of the three spans: just enough.
    let least = present(&gather, &[&ranked[..]], 768).context("256 bytes a span")?;
    assert_eq!(
        least[0]
            .evidence
            .iter()
            .map(|row| (row.excerpt.len(), row.cut_bytes))
            .collect::<Vec<_>>(),
        [(256, 744), (256, 44), (256, 744)]
    );
    // One byte less leaves 255 a span: no call, whatever the split.
    assert_eq!(present(&gather, &[&ranked[..]], 767), None);
    assert_eq!(present(&gather, &[&ranked[..2], &ranked[2..]], 767), None);
    // Nothing above the frame: no call.
    assert_eq!(present(&gather, &[&ranked[..]], 0), None);
    // One span alone: 256 bytes are enough, 255 are not.
    assert!(present(&gather, &[&ranked[1..2]], 256).is_some());
    assert_eq!(present(&gather, &[&ranked[1..2]], 255), None);
    Ok(())
}

#[test]
fn a_window_holds_the_scored_text_and_uses_its_whole_size() {
    let text = format!("{}{}{}", "h".repeat(400), "m".repeat(300), "t".repeat(300));
    // The text fits: all of it, wherever Focus scored.
    assert_eq!(window(&text, 400, 1000), (0, 1000));
    assert_eq!(window(&text, 400, 7680), (0, 1000));
    // It does not fit: from the scored text on.
    assert_eq!(window(&text, 400, 300), (400, 700));
    assert_eq!(window(&text, 0, 300), (0, 300));
    // The scored text is near the end: the window ends with the text and
    // starts earlier, so no byte of the size is unused.
    assert_eq!(window(&text, 900, 300), (700, 1000));
    assert_eq!(window(&text, 999, 999), (1, 1000));
    // Two-byte characters: a window never splits one. From byte 100, 301
    // bytes end inside a character; anchored at the end, 301 bytes would
    // start inside one.
    let accents = "é".repeat(300);
    assert_eq!(window(&accents, 100, 301), (100, 400));
    assert_eq!(window(&accents, 500, 301), (300, 600));
    assert_eq!(window(&accents, 0, 0), (0, 0));
}

/// The reviewer's case: one 3,072-byte read whose identifier first appears
/// at byte 1,900. The whole span fits the call's allowance, so the model is
/// shown all of it, as the deterministic rule read it, and nothing is
/// logged as omitted.
#[test]
fn a_span_that_fits_is_shown_whole_wherever_focus_scored_it() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let text = format!(
        "{}fn restart_guard() {{}}\n{}",
        "// doc line\n".repeat(158),
        "x".repeat(1154)
    );
    assert_eq!(
        (text.len(), text.find("fn restart_guard")),
        (3072, Some(1896))
    );
    let record = retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"need"}),
        Some(1),
        FollowUp::Keyword,
        reply(&scope, vec![hit("src/guard.rs", 1, 170, &text)]),
    )?;
    let mut found = spans(&gather, 1, 1, 1);
    found[0].focus = Some(0.8);
    found[0].from = 1896;
    let shown = present(&gather, &[&found[..]], 7680).context("the span fits")?;
    assert_eq!(
        shown,
        [Shown {
            evidence: vec![excerpt("src/guard.rs", (1, 170), Some(0.8), &text, (0, 0))],
            unshown: 0,
            presented: vec![unit(&gather, record, 0, (0, 3072))],
            omitted: vec![],
        }]
    );
    // With room for 2,000 bytes the window still holds the scored line and
    // ends with the text: bytes 1,072 to 3,072.
    let shown = present(&gather, &[&found[..]], 2000).context("a window fits")?;
    assert_eq!(
        shown[0].evidence,
        [excerpt(
            "src/guard.rs",
            (1, 170),
            Some(0.8),
            &text[1072..],
            (1072, 0)
        )]
    );
    assert_eq!(shown[0].presented, [unit(&gather, record, 0, (1072, 3072))]);
    assert_eq!(shown[0].omitted, [unit(&gather, record, 0, (0, 1072))]);
    Ok(())
}

/// Character boundaries can leave a window just short of the least excerpt.
/// It grows by whole characters when the call's allowance has the bytes,
/// and the call is refused only when it has not.
#[test]
fn a_window_short_by_a_character_grows_when_the_allowance_has_the_bytes() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    // 200 three-byte characters and 300 two-byte ones: 600 bytes each.
    let (wide, accents) = ("日".repeat(200), "é".repeat(300));
    retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"need"}),
        Some(1),
        FollowUp::Keyword,
        reply(
            &scope,
            vec![
                hit("docs/wide.md", 1, 9, &wide),
                hit("docs/accents.md", 1, 9, &accents),
            ],
        ),
    )?;
    let found = spans(&gather, 1, 1, 1);
    let lengths = |allowance: usize| {
        present(&gather, &[&found[..]], allowance).map(|shown| {
            shown[0]
                .evidence
                .iter()
                .map(|row| row.excerpt.len())
                .collect::<Vec<_>>()
        })
    };
    // 515 bytes are 257 a span. Whole characters give 255 and 256, and the
    // four bytes left over pay for one more three-byte character.
    assert_eq!(lengths(515), Some(vec![258, 256]));
    // 513 bytes leave two bytes over: not enough for that character.
    assert_eq!(lengths(513), None);
    Ok(())
}

#[test]
fn an_excerpt_starts_where_focus_scored_and_marks_a_snippet() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let text = format!("{}{}{}", "h".repeat(400), "m".repeat(300), "t".repeat(300));
    let record = retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"need"}),
        Some(1),
        FollowUp::Keyword,
        reply(&scope, vec![hit("src/long.rs", 1, 90, &text)]),
    )?;
    let mut found = spans(&gather, 1, 1, 1);
    assert_eq!((found[0].from, found[0].hit.truncated), (0, false));
    // Focus scored the text from byte 400 on, and the read was a snippet.
    found[0].focus = Some(0.7);
    found[0].from = 400;
    found[0].hit.truncated = true;
    let shown = present(&gather, &[&found[..]], 300).context("one span fits")?;
    let mut row = excerpt(
        "src/long.rs",
        (1, 90),
        Some(0.7),
        &"m".repeat(300),
        (400, 300),
    );
    row.truncated = true;
    assert_eq!(
        shown,
        [Shown {
            evidence: vec![row],
            unshown: 0,
            presented: vec![unit(&gather, record, 0, (400, 700))],
            // The head Focus skipped and the tail the allowance cut.
            omitted: vec![
                unit(&gather, record, 0, (0, 400)),
                unit(&gather, record, 0, (700, 1000))
            ],
        }]
    );
    // Scored near its end, the span is shown from further back, so the 300
    // bytes are all used and the scored text is still inside.
    found[0].from = 900;
    let shown = present(&gather, &[&found[..]], 300).context("one span fits")?;
    assert_eq!(
        (
            shown[0].evidence[0].excerpt.as_str(),
            shown[0].evidence[0].skipped_bytes,
            shown[0].evidence[0].cut_bytes
        ),
        ("t".repeat(300).as_str(), 700, 0)
    );
    assert_eq!(shown[0].presented, [unit(&gather, record, 0, (700, 1000))]);
    assert_eq!(shown[0].omitted, [unit(&gather, record, 0, (0, 700))]);
    Ok(())
}

#[test]
fn a_multi_byte_span_is_cut_on_a_character_and_logged_in_bytes() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    // 300 two-byte characters: 600 bytes.
    let text = "é".repeat(300);
    let record = retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"need"}),
        Some(1),
        FollowUp::Keyword,
        reply(&scope, vec![hit("docs/accents.md", 1, 9, &text)]),
    )?;
    let mut found = spans(&gather, 1, 1, 1);
    // 301 bytes end inside a character: the excerpt stops one byte earlier.
    let shown = present(&gather, &[&found[..]], 301).context("one span fits")?;
    let half = "é".repeat(150);
    assert_eq!(
        shown[0].evidence,
        [excerpt("docs/accents.md", (1, 9), None, &half, (0, 300))]
    );
    assert_eq!(shown[0].presented, [unit(&gather, record, 0, (0, 300))]);
    assert_eq!(shown[0].omitted, [unit(&gather, record, 0, (300, 600))]);
    // From byte 100 on, the same allowance shows bytes 100 to 400.
    found[0].from = 100;
    let shown = present(&gather, &[&found[..]], 301).context("one span fits")?;
    assert_eq!(
        shown[0].evidence,
        [excerpt("docs/accents.md", (1, 9), None, &half, (100, 200))]
    );
    assert_eq!(shown[0].presented, [unit(&gather, record, 0, (100, 400))]);
    Ok(())
}

#[test]
fn a_text_is_shown_once_in_a_call_and_its_repeats_name_that_row() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let text = "g".repeat(1000);
    // Sub-need 1 found the text at two places; sub-need 2 found one of them
    // again through its own search.
    let first = retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"guard"}),
        Some(1),
        FollowUp::Keyword,
        reply(
            &scope,
            vec![
                hit("src/guard.rs", 1, 40, &text),
                hit("vendor/guard.rs", 1, 40, &text),
            ],
        ),
    )?;
    let second = retain(
        &mut gather,
        ("search", v3::LoopRead::Search),
        json!({"query":"guard callers"}),
        Some(2),
        FollowUp::Keyword,
        reply(&scope, vec![hit("src/guard.rs", 1, 40, &text)]),
    )?;
    let found = [spans(&gather, 1, 1, 1), spans(&gather, 1, 2, 1)];
    assert_eq!((found[0].len(), found[1].len()), (2, 1));
    let needs: Vec<&[Span]> = found.iter().map(Vec::as_slice).collect();
    // 300 bytes: were each copy to take a share, each would get 100, which
    // is below the minimum. The one shown copy takes all 300.
    let shown = present(&gather, &needs, 300).context("one text to show")?;
    // The call's first row shows the text.
    let place = 0;
    assert_eq!(
        shown,
        [
            Shown {
                evidence: vec![
                    excerpt("src/guard.rs", (1, 40), None, &"g".repeat(300), (0, 700)),
                    repeated("vendor/guard.rs", (1, 40), None, place),
                ],
                unshown: 0,
                // Each span's own record is logged with the range shown.
                presented: vec![
                    unit(&gather, first, 0, (0, 300)),
                    unit(&gather, first, 1, (0, 300))
                ],
                omitted: vec![
                    unit(&gather, first, 0, (300, 1000)),
                    unit(&gather, first, 1, (300, 1000))
                ],
            },
            Shown {
                evidence: vec![repeated("src/guard.rs", (1, 40), None, place)],
                unshown: 0,
                presented: vec![unit(&gather, second, 0, (0, 300))],
                omitted: vec![unit(&gather, second, 0, (300, 1000))],
            },
        ]
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
