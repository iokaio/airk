// SPDX-License-Identifier: Apache-2.0
//! `airkctl service stop|restart` (U85; split step 3b.1e-3): M19 keyed by a
//! fresh `RequestEpoch` key, then wait for the reported process to exit;
//! a restart then starts the installed service (`crate::start`).
use crate::contract::{ErrorLine, ExitClass};
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse, RequestKey},
    service_lifecycle::{LifecycleAction, LifecycleRecord},
};
use std::{path::Path, time::Duration};

/// How long a stopping service has to exit after it accepted the request.
pub const EXIT: Duration = Duration::from_secs(60);

/// An accepted request whose process has exited.
pub struct Stopped {
    pub accepted: ControlResponse,
    pub record: LifecycleRecord,
    pub instance: Incarnation,
}

fn unknown(code: &str, message: impl Into<String>) -> ErrorLine {
    ErrorLine::new(ExitClass::OutcomeUnknown, code, false, &message.into())
}

/// Stop the service owning `control` (or drain it for a restart) and wait for
/// its process to exit.
pub fn stop(control: &Path, version: &str, action: LifecycleAction) -> Result<Stopped, ErrorLine> {
    let instance = match crate::service::request(control, ControlMethod::Health, version)? {
        ControlResponse::Health { observation } => observation.instance,
        other => return Err(crate::service::refusal(&other)),
    };
    let (namespace, epoch) = match crate::service::request_fenced(
        control,
        ControlMethod::RequestEpoch,
        version,
        Some(instance),
    )? {
        ControlResponse::RequestEpoch { namespace, epoch } => (namespace, epoch),
        other => return Err(crate::service::refusal(&other)),
    };
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| {
        ErrorLine::new(
            ExitClass::Internal,
            "internal",
            false,
            &format!("request nonce: {error}"),
        )
    })?;
    let caller = airk_ipc::endpoint::caller_identity().map_err(|error| {
        ErrorLine::new(
            ExitClass::Internal,
            "internal",
            false,
            &format!("caller identity: {error:#}"),
        )
    })?;
    let method = ControlMethod::Lifecycle {
        action,
        caller,
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes(nonce),
        },
    };
    let accepted = match crate::service::request_fenced(control, method, version, Some(instance)) {
        Ok(response) => response,
        // The request may have been recorded before the transport failed.
        Err(line) if line.class == ExitClass::Internal => {
            return Err(unknown("service_request_outcome_unknown", line.message));
        }
        Err(line) => return Err(line),
    };
    let ControlResponse::LifecycleAccepted {
        record, process_id, ..
    } = &accepted
    else {
        return Err(crate::service::refusal(&accepted));
    };
    let record = record.clone();
    match airk_process::detached::wait_for_exit(*process_id, EXIT) {
        Ok(true) => Ok(Stopped {
            accepted,
            record,
            instance,
        }),
        Ok(false) => Err(unknown(
            "lifecycle_exit_unobserved",
            "The AIRK service accepted the request but did not exit within 60 seconds.",
        )),
        Err(error) => Err(unknown(
            "lifecycle_exit_unobserved",
            format!("process {process_id}: {error}"),
        )),
    }
}
