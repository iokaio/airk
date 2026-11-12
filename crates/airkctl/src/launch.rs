// SPDX-License-Identifier: Apache-2.0
//! `agents launch` (U91): start Claude Code, Codex or VS Code in the
//! foreground with child-only credentials and additive trust, as `airk setup
//! --launch` started Claude Code and Codex. Ported from `airk-cli`'s
//! `standalone_setup::launch`, whose copy leaves with `setup` in Phase 4.
//!
//! The service is asked for the connection's checked references (M15), so its
//! ownership and grant are revalidated immediately before the token is read.
//! The token goes only into the child's environment; it is never printed,
//! written to a configuration file or placed in arguments.
use crate::contract::{ErrorLine, ExitClass};
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    service_http::HttpConnectionInfo,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const MAX_CA_BYTES: usize = 16 * 1024 * 1024;

/// The qualified clients (U91).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Client {
    Claude,
    Codex,
    Vscode,
}

impl Client {
    fn executable_name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Vscode => "code",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Vscode => "vscode",
        }
    }
}

fn refused(message: impl std::fmt::Display) -> ErrorLine {
    ErrorLine::new(
        ExitClass::Refused,
        "operation_failed",
        false,
        &message.to_string(),
    )
}

struct LaunchPlan {
    arguments: Vec<OsString>,
    environment: BTreeMap<OsString, OsString>,
}

#[cfg(any(windows, test))]
fn windows_client_executable(
    client: Client,
    directories: impl IntoIterator<Item = PathBuf>,
) -> Result<PathBuf, String> {
    let name = client.executable_name();
    for directory in directories {
        // Do not let an untrusted current project impersonate the client and
        // receive the scoped credential before Windows searches PATH.
        if !directory.is_absolute() {
            continue;
        }
        let mut candidates = vec![directory.join(format!("{name}.exe"))];
        if client == Client::Claude {
            candidates
                .push(directory.join("node_modules/@anthropic-ai/claude-code/bin/claude.exe"));
        }
        candidates.extend([
            directory.join(format!("{name}.cmd")),
            directory.join(format!("{name}.bat")),
        ]);
        for candidate in candidates {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!(
        "{name} executable not found in an absolute PATH entry; install that client or connect it with `agents connect`"
    ))
}

fn client_executable(client: Client) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let path = std::env::var_os("PATH").ok_or("client PATH is unavailable")?;
        windows_client_executable(client, std::env::split_paths(&path))
    }
    #[cfg(not(windows))]
    {
        Ok(PathBuf::from(client.executable_name()))
    }
}

fn variable(connection: &HttpConnectionInfo) -> String {
    format!(
        "AIRK_MCP_TOKEN_{}",
        connection.connection_id.to_string().to_uppercase()
    )
}

fn client_arguments(
    client: Client,
    connection: &HttpConnectionInfo,
    config: Option<&Path>,
    directory: &Path,
    extra: &[OsString],
) -> Result<Vec<OsString>, String> {
    let mut arguments = Vec::new();
    match client {
        Client::Claude => {
            // Claude's --mcp-config consumes following positional values. Put
            // caller arguments first so a prompt cannot become another file.
            arguments.extend_from_slice(extra);
            arguments.push("--mcp-config".into());
            arguments.push(
                config
                    .ok_or("Claude connection configuration missing")?
                    .as_os_str()
                    .to_owned(),
            );
        }
        Client::Codex => {
            // An already running shared daemon cannot inherit this child's
            // credential/trust environment. Use the connecting process here.
            arguments.push("--no-daemon".into());
            // Replaces only the named AIRK server for this process, including an
            // obsolete command field. Other configured servers stay untouched.
            let value = format!(
                "mcp_servers.airk={{enabled=true,url={},bearer_token_env_var={},http_headers={{X-AIRK-Response-Mode=\"terse\"}}}}",
                serde_json::to_string(&connection.endpoint.url).map_err(|e| e.to_string())?,
                serde_json::to_string(&variable(connection)).map_err(|e| e.to_string())?
            );
            arguments.extend([OsString::from("-c"), OsString::from(value)]);
            arguments.extend_from_slice(extra);
        }
        Client::Vscode => {
            // VS Code's AIRK entry takes its bearer from the AIRK extension's
            // `airk.httpCredential` input, not from this environment. A new
            // window keeps the additive trust in the process it starts; an
            // already running VS Code may open the folder in its own process.
            arguments.push("--new-window".into());
            arguments.extend_from_slice(extra);
            arguments.push(directory.as_os_str().to_owned());
        }
    }
    Ok(arguments)
}

fn combine_certificates(
    new_ca: &[u8],
    existing: impl IntoIterator<Item = Vec<u8>>,
) -> Result<Vec<u8>, String> {
    if new_ca.is_empty() {
        return Err("native AIRK CA is unavailable".into());
    }
    let mut pieces: Vec<Vec<u8>> = Vec::new();
    for bytes in existing.into_iter().chain(std::iter::once(new_ca.to_vec())) {
        if !pieces.contains(&bytes) {
            if pieces.iter().map(Vec::len).sum::<usize>() + bytes.len() + pieces.len()
                >= MAX_CA_BYTES
            {
                return Err("additive client CA bundle exceeds bound".into());
            }
            pieces.push(bytes);
        }
    }
    let mut result = Vec::new();
    for bytes in pieces {
        result.extend(bytes);
        if !result.ends_with(b"\n") {
            result.push(b'\n');
        }
    }
    Ok(result)
}

fn read_ca(path: &Path) -> Result<Vec<u8>, String> {
    let bound = || "existing client CA bundle exceeds bound".to_owned();
    let file = std::fs::File::open(path)
        .map_err(|error| format!("read existing client CA bundle: {error}"))?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_CA_BYTES as u64 {
        return Err(bound());
    }
    let mut bytes = Vec::new();
    file.take((MAX_CA_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > MAX_CA_BYTES {
        return Err(bound());
    }
    Ok(bytes)
}

/// Create a new owner-private file in the owner-private `root`. On Windows the
/// file inherits the root's owner-only ACL; on Unix it is created 0600. The
/// result is checked like every other owner-private read.
fn create_private(root: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if name.contains(['/', '\\']) {
        return Err("client file name is invalid".into());
    }
    airk_ipc::endpoint::require_existing_root(root).map_err(|error| format!("{error:#}"))?;
    let mut file = tempfile::NamedTempFile::new_in(root).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    airk_ipc::endpoint::require_owner_private(file.path()).map_err(|error| format!("{error:#}"))?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    file.persist_noclobber(root.join(name))
        .map_err(|error| error.error.to_string())?;
    Ok(())
}

/// A content-addressed owner-private file: reused when equal, never replaced.
fn immutable(root: &Path, name: &str, bytes: &[u8], maximum: usize) -> Result<PathBuf, String> {
    match crate::agents::private_file(&root.join(name), maximum)? {
        Some(saved) if saved == bytes => {}
        Some(_) => return Err("existing client launch file differs; preserved".into()),
        None => create_private(root, name, bytes)?,
    }
    Ok(root.join(name))
}

fn plan(
    client: Client,
    connection: &HttpConnectionInfo,
    directory: &Path,
    extra: &[OsString],
    inherited: &BTreeMap<OsString, OsString>,
) -> Result<LaunchPlan, String> {
    connection
        .validate()
        .map_err(|error| format!("{error:#}"))?;
    let token_file = Path::new(&connection.credential_file);
    let root = token_file
        .parent()
        .ok_or("native credential directory missing")?;
    let canonical = root.canonicalize().map_err(|error| error.to_string())?;
    if canonical != root
        || token_file.file_name().and_then(|name| name.to_str())
            != Some(
                airk_core::service_http_credentials::filename(connection.connection_id).as_str(),
            )
        || Path::new(&connection.ca_certificate_file) != root.join("ca.pem")
    {
        return Err("native client references crossed connection".into());
    }
    let ca = crate::agents::private_file(&root.join("ca.pem"), 8192)?
        .ok_or("native AIRK CA file missing")?;
    if Sha256Digest::from_bytes(Sha256::digest(&ca).into()) != connection.endpoint.ca_sha256 {
        return Err("native AIRK CA changed before client launch".into());
    }
    let ca_variable = match client {
        Client::Claude | Client::Vscode => "NODE_EXTRA_CA_CERTS",
        Client::Codex => "CODEX_CA_CERTIFICATE",
    };
    let mut old = Vec::new();
    let mut paths = BTreeSet::new();
    for key in [ca_variable, "SSL_CERT_FILE"] {
        if let Some(path) = inherited
            .get(&OsString::from(key))
            .filter(|path| !path.is_empty())
            && paths.insert(path.clone())
        {
            old.push(read_ca(Path::new(path))?);
        }
    }
    let bundle = combine_certificates(&ca, old)?;
    let digest = framed_digest(b"airk/standalone-additive-client-ca/1", &[&bundle]);
    let bundle = immutable(
        root,
        &format!("standalone-client-ca-{digest}.pem"),
        &bundle,
        MAX_CA_BYTES,
    )?;
    let config = if client == Client::Claude {
        let bytes = serde_json::to_vec_pretty(&serde_json::json!({"mcpServers":{"airk":{
            "type":"http", "url":connection.endpoint.url,
            "headers":{"Authorization":format!("Bearer ${{{}}}",variable(connection)),"X-AIRK-Response-Mode":"terse"}
        }}}))
        .map_err(|error| error.to_string())?;
        let digest = framed_digest(b"airk/standalone-claude-config/1", &[&bytes]);
        Some(immutable(
            root,
            &format!(
                "standalone-claude-{}-{digest}.json",
                connection.connection_id
            ),
            &bytes,
            16 * 1024,
        )?)
    } else {
        None
    };
    let mut environment = BTreeMap::new();
    if client != Client::Vscode {
        let bytes = crate::agents::private_file(
            token_file,
            airk_core::service_http_credentials::MAX_TOKEN,
        )?
        .ok_or("HTTP client credential unavailable")?;
        let token =
            airk_core::service_http_credentials::client_token(bytes, connection.connection_id)
                .map_err(|error| format!("{error:#}"))?;
        environment.insert(variable(connection).into(), token.into());
    }
    environment.insert(ca_variable.into(), bundle.into_os_string());
    Ok(LaunchPlan {
        arguments: client_arguments(client, connection, config.as_deref(), directory, extra)?,
        environment,
    })
}

/// Run `client` in `directory` until it exits; the child's exit status is
/// returned. The service continues independently of this foreground client.
pub fn launch(
    control: &Path,
    version: &str,
    client: Client,
    connection_id: Incarnation,
    directory: &Path,
    extra: &[OsString],
) -> Result<i32, ErrorLine> {
    let directory = directory.canonicalize().map_err(refused)?;
    if !directory.is_dir() {
        return Err(refused("client working folder is not a directory"));
    }
    let info = crate::agents::info(control, version, connection_id)?;
    let fresh: HttpConnectionInfo = serde_json::from_value(info).map_err(refused)?;
    if fresh.connection_id != connection_id {
        return Err(refused("native connection changed before launch"));
    }
    let executable = client_executable(client).map_err(refused)?;
    let inherited = std::env::vars_os().collect();
    let plan = plan(client, &fresh, &directory, extra, &inherited).map_err(refused)?;
    let status = Command::new(executable)
        .current_dir(&directory)
        .args(plan.arguments)
        .envs(plan.environment)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| {
            refused(format!(
                "could not launch {}: {error}; install that client or connect it with `agents connect`",
                client.label()
            ))
        })?;
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        Ok(status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
    }
    #[cfg(not(unix))]
    {
        Ok(status.code().unwrap_or(1))
    }
}

#[cfg(test)]
mod tests;
