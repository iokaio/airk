// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn private_joint_reply_requires_explicit_nullable_advice() -> Result<()> {
    let reply = crate::service::ControlResponse::ArbiterContextActionAdvice { advice: None };
    let mut wire = serde_json::to_value(reply)?;
    assert!(wire["advice"].is_null());
    let _: crate::service::ControlResponse = serde_json::from_value(wire.clone())?;
    wire.as_object_mut().unwrap().remove("advice");
    assert!(serde_json::from_value::<crate::service::ControlResponse>(wire).is_err());
    Ok(())
}
use crate::{
    arbiter::context_action::*,
    identity::{Incarnation, Sha256Digest},
};
fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([value; 32])
}
fn request() -> Result<NativeContextActionRequest> {
    let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/split/arbiter/context_action_cases.json"
    ))?;
    let response: ContextResponseV2 = serde_json::from_value(fixture["cases"][0]["input"].clone())?;
    let event = response.assessment_history[0].clone();
    let AssessmentResult::Selected { card, advice } = event.result else {
        anyhow::bail!("native fixture changed")
    };
    let authority = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: response.assessment.scope.index_id,
        grant_id: Some(crate::index_catalog::GrantId::from_identity(
            Incarnation::from_bytes([21; 16]),
        )),
        grant_revision: Some(crate::index_catalog::CatalogRevision::INITIAL),
        selection: None,
        fingerprint: response.assessment.scope.authorization,
    };
    Ok(NativeContextActionRequest {
        parent_budget: crate::arbiter::Budget::default(),
        read_attempt: ReadAttemptBudget {
            schema: 1,
            request_id: Incarnation::from_bytes([22; 16]),
            clock: ReadClock::UnixMonotonic,
            expires_monotonic_ns: 30_000_000_100,
            remaining_ns: 30_000_000_000,
            native_retry_expires_monotonic_ns: None,
        },
        worker: WorkerIdentity {
            service: Incarnation::from_bytes([23; 16]),
            worker: Incarnation::from_bytes([24; 16]),
            project: digest(25),
            fencing_generation: 1,
        },
        authority,
        scope: response.assessment.scope.clone(),
        question: response.question,
        records: response.records,
        choices: vec![],
        roster: ContextActionRoster {
            schema: ACTION_ROSTER_SCHEMA.into(),
            scope: response.assessment.scope,
            input_digest: advice.input_digest,
            policy: advice.task_policy,
            basis: event.basis,
            cards: vec![card],
        },
        ordinal: 1,
        partition: None,
        remaining_ms: 30_000,
        remaining_input_tokens: 2048,
    })
}
#[test]
fn native_hook_rejects_foreign_identity_scope_clock_and_expired_parent() -> Result<()> {
    let original = request()?;
    original.validate(
        original.worker,
        original.authority,
        ReadClock::UnixMonotonic,
        100,
    )?;
    let mut worker = original.worker;
    worker.worker = Incarnation::from_bytes([99; 16]);
    assert!(
        original
            .validate(worker, original.authority, ReadClock::UnixMonotonic, 100)
            .is_err()
    );
    let mut authority = original.authority;
    authority.fingerprint = digest(99);
    assert!(
        original
            .validate(original.worker, authority, ReadClock::UnixMonotonic, 100)
            .is_err()
    );
    for field in ["schema", "remaining_ns", "expires_monotonic_ns"] {
        let mut raw = serde_json::to_value(&original)?;
        raw["read_attempt"][field] = serde_json::json!(0);
        let changed: NativeContextActionRequest = serde_json::from_value(raw)?;
        assert!(
            changed
                .validate(
                    original.worker,
                    original.authority,
                    ReadClock::UnixMonotonic,
                    100
                )
                .is_err(),
            "{field}"
        );
    }
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
    let mut foreign = original.clone();
    foreign.records.push(foreign.records[0].clone());
    assert!(
        foreign
            .validate(
                original.worker,
                original.authority,
                ReadClock::UnixMonotonic,
                100
            )
            .is_err()
    );
    foreign.records[1].canonical_result["contradictory"] = serde_json::json!(true);
    assert!(
        foreign
            .validate(
                original.worker,
                original.authority,
                ReadClock::UnixMonotonic,
                100
            )
            .is_err()
    );
    let mut foreign = original.clone();
    foreign.scope.index_id =
        crate::index_catalog::IndexId::from_identity(Incarnation::from_bytes([99; 16]));
    assert!(
        foreign
            .validate(
                original.worker,
                original.authority,
                ReadClock::UnixMonotonic,
                100
            )
            .is_err()
    );
    foreign = original.clone();
    foreign.scope.authorization = digest(99);
    assert!(
        foreign
            .validate(
                original.worker,
                original.authority,
                ReadClock::UnixMonotonic,
                100
            )
            .is_err()
    );
    Ok(())
}
#[test]
fn native_hook_partition_ordinal_tokens_and_source_universe_are_closed() -> Result<()> {
    let mut narrow = request()?;
    narrow.parent_budget.input_tokens = 512;
    narrow.parent_budget.rounds = 1;
    narrow.parent_budget.deadline_ms = 1000;
    narrow.remaining_input_tokens = 512;
    narrow.remaining_ms = 1000;
    narrow.validate(
        narrow.worker,
        narrow.authority,
        ReadClock::UnixMonotonic,
        100,
    )?;
    let mut reset = narrow.clone();
    reset.remaining_input_tokens = 2048;
    assert!(
        reset
            .validate(reset.worker, reset.authority, ReadClock::UnixMonotonic, 100)
            .is_err()
    );
    reset = narrow.clone();
    reset.ordinal = 2;
    assert!(
        reset
            .validate(reset.worker, reset.authority, ReadClock::UnixMonotonic, 100)
            .is_err()
    );
    reset = narrow.clone();
    reset.remaining_ms = 1001;
    assert!(
        reset
            .validate(reset.worker, reset.authority, ReadClock::UnixMonotonic, 100)
            .is_err()
    );
    let mut original = request()?;
    original.partition = Some(PartitionAllowance {
        schema: crate::combined_context::ALLOWANCE_SCHEMA.into(),
        attempt: original.read_attempt.request_id,
        selection: digest(30),
        index_id: original.scope.index_id,
        parent_budget: crate::arbiter::Budget::default(),
        resources: crate::combined_context::Resources {
            calls: 1,
            input_tokens: 2048,
            candidates: 1,
            bytes: 262144,
        },
        assessment_ordinals: vec![1],
    });
    original.validate(
        original.worker,
        original.authority,
        ReadClock::UnixMonotonic,
        100,
    )?;
    let mut different_parent = original.clone();
    different_parent
        .partition
        .as_mut()
        .unwrap()
        .parent_budget
        .rounds = 4;
    assert!(
        different_parent
            .validate(
                original.worker,
                original.authority,
                ReadClock::UnixMonotonic,
                100
            )
            .is_err()
    );
    for field in ["ordinal", "remaining_input_tokens"] {
        let mut raw = serde_json::to_value(&original)?;
        raw[field] = serde_json::json!(if field == "ordinal" { 2 } else { 2049 });
        let changed: NativeContextActionRequest = serde_json::from_value(raw)?;
        assert!(
            changed
                .validate(
                    original.worker,
                    original.authority,
                    ReadClock::UnixMonotonic,
                    100
                )
                .is_err()
        );
    }
    let mut foreign = original.clone();
    foreign.partition.as_mut().unwrap().attempt = Incarnation::from_bytes([99; 16]);
    assert!(
        foreign
            .validate(
                original.worker,
                original.authority,
                ReadClock::UnixMonotonic,
                100
            )
            .is_err()
    );
    foreign = original.clone();
    foreign.partition.as_mut().unwrap().resources.input_tokens = 1;
    assert!(
        foreign
            .validate(
                original.worker,
                original.authority,
                ReadClock::UnixMonotonic,
                100
            )
            .is_err()
    );
    foreign = original.clone();
    foreign.roster.basis.source_units_presented[0].result_pointer = "/future_native_field".into();
    assert!(
        foreign
            .validate(
                original.worker,
                original.authority,
                ReadClock::UnixMonotonic,
                100
            )
            .is_err()
    );
    foreign = original.clone();
    foreign.roster.basis.read_choices_available = 1;
    foreign.roster.basis.read_choices_omitted = 1;
    assert!(
        foreign
            .validate(
                original.worker,
                original.authority,
                ReadClock::UnixMonotonic,
                100
            )
            .is_err()
    );
    let mut raw = serde_json::to_value(&original)?;
    raw.as_object_mut().unwrap().remove("read_attempt");
    assert!(serde_json::from_value::<NativeContextActionRequest>(raw).is_err());
    let mut raw = serde_json::to_value(&original)?;
    raw.as_object_mut().unwrap().remove("parent_budget");
    assert!(serde_json::from_value::<NativeContextActionRequest>(raw).is_err());
    Ok(())
}

#[test]
fn emit_native_context_action_cases() -> Result<()> {
    let original = request()?;
    let mut cases = Vec::new();
    let mut add = |name: &str, input: NativeContextActionRequest, accepted: bool| {
        assert_eq!(
            input
                .validate(
                    original.worker,
                    original.authority,
                    ReadClock::UnixMonotonic,
                    100
                )
                .is_ok(),
            accepted,
            "{name}"
        );
        cases.push(serde_json::json!({"name":name,"input":input,"worker":original.worker,
            "authority":original.authority,"clock":ReadClock::UnixMonotonic,"now_ns":100,"accepted":accepted}));
    };
    add("captured_native_request", original.clone(), true);
    let mut changed = original.clone();
    changed.worker.worker = Incarnation::from_bytes([99; 16]);
    add("foreign_worker", changed, false);
    let mut changed = original.clone();
    changed.read_attempt.expires_monotonic_ns = 99;
    add("original_attempt_expired", changed, false);
    let mut changed = original.clone();
    changed.records.push(changed.records[0].clone());
    changed.records[1].canonical_result["contradictory"] = serde_json::json!(true);
    add("contradictory_same_record_id", changed, false);
    let value = serde_json::json!({"schema":"airk.native-context-action.cases/1","cases":cases});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/arbiter/native_context_action_cases.json");
    if std::env::var_os("AIRK_WRITE_NATIVE_CONTEXT_ACTION_CASES").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        let mut bytes = serde_json::to_vec_pretty(&value)?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes)?;
    }
    let expected: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    assert_eq!(value, expected);
    Ok(())
}
