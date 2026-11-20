// SPDX-License-Identifier: Apache-2.0
//! Typed current device policy, distinct from legacy configuration3 and status2.
use super::*;
use airk_core::arbiter::device::*;
use airk_kernel::arbiter::device_runtime::NativeConfiguration;
#[cfg(feature = "local-inference")]
use {
    airk_core::identity::Sha256Digest,
    airk_inference::inference_engine::pool::arbiter::{
        DeviceFault, DeviceFaultKey, Handle, NativeDevice, ResidentEligibility, ResidentPermit,
    },
    airk_retrieval::arbiter::runtime::{self, ExecutionProfile},
    airk_store::service::arbiter_assets::{AssetStore, Phase},
    std::collections::BTreeMap,
    tokio_util::sync::CancellationToken,
};

pub(in crate::service_runtime) fn current_request(request: &ManagementRequest) -> bool {
    match request {
        ManagementRequest::StatusDevice
        | ManagementRequest::InitializeDevice { .. }
        | ManagementRequest::MigrateDevice { .. }
        | ManagementRequest::ConfigureDevice { .. }
        | ManagementRequest::ProvisionDevice { .. }
        | ManagementRequest::CancelDevice { .. }
        | ManagementRequest::RemoveDevice { .. }
        | ManagementRequest::RecoverDevice { .. } => true,
        ManagementRequest::Status
        | ManagementRequest::Initialize { .. }
        | ManagementRequest::Configure { .. }
        | ManagementRequest::Provision { .. }
        | ManagementRequest::Cancel { .. }
        | ManagementRequest::Remove { .. }
        | ManagementRequest::Recover { .. } => false,
    }
}
pub(super) struct Readiness {
    pub availability: Availability,
    #[cfg(feature = "local-inference")]
    pub reason: String,
}
#[cfg(feature = "local-inference")]
#[derive(Default)]
pub(super) struct Shared {
    pub state: Mutex<DeviceState>,
    gpu_disabled: std::sync::atomic::AtomicBool,
    pub generation: std::sync::atomic::AtomicU64,
}
#[cfg(feature = "local-inference")]
#[derive(Default)]
pub(super) struct DeviceState {
    pending: BTreeMap<Stage, (Launch, DeviceAction)>,
    runners: std::collections::BTreeSet<Stage>,
    circuits: std::collections::BTreeSet<DeviceFaultKey>,
    intent_generation: BTreeMap<Stage, u64>,
    document: Option<DeviceConfiguration>,
    selected: BTreeMap<Stage, Profile>,
    ready: std::collections::BTreeSet<Stage>,
    eligibilities: BTreeMap<Stage, ResidentEligibility>,
    selection: BTreeMap<Stage, DeviceSelection>,
    devices: BTreeMap<Stage, (NativeDevice, u64)>,
    warm_extra: BTreeMap<Stage, u64>,
    evidence: BTreeMap<Sha256Digest, DeviceEvidence>,
}
#[cfg(feature = "local-inference")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReadDiagnosticStamp {
    generation: u64,
    document: Option<DeviceConfiguration>,
    ready: std::collections::BTreeSet<Stage>,
    selected: BTreeMap<Stage, Sha256Digest>,
    devices: BTreeMap<Stage, (u64, u64, bool)>,
    eligibilities: BTreeMap<Stage, (Sha256Digest, bool)>,
    intent_generation: BTreeMap<Stage, u64>,
    selection: Vec<u8>,
    pending: std::collections::BTreeSet<Stage>,
    runners: std::collections::BTreeSet<Stage>,
}
#[cfg(feature = "local-inference")]
impl Shared {
    pub(super) fn gpu_allowed(&self) -> bool {
        !self.gpu_disabled.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Data-only before/after fence; no Store mutation, native call or permit.
    pub(super) fn read_diagnostic_stamp(&self) -> Result<ReadDiagnosticStamp> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native device diagnostic stamp unavailable"))?;
        Ok(ReadDiagnosticStamp {
            generation: self.generation.load(std::sync::atomic::Ordering::Acquire),
            document: state.document.clone(),
            ready: state.ready.clone(),
            selected: state
                .selected
                .iter()
                .map(|(stage, profile)| (*stage, profile.manifest))
                .collect(),
            devices: state
                .devices
                .iter()
                .map(|(stage, (device, cached))| {
                    (*stage, (*cached, device.epoch(), device.poisoned()))
                })
                .collect(),
            eligibilities: state
                .eligibilities
                .iter()
                .map(|(stage, proof)| (*stage, (proof.profile(), proof.current())))
                .collect(),
            intent_generation: state.intent_generation.clone(),
            selection: serde_json::to_vec(&state.selection)?,
            pending: state.pending.keys().copied().collect(),
            runners: state.runners.clone(),
        })
    }
    fn policy(&self, current: DeviceConfiguration) -> Result<NativeConfiguration> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native device selection unavailable"))?;
        if state.document.as_ref() != Some(&current) {
            let selected = if let Some(previous) = &state.document {
                if previous.installation == current.installation
                    && previous.catalog == current.catalog
                    && previous.device_catalog == current.device_catalog
                {
                    state
                        .selected
                        .iter()
                        .filter(|(stage, _)| previous.stage(**stage) == current.stage(**stage))
                        .filter(|(_, profile)| {
                            self.gpu_allowed() || profile.device_execution.is_none()
                        })
                        .map(|(_, profile)| profile.clone())
                        .collect()
                } else {
                    vec![]
                }
            } else {
                vec![]
            };
            return NativeConfiguration::device(current, selected);
        }
        for (device, epoch) in state.devices.values() {
            ensure!(
                !device.poisoned() && device.epoch() == *epoch,
                "native device epoch changed or settlement unknown"
            );
        }
        NativeConfiguration::device(
            current,
            state
                .selected
                .values()
                .filter(|profile| self.gpu_allowed() || profile.device_execution.is_none())
                .cloned()
                .collect(),
        )
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "one bounded native transition; witnesses are not serialized control data"
    )]
    fn install(
        &self,
        document: DeviceConfiguration,
        stage: Stage,
        profile: Option<Profile>,
        selection: Option<DeviceSelection>,
        device: Option<NativeDevice>,
        extra: Option<u64>,
        evidence: Option<DeviceEvidence>,
    ) -> Result<NativeConfiguration> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native device selection unavailable"))?;
        ensure!(
            self.gpu_allowed() || device.is_none(),
            "service GPU policy changed before selection"
        );
        let mut selected = state.selected.clone();
        let mut ready = state.ready.clone();
        let mut eligibilities = state.eligibilities.clone();
        let mut devices = state.devices.clone();
        let mut warm_extra = state.warm_extra.clone();
        let mut selections = state.selection.clone();
        let mut evidences = state.evidence.clone();
        if state.document.as_ref().is_some_and(|old| {
            old != &document && old.stage(other(stage)) != document.stage(other(stage))
        }) {
            selected.remove(&other(stage));
            ready.remove(&other(stage));
            eligibilities.remove(&other(stage));
            devices.remove(&other(stage));
            warm_extra.remove(&other(stage));
            selections.remove(&other(stage));
        }
        selected.remove(&stage);
        ready.remove(&stage);
        eligibilities.remove(&stage);
        devices.remove(&stage);
        warm_extra.remove(&stage);
        selections.remove(&stage);
        if let Some(profile) = profile {
            if let Some(evidence) = evidence {
                evidences.insert(profile.manifest, evidence);
            }
            selected.insert(stage, profile);
        }
        if let Some(selection) = selection {
            selections.insert(stage, selection);
        }
        if let Some(device) = device {
            ensure!(!device.poisoned(), "cannot activate poisoned native device");
            let epoch = device.epoch();
            devices.insert(stage, (device, epoch));
        }
        if let Some(extra) = extra {
            warm_extra.insert(stage, extra);
        }
        // Validate the complete proposed policy BEFORE changing any owner state.
        let policy = NativeConfiguration::device(
            document.clone(),
            selected
                .values()
                .filter(|profile| self.gpu_allowed() || profile.device_execution.is_none())
                .cloned()
                .collect(),
        )?;
        state.circuits.retain(|key| {
            key.configuration_revision == document.revision
                && key.device_catalog == document.device_catalog.catalog
                && key.catalog_source == document.device_catalog.source
        });
        state.document = Some(document);
        state.selected = selected;
        state.ready = ready;
        state.eligibilities = eligibilities;
        state.devices = devices;
        state.warm_extra = warm_extra;
        state.selection = selections;
        state.evidence = evidences;
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Ok(policy)
    }
    fn finish_activation(
        &self,
        document: &DeviceConfiguration,
        stage: Stage,
        profile: &Profile,
        eligibilities: BTreeMap<Stage, ResidentEligibility>,
        validate: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        validate()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native ready observation unavailable"))?;
        ensure!(
            (profile.device_execution.is_none() || self.gpu_allowed())
                && state.document.as_ref() == Some(document)
                && state
                    .selected
                    .get(&stage)
                    .is_some_and(|actual| actual.manifest == profile.manifest),
            "native selection changed before ready commit"
        );
        ensure!(
            state
                .devices
                .get(&stage)
                .is_none_or(|(device, epoch)| !device.poisoned() && device.epoch() == *epoch),
            "native device epoch changed before ready commit"
        );
        state.eligibilities = eligibilities
            .into_iter()
            .filter(|(stage, proof)| {
                proof.current()
                    && state
                        .selected
                        .get(stage)
                        .is_some_and(|profile| profile.manifest == proof.profile())
            })
            .collect();
        ensure!(
            profile.device_execution.is_none()
                || state
                    .eligibilities
                    .get(&stage)
                    .is_some_and(ResidentEligibility::current),
            "actual local device eligibility is not current"
        );
        state.ready.insert(stage);
        Ok(())
    }
}
#[cfg(feature = "local-inference")]
fn other(stage: Stage) -> Stage {
    match stage {
        Stage::Focus => Stage::Context,
        Stage::Context => Stage::Focus,
    }
}
#[cfg(feature = "local-inference")]
pub(super) fn binding() -> Result<DeviceCatalogBinding> {
    let catalog = runtime::device_catalog()?;
    let profiles = runtime::catalog_profiles()?;
    catalog.binding(runtime::catalog_source_digest(), |id| {
        profiles.iter().find(|p| p.manifest == id)
    })
}
#[cfg(feature = "local-inference")]
fn load_policy(
    store: &airk_store::service::arbiter::ConfigurationStore,
    shared: &Shared,
    binding: &DeviceCatalogBinding,
) -> Result<NativeConfiguration> {
    if let Ok(legacy) = store.load() {
        return Ok(NativeConfiguration::Legacy(legacy));
    }
    match store.device(binding.clone()).load()? {
        airk_store::service::arbiter::device::StoredConfiguration::Device(current) => {
            shared.policy(current)
        }
        airk_store::service::arbiter::device::StoredConfiguration::Legacy(legacy) => {
            Ok(NativeConfiguration::Legacy(legacy))
        }
    }
}
impl StageOwner {
    pub(super) fn read_configuration(&self) -> Result<NativeConfiguration> {
        ensure!(
            self.gate
                .catalog_clock()
                .is_some_and(|clock| clock.namespace == self.namespace),
            airk_core::scoped_reads::ScopeReadRefused
        );
        #[cfg(feature = "local-inference")]
        {
            load_policy(&self.configuration, &self.devices, &binding()?)
        }
        #[cfg(not(feature = "local-inference"))]
        {
            Ok(NativeConfiguration::Legacy(self.configuration()?))
        }
    }
    #[cfg(feature = "local-inference")]
    pub(super) fn policy_reader(
        &self,
    ) -> Result<Arc<dyn Fn() -> Result<NativeConfiguration> + Send + Sync>> {
        let store = self.configuration.clone();
        let devices = self.devices.clone();
        let binding = binding()?;
        Ok(Arc::new(move || load_policy(&store, &devices, &binding)))
    }
    #[cfg(feature = "local-inference")]
    pub(super) fn profile_description(
        &self,
        manifest: airk_core::identity::Sha256Digest,
    ) -> Result<Profile> {
        self.profile_descriptions()?
            .into_iter()
            .find(|p| p.manifest == manifest)
            .context("native selected description unavailable")
    }
    pub(super) fn read_stage_status(&self) -> Result<Readiness> {
        #[cfg(feature = "local-inference")]
        if let NativeConfiguration::Device { document, .. } = self.read_configuration()? {
            let status = self.device_stage_status(&document)?;
            return Ok(Readiness {
                availability: status.availability,
                reason: status.reason,
            });
        }
        let status = self.status()?;
        Ok(Readiness {
            availability: status.availability,
            #[cfg(feature = "local-inference")]
            reason: status.reason,
        })
    }
    #[cfg(feature = "local-inference")]
    pub(super) async fn reuse_resident(
        &self,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
        resident: &ResidentPermit,
    ) -> Result<airk_kernel::service_resources::OptionalReuse> {
        let extra = {
            let state = self
                .devices
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("native device selection unavailable"))?;
            ensure!(
                self.devices.gpu_allowed() || !state.devices.contains_key(&self.stage),
                "GPU residency disabled by service policy"
            );
            state.warm_extra.get(&self.stage).copied()
        };
        match extra {
            Some(extra) => {
                supervisor
                    .reuse_optional_with_extra(resident.reservation(), extra)
                    .await
            }
            None => supervisor.reuse_optional(resident.reservation()).await,
        }
    }
}
impl Owner {
    pub(in crate::service_runtime) async fn manage_device(
        &self,
        request: ManagementRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
        effect: Option<airk_https::admin_plane::NativeDispatchCheck>,
    ) -> Result<DeviceStatus> {
        #[cfg(feature = "local-inference")]
        {
            self.manage_device_native(request, supervisor, effect).await
        }
        #[cfg(not(feature = "local-inference"))]
        {
            let _ = (request, supervisor, effect);
            anyhow::bail!("native optional inference is unavailable in this image")
        }
    }
    pub(in crate::service_runtime) async fn import_device(
        &self,
        request: airk_core::native_offline_import::NativeOfflineImportRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
        effect: Option<airk_https::admin_plane::NativeDispatchCheck>,
    ) -> Result<DeviceStatus> {
        #[cfg(feature = "local-inference")]
        {
            request.validate()?;
            let document = self.current_device_configuration()?;
            ensure!(
                document.revision == request.expected_revision,
                "device import revision changed"
            );
            let profile = runtime::release_asset_profile(request.profile)?;
            let stage = profile.declared_stage().context("import stage missing")?;
            self.stage(stage).launch_device(
                document,
                supervisor,
                effect,
                DeviceAction::Import(profile, std::path::PathBuf::from(request.directory)),
            )?;
            self.device_status()
        }
        #[cfg(not(feature = "local-inference"))]
        {
            let _ = (request, supervisor, effect);
            anyhow::bail!("native optional inference is unavailable in this image")
        }
    }
}
#[cfg(feature = "local-inference")]
fn catalog_live(
    owner: &StageOwner,
    effect: Option<&airk_https::admin_plane::NativeDispatchCheck>,
) -> Result<()> {
    ensure!(
        owner
            .gate
            .catalog_clock()
            .is_some_and(|clock| clock.namespace == owner.namespace),
        airk_core::scoped_reads::ScopeReadRefused
    );
    super::super::native_admin::check_effect(effect)
}
#[cfg(feature = "local-inference")]
impl Owner {
    pub(in crate::service_runtime) fn set_gpu_allowed(
        &self,
        allowed: bool,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<()> {
        use std::sync::atomic::Ordering;

        let shared = &self.context.devices;
        let active: Vec<_> = {
            let state = shared
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("GPU policy unavailable"))?;
            if shared.gpu_disabled.swap(!allowed, Ordering::AcqRel) == !allowed {
                return Ok(());
            }
            shared.generation.fetch_add(1, Ordering::AcqRel);
            [Stage::Focus, Stage::Context]
                .into_iter()
                .filter(|stage| {
                    state.runners.contains(stage)
                        || state.ready.contains(stage)
                        || state.devices.contains_key(stage)
                })
                .collect()
        };
        #[cfg(all(windows, target_arch = "x86_64", feature = "test-support", feature = "gpu-cuda"))]
        self.apply_evaluation_gpu_policy(allowed)?;
        if active.is_empty() {
            return Ok(());
        }
        let document = self.current_device_configuration()?;
        for stage in active {
            if !document.stage(stage).enabled
                || document.stage(stage).device_policy != DevicePolicy::Auto
            {
                continue;
            }
            let owner = self.stage(stage);
            {
                let mut life = owner
                    .lifecycle
                    .lock()
                    .map_err(|_| anyhow::anyhow!("GPU lifecycle unavailable"))?;
                cancel_device_stage(&mut life, shared, stage)?;
                if let Some(handle) = &life.handle {
                    let observation = handle.observation_for(stage)?;
                    let profiles: std::collections::BTreeSet<_> =
                        [observation.active_profile, observation.warming_profile]
                            .into_iter()
                            .flatten()
                            .collect();
                    for profile in profiles {
                        handle.release_profile(profile)?;
                    }
                }
            }
            owner.launch_device(document.clone(), supervisor, None, DeviceAction::Warm)?;
        }
        Ok(())
    }

    pub(super) fn current_device_configuration(&self) -> Result<DeviceConfiguration> {
        let profiles = runtime::catalog_profiles()?;
        let binding = binding()?;
        if let Some(observed) = self
            .configuration
            .device(binding.clone())
            .inspect_device_binding()?
        {
            ensure!(
                observed.binding == binding,
                "configuration4 requires an exact authenticated compiled catalog transition; no unknown source rebind is authorized"
            );
        }
        self.configuration.device(binding).load_or_migrate(
            |id| profiles.iter().find(|p| p.manifest == id),
            || catalog_live(&self.context, None),
        )
    }
    fn device_status(&self) -> Result<DeviceStatus> {
        let binding = binding()?;
        if let Some(initialization) = self
            .configuration
            .device(binding.clone())
            .initialization_status(|| catalog_live(&self.context, None))?
        {
            self.focus.require_initialization_idle()?;
            self.context.require_initialization_idle()?;
            let configuration = DeviceConfiguration::disabled(
                self.context.installation,
                self.context.namespace,
                binding,
            );
            let mut focus = self.focus.device_stage_status(&configuration)?;
            let mut context = self.context.device_stage_status(&configuration)?;
            focus.reason = initialization.reason.clone();
            context.reason = initialization.reason.clone();
            let status = DeviceStatus {
                schema: "airk.arbiter.status/3".into(),
                initialization: Some(initialization),
                configuration,
                focus,
                context,
            };
            status.validate()?;
            return Ok(status);
        }
        let configuration = self.current_device_configuration()?;
        let focus = self.focus.device_stage_status(&configuration)?;
        let context = self.context.device_stage_status(&configuration)?;
        ensure!(
            self.current_device_configuration()? == configuration,
            "device configuration changed during status"
        );
        let status = DeviceStatus {
            schema: "airk.arbiter.status/3".into(),
            initialization: None,
            configuration,
            focus,
            context,
        };
        status.validate()?;
        Ok(status)
    }
    async fn manage_device_native(
        &self,
        request: ManagementRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
        effect: Option<airk_https::admin_plane::NativeDispatchCheck>,
    ) -> Result<DeviceStatus> {
        let binding = binding()?;
        let store = self.configuration.device(binding.clone());
        match request {
            ManagementRequest::StatusDevice => {}
            ManagementRequest::InitializeDevice {
                expected_legacy_configuration,
            } => {
                with_initialization_fence(
                    &self.focus.lifecycle,
                    &self.context.lifecycle,
                    &self.context.devices,
                    || catalog_live(&self.context, effect.as_ref()),
                    |live| {
                        store
                            .initialize_checked(expected_legacy_configuration, live)
                            .map(|_| ())
                    },
                )?;
            }
            ManagementRequest::MigrateDevice {
                expected_revision,
                expected_configuration,
            } => {
                let profiles = runtime::catalog_profiles()?;
                store.migrate_checked(
                    expected_revision,
                    expected_configuration,
                    |id| profiles.iter().find(|p| p.manifest == id),
                    || catalog_live(&self.context, effect.as_ref()),
                )?;
            }
            ManagementRequest::ConfigureDevice {
                expected_revision,
                stage,
                configuration,
            } => {
                configuration.validate(stage)?;
                let owner = self.stage(stage);
                let mut current = self.current_device_configuration()?;
                ensure!(
                    current.revision == expected_revision,
                    "device configuration revision changed"
                );
                let mut lifecycle = owner
                    .lifecycle
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native lifecycle unavailable"))?;
                ensure!(!lifecycle.stopping, "native lifecycle stopping");
                *current.stage_mut(stage) = configuration;
                current.revision = current
                    .revision
                    .checked_add(1)
                    .context("device revision exhausted")?;
                if current.stage(stage).enabled {
                    let model = current
                        .stage(stage)
                        .selected_model
                        .context("enabled device model missing")?;
                    let admitted = runtime::catalog_profiles()?
                        .into_iter()
                        .filter(|profile| profile.model == model.model())
                        .filter(|profile| {
                            current.stage(stage).indexes.iter().all(|enrollment| {
                                supports_stage_task(stage, &enrollment.tasks, &profile.tasks)
                            })
                        })
                        .any(|profile| {
                            if profile.device_execution.is_some() {
                                runtime::release_asset_profile(profile.manifest).is_ok()
                            } else {
                                cpu_model_admission(&profile).is_ok()
                            }
                        });
                    ensure!(
                        admitted,
                        "selected model has no native admitted variant for the requested tasks and target"
                    );
                }
                validate_device_enrollments(owner, &current, stage)?;
                let current = store.apply_checked(expected_revision, &current, || {
                    catalog_live(owner, effect.as_ref())
                })?;
                lifecycle.accepted_configure_revision = current.revision;
                super::lifecycle::cancel_current(&mut lifecycle);
                drop(lifecycle);
                owner.launch_device(current, supervisor, None, DeviceAction::Warm)?;
            }
            ManagementRequest::ProvisionDevice {
                expected_revision,
                profile,
            } => {
                let current = self.current_device_configuration()?;
                ensure!(
                    current.revision == expected_revision,
                    "device provision revision changed"
                );
                let profile = runtime::release_asset_profile(profile)?;
                let stage = profile.declared_stage().context("device stage missing")?;
                self.stage(stage).launch_device(
                    current,
                    supervisor,
                    effect,
                    DeviceAction::Provision(profile),
                )?;
            }
            ManagementRequest::RecoverDevice {
                expected_revision,
                stage,
            } => {
                let current = self.current_device_configuration()?;
                ensure!(
                    current.revision == expected_revision,
                    "device recovery revision changed"
                );
                self.stage(stage).launch_device(
                    current,
                    supervisor,
                    effect,
                    DeviceAction::Recover,
                )?;
            }
            ManagementRequest::CancelDevice {
                expected_revision,
                stage,
            } => {
                let owner = self.stage(stage);
                let mut state = owner
                    .lifecycle
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native lifecycle unavailable"))?;
                ensure!(
                    self.current_device_configuration()?.revision == expected_revision,
                    "device cancel revision changed"
                );
                catalog_live(owner, effect.as_ref())?;
                cancel_device_stage(&mut state, &owner.devices, stage)?;
                if let Some(handle) = &state.handle {
                    let actual = handle.observation_for(stage)?;
                    let mut profiles =
                        [state.profile, actual.active_profile, actual.warming_profile]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>();
                    profiles.sort();
                    profiles.dedup();
                    for profile in profiles {
                        handle.release_profile(profile)?;
                    }
                }
            }
            ManagementRequest::RemoveDevice {
                expected_revision,
                profile,
            } => {
                let current = self.current_device_configuration()?;
                ensure!(
                    current.revision == expected_revision,
                    "device removal revision changed"
                );
                let owned = AssetStore::open_device(
                    self.context.ownership.clone(),
                    self.context.installation,
                    self.context.namespace,
                )?;
                let stage = runtime::catalog_profiles()?
                    .into_iter()
                    .find(|description| description.manifest == profile)
                    .and_then(|description| description.declared_stage())
                    .or(owned
                        .owned_observation(profile)?
                        .map(|observation| observation.stage))
                    .context(
                        "named profile has neither native stage nor original owned cleanup custody",
                    )?;
                let owner = self.stage(stage);
                let active = owner
                    .devices
                    .state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native device selection unavailable"))?
                    .selected
                    .get(&stage)
                    .map(|p| p.manifest);
                let current = if active == Some(profile) {
                    let mut disabled = current;
                    disabled.stage_mut(stage).enabled = false;
                    disabled.revision = disabled
                        .revision
                        .checked_add(1)
                        .context("device revision exhausted")?;
                    store.apply_checked(expected_revision, &disabled, || {
                        catalog_live(owner, effect.as_ref())
                    })?
                } else {
                    current
                };
                owner.launch_device(current, supervisor, effect, DeviceAction::Remove(profile))?;
            }
            ManagementRequest::Status
            | ManagementRequest::Initialize { .. }
            | ManagementRequest::Configure { .. }
            | ManagementRequest::Provision { .. }
            | ManagementRequest::Cancel { .. }
            | ManagementRequest::Remove { .. }
            | ManagementRequest::Recover { .. } => {
                anyhow::bail!("legacy operation cannot mutate current device state")
            }
        }
        self.device_status()
    }
}
#[cfg(feature = "local-inference")]
fn validate_device_enrollments(
    owner: &StageOwner,
    configuration: &DeviceConfiguration,
    stage: Stage,
) -> Result<()> {
    validate_current_enrollments(
        &owner.gate,
        &owner.ownership,
        owner.namespace,
        configuration,
        stage,
    )
}
#[cfg(feature = "local-inference")]
fn validate_current_enrollments(
    gate: &airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    ownership: &airk_store::service::ServiceLease,
    namespace: airk_core::identity::Incarnation,
    configuration: &DeviceConfiguration,
    stage: Stage,
) -> Result<()> {
    configuration.validate()?;
    let clock = gate.catalog_clock().context("native catalog unavailable")?;
    ensure!(
        clock.namespace == namespace,
        "native catalog namespace changed"
    );
    let mut registry =
        airk_store::service::registry::ServiceRegistry::open_saved(ownership.root())?;
    for enrollment in &configuration.stage(stage).indexes {
        let detail = registry.catalog_detail(
            &airk_core::catalog_management::CatalogEntity::Index {
                id: enrollment.index_id,
            },
            Some(clock.revision),
        )?;
        ensure!(
            matches!(detail.item,Some(airk_core::catalog_management::CatalogItem::Index(ref index)) if !index.tombstone&&index.source_import==airk_core::index_catalog::SourceImportState::Complete),
            "device enrollment requires current immutable index"
        );
    }
    ensure!(
        gate.catalog_clock()
            .is_some_and(|now| now.namespace == clock.namespace && now.revision == clock.revision),
        "native catalog changed during device enrollment validation"
    );
    Ok(())
}
#[cfg(feature = "local-inference")]
impl StageOwner {
    pub(super) fn device_stage_status(
        &self,
        document: &DeviceConfiguration,
    ) -> Result<DeviceStageStatus> {
        let profiles = runtime::catalog_profiles()?;
        let requested = document.stage(self.stage);
        let life = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("device lifecycle unavailable"))?;
        let observation = life
            .handle
            .as_ref()
            .map(|handle| handle.observation_for(self.stage))
            .transpose()?;
        let state = self
            .devices
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native device selection unavailable"))?;
        let selected = state.document.as_ref() == Some(document);
        let epoch_live = state
            .devices
            .get(&self.stage)
            .is_none_or(|(device, epoch)| !device.poisoned() && device.epoch() == *epoch);
        let active = observation.as_ref().and_then(|v| v.active_profile);
        let actual_resident = active.is_some_and(|profile| {
            life.handle
                .as_ref()
                .is_some_and(|handle| handle.resident(profile).is_ok())
        });
        let device_eligible = state
            .selected
            .get(&self.stage)
            .is_none_or(|profile| profile.device_execution.is_none())
            || state
                .eligibilities
                .get(&self.stage)
                .is_some_and(ResidentEligibility::current);
        let resident = ready_from_observation(
            &state,
            document,
            self.stage,
            observation.as_ref(),
            actual_resident && device_eligible,
        );
        let models = match self.stage {
            Stage::Focus => vec![SelectedModel::Focus],
            Stage::Context => vec![SelectedModel::ContextCompact, SelectedModel::ContextLarge],
        }
        .into_iter()
        .map(|model| {
            let variants = profiles
                .iter()
                .filter(|p| p.model == model.model())
                .map(|profile| -> Result<DeviceVariantStatus> {
                    let backend = profile.device_execution.as_ref().map(|v| v.backend);
                    let evidence = state.evidence.get(&profile.manifest).cloned();
                    let admitted = if backend.is_some() {
                        runtime::release_asset_profile(profile.manifest).is_ok()
                    } else {
                        cpu_model_admission(profile).is_ok()
                    };
                    let ready = resident
                        && active == Some(profile.manifest)
                        && admitted
                        && (backend.is_none() || evidence.is_some());
                    DeviceVariantStatus::from_description(
                        profile,
                        if ready {
                            Availability::Ready
                        } else if admitted {
                            Availability::Requested
                        } else {
                            Availability::Unqualified
                        },
                        if ready {
                            "Actual authenticated native variant is resident."
                        } else if actual_resident&&active==Some(profile.manifest)&&backend.is_some()&&!device_eligible {
                            "The GPU session remains resident, but local eligibility is not current; Recover can reaccept it under a bounded management operation."
                        } else if admitted {
                            "Native model is admitted; this variant is not active."
                        } else {
                            "Variant has no current native model/device admission."
                        }
                        .into(),
                        if !admitted {
                            ModelAdmissionStatus::Unavailable
                        } else if runtime::is_local_test_profile(profile) {
                            ModelAdmissionStatus::LocalCandidate
                        } else {
                            ModelAdmissionStatus::Qualified
                        },
                        evidence,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let tasks = if variants.is_empty() {
                model.supported_tasks()
            } else {
                variants
                    .iter()
                    .flat_map(|variant| variant.tasks.iter().copied())
                    .collect()
            };
            let ready = variants
                .iter()
                .any(|variant| variant.availability == Availability::Ready);
            let admitted = variants
                .iter()
                .any(|variant| variant.model_admission != ModelAdmissionStatus::Unavailable);
            let availability = if ready {
                Availability::Ready
            } else if admitted {
                Availability::Requested
            } else {
                Availability::Unqualified
            };
            let reason = if variants.is_empty() {
                "No compiled native variant is available for this model."
            } else if ready {
                "An authenticated native variant of this model is active."
            } else if admitted {
                "This model is admitted; no variant is active for the current request."
            } else {
                "No variant of this model has current native admission."
            }
            .into();
            Ok(DeviceModelStatus {
                model,
                variants,
                availability,
                reason,
                tasks,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let draining = !epoch_live
            || life.phase == Availability::Draining
            || (life.cancel.is_cancelled()
                && observation.as_ref().is_some_and(|actual| {
                    actual.active_profile.is_some()
                        || actual.warming_profile.is_some()
                        || actual.running > 0
                }))
            || observation.as_ref().is_some_and(|v| {
                !requested.enabled
                    && (v.active_profile.is_some() || v.warming_profile.is_some() || v.running > 0)
            });
        let status = DeviceStageStatus {
            selected_model: requested.selected_model,
            device_policy: requested.device_policy,
            gpu_precision: requested.gpu_precision,
            active_profile: active,
            availability: if draining {
                Availability::Draining
            } else if !requested.enabled {
                Availability::Disabled
            } else if resident {
                Availability::Ready
            } else if life.phase == Availability::Failed {
                Availability::Failed
            } else if life.busy {
                if life.phase == Availability::Ready {
                    Availability::Requested
                } else {
                    life.phase
                }
            } else {
                Availability::Requested
            },
            reason: bounded_reason(if !epoch_live {
                "Native device epoch changed or settlement is unknown; no execution is admitted."
                    .into()
            } else if actual_resident && !device_eligible {
                "The GPU session remains resident, but local eligibility is absent or stale; explicit Auto management must reaccept it within its original allowance.".into()
            } else if !life.reason.is_empty() {
                life.reason.clone()
            } else if resident {
                "Actual native device selection is resident.".into()
            } else {
                "No actual native variant is active; ordinary evidence remains available.".into()
            }),
            queued: observation.as_ref().map_or(0, |v| v.queued),
            running: observation.as_ref().map_or(0, |v| v.running),
            selection: if selected {
                state.selection.get(&self.stage).cloned()
            } else {
                None
            },
            models,
        };
        status.validate(self.stage, requested)?;
        Ok(status)
    }
}
#[cfg(feature = "local-inference")]
fn cpu_model_admission(profile: &Profile) -> Result<ExecutionProfile> {
    ensure!(
        profile.device_execution.is_none(),
        "GPU profile cannot be projected as CPU"
    );
    ensure!(
        cpu_model_allowed(profile.model, runtime::is_local_test_profile(profile)),
        "known failed unqualified Large CPU is not Auto fallback"
    );
    ExecutionProfile::from_release(profile.manifest)
}
#[cfg(feature = "local-inference")]
fn cpu_candidate(profile: &Profile) -> Result<ExecutionProfile> {
    let execution = cpu_model_admission(profile)?;
    let fit = profile
        .cpu_fit
        .as_ref()
        .context("CPU variant has no declared fit")?;
    ensure!(
        airk_process::cpu_fit::usable_physical_cores()
            .is_some_and(|actual| actual >= fit.minimum_physical_cores),
        "native CPU fit unavailable"
    );
    Ok(execution)
}
#[cfg(feature = "local-inference")]
pub(super) enum DeviceAction {
    Warm,
    RecoverAfterFault {
        key: DeviceFaultKey,
        release: airk_kernel::service_resources::OptionalRelease,
        original_cancel: CancellationToken,
    },
    Recover,
    Provision(Profile),
    Import(Profile, std::path::PathBuf),
    Remove(airk_core::identity::Sha256Digest),
}
#[cfg(feature = "local-inference")]
#[derive(Clone)]
struct Launch {
    stage: Stage,
    intent_generation: u64,
    document: DeviceConfiguration,
    binding: DeviceCatalogBinding,
    configuration: Arc<airk_store::service::arbiter::ConfigurationStore>,
    devices: Arc<Shared>,
    lifecycle: Arc<Mutex<super::lifecycle::State>>,
    gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    lease: Arc<airk_store::service::ServiceLease>,
    assets: Arc<AssetStore>,
    legacy: Arc<AssetStore>,
    management: airk_supervisor::project_supervisor::ArbiterManagement,
    deadline: Instant,
}
#[cfg(feature = "local-inference")]
impl Launch {
    fn current(&self, cancel: Option<&CancellationToken>) -> Result<()> {
        ensure!(
            Instant::now() < self.deadline,
            "original device lifecycle deadline expired"
        );
        self.policy_current(cancel)
    }
    fn policy_current(&self, cancel: Option<&CancellationToken>) -> Result<()> {
        ensure!(
            cancel.is_none_or(|v| !v.is_cancelled()),
            "original device lifecycle canceled"
        );
        continuation_current(
            &self
                .devices
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("device intent unavailable"))?
                .intent_generation,
            self.stage,
            self.intent_generation,
        )?;
        ensure!(
            self.gate
                .catalog_clock()
                .is_some_and(|clock| clock.namespace == self.document.catalog),
            airk_core::scoped_reads::ScopeReadRefused
        );
        let airk_store::service::arbiter::device::StoredConfiguration::Device(current) =
            self.configuration.device(self.binding.clone()).load()?
        else {
            anyhow::bail!("device state replaced by legacy document")
        };
        ensure!(
            current == self.document,
            "original device lifecycle configuration changed"
        );
        validate_current_enrollments(
            &self.gate,
            &self.lease,
            self.document.catalog,
            &current,
            self.stage,
        )?;
        Ok(())
    }
    async fn drain(
        &self,
        handle: &Handle,
        profile: airk_core::identity::Sha256Digest,
    ) -> Result<()> {
        let release = handle
            .resident(profile)
            .ok()
            .map(|v| v.reservation().release_observer());
        handle.release_profile(profile)?;
        handle.wait_profile_released(profile, self.deadline).await?;
        if let Some(release) = release {
            tokio::time::timeout_at(
                tokio::time::Instant::from_std(self.deadline),
                release.wait(),
            )
            .await
            .context("native device host charge remains unsettled")??;
        }
        Ok(())
    }
    fn asset_store(&self, profile: &Profile) -> Result<Arc<AssetStore>> {
        // Reuse actual original CPU custody read-only; GPU never enters v3 assets.
        if profile.device_execution.is_none() && self.legacy.phase(profile)? == Some(Phase::Ready) {
            return Ok(self.legacy.clone());
        }
        Ok(self.assets.clone())
    }
}
#[cfg(feature = "local-inference")]
impl StageOwner {
    pub(super) fn launch_device(
        &self,
        document: DeviceConfiguration,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
        effect: Option<airk_https::admin_plane::NativeDispatchCheck>,
        action: DeviceAction,
    ) -> Result<()> {
        catalog_live(self, effect.as_ref())?;
        validate_device_enrollments(self, &document, self.stage)?;
        let binding = binding()?;
        ensure!(
            binding == document.device_catalog,
            "compiled device catalog/source changed"
        );
        let assets =
            AssetStore::open_device(self.ownership.clone(), self.installation, self.namespace)?;
        let intent_generation = self
            .devices
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("device intent unavailable"))?
            .intent_generation
            .get(&self.stage)
            .copied()
            .unwrap_or(0);
        let context = Launch {
            stage: self.stage,
            intent_generation,
            document,
            binding,
            configuration: self.configuration.clone(),
            devices: self.devices.clone(),
            lifecycle: self.lifecycle.clone(),
            gate: self.gate.clone(),
            lease: self.ownership.clone(),
            assets,
            legacy: self.assets.clone(),
            management: supervisor.arbiter_management(),
            deadline: Instant::now() + Duration::from_secs(120),
        };
        context.queue(action, tokio::runtime::Handle::current())
    }
}
#[cfg(feature = "local-inference")]
impl Launch {
    fn queue(self, action: DeviceAction, runtime: tokio::runtime::Handle) -> Result<()> {
        self.policy_current(None)?;
        let shared = self.devices.clone();
        let lifecycle = self.lifecycle.clone();
        let stage = self.stage;
        let mut life = lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("device lifecycle unavailable"))?;
        ensure!(!life.stopping, "device lifecycle stopping");
        let mut state = shared
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("device continuation unavailable"))?;
        continuation_current(&state.intent_generation, stage, self.intent_generation)?;
        // An internal fault continuation cannot replace accepted user work.
        if matches!(&action, DeviceAction::RecoverAfterFault { .. })
            && state.pending.contains_key(&stage)
        {
            return Ok(());
        }
        // One pending command per stage, replacing only work not yet admitted.
        state.pending.insert(stage, (self, action));
        if !state.runners.insert(stage) {
            return Ok(());
        }
        life.device_tasks = life
            .device_tasks
            .checked_add(1)
            .context("device lifecycle owner count exhausted")?;
        drop(state);
        drop(life);
        runtime.spawn(async move {
            loop {
                let next = {
                    let mut life = lifecycle
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let mut state = shared
                        .state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if life.stopping {
                        state.pending.remove(&stage);
                    }
                    match state.pending.remove(&stage) {
                        Some(command) => Some(command),
                        None => {
                            state.runners.remove(&stage);
                            life.device_tasks -= 1;
                            life.settled.notify_waiters();
                            None
                        }
                    }
                };
                let Some((context, action)) = next else { break };
                let result = context.run_command(action).await;
                if let Err(error) = result {
                    let mut state = lifecycle
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if !state.busy && state.phase != Availability::Draining {
                        state.phase = Availability::Failed;
                        state.reason =
                            bounded_reason(format!("Native device lifecycle refused: {error:#}"));
                    }
                }
                drop(context);
            }
        });
        Ok(())
    }
    async fn run_command(&self, action: DeviceAction) -> Result<()> {
        let settled = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("device lifecycle unavailable"))?
            .settled
            .clone();
        loop {
            let changed = settled.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            self.current(None)?;
            let busy = {
                let state = self
                    .lifecycle
                    .lock()
                    .map_err(|_| anyhow::anyhow!("device lifecycle unavailable"))?;
                ensure!(!state.stopping, "device lifecycle stopping");
                state.busy
            };
            if !busy {
                break;
            }
            tokio::time::timeout_at(tokio::time::Instant::from_std(self.deadline), changed)
                .await
                .context("prior native lifecycle remains unsettled")?;
        }
        self.current(None)?;
        let manifest = match &action {
            DeviceAction::Provision(profile) | DeviceAction::Import(profile, _) => profile.manifest,
            DeviceAction::Remove(manifest) => *manifest,
            DeviceAction::RecoverAfterFault { key, .. } => key.profile,
            DeviceAction::Warm | DeviceAction::Recover => runtime::catalog_profiles()?
                .into_iter()
                .find(|profile| {
                    SelectedModel::from_model(profile.model).ok()
                        == self.document.stage(self.stage).selected_model
                })
                .map(|p| p.manifest)
                .unwrap_or(airk_core::identity::Sha256Digest::from_bytes([0; 32])),
        };
        let (serial, cancel) =
            super::lifecycle::start_checked(&self.lifecycle, manifest, None, || {
                self.current(None)
            })?;
        let result = self.execute(action, serial, &cancel).await;
        super::lifecycle::complete(&self.lifecycle, serial, &result);
        result
    }
    fn finish_resident_activation(
        &self,
        handle: &Handle,
        profile: &Profile,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let eligibilities = handle.resident_eligibilities()?;
        self.devices
            .finish_activation(&self.document, self.stage, profile, eligibilities, || {
                self.current(Some(cancel))?;
                let observed = handle.observation_for(self.stage)?;
                ensure!(
                    observed.active_profile == Some(profile.manifest)
                        && observed.configuration_revision == Some(self.document.revision),
                    "exact native resident/configuration not observed"
                );
                let resident = handle.resident(profile.manifest)?;
                ensure!(
                    resident.reservation().bytes() >= profile.memory_reservation_bytes,
                    "actual native resident charge is insufficient"
                );
                {
                    let mut lifecycle = self
                        .lifecycle
                        .lock()
                        .map_err(|_| anyhow::anyhow!("native lifecycle unavailable"))?;
                    ensure!(
                        !lifecycle.stopping && !cancel.is_cancelled(),
                        "native activation canceled"
                    );
                    lifecycle.profile = Some(profile.manifest);
                }
                self.fault_notifier(handle, profile, cancel)?;
                self.current(Some(cancel))
            })
    }
    fn require_open_retained_population(
        &self,
        handle: &Handle,
        cancel: &CancellationToken,
    ) -> Result<()> {
        self.current(Some(cancel))?;
        if self.document.stage(self.stage).device_policy == DevicePolicy::Cpu
            || !self.devices.gpu_allowed()
        {
            return Ok(());
        }
        let actual = [Stage::Focus, Stage::Context]
            .into_iter()
            .map(|stage| {
                handle
                    .observation_for(stage)
                    .map(|value| (stage, value.active_profile))
            })
            .collect::<Result<Vec<_>>>()?;
        let state = self
            .devices
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("retained GPU circuit observation unavailable"))?;
        ensure!(
            state.document.as_ref() == Some(&self.document),
            "retained population source/configuration changed"
        );
        let targets = retained_gpu_targets(
            actual,
            state.selected.iter().map(|(stage, profile)| {
                (*stage, profile.manifest, profile.device_execution.is_some())
            }),
        );
        for (stage, manifest) in targets {
            let profile = state
                .selected
                .get(&stage)
                .filter(|profile| profile.manifest == manifest)
                .context("retained GPU selection changed")?;
            let (device, epoch) = state
                .devices
                .get(&stage)
                .context("retained GPU device observation missing")?;
            ensure!(
                !device.poisoned() && device.epoch() == *epoch,
                "retained GPU settlement/epoch unknown"
            );
            let key = airk_inference::inference_engine::pool::arbiter::circuit_key(
                profile,
                device,
                &self.document,
            )?;
            ensure!(
                !state.circuits.contains(&key),
                "an actual retained GPU eligibility circuit is open; recover that affected stage explicitly before reaccepting this population"
            );
        }
        Ok(())
    }
    fn record_prework_fault(
        &self,
        handle: &Handle,
        current: Sha256Digest,
        error: &anyhow::Error,
        cancel: &CancellationToken,
    ) -> Result<Option<Sha256Digest>> {
        let failed = recordable_fault_profile(
            prework_device_failure(error),
            handle.unknown_device_work(),
            airk_inference::inference_engine::pool::arbiter::eligibility_failure_profile(error),
            current,
        );
        let Some(failed) = failed else {
            return Ok(None);
        };
        if self.current(Some(cancel)).is_err() {
            return Ok(None);
        }
        let mut state = self
            .devices
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native fault recorder unavailable"))?;
        ensure!(
            state.document.as_ref() == Some(&self.document),
            "fault source/configuration changed"
        );
        let profile = state
            .selected
            .values()
            .find(|description| description.manifest == failed)
            .context("actual failed native resident is no longer selected")?;
        let stage = profile
            .declared_stage()
            .context("failed native stage absent")?;
        let (device, epoch) = state
            .devices
            .get(&stage)
            .context("actual failed device is absent")?;
        if device.poisoned() || device.epoch() != *epoch {
            return Ok(None);
        }
        let key = airk_inference::inference_engine::pool::arbiter::circuit_key(
            profile,
            device,
            &self.document,
        )?;
        ensure!(
            state.circuits.contains(&key) || state.circuits.len() < 16,
            "native fault circuit capacity occupied"
        );
        state.circuits.insert(key);
        Ok(Some(failed))
    }
    fn fault_notifier(
        &self,
        handle: &Handle,
        profile: &Profile,
        cancel: &CancellationToken,
    ) -> Result<()> {
        if self.document.stage(self.stage).device_policy != DevicePolicy::Auto
            || profile.device_execution.is_none()
        {
            return Ok(());
        }
        let context = self.clone();
        let token = cancel.clone();
        let runtime = tokio::runtime::Handle::current();
        handle.device_failure_notifier(
            self.stage,
            Arc::new(move |fault: DeviceFault| {
                if context.policy_current(Some(&token)).is_err()
                    || !context.document.stage(context.stage).enabled
                {
                    return;
                }
                let key = fault.key().clone();
                {
                    let mut state = context
                        .devices
                        .state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if state.document.as_ref() != Some(&context.document)
                        || state.selected.get(&context.stage).map(|p| p.manifest)
                            != Some(key.profile)
                        || state.circuits.contains(&key)
                        || state.circuits.len() >= 16
                    {
                        return;
                    }
                    state.circuits.insert(key.clone());
                    state.ready.remove(&context.stage);
                }
                // New future-only management operation. The failed read is never
                // replayed, rebound, renewed or marked successful by this callback.
                let mut future = context.clone();
                future.deadline = Instant::now() + Duration::from_secs(120);
                let _ = future.queue(
                    DeviceAction::RecoverAfterFault {
                        key,
                        release: fault.release(),
                        original_cancel: token.clone(),
                    },
                    runtime.clone(),
                );
            }),
        )
    }
}

#[cfg(feature = "local-inference")]
impl Launch {
    async fn execute(
        &self,
        action: DeviceAction,
        serial: u64,
        cancel: &CancellationToken,
    ) -> Result<()> {
        self.current(Some(cancel))?;
        match action {
            DeviceAction::RecoverAfterFault {
                key,
                release,
                original_cancel,
            } => {
                self.policy_current(Some(&original_cancel))?;
                ensure!(
                    self.document.stage(self.stage).enabled
                        && self.document.stage(self.stage).device_policy == DevicePolicy::Auto
                        && key.configuration_revision == self.document.revision
                        && key.device_catalog == self.document.device_catalog.catalog
                        && key.catalog_source == self.document.device_catalog.source,
                    "future recovery consent/model/catalog epoch changed"
                );
                super::lifecycle::phase(
                    &self.lifecycle,
                    serial,
                    Availability::Draining,
                    "GPU fault returned; waiting for actual session, pins and memory-charge release before future cached fallback.",
                )?;
                tokio::time::timeout_at(
                    tokio::time::Instant::from_std(self.deadline),
                    release.wait(),
                )
                .await
                .context("GPU session settlement remains unknown; no future fallback")??;
                self.policy_current(Some(&original_cancel))?;
                self.current(Some(cancel))?;
            }
            DeviceAction::Provision(profile) => {
                profile.validate_release_downloads()?;
                super::lifecycle::phase(
                    &self.lifecycle,
                    serial,
                    Availability::Downloading,
                    "Downloading exact native compiled variant assets; no enablement or enrollment is added.",
                )?;
                super::assets::provision(
                    &profile,
                    &self.assets,
                    self.document.revision,
                    cancel,
                    None,
                )
                .await?;
            }
            DeviceAction::Import(profile, path) => {
                let store = self.assets.clone();
                let revision = self.document.revision;
                let configuration = self.configuration.clone();
                let binding = self.binding.clone();
                let expected = self.document.clone();
                let token = cancel.clone();
                let deadline = self.deadline;
                tokio::task::spawn_blocking(move || {
                    let check = || {
                        ensure!(
                            Instant::now() < deadline && !token.is_cancelled(),
                            "original import deadline/cancellation expired"
                        );
                        let airk_store::service::arbiter::device::StoredConfiguration::Device(
                            current,
                        ) = configuration.device(binding.clone()).load()?
                        else {
                            anyhow::bail!("device import state replaced")
                        };
                        ensure!(current == expected, "device import configuration changed");
                        Ok(())
                    };
                    let bundle = airk_store::service::arbiter_assets::OfflineBundle::open(
                        &profile, &path, check,
                    )?;
                    bundle.import(&store, &profile, revision, check)
                })
                .await
                .context("native device import owner closed")??;
            }
            DeviceAction::Remove(manifest) => {
                let handle = self.management.existing().await.ok();
                if let Some(handle) = handle {
                    super::lifecycle::phase(
                        &self.lifecycle,
                        serial,
                        Availability::Draining,
                        "Waiting for actual native device settlement and final host charge before removing assets.",
                    )?;
                    self.drain(&handle, manifest).await?;
                }
                self.current(Some(cancel))?;
                let store = self.assets.clone();
                let revision = self.document.revision;
                tokio::task::spawn_blocking(move || {
                    if store.owned_observation(manifest)?.is_some() {
                        store.remove_owned(manifest, revision)?;
                    }
                    Ok::<_, anyhow::Error>(())
                })
                .await
                .context("device asset removal owner closed")??;
                return super::lifecycle::phase(
                    &self.lifecycle,
                    serial,
                    Availability::MissingAssets,
                    "Current device custody was cleaned after actual native settlement; any reused legacy CPU cache retains its original read-only custody.",
                );
            }
            DeviceAction::Recover => {
                ensure!(
                    self.assets
                        .pending_owned_removals_for(self.stage, self.document.revision)?
                        .is_empty(),
                    "interrupted removal requires exact named cleanup; recovery cannot resume acquisition"
                );
            }
            DeviceAction::Warm => {}
        }
        self.current(Some(cancel))?;
        let snapshot = self.devices.policy(self.document.clone())?;
        let handle = self
            .management
            .ensure_native(snapshot, self.deadline)
            .await?;
        {
            let mut state = self
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("device lifecycle unavailable"))?;
            ensure!(
                state.serial == serial && !state.stopping,
                "device lifecycle superseded"
            );
            state.handle = Some(handle.clone());
        }
        let requested = self.document.stage(self.stage);
        if !requested.enabled {
            let observation = handle.observation_for(self.stage)?;
            for profile in [observation.active_profile, observation.warming_profile]
                .into_iter()
                .flatten()
            {
                self.drain(&handle, profile).await?;
            }
            let current = self.devices.install(
                self.document.clone(),
                self.stage,
                None,
                None,
                None,
                None,
                None,
            )?;
            handle.configure_native(current)?;
            return super::lifecycle::phase(
                &self.lifecycle,
                serial,
                Availability::Disabled,
                "Device stage disabled; actual native work and charges drained.",
            );
        }
        let model = requested
            .selected_model
            .context("enabled device model missing")?;
        let probe_live = {
            let configuration = self.configuration.clone();
            let binding = self.binding.clone();
            let expected = self.document.clone();
            let token = cancel.clone();
            let gate = self.gate.clone();
            let deadline = self.deadline;
            Arc::new(move || {
                Instant::now()<deadline&&!token.is_cancelled()&&gate.catalog_clock().is_some_and(|clock|clock.namespace==expected.catalog)
                &&configuration.device(binding.clone()).load().is_ok_and(|value|matches!(value,airk_store::service::arbiter::device::StoredConfiguration::Device(current) if current==expected))
            }) as Arc<dyn Fn() -> bool + Send + Sync>
        };
        let mut candidates = Vec::new();
        let mut rejections = Vec::new();
        let gpu_allowed = self.devices.gpu_allowed();
        if requested.device_policy == DevicePolicy::Cpu || !gpu_allowed {
            rejections.push(CandidateRejection {
                manifest: None,
                adapter_key: None,
                reason: SelectionReason::ForcedCpu,
            });
        }
        let mut cold_probe_charge: Option<
            Arc<airk_kernel::service_resources::OptionalReservation>,
        > = None;
        let mut probed = None;
        if requested.device_policy == DevicePolicy::Auto && gpu_allowed {
            let mut eligible = runtime::catalog_profiles()?
                .into_iter()
                .filter(|profile| {
                    profile.model == model.model() && profile.device_execution.is_some()
                })
                .filter(|profile| {
                    gpu_precision_allows(
                        requested.gpu_precision,
                        profile.device_execution.as_ref().map(|v| v.precision),
                    )
                })
                .filter(|profile| runtime::release_asset_profile(profile.manifest).is_ok())
                .collect::<Vec<_>>();
            eligible.sort_by_key(|profile| {
                (
                    precision_rank(profile.device_execution.as_ref().map(|v| v.precision)),
                    profile.manifest,
                )
            });
            if let Some(first) = eligible.first() {
                if cfg!(feature = "gpu-cuda") {
                    let existing = {
                        let state =
                            self.devices.state.lock().map_err(|_| {
                                anyhow::anyhow!("native runtime charge unavailable")
                            })?;
                        state
                            .selected
                            .iter()
                            .filter_map(|(stage, profile)| {
                                state
                                    .devices
                                    .get(stage)
                                    .filter(|(device, epoch)| {
                                        !device.poisoned() && device.epoch() == *epoch
                                    })
                                    .filter(|_| {
                                        profile.device_execution.as_ref().map(|v| v.runtime_build)
                                            == first
                                                .device_execution
                                                .as_ref()
                                                .map(|v| v.runtime_build)
                                    })
                                    .map(|_| (*stage, profile.manifest))
                            })
                            .collect::<Vec<_>>()
                    };
                    let borrowed = existing.into_iter().find_map(|(_, manifest)| {
                        handle
                            .resident(manifest)
                            .ok()
                            .map(|resident| (manifest, resident.reservation()))
                    });
                    let (host, new_charge) = match borrowed {
                        Some((manifest, charge)) => {
                            super::lifecycle::remember_charge(
                                &self.lifecycle,
                                manifest,
                                charge.release_observer(),
                            )
                            .await?;
                            (Some(charge), false)
                        }
                        None => match self
                            .management
                            .reserve(first.memory_reservation_bytes)
                            .await
                        {
                            Ok(charge) => {
                                let charge = Arc::new(charge);
                                super::lifecycle::remember_charge(
                                    &self.lifecycle,
                                    first.manifest,
                                    charge.release_observer(),
                                )
                                .await?;
                                (Some(charge), true)
                            }
                            Err(_) => {
                                self.current(Some(cancel))?;
                                rejections.push(CandidateRejection {
                                    manifest: Some(first.manifest),
                                    adapter_key: None,
                                    reason: SelectionReason::HostMemoryInsufficient,
                                });
                                (None, false)
                            }
                        },
                    };
                    if let Some(host) = host {
                        let receive = handle.probe_devices(
                            self.lease.clone(),
                            model,
                            eligible.iter().map(|profile| profile.manifest).collect(),
                            host.clone(),
                            self.deadline,
                            {
                                let live = probe_live.clone();
                                let devices = self.devices.clone();
                                Arc::new(move || devices.gpu_allowed() && live())
                            },
                        )?;
                        probed = Some(
                            tokio::time::timeout_at(
                                tokio::time::Instant::from_std(self.deadline),
                                receive,
                            )
                            .await
                            .context("original charged device probe deadline")?
                            .context("native device probe closed")??,
                        );
                        if new_charge {
                            cold_probe_charge = Some(host);
                        }
                    }
                } else {
                    for profile in eligible {
                        rejections.push(CandidateRejection {
                            manifest: Some(profile.manifest),
                            adapter_key: None,
                            reason: SelectionReason::RuntimeUnavailable,
                        });
                    }
                }
            } else {
                rejections.push(CandidateRejection {
                    manifest: None,
                    adapter_key: None,
                    reason: SelectionReason::NoGpu,
                });
            }
        }
        if let Some(probe) = probed {
            if probe.candidates.is_empty() && probe.rejections.is_empty() {
                rejections.push(CandidateRejection {
                    manifest: None,
                    adapter_key: None,
                    reason: SelectionReason::NoGpu,
                });
            }
            rejections.extend(probe.rejections);
            for candidate in probe.candidates {
                ensure!(
                    !candidate.probe_only,
                    "unqualified native evaluation cannot enter product selection"
                );
                let precision = candidate
                    .profile
                    .description()
                    .device_execution
                    .as_ref()
                    .context("bound GPU descriptor missing")?
                    .precision;
                let permitted = gpu_precision_allows(requested.gpu_precision, Some(precision));
                if permitted {
                    candidates.push(Candidate {
                        execution: candidate.profile,
                        device: Some(candidate.device),
                        epoch: candidate.epoch,
                        adapter: adapter(candidate.uuid),
                        extra: Some(candidate.host_warm_extra_bytes),
                    });
                } else {
                    rejections.push(CandidateRejection {
                        manifest: Some(candidate.profile.description().manifest),
                        adapter_key: Some(adapter(candidate.uuid)),
                        reason: SelectionReason::GpuPrecisionExcluded,
                    });
                }
            }
        }
        for profile in runtime::catalog_profiles()?
            .into_iter()
            .filter(|profile| profile.model == model.model() && profile.device_execution.is_none())
        {
            if let Ok(execution) = cpu_candidate(&profile) {
                candidates.push(Candidate {
                    execution,
                    device: None,
                    epoch: 1,
                    adapter: "cpu".into(),
                    extra: None,
                });
            } else {
                rejections.push(CandidateRejection {
                    manifest: Some(profile.manifest),
                    adapter_key: Some("cpu".into()),
                    reason: if cpu_model_admission(&profile).is_ok() {
                        SelectionReason::CpuFitInsufficient
                    } else {
                        SelectionReason::NoQualifiedCpu
                    },
                });
            }
        }
        candidates.sort_by_key(|candidate| {
            (
                precision_rank(
                    candidate
                        .execution
                        .description()
                        .device_execution
                        .as_ref()
                        .map(|value| value.precision),
                ),
                candidate.adapter.clone(),
                candidate.execution.description().manifest,
            )
        });
        for candidate in candidates {
            self.current(Some(cancel))?;
            if candidate
                .device
                .as_ref()
                .is_some_and(|device| device.poisoned() || device.epoch() != candidate.epoch)
            {
                anyhow::bail!("native device epoch changed or settlement unknown");
            }
            let profile = candidate.execution.description().clone();
            let probe_live = if candidate.device.is_some() {
                let devices = self.devices.clone();
                let live = probe_live.clone();
                Arc::new(move || devices.gpu_allowed() && live())
                    as Arc<dyn Fn() -> bool + Send + Sync>
            } else {
                probe_live.clone()
            };
            if let Some(device) = &candidate.device {
                let key = airk_inference::inference_engine::pool::arbiter::circuit_key(
                    &profile,
                    device,
                    &self.document,
                )?;
                if self
                    .devices
                    .state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native device circuit unavailable"))?
                    .circuits
                    .contains(&key)
                {
                    rejections.push(CandidateRejection {
                        manifest: Some(profile.manifest),
                        adapter_key: Some(candidate.adapter),
                        reason: SelectionReason::CircuitOpen,
                    });
                    continue;
                }
            }

            let store = self.asset_store(&profile)?;
            if store.phase(&profile)? != Some(Phase::Ready) {
                rejections.push(CandidateRejection {
                    manifest: Some(profile.manifest),
                    adapter_key: Some(candidate.adapter),
                    reason: SelectionReason::Unqualified,
                });
                continue;
            }
            let evidence = candidate.execution.device_evidence();
            let selection = DeviceSelection {
                schema: DEVICE_STATUS_SCHEMA.into(),
                model,
                policy: requested.device_policy,
                gpu_precision: requested.gpu_precision,
                selected: Some(SelectedVariant {
                    manifest: profile.manifest,
                    execution_identity: profile.device_execution_identity()?,
                    backend: profile.device_execution.as_ref().map(|v| v.backend),
                    precision: profile.device_execution.as_ref().map(|v| v.precision),
                    adapter_key: candidate.adapter.clone(),
                    capability_epoch: candidate.epoch,
                }),
                reasons: rejections.clone(),
                operation_failure: None,
            };
            let retained_device = {
                let state = self
                    .devices
                    .state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native resident selection unavailable"))?;
                if state
                    .selected
                    .get(&self.stage)
                    .is_some_and(|actual| actual.manifest == profile.manifest)
                {
                    match (&candidate.device,state.devices.get(&self.stage)) {
                        (None,None)=>Some(None),
                        (Some(candidate_device),Some((actual,epoch))) if !actual.poisoned()&&actual.epoch()==*epoch&&actual.epoch()==candidate_device.epoch()
                            &&airk_inference::inference_engine::pool::arbiter::circuit_key(&profile,actual,&self.document)?==airk_inference::inference_engine::pool::arbiter::circuit_key(&profile,candidate_device,&self.document)?=>Some(Some(actual.clone())),
                        _=>None,
                    }
                } else {
                    None
                }
            };
            if handle.observation_for(self.stage)?.active_profile == Some(profile.manifest)
                && handle.resident(profile.manifest).is_ok()
                && let Some(device) = retained_device
            {
                self.current(Some(cancel))?;
                let policy = self.devices.install(
                    self.document.clone(),
                    self.stage,
                    Some(profile.clone()),
                    Some(selection.clone()),
                    device,
                    candidate.extra,
                    evidence.clone(),
                )?;
                handle.configure_native(policy)?;
                if profile.device_execution.is_some()
                    && !handle
                        .resident_eligibilities()?
                        .get(&self.stage)
                        .is_some_and(ResidentEligibility::current)
                {
                    self.require_open_retained_population(&handle, cancel)?;
                    let resident = handle.resident(profile.manifest)?;
                    let receive = handle.reaccept_device(
                        self.lease.clone(),
                        model,
                        resident,
                        self.deadline,
                        probe_live.clone(),
                    )?;
                    let returned = tokio::time::timeout_at(
                        tokio::time::Instant::from_std(self.deadline),
                        receive,
                    )
                    .await
                    .context("original retained eligibility cutoff exhausted")?
                    .context("retained eligibility owner closed without an observed outcome")?;
                    if let Err(error) = returned {
                        if let Err(guard) =
                            self.record_prework_fault(&handle, profile.manifest, &error, cancel)
                        {
                            return Err(secondary_fault_error(error, guard));
                        }
                        return Err(error.context("retained local eligibility failed; original management attempt stops without replay"));
                    }
                    self.current(Some(cancel))?;
                }
                self.finish_resident_activation(&handle, &profile, cancel)?;
                return super::lifecycle::phase(
                    &self.lifecycle,
                    serial,
                    Availability::Ready,
                    "The exact authenticated native resident and original peak charge were retained; no second model load or reservation was created.",
                );
            }
            let observation = handle.observation_for(self.stage)?;
            for old in [observation.active_profile, observation.warming_profile]
                .into_iter()
                .flatten()
            {
                self.drain(&handle, old).await?;
            }
            self.current(Some(cancel))?;
            self.require_open_retained_population(&handle, cancel)?;
            let reservation = if cold_probe_charge
                .as_ref()
                .is_some_and(|charge| charge.bytes() >= profile.memory_reservation_bytes)
            {
                cold_probe_charge
                    .take()
                    .context("charged native probe capacity disappeared")?
            } else {
                if let Some(charge) = cold_probe_charge.take() {
                    let released = charge.release_observer();
                    drop(charge);
                    tokio::time::timeout_at(
                        tokio::time::Instant::from_std(self.deadline),
                        released.wait(),
                    )
                    .await
                    .context("actual cold probe host charge remains held")??;
                }
                match self
                    .management
                    .reserve(profile.memory_reservation_bytes)
                    .await
                {
                    Ok(reservation) => Arc::new(reservation),
                    Err(_) => {
                        self.current(Some(cancel))?;
                        rejections.push(CandidateRejection {
                            manifest: Some(profile.manifest),
                            adapter_key: Some(candidate.adapter),
                            reason: SelectionReason::HostMemoryInsufficient,
                        });
                        continue;
                    }
                }
            };
            super::lifecycle::remember_charge(
                &self.lifecycle,
                profile.manifest,
                reservation.release_observer(),
            )
            .await?;
            let config = self.devices.install(
                self.document.clone(),
                self.stage,
                Some(profile.clone()),
                Some(selection),
                candidate.device.clone(),
                candidate.extra,
                evidence,
            )?;
            handle.configure_native(config)?;
            super::lifecycle::phase(
                &self.lifecycle,
                serial,
                Availability::Warming,
                "Loading exact admitted variant inside the original native owner and host/device reservations.",
            )?;
            let release = reservation.release_observer();
            let source = super::assets::source_for_store(profile.clone(), store);
            let receive = handle.warm_with_charge(
                candidate.execution,
                source,
                reservation,
                self.deadline,
                probe_live.clone(),
            )?;
            let loaded =
                tokio::time::timeout_at(tokio::time::Instant::from_std(self.deadline), receive)
                    .await;
            match loaded {
                Ok(Ok(Ok(()))) => {
                    self.current(Some(cancel))?;
                    let actual = handle.observation_for(self.stage)?;
                    ensure!(
                        actual.active_profile == Some(profile.manifest)
                            && actual.configuration_revision == Some(self.document.revision),
                        "actual native device activation missing"
                    );
                    ensure!(
                        candidate.device.as_ref().is_none_or(
                            |device| !device.poisoned() && device.epoch() == candidate.epoch
                        ),
                        "device epoch changed after native load"
                    );
                    self.finish_resident_activation(&handle, &profile, cancel)?;
                    return super::lifecycle::phase(
                        &self.lifecycle,
                        serial,
                        Availability::Ready,
                        if profile.device_execution.is_some() {
                            "Authenticated GPU execution is active for the selected model on the single metadata-selected compatible dedicated CUDA adapter; other adapters were not initialized."
                        } else if !self.devices.gpu_allowed() {
                            "FP32 CPU execution is active because GPU use is disabled in service resources."
                        } else if requested.device_policy == DevicePolicy::Cpu {
                            "FP32 CPU execution is active by request."
                        } else {
                            "FP32 CPU fallback is active for the selected model. GPU policy considers one compatible dedicated CUDA adapter by physical capacity; other adapters are not initialized or claimed unable to fit."
                        },
                    );
                }
                failure => {
                    let error = match failure {
                        Ok(Ok(Err(error))) => error,
                        Ok(Err(error)) => anyhow::anyhow!(error)
                            .context("original native warm response owner closed"),
                        Err(error) => {
                            anyhow::anyhow!(error).context("original native warm deadline expired")
                        }
                        Ok(Ok(Ok(()))) => unreachable!("successful activation handled above"),
                    };
                    super::lifecycle::phase(
                        &self.lifecycle,
                        serial,
                        Availability::Draining,
                        "Variant did not activate; actual native settlement and charge release are required before any fallback.",
                    )?;
                    super::lifecycle::release_actual(
                        &handle,
                        profile.manifest,
                        release,
                        self.deadline,
                    )
                    .await?;
                    ensure!(
                        candidate
                            .device
                            .as_ref()
                            .is_none_or(|device| !device.poisoned()),
                        "device settlement unknown; automatic fallback refused"
                    );
                    self.current(Some(cancel))?;
                    // Source/input/cancellation/deadline/owner errors are not device
                    // faults and cannot be relabeled as a fallback authorization.
                    let recorded = match self.record_prework_fault(
                        &handle,
                        profile.manifest,
                        &error,
                        cancel,
                    ) {
                        Ok(recorded) => recorded,
                        Err(guard) => return Err(secondary_fault_error(error, guard)),
                    };
                    match recorded {
                        Some(failed) if failed==profile.manifest=>{},
                        Some(_)=>return Err(error.context("a retained GPU resident failed eligibility; current candidate is not declared unfit and the original attempt stops")),
                        None=>return Err(error.context("unsettled or non-device warm failure; automatic fallback and fault circuit refused")),
                    }
                    rejections.push(CandidateRejection {
                        manifest: Some(profile.manifest),
                        adapter_key: Some(candidate.adapter),
                        reason: prework_device_reason(&error),
                    });
                }
            }
        }
        anyhow::bail!(
            "No admitted same-model variant fits current runtime/device/host policy; ordinary evidence remains available"
        )
    }
}
#[cfg(feature = "local-inference")]
struct Candidate {
    execution: ExecutionProfile,
    device: Option<NativeDevice>,
    epoch: u64,
    adapter: String,
    extra: Option<u64>,
}
#[cfg(feature = "local-inference")]
fn adapter(uuid: [u8; 16]) -> String {
    uuid.iter().map(|value| format!("{value:02x}")).collect()
}
#[cfg(feature = "local-inference")]
pub(super) struct Delivery {
    pub binding: DeviceCatalogBinding,
    pub policy: NativeConfiguration,
    shared: Arc<Shared>,
    generation: u64,
    devices: Vec<(NativeDevice, u64)>,
    eligibilities: BTreeMap<Stage, ResidentEligibility>,
}
#[cfg(feature = "local-inference")]
impl Delivery {
    pub fn live(&self) -> Result<()> {
        ensure!(
            self.devices.is_empty() || self.shared.gpu_allowed(),
            "GPU delivery disabled by service policy"
        );
        ensure!(
            self.shared
                .generation
                .load(std::sync::atomic::Ordering::Acquire)
                == self.generation,
            "native selected variant changed before delivery"
        );
        ensure!(
            self.devices
                .iter()
                .all(|(device, epoch)| !device.poisoned() && device.epoch() == *epoch),
            "native device epoch/settlement changed before delivery"
        );
        ensure!(
            self.eligibilities
                .values()
                .all(ResidentEligibility::current),
            "local GPU eligibility changed before delivery"
        );
        Ok(())
    }
    pub fn current(&self, current: &DeviceConfiguration) -> Result<()> {
        self.live()?;
        let NativeConfiguration::Device { document, .. } = &self.policy else {
            anyhow::bail!("device delivery cannot project legacy configuration")
        };
        ensure!(
            document == current,
            "device configuration changed before delivery"
        );
        Ok(())
    }
    pub fn with_selection<T>(
        &self,
        current: &DeviceConfiguration,
        expected: &[airk_core::combined_context::ContextDeliveryBinding],
        effect: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        // Lock order: Store configuration -> native selection -> credential/source/IO.
        // install never calls Store, native gates or lifecycle while holding state.
        let state = self
            .shared
            .state
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native selection commit is busy"))?;
        self.current(current)?;
        ensure!(
            state.document.as_ref() == Some(current),
            "native selected document changed before commit"
        );
        let actual = NativeConfiguration::device(
            current.clone(),
            state.selected.values().cloned().collect(),
        )?;
        ensure!(
            actual.same_selection(&self.policy),
            "native selected variant changed before commit"
        );
        for binding in expected {
            if binding.profile.is_some()
                && let NativeConfiguration::Device { selected, .. } = &self.policy
                && selected
                    .get(&binding.stage)
                    .is_some_and(|profile| profile.device_execution.is_some())
            {
                ensure!(
                    state.ready.contains(&binding.stage)
                        && self
                            .eligibilities
                            .get(&binding.stage)
                            .is_some_and(
                                |proof| Some(proof.profile()) == binding.profile && proof.current()
                            ),
                    "native GPU result lost live local eligibility before effect"
                );
            }
        }
        let result = effect();
        drop(state);
        result
    }
}
#[cfg(feature = "local-inference")]
impl StageOwner {
    pub(super) fn device_delivery(
        &self,
        expected: &[airk_core::combined_context::ContextDeliveryBinding],
    ) -> Result<Option<Delivery>> {
        let policy = self.read_configuration()?;
        let NativeConfiguration::Device { ref document, .. } = policy else {
            return Ok(None);
        };
        let binding = document.device_catalog.clone();
        let state = self
            .devices
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native device selection unavailable"))?;
        ensure!(
            state.document.as_ref() == Some(document),
            "device policy has no original native selection custody"
        );
        let generation = self
            .devices
            .generation
            .load(std::sync::atomic::Ordering::Acquire);
        let devices = state.devices.values().cloned().collect();
        let eligibilities = expected_eligibilities(
            &state.eligibilities,
            expected
                .iter()
                .map(|binding| (binding.stage, binding.profile)),
            ResidentEligibility::profile,
        );
        drop(state);
        let delivery = Delivery {
            binding,
            policy,
            shared: self.devices.clone(),
            generation,
            devices,
            eligibilities,
        };
        delivery.live()?;
        Ok(Some(delivery))
    }
}

#[derive(Debug)]
pub(super) struct CurrentDeviceRequired;
impl std::fmt::Display for CurrentDeviceRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("configuration4 requires typed device operations/status3")
    }
}
impl std::error::Error for CurrentDeviceRequired {}
impl StageOwner {
    pub(super) fn require_legacy_operation(&self) -> Result<()> {
        #[cfg(feature = "local-inference")]
        ensure!(
            self.configuration
                .device(binding()?)
                .inspect_device_binding()?
                .is_none(),
            CurrentDeviceRequired
        );
        Ok(())
    }
}

#[cfg(any(feature = "local-inference", test))]
fn gpu_precision_allows(
    preference: GpuPrecisionPreference,
    precision: Option<DevicePrecision>,
) -> bool {
    matches!(
        (preference, precision),
        (_, None)
            | (GpuPrecisionPreference::Auto, _)
            | (GpuPrecisionPreference::Fp32, Some(DevicePrecision::Float32))
            | (GpuPrecisionPreference::Fp16, Some(DevicePrecision::Float16))
    )
}
#[cfg(any(feature = "local-inference", test))]
fn precision_rank(precision: Option<DevicePrecision>) -> u8 {
    match precision {
        Some(DevicePrecision::Float32) => 0,
        Some(DevicePrecision::Float16) => 1,
        None => 2,
    }
}
#[cfg(any(feature = "local-inference", test))]
fn cpu_model_allowed(model: ModelChoice, local_candidate: bool) -> bool {
    !(model == ModelChoice::ContextQwenLarge && local_candidate)
}
#[cfg(test)]
mod tests;

#[cfg(feature = "local-inference")]
impl StageOwner {
    fn require_initialization_idle(&self) -> Result<()> {
        let state = self
            .lifecycle
            .lock()
            .map_err(|_| anyhow::anyhow!("native lifecycle observation unavailable"))?;
        initialization_idle_state(&state, self.stage, &self.devices)
    }
}
#[cfg(feature = "local-inference")]
fn initialization_idle_state(
    state: &super::lifecycle::State,
    stage: Stage,
    devices: &Shared,
) -> Result<()> {
    use std::{
        future::Future,
        task::{Context as TaskContext, Poll, Waker},
    };
    ensure!(
        !state.stopping
            && !state.busy
            && state.device_tasks == 0
            && state.phase != Availability::Draining,
        "actual lifecycle/native work prevents initialization"
    );
    // Release selection before querying the native queue; no selection -> queue lock inversion.
    ensure!(
        !devices
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native device continuation unavailable"))?
            .pending
            .contains_key(&stage),
        "pending device intent prevents initialization"
    );
    if let Some(handle) = &state.handle {
        let observed = handle.observation_for(stage)?;
        ensure!(
            observed.active_profile.is_none()
                && observed.warming_profile.is_none()
                && observed.running == 0
                && observed.queued == 0,
            "actual resident/pending work prevents initialization"
        );
    }
    for (_, charge) in &state.charges {
        let mut wait = Box::pin(charge.wait());
        ensure!(
            matches!(
                wait.as_mut()
                    .poll(&mut TaskContext::from_waker(Waker::noop())),
                Poll::Ready(Ok(()))
            ),
            "actual unreleased host charge prevents initialization"
        );
    }
    Ok(())
}
#[cfg(feature = "local-inference")]
fn with_initialization_fence<T>(
    focus: &Mutex<super::lifecycle::State>,
    context: &Mutex<super::lifecycle::State>,
    devices: &Shared,
    live: impl Fn() -> Result<()>,
    effect: impl FnOnce(&dyn Fn() -> Result<()>) -> Result<T>,
) -> Result<T> {
    // Same stage admission mutexes as launch/configure/cancel, held through the
    // Store mutation. Fixed order Focus -> Context -> Store -> selection/native.
    let focus = focus
        .lock()
        .map_err(|_| anyhow::anyhow!("focus initialization fence unavailable"))?;
    let context = context
        .lock()
        .map_err(|_| anyhow::anyhow!("context initialization fence unavailable"))?;
    let check = || {
        live()?;
        initialization_idle_state(&focus, Stage::Focus, devices)?;
        initialization_idle_state(&context, Stage::Context, devices)
    };
    check()?;
    let value = effect(&check)?;
    check()?;
    Ok(value)
}

#[cfg(feature = "local-inference")]
fn ready_from_observation(
    state: &DeviceState,
    document: &DeviceConfiguration,
    stage: Stage,
    observed: Option<&airk_inference::inference_engine::pool::arbiter::Observation>,
    charged_resident: bool,
) -> bool {
    state.document.as_ref() == Some(document)
        && state.ready.contains(&stage)
        && document.stage(stage).enabled
        && charged_resident
        && state
            .devices
            .get(&stage)
            .is_none_or(|(device, epoch)| !device.poisoned() && device.epoch() == *epoch)
        && observed.is_some_and(|actual| {
            actual.configuration_revision == Some(document.revision)
                && actual.active_profile.is_some()
                && actual.active_profile
                    == state.selected.get(&stage).map(|profile| profile.manifest)
        })
}
#[cfg(feature = "local-inference")]
fn cancel_device_stage(
    lifecycle: &mut super::lifecycle::State,
    shared: &Shared,
    stage: Stage,
) -> Result<()> {
    // Existing order: stage lifecycle -> device continuation/selection.
    let mut state = shared
        .state
        .lock()
        .map_err(|_| anyhow::anyhow!("device cancellation fence unavailable"))?;
    let DeviceState {
        pending,
        intent_generation,
        ..
    } = &mut *state;
    cancel_continuations(pending, intent_generation, stage)?;
    state.ready.remove(&stage);
    if let Some(proof) = state.eligibilities.remove(&stage) {
        proof.revoke();
    }
    super::lifecycle::cancel_current(lifecycle);
    lifecycle.cancel.cancel();
    Ok(())
}

#[cfg(feature = "local-inference")]
fn continuation_current(
    generations: &BTreeMap<Stage, u64>,
    stage: Stage,
    original: u64,
) -> Result<()> {
    ensure!(
        generations.get(&stage).copied().unwrap_or(0) == original,
        "device continuation was explicitly canceled"
    );
    Ok(())
}
#[cfg(feature = "local-inference")]
fn cancel_continuations<T>(
    pending: &mut BTreeMap<Stage, T>,
    generations: &mut BTreeMap<Stage, u64>,
    stage: Stage,
) -> Result<()> {
    let next = generations
        .get(&stage)
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .context("device continuation generation exhausted")?;
    generations.insert(stage, next);
    pending.remove(&stage);
    Ok(())
}

#[cfg(feature = "local-inference")]
fn prework_device_failure(error: &anyhow::Error) -> bool {
    if runtime::is_native_device_failure(error) {
        return true;
    }
    #[cfg(feature = "gpu-cuda")]
    {
        runtime::cuda::is_device_eligibility_failure(error)
    }
    #[cfg(not(feature = "gpu-cuda"))]
    {
        false
    }
}

#[cfg(feature = "local-inference")]
fn prework_device_reason(error: &anyhow::Error) -> SelectionReason {
    #[cfg(feature = "gpu-cuda")]
    {
        if runtime::cuda::is_device_capacity_failure(error) {
            return SelectionReason::DeviceMemoryInsufficient;
        }
        if !runtime::is_native_device_failure(error)
            && runtime::cuda::is_device_eligibility_failure(error)
        {
            return SelectionReason::Unqualified;
        }
    }
    let _ = error;
    SelectionReason::ProviderInitializationFailed
}

#[cfg(feature = "local-inference")]
fn expected_eligibilities<T: Clone>(
    all: &BTreeMap<Stage, T>,
    expected: impl IntoIterator<Item = (Stage, Option<Sha256Digest>)>,
    profile: impl Fn(&T) -> Sha256Digest,
) -> BTreeMap<Stage, T> {
    let bindings = expected
        .into_iter()
        .filter_map(|(stage, id)| id.map(|id| (stage, id)))
        .collect::<std::collections::BTreeSet<_>>();
    all.iter()
        .filter(|(stage, proof)| bindings.contains(&(**stage, profile(proof))))
        .map(|(stage, proof)| (*stage, proof.clone()))
        .collect()
}

#[cfg(feature = "local-inference")]
fn recordable_fault_profile(
    genuine: bool,
    unknown: bool,
    failed: Option<Sha256Digest>,
    current: Sha256Digest,
) -> Option<Sha256Digest> {
    (genuine && !unknown).then_some(failed.unwrap_or(current))
}

#[cfg(feature = "local-inference")]
fn retained_gpu_targets(
    active: impl IntoIterator<Item = (Stage, Option<Sha256Digest>)>,
    selected: impl IntoIterator<Item = (Stage, Sha256Digest, bool)>,
) -> Vec<(Stage, Sha256Digest)> {
    let selected = selected
        .into_iter()
        .filter(|(_, _, gpu)| *gpu)
        .map(|(stage, profile, _)| (stage, profile))
        .collect::<BTreeMap<_, _>>();
    active
        .into_iter()
        .filter_map(|(stage, profile)| {
            profile
                .filter(|profile| selected.get(&stage) == Some(profile))
                .map(|profile| (stage, profile))
        })
        .collect()
}

#[cfg(feature = "local-inference")]
fn secondary_fault_error(error: anyhow::Error, secondary: anyhow::Error) -> anyhow::Error {
    error.context(format!(
        "secondary native fault recorder refused: {secondary:#}"
    ))
}
