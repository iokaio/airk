// SPDX-License-Identifier: Apache-2.0
//! `airkctl`: the AIRK command-line client (split plan §6). The command line
//! lives in `airkctl::cli`; this entry point runs it over the process streams.
use std::process::ExitCode;

fn main() -> ExitCode {
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
