// SPDX-License-Identifier: Apache-2.0
//! M5-M8 (split step 3c.3b; contracts §2.4): the request journal behind the
//! service. The journal's owner, `airk_host::request_journal`, keeps its file
//! format, lock and limits; these handlers only answer over I1.
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse, PendingClientRequest, RequestKey},
};
use std::path::Path;

fn unavailable(code: &str) -> ControlResponse {
    ControlResponse::Failure { code: code.into() }
}

pub(super) fn prepare(
    control: &Path,
    caller: Incarnation,
    key: RequestKey,
    request: ControlMethod,
) -> ControlResponse {
    airk_host::request_journal::prepare_keyed(control, caller, &key, request)
        .unwrap_or_else(|_| unavailable("client_request_journal_unavailable"))
}

pub(super) fn list(control: &Path) -> ControlResponse {
    match airk_host::request_journal::list(control) {
        Ok(pending) => ControlResponse::ClientRequests {
            pending: pending
                .into_iter()
                .map(|entry| PendingClientRequest {
                    nonce: entry.nonce,
                    project: entry.project,
                    kind: entry.kind,
                })
                .collect(),
            capacity: airk_host::request_journal::CAPACITY,
        },
        Err(_) => unavailable("client_request_journal_unavailable"),
    }
}

/// M7: the saved request with its exact key, sent as `airk` sent it.
async fn send_saved(control: &Path, nonce: Incarnation) -> anyhow::Result<ControlResponse> {
    let (control, request, project, kind) = airk_host::request_journal::saved(control, nonce)?;
    let response = airk_frontend::console_client::request(&control, request).await?;
    airk_host::request_journal::check_receipt(&response, project, kind)?;
    Ok(response)
}

pub(super) async fn submit(control: &Path, nonce: Incarnation) -> ControlResponse {
    match send_saved(control, nonce).await {
        Ok(response @ (ControlResponse::Job { .. } | ControlResponse::Failure { .. })) => response,
        Ok(_) => unavailable("service_request_outcome_unknown"),
        Err(_) => unavailable("client_request_unavailable"),
    }
}

pub(super) fn forget(control: &Path, nonce: Incarnation) -> ControlResponse {
    match airk_host::request_journal::forget_existed(control, nonce) {
        Ok(existed) => ControlResponse::ClientRequestForgotten { nonce, existed },
        Err(_) => unavailable("client_request_journal_unavailable"),
    }
}
