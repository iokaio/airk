// SPDX-License-Identifier: Apache-2.0
//! The `/admin` plane against a live reviewed service (split Phase 5): A1 via
//! `airkctl admin url`, the launch-code exchange over real TLS pinned to the
//! service's issuer, admin reads through the canonical handlers, a keyed
//! mutation round trip, the admin read grant (A2), and the cross-plane
//! refusals.
mod common;
use airk_core::{
    service::{ControlMethod, ControlResponse, ToolResponseMode, ToolSelection},
    service_http::HttpSelection,
    usage::ClientFamily,
};
use anyhow::{Context, Result, ensure};
use rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Platforms with a same-user peer observer (review H1); elsewhere the admin
/// API is refused with `admin_peer_refused` (fail closed).
const PEER_PROOF: bool = cfg!(any(windows, target_os = "linux"));

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
    fn code(&self) -> String {
        self.json()["error"]["code"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

fn dechunk(mut bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line = bytes
            .windows(2)
            .position(|window| window == b"\r\n")
            .context("chunk size line")?;
        let size = usize::from_str_radix(
            std::str::from_utf8(&bytes[..line])?
                .split(';')
                .next()
                .unwrap_or_default()
                .trim(),
            16,
        )?;
        bytes = &bytes[line + 2..];
        if size == 0 {
            return Ok(out);
        }
        ensure!(bytes.len() >= size + 2, "truncated chunk");
        out.extend_from_slice(&bytes[..size]);
        bytes = &bytes[size + 2..];
    }
}

/// One request on a fresh TLS connection pinned to the service's issuer.
fn exchange(port: u16, ca: &CertificateDer<'static>, request: String) -> Result<Reply> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let bytes = runtime.block_on(async {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(ca.clone())?;
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let exchange = async {
            let socket =
                tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
            let mut tls = connector
                .connect(ServerName::try_from("127.0.0.1")?, socket)
                .await?;
            tls.write_all(request.as_bytes()).await?;
            let mut bytes = Vec::new();
            (&mut tls).take(8 << 20).read_to_end(&mut bytes).await?;
            Ok::<_, anyhow::Error>(bytes)
        };
        tokio::time::timeout(Duration::from_secs(60), exchange)
            .await
            .context("admin request timed out")?
    })?;
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .context("response has no header end")?;
    let head = std::str::from_utf8(&bytes[..split])?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .context("malformed status line")?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let mut body = bytes[split + 4..].to_vec();
    if headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("transfer-encoding") && value.eq_ignore_ascii_case("chunked")
    }) {
        body = dechunk(&body)?;
    }
    Ok(Reply {
        status,
        headers,
        body,
    })
}

fn http_request(method: &str, target: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut text = format!("{method} {target} HTTP/1.1\r\n");
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    if method == "POST" {
        text.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    text.push_str("Connection: close\r\n\r\n");
    text.push_str(body);
    text
}

/// `airkctl admin url`: (exit code, stdout, stderr).
fn admin_url(control: &Path) -> (u8, String, String) {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut io = airkctl::cli::Io {
        stdin: &mut std::io::empty(),
        stdout: &mut stdout,
        stderr: &mut stderr,
        full_responses: false,
        window: None,
    };
    let line = [
        "airkctl",
        "--service-runtime-dir",
        control.to_str().unwrap(),
        "admin",
        "url",
    ]
    .into_iter()
    .map(std::ffi::OsString::from)
    .collect();
    let code = airkctl::cli::run(line, &mut io);
    (
        code,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

/// (port, code) of a launch URL.
fn parse_launch(url: &str) -> (u16, String) {
    let rest = url
        .trim()
        .strip_prefix("https://127.0.0.1:")
        .expect("loopback launch URL");
    let (port, fragment) = rest.split_once("/admin/launch#c=").expect("launch path");
    (port.parse().unwrap(), fragment.to_owned())
}

#[test]
fn admin_sign_in_reads_and_cross_plane_refusals() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let service = common::Service::reviewed(source.path(), data.path(), "admin-plane");
    let control = service.control.clone();
    let ca = CertificateDer::from_pem_slice(&std::fs::read(control.join("http-1").join("ca.pem"))?)
        .context("issuer certificate")?;

    let (code, stdout, stderr) = admin_url(&control);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("single-use"), "{stderr}");
    let (port, launch_code) = parse_launch(&stdout);
    assert_eq!(launch_code.len(), 43);
    let host = format!("127.0.0.1:{port}");
    let origin = format!("https://{host}");
    let api = |target: &str, extra: &[(&str, &str)], body: &str| -> Result<Reply> {
        let mut headers = vec![
            ("Host", host.as_str()),
            ("Origin", origin.as_str()),
            ("Sec-Fetch-Site", "same-origin"),
            ("Content-Type", "application/json"),
        ];
        headers.extend_from_slice(extra);
        exchange(port, &ca, http_request("POST", target, &headers, body))
    };

    // The static shell needs no session; a bearer is refused on /admin.
    let shell = exchange(
        port,
        &ca,
        http_request("GET", "/admin/", &[("Host", host.as_str())], ""),
    )?;
    assert_eq!(shell.status, 200);
    assert_eq!(shell.header("cache-control"), Some("no-store"));
    let bearer = exchange(
        port,
        &ca,
        http_request(
            "GET",
            "/admin/",
            &[("Host", host.as_str()), ("Authorization", "Bearer x")],
            "",
        ),
    )?;
    assert_eq!(
        (bearer.status, bearer.code().as_str()),
        (401, "admin_bearer_refused")
    );

    let session_body = json!({ "code": launch_code }).to_string();
    let signed_in = api("/admin/api/v1/session", &[], &session_body)?;
    if !PEER_PROOF {
        assert_eq!(
            (signed_in.status, signed_in.code().as_str()),
            (403, "admin_peer_refused")
        );
        return service.shutdown_reviewed();
    }
    assert_eq!(
        signed_in.status,
        200,
        "{}",
        String::from_utf8_lossy(&signed_in.body)
    );
    let set_cookie = signed_in.header("set-cookie").context("session cookie")?;
    let cookie = set_cookie
        .split(';')
        .next()
        .context("cookie pair")?
        .to_owned();
    assert!(cookie.starts_with("__Host-airk_admin="), "{set_cookie}");
    let envelope = signed_in.json();
    assert_eq!(envelope["operation"], "session.exchange");
    let csrf = envelope["result"]["csrf"]
        .as_str()
        .context("anti-forgery token")?
        .to_owned();

    // Replay of a consumed code.
    let replay = api("/admin/api/v1/session", &[], &session_body)?;
    assert_eq!(
        (replay.status, replay.code().as_str()),
        (401, "admin_launch_code_invalid")
    );

    let authenticated = [("Cookie", cookie.as_str()), ("X-AIRK-CSRF", csrf.as_str())];
    let health = api(
        "/admin/api/v1/call/service.health",
        &authenticated,
        "\"health\"",
    )?;
    assert_eq!(
        health.status,
        200,
        "{}",
        String::from_utf8_lossy(&health.body)
    );
    let health = health.json();
    assert_eq!(
        (health["ok"].clone(), health["operation"].clone()),
        (json!(true), json!("service.health"))
    );
    assert_eq!(health["result"]["status"], "health");

    // indexes.list answers exactly what the pipe answers.
    let projects = json!({"projects": {"after": null, "revision": null, "limit": 32}}).to_string();
    let listed = api("/admin/api/v1/call/indexes.list", &authenticated, &projects)?;
    assert_eq!(
        listed.status,
        200,
        "{}",
        String::from_utf8_lossy(&listed.body)
    );
    let pipe = service.request(ControlMethod::Projects {
        after: None,
        revision: None,
        limit: 32,
    });
    assert_eq!(listed.json()["result"], serde_json::to_value(pipe)?);

    // The cookie alone is not a session: the anti-forgery token is required.
    let missing = api(
        "/admin/api/v1/call/service.health",
        &[("Cookie", cookie.as_str())],
        "\"health\"",
    )?;
    assert_eq!(
        (missing.status, missing.code().as_str()),
        (401, "admin_session_invalid")
    );
    let unknown = api(
        "/admin/api/v1/call/no.such_operation",
        &authenticated,
        "\"health\"",
    )?;
    assert_eq!(
        (unknown.status, unknown.code().as_str()),
        (404, "admin_operation_unknown")
    );

    // The admin cookie is refused on the bearer plane.
    let mcp = exchange(
        port,
        &ca,
        http_request(
            "POST",
            "/mcp",
            &[
                ("Host", host.as_str()),
                ("Cookie", cookie.as_str()),
                ("Content-Type", "application/json"),
            ],
            "{}",
        ),
    )?;
    assert_eq!(mcp.status, 403);
    assert_eq!(mcp.json()["error"]["code"], "cookie_on_bearer_plane");

    // Phase 5b: pause and resume through the canonical handler. The
    // service generates the request nonce; the browser never sends one.
    let binding = service.project.binding.to_string();
    for paused in [true, false] {
        let body = json!({"pause": {"project": binding, "paused": paused}}).to_string();
        let reply = api("/admin/api/v1/call/indexes.pause", &authenticated, &body)?;
        assert_eq!(
            reply.status,
            200,
            "{}",
            String::from_utf8_lossy(&reply.body)
        );
        let envelope = reply.json();
        assert_eq!(envelope["operation"], "indexes.pause");
        assert_eq!(envelope["result"]["status"], "registered");
        assert_eq!(envelope["result"]["project"]["paused"], json!(paused));
        assert_eq!(envelope["request"]["state"], "completed");
        let nonce = envelope["request"]["nonce"]
            .as_str()
            .context("request nonce")?;
        assert!(
            nonce.len() == 32
                && nonce
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "{nonce}"
        );
    }

    // A2: the admin read grant over the reviewed fixture's index at the
    // current catalog revision, bound to this session.
    let HttpSelection::Scoped { selection } = service.http_selection() else {
        anyhow::bail!("the reviewed fixture's selection is scoped");
    };
    let ControlResponse::Health { observation } = service.request(ControlMethod::Health) else {
        anyhow::bail!("health unavailable");
    };
    let clock = observation.catalog_clock.context("catalog clock")?;
    let grant = json!({"admin_scope_grant": {"selection": {
        "namespace": clock.namespace,
        "expected_catalog_revision": clock.revision,
        "index_ids": [selection.index_id],
        "project_ids": selection.project_ids,
    }}})
    .to_string();
    let granted = api(
        "/admin/api/v1/call/search.select_scope",
        &authenticated,
        &grant,
    )?;
    assert_eq!(
        granted.status,
        200,
        "{}",
        String::from_utf8_lossy(&granted.body)
    );
    let granted = granted.json();
    assert_eq!(granted["result"]["status"], "admin_scope_granted");
    assert_eq!(granted["result"]["catalog_revision"], json!(clock.revision));
    // Reads under an in-memory grant wait for their scoped-read admission
    // (admin_plane/CONTRACT.md); the pipe never resolves an admin selection.
    let search = json!({"tools_call": {"tool": "airk_status", "arguments": {},
        "selection": "admin", "response_mode": "terse", "usage_client": "unknown"}})
    .to_string();
    let searched = api("/admin/api/v1/call/search.call", &authenticated, &search)?;
    assert_eq!(
        (searched.status, searched.code().as_str()),
        (503, "not_yet_served")
    );
    let pipe = service.request(ControlMethod::ToolsCall {
        tool: "airk_status".into(),
        arguments: Default::default(),
        selection: ToolSelection::Admin,
        response_mode: ToolResponseMode::Terse,
        usage_client: ClientFamily::Unknown,
    });
    assert_eq!(
        serde_json::to_value(pipe)?,
        json!({"status": "failure", "code": "admin_scope_not_granted"})
    );

    // Logout ends the session.
    let logout = api("/admin/api/v1/session/logout", &authenticated, "{}")?;
    assert_eq!(logout.status, 200);
    let after = api(
        "/admin/api/v1/call/service.health",
        &authenticated,
        "\"health\"",
    )?;
    assert_eq!(
        (after.status, after.code().as_str()),
        (401, "admin_session_invalid")
    );
    service.shutdown_reviewed()
}
