// SPDX-License-Identifier: Apache-2.0
//! A task-name right authenticates the audit token; the kernel signals that
//! exact PID/idversion, never a subsequently reused numeric process ID.
//!
//! proc_pidpath_audittoken is available since macOS 11. The atomic signal API
//! arrived later than our 13.3 deployment minimum, so resolve it at runtime.
//! Unsupported systems retain graceful/SMAppService shutdown and refuse a
//! required hard stop. No debugger entitlement or task control right is used.
use anyhow::{Result, ensure};
use std::time::{Duration, Instant};

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

// An executable vnode can lose its path while its process is still alive.
// ESRCH from a path lookup alone is therefore not an exit observation.
fn path_failure_proves_retirement(missing_process_error: bool, dead_task_name: bool) -> bool {
    missing_process_error && dead_task_name
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Observation {
    Live,
    Retired,
    Unknown { errno: Option<i32> },
    Hard { errno: Option<i32> },
}

// These are Darwin errno values, including when the pure classifier is tested
// on Windows. A path lookup's ESRCH is not proof that the retained task died.
fn classify_path(path: std::result::Result<bool, i32>, dead_task_name: bool) -> Observation {
    match path {
        Ok(true) if dead_task_name => Observation::Retired,
        Ok(true) => Observation::Live,
        Ok(false) => Observation::Hard { errno: None },
        Err(3) if path_failure_proves_retirement(true, dead_task_name) => Observation::Retired,
        Err(3) => Observation::Unknown { errno: Some(3) },
        Err(errno) => Observation::Hard { errno: Some(errno) },
    }
}

#[derive(Clone, Copy)]
struct WaitContext {
    phase: &'static str,
    pid: u32,
    deadline: Instant,
}

#[derive(Debug)]
struct PendingObservation {
    phase: &'static str,
    pid: u32,
    errno: Option<i32>,
    elapsed: Duration,
    last: Observation,
}

impl std::fmt::Display for PendingObservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Mac predecessor observation pending: phase={} pid={} errno={:?} elapsed={:?} last={:?}; activation remains pending",
            self.phase, self.pid, self.errno, self.elapsed, self.last
        )
    }
}

impl std::error::Error for PendingObservation {}

fn wait_until(
    mut observe: impl FnMut() -> Result<Observation>,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
    context: WaitContext,
    accept_live: bool,
) -> Result<Observation> {
    let started = now();
    let mut last = Observation::Unknown { errno: None };
    loop {
        let before = now();
        if before >= context.deadline {
            return Err(pending(context, started, before, last).into());
        }
        last = observe()?;
        let after = now();
        if after >= context.deadline {
            return Err(pending(context, started, after, last).into());
        }
        match last {
            Observation::Retired => return Ok(last),
            Observation::Live if accept_live => return Ok(last),
            Observation::Hard { errno } => anyhow::bail!(
                "Mac predecessor identity failed: phase={} pid={} errno={errno:?}; image changed or path unavailable",
                context.phase,
                context.pid
            ),
            _ => sleep(Duration::from_millis(20).min(context.deadline - after)),
        }
    }
}

fn pending(
    context: WaitContext,
    started: Instant,
    now: Instant,
    last: Observation,
) -> PendingObservation {
    let errno = match last {
        Observation::Unknown { errno } => errno,
        Observation::Hard { errno } => errno,
        _ => None,
    };
    PendingObservation {
        phase: context.phase,
        pid: context.pid,
        errno,
        elapsed: now.saturating_duration_since(started),
        last,
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use anyhow::{Context, bail};
    use std::{
        ffi::{CStr, c_int, c_void},
        path::{Path, PathBuf},
        sync::OnceLock,
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
        fn mach_port_type(task: u32, name: u32, kind: *mut u32) -> c_int;
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
        fn task_name_is_dead(&self) -> bool {
            let mut kind = 0;
            // SAFETY: inspect only our retained task-name right. Unknown or
            // failed observations cannot establish retirement.
            let status = unsafe { mach_port_type(mach_task_self_, self.name, &mut kind) };
            const MACH_PORT_TYPE_DEAD_NAME: u32 = 1 << 20;
            status == 0 && kind & MACH_PORT_TYPE_DEAD_NAME != 0
        }

        fn observation(&self) -> Result<Observation> {
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
                return Ok(classify_path(
                    Err(error.raw_os_error().unwrap_or(0)),
                    self.task_name_is_dead(),
                ));
            }
            ensure!(
                count > 0 && (count as usize) < buffer.len(),
                "Mac process path exceeded bound"
            );
            let path = CStr::from_bytes_until_nul(&buffer)
                .context("Mac process path is not terminated")?;
            use std::os::unix::ffi::OsStrExt;
            let path = PathBuf::from(std::ffi::OsStr::from_bytes(path.to_bytes()));
            let matched = path
                .canonicalize()
                .map(|path| path == self.expected)
                .map_err(|error| error.raw_os_error().unwrap_or(0));
            Ok(classify_path(matched, self.task_name_is_dead()))
        }

        fn observe_until(
            &self,
            phase: &'static str,
            deadline: Instant,
            accept_live: bool,
        ) -> Result<Observation> {
            wait_until(
                || self.observation(),
                Instant::now,
                std::thread::sleep,
                WaitContext {
                    phase,
                    pid: self.token.0[5],
                    deadline,
                },
                accept_live,
            )
        }
    }

    pub(in super::super) fn capture(
        pid: u32,
        expected: &Path,
        deadline: Instant,
    ) -> Result<Option<Owner>> {
        super::super::within_deadline(deadline, "before Mac capture")?;
        ensure!(
            pid != 0 && pid != std::process::id() && pid <= c_int::MAX as u32,
            "installer cannot capture itself or an invalid Mac PID"
        );
        let expected = expected.canonicalize()?;
        super::super::within_deadline(deadline, "after Mac image resolution")?;
        let signal = signal_api()?;
        super::super::within_deadline(deadline, "after Mac signal API observation")?;
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
        super::super::within_deadline(deadline, "after Mac task-name observation")?;
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
        super::super::within_deadline(deadline, "after Mac audit-token observation")?;
        ensure!(
            status == 0 && words == TOKEN_WORDS,
            "Mac process audit-token capture unavailable ({status})"
        );
        // SAFETY: actual native user identity, independent of environment.
        validate_token(owner.token, pid, unsafe { libc::geteuid() })?;
        if owner.observe_until("capture", deadline, true)? == Observation::Retired {
            return Ok(None);
        }
        Ok(Some(owner))
    }

    pub(in super::super) fn terminate(owner: &Owner, deadline: Instant) -> Result<()> {
        if owner.observe_until("terminate", deadline, true)? == Observation::Retired {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "Mac predecessor stop deadline expired before signal; activation remains pending"
        );
        // SAFETY: the kernel compares the original audit PID/idversion as one
        // operation. A PID reused after this path check cannot be signaled.
        let status = unsafe { (owner.signal)(&owner.token, libc::SIGKILL) };
        ensure!(
            Instant::now() < deadline,
            "Mac predecessor stop deadline expired after signal; activation remains pending"
        );
        if status == 0 || status == libc::ESRCH {
            return Ok(());
        }
        bail!(
            "cannot stop exact Mac predecessor: {}",
            std::io::Error::from_raw_os_error(status)
        );
    }

    pub(in super::super) fn wait(owner: &Owner, deadline: Instant) -> Result<()> {
        owner.observe_until("wait", deadline, false)?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn retained_task_name_distinguishes_live_child_from_proven_exit() -> Result<()> {
            let child = std::process::Command::new("/bin/sleep").arg("60").spawn()?;
            struct Child(std::process::Child);
            impl Drop for Child {
                fn drop(&mut self) {
                    let _ = self.0.kill();
                    let _ = self.0.wait();
                }
            }
            let mut child = Child(child);
            let mut name = 0;
            // SAFETY: capture only the owned child's read-only task-name right.
            let status =
                unsafe { task_name_for_pid(mach_task_self_, child.0.id() as c_int, &mut name) };
            ensure!(
                status == 0 && name != 0,
                "owned child task-name capture failed"
            );
            unsafe extern "C" fn unused_signal(_: *const AuditToken, _: c_int) -> c_int {
                libc::EINVAL
            }
            let owner = Owner {
                name,
                token: AuditToken([0; 8]),
                expected: PathBuf::new(),
                signal: unused_signal,
            };
            assert!(!owner.task_name_is_dead());
            child.0.kill()?;
            child.0.wait()?;
            let deadline = Instant::now() + Duration::from_secs(5);
            while !owner.task_name_is_dead() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(
                owner.task_name_is_dead(),
                "owned child retirement was not observed"
            );
            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) use native::{capture, terminate, wait};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn fixture_context(deadline: Instant) -> WaitContext {
        WaitContext {
            phase: "wait",
            pid: 123,
            deadline,
        }
    }

    #[test]
    fn missing_path_without_dead_name_polls_until_positive_retirement() -> Result<()> {
        let start = Instant::now();
        let clock = Cell::new(start);
        let observations = Cell::new(0);
        let result = wait_until(
            || {
                observations.set(observations.get() + 1);
                Ok(classify_path(Err(3), observations.get() > 1))
            },
            || clock.get(),
            |duration| clock.set(clock.get() + duration),
            fixture_context(start + Duration::from_secs(1)),
            false,
        )?;
        assert_eq!(result, Observation::Retired);
        assert_eq!(observations.get(), 2);
        Ok(())
    }

    #[test]
    fn persistent_missing_path_is_pending_with_observation_context() {
        for phase in ["capture", "terminate", "wait"] {
            let start = Instant::now();
            let clock = Cell::new(start);
            let deadline = start + Duration::from_millis(35);
            let error = wait_until(
                || Ok(classify_path(Err(3), false)),
                || clock.get(),
                |duration| clock.set(clock.get() + duration),
                WaitContext {
                    phase,
                    pid: 123,
                    deadline,
                },
                phase != "wait",
            )
            .unwrap_err();
            let pending = error.downcast_ref::<PendingObservation>().unwrap();
            assert_eq!(pending.phase, phase);
            assert_eq!(pending.pid, 123);
            assert_eq!(pending.errno, Some(3));
            assert_eq!(pending.elapsed, Duration::from_millis(35));
            assert_eq!(pending.last, Observation::Unknown { errno: Some(3) });
        }
    }

    #[test]
    fn permission_and_changed_image_fail_without_polling_or_signaling() {
        assert_eq!(
            classify_path(Err(1), true),
            Observation::Hard { errno: Some(1) }
        );
        assert_eq!(
            classify_path(Ok(false), true),
            Observation::Hard { errno: None }
        );
        for observation in [
            classify_path(Err(1), false),
            classify_path(Ok(false), false),
        ] {
            let now = Instant::now();
            let result = wait_until(
                || Ok(observation),
                || now,
                |_| panic!("hard failure must not poll"),
                fixture_context(now + Duration::from_secs(1)),
                true,
            );
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .downcast_ref::<PendingObservation>()
                    .is_none()
            );
        }
    }

    #[test]
    fn expired_deadline_prevents_observation_and_late_retirement_is_not_success() {
        let start = Instant::now();
        let error = wait_until(
            || panic!("expired stop must not observe"),
            || start,
            |_| panic!("must not sleep"),
            fixture_context(start),
            true,
        )
        .unwrap_err();
        let pending = error.downcast_ref::<PendingObservation>().unwrap();
        assert_eq!(pending.errno, None);
        assert_eq!(pending.last, Observation::Unknown { errno: None });
        let clock = Cell::new(start);
        let result = wait_until(
            || {
                clock.set(start + Duration::from_secs(1));
                Ok(Observation::Retired)
            },
            || clock.get(),
            |_| panic!("must not sleep"),
            fixture_context(start + Duration::from_millis(1)),
            true,
        );
        assert!(result.is_err());
    }

    #[test]
    fn capture_and_terminate_unknown_states_share_the_original_deadline() -> Result<()> {
        let start = Instant::now();
        let clock = Cell::new(start);
        let deadline = start + Duration::from_millis(35);
        let observations = Cell::new(0);
        let capture = wait_until(
            || {
                observations.set(observations.get() + 1);
                Ok(if observations.get() == 1 {
                    classify_path(Err(3), false)
                } else {
                    Observation::Live
                })
            },
            || clock.get(),
            |duration| clock.set(clock.get() + duration),
            WaitContext {
                phase: "capture",
                pid: 123,
                deadline,
            },
            true,
        )?;
        assert_eq!(capture, Observation::Live);
        assert_eq!(clock.get() - start, Duration::from_millis(20));
        let result = wait_until(
            || Ok(classify_path(Err(3), false)),
            || clock.get(),
            |duration| clock.set(clock.get() + duration),
            WaitContext {
                phase: "terminate",
                pid: 123,
                deadline,
            },
            true,
        );
        assert!(result.is_err());
        assert_eq!(clock.get(), deadline);
        Ok(())
    }

    #[test]
    fn missing_executable_path_requires_a_proven_dead_task_name() {
        assert!(!path_failure_proves_retirement(true, false));
        assert!(path_failure_proves_retirement(true, true));
        assert!(!path_failure_proves_retirement(false, false));
        assert!(!path_failure_proves_retirement(false, true));
    }
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
