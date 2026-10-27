// SPDX-License-Identifier: Apache-2.0
//! Process-bound service admission. Unscoped work cannot fall back to legacy root slots.
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
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
#[path = "service_execution_progress.rs"]
mod progress;
struct Owner {
    root: PathBuf,
    identity: WorkerIdentity,
    started: Instant,
    renewed: AtomicU64,
    active: Mutex<Option<airk_core::identity::Incarnation>>,
    paused: AtomicBool,
    canceled: AtomicBool,
    progress: Mutex<Option<progress::Tracker>>,
    stalled: AtomicBool,
    waiting: Mutex<Option<airk_core::service_resources::WaitReason>>,
}
static OWNER: OnceLock<Owner> = OnceLock::new();
fn observe(action: impl FnOnce(&mut progress::Tracker, Instant)) {
    if let Some(owner) = OWNER.get()
        && let Ok(mut progress) = owner.progress.lock()
        && let Some(progress) = progress.as_mut()
    {
        action(progress, Instant::now());
    }
}
pub fn progress() -> Option<airk_core::service_progress::WorkProgress> {
    OWNER
        .get()?
        .progress
        .lock()
        .ok()?
        .as_ref()
        .map(|progress| progress.snapshot(Instant::now()))
}
pub fn progress_phase(phase: airk_core::service_progress::WorkPhase) {
    progress_waiting(None);
    observe(|progress, now| progress.phase(phase, now));
}
/// Metadata only: a wait observation must never reset work advancement clocks.
pub fn progress_waiting(reason: Option<airk_core::service_resources::WaitReason>) {
    if let Some(owner) = OWNER.get()
        && let Ok(mut waiting) = owner.waiting.lock()
    {
        *waiting = reason;
    }
}
pub fn waiting_for() -> Option<airk_core::service_resources::WaitReason> {
    *OWNER.get()?.waiting.lock().ok()?
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
pub fn install(root: &Path, identity: WorkerIdentity) -> Result<()> {
    let root = root.canonicalize()?;
    ensure!(
        OWNER
            .set(Owner {
                root,
                identity,
                started: Instant::now(),
                renewed: AtomicU64::new(0),
                active: Mutex::new(None),
                paused: AtomicBool::new(false),
                canceled: AtomicBool::new(false),
                progress: Mutex::new(None),
                stalled: AtomicBool::new(false),
                waiting: Mutex::new(None),
            })
            .is_ok(),
        "service execution already installed"
    );
    Ok(())
}
pub fn activate(grant: &WorkAllowance) -> Result<()> {
    grant.validate()?;
    let owner = OWNER.get().context("service execution not installed")?;
    ensure!(owner.identity == grant.identity, "foreign work grant");
    *owner
        .active
        .lock()
        .map_err(|_| anyhow::anyhow!("work control unavailable"))? = Some(grant.id);
    owner.paused.store(false, Ordering::Release);
    owner.canceled.store(false, Ordering::Release);
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
    renew(grant.identity)
}
pub fn renew(identity: WorkerIdentity) -> Result<()> {
    let owner = OWNER.get().context("service execution not installed")?;
    ensure!(owner.identity == identity, "foreign work renewal");
    owner.renewed.store(
        owner.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        Ordering::Release,
    );
    Ok(())
}
pub fn control(
    identity: WorkerIdentity,
    id: airk_core::identity::Incarnation,
    paused: bool,
    canceled: bool,
) -> Result<()> {
    let owner = OWNER.get().context("service execution not installed")?;
    ensure!(
        owner.identity == identity
            && *owner
                .active
                .lock()
                .map_err(|_| anyhow::anyhow!("work control unavailable"))?
                == Some(id),
        "foreign work control"
    );
    owner.paused.store(paused, Ordering::Release);
    observe(|progress, now| progress.pause(paused, now));
    // Cancel is monotonic for a grant; a later pulse cannot revoke it.
    if canceled {
        owner.canceled.store(true, Ordering::Release);
    }
    Ok(())
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
/// True means the owned worker must exit after safe cancellation failed to drain.
pub fn watchdog() -> bool {
    let mut force = false;
    observe(|progress, now| {
        let (stalled, expired) = progress.watchdog(now);
        force = expired;
        if stalled && let Some(owner) = OWNER.get() {
            owner.stalled.store(true, Ordering::Release);
        }
    });
    force
}
pub fn checkpoint() -> Result<()> {
    let Some(owner) = OWNER.get() else {
        return Ok(());
    };
    let ttl = WORK.with(|slot| slot.borrow().as_ref().map(|work| work.grant.ttl_ms));
    let Some(ttl) = ttl else { return Ok(()) };
    loop {
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
}
pub(crate) fn installed() -> bool {
    OWNER.get().is_some()
}
pub fn run<T>(grant: WorkAllowance, work: impl FnOnce() -> Result<T>) -> Result<T> {
    grant.validate()?;
    ensure!(
        OWNER
            .get()
            .is_some_and(|owner| owner.identity == grant.identity),
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
    with_database_admission(work)
}
pub(crate) fn plan(root: &Path) -> Result<Option<ExecutionPlan>> {
    checkpoint()?;
    let Some(owner) = OWNER.get() else {
        return Ok(None);
    };
    ensure!(
        root.canonicalize()? == owner.root,
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
    let Some(owner) = OWNER.get() else {
        return Ok(None);
    };
    ensure!(
        root.canonicalize()? == owner.root,
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
            "service work grant required; no background work admitted on this thread".into(),
    })))
}
