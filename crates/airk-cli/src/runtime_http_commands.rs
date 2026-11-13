// SPDX-License-Identifier: Apache-2.0
//! Setup-only HTTP discovery and enrollment through the verified native service.
//! Moved from `runtime_install::http_commands` (split step 2b): it validates
//! selections through `http_backend`, above the installer.
use crate::runtime_install::*;
use crate::{control_client, local_ipc, runtime_pair};
use airk_core::service::{ControlMethod, ControlResponse, ServiceHealth};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service_http::{EndpointDescriptor, HttpSelection},
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(test)]
mod tests;

/// The header helper for agents started outside VS Code. It authenticates the
/// owner-private credential of an existing, unrevoked connection against the
/// saved installation binding. It needs no running service and never starts
/// one, enrolls, rotates or writes; error messages never contain the token.
pub(crate) fn credential_header(install: &Path, id: Incarnation) -> Result<serde_json::Value> {
    let control = crate::install_layout::saved_control(install)?;
    let (binding, _) = crate::http_endpoint::trust::saved_identity(&control)?;
    let private = control.join("http-1");
    let token = crate::http_endpoint::read_client_token(&private, id)?;
    let store = crate::http_endpoint::CredentialStore::read_existing(&private, binding)?;
    let now: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?;
    let authenticated = store.authenticate(&token, now)?;
    ensure!(
        authenticated.connection_id == id,
        "native HTTP connection identity changed"
    );
    Ok(serde_json::json!({ "Authorization": format!("Bearer {token}") }))
}

async fn control(install: &Path) -> Result<(PathBuf, ServiceHealth)> {
    ensure!(
        !state_contract::required(install)?,
        "clean_break_required: HTTP enrollment requires the initialized baseline"
    );
    healthy(installed_control(install).await?).await
}
/// The service's own health at a known control root, accepting requests.
async fn healthy(control: PathBuf) -> Result<(PathBuf, ServiceHealth)> {
    let response = control_client::request(&control, ControlMethod::Health).await?;
    let ControlResponse::Health { observation } = response else {
        return refused(response);
    };
    ensure!(observation.policy.accepting, "native service is draining");
    Ok((control, *observation))
}
fn refused<T>(response: ControlResponse) -> Result<T> {
    // The canonical native refusal remains complete in the diagnostic. No
    // credential bytes are ever emitted by these commands or control responses.
    bail!(
        "native HTTP setup refused: {}",
        serde_json::to_string(&response)?
    )
}
fn check_endpoint(
    control: &Path,
    endpoint: &EndpointDescriptor,
    health: &ServiceHealth,
) -> Result<()> {
    endpoint.validate()?;
    let binding = crate::http_endpoint::installation_binding(control, endpoint.namespace)?;
    ensure!(
        endpoint.installation == binding.installation && endpoint.user == binding.user,
        "native HTTP installation binding changed"
    );
    ensure!(
        endpoint.service_instance == health.instance
            && endpoint.user == health.user
            && endpoint.executable_sha256.to_string() == health.executable_sha256,
        "native HTTP endpoint identity changed"
    );
    if let Some(clock) = health.catalog_clock {
        ensure!(
            endpoint.namespace == clock.namespace,
            "native HTTP endpoint namespace changed"
        );
    }
    Ok(())
}
fn same_authority(requested: &HttpSelection, observed: &HttpSelection) -> bool {
    let mut normalized = requested.clone();
    match (&mut normalized, observed) {
        (HttpSelection::Scoped { selection: a }, HttpSelection::Scoped { selection: b })
            if b.expected_catalog_revision >= a.expected_catalog_revision =>
        {
            a.expected_catalog_revision = b.expected_catalog_revision
        }
        (HttpSelection::Combined { selection: a }, HttpSelection::Combined { selection: b })
            if b.expected_catalog_revision >= a.expected_catalog_revision =>
        {
            a.expected_catalog_revision = b.expected_catalog_revision
        }
        (
            HttpSelection::ImportedLegacy { selection: a },
            HttpSelection::ImportedLegacy { selection: b },
        ) if b.expected_catalog_revision >= a.expected_catalog_revision => {
            a.expected_catalog_revision = b.expected_catalog_revision
        }
        (
            HttpSelection::ImportedLegacyUnion { selection: a },
            HttpSelection::ImportedLegacyUnion { selection: b },
        ) if a.selections.len() == b.selections.len() => {
            for (left, right) in a.selections.iter_mut().zip(&b.selections) {
                if right.expected_catalog_revision < left.expected_catalog_revision {
                    return false;
                }
                left.expected_catalog_revision = right.expected_catalog_revision;
            }
        }
        _ => return false,
    }
    normalized == *observed
}
fn connection(
    response: ControlResponse,
    control: &Path,
    health: &ServiceHealth,
    id: Incarnation,
    selection: &HttpSelection,
) -> Result<serde_json::Value> {
    let ControlResponse::HttpConnection { connection } = response else {
        return refused(response);
    };
    connection.validate()?;
    check_endpoint(control, &connection.endpoint, health)?;
    ensure!(
        connection.connection_id == id && same_authority(selection, &connection.selection),
        "native HTTP enrollment selection changed"
    );
    let private = control.join("http-1");
    ensure!(
        Path::new(&connection.ca_certificate_file) == private.join("ca.pem")
            && Path::new(&connection.credential_file)
                == private.join(format!("credential-{id}.token")),
        "native HTTP credential references changed"
    );
    Ok(serde_json::to_value(connection)?)
}
pub async fn endpoint(install: &Path) -> Result<serde_json::Value> {
    let (control, health) = control(install).await?;
    checked_endpoint(control, health).await
}
/// M15a (split step 3e.2): the same read for the service's own control root.
pub(crate) async fn endpoint_at(control: PathBuf) -> Result<serde_json::Value> {
    let (control, health) = healthy(control).await?;
    checked_endpoint(control, health).await
}
async fn checked_endpoint(control: PathBuf, health: ServiceHealth) -> Result<serde_json::Value> {
    let response = control_client::request_for_instance(
        &control,
        ControlMethod::HttpEndpoint,
        health.instance,
    )
    .await?;
    let ControlResponse::HttpEndpoint { endpoint } = response else {
        return refused(response);
    };
    let endpoint = endpoint.context("native HTTPS endpoint is not available")?;
    check_endpoint(&control, &endpoint, &health)?;
    Ok(serde_json::to_value(endpoint)?)
}
/// Resolve only an existing native credential. The secret stays inside this
/// process; callers receive the same protected references as enrollment.
pub async fn existing_connection(install: &Path, id: Incarnation) -> Result<serde_json::Value> {
    let (control, health) = control(install).await?;
    checked_connection(control, health, id).await
}
/// M15 (split step 3e.2): the same read for the service's own control root.
pub(crate) async fn existing_connection_at(
    control: PathBuf,
    id: Incarnation,
) -> Result<serde_json::Value> {
    let (control, health) = healthy(control).await?;
    checked_connection(control, health, id).await
}
async fn checked_connection(
    control: PathBuf,
    health: ServiceHealth,
    id: Incarnation,
) -> Result<serde_json::Value> {
    let response = control_client::request_for_instance(
        &control,
        ControlMethod::HttpEndpoint,
        health.instance,
    )
    .await?;
    let ControlResponse::HttpEndpoint { endpoint } = response else {
        return refused(response);
    };
    let endpoint = endpoint.context("native HTTPS endpoint is not available")?;
    check_endpoint(&control, &endpoint, &health)?;
    resolved_existing_connection(control, health, *endpoint, id).await
}

async fn resolved_existing_connection(
    control: PathBuf,
    health: ServiceHealth,
    endpoint: EndpointDescriptor,
    id: Incarnation,
) -> Result<serde_json::Value> {
    check_endpoint(&control, &endpoint, &health)?;
    let binding = crate::http_endpoint::installation_binding(&control, endpoint.namespace)?;
    let private = control.join("http-1");
    let token_root = private.clone();
    let authenticated = tokio::task::spawn_blocking(move || -> Result<_> {
        let token = crate::http_endpoint::read_client_token(&token_root, id)?;
        let store = crate::http_endpoint::CredentialStore::read_existing(&token_root, binding)?;
        let now: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            .try_into()?;
        store.authenticate(&token, now)
    })
    .await??;
    ensure!(
        authenticated.connection_id == id,
        "native HTTP connection identity changed"
    );
    let context = airk_mcp::read_attempt::ReadAttemptContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        tokio_util::sync::CancellationToken::new(),
    );
    let selection =
        crate::http_backend::validate(&control, &authenticated.selection, &context).await?;
    let connection = airk_core::service_http::HttpConnectionInfo {
        schema: airk_core::service_http::HTTP_CONTRACT_VERSION,
        connection_id: id,
        endpoint,
        selection,
        ca_certificate_file: private
            .join("ca.pem")
            .to_str()
            .context("HTTP CA path not UTF-8")?
            .into(),
        credential_file: private
            .join(format!("credential-{id}.token"))
            .to_str()
            .context("HTTP credential path not UTF-8")?
            .into(),
    };
    connection.validate()?;
    Ok(serde_json::to_value(connection)?)
}

/// Typed configuration ownership lookup. It never starts a service, enrolls a
/// credential, mutates a selector or treats a revoked/missing secret as foreign.
pub async fn lookup_connection(
    install: &Path,
    expected: &str,
    id: Incarnation,
) -> Result<serde_json::Value> {
    let _lock = existing_install_lock(install)?;
    ensure!(
        selection_record(install)?.active == expected,
        "HTTP lookup selected runtime changed"
    );
    let (main, approval) = verify_runtime(install, expected)?;
    let control = crate::install_layout::saved_control(install)?;
    local_ipc::require_existing_root(&control)?;
    let ControlResponse::Health {
        observation: health,
    } = control_client::request_for_version(&control, ControlMethod::Health, &approval.version)
        .await?
    else {
        bail!("HTTP lookup native health unavailable");
    };
    ensure!(
        health.runtime_sha256.as_deref() == Some(expected)
            && health.executable_sha256 == runtime_pair::digest_file(&main)?
            && health.policy.accepting,
        "HTTP lookup native owner is unavailable or changed"
    );
    let mut reply = ownership(control, *health, id).await?;
    reply["active"] = expected.into();
    Ok(reply)
}
/// M16 (split step 3e.4): the same ownership read at the service's own
/// control root, without the selected-runtime and staged-selection checks.
pub(crate) async fn lookup_connection_at(
    control: PathBuf,
    id: Incarnation,
) -> Result<serde_json::Value> {
    let (control, health) = healthy(control).await?;
    ownership(control, health, id).await
}
async fn ownership(
    control: PathBuf,
    health: ServiceHealth,
    id: Incarnation,
) -> Result<serde_json::Value> {
    use crate::http_endpoint::{ConnectionOwnership, CredentialStore};
    let response = control_client::request_for_instance(
        &control,
        ControlMethod::HttpEndpoint,
        health.instance,
    )
    .await?;
    let ControlResponse::HttpEndpoint { endpoint } = response else {
        return refused(response);
    };
    let endpoint = endpoint.context("HTTP lookup endpoint unavailable")?;
    check_endpoint(&control, &endpoint, &health)?;
    let binding = crate::http_endpoint::installation_binding(&control, endpoint.namespace)?;
    let private = control.join("http-1");
    let (ownership, attachment_retirement) = tokio::task::spawn_blocking(move || -> Result<_> {
        let store = CredentialStore::read_existing(&private, binding)?;
        let now: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            .try_into()?;
        Ok((
            store.ownership(id, now)?,
            store.attachment_retirement(id, now)?,
        ))
    })
    .await??;
    let mut reply = serde_json::json!({"schema":1,"connection_id":id});
    if let Some(proof) = attachment_retirement {
        reply["status"] = "unavailable".into();
        reply["code"] = "owned_connection_attachment_retired".into();
        reply["attachment_retirement"] = serde_json::to_value(proof)?;
        reply["guidance"] = "Only the exact original managed standalone claim may replace this retired attachment using a fresh ID and native mutation-time retirement validation.".into();
        return Ok(reply);
    }
    match ownership {
        ConnectionOwnership::NotOwned => reply["status"] = "not_owned".into(),
        ConnectionOwnership::Revoked | ConnectionOwnership::Expired => {
            reply["status"] = "unavailable".into();
            reply["code"] = if ownership == ConnectionOwnership::Revoked {
                "owned_connection_revoked"
            } else {
                "owned_connection_expired"
            }
            .into();
            reply["guidance"] = "This installation owns the ID. Retain its configuration and review renewal or revocation explicitly; do not reenroll it during migration.".into();
        }
        ConnectionOwnership::Active => {
            match resolved_existing_connection(control, health, *endpoint, id).await {
                Ok(connection) => {
                    reply["status"] = "owned".into();
                    reply["connection"] = connection;
                }
                Err(error) => {
                    reply["status"] = "unavailable".into();
                    reply["code"] = "owned_connection_unavailable".into();
                    reply["native_error"] = format!("{error:#}").into();
                    reply["guidance"] = "The owned connection could not be authenticated or its live grant observed. Retain the exact configuration and diagnose the native refusal; no credential was issued.".into();
                }
            }
        }
    }
    Ok(reply)
}
/// V1 (split step 3e.9, `agents verify`): probe this service's own `/mcp`
/// over HTTPS, pinned to the installation's public issuer, with the stored
/// credential of an existing connection. The probe initializes one MCP
/// session and deletes it. The token stays in this process and is never part
/// of the result or an error.
pub(crate) async fn verify_connection_at(
    control: PathBuf,
    id: Incarnation,
) -> Result<serde_json::Value> {
    let (control, health) = healthy(control).await?;
    let response = control_client::request_for_instance(
        &control,
        ControlMethod::HttpEndpoint,
        health.instance,
    )
    .await?;
    let ControlResponse::HttpEndpoint { endpoint } = response else {
        return refused(response);
    };
    let endpoint = endpoint.context("native HTTPS endpoint is not available")?;
    check_endpoint(&control, &endpoint, &health)?;
    let binding = crate::http_endpoint::installation_binding(&control, endpoint.namespace)?;
    let private = control.join("http-1");
    let ca_sha256 = endpoint.ca_sha256;
    let (token, ca) = tokio::task::spawn_blocking(move || -> Result<_> {
        let token = crate::http_endpoint::read_client_token(&private, id)?;
        let store = crate::http_endpoint::CredentialStore::read_existing(&private, binding)?;
        let now: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            .try_into()?;
        let authenticated = store.authenticate(&token, now)?;
        ensure!(
            authenticated.connection_id == id,
            "native HTTP connection identity changed"
        );
        Ok((
            token,
            crate::http_endpoint::trust::public_ca(&private, ca_sha256)?,
        ))
    })
    .await??;
    let url = endpoint.url;
    let mut result = serde_json::json!({
        "schema": 1,
        "connection_id": id,
        "url": url.to_string(),
    });
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "airkctl-verify", "version": env!("CARGO_PKG_VERSION")}
        }
    })
    .to_string();
    match mcp_probe(url.port(), &ca, &token, &initialize).await {
        Ok((status, session, body)) => {
            result["http_status"] = status.into();
            let answer = body.as_ref().and_then(|body| body.get("result"));
            result["verified"] = (status == 200 && answer.is_some()).into();
            if let Some(answer) = answer {
                result["protocol_version"] = answer["protocolVersion"].clone();
                result["server"] = answer["serverInfo"].clone();
            } else if let Some(error) = body.as_ref().and_then(|body| body.get("error")) {
                result["detail"] = error.clone();
            }
            if let Some(session) = session {
                // Best effort: the session would otherwise wait for idle expiry.
                let _ = mcp_close(url.port(), &ca, &token, &session).await;
            }
        }
        Err(error) => {
            result["verified"] = false.into();
            result["detail"] = format!("{error:#}").replace(&token, "<redacted>").into();
        }
    }
    Ok(result)
}

const PROBE_LIMIT: usize = 1 << 20;

async fn mcp_exchange(port: u16, ca: &[u8], request: String) -> Result<Vec<u8>> {
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut roots = rustls::RootCertStore::empty();
    roots.add(rustls::pki_types::CertificateDer::from(ca.to_vec()))?;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let exchange = async {
        let socket = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
        let mut tls = connector
            .connect(
                rustls::pki_types::ServerName::try_from("127.0.0.1")?,
                socket,
            )
            .await?;
        tls.write_all(request.as_bytes()).await?;
        let mut bytes = Vec::new();
        (&mut tls)
            .take(PROBE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .await?;
        ensure!(
            bytes.len() <= PROBE_LIMIT,
            "MCP probe response exceeds bound"
        );
        Ok::<_, anyhow::Error>(bytes)
    };
    tokio::time::timeout(std::time::Duration::from_secs(15), exchange)
        .await
        .context("MCP probe timed out")?
}

/// The status, session id and JSON-RPC body of one `initialize`.
async fn mcp_probe(
    port: u16,
    ca: &[u8],
    token: &str,
    body: &str,
) -> Result<(u16, Option<String>, Option<serde_json::Value>)> {
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let bytes = mcp_exchange(port, ca, request).await?;
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .context("MCP probe response has no header end")?;
    let head = std::str::from_utf8(&bytes[..split]).context("MCP probe header is not UTF-8")?;
    let mut lines = head.split("\r\n");
    let status: u16 = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .context("MCP probe status line is malformed")?;
    let mut session = None;
    let mut chunked = false;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("mcp-session-id") {
                session = Some(value.to_owned());
            } else if name.eq_ignore_ascii_case("transfer-encoding")
                && value.eq_ignore_ascii_case("chunked")
            {
                chunked = true;
            }
        }
    }
    let mut payload = bytes[split + 4..].to_vec();
    if chunked {
        payload = dechunk(&payload)?;
    }
    let text = String::from_utf8_lossy(&payload);
    // Streamable HTTP answers either one JSON body or `data:` events.
    let body = serde_json::from_str::<serde_json::Value>(text.trim())
        .ok()
        .or_else(|| {
            text.lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .find_map(|data| serde_json::from_str(data.trim()).ok())
        });
    Ok((status, session, body))
}

fn dechunk(mut bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let end = bytes
            .windows(2)
            .position(|window| window == b"\r\n")
            .context("MCP probe chunk is malformed")?;
        let size = std::str::from_utf8(&bytes[..end])?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim();
        let size = usize::from_str_radix(size, 16).context("MCP probe chunk size is malformed")?;
        if size == 0 {
            return Ok(out);
        }
        let start = end + 2;
        ensure!(bytes.len() >= start + size, "MCP probe chunk is truncated");
        out.extend_from_slice(&bytes[start..start + size]);
        bytes = bytes.get(start + size + 2..).unwrap_or_default();
    }
}

async fn mcp_close(port: u16, ca: &[u8], token: &str, session: &str) -> Result<()> {
    ensure!(
        session.bytes().all(|byte| byte.is_ascii_graphic()),
        "MCP session id is malformed"
    );
    let request = format!(
        "DELETE /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nMcp-Session-Id: {session}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    mcp_exchange(port, ca, request).await.map(|_| ())
}
fn client(label: &str) -> Result<airk_core::usage::ClientFamily> {
    ensure!(
        !label.is_empty()
            && label.len() <= 64
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)),
        "invalid HTTP client label"
    );
    Ok(airk_core::usage::ClientFamily::from_label(label))
}
async fn enroll_at(
    control: &Path,
    health: &ServiceHealth,
    selection: HttpSelection,
    id: Incarnation,
    client: airk_core::usage::ClientFamily,
    explorer_descriptor: Option<&Path>,
    reattach_connection_id: Option<Incarnation>,
) -> Result<serde_json::Value> {
    selection.validate()?;
    let explorer_descriptor = explorer_descriptor
        .map(|path| -> Result<String> {
            ensure!(path.is_absolute(), "Explorer descriptor must be absolute");
            Ok(path
                .to_str()
                .context("Explorer descriptor path is not UTF-8")?
                .to_owned())
        })
        .transpose()?;
    let response = control_client::request_for_instance(
        control,
        ControlMethod::HttpEnroll {
            connection_id: id,
            selection: Box::new(selection.clone()),
            client,
            explorer_descriptor,
            reattach_connection_id,
        },
        health.instance,
    )
    .await?;
    connection(response, control, health, id, &selection)
}
pub async fn enroll(
    install: &Path,
    file: &Path,
    id: Incarnation,
    label: &str,
    explorer_descriptor: Option<&Path>,
) -> Result<serde_json::Value> {
    let selection = crate::http_endpoint::read_selection_file(file)?;
    let client = client(label)?;
    let (control, health) = control(install).await?;
    enroll_at(
        &control,
        &health,
        selection,
        id,
        client,
        explorer_descriptor,
        None,
    )
    .await
}
/// CLI setup supplies an explicit existing selector, never workspace authority.
pub async fn setup(
    install: Option<&Path>,
    runtime_control: Option<&Path>,
    selection: HttpSelection,
    id: Incarnation,
    label: &str,
    explorer_descriptor: Option<&Path>,
) -> Result<serde_json::Value> {
    let client = client(label)?;
    let (control, health) = if let Some(saved) = runtime_control {
        let saved = saved.canonicalize()?;
        crate::install_layout::validate_control_root(&saved)?;
        let response = control_client::request(&saved, ControlMethod::Health).await?;
        let ControlResponse::Health { observation } = response else {
            return refused(response);
        };
        ensure!(observation.policy.accepting, "native service is draining");
        (saved, *observation)
    } else {
        let install = install
            .map(Path::to_path_buf)
            .map(Ok)
            .unwrap_or_else(caller_install_root)?;
        control(&install).await?
    };
    enroll_at(
        &control,
        &health,
        selection,
        id,
        client,
        explorer_descriptor,
        None,
    )
    .await
}

pub async fn revoke(install: &Path, id: Incarnation) -> Result<serde_json::Value> {
    let (control, health) = control(install).await?;
    let response = control_client::request_for_instance(
        &control,
        ControlMethod::HttpRevoke { connection_id: id },
        health.instance,
    )
    .await?;
    match &response {
        ControlResponse::HttpRevoked { connection_id } if *connection_id == id => {
            Ok(serde_json::to_value(response)?)
        }
        _ => refused(response),
    }
}
#[allow(clippy::too_many_arguments)]
pub async fn enroll_legacy(
    install: &Path,
    file: &Path,
    receipt: Sha256Digest,
    id: Incarnation,
    label: &str,
    explorer_descriptor: Option<&Path>,
    origin_sha256: Option<Sha256Digest>,
    reattach_connection_id: Option<Incarnation>,
) -> Result<serde_json::Value> {
    let registration = crate::http_endpoint::read_registration_file(file)?;
    let client = client(label)?;
    let (control, health) = control(install).await?;
    let resolution = {
        let mut registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
            &control,
            Some(std::time::Instant::now() + Duration::from_secs(2)),
        )?;
        http_legacy::resolve_claim(&mut registry, registration, receipt, origin_sha256)?
    };
    match resolution {
        http_legacy::LegacyMigrationResolution::Eligible { selection } => {
            enroll_at(
                &control,
                &health,
                HttpSelection::ImportedLegacy { selection },
                id,
                client,
                explorer_descriptor,
                reattach_connection_id,
            )
            .await
        }
        http_legacy::LegacyMigrationResolution::ReviewRequired {
            registration,
            configuration_receipt,
        } => Ok(
            serde_json::json!({"schema":1,"status":"review_required","code":"legacy_selection_review_required","registration":registration,"configuration_receipt":configuration_receipt}),
        ),
    }
}

/// Inspection authenticates existing native metadata and observes the live
/// service. It never elects a service, provisions a CA or enrolls a credential.
pub async fn trust(
    install: &Path,
    action: HttpTrustAction,
    expected_ca: Option<Sha256Digest>,
) -> Result<serde_json::Value> {
    ensure!(
        action == HttpTrustAction::Inspect || expected_ca.is_some(),
        "install/remove requires the reviewed issuer SHA256"
    );
    let control = installed_metadata_control(install)?;
    if action != HttpTrustAction::Install {
        let status = tokio::task::spawn_blocking(move || {
            let (binding, endpoint) = crate::http_endpoint::trust::saved_identity(&control)?;
            check_reviewed_ca(action, expected_ca, endpoint.ca_sha256)?;
            crate::http_endpoint::trust::manage(
                &control.join("http-1"),
                binding,
                endpoint.ca_sha256,
                if action == HttpTrustAction::Remove {
                    crate::http_endpoint::trust::Action::Remove
                } else {
                    crate::http_endpoint::trust::Action::Inspect
                },
            )
        })
        .await??;
        return Ok(serde_json::to_value(status)?);
    }
    let approval = match managed::read(install)? {
        Some(policy) => policy.metadata_approval()?,
        None => selected_preflight(install)?.approval,
    };
    let response = control_client::request(&control, ControlMethod::Health).await?;
    let ControlResponse::Health { observation } = response else {
        return refused(response);
    };
    require_matching_health(&approval, &observation)?;
    let response = control_client::request_for_instance(
        &control,
        ControlMethod::HttpEndpoint,
        observation.instance,
    )
    .await?;
    let ControlResponse::HttpEndpoint { endpoint } = response else {
        return refused(response);
    };
    let endpoint = endpoint.context(
        "native HTTPS endpoint is not available; complete native setup before managing user trust",
    )?;
    check_endpoint(&control, &endpoint, &observation)?;
    check_reviewed_ca(action, expected_ca, endpoint.ca_sha256)?;
    let binding = crate::http_endpoint::installation_binding(&control, endpoint.namespace)?;
    let action = match action {
        HttpTrustAction::Inspect => crate::http_endpoint::trust::Action::Inspect,
        HttpTrustAction::Install => crate::http_endpoint::trust::Action::Install,
        HttpTrustAction::Remove => crate::http_endpoint::trust::Action::Remove,
    };
    let root = control.join("http-1");
    let status = tokio::task::spawn_blocking(move || {
        crate::http_endpoint::trust::manage(&root, binding, endpoint.ca_sha256, action)
    })
    .await??;
    Ok(serde_json::to_value(status)?)
}

fn check_reviewed_ca(
    action: HttpTrustAction,
    expected: Option<Sha256Digest>,
    current: Sha256Digest,
) -> Result<()> {
    ensure!(
        action == HttpTrustAction::Inspect || expected.is_some(),
        "install/remove requires the reviewed issuer SHA256"
    );
    ensure!(
        expected.is_none_or(|reviewed| reviewed == current),
        "native HTTPS issuer changed after review"
    );
    Ok(())
}
pub async fn enroll_legacy_union(
    install: &Path,
    file: &Path,
    id: Incarnation,
    label: &str,
    explorer_descriptor: Option<&Path>,
) -> Result<serde_json::Value> {
    let input = crate::http_endpoint::read_registrations_file(file)?;
    let client = client(label)?;
    let (control, health) = control(install).await?;
    let outcomes = {
        let mut registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
            &control,
            Some(std::time::Instant::now() + Duration::from_secs(2)),
        )?;
        input
            .registrations
            .into_iter()
            .map(|member| {
                http_legacy::resolve(
                    &mut registry,
                    member.registration,
                    member.configuration_receipt,
                )
            })
            .collect::<Result<Vec<_>>>()?
    };
    if outcomes.iter().any(|member| {
        matches!(
            member,
            http_legacy::LegacyMigrationResolution::ReviewRequired { .. }
        )
    }) {
        return Ok(
            serde_json::json!({"schema":1,"status":"review_required","code":"legacy_union_review_required","no_default_project":true,"resolutions":outcomes}),
        );
    }
    let selections = outcomes
        .into_iter()
        .map(|member| match member {
            http_legacy::LegacyMigrationResolution::Eligible { selection } => Ok(*selection),
            _ => bail!("legacy union resolution changed"),
        })
        .collect::<Result<Vec<_>>>()?;
    let namespace = selections
        .first()
        .context("legacy union is empty")?
        .namespace;
    let selection = HttpSelection::ImportedLegacyUnion {
        selection: airk_core::service_http::NativeImportedHttpUnion {
            namespace,
            selections,
            no_default_project: true,
        },
    };
    selection.validate()?;
    enroll_at(
        &control,
        &health,
        selection,
        id,
        client,
        explorer_descriptor,
        None,
    )
    .await
}
