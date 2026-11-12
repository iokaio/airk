// SPDX-License-Identifier: Apache-2.0
//! M11 `ExplorerOffline` (split step 3d): the CLI's offline Explorer
//! operations in `airk_mcp::explorer_offline`, unchanged; the service
//! supplies the clock the CLI read. A failure carries the CLI's error chain.
use airk_core::service::{ControlResponse, ExplorerOperation, ProjectBinding};
use airk_mcp::explorer_offline as offline;
use serde_json::{Map, Value};

pub(super) fn run(
    operation: ExplorerOperation,
    arguments: Map<String, Value>,
    document: Option<Value>,
    project: Option<ProjectBinding>,
) -> ControlResponse {
    let result = (|| -> anyhow::Result<Value> {
        let now = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis(),
        )?;
        let project = project.map(|binding| binding.to_string());
        match operation {
            ExplorerOperation::DraftSearch => {
                offline::draft("draft_search", arguments, project, now)
            }
            ExplorerOperation::DraftStrategy => {
                offline::draft("draft_strategy", arguments, project, now)
            }
            ExplorerOperation::DraftPlan => offline::draft("draft_plan", arguments, project, now),
            ExplorerOperation::SuggestView => {
                offline::draft("suggest_view", arguments, project, now)
            }
            ExplorerOperation::ValidateDraft => offline::validate_draft(&arguments),
            ExplorerOperation::ListTemplates => offline::list_templates(&arguments),
            ExplorerOperation::GetTemplate => offline::get_template(&arguments),
            ExplorerOperation::ProposalStatus => {
                offline::proposal_status(&document.unwrap_or(Value::Null))
            }
        }
    })();
    match result {
        Ok(result) => ControlResponse::ExplorerOffline { result },
        Err(error) => ControlResponse::ExplorerOfflineFailed {
            message: super::tools_call::failure_message(&error),
        },
    }
}
