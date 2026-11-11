// SPDX-License-Identifier: Apache-2.0
//! The reviewed session frames in `fixtures/wire/session_frames.json` are the
//! independent expectation for both this definition and the extension client.
use super::*;
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../../fixtures/wire/session_frames.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("session fixture is JSON")
}

fn cases<'a>(fixture: &'a Value, key: &str) -> &'a [Value] {
    let cases = fixture[key].as_array().expect("fixture case list");
    assert!(!cases.is_empty(), "{key}: zero selected cases is a failure");
    cases
}

#[test]
fn fixture_names_this_contract_and_protocol() {
    let fixture = fixture();
    assert_eq!(fixture["schema"], "airk.wire-fixture.session-frames/1");
    assert_eq!(fixture["protocol"], SESSION_PROTOCOL);
}

#[test]
fn reviewed_requests_round_trip_exactly() {
    let fixture = fixture();
    for case in cases(&fixture, "requests") {
        let parsed: SessionRequest = serde_json::from_value(case["json"].clone())
            .unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            case["json"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn reviewed_invalid_requests_are_refused() {
    let fixture = fixture();
    for case in cases(&fixture, "invalid_requests") {
        assert!(
            serde_json::from_value::<SessionRequest>(case["json"].clone()).is_err(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn reviewed_responses_round_trip_exactly() {
    let fixture = fixture();
    for case in cases(&fixture, "responses") {
        let parsed: SessionResponse = serde_json::from_value(case["json"].clone())
            .unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            case["json"],
            "{}",
            case["name"]
        );
        if let SessionOutcome::Failed { code, retryable } = parsed.outcome {
            assert_eq!(code.retryable(), retryable, "{}", case["name"]);
        }
    }
}

#[test]
#[allow(clippy::assertions_on_constants)] // The reviewed budgets are the subject.
fn budgets_fit_the_shared_codec() {
    assert!(MAX_RESPONSE_FRAME_BYTES <= crate::service_ipc::MAX_CODEC_BYTES);
    assert!(MAX_REQUEST_FRAME_BYTES <= crate::service_ipc::MAX_CODEC_BYTES);
    // Quote/backslash escaping of the stdin budget fits; the encoded frame
    // limit remains authoritative for control-character-heavy input.
    assert!(MAX_REQUEST_FRAME_BYTES >= MAX_INVOKE_STDIN_BYTES * 2);
}

#[test]
fn budgets_equal_the_reviewed_fixture() {
    let fixture = fixture();
    let budgets = &fixture["budgets"];
    for (name, value) in [
        ("max_request_frame_bytes", MAX_REQUEST_FRAME_BYTES as u64),
        ("max_invoke_stdin_bytes", MAX_INVOKE_STDIN_BYTES as u64),
        ("max_invoke_args", MAX_INVOKE_ARGS as u64),
        ("max_invoke_arg_bytes", MAX_INVOKE_ARG_BYTES as u64),
        ("max_response_frame_bytes", MAX_RESPONSE_FRAME_BYTES as u64),
        ("max_invoke_stderr_bytes", MAX_INVOKE_STDERR_BYTES as u64),
        ("max_in_flight", MAX_IN_FLIGHT as u64),
        ("max_invoke_deadline_ms", MAX_INVOKE_DEADLINE_MS),
    ] {
        assert_eq!(budgets[name], value, "{name}");
    }
    assert_eq!(budgets.as_object().map(|b| b.len()), Some(8));
}
