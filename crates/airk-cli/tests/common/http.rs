// SPDX-License-Identifier: Apache-2.0
//! Actual HTTPS fixture client. Owns a thread/runtime, never an AIRK frontend process.
#![allow(dead_code)]
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse},
    service_http::{HttpConnectionInfo, HttpSelection},
    usage::ClientFamily,
};
use anyhow::{Context, Result, ensure};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper_util::rt::TokioIo;
use ring::rand::{SecureRandom, SystemRandom};
use rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(90);
type Reply = mpsc::Sender<Result<Vec<Value>>>;
enum Command {
    Send(Value, Instant, Reply),
    Probe(Vec<u8>, Reply),
    FreshTcp(Reply),
    Close(Reply),
}

#[derive(Debug)]
pub struct HttpRefused {
    pub status: u16,
    pub body: Value,
}
impl std::fmt::Display for HttpRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTPS fixture refused ({}): {}", self.status, self.body)
    }
}
impl std::error::Error for HttpRefused {}

pub struct HttpMcpSession {
    commands: Option<mpsc::Sender<Command>>,
    thread: Option<thread::JoinHandle<()>>,
    pending: VecDeque<Value>,
    pub connection: HttpConnectionInfo,
    client: ClientFamily,
    mode: Option<String>,
}
impl HttpMcpSession {
    /// Enrollment/TLS only. The caller explicitly sends initialize/initialized.
    pub fn start(control: &Path, selection: HttpSelection) -> Result<Self> {
        Self::start_for_client(control, selection, ClientFamily::Unknown)
    }
    pub fn start_for_client(
        control: &Path,
        selection: HttpSelection,
        client: ClientFamily,
    ) -> Result<Self> {
        Self::start_with_mode(control, selection, client, Some("full"))
    }
    pub fn start_with_mode(
        control: &Path,
        selection: HttpSelection,
        client: ClientFamily,
        mode: Option<&str>,
    ) -> Result<Self> {
        Self::start_inner(control, selection, client, mode, None)
    }
    /// Explicit fresh TLS/MCP transport after an observed catalog boundary.
    /// Reuse the existing protected credential; never re-enroll or replay a read.
    pub fn reconnect(&self, control: &Path) -> Result<Self> {
        let fresh = Self::start_inner(
            control,
            self.connection.selection.clone(),
            self.client,
            self.mode.as_deref(),
            Some(self.connection.clone()),
        )?;
        ensure!(
            fresh.connection == self.connection,
            "explicit reconnect changed credential references or frozen authority"
        );
        Ok(fresh)
    }
    fn start_inner(
        control: &Path,
        selection: HttpSelection,
        client: ClientFamily,
        mode: Option<&str>,
        existing: Option<HttpConnectionInfo>,
    ) -> Result<Self> {
        ensure!(
            mode.is_none_or(|mode| matches!(mode, "full" | "terse")),
            "invalid fixture display mode"
        );
        let mode = mode.map(str::to_owned);
        let saved_mode = mode.clone();
        let control = control.to_owned();
        let (commands, incoming) = mpsc::channel();
        let (ready, initialized) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("airk-https-fixture".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready.send(Err(anyhow::Error::from(error)));
                        return;
                    }
                };
                let opened = runtime.block_on(async {
                    tokio::time::timeout(
                        TIMEOUT,
                        Wire::open(&control, selection, client, mode, existing),
                    )
                    .await?
                });
                let mut wire = match opened {
                    Ok(wire) => wire,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                if ready.send(Ok(wire.info.clone())).is_err() {
                    return;
                }
                while let Ok(command) = incoming.recv() {
                    match command {
                        Command::Send(value, deadline, reply) => {
                            let result = runtime.block_on(async {
                                tokio::time::timeout_at(
                                    tokio::time::Instant::from_std(deadline),
                                    wire.send(value),
                                )
                                .await
                            });
                            let timed_out = result.is_err();
                            let result =
                                result.map_err(anyhow::Error::from).and_then(|value| value);
                            let _ = reply.send(result);
                            // A timed-out exchange has no reusable framing or delivery
                            // ownership. Closing the owned runtime also drops its TLS driver.
                            if timed_out {
                                break;
                            }
                        }
                        Command::Probe(bytes, reply) => {
                            let result = runtime.block_on(async {
                                tokio::time::timeout(
                                    TIMEOUT,
                                    wire.exchange_bytes(
                                        http::Method::POST,
                                        bytes,
                                        Some(wire.protocol.clone()),
                                    ),
                                )
                                .await?
                            });
                            let _ = reply.send(result);
                        }
                        Command::FreshTcp(reply) => {
                            let result = runtime.block_on(async {
                                tokio::time::timeout(TIMEOUT, wire.fresh_tcp()).await?
                            });
                            let _ = reply.send(result.map(|()| Vec::new()));
                        }
                        Command::Close(reply) => {
                            let result = runtime.block_on(async {
                                tokio::time::timeout(Duration::from_secs(10), wire.close()).await?
                            });
                            let _ = reply.send(result);
                            break;
                        }
                    }
                }
            })?;
        match initialized.recv_timeout(TIMEOUT + Duration::from_secs(5)) {
            Ok(Ok(connection)) => Ok(Self {
                commands: Some(commands),
                thread: Some(worker),
                pending: VecDeque::new(),
                connection,
                client,
                mode: saved_mode,
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(error) => Err(error.into()),
        }
    }
    pub fn send(&mut self, value: Value) -> Result<()> {
        self.send_until(value, Instant::now() + TIMEOUT)
    }
    /// Explicitly replace an idle TCP connection before a new independent read.
    /// Preserve the MCP session and credential; never replay a failed exchange.
    pub fn fresh_tcp(&mut self) -> Result<()> {
        let (sender, receiver) = mpsc::channel();
        self.commands
            .as_ref()
            .context("HTTPS fixture closed")?
            .send(Command::FreshTcp(sender))?;
        receiver.recv_timeout(TIMEOUT + Duration::from_secs(5))??;
        Ok(())
    }
    pub fn send_until(&mut self, value: Value, deadline: Instant) -> Result<()> {
        let replies = submit_until(
            self.commands.as_ref().context("HTTPS fixture closed")?,
            value,
            deadline,
        )?;
        ensure!(
            self.pending.len() + replies.len() <= 128,
            "HTTPS fixture pending message bound exceeded"
        );
        self.pending.extend(replies);
        Ok(())
    }
    pub fn receive_until(&mut self, id: i64, deadline: Instant) -> Result<Value> {
        ensure!(Instant::now() < deadline, "HTTPS reply deadline exhausted");
        self.receive(id)
    }
    pub fn receive(&mut self, id: i64) -> Result<Value> {
        let position = self
            .pending
            .iter()
            .position(|reply| reply.get("id").and_then(Value::as_i64) == Some(id))
            .context("matching HTTPS JSON-RPC reply missing")?;
        self.pending
            .remove(position)
            .context("matching HTTPS reply disappeared")
    }
    /// Deliberate malformed/exact-bound transport probes. Normal calls remain bounded.
    pub fn probe(&mut self, bytes: Vec<u8>) -> Result<Option<Value>> {
        ensure!(
            bytes.len() <= airk_core::service::MAX_FRAME_BYTES + 1,
            "probe exceeds deliberate one-byte overflow"
        );
        let (sender, receiver) = mpsc::channel();
        self.commands
            .as_ref()
            .context("HTTPS fixture closed")?
            .send(Command::Probe(bytes, sender))?;
        let mut replies = receiver.recv_timeout(TIMEOUT + Duration::from_secs(5))??;
        ensure!(
            replies.len() <= 1,
            "single-response HTTPS probe returned multiple messages"
        );
        Ok(replies.pop())
    }
    pub fn receive_value(&mut self) -> Result<Value> {
        self.pending.pop_front().context("HTTPS reply missing")
    }
    pub fn request(&mut self, id: i64, method: &str, params: Value) -> Result<Value> {
        self.request_until(id, method, params, Instant::now() + TIMEOUT)
    }
    pub fn request_until(
        &mut self,
        id: i64,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value> {
        self.send_until(
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
            deadline,
        )?;
        self.receive_until(id, deadline)
    }
    pub fn close(&mut self) -> Result<()> {
        let Some(commands) = self.commands.take() else {
            return Ok(());
        };
        let (sender, receiver) = mpsc::channel();
        let result = if commands.send(Command::Close(sender)).is_ok() {
            receiver
                .recv_timeout(Duration::from_secs(15))
                .map_err(anyhow::Error::from)
                .and_then(|value| value.map(|_| ()))
        } else {
            Ok(())
        };
        drop(commands);
        if let Some(worker) = self.thread.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("HTTPS fixture thread panicked"))?;
        }
        result
    }
}
impl Drop for HttpMcpSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

struct Wire {
    sender: hyper::client::conn::http1::SendRequest<Full<Bytes>>,
    task: tokio::task::JoinHandle<Result<(), hyper::Error>>,
    tls_config: Arc<rustls::ClientConfig>,
    info: HttpConnectionInfo,
    token: String,
    session: Option<String>,
    initialized: bool,
    protocol: String,
    mode: Option<String>,
    evidence: PathBuf,
    sequence: u64,
}
impl Wire {
    async fn open(
        control: &Path,
        selection: HttpSelection,
        client: ClientFamily,
        mode: Option<String>,
        existing: Option<HttpConnectionInfo>,
    ) -> Result<Self> {
        selection.validate()?;
        let ControlResponse::Health {
            observation: health,
        } = airk_cli::service_runtime::request(control, ControlMethod::Health).await?
        else {
            anyhow::bail!("native fixture health unavailable")
        };
        let info = if let Some(info) = existing {
            let response = airk_cli::service_runtime::request_for_instance(
                control,
                ControlMethod::HttpEndpoint,
                health.instance,
            )
            .await?;
            let ControlResponse::HttpEndpoint {
                endpoint: Some(endpoint),
            } = response
            else {
                anyhow::bail!("existing HTTPS endpoint unavailable during explicit reconnect");
            };
            ensure!(
                *endpoint == info.endpoint,
                "endpoint changed; this reconnect is limited to a catalog boundary"
            );
            Box::new(info)
        } else {
            let mut random = [0u8; 16];
            SystemRandom::new()
                .fill(&mut random)
                .map_err(|_| anyhow::anyhow!("fixture connection identity unavailable"))?;
            let id = Incarnation::from_bytes(random);
            let response = airk_cli::service_runtime::request_for_instance(
                control,
                ControlMethod::HttpEnroll {
                    connection_id: id,
                    selection: Box::new(selection.clone()),
                    client,
                    explorer_descriptor: None,
                    reattach_connection_id: None,
                },
                health.instance,
            )
            .await?;
            let ControlResponse::HttpConnection { connection: info } = response else {
                anyhow::bail!(
                    "native HTTPS enrollment refused: {}",
                    serde_json::to_string(&response)?
                )
            };
            ensure!(
                info.connection_id == id,
                "enrollment changed connection identity"
            );
            info
        };
        let id = info.connection_id;
        info.validate()?;
        ensure!(
            info.endpoint.namespace == selection.namespace()
                && info.endpoint.service_instance == health.instance
                && info.endpoint.user == health.user
                && info.endpoint.executable_sha256.to_string() == health.executable_sha256,
            "HTTPS fixture native identity changed"
        );
        same_authority(&selection, &info.selection)?;
        let root = control.join("http-1");
        ensure!(
            Path::new(&info.ca_certificate_file) == root.join("ca.pem")
                && Path::new(&info.credential_file) == root.join(format!("credential-{id}.token")),
            "HTTPS fixture private references changed"
        );
        ensure!(root.is_dir(), "HTTPS fixture private root missing");
        airk_cli::local_ipc::prepare_root(&root)?;
        let path = root.join("ca.pem");
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 8192,
            "invalid fixture issuer file"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "redirected fixture issuer"
            );
        }
        use std::io::Read;
        let mut pem = Vec::new();
        std::fs::File::open(path)?
            .take(8193)
            .read_to_end(&mut pem)?;
        ensure!(pem.len() <= 8192, "fixture issuer grew beyond bound");
        let certificates =
            CertificateDer::pem_slice_iter(&pem).collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(
            certificates.len() == 1
                && Sha256::digest(certificates[0].as_ref()).as_slice()
                    == info.endpoint.ca_sha256.as_bytes(),
            "fixture issuer differs from native control"
        );
        // This isolated native fixture pins its reviewed local issuer. It never
        // modifies/replaces a user's OS or application trust configuration.
        let mut roots = rustls::RootCertStore::empty();
        roots.add(certificates[0].clone().into_owned())?;
        let config = Arc::new(
            rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots)
            .with_no_client_auth(),
        );
        let (sender, task) = Self::connect(&info, config.clone()).await?;
        // Bearer bytes are read only after TLS/IP/issuer/leaf verification.
        let token = airk_cli::http_endpoint::read_client_token(&root, id)?;
        Ok(Self {
            sender,
            task,
            tls_config: config,
            info: *info,
            token,
            session: None,
            initialized: false,
            protocol: "2025-11-25".into(),
            mode,
            evidence: control.to_path_buf(),
            sequence: 0,
        })
    }
    async fn connect(
        info: &HttpConnectionInfo,
        config: Arc<rustls::ClientConfig>,
    ) -> Result<(
        hyper::client::conn::http1::SendRequest<Full<Bytes>>,
        tokio::task::JoinHandle<Result<(), hyper::Error>>,
    )> {
        let socket = tokio::net::TcpStream::connect((
            std::net::Ipv4Addr::LOCALHOST,
            info.endpoint.url.port(),
        ))
        .await?;
        let tls = tokio_rustls::TlsConnector::from(config)
            .connect(ServerName::try_from("127.0.0.1")?, socket)
            .await?;
        let peer = tls
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certificates| certificates.first())
            .context("fixture TLS peer certificate missing")?;
        ensure!(
            Sha256::digest(peer.as_ref()).as_slice() == info.endpoint.certificate_sha256.as_bytes(),
            "fixture TLS peer differs from native control"
        );
        let (sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(tls)).await?;
        let task = tokio::spawn(connection);
        Ok((sender, task))
    }
    async fn fresh_tcp(&mut self) -> Result<()> {
        let (sender, task) = Self::connect(&self.info, self.tls_config.clone()).await?;
        self.task.abort();
        self.sender = sender;
        self.task = task;
        Ok(())
    }
    async fn exchange(
        &mut self,
        method: http::Method,
        value: Option<&Value>,
    ) -> Result<Vec<Value>> {
        let bytes = value
            .map(serde_json::to_vec)
            .transpose()?
            .unwrap_or_default();
        ensure!(
            bytes.len() <= 65_536,
            "HTTPS fixture request exceeds native frame bound"
        );
        let protocol = request_protocol(value, self.initialized, &self.protocol)?;
        self.exchange_bytes(method, bytes, protocol).await
    }
    async fn exchange_bytes(
        &mut self,
        method: http::Method,
        bytes: Vec<u8>,
        protocol: Option<String>,
    ) -> Result<Vec<Value>> {
        let method_name = method.to_string();
        let authority = format!("127.0.0.1:{}", self.info.endpoint.url.port());
        let mut request = http::Request::builder()
            .method(method)
            .uri("/mcp")
            .header(http::header::HOST, authority)
            .header(http::header::CONTENT_TYPE, "application/json")
            .header(http::header::ACCEPT, "application/json, text/event-stream")
            .header(
                http::header::AUTHORIZATION,
                format!("Bearer {}", self.token),
            );
        for (name, value) in standard_headers(&bytes, protocol.as_deref())? {
            request = request.header(name, value);
        }
        if let Some(protocol) = protocol {
            request = request.header("mcp-protocol-version", protocol);
        }
        if let Some(mode) = &self.mode {
            request = request.header("x-airk-response-mode", mode);
        }
        if let Some(session) = &self.session {
            request = request.header("mcp-session-id", session);
        }
        let response = self
            .sender
            .send_request(request.body(Full::new(Bytes::from(bytes)))?)
            .await?;
        let status = response.status();
        let content_type = response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .map(|value| value.to_str().map(str::to_owned))
            .transpose()
            .context("HTTPS fixture returned invalid Content-Type")?;
        if let Some(session) = response.headers().get("mcp-session-id") {
            let session = session.to_str()?;
            ensure!(
                !session.is_empty()
                    && session.len() <= 256
                    && session.bytes().all(|byte| byte.is_ascii_graphic())
                    && self.session.as_ref().is_none_or(|old| old == session),
                "HTTPS fixture session identity changed"
            );
            self.session = Some(session.into());
        }
        let bytes = Limited::new(response.into_body(), 1_048_576)
            .collect()
            .await
            .map_err(anyhow::Error::from_boxed)?
            .to_bytes();
        self.sequence += 1;
        decode_http_body(status, content_type.as_deref(), &bytes)
            .map_err(|error| {
                let context=format!("HTTPS fixture {method_name} /mcp status={} content_type={:?} body_bytes={} body_sha256={:x} prefix={:?}",
                    status.as_u16(),content_type,bytes.len(),Sha256::digest(&bytes),
                    String::from_utf8_lossy(&bytes[..bytes.len().min(96)]));
                if error.downcast_ref::<HttpRefused>().is_none()
                    && let Err(saved)=retain_http_failure(&self.evidence,self.info.connection_id,self.sequence,&context,&bytes) {
                        return error.context(format!("{context}; failure evidence unavailable: {saved:#}"));
                }
                error.context(context)
            })
    }
    async fn send(&mut self, value: Value) -> Result<Vec<Value>> {
        let response = self
            .exchange(http::Method::POST, Some(&value))
            .await
            .with_context(|| {
                format!(
                    "HTTPS JSON-RPC method={:?} id={:?}",
                    value.get("method"),
                    value.get("id")
                )
            })?;
        if value.get("method").and_then(Value::as_str) == Some("initialize")
            && let Some(protocol) = response
                .iter()
                .find(|reply| reply.get("id") == value.get("id"))
                .and_then(|reply| reply.pointer("/result/protocolVersion"))
                .and_then(Value::as_str)
        {
            ensure!(
                valid_protocol(protocol),
                "invalid negotiated fixture protocol"
            );
            self.protocol = protocol.into();
            self.initialized = true;
        }
        Ok(response)
    }
    async fn close(&mut self) -> Result<Vec<Value>> {
        let result = if self.session.is_some() {
            self.exchange(http::Method::DELETE, None).await
        } else {
            Ok(Vec::new())
        };
        self.task.abort();
        self.token.clear();
        result
    }
}
fn submit_until(
    commands: &mpsc::Sender<Command>,
    value: Value,
    deadline: Instant,
) -> Result<Vec<Value>> {
    ensure!(
        Instant::now() < deadline,
        "HTTPS request deadline exhausted before submission"
    );
    let (sender, receiver) = mpsc::channel();
    let method = value.get("method").cloned().unwrap_or(Value::Null);
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    commands.send(Command::Send(value, deadline, sender))?;
    let replies = receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .with_context(|| format!("HTTPS fixture delivery deadline: method={method}, id={id}"))?
        .with_context(|| format!("HTTPS fixture request failed: method={method}, id={id}"))?;
    ensure!(
        Instant::now() < deadline,
        "HTTPS request deadline exhausted before delivery"
    );
    Ok(replies)
}

// SDK SEP-2243 requires exact routing headers for 2026 requests. Malformed
// probes stay malformed; no body rewriting or inferred operation is permitted.
fn standard_headers(bytes: &[u8], protocol: Option<&str>) -> Result<Vec<(&'static str, String)>> {
    if protocol.is_none_or(|value| value < "2026-07-28") {
        return Ok(Vec::new());
    }
    let Ok(body) = serde_json::from_slice::<Value>(bytes) else {
        return Ok(Vec::new());
    };
    let Some(method) = body.get("method").and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    let mut headers = vec![("mcp-method", method.to_owned())];
    let key = match method {
        "tools/call" | "prompts/get" => Some("name"),
        "resources/read" | "resources/subscribe" | "resources/unsubscribe" => Some("uri"),
        "tasks/get" | "tasks/update" | "tasks/cancel" => Some("taskId"),
        _ => None,
    };
    if let Some(name) = key.and_then(|key| body.get("params")?.get(key)?.as_str()) {
        // Fixture identifiers are ASCII. Refuse unsupported header encoding;
        // never send a header differing from the exact JSON operation.
        ensure!(
            name.bytes().all(|byte| (0x21..=0x7e).contains(&byte)),
            "fixture Mcp-Name requires encoded header support"
        );
        headers.push(("mcp-name", name.to_owned()));
    }
    Ok(headers)
}

fn valid_protocol(protocol: &str) -> bool {
    !protocol.is_empty()
        && protocol.len() <= 64
        && protocol.bytes().all(|byte| byte.is_ascii_graphic())
}

fn request_protocol(
    value: Option<&Value>,
    initialized: bool,
    negotiated: &str,
) -> Result<Option<String>> {
    // The SDK negotiates from the untouched initial body. An initial HTTP
    // header is optional, but, when present, must match that requested version.
    if !initialized
        && value
            .and_then(|value| value.get("method"))
            .and_then(Value::as_str)
            == Some("initialize")
    {
        return Ok(None);
    }
    let explicit = value
        .and_then(|value| value.pointer("/params/_meta/io.modelcontextprotocol~1protocolVersion"));
    let protocol = match explicit {
        Some(value) => value
            .as_str()
            .context("invalid explicit fixture protocol")?,
        None => negotiated,
    };
    ensure!(valid_protocol(protocol), "invalid fixture protocol header");
    Ok(Some(protocol.to_owned()))
}

fn decode_http_body(
    status: http::StatusCode,
    content_type: Option<&str>,
    bytes: &[u8],
) -> Result<Vec<Value>> {
    ensure!(
        bytes.len() <= 1_048_576,
        "HTTPS fixture response exceeds byte bound"
    );
    if !status.is_success() {
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned()))
        };
        return Err(HttpRefused {
            status: status.as_u16(),
            body,
        }
        .into());
    }
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mime = content_type
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    if mime.eq_ignore_ascii_case("application/json") {
        return Ok(vec![
            serde_json::from_slice(bytes).context("invalid HTTPS JSON response")?,
        ]);
    }
    ensure!(
        mime.eq_ignore_ascii_case("text/event-stream"),
        "unsupported HTTPS fixture response Content-Type"
    );
    let text = std::str::from_utf8(bytes).context("invalid HTTPS SSE UTF-8")?;
    let normalized = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut data = Vec::new();
    let mut messages = Vec::new();
    for line in normalized.split('\n') {
        if line.is_empty() {
            if !data.is_empty() {
                ensure!(messages.len() < 128, "HTTPS SSE message bound exceeded");
                messages.push(serde_json::from_str(&data.join("\n")).with_context(|| {
                    format!("invalid HTTPS SSE JSON message {}", messages.len() + 1)
                })?);
                data.clear();
            }
        } else if !line.starts_with(':') {
            let (field, value) = line.split_once(':').unwrap_or((line, ""));
            if field == "data" {
                data.push(value.strip_prefix(' ').unwrap_or(value));
            }
        }
    }
    ensure!(data.is_empty(), "HTTPS SSE response ended within a message");
    Ok(messages)
}

fn retain_http_failure(
    control: &Path,
    connection: Incarnation,
    sequence: u64,
    context: &str,
    body: &[u8],
) -> Result<()> {
    let evidence = control.join("http-fixture-observations");
    airk_cli::local_ipc::prepare_root(&evidence)?;
    let path = evidence.join(format!("{connection}-{sequence}.body"));
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?
        .write_all(body)?;
    let marker = control.join("http-fixture-failure.json");
    if !marker.try_exists()? {
        let bytes = serde_json::to_vec(&json!({"context":context,"body_file":path}))?;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(marker)?
            .write_all(&bytes)?;
    }
    eprintln!(
        "HTTPS fixture response evidence retained at {}",
        path.display()
    );
    Ok(())
}

fn same_authority(expected: &HttpSelection, observed: &HttpSelection) -> Result<()> {
    fn normalize(mut value: Value) -> Value {
        if let Some(selection) = value.get_mut("selection").and_then(Value::as_object_mut) {
            selection.remove("expected_catalog_revision");
            if let Some(members) = selection
                .get_mut("selections")
                .and_then(Value::as_array_mut)
            {
                for member in members {
                    if let Some(member) = member.as_object_mut() {
                        member.remove("expected_catalog_revision");
                    }
                }
            }
        }
        value
    }
    ensure!(
        normalize(serde_json::to_value(expected)?) == normalize(serde_json::to_value(observed)?),
        "HTTPS fixture enrollment changed immutable authority"
    );
    Ok(())
}

#[cfg(test)]
mod response_tests {
    use super::*;
    #[test]
    fn request_budget_is_absolute_before_submission_and_during_delivery() -> Result<()> {
        let (send, receive) = mpsc::channel();
        assert!(submit_until(&send, json!({"id":1}), Instant::now()).is_err());
        assert!(
            receive.try_recv().is_err(),
            "expired calls must not be submitted"
        );
        let deadline = Instant::now() + Duration::from_millis(50);
        let exact = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search","arguments":{"query":"unaltered"}}});
        let expected = exact.clone();
        let worker = thread::spawn(move || {
            let Command::Send(actual, observed, reply) = receive.recv().unwrap() else {
                panic!("unexpected command")
            };
            assert_eq!(actual, expected);
            assert_eq!(observed, deadline);
            thread::sleep(Duration::from_millis(100));
            assert!(
                reply.send(Ok(vec![json!({"id":2})])).is_err(),
                "late delivery must have no receiver"
            );
        });
        let start = Instant::now();
        assert!(submit_until(&send, exact, deadline).is_err());
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "must not reset to fixture's 90 second budget"
        );
        worker.join().unwrap();
        Ok(())
    }
    #[test]
    fn standard_routing_headers_match_exact_2026_body() -> Result<()> {
        let body = serde_json::to_vec(
            &json!({"method":"tools/call","params":{"name":"airk_search","arguments":{"query":"unchanged"}}}),
        )?;
        assert!(standard_headers(&body, Some("2025-11-25"))?.is_empty());
        assert_eq!(
            standard_headers(&body, Some("2026-07-28"))?,
            vec![
                ("mcp-method", "tools/call".into()),
                ("mcp-name", "airk_search".into())
            ]
        );
        assert_eq!(
            standard_headers(br#"{"method":"server/discover"}"#, Some("2026-07-28"))?,
            vec![("mcp-method", "server/discover".into())]
        );
        assert!(standard_headers(b"malformed", Some("2026-07-28"))?.is_empty());
        Ok(())
    }
    #[test]
    fn framing_preserves_initial_body_and_explicit_metadata_without_changing_negotiation()
    -> Result<()> {
        for protocol in ["2024-11-05", "2025-11-25", "2026-07-28"] {
            let value = json!({"method":"initialize","params":{"protocolVersion":protocol}});
            let original = value.clone();
            assert_eq!(request_protocol(Some(&value), false, "2025-11-25")?, None);
            assert_eq!(value, original);
        }
        let inline = json!({"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}});
        assert_eq!(
            request_protocol(Some(&inline), false, "2025-11-25")?,
            Some("2026-07-28".into())
        );
        assert_eq!(
            request_protocol(Some(&json!({"method":"ping"})), true, "2024-11-05")?,
            Some("2024-11-05".into())
        );
        assert_eq!(
            request_protocol(None, true, "2024-11-05")?,
            Some("2024-11-05".into())
        );
        for invalid in [
            Value::Null,
            json!(2026),
            json!(""),
            json!("bad\r\nheader"),
            json!("x".repeat(65)),
        ] {
            let value = json!({"method":"ping","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":invalid}}});
            assert!(request_protocol(Some(&value), true, "2025-11-25").is_err());
        }
        Ok(())
    }
    #[test]
    fn actual_stateful_initialize_sse_and_plain_json_retain_the_same_rpc() -> Result<()> {
        let reply = json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","serverInfo":{"name":"AIRK"}}});
        let wire = format!("event: message\nid: 0\ndata: {}\n\n", reply);
        assert_eq!(
            decode_http_body(
                http::StatusCode::OK,
                Some("text/event-stream; charset=utf-8"),
                wire.as_bytes()
            )?,
            vec![reply.clone()]
        );
        assert_eq!(
            decode_http_body(
                http::StatusCode::OK,
                Some("application/json"),
                &serde_json::to_vec(&reply)?
            )?,
            vec![reply]
        );
        Ok(())
    }
    #[test]
    fn sse_preserves_every_notification_and_multiline_crlf_reply() -> Result<()> {
        let wire=b": keepalive\r\nretry: 1000\r\n\r\nevent: message\r\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\"}\r\n\r\ndata: {\"jsonrpc\":\"2.0\",\r\ndata: \"id\":2,\"result\":{\"complete\":true}}\r\n\r\n";
        assert_eq!(
            decode_http_body(http::StatusCode::OK, Some("text/event-stream"), wire)?,
            vec![
                json!({"jsonrpc":"2.0","method":"notifications/message"}),
                json!({"jsonrpc":"2.0","id":2,"result":{"complete":true}})
            ]
        );
        Ok(())
    }
    #[test]
    fn malformed_truncated_oversize_or_excessive_sse_never_fabricates_a_reply() {
        for (mime, wire) in [
            ("text/event-stream", b"data: {bad}\n\n".as_slice()),
            ("text/event-stream", br#"data: {"id":1}"#.as_slice()),
            ("text/event-stream", b"data: \xff\n\n".as_slice()),
            ("text/plain", br#"{"id":1}"#.as_slice()),
        ] {
            assert!(decode_http_body(http::StatusCode::OK, Some(mime), wire).is_err());
        }
        assert!(
            decode_http_body(
                http::StatusCode::OK,
                Some("application/json"),
                &vec![b' '; 1_048_577]
            )
            .is_err()
        );
        assert!(
            decode_http_body(
                http::StatusCode::OK,
                Some("text/event-stream"),
                "data: {}\n\n".repeat(129).as_bytes()
            )
            .is_err()
        );
    }
    #[test]
    fn empty_accepted_and_delete_responses_and_http_refusals_keep_their_contract() -> Result<()> {
        for status in [http::StatusCode::ACCEPTED, http::StatusCode::NO_CONTENT] {
            assert!(decode_http_body(status, None, b"")?.is_empty());
        }
        let original = json!({"error":{"code":"scope_revoked","detail":{"retained":true}}});
        let error = decode_http_body(
            http::StatusCode::FORBIDDEN,
            Some("application/json"),
            &serde_json::to_vec(&original)?,
        )
        .unwrap_err();
        let refusal = error.downcast_ref::<HttpRefused>().unwrap();
        assert_eq!(refusal.status, 403);
        assert_eq!(refusal.body, original);
        Ok(())
    }
}
