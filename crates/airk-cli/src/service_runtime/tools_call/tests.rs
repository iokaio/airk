// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;

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
    )
    .await
    {
        ControlResponse::ToolCallFailed { message } => {
            assert_eq!(message, "project binding required")
        }
        _ => panic!("expected a typed call failure"),
    }
}
