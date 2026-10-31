// SPDX-License-Identifier: Apache-2.0
//! A query client obtains one grant, then communicates only with the project worker.
use crate::{
    local_ipc::{self, Stream},
    project_protocol::{self as protocol, Accepted, Query},
    service_runtime,
};
use airk_core::service::{ControlMethod, ControlResponse, PROTOCOL_VERSION, ProjectBinding};
use anyhow::{Result, ensure};
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug)]
struct RetrySetup(&'static str);
impl std::fmt::Display for RetrySetup {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}
impl std::error::Error for RetrySetup {}

fn setup_retryable(error: &anyhow::Error) -> bool {
    error.is::<RetrySetup>()
        || error.chain().any(|cause| {
            cause.downcast_ref::<std::io::Error>().is_some_and(|error| {
                matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::UnexpectedEof
                )
            })
        })
}
async fn retry_setup<T, F, Fut>(
    mut connect: F,
    budget: Duration,
    attempts: usize,
    backoff: Duration,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    tokio::time::timeout(budget, async {
        for attempt in 0..attempts {
            match connect().await {
                Ok(connection) => return Ok(connection),
                Err(error) if attempt + 1 < attempts && setup_retryable(&error) => {
                    tokio::time::sleep(
                        backoff
                            .saturating_mul(1u32 << attempt.min(6))
                            .min(Duration::from_secs(5)),
                    )
                    .await;
                }
                Err(error) => return Err(error),
            }
        }
        anyhow::bail!("direct setup attempt budget exhausted")
    })
    .await
    .map_err(|_| {
        anyhow::Error::new(RetrySetup(
            "direct setup deadline exceeded before query submission",
        ))
    })?
}

fn setup_rejection(error: &anyhow::Error) -> Option<serde_json::Value> {
    if !setup_retryable(error) {
        return None;
    }
    let detail = serde_json::json!({"error":{"code":"direct_query_setup","reason":"busy",
        "query_submitted":false,"retryable":true,"retry_after_ms":100,
        "fresh_brokerage_required":true}});
    Some(serde_json::json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true}))
}

pub struct Connection {
    stream: Stream,
    identity: airk_core::service::WorkerIdentity,
    authorized_until: Instant,
    last_finished: Instant,
    healthy: bool,
    status_receipt: airk_mcp::response::StatusReceipt,
}
impl Connection {
    pub async fn connect(root: &Path, project: ProjectBinding) -> Result<Self> {
        Self::connect_as(root, project, airk_core::usage::ClientFamily::Unknown).await
    }
    pub async fn connect_as(
        root: &Path,
        project: ProjectBinding,
        client: airk_core::usage::ClientFamily,
    ) -> Result<Self> {
        // No evidence query has been submitted. Every retry gets fresh health,
        // scope and a new short-lived grant; an uncertain query is never replayed.
        retry_setup(
            || Self::connect_once(root, project, client),
            Duration::from_secs(30),
            8,
            Duration::from_millis(100),
        )
        .await
    }
    async fn connect_once(
        root: &Path,
        project: ProjectBinding,
        client: airk_core::usage::ClientFamily,
    ) -> Result<Self> {
        let ControlResponse::Health { observation } =
            service_runtime::request(root, ControlMethod::Health).await?
        else {
            anyhow::bail!("service health unavailable")
        };
        ensure!(
            observation.version == env!("CARGO_PKG_VERSION"),
            "native service version differs from this client"
        );
        let (role, mut grant) =
            match service_runtime::request(root, ControlMethod::Connect { project }).await? {
                ControlResponse::Connection { role, grant } => (role, grant),
                ControlResponse::Failure { code } if code == "project_unregistered" => {
                    anyhow::bail!(
                        "project is unregistered; review native registration before connecting"
                    )
                }
                ControlResponse::Failure { code }
                    if matches!(
                        code.as_str(),
                        "broker_busy"
                            | "project_starting_or_unavailable"
                            | "project_unavailable_or_busy"
                            | "previous_workers_draining"
                    ) =>
                {
                    return Err(RetrySetup("direct worker setup temporarily unavailable").into());
                }
                _ => anyhow::bail!("project unavailable; no direct-store fallback"),
            };
        ensure!(
            grant.protocol == PROTOCOL_VERSION
                && grant.identity.service == observation.instance
                && grant.identity.project == project
                && grant.caller == local_ipc::caller_identity()?
                && role == protocol::role(grant.identity.worker),
            "brokered worker identity mismatch"
        );
        let setup_started = Instant::now();
        let mut stream = tokio::time::timeout(
            Duration::from_secs(5),
            local_ipc::connect(&local_ipc::endpoint(root, &role)?),
        )
        .await??;
        grant.client = client;
        local_ipc::write_message(&mut stream, &grant, Duration::from_secs(5)).await?;
        let accepted: Accepted = local_ipc::read_message(&mut stream, Duration::from_secs(5))
            .await?
            .ok_or_else(|| anyhow::anyhow!("project rejected connection grant"))?;
        ensure!(
            accepted.identity == grant.identity,
            "direct worker identity mismatch"
        );
        Ok(Self {
            stream,
            identity: grant.identity,
            authorized_until: setup_started + Duration::from_secs(55),
            last_finished: Instant::now(),
            healthy: true,
            status_receipt: Default::default(),
        })
    }
    pub(crate) fn reusable(&self, now: Instant) -> bool {
        self.healthy
            && now < self.authorized_until
            && now.saturating_duration_since(self.last_finished) < Duration::from_secs(25)
    }

    fn reusable_for_source(&self, fence: SourceFence, now: Instant) -> bool {
        self.reusable(now)
            && self.identity.project == fence.project
            && self.identity.fencing_generation >= fence.fencing_generation
    }

    pub async fn call(
        &mut self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.exchange(Query {
            frontend: if tool == "airk_diagnostics" {
                Some(crate::runtime_metadata::observe_process())
            } else {
                None
            },
            tool,
            arguments,
            native: None,
            status_receipt: self.status_receipt,
        })
        .await
    }
    pub async fn read_native(
        &mut self,
        request: airk_mcp::local::NativeRead,
    ) -> Result<serde_json::Value> {
        self.exchange(Query {
            frontend: None,
            tool: String::new(),
            arguments: Default::default(),
            native: Some(request),
            status_receipt: self.status_receipt,
        })
        .await
    }
    async fn exchange(&mut self, query: Query) -> Result<serde_json::Value> {
        ensure!(
            self.reusable(Instant::now()),
            "direct connection expired before query submission; obtain fresh brokerage"
        );
        // A dropped async caller may leave an unread result on this stream. Mark
        // it unusable before the first write; restore only after a complete reply.
        self.healthy = false;
        local_ipc::write_message(&mut self.stream, &query, Duration::from_secs(5)).await?;
        let result: protocol::QueryResult =
            local_ipc::read_message(&mut self.stream, Duration::from_secs(120))
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("project query disconnected; retry through fresh brokerage")
                })?;
        self.last_finished = Instant::now();
        self.healthy = !result.rebind;
        self.status_receipt = result.status_receipt;
        Ok(result.result)
    }

    pub fn status_receipt(&self) -> airk_mcp::response::StatusReceipt {
        self.status_receipt
    }
    pub fn restore_status_receipt(&mut self, receipt: airk_mcp::response::StatusReceipt) {
        self.status_receipt = receipt;
    }
}

#[derive(Default)]
struct ClientState {
    connection: Option<Connection>,
    status_receipt: airk_mcp::response::StatusReceipt,
    cached_status: Option<CachedStatus>,
}

pub(crate) struct Backend {
    root: PathBuf,
    project: ProjectBinding,
    state: tokio::sync::Mutex<ClientState>,
    latest_source: std::sync::Mutex<Option<SourceFence>>,
    client: std::sync::Mutex<airk_core::usage::ClientFamily>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceFence {
    pub namespace: airk_core::identity::Incarnation,
    pub revision: u64,
    pub project: ProjectBinding,
    pub fencing_generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CacheScope {
    Pinned(airk_core::identity::Incarnation),
    Source(SourceFence),
}
impl From<airk_core::identity::Incarnation> for CacheScope {
    fn from(value: airk_core::identity::Incarnation) -> Self {
        Self::Pinned(value)
    }
}
pub(crate) fn source_scope_superseded() -> serde_json::Value {
    let detail = serde_json::json!({"error":{"code":"direct_query_setup","reason":"busy",
        "query_submitted":false,"retryable":true,"retry_after_ms":100,"fresh_brokerage_required":true}});
    serde_json::json!({"content":[{"type":"text","text":detail.to_string()}],"isError":true})
}
struct CachedStatus {
    scope: CacheScope,
    captured: Instant,
    observed_unix_ms: u64,
    body: serde_json::Value,
}
impl CachedStatus {
    fn agrees_with(&self, result: &Result<serde_json::Value>) -> bool {
        let Ok(result) = result else {
            return false;
        };
        if result["isError"] == true {
            return false;
        }
        let Some(body) = result["content"][0]["text"]
            .as_str()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        else {
            return false;
        };
        [
            &body["publication"]["token"],
            &body["read"]["current_publication"]["token"],
            &body["read"]["publication"]["token"],
        ]
        .into_iter()
        .find(|token| token.is_string())
        .is_some_and(|token| *token == self.body["publication"]["token"])
    }
    fn new(scope: impl Into<CacheScope>, result: &serde_json::Value) -> Option<Self> {
        if result["isError"] == true {
            return None;
        }
        let body: serde_json::Value =
            serde_json::from_str(result["content"][0]["text"].as_str()?).ok()?;
        if body["index_state"] != "available" || !body["publication"]["token"].is_string() {
            return None;
        }
        Some(Self {
            scope: scope.into(),
            captured: Instant::now(),
            observed_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis()
                .try_into()
                .ok()?,
            body,
        })
    }
    fn reply(
        &self,
        scope: impl Into<CacheScope>,
        now: Instant,
        cached: bool,
    ) -> Option<serde_json::Value> {
        let age = now.saturating_duration_since(self.captured);
        if scope.into() != self.scope || age >= Duration::from_secs(5) {
            return None;
        }
        let mut body = self.body.clone();
        body["observed_unix_ms"] = self.observed_unix_ms.into();
        body["cache_age_ms"] = (age.as_millis() as u64).into();
        body["cached"] = cached.into();
        Some(airk_mcp::unified::success(body))
    }
}

fn status_scope(root: &Path, project: ProjectBinding) -> Result<Option<CacheScope>> {
    if !root.join("service-1.4.sqlite3").try_exists()? {
        return Ok(None);
    }
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(root)?;
    registry.get(project)?; // A removed project cannot be served from a cache.
    Ok(Some(CacheScope::Pinned(registry.incarnation()?)))
}
impl airk_mcp::remote::RemoteTools for Backend {
    fn client(&self, client: airk_core::usage::ClientFamily) {
        if let Ok(mut label) = self.client.lock() {
            *label = client;
        }
    }
    fn call(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(tool, arguments, None, None)
    }
}

impl Backend {
    fn accept_source_fence(&self, fence: SourceFence) -> bool {
        if fence.project != self.project {
            return false;
        }
        let mut latest = self
            .latest_source
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if latest.is_some_and(|old| {
            old.namespace != fence.namespace
                || old.revision > fence.revision
                || (old.revision == fence.revision
                    && old.fencing_generation > fence.fencing_generation)
        }) {
            return false;
        }
        *latest = Some(fence);
        true
    }
    fn source_fence_current(&self, scope: CacheScope) -> bool {
        match scope {
            CacheScope::Pinned(_) => true,
            CacheScope::Source(fence) => {
                self.latest_source
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .as_ref()
                    == Some(&fence)
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn call_observed(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        fence: SourceFence,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(tool, arguments, Some(fence), None)
    }
    pub(crate) fn call_observed_until(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        fence: SourceFence,
        expires: Instant,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.call_bound(tool, arguments, Some(fence), Some(expires))
    }
    fn call_bound(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        observed: Option<SourceFence>,
        source_expires: Option<Instant>,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        Box::pin(async move {
            if tool == "airk_help" {
                return Ok(serde_json::to_value(airk_mcp::help::call(
                    serde_json::Value::Object(arguments),
                ))?);
            }
            if matches!(tool.as_str(), "airk_status" | "airk_diagnostics") {
                if serde_json::from_value::<airk_mcp::StatusArgs>(serde_json::Value::Object(
                    arguments.clone(),
                ))
                .is_err()
                {
                    return Ok(
                        serde_json::json!({"content":[{"type":"text","text":"Invalid airk_status arguments; inspect its current schema with airk_help."}],"isError":true}),
                    );
                }
                // Cold image hashing must not consume a freshly brokered connection's idle/grant lifetime.
                if tool == "airk_diagnostics" {
                    crate::runtime_metadata::observe_process();
                }
            }
            if let Some(fence) = observed
                && !self.accept_source_fence(fence)
            {
                return Ok(source_scope_superseded());
            }
            let scope = if tool == "airk_status" {
                if let Some(fence) = observed {
                    Some(CacheScope::Source(fence))
                } else {
                    let root = self.root.clone();
                    let project = self.project;
                    tokio::task::spawn_blocking(move || status_scope(&root, project)).await??
                }
            } else {
                None
            };
            let mut state = self.state.lock().await;
            if source_expires.is_some_and(|until| Instant::now() >= until) {
                return Ok(source_scope_superseded());
            }
            if observed.is_some_and(|fence| !self.source_fence_current(CacheScope::Source(fence))) {
                return Ok(source_scope_superseded());
            }
            if let Some(scope) = scope
                && let Some(reply) = state
                    .cached_status
                    .as_ref()
                    .and_then(|cached| cached.reply(scope, Instant::now(), true))
            {
                return Ok(reply);
            }
            if tool == "airk_diagnostics" {
                state.cached_status = None;
            }
            if state.connection.as_ref().is_none_or(|connection| {
                observed.map_or_else(
                    || !connection.reusable(Instant::now()),
                    |fence| !connection.reusable_for_source(fence, Instant::now()),
                )
            }) {
                state.connection = None;
                let client = self
                    .client
                    .lock()
                    .map(|client| *client)
                    .unwrap_or(airk_core::usage::ClientFamily::Unknown);
                let connected = if matches!(tool.as_str(), "airk_status" | "airk_diagnostics") {
                    Connection::connect_once(&self.root, self.project, client).await
                } else {
                    Connection::connect_as(&self.root, self.project, client).await
                };
                // Brokerage can await worker admission. A concurrent source
                // observation may supersede this request during that await.
                if source_expires.is_some_and(|until| Instant::now() >= until)
                    || observed
                        .is_some_and(|fence| !self.source_fence_current(CacheScope::Source(fence)))
                {
                    return Ok(source_scope_superseded());
                }
                let mut connection = match connected {
                    Ok(connection) => connection,
                    Err(_) if matches!(tool.as_str(), "airk_status" | "airk_diagnostics") => {
                        if tool == "airk_status" {
                            return Ok(airk_mcp::unified::success(
                                serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","index_state":"unavailable","publication":null,"files":null,"chunks":null,"index_error":{"code":"index_worker_unavailable","message":"No index query was submitted. Call diagnostics for service and queued-job diagnosis."}}),
                            ));
                        }
                        return Ok(
                            crate::runtime_metadata::unavailable(&self.root, self.project).await,
                        );
                    }
                    Err(error) => return setup_rejection(&error).ok_or(error),
                };
                connection.restore_status_receipt(state.status_receipt);
                state.connection = Some(connection);
            }
            if source_expires.is_some_and(|until| Instant::now() >= until) {
                return Ok(source_scope_superseded());
            }
            if observed.is_some_and(|fence| !self.source_fence_current(CacheScope::Source(fence))) {
                return Ok(source_scope_superseded());
            }
            let is_status = tool == "airk_status";
            let mut result = state
                .connection
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("project connection unavailable"))?
                .call(tool, arguments)
                .await;
            if result.is_err() {
                state.connection = None;
            } else if let Some(connection) = &state.connection {
                // Only a completely received result advances this client's
                // observations. Cancellation/reconnect retains its older receipt.
                state.status_receipt = connection.status_receipt();
            }

            if !is_status
                && state
                    .cached_status
                    .as_ref()
                    .is_some_and(|cached| !cached.agrees_with(&result))
            {
                // A changed publication or rejected old token invalidates the
                // preflight cache immediately rather than repeating a stale token.
                state.cached_status = None;
            }
            if is_status {
                state.cached_status = scope
                    .filter(|scope| self.source_fence_current(*scope))
                    .and_then(|scope| {
                        result
                            .as_ref()
                            .ok()
                            .and_then(|value| CachedStatus::new(scope, value))
                    });
                if let Some(cached) = &state.cached_status
                    && let Some(reply) = cached.reply(cached.scope, Instant::now(), false)
                {
                    result = Ok(reply);
                }
            }

            result
        })
    }
}

/// One direct client session, shared by stdio MCP and native read-only tools.
pub fn tools(root: PathBuf, project: ProjectBinding) -> Arc<dyn airk_mcp::remote::RemoteTools> {
    source_tools(root, project)
}
pub(crate) fn source_tools(root: PathBuf, project: ProjectBinding) -> Arc<Backend> {
    Arc::new(Backend {
        root,
        project,
        state: tokio::sync::Mutex::new(Default::default()),
        latest_source: std::sync::Mutex::new(None),
        client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Cli),
    })
}

pub async fn bridge(root: PathBuf, project: ProjectBinding) -> Result<()> {
    airk_mcp::remote::serve(tools(root, project)).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn status_cache_is_success_only_expires_at_five_seconds_and_fences_recreated_registries() {
        let scope = airk_core::identity::Incarnation::from_bytes([1; 16]);
        let value = airk_mcp::unified::success(
            serde_json::json!({"version":"1.6.1","health":"ok","index_state":"available","publication":{"token":"exact-original-token"}}),
        );
        let cached = super::CachedStatus::new(scope, &value).unwrap();
        assert!(cached.agrees_with(&Ok(value.clone())));
        assert!(!cached.agrees_with(&Ok(airk_mcp::unified::success(
            serde_json::json!({"publication":{"token":"new-rebuilt-token"}})
        ))));
        assert!(!cached.agrees_with(&Ok(airk_mcp::unified::failure(
            "current_revision_expired",
            "retired token"
        ))));

        let reply = cached
            .reply(
                scope,
                cached.captured + std::time::Duration::from_millis(4999),
                true,
            )
            .unwrap();
        assert_eq!(reply["structuredContent"]["cache_age_ms"], 4999);
        assert_eq!(reply["structuredContent"]["cached"], true);
        assert_eq!(
            reply["structuredContent"]["publication"]["token"],
            "exact-original-token"
        );
        assert!(
            cached
                .reply(
                    scope,
                    cached.captured + std::time::Duration::from_secs(5),
                    true
                )
                .is_none()
        );
        assert!(
            cached
                .reply(
                    airk_core::identity::Incarnation::from_bytes([2; 16]),
                    cached.captured,
                    true
                )
                .is_none()
        );
        for state in ["unknown", "unavailable"] {
            assert!(
                super::CachedStatus::new(
                    scope,
                    &airk_mcp::unified::success(serde_json::json!({"index_state":state}))
                )
                .is_none()
            );
        }
        assert!(
            super::CachedStatus::new(scope, &airk_mcp::unified::failure("failure", "unavailable"))
                .is_none()
        );
    }
    #[tokio::test]
    async fn status_without_a_service_returns_unknown_counts_and_never_creates_an_index()
    -> anyhow::Result<()> {
        let control = tempfile::tempdir()?;
        let project = "ab".repeat(32).parse()?;
        let backend = super::tools(control.path().into(), project);
        let reply = backend
            .call(
                "airk_diagnostics".into(),
                serde_json::json!({"response_mode":"full"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .await?;
        assert_ne!(reply["isError"], true);
        let status: serde_json::Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(status["index_state"], "unavailable");
        assert!(status["publication"].is_null());
        assert!(status["files"].is_null());
        assert!(status["runtime"]["service"].is_null());
        assert!(
            !status["work"]["failed_observations"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(std::fs::read_dir(control.path())?.next().is_none());
        let invalid = backend
            .call(
                "airk_status".into(),
                serde_json::json!({"project":"foreign"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .await?;
        assert_eq!(invalid["isError"], true);
        Ok(())
    }
    use super::*;

    #[tokio::test]
    async fn setup_retries_transient_admission_without_replaying_a_submitted_query() -> Result<()> {
        let mut attempts = 0;
        let result = retry_setup(
            || {
                attempts += 1;
                std::future::ready(if attempts < 3 {
                    Err(RetrySetup("starting").into())
                } else {
                    Ok(42)
                })
            },
            Duration::from_secs(1),
            4,
            Duration::from_millis(1),
        )
        .await?;
        assert_eq!(result, 42);
        assert_eq!(attempts, 3);
        let mut refused = 0;
        let result: Result<()> = retry_setup(
            || {
                refused += 1;
                std::future::ready(Err(anyhow::anyhow!("worker identity mismatch")))
            },
            Duration::from_secs(1),
            4,
            Duration::from_millis(1),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(refused, 1);
        Ok(())
    }
    #[tokio::test]
    async fn setup_has_finite_attempts_and_an_independent_deadline() {
        let mut attempts = 0;
        let result: Result<()> = retry_setup(
            || {
                attempts += 1;
                std::future::ready(Err(RetrySetup("starting").into()))
            },
            Duration::from_secs(1),
            3,
            Duration::from_millis(1),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(attempts, 3);
        let result: Result<()> = retry_setup(
            std::future::pending,
            Duration::from_millis(10),
            3,
            Duration::from_millis(1),
        )
        .await;
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("before query submission")
        );
        assert!(!setup_retryable(
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()
        ));
        assert!(setup_retryable(
            &std::io::Error::from(std::io::ErrorKind::ConnectionReset).into()
        ));
    }

    #[test]
    fn only_temporary_pre_submission_setup_failures_get_a_retry_receipt() {
        for error in [
            anyhow::Error::new(RetrySetup("capacity")),
            std::io::Error::from(std::io::ErrorKind::ConnectionRefused).into(),
        ] {
            let envelope = setup_rejection(&error).unwrap();
            let detail: serde_json::Value =
                serde_json::from_str(envelope["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(envelope["isError"], true);
            assert_eq!(detail["error"]["code"], "direct_query_setup");
            assert_eq!(detail["error"]["query_submitted"], false);
            assert_eq!(detail["error"]["fresh_brokerage_required"], true);
            assert_eq!(detail["error"]["retryable"], true);
        }
        for error in [
            anyhow::anyhow!("identity mismatch"),
            anyhow::anyhow!("query outcome unknown"),
            std::io::Error::from(std::io::ErrorKind::PermissionDenied).into(),
        ] {
            assert!(setup_rejection(&error).is_none());
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn canceled_direct_exchange_is_not_reused_and_known_deadlines_precede_submission()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let endpoint = local_ipc::endpoint(&root, "direct-client-lifetime-test")?;
        let mut listener = local_ipc::Listener::bind(&endpoint)?;
        let sent = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let peer_sent = sent.clone();
        let peer_release = release.clone();
        let peer = tokio::spawn(async move {
            let mut stream = listener.accept().await?;
            let query: Query = local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .ok_or_else(|| anyhow::anyhow!("test query not submitted"))?;
            ensure!(query.tool == "airk_status", "unexpected fixture request");
            peer_sent.notify_one();
            peer_release.notified().await;
            let _ = local_ipc::write_message(
                &mut stream,
                &serde_json::json!({"fixture":"complete"}),
                Duration::from_secs(5),
            )
            .await;
            Ok::<_, anyhow::Error>(())
        });
        let stream = local_ipc::connect(&endpoint).await?;
        let now = Instant::now();
        let mut connection = Connection {
            stream,
            identity: airk_core::service::WorkerIdentity {
                service: airk_core::identity::Incarnation::from_bytes([1; 16]),
                worker: airk_core::identity::Incarnation::from_bytes([2; 16]),
                project: "ab".repeat(32).parse()?,
                fencing_generation: 1,
            },
            authorized_until: now + Duration::from_secs(55),
            last_finished: now,
            healthy: true,
            status_receipt: Default::default(),
        };
        assert!(connection.reusable(now + Duration::from_secs(24)));
        assert!(!connection.reusable(now + Duration::from_secs(25)));
        assert!(!connection.reusable(now + Duration::from_secs(56)));
        {
            let request = connection.call("airk_status".into(), Default::default());
            tokio::pin!(request);
            tokio::select! {
                _=sent.notified()=>{},
                result=&mut request=>anyhow::bail!("fixture exchange completed before cancellation: {result:?}"),
            }
        }
        assert!(!connection.reusable(Instant::now()));
        drop(connection);
        release.notify_one();
        peer.await??;
        Ok(())
    }
}

#[cfg(test)]
mod source_observation_tests {
    use super::*;

    fn fence(namespace: u8, revision: u64) -> SourceFence {
        SourceFence {
            namespace: airk_core::identity::Incarnation::from_bytes([namespace; 16]),
            revision,
            project: "ab".repeat(32).parse().unwrap(),
            fencing_generation: 1,
        }
    }

    #[test]
    fn revision_namespace_and_generation_are_cache_scope_not_mutable_authority() {
        let old = fence(1, 9);
        let value = airk_mcp::unified::success(serde_json::json!({
            "index_state":"available", "publication":{"token":"saved"}
        }));
        let cached = CachedStatus::new(CacheScope::Source(old), &value).unwrap();
        for changed in [
            fence(2, 9),
            fence(1, 10),
            SourceFence {
                fencing_generation: 2,
                ..old
            },
        ] {
            assert!(
                cached
                    .reply(CacheScope::Source(changed), cached.captured, true)
                    .is_none()
            );
        }
        assert!(
            cached
                .reply(
                    CacheScope::Source(old),
                    cached.captured + Duration::from_secs(5),
                    true
                )
                .is_none()
        );
    }

    #[tokio::test]
    async fn old_observation_waiting_for_backend_is_explicitly_unsubmitted() -> Result<()> {
        let control = tempfile::tempdir()?;
        let old = fence(1, 9);
        let backend = source_tools(control.path().into(), old.project);
        let guard = backend.state.lock().await;
        let mut request = backend.call_observed("airk_status".into(), serde_json::Map::new(), old);
        // Poll exactly once: admit the old scope, then block on the owned state lock.
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(request.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        assert!(backend.accept_source_fence(fence(1, 10)));
        drop(guard);
        let reply = request.await?;
        let detail: serde_json::Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(detail["error"]["query_submitted"], false);
        assert_eq!(detail["error"]["retryable"], true);
        assert!(backend.state.lock().await.connection.is_none());
        assert!(std::fs::read_dir(control.path())?.next().is_none());
        Ok(())
    }

    #[tokio::test]
    async fn observed_cache_hit_does_not_reopen_registry_and_retains_age() -> Result<()> {
        let control = tempfile::tempdir()?;
        let scope = fence(1, 9);
        let backend = source_tools(control.path().into(), scope.project);
        let value = airk_mcp::unified::success(serde_json::json!({
            "index_state":"available", "publication":{"token":"saved"}
        }));
        let mut cached = CachedStatus::new(CacheScope::Source(scope), &value).unwrap();
        cached.captured -= Duration::from_secs(1);
        let timestamp = cached.observed_unix_ms;
        backend.state.lock().await.cached_status = Some(cached);
        // There is deliberately no registry database: the owned observation is the fence.
        let reply = backend
            .call_observed("airk_status".into(), serde_json::Map::new(), scope)
            .await?;
        let body: serde_json::Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(body["cached"], true);
        assert_eq!(body["observed_unix_ms"], timestamp);
        assert!(body["cache_age_ms"].as_u64().unwrap() >= 1000);
        assert!(backend.state.lock().await.connection.is_none());
        assert!(std::fs::read_dir(control.path())?.next().is_none());
        Ok(())
    }
}

#[cfg(test)]
mod source_cache_return_age_tests {
    use super::*;
    #[tokio::test]
    async fn expired_source_observation_cannot_hit_fresh_status_after_backend_wait() -> Result<()> {
        let control = tempfile::tempdir()?;
        let fence = SourceFence {
            namespace: airk_core::identity::Incarnation::from_bytes([81; 16]),
            revision: 1,
            project: "ab".repeat(32).parse()?,
            fencing_generation: 1,
        };
        let backend = source_tools(control.path().into(), fence.project);
        let mut state = backend.state.lock().await;
        state.cached_status = CachedStatus::new(
            CacheScope::Source(fence),
            &airk_mcp::unified::success(
                serde_json::json!({"index_state":"available","publication":{"token":"saved"}}),
            ),
        );
        let mut request = backend.call_observed_until(
            "airk_status".into(),
            serde_json::Map::new(),
            fence,
            Instant::now() + Duration::from_millis(20),
        );
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(request.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
        drop(state);
        let reply = request.await?;
        let detail: serde_json::Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(detail["error"]["query_submitted"], false);
        assert_eq!(detail["error"]["retryable"], true);
        assert!(backend.state.lock().await.connection.is_none());
        assert!(std::fs::read_dir(control.path())?.next().is_none());
        Ok(())
    }
}

#[cfg(test)]
mod source_brokerage_ipc_tests {
    use super::*;
    use airk_core::{
        identity::Incarnation,
        service::{ConnectionGrant, ControlRequest, ServiceHealth, WorkerIdentity},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Peer {
        broker: tokio::task::JoinHandle<Result<()>>,
        worker: tokio::task::JoinHandle<Result<()>>,
        broker_began: Arc<tokio::sync::Notify>,
        broker_release: Arc<tokio::sync::Notify>,
        query_began: Arc<tokio::sync::Notify>,
        query_release: Arc<tokio::sync::Notify>,
        queries: Arc<AtomicUsize>,
        receipt: airk_mcp::response::StatusReceipt,
    }
    fn fence(generation: u64, revision: u64) -> SourceFence {
        SourceFence {
            namespace: Incarnation::from_bytes([101; 16]),
            revision,
            project: "ab".repeat(32).parse().unwrap(),
            fencing_generation: generation,
        }
    }
    // Real same-user control and worker transports; the peer implements only
    // Health/Connect/grant acceptance and records actual submitted Query frames.
    fn peer(
        root: &Path,
        old: SourceFence,
        generation: u64,
        pause_broker: bool,
        pause_reply: bool,
    ) -> Result<Peer> {
        local_ipc::prepare_root(root)?;
        let mut control = local_ipc::Listener::bind(&local_ipc::endpoint(root, "control")?)?;
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([102; 16]),
            worker: Incarnation::from_bytes([103; 16]),
            project: old.project,
            fencing_generation: generation,
        };
        let role = protocol::role(identity.worker);
        let mut worker = local_ipc::Listener::bind(&local_ipc::endpoint(root, &role)?)?;
        let grant = ConnectionGrant {
            protocol: PROTOCOL_VERSION,
            identity,
            caller: local_ipc::caller_identity()?,
            token: "cd".repeat(32).parse()?,
            ttl_ms: 5000,
            client: airk_core::usage::ClientFamily::Unknown,
        };
        let health = ServiceHealth {
            storage_formats: None,
            build: None,
            build_error: None,
            logs: airk_core::service_logs::LogTelemetry {
                available: false,
                dropped_records: 0,
                write_failures: 0,
            },
            version: env!("CARGO_PKG_VERSION").into(),
            protocol: PROTOCOL_VERSION,
            instance: identity.service,
            process_id: std::process::id(),
            executable_sha256: "00".repeat(32),
            runtime_sha256: None,
            user: local_ipc::current_user()?,
            uptime_ms: 0,
            policy: airk_kernel::service_broker::Broker::new(identity.service, Default::default())?
                .status(Instant::now()),
        };
        let broker_began = Arc::new(tokio::sync::Notify::new());
        let broker_release = Arc::new(tokio::sync::Notify::new());
        let began = broker_began.clone();
        let release = broker_release.clone();
        let broker = tokio::spawn(async move {
            let mut stream = control.accept().await?;
            let request: ControlRequest =
                local_ipc::read_message(&mut stream, Duration::from_secs(5))
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("health request missing"))?;
            ensure!(
                matches!(request.method, ControlMethod::Health),
                "unexpected control request"
            );
            local_ipc::write_message(
                &mut stream,
                &ControlResponse::Health {
                    observation: Box::new(health),
                },
                Duration::from_secs(5),
            )
            .await?;
            drop(stream);
            let mut stream = control.accept().await?;
            let request: ControlRequest =
                local_ipc::read_message(&mut stream, Duration::from_secs(5))
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("Connect missing"))?;
            ensure!(
                matches!(request.method,ControlMethod::Connect { project } if project==identity.project),
                "unexpected brokerage"
            );
            began.notify_one();
            if pause_broker {
                release.notified().await;
            }
            local_ipc::write_message(
                &mut stream,
                &ControlResponse::Connection { role, grant },
                Duration::from_secs(5),
            )
            .await?;
            Ok(())
        });
        let query_began = Arc::new(tokio::sync::Notify::new());
        let query_release = Arc::new(tokio::sync::Notify::new());
        let queries = Arc::new(AtomicUsize::new(0));
        let counted = queries.clone();
        let began = query_began.clone();
        let release = query_release.clone();
        let receipt: airk_mcp::response::StatusReceipt =
            serde_json::from_value(serde_json::json!({"freshness":"ef".repeat(32)}))?;
        let worker = tokio::spawn(async move {
            let mut stream = worker.accept().await?;
            let observed: ConnectionGrant =
                local_ipc::read_message(&mut stream, Duration::from_secs(5))
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("grant missing"))?;
            ensure!(observed.identity == identity, "grant identity changed");
            local_ipc::write_message(&mut stream, &Accepted { identity }, Duration::from_secs(5))
                .await?;
            while let Some(_query) =
                local_ipc::read_message::<_, Query>(&mut stream, Duration::from_secs(5)).await?
            {
                counted.fetch_add(1, Ordering::SeqCst);
                began.notify_one();
                if pause_reply {
                    release.notified().await;
                }
                local_ipc::write_message(&mut stream,&protocol::QueryResult {
                    result:airk_mcp::unified::success(serde_json::json!({"index_state":"available","publication":{"token":"original-query"}})),
                    status_receipt:receipt,rebind:false },Duration::from_secs(5)).await?;
            }
            Ok(())
        });
        Ok(Peer {
            broker,
            worker,
            broker_began,
            broker_release,
            query_began,
            query_release,
            queries,
            receipt,
        })
    }
    fn body(reply: &serde_json::Value) -> Result<serde_json::Value> {
        Ok(serde_json::from_str(
            reply["content"][0]["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("reply body missing"))?,
        )?)
    }
    #[tokio::test]
    async fn superseded_during_actual_brokerage_never_submits_query_or_installs_receipt()
    -> Result<()> {
        for newer in [fence(1, 10), fence(2, 9)] {
            let owner = tempfile::tempdir()?;
            let root = owner.path().join("runtime");
            let old = fence(1, 9);
            let peer = peer(&root, old, 2, true, false)?;
            let backend = source_tools(root.clone(), old.project);
            let mut request = backend.call_observed("airk_status".into(), Default::default(), old);
            tokio::select! { _=peer.broker_began.notified()=>{}, result=&mut request=>anyhow::bail!("broker did not block: {result:?}") }
            assert!(backend.accept_source_fence(newer));
            peer.broker_release.notify_one();
            let reply = request.await?;
            {
                let state = backend.state.lock().await;
                assert!(state.connection.is_none());
                assert!(state.cached_status.is_none());
                assert_eq!(state.status_receipt, Default::default());
            }
            drop(backend);
            peer.broker.await??;
            peer.worker.await??;
            assert_eq!(peer.queries.load(Ordering::SeqCst), 0);
            assert_eq!(body(&reply)?["error"]["query_submitted"], false);
            assert_eq!(body(&reply)?["error"]["retryable"], true);
        }
        Ok(())
    }
    #[tokio::test]
    async fn cold_newer_grant_submits_but_newer_observed_epoch_discards_retained_channel()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        let old = fence(0, 9);
        let peer = peer(&root, old, 2, false, false)?;
        let backend = source_tools(root.clone(), old.project);
        let first = backend
            .call_observed("airk_status".into(), Default::default(), old)
            .await?;
        assert_eq!(body(&first)?["index_state"], "available");
        peer.broker.await??; // No broker remains; a known stale channel must not hide that fact.
        {
            let state = backend.state.lock().await;
            let connection = state.connection.as_ref().unwrap();
            assert_eq!(connection.identity.fencing_generation, 2);
            assert!(connection.reusable_for_source(old, Instant::now()));
            assert!(connection.reusable_for_source(fence(2, 10), Instant::now()));
            assert!(!connection.reusable_for_source(fence(3, 11), Instant::now()));
        }
        let second = backend
            .call_observed("airk_status".into(), Default::default(), fence(3, 11))
            .await?;
        assert_eq!(
            body(&second)?["index_error"]["code"],
            "index_worker_unavailable"
        );
        assert_eq!(
            backend.state.lock().await.status_receipt,
            peer.receipt,
            "logical complete receipt survives channel invalidation"
        );
        drop(backend);
        peer.worker.await??;
        assert_eq!(
            peer.queries.load(Ordering::SeqCst),
            1,
            "no query may use the known superseded channel"
        );
        Ok(())
    }
    #[tokio::test]
    async fn submitted_old_reply_preserves_own_receipt_but_cannot_repopulate_source_status_cache()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        let old = fence(1, 9);
        let peer = peer(&root, old, 1, false, true)?;
        let backend = source_tools(root.clone(), old.project);
        let mut request = backend.call_observed("airk_status".into(), Default::default(), old);
        tokio::select! { _=peer.query_began.notified()=>{}, result=&mut request=>anyhow::bail!("query did not block: {result:?}") }
        assert!(backend.accept_source_fence(fence(2, 10)));
        peer.query_release.notify_one();
        let reply = request.await?;
        assert_eq!(body(&reply)?["publication"]["token"], "original-query");
        {
            let state = backend.state.lock().await;
            assert!(state.cached_status.is_none());
            assert_eq!(state.status_receipt, peer.receipt);
        }
        drop(backend);
        peer.broker.await??;
        peer.worker.await??;
        assert_eq!(
            peer.queries.load(Ordering::SeqCst),
            1,
            "submitted reads must never replay"
        );
        Ok(())
    }
}
