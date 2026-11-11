// SPDX-License-Identifier: Apache-2.0
//! `airkctl service record-error` (M14; U75; split step 3b.2): append one
//! bounded client error. The running service records it through M14; while it
//! is down, the verified `airk` runs the private `record-error` role, which
//! appends through the same writer.
use crate::contract::{ErrorLine, ExitClass};
use airk_core::service::{ControlMethod, ControlResponse, MAX_CLIENT_ERROR_INPUT_BYTES};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

/// Whether this call reached the running service.
pub enum Recorded {
    ByService,
    ServiceDown,
}

/// Ask the running service to record the error; `ServiceDown` when none answers.
pub fn by_service(
    control: &Path,
    operation: &str,
    input: &[u8],
    version: &str,
) -> Result<Recorded, ErrorLine> {
    let input = std::str::from_utf8(input).map_err(|_| {
        ErrorLine::new(
            ExitClass::Usage,
            "usage",
            false,
            "the client error must be UTF-8 JSON",
        )
    })?;
    let method = ControlMethod::RecordClientError {
        operation: operation.into(),
        input: input.into(),
    };
    match crate::service::request(control, method, version) {
        Ok(ControlResponse::ClientErrorRecorded {}) => Ok(Recorded::ByService),
        Ok(other) => Err(crate::service::refusal(&other)),
        Err(line) if line.class == ExitClass::ServiceMissing => Ok(Recorded::ServiceDown),
        Err(line) => Err(line),
    }
}

/// Record the error through the verified `airk` image's private role.
pub fn by_role(
    airk: &Path,
    control: &Path,
    operation: &str,
    input: &[u8],
) -> Result<(), ErrorLine> {
    let failed = |message: String| {
        ErrorLine::new(
            ExitClass::Refused,
            "client_error_not_recorded",
            false,
            &message,
        )
    };
    let mut child = Command::new(airk)
        .args(["__airk_role", "record-error", "--runtime-dir"])
        .arg(control)
        .args(["--operation", operation])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| failed(format!("{}: {error}", airk.display())))?;
    let written = child.stdin.take().map(|mut stdin| stdin.write_all(input));
    let output = child
        .wait_with_output()
        .map_err(|error| failed(error.to_string()))?;
    if let Some(Err(error)) = written {
        return Err(failed(error.to_string()));
    }
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(failed(
        stderr
            .lines()
            .last()
            .unwrap_or("the record-error role failed")
            .trim_start_matches("Error: ")
            .to_owned(),
    ))
}

/// The bounded stdin input, as `airk service record-error` read it.
pub fn bounded(input: &mut impl Read) -> Result<Vec<u8>, ErrorLine> {
    let mut bytes = Vec::new();
    input
        .take(MAX_CLIENT_ERROR_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            ErrorLine::new(ExitClass::Internal, "internal", false, &error.to_string())
        })?;
    if bytes.len() > MAX_CLIENT_ERROR_INPUT_BYTES {
        return Err(ErrorLine::new(
            ExitClass::Usage,
            "usage",
            false,
            "input exceeds 16384 bytes",
        ));
    }
    Ok(bytes)
}
