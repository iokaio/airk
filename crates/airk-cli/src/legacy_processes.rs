// SPDX-License-Identifier: Apache-2.0
//! Same-user executable-path candidates. PIDs here never authorize a signal.
#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::Result;
#[cfg(any(windows, target_os = "macos", test))]
use anyhow::ensure;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[cfg(any(windows, target_os = "macos"))]
use std::path::PathBuf;
const MAX_SCANNED: usize = 16384;
const MAX_MATCHES: usize = 64;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Audit {
    pub matching_executable_pids: Vec<u32>,
    pub scanned: u32,
    pub complete: bool,
    pub unobserved: u32,
    pub failure: Option<String>,
}
impl Audit {
    fn empty() -> Self {
        Self {
            matching_executable_pids: Vec::new(),
            scanned: 0,
            complete: true,
            unobserved: 0,
            failure: None,
        }
    }
    pub fn unavailable() -> Self {
        Self {
            complete: false,
            failure: Some("process_observation_unavailable".into()),
            ..Self::empty()
        }
    }
    fn matched(&mut self, pid: u32) {
        if self.matching_executable_pids.len() < MAX_MATCHES {
            self.matching_executable_pids.push(pid);
        } else {
            self.complete = false;
            self.failure = Some("matching_process_budget".into());
        }
    }
    fn missed(&mut self) {
        self.unobserved = self.unobserved.saturating_add(1);
        self.complete = false;
    }
}
fn matches(actual: &Path, expected: &Path) -> bool {
    match (actual.canonicalize(), expected.canonicalize()) {
        (Ok(actual), Ok(expected)) => actual == expected,
        _ => {
            #[cfg(windows)]
            {
                let normalize = |path: &Path| {
                    path.to_string_lossy()
                        .trim_start_matches(r"\\?\")
                        .replace('/', r"\")
                        .to_lowercase()
                };
                normalize(actual) == normalize(expected)
            }
            #[cfg(not(windows))]
            {
                actual == expected
            }
        }
    }
}
pub(crate) fn audit(executable: &Path) -> Audit {
    audit_where(&|actual| matches(actual, executable))
}
/// Same-user processes whose executable path satisfies `matcher`.
pub(crate) fn audit_where(matcher: &dyn Fn(&Path) -> bool) -> Audit {
    let mut result = platform(matcher).unwrap_or_else(|_| Audit::unavailable());
    result.matching_executable_pids.sort_unstable();
    result.matching_executable_pids.dedup();
    result
}
#[cfg(target_os = "linux")]
fn proc_executable_matches(actual: &Path, matcher: &dyn Fn(&Path) -> bool) -> bool {
    use std::os::unix::ffi::OsStrExt;
    matcher(actual)
        || actual
            .as_os_str()
            .as_bytes()
            .strip_suffix(b" (deleted)")
            .is_some_and(|bytes| matcher(Path::new(std::ffi::OsStr::from_bytes(bytes))))
}
#[cfg(target_os = "linux")]
fn platform(matcher: &dyn Fn(&Path) -> bool) -> Result<Audit> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: read-only native effective-user identity, independent of environment.
    let uid = unsafe { libc::geteuid() };
    let mut result = Audit::empty();
    for entry in std::fs::read_dir("/proc")? {
        if result.scanned as usize == MAX_SCANNED {
            result.complete = false;
            result.failure = Some("process_scan_budget".into());
            break;
        }
        result.scanned += 1;
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                result.missed();
                continue;
            }
        };
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                result.missed();
                continue;
            }
        };
        if metadata.uid() != uid {
            continue;
        }
        match std::fs::read_link(entry.path().join("exe")) {
            Ok(path) if proc_executable_matches(&path, matcher) => result.matched(pid),
            Ok(_) => (),
            Err(_) => result.missed(),
        }
    }
    Ok(result)
}
#[cfg(windows)]
fn platform(matcher: &dyn Fn(&Path) -> bool) -> Result<Audit> {
    use std::{
        ffi::OsString,
        os::windows::{
            ffi::OsStringExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
    };
    use windows_sys::Win32::System::{
        ProcessStatus::K32EnumProcesses,
        Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    };
    let mut pids = vec![0u32; MAX_SCANNED];
    let mut bytes = 0u32;
    // SAFETY: writable fixed-capacity PID buffer and sized output length.
    ensure!(
        unsafe {
            K32EnumProcesses(
                pids.as_mut_ptr(),
                std::mem::size_of_val(pids.as_slice()) as u32,
                &mut bytes,
            )
        } != 0,
        "native process enumeration failed"
    );
    ensure!(
        bytes as usize <= std::mem::size_of_val(pids.as_slice()) && bytes.is_multiple_of(4),
        "invalid process enumeration length"
    );
    let mut result = Audit::empty();
    if bytes as usize == std::mem::size_of_val(pids.as_slice()) {
        result.complete = false;
        result.failure = Some("process_scan_budget".into());
    }
    pids.truncate(bytes as usize / 4);
    let user = crate::local_ipc::current_user()?;
    for pid in pids.into_iter().filter(|pid| *pid != 0) {
        result.scanned += 1;
        // SAFETY: read-only query rights; no terminate/suspend/write capability.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            // Limited query access is granted to a process's own user at any
            // integrity level, so a denial is another account's process and
            // an invalid parameter is one that exited after enumeration.
            if !other_or_gone(std::io::Error::last_os_error().raw_os_error()) {
                result.missed();
            }
            continue;
        }
        // SAFETY: a successful OpenProcess transfers one owned native handle.
        let process = unsafe { OwnedHandle::from_raw_handle(process) };
        // The open handle keeps the PID from being reused, so a failed query
        // below is a miss only while the process is still running.
        let exit_code = || {
            let mut code = 0u32;
            // SAFETY: live read-only handle and writable exit-code storage.
            (unsafe { GetExitCodeProcess(process.as_raw_handle(), &mut code) } != 0).then_some(code)
        };
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        // SAFETY: live read-only handle, writable path buffer and bounded length.
        if unsafe {
            QueryFullProcessImageNameW(process.as_raw_handle(), 0, path.as_mut_ptr(), &mut length)
        } == 0
            || length as usize >= path.len()
        {
            if still_running(exit_code()) {
                result.missed();
            }
            continue;
        }
        let path = PathBuf::from(OsString::from_wide(&path[..length as usize]));
        if !matcher(&path) {
            continue;
        }
        match crate::local_ipc::process_user(pid) {
            Ok(owner) if owner == user => result.matched(pid),
            Ok(_) => (),
            Err(_) if still_running(exit_code()) => result.missed(),
            Err(_) => (),
        }
    }
    Ok(result)
}
/// Pure (Windows): an `OpenProcess` failure that does not hide a process of
/// this user: access denied (5, another account) or invalid parameter (87,
/// the process exited after enumeration).
#[cfg_attr(not(windows), allow(dead_code))]
fn other_or_gone(error: Option<i32>) -> bool {
    matches!(error, Some(5) | Some(87))
}
/// Pure (Windows): whether a process whose image or owner query failed may
/// still be running, from `GetExitCodeProcess` on its open handle. Only an
/// observed exit code other than `STILL_ACTIVE` (259) proves it exited (a
/// process that exits while setup drains); an unknown answer stays a miss.
#[cfg_attr(not(windows), allow(dead_code))]
fn still_running(exit_code: Option<u32>) -> bool {
    exit_code.is_none_or(|code| code == 259)
}
#[cfg(target_os = "macos")]
fn platform(matcher: &dyn Fn(&Path) -> bool) -> Result<Audit> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_listpids(kind: u32, info: u32, buffer: *mut libc::c_void, bytes: i32) -> i32;
        fn proc_pidpath(pid: i32, buffer: *mut libc::c_void, bytes: u32) -> i32;
    }
    let mut pids = vec![0i32; MAX_SCANNED];
    // SAFETY: native same-effective-user filter and aligned, bounded PID storage.
    const PROC_UID_ONLY: u32 = 4;
    let bytes = unsafe {
        proc_listpids(
            PROC_UID_ONLY,
            libc::geteuid(),
            pids.as_mut_ptr().cast(),
            std::mem::size_of_val(pids.as_slice()) as i32,
        )
    };
    ensure!(
        bytes > 0 && bytes as usize <= std::mem::size_of_val(pids.as_slice()) && bytes % 4 == 0,
        "native process enumeration failed"
    );
    let mut result = Audit::empty();
    if bytes as usize == std::mem::size_of_val(pids.as_slice()) {
        result.complete = false;
        result.failure = Some("process_scan_budget".into());
    }
    pids.truncate(bytes as usize / 4);
    for pid in pids.into_iter().filter(|pid| *pid > 0) {
        result.scanned += 1;
        let mut path = vec![0u8; 4096];
        // SAFETY: read-only path query into a fixed output buffer.
        let count = unsafe { proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
        if count <= 0 || count as usize >= path.len() {
            result.missed();
            continue;
        }
        let end = path
            .iter()
            .position(|byte| *byte == 0)
            .context("unterminated native process path")?;
        path.truncate(end);
        if matcher(&PathBuf::from(OsString::from_vec(path))) {
            result.matched(pid as u32);
        }
    }
    Ok(result)
}
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn platform(_matcher: &dyn Fn(&Path) -> bool) -> Result<Audit> {
    Ok(Audit::unavailable())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn deleted_owned_executable_remains_a_read_only_process_candidate() -> Result<()> {
        use std::process::{Command, Stdio};
        let owner = tempfile::tempdir()?;
        let executable = owner.path().canonicalize()?.join("airk");
        std::fs::copy(std::env::current_exe()?, &executable)?;
        let child = Command::new(&executable)
            .args(["--exact", "service_manager::tests::manager_command_fixture"])
            .env("AIRK_MANAGER_COMMAND_FIXTURE", "blocked-input")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        struct Owned(std::process::Child);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let child = Owned(child);
        std::fs::remove_file(&executable)?;
        let observation = audit(&executable);
        assert!(
            observation.matching_executable_pids.contains(&child.0.id()),
            "deleted owned fixture not observed: {observation:?}"
        );
        Ok(())
    }
    #[test]
    fn a_failed_query_is_excused_only_by_an_observed_exit() {
        assert!(!still_running(Some(0)));
        assert!(!still_running(Some(1)));
        assert!(still_running(Some(259)), "STILL_ACTIVE");
        assert!(still_running(None), "unknown exit state stays a miss");
    }
    #[test]
    fn only_another_accounts_or_an_exited_process_is_not_a_miss() {
        assert!(other_or_gone(Some(5)));
        assert!(other_or_gone(Some(87)));
        for error in [None, Some(0), Some(6), Some(1314)] {
            assert!(!other_or_gone(error), "{error:?}");
        }
    }
    #[test]
    fn same_user_exact_executable_audit_finds_self_without_a_stop_capability() -> Result<()> {
        let executable = std::env::current_exe()?;
        let result = audit(&executable);
        ensure!(
            result
                .matching_executable_pids
                .contains(&std::process::id()),
            "current owned test process was not observed: {result:?}"
        );
        assert!(
            result.matching_executable_pids.len() <= MAX_MATCHES
                && result.scanned as usize <= MAX_SCANNED
        );
        let other = tempfile::tempdir()?.path().join("not-an-airk-executable");
        assert!(!matches(&executable, &other));
        Ok(())
    }
}
