// SPDX-License-Identifier: Apache-2.0
//! Native helper containment also owns startup-error and failed-helper cleanup.
use super::*;

#[cfg(unix)]
type NativeChild = airk_process::GroupChild;
#[cfg(windows)]
type NativeChild = std::process::Child;
pub(super) type Input = std::fs::File;

enum Helper {
    Legacy {
        child: NativeChild,
        #[cfg(windows)]
        containment: airk_host::child_job::ChildJob,
    },
    #[cfg(unix)]
    Guarded(airk_process::GroupChild),
    #[cfg(windows)]
    Guarded(super::shared::ManagedWindows),
}

pub(super) struct OwnedHelper {
    helper: Helper,
    pub stdin: Option<std::fs::File>,
    pub stdout: Option<std::fs::File>,
}
impl OwnedHelper {
    #[cfg(windows)]
    pub(super) fn member_pids(&self) -> Result<Vec<u32>> {
        match &self.helper {
            Helper::Legacy { containment, .. } => containment.member_pids(),
            Helper::Guarded(owner) => owner.member_pids(),
        }
    }
    pub(super) fn spawn(command: &mut Command) -> Result<Self> {
        #[cfg(unix)]
        {
            let mut child =
                airk_process::GroupChild::spawn_provider(command, None).map_err(spawn_failure)?;
            let stdin = child.stdin.take();
            let stdout = child.stdout.take();
            Ok(Self {
                helper: Helper::Legacy { child },
                stdin,
                stdout,
            })
        }
        #[cfg(windows)]
        {
            let mut child = airk_host::native_process::spawn(command).map_err(spawn_failure)?;
            let containment = airk_host::native_process::attach_owned(
                &mut child,
                airk_host::child_job::ChildJob::attach,
            )
            .context(FailureCode("helper_containment_failed".into()))?;
            let stdin = child
                .stdin
                .take()
                .map(|file| std::fs::File::from(std::os::windows::io::OwnedHandle::from(file)));
            let stdout = child
                .stdout
                .take()
                .map(|file| std::fs::File::from(std::os::windows::io::OwnedHandle::from(file)));
            Ok(Self {
                helper: Helper::Legacy { child, containment },
                stdin,
                stdout,
            })
        }
    }
    pub(super) fn guarded(
        root: &Path,
        service: Incarnation,
        helper: Incarnation,
        main: &Path,
        program: &Path,
    ) -> Result<Self> {
        #[cfg(unix)]
        {
            let mut child =
                crate::unix_guardian::shared::spawn(root, service, helper, main, program)?;
            let stdin = child.stdin.take();
            let stdout = child.stdout.take();
            Ok(Self {
                helper: Helper::Guarded(child),
                stdin,
                stdout,
            })
        }
        #[cfg(windows)]
        {
            let _ = main;
            let mut owner = super::shared::ManagedWindows::spawn(root, service, helper, program)?;
            let child = owner.child.as_mut().expect("guarded spawn owns a child");
            let stdin = child.stdin.take();
            let stdout = child.stdout.take();
            Ok(Self {
                helper: Helper::Guarded(owner),
                stdin,
                stdout,
            })
        }
    }
    pub(super) fn id(&self) -> u32 {
        match &self.helper {
            Helper::Legacy { child, .. } => child.id(),
            #[cfg(unix)]
            Helper::Guarded(child) => child.id(),
            #[cfg(windows)]
            Helper::Guarded(owner) => owner.child.as_ref().expect("guarded child").id(),
        }
    }
    pub(super) fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        match &mut self.helper {
            Helper::Legacy { child, .. } => child.try_wait(),
            #[cfg(unix)]
            Helper::Guarded(child) => child.try_wait(),
            #[cfg(windows)]
            Helper::Guarded(owner) => owner.child.as_mut().expect("guarded child").try_wait(),
        }
    }
    #[cfg(test)]
    pub(super) fn wait(&mut self) -> std::io::Result<ExitStatus> {
        match &mut self.helper {
            Helper::Legacy { child, .. } => child.wait(),
            #[cfg(unix)]
            Helper::Guarded(child) => child.wait(),
            #[cfg(windows)]
            Helper::Guarded(owner) => owner.child.as_mut().expect("guarded child").wait(),
        }
    }
    pub(super) fn drain(&mut self, timeout: Duration) -> Result<()> {
        match &mut self.helper {
            Helper::Legacy { child, .. } => {
                #[cfg(unix)]
                child.drain_once(timeout)?;
                #[cfg(windows)]
                {
                    let _ = child;
                }
            }
            #[cfg(unix)]
            Helper::Guarded(child) => {
                child.drain_once(timeout)?;
            }
            #[cfg(windows)]
            Helper::Guarded(owner) => owner.drain(timeout)?,
        }
        #[cfg(windows)]
        if let Helper::Legacy { child, containment } = &mut self.helper {
            containment.terminate_and_wait(timeout)?;
            child.wait()?;
        }
        Ok(())
    }
}
impl Drop for OwnedHelper {
    fn drop(&mut self) {
        while self.drain(Duration::from_secs(2)).is_err() {
            // Native uncertainty retains the exact group/job ownership.
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
