// SPDX-License-Identifier: Apache-2.0
//! Actual async IO driver entry controls. No service/model/NN or forged role.
use super::{clipped, drive_control, next_read_budget};
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread::ThreadId,
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn exchange(address: SocketAddr, calls: Arc<AtomicUsize>) -> Result<ThreadId> {
    calls.fetch_add(1, Ordering::SeqCst);
    let mut stream = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::net::TcpStream::connect(address),
    )
    .await??;
    stream.write_all(b"ping").await?;
    let mut reply = [0u8; 4];
    stream.read_exact(&mut reply).await?;
    ensure!(&reply == b"pong", "async IO reply differs");
    Ok(std::thread::current().id())
}
fn actual_io(entry: impl FnOnce(SocketAddr, Arc<AtomicUsize>) -> Result<ThreadId>) -> Result<()> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let calls = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|scope| -> Result<()> {
        let server = scope.spawn(move || -> Result<()> {
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::yield_now()
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            stream.set_write_timeout(Some(Duration::from_secs(3)))?;
            let mut request = [0u8; 4];
            stream.read_exact(&mut request)?;
            ensure!(&request == b"ping", "async IO request differs");
            stream.write_all(b"pong")?;
            Ok(())
        });
        let result = entry(address, calls.clone());
        server
            .join()
            .map_err(|_| anyhow::anyhow!("test IO peer panicked"))??;
        result?;
        ensure!(
            calls.load(Ordering::SeqCst) == 1,
            "control callback replayed"
        );
        Ok(())
    })
}

#[test]
fn sync_control_drives_actual_async_io_without_an_ambient_runtime() -> Result<()> {
    actual_io(|address, calls| drive_control(|| exchange(address, calls)))
}
#[test]
fn sync_control_drives_actual_async_io_from_entered_multithread_runtime() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    actual_io(|address, calls| {
        runtime.block_on(async {
            drive_control(|| async move {
                ensure!(
                    matches!(
                        tokio::runtime::Handle::current().runtime_flavor(),
                        tokio::runtime::RuntimeFlavor::MultiThread
                    ),
                    "production callback did not retain the existing runtime flavor"
                );
                exchange(address, calls).await
            })
        })
    })
}
#[test]
fn sync_control_drives_actual_async_io_from_entered_current_thread_runtime() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let caller = std::thread::current().id();
    actual_io(|address, calls| {
        runtime.block_on(async {
            let thread = drive_control(|| exchange(address, calls))?;
            ensure!(
                thread != caller,
                "current-thread callback nested its own driver"
            );
            Ok(thread)
        })
    })
}
#[test]
fn sync_control_drives_actual_async_io_from_existing_blocking_worker() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    actual_io(|address, calls| {
        runtime.block_on(async {
            tokio::task::spawn_blocking(move || drive_control(|| exchange(address, calls))).await?
        })
    })
}
#[test]
fn sync_control_preserves_typed_errors_without_retry() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let calls = AtomicUsize::new(0);
    let result: Result<()> = runtime.block_on(async {
        drive_control(|| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(airk_core::scoped_reads::ScopeReadRefused.into())
        })
    });
    ensure!(
        result.is_err_and(|error| error.is::<airk_core::scoped_reads::ScopeReadRefused>()),
        "typed authority refusal changed"
    );
    ensure!(calls.load(Ordering::SeqCst) == 1, "failed callback retried");
    Ok(())
}
#[test]
fn sync_control_constructs_timers_inside_driver_and_keeps_expired_original_deadline() -> Result<()>
{
    let calls = AtomicUsize::new(0);
    let deadline = Instant::now() - Duration::from_secs(1);
    let result: Result<()> = drive_control(|| {
        calls.fetch_add(1, Ordering::SeqCst);
        // Construct here, before returning the future: the factory must already
        // be entered into its driver even when the caller had no runtime.
        let timeout = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            std::future::pending::<()>(),
        );
        async move {
            timeout
                .await
                .context("original control deadline exhausted")?;
            Ok(())
        }
    });
    ensure!(
        result.is_err_and(|error| error.is::<tokio::time::error::Elapsed>()),
        "expired original deadline was refreshed"
    );
    ensure!(
        calls.load(Ordering::SeqCst) == 1,
        "expired callback replayed"
    );
    Ok(())
}

#[test]
fn metadata_waits_clip_to_the_original_read_deadline_and_never_start_late() -> Result<()> {
    // A transport that never answers is cut off at the ORIGINAL deadline, well
    // inside the unchanged five-second I1 request ceiling.
    let started = Instant::now();
    let deadline = started + Duration::from_millis(100);
    let hung = drive_control(|| {
        clipped(
            deadline,
            "test metadata",
            std::future::pending::<Result<()>>(),
        )
    });
    let error = hung
        .err()
        .context("a hung callback outlived its deadline")?;
    ensure!(
        error
            .to_string()
            .contains("original deadline exhausted in native transport"),
        "unexpected clipping error: {error}"
    );
    ensure!(
        started.elapsed() < Duration::from_secs(3),
        "clipped wait was not bounded by the original deadline"
    );
    // An already-expired deadline refuses before any transport work is polled.
    let polled = Arc::new(AtomicUsize::new(0));
    let witness = polled.clone();
    let past = Instant::now()
        .checked_sub(Duration::from_millis(1))
        .unwrap_or_else(Instant::now);
    let expired = drive_control(|| {
        clipped(past, "test metadata", async move {
            witness.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    });
    ensure!(
        expired.is_err_and(|error| error
            .to_string()
            .contains("original deadline exhausted before native transport")),
        "expired deadline was started"
    );
    ensure!(
        polled.load(Ordering::SeqCst) == 0,
        "expired callback polled"
    );
    // A prompt answer, and a typed scope refusal, pass through unchanged.
    let deadline = Instant::now() + Duration::from_secs(30);
    ensure!(
        drive_control(|| clipped(deadline, "test metadata", async { Ok(7u8) }))? == 7,
        "prompt answer altered"
    );
    let refused = drive_control(|| {
        clipped(deadline, "test metadata", async {
            Err::<(), anyhow::Error>(airk_core::scoped_reads::ScopeReadRefused.into())
        })
    });
    ensure!(
        refused.is_err_and(|error| error.is::<airk_core::scoped_reads::ScopeReadRefused>()),
        "scope refusal was not preserved"
    );
    Ok(())
}

#[test]
fn next_read_narrows_each_original_budget_without_minting_an_attempt() -> Result<()> {
    use airk_core::{
        identity::Incarnation,
        read_attempt::{ReadAttemptBudget, ReadClock},
    };
    let sampled = Instant::now();
    let now_ns = 1_000_000_000;
    let original = ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([7; 16]),
        clock: ReadClock::WindowsPerformanceCounter,
        expires_monotonic_ns: now_ns + 20_000_000_000,
        remaining_ns: 18_000_000_000,
        native_retry_expires_monotonic_ns: Some(now_ns + 5_000_000_000),
    };
    for (local_ms, caller_ms, host_now, expected_ns) in [
        (30_000, 5_000, now_ns, 5_000_000_000),
        (3_000, 30_000, now_ns, 3_000_000_000),
        (30_000, 30_000, now_ns + 19_000_000_000, 1_000_000_000),
        (30_000, 30_000, now_ns, 18_000_000_000),
    ] {
        let (narrowed, remaining_ms) = next_read_budget(
            Some(original),
            caller_ms,
            sampled + Duration::from_millis(local_ms),
            sampled,
            ReadClock::WindowsPerformanceCounter,
            host_now,
        )?;
        assert_eq!(narrowed.request_id, original.request_id);
        assert_eq!(narrowed.clock, original.clock);
        assert_eq!(narrowed.schema, original.schema);
        assert_eq!(narrowed.expires_monotonic_ns, original.expires_monotonic_ns);
        assert_eq!(
            narrowed.native_retry_expires_monotonic_ns,
            original.native_retry_expires_monotonic_ns
        );
        assert_eq!(narrowed.remaining_ns, expected_ns);
        assert_eq!(remaining_ms, expected_ns / 1_000_000);
    }
    for (attempt, caller_ms, deadline, clock, host_now) in [
        (
            None,
            30_000,
            sampled + Duration::from_secs(30),
            original.clock,
            now_ns,
        ),
        (
            Some(original),
            0,
            sampled + Duration::from_secs(30),
            original.clock,
            now_ns,
        ),
        (
            Some(original),
            120_001,
            sampled + Duration::from_secs(30),
            original.clock,
            now_ns,
        ),
        (Some(original), 30_000, sampled, original.clock, now_ns),
        (
            Some(original),
            30_000,
            sampled + Duration::from_secs(30),
            original.clock,
            original.expires_monotonic_ns,
        ),
        (
            Some(original),
            30_000,
            sampled + Duration::from_secs(30),
            ReadClock::UnixMonotonic,
            now_ns,
        ),
    ] {
        assert!(next_read_budget(attempt, caller_ms, deadline, sampled, clock, host_now).is_err());
    }
    // The caller's retained value is not mutated or replaced by projection.
    assert_eq!(original.remaining_ns, 18_000_000_000);
    Ok(())
}
