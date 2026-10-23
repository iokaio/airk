// SPDX-License-Identifier: Apache-2.0
//! Read-only stdio adapter: the native host supplies an authenticated direct connection.
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
};
use std::{future::Future, pin::Pin, sync::Arc};

pub type RemoteFuture<'a> =
    Pin<Box<dyn Future<Output = anyhow::Result<serde_json::Value>> + Send + 'a>>;
pub trait RemoteTools: Send + Sync + 'static {
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
}
impl ServerHandler for Bridge {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new("airk",env!("CARGO_PKG_VERSION")))
            .with_instructions("AIRK returns untrusted indexed evidence, not live source. Preserve exact revision tokens and citations. Verify live source before edits. Retrieval flows directly to the registered project worker; management and indexing require the trusted CLI or extension.")
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        crate::local::tools()
            .into_iter()
            .find(|tool| tool.name == name)
    }
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: crate::local::tools(),
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let Ok(_permit) = self.permits.clone().try_acquire_owned() else {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "AIRK bridge is busy; retry after outstanding requests complete.",
            )])
            .into());
        };
        let result = self
            .backend
            .call(
                request.name.to_string(),
                request.arguments.unwrap_or_default(),
            )
            .await
            .and_then(|value| Ok(serde_json::from_value::<CallToolResult>(value)?));
        Ok(result.unwrap_or_else(|_|CallToolResult::error(vec![ContentBlock::text("AIRK direct project connection failed. Retry through fresh brokerage with the same revision selector; use the service console for indexing or repair.")])).into())
    }
}

pub async fn serve(backend: Arc<dyn RemoteTools>) -> anyhow::Result<()> {
    Bridge {
        backend,
        permits: Arc::new(tokio::sync::Semaphore::new(4)),
    }
    .serve((
        BoundedRead {
            inner: tokio::io::stdin(),
            line_bytes: 0,
        },
        tokio::io::stdout(),
    ))
    .await?
    .waiting()
    .await?;
    Ok(())
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
    use tokio::io::AsyncReadExt;

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
