// SPDX-License-Identifier: Apache-2.0
//! `agents endpoint`, `agents info` and `agents revoke` (split step 3e.2):
//! the local HTTPS endpoint and connections, as `airk runtime http-endpoint`,
//! `http-connection-info` and `http-revoke` printed them. None returns a
//! credential except `agents credential` (3e.3), the offline header helper.
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

/// `agents credential` (U75, U90, U92): the bearer header of an existing,
/// unrevoked connection for an agent's header helper. It reads only the
/// owner-private ledger and token under `<control>/http-1`, needs no running
/// service, never enrolls, rotates or writes, and no error contains the token.
/// The ledger must be bound to this control root, this user and its own
/// namespace; the service still checks the token against its live namespace.
pub fn credential(control: &Path, connection_id: Incarnation) -> Result<Value, ErrorLine> {
    use airk_core::service_http_credentials::{self as credentials, InstallationBinding, Ledger};
    let fail = |error: String| refused(&error);
    let private = control.join("http-1");
    airk_ipc::endpoint::require_existing_root(&private)
        .map_err(|error| fail(format!("{error:#}")))?;
    let ledger = private_file(&private.join(credentials::STORE), credentials::MAX_STORE)
        .map_err(fail)?
        .ok_or_else(|| fail("HTTP credential ledger is unobserved".into()))?;
    let ledger = Ledger::parse(&ledger).map_err(|error| fail(format!("{error:#}")))?;
    let user = airk_ipc::endpoint::current_user().map_err(|error| fail(format!("{error:#}")))?;
    let canonical = control
        .canonicalize()
        .map_err(|error| fail(error.to_string()))?;
    let binding = InstallationBinding::derive(&canonical, &user, ledger.binding.namespace)
        .map_err(|error| fail(format!("{error:#}")))?;
    ledger
        .validate(&binding)
        .map_err(|error| fail(format!("{error:#}")))?;
    let bytes = private_file(
        &private.join(credentials::filename(connection_id)),
        credentials::MAX_TOKEN,
    )
    .map_err(fail)?
    .ok_or_else(|| fail("HTTP client credential unavailable".into()))?;
    let token = credentials::client_token(bytes, connection_id)
        .map_err(|error| fail(format!("{error:#}")))?;
    let now: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| fail(error.to_string()))?
        .as_millis()
        .try_into()
        .map_err(|_| fail("clock out of range".into()))?;
    let authenticated = ledger
        .authenticate(&token, now)
        .map_err(|error| fail(format!("{error:#}")))?;
    if authenticated.connection_id != connection_id {
        return Err(fail("native HTTP connection identity changed".into()));
    }
    Ok(serde_json::json!({ "Authorization": format!("Bearer {token}") }))
}

/// An owner-private file within `limit` bytes, `None` when absent, with the
/// messages `airk`'s HTTPS private-record reader gives.
fn private_file(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, String> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(metadata) if metadata.is_file() && metadata.len() > limit as u64 => {
            return Err("HTTP private record exceeds bound".into());
        }
        Ok(_) => {}
    }
    crate::install::owned_bytes(path, limit).map(Some)
}
