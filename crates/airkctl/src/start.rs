// SPDX-License-Identifier: Apache-2.0
//! `airkctl service start` (split step 3b.1d; `docs/split/contracts.md`
//! §3.10.2 and §5.7): start the install record's verified `airk` image as the
//! private `service-detached` role, admitted by an owner-private single-use
//! start request, and wait for its `Health` as the service's own on-demand
//! start does (every 100 ms, at most 60 s).
use crate::contract::{ErrorLine, ExitClass};
use airk_core::{
    identity::Incarnation,
    install_record::{
        MAX_START_REQUEST_LIFETIME_MS, START_REQUEST_SCHEMA, StartMode, StartRequest, canonical,
    },
    service::{ControlMethod, ControlResponse},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long the started service has to answer `Health`.
pub const READINESS: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(100);

/// The outcome of a start: the service's `Health`, and whether this call
/// started it.
pub struct Started {
    pub health: ControlResponse,
    pub started: bool,
}

fn failed(message: impl Into<String>) -> ErrorLine {
    ErrorLine::new(
        ExitClass::ServiceMissing,
        "service_start_failed",
        false,
        &message.into(),
    )
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// The running service's `Health`, `None` when no service answers, or the
/// refusal (for example another version) that must stop the start.
fn health(control: &Path, version: &str) -> Result<Option<ControlResponse>, ErrorLine> {
    match crate::service::request(control, ControlMethod::Health, version) {
        Ok(response @ ControlResponse::Health { .. }) => Ok(Some(response)),
        Ok(other) => Err(crate::service::refusal(&other)),
        Err(line) if line.class == ExitClass::ServiceMissing => Ok(None),
        Err(line) => Err(line),
    }
}

/// Write the owner-private start request (`<control>/start/<nonce>.json`)
/// through a temporary name so the role never reads a partial file.
fn write_request(
    control: &Path,
    nonce: Incarnation,
    now: u64,
    mode: StartMode,
) -> Result<PathBuf, ErrorLine> {
    let folder = control.join("start");
    airk_ipc::endpoint::prepare_root(control)
        .and_then(|()| airk_ipc::endpoint::prepare_root(&folder))
        .map_err(|error| failed(format!("start request folder: {error:#}")))?;
    let request = StartRequest {
        schema: START_REQUEST_SCHEMA.into(),
        nonce,
        mode,
        created_unix_ms: now,
        expires_unix_ms: now + MAX_START_REQUEST_LIFETIME_MS,
    };
    let path = folder.join(format!("{nonce}.json"));
    let temporary = folder.join(format!("{nonce}.json.writing"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options.open(&temporary).and_then(|mut file| {
        use std::io::Write;
        file.write_all(&canonical(&request))?;
        file.sync_all()
    });
    written
        .and_then(|()| std::fs::rename(&temporary, &path))
        .map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            failed(format!("start request: {error}"))
        })?;
    Ok(path)
}

/// A started `airk` the caller can observe and, on failure, stop.
struct Launched {
    #[cfg(windows)]
    child: airk_process::detached::DetachedChild,
    #[cfg(unix)]
    child: std::process::Child,
}

impl Launched {
    fn exited(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.to_string()),
            Ok(None) => None,
            Err(error) => Some(error.to_string()),
        }
    }
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start `airk` detached as the private role; it inherits no handles and no
/// terminal (`airk_process::detached`, U84).
fn launch(airk: &Path, control: &Path, nonce: Incarnation) -> Result<Launched, ErrorLine> {
    let arguments: Vec<std::ffi::OsString> = vec![
        "__airk_role".into(),
        "service-detached".into(),
        "--runtime-dir".into(),
        control.into(),
        "--start-request".into(),
        nonce.to_string().into(),
    ];
    #[cfg(windows)]
    let child = airk_process::detached::spawn_detached(airk, &arguments);
    #[cfg(unix)]
    let child = {
        let mut command = std::process::Command::new(airk);
        command
            .args(&arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        airk_process::detached::new_session(&mut command);
        airk_process::child_group::require_waitable_children()
            .and_then(|()| airk_process::detached::without_error_dialogs(|| command.spawn()))
    };
    child
        .map(|child| Launched { child })
        .map_err(|error| failed(format!("{}: {error}", airk.display())))
}

/// Start the service owning `control` from the verified `airk` image, or
/// return the running service's `Health` (an explicit start, which clears a
/// saved Stop).
pub fn start(airk: &Path, control: &Path, version: &str) -> Result<Started, ErrorLine> {
    start_with(airk, control, version, StartMode::Explicit)
}

/// As [`start`] with the start mode: an automatic start (a command that
/// needs the service, contracts §3.2) honors a saved explicit Stop.
pub fn start_with(
    airk: &Path,
    control: &Path,
    version: &str,
    mode: StartMode,
) -> Result<Started, ErrorLine> {
    if let Some(health) = health(control, version)? {
        return Ok(Started {
            health,
            started: false,
        });
    }
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| failed(format!("start nonce: {error}")))?;
    let nonce = Incarnation::from_bytes(nonce);
    let request = write_request(control, nonce, now_unix_ms(), mode)?;
    let mut launched = match launch(airk, control, nonce) {
        Ok(launched) => launched,
        Err(line) => {
            let _ = std::fs::remove_file(&request);
            return Err(line);
        }
    };
    let deadline = Instant::now() + READINESS;
    let outcome = loop {
        match health(control, version) {
            Ok(Some(health)) => break Ok(health),
            Ok(None) => {}
            Err(line) => break Err(line),
        }
        if let Some(status) = launched.exited() {
            break Err(failed(if mode == StartMode::Automatic {
                format!(
                    "the AIRK service exited ({status}) before it was ready; if it was stopped explicitly, run `airkctl service start`"
                )
            } else {
                format!("the AIRK service exited ({status}) before it was ready")
            }));
        }
        if Instant::now() >= deadline {
            launched.stop();
            break Err(failed(
                "the AIRK service did not become ready within 60 seconds",
            ));
        }
        std::thread::sleep(POLL);
    };
    // The role consumes the request; one that is still present was never used.
    let _ = std::fs::remove_file(&request);
    outcome.map(|health| Started {
        health,
        started: true,
    })
}
