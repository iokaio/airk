// SPDX-License-Identifier: Apache-2.0
//! Optional, bounded compute lane inside the actual inference owner.
use super::*;
use airk_core::arbiter::{Configuration, ScopeBinding, Stage, Task};
use airk_retrieval::arbiter::{
    compiler::Candidate,
    runtime::{AssetSource, ExecutionProfile, Scorer},
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Condvar,
};

mod device;
mod eligibility;
pub use eligibility::{ResidentEligibility, failed_profile as eligibility_failure_profile};
#[cfg(feature = "test-support")]
pub mod execution_observer;
pub use device::{
    BoundDeviceCandidate, DeviceFailureNotifier, DeviceFault, DeviceFaultKey, DeviceProbe,
    NativeDevice, circuit_key,
};

pub use airk_kernel::arbiter::device_runtime::NativeConfiguration;
/// Native admission owner retains the memory reservation until actual FFI
/// completion. A disconnected delivery never releases this reservation early.
pub use airk_kernel::service_resources::OptionalReservation as Reservation;
pub type Authority = Arc<dyn Fn(&ScopeBinding) -> bool + Send + Sync>;
pub struct ScoreRequest {
    pub worker: Incarnation,
    pub scope: ScopeBinding,
    pub task: Task,
    pub deadline: Instant,
    pub authority: Authority,
    pub state: String,
    pub question: String,
    pub candidates: Vec<Candidate>,
    pub resident: ResidentPermit,
    pub admission: airk_kernel::service_resources::OptionalReuse,
    pub max_input_tokens: usize,
}
/// Opaque proof of the exact charged resident session, never a second charge.
pub struct ResidentPermit {
    profile: Sha256Digest,
    reservation: Arc<Reservation>,
}
impl ResidentPermit {
    pub fn reservation(&self) -> Arc<Reservation> {
        self.reservation.clone()
    }
}
pub struct Scored {
    pub scores: Vec<f32>,
    pub input_digest: Sha256Digest,
    pub input_tokens: usize,
    pub queue_us: u64,
    pub compile_us: u64,
    pub inference_us: u64,
}
#[derive(Clone, Debug)]
pub struct CompiledReceipt {
    pub input_digest: Sha256Digest,
    pub input_tokens: usize,
    pub queue_us: u64,
    pub compile_us: u64,
}
/// Fixed-size, private native measurements after an actual score call returns.
/// This is not a wire response or evidence/source authority.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "test-support", derive(serde::Serialize, serde::Deserialize))]
pub struct ScoreReceipt {
    pub input_digest: Sha256Digest,
    pub input_tokens: usize,
    pub queue_us: u64,
    pub compile_us: u64,
    pub inference_us: u64,
}
/// A failed completed score retains known work. Absence of this type never
/// implies zero work: a canceled/undelivered result can remain unknown.
#[derive(Debug)]
pub struct KnownScoreFailure {
    pub receipt: ScoreReceipt,
    pub source_delivery_refused: bool,
    cause: anyhow::Error,
}
impl std::fmt::Display for KnownScoreFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Optional native score failed after observed completion")
    }
}
impl std::error::Error for KnownScoreFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

fn observed_score_call(
    compiled: &CompiledReceipt,
    started: Instant,
    now: impl FnOnce() -> Instant,
    score: impl FnOnce() -> Result<Vec<f32>>,
) -> Result<Scored> {
    let result = score();
    let receipt = ScoreReceipt {
        input_digest: compiled.input_digest,
        input_tokens: compiled.input_tokens,
        queue_us: compiled.queue_us,
        compile_us: compiled.compile_us,
        inference_us: now()
            .saturating_duration_since(started)
            .as_micros()
            .min(u64::MAX as u128) as u64,
    };
    match result {
        Ok(scores) => Ok(Scored {
            scores,
            input_digest: receipt.input_digest,
            input_tokens: receipt.input_tokens,
            queue_us: receipt.queue_us,
            compile_us: receipt.compile_us,
            inference_us: receipt.inference_us,
        }),
        Err(cause) => Err(KnownScoreFailure {
            receipt,
            source_delivery_refused: false,
            cause,
        }
        .into()),
    }
}

fn finish_score_delivery(result: Result<Scored>, admission: Result<()>) -> Result<Scored> {
    let Err(refused) = admission else {
        return result;
    };
    let (receipt, cause) = match result {
        Ok(scored) => (
            Some(ScoreReceipt {
                input_digest: scored.input_digest,
                input_tokens: scored.input_tokens,
                queue_us: scored.queue_us,
                compile_us: scored.compile_us,
                inference_us: scored.inference_us,
            }),
            refused,
        ),
        Err(error) => (
            error
                .downcast_ref::<KnownScoreFailure>()
                .map(|failure| failure.receipt.clone()),
            error.context(refused),
        ),
    };
    match receipt {
        Some(receipt) => Err(KnownScoreFailure {
            receipt,
            source_delivery_refused: true,
            cause,
        }
        .into()),
        None => Err(cause),
    }
}

pub struct PendingScore {
    pub compiled: oneshot::Receiver<CompiledReceipt>,
    pub completed: oneshot::Receiver<Result<Scored>>,
}
pub struct Observation {
    pub configuration_revision: Option<u64>,
    pub active_profile: Option<Sha256Digest>,
    pub warming_profile: Option<Sha256Digest>,
    pub queued: usize,
    pub running: usize,
}

struct Job {
    queued_at: Instant,
    max_input_tokens: usize,
    scope: ScopeBinding,
    task: Task,
    deadline: Instant,
    authority: Authority,
    state: String,
    question: String,
    candidates: Vec<Candidate>,
    compute: accounting::ComputeCharge,
    delivery: accounting::DeliveryCharge,
    resident: ResidentPermit,
    admission: airk_kernel::service_resources::OptionalReuse,
    reply: oneshot::Sender<Result<Scored>>,
    compiled: Option<oneshot::Sender<CompiledReceipt>>,
    #[cfg(feature = "test-support")]
    observer: Option<execution_observer::Observer>,
    // Last field: actual original work/delivery/permit drops precede settlement observation.
    #[cfg(feature = "test-support")]
    observation: Option<execution_observer::Witness>,
}
/// Field drop order is part of the native lifetime contract: actual scorer
/// teardown completes before the exact authenticated asset-source pins release.
struct ResidentSession<S> {
    scorer: S,
    _source: Box<dyn AssetSource + Send>,
}
impl<S> std::ops::Deref for ResidentSession<S> {
    type Target = S;
    fn deref(&self) -> &S {
        &self.scorer
    }
}
struct Warm {
    stage: Stage,
    profile: ExecutionProfile,
    source: Box<dyn AssetSource + Send>,
    _reservation: Arc<Reservation>,
    reply: oneshot::Sender<Result<()>>,
    _compute: accounting::ComputeCharge,
    _delivery: accounting::DeliveryCharge,
    deadline: Option<Instant>,
    authority: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}
#[derive(Default)]
struct Queue {
    eligibility: eligibility::State,
    closed: bool,
    configuration: Option<NativeConfiguration>,
    device_failure: BTreeMap<Stage, DeviceFailureNotifier>,
    probes: VecDeque<device::Probe>,
    jobs: VecDeque<Job>,
    warm: VecDeque<Warm>,
    last_index: Option<airk_core::index_catalog::IndexId>,
    running_index: Option<airk_core::index_catalog::IndexId>,
    active_profiles: BTreeMap<Stage, Sha256Digest>,
    joint_calibrations:
        BTreeMap<Stage, Arc<airk_core::arbiter::context_action::calibration::JointCalibration>>,
    residents: BTreeMap<Stage, std::sync::Weak<Reservation>>,
    warming_profile: Option<(Stage, Sha256Digest)>,
    running_stage: Option<Stage>,
    release_profiles: std::collections::BTreeSet<Sha256Digest>,
}
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    accounting: Arc<accounting::Accounting>,
    closing: Arc<AtomicBool>,
    service: Incarnation,
    done: AtomicBool,
    unsettled: AtomicBool,
    running: AtomicUsize,
    changed: tokio::sync::Notify,
}
#[derive(Clone)]
pub struct Handle(Arc<Shared>);
struct LaneCompleted(Arc<Shared>);
impl Drop for LaneCompleted {
    fn drop(&mut self) {
        if self.0.unsettled.load(Ordering::Acquire) {
            {
                let mut queue = self
                    .0
                    .queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                queue.closed = true;
                queue.device_failure.clear();
                queue.eligibility.invalidate();
                queue.probes.clear();
                queue.jobs.clear();
                queue.warm.clear();
            }
            self.0.done.store(true, Ordering::Release);
            self.0.wake.notify_all();
            self.0.changed.notify_waiters();
            return;
        }
        let mut queue = self
            .0
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        queue.closed = true;
        queue.device_failure.clear();
        queue.probes.clear();
        queue.jobs.clear();
        queue.warm.clear();
        queue.active_profiles.clear();
        queue.joint_calibrations.clear();
        queue.warming_profile = None;
        queue.running_index = None;
        queue.running_stage = None;
        queue.residents.clear();
        self.0.running.store(0, Ordering::Release);
        self.0.accounting.arbiter_cached(false);
        self.0.accounting.mark_arbiter_barrier();
        self.0.done.store(true, Ordering::Release);
        self.0.wake.notify_all();
        self.0.changed.notify_waiters();
    }
}
pub(super) struct Lane {
    pub handle: Handle,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
}
fn stage_for_task(task: Task) -> Stage {
    if task == Task::FocusRank {
        Stage::Focus
    } else {
        Stage::Context
    }
}
fn selected_profile(configuration: &NativeConfiguration, stage: Stage) -> Option<Sha256Digest> {
    configuration.selected_profile(stage)
}
fn retains_resident(
    configuration: Option<&NativeConfiguration>,
    stage: Stage,
    manifest: Sha256Digest,
    released: &std::collections::BTreeSet<Sha256Digest>,
) -> bool {
    configuration.is_some_and(|config| selected_profile(config, stage) == Some(manifest))
        && !released.contains(&manifest)
}
fn permitted(
    configuration: Option<&NativeConfiguration>,
    job: &Job,
    profile: Sha256Digest,
) -> bool {
    configuration.is_some_and(|config| {
        config.installation() == job.scope.installation
            && config.catalog() == job.scope.catalog
            && config.revision() == job.scope.configuration_revision
            && config.selected_profile(stage_for_task(job.task)) == Some(profile)
            && config.eligible_for(stage_for_task(job.task), job.scope.index_id, job.task)
    }) && Instant::now() < job.deadline
}
impl Handle {
    pub fn device_failure_notifier(
        &self,
        stage: Stage,
        notifier: DeviceFailureNotifier,
    ) -> Result<()> {
        let mut queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        ensure!(
            !queue.closed && !self.0.closing.load(Ordering::Acquire),
            "Arbiter admission closed"
        );
        queue.device_failure.insert(stage, notifier);
        Ok(())
    }

    /// Runs on the existing lane under its actual native owner. Status never
    /// loads a provider; only explicit lifecycle admission calls this method.
    pub fn probe_devices(
        &self,
        lease: Arc<airk_store::service::ServiceLease>,
        model: airk_core::arbiter::device::SelectedModel,
        allowed_profiles: Vec<Sha256Digest>,
        host: Arc<Reservation>,
        deadline: Instant,
        authority: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<oneshot::Receiver<Result<DeviceProbe>>> {
        device::warm_admission(Some(deadline), Some(authority.as_ref()), Instant::now)?;
        let mut queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        ensure!(
            !queue.closed && !self.0.closing.load(Ordering::Acquire) && queue.probes.len() < 2,
            "device probe admission closed"
        );
        let (compute, delivery) = self.0.accounting.admit(self.0.service)?;
        let (reply, receive) = oneshot::channel();
        queue.probes.push_back(device::Probe {
            lease,
            host,
            #[cfg(all(feature = "test-support", feature = "gpu-cuda"))]
            evaluation: None,
            retained: None,
            allowed_profiles,
            model,
            deadline,
            authority,
            reply,
            compute,
            _delivery: delivery,
        });
        self.0.wake.notify_one();
        self.0.changed.notify_waiters();
        Ok(receive)
    }
    /// Reaccept actual retained sessions on the existing management lane. This
    /// borrows the exact resident charge; it cannot load a second model.
    pub fn reaccept_device(
        &self,
        lease: Arc<airk_store::service::ServiceLease>,
        model: airk_core::arbiter::device::SelectedModel,
        resident: ResidentPermit,
        deadline: Instant,
        authority: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<oneshot::Receiver<Result<DeviceProbe>>> {
        device::warm_admission(Some(deadline), Some(authority.as_ref()), Instant::now)?;
        let mut queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("retained eligibility queue unavailable"))?;
        ensure!(
            !queue.closed && !self.0.closing.load(Ordering::Acquire) && queue.probes.len() < 2,
            "retained eligibility admission closed"
        );
        ensure!(
            queue.active_profiles.get(&model.stage()) == Some(&resident.profile)
                && queue
                    .residents
                    .get(&model.stage())
                    .and_then(std::sync::Weak::upgrade)
                    .is_some_and(|charge| Arc::ptr_eq(&charge, &resident.reservation)),
            "retained eligibility requires exact current charged resident"
        );
        let (compute, delivery) = self.0.accounting.admit(self.0.service)?;
        let (reply, receive) = oneshot::channel();
        queue.probes.push_back(device::Probe {
            lease,
            host: resident.reservation,
            retained: Some(resident.profile),
            allowed_profiles: Vec::new(),
            #[cfg(all(feature = "test-support", feature = "gpu-cuda"))]
            evaluation: None,
            model,
            deadline,
            authority,
            reply,
            compute,
            _delivery: delivery,
        });
        self.0.wake.notify_one();
        self.0.changed.notify_waiters();
        Ok(receive)
    }
    /// Native observation only; false alone is never a completion permit.
    /// Fault recording additionally requires an actual returned typed outcome.
    pub fn unknown_device_work(&self) -> bool {
        if self.0.unsettled.load(Ordering::Acquire) {
            return true;
        }
        #[cfg(feature = "gpu-cuda")]
        {
            airk_retrieval::arbiter::runtime::cuda::process_poisoned()
        }
        #[cfg(not(feature = "gpu-cuda"))]
        {
            false
        }
    }
    pub fn resident_eligibilities(&self) -> Result<BTreeMap<Stage, ResidentEligibility>> {
        let queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("native eligibility observation unavailable"))?;
        ensure!(!queue.closed, "native eligibility owner closed");
        Ok(queue
            .eligibility
            .captured(queue.configuration.as_ref())
            .into_iter()
            .filter(|(stage, proof)| queue.active_profiles.get(stage) == Some(&proof.profile()))
            .collect())
    }
    /// Unqualified native evaluation only: the exact original lease, explicit
    /// Core bounds and actual resource reservation are required independently.
    #[cfg(all(feature = "test-support", feature = "gpu-cuda"))]
    #[allow(
        clippy::too_many_arguments,
        reason = "native evaluation requires independent lease, resource, device and original-deadline witnesses"
    )]
    pub fn probe_evaluation_device(
        &self,
        lease: Arc<airk_store::service::ServiceLease>,
        profile: ExecutionProfile,
        plan: airk_retrieval::arbiter::runtime::cuda::DeviceCapacityPlan,
        adapter: [u8; 16],
        host: Arc<Reservation>,
        bounds: airk_core::arbiter::device::GpuProbeReservation,
        deadline: Instant,
        authority: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<oneshot::Receiver<Result<DeviceProbe>>> {
        bounds.validate()?;
        ensure!(
            Instant::now() < deadline
                && deadline.saturating_duration_since(Instant::now())
                    <= Duration::from_millis(bounds.deadline_ms)
                && authority(),
            "original evaluation probe deadline/authority invalid"
        );
        let erased: Arc<dyn Send + Sync> = lease.clone();
        ensure!(
            profile.evaluation()
                && !profile.release_qualified()
                && profile.matches_evaluation_lease(&erased),
            "native GPU evaluation belongs to another lease"
        );
        device::validate_probe_bounds(
            &bounds,
            host.bytes(),
            profile.description().memory_reservation_bytes,
            plan.cold_peak_bytes(),
            plan.headroom_bytes(),
        )?;
        let model =
            airk_core::arbiter::device::SelectedModel::from_model(profile.description().model)?;
        let mut queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        ensure!(
            !queue.closed && !self.0.closing.load(Ordering::Acquire) && queue.probes.len() < 2,
            "native evaluation probe admission closed"
        );
        let (compute, delivery) = self.0.accounting.admit(self.0.service)?;
        let (reply, receive) = oneshot::channel();
        queue.probes.push_back(device::Probe {
            lease,
            host,
            retained: None,
            allowed_profiles: Vec::new(),
            model,
            deadline,
            authority,
            reply,
            compute,
            _delivery: delivery,
            evaluation: Some(device::EvaluationProbe {
                profile: Mutex::new(Some(profile)),
                plan,
                adapter,
                bounds,
            }),
        });
        self.0.wake.notify_one();
        self.0.changed.notify_waiters();
        Ok(receive)
    }
    pub async fn wait_profile_released(
        &self,
        profile: Sha256Digest,
        deadline: Instant,
    ) -> Result<()> {
        loop {
            let changed = self.0.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let released = {
                let queue = self
                    .0
                    .queue
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
                !queue
                    .active_profiles
                    .values()
                    .any(|value| *value == profile)
                    && !queue
                        .warming_profile
                        .is_some_and(|(_, value)| value == profile)
                    && !queue
                        .warm
                        .iter()
                        .any(|warm| warm.profile.description().manifest == profile)
            };
            if released {
                return Ok(());
            }
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), changed)
                .await
                .context("Arbiter native profile drain remains pending")?;
        }
    }
    pub fn observation(&self) -> Result<Observation> {
        self.observation_for(Stage::Context)
    }
    pub fn observation_for(&self, stage: Stage) -> Result<Observation> {
        let queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        Ok(Observation {
            configuration_revision: queue
                .configuration
                .as_ref()
                .map(NativeConfiguration::revision),
            active_profile: queue.active_profiles.get(&stage).copied(),
            warming_profile: queue
                .warming_profile
                .filter(|(warming, _)| *warming == stage)
                .map(|(_, profile)| profile),
            queued: queue
                .jobs
                .iter()
                .filter(|job| stage_for_task(job.task) == stage)
                .count()
                + queue.warm.iter().filter(|warm| warm.stage == stage).count(),
            running: usize::from(queue.running_stage == Some(stage)),
        })
    }
    /// Revoke queued work and ask the owned compute lane to drop this session
    /// after actual synchronous work. Observation, not this request, proves drain.
    pub fn release_profile(&self, profile: Sha256Digest) -> Result<()> {
        let mut queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        queue.release_profiles.insert(profile);
        queue
            .warm
            .retain(|warm| warm.profile.description().manifest != profile);
        queue.jobs.retain(|job| job.resident.profile != profile);
        self.0.wake.notify_one();
        self.0.changed.notify_waiters();
        Ok(())
    }
    pub fn configure(&self, configuration: Configuration) -> Result<()> {
        self.configure_native(NativeConfiguration::Legacy(configuration))
    }
    pub fn configure_native(&self, configuration: NativeConfiguration) -> Result<()> {
        configuration.validate()?;
        let mut queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        ensure!(
            !queue.closed && !self.0.closing.load(Ordering::Acquire),
            "Arbiter admission closed"
        );
        if let Some(previous) = &queue.configuration {
            ensure!(
                configuration.installation() == previous.installation()
                    && configuration.catalog() == previous.catalog()
                    && configuration.revision() >= previous.revision(),
                "Arbiter configuration identity regressed"
            );
            ensure!(
                configuration.revision() != previous.revision()
                    || configuration.same_document(previous),
                "Arbiter revision changed document"
            );
            for stage in [Stage::Focus, Stage::Context] {
                if configuration.selected_profile(stage).is_some()
                    && configuration.selected_profile(stage) != previous.selected_profile(stage)
                {
                    ensure!(
                        queue.running_stage != Some(stage)
                            && queue
                                .warming_profile
                                .is_none_or(|(warming, _)| warming != stage),
                        "variant cannot change while original native work is unsettled"
                    );
                    if let Some(active) = queue.active_profiles.get(&stage) {
                        ensure!(
                            queue.release_profiles.contains(active),
                            "variant change requires explicit resident drain"
                        );
                    }
                }
            }
        }
        if queue
            .configuration
            .as_ref()
            .is_none_or(|previous| !previous.same_selection(&configuration))
        {
            queue.eligibility.invalidate();
        }
        queue.eligibility.configure(&configuration);
        queue.configuration = Some(configuration);
        let current = queue.configuration.clone();
        queue.warm.retain(|warm| {
            current
                .as_ref()
                .is_some_and(|config| config.check_profile(warm.profile.description()).is_ok())
        });
        queue
            .jobs
            .retain(|job| permitted(current.as_ref(), job, job.resident.profile));
        self.0.wake.notify_one();
        self.0.changed.notify_waiters();
        Ok(())
    }
    pub fn warm(
        &self,
        profile: ExecutionProfile,
        source: Box<dyn AssetSource + Send>,
        reservation: Reservation,
    ) -> Result<oneshot::Receiver<Result<()>>> {
        ensure!(
            profile.description().device_execution.is_none(),
            "GPU warm requires original native lifecycle deadline"
        );
        self.warm_inner(profile, source, Arc::new(reservation), None, None)
    }
    pub fn warm_with_charge(
        &self,
        profile: ExecutionProfile,
        source: Box<dyn AssetSource + Send>,
        reservation: Arc<Reservation>,
        deadline: Instant,
        authority: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<oneshot::Receiver<Result<()>>> {
        ensure!(
            Instant::now() < deadline,
            "original native warm deadline expired"
        );
        self.warm_inner(
            profile,
            source,
            reservation,
            Some(deadline),
            Some(authority),
        )
    }
    fn warm_inner(
        &self,
        profile: ExecutionProfile,
        source: Box<dyn AssetSource + Send>,
        reservation: Arc<Reservation>,
        deadline: Option<Instant>,
        authority: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    ) -> Result<oneshot::Receiver<Result<()>>> {
        ensure!(
            profile.execution_authorized(),
            "Arbiter profile has no release qualification"
        );
        ensure!(
            reservation.bytes() >= profile.description().memory_reservation_bytes,
            "Arbiter warm memory reservation is too small"
        );
        let mut queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        ensure!(
            !queue.closed && !self.0.closing.load(Ordering::Acquire),
            "Arbiter admission closed"
        );
        let stage = profile
            .description()
            .declared_stage()
            .context("Native warm requires an explicit stage profile")?;
        queue
            .configuration
            .as_ref()
            .context("Arbiter configuration unavailable")?
            .check_profile(profile.description())?;
        ensure!(
            queue.warm.len() < 2 && !queue.warm.iter().any(|warm| warm.stage == stage),
            "Arbiter stage warm already queued"
        );
        ensure!(
            !queue
                .warming_profile
                .is_some_and(|(warming, _)| warming == stage),
            "Arbiter stage native warm still running"
        );
        queue
            .release_profiles
            .remove(&profile.description().manifest);
        let (compute, delivery) = self.0.accounting.admit(self.0.service)?;
        let (reply, response) = oneshot::channel();
        queue.warm.push_back(Warm {
            stage,
            profile,
            source,
            _reservation: reservation,
            reply,
            _compute: compute,
            _delivery: delivery,
            deadline,
            authority,
        });
        self.0.wake.notify_one();
        self.0.changed.notify_waiters();
        Ok(response)
    }
    pub fn resident(&self, profile: Sha256Digest) -> Result<ResidentPermit> {
        let queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        ensure!(
            !queue.closed
                && queue
                    .active_profiles
                    .values()
                    .any(|active| *active == profile)
                && !queue.release_profiles.contains(&profile),
            "Arbiter resident profile unavailable"
        );
        let stage = queue
            .active_profiles
            .iter()
            .find_map(|(stage, active)| (*active == profile).then_some(*stage))
            .context("Arbiter stage resident unavailable")?;
        let reservation = queue
            .residents
            .get(&stage)
            .and_then(std::sync::Weak::upgrade)
            .context("Arbiter native resident charge unavailable")?;
        Ok(ResidentPermit {
            profile,
            reservation,
        })
    }
    /// Immutable metadata authenticated by the actual resident scorer's warm.
    /// This never reopens paths, rehashes all weights or creates a session.
    pub fn joint_calibration(
        &self,
        profile: Sha256Digest,
    ) -> Result<Arc<airk_core::arbiter::context_action::calibration::JointCalibration>> {
        let queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        ensure!(
            !queue.closed
                && queue
                    .active_profiles
                    .values()
                    .any(|active| *active == profile)
                && !queue.release_profiles.contains(&profile),
            "joint resident profile unavailable"
        );
        queue
            .active_profiles
            .iter()
            .find_map(|(stage, active)| {
                if *active == profile {
                    queue.joint_calibrations.get(stage).cloned()
                } else {
                    None
                }
            })
            .context("resident has no authenticated joint calibration")
    }
    pub fn score(&self, request: ScoreRequest) -> Result<PendingScore> {
        self.score_inner(
            request,
            #[cfg(feature = "test-support")]
            None,
        )
    }
    #[cfg(feature = "test-support")]
    pub fn score_observed(
        &self,
        request: ScoreRequest,
        observer: execution_observer::Observer,
    ) -> Result<PendingScore> {
        self.score_inner(request, Some(observer))
    }
    fn score_inner(
        &self,
        request: ScoreRequest,
        #[cfg(feature = "test-support")] observer: Option<execution_observer::Observer>,
    ) -> Result<PendingScore> {
        let ScoreRequest {
            worker,
            scope,
            task,
            deadline,
            authority,
            state,
            question,
            candidates,
            resident,
            admission,
            max_input_tokens,
        } = request;
        // No source-authority callback runs under the optional queue lock.
        ensure!(authority(&scope), "Arbiter source authority expired");
        // Bounds are checked before ownership transfers to the optional lane.
        ensure!(
            state.len() <= 65536
                && question.len() <= 16384
                && !candidates.is_empty()
                && candidates.len() <= 8
                && candidates
                    .iter()
                    .all(|candidate| candidate.id.len() <= 128 && candidate.text.len() <= 8192),
            "Arbiter input bounds exceeded"
        );
        let mut queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        ensure!(
            !queue.closed && !self.0.closing.load(Ordering::Acquire),
            "Arbiter admission closed"
        );
        ensure!(
            queue.jobs.len() < 8
                && queue
                    .jobs
                    .iter()
                    .filter(|job| job.scope.index_id == scope.index_id)
                    .count()
                    + usize::from(queue.running_index == Some(scope.index_id))
                    < 2,
            "Arbiter queue constrained"
        );
        let stage = stage_for_task(task);
        ensure!(
            !queue.release_profiles.contains(&resident.profile),
            "Arbiter profile is draining"
        );
        ensure!(
            queue.active_profiles.get(&stage) == Some(&resident.profile)
                && queue
                    .residents
                    .get(&stage)
                    .and_then(std::sync::Weak::upgrade)
                    .is_some_and(|actual| Arc::ptr_eq(&actual, &resident.reservation)),
            "Arbiter resident session changed before score admission"
        );
        admission.validate(&resident.reservation)?;
        let (compute, delivery) = self.0.accounting.admit(worker)?;
        let (reply, response) = oneshot::channel();
        let (compiled, compiled_response) = oneshot::channel();
        let job = Job {
            queued_at: Instant::now(),
            max_input_tokens,
            compiled: Some(compiled),
            #[cfg(feature = "test-support")]
            observer,
            #[cfg(feature = "test-support")]
            observation: None,
            scope,
            task,
            deadline,
            authority,
            state,
            question,
            candidates,
            compute,
            delivery,
            resident,
            admission,
            reply,
        };
        ensure!(
            permitted(queue.configuration.as_ref(), &job, job.resident.profile),
            "Arbiter scope is not enrolled or live"
        );
        queue.jobs.push_back(job);
        self.0.wake.notify_one();
        self.0.changed.notify_waiters();
        Ok(PendingScore {
            compiled: compiled_response,
            completed: response,
        })
    }
    pub(super) fn close(&self) {
        let mut queue = self
            .0
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        queue.closed = true;
        queue.device_failure.clear();
        queue.probes.clear();
        queue.jobs.clear();
        queue.warm.clear();
        self.0.wake.notify_one();
        self.0.changed.notify_waiters();
    }
    pub(super) fn close_and_wait(&self) {
        self.close();
        let mut queue = self
            .0
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !self.0.done.load(Ordering::Acquire) {
            queue = self
                .0
                .wake
                .wait(queue)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}
impl Lane {
    pub(super) fn start(
        ownership: airk_kernel::inference_owner::NativeInferenceOwner,
        state: &Arc<State>,
    ) -> Result<Self> {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
            accounting: state
                .accounting
                .as_ref()
                .context("missing inference accounting")?
                .clone(),
            closing: state.closing.clone(),
            service: state.service,
            done: AtomicBool::new(false),
            unsettled: AtomicBool::new(false),
            running: AtomicUsize::new(0),
            changed: tokio::sync::Notify::new(),
        });
        let handle = Handle(shared.clone());
        shared.accounting.begin_arbiter();
        let thread = std::thread::Builder::new()
            .name("airk-arbiter-compute".into())
            .stack_size(8 << 20)
            .spawn(move || {
                let lane_ownership=ownership.clone();
                ownership.run(move || {
                    let ownership=lane_ownership;
                    // Declared before resident resources: its final observation
                    // follows actual scorer teardown on return or unwind.
                    let _completed = LaneCompleted(shared.clone());
                    // Tuple declaration order preserves scorer/source teardown before its charge.
                    let mut residents: BTreeMap<Stage, device::GuardedResident> = BTreeMap::new();
                    loop {
                        let mut queue = shared
                            .queue
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
                        if queue.closed {
                            break;
                        }
                        residents.retain(|stage, resident| {
                            retains_resident(queue.configuration.as_ref(), *stage, resident.scorer().profile().manifest, &queue.release_profiles)
                        });
                        if shared.unsettled.load(Ordering::Acquire) { queue.closed=true; break; }
                        queue.eligibility.reconcile(&residents)?;
                        queue.active_profiles.retain(|stage, _| residents.contains_key(stage));
                        queue.joint_calibrations.retain(|stage, _| residents.contains_key(stage));
                        queue.residents.retain(|stage, _| residents.contains_key(stage));
                        shared.accounting.arbiter_cached_count(residents.len());
                        shared.changed.notify_waiters();
                        if let Some(mut probe)=queue.probes.pop_front() {
                            probe.compute.running();shared.running.store(1,Ordering::Release);queue.running_stage=Some(probe.model.stage());drop(queue);
                            let result=if let Some(profile)=probe.retained {
                                let result=(|| {
                                    ensure!(ownership.same_lease(&probe.lease),"retained eligibility belongs to another native lease");
                                    let resident=residents.get(&probe.model.stage()).context("retained eligibility resident missing")?;
                                    ensure!(resident.scorer().profile().manifest==profile&&Arc::ptr_eq(resident.charge(),&probe.host),"retained eligibility differs from actual resident/charge");
                                    eligibility::refresh(&shared,&residents,probe.model.stage(),Some(probe.deadline),Some(probe.authority.clone()))?;
                                    Ok(DeviceProbe{candidates:vec![],rejections:vec![]})
                                })();
                                if residents.values().any(|resident|!resident.scorer().device_settled()) {shared.unsettled.store(true,Ordering::Release);}
                                result
                            }else {device::probe(&ownership,&probe,&shared)};
                            #[cfg(feature="gpu-cuda")]
                            if airk_retrieval::arbiter::runtime::cuda::process_poisoned(){shared.unsettled.store(true,Ordering::Release);}
                            if shared.unsettled.load(Ordering::Acquire) {
                                let error=match result {Err(error)=>error.context("native probe/eligibility settlement unknown; custody retained"),Ok(value)=>{std::mem::forget(value);anyhow::anyhow!("native probe/eligibility settlement unknown; custody retained")}};
                                let _=probe.reply.send(Err(error));std::mem::forget((probe.lease,probe.host,probe.compute,probe._delivery,ownership.clone()));break;
                            }
                            let _=probe.reply.send(result);
                            shared.queue.lock().map_err(|_|anyhow::anyhow!("Arbiter queue unavailable"))?.running_stage=None;
                            shared.running.store(0,Ordering::Release);shared.changed.notify_waiters();continue;
                        }
                        if let Some(mut warm) = queue.warm.pop_front() {
                            warm._compute.running();
                            shared.running.store(1, Ordering::Release);
                            shared.changed.notify_waiters();
                            queue.warming_profile = Some((warm.stage, warm.profile.description().manifest));
                            queue.running_stage = Some(warm.stage);
                            drop(queue);
                            if !eligibility::warm(warm,&ownership,&shared,&mut residents)? {break;}
                            continue;
                        }
                        if !queue.jobs.is_empty() {
                            let position = queue
                                .jobs
                                .iter()
                                .position(|job| Some(job.scope.index_id) != queue.last_index)
                                .unwrap_or(0);
                            let mut job = queue.jobs.remove(position).unwrap();
                            queue.last_index = Some(job.scope.index_id);
                            queue.running_index = Some(job.scope.index_id);
                            let stage = stage_for_task(job.task);
                            queue.running_stage = Some(stage);
                            let allowed = residents.get(&stage).is_some_and(|resident| {
                                let scorer=resident.scorer();let charge=resident.charge();
                                permitted(
                                    queue.configuration.as_ref(),
                                    &job,
                                    scorer.profile().manifest,
                                ) && queue.eligibility.current(stage,queue.configuration.as_ref())
                                    && job.resident.profile==scorer.profile().manifest
                                    && Arc::ptr_eq(charge,&job.resident.reservation)
                                    && scorer.profile().tasks.contains(&job.task)
                                    && !queue.release_profiles.contains(&scorer.profile().manifest)
                            });
                            drop(queue);
                            if !allowed || job.reply.is_closed() || !(job.authority)(&job.scope) {
                                shared.queue.lock().map_err(|_|anyhow::anyhow!("Arbiter queue unavailable"))?.running_index=None;
                                shared.queue.lock().map_err(|_|anyhow::anyhow!("Arbiter queue unavailable"))?.running_stage=None;
                                continue;
                            }
                            job.compute.running();
                            shared.running.store(1, Ordering::Release);
                            shared.changed.notify_waiters();
                            let resident=&residents.get(&stage).unwrap();
                            resident.deadline.set(Some(job.deadline));
                            let scorer = resident.scorer();
                            let queue_us = job.queued_at.elapsed().as_micros().min(u64::MAX as u128) as u64;
                            let compile_started=Instant::now();
                            let result = scorer.compile(&job.state, &job.question, &job.candidates).and_then(|input| {
                                let compile_us=compile_started.elapsed().as_micros().min(u64::MAX as u128) as u64;
                                // Known compilation is observable even when a
                                // subsequent token/deadline/authority gate refuses
                                // FFI. No native execution precedes those checks.
                                let receipt=CompiledReceipt {input_digest:input.input_digest,input_tokens:input.input_ids.len(),queue_us,compile_us};
                                let compiled_delivered=job.compiled.take().context("compiled receipt already consumed")?.send(receipt.clone()).is_ok();
                                ensure!(input.input_ids.len() <= job.max_input_tokens,"Arbiter original input-token budget exhausted before native execution");
                                ensure!(Instant::now() < job.deadline,"Arbiter original deadline exhausted during compilation");
                                let current={let queue=shared.queue.lock().map_err(|_|anyhow::anyhow!("Arbiter queue unavailable"))?;
                                    !queue.closed && !queue.release_profiles.contains(&scorer.profile().manifest) && queue.eligibility.current(stage,queue.configuration.as_ref()) && permitted(queue.configuration.as_ref(),&job,scorer.profile().manifest)};
                                ensure!(current && (job.authority)(&job.scope),"Arbiter source authority expired during compilation");
                                ensure!(compiled_delivered && !job.reply.is_closed(),"Arbiter delivery canceled before native dispatch");
                                let started=Instant::now();
                                ensure!(airk_kernel::service_resource_observations::available_memory().is_some_and(|available|available>=job.admission.warm_extra_bytes().saturating_add(256<<20)),"optional native score has no fresh OS headroom");
                                job.admission.validate(&job.resident.reservation)?;
                                #[cfg(feature = "test-support")]
                                let prepared_observation=job.observer.as_ref().and_then(|observer|observer.prepare(&job,&receipt));
                                dispatch_before_original_deadline(job.deadline,Instant::now,||job.reply.is_closed(),|| {
                                    #[cfg(feature = "test-support")]
                                    { job.observation=prepared_observation.map(|prepared|prepared.admitted()); }
                                    let scored=observed_score_call(&receipt, started, Instant::now, || scorer.score_with_deadline(&input,job.deadline));
                                    let scored=if scorer.device_settled() {scored} else {Err(anyhow::anyhow!("native GPU work settlement unknown; original reservation held"))};
                                    #[cfg(feature = "test-support")]
                                    if let Some(observation)=&mut job.observation {observation.returned(&scored);}
                                    scored
                                })
                            });
                            if !scorer.device_settled() {
                                shared.unsettled.store(true,Ordering::Release);
                                let _=job.reply.send(Err(anyhow::anyhow!("native GPU work settlement unknown; no retry or fallback")));
                                #[cfg(feature="test-support")]
                                std::mem::forget((job.compute,job.delivery,job.resident,job.admission,job.observation,ownership.clone()));
                                #[cfg(not(feature="test-support"))]
                                std::mem::forget((job.compute,job.delivery,job.resident,job.admission,ownership.clone()));
                                break;
                            }
                            let source_live = (job.authority)(&job.scope);
                            let recovery=if device::fault_reporting_allowed(true,scorer.device_settled(),source_live,Instant::now()<job.deadline,!job.reply.is_closed()) {
                                let queue=shared.queue.lock().map_err(|_|anyhow::anyhow!("Arbiter queue unavailable"))?;
                                queue.configuration.as_ref().filter(|configuration|permitted(Some(configuration),&job,scorer.profile().manifest))
                                    .and_then(|configuration|device::known_fault(&result,resident,configuration))
                                    .and_then(|fault|queue.device_failure.get(&stage).cloned().map(|notify|(fault,notify)))
                            }else{None};
                            let delivery_admission = shared.queue.lock()
                                .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))
                                .and_then(|mut queue| {
                                    queue.running_index=None;
                                    queue.running_stage=None;
                                    ensure!(source_live && !queue.closed
                                        && queue.eligibility.current(stage,queue.configuration.as_ref())
                                        && !queue.release_profiles.contains(&scorer.profile().manifest)
                                        && permitted(queue.configuration.as_ref(), &job, scorer.profile().manifest),
                                        "Arbiter delivery revoked");
                                    Ok(())
                                });
                            let result = finish_score_delivery(result, delivery_admission);
                            let _delivery = job.delivery;
                            let _ = job.reply.send(result);
                            if let Some((fault,notify))=recovery {
                                {let mut queue=shared.queue.lock().map_err(|_|anyhow::anyhow!("Arbiter queue unavailable"))?;queue.release_profiles.insert(fault.key().profile);queue.jobs.retain(|job|job.resident.profile!=fault.key().profile);}
                                notify(fault);
                            }
                            shared.running.store(0, Ordering::Release);
                            shared.changed.notify_waiters();
                            continue;
                        }
                        drop(
                            shared
                                .wake
                                .wait(queue)
                                .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?,
                        );
                    }
                    drop(residents);
                    {
                        let mut queue = shared
                            .queue
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        queue.eligibility.invalidate();
                        queue.active_profiles.clear();
                        queue.joint_calibrations.clear();
                        queue.residents.clear();
                        queue.warming_profile = None;
                        queue.running_stage = None;
                    }
                    if !shared.unsettled.load(Ordering::Acquire) {
                        shared.accounting.arbiter_cached(false);
                        shared.accounting.mark_arbiter_barrier();
                    }
                    shared.changed.notify_waiters();
                    Ok(())
                })?
            })?;
        Ok(Self {
            handle,
            thread: Some(thread),
        })
    }
    pub(super) fn poll_stop(&mut self) -> Result<bool> {
        self.handle.close();
        if self
            .thread
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
        {
            self.thread
                .take()
                .unwrap()
                .join()
                .map_err(|_| anyhow::anyhow!("Arbiter compute panicked"))??;
        }
        Ok(self.thread.is_none())
    }
}
fn dispatch_before_original_deadline<T>(
    deadline: Instant,
    now: impl FnOnce() -> Instant,
    canceled: impl FnOnce() -> bool,
    dispatch: impl FnOnce() -> Result<T>,
) -> Result<T> {
    ensure!(
        now() < deadline,
        "Arbiter original deadline exhausted before native dispatch"
    );
    ensure!(
        !canceled(),
        "Arbiter delivery canceled before native dispatch"
    );
    dispatch()
}

impl Drop for Lane {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
#[cfg(all(test, feature = "test-support"))]
mod qualification;
#[cfg(test)]
mod tests;
