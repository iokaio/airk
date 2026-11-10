// SPDX-License-Identifier: Apache-2.0
//! Installer-owned hard stop, independent of the predecessor's control protocol.
use super::*;

#[cfg(any(target_os = "macos", test))]
#[path = "runtime_upgrade_stop_macos.rs"]
mod macos;
#[cfg(target_os = "macos")]
use macos::{capture, terminate, wait};

pub(super) fn stop(root: &Path, identity: &str) -> Result<()> {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    {
        let (main, _) = verify_runtime(root, identity)?;
        let current = std::env::current_exe()?.canonicalize()?;
        ensure!(
            current != main.canonicalize()?
                && (!launcher_path(root).try_exists()?
                    || current != launcher_path(root).canonicalize()?),
            "run the upgrade from the new distribution, outside the preceding runtime"
        );
        let paths = predecessor_images(root, &main)?;
        stop_paths(&paths)?;
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    let _ = (root, identity); // Other hosts retain authenticated control/manager drain.
    Ok(())
}

/// A clean break retires every approved earlier payload in this installation,
/// including processes left by an older selection than the immediate prior.
/// Physical payload removal remains the canonical retirement owner's work.
pub(super) fn stop_all_predecessors(root: &Path, candidate: &str) -> Result<()> {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    {
        let paths = all_predecessor_images(root, candidate)?;
        let current = std::env::current_exe()?.canonicalize()?;
        for path in &paths {
            ensure!(
                !image_present(path)? || path.canonicalize()? != current,
                "run the clean break from the new distribution, outside every earlier runtime and launcher"
            );
        }
        stop_paths(&paths)?;
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    let _ = (root, candidate);
    Ok(())
}

fn require_absent_optional_alias_observation(
    path: &Path,
    audit: &crate::legacy_processes::Audit,
) -> Result<()> {
    ensure!(
        audit.complete && audit.failure.is_none() && audit.matching_executable_pids.is_empty(),
        "clean_break_predecessor_process_observation_unsupported: optional predecessor image {} is absent and has no complete native process-exit observation; successor-state activation is blocked; observation={}",
        path.display(),
        serde_json::to_string(audit)?
    );
    Ok(())
}

/// Missing optional aliases cannot acquire a Windows held-image exclusion.
/// Refuse unsupported observations before reset admission instead of rebuilding
/// fresh indexes and only discovering the same blocker during final retirement.
pub(super) fn preflight_all_predecessors(root: &Path, candidate: &str) -> Result<()> {
    for image in retirement::absent_optional_process_aliases(root, candidate)? {
        require_absent_optional_alias_observation(
            &image.path,
            &crate::legacy_processes::audit(&image.path),
        )?;
    }
    Ok(())
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn image_present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn all_predecessor_images(root: &Path, candidate: &str) -> Result<Vec<PathBuf>> {
    let (_, channel) = crate::install_layout::retained_parameters(root)?;
    all_predecessor_images_with_universal(root, candidate, || {
        let Some(image) = crate::universal_launcher::retirement_image_for(root, channel)? else {
            return Ok(None);
        };
        let sha256 = crate::universal_launcher::retirement_digest_for(root, channel, &image)?;
        Ok(Some((image, sha256)))
    })
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn all_predecessor_images_with_universal(
    root: &Path,
    candidate: &str,
    universal: impl FnOnce() -> Result<Option<(PathBuf, String)>>,
) -> Result<Vec<PathBuf>> {
    let (candidate_main, _) = verify_runtime(root, candidate)?;
    let candidate_main = candidate_main.canonicalize()?;
    // This shared inventory authenticates every noncandidate manifest/approval,
    // role copy and surviving image, however many predecessor payloads exist.
    let observed = retirement::superseded_images(root, candidate)?;
    let mut images = Vec::new();
    for image in observed {
        let path = PathBuf::from(
            image["path"]
                .as_str()
                .context("predecessor image path missing")?,
        );
        let sha256 = image["sha256"]
            .as_str()
            .context("predecessor image digest missing")?;
        digest(sha256)?;
        ensure!(
            path.starts_with(root.join("runtimes")),
            "predecessor image crossed its installation"
        );
        safe_file(&path)?;
        ensure!(
            runtime_pair::digest_file(&path)? == sha256,
            "predecessor image changed after inventory"
        );
        ensure!(
            path.canonicalize()? != candidate_main,
            "candidate crossed predecessor image roster"
        );
        if !images.contains(&path) {
            images.push(path);
        }
    }
    // The stable and universal launcher are separate receipt-owned scopes, each
    // with at most one image. They do not become approved through their names.
    let launcher = launcher_path(root);
    if image_present(&launcher)? || image_present(&root.join("launcher.json"))? {
        let receipt: LauncherReceipt =
            serde_json::from_slice(&read_bounded(&root.join("launcher.json"), 4096)?)?;
        digest(&receipt.sha256)?;
        ensure!(receipt.schema == 1, "unsupported stable launcher receipt");
        if image_present(&launcher)? {
            safe_file(&launcher)?;
            ensure!(
                runtime_pair::digest_file(&launcher)? == receipt.sha256,
                "unknown or modified launcher preserved"
            );
        }
        if !images.contains(&launcher) {
            images.push(launcher);
        }
    }
    if let Some((image, sha256)) = universal()? {
        digest(&sha256)?;
        if image_present(&image)? {
            safe_file(&image)?;
            ensure!(
                runtime_pair::digest_file(&image)? == sha256,
                "universal launcher image changed after ownership validation"
            );
            ensure!(
                image.canonicalize()? != candidate_main,
                "candidate crossed universal launcher scope"
            );
        }
        if !images.contains(&image) {
            images.push(image);
        }
    }
    Ok(images)
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn predecessor_images(root: &Path, main: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = vec![main.to_path_buf()];
    if !runtime_pair::single_executable(main)? {
        paths.push(runtime_pair::payload_root(main)?.join(helper_name()));
    }
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
        let path = crate::process_roles::executable(main, role)?;
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    Ok(paths)
}

#[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
#[path = "runtime_upgrade_stop_roster_tests.rs"]
mod all_predecessor_tests;

/// The caller must prove every image from the installation's inventory/launcher receipts.
/// Keep OS identities open through termination; filenames and PIDs alone confer no authority.
pub(super) fn stop_paths(paths: &[PathBuf]) -> Result<()> {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    {
        // Audit, capture, signal and retirement consume the same finite budget.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut owners = Vec::new();
        for path in paths {
            within_deadline(deadline, "before audit")?;
            let audit = crate::legacy_processes::audit(path);
            within_deadline(deadline, "after audit")?;
            ensure!(
                audit.failure.is_none(),
                "AIRK process audit failed: {:?}",
                audit.failure
            );
            for pid in audit.matching_executable_pids {
                within_deadline(deadline, "before capture")?;
                if let Some(owner) = capture(pid, path, deadline)? {
                    owners.push(owner);
                }
                within_deadline(deadline, "after capture")?;
            }
        }
        for owner in &owners {
            within_deadline(deadline, "before terminate")?;
            terminate(owner, deadline)?;
            within_deadline(deadline, "after terminate")?;
        }
        for owner in &owners {
            within_deadline(deadline, "before wait")?;
            wait(owner, deadline)?;
            within_deadline(deadline, "after wait")?;
        }
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    let _ = paths;
    Ok(())
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn within_deadline(deadline: std::time::Instant, phase: &str) -> Result<()> {
    ensure!(
        std::time::Instant::now() < deadline,
        "preceding AIRK process stop deadline expired {phase}; activation remains pending"
    );
    Ok(())
}

#[cfg(windows)]
type Owner = std::os::windows::io::OwnedHandle;

#[cfg(windows)]
fn capture(pid: u32, expected: &Path, deadline: std::time::Instant) -> Result<Option<Owner>> {
    within_deadline(deadline, "before capture")?;
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
fn terminate(owner: &Owner, deadline: std::time::Instant) -> Result<()> {
    within_deadline(deadline, "before terminate")?;
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

#[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_predecessor_roster_uses_payload_root_for_legacy_helper_and_main_for_current_roles()
    -> Result<()> {
        let (_owner, root, identity) = super::super::tests::unselected_historical_fixture()?;
        let (main, _, inventory) = verify_runtime_inventory(&root, &identity)?;
        prepare_launcher(&root, &main, &inventory)?;
        let paths = predecessor_images(&root, &main)?;
        assert_eq!(
            paths,
            vec![
                main.clone(),
                root.join("runtimes")
                    .join(&identity)
                    .join("AIRK.app/Contents/MacOS/airk-embed"),
                launcher_path(&root)
            ]
        );
        assert!(paths.iter().all(|path| path.is_file()));

        let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
        let (main, _) = verify_runtime(&root, &identity)?;
        assert_eq!(predecessor_images(&root, &main)?, vec![main]);
        Ok(())
    }

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
        let started = std::time::Instant::now();
        let deadline = started + Duration::from_secs(5);
        assert!(capture(pid, &std::env::current_exe()?, deadline).is_err());
        assert!(child.try_wait()?.is_none());
        let owner =
            capture(pid, &executable, deadline)?.context("fixture exited before capture")?;
        terminate(&owner, deadline)?;
        wait(&owner, deadline)?;
        assert!(!child.wait()?.success());
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(capture(std::process::id(), &std::env::current_exe()?, deadline).is_err());
        Ok(())
    }
}

#[cfg(target_os = "linux")]
type Owner = std::os::fd::OwnedFd;

#[cfg(target_os = "linux")]
fn capture(pid: u32, expected: &Path, deadline: std::time::Instant) -> Result<Option<Owner>> {
    within_deadline(deadline, "before capture")?;
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
fn terminate(owner: &Owner, deadline: std::time::Instant) -> Result<()> {
    within_deadline(deadline, "before terminate")?;
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
