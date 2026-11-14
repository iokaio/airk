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
//! §5.7). `airk` resolves its verified I4 pair and renders the CLI's JSON;
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

/// The sibling CLI of an explicit uninstalled development layout.
pub fn sibling_airkctl(airk: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "airkctl.exe"
    } else {
        "airkctl"
    };
    airk.parent().map(|folder| folder.join(name))
}

fn resolve_airkctl(
    airk: &Path,
    discovery: airk_ipc::install_reader::Discovery,
    development: bool,
) -> Result<PathBuf, String> {
    use airk_ipc::install_reader::Discovery;
    match discovery {
        Discovery::Verified(installed) => {
            let own = airk.canonicalize().map_err(|error| format!("cannot identify this airk image: {error}"))?;
            let expected = installed.airk.canonicalize().map_err(|error| format!("cannot identify the installed airk image: {error}"))?;
            if own != expected || installed.record.version != env!("CARGO_PKG_VERSION") {
                return Err("This airk image is not the verified installed service. Run the airk executable named by the AIRK Service installation.".into());
            }
            Ok(installed.airkctl)
        }
        Discovery::Unverified { code, message } => Err(format!("AIRK Service installation could not be verified ({code}): {message}")),
        Discovery::NotFound if development && airk.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new("debug")) => {
            sibling_airkctl(airk).ok_or_else(|| "The development image has no program folder.".into())
        }
        Discovery::NotFound => Err("AIRK Service is not installed for this image. Use the AIRK Service installer or the portable setup instructions.".into()),
    }
}

fn installed_pair() -> airk_ipc::install_reader::Discovery {
    use airk_core::install_record::Signing;
    use airk_process::signature::Claim;
    let pointer = match airk_process::user_folders::install_pointer() {
        Ok(pointer) => pointer,
        Err(error) => {
            return airk_ipc::install_reader::Discovery::Unverified {
                code: "installation_unverified".into(),
                message: format!("{error:#}"),
            };
        }
    };
    airk_ipc::install_reader::read(&pointer, |signing, images| {
        let claim = match signing {
            Signing::Windows {
                publisher,
                identity_eku,
            } => Claim::Windows {
                publisher,
                identity_eku,
            },
            Signing::Macos { team_id } => Claim::Macos { team_id },
            Signing::Linux { signed, integrity } => Claim::Linux {
                signed: *signed,
                integrity,
            },
        };
        airk_process::signature::verify(claim, images)
            .map(|_| ())
            .map_err(|refusal| (refusal.code.to_owned(), refusal.message))
    })
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

/// Run the verified paired `airkctl` with `--output json` and `arguments`.
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
            "The paired airkctl could not be run ({}: {error}); repair AIRK Service.",
            program.display()
        ),
        code: 1,
    }
}

/// §5.1 for `airk` at `airk` with `user_arguments` (empty: start).
pub fn run(airk: &Path, user_arguments: bool) -> Outcome {
    let program = match resolve_airkctl(airk, installed_pair(), cfg!(debug_assertions)) {
        Ok(program) => program,
        Err(_) if user_arguments => return takes_no_commands(None),
        Err(message) => return Outcome { message, code: 1 },
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
mod pair_tests;

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
