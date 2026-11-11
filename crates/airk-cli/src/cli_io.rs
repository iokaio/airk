// SPDX-License-Identifier: Apache-2.0
//! Standard streams for one CLI command body.
//!
//! The `airk` binary uses [`CliIo::process`], which writes exactly what the
//! former `println!`/`print!`/`eprintln!` calls wrote, including their panic on a
//! closed stream. `airk service session` runs an allowlisted command inside the
//! session process against [`CliIo::captured`]: the request's own stdin, a
//! complete stdout bounded by the response budget (overflow is reported, never
//! truncated silently) and the tail of stderr.
//!
//! Deep library diagnostics that have no `io` parameter use [`diagnostic_line`]:
//! inside [`CliIo::scope_diagnostics`] (a session request) they reach that
//! request's stderr; elsewhere they are `eprintln!` exactly as before.
use std::{
    fmt::Display,
    io::{self, Read, Write},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct CliIo(Streams);

#[derive(Clone)]
enum Streams {
    Process,
    Captured(Arc<Mutex<Captured>>),
}

tokio::task_local! {
    static DIAGNOSTICS: CliIo;
}

/// One diagnostic line for the current command's stderr (see the module docs).
pub fn diagnostic_line(text: impl Display) {
    let text = text.to_string();
    if DIAGNOSTICS.try_with(|io| io.error_line(&text)).is_err() {
        eprintln!("{text}");
    }
}

struct Captured {
    stdin: Option<Vec<u8>>,
    stdout: Vec<u8>,
    stdout_limit: usize,
    stdout_overflow: bool,
    stderr: Vec<u8>,
    stderr_limit: usize,
}

/// What a captured command wrote. `stderr` holds at most its limit, the tail kept.
pub struct CapturedOutput {
    pub stdout: Vec<u8>,
    pub stdout_overflow: bool,
    pub stderr: Vec<u8>,
}

impl CliIo {
    pub fn process() -> Self {
        Self(Streams::Process)
    }

    pub fn captured(stdin: Option<Vec<u8>>, stdout_limit: usize, stderr_limit: usize) -> Self {
        Self(Streams::Captured(Arc::new(Mutex::new(Captured {
            stdin,
            stdout: Vec::new(),
            stdout_limit,
            stdout_overflow: false,
            stderr: Vec::new(),
            stderr_limit,
        }))))
    }

    /// Run `future` with [`diagnostic_line`] writing to this command's stderr.
    pub async fn scope_diagnostics<F: std::future::Future>(&self, future: F) -> F::Output {
        DIAGNOSTICS.scope(self.clone(), future).await
    }

    fn with(&self, apply: impl FnOnce(&mut Captured)) {
        if let Streams::Captured(captured) = &self.0 {
            // A panicking writer leaves only appended bytes behind; keep them.
            let mut captured = captured
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            apply(&mut captured);
        }
    }

    /// `println!("{text}")`.
    pub fn line(&self, text: impl Display) {
        match &self.0 {
            Streams::Process => println!("{text}"),
            Streams::Captured(_) => {
                self.with(|captured| captured.out(format!("{text}\n").as_bytes()))
            }
        }
    }

    /// `print!("{text}")`.
    pub fn text(&self, text: impl Display) {
        match &self.0 {
            Streams::Process => print!("{text}"),
            Streams::Captured(_) => {
                self.with(|captured| captured.out(format!("{text}").as_bytes()))
            }
        }
    }

    /// Raw stdout bytes, as `stdout().lock().write_all(bytes)` did.
    pub fn bytes(&self, bytes: &[u8]) -> io::Result<()> {
        match &self.0 {
            Streams::Process => io::stdout().lock().write_all(bytes),
            Streams::Captured(_) => {
                self.with(|captured| captured.out(bytes));
                Ok(())
            }
        }
    }

    pub fn flush(&self) -> io::Result<()> {
        match &self.0 {
            Streams::Process => io::stdout().flush(),
            Streams::Captured(_) => Ok(()),
        }
    }

    /// `eprintln!("{text}")`.
    pub fn error_line(&self, text: impl Display) {
        match &self.0 {
            Streams::Process => eprintln!("{text}"),
            Streams::Captured(_) => {
                self.with(|captured| captured.err(format!("{text}\n").as_bytes()))
            }
        }
    }

    /// The command's standard input: the process stream, or the request's bytes
    /// once (an absent or already consumed request input reads as empty).
    pub fn stdin(&self) -> Box<dyn Read + '_> {
        match &self.0 {
            Streams::Process => Box::new(io::stdin()),
            Streams::Captured(_) => {
                let mut bytes = Vec::new();
                self.with(|captured| bytes = captured.stdin.take().unwrap_or_default());
                Box::new(io::Cursor::new(bytes))
            }
        }
    }

    /// Captured output; the process variant has nothing retained.
    pub fn finish(self) -> CapturedOutput {
        match self.0 {
            Streams::Process => CapturedOutput {
                stdout: Vec::new(),
                stdout_overflow: false,
                stderr: Vec::new(),
            },
            Streams::Captured(captured) => {
                let mut captured = captured
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                CapturedOutput {
                    stdout: std::mem::take(&mut captured.stdout),
                    stdout_overflow: captured.stdout_overflow,
                    stderr: std::mem::take(&mut captured.stderr),
                }
            }
        }
    }
}

impl Captured {
    fn out(&mut self, bytes: &[u8]) {
        if self.stdout_overflow || bytes.len() > self.stdout_limit - self.stdout.len() {
            // Retain nothing further: the response is refused as oversize.
            self.stdout_overflow = true;
            return;
        }
        self.stdout.extend_from_slice(bytes);
    }

    fn err(&mut self, bytes: &[u8]) {
        self.stderr.extend_from_slice(bytes);
        if self.stderr.len() > self.stderr_limit {
            let excess = self.stderr.len() - self.stderr_limit;
            self.stderr.drain(..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_streams_keep_stdout_complete_stderr_tail_and_stdin_once() {
        let io = CliIo::captured(Some(b"request input".to_vec()), 16, 8);
        let mut input = String::new();
        io.stdin().read_to_string(&mut input).unwrap();
        assert_eq!(input, "request input");
        let mut again = String::new();
        io.stdin().read_to_string(&mut again).unwrap();
        assert_eq!(again, "", "request input is consumed once");
        io.line("{}");
        io.text("ab");
        io.bytes(b"cd").unwrap();
        io.error_line("first diagnostic");
        io.error_line("tail");
        let output = io.finish();
        assert_eq!(output.stdout, b"{}\nabcd");
        assert!(!output.stdout_overflow);
        assert_eq!(output.stderr, b"ic\ntail\n");
    }

    #[test]
    fn captured_stdout_overflow_is_reported_never_truncated_silently() {
        let io = CliIo::captured(None, 4, 4);
        io.text("abc");
        io.text("de");
        io.text("f");
        let output = io.finish();
        assert!(output.stdout_overflow);
        assert_eq!(output.stdout, b"abc", "nothing past the budget is retained");
    }

    #[test]
    fn scoped_diagnostics_reach_only_that_commands_stderr() {
        let io = CliIo::captured(None, 64, 64);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(io.scope_diagnostics(async {
            tokio::task::yield_now().await;
            diagnostic_line("deep library diagnostic");
        }));
        // Outside a scope the process stream is used, as before.
        diagnostic_line("process diagnostic");
        assert_eq!(io.finish().stderr, b"deep library diagnostic\n");
    }
}
