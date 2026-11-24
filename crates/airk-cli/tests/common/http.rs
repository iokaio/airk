// SPDX-License-Identifier: Apache-2.0
//! Actual loopback HTTP fixture client (local trust plan C: plain TCP). Owns a
//! thread/runtime, never an AIRK frontend process.
#![allow(dead_code)]
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse},
    service_http::EndpointDescriptor,
};
use anyhow::{Context, Result, ensure};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper_util::rt::TokioIo;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::mpsc,
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
        write!(f, "HTTP fixture refused ({}): {}", self.status, self.body)
    }
}
impl std::error::Error for HttpRefused {}

/// U110: the per-call scope this fixture names on every read. The HTTP MCP
/// endpoint takes no credential; every session is the service's local agent
/// session, and a read names `index_ids`/`index_set_ids` (ids or names) or a
/// `workspace`. `Scope::none()` names nothing, so a read is refused
/// `scope_required` unless the call itself names a scope.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    pub index_ids: Vec<String>,
    pub index_set_ids: Vec<String>,
}
impl Scope {
    pub fn none() -> Self {
        Self::default()
    }
    pub fn indexes<T: ToString>(ids: impl IntoIterator<Item = T>) -> Self {
        Self {
            index_ids: ids.into_iter().map(|id| id.to_string()).collect(),
            index_set_ids: Vec::new(),
        }
    }
    pub fn sets<T: ToString>(ids: impl IntoIterator<Item = T>) -> Self {
        Self {
            index_ids: Vec::new(),
            index_set_ids: ids.into_iter().map(|id| id.to_string()).collect(),
        }
    }
    /// The M3 open selection reading this same scope through the pipe
    /// (`airkctl tools call --index/--set` takes the same service path).
    pub fn open_selection(&self) -> airk_core::service::ToolSelection {
        airk_core::service::ToolSelection::Open {
            index_ids: self.index_ids.clone(),
            index_set_ids: self.index_set_ids.clone(),
            workspace: None,
        }
    }
    /// The `airkctl tools call` selector arguments naming this scope.
    pub fn cli_arguments(&self) -> Vec<String> {
        self.index_ids
            .iter()
            .flat_map(|id| ["--index".to_owned(), id.clone()])
            .chain(
                self.index_set_ids
                    .iter()
                    .flat_map(|id| ["--set".to_owned(), id.clone()]),
            )
            .collect()
    }
    /// Name this scope in a `tools/call` request unless the call already
    /// names `index_ids`, `index_set_ids` or `workspace`. `list_scopes`
    /// takes no selection and is left unchanged, as is every other method.
    fn inject(&self, value: &mut Value) {
        if self.index_ids.is_empty() && self.index_set_ids.is_empty() {
            return;
        }
        if value.get("method").and_then(Value::as_str) != Some("tools/call") {
            return;
        }
        let Some(params) = value.get_mut("params").and_then(Value::as_object_mut) else {
            return;
        };
        if params.get("name").and_then(Value::as_str) == Some("list_scopes") {
            return;
        }
        let Some(arguments) = params
            .entry("arguments")
            .or_insert_with(|| Value::Object(Default::default()))
            .as_object_mut()
        else {
            return;
        };
        if ["index_ids", "index_set_ids", "workspace"]
            .iter()
            .any(|key| arguments.contains_key(*key))
        {
            return;
        }
        if !self.index_ids.is_empty() {
            arguments.insert("index_ids".into(), json!(self.index_ids));
        }
        if !self.index_set_ids.is_empty() {
            arguments.insert("index_set_ids".into(), json!(self.index_set_ids));
        }
    }
}

pub struct HttpMcpSession {
    commands: Option<mpsc::Sender<Command>>,
    thread: Option<thread::JoinHandle<()>>,
    pending: VecDeque<Value>,
    /// The service endpoint this transport verified (no connection, no token).
    pub endpoint: EndpointDescriptor,
    /// The scope injected into every read this session sends.
    pub scope: Scope,
    mode: Option<String>,
}
impl HttpMcpSession {
    /// Plain HTTP only: no enrollment and no credential. The caller explicitly sends
    /// initialize/initialized; its `clientInfo` names the client family.
    pub fn start(control: &Path, scope: Scope) -> Result<Self> {
        Self::start_with_mode(control, scope, Some("full"))
    }
    pub fn start_with_mode(control: &Path, scope: Scope, mode: Option<&str>) -> Result<Self> {
        Self::start_inner(control, scope, mode, None)
    }
    /// Explicit fresh HTTP/MCP transport after an observed catalog boundary on
    /// the same endpoint. Never replays a read.
    pub fn reconnect(&self, control: &Path) -> Result<Self> {
        let fresh = Self::start_inner(
            control,
            self.scope.clone(),
            self.mode.as_deref(),
            Some(self.endpoint.clone()),
        )?;
        ensure!(
            fresh.endpoint == self.endpoint,
            "explicit reconnect changed the HTTP endpoint"
        );
        Ok(fresh)
    }
    fn start_inner(
        control: &Path,
        scope: Scope,
        mode: Option<&str>,
        existing: Option<EndpointDescriptor>,
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
                    tokio::time::timeout(TIMEOUT, Wire::open(&control, mode, existing)).await?
                });
                let mut wire = match opened {
                    Ok(wire) => wire,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                if ready.send(Ok(wire.endpoint.clone())).is_err() {
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
            Ok(Ok(endpoint)) => Ok(Self {
                commands: Some(commands),
                thread: Some(worker),
                pending: VecDeque::new(),
                endpoint,
                scope,
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
            .context("HTTP fixture closed")?
            .send(Command::FreshTcp(sender))?;
        receiver.recv_timeout(TIMEOUT + Duration::from_secs(5))??;
        Ok(())
    }
    pub fn send_until(&mut self, mut value: Value, deadline: Instant) -> Result<()> {
        self.scope.inject(&mut value);
        let replies = submit_until(
            self.commands.as_ref().context("HTTP fixture closed")?,
            value,
            deadline,
        )?;
        ensure!(
            self.pending.len() + replies.len() <= 128,
            "HTTP fixture pending message bound exceeded"
        );
        self.pending.extend(replies);
        Ok(())
    }
    pub fn receive_until(&mut self, id: i64, deadline: Instant) -> Result<Value> {
        ensure!(Instant::now() < deadline, "HTTP reply deadline exhausted");
        self.receive(id)
    }
    pub fn receive(&mut self, id: i64) -> Result<Value> {
        let position = self
            .pending
            .iter()
            .position(|reply| reply.get("id").and_then(Value::as_i64) == Some(id))
            .context("matching HTTP JSON-RPC reply missing")?;
        self.pending
            .remove(position)
            .context("matching HTTP reply disappeared")
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
            .context("HTTP fixture closed")?
            .send(Command::Probe(bytes, sender))?;
        let mut replies = receiver.recv_timeout(TIMEOUT + Duration::from_secs(5))??;
        ensure!(
            replies.len() <= 1,
            "single-response HTTP probe returned multiple messages"
        );
        Ok(replies.pop())
    }
    pub fn receive_value(&mut self) -> Result<Value> {
        self.pending.pop_front().context("HTTP reply missing")
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
                .map_err(|_| anyhow::anyhow!("HTTP fixture thread panicked"))?;
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
    endpoint: EndpointDescriptor,
    /// A fixture-local label naming this transport's retained failure evidence.
    label: Incarnation,
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
        mode: Option<String>,
        existing: Option<EndpointDescriptor>,
    ) -> Result<Self> {
        let ControlResponse::Health {
            observation: health,
        } = airk_cli::service_runtime::request(control, ControlMethod::Health).await?
        else {
            anyhow::bail!("native fixture health unavailable")
        };
        // U110: the endpoint comes from the service's endpoint read; nothing
        // is enrolled, no credential exists and there is no issuer to pin.
        let deadline = Instant::now() + TIMEOUT;
        let endpoint = loop {
            let response = airk_cli::service_runtime::request_for_instance(
                control,
                ControlMethod::HttpEndpoint,
                health.instance,
            )
            .await?;
            match response {
                ControlResponse::HttpEndpoint {
                    endpoint: Some(endpoint),
                } => break *endpoint,
                // The endpoint owner starts after the control plane; wait
                // for it under the fixture deadline (not after a reconnect).
                ControlResponse::HttpEndpoint { endpoint: None }
                    if existing.is_none() && Instant::now() < deadline =>
                {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                other => anyhow::bail!(
                    "HTTP endpoint unavailable: {}",
                    serde_json::to_string(&other)?
                ),
            }
        };
        endpoint.validate()?;
        ensure!(
            existing
                .as_ref()
                .is_none_or(|existing| existing == &endpoint),
            "endpoint changed; this reconnect is limited to a catalog boundary"
        );
        ensure!(
            endpoint.service_instance == health.instance
                && endpoint.user == health.user
                && endpoint.executable_sha256.to_string() == health.executable_sha256,
            "HTTP fixture native identity changed"
        );
        ensure!(
            endpoint.url.to_string() == format!("http://127.0.0.1:{}/mcp", endpoint.url.port()),
            "HTTP fixture endpoint is not plain loopback HTTP"
        );
        let (sender, task) = Self::connect(&endpoint).await?;
        let mut random = [0u8; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| anyhow::anyhow!("fixture evidence label unavailable"))?;
        Ok(Self {
            sender,
            task,
            endpoint,
            label: Incarnation::from_bytes(random),
            session: None,
            initialized: false,
            protocol: "2025-11-25".into(),
            mode,
            evidence: control.to_path_buf(),
            sequence: 0,
        })
    }
    async fn connect(
        endpoint: &EndpointDescriptor,
    ) -> Result<(
        hyper::client::conn::http1::SendRequest<Full<Bytes>>,
        tokio::task::JoinHandle<Result<(), hyper::Error>>,
    )> {
        let socket =
            tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, endpoint.url.port()))
                .await?;
        let (sender, connection) =
            hyper::client::conn::http1::handshake(TokioIo::new(socket)).await?;
        let task = tokio::spawn(connection);
        Ok((sender, task))
    }
    async fn fresh_tcp(&mut self) -> Result<()> {
        let (sender, task) = Self::connect(&self.endpoint).await?;
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
            "HTTP fixture request exceeds native frame bound"
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
        let authority = format!("127.0.0.1:{}", self.endpoint.url.port());
        let mut request = http::Request::builder()
            .method(method)
            .uri("/mcp")
            .header(http::header::HOST, authority)
            .header(http::header::CONTENT_TYPE, "application/json")
            .header(http::header::ACCEPT, "application/json, text/event-stream");
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
            .context("HTTP fixture returned invalid Content-Type")?;
        if let Some(session) = response.headers().get("mcp-session-id") {
            let session = session.to_str()?;
            ensure!(
                !session.is_empty()
                    && session.len() <= 256
                    && session.bytes().all(|byte| byte.is_ascii_graphic())
                    && self.session.as_ref().is_none_or(|old| old == session),
                "HTTP fixture session identity changed"
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
                let context=format!("HTTP fixture {method_name} /mcp status={} content_type={:?} body_bytes={} body_sha256={:x} prefix={:?}",
                    status.as_u16(),content_type,bytes.len(),Sha256::digest(&bytes),
                    String::from_utf8_lossy(&bytes[..bytes.len().min(96)]));
                if error.downcast_ref::<HttpRefused>().is_none()
                    && let Err(saved)=retain_http_failure(&self.evidence,self.label,self.sequence,&context,&bytes) {
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
                    "HTTP JSON-RPC method={:?} id={:?}",
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
        "HTTP request deadline exhausted before submission"
    );
    let (sender, receiver) = mpsc::channel();
    let method = value.get("method").cloned().unwrap_or(Value::Null);
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    commands.send(Command::Send(value, deadline, sender))?;
    let replies = receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .with_context(|| format!("HTTP fixture delivery deadline: method={method}, id={id}"))?
        .with_context(|| format!("HTTP fixture request failed: method={method}, id={id}"))?;
    ensure!(
        Instant::now() < deadline,
        "HTTP request deadline exhausted before delivery"
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
        "HTTP fixture response exceeds byte bound"
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
            serde_json::from_slice(bytes).context("invalid HTTP JSON response")?,
        ]);
    }
    ensure!(
        mime.eq_ignore_ascii_case("text/event-stream"),
        "unsupported HTTP fixture response Content-Type"
    );
    let text = std::str::from_utf8(bytes).context("invalid HTTP SSE UTF-8")?;
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
                ensure!(messages.len() < 128, "HTTP SSE message bound exceeded");
                messages.push(serde_json::from_str(&data.join("\n")).with_context(|| {
                    format!("invalid HTTP SSE JSON message {}", messages.len() + 1)
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
    ensure!(data.is_empty(), "HTTP SSE response ended within a message");
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
        "HTTP fixture response evidence retained at {}",
        path.display()
    );
    Ok(())
}

#[cfg(test)]
mod response_tests {
    use super::*;
    #[test]
    fn scope_names_every_read_unless_the_call_names_its_own() {
        let scope = Scope::indexes(["a", "b"]);
        let mut read =
            json!({"method":"tools/call","params":{"name":"search","arguments":{"query":"q"}}});
        scope.inject(&mut read);
        assert_eq!(read["params"]["arguments"]["index_ids"], json!(["a", "b"]));
        assert!(read["params"]["arguments"].get("index_set_ids").is_none());
        let mut bare = json!({"method":"tools/call","params":{"name":"status"}});
        scope.inject(&mut bare);
        assert_eq!(bare["params"]["arguments"], json!({"index_ids":["a","b"]}));
        for own in [
            json!({"index_ids":["c"]}),
            json!({"index_set_ids":["s"]}),
            json!({"workspace":"/w"}),
        ] {
            let mut named =
                json!({"method":"tools/call","params":{"name":"search","arguments":own.clone()}});
            scope.inject(&mut named);
            assert_eq!(named["params"]["arguments"], own);
        }
        for unchanged in [
            json!({"method":"tools/call","params":{"name":"list_scopes","arguments":{}}}),
            json!({"method":"tools/list","params":{}}),
        ] {
            let mut value = unchanged.clone();
            scope.inject(&mut value);
            assert_eq!(value, unchanged);
        }
        let mut unscoped = read.clone();
        unscoped["params"]["arguments"] = json!({"query":"q"});
        let before = unscoped.clone();
        Scope::none().inject(&mut unscoped);
        assert_eq!(unscoped, before);
        assert_eq!(
            Scope::sets(["s"]).cli_arguments(),
            vec!["--set".to_owned(), "s".to_owned()]
        );
    }
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
