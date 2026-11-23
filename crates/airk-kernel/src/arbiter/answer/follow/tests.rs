// SPDX-License-Identifier: Apache-2.0
//! Follow-up term extraction and its de-duplication.
use super::super::gather::{Cause, Observed};
use super::super::retrieve::NeedKind;
use super::*;
use crate::arbiter::answer::tests::{hit, reply, scope};

fn span(record: usize, text: &str, end: usize) -> Span {
    let value = hit("crates/k/src/guard.rs", 10, end, text);
    Span {
        record,
        pointer: "/hits/0".into(),
        hit: serde_json::from_value(value).unwrap(),
        round: 1,
        focus: None,
        from: 0,
    }
}

fn need(query: &str) -> Need {
    Need {
        id: 2,
        kind: NeedKind::Implementation,
        subject: None,
        query: query.into(),
    }
}

fn summary(found: &[Candidate]) -> Vec<(&'static str, String)> {
    found
        .iter()
        .map(|candidate| (candidate.tool, candidate.term.clone()))
        .collect()
}

const TEXT: &str = "pub fn retry_window() -> Result<()> {\n    let budget = ReadBudget::new();\n    see crates/k/src/guard.rs\n    bail!(\"retry refused by policy\");\n}\n";

#[test]
fn extraction_order_identifiers_declared_paths_errors() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let search = gather.dispatch(
        ("search", v3::LoopRead::Search),
        json!({"query":"guard"}),
        (1, Some(2), 1),
        Cause::NeedSearch,
        FollowUp::Keyword,
        Step::Search,
        "guard",
    )?;
    gather.offer(search);
    let taken = gather.take(1, 1);
    let observed = gather.observe(
        taken[0].seq,
        Ok(reply(
            &scope,
            vec![hit("crates/k/src/guard.rs", 10, 40, TEXT)],
        )),
    )?;
    assert_eq!(observed, Observed::Retained(0));
    let found = candidates(
        &gather,
        1,
        &need("`RestartGuard` replay"),
        &[span(0, TEXT, 40)],
        &BTreeSet::new(),
        &BTreeSet::new(),
    )?;
    assert_eq!(
        summary(&found),
        [
            ("symbols", "RestartGuard".into()),
            ("symbols", "retry_window".into()),
            ("outline", "crates/k/src/guard.rs".into()),
            ("search", "retry refused by policy".into()),
            ("references", "retry_window".into()),
            ("symbols", "ReadBudget".into()),
            ("references", "ReadBudget".into()),
            ("symbols", "new".into()),
        ]
    );
    assert_eq!(found[3].class, v3::CardClass::FollowUpQuery);
    assert_eq!(found[0].class, v3::CardClass::FollowUpHop);
    Ok(())
}

#[test]
fn dedups_against_tried_and_literal_queries_and_the_cache() -> Result<()> {
    let scope = scope();
    let gather = Gathering::new(v3::BudgetV3::default(), scope)?;
    let all = candidates(
        &gather,
        1,
        &need("x"),
        &[span(0, TEXT, 40)],
        &BTreeSet::new(),
        &BTreeSet::new(),
    )?;
    let searched = BTreeSet::from(["retry refused by policy".to_owned()]);
    let tried = BTreeSet::from([all[0].key]);
    let fewer = candidates(
        &gather,
        1,
        &need("x"),
        &[span(0, TEXT, 40)],
        &searched,
        &tried,
    )?;
    assert!(!fewer.iter().any(|candidate| candidate.tool == "search"));
    assert!(!fewer.iter().any(|candidate| candidate.key == all[0].key));
    assert_eq!((all.len(), fewer.len()), (7, 5));
    Ok(())
}

#[test]
fn a_snippet_top_span_reads_its_whole_chunk_and_cards_name_each_read() -> Result<()> {
    let gather = Gathering::new(v3::BudgetV3::default(), scope())?;
    let found = candidates(
        &gather,
        1,
        &need("x"),
        &[span(0, "fn tiny() {}", 12)],
        &BTreeSet::new(),
        &BTreeSet::new(),
    )?;
    assert_eq!(found[0].tool, "context");
    assert_eq!(found[0].term, "crates/k/src/guard.rs:10-12");
    let roster = cards(2, &found, 4);
    assert_eq!(roster[0].text, "s2 read crates/k/src/guard.rs:10-12");
    assert_eq!(roster[1].text, "s2 symbols tiny");
    assert!(roster.len() <= 4);
    assert_eq!(found[1].unrun(v3::UnrunReason::NotChosen).target, "tiny");
    Ok(())
}

#[test]
fn a_rejected_quote_does_not_hide_the_next_error_string() {
    assert_eq!(
        error_strings("don't touch 'retry_refused' here"),
        ["retry_refused"]
    );
}

#[test]
fn a_repeated_error_literal_is_skipped_whole() {
    let text =
        r#"log("open failed"); retry(); log("open failed"); then the call failed done("ok")"#;
    assert_eq!(error_strings(text), ["open failed"]);
}

#[test]
fn a_rejected_literal_is_skipped_whole() {
    let text = r#"log("ok"); then the call failed; log("done"); err("open failed")"#;
    assert_eq!(error_strings(text), ["open failed"]);
    // A single-quoted literal after an apostrophe word is still found.
    assert_eq!(
        error_strings("it's 'fine' but the call failed: 'retry refused'"),
        ["retry refused"]
    );
}
