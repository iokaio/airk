// SPDX-License-Identifier: Apache-2.0
//! `agents list`, `connect`, `repair` and `remove` (split steps 3e.6–3e.8):
//! the command layer over `config`. The service supplies observations
//! (M15a endpoint, M16 ownership, `HttpEnroll`); `airkctl` reads and writes
//! the files as the invoking user (§6.4). The service never writes them.
use super::config::{
    self, Agent, Class, Connection, Observations, Ownership, Platform, Scope, Target, UserEnv,
};
use crate::contract::{ErrorLine, ExitClass};
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse},
    service_http::{HttpConnectionInfo, HttpSelection, MAX_CONNECTION_BYTES},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

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
    /// The installed `airkctl` that written helpers name (U92).
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

/// M16 for one id; anything but a clear answer is `Unavailable`.
fn ownership(inputs: &Inputs<'_>, id: Incarnation) -> Ownership {
    match crate::agents::ownership(inputs.control, inputs.version, id) {
        Ok(result) => match result["status"].as_str() {
            Some("owned") => match result["connection"]["endpoint"]["url"].as_str() {
                Some(url) => Ownership::Owned { url: url.into() },
                None => Ownership::Unavailable,
            },
            Some("not_owned") => Ownership::NotOwned,
            _ => Ownership::Unavailable,
        },
        Err(_) => Ownership::Unavailable,
    }
}

fn observe<T>(inputs: &Inputs<'_>, read: impl FnOnce(&Observations<'_>) -> T) -> T {
    let url = endpoint_url(inputs);
    let cache = RefCell::new(BTreeMap::new());
    let owned = |id: Incarnation| -> Ownership {
        if let Some(known) = cache.borrow().get(&id) {
            return Ownership::clone(known);
        }
        let answer = ownership(inputs, id);
        cache.borrow_mut().insert(id, answer.clone());
        answer
    };
    let observations = Observations {
        endpoint_url: url.as_deref(),
        // Trust inspection joins with `service trust` (3e.5); until then unknown.
        trust_installed: None,
        ownership: &owned,
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

/// Read an owner-private selection file (as `airk runtime http-connection`).
fn selection_file(path: &Path) -> Result<HttpSelection, ErrorLine> {
    if !path.is_absolute() {
        return Err(refused("HTTP selection file must be absolute"));
    }
    let bytes = crate::agents::private_file(path, MAX_CONNECTION_BYTES)
        .map_err(refused)?
        .ok_or_else(|| refused("HTTP selection file unavailable"))?;
    let selection: HttpSelection = serde_json::from_slice(&bytes).map_err(refused)?;
    selection
        .validate()
        .map_err(|error| refused(format!("{error:#}")))?;
    Ok(selection)
}

/// Enroll `selection` as connection `id` (`HttpEnroll`, fenced to the
/// observed instance), checked as `airk runtime http-connection` checked it.
fn enroll(
    inputs: &Inputs<'_>,
    id: Incarnation,
    selection: HttpSelection,
    client: super::Client,
) -> Result<HttpConnectionInfo, ErrorLine> {
    let health =
        match crate::service::request(inputs.control, ControlMethod::Health, inputs.version)? {
            ControlResponse::Health { observation } => observation,
            other => return Err(crate::service::refusal(&other)),
        };
    if !health.policy.accepting {
        return Err(refused("native service is draining"));
    }
    let label = match client {
        super::Client::Claude => "claude",
        super::Client::Codex => "codex",
        super::Client::Vscode => "vscode",
    };
    let response = crate::service::request_fenced(
        inputs.control,
        ControlMethod::HttpEnroll {
            connection_id: id,
            selection: Box::new(selection.clone()),
            client: airk_core::usage::ClientFamily::from_label(label),
            explorer_descriptor: None,
            reattach_connection_id: None,
        },
        inputs.version,
        Some(health.instance),
    )?;
    let ControlResponse::HttpConnection { connection } = response else {
        return Err(refused(format!(
            "native HTTP setup refused: {}",
            serde_json::to_string(&response).unwrap_or_default()
        )));
    };
    connection
        .validate()
        .map_err(|error| refused(format!("{error:#}")))?;
    if connection.connection_id != id
        || !airk_core::service_http_credentials::same_authority(&selection, &connection.selection)
    {
        return Err(refused("native HTTP enrollment selection changed"));
    }
    Ok(*connection)
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

/// How `connect` obtains its connection.
pub enum Source {
    Existing(Incarnation),
    Enroll {
        id: Incarnation,
        selection_file: PathBuf,
    },
    /// U105: a new open connection with a freshly minted id, in the
    /// endpoint's namespace.
    Open,
}

/// The open selection of the live endpoint's namespace.
fn open_selection(inputs: &Inputs<'_>) -> Result<HttpSelection, ErrorLine> {
    let endpoint = crate::agents::endpoint(inputs.control, inputs.version)?;
    let namespace = endpoint["namespace"]
        .as_str()
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| refused("the HTTPS endpoint names no namespace"))?;
    Ok(HttpSelection::Open {
        selection: airk_core::service_http::OpenHttpSelection { namespace },
    })
}

/// `agents connect`: enroll or reuse a connection and write its entry.
pub fn connect(
    inputs: &Inputs<'_>,
    client: super::Client,
    source: Source,
    scope: Scope,
    server_key: &str,
) -> Result<Value, ErrorLine> {
    let agent = agent(client);
    let project = inputs
        .projects
        .first()
        .cloned()
        .ok_or_else(|| refused("connect needs a project folder"))?;
    let file = target_file(agent, scope, &project)?;
    config::preflight_edit(&file).map_err(edit_failed)?;
    let connection =
        match source {
            Source::Existing(id) => serde_json::from_value::<HttpConnectionInfo>(
                crate::agents::info(inputs.control, inputs.version, id)?,
            )
            .map_err(refused)?,
            Source::Enroll {
                id,
                selection_file: file,
            } => enroll(inputs, id, selection_file(&file)?, client)?,
            Source::Open => enroll(
                inputs,
                crate::journal::mint()?,
                open_selection(inputs)?,
                client,
            )?,
        };
    let url = connection.endpoint.url.to_string();
    let airkctl = inputs.airkctl.to_string_lossy().into_owned();
    let written = Connection {
        id: connection.connection_id,
        url: &url,
        airkctl: &airkctl,
    };
    let changed = config::edit_file_for(
        config::EditOperation::Connect,
        &file,
        (scope == Scope::Project).then_some(project.as_path()),
        true,
        |source| config::connect(agent, scope, source, server_key, &written),
    )
    .map_err(|error| {
        let error = edit_failed(error);
        ErrorLine::new(
            error.class,
            &error.code,
            false,
            &format!(
                "The selected connection is {}. {}",
                connection.connection_id, error.message
            ),
        )
    })?;
    let mut written = json!({
        "schema": 1,
        "client": agent.name(),
        "scope": scope,
        "file": file,
        "serverKey": server_key,
        "connectionId": connection.connection_id,
        "url": url,
        "changed": changed,
    });
    // U105: an open connection reports its stored defaults and choice.
    if let Some(open) = &connection.open {
        written["open"] = serde_json::to_value(open).map_err(refused)?;
    }
    Ok(written)
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

/// `agents repair`: rewrite recognized connectable entries whose endpoint or
/// helper is stale; an unknown connection is rewritten only to `replacement`.
pub fn repair(inputs: &Inputs<'_>, replacement: Option<Incarnation>) -> Result<Value, ErrorLine> {
    let report = report(inputs);
    let url = endpoint_url(inputs)
        .ok_or_else(|| refused("the AIRK service's HTTPS endpoint is not available"))?;
    let airkctl = inputs.airkctl.to_string_lossy().into_owned();
    let mut actions = Vec::new();
    for entry in &report.entries {
        let Some(status) = entry.status else { continue };
        let id = match status {
            Class::HttpsEndpointMismatch | Class::HttpsHelperMissing => entry.connection_id,
            Class::HttpsUnknownConnection => replacement,
            _ => None,
        };
        let (Some(id), true) = (id, entry.agent.connectable()) else {
            continue;
        };
        let target = Target {
            container_path: entry.container_path.clone(),
            server_key: entry.server_key.clone(),
        };
        let written = Connection {
            id,
            url: &url,
            airkctl: &airkctl,
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
                    &written,
                )
            },
        );
        actions.push(json!({
            "file": entry.file,
            "serverKey": entry.server_key,
            "problem": status,
            "connectionId": id,
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
