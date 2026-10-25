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

    pub async fn call(
        &mut self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.exchange(Query {
            frontend: if tool == "airk_status" {
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
}

struct Backend {
    root: PathBuf,
    project: ProjectBinding,
    state: tokio::sync::Mutex<ClientState>,
    client: std::sync::Mutex<airk_core::usage::ClientFamily>,
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
        Box::pin(async move {
            if tool == "airk_help" {
                return Ok(serde_json::to_value(airk_mcp::help::call(
                    serde_json::Value::Object(arguments),
                ))?);
            }
            if tool == "airk_status" {
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
                crate::runtime_metadata::observe_process();
            }
            let mut state = self.state.lock().await;
            if state
                .connection
                .as_ref()
                .is_none_or(|connection| !connection.reusable(Instant::now()))
            {
                state.connection = None;
                let client = self
                    .client
                    .lock()
                    .map(|client| *client)
                    .unwrap_or(airk_core::usage::ClientFamily::Unknown);
                let connected = if tool == "airk_status" {
                    Connection::connect_once(&self.root, self.project, client).await
                } else {
                    Connection::connect_as(&self.root, self.project, client).await
                };
                let mut connection = match connected {
                    Ok(connection) => connection,
                    Err(_) if tool == "airk_status" => {
                        return Ok(
                            crate::runtime_metadata::unavailable(&self.root, self.project).await,
                        );
                    }
                    Err(error) => return setup_rejection(&error).ok_or(error),
                };
                connection.restore_status_receipt(state.status_receipt);
                state.connection = Some(connection);
            }
            let result = state
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

            result
        })
    }
}

/// One direct client session, shared by stdio MCP and native read-only tools.
pub fn tools(root: PathBuf, project: ProjectBinding) -> Arc<dyn airk_mcp::remote::RemoteTools> {
    Arc::new(Backend {
        root,
        project,
        state: tokio::sync::Mutex::new(Default::default()),
        client: std::sync::Mutex::new(airk_core::usage::ClientFamily::Cli),
    })
}

pub async fn bridge(root: PathBuf, project: ProjectBinding) -> Result<()> {
    airk_mcp::remote::serve(tools(root, project)).await
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn status_without_a_service_returns_unknown_counts_and_never_creates_an_index()
    -> anyhow::Result<()> {
        let control = tempfile::tempdir()?;
        let project = "ab".repeat(32).parse()?;
        let backend = super::tools(control.path().into(), project);
        let reply = backend
            .call("airk_status".into(), Default::default())
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
