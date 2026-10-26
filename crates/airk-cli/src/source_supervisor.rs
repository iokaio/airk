// SPDX-License-Identifier: Apache-2.0
//! One bounded watcher child, metadata-only delivery, and independent cached observations.
use crate::{
    native_process, service_ipc,
    source_protocol::{self as protocol, Command, Reply},
};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{ControlResponse, PROTOCOL_VERSION, ProjectBinding},
    service_watch::{WatchBackend, WatchFailure},
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
    process::{Child, Command as ProcessCommand, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Replies {
    alive: bool,
    pulse: Option<Instant>,
    directories: Option<u16>,
    projects: Option<u16>,
    pending: Option<u16>,
    observations: BTreeMap<ProjectBinding, (u64, u16, WatchBackend, Option<WatchFailure>)>,
    events: BTreeMap<ProjectBinding, (u64, u64)>,
}
enum Outgoing {
    Initialize(protocol::Init),
    Commands(Vec<Command>),
}

struct WatchChild {
    process: Child,
    _containment: crate::child_job::ChildJob,
    worker: Incarnation,
    sender: Option<SyncSender<Outgoing>>,
    writer: Option<std::thread::JoinHandle<()>>,
    reader: Option<std::thread::JoinHandle<()>>,
    replies: Arc<Mutex<Replies>>,
    started: Instant,
    pid: Arc<AtomicU32>,
}
impl Drop for WatchChild {
    fn drop(&mut self) {
        self.sender.take();
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            match self.process.try_wait() {
                Ok(Some(_)) => break,
                Err(_) => std::thread::sleep(Duration::from_millis(25)),
                Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(25)),
                Ok(None) => {
                    let _ = self.process.kill();
                    let _ = self.process.wait();
                    break;
                }
            }
        }
        self._containment.drain();
        // Killing owned descendants also releases inherited pipe endpoints.
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        self.pid.store(0, Ordering::Release);
    }
}
impl WatchChild {
    fn start(root: &Path, service: Incarnation, pid: Arc<AtomicU32>) -> Result<Self> {
        // Unknown predecessor capacity stays reserved until its OS lease releases.
        drop(SourceWatchLease::acquire(root)?);
        let worker = new_instance_id()?;
        let mut command = ProcessCommand::new(std::env::current_exe()?);
        command
            .arg("source-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut process = native_process::spawn(&mut command)?;
        let containment = crate::child_job::ChildJob::attach(&mut process)?;
        let mut input = process.stdin.take().context("watcher input missing")?;
        let mut output = process.stdout.take().context("watcher output missing")?;
        let replies = Arc::new(Mutex::new(Replies {
            alive: true,
            ..Replies::default()
        }));
        let observed = replies.clone();
        let (ready, handshake) = mpsc::sync_channel(2);
        let mut child = Self {
            process,
            _containment: containment,
            worker,
            sender: None,
            writer: None,
            reader: None,
            replies,
            started: Instant::now(),
            pid,
        };
        child.pid.store(child.process.id(), Ordering::Release);
        child.reader = Some(
            std::thread::Builder::new()
                .name("airk-watch-replies".into())
                .spawn(move || {
                    while let Ok(Some(reply)) = service_ipc::read_frame::<_, Reply>(&mut output) {
                        match reply {
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
                                if !protocol::valid_pulse(
                                    watched_directories,
                                    projects,
                                    pending_events,
                                ) {
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
                            Reply::Observation {
                                project,
                                configuration_epoch,
                                native_roots,
                                backend,
                                failure,
                            } => {
                                if native_roots > 17 {
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
                                    .is_none_or(|(epoch, _, _, _)| *epoch <= configuration_epoch)
                                {
                                    observed.observations.insert(
                                        project,
                                        (configuration_epoch, native_roots, backend, failure),
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
                                if !observed.events.contains_key(&project)
                                    && observed.events.len() >= 1024
                                {
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
                })?,
        );
        let (sender, receiver) = mpsc::sync_channel::<Outgoing>(8);
        child.writer = Some(
            std::thread::Builder::new()
                .name("airk-watch-commands".into())
                .spawn(move || {
                    while let Ok(batch) = receiver.recv() {
                        match batch {
                            Outgoing::Initialize(init) => {
                                if service_ipc::write_frame(&mut input, &init).is_err() {
                                    return;
                                }
                            }
                            Outgoing::Commands(commands) => {
                                for command in commands {
                                    if service_ipc::write_frame(&mut input, &command).is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                })?,
        );
        child.sender = Some(sender);
        child
            .sender
            .as_ref()
            .unwrap()
            .try_send(Outgoing::Initialize(protocol::Init {
                protocol: PROTOCOL_VERSION,
                version: env!("CARGO_PKG_VERSION").into(),
                runtime: root.into(),
                service,
                worker,
            }))?;
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
    sender: SyncSender<()>,
    revision: Arc<AtomicU64>,
    idle: Arc<AtomicBool>,
}
impl SourceWake {
    pub(crate) fn changed(&self) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        self.idle.store(false, Ordering::Release);
        // A queued wake already covers this revision. The coordinator also checks
        // the revision when publishing an idle observation, closing the scan race.
        let _ = self.sender.try_send(());
    }
}
pub struct SourceSupervisor {
    sender: Option<SyncSender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub process_id: Arc<AtomicU32>,
    pub telemetry_complete: Arc<AtomicBool>,
    cache: Arc<Mutex<Cache>>,
    idle_safe: Arc<AtomicBool>,
    configuration_revision: Arc<AtomicU64>,
}
impl Drop for SourceSupervisor {
    fn drop(&mut self) {
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
        let thread = std::thread::Builder::new()
            .name("airk-source-coordinator".into())
            .spawn(move || {
                let mut child: Option<WatchChild> = None;
                let mut registry = None;
                let mut after = None;
                let mut sent = BTreeMap::<ProjectBinding, (u64, Option<Sha256Digest>)>::new();
                let mut periodic = BTreeMap::<ProjectBinding, Instant>::new();
                let mut enabled = BTreeSet::new();
                let mut seen = BTreeSet::new();
                let mut scanned = false;
                let mut next_page = Instant::now();
                let mut attempts = 0u32;
                let mut retry = Instant::now();
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
                    let configuration_snapshot = configuration.load(Ordering::Acquire);
                    if child.is_none() {
                        let predecessor = SourceWatchLease::acquire(&root).is_err();
                        telemetry.store(!predecessor, Ordering::Release);
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
                            || airk_kernel::service_resource_observations::rss(child.process.id())
                                .is_some_and(|bytes| {
                                    bytes > airk_kernel::source_watch::memory_allowance()
                                })
                    }) {
                        log.record(
                            airk_core::service_logs::LogKind::SourceWorkerFailed,
                            None,
                            None,
                        );
                        drop(child.take());
                        pid.store(0, Ordering::Release);
                        sent.clear();
                        attempts = attempts.saturating_add(1);
                        retry = Instant::now() + Duration::from_secs(1 << attempts.min(6));
                    }
                    if registry.is_none() {
                        registry = ServiceRegistry::open(&root, service).ok();
                    }
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .and_then(|time| u64::try_from(time.as_millis()).ok());
                    if let (Some(registry), Some(now_ms)) = (registry.as_mut(), now_ms) {
                        if let Some(active) = child.as_ref() {
                            let (observations, events) = if let Ok(mut replies) =
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
                                (observations, events)
                            } else {
                                (vec![], vec![])
                            };
                            for (project, (epoch, native, backend, failure)) in observations {
                                if failure == Some(WatchFailure::RootLimit) {
                                    sent.remove(&project);
                                }
                                let _ = registry.record_watch_observation(
                                    project, epoch, native, backend, failure, now_ms,
                                );
                            }
                            for (project, (epoch, sequence)) in events {
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
                        if Instant::now() >= next_page
                            && let Ok(projects) = registry.watch_projects(after, 32)
                        {
                            let mut complete = true;
                            for project in projects {
                                let id = project.project.registration.binding;
                                seen.insert(id);
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
                                    if child.is_none() && attempts < 3 && Instant::now() >= retry {
                                        let predecessor = SourceWatchLease::acquire(&root).is_err();
                                        if !predecessor
                                            && registry.reset_native_watch_observations().is_ok()
                                        {
                                            match WatchChild::start(&root, service, pid.clone()) {
                                                Ok(active) => {
                                                    pid.store(
                                                        active.process.id(),
                                                        Ordering::Release,
                                                    );
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
                                } else {
                                    enabled.remove(&id);
                                    periodic.remove(&id);
                                }
                                if let Some(active) = child.as_ref() {
                                    if send_configuration(active, &project, &mut sent).is_err() {
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
                        let no_background_intent = scanned && enabled.is_empty();
                        cache.blocked = attempts >= 3;
                        cache.available = false;
                        if let Some(active) = child.as_ref()
                            && let Ok(replies) = active.replies.lock()
                        {
                            cache.available = replies.alive
                                && replies
                                    .pulse
                                    .is_some_and(|pulse| pulse.elapsed() < Duration::from_secs(15));
                            cache.process_id = Some(active.process.id());
                            cache.directories = replies.directories;
                            cache.projects = replies.projects;
                            cache.pending = replies.pending;
                            cache.observed = replies.pulse;
                            idle.store(
                                no_background_intent
                                    && replies.events.is_empty()
                                    && replies.pending == Some(0),
                                Ordering::Release,
                            );
                        } else {
                            cache.process_id = None;
                            idle.store(no_background_intent, Ordering::Release);
                        }
                    }
                }
                drop(child);
                pid.store(0, Ordering::Release);
            })?;
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
            process_id,
            telemetry_complete,
            cache,
            idle_safe,
            configuration_revision,
        })
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
            sender: self
                .sender
                .as_ref()
                .expect("active source supervisor")
                .clone(),
            revision: self.configuration_revision.clone(),
            idle: self.idle_safe.clone(),
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
