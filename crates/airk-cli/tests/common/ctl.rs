// SPDX-License-Identifier: Apache-2.0
//! In-process `airkctl` invocations for integration tests (split Phase 4).
//!
//! `Ctl` has the part of `std::process::Command`'s shape the tests use
//! (`arg`, `args`, `env`, `env_remove`, `output`, `status`, `get_args`), so a
//! test that drove the pre-split `airk` user CLI changes only its builder.
//! The run is `airkctl::cli::run` in this process: there is no child to kill,
//! so a deadline abandons the run's thread instead (`output_until`).
//!
//! Environment: `AIRK_MCP_RESPONSE_MODE=full` selects full responses and
//! `AIRK_ERROR_FORMAT=json` selects `--output json`, as the pre-split CLI read
//! them; other variables are recorded but not applied. There is no working
//! directory: every path argument must be absolute. Standard input is the
//! bytes given to `input` (empty otherwise). Every run, including `output`,
//! happens on its own thread, so a caller inside an async runtime does not
//! nest airkctl's own runtime in it.
use std::{
    ffi::{OsStr, OsString},
    io,
    process::{ExitStatus, Output},
    time::Instant,
};

#[derive(Clone, Debug, Default)]
pub struct Ctl {
    args: Vec<OsString>,
    envs: Vec<(OsString, Option<OsString>)>,
    input: Vec<u8>,
}

fn exit_status(code: u8) -> ExitStatus {
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(u32::from(code))
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(i32::from(code) << 8)
    }
}

impl Ctl {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn arg(&mut self, argument: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(argument.as_ref().to_owned());
        self
    }
    pub fn args<I, S>(&mut self, arguments: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for argument in arguments {
            self.arg(argument);
        }
        self
    }
    pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.envs
            .push((key.as_ref().to_owned(), Some(value.as_ref().to_owned())));
        self
    }
    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        self.envs.push((key.as_ref().to_owned(), None));
        self
    }
    /// The run's standard input (`Command::stdin` with piped bytes).
    pub fn input(&mut self, bytes: impl Into<Vec<u8>>) -> &mut Self {
        self.input = bytes.into();
        self
    }
    pub fn get_args(&self) -> impl Iterator<Item = &OsStr> {
        self.args.iter().map(OsString::as_os_str)
    }
    fn variable(&self, key: &str) -> Option<&OsStr> {
        self.envs
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .and_then(|(_, value)| value.as_deref())
    }
    /// The full command line `airkctl::cli::run` receives.
    pub fn command_line(&self) -> Vec<OsString> {
        let mut line = vec![OsString::from("airkctl")];
        if self.variable("AIRK_ERROR_FORMAT") == Some(OsStr::new("json"))
            && !self.args.iter().any(|argument| argument == "--output")
        {
            line.extend(["--output".into(), "json".into()]);
        }
        line.extend(self.args.iter().cloned());
        line
    }
    fn full_responses(&self) -> bool {
        self.variable("AIRK_MCP_RESPONSE_MODE") == Some(OsStr::new("full"))
    }
    pub fn output(&mut self) -> io::Result<Output> {
        let (line, full, input) = (self.command_line(), self.full_responses(), &self.input);
        std::thread::scope(|scope| {
            scope
                .spawn(move || run(line, full, input))
                .join()
                .map_err(|_| io::Error::other("airkctl run panicked"))
        })
    }
    pub fn status(&mut self) -> io::Result<ExitStatus> {
        self.output().map(|output| output.status)
    }
    /// As `output`, but give up at `deadline` (the run's thread is left to
    /// finish; nothing is killed).
    pub fn output_until(&self, deadline: Instant) -> io::Result<Output> {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "airkctl read deadline exhausted",
            ));
        }
        let (line, full, input) = (
            self.command_line(),
            self.full_responses(),
            self.input.clone(),
        );
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(run(line, full, &input));
        });
        let wait = deadline.saturating_duration_since(Instant::now());
        receiver
            .recv_timeout(wait)
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "airkctl read deadline exhausted"))
    }
}

impl super::read_retry::Invocation for Ctl {
    fn invocation_args(&self) -> Vec<&OsStr> {
        self.get_args().collect()
    }
    fn run_until(&self, deadline: Instant) -> io::Result<Output> {
        self.output_until(deadline)
    }
}

fn run(line: Vec<OsString>, full_responses: bool, input: &[u8]) -> Output {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = {
        let mut stdin = input;
        let mut io = airkctl::cli::Io {
            stdin: &mut stdin,
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses,
            window: None,
        };
        airkctl::cli::run(line, &mut io)
    };
    Output {
        status: exit_status(code),
        stdout,
        stderr,
    }
}
