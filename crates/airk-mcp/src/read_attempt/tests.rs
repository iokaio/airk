// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn controlled_deadline_and_shared_member_credit_allow_only_one_wait() {
    let now = Instant::now();
    let original = ReadAttemptContext::new(now + Duration::from_secs(1), CancellationToken::new());
    let short = original.narrowed(now + Duration::from_millis(50));
    assert!(
        !short.reserve_at(now),
        "the full wait must fit strictly inside original budget"
    );
    let member = original.narrowed(now + Duration::from_millis(51));
    assert!(member.reserve_at(now));
    assert!(
        !original.reserve_at(now),
        "members must not mint another retry credit"
    );
    assert_eq!(
        original.narrowed(now + Duration::from_secs(10)).deadline(),
        original.deadline()
    );
}

#[test]
fn canceled_read_cannot_reserve_or_reset_credit() {
    let canceled = CancellationToken::new();
    let original =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(1), canceled.clone());
    canceled.cancel();
    assert!(!original.reserve_at(Instant::now()));
    assert!(!original.retry_spent.load(Ordering::Acquire));
}

#[test]
fn original_native_retry_budget_must_fit_the_complete_wait_without_reset() {
    let now = Instant::now();
    let original =
        ReadAttemptContext::new(now + Duration::from_secs(125), CancellationToken::new());
    for remaining_ms in [0, 49, 50] {
        let native = original
            .clone()
            .with_native_retry_deadline(now + Duration::from_millis(remaining_ms));
        assert!(!native.reserve_at(now));
        assert!(!original.retry_spent.load(Ordering::Acquire));
    }
    let native = original
        .clone()
        .with_native_retry_deadline(now + Duration::from_millis(51));
    assert_eq!(
        native
            .clone()
            .with_native_retry_deadline(now + Duration::from_secs(5))
            .selection_deadline(),
        now + Duration::from_millis(51)
    );
    assert!(native.reserve_at(now));
    assert!(
        !original.reserve_at(now),
        "native and aggregate share the single retry credit"
    );
}

#[tokio::test]
async fn cancellation_interrupts_one_reserved_wait_without_releasing_its_credit() {
    let canceled = CancellationToken::new();
    let original =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(1), canceled.clone());
    let waiter = original.clone();
    let task = tokio::spawn(async move { waiter.wait_for_selection_admission().await });
    while !original.retry_spent.load(Ordering::Acquire) {
        tokio::task::yield_now().await;
    }
    canceled.cancel();
    assert!(!task.await.unwrap());
    assert!(!original.wait_for_selection_admission().await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_local_handler_retries_only_selection_preflight_and_accounts_once()
-> anyhow::Result<()> {
    use std::fs::OpenOptions;
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    std::fs::write(source.path().join("fixture.md"), "# retained fixture\n")?;
    let kernel = airk_kernel::Kernel::open_service_collection(
        source.path(),
        Some(data.path().into()),
        "fixture",
    )?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    kernel.configure_usage(airk_core::usage::RecordingMode::AggregatesOnly)?;
    let before = kernel.usage_report(30)?.report.calls;
    let mut tools = crate::local::LocalTools::new(kernel.clone()).await?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(data.path().join("collections-1.4/fixture/manifest.lock"))?;
    lock.try_lock()?;
    let context = ReadAttemptContext::new(
        Instant::now() + Duration::from_secs(5),
        CancellationToken::new(),
    );
    let waiting = context.clone();
    let release = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !waiting.retry_spent.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        drop(lock);
    });
    let reply = tools
        .call_with_read_context("airk_status".into(), Default::default(), context.clone())
        .await?;
    release.join().unwrap();
    assert_ne!(reply.is_error, Some(true));
    assert!(context.retry_spent.load(Ordering::Acquire));
    assert_eq!(kernel.usage_report(30)?.report.calls, before + 1);
    tools.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_worker_proof_is_typed_preflight_only_and_legacy_does_not_mint_it()
-> anyhow::Result<()> {
    use std::fs::OpenOptions;
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    std::fs::write(source.path().join("fixture.md"), "# fixture\n")?;
    let kernel = airk_kernel::Kernel::open_service_collection(
        source.path(),
        Some(data.path().into()),
        "fixture",
    )?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    kernel.configure_usage(airk_core::usage::RecordingMode::AggregatesOnly)?;
    let before = kernel.usage_report(30)?.report.calls;
    let mut tools = crate::local::LocalTools::new(kernel.clone()).await?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(data.path().join("collections-1.4/fixture/manifest.lock"))?;
    lock.try_lock()?;
    let context = ReadAttemptContext::new(
        Instant::now() + Duration::from_secs(5),
        CancellationToken::new(),
    )
    .remote_admission();
    let refused = tools
        .call_with_read_context("airk_status".into(), Default::default(), context.clone())
        .await?;
    assert_eq!(refused.is_error, Some(true));
    assert!(context.selection_admission_refused());
    assert!(
        !context.retry_spent.load(Ordering::Acquire),
        "worker must not consume frontend credit"
    );
    drop(lock);
    assert_eq!(kernel.usage_report(30)?.report.calls, before);
    let mut server = crate::AirkServer::new(kernel.clone());
    let body_context = ReadAttemptContext::new(
        Instant::now() + Duration::from_secs(5),
        CancellationToken::new(),
    )
    .remote_admission();
    server.read_attempt = Some(body_context.clone());
    let failed = server
        .execute(airk_core::usage::UsageOperation::Status, |_| {
            Err::<serde_json::Value, _>(anyhow::Error::new(
                airk_kernel::retrieval_retry::SelectionReadAdmissionContention,
            ))
        })
        .await;
    assert_eq!(failed.is_error, Some(true));
    assert!(
        !body_context.selection_admission_refused(),
        "body failure cannot authorize replay"
    );
    assert_eq!(kernel.usage_report(30)?.report.calls, before + 1);
    tools.close().await?;
    Ok(())
}
