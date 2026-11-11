// SPDX-License-Identifier: Apache-2.0
//! M2 `ToolsCatalog` (split step 3a.1; `docs/split/contracts.md` §2): the
//! catalog documents the pre-split `airk tools list --json` and
//! `tools help --json` print, served to `airkctl` over I1.
use airk_core::service::{ControlResponse, ToolsCatalogDetail};
use airk_mcp::remote::Tool;
use anyhow::Result;
use serde_json::json;

/// Whether `name` selects `tool`: its exact name, or its name after an
/// `airk_` prefix. Shared by `airk tools help` and M2.
pub fn selects(tool: &Tool, name: &str) -> bool {
    tool.name == *name || name.strip_prefix("airk_") == Some(tool.name.as_ref())
}

/// Answer M2 from the unified catalog.
pub fn response(detail: &ToolsCatalogDetail) -> Result<ControlResponse> {
    response_from(detail, airk_mcp::unified::tools())
}

/// Answer M2 from an explicit catalog (a combined session's, step 3a.2d).
pub(crate) fn response_from(
    detail: &ToolsCatalogDetail,
    catalog: Vec<Tool>,
) -> Result<ControlResponse> {
    let document = match detail {
        ToolsCatalogDetail::List { schemas: true } => {
            serde_json::to_string_pretty(&json!({ "tools": catalog }))?
        }
        ToolsCatalogDetail::List { schemas: false } => {
            let tools: Vec<_> = catalog
                .iter()
                .map(|tool| json!({ "name": tool.name, "description": tool.description }))
                .collect();
            serde_json::to_string_pretty(&json!({ "tools": tools }))?
        }
        ToolsCatalogDetail::Help { tool: name } => {
            match catalog.into_iter().find(|tool| selects(tool, name)) {
                Some(tool) => serde_json::to_string_pretty(&tool)?,
                None => {
                    return Ok(ControlResponse::Failure {
                        code: "unknown_tool".into(),
                    });
                }
            }
        }
    };
    Ok(ControlResponse::ToolsCatalog { document })
}

#[cfg(test)]
mod tests;
