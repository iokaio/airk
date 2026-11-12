// SPDX-License-Identifier: Apache-2.0
//! N1 `UsageDocument` (split step 3c.5c): `settings` and `usage
//! report|policy` documents through the CLI's own readers, unchanged (paged
//! native reads and reassembly in `usage_client`, scoped connections for a
//! catalog session). A failure carries the CLI's error chain.
use airk_core::{
    native_reads::NativeRead,
    service::{ControlResponse, ProjectBinding, ScopedConnectRequest, UsageDocumentRead},
    usage::ClientFamily,
};
use std::path::Path;

pub(super) async fn document(
    control: &Path,
    project: ProjectBinding,
    catalog: Option<ScopedConnectRequest>,
    read: UsageDocumentRead,
) -> ControlResponse {
    let selection = catalog.as_ref();
    let absolute = |path: &str| {
        Path::new(path)
            .is_absolute()
            .then(|| Path::new(path).to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("usage and settings input paths must be absolute"))
    };
    let result = match read {
        UsageDocumentRead::Settings { view } => {
            crate::usage_client::settings_selected(control, project, view, selection).await
        }
        UsageDocumentRead::ValidateSettingsFile { view, path } => match absolute(&path) {
            Ok(path) => {
                crate::usage_client::validate_settings_file_selected(
                    control, project, view, &path, selection,
                )
                .await
            }
            Err(error) => Err(error),
        },
        UsageDocumentRead::Report { days, format } => {
            crate::usage_client::render_selected(control, project, days, format, None, selection)
                .await
        }
        UsageDocumentRead::ReportFile {
            days,
            format,
            price_scenario_path,
        } => match absolute(&price_scenario_path) {
            Ok(path) => {
                crate::usage_client::render_file_selected(
                    control, project, days, format, &path, selection,
                )
                .await
            }
            Err(error) => Err(error),
        },
        UsageDocumentRead::Policy => policy(control, project, selection).await,
    };
    match result {
        Ok(document) => ControlResponse::UsageDocument { document },
        Err(error) => ControlResponse::UsageReadFailed {
            message: super::tools_call::failure_message(&error),
            native: error
                .downcast_ref::<crate::usage_client::NativeReadFailure>()
                .map(|failure| failure.body().clone()),
        },
    }
}

/// `usage policy` as the CLI read it: through the scoped connection for a
/// catalog session, else an administrative native read.
async fn policy(
    control: &Path,
    project: ProjectBinding,
    selection: Option<&ScopedConnectRequest>,
) -> anyhow::Result<serde_json::Value> {
    let value = if let Some(selection) = selection {
        let mut connection = crate::project_client::Connection::connect_scoped(
            control,
            project,
            selection,
            ClientFamily::Cli,
        )
        .await?;
        connection.read_native(NativeRead::UsagePolicy).await?
    } else {
        crate::usage_client::administrative_read(
            control,
            project,
            NativeRead::UsagePolicy,
            ClientFamily::Cli,
        )
        .await?
    };
    crate::usage_client::require_native_success(value)
}
