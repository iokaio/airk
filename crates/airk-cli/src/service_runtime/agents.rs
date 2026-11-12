// SPDX-License-Identifier: Apache-2.0
//! M15a `AgentEndpoint` and M15 `AgentConnectionInfo` (split step 3e.2): the
//! CLI's `http-endpoint` and `http-connection-info` reads at the service's own
//! control root, unchanged (health, endpoint and binding checks; the
//! credential stays in this process). A refusal carries the CLI's error chain.
use airk_core::{identity::Incarnation, service::ControlResponse};
use std::path::Path;

fn refused(error: &anyhow::Error) -> ControlResponse {
    ControlResponse::AgentRefused {
        message: super::tools_call::failure_message(error),
    }
}

pub(super) async fn endpoint(control: &Path) -> ControlResponse {
    let read = crate::runtime_install::http_commands::endpoint_at(control.to_path_buf()).await;
    match read.and_then(|value| Ok(serde_json::from_value(value)?)) {
        Ok(endpoint) => ControlResponse::AgentEndpoint {
            endpoint: Box::new(endpoint),
        },
        Err(error) => refused(&error),
    }
}

pub(super) async fn connection(control: &Path, id: Incarnation) -> ControlResponse {
    let read =
        crate::runtime_install::http_commands::existing_connection_at(control.to_path_buf(), id)
            .await;
    match read.and_then(|value| Ok(serde_json::from_value(value)?)) {
        Ok(connection) => ControlResponse::AgentConnectionInfo {
            connection: Box::new(connection),
        },
        Err(error) => refused(&error),
    }
}
