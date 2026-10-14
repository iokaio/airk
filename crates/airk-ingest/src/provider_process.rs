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
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}
impl Process {
    pub fn spawn(spec: &ProviderCommand, directory: &Path, extra: &[String]) -> Result<Self> {
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
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let child = command
            .spawn()
            .context("start explicitly provisioned local provider")?;
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
            // SAFETY: all pointers reference initialized structs, and owned handles remain
            // valid until Drop. Closing this job terminates its descendant processes.
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            let mut process = Self { child, job };
            if job.is_null() {
                anyhow::bail!(
                    "create provider process job: {}",
                    std::io::Error::last_os_error()
                );
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = unsafe {
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&limits) as u32,
                )
            };
            let assigned = configured != 0
                && unsafe { AssignProcessToJobObject(job, process.child.as_raw_handle().cast()) }
                    != 0;
            if !assigned {
                let error = std::io::Error::last_os_error();
                let _ = process.child.kill();
                let _ = process.child.wait();
                unsafe {
                    CloseHandle(job);
                }
                process.job = std::ptr::null_mut();
                anyhow::bail!("contain provider process job: {error}");
            }
            Ok(process)
        }
        #[cfg(not(windows))]
        {
            Ok(Self { child })
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        #[cfg(windows)]
        if !self.job.is_null() {
            // SAFETY: this is the unique owned job handle.
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.job);
            }
        }
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
    let deadline = Instant::now() + Duration::from_millis(spec.timeout_ms.min(120_000));
    let mut process = Process::spawn(spec, directory, extra)?;
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
        for _ in 0..2 {
            let (is_output, bytes) = receiver
                .recv_timeout(remaining(deadline)?)
                .context("provider output timeout/disconnect")?;
            let bytes = bytes?;
            if is_output {
                output = Some(bytes);
            }
        }
        loop {
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
