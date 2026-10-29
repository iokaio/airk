// SPDX-License-Identifier: Apache-2.0
//! Installer-owned hard stop, independent of the predecessor's control protocol.
use super::*;

pub(super) fn stop(root: &Path, identity: &str) -> Result<()> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        let (main, _) = verify_runtime(root, identity)?;
        let current = std::env::current_exe()?.canonicalize()?;
        ensure!(
            current != main.canonicalize()?
                && (!launcher_path(root).try_exists()?
                    || current != launcher_path(root).canonicalize()?),
            "run the upgrade from the new distribution, outside the preceding runtime"
        );
        let directory = main.parent().context("runtime parent missing")?;
        let mut paths = vec![main.clone(), directory.join(helper_name())];
        // This stable launcher may itself be held by old MCP/Assistance clients.
        // Its saved receipt, rather than the new candidate, proves its identity.
        let launcher = launcher_path(root);
        if launcher.try_exists()? {
            safe_file(&launcher)?;
            let receipt: LauncherReceipt =
                serde_json::from_slice(&read_bounded(&root.join("launcher.json"), 4096)?)?;
            ensure!(
                receipt.schema == 1 && runtime_pair::digest_file(&launcher)? == receipt.sha256,
                "unknown or modified launcher preserved"
            );
            paths.push(launcher);
        }
        for role in [
            crate::process_roles::Role::Service,
            crate::process_roles::Role::Index,
            crate::process_roles::Role::Source,
        ] {
            let path = crate::process_roles::executable(&main, role)?;
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        stop_paths(&paths)?;
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = (root, identity); // Other hosts retain authenticated control/manager drain.
    Ok(())
}

/// The caller must prove every image from the installation's inventory/launcher receipts.
/// Keep OS identities open through termination; filenames and PIDs alone confer no authority.
pub(super) fn stop_paths(paths: &[PathBuf]) -> Result<()> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        let mut owners = Vec::new();
        for path in paths {
            let audit = crate::legacy_processes::audit(path);
            ensure!(
                audit.failure.is_none(),
                "AIRK process audit failed: {:?}",
                audit.failure
            );
            for pid in audit.matching_executable_pids {
                if let Some(owner) = capture(pid, path)? {
                    owners.push(owner);
                }
            }
        }
        for owner in &owners {
            terminate(owner)?;
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        for owner in &owners {
            wait(owner, deadline)?;
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = paths;
    Ok(())
}

#[cfg(windows)]
type Owner = std::os::windows::io::OwnedHandle;

#[cfg(windows)]
fn capture(pid: u32, expected: &Path) -> Result<Option<Owner>> {
    use std::os::windows::{
        ffi::OsStringExt,
        io::{AsRawHandle, FromRawHandle},
    };
    use windows_sys::Win32::System::Threading::*;
    ensure!(
        pid != std::process::id(),
        "installer cannot terminate itself"
    );
    // SAFETY: capture a kernel process object; later signals never reopen its PID.
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if raw.is_null() {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(87) {
            return Ok(None);
        }
        return Err(error.into());
    }
    // SAFETY: successful OpenProcess transfers one owned handle.
    let owner = unsafe { Owner::from_raw_handle(raw) };
    // SAFETY: a zero-time wait on our owned process handle observes exit only.
    if unsafe { WaitForSingleObject(owner.as_raw_handle(), 0) } == 0 {
        return Ok(None);
    }
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    // SAFETY: held handle and bounded writable path buffer.
    // SAFETY: the handle and path buffer remain live through the query.
    if unsafe {
        QueryFullProcessImageNameW(owner.as_raw_handle(), 0, path.as_mut_ptr(), &mut length)
    } == 0
    {
        // SAFETY: distinguish concurrent exit from an unavailable live identity.
        if unsafe { WaitForSingleObject(owner.as_raw_handle(), 0) } == 0 {
            return Ok(None);
        }
        bail!(
            "upgrade process image unavailable: {}",
            std::io::Error::last_os_error()
        );
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(&path[..length as usize]));
    ensure!(
        path.canonicalize()? == expected.canonicalize()?
            && crate::local_ipc::process_user(pid)? == crate::local_ipc::current_user()?,
        "upgrade process ownership changed"
    );
    Ok(Some(owner))
}

#[cfg(windows)]
fn terminate(owner: &Owner) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};
    // SAFETY: the held handle was verified before signaling; no PID reuse.
    if unsafe { WaitForSingleObject(owner.as_raw_handle(), 0) } != 0 {
        ensure!(
            unsafe { TerminateProcess(owner.as_raw_handle(), 1) } != 0
                // A previously terminated parent can concurrently drain this child.
                || unsafe { WaitForSingleObject(owner.as_raw_handle(), 0) } == 0,
            "cannot terminate verified preceding AIRK process: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

#[cfg(windows)]
fn wait(owner: &Owner, deadline: std::time::Instant) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    let millis = deadline
        .saturating_duration_since(std::time::Instant::now())
        .as_millis()
        .min(u32::MAX as u128) as u32;
    // SAFETY: bounded wait on the same verified process object.
    ensure!(
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(
                owner.as_raw_handle(),
                millis,
            )
        } == 0,
        "preceding AIRK process did not exit; activation remains pending"
    );
    Ok(())
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn old_unresponsive_process_is_stopped_by_verified_handle_only() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let executable = directory.path().join(if cfg!(windows) {
            "old-airk.exe"
        } else {
            "old-airk"
        });
        fs::copy(std::env::current_exe()?, &executable)?;
        let mut command = Command::new(&executable);
        command
            .args(["--exact", "service_manager::tests::manager_command_fixture"])
            .env("AIRK_MANAGER_COMMAND_FIXTURE", "blocked-input")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = crate::native_process::ContainedChild::spawn(&mut command)?;
        let pid = child.id();
        assert!(capture(pid, &std::env::current_exe()?).is_err());
        assert!(child.try_wait()?.is_none());
        let owner = capture(pid, &executable)?.context("fixture exited before capture")?;
        let started = std::time::Instant::now();
        terminate(&owner)?;
        wait(&owner, started + Duration::from_secs(5))?;
        assert!(!child.wait()?.success());
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(capture(std::process::id(), &std::env::current_exe()?).is_err());
        Ok(())
    }
}

#[cfg(target_os = "linux")]
type Owner = std::os::fd::OwnedFd;

#[cfg(target_os = "linux")]
fn capture(pid: u32, expected: &Path) -> Result<Option<Owner>> {
    use std::os::{fd::FromRawFd, unix::fs::MetadataExt};
    ensure!(
        pid != std::process::id(),
        "installer cannot terminate itself"
    );
    // SAFETY: pidfd captures one exact process identity, independently of PID reuse.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if fd < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(None);
        }
        return Err(error.into());
    }
    // SAFETY: successful pidfd_open transfers one owned descriptor.
    let owner = unsafe { Owner::from_raw_fd(fd as i32) };
    let process = PathBuf::from(format!("/proc/{pid}"));
    // SAFETY: geteuid observes this process's native user identity.
    ensure!(
        fs::metadata(&process)?.uid() == unsafe { libc::geteuid() }
            && process.join("exe").canonicalize()? == expected.canonicalize()?,
        "upgrade process ownership changed"
    );
    Ok(Some(owner))
}

#[cfg(target_os = "linux")]
fn terminate(owner: &Owner) -> Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: signal the verified process descriptor, never a numeric PID.
    let result = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            owner.as_raw_fd(),
            libc::SIGKILL,
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    };
    if result < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn wait(owner: &Owner, deadline: std::time::Instant) -> Result<()> {
    use std::os::fd::AsRawFd;
    let mut descriptor = libc::pollfd {
        fd: owner.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let millis = deadline
        .saturating_duration_since(std::time::Instant::now())
        .as_millis()
        .min(i32::MAX as u128) as i32;
    // SAFETY: one initialized descriptor and a finite timeout.
    ensure!(
        unsafe { libc::poll(&mut descriptor, 1, millis) } > 0
            && descriptor.revents & libc::POLLIN != 0,
        "preceding AIRK process did not exit; activation remains pending"
    );
    Ok(())
}
