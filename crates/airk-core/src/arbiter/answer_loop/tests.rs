// SPDX-License-Identifier: Apache-2.0
//! Literal expectations for the Answer /3 contract, written from plan section
//! 12 and the recorded L1 decisions, never derived from the implementation.
use super::*;
use crate::arbiter::context_action::{EvidenceRef, SourceUnitKind};
use crate::arbiter::device::DevicePrecision;
use crate::arbiter::stage_tests::{current_profile, digest, scope};
use serde_json::{Value, json};

const QUESTION: &str = "Where is the restart guard defined? Which README section covers search?";

fn budget() -> BudgetV3 {
    BudgetV3::default()
}
fn first_end() -> usize {
    QUESTION.find('?').unwrap() + 1
}

#[test]
fn default_budget_is_the_literal_recorded_decision() -> Result<()> {
    let budget = budget();
    budget.validate()?;
    assert_eq!(
        serde_json::to_value(&budget)?,
        json!({
            "deadline_ms":300000,"max_bytes":262144,"max_parts":4,"max_sub_needs":4,
            "rounds":3,"calls":132,"fanout":8,"candidates":132,"input_tokens":380000,
            "round_input_tokens":32768,"context_calls":48,"focus_calls":24,"focus_pairs":192
        })
    );
    assert_eq!(MAX_RESPONSE_BYTES, 65_536, "other tools and Answer /2");
    assert_eq!(
        MAX_ANSWER_RESPONSE_BYTES, 131_072,
        "Answer /3, user 22 November"
    );
    assert_eq!(reply_limit(&budget), 131_072);
    let mut small = budget.clone();
    small.max_bytes = 50_000;
    assert_eq!(reply_limit(&small), 50_000);
    assert_eq!(MAX_DECISIONS, 101);
    // The user's decision of 23 November 2026 (change-log entry 50): the
    // default pool is the maximum, 380,000 tokens.
    assert_eq!(DEFAULT_INPUT_TOKENS, 380_000);
    assert_eq!(MAX_INPUT_TOKENS, 380_000);
    // Four parts share it equally: 95,000 a part, above the about 84,900 a
    // part spends over three rounds with evidence-carrying card calls, and
    // the pool needs no particular multiple (the round allowance is a cap,
    // not a divisor).
    assert_eq!(budget.input_tokens / budget.max_parts, 95_000);
    assert_eq!(budget.input_tokens % budget.max_parts, 0);
    let mut over = budget.clone();
    over.input_tokens = 380_001;
    assert!(over.validate().is_err(), "a pool above 380,000 is refused");
    Ok(())
}

/// (field, accepted maximum) pairs from the recorded decision.
const MAXIMA: [(&str, u64); 13] = [
    ("deadline_ms", 600_000),
    ("max_bytes", 524_288),
    ("max_parts", 4),
    ("max_sub_needs", 4),
    ("rounds", 3),
    ("calls", 160),
    ("fanout", 12),
    ("candidates", 160),
    ("input_tokens", 380_000),
    ("round_input_tokens", 32_768),
    ("context_calls", 64),
    ("focus_calls", 48),
    ("focus_pairs", 384),
];

fn budget_with(field: &str, value: u64) -> Result<BudgetV3> {
    let mut raw = serde_json::to_value(budget())?;
    raw[field] = value.into();
    // Keep the round allowance inside the pool so only the tested bound moves.
    if field == "input_tokens" && value < 32_768 {
        raw["round_input_tokens"] = value.into();
    }
    Ok(serde_json::from_value(raw)?)
}

fn budget_cases() -> Result<Vec<Value>> {
    let mut cases = Vec::new();
    for (field, maximum) in MAXIMA {
        let minimum = if field == "max_bytes" { 1024 } else { 1 };
        for (id, value, accepted) in [
            ("maximum", maximum, true),
            ("over_maximum", maximum + 1, false),
            // One Focus pair cannot carry the default 24 Focus calls.
            ("minimum", minimum, field != "focus_pairs"),
            ("under_minimum", minimum - 1, false),
        ] {
            let candidate = budget_with(field, value)?;
            assert_eq!(
                candidate.validate().is_ok(),
                accepted,
                "{field} {id} {value}"
            );
            cases
                .push(json!({"id":format!("{field}_{id}"),"budget":candidate,"accepted":accepted}));
        }
    }
    let mut over_pool = budget();
    over_pool.input_tokens = 16_384;
    assert!(over_pool.validate().is_err(), "round allowance above pool");
    cases.push(json!({"id":"round_allowance_above_pool","budget":over_pool,"accepted":false}));
    Ok(cases)
}

#[test]
fn budget_bounds_accept_each_maximum_and_refuse_beyond() -> Result<()> {
    assert_eq!(budget_cases()?.len(), 53);
    Ok(())
}

fn request() -> ContextRequestV3 {
    ContextRequestV3 {
        needs: Vec::new(),
        anchors: Vec::new(),
        deliverable: None,
        known: Vec::new(),
        schema: "airk.arbiter.context.request/3".into(),
        question: QUESTION.into(),
        budget: budget(),
        continuation: None,
    }
}

#[test]
fn request_keeps_the_question_bound_and_optional_continuation() -> Result<()> {
    let mut request = request();
    request.validate()?;
    assert!(
        !serde_json::to_string(&request)?.contains("continuation"),
        "absent continuation is omitted"
    );
    request.continuation = Some(digest(90));
    request.validate()?;
    let parsed: ContextRequestV3 = serde_json::from_value(serde_json::to_value(&request)?)?;
    assert_eq!(parsed.continuation, Some(digest(90)));
    for bad in ["", "   ", &"x".repeat(16_385)] {
        let mut wrong = request.clone();
        wrong.question = bad.into();
        assert!(wrong.validate().is_err());
    }
    let mut wrong = request.clone();
    wrong.question = "x".repeat(16_384);
    wrong.validate()?;
    wrong.schema = "airk.arbiter.context.request/2".into();
    assert!(wrong.validate().is_err());
    let mut raw = serde_json::to_value(&request)?;
    raw["unknown"] = true.into();
    assert!(serde_json::from_value::<ContextRequestV3>(raw).is_err());
    Ok(())
}

#[test]
fn a_request_without_caller_inputs_keeps_its_bytes() -> Result<()> {
    // The bytes a request had before the optional caller inputs existed.
    let text = serde_json::to_string(&request())?;
    let raw: serde_json::Value = serde_json::from_str(&text)?;
    let keys: Vec<&str> = raw
        .as_object()
        .expect("request object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["budget", "question", "schema"]);
    let built = ContextRequestV3::new(QUESTION, budget());
    assert_eq!(serde_json::to_string(&built)?, text);
    let parsed: ContextRequestV3 = serde_json::from_str(&text)?;
    assert!(parsed.needs.is_empty() && parsed.anchors.is_empty());
    assert!(parsed.deliverable.is_none() && parsed.known.is_empty());
    Ok(())
}

#[test]
fn a_gathering_report_accounts_for_every_call_and_names_each_stage_once() -> Result<()> {
    let stage = |kind, count, lookups| GatheringStage {
        stage: kind,
        reads: vec![GatheringReads {
            operation: LoopRead::Search,
            count,
        }],
        lookups,
        passages_fetched: 3,
        passages_offered: 3,
        passages_delivered: 1,
        model_judgments: 0,
        elapsed_ms: 100,
        skipped: None,
    };
    // The valid reply makes six calls: four searches and two lookups.
    let mut packet = response();
    packet.gathering = Some(GatheringReport {
        rules: vec![GatheringRule::SearchLanes, GatheringRule::DeliveryByNeed],
        stages: vec![
            stage(GatheringStageKind::Retrieve, 3, 0),
            stage(GatheringStageKind::Structure, 1, 2),
        ],
    });
    packet.validate(&budget())?;
    let raw = serde_json::to_value(&packet)?;
    assert_eq!(
        raw["gathering"]["rules"],
        serde_json::json!(["search_lanes", "delivery_by_need"])
    );
    assert_eq!(raw["gathering"]["stages"][1]["stage"], "structure");
    // Absent, it is omitted, so a reply without it keeps its bytes.
    assert!(serde_json::to_value(response())?.get("gathering").is_none());
    let refuse = |change: &dyn Fn(&mut GatheringReport), error: &str| {
        let mut wrong = packet.clone();
        change(wrong.gathering.as_mut().unwrap());
        let found = wrong.validate(&budget()).unwrap_err().to_string();
        assert!(found.contains(error), "{found}");
    };
    refuse(
        &|report| report.stages[1].stage = GatheringStageKind::Retrieve,
        "names a rule or stage twice",
    );
    refuse(
        &|report| report.rules.push(GatheringRule::SearchLanes),
        "names a rule or stage twice",
    );
    refuse(
        &|report| report.stages[0].reads[0].count = 0,
        "not one count per operation",
    );
    // The stages are spans of one run: together at most the 300,000 ms
    // deadline and its 1,000 ms tolerance, whatever the reply's own wait.
    let mut attached = packet.clone();
    attached.elapsed_ms = 5;
    attached.validate(&budget())?;
    refuse(
        &|report| report.stages[0].elapsed_ms = 300_901,
        "outlast the deadline and its tolerance",
    );
    refuse(
        &|report| report.stages[1].lookups = 3,
        "differ from the reply's calls",
    );
    // A stage that admitted no read may say why; the reason is omitted
    // when absent.
    assert!(raw["gathering"]["stages"][0].get("skipped").is_none());
    let mut skipped = packet.clone();
    skipped
        .gathering
        .as_mut()
        .unwrap()
        .stages
        .push(GatheringStage {
            stage: GatheringStageKind::Guidance,
            reads: vec![],
            lookups: 0,
            passages_fetched: 0,
            passages_offered: 0,
            passages_delivered: 0,
            model_judgments: 0,
            elapsed_ms: 0,
            skipped: Some(GatheringSkip::RuleOff),
        });
    skipped.validate(&budget())?;
    assert_eq!(
        serde_json::to_value(&skipped)?["gathering"]["stages"][2]["skipped"],
        "rule_off"
    );
    let refuse_skipped = |change: &dyn Fn(&mut GatheringReport), error: &str| {
        let mut wrong = skipped.clone();
        change(wrong.gathering.as_mut().unwrap());
        let found = wrong.validate(&budget()).unwrap_err().to_string();
        assert!(found.contains(error), "{found}");
    };
    // A skipped stage made no read.
    refuse_skipped(
        &|report| report.stages[1].skipped = Some(GatheringSkip::NothingFound),
        "skipped but read",
    );
    // A stage whose rule is off, or that never began, made no lookup.
    refuse_skipped(
        &|report| {
            report.stages[2].lookups = 1;
            report.stages[1].lookups = 1;
        },
        "did not run made lookups",
    );
    // Its rule is named off only when it is off.
    refuse_skipped(
        &|report| report.rules.push(GatheringRule::ProjectGuidance),
        "names its rule off, but it is on",
    );
    refuse_skipped(
        &|report| report.stages[2].stage = GatheringStageKind::Coverage,
        "names its rule off, but it is on",
    );
    Ok(())
}

#[test]
fn caller_inputs_round_trip_and_keep_their_bounds() -> Result<()> {
    let mut full = request();
    full.needs = (0..8).map(|n| format!("need {n}")).collect();
    full.anchors = (0..16).map(|n| format!("ANCHOR_{n}")).collect();
    full.deliverable = Some(Deliverable::CodeChange);
    full.known = vec![
        KnownRange {
            path: "src/a.rs".into(),
            start_line: None,
            end_line: None,
        },
        KnownRange {
            path: "src/b.rs".into(),
            start_line: Some(3),
            end_line: Some(3),
        },
    ];
    full.validate()?;
    let raw = serde_json::to_value(&full)?;
    assert_eq!(raw["deliverable"], "code_change");
    assert_eq!(raw["known"][0], serde_json::json!({"path":"src/a.rs"}));
    assert_eq!(
        raw["known"][1],
        serde_json::json!({"path":"src/b.rs","start_line":3,"end_line":3})
    );
    let parsed: ContextRequestV3 = serde_json::from_value(raw)?;
    assert_eq!(parsed.needs.len(), 8);
    assert_eq!(parsed.anchors.len(), 16);

    let refuse = |change: &dyn Fn(&mut ContextRequestV3)| {
        let mut wrong = full.clone();
        change(&mut wrong);
        assert!(wrong.validate().is_err());
    };
    refuse(&|r| r.needs.push("a ninth need".into()));
    refuse(&|r| r.needs[0] = "x".repeat(401));
    refuse(&|r| r.needs[0] = "   ".into());
    refuse(&|r| r.needs[0] = "line\nbreak".into());
    refuse(&|r| r.needs[1] = r.needs[0].clone());
    refuse(&|r| r.anchors.push("a seventeenth".into()));
    refuse(&|r| r.anchors[0] = "x".repeat(201));
    refuse(&|r| r.anchors[1] = r.anchors[0].clone());
    refuse(&|r| r.known[0].path = "../outside.rs".into());
    refuse(&|r| r.known[1].start_line = Some(0));
    refuse(&|r| r.known[1].end_line = Some(2));
    refuse(&|r| r.known[1].end_line = None);
    refuse(&|r| {
        r.known = (0..33)
            .map(|n| KnownRange {
                path: format!("src/{n}.rs"),
                start_line: None,
                end_line: None,
            })
            .collect()
    });
    let mut edge = full.clone();
    edge.needs[0] = "x".repeat(400);
    edge.anchors[0] = "y".repeat(200);
    edge.validate()?;
    let mut unknown = serde_json::to_value(&full)?;
    unknown["deliverable"] = "essay".into();
    assert!(serde_json::from_value::<ContextRequestV3>(unknown).is_err());
    Ok(())
}

/// The public Answer call (user decision of 25 November 2026): `task` alone
/// gets the request schema and the complete default budget, literal here.
#[test]
fn a_task_alone_becomes_the_canonical_request_with_the_default_budget() -> Result<()> {
    let call: AnswerCallV3 = serde_json::from_value(json!({"task": QUESTION}))?;
    let request = call.into_request()?;
    request.validate()?;
    assert_eq!(
        serde_json::to_value(&request)?,
        json!({
            "schema":"airk.arbiter.context.request/3","question":QUESTION,
            "budget":{"deadline_ms":300000,"max_bytes":262144,"max_parts":4,"max_sub_needs":4,
                "rounds":3,"calls":132,"fanout":8,"candidates":132,"input_tokens":380000,
                "round_input_tokens":32768,"context_calls":48,"focus_calls":24,"focus_pairs":192}
        })
    );
    Ok(())
}

#[test]
fn a_task_keeps_an_explicit_valid_budget_field_by_field() -> Result<()> {
    let sent = json!({"deadline_ms":20000,"max_bytes":65536,"max_parts":1,"max_sub_needs":2,
        "rounds":2,"calls":9,"fanout":3,"candidates":7,"input_tokens":4096,
        "round_input_tokens":2048,"context_calls":5,"focus_calls":2,"focus_pairs":16});
    let call: AnswerCallV3 = serde_json::from_value(json!({"task": QUESTION, "budget": sent}))?;
    let request = call.into_request()?;
    request.validate()?;
    assert_eq!(serde_json::to_value(&request.budget)?, sent);
    assert_eq!(request.schema, "airk.arbiter.context.request/3");
    assert_eq!(request.question, QUESTION);
    Ok(())
}

#[test]
fn the_existing_full_request_and_its_task_call_are_one_canonical_request() -> Result<()> {
    let legacy = json!({
        "schema":"airk.arbiter.context.request/3","question":QUESTION,
        "budget":{"deadline_ms":300000,"max_bytes":262144,"max_parts":4,"max_sub_needs":4,
            "rounds":3,"calls":132,"fanout":8,"candidates":132,"input_tokens":380000,
            "round_input_tokens":32768,"context_calls":48,"focus_calls":24,"focus_pairs":192},
        "needs":["where the guard is defined"],"anchors":["restart guard"]
    });
    // The bytes the existing request had as a ContextRequestV3, before the
    // public call existed.
    let direct: ContextRequestV3 = serde_json::from_value(legacy.clone())?;
    let through_call = serde_json::from_value::<AnswerCallV3>(legacy.clone())?.into_request()?;
    let task = serde_json::from_value::<AnswerCallV3>(json!({
        "task":QUESTION,"needs":["where the guard is defined"],"anchors":["restart guard"]
    }))?
    .into_request()?;
    let bytes = serde_json::to_vec(&direct)?;
    assert_eq!(serde_json::to_vec(&through_call)?, bytes);
    assert_eq!(serde_json::to_vec(&task)?, bytes);
    assert_eq!(serde_json::from_slice::<Value>(&bytes)?, legacy);
    // A task call may also name the schema and the default budget.
    let mut explicit = legacy.clone();
    let question = explicit
        .as_object_mut()
        .unwrap()
        .remove("question")
        .unwrap();
    explicit["task"] = question;
    let explicit = serde_json::from_value::<AnswerCallV3>(explicit)?.into_request()?;
    assert_eq!(serde_json::to_vec(&explicit)?, bytes);
    Ok(())
}

#[test]
fn a_call_with_both_or_neither_of_task_and_question_is_refused() -> Result<()> {
    let refusal = |value: Value| {
        serde_json::from_value::<AnswerCallV3>(value)
            .expect("well-formed call")
            .into_request()
            .unwrap_err()
    };
    let both = refusal(json!({"task":"a","question":"a"}));
    assert_eq!(both, AnswerCallError::TaskAndQuestion);
    assert_eq!(both.code(), "answer_task_and_question");
    assert!(both.to_string().starts_with("answer_task_and_question: "));
    let neither = refusal(json!({"needs":["what owns the guard"]}));
    assert_eq!(neither, AnswerCallError::TaskRequired);
    assert!(neither.to_string().starts_with("answer_task_required: "));
    // The existing full request keeps requiring its schema and budget.
    let missing_schema = refusal(json!({"question":"a","budget":json!(budget())}));
    assert_eq!(missing_schema, AnswerCallError::MissingSchema);
    assert!(
        missing_schema
            .to_string()
            .contains("missing field `schema`")
    );
    let missing_budget = refusal(json!({"question":"a","schema":REQUEST_SCHEMA_V3}));
    assert!(
        missing_budget
            .to_string()
            .contains("missing field `budget`")
    );
    assert!(serde_json::from_value::<AnswerCallV3>(json!({"task":"a","unknown":1})).is_err());
    Ok(())
}

#[test]
fn caller_inputs_pass_through_a_task_call_unchanged() -> Result<()> {
    let inputs = json!({
        "needs":["which function enforces the reply frame","which test covers it"],
        "anchors":["MAX_ANSWER_RESPONSE_BYTES","128 KiB"],
        "deliverable":"test",
        "known":[{"path":"crates/airk-core/src/lib.rs","start_line":80,"end_line":95}]
    });
    let mut call = inputs.clone();
    call["task"] = json!(QUESTION);
    let request = serde_json::from_value::<AnswerCallV3>(call)?.into_request()?;
    request.validate()?;
    let raw = serde_json::to_value(&request)?;
    for field in ["needs", "anchors", "deliverable", "known"] {
        assert_eq!(raw[field], inputs[field], "{field}");
    }
    Ok(())
}

#[test]
fn the_public_call_schema_requires_task_alone() -> Result<()> {
    let schema = serde_json::to_value(schemars::schema_for!(AnswerCallV3))?;
    assert_eq!(schema["required"], json!(["task"]));
    assert_eq!(schema["properties"]["task"]["type"], "string");
    for field in [
        "schema",
        "question",
        "budget",
        "needs",
        "anchors",
        "deliverable",
        "known",
    ] {
        assert!(schema["properties"].get(field).is_some(), "{field}");
    }
    Ok(())
}

#[test]
fn request_refuses_retention_too_small_for_question_and_skeleton() -> Result<()> {
    let mut request = request();
    // 71-byte question + 4,096-byte skeleton = 4,167 bytes.
    request.budget.max_bytes = 4_167;
    request.validate()?;
    request.budget.max_bytes = 4_166;
    let error = request.validate().unwrap_err().to_string();
    assert!(
        error.contains("cannot hold the question and a minimal reply"),
        "{error}"
    );
    Ok(())
}

fn record(id: u8, operation: LoopRead) -> LoopEvidence {
    LoopEvidence {
        id: digest(id),
        operation,
        arguments: BTreeMap::from([("query".into(), json!("restart guard"))]),
        role: EvidenceRole::Definition,
        canonical_result: json!({"hits":[],"future_native_field":{"retained":true}}),
    }
}
fn unit(id: u8) -> EvidenceRef {
    EvidenceRef {
        record: digest(id),
        kind: SourceUnitKind::Hit,
        result_pointer: "/hits/0".into(),
        utf8_start: 0,
        utf8_end: 5,
    }
}
fn chosen(symbol: &str, class: CardClass) -> ChosenCard {
    ChosenCard {
        symbol: symbol.into(),
        class,
    }
}
fn basis(units: &[EvidenceRef]) -> BasisSummary {
    BasisSummary {
        presented: u16::try_from(units.len()).unwrap(),
        omitted: 0,
        digest: basis_digest(units, &[]).unwrap(),
    }
}
fn split_cards() -> Vec<CardRecord> {
    vec![
        CardRecord {
            symbol: "A".into(),
            class: CardClass::SplitAt,
            text_digest: digest(41),
            partition: vec![span(0, first_end()), span(first_end(), QUESTION.len())],
        },
        CardRecord {
            symbol: "B".into(),
            class: CardClass::KeepWhole,
            text_digest: digest(42),
            partition: vec![span(0, QUESTION.len())],
        },
    ]
}
fn follow_up_cards() -> Vec<CardRecord> {
    [
        ("A", CardClass::FollowUpQuery, 48),
        ("B", CardClass::FollowUpHop, 49),
    ]
    .into_iter()
    .map(|(symbol, class, text)| CardRecord {
        symbol: symbol.into(),
        class,
        text_digest: digest(text),
        partition: vec![],
    })
    .collect()
}
fn decisions() -> Vec<DecisionRecord> {
    let split = DecisionRecord {
        ordinal: 1,
        stage: LoopStage::Split,
        part: None,
        need: None,
        round: None,
        roster_digest: roster_digest(LoopStage::Split, &split_cards()).unwrap(),
        card_slot: None,
        chosen: vec![chosen("A", CardClass::SplitAt)],
        source: DecisionSource::Deterministic {
            rule: DeterministicRule::ExplicitBreaks,
        },
        top: None,
        runner_up: None,
        margin: None,
        model: None,
        device: None,
        precision: None,
        batched_with: vec![],
        applicability: AssessmentApplicability::Current,
        receipt: None,
        basis: basis(&[]),
        work_unknown: false,
    };
    let covered = DecisionRecord {
        ordinal: 2,
        stage: LoopStage::Coverage,
        part: Some(1),
        need: Some(1),
        round: Some(1),
        roster_digest: roster_digest(LoopStage::Coverage, &coverage_cards()).unwrap(),
        card_slot: Some(1),
        chosen: vec![chosen("A", CardClass::Covered)],
        source: DecisionSource::Model {
            mode: ModelMode::TopScore,
            rule: None,
        },
        top: Some(2.0),
        runner_up: Some(0.5),
        margin: Some(1.5),
        model: Some(SelectedModel::Arbiter),
        device: Some(ExecutionDevice::Cuda),
        precision: Some(DevicePrecision::Float16),
        batched_with: vec![3],
        applicability: AssessmentApplicability::Current,
        receipt: Some(digest(60)),
        basis: basis(&[unit(32)]),
        work_unknown: false,
    };
    let abstained = DecisionRecord {
        ordinal: 3,
        part: Some(2),
        chosen: vec![chosen("B", CardClass::ReadMoreHere)],
        source: DecisionSource::Abstained {
            fallback: DeterministicRule::TopFocusSpan,
            reason: AssessmentUnavailable::Abstained,
        },
        top: Some(0.2),
        runner_up: Some(0.1),
        margin: None,
        batched_with: vec![2],
        basis: basis(&[unit(31)]),
        ..covered.clone()
    };
    let follow_up = DecisionRecord {
        ordinal: 4,
        stage: LoopStage::FollowUp,
        part: Some(2),
        need: Some(1),
        round: Some(1),
        roster_digest: roster_digest(LoopStage::FollowUp, &follow_up_cards()).unwrap(),
        chosen: vec![chosen("A", CardClass::FollowUpQuery)],
        source: DecisionSource::Deterministic {
            rule: DeterministicRule::FirstExtractedTerm,
        },
        ..split.clone()
    };
    vec![split, covered, abstained, follow_up]
}
fn summary() -> DecisionLogSummary {
    DecisionLogSummary {
        digest: digest(70),
        total: 4,
        included: 4,
        omitted: 0,
        card_calls: 1,
        counts: vec![
            StageSourceCount {
                stage: LoopStage::Split,
                source: SourceKind::Deterministic,
                model: None,
                count: 1,
            },
            StageSourceCount {
                stage: LoopStage::Coverage,
                source: SourceKind::Model,
                model: Some(SelectedModel::Arbiter),
                count: 1,
            },
            StageSourceCount {
                stage: LoopStage::Coverage,
                source: SourceKind::Abstained,
                model: Some(SelectedModel::Arbiter),
                count: 1,
            },
            StageSourceCount {
                stage: LoopStage::FollowUp,
                source: SourceKind::Deterministic,
                model: None,
                count: 1,
            },
        ],
        focus_calls: 1,
        // The logged Focus call dispatched four pairs and scored two: the
        // summary counts what was dispatched.
        focus_pairs: 4,
        attribution: vec![
            TokenAttribution {
                part: 1,
                round: 1,
                tokens: 76,
            },
            TokenAttribution {
                part: 2,
                round: 1,
                tokens: 40,
            },
        ],
        timings: LoopTimings {
            compile_us: Some(4),
            inference_us: Some(6),
            ..LoopTimings::default()
        },
        work_unknown: 0,
    }
}
fn round(
    round: u8,
    calls: usize,
    fanout: usize,
    input_tokens: usize,
    new_evidence: usize,
    follow_ups_run: Vec<FollowUp>,
    covered: usize,
) -> RoundProvenance {
    RoundProvenance {
        round,
        calls,
        fanout,
        input_tokens,
        new_evidence,
        follow_ups_run,
        sub_needs_covered: covered,
    }
}
fn response() -> ContextResponseV3 {
    ContextResponseV3 {
        schema: "airk.arbiter.context/3".into(),
        packet_schema: "airk.arbiter.packet/2".into(),
        question: QUESTION.into(),
        scope: scope(),
        parts: vec![
            AnswerPart {
                id: 1,
                span: QuestionSpan {
                    utf8_start: 0,
                    utf8_end: first_end(),
                },
                deadline_share_ms: 120_000,
                sub_needs: vec![SubNeed {
                    origin: Default::default(),
                    id: 1,
                    text: "where is the restart guard defined".into(),
                    kind: SubNeedKind::Definition,
                    coverage: Coverage::Covered,
                    evidence: vec![digest(32)],
                    evidence_omitted: 0,
                    unrun_reads: vec![],
                }],
                gaps: vec![Gap::Sufficient],
                rounds: vec![round(1, 3, 2, 100, 2, vec![], 1)],
                stop: PartStop::Covered,
                shortened: false,
            },
            AnswerPart {
                id: 2,
                span: QuestionSpan {
                    utf8_start: first_end() + 1,
                    utf8_end: QUESTION.len(),
                },
                deadline_share_ms: 120_000,
                sub_needs: vec![SubNeed {
                    origin: Default::default(),
                    id: 1,
                    text: "README section on search".into(),
                    kind: SubNeedKind::Documentation,
                    coverage: Coverage::Partial,
                    evidence: vec![digest(31)],
                    evidence_omitted: 0,
                    unrun_reads: vec![UnrunRead {
                        operation: LoopRead::DocumentSections,
                        target: "README.md".into(),
                        reason: UnrunReason::NotChosen,
                    }],
                }],
                gaps: vec![Gap::Example],
                rounds: vec![
                    round(1, 2, 2, 40, 1, vec![], 0),
                    round(
                        2,
                        1,
                        1,
                        0,
                        0,
                        vec![FollowUp {
                            operation: LoopRead::Search,
                            target: "README search".into(),
                        }],
                        0,
                    ),
                ],
                stop: PartStop::NoProgress,
                shortened: false,
            },
        ],
        records: vec![record(31, LoopRead::Search), record(32, LoopRead::Symbols)],
        omitted_evidence: vec![],
        record_uses: vec![
            RecordUse {
                record: digest(31),
                part: 1,
                need: None,
                round: 1,
                cause: UseCause::PartSearch,
            },
            RecordUse {
                record: digest(31),
                part: 2,
                need: Some(1),
                round: 1,
                cause: UseCause::NeedSearch,
            },
            RecordUse {
                record: digest(32),
                part: 1,
                need: Some(1),
                round: 1,
                cause: UseCause::StructuralRead,
            },
        ],
        paragraphs: vec![GroundedParagraph {
            text: "fn restart_guard() {}".into(),
            evidence: vec![digest(32)],
            interpretation: false,
        }],
        union: UnionSummary {
            dedup_count: 1,
            ..UnionSummary::default()
        },
        partial: None,
        ticket: None,
        degradations: vec![Degradation::ContextAbstained { count: 1 }],
        decisions: decisions(),
        decision_log: summary(),
        work: Work {
            calls: 6,
            rounds: 2,
            fanout: 2,
            input_tokens: 140,
            candidates: 2,
            bytes: 2048,
            compile_us: Some(4),
            inference_us: Some(6),
            ..Work::default()
        },
        model_work: LoopModelWork {
            context_calls: 1,
            focus_calls: 1,
            focus_pairs: 4,
            focus_profile: Some(digest(10)),
        },
        elapsed_ms: 2_000,
        candidate_count: 2,
        omitted_count: 0,
        applications: vec![IndexApplication {
            scope: scope(),
            profile: Some(digest(10)),
            availability: Availability::Ready,
            reason: "Compact on CUDA".into(),
        }],
        read_status: None,
        gathering: None,
    }
}
fn ticket() -> ContextResponseV3 {
    ContextResponseV3 {
        parts: vec![],
        records: vec![],
        record_uses: vec![],
        paragraphs: vec![],
        union: UnionSummary::default(),
        partial: Some(PartialMarker {
            stage: LoopStage::Admission,
            reason: PartialReason::Queued,
            part: None,
            continuation: None,
        }),
        ticket: Some(AnswerTicket {
            id: digest(80),
            queue_position: 3,
            estimated_start_ms: 90_000,
            estimated_finish_ms: 400_000,
            retry_after_ms: 90_000,
        }),
        degradations: vec![],
        decisions: vec![],
        decision_log: DecisionLogSummary {
            digest: empty_log_digest(),
            total: 0,
            included: 0,
            card_calls: 0,
            counts: vec![],
            focus_calls: 0,
            focus_pairs: 0,
            attribution: vec![],
            timings: LoopTimings::default(),
            ..summary()
        },
        work: Work::default(),
        elapsed_ms: 5,
        model_work: LoopModelWork::default(),
        candidate_count: 0,
        ..response()
    }
}
fn shortened(packet: &mut ContextResponseV3) {
    packet.parts[1].shortened = true;
    packet.omitted_count = 1;
    packet.candidate_count = 3;
    packet.partial = Some(PartialMarker {
        stage: LoopStage::Union,
        reason: PartialReason::Bytes,
        part: Some(2),
        continuation: None,
    });
}

type Mutation = fn(&mut ContextResponseV3);

/// Each mutation breaks exactly one rule; the second value is its error.
fn refusals() -> Vec<(&'static str, Mutation, &'static str)> {
    vec![
        (
            "wrong_packet_schema",
            |r| r.packet_schema = "airk.arbiter.packet/1".into(),
            "invalid Answer response schema",
        ),
        (
            "part_ids_out_of_order",
            |r| r.parts[1].id = 3,
            "noncanonical Answer part id",
        ),
        (
            "overlapping_spans",
            |r| r.parts[1].span.utf8_start = 3,
            "invalid Answer part span",
        ),
        (
            "span_past_question",
            |r| r.parts[1].span.utf8_end = QUESTION.len() + 1,
            "invalid Answer part span",
        ),
        (
            "parts_degraded_below_observed",
            |r| {
                r.degradations.push(Degradation::Parts {
                    requested: 4,
                    applied: 1,
                    reason: DegradationReason::DeadlineEstimate,
                })
            },
            "Answer part limit exceeded",
        ),
        (
            "missing_need_cites_evidence",
            |r| r.parts[0].sub_needs[0].coverage = Coverage::Missing,
            "Answer coverage disagrees with cited evidence",
        ),
        (
            "covered_need_keeps_unrun_reads",
            |r| r.parts[0].sub_needs[0].unrun_reads = r.parts[1].sub_needs[0].unrun_reads.clone(),
            "invalid Answer unrun reads",
        ),
        (
            "unrun_deadline_without_deadline_stop",
            |r| r.parts[1].sub_needs[0].unrun_reads[0].reason = UnrunReason::Deadline,
            "Answer unrun read reason differs from its part's stop",
        ),
        (
            "cites_unknown_evidence",
            |r| r.parts[1].sub_needs[0].evidence = vec![digest(99)],
            "Answer sub-need cites unknown or uncounted evidence",
        ),
        (
            "omitted_citation_uncounted",
            |r| {
                r.omitted_evidence = vec![digest(33)];
                r.omitted_count = 1;
                r.candidate_count = 3;
                r.parts[1].sub_needs[0].evidence.push(digest(33));
            },
            "Answer sub-need cites unknown or uncounted evidence",
        ),
        (
            "sufficient_gap_without_covered",
            |r| r.parts[1].gaps = vec![Gap::Sufficient],
            "Answer part gaps disagree with its stop",
        ),
        (
            "clarification_without_ambiguous",
            |r| r.parts[1].stop = PartStop::ClarificationRequired,
            "Answer part gaps disagree with its stop",
        ),
        (
            "covered_stop_with_partial_need",
            |r| {
                r.parts[1].stop = PartStop::Covered;
                r.parts[1].gaps = vec![Gap::Sufficient];
            },
            "Answer part stop disagrees with its rounds",
        ),
        (
            "no_progress_after_new_evidence",
            |r| r.parts[1].rounds[1].new_evidence = 1,
            "Answer part stop disagrees with its rounds",
        ),
        (
            "follow_up_in_round_one",
            |r| r.parts[0].rounds[0].follow_ups_run = r.parts[1].rounds[1].follow_ups_run.clone(),
            "Answer follow-ups run outside a follow-up round",
        ),
        (
            "round_reads_over_structure",
            // One part search, sixteen anchor lookups, 48 gathering reads and
            // six recipe reads and two lane searches for the one sub-need allow 73.
            |r| r.parts[0].rounds[0].calls = 74,
            "Answer round reads exceed its structural limit",
        ),
        (
            "standing_need_in_a_two_part_reply",
            |r| r.parts[1].sub_needs[0].origin = SubNeedOrigin::Standing,
            "Answer standing sub-needs follow the others, in a one-part reply",
        ),
        (
            "gathering_calls_differ_from_the_work",
            |r| {
                r.gathering = Some(GatheringReport {
                    rules: vec![GatheringRule::SearchLanes],
                    stages: vec![GatheringStage {
                        stage: GatheringStageKind::Retrieve,
                        reads: vec![GatheringReads {
                            operation: LoopRead::Search,
                            count: 5,
                        }],
                        lookups: 0,
                        passages_fetched: 4,
                        passages_offered: 4,
                        passages_delivered: 2,
                        model_judgments: 0,
                        elapsed_ms: 10,
                        skipped: None,
                    }],
                })
            },
            "Answer gathering reads and lookups differ from the reply's calls",
        ),
        (
            "skipped_gathering_stage_read",
            |r| {
                r.gathering = Some(GatheringReport {
                    rules: vec![GatheringRule::SearchLanes],
                    stages: vec![GatheringStage {
                        stage: GatheringStageKind::Guidance,
                        reads: vec![GatheringReads {
                            operation: LoopRead::Context,
                            count: 5,
                        }],
                        lookups: 0,
                        passages_fetched: 1,
                        passages_offered: 1,
                        passages_delivered: 0,
                        model_judgments: 0,
                        elapsed_ms: 10,
                        skipped: Some(GatheringSkip::NothingFound),
                    }],
                })
            },
            "Answer gathering stage is skipped but read",
        ),
        (
            "fanout_over_budget",
            |r| {
                r.parts[0].rounds[0].calls = 7;
                r.parts[0].rounds[0].fanout = 9;
            },
            "Answer round fanout exceeds its reads or budget",
        ),
        (
            "round_tokens_over_allowance",
            |r| r.parts[0].rounds[0].input_tokens = 32_769,
            "Answer round token allowance exceeded",
        ),
        (
            "rounds_degraded_below_observed",
            |r| {
                r.degradations.push(Degradation::Rounds {
                    requested: 3,
                    applied: 1,
                    reason: DegradationReason::DeadlineEstimate,
                })
            },
            "Answer round limit exceeded",
        ),
        (
            "record_use_unknown_round",
            |r| r.record_uses[0].round = 2,
            "Answer record use names an unknown part, sub-need or round",
        ),
        (
            "record_without_provenance",
            |r| r.record_uses.truncate(2),
            "Answer record without provenance",
        ),
        (
            "dedup_count_wrong",
            |r| r.union.dedup_count = 0,
            "Answer dedup count differs",
        ),
        (
            "definition_conflict_single_record",
            |r| {
                r.union.conflicts.push(ConflictNote {
                    kind: ConflictKind::DivergentDefinition,
                    parts: vec![1, 2],
                    evidence: vec![digest(31)],
                    note: "two definitions".into(),
                })
            },
            "invalid Answer conflict note",
        ),
        (
            "conflict_part_without_evidence",
            |r| {
                r.union.conflicts.push(ConflictNote {
                    kind: ConflictKind::DivergentVerdict,
                    parts: vec![1, 2],
                    evidence: vec![digest(32)],
                    note: "judged differently".into(),
                })
            },
            "Answer conflict part found none of its evidence",
        ),
        (
            "gpu_without_precision",
            |r| r.decisions[1].precision = None,
            "Answer GPU decision precision must be stated",
        ),
        (
            "deterministic_with_scores",
            |r| r.decisions[0].top = Some(1.0),
            "deterministic Answer decision carries model facts",
        ),
        (
            "focus_model_scores_cards",
            |r| r.decisions[1].model = Some(SelectedModel::Focus),
            "only the Arbiter scores loop cards",
        ),
        (
            "split_with_location",
            |r| r.decisions[0].part = Some(1),
            "Answer decision location differs from its stage",
        ),
        (
            "class_of_other_stage",
            |r| r.decisions[1].chosen[0].class = CardClass::FollowUpHop,
            "invalid Answer chosen cards",
        ),
        (
            "coverage_chooses_two",
            |r| {
                r.decisions[1]
                    .chosen
                    .push(chosen("B", CardClass::ReadMoreHere))
            },
            "invalid Answer chosen cards",
        ),
        (
            "margin_not_top_minus_runner_up",
            |r| r.decisions[1].margin = Some(1.0),
            "Answer decision margin differs from its top and runner-up",
        ),
        (
            "rule_mode_without_rule",
            |r| {
                r.decisions[1].source = DecisionSource::Model {
                    mode: ModelMode::Rule,
                    rule: None,
                }
            },
            "accepted Answer decision lacks its device, receipt, margin or rule",
        ),
        (
            "abstained_with_other_reason",
            |r| {
                r.decisions[2].source = DecisionSource::Abstained {
                    fallback: DeterministicRule::TopFocusSpan,
                    reason: AssessmentUnavailable::TokenBudget,
                }
            },
            "abstained Answer decision lacks its device, receipt or reason",
        ),
        (
            "slot_over_budget",
            |r| r.decisions[1].card_slot = Some(49),
            "Answer model decision lacks a card slot within its budget",
        ),
        (
            "batch_peer_not_symmetric",
            |r| r.decisions[2].batched_with = vec![],
            "Answer batch peers do not share one card call",
        ),
        (
            "batch_peer_other_slot",
            |r| r.decisions[2].card_slot = Some(2),
            "Answer batch peers do not share one card call",
        ),
        (
            "decision_ordinals_not_increasing",
            |r| r.decisions[2].ordinal = 2,
            "Answer decision ordinals are not increasing within the log",
        ),
        (
            "stale_decision_stays_current",
            |r| {
                r.decisions[2].part = Some(1);
            },
            "only the latest local decision can remain current",
        ),
        (
            "log_counts_disagree",
            |r| r.decision_log.included = 3,
            "decision log summary counts disagree",
        ),
        (
            "log_counts_do_not_sum",
            |r| r.decision_log.counts[0].count = 2,
            "invalid decision log stage and source counts",
        ),
        (
            "decision_of_uncounted_source",
            |r| r.decision_log.counts[1].source = SourceKind::Unavailable,
            "reply decision of an uncounted stage and source",
        ),
        (
            "model_decision_omitted_before_deterministic",
            |r| {
                r.decision_log.total = 5;
                r.decision_log.omitted = 1;
                r.decision_log.counts.push(StageSourceCount {
                    stage: LoopStage::FollowUp,
                    source: SourceKind::Model,
                    model: Some(SelectedModel::Arbiter),
                    count: 1,
                });
            },
            "Union omits deterministic decisions before model-scored ones",
        ),
        (
            "card_calls_over_scored",
            |r| {
                r.decision_log.card_calls = 3;
                r.model_work.context_calls = 3;
            },
            "decision log card calls disagree with scored decisions or slots",
        ),
        (
            "model_work_differs_from_log",
            |r| r.model_work.context_calls = 2,
            "Answer model work differs from the decision log",
        ),
        (
            "focus_pairs_over_budget",
            |r| {
                r.decision_log.focus_pairs = 193;
                r.model_work.focus_pairs = 193;
            },
            "Answer Focus work exceeds its budget: 193 Focus pairs of 192",
        ),
        // Each of the four Focus bounds names itself and its figures.
        (
            "focus_calls_over_budget",
            |r| {
                (r.decision_log.focus_calls, r.decision_log.focus_pairs) = (25, 25);
                (r.model_work.focus_calls, r.model_work.focus_pairs) = (25, 25);
            },
            "Answer Focus work exceeds its budget: 25 Focus calls of 24",
        ),
        (
            "focus_call_without_dispatched_pairs",
            |r| {
                (r.decision_log.focus_calls, r.decision_log.focus_pairs) = (1, 0);
                (r.model_work.focus_calls, r.model_work.focus_pairs) = (1, 0);
            },
            "Answer Focus work exceeds its budget: 1 Focus calls dispatched only 0 pairs",
        ),
        (
            "focus_pairs_over_batch_ceiling",
            |r| {
                (r.decision_log.focus_calls, r.decision_log.focus_pairs) = (1, 33);
                (r.model_work.focus_calls, r.model_work.focus_pairs) = (1, 33);
            },
            "Answer Focus work exceeds its budget: 33 Focus pairs over 1 calls of at most 32",
        ),
        (
            "attribution_not_retained",
            |r| r.decision_log.attribution[1].tokens = 41,
            "attributed round tokens are not retained by the round",
        ),
        (
            "attribution_duplicated",
            |r| {
                r.decision_log.attribution.push(TokenAttribution {
                    part: 1,
                    round: 1,
                    tokens: 1,
                })
            },
            "invalid model token attribution",
        ),
        (
            "abstained_count_wrong",
            |r| r.degradations = vec![Degradation::ContextAbstained { count: 2 }],
            "Answer abstention count differs from the decision log",
        ),
        (
            "context_absent_with_scored",
            |r| r.degradations.push(Degradation::ContextAbsent),
            "absent Context model cannot have scored a card",
        ),
        (
            "focus_absent_with_focus_calls",
            |r| r.degradations.push(Degradation::FocusAbsent),
            "FocusAbsent is disclosed exactly when no Focus call ran",
        ),
        (
            "duplicate_degradation",
            |r| {
                r.degradations
                    .push(Degradation::ContextAbstained { count: 1 })
            },
            "duplicate Answer degradation",
        ),
        (
            "model_degraded_to_another_model",
            |r| {
                r.degradations.push(Degradation::Model {
                    stage: Stage::Context,
                    requested: SelectedModel::Arbiter,
                    applied: Some(SelectedModel::Focus),
                    reason: DegradationReason::MemoryUnavailable,
                });
            },
            "invalid Answer model degradation",
        ),
        (
            "model_degraded_yet_decides",
            |r| {
                r.degradations.push(Degradation::Model {
                    stage: Stage::Context,
                    requested: SelectedModel::Arbiter,
                    applied: None,
                    reason: DegradationReason::MemoryUnavailable,
                });
            },
            "degraded Answer model still decided",
        ),
        (
            "unknown_outcome_without_unknown_work",
            |r| r.degradations.push(Degradation::ModelOutcomeUnknown),
            "unknown model outcome without unknown logged work",
        ),
        (
            "deadline_stop_without_partial",
            |r| r.parts[1].stop = PartStop::Deadline,
            "an unfinished Answer must be marked partial",
        ),
        (
            "omission_without_shortened_part",
            |r| {
                shortened(r);
                r.parts[1].shortened = false;
            },
            "Answer shortened parts disagree with omissions",
        ),
        (
            "queued_without_ticket",
            |r| {
                r.partial = Some(PartialMarker {
                    stage: LoopStage::Coverage,
                    reason: PartialReason::Queued,
                    part: None,
                    continuation: None,
                })
            },
            "Answer partial reason is not observed",
        ),
        (
            "application_revision_changed_undisclosed",
            |r| r.applications[0].scope.configuration_revision = 2,
            "Answer application scope mismatch",
        ),
        (
            "search_carries_focus_receipt",
            |r| {
                r.records[0].canonical_result = crate::arbiter::focus::tests::search_record(scope())
                    .unwrap()
                    .canonical_result
            },
            "Answer loop searches defer in-search Focus",
        ),
        (
            "duplicate_record",
            |r| r.records[1].id = digest(31),
            "Answer evidence is not one exact deduplicated partition",
        ),
        (
            "elapsed_over_deadline",
            |r| r.elapsed_ms = 301_001,
            "Answer elapsed time exceeds its deadline and tolerance",
        ),
        (
            "count_without_model",
            |r| r.decision_log.counts[1].model = None,
            "invalid decision log stage and source counts",
        ),
        (
            "empty_digest_on_nonempty_log",
            |r| r.decision_log.digest = empty_log_digest(),
            "only an empty decision log has the empty log digest",
        ),
        (
            "ticket_with_parts",
            |r| r.ticket = ticket().ticket,
            "a ticket reply carries no work and is marked queued",
        ),
        (
            "work_calls_differ_from_rounds",
            |r| r.work.calls = 5,
            "Answer work differs from its rounds",
        ),
        (
            "work_tokens_not_retained",
            |r| r.work.input_tokens = 139,
            "Answer work does not retain its round, Split, Plan and Focus tokens",
        ),
        (
            "timing_not_retained",
            |r| r.work.inference_us = Some(5),
            "Answer measured model timing not retained",
        ),
        (
            "work_tokens_over_pool",
            |r| r.work.input_tokens = 380_001,
            "Answer original work budget exceeded",
        ),
        (
            "candidates_exceed_reads",
            |r| {
                shortened(r);
                r.omitted_count = 5;
                r.candidate_count = 7;
            },
            "Answer original work budget exceeded",
        ),
        (
            "reply_over_frame",
            |r| r.paragraphs[0].text = "x".repeat(140_000),
            "Answer response exceeds the reply frame",
        ),
    ]
}

/// Change-log entry 54: one optional reply-level status, never a record field.
#[test]
fn reply_validates_with_and_without_its_one_read_status() -> Result<()> {
    let without = response();
    assert!(serde_json::to_value(&without)?.get("read_status").is_none());
    without.validate(&budget())?;
    let mut with = response();
    let status = json!({"freshness":{"state":"idle","observed_unix_ms":7},
        "discovery_readiness":{"state":"ready"},"semantic_readiness":{"state":"ready"}});
    with.read_status = status.as_object().cloned();
    with.validate(&budget())?;
    let value = serde_json::to_value(&with)?;
    assert_eq!(value["read_status"], status);
    let parsed = ContextResponseV3::from_canonical(&value, &budget())?;
    assert_eq!(parsed.read_status, with.read_status);
    for bad in [json!({}), json!({"request_id":"r"}), json!({"hits":[]})] {
        let mut wrong = response();
        wrong.read_status = bad.as_object().cloned();
        assert!(wrong.validate(&budget()).is_err(), "{bad}");
    }
    assert!(
        READ_STATUS_FIELDS
            .iter()
            .all(|field| VOLATILE_READ_FIELDS.contains(field))
    );
    Ok(())
}

#[test]
fn valid_two_part_response_and_ticket_are_accepted() -> Result<()> {
    response().validate(&budget())?;
    ticket().validate(&budget())?;
    let mut value = serde_json::to_value(response())?;
    value["publication"] = serde_json::to_value(&scope().publication)?;
    value["transport_identity"] = json!("normalized elsewhere");
    ContextResponseV3::from_canonical(&value, &budget())?;
    Ok(())
}

#[test]
fn each_rule_refuses_only_its_own_violation() -> Result<()> {
    let cases = refusals();
    // 76 with `skipped_gathering_stage_read` (change-log entry 77).
    assert_eq!(cases.len(), 76);
    for (id, mutate, expected) in cases {
        let mut packet = response();
        mutate(&mut packet);
        let error = packet
            .validate(&budget())
            .err()
            .unwrap_or_else(|| panic!("{id} must refuse"))
            .to_string();
        assert!(error.contains(expected), "{id}: {error}");
    }
    Ok(())
}

#[test]
fn the_frame_is_measured_on_the_transported_envelope() -> Result<()> {
    let packet = response();
    packet.validate(&budget())?;
    let mut value = serde_json::to_value(&packet)?;
    value["transport_padding"] = json!("x".repeat(131_072));
    let error = ContextResponseV3::from_canonical(&value, &budget())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Answer envelope exceeds the reply frame"),
        "{error}"
    );
    let mut small = budget();
    small.max_bytes = 4_096;
    let value = serde_json::to_value(&packet)?;
    assert!(
        ContextResponseV3::from_canonical(&value, &small).is_err(),
        "max_bytes below the reply bounds it"
    );
    Ok(())
}

#[test]
fn disclosed_degradations_and_partial_results_are_accepted() -> Result<()> {
    let mut packet = response();
    packet.parts[1].stop = PartStop::Deadline;
    packet.parts[1].sub_needs[0].unrun_reads[0].reason = UnrunReason::Deadline;
    packet.partial = Some(PartialMarker {
        stage: LoopStage::FollowUp,
        reason: PartialReason::Deadline,
        part: Some(2),
        continuation: Some(digest(81)),
    });
    packet.degradations.push(Degradation::ModelShare {
        part: 2,
        stage: LoopStage::Coverage,
    });
    packet.degradations.push(Degradation::Device {
        stage: Stage::Focus,
        requested: ExecutionDevice::Cuda,
        applied: ExecutionDevice::Cpu,
        reason: DegradationReason::DeviceUnavailable,
    });
    packet.validate(&budget())?;
    // A frame-omitted record may still be cited and counted.
    let mut cut = response();
    shortened(&mut cut);
    cut.omitted_evidence = vec![digest(33)];
    cut.parts[1].sub_needs[0].evidence.push(digest(33));
    cut.parts[1].sub_needs[0].evidence_omitted = 1;
    cut.union.dedup_count = 2;
    cut.validate(&budget())?;
    let mut wrong = cut.clone();
    let marker = wrong.partial.as_mut().unwrap();
    marker.stage = LoopStage::FollowUp;
    marker.reason = PartialReason::Deadline;
    assert!(
        wrong.validate(&budget()).is_err(),
        "no part or late stage ran out of time"
    );
    // Compose and Union may stop on the deadline with no part stopped by it.
    let mut late = response();
    late.partial = Some(PartialMarker {
        stage: LoopStage::Compose,
        reason: PartialReason::Deadline,
        part: None,
        continuation: Some(digest(82)),
    });
    late.validate(&budget())?;
    let mut verdict = response();
    verdict.union.conflicts.push(ConflictNote {
        kind: ConflictKind::DivergentVerdict,
        parts: vec![1, 2],
        evidence: vec![digest(31)],
        note: "one part found it sufficient, the other did not".into(),
    });
    verdict.validate(&budget())?;
    let mut clarify = response();
    clarify.parts[1].stop = PartStop::ClarificationRequired;
    clarify.parts[1].gaps = vec![Gap::Ambiguous];
    clarify.validate(&budget())?;
    let mut no_read = response();
    no_read.parts[1].stop = PartStop::NoJustifiedRead;
    no_read.parts[1].gaps = vec![Gap::Counterevidence];
    no_read.validate(&budget())?;
    let mut omitted = response();
    omitted.decisions.remove(0);
    omitted.decision_log.included = 3;
    omitted.decision_log.omitted = 1;
    omitted.validate(&budget())?;
    Ok(())
}

#[test]
fn partial_and_ticket_are_required_and_nullable() -> Result<()> {
    let mut value = serde_json::to_value(response())?;
    assert_eq!(value["partial"], Value::Null);
    assert_eq!(value["ticket"], Value::Null);
    value.as_object_mut().unwrap().remove("ticket");
    assert!(serde_json::from_value::<ContextResponseV3>(value).is_err());
    let mut decision = serde_json::to_value(&decisions()[0])?;
    decision["unknown"] = true.into();
    assert!(serde_json::from_value::<DecisionRecord>(decision).is_err());
    Ok(())
}

#[test]
fn plan_chooses_at_most_four_needs() -> Result<()> {
    let plan = |count: usize| DecisionRecord {
        ordinal: 2,
        stage: LoopStage::Plan,
        part: Some(1),
        chosen: ["A", "B", "C", "D", "E"][..count]
            .iter()
            .map(|symbol| chosen(symbol, CardClass::PlanNeed))
            .collect(),
        ..decisions()[0].clone()
    };
    plan(4).validate(48)?;
    let error = plan(5).validate(48).unwrap_err().to_string();
    assert!(error.contains("invalid Answer chosen cards"), "{error}");
    Ok(())
}

#[test]
fn unavailable_decisions_carry_typed_reason_and_unknown_work() -> Result<()> {
    let mut lost = decisions()[1].clone();
    lost.source = DecisionSource::Unavailable {
        fallback: DeterministicRule::TopFocusSpan,
        reason: AssessmentUnavailable::Unavailable,
    };
    lost.receipt = None;
    lost.top = None;
    lost.runner_up = None;
    lost.margin = None;
    lost.work_unknown = true;
    lost.validate(48)?;
    lost.work_unknown = false;
    assert!(lost.validate(48).is_err(), "a lost reply is unknown work");
    lost.work_unknown = true;
    lost.source = DecisionSource::Unavailable {
        fallback: DeterministicRule::TopFocusSpan,
        reason: AssessmentUnavailable::Abstained,
    };
    assert!(lost.validate(48).is_err());
    Ok(())
}

#[test]
fn a_call_not_dispatched_names_its_model_and_reason_and_holds_no_slot() -> Result<()> {
    let shed = |reason| DecisionRecord {
        source: DecisionSource::NotDispatched {
            fallback: DeterministicRule::FirstExtractedTerm,
            reason,
        },
        model: Some(SelectedModel::Arbiter),
        ..decisions()[3].clone()
    };
    for reason in [
        AssessmentUnavailable::TokenBudget,
        AssessmentUnavailable::Deadline,
        AssessmentUnavailable::Unavailable,
    ] {
        let decision = shed(reason);
        decision.validate(48)?;
        assert!(!decision.scored_by_model(), "no card call ran for it");
        assert_eq!(decision.source.kind(), SourceKind::NotDispatched);
    }
    assert_eq!(
        serde_json::to_value(shed(AssessmentUnavailable::TokenBudget))?["source"],
        json!({"kind":"not_dispatched","fallback":"first_extracted_term","reason":"token_budget"})
    );
    assert!(
        shed(AssessmentUnavailable::Abstained).validate(48).is_err(),
        "an abstention is a call that ran"
    );
    assert!(
        shed(AssessmentUnavailable::NoEvidence)
            .validate(48)
            .is_err()
    );
    let refused = |mutate: fn(&mut DecisionRecord)| {
        let mut decision = shed(AssessmentUnavailable::Deadline);
        mutate(&mut decision);
        decision.validate(48).is_err()
    };
    assert!(
        refused(|d| d.model = None),
        "the model that was present is named"
    );
    assert!(
        refused(|d| d.model = Some(SelectedModel::Focus)),
        "only a Context model scores cards"
    );
    assert!(refused(|d| d.card_slot = Some(1)), "no card slot is held");
    assert!(refused(|d| d.receipt = Some(digest(60))));
    assert!(refused(|d| d.device = Some(ExecutionDevice::Cuda)));
    assert!(refused(|d| d.work_unknown = true));
    // The no-model path still names no model.
    let mut plain = decisions()[3].clone();
    plain.validate(48)?;
    plain.model = Some(SelectedModel::Arbiter);
    assert!(plain.validate(48).is_err());
    Ok(())
}

#[test]
fn a_reply_counts_calls_not_dispatched_apart_from_the_no_model_path() -> Result<()> {
    // The fixture's Follow-up decision, with its card call shed for tokens.
    let mut packet = response();
    packet.decisions[3].source = DecisionSource::NotDispatched {
        fallback: DeterministicRule::FirstExtractedTerm,
        reason: AssessmentUnavailable::TokenBudget,
    };
    packet.decisions[3].model = Some(SelectedModel::Arbiter);
    let row = packet
        .decision_log
        .counts
        .iter_mut()
        .find(|row| row.stage == LoopStage::FollowUp)
        .expect("the Follow-up row");
    row.source = SourceKind::NotDispatched;
    row.model = Some(SelectedModel::Arbiter);
    packet.validate(&budget())?;
    let log = &packet.decision_log;
    // One reader's count per way a decision was taken.
    assert_eq!(
        [
            SourceKind::Model,
            SourceKind::Abstained,
            SourceKind::Unavailable,
            SourceKind::NotDispatched,
            SourceKind::Deterministic,
        ]
        .map(|source| log.count(None, source)),
        [1, 1, 0, 1, 1]
    );
    assert_eq!(
        log.count(Some(LoopStage::FollowUp), SourceKind::NotDispatched),
        1
    );
    // Its row names the model that was present; the no-model row names none.
    let mut unnamed = packet.clone();
    unnamed
        .decision_log
        .counts
        .iter_mut()
        .find(|row| row.stage == LoopStage::FollowUp)
        .expect("the Follow-up row")
        .model = None;
    let error = unnamed.validate(&budget()).unwrap_err().to_string();
    assert!(
        error.contains("invalid decision log stage and source counts"),
        "{error}"
    );
    // A shed call holds no card slot, so the reply's one card call stands.
    assert_eq!(log.card_calls, 1);
    Ok(())
}

fn cuda_runtime() -> String {
    format!("onnxruntime-cuda/1.30.0/device-execution/{}", digest(70))
}
fn receipt() -> CardAdvice {
    CardAdvice {
        schema: "airk.arbiter.advice/3".into(),
        task: Task::ContextCoverage,
        profile: digest(10),
        runtime: cuda_runtime(),
        compiler: digest(11),
        calibration: digest(12),
        policy: digest(13),
        task_policy: digest(51),
        scope: scope(),
        input_digest: digest(60),
        card_slot: 1,
        reserved_input_tokens: 200,
        observed_input_tokens: Some(100),
        attribution: vec![
            TokenAttribution {
                part: 1,
                round: 1,
                tokens: 60,
            },
            TokenAttribution {
                part: 2,
                round: 1,
                tokens: 40,
            },
        ],
        work: Work {
            input_tokens: 100,
            compile_us: Some(1),
            inference_us: Some(2),
            ..Work::default()
        },
        segments: vec![
            SegmentOutcome {
                ordinal: 2,
                raw_scores: vec![2.0, 0.5, -1.0],
                outcome: Outcome::Accepted {
                    decision: Decision::LoopCard(digest(43)),
                },
                rule: None,
                margin: Some(1.5),
            },
            SegmentOutcome {
                ordinal: 3,
                raw_scores: vec![0.1, 0.2, 0.0],
                outcome: Outcome::Abstained {
                    reason: "no accepted loop rule".into(),
                },
                rule: None,
                margin: None,
            },
        ],
    }
}
fn focus_receipt() -> Result<FocusBatchReceipt> {
    let mut focus = batch(2, 4)?;
    focus.observed_input_tokens = Some(16);
    focus.reserved_input_tokens = 16;
    focus.attribution[0].tokens = 16;
    focus.work.input_tokens = 16;
    focus.work.compile_us = Some(3);
    focus.work.inference_us = Some(4);
    focus.work.queue_us = None;
    focus.work.policy_us = None;
    Ok(focus)
}
fn coverage_cards() -> Vec<CardRecord> {
    [
        ("A", CardClass::Covered, 43),
        ("B", CardClass::ReadMoreHere, 44),
        ("C", CardClass::SearchForTerm, 45),
    ]
    .into_iter()
    .map(|(symbol, class, text)| CardRecord {
        symbol: symbol.into(),
        class,
        text_digest: digest(text),
        partition: vec![],
    })
    .collect()
}
fn span(utf8_start: usize, utf8_end: usize) -> QuestionSpan {
    QuestionSpan {
        utf8_start,
        utf8_end,
    }
}
fn log() -> Result<DecisionLog> {
    let [split, covered, abstained, follow_up]: [DecisionRecord; 4] =
        decisions().try_into().unwrap();
    Ok(DecisionLog {
        schema: "airk.arbiter.decision-log/1".into(),
        question: QUESTION.into(),
        scope: scope(),
        entries: vec![
            DecisionLogEntry {
                decision: split,
                policy: digest(50),
                cards: split_cards(),
                scores: vec![],
                chosen: vec![0],
                basis: vec![],
                basis_omitted: vec![],
            },
            DecisionLogEntry {
                decision: covered,
                policy: digest(51),
                cards: coverage_cards(),
                scores: vec![2.0, 0.5, -1.0],
                chosen: vec![0],
                basis: vec![unit(32)],
                basis_omitted: vec![],
            },
            DecisionLogEntry {
                decision: abstained,
                policy: digest(51),
                cards: coverage_cards(),
                scores: vec![0.1, 0.2, 0.0],
                chosen: vec![1],
                basis: vec![unit(31)],
                basis_omitted: vec![],
            },
            DecisionLogEntry {
                decision: follow_up,
                policy: digest(52),
                cards: follow_up_cards(),
                scores: vec![],
                chosen: vec![0],
                basis: vec![],
                basis_omitted: vec![],
            },
        ],
        receipts: vec![receipt()],
        focus_receipts: vec![focus_receipt()?],
    })
}

#[test]
fn decision_log_summary_is_what_the_reply_carries() -> Result<()> {
    let log = log()?;
    log.validate(&budget())?;
    let summary = log.summary(4)?;
    assert_eq!(
        DecisionLogSummary {
            digest: log.digest()?,
            ..self::summary()
        },
        summary
    );
    let mut packet = response();
    packet.decision_log = summary;
    packet.validate(&budget())?;
    Ok(())
}

/// A Focus call that scored none of its pairs (the live shape: the service
/// stopped before the first pair) still dispatched them; the reply counts
/// the call and its dispatched pairs and validates (change-log entry 53).
#[test]
fn a_focus_call_that_scored_nothing_counts_its_dispatched_pairs() -> Result<()> {
    let mut log = log()?;
    let focus = &mut log.focus_receipts[0];
    focus.pairs.clear();
    focus.applied = false;
    focus.availability = Availability::Failed;
    focus.ranked = (0..4).collect();
    focus.work.candidates = 0;
    log.validate(&budget())?;
    let summary = log.summary(4)?;
    assert_eq!((summary.focus_calls, summary.focus_pairs), (1, 4));
    let mut packet = response();
    packet.decision_log = summary;
    packet.validate_with_log(&log, &budget())?;
    // A dispatch outside 1..=32, or more scored than dispatched, is refused.
    for (candidates, scored, expected) in [
        (0, 0, "Focus batch dispatched 0 pairs and scored 0"),
        (33, 0, "Focus batch dispatched 33 pairs and scored 0"),
    ] {
        let mut wrong = log.clone();
        let focus = &mut wrong.focus_receipts[0];
        focus.candidate_count = candidates;
        focus.ranked = (0..u16::try_from(candidates)?).collect();
        focus.pairs.truncate(scored);
        let error = wrong.validate(&budget()).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
    let mut over = self::log()?;
    over.focus_receipts[0].candidate_count = 1;
    over.focus_receipts[0].ranked = vec![0];
    over.focus_receipts[0].pairs.truncate(1);
    over.focus_receipts[0].pairs.push(FocusPairScore {
        candidate: 0,
        question: 1,
        no_logit: 0.0,
        yes_logit: 1.0,
    });
    over.focus_receipts[0].work.candidates = 2;
    let error = over.validate(&budget()).unwrap_err().to_string();
    assert!(
        error.contains("Focus batch dispatched 1 pairs and scored 2"),
        "{error}"
    );
    // The budget bound on the log names its figures.
    let tight = BudgetV3 {
        focus_calls: 1,
        focus_pairs: 3,
        ..budget()
    };
    let error = log.validate(&tight).unwrap_err().to_string();
    assert!(
        error.contains("1 calls of 1 and 4 dispatched pairs of 3"),
        "{error}"
    );
    Ok(())
}

fn joined() -> Result<(ContextResponseV3, DecisionLog)> {
    let log = log()?;
    let mut packet = response();
    packet.decision_log = log.summary(4)?;
    Ok((packet, log))
}

#[test]
fn reply_validates_against_its_log() -> Result<()> {
    let (packet, log) = joined()?;
    packet.validate_with_log(&log, &budget())?;
    // The reply names the Focus profile its logged receipts ran on.
    for profile in [None, Some(digest(77))] {
        let mut foreign = packet.clone();
        foreign.model_work.focus_profile = profile;
        let error = foreign
            .validate_with_log(&log, &budget())
            .unwrap_err()
            .to_string();
        assert!(error.contains("Focus profile"), "{error}");
    }
    let mut unfocused = packet.clone();
    unfocused.model_work.focus_profile = Some(digest(10));
    unfocused.model_work.focus_calls = 0;
    unfocused.decision_log.focus_calls = 0;
    assert!(
        unfocused.validate(&budget()).is_err(),
        "profile without calls"
    );
    // Union omitted the oldest deterministic decision.
    let mut omitted = packet.clone();
    omitted.decisions.remove(0);
    omitted.decision_log = log.summary(3)?;
    omitted.validate_with_log(&log, &budget())?;
    // A Parts degradation explains a partition that differs from the parts.
    let mut degraded = packet.clone();
    let mut whole = log.clone();
    whole.entries[0].chosen = vec![1];
    whole.entries[0].decision.chosen = vec![chosen("B", CardClass::KeepWhole)];
    degraded.decisions[0].chosen = vec![chosen("B", CardClass::KeepWhole)];
    degraded.degradations.push(Degradation::Parts {
        requested: 4,
        applied: 2,
        reason: DegradationReason::DeadlineEstimate,
    });
    degraded.decision_log = whole.summary(4)?;
    degraded.validate_with_log(&whole, &budget())?;
    Ok(())
}

type JoinMutation = fn(&mut ContextResponseV3, &mut DecisionLog);
fn join_refusals() -> Vec<(&'static str, JoinMutation, &'static str)> {
    vec![
        (
            "log_invalid",
            |_, l| l.entries[0].decision.ordinal = 2,
            "decision log ordinals are not contiguous",
        ),
        (
            "question_differs",
            |_, l| l.question = "Something else?".into(),
            "decision log question or scope differs from the reply",
        ),
        (
            "scope_differs",
            |_, l| l.scope.configuration_revision = 2,
            "decision log question or scope differs from the reply",
        ),
        (
            "digest_differs",
            |r, _| r.decision_log.digest = digest(99),
            "reply decision log digest differs from the log",
        ),
        (
            "summary_differs",
            |r, _| r.decision_log.timings.compile_us = Some(3),
            "reply decision log summary differs from the log",
        ),
        (
            "entry_part_missing",
            |_, l| l.entries[3].decision.part = Some(3),
            "logged decision names a part, sub-need or round the reply lacks",
        ),
        (
            "included_decision_differs",
            |r, _| r.decisions[2].top = Some(0.3),
            "reply decision differs from its logged decision",
        ),
        (
            "newest_deterministic_omitted",
            |r, l| {
                r.decisions.remove(3);
                r.decision_log = l.summary(3).unwrap();
            },
            "Union must omit the oldest deterministic decisions first",
        ),
        (
            "split_partition_differs",
            |r, l| {
                l.entries[0].chosen = vec![1];
                l.entries[0].decision.chosen = vec![chosen("B", CardClass::KeepWhole)];
                r.decisions[0].chosen = vec![chosen("B", CardClass::KeepWhole)];
            },
            "the chosen Split partition differs from the reply's parts",
        ),
    ]
}

#[test]
fn each_join_rule_refuses_only_its_own_violation() -> Result<()> {
    let cases = join_refusals();
    assert_eq!(cases.len(), 9);
    for (id, mutate, expected) in cases {
        let (mut packet, mut log) = joined()?;
        let before = log.digest()?;
        mutate(&mut packet, &mut log);
        // A changed log is re-summarized so only the targeted rule can fail.
        if log.digest()? != before && packet.decision_log.digest == before {
            packet.decision_log = log
                .summary(packet.decisions.len())
                .unwrap_or(packet.decision_log);
        }
        let error = packet
            .validate_with_log(&log, &budget())
            .err()
            .unwrap_or_else(|| panic!("{id} must refuse"))
            .to_string();
        assert!(error.contains(expected), "{id}: {error}");
    }
    Ok(())
}

#[test]
fn an_empty_log_has_a_fixed_documented_digest() -> Result<()> {
    let empty = |question: &str| DecisionLog {
        schema: "airk.arbiter.decision-log/1".into(),
        question: question.into(),
        scope: scope(),
        entries: vec![],
        receipts: vec![],
        focus_receipts: vec![],
    };
    let first = empty("First question?").digest()?;
    assert_eq!(first, empty("Second question?").digest()?);
    assert_eq!(first, empty_log_digest());
    assert_eq!(first.to_string(), EMPTY_LOG_DIGEST_HEX);
    ticket().validate(&budget())?;
    let mut wrong = ticket();
    wrong.decision_log.digest = digest(99);
    let error = wrong.validate(&budget()).unwrap_err().to_string();
    assert!(
        error.contains("an empty decision log has the fixed empty log digest"),
        "{error}"
    );
    Ok(())
}
/// Recorded in CONTRACT.md.
const EMPTY_LOG_DIGEST_HEX: &str =
    "ab089e739cf3e4ed3e1b500ace2b8f30b3ecf32c0dcbfe1a5c72de5e4a80ae63";

type LogMutation = fn(&mut DecisionLog);
fn log_refusals() -> Vec<(&'static str, LogMutation, &'static str)> {
    vec![
        (
            "ordinal_gap",
            |l| l.entries[0].decision.ordinal = 2,
            "decision log ordinals are not contiguous",
        ),
        (
            "basis_omitted_differs",
            |l| l.entries[1].basis_omitted = vec![unit(33)],
            "reply basis differs from the logged basis",
        ),
        (
            "moved_split_partition",
            |l| l.entries[0].cards[0].partition = vec![span(0, 6), span(6, QUESTION.len())],
            "logged roster differs from its digest",
        ),
        (
            "stale_logged_decision_current",
            |l| l.entries[2].decision.part = Some(1),
            "only the latest logged decision can remain current",
        ),
        (
            "split_partition_gap",
            |l| l.entries[0].cards[0].partition[1].utf8_start += 1,
            "Split partition ranges must be contiguous and non-empty",
        ),
        (
            "split_partition_short",
            |l| {
                l.entries[0].cards[0].partition.pop();
            },
            "Split partition must cover the question in at most four ranges",
        ),
        (
            "keep_whole_two_ranges",
            |l| l.entries[0].cards[1].partition = l.entries[0].cards[0].partition.clone(),
            "KeepWhole is one range",
        ),
        (
            "split_at_one_range",
            |l| l.entries[0].cards[0].partition = l.entries[0].cards[1].partition.clone(),
            "SplitAt proposes two or more parts",
        ),
        (
            "partition_on_coverage_card",
            |l| l.entries[1].cards[0].partition = vec![span(0, QUESTION.len())],
            "only Split cards carry a partition",
        ),
        (
            "reply_chosen_differs",
            |l| l.entries[1].chosen = vec![1],
            "reply decision differs from its logged chosen cards",
        ),
        (
            "reply_top_differs",
            |l| l.entries[1].scores[0] = 2.5,
            "reply top and runner-up differ from the logged scores",
        ),
        (
            "reply_basis_differs",
            |l| l.entries[1].basis = vec![unit(31)],
            "reply basis differs from the logged basis",
        ),
        (
            "receipt_slot_differs",
            |l| l.receipts[0].card_slot = 2,
            "decision and receipt task, policy or slot differ",
        ),
        (
            "segment_part_uncharged",
            |l| {
                l.receipts[0].attribution.pop();
                l.receipts[0].attribution[0].tokens = 100;
            },
            "a batched segment's part and round carry no charge",
        ),
        (
            "batch_peers_incomplete",
            |l| l.entries[1].decision.batched_with = vec![],
            "decisions sharing a receipt must name each other",
        ),
        (
            "receipt_scores_differ",
            |l| l.receipts[0].segments[0].raw_scores[2] = -2.0,
            "decision scores differ from the receipt",
        ),
        (
            "receipt_outcome_differs",
            |l| {
                l.receipts[0].segments[1].outcome = Outcome::Unavailable {
                    reason: "lost".into(),
                }
            },
            "decision differs from its receipt outcome",
        ),
        (
            "receipt_segment_without_decision",
            |l| {
                let mut extra = l.receipts[0].segments[1].clone();
                extra.ordinal = 9;
                l.receipts[0].segments.push(extra);
            },
            "receipt segments differ from the decisions naming it",
        ),
        (
            "slot_names_two_receipts",
            |l| {
                let mut other = l.receipts[0].clone();
                other.input_digest = digest(61);
                other.segments.truncate(1);
                other.segments[0].ordinal = 3;
                l.receipts[0].segments.truncate(1);
                l.entries[2].decision.receipt = Some(digest(61));
                l.entries[1].decision.batched_with.clear();
                l.entries[2].decision.batched_with.clear();
                l.receipts.push(other);
            },
            "decisions of one card slot name different receipts",
        ),
        (
            "repeated_focus_receipt",
            |l| l.focus_receipts.push(l.focus_receipts[0].clone()),
            "repeated Focus batch receipt",
        ),
    ]
}

#[test]
fn decision_log_rules_refuse_only_their_own_violation() -> Result<()> {
    let cases = log_refusals();
    assert_eq!(cases.len(), 20);
    for (id, mutate, expected) in cases {
        let mut log = log()?;
        mutate(&mut log);
        let error = log
            .validate(&budget())
            .err()
            .unwrap_or_else(|| panic!("{id} must refuse"))
            .to_string();
        assert!(error.contains(expected), "{id}: {error}");
    }
    Ok(())
}

const FULL_QUESTION: &str = "Where is alpha defined? Who calls beta? Which tests cover gamma? \
Which contract governs delta?";

/// A full loop at the default budget: 4 parts, 4 sub-needs, 3 rounds, every
/// decision and every retained record in the reply, nothing shortened.
fn full_loop() -> ContextResponseV3 {
    let ends: Vec<usize> = FULL_QUESTION
        .match_indices('?')
        .map(|(index, _)| index + 1)
        .collect();
    let id = |part: u8, need: u8, round: u8| {
        Sha256Digest::from_bytes([part * 16 + need * 4 + round; 32])
    };
    let mut parts = Vec::new();
    let mut records = Vec::new();
    let mut uses = Vec::new();
    for part in 1..=4u8 {
        let start = if part == 1 {
            0
        } else {
            ends[usize::from(part) - 2] + 1
        };
        let mut needs = Vec::new();
        for need in 1..=4u8 {
            needs.push(SubNeed {
                origin: Default::default(),
                id: need,
                text: format!("sub-need {need} of part {part}: where and who and which tests"),
                kind: SubNeedKind::Callers,
                coverage: Coverage::Partial,
                evidence: (1..=3).map(|round| id(part, need, round)).collect(),
                evidence_omitted: 0,
                unrun_reads: vec![UnrunRead {
                    operation: LoopRead::References,
                    target: format!("callers_of_part_{part}_need_{need}"),
                    reason: UnrunReason::RoundLimit,
                }],
            });
            for round in 1..=3u8 {
                let mut evidence = record(0, LoopRead::Search);
                evidence.id = id(part, need, round);
                evidence.arguments = BTreeMap::from([
                    (
                        "query".into(),
                        json!(format!("restart guard caller part {part} need {need}")),
                    ),
                    ("mode".into(), json!("hybrid")),
                    ("limit".into(), json!(4)),
                    ("max_bytes".into(), json!(4096)),
                ]);
                evidence.canonical_result = realistic_search(part * 16 + need * 4 + round);
                records.push(evidence);
                uses.push(RecordUse {
                    record: id(part, need, round),
                    part,
                    need: Some(need),
                    round,
                    cause: if round == 1 {
                        UseCause::NeedSearch
                    } else {
                        UseCause::FollowUp
                    },
                });
            }
        }
        let follow_ups: Vec<FollowUp> = (1..=4)
            .map(|need| FollowUp {
                operation: LoopRead::Symbols,
                target: format!("symbol_{part}_{need}"),
            })
            .collect();
        parts.push(AnswerPart {
            id: part,
            span: QuestionSpan {
                utf8_start: start,
                utf8_end: ends[usize::from(part) - 1],
            },
            deadline_share_ms: 70_000,
            sub_needs: needs,
            gaps: vec![Gap::Example],
            rounds: vec![
                round(1, 25, 8, 8000, 4, vec![], 0),
                round(2, 4, 4, 8000, 4, follow_ups.clone(), 0),
                round(3, 4, 4, 8000, 4, follow_ups, 0),
            ],
            stop: PartStop::Rounds,
            shortened: false,
        });
    }
    let deterministic = |ordinal: u8, stage, part, need, round, chosen| DecisionRecord {
        ordinal,
        stage,
        part,
        need,
        round,
        chosen,
        ..decisions()[0].clone()
    };
    let mut decisions = vec![deterministic(
        1,
        LoopStage::Split,
        None,
        None,
        None,
        vec![chosen("A", CardClass::SplitAt)],
    )];
    for part in 1..=4u8 {
        decisions.push(deterministic(
            1 + part,
            LoopStage::Plan,
            Some(part),
            None,
            None,
            ["A", "B", "C", "D"]
                .iter()
                .map(|symbol| chosen(symbol, CardClass::PlanNeed))
                .collect(),
        ));
    }
    let mut slot = 0u8;
    for round in 1..=3u8 {
        for part in 1..=4u8 {
            for pair in [1u8, 3] {
                slot += 1;
                let first = u8::try_from(decisions.len()).unwrap() + 1;
                for need in [pair, pair + 1] {
                    let ordinal = u8::try_from(decisions.len()).unwrap() + 1;
                    decisions.push(DecisionRecord {
                        ordinal,
                        part: Some(part),
                        need: Some(need),
                        round: Some(round),
                        card_slot: Some(slot),
                        batched_with: vec![if ordinal == first { first + 1 } else { first }],
                        applicability: if round == 3 {
                            AssessmentApplicability::Current
                        } else {
                            AssessmentApplicability::HistoricalAfterRead
                        },
                        receipt: Some(Sha256Digest::from_bytes([slot; 32])),
                        ..self::decisions()[1].clone()
                    });
                }
            }
            if round < 3 {
                for need in 1..=4u8 {
                    let ordinal = u8::try_from(decisions.len()).unwrap() + 1;
                    let mut follow = deterministic(
                        ordinal,
                        LoopStage::FollowUp,
                        Some(part),
                        Some(need),
                        Some(round),
                        vec![chosen("A", CardClass::FollowUpQuery)],
                    );
                    follow.source = DecisionSource::Deterministic {
                        rule: DeterministicRule::FirstExtractedTerm,
                    };
                    follow.applicability = if round == 2 {
                        AssessmentApplicability::Current
                    } else {
                        AssessmentApplicability::HistoricalAfterRead
                    };
                    decisions.push(follow);
                }
            }
        }
    }
    let attribution = (1..=4u8)
        .flat_map(|part| {
            (1..=3u8).map(move |round| TokenAttribution {
                part,
                round,
                tokens: 4000,
            })
        })
        .collect();
    ContextResponseV3 {
        question: FULL_QUESTION.into(),
        parts,
        records,
        record_uses: uses,
        paragraphs: vec![],
        union: UnionSummary::default(),
        degradations: vec![],
        decision_log: DecisionLogSummary {
            digest: digest(71),
            total: 85,
            included: 85,
            omitted: 0,
            card_calls: 24,
            counts: vec![
                StageSourceCount {
                    stage: LoopStage::Split,
                    source: SourceKind::Deterministic,
                    model: None,
                    count: 1,
                },
                StageSourceCount {
                    stage: LoopStage::Plan,
                    source: SourceKind::Deterministic,
                    model: None,
                    count: 4,
                },
                StageSourceCount {
                    stage: LoopStage::Coverage,
                    source: SourceKind::Model,
                    model: Some(SelectedModel::Arbiter),
                    count: 48,
                },
                StageSourceCount {
                    stage: LoopStage::FollowUp,
                    source: SourceKind::Deterministic,
                    model: None,
                    count: 32,
                },
            ],
            focus_calls: 12,
            focus_pairs: 192,
            attribution,
            timings: LoopTimings::default(),
            work_unknown: 0,
        },
        decisions,
        work: Work {
            calls: 132,
            rounds: 3,
            fanout: 8,
            input_tokens: 96_000,
            candidates: 48,
            bytes: 120_000,
            ..Work::default()
        },
        model_work: LoopModelWork {
            context_calls: 24,
            focus_calls: 12,
            focus_pairs: 192,
            focus_profile: Some(digest(10)),
        },
        elapsed_ms: 240_000,
        candidate_count: 48,
        omitted_count: 0,
        ..response()
    }
}

/// One search result at the median sizes of 115 real search records from
/// the one-turn evaluation captures: four hits of about 1,000 bytes, each
/// with its content hash and location, and about 2,300 bytes of metadata.
fn realistic_search(seed: u8) -> Value {
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
        "publication":detail(297),
        "request_id":format!("req-{seed:032x}"),
        "scope_observation":detail(519),
        "semantic_readiness":detail(432),
        "truncated":false,
    })
}

/// Shorten as Union does: drop retained records from the newest, disclose
/// them as omitted and mark their parts shortened, until the frame fits.
fn pack(packet: &mut ContextResponseV3) -> Result<usize> {
    while serde_json::to_vec(packet)?.len() > reply_limit(&budget()) {
        let dropped = packet.records.pop().expect("records to drop");
        packet.record_uses.retain(|item| item.record != dropped.id);
        packet.omitted_evidence.push(dropped.id);
        packet.omitted_count += 1;
        for part in &mut packet.parts {
            for need in &mut part.sub_needs {
                if need.evidence.contains(&dropped.id) {
                    need.evidence_omitted += 1;
                    part.shortened = true;
                }
            }
        }
        packet.partial = Some(PartialMarker {
            stage: LoopStage::Union,
            reason: PartialReason::Bytes,
            part: None,
            continuation: None,
        });
    }
    Ok(serde_json::to_vec(packet)?.len())
}

#[test]
fn a_full_default_loop_with_realistic_records_is_shortened_to_the_frame() -> Result<()> {
    let packet = full_loop();
    assert_eq!(packet.decisions.len(), 85);
    assert_eq!(packet.records.len(), 48);
    let record = serde_json::to_vec(&packet.records[0])?.len();
    let unshortened = serde_json::to_vec(&packet)?.len();
    let mut packed = full_loop();
    let bytes = pack(&mut packed)?;
    packed.validate(&budget())?;
    eprintln!(
        "full default loop: record {record} bytes, unshortened {unshortened} bytes, \
         packed {bytes} bytes with {} of 48 records, headroom {}",
        packed.records.len(),
        131_072 - bytes
    );
    assert!(
        unshortened > 131_072,
        "48 realistic records exceed the frame"
    );
    assert!(bytes <= 131_072);
    assert!(!packed.records.is_empty() && packed.omitted_count > 0);
    let packet = packed;
    // Union may still omit decisions, oldest deterministic first, and disclose it.
    let mut trimmed = packet.clone();
    trimmed
        .decisions
        .retain(|decision| decision.scored_by_model() || decision.ordinal > 60);
    trimmed.decision_log.included = trimmed.decisions.len();
    trimmed.decision_log.omitted = 85 - trimmed.decisions.len();
    trimmed.validate(&budget())?;
    let mut wrong = packet.clone();
    wrong.decisions.retain(|decision| decision.ordinal != 6);
    wrong.decision_log.included = 84;
    wrong.decision_log.omitted = 1;
    let error = wrong.validate(&budget()).unwrap_err().to_string();
    assert!(
        error.contains("Union omits deterministic decisions before model-scored ones"),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_minimal_reply_fits_the_skeleton_around_its_question() -> Result<()> {
    let mut packet = response();
    packet.parts.truncate(1);
    packet.parts[0].span.utf8_end = first_end();
    packet.records.truncate(2);
    packet.paragraphs.clear();
    packet.union.dedup_count = 0;
    packet.record_uses.remove(1);
    packet.decisions.clear();
    packet.decision_log = DecisionLogSummary {
        digest: empty_log_digest(),
        total: 0,
        included: 0,
        card_calls: 0,
        counts: vec![],
        focus_calls: 0,
        focus_pairs: 0,
        attribution: vec![],
        timings: LoopTimings::default(),
        ..summary()
    };
    packet.degradations = vec![Degradation::FocusAbsent];
    packet.model_work = LoopModelWork::default();
    packet.records.retain(|record| record.id == digest(32));
    packet.record_uses.retain(|item| item.record == digest(32));
    packet.candidate_count = 1;
    packet.work.calls = 3;
    packet.work.rounds = 1;
    packet.work.input_tokens = 100;
    packet.work.candidates = 1;
    packet.work.compile_us = None;
    packet.work.inference_us = None;
    packet.records[0].canonical_result = json!({"hits":[]});
    packet.validate(&budget())?;
    let bytes = serde_json::to_vec(&packet)?.len();
    assert!(
        bytes - packet.question.len() <= MIN_REPLY_SKELETON_BYTES,
        "{bytes}"
    );
    Ok(())
}

/// Replacements turning `before` into `after`: whole values at the first
/// differing pointer, and whole arrays whose length changed.
fn changed(before: &Value, after: &Value, pointer: String, out: &mut Vec<Value>) {
    match (before, after) {
        (Value::Object(left), Value::Object(right)) => {
            let keys: BTreeSet<_> = left.keys().chain(right.keys()).collect();
            for key in keys {
                let next = format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                match (left.get(key), right.get(key)) {
                    (Some(l), Some(r)) => changed(l, r, next, out),
                    (_, Some(r)) => out.push(json!({"pointer":next,"value":r})),
                    (_, None) => out.push(json!({"pointer":next,"remove":true})),
                }
            }
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (l, r)) in left.iter().zip(right).enumerate() {
                changed(l, r, format!("{pointer}/{index}"), out);
            }
        }
        // A long run of one character is written as that character and a count.
        (_, Value::String(text))
            if before != after
                && text.len() > 1024
                && text.chars().all(|c| text.starts_with(c)) =>
        {
            out.push(json!({"pointer":pointer,"repeat":&text[..1],"count":text.len()}))
        }
        _ if before != after => out.push(json!({"pointer":pointer,"value":after})),
        _ => {}
    }
}

/// Rust-emitted semantic cases for generated clients. Write with
/// `AIRK_WRITE_ANSWER_LOOP_CASES=1`; otherwise compare.
#[test]
fn answer_loop_semantic_case_emitter() -> Result<()> {
    let valid = serde_json::to_value(response())?;
    let responses = vec![
        json!({"id":"two_parts_valid","response":valid,"accepted":true}),
        json!({"id":"ticket_valid","response":ticket(),"accepted":true}),
    ];
    // Each refusal is the valid response with these JSON-pointer replacements.
    let mut refused = Vec::new();
    for (id, mutate, expected) in refusals() {
        let mut packet = response();
        mutate(&mut packet);
        let error = packet.validate(&budget()).unwrap_err().to_string();
        assert!(error.contains(expected), "{id}: {error}");
        let mut changes = Vec::new();
        changed(
            &valid,
            &serde_json::to_value(&packet)?,
            String::new(),
            &mut changes,
        );
        assert!(!changes.is_empty(), "{id}");
        refused.push(json!({"id":id,"base":"two_parts_valid","replace":changes,
            "accepted":false,"error":expected}));
    }
    let document = json!({
        "schema":"airk.arbiter.answer-loop-cases/1",
        "qualification":false,
        "request_schema":"airk.arbiter.context.request/3",
        "response_schema":"airk.arbiter.context/3",
        "packet_schema":"airk.arbiter.packet/2",
        "reply_frame_bytes":131072,
        "minimum_reply_skeleton_bytes":4096,
        "default_budget":budget(),
        "budget_cases":budget_cases()?,
        "response_cases":responses,
        "refusal_cases":refused,
    });
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/arbiter/answer_loop_cases.json");
    if std::env::var_os("AIRK_WRITE_ANSWER_LOOP_CASES").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        let mut bytes = serde_json::to_vec_pretty(&document)?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes)?;
    }
    let expected: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    assert_eq!(
        document, expected,
        "regenerate only with the actual Core owner"
    );
    Ok(())
}

#[test]
fn card_advice_refuses_foreign_tasks_and_unbound_acceptance() -> Result<()> {
    receipt().validate()?;
    let mut wrong = receipt();
    wrong.task = Task::ContextAction;
    assert!(
        wrong.validate().is_err(),
        "the /2 joint task is not a card task"
    );
    let mut wrong = receipt();
    wrong.segments[0].outcome = Outcome::Accepted {
        decision: Decision::ContextAction(digest(43)),
    };
    assert!(wrong.validate().is_err());
    let mut wrong = receipt();
    wrong.observed_input_tokens = Some(201);
    wrong.work.input_tokens = 201;
    assert!(wrong.validate().is_err(), "observed above reservation");
    let mut wrong = receipt();
    wrong.reserved_input_tokens = 8193;
    assert!(wrong.validate().is_err(), "one call is at most 8192 tokens");
    let mut wrong = receipt();
    wrong.attribution[1].part = 0;
    assert!(
        wrong.validate().is_err(),
        "part 0 is Split, before the rounds"
    );
    let mut unknown = receipt();
    unknown.observed_input_tokens = None;
    unknown.work.input_tokens = 0;
    unknown.segments = vec![SegmentOutcome {
        ordinal: 2,
        raw_scores: vec![],
        outcome: Outcome::Unavailable {
            reason: "reply lost".into(),
        },
        rule: None,
        margin: None,
    }];
    unknown.validate()?;
    Ok(())
}

fn batch(pairs: usize, candidates: usize) -> Result<FocusBatchReceipt> {
    Ok(FocusBatchReceipt {
        schema: "airk.arbiter.focus.batch/1".into(),
        scope_digest: scope_digest(&scope())?,
        identity: Some(crate::arbiter::focus::FocusIdentity {
            profile: digest(10),
            runtime: cuda_runtime(),
            compiler: digest(11),
            calibration: digest(12),
            task_policy: digest(13),
        }),
        availability: Availability::Ready,
        reason: "r".repeat(256),
        applied: true,
        input_digest: digest(14),
        order_digest: digest(15),
        questions_digest: digest(16),
        question_count: 16,
        candidate_count: candidates,
        ranked: (0..u16::try_from(candidates)?).rev().collect(),
        pairs: (0..pairs)
            .map(|index| FocusPairScore {
                candidate: u16::try_from(index).unwrap(),
                question: u8::try_from(index % 16).unwrap(),
                no_logit: -12_345.678,
                yes_logit: 98_765.43,
            })
            .collect(),
        reserved_input_tokens: 16_384,
        observed_input_tokens: Some(16_384),
        attribution: vec![TokenAttribution {
            part: 1,
            round: 1,
            tokens: 16_384,
        }],
        work: Work {
            input_tokens: 16_384,
            candidates: pairs,
            compile_us: Some(u64::MAX),
            inference_us: Some(u64::MAX),
            queue_us: Some(u64::MAX),
            policy_us: Some(u64::MAX),
            ..Work::default()
        },
    })
}

#[test]
fn a_full_32_pair_focus_batch_fits_12_kib_of_custody() -> Result<()> {
    let receipt = batch(32, 100)?;
    receipt.validate(&scope(), &budget())?;
    let bytes = serde_json::to_vec(&receipt)?.len();
    assert!(bytes <= 12_288, "{bytes}");
    assert!(FocusBatchReceipt::bound(100, 32) <= 12_288);
    let mut over = batch(33, 100)?;
    over.pairs.truncate(33);
    assert!(over.validate(&scope(), &budget()).is_err(), "33 pairs");
    let mut duplicate = batch(2, 4)?;
    duplicate.pairs[1].candidate = 0;
    duplicate.pairs[1].question = 0;
    assert!(duplicate.validate(&scope(), &budget()).is_err());
    let mut lost = batch(2, 4)?;
    lost.ranked[0] = lost.ranked[1];
    assert!(
        lost.validate(&scope(), &budget()).is_err(),
        "not a permutation"
    );
    let mut unapplied = batch(0, 4)?;
    unapplied.applied = false;
    unapplied.observed_input_tokens = Some(0);
    unapplied.work.input_tokens = 0;
    unapplied.attribution[0].tokens = 0;
    assert!(
        unapplied.validate(&scope(), &budget()).is_err(),
        "reordered"
    );
    unapplied.ranked = vec![0, 1, 2, 3];
    unapplied.validate(&scope(), &budget())?;
    let mut foreign = batch(2, 4)?;
    foreign.scope_digest = digest(99);
    assert!(foreign.validate(&scope(), &budget()).is_err());
    Ok(())
}

fn loop_profile() -> Result<Profile> {
    let mut profile = current_profile(Stage::Context)?;
    profile.tasks.insert(Task::ContextCoverage);
    profile
        .task_policies
        .insert(Task::ContextCoverage, digest(51));
    Ok(profile)
}
fn calibration(profile: &Profile) -> LoopCalibration {
    LoopCalibration {
        schema: "airk.arbiter.loop-card.calibration/1".into(),
        model: profile.model,
        task: Task::ContextCoverage,
        compiler: profile.compiler,
        task_policy: digest(51),
        score_abi: profile.abi.clone(),
        assets: profile
            .assets
            .iter()
            .filter(|asset| {
                matches!(
                    asset.role,
                    AssetRole::Model | AssetRole::ExternalWeights | AssetRole::Tokenizer
                )
            })
            .map(|asset| {
                (
                    asset.name.clone(),
                    crate::arbiter::context_action::calibration::JointAssetBinding {
                        role: asset.role.clone(),
                        sha256: asset.sha256,
                    },
                )
            })
            .collect(),
        rules: vec![],
    }
}

#[test]
fn loop_calibration_ships_with_zero_rules_and_binds_its_profile() -> Result<()> {
    let profile = loop_profile()?;
    let shipped = calibration(&profile);
    shipped.validate(&profile)?;
    assert_eq!(
        shipped.admits(CardClass::Covered, 1e9),
        None,
        "zero rules abstain"
    );
    let raw = serde_json::to_vec(&shipped)?;
    LoopCalibration::parse_and_validate(&raw, &profile)?;
    let mut ruled = calibration(&profile);
    ruled.rules = vec![LoopRule {
        class: CardClass::Covered,
        minimum_raw_margin: 0.75,
    }];
    ruled.validate(&profile)?;
    assert_eq!(ruled.admits(CardClass::Covered, 0.75), Some(0));
    assert_eq!(ruled.admits(CardClass::Covered, 0.74), None);
    assert_eq!(ruled.admits(CardClass::ReadMoreHere, 5.0), None);
    for bad in [
        LoopRule {
            class: CardClass::PlanNeed,
            minimum_raw_margin: 1.0,
        },
        LoopRule {
            class: CardClass::Covered,
            minimum_raw_margin: -0.1,
        },
        LoopRule {
            class: CardClass::Covered,
            minimum_raw_margin: f32::NAN,
        },
    ] {
        let mut wrong = calibration(&profile);
        wrong.rules = vec![bad];
        assert!(wrong.validate(&profile).is_err());
    }
    let mut wrong = calibration(&profile);
    wrong.task_policy = digest(52);
    assert!(wrong.validate(&profile).is_err());
    let mut wrong = calibration(&profile);
    wrong.task = Task::ContextAction;
    assert!(wrong.validate(&profile).is_err());
    let mut wrong = calibration(&profile);
    wrong.assets.clear();
    assert!(wrong.validate(&profile).is_err());
    let text = String::from_utf8(raw)?;
    let first = text.find("\"assets\":{").unwrap() + "\"assets\":{".len();
    let end = text[first..].find("},").unwrap() + first + 1;
    let entry = &text[first..end];
    let duplicated = text.replacen(entry, &format!("{entry},{entry}"), 1);
    assert!(
        LoopCalibration::parse_and_validate(duplicated.as_bytes(), &profile).is_err(),
        "duplicate asset names refuse"
    );
    Ok(())
}

#[test]
fn calibration_set_holds_one_unruled_entry_per_admitted_card_task() -> Result<()> {
    let mut profile = loop_profile()?;
    profile.tasks.insert(Task::ContextSplit);
    profile.task_policies.insert(Task::ContextSplit, digest(53));
    let coverage = calibration(&profile);
    let split = LoopCalibration {
        task: Task::ContextSplit,
        task_policy: digest(53),
        ..calibration(&profile)
    };
    let set = LoopCalibrationSet {
        schema: "airk.arbiter.loop-card.calibration-set/1".into(),
        model: profile.model,
        compiler: profile.compiler,
        entries: vec![split.clone(), coverage.clone()],
    };
    set.validate(&profile)?;
    assert!(set.is_unruled());
    let raw = serde_json::to_vec(&set)?;
    let digest_of_raw = LoopCalibrationSet::digest(&raw);
    for asset in &mut profile.assets {
        if asset.role == AssetRole::Calibration {
            asset.sha256 = digest_of_raw;
            asset.bytes = u64::try_from(raw.len())?;
        }
    }
    profile.calibration = digest_of_raw;
    LoopCalibrationSet::parse_and_validate(&raw, &profile)?;
    let mut tampered = raw.clone();
    tampered.push(b' ');
    assert!(
        LoopCalibrationSet::parse_and_validate(&tampered, &profile).is_err(),
        "bytes other than the profile's calibration asset refuse"
    );
    for entries in [
        vec![coverage.clone()],
        vec![coverage.clone(), split.clone()],
        vec![split.clone(), coverage.clone(), coverage.clone()],
    ] {
        let wrong = LoopCalibrationSet {
            entries,
            ..set.clone()
        };
        assert!(wrong.validate(&profile).is_err());
    }
    let mut ruled = set.clone();
    ruled.entries[1].rules = vec![LoopRule {
        class: CardClass::Covered,
        minimum_raw_margin: 1.0,
    }];
    ruled.validate(&profile)?;
    assert!(!ruled.is_unruled());
    Ok(())
}
