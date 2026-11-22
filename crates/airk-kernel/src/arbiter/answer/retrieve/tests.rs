// SPDX-License-Identifier: Apache-2.0
//! Per-sub-need recipes over synthetic canonical replies.
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
