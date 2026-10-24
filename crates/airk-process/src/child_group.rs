// SPDX-License-Identifier: Apache-2.0
//! Private Unix process groups for trusted providers that remain in their group.
//! Peeking never reaps the leader: its PID cannot be reused before group drain.
use std::{
    fs::File,
    io,
    os::{
        fd::{FromRawFd, IntoRawFd},
        unix::process::{CommandExt, ExitStatusExt},
    },
    process::{Child, Command, ExitStatus},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

/// Native adapters can keep the actual parent in a surviving guardian. The
/// process crate owns no service state, registry policy or serialized payloads.
pub trait GroupOwner: Send {
    fn id(&self) -> u32;
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>>;
    fn kill(&mut self) -> io::Result<()>;
    fn terminate_and_wait(&mut self, timeout: Duration) -> io::Result<ExitStatus>;
}
#[derive(Clone, Copy)]
pub struct ProviderLimits {
    pub memory_bytes: usize,
    pub cpu_seconds: u64,
}
impl ProviderLimits {
    pub fn configure(self, command: &mut Command) {
        // SAFETY: only async-signal-safe resource calls run before exec.
        unsafe {
            command.pre_exec(move || {
                #[cfg(not(target_os = "macos"))]
                let resources = [
                    (libc::RLIMIT_AS, self.memory_bytes as libc::rlim_t),
                    (libc::RLIMIT_CPU, self.cpu_seconds as libc::rlim_t),
                ];
                #[cfg(target_os = "macos")]
                let resources = [(libc::RLIMIT_CPU, self.cpu_seconds as libc::rlim_t)];
                for (resource, value) in resources {
                    let limit = libc::rlimit {
                        rlim_cur: value,
                        rlim_max: value,
                    };
                    if libc::setrlimit(resource, &limit) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }
}
pub trait ProviderSpawner: Send + Sync {
    /// Called only by the explicit provider adapter, with piped standard I/O.
    fn spawn(&self, command: &Command, limits: Option<ProviderLimits>) -> io::Result<GroupChild>;
}
static PROVIDER_SPAWNER: OnceLock<Arc<dyn ProviderSpawner>> = OnceLock::new();
pub fn install_provider_spawner(spawner: Arc<dyn ProviderSpawner>) -> io::Result<()> {
    PROVIDER_SPAWNER
        .set(spawner)
        .map_err(|_| io::Error::other("provider guardian already installed"))
}
fn pipe_file(handle: impl IntoRawFd) -> File {
    // SAFETY: transfer exactly this owned pipe descriptor to the File.
    unsafe { File::from_raw_fd(handle.into_raw_fd()) }
}

pub struct GroupChild {
    child: Option<Child>,
    remote: Option<Box<dyn GroupOwner>>,
    reaped: Option<ExitStatus>,
    pub stdin: Option<File>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
}
/// Native supervisors must retain their children until the exact owner waits.
/// A custom handler could reap an unrelated child and invalidate PID ownership.
pub fn require_waitable_children() -> io::Result<()> {
    let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
    // SAFETY: read-only native disposition query with correctly sized output.
    if unsafe { libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut action) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if action.sa_sigaction != libc::SIG_DFL || action.sa_flags & libc::SA_NOCLDWAIT != 0 {
        return Err(io::Error::other(
            "external child reaping prevents native process ownership",
        ));
    }
    Ok(())
}

impl GroupChild {
    pub fn spawn_provider(
        command: &mut Command,
        limits: Option<ProviderLimits>,
    ) -> io::Result<Self> {
        if let Some(spawner) = PROVIDER_SPAWNER.get() {
            return spawner.spawn(command, limits);
        }
        Self::spawn(command)
    }
    pub fn from_owner(owner: Box<dyn GroupOwner>, stdin: File, stdout: File, stderr: File) -> Self {
        Self {
            child: None,
            remote: Some(owner),
            reaped: None,
            stdin: Some(stdin),
            stdout: Some(stdout),
            stderr: Some(stderr),
        }
    }
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native process-group observation unavailable",
        ));
        require_waitable_children()?;
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::{FromRawFd, OwnedFd};
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, libc::getpid(), 0) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            drop(unsafe { OwnedFd::from_raw_fd(fd as i32) });
        }
        command.process_group(0);
        let mut child = command.spawn()?;
        Ok(Self {
            stdin: child.stdin.take().map(pipe_file),
            stdout: child.stdout.take().map(pipe_file),
            stderr: child.stderr.take().map(pipe_file),
            child: Some(child),
            remote: None,
            reaped: None,
        })
    }
    pub fn id(&self) -> u32 {
        self.remote.as_ref().map_or_else(
            || self.child.as_ref().expect("owned child missing").id(),
            |owner| owner.id(),
        )
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.reaped {
            return Ok(Some(status));
        }
        if let Some(owner) = self.remote.as_mut() {
            return owner.try_wait();
        }
        peek(self.id())
    }
    pub fn kill(&mut self) -> io::Result<()> {
        if self.reaped.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "process group already reaped",
            ));
        }
        if let Some(owner) = self.remote.as_mut() {
            return owner.kill();
        }
        // A successful peek (including None) proves this is still our unreaped
        // child. ECHILD is never permission to signal a possibly reused number.
        peek(self.id())?;
        let pid = libc::pid_t::try_from(self.id()).map_err(io::Error::other)?;
        if pid <= 1 || pid == unsafe { libc::getpgrp() } {
            return Err(io::Error::other("refusing a non-private process group"));
        }
        // SAFETY: the private group is named by our unreaped child's reserved PID.
        if unsafe { libc::kill(-pid, libc::SIGKILL) } != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
        }
        Ok(())
    }
    pub fn terminate_and_wait(&mut self, timeout: Duration) -> io::Result<ExitStatus> {
        if let Some(status) = self.reaped {
            return Ok(status);
        }
        if let Some(owner) = self.remote.as_mut() {
            let status = owner.terminate_and_wait(timeout)?;
            self.reaped = Some(status);
            return Ok(status);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| io::Error::other("group drain deadline overflow"))?;
        self.kill()?;
        loop {
            if peek(self.id())?.is_some() && !has_live_members(self.id(), deadline)? {
                // The leader is already waitable and no group member can still
                // hold working memory or pipes. Only now release its PID.
                let status = self.child.as_mut().expect("owned child missing").wait()?;
                self.reaped = Some(status);
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "owned process group has not drained",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.reaped {
            return Ok(status);
        }
        loop {
            if self.try_wait()?.is_some() {
                return self.terminate_and_wait(Duration::from_secs(2));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    pub fn drain(&mut self) {
        while self.terminate_and_wait(Duration::from_secs(2)).is_err() {
            // Unknown native state retains this owner and its capacity. Neither
            // an expired wait nor a successful kill syscall proves process exit.
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
impl Drop for GroupChild {
    fn drop(&mut self) {
        self.drain();
    }
}

fn peek(pid: u32) -> io::Result<Option<ExitStatus>> {
    let mut info = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
    // SAFETY: bounded non-reaping query for this exact owned child, with sized output.
    if unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    let returned = unsafe { info.si_pid() };
    if returned == 0 {
        return Ok(None);
    }
    if returned < 0 || returned as u32 != pid {
        return Err(io::Error::other("foreign child wait observation"));
    }
    let status = unsafe { info.si_status() };
    let raw = match info.si_code {
        libc::CLD_EXITED if (0..=255).contains(&status) => status << 8,
        libc::CLD_KILLED if (1..=127).contains(&status) => status,
        libc::CLD_DUMPED if (1..=127).contains(&status) => status | 0x80,
        _ => return Err(io::Error::other("unsupported child exit observation")),
    };
    Ok(Some(ExitStatus::from_raw(raw)))
}

#[cfg(target_os = "linux")]
fn has_live_members(group: u32, deadline: Instant) -> io::Result<bool> {
    let mut examined = 0;
    for entry in std::fs::read_dir("/proc")? {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "process-group observation deadline",
            ));
        }
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<libc::pid_t>().ok())
        else {
            continue;
        };
        examined += 1;
        if examined > 131_072 {
            return Err(io::Error::other(
                "process-group observation capacity exceeded",
            ));
        }
        // Observe group identity before reading process details. Foreign-user
        // processes with restricted /proc details need no read or permission change.
        let actual = unsafe { libc::getpgid(pid) };
        if actual < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                continue;
            }
            return Err(error);
        }
        if actual as u32 != group {
            continue;
        }
        // A zombie state can precede the kernel's final exit notification.
        // Poll a native lifetime handle rather than equating /proc state with
        // actual termination. ESRCH means that this process has been released.
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        if fd < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                continue;
            }
            return Err(error);
        }
        let handle = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        let mut observation = libc::pollfd {
            fd: handle.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut observation, 1, 0) } < 0 {
            return Err(io::Error::last_os_error());
        }
        if observation.revents & libc::POLLIN == 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
fn has_live_members(group: u32, deadline: Instant) -> io::Result<bool> {
    unsafe extern "C" {
        fn proc_listpgrppids(
            group: libc::pid_t,
            buffer: *mut libc::c_void,
            bytes: libc::c_int,
        ) -> libc::c_int;
    }
    let mut pids = [0 as libc::pid_t; 1025];
    // libproc returns a count here, unlike the lower-level byte-count interface.
    unsafe {
        *libc::__error() = 0;
    }
    let count = unsafe {
        proc_listpgrppids(
            group as libc::pid_t,
            pids.as_mut_ptr().cast(),
            std::mem::size_of_val(&pids) as libc::c_int,
        )
    };
    if count < 0 || count == 0 && unsafe { *libc::__error() } != 0 {
        return Err(io::Error::last_os_error());
    }
    if count as usize >= pids.len() {
        return Err(io::Error::other(
            "process-group observation capacity exceeded",
        ));
    }
    for pid in pids.into_iter().take(count as usize).filter(|pid| *pid > 0) {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "process-group observation deadline",
            ));
        }
        let mut info = unsafe { std::mem::zeroed::<libc::proc_bsdinfo>() };
        let bytes = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                // Include unreaped zombies. Without this flag, a process in
                // exit teardown can disappear from the live-only lookup before
                // its terminal state is available for inspection.
                1,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                std::mem::size_of_val(&info) as libc::c_int,
            )
        };
        if bytes as usize != std::mem::size_of_val(&info) {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                continue;
            }
            return Err(io::Error::other(
                "native process-state observation unavailable",
            ));
        }
        if info.pbi_pid == pid as u32 && info.pbi_pgid == group && info.pbi_status != libc::SZOMB {
            return Ok(true);
        }
    }
    Ok(false)
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn has_live_members(_: u32, _: Instant) -> io::Result<bool> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native process-group observation unavailable",
    ))
}
