// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::arbiter::answer_loop::QuestionSpan;
use crate::arbiter::stage_tests::{digest, index, scope};
use crate::identity::Incarnation;

fn hit() -> Hit {
    Hit {
        citation: crate::Citation {
            path: "guard.rs".into(),
            content_hash: digest(12).to_string(),
            start_line: 1,
            end_line: 1,
            location: crate::DocumentLocation::Lines { start: 1, end: 1 },
        },
        text: "fn restart_guard() {}".into(),
        truncated: false,
    }
}
fn window() -> FocusSourceWindow {
    FocusSourceWindow {
        utf8_start: 0,
        utf8_end: 21,
    }
}
fn questions() -> Vec<String> {
    vec![
        "where is the restart guard defined".into(),
        "who calls the restart guard".into(),
    ]
}
fn worker() -> WorkerIdentity {
    WorkerIdentity {
        service: Incarnation::from_bytes([23; 16]),
        worker: Incarnation::from_bytes([24; 16]),
        project: digest(25),
        fencing_generation: 1,
    }
}
fn authority() -> ScopedReadRef {
    ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: index(5),
        grant_id: Some(crate::index_catalog::GrantId::from_identity(
            Incarnation::from_bytes([21; 16]),
        )),
        grant_revision: Some(crate::index_catalog::CatalogRevision::INITIAL),
        selection: None,
        fingerprint: digest(6),
    }
}
fn attempt() -> ReadAttemptBudget {
    ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([22; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: 30_000_000_100,
        remaining_ns: 30_000_000_000,
        native_retry_expires_monotonic_ns: None,
    }
}
fn focus_request() -> Result<NativeFocusRequestV3> {
    let scope = scope();
    let questions = questions();
    let candidates = vec![
        NativeFocusCandidateV3::new(&scope, &questions, 0, 0, hit(), window())?,
        NativeFocusCandidateV3::new(&scope, &questions, 1, 1, hit(), window())?,
    ];
    Ok(NativeFocusRequestV3 {
        read_attempt: attempt(),
        parent_budget: BudgetV3::default(),
        worker: worker(),
        authority: authority(),
        scope,
        questions,
        order_digest: NativeFocusRequestV3::candidate_order_digest(&candidates)?,
        candidates,
        score_count: 2,
        ordinal: 1,
        attribution: vec![TokenAttribution {
            part: 1,
            round: 1,
            tokens: 1024,
        }],
        partition: None,
        remaining_ms: 30_000,
        remaining_input_tokens: 1024,
    })
}
fn validate_focus(request: &NativeFocusRequestV3) -> Result<Duration> {
    request.validate(
        request.worker,
        request.authority,
        ReadClock::UnixMonotonic,
        100,
    )
}

#[test]
fn focus_candidate_identity_hashes_its_own_question() -> Result<()> {
    let request = focus_request()?;
    validate_focus(&request)?;
    // Same hit, window and scope; only the question (and ordinal) differ.
    let scope = scope();
    let same = NativeFocusCandidateV3::new(&scope, &questions(), 0, 0, hit(), window())?;
    let other = NativeFocusCandidateV3::new(&scope, &questions(), 1, 0, hit(), window())?;
    assert_ne!(same.id, other.id);
    let mut swapped = request.clone();
    swapped.candidates[0].question = 1;
    assert!(validate_focus(&swapped).is_err(), "question index is bound");
    let mut reworded = request.clone();
    reworded.questions[0] = "where is the guard declared".into();
    assert!(validate_focus(&reworded).is_err(), "question text is bound");
    Ok(())
}

#[test]
fn focus_request_bounds_questions_pairs_and_attribution() -> Result<()> {
    let request = focus_request()?;
    let mut repeated = request.clone();
    repeated.questions[1] = repeated.questions[0].clone();
    assert!(validate_focus(&repeated).is_err(), "questions are distinct");
    let mut many = request.clone();
    many.questions = (0..17).map(|n| format!("question {n}")).collect();
    assert!(validate_focus(&many).is_err(), "at most 16 questions");
    let mut unknown = request.clone();
    unknown.candidates[1].question = 2;
    assert!(validate_focus(&unknown).is_err());
    let mut charged = request.clone();
    charged.attribution[0].tokens = 1023;
    assert!(
        validate_focus(&charged).is_err(),
        "attribution equals reservation"
    );
    let mut split = request.clone();
    split.attribution[0].round = 0;
    assert!(validate_focus(&split).is_err(), "Focus runs inside rounds");
    let mut ordinal = request.clone();
    ordinal.ordinal = 25;
    assert!(validate_focus(&ordinal).is_err(), "default 24 Focus calls");
    let mut long = request.clone();
    long.remaining_ms = 600_001;
    assert!(validate_focus(&long).is_err());
    Ok(())
}

fn segment(ordinal: u8, part: u8) -> CardSegment {
    CardSegment {
        ordinal,
        card_slot: 1,
        stage: LoopStage::Coverage,
        part,
        need: 1,
        round: 1,
        subject: "where is the restart guard defined".into(),
        cards: vec![
            LoopCard {
                symbol: "A".into(),
                class: CardClass::Covered,
                text: "covered by guard.rs:1".into(),
                partition: vec![],
            },
            LoopCard {
                symbol: "B".into(),
                class: CardClass::SearchForTerm,
                text: "search for restart_guard callers".into(),
                partition: vec![],
            },
        ],
        multiple: false,
        evidence: vec![],
    }
}
fn card_request() -> NativeCardRequest {
    NativeCardRequest {
        schema: "airk.arbiter.advice.request/3".into(),
        read_attempt: attempt(),
        parent_budget: BudgetV3::default(),
        worker: worker(),
        authority: authority(),
        scope: scope(),
        task: Task::ContextCoverage,
        task_policy: digest(51),
        segments: vec![segment(2, 1), segment(3, 2)],
        attribution: vec![
            TokenAttribution {
                part: 1,
                round: 1,
                tokens: 600,
            },
            TokenAttribution {
                part: 2,
                round: 1,
                tokens: 400,
            },
        ],
        partition: None,
        remaining_ms: 30_000,
        remaining_input_tokens: 1000,
    }
}
fn validate_cards(request: &NativeCardRequest) -> Result<Duration> {
    request.validate(
        request.worker,
        request.authority,
        ReadClock::UnixMonotonic,
        100,
    )
}

#[test]
fn card_request_batches_segments_of_one_task_with_attributed_tokens() -> Result<()> {
    let request = card_request();
    validate_cards(&request)?;
    assert_ne!(
        request.segments[0].cards[0].text_digest()?,
        request.segments[0].cards[1].text_digest()?
    );
    let mut foreign = request.clone();
    foreign.segments[1].stage = LoopStage::FollowUp;
    assert!(validate_cards(&foreign).is_err(), "one task per call");
    let mut uncharged = request.clone();
    uncharged.attribution.pop();
    uncharged.attribution[0].tokens = 1000;
    assert!(
        validate_cards(&uncharged).is_err(),
        "every segment's part has a charge"
    );
    let mut stray = request.clone();
    stray.attribution[1].part = 3;
    assert!(
        validate_cards(&stray).is_err(),
        "charges name a segment's part"
    );
    let mut over = request.clone();
    over.attribution[0].tokens = 7_793;
    over.remaining_input_tokens = 8_193;
    assert!(
        validate_cards(&over).is_err(),
        "one call is at most 8192 tokens"
    );
    let mut duplicate = request.clone();
    duplicate.segments[1].ordinal = 2;
    assert!(validate_cards(&duplicate).is_err());
    let mut slot = request.clone();
    slot.segments[0].card_slot = 49;
    slot.segments[1].card_slot = 49;
    assert!(validate_cards(&slot).is_err(), "default 48 card slots");
    let mut split_slots = request.clone();
    split_slots.segments[1].card_slot = 2;
    assert!(validate_cards(&split_slots).is_err(), "one call, one slot");
    let mut late = request.clone();
    late.segments[1].ordinal = 101;
    validate_cards(&late)?;
    late.segments[1].ordinal = 102;
    assert!(validate_cards(&late).is_err(), "at most 101 decisions");
    let mut multiple = request.clone();
    multiple.segments[0].multiple = true;
    assert!(
        validate_cards(&multiple).is_err(),
        "only Plan chooses several"
    );
    let mut symbols = request;
    symbols.segments[0].cards[1].symbol = "A".into();
    assert!(validate_cards(&symbols).is_err());
    Ok(())
}

#[test]
fn split_segments_propose_whole_partitions() -> Result<()> {
    let question = "Where is it? Who calls it?";
    let span = |utf8_start, utf8_end| QuestionSpan {
        utf8_start,
        utf8_end,
    };
    let mut request = card_request();
    request.task = Task::ContextSplit;
    request.segments = vec![CardSegment {
        ordinal: 1,
        card_slot: 1,
        stage: LoopStage::Split,
        part: 0,
        need: 0,
        round: 0,
        subject: question.into(),
        cards: vec![
            LoopCard {
                symbol: "A".into(),
                class: CardClass::SplitAt,
                text: "two questions".into(),
                partition: vec![span(0, 12), span(12, 26)],
            },
            LoopCard {
                symbol: "B".into(),
                class: CardClass::KeepWhole,
                text: "one request".into(),
                partition: vec![span(0, 26)],
            },
        ],
        multiple: false,
        evidence: vec![],
    }];
    request.attribution = vec![TokenAttribution {
        part: 0,
        round: 0,
        tokens: 1000,
    }];
    validate_cards(&request)?;
    let mut gap = request.clone();
    gap.segments[0].cards[0].partition[1].utf8_start = 13;
    assert!(validate_cards(&gap).is_err(), "contiguous ranges");
    let mut five = request.clone();
    five.segments[0].cards[0].partition = (0..5).map(|n| span(n * 5, n * 5 + 5)).collect();
    five.segments[0].cards[0].partition[4].utf8_end = 26;
    assert!(validate_cards(&five).is_err(), "at most four ranges");
    let mut whole = request;
    whole.segments[0].cards[1].partition = vec![span(0, 12), span(12, 26)];
    assert!(validate_cards(&whole).is_err(), "KeepWhole is one range");
    Ok(())
}

fn allowance() -> PartitionAllowanceV3 {
    PartitionAllowanceV3 {
        schema: "airk.context-partition-allowance/3".into(),
        attempt: Incarnation::from_bytes([22; 16]),
        selection: digest(8),
        index_id: index(5),
        parent_budget: BudgetV3::default(),
        resources: LoopResources {
            calls: 33,
            input_tokens: 32_768,
            candidates: 33,
            bytes: 65_536,
            focus_pairs: 48,
        },
        card_ordinals: vec![1, 2, 3],
        focus_ordinals: vec![1],
    }
}

#[test]
fn v3_allowance_has_a_budget_v3_parent_and_slot_ordinals() -> Result<()> {
    allowance().validate()?;
    allowance().validate_binding(&attempt(), digest(8), index(5))?;
    for (id, mutate) in [
        (
            "unordered slots",
            (|a: &mut PartitionAllowanceV3| a.card_ordinals = vec![2, 1])
                as fn(&mut PartitionAllowanceV3),
        ),
        ("slots are not a prefix", |a| {
            a.card_ordinals = vec![1, 2, 12]
        }),
        ("Focus ordinals are not a prefix", |a| {
            a.focus_ordinals = vec![1, 3]
        }),
        ("slot over budget", |a| a.card_ordinals = (1..=49).collect()),
        ("focus over budget", |a| {
            a.focus_ordinals = (1..=25).collect();
            a.resources.focus_pairs = 25;
        }),
        ("Focus calls without pairs", |a| a.resources.focus_pairs = 0),
        ("Focus pairs without calls", |a| a.focus_ordinals.clear()),
        ("pairs over parent", |a| a.resources.focus_pairs = 193),
        ("tokens without model use", |a| {
            a.card_ordinals.clear();
            a.focus_ordinals.clear();
            a.resources.focus_pairs = 0;
        }),
        ("reads over parent", |a| a.resources.calls = 133),
        ("v2 schema", |a| {
            a.schema = "airk.context-partition-allowance/1".into()
        }),
    ] {
        let mut wrong = allowance();
        mutate(&mut wrong);
        assert!(wrong.validate().is_err(), "{id}");
    }
    let mut request = card_request();
    request.partition = Some(allowance());
    validate_cards(&request)?;
    let mut unslotted = request.clone();
    unslotted.partition.as_mut().unwrap().card_ordinals = vec![2];
    assert!(validate_cards(&unslotted).is_err(), "slot not allowed");
    let mut focus = focus_request()?;
    focus.partition = Some(allowance());
    validate_focus(&focus)?;
    focus.ordinal = 2;
    assert!(validate_focus(&focus).is_err(), "Focus ordinal not allowed");
    let mut pairs = focus_request()?;
    let mut narrow = allowance();
    narrow.resources.focus_pairs = 1;
    pairs.partition = Some(narrow);
    assert_eq!(pairs.score_count, 2);
    assert!(validate_focus(&pairs).is_err(), "Focus pairs not allowed");
    Ok(())
}

#[test]
fn v3_allowance_clamps_the_member_budget() -> Result<()> {
    let clamped = allowance().clamp(&BudgetV3::default())?;
    assert_eq!(
        (
            clamped.calls,
            clamped.candidates,
            clamped.max_bytes,
            clamped.input_tokens,
            clamped.round_input_tokens,
            clamped.context_calls,
            clamped.focus_calls,
            clamped.focus_pairs,
        ),
        (33, 33, 65_536, 32_768, 32_768, 3, 1, 48)
    );
    assert_eq!(clamped.deadline_ms, BudgetV3::default().deadline_ms);
    let mut reads_only = allowance();
    reads_only.card_ordinals.clear();
    reads_only.focus_ordinals.clear();
    reads_only.resources.focus_pairs = 0;
    reads_only.resources.input_tokens = 0;
    let clamped = reads_only.clamp(&BudgetV3::default())?;
    // The budget keeps its positive minimums; the allowance, carried by every
    // card and Focus request, grants none of them.
    assert_eq!(
        (
            clamped.input_tokens,
            clamped.context_calls,
            clamped.focus_calls
        ),
        (1, 1, 1)
    );
    let mut request = card_request();
    request.partition = Some(reads_only);
    assert!(validate_cards(&request).is_err(), "no slot granted");
    assert_eq!(ordinal_prefix(3)?, vec![1, 2, 3]);
    Ok(())
}

#[test]
fn card_digests_bind_split_partitions_and_match_the_log() -> Result<()> {
    let card = |partition: Vec<QuestionSpan>| LoopCard {
        symbol: "A".into(),
        class: CardClass::SplitAt,
        text: "two questions".into(),
        partition,
    };
    let span = |utf8_start, utf8_end| QuestionSpan {
        utf8_start,
        utf8_end,
    };
    let first = card(vec![span(0, 12), span(12, 26)]);
    let moved = card(vec![span(0, 13), span(13, 26)]);
    assert_ne!(
        first.text_digest()?,
        moved.text_digest()?,
        "partition is bound"
    );
    let segment = CardSegment {
        cards: vec![first.clone()],
        ..card_request().segments[0].clone()
    };
    assert_eq!(
        segment.roster_digest()?,
        crate::arbiter::answer_loop::roster_digest(segment.stage, &[first.record()?])?,
        "the log recomputes the native roster digest"
    );
    Ok(())
}

fn evidence(path: &str, focus: Option<f32>, excerpt: &str, cut_bytes: usize) -> CardEvidence {
    CardEvidence {
        path: path.into(),
        start_line: 10,
        end_line: 40,
        focus,
        excerpt: excerpt.into(),
        cut_bytes,
    }
}

/// A two-segment Coverage call with 8,192 reserved tokens, all charged to
/// part 1 and round 1.
fn evidence_request() -> NativeCardRequest {
    let mut request = card_request();
    request.segments[1].part = 1;
    request.segments[1].need = 2;
    request.attribution = vec![TokenAttribution {
        part: 1,
        round: 1,
        tokens: 8192,
    }];
    request.remaining_input_tokens = 8192;
    request
}

#[test]
fn the_excerpt_allowance_is_the_reservation_above_the_frame() {
    assert_eq!(CARD_FRAME_TOKENS, 2048);
    for (reserved, bytes) in [
        (0, 0),
        (1, 0),
        (2047, 0),
        (2048, 0),
        (2049, 1),
        (4096, 2048),
        (8192, 6144),
    ] {
        assert_eq!(evidence_allowance(reserved), bytes, "{reserved} tokens");
    }
}

#[test]
fn coverage_segments_carry_ranked_evidence_within_the_reservation() -> Result<()> {
    let mut request = evidence_request();
    request.segments[0].evidence = vec![
        evidence("src/guard.rs", Some(0.93), "pub fn restart_guard() {", 120),
        evidence("src/caller.rs", Some(0.93), "restart_guard();", 0),
        evidence("docs/guard.md", Some(0.25), "# Restart guard", 0),
        evidence("tests/guard.rs", None, "fn restart_guard_test() {}", 0),
        evidence("tests/other.rs", None, "fn other() {}", 0),
    ];
    // A sub-need with no span shows no evidence.
    assert!(request.segments[1].evidence.is_empty());
    validate_cards(&request)?;

    let refused = |mutate: fn(&mut NativeCardRequest)| {
        let mut wrong = evidence_request();
        wrong.segments[0].evidence = vec![
            evidence("src/guard.rs", Some(0.5), "pub fn restart_guard() {", 0),
            evidence("docs/guard.md", None, "# Restart guard", 0),
        ];
        validate_cards(&wrong).expect("the unchanged request is valid");
        mutate(&mut wrong);
        validate_cards(&wrong).is_err()
    };
    assert!(
        refused(|r| r.segments[0].evidence.swap(0, 1)),
        "an unscored span never precedes a scored one"
    );
    assert!(
        refused(|r| r.segments[0].evidence[1].focus = Some(0.75)),
        "scored spans fall in relevance"
    );
    assert!(refused(|r| r.segments[0].evidence[0].focus = Some(1.5)));
    assert!(refused(|r| r.segments[0].evidence[0].focus = Some(f32::NAN)));
    assert!(refused(|r| r.segments[0].evidence[0].excerpt.clear()));
    assert!(refused(|r| r.segments[0].evidence[0].path.clear()));
    assert!(refused(|r| r.segments[0].evidence[0].start_line = 0));
    assert!(refused(|r| r.segments[0].evidence[0].end_line = 9));
    assert!(
        refused(|r| r.segments[0].evidence[0].cut_bytes = 524_289),
        "no span is larger than the retention ceiling"
    );
    assert!(
        refused(|r| {
            let unscored = r.segments[0].evidence[1].clone();
            r.segments[0].evidence.extend(vec![unscored; 7]);
        }),
        "at most eight spans a segment"
    );
    Ok(())
}

#[test]
fn only_coverage_segments_carry_evidence() -> Result<()> {
    let shown = vec![evidence(
        "src/guard.rs",
        None,
        "pub fn restart_guard() {",
        0,
    )];
    let mut follow_up = evidence_request();
    follow_up.task = Task::ContextFollowUp;
    for segment in &mut follow_up.segments {
        segment.stage = LoopStage::FollowUp;
        for card in &mut segment.cards {
            card.class = CardClass::FollowUpHop;
        }
    }
    validate_cards(&follow_up)?;
    follow_up.segments[0].evidence = shown.clone();
    assert!(validate_cards(&follow_up).is_err(), "Follow-up shows none");

    let mut plan = evidence_request();
    plan.task = Task::ContextPlan;
    plan.segments.truncate(1);
    plan.segments[0].stage = LoopStage::Plan;
    plan.segments[0].need = 0;
    plan.segments[0].round = 0;
    plan.segments[0].multiple = true;
    plan.attribution[0].round = 0;
    for card in &mut plan.segments[0].cards {
        card.class = CardClass::PlanNeed;
    }
    validate_cards(&plan)?;
    plan.segments[0].evidence = shown;
    assert!(validate_cards(&plan).is_err(), "Plan shows none");
    Ok(())
}

#[test]
fn a_call_s_excerpts_stay_within_its_reservation_s_allowance() -> Result<()> {
    // 2,124 reserved tokens leave 76 excerpt bytes for the whole call.
    let mut request = evidence_request();
    request.attribution[0].tokens = 2124;
    request.remaining_input_tokens = 2124;
    request.segments[0].evidence = vec![evidence("src/a.rs", None, &"a".repeat(40), 0)];
    request.segments[1].evidence = vec![evidence("src/b.rs", None, &"b".repeat(36), 0)];
    validate_cards(&request)?;
    request.segments[1].evidence[0].excerpt.push('b');
    assert!(
        validate_cards(&request).is_err(),
        "77 bytes pass the 76-byte allowance"
    );
    // At the frame or below, a call shows no excerpt at all.
    let mut bare = evidence_request();
    bare.attribution[0].tokens = 2048;
    bare.remaining_input_tokens = 2048;
    validate_cards(&bare)?;
    bare.segments[0].evidence = vec![evidence("src/a.rs", None, "a", 0)];
    assert!(validate_cards(&bare).is_err());
    // The full reservation: 8,192 tokens allow 6,144 bytes.
    let mut full = evidence_request();
    full.segments[0].evidence = vec![evidence("src/a.rs", None, &"a".repeat(6144), 0)];
    validate_cards(&full)?;
    full.segments[0].evidence[0].excerpt.push('a');
    assert!(validate_cards(&full).is_err());
    Ok(())
}

#[test]
fn a_segment_without_evidence_keeps_its_wire_form() -> Result<()> {
    let bare = serde_json::to_value(segment(2, 1))?;
    let mut keys: Vec<&str> = bare
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "card_slot",
            "cards",
            "multiple",
            "need",
            "ordinal",
            "part",
            "round",
            "stage",
            "subject"
        ],
        "no evidence key, so an evidence-free request hashes as before"
    );
    let read: CardSegment = serde_json::from_value(bare)?;
    assert!(read.evidence.is_empty());
    let mut shown = segment(2, 1);
    shown.evidence = vec![
        evidence("src/guard.rs", Some(0.5), "pub fn restart_guard() {", 7),
        evidence("docs/guard.md", None, "# Restart guard", 0),
    ];
    assert_eq!(
        serde_json::to_value(&shown)?["evidence"],
        serde_json::json!([
            {"path":"src/guard.rs","start_line":10,"end_line":40,"focus":0.5,
             "excerpt":"pub fn restart_guard() {","cut_bytes":7},
            {"path":"docs/guard.md","start_line":10,"end_line":40,
             "excerpt":"# Restart guard","cut_bytes":0}
        ])
    );
    Ok(())
}
