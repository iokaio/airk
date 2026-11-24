// SPDX-License-Identifier: Apache-2.0
//! N1 `UsageDocument` (split step 3c.5c): `settings` and `usage
//! report|policy` documents through the CLI's own readers, unchanged (paged
//! native reads and reassembly in `usage_client`). A failure carries the
//! CLI's error chain.
use airk_core::{
    native_reads::NativeRead,
    service::{ControlResponse, ProjectBinding, UsageDocumentRead},
    usage::ClientFamily,
};
use std::path::Path;

pub(super) async fn document(
    control: &Path,
    project: ProjectBinding,
    read: UsageDocumentRead,
) -> ControlResponse {
    let absolute = |path: &str| {
        Path::new(path)
            .is_absolute()
            .then(|| Path::new(path).to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("usage and settings input paths must be absolute"))
    };
    let result = match read {
        UsageDocumentRead::Settings { view } => {
            airk_frontend::usage_client::settings(control, project, view).await
        }
        UsageDocumentRead::ValidateSettingsFile { view, path } => match absolute(&path) {
            Ok(path) => {
                airk_frontend::usage_client::validate_settings_file(control, project, view, &path)
                    .await
            }
            Err(error) => Err(error),
        },
        UsageDocumentRead::Report { days, format } => {
            airk_frontend::usage_client::render(control, project, days, format, None).await
        }
        UsageDocumentRead::ReportFile {
            days,
            format,
            price_scenario_path,
        } => match absolute(&price_scenario_path) {
            Ok(path) => {
                airk_frontend::usage_client::render_file(control, project, days, format, &path)
                    .await
            }
            Err(error) => Err(error),
        },
        UsageDocumentRead::Policy => policy(control, project).await,
    };
    match result {
        Ok(document) => ControlResponse::UsageDocument { document },
        Err(error) => ControlResponse::UsageReadFailed {
            message: super::tools_call::failure_message(&error),
            native: error
                .downcast_ref::<airk_frontend::usage_client::NativeReadFailure>()
                .map(|failure| failure.body().clone()),
        },
    }
}

/// `usage policy` as the CLI read it: an administrative native read.
async fn policy(control: &Path, project: ProjectBinding) -> anyhow::Result<serde_json::Value> {
    let value = airk_frontend::usage_client::administrative_read(
        control,
        project,
        NativeRead::UsagePolicy,
        ClientFamily::Cli,
    )
    .await?;
    airk_frontend::usage_client::require_native_success(value)
}
