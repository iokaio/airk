// SPDX-License-Identifier: Apache-2.0
//! M15a `AgentEndpoint` (split step 3e.2): the CLI's `http-endpoint` read at
//! the service's own control root, unchanged (health, endpoint and binding
//! checks). A refusal carries the CLI's error chain. The connection reads
//! (M15, M16, V1) were removed with the credential (local trust plan B).
use airk_core::service::ControlResponse;
use std::path::Path;

fn refused(error: &anyhow::Error) -> ControlResponse {
    ControlResponse::AgentRefused {
        message: super::tools_call::failure_message(error),
    }
}

pub(super) async fn endpoint(control: &Path) -> ControlResponse {
    let read = airk_https::runtime_http_commands::endpoint_at(control.to_path_buf()).await;
    match read.and_then(|value| Ok(serde_json::from_value(value)?)) {
        Ok(endpoint) => {
            let endpoint: airk_core::service_http::EndpointDescriptor = endpoint;
            let manual = airk_core::agent_manual::project_entries(endpoint.url);
            ControlResponse::AgentEndpoint {
                endpoint: Box::new(endpoint),
                manual,
            }
        }
        Err(error) => refused(&error),
    }
}
