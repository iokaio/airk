// SPDX-License-Identifier: Apache-2.0
//! Detached process start (split U84): the primitives the service's on-demand
//! start uses, shared with `airkctl service start`. Moved from `airk-cli`
//! (`native_process.rs`, `runtime_install.rs`) by
//! `scripts/dev/split_u84c_detached_spawn.py`.
use std::io;

pub fn without_error_dialogs<T>(action: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Diagnostics::Debug::{
            GetThreadErrorMode, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX,
            SEM_NOOPENFILEERRORBOX, SetThreadErrorMode,
        };
        struct Restore(u32);
        impl Drop for Restore {
            fn drop(&mut self) {
                // SAFETY: restores this thread's own scalar error mode, with no output pointer.
                unsafe {
                    SetThreadErrorMode(self.0, std::ptr::null_mut());
                }
            }
        }
        // SAFETY: GetThreadErrorMode takes no pointers and observes only this thread.
        let original = unsafe { GetThreadErrorMode() };
        // SAFETY: scalar mode and no returned pointer; all prior flags are preserved.
        if unsafe {
            SetThreadErrorMode(
                original | SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let _restore = Restore(original);
        action()
    }
    #[cfg(not(windows))]
    action()
}

#[cfg(windows)]
pub struct DetachedChild {
    handle: std::os::windows::io::OwnedHandle,
    pid: u32,
}

#[cfg(windows)]
impl DetachedChild {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        use std::os::windows::{io::AsRawHandle, process::ExitStatusExt};
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
        // SAFETY: valid owned process handle; zero timeout never blocks.
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) } {
            0 => {
                let mut code = 0;
                // SAFETY: writable scalar output and the same live process handle.
                if unsafe { GetExitCodeProcess(self.handle.as_raw_handle(), &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(std::process::ExitStatus::from_raw(code)))
            }
            258 => Ok(None),
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub fn kill(&mut self) -> io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        // SAFETY: owned handle is for this exact launched process, never a reused PID.
        if unsafe {
            windows_sys::Win32::System::Threading::TerminateProcess(self.handle.as_raw_handle(), 1)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

pub fn windows_argument(value: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        if ch == '"' {
            out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            out.extend(std::iter::repeat_n('\\', slashes));
        }
        slashes = 0;
        out.push(ch);
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}
/// A detached Windows service inherits no handles at all, including caller pipes,
/// editor handles and installer locks. It negotiates control over private IPC later.
#[cfg(windows)]
pub fn spawn_detached(
    executable: &std::path::Path,
    args: &[std::ffi::OsString],
) -> io::Result<DetachedChild> {
    use std::os::windows::{
        ffi::OsStrExt,
        io::{FromRawHandle, OwnedHandle},
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CreateProcessW, PROCESS_INFORMATION,
        STARTUPINFOW,
    };
    let application: Vec<u16> = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut line = windows_argument(&executable.as_os_str().to_string_lossy());
    for arg in args {
        line.push(' ');
        line.push_str(&windows_argument(&arg.to_string_lossy()));
    }
    let mut line: Vec<u16> = line.encode_utf16().chain(Some(0)).collect();
    // SAFETY: zero initialization gives valid defaults; cb is set before the call.
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of_val(&startup) as u32;
    // SAFETY: the native function fully initializes this output on success.
    let mut information: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    without_error_dialogs(|| {
        // SAFETY: all buffers remain live, paths are terminated, environment/cwd are
        // inherited by OS convention, and bInheritHandles=FALSE forbids handle leakage.
        if unsafe {
            CreateProcessW(
                application.as_ptr(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP,
                std::ptr::null(),
                std::ptr::null(),
                &startup,
                &mut information,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful creation transfers these two distinct owned handles.
        let process = unsafe { OwnedHandle::from_raw_handle(information.hProcess) };
        // SAFETY: hThread is the other newly created native handle.
        let thread = unsafe { OwnedHandle::from_raw_handle(information.hThread) };
        drop(thread);
        Ok(DetachedChild {
            handle: process,
            pid: information.dwProcessId,
        })
    })
}

/// Start the child in a new session (`setsid` before exec), as the service's
/// on-demand start does on Unix.
#[cfg(unix)]
pub fn new_session(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: setsid has no pointer arguments and runs before exec in the child.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}

/// Wait up to `timeout` for the process `pid` to exit; `true` once it has.
/// The process need not be a child (split step 3b.1e: `airkctl` observes the
/// service it asked to stop). On Unix a child of this process is reaped.
pub fn wait_for_exit(pid: u32, timeout: std::time::Duration) -> io::Result<bool> {
    #[cfg(windows)]
    {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        const ERROR_INVALID_PARAMETER: i32 = 87;
        const WAIT_OBJECT_0: u32 = 0;
        const WAIT_TIMEOUT: u32 = 258;
        // SAFETY: scalar rights and process id; a null result reports failure.
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if raw.is_null() {
            let error = io::Error::last_os_error();
            // No process has this id any more.
            return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER) {
                Ok(true)
            } else {
                Err(error)
            };
        }
        // SAFETY: a successful OpenProcess returns one owned handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
        // SAFETY: the handle is live for the call.
        match unsafe { WaitForSingleObject(handle.as_raw_handle(), millis) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }
    #[cfg(unix)]
    {
        let pid = libc::pid_t::try_from(pid)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process id out of range"))?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let mut status = 0;
            // SAFETY: a writable status and a scalar id; ECHILD means not our child.
            if unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) } == pid {
                return Ok(true);
            }
            // SAFETY: signal 0 checks existence without delivering a signal.
            if unsafe { libc::kill(pid, 0) } != 0
                && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                return Ok(true);
            }
            if std::time::Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
mod wait_tests {
    use super::*;

    /// A long-running process with no shell wrapper and no inherited output,
    /// so killing it leaves nothing behind.
    fn long_child() -> std::process::Child {
        #[cfg(windows)]
        let mut command = {
            let mut command = std::process::Command::new("ping");
            command.args(["-n", "30", "127.0.0.1"]);
            command
        };
        #[cfg(unix)]
        let mut command = {
            let mut command = std::process::Command::new("sleep");
            command.arg("30");
            command
        };
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    #[cfg_attr(
        unix,
        expect(
            clippy::zombie_processes,
            reason = "wait_for_exit reaps this child through waitpid; that is what the test proves"
        )
    )]
    fn a_running_process_is_not_reported_exited_and_an_exited_one_is() {
        let mut child = long_child();
        let pid = child.id();
        assert!(!wait_for_exit(pid, std::time::Duration::from_millis(100)).unwrap());
        child.kill().unwrap();
        #[cfg(windows)]
        child.wait().unwrap();
        assert!(wait_for_exit(pid, std::time::Duration::from_secs(10)).unwrap());
    }
}
