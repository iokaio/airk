// SPDX-License-Identifier: Apache-2.0
//! Service-owned verified inference child. Private control contains no embedding inputs.
#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
use crate::{runtime_pair, service_ipc};
use airk_core::identity::Incarnation;
#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    service::{ProjectRegistration, WorkerIdentity},
};
#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
use airk_host::embedding_pool_protocol as protocol;
#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
use airk_kernel::service_broker::new_instance_id;
use anyhow::{Context, Result, ensure};
#[cfg(any(
    windows,
    not(feature = "local-inference"),
    any(test, feature = "test-support")
))]
use std::time::Duration;
use std::{path::Path, sync::mpsc, time::Instant};
#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
use std::{
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
    sync::{Arc, Mutex, mpsc::Receiver},
};

#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
mod owned;
#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
mod retirement;
pub mod shared;
#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
pub use protocol::ProviderDrainTicket;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderDrainState {
    Pending,
    Drained,
    Unknown,
}
pub use shared::{
    SharedHelperPredecessorProof, prune_predecessor_shared_helpers,
    recover_predecessor_shared_helpers, recover_shared_helper,
};

/// Native ownership witness, constructed only after the exact group/job,
/// leader, and reply-reader have all completed. EOF or assuming Drop completed
/// is insufficient; callers must obtain the actual native witness.
#[derive(Clone, Copy, Debug)]
pub struct ProviderReapWitness {
    service: Incarnation,
    helper: Incarnation,
}
impl ProviderReapWitness {
    pub fn service(&self) -> Incarnation {
        self.service
    }
    pub fn matches(&self, service: Incarnation, helper: Incarnation) -> bool {
        self.service == service && self.helper == helper
    }
    pub fn helper(&self) -> Incarnation {
        self.helper
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PoolHealth {
    pub instance: Incarnation,
    pub ready_at: Instant,
    pub last_ack_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renewal {
    Renewed,
    Missing,
}

#[derive(Debug)]
struct FailureCode(String);
impl std::fmt::Display for FailureCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for FailureCode {}

#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
fn spawn_failure(error: std::io::Error) -> anyhow::Error {
    let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
        "helper_permission_denied"
    } else {
        "helper_spawn_failed"
    };
    anyhow::Error::new(error).context(FailureCode(code.into()))
}

#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
pub fn exit_code(status: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("helper_exit_signal_{signal}");
        }
    }
    match status.code() {
        Some(code) => format!("helper_exit_code_{}", code as u32),
        None => "helper_exit_status_unknown".into(),
    }
}

pub fn failure_code(error: &anyhow::Error, fallback: &str) -> String {
    if let Some(code) = error.downcast_ref::<FailureCode>() {
        return code.0.clone();
    }
    if let Some(error) = error.downcast_ref::<mpsc::RecvTimeoutError>() {
        return match error {
            mpsc::RecvTimeoutError::Timeout => "helper_control_timeout",
            mpsc::RecvTimeoutError::Disconnected => "helper_control_closed",
        }
        .into();
    }
    if let Some(error) = error.downcast_ref::<std::io::Error>() {
        return match error.kind() {
            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::UnexpectedEof => {
                "helper_control_closed"
            }
            std::io::ErrorKind::InvalidData => "helper_control_protocol_invalid",
            std::io::ErrorKind::PermissionDenied => "helper_permission_denied",
            _ => fallback,
        }
        .into();
    }
    fallback.into()
}

#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
pub struct Pool {
    child: owned::OwnedHelper,
    input: Option<owned::Input>,
    output: Receiver<protocol::Observation>,
    reader: Option<std::thread::JoinHandle<()>>,
    runtime: PathBuf,
    service: Incarnation,
    instance: Incarnation,
    pending_status: bool,
    control_failure: Option<String>,
    health: PoolHealth,
    drains: Arc<Mutex<retirement::Drains>>,
    reaped: Option<ProviderReapWitness>,
}

#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
fn route_observation(
    observation: protocol::Observation,
    drains: &Mutex<retirement::Drains>,
    sender: &mpsc::SyncSender<protocol::Observation>,
) -> bool {
    match observation {
        protocol::Observation::Drain {
            ticket,
            observation,
        } => {
            if let Ok(mut drains) = drains.lock() {
                drains.observe(ticket, observation);
            }
            true
        }
        observation => sender.try_send(observation).is_ok(),
    }
}
#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
impl Drop for Pool {
    fn drop(&mut self) {
        if let Some(mut input) = self.input.take() {
            let _ = service_ipc::write_frame(&mut input, &protocol::Control::Stop);
        }
        let until = Instant::now() + Duration::from_secs(10);
        while self.reaped.is_none() {
            match self.child.try_wait() {
                Ok(Some(_)) => {}
                // Unknown wait state never authorizes a numerical PID signal.
                // Retain ownership/admission until native exit is observable.
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(25));
                    continue;
                }
                Ok(None) if Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(25));
                    continue;
                }
                Ok(None) => {}
            }
            if self.finish_native_reap().is_err() {
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}
#[cfg(any(not(feature = "local-inference"), any(test, feature = "test-support")))]
impl Pool {
    pub fn terminate(&mut self) -> Result<()> {
        self.finish_native_reap()
    }
    fn finish_native_reap(&mut self) -> Result<()> {
        if self.reaped.is_some() {
            return Ok(());
        }
        self.input.take();
        self.child.drain(Duration::from_secs(2))?;
        if let Some(reader) = self.reader.take() {
            reader
                .join()
                .map_err(|_| anyhow::anyhow!("helper reply reader failed during native reap"))?;
        }
        if let Ok(mut drains) = self.drains.lock() {
            drains.closed();
        }
        self.reaped = Some(ProviderReapWitness {
            service: self.service,
            helper: self.instance,
        });
        let _ =
            crate::local_ipc::remove_exited_endpoint(&self.runtime, &protocol::role(self.instance));
        Ok(())
    }
    pub fn reap_witness(&self) -> Option<ProviderReapWitness> {
        self.reaped
    }
    pub fn identity(&self) -> Incarnation {
        self.instance
    }
    pub fn process_id(&self) -> u32 {
        self.child.id()
    }
    #[cfg(windows)]
    pub fn descendant_rss(&self) -> Result<Vec<Option<u64>>> {
        Ok(self
            .child
            .member_pids()?
            .into_iter()
            .filter(|pid| *pid != self.process_id())
            .map(airk_kernel::service_resource_observations::rss)
            .collect())
    }
    #[cfg(not(feature = "local-inference"))]
    pub fn start(root: &Path, service: Incarnation, inference_threads: u16) -> Result<Self> {
        Self::start_from_main(
            root,
            service,
            inference_threads,
            &std::env::current_exe()?.canonicalize()?,
        )
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn start_fixture(root: &Path, service: Incarnation, main: &Path) -> Result<Self> {
        Self::start_from_main(root, service, 1, main)
    }
    fn start_from_main(
        root: &Path,
        service: Incarnation,
        inference_threads: u16,
        main: &Path,
    ) -> Result<Self> {
        let helper = runtime_pair::verify_helper(main)
            .context(FailureCode("helper_payload_verification_failed".into()))?;
        let instance = new_instance_id()?;
        let guarded = crate::embedding_pool_bootstrap::shared_mode(root)?;
        let mut command = Command::new(&helper);
        command
            .arg("pool")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = if guarded {
            owned::OwnedHelper::guarded(root, service, instance, main, &helper)?
        } else {
            owned::OwnedHelper::spawn(&mut command)?
        };
        let input = child.stdin.take().context("pool input missing")?;
        let mut output = child.stdout.take().context("pool output missing")?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let drains = Arc::new(Mutex::new(retirement::Drains::new(service, instance)));
        let mut pool = Self {
            child,
            input: Some(input),
            output: receiver,
            reader: None,
            runtime: root.into(),
            service,
            instance,
            pending_status: false,
            control_failure: None,
            health: PoolHealth {
                instance,
                ready_at: Instant::now(),
                last_ack_at: Instant::now(),
            },
            drains: drains.clone(),
            reaped: None,
        };
        pool.reader = Some(
            std::thread::Builder::new()
                .name("airk-pool-replies".into())
                .spawn(move || {
                    while let Ok(Some(observation)) = service_ipc::read_frame(&mut output) {
                        if !route_observation(observation, &drains, &sender) {
                            break;
                        }
                    }
                    if let Ok(mut drains) = drains.lock() {
                        drains.closed();
                    }
                })?,
        );
        if guarded {
            let namespace = shared::registry(root)?.incarnation()?;
            ensure!(
                matches!(pool.output.recv_timeout(Duration::from_secs(10))?,
                protocol::Observation::GuardedReady { namespace: observed, service: owner, pool: helper }
                if observed == namespace && owner == service && helper == instance),
                "guarded helper startup identity mismatch"
            );
        }
        service_ipc::write_frame(
            pool.input.as_mut().context("pool input missing")?,
            &protocol::Init {
                runtime: root.into(),
                service,
                pool: instance,
                inference_threads,
            },
        )?;
        match pool.output.recv_timeout(Duration::from_secs(10)) {
            Ok(protocol::Observation::Ready {
                pool: observed,
                role,
            }) if observed == instance && role == protocol::role(instance) => {}
            Ok(_) => anyhow::bail!(FailureCode("helper_start_identity_mismatch".into())),
            Err(error) => {
                let code = match pool.child.try_wait() {
                    Ok(Some(status)) => exit_code(status),
                    Ok(None) => match error {
                        mpsc::RecvTimeoutError::Timeout => "helper_start_timeout".into(),
                        mpsc::RecvTimeoutError::Disconnected => {
                            "helper_start_control_closed".into()
                        }
                    },
                    Err(_) => "helper_start_wait_unavailable".into(),
                };
                return Err(anyhow::Error::new(error).context(FailureCode(code)));
            }
        }
        pool.health.ready_at = Instant::now();
        pool.health.last_ack_at = pool.health.ready_at;
        Ok(pool)
    }
    pub fn exit_status(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }
    #[cfg(any(
        not(feature = "local-inference"),
        all(any(test, feature = "test-support"), unix)
    ))]
    pub fn containment_failed(&self, error: &std::io::Error) -> bool {
        #[cfg(unix)]
        {
            crate::unix_guardian::shared::owner_reaped(error, self.service, self.instance)
        }
        #[cfg(windows)]
        {
            let _ = error;
            false
        }
    }
    pub fn health(&self) -> PoolHealth {
        self.health
    }
    fn acknowledged(&mut self) {
        self.health.last_ack_at = Instant::now();
    }
    pub fn control_failure(&self) -> Option<&str> {
        self.control_failure.as_deref()
    }
    fn require_control(&self) -> Result<()> {
        if let Some(code) = &self.control_failure {
            anyhow::bail!(FailureCode(code.clone()));
        }
        Ok(())
    }
    fn finish_control<T>(&mut self, result: Result<T>) -> Result<T> {
        if let Err(error) = &result {
            self.control_failure = Some(failure_code(error, "helper_control_outcome_unknown"));
        }
        result
    }
    fn begin_control(&mut self) -> Result<()> {
        self.require_control()?;
        // A timed-out Status has one known outstanding reply. Finish that exact
        // observation before submitting another operation; do not replay it.
        self.finish_pending_status()
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn access(
        &mut self,
        worker: Incarnation,
        project: ProjectRegistration,
    ) -> Result<protocol::Access> {
        self.access_registered(worker, project, None)
    }

    pub fn access_worker(
        &mut self,
        identity: WorkerIdentity,
        project: ProjectRegistration,
    ) -> Result<protocol::Access> {
        ensure!(
            identity.project == project.binding,
            "provider worker project identity mismatch"
        );
        self.drains
            .lock()
            .map_err(|_| anyhow::anyhow!("provider retirement state unavailable"))?
            .validate_member(identity)?;
        self.access_registered(identity.worker, project, Some(identity))
    }

    fn access_registered(
        &mut self,
        worker: Incarnation,
        project: ProjectRegistration,
        worker_identity: Option<WorkerIdentity>,
    ) -> Result<protocol::Access> {
        self.begin_control()?;
        let first = new_instance_id()?;
        let second = new_instance_id()?;
        let token: Sha256Digest = framed_digest(
            b"airk-pool-capability/1",
            &[first.as_bytes(), second.as_bytes()],
        );
        let grant = protocol::Grant {
            service: self.service,
            pool: self.instance,
            project,
            worker,
            token,
        };
        let result = (|| {
            service_ipc::write_frame(
                self.input.as_mut().context("pool input missing")?,
                &protocol::Control::Register {
                    grant: grant.clone(),
                    worker_identity,
                },
            )?;
            match self.output.recv_timeout(Duration::from_secs(5))? {
                protocol::Observation::Registered => {}
                protocol::Observation::Failed => {
                    anyhow::bail!(FailureCode("helper_grant_registration_failed".into()))
                }
                _ => anyhow::bail!(FailureCode("helper_control_protocol_invalid".into())),
            }
            self.acknowledged();
            if let Some(identity) = worker_identity {
                self.drains
                    .lock()
                    .map_err(|_| anyhow::anyhow!("provider retirement state unavailable"))?
                    .registered(identity);
            }
            Ok(protocol::Access {
                runtime: self.runtime.clone(),
                role: protocol::role(self.instance),
                grant,
                in_process: false,
            })
        })();
        self.finish_control(result)
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn revoke(&mut self, worker: Incarnation) -> Result<()> {
        self.begin_control()?;
        let result = (|| {
            service_ipc::write_frame(
                self.input.as_mut().context("pool input missing")?,
                &protocol::Control::Revoke { worker },
            )?;
            match self.output.recv_timeout(Duration::from_secs(5))? {
                protocol::Observation::Revoked => {}
                protocol::Observation::Failed => {
                    anyhow::bail!(FailureCode("helper_revocation_rejected".into()))
                }
                _ => anyhow::bail!(FailureCode("helper_control_protocol_invalid".into())),
            }
            self.acknowledged();
            self.drains
                .lock()
                .map_err(|_| anyhow::anyhow!("provider retirement state unavailable"))?
                .revoked(worker);
            Ok(())
        })();
        self.finish_control(result)
    }

    /// Begin before ordinary fast revoke. The ticket is retained independently
    /// of a caller waiting for completion; control/EOF never proves drain.
    pub fn begin_retirement(&mut self, identity: WorkerIdentity) -> Result<ProviderDrainTicket> {
        let (ticket, send) = self
            .drains
            .lock()
            .map_err(|_| anyhow::anyhow!("provider retirement state unavailable"))?
            .begin(identity, new_instance_id()?)?;
        if send {
            self.write_drain(&ticket, protocol::Control::BeginDrain { ticket });
        }
        Ok(ticket)
    }

    /// Bounded local lookup; never starts or replays a helper operation.
    pub fn retirement_ticket(
        &self,
        identity: WorkerIdentity,
    ) -> Result<Option<ProviderDrainTicket>> {
        self.drains
            .lock()
            .map_err(|_| anyhow::anyhow!("provider retirement state unavailable"))?
            .ticket(identity)
    }

    pub fn poll_retirement(&mut self, ticket: &ProviderDrainTicket) -> Result<ProviderDrainState> {
        let (state, send) = self
            .drains
            .lock()
            .map_err(|_| anyhow::anyhow!("provider retirement state unavailable"))?
            .poll(ticket)?;
        if send && !self.write_drain(ticket, protocol::Control::PollDrain { ticket: *ticket }) {
            return Ok(ProviderDrainState::Unknown);
        }
        // A write failure leaves an unknown retained ticket, never a successful
        // revocation or a reason to kill a sibling's shared helper.
        Ok(state)
    }

    /// Call only after the exact engine ACK and combined native retirement
    /// proof have been durably recorded. Drop/canceled callers must not call it.
    pub fn finish_retirement(&mut self, ticket: &ProviderDrainTicket) -> Result<()> {
        let send = self
            .drains
            .lock()
            .map_err(|_| anyhow::anyhow!("provider retirement state unavailable"))?
            .forget(ticket)?;
        if send {
            self.write_drain(ticket, protocol::Control::ForgetDrain { ticket: *ticket });
        }
        Ok(())
    }

    fn write_drain(&mut self, ticket: &ProviderDrainTicket, command: protocol::Control) -> bool {
        let result = self
            .input
            .as_mut()
            .context("pool input missing")
            .and_then(|input| {
                service_ipc::write_frame(input, &command).map_err(anyhow::Error::from)
            });
        if result.is_err()
            && let Ok(mut drains) = self.drains.lock()
        {
            drains.failed_write(ticket);
        }
        result.is_ok()
    }
    pub fn status(&mut self) -> Result<protocol::Observation> {
        self.require_control()?;
        if !self.pending_status {
            let result = (|| {
                service_ipc::write_frame(
                    self.input.as_mut().context("pool input missing")?,
                    &protocol::Control::Status,
                )?;
                Ok(())
            })();
            self.finish_control(result)?;
            self.pending_status = true;
        }
        match self.output.recv_timeout(Duration::from_secs(3)) {
            Ok(observation @ protocol::Observation::Status { .. }) => {
                self.pending_status = false;
                self.acknowledged();
                Ok(observation)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Only this known, unconsumed Status reply may be collected later.
                Err(mpsc::RecvTimeoutError::Timeout.into())
            }
            Err(error) => self.finish_control(Err(error.into())),
            Ok(protocol::Observation::Failed) => {
                self.finish_control(Err(FailureCode("helper_status_rejected".into()).into()))
            }
            Ok(_) => self
                .finish_control(Err(
                    FailureCode("helper_control_protocol_invalid".into()).into()
                )),
        }
    }
    pub fn configure_threads(&mut self, threads: u16) -> Result<()> {
        self.begin_control()?;
        let result = (|| {
            service_ipc::write_frame(
                self.input.as_mut().context("pool input missing")?,
                &protocol::Control::ConfigureThreads { threads },
            )?;
            match self.output.recv_timeout(Duration::from_secs(3))? {
                protocol::Observation::Renewed => {}
                protocol::Observation::Failed => {
                    anyhow::bail!(FailureCode("helper_configuration_rejected".into()))
                }
                _ => anyhow::bail!(FailureCode("helper_control_protocol_invalid".into())),
            }
            self.acknowledged();
            Ok(())
        })();
        self.finish_control(result)
    }
    pub fn renew(&mut self, worker: Incarnation) -> Result<Renewal> {
        self.begin_control()?;
        let result = (|| {
            service_ipc::write_frame(
                self.input.as_mut().context("pool input missing")?,
                &protocol::Control::Renew { worker },
            )?;
            match self.output.recv_timeout(Duration::from_secs(3))? {
                protocol::Observation::Renewed => {
                    self.acknowledged();
                    Ok(Renewal::Renewed)
                }
                protocol::Observation::GrantMissing { worker: observed } if observed == worker => {
                    // A correlated missing membership is not proof that the pool is unhealthy.
                    self.acknowledged();
                    Ok(Renewal::Missing)
                }
                protocol::Observation::Failed => {
                    anyhow::bail!(FailureCode("helper_grant_renewal_rejected".into()))
                }
                _ => anyhow::bail!(FailureCode("helper_control_protocol_invalid".into())),
            }
        })();
        self.finish_control(result)
    }
    fn finish_pending_status(&mut self) -> Result<()> {
        if self.pending_status {
            ensure!(
                matches!(self.status()?, protocol::Observation::Status { .. }),
                "invalid deferred pool observation"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker(pool: &Pool) -> WorkerIdentity {
        WorkerIdentity {
            service: pool.service,
            worker: Incarnation::from_bytes([3; 16]),
            project: Sha256Digest::from_bytes([4; 32]),
            fencing_generation: 1,
        }
    }

    #[test]
    fn correlated_drain_reader_never_consumes_or_blocks_ordinary_control_reply() -> Result<()> {
        let service = Incarnation::from_bytes([1; 16]);
        let helper = Incarnation::from_bytes([2; 16]);
        let identity = WorkerIdentity {
            service,
            worker: Incarnation::from_bytes([3; 16]),
            project: Sha256Digest::from_bytes([4; 32]),
            fencing_generation: 1,
        };
        let drains = Mutex::new(retirement::Drains::new(service, helper));
        drains.lock().unwrap().registered(identity);
        let ticket = drains
            .lock()
            .unwrap()
            .begin(identity, Incarnation::from_bytes([5; 16]))?
            .0;
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.send(protocol::Observation::Renewed)?;
        let mut frame = Vec::new();
        service_ipc::write_frame(
            &mut frame,
            &protocol::Observation::Drain {
                ticket,
                observation: protocol::ProviderDrainObservation::Drained,
            },
        )?;
        let observation = service_ipc::read_frame(&mut std::io::Cursor::new(frame))?
            .context("missing actual drain frame")?;
        assert!(route_observation(observation, &drains, &sender));
        assert!(matches!(
            receiver.try_recv()?,
            protocol::Observation::Renewed
        ));
        assert_eq!(
            drains.lock().unwrap().poll(&ticket)?,
            (ProviderDrainState::Drained, false)
        );
        assert!(receiver.try_recv().is_err());
        Ok(())
    }

    #[test]
    fn canceled_provider_waiter_keeps_ticket_and_sibling_control_healthy() -> Result<()> {
        let (mut pool, sender, _owner, frames) = held_control()?;
        let identity = worker(&pool);
        pool.drains.lock().unwrap().registered(identity);
        let ticket = pool.begin_retirement(identity)?;
        assert_eq!(pool.retirement_ticket(identity)?, Some(ticket));
        assert_eq!(pool.poll_retirement(&ticket)?, ProviderDrainState::Pending);
        assert!(pool.finish_retirement(&ticket).is_err());
        // A correlated drain has no waiter on the ordinary response channel.
        sender.send(protocol::Observation::Renewed)?;
        assert_eq!(
            pool.renew(Incarnation::from_bytes([8; 16]))?,
            Renewal::Renewed
        );
        assert!(pool.control_failure().is_none());
        assert!(pool.child.try_wait()?.is_none());
        pool.drains.lock().unwrap().closed();
        assert_eq!(pool.poll_retirement(&ticket)?, ProviderDrainState::Unknown);
        assert!(pool.control_failure().is_none());
        assert_eq!(pool.retirement_ticket(identity)?, Some(ticket));
        let controls = written_controls(&mut pool, &frames)?;
        assert_eq!(controls.len(), 2);
        assert!(matches!(controls[0], protocol::Control::BeginDrain { .. }));
        assert!(matches!(controls[1], protocol::Control::Renew { .. }));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn native_helper_reap_witness_requires_descendant_exit_and_reply_reader_completion()
    -> Result<()> {
        use std::io::Read;
        let owner = tempfile::tempdir()?;
        let runtime = owner.path().join("runtime");
        crate::local_ipc::prepare_root(&runtime)?;
        let ready = owner.path().join("descendant-ready");
        let lease = owner.path().join("descendant.lock");
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "embedding_pool_process::tests::owned_helper_parent_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("AIRK_HELPER_DESCENDANT_READY", &ready)
            .env("AIRK_HELPER_DESCENDANT_LEASE", &lease)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = owned::OwnedHelper::spawn(&mut command)?;
        let input = child.stdin.take();
        let mut output = child
            .stdout
            .take()
            .context("missing helper reader fixture")?;
        let reader_finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let finished = reader_finished.clone();
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            output.read_to_end(&mut bytes).unwrap();
            finished.store(true, std::sync::atomic::Ordering::Release);
        });
        let (_sender, receiver) = mpsc::sync_channel(1);
        let instance = Incarnation::from_bytes([2; 16]);
        let service = Incarnation::from_bytes([1; 16]);
        let mut pool = Pool {
            child,
            input,
            output: receiver,
            reader: Some(reader),
            runtime,
            service,
            instance,
            pending_status: false,
            control_failure: None,
            health: PoolHealth {
                instance,
                ready_at: Instant::now(),
                last_ack_at: Instant::now(),
            },
            drains: Arc::new(Mutex::new(retirement::Drains::new(service, instance))),
            reaped: None,
        };
        let until = Instant::now() + Duration::from_secs(10);
        while !ready.try_exists()? || pool.child.try_wait()?.is_none() {
            ensure!(
                Instant::now() < until,
                "owned descendant fixture did not become ready"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let guard = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lease)?;
        assert!(
            matches!(guard.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
            "descendant was not holding native resources"
        );
        assert!(!reader_finished.load(std::sync::atomic::Ordering::Acquire));
        assert!(
            pool.reap_witness().is_none(),
            "leader exit alone produced a reap proof"
        );
        pool.terminate()?;
        guard.try_lock()?;
        assert!(reader_finished.load(std::sync::atomic::Ordering::Acquire));
        let witness = pool
            .reap_witness()
            .context("missing actual native reap witness")?;
        assert!(witness.matches(service, instance));
        assert!(!witness.matches(service, Incarnation::from_bytes([9; 16])));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "actual owned helper parent fixture invoked by native reap test"]
    fn owned_helper_parent_fixture() -> Result<()> {
        let Some(ready) = std::env::var_os("AIRK_HELPER_DESCENDANT_READY") else {
            return Ok(());
        };
        let lease = std::env::var_os("AIRK_HELPER_DESCENDANT_LEASE")
            .context("missing native lease path")?;
        let mut child = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "embedding_pool_process::tests::owned_helper_descendant_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("AIRK_HELPER_DESCENDANT_READY", &ready)
            .env("AIRK_HELPER_DESCENDANT_LEASE", lease)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::null())
            .spawn()?;
        let until = Instant::now() + Duration::from_secs(10);
        while !Path::new(&ready).try_exists()? {
            ensure!(
                Instant::now() < until,
                "native descendant did not acquire its resource"
            );
            ensure!(
                child.try_wait()?.is_none(),
                "native descendant fixture exited early"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        // Intentionally leave a real descendant and inherited reply pipe after
        // leader exit. Only canonical native group drain may finish the proof.
        std::mem::forget(child);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "actual inherited-group descendant fixture"]
    fn owned_helper_descendant_fixture() -> Result<()> {
        let Some(ready) = std::env::var_os("AIRK_HELPER_DESCENDANT_READY") else {
            return Ok(());
        };
        let lease = std::env::var_os("AIRK_HELPER_DESCENDANT_LEASE")
            .context("missing native lease path")?;
        let guard = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(lease)?;
        guard.try_lock()?;
        std::fs::write(ready, b"owned")?;
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    // The child records real writes to its private stdin. Replies are controlled
    // independently so a late acknowledgment cannot accidentally satisfy a new
    // request in the test or in the owning Pool implementation.
    fn held_control() -> Result<(
        Pool,
        mpsc::SyncSender<protocol::Observation>,
        tempfile::TempDir,
        PathBuf,
    )> {
        let owner = tempfile::tempdir()?;
        let runtime = owner.path().join("runtime");
        crate::local_ipc::prepare_root(&runtime)?;
        let frames = owner.path().join("frames.json");
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "embedding_pool_process::tests::uninitialized_child_fixture",
                "--ignored",
            ])
            .env("AIRK_HELD_CONTROL_FRAMES", &frames)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = owned::OwnedHelper::spawn(&mut command)?;
        let input = child.stdin.take().context("fixture stdin missing")?;
        let (sender, output) = mpsc::sync_channel(4);
        let instance = Incarnation::from_bytes([2; 16]);
        let now = Instant::now();
        Ok((
            Pool {
                child,
                input: Some(input),
                output,
                reader: None,
                runtime,
                service: Incarnation::from_bytes([1; 16]),
                instance,
                pending_status: false,
                control_failure: None,
                health: PoolHealth {
                    instance,
                    ready_at: now,
                    last_ack_at: now,
                },
                drains: Arc::new(Mutex::new(retirement::Drains::new(
                    Incarnation::from_bytes([1; 16]),
                    instance,
                ))),
                reaped: None,
            },
            sender,
            owner,
            frames,
        ))
    }

    fn written_controls(pool: &mut Pool, frames: &Path) -> Result<Vec<protocol::Control>> {
        // EOF proves that the child consumed all writes before inspecting its
        // receipt, avoiding a timing assertion about its current scheduling.
        pool.input.take();
        ensure!(pool.child.wait()?.success(), "control fixture failed");
        Ok(serde_json::from_slice(&std::fs::read(frames)?)?)
    }

    #[test]
    fn ambiguous_mutations_fence_late_acknowledgments_and_every_later_control() -> Result<()> {
        for revoke in [false, true] {
            let (mut pool, sender, owner, frames) = held_control()?;
            let worker = Incarnation::from_bytes([3; 16]);
            let failed = if revoke {
                pool.revoke(worker)
            } else {
                pool.configure_threads(2)
            };
            assert!(failed.is_err());
            assert_eq!(pool.control_failure(), Some("helper_control_timeout"));
            sender.send(if revoke {
                protocol::Observation::Revoked
            } else {
                protocol::Observation::Renewed
            })?;
            let source = owner.path().join("source");
            let data = owner.path().join("data");
            std::fs::create_dir(&source)?;
            std::fs::create_dir(&data)?;
            let project =
                airk_kernel::service_projects::registration(&source, &data, "held-control")?;
            let health = pool.health();
            assert!(
                pool.configure_threads(1).is_err(),
                "rollback reused an ambiguous stream"
            );
            assert!(
                pool.renew(worker).is_err(),
                "late Renewed satisfied renewal"
            );
            assert!(pool.revoke(worker).is_err());
            assert!(pool.access(worker, project).is_err());
            assert!(pool.status().is_err());
            assert_eq!(pool.health().last_ack_at, health.last_ack_at);
            assert!(
                matches!(
                    (revoke, pool.output.try_recv()?),
                    (true, protocol::Observation::Revoked)
                        | (false, protocol::Observation::Renewed)
                ),
                "a later control consumed the obsolete acknowledgment"
            );
            let controls = written_controls(&mut pool, &frames)?;
            assert_eq!(controls.len(), 1, "poisoned controls wrote another request");
            assert!(matches!(
                (revoke, &controls[0]),
                (true, protocol::Control::Revoke { .. })
                    | (false, protocol::Control::ConfigureThreads { threads: 2 })
            ));
        }
        Ok(())
    }

    #[test]
    fn pending_status_finishes_its_exact_reply_before_a_mutation_without_replay() -> Result<()> {
        let (mut pool, sender, _owner, frames) = held_control()?;
        assert!(pool.status().is_err());
        assert!(pool.pending_status);
        assert!(pool.control_failure().is_none());
        sender.send(protocol::Observation::Status {
            loads: 0,
            reuses: 0,
            sessions: 0,
            age_ms: 0,
        })?;
        sender.send(protocol::Observation::Renewed)?;
        pool.configure_threads(2)?;
        assert!(!pool.pending_status);
        assert!(pool.control_failure().is_none());
        let controls = written_controls(&mut pool, &frames)?;
        assert_eq!(controls.len(), 2);
        assert!(matches!(controls[0], protocol::Control::Status));
        assert!(matches!(
            controls[1],
            protocol::Control::ConfigureThreads { threads: 2 }
        ));
        Ok(())
    }

    #[test]
    fn invalid_or_disconnected_status_poison_but_correlated_missing_grants_do_not() -> Result<()> {
        for invalid in [false, true] {
            let (mut pool, sender, _owner, frames) = held_control()?;
            if invalid {
                sender.send(protocol::Observation::Renewed)?;
            }
            drop(sender);
            assert!(pool.status().is_err());
            assert_eq!(
                pool.control_failure(),
                Some(if invalid {
                    "helper_control_protocol_invalid"
                } else {
                    "helper_control_closed"
                })
            );
            assert!(pool.configure_threads(1).is_err());
            assert_eq!(written_controls(&mut pool, &frames)?.len(), 1);
        }
        let (mut pool, sender, _owner, frames) = held_control()?;
        let worker = Incarnation::from_bytes([3; 16]);
        sender.send(protocol::Observation::GrantMissing { worker })?;
        assert_eq!(pool.renew(worker)?, Renewal::Missing);
        assert!(pool.control_failure().is_none());
        sender.send(protocol::Observation::Renewed)?;
        pool.configure_threads(2)?;
        assert!(pool.control_failure().is_none());
        assert_eq!(written_controls(&mut pool, &frames)?.len(), 2);
        Ok(())
    }

    #[test]
    fn valid_negative_replies_keep_their_rejection_reason() -> Result<()> {
        for (operation, code) in [
            ("register", "helper_grant_registration_failed"),
            ("configure", "helper_configuration_rejected"),
            ("revoke", "helper_revocation_rejected"),
            ("renew", "helper_grant_renewal_rejected"),
            ("status", "helper_status_rejected"),
        ] {
            let (mut pool, sender, owner, frames) = held_control()?;
            sender.send(protocol::Observation::Failed)?;
            let worker = Incarnation::from_bytes([3; 16]);
            let error = match operation {
                "register" => {
                    let source = owner.path().join("source");
                    let data = owner.path().join("data");
                    std::fs::create_dir(&source)?;
                    std::fs::create_dir(&data)?;
                    let project = airk_kernel::service_projects::registration(
                        &source,
                        &data,
                        "negative-control",
                    )?;
                    pool.access(worker, project).map(|_| ())
                }
                "configure" => pool.configure_threads(2),
                "revoke" => pool.revoke(worker),
                "renew" => pool.renew(worker).map(|_| ()),
                "status" => pool.status().map(|_| ()),
                _ => unreachable!(),
            }
            .unwrap_err();
            assert_eq!(failure_code(&error, "unexpected"), code);
            assert_eq!(pool.control_failure(), Some(code));
            assert!(pool.status().is_err());
            assert_eq!(written_controls(&mut pool, &frames)?.len(), 1);
        }
        Ok(())
    }

    #[test]
    fn containment_failure_reaps_exact_uninitialized_child() -> Result<()> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "embedding_pool_process::tests::uninitialized_child_fixture",
                "--ignored",
            ])
            .env("AIRK_UNINITIALIZED_HELPER_FIXTURE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = airk_host::native_process::spawn(&mut command)?;
        let result = airk_host::native_process::attach_owned(&mut child, |_| {
            anyhow::bail!("injected containment failure")
        });
        assert!(result.is_err());
        assert!(child.try_wait()?.is_some(), "owned child was not reaped");
        Ok(())
    }

    #[test]
    #[ignore = "owned child fixture invoked only by containment/control tests"]
    fn uninitialized_child_fixture() -> Result<()> {
        if let Some(path) = std::env::var_os("AIRK_HELD_CONTROL_FRAMES") {
            let mut controls = Vec::new();
            while let Some(control) =
                service_ipc::read_frame::<_, protocol::Control>(&mut std::io::stdin().lock())?
            {
                let stop = matches!(control, protocol::Control::Stop);
                controls.push(control);
                std::fs::write(&path, serde_json::to_vec(&controls)?)?;
                if stop {
                    break;
                }
            }
            return Ok(());
        }
        if std::env::var_os("AIRK_UNINITIALIZED_HELPER_FIXTURE").is_some() {
            std::thread::sleep(Duration::from_secs(60));
        }
        Ok(())
    }

    #[test]
    fn failure_codes_preserve_observed_exit_and_control_conditions() {
        assert_eq!(
            failure_code(
                &spawn_failure(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
                "fallback"
            ),
            "helper_permission_denied"
        );
        assert_eq!(
            failure_code(
                &spawn_failure(std::io::Error::from(std::io::ErrorKind::WouldBlock)),
                "fallback"
            ),
            "helper_spawn_failed"
        );
        let timeout = anyhow::Error::new(mpsc::RecvTimeoutError::Timeout);
        assert_eq!(failure_code(&timeout, "fallback"), "helper_control_timeout");
        let closed = anyhow::Error::new(mpsc::RecvTimeoutError::Disconnected);
        assert_eq!(failure_code(&closed, "fallback"), "helper_control_closed");
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            assert_eq!(
                exit_code(ExitStatus::from_raw(0xc0000005)),
                "helper_exit_code_3221225477"
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(exit_code(ExitStatus::from_raw(9)), "helper_exit_signal_9");
            assert_eq!(
                exit_code(ExitStatus::from_raw(7 << 8)),
                "helper_exit_code_7"
            );
        }
    }
}
