// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;

fn short_answer_arguments() -> Result<Map<String, Value>> {
    let budget = airk_core::arbiter::answer_loop::BudgetV3 {
        deadline_ms: 2000,
        ..Default::default()
    };
    let request = airk_core::arbiter::answer_loop::ContextRequestV3 {
        needs: Vec::new(),
        anchors: Vec::new(),
        deliverable: None,
        known: Vec::new(),
        schema: airk_core::arbiter::answer_loop::REQUEST_SCHEMA_V3.into(),
        question: "synthetic bounded source question".into(),
        budget,
        continuation: None,
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
        read_context("airk_status", None).unwrap(),
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
            read_context("airk_status", None).unwrap(),
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
        read_context("airk_status", None).unwrap(),
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
fn supplied_short_and_long_parents_are_validated_before_selection() {
    let host_clock = if cfg!(windows) {
        airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
    } else {
        airk_core::read_attempt::ReadClock::UnixMonotonic
    };
    let now = airk_process::monotonic::now_ns().unwrap();
    for seconds in [2u64, 90] {
        let context = read_context(
            "airk_search",
            Some(airk_core::read_attempt::ReadAttemptBudget {
                schema: 1,
                request_id: Incarnation::from_bytes([9; 16]),
                clock: host_clock,
                expires_monotonic_ns: now + seconds * 1_000_000_000,
                remaining_ns: seconds * 1_000_000_000,
                native_retry_expires_monotonic_ns: None,
            }),
        )
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
        read_context(
            "airk_search",
            Some(airk_core::read_attempt::ReadAttemptBudget {
                schema: 1,
                request_id: Incarnation::from_bytes([9; 16]),
                clock: host_clock,
                expires_monotonic_ns: now,
                remaining_ns: 2_000_000_000,
                native_retry_expires_monotonic_ns: None
            })
        )
        .is_err()
    );
}

#[test]
fn explicit_subset_controls_use_native_combined_owner_even_for_one_index() {
    for arguments in [
        json!({"project_ids":[]}),
        json!({"index_ids":null}),
        json!({"member_availability":"invalid"}),
        json!({"path":"a","project_ids":["untrusted"],"index_ids":["foreign"]}),
    ] {
        assert!(uses_combined_arguments(arguments.as_object().unwrap()));
    }
    for arguments in [
        json!({}),
        json!({"query":"q","limit_per_index":2}),
        json!({"path":"a","revision":"original","known_hash":"original"}),
    ] {
        assert!(!uses_combined_arguments(arguments.as_object().unwrap()));
    }
}

#[test]
fn answer_parents_run_to_their_own_ceiling_and_other_tools_keep_125_seconds() {
    let host_clock = if cfg!(windows) {
        airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
    } else {
        airk_core::read_attempt::ReadClock::UnixMonotonic
    };
    let now = airk_process::monotonic::now_ns().unwrap();
    let parent = |seconds: u64| airk_core::read_attempt::ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([10; 16]),
        clock: host_clock,
        expires_monotonic_ns: now + seconds * 1_000_000_000,
        remaining_ns: seconds * 1_000_000_000,
        native_retry_expires_monotonic_ns: None,
    };
    let left = |context: ReadAttemptContext| {
        context
            .deadline()
            .saturating_duration_since(std::time::Instant::now())
    };
    let answer = read_context("airk_answer", Some(parent(600))).unwrap();
    assert!(left(answer) > std::time::Duration::from_secs(590));
    assert!(read_context("airk_search", Some(parent(600))).is_err());
    assert!(read_context("airk_answer", Some(parent(606))).is_err());
    let minted = read_context("airk_answer", None).unwrap();
    assert!(left(minted.clone()) > std::time::Duration::from_secs(600));
    assert_eq!(minted.read_ceiling(), std::time::Duration::from_secs(605));
    let minted = read_context("airk_search", None).unwrap();
    assert!(left(minted.clone()) <= std::time::Duration::from_secs(125));
    assert_eq!(minted.read_ceiling(), std::time::Duration::from_secs(125));
}

/// Change-log entry 67: a selection or open backend whose setup failed keeps
/// a typed pre-query transient's own retryable code; everything else is the
/// scope refusal, and untyped text never becomes a code.
#[test]
fn selection_setup_keeps_typed_transients_and_refuses_the_rest() {
    use airk_catalog_control::{
        catalog_admission::CatalogAdmissionPending,
        catalog_coordinator::{CatalogClockPending, CatalogRevisionChanged},
    };
    use airk_supervisor::project_supervisor::ConnectionSetupPending;
    let code = |error: anyhow::Error| match setup_refusal(error.context("selection setup")) {
        SelectionRead::Refused(code) => code,
        SelectionRead::Failed(_) => "failed",
    };
    assert_eq!(
        code(anyhow::Error::new(ConnectionSetupPending::Queue)),
        "catalog_connection_busy"
    );
    assert_eq!(
        code(anyhow::Error::new(CatalogRevisionChanged)),
        "catalog_revision_changed"
    );
    assert_eq!(
        code(anyhow::Error::new(CatalogClockPending)),
        "catalog_clock_pending"
    );
    assert_eq!(
        code(anyhow::Error::new(CatalogAdmissionPending)),
        "catalog_connection_busy"
    );
    assert_eq!(
        code(anyhow::Error::new(
            airk_core::scoped_reads::ScopeReadRefused
        )),
        "scoped_read_unavailable"
    );
    assert_eq!(
        code(anyhow::anyhow!("catalog_clock_pending")),
        "scoped_read_unavailable"
    );
}
