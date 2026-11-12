// SPDX-License-Identifier: Apache-2.0
//! M18 `HttpIssuerRotation` (split step 3e.5): the `runtime
//! http-issuer-rotation --action status` document, read at the service's own
//! control root by the same function (`http_endpoint::rotation::output`). A
//! refusal carries the CLI's error chain.
use airk_core::service::{ControlResponse, HttpIssuerRotationStep};
use std::path::Path;

pub(super) async fn response(control: &Path, action: HttpIssuerRotationStep) -> ControlResponse {
    let control = control.to_path_buf();
    let read = tokio::task::spawn_blocking(move || match action {
        HttpIssuerRotationStep::Status => crate::http_endpoint::rotation::output(&control, None),
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|result| result);
    match read {
        Ok(result) => ControlResponse::HttpIssuerRotation { result },
        Err(error) => ControlResponse::HttpIssuerRotationRefused {
            message: super::tools_call::failure_message(&error),
        },
    }
}
