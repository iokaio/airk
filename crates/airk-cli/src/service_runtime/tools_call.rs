// SPDX-License-Identifier: Apache-2.0
//! The M3 `ToolsCall` handler (split step 3a.2b; `docs/split/contracts.md`
//! §2.2): resolves the selection as `--service-project` / `--catalog-session`
//! with `--service-runtime-dir` did, then runs the shared CLI call path
//! (`crate::tool_call`). It lives with the service dispatch because it uses
//! the same project client and front-end resolution the dispatch already uses.
use crate::tool_call::{call, is_local_help, local_help, project_backend, require_envelope};
use airk_core::service::{ControlResponse, ProjectBinding, ToolResponseMode, ToolSelection};
use airk_mcp::response::ResponseMode;
use anyhow::Result;
use serde_json::{Map, Value};
use std::path::Path;

/// The longest M3 failure message, as the CLI's error line bounded it.
const MAX_MESSAGE_CHARS: usize = 4096;

fn response_mode(mode: ToolResponseMode) -> ResponseMode {
    match mode {
        ToolResponseMode::Terse => ResponseMode::Terse,
        ToolResponseMode::Full => ResponseMode::Full,
    }
}

/// The M3 failure message: the full error chain, bounded as the CLI's error
/// line bounded it.
fn failure_message(error: &anyhow::Error) -> String {
    format!("{error:#}")
        .chars()
        .take(MAX_MESSAGE_CHARS)
        .collect()
}

/// Answer M3 for the service whose control root is `control`.
pub(super) async fn response(
    control: &Path,
    tool: String,
    arguments: Map<String, Value>,
    selection: ToolSelection,
    mode: ToolResponseMode,
    client: airk_core::usage::ClientFamily,
) -> ControlResponse {
    // Combined selections land with their handler in step 3a.2d.
    if matches!(selection, ToolSelection::Combined { .. }) {
        return ControlResponse::Failure {
            code: "not_yet_served".into(),
        };
    }
    match run(control, tool, arguments, selection, mode, client).await {
        Ok(result) => ControlResponse::ToolResult { result },
        Err(error) => ControlResponse::ToolCallFailed {
            message: failure_message(&error),
        },
    }
}

async fn run(
    control: &Path,
    tool: String,
    arguments: Map<String, Value>,
    selection: ToolSelection,
    mode: ToolResponseMode,
    client: airk_core::usage::ClientFamily,
) -> Result<Value> {
    if !matches!(selection, ToolSelection::Catalog { .. }) && is_local_help(&tool) {
        return local_help(arguments);
    }
    // The same resolution as `--service-project` / `--catalog-session` with
    // `--service-runtime-dir`: read-only, never starting or electing a service.
    let (control, binding, collection, scoped): (_, ProjectBinding, _, _) = match selection {
        ToolSelection::Unselected => {
            anyhow::bail!("project binding required")
        }
        ToolSelection::Combined { .. } => unreachable!("refused before the call"),
        ToolSelection::Project { binding } => {
            let project = crate::service_frontend::resolve_control(control, binding).await?;
            (
                project.control,
                project.registration.binding,
                project.registration.collection,
                None,
            )
        }
        ToolSelection::Catalog { request } => {
            let selected =
                crate::service_frontend::resolve_scoped(None, Some(control), &request).await?;
            let registration = selected.project.registration;
            (
                selected.project.control,
                registration.binding,
                registration.collection,
                Some(selected.selection),
            )
        }
    };
    let inner = match scoped {
        Some(selection) => crate::project_client::scoped_tools(control, binding, selection)?,
        None => crate::project_client::tools(control, binding),
    };
    inner.client(client);
    let backend = project_backend(inner, binding.to_string(), &collection)?;
    let result: Result<Value> = async {
        let result = call(backend.as_ref(), tool, arguments, response_mode(mode)).await?;
        require_envelope(&result)?;
        Ok(result)
    }
    .await;
    // A one-shot CLI process released its connection by exiting; the service
    // releases it explicitly. A cleanup failure never replaces a result.
    backend.close();
    let cleanup = backend.shutdown().await;
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => {
            Err(error.context(format!("tool session cleanup also failed: {cleanup:#}")))
        }
        (result, _) => result,
    }
}

#[cfg(test)]
mod tests;
