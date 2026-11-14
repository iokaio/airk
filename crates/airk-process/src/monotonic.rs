// SPDX-License-Identifier: Apache-2.0
//! Same-host monotonic timestamps for private IPC deadlines, never wall time.
use anyhow::{Result, ensure};

#[cfg(unix)]
fn clock_id() -> libc::clockid_t {
    // Match std::time::Instant: Darwin CLOCK_MONOTONIC has a different
    // resolution and sleep basis from the mach absolute uptime clock.
    #[cfg(target_vendor = "apple")]
    {
        libc::CLOCK_UPTIME_RAW
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        libc::CLOCK_MONOTONIC
    }
}

#[cfg(unix)]
pub fn now_ns() -> Result<u64> {
    let mut value: libc::timespec = unsafe { std::mem::zeroed() };
    // SAFETY: clock_gettime writes one correctly sized timespec.
    ensure!(
        unsafe { libc::clock_gettime(clock_id(), &mut value) } == 0,
        "native monotonic clock unavailable: {}",
        std::io::Error::last_os_error()
    );
    ensure!(
        value.tv_sec >= 0 && (0..1_000_000_000).contains(&value.tv_nsec),
        "invalid native monotonic clock"
    );
    let nanos = (value.tv_sec as u128) * 1_000_000_000 + value.tv_nsec as u128;
    Ok(u64::try_from(nanos)?)
}

#[cfg(all(test, target_vendor = "apple"))]
#[test]
fn apple_native_deadlines_share_the_instant_uptime_clock() -> Result<()> {
    assert_eq!(clock_id(), libc::CLOCK_UPTIME_RAW);
    assert_ne!(clock_id(), libc::CLOCK_MONOTONIC);
    // Each enclosing native interval must cover its enclosed Instant interval.
    // These are independent samples, with no sleep, tolerance or retry.
    for _ in 0..128 {
        let before = now_ns()?;
        let start = std::time::Instant::now();
        let end = std::time::Instant::now();
        let after = now_ns()?;
        assert!(u128::from(after - before) >= end.duration_since(start).as_nanos());
    }
    Ok(())
}

#[cfg(windows)]
pub fn now_ns() -> Result<u64> {
    use windows_sys::Win32::System::Performance::{
        QueryPerformanceCounter, QueryPerformanceFrequency,
    };
    let mut ticks = 0i64;
    let mut frequency = 0i64;
    // SAFETY: these functions receive valid pointers to the documented i64 outputs.
    ensure!(
        unsafe { QueryPerformanceCounter(&mut ticks) } != 0
            && unsafe { QueryPerformanceFrequency(&mut frequency) } != 0
            && ticks >= 0
            && frequency > 0,
        "native monotonic clock unavailable"
    );
    Ok(u64::try_from(
        (ticks as u128) * 1_000_000_000 / frequency as u128,
    )?)
}

#[cfg(test)]
#[test]
fn clock_has_one_monotonic_domain_across_threads() -> Result<()> {
    let before = now_ns()?;
    let other = std::thread::spawn(now_ns).join().unwrap()?;
    let after = now_ns()?;
    ensure!(
        before <= other && other <= after,
        "native monotonic clock moved backwards"
    );
    Ok(())
}
