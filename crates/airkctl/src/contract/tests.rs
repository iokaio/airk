// SPDX-License-Identifier: Apache-2.0
//! The reviewed fixtures are the independent expectation for the I2 contract
//! types; these tests only compare against them.
use super::*;
use serde_json::Value;

const FRAMES: &str = include_str!("../../../../fixtures/wire/airkctl_session_frames.json");
const EXIT_CODES: &str = include_str!("../../../../fixtures/split/i2/exit_codes.json");
const ERROR_LINES: &str = include_str!("../../../../fixtures/split/i2/error_lines.json");
const VERSION: &str = include_str!("../../../../fixtures/split/i2/version.json");

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("fixture is JSON")
}

fn cases<'a>(fixture: &'a Value, key: &str) -> &'a [Value] {
    let cases = fixture[key].as_array().expect("fixture case list");
    assert!(!cases.is_empty(), "{key}: zero selected cases is a failure");
    cases
}

fn round_trip<T: serde::de::DeserializeOwned + Serialize>(case: &Value) -> T {
    let parsed: T = serde_json::from_value(case["json"].clone())
        .unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
    assert_eq!(
        serde_json::to_value(&parsed).unwrap(),
        case["json"],
        "{}",
        case["name"]
    );
    parsed
}

#[test]
fn session_fixture_names_this_contract_and_budgets() {
    let fixture = json(FRAMES);
    assert_eq!(
        fixture["schema"],
        "airk.wire-fixture.airkctl-session-frames/1"
    );
    assert_eq!(fixture["protocol"], SESSION_PROTOCOL);
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
        ("max_buffered_events", MAX_BUFFERED_EVENTS as u64),
        ("max_buffered_event_bytes", MAX_BUFFERED_EVENT_BYTES as u64),
    ] {
        assert_eq!(budgets[name], value, "{name}");
    }
}

#[test]
fn session_requests_round_trip_and_invalid_ones_are_refused() {
    let fixture = json(FRAMES);
    for case in cases(&fixture, "requests") {
        round_trip::<SessionRequest>(case);
    }
    for case in cases(&fixture, "invalid_requests") {
        assert!(
            serde_json::from_value::<SessionRequest>(case["json"].clone()).is_err(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn session_responses_round_trip_with_consistent_retryability() {
    let fixture = json(FRAMES);
    for case in cases(&fixture, "responses") {
        let parsed: SessionResponse = round_trip(case);
        match parsed.outcome {
            SessionOutcome::Failed { code, retryable } => {
                assert_eq!(code.retryable(), retryable, "{}", case["name"]);
            }
            SessionOutcome::Invoked {
                exit_code,
                class,
                error,
                ..
            } => {
                assert_eq!(class.code(), exit_code, "{}", case["name"]);
                if let Some(error) = error {
                    assert_eq!(error.class, class, "{}", case["name"]);
                    error.validate().unwrap();
                }
            }
            _ => {}
        }
    }
}

#[test]
fn session_events_round_trip_and_unknown_ones_are_refused() {
    let fixture = json(FRAMES);
    for case in cases(&fixture, "events") {
        round_trip::<SessionEventFrame>(case);
    }
    for case in cases(&fixture, "invalid_events") {
        assert!(
            serde_json::from_value::<SessionEventFrame>(case["json"].clone()).is_err(),
            "{}",
            case["name"]
        );
    }
}

fn keys(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                found.push(key.clone());
                keys(value, found);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| keys(item, found)),
        _ => {}
    }
}

#[test]
fn no_session_frame_carries_a_token_lease_or_launch_code() {
    let mut found = Vec::new();
    keys(&json(FRAMES), &mut found);
    for key in found {
        for forbidden in [
            "token",
            "bearer",
            "credential",
            "lease",
            "launch_code",
            "grant",
        ] {
            assert!(
                !key.contains(forbidden),
                "session frame key {key} is forbidden"
            );
        }
    }
}

#[test]
fn exit_codes_match_the_reviewed_table() {
    let fixture = json(EXIT_CODES);
    let rows = cases(&fixture, "codes");
    assert_eq!(rows.len(), ExitClass::ALL.len());
    for row in rows {
        let code = u8::try_from(row["code"].as_u64().unwrap()).unwrap();
        let class = ExitClass::from_code(code).unwrap_or_else(|| panic!("code {code}"));
        assert_eq!(serde_json::to_value(class).unwrap(), row["class"]);
        assert_eq!(
            serde_json::to_value(class.mutation_possible()).unwrap(),
            row["mutation_possible"]
        );
    }
    for code in cases(&fixture, "unassigned") {
        let code = u8::try_from(code.as_u64().unwrap()).unwrap();
        assert_eq!(ExitClass::from_code(code), None, "{code}");
    }
}

#[test]
fn error_lines_decode_validate_and_refuse_as_reviewed() {
    let fixture = json(ERROR_LINES);
    assert_eq!(fixture["max_message_chars"], MAX_ERROR_MESSAGE_CHARS as u64);
    for case in cases(&fixture, "valid") {
        let line: ErrorLine = round_trip(case);
        line.validate()
            .unwrap_or_else(|reason| panic!("{}: {reason}", case["name"]));
    }
    for case in cases(&fixture, "invalid") {
        assert!(
            serde_json::from_value::<ErrorLine>(case["json"].clone()).is_err(),
            "{}",
            case["name"]
        );
    }
    for case in cases(&fixture, "rejected_after_decoding") {
        let line: ErrorLine = serde_json::from_value(case["json"].clone()).unwrap();
        assert_eq!(
            line.validate(),
            Err(case["reason"].as_str().unwrap()),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn error_line_messages_are_bounded_by_characters() {
    let long = "é".repeat(MAX_ERROR_MESSAGE_CHARS + 10);
    let line = ErrorLine::new(ExitClass::Internal, "internal", false, &long);
    assert_eq!(line.message.chars().count(), MAX_ERROR_MESSAGE_CHARS);
    line.validate().unwrap();
}

#[test]
fn version_documents_round_trip_and_tolerate_unknown_fields() {
    let fixture = json(VERSION);
    for case in cases(&fixture, "documents") {
        round_trip::<VersionDocument>(case);
    }
    for case in cases(&fixture, "forward_compatible") {
        serde_json::from_value::<VersionDocument>(case["json"].clone())
            .unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
    }
    let values = &fixture["contract_values"];
    assert_eq!(values["schema"], VERSION_SCHEMA);
    assert_eq!(values["product"], PRODUCT);
    assert_eq!(values["i2_contract"], I2_CONTRACT);
    assert_eq!(
        values["i2_contracts_supported"],
        serde_json::to_value(I2_CONTRACTS_SUPPORTED).unwrap()
    );
    assert_eq!(values["session_protocol"], SESSION_PROTOCOL);
}
