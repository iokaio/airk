// SPDX-License-Identifier: Apache-2.0
//! Finite child supervision; allocation survives timeout until child exit is proven.
use crate::{
    local_ipc,
    project_protocol::{self as protocol, Command as WorkerCommand, Reply},
    service_ipc,
};
use airk_core::{
    identity::Incarnation,
    service::{ControlResponse, MAX_FRAME_BYTES, ProjectBinding, QueryAllowance, WorkerIdentity},
};
use airk_kernel::service_broker::{Broker, new_instance_id};
use airk_store::service::{ProjectLease, registry::ServiceRegistry};
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

const MAX_WORKERS: usize = 4;
struct ChildWorker {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Reply>,
    reader: Option<std::thread::JoinHandle<()>>,
    identity: Option<WorkerIdentity>,
    role: String,
    started: Instant,
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
        let mut child = command.spawn().context("start project owner")?;
        let input = child.stdin.take().context("project input missing")?;
        let mut output = child.stdout.take().context("project output missing")?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut process = Self {
            child,
            input: Some(input),
            output: receiver,
            reader: None,
            identity: None,
            role: protocol::role(worker),
            started: Instant::now(),
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

    fn activate(&mut self, identity: WorkerIdentity, allowance: QueryAllowance) -> Result<()> {
        service_ipc::write_frame(
            self.input.as_mut().context("project input missing")?,
            &WorkerCommand::Activate {
                identity,
                allowance,
            },
        )?;
        ensure!(
            matches!(
                self.output.recv_timeout(Duration::from_secs(10))?,
                Reply::Activated
            ),
            "project activation failed"
        );
        Ok(())
    }
}

type Envelope = (ProjectBinding, oneshot::Sender<ControlResponse>);
pub struct Supervisor {
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

fn start_worker(
    workers: &mut BTreeMap<ProjectBinding, ChildWorker>,
    root: &Path,
    project: ProjectBinding,
    instance: Incarnation,
    policy: &Mutex<Broker>,
    backoff: &mut BTreeMap<ProjectBinding, (Instant, u32)>,
) -> Result<()> {
    if workers.contains_key(&project) {
        return Ok(());
    }
    ensure!(workers.len() < MAX_WORKERS, "project capacity busy");
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
            return Err(error);
        }
    };
    let identity = worker.identity.context("missing worker identity")?;
    let allowance = QueryAllowance {
        concurrent_queries: 4,
        response_bytes: 4 * MAX_FRAME_BYTES,
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
    if let Err(error) = worker.activate(identity, allowance) {
        drop(worker);
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
        return Err(error);
    }
    workers.insert(project, worker);
    Ok(())
}

impl Supervisor {
    pub fn start(root: PathBuf, instance: Incarnation, policy: Arc<Mutex<Broker>>) -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Envelope>(32);
        let thread = std::thread::Builder::new()
            .name("airk-project-supervisor".into())
            .spawn(move || {
                let mut workers = BTreeMap::<ProjectBinding, ChildWorker>::new();
                let mut registry = None;
                let mut backoff = BTreeMap::<ProjectBinding, (Instant, u32)>::new();
                let mut predecessors: Option<Vec<ProjectBinding>> = None;
                loop {
                    if registry.is_none() {
                        registry = ServiceRegistry::open(&root, instance).ok();
                    }
                    if predecessors.is_none()
                        && let Some(registry) = registry.as_ref()
                    {
                        predecessors = registry.bindings().ok().map(|bindings| {
                            bindings
                                .into_iter()
                                .filter(|binding| ProjectLease::acquire(&root, *binding).is_err())
                                .collect()
                        });
                    }
                    if let Some(predecessors) = predecessors.as_mut() {
                        predecessors
                            .retain(|binding| ProjectLease::acquire(&root, *binding).is_err());
                    }
                    // A restarted service must not reassign unknown old query/model capacity.
                    // Parent EOF stops old admission, but only OS lock release proves drain.
                    let may_allocate = predecessors.as_ref().is_some_and(Vec::is_empty);
                    let finished: Vec<_> = workers
                        .iter_mut()
                        .filter_map(|(project, worker)| {
                            matches!(worker.child.try_wait(), Ok(Some(_))).then_some(*project)
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
                        if let Some(worker) = workers.remove(&project)
                            && let (Some(identity), Ok(mut policy)) =
                                (worker.identity, policy.lock())
                        {
                            let _ = policy.process_exited(identity);
                        }
                    }
                    match receiver.recv_timeout(Duration::from_millis(250)) {
                        Ok((project, reply)) => {
                            let response = (|| {
                                ensure!(may_allocate, "previous project workers still draining");
                                registry
                                    .as_ref()
                                    .context("service registry unavailable")?
                                    .get(project)?;
                                start_worker(
                                    &mut workers,
                                    &root,
                                    project,
                                    instance,
                                    &policy,
                                    &mut backoff,
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
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if may_allocate
                        && let Some(registry) = registry.as_ref()
                        && let Ok(pending) = registry.pending_projects(32)
                    {
                        for project in pending {
                            if workers.len() >= MAX_WORKERS {
                                break;
                            }
                            let _ = start_worker(
                                &mut workers,
                                &root,
                                project,
                                instance,
                                &policy,
                                &mut backoff,
                            );
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
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    pub async fn connect(&self, project: ProjectBinding) -> ControlResponse {
        let (sender, receiver) = oneshot::channel();
        if self
            .sender
            .as_ref()
            .is_none_or(|queue| queue.try_send((project, sender)).is_err())
        {
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
}
