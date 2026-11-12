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

/// M20 on a real ledger: a pending restart of another instance is abandoned,
/// a repeat answers the same record, this instance's drain is refused and an
/// unknown id is refused.
#[test]
fn abandon_marks_a_stuck_record_and_refuses_this_instances_drain() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let mut registry = ServiceRegistry::open(&root, Incarnation::from_bytes([9; 16]))?;
    let mut request = |nonce: u8, instance: u8| -> Result<LifecycleRecord> {
        let (namespace, epoch) = registry.request_epoch()?;
        registry.begin_lifecycle(&LifecycleRequest {
            caller: Incarnation::from_bytes([2; 16]),
            key: RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes([nonce; 16]),
            },
            action: LifecycleAction::Restart,
            expected_instance: Incarnation::from_bytes([instance; 16]),
            runtime: Sha256Digest::from_bytes([4; 32]),
            manager: false,
        })
    };
    let stuck = request(1, 3)?;
    drop(registry);
    let this = Incarnation::from_bytes([8; 16]);
    let abandoned = abandon(&root, this, stuck.id)?;
    assert_eq!(abandoned.phase, LifecyclePhase::Abandoned);
    let again = abandon(&root, this, stuck.id)?;
    assert_eq!((again.id, again.phase), (abandoned.id, abandoned.phase));
    let mut registry = ServiceRegistry::open(&root, Incarnation::from_bytes([9; 16]))?;
    let (namespace, epoch) = registry.request_epoch()?;
    let draining = registry.begin_lifecycle(&LifecycleRequest {
        caller: Incarnation::from_bytes([2; 16]),
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([6; 16]),
        },
        action: LifecycleAction::Stop,
        expected_instance: this,
        runtime: Sha256Digest::from_bytes([4; 32]),
        manager: false,
    })?;
    drop(registry);
    let refused = abandon(&root, this, draining.id).unwrap_err();
    assert_eq!(failure_code(&refused), "lifecycle_request_in_progress");
    let unknown = abandon(&root, this, Sha256Digest::from_bytes([0xee; 32])).unwrap_err();
    assert_eq!(failure_code(&unknown), "lifecycle_request_unknown");
    Ok(())
}
