// SPDX-License-Identifier: Apache-2.0
//! Private Unix process groups for trusted providers that remain in their group.
//! Peeking never reaps the leader: its PID cannot be reused before group drain.
use std::{
    io,
    os::unix::process::{CommandExt, ExitStatusExt},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus},
    time::{Duration, Instant},
};

pub struct GroupChild {
    child: Child,
    reaped: Option<ExitStatus>,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}
impl GroupChild {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native process-group observation unavailable",
        ));
        // An embedding host that auto-reaps children cannot reserve a PID for
        // later group signaling. Refuse before starting any provider code.
        let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
        // SAFETY: read-only native signal-disposition query and sized output.
        if unsafe { libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut action) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if action.sa_sigaction == libc::SIG_IGN || action.sa_flags & libc::SA_NOCLDWAIT != 0 {
            return Err(io::Error::other(
                "automatic child reaping prevents owned process-group drain",
            ));
        }
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
            stdin: child.stdin.take(),
            stdout: child.stdout.take(),
            stderr: child.stderr.take(),
            child,
            reaped: None,
        })
    }
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.reaped {
            return Ok(Some(status));
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
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| io::Error::other("group drain deadline overflow"))?;
        self.kill()?;
        loop {
            if peek(self.id())?.is_some() && !has_live_members(self.id(), deadline)? {
                // The leader is already waitable and no group member can still
                // hold working memory or pipes. Only now release its PID.
                let status = self.child.wait()?;
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
