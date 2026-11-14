// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::{Value, json};

fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([value; 32])
}
fn scope() -> ScopeBinding {
    ScopeBinding {
        installation: digest(1),
        catalog: Incarnation::from_bytes([2; 16]),
        index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
        authorization: digest(4),
        configuration_revision: 1,
        publication: crate::evidence::RevisionEnvelope::new(
            "fixture".into(),
            crate::identity::RevisionToken {
                incarnation: Incarnation::from_bytes([5; 16]),
                generation: 1,
                revision: digest(6),
            },
        ),
    }
}
fn record() -> EvidenceRecord {
    let hit = crate::Hit {
        citation: crate::Citation {
            path: "source.rs".into(),
            content_hash: digest(7).to_string(),
            start_line: 1,
            end_line: 1,
            location: crate::DocumentLocation::Lines { start: 1, end: 1 },
        },
        text: "fn café() {}".into(),
        truncated: false,
    };
    EvidenceRecord {
        id: digest(8),
        scope: scope(),
        operation: NativeRead::Search,
        arguments: BTreeMap::from([("query".into(), json!("café definition"))]),
        role: EvidenceRole::Unknown,
        canonical_result: json!({"hits":[hit],"future_native_field":{"retained":true}}),
    }
}
fn unit() -> EvidenceRef {
    EvidenceRef {
        record: digest(8),
        kind: SourceUnitKind::Hit,
        result_pointer: "/hits/0".into(),
        utf8_start: 0,
        utf8_end: "fn café() {}".len(),
    }
}
fn basis() -> AssessmentBasis {
    AssessmentBasis {
        source_units_available: 1,
        source_units_presented: vec![unit()],
        source_units_omitted: 0,
        source_units_partial: 0,
        read_choices_available: 0,
        read_choices_scored: vec![],
        read_choices_omitted: 0,
    }
}
fn sufficient() -> ContextActionCard {
    ContextActionCard {
        id: digest(9),
        action: ContextAction::Sufficient,
        gap: Gap::Sufficient,
        reason: ContextReason::RetainedEvidenceCoversQuestion,
        assessments: vec![EvidenceAssessment {
            evidence: unit(),
            role: EvidenceRole::Definition,
            relevance: RelevanceHypothesis::Useful,
        }],
    }
}
fn advice() -> ContextActionAdvice {
    ContextActionAdvice {
        schema: ADVICE_SCHEMA_V2.into(),
        task: Task::ContextAction,
        profile: digest(10),
        runtime: RUNTIME.into(),
        compiler: digest(11),
        calibration: digest(12),
        policy: digest(13),
        scope: scope(),
        input_digest: digest(14),
        evidence: vec![digest(8)],
        work: Work {
            input_tokens: 100,
            ..Work::default()
        },
        outcome: Outcome::Accepted {
            decision: Decision::ContextAction(digest(9)),
        },
    }
}
fn response() -> ContextResponseV2 {
    ContextResponseV2 {
        schema: CONTEXT_SCHEMA_V2.into(),
        packet_schema: PACKET_SCHEMA.into(),
        question: "Where is café defined?".into(),
        records: vec![record()],
        paragraphs: vec![GroundedParagraph {
            text: "fn café() {}".into(),
            evidence: vec![digest(8)],
            interpretation: false,
        }],
        gaps: vec![Gap::Sufficient],
        stop: PartitionTermination::Covered,
        work: Work {
            calls: 1,
            input_tokens: 100,
            ..Work::default()
        },
        candidate_count: 1,
        omitted_count: 0,
        assessment_history: vec![AssessmentEvent {
            ordinal: 1,
            partition: scope(),
            basis: basis(),
            result: AssessmentResult::Selected {
                card: sufficient(),
                advice: advice(),
            },
            applicability: AssessmentApplicability::Current,
        }],
        assessment: PartitionAssessment {
            scope: scope(),
            coverage: PartitionCoverage::Assessed { ordinal: 1 },
            termination: PartitionTermination::Covered,
        },
        applications: vec![],
    }
}
fn roster() -> ContextActionRoster {
    ContextActionRoster {
        schema: ACTION_ROSTER_SCHEMA.into(),
        scope: scope(),
        input_digest: digest(14),
        policy: digest(13),
        basis: basis(),
        cards: vec![sufficient()],
    }
}

fn overrun_deadline() -> ContextResponseV2 {
    let mut value = response();
    value.stop = PartitionTermination::Deadline;
    value.assessment.termination = value.stop;
    value.assessment.coverage = PartitionCoverage::Unassessed {
        reason: AssessmentUnavailable::Deadline,
    };
    value.gaps.clear();
    let work = Work {
        queue_us: Some(30_000_001),
        input_tokens: 100,
        ..Work::default()
    };
    value.work.queue_us = work.queue_us;
    value.assessment_history[0].result = AssessmentResult::Unavailable {
        reason: AssessmentUnavailable::Deadline,
        work,
        work_unknown: false,
    };
    value
}

#[test]
fn current_request_preserves_all_public_maxima_but_refuses_other_versions_and_authority()
-> Result<()> {
    let request = ContextRequestV2 {
        schema: REQUEST_SCHEMA_V2.into(),
        question: "界".repeat(5461),
        budget: Budget {
            calls: 12,
            rounds: 4,
            fanout: 12,
            input_tokens: 2048,
            candidates: 64,
            max_bytes: 524_288,
            deadline_ms: 30_000,
        },
    };
    request.validate()?;
    let mut changed = request.clone();
    changed.question.push('a');
    changed.validate()?; // exactly 16,384 UTF-8 bytes
    changed.question.push('a');
    assert!(changed.validate().is_err());
    let mut value = serde_json::to_value(&request)?;
    value["authorization"] = json!("caller-created");
    assert!(serde_json::from_value::<ContextRequestV2>(value).is_err());
    let mut changed = request;
    changed.schema = REQUEST_SCHEMA.into();
    assert!(changed.validate().is_err());
    Ok(())
}

#[test]
fn source_units_require_actual_typed_hits_and_exact_utf8_windows() -> Result<()> {
    assert!(!unit().validate(&[record()], &scope())?);
    let mut partial = unit();
    partial.utf8_start = 3;
    assert!(partial.validate(&[record()], &scope())?);
    partial.utf8_end = 7; // middle of é
    assert!(partial.validate(&[record()], &scope()).is_err());
    let mut metadata = unit();
    metadata.result_pointer = "/future_native_field/retained".into();
    assert!(metadata.validate(&[record()], &scope()).is_err());
    let mut forged = record();
    forged.canonical_result["future_native_field"]["forged_hit"] =
        forged.canonical_result["hits"][0].clone();
    metadata.result_pointer = "/future_native_field/forged_hit".into();
    assert!(metadata.validate(&[forged], &scope()).is_err());
    for pointer in [
        "/hits/00",
        "/hits/+0",
        "/hits/-0",
        "/hits/0/text",
        "/hits/0/",
        "/hits/1",
    ] {
        metadata.result_pointer = pointer.into();
        assert!(
            metadata.validate(&[record()], &scope()).is_err(),
            "{pointer}"
        );
    }
    let mut wrong_operation = record();
    wrong_operation.operation = NativeRead::Definitions;
    assert!(unit().validate(&[wrong_operation], &scope()).is_err());
    let mut foreign = scope();
    foreign.authorization = digest(99);
    assert!(unit().validate(&[record()], &foreign).is_err());
    Ok(())
}

#[test]
fn eight_total_frozen_cards_do_not_grant_unknown_choices_or_dismiss_omissions() -> Result<()> {
    roster().validate(&[record()])?;
    let mut duplicate_terminal = roster();
    let mut duplicate = sufficient();
    duplicate.id = digest(90);
    duplicate_terminal.cards.push(duplicate);
    assert!(duplicate_terminal.validate(&[record()]).is_err());
    let mut full = roster();
    let mut clarify = sufficient();
    clarify.id = digest(91);
    clarify.action = ContextAction::Clarify;
    clarify.gap = Gap::Ambiguous;
    clarify.reason = ContextReason::AmbiguousQuestion;
    full.cards.push(clarify);
    let mut no_read = sufficient();
    no_read.id = digest(92);
    no_read.action = ContextAction::NoJustifiedRead;
    no_read.gap = Gap::Definition;
    no_read.reason = ContextReason::NoVerifiedUsefulRead;
    full.cards.push(no_read);
    for value in 40..45 {
        let mut card = sufficient();
        card.id = digest(value);
        card.action = ContextAction::Read {
            choice: digest(value + 10),
        };
        card.gap = Gap::Definition;
        card.reason = ContextReason::MissingDefinition;
        full.basis.read_choices_scored.push(digest(value + 10));
        full.cards.push(card);
    }
    full.basis.read_choices_available = 5;
    full.validate(&[record()])?;
    assert!(roster().selected(digest(99)).is_err());
    let mut oversized = roster();
    oversized.cards = (0..9)
        .map(|id| {
            let mut card = sufficient();
            card.id = digest(id);
            card
        })
        .collect();
    assert!(oversized.validate(&[record()]).is_err());
    let mut terminal = sufficient();
    terminal.action = ContextAction::NoJustifiedRead;
    terminal.gap = Gap::Definition;
    terminal.reason = ContextReason::NoVerifiedUsefulRead;
    terminal.validate(&basis())?;
    let mut omitted = basis();
    omitted.read_choices_available = 1;
    omitted.read_choices_omitted = 1;
    assert!(terminal.validate(&omitted).is_err());
    let mut read = terminal;
    read.action = ContextAction::Read { choice: digest(55) };
    read.reason = ContextReason::MissingDefinition;
    assert!(read.validate(&basis()).is_err());
    Ok(())
}

#[test]
fn whole_card_reason_role_relevance_and_gap_must_agree() -> Result<()> {
    sufficient().validate(&basis())?;
    let mut card = sufficient();
    card.gap = Gap::Definition;
    assert!(card.validate(&basis()).is_err());
    card = sufficient();
    card.assessments[0].relevance = RelevanceHypothesis::Redundant;
    assert!(card.validate(&basis()).is_err());
    card = sufficient();
    card.assessments.clear();
    assert!(card.validate(&basis()).is_err());
    card = sufficient();
    card.action = ContextAction::Clarify;
    card.gap = Gap::Ambiguous;
    card.reason = ContextReason::ConflictingEvidence;
    assert!(card.validate(&basis()).is_err());
    let mut terminal = response();
    terminal.stop = PartitionTermination::NoJustifiedRead;
    terminal.assessment.termination = terminal.stop;
    let AssessmentResult::Selected { card, .. } = &mut terminal.assessment_history[0].result else {
        unreachable!()
    };
    card.action = ContextAction::NoJustifiedRead;
    card.gap = Gap::Definition;
    card.reason = ContextReason::NoVerifiedUsefulRead;
    assert!(terminal.validate(&Budget::default()).is_err());
    terminal.gaps = vec![Gap::Constraint];
    assert!(terminal.validate(&Budget::default()).is_err());
    terminal.gaps = vec![Gap::Definition];
    terminal.validate(&Budget::default())?;
    Ok(())
}

#[test]
fn exact_unit_history_and_partition_prevent_stale_or_packed_sufficiency() -> Result<()> {
    response().validate(&Budget::default())?;
    for applicability in [
        AssessmentApplicability::HistoricalAfterRead,
        AssessmentApplicability::BasisOmitted,
    ] {
        let mut value = response();
        value.assessment_history[0].applicability = applicability;
        assert!(value.validate(&Budget::default()).is_err());
    }
    let mut value = response();
    value.records.clear();
    value.paragraphs.clear();
    value.omitted_count = 1;
    assert!(value.validate(&Budget::default()).is_err());
    let mut value = response();
    value.assessment_history[0].partition.index_id =
        IndexId::from_identity(Incarnation::from_bytes([99; 16]));
    assert!(value.validate(&Budget::default()).is_err());
    let mut value = response();
    value.records[0].canonical_result["hits"][0]["text"] = json!("changed");
    assert!(value.validate(&Budget::default()).is_err());
    Ok(())
}

#[test]
fn ordinals_are_sparse_global_subsets_not_a_one_assessment_cap_or_budget_reset() -> Result<()> {
    let mut read_card = sufficient();
    read_card.action = ContextAction::Read { choice: digest(60) };
    read_card.gap = Gap::Definition;
    read_card.reason = ContextReason::MissingDefinition;
    for later_result in [
        AssessmentResult::Unavailable {
            reason: AssessmentUnavailable::Unavailable,
            work: Work::default(),
            work_unknown: false,
        },
        AssessmentResult::Abstained {
            advice: ContextActionAdvice {
                outcome: Outcome::Abstained {
                    reason: "known abstention".into(),
                },
                ..advice()
            },
        },
        AssessmentResult::Selected {
            card: read_card,
            advice: advice(),
        },
    ] {
        let mut old = response();
        let mut later = old.assessment_history[0].clone();
        later.ordinal = 2;
        later.result = later_result;
        if matches!(later.result, AssessmentResult::Selected { .. }) {
            later.basis.read_choices_available = 1;
            later.basis.read_choices_scored = vec![digest(60)];
        }
        old.assessment_history.push(later);
        old.work.input_tokens = 200;
        assert!(old.validate(&Budget::default()).is_err());
        old.assessment_history[0].applicability = AssessmentApplicability::HistoricalAfterRead;
        assert!(old.validate(&Budget::default()).is_err());
        old.assessment.coverage = PartitionCoverage::Unassessed {
            reason: AssessmentUnavailable::NewEvidence,
        };
        old.stop = PartitionTermination::NoProgress;
        old.assessment.termination = old.stop;
        old.gaps.clear();
        old.validate(&Budget::default())?;
    }
    let mut value = response();
    value.assessment_history[0].ordinal = 3;
    value.assessment.coverage = PartitionCoverage::Assessed { ordinal: 3 };
    value.validate(&Budget::default())?;
    let mut early = value.assessment_history[0].clone();
    early.ordinal = 1;
    early.applicability = AssessmentApplicability::HistoricalAfterRead;
    value.assessment_history.insert(0, early);
    value.work.input_tokens = 200;
    value.validate(&Budget::default())?;
    value.assessment_history[1].ordinal = 1;
    assert!(value.validate(&Budget::default()).is_err());
    value.assessment_history[1].ordinal = 4;
    let budget = Budget {
        rounds: 4,
        ..Budget::default()
    };
    assert!(value.validate(&budget).is_err());
    Ok(())
}

#[test]
fn unknown_postdispatch_work_blocks_later_optional_decisions_and_never_becomes_coverage()
-> Result<()> {
    let mut value = response();
    value.stop = PartitionTermination::Deadline;
    value.assessment.termination = PartitionTermination::Deadline;
    value.assessment.coverage = PartitionCoverage::Unassessed {
        reason: AssessmentUnavailable::Deadline,
    };
    value.gaps.clear();
    value.assessment_history[0].result = AssessmentResult::Unavailable {
        reason: AssessmentUnavailable::Deadline,
        work: Work {
            input_tokens: 100,
            ..Work::default()
        },
        work_unknown: true,
    };
    value.validate(&Budget::default())?;
    let mut later = response().assessment_history.remove(0);
    later.ordinal = 2;
    value.assessment_history.push(later);
    value.work.input_tokens = 200;
    assert!(value.validate(&Budget::default()).is_err());
    Ok(())
}

#[test]
fn joint_advice_rejects_next_read_masquerading_and_retains_complete_native_values() -> Result<()> {
    let mut legacy = advice();
    legacy.task = Task::ContextNextRead;
    assert!(legacy.validate().is_err());
    legacy = advice();
    legacy.outcome = Outcome::Accepted {
        decision: Decision::NextRead(digest(9)),
    };
    assert!(legacy.validate().is_err());
    let original = response();
    let decoded: ContextResponseV2 = serde_json::from_slice(&serde_json::to_vec(&original)?)?;
    decoded.validate(&Budget::default())?;
    assert_eq!(
        decoded.records[0].canonical_result,
        original.records[0].canonical_result
    );
    assert_eq!(
        decoded.records[0].canonical_result["future_native_field"]["retained"],
        true
    );
    for field in [
        "calls",
        "rounds",
        "fanout",
        "candidates",
        "bytes",
        "input_tokens",
        "queue_us",
        "compile_us",
        "inference_us",
        "policy_us",
    ] {
        let mut value = serde_json::to_value(response())?;
        value["assessment_history"][0]["result"]["advice"]["work"][field] = json!(u64::MAX);
        let decoded: ContextResponseV2 = serde_json::from_value(value)?;
        assert!(decoded.validate(&Budget::default()).is_err(), "{field}");
    }
    let mut measured = response();
    let AssessmentResult::Selected { advice, .. } = &mut measured.assessment_history[0].result
    else {
        unreachable!()
    };
    advice.work.queue_us = Some(12);
    assert!(measured.validate(&Budget::default()).is_err());
    measured.work.queue_us = Some(12);
    measured.validate(&Budget::default())?;
    let mut previous = measured.assessment_history[0].clone();
    previous.ordinal = 1;
    previous.applicability = AssessmentApplicability::HistoricalAfterRead;
    measured.assessment_history[0].ordinal = 3;
    measured.assessment.coverage = PartitionCoverage::Assessed { ordinal: 3 };
    measured.assessment_history.insert(0, previous);
    measured.work.input_tokens = 200;
    assert!(measured.validate(&Budget::default()).is_err());
    measured.work.queue_us = Some(24);
    measured.validate(&Budget::default())?;
    let mut overrun = response();
    let AssessmentResult::Selected { advice, .. } = &mut overrun.assessment_history[0].result
    else {
        unreachable!()
    };
    advice.work.queue_us = Some(30_000_001);
    overrun.work.queue_us = advice.work.queue_us;
    assert!(overrun.validate(&Budget::default()).is_err());
    let failed = overrun_deadline();
    failed.validate(&Budget::default())?;
    let decoded: ContextResponseV2 = serde_json::from_slice(&serde_json::to_vec(&failed)?)?;
    decoded.validate(&Budget::default())?;
    assert_eq!(decoded.work.queue_us, Some(30_000_001));
    let AssessmentResult::Unavailable { work, .. } = &decoded.assessment_history[0].result else {
        unreachable!()
    };
    assert_eq!(work.queue_us, Some(30_000_001));
    let mut overflow = failed;
    let mut later = overflow.assessment_history[0].clone();
    later.ordinal = 2;
    let AssessmentResult::Unavailable { work, .. } = &mut overflow.assessment_history[0].result
    else {
        unreachable!()
    };
    work.queue_us = Some(u64::MAX);
    overflow.assessment_history[0].applicability = AssessmentApplicability::HistoricalAfterRead;
    overflow.assessment_history.push(later);
    overflow.work.input_tokens = 200;
    overflow.work.queue_us = Some(u64::MAX);
    assert!(overflow.validate(&Budget::default()).is_err());
    Ok(())
}

#[test]
fn emit_context_action_semantic_cases() -> Result<()> {
    let budget = Budget::default();
    let mut cases = Vec::new();
    let mut add = |name: &str, input: ContextResponseV2, accepted: bool| -> Result<()> {
        assert_eq!(input.validate(&budget).is_ok(), accepted, "{name}");
        cases.push(json!({"name":name,"budget":budget,"input":input,"accepted":accepted}));
        Ok(())
    };
    add(
        "local_sufficient_with_complete_native_result",
        response(),
        true,
    )?;
    let mut input = response();
    input.assessment_history[0].applicability = AssessmentApplicability::BasisOmitted;
    add("packed_basis_cannot_be_sufficient", input, false)?;
    let mut input = response();
    input.assessment_history[0].ordinal = 4;
    add("fourth_joint_assessment_refused", input, false)?;
    let mut input = response();
    input
        .assessment_history
        .push(input.assessment_history[0].clone());
    add("duplicate_original_ordinal_refused", input, false)?;
    add(
        "overrun_deadline_metadata_retained",
        overrun_deadline(),
        true,
    )?;
    let mut input = response();
    input.work.queue_us = Some(30_000_001);
    let AssessmentResult::Selected { advice, .. } = &mut input.assessment_history[0].result else {
        unreachable!()
    };
    advice.work.queue_us = input.work.queue_us;
    add("overrun_cannot_be_covered", input, false)?;
    let actual = json!({"schema":"airk.arbiter.context-action.cases/1","cases":cases});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/arbiter/context_action_cases.json");
    if std::env::var_os("AIRK_WRITE_CONTEXT_ACTION_CASES").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let mut bytes = serde_json::to_vec_pretty(&actual)?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes)?;
    }
    let expected: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    assert_eq!(
        actual, expected,
        "regenerate only with the actual Core owner"
    );
    Ok(())
}
