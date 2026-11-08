// SPDX-License-Identifier: Apache-2.0
//! Bounded SDK IO component contracts. This is not a current runtime transport.
use airk_mcp::remote::{RemoteFuture, RemoteTools, serve_io_with_input_lifecycle};
use serde_json::{Value, json};
use std::{
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

const LIMIT: usize = airk_core::service::MAX_FRAME_BYTES;
const OUTPUT_LIMIT: usize = 1024 * 1024;

struct Inert(Arc<AtomicUsize>);
impl RemoteTools for Inert {
    fn call(&self, _: String, _: serde_json::Map<String, Value>) -> RemoteFuture<'_> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { anyhow::bail!("Framing component must not submit native reads") })
    }
}

struct Chunked {
    bytes: Vec<u8>,
    position: usize,
    chunk: usize,
}
impl AsyncRead for Chunked {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let count = self
            .chunk
            .min(output.remaining())
            .min(self.bytes.len() - self.position);
        output.put_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Poll::Ready(Ok(()))
    }
}

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl AsyncWrite for Capture {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut output = self.0.lock().unwrap();
        if bytes.len() > OUTPUT_LIMIT - output.len() {
            return Poll::Ready(Err(std::io::Error::other(
                "Component output bound exceeded",
            )));
        }
        output.extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

struct Observation {
    result: anyhow::Result<()>,
    bytes: Vec<u8>,
    replies: Vec<Value>,
}
async fn observe(bytes: Vec<u8>, chunk: usize) -> anyhow::Result<Observation> {
    assert!(chunk > 0);
    let calls = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let closed = closes.clone();
    let output = Capture(Arc::new(Mutex::new(Vec::new())));
    let result = tokio::time::timeout(
        Duration::from_secs(15),
        serve_io_with_input_lifecycle(
            Arc::new(Inert(calls.clone())),
            Chunked {
                bytes,
                position: 0,
                chunk,
            },
            output.clone(),
            Arc::new(move || {
                closed.fetch_add(1, Ordering::SeqCst);
            }),
        ),
    )
    .await?;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(closes.load(Ordering::SeqCst), 1);
    let bytes = output.0.lock().unwrap().clone();
    let replies: Vec<Value> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(serde_json::from_slice)
        .collect::<Result<_, _>>()?;
    for reply in &replies {
        assert_eq!(reply["jsonrpc"], "2.0");
    }
    Ok(Observation {
        result,
        bytes,
        replies,
    })
}
fn initialize() -> Vec<u8> {
    serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"framing-component","version":"1"}
    }})).unwrap()
}
fn padded_line(mut json: Vec<u8>) -> Vec<u8> {
    assert!(json.len() < LIMIT);
    json.resize(LIMIT - 1, b' ');
    json.push(b'\n');
    json
}

#[tokio::test]
async fn exact_frame_cap_counts_newline_and_resets_per_frame() -> anyhow::Result<()> {
    for chunk in [1, 8192, LIMIT + 1] {
        let mut bytes = padded_line(initialize());
        bytes.extend(padded_line(
            br#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#.to_vec(),
        ));
        bytes.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"ping\"}\n");
        let observed = observe(bytes, chunk).await?;
        observed.result?;
        for id in [1, 2, 3] {
            assert!(observed.replies.iter().any(|reply| reply["id"] == id));
        }
    }
    Ok(())
}

#[tokio::test]
async fn overflow_is_terminal_atomic_and_content_free() -> anyhow::Result<()> {
    for (newline, chunk) in [(true, 1), (true, 8192), (false, LIMIT + 1)] {
        let mut bytes = initialize();
        bytes.resize(LIMIT, b' ');
        if newline {
            bytes.push(b'\n');
        } else {
            bytes.push(b'x');
        }
        bytes.extend_from_slice(
            b"PRIVATE_REJECTED_PAYLOAD\n{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"ping\"}\n",
        );
        let observed = observe(bytes, chunk).await?;
        let error = observed.result.unwrap_err();
        assert_eq!(error.to_string(), "MCP request exceeds frame bound");
        assert!(!format!("{error:?}").contains("PRIVATE_REJECTED_PAYLOAD"));
        assert!(
            observed.bytes.is_empty(),
            "An oversized first frame must not be dispatched"
        );
    }
    Ok(())
}

#[tokio::test]
async fn eof_without_newline_retains_complete_json_through_exact_cap() -> anyhow::Result<()> {
    for padded in [false, true] {
        let mut bytes = initialize();
        if padded {
            bytes.resize(LIMIT, b' ');
        }
        let observed = observe(bytes, if padded { 8192 } else { 17 }).await?;
        observed.result?;
        assert!(observed.replies.iter().any(|reply| reply["id"] == 1));
    }
    Ok(())
}

#[tokio::test]
async fn malformed_line_is_not_reflected_and_next_ping_uses_sdk_semantics() -> anyhow::Result<()> {
    let observed = observe(
        b"PRIVATE_MALFORMED\n{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"ping\"}\n".to_vec(),
        8192,
    )
    .await?;
    observed.result?;
    assert_eq!(observed.replies.len(), 1);
    assert_eq!(observed.replies[0]["id"], 99);
    assert!(observed.replies[0]["result"].is_object());
    assert!(!String::from_utf8(observed.bytes)?.contains("PRIVATE_MALFORMED"));
    Ok(())
}
