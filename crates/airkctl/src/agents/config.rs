// SPDX-License-Identifier: Apache-2.0
//! Agent MCP configuration (split plan §6.4, §8.4; U12, U49, U91, U92): the
//! one native implementation of `airkctl agents list/connect/repair/remove`.
//!
//! The TypeScript in `packages/vscode/src` (`agentConfigHealth.ts`,
//! `agentConfig.ts`, `agentToml.ts`, `unifiedAgentConfig.ts`, `model.ts`,
//! `serviceHttp.ts`, `globalAgentInventory.ts`) is the semantic reference.
//! The split narrows it: pre-split migration, clean-break cleanup plans,
//! reconnect ledgers and stdio rebinding are not ported (§9.3 owns pre-split
//! cleanup), and only the U92 helper form proves an HTTPS entry is AIRK's.
//!
//! Classification and edits are pure functions over file text and injected
//! observations; `io` is the only module that touches the file system.
//! Unknown observations are never reported as `ok` or as foreignness.
use airk_core::identity::Incarnation;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod classify;
mod edit;
// Contract-first: no production archive reader or writer is enabled yet.
#[cfg(test)]
mod edit_record;
mod io;
mod jsonc;
mod locations;
mod recognize;
#[cfg(test)]
mod tests;
mod toml;

pub(crate) use classify::{Classifier, Entry, FileReport, revalidate_entry};
pub(crate) use edit::{Connection, Target, connect, remove, repair};
pub(crate) use io::{edit_file, expand_user_files, path_exists};
pub(crate) use locations::{PROJECT_FILES, Platform, UserEnv, user_locations};

/// The agents whose files AIRK reads (`agentMigration.ts` `locations`).
/// Only Claude Code, Codex and VS Code (`copilot`) can be connected (U91).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Agent {
    Codex,
    Claude,
    Copilot,
    Gemini,
    Antigravity,
    Cursor,
    Roo,
    Junie,
}

impl Agent {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Agent::Codex => "codex",
            Agent::Claude => "claude",
            Agent::Copilot => "copilot",
            Agent::Gemini => "gemini",
            Agent::Antigravity => "antigravity",
            Agent::Cursor => "cursor",
            Agent::Roo => "roo",
            Agent::Junie => "junie",
        }
    }

    /// `AGENT_CONFIG_LABELS` in `agentConfigHealth.ts`.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Copilot => "VS Code / GitHub Copilot Chat",
            Agent::Gemini => "Gemini CLI",
            Agent::Antigravity => "Google Antigravity",
            Agent::Cursor => "Cursor",
            Agent::Roo => "Roo Code",
            Agent::Junie => "JetBrains Junie",
        }
    }

    /// The project-level server container.
    pub(crate) fn container(self) -> &'static str {
        match self {
            Agent::Codex => "mcp_servers",
            Agent::Copilot => "servers",
            _ => "mcpServers",
        }
    }

    /// Whether `connect` and `repair` may write this agent's files (U91).
    pub(crate) fn connectable(self) -> bool {
        matches!(self, Agent::Claude | Agent::Codex | Agent::Copilot)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Scope {
    Project,
    User,
}

/// The §6.4 classes, plus `unreadable` for a present file that could not be
/// read or parsed (it could hide a broken entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Class {
    StdioRetired,
    StdioMissingCommand,
    HttpsUnknownConnection,
    HttpsEndpointMismatch,
    HttpsHelperMissing,
    HttpsTrustMissing,
    Ok,
    Unreadable,
}

/// `AgentConfigFinding` (explorerContracts.ts), field for field, so the
/// extension renders it unchanged. `containerPath` appears only when it is
/// not the single top-level container.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Finding {
    pub(crate) agent: Agent,
    pub(crate) label: &'static str,
    pub(crate) file: String,
    pub(crate) scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) server_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) container_path: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) collection: Option<String>,
    pub(crate) problem: Class,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detail: Option<String>,
}

impl Finding {
    pub(crate) fn unreadable(agent: Agent, scope: Scope, file: &str, detail: String) -> Self {
        Finding {
            agent,
            label: agent.label(),
            file: file.to_string(),
            scope,
            server_key: None,
            container_path: None,
            command: None,
            url: None,
            connection_id: None,
            root: None,
            collection: None,
            problem: Class::Unreadable,
            detail: Some(detail),
        }
    }
}

/// The service's answer for one connection ID (the M16 read).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ownership {
    /// The connection exists, belongs to this user and is served at `url`.
    Owned { url: String },
    /// The service answered that it does not know the connection.
    NotOwned,
    /// No answer (service down, refused, timed out). Never proof either way.
    Unavailable,
}

/// Everything classification needs from outside the file text.
pub(crate) struct Observations<'a> {
    /// The URL the running service publishes, when known.
    pub(crate) endpoint_url: Option<&'a str>,
    /// Whether this user's OS store trusts the local issuer; `None` = unknown.
    pub(crate) trust_installed: Option<bool>,
    pub(crate) ownership: &'a dyn Fn(Incarnation) -> Ownership,
    /// `Some(false)` only for an observed absence; `None` when unknown.
    pub(crate) path_exists: &'a dyn Fn(&str) -> Option<bool>,
    /// Executables proven to belong to an AIRK installation of this user;
    /// the only proof for a user-level stdio entry.
    pub(crate) verified_launchers: &'a [String],
    /// Compare paths case-insensitively with either separator.
    pub(crate) windows: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Blocker {
    pub(crate) code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) file: Option<String>,
}

/// `airkctl agents list`: files checked, findings (non-`ok`), every
/// recognized entry (`status: null` when an observation was unavailable) and
/// inventory blockers.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Report {
    pub(crate) checked: Vec<String>,
    pub(crate) findings: Vec<Finding>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) blockers: Vec<Blocker>,
}

impl Report {
    fn add(&mut self, file: String, report: Option<FileReport>) {
        let Some(report) = report else { return };
        if report.checked {
            self.checked.push(file);
        }
        self.findings.extend(report.findings);
        self.entries.extend(report.entries);
    }
}

const REDIRECTED: &str =
    "The configuration folder is a link or resolves outside this project, so it was not checked.";

/// Read and classify every recognized project file under `project_roots`
/// and the given user-level files. Read-only; missing files are skipped.
pub(crate) fn list(
    project_roots: &[PathBuf],
    user_files: &[(Agent, PathBuf)],
    observations: &Observations<'_>,
) -> Report {
    let mut report = Report::default();
    let mut classifier = Classifier::new(observations);
    for root in project_roots {
        for (agent, parts) in PROJECT_FILES {
            let file = parts
                .iter()
                .fold(root.clone(), |path, part| path.join(part));
            let shown = file.display().to_string();
            let source = match io::folder_state(root, &parts[..parts.len() - 1]) {
                Ok(io::FolderState::Absent) => continue,
                Ok(io::FolderState::Redirected) => Err(REDIRECTED.to_string()),
                Ok(io::FolderState::Present) => io::read_config(&file),
                Err(error) => Err(error),
            };
            let found = classifier.read(agent, Scope::Project, &shown, source);
            report.add(shown, found);
        }
    }
    for (agent, file) in user_files {
        let shown = file.display().to_string();
        let source = match file.parent().map(io::direct_directory) {
            Some(Ok(true)) => io::read_config(file),
            Some(Ok(false)) => continue,
            Some(Err(error)) => Err(error),
            None => Err("configuration file has no parent folder".to_string()),
        };
        let found = classifier.read(*agent, Scope::User, &shown, source);
        report.add(shown, found);
    }
    report
}
