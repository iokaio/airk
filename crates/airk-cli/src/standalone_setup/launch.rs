// SPDX-License-Identifier: Apache-2.0
//! Foreground client startup with child-only credentials and additive trust.
use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::Read,
    process::{Command, Stdio},
};

const MAX_CA_BYTES: usize = 16 * 1024 * 1024;

pub fn launch_instructions(
    client: Client,
    connection: &HttpConnectionInfo,
    install: &Path,
    directory: Option<&Path>,
) -> Value {
    let mut arguments = vec![
        "--service-install-dir".to_owned(),
        install.to_string_lossy().into_owned(),
    ];
    if let Some(directory) = directory {
        arguments.extend(["--root".into(), directory.to_string_lossy().into_owned()]);
    }
    arguments.extend([
        "setup".into(),
        "--client".into(),
        client.label().into(),
        "--existing-connection-id".into(),
        connection.connection_id.to_string(),
        "--launch".into(),
    ]);
    serde_json::json!({
        "command": "airk", "client": client.label(),
        "arguments": arguments,
        "working_directory":directory,
        "working_directory_instruction":"Supply --root <working-folder> when no working directory is recorded; no common parent of scoped roots is inferred.",
        "credential_handling":"The native launcher reads the protected token and passes it only to the child process environment.",
        "trust_handling":"The child receives an additive CA bundle; existing extra CA files and public/system defaults remain enabled.",
        "configuration_handling":"Per-session AIRK configuration preserves existing project files and unrelated client settings."
    })
}

struct LaunchPlan {
    arguments: Vec<OsString>,
    environment: BTreeMap<OsString, OsString>,
}

#[cfg(any(windows, test))]
fn windows_client_executable(
    client: Client,
    directories: impl IntoIterator<Item = PathBuf>,
) -> Result<PathBuf> {
    for directory in directories {
        // Do not let an untrusted current project impersonate the client and
        // receive the scoped credential before Windows searches PATH.
        if !directory.is_absolute() {
            continue;
        }
        let mut candidates = vec![directory.join(format!("{}.exe", client.label()))];
        if client == Client::Claude {
            candidates
                .push(directory.join("node_modules/@anthropic-ai/claude-code/bin/claude.exe"));
        }
        candidates.extend([
            directory.join(format!("{}.cmd", client.label())),
            directory.join(format!("{}.bat", client.label())),
        ]);
        for candidate in candidates {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    bail!(
        "{} executable not found in an absolute PATH entry; install that client or use setup without --launch",
        client.label()
    )
}

fn client_executable(client: Client) -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let path = std::env::var_os("PATH").context("client PATH is unavailable")?;
        windows_client_executable(client, std::env::split_paths(&path))
    }
    #[cfg(not(windows))]
    {
        Ok(PathBuf::from(client.label()))
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
    extra: &[OsString],
) -> Result<Vec<OsString>> {
    // Claude's --mcp-config consumes following positional values. Put caller
    // arguments first so a user prompt cannot become another configuration file.
    let mut arguments = Vec::new();
    match client {
        Client::Claude => {
            arguments.extend_from_slice(extra);
            arguments.push("--mcp-config".into());
            arguments.push(
                config
                    .context("Claude connection configuration missing")?
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
                serde_json::to_string(&connection.endpoint.url)?,
                serde_json::to_string(&variable(connection))?
            );
            arguments.extend([OsString::from("-c"), OsString::from(value)]);
            arguments.extend_from_slice(extra);
        }
    }
    Ok(arguments)
}

fn combine_certificates(
    new_ca: &[u8],
    existing: impl IntoIterator<Item = Vec<u8>>,
) -> Result<Vec<u8>> {
    ensure!(!new_ca.is_empty(), "native AIRK CA is unavailable");
    let mut pieces = Vec::new();
    for bytes in existing.into_iter().chain(std::iter::once(new_ca.to_vec())) {
        if !pieces.contains(&bytes) {
            ensure!(
                pieces.iter().map(Vec::len).sum::<usize>() + bytes.len() + pieces.len()
                    < MAX_CA_BYTES,
                "additive client CA bundle exceeds bound"
            );
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

fn read_ca(path: &Path) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).context("read existing client CA bundle")?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= MAX_CA_BYTES as u64,
        "existing client CA bundle exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take((MAX_CA_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_CA_BYTES,
        "existing client CA bundle exceeds bound"
    );
    Ok(bytes)
}

fn immutable(root: &Path, name: &str, bytes: &[u8], maximum: usize) -> Result<PathBuf> {
    match crate::http_endpoint::read_private(root, name, maximum)? {
        Some(saved) => ensure!(
            saved == bytes,
            "existing standalone client file differs; preserved"
        ),
        None => write_private_file(root, name, bytes, maximum, false)?,
    }
    Ok(root.join(name))
}

fn plan(
    client: Client,
    connection: &HttpConnectionInfo,
    extra: &[OsString],
    inherited: &BTreeMap<OsString, OsString>,
) -> Result<LaunchPlan> {
    connection.validate()?;
    let token_file = Path::new(&connection.credential_file);
    let root = token_file
        .parent()
        .context("native credential directory missing")?;
    ensure!(
        root.canonicalize()? == root
            && token_file.file_name().and_then(|name| name.to_str())
                == Some(format!("credential-{}.token", connection.connection_id).as_str())
            && Path::new(&connection.ca_certificate_file) == root.join("ca.pem"),
        "native client references crossed connection"
    );
    let token = crate::http_endpoint::read_client_token(root, connection.connection_id)?;
    let ca = crate::http_endpoint::read_private(root, "ca.pem", 8192)?
        .context("native AIRK CA file missing")?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&ca).into()) == connection.endpoint.ca_sha256,
        "native AIRK CA changed before client launch"
    );
    let ca_variable = match client {
        Client::Claude => "NODE_EXTRA_CA_CERTS",
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
        }}}))?;
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
    environment.insert(variable(connection).into(), token.into());
    environment.insert(ca_variable.into(), bundle.into_os_string());
    Ok(LaunchPlan {
        arguments: client_arguments(client, connection, config.as_deref(), extra)?,
        environment,
    })
}

/// Revalidate existing native ownership/grant immediately before reading the
/// token. The service continues independently of this foreground client.
pub async fn launch_client(
    client: Client,
    connection: &HttpConnectionInfo,
    install: Option<&Path>,
    extra: &[OsString],
    directory: &Path,
) -> Result<i32> {
    let directory = directory.canonicalize()?;
    ensure!(
        directory.is_dir(),
        "client working folder is not a directory"
    );
    let install = installation(install)?;
    let fresh: HttpConnectionInfo = serde_json::from_value(
        crate::runtime_commands::existing_http_connection(&install, connection.connection_id)
            .await?,
    )?;
    ensure!(
        fresh.connection_id == connection.connection_id,
        "native connection changed before launch"
    );
    let executable = client_executable(client)?;
    let inherited = std::env::vars_os().collect();
    let plan = plan(client, &fresh, extra, &inherited)?;
    let status = Command::new(executable).current_dir(directory).args(plan.arguments).envs(plan.environment)
        .stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit())
        .status().with_context(||format!("could not launch {}; install that client or run setup without --launch for its configuration",client.label()))?;
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
