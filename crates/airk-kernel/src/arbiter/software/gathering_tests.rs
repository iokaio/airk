// SPDX-License-Identifier: Apache-2.0
//! Change-set Gathering with synthetic canonical replies; no index or model.
use super::plan::{ContextGate, select_kept};
use super::*;
use airk_core::{
    DocumentLocation,
    evidence::RevisionEnvelope,
    identity::{Incarnation, RevisionToken},
    index_catalog::IndexId,
};
use std::cell::RefCell;

fn scope() -> ScopeBinding {
    ScopeBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        catalog: Incarnation::from_bytes([2; 16]),
        index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
        authorization: Sha256Digest::from_bytes([4; 32]),
        configuration_revision: 1,
        publication: RevisionEnvelope::new(
            "fixture".into(),
            RevisionToken {
                incarnation: Incarnation::from_bytes([5; 16]),
                generation: 1,
                revision: Sha256Digest::from_bytes([6; 32]),
            },
        ),
    }
}
fn application(scope: &ScopeBinding) -> IndexApplication {
    IndexApplication {
        scope: scope.clone(),
        profile: None,
        availability: Availability::Disabled,
        reason: "Arbiter is disabled; canonical gathering remains native".into(),
    }
}
fn request(fanout: usize) -> ContextRequest {
    ContextRequest {
        schema: REQUEST_SCHEMA.into(),
        question: "Why does `Gathering` stop when plan_followups fails?".into(),
        budget: Budget {
            fanout,
            calls: 12,
            ..Budget::default()
        },
    }
}
fn hit(path: &str, start: usize, end: usize, text: &str) -> Value {
    json!({"citation":{"path":path,"content_hash":format!("hash-{path}"),"start_line":start,"end_line":end,
        "location":DocumentLocation::Lines {start,end}},"text":text,"truncated":false})
}
fn reply(scope: &ScopeBinding, hits: Vec<Value>) -> Value {
    json!({"publication":scope.publication,"generation":1,"collection":"fixture","truncated":false,"hits":hits})
}
fn code_search(scope: &ScopeBinding) -> Value {
    reply(
        scope,
        vec![
            hit("crates/k/src/arbiter.rs", 10, 40, "pub struct Gathering {\n    queue: u8,\n}\n"),
            hit("crates/k/src/arbiter/joint.rs", 5, 20, "fn pack() {}\n"),
        ],
    )
}
fn started(fanout: usize) -> Result<(Gathering, Value, Instant)> {
    let now = Instant::now();
    let scope = scope();
    let mut gather = Gathering::with_layers(
        request(fanout),
        scope.clone(),
        application(&scope),
        now,
        Layers::all(),
    )?;
    let initial = gather.next(now)?.context("initial search")?;
    assert_eq!(
        Value::Object(initial.arguments.clone()),
        json!({"query":request(fanout).question,"limit":16,"max_bytes":12_288,"full_chunks":false})
    );
    let body = code_search(&scope);
    gather.observe(Ok(body.clone()), now)?;
    Ok((gather, body, now))
}

#[test]
fn a_code_search_queues_whole_chunks_definitions_references_and_outlines_spread_across_kinds()
-> Result<()> {
    let (gather, body, _) = started(12)?;
    let per_read = per_read_bytes(serde_json::to_vec(&body)?.len());
    let kinds: Vec<_> = gather.queue.iter().map(|d| (d.kind, d.subject.clone())).collect();
    assert_eq!(
        kinds,
        vec![
            (FollowUp::WholeChunk, String::new()),
            (FollowUp::Definition, "Gathering".into()),
            (FollowUp::References, "Gathering".into()),
            (FollowUp::Outline, "crates/k/src/arbiter.rs".into()),
            (FollowUp::WholeChunk, String::new()),
            (FollowUp::Definition, "plan_followups".into()),
            (FollowUp::References, "plan_followups".into()),
            (FollowUp::Outline, "crates/k/src/arbiter/joint.rs".into()),
            (FollowUp::Definition, "pack".into()),
            (FollowUp::References, "pack".into()),
        ]
    );
    let chunk = &gather.queue[0];
    assert_eq!(chunk.tool, "context");
    assert_eq!(chunk.choice.operation, NativeRead::Context);
    assert_eq!(
        Value::Object(chunk.arguments.clone()),
        // A 30-line citation still reads WHOLE_CHUNK_LINES (40) past its first line.
        json!({"path":"crates/k/src/arbiter.rs","line":10,"before":0,"after":40,"max_bytes":per_read})
    );
    let definition = &gather.queue[1];
    assert_eq!(definition.tool, "symbols");
    assert_eq!(
        Value::Object(definition.arguments.clone()),
        json!({"name":"Gathering","limit":8,"max_bytes":4096})
    );
    assert_eq!(
        definition.choice.target,
        "Find the definition of Gathering (symbols) named at crates/k/src/arbiter.rs:10-40"
    );
    assert_eq!(gather.queue[2].tool, "references");
    assert_eq!(gather.queue[3].tool, "outline");
    let revision = scope().publication.token.to_string();
    assert!(gather.queue.iter().all(|d| d.revision == revision && d.round == 2));
    assert_eq!(gather.output.work.fanout, 10);
    assert_eq!(gather.plan_probe_paths().len(), 6);
    // The default fanout of 8 bounds the queue to the first eight, still varied.
    let (narrow, _, _) = started(8)?;
    assert_eq!(narrow.queue.len(), 8);
    assert!(narrow.queue.iter().take(4).map(|d| d.kind).collect::<BTreeSet<_>>().len() == 4);
    Ok(())
}

#[test]
fn governing_documents_are_queued_only_after_a_canonical_file_info_check() -> Result<()> {
    let (mut gather, _, _) = started(12)?;
    let calls = RefCell::new(Vec::new());
    let publication = scope().publication;
    let reader = |tool: &str, arguments: &Map<String, Value>, revision: Option<&str>| -> Result<Value> {
        calls.borrow_mut().push((tool.to_owned(), arguments["path"].clone(), revision.map(str::to_owned)));
        if arguments["path"] == "crates/k/src/arbiter/CONTRACT.md" {
            Ok(json!({"publication":publication,"path":arguments["path"],"indexed_lines":40}))
        } else {
            anyhow::bail!("not indexed")
        }
    };
    gather.resolve_governing(&reader)?;
    let governing = &gather.queue[1];
    assert_eq!(governing.kind, FollowUp::Governing);
    assert_eq!(governing.tool, "read_file");
    assert_eq!(governing.arguments["path"], "crates/k/src/arbiter/CONTRACT.md");
    assert_eq!(governing.arguments["start_line"], 1);
    assert_eq!(governing.arguments["end_line"], 40);
    let calls = calls.into_inner();
    assert!(calls.iter().all(|(tool, _, revision)| tool == "file_info"
        && revision.as_deref() == Some(scope().publication.token.to_string().as_str())));
    // The README of an already governed folder is not probed again.
    assert!(!calls.iter().any(|(_, path, _)| path == "crates/k/src/arbiter/README.md"));
    assert!(gather.queue.len() <= 12);
    Ok(())
}

#[test]
fn a_definition_reply_queues_its_excerpt_next_with_the_recipe_known_hash() -> Result<()> {
    let (mut gather, _, now) = started(12)?;
    let scope = scope();
    let chunk = gather.next(now)?.context("whole chunk")?;
    assert_eq!(chunk.kind, FollowUp::WholeChunk);
    gather.observe(Ok(reply(&scope, vec![hit("crates/k/src/arbiter.rs", 10, 40, "whole chunk text")])), now)?;
    let definition = gather.next(now)?.context("definition")?;
    assert_eq!(definition.kind, FollowUp::Definition);
    let facts = json!({"publication":scope.publication,"generation":1,"collection":"fixture","truncated":false,
        "facts":[{"citation":{"path":"crates/k/src/arbiter.rs","content_hash":"h1","start_line":12,"end_line":30,
            "location":DocumentLocation::Lines {start:12,end:30}},"name":"Gathering","kind":"struct"}]});
    gather.observe(Ok(facts), now)?;
    let excerpt = gather.next(now)?.context("excerpt")?;
    assert_eq!(excerpt.kind, FollowUp::Excerpt);
    assert_eq!(excerpt.tool, "read_file");
    assert_eq!(excerpt.round, 3);
    assert_eq!(excerpt.arguments["path"], "crates/k/src/arbiter.rs");
    assert_eq!(excerpt.arguments["start_line"], 12);
    assert_eq!(excerpt.arguments["end_line"], 30);
    assert_eq!(excerpt.arguments["known_hash"], "h1");
    assert_eq!(excerpt.revision, scope.publication.token.to_string());
    Ok(())
}

#[test]
fn a_references_reply_splits_callers_from_tests_and_runs_one_of_each_next() -> Result<()> {
    let (mut gather, _, now) = started(12)?;
    let scope = scope();
    while gather.queue.front().is_some_and(|d| d.kind != FollowUp::References) {
        gather.queue.pop_front();
    }
    let references = gather.next(now)?.context("references")?;
    let body = json!({"resolution":"exact","page":{},"evidence":reply(&scope, vec![
        hit("crates/k/src/arbiter/joint.rs", 50, 70, "gather.observe(result)"),
        hit("crates/k/src/arbiter/tests.rs", 5, 9, "Gathering::new(request)"),
        hit("crates/k/src/lib.rs", 3, 4, "use arbiter::Gathering;"),
    ])});
    gather.observe(Ok(body), now)?;
    assert_eq!(references.subject, "Gathering");
    let front: Vec<_> = gather.queue.iter().take(2).map(|d| (d.kind, d.arguments["path"].clone())).collect();
    assert_eq!(
        front,
        vec![
            (FollowUp::Callers, json!("crates/k/src/arbiter/joint.rs")),
            (FollowUp::Tests, json!("crates/k/src/arbiter/tests.rs")),
        ]
    );
    assert_eq!(gather.queue[0].arguments["end_line"], 70);
    assert!(gather.queue.iter().any(|d| d.kind == FollowUp::Callers
        && d.arguments["path"] == "crates/k/src/lib.rs"));
    Ok(())
}

#[test]
fn a_failed_follow_up_keeps_the_queue_and_its_receipt_but_an_initial_failure_refuses() -> Result<()> {
    let (mut gather, _, now) = started(12)?;
    let queued = gather.queue.len();
    gather.next(now)?.context("follow-up")?;
    let receipt = json!({"isError":true,"error":{"code":"ambiguous_definition"}});
    gather.observe(Err(crate::workbench::ReadFailure(receipt.clone()).into()), now)?;
    assert_eq!(gather.queue.len(), queued - 1);
    assert_ne!(gather.output.stop, StopReason::Refused);
    assert_eq!(gather.output.records.last().unwrap().canonical_result, receipt);
    assert!(gather.next(now)?.is_some());

    let scope = scope();
    let mut refused = Gathering::with_layers(request(12), scope.clone(), application(&scope), now, Layers::all())?;
    refused.next(now)?;
    refused.observe(Err(anyhow::anyhow!("index unavailable")), now)?;
    assert_eq!(refused.output.stop, StopReason::Refused);
    assert!(refused.queue.is_empty());
    Ok(())
}

#[test]
fn presentation_drops_overlap_groups_by_purpose_and_discloses_counts() -> Result<()> {
    let now = Instant::now();
    let scope = scope();
    let mut gather = Gathering::with_layers(request(12), scope.clone(), application(&scope), now, Layers::all())?;
    gather.next(now)?;
    gather.observe(
        Ok(reply(
            &scope,
            vec![
                hit("crates/k/src/arbiter.rs", 10, 40, "pub struct Gathering {}"),
                hit("crates/k/src/arbiter.rs", 12, 20, "queue: u8"),
                hit("crates/k/src/arbiter/tests.rs", 1, 9, "fn gathering_test() {}"),
                hit("crates/k/src/arbiter/CONTRACT.md", 1, 5, "Answer never removes evidence."),
                hit("crates/k/src/other.rs", 12, 20, "queue: u8"),
            ],
        )),
        now,
    )?;
    let answer = gather.finish(now)?;
    let texts: Vec<&str> = answer.paragraphs.iter().map(|p| p.text.as_str()).collect();
    let position = |needle: &str| texts.iter().position(|text| text.starts_with(needle));
    let change = position("Where to change:").context("change group")?;
    let rules = position("Rules to respect:").context("rules group")?;
    let tests = position("Tests:").context("tests group")?;
    assert!(change < rules && rules < tests);
    assert!(texts.iter().any(|t| t.starts_with("(crates/k/src/arbiter.rs:10–40)")));
    assert!(!texts.iter().any(|t| t.starts_with("(crates/k/src/arbiter.rs:12–20)")));
    assert!(texts.iter().any(|t| t.starts_with("(crates/k/src/other.rs:12–20)")));
    let disclosure = texts.last().unwrap();
    assert!(disclosure.contains("presented 4 evidence units") && disclosure.contains("1 overlapping"));
    assert!(answer.paragraphs.iter().all(|p| !p.evidence.is_empty()));
    Ok(())
}

#[test]
fn product_gathering_is_unchanged_without_layers() -> Result<()> {
    let now = Instant::now();
    let scope = scope();
    let mut gather = Gathering::new(request(12), scope.clone(), application(&scope), now)?;
    let initial = gather.next(now)?.context("initial")?;
    assert_eq!(initial.arguments["limit"], 8);
    assert_eq!(initial.arguments["full_chunks"], true);
    gather.observe(Ok(code_search(&scope)), now)?;
    assert!(gather.queue.iter().all(|d| d.tool == "context" || d.tool == "search"));
    assert_eq!(gather.queue[0].arguments["after"], 32);
    assert!(evaluation_layers() == Layers::default() || evaluation_layers().any());
    Ok(())
}

#[test]
fn kept_hits_pass_the_threshold_then_fill_from_the_unscored_tail() {
    let id = |byte| Sha256Digest::from_bytes([byte; 32]);
    let order: Vec<_> = (1..=6).map(id).collect();
    let scores = std::collections::BTreeMap::from([(id(1), 0.05), (id(2), 0.5), (id(3), 0.2), (id(4), 0.01)]);
    assert_eq!(select_kept(&order, &scores), vec![1, 2, 4, 5]);
    let all_low = std::collections::BTreeMap::from([(id(1), 0.0), (id(2), 0.0)]);
    assert_eq!(select_kept(&order[..2], &all_low), vec![0, 1]);
    assert!(select_kept(&[], &scores).is_empty());
}

#[test]
fn context_gate_uses_the_measured_cost_and_keeps_the_read_reserve() {
    let now = Instant::now();
    let mut gate = ContextGate::new(true);
    assert!(gate.admits(now + Duration::from_secs(60), now));
    assert!(!gate.admits(now + Duration::from_secs(21), now));
    gate.observe(Duration::from_secs(9));
    assert!(gate.admits(now + Duration::from_secs(30), now));
    assert!(!gate.admits(now + Duration::from_secs(20), now));
    gate.observe(Duration::from_secs(40));
    assert!(!gate.admits(now + Duration::from_secs(120), now));
    assert!(ContextGate::new(false).admits(now, now));
}

/// Starts change-set gathering with `layers` and observes `hits` as the first
/// search; the initial arguments come from the process's (unbound) layers.
fn started_with(layers: Layers, hits: Vec<Value>) -> Result<(Gathering, Value, Instant)> {
    let now = Instant::now();
    let scope = scope();
    let mut gather = Gathering::with_layers(request(12), scope.clone(), application(&scope), now, layers)?;
    gather.next(now)?.context("initial search")?;
    let body = reply(&scope, hits);
    gather.observe(Ok(body.clone()), now)?;
    Ok((gather, body, now))
}

#[test]
fn pool_full_below_the_snippet_pool_runs_one_full_chunk_search_of_the_top_hits_next() -> Result<()> {
    let (gather, body, _) = started_with(Layers::parse("all,pool_full=8")?, vec![
        hit("crates/k/src/arbiter.rs", 10, 40, "pub struct Gathering {\n    queue: u8,\n}\n"),
        hit("crates/k/src/arbiter/joint.rs", 5, 20, "fn pack() {}\n"),
    ])?;
    let search = &gather.queue[0];
    assert_eq!((search.kind, search.tool, search.round), (FollowUp::WholeChunk, "search", 2));
    assert_eq!(search.choice.operation, NativeRead::Search);
    assert_eq!(
        Value::Object(search.arguments.clone()),
        json!({"query":request(12).question,"limit":8,"max_bytes":12_288,"full_chunks":true})
    );
    assert!(search.choice.target.ends_with(" at crates/k/src/arbiter.rs:10-40"));
    // The reserved search bytes come out of the per-read split.
    let per_read = per_read_bytes_with(serde_json::to_vec(&body)?.len() + 12_288, KEPT, READ_FLOOR);
    assert_eq!(gather.queue[1].arguments["max_bytes"], per_read);
    assert!(gather.queue.iter().skip(1).all(|d| d.tool != "search" || d.kind != FollowUp::WholeChunk));
    // Change set 1's default, `pool_full=0` and a covering `pool_full` add none.
    for text in ["all", "all,pool_full=0", "all,pool_full=16"] {
        let (gather, _, _) = started_with(Layers::parse(text)?, vec![
            hit("crates/k/src/arbiter.rs", 10, 40, "pub struct Gathering {}\n"),
        ])?;
        assert!(!gather.queue.iter().any(|d| d.tool == "search" && d.kind == FollowUp::WholeChunk), "{text}");
    }
    Ok(())
}

#[test]
fn read_floor_and_kept_tokens_reach_the_follow_up_reads() -> Result<()> {
    let big = "x".repeat(25_000);
    let hits = || vec![
        hit("crates/k/src/arbiter.rs", 10, 40, &big),
        hit("crates/k/src/a.rs", 1, 2, "a"),
        hit("crates/k/src/b.rs", 1, 2, "b"),
        hit("crates/k/src/c.rs", 1, 2, "c"),
    ];
    let (loose, _, _) = started_with(Layers::all(), hits())?;
    let (tuned, _, _) = started_with(Layers::parse("all,read_floor=3072,kept=3")?, hits())?;
    let chunks = |gather: &Gathering| -> Vec<Value> {
        gather.queue.iter().filter(|d| d.tool == "context").map(|d| d.arguments["max_bytes"].clone()).collect()
    };
    assert_eq!(chunks(&loose), vec![json!(1536); 4]);
    assert_eq!(chunks(&tuned), vec![json!(3072); 3]);
    Ok(())
}

#[test]
fn strict_subjects_look_up_only_compound_names_that_kept_hits_name() -> Result<()> {
    let hits = || vec![
        hit("crates/k/src/arbiter.rs", 10, 40, "pub struct Gathering {}\nfn plan_followups() {}\n"),
        hit("pro/src/App.Services/ChapterService.cs", 5, 20, "public class ChapterService {}\n"),
    ];
    let subjects = |gather: &Gathering| -> Vec<String> {
        gather.queue.iter().filter(|d| d.kind == FollowUp::Definition).map(|d| d.subject.clone()).collect()
    };
    let (loose, _, _) = started_with(Layers::all(), hits())?;
    assert_eq!(subjects(&loose), ["Gathering", "plan_followups", "ChapterService"]);
    let (strict, _, _) = started_with(Layers::parse("all,subjects=strict")?, hits())?;
    assert_eq!(subjects(&strict), ["plan_followups", "ChapterService"]);
    // A strict question name that no kept hit names is not looked up.
    let (unnamed, _, _) = started_with(Layers::parse("all,subjects=strict")?, vec![
        hit("crates/k/src/arbiter.rs", 10, 40, "pub struct Gathering {}\n"),
    ])?;
    assert!(subjects(&unnamed).is_empty());
    Ok(())
}

#[test]
fn governing_project_probes_stay_inside_each_hits_project_folder() -> Result<()> {
    let hits = || vec![
        hit("pro/src/App.Services/ChapterService.cs", 5, 20, "public class ChapterService {}\n"),
        hit("crates/k/src/arbiter.rs", 10, 40, "pub struct Gathering {}\n"),
    ];
    let (any, _, _) = started_with(Layers::all(), hits())?;
    assert_eq!(
        any.plan_probe_paths(),
        [
            "pro/src/App.Services/CONTRACT.md",
            "pro/src/App.Services/README.md",
            "pro/src/CONTRACT.md",
            "pro/src/README.md",
            "crates/k/src/arbiter/CONTRACT.md",
            "crates/k/src/arbiter/README.md",
        ]
    );
    let (project, _, _) = started_with(Layers::parse("all,governing=project")?, hits())?;
    assert_eq!(
        project.plan_probe_paths(),
        [
            "pro/src/App.Services/CONTRACT.md",
            "pro/src/App.Services/README.md",
            "crates/k/src/arbiter/CONTRACT.md",
            "crates/k/src/arbiter/README.md",
            "crates/k/src/CONTRACT.md",
            "crates/k/src/README.md",
        ]
    );
    Ok(())
}
