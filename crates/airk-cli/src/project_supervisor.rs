// SPDX-License-Identifier: Apache-2.0
//! Finite child supervision; allocation survives timeout until child exit is proven.
use crate::{
    embedding_pool_process::Pool,
    local_ipc,
    project_protocol::{self as protocol, Command as WorkerCommand, Reply},
    service_ipc,
};
use airk_core::{
    identity::Incarnation,
    service::{ControlResponse, MAX_FRAME_BYTES, ProjectBinding, QueryAllowance, WorkerIdentity},
};
use airk_kernel::service_broker::{Broker, new_instance_id};
use airk_store::service::{EmbeddingLease, ProjectLease, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

struct ChildWorker {
    _containment: crate::child_job::ChildJob,
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Reply>,
    reader: Option<std::thread::JoinHandle<()>>,
    identity: Option<WorkerIdentity>,
    role: String,
    runtime: PathBuf,
    started: Instant,
    last_renewal: Instant,
    pool_active: bool,
    last_status: Instant,
}

impl Drop for ChildWorker {
    fn drop(&mut self) {
        if let Some(mut input) = self.input.take() {
            let _ = service_ipc::write_frame(&mut input, &WorkerCommand::Stop);
        }
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                _ if Instant::now() < until => std::thread::sleep(Duration::from_millis(25)),
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        self._containment.drain();
        let _ = local_ipc::remove_exited_endpoint(&self.runtime, &self.role);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl ChildWorker {
    fn spawn(root: &Path, project: ProjectBinding, service: Incarnation) -> Result<Self> {
        let executable = std::env::current_exe()?.canonicalize()?;
        let worker = new_instance_id()?;
        let mut command = Command::new(executable);
        command
            .arg("project-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child =
            crate::native_process::spawn(&mut command).context("start project owner")?;
        let containment = crate::child_job::ChildJob::attach(&mut child)?;
        let input = child.stdin.take().context("project input missing")?;
        let mut output = child.stdout.take().context("project output missing")?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut process = Self {
            child,
            _containment: containment,
            input: Some(input),
            output: receiver,
            reader: None,
            identity: None,
            role: protocol::role(worker),
            runtime: root.into(),
            started: Instant::now(),
            last_renewal: Instant::now(),
            pool_active: false,
            last_status: Instant::now(),
        };
        process.reader = Some(
            std::thread::Builder::new()
                .name("airk-project-replies".into())
                .spawn(move || {
                    while let Ok(Some(reply)) = service_ipc::read_frame(&mut output) {
                        if sender.try_send(reply).is_err() {
                            break;
                        }
                    }
                })?,
        );
        service_ipc::write_frame(
            process.input.as_mut().context("project input missing")?,
            &protocol::Init {
                engine: crate::runtime_pair::engine_identity()?,
                runtime: root.into(),
                project,
                service,
                worker,
            },
        )?;
        let reply = process
            .output
            .recv_timeout(Duration::from_secs(10))
            .context("project startup failed or timed out")?;
        match reply {
            Reply::Ready { identity, role } => {
                ensure!(
                    identity.service == service
                        && identity.worker == worker
                        && identity.project == project
                        && identity.fencing_generation > 0
                        && role == process.role,
                    "project owner identity mismatch"
                );
                process.identity = Some(identity);
            }
            _ => anyhow::bail!("project owner failed initialization"),
        }
        Ok(process)
    }

    fn grant(&mut self) -> Result<ControlResponse> {
        service_ipc::write_frame(
            self.input.as_mut().context("project input missing")?,
            &WorkerCommand::Grant {
                caller: local_ipc::caller_identity()?,
            },
        )?;
        let reply = self
            .output
            .recv_timeout(Duration::from_secs(3))
            .context("project grant response unknown")?;
        match reply {
            Reply::Grant { grant } => {
                ensure!(
                    Some(grant.identity) == self.identity,
                    "foreign project connection grant"
                );
                Ok(ControlResponse::Connection {
                    role: self.role.clone(),
                    grant,
                })
            }
            _ => anyhow::bail!("project rejected connection brokerage"),
        }
    }

    fn activate(
        &mut self,
        identity: WorkerIdentity,
        allowance: QueryAllowance,
        pool: Option<crate::embedding_pool_protocol::Access>,
    ) -> Result<()> {
        let pool_active = pool.is_some();
        service_ipc::write_frame(
            self.input.as_mut().context("project input missing")?,
            &WorkerCommand::Activate {
                identity,
                allowance,
                pool: pool.map(Box::new),
            },
        )?;
        ensure!(
            matches!(
                self.output.recv_timeout(Duration::from_secs(10))?,
                Reply::Activated
            ),
            "project activation failed"
        );
        self.pool_active = pool_active;
        Ok(())
    }

    fn run_job(&mut self, allowance: airk_core::service_resources::WorkAllowance) -> Result<()> {
        let identity = self.identity.context("project identity missing")?;
        service_ipc::write_frame(
            self.input.as_mut().context("project input missing")?,
            &WorkerCommand::RunJob {
                identity,
                allowance,
            },
        )?;
        ensure!(
            matches!(
                self.output.recv_timeout(Duration::from_secs(3))?,
                Reply::WorkAccepted
            ),
            "project work admission failed"
        );
        Ok(())
    }
    fn work_status(
        &mut self,
        control: Option<protocol::WorkControl>,
        keep_for_maintenance: bool,
    ) -> Result<protocol::WorkObservation> {
        let identity = self.identity.context("project identity missing")?;
        service_ipc::write_frame(
            self.input.as_mut().context("project input missing")?,
            &WorkerCommand::WorkStatus {
                identity,
                control,
                keep_for_maintenance,
            },
        )?;
        let Reply::WorkStatus { observation } = self.output.recv_timeout(Duration::from_secs(3))?
        else {
            anyhow::bail!("project work status failed")
        };
        if let Some(maintenance) = &observation.maintenance {
            maintenance.validate()?;
        }
        self.last_status = Instant::now();
        Ok(observation)
    }

    fn renew(&mut self) -> Result<()> {
        let identity = self.identity.context("project identity missing")?;
        service_ipc::write_frame(
            self.input.as_mut().context("project input missing")?,
            &WorkerCommand::Renew { identity },
        )?;
        ensure!(
            matches!(
                self.output.recv_timeout(Duration::from_secs(3))?,
                Reply::Renewed
            ),
            "project allowance renewal failed"
        );
        self.last_renewal = Instant::now();
        Ok(())
    }
}

struct ResourceCache {
    snapshot: airk_core::service_resources::ResourceSnapshot,
    observed: Instant,
    configuration_error: bool,
    idle_safe: bool,
}
enum Envelope {
    ConfigureResources {
        configuration: airk_core::service_resources::ResourceConfiguration,
        reply: oneshot::Sender<ControlResponse>,
    },
    Connect {
        project: ProjectBinding,
        reply: oneshot::Sender<ControlResponse>,
    },
    EmbeddingStatus {
        reply: oneshot::Sender<ControlResponse>,
    },
}
pub struct Supervisor {
    resources: Arc<Mutex<ResourceCache>>,
    sender: Option<SyncSender<Envelope>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct WorkerAdmission<'a> {
    log: &'a crate::service_logs::Log,
    policy: &'a Mutex<Broker>,
    pool: &'a mut Option<Pool>,
    resources: &'a mut airk_kernel::service_resources::Resources,
}

fn settle_failed_start(root: &Path, instance: Incarnation, project: ProjectBinding) {
    if let Ok(lease) = ProjectLease::acquire(root, project)
        && let Ok(mut registry) = ServiceRegistry::open(root, instance)
    {
        let _ = registry.fail_pending_start(&lease, "worker_start_failed");
    }
}

fn start_worker(
    workers: &mut BTreeMap<ProjectBinding, ChildWorker>,
    root: &Path,
    registration: airk_core::service::ProjectRegistration,
    instance: Incarnation,
    backoff: &mut BTreeMap<ProjectBinding, (Instant, u32)>,
    admission: WorkerAdmission<'_>,
) -> Result<()> {
    let WorkerAdmission {
        log,
        policy,
        pool,
        resources,
    } = admission;
    let project = registration.binding;
    if workers.contains_key(&project) {
        return Ok(());
    }
    ensure!(resources.may_start_resident(), "project capacity busy");
    ensure!(
        backoff
            .get(&project)
            .is_none_or(|(until, _)| Instant::now() >= *until),
        "project restart backoff"
    );
    let mut worker = match ChildWorker::spawn(root, project, instance) {
        Ok(worker) => worker,
        Err(error) => {
            let attempt = backoff
                .get(&project)
                .map_or(1, |(_, attempt)| attempt.saturating_add(1).min(6));
            backoff.insert(
                project,
                (Instant::now() + Duration::from_secs(1 << attempt), attempt),
            );
            log.record(
                airk_core::service_logs::LogKind::WorkerFailed,
                Some(project),
                None,
            );
            settle_failed_start(root, instance, project);
            return Err(error);
        }
    };
    let identity = worker.identity.context("missing worker identity")?;
    let allowance = QueryAllowance {
        concurrent_queries: 1,
        response_bytes: MAX_FRAME_BYTES,
    };
    policy
        .lock()
        .map_err(|_| anyhow::anyhow!("service policy unavailable"))?
        .register(
            identity,
            allowance,
            Instant::now(),
            Duration::from_secs(300),
        )?;
    resources.resident_started(identity)?;
    if pool.is_none() {
        *pool = Pool::start(root, instance, resources.snapshot().inference_threads_limit).ok();
    }
    let access = pool
        .as_mut()
        .and_then(|pool| pool.access(identity.worker, registration).ok());
    if let Err(error) = worker.activate(identity, allowance, access) {
        drop(worker);
        resources.process_exited(identity)?;
        if let Some(pool) = pool.as_mut() {
            let _ = pool.revoke(identity.worker);
        }
        policy
            .lock()
            .map_err(|_| anyhow::anyhow!("service policy unavailable"))?
            .process_exited(identity)?;
        let attempt = backoff
            .get(&project)
            .map_or(1, |(_, attempt)| attempt.saturating_add(1).min(6));
        backoff.insert(
            project,
            (Instant::now() + Duration::from_secs(1 << attempt), attempt),
        );
        log.record(
            airk_core::service_logs::LogKind::WorkerFailed,
            Some(project),
            None,
        );
        settle_failed_start(root, instance, project);
        return Err(error);
    }
    workers.insert(project, worker);
    Ok(())
}

impl Supervisor {
    pub fn start(
        root: PathBuf,
        instance: Incarnation,
        policy: Arc<Mutex<Broker>>,
        source_pid: Arc<std::sync::atomic::AtomicU32>,
        source_telemetry: Arc<std::sync::atomic::AtomicBool>,
        log: crate::service_logs::Log,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Envelope>(32);
        let loaded = crate::service_resource_policy::load(&root);
        let configuration_error = loaded.is_err();
        let mut initial =
            airk_kernel::service_resources::Resources::detect(loaded.unwrap_or_default())?;
        // Invalid persisted policy is visible and blocks new admission until explicitly corrected.
        if configuration_error {
            initial.block_admission(true);
        }
        let resources = Arc::new(Mutex::new(ResourceCache {
            snapshot: initial.snapshot(),
            observed: Instant::now(),
            configuration_error,
            idle_safe: false,
        }));
        let resource_cache = resources.clone();
        let thread = std::thread::Builder::new()
            .name("airk-project-supervisor".into())
            .spawn(move || {
                let mut workers = BTreeMap::<ProjectBinding, ChildWorker>::new();
                let mut pool: Option<Pool> = None;
                let mut registry: Option<ServiceRegistry> = None;
                let mut backoff = BTreeMap::<ProjectBinding, (Instant, u32)>::new();
                let mut predecessors: Option<Vec<ProjectBinding>> = None;
                let mut predecessor_pool = None;
                let mut resources = initial;
                let mut configuration_error = configuration_error;
                let mut pending_after = None;
                let mut source_after = None;
                let mut endpoints_ready = false;
                loop {
                    let observations = std::iter::once(std::process::id())
                        .chain(workers.values().map(|worker| worker.child.id()))
                        .chain(pool.as_ref().map(Pool::process_id))
                        .chain(
                            std::iter::once(source_pid.load(std::sync::atomic::Ordering::Acquire))
                                .filter(|pid| *pid != 0),
                        )
                        .map(airk_kernel::service_resource_observations::rss)
                        .collect::<Vec<_>>();
                    let complete = observations.iter().all(Option::is_some)
                        && source_telemetry.load(std::sync::atomic::Ordering::Acquire);
                    let rss = observations
                        .into_iter()
                        .flatten()
                        .reduce(u64::saturating_add);
                    let available = airk_kernel::service_resource_observations::available_memory();
                    resources.block_admission(configuration_error);
                    resources.observe(rss, available, complete && available.is_some());
                    if let Ok(mut cache) = resource_cache.lock() {
                        cache.snapshot = resources.snapshot();
                        cache.observed = Instant::now();
                        cache.configuration_error = configuration_error;
                        cache.idle_safe = workers.is_empty()
                            && registry.as_ref().is_some_and(|registry| {
                                registry
                                    .pending_projects(1)
                                    .is_ok_and(|page| page.is_empty())
                                    && registry
                                        .due_source_projects(None, i64::MAX as u64, 1)
                                        .is_ok_and(|page| page.is_empty())
                            });
                    }
                    if pool.as_mut().is_some_and(Pool::exited) {
                        log.record(airk_core::service_logs::LogKind::ModelFailed, None, None);
                        pool = None;
                    }
                    if registry.is_none() {
                        registry = ServiceRegistry::open(&root, instance).ok();
                    }
                    if predecessors.is_none()
                        && let Some(registry) = registry.as_ref()
                    {
                        predecessor_pool = Some(EmbeddingLease::acquire(&root).is_err());
                        predecessors = registry.bindings().ok().map(|bindings| {
                            bindings
                                .into_iter()
                                .filter(|binding| ProjectLease::acquire(&root, *binding).is_err())
                                .collect()
                        });
                    }
                    if predecessor_pool == Some(true) {
                        predecessor_pool = Some(EmbeddingLease::acquire(&root).is_err());
                    }
                    if let Some(predecessors) = predecessors.as_mut() {
                        predecessors
                            .retain(|binding| ProjectLease::acquire(&root, *binding).is_err());
                    }
                    // A restarted service must not reassign unknown old query/model capacity.
                    // Parent EOF stops old admission, but only OS lock release proves drain.
                    let predecessors_released = predecessors.as_ref().is_some_and(Vec::is_empty)
                        && predecessor_pool == Some(false);
                    if predecessors_released && !endpoints_ready {
                        endpoints_ready =
                            local_ipc::cleanup_predecessor_endpoints(&root).unwrap_or(false);
                    }
                    let may_allocate = predecessors_released && endpoints_ready;
                    let finished: Vec<_> = workers
                        .iter_mut()
                        .filter_map(|(project, worker)| match worker.child.try_wait() {
                            Ok(Some(status)) => {
                                if !status.success() {
                                    log.record(
                                        airk_core::service_logs::LogKind::WorkerFailed,
                                        Some(*project),
                                        None,
                                    );
                                }
                                Some(*project)
                            }
                            _ => None,
                        })
                        .collect();
                    for project in finished {
                        if let Some(worker) = workers.get(&project) {
                            if worker.started.elapsed() < Duration::from_secs(5) {
                                let attempt = backoff
                                    .get(&project)
                                    .map_or(1, |(_, attempt)| attempt.saturating_add(1).min(6));
                                backoff.insert(
                                    project,
                                    (Instant::now() + Duration::from_secs(1 << attempt), attempt),
                                );
                            } else {
                                backoff.remove(&project);
                            }
                        }
                        if let Some(worker) = workers.remove(&project) {
                            let identity = worker.identity;
                            // ChildJob drop also proves Windows descendant exit.
                            // Never return admission while an owned extractor lives.
                            drop(worker);
                            if let (Some(identity), Ok(mut policy)) = (identity, policy.lock()) {
                                let _ = policy.process_exited(identity);
                                let _ = resources.process_exited(identity);
                                drop(policy);
                                if let Some(pool) = pool.as_mut() {
                                    let _ = pool.revoke(identity.worker);
                                }
                            }
                        }
                    }
                    let mut failed_renewals = Vec::new();
                    for (project, worker) in &mut workers {
                        if worker.last_renewal.elapsed() < Duration::from_secs(30) {
                            continue;
                        }
                        let Some(identity) = worker.identity else {
                            continue;
                        };
                        let renewed = worker.renew().and_then(|_| {
                            policy
                                .lock()
                                .map_err(|_| anyhow::anyhow!("service policy unavailable"))?
                                .renew(identity, Instant::now(), Duration::from_secs(300))
                                .map_err(Into::into)
                        });
                        if renewed.is_err() {
                            failed_renewals.push(*project);
                            continue;
                        }
                        if worker.pool_active
                            && let Some(pool) = pool.as_mut()
                        {
                            let _ = pool.renew(identity.worker);
                        }
                    }
                    for project in failed_renewals {
                        log.record(
                            airk_core::service_logs::LogKind::WorkerFailed,
                            Some(project),
                            None,
                        );
                        if let Some(worker) = workers.remove(&project) {
                            let identity = worker.identity;
                            // An ambiguous acknowledgement never releases live capacity.
                            drop(worker);
                            if let (Some(identity), Ok(mut policy)) = (identity, policy.lock()) {
                                let _ = policy.process_exited(identity);
                                let _ = resources.process_exited(identity);
                            }
                        }
                    }
                    match receiver.recv_timeout(Duration::from_millis(250)) {
                        Ok(Envelope::ConfigureResources {
                            configuration,
                            reply,
                        }) => {
                            let result = configuration
                                .validate()
                                .and_then(|_| {
                                    crate::service_resource_policy::save(&root, &configuration)
                                })
                                .and_then(|_| resources.reconfigure(configuration));
                            let result = result.and_then(|_| match pool.as_mut() {
                                Some(pool) => pool.configure_threads(
                                    resources.snapshot().inference_threads_limit,
                                ),
                                None => Ok(()),
                            });
                            let response = if result.is_ok() {
                                configuration_error = false;
                                ControlResponse::Resources {
                                    observation: Box::new(resources.snapshot()),
                                    age_ms: 0,
                                    configuration_error: false,
                                }
                            } else {
                                configuration_error = true;
                                ControlResponse::Failure {
                                    code: "resource_configuration_rejected".into(),
                                }
                            };
                            let _ = reply.send(response);
                        }
                        Ok(Envelope::Connect { project, reply }) => {
                            let response = (|| {
                                ensure!(may_allocate, "previous project workers still draining");
                                let registration = registry
                                    .as_ref()
                                    .context("service registry unavailable")?
                                    .get(project)?
                                    .registration;
                                start_worker(
                                    &mut workers,
                                    &root,
                                    registration,
                                    instance,
                                    &mut backoff,
                                    WorkerAdmission {
                                        log: &log,
                                        policy: &policy,
                                        pool: &mut pool,
                                        resources: &mut resources,
                                    },
                                )?;
                                workers
                                    .get_mut(&project)
                                    .context("project worker missing")?
                                    .grant()
                            })()
                            .unwrap_or(ControlResponse::Failure {
                                code: if may_allocate {
                                    "project_unavailable_or_busy"
                                } else {
                                    "previous_workers_draining"
                                }
                                .into(),
                            });
                            let _ = reply.send(response);
                        }
                        Ok(Envelope::EmbeddingStatus { reply }) => {
                            let response = match pool.as_mut().map(|pool| {
                                let id = pool.process_id();
                                pool.status().map(|status| (id, status))
                            }) {
                                Some(Ok((
                                    id,
                                    crate::embedding_pool_protocol::Observation::Status {
                                        loads,
                                        reuses,
                                        sessions,
                                        age_ms,
                                    },
                                ))) => ControlResponse::EmbeddingStatus {
                                    available: true,
                                    process_id: Some(id),
                                    loads,
                                    reuses,
                                    sessions,
                                    observation_age_ms: Some(age_ms),
                                },
                                None => ControlResponse::EmbeddingStatus {
                                    available: false,
                                    process_id: None,
                                    loads: 0,
                                    reuses: 0,
                                    sessions: 0,
                                    observation_age_ms: None,
                                },
                                _ => ControlResponse::Failure {
                                    code: "embedding_observation_busy".into(),
                                },
                            };
                            let _ = reply.send(response);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if let Some(registry) = registry.as_mut() {
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64);
                        if let Ok(due) = registry.due_source_projects(source_after, now_ms, 32) {
                            source_after = due.last().map(|(project, _)| *project);
                            for (project, revision) in due {
                                let Ok(nonce) = new_instance_id() else {
                                    continue;
                                };
                                let _ = registry.enqueue_source_reconcile(
                                    project, instance, nonce, revision, now_ms,
                                );
                            }
                        }
                        if let Ok(due) = registry.due_maintenance(now_ms) {
                            for project in due {
                                if resources.running(project).is_none() {
                                    let Ok(nonce) = new_instance_id() else {
                                        continue;
                                    };
                                    let _ = registry
                                        .enqueue_due_maintenance(project, instance, nonce, now_ms);
                                }
                            }
                        }
                    }
                    if may_allocate
                        && let Some(registry) = registry.as_ref()
                        && let Ok(pending) = registry.pending_project_page(pending_after, 32)
                    {
                        pending_after = pending.last().copied();
                        for project in pending {
                            let Ok(registration) =
                                registry.get(project).map(|record| record.registration)
                            else {
                                continue;
                            };
                            let _ = start_worker(
                                &mut workers,
                                &root,
                                registration,
                                instance,
                                &mut backoff,
                                WorkerAdmission {
                                    log: &log,
                                    policy: &policy,
                                    pool: &mut pool,
                                    resources: &mut resources,
                                },
                            );
                        }
                    }
                    // Read only bounded metadata candidates. Direct query payloads never enter this loop.
                    if let Some(registry) = registry.as_ref() {
                        for project in workers.keys().copied() {
                            match registry.next_pending_job(project) {
                                Ok(Some(job)) => {
                                    let _ = resources.pending(
                                        project,
                                        job.id,
                                        job.kind,
                                        Instant::now(),
                                    );
                                }
                                Ok(None) => resources.forget_pending(project),
                                Err(_) => {}
                            }
                        }
                    }
                    let mut failed = Vec::new();
                    for (project, worker) in &mut workers {
                        if worker.last_status.elapsed() < Duration::from_secs(1) {
                            continue;
                        }
                        let control = resources.running(*project).and_then(|grant| {
                            registry.as_ref().and_then(|registry| {
                                let (plan_paused, plan_canceled) =
                                    registry.maintenance_job_control(grant.job).ok()?;
                                Some(protocol::WorkControl {
                                    grant: grant.id,
                                    paused: plan_paused
                                        || (registry.get(*project).ok()?.paused
                                            && !matches!(
                                                registry.job(grant.job).ok()?.kind,
                                                airk_core::service::JobKind::Inspect
                                                    | airk_core::service::JobKind::PlanInspect
                                            )),
                                    canceled: plan_canceled
                                        || registry.cancellation_requested(grant.job).ok()?,
                                })
                            })
                        });
                        let keep_for_maintenance = registry.as_ref().is_some_and(|registry| {
                            registry.maintenance_retrying(*project).unwrap_or(false)
                        });
                        match worker.work_status(control, keep_for_maintenance) {
                            Ok(observation) => {
                                if let Some(progress) = observation.progress.as_ref()
                                    && let Some(grant) = resources.running(*project)
                                {
                                    if progress.grant != grant.id
                                        || progress.job != grant.job
                                        || progress.validate().is_err()
                                    {
                                        failed.push(*project);
                                        continue;
                                    }
                                    if let Some(registry) = registry.as_mut()
                                        && let Some(now_ms) = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .ok()
                                            .and_then(|time| u64::try_from(time.as_millis()).ok())
                                    {
                                        let _ = registry.observe_progress(
                                            grant.identity,
                                            progress,
                                            now_ms,
                                        );
                                    }
                                }
                                if let Some(finished) = observation.finished
                                    && resources
                                        .running(*project)
                                        .is_some_and(|grant| grant.id == finished)
                                    && let Some(identity) = worker.identity
                                {
                                    let _ = resources.finished(identity, finished);
                                }
                            }
                            Err(_) => failed.push(*project),
                        }
                    }
                    loop {
                        let Ok(Some(grant)) = resources.next(Instant::now()) else {
                            break;
                        };
                        let project = grant.identity.project;
                        if workers
                            .get_mut(&project)
                            .is_none_or(|worker| worker.run_job(grant).is_err())
                        {
                            failed.push(project);
                        }
                    }
                    for project in failed {
                        log.record(
                            airk_core::service_logs::LogKind::WorkerFailed,
                            Some(project),
                            None,
                        );
                        if let Some(worker) = workers.remove(&project) {
                            let identity = worker.identity;
                            drop(worker); // Drop drains/kills and reaps before capacity returns.
                            if let Some(identity) = identity {
                                let _ = resources.process_exited(identity);
                                if let Ok(mut policy) = policy.lock() {
                                    let _ = policy.process_exited(identity);
                                }
                                if let Some(pool) = pool.as_mut() {
                                    let _ = pool.revoke(identity.worker);
                                }
                            }
                        }
                    }
                }
                // Children stop before the service lock is released. Interrupted jobs retain receipts.
                if let Ok(mut policy) = policy.lock() {
                    policy.ownership_lost();
                }
                drop(workers);
            })?;
        Ok(Self {
            resources,
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    pub fn can_idle_exit(&self) -> bool {
        self.resources
            .lock()
            .is_ok_and(|cache| cache.idle_safe && cache.observed.elapsed() < Duration::from_secs(2))
    }
    pub fn resource_status(&self) -> ControlResponse {
        match self.resources.lock() {
            Ok(cache) => ControlResponse::Resources {
                observation: Box::new(cache.snapshot.clone()),
                age_ms: cache.observed.elapsed().as_millis().min(u64::MAX as u128) as u64,
                configuration_error: cache.configuration_error,
            },
            Err(_) => ControlResponse::Failure {
                code: "resource_observation_unavailable".into(),
            },
        }
    }
    pub async fn configure_resources(
        &self,
        configuration: airk_core::service_resources::ResourceConfiguration,
    ) -> ControlResponse {
        let (reply, receiver) = oneshot::channel();
        if self.sender.as_ref().is_none_or(|queue| {
            queue
                .try_send(Envelope::ConfigureResources {
                    configuration,
                    reply,
                })
                .is_err()
        }) {
            return ControlResponse::Failure {
                code: "broker_busy".into(),
            };
        }
        match tokio::time::timeout(Duration::from_secs(4), receiver).await {
            Ok(Ok(response)) => response,
            _ => ControlResponse::Failure {
                code: "resource_configuration_outcome_unknown".into(),
            },
        }
    }
    pub async fn connect(&self, project: ProjectBinding) -> ControlResponse {
        let (sender, receiver) = oneshot::channel();
        if self.sender.as_ref().is_none_or(|queue| {
            queue
                .try_send(Envelope::Connect {
                    project,
                    reply: sender,
                })
                .is_err()
        }) {
            return ControlResponse::Failure {
                code: "broker_busy".into(),
            };
        }
        match tokio::time::timeout(Duration::from_secs(15), receiver).await {
            Ok(Ok(response)) => response,
            _ => ControlResponse::Failure {
                code: "project_starting_or_unavailable".into(),
            },
        }
    }
    pub async fn embedding_status(&self) -> ControlResponse {
        let (reply, receiver) = oneshot::channel();
        if self
            .sender
            .as_ref()
            .is_none_or(|queue| queue.try_send(Envelope::EmbeddingStatus { reply }).is_err())
        {
            return ControlResponse::Failure {
                code: "broker_busy".into(),
            };
        }
        match tokio::time::timeout(Duration::from_secs(4), receiver).await {
            Ok(Ok(response)) => response,
            _ => ControlResponse::Failure {
                code: "embedding_observation_busy".into(),
            },
        }
    }
}
