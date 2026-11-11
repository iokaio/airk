// SPDX-License-Identifier: Apache-2.0
//! Native denial stays an error; background startup must never await an OS error dialog.
use std::{
    io,
    process::{Child, Command},
};

/// Contain this exact newly spawned child before Init, or terminate and reap it.
/// Production containment uses it only on Windows; tests use it everywhere.
#[cfg(any(windows, test))]
pub(crate) fn attach_owned(
    child: &mut Child,
    attach: impl FnOnce(&mut Child) -> anyhow::Result<crate::child_job::ChildJob>,
) -> anyhow::Result<crate::child_job::ChildJob> {
    use anyhow::Context;
    match attach(child) {
        Ok(containment) => Ok(containment),
        Err(error) => {
            let _ = child.kill();
            child.wait().context("failed to reap uncontained child")?;
            Err(error)
        }
    }
}

/// Replace this process with `command` (Unix) or run it with inherited stdio inside a
/// kill-on-close job and exit with its code (Windows). Callers must not write to stdout first.
pub(crate) fn forward_stdio(mut command: Command) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec().into())
    }
    #[cfg(not(unix))]
    {
        let mut child = ContainedChild::spawn(&mut command)?;
        let status = child.wait()?;
        drop(child);
        std::process::exit(status.code().unwrap_or(1));
    }
}

pub(crate) fn spawn(command: &mut Command) -> io::Result<Child> {
    #[cfg(unix)]
    airk_process::child_group::require_waitable_children()?;
    without_error_dialogs(|| command.spawn())
}

/// A short-lived native command is never left running by a pipe, parse or wait
/// failure. Descendants are drained before the enclosing owner releases.
#[cfg(windows)]
type ManagedChild = Child;
#[cfg(unix)]
type ManagedChild = airk_process::GroupChild;

pub(crate) struct ContainedChild {
    child: ManagedChild,
    #[cfg(windows)]
    containment: crate::child_job::ChildJob,
}
impl ContainedChild {
    pub fn spawn(command: &mut Command) -> anyhow::Result<Self> {
        #[cfg(windows)]
        {
            Self::attach(spawn(command)?, crate::child_job::ChildJob::attach)
        }
        #[cfg(unix)]
        Ok(Self {
            child: airk_process::GroupChild::spawn(command)?,
        })
    }

    #[cfg(windows)]
    fn attach(
        mut child: Child,
        attach: impl FnOnce(&mut Child) -> anyhow::Result<crate::child_job::ChildJob>,
    ) -> anyhow::Result<Self> {
        let containment = attach_owned(&mut child, attach)?;
        Ok(Self { child, containment })
    }
}
impl std::ops::Deref for ContainedChild {
    type Target = ManagedChild;
    fn deref(&self) -> &ManagedChild {
        &self.child
    }
}
impl std::ops::DerefMut for ContainedChild {
    fn deref_mut(&mut self) -> &mut ManagedChild {
        &mut self.child
    }
}
impl Drop for ContainedChild {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            let _ = self.child.kill();
            self.containment.drain();
            let _ = self.child.wait();
        }
        #[cfg(unix)]
        self.child.drain();
    }
}

#[cfg(windows)]
pub(crate) use airk_process::detached::spawn_detached;
/// Moved to `airk-process` (U84); re-exported for existing callers.
pub(crate) use airk_process::detached::{windows_argument, without_error_dialogs};

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use anyhow::Context;
    use std::{
        io::Read,
        process::Stdio,
        time::{Duration, Instant},
    };

    #[test]
    fn containment_admission_failure_terminates_and_reaps_the_spawned_child() -> anyhow::Result<()>
    {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::{
            Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, WAIT_OBJECT_0},
            System::Threading::{GetCurrentProcess, WaitForSingleObject},
        };
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "native_process::tests::detached_child_fixture",
                "--nocapture",
            ])
            .env("AIRK_DETACHED_PIPE_FIXTURE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut witness: Option<OwnedHandle> = None;
        let result = ContainedChild::attach(spawn(&mut command)?, |child| {
            let mut duplicate = std::ptr::null_mut();
            // SAFETY: duplicate this exact owned child's live process handle,
            // avoiding PID reuse; the new handle is adopted once below.
            let copied = unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    child.as_raw_handle(),
                    GetCurrentProcess(),
                    &mut duplicate,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            };
            anyhow::ensure!(
                copied != 0,
                "duplicate fixture process handle: {}",
                std::io::Error::last_os_error()
            );
            // SAFETY: successful DuplicateHandle returned a new owned handle.
            witness = Some(unsafe { OwnedHandle::from_raw_handle(duplicate) });
            anyhow::bail!("injected containment admission failure")
        });
        assert!(result.is_err());
        let witness = witness.context("fixture process witness missing")?;
        // SAFETY: the witness remains owned and valid through this observation.
        assert_eq!(
            unsafe { WaitForSingleObject(witness.as_raw_handle(), 0) },
            WAIT_OBJECT_0
        );
        Ok(())
    }

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
