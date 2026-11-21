// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::{Incarnation, RevisionToken, Sha256Digest},
    index_catalog::IndexId,
};
fn advice() -> ContextActionAdvice {
    ContextActionAdvice {
        schema: airk_core::arbiter::context_action::ADVICE_SCHEMA_V2.into(),
        task: Task::ContextAction,
        profile: Sha256Digest::from_bytes([1; 32]),
        runtime: RUNTIME.into(),
        compiler: Sha256Digest::from_bytes([2; 32]),
        calibration: Sha256Digest::from_bytes([3; 32]),
        policy: Sha256Digest::from_bytes([4; 32]),
        task_policy: Sha256Digest::from_bytes([5; 32]),
        scope: ScopeBinding {
            installation: Sha256Digest::from_bytes([6; 32]),
            catalog: Incarnation::from_bytes([7; 16]),
            index_id: IndexId::from_identity(Incarnation::from_bytes([8; 16])),
            authorization: Sha256Digest::from_bytes([9; 32]),
            configuration_revision: 2,
            publication: airk_core::evidence::RevisionEnvelope::new(
                "fixture".into(),
                RevisionToken {
                    incarnation: Incarnation::from_bytes([10; 16]),
                    generation: 1,
                    revision: Sha256Digest::from_bytes([11; 32]),
                },
            ),
        },
        input_digest: Sha256Digest::from_bytes([12; 32]),
        evidence: Vec::new(),
        reserved_input_tokens: 128,
        observed_input_tokens: None,
        work: Work::default(),
        outcome: Outcome::Unavailable {
            reason: "synthetic known native refusal".into(),
        },
    }
}
#[test]
fn native_completion_then_scope_revocation_retains_work_but_never_a_decision() -> Result<()> {
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let (revoke_tx, revoke_rx) = std::sync::mpsc::channel();
    let child = std::thread::spawn(move || -> Result<ContextActionAdvice> {
        let mut value = advice();
        observe_work(
            &mut value,
            Sha256Digest::from_bytes([33; 32]),
            64,
            17,
            19,
            Some(23),
        );
        value.outcome = Outcome::Accepted {
            decision: Decision::ContextAction(Sha256Digest::from_bytes([34; 32])),
        };
        completed_tx.send(())?;
        revoke_rx.recv_timeout(std::time::Duration::from_secs(5))?;
        assert!(retain_late_scope_refusal(
            &mut value,
            Err(airk_core::scoped_reads::ScopeReadRefused.into())
        ));
        value.validate()?;
        Ok(value)
    });
    completed_rx.recv_timeout(std::time::Duration::from_secs(5))?;
    revoke_tx.send(())?;
    let value = child.join().unwrap()?;
    assert_eq!(value.observed_input_tokens, Some(64));
    assert_eq!(value.input_digest, Sha256Digest::from_bytes([33; 32]));
    assert_eq!(
        (
            value.work.queue_us,
            value.work.compile_us,
            value.work.inference_us
        ),
        (Some(17), Some(19), Some(23))
    );
    assert!(matches!(value.outcome, Outcome::Unavailable { .. }));
    Ok(())
}
#[test]
fn actual_service_receipt_projection_preserves_known_overrun_and_core_counter_contract()
-> Result<()> {
    let mut value = advice();
    let digest = Sha256Digest::from_bytes([13; 32]);
    observe_work(&mut value, digest, 256, 17, 19, Some(23));
    assert_eq!(value.input_digest, digest);
    assert_eq!(value.observed_input_tokens, Some(256));
    assert_eq!(
        (
            value.work.input_tokens,
            value.work.queue_us,
            value.work.compile_us,
            value.work.inference_us
        ),
        (256, Some(17), Some(19), Some(23))
    );
    assert_eq!(
        (
            value.work.calls,
            value.work.rounds,
            value.work.fanout,
            value.work.candidates,
            value.work.bytes
        ),
        (0, 0, 0, 0, 0)
    );
    value.validate()?; // Failed measured overrun is retained, never accepted.
    value.outcome = Outcome::Accepted {
        decision: Decision::ContextAction(Sha256Digest::from_bytes([14; 32])),
    };
    assert!(value.validate().is_err());
    value.outcome = Outcome::Unavailable {
        reason: "synthetic known refusal".into(),
    };
    value.work.rounds = 1;
    assert!(
        value.validate().is_err(),
        "NN ordinal must never become a canonical read round"
    );
    Ok(())
}

#[test]
fn context_reservation_is_the_smaller_of_remaining_tokens_and_the_tier_ceiling() {
    for (remaining, ceiling, reserved) in [
        (8192, 2048, 2048),
        (8192, 8192, 8192),
        (1000, 2048, 1000),
        (2048, 2048, 2048),
        (1, 8192, 1),
        (4096, 128, 128),
    ] {
        assert_eq!(
            reserved_input_tokens(remaining, ceiling),
            reserved,
            "{remaining} remaining, {ceiling} ceiling"
        );
    }
}

#[cfg(feature = "local-inference")]
fn read_attempt() -> airk_core::read_attempt::ReadAttemptBudget {
    airk_core::read_attempt::ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([1; 16]),
        clock: airk_core::read_attempt::ReadClock::WindowsPerformanceCounter,
        expires_monotonic_ns: 120_000_000_100,
        remaining_ns: 120_000_000_000,
        native_retry_expires_monotonic_ns: None,
    }
}

#[test]
#[cfg(feature = "local-inference")]
fn input_above_the_tier_ceiling_is_a_known_refusal_with_no_native_work_and_no_held_reservation()
-> Result<()> {
    // The native lane's side, with an injected compiler and scorer, is
    // `input_above_the_token_ceiling_is_a_typed_refusal_before_any_native_score`
    // in the inference pool. This is the service's settlement of that refusal.
    let parent = airk_core::arbiter::Budget::default();
    assert_eq!(parent.input_tokens, 8192);
    for (compiled, reason) in [
        (
            None,
            "The native compiler refused this input before any model work, as it does for an input above the profile's token ceiling. No native work ran and the token reservation was released.",
        ),
        (
            Some(2300),
            "The compiled input is above the token ceiling reserved for this assessment. No native work ran and the token reservation was released.",
        ),
    ] {
        // A Fast tier (2,048 tokens) with 8,192 request tokens remaining.
        let reserved = reserved_input_tokens(8192, 2048);
        assert_eq!(reserved, 2048);
        let ledger = Mutex::new(super::super::joint_ledger::Ledger::default());
        let ticket =
            ledger
                .lock()
                .unwrap()
                .reserve(read_attempt(), &parent, 1, reserved, None, 100)?;
        // The hold is real: only the rest of the parent budget is free.
        assert!(
            ledger
                .lock()
                .unwrap()
                .reserve(read_attempt(), &parent, 2, 8192 - reserved + 1, None, 101)
                .is_err()
        );
        let mut value = advice();
        value.reserved_input_tokens = reserved;
        settle_known_refusal(&mut value, &ledger, ticket, compiled, 17, 19);
        // Compile work is known and no inference ran.
        assert_eq!(value.observed_input_tokens, Some(compiled.unwrap_or(0)));
        assert_eq!(value.work.input_tokens, compiled.unwrap_or(0));
        assert_eq!(
            (
                value.work.queue_us,
                value.work.compile_us,
                value.work.inference_us,
                value.work.policy_us
            ),
            (Some(17), Some(19), None, None)
        );
        assert!(
            matches!(&value.outcome, Outcome::Unavailable { reason: actual } if actual == reason),
            "{:?}",
            value.outcome
        );
        value.validate()?;
        // No reservation is held: the next ordinal may take the whole budget.
        ledger
            .lock()
            .unwrap()
            .reserve(read_attempt(), &parent, 2, 8192, None, 102)?;
        // The refused ordinal itself is never reusable.
        assert!(
            ledger
                .lock()
                .unwrap()
                .reserve(read_attempt(), &parent, 1, 1, None, 103)
                .is_err()
        );
    }
    // A refusal that contradicts a recorded compile receipt keeps its hold.
    let ledger = Mutex::new(super::super::joint_ledger::Ledger::default());
    let ticket = ledger
        .lock()
        .unwrap()
        .reserve(read_attempt(), &parent, 1, 2048, None, 100)?;
    ledger.lock().unwrap().compiled(ticket, 512)?;
    let mut value = advice();
    value.reserved_input_tokens = 2048;
    settle_known_refusal(&mut value, &ledger, ticket, None, 17, 19);
    assert!(
        matches!(&value.outcome, Outcome::Unavailable { reason } if reason.ends_with("the original reservation remains held.")),
        "{:?}",
        value.outcome
    );
    assert!(
        ledger
            .lock()
            .unwrap()
            .reserve(read_attempt(), &parent, 2, 8192 - 2048 + 1, None, 101)
            .is_err()
    );
    assert_eq!(value.work.inference_us, None);
    Ok(())
}

#[test]
#[cfg(all(feature = "local-inference", feature = "test-support"))]
fn private_observation_is_known_zero_dispatch_not_lost_work_or_a_decision() -> Result<()> {
    let cases: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/split/arbiter/native_context_action_cases.json"
    ))?;
    let structural: NativeContextActionRequest =
        serde_json::from_value(cases["cases"][0]["input"].clone())?;
    // Core semantic cases establish structural/source authority shapes, not
    // the current Retrieval constructor's task policy or roster digest.
    let mut request = structural.clone();
    let eligible = structural
        .roster
        .basis
        .read_choices_scored
        .iter()
        .copied()
        .collect();
    request.roster = airk_retrieval::arbiter::context_action::build_roster_with_eligible(
        &structural.question,
        &structural.scope,
        &structural.records,
        &structural.choices,
        &eligible,
    )?;
    let policy = airk_retrieval::arbiter::context_action::policy_digest()?;
    assert_eq!(request.roster.policy, policy);
    // Only the owner-generated roster changes. Keep every actual source,
    // authority, original attempt, clock/allowance and choice field untouched.
    let original = serde_json::to_value(&structural)?;
    let mut preserved = serde_json::to_value(&request)?;
    preserved["roster"] = original["roster"].clone();
    assert_eq!(preserved, original);
    let profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    // This historical Profile is inert receipt metadata only: no current ABI,
    // model execution, qualification or capability is inferred from this unit.
    assert!(
        captured_without_dispatch(&structural, &profile, policy).is_err(),
        "a structurally valid Core fixture must not bypass the owning task policy"
    );
    let before = serde_json::to_value(&request)?;
    let value = captured_without_dispatch(&request, &profile, policy)?;
    assert_eq!(value.observed_input_tokens, Some(0));
    assert_eq!(value.work, Work::default());
    // The reservation is the smaller of the remaining request tokens (2,048)
    // and this profile's token ceiling (128), not the remaining tokens alone.
    assert_eq!(
        (request.remaining_input_tokens, profile.max_tokens),
        (2048, 128)
    );
    assert_eq!(value.reserved_input_tokens, 128);
    assert!(matches!(value.outcome, Outcome::Unavailable { .. }));
    assert_eq!(value.scope, request.scope);
    assert_eq!(value.task_policy, request.roster.policy);
    assert_eq!(serde_json::to_value(&request)?, before);
    value.validate()?;
    Ok(())
}
