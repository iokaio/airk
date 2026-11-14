// SPDX-License-Identifier: Apache-2.0
//! The I1 control client (split plan Option A): one framed control request
//! per call over `airk-ipc`, mapped to the I2 exit classes (contracts §3.4).
use crate::contract::{ErrorLine, ExitClass};
use airk_core::service::{ControlMethod, ControlResponse};
use std::path::Path;

/// The refusal `airk-ipc` raises when a `Health` answer names another
/// version, protocol or user.
const IDENTITY_REFUSAL: &str = "incompatible native service identity";

/// Send one control request to the service owning `runtime_dir`.
pub fn request(
    runtime_dir: &Path,
    method: ControlMethod,
    version: &str,
) -> Result<ControlResponse, ErrorLine> {
    request_fenced(runtime_dir, method, version, None)
}

/// Send one control request that the service answers only while its instance
/// is `expected_instance` (otherwise `service_instance_changed`).
pub fn request_fenced(
    runtime_dir: &Path,
    method: ControlMethod,
    version: &str,
    expected_instance: Option<airk_core::identity::Incarnation>,
) -> Result<ControlResponse, ErrorLine> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            ErrorLine::new(
                ExitClass::Internal,
                "internal",
                false,
                &format!("async runtime: {error}"),
            )
        })?;
    runtime
        .block_on(airk_ipc::control::request_once(
            runtime_dir,
            method,
            version,
            expected_instance,
        ))
        .map_err(|error| {
            if error
                .chain()
                .any(|cause| cause.to_string() == IDENTITY_REFUSAL)
            {
                return ErrorLine::new(
                    ExitClass::IncompatiblePair,
                    "service_rebind_required",
                    false,
                    "The AIRK service is another version, protocol or user than this airkctl.",
                );
            }
            let missing = error.chain().any(|cause| {
                cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
                    matches!(
                        io.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    )
                })
            });
            if missing {
                ErrorLine::new(
                    ExitClass::ServiceMissing,
                    "service_unavailable",
                    true,
                    "The AIRK service is not running for this runtime directory.",
                )
            } else {
                ErrorLine::new(
                    ExitClass::Internal,
                    "service_transport",
                    false,
                    &format!("The AIRK service request failed: {error:#}"),
                )
            }
        })
}

/// The I2 error line for an answer that is not the expected success: a typed
/// failure, an incompatible protocol, or an unexpected response. The complete
/// native response is kept.
pub fn refusal(response: &ControlResponse) -> ErrorLine {
    if response.carries_private_transport_secret() {
        return ErrorLine::new(
            ExitClass::Internal,
            "console_private_response_forbidden",
            false,
            "Unexpected private native response was refused.",
        );
    }
    let mut line = match response {
        ControlResponse::Failure { code } if code == "not_yet_served" => ErrorLine::new(
            ExitClass::UnsupportedCapability,
            code,
            false,
            "This command is not served by this AIRK Service version.",
        ),
        ControlResponse::Failure { code } => ErrorLine::new(
            ExitClass::Refused,
            code,
            code == "registry_busy" || code == "runtime_upgrade_in_progress",
            code,
        ),
        ControlResponse::Incompatible { protocol } => ErrorLine::new(
            ExitClass::IncompatiblePair,
            "service_protocol_incompatible",
            false,
            &format!(
                "The AIRK service speaks control protocol {protocol}; airkctl speaks {}.",
                airk_core::service::PROTOCOL_VERSION
            ),
        ),
        _ => ErrorLine::new(
            ExitClass::Internal,
            "unexpected_response",
            false,
            "The AIRK service answered with an unexpected response.",
        ),
    };
    line.native = serde_json::to_value(response).ok();
    line
}
