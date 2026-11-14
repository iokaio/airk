// SPDX-License-Identifier: Apache-2.0
//! Shared R10 native setup launcher; an uncertain wait never kills admitted work.
use super::*;

pub(crate) struct RecoveryChild {
    #[cfg(windows)]
    child: airk_process::detached::DetachedChild,
    #[cfg(unix)]
    child: std::process::Child,
}
impl RecoveryChild {
    pub(crate) fn id(&self) -> u32 {
        self.child.id()
    }
    pub(crate) fn wait(&mut self, timeout: std::time::Duration) -> Result<bool, ErrorLine> {
        #[cfg(windows)]
        let observed = self
            .child
            .wait_timeout(timeout)
            .map(|status| status.is_some());
        #[cfg(unix)]
        let observed = airk_process::detached::wait_for_exit(self.child.id(), timeout);
        observed.map_err(|error| failed(format!("native setup observation failed: {error}")))
    }
}

pub(crate) fn launch(airk: &Path, operation: &Path) -> Result<RecoveryChild, ErrorLine> {
    let arguments: Vec<std::ffi::OsString> = vec![
        "__airk_role".into(),
        "setup".into(),
        "--operation".into(),
        operation.into(),
    ];
    #[cfg(windows)]
    let child = airk_process::detached::spawn_detached(airk, &arguments);
    #[cfg(unix)]
    let child = {
        let mut command = std::process::Command::new(airk);
        command
            .args(&arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        airk_process::detached::new_session(&mut command);
        airk_process::child_group::require_waitable_children()
            .and_then(|()| airk_process::detached::without_error_dialogs(|| command.spawn()))
    };
    child
        .map(|child| RecoveryChild { child })
        .map_err(|error| failed(format!("{}: {error}", airk.display())))
}
