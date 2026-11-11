// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::identity::Sha256Digest;

fn record(action: LifecycleAction, phase: LifecyclePhase) -> LifecycleRecord {
    let id = |byte: u8| Incarnation::from_bytes([byte; 16]);
    LifecycleRecord {
        id: Sha256Digest::from_bytes([1; 32]),
        request: LifecycleRequest {
            caller: id(2),
            key: RequestKey {
                namespace: id(3),
                epoch: 4,
                nonce: id(5),
            },
            action,
            expected_instance: id(6),
            runtime: Sha256Digest::from_bytes([7; 32]),
            manager: false,
        },
        phase,
    }
}

#[test]
fn a_drained_stop_completes_and_a_drained_restart_awaits_its_start() {
    assert_eq!(drained(LifecycleAction::Stop), LifecyclePhase::Complete);
    assert_eq!(drained(LifecycleAction::Restart), LifecyclePhase::Starting);
}

#[test]
fn an_admitted_start_completes_every_pending_record() {
    use LifecycleAction::{Restart, Stop};
    use LifecyclePhase::{Abandoned, Complete, Draining, Starting};
    for (action, phase, expected) in [
        (Stop, Draining, vec![Complete]),
        (Restart, Draining, vec![Starting, Complete]),
        (Restart, Starting, vec![Complete]),
        (Stop, Complete, vec![]),
        (Restart, Abandoned, vec![]),
    ] {
        assert_eq!(
            completion(&record(action, phase)),
            expected,
            "{action:?} {phase:?}"
        );
    }
}

#[test]
fn ledger_refusals_keep_their_code_and_others_are_unavailable() {
    for code in [
        "another_lifecycle_request_is_pending",
        "lifecycle_request_key_expired",
    ] {
        assert_eq!(failure_code(&anyhow::anyhow!(code)), code);
    }
    for message in ["database is locked", "", "Lifecycle_Key"] {
        assert_eq!(
            failure_code(&anyhow::anyhow!("{message}")),
            "lifecycle_unavailable"
        );
    }
}
