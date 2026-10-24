// SPDX-License-Identifier: Apache-2.0
//! Provider lifetime adapter. Source/result pipes connect the provider directly to its worker.
use crate::service_ipc::{read_frame, write_frame};
use airk_core::{identity::Incarnation, service::WorkerIdentity};
use airk_process::{
    GroupChild,
    child_group::{GroupOwner, ProviderLimits, ProviderSpawner},
};
use airk_store::service::{
    ProjectLease,
    registry::{ProviderDrain, ServiceRegistry},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{fs::MetadataExt, net::UnixStream, process::ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, OnceLock},
    time::Duration,
};
#[path = "unix_guardian_native.rs"]
mod native;
static PIN: OnceLock<File> = OnceLock::new();

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Init {
    version: u32,
    runtime: PathBuf,
    namespace: Incarnation,
    identity: WorkerIdentity,
    id: Incarnation,
    program: PathBuf,
    args: Vec<String>,
    directory: PathBuf,
    environment: Vec<(String, Option<String>)>,
    limits: Option<(usize, u64)>,
}
#[derive(Serialize, Deserialize)]
enum Request {
    Peek,
    Kill,
    Drain { millis: u64 },
}
#[derive(Serialize, Deserialize)]
enum Reply {
    Ready { group: u32 },
    Status { raw: Option<i32> },
    Killed,
    Drained { raw: i32 },
    Failed,
}

pub fn install(
    runtime: &Path,
    namespace: Incarnation,
    identity: WorkerIdentity,
    lease: &ProjectLease,
    executable: &Path,
) -> Result<()> {
    airk_process::child_group::install_provider_spawner(Arc::new(Spawner {
        runtime: runtime.canonicalize()?,
        namespace,
        identity,
        pin: lease.duplicate_native_pin()?,
        executable: executable.canonicalize()?,
    }))?;
    Ok(())
}
struct Spawner {
    runtime: PathBuf,
    namespace: Incarnation,
    identity: WorkerIdentity,
    pin: File,
    executable: PathBuf,
}
fn text(value: &std::ffi::OsStr) -> io::Result<String> {
    value
        .to_str()
        .filter(|s| s.len() <= 32768 && !s.contains('\0'))
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("provider command is not bounded UTF-8"))
}
impl ProviderSpawner for Spawner {
    fn spawn(&self, command: &Command, limits: Option<ProviderLimits>) -> io::Result<GroupChild> {
        let args = command
            .get_args()
            .map(text)
            .collect::<io::Result<Vec<_>>>()?;
        let environment = command
            .get_envs()
            .map(|(k, v)| Ok((text(k)?, v.map(text).transpose()?)))
            .collect::<io::Result<Vec<_>>>()?;
        if args.len() > 256 || environment.len() > 128 {
            return Err(io::Error::other("provider argument count exceeded"));
        }
        let init = Init {
            version: 1,
            runtime: self.runtime.clone(),
            namespace: self.namespace,
            identity: self.identity,
            id: airk_kernel::service_broker::new_instance_id().map_err(io::Error::other)?,
            program: PathBuf::from(command.get_program()).canonicalize()?,
            args,
            directory: command
                .get_current_dir()
                .map(Path::to_path_buf)
                .unwrap_or(std::env::current_dir()?),
            environment,
            limits: limits.map(|l| (l.memory_bytes, l.cpu_seconds)),
        };
        // Serialize/admit the complete command before creating the guardian.
        let mut frame = Vec::new();
        write_frame(&mut frame, &init)?;
        let (mut socket, peer) = UnixStream::pair()?;
        socket.set_read_timeout(Some(Duration::from_secs(5)))?;
        socket.set_write_timeout(Some(Duration::from_secs(5)))?;
        let (input_read, input_write) = std::io::pipe()?;
        let (output_read, output_write) = std::io::pipe()?;
        let (error_read, error_write) = std::io::pipe()?;
        let input_read = File::from(std::os::fd::OwnedFd::from(input_read));
        let output_write = File::from(std::os::fd::OwnedFd::from(output_write));
        let error_write = File::from(std::os::fd::OwnedFd::from(error_write));
        let child = Command::new(&self.executable)
            .arg("process-guardian")
            .stdin(Stdio::from(std::os::fd::OwnedFd::from(peer.try_clone()?)))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        drop(peer);
        let mut owner = Owner {
            child,
            socket: None,
            runtime: self.runtime.clone(),
            identity: self.identity,
            id: init.id,
            group: 0,
            finished: None,
        };
        let handshake = (|| {
            native::send_files(
                &socket,
                &[&input_read, &output_write, &error_write, &self.pin],
            )?;
            use std::io::Write;
            socket.write_all(&frame)?;
            match read_frame(&mut socket)? {
                Some(Reply::Ready { group }) if group > 1 => Ok(group),
                _ => Err(io::Error::other("provider guardian admission failed")),
            }
        })();
        drop((input_read, output_write, error_write));
        match handshake {
            Ok(group) => {
                owner.group = group;
                owner.socket = Some(socket);
                Ok(GroupChild::from_owner(
                    Box::new(owner),
                    File::from(std::os::fd::OwnedFd::from(input_write)),
                    File::from(std::os::fd::OwnedFd::from(output_read)),
                    File::from(std::os::fd::OwnedFd::from(error_read)),
                ))
            }
            Err(error) => {
                drop(socket);
                owner.finish_uncertain();
                Err(error)
            }
        }
    }
}
struct Owner {
    child: Child,
    socket: Option<UnixStream>,
    runtime: PathBuf,
    identity: WorkerIdentity,
    id: Incarnation,
    group: u32,
    finished: Option<ExitStatus>,
}
impl Owner {
    fn request(&mut self, request: Request) -> io::Result<Reply> {
        let result = (|| {
            let socket = self
                .socket
                .as_mut()
                .ok_or_else(|| io::Error::other("guardian disconnected"))?;
            write_frame(socket, &request)?;
            read_frame(socket)?.ok_or_else(|| io::Error::other("guardian closed"))
        })();
        if result.is_err() {
            self.socket = None
        }
        result
    }
    fn proven(&mut self) -> Result<bool> {
        let mut registry = ServiceRegistry::open(&self.runtime, self.identity.service)?;
        registry.recover_provider_drains(&native::boot()?, &[], native::absent)?;
        Ok(self.child.try_wait()?.is_some()
            && !registry.provider_drains()?.iter().any(|r| r.id == self.id))
    }
    fn finish_uncertain(&mut self) {
        self.socket = None;
        while !self.proven().unwrap_or(false) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.wait();
    }
}
impl GroupOwner for Owner {
    fn id(&self) -> u32 {
        self.group
    }
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.finished {
            return Ok(Some(status));
        }
        match self.request(Request::Peek)? {
            Reply::Status { raw } => Ok(raw.map(ExitStatus::from_raw)),
            _ => Err(io::Error::other("invalid guardian status")),
        }
    }
    fn kill(&mut self) -> io::Result<()> {
        match self.request(Request::Kill)? {
            Reply::Killed => Ok(()),
            _ => Err(io::Error::other("guardian kill failed")),
        }
    }
    fn terminate_and_wait(&mut self, timeout: Duration) -> io::Result<ExitStatus> {
        if let Some(status) = self.finished {
            return Ok(status);
        }
        if self.socket.is_none() {
            if self.proven().map_err(io::Error::other)? {
                let status = ExitStatus::from_raw(libc::SIGKILL);
                self.finished = Some(status);
                return Ok(status);
            }
            return Err(io::Error::other("provider ownership remains uncertain"));
        }
        match self.request(Request::Drain {
            millis: timeout.as_millis().clamp(1, 2000) as u64,
        })? {
            Reply::Drained { raw } => {
                self.child.wait()?;
                self.socket = None;
                let status = ExitStatus::from_raw(raw);
                self.finished = Some(status);
                Ok(status)
            }
            _ => Err(io::Error::other("provider guardian still draining")),
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if self.finished.is_none() {
            self.finish_uncertain()
        }
    }
}

/// Dispatched before the executable creates its async runtime. No source bodies
/// cross this role's control socket, service registry or diagnostic output.
pub fn serve() -> Result<()> {
    // SAFETY: duplicate inherited private socket without taking ownership of stdin.
    let fd = unsafe { libc::fcntl(0, libc::F_DUPFD_CLOEXEC, 3) };
    ensure!(fd >= 0, "guardian requires private socket");
    let mut socket = unsafe { UnixStream::from_raw_fd(fd) };
    let [input, output, error, pin] = native::receive_files(&socket)?;
    let init: Init = read_frame(&mut socket)?.context("guardian initialization missing")?;
    ensure!(
        init.version == 1
            && init.args.len() <= 256
            && init.environment.len() <= 128
            && init.program.is_absolute()
            && init.directory.is_absolute(),
        "invalid guardian initialization"
    );
    ensure!(
        init.runtime.is_absolute() && init.runtime.canonicalize()? == init.runtime,
        "guardian runtime must be canonical"
    );
    crate::local_ipc::prepare_root(&init.runtime)?;
    let expected = std::fs::symlink_metadata(
        init.runtime
            .join(format!("worker-1.4-{}.lock", init.identity.project)),
    )?;
    let actual = pin.metadata()?;
    ensure!(
        expected.is_file()
            && !expected.file_type().is_symlink()
            && actual.is_file()
            && actual.len() == 0
            && expected.len() == 0
            && actual.nlink() == 1
            && expected.nlink() == 1
            && actual.mode() & 0o077 == 0
            && actual.dev() == expected.dev()
            && actual.ino() == expected.ino()
            && actual.uid() == unsafe { libc::geteuid() },
        "guardian lease identity mismatch"
    );
    pin.try_lock()?;
    ensure!(PIN.set(pin).is_ok(), "guardian pin already installed");
    let mut registry = ServiceRegistry::open(&init.runtime, init.identity.service)?;
    ensure!(
        registry.incarnation()? == init.namespace,
        "guardian registry namespace changed"
    );
    let pending = registry.begin_provider_drain(
        init.identity,
        init.id,
        std::process::id(),
        native::boot()?,
    )?;
    // EOF before spawn does not authorize executing a late provider.
    let mut byte = [0u8; 1];
    let observed = unsafe {
        libc::recv(
            socket.as_raw_fd(),
            byte.as_mut_ptr().cast(),
            1,
            libc::MSG_PEEK | libc::MSG_DONTWAIT,
        )
    };
    if observed == 0 {
        registry.complete_provider_drain(&pending)?;
        return Ok(());
    }
    let mut command = Command::new(&init.program);
    command
        .args(&init.args)
        .current_dir(&init.directory)
        .stdin(Stdio::from(input))
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(error));
    for (key, value) in &init.environment {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    if let Some((memory_bytes, cpu_seconds)) = init.limits {
        ProviderLimits {
            memory_bytes,
            cpu_seconds,
        }
        .configure(&mut command)
    }
    let result = GroupChild::spawn(&mut command);
    drop(command);
    let mut child = match result {
        Ok(child) => child,
        Err(_) => {
            registry.complete_provider_drain(&pending)?;
            let _ = write_frame(&mut socket, &Reply::Failed);
            return Ok(());
        }
    };
    let record = match registry.bind_provider_group(&pending, child.id()) {
        Ok(record) => record,
        Err(error) => {
            child.drain();
            registry.complete_provider_drain(&pending)?;
            return Err(error);
        }
    };
    if write_frame(&mut socket, &Reply::Ready { group: child.id() }).is_err() {
        return drain(&mut child, &mut registry, &record);
    }
    loop {
        let reply = match read_frame::<_, Request>(&mut socket) {
            Ok(Some(Request::Peek)) => child.try_wait().map(|status| Reply::Status {
                raw: status.map(ExitStatusExt::into_raw),
            }),
            Ok(Some(Request::Kill)) => child.kill().map(|_| Reply::Killed),
            Ok(Some(Request::Drain { millis })) => {
                match child.terminate_and_wait(Duration::from_millis(millis.clamp(1, 2000))) {
                    Ok(status) => {
                        registry.complete_provider_drain(&record)?;
                        let _ = write_frame(
                            &mut socket,
                            &Reply::Drained {
                                raw: status.into_raw(),
                            },
                        );
                        return Ok(());
                    }
                    Err(_) => Ok(Reply::Failed),
                }
            }
            _ => return drain(&mut child, &mut registry, &record),
        }
        .unwrap_or(Reply::Failed);
        if write_frame(&mut socket, &reply).is_err() {
            return drain(&mut child, &mut registry, &record);
        }
    }
}
fn drain(
    child: &mut GroupChild,
    registry: &mut ServiceRegistry,
    record: &ProviderDrain,
) -> Result<()> {
    child.drain();
    registry.complete_provider_drain(record)
}

pub(super) fn recover(registry: &mut ServiceRegistry, active: &[WorkerIdentity]) -> Result<usize> {
    registry.recover_provider_drains(&native::boot()?, active, native::absent)
}
pub(super) fn rss(registry: &ServiceRegistry) -> Result<Vec<Option<u64>>> {
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    let boot = native::boot()?;
    let records = registry.provider_drains()?;
    ensure!(
        records
            .iter()
            .all(|r| r.boot.process_scope == boot.process_scope),
        "foreign provider process scope"
    );
    let groups = records
        .iter()
        .filter_map(|record| record.group)
        .collect::<Vec<_>>();
    let mut pids = native::members(&groups)?;
    pids.extend(records.iter().map(|record| record.guardian_pid));
    pids.sort_unstable();
    pids.dedup();
    let mut observations = Vec::new();
    for pid in pids {
        ensure!(
            std::time::Instant::now() < deadline,
            "provider RSS observation deadline"
        );
        observations.push(airk_kernel::service_resource_observations::rss(pid));
    }
    Ok(observations)
}
pub(super) fn wait_for_worker(runtime: &Path, identity: WorkerIdentity) {
    loop {
        let done = (|| -> Result<bool> {
            let mut registry = ServiceRegistry::open(runtime, identity.service)?;
            recover(&mut registry, &[])?;
            Ok(!registry
                .provider_drains()?
                .iter()
                .any(|r| r.identity == identity)
                && ProjectLease::acquire(runtime, identity.project).is_ok())
        })();
        if done.unwrap_or(false) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
