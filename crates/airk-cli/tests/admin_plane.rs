// SPDX-License-Identifier: Apache-2.0
//! The `/admin` plane against a live reviewed service (split Phase 5): A1 via
//! `airkctl admin url`, the launch-code exchange over real TLS pinned to the
//! service's issuer, admin reads through the canonical handlers, a keyed
//! mutation round trip, the admin read grant (A2), and the cross-plane
//! refusals.
mod common;
use airk_core::{
    service::{ControlMethod, ControlResponse, ToolResponseMode, ToolSelection},
    usage::ClientFamily,
};
use anyhow::{Context, Result, ensure};
use rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Platforms with a same-user peer observer (review H1); elsewhere the admin
/// API is refused with `admin_peer_refused` (fail closed).
const PEER_PROOF: bool = cfg!(any(windows, target_os = "linux", target_os = "macos"));

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

fn settled_answer_publication(status: &Value) -> Result<bool> {
    use airk_core::readiness::{SemanticReadiness, SemanticState};
    let readiness: SemanticReadiness =
        serde_json::from_value(status["semantic_readiness"].clone())?;
    ensure!(readiness.version == 1, "unknown semantic readiness version");
    let enabled = status["vectors_enabled"]
        .as_bool()
        .context("saved vector policy is unknown")?;
    match readiness.state {
        SemanticState::Ready => {
            ensure!(
                enabled && readiness.failure.is_none(),
                "ready semantic state contradicts saved policy"
            );
            Ok(true)
        }
        SemanticState::Disabled => {
            ensure!(
                !enabled && readiness.failure.is_none(),
                "disabled semantic state lacks declared disabled policy"
            );
            Ok(true)
        }
        SemanticState::Pending | SemanticState::Partial => Ok(false),
        SemanticState::Unavailable | SemanticState::Failed => {
            anyhow::bail!("native semantic publication unavailable: {readiness:?}")
        }
    }
}
fn retain_answer_comparison(
    control: &Path,
    request: &Value,
    budget: &airk_core::arbiter::answer_loop::BudgetV3,
    browser: &Value,
    native: &Value,
    normalized: Option<(&Value, &Value)>,
    reason: &str,
) -> Result<std::path::PathBuf> {
    let raw = match airk_host::service_logs::export_raw(control) {
        Ok(bytes) => json!({"bytes":bytes}),
        Err(error) => json!({"export_error":format!("{error:#}")}),
    };
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("workspace root missing")?
        .join("artifacts/testing/admin-plane");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!(
        "{}.json",
        airk_kernel::service_broker::new_instance_id()?
    ));
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(&serde_json::to_vec_pretty(&json!({"label":"finite Admin Answer original diagnostic",
        "original_request":request,"budget":budget,"browser_i3":browser,"native_i3":native,
        "normalized":normalized.map(|(browser,native)|json!({"browser_i3":browser,"native_i3":native})),
        "raw_service_logs":raw,"comparison_failure":reason,"replayed":false}))?)?;
    Ok(path)
}

#[test]
fn answer_publication_wait_requires_native_semantic_epoch_and_declared_policy() -> Result<()> {
    use airk_core::readiness::*;
    let mut readiness = SemanticReadiness {
        version: 1,
        state: SemanticState::Pending,
        default_search_policy: DefaultSearchPolicy::PartialHybrid,
        selected_artifacts: 1,
        completed_artifacts: 0,
        selected_units: Some(1),
        completed_units: 0,
        first_ready_unix_ms: None,
        fully_ready_unix_ms: None,
        selection_digest: None,
        pipeline_digest: None,
        failure: None,
    };
    let status = |readiness: &SemanticReadiness, enabled: bool| json!({"vectors_enabled":enabled,"semantic_readiness":readiness});
    assert!(!settled_answer_publication(&status(&readiness, true))?);
    readiness.state = SemanticState::Ready;
    readiness.completed_artifacts = 1;
    readiness.completed_units = 1;
    assert!(settled_answer_publication(&status(&readiness, true))?);
    assert!(settled_answer_publication(&status(&readiness, false)).is_err());
    readiness.state = SemanticState::Disabled;
    assert!(settled_answer_publication(&status(&readiness, false))?);
    assert!(settled_answer_publication(&status(&readiness, true)).is_err());
    let mut unknown = status(&readiness, false);
    unknown.as_object_mut().unwrap().remove("vectors_enabled");
    assert!(settled_answer_publication(&unknown).is_err());
    readiness.state = SemanticState::Failed;
    assert!(settled_answer_publication(&status(&readiness, true)).is_err());
    readiness.state = SemanticState::Unavailable;
    assert!(settled_answer_publication(&status(&readiness, true)).is_err());
    Ok(())
}

#[test]
fn native_lightweight_status_is_not_a_complete_semantic_readiness_document() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    std::fs::write(
        source.path().join("fixture.md"),
        "# Native status fixture\nOriginal indexed text.\n",
    )?;
    let kernel = airk_kernel::Kernel::open(source.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let summary = kernel.status_summary()?;
    assert!(summary["semantic_readiness"].get("version").is_none());
    assert!(summary.get("vectors_enabled").is_none());
    assert!(
        serde_json::from_value::<airk_core::readiness::SemanticReadiness>(
            summary["semantic_readiness"].clone()
        )
        .is_err()
    );
    assert!(settled_answer_publication(&summary).is_err());
    let complete = serde_json::to_value(kernel.status()?)?;
    let typed: airk_core::readiness::SemanticReadiness =
        serde_json::from_value(complete["semantic_readiness"].clone())?;
    assert_eq!(typed.version, 1);
    assert_eq!(typed.state, airk_core::readiness::SemanticState::Disabled);
    assert!(settled_answer_publication(&complete)?);
    Ok(())
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
    exchange_before(
        port,
        ca,
        request,
        std::time::Instant::now() + Duration::from_secs(60),
    )
}
fn exchange_before(
    port: u16,
    ca: &CertificateDer<'static>,
    request: String,
    deadline: std::time::Instant,
) -> Result<Reply> {
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
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), exchange)
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
        read_attempt: None,
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
    let selection = service.reviewed_scope();
    let ControlResponse::Health { observation } = service.request(ControlMethod::Health) else {
        anyhow::bail!("health unavailable");
    };
    let clock = observation.catalog_clock.context("catalog clock")?;
    let grant = json!({"admin_scope_grant": {"selection": {
        "namespace": clock.namespace,
        "expected_catalog_revision": clock.revision,
        "index_ids": [selection.index_id],
        "project_ids": [selection.project_id],
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
    // S2: a read under the session's current grant is served through the
    // canonical selection read; the pipe never resolves an admin selection.
    let search = json!({"tools_call": {"tool": "airk_status", "arguments": {},
        "selection": "admin", "response_mode": "terse", "usage_client": "unknown"}})
    .to_string();
    let searched = api("/admin/api/v1/call/search.call", &authenticated, &search)?;
    assert_eq!(
        searched.status,
        200,
        "{}",
        String::from_utf8_lossy(&searched.body)
    );
    assert_eq!(searched.json()["result"]["status"], "tool_result");
    // N3: the grant's nonce never reaches the browser.
    assert!(!String::from_utf8_lossy(&searched.body).contains("grant_nonce"));
    let pipe = service.request(ControlMethod::ToolsCall {
        read_attempt: None,
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

    let fixture_text = "# Owned Admin fixture evidence\nThis indexed source retains the original publication and source locations.\n";
    for (indexed, call_identity) in [(false, 0x55u8), (true, 0x56u8)] {
        if indexed {
            // One real I3 mutation creates the prerequisite publication. The
            // previous unindexed refusal is not retried or treated as ready.
            std::fs::write(source.path().join("fixture.md"), fixture_text)?;
            let deadline = std::time::Instant::now() + Duration::from_secs(30);
            let index_api = |target: &str, body: &str| -> Result<Reply> {
                let mut headers = vec![
                    ("Host", host.as_str()),
                    ("Origin", origin.as_str()),
                    ("Sec-Fetch-Site", "same-origin"),
                    ("Content-Type", "application/json"),
                ];
                headers.extend_from_slice(&authenticated);
                exchange_before(
                    port,
                    &ca,
                    http_request("POST", target, &headers, body),
                    deadline,
                )
            };
            let enqueue =
                json!({"enqueue":{"project":service.project.binding,"kind":"index"}}).to_string();
            let accepted = index_api("/admin/api/v1/call/jobs.enqueue", &enqueue)?;
            ensure!(
                accepted.status == 200,
                "I3 Index request refused without replay: {}",
                String::from_utf8_lossy(&accepted.body)
            );
            let accepted = accepted.json();
            ensure!(
                accepted["result"]["status"] == "job",
                "I3 Index was not accepted: {accepted}"
            );
            let id: airk_core::identity::Sha256Digest =
                serde_json::from_value(accepted["result"]["job"]["id"].clone())?;
            let observe = json!({"job":{"id":id}}).to_string();
            loop {
                ensure!(
                    std::time::Instant::now() < deadline,
                    "I3 original Index observation deadline expired; accepted job {id}"
                );
                let observed = index_api("/admin/api/v1/call/jobs.get", &observe)?;
                ensure!(
                    observed.status == 200,
                    "I3 Index observation refused: {}",
                    String::from_utf8_lossy(&observed.body)
                );
                let observed = observed.json();
                let job: airk_core::service::JobRecord =
                    serde_json::from_value(observed["result"]["job"].clone())?;
                ensure!(
                    job.id == id
                        && job.project == service.project.binding
                        && job.kind == airk_core::service::JobKind::Index,
                    "I3 observation changed accepted Index identity: {observed}"
                );
                if job.state == airk_core::service::JobState::Succeeded {
                    ensure!(
                        matches!(
                            job.outcome,
                            Some(airk_core::service::JobOutcome::Succeeded { revision: Some(_) })
                        ),
                        "I3 successful Index lost actual publication: {observed}"
                    );
                    break;
                }
                ensure!(
                    !matches!(
                        job.state,
                        airk_core::service::JobState::Failed
                            | airk_core::service::JobState::Canceled
                    ),
                    "I3 Index failed: {observed}"
                );
                // Each iteration observes the same accepted job through real
                // I3 I/O under the original deadline; no fixed sleep or replay.
            }
            // Text Index success can precede its scheduled semantic publication.
            // Observe the native policy/readiness epoch before either positive
            // Answer; do not normalize away a real generation transition.
            let status_request = serde_json::to_string(&ControlMethod::ToolsCall {
                tool: "airk_diagnostics".into(),
                arguments: Default::default(),
                selection: ToolSelection::Admin,
                response_mode: ToolResponseMode::Full,
                usage_client: ClientFamily::Unknown,
                read_attempt: None,
            })?;
            let mut busy_repeats = 0usize;
            loop {
                ensure!(
                    std::time::Instant::now() < deadline,
                    "original I3 Index/readiness observation deadline expired"
                );
                let wire_response = index_api("/admin/api/v1/call/search.call", &status_request)?;
                let response = wire_response.json();
                let parsed = (|| -> Result<bool> {
                    ensure!(
                        wire_response.status == 200,
                        "I3 readiness observation refused: {}",
                        String::from_utf8_lossy(&wire_response.body)
                    );
                    let returned = &response["result"]["result"];
                    // The typed `publication_busy` / `repeat_same_request`
                    // tells a client to send the same read again; a
                    // publication settling after Index is exactly that, so
                    // it counts as not yet settled, within the original
                    // deadline and a bounded number of repeats. Every other
                    // failed read still fails here.
                    let retry: Option<Value> = returned
                        .pointer("/content/0/text")
                        .and_then(Value::as_str)
                        .and_then(|text| serde_json::from_str(text).ok());
                    if returned["isError"] == true
                        && retry.as_ref().is_some_and(|retry| {
                            retry["code"] == "publication_busy"
                                && retry["retryable"] == true
                                && retry["action"] == "repeat_same_request"
                        })
                    {
                        busy_repeats += 1;
                        ensure!(
                            busy_repeats <= 50,
                            "readiness stayed publication_busy for 50 repeats: {response}"
                        );
                        return Ok(false);
                    }
                    ensure!(
                        returned["isError"] != true,
                        "native readiness read failed: {response}"
                    );
                    let status = match returned.get("structuredContent") {
                        Some(value) => value.clone(),
                        None => serde_json::from_str(
                            returned
                                .pointer("/content/0/text")
                                .and_then(Value::as_str)
                                .context("native readiness JSON missing")?,
                        )?,
                    };
                    settled_answer_publication(&status)
                })();
                let settled = match parsed {
                    Ok(settled) => settled,
                    Err(error) => {
                        let raw = match airk_host::service_logs::export_raw(&service.control) {
                            Ok(bytes) => json!({"bytes":bytes}),
                            Err(error) => json!({"export_error":format!("{error:#}")}),
                        };
                        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
                            .parent()
                            .and_then(Path::parent)
                            .context("workspace root missing")?
                            .join("artifacts/testing/admin-plane");
                        std::fs::create_dir_all(&directory)?;
                        let path = directory.join(format!(
                            "{}.json",
                            airk_kernel::service_broker::new_instance_id()?
                        ));
                        use std::io::Write;
                        let mut file = std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&path)?;
                        file.write_all(&serde_json::to_vec_pretty(&json!({"label":"original I3 readiness observation failure",
                            "request":serde_json::from_str::<Value>(&status_request)?,"reply":response,
                            "http_status":wire_response.status,"raw_body_bytes":wire_response.body,"raw_service_logs":raw,
                            "error":format!("{error:#}"),"original_index_observation_deadline_ms":30_000,"request_replayed":false}))?)?;
                        return Err(error.context(format!(
                            "original readiness response retained at {}",
                            path.display()
                        )));
                    }
                };
                if settled {
                    ensure!(
                        std::time::Instant::now() < deadline,
                        "semantic readiness was observed after original I3 Index deadline"
                    );
                    break;
                }
            }
        }
        // The same A2 session and grant traverse real TLS and native I1 chunks.
        // Neither finite Answer is retried, including after an unknown outcome.
        let answer = airk_core::arbiter::answer_loop::ContextRequestV3 {
            schema: airk_core::arbiter::answer_loop::REQUEST_SCHEMA_V3.into(),
            question: "owned Admin fixture evidence".into(),
            budget: Default::default(),
            continuation: None,
        };
        answer.validate()?;
        let budget = answer.budget.clone();
        let request = ControlMethod::ToolsCall {
            tool: "airk_answer".into(),
            arguments: serde_json::to_value(answer)?
                .as_object()
                .context("Answer arguments")?
                .clone(),
            selection: ToolSelection::Admin,
            response_mode: ToolResponseMode::Full,
            usage_client: ClientFamily::Unknown,
            read_attempt: None,
        };
        let request = serde_json::to_string(&request)?;
        let original_answer_request: Value = serde_json::from_str(&request)?;
        let browser = api("/admin/api/v1/call/search.call", &authenticated, &request)?;
        ensure!(
            browser.status == 200,
            "finite browser Answer refused without replay: {}",
            String::from_utf8_lossy(&browser.body)
        );
        let native_session = airk_core::native_admin::NativeAdminSessionSecret::new(
            cookie
                .strip_prefix("__Host-airk_admin=")
                .context("owned session cookie")?
                .to_owned(),
            csrf.clone(),
        )?;
        let request_id = airk_core::identity::Incarnation::from_bytes([call_identity; 16]);
        let mut response = service.request(ControlMethod::AdminNativeCall {
            session: native_session.clone(),
            request_id,
            operation: "search.call".into(),
            body: serde_json::value::RawValue::from_string(request)?,
            deadline_ns: airk_process::monotonic::now_ns()?
                .checked_add(30_000_000_000)
                .context("Admin original deadline")?,
        });
        let mut assembly = airk_core::native_admin::NativeAdminAssembly::default();
        let mut native = loop {
            let ControlResponse::AdminNativeChunk { chunk } = response else {
                anyhow::bail!(
                    "finite native Admin Answer refused without replay: {}",
                    serde_json::to_string(&response)?
                )
            };
            let next = chunk.validate()?;
            if let Some(result) = assembly.push(&chunk)? {
                break result;
            }
            response = service.request(ControlMethod::AdminNativeRead {
                session: native_session.clone(),
                request_id,
                offset: next,
            });
        };
        ensure!(assembly.complete(), "native Admin Answer stream incomplete");
        let mut browser = browser.json();
        let browser_result = browser
            .pointer("/result/result")
            .context("browser canonical ToolResult")?;
        let native_result = native
            .pointer("/result/result")
            .context("native canonical ToolResult")?;
        if !indexed {
            assert_eq!(
                browser, native,
                "unindexed I3 refusal must retain both complete original envelopes"
            );
            assert_eq!(browser_result["isError"], true);
            let refused = &browser_result["structuredContent"];
            assert_eq!(refused["error"]["code"], "read_failed");
            assert_eq!(refused["error"]["retryable"], false);
            assert!(
                refused["error"]["message"]
                    .as_str()
                    .is_some_and(|s| s.contains("workspace is not indexed"))
            );
            assert!(
                refused.get("assessment_history").is_none() && refused.get("work").is_none(),
                "unindexed refusal must not fabricate NN/work receipts"
            );
            assert!(
                std::fs::read_dir(source.path())?.next().is_none(),
                "unindexed read mutated the original source"
            );
            continue;
        }
        let original_browser_i3 = browser.clone();
        let original_native_i3 = native.clone();
        let (browser_result, native_result) =
            match common::parity::context::envelopes(browser_result, native_result, &budget) {
                Ok(pair) => pair,
                Err(error) => {
                    let path = retain_answer_comparison(
                        &service.control,
                        &original_answer_request,
                        &budget,
                        &original_browser_i3,
                        &original_native_i3,
                        None,
                        &format!("{error:#}"),
                    )?;
                    return Err(error.context(format!(
                        "original Admin responses and private ring retained at {}",
                        path.display()
                    )));
                }
            };
        browser["result"]["result"] = browser_result;
        native["result"]["result"] = native_result;
        if browser != native {
            let path = retain_answer_comparison(
                &service.control,
                &original_answer_request,
                &budget,
                &original_browser_i3,
                &original_native_i3,
                Some((&browser, &native)),
                "complete I3 envelope/authority/canonical Answer result differs",
            )?;
            anyhow::bail!(
                "complete I3 envelope/authority/canonical Answer result differs; raw and normalized responses retained at {}",
                path.display()
            );
        }
        assert_eq!(
            std::fs::read_to_string(source.path().join("fixture.md"))?,
            fixture_text,
            "canonical reads changed indexed fixture source"
        );
    }

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
