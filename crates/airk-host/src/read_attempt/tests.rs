// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde::Deserialize;

#[test]
fn combined_members_reuse_exact_captured_expiry_and_only_narrow_children() -> Result<()> {
    use std::time::Duration;
    let base = Instant::now();
    let id = Incarnation::from_bytes([17; 16]);
    let context = ReadAttemptContext::new(base + Duration::from_nanos(2000), Default::default())
        .with_request_id(id)?;
    let first = encode_sampled(&context, id, 1010, base + Duration::from_nanos(11))?;
    assert_eq!(first.expires_monotonic_ns, 2999);
    let context = context.capture_transport_attempt(first)?;
    for (tick, instant, remaining) in [(1100, 101, 1899), (1503, 500, 1496)] {
        let member = encode_sampled(&context, id, tick, base + Duration::from_nanos(instant))?;
        assert_eq!(member.request_id, id);
        assert_eq!(member.expires_monotonic_ns, 2999);
        assert_eq!(member.remaining_ns, remaining);
    }
    let child = context.narrowed(base + Duration::from_nanos(1200));
    let narrowed = encode_sampled(&child, id, 1503, base + Duration::from_nanos(500))?;
    assert_eq!(narrowed.expires_monotonic_ns, 2203);
    assert!(
        context
            .clone()
            .capture_transport_attempt(ReadAttemptBudget {
                expires_monotonic_ns: 3000,
                ..first
            })
            .is_err()
    );
    assert!(
        encode_sampled(
            &context,
            Incarnation::from_bytes([18; 16]),
            1503,
            base + Duration::from_nanos(500)
        )
        .is_err()
    );
    assert!(encode_sampled(&context, id, 2999, base + Duration::from_nanos(1999)).is_err());
    let foreign = ReadAttemptBudget {
        clock: match clock() {
            ReadClock::UnixMonotonic => ReadClock::WindowsPerformanceCounter,
            ReadClock::WindowsPerformanceCounter => ReadClock::UnixMonotonic,
        },
        ..first
    };
    let foreign = ReadAttemptContext::new(context.deadline(), Default::default())
        .with_wire_attempt(foreign)?;
    assert!(encode_sampled(&foreign, id, 1503, base + Duration::from_nanos(500)).is_err());
    context.cancellation().cancel();
    assert!(encode_sampled(&context, id, 1503, base + Duration::from_nanos(500)).is_err());
    assert!(child.cancellation().is_cancelled());
    Ok(())
}

#[test]
fn sampled_conversion_charges_nanoseconds_and_repeated_frames_without_refresh() -> Result<()> {
    use std::time::Duration;
    let base = Instant::now();
    let context = ReadAttemptContext::new(base + Duration::from_nanos(1_000), Default::default())
        .with_native_retry_deadline(base + Duration::from_nanos(200));
    // A shared clock epoch maps base to tick 1_000. Encode's native sample
    // precedes its Instant sample by one nanosecond.
    let budget = encode_sampled(
        &context,
        Incarnation::from_bytes([3; 16]),
        1_010,
        base + Duration::from_nanos(11),
    )?;
    assert_eq!(budget.expires_monotonic_ns, 1_999);
    assert_eq!(budget.native_retry_expires_monotonic_ns, Some(1_199));
    for (instant_ns, tick, expected_retry_ns, expected_read_ns) in
        [(20, 1_022, 197, 997), (100, 1_103, 196, 996)]
    {
        // Decode's Instant sample precedes its native sample. Reusing the
        // original frame later consumes time rather than resetting its expiry.
        let decoded = decode_sampled(budget, base + Duration::from_nanos(instant_ns), tick)?;
        assert_eq!(decoded.request_id(), Some(Incarnation::from_bytes([3; 16])));
        assert_eq!(
            decoded.wire_attempt().unwrap().expires_monotonic_ns,
            budget.expires_monotonic_ns
        );
        assert_eq!(
            decoded.deadline(),
            base + Duration::from_nanos(expected_read_ns)
        );
        assert_eq!(
            decoded.native_retry_deadline(),
            Some(base + Duration::from_nanos(expected_retry_ns))
        );
        assert!(decoded.deadline() <= context.deadline());
        assert!(decoded.native_retry_deadline() <= context.native_retry_deadline());
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyQuery {
    frontend: Option<airk_core::service::RuntimeProcess>,
    tool: String,
    arguments: serde_json::Map<String, serde_json::Value>,
    native: Option<airk_mcp::local::NativeRead>,
    status_receipt: airk_mcp::response::StatusReceipt,
}

#[test]
fn feature_gate_and_absent_budget_preserve_frozen_legacy_query() -> Result<()> {
    for version in ["1.8.12", "1.8.13", "invalid", "1.8.14.extra"] {
        assert!(!supported(version));
    }
    assert!(supported("1.8.14"));
    assert!(supported("1.9.0"));
    let query = crate::project_protocol::Query {
        context_partition: None,
        read_attempt: None,
        frontend: None,
        tool: "airk_status".into(),
        arguments: Default::default(),
        native: None,
        status_receipt: Default::default(),
    };
    let value = serde_json::to_value(&query)?;
    assert!(value.get("read_attempt").is_none());
    let legacy: LegacyQuery = serde_json::from_value(value.clone())?;
    assert_eq!(legacy.tool, "airk_status");
    assert!(legacy.frontend.is_none() && legacy.native.is_none() && legacy.arguments.is_empty());
    assert_eq!(
        serde_json::to_value(legacy.status_receipt)?,
        value["status_receipt"]
    );
    let current: crate::project_protocol::Query = serde_json::from_value(value)?;
    assert!(current.read_attempt.is_none());
    let context = ReadAttemptContext::new(
        Instant::now() + std::time::Duration::from_secs(1),
        Default::default(),
    );
    let budget = encode(&context, Incarnation::from_bytes([1; 16]))?;
    assert!(decode(budget)?.deadline() <= context.deadline());
    let settings = context
        .clone()
        .with_native_retry_deadline(Instant::now() + std::time::Duration::from_millis(200));
    let settings_budget = encode(&settings, Incarnation::from_bytes([2; 16]))?;
    let original = settings.native_retry_deadline().unwrap();
    assert!(decode(settings_budget)?.native_retry_deadline().unwrap() <= original);
    assert!(
        decode(settings_budget)?.native_retry_deadline().unwrap() <= original,
        "a repeated frame must not restart the native settings loop"
    );
    let mut extended = serde_json::to_value(query)?;
    extended["read_attempt"] = serde_json::to_value(budget)?;
    assert!(
        serde_json::from_value::<LegacyQuery>(extended).is_err(),
        "gate is required for old strict readers"
    );
    Ok(())
}

#[test]
fn legacy_results_omit_internal_admission_proof() -> Result<()> {
    let result = crate::project_protocol::QueryResult {
        read_admission: None,
        result: serde_json::json!({"unchanged":true}),
        status_receipt: Default::default(),
        rebind: false,
    };
    let value = serde_json::to_value(result)?;
    assert!(value.get("read_admission").is_none());
    let current: crate::project_protocol::QueryResult = serde_json::from_value(value)?;
    assert!(current.read_admission.is_none());
    Ok(())
}
