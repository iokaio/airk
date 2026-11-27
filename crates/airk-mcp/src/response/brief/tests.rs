// SPDX-License-Identifier: Apache-2.0
//! Brief reply tests over canned kernel replies. Expectations come from the
//! fixture's own texts and locations, never from the projection.
use super::*;
use crate::response::ResponseMode;
use airk_core::arbiter::answer_loop::{
    AnswerPart, AnswerTicket, CONTEXT_SCHEMA_V3, Coverage, DecisionLogSummary, GatheringReads,
    GatheringReport, GatheringStage, GatheringStageKind, LoopEvidence, LoopModelWork, LoopRead,
    LoopStage, LoopTimings, PACKET_SCHEMA_V2, PartStop, PartialMarker, PartialReason, QuestionSpan,
    RecordUse, SubNeed, SubNeedKind, SubNeedOrigin, UnionSummary, UnrunRead, UnrunReason, UseCause,
    empty_log_digest,
};
use airk_core::arbiter::{EvidenceRole, Gap, GroundedParagraph, ScopeBinding, Work};
use airk_core::evidence::RevisionEnvelope;
use airk_core::identity::{Incarnation, RevisionToken, Sha256Digest};
use airk_core::index_catalog::IndexId;
use serde_json::json;

fn token() -> RevisionToken {
    RevisionToken {
        incarnation: Incarnation::from_bytes([3; 16]),
        generation: 7,
        revision: Sha256Digest::from_bytes([4; 32]),
    }
}

fn scope() -> ScopeBinding {
    ScopeBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        catalog: Incarnation::from_bytes([2; 16]),
        index_id: IndexId::from_identity(Incarnation::from_bytes([5; 16])),
        authorization: Sha256Digest::from_bytes([6; 32]),
        configuration_revision: 1,
        publication: RevisionEnvelope::new("fixture".into(), token()),
    }
}

fn id(record: usize) -> Sha256Digest {
    let mut bytes = [9u8; 32];
    bytes[0] = u8::try_from(record).unwrap();
    bytes[1] = 0xab;
    Sha256Digest::from_bytes(bytes)
}

/// `lines` numbered source lines of about `width` bytes each.
fn text(path: &str, start: usize, lines: usize, width: usize) -> String {
    (start..start + lines)
        .map(|line| {
            let head = format!("    let {}_{line} = \"", path.replace(['/', '.'], "_"));
            format!(
                "{head}{}\";\n",
                "x".repeat(width.saturating_sub(head.len() + 3))
            )
        })
        .collect()
}

/// A canned kernel reply and, independently, every location's whole text.
struct Canned {
    reply: ContextResponseV3,
    texts: BTreeMap<String, String>,
    /// Each need's cited locations in its paragraph's order.
    cited: Vec<Vec<String>>,
}

const NEED_TEXTS: [&str; 8] = [
    "where the retention budget is decided",
    "who calls the delivery function",
    "how the union packs the reply frame",
    "which tests cover partial replies",
    "the project's rules for changing replies",
    "how the test command runs",
    "the build file of the MCP crate",
    "where the agent's billing ledger is written",
];

/// Eight sub-needs in one part, five search records each (about forty
/// records of two spans, each span about 1.6 KB), the last need with none.
fn canned(width: usize) -> Canned {
    let mut records = Vec::new();
    let mut uses = Vec::new();
    let mut texts = BTreeMap::new();
    let mut cited = Vec::new();
    let mut needs = Vec::new();
    let mut paragraphs = Vec::new();
    for need in 1..=8u8 {
        let mut evidence = Vec::new();
        let mut listed = Vec::new();
        let records_for = if need == 8 { 0 } else { 5 };
        for k in 0..records_for {
            let index = records.len();
            let mut spans = Vec::new();
            for hit in 0..2 {
                let path = format!("src/need{need}/file{k}.rs");
                let start = 10 + hit * 40;
                let body = text(&path, start, 24, width);
                let end = start + 23;
                let at = format!("{path}:{start}-{end}");
                texts.insert(at.clone(), body.clone());
                if hit == 0 {
                    listed.push(at.clone());
                }
                spans.push(
                    json!({"pointer":format!("/hits/{hit}"),"path":path,"start_line":start,
                    "end_line":end,"content_hash":"ab".repeat(32),"text":body,"truncated":false}),
                );
            }
            records.push(LoopEvidence {
                id: id(index),
                operation: LoopRead::Search,
                arguments: [("query".to_owned(), json!(format!("need {need} query {k}")))]
                    .into_iter()
                    .collect(),
                role: if k == 0 {
                    EvidenceRole::Definition
                } else {
                    EvidenceRole::Unknown
                },
                canonical_result: json!({"projection":"airk.arbiter.loop-evidence/1",
                    "revision":token().to_string(),"full_bytes":9000,"full_digest":"cd".repeat(32),
                    "hits_total":2,"spans":spans}),
            });
            uses.push(RecordUse {
                record: id(index),
                part: 1,
                need: Some(need),
                round: 1,
                cause: if k == 0 {
                    UseCause::NeedSearch
                } else {
                    UseCause::StructuralRead
                },
            });
            evidence.push(id(index));
        }
        if need == 8 {
            // A search for the last need that found nothing.
            let index = records.len();
            records.push(LoopEvidence {
                id: id(index),
                operation: LoopRead::Search,
                arguments: [("query".to_owned(), json!("billing ledger"))]
                    .into_iter()
                    .collect(),
                role: EvidenceRole::Unknown,
                canonical_result: json!({"projection":"airk.arbiter.loop-evidence/1",
                    "revision":token().to_string(),"full_bytes":120,"full_digest":"cd".repeat(32),
                    "hits_total":0,"spans":[],"rows_total":0,"rows":[]}),
            });
            uses.push(RecordUse {
                record: id(index),
                part: 1,
                need: Some(8),
                round: 1,
                cause: UseCause::NeedSearch,
            });
        } else {
            let mut paragraph = format!(
                "Part 1 sub-need {need} (definition, covered): {}",
                NEED_TEXTS[usize::from(need - 1)]
            );
            for at in &listed {
                paragraph.push_str(&format!("\n- {at} (round 1)"));
            }
            paragraphs.push(GroundedParagraph {
                text: paragraph,
                evidence: evidence.clone(),
                interpretation: true,
            });
        }
        needs.push(SubNeed {
            id: need,
            text: NEED_TEXTS[usize::from(need - 1)].into(),
            kind: SubNeedKind::Definition,
            coverage: match need {
                8 => Coverage::Missing,
                4 => Coverage::Partial,
                _ => Coverage::Covered,
            },
            evidence,
            evidence_omitted: 0,
            unrun_reads: if need == 2 {
                vec![UnrunRead {
                    operation: LoopRead::References,
                    target: "deliver".into(),
                    reason: UnrunReason::NotChosen,
                }]
            } else {
                vec![]
            },
            origin: if need >= 5 {
                SubNeedOrigin::Standing
            } else {
                SubNeedOrigin::Caller
            },
        });
        cited.push(listed);
    }
    // One listing record names a file no location opens.
    let index = records.len();
    records.push(LoopEvidence {
        id: id(index),
        operation: LoopRead::Symbols,
        arguments: [("name".to_owned(), json!("Ledger"))].into_iter().collect(),
        role: EvidenceRole::Unknown,
        canonical_result: json!({"projection":"airk.arbiter.loop-evidence/1",
            "revision":token().to_string(),"full_bytes":300,"full_digest":"cd".repeat(32),
            "hits_total":0,"spans":[],"rows_total":1,
            "rows":[{"path":"src/ledger.rs","start_line":4,"end_line":30,"text":"pub struct Ledger"}]}),
    });
    uses.push(RecordUse {
        record: id(index),
        part: 1,
        need: None,
        round: 1,
        cause: UseCause::PartSearch,
    });
    let reply = ContextResponseV3 {
        schema: CONTEXT_SCHEMA_V3.into(),
        packet_schema: PACKET_SCHEMA_V2.into(),
        question: "Change how the reply frame is packed so partial replies keep their passages"
            .into(),
        scope: scope(),
        parts: vec![AnswerPart {
            id: 1,
            span: QuestionSpan {
                utf8_start: 0,
                utf8_end: 10,
            },
            deadline_share_ms: 30_000,
            sub_needs: needs,
            gaps: vec![Gap::Test],
            rounds: vec![],
            stop: PartStop::Rounds,
            shortened: false,
        }],
        records,
        omitted_evidence: vec![],
        record_uses: uses,
        paragraphs,
        union: UnionSummary::default(),
        partial: None,
        ticket: None,
        degradations: vec![v3::Degradation::ContextAbsent],
        decisions: vec![],
        decision_log: log(),
        work: Work::default(),
        model_work: LoopModelWork::default(),
        elapsed_ms: 8_400,
        candidate_count: 0,
        omitted_count: 0,
        applications: vec![],
        read_status: None,
        gathering: Some(GatheringReport {
            rules: vec![],
            stages: vec![GatheringStage {
                stage: GatheringStageKind::Retrieve,
                reads: vec![GatheringReads {
                    operation: LoopRead::Search,
                    count: 37,
                }],
                lookups: 3,
                passages_fetched: 70,
                passages_offered: 90,
                passages_delivered: 35,
                model_judgments: 0,
                elapsed_ms: 8_100,
            }],
        }),
    };
    Canned {
        reply,
        texts,
        cited,
    }
}

fn log() -> DecisionLogSummary {
    DecisionLogSummary {
        digest: empty_log_digest(),
        total: 0,
        included: 0,
        omitted: 0,
        card_calls: 0,
        counts: vec![],
        focus_calls: 0,
        focus_pairs: 0,
        attribution: vec![],
        timings: LoopTimings::default(),
        work_unknown: 0,
    }
}

fn rendered(reply: &ContextResponseV3) -> Value {
    serde_json::to_value(reply).unwrap()
}

fn brief(value: &Value) -> (BriefReply, String) {
    let brief = brief_of(value).expect("an Answer /3 reply has a brief");
    let text = serde_json::to_string(&brief).unwrap();
    (brief, text)
}

#[test]
fn eight_needs_and_forty_records_fit_a_small_map_with_whole_passages() {
    let canned = canned(70);
    let full = rendered(&canned.reply);
    assert!(
        serde_json::to_vec(&full).unwrap().len() > 100_000,
        "the canned reply is realistic in size"
    );
    let (brief, text) = brief(&full);
    assert!(text.len() <= MAX_BRIEF_BYTES, "{} bytes", text.len());
    assert_eq!(brief.schema, "airk.gather-context.brief/1");
    // Every sub-need, in order, with its status and origin.
    let texts: Vec<&str> = brief.needs.iter().map(|need| need.text.as_str()).collect();
    assert_eq!(texts, NEED_TEXTS);
    let numbers: Vec<usize> = brief.needs.iter().map(|need| need.need).collect();
    assert_eq!(numbers, (1..=8).collect::<Vec<_>>());
    assert_eq!(brief.needs[0].status, BriefStatus::Covered);
    assert_eq!(brief.needs[3].status, BriefStatus::PartlyCovered);
    assert_eq!(brief.needs[7].status, BriefStatus::Open);
    assert_eq!(brief.needs[0].origin, BriefOrigin::Caller);
    assert_eq!(brief.needs[5].origin, BriefOrigin::Standing);
    // Locations are the paragraph's, best first, at most four.
    for (need, cited) in brief.needs.iter().zip(&canned.cited) {
        let at: Vec<&str> = need.locations.iter().map(|row| row.at.as_str()).collect();
        let expected: Vec<&str> = cited
            .iter()
            .take(WHERE_PER_NEED)
            .map(String::as_str)
            .collect();
        assert_eq!(at, expected, "need {}", need.need);
    }
    assert!(
        brief.needs[0].locations[0]
            .why
            .ends_with("search \"need 1 query 0\", need search, definition"),
        "{:?}",
        brief.needs[0].locations[0]
    );
    assert!(
        brief.needs[0].locations[0]
            .why
            .starts_with("let src_need1_file0_rs_10 = ")
    );
    assert!(
        brief.needs[0].locations[1]
            .why
            .ends_with(", structure read")
    );
    // Each need with a passage has its best one before any need gets two.
    for (rank, passage) in brief.passages.iter().take(7).enumerate() {
        assert_eq!(passage.needs, vec![rank + 1]);
        assert_eq!(passage.at, canned.cited[rank][0]);
    }
    let mut size = 0;
    for passage in &brief.passages {
        // Byte for byte the kernel's text for those lines, never cut.
        assert_eq!(Some(&passage.text), canned.texts.get(&passage.at));
        assert!(!passage.clipped);
        size += passage.text.len();
    }
    assert!(size <= BRIEF_PASSAGE_BYTES);
    assert!(brief.passages.len() > 7, "room for second passages");
    // Every cited location without its text here is listed as omitted.
    let carried: BTreeSet<&str> = brief.passages.iter().map(|row| row.at.as_str()).collect();
    let mut expected = Vec::new();
    for cited in &canned.cited {
        for at in cited {
            if !carried.contains(at.as_str()) && !expected.contains(at) {
                expected.push(at.clone());
            }
        }
    }
    assert_eq!(brief.omitted, expected);
    assert_eq!(brief.omitted_more, None);
    // The publication token, once.
    let token = token().to_string();
    assert_eq!(text.matches(&token).count(), 1, "{text}");
    assert_eq!(brief.publication.token, token);
    assert_eq!(brief.result, BriefResult::Complete);
    assert_eq!(brief.stop, "rounds");
    assert_eq!(brief.full, FULL_NOTE);
    assert_eq!(brief.degradations, vec!["context_absent".to_owned()]);
    assert_eq!(brief.next.names, vec!["references deliver".to_owned()]);
    assert_eq!(brief.next.paths, vec!["src/ledger.rs".to_owned()]);
    assert_eq!(brief.next.gaps, vec!["a test".to_owned()]);
    assert!(brief.next.ask.contains("known ranges"));
    assert_eq!(
        brief.gathering,
        vec!["retrieve: 37 reads, 35 delivered, 8100 ms".to_owned()]
    );
    assert_eq!(brief.elapsed_ms, 8_400);
    // Nothing of the full reply's bulk.
    for field in [
        "records",
        "record_uses",
        "decision_log",
        "applications",
        "paragraphs",
    ] {
        assert!(!text.contains(&format!("\"{field}\"")), "{field}");
    }
}

#[test]
fn a_need_with_no_evidence_is_open_and_says_what_was_searched() {
    let canned = canned(70);
    let (brief, _) = brief(&rendered(&canned.reply));
    let last = &brief.needs[7];
    assert_eq!(last.status, BriefStatus::Open);
    assert!(last.locations.is_empty());
    assert_eq!(
        brief.not_found,
        vec![
            "need 8: nothing found for \"where the agent's billing ledger is written\"; searched: search \"billing ledger\", need search, no hits"
                .to_owned()
        ]
    );
    assert!(
        brief
            .passages
            .iter()
            .all(|passage| !passage.needs.contains(&8))
    );
}

#[test]
fn full_mode_and_other_tools_render_exactly_as_before() {
    use airk_core::usage::UsageOperation;
    let canned = canned(70);
    let original = rendered(&canned.reply);
    for (mode, operation) in [
        (ResponseMode::Full, UsageOperation::Answer),
        (ResponseMode::Full, UsageOperation::Search),
        (ResponseMode::Terse, UsageOperation::Search),
    ] {
        let mut before = original.clone();
        crate::response::Session::default().display(&mut before, mode, false);
        let mut after = original.clone();
        crate::response::Session::default().display_for(&mut after, mode, false, operation);
        assert_eq!(
            serde_json::to_vec(&after).unwrap(),
            serde_json::to_vec(&before).unwrap(),
            "{mode:?} {operation:?}"
        );
    }
    let mut terse = original.clone();
    crate::response::Session::default().display_for(
        &mut terse,
        ResponseMode::Terse,
        false,
        UsageOperation::Answer,
    );
    assert_eq!(terse["schema"], BRIEF_SCHEMA);
    // A value that is no Answer /3 reply stays as it is.
    let mut other = json!({"schema":"airk.arbiter.context/2","records":[]});
    let copy = other.clone();
    project(&mut other);
    assert_eq!(other, copy);
}

#[test]
fn a_reply_too_large_for_one_passage_keeps_the_map_and_omits_every_location() {
    let canned = canned(800);
    assert!(
        canned
            .texts
            .values()
            .all(|text| text.len() > BRIEF_PASSAGE_BYTES)
    );
    let (brief, text) = brief(&rendered(&canned.reply));
    assert!(text.len() <= MAX_BRIEF_BYTES, "{} bytes", text.len());
    assert!(brief.passages.is_empty());
    let mut every = Vec::new();
    for cited in &canned.cited {
        every.extend(cited.iter().cloned());
    }
    assert_eq!(brief.omitted, every);
    for (need, cited) in brief.needs.iter().zip(&canned.cited) {
        assert_eq!(need.locations.len(), cited.len().min(WHERE_PER_NEED));
    }
}

#[test]
fn the_brief_is_deterministic() {
    let canned = canned(70);
    let value = rendered(&canned.reply);
    let first = serde_json::to_vec(&brief_of(&value).unwrap()).unwrap();
    let second = serde_json::to_vec(&brief_of(&value.clone()).unwrap()).unwrap();
    assert_eq!(first, second);
}

/// A repeat names its carrier; a trimmed overlap holds no whole lines; a
/// cut text keeps its last whole line.
#[test]
fn repeats_overlaps_and_cut_texts_follow_the_kernel_rows() {
    let mut canned = canned(30);
    let carrier = id(0).to_string();
    let spans = canned.reply.records[1].canonical_result["spans"]
        .as_array_mut()
        .unwrap();
    // Need 1's second location repeats need 1's first.
    spans[0] = json!({"pointer":"/hits/0","path":"src/need1/file1.rs","start_line":10,"end_line":33,
        "content_hash":"ab".repeat(32),"same_as":{"record":carrier,"pointer":"/hits/0"},"truncated":false});
    let spans = canned.reply.records[2].canonical_result["spans"]
        .as_array_mut()
        .unwrap();
    spans[0]["overlaps"] = json!([{"record":carrier,"pointer":"/hits/1","bytes":40,"side":"head"}]);
    let whole = canned.texts["src/need1/file3.rs:10-33"].clone();
    let lines: Vec<&str> = whole.split_inclusive('\n').collect();
    let partial = format!("{}{}", lines[..5].concat(), &lines[5][..10]);
    let spans = canned.reply.records[3].canonical_result["spans"]
        .as_array_mut()
        .unwrap();
    spans[0]["text"] = json!(partial);
    spans[0]["truncated"] = json!(true);
    let (brief, _) = brief(&rendered(&canned.reply));
    let at: Vec<&str> = brief.passages.iter().map(|row| row.at.as_str()).collect();
    // The repeat is need 1's first passage, sent once.
    assert_eq!(
        at.iter()
            .filter(|at| **at == "src/need1/file0.rs:10-33")
            .count(),
        1
    );
    assert!(!at.contains(&"src/need1/file1.rs:10-33"));
    assert!(!at.contains(&"src/need1/file2.rs:10-33"));
    assert!(
        brief
            .omitted
            .contains(&"src/need1/file2.rs:10-33".to_owned())
    );
    let clipped = brief
        .passages
        .iter()
        .find(|row| row.at == "src/need1/file3.rs:10-14")
        .expect("the cut text keeps five whole lines");
    assert!(clipped.clipped);
    assert_eq!(clipped.text, lines[..5].concat());
}

#[test]
fn a_ticket_reply_says_how_to_come_back() {
    let mut canned = canned(70);
    let reply = &mut canned.reply;
    reply.parts.clear();
    reply.records.clear();
    reply.record_uses.clear();
    reply.paragraphs.clear();
    reply.gathering = None;
    reply.partial = Some(PartialMarker {
        stage: LoopStage::Admission,
        reason: PartialReason::Queued,
        part: None,
        continuation: None,
    });
    reply.ticket = Some(AnswerTicket {
        id: Sha256Digest::from_bytes([12; 32]),
        queue_position: 2,
        estimated_start_ms: 1_000,
        estimated_finish_ms: 9_000,
        retry_after_ms: 1_000,
    });
    let (brief, _) = brief(&rendered(reply));
    assert_eq!(brief.result, BriefResult::Partial);
    assert_eq!(brief.why.as_deref(), Some("queued at admission"));
    assert_eq!(brief.stop, "queued");
    assert_eq!(brief.ticket.as_ref().unwrap()["retry_after_ms"], 1_000);
    assert!(brief.next.ask.contains("retry_after_ms"));
    assert!(brief.needs.is_empty());
}

/// Status, request identity and a required-verification error are carried;
/// the verification receipt becomes one line.
#[test]
fn transport_fields_are_carried_and_verification_is_one_line() {
    let canned = canned(70);
    let mut value = rendered(&canned.reply);
    value["request_id"] = json!("request-1");
    value["freshness"] = json!({"state":"current"});
    value["guidance"] = json!({"version":1});
    value["source_verification"] = json!({"schema":"airk.saved-source-verification/1",
        "status":"report_complete","counts":{"checked_sources":8,"matched_sources":7,
        "mismatched_sources":1,"unchecked_sources":2,"requested_sources":10},
        "sources":[{"path":"src/need1/file0.rs","state":"mismatch"},{"path":"src/a.rs","state":"matched"}]});
    let (brief, text) = brief(&value);
    assert_eq!(brief.carried["request_id"], "request-1");
    assert_eq!(brief.carried["freshness"]["state"], "current");
    assert!(!text.contains("\"guidance\""));
    assert!(!text.contains("\"source_verification\""));
    assert_eq!(
        brief.verification.as_deref(),
        Some(
            "report_complete: 8 sources checked, 7 matched, 1 differ, 2 not checked (differ: src/need1/file0.rs)"
        )
    );
}
