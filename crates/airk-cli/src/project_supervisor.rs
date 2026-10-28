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
    derived: bool,
    _containment: crate::child_job::ChildJob,
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Reply>,
    reader: Option<std::thread::JoinHandle<()>>,
    identity: Option<WorkerIdentity>,
    role: String,
    runtime: PathBuf,
    last_renewal: Instant,
    pool_binding: Option<Incarnation>,
    last_status: Instant,
    waiting: Option<(
        Incarnation,
        airk_core::service_resources::WaitReason,
        Instant,
    )>,
}

fn clean_disconnected_exit(child: &mut Child, error: &anyhow::Error) -> bool {
    let disconnected = error.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|error| {
            matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::UnexpectedEof
            )
        }) || cause.downcast_ref::<mpsc::RecvTimeoutError>()
            == Some(&mpsc::RecvTimeoutError::Disconnected)
    });
    if !disconnected {
        return false;
    }
    // A worker can close its control pipes while draining an intentional idle exit.
    // Observe its actual exit without sending Stop or releasing ownership first.
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => return false,
        }
    }
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
                // Unknown wait state never authorizes a numerical PID signal.
                // Retain ownership/admission until native exit is observable.
                Err(_) => std::thread::sleep(Duration::from_millis(25)),
                Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(25)),
                Ok(None) => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        self._containment.drain();
        #[cfg(unix)]
        if let Some(identity) = self.identity {
            crate::unix_guardian::wait_for_worker(&self.runtime, identity);
        }
        let _ = local_ipc::remove_exited_endpoint(&self.runtime, &self.role);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl ChildWorker {
    fn spawn(
        root: &Path,
        project: ProjectBinding,
        service: Incarnation,
        derived: bool,
    ) -> Result<Self> {
        let executable = std::env::current_exe()?.canonicalize()?;
        let worker = new_instance_id()?;
        let mut command = Command::new(executable);
        command
            .arg(if derived {
                "derived-worker"
            } else {
                "project-worker"
            })
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
            derived,
            child,
            _containment: containment,
            input: Some(input),
            output: receiver,
            reader: None,
            identity: None,
            role: protocol::role(worker),
            runtime: root.into(),
            last_renewal: Instant::now(),
            pool_binding: None,
            last_status: Instant::now(),
            waiting: None,
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
        let pool_binding = pool.as_ref().map(|access| access.grant.pool);
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
        self.pool_binding = pool_binding;
        Ok(())
    }

    fn configure_pool(
        &mut self,
        pool: Option<crate::embedding_pool_protocol::Access>,
    ) -> Result<()> {
        let pool_binding = pool.as_ref().map(|access| access.grant.pool);
        service_ipc::write_frame(
            self.input.as_mut().context("project input missing")?,
            &WorkerCommand::ConfigurePool {
                identity: self.identity.context("project identity missing")?,
                pool: pool.map(Box::new),
            },
        )?;
        ensure!(
            matches!(
                self.output.recv_timeout(Duration::from_secs(3))?,
                Reply::PoolConfigured
            ),
            "project pool configuration unknown"
        );
        self.pool_binding = pool_binding;
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
        freshness: Option<airk_core::changes::FreshnessObservation>,
    ) -> Result<protocol::WorkObservation> {
        let identity = self.identity.context("project identity missing")?;
        service_ipc::write_frame(
            self.input.as_mut().context("project input missing")?,
            &WorkerCommand::WorkStatus {
                identity,
                control,
                keep_for_maintenance,
                freshness: freshness.map(Box::new),
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
        self.waiting = observation
            .active
            .zip(observation.waiting_for)
            .map(|(grant, reason)| (grant, reason, self.last_status));
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

// Three attempts per service lifetime (or explicit start), with no source-job dependency.
struct PoolRecovery {
    attempts: u32,
    retry_at: Instant,
    suppressed: bool,
    requested: bool,
}
impl PoolRecovery {
    fn new(now: Instant) -> Self {
        Self {
            attempts: 0,
            retry_at: now,
            suppressed: false,
            requested: false,
        }
    }
    fn due(&self, now: Instant) -> bool {
        !self.suppressed && self.attempts < 3 && now >= self.retry_at
    }
    fn attempted(&mut self) {
        self.attempts += 1;
    }
    fn failed(&mut self, now: Instant) {
        self.retry_at = now + Duration::from_secs(1 << self.attempts.min(3));
    }
}

struct ResourceCache {
    snapshot: airk_core::service_resources::ResourceSnapshot,
    observed: Instant,
    configuration_error: bool,
    idle_safe: bool,
    workers: BTreeMap<ProjectBinding, WorkerIdentity>,
    restarting: std::collections::BTreeSet<ProjectBinding>,
    blocked: std::collections::BTreeSet<ProjectBinding>,
    waiting: BTreeMap<ProjectBinding, WaitSample>,
}

#[derive(Clone)]
struct WaitSample {
    job: airk_core::identity::Sha256Digest,
    reason: airk_core::service_resources::WaitReason,
    observed: Instant,
}
fn sample_wait(
    samples: &mut BTreeMap<ProjectBinding, WaitSample>,
    project: ProjectBinding,
    job: airk_core::identity::Sha256Digest,
    reason: airk_core::service_resources::WaitReason,
) {
    if !samples.contains_key(&project)
        && samples.len() >= 1024
        && let Some(oldest) = samples
            .iter()
            .min_by_key(|(_, sample)| sample.observed)
            .map(|(project, _)| *project)
    {
        samples.remove(&oldest);
    }
    samples.insert(
        project,
        WaitSample {
            job,
            reason,
            observed: Instant::now(),
        },
    );
}
impl ResourceCache {
    fn wait_observation(
        &self,
        project: ProjectBinding,
    ) -> Option<airk_core::service_resources::WaitObservation> {
        self.waiting
            .get(&project)
            .map(|sample| airk_core::service_resources::WaitObservation {
                job: sample.job,
                reason: sample.reason,
                age_ms: sample.observed.elapsed().as_millis().min(u64::MAX as u128) as u64,
            })
    }
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
    RestartProject {
        project: ProjectBinding,
        expected: Option<WorkerIdentity>,
        reply: oneshot::Sender<ControlResponse>,
    },
    StartEmbedding {
        reply: oneshot::Sender<ControlResponse>,
    },
    StopEmbedding {
        worker: Incarnation,
        force: bool,
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

fn worker_failed(
    root: &Path,
    backoff: &mut BTreeMap<ProjectBinding, (Instant, u32)>,
    project: ProjectBinding,
    now: Instant,
) {
    let saved = (|| -> Result<_> {
        let namespace = ServiceRegistry::open_saved(root)?.incarnation()?;
        let mut registry = ServiceRegistry::open(root, namespace)?;
        registry.recovery_failed(
            &project.to_string(),
            crate::runtime_pair::engine_identity()?,
            unix_ms()?,
        )
    })();
    // Missing durable accounting cannot authorize another automatic attempt.
    let attempts = saved.map_or(3, |budget| budget.failures);
    backoff.insert(
        project,
        (now + Duration::from_secs(1 << attempts), attempts),
    );
}
fn unix_ms() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}
fn check_recovery_conditions(registry: &mut ServiceRegistry, helper_available: bool) -> Result<()> {
    use airk_store::service::registry::RecoveryCondition;
    let now = unix_ms()?;
    for waiting in registry.waiting_recoveries(now)? {
        let project = registry.get(waiting.project)?.registration;
        let ready = match waiting.condition {
            RecoveryCondition::Delay => true,
            RecoveryCondition::Helper => helper_available,
            RecoveryCondition::Source => {
                let roots = registry.watch(waiting.project)?.additional_roots;
                std::iter::once(&project.root)
                    .chain(roots.iter())
                    .all(|root| {
                        std::fs::symlink_metadata(root)
                            .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
                            && std::fs::canonicalize(root).is_ok_and(|p| p == Path::new(root))
                    })
            }
            RecoveryCondition::Capacity { required } => {
                airk_store::capacity::probe(Path::new(&project.data_root))
                    .is_ok_and(|c| c.available_growth_bytes >= required)
            }
            RecoveryCondition::Quota { required, scope } => {
                airk_store::quota::Quota::service_recovery_available(
                    Path::new(&project.data_root),
                    &project.collection,
                    scope,
                )
                .is_ok_and(|available| available >= required)
            }
        };
        registry.check_recovery(waiting.job, ready, now)?;
    }
    Ok(())
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
    derived: bool,
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
    if let Some(worker) = workers.get(&project) {
        ensure!(worker.derived == derived, "project owner role is draining");
        return Ok(());
    }
    let saved = ServiceRegistry::open_saved(root)?.recovery_budget(
        &project.to_string(),
        crate::runtime_pair::engine_identity()?,
    )?;
    if saved.failures > 0 {
        backoff.insert(
            project,
            (
                Instant::now()
                    + Duration::from_millis(saved.next_ms.saturating_sub(unix_ms()?).min(60_000)),
                saved.failures,
            ),
        );
    } else if !backoff
        .get(&project)
        .is_some_and(|(_, attempts)| *attempts >= 3)
    {
        backoff.remove(&project);
    }
    ensure!(resources.may_start_resident(), "project capacity busy");
    if backoff
        .get(&project)
        .is_some_and(|(_, attempts)| *attempts >= 3)
    {
        // New requests cannot remain pending behind an already exhausted circuit.
        // This also covers an activated child that exited before accepting work.
        settle_failed_start(root, instance, project);
    }
    ensure!(
        backoff
            .get(&project)
            .is_none_or(|(until, attempts)| *attempts < 3 && Instant::now() >= *until),
        "project restart backoff"
    );
    let spawn = || -> Result<ChildWorker> {
        if !derived && ServiceRegistry::open_saved(root)?.project_setup_pending(project)? {
            crate::service_registration::prepare_storage(root, project)?;
        }
        ChildWorker::spawn(root, project, instance, derived)
    };
    let mut worker = match spawn() {
        Ok(worker) => worker,
        Err(error) => {
            worker_failed(root, backoff, project, Instant::now());
            log.record(
                airk_core::service_logs::LogKind::WorkerFailed,
                Some(project),
                None,
            );
            if backoff
                .get(&project)
                .is_some_and(|(_, attempts)| *attempts >= 3)
            {
                settle_failed_start(root, instance, project);
            }
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
    let access = pool
        .as_mut()
        .filter(|_| !derived)
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
        worker_failed(root, backoff, project, Instant::now());
        log.record(
            airk_core::service_logs::LogKind::WorkerFailed,
            Some(project),
            None,
        );
        if backoff
            .get(&project)
            .is_some_and(|(_, attempts)| *attempts >= 3)
        {
            settle_failed_start(root, instance, project);
        }
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
            workers: BTreeMap::new(),
            restarting: Default::default(),
            blocked: Default::default(),
            waiting: BTreeMap::new(),
        }));
        let resource_cache = resources.clone();
        let thread = std::thread::Builder::new()
            .name("airk-project-supervisor".into())
            .spawn(move || {
                let mut workers = BTreeMap::<ProjectBinding, ChildWorker>::new();
                let mut pool: Option<Pool> = None;
                let mut pool_recovery = PoolRecovery::new(Instant::now());
                let mut registry: Option<ServiceRegistry> = None;
                let mut backoff = BTreeMap::<ProjectBinding, (Instant, u32)>::new();
                let mut waiting = BTreeMap::<ProjectBinding, WaitSample>::new();
                let mut restarting = BTreeMap::<ProjectBinding, Option<WorkerIdentity>>::new();
                let mut predecessors: Option<Vec<ProjectBinding>> = None;
                let mut predecessor_pool = None;
                let mut resources = initial;
                let mut configuration_error = configuration_error;
                let mut pending_after = None;
                let mut source_after = None;
                let mut background_after = None;
                let mut endpoints_ready = false;
                let mut next_recovery_check=Instant::now();
                loop {
                    if registry.is_none() {
                        registry = ServiceRegistry::open(&root, instance).ok();
                    }
                    if Instant::now()>=next_recovery_check {
                        if let Some(registry)=registry.as_mut(){let _=check_recovery_conditions(registry,pool.is_some());}
                        next_recovery_check=Instant::now()+Duration::from_secs(1);
                    }
                    if pool.is_none() {
                        match registry.as_ref().and_then(|r|r.recovery_budget("helper",crate::runtime_pair::engine_identity().ok()?).ok()) {
                            Some(budget)=>{
                                pool_recovery.attempts=budget.failures;
                                pool_recovery.suppressed|=budget.suppressed;
                                if let Ok(now)=unix_ms(){pool_recovery.retry_at=Instant::now()+Duration::from_millis(budget.next_ms.saturating_sub(now).min(60_000));}
                            }
                            None=>pool_recovery.suppressed=true,
                        }
                    }
                    // A committed unregister closes fresh brokerage immediately.
                    // Drain only our exact child before releasing resident/query
                    // allowances; unknown registry state never authorizes a stop.
                    let unregistered = workers
                        .iter()
                        .filter(|(project, worker)| {
                            registry.as_ref().is_some_and(|registry| {
                                match registry.is_registered(**project) {
                                    Ok(active) => {
                                        if worker.derived {
                                            active
                                                || resources.running(**project).is_none()
                                                    && matches!(
                                                        registry.derived_work_pending(**project),
                                                        Ok(false)
                                                    )
                                        } else {
                                            !active
                                        }
                                    }
                                    Err(_) => false,
                                }
                            })
                        })
                        .map(|(project, _)| *project)
                        .collect::<Vec<_>>();
                    for project in unregistered {
                        if let Some(worker) = workers.remove(&project) {
                            let identity = worker.identity;
                            drop(worker);
                            if let Some(identity) = identity {
                                let _ = resources.process_exited(identity);
                                if let Ok(mut policy) = policy.lock() {
                                    let _ = policy.process_exited(identity);
                                }
                                if let Some(pool) = pool.as_mut() {
                                    let _ = pool.revoke(identity.worker);
                                }
                            }
                            log.record(
                                airk_core::service_logs::LogKind::WorkerStopped,
                                Some(project),
                                None,
                            );
                        }
                        backoff.remove(&project);
                        restarting.remove(&project);
                    }
                    // Stop only the exact accepted incarnation. Admission remains charged
                    // through descendant drain; a newer worker is never stopped by replay.
                    for (project, expected) in &mut restarting {
                        if let Some(worker) = workers.get(project)
                            && worker.identity == *expected
                        {
                            let worker = workers.remove(project).expect("observed restart worker");
                            let identity = worker.identity;
                            drop(worker);
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
                        *expected = None;
                    }
                    #[cfg(unix)]
                    let provider_drain = registry.as_mut().map(|registry| {
                        let active = workers
                            .values_mut()
                            .filter_map(|worker| {
                                matches!(worker.child.try_wait(), Ok(None))
                                    .then_some(worker.identity)
                                    .flatten()
                            })
                            .collect::<Vec<_>>();
                        crate::unix_guardian::recover(registry, &active)
                    });
                    #[cfg(unix)]
                    let (pending_providers, provider_error) = match provider_drain {
                        Some(Ok(pending)) => (pending, false),
                        _ => (0, true),
                    };
                    #[cfg(not(unix))]
                    let (pending_providers, provider_error) = (0, false);
                    resources.observe_provider_drains(pending_providers, provider_error);
                    #[cfg(unix)]
                    let provider_rss = registry
                        .as_ref()
                        .and_then(|registry| crate::unix_guardian::rss(registry).ok());
                    #[cfg(not(unix))]
                    let provider_rss = Some(Vec::new());
                    let observations = std::iter::once(std::process::id())
                        .chain(workers.values().map(|worker| worker.child.id()))
                        .chain(pool.as_ref().map(Pool::process_id))
                        .chain(
                            std::iter::once(source_pid.load(std::sync::atomic::Ordering::Acquire))
                                .filter(|pid| *pid != 0),
                        )
                        .map(airk_kernel::service_resource_observations::rss)
                        .chain(provider_rss.clone().unwrap_or_default())
                        .collect::<Vec<_>>();
                    let complete = observations.iter().all(Option::is_some)
                        && source_telemetry.load(std::sync::atomic::Ordering::Acquire)
                        && provider_rss.is_some();
                    let rss = observations
                        .into_iter()
                        .flatten()
                        .reduce(u64::saturating_add);
                    let available = airk_kernel::service_resource_observations::available_memory();
                    resources.block_admission(
                        configuration_error || provider_error || pending_providers > 0,
                    );
                    resources.observe(rss, available, complete && available.is_some());
                    // Registry I/O must never occur while cached observations
                    // are locked by health/console readers.
                    let idle_safe = workers.is_empty() && restarting.is_empty()
                        && !provider_error && pending_providers == 0
                        && registry.as_ref().is_some_and(|registry| {
                            registry.pending_projects(1).is_ok_and(|page| page.is_empty())
                                && registry.has_recovery_waits().is_ok_and(|pending|!pending)
                                && registry.has_background_work().is_ok_and(|pending| !pending)
                                && registry.due_source_projects(None, i64::MAX as u64, 1).is_ok_and(|page| page.is_empty())
                        });
                    let mut sampled_waits = waiting.clone();
                    for (project, worker) in &workers {
                        if let Some(grant) = resources.running(*project) {
                            sampled_waits.remove(project);
                            if let Some((active, reason, observed)) = worker.waiting && active == grant.id {
                                sampled_waits.insert(*project, WaitSample { job: grant.job, reason, observed });
                            }
                        }
                    }
                    if let Ok(mut cache) = resource_cache.lock() {
                        cache.workers = workers
                            .iter()
                            .filter_map(|(project, worker)| {
                                worker.identity.map(|identity| (*project, identity))
                            })
                            .collect();
                        cache.restarting = restarting.keys().copied().collect();
                        cache.blocked = backoff
                            .iter()
                            .filter_map(|(project, (_, attempts))| {
                                (*attempts >= 3).then_some(*project)
                            })
                            .collect();
                        cache.snapshot = resources.snapshot();
                        cache.observed = Instant::now();
                        cache.configuration_error = configuration_error;
                        cache.idle_safe = idle_safe;
                        cache.waiting = sampled_waits;
                    }
                    if pool.as_mut().is_some_and(Pool::exited) {
                        log.record(airk_core::service_logs::LogKind::ModelFailed, None, None);
                        drop(pool.take());
                        pool_recovery.failed(Instant::now());
                        if registry.as_mut().is_none_or(|r|crate::runtime_pair::engine_identity().and_then(|engine|r.recovery_failed("helper",engine,unix_ms()?)).is_err()){pool_recovery.suppressed=true;}
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
                    let may_allocate = predecessors_released
                        && endpoints_ready
                        && !provider_error
                        && pending_providers == 0;
                    if may_allocate
                        && pool.is_none()
                        && pool_recovery.due(Instant::now())
                        && (workers.values().any(|worker| !worker.derived)
                            || pool_recovery.requested)
                    {
                        pool_recovery.attempted();
                        pool = Pool::start(
                            &root,
                            instance,
                            resources.snapshot().inference_threads_limit,
                        )
                        .ok();
                        if pool.is_none() {
                            pool_recovery.failed(Instant::now());
                        if registry.as_mut().is_none_or(|r|crate::runtime_pair::engine_identity().and_then(|engine|r.recovery_failed("helper",engine,unix_ms()?)).is_err()){pool_recovery.suppressed=true;}
                            log.record(airk_core::service_logs::LogKind::ModelFailed, None, None);
                        }
                    }
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
                        if workers.get_mut(&project).is_some_and(|worker| {
                            worker
                                .child
                                .try_wait()
                                .is_ok_and(|status| status.is_some_and(|status| status.success()))
                        }) {
                            backoff.remove(&project);
                        } else {
                            worker_failed(&root, &mut backoff, project, Instant::now());
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
                    let mut clean_renewals = std::collections::BTreeSet::new();
                    let mut pool_control_failed = false;
                    for (project, worker) in &mut workers {
                        if pool_control_failed {
                            continue;
                        }
                        let desired_pool = pool
                            .as_ref()
                            .filter(|_| !worker.derived)
                            .map(Pool::identity);
                        if worker.pool_binding != desired_pool {
                            let access = (|| -> Result<_> {
                                match pool.as_mut() {
                                    Some(pool) => Ok(Some(
                                        pool.access(
                                            worker
                                                .identity
                                                .context("project identity missing")?
                                                .worker,
                                            registry
                                                .as_ref()
                                                .context("registry unavailable")?
                                                .get(*project)?
                                                .registration,
                                        )?,
                                    )),
                                    None => Ok(None),
                                }
                            })();
                            let access = match access {
                                Ok(access) => access,
                                Err(_) => {
                                    // The failed authority is the helper, not this project.
                                    // Drain it before retrying any ambiguous control exchange.
                                    pool_control_failed = true;
                                    continue;
                                }
                            };
                            if let Err(error) = worker.configure_pool(access) {
                                if clean_disconnected_exit(&mut worker.child, &error) {
                                    clean_renewals.insert(*project);
                                }
                                failed_renewals.push(*project);
                                continue;
                            }
                        }
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
                        if let Err(error) = renewed {
                            if clean_disconnected_exit(&mut worker.child, &error) {
                                clean_renewals.insert(*project);
                            }
                            failed_renewals.push(*project);
                            continue;
                        }
                        if let Some(pool) = pool.as_mut()
                            && worker.pool_binding == Some(pool.identity())
                        {
                            pool_control_failed = pool.renew(identity.worker).is_err();
                        }
                    }
                    if pool_control_failed {
                        drop(pool.take());
                        pool_recovery.failed(Instant::now());
                        if registry.as_mut().is_none_or(|r|crate::runtime_pair::engine_identity().and_then(|engine|r.recovery_failed("helper",engine,unix_ms()?)).is_err()){pool_recovery.suppressed=true;}
                        log.record(airk_core::service_logs::LogKind::ModelFailed, None, None);
                    }
                    for project in failed_renewals {
                        let clean = clean_renewals.contains(&project);
                        if clean {
                            backoff.remove(&project);
                        } else {
                            worker_failed(&root, &mut backoff, project, Instant::now());
                        }
                        log.record(
                            if clean {
                                airk_core::service_logs::LogKind::WorkerStopped
                            } else {
                                airk_core::service_logs::LogKind::WorkerFailed
                            },
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
                            let unregistered = registry.as_ref().is_some_and(|registry| {
                                matches!(registry.is_registered(project), Ok(false))
                            });
                            let response = (|| {
                                ensure!(!unregistered, "project is unregistered");
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
                                    false,
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
                                code: if unregistered {
                                    "project_unregistered"
                                } else if may_allocate {
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
                                    worker: pool.as_ref().map(Pool::identity),
                                    loads,
                                    reuses,
                                    sessions,
                                    observation_age_ms: Some(age_ms),
                                },
                                None => ControlResponse::EmbeddingStatus {
                                    available: false,
                                    process_id: None,
                                    worker: None,
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
                        Ok(Envelope::RestartProject {
                            project,
                            expected,
                            reply,
                        }) => {
                            let current = workers.get(&project).and_then(|worker| worker.identity);
                            let response = if current != expected {
                                ControlResponse::Failure {
                                    code: "project_worker_changed".into(),
                                }
                            } else if registry
                                .as_ref()
                                .is_none_or(|registry| registry.get(project).is_err())
                            {
                                ControlResponse::Failure {
                                    code: "project_unavailable".into(),
                                }
                            } else if restarting.len() >= 32 && !restarting.contains_key(&project) {
                                ControlResponse::Failure {
                                    code: "project_restart_queue_full".into(),
                                }
                            } else {
                                if let std::collections::btree_map::Entry::Vacant(entry) =
                                    restarting.entry(project)
                                {
                                    if registry.as_mut().is_none_or(|r|r.reset_recovery_budget(&project.to_string()).is_err()) {
                                        let _=reply.send(ControlResponse::Failure{code:"recovery_retry_not_saved".into()});continue;
                                    }
                                    entry.insert(expected);
                                    backoff.remove(&project);
                                }
                                if let Ok(mut cache) = resource_cache.lock() {
                                    cache.restarting.insert(project);
                                }
                                ControlResponse::ProjectRestarting {
                                    project,
                                    previous: expected,
                                }
                            };
                            let _ = reply.send(response);
                        }
                        Ok(Envelope::StartEmbedding { reply }) => {
                            let response = if pool.is_some() {
                                ControlResponse::Failure {
                                    code: "embedding_already_running".into(),
                                }
                            } else {
                                if registry.as_mut().is_none_or(|r|r.reset_recovery_budget("helper").is_err()) {
                                    let _=reply.send(ControlResponse::Failure{code:"recovery_retry_not_saved".into()});continue;
                                }
                                pool_recovery = PoolRecovery::new(Instant::now());
                                pool_recovery.requested = true;
                                ControlResponse::EmbeddingStarting
                            };
                            let _ = reply.send(response);
                        }
                        Ok(Envelope::StopEmbedding {
                            worker,
                            force,
                            reply,
                        }) => {
                            let response = if pool
                                .as_ref()
                                .is_some_and(|pool| pool.identity() == worker)
                            {
                                // The actual native owner drains its child/job.
                                // Caller-supplied numerical PIDs never authorize a signal.
                                if registry.as_mut().is_none_or(|r|crate::runtime_pair::engine_identity().and_then(|engine|r.suppress_helper_recovery(engine)).is_err()) {
                                    let _=reply.send(ControlResponse::Failure{code:"embedding_stop_not_saved".into()});continue;
                                }
                                pool_recovery.suppressed = true;
                                let failed = force
                                    && pool.as_mut().is_some_and(|pool| pool.terminate().is_err());
                                drop(pool.take());
                                log.record(
                                    airk_core::service_logs::LogKind::ModelStopped,
                                    None,
                                    None,
                                );
                                if failed {
                                    ControlResponse::Failure {
                                        code: "embedding_stop_outcome_unknown".into(),
                                    }
                                } else {
                                    ControlResponse::EmbeddingStopped { worker }
                                }
                            } else {
                                ControlResponse::Failure {
                                    code: "embedding_worker_changed".into(),
                                }
                            };
                            let _ = reply.send(response);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if let Some(registry) = registry.as_mut() {
                        if let Ok(due) = registry.due_background_jobs(background_after, 8) {
                            background_after = if due.len() == 8 {
                                due.last().map(|item| item.0)
                            } else {
                                None
                            };
                            for (project, revision, kind) in due {
                                if let Ok(nonce) = new_instance_id() {
                                    let _ = registry.enqueue_background_job(
                                        project, revision, kind, instance, nonce,
                                    );
                                }
                            }
                        }
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
                        && let Some(project) = restarting
                            .iter()
                            .find_map(|(project, expected)| expected.is_none().then_some(*project))
                        && let Some(registry) = registry.as_ref()
                    {
                        if let Ok(record) = registry.get(project) {
                            let _ = start_worker(
                                &mut workers,
                                &root,
                                record.registration,
                                instance,
                                false,
                                &mut backoff,
                                WorkerAdmission {
                                    log: &log,
                                    policy: &policy,
                                    pool: &mut pool,
                                    resources: &mut resources,
                                },
                            );
                        }
                        if workers.contains_key(&project)
                            || backoff
                                .get(&project)
                                .is_some_and(|(_, attempts)| *attempts >= 3)
                        {
                            restarting.remove(&project);
                        }
                    }
                    if let Some(registry) = registry.as_ref()
                        && let Ok(pending) = registry.pending_project_page(pending_after, 32)
                    {
                        pending_after = pending.last().copied();
                        for project in pending {
                            if let Ok(Some(job)) = registry.pending_job_observation(project)
                                && resources.running(project).is_none() {
                                let reason = if configuration_error {
                                    airk_core::service_resources::WaitReason::ResourcePolicy
                                } else if !may_allocate {
                                    airk_core::service_resources::WaitReason::ProviderDrain
                                } else {
                                    resources.work_wait_reason(project).unwrap_or(airk_core::service_resources::WaitReason::Scheduler)
                                };
                                sample_wait(&mut waiting, project, job.id, reason);
                            }
                            if !may_allocate || restarting.contains_key(&project) {
                                continue;
                            }
                            let derived = registry.derived_work_pending(project).unwrap_or(false);
                            let Ok(registration) = (if derived {
                                registry.retained_project(project)
                            } else {
                                registry.get(project)
                            })
                            .map(|record| record.registration) else {
                                continue;
                            };
                            let _ = start_worker(
                                &mut workers,
                                &root,
                                registration,
                                instance,
                                derived,
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
                            if restarting.contains_key(&project) {
                                resources.forget_pending(project);
                                continue;
                            }
                            match registry.next_pending_job(project) {
                                Ok(Some(job)) => {
                                    let derived = registry
                                        .configuration_work(job.id)
                                        .ok()
                                        .flatten()
                                        .is_some_and(|work| {
                                            work.request.configuration.is_derived_cleanup()
                                        });
                                    if workers
                                        .get(&project)
                                        .is_none_or(|worker| worker.derived != derived)
                                    {
                                        sample_wait(&mut waiting, project, job.id, airk_core::service_resources::WaitReason::WorkerRoleChange);
                                        resources.forget_pending(project);
                                        continue;
                                    }
                                    // A newly admitted worker may precede the first helper turn.
                                    // Wait through finite recovery, then expose failure normally;
                                    // source jobs never depend on this inference-only gate.
                                    let needs_pool = matches!(
                                        job.kind,
                                        airk_core::service::JobKind::ModelSetup
                                            | airk_core::service::JobKind::Vectors
                                    ) || job.kind
                                        == airk_core::service::JobKind::Configure
                                        && registry
                                            .configuration_work(job.id)
                                            .ok()
                                            .flatten()
                                            .is_some_and(|work| {
                                                work.request.configuration.needs_inference()
                                            });
                                    if needs_pool
                                        && workers
                                            .get(&project)
                                            .is_some_and(|worker| worker.pool_binding.is_none())
                                        && !pool_recovery.suppressed
                                        && pool_recovery.attempts < 3
                                    {
                                        sample_wait(&mut waiting, project, job.id, airk_core::service_resources::WaitReason::EmbeddingRecovery);
                                        resources.forget_pending(project);
                                        continue;
                                    }
                                    let _ = resources.pending(
                                        project,
                                        job.id,
                                        job.kind,
                                        Instant::now(),
                                    );
                                    if !may_allocate {
                                        sample_wait(&mut waiting, project, job.id, airk_core::service_resources::WaitReason::ProviderDrain);
                                    } else if let Some(reason) = resources.work_wait_reason(project) {
                                        sample_wait(&mut waiting, project, job.id, reason);
                                    }
                                }
                                Ok(None) => resources.forget_pending(project),
                                Err(_) => {}
                            }
                        }
                    }
                    let mut failed = Vec::new();
                    let mut clean_exits = std::collections::BTreeSet::new();
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
                                        || (registry.retained_project(*project).ok()?.paused
                                            && !registry.job_runs_while_paused(grant.job).ok()?),
                                    canceled: plan_canceled
                                        || registry.cancellation_requested(grant.job).ok()?,
                                })
                            })
                        });
                        let keep_for_maintenance = registry.as_ref().is_some_and(|registry| {
                            registry.maintenance_retrying(*project).unwrap_or(false)
                        });
                        let freshness = registry.as_ref().and_then(|registry| {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .ok()?
                                .as_millis();
                            registry
                                .source_freshness(*project, u64::try_from(now).ok()?)
                                .ok()
                        });
                        match worker.work_status(control, keep_for_maintenance, freshness) {
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
                                    // A job can fail while its worker remains healthy.
                                    // Preserve its exact durable result before releasing
                                    // the grant; process-exit logs alone miss this case.
                                    if let Some(grant) = resources.running(*project)
                                        && let Some(registry) = registry.as_ref()
                                        && let Ok(job) = registry.job(grant.job)
                                        && job.project == *project
                                    {
                                        if let Some(failure) = observation.failure.as_ref().filter(|failure|
                                            failure.job == job.id && failure.grant == finished && failure.worker == identity.worker && failure.operation == job.kind && failure.validate().is_ok()) {
                                            // Deferred retries have no terminal failed receipt
                                            // yet. Keep this attempt's error before releasing
                                            // the grant as well as terminal job failures.
                                            log.job_failure(*project, failure.clone());
                                        } else if let Some(airk_core::service::JobOutcome::Failed { code }) = &job.outcome {
                                            log.record_code(
                                            airk_core::service_logs::LogKind::JobFailed,
                                            Some(*project), Some(job.id),
                                            Some(code),
                                        );
                                        }
                                    }
                                    let _ = resources.finished(identity, finished);
                                }
                            }
                            Err(error) => {
                                let clean = clean_disconnected_exit(&mut worker.child, &error);
                                if clean {
                                    clean_exits.insert(*project);
                                }
                                failed.push(*project);
                            }
                        }
                    }
                    loop {
                        let Ok(Some(grant)) = resources.next(Instant::now()) else {
                            break;
                        };
                        let project = grant.identity.project;
                        if let Some(worker) = workers.get_mut(&project) {
                            if let Err(error) = worker.run_job(grant) {
                                if clean_disconnected_exit(&mut worker.child, &error) {
                                    clean_exits.insert(project);
                                }
                                failed.push(project);
                            }
                        } else {
                            failed.push(project);
                        }
                    }
                    for project in failed {
                        let clean = clean_exits.contains(&project);
                        if clean {
                            backoff.remove(&project);
                        } else {
                            worker_failed(&root, &mut backoff, project, Instant::now());
                        }
                        log.record(
                            if clean {
                                airk_core::service_logs::LogKind::WorkerStopped
                            } else {
                                airk_core::service_logs::LogKind::WorkerFailed
                            },
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
    pub fn workers_status(&self, projects: &[ProjectBinding]) -> ControlResponse {
        let unique: std::collections::BTreeSet<_> = projects.iter().copied().collect();
        if projects.len() > 32 || unique.len() != projects.len() {
            return ControlResponse::Failure {
                code: "invalid_worker_observation_scope".into(),
            };
        }
        match self.resources.lock() {
            Ok(cache) => ControlResponse::Workers {
                observations: projects
                    .iter()
                    .map(|project| airk_core::service_console::WorkerObservation {
                        project: *project,
                        identity: cache.workers.get(project).copied(),
                        restarting: cache.restarting.contains(project),
                        automatic_start_blocked: cache.blocked.contains(project),
                        waiting: cache.wait_observation(*project),
                    })
                    .collect(),
                age_ms: cache
                    .observed
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
            },
            Err(_) => ControlResponse::Failure {
                code: "worker_observation_unavailable".into(),
            },
        }
    }
    pub fn worker_status(&self, project: ProjectBinding) -> ControlResponse {
        match self.resources.lock() {
            Ok(cache) => ControlResponse::Worker {
                project,
                identity: cache.workers.get(&project).copied(),
                restarting: cache.restarting.contains(&project),
                automatic_start_blocked: cache.blocked.contains(&project),
                waiting: cache.wait_observation(project),
                age_ms: cache
                    .observed
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
            },
            Err(_) => ControlResponse::Failure {
                code: "worker_observation_unavailable".into(),
            },
        }
    }
    pub async fn restart_project(
        &self,
        project: ProjectBinding,
        expected: Option<WorkerIdentity>,
    ) -> ControlResponse {
        let (reply, receiver) = oneshot::channel();
        if self.sender.as_ref().is_none_or(|queue| {
            queue
                .try_send(Envelope::RestartProject {
                    project,
                    expected,
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
                code: "project_restart_outcome_unknown".into(),
            },
        }
    }
    pub async fn start_embedding(&self) -> ControlResponse {
        let (reply, receiver) = oneshot::channel();
        if self
            .sender
            .as_ref()
            .is_none_or(|queue| queue.try_send(Envelope::StartEmbedding { reply }).is_err())
        {
            return ControlResponse::Failure {
                code: "broker_busy".into(),
            };
        }
        match tokio::time::timeout(Duration::from_secs(4), receiver).await {
            Ok(Ok(response)) => response,
            _ => ControlResponse::Failure {
                code: "embedding_start_outcome_unknown".into(),
            },
        }
    }
    pub async fn stop_embedding(&self, worker: Incarnation, force: bool) -> ControlResponse {
        let (reply, receiver) = oneshot::channel();
        if self.sender.as_ref().is_none_or(|queue| {
            queue
                .try_send(Envelope::StopEmbedding {
                    worker,
                    force,
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
                code: "embedding_stop_outcome_unknown".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_samples_replace_jobs_and_bound_metadata_without_renewing_old_samples() {
        use airk_core::{identity::Sha256Digest, service_resources::WaitReason};
        let mut samples = BTreeMap::new();
        let first = Sha256Digest::from_bytes([0; 32]);
        sample_wait(&mut samples, first, first, WaitReason::Scheduler);
        let old = Instant::now() - Duration::from_secs(10);
        samples.get_mut(&first).unwrap().observed = old;
        for n in 1_u32..=1024 {
            let mut bytes = [0; 32];
            bytes[..4].copy_from_slice(&n.to_le_bytes());
            let project = Sha256Digest::from_bytes(bytes);
            sample_wait(&mut samples, project, project, WaitReason::ResidentCapacity);
        }
        assert_eq!(samples.len(), 1024);
        assert!(!samples.contains_key(&first));
        let project = *samples.keys().next().unwrap();
        sample_wait(&mut samples, project, first, WaitReason::EmbeddingRecovery);
        assert_eq!(samples[&project].job, first);
        assert_eq!(samples[&project].reason, WaitReason::EmbeddingRecovery);
        assert_eq!(samples.len(), 1024);
    }

    #[cfg(unix)]
    #[test]
    fn idle_pipe_exit_fixture() -> Result<()> {
        let Some(marker) = std::env::var_os("AIRK_IDLE_PIPE_EXIT_FIXTURE") else {
            return Ok(());
        };
        // This runs only in the explicitly spawned test process.
        unsafe {
            libc::close(libc::STDIN_FILENO);
        }
        std::fs::write(marker, b"control closed")?;
        std::thread::sleep(Duration::from_millis(200));
        std::process::exit(std::env::var("AIRK_IDLE_PIPE_EXIT_CODE")?.parse()?);
    }

    #[cfg(unix)]
    #[test]
    fn disconnected_control_requires_observed_success_without_signaling() -> Result<()> {
        use std::io::Write;
        struct Owned(Child);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let owner = tempfile::tempdir()?;
        for code in [0, 7] {
            let marker = owner.path().join(format!("ready-{code}"));
            let mut child = Owned(
                Command::new(std::env::current_exe()?)
                    .args([
                        "--exact",
                        "project_supervisor::tests::idle_pipe_exit_fixture",
                        "--nocapture",
                    ])
                    .env("AIRK_IDLE_PIPE_EXIT_FIXTURE", &marker)
                    .env("AIRK_IDLE_PIPE_EXIT_CODE", code.to_string())
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?,
            );
            let until = Instant::now() + Duration::from_secs(10);
            while !marker.try_exists()? {
                ensure!(Instant::now() < until, "owned idle fixture did not start");
                std::thread::sleep(Duration::from_millis(10));
            }
            let error = child
                .0
                .stdin
                .as_mut()
                .context("fixture stdin missing")?
                .write_all(b"status")
                .unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
            assert_eq!(
                clean_disconnected_exit(&mut child.0, &error.into()),
                code == 0
            );
            assert!(child.0.try_wait()?.is_some());
            assert!(!clean_disconnected_exit(
                &mut child.0,
                &anyhow::anyhow!("invalid protocol reply")
            ));
        }
        Ok(())
    }
    #[test]
    fn helper_recovery_is_bounded_and_manual_stop_remains_stopped() {
        let now = Instant::now();
        let mut recovery = PoolRecovery::new(now);
        for attempt in 1..=3 {
            assert!(recovery.due(now + Duration::from_secs(30 * attempt)));
            recovery.attempted();
            let failed = now + Duration::from_secs(30 * attempt);
            recovery.failed(failed);
            assert!(!recovery.due(failed));
        }
        assert!(!recovery.due(now + Duration::from_secs(3600)));
        let mut explicit = PoolRecovery::new(now);
        explicit.suppressed = true;
        assert!(!explicit.due(now + Duration::from_secs(3600)));
        explicit = PoolRecovery::new(now);
        assert!(explicit.due(now));
    }
}
