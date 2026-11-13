// SPDX-License-Identifier: Apache-2.0
//! Direct per-user Windows/Linux manager entry; never forwards or launches a child.
use super::*;

pub(super) async fn serve(root: &Path) -> Result<()> {
    ensure!(
        cfg!(any(windows, target_os = "linux")),
        "direct task/systemd entry is unavailable on this platform"
    );
    #[cfg(windows)]
    detach_task_console();
    managed::require_unmanaged(root)?;
    // Startup may be requested while the installer holds its mutation lease.
    // Authenticate immutable captured metadata without reacquiring that lease;
    // the service loop acquires its independent single-instance ownership.
    let (selected, approval) = startup_selected(root)?;
    let current = std::env::current_exe()?.canonicalize()?;
    ensure!(
        current == selected.canonicalize()?,
        "direct manager must execute the selected immutable main in place"
    );
    let manager = crate::service_manager::ServiceManager::for_runtime(root)?;
    manager.require_direct_entry(&current, &approval.manifest_sha256)?;
    let control = layout::control(root)?;
    service_runtime::serve(&control).await
}

/// Task Scheduler gives a console-subsystem image started at logon its own
/// visible console window, and closing that window would end the service.
/// Leave only a console that no other process shares: one a person or parent
/// process opened (two or more attached processes) is retained.
#[cfg_attr(not(windows), allow(dead_code))]
fn detach_from_console(attached_processes: u32) -> bool {
    attached_processes == 1
}

#[cfg(windows)]
fn detach_task_console() {
    use windows_sys::Win32::System::Console::{
        FreeConsole, GetConsoleProcessList, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };
    let mut processes = [0u32; 2];
    // SAFETY: the buffer is valid for its declared length; a process without a
    // console receives zero.
    let attached = unsafe { GetConsoleProcessList(processes.as_mut_ptr(), processes.len() as u32) };
    if !detach_from_console(attached) {
        return;
    }
    // SAFETY: no pointer arguments. Clearing the standard handles afterward makes
    // later standard-stream writes no-ops instead of using closed console handles.
    unsafe {
        FreeConsole();
        for handle in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            SetStdHandle(handle, std::ptr::null_mut());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::detach_from_console;

    #[test]
    fn only_a_console_owned_by_the_task_alone_is_released() {
        assert!(!detach_from_console(0), "no console attached");
        assert!(
            detach_from_console(1),
            "the logon task created this console"
        );
        assert!(
            !detach_from_console(2),
            "a person's or parent's console is retained"
        );
        assert!(!detach_from_console(5));
    }
}
