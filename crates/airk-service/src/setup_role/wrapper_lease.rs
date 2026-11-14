// SPDX-License-Identifier: Apache-2.0
//! OS-held exclusion across all outer placement producers; no TTL/PID clearing.
use super::*;
#[cfg(unix)]
const FD_ENV: &str = "AIRK_SETUP_WRAPPER_LEASE_FD";
#[cfg(unix)]
const LOCK: &str = "wrapper.lock";

pub(super) fn name() -> Result<String> {
    #[cfg(windows)]
    {
        Ok(format!(
            "Global\\AIRK.SetupWrapper.{}",
            airk_ipc::endpoint::current_user()?
        ))
    }
    #[cfg(unix)]
    {
        Ok(files::pointer_path()?
            .parent()
            .context("authority parent missing")?
            .join(LOCK)
            .to_string_lossy()
            .into_owned())
    }
}

pub(super) fn windows_handle() -> Result<usize> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsHandle;
        use windows_sys::Win32::System::Threading::MUTEX_MODIFY_STATE;
        airk_ipc::endpoint::with_current_user_job_security(|attributes, user| {
            let handle = airk_process::named_mutex::create(attributes, &name()?)?;
            // The existing policy validates a protected owner-only SE_KERNEL_OBJECT.
            airk_ipc::endpoint::require_job_user(handle.as_handle(), user)?;
            airk_process::client_owner::duplicate_to_launcher(
                handle.as_handle(),
                0x0010_0000 | MUTEX_MODIFY_STATE,
            )
        })
    }
    #[cfg(unix)]
    {
        bail!("wrapper mutex handle is Windows-only")
    }
}

#[cfg(unix)]
fn inherited_file() -> Result<Option<(i32, std::fs::File)>> {
    use std::{ffi::OsStr, os::fd::FromRawFd};
    let Some(fd) = std::env::var(FD_ENV)
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|fd| *fd >= 3 && *fd < 4096)
    else {
        return Ok(None);
    };
    // SAFETY: fcntl atomically duplicates only a valid descriptor. An invalid
    // untrusted environment number never becomes a borrowed/owned Rust handle.
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if copy < 0 {
        return Ok(None);
    }
    // SAFETY: successful fcntl returned one new owned descriptor.
    let file = unsafe { std::fs::File::from_raw_fd(copy) };
    let pointer = files::pointer_path()?;
    let root = match airk_process::directory::OpenedDirectory::open(
        pointer.parent().context("authority parent")?,
    ) {
        Ok(root) => root,
        Err(_) => return Ok(None),
    };
    let expected = match root.open_file(OsStr::new(LOCK)) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    if airk_process::directory::identity(&file)? != airk_process::directory::identity(&expected)? {
        return Ok(None);
    }
    airk_ipc::endpoint::require_private_metadata_handle(&file)?;
    if file.try_lock().is_err() {
        return Ok(None);
    }
    Ok(Some((fd, file)))
}

pub(super) fn inherited() -> Result<bool> {
    #[cfg(unix)]
    {
        Ok(inherited_file()?.is_some())
    }
    #[cfg(windows)]
    {
        Ok(false)
    }
}

pub(super) fn protect_inherited() -> Result<()> {
    #[cfg(unix)]
    if let Some((fd, _file)) = inherited_file()? {
        // Descriptor flags are per-process; the shell/copy producers retain
        // their inherited descriptor, but this native child cannot leak it
        // into a resident service or another exec descendant.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        ensure!(
            flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } >= 0,
            "cannot protect inherited wrapper lease"
        );
    }
    Ok(())
}

pub(super) fn exec_script(request: &SetupRequest, clock: Clock) -> Result<()> {
    #[cfg(windows)]
    {
        let _ = (request, clock);
        bail!("Windows wrappers own their validated global mutex")
    }
    #[cfg(unix)]
    {
        use std::os::{fd::AsRawFd, unix::process::CommandExt};
        let setup = recovery::review(request, clock)?;
        ensure!(
            matches!(setup.operation.requested_by.as_str(), "install-sh" | "dmg")
                || setup.operation.requested_by == "airkctl" && plan::removes(setup.operation.kind),
            "wrapper lease source is not a native installer/removal"
        );
        let script = request
            .wrapper_script
            .as_ref()
            .context("wrapper script missing")?;
        ensure!(script.is_absolute(), "wrapper script must be absolute");
        files::no_links(script)?;
        airk_ipc::private_record::require_regular_unlinked(script)?;
        let script = script.canonicalize()?;
        ensure!(
            !script.starts_with(canonical_or_same(Path::new(
                &setup.operation.locations.program
            ))),
            "wrapper script must remain outside tracked program"
        );
        let pointer = files::pointer_path()?;
        let authority_path = pointer.parent().context("authority parent")?;
        airk_ipc::endpoint::prepare_root(authority_path)?;
        let authority = airk_process::directory::OpenedDirectory::open(authority_path)?;
        let lock = airk_ipc::private_record::lock_at(
            &authority,
            LOCK,
            "installation_in_progress: another outer installer owns placement/finalization",
        )?;
        let fd = lock.as_file().as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        ensure!(
            flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } >= 0,
            "cannot inherit wrapper lock"
        );
        // exec preserves this exact kernel lock/open description. Every shell
        // copy/mv descendant inherits it; abrupt shell death cannot release
        // exclusion while a producer still owns a descriptor.
        ensure!(
            !request.wrapper_silent || setup.operation.options.trust == TrustChoice::Skip,
            "silent recovery requires original trust-skip choice; resume manually"
        );
        let mut command = std::process::Command::new("/bin/bash");
        command
            .arg(&script)
            .arg("--resume")
            .arg(&request.operation)
            .env(FD_ENV, fd.to_string())
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .env_remove("BASH_XTRACEFD");
        if request.wrapper_silent {
            command.arg("--silent");
        }
        let error = command.exec();
        Err(error).context("wrapper lease exec failed")
    }
}
