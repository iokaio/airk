// SPDX-License-Identifier: Apache-2.0
//! One bounded in-process watcher, metadata-only delivery, and independent cached observations.
use crate::{
    source_protocol::{self as protocol, Command, Reply},
    worker_host::{OwnerThread, StopStep, WorkerHost},
    worker_transport::{FrameSink, FrameSource},
};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{ControlResponse, PROTOCOL_VERSION, ProjectBinding},
    service_watch::{WatchBackend, WatchFailure, WatchTraversal},
};
use airk_kernel::service_broker::new_instance_id;
use airk_store::service::{
    SourceWatchLease,
    registry::{ServiceRegistry, WatchProject},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};
mod checkpoint;
pub(crate) use checkpoint::SourceCheckpointObservation;

#[derive(Default)]
struct Replies {
    checkpoint: Option<checkpoint::Barrier>,
    alive: bool,
    pulse: Option<Instant>,
    directories: Option<u16>,
    projects: Option<u16>,
    pending: Option<u16>,
    observations: BTreeMap<ProjectBinding, WatchReport>,
    events: BTreeMap<ProjectBinding, (u64, u64)>,
    missing: BTreeMap<ProjectBinding, (u64, u64)>,
    removal_results: BTreeMap<ProjectBinding, (bool, bool)>,
    policy_acks: BTreeMap<ProjectBinding, (Incarnation, Incarnation, bool)>,
}

/// Native watcher evidence only. Wire callers cannot construct or deserialize
/// this event; the accepted metadata owner rechecks its current catalog binding.
pub(crate) struct NativeMissingSourceEvent {
    project: ProjectBinding,
    registration_epoch: u64,
    configuration_epoch: u64,
}
impl NativeMissingSourceEvent {
    pub(crate) fn binding(&self) -> ProjectBinding {
        self.project
    }
    pub(crate) fn registration_epoch(&self) -> u64 {
        self.registration_epoch
    }
    pub(crate) fn configuration_epoch(&self) -> u64 {
        self.configuration_epoch
    }
    fn observed(
        registry: &ServiceRegistry,
        project: ProjectBinding,
        registration_epoch: u64,
        configuration_epoch: u64,
    ) -> Result<Option<Self>> {
        if !registry.is_registered(project)? {
            return Ok(None);
        }
        let current = registry.watch(project)?;
        if current.project.fencing_generation != registration_epoch
            || current.configuration_epoch != configuration_epoch
            || !current.observation.enabled
        {
            return Ok(None);
        }
        match std::fs::symlink_metadata(&current.project.registration.root) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Some(Self {
                project,
                registration_epoch,
                configuration_epoch,
            })),
            Ok(_) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

/// Implementations retain only a weak metadata-actor sender. An unavailable or
/// full queue returns ownership so the bounded watcher can retry without loss.
pub(crate) trait MissingSourceReporter: Send + Sync {
    fn try_report(
        &self,
        event: NativeMissingSourceEvent,
    ) -> std::result::Result<(), NativeMissingSourceEvent>;
}

struct WatchReport {
    epoch: u64,
    native: u16,
    backend: WatchBackend,
    failure: Option<WatchFailure>,
    traversal: Option<WatchTraversal>,
}
enum Outgoing {
    Commands(Vec<Command>),
}

/// In-process watcher control frames queue like bytes in a child's pipes.
const WATCH_FRAMES: usize = 64;
/// The existing period a stopped watcher has to leave its loop on its own.
const WATCH_GRACE: Duration = Duration::from_secs(5);
/// After the existing 5-second stop period, the watcher's further time to
/// leave its loop; it has no native call that blocks longer than a turn. A
/// watcher still running after both is quarantined, never killed.
const WATCH_DRAIN: Duration = Duration::from_secs(20);
/// Diagnostic code of a watcher that did not stop within both periods.
const WATCH_QUARANTINE_CODE: &str = "watcher_quarantined_stop_deadline";

/// The one source watcher, on its own service thread. Its memory is bounded by
/// its validated directory, project and pending-event limits and is observed as
/// part of the service's resident set; it has no process of its own to kill.
struct WatchChild {
    process: WorkerHost,
    worker: Incarnation,
    sender: Option<SyncSender<Outgoing>>,
    writer: Option<std::thread::JoinHandle<()>>,
    reader: Option<std::thread::JoinHandle<()>>,
    replies: Arc<Mutex<Replies>>,
    started: Instant,
    pid: Arc<AtomicU32>,
}
/// Dropping a watcher never waits for it. Closing its command carrier is its
/// stop request; only a watcher whose exit was observed has its carrier
/// threads joined here. The coordinator keeps a stopping watcher in its
/// [`StoppingWatch`] until that exit is observed.
impl Drop for WatchChild {
    fn drop(&mut self) {
        self.sender.take();
        if matches!(self.process.try_wait(), Ok(Some(_))) {
            // The watcher returned, so both carriers are closed.
            if let Some(writer) = self.writer.take() {
                let _ = writer.join();
            }
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
        self.pid.store(0, Ordering::Release);
    }
}

/// A watcher asked to stop, kept (with the source-watch lease its own thread
/// holds) until its thread's exit is observed. It is never killed: past its
/// stop and drain periods it is quarantined and source watching stays
/// blocked, while the service keeps serving; a service restart recovers it.
struct StoppingWatch {
    child: WatchChild,
    requested: Instant,
    drain_requested: bool,
    quarantined: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WatchStop {
    Pending,
    /// First observation that the watcher cannot stop.
    Quarantined,
    Exited,
}
impl StoppingWatch {
    fn new(mut child: WatchChild, requested: Instant) -> Self {
        child.sender.take();
        Self {
            child,
            requested,
            drain_requested: false,
            quarantined: false,
        }
    }
    /// One step of the pure stop policy at `now`; it never waits.
    fn poll(&mut self, now: Instant) -> WatchStop {
        if matches!(self.child.process.try_wait(), Ok(Some(_))) {
            return WatchStop::Exited;
        }
        if self.quarantined {
            return WatchStop::Pending;
        }
        match crate::worker_host::stop_step(
            self.child.process.in_process(),
            self.requested,
            now,
            WATCH_GRACE,
            WATCH_DRAIN,
            self.drain_requested,
        ) {
            // The watcher always runs on a service thread; it is never killed.
            StopStep::Wait | StopStep::Kill => WatchStop::Pending,
            // The closed carrier already asks the loop to return.
            StopStep::Drain => {
                self.drain_requested = true;
                WatchStop::Pending
            }
            StopStep::Quarantine => {
                self.quarantined = true;
                WatchStop::Quarantined
            }
        }
    }
}
fn record_watch_quarantine(log: &crate::service_logs::Log) {
    log.record_code(
        airk_core::service_logs::LogKind::SourceWorkerFailed,
        None,
        None,
        Some(WATCH_QUARANTINE_CODE),
    );
    eprintln!(
        "airk: source watcher did not reach a safe point after its stop and drain periods; \
         it keeps its lease and source watching stays blocked; restart the service to recover"
    );
}
/// Validate and record one watcher's replies until its carrier closes.
fn observe_replies(
    mut output: Box<dyn FrameSource<Reply>>,
    observed: &Mutex<Replies>,
    ready: &SyncSender<Reply>,
    service: Incarnation,
    worker: Incarnation,
) {
    while let Ok(Some(reply)) = output.recv() {
        match reply {
            Reply::Checkpoint {
                service: reply_service,
                worker: reply_worker,
                operation_id,
                targets,
            } => {
                if reply_service != service || reply_worker != worker || targets.len() > 1024 {
                    break;
                }
                let mut projects = BTreeSet::new();
                if targets.iter().any(|target| {
                    target.sequence == 0
                        || target.sequence > i64::MAX as u64
                        || !projects.insert(target.project)
                }) {
                    break;
                }
                let Ok(mut observed) = observed.lock() else {
                    break;
                };
                if observed.checkpoint.is_some() {
                    break;
                }
                observed.checkpoint = Some(checkpoint::Barrier {
                    operation_id,
                    targets,
                });
            }
            Reply::PolicyFenced {
                service: reply_service,
                worker: reply_worker,
                project,
                operation_id,
                nonce,
            }
            | Reply::PolicyReleased {
                service: reply_service,
                worker: reply_worker,
                project,
                operation_id,
                nonce,
            } => {
                if reply_service != service || reply_worker != worker {
                    break;
                }
                let released = matches!(reply, Reply::PolicyReleased { .. });
                let Ok(mut observed) = observed.lock() else {
                    break;
                };
                if !observed.policy_acks.contains_key(&project)
                    && observed.policy_acks.len() >= 1024
                {
                    break;
                }
                observed
                    .policy_acks
                    .insert(project, (operation_id, nonce, released));
                observed.pulse = Some(Instant::now());
            }
            Reply::Ready { .. } | Reply::Activated => {
                if ready.try_send(reply).is_err() {
                    break;
                }
            }
            Reply::Pulse {
                watched_directories,
                projects,
                pending_events,
            } => {
                if !protocol::valid_pulse(watched_directories, projects, pending_events) {
                    break;
                }
                let Ok(mut observed) = observed.lock() else {
                    break;
                };
                observed.pulse = Some(Instant::now());
                observed.directories = Some(watched_directories);
                observed.projects = Some(projects);
                observed.pending = Some(pending_events);
            }
            Reply::RemovalResult {
                project,
                complete,
                failed,
            } => {
                let Ok(mut observed) = observed.lock() else {
                    break;
                };
                if !observed.removal_results.contains_key(&project)
                    && observed.removal_results.len() >= 1024
                {
                    break;
                }
                observed.removal_results.insert(project, (complete, failed));
                // Queued cleanup batches can delay the periodic
                // pulse; each valid result also proves liveness.
                observed.pulse = Some(Instant::now());
            }
            Reply::MissingSource {
                project,
                registration_epoch,
                configuration_epoch,
            } => {
                let Ok(mut observed) = observed.lock() else {
                    break;
                };
                if !observed.missing.contains_key(&project) && observed.missing.len() >= 1024 {
                    break;
                }
                observed
                    .missing
                    .insert(project, (registration_epoch, configuration_epoch));
            }
            Reply::Observation {
                project,
                configuration_epoch,
                native_roots,
                backend,
                failure,
                traversal,
            } => {
                if native_roots > 17
                    || traversal
                        .as_ref()
                        .is_some_and(|value| value.validate().is_err())
                {
                    break;
                }
                let Ok(mut observed) = observed.lock() else {
                    break;
                };
                if !observed.observations.contains_key(&project)
                    && observed.observations.len() >= 1024
                {
                    break;
                }
                if observed
                    .observations
                    .get(&project)
                    .is_none_or(|report| report.epoch <= configuration_epoch)
                {
                    observed.observations.insert(
                        project,
                        WatchReport {
                            epoch: configuration_epoch,
                            native: native_roots,
                            backend,
                            failure,
                            traversal,
                        },
                    );
                }
            }
            Reply::Event {
                project,
                configuration_epoch,
                sequence,
            } => {
                if sequence == 0 || sequence > i64::MAX as u64 {
                    break;
                }
                let Ok(mut observed) = observed.lock() else {
                    break;
                };
                if !observed.events.contains_key(&project) && observed.events.len() >= 1024 {
                    break;
                }
                if observed.events.get(&project).is_none_or(|(epoch, seq)| {
                    *epoch < configuration_epoch
                        || (*epoch == configuration_epoch && *seq <= sequence)
                }) {
                    observed
                        .events
                        .insert(project, (configuration_epoch, sequence));
                }
            }
        }
    }
    if let Ok(mut observed) = observed.lock() {
        observed.alive = false;
    }
}

impl WatchChild {
    fn start(root: &Path, service: Incarnation, pid: Arc<AtomicU32>) -> Result<Self> {
        // Unknown predecessor capacity stays reserved until its OS lease releases.
        drop(SourceWatchLease::acquire(root)?);
        let worker = new_instance_id()?;
        let (mut input, commands) = crate::worker_transport::channel::<Command>(WATCH_FRAMES);
        let (outbound, output) = crate::worker_transport::channel::<Reply>(WATCH_FRAMES);
        let init = protocol::Init {
            protocol: PROTOCOL_VERSION,
            version: env!("CARGO_PKG_VERSION").into(),
            runtime: root.into(),
            service,
            worker,
        };
        let process = WorkerHost::Thread(OwnerThread::new(
            std::thread::Builder::new()
                .name("airk-source-watch".into())
                .spawn(move || {
                    crate::source_worker::serve(
                        init,
                        Box::new(commands),
                        Box::new(outbound),
                        crate::worker_scope::Residency::Service(
                            crate::worker_scope::WorkerScope::new(),
                        ),
                    )
                })
                .context("start source watcher")?,
        ));
        let replies = Arc::new(Mutex::new(Replies {
            alive: true,
            ..Replies::default()
        }));
        let observed = replies.clone();
        let (ready, handshake) = mpsc::sync_channel(2);
        let mut child = Self {
            process,
            worker,
            sender: None,
            writer: None,
            reader: None,
            replies,
            started: Instant::now(),
            pid,
        };
        // Only a separate watcher process has resident memory of its own.
        child
            .pid
            .store(child.process.id().unwrap_or(0), Ordering::Release);
        child.reader = Some(
            std::thread::Builder::new()
                .name("airk-watch-replies".into())
                .spawn(move || {
                    observe_replies(Box::new(output), &observed, &ready, service, worker)
                })?,
        );
        let (sender, receiver) = mpsc::sync_channel::<Outgoing>(8);
        child.writer = Some(
            std::thread::Builder::new()
                .name("airk-watch-commands".into())
                .spawn(move || {
                    while let Ok(batch) = receiver.recv() {
                        match batch {
                            Outgoing::Commands(commands) => {
                                for command in commands {
                                    if input.send(command).is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                })?,
        );
        child.sender = Some(sender);
        // The watcher already holds its Init; its first reply is Ready.
        ensure!(
            matches!(handshake.recv_timeout(Duration::from_secs(10))?,Reply::Ready {service:ready_service,worker:ready_worker} if ready_service==service && ready_worker==worker),
            "watcher owner identity mismatch"
        );
        child.send(vec![Command::Activate { service, worker }])?;
        ensure!(
            matches!(
                handshake.recv_timeout(Duration::from_secs(10))?,
                Reply::Activated
            ),
            "watcher activation failed"
        );
        Ok(child)
    }
    /// The process that runs the watcher: the service itself when in process.
    fn host_process_id(&self) -> u32 {
        self.process.id().unwrap_or_else(std::process::id)
    }
    fn send(&self, commands: Vec<Command>) -> Result<()> {
        ensure!(
            commands.len() <= 20 && serde_json::to_vec(&commands)?.len() <= 128 * 1024,
            "watcher command batch exceeds budget"
        );
        self.sender
            .as_ref()
            .context("watcher writer closed")?
            .try_send(Outgoing::Commands(commands))
            .map_err(|_| anyhow::anyhow!("watcher command queue busy"))?;
        Ok(())
    }
}

/// Only this coordinator owns child handles and may qualify an acknowledgement
/// or prove native absence with the actual source lease. PID caches are unused.
fn coordinate_policy_fences(
    root: &Path,
    service: Incarnation,
    child: Option<&WatchChild>,
    fences: &PolicyFences,
) -> Result<bool> {
    let mut entries = fences
        .entries
        .lock()
        .map_err(|_| anyhow::anyhow!("source policy fence unavailable"))?;
    if entries.is_empty() {
        return Ok(false);
    }
    let mut released = false;
    if let Some(child) = child {
        let acknowledgements = {
            let mut replies = child
                .replies
                .lock()
                .map_err(|_| anyhow::anyhow!("source policy acknowledgement unavailable"))?;
            std::mem::take(&mut replies.policy_acks)
        };
        for (project, (operation_id, nonce, was_released)) in acknowledgements {
            let Some(current) = entries.get_mut(&project) else {
                continue;
            };
            if !current.matches_ack(child.worker, operation_id, nonce, was_released) {
                continue;
            }
            if was_released {
                entries.remove(&project);
                released = true;
            } else {
                current.acknowledged = true;
            }
        }
        let commands = entries
            .iter()
            .filter(|(_, fence)| fence.sent != Some((child.worker, fence.release_requested)))
            .take(20)
            .map(|(project, fence)| {
                let command = if fence.release_requested {
                    Command::ReleasePolicy {
                        service,
                        worker: child.worker,
                        project: *project,
                        operation_id: fence.operation_id,
                        nonce: fence.nonce,
                    }
                } else {
                    Command::FencePolicy {
                        service,
                        worker: child.worker,
                        project: *project,
                        operation_id: fence.operation_id,
                        nonce: fence.nonce,
                    }
                };
                (*project, fence.release_requested, command)
            })
            .collect::<Vec<_>>();
        if !commands.is_empty() {
            let projects = commands
                .iter()
                .map(|(project, release, _)| (*project, *release))
                .collect::<Vec<_>>();
            child.send(
                commands
                    .into_iter()
                    .map(|(_, _, command)| command)
                    .collect(),
            )?;
            for (project, release) in projects {
                if let Some(fence) = entries.get_mut(&project) {
                    fence.sent = Some((child.worker, release));
                }
            }
        }
    } else if let Ok(_absence) = SourceWatchLease::acquire(root) {
        entries.retain(|_, fence| {
            if fence.release_requested {
                released = true;
                false
            } else {
                fence.acknowledged = true;
                fence.sent = None;
                true
            }
        });
    }
    fences.changed.notify_all();
    Ok(released)
}

#[derive(Default)]
struct Cache {
    available: bool,
    process_id: Option<u32>,
    directories: Option<u16>,
    projects: Option<u16>,
    pending: Option<u16>,
    observed: Option<Instant>,
    blocked: bool,
    predecessor: bool,
    coordinated: Option<Instant>,
    configuration_revision: u64,
}
#[derive(Clone)]
pub(crate) struct SourceWake {
    sender: Weak<SyncSender<()>>,
    revision: Arc<AtomicU64>,
    idle: Arc<AtomicBool>,
    policy_fences: Arc<PolicyFences>,
}

struct PolicyFence {
    operation_id: Incarnation,
    nonce: Incarnation,
    acknowledged: bool,
    release_requested: bool,
    sent: Option<(Incarnation, bool)>,
}
impl PolicyFence {
    fn matches_ack(
        &self,
        worker: Incarnation,
        operation_id: Incarnation,
        nonce: Incarnation,
        released: bool,
    ) -> bool {
        self.operation_id == operation_id
            && self.nonce == nonce
            && self.sent == Some((worker, released))
            && (!released || self.release_requested)
    }
}
#[derive(Default)]
struct PolicyFences {
    entries: Mutex<BTreeMap<ProjectBinding, PolicyFence>>,
    changed: Condvar,
    closing: AtomicBool,
}
impl PolicyFences {
    fn blocked(&self, project: ProjectBinding) -> bool {
        self.entries
            .lock()
            .map_or(true, |entries| entries.contains_key(&project))
    }
}
impl SourceWake {
    pub(crate) fn changed(&self) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        self.idle.store(false, Ordering::Release);
        // A queued wake already covers this revision. The coordinator also checks
        // the revision when publishing an idle observation, closing the scan race.
        if let Some(sender) = self.sender.upgrade() {
            let _ = sender.try_send(());
        }
    }
    /// Close configuration admission before waiting for the exact native source
    /// owner to discard its retained traversal plan. Timeout retains the fence.
    pub(crate) fn fence_policy(
        &self,
        project: ProjectBinding,
        operation_id: Incarnation,
    ) -> Result<()> {
        ensure!(
            !self.policy_fences.closing.load(Ordering::Acquire),
            "source owner is closing"
        );
        let mut entries = self
            .policy_fences
            .entries
            .lock()
            .map_err(|_| anyhow::anyhow!("source policy fence unavailable"))?;
        if let Some(existing) = entries.get(&project) {
            ensure!(
                existing.operation_id == operation_id && !existing.release_requested,
                "source policy fence belongs to another operation"
            );
        } else {
            ensure!(
                entries.len() < 1024,
                "source policy fence capacity exhausted"
            );
            entries.insert(
                project,
                PolicyFence {
                    operation_id,
                    nonce: new_instance_id()?,
                    acknowledged: false,
                    release_requested: false,
                    sent: None,
                },
            );
        }
        drop(entries);
        self.changed();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut entries = self
            .policy_fences
            .entries
            .lock()
            .map_err(|_| anyhow::anyhow!("source policy fence unavailable"))?;
        loop {
            ensure!(
                !self.policy_fences.closing.load(Ordering::Acquire),
                "source owner closed before policy acknowledgement"
            );
            let current = entries
                .get(&project)
                .context("source policy fence disappeared")?;
            ensure!(
                current.operation_id == operation_id && !current.release_requested,
                "source policy fence changed"
            );
            if current.acknowledged {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            ensure!(
                !remaining.is_zero(),
                "source policy fence acknowledgement is uncertain; exclusion remains closed"
            );
            (entries, _) = self
                .policy_fences
                .changed
                .wait_timeout(entries, remaining)
                .map_err(|_| anyhow::anyhow!("source policy fence unavailable"))?;
        }
    }
    /// Release only an exact completed/no-effect native operation. No saved
    /// source binding is replayed: the coordinator observes the registry anew.
    pub(crate) fn release_policy(
        &self,
        project: ProjectBinding,
        operation_id: Incarnation,
    ) -> Result<()> {
        ensure!(
            !self.policy_fences.closing.load(Ordering::Acquire),
            "source owner is closing"
        );
        let mut entries = self
            .policy_fences
            .entries
            .lock()
            .map_err(|_| anyhow::anyhow!("source policy fence unavailable"))?;
        let Some(current) = entries.get_mut(&project) else {
            return Ok(());
        };
        ensure!(
            current.operation_id == operation_id,
            "source policy release belongs to another operation"
        );
        current.release_requested = true;
        drop(entries);
        self.changed();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut entries = self
            .policy_fences
            .entries
            .lock()
            .map_err(|_| anyhow::anyhow!("source policy fence unavailable"))?;
        loop {
            ensure!(
                !self.policy_fences.closing.load(Ordering::Acquire),
                "source owner closed before policy release"
            );
            let Some(current) = entries.get(&project) else {
                return Ok(());
            };
            ensure!(
                current.operation_id == operation_id && current.release_requested,
                "source policy release changed"
            );
            let remaining = deadline.saturating_duration_since(Instant::now());
            ensure!(
                !remaining.is_zero(),
                "source policy release acknowledgement is uncertain"
            );
            (entries, _) = self
                .policy_fences
                .changed
                .wait_timeout(entries, remaining)
                .map_err(|_| anyhow::anyhow!("source policy fence unavailable"))?;
        }
    }
}
pub struct SourceSupervisor {
    checkpoint_admission_observed: Arc<AtomicBool>,
    checkpoint_seal: Arc<AtomicBool>,
    checkpoint_operation: Arc<Mutex<Option<Incarnation>>>,
    checkpoint_result: Arc<Mutex<Option<SourceCheckpointObservation>>>,
    checkpoint_error: Arc<Mutex<Option<String>>>,
    checkpoint_closed: Arc<AtomicBool>,
    checkpoint_observed: Arc<AtomicBool>,
    sender: Option<Arc<SyncSender<()>>>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub process_id: Arc<AtomicU32>,
    pub telemetry_complete: Arc<AtomicBool>,
    cache: Arc<Mutex<Cache>>,
    idle_safe: Arc<AtomicBool>,
    configuration_revision: Arc<AtomicU64>,
    missing_reporter: Arc<Mutex<Option<Arc<dyn MissingSourceReporter>>>>,
    policy_fences: Arc<PolicyFences>,
}
impl Drop for SourceSupervisor {
    fn drop(&mut self) {
        self.policy_fences.closing.store(true, Ordering::Release);
        self.policy_fences.changed.notify_all();
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl SourceSupervisor {
    pub fn start(
        root: PathBuf,
        service: Incarnation,
        log: crate::service_logs::Log,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let checkpoint_closed = Arc::new(AtomicBool::new(false));
        let checkpoint_observed = Arc::new(AtomicBool::new(false));
        let checkpoint_pause = checkpoint_closed.clone();
        let checkpoint_ack = checkpoint_observed.clone();
        let checkpoint_admission_observed = Arc::new(AtomicBool::new(false));
        let checkpoint_admission_ack = checkpoint_admission_observed.clone();
        let checkpoint_seal = Arc::new(AtomicBool::new(false));
        let seal_source = checkpoint_seal.clone();
        let checkpoint_operation = Arc::new(Mutex::new(None::<Incarnation>));
        let source_operation = checkpoint_operation.clone();
        let checkpoint_result = Arc::new(Mutex::new(None));
        let source_result = checkpoint_result.clone();
        let checkpoint_error = Arc::new(Mutex::new(None));
        let source_error = checkpoint_error.clone();
        let recovery_fenced = crate::checkpoint_fence::admission_closed(&root)?;
        let process_id = Arc::new(AtomicU32::new(0));
        let pid = process_id.clone();
        let telemetry_complete = Arc::new(AtomicBool::new(false));
        let telemetry = telemetry_complete.clone();
        let cache = Arc::new(Mutex::new(Cache::default()));
        let state = cache.clone();
        let idle_safe = Arc::new(AtomicBool::new(false));
        let idle = idle_safe.clone();
        let configuration_revision = Arc::new(AtomicU64::new(0));
        let configuration = configuration_revision.clone();
        let missing_reporter = Arc::new(Mutex::new(None::<Arc<dyn MissingSourceReporter>>));
        let report_missing = missing_reporter.clone();
        let policy_fences = Arc::new(PolicyFences::default());
        let fences = policy_fences.clone();
        let thread = std::thread::Builder::new()
            .name("airk-source-coordinator".into())
            .spawn(move || {
                // The metadata owner releases this gate after its first registry
                // open. Cold schema creation must not race a second adapter.
                if receiver.recv().is_err() {
                    return;
                }
                let mut child: Option<WatchChild> = None;
                // A previous watcher not yet observed to exit. No new watcher
                // starts while it holds the source-watch lease.
                let mut stopping: Option<StoppingWatch> = None;
                let mut registry = None;
                let mut after = None;
                let mut removal_after = None;
                let mut removals_pending = false;
                let mut failed_removals = BTreeSet::new();
                let mut sent = BTreeMap::<ProjectBinding, (u64, Option<Sha256Digest>)>::new();
                let mut periodic = BTreeMap::<ProjectBinding, Instant>::new();
                let mut enabled = BTreeSet::new();
                let mut seen = BTreeSet::new();
                let mut scanned = false;
                let mut next_page = Instant::now();
                let mut attempts = 0u32;
                let mut retry = Instant::now();
                let mut source_checkpoint=checkpoint::Progress::default();
                loop {
                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Ok(()) => {
                            after = None;
                            seen.clear();
                            scanned = false;
                            next_page = Instant::now();
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    match stopping.as_mut().map(|old| old.poll(Instant::now())) {
                        Some(WatchStop::Quarantined) => record_watch_quarantine(&log),
                        // Its exit is observed: dropping it joins its carriers.
                        Some(WatchStop::Exited) => stopping = None,
                        Some(WatchStop::Pending) | None => {}
                    }
                    // A source checkpoint needs the watcher's exit proof or the
                    // free source-watch lease; it waits while one is stopping.
                    if seal_source.load(Ordering::Acquire) && stopping.is_some() {
                        continue;
                    }
                    if seal_source.load(Ordering::Acquire) {
                        let operation=source_operation.lock().ok().and_then(|value|*value);
                        let result=operation.context("source checkpoint operation missing").and_then(|operation|checkpoint::step(&root,service,operation,&mut child,&mut registry,&mut source_checkpoint));
                        match result {
                            Ok(checkpoint::Step::Complete(observation))=>{
                                if let Ok(mut result)=source_result.lock(){*result=Some(observation);}
                                checkpoint_ack.store(true,Ordering::Release);
                                continue;
                            },
                            Ok(checkpoint::Step::Pending)=>continue,
                            Ok(checkpoint::Step::ContinueAcceptedCleanup)=>{},
                            Err(error)=>{
                                checkpoint_ack.store(false,Ordering::Release);
                                if let Ok(mut saved)=source_error.lock()
                                    && saved.is_none() {
                                    *saved=Some(format!("{error:#}").chars().take(2048).collect());
                                }
                                continue;
                            },
                        }
                    }
                    let configuration_snapshot = configuration.load(Ordering::Acquire);
                    if child.is_none() {
                        // This service's own stopping watcher holds the lease;
                        // its memory is part of the service's resident set, so
                        // telemetry stays complete while it drains.
                        let own = stopping.is_some();
                        let predecessor = own || SourceWatchLease::acquire(&root).is_err();
                        telemetry.store(own || !predecessor, Ordering::Release);
                        if let Ok(mut cache) = state.lock() {
                            cache.predecessor = predecessor;
                        }
                    } else {
                        telemetry.store(true, Ordering::Release);
                    }
                    if child.as_mut().is_some_and(|child| {
                        matches!(child.process.try_wait(), Ok(Some(_)))
                            || child.replies.lock().is_ok_and(|replies| {
                                !replies.alive
                                    || replies.pulse.unwrap_or(child.started).elapsed()
                                        > Duration::from_secs(15)
                            })
                            || child
                                .process
                                .id()
                                .and_then(airk_kernel::service_resource_observations::rss)
                                .is_some_and(|bytes| {
                                    bytes > airk_kernel::source_watch::memory_allowance()
                                })
                    }) {
                        log.record(
                            airk_core::service_logs::LogKind::SourceWorkerFailed,
                            None,
                            None,
                        );
                        // Never wait for it here: it stops on its own thread and
                        // the poll above reaps it, or quarantines it, later.
                        if let Some(failed) = child.take() {
                            stopping = Some(StoppingWatch::new(failed, Instant::now()));
                        }
                        pid.store(0, Ordering::Release);
                        sent.clear();
                        attempts = attempts.saturating_add(1);
                        retry = Instant::now() + Duration::from_secs(1 << attempts.min(6));
                    }
                    if coordinate_policy_fences(&root, service, child.as_ref(), &fences).unwrap_or(false) {
                        after = None;
                        seen.clear();
                        scanned = false;
                        next_page = Instant::now();
                        // Released bindings are reconstructed from current
                        // registry metadata, never replayed from this cache.
                        sent.retain(|project, _| !fences.blocked(*project));
                    }
                    if let Ok(entries) = fences.entries.lock() {
                        for project in entries.keys() {
                            sent.remove(project);
                            periodic.remove(project);
                            enabled.remove(project);
                        }
                    }
                    // Keep exact already accepted policy-fence coordination
                    // above alive, but create no new source observations/work.
                    if recovery_fenced && crate::checkpoint_fence::admission_closed(&root).unwrap_or(true) {
                        checkpoint_admission_ack.store(true,Ordering::Release);
                        checkpoint_ack.store(true, Ordering::Release);
                        continue;
                    }
                    if checkpoint_pause.load(Ordering::Acquire) {checkpoint_admission_ack.store(true,Ordering::Release);}
                    checkpoint_ack.store(false, Ordering::Release);
                    if registry.is_none() {
                        registry = ServiceRegistry::open(&root, service).ok();
                    }
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .and_then(|time| u64::try_from(time.as_millis()).ok());
                    if let (Some(registry), Some(now_ms)) = (registry.as_mut(), now_ms) {
                        if let Some(active) = child.as_ref() {
                            if let Ok(mut replies) = active.replies.lock() {
                                let results = std::mem::take(&mut replies.removal_results);
                                for (project, (complete, failed)) in results {
                                    if complete { failed_removals.remove(&project); log.record(airk_core::service_logs::LogKind::ProjectRemovalCompleted, Some(project), None); }
                                    else if failed && failed_removals.insert(project) { log.record(airk_core::service_logs::LogKind::ProjectRemovalFailed, Some(project), None); }
                                    else if !failed { failed_removals.remove(&project); }
                                }
                            }
                            let (observations, events, missing) = if let Ok(mut replies) =
                                active.replies.lock()
                            {
                                let observations = replies
                                    .observations
                                    .keys()
                                    .take(16)
                                    .copied()
                                    .collect::<Vec<_>>()
                                    .into_iter()
                                    .filter_map(|id| {
                                        replies.observations.remove(&id).map(|value| (id, value))
                                    })
                                    .collect::<Vec<_>>();
                                let events = replies
                                    .events
                                    .keys()
                                    .take(16)
                                    .copied()
                                    .collect::<Vec<_>>()
                                    .into_iter()
                                    .filter_map(|id| {
                                        replies.events.remove(&id).map(|value| (id, value))
                                    })
                                    .collect::<Vec<_>>();
                                let missing = replies
                                    .missing
                                    .keys()
                                    .take(16)
                                    .copied()
                                    .collect::<Vec<_>>()
                                    .into_iter()
                                    .filter_map(|id| {
                                        replies.missing.remove(&id).map(|value| (id, value))
                                    })
                                    .collect::<Vec<_>>();
                                (observations, events, missing)
                            } else {
                                (vec![], vec![], vec![])
                            };
                            for (project, (registration_epoch, configuration_epoch)) in missing {
                                if checkpoint_pause.load(Ordering::Acquire){continue;}
                                if fences.blocked(project) { continue; }
                                match registry.catalog_state() {
                                    Ok(Some(catalog)) if catalog.authority_contract == 1 => {
                                        let event = NativeMissingSourceEvent::observed(registry,
                                            project, registration_epoch, configuration_epoch);
                                        match event {
                                            Ok(Some(event)) => {
                                                let reporter = report_missing.lock().ok()
                                                    .and_then(|value| value.clone());
                                                let retry = match reporter {
                                                    Some(reporter) => reporter.try_report(event).is_err(),
                                                    None => true,
                                                };
                                                if retry && let Ok(mut replies) = active.replies.lock() {
                                                    if replies.missing.contains_key(&project) || replies.missing.len() < 1024 {
                                                        replies.missing.entry(project).or_insert((registration_epoch, configuration_epoch));
                                                    } else {
                                                        log.record(airk_core::service_logs::LogKind::MetadataUnavailable, Some(project), None);
                                                    }
                                                }
                                            }
                                            Ok(None) => (),
                                            Err(_) => log.record(airk_core::service_logs::LogKind::MetadataUnavailable, Some(project), None),
                                        }
                                        continue;
                                    }
                                    Ok(_) => (),
                                    Err(_) => {
                                        log.record(airk_core::service_logs::LogKind::MetadataUnavailable, Some(project), None);
                                        continue;
                                    }
                                }
                                match registry.retire_missing_source(
                                    project,
                                    registration_epoch,
                                    configuration_epoch,
                                ) {
                                    Ok(true) => {
                                        enabled.remove(&project);
                                        periodic.remove(&project);
                                        sent.remove(&project);
                                        let _ = active.send(vec![Command::Remove {
                                            project,
                                            configuration_epoch,
                                        }]);
                                        log.record(
                                            airk_core::service_logs::LogKind::SourceProjectRemoved,
                                            Some(project),
                                            None,
                                        );
                                    }
                                    Ok(false) => (),
                                    Err(_) => log.record(
                                        airk_core::service_logs::LogKind::MetadataUnavailable,
                                        Some(project),
                                        None,
                                    ),
                                }
                            }
                            for (
                                project,
                                WatchReport {
                                    epoch,
                                    native,
                                    backend,
                                    failure,
                                    traversal,
                                },
                            ) in observations
                            {
                                if fences.blocked(project) { continue; }
                                retry_unavailable_configuration(&mut sent, project, epoch, failure);
                                if registry
                                    .record_watch_observation(
                                        project, epoch, native, backend, failure, now_ms,
                                    )
                                    .is_ok()
                                    && let Some(traversal) = traversal
                                    && let Ok(Some(unavailable)) = registry
                                        .record_watch_traversal(project, epoch, &traversal, now_ms)
                                {
                                    use airk_core::service_logs::LogKind;
                                    log.record(
                                        if unavailable {
                                            LogKind::SourceTraversalUnavailable
                                        } else {
                                            LogKind::SourceTraversalRecovered
                                        },
                                        Some(project),
                                        None,
                                    );
                                }
                            }
                            for (project, (epoch, sequence)) in events {
                                if fences.blocked(project) { continue; }
                                if registry
                                    .source_event(project, epoch, active.worker, sequence)
                                    .is_ok()
                                {
                                    let _ = active.send(vec![Command::Acknowledge {
                                        project,
                                        configuration_epoch: epoch,
                                        sequence,
                                    }]);
                                }
                            }
                        }
                        if Instant::now() >= next_page {
                            match registry.pending_project_removals(removal_after, 16) {
                                Ok(removals) => {
                                    removals_pending = registry.pending_project_removals(None, 1)
                                        .map_or(true, |pending| !pending.is_empty());
                                    if removals.is_empty() {
                                        removal_after = None;
                                    }
                                    for registration in removals {
                                        if child.is_none()
                                            && stopping.is_none()
                                            && attempts < 3
                                            && Instant::now() >= retry
                                            && let Ok(active) = WatchChild::start(&root, service, pid.clone())
                                        {
                                            pid.store(active.process.id().unwrap_or(0), Ordering::Release);
                                            child = Some(active);
                                            sent.clear();
                                        }
                                        if let Some(active) = child.as_ref() {
                                            if active
                                                .send(vec![
                                                    Command::CleanupRemoved {
                                                        project: registration.binding,
                                                    },
                                                ])
                                                .is_err()
                                            {
                                                break;
                                            }
                                            removal_after = Some(registration.binding);
                                        }
                                    }
                                }
                                Err(_) => {
                                    removals_pending = true;
                                }
                            }
                        }
                        if Instant::now() >= next_page
                            && let Ok(projects) = registry.watch_projects(after, 32)
                        {
                            let mut complete = true;
                            for project in projects {
                                let id = project.project.registration.binding;
                                seen.insert(id);
                                if fences.blocked(id) {
                                    enabled.remove(&id);
                                    periodic.remove(&id);
                                    sent.remove(&id);
                                    after = Some(id);
                                    continue;
                                }
                                if project.observation.enabled {
                                    enabled.insert(id);
                                    if periodic.get(&id).is_none_or(|time| {
                                        time.elapsed() >= Duration::from_secs(300)
                                    }) && ((!periodic.contains_key(&id)
                                        && project.observation.requested_revision
                                            > project.observation.completed_revision)
                                        || registry
                                            .source_changed(id, project.configuration_epoch)
                                            .is_ok())
                                    {
                                        periodic.insert(id, Instant::now());
                                    }
                                } else {
                                    enabled.remove(&id);
                                    periodic.remove(&id);
                                }
                                if child.is_none()
                                    && stopping.is_none()
                                    && attempts < 3
                                    && Instant::now() >= retry
                                {
                                    let predecessor = SourceWatchLease::acquire(&root).is_err();
                                    if !predecessor
                                        && registry.reset_native_watch_observations().is_ok()
                                    {
                                        match WatchChild::start(&root, service, pid.clone()) {
                                            Ok(active) => {
                                                pid.store(active.process.id().unwrap_or(0), Ordering::Release);
                                                child = Some(active);
                                                sent.clear();
                                            }
                                            Err(_) => {
                                                attempts += 1;
                                                retry = Instant::now()
                                                    + Duration::from_secs(1 << attempts.min(6));
                                            }
                                        }
                                    }
                                    if let Ok(mut cache) = state.lock() {
                                        cache.predecessor = predecessor;
                                    }
                                }
                                if let Some(active) = child.as_ref() {
                                    let Ok(policy_guard) = fences.entries.lock() else {
                                        complete = false;
                                        break;
                                    };
                                    if policy_guard.contains_key(&id) {
                                        sent.remove(&id);
                                        after = Some(id);
                                        continue;
                                    }
                                    if active
                                        .send(vec![Command::CheckSource {
                                            registration: project.project.registration.clone(),
                                            registration_epoch: project.project.fencing_generation,
                                            configuration_epoch: project.configuration_epoch,
                                        }])
                                        .and_then(|()| {
                                            send_configuration(active, &project, &mut sent)
                                        })
                                        .is_err()
                                    {
                                        complete = false;
                                        break;
                                    }
                                } else if project.observation.enabled {
                                    let _ = registry.record_watch_observation(
                                        id,
                                        project.configuration_epoch,
                                        0,
                                        WatchBackend::PeriodicFallback,
                                        Some(WatchFailure::BackendUnavailable),
                                        now_ms,
                                    );
                                }
                                after = Some(id);
                            }
                            if complete {
                                next_page = Instant::now() + Duration::from_millis(100);
                            }
                            if registry
                                .watch_projects(after, 1)
                                .is_ok_and(|page| page.is_empty())
                            {
                                enabled.retain(|id| seen.contains(id));
                                periodic.retain(|id, _| seen.contains(id));
                                seen.clear();
                                scanned = true;
                                after = None;
                                next_page = Instant::now() + Duration::from_secs(10);
                            }
                        }
                    }
                    if let Ok(mut cache) = state.lock() {
                        cache.coordinated = Some(Instant::now());
                        cache.configuration_revision = configuration_snapshot;
                        let no_background_intent =
                            scanned && enabled.is_empty() && !removals_pending
                                && fences.entries.lock().is_ok_and(|entries| entries.is_empty());
                        // An exhausted restart budget and a quarantined
                        // watcher both block it; the log record names which.
                        cache.blocked = attempts >= 3
                            || stopping.as_ref().is_some_and(|old| old.quarantined);
                        cache.available = false;
                        if let Some(active) = child.as_ref()
                            && let Ok(replies) = active.replies.lock()
                        {
                            cache.available = replies.alive
                                && replies
                                    .pulse
                                    .is_some_and(|pulse| pulse.elapsed() < Duration::from_secs(15));
                            cache.process_id = Some(active.host_process_id());
                            cache.directories = replies.directories;
                            cache.projects = replies.projects;
                            cache.pending = replies.pending;
                            cache.observed = replies.pulse;
                            idle.store(
                                no_background_intent
                                    && replies.events.is_empty()
                                    && replies.missing.is_empty()
                                    && replies.pending == Some(0),
                                Ordering::Release,
                            );
                        } else {
                            cache.process_id = None;
                            idle.store(no_background_intent, Ordering::Release);
                        }
                    }
                }
                // Shutdown waits for the watcher's own exit for at most its stop
                // and drain periods. A watcher still running then is quarantined
                // and left to the service's exit, which releases its lease.
                let mut remaining = stopping.into_iter().collect::<Vec<_>>();
                if let Some(active) = child.take() {
                    remaining.push(StoppingWatch::new(active, Instant::now()));
                }
                while !remaining.is_empty() {
                    let now = Instant::now();
                    remaining.retain_mut(|old| match old.poll(now) {
                        WatchStop::Exited => false,
                        WatchStop::Quarantined => {
                            record_watch_quarantine(&log);
                            true
                        }
                        WatchStop::Pending => true,
                    });
                    if remaining.iter().all(|old| old.quarantined) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                drop(remaining);
                pid.store(0, Ordering::Release);
            })?;
        Ok(Self {
            checkpoint_admission_observed,
            checkpoint_seal,
            checkpoint_operation,
            checkpoint_result,
            checkpoint_error,
            checkpoint_closed,
            checkpoint_observed,
            sender: Some(Arc::new(sender)),
            thread: Some(thread),
            process_id,
            telemetry_complete,
            cache,
            idle_safe,
            configuration_revision,
            missing_reporter,
            policy_fences,
        })
    }
    pub(crate) fn set_missing_reporter(
        &self,
        reporter: Arc<dyn MissingSourceReporter>,
    ) -> Result<()> {
        let mut current = self
            .missing_reporter
            .lock()
            .map_err(|_| anyhow::anyhow!("missing-source metadata port unavailable"))?;
        ensure!(
            current.is_none(),
            "missing-source metadata port is already bound"
        );
        *current = Some(reporter);
        Ok(())
    }
    pub(crate) fn begin_checkpoint(&self, operation: Incarnation) -> Result<()> {
        let mut saved = self
            .checkpoint_operation
            .lock()
            .map_err(|_| anyhow::anyhow!("source checkpoint identity unavailable"))?;
        ensure!(
            saved.is_none_or(|saved| saved == operation),
            "another source checkpoint owns admission"
        );
        *saved = Some(operation);
        if !self.checkpoint_closed.swap(true, Ordering::AcqRel) {
            self.checkpoint_observed.store(false, Ordering::Release);
        }
        Ok(())
    }
    pub(crate) fn checkpoint_admission_observed(&self) -> bool {
        self.checkpoint_admission_observed.load(Ordering::Acquire)
    }
    pub(crate) fn seal_checkpoint(&self, operation: Incarnation) -> Result<()> {
        ensure!(
            *self
                .checkpoint_operation
                .lock()
                .map_err(|_| anyhow::anyhow!("source checkpoint identity unavailable"))?
                == Some(operation),
            "source checkpoint seal identity changed"
        );
        self.checkpoint_seal.store(true, Ordering::Release);
        Ok(())
    }
    pub(crate) fn checkpoint_result(&self) -> Result<Option<SourceCheckpointObservation>> {
        if let Some(error) = self
            .checkpoint_error
            .lock()
            .map_err(|_| anyhow::anyhow!("source checkpoint failure unavailable"))?
            .as_ref()
        {
            anyhow::bail!("{error}");
        }
        let result = self
            .checkpoint_result
            .lock()
            .map_err(|_| anyhow::anyhow!("source checkpoint receipt unavailable"))?
            .clone();
        if let Some(result) = &result {
            result.validate()?;
        }
        Ok(result)
    }
    pub(crate) fn checkpoint_observed(&self) -> bool {
        self.checkpoint_observed.load(Ordering::Acquire)
    }
    pub fn can_idle_exit(&self) -> bool {
        self.idle_safe.load(Ordering::Acquire)
            && self.cache.lock().is_ok_and(|cache| {
                cache.configuration_revision == self.configuration_revision.load(Ordering::Acquire)
                    && cache
                        .coordinated
                        .is_some_and(|time| time.elapsed() < Duration::from_secs(2))
            })
    }
    pub(crate) fn wake_handle(&self) -> SourceWake {
        SourceWake {
            sender: Arc::downgrade(self.sender.as_ref().expect("active source supervisor")),
            revision: self.configuration_revision.clone(),
            idle: self.idle_safe.clone(),
            policy_fences: self.policy_fences.clone(),
        }
    }
    pub fn status(&self) -> ControlResponse {
        match self.cache.lock() {
            Ok(cache) => ControlResponse::SourceStatus {
                available: cache.available
                    && cache
                        .observed
                        .is_some_and(|time| time.elapsed() < Duration::from_secs(15)),
                process_id: cache.process_id,
                watched_directories: cache.directories,
                projects: cache.projects,
                pending_events: cache.pending,
                observation_age_ms: cache
                    .observed
                    .map(|time| time.elapsed().as_millis().min(u64::MAX as u128) as u64),
                blocked: cache.blocked,
                predecessor_draining: cache.predecessor,
            },
            Err(_) => ControlResponse::Failure {
                code: "source_observation_unavailable".into(),
            },
        }
    }
}

fn send_configuration(
    child: &WatchChild,
    project: &WatchProject,
    sent: &mut BTreeMap<ProjectBinding, (u64, Option<Sha256Digest>)>,
) -> Result<()> {
    let id = project.project.registration.binding;
    let epoch = project.configuration_epoch;
    if !project.observation.enabled {
        if sent.get(&id) != Some(&(epoch, None)) {
            child.send(vec![Command::Remove {
                project: id,
                configuration_epoch: epoch,
            }])?;
            sent.insert(id, (epoch, None));
        }
        return Ok(());
    }
    let digest = protocol::binding_digest(
        &project.project.registration,
        epoch,
        project.observation.configuration_observed,
        &project.additional_roots,
    )?;
    if sent.get(&id) == Some(&(epoch, Some(digest))) {
        return Ok(());
    }
    let mut commands = vec![Command::Begin {
        registration: project.project.registration.clone(),
        configuration_epoch: epoch,
        configuration_observed: project.observation.configuration_observed,
        additional_roots: project.additional_roots.len() as u16,
        digest,
    }];
    commands.extend(project.additional_roots.iter().map(|path| Command::Root {
        project: id,
        configuration_epoch: epoch,
        path: path.clone(),
    }));
    commands.push(Command::Commit {
        project: id,
        configuration_epoch: epoch,
    });
    child.send(commands)?;
    sent.insert(id, (epoch, Some(digest)));
    Ok(())
}

fn retry_unavailable_configuration(
    sent: &mut BTreeMap<ProjectBinding, (u64, Option<Sha256Digest>)>,
    project: ProjectBinding,
    epoch: u64,
    failure: Option<WatchFailure>,
) {
    if matches!(
        failure,
        Some(WatchFailure::RootLimit | WatchFailure::MetadataUnavailable)
    ) && sent
        .get(&project)
        .is_some_and(|(current, _)| *current == epoch)
    {
        // Retry through the existing bounded registry sweep. A delayed failure
        // from a superseded epoch must not invalidate its replacement binding.
        sent.remove(&project);
    }
}

#[cfg(test)]
mod retry_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stuck_watcher_is_quarantined_with_its_lease_and_reaped_after_its_own_exit() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?.join("control");
        crate::local_ipc::prepare_root(&root)?;
        let (acquired, held) = mpsc::channel();
        let (release, gate) = mpsc::channel::<()>();
        let lease_root = root.clone();
        // A watcher thread inside a call that never returns to its loop.
        let thread = std::thread::spawn(move || -> Result<()> {
            let lease = SourceWatchLease::acquire(&lease_root)?;
            let _ = acquired.send(());
            let _ = gate.recv();
            drop(lease);
            Ok(())
        });
        held.recv_timeout(Duration::from_secs(10))?;
        let child = WatchChild {
            process: WorkerHost::Thread(OwnerThread::new(thread)),
            worker: new_instance_id()?,
            sender: None,
            writer: None,
            reader: None,
            replies: Arc::new(Mutex::new(Replies::default())),
            started: Instant::now(),
            pid: Arc::new(AtomicU32::new(0)),
        };
        let start = Instant::now();
        let at = |seconds| start + Duration::from_secs(seconds);
        let mut stopping = StoppingWatch::new(child, start);
        // Manual clock: the stop period, the drain period, then one quarantine.
        assert_eq!(stopping.poll(at(4)), WatchStop::Pending);
        assert_eq!(stopping.poll(at(5)), WatchStop::Pending);
        assert!(stopping.drain_requested);
        assert_eq!(stopping.poll(at(24)), WatchStop::Pending);
        assert_eq!(stopping.poll(at(25)), WatchStop::Quarantined);
        assert_eq!(stopping.poll(at(3600)), WatchStop::Pending);
        // Quarantine never releases the lease its own thread holds.
        assert!(SourceWatchLease::acquire(&root).is_err());
        release.send(())?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while stopping.poll(Instant::now()) != WatchStop::Exited {
            ensure!(
                Instant::now() < deadline,
                "released watcher exit was not observed"
            );
            std::thread::yield_now();
        }
        assert!(stopping.quarantined);
        drop(stopping);
        drop(SourceWatchLease::acquire(&root)?);
        Ok(())
    }

    #[test]
    fn policy_acknowledgements_bind_operation_nonce_child_and_release_phase() -> Result<()> {
        let operation_id = new_instance_id()?;
        let nonce = new_instance_id()?;
        let worker = new_instance_id()?;
        let mut fence = PolicyFence {
            operation_id,
            nonce,
            acknowledged: false,
            release_requested: false,
            sent: Some((worker, false)),
        };
        assert!(fence.matches_ack(worker, operation_id, nonce, false));
        assert!(!fence.matches_ack(new_instance_id()?, operation_id, nonce, false));
        assert!(!fence.matches_ack(worker, new_instance_id()?, nonce, false));
        assert!(!fence.matches_ack(worker, operation_id, new_instance_id()?, false));
        assert!(!fence.matches_ack(worker, operation_id, nonce, true));
        fence.release_requested = true;
        assert!(!fence.matches_ack(worker, operation_id, nonce, true));
        fence.sent = Some((worker, true));
        assert!(fence.matches_ack(worker, operation_id, nonce, true));
        assert!(!fence.matches_ack(worker, operation_id, nonce, false));
        Ok(())
    }

    #[test]
    fn policy_fence_requires_native_lease_absence_and_exact_release_without_scanning() -> Result<()>
    {
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?.join("control");
        crate::local_ipc::prepare_root(&root)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let sender = Arc::new(sender);
        let fences = Arc::new(PolicyFences::default());
        let wake = SourceWake {
            sender: Arc::downgrade(&sender),
            revision: Arc::new(AtomicU64::new(0)),
            idle: Arc::new(AtomicBool::new(true)),
            policy_fences: fences.clone(),
        };
        let project = Sha256Digest::from_bytes([73; 32]);
        let operation = new_instance_id()?;
        let service = new_instance_id()?;
        let waiting = wake.clone();
        let fence_thread = std::thread::spawn(move || waiting.fence_policy(project, operation));
        receiver.recv_timeout(Duration::from_secs(5))?;
        let lease = SourceWatchLease::acquire(&root)?;
        assert!(!coordinate_policy_fences(&root, service, None, &fences)?);
        assert!(
            fences
                .entries
                .lock()
                .unwrap()
                .get(&project)
                .is_some_and(|fence| !fence.acknowledged)
        );
        drop(lease);
        assert!(!coordinate_policy_fences(&root, service, None, &fences)?);
        fence_thread
            .join()
            .map_err(|_| anyhow::anyhow!("policy fence fixture panicked"))??;
        assert!(fences.blocked(project));
        assert!(wake.fence_policy(project, new_instance_id()?).is_err());
        assert!(wake.release_policy(project, new_instance_id()?).is_err());
        let releasing = wake.clone();
        let release_thread =
            std::thread::spawn(move || releasing.release_policy(project, operation));
        receiver.recv_timeout(Duration::from_secs(5))?;
        assert!(coordinate_policy_fences(&root, service, None, &fences)?);
        release_thread
            .join()
            .map_err(|_| anyhow::anyhow!("policy release fixture panicked"))??;
        assert!(!fences.blocked(project));
        assert!(
            !root.join("service-1.4.sqlite3").exists(),
            "source fence absence proof cannot create/read source configuration"
        );
        drop(sender);
        assert!(
            wake.sender.upgrade().is_none(),
            "wake clones must not retain the coordinator sender"
        );
        fences.closing.store(true, Ordering::Release);
        assert!(wake.fence_policy(project, operation).is_err());
        Ok(())
    }

    #[test]
    fn native_missing_event_requires_current_enabled_epochs_and_actual_missing_root() -> Result<()>
    {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let source = base.join("source");
        std::fs::create_dir(&source)?;
        std::fs::write(source.join("retained.txt"), b"preserved")?;
        let control = base.join("control");
        crate::local_ipc::prepare_root(&control)?;
        let mut registry = ServiceRegistry::open(&control, new_instance_id()?)?;
        let registration = airk_kernel::service_projects::planned_registration(
            &source,
            &base.join("data"),
            "missing-event",
        )?;
        registry.register(&registration)?;
        registry.configure_watch(registration.binding, true)?;
        let watch = registry.watch(registration.binding)?;
        let epoch = watch.project.fencing_generation;
        let configuration = watch.configuration_epoch;
        assert!(
            NativeMissingSourceEvent::observed(
                &registry,
                registration.binding,
                epoch,
                configuration
            )?
            .is_none()
        );
        std::fs::rename(&source, base.join("parked"))?;
        let event = NativeMissingSourceEvent::observed(
            &registry,
            registration.binding,
            epoch,
            configuration,
        )?
        .context("actual missing source did not produce native evidence")?;
        assert_eq!(event.binding(), registration.binding);
        assert_eq!(event.registration_epoch(), epoch);
        assert_eq!(event.configuration_epoch(), configuration);
        assert!(
            NativeMissingSourceEvent::observed(
                &registry,
                registration.binding,
                epoch + 1,
                configuration
            )?
            .is_none()
        );
        assert!(
            NativeMissingSourceEvent::observed(
                &registry,
                registration.binding,
                epoch,
                configuration + 1
            )?
            .is_none()
        );
        registry.configure_watch(registration.binding, false)?;
        let disabled = registry.watch(registration.binding)?;
        assert!(
            NativeMissingSourceEvent::observed(
                &registry,
                registration.binding,
                epoch,
                disabled.configuration_epoch
            )?
            .is_none()
        );
        assert_eq!(
            std::fs::read(base.join("parked/retained.txt"))?,
            b"preserved"
        );
        assert!(!base.join("data").exists());
        assert!(
            registry.is_registered(registration.binding)?,
            "an observation cannot retire metadata"
        );
        Ok(())
    }

    #[test]
    fn source_coordination_waits_for_metadata_bootstrap_and_can_drop_before_release() -> Result<()>
    {
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?.join("control");
        crate::local_ipc::prepare_root(&root)?;
        let service = new_instance_id()?;
        let (log, _log_guard) = crate::service_logs::Log::start(root.clone(), service);
        let held = SourceSupervisor::start(root.clone(), service, log.clone())?;
        std::thread::sleep(Duration::from_millis(50));
        assert!(!root.join("service-1.4.sqlite3").exists());
        drop(held);
        assert!(!root.join("service-1.4.sqlite3").exists());

        let sources = SourceSupervisor::start(root.clone(), service, log)?;
        let registry = ServiceRegistry::open(&root, service)?;
        let incarnation = registry.incarnation()?;
        drop(registry);
        sources.wake_handle().changed();
        let deadline = Instant::now() + Duration::from_secs(5);
        while sources.cache.lock().unwrap().coordinated.is_none() {
            ensure!(
                Instant::now() < deadline,
                "source coordinator did not resume"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            ServiceRegistry::open(&root, service)?.incarnation()?,
            incarnation
        );
        Ok(())
    }
}
