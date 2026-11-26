// SPDX-License-Identifier: Apache-2.0
//! Follow-up term extraction and its de-duplication.
use super::super::gather::{Cause, Dispatch, Observed};
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
fn a_document_top_span_is_read_only_within_its_own_location() -> Result<()> {
    let gather = Gathering::new(v3::BudgetV3::default(), scope())?;
    let at = |location: airk_core::DocumentLocation, truncated: bool| Span {
        record: 0,
        pointer: "/hits/0".into(),
        hit: serde_json::from_value(json!({"citation":{"path":"pro/docs/plan.docx",
            "content_hash":"abc","start_line":87,"end_line":88,"location":location},
            "text":"Forward plan","truncated":truncated}))
        .unwrap(),
        round: 1,
        focus: None,
        from: 0,
    };
    // An uncut paragraph block: the span already holds its text whole.
    let uncut = candidates(
        &gather,
        1,
        &need("x"),
        &[at(airk_core::DocumentLocation::Block { number: 31 }, false)],
        &BTreeSet::new(),
        &BTreeSet::new(),
    )?;
    assert!(!uncut.iter().any(|candidate| candidate.tool == "context"));
    // A cut page: its own two cited lines, never a 40-line window.
    let cut = candidates(
        &gather,
        1,
        &need("x"),
        &[at(airk_core::DocumentLocation::Page { number: 3 }, true)],
        &BTreeSet::new(),
        &BTreeSet::new(),
    )?;
    let context: Vec<_> = cut
        .iter()
        .filter(|candidate| candidate.tool == "context")
        .map(|candidate| {
            (
                candidate.arguments["line"].clone(),
                candidate.arguments["before"].clone(),
                candidate.arguments["after"].clone(),
            )
        })
        .collect();
    assert_eq!(context, [(json!(87), json!(0), json!(1))]);
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

fn keep(
    gather: &mut Gathering,
    scope: &ScopeBinding,
    read: Dispatch,
    hits: Vec<Value>,
) -> Result<()> {
    assert_eq!(read.revision, gather.revision);
    let part = read.part;
    gather.offer(read);
    let taken = gather.take(part, 1);
    assert_eq!(taken.len(), 1);
    assert!(matches!(
        gather.observe(taken[0].seq, Ok(reply(scope, hits)))?,
        Observed::Retained(_)
    ));
    Ok(())
}

fn read_source(gather: &mut Gathering, path: &str) -> Result<Dispatch> {
    gather.dispatch(
        ("read_file", v3::LoopRead::ReadFile),
        json!({"path":path,"start_line":5,"end_line":15,"max_bytes":3072}),
        (1, Some(2), 1),
        Cause::Structural,
        FollowUp::Excerpt,
        Step::Leaf,
        path,
    )
}

#[test]
fn source_call_joins_a_distinct_cited_hash_bound_range_after_rename_and_move() -> Result<()> {
    for (literal, origin, target) in [
        ("flush.stage-a", "shell/outbound.ts", "worker/receive.ts"),
        ("/v3/stages/flush", "moved/client.ts", "routing/typed.ts"),
    ] {
        let scope = scope();
        let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
        let source = read_source(&mut gather, origin)?;
        keep(
            &mut gather,
            &scope,
            source,
            vec![hit(origin, 5, 15, &format!("relay.emit('{literal}', payload);"))],
        )?;
        let search = gather.dispatch(
            ("search", v3::LoopRead::Search),
            json!({"query":"locate the receiver", "full_chunks":true, "limit":4}),
            (1, Some(2), 1),
            Cause::NeedSearch,
            FollowUp::Keyword,
            Step::Search,
            "locate the receiver",
        )?;
        let receiver = hit(target, 32, 42, &format!("routes.bind(\"{literal}\", apply);"));
        let hash = receiver["citation"]["content_hash"].clone();
        keep(&mut gather, &scope, search, vec![receiver])?;
        let spans = super::super::coverage::spans(&gather, 1, 2, 1);
        let found = candidates(
            &gather,
            1,
            &need("locate the receiver"),
            &spans,
            &BTreeSet::new(),
            &BTreeSet::new(),
        )?;
        assert_eq!(found[0].tool, "read_file");
        assert_eq!(
            Value::Object(found[0].arguments.clone()),
            json!({"path":target,"start_line":32,"end_line":42,
                   "known_hash":hash,"max_bytes":3072})
        );
        assert_eq!(
            found[0].key,
            read_key("read_file", &found[0].arguments, &gather.revision)?
        );
        assert_eq!(found[0].term, format!("{target}:32-42"));
        assert!(!found.iter().any(|candidate| candidate.term == literal));
    }
    Ok(())
}

#[test]
fn tokenized_join_search_is_navigation_and_only_exact_contrasting_call_advances() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let origin = "sending/outbound.ts";
    let source = read_source(&mut gather, origin)?;
    let sender = hit(origin, 5, 15, "client.send('pipe.open-lane', payload);");
    keep(&mut gather, &scope, source, vec![sender.clone()])?;
    let spans = super::super::coverage::spans(&gather, 1, 2, 1);
    let task = need("locate the receiver");
    let found = candidates(
        &gather,
        1,
        &task,
        &spans,
        &BTreeSet::new(),
        &BTreeSet::new(),
    )?;
    let discovery = &found[0];
    assert_eq!(discovery.operation, v3::LoopRead::Search);
    assert_eq!(
        Value::Object(discovery.arguments.clone()),
        json!({"query":"pipe.open-lane","mode":"lexical","limit":4,
               "max_bytes":4096,"full_chunks":true})
    );
    let read = gather.dispatch(
        (discovery.tool, discovery.operation),
        Value::Object(discovery.arguments.clone()),
        (1, Some(2), 2),
        Cause::FollowUp,
        discovery.kind,
        discovery.step,
        &discovery.term,
    )?;
    let target = "handled/route.ts";
    let receiver = hit(target, 20, 30, "schema.route(\"pipe.open-lane\", parse);");
    let hash = receiver["citation"]["content_hash"].clone();
    keep(
        &mut gather,
        &scope,
        read,
        vec![
            hit("other/near.ts", 20, 30, "schema.route('pipe.open-lane-extra', parse);"),
            hit("other/case.ts", 20, 30, "schema.route('Pipe.open-lane', parse);"),
            hit("other/call.ts", 20, 30, "client.send('pipe.open-lane', payload);"),
            receiver,
        ],
    )?;
    assert!(only_join_discovery(&gather, 1, 2, 2));
    assert!(!only_join_discovery(&gather, 1, 2, 1));
    assert!(!only_join_discovery(&gather, 1, 1, 2));
    let spans = super::super::coverage::spans(&gather, 1, 2, 2);
    assert!(spans.is_empty());
    assert_eq!(gather.records[1].body["hits"].as_array().unwrap().len(), 4);
    let exact = candidates(
        &gather,
        1,
        &task,
        &spans,
        &BTreeSet::new(),
        &BTreeSet::new(),
    )?;
    assert_eq!(exact[0].operation, v3::LoopRead::ReadFile);
    assert_eq!(
        Value::Object(exact[0].arguments.clone()),
        json!({"path":target,"start_line":20,"end_line":30,
               "known_hash":hash,"max_bytes":3072})
    );
    let tried = BTreeSet::from([exact[0].key]);
    let remaining = candidates(&gather, 1, &task, &spans, &BTreeSet::new(), &tried)?;
    assert!(!remaining.iter().any(|candidate| candidate.tool == "read_file"));
    Ok(())
}

#[test]
fn faked_escaped_multiline_or_incomplete_calls_do_not_supply_join_tokens() {
    let text = r#"
const quoted = "fake('from.string')";
const nested = "a multiline string
  fake('from.string');
";
send("has\.escape");
send('two.
lines');
send('joined.route' + suffix);
send(value, 'second.arg');
send("unfinished.route
still unmatched");
send('/path/v1', handle);
send(`dynamic.${name}`);
// shadow('from.comment');
/*
  shadow('from.block');
*/
"#;
    let calls: Vec<_> = call_literals(text)
        .into_iter()
        .map(|literal| (literal.text, literal.site))
        .collect();
    assert_eq!(calls, [("/path/v1", "send")]);
}
