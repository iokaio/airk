// SPDX-License-Identifier: Apache-2.0
//! Bounded local subprocess transport. Programs are explicitly selected, never shells.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderCommand {
    pub program: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    pub timeout_ms: u64,
}
impl ProviderCommand {
    pub(crate) fn validate(&self) -> Result<PathBuf> {
        ensure!(
            (100..=120_000).contains(&self.timeout_ms),
            "provider timeout must be 100..120000 ms"
        );
        ensure!(
            self.program.is_absolute(),
            "provider executable must be an explicitly selected absolute path"
        );
        ensure!(
            self.args.len() <= 64
                && self
                    .args
                    .iter()
                    .all(|arg| arg.len() <= 4096 && !arg.contains('\0')),
            "provider argument limit exceeded"
        );
        let program = self.program.canonicalize()?;
        ensure!(
            program.is_file(),
            "provider executable is not a regular file"
        );
        Ok(program)
    }
}

pub(crate) struct Process {
    pub child: Child,
    #[cfg(target_os = "macos")]
    memory_limit: Option<usize>,
    #[cfg(windows)]
    job: airk_process::ChildJob,
}
#[derive(Clone, Copy)]
pub(crate) struct ProcessLimits {
    pub memory_bytes: usize,
    pub cpu_seconds: u64,
}
impl Process {
    pub fn spawn(spec: &ProviderCommand, directory: &Path, extra: &[String]) -> Result<Self> {
        Self::spawn_limited(spec, directory, extra, None)
    }

    fn spawn_limited(
        spec: &ProviderCommand,
        directory: &Path,
        extra: &[String],
        limits: Option<ProcessLimits>,
    ) -> Result<Self> {
        let mut command = Command::new(spec.validate()?);
        command
            .args(&spec.args)
            .args(extra)
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Avoid providers writing caches in the source directory. Providers remain trusted
        // local executables, not a security sandbox; no workspace command is requested.
        command
            .env("TMPDIR", directory)
            .env("TMP", directory)
            .env("TEMP", directory);
        command.env("CARGO_NET_OFFLINE", "true");
        if limits.is_some() {
            command.env("RAYON_NUM_THREADS", "1");
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            if let Some(limits) = limits {
                // SAFETY: only async-signal-safe setrlimit calls run between fork and exec.
                unsafe {
                    command.pre_exec(move || {
                        #[cfg(not(target_os = "macos"))]
                        let resources = [
                            (libc::RLIMIT_AS, limits.memory_bytes as libc::rlim_t),
                            (libc::RLIMIT_CPU, limits.cpu_seconds as libc::rlim_t),
                        ];
                        // Darwin rejects a finite RLIMIT_AS below the inherited
                        // parent's existing VM mappings. Observe physical footprint
                        // from the parent instead; CPU remains a kernel-enforced limit.
                        #[cfg(target_os = "macos")]
                        let resources = [(libc::RLIMIT_CPU, limits.cpu_seconds as libc::rlim_t)];
                        for (resource, value) in resources {
                            let limit = libc::rlimit {
                                rlim_cur: value,
                                rlim_max: value,
                            };
                            if libc::setrlimit(resource, &limit) != 0 {
                                return Err(std::io::Error::last_os_error());
                            }
                        }
                        Ok(())
                    });
                }
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No provider instruction can execute before job containment is installed.
            command.creation_flags(0x08000000 | 0x00000004); // CREATE_NO_WINDOW | CREATE_SUSPENDED
        }
        #[cfg(windows)]
        let job = airk_process::ChildJob::unassigned()?;
        let child = command
            .spawn()
            .context("start explicitly provisioned local provider")?;
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::*;
            // SAFETY: all pointers reference initialized structs, and owned handles remain
            // valid until Drop. Closing this job terminates its descendant processes.
            let mut process = Self { child, job };
            let job = process.job.as_raw_handle();
            let mut job_limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION =
                unsafe { std::mem::zeroed() };
            job_limits.BasicLimitInformation.LimitFlags =
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
            job_limits.BasicLimitInformation.ActiveProcessLimit = 32;
            if let Some(limits) = limits {
                job_limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY
                    | JOB_OBJECT_LIMIT_JOB_MEMORY
                    | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                    | JOB_OBJECT_LIMIT_PROCESS_TIME;
                job_limits.ProcessMemoryLimit = limits.memory_bytes;
                job_limits.JobMemoryLimit = limits.memory_bytes;
                job_limits.BasicLimitInformation.ActiveProcessLimit = 1;
                job_limits.BasicLimitInformation.PerProcessUserTimeLimit =
                    (limits.cpu_seconds * 10_000_000) as i64;
            }
            let configured = unsafe {
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    (&job_limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&job_limits) as u32,
                )
            };
            let assigned = configured != 0
                && unsafe { AssignProcessToJobObject(job, process.child.as_raw_handle().cast()) }
                    != 0;
            if !assigned {
                let error = std::io::Error::last_os_error();
                let _ = process.child.kill();
                let _ = process.child.wait();
                anyhow::bail!("contain provider process job: {error}");
            }
            resume_suspended(process.child.id()).context("resume contained provider")?;
            Ok(process)
        }
        #[cfg(not(windows))]
        {
            Ok(Self {
                child,
                #[cfg(target_os = "macos")]
                memory_limit: limits.map(|limits| limits.memory_bytes),
            })
        }
    }

    fn check_memory_budget(&mut self) -> Result<()> {
        #[cfg(target_os = "macos")]
        if let Some(limit) = self.memory_limit {
            // A reaped child has no further allocations. A concurrent exit between
            // this check and proc_pid_rusage is handled by the second try_wait.
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            let mut usage: libc::rusage_info_v0 = unsafe { std::mem::zeroed() };
            // SAFETY: the flavor matches this initialized writable buffer, the PID
            // is our unreaped child, and the API accepts its address as rusage_info_t*.
            let result = unsafe {
                libc::proc_pid_rusage(
                    self.child.id() as i32,
                    libc::RUSAGE_INFO_V0,
                    (&mut usage as *mut libc::rusage_info_v0).cast::<libc::rusage_info_t>(),
                )
            };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if self.child.try_wait()?.is_some() {
                    return Ok(());
                }
                return Err(error).context("observe macOS provider memory budget");
            }
            ensure!(
                usage.ri_phys_footprint <= limit as u64,
                "local provider exceeded its macOS physical-memory budget ({} > {} bytes)",
                usage.ri_phys_footprint,
                limit
            );
        }
        Ok(())
    }
}

#[cfg(windows)]
fn resume_suspended(pid: u32) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };
    // std::process owns the process handle but not the primary thread handle. A
    // suspended process has exactly its initial thread; enumerate and resume it
    // only after assignment to the kill-on-close job. All handles are closed.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        ensure!(
            snapshot != INVALID_HANDLE_VALUE,
            "snapshot suspended provider threads: {}",
            std::io::Error::last_os_error()
        );
        let result = (|| {
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            let mut available = Thread32First(snapshot, &mut entry);
            while available != 0 {
                if entry.th32OwnerProcessID == pid {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    ensure!(
                        !thread.is_null(),
                        "open suspended provider thread: {}",
                        std::io::Error::last_os_error()
                    );
                    let resumed = ResumeThread(thread);
                    let error = std::io::Error::last_os_error();
                    CloseHandle(thread);
                    ensure!(
                        resumed != u32::MAX,
                        "resume suspended provider thread: {error}"
                    );
                    return Ok(());
                }
                available = Thread32Next(snapshot, &mut entry);
            }
            anyhow::bail!("suspended provider thread missing")
        })();
        CloseHandle(snapshot);
        result
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        #[cfg(windows)]
        self.job.drain();
        #[cfg(unix)]
        {
            // SAFETY: a negative child PID addresses the private process group created
            // above; the provider never shares AIRK's process group.
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .context("local provider exceeded its time budget")
}

pub(crate) fn read_capped(mut stream: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream
        .by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "local provider output limit exceeded");
    Ok(bytes)
}

pub(crate) fn run(
    spec: &ProviderCommand,
    directory: &Path,
    extra: &[String],
    max_output: usize,
) -> Result<Vec<u8>> {
    run_limited(spec, directory, extra, max_output, None)
}

pub(crate) fn run_limited(
    spec: &ProviderCommand,
    directory: &Path,
    extra: &[String],
    max_output: usize,
    limits: Option<ProcessLimits>,
) -> Result<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_millis(spec.timeout_ms.min(120_000));
    let mut process = Process::spawn_limited(spec, directory, extra, limits)?;
    drop(process.child.stdin.take());
    let stdout = process
        .child
        .stdout
        .take()
        .context("missing provider stdout")?;
    let stderr = process
        .child
        .stderr
        .take()
        .context("missing provider stderr")?;
    let (sender, receiver) = mpsc::channel();
    let errors = sender.clone();
    let output_thread = thread::spawn(move || {
        let _ = sender.send((true, read_capped(stdout, max_output)));
    });
    let error_thread = thread::spawn(move || {
        let _ = errors.send((false, read_capped(stderr, 65_536)));
    });
    let result = (|| {
        let mut output = None;
        let mut collected = 0;
        while collected < 2 {
            process.check_memory_budget()?;
            let (is_output, bytes) =
                match receiver.recv_timeout(remaining(deadline)?.min(Duration::from_millis(5))) {
                    Ok(message) => message,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(error) => return Err(error).context("provider output disconnect"),
                };
            collected += 1;
            let bytes = bytes?;
            if is_output {
                output = Some(bytes);
            }
        }
        loop {
            process.check_memory_budget()?;
            if let Some(status) = process.child.try_wait()? {
                ensure!(
                    status.success(),
                    "local provider exited unsuccessfully ({status}); no evidence published"
                );
                return output.context("provider stdout missing");
            }
            thread::sleep(remaining(deadline)?.min(Duration::from_millis(5)));
        }
    })();
    drop(process); // terminate descendants before joining blocked pipe readers
    let _ = output_thread.join();
    let _ = error_thread.join();
    result
}

pub(crate) struct Writer {
    sender: Option<mpsc::SyncSender<Vec<u8>>>,
    acknowledgements: mpsc::Receiver<Result<()>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Writer {
    pub fn new(mut input: impl Write + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(1);
        let (ack, acknowledgements) = mpsc::channel();
        let thread = thread::spawn(move || {
            while let Ok(message) = receiver.recv() {
                let result = input
                    .write_all(&message)
                    .and_then(|_| input.flush())
                    .map_err(anyhow::Error::from);
                let failed = result.is_err();
                if ack.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Self {
            sender: Some(sender),
            acknowledgements,
            thread: Some(thread),
        }
    }
    pub fn send(&self, message: Vec<u8>, deadline: Instant) -> Result<()> {
        self.sender
            .as_ref()
            .context("provider input closed")?
            .try_send(message)
            .context("provider input backlog")?;
        self.acknowledgements
            .recv_timeout(remaining(deadline)?)
            .context("provider input timeout/disconnect")??;
        Ok(())
    }
    pub fn close(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "invoked only as a bounded subprocess fixture"]
    fn sleeping_fixture() {
        thread::sleep(Duration::from_secs(60));
    }

    #[test]
    #[ignore = "invoked only as a bounded subprocess fixture"]
    fn descendant_fixture() -> Result<()> {
        // Spawn immediately: containment must exist before any provider code runs.
        let mut descendant = Command::new(std::env::current_exe()?)
            .args([
                "--ignored",
                "--exact",
                "provider_process::tests::sleeping_fixture",
            ])
            .spawn()?;
        std::fs::write("descendant.pid", descendant.id().to_string())?;
        let _ = descendant.wait();
        Ok(())
    }

    #[test]
    #[ignore = "invoked only as a bounded subprocess fixture"]
    fn allocation_fixture() {
        #[cfg(not(target_os = "macos"))]
        {
            let mut bytes = Vec::<u8>::new();
            assert!(
                bytes.try_reserve_exact(1024 * 1024 * 1024).is_err(),
                "child allocation limit was not enforced"
            );
            assert!(
                bytes.try_reserve_exact(1024 * 1024).is_ok(),
                "ordinary child allocation failed"
            );
        }
        #[cfg(target_os = "macos")]
        {
            // Physical footprint requires committed pages, not a virtual reservation.
            // Grow gradually so the watchdog must detect the named memory reason.
            let mut committed = Vec::new();
            for _ in 0..1024 {
                committed.push(vec![0x5a; 1024 * 1024]);
                std::hint::black_box(&committed);
                thread::sleep(Duration::from_millis(2));
            }
            thread::sleep(Duration::from_secs(60));
        }
    }

    #[test]
    fn limited_child_memory_and_immediate_descendants_are_contained() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let mut command = ProviderCommand {
            program: std::env::current_exe()?,
            args: vec![
                "--ignored".into(),
                "--exact".into(),
                "provider_process::tests::allocation_fixture".into(),
            ],
            timeout_ms: 10_000,
        };
        let allocation = run_limited(
            &command,
            temporary.path(),
            &[],
            65_536,
            Some(ProcessLimits {
                memory_bytes: 512 * 1024 * 1024,
                cpu_seconds: 10,
            }),
        );
        #[cfg(not(target_os = "macos"))]
        allocation?;
        #[cfg(target_os = "macos")]
        {
            let error = allocation.expect_err("committed allocation must trigger the watchdog");
            assert!(
                error.to_string().contains("macOS physical-memory budget"),
                "{error:#}"
            );
        }
        command.args[2] = "provider_process::tests::descendant_fixture".into();
        command.timeout_ms = 1500;
        let started = Instant::now();
        assert!(run(&command, temporary.path(), &[], 65_536).is_err());
        assert!(
            temporary.path().join("descendant.pid").is_file(),
            "descendant fixture never started"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "descendant kept its inherited output pipe open after timeout"
        );
        Ok(())
    }

    #[test]
    fn subprocess_output_and_runtime_limits_terminate_and_reap() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let mut command = ProviderCommand {
            program: std::env::current_exe()?,
            args: vec!["--list".into()],
            timeout_ms: 10_000,
        };
        assert!(run(&command, temporary.path(), &[], 8).is_err());
        command.args = vec![
            "--ignored".into(),
            "--exact".into(),
            "provider_process::tests::sleeping_fixture".into(),
        ];
        command.timeout_ms = 200;
        let started = Instant::now();
        assert!(run(&command, temporary.path(), &[], 65_536).is_err());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timed-out child was not terminated promptly"
        );
        Ok(())
    }
}
