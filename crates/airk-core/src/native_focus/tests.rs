// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    arbiter::stage_tests::{digest, index, scope},
    identity::Incarnation,
};
fn request() -> Result<NativeFocusRequest> {
    let scope = scope();
    let question = "Where is this defined?";
    let hit = Hit {
        citation: crate::Citation {
            path: "policy.md".into(),
            content_hash: digest(12).to_string(),
            start_line: 1,
            end_line: 1,
            location: crate::DocumentLocation::Lines { start: 1, end: 1 },
        },
        text: "defined here".into(),
        truncated: false,
    };
    let candidate = NativeFocusCandidate::new(
        &scope,
        question,
        0,
        hit,
        FocusSourceWindow {
            utf8_start: 0,
            utf8_end: 12,
        },
    )?;
    let candidates = vec![candidate];
    Ok(NativeFocusRequest {
        read_attempt: ReadAttemptBudget {
            schema: 1,
            request_id: Incarnation::from_bytes([22; 16]),
            clock: ReadClock::UnixMonotonic,
            expires_monotonic_ns: 30_000_000_100,
            remaining_ns: 30_000_000_000,
            native_retry_expires_monotonic_ns: None,
        },
        parent_budget: Budget::default(),
        worker: WorkerIdentity {
            service: Incarnation::from_bytes([23; 16]),
            worker: Incarnation::from_bytes([24; 16]),
            project: digest(25),
            fencing_generation: 1,
        },
        authority: ScopedReadRef {
            schema: 1,
            lease_generation: 1,
            index_id: index(5),
            grant_id: Some(crate::index_catalog::GrantId::from_identity(
                Incarnation::from_bytes([21; 16]),
            )),
            grant_revision: Some(crate::index_catalog::CatalogRevision::INITIAL),
            selection: None,
            fingerprint: digest(6),
        },
        scope,
        question: question.into(),
        order_digest: NativeFocusRequest::candidate_order_digest(&candidates)?,
        candidates,
        score_count: 1,
        ordinal: 1,
        partition: None,
        remaining_ms: 30_000,
        remaining_input_tokens: 2048,
    })
}
fn validate(request: &NativeFocusRequest) -> Result<Duration> {
    request.validate(
        request.worker,
        request.authority,
        ReadClock::UnixMonotonic,
        100,
    )
}
#[test]
fn original_worker_scope_clock_and_attempt_are_required_before_focus() -> Result<()> {
    let original = request()?;
    assert_eq!(validate(&original)?, Duration::from_secs(30));
    let mut worker = original.worker;
    worker.worker = Incarnation::from_bytes([99; 16]);
    assert!(
        original
            .validate(worker, original.authority, ReadClock::UnixMonotonic, 100)
            .is_err()
    );
    let mut value = original.clone();
    value.scope.authorization = digest(99);
    assert!(validate(&value).is_err());
    assert!(
        original
            .validate(
                original.worker,
                original.authority,
                ReadClock::WindowsPerformanceCounter,
                100
            )
            .is_err()
    );
    assert!(
        original
            .validate(
                original.worker,
                original.authority,
                ReadClock::UnixMonotonic,
                30_000_000_100
            )
            .is_err()
    );
    let mut value = original.clone();
    value.remaining_ms = 30_001;
    assert!(validate(&value).is_err());
    Ok(())
}
#[test]
fn full_canonical_candidate_bodies_order_and_exact_source_windows_are_bound() -> Result<()> {
    let original = request()?;
    let mut value = original.clone();
    value.candidates[0].hit.text.push('!');
    assert!(validate(&value).is_err());
    let mut value = original.clone();
    value.candidates[0].hit.citation.path = "foreign.md".into();
    assert!(validate(&value).is_err());
    let mut value = original.clone();
    value.candidates[0].window.utf8_end = 11;
    assert!(validate(&value).is_err());
    let mut value = original.clone();
    value.order_digest = digest(99);
    assert!(validate(&value).is_err());
    let mut value = original.clone();
    value.candidates.push(value.candidates[0].clone());
    assert!(validate(&value).is_err());
    let mut value = original;
    value.score_count = 2;
    assert!(validate(&value).is_err());
    Ok(())
}
#[test]
fn input_digest_and_full_permutation_response_cannot_bind_another_request() -> Result<()> {
    let request = request()?;
    let mut ranking = crate::arbiter::focus::tests::ranking()?;
    ranking.scope = request.scope.clone();
    ranking.input_digest = request.input_digest()?;
    ranking.original_order = request.candidate_order();
    ranking.ranked_order = ranking.original_order.clone();
    ranking.scores = vec![crate::arbiter::FocusScore::from_logits(
        ranking.original_order[0],
        0.,
        1.,
    )?];
    ranking.presentations = vec![crate::arbiter::FocusPresentation {
        candidate: ranking.original_order[0],
        utf8_start: 0,
        utf8_end: 12,
        source_bytes: 12,
    }];
    ranking.presented_count = 1;
    ranking.candidate_count = 1;
    ranking.scored_count = 1;
    ranking.omitted_count = 0;
    ranking.work.candidates = 1;
    let mut response = NativeFocusResponse { ranking };
    response.validate(&request)?;
    response.ranking.presentations[0].utf8_end = 11;
    assert!(response.validate(&request).is_err());
    response.ranking.presentations[0].utf8_end = 12;
    response.ranking.input_digest = digest(99);
    assert!(response.validate(&request).is_err());
    Ok(())
}
#[test]
fn native_focus_request_fixture_emitter() -> Result<()> {
    let request = request()?;
    validate(&request)?;
    if let Some(path) = std::env::var_os("AIRK_WRITE_NATIVE_FOCUS_CASES") {
        std::fs::write(path, serde_json::to_vec_pretty(&request)?)?;
    }
    Ok(())
}
#[test]
fn focus_private_control_variants_are_closed_framed_and_explicitly_nullable() -> Result<()> {
    use crate::service::{ControlMethod, ControlResponse};
    let original = request()?;
    let methods = [
        ControlMethod::ArbiterFocusApplication {
            worker: original.worker,
            authority: original.authority,
            publication: original.scope.publication.clone(),
        },
        ControlMethod::ArbiterFocusRank {
            request: Box::new(original),
        },
    ];
    for method in &methods {
        assert_eq!(
            method.metadata_relay_refusal(),
            Some("console_private_setup_forbidden")
        );
        assert_eq!(
            method.request_frame_budget(),
            crate::service::MAX_TOOLS_CALL_REQUEST_BYTES
        );
        assert_eq!(
            method.response_frame_budget(),
            crate::service::MAX_TOOLS_CALL_RESPONSE_BYTES
        );
        let _: ControlMethod = serde_json::from_value(serde_json::to_value(method)?)?;
    }
    for (response, field) in [
        (
            ControlResponse::ArbiterFocusApplication { application: None },
            "application",
        ),
        (
            ControlResponse::ArbiterFocusRank { response: None },
            "response",
        ),
    ] {
        let mut value = serde_json::to_value(response)?;
        assert!(value[field].is_null());
        let _: ControlResponse = serde_json::from_value(value.clone())?;
        value.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ControlResponse>(value).is_err());
    }
    if let Some(path) = std::env::var_os("AIRK_WRITE_NATIVE_FOCUS_CONTROL_CASES") {
        std::fs::write(path, serde_json::to_vec_pretty(&methods)?)?;
    }
    Ok(())
}
#[test]
fn focus_semantic_digest_only_ignores_shortened_relative_time_carriers() -> Result<()> {
    let original = request()?;
    let original_digest = original.input_digest()?;
    let mut shorter = original.clone();
    shorter.remaining_ms = 1000;
    shorter.read_attempt.remaining_ns = 1_000_000_000;
    assert_eq!(shorter.input_digest()?, original_digest);
    validate(&shorter)?;
    let mut expired = shorter.clone();
    expired.remaining_ms = 0;
    assert_eq!(expired.input_digest()?, original_digest);
    assert!(validate(&expired).is_err());
    let mut changes = Vec::new();
    let mut changed = original.clone();
    changed.read_attempt.expires_monotonic_ns -= 1;
    changes.push(changed);
    let mut changed = original.clone();
    changed.read_attempt.request_id = Incarnation::from_bytes([99; 16]);
    changes.push(changed);
    let mut changed = original.clone();
    changed.read_attempt.clock = ReadClock::WindowsPerformanceCounter;
    changes.push(changed);
    let mut changed = original.clone();
    changed.read_attempt.native_retry_expires_monotonic_ns = Some(1000);
    changes.push(changed);
    let mut changed = original.clone();
    changed.remaining_input_tokens -= 1;
    changes.push(changed);
    let mut changed = original.clone();
    changed.parent_budget.input_tokens -= 1;
    changes.push(changed);
    let mut changed = original.clone();
    changed.candidates[0].hit.text.push('!');
    changes.push(changed);
    let mut changed = original.clone();
    changed.score_count = 0;
    changes.push(changed);
    for changed in changes {
        assert_ne!(changed.input_digest()?, original_digest);
    }
    if let Some(path) = std::env::var_os("AIRK_WRITE_NATIVE_FOCUS_DIGEST_CASES") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &serde_json::json!({"schema":"airk.native-focus-input-digest-cases/2",
            "domain":"airk.arbiter.focus.input/2","original":original,"shortened":shorter,"semantic_digest":original_digest,
            "shortened_semantic_digest":shorter.input_digest()?,"qualification":false}),
            )?,
        )?;
    }
    Ok(())
}
#[test]
fn ordinary_focus_only_allowance_retains_actual_focus_attempt_index_and_delivery_custody()
-> Result<()> {
    let mut original = request()?;
    original.parent_budget.calls = 1;
    original.parent_budget.rounds = 1;
    original.parent_budget.fanout = 1;
    original.parent_budget.max_bytes = 65536;
    let allowance = PartitionAllowance {
        schema: crate::combined_context::ALLOWANCE_SCHEMA.into(),
        attempt: original.read_attempt.request_id,
        selection: digest(8),
        index_id: original.scope.index_id,
        parent_budget: original.parent_budget.clone(),
        resources: crate::combined_context::Resources::budget(&original.parent_budget),
        assessment_ordinals: vec![],
    };
    assert!(allowance.is_ordinary_focus());
    allowance.validate()?;
    original.partition = Some(allowance);
    validate(&original)?;
    let mut foreign = original.clone();
    foreign.partition.as_mut().unwrap().index_id = index(99);
    assert!(validate(&foreign).is_err());
    let mut foreign = original;
    foreign.partition.as_mut().unwrap().attempt = Incarnation::from_bytes([99; 16]);
    assert!(validate(&foreign).is_err());
    Ok(())
}
