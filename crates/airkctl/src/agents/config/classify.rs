// SPDX-License-Identifier: Apache-2.0
//! Read-only classification of recognized AIRK entries (port of
//! agentConfigHealth.ts `classify`/`classifyFile`). Nothing is written and no
//! process is started. Every classification comes from injected
//! observations; one that is unavailable leaves the entry unclassified
//! (`status: None`) instead of `ok` or foreign.
use super::recognize::{self, Declaration, Recognized};
use super::{Agent, Class, Finding, Observations, Ownership, Scope};
use airk_core::identity::Incarnation;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Transport {
    Stdio,
    Https,
}

/// One recognized entry, healthy ones included.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entry {
    pub(crate) agent: Agent,
    pub(crate) scope: Scope,
    pub(crate) file: String,
    /// Exact declaration container, carried unchanged from the native report.
    pub(crate) container_path: Vec<String>,
    /// A proven helper or launcher can establish ownership with a custom key.
    pub(crate) server_key: String,
    pub(crate) transport: Transport,
    /// `None`: recognized, but an observation it needs is unavailable.
    #[serde(deserialize_with = "entry_status")]
    #[schemars(schema_with = "entry_status_schema", required)]
    pub(crate) status: Option<Class>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "entry_connection_schema")]
    pub(crate) connection_id: Option<Incarnation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) collection: Option<String>,
}

// Unlike optional observation fields, the report always emits status, including null.
fn entry_status<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Class>, D::Error> {
    Option::<Class>::deserialize(deserializer)
}

fn entry_status_schema(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf": [generator.subschema_for::<Class>(), {"type": "null"}]})
}

fn entry_connection_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf": [{"type": "string", "pattern": "^[0-9a-f]{32}$"}, {"type": "null"}]})
}

impl Entry {
    /// The `AgentConfigFinding` of a non-`ok`, classified entry.
    pub(crate) fn finding(&self) -> Option<Finding> {
        let problem = self.status.filter(|status| *status != Class::Ok)?;
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
            detail: None,
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

/// Classifies files against one set of observations; asks the ownership
/// observer at most once per connection ID.
pub(crate) struct Classifier<'a> {
    observations: &'a Observations<'a>,
    lookups: HashMap<Incarnation, Ownership>,
}

impl<'a> Classifier<'a> {
    pub(crate) fn new(observations: &'a Observations<'a>) -> Self {
        Classifier {
            observations,
            lookups: HashMap::new(),
        }
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

    fn ownership(&mut self, id: Incarnation) -> Ownership {
        let observe = self.observations.ownership;
        self.lookups
            .entry(id)
            .or_insert_with(|| observe(id))
            .clone()
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
            Recognized::Https {
                id,
                url,
                helper,
                proven,
            } => {
                entry.transport = Transport::Https;
                entry.connection_id = Some(id);
                entry.status = match self.ownership(id) {
                    // A user-level entry without the U92 proof counts only
                    // once the service vouches for it or it names the live endpoint.
                    Ownership::Unavailable if scope == Scope::User && !proven => return None,
                    Ownership::Unavailable => None,
                    Ownership::NotOwned => {
                        if scope == Scope::User
                            && !proven
                            && observations.endpoint_url != Some(url.as_str())
                        {
                            return None;
                        }
                        Some(Class::HttpsUnknownConnection)
                    }
                    Ownership::Owned { url: owned } => {
                        https_status(agent, &url, &owned, helper.as_deref(), observations)
                    }
                };
            }
        }
        Some(entry)
    }
}

fn https_status(
    agent: Agent,
    url: &str,
    owned: &str,
    helper: Option<&str>,
    observations: &Observations<'_>,
) -> Option<Class> {
    if url != owned {
        return Some(Class::HttpsEndpointMismatch);
    }
    if let Some(helper) = helper {
        match (observations.path_exists)(helper) {
            Some(false) => return Some(Class::HttpsHelperMissing),
            None => return None,
            Some(true) => {}
        }
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
