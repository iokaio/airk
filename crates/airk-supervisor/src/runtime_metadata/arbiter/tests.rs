// SPDX-License-Identifier: Apache-2.0
//! Actual async IO driver entry controls. No service/model/NN or forged role.
use super::drive_control;
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
