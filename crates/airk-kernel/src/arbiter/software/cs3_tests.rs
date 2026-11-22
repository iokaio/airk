// SPDX-License-Identifier: Apache-2.0
//! Change set 3 tokens: wide pool, keyword searches and the strict deadline.
use super::plan::ContextGate;
use super::*;
use airk_core::{
    DocumentLocation,
    evidence::RevisionEnvelope,
    identity::{Incarnation, RevisionToken},
    index_catalog::IndexId,
};

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
fn request() -> ContextRequest {
    ContextRequest {
        schema: REQUEST_SCHEMA.into(),
        question: "Make `lease` honour the priority queue when workers retry failed jobs".into(),
        budget: Budget {
            calls: 12,
            fanout: 12,
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
            hit("src/queue.rs", 10, 40, "pub fn lease(&mut self) -> Option<Job> {}\n"),
            hit("src/retry.rs", 5, 20, "fn retry_after(job: &Job) {}\n"),
        ],
    )
}
/// Gathering after the initial search, with every queued dispatch rendered.
fn gathered(layers: Layers, now: Instant) -> Result<(Gathering, Value)> {
    let scope = scope();
    let mut gather = Gathering::with_layers(request(), scope.clone(), application(&scope), now, layers)?;
    gather.next(now)?.context("initial search")?;
    gather.observe(Ok(code_search(&scope)), now)?;
    let queue: Vec<Value> = gather
        .queue
        .iter()
        .map(|d| {
            json!({"id":d.choice.id,"target":d.choice.target,"tool":d.tool,"arguments":d.arguments,
                "round":d.round,"kind":d.kind})
        })
        .collect();
    Ok((gather, Value::Array(queue)))
}

#[test]
fn change_set_3_tokens_parse_and_name_back() -> Result<()> {
    let layers = Layers::parse("all,pool=48,pool_full=4,keyword_searches=3,deadline=strict")?;
    assert_eq!(layers.pool_size(), 48);
    assert_eq!(layers.keyword_searches, Some(3));
    assert!(layers.deadline_strict);
    assert_eq!(Layers::parse(&layers.names().join(","))?, layers);
    assert_eq!(Layers::parse("all,keyword_searches=0")?.keyword_searches, None);
    assert!(!Layers::parse("all,deadline=strict,deadline=lenient")?.deadline_strict);
    for refused in ["pool=49", "pool_full=49", "keyword_searches=7", "deadline=fast"] {
        assert!(Layers::parse(refused).is_err(), "{refused}");
    }
    Ok(())
}

#[test]
fn all_is_byte_identical_with_the_new_tokens_at_their_off_values() -> Result<()> {
    assert_eq!(Layers::parse("all")?, Layers::all());
    assert_eq!(
        Layers::all().names(),
        vec!["followups", "spread", "pool", "focus", "present", "device"]
    );
    let now = Instant::now();
    let off = Layers::parse("all,pool=16,keyword_searches=0,deadline=lenient")?;
    let (base, base_queue) = gathered(Layers::all(), now)?;
    let (same, same_queue) = gathered(off, now)?;
    assert_eq!(
        serde_json::to_vec(&base_queue)?,
        serde_json::to_vec(&same_queue)?
    );
    let base = serde_json::to_vec(&base.finish(now)?.paragraphs)?;
    let same = serde_json::to_vec(&same.finish(now)?.paragraphs)?;
    assert_eq!(base, same);
    // The off values change nothing at the deadline either.
    let late = now + Duration::from_secs(118);
    let (mut base, _) = gathered(Layers::all(), now)?;
    let (mut same, _) = gathered(off, now)?;
    assert_eq!(base.next(late)?.is_some(), same.next(late)?.is_some());
    Ok(())
}

#[test]
fn wide_pool_requests_shorter_snippets_and_is_bounded_by_the_candidates_budget() {
    let layers = Layers::parse("all,pool=48").unwrap();
    let wide = Budget {
        candidates: 64,
        fanout: 12,
        ..Budget::default()
    };
    let search = initial_search_for("question", &wide, layers);
    assert_eq!(search["limit"], 48);
    assert_eq!(search["max_bytes"], 48 * WIDE_POOL_SNIPPET_BYTES);
    assert_eq!(search["full_chunks"], false);
    // The request's candidates budget (default 32) still bounds the pool.
    assert_eq!(initial_search_for("question", &Budget::default(), layers)["limit"], 32);
    // At or below POOL the change set 1 formula is unchanged.
    let sixteen = initial_search_for("question", &wide, Layers::parse("all,pool=16").unwrap());
    assert_eq!(sixteen, initial_search_for("question", &wide, Layers::all()));
}

#[test]
fn distinct_files_come_first_then_second_hits_in_rank_order() {
    assert_eq!(
        distinct_files_first(&["a", "a", "b", "c", "b", "a"]),
        vec![0, 2, 3, 1, 4, 5]
    );
    assert!(distinct_files_first(&[]).is_empty());
}

#[test]
fn keyword_queries_name_identifiers_then_key_noun_pairs() {
    assert_eq!(
        keyword_queries(&request().question, 4),
        vec!["lease", "honour priority", "queue workers", "retry failed"]
    );
    assert_eq!(keyword_queries(&request().question, 1), vec!["lease"]);
    assert!(keyword_queries("Where is it?", 3).is_empty());
}

#[test]
fn keyword_searches_join_the_spread_lanes_as_follow_up_searches() -> Result<()> {
    let (gather, _) = gathered(Layers::parse("all,keyword_searches=2")?, Instant::now())?;
    let searches: Vec<&Dispatch> = gather
        .queue
        .iter()
        .filter(|d| d.kind == FollowUp::Keyword)
        .collect();
    assert_eq!(searches.len(), 2);
    assert_eq!(
        Value::Object(searches[0].arguments.clone()),
        json!({"query":"lease","limit":6,"max_bytes":4096,"full_chunks":false})
    );
    assert_eq!(searches[1].arguments["query"], "honour priority");
    assert!(searches.iter().all(|d| d.tool == "search" && d.round == 2));
    // Spread: the first keyword search runs within the first cycle of kinds.
    let first = gather.queue.iter().position(|d| d.kind == FollowUp::Keyword).unwrap();
    assert!(first < 8, "{first}");
    Ok(())
}

#[test]
fn strict_context_gate_assumes_the_worst_first_call_under_an_injected_clock() {
    let now = Instant::now();
    let gate = ContextGate::with_deadline(false, true);
    assert!(!gate.admits(now + Duration::from_secs(59), now));
    assert!(gate.admits(now + Duration::from_secs(61), now));
    let mut gate = ContextGate::with_deadline(true, true);
    gate.observe(Duration::from_secs(9));
    assert!(gate.admits(now + Duration::from_secs(22), now));
    assert!(!gate.admits(now + Duration::from_secs(21), now));
    gate.observe(Duration::from_secs(40));
    assert!(!gate.admits(now + Duration::from_secs(120), now));
    // Lenient keeps change set 1's 12 s first estimate.
    assert!(ContextGate::with_deadline(true, false).admits(now + Duration::from_secs(23), now));
}

#[test]
fn strict_deadline_drops_late_searches_then_stops_before_the_deadline() -> Result<()> {
    let now = Instant::now();
    let layers = Layers::parse("all,keyword_searches=2,deadline=strict")?;
    let (mut gather, _) = gathered(layers, now)?;
    assert!(gather.queue.iter().any(|d| d.tool == "search"));
    // 20 s left: below the search reserve, above the dispatch reserve.
    let next = gather
        .next(now + Duration::from_secs(100))?
        .context("a non-search read still runs")?;
    assert_ne!(next.tool, "search");
    assert!(gather.queue.iter().all(|d| d.tool != "search"));
    gather.observe(Ok(reply(&scope(), vec![hit("src/queue.rs", 10, 50, "whole")])), now)?;
    // 3 s left: nothing starts, and the stop says why.
    assert!(gather.next(now + Duration::from_secs(117))?.is_none());
    assert_eq!(gather.output.stop, StopReason::Deadline);
    Ok(())
}

#[test]
fn wide_pool_presentation_trims_snippets_and_lists_further_locations() -> Result<()> {
    let now = Instant::now();
    let scope = scope();
    let layers = Layers::parse("all,pool=48")?;
    let mut gather = Gathering::with_layers(request(), scope.clone(), application(&scope), now, layers)?;
    gather.next(now)?;
    let text = "x".repeat(700);
    let hits: Vec<Value> = (0..40)
        .map(|n| hit(&format!("src/file{}.rs", n % 30), 10 + 50 * (n / 30), 20 + 50 * (n / 30), &text))
        .collect();
    gather.observe(Ok(reply(&scope, hits)), now)?;
    let answer = gather.finish(now)?;
    let texts: Vec<&str> = answer.paragraphs.iter().map(|p| p.text.as_str()).collect();
    let menu = texts
        .iter()
        .find(|t| t.starts_with("Further pool locations"))
        .context("location menu")?;
    assert!(menu.contains("src/file"));
    // Pool units beyond the kept four are at most WIDE_POOL_SNIPPET_BYTES of text.
    let trimmed = texts
        .iter()
        .filter(|t| t.starts_with("(src/file") && t.ends_with(&"x".repeat(WIDE_POOL_SNIPPET_BYTES)))
        .count();
    assert!(trimmed > 0);
    assert!(
        texts
            .iter()
            .all(|t| !t.starts_with("(src/file") || t.len() < 760),
        "kept hits are presented whole, others trimmed"
    );
    Ok(())
}
