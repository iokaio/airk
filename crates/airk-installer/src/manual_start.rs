// SPDX-License-Identifier: Apache-2.0
//! `airk` without a private role (split plan §5.1, Phase 4).
//!
//! With no arguments `airk` starts the installed service detached and says
//! so, or says it is already running. With any other arguments it explains
//! that `airk` takes no commands and that `airkctl` is the CLI, with the
//! current running state, and starts nothing.
//!
//! One implementation of start and status exists, in `airkctl` (it verifies
//! the installation and admits the private `service-detached` role, contracts
//! §5.7). `airk` runs the `airkctl` installed beside it and renders its JSON;
//! the rendering is pure and tested here.
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// What the user sees and the exit status.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub message: String,
    pub code: u8,
}

const STOP: &str = "`airkctl service stop` stops it.";

/// The `airkctl` installed beside this `airk`.
pub fn sibling_airkctl(airk: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "airkctl.exe"
    } else {
        "airkctl"
    };
    airk.parent().map(|folder| folder.join(name))
}

fn version(health: &Value) -> String {
    health["version"]
        .as_str()
        .map(|version| format!(" (version {version})"))
        .unwrap_or_default()
}

/// The error message of an `airkctl --output json` error line, if any.
fn error_message(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
        .and_then(|line| line["message"].as_str().map(str::to_owned))
}

/// Whether a status document says the service answers.
pub fn running_health(status: &Value) -> Option<&Value> {
    (status["service"]["state"] == "running").then(|| &status["service"]["health"])
}

/// No arguments, the service already answers.
pub fn already_running(health: &Value) -> Outcome {
    Outcome {
        message: format!(
            "The AIRK service is already running{}. `airkctl service status` shows details; {STOP}",
            version(health)
        ),
        code: 0,
    }
}

/// No arguments, after `airkctl service start` exited with `code`.
pub fn started(code: i32, stdout: &str, stderr: &str) -> Outcome {
    if code == 0 {
        let health = serde_json::from_str::<Value>(stdout.trim()).unwrap_or(Value::Null);
        let health = health.get("observation").unwrap_or(&health);
        return Outcome {
            message: format!(
                "The AIRK service has started{}. It keeps running after this terminal closes; {STOP}",
                version(health)
            ),
            code: 0,
        };
    }
    Outcome {
        message: format!(
            "The AIRK service did not start: {}\nIts logs: `airkctl service logs` (or `airkctl service logs --all`).",
            error_message(stderr).unwrap_or_else(|| stderr.trim().to_owned())
        ),
        code: 1,
    }
}

/// Any user-style arguments: nothing is started.
pub fn takes_no_commands(state: Option<&str>) -> Outcome {
    let state = match state {
        Some("running") => "The AIRK service is running.",
        Some(_) => "The AIRK service is not running; run `airk` with no arguments to start it.",
        None => "Its running state is unknown (`airkctl service status` reports it).",
    };
    Outcome {
        message: format!(
            "airk takes no commands: it is the AIRK service. Use airkctl, the AIRK command-line client (`airkctl --help`).\n{state}"
        ),
        code: 2,
    }
}

/// Run the sibling `airkctl` with `--output json` and `arguments`.
fn airkctl(program: &Path, arguments: &[&str]) -> std::io::Result<(i32, String, String)> {
    let output = Command::new(program)
        .args(["--output", "json"])
        .args(arguments)
        .stdin(Stdio::null())
        .output()?;
    Ok((
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

fn missing(program: &Path, error: &std::io::Error) -> Outcome {
    Outcome {
        message: format!(
            "airkctl was not found beside airk ({}: {error}); reinstall AIRK Service.",
            program.display()
        ),
        code: 1,
    }
}

/// §5.1 for `airk` at `airk` with `user_arguments` (empty: start).
pub fn run(airk: &Path, user_arguments: bool) -> Outcome {
    let Some(program) = sibling_airkctl(airk) else {
        return missing(airk, &std::io::Error::other("no program folder"));
    };
    let status = airkctl(&program, &["service", "status"]);
    let status = match status {
        Ok((_, stdout, _)) => serde_json::from_str::<Value>(stdout.trim()).ok(),
        Err(error) if user_arguments => {
            let _ = error;
            None
        }
        Err(error) => return missing(&program, &error),
    };
    if user_arguments {
        let state = status
            .as_ref()
            .and_then(|status| status["service"]["state"].as_str());
        return takes_no_commands(state);
    }
    if let Some(health) = status.as_ref().and_then(running_health) {
        return already_running(health);
    }
    match airkctl(&program, &["service", "start"]) {
        Ok((code, stdout, stderr)) => started(code, &stdout, &stderr),
        Err(error) => missing(&program, &error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn manual_start_messages_follow_section_5_1() {
        let health = json!({"version": "1.9.0", "instance": "6f".repeat(16)});
        let running = already_running(&health);
        assert_eq!(running.code, 0);
        assert!(running.message.contains("already running (version 1.9.0)"));
        assert!(running.message.contains("airkctl service status"));
        assert!(running.message.contains("airkctl service stop"));
        let started_ok = started(
            0,
            &json!({"status": "health", "observation": {"version": "1.9.0"}}).to_string(),
            "",
        );
        assert_eq!(started_ok.code, 0);
        assert!(started_ok.message.contains("has started (version 1.9.0)"));
        assert!(
            started_ok
                .message
                .contains("keeps running after this terminal closes")
        );
        let failed = started(
            3,
            "",
            &format!(
                "{}\n",
                json!({"class": "service_missing", "code": "service_start_failed", "message": "the AIRK service did not become ready within 60 seconds"})
            ),
        );
        assert_ne!(failed.code, 0);
        assert!(
            failed
                .message
                .contains("did not become ready within 60 seconds")
        );
        assert!(failed.message.contains("airkctl service logs"));
        for (state, says) in [
            (Some("running"), "is running"),
            (Some("not_running"), "is not running"),
            (None, "unknown"),
        ] {
            let refused = takes_no_commands(state);
            assert_ne!(refused.code, 0);
            assert!(refused.message.contains("airk takes no commands"));
            assert!(refused.message.contains("airkctl"));
            assert!(refused.message.contains(says), "{}", refused.message);
        }
    }

    #[test]
    fn a_status_document_is_running_only_with_a_running_service() {
        let running = json!({"service": {"state": "running", "health": {"version": "1.9.0"}}});
        assert_eq!(running_health(&running).unwrap()["version"], "1.9.0");
        assert!(running_health(&json!({"service": {"state": "not_running"}})).is_none());
        assert!(running_health(&json!({})).is_none());
    }
}
