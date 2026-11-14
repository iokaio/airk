// SPDX-License-Identifier: Apache-2.0
//! Original native read custody reaches the ACTUAL outer I1 byte writer.
use airk_mcp::read_attempt::ReadAttemptContext;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::AsyncWrite;

pub(super) struct PolicyWriter<'a, S> {
    pub inner: &'a mut S,
    pub context: &'a ReadAttemptContext,
}
impl<S: AsyncWrite + Unpin> AsyncWrite for PolicyWriter<'_, S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = &mut *self;
        this.context.poll_delivery(&mut || {
            this.context.poll_source_delivery(&mut || {
                if let Err(error) = this.context.check_delivery_live() {
                    return Poll::Ready(Err(error));
                }
                Pin::new(&mut *this.inner).poll_write(cx, bytes)
            })
        })
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = &mut *self;
        this.context
            .poll_delivery(&mut || {
                this.context.poll_source_delivery(&mut || {
                    if let Err(error) = this.context.check_delivery_live() {
                        return Poll::Ready(Err(error));
                    }
                    Pin::new(&mut *this.inner)
                        .poll_flush(cx)
                        .map(|result| result.map(|()| 0))
                })
            })
            .map(|result| result.map(|_| ()))
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // Cleanup may close a revoked exchange; it does not disclose bytes.
        Pin::new(&mut *self.inner).poll_shutdown(cx)
    }
}
