// SPDX-License-Identifier: Apache-2.0
//! Canonical token-free agent setup text. These inert values grant no write authority.
use crate::service_records::LoopbackMcpUrl;
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ManualClient {
    Claude,
    Codex,
    Vscode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ManualScope {
    Project,
    User,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ManualFormat {
    Json,
    Toml,
}

/// Exact text to merge into the named configuration file; never replace unrelated entries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentManualEntry {
    pub client: ManualClient,
    pub scope: ManualScope,
    pub file: String,
    pub format: ManualFormat,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentConfigRefusalReason {
    AutomaticPolicy,
    ConflictRetained,
    OutcomeUnknown,
}

/// A presentation projection of the native writer outcome, never retry authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentConfigRefusal {
    pub reason: AgentConfigRefusalReason,
    pub detail: String,
    pub file: String,
    pub manual: Option<AgentManualEntry>,
}

/// Render a complete mergeable entry using the exact live endpoint and selected path.
/// JSON strings are also valid TOML basic strings for this bounded ASCII key/URL profile.
pub fn entry(
    client: ManualClient,
    scope: ManualScope,
    file: String,
    server_key: &str,
    url: LoopbackMcpUrl,
) -> Result<AgentManualEntry> {
    ensure!(
        !file.is_empty() && file.len() <= 32768 && !file.contains('\0'),
        "invalid manual target file"
    );
    ensure!(
        server_key == "airk" || server_key.starts_with("airk-") || server_key.starts_with("airk_"),
        "manual server key must name AIRK"
    );
    ensure!(
        server_key.len() <= 256 && server_key.chars().all(|ch| !ch.is_control()),
        "invalid manual server key"
    );
    let url = url.to_string();
    let (format, text) = match client {
        ManualClient::Codex => {
            let key = if server_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            {
                server_key.to_owned()
            } else {
                serde_json::to_string(server_key)?
            };
            (
                ManualFormat::Toml,
                format!(
                    "[mcp_servers.{key}]\nurl = {}\ntool_timeout_sec = {}\n\n[mcp_servers.{key}.http_headers]\n\"X-AIRK-Response-Mode\" = \"terse\"\n",
                    serde_json::to_string(&url)?,
                    crate::read_attempt::MAX_ANSWER_READ_BUDGET.as_secs()
                ),
            )
        }
        ManualClient::Claude | ManualClient::Vscode => {
            let container = if client == ManualClient::Vscode {
                "servers"
            } else {
                "mcpServers"
            };
            let value = serde_json::json!({container: {server_key: {"type": "http", "url": url, "headers": {"X-AIRK-Response-Mode": "terse"}}}});
            (
                ManualFormat::Json,
                format!("{}\n", serde_json::to_string_pretty(&value)?),
            )
        }
    };
    Ok(AgentManualEntry {
        client,
        scope,
        file,
        format,
        text,
    })
}

/// Relative project targets for endpoint consumers that have not selected a folder.
pub fn project_entries(url: LoopbackMcpUrl) -> Vec<AgentManualEntry> {
    [
        (ManualClient::Claude, ".mcp.json"),
        (ManualClient::Codex, ".codex/config.toml"),
        (ManualClient::Vscode, ".vscode/mcp.json"),
    ]
    .into_iter()
    .map(|(client, file)| {
        entry(client, ManualScope::Project, file.into(), "airk", url)
            .expect("fixed canonical manual entry")
    })
    .collect()
}

#[cfg(test)]
mod tests;
