// SPDX-License-Identifier: Apache-2.0
//! Explicit model lifecycle. Source reads never start this owner or acquire assets.
use super::*;
use airk_core::identity::Sha256Digest;
use airk_inference::inference_engine::pool::arbiter::Handle;
use airk_retrieval::arbiter::runtime::ExecutionProfile;
use airk_store::service::arbiter_assets::{AssetStore, Phase, PinnedAssets};
use airk_supervisor::project_supervisor::{ArbiterManagement, Supervisor};
use tokio_util::sync::CancellationToken;

#[cfg(all(test, feature = "test-support"))]
pub(in crate::service_runtime) mod acquisition_fixture;

pub(super) struct State {
    pub(super) serial: u64,
    pub(super) device_tasks: usize,
    pub(super) accepted_configure_revision: u64,
    pub(super) busy: bool,
    pub(super) stopping: bool,
    pub(super) settled: Arc<tokio::sync::Notify>,
    pub(super) charges: Vec<(
        Sha256Digest,
        airk_kernel::service_resources::OptionalRelease,
    )>,
    pub(super) profile: Option<Sha256Digest>,
    pub(super) phase: Availability,
    pub(super) reason: String,
    pub(super) cancel: CancellationToken,
    pub(super) handle: Option<Handle>,
    pending: Option<PreparedLaunch>,
    pub(super) source_log: Option<airk_host::service_logs::Log>,
    #[cfg(all(
        windows,
        target_arch = "x86_64",
        feature = "test-support",
        feature = "gpu-cuda"
    ))]
    /// Private CUDA evaluation entries keyed by profile manifest: at most one
    /// per Context model, all bound to the same lease and gate. Only the
    /// selected entry is ever resident; a switch releases the old one first.
    pub(super) private_gpu:
        std::collections::BTreeMap<Sha256Digest, Arc<super::evaluation::CudaAnswerEvaluation>>,
    #[cfg(all(test, feature = "test-support"))]
    pub(super) warm_observer: Option<super::warm_observer::Observer>,
    /// Idle unload state (plan 12.5).
    pub(super) idle: super::idle::Idle,
}
impl State {
    fn has_private_cuda_assets(&self, manifest: Sha256Digest) -> bool {
        #[cfg(all(
            windows,
            target_arch = "x86_64",
            feature = "test-support",
            feature = "gpu-cuda"
        ))]
        {
            self.private_gpu.contains_key(&manifest)
        }
        #[cfg(not(all(
            windows,
            target_arch = "x86_64",
            feature = "test-support",
            feature = "gpu-cuda"
        )))]
        {
            let _ = manifest;
            false
        }
    }
}
impl Default for State {
    fn default() -> Self {
        Self {
            serial: 0,
            device_tasks: 0,
            accepted_configure_revision: 0,
            busy: false,
            stopping: false,
            settled: Arc::new(tokio::sync::Notify::new()),
            charges: Vec::new(),
            profile: None,
            phase: Availability::Disabled,
            reason: String::new(),
            cancel: CancellationToken::new(),
            handle: None,
            pending: None,
            source_log: None,
            #[cfg(all(
                windows,
                target_arch = "x86_64",
                feature = "test-support",
                feature = "gpu-cuda"
            ))]
            private_gpu: std::collections::BTreeMap::new(),
            #[cfg(all(test, feature = "test-support"))]
            warm_observer: None,
            idle: super::idle::Idle::default(),
        }
    }
}
/// Availability reasons for a compiled unqualified local-test profile start with
/// this note so no surface presents it as release-qualified.
const LOCAL_TEST_NOTE: &str = "Unqualified local test profile (not release-qualified).";

pub(super) fn phase(
    shared: &Arc<Mutex<State>>,
    serial: u64,
    value: Availability,
    reason: &str,
) -> Result<()> {
    let mut state = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
    ensure!(
        state.serial == serial,
        "Arbiter lifecycle operation superseded"
    );
    ensure!(
        value != Availability::Ready || !state.cancel.is_cancelled(),
        "Arbiter activation was canceled"
    );
    state.phase = value;
    state.reason = bounded_reason(reason);
    Ok(())
}
pub(super) fn start(
    shared: &Arc<Mutex<State>>,
    profile: Sha256Digest,
    effect_check: Option<&airk_https::admin_plane::NativeDispatchCheck>,
) -> Result<(u64, CancellationToken)> {
    start_checked(shared, profile, effect_check, || Ok(()))
}
pub(super) fn start_checked(
    shared: &Arc<Mutex<State>>,
    profile: Sha256Digest,
    effect_check: Option<&airk_https::admin_plane::NativeDispatchCheck>,
    admit: impl FnOnce() -> Result<()>,
) -> Result<(u64, CancellationToken)> {
    let mut state = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
    ensure!(!state.stopping, "Arbiter lifecycle is stopping");
    ensure!(
        !state.busy,
        "Arbiter lifecycle operation remains active; cancel or wait for its original result"
    );
    super::super::native_admin::check_effect(effect_check)?;
    admit()?;
    state.serial = state
        .serial
        .checked_add(1)
        .context("Arbiter lifecycle revision exhausted")?;
    state.busy = true;
    state.profile = Some(profile);
    state.phase = Availability::Requested;
    state.reason = "Explicit Arbiter operation accepted.".into();
    state.cancel = CancellationToken::new();
    Ok((state.serial, state.cancel.clone()))
}
pub(super) fn complete(shared: &Arc<Mutex<State>>, serial: u64, result: &Result<()>) {
    if let Ok(mut state) = shared.lock()
        && state.serial == serial
    {
        if result.is_err()
            && state.phase == Availability::Draining
            && let Some(pending) = state.pending.take()
        {
            state.profile = Some(pending.manifest);
        }
        state.busy = state.pending.is_some();
        if let Err(error) = result {
            state.phase = if state.phase == Availability::Draining {
                Availability::Draining
            } else if state.cancel.is_cancelled() {
                Availability::Requested
            } else {
                Availability::Failed
            };
            state.reason = bounded_reason(user_failure_reason(error));
        }
        state.settled.notify_waiters();
    }
}
/// Final user-facing reason for a failed operation. A source-class download
/// failure or an expired part deadline gets a fixed message with a retry note;
/// the underlying chain (which may carry transport detail) is never shown.
pub(super) fn user_failure_reason(error: &anyhow::Error) -> String {
    match error.downcast_ref::<super::download::Failure>() {
        Some(super::download::Failure::Source(_)) => {
            "The model download failed because the download source is unavailable or returned an unexpected response. Nothing was enabled. Try again later. Partially downloaded data is kept; use Remove to reclaim its disk space before retrying.".into()
        }
        Some(super::download::Failure::Deadline) => {
            "The model download did not finish before its deadline. Nothing was enabled. Try again later. Partially downloaded data is kept; use Remove to reclaim its disk space before retrying.".into()
        }
        _ => format!("{error:#}"),
    }
}
pub(super) fn cancel_current(state: &mut State) {
    state.pending = None;
    if state.busy {
        state.cancel.cancel();
        // A download/import has no native completion barrier. The actual
        // warm/remove owner sets Draining when it starts a native release;
        // cancellation must retain that already-owned unknown outcome.
        if state.phase != Availability::Draining {
            state.phase = Availability::Requested;
        }
        state.reason = "Cancellation requested; the original operation must settle. Actual native drains retain their owned completion proof.".into();
    }
}
fn stage_snapshot(
    configuration: &airk_store::service::arbiter::ConfigurationStore,
    expected: &Configuration,
    stage: Stage,
) -> Result<Configuration> {
    let current = configuration.load()?;
    ensure!(
        current.installation == expected.installation
            && current.catalog == expected.catalog
            && current.revision >= expected.revision
            && current.stage(stage) == expected.stage(stage),
        "Arbiter stage configuration changed during operation"
    );
    Ok(current)
}
fn management_snapshot(
    configuration: &airk_store::service::arbiter::ConfigurationStore,
    expected: &Configuration,
    stage: Stage,
    cancel: &CancellationToken,
) -> Result<Configuration> {
    ensure!(!cancel.is_cancelled(), "Arbiter operation canceled");
    stage_snapshot(configuration, expected, stage)
}
fn current(
    configuration: &airk_store::service::arbiter::ConfigurationStore,
    expected: &Configuration,
    stage: Stage,
    cancel: &CancellationToken,
) -> Result<()> {
    management_snapshot(configuration, expected, stage, cancel).map(|_| ())
}
fn admitted_current(
    configuration: &airk_store::service::arbiter::ConfigurationStore,
    expected: &Configuration,
    stage: Stage,
    cancel: &CancellationToken,
    admission: Option<&DeferredAdmission>,
) -> Result<()> {
    current(configuration, expected, stage, cancel)?;
    if let Some(admission) = admission {
        admission(expected)?;
    }
    current(configuration, expected, stage, cancel)
}

/// Called while the own-stage lifecycle guard remains held, before the new
/// cached continuation is prepared. Even identical fields are a fresh intent.
fn commit_configure(
    state: &mut State,
    configuration: &airk_store::service::arbiter::ConfigurationStore,
    expected_revision: u64,
    candidate: &Configuration,
    check: impl Fn() -> Result<()>,
) -> Result<Configuration> {
    let selected = configuration.apply_checked(expected_revision, candidate, check)?;
    state.accepted_configure_revision = selected.revision;
    state.cancel.cancel();
    state.pending = None;
    Ok(selected)
}
/// Lifecycle -> Store is the existing Configure order. Keep the intent guard
/// through the rollback CAS, so an accepted same-stage Configure cannot race
/// between the check and persisted restoration. Other-stage revisions remain
/// eligible because they never alter this stage's accepted intent marker.
fn rollback_current_intent(
    shared: &Arc<Mutex<State>>,
    serial: u64,
    configuration: &airk_store::service::arbiter::ConfigurationStore,
    expected: &Configuration,
    prior: &Configuration,
    stage: Stage,
) -> Result<Option<Configuration>> {
    let state = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
    if state.stopping
        || state.serial != serial
        || state.accepted_configure_revision > expected.revision
    {
        return Ok(None);
    }
    rollback(configuration, expected, prior, stage)
}

fn restore_cancellation(
    state: &mut State,
    serial: u64,
    restored_revision: u64,
    profile: Sha256Digest,
) -> Result<CancellationToken> {
    ensure!(
        !state.stopping
            && state.serial == serial
            && state.accepted_configure_revision <= restored_revision,
        "Arbiter rollback operation superseded by accepted Configure"
    );
    let cancel = CancellationToken::new();
    state.cancel = cancel.clone();
    state.profile = Some(profile);
    Ok(cancel)
}

fn rollback(
    configuration: &airk_store::service::arbiter::ConfigurationStore,
    expected: &Configuration,
    prior: &Configuration,
    stage: Stage,
) -> Result<Option<Configuration>> {
    let current = configuration.load()?;
    if current.installation != expected.installation
        || current.catalog != expected.catalog
        || current.revision < expected.revision
        || current.stage(stage) != expected.stage(stage)
    {
        return Ok(None);
    }
    let mut restored = current.clone();
    *restored.stage_mut(stage) = prior.stage(stage).clone();
    restored.revision = current
        .revision
        .checked_add(1)
        .context("Arbiter rollback revision exhausted")?;
    Ok(Some(configuration.apply(current.revision, &restored)?))
}
fn release_disabled(handle: &Handle, configuration: &Configuration, stage: Stage) -> Result<()> {
    let observation = handle.observation_for(stage)?;
    let mut profiles = [
        observation.active_profile,
        observation.warming_profile,
        configuration.stage(stage).selected_profile,
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    profiles.sort();
    profiles.dedup();
    for profile in profiles {
        handle.release_profile(profile)?;
    }
    Ok(())
}
struct WarmContext<'a> {
    stage: Stage,
    expected: &'a Configuration,
    configuration: &'a Arc<airk_store::service::arbiter::ConfigurationStore>,
    management: &'a ArbiterManagement,
    shared: &'a Arc<Mutex<State>>,
    serial: u64,
    cancel: &'a CancellationToken,
    store: &'a Arc<AssetStore>,
    rollback_profile: Option<&'a Profile>,
    rollback_pins: &'a mut Vec<PinnedAssets>,
    admission: Option<&'a DeferredAdmission>,
}
pub(super) async fn remember_charge(
    shared: &Arc<Mutex<State>>,
    manifest: Sha256Digest,
    charge: airk_kernel::service_resources::OptionalRelease,
) -> Result<()> {
    std::future::poll_fn(|cx| {
        let mut state = match shared.lock() {
            Ok(state) => state,
            Err(_) => {
                return std::task::Poll::Ready(Err(anyhow::anyhow!(
                    "Arbiter lifecycle unavailable"
                )));
            }
        };
        if state.stopping {
            return std::task::Poll::Ready(Err(anyhow::anyhow!("Arbiter lifecycle is stopping")));
        }
        state.charges.retain(|(_, known)| {
            let mut observed = known.wait();
            // A completed successful observation is the only pruning proof.
            !matches!(
                std::future::Future::poll(std::pin::Pin::new(&mut observed), cx),
                std::task::Poll::Ready(Ok(()))
            )
        });
        if let Some((current, _)) = state
            .charges
            .iter_mut()
            .find(|(_, known)| known.same_charge(&charge))
        {
            // One runtime-init charge can transfer to the actual selected warm.
            // Update its target association without adding/releasing an observer.
            *current = manifest;
            return std::task::Poll::Ready(Ok(()));
        }
        if state.charges.len() >= airk_kernel::service_resources::MAX_OPTIONAL_RESERVATIONS {
            return std::task::Poll::Ready(Err(anyhow::anyhow!(
                "Actual optional charge observation capacity remains occupied"
            )));
        }
        state.charges.push((manifest, charge.clone()));
        std::task::Poll::Ready(Ok(()))
    })
    .await
}

pub(super) async fn release_actual(
    handle: &Handle,
    manifest: Sha256Digest,
    release: airk_kernel::service_resources::OptionalRelease,
    deadline: Instant,
) -> Result<()> {
    // A drain after failure or cancellation always runs to its observed end.
    super::release::release_then(
        &super::release::NativeRelease {
            handle,
            charge: release,
        },
        manifest,
        deadline,
        &CancellationToken::new(),
        async { Ok(()) },
    )
    .await
}
/// The resident a warm must release before it reserves: a different active
/// profile of the same stage that holds private device memory. Any other
/// switch keeps the ordinary budget-driven drain (product device variants
/// drain in device management before their own load).
pub(super) fn device_switch_release(
    prior: Option<Sha256Digest>,
    incoming: Sha256Digest,
    prior_private_device: bool,
) -> Option<Sha256Digest> {
    prior.filter(|old| *old != incoming && prior_private_device)
}

/// A refused restore may mark the stage Failed only if no operation claimed
/// or changed the lifecycle since the restore began.
fn restore_failure_applies(current_serial: u64, started_serial: u64, busy: bool) -> bool {
    current_serial == started_serial && !busy
}

/// Private entries a GPU-use revocation releases: every entry the native owner
/// holds active or warming, and the selected entry while its operation runs.
#[cfg(any(test, all(windows, target_arch = "x86_64", feature = "test-support", feature = "gpu-cuda")))]
pub(super) fn private_release_targets(
    entries: impl IntoIterator<Item = Sha256Digest>,
    active: Option<Sha256Digest>,
    warming: Option<Sha256Digest>,
    selected_in_progress: Option<Sha256Digest>,
) -> Vec<Sha256Digest> {
    let mut targets: Vec<_> = entries
        .into_iter()
        .filter(|manifest| {
            active == Some(*manifest)
                || warming == Some(*manifest)
                || selected_in_progress == Some(*manifest)
        })
        .collect();
    targets.sort();
    targets.dedup();
    targets
}

struct DrainContext<'a, 'b> {
    stage: Stage,
    expected: &'a Configuration,
    configuration: &'a Arc<airk_store::service::arbiter::ConfigurationStore>,
    shared: &'a Arc<Mutex<State>>,
    serial: u64,
    cancel: &'a CancellationToken,
    store: &'a Arc<AssetStore>,
    rollback_profile: Option<&'a Profile>,
    rollback_pins: &'b mut Vec<PinnedAssets>,
    admission: Option<&'a DeferredAdmission>,
}

/// Releases the prior resident, observes its native release and final charge
/// drain, and only then runs `next` (the new reservation), through the one
/// order of `release::release_then`. A store-backed prior keeps its verified
/// files pinned for rollback; a private CUDA prior keeps its held descriptors.
async fn drain_prior<T>(
    handle: &Handle,
    old: Sha256Digest,
    private_old: bool,
    context: DrainContext<'_, '_>,
    reason: &str,
    next: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    let DrainContext {
        stage,
        expected,
        configuration,
        shared,
        serial,
        cancel,
        store,
        rollback_profile,
        rollback_pins,
        admission,
    } = context;
    admitted_current(configuration, expected, stage, cancel, admission)?;
    if !private_old {
        let rollback = rollback_profile
            .filter(|profile| profile.manifest == old)
            .context("Arbiter prior disk rollback is unproven; its resident was preserved")?;
        // Keep the old verified files pinned through new activation/rollback.
        let rollback = rollback.clone();
        let rollback_store = store.clone();
        rollback_pins.push(
            super::assets::owned_asset_operation(async move { rollback_store.pin(&rollback) })
                .await?,
        );
    }
    admitted_current(configuration, expected, stage, cancel, admission)?;
    let release = handle.resident(old)?.reservation().release_observer();
    phase(shared, serial, Availability::Draining, reason)?;
    super::release::release_then(
        &super::release::NativeRelease {
            handle,
            charge: release,
        },
        old,
        Instant::now() + Duration::from_secs(30),
        cancel,
        async {
            phase(
                shared,
                serial,
                Availability::Requested,
                "Prior native session and its actual final charge were released; admitting the selected cached warm.",
            )?;
            admitted_current(configuration, expected, stage, cancel, admission)?;
            next.await
        },
    )
    .await
}

async fn warm(
    profile: ExecutionProfile,
    source: Box<dyn airk_retrieval::arbiter::runtime::AssetSource + Send>,
    context: WarmContext<'_>,
) -> Result<()> {
    #[cfg(all(test, feature = "test-support"))]
    let mut observation = WarmCapture::capture(&context, profile.description().manifest);
    let result = async {
    let WarmContext {
        stage,
        expected,
        configuration,
        management,
        shared,
        serial,
        cancel,
        store,
        rollback_profile,
        rollback_pins,
        admission,
    } = context;
    let manifest = profile.description().manifest;
    #[cfg(all(windows, target_arch = "x86_64", feature = "test-support", feature = "gpu-cuda"))]
    let cuda_answer = shared.lock()
        .map_err(|_| anyhow::anyhow!("private CUDA Answer lifecycle unavailable"))?
        .private_gpu.get(&manifest)
        .filter(|answer| answer.manifest == manifest)
        .cloned()
        .map(|answer| -> Result<_> { let grant = super::evaluation::private_gpu_grant(&answer.devices)?; Ok((answer, grant)) })
        .transpose()?;
    let host_bytes = profile.description().memory_reservation_bytes;
    // Private CUDA Answer host and device envelopes: Compact keeps its original
    // 32 GiB host and 24 GiB device bounds; Large uses its profile's 40 GiB host
    // reservation and the 44 GiB device envelope its reviewed graph ran under.
    #[cfg(all(windows, target_arch = "x86_64", feature = "test-support", feature = "gpu-cuda"))]
    let (host_bytes, device_limit_bytes) = if let Some((answer, _)) = &cuda_answer {
        ensure!(Instant::now() < answer.route.expires(), "private CUDA fixture expired");
        match profile.description().model {
            airk_core::arbiter::ModelChoice::ContextQwenLarge => (host_bytes.max(40 << 30), 44 << 30),
            // A strict-GPU Focus entry keeps its profile's host reservation.
            airk_core::arbiter::ModelChoice::FocusQwenReranker06B => (host_bytes, 24 << 30),
            _ => (32 << 30, 24 << 30),
        }
    } else {
        (host_bytes, 24 << 30)
    };
    admitted_current(configuration, expected, stage, cancel, admission)?;
    ensure!(
        configuration.load()? == *expected,
        "Native warm snapshot changed before admission"
    );
    ensure!(
        expected.stage(stage).enabled && expected.stage(stage).selected_profile == Some(manifest),
        "Arbiter warm requires the exact enabled selection"
    );
    let existing = management.existing().await.ok();
    admitted_current(configuration, expected, stage, cancel, admission)?;
    if let Some(handle) = &existing {
        handle.configure(expected.clone())?;
        let retained_observation = handle.observation_for(stage)?;
        if retained_observation.active_profile == Some(manifest)
            && handle.resident(manifest).is_ok()
        {
            admitted_current(configuration, expected, stage, cancel, admission)?;
            let mut state = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
            ensure!(state.serial == serial, "Arbiter operation superseded");
            state.handle = Some(handle.clone());
            drop(state);
            admitted_current(configuration, expected, stage, cancel, admission)?;
            #[cfg(all(test, feature = "test-support"))]
            if let Some(observation) = observation.as_mut() {
                observation.activation_path = super::warm_observer::ActivationPath::RetainedResident;
                observation.native_profile = retained_observation.active_profile;
                observation.native_revision = retained_observation.configuration_revision;
            }
            return phase(
                shared,
                serial,
                Availability::Ready,
                "The exact verified resident profile was retained for the new configuration; no second load or charge was created.",
            );
        }
    }
    phase(
        shared,
        serial,
        Availability::Verifying,
        "Verifying complete compiled Arbiter asset closure.",
    )?;
    // A private device resident never shares the GPU with the next profile of
    // its stage: it is released, its release observed, and only then is the
    // new one reserved and warmed (plan 12.5.2 item 4).
    let prior = existing
        .as_ref()
        .map(|handle| handle.observation_for(stage))
        .transpose()?
        .and_then(|observation| observation.active_profile);
    let prior_private = prior.is_some_and(|old| {
        shared
            .lock()
            .is_ok_and(|state| state.has_private_cuda_assets(old))
    });
    let device_switch = device_switch_release(prior, manifest, prior_private);
    let reservation = if let Some(old) = device_switch {
        let handle = existing
            .as_ref()
            .context("Arbiter device switch has no prior resident to drain")?;
        let released_at = Instant::now();
        let switched = drain_prior(
            handle,
            old,
            prior_private,
            DrainContext {
                stage,
                expected,
                configuration,
                shared,
                serial,
                cancel,
                store,
                rollback_profile,
                rollback_pins: &mut *rollback_pins,
                admission,
            },
            "Switching device profiles; draining the old native session and its actual final memory charge before the new one is reserved, so both are never resident together.",
            async {
                let release_ms = u64::try_from(released_at.elapsed().as_millis()).ok();
                let reservation = management
                    .reserve(host_bytes)
                    .await
                    .context("New device profile cannot fit after actual prior charge release")?;
                Ok((reservation, release_ms))
            },
        )
        .await
        .context("Device profile switch: prior resident drain or new reservation incomplete")?;
        let (reservation, _release_ms) = switched;
        #[cfg(all(test, feature = "test-support"))]
        if let Some(observation) = observation.as_mut() {
            observation.released_prior = Some(old);
            observation.release_ms = _release_ms;
        }
        reservation
    } else {
        match management.reserve(host_bytes).await {
            Ok(reservation) => reservation,
            Err(original) => {
                admitted_current(configuration, expected, stage, cancel, admission)?;
                let handle = existing
                    .as_ref()
                    .context("Arbiter warm has no prior resident to drain")?;
                let old = handle
                    .observation_for(stage)?
                    .active_profile
                    .filter(|old| *old != manifest)
                    .context("Arbiter warm cannot fit its new charge and has no distinct resident to release")?;
                drain_prior(
                    handle,
                    old,
                    prior_private && prior == Some(old),
                    DrainContext {
                        stage,
                        expected,
                        configuration,
                        shared,
                        serial,
                        cancel,
                        store,
                        rollback_profile,
                        rollback_pins: &mut *rollback_pins,
                        admission,
                    },
                    "The budget cannot retain both profiles; draining the old native session and its actual final memory charge while verified rollback assets remain pinned.",
                    async {
                        management
                            .reserve(host_bytes)
                            .await
                            .context("New profile still cannot fit after actual prior charge release")
                    },
                )
                .await
                .with_context(|| format!("New profile reservation refused ({original:#}); prior resident drain or new reservation incomplete"))?
            }
        }
    };
    let release = reservation.release_observer();
    admitted_current(configuration, expected, stage, cancel, admission)?;
    remember_charge(shared, manifest, release.clone()).await?;
    let handle = management.ensure(expected.clone()).await?;
    admitted_current(configuration, expected, stage, cancel, admission)?;
    handle.configure(expected.clone())?;
    {
        let mut state = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
        ensure!(state.serial == serial, "Arbiter operation superseded");
        state.handle = Some(handle.clone());
    }
    phase(
        shared,
        serial,
        Availability::Warming,
        "Loading verified Arbiter bytes inside the existing native inference owner.",
    )?;
    admitted_current(configuration, expected, stage, cancel, admission)?;
    let local_test = airk_retrieval::arbiter::runtime::is_local_test_profile(profile.description());
    #[cfg(all(windows, target_arch = "x86_64", feature = "test-support", feature = "gpu-cuda"))]
    let local_test = local_test || cuda_answer.is_some();
    #[cfg(all(windows, target_arch = "x86_64", feature = "test-support", feature = "gpu-cuda"))]
    let receiver = if let Some((answer, gpu_current)) = cuda_answer.as_ref() {
        let deadline = (Instant::now() + Duration::from_secs(120)).min(answer.route.expires());
        let authority = {
            let expected = expected.clone();
            let configuration = configuration.clone();
            let cancel = cancel.clone();
            let gate = answer.gate.clone();
            let gpu_current = gpu_current.clone();
            Arc::new(move || {
                gpu_current()
                    && !cancel.is_cancelled()
                    && gate.catalog_clock().is_some_and(|clock| clock.namespace == expected.catalog)
                    && configuration.load().is_ok_and(|current| current == expected)
            })
        };
        let host = Arc::new(reservation);
        let plan = match &answer.route {
            super::evaluation::PrivateGpuRoute::Answer(graph) => graph.capacity_plan(&profile)?,
            super::evaluation::PrivateGpuRoute::Focus { .. } => {
                super::evaluation::focus_capacity_plan(profile.description())?
            }
        };
        let probe = handle.probe_evaluation_device(
            answer.lease.clone(),
            profile,
            plan,
            answer.adapter,
            host.clone(),
            airk_core::arbiter::device::GpuProbeReservation {
                schema: "airk.arbiter.gpu-probe-reservation/1".into(),
                host_limit_bytes: host_bytes,
                device_limit_bytes,
                deadline_ms: 120_000,
            },
            deadline,
            authority.clone(),
        )?;
        let mut bound = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), probe)
            .await
            .context("original private CUDA Answer probe deadline")?
            .context("private CUDA Answer probe delivery closed")??;
        ensure!(
            bound.rejections.is_empty()
                && bound.candidates.len() == 1
                && bound.candidates[0].probe_only
                && bound.candidates[0].uuid == answer.adapter
                && bound.candidates[0].profile.description().manifest == manifest,
            "exact private CUDA Answer probe binding unavailable"
        );
        let bound = bound.candidates.remove(0).profile;
        match &answer.route {
            super::evaluation::PrivateGpuRoute::Answer(graph) => handle.warm_evaluation_answer(
                bound,
                source,
                host,
                deadline,
                authority,
                graph.admit(gpu_current.clone())?,
            )?,
            // Focus opens its strict-GPU profile under the probe binding; the
            // scorer verifies every node's placement against the live session.
            super::evaluation::PrivateGpuRoute::Focus { .. } => {
                handle.warm_with_charge(bound, source, host, deadline, authority)?
            }
        }
    } else {
        handle.warm(profile, source, reservation)?
    };
    #[cfg(not(all(windows, target_arch = "x86_64", feature = "test-support", feature = "gpu-cuda")))]
    let receiver = handle.warm(profile, source, reservation)?;
    #[cfg(all(test, feature = "test-support"))]
    if let Some(observation) = observation.as_mut() { observation.activation_path = super::warm_observer::ActivationPath::NativeWarmAdmitted; }
    let result=async {
        tokio::select!{biased;_ = cancel.cancelled()=>anyhow::bail!("Arbiter warm canceled"),value=receiver=>value.context("Arbiter warm delivery closed")??};
        admitted_current(configuration,expected,stage,cancel,admission)?;
        ensure!(configuration.load()? == *expected,"Native warm configuration changed before activation");
        let observed=handle.observation_for(stage)?;
        #[cfg(all(test, feature = "test-support"))]
        if let Some(observation) = observation.as_mut() {
            observation.native_profile=observed.active_profile;
            observation.native_revision=observed.configuration_revision;
        }
        ensure!(observed.active_profile==Some(manifest)&&observed.configuration_revision==Some(expected.revision),"Arbiter native activation not observed");
        admitted_current(configuration,expected,stage,cancel,admission)?;
        ensure!(configuration.load()? == *expected,"Native warm configuration changed before activation");
        phase(shared,serial,Availability::Ready,if local_test {"Unqualified local test profile (not release-qualified) is active in the owned native compute lane."} else {"Qualified Arbiter profile is active in the owned native compute lane."})
    }.await;
    if let Err(error) = result {
        phase(
            shared,
            serial,
            Availability::Draining,
            "Failed or canceled warm is draining; actual native completion and final charge release remain required.",
        )?;
        release_actual(
            &handle,
            manifest,
            release,
            Instant::now() + Duration::from_secs(30),
        )
        .await?;
        phase(
            shared,
            serial,
            Availability::Requested,
            "Failed or canceled native warm and its actual final memory charge completed draining.",
        )?;
        return Err(error);
    }
    Ok(())
    }.await;
    #[cfg(all(test, feature = "test-support"))]
    if let Some(observation) = observation {
        observation.finish(&result);
    }
    result
}

fn cached_intent_eligible(configuration: &Configuration, stage: Stage, assets_ready: bool) -> bool {
    configuration.stage(stage).enabled
        && !configuration.stage(stage).indexes.is_empty()
        && assets_ready
}

fn recovery_removal(
    pending: &std::collections::BTreeMap<Sha256Digest, Phase>,
) -> Result<Option<Sha256Digest>> {
    ensure!(
        pending.len() <= 1,
        "Multiple incomplete Arbiter operations require an explicit named Provision, Import or Remove"
    );
    match pending.first_key_value() {
        Some((&manifest, &Phase::Removing)) => Ok(Some(manifest)),
        Some(_) => anyhow::bail!(
            "Incomplete asset acquisition requires explicit Provision for the compiled network source, or owner-only Import with the original verified local bundle; Recover never changes acquisition authority"
        ),
        None => Ok(None),
    }
}

type NativeAssetSource = Box<dyn airk_retrieval::arbiter::runtime::AssetSource + Send>;
type Restoration = (ExecutionProfile, NativeAssetSource);
type DeferredAdmission = Box<dyn Fn(&Configuration) -> Result<()> + Send + Sync>;
struct PreparedLaunch {
    stage: Stage,
    expected: Configuration,
    manifest: Sha256Digest,
    download: bool,
    import_directory: Option<std::path::PathBuf>,
    previous: Option<Configuration>,
    execution: ExecutionProfile,
    source: NativeAssetSource,
    restoration: Option<Restoration>,
    continuation: Option<Restoration>,
    admission: Option<DeferredAdmission>,
    resident_fallback: Option<Profile>,
}
struct LaunchContext {
    store: Arc<AssetStore>,
    configuration: Arc<airk_store::service::arbiter::ConfigurationStore>,
    shared: Arc<Mutex<State>>,
    management: ArbiterManagement,
    serial: u64,
    cancel: CancellationToken,
}
fn spawn_launch(prepared: PreparedLaunch, context: LaunchContext) {
    let PreparedLaunch {
        stage,
        expected,
        manifest,
        download,
        import_directory,
        previous,
        execution,
        source,
        mut restoration,
        mut continuation,
        admission,
        resident_fallback,
    } = prepared;
    let LaunchContext {
        store,
        configuration,
        shared,
        management,
        serial,
        cancel,
    } = context;
    tokio::spawn(async move {
        let mut rollback_pins = Vec::new();
        let rollback_description = resident_fallback.or_else(|| {
            restoration
                .as_ref()
                .map(|(profile, _)| profile.description().clone())
        });
        let mut result = async {
            current(&configuration, &expected, stage, &cancel)?;
            if let Some(admission) = &admission {
                admission(&expected)?;
            }

            if let Some(directory) = import_directory {
                phase(&shared, serial, Availability::Verifying, "Verifying and importing the exact compiled local asset bundle; source files remain read-only.")?;
                let imported_profile = execution.description().clone();
                let imported_store = store.clone();
                let imported_configuration = configuration.clone();
                let imported_expected = expected.clone();
                let imported_cancel = cancel.clone();
                tokio::task::spawn_blocking(move || {
                    let check = || current(&imported_configuration, &imported_expected, stage, &imported_cancel);
                    let bundle = airk_store::service::arbiter_assets::OfflineBundle::open(&imported_profile, &directory, check)?;
                    bundle.import(&imported_store, &imported_profile, imported_expected.revision, check)
                }).await.context("Native local bundle import task closed")??;
            }
            if download {
                phase(
                    &shared,
                    serial,
                    Availability::Downloading,
                    "Downloading the exact compiled Arbiter asset parts.",
                )?;
                let source_log = {
                    let state = shared.lock().map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
                    state.source_log.clone()
                };
                super::assets::provision(
                    execution.description(),
                    &store,
                    expected.revision,
                    &cancel,
                    source_log.as_ref(),
                )
                .await?;
            }
            current(&configuration, &expected, stage, &cancel)?;
            let expected = management_snapshot(&configuration, &expected, stage, &cancel)?;
            if expected.stage(stage).enabled
                && expected.stage(stage).selected_profile == Some(manifest)
            {
                let warming_revision = expected.revision;
                let warmed = warm(
                    execution,
                    source,
                    WarmContext {
                        stage,
                        expected: &expected,
                        configuration: &configuration,
                        management: &management,
                        shared: &shared,
                        serial,
                        cancel: &cancel,
                        store: &store,
                        rollback_profile: rollback_description.as_ref(),
                        rollback_pins: &mut rollback_pins,
                        admission: admission.as_ref(),
                    },
                )
                .await;
                if let Err(original) = warmed {
                    // This is one new-input continuation, after warm() has
                    // observed actual session/charge drain. It cannot download,
                    // renew cancellation, or retry unchanged failed inputs.
                    let latest = management_snapshot(&configuration, &expected, stage, &cancel)?;
                    let may_continue = {
                        let state = shared
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
                        state.phase != Availability::Draining && !state.has_private_cuda_assets(manifest)
                    };
                    if !may_continue
                        || cancel.is_cancelled()
                        || latest.revision <= warming_revision
                        || latest.stage(stage) != expected.stage(stage)
                        || latest.installation != expected.installation
                        || latest.catalog != expected.catalog
                    {
                        return Err(original);
                    }
                    let (cached, cached_source) = continuation
                        .take()
                        .context("No further cached stage continuation is admitted")?;
                    let cached_description = cached.description().clone();
                    let cached_store = store.clone();
                    super::assets::owned_asset_operation(async move {
                        cached_store.pin(&cached_description)
                    })
                    .await?;
                    warm(
                        cached,
                        cached_source,
                        WarmContext {
                            stage,
                            expected: &latest,
                            configuration: &configuration,
                            management: &management,
                            shared: &shared,
                            serial,
                            cancel: &cancel,
                            store: &store,
                            rollback_profile: rollback_description.as_ref(),
                            rollback_pins: &mut rollback_pins,
                            admission: admission.as_ref(),
                        },
                    )
                    .await?;
                }
            } else {
                phase(
                    &shared,
                    serial,
                    Availability::Requested,
                    "Verified assets retained; this operation did not enable or enroll Arbiter.",
                )?;
            }
            Ok(())
        }
        .await;
        let drain_incomplete = shared
            .lock()
            .map(|state| state.phase == Availability::Draining)
            .unwrap_or(true);
        if result.is_err()
            && !drain_incomplete
            && let Some(prior) = previous
        {
            let failure = result
                .as_ref()
                .err()
                .map(user_failure_reason)
                .unwrap_or_default();
            let restore_result = async {
                let Some(restored) = rollback_current_intent(
                    &shared,
                    serial,
                    &configuration,
                    &expected,
                    &prior,
                    stage,
                )?
                else {
                    return Ok(());
                };
                if restored.stage(stage).enabled {
                    let (old, old_source) = restoration.take().context(
                        "Previous profile has no currently verified rollback capability",
                    )?;
                    let rollback_cancel = {
                        let mut state = shared
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
                        restore_cancellation(
                            &mut state,
                            serial,
                            restored.revision,
                            old.description().manifest,
                        )?
                    };
                    warm(
                        old,
                        old_source,
                        WarmContext {
                            stage,
                            expected: &restored,
                            configuration: &configuration,
                            management: &management,
                            shared: &shared,
                            serial,
                            cancel: &rollback_cancel,
                            store: &store,
                            rollback_profile: None,
                            rollback_pins: &mut rollback_pins,
                            admission: admission.as_ref(),
                        },
                    )
                    .await?;
                } else if let Ok(handle) = management.existing().await {
                    let latest = stage_snapshot(&configuration, &restored, stage)?;
                    handle.configure(latest.clone())?;
                    release_disabled(&handle, &latest, stage)?;
                }
                Ok::<(), anyhow::Error>(())
            }
            .await;
            if let Ok(mut state) = shared.lock()
                && state.serial == serial
            {
                state.profile = Some(manifest);
            }
            if let Err(error) = restore_result {
                result = Err(anyhow::anyhow!(
                    "Switch failed: {failure}; previous configuration/cached profile restoration also failed: {error:#}"
                ));
            }
        }
        settle_and_resume(store, configuration, shared, management, serial, &result);
    });
}
fn next_pending(
    shared: &Arc<Mutex<State>>,
    settled_serial: u64,
) -> Result<Option<(PreparedLaunch, u64, CancellationToken)>> {
    let mut state = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
    if state.serial != settled_serial {
        return Ok(None);
    }
    if state.stopping {
        state.pending = None;
        state.busy = false;
        state.settled.notify_waiters();
        return Ok(None);
    }
    let Some(prepared) = state.pending.take() else {
        state.busy = false;
        state.settled.notify_waiters();
        return Ok(None);
    };
    state.serial = state
        .serial
        .checked_add(1)
        .context("Arbiter lifecycle revision exhausted")?;
    state.busy = true;
    state.profile = Some(prepared.manifest);
    state.phase = Availability::Requested;
    state.cancel = CancellationToken::new();
    Ok(Some((prepared, state.serial, state.cancel.clone())))
}

fn settle_and_take_pending(
    shared: &Arc<Mutex<State>>,
    settled_serial: u64,
    result: &Result<()>,
) -> Result<Option<(PreparedLaunch, u64, CancellationToken)>> {
    complete(shared, settled_serial, result);
    next_pending(shared, settled_serial)
}
fn settle_and_resume(
    store: Arc<AssetStore>,
    configuration: Arc<airk_store::service::arbiter::ConfigurationStore>,
    shared: Arc<Mutex<State>>,
    management: ArbiterManagement,
    settled_serial: u64,
    result: &Result<()>,
) {
    let next = settle_and_take_pending(&shared, settled_serial, result);
    match next {
        Ok(Some((prepared, serial, cancel))) => spawn_launch(
            prepared,
            LaunchContext {
                store,
                configuration,
                shared,
                management,
                serial,
                cancel,
            },
        ),
        Ok(None) => {}
        Err(error) => {
            if let Ok(mut state) = shared.lock() {
                state.busy = false;
                state.phase = Availability::Failed;
                state.reason = bounded_reason(format!(
                    "Cached latest selection could not be admitted: {error:#}"
                ));
                state.settled.notify_waiters();
            }
        }
    }
}

fn unavailable_stage_status(
    stage: Stage,
    phase: Availability,
    reason: &str,
    observation: Option<airk_inference::inference_engine::pool::arbiter::Observation>,
) -> StageStatus {
    StageStatus {
        selected_profile: None,
        active_profile: observation.as_ref().and_then(|value| value.active_profile),
        availability: if phase == Availability::Draining {
            Availability::Draining
        } else {
            Availability::Failed
        },
        reason: bounded_reason(reason),
        queued: observation.as_ref().map_or(0, |value| value.queued),
        running: observation.as_ref().map_or(0, |value| value.running),
        profiles: stage_models(stage)
            .into_iter()
            .map(|model| ProfileStatus {
                cpu_fit: None,
                qualified_targets: vec![],
                model,
                profile: None,
                availability: Availability::Unqualified,
                reason: "No stage was enabled or initialized by this status read.".into(),
                tasks: Default::default(),
                download_bytes: None,
                memory_reservation_bytes: None,
            })
            .collect(),
    }
}

impl StageOwner {
    pub(super) fn bind_source_diagnostics(&self, log: airk_host::service_logs::Log) -> Result<()> {
        self.lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?
            .source_log = Some(log);
        Ok(())
    }
    pub(super) fn unavailable_status(&self, reason: &str) -> Result<StageStatus> {
        let state = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("Actual Arbiter stage observation is unavailable"))?;
        let observed = state
            .handle
            .as_ref()
            .map(|handle| handle.observation_for(self.stage))
            .transpose()?;
        Ok(unavailable_stage_status(
            self.stage,
            state.phase,
            reason,
            observed,
        ))
    }
    pub(super) fn status(&self) -> Result<StageStatus> {
        let configuration = self.configuration()?;
        let catalog = self.profile_descriptions()?;
        let state = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
        let observation = state
            .handle
            .as_ref()
            .map(|handle| handle.observation_for(self.stage))
            .transpose()?;
        let active = observation.as_ref().and_then(|value| value.active_profile);
        let native_revision_matches = observation
            .as_ref()
            .is_some_and(|value| value.configuration_revision == Some(configuration.revision));
        let mut profiles = Vec::new();
        for model in stage_models(self.stage) {
            let profile = catalog
                .iter()
                .filter(|profile| profile.model == model)
                .find(|profile| {
                    Some(profile.manifest) == configuration.stage(self.stage).selected_profile
                })
                .or_else(|| catalog.iter().find(|profile| profile.model == model));
            let row = if let Some(profile) = profile {
                let admission = self.execution_profile(profile.manifest);
                let fit = profile.cpu_fit.clone().unwrap_or_default();
                let fit_reason = match &profile.cpu_fit {
                    Some(fit) => format!(
                        "Declared optional CPU tier: at least {} usable physical cores; intra-op {}, inter-op {}, sequential {}, spinning {}.",
                        fit.minimum_physical_cores,
                        fit.intra_threads,
                        fit.inter_threads,
                        fit.sequential,
                        fit.spinning
                    ),
                    None => {
                        "No CPU execution tier is declared; no measured CPU fit is claimed.".into()
                    }
                };
                let physical_fit = airk_process::cpu_fit::usable_physical_cores();
                let (availability, reason) = if let Err(error) = admission {
                    (
                        Availability::Unqualified,
                        format!("Native profile admission refused: {error:#}."),
                    )
                } else if !physical_fit
                    .as_ref()
                    .is_some_and(|cores| *cores >= fit.minimum_physical_cores)
                {
                    (
                        Availability::UnsupportedSystem,
                        match physical_fit {
                            Some(cores) => format!(
                                "Native physical-core fit refused: {cores} usable physical cores observed."
                            ),
                            None => {
                                "Native usable physical-core observation is unavailable.".into()
                            }
                        },
                    )
                } else if state.profile == Some(profile.manifest)
                    && (state.busy
                        || matches!(state.phase, Availability::Failed | Availability::Draining))
                {
                    (state.phase, state.reason.clone())
                } else if active == Some(profile.manifest)
                    && native_revision_matches
                    && configuration.stage(self.stage).enabled
                    && configuration.stage(self.stage).selected_profile == Some(profile.manifest)
                {
                    (
                        Availability::Ready,
                        "Verified native profile is resident.".into(),
                    )
                } else if active == Some(profile.manifest) {
                    (Availability::Requested,"Previous native session is retained but is not admitted for the current enabled selection.".into())
                } else if state.has_private_cuda_assets(profile.manifest) {
                    (
                        Availability::Requested,
                        "Verified read-only private CUDA assets are retained for the exact graph-bound fixture.".into(),
                    )
                } else if self.catalog_only_held(profile.manifest)? {
                    (
                        Availability::Requested,
                        "Authenticated read-only private catalog-only assets are held for this evaluation lease.".into(),
                    )
                } else {
                    match self.assets.phase(profile)?{Some(Phase::Ready)=>(Availability::Requested,"Completed provisioning receipt retained; native activation requires re-verification and the selected enabled choice.".into()),Some(Phase::Downloading)|Some(Phase::Failed)|Some(Phase::Canceled)=>(Availability::Failed,"Asset acquisition did not complete. Use a new explicit Provision or verified local Import; Recover does not download.".into()),Some(Phase::Removing)=>(Availability::Failed,"An unfinished asset removal requires local Recover; no download was resumed.".into()),_=>(Availability::MissingAssets,"Explicit provisioning is required.".into())}
                };
                ProfileStatus {
                    cpu_fit: profile.cpu_fit.clone(),
                    qualified_targets: profile.qualified_targets.clone(),
                    model,
                    profile: Some(profile.manifest),
                    availability,
                    reason: bounded_reason(
                        if airk_retrieval::arbiter::runtime::is_local_test_profile(profile) {
                            format!("{LOCAL_TEST_NOTE} {reason} {fit_reason}")
                        } else {
                            format!("{reason} {fit_reason}")
                        },
                    ),
                    tasks: profile.tasks.clone(),
                    download_bytes: Some(profile.assets.iter().try_fold(
                        0_u64,
                        |total, asset| {
                            total
                                .checked_add(asset.bytes)
                                .context("Arbiter asset total overflow")
                        },
                    )?),
                    memory_reservation_bytes: Some(profile.memory_reservation_bytes),
                }
            } else {
                ProfileStatus {
                    cpu_fit: None,
                    qualified_targets: vec![],
                    model,
                    profile: None,
                    availability: Availability::Unqualified,
                    reason: "No release-qualified compiled profile is available for this choice."
                        .into(),
                    tasks: Default::default(),
                    download_bytes: None,
                    memory_reservation_bytes: None,
                }
            };
            profiles.push(row);
        }
        let selected = configuration.stage(self.stage).selected_profile;
        let (availability, reason) = if !configuration.stage(self.stage).enabled {
            if observation.as_ref().is_some_and(|value| {
                value.active_profile.is_some()
                    || value.warming_profile.is_some()
                    || value.running > 0
            }) {
                (
                    Availability::Draining,
                    "Disabled Arbiter native work remains draining.".into(),
                )
            } else {
                (
                    Availability::Disabled,
                    if state.reason.is_empty() {
                        "Optional local Arbiter is disabled.".into()
                    } else {
                        format!("Optional local Arbiter is disabled. {}", state.reason)
                    },
                )
            }
        } else if state.phase == Availability::Draining
            && state.profile == selected
            && observation.as_ref().is_some_and(|value| {
                value.active_profile == selected || value.warming_profile == selected
            })
        {
            (Availability::Draining, state.reason.clone())
        } else if active == selected && selected.is_some() && native_revision_matches {
            let local_test = profiles
                .iter()
                .find(|row| row.profile == selected)
                .is_some_and(|row| row.reason.starts_with(LOCAL_TEST_NOTE));
            (
                Availability::Ready,
                if local_test {
                    format!("{LOCAL_TEST_NOTE} Selected profile is active.")
                } else {
                    "Selected qualified Arbiter profile is active.".into()
                },
            )
        } else {
            profiles
                .iter()
                .find(|row| row.profile == selected)
                .map(|row| (row.availability, row.reason.clone()))
                .unwrap_or((
                    Availability::Unqualified,
                    "Selected profile is absent from the qualified native catalog.".into(),
                ))
        };
        #[cfg(feature = "test-support")]
        let reason = if self.evaluation.is_some() {
            for row in &mut profiles {
                row.reason = bounded_reason(format!(
                    "Private evaluation only; candidate is not release-qualified. {}",
                    row.reason
                ));
            }
            format!("Private evaluation only; candidate is not release-qualified. {reason}")
        } else {
            reason
        };
        ensure!(
            self.configuration()? == configuration,
            "Arbiter status configuration changed before delivery"
        );
        Ok(StageStatus {
            selected_profile: selected,
            active_profile: active,
            availability,
            reason: bounded_reason(reason),
            queued: observation.as_ref().map_or(0, |value| value.queued),
            running: observation.as_ref().map_or(0, |value| value.running),
            profiles,
        })
    }
    fn lifecycle_admission(&self) -> DeferredAdmission {
        let gate = self.gate.clone();
        let ownership = self.ownership.clone();
        let namespace = self.namespace;
        let stage = self.stage;
        Box::new(move |configuration| {
            validate_configuration_enrollments(&gate, &ownership, namespace, configuration, stage)
        })
    }
    fn resident_description(&self) -> Result<Option<Profile>> {
        let handle = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?
            .handle
            .clone();
        Ok(handle
            .and_then(|handle| handle.observation_for(self.stage).ok())
            .and_then(|observation| observation.active_profile)
            .and_then(|manifest| self.execution_profile(manifest).ok())
            .map(|profile| profile.description().clone()))
    }
    fn launch(
        &self,
        expected: Configuration,
        manifest: Sha256Digest,
        download: bool,
        previous: Option<Configuration>,
        management: ArbiterManagement,
        effect_check: Option<&airk_https::admin_plane::NativeDispatchCheck>,
    ) -> Result<()> {
        let execution = self.execution_profile(manifest)?;
        execution.description().validate_stage(self.stage)?;
        ensure!(
            !download || execution.release_qualified(),
            "Private evaluation assets require descriptor seeding, not product downloads"
        );
        let source = self.asset_source(manifest)?;
        let restoration = previous
            .as_ref()
            .filter(|prior| prior.stage(self.stage).enabled)
            .and_then(|prior| prior.stage(self.stage).selected_profile)
            .and_then(|old| {
                self.execution_profile(old)
                    .ok()
                    .zip(self.asset_source(old).ok())
            });
        let resident_fallback = self.resident_description()?;
        let continuation = Some((
            self.execution_profile(manifest)?,
            self.asset_source(manifest)?,
        ));
        let (serial, cancel) = start(&self.lifecycle, manifest, effect_check)?;
        spawn_launch(
            PreparedLaunch {
                stage: self.stage,
                expected,
                manifest,
                download,
                import_directory: None,
                previous,
                execution,
                source,
                restoration,
                continuation,
                admission: Some(self.lifecycle_admission()),
                resident_fallback,
            },
            LaunchContext {
                store: self.assets.clone(),
                configuration: self.configuration.clone(),
                shared: self.lifecycle.clone(),
                management,
                serial,
                cancel,
            },
        );
        Ok(())
    }
    pub(super) fn launch_import(
        &self,
        expected: Configuration,
        manifest: Sha256Digest,
        directory: std::path::PathBuf,
        management: ArbiterManagement,
    ) -> Result<()> {
        let execution = self.execution_profile(manifest)?;
        ensure!(
            execution.release_qualified(),
            "Offline import requires the actual compiled release-qualified profile"
        );
        execution.description().validate_stage(self.stage)?;
        let source = self.asset_source(manifest)?;
        let continuation = Some((
            self.execution_profile(manifest)?,
            self.asset_source(manifest)?,
        ));
        let resident_fallback = self.resident_description()?;
        let (serial, cancel) = start(&self.lifecycle, manifest, None)?;
        spawn_launch(
            PreparedLaunch {
                stage: self.stage,
                expected,
                manifest,
                download: false,
                import_directory: Some(directory),
                previous: None,
                execution,
                source,
                restoration: None,
                continuation,
                admission: Some(self.lifecycle_admission()),
                resident_fallback,
            },
            LaunchContext {
                store: self.assets.clone(),
                configuration: self.configuration.clone(),
                shared: self.lifecycle.clone(),
                management,
                serial,
                cancel,
            },
        );
        Ok(())
    }
    fn launch_remove(
        &self,
        expected: Configuration,
        manifest: Sha256Digest,
        management: ArbiterManagement,
        serial: u64,
        cancel: CancellationToken,
    ) -> Result<()> {
        let stage = self.stage;
        let shared = self.lifecycle.clone();
        let store = self.assets.clone();
        let configuration = self.configuration.clone();
        let retained = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?
            .handle
            .clone();
        tokio::spawn(async move {
            let result=async {
                current(&configuration,&expected,stage,&cancel)?;
                let handle=match retained {Some(handle)=>Some(handle),None=>management.existing().await.ok()};
                if let Some(handle)=handle {
                    let latest = management_snapshot(&configuration, &expected, stage, &cancel)?;
                    handle.configure(latest.clone())?;
                    if !latest.stage(stage).enabled {release_disabled(&handle,&latest,stage)?;}
                    let charge = handle.resident(manifest).ok().map(|resident|resident.reservation().release_observer());
                    handle.release_profile(manifest)?;
                    phase(&shared,serial,Availability::Draining,"Waiting for actual native completion before asset removal.")?;
                    let deadline=Instant::now()+Duration::from_secs(30);
                    handle.wait_profile_released(manifest,deadline).await?;
                    if let Some(charge)=charge {tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),charge.wait()).await.context("Removed profile's actual final charge remains draining")??;}
                    phase(&shared,serial,Availability::Verifying,"Native session and final memory charge released; verifying owned disk cleanup.")?;
                }
                current(&configuration,&expected,stage,&cancel)?;
                if store.owned_observation(manifest)?.is_some() {
                    let removal_store = store.clone();
                    tokio::task::spawn_blocking(move||removal_store.remove_owned(manifest,expected.revision)).await.context("Arbiter removal task closed")??;
                }
                phase(&shared,serial,Availability::MissingAssets,"Declared verified assets removed; interrupted recovery stages remain preserved.")?;
                Ok(())
            }.await;
            settle_and_resume(store, configuration, shared, management, serial, &result);
        });
        Ok(())
    }
    pub(in crate::service_runtime) async fn manage(
        &self,
        request: ManagementRequest,
        supervisor: &Supervisor,
        effect_check: Option<airk_https::admin_plane::NativeDispatchCheck>,
    ) -> Result<StageStatus> {
        let management = supervisor.arbiter_management();
        match request {
            ManagementRequest::StatusDevice
            | ManagementRequest::InitializeDevice { .. }
            | ManagementRequest::MigrateDevice { .. }
            | ManagementRequest::ConfigureDevice { .. }
            | ManagementRequest::ProvisionDevice { .. }
            | ManagementRequest::CancelDevice { .. }
            | ManagementRequest::RemoveDevice { .. }
            | ManagementRequest::RecoverDevice { .. } => {
                anyhow::bail!("current device operation requires typed status3 native owner")
            }
            ManagementRequest::Status => {}
            ManagementRequest::Initialize { .. } => {
                anyhow::bail!("Initialization belongs to the common Arbiter owner")
            }
            ManagementRequest::Configure {
                stage,
                expected_revision,
                enabled,
                selected_profile,
                indexes,
            } => {
                ensure!(stage == self.stage, "Arbiter management stage differs");
                let previous = self.configuration()?;
                ensure!(
                    previous.revision == expected_revision,
                    "Arbiter configuration revision changed"
                );
                let mut candidate = previous.clone();
                candidate.revision = expected_revision
                    .checked_add(1)
                    .context("Arbiter revision exhausted")?;
                *candidate.stage_mut(stage) = StageConfiguration {
                    enabled,
                    selected_profile,
                    indexes,
                };
                self.validate_enrollments(&candidate)?;
                if let Some(profile) = selected_profile {
                    self.execution_profile(profile)?
                        .description()
                        .validate_stage(self.stage)?;
                }
                let (selected, handle, busy) = {
                    let mut state = self
                        .lifecycle
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
                    let selected = commit_configure(
                        &mut state,
                        &self.configuration,
                        expected_revision,
                        &candidate,
                        || super::super::native_admin::check_effect(effect_check.as_ref()),
                    )?;
                    (selected, state.handle.clone(), state.busy)
                };
                if let Some(handle) = handle {
                    let latest = stage_snapshot(&self.configuration, &selected, self.stage)?;
                    handle.configure(latest.clone())?;
                    if !latest.stage(self.stage).enabled {
                        release_disabled(&handle, &latest, self.stage)?;
                    }
                }
                if selected.stage(self.stage).enabled {
                    let manifest = selected
                        .stage(self.stage)
                        .selected_profile
                        .context("Arbiter profile missing")?;
                    let profile = self.execution_profile(manifest)?;
                    let has_private_assets = self
                        .lifecycle
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?
                        .has_private_cuda_assets(manifest);
                    let assets_ready = has_private_assets
                        || self.catalog_only_held(manifest)?
                        || self.assets.phase(profile.description())? == Some(Phase::Ready);
                    if cached_intent_eligible(&selected, self.stage, assets_ready) {
                        if busy {
                            let source = self.asset_source(manifest)?;
                            let restoration = previous
                                .stage(self.stage)
                                .enabled
                                .then_some(previous.stage(self.stage).selected_profile)
                                .flatten()
                                .and_then(|old| {
                                    self.execution_profile(old)
                                        .ok()
                                        .zip(self.asset_source(old).ok())
                                });
                            let resident_fallback = self.resident_description()?;
                            let prepared = PreparedLaunch {
                                stage: self.stage,
                                expected: selected.clone(),
                                manifest,
                                download: false,
                                import_directory: None,
                                previous: Some(previous.clone()),
                                execution: profile,
                                source,
                                restoration,
                                continuation: Some((
                                    self.execution_profile(manifest)?,
                                    self.asset_source(manifest)?,
                                )),
                                resident_fallback,
                                admission: Some(self.lifecycle_admission()),
                            };
                            let mut state = self
                                .lifecycle
                                .lock()
                                .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
                            ensure!(
                                self.configuration()? == selected,
                                "Arbiter selection changed before cached intent admission"
                            );
                            if state.busy {
                                state.pending = Some(prepared);
                            } else {
                                drop(state);
                                self.launch(
                                    selected,
                                    manifest,
                                    false,
                                    Some(previous),
                                    management,
                                    None,
                                )?;
                            }
                        } else {
                            self.launch(
                                selected,
                                manifest,
                                false,
                                Some(previous),
                                management,
                                None,
                            )?;
                        }
                    }
                }
            }
            ManagementRequest::Provision {
                expected_revision,
                profile,
            } => {
                let configuration = self.configuration()?;
                ensure!(
                    configuration.revision == expected_revision,
                    "Arbiter configuration revision changed"
                );
                self.execution_profile(profile)?
                    .description()
                    .validate_stage(self.stage)?;
                self.launch(
                    configuration,
                    profile,
                    true,
                    None,
                    management,
                    effect_check.as_ref(),
                )?;
            }
            ManagementRequest::Recover {
                expected_revision,
                stage,
            } => {
                ensure!(stage == self.stage, "Arbiter recovery stage differs");
                let configuration = self.configuration()?;
                ensure!(
                    configuration.revision == expected_revision,
                    "Arbiter configuration revision changed"
                );
                let mut pending = std::collections::BTreeMap::new();
                for profile in self
                    .profile_descriptions()?
                    .into_iter()
                    .filter(|profile| profile.declared_stage() == Some(self.stage))
                {
                    if let Some(observed) = self.assets.observation(&profile)?
                        && matches!(observed.phase, Phase::Downloading | Phase::Failed)
                        && observed.configuration_revision == expected_revision
                    {
                        pending.insert(profile.manifest, observed.phase);
                    }
                }
                for manifest in self
                    .assets
                    .pending_owned_removals_for(self.stage, expected_revision)?
                {
                    pending.insert(manifest, Phase::Removing);
                }
                if let Some(manifest) = recovery_removal(&pending)? {
                    let (serial, cancel) = start(&self.lifecycle, manifest, effect_check.as_ref())?;
                    self.launch_remove(configuration, manifest, management, serial, cancel)?;
                } else {
                    let manifest = configuration.stage(self.stage).selected_profile.context(
                        "Recovery requires a named prior operation or selected retained profile",
                    )?;
                    let profile = self.execution_profile(manifest)?;
                    ensure!(
                        self.assets.phase(profile.description())? == Some(Phase::Ready),
                        "No verified cached profile; use explicit Provision or owner-only Import after incomplete acquisition, cancellation or removal"
                    );
                    self.launch(
                        configuration,
                        manifest,
                        false,
                        None,
                        management,
                        effect_check.as_ref(),
                    )?;
                }
            }
            ManagementRequest::Cancel {
                expected_revision,
                stage,
            } => {
                ensure!(stage == self.stage, "Arbiter cancellation stage differs");
                ensure!(
                    self.configuration()?.revision == expected_revision,
                    "Arbiter configuration revision changed"
                );
                let mut state = self
                    .lifecycle
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
                ensure!(
                    self.configuration()?.revision == expected_revision,
                    "Arbiter cancellation revision changed while queued"
                );
                super::super::native_admin::check_effect(effect_check.as_ref())?;
                cancel_current(&mut state);
                if state.busy
                    && let (Some(handle), Some(profile)) = (&state.handle, state.profile)
                {
                    handle.release_profile(profile)?;
                }
            }
            ManagementRequest::Remove {
                expected_revision,
                profile,
            } => {
                let mut configuration = self.configuration()?;
                ensure!(
                    configuration.revision == expected_revision,
                    "Arbiter configuration revision changed"
                );
                if self.execution_profile(profile).is_err() {
                    ensure!(
                        self.assets.owned_observation(profile)?.is_some(),
                        "Withdrawn profile has no valid owned cleanup receipt"
                    );
                }
                let (serial, cancel) = start(&self.lifecycle, profile, effect_check.as_ref())?;
                if configuration.stage(self.stage).enabled
                    && configuration.stage(self.stage).selected_profile == Some(profile)
                {
                    configuration.stage_mut(self.stage).enabled = false;
                    configuration.revision = expected_revision
                        .checked_add(1)
                        .context("Arbiter revision exhausted")?;
                    match self.configuration.apply(expected_revision, &configuration) {
                        Ok(value) => configuration = value,
                        Err(error) => {
                            settle_and_resume(
                                self.assets.clone(),
                                self.configuration.clone(),
                                self.lifecycle.clone(),
                                management,
                                serial,
                                &Err(anyhow::anyhow!("Arbiter removal configuration CAS failed")),
                            );
                            return Err(error);
                        }
                    }
                }
                self.launch_remove(configuration, profile, management, serial, cancel)?;
            }
        }
        self.status()
    }
    /// Counts one model call of this stage for the idle rule (plan 12.5). The
    /// first call after an idle unload starts the reload and is answered at
    /// once from the current, not yet Ready, stage: it never waits for the
    /// reload and so never times out because of it.
    pub(super) async fn begin_model_use(
        &self,
        supervisor: &Supervisor,
    ) -> Result<super::idle::UseGuard> {
        let start = super::idle::begin_use(&self.lifecycle)?;
        if let Some(manifest) = start.reload {
            let serial = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?
                .serial;
            if let Err(error) = self.reload_after_idle(supervisor).await {
                // A refused reload (for example, a Configure claimed the
                // lifecycle first) keeps the marker for the next call.
                super::idle::restore_marker(&self.lifecycle, manifest);
                self.record_restore_failure(serial, &error);
            }
        }
        Ok(start.guard)
    }
    /// Sets this stage's idle unload time; `None` or zero never unloads.
    /// Only the private evaluation sets it today; the product setting is a
    /// persisted-configuration change recorded as a follow-up.
    #[cfg(feature = "test-support")]
    pub(super) fn set_idle_unload(&self, timeout: Option<Duration>) -> Result<()> {
        self.lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?
            .idle
            .timeout = timeout;
        super::idle::ensure_watch(&self.lifecycle);
        Ok(())
    }
    #[cfg(feature = "test-support")]
    pub(in crate::service_runtime) async fn restore_verified(
        &self,
        supervisor: &Supervisor,
    ) -> Result<()> {
        let serial = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .serial;
        if let Err(error) = self.try_restore_verified(supervisor).await {
            self.record_restore_failure(serial, &error);
        }
        Ok(())
    }
    /// Records a refused restore as Failed only when no other operation has
    /// claimed or changed the lifecycle since the restore began; otherwise
    /// that operation owns the phase and its reason.
    fn record_restore_failure(&self, serial: u64, error: &anyhow::Error) {
        let mut state = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if restore_failure_applies(state.serial, serial, state.busy) {
            state.phase = Availability::Failed;
            state.reason = bounded_reason(format!(
                "Optional Arbiter restore refused; ordinary service remains available: {error:#}"
            ));
        }
    }
    #[cfg(feature = "test-support")]
    pub(super) async fn try_restore_verified(&self, supervisor: &Supervisor) -> Result<()> {
        let configuration = self.configuration()?;
        if !configuration.stage(self.stage).enabled {
            return Ok(());
        }
        let Some(manifest) = configuration.stage(self.stage).selected_profile else {
            return Ok(());
        };
        let Ok(profile) = self.execution_profile(manifest) else {
            return Ok(());
        };
        let private_device = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?
            .has_private_cuda_assets(manifest);
        if private_device
            || self.catalog_only_held(manifest)?
            || self.assets.phase(profile.description())? == Some(Phase::Ready)
        {
            self.validate_enrollments(&configuration)?;
            self.launch(
                configuration,
                manifest,
                false,
                None,
                supervisor.arbiter_management(),
                None,
            )?;
        }
        Ok(())
    }
}
pub(super) struct StageShutdown {
    native: Option<NativeShutdown>,
    charges: Vec<airk_kernel::service_resources::OptionalRelease>,
    capture_error: Option<anyhow::Error>,
}
struct NativeShutdown {
    handle: Handle,
    stage: Stage,
    targets: Vec<Sha256Digest>,
}
fn shutdown_targets(
    latest: Option<Sha256Digest>,
    active: Option<Sha256Digest>,
    warming: Option<Sha256Digest>,
) -> Vec<Sha256Digest> {
    let mut targets = [latest, active, warming]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    targets.sort();
    targets.dedup();
    targets
}
fn shutdown_observation_live(deadline: Instant) -> Result<()> {
    ensure!(
        tokio::time::Instant::now() < tokio::time::Instant::from_std(deadline),
        "Original Arbiter shutdown observation cutoff expired; actual work remains owned"
    );
    Ok(())
}
fn begin_shutdown(shared: &Arc<Mutex<State>>, stage: Stage) -> Result<StageShutdown> {
    let (handle, latest, known_targets, mut charges, may_have_native) = {
        let mut state = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
        state.stopping = true;
        state.pending = None;
        (
            state.handle.clone(),
            state.profile,
            state
                .charges
                .iter()
                .map(|(manifest, _)| *manifest)
                .collect::<Vec<_>>(),
            state
                .charges
                .iter()
                .map(|(_, charge)| charge.clone())
                .collect::<Vec<_>>(),
            matches!(
                state.phase,
                Availability::Ready | Availability::Warming | Availability::Draining
            ),
        )
    };
    let mut capture_error = None;
    let mut native = None;
    if let Some(handle) = handle {
        match handle.observation_for(stage) {
            Ok(observed) => {
                let mut targets =
                    shutdown_targets(latest, observed.active_profile, observed.warming_profile);
                targets.extend(known_targets.iter().copied());
                targets.sort();
                targets.dedup();
                if let Some(active) = observed.active_profile {
                    // Capture the ACTUAL resident, including retained rollback,
                    // before release enqueue can revoke resident admission.
                    match handle.resident(active) {
                        Ok(resident) => charges.push(resident.reservation().release_observer()),
                        Err(error) if !known_targets.contains(&active) => {
                            capture_error = Some(error.context(
                                "Actual resident charge unavailable before shutdown release",
                            ))
                        }
                        Err(_) => {} // Original authentic observer already owns this release proof.
                    }
                }
                if observed
                    .warming_profile
                    .is_some_and(|warming| !known_targets.contains(&warming))
                {
                    capture_error = Some(anyhow::anyhow!(
                        "Native warm has no original charge observation owner"
                    ));
                }
                native = Some(NativeShutdown {
                    handle,
                    stage,
                    targets,
                });
            }
            Err(error) => {
                capture_error =
                    Some(error.context("Actual native stage observation unavailable at shutdown"))
            }
        }
    }
    // Cancellation comes AFTER capturing any actual resident charge observer.
    // A stopping fence already prevents restoration and future admission.
    {
        let mut state = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
        cancel_current(&mut state);
        state.cancel.cancel();
    }
    if let Some(native) = &native {
        for target in &native.targets {
            if let Err(error) = native.handle.release_profile(*target) {
                capture_error = Some(error.context("Native shutdown release was not accepted"));
            }
        }
    }
    if may_have_native && charges.is_empty() {
        capture_error = Some(anyhow::anyhow!(
            "Actual native charge release has no retained observation proof"
        ));
    }
    Ok(StageShutdown {
        native,
        charges,
        capture_error,
    })
}

async fn wait_for_settlement(shared: &Arc<Mutex<State>>, deadline: Instant) -> Result<()> {
    let settled = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?
        .settled
        .clone();
    loop {
        shutdown_observation_live(deadline)?;
        let observation = settled.notified();
        tokio::pin!(observation);
        // Register before checking state, including completion before first poll.
        observation.as_mut().enable();
        {
            let state = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
            ensure!(state.stopping, "Arbiter shutdown was not fenced");
            if !state.busy && state.device_tasks == 0 {
                return Ok(());
            }
        }
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), observation)
            .await
            .context(
                "Original Arbiter shutdown cutoff expired before actual lifecycle settlement",
            )?;
    }
}
async fn wait_shutdown_charges(
    charges: Vec<airk_kernel::service_resources::OptionalRelease>,
    deadline: Instant,
) -> Result<()> {
    for charge in charges {
        shutdown_observation_live(deadline)?;
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), charge.wait())
            .await
            .context("Original Arbiter shutdown cutoff expired before actual charge release")??;
        shutdown_observation_live(deadline)?;
    }
    Ok(())
}
async fn finish_shutdown(
    shared: &Arc<Mutex<State>>,
    shutdown: StageShutdown,
    deadline: Instant,
) -> Result<()> {
    let StageShutdown {
        native,
        charges,
        capture_error,
    } = shutdown;
    wait_for_settlement(shared, deadline).await?;
    if let Some(error) = capture_error {
        return Err(error);
    }
    if let Some(native) = &native {
        for target in &native.targets {
            shutdown_observation_live(deadline)?;
            native
                .handle
                .wait_profile_released(*target, deadline)
                .await?;
        }
    }
    wait_shutdown_charges(charges, deadline).await?;
    if let Some(native) = native {
        let actual = native.handle.observation_for(native.stage)?;
        ensure!(
            actual.active_profile.is_none()
                && actual.warming_profile.is_none()
                && actual.queued == 0
                && actual.running == 0,
            "Actual native stage work remains after shutdown release observations"
        );
    }
    shutdown_observation_live(deadline)
}

impl StageOwner {
    pub(super) fn begin_shutdown(&self) -> Result<StageShutdown> {
        begin_shutdown(&self.lifecycle, self.stage)
    }
    pub(super) async fn finish_shutdown(
        &self,
        shutdown: StageShutdown,
        deadline: Instant,
    ) -> Result<()> {
        finish_shutdown(&self.lifecycle, shutdown, deadline).await
    }
}

#[cfg(test)]
mod shutdown_tests;

impl Drop for StageOwner {
    fn drop(&mut self) {
        if let Ok(mut state) = self.lifecycle.lock() {
            state.pending = None;
            state.cancel.cancel();
            if let (Some(handle), Some(profile)) = (&state.handle, state.profile) {
                let _ = handle.release_profile(profile);
            }
        }
    }
}

#[cfg(all(test, feature = "test-support"))]
pub(super) fn observe_warm(
    shared: &Arc<Mutex<State>>,
    observer: super::warm_observer::Observer,
) -> Result<()> {
    shared
        .lock()
        .map_err(|_| anyhow::anyhow!("warm observer lifecycle unavailable"))?
        .warm_observer = Some(observer);
    Ok(())
}
#[cfg(all(test, feature = "test-support"))]
struct WarmCapture {
    observer: super::warm_observer::Observer,
    stage: Stage,
    expected: Configuration,
    manifest: Sha256Digest,
    serial: u64,
    shared: Arc<Mutex<State>>,
    activation_path: super::warm_observer::ActivationPath,
    native_profile: Option<Sha256Digest>,
    native_revision: Option<u64>,
    cancel: CancellationToken,
    captured_configure_revision: u64,
    released_prior: Option<Sha256Digest>,
    release_ms: Option<u64>,
    _guard: super::warm_observer::Guard,
}
#[cfg(all(test, feature = "test-support"))]
impl WarmCapture {
    fn capture(context: &WarmContext<'_>, manifest: Sha256Digest) -> Option<Self> {
        let state = context.shared.lock().ok()?;
        let observer = state.warm_observer.clone()?;
        let captured_configure_revision = state.accepted_configure_revision;
        drop(state);
        let guard = observer.begin()?;
        Some(Self {
            observer,
            stage: context.stage,
            expected: context.expected.clone(),
            manifest,
            serial: context.serial,
            shared: context.shared.clone(),
            cancel: context.cancel.clone(),
            activation_path: super::warm_observer::ActivationPath::NotAdmitted,
            native_profile: None,
            native_revision: None,
            captured_configure_revision,
            released_prior: None,
            release_ms: None,
            _guard: guard,
        })
    }
    fn finish(mut self, result: &Result<()>) {
        let Ok(state) = self.shared.try_lock() else {
            return;
        };
        let (current, accepted, phase) =
            (state.serial, state.accepted_configure_revision, state.phase);
        drop(state);
        let observed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok());
        let Some(observed) = observed else { return };
        self.observer.emit(super::warm_observer::Record {
            schema: "airk.private.arbiter-warm-completion/1".into(),
            observer_id: self.observer.id(),
            sequence: 0,
            observed_unix_ms: observed,
            stage: self.stage,
            profile_manifest: self.manifest,
            expected_configuration: self.expected,
            activation_path: self.activation_path,
            lifecycle_serial: self.serial,
            current_lifecycle_serial: current,
            accepted_configure_revision: accepted,
            phase,
            captured_configure_revision: self.captured_configure_revision,
            active_profile: self.native_profile,
            native_configuration_revision: self.native_revision,
            canceled: self.cancel.is_cancelled(),
            operation_succeeded: result.is_ok(),
            outcome: super::warm_observer::Outcome::Failed,
            error: result
                .as_ref()
                .err()
                .map(|error| format!("{error:#}").chars().take(1024).collect()),
            released_prior: self.released_prior,
            release_ms: self.release_ms,
        });
        self._guard.complete();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_switch_releases_a_different_resident_before_reserving() {
        let digest = |byte| Sha256Digest::from_bytes([byte; 32]);
        let (compact, large, cpu) = (digest(1), digest(2), digest(3));
        // A private device resident is released before the next profile loads,
        // whether that profile is the other device entry or a CPU profile.
        assert_eq!(device_switch_release(Some(compact), large, true), Some(compact));
        assert_eq!(device_switch_release(Some(large), compact, true), Some(large));
        assert_eq!(device_switch_release(Some(large), cpu, true), Some(large));
        // The same profile, no resident, or a CPU resident: no early release;
        // the budget-driven drain still applies.
        assert_eq!(device_switch_release(Some(large), large, true), None);
        assert_eq!(device_switch_release(None, large, true), None);
        assert_eq!(device_switch_release(Some(cpu), large, false), None);
        assert_eq!(device_switch_release(Some(cpu), digest(4), false), None);
    }
    #[test]
    fn a_refused_restore_never_overwrites_a_newer_operation() {
        assert!(restore_failure_applies(4, 4, false));
        // A Configure claimed the lifecycle meanwhile: it owns the phase.
        assert!(!restore_failure_applies(4, 4, true));
        assert!(!restore_failure_applies(5, 4, false));
    }
    #[test]
    fn gpu_revocation_releases_every_private_entry_the_owner_holds() {
        let digest = |byte| Sha256Digest::from_bytes([byte; 32]);
        let (compact, large, other) = (digest(1), digest(2), digest(9));
        let entries = [large, compact];
        assert_eq!(
            private_release_targets(entries, Some(compact), None, None),
            vec![compact]
        );
        // A warm of the other entry in progress is released too.
        assert_eq!(
            private_release_targets(entries, Some(compact), Some(large), None),
            vec![compact, large]
        );
        // The selected entry whose operation is still running is a target.
        assert_eq!(
            private_release_targets(entries, None, None, Some(large)),
            vec![large]
        );
        // A resident that is not a private entry is never touched here.
        assert!(private_release_targets(entries, Some(other), Some(other), None).is_empty());
        assert_eq!(
            private_release_targets(entries, Some(large), Some(large), Some(large)),
            vec![large]
        );
    }
    #[test]
    fn initialization_status_formatter_retains_actual_native_observation_and_unknown_drain_phase()
    -> Result<()> {
        let profile = Sha256Digest::from_bytes([34; 32]);
        // This exercises metadata projection, not a model/runtime qualification.
        let observed = airk_inference::inference_engine::pool::arbiter::Observation {
            configuration_revision: Some(9),
            active_profile: Some(profile),
            warming_profile: None,
            queued: 3,
            running: 1,
        };
        let status = unavailable_stage_status(
            Stage::Focus,
            Availability::Draining,
            &"λ".repeat(1024),
            Some(observed),
        );
        assert_eq!(status.active_profile, Some(profile));
        assert_eq!((status.queued, status.running), (3, 1));
        assert_eq!(status.availability, Availability::Draining);
        assert!(status.selected_profile.is_none());
        assert_eq!(status.reason.len(), 1024);
        status.validate(Stage::Focus, &StageConfiguration::default())?;
        let no_handle = unavailable_stage_status(
            Stage::Context,
            Availability::Requested,
            "No native handle has been created",
            None,
        );
        assert_eq!((no_handle.queued, no_handle.running), (0, 0));
        assert!(no_handle.active_profile.is_none());
        assert_eq!(no_handle.availability, Availability::Failed);
        Ok(())
    }
    #[test]
    fn known_acquisition_cancellation_settles_but_unknown_native_drain_is_retained() -> Result<()> {
        for actual_drain in [false, true] {
            let shared = Arc::new(Mutex::new(State::default()));
            let (serial, original_cancel) =
                start(&shared, Sha256Digest::from_bytes([33; 32]), None)?;
            phase(
                &shared,
                serial,
                if actual_drain {
                    Availability::Draining
                } else {
                    Availability::Downloading
                },
                "original owned operation",
            )?;
            cancel_current(&mut shared.lock().unwrap());
            assert!(original_cancel.is_cancelled());
            assert_eq!(shared.lock().unwrap().serial, serial);
            complete(
                &shared,
                serial,
                &Err(anyhow::anyhow!("original operation returned")),
            );
            let settled = shared.lock().unwrap();
            assert!(!settled.busy);
            assert_eq!(
                settled.phase,
                if actual_drain {
                    Availability::Draining
                } else {
                    Availability::Requested
                }
            );
        }
        Ok(())
    }
    #[test]
    fn recovery_retains_removal_intent_and_never_promotes_incomplete_acquisition_to_download()
    -> Result<()> {
        let profile = Sha256Digest::from_bytes([31; 32]);
        assert_eq!(
            recovery_removal(&Default::default())?,
            None,
            "no incomplete operation permits only the separately verified cached path"
        );
        assert_eq!(
            recovery_removal(&std::collections::BTreeMap::from([(
                profile,
                Phase::Removing
            )]))?,
            Some(profile)
        );
        for phase in [
            Phase::Downloading,
            Phase::Failed,
            Phase::Canceled,
            Phase::Removed,
        ] {
            assert!(
                recovery_removal(&std::collections::BTreeMap::from([(profile, phase)])).is_err(),
                "incomplete/terminal acquisition cannot acquire network authority: {phase:?}"
            );
        }
        assert!(
            recovery_removal(&std::collections::BTreeMap::from([
                (profile, Phase::Removing),
                (Sha256Digest::from_bytes([32; 32]), Phase::Removing)
            ]))
            .is_err()
        );
        let mut cached = Configuration::disabled(
            Sha256Digest::from_bytes([1; 32]),
            airk_core::identity::Incarnation::from_bytes([2; 16]),
        );
        cached.focus.enabled = true;
        cached.focus.selected_profile = Some(profile);
        cached.focus.indexes = vec![Enrollment {
            index_id: airk_core::index_catalog::IndexId::from_identity(
                airk_core::identity::Incarnation::from_bytes([3; 16]),
            ),
            tasks: std::collections::BTreeSet::from([Task::FocusRank]),
        }];
        assert!(cached_intent_eligible(&cached, Stage::Focus, true));
        assert!(!cached_intent_eligible(&cached, Stage::Focus, false));
        assert!(!cached_intent_eligible(&cached, Stage::Context, true));
        Ok(())
    }
    #[test]
    fn explicit_lifecycle_preserves_original_operation_and_refuses_stale_completion() -> Result<()>
    {
        let state = Arc::new(Mutex::new(State::default()));
        let profile = Sha256Digest::from_bytes([1; 32]);
        let (first, cancel) = start(&state, profile, None)?;
        assert!(start(&state, profile, None).is_err());
        cancel.cancel();
        assert!(phase(&state, first, Availability::Ready, "wrong success").is_err());
        complete(&state, first, &Err(anyhow::anyhow!("canceled")));
        let (second, _) = start(&state, Sha256Digest::from_bytes([2; 32]), None)?;
        assert_ne!(first, second);
        assert!(phase(&state, first, Availability::Ready, "stale").is_err());
        complete(&state, first, &Ok(()));
        assert!(state.lock().unwrap().busy);
        complete(&state, second, &Ok(()));
        assert!(!state.lock().unwrap().busy);
        Ok(())
    }

    #[test]
    fn identical_configure_intent_blocks_old_rollback_before_pending_preparation() -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = Arc::new(airk_store::service::arbiter::ConfigurationStore::open(
            Arc::new(airk_store::service::ServiceLease::acquire(root.path())?),
            Sha256Digest::from_bytes([1; 32]),
            airk_core::identity::Incarnation::from_bytes([2; 16]),
        )?);
        let prior = store.load()?;
        let state = Arc::new(Mutex::new(State::default()));
        let mut candidate = prior.clone();
        candidate.revision = 2;
        candidate.context.selected_profile = Some(Sha256Digest::from_bytes([3; 32]));
        assert!(
            commit_configure(
                &mut state.lock().unwrap(),
                &store,
                1,
                &candidate,
                || anyhow::bail!("effect revoked")
            )
            .is_err()
        );
        assert_eq!(store.load()?, prior);
        assert_eq!(state.lock().unwrap().accepted_configure_revision, 0);
        assert!(!state.lock().unwrap().cancel.is_cancelled());
        let selected =
            commit_configure(&mut state.lock().unwrap(), &store, 1, &candidate, || Ok(()))?;
        let (serial, original_cancel) = start(&state, Sha256Digest::from_bytes([3; 32]), None)?;
        let (allow_failure, barrier) = std::sync::mpsc::channel();
        let old = {
            let state = state.clone();
            let store = store.clone();
            let selected = selected.clone();
            let prior = prior.clone();
            std::thread::spawn(move || -> Result<Option<Configuration>> {
                barrier.recv_timeout(Duration::from_secs(5))?;
                rollback_current_intent(&state, serial, &store, &selected, &prior, Stage::Context)
            })
        };
        let mut identical = selected.clone();
        identical.revision = 3;
        let accepted =
            commit_configure(&mut state.lock().unwrap(), &store, 2, &identical, || Ok(()))?;
        assert!(original_cancel.is_cancelled());
        // Exact vulnerable interval: Configure CAS/cancel has completed but no
        // pending cached intent exists yet. No sleeps, worker or NN required.
        assert!(state.lock().unwrap().pending.is_none());
        allow_failure.send(())?;
        assert_eq!(
            old.join()
                .map_err(|_| anyhow::anyhow!("rollback thread panicked"))??,
            None
        );
        assert_eq!(store.load()?, accepted);
        complete(
            &state,
            serial,
            &Err(anyhow::anyhow!("original warm canceled")),
        );
        let (next, _) = start(&state, Sha256Digest::from_bytes([3; 32]), None)?;
        let mut other = accepted.clone();
        other.revision = 4;
        other.focus.selected_profile = Some(Sha256Digest::from_bytes([4; 32]));
        let other = store.apply(3, &other)?;
        let restored =
            rollback_current_intent(&state, next, &store, &accepted, &prior, Stage::Context)?
                .context("unrelated-stage advance blocked ordinary rollback")?;
        assert_eq!(restored.revision, 5);
        assert_eq!(restored.context, prior.context);
        assert_eq!(restored.focus, other.focus);
        let old_profile = state.lock().unwrap().profile;
        let mut newer = restored.clone();
        newer.revision = 6;
        let newer = commit_configure(&mut state.lock().unwrap(), &store, 5, &newer, || Ok(()))?;
        // A fresh identical Configure between rollback CAS and token renewal
        // must not have its cancellation replaced by the old restoration.
        let mut guard = state.lock().unwrap();
        assert!(guard.cancel.is_cancelled());
        assert!(
            restore_cancellation(
                &mut guard,
                next,
                restored.revision,
                Sha256Digest::from_bytes([9; 32])
            )
            .is_err()
        );
        assert!(guard.cancel.is_cancelled());
        assert_eq!(guard.profile, old_profile);
        drop(guard);
        assert_eq!(store.load()?, newer);
        Ok(())
    }

    #[test]
    fn pre_spawn_remove_cas_failure_settles_and_admits_one_fresh_cached_intent() -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = Arc::new(airk_store::service::arbiter::ConfigurationStore::open(
            Arc::new(airk_store::service::ServiceLease::acquire(root.path())?),
            Sha256Digest::from_bytes([1; 32]),
            airk_core::identity::Incarnation::from_bytes([2; 16]),
        )?);
        let state = Arc::new(Mutex::new(State::default()));
        let mut selected = store.load()?;
        selected.revision = 2;
        selected.context.enabled = true;
        selected.context.selected_profile = Some(Sha256Digest::from_bytes([3; 32]));
        selected.context.indexes = vec![Enrollment {
            index_id: airk_core::index_catalog::IndexId::from_identity(
                airk_core::identity::Incarnation::from_bytes([7; 16]),
            ),
            tasks: std::collections::BTreeSet::from([Task::ContextAction]),
        }];
        let selected =
            commit_configure(&mut state.lock().unwrap(), &store, 1, &selected, || Ok(()))?;
        let (serial, canceled) = start(&state, Sha256Digest::from_bytes([3; 32]), None)?;
        let mut removal = selected.clone();
        removal.revision = 3;
        removal.context.enabled = false;
        let (release, waiting) = std::sync::mpsc::channel();
        let old = {
            let store = store.clone();
            let state = state.clone();
            std::thread::spawn(
                move || -> Result<Option<(PreparedLaunch, u64, CancellationToken)>> {
                    waiting.recv_timeout(Duration::from_secs(5))?;
                    assert!(
                        store.apply(2, &removal).is_err(),
                        "new Configure must win original removal CAS"
                    );
                    // Exact settlement seam used by the synchronous Remove CAS error,
                    // where no asynchronous native removal owner was ever spawned.
                    settle_and_take_pending(
                        &state,
                        serial,
                        &Err(anyhow::anyhow!("original removal CAS failed")),
                    )
                },
            )
        };
        let mut next = selected;
        next.revision = 3;
        let accepted = commit_configure(&mut state.lock().unwrap(), &store, 2, &next, || Ok(()))?;
        assert!(canceled.is_cancelled());
        assert!(cached_intent_eligible(&accepted, Stage::Context, true));
        let mut prepared = pending_fixture(3, 3)?;
        prepared.expected = accepted.clone();
        state.lock().unwrap().pending = Some(prepared);
        release.send(())?;
        let (prepared, next_serial, next_cancel) = old
            .join()
            .map_err(|_| anyhow::anyhow!("removal thread panicked"))??
            .context("new cached intent stranded after failed removal")?;
        assert_eq!(prepared.expected, accepted);
        assert_ne!(next_serial, serial);
        assert!(!prepared.download);
        assert!(!next_cancel.is_cancelled());
        assert!(state.lock().unwrap().pending.is_none());
        assert!(state.lock().unwrap().busy);
        assert!(
            settle_and_take_pending(
                &state,
                serial,
                &Err(anyhow::anyhow!("stale removal completion"))
            )?
            .is_none()
        );
        assert_eq!(store.load()?, accepted);
        complete(&state, next_serial, &Ok(()));
        assert!(!state.lock().unwrap().busy);
        Ok(())
    }

    #[test]
    fn failed_switch_rollback_uses_actual_configuration_cas_and_preserves_later_choices()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(root.path())?);
        let store = airk_store::service::arbiter::ConfigurationStore::open(
            lease,
            Sha256Digest::from_bytes([1; 32]),
            airk_core::identity::Incarnation::from_bytes([2; 16]),
        )?;
        let prior = store.load()?;
        let mut selected = prior.clone();
        selected.revision = 2;
        selected.stage_mut(Stage::Context).selected_profile =
            Some(Sha256Digest::from_bytes([3; 32]));
        let selected = store.apply(1, &selected)?;
        let restored =
            rollback(&store, &selected, &prior, Stage::Context)?.context("rollback missing")?;
        assert_eq!(restored.revision, 3);
        assert_eq!(restored.stage(Stage::Context).selected_profile, None);
        assert!(!restored.stage(Stage::Context).enabled);
        let mut later = restored.clone();
        later.revision = 4;
        later.stage_mut(Stage::Context).selected_profile = Some(Sha256Digest::from_bytes([4; 32]));
        let later = store.apply(3, &later)?;
        assert_eq!(rollback(&store, &selected, &prior, Stage::Context)?, None);
        assert_eq!(store.load()?, later);
        Ok(())
    }
    #[test]
    fn other_stage_changes_preserve_management_but_rollback_only_its_own_stage() -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = airk_store::service::arbiter::ConfigurationStore::open(
            Arc::new(airk_store::service::ServiceLease::acquire(root.path())?),
            Sha256Digest::from_bytes([1; 32]),
            airk_core::identity::Incarnation::from_bytes([2; 16]),
        )?;
        let prior = store.load()?;
        let mut selected = prior.clone();
        selected.revision = 2;
        selected.context.selected_profile = Some(Sha256Digest::from_bytes([3; 32]));
        let selected = store.apply(1, &selected)?;
        let mut changed = selected.clone();
        changed.revision = 3;
        changed.focus.selected_profile = Some(Sha256Digest::from_bytes([4; 32]));
        let changed = store.apply(2, &changed)?;
        let cancel = CancellationToken::new();
        current(&store, &selected, Stage::Context, &cancel)?;
        assert!(current(&store, &selected, Stage::Focus, &cancel).is_err());
        let restored = rollback(&store, &selected, &prior, Stage::Context)?
            .context("own stage rollback absent")?;
        assert_eq!(restored.revision, 4);
        assert_eq!(restored.focus, changed.focus);
        assert_eq!(restored.context, prior.context);
        cancel.cancel();
        assert!(current(&store, &restored, Stage::Context, &cancel).is_err());
        Ok(())
    }
    #[test]
    fn remove_snapshot_refreshes_after_other_stage_advance_without_changing_original_custody_revision()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = Arc::new(airk_store::service::arbiter::ConfigurationStore::open(
            Arc::new(airk_store::service::ServiceLease::acquire(root.path())?),
            Sha256Digest::from_bytes([1; 32]),
            airk_core::identity::Incarnation::from_bytes([2; 16]),
        )?);
        let mut expected = store.load()?;
        expected.revision = 2;
        expected.context.selected_profile = Some(Sha256Digest::from_bytes([3; 32]));
        let expected = store.apply(1, &expected)?;
        let (entered, admitted) = std::sync::mpsc::channel();
        let (release, waiting) = std::sync::mpsc::channel();
        let removing = {
            let store = store.clone();
            let expected = expected.clone();
            std::thread::spawn(move || -> Result<(Configuration, u64)> {
                entered.send(())?;
                waiting.recv_timeout(Duration::from_secs(5))?;
                // This is the exact native-configuration seam used after the
                // removal owner's asynchronous existing-handle wait.
                let latest = management_snapshot(
                    &store,
                    &expected,
                    Stage::Context,
                    &CancellationToken::new(),
                )?;
                Ok((latest, expected.revision))
            })
        };
        admitted.recv_timeout(Duration::from_secs(5))?;
        let mut changed = store.load()?;
        changed.revision = 3;
        changed.focus.selected_profile = Some(Sha256Digest::from_bytes([4; 32]));
        let changed = store.apply(2, &changed)?;
        release.send(())?;
        let (native_snapshot, cleanup_revision) = removing.join().unwrap()?;
        assert_eq!(native_snapshot, changed);
        assert_eq!(native_snapshot.context, expected.context);
        assert_eq!(cleanup_revision, 2);
        let mut own_change = changed;
        own_change.revision = 4;
        own_change.context.selected_profile = None;
        store.apply(3, &own_change)?;
        assert!(
            management_snapshot(&store, &expected, Stage::Context, &CancellationToken::new())
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn expired_native_management_never_admits_background_effects() -> Result<()> {
        let state = Arc::new(Mutex::new(State::default()));
        let check: airk_https::admin_plane::NativeDispatchCheck =
            Arc::new(|| Err("admin_native_deadline"));
        assert!(start(&state, Sha256Digest::from_bytes([3; 32]), Some(&check)).is_err());
        let state = state.lock().unwrap();
        assert!(!state.busy);
        assert_eq!(state.serial, 0);
        assert_eq!(state.profile, None);
        Ok(())
    }
    pub(super) fn pending_fixture(revision: u64, byte: u8) -> Result<PreparedLaunch> {
        struct Unread;
        impl airk_retrieval::arbiter::runtime::AssetSource for Unread {
            fn read_asset(&self, _: &Asset) -> Result<Vec<u8>> {
                anyhow::bail!("queue test must not read model bytes")
            }
        }
        let mut profile: Profile = serde_json::from_str(include_str!(
            "../../../../../fixtures/arbiter/profile-downloads.json"
        ))?;
        profile.manifest = Sha256Digest::from_bytes([byte; 32]);
        profile.qualification = None;
        profile.stage = Some(Stage::Context);
        profile.model = ModelChoice::ContextQwenCompact;
        profile.tasks = std::collections::BTreeSet::from([Task::ContextAction]);
        let policy = airk_retrieval::arbiter::context_action::policy_digest()?;
        profile.task_policies = std::collections::BTreeMap::from([(Task::ContextAction, policy)]);
        profile.abi = super::super::failure_tests::structural_context_abi();
        let execution = ExecutionProfile::for_qualification(profile)?;
        let mut expected = Configuration::disabled(
            Sha256Digest::from_bytes([1; 32]),
            airk_core::identity::Incarnation::from_bytes([2; 16]),
        );
        expected.revision = revision;
        expected.context.enabled = true;
        expected.context.selected_profile = Some(execution.description().manifest);
        Ok(PreparedLaunch {
            stage: Stage::Context,
            manifest: execution.description().manifest,
            expected,
            execution,
            source: Box::new(Unread),
            download: false,
            import_directory: None,
            previous: None,
            restoration: None,
            continuation: None,
            resident_fallback: None,
            admission: None,
        })
    }
    #[test]
    fn rapid_cached_switch_keeps_only_latest_and_off_or_missing_assets_invalidate_intent()
    -> Result<()> {
        let state = Arc::new(Mutex::new(State::default()));
        let (first, cancel) = start(&state, Sha256Digest::from_bytes([1; 32]), None)?;
        {
            let mut state = state.lock().unwrap();
            state.pending = Some(pending_fixture(2, 2)?);
            state.pending = Some(pending_fixture(3, 3)?);
        }
        cancel.cancel();
        complete(&state, first, &Err(anyhow::anyhow!("superseded")));
        assert!(state.lock().unwrap().busy);
        let (latest, serial, _) = next_pending(&state, first)?.context("latest intent missing")?;
        assert_eq!(latest.expected.revision, 3);
        assert_eq!(latest.manifest, Sha256Digest::from_bytes([3; 32]));
        assert!(!latest.download);
        assert!(
            next_pending(&state, first)?.is_none(),
            "old completion cannot steal a new job's intent"
        );
        {
            let mut state = state.lock().unwrap();
            state.pending = Some(pending_fixture(4, 4)?);
            state.pending = None;
        }
        complete(&state, serial, &Ok(()));
        assert!(next_pending(&state, serial)?.is_none());
        assert!(!state.lock().unwrap().busy);
        let mut configuration = latest.expected;
        assert!(
            !cached_intent_eligible(&configuration, Stage::Context, true),
            "unenrolled selection must not warm"
        );
        configuration.stage_mut(Stage::Context).indexes = vec![Enrollment {
            index_id: airk_core::index_catalog::IndexId::from_identity(
                airk_core::identity::Incarnation::from_bytes([7; 16]),
            ),
            tasks: std::collections::BTreeSet::from([Task::ContextNextRead]),
        }];
        assert!(cached_intent_eligible(&configuration, Stage::Context, true));
        assert!(
            !cached_intent_eligible(&configuration, Stage::Context, false),
            "missing cache never queues a download"
        );
        assert!(!cached_intent_eligible(
            &configuration,
            Stage::Context,
            false
        ));
        configuration.stage_mut(Stage::Context).enabled = false;
        assert!(
            !cached_intent_eligible(&configuration, Stage::Context, true),
            "off invalidates cached activation"
        );
        let (draining, _) = start(&state, Sha256Digest::from_bytes([5; 32]), None)?;
        state.lock().unwrap().pending = Some(pending_fixture(6, 6)?);
        phase(
            &state,
            draining,
            Availability::Draining,
            "actual final charge completion unknown",
        )?;
        complete(
            &state,
            draining,
            &Err(anyhow::anyhow!("drain deadline expired")),
        );
        assert!(
            next_pending(&state, draining)?.is_none(),
            "unknown drain cannot admit a pending native warm"
        );
        assert!(!state.lock().unwrap().busy);
        Ok(())
    }
    #[tokio::test]
    async fn admission_revoked_during_observed_wait_never_reaches_warm_or_ready() -> Result<()> {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let root = tempfile::tempdir()?;
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(root.path())?);
        let configuration = Arc::new(airk_store::service::arbiter::ConfigurationStore::open(
            lease,
            Sha256Digest::from_bytes([1; 32]),
            airk_core::identity::Incarnation::from_bytes([2; 16]),
        )?);
        let expected = configuration.load()?;
        let live = Arc::new(AtomicBool::new(true));
        let warm = Arc::new(AtomicUsize::new(0));
        let ready = Arc::new(AtomicUsize::new(0));
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        let task = {
            let live = live.clone();
            let warm = warm.clone();
            let ready = ready.clone();
            tokio::spawn(async move {
                let admission: DeferredAdmission = Box::new(move |_| {
                    ensure!(live.load(Ordering::Acquire), "index admission revoked");
                    Ok(())
                });
                let cancel = CancellationToken::new();
                admitted_current(
                    &configuration,
                    &expected,
                    Stage::Context,
                    &cancel,
                    Some(&admission),
                )?;
                entered
                    .send(())
                    .map_err(|_| anyhow::anyhow!("barrier observer closed"))?;
                wait.await?;
                admitted_current(
                    &configuration,
                    &expected,
                    Stage::Context,
                    &cancel,
                    Some(&admission),
                )?;
                warm.fetch_add(1, Ordering::AcqRel);
                admitted_current(
                    &configuration,
                    &expected,
                    Stage::Context,
                    &cancel,
                    Some(&admission),
                )?;
                ready.fetch_add(1, Ordering::AcqRel);
                Ok::<(), anyhow::Error>(())
            })
        };
        waiting.await?;
        live.store(false, Ordering::Release);
        release
            .send(())
            .map_err(|_| anyhow::anyhow!("barrier worker closed"))?;
        assert!(task.await?.is_err());
        assert_eq!(warm.load(Ordering::Acquire), 0);
        assert_eq!(ready.load(Ordering::Acquire), 0);
        Ok(())
    }
}
