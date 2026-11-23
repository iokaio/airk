// SPDX-License-Identifier: Apache-2.0
//! Per-sub-need recipes over synthetic canonical replies.
use super::*;
use crate::arbiter::answer::tests::{hit, reply, scope};
use airk_core::DocumentLocation;

fn need(kind: NeedKind, subject: Option<&str>, query: &str) -> Need {
    Need {
        id: 1,
        kind,
        subject: subject.map(str::to_owned),
        query: query.into(),
    }
}

/// Offer, take and observe one read; returns its record index.
fn run(gather: &mut Gathering, dispatch: Dispatch, body: Value) -> Result<usize> {
    gather.offer(dispatch.clone());
    let taken = gather.take(dispatch.part, 1);
    assert_eq!(taken.len(), 1);
    match gather.observe(taken[0].seq, Ok(body))? {
        super::super::gather::Observed::Retained(index) => Ok(index),
        other => anyhow::bail!("unexpected {other:?}"),
    }
}

fn tools(reads: &[Dispatch]) -> Vec<&'static str> {
    reads.iter().map(|read| read.tool).collect()
}

fn located(scope: &ScopeBinding, rows: &[(&str, usize, usize)]) -> Value {
    let facts: Vec<Value> = rows
        .iter()
        .map(|(path, start, end)| {
            json!({"text":"restart_guard","fact":{"kind":"function"},
                "citation":{"path":path,"start_line":start,"end_line":end,"content_hash":"abc"}})
        })
        .collect();
    json!({"publication":scope.publication,"facts":facts})
}

#[test]
fn definition_recipe_searches_finds_symbols_excerpts_then_outlines() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let need = need(NeedKind::Definition, Some("restart_guard"), "restart_guard");
    let first = initial(&mut gather, 1, &need)?;
    assert_eq!(tools(&first), ["search", "symbols"]);
    assert_eq!(first[0].arguments["query"], "restart_guard");
    assert_eq!(first[1].step, Step::Definitions);
    let index = run(
        &mut gather,
        first[1].clone(),
        located(&scope, &[("crates/k/src/guard.rs", 10, 40)]),
    )?;
    let record = gather.records[index].clone();
    let next = react(&mut gather, 1, &need, Step::Definitions, &record)?;
    assert_eq!(tools(&next.reads), ["read_file"]);
    assert_eq!(next.reads[0].arguments["known_hash"], "abc");
    assert_eq!(next.reads[0].arguments["end_line"], 40);
    let excerpt = run(
        &mut gather,
        next.reads[0].clone(),
        reply(
            &scope,
            vec![hit(
                "crates/k/src/guard.rs",
                10,
                40,
                "fn restart_guard() {}",
            )],
        ),
    )?;
    let record = gather.records[excerpt].clone();
    let last = react(&mut gather, 1, &need, Step::Excerpt, &record)?;
    assert_eq!(tools(&last.reads), ["outline"]);
    assert_eq!(last.reads[0].arguments["path"], "crates/k/src/guard.rs");
    Ok(())
}

#[test]
fn callers_recipe_excerpts_two_non_test_callers() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let need = need(NeedKind::Callers, Some("restart_guard"), "restart_guard");
    let first = initial(&mut gather, 1, &need)?;
    assert_eq!(tools(&first), ["search", "references"]);
    let body = located(
        &scope,
        &[
            ("src/a.rs", 1, 3),
            ("tests/guard.rs", 4, 6),
            ("src/b.rs", 7, 9),
            ("src/c.rs", 10, 12),
        ],
    );
    let index = run(&mut gather, first[1].clone(), body)?;
    let record = gather.records[index].clone();
    let next = react(&mut gather, 1, &need, Step::References, &record)?;
    let paths: Vec<_> = next
        .reads
        .iter()
        .map(|read| read.arguments["path"].clone())
        .collect();
    assert_eq!(paths, [json!("src/a.rs"), json!("src/b.rs")]);
    assert!(next.reads.iter().all(|read| read.kind == FollowUp::Callers));
    Ok(())
}

#[test]
fn tests_recipe_excerpts_test_references_or_searches_for_tests() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let need = need(NeedKind::Tests, Some("restart_guard"), "restart_guard test");
    let first = initial(&mut gather, 1, &need)?;
    assert_eq!(tools(&first), ["search", "references"]);
    let index = run(
        &mut gather,
        first[1].clone(),
        located(&scope, &[("src/a.rs", 1, 3), ("tests/guard.rs", 4, 6)]),
    )?;
    let record = gather.records[index].clone();
    let next = react(&mut gather, 1, &need, Step::References, &record)?;
    assert_eq!(next.reads.len(), 1);
    assert_eq!(next.reads[0].arguments["path"], "tests/guard.rs");
    let index = run(
        &mut gather,
        next.reads[0].clone(),
        located(&scope, &[("src/only.rs", 1, 2)]),
    )?;
    let record = gather.records[index].clone();
    let fallback = react(&mut gather, 1, &need, Step::References, &record)?;
    assert_eq!(tools(&fallback.reads), ["search"]);
    assert_eq!(fallback.reads[0].arguments["query"], "restart_guard test");
    Ok(())
}

#[test]
fn contract_and_documentation_searches_probe_governing_documents_only() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let contract = need(NeedKind::Contract, None, "restart guard contract rules");
    let first = initial(&mut gather, 1, &contract)?;
    assert_eq!(tools(&first), ["search"]);
    let index = run(
        &mut gather,
        first[0].clone(),
        reply(
            &scope,
            vec![hit("crates/airk-kernel/src/arbiter/joint.rs", 1, 9, "x")],
        ),
    )?;
    let record = gather.records[index].clone();
    let next = react(&mut gather, 1, &contract, Step::Search, &record)?;
    assert!(
        next.reads.is_empty(),
        "governing documents wait for file_info"
    );
    let probes: Vec<_> = next
        .probes
        .iter()
        .map(|probe| probe.path.as_str())
        .collect();
    assert_eq!(
        probes,
        [
            "crates/airk-kernel/src/arbiter/joint/CONTRACT.md",
            "crates/airk-kernel/src/arbiter/joint/README.md",
            "crates/airk-kernel/src/arbiter/CONTRACT.md",
            "crates/airk-kernel/src/arbiter/README.md"
        ]
    );
    let read = governing_read(&mut gather, &next.probes[2], 300)?;
    assert_eq!((read.tool, read.kind), ("read_file", FollowUp::Governing));
    assert_eq!(read.arguments["end_line"], 120);
    let docs = need(NeedKind::Documentation, None, "search documentation");
    let first = initial(&mut gather, 1, &docs)?;
    let index = run(
        &mut gather,
        first[0].clone(),
        reply(&scope, vec![hit("docs/search.md", 3, 9, "Search")]),
    )?;
    let record = gather.records[index].clone();
    let next = react(&mut gather, 1, &docs, Step::Search, &record)?;
    assert_eq!(tools(&next.reads), ["context"]);
    Ok(())
}

#[test]
fn implementation_configuration_interfaces_and_errors_plan_their_structural_reads() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let body = reply(
        &scope,
        vec![
            hit("src/a.rs", 1, 3, "pub fn alpha_beta() {}"),
            hit("src/a.rs", 9, 12, "fn more() {}"),
            hit("src/b.rs", 1, 3, "x"),
        ],
    );
    for (kind, step, expected) in [
        (
            NeedKind::Implementation,
            Step::Chunks,
            vec!["context", "outline", "context"],
        ),
        (
            NeedKind::Configuration,
            Step::Chunks,
            vec!["context", "outline", "context"],
        ),
        (
            NeedKind::Overview,
            Step::Chunks,
            vec!["context", "outline", "context"],
        ),
        (
            NeedKind::Interfaces,
            Step::Imports,
            vec!["imports", "symbols"],
        ),
        (NeedKind::Errors, Step::Literal, vec!["context"]),
    ] {
        let need = need(kind, None, &format!("{} query", kind.name()));
        let first = initial(&mut gather, 1, &need)?;
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].step, step);
        assert_eq!(first[0].arguments["full_chunks"], kind == NeedKind::Errors);
        let index = run(&mut gather, first[0].clone(), body.clone())?;
        let record = gather.records[index].clone();
        let next = react(&mut gather, 1, &need, step, &record)?;
        assert_eq!(tools(&next.reads), expected, "{kind:?}");
    }
    Ok(())
}

/// A search hit at a native document location (page, block, slide).
fn document_hit(
    path: &str,
    (start, end): (usize, usize),
    location: DocumentLocation,
    truncated: bool,
) -> Value {
    json!({"citation":{"path":path,"content_hash":"abc","start_line":start,"end_line":end,
        "location":location},"text":"document text","truncated":truncated})
}

/// Every planned `context` read of `path` must stay inside `start..=end`:
/// the context tool reads a document one location at a time.
fn confined(reads: &[Dispatch], path: &str, (start, end): (usize, usize)) {
    for read in reads
        .iter()
        .filter(|read| read.tool == "context" && read.arguments["path"] == path)
    {
        let line = read.arguments["line"].as_u64().unwrap() as usize;
        let after = read.arguments["after"].as_u64().unwrap() as usize;
        assert_eq!(read.arguments["before"], 0, "{path}");
        assert!(
            line >= start && line + after <= end,
            "{path}: {line}+{after}"
        );
    }
}

#[test]
fn document_hits_get_no_line_window_and_code_hits_keep_theirs() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    // Chunks: an uncut paragraph (block) gets no context read, only its
    // outline; a cut PDF page is read over its own cited lines only.
    let docx = "pro/docs/plan.docx";
    let pdf = "pro/papers/report.pdf";
    let body = reply(
        &scope,
        vec![
            document_hit(
                docx,
                (87, 87),
                DocumentLocation::Block { number: 31 },
                false,
            ),
            document_hit(pdf, (300, 305), DocumentLocation::Page { number: 12 }, true),
        ],
    );
    let implementation = need(NeedKind::Implementation, None, "forward plan");
    let first = initial(&mut gather, 1, &implementation)?;
    let index = run(&mut gather, first[0].clone(), body)?;
    let record = gather.records[index].clone();
    let next = react(&mut gather, 1, &implementation, Step::Chunks, &record)?;
    assert_eq!(tools(&next.reads), ["outline", "context", "outline"]);
    assert_eq!(next.reads[0].arguments["path"], docx);
    assert_eq!(
        (
            &next.reads[1].arguments["path"],
            &next.reads[1].arguments["line"],
            &next.reads[1].arguments["after"]
        ),
        (&json!(pdf), &json!(300), &json!(5))
    );
    confined(&next.reads, docx, (87, 87));
    confined(&next.reads, pdf, (300, 305));

    // Literal: an uncut slide already carries its text; nothing more to read.
    let errors = need(NeedKind::Errors, None, "deck failure");
    let first = initial(&mut gather, 1, &errors)?;
    let index = run(
        &mut gather,
        first[0].clone(),
        reply(
            &scope,
            vec![document_hit(
                "pro/decks/review.pptx",
                (40, 52),
                DocumentLocation::Slide { number: 4 },
                false,
            )],
        ),
    )?;
    let record = gather.records[index].clone();
    let next = react(&mut gather, 1, &errors, Step::Literal, &record)?;
    assert!(next.reads.is_empty(), "{:?}", tools(&next.reads));

    // Documentation: an uncut document under docs/ is passed over for the
    // first documentation hit a whole-chunk read can add to.
    let docs = need(NeedKind::Documentation, None, "approach documentation");
    let first = initial(&mut gather, 1, &docs)?;
    let index = run(
        &mut gather,
        first[0].clone(),
        reply(
            &scope,
            vec![
                document_hit(
                    "pro/docs/approach.docx",
                    (308, 308),
                    DocumentLocation::Block { number: 77 },
                    false,
                ),
                hit("docs/guide.md", 3, 9, "Guide"),
            ],
        ),
    )?;
    let record = gather.records[index].clone();
    let next = react(&mut gather, 1, &docs, Step::Search, &record)?;
    assert_eq!(tools(&next.reads), ["context"]);
    assert_eq!(next.reads[0].arguments["path"], "docs/guide.md");
    assert_eq!(next.reads[0].arguments["after"], 40);

    // A code hit keeps its line window, unchanged: at least 40 lines past
    // its first cited line, at most 100.
    let code = need(NeedKind::Overview, None, "restart guard overview");
    let first = initial(&mut gather, 1, &code)?;
    let index = run(
        &mut gather,
        first[0].clone(),
        reply(
            &scope,
            vec![
                hit("src/guard.rs", 10, 12, "fn guard() {}"),
                hit("src/long.rs", 1, 150, "fn long() {}"),
            ],
        ),
    )?;
    let record = gather.records[index].clone();
    let next = react(&mut gather, 1, &code, Step::Chunks, &record)?;
    let windows: Vec<_> = next
        .reads
        .iter()
        .filter(|read| read.tool == "context")
        .map(|read| {
            (
                read.arguments["path"].clone(),
                read.arguments["line"].clone(),
                read.arguments["after"].clone(),
            )
        })
        .collect();
    assert_eq!(
        windows,
        [
            (json!("src/guard.rs"), json!(10), json!(40)),
            (json!("src/long.rs"), json!(1), json!(100))
        ]
    );
    Ok(())
}

#[test]
fn structural_empty_result_falls_back_to_search() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let need = need(NeedKind::Definition, Some("MissingThing"), "MissingThing");
    let first = initial(&mut gather, 1, &need)?;
    let index = run(
        &mut gather,
        first[1].clone(),
        json!({"publication":scope.publication,"facts":[]}),
    )?;
    let record = gather.records[index].clone();
    let next = react(&mut gather, 1, &need, Step::Definitions, &record)?;
    assert_eq!(tools(&next.reads), ["search"]);
    assert_eq!(next.reads[0].arguments["full_chunks"], true);
    Ok(())
}

#[test]
fn a_part_search_uses_the_promoted_pool() -> Result<()> {
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope())?;
    let search = part_search(&mut gather, 2, "explain the restart guard")?;
    assert_eq!(
        (search.part, search.need, search.cause),
        (2, None, Cause::PartSearch)
    );
    assert_eq!(search.arguments["limit"], 16);
    Ok(())
}
