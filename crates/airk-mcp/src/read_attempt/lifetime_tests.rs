// SPDX-License-Identifier: Apache-2.0
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canceled_handler_retains_lifetime_guard_until_blocking_work_finishes() -> anyhow::Result<()>
{
    struct Guard(Arc<std::sync::atomic::AtomicUsize>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = airk_kernel::Kernel::open(source.path(), Some(data.path().into()))?;
    let dropped = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let context = ReadAttemptContext::new(
        Instant::now() + Duration::from_secs(5),
        CancellationToken::new(),
    )
    .with_lifetime_guard(Arc::new(Guard(dropped.clone())));
    let mut server = crate::AirkServer::new(kernel);
    server.read_attempt = Some(context);
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let task = tokio::spawn(async move {
        server
            .execute(airk_core::usage::UsageOperation::Status, move |_| {
                let _ = entered.send(());
                wait.recv()?;
                Ok(serde_json::json!({"complete":true}))
            })
            .await
    });
    started.await?;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        dropped.load(Ordering::Acquire),
        0,
        "caller cancellation released accepted native lifetime"
    );
    release.send(())?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while dropped.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_eq!(dropped.load(Ordering::Acquire), 1);
    Ok(())
}
