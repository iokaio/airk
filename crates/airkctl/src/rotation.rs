// SPDX-License-Identifier: Apache-2.0
//! `service issuer-rotation status` (split step 3e.5, M18): the saved issuer
//! rotation and current endpoint, as `airk runtime http-issuer-rotation
//! --action status` printed them, read by the running service at its own
//! control root. The other rotation actions and the offline `recover` are not
//! served yet (contracts §2.3 M18, §3.9).
use crate::contract::{ErrorLine, ExitClass};
use airk_core::service::{ControlMethod, ControlResponse, HttpIssuerRotationStep};
use serde_json::Value;
use std::path::Path;

/// M18 `Status`: the CLI's document, or its refusal as `operation_failed`.
pub fn status(control: &Path, version: &str) -> Result<Value, ErrorLine> {
    match crate::service::request(
        control,
        ControlMethod::HttpIssuerRotation {
            action: HttpIssuerRotationStep::Status,
        },
        version,
    )? {
        ControlResponse::HttpIssuerRotation { result } => Ok(result),
        ControlResponse::HttpIssuerRotationRefused { message } => Err(ErrorLine::new(
            ExitClass::Refused,
            "operation_failed",
            false,
            &message,
        )),
        other => Err(crate::service::refusal(&other)),
    }
}
