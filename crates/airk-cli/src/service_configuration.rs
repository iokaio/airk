// SPDX-License-Identifier: Apache-2.0
//! `airk_host::service_configuration` with this crate's control-root calls.
//!
//! The trait `ConfigurationReads` lives in `airk-host`, so the CLI's
//! implementation for a control root is the local `ControlRoot` (the orphan
//! rule). `prepare` keeps its control-root signature for every caller.
pub use airk_host::service_configuration::*;

use airk_core::{service::ProjectBinding, service_configuration::ConfigurationRequest};
use anyhow::Result;
use std::path::Path;

/// Prepare a configuration request for the service whose control root is
/// `control`; the same preparation as before the crate split.
pub async fn prepare(
    control: &Path,
    project: ProjectBinding,
    draft: Draft,
) -> Result<ConfigurationRequest> {
    airk_host::service_configuration::prepare(
        &crate::client_requests::ControlRoot(control),
        project,
        draft,
    )
    .await
}
