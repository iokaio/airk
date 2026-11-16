// SPDX-License-Identifier: Apache-2.0
//! Read-only classification of recognized AIRK entries (port of
//! agentConfigHealth.ts `classify`/`classifyFile`). Nothing is written and no
//! process is started. Every classification comes from injected
//! observations; one that is unavailable leaves the entry unclassified
//! (`status: None`) instead of `ok` or foreign.
use super::recognize::{self, Declaration, Recognized};
use super::{Agent, Class, Entry, Finding, Observations, Scope, Transport};
use serde_json::Value;

/// U110: a legacy HTTPS entry still carrying credential fields is reported
/// as `https_unknown_connection` (its connection no longer exists) with this
/// detail, and `agents repair` rewrites it to the URL-only form.
const STALE_CREDENTIAL: &str = "The entry still carries AIRK connection credential fields (Authorization, headersHelper, http_headers_helper or bearer_token_env_var). AIRK agent connections no longer exist (U110); run airkctl agents repair to keep only the endpoint URL.";

impl Entry {
    /// The `AgentConfigFinding` of a non-`ok`, classified entry.
    pub(crate) fn finding(&self) -> Option<Finding> {
        let problem = self.status.filter(|status| *status != Class::Ok)?;
        let stale = problem == Class::HttpsUnknownConnection && self.transport == Transport::Https;
        Some(Finding {
            agent: self.agent,
            label: self.agent.label(),
            file: self.file.clone(),
            scope: self.scope,
            server_key: Some(self.server_key.clone()),
            container_path: (self.container_path.len() != 1).then(|| self.container_path.clone()),
            command: self.command.clone(),
            url: self.url.clone(),
            connection_id: self.connection_id.map(|id| id.to_string()),
            root: self.root.clone(),
            collection: self.collection.clone(),
            problem,
            detail: stale.then(|| STALE_CREDENTIAL.to_string()),
        })
    }
}

/// Compare the exact observed row again using the source being edited and fresh observations.
pub(crate) fn revalidate_entry(
    source: &str,
    expected: &Entry,
    observations: &Observations<'_>,
) -> Result<(), String> {
    let report =
        Classifier::new(observations).file(expected.agent, expected.scope, &expected.file, source);
    let matches: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| {
            entry.container_path == expected.container_path
                && entry.server_key == expected.server_key
        })
        .collect();
    if matches.len() != 1 || matches[0] != expected {
        return Err(
            "selected agent entry changed before its edit; this entry was not changed".into(),
        );
    }
    Ok(())
}

/// The classification of one file.
#[derive(Debug, Default)]
pub(crate) struct FileReport {
    /// The file was parsed.
    pub(crate) checked: bool,
    pub(crate) findings: Vec<Finding>,
    pub(crate) entries: Vec<Entry>,
}

/// Classifies files against one set of observations.
pub(crate) struct Classifier<'a> {
    observations: &'a Observations<'a>,
}

impl<'a> Classifier<'a> {
    pub(crate) fn new(observations: &'a Observations<'a>) -> Self {
        Classifier { observations }
    }

    /// Classify the result of reading one file: absent files are skipped, a
    /// read failure is `unreadable`.
    pub(crate) fn read(
        &mut self,
        agent: Agent,
        scope: Scope,
        file: &str,
        source: Result<Option<String>, String>,
    ) -> Option<FileReport> {
        match source {
            Ok(None) => None,
            Ok(Some(source)) => Some(self.file(agent, scope, file, &source)),
            Err(detail) => Some(FileReport {
                checked: false,
                findings: vec![Finding::unreadable(agent, scope, file, detail)],
                entries: Vec::new(),
            }),
        }
    }

    /// Classify one file's text. A project file is read only at the agent's
    /// own container; a user file at every container.
    pub(crate) fn file(
        &mut self,
        agent: Agent,
        scope: Scope,
        file: &str,
        source: &str,
    ) -> FileReport {
        let rows = match recognize::declarations(agent.parser(), source) {
            Ok(rows) => rows,
            Err(detail) => {
                return FileReport {
                    checked: false,
                    findings: vec![Finding::unreadable(agent, scope, file, detail)],
                    entries: Vec::new(),
                };
            }
        };
        let mut report = FileReport {
            checked: true,
            ..FileReport::default()
        };
        for row in &rows {
            if scope == Scope::Project && row.container != [agent.container()] {
                continue;
            }
            if let Some(entry) = self.classify(agent, scope, file, row) {
                report.findings.extend(entry.finding());
                report.entries.push(entry);
            }
        }
        report
    }

    fn classify(
        &mut self,
        agent: Agent,
        scope: Scope,
        file: &str,
        row: &Declaration,
    ) -> Option<Entry> {
        let observations = self.observations;
        let recognized = recognize::recognize(
            agent,
            scope,
            row,
            observations.verified_launchers,
            observations.windows,
        )?;
        let text = |field: &str| {
            row.fields
                .get(field)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let mut entry = Entry {
            agent,
            scope,
            file: file.to_string(),
            container_path: row.container.clone(),
            server_key: row.server_key.clone(),
            transport: Transport::Stdio,
            status: None,
            command: text("command"),
            url: text("url"),
            connection_id: None,
            root: None,
            collection: None,
        };
        match recognized {
            Recognized::Stdio {
                command,
                root,
                collection,
            } => {
                entry.root = root;
                entry.collection = collection;
                let missing = recognize::is_absolute(&command)
                    && (observations.path_exists)(&command) == Some(false);
                entry.status = Some(if missing {
                    Class::StdioMissingCommand
                } else {
                    Class::StdioRetired
                });
            }
            Recognized::Https { url, id, stale } => {
                entry.transport = Transport::Https;
                entry.connection_id = id;
                entry.status = https_status(agent, &url, !stale.is_empty(), observations);
            }
        }
        Some(entry)
    }
}

/// U110: stale credential fields first (they are determined by the file
/// alone), then the live endpoint, then trust for the clients that need it.
fn https_status(
    agent: Agent,
    url: &str,
    stale: bool,
    observations: &Observations<'_>,
) -> Option<Class> {
    if stale {
        return Some(Class::HttpsUnknownConnection);
    }
    if observations.endpoint_url? != url {
        return Some(Class::HttpsEndpointMismatch);
    }
    if matches!(agent, Agent::Claude | Agent::Codex) {
        return match observations.trust_installed {
            Some(false) => Some(Class::HttpsTrustMissing),
            Some(true) => Some(Class::Ok),
            None => None,
        };
    }
    Some(Class::Ok)
}
