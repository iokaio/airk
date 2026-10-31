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
use std::{future::Future, pin::Pin, sync::Arc};

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
    permits: Arc<tokio::sync::Semaphore>,
    response_mode: crate::response::ResponseMode,
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
        let Ok(_permit) = self.permits.clone().try_acquire_owned() else {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "AIRK bridge is busy; retry after outstanding requests complete.",
            )])
            .into());
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
        let result = self
            .backend
            .call(request.name.to_string(), arguments)
            .await
            .and_then(|value| Ok(serde_json::from_value::<CallToolResult>(value)?));
        Ok(result.unwrap_or_else(|_|CallToolResult::error(vec![ContentBlock::text("AIRK direct project connection failed. Retry through fresh brokerage with the same revision selector; use the service console for indexing or repair.")])).into())
    }
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
    let signal = Arc::new(InputClosed {
        callback: closed,
        fired: std::sync::atomic::AtomicBool::new(false),
    });
    let observed = signal.clone();
    InputOwner {
        closed: signal,
        future: Box::pin(async move {
            let bridge = Bridge {
                backend,
                permits: Arc::new(tokio::sync::Semaphore::new(4)),
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
                permits: Arc::new(tokio::sync::Semaphore::new(4)),
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
