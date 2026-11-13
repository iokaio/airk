// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::io::AsyncReadExt;

fn identity() -> airk_core::service::WorkerIdentity {
    airk_core::service::WorkerIdentity {
        service: airk_core::identity::Incarnation::from_bytes([1; 16]),
        worker: airk_core::identity::Incarnation::from_bytes([2; 16]),
        project: "ab".repeat(32).parse().unwrap(),
        fencing_generation: 1,
    }
}

fn query() -> Query {
    Query {
        read_attempt: None,
        frontend: None,
        tool: "airk_status".into(),
        arguments: Default::default(),
        native: None,
        status_receipt: Default::default(),
    }
}

async fn observed(polls: &AtomicUsize) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while polls.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn complete_write_retains_capacity_after_cancellation_until_actual_reply() -> Result<()> {
    let capacity = Arc::new(tokio::sync::Semaphore::new(1));
    let permit = capacity.clone().acquire_owned().await?;
    let context = airk_mcp::read_attempt::ReadAttemptContext::new(
        Instant::now() + Duration::from_secs(5),
        Default::default(),
    )
    .with_lifetime_guard(Arc::new(permit));
    let cancellation = context.cancellation().clone();
    let (client, mut peer) = tokio::io::duplex(4096);
    let task = tokio::spawn(submit(client, query(), identity(), None, context));
    let received: Option<Query> =
        local_ipc::read_message(&mut peer, Duration::from_secs(5)).await?;
    assert!(received.is_some());
    cancellation.cancel();
    tokio::task::yield_now().await;
    assert!(!task.is_finished());
    assert_eq!(capacity.available_permits(), 0);
    local_ipc::write_message(
        &mut peer,
        &protocol::QueryResult {
            read_admission: None,
            result: serde_json::json!({"complete":true}),
            status_receipt: Default::default(),
            rebind: false,
        },
        Duration::from_secs(5),
    )
    .await?;
    let (result, _) = task.await?;
    assert!(result.is_ok());
    assert_eq!(capacity.available_permits(), 1);
    Ok(())
}

#[tokio::test]
async fn eof_retains_capacity_until_exact_supervisor_reap_and_cancellation_is_not_settlement()
-> Result<()> {
    let capacity = Arc::new(tokio::sync::Semaphore::new(1));
    let permit = capacity.clone().acquire_owned().await?;
    let reaped = Arc::new(AtomicBool::new(false));
    let polls = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(AtomicUsize::new(0));
    let requested = requests.clone();
    let witness = reaped.clone();
    let seen = polls.clone();
    let expected = identity();
    let context = airk_mcp::read_attempt::ReadAttemptContext::new(
        Instant::now() + Duration::from_secs(5),
        Default::default(),
    )
    .with_lifetime_guard(Arc::new(permit))
    .with_worker_settlement_requester(Arc::new(move |worker| {
        ensure!(worker == expected, "crossed graceful drain request");
        if requested.fetch_add(1, Ordering::AcqRel) == 0 {
            anyhow::bail!("fictional supervisor queue full");
        }
        Ok(())
    }))
    .with_worker_reap_observer(Arc::new(move |observed| {
        ensure!(observed == expected, "crossed worker settlement");
        seen.fetch_add(1, Ordering::AcqRel);
        Ok(witness.load(Ordering::Acquire))
    }));
    let cancellation = context.cancellation().clone();
    let (client, mut peer) = tokio::io::duplex(4096);
    let task = tokio::spawn(submit(client, query(), expected, None, context));
    let _: Option<Query> = local_ipc::read_message(&mut peer, Duration::from_secs(5)).await?;
    drop(peer);
    observed(&polls).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while requests.load(Ordering::Acquire) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    cancellation.cancel();
    assert!(!task.is_finished());
    assert_eq!(capacity.available_permits(), 0);
    reaped.store(true, Ordering::Release);
    let (result, stream) = tokio::time::timeout(Duration::from_secs(5), task).await??;
    assert!(result.is_err() && stream.is_none());
    assert_eq!(capacity.available_permits(), 1);
    Ok(())
}

#[tokio::test]
async fn actual_partial_frame_closes_socket_and_cannot_return_capacity_from_cancellation()
-> Result<()> {
    let capacity = Arc::new(tokio::sync::Semaphore::new(1));
    let permit = capacity.clone().acquire_owned().await?;
    let reaped = Arc::new(AtomicBool::new(false));
    let polls = Arc::new(AtomicUsize::new(0));
    let witness = reaped.clone();
    let seen = polls.clone();
    let context = airk_mcp::read_attempt::ReadAttemptContext::new(
        Instant::now() + Duration::from_secs(5),
        Default::default(),
    )
    .with_lifetime_guard(Arc::new(permit))
    .with_worker_reap_observer(Arc::new(move |observed| {
        ensure!(observed == identity(), "crossed worker settlement");
        seen.fetch_add(1, Ordering::AcqRel);
        Ok(witness.load(Ordering::Acquire))
    }));
    let cancellation = context.cancellation().clone();
    let (client, mut peer) = tokio::io::duplex(8);
    let mut request = query();
    request.arguments.insert(
        "fictional_payload".into(),
        serde_json::json!("x".repeat(4096)),
    );
    let task = tokio::spawn(submit(client, request, identity(), None, context));
    // Observe a real header before cancellation. The eight-byte capacity cannot
    // hold the remaining payload, so submission is genuinely partial.
    let mut header = [0; 4];
    peer.read_exact(&mut header).await?;
    cancellation.cancel();
    observed(&polls).await?;
    let mut partial = Vec::new();
    peer.read_to_end(&mut partial).await?;
    assert!(!partial.is_empty() && partial.len() <= 8);
    assert!(!task.is_finished());
    assert_eq!(capacity.available_permits(), 0);
    reaped.store(true, Ordering::Release);
    let (result, stream) = tokio::time::timeout(Duration::from_secs(5), task).await??;
    assert!(result.is_err() && stream.is_none());
    assert_eq!(capacity.available_permits(), 1);
    Ok(())
}
