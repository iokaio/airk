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
