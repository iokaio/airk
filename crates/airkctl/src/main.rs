// SPDX-License-Identifier: Apache-2.0
//! `airkctl`: the AIRK command-line client (split plan §6). The command line
//! lives in `airkctl::cli`; this entry point runs it over the process streams.
use std::process::ExitCode;

fn main() -> ExitCode {
    // Match the pre-split CLI and session request threads. Clap constructs the
    // complete command tree even for a small command; its debug derives exceed
    // the Windows main thread's default stack before parsing finishes.
    match std::thread::Builder::new()
        .name("airkctl-command".into())
        .stack_size(8 << 20)
        .spawn(command_main)
    {
        Ok(thread) => thread
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
        Err(error) => {
            let mut stderr = std::io::stderr().lock();
            ExitCode::from(airkctl::cli::startup_failure(
                &std::env::args_os().collect::<Vec<_>>(),
                &mut stderr,
                &format!("could not start command thread: {error}"),
            ))
        }
    }
}

fn command_main() -> ExitCode {
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let stdin = std::io::stdin();
    let mut io = airkctl::cli::Io {
        stdin: &mut stdin.lock(),
        stdout: &mut stdout.lock(),
        stderr: &mut stderr.lock(),
        full_responses: std::env::var("AIRK_MCP_RESPONSE_MODE").as_deref() == Ok("full"),
        window: None,
    };
    ExitCode::from(airkctl::cli::run(std::env::args_os().collect(), &mut io))
}
