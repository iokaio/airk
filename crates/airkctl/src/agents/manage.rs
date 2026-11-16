// SPDX-License-Identifier: Apache-2.0
//! `agents list`, `connect`, `repair` and `remove` (split steps 3e.6–3e.8):
//! the command layer over `config`. The service supplies the M15a endpoint
//! URL; `airkctl` reads and writes the files as the invoking user (§6.4). The
//! service never writes them. U110: no connection is enrolled or consulted.
use super::config::{self, Agent, Class, Observations, Platform, Scope, Target, UserEnv};
use crate::contract::{ErrorLine, ExitClass};
use airk_core::identity::Incarnation;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn refused(message: impl std::fmt::Display) -> ErrorLine {
    ErrorLine::new(
        ExitClass::Refused,
        "operation_failed",
        false,
        &message.to_string(),
    )
}

fn edit_failed(error: config::EditFailure) -> ErrorLine {
    let (class, code) = match error.kind {
        config::EditFailureKind::OutcomeUnknown => (
            ExitClass::OutcomeUnknown,
            "agent_config_edit_outcome_unknown",
        ),
        config::EditFailureKind::ConflictRetained => {
            (ExitClass::Refused, "agent_config_edit_conflict_retained")
        }
        config::EditFailureKind::Refused => (ExitClass::Refused, "agent_config_edit_refused"),
    };
    ErrorLine::new(class, code, false, &error.to_string())
}

/// The client `connect` writes for.
pub fn agent(client: super::Client) -> Agent {
    match client {
        super::Client::Claude => Agent::Claude,
        super::Client::Codex => Agent::Codex,
        super::Client::Vscode => Agent::Copilot,
    }
}

/// Where the files are and which service to ask.
pub struct Inputs<'a> {
    pub control: &'a Path,
    pub version: &'a str,
    /// The installed `airkctl`. Unused since U110 writes no header helper;
    /// kept until the caller stops passing it.
    #[allow(dead_code)]
    pub airkctl: &'a Path,
    /// Project folders to check (absolute).
    pub projects: Vec<PathBuf>,
    /// Include this user's agent configuration files.
    pub user: bool,
}

fn user_env() -> UserEnv {
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    UserEnv {
        home: if cfg!(windows) {
            var("USERPROFILE").or_else(|| var("HOME"))
        } else {
            var("HOME")
        },
        appdata: var("APPDATA"),
        codex_home: var("CODEX_HOME"),
        claude_config_dir: var("CLAUDE_CONFIG_DIR"),
        xdg_config_home: var("XDG_CONFIG_HOME"),
        vscode_portable: var("VSCODE_PORTABLE"),
    }
}

/// The live endpoint URL, when the service answers.
fn endpoint_url(inputs: &Inputs<'_>) -> Option<String> {
    crate::agents::endpoint(inputs.control, inputs.version)
        .ok()
        .and_then(|endpoint| endpoint["url"].as_str().map(str::to_owned))
}

fn observe<T>(inputs: &Inputs<'_>, read: impl FnOnce(&Observations<'_>) -> T) -> T {
    let url = endpoint_url(inputs);
    let observations = Observations {
        endpoint_url: url.as_deref(),
        // Trust inspection joins with `service trust` (3e.5); until then unknown.
        trust_installed: None,
        path_exists: &config::path_exists,
        verified_launchers: &[],
        windows: cfg!(windows),
    };
    read(&observations)
}

/// Classify every recognized entry in the selected files.
pub fn report(inputs: &Inputs<'_>) -> config::Report {
    observe(inputs, |observations| {
        let (user_files, mut blockers) = if inputs.user {
            config::expand_user_files(&config::user_locations(Platform::host(), &user_env()))
        } else {
            (Vec::new(), Vec::new())
        };
        let mut report = config::list(&inputs.projects, &user_files, observations);
        report.blockers.append(&mut blockers);
        report
    })
}

/// `agents list`.
pub fn list(inputs: &Inputs<'_>) -> Result<Value, ErrorLine> {
    serde_json::to_value(report(inputs)).map_err(refused)
}

/// The file `connect` writes for `agent` at `scope`.
fn target_file(agent: Agent, scope: Scope, project: &Path) -> Result<PathBuf, ErrorLine> {
    match scope {
        Scope::Project => config::PROJECT_FILES
            .iter()
            .find(|(known, _)| *known == agent)
            .map(|(_, parts)| {
                parts
                    .iter()
                    .fold(project.to_path_buf(), |path, part| path.join(part))
            })
            .ok_or_else(|| refused("no project configuration file for this client")),
        Scope::User => config::user_locations(Platform::host(), &user_env())
            .files
            .into_iter()
            .find(|(known, _)| *known == agent)
            .map(|(_, path)| PathBuf::from(path))
            .ok_or_else(|| refused("this user's configuration folder is unavailable")),
    }
}

/// How the caller asked `connect` to obtain a connection. U110: connections
/// no longer exist; `Open` (or no source) writes the endpoint URL and the
/// other forms are refused.
pub enum Source {
    Existing(Incarnation),
    Enroll {
        id: Incarnation,
        selection_file: PathBuf,
    },
    Open,
}

const NO_CONNECTIONS: &str =
    "AIRK agent connections no longer exist; agents connect writes the endpoint URL only (U110)";

/// `agents connect` (U110): write the entry naming the live endpoint URL
/// with only the `X-AIRK-Response-Mode` header. Nothing is enrolled.
pub fn connect(
    inputs: &Inputs<'_>,
    client: super::Client,
    source: Source,
    scope: Scope,
    server_key: &str,
) -> Result<Value, ErrorLine> {
    match source {
        Source::Open => {}
        Source::Existing(id) => {
            return Err(refused(format!(
                "{NO_CONNECTIONS}; --connection-id {id} was not used."
            )));
        }
        Source::Enroll { id, selection_file } => {
            return Err(refused(format!(
                "{NO_CONNECTIONS}; --connection-id {id} and --selection-file {} were not used.",
                selection_file.display()
            )));
        }
    }
    let agent = agent(client);
    let project = inputs
        .projects
        .first()
        .cloned()
        .ok_or_else(|| refused("connect needs a project folder"))?;
    let file = target_file(agent, scope, &project)?;
    config::preflight_edit(&file).map_err(edit_failed)?;
    let endpoint = crate::agents::endpoint(inputs.control, inputs.version)?;
    let url = endpoint["url"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| refused("the AIRK service's HTTPS endpoint names no URL"))?;
    let changed = config::edit_file_for(
        config::EditOperation::Connect,
        &file,
        (scope == Scope::Project).then_some(project.as_path()),
        true,
        |source| config::connect(agent, scope, source, server_key, &url),
    )
    .map_err(edit_failed)?;
    Ok(json!({
        "schema": 1,
        "client": agent.name(),
        "scope": scope,
        "file": file,
        "serverKey": server_key,
        "url": url,
        "changed": changed,
    }))
}

fn project_root_of(entry: &config::Entry, inputs: &Inputs<'_>) -> Option<PathBuf> {
    (entry.scope == Scope::Project)
        .then(|| {
            inputs
                .projects
                .iter()
                .find(|root| Path::new(&entry.file).starts_with(root))
                .cloned()
        })
        .flatten()
}

/// `agents repair` (U110): rewrite every recognized connectable HTTPS entry
/// that names another endpoint or still carries legacy credential fields to
/// the URL-only form. `_replacement` (`--connection-id`) is ignored:
/// connections no longer exist.
pub fn repair(inputs: &Inputs<'_>, _replacement: Option<Incarnation>) -> Result<Value, ErrorLine> {
    let report = report(inputs);
    let url = endpoint_url(inputs)
        .ok_or_else(|| refused("the AIRK service's HTTPS endpoint is not available"))?;
    let mut actions = Vec::new();
    for entry in &report.entries {
        let Some(status) = entry.status else { continue };
        let stale = matches!(
            status,
            Class::HttpsEndpointMismatch
                | Class::HttpsUnknownConnection
                | Class::HttpsHelperMissing
        );
        if !stale || !entry.agent.connectable() {
            continue;
        }
        let target = Target {
            container_path: entry.container_path.clone(),
            server_key: entry.server_key.clone(),
        };
        let root = project_root_of(entry, inputs);
        let outcome = config::edit_file_for(
            config::EditOperation::Repair,
            Path::new(&entry.file),
            root.as_deref(),
            false,
            |source| {
                config::repair(
                    entry.agent,
                    entry.scope,
                    source.ok_or("configuration file disappeared")?,
                    &target,
                    &url,
                )
            },
        );
        actions.push(json!({
            "file": entry.file,
            "serverKey": entry.server_key,
            "problem": status,
            "connectionId": entry.connection_id,
            "url": url,
            "changed": outcome.as_ref().ok(),
            "error": outcome.err().map(|error| error.to_string()),
        }));
    }
    Ok(json!({"schema": 1, "actions": actions, "blockers": report.blockers}))
}

/// `agents remove`: delete exact selected rows, entries naming `connection`,
/// or all recognized AIRK entries when neither selector is supplied.
/// Other servers, comments and policies stay.
pub fn remove(
    inputs: &Inputs<'_>,
    connection: Option<Incarnation>,
    selected: &[config::Entry],
) -> Result<Value, ErrorLine> {
    let report = report(inputs);
    let entries = selected_entries(&report.entries, connection, selected)?;
    let mut actions = Vec::new();
    for entry in entries {
        let target = Target {
            container_path: entry.container_path.clone(),
            server_key: entry.server_key.clone(),
        };
        let root = project_root_of(entry, inputs);
        let outcome = config::edit_file_for(
            config::EditOperation::Remove,
            Path::new(&entry.file),
            root.as_deref(),
            false,
            |source| {
                let source = source.ok_or("configuration file disappeared")?;
                if !selected.is_empty() {
                    observe(inputs, |observations| {
                        config::revalidate_entry(source, entry, observations)
                    })?;
                }
                config::remove(entry.agent, entry.scope, source, &target)
            },
        );
        actions.push(json!({
            "file": entry.file,
            "serverKey": entry.server_key,
            "connectionId": entry.connection_id,
            "changed": outcome.as_ref().ok(),
            "error": outcome.err().map(|error| error.to_string()),
        }));
    }
    Ok(json!({"schema": 1, "actions": actions, "blockers": report.blockers}))
}

fn same_target(left: &config::Entry, right: &config::Entry) -> bool {
    left.agent == right.agent
        && left.scope == right.scope
        && left.file == right.file
        && left.container_path == right.container_path
        && left.server_key == right.server_key
}

fn selected_entries<'a>(
    current: &'a [config::Entry],
    connection: Option<Incarnation>,
    selected: &'a [config::Entry],
) -> Result<Vec<&'a config::Entry>, ErrorLine> {
    if selected.is_empty() {
        return Ok(current
            .iter()
            .filter(|entry| connection.is_none() || entry.connection_id == connection)
            .collect());
    }
    if connection.is_some() {
        return Err(refused("--entry conflicts with --connection-id"));
    }
    for (index, expected) in selected.iter().enumerate() {
        if selected[..index]
            .iter()
            .any(|previous| same_target(previous, expected))
        {
            return Err(refused("duplicate selected agent entry"));
        }
        let matches: Vec<_> = current
            .iter()
            .filter(|entry| same_target(entry, expected))
            .collect();
        if matches.len() != 1 || matches[0] != expected {
            return Err(refused(format!(
                "selected agent entry changed, disappeared or is outside scope: {} / {}",
                expected.file, expected.server_key
            )));
        }
    }
    Ok(selected.iter().collect())
}
