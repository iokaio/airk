// SPDX-License-Identifier: Apache-2.0
//! Bounded native command runner (split U90): one contained child, a fixed
//! deadline and capped input and output; callers own each command's
//! meaning. Shared by `airk` and `airkctl` (user certificate trust). Moved
//! from `airk-cli` (`native_command`, `native_process`) by
//! `scripts/dev/split_u90b_forwarders.py`.
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// Byte bound shared by service configuration and native command input.
pub const MAX_CONFIG: u64 = 128 * 1024;

#[derive(Clone, Debug)]
pub struct Invocation {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub input: Option<Vec<u8>>,
}
impl Invocation {
    pub fn new(program: impl Into<PathBuf>, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(|arg| (*arg).into()).collect(),
            env: BTreeMap::new(),
            input: None,
        }
    }
}
pub struct Output {
    pub success: bool,
    pub text: String,
}

#[cfg(windows)]
pub fn windows_tool(name: &str) -> Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    let mut buffer = vec![0u16; 32768];
    // SAFETY: the native API receives a writable, correctly bounded UTF-16 buffer.
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    };
    ensure!(
        length > 0 && (length as usize) < buffer.len(),
        "native system directory unavailable"
    );
    let path = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length as usize])).join(name);
    ensure!(
        path.is_absolute() && path.is_file(),
        "native Windows lifecycle tool is unavailable"
    );
    Ok(path)
}
#[cfg(not(windows))]
pub fn windows_tool(_name: &str) -> Result<PathBuf> {
    bail!("Windows lifecycle tools are unavailable on this host")
}
pub fn execute(invocation: &Invocation) -> Result<Output> {
    execute_with_timeout(invocation, Duration::from_secs(30))
}
pub fn execute_native_program(program: &Path, args: &[&str]) -> Result<bool> {
    Ok(execute(&Invocation::new(program, args))?.success)
}
pub fn execute_native_observation(program: &Path, args: &[&str]) -> Result<(bool, String)> {
    let output = execute(&Invocation::new(program, args))?;
    Ok((output.success, output.text))
}
pub fn execute_with_timeout(invocation: &Invocation, timeout: Duration) -> Result<Output> {
    ensure!(
        invocation
            .input
            .as_ref()
            .is_none_or(|input| input.len() as u64 <= MAX_CONFIG),
        "manager input exceeds byte allowance"
    );
    let mut command = Command::new(&invocation.program);
    command
        .args(&invocation.args)
        .envs(&invocation.env)
        .stdin(if invocation.input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let deadline = Instant::now() + timeout;
    let mut child = ContainedChild::spawn(&mut command)
        .with_context(|| format!("start service manager {}", invocation.program.display()))?;
    let input = if let Some(bytes) = &invocation.input {
        let bytes = bytes.clone();
        let mut input = child.stdin.take().context("missing manager input")?;
        let (sender, completion) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("airk-manager-input".into())
            .spawn(move || {
                let _ = sender.send(input.write_all(&bytes));
            })?;
        Some(completion)
    } else {
        None
    };
    let (sender, receiver) = mpsc::channel();
    for mut stream in [
        Box::new(child.stdout.take().context("missing manager stdout")?) as Box<dyn Read + Send>,
        Box::new(child.stderr.take().context("missing manager stderr")?),
    ] {
        let sender = sender.clone();
        thread::Builder::new()
            .name("airk-manager-output".into())
            .spawn(move || {
                let mut retained = Vec::new();
                let mut block = [0; 4096];
                let result = loop {
                    match stream.read(&mut block) {
                        Ok(0) => break Ok(retained),
                        Ok(read) if retained.len() + read <= 65536 => {
                            retained.extend_from_slice(&block[..read])
                        }
                        Ok(_) => break Err(std::io::Error::other("manager output exceeds 64 KiB")),
                        Err(error) => break Err(error),
                    }
                };
                let _ = sender.send(result);
            })?;
    }
    drop(sender);
    let success = loop {
        if let Some(status) = child.try_wait()? {
            break status.success();
        }
        if Instant::now() >= deadline {
            bail!(
                "service manager exceeded its command deadline; inspect OS service status before retrying"
            );
        }
        thread::sleep(Duration::from_millis(20));
    };
    if let Some(input) = input {
        input
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .context("service manager input did not complete before its deadline")??;
    }
    let mut text = String::new();
    for _ in 0..2 {
        let bytes = receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .context("service manager output did not close before its deadline")??;
        text.push_str(&String::from_utf8_lossy(&bytes));
    }
    Ok(Output { success, text })
}

/// Contain this exact newly spawned child before Init, or terminate and reap it.
/// Production containment uses it only on Windows; tests use it everywhere.
pub fn attach_owned(
    child: &mut Child,
    attach: impl FnOnce(&mut Child) -> anyhow::Result<crate::child_job::ChildJob>,
) -> anyhow::Result<crate::child_job::ChildJob> {
    use anyhow::Context;
    match attach(child) {
        Ok(containment) => Ok(containment),
        Err(error) => {
            let _ = child.kill();
            child.wait().context("failed to reap uncontained child")?;
            Err(error)
        }
    }
}

pub fn spawn(command: &mut Command) -> io::Result<Child> {
    #[cfg(unix)]
    crate::child_group::require_waitable_children()?;
    crate::detached::without_error_dialogs(|| command.spawn())
}

/// A short-lived native command is never left running by a pipe, parse or wait
/// failure. Descendants are drained before the enclosing owner releases.
#[cfg(windows)]
type ManagedChild = Child;
#[cfg(unix)]
type ManagedChild = crate::GroupChild;

pub struct ContainedChild {
    child: ManagedChild,
    #[cfg(windows)]
    containment: crate::child_job::ChildJob,
}
impl ContainedChild {
    pub fn spawn(command: &mut Command) -> anyhow::Result<Self> {
        #[cfg(windows)]
        {
            Self::attach(spawn(command)?, crate::child_job::ChildJob::attach)
        }
        #[cfg(unix)]
        Ok(Self {
            child: crate::GroupChild::spawn(command)?,
        })
    }

    #[cfg(windows)]
    pub fn attach(
        mut child: Child,
        attach: impl FnOnce(&mut Child) -> anyhow::Result<crate::child_job::ChildJob>,
    ) -> anyhow::Result<Self> {
        let containment = attach_owned(&mut child, attach)?;
        Ok(Self { child, containment })
    }
}
impl std::ops::Deref for ContainedChild {
    type Target = ManagedChild;
    fn deref(&self) -> &ManagedChild {
        &self.child
    }
}
impl std::ops::DerefMut for ContainedChild {
    fn deref_mut(&mut self) -> &mut ManagedChild {
        &mut self.child
    }
}
impl Drop for ContainedChild {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            let _ = self.child.kill();
            self.containment.drain();
            let _ = self.child.wait();
        }
        #[cfg(unix)]
        self.child.drain();
    }
}
