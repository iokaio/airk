// SPDX-License-Identifier: Apache-2.0
//! `agents endpoint`, `agents info` and `agents revoke` (split step 3e.2):
//! the local HTTPS endpoint and connections, as `airk runtime http-endpoint`,
//! `http-connection-info` and `http-revoke` printed them. No command here
//! returns a credential.
use crate::contract::{ErrorLine, ExitClass};
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse},
};
use serde_json::Value;
use std::path::Path;

fn refused(message: &str) -> ErrorLine {
    ErrorLine::new(ExitClass::Refused, "operation_failed", false, message)
}

/// `airk`'s refusal for an unexpected native answer.
fn native_refusal(response: &ControlResponse) -> ErrorLine {
    let mut line = refused(&format!(
        "native HTTP setup refused: {}",
        serde_json::to_string(response).unwrap_or_default()
    ));
    line.native = serde_json::to_value(response).ok();
    line
}

fn answer(response: ControlResponse) -> Result<Value, ErrorLine> {
    match response {
        ControlResponse::AgentEndpoint { endpoint } => {
            serde_json::to_value(endpoint).map_err(|error| refused(&error.to_string()))
        }
        ControlResponse::AgentConnectionInfo { connection } => {
            serde_json::to_value(connection).map_err(|error| refused(&error.to_string()))
        }
        ControlResponse::AgentRefused { message } => Err(refused(&message)),
        other => Err(crate::service::refusal(&other)),
    }
}

/// M15a: the checked endpoint descriptor.
pub fn endpoint(control: &Path, version: &str) -> Result<Value, ErrorLine> {
    answer(crate::service::request(
        control,
        ControlMethod::AgentEndpoint {},
        version,
    )?)
}

/// M15: an existing connection's references.
pub fn info(control: &Path, version: &str, connection_id: Incarnation) -> Result<Value, ErrorLine> {
    answer(crate::service::request(
        control,
        ControlMethod::AgentConnectionInfo { connection_id },
        version,
    )?)
}

/// `http-revoke`: the service must accept requests; the revoke is fenced to
/// the observed instance and must name the connection asked for.
pub fn revoke(
    control: &Path,
    version: &str,
    connection_id: Incarnation,
) -> Result<Value, ErrorLine> {
    let health = match crate::service::request(control, ControlMethod::Health, version)? {
        ControlResponse::Health { observation } => observation,
        other => return Err(native_refusal(&other)),
    };
    if !health.policy.accepting {
        return Err(refused("native service is draining"));
    }
    let response = crate::service::request_fenced(
        control,
        ControlMethod::HttpRevoke { connection_id },
        version,
        Some(health.instance),
    )?;
    match &response {
        ControlResponse::HttpRevoked {
            connection_id: revoked,
        } if *revoked == connection_id => {
            serde_json::to_value(&response).map_err(|error| refused(&error.to_string()))
        }
        other => Err(native_refusal(other)),
    }
}
