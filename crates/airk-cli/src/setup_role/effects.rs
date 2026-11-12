// SPDX-License-Identifier: Apache-2.0
//! Setup's side effects, each a thin caller of an existing owner:
//! predecessor discovery and receipts-only retirement
//! (`runtime_install::replacement`, U13/U14), the I1 control endpoint for the
//! drain, exact-image process audits (`legacy_processes`), login registration
//! (`service_manager`), user trust (`http_endpoint::trust` over
//! `airk_process::cert_trust`), and the installed `airkctl` for the start
//! (U94: one start implementation).
use super::plan::Host;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{ControlMethod, ControlRequest, ControlResponse, PROTOCOL_VERSION},
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long a stopping service has to exit after it accepted `Shutdown`.
const EXIT: Duration = Duration::from_secs(60);
/// How long open sessions of the installed images have to exit by themselves.
const SESSIONS: Duration = Duration::from_secs(15);

/// Every pre-split location that a chosen location must not overlap (§9.3
/// roster, `user_paths`), both channels, both layout families, plus the
/// pre-split Inno `standalone` folder and the fixed pointer folder.
pub(crate) fn reserved_roots() -> Result<Vec<PathBuf>> {
    use crate::user_paths::{Channel, UserPaths};
    let mut roots = Vec::new();
    for channel in [Channel::Stable, Channel::Preview] {
        let historical = UserPaths::historical(channel)?;
        let discovered = crate::runtime_install::discover_user_paths(channel)?;
        for paths in [historical, discovered] {
            roots.extend([
                paths.install,
                paths.launcher,
                paths.configuration,
                paths.data,
                paths.state,
                paths.cache,
                paths.logs,
            ]);
        }
    }
    if let Some(standalone) = standalone_folder()? {
        roots.push(standalone);
    }
    roots.push(
        super::files::pointer_path()?
            .parent()
            .context("install pointer folder")?
            .to_path_buf(),
    );
    roots.sort();
    roots.dedup();
    Ok(roots)
}

/// The pre-split Inno Setup folder (`installers/windows/airk.iss`).
fn standalone_folder() -> Result<Option<PathBuf>> {
    #[cfg(windows)]
    {
        Ok(Some(
            airk_process::user_folders::known_folder(
                airk_process::user_folders::KnownFolder::LocalAppData,
            )?
            .join("AIRK")
            .join("standalone"),
        ))
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

/// One pre-split installation root found on this machine.
pub(crate) struct Predecessor {
    pub install: PathBuf,
    /// The read-only replacement plan, or why it could not be made.
    pub plan: Result<(bool, Value), String>,
}

/// The pre-split installation roots that exist (both channels, both layout
/// families), each with its read-only replacement plan.
pub(crate) fn predecessors(candidate: &str, operation: &str) -> Result<Vec<Predecessor>> {
    use crate::user_paths::{Channel, UserPaths};
    let mut roots = Vec::new();
    for channel in [Channel::Stable, Channel::Preview] {
        roots.push(UserPaths::historical(channel)?.install);
        roots.push(crate::runtime_install::discover_user_paths(channel)?.install);
    }
    roots.sort();
    roots.dedup();
    Ok(roots
        .into_iter()
        .filter(|root| std::fs::symlink_metadata(root).is_ok())
        .map(|install| {
            let plan = crate::runtime_install::replacement_plan(
                &install,
                candidate,
                Some(operation),
                None,
            )
            .map_err(|error| format!("{error:#}"));
            Predecessor { install, plan }
        })
        .collect())
}

/// Items of the §9.3 roster this build finds but does not retire: reported,
/// never removed (U14).
pub(crate) fn unretired_items() -> Result<Vec<airk_core::install_record::Leftover>> {
    let mut found = Vec::new();
    if let Some(standalone) = standalone_folder()?
        && std::fs::symlink_metadata(&standalone).is_ok()
    {
        found.push(airk_core::install_record::Leftover {
            item: standalone.display().to_string(),
            reason: "the earlier AIRK CLI and MCP server installer folder; removing it through its receipts is not implemented in this build, so it was left. Remove \"AIRK CLI and MCP server\" in Settings > Apps if you no longer use it".into(),
            restarts_old_service: false,
        });
    }
    Ok(found)
}

/// Retire one predecessor through the existing receipts-only replacement.
pub(crate) fn retire(install: &Path, candidate: &str, operation: &str) -> Result<Value> {
    crate::runtime_install::replace_installation(install, candidate, Some(operation), None)
}

/// The retirement operation identity the replacement receipts record: a
/// 64-hex digest bound to this setup operation, stable across resumes.
pub(crate) fn retirement_operation(operation_id: Incarnation) -> String {
    airk_core::identity::framed_digest(b"airk/split-setup-retirement/1", &[operation_id.as_bytes()])
        .to_string()
}

fn missing(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
            matches!(
                io.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            )
        })
    })
}

/// One raw I1 exchange. Unlike `airk_ipc::control::request_once` it accepts
/// a `Health` from another split version, so an upgrade can drain the older
/// service. `None` when nothing listens.
async fn exchange(
    control: &Path,
    method: ControlMethod,
    expected_instance: Option<Incarnation>,
) -> Result<Option<ControlResponse>> {
    if std::fs::symlink_metadata(control).is_err() {
        return Ok(None);
    }
    let address = crate::local_ipc::endpoint(control, "control")?;
    let mut stream =
        match tokio::time::timeout(Duration::from_secs(5), crate::local_ipc::connect(&address))
            .await
        {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) if missing(&error) => return Ok(None),
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                bail!("service_drain_unproven: the control endpoint did not answer within 5 s")
            }
        };
    let (request_budget, response_budget) = (
        method.request_frame_budget(),
        method.response_frame_budget(),
    );
    crate::local_ipc::write_message_limited(
        &mut stream,
        &ControlRequest {
            protocol: PROTOCOL_VERSION,
            expected_instance,
            method,
        },
        Duration::from_secs(5),
        request_budget,
    )
    .await?;
    crate::local_ipc::read_message_limited(&mut stream, Duration::from_secs(10), response_budget)
        .await?
        .context("service disconnected before its control response")
        .map(Some)
}

/// Stop the service owning `control`, if one answers, with exact-image
/// evidence: it must be this user's service running one of `images` (the
/// SHA-256 it reports of its own executable); then `Shutdown` fenced to its
/// instance and the exit of its process. A name or PID alone never
/// authorizes anything. Returns the outcome code and detail.
pub(crate) async fn drain_service(
    control: &Path,
    images: &[Sha256Digest],
) -> Result<(&'static str, String)> {
    let Some(response) = exchange(control, ControlMethod::Health, None).await? else {
        return Ok((
            "not_running",
            format!("no service answers at {}", control.display()),
        ));
    };
    let ControlResponse::Health { observation } = response else {
        bail!(
            "service_drain_unproven: the service at {} did not answer Health",
            control.display()
        );
    };
    ensure!(
        observation.user == crate::local_ipc::current_user()?,
        "service_drain_unproven: the service at {} belongs to another user",
        control.display()
    );
    let image: Sha256Digest = observation
        .executable_sha256
        .parse()
        .context("service_drain_unproven: the service reported no executable digest")?;
    ensure!(
        images.contains(&image),
        "service_drain_unproven: the service at {} runs an image ({image}) this installation did not place; stop it with its own airkctl, then retry",
        control.display()
    );
    match exchange(control, ControlMethod::Shutdown, Some(observation.instance)).await? {
        Some(ControlResponse::Stopping) => {}
        // It exited between the two requests.
        None => {}
        Some(other) => bail!(
            "service_drain_refused: the service answered Shutdown with {}",
            serde_json::to_string(&other).unwrap_or_default()
        ),
    }
    let exited = airk_process::detached::wait_for_exit(observation.process_id, EXIT)
        .context("service_drain_unproven: the stopping process could not be observed")?;
    ensure!(
        exited,
        "service_drain_timeout: the AIRK service (process {}) did not exit within 60 seconds",
        observation.process_id
    );
    Ok((
        "stopped",
        format!(
            "service {} version {} (process {}, image {image}) stopped and exited",
            observation.instance, observation.version, observation.process_id
        ),
    ))
}

/// Wait for every process running exactly one of `images` (except this
/// process) to exit by itself; open `airkctl` sessions keep `airkctl.exe`
/// locked on Windows (§9.6.2). Nothing is terminated.
pub(crate) fn await_sessions(images: &[PathBuf]) -> Result<String> {
    let own = std::process::id();
    let deadline = Instant::now() + SESSIONS;
    loop {
        let mut open = Vec::new();
        for image in images.iter().filter(|image| image.is_file()) {
            let audit = crate::legacy_processes::audit(image);
            ensure!(
                audit.complete,
                "process_observation_incomplete: processes running {} could not all be observed ({})",
                image.display(),
                audit.failure.as_deref().unwrap_or("unobserved")
            );
            open.extend(
                audit
                    .matching_executable_pids
                    .into_iter()
                    .filter(|pid| *pid != own)
                    .map(|pid| format!("{} (process {pid})", image.display())),
            );
        }
        if open.is_empty() {
            return Ok(format!("{} installed image(s) not running", images.len()));
        }
        ensure!(
            Instant::now() < deadline,
            "installation_files_in_use: close the programs using AIRK (VS Code windows with the AIRK extension, open airkctl sessions) and retry: {}",
            open.join(", ")
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Register login startup for the installation (`service-login` role).
/// Returns the phase, outcome and detail; a failure is recorded, not raised.
pub(crate) fn register_login(
    host: Host,
    state: &Path,
    airk: &Path,
    install_id: Incarnation,
) -> (airk_core::install_record::SetupPhase, &'static str, String) {
    use airk_core::install_record::SetupPhase;
    if host == Host::Macos {
        return (
            SetupPhase::LoginUnsupported,
            "login_unsupported",
            "SMAppService registration of AIRK.app is not implemented in this build; start the service with `airkctl service start`".into(),
        );
    }
    let result = (|| -> Result<&'static str> {
        let manager = crate::service_manager::ServiceManager::for_installation(
            state,
            airk,
            &install_id.to_string(),
        )?;
        if manager.owned_registration_exists()? {
            return Ok("already_registered");
        }
        manager.register()?;
        Ok("registered")
    })();
    match result {
        Ok(outcome) => (
            SetupPhase::LoginRegistered,
            outcome,
            airk.display().to_string(),
        ),
        Err(error) => (
            SetupPhase::LoginFailed,
            "login_failed",
            format!("{error:#}"),
        ),
    }
}

/// Remove the installation's own login registration; `Err` leaves it.
pub(crate) fn unregister_login(
    host: Host,
    state: &Path,
    airk: &Path,
    install_id: Incarnation,
) -> Result<&'static str> {
    if host == Host::Macos {
        return Ok("none_registered");
    }
    let manager = crate::service_manager::ServiceManager::for_installation(
        state,
        airk,
        &install_id.to_string(),
    )?;
    if !manager.owned_registration_exists()? {
        return Ok("absent");
    }
    manager.uninstall()?;
    Ok("removed")
}

/// Install the service's HTTPS issuer into this user's trust store (the OS
/// shows its consent dialog). `None` when the issuer does not exist yet.
pub(crate) fn apply_trust(state: &Path) -> Result<Option<String>> {
    let Ok((binding, endpoint)) = crate::http_endpoint::trust::saved_identity(state) else {
        return Ok(None);
    };
    let status = crate::http_endpoint::trust::manage(
        &state.join("http-1"),
        binding,
        endpoint.ca_sha256,
        crate::http_endpoint::trust::Action::Install,
    )?;
    Ok(Some(format!(
        "{} (CA SHA-256 {})",
        status.outcome, status.ca_sha256
    )))
}

/// Remove trust this installation added, through its ownership receipt.
pub(crate) fn remove_trust(state: &Path) -> Result<String> {
    Ok(match crate::http_endpoint::trust::remove_owned(state)? {
        Some(status) => format!("{} (CA SHA-256 {})", status.outcome, status.ca_sha256),
        None => "none_owned".into(),
    })
}

/// Start the installed service through the installed `airkctl` (U94).
/// Returns the `Health` line it printed.
pub(crate) fn start(airkctl: &Path) -> Result<String> {
    let output = std::process::Command::new(airkctl)
        .args(["--output", "json", "service", "start"])
        .stdin(std::process::Stdio::null())
        .output()
        .with_context(|| format!("service_start_failed: {}", airkctl.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
        .and_then(|line| line["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| stderr.trim().to_owned());
    bail!("service_not_healthy: {message}")
}

/// Delete one chosen location's contents (`purge`): never a folder that
/// contains the program folder or the pointer folder. `keep` names direct
/// entries to leave (the state folder keeps its `setup` receipts).
pub(crate) fn purge(location: &Path, protected: &[PathBuf], keep: &[&str]) -> Result<usize> {
    if std::fs::symlink_metadata(location).is_err() {
        return Ok(0);
    }
    super::files::no_links(location)?;
    for other in protected {
        ensure!(
            !other.starts_with(location),
            "purge_refused: {} contains {}",
            location.display(),
            other.display()
        );
    }
    let mut removed = 0;
    for entry in std::fs::read_dir(location)? {
        let entry = entry?;
        if keep
            .iter()
            .any(|name| entry.file_name() == std::ffi::OsStr::new(name))
        {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
        removed += 1;
    }
    if keep.is_empty() {
        std::fs::remove_dir(location)?;
    }
    Ok(removed)
}
