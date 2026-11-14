// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;

fn short_answer_arguments() -> Result<Map<String, Value>> {
    let budget = airk_core::arbiter::Budget {
        deadline_ms: 2000,
        ..Default::default()
    };
    let request = airk_core::arbiter::context_action::ContextRequestV2 {
        schema: airk_core::arbiter::context_action::REQUEST_SCHEMA_V2.into(),
        question: "synthetic bounded source question".into(),
        budget,
    };
    Ok(serde_json::to_value(request)?.as_object().unwrap().clone())
}
#[test]
fn singleton_answer_narrows_ninety_second_wire_before_local_dispatch_and_never_refreshes_it()
-> Result<()> {
    let id = Incarnation::from_bytes([45; 16]);
    let parent = ReadAttemptContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(90),
        Default::default(),
    )
    .with_request_id(id)?;
    let original = airk_host::read_attempt::encode(&parent, id)?;
    let parent = parent.capture_transport_attempt(original)?;
    let captured = admit_context("airk_answer", &short_answer_arguments()?, parent.clone())?;
    let first = captured.captured_transport_attempt().unwrap();
    assert_eq!(first.request_id, id);
    assert_eq!(first.clock, original.clock);
    assert!(first.expires_monotonic_ns < original.expires_monotonic_ns);
    assert!(first.remaining_ns <= 2_000_000_000);
    let after = airk_process::monotonic::now_ns()?;
    assert!(first.expires_monotonic_ns <= after.saturating_add(2_000_000_000));
    assert!(captured.deadline() < parent.deadline());
    let again = admit_context("answer", &short_answer_arguments()?, captured.clone())?;
    let second = again.captured_transport_attempt().unwrap();
    assert_eq!(second.request_id, id);
    assert_eq!(second.expires_monotonic_ns, first.expires_monotonic_ns);
    assert!(second.remaining_ns <= first.remaining_ns);
    parent.cancellation().cancel();
    assert!(captured.cancellation().is_cancelled());
    assert!(admit_context("airk_answer", &short_answer_arguments()?, captured).is_err());
    Ok(())
}
#[test]
fn singleton_admission_cannot_restore_an_earlier_deadline_or_rebind_original_identity() -> Result<()>
{
    let id = Incarnation::from_bytes([46; 16]);
    let parent = ReadAttemptContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(90),
        Default::default(),
    )
    .with_request_id(id)?;
    let original = airk_host::read_attempt::encode(&parent, id)?;
    let parent = parent.capture_transport_attempt(original)?;
    let earlier = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let context = admit_context(
        "airk_answer",
        &short_answer_arguments()?,
        parent.narrowed(earlier),
    )?;
    assert_eq!(context.deadline(), earlier);
    let wire = context.captured_transport_attempt().unwrap();
    assert_eq!(wire.request_id, id);
    assert!(wire.remaining_ns <= 1_000_000_000);
    let expired = context.narrowed(std::time::Instant::now());
    assert!(admit_context("answer", &short_answer_arguments()?, expired).is_err());
    Ok(())
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn both_wire_modes_map_to_their_mcp_mode() {
    assert_eq!(response_mode(ToolResponseMode::Terse), ResponseMode::Terse);
    assert_eq!(response_mode(ToolResponseMode::Full), ResponseMode::Full);
}

#[test]
fn failure_messages_keep_the_chain_and_the_cli_bound() {
    let error = anyhow::anyhow!("root cause").context("outer");
    assert_eq!(failure_message(&error), "outer: root cause");
    let long = anyhow::anyhow!("é".repeat(MAX_MESSAGE_CHARS + 10));
    assert_eq!(failure_message(&long).chars().count(), MAX_MESSAGE_CHARS);
}

#[tokio::test]
async fn a_selection_that_cannot_resolve_is_a_typed_failure() {
    let missing = tempfile::tempdir().unwrap().path().join("missing-control");
    let response = response(
        &missing,
        "airk_status".into(),
        Map::new(),
        ToolSelection::Project {
            binding: "ab".repeat(32).parse().unwrap(),
        },
        ToolResponseMode::Full,
        airk_core::usage::ClientFamily::Cli,
        read_context(None).unwrap(),
    )
    .await;
    match response {
        ControlResponse::ToolCallFailed { message } => assert!(!message.is_empty()),
        _ => panic!("expected a typed call failure"),
    }
}

#[tokio::test]
async fn without_a_project_only_the_in_process_help_answers() {
    let nowhere = tempfile::tempdir().unwrap().path().join("no-control");
    let arguments = object(json!({"topic": "workbench"}));
    for tool in ["help", "airk_help"] {
        match response(
            &nowhere,
            tool.into(),
            arguments.clone(),
            ToolSelection::Unselected,
            ToolResponseMode::Terse,
            airk_core::usage::ClientFamily::Cli,
            read_context(None).unwrap(),
        )
        .await
        {
            ControlResponse::ToolResult { result } => {
                assert_eq!(result, local_help(arguments.clone()).unwrap())
            }
            _ => panic!("expected the in-process help answer"),
        }
    }
    match response(
        &nowhere,
        "airk_status".into(),
        Map::new(),
        ToolSelection::Unselected,
        ToolResponseMode::Terse,
        airk_core::usage::ClientFamily::Cli,
        read_context(None).unwrap(),
    )
    .await
    {
        ControlResponse::ToolCallFailed { message } => {
            assert_eq!(message, "project binding required")
        }
        _ => panic!("expected a typed call failure"),
    }
}

#[test]
fn combined_defaults_keep_the_cli_shape_and_need_projects_to_narrow() {
    use airk_core::combined_reads::MemberAvailability;
    let project: airk_core::index_catalog::QueryProjectId =
        serde_json::from_value(json!(format!("prj_{}", "1a".repeat(16)))).unwrap();
    let index: airk_core::index_catalog::IndexId =
        serde_json::from_value(json!(format!("idx_{}", "1a".repeat(16)))).unwrap();
    assert_eq!(combined_defaults(None, None, None).unwrap(), Map::new());
    assert_eq!(
        Value::Object(
            combined_defaults(
                Some(vec![project]),
                Some(vec![index]),
                Some(MemberAvailability::Require)
            )
            .unwrap()
        ),
        json!({"project_ids": [project], "index_ids": [index], "member_availability": "require"})
    );
    assert!(combined_defaults(None, Some(vec![index]), None).is_err());
    assert!(combined_defaults(None, None, Some(MemberAvailability::Report)).is_err());
}

#[test]
fn supplied_short_and_long_parents_are_validated_before_selection() {
    let host_clock = if cfg!(windows) {
        airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
    } else {
        airk_core::read_attempt::ReadClock::UnixMonotonic
    };
    let now = airk_process::monotonic::now_ns().unwrap();
    for seconds in [2u64, 90] {
        let context = read_context(Some(airk_core::read_attempt::ReadAttemptBudget {
            schema: 1,
            request_id: Incarnation::from_bytes([9; 16]),
            clock: host_clock,
            expires_monotonic_ns: now + seconds * 1_000_000_000,
            remaining_ns: seconds * 1_000_000_000,
            native_retry_expires_monotonic_ns: None,
        }))
        .unwrap();
        let remaining = context
            .deadline()
            .saturating_duration_since(std::time::Instant::now());
        assert!(remaining <= std::time::Duration::from_secs(seconds));
        if seconds == 90 {
            assert!(remaining > std::time::Duration::from_secs(30));
        }
    }
    assert!(
        read_context(Some(airk_core::read_attempt::ReadAttemptBudget {
            schema: 1,
            request_id: Incarnation::from_bytes([9; 16]),
            clock: host_clock,
            expires_monotonic_ns: now,
            remaining_ns: 2_000_000_000,
            native_retry_expires_monotonic_ns: None
        }))
        .is_err()
    );
}
