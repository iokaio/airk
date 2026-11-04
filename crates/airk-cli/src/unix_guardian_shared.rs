// SPDX-License-Identifier: Apache-2.0
//! Shared helpers have a real service/helper owner, never a synthetic project.
//! The pre-fork row and inherited pin cover every guarded startup actor.
use super::*;
use crate::embedding_pool_bootstrap::{GuardPermit, GuardedBootstrap};
use airk_store::service::{
    SharedHelperLease,
    registry::{
        SharedHelperContainment, SharedHelperDrain, SharedHelperIdentity, SharedHelperPhase,
    },
    shared_helper_pin_name,
};
use std::os::unix::process::CommandExt;

const ROOT: &str = "AIRK_SHARED_GUARD_ROOT";
const NAMESPACE: &str = "AIRK_SHARED_GUARD_NAMESPACE";
const SERVICE: &str = "AIRK_SHARED_GUARD_SERVICE";
const HELPER: &str = "AIRK_SHARED_GUARD_HELPER";
const FD: &str = "AIRK_SHARED_GUARD_PIN_FD";
static SHARED_PIN: OnceLock<File> = OnceLock::new();

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SharedInit {
    version: u32,
    context: GuardPermit,
    program: PathBuf,
}

struct Reservation {
    root: PathBuf,
    owner: SharedHelperIdentity,
    transferred: bool,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if !self.transferred {
            let _ = crate::embedding_pool_process::shared::complete(&self.root, self.owner);
        }
    }
}

fn bound_env(key: &str, maximum: usize) -> Result<String> {
    let value = std::env::var(key).context("shared guardian native environment is missing")?;
    ensure!(
        value.len() <= maximum && !value.contains('\0'),
        "shared guardian environment exceeds bound"
    );
    Ok(value)
}
fn install_environment(command: &mut Command, context: &GuardPermit, pin: &File) -> Result<()> {
    for key in [ROOT, NAMESPACE, SERVICE, HELPER, FD] {
        command.env_remove(key);
    }
    let root = context
        .runtime
        .to_str()
        .context("shared guardian root is not bounded UTF-8")?;
    ensure!(root.len() <= 4096, "shared guardian root exceeds bound");
    let fd = pin.as_raw_fd();
    ensure!(fd >= 3, "shared guardian pin overlaps standard I/O");
    command
        .env(ROOT, root)
        .env(NAMESPACE, context.namespace.to_string())
        .env(SERVICE, context.owner.service.to_string())
        .env(HELPER, context.owner.helper.to_string())
        .env(FD, fd.to_string());
    // SAFETY: the child callback performs only async-signal-safe fcntl on one
    // known inherited descriptor. No allocation, locks, or metadata I/O occurs.
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}

fn capture() -> Result<(GuardPermit, &'static File)> {
    let fd: i32 = bound_env(FD, 16)?.parse()?;
    // SAFETY: read-only descriptor validation precedes taking ownership.
    ensure!(
        fd >= 3 && unsafe { libc::fcntl(fd, libc::F_GETFD) } >= 0,
        "shared guardian inherited pin is invalid"
    );
    // SAFETY: this pre-runtime role owns the validated inherited descriptor.
    // Install it before any fallible validation so even rejection retains the
    // native pin until actual process exit, never merely until Rust unwinding.
    let pin = unsafe { File::from_raw_fd(fd) };
    ensure!(
        SHARED_PIN.set(pin).is_ok(),
        "shared guardian pin is already installed"
    );
    let pin = SHARED_PIN.get().expect("installed shared guardian pin");
    // SAFETY: private child descriptor; descendants require their own explicit
    // guarded inheritance, rather than accidental propagation through exec.
    ensure!(
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } >= 0,
        io::Error::last_os_error()
    );
    let context = GuardPermit {
        version: 2,
        runtime: PathBuf::from(bound_env(ROOT, 4096)?),
        namespace: bound_env(NAMESPACE, 32)?.parse()?,
        owner: SharedHelperIdentity {
            service: bound_env(SERVICE, 32)?.parse()?,
            helper: bound_env(HELPER, 32)?.parse()?,
        },
    };
    ensure!(
        context.runtime.is_absolute() && context.runtime.canonicalize()? == context.runtime,
        "shared guardian root must be canonical"
    );
    crate::local_ipc::require_existing_root(&context.runtime)?;
    let registry = ServiceRegistry::open_saved(&context.runtime)?;
    ensure!(
        registry.incarnation()? == context.namespace,
        "shared guardian namespace changed"
    );
    let record = registry
        .shared_helper_drain(context.owner)?
        .context("shared guardian reservation missing")?;
    let SharedHelperContainment::Unix {
        pin_device,
        pin_inode,
        ..
    } = record.containment
    else {
        anyhow::bail!("shared guardian platform changed");
    };
    let actual = pin.metadata()?;
    let expected =
        std::fs::symlink_metadata(context.runtime.join(shared_helper_pin_name(context.owner)))?;
    // SAFETY: scalar native identity query with no pointer arguments.
    let user = unsafe { libc::geteuid() };
    ensure!(
        actual.is_file()
            && actual.len() == 0
            && actual.nlink() == 1
            && actual.uid() == user
            && actual.mode() & 0o077 == 0
            && (actual.dev(), actual.ino()) == (pin_device, pin_inode)
            && expected.is_file()
            && !expected.file_type().is_symlink()
            && (expected.dev(), expected.ino()) == (pin_device, pin_inode),
        "shared guardian native pin identity changed"
    );
    pin.try_lock()?;
    Ok((context, pin))
}

fn socket() -> Result<UnixStream> {
    // SAFETY: duplicate the inherited private control socket without owning stdin.
    let fd = unsafe { libc::fcntl(0, libc::F_DUPFD_CLOEXEC, 3) };
    ensure!(fd >= 0, io::Error::last_os_error());
    // SAFETY: successful duplication transfers exactly this descriptor.
    Ok(unsafe { UnixStream::from_raw_fd(fd) })
}

pub(crate) fn spawn(
    root: &Path,
    service: Incarnation,
    helper: Incarnation,
    main: &Path,
    program: &Path,
) -> Result<GroupChild> {
    let root = root.canonicalize()?;
    let mut registry = crate::embedding_pool_process::shared::registry(&root)?;
    ensure!(
        registry.recorded_service_instance()? == Some(service),
        "shared guardian has another native service owner"
    );
    let owner = SharedHelperIdentity { service, helper };
    let lease = SharedHelperLease::acquire(&root, owner)?;
    let (device, inode) = lease.identity()?;
    let record = registry.begin_shared_helper_drain(
        owner,
        SharedHelperContainment::Unix {
            guardian_pid: None,
            group: None,
            pin_device: device,
            pin_inode: inode,
        },
        native::boot()?,
    )?;
    let context = GuardPermit {
        version: 2,
        runtime: root.clone(),
        namespace: record.namespace,
        owner,
    };
    let mut reservation = Reservation {
        root: root.clone(),
        owner,
        transferred: false,
    };
    let pin = lease.duplicate_native_pin()?;
    let (control, peer) = UnixStream::pair()?;
    control.set_read_timeout(Some(Duration::from_secs(5)))?;
    control.set_write_timeout(Some(Duration::from_secs(5)))?;
    let (input_read, input_write) = std::io::pipe()?;
    let (output_read, output_write) = std::io::pipe()?;
    let input = File::from(std::os::fd::OwnedFd::from(input_read));
    let output = File::from(std::os::fd::OwnedFd::from(output_write));
    let error = std::fs::OpenOptions::new().write(true).open("/dev/null")?;
    let error_read = File::open("/dev/null")?;
    let mut command = Command::new(main);
    command
        .arg("shared-process-guardian")
        .stdin(Stdio::from(std::os::fd::OwnedFd::from(peer.try_clone()?)))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    install_environment(&mut command, &context, &pin)?;
    let child = command.spawn();
    drop(command);
    drop(peer);
    let child = match child {
        Ok(child) => child,
        Err(error) => {
            drop((pin, lease));
            let _ = crate::embedding_pool_process::shared::complete(&root, owner);
            return Err(error.into());
        }
    };
    // The child already owns its inherited pin. Close every parent copy before
    // any uncertain cleanup waits for the pin to become free.
    drop((pin, lease));
    let mut owned = SharedOwner {
        child,
        socket: Some(control),
        root: root.clone(),
        namespace: record.namespace,
        owner,
        group: 0,
        finished: None,
        drained: false,
    };
    reservation.transferred = true;
    let handshake = (|| -> Result<u32> {
        let child_id = owned.child.id();
        let control = owned
            .socket
            .as_mut()
            .context("shared guardian control missing")?;
        let mut expected = record.clone();
        let SharedHelperContainment::Unix { guardian_pid, .. } = &mut expected.containment else {
            unreachable!()
        };
        *guardian_pid = Some(child_id);
        let result = registry.bind_shared_guardian(&record, child_id);
        crate::embedding_pool_process::shared::known_transition(&registry, result, &expected)?;
        let pin = airk_store::service::SharedHelperLease::open_pin(&root, owner)?;
        native::send_files(control, &[&input, &output, &error, &pin])?;
        write_frame(
            control,
            &SharedInit {
                version: 2,
                context: context.clone(),
                program: program.to_owned(),
            },
        )?;
        match read_frame(control)? {
            Some(Reply::Ready { group }) if group > 1 => Ok(group),
            _ => anyhow::bail!("shared guardian admission failed"),
        }
    })();
    drop((input, output, error));
    match handshake {
        Ok(group) => {
            owned.group = group;
            Ok(GroupChild::from_owner(
                Box::new(owned),
                File::from(std::os::fd::OwnedFd::from(input_write)),
                File::from(std::os::fd::OwnedFd::from(output_read)),
                error_read,
            ))
        }
        Err(error) => {
            drop((input_write, output_read));
            owned.finish_uncertain();
            Err(error)
        }
    }
}

struct SharedOwner {
    child: Child,
    socket: Option<UnixStream>,
    root: PathBuf,
    namespace: Incarnation,
    owner: SharedHelperIdentity,
    group: u32,
    finished: Option<ExitStatus>,
    drained: bool,
}
#[derive(Debug)]
struct SharedGuardianReaped {
    owner: SharedHelperIdentity,
    status: ExitStatus,
}
impl std::fmt::Display for SharedGuardianReaped {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "owned shared provider guardian exited: {}",
            self.status
        )
    }
}
impl std::error::Error for SharedGuardianReaped {}
pub(crate) fn owner_reaped(error: &io::Error, service: Incarnation, helper: Incarnation) -> bool {
    error
        .get_ref()
        .and_then(|error| error.downcast_ref::<SharedGuardianReaped>())
        .is_some_and(|error| error.owner == SharedHelperIdentity { service, helper })
}
impl SharedOwner {
    fn request(&mut self, request: Request) -> io::Result<Reply> {
        let result = (|| {
            let socket = self
                .socket
                .as_mut()
                .ok_or_else(|| io::Error::other("shared guardian disconnected"))?;
            write_frame(socket, &request)?;
            read_frame(socket)?.ok_or_else(|| io::Error::other("shared guardian closed"))
        })();
        if result.is_err() {
            self.socket = None;
        }
        result
    }
    fn proven(&mut self) -> Result<bool> {
        if self.child.try_wait()?.is_none() {
            return Ok(false);
        }
        let registry = ServiceRegistry::open_saved(&self.root)?;
        ensure!(
            registry.incarnation()? == self.namespace,
            "shared guardian namespace changed"
        );
        let record = registry
            .shared_helper_drain(self.owner)?
            .context("shared guardian ownership record missing")?;
        if record.phase != SharedHelperPhase::Drained && !recover(&record, &self.root)? {
            return Ok(false);
        }
        crate::embedding_pool_process::shared::complete(&self.root, self.owner)?;
        Ok(true)
    }
    fn finish_uncertain(&mut self) {
        self.socket = None;
        while !self.proven().unwrap_or(false) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.wait();
        self.drained = true;
    }
}
impl GroupOwner for SharedOwner {
    fn id(&self) -> u32 {
        self.group
    }
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.finished {
            return Ok(Some(status));
        }
        if self.drained {
            return Err(io::Error::other(
                "native helper drained; actual exit status unavailable after guardian loss",
            ));
        }
        let reply = match self.request(Request::Peek) {
            Ok(reply) => reply,
            Err(error) => {
                if let Some(status) = self.child.try_wait()? {
                    return Err(io::Error::other(SharedGuardianReaped {
                        owner: self.owner,
                        status,
                    }));
                }
                return Err(error);
            }
        };
        match reply {
            Reply::Status { raw } => Ok(raw.map(ExitStatus::from_raw)),
            _ => Err(io::Error::other("invalid shared guardian status")),
        }
    }
    fn kill(&mut self) -> io::Result<()> {
        if self.drained {
            return Ok(());
        }
        match self.request(Request::Kill)? {
            Reply::Killed => Ok(()),
            _ => Err(io::Error::other("shared guardian kill failed")),
        }
    }
    fn terminate_and_wait(&mut self, timeout: Duration) -> io::Result<ExitStatus> {
        if let Some(status) = self.finished {
            return Ok(status);
        }
        if self.drained {
            return Err(io::Error::other(
                "native helper drained; actual exit status unavailable after guardian loss",
            ));
        }
        if self.socket.is_none() {
            if self.proven().map_err(io::Error::other)? {
                self.drained = true;
                return Err(io::Error::other(
                    "native helper drained; actual exit status unavailable after guardian loss",
                ));
            }
            return Err(io::Error::other(
                "shared guardian ownership remains uncertain",
            ));
        }
        match self.request(Request::Drain {
            millis: timeout.as_millis().clamp(1, 2000) as u64,
        })? {
            Reply::Drained { raw } => {
                self.child.wait()?;
                self.socket = None;
                let status = ExitStatus::from_raw(raw);
                self.finished = Some(status);
                self.drained = true;
                Ok(status)
            }
            _ => Err(io::Error::other("shared guardian is still draining")),
        }
    }
    fn drain_once(&mut self, timeout: Duration) -> io::Result<()> {
        if self.drained {
            return Ok(());
        }
        let error = match self.terminate_and_wait(timeout) {
            Ok(_) => return Ok(()),
            Err(_) if self.drained => return Ok(()),
            Err(error) => error,
        };
        if self.socket.is_none() && self.proven().map_err(io::Error::other)? {
            self.drained = true;
            return Ok(());
        }
        Err(error)
    }
}
impl Drop for SharedOwner {
    fn drop(&mut self) {
        if !self.drained && self.finished.is_none() {
            self.finish_uncertain();
        }
    }
}

pub(crate) fn serve() -> Result<()> {
    let (context, pin) = capture()?;
    let mut parent = socket()?;
    let [input, output, error, transferred_pin] = native::receive_files(&parent)?;
    let init: SharedInit =
        read_frame(&mut parent)?.context("shared guardian initialization missing")?;
    ensure!(
        init.version == 2 && init.context == context,
        "shared guardian initialization changed native ownership"
    );
    let main = std::env::current_exe()?.canonicalize()?;
    ensure!(
        crate::runtime_pair::verify_helper(&main)? == init.program,
        "shared guardian helper payload changed"
    );
    let actual = pin.metadata()?;
    let transferred = transferred_pin.metadata()?;
    ensure!(
        (actual.dev(), actual.ino()) == (transferred.dev(), transferred.ino()),
        "shared guardian transferred another pin"
    );
    drop(transferred_pin);
    let mut registry = ServiceRegistry::open(&context.runtime, context.namespace)?;
    let record = registry
        .shared_helper_drain(context.owner)?
        .context("shared guardian reservation missing")?;
    ensure!(
        registry.recorded_service_instance()? == Some(context.owner.service),
        "shared guardian service was replaced"
    );
    let SharedHelperContainment::Unix {
        guardian_pid,
        group,
        ..
    } = record.containment
    else {
        anyhow::bail!("shared guardian platform changed");
    };
    ensure!(
        guardian_pid == Some(std::process::id())
            && group.is_none()
            && record.phase == SharedHelperPhase::Admitting,
        "shared guardian has no exact parent binding"
    );
    let (mut bootstrap, peer) = UnixStream::pair()?;
    let mut command = Command::new(&init.program);
    command
        .arg("pool-guarded")
        .stdin(Stdio::from(std::os::fd::OwnedFd::from(peer.try_clone()?)))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    install_environment(&mut command, &context, pin)?;
    let child = GroupChild::spawn(&mut command);
    drop(command);
    drop(peer);
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            crate::embedding_pool_process::shared::complete(&context.runtime, context.owner)?;
            return Err(error.into());
        }
    };
    let result = (|| -> Result<SharedHelperDrain> {
        let mut expected = record.clone();
        let SharedHelperContainment::Unix { group, .. } = &mut expected.containment else {
            unreachable!()
        };
        *group = Some(child.id());
        expected.phase = SharedHelperPhase::Bound;
        let result = registry.bind_shared_helper(&record, child.id());
        let bound =
            crate::embedding_pool_process::shared::known_transition(&registry, result, &expected)?;
        native::send_files(&bootstrap, &[&input, &output, &error, pin])?;
        write_frame(&mut bootstrap, &context)?;
        Ok(bound)
    })();
    // Retain this SAME private peer for the guardian's whole native lifetime.
    // The helper's existing timer observes its loss; no extra actor is needed.
    drop((input, output, error));
    let _bound = match result {
        Ok(record) => record,
        Err(error) => {
            child.drain();
            crate::embedding_pool_process::shared::complete(&context.runtime, context.owner)?;
            return Err(error);
        }
    };
    if write_frame(&mut parent, &Reply::Ready { group: child.id() }).is_err() {
        child.drain();
        crate::embedding_pool_process::shared::complete(&context.runtime, context.owner)?;
        return Ok(());
    }
    loop {
        let reply = match read_frame::<_, Request>(&mut parent) {
            Ok(Some(Request::Peek)) => child.try_wait().map(|status| Reply::Status {
                raw: status.map(ExitStatusExt::into_raw),
            }),
            Ok(Some(Request::Kill)) => child.kill().map(|()| Reply::Killed),
            Ok(Some(Request::Drain { millis })) => {
                match child.terminate_and_wait(Duration::from_millis(millis.clamp(1, 2000))) {
                    Ok(status) => {
                        crate::embedding_pool_process::shared::complete(
                            &context.runtime,
                            context.owner,
                        )?;
                        let _ = write_frame(
                            &mut parent,
                            &Reply::Drained {
                                raw: status.into_raw(),
                            },
                        );
                        return Ok(());
                    }
                    Err(_) => Ok(Reply::Failed),
                }
            }
            _ => {
                child.drain();
                crate::embedding_pool_process::shared::complete(&context.runtime, context.owner)?;
                return Ok(());
            }
        }
        .unwrap_or(Reply::Failed);
        if write_frame(&mut parent, &reply).is_err() {
            child.drain();
            crate::embedding_pool_process::shared::complete(&context.runtime, context.owner)?;
            return Ok(());
        }
    }
}

pub(crate) fn accept_bootstrap() -> Result<GuardedBootstrap> {
    let (context, pin) = capture()?;
    let mut bootstrap = socket()?;
    let [input, output, error, transferred_pin] = native::receive_files(&bootstrap)?;
    let permit: GuardPermit =
        read_frame(&mut bootstrap)?.context("shared helper startup permit missing")?;
    ensure!(
        permit == context,
        "shared helper startup permit changed native ownership"
    );
    let record = crate::embedding_pool_bootstrap::validate_permit(&permit)?;
    let SharedHelperContainment::Unix { group, .. } = record.containment else {
        anyhow::bail!("shared helper platform changed");
    };
    // SAFETY: scalar native identity queries with no pointer arguments.
    ensure!(
        group == Some(std::process::id())
            && unsafe { libc::getpgrp() } == std::process::id() as i32,
        "shared helper is outside its exact native group"
    );
    let actual = pin.metadata()?;
    let other = transferred_pin.metadata()?;
    ensure!(
        (actual.dev(), actual.ino()) == (other.dev(), other.ino()),
        "shared helper transferred another pin"
    );
    for (file, target) in [(&input, 0), (&output, 1), (&error, 2)] {
        // SAFETY: these retained private I/O descriptors were transferred by the
        // exact guardian; standard I/O is replaced before any async threads exist.
        ensure!(
            unsafe { libc::dup2(file.as_raw_fd(), target) } >= 0,
            io::Error::last_os_error()
        );
    }
    GuardedBootstrap::from_guardian(
        permit,
        bootstrap,
        group.context("guarded helper group missing")?,
    )
}

pub(crate) fn recover(record: &SharedHelperDrain, root: &Path) -> Result<bool> {
    let SharedHelperContainment::Unix {
        group,
        pin_device,
        pin_inode,
        ..
    } = record.containment
    else {
        anyhow::bail!("shared helper platform changed");
    };
    let current = native::boot()?;
    if current.changed_from(&record.boot) {
        return Ok(true);
    }
    ensure!(
        current.process_scope == record.boot.process_scope,
        "shared helper native process scope changed"
    );
    if !SharedHelperLease::released(root, record.owner, pin_device, pin_inode)? {
        return Ok(false);
    }
    match group {
        Some(group) => native::absent(group),
        None => Ok(record.phase != SharedHelperPhase::Bound),
    }
}
