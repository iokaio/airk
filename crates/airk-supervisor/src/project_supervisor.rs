// SPDX-License-Identifier: Apache-2.0
//! Finite child supervision; allocation survives timeout until child exit is proven.
use crate::{
    local_ipc,
    project_protocol::{self as protocol, Command as WorkerCommand, Reply},
    service_inference::Pool,
    service_ipc,
    worker_host::{ObservedExit, OwnerThread, StopStep, WorkerHost},
    worker_transport::{FrameSink, FrameSource},
};
// Only the child-process owner fixtures still start processes here.
#[cfg(test)]
use crate::worker_transport::StreamFrames;
use airk_core::{
    identity::Incarnation,
    service::{ControlResponse, MAX_FRAME_BYTES, ProjectBinding, QueryAllowance, WorkerIdentity},
};
use airk_kernel::service_broker::{Broker, new_instance_id};
use airk_store::service::{EmbeddingLease, ProjectLease, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
#[cfg(test)]
use std::process::{Command, Stdio};
// `Child` is named only by the Unix fixture in `tests` and by the
// local-inference qualification tests (through `super::*`).
#[cfg(all(test, any(unix, feature = "local-inference")))]
use std::process::Child;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

mod helper_recovery;
use helper_recovery::PoolRecovery;
#[cfg(all(test, feature = "local-inference"))]
pub(crate) use helper_recovery::PoolRecovery as TestPoolRecovery;
mod worker_resource_drains;
use worker_resource_drains::{DrainProgress, WorkerResourceDrains};
mod worker_read_settlement;
use worker_read_settlement::ReadSettlement;
mod catalog;
#[cfg(all(test, feature = "local-inference"))]
mod guarded_qualification_tests;
#[cfg(test)]
mod pool_tests;
#[cfg(all(test, feature = "local-inference"))]
mod provider_restart_tests;
#[cfg(test)]
mod worker_exit_capacity_tests;
pub use catalog::{
    AdministrativeKey, CatalogControlHandle, NativeAdministrativeConnection,
    NativeAdministrativeRead, NativeLegacyAuthoritySelection, ScopedConnectionDelivery,
};
pub type NativeCatalogCoordinator =
    airk_catalog_control::catalog_coordinator::CatalogCoordinator<CatalogControlHandle>;

/// Explicit pre-query setup admission. Accepted catalog exchanges and unknown
/// worker outcomes never construct this type.
#[derive(Debug, Clone, Copy)]
pub enum ConnectionSetupPending {
    Queue,
    Draining,
    ResidentCapacity,
    RestartBackoff,
}
impl ConnectionSetupPending {
    pub fn code(self) -> &'static str {
        match self {
            Self::Queue => "catalog_connection_busy",
            Self::Draining => "previous_workers_draining",
            Self::ResidentCapacity => "project_capacity_busy",
            Self::RestartBackoff => "project_restart_backoff",
        }
    }
}
impl std::fmt::Display for ConnectionSetupPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for ConnectionSetupPending {}

struct ChildWorker {
    exiting: bool,
    read_settlement: Option<ReadSettlement>,
    catalog_gate: Option<airk_catalog_control::catalog_admission::NativeCatalogAdmission>,
    catalog_pending: Option<catalog::Pending>,
    catalog_quarantined: bool,
    derived: bool,
    /// Present only for a child host; an in-process owner has no job object.
    _containment: Option<airk_host::child_job::ChildJob>,
    child: WorkerHost,
    /// The in-process owner's slots, used to ask it to drain.
    scope: Option<crate::worker_scope::WorkerScope>,
    input: Option<Box<dyn FrameSink<WorkerCommand>>>,
    output: Receiver<Reply>,
    reader: Option<std::thread::JoinHandle<()>>,
    stderr: Option<airk_host::process_diagnostics::WorkerStderr>,
    identity: Option<WorkerIdentity>,
    role: String,
    runtime: PathBuf,
    last_renewal: Instant,
    pool_binding: Option<Incarnation>,
    provider_helpers: Vec<Incarnation>,
    provider_in_process: bool,
    #[cfg(feature = "local-inference")]
    provider_completed: Option<airk_inference::inference_completion_ledger::NativeLedgerWitness>,
    pool_pending: bool,
    last_status: Instant,
    waiting: Option<(
        Incarnation,
        airk_core::service_resources::WaitReason,
        Instant,
    )>,
}

#[cfg(feature = "local-inference")]
pub(crate) fn complete_provider_authorizations(
    service: Incarnation,
    in_process: bool,
    generations: &mut Vec<Incarnation>,
    completed: &mut Option<airk_inference::inference_completion_ledger::NativeLedgerWitness>,
    event: &airk_inference::service_inference::CompletedProvider,
) -> Result<()> {
    let provider = event.completion.observation().provider;
    if !in_process || service != provider.service_instance {
        return Ok(());
    }
    if generations.contains(&provider.provider_generation) || completed.is_some() {
        ensure!(
            event
                .completion
                .matches(service, provider.provider_generation)
                && event
                    .ledger
                    .matches(event.ledger.observation().namespace, service)
                && event.ledger.observation().approved_execution_sha256
                    == provider.approved_execution_sha256,
            "crossed native provider completion delivery"
        );
        generations.retain(|generation| *generation != provider.provider_generation);
        *completed = Some(event.ledger.clone());
    }
    Ok(())
}

fn clean_disconnected_exit(child: &mut impl ObservedExit, error: &anyhow::Error) -> bool {
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
        match child.observed_exit() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => return false,
        }
    }
}

/// The existing stop period an owner has to exit on its own after its stop
/// request. A child is then killed; an in-process owner is asked to drain.
const OWNER_STOP_GRACE: Duration = Duration::from_secs(10);
/// The further time an in-process owner has, after the existing 10-second
/// stop period, to reach a safe point once asked to drain. It covers the
/// 15-second bound of an isolated PDF parse that may be in progress. An owner
/// still running after both is quarantined, never killed or released.
const IN_PROCESS_DRAIN: Duration = Duration::from_secs(20);
/// Diagnostic code of an owner that did not stop within both periods.
const STOP_DEADLINE_CODE: &str = "owner_quarantined_stop_deadline";
/// Diagnostic code recorded when a quarantined owner's thread later returned.
const QUARANTINE_RELEASED_CODE: &str = "owner_quarantine_released";
/// An in-process project owner's thread stack, matching the CLI thread that
/// ran a child project owner.
const OWNER_STACK_BYTES: usize = 8 << 20;
/// In-process control frames queue like bytes in a child's pipes.
const CONTROL_FRAMES: usize = 16;
fn retire_helper(
    pool: &mut Option<Pool>,
    gate: Option<&airk_catalog_control::catalog_admission::NativeCatalogAdmission>,
    recovery: &mut PoolRecovery,
) {
    if let Some(mut old) = pool.take() {
        #[cfg(feature = "local-inference")]
        if old.terminate().is_err() {
            // A pending FFI call cannot be killed or unloaded. Keep its actual
            // owner and charged resources for the next bounded actor turn.
            *pool = Some(old);
            return;
        }
        #[cfg(not(feature = "local-inference"))]
        while old.terminate().is_err() {
            std::thread::sleep(Duration::from_millis(100));
        }
        // Modern terminate queues the joined, durably appended completion in
        // its factory. The actor ingests/acknowledges that owned event before
        // discarding any live worker authorization generation.
        if let Some(witness) = old.reap_witness() {
            let retained = match gate {
                Some(gate) => gate.observe_provider_reaped(witness),
                None => recovery.record_reaped(witness),
            };
            if retained.is_err() {
                // Preserve the actual native witness in its owning object on
                // explicit bounded-history refusal; never silently truncate it.
                *pool = Some(old);
                return;
            }
        }
        drop(old);
    }
}

/// Dropping an owner never waits for an in-process one. Only an owner whose
/// exit was already observed is finalized here; the supervisor keeps every
/// other stopping in-process owner in its stopping collection instead, so a
/// running owner reaches this only if it was dropped outside that path.
impl Drop for ChildWorker {
    fn drop(&mut self) {
        if self.child.in_process() {
            // Closing the control carrier is the stop request. Sending a frame
            // could block on an owner that no longer reads its carrier.
            self.input.take();
            if !matches!(self.child.try_wait(), Ok(Some(_))) {
                // Not observed to exit: its own thread keeps its lease, and no
                // reap is reported, so nothing it holds is released here.
                if let Some(scope) = &self.scope {
                    scope.execution.drain();
                }
                return;
            }
        } else {
            self.stop_child();
        }
        self.finish_exit();
    }
}

impl ChildWorker {
    /// A child fixture keeps its process semantics: Stop, the grace period,
    /// then a contained kill, waiting until its exit is actually observed.
    fn stop_child(&mut self) {
        if let Some(mut input) = self.input.take() {
            let _ = input.send(WorkerCommand::Stop);
        }
        let requested = Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                // Unknown wait state never authorizes a numerical PID signal.
                // Retain ownership/admission until native exit is observable.
                Err(_) => std::thread::sleep(Duration::from_millis(25)),
                Ok(None) => match crate::worker_host::stop_step(
                    false,
                    requested,
                    Instant::now(),
                    OWNER_STOP_GRACE,
                    IN_PROCESS_DRAIN,
                    false,
                ) {
                    StopStep::Kill => {
                        if let Some(child) = self.child.child_mut() {
                            let _ = child.kill();
                            if child.wait().is_ok() {
                                break;
                            }
                        }
                        // Failed native wait is uncertainty, never a reap receipt
                        // or permission to release accepted resource ownership.
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    // A child is only ever waited for or killed.
                    StopStep::Wait | StopStep::Drain | StopStep::Quarantine => {
                        std::thread::sleep(Duration::from_millis(25))
                    }
                },
            }
        }
    }

    /// The in-process owner's own report that it cannot reach a safe point.
    fn quarantine_reason(&self) -> Option<&'static str> {
        self.scope
            .as_ref()?
            .execution
            .stuck()
            .map(airk_kernel::service_execution::StuckReason::code)
    }

    /// Release what an owner's observed exit proves is no longer in use.
    fn finish_exit(&mut self) {
        if let Some(containment) = &self._containment {
            containment.drain();
        }
        #[cfg(unix)]
        if !self.child.in_process()
            && let Some(identity) = self.identity
        {
            airk_inference::unix_guardian::wait_for_worker(&self.runtime, identity);
        }
        let _ = local_ipc::remove_exited_endpoint(&self.runtime, &self.role);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(stderr) = self.stderr.take()
            && let Some(child) = self.child.child_mut()
        {
            stderr.finish(child);
        }
        if let (Some(gate), Some(identity)) = (&self.catalog_gate, self.identity) {
            let _ = gate.observe_worker_reaped(identity);
            let _ = gate.observe_worker_provider_history_completed(
                identity,
                self.provider_helpers.clone(),
                self.provider_in_process,
                self.completed_authorizations(),
            );
        }
    }
}

/// Replies queue one at a time; an undrained reply closes the carrier, as a
/// child's reply reader always did.
fn spawn_reply_reader(
    mut output: Box<dyn FrameSource<Reply>>,
    sender: SyncSender<Reply>,
) -> Result<std::thread::JoinHandle<()>> {
    Ok(std::thread::Builder::new()
        .name("airk-project-replies".into())
        .spawn(move || {
            while let Ok(Some(reply)) = output.recv() {
                if sender.try_send(reply).is_err() {
                    break;
                }
            }
        })?)
}

/// The body of an in-process owner's thread. Like a child process exit, it
/// returns only after the owner's runtime has finished every blocking task,
/// so the owner's lease is released before the supervisor observes the exit.
fn run_in_process(
    init: protocol::Init,
    commands: Box<dyn FrameSource<WorkerCommand>>,
    replies: Box<dyn FrameSink<Reply>>,
    scope: crate::worker_scope::WorkerScope,
    derived: bool,
    hooks: crate::project_worker::Hooks,
) -> Result<()> {
    /// Releases the owner's project lease last: after its runtime, and so
    /// every blocking task still holding owner state, has returned. This
    /// also runs when the body unwinds.
    struct LeaseRelease(crate::worker_scope::WorkerScope);
    impl Drop for LeaseRelease {
        fn drop(&mut self) {
            self.0.release_lease();
        }
    }
    let _release = LeaseRelease(scope.clone());
    let _bound = scope.bind_thread();
    let residency = crate::worker_scope::Residency::Service(scope.clone());
    if derived {
        return crate::derived_worker::serve(init, commands, replies, residency);
    }
    // The same two-worker runtime shape that served a child project owner.
    // Every runtime thread, including its blocking pool, binds this owner.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("airk-project-runtime")
        .on_thread_start(move || scope.bind_for_thread_life())
        .enable_all()
        .build()?;
    let result = runtime.block_on(crate::project_worker::serve(
        init, commands, replies, residency, hooks,
    ));
    drop(runtime);
    result
}

impl ChildWorker {
    fn completed_authorizations(
        &self,
    ) -> Option<airk_core::inference_provider::CompletionLedgerObservation> {
        #[cfg(feature = "local-inference")]
        return self
            .provider_completed
            .as_ref()
            .map(|proof| proof.observation().clone());
        #[cfg(not(feature = "local-inference"))]
        None
    }
    #[cfg(feature = "local-inference")]
    fn complete_provider(
        &mut self,
        event: &airk_inference::service_inference::CompletedProvider,
    ) -> Result<()> {
        let Some(identity) = self.identity else {
            return Ok(());
        };
        complete_provider_authorizations(
            identity.service,
            self.provider_in_process,
            &mut self.provider_helpers,
            &mut self.provider_completed,
            event,
        )
    }
    fn record_provider_delivery(
        &mut self,
        helper: Option<Incarnation>,
        in_process: bool,
    ) -> Result<()> {
        ensure!(!self.exiting, "native worker resources are draining");
        if let Some(helper) = helper
            && !self.provider_helpers.contains(&helper)
        {
            ensure!(
                self.provider_helpers.len() < 32,
                "native provider history capacity exceeded"
            );
            // Set BEFORE the credential can enter a worker stdin; a late or
            // missing reply never turns uncertain authorization into NEVER.
            ensure!(
                self.provider_helpers.is_empty() || self.provider_in_process == in_process,
                "native provider execution kind changed"
            );
            self.provider_in_process = in_process;
            self.provider_helpers.push(helper);
        }
        Ok(())
    }
    /// Production start: every project and cleanup owner runs in the service.
    fn spawn(
        root: &Path,
        project: ProjectBinding,
        service: Incarnation,
        derived: bool,
        hooks: crate::project_worker::Hooks,
    ) -> Result<Self> {
        Self::start_in_process(
            root,
            project,
            service,
            derived,
            airk_host::runtime_pair::engine_identity()?,
            hooks,
        )
    }

    /// One owner on a service thread. The control protocol, its frames, the
    /// handshake and every identity check are those of a child owner.
    fn start_in_process(
        root: &Path,
        project: ProjectBinding,
        service: Incarnation,
        derived: bool,
        engine: airk_core::identity::Sha256Digest,
        hooks: crate::project_worker::Hooks,
    ) -> Result<Self> {
        let worker = new_instance_id()?;
        let (commands, command_source) =
            crate::worker_transport::channel::<WorkerCommand>(CONTROL_FRAMES);
        let (replies, reply_source) = crate::worker_transport::channel::<Reply>(CONTROL_FRAMES);
        let scope = crate::worker_scope::WorkerScope::new();
        let init = protocol::Init {
            engine,
            runtime: root.into(),
            project,
            service,
            worker,
        };
        let owner_scope = scope.clone();
        let handle = std::thread::Builder::new()
            .name("airk-project-owner".into())
            .stack_size(OWNER_STACK_BYTES)
            .spawn(move || {
                run_in_process(
                    init,
                    Box::new(command_source),
                    Box::new(replies),
                    owner_scope,
                    derived,
                    hooks,
                )
            })
            .context("start project owner")?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut process = Self {
            exiting: false,
            read_settlement: None,
            catalog_gate: None,
            catalog_pending: None,
            catalog_quarantined: false,
            derived,
            child: WorkerHost::Thread(OwnerThread::new(handle)),
            _containment: None,
            scope: Some(scope),
            input: Some(Box::new(commands)),
            output: receiver,
            reader: None,
            stderr: None,
            identity: None,
            role: protocol::role(worker),
            runtime: root.into(),
            last_renewal: Instant::now(),
            pool_binding: None,
            provider_helpers: Vec::new(),
            provider_in_process: false,
            #[cfg(feature = "local-inference")]
            provider_completed: None,
            pool_pending: false,
            last_status: Instant::now(),
            waiting: None,
        };
        let started = (|| -> Result<()> {
            process.reader = Some(spawn_reply_reader(Box::new(reply_source), sender)?);
            process.finish_start(project, service, worker)
        })();
        if let Err(error) = started {
            // Never wait here for an owner that did not start. It stops on its
            // own thread, keeping its lease until then, and the supervisor
            // reaps it after observing that exit; its project stays draining.
            stop_in_process(project, process, Instant::now());
            return Err(error);
        }
        Ok(process)
    }

    /// The existing child-process owner, kept for its process fixtures.
    #[cfg(test)]
    fn spawn_image(
        root: &Path,
        project: ProjectBinding,
        service: Incarnation,
        derived: bool,
        executable: &Path,
        engine: airk_core::identity::Sha256Digest,
    ) -> Result<Self> {
        let worker = new_instance_id()?;
        let diagnostic_directory = airk_host::process_diagnostics::directory();
        let mut command = Command::new(executable);
        command
            .args(["__airk_role", "setup-interim"])
            .arg(if derived {
                "derived-worker"
            } else {
                "project-worker"
            })
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(if diagnostic_directory.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child =
            airk_host::native_process::spawn(&mut command).context("start project owner")?;
        let containment = airk_host::native_process::attach_owned(
            &mut child,
            airk_host::child_job::ChildJob::attach,
        )?;
        let mut input = child.stdin.take().context("project input missing")?;
        let output: Box<dyn FrameSource<Reply>> = Box::new(StreamFrames(
            child.stdout.take().context("project output missing")?,
        ));
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut process = Self {
            exiting: false,
            read_settlement: None,
            catalog_gate: None,
            catalog_pending: None,
            catalog_quarantined: false,
            derived,
            child: WorkerHost::Child(child),
            _containment: Some(containment),
            scope: None,
            input: None,
            output: receiver,
            reader: None,
            stderr: None,
            identity: None,
            role: protocol::role(worker),
            runtime: root.into(),
            last_renewal: Instant::now(),
            pool_binding: None,
            provider_helpers: Vec::new(),
            provider_in_process: false,
            #[cfg(feature = "local-inference")]
            provider_completed: None,
            pool_pending: false,
            last_status: Instant::now(),
            waiting: None,
        };
        if let Some(directory) = diagnostic_directory {
            process.stderr = Some(airk_host::process_diagnostics::WorkerStderr::start(
                process.child.child_mut().context("project child missing")?,
                &process.role,
                &directory,
            )?);
        }
        process.reader = Some(spawn_reply_reader(output, sender)?);
        // Init is the only frame of its type; the boxed carrier owns the rest.
        service_ipc::write_frame(
            &mut input,
            &protocol::Init {
                engine,
                runtime: root.into(),
                project,
                service,
                worker,
            },
        )?;
        process.input = Some(Box::new(StreamFrames(input)));
        process.finish_start(project, service, worker)?;
        Ok(process)
    }

    /// The owner's first reply must name exactly the generation it was started as.
    fn finish_start(
        &mut self,
        project: ProjectBinding,
        service: Incarnation,
        worker: Incarnation,
    ) -> Result<()> {
        let reply = self
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
                        && role == self.role,
                    "project owner identity mismatch"
                );
                self.identity = Some(identity);
            }
            _ => anyhow::bail!("project owner failed initialization"),
        }
        Ok(())
    }

    fn grant(&mut self) -> Result<ControlResponse> {
        ensure!(
            self.read_settlement.is_none(),
            "native read worker is retiring"
        );
        self.require_catalog_ready()?;
        self.input
            .as_mut()
            .context("project input missing")?
            .send(WorkerCommand::Grant {
                caller: local_ipc::caller_identity()?,
            })?;
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
        pool: Option<airk_host::embedding_pool_protocol::Access>,
        helper_pending: bool,
    ) -> Result<()> {
        ensure!(
            self.read_settlement.is_none(),
            "native read worker is retiring"
        );
        self.require_catalog_ready()?;
        let pool_binding = pool.as_ref().map(|access| access.grant.pool);
        self.record_provider_delivery(
            pool_binding,
            pool.as_ref().is_some_and(|access| access.in_process),
        )?;
        let pool_pending = helper_pending && !self.derived && pool_binding.is_none();
        self.input
            .as_mut()
            .context("project input missing")?
            .send(WorkerCommand::Activate {
                identity,
                allowance,
                pool: pool.map(Box::new),
                pool_pending,
            })?;
        ensure!(
            matches!(
                self.output.recv_timeout(Duration::from_secs(10))?,
                Reply::Activated
            ),
            "project activation failed"
        );
        self.pool_binding = pool_binding;
        self.pool_pending = pool_pending;
        Ok(())
    }

    fn configure_pool(
        &mut self,
        pool: Option<airk_host::embedding_pool_protocol::Access>,
        pool_pending: bool,
    ) -> Result<()> {
        self.require_catalog_ready()?;
        let pool_binding = pool.as_ref().map(|access| access.grant.pool);
        self.record_provider_delivery(
            pool_binding,
            pool.as_ref().is_some_and(|access| access.in_process),
        )?;
        self.input.as_mut().context("project input missing")?.send(
            WorkerCommand::ConfigurePool {
                identity: self.identity.context("project identity missing")?,
                pool: pool.map(Box::new),
                pool_pending,
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
        self.pool_pending = pool_pending;
        Ok(())
    }

    fn prepare_pool_binding(
        &mut self,
        pool: Option<&mut Pool>,
        registry: &ServiceRegistry,
        helper_pending: bool,
    ) -> Result<Option<(Option<airk_host::embedding_pool_protocol::Access>, bool)>> {
        ensure!(!self.exiting, "native worker resources are draining");
        if self
            .read_settlement
            .as_ref()
            .is_some_and(ReadSettlement::sealed)
        {
            return Ok(None);
        }
        if let Some(gate) = &self.catalog_gate
            && gate
                .require_worker_start(self.identity.context("project identity missing")?.project)
                .is_err()
        {
            // Capture is immutable: no new helper credential generation may be
            // delivered to a retiring worker after its admission/census closes.
            return Ok(None);
        }
        let pool = pool.filter(|_| !self.derived);
        let desired = pool.as_ref().map(|pool| pool.identity());
        let pending = desired.is_none() && !self.derived && helper_pending;
        if self.pool_binding == desired && self.pool_pending == pending {
            return Ok(None);
        }
        let access = match pool {
            Some(pool) => {
                let identity = self.identity.context("project identity missing")?;
                // Register itself may have reached the native helper even if
                // its reply/access construction later fails. Retain that exact
                // generation before any wire effect, not merely on success.
                self.record_provider_delivery(
                    Some(pool.identity()),
                    airk_inference::service_inference::in_process(pool),
                )?;
                Some(pool.access_worker(identity, registry.get(identity.project)?.registration)?)
            }
            None => None,
        };
        Ok(Some((access, pending)))
    }

    fn renew_pool_binding(
        &mut self,
        pool: &mut Pool,
    ) -> Result<airk_inference::embedding_pool_process::Renewal> {
        let identity = self.identity.context("project identity missing")?;
        ensure!(
            self.pool_binding == Some(pool.identity()),
            "pool renewal binding changed"
        );
        let result = pool.renew(identity.worker)?;
        if result == airk_inference::embedding_pool_process::Renewal::Missing {
            self.pool_binding = None;
        }
        Ok(result)
    }
    fn run_job(&mut self, allowance: airk_core::service_resources::WorkAllowance) -> Result<()> {
        ensure!(
            self.read_settlement.is_none(),
            "native read worker is retiring"
        );
        self.require_catalog_ready()?;
        let identity = self.identity.context("project identity missing")?;
        self.input
            .as_mut()
            .context("project input missing")?
            .send(WorkerCommand::RunJob {
                identity,
                allowance,
            })?;
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
        self.require_catalog_ready()?;
        let identity = self.identity.context("project identity missing")?;
        self.input
            .as_mut()
            .context("project input missing")?
            .send(WorkerCommand::WorkStatus {
                identity,
                control,
                keep_for_maintenance,
                freshness: freshness.map(Box::new),
            })?;
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
        self.require_catalog_ready()?;
        let identity = self.identity.context("project identity missing")?;
        self.input
            .as_mut()
            .context("project input missing")?
            .send(WorkerCommand::Renew { identity })?;
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
    fn configure_queries(&mut self, concurrency: usize) -> Result<()> {
        self.require_catalog_ready()?;
        let identity = self.identity.context("project identity missing")?;
        self.input.as_mut().context("project input missing")?.send(
            WorkerCommand::ConfigureQueries {
                identity,
                concurrency,
            },
        )?;
        ensure!(
            matches!(
                self.output.recv_timeout(Duration::from_secs(3))?,
                Reply::QueriesConfigured
            ),
            "project query capacity update failed"
        );
        Ok(())
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
    SettleRead {
        identity: WorkerIdentity,
        operation_id: Incarnation,
    },
    Checkpoint {
        operation_id: Incarnation,
        seal: bool,
        reply: oneshot::Sender<Result<Vec<protocol::WorkerCheckpointObservation>>>,
    },
    CheckpointInference {
        operation_id: Incarnation,
        workers: Vec<WorkerIdentity>,
        reply:
            oneshot::Sender<Result<Option<airk_core::inference_provider::CompletionObservation>>>,
    },
    Catalog {
        gate: airk_catalog_control::catalog_admission::NativeCatalogAdmission,
        operation: Box<catalog::Operation>,
        reply: SyncSender<Result<catalog::Response>>,
    },
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
    checkpoint_closed: Arc<std::sync::atomic::AtomicBool>,
    resources: Arc<Mutex<ResourceCache>>,
    sender: Option<Arc<SyncSender<Envelope>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[derive(Debug)]
pub struct CheckpointResourcesPending;
impl std::fmt::Display for CheckpointResourcesPending {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("checkpoint_predecessor_resources_pending")
    }
}
impl std::error::Error for CheckpointResourcesPending {}

fn checkpoint_workers(
    workers: &mut BTreeMap<ProjectBinding, ChildWorker>,
    operation_id: Incarnation,
    seal: bool,
) -> Result<Vec<protocol::WorkerCheckpointObservation>> {
    let until = Instant::now() + Duration::from_secs(10);
    let engine = airk_host::runtime_pair::engine_identity()?;
    let mut observed = Vec::new();
    for worker in workers.values_mut() {
        ensure!(worker.read_settlement.is_none(), CheckpointResourcesPending);
        ensure!(
            Instant::now() < until,
            "worker checkpoint observation budget expired"
        );
        let identity = worker
            .identity
            .context("checkpoint worker identity missing")?;
        worker
            .input
            .as_mut()
            .context("checkpoint worker input missing")?
            .send(WorkerCommand::Checkpoint {
                identity,
                operation_id,
                seal,
            })?;
        let Reply::Checkpoint { observation } = worker
            .output
            .recv_timeout(until.saturating_duration_since(Instant::now()))?
        else {
            anyhow::bail!("worker checkpoint acknowledgment unavailable");
        };
        ensure!(
            observation.schema == 1
                && observation.operation_id == operation_id
                && observation.identity == identity
                && observation.engine_sha256 == engine
                && observation.admission_closed,
            "crossed worker checkpoint acknowledgment"
        );
        observed.push(observation);
    }
    Ok(observed)
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
            airk_host::runtime_pair::engine_identity()?,
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
    log: &'a airk_host::service_logs::Log,
    policy: &'a Mutex<Broker>,
    pool: &'a mut Option<Pool>,
    resources: &'a mut airk_kernel::service_resources::Resources,
    recovery: &'a mut PoolRecovery,
    drains: &'a mut WorkerResourceDrains,
}

fn begin_worker_exit(
    workers: &mut BTreeMap<ProjectBinding, ChildWorker>,
    project: ProjectBinding,
    pool: &mut Option<Pool>,
    drains: &mut WorkerResourceDrains,
) -> Result<()> {
    let Some(worker) = workers.get_mut(&project) else {
        return Ok(());
    };
    worker.exiting = true;
    let identity = drains.reserve(worker)?;
    // On any refusal the original ChildWorker, charges and reserved history
    // remain owned; the existing Supervisor turn retries this same exit.
    drains.begin(identity, pool.as_mut())?;
    let mut child = workers
        .remove(&project)
        .context("native exit child changed")?;
    // A running in-process owner is never waited for here. It joins the
    // stopping owners, still charged, and the actor's turn reaps it once its
    // exit is observed. One already observed to exit is reaped at once.
    if child.child.in_process() && !matches!(child.child.try_wait(), Ok(Some(_))) {
        stop_in_process(project, child, Instant::now());
        return Ok(());
    }
    drop(child);
    drains.child_reaped(identity)?;
    Ok(())
}

/// One in-process owner asked to stop. It stays here, with its admission,
/// query and provider charges, until its thread's exit is observed.
struct Stopping {
    project: ProjectBinding,
    worker: ChildWorker,
    requested: Instant,
    drain_requested: bool,
    /// The typed reason, once quarantined. Quarantine never releases anything.
    quarantined: Option<&'static str>,
}

/// What one non-blocking poll of a stopping owner observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopProgress {
    Pending,
    /// First observation that the owner cannot stop; the reason's code.
    Quarantined(&'static str),
    /// The owner's thread returned; it may now be reaped.
    Exited,
}

impl Stopping {
    fn new(project: ProjectBinding, mut worker: ChildWorker, requested: Instant) -> Self {
        // Closing the control carrier is the stop request.
        worker.input.take();
        Self {
            project,
            worker,
            requested,
            drain_requested: false,
            quarantined: None,
        }
    }

    /// One step of the pure stop policy at `now`; it never waits.
    fn poll(&mut self, now: Instant) -> StopProgress {
        if matches!(self.worker.child.try_wait(), Ok(Some(_))) {
            return StopProgress::Exited;
        }
        if self.quarantined.is_some() {
            return StopProgress::Pending;
        }
        // An owner that reported itself stuck is quarantined at once.
        if let Some(code) = self.worker.quarantine_reason() {
            self.quarantined = Some(code);
            return StopProgress::Quarantined(code);
        }
        match crate::worker_host::stop_step(
            true,
            self.requested,
            now,
            OWNER_STOP_GRACE,
            IN_PROCESS_DRAIN,
            self.drain_requested,
        ) {
            // An in-process owner is never killed.
            StopStep::Wait | StopStep::Kill => StopProgress::Pending,
            StopStep::Drain => {
                // The cooperative equivalent of the kill: work stops at its
                // next safe point and stays unsettled for the next owner.
                if let Some(scope) = &self.worker.scope {
                    scope.execution.drain();
                }
                self.drain_requested = true;
                StopProgress::Pending
            }
            StopStep::Quarantine => {
                self.quarantined = Some(STOP_DEADLINE_CODE);
                StopProgress::Quarantined(STOP_DEADLINE_CODE)
            }
        }
    }
}

thread_local! {
    /// The supervisor actor's stopping in-process owners. Only the thread that
    /// started an owner (the actor, or a test) holds and polls its entry.
    static STOPPING: std::cell::RefCell<Vec<Stopping>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn stop_in_process(project: ProjectBinding, worker: ChildWorker, now: Instant) {
    let stopping = Stopping::new(project, worker, now);
    STOPPING.with(|entries| entries.borrow_mut().push(stopping));
}

/// A previous owner of `project` has not been observed to exit.
fn stopping_project(project: ProjectBinding) -> bool {
    STOPPING.with(|entries| {
        entries
            .borrow()
            .iter()
            .any(|entry| entry.project == project)
    })
}

fn stopping_is_empty() -> bool {
    STOPPING.with(|entries| entries.borrow().is_empty())
}

/// Whether every stopping owner is quarantined, and the identities they hold.
fn stopping_quarantined() -> (bool, Vec<WorkerIdentity>) {
    STOPPING.with(|entries| {
        let entries = entries.borrow();
        (
            entries.iter().all(|entry| entry.quarantined.is_some()),
            entries
                .iter()
                .filter(|entry| entry.quarantined.is_some())
                .filter_map(|entry| entry.worker.identity)
                .collect(),
        )
    })
}

/// One observation of the stopping owners for the actor's turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopEvent {
    Quarantined {
        project: ProjectBinding,
        code: &'static str,
    },
    /// The owner's exit was observed and it was reaped. `quarantined` keeps
    /// the record of a quarantine that its later exit ended.
    Reaped {
        project: ProjectBinding,
        quarantined: Option<&'static str>,
    },
}

/// Poll every stopping owner once, without waiting. An exited owner is
/// finalized (endpoint, reply reader, catalog reap receipt) by its Drop and
/// only then marked reaped for its resource drain; nothing is released for an
/// owner whose exit was not observed.
fn poll_stopping_owners(drains: &mut WorkerResourceDrains, now: Instant) -> Vec<StopEvent> {
    let mut events = Vec::new();
    let exited = STOPPING.with(|entries| {
        let mut entries = entries.borrow_mut();
        let mut exited = Vec::new();
        let mut index = 0;
        while index < entries.len() {
            match entries[index].poll(now) {
                StopProgress::Pending => index += 1,
                StopProgress::Quarantined(code) => {
                    events.push(StopEvent::Quarantined {
                        project: entries[index].project,
                        code,
                    });
                    index += 1;
                }
                StopProgress::Exited => exited.push(entries.remove(index)),
            }
        }
        exited
    });
    // Finalize outside the collection's borrow.
    for entry in exited {
        let Stopping {
            project,
            worker,
            quarantined,
            ..
        } = entry;
        let identity = worker.identity;
        drop(worker);
        if let Some(identity) = identity
            && drains.contains(identity)
        {
            let _ = drains.child_reaped(identity);
        }
        events.push(StopEvent::Reaped {
            project,
            quarantined,
        });
    }
    events
}

/// Apply stopping-owner observations: a quarantine blocks its project with
/// its typed reason; a quarantined owner's later exit returns the project to
/// the normal failed-exit restart and backoff rules, keeping a log record.
fn record_stop_events(
    events: Vec<StopEvent>,
    root: &Path,
    backoff: &mut BTreeMap<ProjectBinding, (Instant, u32)>,
    quarantined: &mut BTreeMap<ProjectBinding, &'static str>,
    log: &airk_host::service_logs::Log,
) {
    for event in events {
        match event {
            StopEvent::Quarantined { project, code } => {
                quarantined.insert(project, code);
                log.record_code(
                    airk_core::service_logs::LogKind::WorkerFailed,
                    Some(project),
                    None,
                    Some(code),
                );
                eprintln!(
                    "airk: project owner for {project} cannot reach a safe point ({code}); \
                     it keeps its lease and the project stays blocked; restart the service to recover"
                );
            }
            StopEvent::Reaped {
                project,
                quarantined: Some(_),
            } => {
                quarantined.remove(&project);
                log.record_code(
                    airk_core::service_logs::LogKind::WorkerStopped,
                    Some(project),
                    None,
                    Some(QUARANTINE_RELEASED_CODE),
                );
                worker_failed(root, backoff, project, Instant::now());
            }
            StopEvent::Reaped {
                quarantined: None, ..
            } => {}
        }
    }
}

/// Borrowed identity and gate from the existing Supervisor resource owner.
struct WorkerOwnerContext<'a> {
    root: &'a Path,
    instance: Incarnation,
    gate: Option<&'a airk_catalog_control::catalog_admission::NativeCatalogAdmission>,
    hooks: crate::project_worker::Hooks,
}

fn poll_worker_exits(
    drains: &mut WorkerResourceDrains,
    pool: &mut Option<Pool>,
    recovery: &mut PoolRecovery,
    resources: &mut airk_kernel::service_resources::Resources,
    policy: &Mutex<Broker>,
    owner: WorkerOwnerContext<'_>,
) -> Result<()> {
    let WorkerOwnerContext {
        root,
        instance,
        gate,
        hooks: _,
    } = owner;
    let mut first_error = None;
    for identity in drains.identities() {
        let result = (|| -> Result<()> {
            let ledger_ready = match drains.completed_authorizations(identity)? {
                Some(anchor) => recover_completed_ledger(root, instance, &anchor, gate, recovery)?,
                None => true,
            };
            let progress = drains.poll_completed(
                identity,
                pool.as_mut(),
                |service, helper, in_process| {
                    if !in_process
                        && recovery
                            .reap_witnesses()
                            .iter()
                            .any(|proof| proof.matches(service, helper))
                    {
                        return Ok(true);
                    }
                    #[cfg(feature = "local-inference")]
                    if in_process && recovery.provider_was_completed(service, helper) {
                        return Ok(true);
                    }
                    recover_retired_helper(
                        root, instance, service, helper, in_process, gate, recovery,
                    )
                },
                |_| Ok(ledger_ready),
            )?;
            if progress != DrainProgress::Ready {
                return Ok(());
            }
            if !drains.released(identity)? {
                if identity.service == instance {
                    policy
                        .lock()
                        .map_err(|_| anyhow::anyhow!("service policy unavailable"))?
                        .process_exited(identity)?;
                    resources.process_exited(identity)?;
                }
                drains.mark_released(identity)?;
            }
            let removed = match gate {
                Some(gate) => gate
                    .with_unclaimed_resource_owner(identity, || drains.remove_completed(identity))?
                    .is_some(),
                None => {
                    drains.remove_completed(identity)?;
                    true
                }
            };
            if removed
                && let Some(helper) = pool.as_mut()
                && let Some(ticket) = helper.retirement_ticket(identity)?
            {
                helper.finish_retirement(&ticket)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn recover_retired_helper(
    root: &Path,
    current: Incarnation,
    service: Incarnation,
    helper: Incarnation,
    in_process: bool,
    gate: Option<&airk_catalog_control::catalog_admission::NativeCatalogAdmission>,
    recovery: &mut PoolRecovery,
) -> Result<bool> {
    if let Some(gate) = gate
        && gate.provider_was_completed(service, helper, in_process)?
    {
        return Ok(true);
    }
    if service == current {
        return Ok(false);
    }
    if in_process {
        let witness = recovery.predecessor_exclusion(root, current, service, helper)?;
        if let Some(gate) = gate {
            gate.observe_provider_excluded(witness)?;
        }
        return Ok(true);
    }
    let Some(witness) = airk_inference::embedding_pool_process::recover_shared_helper(
        root,
        airk_store::service::registry::SharedHelperIdentity { service, helper },
    )?
    else {
        return Ok(false);
    };
    // Missing rows, EOF and elapsed time never produce this opaque witness.
    if let Some(gate) = gate {
        gate.observe_provider_reaped(witness)?;
    } else {
        recovery.record_reaped(witness)?;
    }
    Ok(true)
}

fn recover_completed_ledger(
    root: &Path,
    current: Incarnation,
    anchor: &airk_core::inference_provider::CompletionLedgerObservation,
    gate: Option<&airk_catalog_control::catalog_admission::NativeCatalogAdmission>,
    recovery: &mut PoolRecovery,
) -> Result<bool> {
    anchor.validate()?;
    if recovery.ledger_was_completed(anchor)
        || match gate {
            Some(gate) => gate.ledger_was_completed(anchor)?,
            None => false,
        }
    {
        return Ok(true);
    }
    if anchor.service_instance == current {
        return Ok(false);
    }
    let witness = recovery.predecessor_exclusion(
        root,
        current,
        anchor.service_instance,
        anchor.factory_generation,
    )?;
    if let Some(gate) = gate {
        gate.observe_provider_excluded(witness)?;
    }
    Ok(true)
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
    owner: WorkerOwnerContext<'_>,
    registration: airk_core::service::ProjectRegistration,
    derived: bool,
    backoff: &mut BTreeMap<ProjectBinding, (Instant, u32)>,
    admission: WorkerAdmission<'_>,
) -> Result<()> {
    let WorkerOwnerContext {
        root,
        instance,
        gate: catalog_gate,
        hooks,
    } = owner;
    let WorkerAdmission {
        log,
        policy,
        pool,
        resources,
        recovery,
        drains,
    } = admission;
    let project = registration.binding;
    if let Some(gate) = catalog_gate {
        gate.require_worker_start(project)?;
    }
    if let Some(worker) = workers.get(&project) {
        ensure!(!worker.exiting, ConnectionSetupPending::Draining);
        ensure!(
            worker.read_settlement.is_none(),
            ConnectionSetupPending::Draining
        );
        ensure!(worker.derived == derived, "project owner role is draining");
        return Ok(());
    }
    ensure!(
        drains.for_project(project).is_empty(),
        ConnectionSetupPending::Draining
    );
    // A previous in-process owner, including a quarantined one or one that
    // failed to start, keeps this project until its exit is observed.
    ensure!(!stopping_project(project), ConnectionSetupPending::Draining);
    let saved = ServiceRegistry::open_saved(root)?.recovery_budget(
        &project.to_string(),
        airk_host::runtime_pair::engine_identity()?,
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
    ensure!(
        resources.may_start_resident(),
        ConnectionSetupPending::ResidentCapacity
    );
    if backoff
        .get(&project)
        .is_some_and(|(_, attempts)| *attempts >= 3)
    {
        // New requests cannot remain pending behind an already exhausted circuit.
        // This also covers an activated child that exited before accepting work.
        settle_failed_start(root, instance, project);
    }
    if let Some((until, attempts)) = backoff.get(&project) {
        ensure!(*attempts < 3, "project restart circuit exhausted");
        ensure!(
            Instant::now() >= *until,
            ConnectionSetupPending::RestartBackoff
        );
    }
    let spawn = || -> Result<ChildWorker> {
        if !derived && ServiceRegistry::open_saved(root)?.project_setup_pending(project)? {
            airk_host::service_registration::prepare_storage(root, project)?;
        }
        ChildWorker::spawn(root, project, instance, derived, hooks)
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
        concurrent_queries: resources.snapshot().configuration.query_concurrency + 1,
        response_bytes: (resources.snapshot().configuration.query_concurrency + 1)
            * MAX_FRAME_BYTES,
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
    // Native Register can create membership before its ACK is observed. The
    // monotonic history covers uncertainty as well as delivered credentials.
    worker.record_provider_delivery(
        pool.as_ref().filter(|_| !derived).map(Pool::identity),
        pool.as_ref()
            .is_some_and(airk_inference::service_inference::in_process),
    )?;
    let access = match pool.as_mut().filter(|_| !derived) {
        Some(helper) => match helper.access_worker(identity, registration) {
            Ok(access) => Some(access),
            Err(error) => {
                // A failed exchange leaves its reply stream ambiguous. Retire the
                // helper before admitting any sibling control call, but keep this
                // project's lexical worker and account one helper outage.
                let code = airk_inference::embedding_pool_process::failure_code(
                    &error,
                    "helper_grant_registration_failed",
                );
                retire_helper(pool, catalog_gate, recovery);
                recovery.failed(Instant::now());
                let saved = (|| -> Result<_> {
                    ServiceRegistry::open(root, instance)?.recovery_failed(
                        "helper",
                        airk_host::runtime_pair::engine_identity()?,
                        unix_ms()?,
                    )
                })();
                if saved.is_err() {
                    recovery.suppressed = true;
                }
                log.record_code(
                    airk_core::service_logs::LogKind::ModelFailed,
                    None,
                    None,
                    Some(&code),
                );
                None
            }
        },
        None => None,
    };
    let helper_pending = recovery.admission_pending();
    let activated = worker
        .activate(identity, allowance, access, helper_pending)
        .and_then(|_| {
            if let Some(gate) = catalog_gate {
                gate.register_worker(identity)?;
                worker.catalog_gate = Some(gate.clone());
            }
            Ok(())
        });
    if let Err(error) = activated {
        worker.exiting = true;
        workers.insert(project, worker);
        let _ = begin_worker_exit(workers, project, pool, drains);
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
    if let Some(gate) = catalog_gate {
        let registry = ServiceRegistry::open_saved(root).map_err(|error| {
            error.context(airk_catalog_control::catalog_coordinator::CatalogClockPending)
        })?;
        if airk_catalog_control::catalog_coordinator::refresh_catalog_clock(gate, &registry)? {
            return Err(airk_catalog_control::catalog_coordinator::CatalogRevisionChanged.into());
        }
    }
    Ok(())
}

impl Supervisor {
    #[allow(clippy::too_many_arguments)] // The owned inputs and service hooks stay explicit.
    pub fn start(
        root: PathBuf,
        instance: Incarnation,
        policy: Arc<Mutex<Broker>>,
        source_pid: Arc<std::sync::atomic::AtomicU32>,
        source_telemetry: Arc<std::sync::atomic::AtomicBool>,
        log: airk_host::service_logs::Log,
        ownership: Arc<airk_store::service::ServiceLease>,
        hooks: crate::project_worker::Hooks,
    ) -> Result<Self> {
        ensure!(
            ownership.root() == root.as_path(),
            "foreign native supervisor owner"
        );
        let (sender, receiver) = mpsc::sync_channel::<Envelope>(32);
        let checkpoint_closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let checkpoint_pause = checkpoint_closed.clone();
        let recovery_fenced = airk_host::checkpoint_fence::admission_closed(&root)?;
        let loaded = airk_host::service_resource_policy::load(&root);
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
        #[cfg(feature = "local-inference")]
        let mut inference_factory =
            airk_inference::service_inference::InferenceFactory::new(ownership.clone(), instance)?;
        let thread = std::thread::Builder::new()
            .name("airk-project-supervisor".into())
            .spawn(move || {
                let mut workers = BTreeMap::<ProjectBinding, ChildWorker>::new();
                let mut catalog_gate: Option<airk_catalog_control::catalog_admission::NativeCatalogAdmission> = None;
                let mut pool: Option<Pool> = None;
                let mut pool_recovery = PoolRecovery::new_owned(Instant::now(),ownership.clone(),instance);
                let mut registry: Option<ServiceRegistry> = None;
                let mut backoff = BTreeMap::<ProjectBinding, (Instant, u32)>::new();
                // Projects whose in-process owner is quarantined, with its reason.
                let mut quarantined = BTreeMap::<ProjectBinding, &'static str>::new();
                let mut waiting = BTreeMap::<ProjectBinding, WaitSample>::new();
                let mut restarting = BTreeMap::<ProjectBinding, Option<WorkerIdentity>>::new();
                let mut predecessors: Option<Vec<ProjectBinding>> = None;
                let mut predecessor_pool = None;
                let mut shared_mode = None;
                let mut shared_predecessors: Option<airk_inference::embedding_pool_process::SharedHelperPredecessorProof> = None;
                let mut shared_recovery_failed = false;
                let mut shared_gc_complete = false;
                let mut next_shared_gc = Instant::now();
                let mut next_shared_recovery = Instant::now();
                let mut resources = initial;
                let mut resource_drains = WorkerResourceDrains::default();
                let mut configuration_error = configuration_error;
                let mut pending_after = None;
                let mut source_after = None;
                let mut background_after = None;
                let mut next_receipt_maintenance = Instant::now();
                let mut endpoints_ready = false;
                let mut next_recovery_check=Instant::now();
                let mut checkpoint_frozen = false;
                loop {
                    #[cfg(feature = "local-inference")]
                    {
                        if let Ok(events) = inference_factory.take_completed() {
                            for event in events { pool_recovery.record_completed(event); }
                        }
                        let fallback_ready=match &catalog_gate {
                            Some(gate)=>pool_recovery.transfer_fallback(gate).is_ok(),
                            None=>true,
                        };
                        if fallback_ready {
                            let _ = pool_recovery.transfer_completed(catalog_gate.as_ref(), |event| {
                                for worker in workers.values_mut() { worker.complete_provider(event)?; }
                                Ok(())
                            });
                        }
                    }
                    if checkpoint_frozen {
                        match receiver.recv_timeout(Duration::from_millis(250)) {
                            Ok(Envelope::Checkpoint { operation_id, seal, reply }) => { let _ = reply.send(checkpoint_workers(&mut workers, operation_id, seal)); },
                            Ok(Envelope::CheckpointInference {operation_id, workers, reply}) => {
                                #[cfg(feature = "local-inference")]
                                let result = match pool.as_mut() {
                                    Some(pool) => pool.checkpoint_drain(operation_id, workers),
                                    None => inference_factory.empty_checkpoint(operation_id, workers),
                                };
                                #[cfg(not(feature = "local-inference"))]
                                let result = { let _ = (operation_id, workers); Err(anyhow::anyhow!("in-process inference is unavailable in this build")) };
                                let _ = reply.send(result);
                            },
                            Ok(other) => drop(other),
                            Err(mpsc::RecvTimeoutError::Timeout) => {},
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                        continue;
                    }
                    // An owner that reported itself stuck is stopped now; the
                    // poll below quarantines it. Nothing here waits for it.
                    let stuck: Vec<_> = workers.iter().filter(|(_, worker)| worker.quarantine_reason().is_some()).map(|(project, _)| *project).collect();
                    for project in stuck {
                        let _ = begin_worker_exit(&mut workers, project, &mut pool, &mut resource_drains);
                    }
                    let exiting: Vec<_> = workers.iter().filter(|(_, worker)| worker.exiting).map(|(project, _)| *project).collect();
                    for project in exiting {
                        let _ = begin_worker_exit(&mut workers, project, &mut pool, &mut resource_drains);
                    }
                    let stop_events = poll_stopping_owners(&mut resource_drains, Instant::now());
                    record_stop_events(stop_events, &root, &mut backoff, &mut quarantined, &log);
                    let _ = poll_worker_exits(&mut resource_drains,
&mut pool,
&mut pool_recovery,
&mut resources,
&policy,
WorkerOwnerContext { root: &root, instance, gate: catalog_gate.as_ref(), hooks });
                    {
                        let mut referenced = resource_drains.referenced_helpers();
                        referenced.extend(workers.values().flat_map(|worker| worker.provider_helpers.iter().copied()));
                        let mut ledger_heads = resource_drains.referenced_ledgers();
                        ledger_heads.extend(workers.values().filter_map(|worker| worker.completed_authorizations().map(|anchor| anchor.head_sha256)));
                        if let Some(gate) = &catalog_gate { let _ = gate.prune_provider_reaps(&referenced, &ledger_heads); }
                        #[cfg(feature = "local-inference")]
                        pool_recovery.prune_completed(&referenced,&ledger_heads);
                    }
                    if registry.is_none() {
                        registry = ServiceRegistry::open(&root, instance).ok();
                    }
                    if Instant::now() >= next_shared_recovery {
                        shared_recovery_failed = false;
                        if shared_mode.is_none() {
                            match airk_inference::embedding_pool_bootstrap::shared_mode(&root) {
                                Ok(mode) => shared_mode=Some(mode),
                                Err(_) => shared_recovery_failed=true,
                            }
                        }
                        if shared_mode==Some(true) && shared_predecessors.is_none() {
                            match airk_inference::embedding_pool_process::recover_predecessor_shared_helpers(&root,instance) {
                                Ok(Some(proof)) => {
                                    let accepted=(|| -> Result<()> {
                                        ensure!(proof.matches(registry.as_ref().context("native registry unavailable")?.incarnation()?,instance),"foreign shared predecessor proof");
                                        for witness in proof.witnesses() {
                                            match &catalog_gate {Some(gate)=>gate.observe_provider_reaped(*witness)?,None=>pool_recovery.record_reaped(*witness)?,}
                                        }
                                        Ok(())
                                    })();
                                    if accepted.is_ok() {shared_predecessors=Some(proof);} else {shared_recovery_failed=true;}
                                },
                                Ok(None)=>{},
                                Err(_)=>shared_recovery_failed=true,
                            }
                        }
                        next_shared_recovery=Instant::now()+Duration::from_secs(1);
                    }
                    if Instant::now()>=next_recovery_check {
                        if let Some(registry)=registry.as_mut(){let _=check_recovery_conditions(registry,pool.is_some());}
                        next_recovery_check=Instant::now()+Duration::from_secs(1);
                    }
                    if pool.is_none() {
                        let refreshed = (|| -> Result<Option<&'static str>> {
                            pool_recovery.refresh(
                                registry.as_mut().context("registry unavailable")?,
                                airk_host::runtime_pair::engine_identity()?, Instant::now(), unix_ms()?,
                            )
                        })();
                        match refreshed {
                            Ok(Some(code)) => log.record_code(airk_core::service_logs::LogKind::ModelRecovery, None, None, Some(code)),
                            Ok(None) => {},
                            Err(_) => {
                                if !pool_recovery.suppressed {
                                    log.record_code(airk_core::service_logs::LogKind::ModelRecovery, None, None, Some("helper_recovery_metadata_unavailable"));
                                }
                                pool_recovery.suppressed = true;
                            },
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
                        if begin_worker_exit(&mut workers, project, &mut pool, &mut resource_drains).is_ok() {
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
                            && begin_worker_exit(&mut workers, *project, &mut pool, &mut resource_drains).is_err()
                        {
                            continue;
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
                        airk_inference::unix_guardian::recover(registry, &active)
                    });
                    #[cfg(unix)]
                    let (pending_providers, provider_error) = match provider_drain {
                        Some(Ok(pending)) => (pending, false),
                        _ => (0, true),
                    };
                    #[cfg(not(unix))]
                    let (pending_providers, provider_error) = (0, false);
                    let shared_pending=usize::from(shared_mode==Some(true) && shared_predecessors.is_none());
                    let pending_providers=pending_providers+shared_pending;
                    let provider_error=provider_error || shared_mode.is_none() || shared_recovery_failed;
                    resources.observe_provider_drains(pending_providers, provider_error);
                    #[cfg(unix)]
                    let provider_rss = registry
                        .as_ref()
                        .and_then(|registry| airk_inference::unix_guardian::rss(registry,pool.as_ref().and_then(airk_inference::service_inference::process_id)).ok());
                    #[cfg(windows)]
                    let provider_rss = pool.as_ref().map(Pool::descendant_rss).transpose().ok().map(Option::unwrap_or_default);
                    let observations = std::iter::once(std::process::id())
                        // An in-process owner is part of the service's own resident set.
                        .chain(workers.values().filter_map(|worker| worker.child.id()))
                        .chain(pool.as_ref().and_then(airk_inference::service_inference::process_id))
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
                    let mut rss = None::<u64>;
                    let mut rss_overflow=false;
                    for sample in observations.into_iter().flatten() {
                        match rss.unwrap_or(0).checked_add(sample) {
                            Some(total)=>rss=Some(total),
                            None=>{rss=None;rss_overflow=true;break;},
                        }
                    }
                    let complete=complete && !rss_overflow && !provider_error && pending_providers==0;
                    let available = airk_kernel::service_resource_observations::available_memory();
                    resources.block_admission(
                        configuration_error || provider_error || pending_providers > 0,
                    );
                    resources.observe(rss, available, complete && available.is_some());
                    // Registry I/O must never occur while cached observations
                    // are locked by health/console readers.
                    let idle_safe = workers.is_empty() && resource_drains.is_empty() && restarting.is_empty() && stopping_is_empty()
                        && !provider_error && pending_providers == 0
                        && registry.as_ref().is_some_and(|registry| {
                            registry.pending_projects(1).is_ok_and(|page| page.is_empty())
                                && registry.has_recovery_waits().is_ok_and(|pending|!pending)
                                && registry.has_background_work().is_ok_and(|pending| !pending)
                                && registry.pending_project_removals(None,1).is_ok_and(|pending| pending.is_empty())
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
                        // An open restart circuit and a quarantined owner both
                        // block automatic starts; the log record names which.
                        cache.blocked = backoff
                            .iter()
                            .filter_map(|(project, (_, attempts))| {
                                (*attempts >= 3).then_some(*project)
                            })
                            .chain(quarantined.keys().copied())
                            .collect();
                        cache.snapshot = resources.snapshot();
                        cache.observed = Instant::now();
                        cache.configuration_error = configuration_error;
                        cache.idle_safe = idle_safe;
                        cache.waiting = sampled_waits;
                    }
                    #[cfg(not(feature = "local-inference"))]
                    {                    match pool.as_mut().map(Pool::exit_status) {
                        Some(Ok(Some(status))) => {
                            let code = airk_inference::embedding_pool_process::exit_code(status);
                            log.record_code(airk_core::service_logs::LogKind::ModelFailed, None, None, Some(&code));
                            retire_helper(&mut pool,catalog_gate.as_ref(),&mut pool_recovery);
                            pool_recovery.failed(Instant::now());
                            if registry.as_mut().is_none_or(|r|airk_host::runtime_pair::engine_identity().and_then(|engine|r.recovery_failed("helper",engine,unix_ms()?)).is_err()){pool_recovery.suppressed=true;}
                        },
                        Some(Err(error)) if pool.as_ref().is_some_and(|pool|pool.containment_failed(&error)) => {
                            // Only the exact owned guardian's actual native
                            // reap tags this failure. Generic wait errors are
                            // uncertainty and keep the original owner below.
                            retire_helper(&mut pool,catalog_gate.as_ref(),&mut pool_recovery);
                            pool_recovery.failed(Instant::now());
                            if registry.as_mut().is_none_or(|r|airk_host::runtime_pair::engine_identity().and_then(|engine|r.recovery_failed("helper",engine,unix_ms()?)).is_err()){pool_recovery.suppressed=true;}
                            log.record_code(airk_core::service_logs::LogKind::ModelFailed,None,None,Some("helper_containment_failed"));
                        },
                        Some(Err(_)) => {
                            if !pool_recovery.suppressed {
                                log.record_code(airk_core::service_logs::LogKind::ModelFailed, None, None, Some("helper_wait_unavailable"));
                            }
                            pool_recovery.suppressed = true; // Unknown exit never authorizes replacement.
                        },
                        Some(Ok(None)) => {
                            if let Some(code) = pool.as_ref().and_then(Pool::control_failure).map(str::to_owned) {
                                // A timed-out mutation has no correlation-safe reply left.
                                // This also handles errors ignored during worker cleanup or
                                // resource rollback; no later control can reuse that stream.
                                retire_helper(&mut pool,catalog_gate.as_ref(),&mut pool_recovery);
                                pool_recovery.failed(Instant::now());
                                if registry.as_mut().is_none_or(|r|airk_host::runtime_pair::engine_identity().and_then(|engine|r.recovery_failed("helper",engine,unix_ms()?)).is_err()){pool_recovery.suppressed=true;}
                                log.record_code(airk_core::service_logs::LogKind::ModelFailed, None, None, Some(&code));
                            } else {
                            let health = pool.as_ref().unwrap().health();
                            let reset = (|| -> Result<bool> {
                                pool_recovery.healthy(registry.as_mut().context("registry unavailable")?,
                                    airk_host::runtime_pair::engine_identity()?, health, Instant::now())
                            })();
                            if matches!(reset, Ok(true)) {
                                log.record_code(airk_core::service_logs::LogKind::ModelRecovery, None, None, Some("helper_healthy_reset"));
                            }
                            }
                        },
                        None => {},
                    }
                    }
                    #[cfg(feature = "local-inference")]
                    if let Some(current) = pool.as_mut() {
                        if current.retiring() {
                            retire_helper(&mut pool,catalog_gate.as_ref(),&mut pool_recovery);
                            if pool.is_none() {log.record_code(airk_core::service_logs::LogKind::ModelStopped,None,None,Some("inference_native_drain_complete"));}
                        } else { match current.failure() {
                            Ok(Some(code)) => {
                                log.record_code(airk_core::service_logs::LogKind::ModelFailed,None,None,Some(&code));
                                retire_helper(&mut pool,catalog_gate.as_ref(),&mut pool_recovery);
                                pool_recovery.failed(Instant::now());
                            }
                            Err(_) => { pool_recovery.suppressed = true; }
                            Ok(None) => {
                                let health = current.health();
                                let reset = (|| -> Result<bool> {
                                    pool_recovery.healthy(registry.as_mut().context("registry unavailable")?,airk_host::runtime_pair::engine_identity()?,health,Instant::now())
                                })();
                                if matches!(reset, Ok(true)) {log.record_code(airk_core::service_logs::LogKind::ModelRecovery,None,None,Some("inference_healthy_reset"));}
                            }
                        }
                    }
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
                    if may_allocate && !shared_gc_complete && Instant::now()>=next_shared_gc && resource_drains.is_empty()
                        && catalog_gate.as_ref().is_some_and(|gate| gate.pending_fence_key().is_ok_and(|pending|pending.is_none()))
                        && let Some(proof)=&shared_predecessors {
                        // The same native lease and complete predecessor proof
                        // protect GC; Store rechecks durable pending fences in
                        // the deletion transaction. Current-service rows remain.
                        if airk_inference::embedding_pool_process::prune_predecessor_shared_helpers(&root,instance,&ownership,proof).is_ok() {shared_gc_complete=true;}
                        next_shared_gc=Instant::now()+Duration::from_secs(1);
                    }
                    if may_allocate
                        && pool.is_none()
                        && pool_recovery.due(Instant::now())
                        && (workers.values().any(|worker| !worker.derived)
                            || pool_recovery.requested)
                    {
                        pool_recovery.attempted();
                        #[cfg(feature = "local-inference")]
                        let started = inference_factory.start(resources.snapshot().inference_threads_limit);
                        #[cfg(not(feature = "local-inference"))]
                        let started = Pool::start(&root, instance, resources.snapshot().inference_threads_limit);
                        match started {
                            Ok(started) => {
                                pool_recovery.started(started.identity());
                                pool = Some(started);
                                log.record_code(airk_core::service_logs::LogKind::ModelRecovery, None, None, Some("helper_started"));
                            },
                            Err(error) => {
                                let code = airk_inference::embedding_pool_process::failure_code(&error, "helper_start_failed");
                                pool_recovery.failed(Instant::now());
                                if registry.as_mut().is_none_or(|r|airk_host::runtime_pair::engine_identity().and_then(|engine|r.recovery_failed("helper",engine,unix_ms()?)).is_err()){pool_recovery.suppressed=true;}
                                if matches!(code.as_str(), "helper_payload_verification_failed" | "helper_containment_failed" | "helper_start_identity_mismatch" | "helper_start_wait_unavailable" | "helper_permission_denied") {
                                    pool_recovery.suppressed = true;
                                    if let Some(registry) = registry.as_mut() {
                                        let _ = airk_host::runtime_pair::engine_identity().and_then(|engine| registry.suppress_helper_recovery(engine));
                                    }
                                }
                                log.record_code(airk_core::service_logs::LogKind::ModelFailed, None, None, Some(&code));
                            },
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
                        let _ = begin_worker_exit(&mut workers, project, &mut pool, &mut resource_drains);
                    }
                    let mut failed_renewals = Vec::new();
                    let settled_reads = workers.iter_mut().filter_map(|(project, worker)| {
                        match worker.poll_read_settlement() {
                            Ok(true) if resources.running(*project).is_none() => Some(*project),
                            Err(_) => {
                                log.record_code(airk_core::service_logs::LogKind::MetadataUnavailable,
                                    Some(*project), None, Some("worker_read_settlement_unavailable"));
                                None
                            }
                            _ => None,
                        }
                    }).collect::<Vec<_>>();
                    for project in settled_reads {
                        // The exact native census sealed this worker only after
                        // its accepted work and read charges reached zero.
                        let _ = begin_worker_exit(&mut workers, project, &mut pool, &mut resource_drains);
                    }
                    let mut clean_renewals = std::collections::BTreeSet::new();
                    let mut pool_control_failure = None;
                    for (project, worker) in &mut workers {
                        match worker.catalog_poll_ready() {
                            Ok(true) => {},
                            Ok(false) => continue,
                            Err(_) => { failed_renewals.push(*project); continue; },
                        }
                        if pool_control_failure.is_some() {
                            continue;
                        }
                        let update = (|| -> Result<_> {
                            worker.prepare_pool_binding(pool.as_mut(),
                                registry.as_ref().context("registry unavailable")?,
                                pool_recovery.admission_pending())
                        })();
                        match update {
                            Ok(Some((access, pending))) => {
                                let registered = access.is_some();
                                if let Err(error) = worker.configure_pool(access, pending) {
                                    if clean_disconnected_exit(&mut worker.child, &error) {
                                        clean_renewals.insert(*project);
                                    }
                                    failed_renewals.push(*project);
                                    continue;
                                }
                                if registered {
                                    log.record_code(airk_core::service_logs::LogKind::ModelRecovery, Some(*project), None, Some("helper_grant_registered"));
                                }
                            },
                            Ok(None) => {},
                            Err(error) => {
                                pool_control_failure = Some(airk_inference::embedding_pool_process::failure_code(&error, "helper_grant_registration_failed"));
                                continue;
                            },
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
                            match worker.renew_pool_binding(pool) {
                                Ok(airk_inference::embedding_pool_process::Renewal::Renewed) => {},
                                Ok(airk_inference::embedding_pool_process::Renewal::Missing) => {
                                    // The exact correlated reply proves only this membership is
                                    // absent. The next ordinary turn installs a new capability;
                                    // other workers and the healthy shared model remain owned.
                                    log.record_code(airk_core::service_logs::LogKind::ModelRecovery, Some(*project), None, Some("helper_grant_missing"));
                                },
                                Err(error) => {
                                    pool_control_failure = Some(airk_inference::embedding_pool_process::failure_code(&error, "helper_grant_renewal_failed"));
                                },
                            }
                        }
                    }
                    if let Some(code) = pool_control_failure {
                        retire_helper(&mut pool,catalog_gate.as_ref(),&mut pool_recovery);
                        pool_recovery.failed(Instant::now());
                        if registry.as_mut().is_none_or(|r|airk_host::runtime_pair::engine_identity().and_then(|engine|r.recovery_failed("helper",engine,unix_ms()?)).is_err()){pool_recovery.suppressed=true;}
                        log.record_code(airk_core::service_logs::LogKind::ModelFailed, None, None, Some(&code));
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
                        let _ = begin_worker_exit(&mut workers, project, &mut pool, &mut resource_drains);
                    }
                    match receiver.recv_timeout(Duration::from_millis(250)) {
                        Ok(Envelope::SettleRead { identity, operation_id }) => {
                            if let Some(worker) = workers.get_mut(&identity.project)
                                && worker.identity == Some(identity)
                                && worker.read_settlement.is_none()
                            {
                                worker.read_settlement = Some(ReadSettlement::new(operation_id));
                                resources.forget_pending(identity.project);
                            }
                        },
                        Ok(Envelope::Checkpoint { operation_id, seal, reply }) => {
                            let result = (|| -> Result<Vec<protocol::WorkerCheckpointObservation>> {
                                ensure!(checkpoint_pause.load(std::sync::atomic::Ordering::Acquire), "worker checkpoint admission is not closed");
                                ensure!(resource_drains.is_empty() && restarting.is_empty() && stopping_is_empty(), CheckpointResourcesPending);
                                checkpoint_workers(&mut workers, operation_id, seal)
                            })();
                            if seal && result.as_ref().is_ok_and(|workers| workers.iter().all(|worker| worker.sealed && worker.durable_boundary)) { checkpoint_frozen = true; }
                            let _ = reply.send(result);
                        },
                        Ok(Envelope::CheckpointInference {operation_id, workers, reply}) => {
                            let _ = (operation_id, workers);
                            let _ = reply.send(Err(anyhow::anyhow!("worker checkpoint is not sealed")));
                        },
                        Ok(Envelope::Catalog { gate, operation, reply }) => {
                            let result = catalog::execute(&mut catalog_gate, gate, *operation, &mut workers,
                                &mut registry, &root, instance, may_allocate, &mut backoff,
                                WorkerAdmission { log: &log, policy: &policy, pool: &mut pool, resources: &mut resources, recovery: &mut pool_recovery, drains: &mut resource_drains }, hooks);
                            catalog::send_reply(&reply, result);
                        },
                        Ok(Envelope::ConfigureResources {
                            configuration,
                            reply,
                        }) => {
                            let previous = resources.snapshot().configuration;
                            let result = (|| -> Result<()> {
                                configuration.validate()?;
                                for worker in workers.values_mut() {
                                    let identity = worker.identity.context("project identity missing")?;
                                    worker.configure_queries(configuration.query_concurrency)?;
                                    policy.lock().map_err(|_| anyhow::anyhow!("service policy unavailable"))?
                                        .configure_queries(identity, configuration.query_concurrency, Instant::now())?;
                                }
                                resources.reconfigure(configuration.clone())?;
                                if let Some(pool) = pool.as_mut() {
                                    pool.configure_threads(resources.snapshot().inference_threads_limit)?;
                                }
                                airk_host::service_resource_policy::save(&root, &configuration)
                            })();
                            if result.is_err() {
                                // Never publish a setting before native owners acknowledge it.
                                // Restore accepted policy; failed acknowledgements remain explicit.
                                for worker in workers.values_mut() {
                                    let _ = worker.configure_queries(previous.query_concurrency);
                                    if let (Some(identity), Ok(mut policy)) = (worker.identity, policy.lock()) {
                                        let _ = policy.configure_queries(identity, previous.query_concurrency, Instant::now());
                                    }
                                }
                                let _ = resources.reconfigure(previous);
                                if let Some(pool) = pool.as_mut() {
                                    let _ = pool.configure_threads(resources.snapshot().inference_threads_limit);
                                }
                            }
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
                            {
                                let mut cache = resource_cache.lock().unwrap_or_else(|error| error.into_inner());
                                cache.snapshot = resources.snapshot();
                                cache.observed = Instant::now();
                                cache.configuration_error = configuration_error;
                            }
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
                                start_worker(&mut workers,
WorkerOwnerContext { root: &root, instance, gate: catalog_gate.as_ref(), hooks },
registration,
false,
&mut backoff,
WorkerAdmission {
                                        log: &log,
                                        policy: &policy,
                                        pool: &mut pool,
                                        resources: &mut resources,
                                    recovery: &mut pool_recovery,
                                    drains: &mut resource_drains,
                                    })?;
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
                                let id = airk_inference::service_inference::process_id(pool);
                                pool.status().and_then(|status|airk_inference::service_inference::provider_status(pool).map(|provider|(id,status,provider)))
                            }) {
                                Some(Ok((
                                    id,
                                    airk_host::embedding_pool_protocol::Observation::Status {
                                        loads,
                                        reuses,
                                        sessions,
                                        age_ms,
                                    },
                                    provider,
                                ))) => ControlResponse::EmbeddingStatus {
                                    available: provider.as_ref().is_none_or(|provider|provider.admission_open&&provider.failure.is_none()),
                                    process_id: id,
                                    worker: pool.as_ref().map(Pool::identity),
                                    loads,
                                    reuses,
                                    sessions,
                                    observation_age_ms: Some(age_ms),
                                    provider,
                                },
                                None => ControlResponse::EmbeddingStatus {
                                    available: false,
                                    process_id: None,
                                    worker: None,
                                    loads: 0,
                                    reuses: 0,
                                    sessions: 0,
                                    observation_age_ms: None,
                                    provider:None,
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
                            let response = if quarantined.contains_key(&project) {
                                // A project restart cannot end a stuck owner's
                                // native call; only a service restart can.
                                ControlResponse::Failure {
                                    code: "project_owner_quarantined".into(),
                                }
                            } else if current != expected {
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
                                pool_recovery = PoolRecovery::new_owned(Instant::now(),ownership.clone(),instance);
                                pool_recovery.requested = true;
                                log.record_code(airk_core::service_logs::LogKind::ModelRecovery, None, None, Some("helper_explicit_start"));
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
                                if registry.as_mut().is_none_or(|r|airk_host::runtime_pair::engine_identity().and_then(|engine|r.suppress_helper_recovery(engine)).is_err()) {
                                    let _=reply.send(ControlResponse::Failure{code:"embedding_stop_not_saved".into()});continue;
                                }
                                pool_recovery.suppressed = true;
                                let failed = force
                                    && pool.as_mut().is_some_and(|pool| pool.terminate().is_err());
                                retire_helper(&mut pool,catalog_gate.as_ref(),&mut pool_recovery);
                                if pool.is_none() {log.record_code(
                                    airk_core::service_logs::LogKind::ModelStopped,
                                    None,
                                    None,
                                    Some("helper_explicit_stop"),
                                );}
                                if failed || pool.is_some() {
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
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64);
                    if let Some(registry) = registry.as_mut() {
                        let recovering = recovery_fenced && airk_host::checkpoint_fence::admission_closed(&root).unwrap_or(true);
                        let checkpointing = checkpoint_pause.load(std::sync::atomic::Ordering::Acquire) || recovering;
                        if !checkpointing && Instant::now() >= next_receipt_maintenance {
                            let _ = registry.maintain_job_receipts();
                            next_receipt_maintenance = Instant::now() + Duration::from_secs(2);
                        }
                        // Source observation is already fenced. These calls
                        // materialize only committed desired-state continuations
                        // (including follow-up work from an accepted job), not
                        // fresh watch events or new periodic maintenance/audits.
                        if !checkpointing && let Ok(due) = registry.due_source_projects(source_after, now_ms, 32) {
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
                        // Source admission gets the existing writer opportunity first.
                        // Store also fences Vector admission transactionally, including
                        // admissible source projects beyond this bounded source page.
                        if !checkpointing && let Ok(due) = registry.due_background_jobs(background_after, now_ms, 8) {
                            background_after = if due.len() == 8 {
                                due.last().map(|item| item.0)
                            } else {
                                None
                            };
                            for (project, revision, kind) in due {
                                if let Ok(nonce) = new_instance_id() {
                                    let _ = registry.enqueue_background_job(
                                        project, revision, kind, instance, nonce, now_ms,
                                    );
                                }
                            }
                        }
                        if !checkpointing && let Ok(due) = registry.due_automatic_audits(now_ms, 8) {
                            for (project,revision) in due {
                                if let Ok(nonce)=new_instance_id() {
                                    let _=registry.enqueue_automatic_audit(project,revision,instance,nonce,now_ms);
                                }
                            }
                        }
                        if !checkpointing && let Ok(due) = registry.due_maintenance(now_ms) {
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
                            let _ = start_worker(&mut workers,
WorkerOwnerContext { root: &root, instance, gate: catalog_gate.as_ref(), hooks },
record.registration,
false,
&mut backoff,
WorkerAdmission {
                                    log: &log,
                                    policy: &policy,
                                    pool: &mut pool,
                                    resources: &mut resources,
                                recovery: &mut pool_recovery,
                                drains: &mut resource_drains,
                                });
                        }
                        // A quarantined previous owner ends the restart too:
                        // only a service restart can recover that project.
                        if workers.contains_key(&project)
                            || quarantined.contains_key(&project)
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
                            if !may_allocate || restarting.contains_key(&project)
                                || workers.get(&project).is_some_and(|worker| worker.read_settlement.is_some()) {
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
                            let _ = start_worker(&mut workers,
WorkerOwnerContext { root: &root, instance, gate: catalog_gate.as_ref(), hooks },
registration,
derived,
&mut backoff,
WorkerAdmission {
                                    log: &log,
                                    policy: &policy,
                                    pool: &mut pool,
                                    resources: &mut resources,
                                recovery: &mut pool_recovery,
                                drains: &mut resource_drains,
                                });
                        }
                    }
                    // Read only bounded metadata candidates. Direct query payloads never enter this loop.
                    if let Some(registry) = registry.as_ref() {
                        for project in workers.keys().copied() {
                            if catalog_gate.as_ref().is_some_and(|gate| gate.require_worker_start(project).is_err()) {
                                resources.forget_pending(project);
                                continue;
                            }
                            if workers.get(&project).is_some_and(|worker| worker.catalog_pending.is_some() || worker.catalog_quarantined) {
                                resources.forget_pending(project);
                                continue;
                            }
                            if restarting.contains_key(&project)
                                || workers.get(&project).is_some_and(|worker| worker.read_settlement.is_some()) {
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
                        match worker.catalog_poll_ready() {
                            Ok(true) => {},
                            Ok(false) => continue,
                            Err(_) => { failed.push(*project); continue; },
                        }
                        if worker.last_status.elapsed() < Duration::from_secs(1) {
                            continue;
                        }
                        let control = resources.running(*project).and_then(|grant| {
                            registry.as_ref().and_then(|registry| {
                                let (plan_paused, plan_canceled) =
                                    registry.maintenance_job_control(grant.job).ok()?;
                                let yield_to_source = registry
                                    .source_yield_requested(*project, grant.job, now_ms)
                                    .unwrap_or_else(|_| {
                                        log.record_code(
                                            airk_core::service_logs::LogKind::MetadataUnavailable,
                                            Some(*project), None,
                                            Some("source_yield_observation_failed"),
                                        );
                                        false
                                    });
                                Some(protocol::WorkControl {
                                    grant: grant.id,
                                    paused: plan_paused
                                        || (registry.retained_project(*project).ok()?.paused
                                            && !registry.job_runs_while_paused(grant.job).ok()?),
                                    canceled: plan_canceled
                                        || registry.cancellation_requested(grant.job).ok()?,
                                    yield_to_source,
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
                                if let Some(code) = observation.integrity_observation_failure.as_deref() {
                                    log.record_code(
                                        airk_core::service_logs::LogKind::MetadataUnavailable,
                                        Some(*project), None, Some(code),
                                    );
                                }
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
                                    let configuration_finished = resources.running(*project).is_some_and(|grant| registry.as_ref().is_some_and(|registry| registry.configuration_work(grant.job).ok().flatten().is_some()));
                                    let _ = resources.finished(identity, finished);
                                    if configuration_finished && let (Some(gate),Some(registry)) = (catalog_gate.as_ref(),registry.as_ref()) {
                                        let _ = airk_catalog_control::catalog_coordinator::refresh_catalog_clock(gate,registry);
                                    }
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
                        let _ = begin_worker_exit(&mut workers, project, &mut pool, &mut resource_drains);
                    }
                }
                // Children stop before the service lock is released. Interrupted jobs retain receipts.
                // Shutdown retains both owning children and accepted grants
                // until the same native child+provider barriers complete.
                // A quarantined owner never reaches its exit here; once every
                // remaining owner is quarantined, the service's own exit is what
                // releases them, as an explicit service restart requires.
                loop {
                    for project in workers.keys().copied().collect::<Vec<_>>() {
                        let _ = begin_worker_exit(&mut workers, project, &mut pool, &mut resource_drains);
                    }
                    let stop_events = poll_stopping_owners(&mut resource_drains, Instant::now());
                    record_stop_events(stop_events, &root, &mut backoff, &mut quarantined, &log);
                    let _ = poll_worker_exits(&mut resource_drains,
&mut pool,
&mut pool_recovery,
&mut resources,
&policy,
WorkerOwnerContext { root: &root, instance, gate: catalog_gate.as_ref(), hooks });
                    let (all_quarantined, held) = stopping_quarantined();
                    if workers.is_empty() && all_quarantined && resource_drains.identities().iter().all(|identity| held.contains(identity) || resource_drains.released(*identity).unwrap_or(false)) { break; }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if let Ok(mut policy) = policy.lock() { policy.ownership_lost(); }
            })?;
        Ok(Self {
            checkpoint_closed,
            resources,
            sender: Some(Arc::new(sender)),
            thread: Some(thread),
        })
    }

    pub fn begin_checkpoint(&self) {
        self.checkpoint_closed
            .store(true, std::sync::atomic::Ordering::Release);
    }

    pub fn read_settlement_requester(&self) -> airk_mcp::read_attempt::WorkerSettlementRequester {
        // A read must not keep the actor channel alive while Supervisor::drop
        // joins it. Only the actual supervisor owns the strong sender lifetime.
        worker_read_settlement::requester(self.sender.as_ref().map(Arc::downgrade))
    }
    pub async fn observe_checkpoint(
        &self,
        operation_id: Incarnation,
        seal: bool,
    ) -> Result<Vec<protocol::WorkerCheckpointObservation>> {
        self.begin_checkpoint();
        let (reply, receive) = oneshot::channel();
        self.sender
            .as_ref()
            .context("checkpoint supervisor closed")?
            .try_send(Envelope::Checkpoint {
                operation_id,
                seal,
                reply,
            })?;
        tokio::time::timeout(Duration::from_secs(15), receive)
            .await
            .map_err(|_| anyhow::anyhow!("worker checkpoint acknowledgment pending"))?
            .map_err(|_| anyhow::anyhow!("worker checkpoint owner closed"))?
    }
    pub async fn observe_inference_checkpoint(
        &self,
        operation_id: Incarnation,
        workers: Vec<WorkerIdentity>,
    ) -> Result<Option<airk_core::inference_provider::CompletionObservation>> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .as_ref()
            .context("checkpoint supervisor closed")?
            .try_send(Envelope::CheckpointInference {
                operation_id,
                workers,
                reply,
            })?;
        tokio::time::timeout(Duration::from_secs(5), receive)
            .await
            .map_err(|_| anyhow::anyhow!("inference checkpoint acknowledgment pending"))?
            .map_err(|_| anyhow::anyhow!("inference checkpoint owner closed"))?
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
    pub fn query_capacity_reader(&self) -> airk_host::service_resource_policy::QueryCapacityReader {
        let resources = self.resources.clone();
        Arc::new(move || {
            resources
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .snapshot
                .configuration
                .query_concurrency
        })
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

    fn owner_identity(byte: u8) -> WorkerIdentity {
        WorkerIdentity {
            service: Incarnation::from_bytes([byte; 16]),
            worker: Incarnation::from_bytes([byte.wrapping_add(1); 16]),
            project: ProjectBinding::from_bytes([byte; 32]),
            fencing_generation: 1,
        }
    }

    /// An in-process owner whose thread holds its project lease inside a
    /// native call with no safe point, until `release` sends or is dropped.
    fn held_thread_owner(
        root: &Path,
        identity: WorkerIdentity,
        scope: crate::worker_scope::WorkerScope,
    ) -> Result<(ChildWorker, mpsc::Sender<()>)> {
        let (acquired, held) = mpsc::channel();
        let (release, gate) = mpsc::channel::<()>();
        let lease_root = root.to_owned();
        let handle = std::thread::spawn(move || -> Result<()> {
            let lease = ProjectLease::acquire(&lease_root, identity.project)?;
            let _ = acquired.send(());
            let _ = gate.recv();
            drop(lease);
            Ok(())
        });
        held.recv_timeout(Duration::from_secs(10))
            .context("fixture owner did not take its lease")?;
        let (_replies, output) = mpsc::sync_channel(1);
        let worker = ChildWorker {
            exiting: false,
            read_settlement: None,
            catalog_gate: None,
            catalog_pending: None,
            catalog_quarantined: false,
            derived: false,
            _containment: None,
            child: WorkerHost::Thread(OwnerThread::new(handle)),
            scope: Some(scope),
            input: None,
            output,
            reader: None,
            stderr: None,
            identity: Some(identity),
            role: protocol::role(identity.worker),
            runtime: root.to_owned(),
            last_renewal: Instant::now(),
            pool_binding: None,
            provider_helpers: Vec::new(),
            provider_in_process: false,
            #[cfg(feature = "local-inference")]
            provider_completed: None,
            pool_pending: false,
            last_status: Instant::now(),
            waiting: None,
        };
        Ok((worker, release))
    }

    /// Poll the stopping owners at the real clock until one event is observed.
    fn next_stop_events(drains: &mut WorkerResourceDrains) -> Result<Vec<StopEvent>> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let events = poll_stopping_owners(drains, Instant::now());
            if !events.is_empty() {
                return Ok(events);
            }
            ensure!(
                Instant::now() < deadline,
                "stopping owner exit was not observed"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_stuck_in_process_owner_is_quarantined_with_its_lease_and_reaped_after_its_own_exit()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let identity = owner_identity(31);
        let scope = crate::worker_scope::WorkerScope::new();
        let (worker, release) = held_thread_owner(&root, identity, scope.clone())?;
        let mut drains = WorkerResourceDrains::default();
        drains.reserve_history(identity, vec![])?;
        let start = Instant::now();
        let at = |seconds| start + Duration::from_secs(seconds);
        stop_in_process(identity.project, worker, start);
        assert!(stopping_project(identity.project));
        // Manual clock: the stop grace, then a drain, then one quarantine.
        assert!(poll_stopping_owners(&mut drains, at(9)).is_empty());
        assert!(!scope.execution.draining());
        assert!(poll_stopping_owners(&mut drains, at(10)).is_empty());
        assert!(scope.execution.draining());
        assert!(poll_stopping_owners(&mut drains, at(29)).is_empty());
        assert_eq!(
            poll_stopping_owners(&mut drains, at(30)),
            vec![StopEvent::Quarantined {
                project: identity.project,
                code: STOP_DEADLINE_CODE,
            }]
        );
        assert!(poll_stopping_owners(&mut drains, at(3600)).is_empty());
        // Quarantine releases nothing: the owner's own thread keeps its lease,
        // its resource drain stays charged and its project stays blocked.
        assert_eq!(stopping_quarantined(), (true, vec![identity]));
        assert!(ProjectLease::acquire(&root, identity.project).is_err());
        assert_eq!(
            drains.poll(identity, None, |_, _, _| Ok(false))?,
            DrainProgress::ChildPending
        );
        assert!(stopping_project(identity.project));
        // The stuck call later returns on its own: the owner is reaped
        // normally, and the event keeps the record of its quarantine.
        release.send(())?;
        assert_eq!(
            next_stop_events(&mut drains)?,
            vec![StopEvent::Reaped {
                project: identity.project,
                quarantined: Some(STOP_DEADLINE_CODE),
            }]
        );
        assert!(stopping_is_empty());
        assert_eq!(
            drains.poll(identity, None, |_, _, _| Ok(false))?,
            DrainProgress::Ready
        );
        drop(ProjectLease::acquire(&root, identity.project)?);
        Ok(())
    }

    #[test]
    fn an_owner_that_reports_itself_stuck_is_quarantined_at_once_and_its_project_blocked()
    -> Result<()> {
        use airk_kernel::service_execution::StuckReason;
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let identity = owner_identity(41);
        let scope = crate::worker_scope::WorkerScope::new();
        let (worker, release) = held_thread_owner(&root, identity, scope.clone())?;
        // The owner's own query deadline marks its scope; it never exits.
        scope.execution.quarantine(StuckReason::QueryDeadline);
        assert_eq!(
            worker.quarantine_reason(),
            Some("owner_quarantined_query_deadline")
        );
        let mut drains = WorkerResourceDrains::default();
        let start = Instant::now();
        stop_in_process(identity.project, worker, start);
        let quarantine = poll_stopping_owners(&mut drains, start);
        assert_eq!(
            quarantine,
            vec![StopEvent::Quarantined {
                project: identity.project,
                code: "owner_quarantined_query_deadline",
            }]
        );
        let (log, guard) = airk_host::service_logs::Log::start(root.clone(), identity.service);
        let mut backoff = BTreeMap::new();
        let mut quarantined = BTreeMap::new();
        record_stop_events(quarantine, &root, &mut backoff, &mut quarantined, &log);
        assert_eq!(
            quarantined.get(&identity.project),
            Some(&"owner_quarantined_query_deadline")
        );
        // Quarantine alone is not counted as a restart attempt.
        assert!(backoff.is_empty());
        release.send(())?;
        let reaped = next_stop_events(&mut drains)?;
        record_stop_events(reaped, &root, &mut backoff, &mut quarantined, &log);
        // Its later exit returns the project to the failed-exit backoff rules.
        assert!(quarantined.is_empty());
        assert!(backoff.contains_key(&identity.project));
        drop(log);
        drop(guard);
        let codes = airk_host::service_logs::recent(&root, 8)?
            .records
            .into_iter()
            .filter(|record| record.project == Some(identity.project))
            .map(|record| (record.kind, record.code))
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            vec![
                (
                    airk_core::service_logs::LogKind::WorkerFailed,
                    Some("owner_quarantined_query_deadline".to_owned())
                ),
                (
                    airk_core::service_logs::LogKind::WorkerStopped,
                    Some(QUARANTINE_RELEASED_CODE.to_owned())
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn production_owner_start_runs_in_the_service_holds_its_lease_and_is_reaped() -> Result<()> {
        // The production entry point, `ChildWorker::spawn`, with the real
        // `runtime_pair::engine_identity()`. A test image has no runtime
        // receipt beside it, which is the single-executable identity.
        let owner = tempfile::Builder::new()
            .prefix("airk-production-start-")
            .tempdir_in(std::env::temp_dir().canonicalize()?)?;
        let root = owner.path().join("control");
        local_ipc::prepare_root(&root)?;
        let service = new_instance_id()?;
        let mut registry = ServiceRegistry::open(&root, service)?;
        let source = owner.path().join("source");
        let data = owner.path().join("data");
        for path in [&source, &data] {
            std::fs::create_dir(path)?;
        }
        std::fs::write(source.join("memo.md"), "# Production owner\n")?;
        airk_store::service::data_scope::bind(&data, &root, registry.incarnation()?)?;
        let registration =
            airk_kernel::service_projects::registration(&source, &data, "production")?;
        registry.register(&registration)?;
        let project = registration.binding;
        let worker = ChildWorker::spawn(
            &root,
            project,
            service,
            false,
            crate::runtime_metadata::project_worker_hooks(),
        )?;
        let identity = worker.identity.context("production owner identity")?;
        assert_eq!((identity.service, identity.project), (service, project));
        // No process of its own: the owner is a thread of this process.
        assert!(worker.child.in_process());
        assert_eq!(worker.child.id(), None);
        assert!(ProjectLease::acquire(&root, project).is_err());
        stop_in_process(project, worker, Instant::now());
        let mut drains = WorkerResourceDrains::default();
        assert_eq!(
            next_stop_events(&mut drains)?,
            vec![StopEvent::Reaped {
                project,
                quarantined: None,
            }]
        );
        assert!(stopping_is_empty());
        drop(ProjectLease::acquire(&root, project)?);
        Ok(())
    }

    #[test]
    fn an_in_process_project_owner_starts_holds_its_lease_and_is_reaped_without_blocking()
    -> Result<()> {
        let owner = tempfile::Builder::new()
            .prefix("airk-in-process-start-")
            .tempdir_in(std::env::temp_dir().canonicalize()?)?;
        let root = owner.path().join("control");
        local_ipc::prepare_root(&root)?;
        let service = new_instance_id()?;
        let mut registry = ServiceRegistry::open(&root, service)?;
        let source = owner.path().join("source");
        let data = owner.path().join("data");
        for path in [&source, &data] {
            std::fs::create_dir(path)?;
        }
        std::fs::write(source.join("memo.md"), "# In-process owner\n")?;
        airk_store::service::data_scope::bind(&data, &root, registry.incarnation()?)?;
        let registration = airk_kernel::service_projects::registration(&source, &data, "owner")?;
        registry.register(&registration)?;
        let project = registration.binding;
        // The production thread host, carriers, scope binding and handshake.
        let engine = airk_core::identity::Sha256Digest::from_bytes([53; 32]);
        let worker = ChildWorker::start_in_process(
            &root,
            project,
            service,
            false,
            engine,
            crate::runtime_metadata::project_worker_hooks(),
        )?;
        let identity = worker.identity.context("in-process owner identity")?;
        assert_eq!((identity.service, identity.project), (service, project));
        assert!(identity.fencing_generation > 0);
        assert!(worker.child.in_process());
        assert_eq!(worker.child.id(), None);
        assert!(ProjectLease::acquire(&root, project).is_err());
        // Stopping returns at once; the actor's poll reaps the owner after
        // its own exit, and only then is its lease free.
        let requested = Instant::now();
        stop_in_process(project, worker, requested);
        assert!(requested.elapsed() < OWNER_STOP_GRACE);
        assert!(stopping_project(project));
        let mut drains = WorkerResourceDrains::default();
        assert_eq!(
            next_stop_events(&mut drains)?,
            vec![StopEvent::Reaped {
                project,
                quarantined: None,
            }]
        );
        assert!(stopping_is_empty());
        drop(ProjectLease::acquire(&root, project)?);
        Ok(())
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
            assert!(recovery.admission_pending());
            assert!(recovery.due(now + Duration::from_secs(30 * attempt)));
            recovery.attempted();
            let failed = now + Duration::from_secs(30 * attempt);
            recovery.failed(failed);
            assert!(!recovery.due(failed));
        }
        assert!(
            !recovery.admission_pending(),
            "exhaustion must publish unavailable even with no helper identity"
        );
        assert!(!recovery.due(now + Duration::from_secs(3600)));
        let mut explicit = PoolRecovery::new(now);
        explicit.suppressed = true;
        assert!(
            !explicit.admission_pending(),
            "explicit suppression must wake authority waiters"
        );
        assert!(!explicit.due(now + Duration::from_secs(3600)));
        explicit = PoolRecovery::new(now);
        assert!(
            explicit.admission_pending(),
            "explicit retry restores only a finite admission window"
        );
        assert!(explicit.due(now));
    }
}
