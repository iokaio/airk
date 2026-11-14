// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Sha256Digest,
    index_catalog::IndexId,
    read_attempt::{ReadAttemptBudget, ReadClock},
};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../fixtures/split/i1/combined_context_allowance.json"
    ))
    .unwrap()
}
fn allocated_query() -> Value {
    let f = fixture();
    let mut value: Value =
        serde_json::from_str(f["ordinary_query_bytes"].as_str().unwrap()).unwrap();
    value["tool"] = json!("airk_answer");
    value["context_partition"] = f["allowance"].clone();
    value
}

#[test]
fn combined_context_wire_absent_field_keeps_literal_ordinary_query_bytes() {
    let f = fixture();
    let bytes = f["ordinary_query_bytes"].as_str().unwrap();
    let query: Query = serde_json::from_str(bytes).unwrap();
    assert!(query.context_partition.is_none());
    assert_eq!(serde_json::to_string(&query).unwrap(), bytes);
}

#[test]
fn combined_context_wire_some_is_typed_and_old_closed_decoder_refuses_before_effects() {
    #[derive(Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct PreviousQuery {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        read_attempt: Option<ReadAttemptBudget>,
        frontend: Option<airk_core::service::RuntimeProcess>,
        tool: String,
        arguments: serde_json::Map<String, Value>,
        native: Option<airk_mcp::local::NativeRead>,
        status_receipt: airk_mcp::response::StatusReceipt,
    }
    let ordinary: Value =
        serde_json::from_str(fixture()["ordinary_query_bytes"].as_str().unwrap()).unwrap();
    let previous: PreviousQuery = serde_json::from_value(ordinary.clone()).unwrap();
    assert_eq!(serde_json::to_value(previous).unwrap(), ordinary);
    let value = allocated_query();
    let current: Query = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        current
            .context_partition
            .as_ref()
            .unwrap()
            .assessment_ordinals,
        vec![1, 2]
    );
    let mut effects = 0;
    if serde_json::from_value::<PreviousQuery>(value).is_ok() {
        effects += 1;
    }
    assert_eq!(effects, 0);
}

#[test]
fn combined_context_wire_null_unknown_and_malformed_allocations_refuse() {
    let mut null = allocated_query();
    null["context_partition"] = Value::Null;
    assert!(serde_json::from_value::<Query>(null).is_err());
    let mut unknown = allocated_query();
    unknown["context_partition"]["caller_authority"] = json!(true);
    assert!(serde_json::from_value::<Query>(unknown).is_err());
    for bad in fixture()["malformed_allowance"].as_array().unwrap() {
        let mut value = allocated_query();
        *value["context_partition"]
            .pointer_mut(bad["path"].as_str().unwrap())
            .unwrap() = bad["value"].clone();
        assert!(
            serde_json::from_value::<Query>(value).is_err(),
            "{}",
            bad["path"]
        );
    }
}

#[test]
fn combined_context_wire_cannot_rebind_original_attempt_selection_or_index() {
    let query: Query = serde_json::from_value(allocated_query()).unwrap();
    let allowance = query.context_partition.unwrap();
    let mut parent = ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([9; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: 3_000_000_000,
        remaining_ns: 2_000_000_000,
        native_retry_expires_monotonic_ns: None,
    };
    let selection = Sha256Digest::from_bytes([8; 32]);
    let index = IndexId::from_identity(Incarnation::from_bytes([1; 16]));
    allowance
        .validate_binding(&parent, selection, index)
        .unwrap();
    parent.request_id = Incarnation::from_bytes([7; 16]);
    assert!(
        allowance
            .validate_binding(&parent, selection, index)
            .is_err()
    );
    parent.request_id = Incarnation::from_bytes([9; 16]);
    assert!(
        allowance
            .validate_binding(&parent, Sha256Digest::from_bytes([7; 32]), index)
            .is_err()
    );
    assert!(
        allowance
            .validate_binding(
                &parent,
                selection,
                IndexId::from_identity(Incarnation::from_bytes([2; 16]))
            )
            .is_err()
    );
}
