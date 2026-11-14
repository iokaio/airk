// SPDX-License-Identifier: Apache-2.0
//! Explicit model lifecycle. Source reads never start this owner or acquire assets.
use super::*;
use airk_core::identity::Sha256Digest;
use airk_inference::inference_engine::pool::arbiter::Handle;
use airk_retrieval::arbiter::runtime::ExecutionProfile;
use airk_store::service::arbiter_assets::{AssetStore, Phase, PinnedAssets};
use airk_supervisor::project_supervisor::{ArbiterManagement, Supervisor};
use tokio_util::sync::CancellationToken;

pub(super) struct State {
    serial: u64,
    busy: bool,
    profile: Option<Sha256Digest>,
    phase: Availability,
    reason: String,
    cancel: CancellationToken,
    handle: Option<Handle>,
    pending: Option<PreparedLaunch>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            serial: 0,
            busy: false,
            profile: None,
            phase: Availability::Disabled,
            reason: String::new(),
            cancel: CancellationToken::new(),
            handle: None,
            pending: None,
        }
    }
}
fn phase(shared: &Arc<Mutex<State>>, serial: u64, value: Availability, reason: &str) -> Result<()> {
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
    state.reason = reason.chars().take(1024).collect();
    Ok(())
}
fn start(
    shared: &Arc<Mutex<State>>,
    profile: Sha256Digest,
    effect_check: Option<&airk_https::admin_plane::NativeDispatchCheck>,
) -> Result<(u64, CancellationToken)> {
    let mut state = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
    ensure!(
        !state.busy,
        "Arbiter lifecycle operation remains active; cancel or wait for its original result"
    );
    super::super::native_admin::check_effect(effect_check)?;
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
fn complete(shared: &Arc<Mutex<State>>, serial: u64, result: &Result<()>) {
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
            state.reason = format!("{error:#}").chars().take(1024).collect();
        }
    }
}
fn current(
    configuration: &airk_store::service::arbiter::ConfigurationStore,
    expected: &Configuration,
    cancel: &CancellationToken,
) -> Result<()> {
    ensure!(!cancel.is_cancelled(), "Arbiter operation canceled");
    ensure!(
        configuration.load()? == *expected,
        "Arbiter configuration changed during operation"
    );
    Ok(())
}
fn admitted_current(
    configuration: &airk_store::service::arbiter::ConfigurationStore,
    expected: &Configuration,
    cancel: &CancellationToken,
    admission: Option<&DeferredAdmission>,
) -> Result<()> {
    current(configuration, expected, cancel)?;
    if let Some(admission) = admission {
        admission(expected)?;
    }
    current(configuration, expected, cancel)
}

fn rollback(
    configuration: &airk_store::service::arbiter::ConfigurationStore,
    expected: &Configuration,
    prior: &Configuration,
) -> Result<Option<Configuration>> {
    if configuration.load()? != *expected {
        return Ok(None);
    }
    let mut restored = prior.clone();
    restored.revision = expected
        .revision
        .checked_add(1)
        .context("Arbiter rollback revision exhausted")?;
    Ok(Some(configuration.apply(expected.revision, &restored)?))
}
fn release_disabled(handle: &Handle, configuration: &Configuration) -> Result<()> {
    let observation = handle.observation()?;
    let mut profiles = [
        observation.active_profile,
        observation.warming_profile,
        configuration.selected_profile,
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
async fn release_actual(
    handle: &Handle,
    manifest: Sha256Digest,
    release: airk_kernel::service_resources::OptionalRelease,
    deadline: Instant,
) -> Result<()> {
    handle.release_profile(manifest)?;
    handle.wait_profile_released(manifest, deadline).await?;
    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), release.wait())
        .await
        .context("Actual Arbiter memory charge did not finish draining")??;
    Ok(())
}
async fn warm(
    profile: ExecutionProfile,
    source: Box<dyn airk_retrieval::arbiter::runtime::AssetSource + Send>,
    context: WarmContext<'_>,
) -> Result<()> {
    let WarmContext {
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
    admitted_current(configuration, expected, cancel, admission)?;
    ensure!(
        expected.enabled && expected.selected_profile == Some(manifest),
        "Arbiter warm requires the exact enabled selection"
    );
    let existing = management.existing().await.ok();
    admitted_current(configuration, expected, cancel, admission)?;
    if let Some(handle) = &existing {
        handle.configure(expected.clone())?;
        if handle.observation()?.active_profile == Some(manifest)
            && handle.resident(manifest).is_ok()
        {
            admitted_current(configuration, expected, cancel, admission)?;
            let mut state = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
            ensure!(state.serial == serial, "Arbiter operation superseded");
            state.handle = Some(handle.clone());
            drop(state);
            admitted_current(configuration, expected, cancel, admission)?;
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
    let reservation = match management
        .reserve(profile.description().memory_reservation_bytes)
        .await
    {
        Ok(reservation) => reservation,
        Err(original) => {
            admitted_current(configuration, expected, cancel, admission)?;
            let handle = existing
                .as_ref()
                .context("Arbiter warm has no prior resident to drain")?;
            let old=handle.observation()?.active_profile.filter(|old|*old!=manifest)
                .context("Arbiter warm cannot fit its new charge and has no distinct resident to release")?;
            let rollback = rollback_profile
                .filter(|profile| profile.manifest == old)
                .context("Arbiter prior disk rollback is unproven; its resident was preserved")?;
            // Keep the old verified files pinned through new activation/rollback.
            rollback_pins.push(store.pin(rollback)?);
            admitted_current(configuration, expected, cancel, admission)?;
            let release = handle.resident(old)?.reservation().release_observer();
            phase(
                shared,
                serial,
                Availability::Draining,
                "The budget cannot retain both profiles; draining the old native session and its actual final memory charge while verified rollback assets remain pinned.",
            )?;
            release_actual(handle,old,release,Instant::now()+Duration::from_secs(30)).await
                .with_context(||format!("New profile reservation refused ({original:#}); prior resident drain incomplete"))?;
            phase(
                shared,
                serial,
                Availability::Requested,
                "Prior native session and its actual final charge were released; admitting the selected cached warm.",
            )?;
            admitted_current(configuration, expected, cancel, admission)?;
            management
                .reserve(profile.description().memory_reservation_bytes)
                .await
                .context("New profile still cannot fit after actual prior charge release")?
        }
    };
    let release = reservation.release_observer();
    admitted_current(configuration, expected, cancel, admission)?;
    let handle = management.ensure(expected.clone()).await?;
    admitted_current(configuration, expected, cancel, admission)?;
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
    admitted_current(configuration, expected, cancel, admission)?;
    let receiver = handle.warm(profile, source, reservation)?;
    let result=async {
        tokio::select!{biased;_ = cancel.cancelled()=>anyhow::bail!("Arbiter warm canceled"),value=receiver=>value.context("Arbiter warm delivery closed")??};
        admitted_current(configuration,expected,cancel,admission)?;
        let observed=handle.observation()?;
        ensure!(observed.active_profile==Some(manifest)&&observed.configuration_revision==Some(expected.revision),"Arbiter native activation not observed");
        admitted_current(configuration,expected,cancel,admission)?;
        phase(shared,serial,Availability::Ready,"Qualified Arbiter profile is active in the owned native compute lane.")
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
}

fn cached_intent_eligible(configuration: &Configuration, phase: Option<Phase>) -> bool {
    configuration.enabled && !configuration.indexes.is_empty() && phase == Some(Phase::Ready)
}

type NativeAssetSource = Box<dyn airk_retrieval::arbiter::runtime::AssetSource + Send>;
type Restoration = (ExecutionProfile, NativeAssetSource);
type DeferredAdmission = Box<dyn Fn(&Configuration) -> Result<()> + Send + Sync>;
struct PreparedLaunch {
    expected: Configuration,
    manifest: Sha256Digest,
    download: bool,
    previous: Option<Configuration>,
    execution: ExecutionProfile,
    source: NativeAssetSource,
    restoration: Option<Restoration>,
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
        expected,
        manifest,
        download,
        previous,
        execution,
        source,
        mut restoration,
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
            current(&configuration, &expected, &cancel)?;
            if let Some(admission) = &admission {
                admission(&expected)?;
            }

            if download {
                phase(
                    &shared,
                    serial,
                    Availability::Downloading,
                    "Downloading the exact compiled qualified Arbiter asset parts.",
                )?;
                super::assets::provision(
                    execution.description(),
                    &store,
                    expected.revision,
                    &cancel,
                )
                .await?;
            }
            current(&configuration, &expected, &cancel)?;
            if expected.enabled && expected.selected_profile == Some(manifest) {
                warm(
                    execution,
                    source,
                    WarmContext {
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
                .await?;
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
                .map(|error| format!("{error:#}"))
                .unwrap_or_default();
            let restore_result = async {
                let Some(restored) = rollback(&configuration, &expected, &prior)? else {
                    return Ok(());
                };
                if restored.enabled {
                    let (old, old_source) = restoration.take().context(
                        "Previous profile has no currently verified rollback capability",
                    )?;
                    let rollback_cancel = CancellationToken::new();
                    {
                        let mut state = shared
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
                        ensure!(
                            state.serial == serial,
                            "Arbiter rollback operation superseded"
                        );
                        state.cancel = rollback_cancel.clone();
                        state.profile = Some(old.description().manifest);
                    }
                    warm(
                        old,
                        old_source,
                        WarmContext {
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
                    handle.configure(restored.clone())?;
                    release_disabled(&handle, &restored)?;
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
        complete(&shared, serial, &result);
        resume_pending(store, configuration, shared, management, serial);
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
    let Some(prepared) = state.pending.take() else {
        state.busy = false;
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

fn resume_pending(
    store: Arc<AssetStore>,
    configuration: Arc<airk_store::service::arbiter::ConfigurationStore>,
    shared: Arc<Mutex<State>>,
    management: ArbiterManagement,
    settled_serial: u64,
) {
    let next = next_pending(&shared, settled_serial);
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
                state.reason = format!("Cached latest selection could not be admitted: {error:#}");
            }
        }
    }
}

impl Owner {
    pub(super) fn status(&self) -> Result<Status> {
        let configuration = self.configuration()?;
        let catalog = self.profile_descriptions()?;
        let state = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
        let observation = state.handle.as_ref().map(Handle::observation).transpose()?;
        let active = observation.as_ref().and_then(|value| value.active_profile);
        let native_revision_matches = observation
            .as_ref()
            .is_some_and(|value| value.configuration_revision == Some(configuration.revision));
        let mut profiles = Vec::new();
        for model in [
            ModelChoice::SmallIntern,
            ModelChoice::SmallStrands,
            ModelChoice::LargeIntern,
            ModelChoice::LargeKev,
        ] {
            let profile = catalog
                .iter()
                .filter(|profile| profile.model == model)
                .find(|profile| Some(profile.manifest) == configuration.selected_profile)
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
                    && configuration.enabled
                    && configuration.selected_profile == Some(profile.manifest)
                {
                    (
                        Availability::Ready,
                        "Verified native profile is resident.".into(),
                    )
                } else if active == Some(profile.manifest) {
                    (Availability::Requested,"Previous native session is retained but is not admitted for the current enabled selection.".into())
                } else {
                    match self.assets.phase(profile)?{Some(Phase::Ready)=>(Availability::Requested,"Completed provisioning receipt retained; native activation requires re-verification and the selected enabled choice.".into()),Some(Phase::Downloading)|Some(Phase::Failed)|Some(Phase::Canceled)|Some(Phase::Removing)=>(Availability::Failed,"Prior asset operation did not complete. Explicit recovery is required; no download was resumed.".into()),_=>(Availability::MissingAssets,"Explicit provisioning is required.".into())}
                };
                ProfileStatus {
                    model,
                    profile: Some(profile.manifest),
                    availability,
                    reason: format!("{reason} {fit_reason}"),
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
        let selected = configuration.selected_profile;
        let (availability, reason) = if !configuration.enabled {
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
            (
                Availability::Ready,
                "Selected qualified Arbiter profile is active.".into(),
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
                row.reason = format!(
                    "Private evaluation only; candidate is not release-qualified. {}",
                    row.reason
                );
            }
            format!("Private evaluation only; candidate is not release-qualified. {reason}")
        } else {
            reason
        };
        ensure!(
            self.configuration()? == configuration,
            "Arbiter status configuration changed before delivery"
        );
        Ok(Status {
            schema: STATUS_SCHEMA.into(),
            configuration,
            selected_profile: selected,
            active_profile: active,
            availability,
            reason,
            queued: observation.as_ref().map_or(0, |value| value.queued),
            running: observation.as_ref().map_or(0, |value| value.running),
            profiles,
        })
    }
    fn lifecycle_admission(&self) -> DeferredAdmission {
        let gate = self.gate.clone();
        let ownership = self.ownership.clone();
        let namespace = self.namespace;
        Box::new(move |configuration| {
            validate_configuration_enrollments(&gate, &ownership, namespace, configuration)
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
            .and_then(|handle| handle.observation().ok())
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
        ensure!(
            !download || execution.release_qualified(),
            "Private evaluation assets require descriptor seeding, not product downloads"
        );
        let source = self.asset_source(manifest)?;
        let restoration = previous
            .as_ref()
            .filter(|prior| prior.enabled)
            .and_then(|prior| prior.selected_profile)
            .and_then(|old| {
                self.execution_profile(old)
                    .ok()
                    .zip(self.asset_source(old).ok())
            });
        let resident_fallback = self.resident_description()?;
        let (serial, cancel) = start(&self.lifecycle, manifest, effect_check)?;
        spawn_launch(
            PreparedLaunch {
                expected,
                manifest,
                download,
                previous,
                execution,
                source,
                restoration,
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
                current(&configuration,&expected,&cancel)?;
                let handle=match retained {Some(handle)=>Some(handle),None=>management.existing().await.ok()};
                if let Some(handle)=handle {
                    handle.configure(expected.clone())?;
                    if !expected.enabled {release_disabled(&handle,&expected)?;}
                    let charge = handle.resident(manifest).ok().map(|resident|resident.reservation().release_observer());
                    handle.release_profile(manifest)?;
                    phase(&shared,serial,Availability::Draining,"Waiting for actual native completion before asset removal.")?;
                    let deadline=Instant::now()+Duration::from_secs(30);
                    handle.wait_profile_released(manifest,deadline).await?;
                    if let Some(charge)=charge {tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),charge.wait()).await.context("Removed profile's actual final charge remains draining")??;}
                    phase(&shared,serial,Availability::Verifying,"Native session and final memory charge released; verifying owned disk cleanup.")?;
                }
                current(&configuration,&expected,&cancel)?;
                if store.owned_observation(manifest)?.is_some() {
                    let removal_store = store.clone();
                    tokio::task::spawn_blocking(move||removal_store.remove_owned(manifest,expected.revision)).await.context("Arbiter removal task closed")??;
                }
                phase(&shared,serial,Availability::MissingAssets,"Declared verified assets removed; interrupted recovery stages remain preserved.")?;
                Ok(())
            }.await;
            complete(&shared, serial, &result);
            resume_pending(store, configuration, shared, management, serial);
        });
        Ok(())
    }
    pub(in crate::service_runtime) async fn manage(
        &self,
        request: ManagementRequest,
        supervisor: &Supervisor,
        effect_check: Option<airk_https::admin_plane::NativeDispatchCheck>,
    ) -> Result<Status> {
        let management = supervisor.arbiter_management();
        match request {
            ManagementRequest::Status => {}
            ManagementRequest::Configure {
                expected_revision,
                enabled,
                selected_profile,
                indexes,
            } => {
                let previous = self.configuration()?;
                ensure!(
                    previous.revision == expected_revision,
                    "Arbiter configuration revision changed"
                );
                let candidate = Configuration {
                    revision: expected_revision
                        .checked_add(1)
                        .context("Arbiter revision exhausted")?,
                    enabled,
                    selected_profile,
                    indexes,
                    ..previous.clone()
                };
                self.validate_enrollments(&candidate)?;
                if enabled {
                    self.execution_profile(selected_profile.context("Arbiter profile missing")?)?;
                }
                let (selected, handle, busy) = {
                    let mut state = self
                        .lifecycle
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Arbiter lifecycle unavailable"))?;
                    let selected =
                        self.configuration
                            .apply_checked(expected_revision, &candidate, || {
                                super::super::native_admin::check_effect(effect_check.as_ref())
                            })?;
                    state.cancel.cancel();
                    state.pending = None;
                    (selected, state.handle.clone(), state.busy)
                };
                if let Some(handle) = handle {
                    handle.configure(selected.clone())?;
                    if !selected.enabled {
                        release_disabled(&handle, &selected)?;
                    }
                }
                if selected.enabled {
                    let manifest = selected
                        .selected_profile
                        .context("Arbiter profile missing")?;
                    let profile = self.execution_profile(manifest)?;
                    if cached_intent_eligible(&selected, self.assets.phase(profile.description())?)
                    {
                        if busy {
                            let source = self.asset_source(manifest)?;
                            let restoration = previous
                                .enabled
                                .then_some(previous.selected_profile)
                                .flatten()
                                .and_then(|old| {
                                    self.execution_profile(old)
                                        .ok()
                                        .zip(self.asset_source(old).ok())
                                });
                            let resident_fallback = self.resident_description()?;
                            let prepared = PreparedLaunch {
                                expected: selected.clone(),
                                manifest,
                                download: false,
                                previous: Some(previous.clone()),
                                execution: profile,
                                source,
                                restoration,
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
                self.execution_profile(profile)?;
                self.launch(
                    configuration,
                    profile,
                    true,
                    None,
                    management,
                    effect_check.as_ref(),
                )?;
            }
            ManagementRequest::Recover { expected_revision } => {
                let configuration = self.configuration()?;
                ensure!(
                    configuration.revision == expected_revision,
                    "Arbiter configuration revision changed"
                );
                let mut pending = std::collections::BTreeMap::new();
                for profile in self.profile_descriptions()? {
                    if let Some(observed) = self.assets.observation(&profile)?
                        && matches!(observed.phase, Phase::Downloading | Phase::Failed)
                        && observed.configuration_revision == expected_revision
                    {
                        pending.insert(profile.manifest, observed.phase);
                    }
                }
                for manifest in self.assets.pending_owned_removals(expected_revision)? {
                    pending.insert(manifest, Phase::Removing);
                }
                ensure!(
                    pending.len() <= 1,
                    "Multiple incomplete Arbiter operations require explicit named Provision or Remove"
                );
                if let Some((&manifest, &Phase::Removing)) = pending.first_key_value() {
                    let (serial, cancel) = start(&self.lifecycle, manifest, effect_check.as_ref())?;
                    self.launch_remove(configuration, manifest, management, serial, cancel)?;
                } else if let Some((&manifest, _)) = pending.first_key_value() {
                    self.execution_profile(manifest)?;
                    self.launch(
                        configuration,
                        manifest,
                        true,
                        None,
                        management,
                        effect_check.as_ref(),
                    )?;
                } else {
                    let manifest = configuration.selected_profile.context(
                        "Recovery requires a named prior operation or selected retained profile",
                    )?;
                    let profile = self.execution_profile(manifest)?;
                    ensure!(
                        self.assets.phase(profile.description())? == Some(Phase::Ready),
                        "No matching unfinished operation or Ready profile; use explicit Provision after cancellation/removal"
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
            ManagementRequest::Cancel { expected_revision } => {
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
                state.pending = None;
                if state.busy {
                    state.cancel.cancel();
                    state.phase = Availability::Draining;
                    state.reason="Cancellation requested; original work must settle before recovery or removal.".into();
                    if let (Some(handle), Some(profile)) = (&state.handle, state.profile) {
                        handle.release_profile(profile)?;
                    }
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
                if configuration.enabled && configuration.selected_profile == Some(profile) {
                    configuration.enabled = false;
                    configuration.revision = expected_revision
                        .checked_add(1)
                        .context("Arbiter revision exhausted")?;
                    match self.configuration.apply(expected_revision, &configuration) {
                        Ok(value) => configuration = value,
                        Err(error) => {
                            complete(
                                &self.lifecycle,
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
    pub(in crate::service_runtime) async fn restore_verified(
        &self,
        supervisor: &Supervisor,
    ) -> Result<()> {
        if let Err(error) = self.try_restore_verified(supervisor).await {
            let mut state = self
                .lifecycle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.phase = Availability::Failed;
            state.reason = format!(
                "Optional Arbiter restore refused; ordinary service remains available: {error:#}"
            )
            .chars()
            .take(1024)
            .collect();
        }
        Ok(())
    }
    async fn try_restore_verified(&self, supervisor: &Supervisor) -> Result<()> {
        let configuration = self.configuration()?;
        if !configuration.enabled {
            return Ok(());
        }
        let Some(manifest) = configuration.selected_profile else {
            return Ok(());
        };
        let Ok(profile) = self.execution_profile(manifest) else {
            return Ok(());
        };
        if self.assets.phase(profile.description())? == Some(Phase::Ready) {
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
impl Drop for Owner {
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

#[cfg(test)]
mod tests {
    use super::*;
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
        selected.selected_profile = Some(Sha256Digest::from_bytes([3; 32]));
        let selected = store.apply(1, &selected)?;
        let restored = rollback(&store, &selected, &prior)?.context("rollback missing")?;
        assert_eq!(restored.revision, 3);
        assert_eq!(restored.selected_profile, None);
        assert!(!restored.enabled);
        let mut later = restored.clone();
        later.revision = 4;
        later.selected_profile = Some(Sha256Digest::from_bytes([4; 32]));
        let later = store.apply(3, &later)?;
        assert_eq!(rollback(&store, &selected, &prior)?, None);
        assert_eq!(store.load()?, later);
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
    fn pending_fixture(revision: u64, byte: u8) -> Result<PreparedLaunch> {
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
        let execution = ExecutionProfile::for_qualification(profile)?;
        let expected = Configuration {
            schema: CONFIGURATION_SCHEMA.into(),
            installation: Sha256Digest::from_bytes([1; 32]),
            catalog: airk_core::identity::Incarnation::from_bytes([2; 16]),
            revision,
            enabled: true,
            selected_profile: Some(execution.description().manifest),
            indexes: vec![],
        };
        Ok(PreparedLaunch {
            manifest: execution.description().manifest,
            expected,
            execution,
            source: Box::new(Unread),
            download: false,
            previous: None,
            restoration: None,
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
            !cached_intent_eligible(&configuration, Some(Phase::Ready)),
            "unenrolled selection must not warm"
        );
        configuration.indexes = vec![Enrollment {
            index_id: airk_core::index_catalog::IndexId::from_identity(
                airk_core::identity::Incarnation::from_bytes([7; 16]),
            ),
            tasks: std::collections::BTreeSet::from([Task::ContextNextRead]),
        }];
        assert!(cached_intent_eligible(&configuration, Some(Phase::Ready)));
        assert!(
            !cached_intent_eligible(&configuration, None),
            "missing cache never queues a download"
        );
        assert!(!cached_intent_eligible(&configuration, Some(Phase::Failed)));
        configuration.enabled = false;
        assert!(
            !cached_intent_eligible(&configuration, Some(Phase::Ready)),
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
                admitted_current(&configuration, &expected, &cancel, Some(&admission))?;
                entered
                    .send(())
                    .map_err(|_| anyhow::anyhow!("barrier observer closed"))?;
                wait.await?;
                admitted_current(&configuration, &expected, &cancel, Some(&admission))?;
                warm.fetch_add(1, Ordering::AcqRel);
                admitted_current(&configuration, &expected, &cancel, Some(&admission))?;
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
