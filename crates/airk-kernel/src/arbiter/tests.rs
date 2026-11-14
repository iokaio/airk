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
fn exact_returned_query_line_anchors_context_without_escaping_verified_citation() -> Result<()> {
    for (body, expected) in [
        (
            result(
                &scope(),
                161,
                240,
                &format!("{}amber_eof\n", "background\n".repeat(79)),
            ),
            240,
        ),
        (result(&scope(), 20, 20, "é中\namber_eof\n"), 20),
        (result(&scope(), 20, 21, "é中\namber_eof\n"), 21),
        (result(&scope(), 1, 1, "amber_eof\n"), 1),
        (
            result(&scope(), 20, 22, "amber_eof\nbackground\namber_eof\n"),
            20,
        ),
        (result(&scope(), 20, 21, "unrelated returned evidence"), 20),
    ] {
        let now = Instant::now();
        let scope = scope();
        let mut request = request();
        request.question = "amber_eof".into();
        let mut gather = Gathering::new(request, scope.clone(), application(&scope), now)?;
        gather.next(now)?;
        gather.observe(Ok(body), now)?;
        let dispatch = gather.next(now)?.context("missing verified context")?;
        assert_eq!(dispatch.arguments["line"], expected);
        assert_eq!(dispatch.arguments["before"], 0);
        assert_eq!(dispatch.arguments["after"], 32);
        assert_eq!(dispatch.arguments["max_bytes"], 4096);
    }
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

fn packing_fixture() -> Result<ContextResponse> {
    let now = Instant::now();
    let scope = scope();
    let mut gathering = Gathering::new(request(), scope.clone(), application(&scope), now)?;
    assert_eq!(gathering.next(now)?.unwrap().tool, "search");
    gathering.observe(
        Ok(json!({"publication": scope.publication, "hits": []})),
        now,
    )?;
    gathering.finish(now)
}

#[test]
fn native_packet_packing_keeps_original_search_and_omits_whole_later_receipts() -> Result<()> {
    let mut response = packing_fixture()?;
    let original = json!({"execution":{"served_mode":"hybrid","incomplete":false},
        "query":request().question,"intact":"\"".repeat(10_000)});
    response.records[0].canonical_result = original.clone();
    let initial_id = response.records[0].id;
    let mut context = response.records[0].clone();
    context.id = Sha256Digest::from_bytes([21; 32]);
    context.operation = NativeRead::Context;
    context.canonical_result = json!({"context":"\"".repeat(10_000)});
    let context_id = context.id;
    let mut later_search = context.clone();
    later_search.id = Sha256Digest::from_bytes([22; 32]);
    later_search.operation = NativeRead::Search;
    later_search.canonical_result = json!({"query":"literal bridge","intact":"\"".repeat(10_000)});
    let later_id = later_search.id;
    response.records.extend([context, later_search]);
    response.candidate_count = 3;
    response.paragraphs = vec![
        GroundedParagraph {
            text: "Initial evidence".into(),
            evidence: vec![initial_id],
            interpretation: false,
        },
        GroundedParagraph {
            text: "Additional context".into(),
            evidence: vec![context_id],
            interpretation: false,
        },
        GroundedParagraph {
            text: "Bridge relationship".into(),
            evidence: vec![initial_id, later_id],
            interpretation: true,
        },
    ];
    // The wire encodes the complete result inside a JSON string. Quotes must
    // count at that boundary, rather than just measuring the inner object.
    let envelope_scope = json!({"original_scope":"retained"});
    assert!(
        native_response_bytes(&response, Some(&envelope_scope))? > airk_core::MAX_RESPONSE_BYTES
    );
    pack_native_response(&mut response, Some(&envelope_scope))?;
    assert_eq!(response.records.len(), 1);
    assert_eq!(response.records[0].id, initial_id);
    assert_eq!(response.records[0].canonical_result, original);
    assert_eq!(response.records[0].arguments["query"], request().question);
    assert_eq!(response.candidate_count, 3);
    assert_eq!(response.omitted_count, 2);
    assert_eq!(response.stop, StopReason::Budget);
    assert_eq!(response.paragraphs.len(), 1);
    assert_eq!(response.paragraphs[0].evidence, vec![initial_id]);
    assert!(
        native_response_bytes(&response, Some(&envelope_scope))? <= airk_core::MAX_RESPONSE_BYTES
    );
    response.validate(&request().budget)?;
    Ok(())
}

#[test]
fn native_packet_packing_refuses_oversized_initial_search_without_removing_or_truncating_it()
-> Result<()> {
    let mut response = packing_fixture()?;
    response.records[0].canonical_result =
        json!({"full_result":"x".repeat(airk_core::MAX_RESPONSE_BYTES)});
    let original = serde_json::to_value(&response.records[0])?;
    let error = pack_native_response(&mut response, None).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("mandatory initial canonical search")
    );
    assert_eq!(response.records.len(), 1);
    assert_eq!(serde_json::to_value(&response.records[0])?, original);
    assert_eq!(response.omitted_count, 0);
    Ok(())
}

#[test]
fn native_packet_packing_omits_later_search_before_retaining_additional_context() -> Result<()> {
    let mut response = packing_fixture()?;
    let original = serde_json::to_value(&response.records[0])?;
    let mut context = response.records[0].clone();
    context.id = Sha256Digest::from_bytes([31; 32]);
    context.operation = NativeRead::Context;
    context.canonical_result = json!({"complete_context":"retained"});
    let original_context = serde_json::to_value(&context)?;
    let mut search = context.clone();
    search.id = Sha256Digest::from_bytes([32; 32]);
    search.operation = NativeRead::Search;
    search.canonical_result = json!({"later_search":"\"".repeat(20_000)});
    response.records.extend([context, search]);
    response.candidate_count = 3;
    pack_native_response(&mut response, None)?;
    assert_eq!(response.records.len(), 2);
    assert_eq!(serde_json::to_value(&response.records[0])?, original);
    assert_eq!(
        serde_json::to_value(&response.records[1])?,
        original_context
    );
    assert_eq!(response.omitted_count, 1);
    assert_eq!(response.candidate_count, 3);
    assert_eq!(response.stop, StopReason::Budget);
    response.validate(&request().budget)?;
    Ok(())
}

fn exact_sized_initial_result(scope: &ScopeBinding, bytes: usize) -> Result<Value> {
    let mut body = json!({"publication":scope.publication,"hits":[],"padding":""});
    let overhead = serde_json::to_vec(&body)?.len();
    assert!(bytes >= overhead);
    body["padding"] = Value::String("x".repeat(bytes - overhead));
    assert_eq!(serde_json::to_vec(&body)?.len(), bytes);
    Ok(body)
}

#[test]
fn native_initial_retention_small_budget_refuses_instead_of_packing_an_empty_success() -> Result<()>
{
    let now = Instant::now();
    let scope = scope();
    let mut request = request();
    request.budget.max_bytes = 4096;
    request.validate()?;
    let mut gathering = Gathering::new(request, scope.clone(), application(&scope), now)?;
    assert_eq!(gathering.next(now)?.unwrap().tool, "search");
    let body = exact_sized_initial_result(&scope, 3072)?;
    let error = gathering.observe(Ok(body), now).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("mandatory initial canonical search exceeds original retention budget")
    );
    // Even a caller that catches the admission error cannot finish and pack an
    // apparently valid metadata-only success after losing the initial receipt.
    assert!(gathering.finish(now).is_err());
    Ok(())
}

#[test]
fn native_initial_retention_exact_boundary_keeps_full_receipt_through_finish_and_pack() -> Result<()>
{
    let now = Instant::now();
    let scope = scope();
    let mut request = request();
    request.budget.max_bytes = 8192;
    request.validate()?;
    let mut gathering = Gathering::new(request.clone(), scope.clone(), application(&scope), now)?;
    assert_eq!(gathering.next(now)?.unwrap().tool, "search");
    let body = exact_sized_initial_result(&scope, 4096)?;
    gathering.observe(Ok(body.clone()), now)?;
    let mut response = gathering.finish(now)?;
    pack_native_response(&mut response, None)?;
    assert_eq!(response.records.len(), 1);
    assert_eq!(response.records[0].canonical_result, body);
    assert_eq!(response.records[0].arguments["query"], request.question);
    assert_eq!(response.candidate_count, 1);
    assert_eq!(response.omitted_count, 0);
    response.validate(&request.budget)?;
    Ok(())
}

#[test]
fn native_initial_retention_one_byte_over_boundary_refuses_without_raising_budget() -> Result<()> {
    let now = Instant::now();
    let scope = scope();
    let mut request = request();
    request.budget.max_bytes = 8192;
    request.validate()?;
    let mut gathering = Gathering::new(request, scope.clone(), application(&scope), now)?;
    assert_eq!(gathering.next(now)?.unwrap().tool, "search");
    let error = gathering
        .observe(Ok(exact_sized_initial_result(&scope, 4097)?), now)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("mandatory initial canonical search exceeds original retention budget")
    );
    assert!(gathering.finish(now).is_err());
    Ok(())
}
