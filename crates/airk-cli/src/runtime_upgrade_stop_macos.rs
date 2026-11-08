// SPDX-License-Identifier: Apache-2.0
//! A task-name right authenticates the audit token; the kernel signals that
//! exact PID/idversion, never a subsequently reused numeric process ID.
//!
//! proc_pidpath_audittoken is available since macOS 11. The atomic signal API
//! arrived later than our 13.3 deployment minimum, so resolve it at runtime.
//! Unsupported systems retain graceful/SMAppService shutdown and refuse a
//! required hard stop. No debugger entitlement or task control right is used.
use anyhow::{Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
struct AuditToken([u32; 8]);

fn validate_token(token: AuditToken, expected_pid: u32, user: u32) -> Result<()> {
    ensure!(
        expected_pid != 0
            && token.0[5] == expected_pid
            && token.0[1] == user
            && token.0[3] == user
            && token.0[7] != 0,
        "Mac upgrade process audit identity differs"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use anyhow::{Context, bail};
    use std::{
        ffi::{CStr, c_int, c_void},
        path::{Path, PathBuf},
        sync::OnceLock,
        time::{Duration, Instant},
    };

    type Signal = unsafe extern "C" fn(*const AuditToken, c_int) -> c_int;
    const TASK_AUDIT_TOKEN: u32 = 15;
    const TOKEN_WORDS: u32 = 8;

    #[link(name = "System")]
    unsafe extern "C" {
        static mach_task_self_: u32;
        fn task_name_for_pid(task: u32, pid: c_int, name: *mut u32) -> c_int;
        fn task_info(task: u32, flavor: u32, output: *mut u32, words: *mut u32) -> c_int;
        fn mach_port_deallocate(task: u32, name: u32) -> c_int;
    }
    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_pidpath_audittoken(
            token: *const AuditToken,
            path: *mut c_void,
            capacity: u32,
        ) -> c_int;
    }

    fn signal_api() -> Result<Signal> {
        static API: OnceLock<Option<Signal>> = OnceLock::new();
        // SAFETY: resolve the public native function without introducing a
        // strong reference that would prevent startup on older supported OSes.
        let api = API.get_or_init(|| unsafe {
            let pointer = libc::dlsym(libc::RTLD_DEFAULT, c"proc_signal_with_audittoken".as_ptr());
            if pointer.is_null() {
                None
            } else {
                Some(std::mem::transmute::<*mut c_void, Signal>(pointer))
            }
        });
        api.context("macos_atomic_process_stop_unavailable: graceful or Service Management shutdown must release the old owners on this OS")
    }

    pub(in super::super) struct Owner {
        name: u32,
        token: AuditToken,
        expected: PathBuf,
        signal: Signal,
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            // SAFETY: this object owns the send right acquired by capture.
            unsafe { mach_port_deallocate(mach_task_self_, self.name) };
        }
    }
    impl Owner {
        fn path(&self) -> Result<Option<PathBuf>> {
            let mut buffer = [0u8; 4096];
            // SAFETY: token is captured from our retained task-name port; this
            // API atomically compares PID/idversion before reading its path.
            let count = unsafe {
                *libc::__error() = 0;
                proc_pidpath_audittoken(
                    &self.token,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                )
            };
            if count == 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::ESRCH) {
                    return Ok(None);
                }
                return Err(error).context("Mac captured process path unavailable");
            }
            ensure!(
                count > 0 && (count as usize) < buffer.len(),
                "Mac process path exceeded bound"
            );
            let path = CStr::from_bytes_until_nul(&buffer)
                .context("Mac process path is not terminated")?;
            use std::os::unix::ffi::OsStrExt;
            let path = PathBuf::from(std::ffi::OsStr::from_bytes(path.to_bytes()));
            ensure!(
                path.canonicalize()? == self.expected,
                "Mac captured process image changed"
            );
            Ok(Some(path))
        }
    }

    pub(in super::super) fn capture(pid: u32, expected: &Path) -> Result<Option<Owner>> {
        ensure!(
            pid != 0 && pid != std::process::id() && pid <= c_int::MAX as u32,
            "installer cannot capture itself or an invalid Mac PID"
        );
        let expected = expected.canonicalize()?;
        let signal = signal_api()?;
        let mut name = 0;
        // SAFETY: read-only task-name capability, not task_for_pid/control.
        let status = unsafe { task_name_for_pid(mach_task_self_, pid as c_int, &mut name) };
        ensure!(
            status == 0 && name != 0,
            "Mac process task-name capture unavailable ({status}); no numeric PID signal attempted"
        );
        let mut owner = Owner {
            name,
            token: AuditToken([0; 8]),
            expected,
            signal,
        };
        let mut words = TOKEN_WORDS;
        // SAFETY: eight writable token words and a retained read-only right.
        let status = unsafe {
            task_info(
                name,
                TASK_AUDIT_TOKEN,
                owner.token.0.as_mut_ptr(),
                &mut words,
            )
        };
        ensure!(
            status == 0 && words == TOKEN_WORDS,
            "Mac process audit-token capture unavailable ({status})"
        );
        // SAFETY: actual native user identity, independent of environment.
        validate_token(owner.token, pid, unsafe { libc::geteuid() })?;
        if owner.path()?.is_none() {
            return Ok(None);
        }
        Ok(Some(owner))
    }

    pub(in super::super) fn terminate(owner: &Owner) -> Result<()> {
        if owner.path()?.is_none() {
            return Ok(());
        }
        // SAFETY: the kernel compares the original audit PID/idversion as one
        // operation. A PID reused after this path check cannot be signaled.
        let status = unsafe { (owner.signal)(&owner.token, libc::SIGKILL) };
        if status == 0 || status == libc::ESRCH {
            return Ok(());
        }
        bail!(
            "cannot stop exact Mac predecessor: {}",
            std::io::Error::from_raw_os_error(status)
        );
    }

    pub(in super::super) fn wait(owner: &Owner, deadline: Instant) -> Result<()> {
        loop {
            if owner.path()?.is_none() {
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "exact Mac predecessor did not exit; activation remains pending"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) use native::{capture, terminate, wait};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_requires_exact_pid_native_user_and_nonzero_execution_generation() -> Result<()> {
        let mut token = AuditToken([501, 501, 20, 501, 20, 123, 1, 72]);
        validate_token(token, 123, 501)?;
        assert!(validate_token(token, 124, 501).is_err());
        assert!(validate_token(token, 123, 502).is_err());
        for field in [1, 3, 5, 7] {
            let original = token;
            token.0[field] = 0;
            assert!(validate_token(token, 123, 501).is_err());
            token = original;
        }
        Ok(())
    }
}
