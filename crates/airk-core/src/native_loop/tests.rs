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
