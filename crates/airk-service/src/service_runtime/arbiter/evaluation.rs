// SPDX-License-Identifier: Apache-2.0
//! Typed private evaluation seam. No wire value attaches this catalog.
use super::*;
use airk_core::identity::Sha256Digest;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};

#[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
impl Owner {
    pub(super) fn apply_evaluation_gpu_policy(&self, allowed: bool) -> Result<()> {
        if allowed {
            return Ok(());
        }
        // Every stage that holds private CUDA entries. Each stage is released
        // even when another stage's release fails, and the errors are joined.
        release_every_stage([
            Box::new(|| release_private_gpu(&self.context, Stage::Context))
                as Box<dyn FnOnce() -> Result<()> + '_>,
            Box::new(|| release_private_gpu(&self.focus, Stage::Focus)),
        ])
    }
}

/// Runs every stage's release, then joins any failures into one error, so one
/// stage's failure never leaves another stage's private entries resident.
fn release_every_stage<'a>(
    releases: impl IntoIterator<Item = Box<dyn FnOnce() -> Result<()> + 'a>>,
) -> Result<()> {
    let failures: Vec<String> = releases
        .into_iter()
        .filter_map(|release| release().err().map(|error| format!("{error:#}")))
        .collect();
    ensure!(
        failures.is_empty(),
        "private GPU release incomplete: {}",
        failures.join("; ")
    );
    Ok(())
}

/// Releases and drains every private CUDA entry one stage's native owner
/// holds, after GPU use was revoked.
#[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
fn release_private_gpu(owner: &StageOwner, stage: Stage) -> Result<()> {
    let lifecycle = owner.lifecycle.clone();
    let mut state = lifecycle
        .lock()
        .map_err(|_| anyhow::anyhow!("private GPU lifecycle unavailable"))?;
    if state.private_gpu.is_empty() {
        return Ok(());
    }
    // The selected private entry, and every private entry the native owner
    // holds active or warming. Each is released and drained.
    let selected = state
        .profile
        .filter(|manifest| state.private_gpu.contains_key(manifest));
    let in_progress = selected.is_some() && (state.busy || state.phase == Availability::Ready);
    let native = state
        .handle
        .as_ref()
        .map(|handle| handle.observation_for(stage))
        .transpose()?;
    let targets = super::lifecycle::private_release_targets(
        state.private_gpu.keys().copied(),
        native.as_ref().and_then(|snapshot| snapshot.active_profile),
        native
            .as_ref()
            .and_then(|snapshot| snapshot.warming_profile),
        selected.filter(|_| in_progress),
    );
    if targets.is_empty() {
        return Ok(());
    }
    let selected_target = selected.filter(|manifest| targets.contains(manifest));
    if selected_target.is_some() {
        super::lifecycle::cancel_current(&mut state);
        state.cancel.cancel();
        state.phase = Availability::Draining;
        state.reason = "GPU use disabled; waiting for the private CUDA operation and memory reservation to settle.".into();
    }
    if let Some(handle) = &state.handle {
        for manifest in &targets {
            handle.release_profile(*manifest)?;
        }
    }
    state.device_tasks = state
        .device_tasks
        .checked_add(1)
        .context("private GPU lifecycle count exhausted")?;
    let serial = state.serial;
    let settled = state.settled.clone();
    drop(state);
    tokio::spawn(async move {
        let deadline = Instant::now() + Duration::from_secs(30);
        let result = async {
            let (handle, charges) = loop {
                let changed = settled.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                {
                    let state = lifecycle
                        .lock()
                        .map_err(|_| anyhow::anyhow!("private GPU lifecycle unavailable"))?;
                    if selected_target.is_none() || state.serial != serial || !state.busy {
                        break (
                            state.handle.clone(),
                            state
                                .charges
                                .iter()
                                .filter(|(profile, _)| targets.contains(profile))
                                .map(|(_, charge)| charge.clone())
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), changed)
                    .await
                    .context("private GPU activation did not settle")?;
            };
            if let Some(handle) = handle {
                for manifest in &targets {
                    handle.wait_profile_released(*manifest, deadline).await?;
                }
            }
            for charge in charges {
                tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), charge.wait())
                    .await
                    .context("private GPU host memory did not settle")??;
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let mut state = lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.device_tasks -= 1;
        if state.serial == serial
            && state
                .profile
                .is_some_and(|manifest| state.private_gpu.contains_key(&manifest))
        {
            match result {
                Ok(()) => {
                    // The isolated fixture authenticates exactly one variant per model.
                    state.phase = Availability::Constrained;
                    state.reason = "GPU use is off; the private CUDA profile was released. Select this stage's CPU profile to use the model again.".into();
                }
                Err(error) => {
                    state.phase = Availability::Draining;
                    state.reason =
                        bounded_reason(format!("Private CUDA drain is incomplete: {error:#}"));
                }
            }
        }
        state.settled.notify_waiters();
    });
    Ok(())
}

#[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
pub(super) fn private_gpu_grant(
    devices: &std::sync::Weak<super::device_management::Shared>,
) -> Result<Arc<dyn Fn() -> bool + Send + Sync>> {
    use std::sync::atomic::Ordering;
    let owner = devices
        .upgrade()
        .context("private CUDA Answer service stopped")?;
    let generation = owner.generation.load(Ordering::Acquire);
    ensure!(owner.gpu_allowed(), "service GPU use is disabled");
    let devices = devices.clone();
    Ok(Arc::new(move || {
        devices.upgrade().is_some_and(|current| {
            current.gpu_allowed() && current.generation.load(Ordering::Acquire) == generation
        })
    }))
}

#[cfg(test)]
type FixtureDescriptorSource<'a> = &'a mut (dyn FnMut(&Asset) -> Result<File> + 'a);

/// Native test-support observation only. The sink is attached before exact
/// same-lease/same-gate fixture composition; no wire value attaches it.
pub type ContextObservationSink = Arc<
    dyn Fn(&airk_core::native_context_action::NativeContextActionRequest, &Profile) -> Result<()>
        + Send
        + Sync,
>;

#[derive(serde::Serialize)]
pub struct EvaluationResult<T> {
    evaluation: bool,
    release_qualified: bool,
    result: T,
}
impl<T> EvaluationResult<T> {
    fn new(result: T) -> Self {
        Self {
            evaluation: true,
            release_qualified: false,
            result,
        }
    }
    pub fn result(&self) -> &T {
        &self.result
    }
    pub fn into_result(self) -> T {
        self.result
    }
}
/// Only a native fixture with the real private service lease constructs this.
/// The caller must use an isolated control root, never an ordinary installation.
pub struct EvaluationOwner {
    owner: Owner,
}

#[cfg(all(
    windows,
    target_arch = "x86_64",
    feature = "test-support",
    feature = "gpu-cuda"
))]
pub(super) struct CudaAnswerEvaluation {
    pub(super) devices: std::sync::Weak<super::device_management::Shared>,
    pub(super) lease: Arc<airk_store::service::ServiceLease>,
    pub(super) gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    pub(super) manifest: Sha256Digest,
    pub(super) adapter: [u8; 16],
    pub(super) assets: CudaAnswerAssets,
    pub(super) route: PrivateGpuRoute,
}

/// How a private CUDA entry opens. A Context entry opens its reviewed Answer
/// graph; a Focus entry opens its strict-GPU profile under the probe binding,
/// whose placement the scorer verifies against the live session.
#[cfg(all(
    windows,
    target_arch = "x86_64",
    feature = "test-support",
    feature = "gpu-cuda"
))]
pub enum PrivateGpuRoute {
    Answer(airk_inference::inference_engine::pool::arbiter::EvaluationAnswerGraph),
    Focus { expires: Instant },
}

#[cfg(all(
    windows,
    target_arch = "x86_64",
    feature = "test-support",
    feature = "gpu-cuda"
))]
impl PrivateGpuRoute {
    pub(super) fn expires(&self) -> Instant {
        match self {
            Self::Answer(graph) => graph.expires(),
            Self::Focus { expires } => *expires,
        }
    }
    pub(super) fn stage(&self) -> Stage {
        match self {
            Self::Answer(_) => Stage::Context,
            Self::Focus { .. } => Stage::Focus,
        }
    }
}

/// The device reservation a strict-GPU Focus profile probes under: its
/// declared dedicated peak, with the warm extra bounded by the same peak.
#[cfg(all(
    windows,
    target_arch = "x86_64",
    feature = "test-support",
    feature = "gpu-cuda"
))]
pub(super) fn focus_capacity_plan(
    profile: &Profile,
) -> Result<airk_retrieval::arbiter::runtime::cuda::DeviceCapacityPlan> {
    let device = profile
        .device_execution
        .as_ref()
        .context("private CUDA Focus profile has no device execution")?;
    ensure!(
        profile.model == ModelChoice::FocusQwenReranker06B
            && matches!(
                device.placement,
                airk_core::arbiter::device::PlacementPolicy::StrictGpu
            ),
        "private CUDA Focus requires a strict-GPU Focus profile"
    );
    match device.memory {
        airk_core::arbiter::device::DeviceMemory::Dedicated {
            required_peak_bytes,
            headroom_bytes,
        } => airk_retrieval::arbiter::runtime::cuda::DeviceCapacityPlan::probe(
            required_peak_bytes,
            required_peak_bytes,
            headroom_bytes,
        ),
        _ => anyhow::bail!("private CUDA Focus profile declares no dedicated device memory"),
    }
}

/// One private CUDA Context entry for [`EvaluationOwner::with_private_gpu`].
#[cfg(all(
    windows,
    target_arch = "x86_64",
    feature = "test-support",
    feature = "gpu-cuda"
))]
pub struct PrivateGpuEntry<'a> {
    pub manifest: Sha256Digest,
    pub adapter: [u8; 16],
    pub route: PrivateGpuRoute,
    pub source: &'a mut (dyn FnMut(&Asset) -> Result<File> + 'a),
}

#[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
#[derive(Clone)]
pub(super) struct CudaAnswerAssets(Arc<Mutex<std::collections::BTreeMap<String, (Asset, File)>>>);

#[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
impl CudaAnswerAssets {
    fn retain(
        profile: &Profile,
        mut source: impl FnMut(&Asset) -> Result<File>,
        deadline: Instant,
    ) -> Result<Self> {
        use sha2::{Digest, Sha256};

        let mut files = std::collections::BTreeMap::new();
        let mut buffer = [0_u8; 65536];
        for asset in &profile.assets {
            let mut file = source(asset)?;
            ensure!(
                file.metadata()?.len() == asset.bytes,
                "private CUDA asset length differs"
            );
            file.seek(SeekFrom::Start(0))?;
            let mut digest = Sha256::new();
            let mut remaining = asset.bytes;
            while remaining > 0 {
                ensure!(
                    Instant::now() < deadline,
                    "original private CUDA asset deadline"
                );
                let limit = usize::try_from(remaining.min(buffer.len() as u64))?;
                let read = file.read(&mut buffer[..limit])?;
                ensure!(read > 0, "private CUDA asset shortened");
                digest.update(&buffer[..read]);
                remaining -= u64::try_from(read)?;
            }
            ensure!(
                file.read(&mut [0])? == 0
                    && Sha256Digest::from_bytes(digest.finalize().into()) == asset.sha256,
                "private CUDA asset bytes changed"
            );
            files.insert(asset.name.clone(), (asset.clone(), file));
        }
        Ok(Self(Arc::new(Mutex::new(files))))
    }
}

#[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
impl airk_retrieval::arbiter::runtime::AssetSource for CudaAnswerAssets {
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>> {
        let mut files = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("private CUDA asset descriptors unavailable"))?;
        let (declared, file) = files
            .get_mut(&asset.name)
            .context("private CUDA asset missing")?;
        ensure!(declared == asset, "private CUDA asset declaration changed");
        let length = usize::try_from(asset.bytes)?;
        let limit = asset
            .bytes
            .checked_add(1)
            .context("private CUDA asset size overflow")?;
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length)?;
        file.take(limit).read_to_end(&mut bytes)?;
        ensure!(bytes.len() == length, "private CUDA asset length changed");
        // Scorer independently checks the exact SHA and graph it consumes.
        Ok(bytes)
    }
}
impl EvaluationOwner {
    pub(in crate::service_runtime) fn into_owner_for(
        self,
        lease: &Arc<airk_store::service::ServiceLease>,
        gate: &airk_catalog_control::catalog_admission::NativeCatalogAdmission,
    ) -> Result<Owner> {
        ensure!(
            Arc::ptr_eq(&self.owner.ownership, lease) && self.owner.gate.same_instance(gate),
            "Private evaluation owner differs from the running service lease or admission gate"
        );
        Ok(self.owner)
    }
    pub fn new(
        ownership: Arc<airk_store::service::ServiceLease>,
        gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
        profiles: Vec<Profile>,
    ) -> Result<Self> {
        Self::new_bound(ownership, gate, profiles, None)
    }
    #[cfg(test)]
    pub(in crate::service_runtime) fn reopen_same_configuration(
        ownership: Arc<airk_store::service::ServiceLease>,
        gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
        profiles: Vec<Profile>,
        expected: &Configuration,
    ) -> Result<Self> {
        Self::verify_restart_settled(&ownership)?;
        Self::new_bound(ownership, gate, profiles, Some(expected))
    }
    #[cfg(test)]
    pub(in crate::service_runtime) fn verify_restart_settled(
        lease: &airk_store::service::ServiceLease,
    ) -> Result<()> {
        let mut registry =
            airk_store::service::registry::ServiceRegistry::open_saved(lease.root())?;
        ensure!(
            registry.pending_projects(1)?.is_empty(),
            "Restart has native pending/running work"
        );
        let mut after = None;
        let mut revision = None;
        loop {
            let page = registry.list(after, revision, 32)?;
            revision = Some(page.revision);
            for project in page.projects {
                let binding = project.registration.binding;
                ensure!(
                    registry.pending_job_observation(binding)?.is_none()
                        && !registry.has_unsettled_configuration(binding)?,
                    "Restart has paused queued work or unfinished configuration"
                );
            }
            after = page.after;
            if after.is_none() {
                break;
            }
        }
        drop(registry);
        Ok(())
    }
    fn new_bound(
        ownership: Arc<airk_store::service::ServiceLease>,
        gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
        profiles: Vec<Profile>,
        expected: Option<&Configuration>,
    ) -> Result<Self> {
        let catalog =
            airk_kernel::inference_owner::evaluation_catalog(ownership.clone(), profiles)?;
        let mut owner = Owner::new(ownership, gate)?;
        let configuration = owner.configuration()?;
        ensure!(
            expected.is_some_and(|saved| saved == &configuration)
                || (expected.is_none()
                    && ![&configuration.focus, &configuration.context]
                        .into_iter()
                        .any(|stage| stage.enabled
                            || !stage.indexes.is_empty()
                            || stage.selected_profile.is_some())),
            "Private evaluation requires an unconfigured owner or exact saved restart configuration"
        );
        owner.focus.evaluation = Some(catalog.clone());
        owner.context.evaluation = Some(catalog);
        Ok(Self { owner })
    }
    #[cfg(all(
        windows,
        target_arch = "x86_64",
        feature = "test-support",
        feature = "gpu-cuda"
    ))]
    pub fn with_cuda_answer(
        self,
        manifest: Sha256Digest,
        adapter: [u8; 16],
        graph: airk_inference::inference_engine::pool::arbiter::EvaluationAnswerGraph,
        mut source: impl FnMut(&Asset) -> Result<File>,
    ) -> Result<Self> {
        self.with_private_gpu(vec![PrivateGpuEntry {
            manifest,
            adapter,
            route: PrivateGpuRoute::Answer(graph),
            source: &mut source,
        }])
    }

    /// Attaches every private CUDA entry of this lifetime at once, keyed by
    /// manifest: at most one Compact and one Large Context entry, and at most
    /// one Focus entry, all on the same lease, gate and adapter. Each joins its
    /// own stage. Only one entry per stage is ever resident: selecting another
    /// releases the resident one first (plan 12.5.2 item 4). A second attach is
    /// refused.
    #[cfg(all(
        windows,
        target_arch = "x86_64",
        feature = "test-support",
        feature = "gpu-cuda"
    ))]
    pub fn with_private_gpu(self, entries: Vec<PrivateGpuEntry<'_>>) -> Result<Self> {
        ensure!(
            (1..=3).contains(&entries.len()),
            "private CUDA attaches one to three entries"
        );
        let mut focus = std::collections::BTreeMap::new();
        let mut context = std::collections::BTreeMap::new();
        let mut models = std::collections::BTreeSet::new();
        let adapter = entries[0].adapter;
        for entry in entries {
            let stage = entry.route.stage();
            let owner = match stage {
                Stage::Focus => &self.owner.focus,
                Stage::Context => &self.owner.context,
            };
            let profile = owner.execution_profile(entry.manifest)?;
            let model = profile.description().model;
            let model_fits = match &entry.route {
                PrivateGpuRoute::Answer(_) => matches!(
                    model,
                    ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge
                ),
                PrivateGpuRoute::Focus { .. } => focus_capacity_plan(profile.description()).is_ok(),
            };
            ensure!(
                profile.evaluation()
                    && !profile.release_qualified()
                    && model_fits
                    && profile.description().device_execution.is_some()
                    && entry.adapter != [0; 16]
                    && entry.adapter == adapter
                    && Instant::now() < entry.route.expires(),
                "private CUDA requires an exact Compact or Large Answer graph, or a strict-GPU Focus profile, on one adapter"
            );
            ensure!(
                models.insert(model)
                    && !focus.contains_key(&entry.manifest)
                    && !context.contains_key(&entry.manifest),
                "private CUDA holds at most one entry per model"
            );
            let assets = CudaAnswerAssets::retain(
                profile.description(),
                &mut *entry.source,
                entry.route.expires(),
            )?;
            let prepared = Arc::new(CudaAnswerEvaluation {
                devices: Arc::downgrade(&owner.devices),
                lease: self.owner.ownership.clone(),
                gate: self.owner.gate.clone(),
                manifest: entry.manifest,
                adapter: entry.adapter,
                assets,
                route: entry.route,
            });
            match stage {
                Stage::Focus => focus.insert(entry.manifest, prepared),
                Stage::Context => context.insert(entry.manifest, prepared),
            };
        }
        for (owner, prepared) in [(&self.owner.focus, focus), (&self.owner.context, context)] {
            let mut state = owner
                .lifecycle
                .lock()
                .map_err(|_| anyhow::anyhow!("private CUDA lifecycle unavailable"))?;
            ensure!(
                state.private_gpu.is_empty() && !state.busy && !state.stopping,
                "private CUDA must be attached to an idle, isolated evaluation owner"
            );
            state.private_gpu = prepared;
        }
        Ok(self)
    }

    /// Observe an already admitted request without dispatching Context scoring.
    /// Source/configuration/deadline postchecks still decide the canonical reply.
    /// Separate compiler census/recording work is not a deployed-model timing.
    pub fn observe_context(mut self, sink: ContextObservationSink) -> Self {
        self.owner.context.context_observer = Some(sink);
        self
    }
    /// Observation only; actual service composition still checks the SAME native lease/gate.
    pub fn observe_execution(
        mut self,
        observer: airk_inference::inference_engine::pool::arbiter::execution_observer::Observer,
    ) -> Self {
        self.owner.focus.execution_observer = Some(observer.clone());
        self.owner.context.execution_observer = Some(observer);
        self
    }
    #[cfg(test)]
    pub(in crate::service_runtime) fn observe_warm(
        self,
        observer: super::warm_observer::Observer,
    ) -> Result<Self> {
        super::lifecycle::observe_warm(&self.owner.focus.lifecycle, observer.clone())?;
        super::lifecycle::observe_warm(&self.owner.context.lifecycle, observer)?;
        Ok(self)
    }
    /// Sets the idle unload time of each stage for this private lifetime
    /// (plan 12.5; default 30 minutes, `None` or zero never unloads).
    pub fn with_idle_unload(
        self,
        focus: Option<Duration>,
        context: Option<Duration>,
    ) -> Result<Self> {
        self.owner.focus.set_idle_unload(focus)?;
        self.owner.context.set_idle_unload(context)?;
        Ok(self)
    }
    pub fn configuration(&self) -> Result<EvaluationResult<Configuration>> {
        Ok(EvaluationResult::new(self.owner.configuration()?))
    }
    /// The fixture supplies retained read descriptors for its approved model
    /// assets. No path/URL supplied by a client is opened, and source files are
    /// never written. The ordinary Store transaction verifies exact actual bytes.
    pub fn seed(
        &self,
        manifest: Sha256Digest,
        source: &mut dyn FnMut(&Asset) -> Result<File>,
    ) -> Result<EvaluationResult<()>> {
        let execution = self.owner.context.execution_profile(manifest)?;
        ensure!(
            execution.evaluation() && !execution.release_qualified(),
            "Private seed requires an opaque evaluation capability"
        );
        let profile = execution.description();
        let configuration = self.owner.configuration()?;
        let transaction = self.owner.assets.begin(profile, configuration.revision)?;
        let mut buffer = [0_u8; 65536];
        for asset in &profile.assets {
            if let Some(mut writer) = transaction.asset(asset)? {
                let mut input = source(asset)?;
                ensure!(
                    input.metadata()?.is_file() && input.metadata()?.len() == asset.bytes,
                    "Private evaluation asset descriptor differs"
                );
                input.seek(SeekFrom::Start(0))?;
                let mut remaining = asset.bytes;
                while remaining > 0 {
                    let size = usize::try_from(remaining.min(buffer.len() as u64))?;
                    let bytes = input.read(&mut buffer[..size])?;
                    ensure!(bytes > 0, "Private evaluation source shortened");
                    writer.write_all(&buffer[..bytes])?;
                    remaining -= u64::try_from(bytes)?;
                }
                let mut extra = [0];
                ensure!(
                    input.read(&mut extra)? == 0 && input.metadata()?.len() == asset.bytes,
                    "Private evaluation source changed"
                );
                transaction.publish(writer)?;
            }
        }
        transaction.finish()?;
        Ok(EvaluationResult::new(()))
    }
    /// Native ignored regression only. The opaque owner supplies the SAME
    /// private execution capability, lease and stage state as the live service.
    #[cfg(test)]
    pub(in crate::service_runtime) fn actual_acquisition_fixture(
        &self,
        manifest: Sha256Digest,
        source: Option<FixtureDescriptorSource<'_>>,
    ) -> Result<super::lifecycle::acquisition_fixture::AcquisitionFixture> {
        let execution = self.owner.context.execution_profile(manifest)?;
        ensure!(
            execution.evaluation() && !execution.release_qualified(),
            "actual acquisition regression requires an opaque private evaluation capability"
        );
        let profile = execution.description().clone();
        let sources = if let Some(source) = source {
            let mut files = std::collections::BTreeMap::new();
            for asset in &profile.assets {
                files.insert(asset.name.clone(), source(asset)?);
            }
            Some(files)
        } else {
            None
        };
        super::lifecycle::acquisition_fixture::AcquisitionFixture::capture(
            &self.owner.context,
            profile,
            sources,
        )
    }
    pub async fn manage(
        &self,
        request: ManagementRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<EvaluationResult<Status>> {
        let mut status = self.owner.manage(request, supervisor, None).await?;
        for stage in [&mut status.focus, &mut status.context] {
            stage.reason = bounded_reason(format!(
                "Private evaluation only; candidate is not release-qualified. {}",
                stage.reason
            ));
        }
        Ok(EvaluationResult::new(status))
    }
    pub fn application(
        &self,
        worker: WorkerIdentity,
        authority: ScopedReadRef,
        publication: airk_core::evidence::RevisionEnvelope,
    ) -> Result<EvaluationResult<IndexApplication>> {
        Ok(EvaluationResult::new(self.owner.application(
            worker,
            authority,
            publication,
        )?))
    }
    pub async fn next_read(
        &self,
        request: NativeNextReadRequest,
        supervisor: &airk_supervisor::project_supervisor::Supervisor,
    ) -> Result<EvaluationResult<Option<Advice>>> {
        Ok(EvaluationResult::new(
            self.owner.next_read(request, supervisor).await?,
        ))
    }
}

#[cfg(test)]
mod execution_observer_tests {
    use super::*;

    #[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
    #[test]
    fn a_failed_focus_release_still_releases_context_and_reports_both() {
        let context_released = std::cell::Cell::new(false);
        let result = release_every_stage([
            Box::new(|| -> Result<()> {
                context_released.set(true);
                Ok(())
            }) as Box<dyn FnOnce() -> Result<()> + '_>,
            Box::new(|| anyhow::bail!("focus lifecycle unavailable")),
        ]);
        assert!(context_released.get());
        assert!(format!("{:#}", result.unwrap_err()).contains("focus lifecycle unavailable"));
        // Order does not matter: a failure first still runs the rest.
        let focus_released = std::cell::Cell::new(false);
        let result = release_every_stage([
            Box::new(|| anyhow::bail!("context drain")) as Box<dyn FnOnce() -> Result<()> + '_>,
            Box::new(|| -> Result<()> {
                focus_released.set(true);
                Ok(())
            }),
        ]);
        assert!(focus_released.get() && result.is_err());
        assert!(release_every_stage(Vec::new()).is_ok());
    }

    #[cfg(all(
        windows,
        target_arch = "x86_64",
        feature = "test-support",
        feature = "gpu-cuda"
    ))]
    #[test]
    fn private_routes_name_their_stage() {
        let expires = Instant::now() + Duration::from_secs(60);
        let focus = PrivateGpuRoute::Focus { expires };
        assert_eq!(focus.stage(), Stage::Focus);
        assert_eq!(focus.expires(), expires);
    }

    #[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
    #[test]
    fn private_gpu_grant_expires_without_retaining_service_or_reauthorizing_old_generation()
    -> Result<()> {
        let owner = Arc::new(super::super::device_management::Shared::default());
        let weak = Arc::downgrade(&owner);
        // The device layer starts closed: no grant exists until the service
        // applies a configuration with GPU use on.
        assert!(private_gpu_grant(&weak).is_err());
        assert!(owner.apply_gpu_allowed(true));
        let original = private_gpu_grant(&weak)?;
        assert!(original());
        assert_eq!(Arc::strong_count(&owner), 1);
        owner
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        assert!(!original());
        let current = private_gpu_grant(&weak)?;
        assert!(current());
        drop(owner);
        assert!(!current());
        assert!(private_gpu_grant(&weak).is_err());
        Ok(())
    }

    #[test]
    fn private_execution_observer_attachment_still_requires_actual_startup_lease_and_gate()
    -> Result<()> {
        use airk_catalog_control::catalog_admission::NativeCatalogAdmission;
        use airk_core::{identity::Incarnation, index_catalog::CatalogRevision};
        let root = tempfile::tempdir()?;
        let other = tempfile::tempdir()?;
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(root.path())?);
        let foreign = Arc::new(airk_store::service::ServiceLease::acquire(other.path())?);
        let gate = NativeCatalogAdmission::new(
            Incarnation::from_bytes([81; 16]),
            Incarnation::from_bytes([82; 16]),
            CatalogRevision::INITIAL,
            1,
        )?;
        for variant in [0, 1, 2] {
            let(observer,_)=airk_inference::inference_engine::pool::arbiter::execution_observer::Observer::channel(16)?;
            // Attachment itself grants no execution catalog; actual score routing separately requires it.
            let owner = EvaluationOwner {
                owner: Owner::new(lease.clone(), gate.clone())?,
            }
            .observe_execution(observer);
            assert!(
                owner.owner.focus.execution_observer.is_some()
                    && owner.owner.context.execution_observer.is_some()
            );
            let supplied = if variant == 0 { &foreign } else { &lease };
            let other_gate = NativeCatalogAdmission::new(
                Incarnation::from_bytes([81; 16]),
                Incarnation::from_bytes([82; 16]),
                CatalogRevision::INITIAL,
                1,
            )?;
            assert_eq!(
                owner
                    .into_owner_for(supplied, if variant == 1 { &other_gate } else { &gate })
                    .is_ok(),
                variant == 2
            );
        }
        Ok(())
    }
}
