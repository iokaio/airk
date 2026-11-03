// SPDX-License-Identifier: Apache-2.0
//! Read-only stdio adapter: the native host supplies an authenticated direct connection.
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

pub type RemoteFuture<'a> =
    Pin<Box<dyn Future<Output = anyhow::Result<serde_json::Value>> + Send + 'a>>;
pub type ReadyFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;
#[derive(Debug)]
pub struct PreparationExpired;
impl std::fmt::Display for PreparationExpired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("owned status setup deadline expired")
    }
}
impl std::error::Error for PreparationExpired {}
pub trait RemoteTools: Send + Sync + 'static {
    fn query_concurrency(&self) -> usize {
        airk_core::service_resources::DEFAULT_QUERY_CONCURRENCY
    }
    /// Static catalog/help/initialize never invokes this owned setup hook.
    fn prepare(&self) -> ReadyFuture<'_> {
        Box::pin(async { Ok(()) })
    }
    fn close(&self) {}
    fn shutdown(&self) -> ReadyFuture<'_> {
        Box::pin(async { Ok(()) })
    }
    fn catalog(&self) -> Vec<Tool> {
        crate::local::tools()
    }
    fn guidance(&self) -> &'static str {
        crate::response::GUIDANCE
    }
    fn local_help(&self) -> bool {
        true
    }
    fn uses_response_mode(&self, _tool: &str) -> bool {
        true
    }
    fn client(&self, _client: airk_core::usage::ClientFamily) {}
    fn call(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
    ) -> RemoteFuture<'_>;
}

#[derive(Clone)]
struct Bridge {
    backend: Arc<dyn RemoteTools>,
    admission: Arc<Admission>,
    response_mode: crate::response::ResponseMode,
}

// Evidence dispatch can wait on a native owner for much longer than a scalar
// status observation. Reserve the fourth executing slot for the exact status
// aliases so admission cannot put status behind those evidence calls. Neither
// diagnostics, help nor UI proposals are admitted through that reserved slot.
struct Admission {
    calls: Arc<crate::query_capacity::QueryCapacity>,
    status: Arc<tokio::sync::Semaphore>,
    waiting: Arc<tokio::sync::Semaphore>,
}
impl Default for Admission {
    fn default() -> Self {
        Self {
            calls: crate::query_capacity::QueryCapacity::new(
                airk_core::service_resources::DEFAULT_QUERY_CONCURRENCY,
            ),
            status: Arc::new(tokio::sync::Semaphore::new(1)),
            waiting: Arc::new(tokio::sync::Semaphore::new(128)),
        }
    }
}
#[derive(Debug)]
#[allow(dead_code)] // Variants retain their owned permits until handler completion.
enum BridgePermit {
    Evidence(crate::query_capacity::QueryPermit),
    Status(tokio::sync::OwnedSemaphorePermit),
}

#[derive(Debug, PartialEq, Eq)]
enum AdmissionFailure {
    Busy,
    Closed,
    Canceled,
}
impl Admission {
    fn close(&self) {
        self.calls.close();
        self.status.close();
        self.waiting.close();
    }

    async fn acquire(
        &self,
        tool: &str,
        canceled: impl Future<Output = ()>,
        expires: tokio::time::Instant,
    ) -> Result<BridgePermit, AdmissionFailure> {
        let status = matches!(tool, "status" | "airk_status");
        let lane = &self.calls;
        let admitted = async {
            if tokio::time::Instant::now() >= expires {
                return Err(AdmissionFailure::Busy);
            }
            if status {
                match self.status.clone().try_acquire_owned() {
                    Ok(permit) => return Ok(BridgePermit::Status(permit)),
                    Err(tokio::sync::TryAcquireError::Closed) => {
                        return Err(AdmissionFailure::Closed);
                    }
                    Err(tokio::sync::TryAcquireError::NoPermits) => {}
                }
                // Identical status calls can share the existing StatusOwner
                // observation. Let them reach that owner through an idle generic
                // lane, but never queue behind evidence to do so.
            }
            match lane.clone().try_acquire_owned() {
                Ok(permit) => return Ok(BridgePermit::Evidence(permit)),
                Err(tokio::sync::TryAcquireError::Closed) => {
                    return Err(AdmissionFailure::Closed);
                }
                Err(tokio::sync::TryAcquireError::NoPermits) if status => {
                    // Status already owns a separate cold-setup and observation
                    // deadline. Do not add a generic wait ahead of that budget.
                    return Err(AdmissionFailure::Busy);
                }
                Err(tokio::sync::TryAcquireError::NoPermits) => {}
            }
            let _waiting = self.waiting.clone().try_acquire_owned().map_err(|error| {
                if error == tokio::sync::TryAcquireError::Closed {
                    AdmissionFailure::Closed
                } else {
                    AdmissionFailure::Busy
                }
            })?;
            // Tokio's semaphore preserves FIFO order. The separate waiting
            // permit bounds retained handlers, independently of executing work.
            match tokio::time::timeout_at(expires, lane.clone().acquire_owned()).await {
                Ok(Ok(permit)) if tokio::time::Instant::now() < expires => {
                    Ok(BridgePermit::Evidence(permit))
                }
                Ok(Ok(_)) => Err(AdmissionFailure::Busy),
                Ok(Err(_)) => Err(AdmissionFailure::Closed),
                Err(_) => Err(AdmissionFailure::Busy),
            }
        };
        tokio::select! {
            biased;
            _ = canceled => Err(AdmissionFailure::Canceled),
            result = admitted => result,
        }
    }
}

fn admission_failure(error: AdmissionFailure) -> CallToolResult {
    let busy = error == AdmissionFailure::Busy;
    let detail = serde_json::json!({"error":{
        "code":if error == AdmissionFailure::Canceled {"request_canceled"}else{"direct_query_admission"},
        "reason":if busy {"busy"}else if error == AdmissionFailure::Canceled {"canceled"}else{"draining"},
        "query_submitted":false,"retryable":busy,
        "retry_after_ms":if busy {Some(25)}else{None},
        "fresh_brokerage_required":error == AdmissionFailure::Closed
    }});
    CallToolResult::error(vec![ContentBlock::text(detail.to_string())])
}

impl ServerHandler for Bridge {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new(
                "airk",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(self.backend.guidance())
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.backend
            .catalog()
            .into_iter()
            .find(|tool| tool.name == name)
    }
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: self.backend.catalog(),
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let query_concurrency = self.backend.query_concurrency();
        self.admission.calls.configure(query_concurrency);
        let _permit = match self
            .admission
            .acquire(
                &request.name,
                context.ct.cancelled(),
                tokio::time::Instant::now() + Duration::from_secs(2),
            )
            .await
        {
            Ok(permit) => permit,
            Err(error) => return Ok(admission_failure(error).into()),
        };
        let mut arguments = request.arguments.unwrap_or_default();
        if request.name == "airk_help" && self.backend.local_help() {
            return Ok(crate::help::call(serde_json::Value::Object(arguments)).into());
        }
        let client = context
            .client_info()
            .map_or(airk_core::usage::ClientFamily::Unknown, |info| {
                airk_core::usage::ClientFamily::from_label(&info.name)
            });
        self.backend.client(client);
        if self.backend.uses_response_mode(&request.name) {
            arguments.entry("response_mode").or_insert_with(|| {
                serde_json::to_value(self.response_mode).expect("scalar display mode")
            });
        }
        let result = call_at_capacity(
            self.backend.as_ref(),
            request.name.to_string(),
            arguments,
            query_concurrency,
        )
        .await;
        Ok(result.unwrap_or_else(|error| {
            if std::env::var_os("AIRK_PROCESS_DIAGNOSTICS").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                // Explicit local/CI diagnostics only: keep the bounded cause on
                // stderr without changing the public reply or retry authority.
                let detail: String = format!("{error:#}").chars().take(4096).collect();
                eprintln!("AIRK direct project connection failed: {detail}");
            }
            CallToolResult::error(vec![ContentBlock::text("AIRK direct project connection failed. Retry through fresh brokerage with the same revision selector; use the service console for indexing or repair.")])
        }).into())
    }
}

/// One-shot CLI reads use the same response observations as the MCP bridge.
/// The backend owns native channel/worker admission and read retries.
pub async fn call_observed(
    backend: &dyn RemoteTools,
    tool: String,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> anyhow::Result<serde_json::Value> {
    let limit = backend.query_concurrency();
    Ok(serde_json::to_value(
        call_at_capacity(backend, tool, arguments, limit).await?,
    )?)
}

async fn call_at_capacity(
    backend: &dyn RemoteTools,
    tool: String,
    arguments: serde_json::Map<String, serde_json::Value>,
    limit: usize,
) -> anyhow::Result<CallToolResult> {
    let value = backend.call(tool.clone(), arguments).await?;
    let mut result = serde_json::from_value::<CallToolResult>(value)?;
    report_query_concurrency(&mut result, &tool, limit);
    Ok(result)
}

/// Reuse the admission observation even when the backend returns cached status.
/// This describes configured capacity, not free slots or guaranteed admission.
fn report_query_concurrency(result: &mut CallToolResult, tool: &str, limit: usize) {
    if result.is_error == Some(true) {
        return;
    }
    if !matches!(
        tool,
        "parse" | "airk_parse" | "status" | "airk_status" | "diagnostics" | "airk_diagnostics"
    ) {
        return;
    }
    // Native worker replies can carry JSON text without structured content.
    // Unified routing adds structured content, while direct CLI/legacy calls do not.
    if result.structured_content.is_none() {
        result.structured_content = result
            .content
            .first()
            .and_then(|content| content.as_text())
            .and_then(|text| serde_json::from_str(&text.text).ok());
    }
    if matches!(tool, "parse" | "airk_parse") {
        if let Some(value) = result.structured_content.clone() {
            match report_plan_query_concurrency(value, limit) {
                Ok(value) => {
                    result.content = vec![ContentBlock::text(value.to_string())];
                    result.structured_content = Some(value);
                }
                Err(error) => {
                    *result = crate::response::error_result(&error);
                }
            }
        }
        return;
    }
    if !matches!(
        tool,
        "status" | "airk_status" | "diagnostics" | "airk_diagnostics"
    ) {
        return;
    }
    if let Some(body) = result
        .structured_content
        .as_mut()
        .and_then(|v| v.as_object_mut())
    {
        body.insert("query_concurrency".into(), limit.into());
        result.content = vec![ContentBlock::text(
            serde_json::to_string(body).expect("JSON status body"),
        )];
    }
}

fn report_plan_query_concurrency(
    value: serde_json::Value,
    limit: usize,
) -> anyhow::Result<serde_json::Value> {
    map_plan_results(value, |tool, body| {
        if matches!(tool, "status" | "diagnostics") {
            body.insert("query_concurrency".into(), limit.into());
        }
    })
}

pub(crate) fn map_plan_results(
    value: serde_json::Value,
    mut update: impl FnMut(&str, &mut serde_json::Map<String, serde_json::Value>),
) -> anyhow::Result<serde_json::Value> {
    use airk_core::workbench::ResultEnvelope;
    let mut envelope: ResultEnvelope = serde_json::from_value(value)?;
    for report in &mut envelope.steps {
        if report.status != "complete" {
            continue;
        }
        let Some(body) = report.result.as_mut().and_then(|v| v.as_object_mut()) else {
            continue;
        };
        let old_bytes = serde_json::to_vec(body)?.len();
        update(&report.tool, body);
        let new_bytes = serde_json::to_vec(body)?.len();
        let payload_bytes = envelope.payload_bytes.saturating_sub(old_bytes) + new_bytes;
        if payload_bytes > envelope.plan.budgets.max_bytes {
            envelope.payload_bytes = envelope.payload_bytes.saturating_sub(old_bytes);
            report.result = None;
            report.status = "failed".into();
            report.error = Some("payload budget exceeded".into());
            envelope.truncated = true;
            envelope.completeness.push(format!(
                "{}: materialized payload exceeds budget",
                report.id
            ));
            if envelope.status != "publication_changed" {
                envelope.status = "partial".into();
            }
        } else {
            envelope.payload_bytes = payload_bytes;
        }
    }
    if envelope.status == "partial" && !envelope.steps.iter().any(|s| s.status == "complete") {
        envelope.status = "failed".into();
    }
    let value = serde_json::to_value(airk_kernel::workbench::bounded_result(envelope)?)?;
    airk_core::verification::validate_projection(&value)?;
    Ok(value)
}

pub async fn serve(backend: Arc<dyn RemoteTools>) -> anyhow::Result<()> {
    let close = backend.clone();
    let served = serve_with_input_lifecycle(backend.clone(), Arc::new(move || close.close())).await;
    let cleanup = backend.shutdown().await;
    match (served, cleanup) {
        (Err(error), Err(cleanup)) => Err(error.context(format!(
            "owned observation cleanup also failed: {cleanup:#}"
        ))),
        (Err(error), _) => Err(error),
        (_, result) => result,
    }
}

/// The callback must be synchronous, nonblocking and infallible. It closes
/// caller-owned admission at actual EOF/error, before the SDK drains handlers.
pub async fn serve_with_input_lifecycle(
    backend: Arc<dyn RemoteTools>,
    closed: Arc<dyn Fn() + Send + Sync>,
) -> anyhow::Result<()> {
    serve_io_with_input_lifecycle(backend, tokio::io::stdin(), tokio::io::stdout(), closed).await
}

/// Same bounded SDK transport, with injectable IO for real EOF/cancellation tests.
pub async fn serve_io_with_input_lifecycle<R, W>(
    backend: Arc<dyn RemoteTools>,
    input: R,
    output: W,
    closed: Arc<dyn Fn() + Send + Sync>,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let admission = Arc::new(Admission::default());
    let close_admission = admission.clone();
    let signal = Arc::new(InputClosed {
        callback: Arc::new(move || {
            close_admission.close();
            closed();
        }),
        fired: std::sync::atomic::AtomicBool::new(false),
    });
    let observed = signal.clone();
    InputOwner {
        closed: signal,
        future: Box::pin(async move {
            let bridge = Bridge {
                backend,
                admission,
                response_mode: crate::response::ResponseMode::from_environment(),
            };
            rmcp::service::serve_directly(
                crate::lifecycle::StdioLifecycle::new(bridge),
                (
                    ObservedRead {
                        inner: BoundedRead {
                            inner: input,
                            line_bytes: 0,
                        },
                        closed: observed,
                    },
                    output,
                ),
                None,
            )
            .waiting()
            .await?;
            Ok(())
        }),
    }
    .await
}

struct InputClosed {
    callback: Arc<dyn Fn() + Send + Sync>,
    fired: std::sync::atomic::AtomicBool,
}
impl InputClosed {
    fn fire(&self) {
        if !self.fired.swap(true, std::sync::atomic::Ordering::AcqRel) {
            (self.callback)();
        }
    }
}
// Closing the outer serving future also closes admission BEFORE dropping its
// SDK future (which can start a two-second cancellation drain).
struct InputOwner<F> {
    future: Pin<Box<F>>,
    closed: Arc<InputClosed>,
}
impl<F: Future> Future for InputOwner<F> {
    type Output = F::Output;
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.future.as_mut().poll(cx)
    }
}
impl<F> Drop for InputOwner<F> {
    fn drop(&mut self) {
        self.closed.fire();
    }
}
struct ObservedRead<R> {
    inner: R,
    closed: Arc<InputClosed>,
}
impl<R> Drop for ObservedRead<R> {
    fn drop(&mut self) {
        self.closed.fire();
    }
}
impl<R: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for ObservedRead<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        output: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let remaining = output.remaining();
        let before = output.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, output);
        if matches!(&result, std::task::Poll::Ready(Err(_)))
            || matches!(&result, std::task::Poll::Ready(Ok(())))
                && remaining > 0
                && output.filled().len() == before
        {
            self.closed.fire();
        }
        result
    }
}

/// Bound stdio lines before rmcp's read_until can allocate an oversized request.
struct BoundedRead<R> {
    inner: R,
    line_bytes: usize,
}
impl<R: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for BoundedRead<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        output: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let mut bytes = [0; 8192];
        let count = output.remaining().min(bytes.len());
        let mut buffer = tokio::io::ReadBuf::new(&mut bytes[..count]);
        match Pin::new(&mut self.inner).poll_read(cx, &mut buffer) {
            std::task::Poll::Ready(Ok(())) => {
                for &byte in buffer.filled() {
                    self.line_bytes += 1;
                    if self.line_bytes > airk_core::service::MAX_FRAME_BYTES {
                        return std::task::Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "MCP request exceeds frame bound",
                        )));
                    }
                    if byte == b'\n' {
                        self.line_bytes = 0;
                    }
                }
                output.put_slice(buffer.filled());
                std::task::Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::ServiceExt;
    use tokio::io::{AsyncRead, AsyncReadExt};

    fn plan_fixture() -> serde_json::Value {
        serde_json::json!({
            "schema":"airk.workbench.result/1", "collection":"fixture", "publication":null,
            "status":"complete", "executed":true,
            "plan":airk_core::workbench::Plan::default(),
            "requires_input":[], "evidence":[], "warnings":[], "completeness":[],
            "truncated":false, "calls":1, "payload_bytes":14,
            "steps":[{"id":"query","tool":"status","arguments":{},"elapsed_ms":6,
                "status":"complete","error":null,"result":{"files":1414}}]
        })
    }

    #[tokio::test]
    async fn sdk_preserves_typed_refusal_when_late_observations_remove_checked_evidence()
    -> anyhow::Result<()> {
        struct CheckedPlan;
        impl RemoteTools for CheckedPlan {
            fn call(
                &self,
                _: String,
                _: serde_json::Map<String, serde_json::Value>,
            ) -> RemoteFuture<'_> {
                Box::pin(async {
                    let mut value = plan_fixture();
                    value["plan"]["budgets"]["max_bytes"] = 14.into();
                    // Isolate the projection fence: this fixture's checked
                    // location exists until the transport adds observations.
                    let fixtures: serde_json::Value = serde_json::from_str(include_str!(
                        "../../../docs/research/contracts-v190/fixtures.json"
                    ))?;
                    let mut receipt = fixtures["receipt_cases"][0]["value"].clone();
                    for reference in receipt["references"].as_array_mut().unwrap() {
                        reference["result_pointer"] = "/steps/0/result".into();
                    }
                    value["source_verification"] = receipt;
                    airk_core::verification::validate_projection(&value)?;
                    Ok(crate::unified::success(value))
                })
            }
        }
        for unified in [false, true] {
            let backend: Arc<dyn RemoteTools> = if unified {
                let binding = "ab".repeat(32);
                Arc::new(crate::unified::Unified::new(
                    vec![crate::unified::Project {
                        binding: binding.clone(),
                        name: "fixture".into(),
                        collection: "fixture".into(),
                        backend: Arc::new(CheckedPlan),
                    }],
                    Some(binding),
                    None,
                )?)
            } else {
                Arc::new(CheckedPlan)
            };
            let (client_io, server_io) = tokio::io::duplex(65536);
            let bridge = Bridge {
                backend,
                admission: Arc::new(Admission::default()),
                response_mode: crate::response::ResponseMode::Full,
            };
            let (mut client, mut server) = tokio::try_join!(
                async { Ok::<_, anyhow::Error>(().serve(client_io).await?) },
                async { Ok::<_, anyhow::Error>(bridge.serve(server_io).await?) }
            )?;
            for alias in ["parse", "airk_parse"] {
                let result = client.call_tool(CallToolRequestParams::new(alias)).await?;
                assert_eq!(result.is_error, Some(true), "{result:?}");
                let value: serde_json::Value = serde_json::from_str(
                    &result.content[0].as_text().unwrap().text,
                )
                .map_err(|error| anyhow::anyhow!("unified={unified}: {error}; {result:?}"))?;
                assert_eq!(value["error"]["code"], "verification_projection_changed");
                assert_eq!(value["error"]["retryable"], false);
                assert!(value.get("source_verification").is_none());
            }
            client.close().await?;
            server.close().await?;
        }
        Ok(())
    }

    #[test]
    fn plan_capacity_preserves_step_contracts_and_bounds() -> anyhow::Result<()> {
        for operation in ["status", "diagnostics"] {
            for alias in ["parse", "airk_parse"] {
                let mut original = plan_fixture();
                original["steps"][0]["tool"] = operation.into();
                for limit in [4, 8, 16, 32, 64] {
                    let mut reply: CallToolResult =
                        serde_json::from_value(crate::unified::success(original.clone()))?;
                    if alias == "airk_parse" {
                        reply.structured_content = None;
                    }
                    report_query_concurrency(&mut reply, alias, limit);
                    let body = reply.structured_content.as_ref().unwrap();
                    assert_eq!(body["steps"][0]["result"]["query_concurrency"], limit);
                    assert_eq!(body["steps"][0]["elapsed_ms"], 6);
                    assert_eq!(body["plan"], original["plan"]);
                    assert_eq!(
                        body["payload_bytes"],
                        serde_json::to_vec(&body["steps"][0]["result"])?.len()
                    );
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(
                            &reply.content[0].as_text().unwrap().text
                        )?,
                        *body
                    );
                }
            }
        }
        let mut failed = plan_fixture();
        failed["status"] = "failed".into();
        failed["steps"][0]["status"] = "failed".into();
        assert_eq!(report_plan_query_concurrency(failed.clone(), 16)?, failed);
        let mut search = plan_fixture();
        search["steps"][0]["tool"] = "search".into();
        assert_eq!(report_plan_query_concurrency(search.clone(), 16)?, search);
        let mut tight = plan_fixture();
        tight["plan"]["budgets"]["max_bytes"] = 14.into();
        let bounded = report_plan_query_concurrency(tight, 16)?;
        assert_eq!(bounded["status"], "failed");
        assert_eq!(bounded["truncated"], true);
        assert!(bounded["steps"][0]["result"].is_null());
        assert_eq!(bounded["payload_bytes"], 0);
        let mut mixed = plan_fixture();
        let mut small = mixed["steps"][0].clone();
        small["id"] = "small".into();
        mixed["steps"][0]["result"]["padding"] = "x".repeat(100).into();
        mixed["steps"].as_array_mut().unwrap().push(small);
        let raw_bytes: usize = mixed["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| serde_json::to_vec(&step["result"]).unwrap().len())
            .sum();
        mixed["payload_bytes"] = raw_bytes.into();
        mixed["plan"]["budgets"]["max_bytes"] = raw_bytes.into();
        let bounded = report_plan_query_concurrency(mixed, 16)?;
        assert_eq!(bounded["status"], "partial");
        assert!(bounded["steps"][0]["result"].is_null());
        assert_eq!(bounded["steps"][1]["status"], "complete");
        assert_eq!(bounded["steps"][1]["result"]["query_concurrency"], 16);
        assert_eq!(
            bounded["payload_bytes"],
            serde_json::to_vec(&bounded["steps"][1]["result"])?.len()
        );
        let mut clipped = plan_fixture();
        clipped["steps"][0]["result"]["padding"] = "x".repeat(100).into();
        clipped["payload_bytes"] = serde_json::to_vec(&clipped["steps"][0]["result"])?
            .len()
            .into();
        clipped["plan"]["output"]["max_bytes"] = serde_json::to_vec(&clipped)?.len().into();
        let bounded = report_plan_query_concurrency(clipped, 16)?;
        assert_eq!(bounded["truncated"], true);
        assert!(bounded["steps"][0]["result"].is_null());
        assert!(
            serde_json::to_vec(&bounded)?.len()
                <= bounded["plan"]["output"]["max_bytes"].as_u64().unwrap() as usize
        );
        Ok(())
    }

    fn pending<T>(future: Pin<&mut impl Future<Output = T>>) {
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(future.poll(&mut context).is_pending());
    }
    fn deadline() -> tokio::time::Instant {
        tokio::time::Instant::now() + Duration::from_secs(1)
    }

    #[tokio::test]
    async fn configured_evidence_capacity_keeps_independent_status_and_drains_on_decrease() {
        for limit in [4, 8, 16, 32, 64] {
            let admission = Admission::default();
            admission.calls.configure(limit);
            let mut held = Vec::new();
            for _ in 0..limit {
                held.push(
                    admission
                        .acquire("search", std::future::pending(), deadline())
                        .await
                        .unwrap(),
                );
            }
            let status = admission
                .acquire("status", std::future::pending(), deadline())
                .await
                .unwrap();
            assert_eq!(admission.calls.available_permits(), 0);
            admission.calls.configure(4);
            let mut queued =
                Box::pin(admission.acquire("search", std::future::pending(), deadline()));
            pending(queued.as_mut());
            while held.len() >= 4 {
                drop(held.pop());
            }
            assert!(queued.await.is_ok());
            drop(status);
            drop(held);
        }
    }

    #[tokio::test]
    async fn admission_reserves_status_and_bounds_the_non_status_queue() {
        let admission = Admission {
            calls: crate::query_capacity::QueryCapacity::new(3),
            status: Arc::new(tokio::sync::Semaphore::new(1)),
            waiting: Arc::new(tokio::sync::Semaphore::new(8)),
        };
        let mut executing = Vec::new();
        for name in ["airk_search", "draft_search", "diagnostics"] {
            executing.push(
                admission
                    .acquire(name, std::future::pending(), deadline())
                    .await
                    .unwrap(),
            );
        }
        let mut waiting: Vec<_> = (0..8)
            .map(|_| Box::pin(admission.acquire("search", std::future::pending(), deadline())))
            .collect();
        for call in &mut waiting {
            pending(call.as_mut());
        }
        assert_eq!(admission.waiting.available_permits(), 0);
        assert_eq!(
            admission
                .acquire("help", std::future::pending(), deadline())
                .await
                .unwrap_err(),
            AdmissionFailure::Busy
        );
        // A whole queue of long evidence/UI calls cannot consume status's lane.
        let status = admission
            .acquire("status", std::future::pending(), deadline())
            .await
            .unwrap();
        assert_eq!(
            admission
                .acquire("airk_status", std::future::pending(), deadline())
                .await
                .unwrap_err(),
            AdmissionFailure::Busy
        );
        drop(status);
        assert!(
            admission
                .acquire("airk_status", std::future::pending(), deadline())
                .await
                .is_ok()
        );
        drop(waiting.pop());
        assert_eq!(admission.waiting.available_permits(), 1);
        drop(executing.pop());
        let first = waiting.remove(0).await.unwrap();
        drop(first);
        for call in waiting {
            drop(call.await.unwrap());
        }
        assert_eq!(admission.waiting.available_permits(), 8);
    }

    #[tokio::test]
    async fn idle_generic_lanes_allow_concurrent_status_calls_to_reach_shared_owner() {
        let admission = Admission {
            calls: crate::query_capacity::QueryCapacity::new(3),
            status: Arc::new(tokio::sync::Semaphore::new(1)),
            waiting: Arc::new(tokio::sync::Semaphore::new(8)),
        };
        let mut held = Vec::new();
        for name in ["status", "airk_status", "status", "airk_status"] {
            held.push(
                admission
                    .acquire(name, std::future::pending(), deadline())
                    .await
                    .unwrap(),
            );
        }
        assert_eq!(admission.status.available_permits(), 0);
        assert_eq!(admission.calls.available_permits(), 0);
        assert_eq!(admission.waiting.available_permits(), 8);
        assert_eq!(
            admission
                .acquire("status", std::future::pending(), deadline())
                .await
                .unwrap_err(),
            AdmissionFailure::Busy
        );
        drop(held);
        assert_eq!(admission.calls.available_permits(), 3);
    }

    #[tokio::test]
    async fn expired_canceled_and_closed_admission_never_take_execution_slots() {
        let admission = Admission {
            calls: crate::query_capacity::QueryCapacity::new(3),
            status: Arc::new(tokio::sync::Semaphore::new(1)),
            waiting: Arc::new(tokio::sync::Semaphore::new(8)),
        };
        assert_eq!(
            admission
                .acquire(
                    "search",
                    std::future::pending(),
                    tokio::time::Instant::now()
                )
                .await
                .unwrap_err(),
            AdmissionFailure::Busy
        );
        assert_eq!(
            admission
                .acquire("search", std::future::ready(()), deadline())
                .await
                .unwrap_err(),
            AdmissionFailure::Canceled
        );
        assert_eq!(admission.calls.available_permits(), 3);
        let executing: Vec<_> = {
            let mut held = Vec::new();
            for _ in 0..3 {
                held.push(
                    admission
                        .acquire("search", std::future::pending(), deadline())
                        .await
                        .unwrap(),
                );
            }
            held
        };
        let (cancel, canceled) = tokio::sync::oneshot::channel();
        let mut cancel_wait = Box::pin(admission.acquire(
            "search",
            async {
                let _ = canceled.await;
            },
            deadline(),
        ));
        pending(cancel_wait.as_mut());
        assert_eq!(admission.waiting.available_permits(), 7);
        cancel.send(()).unwrap();
        assert_eq!(cancel_wait.await.unwrap_err(), AdmissionFailure::Canceled);
        assert_eq!(admission.waiting.available_permits(), 8);
        let mut waiting = Box::pin(admission.acquire("search", std::future::pending(), deadline()));
        pending(waiting.as_mut());
        admission.close();
        assert_eq!(waiting.await.unwrap_err(), AdmissionFailure::Closed);
        assert_eq!(
            executing.len(),
            3,
            "closing admission does not release accepted work"
        );
        assert_eq!(
            admission
                .acquire("status", std::future::pending(), deadline())
                .await
                .unwrap_err(),
            AdmissionFailure::Closed
        );
    }

    #[test]
    fn admission_busy_receipt_is_typed_and_unsubmitted() {
        let reply = admission_failure(AdmissionFailure::Busy);
        let detail: serde_json::Value =
            serde_json::from_str(&reply.content[0].as_text().unwrap().text).unwrap();
        assert_eq!(detail["error"]["code"], "direct_query_admission");
        assert_eq!(detail["error"]["query_submitted"], false);
        assert_eq!(detail["error"]["retryable"], true);
        assert_eq!(detail["error"]["fresh_brokerage_required"], false);
        for failure in [AdmissionFailure::Canceled, AdmissionFailure::Closed] {
            let reply = admission_failure(failure);
            let detail: serde_json::Value =
                serde_json::from_str(&reply.content[0].as_text().unwrap().text).unwrap();
            assert_eq!(detail["error"]["query_submitted"], false);
            assert_eq!(detail["error"]["retryable"], false);
        }
    }

    #[tokio::test]
    async fn input_close_observes_real_eof_once_and_ignores_zero_capacity() -> anyhow::Result<()> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let closed = Arc::new(InputClosed {
            callback: Arc::new(move || {
                observed.fetch_add(1, Ordering::SeqCst);
            }),
            fired: std::sync::atomic::AtomicBool::new(false),
        });
        let mut read = ObservedRead {
            inner: BoundedRead {
                inner: &b""[..],
                line_bytes: 0,
            },
            closed: closed.clone(),
        };
        let mut empty = [];
        let mut buffer = tokio::io::ReadBuf::new(&mut empty);
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut read).poll_read(cx, &mut buffer)).await?;
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let mut byte = [0];
        assert_eq!(read.read(&mut byte).await?, 0);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        drop(read);
        closed.fire();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn input_drop_closes_admission_without_eof() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let read = ObservedRead {
            inner: BoundedRead {
                inner: &b"unread"[..],
                line_bytes: 0,
            },
            closed: Arc::new(InputClosed {
                callback: Arc::new(move || {
                    observed.fetch_add(1, Ordering::SeqCst);
                }),
                fired: std::sync::atomic::AtomicBool::new(false),
            }),
        };
        drop(read);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn actual_sdk_eof_closes_before_blocked_handler_drain() -> anyhow::Result<()> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        struct Blocked {
            began: Arc<tokio::sync::Notify>,
            release: Arc<tokio::sync::Notify>,
        }
        impl RemoteTools for Blocked {
            fn local_help(&self) -> bool {
                false
            }
            fn call(
                &self,
                _: String,
                _: serde_json::Map<String, serde_json::Value>,
            ) -> RemoteFuture<'_> {
                Box::pin(async move {
                    self.began.notify_one();
                    self.release.notified().await;
                    Ok(serde_json::to_value(CallToolResult::success(vec![]))?)
                })
            }
        }
        let began = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let closed = Arc::new(tokio::sync::Notify::new());
        let observed = closed.clone();
        let (client, server) = tokio::io::duplex(65536);
        let (input, output) = tokio::io::split(server);
        let task = tokio::spawn(serve_io_with_input_lifecycle(
            Arc::new(Blocked {
                began: began.clone(),
                release: release.clone(),
            }),
            input,
            output,
            Arc::new(move || observed.notify_one()),
        ));
        let (input, mut output) = tokio::io::split(client);
        let mut lines = tokio::io::BufReader::new(input).lines();
        output.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"fixture\",\"version\":\"1\"}}}\n").await?;
        let initialized = lines.next_line().await?.expect("initialize response");
        assert!(serde_json::from_str::<serde_json::Value>(&initialized)?["result"].is_object());
        output.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"airk_search\",\"arguments\":{}}}\n").await?;
        began.notified().await;
        output.shutdown().await?;
        tokio::time::timeout(std::time::Duration::from_secs(1), closed.notified()).await?;
        assert!(
            !task.is_finished(),
            "EOF must close admission before the SDK finishes draining"
        );
        release.notify_one();
        task.await??;
        Ok(())
    }

    #[tokio::test]
    async fn terminal_read_error_closes_admission_and_preserves_error() {
        struct Failed;
        impl tokio::io::AsyncRead for Failed {
            fn poll_read(
                self: Pin<&mut Self>,
                _: &mut std::task::Context<'_>,
                _: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::task::Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "fixture read error",
                )))
            }
        }
        let fired = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = fired.clone();
        let mut input = ObservedRead {
            inner: Failed,
            closed: Arc::new(InputClosed {
                callback: Arc::new(move || {
                    observed.store(true, std::sync::atomic::Ordering::Release);
                }),
                fired: std::sync::atomic::AtomicBool::new(false),
            }),
        };
        let error = input.read(&mut [0]).await.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        assert_eq!(error.to_string(), "fixture read error");
        assert!(fired.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn abandoned_serving_future_closes_before_inner_dispatcher_drop() {
        struct Inner(Arc<std::sync::atomic::AtomicBool>);
        impl Future for Inner {
            type Output = ();
            fn poll(self: Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<()> {
                std::task::Poll::Pending
            }
        }
        impl Drop for Inner {
            fn drop(&mut self) {
                assert!(self.0.load(std::sync::atomic::Ordering::Acquire));
            }
        }
        let fired = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = fired.clone();
        drop(InputOwner {
            future: Box::pin(Inner(fired)),
            closed: Arc::new(InputClosed {
                callback: Arc::new(move || {
                    observed.store(true, std::sync::atomic::Ordering::Release);
                }),
                fired: std::sync::atomic::AtomicBool::new(false),
            }),
        });
    }

    #[tokio::test]
    async fn bridge_reports_live_query_concurrency_without_an_extra_policy_read()
    -> anyhow::Result<()> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Cached {
            limit: AtomicUsize,
            reads: AtomicUsize,
        }
        impl RemoteTools for Cached {
            fn query_concurrency(&self) -> usize {
                self.reads.fetch_add(1, Ordering::SeqCst);
                self.limit.load(Ordering::SeqCst)
            }
            fn catalog(&self) -> Vec<Tool> {
                [
                    "status",
                    "airk_status",
                    "diagnostics",
                    "airk_diagnostics",
                    "airk_search",
                    "parse",
                    "airk_parse",
                ]
                .into_iter()
                .map(|name| {
                    let mut tool = crate::local::tools()
                        .into_iter()
                        .find(|t| t.name == "airk_status")
                        .unwrap();
                    tool.name = name.to_string().into();
                    tool
                })
                .collect()
            }
            fn call(
                &self,
                tool: String,
                arguments: serde_json::Map<String, serde_json::Value>,
            ) -> RemoteFuture<'_> {
                Box::pin(async move {
                    if arguments.get("include_status") == Some(&serde_json::Value::Bool(true)) {
                        return Ok(crate::unified::failure(
                            "fixture_failure",
                            "Retain error receipt",
                        ));
                    }
                    if matches!(tool.as_str(), "parse" | "airk_parse") {
                        return Ok(crate::unified::success(plan_fixture()));
                    }
                    Ok(crate::unified::success(serde_json::json!({
                        "cached": true, "observed_unix_ms": 123, "cache_age_ms": 456,
                        "publication": {"token": "fixture-publication"}, "query_concurrency": 8
                    })))
                })
            }
        }
        let backend = Arc::new(Cached {
            limit: AtomicUsize::new(16),
            reads: AtomicUsize::new(0),
        });
        let (client_io, server_io) = tokio::io::duplex(65536);
        let bridge = Bridge {
            backend: backend.clone(),
            admission: Arc::new(Admission::default()),
            response_mode: crate::response::ResponseMode::Terse,
        };
        let (mut client, mut server) = tokio::try_join!(
            async { Ok::<_, anyhow::Error>(().serve(client_io).await?) },
            async { Ok::<_, anyhow::Error>(bridge.serve(server_io).await?) }
        )?;
        for (request, (tool, mode, limit)) in [
            ("status", "terse", 16),
            ("airk_status", "full", 32),
            ("diagnostics", "terse", 64),
            ("airk_diagnostics", "full", 4),
            ("status", "full", 8),
            ("airk_search", "terse", 16),
            ("parse", "terse", 32),
            ("airk_parse", "full", 64),
        ]
        .into_iter()
        .enumerate()
        {
            backend.limit.store(limit, Ordering::SeqCst);
            let reply = client
                .call_tool(
                    CallToolRequestParams::new(tool).with_arguments(
                        serde_json::json!({"response_mode": mode})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await?;
            let body = reply.structured_content.as_ref().unwrap();
            if matches!(tool, "parse" | "airk_parse") {
                assert_eq!(body["steps"][0]["result"]["query_concurrency"], limit);
                assert_eq!(backend.reads.load(Ordering::SeqCst), request + 1);
                continue;
            }
            assert_eq!(
                body["query_concurrency"],
                if tool == "airk_search" { 8 } else { limit }
            );
            assert_eq!(body["cached"], true);
            assert_eq!(body["observed_unix_ms"], 123);
            assert_eq!(body["cache_age_ms"], 456);
            assert_eq!(body["publication"]["token"], "fixture-publication");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(
                    &reply.content[0].as_text().unwrap().text
                )?,
                *body
            );
            assert_eq!(backend.reads.load(Ordering::SeqCst), request + 1);
        }
        let reply = client
            .call_tool(
                CallToolRequestParams::new("status").with_arguments(
                    serde_json::json!({"include_status":true})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await?;
        assert_eq!(reply.is_error, Some(true));
        assert_eq!(
            reply.structured_content.as_ref().unwrap()["error"]["code"],
            "fixture_failure"
        );
        assert!(
            reply
                .structured_content
                .as_ref()
                .unwrap()
                .get("query_concurrency")
                .is_none()
        );
        assert_eq!(backend.reads.load(Ordering::SeqCst), 9);
        client.close().await?;
        server.close().await?;
        Ok(())
    }

    struct Echo;
    impl RemoteTools for Echo {
        fn call(
            &self,
            _tool: String,
            arguments: serde_json::Map<String, serde_json::Value>,
        ) -> RemoteFuture<'_> {
            Box::pin(async move {
                Ok(serde_json::to_value(CallToolResult::success(vec![
                    ContentBlock::text(serde_json::to_string(&arguments)?),
                ]))?)
            })
        }
    }

    #[tokio::test]
    async fn bridge_forwards_panel_default_and_preserves_individual_override() -> anyhow::Result<()>
    {
        for default in [
            crate::response::ResponseMode::Full,
            crate::response::ResponseMode::Terse,
        ] {
            let (client_io, server_io) = tokio::io::duplex(65536);
            let bridge = Bridge {
                backend: Arc::new(Echo),
                admission: Arc::new(Admission::default()),
                response_mode: default,
            };
            let (client, server) = tokio::try_join!(
                async { Ok::<_, anyhow::Error>(().serve(client_io).await?) },
                async { Ok::<_, anyhow::Error>(bridge.serve(server_io).await?) }
            )?;
            let mut client = client;
            let mut server = server;
            for mode in [None, Some("full"), Some("terse")] {
                let mut args = serde_json::json!({"query":"fixture","include_status":true})
                    .as_object()
                    .unwrap()
                    .clone();
                if let Some(mode) = mode {
                    args.insert("response_mode".into(), mode.into());
                }
                let result = client
                    .call_tool(CallToolRequestParams::new("airk_search").with_arguments(args))
                    .await?;
                let value: serde_json::Value =
                    serde_json::from_str(&result.content[0].as_text().unwrap().text)?;
                assert_eq!(
                    value["response_mode"],
                    mode.map(serde_json::Value::from)
                        .unwrap_or(serde_json::to_value(default)?)
                );
                assert_eq!(value["include_status"], true);
            }
            let help = client
                .call_tool(
                    CallToolRequestParams::new("airk_help").with_arguments(Default::default()),
                )
                .await?;
            let help: serde_json::Value =
                serde_json::from_str(&help.content[0].as_text().unwrap().text)?;
            assert!(
                help["topics"].as_array().unwrap().len() >= 25,
                "help must be served locally rather than forwarded to the echo backend"
            );
            client.close().await?;
            server.close().await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn request_line_bound_applies_across_reads_and_resets_only_at_newline()
    -> anyhow::Result<()> {
        let bytes = vec![b'x'; airk_core::service::MAX_FRAME_BYTES + 1];
        let mut read = BoundedRead {
            inner: bytes.as_slice(),
            line_bytes: 0,
        };
        let mut captured = Vec::new();
        assert_eq!(
            read.read_to_end(&mut captured).await.unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        assert!(captured.len() <= airk_core::service::MAX_FRAME_BYTES);
        let mut bytes = vec![b'x'; airk_core::service::MAX_FRAME_BYTES - 1];
        bytes.push(b'\n');
        bytes.extend_from_within(..);
        let mut read = BoundedRead {
            inner: bytes.as_slice(),
            line_bytes: 0,
        };
        let mut captured = Vec::new();
        read.read_to_end(&mut captured).await?;
        assert_eq!(captured, bytes);
        Ok(())
    }
}
