// SPDX-License-Identifier: Apache-2.0
//! Setup's side effects, each a thin caller of an existing owner:
//! predecessor discovery and receipts-only retirement
//! (`runtime_install::replacement`, U13/U14), the I1 control endpoint for the
//! drain, exact-image process audits (`legacy_processes`), login registration
//! (`service_manager`), user trust (`http_endpoint::trust` over
//! `airk_process::cert_trust`), and the installed `airkctl` for the start
//! (U94: one start implementation).
use super::plan::Host;
pub(crate) mod program_files;
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
    use airk_host::user_paths::{Channel, UserPaths};
    let mut roots = Vec::new();
    for channel in [Channel::Stable, Channel::Preview] {
        let historical = UserPaths::historical(channel)?;
        let discovered = airk_installer::runtime_install::discover_user_paths(channel)?;
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
    for install in airk_installer::runtime_install::predecessor_install_roots()? {
        roots.extend(airk_host::user_paths::owned_install_locations(&install)?);
        roots.push(install);
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
    let roots = airk_installer::runtime_install::predecessor_install_roots()?;
    Ok(roots
        .into_iter()
        .filter(|root| std::fs::symlink_metadata(root).is_ok())
        .map(|install| {
            let plan = airk_installer::runtime_install::replacement_plan(
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
    super::predecessor_roster::unretired_items()
}

/// Retire one predecessor through the existing receipts-only replacement.
pub(crate) fn retire(install: &Path, candidate: &str, operation: &str) -> Result<Value> {
    airk_installer::runtime_install::replace_installation(install, candidate, Some(operation), None)
}

/// The blocker a running pre-split AIRK editor extension raises for placing
/// operations, or `None`. Such an extension sets the pre-split runtime up
/// again after setup retires it, so setup waits until it is updated or its
/// editor is closed. An incomplete observation is reported, not ignored.
pub(crate) fn old_extension_blocker(version: &str) -> Option<String> {
    let audit =
        airk_installer::legacy_processes::audit_where(&super::plan::pre_split_extension_image);
    if !audit.complete {
        return Some(format!(
            "process_observation_incomplete: running editor extensions could not all be observed ({})",
            audit.failure.as_deref().unwrap_or("unobserved")
        ));
    }
    (!audit.matching_executable_pids.is_empty()).then(|| {
        let pids = audit
            .matching_executable_pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "old_extension_running: VS Code (or another editor) is running an AIRK extension older than 1.8.24 (process {pids}); it would set up the old AIRK again. Update the AIRK extension to {version} or close the editor, then retry"
        )
    })
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
        let mut incomplete = None;
        for image in images.iter().filter(|image| image.is_file()) {
            let audit = airk_installer::legacy_processes::audit(image);
            if !audit.complete && incomplete.is_none() {
                incomplete = Some(format!(
                    "processes running {} could not all be observed ({})",
                    image.display(),
                    audit.failure.as_deref().unwrap_or("unobserved")
                ));
            }
            open.extend(
                audit
                    .matching_executable_pids
                    .into_iter()
                    .filter(|pid| *pid != own)
                    .map(|pid| format!("{} (process {pid})", image.display())),
            );
        }
        match session_wait(&open, incomplete.as_deref(), Instant::now() >= deadline) {
            SessionWait::Done => {
                return Ok(format!("{} installed image(s) not running", images.len()));
            }
            SessionWait::Refuse(message) => bail!(message),
            SessionWait::Observe => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SessionWait {
    Done,
    Observe,
    Refuse(String),
}

/// Pure: one observation of the installed images during the drain. Done only
/// on a complete scan that finds none running; until the deadline anything
/// else is observed again (processes exit during the drain, and one that
/// exits mid-scan can leave that scan incomplete); at the deadline a running
/// image is reported first because closing it is the user's action.
fn session_wait(open: &[String], incomplete: Option<&str>, expired: bool) -> SessionWait {
    match (open.is_empty(), incomplete, expired) {
        (true, None, _) => SessionWait::Done,
        (_, _, false) => SessionWait::Observe,
        (false, _, true) => SessionWait::Refuse(format!(
            "installation_files_in_use: programs still use AIRK; update the AIRK extension in VS Code (an extension older than 1.8.30 does not release AIRK during setup) or close VS Code, end open airkctl sessions, and retry: {}",
            open.join(", ")
        )),
        (true, Some(detail), true) => {
            SessionWait::Refuse(format!("process_observation_incomplete: {detail}"))
        }
    }
}

/// Register login startup for the installation (`service-login` role).
/// Returns the phase, outcome and detail; a failure is recorded, not raised.
pub(crate) fn register_login(
    host: Host,
    state: &Path,
    airk: &Path,
    install_id: Incarnation,
    operation: Option<&Path>,
) -> (airk_core::install_record::SetupPhase, &'static str, String) {
    use airk_core::install_record::SetupPhase;
    if host == Host::Macos {
        return match super::mac_login::apply(airk, install_id, "register", operation) {
            Ok(airk_installer::service_manager::MacLoginState::Enabled) => (
                SetupPhase::LoginRegistered,
                "registered",
                airk.display().to_string(),
            ),
            Ok(airk_installer::service_manager::MacLoginState::RequiresApproval) => (
                SetupPhase::LoginFailed,
                "login_consent_required",
                "Approve AIRK in System Settings Login Items, then resume the original setup"
                    .into(),
            ),
            Ok(state) => (
                SetupPhase::LoginFailed,
                "login_failed",
                format!("SMAppService registration is {state:?}"),
            ),
            Err(error) => (
                SetupPhase::LoginFailed,
                "login_failed",
                format!("{error:#}"),
            ),
        };
    }
    let result = (|| -> Result<&'static str> {
        let manager = airk_installer::service_manager::ServiceManager::for_installation(
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
    operation: Option<&Path>,
) -> Result<&'static str> {
    if host == Host::Macos {
        return match super::mac_login::apply(airk, install_id, "unregister", operation)? {
            airk_installer::service_manager::MacLoginState::NotRegistered
            | airk_installer::service_manager::MacLoginState::NotFound => Ok("removed"),
            state => bail!("login_startup_failed: SMAppService remains {state:?}"),
        };
    }
    let manager = airk_installer::service_manager::ServiceManager::for_installation(
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
    let Ok((binding, endpoint)) = airk_endpoint::http_endpoint::trust::saved_identity(state) else {
        return Ok(None);
    };
    let status = airk_endpoint::http_endpoint::trust::manage(
        &state.join("http-1"),
        binding,
        endpoint.ca_sha256,
        airk_endpoint::http_endpoint::trust::Action::Install,
    )?;
    Ok(Some(format!(
        "{} (CA SHA-256 {})",
        status.outcome, status.ca_sha256
    )))
}

/// Remove trust this installation added, through its ownership receipt.
pub(crate) fn remove_trust(state: &Path) -> Result<String> {
    Ok(
        match airk_endpoint::http_endpoint::trust::remove_owned(state)? {
            Some(status) => format!("{} (CA SHA-256 {})", status.outcome, status.ca_sha256),
            None => "none_owned".into(),
        },
    )
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
#[cfg(test)]
pub(crate) fn purge(location: &Path, protected: &[PathBuf], keep: &[&str]) -> Result<usize> {
    if std::fs::symlink_metadata(location).is_err() {
        return Ok(0);
    }
    let held = airk_process::directory::OpenedDirectory::open(location)?;
    purge_held(&held, location, protected, keep)
}

pub(crate) fn purge_held(
    held: &airk_process::directory::OpenedDirectory,
    location: &Path,
    protected: &[PathBuf],
    keep: &[&str],
) -> Result<usize> {
    for other in protected {
        ensure!(
            !other.starts_with(location),
            "purge_refused: {} contains {}",
            location.display(),
            other.display()
        );
    }
    #[cfg(unix)]
    let removed = rooted_purge::purge(held.as_file(), keep)?;
    #[cfg(windows)]
    let removed = {
        let current = airk_process::directory::OpenedDirectory::open(location)?;
        ensure!(
            current.identity()? == held.identity()?,
            "purge_refused: root identity changed"
        );
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
        removed
    };
    // The transaction binds these root objects before any destructive effect.
    // Purge their contents while preserving the observed root identity for
    // interruption recovery and settlement; state additionally keeps setup/.
    held.sync_all()?;
    Ok(removed)
}

#[cfg(unix)]
mod rooted_purge;

#[cfg(test)]
mod session_wait_tests {
    use super::{SessionWait, session_wait};

    #[test]
    fn an_incomplete_scan_is_observed_again_until_the_deadline() {
        let open = vec!["airkctl.exe (process 7)".to_string()];
        assert_eq!(session_wait(&[], None, false), SessionWait::Done);
        assert_eq!(session_wait(&[], None, true), SessionWait::Done);
        assert_eq!(
            session_wait(&[], Some("unobserved"), false),
            SessionWait::Observe
        );
        assert_eq!(session_wait(&open, None, false), SessionWait::Observe);
        assert_eq!(
            session_wait(&open, Some("unobserved"), false),
            SessionWait::Observe
        );
        let SessionWait::Refuse(message) = session_wait(&[], Some("unobserved"), true) else {
            panic!("an incomplete scan at the deadline is refused");
        };
        assert!(
            message.starts_with("process_observation_incomplete: unobserved"),
            "{message}"
        );
        for incomplete in [None, Some("unobserved")] {
            let SessionWait::Refuse(message) = session_wait(&open, incomplete, true) else {
                panic!("a running image at the deadline is refused");
            };
            assert!(
                message.starts_with("installation_files_in_use:") && message.contains("process 7"),
                "{message}"
            );
        }
    }
}
