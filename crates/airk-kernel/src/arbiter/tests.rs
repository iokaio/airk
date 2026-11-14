// SPDX-License-Identifier: Apache-2.0
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
fn request() -> ContextRequest {
    ContextRequest {
        schema: REQUEST_SCHEMA.into(),
        question: "Where is the constraint defined?".into(),
        budget: Budget::default(),
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
fn result(scope: &ScopeBinding, start: usize, end: usize, text: &str) -> Value {
    json!({"publication":scope.publication,"generation":1,"collection":"fixture","truncated":false,
        "hits":[{"citation":{"path":"policy.md","content_hash":"source-hash","start_line":start,"end_line":end,
            "location":DocumentLocation::Lines {start,end}},"text":text,"truncated":false}],
        "continuation":{"preserve":"all original fields"},"readiness":{"source":"fixture"}})
}

#[test]
fn gathering_expands_canonical_context_and_keeps_full_original_receipts() -> Result<()> {
    let now = Instant::now();
    let scope = scope();
    let mut gather = Gathering::new(request(), scope.clone(), application(&scope), now)?;
    assert_eq!(gather.next(now)?.unwrap().tool, "search");
    let initial = result(&scope, 20, 22, "A bounded request retains its deadline.");
    gather.observe(Ok(initial.clone()), now)?;
    let expanded = gather.next(now)?.unwrap();
    assert_eq!(expanded.tool, "context");
    assert_eq!(expanded.arguments["path"], "policy.md");
    assert_eq!(expanded.revision, scope.publication.token.to_string());
    let full = result(
        &scope,
        20,
        52,
        "The exception also requires delivery authority to remain live.",
    );
    gather.observe(Ok(full.clone()), now)?;
    assert!(gather.next(now)?.is_none());
    let answer = gather.finish(now)?;
    assert_eq!(answer.records[0].canonical_result, initial);
    assert_eq!(answer.records[1].canonical_result, full);
    assert!(
        answer
            .paragraphs
            .iter()
            .any(|paragraph| paragraph.text.contains("delivery authority"))
    );
    assert_eq!(answer.work.calls, 2);
    assert_eq!(answer.applications[0].availability, Availability::Disabled);
    assert!(answer.advice.is_empty());
    Ok(())
}
#[test]
fn overlapping_original_hits_keep_separate_bounded_verified_expansions() -> Result<()> {
    let now = Instant::now();
    let scope = scope();
    let mut gather = Gathering::new(request(), scope.clone(), application(&scope), now)?;
    gather.next(now)?;
    let mut body = result(&scope, 20, 22, "first");
    body["hits"].as_array_mut().unwrap().extend(
        result(&scope, 23, 25, "second")["hits"]
            .as_array()
            .unwrap()
            .clone(),
    );
    gather.observe(Ok(body), now)?;
    assert_eq!(gather.choices().len(), 2);
    assert_eq!(gather.choices()[0].target, "policy.md:20-52");
    assert_eq!(gather.choices()[1].target, "policy.md:23-55");
    Ok(())
}
#[test]
fn original_call_limit_and_deadline_never_grant_another_dispatch() -> Result<()> {
    let now = Instant::now();
    let scope = scope();
    let mut limited = request();
    limited.budget.calls = 1;
    let mut gather = Gathering::new(limited, scope.clone(), application(&scope), now)?;
    gather.next(now)?;
    gather.observe(Ok(result(&scope, 20, 22, "initial")), now)?;
    assert!(gather.next(now)?.is_none());
    assert_eq!(gather.finish(now)?.stop, StopReason::Budget);
    let mut gather = Gathering::new(request(), scope.clone(), application(&scope), now)?;
    let expired = now + Duration::from_secs(31);
    assert!(gather.next(expired)?.is_none());
    let answer = gather.finish(expired)?;
    assert_eq!(answer.stop, StopReason::Deadline);
    assert_eq!(answer.work.calls, 0);
    Ok(())
}
#[test]
fn crossed_publication_is_not_retained_as_source_evidence() -> Result<()> {
    let now = Instant::now();
    let original = scope();
    let mut later = scope();
    later.publication.generation = 2;
    later.publication.token.generation = 2;
    let mut gather = Gathering::new(request(), original.clone(), application(&original), now)?;
    gather.next(now)?;
    gather.observe(Ok(result(&later, 20, 22, "crossed publication")), now)?;
    let answer = gather.finish(now)?;
    assert_eq!(answer.stop, StopReason::Stale);
    assert!(answer.records.is_empty());
    Ok(())
}

#[test]
fn long_document_late_hit_choices_start_at_the_verified_hit_and_stay_distinct() -> Result<()> {
    let now = Instant::now();
    let scope = scope();
    let mut gather = Gathering::new(request(), scope.clone(), application(&scope), now)?;
    gather.next(now)?;
    let mut body = result(&scope, 200, 220, "late verified evidence");
    body["hits"].as_array_mut().unwrap().extend(
        result(&scope, 240, 260, "another required section")["hits"]
            .as_array()
            .unwrap()
            .clone(),
    );
    gather.observe(Ok(body), now)?;
    let choices = gather.choices();
    assert_eq!(
        choices.len(),
        2,
        "nearby long-document hits must remain separately reachable under each byte cap"
    );
    assert_eq!(choices[0].target, "policy.md:200-232");
    assert_eq!(choices[1].target, "policy.md:240-272");
    let dispatch = gather.next(now)?.unwrap();
    assert_eq!(dispatch.arguments["line"], 200);
    assert_eq!(dispatch.arguments["max_bytes"], 4096);
    Ok(())
}

#[test]
fn duplicate_context_keeps_the_same_bounded_literal_bridge_choice_for_both_conditions() -> Result<()>
{
    let now = Instant::now();
    let scope = scope();
    let initial = result(&scope, 1, 1, "John Smith was the connected source actor.");
    let mut first = Gathering::new(request(), scope.clone(), application(&scope), now)?;
    first.next(now)?;
    first.observe(Ok(initial.clone()), now)?;
    let mut same = Gathering::new(request(), scope.clone(), application(&scope), now)?;
    same.next(now)?;
    same.observe(Ok(initial.clone()), now)?;
    assert_eq!(
        serde_json::to_value(first.choices())?,
        serde_json::to_value(same.choices())?,
        "deterministic and model conditions must receive the same native catalog"
    );
    assert_eq!(first.choices().len(), 2);
    assert_eq!(first.choices()[0].operation, NativeRead::Context);
    assert_eq!(first.choices()[1].operation, NativeRead::Search);
    let provenance = literal::SourceLiteral::parse(&first.choices()[1].target)?.unwrap();
    assert_eq!(provenance.query, "John Smith");
    assert_eq!(
        (provenance.returned_utf8_start, provenance.returned_utf8_end),
        (0, 10)
    );
    assert_eq!(first.next(now)?.unwrap().tool, "context");
    first.observe(Ok(initial), now)?;
    let bridge = first
        .next(now)?
        .context("duplicate context must not discard a different legal source search")?;
    assert_eq!(bridge.tool, "search");
    assert_eq!(
        bridge.arguments,
        json!({"query":"John Smith","limit":4,"max_bytes":4096,"full_chunks":true})
            .as_object()
            .unwrap()
            .clone()
    );
    assert_eq!(bridge.round, 2);
    let mut additional = result(
        &scope,
        1,
        2,
        "A connected fact from another indexed source.",
    );
    additional["hits"][0]["citation"]["path"] = "bridge.md".into();
    first.observe(Ok(additional.clone()), now)?;
    let response = first.finish(now)?;
    assert!(
        response
            .records
            .iter()
            .any(|r| r.canonical_result == additional)
    );
    assert!(response.paragraphs.iter().any(|p| p.interpretation
        && p.evidence.len() == 2
        && p.text.contains("UTF-8 bytes 0..10")));
    assert_eq!(response.work.calls, 3);
    Ok(())
}
#[test]
fn literal_bridge_preserves_original_call_and_publication_fences() -> Result<()> {
    let now = Instant::now();
    let scope = scope();
    let mut limited = request();
    limited.budget.calls = 2;
    let mut gather = Gathering::new(limited, scope.clone(), application(&scope), now)?;
    gather.next(now)?;
    let initial = result(&scope, 1, 1, "John Smith was the bridge.");
    gather.observe(Ok(initial.clone()), now)?;
    gather.next(now)?;
    gather.observe(Ok(initial), now)?;
    assert!(gather.next(now)?.is_none());
    assert_eq!(gather.finish(now)?.stop, StopReason::Budget);
    let mut gather = Gathering::new(request(), scope.clone(), application(&scope), now)?;
    gather.next(now)?;
    gather.observe(Ok(result(&scope, 1, 1, "John Smith was the bridge.")), now)?;
    gather.next(now)?;
    gather.observe(Ok(result(&scope, 1, 1, "John Smith was the bridge.")), now)?;
    gather.next(now)?;
    let mut foreign = scope.clone();
    foreign.publication.generation = 2;
    foreign.publication.token.generation = 2;
    gather.observe(
        Ok(result(
            &foreign,
            1,
            1,
            "FOREIGN source must not be retained",
        )),
        now,
    )?;
    let response = gather.finish(now)?;
    assert_eq!(response.stop, StopReason::Stale);
    assert!(!serde_json::to_string(&response)?.contains("FOREIGN source"));
    Ok(())
}
