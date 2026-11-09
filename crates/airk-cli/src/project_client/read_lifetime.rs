// SPDX-License-Identifier: Apache-2.0
//! One owned IPC exchange outlives its caller until completion or exact reap.
use super::*;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

pub(super) async fn submit<S>(
    mut stream: S,
    query: Query,
    identity: airk_core::service::WorkerIdentity,
    scope: Option<airk_core::service::ScopedReadRef>,
    context: airk_mcp::read_attempt::ReadAttemptContext,
) -> (Result<protocol::QueryResult>, Option<S>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    // Serialization errors are provably before the first transport write.
    let mut frame = Vec::new();
    if let Err(error) = crate::service_ipc::write_frame(&mut frame, &query) {
        return (Err(error.into()), Some(stream));
    }
    if context.cancellation().is_cancelled() || Instant::now() >= context.deadline() {
        return (
            Err(anyhow::anyhow!("native read canceled before submission")),
            Some(stream),
        );
    }
    let write = tokio::time::timeout(
        context
            .deadline()
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(5)),
        async {
            stream.write_all(&frame).await?;
            stream.flush().await
        },
    );
    let submitted = tokio::select! {
        biased;
        _ = context.cancellation().cancelled() => Err(anyhow::anyhow!("native read canceled during submission")),
        result = write => result.context("native frame write deadline exceeded").and_then(|result| result.map_err(Into::into)),
    };
    let response = match submitted {
        Err(error) => Err(error),
        Ok(()) => {
            // The complete frame may already be executing. A caller deadline or
            // canceled HTTP wait suppresses delivery, never the accepted owner.
            // The worker's existing hard-reap budget is 125 s. A later transport
            // timeout is still UNKNOWN and must await native reap below.
            local_ipc::read_message::<_, protocol::QueryResult>(
                &mut stream,
                Duration::from_secs(130),
            )
            .await
            .and_then(|response| response.context("native query disconnected before completion"))
            .and_then(|response| {
                if let Some(admission) = &response.read_admission {
                    ensure!(
                        query
                            .read_attempt
                            .is_some_and(|attempt| attempt.request_id == admission.request_id)
                            && admission.identity == identity
                            && admission.scope == scope
                            && !response.rebind,
                        "native selection admission proof differs from the submitted request"
                    );
                }
                Ok(response)
            })
        }
    };
    match response {
        Ok(response) => (Ok(response), Some(stream)),
        Err(error) => {
            // Drop BOTH directions before observing reap. Never resume a frame
            // after partial write/read or treat EOF/elapsed time as settlement.
            drop(stream);
            let _actually_reaped = context.wait_for_worker_reap(identity).await;
            (Err(error), None)
        }
    }
}

#[cfg(test)]
mod tests;
