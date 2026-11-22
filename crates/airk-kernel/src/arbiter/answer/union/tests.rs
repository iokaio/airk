// SPDX-License-Identifier: Apache-2.0
//! Union packing over synthetic loops, including the realistic full loop.
use super::super::coverage::Span;
use super::super::decide::At;
use super::super::gather::{Cause, Step};
use super::super::retrieve::{Need, NeedKind};
use super::super::state::{NeedState, RoundState};
use super::*;
use crate::arbiter::answer::tests::scope;
use airk_core::native_loop::LoopCard;

const QUESTION: &str = "Where is the restart guard defined? Who calls the replay ledger? Which tests cover the retry budget? What contract governs the queue policy?";

/// One search result at the median sizes of 115 real search records from
/// the one-turn evaluation captures (as the L1 contract test): four hits of
/// about 1,000 bytes with content hash and location, and about 2,300 bytes
/// of metadata, here with a real publication so the revision fence passes.
fn realistic_search(scope: &ScopeBinding, seed: u8) -> Value {
    let line = "    let remaining = attempt.remaining_at(clock, now_ns)?.min(Duration::from_millis(remaining_ms));\n";
    let hits: Vec<Value> = (0..4u32)
        .map(|index| {
            let start = 100 + 40 * index + u32::from(seed);
            json!({
                "citation":{
                    "content_hash":Sha256Digest::from_bytes([seed ^ 0x5a; 32]).to_string(),
                    "end_line":start + 8,
                    "location":{"Lines":{"start":start,"end":start + 8}},
                    "path":format!("crates/airk-kernel/src/arbiter/joint/part{seed}/module{index}.rs"),
                    "start_line":start,
                },
                "text":line.repeat(8),
                "truncated":false,
            })
        })
        .collect();
    let detail =
        |bytes: usize| json!({"state":"ready","detail":"d".repeat(bytes.saturating_sub(30))});
    json!({
        "collection":"airk-final",
        "discovery_readiness":detail(167),
        "execution":detail(82),
        "freshness":detail(231),
        "generation":12,
        "guidance":detail(371),
        "hits":hits,
        "publication":scope.publication,
        "request_id":format!("req-{seed:032x}"),
        "scope_observation":detail(519),
        "semantic_readiness":detail(432),
        "truncated":false,
    })
}

fn card(symbol: &str, class: v3::CardClass, text: &str) -> LoopCard {
    LoopCard {
        symbol: symbol.into(),
        class,
        text: text.into(),
        partition: vec![],
    }
}

struct Built {
    gather: Gathering,
    parts: Vec<PartState>,
    ledger: Ledger,
}

/// The default loop at its full shape: 4 parts, 4 sub-needs, 3 rounds, one
/// realistic search per sub-need and round (48 records) and 85 decisions.
fn full_loop() -> Result<Built> {
    let scope = scope();
    let budget = v3::BudgetV3 {
        max_bytes: v3::MAX_RETENTION_BYTES,
        ..v3::BudgetV3::default()
    };
    let mut gather = Gathering::new(budget, scope.clone())?;
    let mut ledger = Ledger::default();
    let plan = super::super::split::propose(QUESTION, 4);
    let partition = &plan.cards[plan.deterministic];
    assert_eq!(partition.0.len(), 4);
    let cards: Vec<LoopCard> = plan
        .cards
        .iter()
        .enumerate()
        .map(|(index, partition)| {
            super::super::split::card(QUESTION, partition, ["A", "B", "C", "D"][index])
        })
        .collect();
    ledger.deterministic(
        At::split(),
        &cards,
        &[plan.deterministic],
        v3::DeterministicRule::ExplicitBreaks,
        vec![],
    )?;
    let mut parts = Vec::new();
    for (index, range) in partition.0.iter().enumerate() {
        let id = index as u8 + 1;
        let span = super::super::split::part_span(QUESTION, *range);
        let mut part = PartState::new(id, span, &QUESTION[span.0..span.1]);
        let plan_cards: Vec<LoopCard> = (0..5)
            .map(|need| {
                card(
                    ["A", "B", "C", "D", "E"][need],
                    v3::CardClass::PlanNeed,
                    &format!("need {need}"),
                )
            })
            .collect();
        ledger.deterministic(
            At::plan(id),
            &plan_cards,
            &[0, 1, 2, 3],
            v3::DeterministicRule::EveryProposedNeed,
            vec![],
        )?;
        for need in 1..=4u8 {
            part.needs.push(NeedState::new(Need {
                id: need,
                kind: NeedKind::Callers,
                subject: None,
                query: format!("restart guard caller part {id} need {need}"),
            }));
        }
        parts.push(part);
    }
    for round in 1..=3u8 {
        for part in &mut parts {
            let mut state = RoundState::default();
            for need in 1..=4u8 {
                let dispatch = gather.dispatch(
                    ("search", v3::LoopRead::Search),
                    json!({"query":format!("restart guard caller part {} need {need} round {round}", part.id),
                        "limit":4,"max_bytes":4096,"full_chunks":false}),
                    (part.id, Some(need), round),
                    if round == 1 { Cause::NeedSearch } else { Cause::FollowUp },
                    software::FollowUp::Keyword,
                    Step::Search,
                    "restart guard",
                )?;
                gather.offer(dispatch);
                let taken = gather.take(part.id, 1);
                let seed = part.id * 16 + need * 4 + round;
                let record =
                    match gather.observe(taken[0].seq, Ok(realistic_search(&scope, seed)))? {
                        super::super::gather::Observed::Retained(record) => record,
                        other => anyhow::bail!("unexpected {other:?}"),
                    };
                let state_need = part.need(need).unwrap();
                state_need.coverage = v3::Coverage::Partial;
                state_need.evidence.push(record);
                for (pointer, hit) in super::super::coverage::record_hits(&gather.records[record]) {
                    state_need.history.push(Span {
                        record,
                        pointer,
                        hit,
                        round,
                        focus: None,
                    });
                }
                if round > 1 {
                    state.follow_ups_run.push(v3::FollowUp {
                        operation: v3::LoopRead::Search,
                        target: format!("term {need}"),
                    });
                }
                state.calls += 1;
            }
            state.fanout = 4;
            state.new_evidence = 4;
            part.rounds.push(state);
        }
        for part in &parts {
            for need in 1..=4u8 {
                let coverage = [
                    card("A", v3::CardClass::Covered, &format!("s{need} covered")),
                    card(
                        "B",
                        v3::CardClass::SearchForTerm,
                        &format!("s{need} search: term"),
                    ),
                ];
                ledger.deterministic(
                    At::need(v3::LoopStage::Coverage, part.id, need, round),
                    &coverage,
                    &[1],
                    v3::DeterministicRule::TopFocusSpan,
                    vec![],
                )?;
                if round < 3 {
                    ledger.deterministic(
                        At::need(v3::LoopStage::FollowUp, part.id, need, round),
                        &[card(
                            "A",
                            v3::CardClass::FollowUpQuery,
                            &format!("s{need} search \"term\""),
                        )],
                        &[0],
                        v3::DeterministicRule::FirstExtractedTerm,
                        vec![],
                    )?;
                }
            }
        }
    }
    for part in &mut parts {
        part.stop = Some(v3::PartStop::Rounds);
        part.share_ms = 70_000;
        for need in &mut part.needs {
            need.unrun.push(v3::UnrunRead {
                operation: v3::LoopRead::References,
                target: format!("callers_of_part_{}_need_{}", part.id, need.need.id),
                reason: v3::UnrunReason::RoundLimit,
            });
        }
    }
    assert_eq!((gather.records.len(), ledger.entries.len()), (48, 85));
    Ok(Built {
        gather,
        parts,
        ledger,
    })
}

fn application(scope: &ScopeBinding) -> IndexApplication {
    IndexApplication {
        scope: scope.clone(),
        profile: None,
        availability: Availability::Disabled,
        reason: "fixture".into(),
    }
}

fn assembly<'a>(built: &'a Built, scope: &'a ScopeBinding, limit: usize) -> Assembly<'a> {
    Assembly {
        question: QUESTION,
        scope,
        parts: &built.parts,
        gather: &built.gather,
        ledger: &built.ledger,
        degradations: vec![
            v3::Degradation::ContextAbsent,
            v3::Degradation::FocusAbsent,
            v3::Degradation::SearchFocusDeferred,
        ],
        application: application(scope),
        elapsed_ms: 240_000,
        limit,
    }
}

#[test]
fn the_realistic_full_loop_keeps_evidence_before_decisions() -> Result<()> {
    let built = full_loop()?;
    let scope = scope();
    let budget = v3::BudgetV3 {
        max_bytes: v3::MAX_RETENTION_BYTES,
        ..v3::BudgetV3::default()
    };
    // The kernel packs against the reply frame less the default 4 KiB
    // transport reserve.
    let limit = v3::reply_limit(&budget) - 4096;
    let (response, log) = assemble(&assembly(&built, &scope, limit))?;
    response.validate_with_log(&log, &budget)?;
    let bytes = serde_json::to_vec(&response)?.len();
    let spans: usize = response
        .records
        .iter()
        .map(|record| {
            record.canonical_result["spans"]
                .as_array()
                .map_or(0, Vec::len)
        })
        .sum();
    let excerpt: usize = response
        .records
        .iter()
        .flat_map(|record| {
            record.canonical_result["spans"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .map(|span| span["text"].as_str().map_or(0, str::len))
        .max()
        .unwrap_or(0);
    let full: usize = built.gather.records.iter().map(|record| record.bytes).sum();
    eprintln!(
        "realistic full loop: {} of 48 records, {spans} of 192 spans (excerpts up to {excerpt} bytes), \
         {} of 85 decisions in {bytes} bytes (limit {limit}); intact records {full} bytes in custody",
        response.records.len(),
        response.decisions.len(),
    );
    eprintln!(
        "  records {} paragraphs {} parts {} record_uses {} decisions {} log {}",
        serde_json::to_vec(&response.records)?.len(),
        serde_json::to_vec(&response.paragraphs)?.len(),
        serde_json::to_vec(&response.parts)?.len(),
        serde_json::to_vec(&response.record_uses)?.len(),
        serde_json::to_vec(&response.decisions)?.len(),
        serde_json::to_vec(&response.decision_log)?.len(),
    );
    assert!(bytes <= limit);
    assert!(
        response.records.len() > 9,
        "more records survive than whole-record packing kept"
    );
    assert!(
        response.decisions.len() < 85,
        "decisions are omitted before records"
    );
    assert_eq!(response.decision_log.total, 85);
    assert_eq!(response.decision_log.omitted, 85 - response.decisions.len());
    Ok(())
}

#[test]
fn frame_shared_by_parts_shortened_part_says_so() -> Result<()> {
    let built = full_loop()?;
    let scope = scope();
    let budget = v3::BudgetV3 {
        max_bytes: v3::MAX_RETENTION_BYTES,
        ..v3::BudgetV3::default()
    };
    // A small frame forces whole records out after decisions and excerpts.
    let (response, log) = assemble(&assembly(&built, &scope, 40_000))?;
    response.validate_with_log(&log, &budget)?;
    assert!(response.omitted_count > 0);
    assert!(response.decisions.is_empty());
    assert!(response.parts.iter().any(|part| part.shortened));
    assert_eq!(
        response
            .partial
            .as_ref()
            .map(|partial| (partial.stage, partial.reason)),
        Some((v3::LoopStage::Union, v3::PartialReason::Bytes))
    );
    assert!(!response.omitted_evidence.is_empty());
    for part in &response.parts {
        for need in &part.sub_needs {
            let omitted = need
                .evidence
                .iter()
                .filter(|id| response.omitted_evidence.contains(id))
                .count();
            assert_eq!(usize::from(need.evidence_omitted), omitted);
        }
    }
    Ok(())
}

#[test]
fn duplicate_unit_presented_once_with_all_provenance() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let mut parts = Vec::new();
    let mut ledger = Ledger::default();
    let question = "Where is the restart guard? Who calls the restart guard now?";
    let plan = super::super::split::propose(question, 4);
    let partition = &plan.cards[plan.deterministic];
    let cards: Vec<LoopCard> = plan
        .cards
        .iter()
        .enumerate()
        .map(|(index, partition)| {
            super::super::split::card(question, partition, ["A", "B", "C"][index])
        })
        .collect();
    ledger.deterministic(
        At::split(),
        &cards,
        &[plan.deterministic],
        v3::DeterministicRule::ExplicitBreaks,
        vec![],
    )?;
    for (index, range) in partition.0.iter().enumerate() {
        let id = index as u8 + 1;
        let span = super::super::split::part_span(question, *range);
        let mut part = PartState::new(id, span, &question[span.0..span.1]);
        part.needs.push(NeedState::new(Need {
            id: 1,
            kind: NeedKind::Definition,
            subject: Some("restart_guard".into()),
            query: "restart_guard".into(),
        }));
        ledger.deterministic(
            At::plan(id),
            &[
                card("A", v3::CardClass::PlanNeed, "need"),
                card("B", v3::CardClass::PlanNeed, "none"),
            ],
            &[0],
            v3::DeterministicRule::EveryProposedNeed,
            vec![],
        )?;
        parts.push(part);
    }
    for part in &mut parts {
        let dispatch = gather.dispatch(
            ("symbols", v3::LoopRead::Symbols),
            json!({"name":"restart_guard"}),
            (part.id, Some(1), 1),
            Cause::Structural,
            software::FollowUp::Definition,
            Step::Leaf,
            "restart_guard",
        )?;
        gather.offer(dispatch);
    }
    let taken = gather.take(1, 1);
    let body = json!({"publication":scope.publication,"facts":[{"text":"restart_guard",
        "citation":{"path":"src/guard.rs","start_line":1,"end_line":9,"content_hash":"abc"}}]});
    gather.observe(taken[0].seq, Ok(body))?;
    let search = gather.dispatch(
        ("search", v3::LoopRead::Search),
        json!({"query":"Who calls the restart guard now?"}),
        (2, None, 1),
        Cause::PartSearch,
        software::FollowUp::Initial,
        Step::Search,
        "part",
    )?;
    gather.offer(search);
    let taken = gather.take(2, 1);
    gather.observe(
        taken[0].seq,
        Ok(crate::arbiter::answer::tests::reply(&scope, vec![])),
    )?;
    for part in &mut parts {
        let need = part.need(1).unwrap();
        need.coverage = v3::Coverage::Partial;
        need.evidence = vec![0];
        part.rounds.push(RoundState {
            calls: 1,
            fanout: 1,
            ..RoundState::default()
        });
        part.stop = Some(v3::PartStop::NoJustifiedRead);
    }
    let built = Built {
        gather,
        parts,
        ledger,
    };
    let budget = v3::BudgetV3::default();
    let (response, log) = assemble(&assembly_for(&built, &scope, question))?;
    response.validate_with_log(&log, &budget)?;
    assert_eq!(response.records.len(), 2);
    let symbols = &response.records[0];
    let parts: BTreeSet<u8> = response
        .record_uses
        .iter()
        .filter(|item| item.record == symbols.id)
        .map(|item| item.part)
        .collect();
    assert_eq!(
        parts,
        BTreeSet::from([1, 2]),
        "one record keeps both parts' provenance"
    );
    assert_eq!(response.union.dedup_count, 1);
    assert_eq!(symbols.canonical_result["rows"][0]["path"], "src/guard.rs");
    Ok(())
}

fn assembly_for<'a>(built: &'a Built, scope: &'a ScopeBinding, question: &'a str) -> Assembly<'a> {
    Assembly {
        question,
        ..assembly(built, scope, 120_000)
    }
}

#[test]
fn divergent_verdicts_are_noted_not_resolved() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let dispatch = gather.dispatch(
        ("search", v3::LoopRead::Search),
        json!({"query":"guard"}),
        (1, Some(1), 1),
        Cause::NeedSearch,
        software::FollowUp::Keyword,
        Step::Search,
        "guard",
    )?;
    gather.offer(dispatch);
    let other = gather.dispatch(
        ("search", v3::LoopRead::Search),
        json!({"query":"guard"}),
        (2, Some(1), 1),
        Cause::NeedSearch,
        software::FollowUp::Keyword,
        Step::Search,
        "guard",
    )?;
    gather.offer(other);
    let taken = gather.take(1, 1);
    gather.observe(
        taken[0].seq,
        Ok(crate::arbiter::answer::tests::reply(&scope, vec![])),
    )?;
    let make = |id: u8, coverage: v3::Coverage| {
        let mut part = PartState::new(id, (0, 1), "x");
        let mut need = NeedState::new(Need {
            id: 1,
            kind: NeedKind::Tests,
            subject: Some("restart_guard".into()),
            query: "restart_guard test".into(),
        });
        need.coverage = coverage;
        need.evidence = vec![0];
        part.needs.push(need);
        part
    };
    let parts = vec![
        make(1, v3::Coverage::Covered),
        make(2, v3::Coverage::Partial),
    ];
    let notes = conflicts(&gather, &parts);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].kind, v3::ConflictKind::DivergentVerdict);
    assert_eq!(notes[0].parts, [1, 2]);
    assert_eq!(notes[0].evidence, [gather.records[0].id]);
    Ok(())
}
