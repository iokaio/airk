// SPDX-License-Identifier: Apache-2.0
//! Worker-bound service admission. Unscoped work cannot fall back to legacy root slots.
//!
//! A child-process worker installs its one owner for the process (`install`).
//! An in-process worker installs its owner into its own [`ExecutionScope`] and
//! binds that scope on every thread it runs; a bound thread only ever observes
//! its scope's owner, never the process owner or another worker's owner.
use airk_core::{
    execution::{ExecutionMode, ExecutionPlan},
    service::WorkerIdentity,
    service_resources::WorkAllowance,
};
use anyhow::{Context, Result, ensure};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
mod progress;
struct Owner {
    root: PathBuf,
    identity: WorkerIdentity,
    started: Instant,
    renewed: AtomicU64,
    active: Mutex<Option<airk_core::identity::Incarnation>>,
    paused: AtomicBool,
    canceled: AtomicBool,
    yield_to_source: AtomicBool,
    progress: Mutex<Option<progress::Tracker>>,
    stalled: AtomicBool,
    waiting: Mutex<Option<airk_core::service_resources::WaitReason>>,
}
static OWNER: OnceLock<Owner> = OnceLock::new();

/// Why an in-process owner stopped accepting work without reaching a safe
/// point. A child process exits instead; an in-process owner is quarantined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StuckReason {
    /// Accepted work stopped advancing and did not drain after cancellation.
    WorkStalled,
    /// A read passed its hard deadline; its native call may still be running.
    QueryDeadline,
}
impl StuckReason {
    /// The diagnostic code recorded for an owner quarantined for this reason.
    pub fn code(self) -> &'static str {
        match self {
            Self::WorkStalled => "owner_quarantined_work_stalled",
            Self::QueryDeadline => "owner_quarantined_query_deadline",
        }
    }
    fn encode(self) -> u8 {
        match self {
            Self::WorkStalled => 1,
            Self::QueryDeadline => 2,
        }
    }
    fn decode(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::WorkStalled),
            2 => Some(Self::QueryDeadline),
            _ => None,
        }
    }
}

/// One in-process worker's slots. The stop flags exist before its owner is
/// installed, so a stop requested during startup or activation is observed.
#[derive(Default)]
struct ScopeState {
    owner: OnceLock<Owner>,
    /// Set only for a stopped or quarantined owner; never by a work grant.
    draining: AtomicBool,
    /// The first [`StuckReason`], encoded; zero while the owner is healthy.
    stuck: AtomicU8,
}

/// The owner slot of one in-process worker, shared by every thread it binds.
#[derive(Clone, Default)]
pub struct ExecutionScope(Arc<ScopeState>);
thread_local! { static SCOPE: RefCell<Option<ExecutionScope>> = const { RefCell::new(None) }; }

/// Restores the thread's previous binding when dropped.
pub struct ScopeBinding(Option<ExecutionScope>);
impl Drop for ScopeBinding {
    fn drop(&mut self) {
        let previous = self.0.take();
        // A thread-exit binding may drop after this thread's slot is gone.
        let _ = SCOPE.try_with(|slot| *slot.borrow_mut() = previous);
    }
}

impl ExecutionScope {
    pub fn new() -> Self {
        Self::default()
    }
    /// The in-process equivalent of [`install`]: exactly once per scope.
    pub fn install(&self, root: &Path, identity: WorkerIdentity) -> Result<()> {
        ensure!(
            self.0.owner.set(Owner::new(root, identity)?).is_ok(),
            "service execution already installed"
        );
        Ok(())
    }
    /// Bind this scope to the calling thread until the returned guard drops.
    pub fn bind_thread(&self) -> ScopeBinding {
        ScopeBinding(SCOPE.with(|slot| slot.borrow_mut().replace(self.clone())))
    }
    /// The calling thread's binding, for helper threads of the same work.
    /// `None` means the thread uses the process owner, if one is installed.
    pub fn current() -> Option<Self> {
        SCOPE.with(|slot| slot.borrow().clone())
    }
    /// Ask this owner to stop at its next safe point: inside accepted work, and
    /// equally during startup, activation and reads, before or after `install`.
    /// This is the in-process replacement for killing a stopped owner's
    /// process; it is permanent for the scope and never settles the work.
    pub fn drain(&self) {
        self.0.draining.store(true, Ordering::Release);
    }
    pub fn draining(&self) -> bool {
        self.0.draining.load(Ordering::Acquire)
    }
    /// The owner cannot reach a safe point. It accepts no more work, every
    /// later safe point drains, and its supervisor quarantines it: the stuck
    /// thread keeps its lease until it returns. The first reason is kept.
    pub fn quarantine(&self, reason: StuckReason) {
        let _ =
            self.0
                .stuck
                .compare_exchange(0, reason.encode(), Ordering::AcqRel, Ordering::Acquire);
        self.drain();
    }
    pub fn stuck(&self) -> Option<StuckReason> {
        StuckReason::decode(self.0.stuck.load(Ordering::Acquire))
    }
}

/// The calling thread's worker bindings, carried into a helper thread that
/// performs part of the same work. Unbound threads carry nothing, so a child
/// process keeps observing its process owner and connector.
pub(crate) struct Inherited {
    execution: Option<ExecutionScope>,
    connector: Option<airk_retrieval::embedding_worker::ConnectorScope>,
}
impl Inherited {
    pub(crate) fn capture() -> Self {
        Self {
            execution: ExecutionScope::current(),
            connector: airk_retrieval::embedding_worker::ConnectorScope::current(),
        }
    }
    pub(crate) fn bind(
        &self,
    ) -> (
        Option<ScopeBinding>,
        Option<airk_retrieval::embedding_worker::ConnectorBinding>,
    ) {
        (
            self.execution.as_ref().map(ExecutionScope::bind_thread),
            self.connector
                .as_ref()
                .map(airk_retrieval::embedding_worker::ConnectorScope::bind_thread),
        )
    }
}

/// The calling thread's owner: its bound scope's, otherwise the process owner.
fn with_owner<R>(action: impl FnOnce(&Owner) -> R) -> Option<R> {
    SCOPE.with(|slot| match slot.borrow().as_ref() {
        Some(scope) => scope.0.owner.get().map(action),
        None => OWNER.get().map(action),
    })
}
/// The calling thread's in-process owner was asked to drain. A thread that
/// is not bound to a scope (a child process) is never drained this way.
fn bound_scope_draining() -> bool {
    SCOPE.with(|slot| slot.borrow().as_ref().is_some_and(ExecutionScope::draining))
}
fn observe(action: impl FnOnce(&mut progress::Tracker, Instant)) {
    with_owner(|owner| {
        if let Ok(mut progress) = owner.progress.lock()
            && let Some(progress) = progress.as_mut()
        {
            action(progress, Instant::now());
        }
    });
}
pub fn progress() -> Option<airk_core::service_progress::WorkProgress> {
    with_owner(|owner| {
        owner
            .progress
            .lock()
            .ok()?
            .as_ref()
            .map(|progress| progress.snapshot(Instant::now()))
    })
    .flatten()
}
pub fn progress_phase(phase: airk_core::service_progress::WorkPhase) {
    progress_waiting(None);
    observe(|progress, now| progress.phase(phase, now));
}
/// Metadata only: a wait observation must never reset work advancement clocks.
pub fn progress_waiting(reason: Option<airk_core::service_resources::WaitReason>) {
    observe(|progress, now| progress.wait(reason.is_some(), now));
    with_owner(|owner| {
        if let Ok(mut waiting) = owner.waiting.lock() {
            *waiting = reason;
        }
    });
}
pub fn waiting_for() -> Option<airk_core::service_resources::WaitReason> {
    with_owner(|owner| *owner.waiting.lock().ok()?).flatten()
}
pub fn with_database_admission<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
    airk_store::current::with_work_admission(
        Duration::from_secs(30),
        checkpoint,
        |waiting| {
            progress_waiting(
                waiting.then_some(airk_core::service_resources::WaitReason::Publication),
            )
        },
        work,
    )
}
pub(crate) fn progress_capture() {
    observe(|progress, now| progress.capture(now));
}
pub(crate) fn progress_extracted() {
    observe(|progress, now| progress.extracted(now));
}
pub(crate) fn progress_prepared(changes: u64) {
    observe(|progress, now| progress.prepared(changes, now));
}
pub(crate) fn progress_semantic_prepared() {
    observe(|progress, now| progress.semantic_prepared(now));
}
pub(crate) fn progress_committed(changes: u64) {
    observe(|progress, now| progress.committed(changes, now));
}
pub fn progress_verified(objects: u64) {
    observe(|progress, now| progress.verified(objects, now));
}
pub(crate) fn progress_indexed(event: &airk_core::readiness::IndexProgress) {
    observe(|progress, now| progress.indexed(event, now));
}
struct Work {
    grant: WorkAllowance,
    started: Instant,
    bytes: u64,
}
thread_local! { static WORK: RefCell<Option<Work>> = const { RefCell::new(None) }; }
impl Owner {
    fn new(root: &Path, identity: WorkerIdentity) -> Result<Self> {
        Ok(Owner {
            root: root.canonicalize()?,
            identity,
            started: Instant::now(),
            renewed: AtomicU64::new(0),
            active: Mutex::new(None),
            paused: AtomicBool::new(false),
            canceled: AtomicBool::new(false),
            yield_to_source: AtomicBool::new(false),
            progress: Mutex::new(None),
            stalled: AtomicBool::new(false),
            waiting: Mutex::new(None),
        })
    }
}
pub fn install(root: &Path, identity: WorkerIdentity) -> Result<()> {
    let owner = Owner::new(root, identity)?;
    ensure!(
        OWNER.set(owner).is_ok(),
        "service execution already installed"
    );
    Ok(())
}
pub fn activate(grant: &WorkAllowance) -> Result<()> {
    grant.validate()?;
    with_owner(|owner| -> Result<()> {
        ensure!(owner.identity == grant.identity, "foreign work grant");
        let mut active = owner
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("work control unavailable"))?;
        *active = Some(grant.id);
        owner.paused.store(false, Ordering::Release);
        owner.canceled.store(false, Ordering::Release);
        owner.yield_to_source.store(false, Ordering::Release);
        owner.stalled.store(false, Ordering::Release);
        *owner
            .waiting
            .lock()
            .map_err(|_| anyhow::anyhow!("work wait observation unavailable"))? = None;
        *owner
            .progress
            .lock()
            .map_err(|_| anyhow::anyhow!("work observation unavailable"))? =
            Some(progress::Tracker::new(grant, Instant::now()));
        Ok(())
    })
    .context("service execution not installed")??;
    renew(grant.identity)
}
pub fn renew(identity: WorkerIdentity) -> Result<()> {
    with_owner(|owner| -> Result<()> {
        ensure!(owner.identity == identity, "foreign work renewal");
        owner.renewed.store(
            owner.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            Ordering::Release,
        );
        Ok(())
    })
    .context("service execution not installed")?
}
pub fn control(
    identity: WorkerIdentity,
    id: airk_core::identity::Incarnation,
    paused: bool,
    canceled: bool,
    yield_to_source: bool,
) -> Result<()> {
    with_owner(|owner| -> Result<()> {
        let active = owner
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("work control unavailable"))?;
        ensure!(
            owner.identity == identity && *active == Some(id),
            "foreign work control"
        );
        owner.paused.store(paused, Ordering::Release);
        observe(|progress, now| progress.pause(paused, now));
        // Cancel is monotonic for a grant; a later pulse cannot revoke it.
        if canceled {
            owner.canceled.store(true, Ordering::Release);
        }
        // Admission is recomputed by the owner. Unlike cancellation, a source hint
        // can be withdrawn without changing this exact accepted grant.
        owner
            .yield_to_source
            .store(yield_to_source, Ordering::Release);
        Ok(())
    })
    .context("service execution not installed")?
}

/// A process-owned metadata hint for this thread's exact active work grant.
/// This never inspects sources, databases, model state or another worker.
pub fn source_yield_requested() -> bool {
    with_owner(|owner| {
        let Some(id) = WORK.with(|slot| slot.borrow().as_ref().map(|work| work.grant.id)) else {
            return false;
        };
        owner.active.lock().is_ok_and(|active| {
            *active == Some(id) && owner.yield_to_source.load(Ordering::Acquire)
        })
    })
    .unwrap_or(false)
}
#[derive(Debug)]
pub struct Canceled;
impl std::fmt::Display for Canceled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("service job canceled at a safe boundary")
    }
}
impl std::error::Error for Canceled {}
#[derive(Debug)]
pub struct WorkStalled;
impl std::fmt::Display for WorkStalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("service job stopped advancing")
    }
}
impl std::error::Error for WorkStalled {}
/// A stopped in-process owner reached a safe point; its work is not settled.
#[derive(Debug)]
pub struct Draining;
impl std::fmt::Display for Draining {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("service owner stopped at a safe boundary")
    }
}
impl std::error::Error for Draining {}
/// True means the owned worker must exit after safe cancellation failed to drain.
pub fn watchdog() -> bool {
    with_owner(|owner| {
        let mut force = false;
        if let Ok(mut progress) = owner.progress.lock()
            && let Some(progress) = progress.as_mut()
        {
            let (stalled, expired) = progress.watchdog(Instant::now());
            force = expired;
            if stalled {
                owner.stalled.store(true, Ordering::Release);
            }
        }
        force
    })
    .unwrap_or(false)
}
pub fn checkpoint() -> Result<()> {
    // A drained in-process owner stops at every safe point: inside a work
    // grant, and equally in its startup, activation and reads, which run
    // without one (and possibly before its owner is installed).
    ensure!(!bound_scope_draining(), Draining);
    with_owner(|owner| {
        let ttl = WORK.with(|slot| slot.borrow().as_ref().map(|work| work.grant.ttl_ms));
        let Some(ttl) = ttl else { return Ok(()) };
        loop {
            ensure!(!bound_scope_draining(), Draining);
            ensure!(!owner.canceled.load(Ordering::Acquire), Canceled);
            ensure!(!owner.stalled.load(Ordering::Acquire), WorkStalled);
            let elapsed = owner.started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            ensure!(
                elapsed.saturating_sub(owner.renewed.load(Ordering::Acquire)) < ttl,
                "service work allowance expired"
            );
            if !owner.paused.load(Ordering::Acquire) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    })
    .unwrap_or(Ok(()))
}
pub(crate) fn installed() -> bool {
    with_owner(|_| ()).is_some()
}
pub fn run<T>(grant: WorkAllowance, work: impl FnOnce() -> Result<T>) -> Result<T> {
    grant.validate()?;
    ensure!(
        with_owner(|owner| owner.identity == grant.identity).unwrap_or(false),
        "foreign service work allowance"
    );
    WORK.with(|slot| {
        ensure!(slot.borrow().is_none(), "nested service work grant");
        *slot.borrow_mut() = Some(Work {
            grant,
            started: Instant::now(),
            bytes: 0,
        });
        Ok::<_, anyhow::Error>(())
    })?;
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            WORK.with(|slot| *slot.borrow_mut() = None);
        }
    }
    let _reset = Reset;
    // Owner work outside a request: a usage span that overlaps it is reported
    // as overlapped, except spans this job opens on this same thread.
    let _activity = crate::OwnerActivity::enter();
    // Extraction helpers waiting for the one PDF parser slot stop when this
    // owner drains, is canceled or stalls; they never start a parser then.
    let scope = ExecutionScope::current();
    let probe = scope.clone();
    let interruption: airk_ingest::pdf_slot::Interruption =
        Arc::new(move || owner_interrupted(probe.as_ref()));
    let result = airk_ingest::pdf_slot::with_interruption(interruption, || {
        airk_store::quota::with_lock_admission(
            Duration::from_secs(30),
            checkpoint,
            |waiting| {
                progress_waiting(
                    waiting.then_some(airk_core::service_resources::WaitReason::NativeLock),
                )
            },
            || with_database_admission(work),
        )
    });
    result.map_err(|error| interrupted_stop(scope.as_ref(), error))
}
/// An interrupted PDF slot wait is this owner's own stop, reported with the
/// same precedence as [`checkpoint`]: draining, then canceled, then stalled.
/// Owner flags are read directly because `checkpoint` waits while paused.
fn interrupted_stop(scope: Option<&ExecutionScope>, error: anyhow::Error) -> anyhow::Error {
    if !error
        .chain()
        .any(|cause| cause.is::<airk_ingest::pdf_slot::PdfSlotInterrupted>())
    {
        return error;
    }
    let owner = match scope {
        Some(scope) => scope.0.owner.get(),
        None => OWNER.get(),
    };
    let stop = if scope.is_some_and(ExecutionScope::draining) {
        anyhow::Error::new(Draining)
    } else if owner.is_some_and(|owner| owner.canceled.load(Ordering::Acquire)) {
        anyhow::Error::new(Canceled)
    } else if owner.is_some_and(|owner| owner.stalled.load(Ordering::Acquire)) {
        anyhow::Error::new(WorkStalled)
    } else {
        // The extraction pool interrupted itself: keep the original error.
        return error;
    };
    stop.context(format!("{error:#}"))
}
/// The owner of `scope` (or the process owner) must stop its accepted work.
fn owner_interrupted(scope: Option<&ExecutionScope>) -> bool {
    let stop = |owner: &Owner| {
        owner.canceled.load(Ordering::Acquire) || owner.stalled.load(Ordering::Acquire)
    };
    match scope {
        Some(scope) => scope.draining() || scope.0.owner.get().is_some_and(stop),
        None => OWNER.get().is_some_and(stop),
    }
}
pub(crate) fn plan(root: &Path) -> Result<Option<ExecutionPlan>> {
    checkpoint()?;
    let Some(owner_root) = with_owner(|owner| owner.root.clone()) else {
        return Ok(None);
    };
    ensure!(
        root.canonicalize()? == owner_root,
        "service grant storage root mismatch"
    );
    WORK.with(|slot| {
        let slot = slot.borrow();
        let work = slot.as_ref().context(crate::execution::CapacityBusy)?;

        Ok(Some(make_plan(&work.grant)))
    })
}
fn make_plan(grant: &WorkAllowance) -> ExecutionPlan {
    ExecutionPlan {
        mode: ExecutionMode::Automatic,
        logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
        detected_memory_bytes: crate::limits::machine::memory_bytes(),
        requested_extraction_workers: grant.extraction_threads as usize,
        extraction_workers: grant.extraction_threads as usize,
        requested_inference_workers: grant.inference_threads as usize,
        inference_workers: grant.inference_threads as usize,
        working_memory_bytes: grant.working_memory_bytes as usize,
        resource_slots: 0,
        coordination: "service-wide finite work grant; retained until completion or confirmed exit"
            .into(),
    }
}
/// Pace source bytes between bounded artifacts, never inside an atomic publication.
pub(crate) fn source_bytes(bytes: u64) -> Result<()> {
    checkpoint()?;
    let due = WORK.with(|slot| {
        let mut slot = slot.borrow_mut();
        let work = slot.as_mut()?;
        work.bytes = work.bytes.saturating_add(bytes);
        Some((
            work.started,
            Duration::from_secs_f64(
                work.bytes as f64 / work.grant.background_io_bytes_per_second as f64,
            ),
        ))
    });
    if let Some((started, due)) = due {
        while let Some(delay) = due.checked_sub(started.elapsed()) {
            checkpoint()?;
            std::thread::sleep(delay.min(Duration::from_millis(100)));
        }
    }
    Ok(())
}

pub(crate) fn status(root: &Path) -> Result<Option<ExecutionPlan>> {
    let Some(owner_root) = with_owner(|owner| owner.root.clone()) else {
        return Ok(None);
    };
    ensure!(
        root.canonicalize()? == owner_root,
        "service status storage root mismatch"
    );
    let active = WORK.with(|slot| slot.borrow().as_ref().map(|work| make_plan(&work.grant)));
    Ok(Some(active.unwrap_or(ExecutionPlan {
        mode: ExecutionMode::Automatic,
        logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
        detected_memory_bytes: crate::limits::machine::memory_bytes(),
        requested_extraction_workers: 0,
        extraction_workers: 0,
        requested_inference_workers: 0,
        inference_workers: 0,
        working_memory_bytes: 0,
        resource_slots: 0,
        coordination:
            "read-only request context: zero workers here are not service capacity; the service scheduler grants background work to jobs automatically".into(),
    })))
}
