// SPDX-License-Identifier: Apache-2.0
//! Idle unload of a stage's model (plan section 12.5).
//!
//! A stage's resident model unloads after a period with no model call
//! (default 30 minutes; `None` or zero means never). The unload is an ordinary
//! lifecycle operation: it claims the lifecycle like Configure does, runs only
//! when no call for the stage is in flight, and uses the same release and
//! drain as a profile change (`release_profile`, the observed release, then
//! the final charge). The next call that finds the stage unloaded starts the
//! reload and is answered without the model at once: the stage is not Ready,
//! so the call returns no advice and the Answer runs on reads or the no-model
//! loop. A reload never blocks or times out an Answer.
use super::*;
use airk_core::identity::Sha256Digest;
use std::sync::Weak;

/// The default idle time before a stage's model unloads.
pub(super) const DEFAULT_IDLE_UNLOAD: Duration = Duration::from_secs(30 * 60);
/// The longest a watcher sleeps between checks, so a shortened timeout or a
/// late call is observed promptly.
const MAX_CHECK_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IdleDecision {
    /// Never unload (no timeout).
    Never,
    /// Check again after this long.
    CheckAfter(Duration),
    /// Unload now.
    Unload,
}

/// The idle rule, with the clock supplied by the caller.
pub(super) fn decide(
    now: Instant,
    last_use: Instant,
    timeout: Option<Duration>,
    phase: Availability,
    busy: bool,
    in_flight: usize,
) -> IdleDecision {
    let Some(timeout) = timeout.filter(|value| !value.is_zero()) else {
        return IdleDecision::Never;
    };
    let wait = |value: Duration| IdleDecision::CheckAfter(value.clamp(Duration::from_millis(10), MAX_CHECK_INTERVAL));
    // Only a settled Ready resident with no call in flight unloads. Any
    // lifecycle operation owns the stage until it settles.
    if phase != Availability::Ready || busy || in_flight > 0 {
        return wait(timeout);
    }
    let idle = now.saturating_duration_since(last_use);
    if idle >= timeout {
        IdleDecision::Unload
    } else {
        wait(timeout - idle)
    }
}

/// Per-stage idle state, held in the lifecycle state.
#[derive(Debug)]
pub(super) struct Idle {
    pub(super) timeout: Option<Duration>,
    pub(super) last_use: Instant,
    pub(super) in_flight: usize,
    /// The manifest the idle rule unloaded, until a reload starts.
    pub(super) unloaded: Option<Sha256Digest>,
    /// True while a watcher task runs for this stage.
    pub(super) watching: bool,
    /// Measured release time of the last idle unload, for status and receipts.
    pub(super) last_unload_ms: Option<u64>,
}
impl Default for Idle {
    fn default() -> Self {
        Self {
            timeout: Some(DEFAULT_IDLE_UNLOAD),
            last_use: Instant::now(),
            in_flight: 0,
            unloaded: None,
            watching: false,
            last_unload_ms: None,
        }
    }
}

/// Counts one model call of the stage while it runs; the call's start and end
/// both count as use.
pub(super) struct UseGuard {
    shared: Weak<Mutex<lifecycle::State>>,
}
impl Drop for UseGuard {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            let mut state = shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.idle.in_flight = state.idle.in_flight.saturating_sub(1);
            state.idle.last_use = Instant::now();
        }
    }
}

/// What a call found when it began.
pub(super) struct UseStart {
    pub(super) guard: UseGuard,
    /// The stage was unloaded for idleness; the caller starts the reload.
    pub(super) reload: bool,
}

pub(super) fn begin_use(shared: &Arc<Mutex<lifecycle::State>>) -> Result<UseStart> {
    let mut state = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
    state.idle.in_flight = state
        .idle
        .in_flight
        .checked_add(1)
        .context("Arbiter call count exhausted")?;
    state.idle.last_use = Instant::now();
    let reload = state.idle.unloaded.take().is_some() && !state.busy && !state.stopping;
    drop(state);
    let guard = UseGuard {
        shared: Arc::downgrade(shared),
    };
    ensure_watch(shared);
    Ok(UseStart { guard, reload })
}

/// Starts this stage's watcher if none runs and a runtime is present.
pub(super) fn ensure_watch(shared: &Arc<Mutex<lifecycle::State>>) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    {
        let Ok(mut state) = shared.lock() else { return };
        if state.idle.watching || state.idle.timeout.is_none_or(|value| value.is_zero()) {
            return;
        }
        state.idle.watching = true;
    }
    let weak = Arc::downgrade(shared);
    runtime.spawn(async move { watch(weak).await });
}

async fn watch(weak: Weak<Mutex<lifecycle::State>>) {
    loop {
        let decision = {
            let Some(shared) = weak.upgrade() else { return };
            let Ok(mut state) = shared.lock() else { return };
            let decision = decide(
                Instant::now(),
                state.idle.last_use,
                state.idle.timeout,
                state.phase,
                state.busy,
                state.idle.in_flight,
            );
            if matches!(decision, IdleDecision::Never) || state.stopping {
                state.idle.watching = false;
                return;
            }
            decision
        };
        match decision {
            IdleDecision::CheckAfter(wait) => tokio::time::sleep(wait).await,
            IdleDecision::Unload => {
                let Some(shared) = weak.upgrade() else { return };
                // A refused or failed unload keeps the stage as the release
                // left it; the lifecycle reason says why.
                let _ = unload(&shared).await;
            }
            IdleDecision::Never => return,
        }
    }
}

/// Unloads the Ready resident through the lifecycle's safe release path.
async fn unload(shared: &Arc<Mutex<lifecycle::State>>) -> Result<()> {
    let (handle, manifest, before) = {
        let state = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
        let manifest = state.profile.context("idle unload has no selected profile")?;
        let handle = state.handle.clone().context("idle unload has no native owner")?;
        (handle, manifest, (state.phase, state.reason.clone()))
    };
    // Claim the lifecycle like any operation; a Configure that arrived first
    // wins, and this attempt is simply skipped.
    let (serial, _cancel) =
        lifecycle::start(shared, manifest, None).context("idle unload not admitted")?;
    let started = Instant::now();
    let mut released = false;
    let result = async {
        let release = {
            let state = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
            ensure!(
                state.idle.in_flight == 0,
                "a model call arrived; idle unload skipped"
            );
            handle.resident(manifest)?.reservation().release_observer()
        };
        lifecycle::phase(
            shared,
            serial,
            Availability::Draining,
            "Idle: unloading the model through the ordinary release; the next call reloads it.",
        )?;
        released = true;
        lifecycle::release_actual(
            &handle,
            manifest,
            release,
            Instant::now() + Duration::from_secs(30),
        )
        .await?;
        let mut state = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
        state.charges.retain(|(profile, _)| *profile != manifest);
        state.idle.unloaded = Some(manifest);
        state.idle.last_unload_ms = u64::try_from(started.elapsed().as_millis()).ok();
        drop(state);
        lifecycle::phase(
            shared,
            serial,
            Availability::Requested,
            "Unloaded after the idle timeout. The next Answer starts the reload and runs on reads or without the model until it is Ready.",
        )
    }
    .await;
    lifecycle::complete(shared, serial, &result);
    if result.is_err()
        && !released
        && let Ok(mut state) = shared.lock()
        && state.serial == serial
        && !state.busy
    {
        // Nothing was released: the resident stays exactly as it was.
        (state.phase, state.reason) = before;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ready_idle_stage_unloads_only_after_its_timeout() {
        let start = Instant::now();
        let timeout = Some(Duration::from_secs(1800));
        let at = |seconds| start + Duration::from_secs(seconds);
        assert_eq!(
            decide(at(1799), start, timeout, Availability::Ready, false, 0),
            IdleDecision::CheckAfter(Duration::from_secs(1))
        );
        assert_eq!(
            decide(at(1800), start, timeout, Availability::Ready, false, 0),
            IdleDecision::Unload
        );
        // A long wait is capped so a changed timeout is seen promptly.
        assert_eq!(
            decide(at(0), start, timeout, Availability::Ready, false, 0),
            IdleDecision::CheckAfter(MAX_CHECK_INTERVAL)
        );
    }

    #[test]
    fn calls_operations_and_other_phases_keep_the_model() {
        let start = Instant::now();
        let late = start + Duration::from_secs(4000);
        let timeout = Some(Duration::from_secs(1800));
        for (phase, busy, in_flight) in [
            (Availability::Ready, false, 1),
            (Availability::Ready, true, 0),
            (Availability::Warming, false, 0),
            (Availability::Draining, false, 0),
            (Availability::Requested, false, 0),
        ] {
            assert!(matches!(
                decide(late, start, timeout, phase, busy, in_flight),
                IdleDecision::CheckAfter(_)
            ));
        }
    }

    #[test]
    fn zero_or_no_timeout_never_unloads() {
        let start = Instant::now();
        let late = start + Duration::from_secs(1 << 20);
        for timeout in [None, Some(Duration::ZERO)] {
            assert_eq!(
                decide(late, start, timeout, Availability::Ready, false, 0),
                IdleDecision::Never
            );
        }
    }

    #[test]
    fn a_call_counts_as_use_and_takes_the_reload() -> Result<()> {
        let shared = Arc::new(Mutex::new(lifecycle::State::default()));
        let manifest = Sha256Digest::from_bytes([7; 32]);
        {
            let mut state = shared.lock().unwrap();
            state.idle.unloaded = Some(manifest);
            state.idle.last_use = Instant::now() - Duration::from_secs(3600);
        }
        let first = begin_use(&shared)?;
        assert!(first.reload);
        {
            let state = shared.lock().unwrap();
            assert_eq!(state.idle.in_flight, 1);
            assert!(state.idle.unloaded.is_none());
            assert!(state.idle.last_use.elapsed() < Duration::from_secs(60));
        }
        // Only the first call after an unload starts the reload.
        let second = begin_use(&shared)?;
        assert!(!second.reload);
        drop(first);
        drop(second);
        assert_eq!(shared.lock().unwrap().idle.in_flight, 0);
        // A call during another lifecycle operation leaves the reload to it.
        {
            let mut state = shared.lock().unwrap();
            state.idle.unloaded = Some(manifest);
            state.busy = true;
        }
        assert!(!begin_use(&shared)?.reload);
        Ok(())
    }
}
