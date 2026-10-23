// SPDX-License-Identifier: Apache-2.0
//! Native denial stays an error; background startup must never await an OS error dialog.
use std::{
    io,
    process::{Child, Command},
};

pub(crate) fn spawn(command: &mut Command) -> io::Result<Child> {
    without_error_dialogs(|| command.spawn())
}

/// A short-lived native command is never left running by a pipe, parse or wait
/// failure. Windows descendants are drained before the enclosing owner releases.
pub(crate) struct ContainedChild {
    child: Child,
    containment: crate::child_job::ChildJob,
}
impl ContainedChild {
    pub fn spawn(command: &mut Command) -> anyhow::Result<Self> {
        let mut child = spawn(command)?;
        let containment = crate::child_job::ChildJob::attach(&mut child)?;
        Ok(Self { child, containment })
    }
}
impl std::ops::Deref for ContainedChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}
impl std::ops::DerefMut for ContainedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}
impl Drop for ContainedChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        self.containment.drain();
        let _ = self.child.wait();
    }
}

pub(crate) fn without_error_dialogs<T>(action: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
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
pub(crate) struct DetachedChild {
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

/// A detached Windows service inherits no handles at all, including caller pipes,
/// editor handles and installer locks. It negotiates control over private IPC later.
#[cfg(windows)]
pub(crate) fn spawn_detached(
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
    let mut line =
        crate::service_manager::windows_argument(&executable.as_os_str().to_string_lossy());
    for arg in args {
        line.push(' ');
        line.push_str(&crate::service_manager::windows_argument(
            &arg.to_string_lossy(),
        ));
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        io::Read,
        process::Stdio,
        time::{Duration, Instant},
    };

    #[test]
    fn detached_child_fixture() {
        if std::env::var_os("AIRK_DETACHED_PIPE_FIXTURE").is_some() {
            std::thread::sleep(Duration::from_secs(8));
        }
    }

    #[test]
    fn detached_launcher_fixture() {
        if std::env::var_os("AIRK_DETACHED_PIPE_FIXTURE").is_some() {
            let _child = spawn_detached(
                &std::env::current_exe().unwrap(),
                &[
                    "--exact".into(),
                    "native_process::tests::detached_child_fixture".into(),
                    "--nocapture".into(),
                ],
            )
            .unwrap();
        }
    }

    #[test]
    fn detached_service_does_not_keep_launcher_capture_pipes_open() -> anyhow::Result<()> {
        let mut launcher = Command::new(std::env::current_exe()?);
        launcher
            .args([
                "--exact",
                "native_process::tests::detached_launcher_fixture",
                "--nocapture",
            ])
            .env("AIRK_DETACHED_PIPE_FIXTURE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = spawn(&mut launcher)?;
        let mut output = child.stdout.take().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = output.read_to_end(&mut bytes);
            let _ = sender.send(bytes);
        });
        let until = Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(status) = child.try_wait()? {
                anyhow::ensure!(status.success(), "fixture launcher failed");
                break;
            }
            if Instant::now() >= until {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("fixture launcher did not exit");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        anyhow::ensure!(
            receiver
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .is_ok(),
            "detached descendant retained caller stdout after its launcher exited"
        );
        reader.join().unwrap();
        Ok(())
    }
}
