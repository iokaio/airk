// SPDX-License-Identifier: Apache-2.0
//! M9 `PrepareConfiguration` (split step 3c.5b): the CLI's configuration
//! preparation, unchanged (its inspection, pinned native inputs and checks);
//! a refusal carries the CLI's error chain.
use airk_core::{
    service::{ControlResponse, ProjectBinding},
    service_configuration::ConfigurationDraft,
};
use std::path::Path;

pub(super) async fn prepare(
    control: &Path,
    project: ProjectBinding,
    draft: ConfigurationDraft,
) -> ControlResponse {
    match airk_host::service_configuration::prepare(
        &airk_frontend::client_requests::ControlRoot(control),
        project,
        draft,
    )
    .await
    {
        Ok(request) => ControlResponse::ConfigurationPrepared {
            request: Box::new(request),
        },
        Err(error) => ControlResponse::ConfigurationRefused {
            message: super::tools_call::failure_message(&error),
        },
    }
}
