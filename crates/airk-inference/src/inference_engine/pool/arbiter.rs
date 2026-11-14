// SPDX-License-Identifier: Apache-2.0
//! Optional, bounded compute lane inside the actual inference owner.
use super::*;
use airk_core::arbiter::{Configuration, ScopeBinding, Task};
use airk_retrieval::arbiter::{
    compiler::Candidate,
    runtime::{AssetSource, ExecutionProfile, Scorer},
};
use std::{collections::VecDeque, sync::Condvar};

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
    profile: ExecutionProfile,
    source: Box<dyn AssetSource + Send>,
    _reservation: Reservation,
    reply: oneshot::Sender<Result<()>>,
    _compute: accounting::ComputeCharge,
    _delivery: accounting::DeliveryCharge,
}
#[derive(Default)]
struct Queue {
    closed: bool,
    configuration: Option<Configuration>,
    jobs: VecDeque<Job>,
    warm: Option<Warm>,
    last_index: Option<airk_core::index_catalog::IndexId>,
    running_index: Option<airk_core::index_catalog::IndexId>,
    active_profile: Option<Sha256Digest>,
    joint_calibration:
        Option<Arc<airk_core::arbiter::context_action::calibration::JointCalibration>>,
    resident: std::sync::Weak<Reservation>,
    warming_profile: Option<Sha256Digest>,
    release_profiles: std::collections::BTreeSet<Sha256Digest>,
}
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    accounting: Arc<accounting::Accounting>,
    closing: Arc<AtomicBool>,
    service: Incarnation,
    done: AtomicBool,
    running: AtomicUsize,
    changed: tokio::sync::Notify,
}
#[derive(Clone)]
pub struct Handle(Arc<Shared>);
struct LaneCompleted(Arc<Shared>);
impl Drop for LaneCompleted {
    fn drop(&mut self) {
        let mut queue = self
            .0
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        queue.closed = true;
        queue.jobs.clear();
        queue.warm = None;
        queue.active_profile = None;
        queue.joint_calibration = None;
        queue.warming_profile = None;
        queue.running_index = None;
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
fn permitted(configuration: Option<&Configuration>, job: &Job, profile: Sha256Digest) -> bool {
    configuration.is_some_and(|config| {
        config.enabled
            && config.installation == job.scope.installation
            && config.catalog == job.scope.catalog
            && config.revision == job.scope.configuration_revision
            && config.selected_profile == Some(profile)
            && config.indexes.iter().any(|index| {
                index.index_id == job.scope.index_id && index.tasks.contains(&job.task)
            })
    }) && Instant::now() < job.deadline
}
impl Handle {
    pub async fn wait_profile_released(
        &self,
        profile: Sha256Digest,
        deadline: Instant,
    ) -> Result<()> {
        loop {
            let changed = self.0.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let state = self.observation()?;
            if state.active_profile != Some(profile) && state.warming_profile != Some(profile) {
                return Ok(());
            }
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), changed)
                .await
                .context("Arbiter native profile drain remains pending")?;
        }
    }
    pub fn observation(&self) -> Result<Observation> {
        let queue = self
            .0
            .queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
        Ok(Observation {
            configuration_revision: queue
                .configuration
                .as_ref()
                .map(|configuration| configuration.revision),
            active_profile: queue.active_profile,
            warming_profile: queue.warming_profile,
            queued: queue.jobs.len() + usize::from(queue.warm.is_some()),
            running: self.0.running.load(Ordering::Acquire),
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
        if queue
            .warm
            .as_ref()
            .is_some_and(|warm| warm.profile.description().manifest == profile)
        {
            queue.warm = None;
        }
        if queue
            .configuration
            .as_ref()
            .is_some_and(|config| config.selected_profile == Some(profile))
        {
            queue.jobs.clear();
        }
        self.0.wake.notify_one();
        self.0.changed.notify_waiters();
        Ok(())
    }
    pub fn configure(&self, configuration: Configuration) -> Result<()> {
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
                configuration.installation == previous.installation
                    && configuration.catalog == previous.catalog
                    && configuration.revision >= previous.revision,
                "Arbiter configuration identity regressed"
            );
            ensure!(
                configuration.revision != previous.revision || configuration == *previous,
                "Arbiter revision changed bytes"
            );
        }
        queue.configuration = Some(configuration);
        if queue.warm.as_ref().is_some_and(|warm| {
            !queue.configuration.as_ref().is_some_and(|config| {
                config.enabled
                    && config.selected_profile == Some(warm.profile.description().manifest)
            })
        }) {
            queue.warm = None;
        }
        // Revocation discards queued source bytes without compiling them.
        let profile = queue
            .configuration
            .as_ref()
            .and_then(|config| config.selected_profile);
        let configuration = queue.configuration.clone();
        queue.jobs.retain(|job| {
            profile.is_some_and(|profile| permitted(configuration.as_ref(), job, profile))
        });
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
        ensure!(
            queue
                .configuration
                .as_ref()
                .is_some_and(|config| config.enabled
                    && config.selected_profile == Some(profile.description().manifest)),
            "Arbiter profile is not enabled"
        );
        ensure!(queue.warm.is_none(), "Arbiter warm already queued");
        ensure!(
            queue.warming_profile.is_none(),
            "Arbiter native warm still running"
        );
        queue
            .release_profiles
            .remove(&profile.description().manifest);
        let (compute, delivery) = self.0.accounting.admit(self.0.service)?;
        let (reply, response) = oneshot::channel();
        queue.warm = Some(Warm {
            profile,
            source,
            _reservation: reservation,
            reply,
            _compute: compute,
            _delivery: delivery,
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
                && queue.active_profile == Some(profile)
                && !queue.release_profiles.contains(&profile),
            "Arbiter resident profile unavailable"
        );
        let reservation = queue
            .resident
            .upgrade()
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
                && queue.active_profile == Some(profile)
                && !queue.release_profiles.contains(&profile),
            "joint resident profile unavailable"
        );
        queue
            .joint_calibration
            .clone()
            .context("resident has no authenticated joint calibration")
    }
    pub fn score(&self, request: ScoreRequest) -> Result<PendingScore> {
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
        ensure!(
            !queue
                .configuration
                .as_ref()
                .and_then(|config| config.selected_profile)
                .is_some_and(|profile| queue.release_profiles.contains(&profile)),
            "Arbiter profile is draining"
        );
        ensure!(
            queue.active_profile == Some(resident.profile)
                && queue
                    .resident
                    .upgrade()
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
            queue
                .configuration
                .as_ref()
                .and_then(|config| config.selected_profile)
                .is_some_and(|profile| permitted(queue.configuration.as_ref(), &job, profile)),
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
        queue.jobs.clear();
        queue.warm = None;
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
            running: AtomicUsize::new(0),
            changed: tokio::sync::Notify::new(),
        });
        let handle = Handle(shared.clone());
        shared.accounting.begin_arbiter();
        let thread = std::thread::Builder::new()
            .name("airk-arbiter-compute".into())
            .stack_size(8 << 20)
            .spawn(move || {
                ownership.run(move || {
                    // Declared before resident resources: its final observation
                    // follows actual scorer teardown on return or unwind.
                    let _completed = LaneCompleted(shared.clone());
                    let mut resident_reservation: Option<Arc<Reservation>> = None;
                    let mut scorer: Option<ResidentSession<Scorer>> = None;
                    loop {
                        let mut queue = shared
                            .queue
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
                        if queue.closed {
                            break;
                        }
                        if !queue
                            .configuration
                            .as_ref()
                            .is_some_and(|config| config.enabled && !config.indexes.is_empty())
                            || scorer.as_ref().is_some_and(|scorer| {
                                queue.release_profiles.contains(&scorer.profile().manifest)
                            })
                        {
                            scorer = None;
                            queue.active_profile = None;
                            queue.joint_calibration = None;
                            queue.resident=std::sync::Weak::new();
                            resident_reservation = None;
                            shared.accounting.arbiter_cached(false);
                            shared.changed.notify_waiters();
                        }
                        if let Some(mut warm) = queue.warm.take() {
                            warm._compute.running();
                            shared.running.store(1, Ordering::Release);
                            shared.changed.notify_waiters();
                            queue.warming_profile = Some(warm.profile.description().manifest);
                            drop(queue);
                            let loaded = Scorer::open(warm.profile, warm.source.as_ref());
                            let mut queue = shared
                                .queue
                                .lock()
                                .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))?;
                            let result = match loaded {
                                Ok(loaded)
                                    if !queue.closed
                                        && !queue
                                            .release_profiles
                                            .contains(&loaded.profile().manifest)
                                        && queue.configuration.as_ref().is_some_and(|config| {
                                            config.enabled
                                                && config.selected_profile
                                                    == Some(loaded.profile().manifest)
                                        }) =>
                                {
                                    queue.joint_calibration=loaded.joint_calibration();
                                    scorer = Some(ResidentSession {scorer:loaded,_source:warm.source});
                                    queue.active_profile =
                                        scorer.as_ref().map(|scorer| scorer.profile().manifest);
                                    resident_reservation = Some(Arc::new(warm._reservation));
                                    queue.resident=Arc::downgrade(resident_reservation.as_ref().unwrap());
                                    shared.accounting.arbiter_cached(true);
                                    Ok(())
                                }
                                Ok(_) => Err(anyhow::anyhow!("Arbiter warm revoked")),
                                Err(error) => Err(error),
                            };
                            queue.warming_profile = None;
                            drop(queue);
                            shared.running.store(0, Ordering::Release);
                            shared.changed.notify_waiters();
                            let _ = warm.reply.send(result);
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
                            let allowed = scorer.as_ref().is_some_and(|scorer| {
                                permitted(
                                    queue.configuration.as_ref(),
                                    &job,
                                    scorer.profile().manifest,
                                ) && job.resident.profile==scorer.profile().manifest
                                    && resident_reservation.as_ref().is_some_and(|resident|Arc::ptr_eq(resident,&job.resident.reservation))
                                    && scorer.profile().tasks.contains(&job.task)
                                    && !queue.release_profiles.contains(&scorer.profile().manifest)
                            });
                            drop(queue);
                            if !allowed || job.reply.is_closed() || !(job.authority)(&job.scope) {
                                shared.queue.lock().map_err(|_|anyhow::anyhow!("Arbiter queue unavailable"))?.running_index=None;
                                continue;
                            }
                            job.compute.running();
                            shared.running.store(1, Ordering::Release);
                            shared.changed.notify_waiters();
                            let scorer = scorer.as_ref().unwrap();
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
                                    !queue.closed && !queue.release_profiles.contains(&scorer.profile().manifest) && permitted(queue.configuration.as_ref(),&job,scorer.profile().manifest)};
                                ensure!(current && (job.authority)(&job.scope),"Arbiter source authority expired during compilation");
                                ensure!(compiled_delivered && !job.reply.is_closed(),"Arbiter delivery canceled before native dispatch");
                                let started=Instant::now();
                                ensure!(airk_kernel::service_resource_observations::available_memory().is_some_and(|available|available>=job.resident.reservation.bytes().saturating_add(256<<20)),"optional native score has no fresh OS headroom");
                                job.admission.validate(&job.resident.reservation)?;
                                dispatch_before_original_deadline(job.deadline,Instant::now,||job.reply.is_closed(),|| {
                                    observed_score_call(&receipt, started, Instant::now, || scorer.score(&input))
                                })
                            });
                            let source_live = (job.authority)(&job.scope);
                            let delivery_admission = shared.queue.lock()
                                .map_err(|_| anyhow::anyhow!("Arbiter queue unavailable"))
                                .and_then(|mut queue| {
                                    queue.running_index=None;
                                    ensure!(source_live && !queue.closed
                                        && !queue.release_profiles.contains(&scorer.profile().manifest)
                                        && permitted(queue.configuration.as_ref(), &job, scorer.profile().manifest),
                                        "Arbiter delivery revoked");
                                    Ok(())
                                });
                            let result = finish_score_delivery(result, delivery_admission);
                            let _delivery = job.delivery;
                            let _ = job.reply.send(result);
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
                    drop(scorer);
                    drop(resident_reservation);
                    {
                        let mut queue = shared
                            .queue
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        queue.active_profile = None;
                        queue.joint_calibration = None;
                        queue.warming_profile = None;
                    }
                    shared.accounting.arbiter_cached(false);
                    shared.accounting.mark_arbiter_barrier();
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
