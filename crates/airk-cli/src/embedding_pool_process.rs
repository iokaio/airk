// SPDX-License-Identifier: Apache-2.0
//! Service-owned verified inference child. Private control contains no embedding inputs.
use crate::{embedding_pool_protocol as protocol, runtime_pair, service_ipc};
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    service::ProjectRegistration,
};
use airk_kernel::service_broker::new_instance_id;
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

pub struct Pool {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<protocol::Observation>,
    reader: Option<std::thread::JoinHandle<()>>,
    runtime: PathBuf,
    service: Incarnation,
    instance: Incarnation,
    pending_status: bool,
}
impl Drop for Pool {
    fn drop(&mut self) {
        if let Some(mut input) = self.input.take() {
            let _ = service_ipc::write_frame(&mut input, &protocol::Control::Stop);
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
impl Pool {
    pub fn process_id(&self) -> u32 {
        self.child.id()
    }
    pub fn start(root: &Path, service: Incarnation) -> Result<Self> {
        let helper = runtime_pair::verify_helper(&std::env::current_exe()?.canonicalize()?)?;
        let instance = new_instance_id()?;
        let mut command = Command::new(helper);
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
        let mut child = command.spawn()?;
        let input = child.stdin.take().context("pool input missing")?;
        let mut output = child.stdout.take().context("pool output missing")?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut pool = Self {
            child,
            input: Some(input),
            output: receiver,
            reader: None,
            runtime: root.into(),
            service,
            instance,
            pending_status: false,
        };
        pool.reader = Some(
            std::thread::Builder::new()
                .name("airk-pool-replies".into())
                .spawn(move || {
                    while let Ok(Some(observation)) = service_ipc::read_frame(&mut output) {
                        if sender.try_send(observation).is_err() {
                            break;
                        }
                    }
                })?,
        );
        service_ipc::write_frame(
            pool.input.as_mut().context("pool input missing")?,
            &protocol::Init {
                runtime: root.into(),
                service,
                pool: instance,
            },
        )?;
        ensure!(
            matches!(pool.output.recv_timeout(Duration::from_secs(10))?,protocol::Observation::Ready{pool,role} if pool==instance&&role==protocol::role(instance)),
            "pool identity mismatch"
        );
        Ok(pool)
    }
    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
    pub fn access(
        &mut self,
        worker: Incarnation,
        project: ProjectRegistration,
    ) -> Result<protocol::Access> {
        self.finish_pending_status()?;
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
        service_ipc::write_frame(
            self.input.as_mut().context("pool input missing")?,
            &protocol::Control::Register {
                grant: grant.clone(),
            },
        )?;
        ensure!(
            matches!(
                self.output.recv_timeout(Duration::from_secs(5))?,
                protocol::Observation::Registered
            ),
            "pool rejected project capability"
        );
        Ok(protocol::Access {
            runtime: self.runtime.clone(),
            role: protocol::role(self.instance),
            grant,
        })
    }
    pub fn revoke(&mut self, worker: Incarnation) -> Result<()> {
        self.finish_pending_status()?;
        service_ipc::write_frame(
            self.input.as_mut().context("pool input missing")?,
            &protocol::Control::Revoke { worker },
        )?;
        ensure!(
            matches!(
                self.output.recv_timeout(Duration::from_secs(5))?,
                protocol::Observation::Revoked
            ),
            "pool revocation response unknown"
        );
        Ok(())
    }
    pub fn status(&mut self) -> Result<protocol::Observation> {
        if !self.pending_status {
            service_ipc::write_frame(
                self.input.as_mut().context("pool input missing")?,
                &protocol::Control::Status,
            )?;
            self.pending_status = true;
        }
        let observation = self.output.recv_timeout(Duration::from_secs(3))?;
        self.pending_status = false;
        Ok(observation)
    }
    pub fn renew(&mut self, worker: Incarnation) -> Result<()> {
        self.finish_pending_status()?;
        service_ipc::write_frame(
            self.input.as_mut().context("pool input missing")?,
            &protocol::Control::Renew { worker },
        )?;
        ensure!(
            matches!(
                self.output.recv_timeout(Duration::from_secs(3))?,
                protocol::Observation::Renewed
            ),
            "pool allowance renewal failed"
        );
        Ok(())
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
