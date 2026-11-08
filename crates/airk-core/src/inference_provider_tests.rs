// SPDX-License-Identifier: Apache-2.0
use super::*;

fn observation() -> CompletionObservation {
    CompletionObservation {
        completion_basis: CompletionBasis::NativeEngineBarrier,
        schema: 1,
        provider: ProviderIdentity {
            schema: 1,
            service_instance: Incarnation::from_bytes([1; 16]),
            approved_execution_sha256: Sha256Digest::from_bytes([2; 32]),
            provider_generation: Incarnation::from_bytes([3; 16]),
        },
        operation_id: Incarnation::from_bytes([4; 16]),
        captured_workers: vec![WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([5; 16]),
            project: Sha256Digest::from_bytes([6; 32]),
            fencing_generation: 1,
        }],
        admitted_high_water: 3,
        admission_closed: true,
        queued_outstanding: 0,
        running_outstanding: 0,
        callback_outstanding: 0,
        delivery_outstanding: 0,
        model_references_outstanding: 0,
        state: CompletionState::Complete,
        completion_ledger: None,
    }
}

#[test]
fn complete_observation_requires_closed_admission_and_every_owner_settled() {
    assert!(observation().validate().is_ok());
    for change in [
        |value: &mut CompletionObservation| value.admission_closed = false,
        |value: &mut CompletionObservation| value.queued_outstanding = 1,
        |value: &mut CompletionObservation| value.running_outstanding = 1,
        |value: &mut CompletionObservation| value.callback_outstanding = 1,
        |value: &mut CompletionObservation| value.delivery_outstanding = 1,
        |value: &mut CompletionObservation| value.model_references_outstanding = 1,
    ] {
        let mut value = observation();
        change(&mut value);
        assert!(value.validate().is_err());
        value.state = CompletionState::Pending;
        assert!(value.validate().is_ok());
    }
}

#[test]
fn crossed_workers_versions_counts_and_duplicate_census_refuse() {
    let mut value = observation();
    value.schema = 2;
    assert!(value.validate().is_err());
    value = observation();
    value.provider.schema = 2;
    assert!(value.validate().is_err());
    value = observation();
    value.captured_workers[0].service = Incarnation::from_bytes([7; 16]);
    assert!(value.validate().is_err());
    value = observation();
    value.captured_workers.push(value.captured_workers[0]);
    assert!(value.validate().is_err());
    value = observation();
    value.state = CompletionState::Pending;
    value.callback_outstanding = 4;
    assert!(value.validate().is_err());
    value = observation();
    value.state = CompletionState::Pending;
    value.queued_outstanding = u64::MAX;
    value.running_outstanding = 1;
    assert!(value.validate().is_err());
}

#[test]
fn completion_wire_roundtrip_is_inert_and_strict() {
    let original = observation();
    let bytes = serde_json::to_vec(&original).unwrap();
    let parsed: CompletionObservation = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed, original);
    let mut crossed = serde_json::to_value(&original).unwrap();
    crossed["native_reap_proof"] = serde_json::json!(true);
    assert!(serde_json::from_value::<CompletionObservation>(crossed).is_err());
}
#[test]
fn zero_factory_observation_cannot_admit_work_or_claim_pending() {
    let mut value = observation();
    value.completion_basis = CompletionBasis::NativeFactoryNeverAllocated;
    assert!(value.validate().is_err());
    value.admitted_high_water = 0;
    assert!(value.validate().is_ok());
    value.model_references_outstanding = 1;
    assert!(value.validate().is_err());
    value.model_references_outstanding = 0;
    value.state = CompletionState::Pending;
    assert!(value.validate().is_err());
}

fn ledger() -> CompletionLedgerObservation {
    CompletionLedgerObservation {
        schema: 1,
        namespace: Incarnation::from_bytes([8; 16]),
        service_instance: Incarnation::from_bytes([1; 16]),
        approved_execution_sha256: Sha256Digest::from_bytes([2; 32]),
        factory_generation: Incarnation::from_bytes([9; 16]),
        completed_owners: 1000,
        admitted_high_water: 3,
        head_sha256: Sha256Digest::from_bytes([10; 32]),
    }
}
#[test]
fn cumulative_completion_is_bounded_and_crosses_no_native_identity() {
    let mut value = observation();
    value.completion_basis = CompletionBasis::NativeFactoryCompletionLedger;
    value.provider.provider_generation = ledger().factory_generation;
    value.completion_ledger = Some(ledger());
    value.validate().unwrap();
    assert!(serde_json::to_vec(&value).unwrap().len() < 2048);
    assert_eq!(
        serde_json::from_slice::<CompletionObservation>(&serde_json::to_vec(&value).unwrap())
            .unwrap(),
        value
    );
    for change in [
        |v: &mut CompletionObservation| v.completion_ledger = None,
        |v: &mut CompletionObservation| v.admitted_high_water = 0,
        |v: &mut CompletionObservation| v.state = CompletionState::Pending,
        |v: &mut CompletionObservation| v.completion_ledger.as_mut().unwrap().schema = 2,
        |v: &mut CompletionObservation| v.completion_ledger.as_mut().unwrap().completed_owners = 0,
        |v: &mut CompletionObservation| {
            v.completion_ledger.as_mut().unwrap().service_instance =
                Incarnation::from_bytes([11; 16])
        },
        |v: &mut CompletionObservation| {
            v.completion_ledger
                .as_mut()
                .unwrap()
                .approved_execution_sha256 = Sha256Digest::from_bytes([11; 32])
        },
        |v: &mut CompletionObservation| {
            v.completion_ledger.as_mut().unwrap().factory_generation =
                Incarnation::from_bytes([11; 16])
        },
    ] {
        let mut crossed = value.clone();
        change(&mut crossed);
        assert!(crossed.validate().is_err());
    }
    let mut plain = observation();
    plain.completion_ledger = Some(ledger());
    assert!(plain.validate().is_err());
}
