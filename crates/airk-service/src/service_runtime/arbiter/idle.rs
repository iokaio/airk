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
    /// The manifest an idle unload left, when this call is the one to start
    /// its reload. The marker is taken only when no lifecycle operation owns
    /// the stage, so the reload can actually start; the caller puts it back
    /// with [`restore_marker`] if the reload is refused.
    pub(super) reload: Option<Sha256Digest>,
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
    let reload = if !state.busy && !state.stopping {
        state.idle.unloaded.take()
    } else {
        None
    };
    drop(state);
    let guard = UseGuard {
        shared: Arc::downgrade(shared),
    };
    ensure_watch(shared);
    Ok(UseStart { guard, reload })
}

/// Puts an idle-unload marker back after its reload was refused, unless a
/// newer marker exists or the selection moved on.
pub(super) fn restore_marker(shared: &Arc<Mutex<lifecycle::State>>, manifest: Sha256Digest) {
    if let Ok(mut state) = shared.lock()
        && state.idle.unloaded.is_none()
        && state.profile == Some(manifest)
        && state.phase != Availability::Ready
    {
        state.idle.unloaded = Some(manifest);
    }
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
    let charge = match handle.resident(manifest) {
        Ok(resident) => resident.reservation().release_observer(),
        Err(error) => {
            let result = Err(error.context("idle unload found no resident"));
            settle_skipped(shared, serial, before, &result);
            return result;
        }
    };
    unload_claimed(
        shared,
        serial,
        manifest,
        before,
        &super::release::NativeRelease {
            handle: &handle,
            charge,
        },
    )
    .await
}

/// Settles a claimed unload that released nothing: the stage stays as it was.
fn settle_skipped(
    shared: &Arc<Mutex<lifecycle::State>>,
    serial: u64,
    before: (Availability, String),
    result: &Result<()>,
) {
    lifecycle::complete(shared, serial, result);
    if let Ok(mut state) = shared.lock()
        && state.serial == serial
        && !state.busy
    {
        (state.phase, state.reason) = before;
    }
}

/// The claimed part of an unload, with the release steps supplied.
///
/// The in-flight check and the switch to Draining happen in one critical
/// section, so a call that counts itself afterwards reads a stage that is not
/// Ready and never scores against the resident being released. A drain that
/// fails or times out keeps Draining and still leaves the reload marker, so
/// the next call retries the reload instead of finding a stranded stage.
pub(super) async fn unload_claimed<S: super::release::ReleaseSteps>(
    shared: &Arc<Mutex<lifecycle::State>>,
    serial: u64,
    manifest: Sha256Digest,
    before: (Availability, String),
    steps: &S,
) -> Result<()> {
    let started = Instant::now();
    let admitted = {
        let mut state = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
        let admitted = state.serial == serial && state.idle.in_flight == 0;
        if admitted {
            state.phase = Availability::Draining;
            state.reason = "Idle: unloading the model through the ordinary release; the next call reloads it.".into();
        }
        admitted
    };
    if !admitted {
        let result = Err(anyhow::anyhow!("a model call arrived; idle unload skipped"));
        settle_skipped(shared, serial, before, &result);
        return result;
    }
    let result = super::release::release_then(
        steps,
        manifest,
        Instant::now() + Duration::from_secs(30),
        &tokio_util::sync::CancellationToken::new(),
        async { Ok(()) },
    )
    .await;
    {
        let mut state = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
        state.idle.unloaded = Some(manifest);
        if result.is_ok() && state.serial == serial {
            state.charges.retain(|(profile, _)| *profile != manifest);
            state.idle.last_unload_ms = u64::try_from(started.elapsed().as_millis()).ok();
            state.phase = Availability::Requested;
            state.reason = "Unloaded after the idle timeout. The next Answer starts the reload and runs on reads or without the model until it is Ready.".into();
        }
    }
    lifecycle::complete(shared, serial, &result);
    if let Err(error) = &result
        && let Ok(mut state) = shared.lock()
        && state.serial == serial
    {
        state.reason = bounded_reason(format!(
            "Idle unload did not finish draining ({error:#}); the next call retries the reload."
        ));
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
        assert_eq!(first.reload, Some(manifest));
        {
            let state = shared.lock().unwrap();
            assert_eq!(state.idle.in_flight, 1);
            assert!(state.idle.unloaded.is_none());
            assert!(state.idle.last_use.elapsed() < Duration::from_secs(60));
        }
        // Only the first call after an unload starts the reload.
        let second = begin_use(&shared)?;
        assert_eq!(second.reload, None);
        drop(first);
        drop(second);
        assert_eq!(shared.lock().unwrap().idle.in_flight, 0);
        Ok(())
    }

    #[test]
    fn the_reload_marker_survives_a_busy_lifecycle_and_a_refused_reload() -> Result<()> {
        let shared = Arc::new(Mutex::new(lifecycle::State::default()));
        let manifest = Sha256Digest::from_bytes([7; 32]);
        {
            let mut state = shared.lock().unwrap();
            state.profile = Some(manifest);
            state.phase = Availability::Requested;
            state.idle.unloaded = Some(manifest);
            state.busy = true;
        }
        // While an operation (such as the unload itself) owns the lifecycle,
        // the marker stays for a later call.
        assert_eq!(begin_use(&shared)?.reload, None);
        assert_eq!(shared.lock().unwrap().idle.unloaded, Some(manifest));
        shared.lock().unwrap().busy = false;
        let taken = begin_use(&shared)?;
        assert_eq!(taken.reload, Some(manifest));
        // A refused reload puts the marker back for the next call.
        restore_marker(&shared, manifest);
        assert_eq!(begin_use(&shared)?.reload, Some(manifest));
        // A marker is not restored once the stage is Ready again.
        shared.lock().unwrap().phase = Availability::Ready;
        restore_marker(&shared, manifest);
        assert_eq!(shared.lock().unwrap().idle.unloaded, None);
        Ok(())
    }

    fn ready_stage(manifest: Sha256Digest) -> Arc<Mutex<lifecycle::State>> {
        let shared = Arc::new(Mutex::new(lifecycle::State::default()));
        {
            let mut state = shared.lock().unwrap();
            state.profile = Some(manifest);
            state.phase = Availability::Ready;
            state.reason = "ready".into();
        }
        shared
    }
    fn claim(shared: &Arc<Mutex<lifecycle::State>>, manifest: Sha256Digest) -> Result<(u64, (Availability, String))> {
        let before = {
            let state = shared.lock().unwrap();
            (state.phase, state.reason.clone())
        };
        let (serial, _) = lifecycle::start(shared, manifest, None)?;
        Ok((serial, before))
    }

    #[tokio::test]
    async fn an_idle_unload_releases_in_order_and_leaves_the_reload_marker() -> Result<()> {
        let manifest = Sha256Digest::from_bytes([5; 32]);
        let shared = ready_stage(manifest);
        let (serial, before) = claim(&shared, manifest)?;
        let steps = super::super::release::tests::Recorder::default();
        unload_claimed(&shared, serial, manifest, before, &steps).await?;
        assert_eq!(steps.seen(), vec!["release", "released", "charge-drained"]);
        let state = shared.lock().unwrap();
        assert_eq!(state.phase, Availability::Requested);
        assert!(!state.busy);
        assert_eq!(state.idle.unloaded, Some(manifest));
        assert!(state.idle.last_unload_ms.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn a_call_in_flight_skips_the_unload_and_keeps_the_resident_ready() -> Result<()> {
        let manifest = Sha256Digest::from_bytes([5; 32]);
        let shared = ready_stage(manifest);
        let call = begin_use(&shared)?;
        let (serial, before) = claim(&shared, manifest)?;
        let steps = super::super::release::tests::Recorder::default();
        assert!(unload_claimed(&shared, serial, manifest, before, &steps).await.is_err());
        assert!(steps.seen().is_empty());
        {
            let state = shared.lock().unwrap();
            assert_eq!(state.phase, Availability::Ready);
            assert_eq!(state.reason, "ready");
            assert!(!state.busy);
            assert_eq!(state.idle.unloaded, None);
        }
        drop(call);
        Ok(())
    }

    #[tokio::test]
    async fn a_failed_drain_is_never_stranded() -> Result<()> {
        let manifest = Sha256Digest::from_bytes([5; 32]);
        let shared = ready_stage(manifest);
        let (serial, before) = claim(&shared, manifest)?;
        let steps = super::super::release::tests::Recorder {
            fail_charge_wait: true,
            ..Default::default()
        };
        assert!(unload_claimed(&shared, serial, manifest, before, &steps).await.is_err());
        {
            let state = shared.lock().unwrap();
            // The unknown drain stays visible, and the lifecycle is free.
            assert_eq!(state.phase, Availability::Draining);
            assert!(!state.busy);
            assert!(state.reason.contains("retries the reload"));
        }
        // The next call takes the marker and starts the reload.
        assert_eq!(begin_use(&shared)?.reload, Some(manifest));
        Ok(())
    }
}
