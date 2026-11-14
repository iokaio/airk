// SPDX-License-Identifier: Apache-2.0
//! Literal Core wire compatibility only; service/transport admission is tested
//! by its owning consumers. The legacy decoder is independently retained here.
use super::*;
use crate::service::{ControlMethod, ToolResponseMode, ToolSelection};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

const CASES: &str = include_str!("../../../../fixtures/split/i1/tools_call_read_attempt.json");
const RETAINED: &str = include_str!("../../../../fixtures/split/i1/control_methods.json");

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum LegacyControlMethod {
    ToolsCall {
        tool: String,
        arguments: Map<String, Value>,
        selection: ToolSelection,
        response_mode: ToolResponseMode,
        usage_client: crate::usage::ClientFamily,
    },
}
fn legacy_dispatch(raw: &str, effects: &AtomicUsize) -> Result<()> {
    let LegacyControlMethod::ToolsCall {
        tool,
        arguments,
        selection,
        response_mode,
        usage_client,
    } = serde_json::from_str(raw)?;
    drop((tool, arguments, selection, response_mode, usage_client));
    effects.fetch_add(1, Ordering::SeqCst);
    Ok(())
}
fn fixture() -> Value {
    serde_json::from_str(CASES).unwrap()
}
fn typed_budget(raw: &str) -> Result<ReadAttemptBudget> {
    let method: ControlMethod = serde_json::from_str(raw)?;
    let ControlMethod::ToolsCall {
        read_attempt: Some(budget),
        ..
    } = method
    else {
        anyhow::bail!("typed parent budget required")
    };
    Ok(budget)
}

#[test]
fn tools_call_attempt_none_retains_all_seven_literal_m3_messages() -> Result<()> {
    let retained: Value = serde_json::from_str(RETAINED)?;
    let group = retained["variants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["variant"] == "tools_call")
        .unwrap();
    let rows = group["methods"].as_array().unwrap();
    assert_eq!(rows.len(), 7);
    for row in rows {
        let raw = row["bytes"].as_str().unwrap();
        let method: ControlMethod = serde_json::from_str(raw)?;
        assert!(matches!(
            &method,
            ControlMethod::ToolsCall {
                read_attempt: None,
                ..
            }
        ));
        assert_eq!(
            serde_json::to_string(&method)?,
            raw,
            "retained {}",
            row["name"]
        );
        assert_eq!(
            method.request_frame_budget(),
            row["request_frame_budget"].as_u64().unwrap() as usize
        );
        assert_eq!(
            method.response_frame_budget(),
            row["response_frame_budget"].as_u64().unwrap() as usize
        );
    }
    Ok(())
}

#[test]
fn tools_call_attempt_some_roundtrips_original_absolute_and_ninety_second_query() -> Result<()> {
    for row in fixture()["some"].as_array().unwrap() {
        let raw = row["bytes"].as_str().unwrap();
        let method: ControlMethod = serde_json::from_str(raw)?;
        assert_eq!(serde_json::to_string(&method)?, raw);
        let budget = typed_budget(raw)?;
        let original = budget.expires_monotonic_ns;
        let clock: ReadClock = serde_json::from_value(row["clock"].clone())?;
        assert_eq!(
            budget.remaining_at(clock, row["now_ns"].as_u64().unwrap())?,
            Duration::from_nanos(row["remaining_ns"].as_u64().unwrap())
        );
        assert_eq!(budget.expires_monotonic_ns, original);
        assert_eq!(
            budget.request_id,
            crate::identity::Incarnation::from_bytes([1; 16])
        );
        assert_eq!(
            method.request_frame_budget(),
            crate::service::MAX_TOOLS_CALL_REQUEST_BYTES
        );
        assert_eq!(
            method.response_frame_budget(),
            crate::service::MAX_TOOLS_CALL_RESPONSE_BYTES
        );
    }
    Ok(())
}

#[test]
fn tools_call_attempt_old_strict_reader_refuses_some_before_effect() -> Result<()> {
    let cases = fixture();
    let effects = AtomicUsize::new(0);
    legacy_dispatch(cases["none"].as_str().unwrap(), &effects)?;
    assert_eq!(effects.load(Ordering::SeqCst), 1);
    effects.store(0, Ordering::SeqCst);
    for row in cases["some"].as_array().unwrap() {
        assert!(legacy_dispatch(row["bytes"].as_str().unwrap(), &effects).is_err());
        assert_eq!(
            effects.load(Ordering::SeqCst),
            0,
            "old closed reader must refuse before dispatch"
        );
    }
    Ok(())
}

#[test]
fn tools_call_attempt_malformed_budget_never_reaches_validated_effect() -> Result<()> {
    let cases = fixture();
    let effects = AtomicUsize::new(0);
    for row in cases["invalid"].as_array().unwrap() {
        let clock: ReadClock = serde_json::from_value(row["clock"].clone())?;
        let admitted = typed_budget(row["bytes"].as_str().unwrap()).and_then(|budget| {
            let left = budget.remaining_at(clock, row["now_ns"].as_u64().unwrap())?;
            anyhow::ensure!(!left.is_zero(), "expired original budget");
            Ok(())
        });
        if admitted.is_ok() {
            effects.fetch_add(1, Ordering::SeqCst);
        }
        assert!(admitted.is_err(), "{} must fail closed", row["name"]);
        assert_eq!(effects.load(Ordering::SeqCst), 0);
    }
    Ok(())
}
