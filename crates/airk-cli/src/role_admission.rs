// SPDX-License-Identifier: Apache-2.0
//! Private role admission for `airk` (split plan §5.1; `docs/split/contracts.md`
//! §5.7; step 3.0).
//!
//! A private role is selected before command parsing with
//! `airk __airk_role <name> …`. A role starts only after its authority is
//! verified, before any state is opened. A listed role whose step has not
//! landed is refused (`role_not_available`), and a bare or unknown selector is
//! refused (`role_admission_refused`). Phase 7 serves `setup` and
//! `service-login` (`setup_role`).
use airk_core::{
    identity::Incarnation,
    install_record::{MAX_START_REQUEST_BYTES, StartMode, StartRequest},
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
};

/// The pre-parse selector for private roles.
pub const SELECTOR: &str = "__airk_role";
/// U93: the interim role that reaches the pre-split operations with Phase 7
/// or developer owners; the `airk` binary parses its arguments.
pub const INTERIM: &str = "setup-interim";
/// Roles named by the contract, in the order of `fixtures/split/i4/role_admission.json`.
pub const ROLES: [&str; 7] = [
    "service-detached",
    "service-foreground",
    "setup-interim",
    "service-login",
    "pdf-child",
    "record-error",
    "setup",
];

/// An admitted role with its verified-syntax inputs.
#[derive(Debug, PartialEq, Eq)]
pub enum RoleRequest {
    ServiceDetached {
        runtime_dir: PathBuf,
        nonce: Incarnation,
    },
    /// Phase 4: the service in the foreground on an explicit control root,
    /// for development, tests and an external supervisor (was `airk serve`).
    ServiceForeground {
        runtime_dir: PathBuf,
        idle_exit_seconds: Option<u64>,
    },
    /// U75: append one client error while the service is down (step 3b.2).
    RecordError {
        runtime_dir: PathBuf,
        operation: String,
    },
    /// Phase 7 (U33): run one setup operation file (`setup_role`).
    Setup(crate::setup_role::SetupRequest),
    /// Phase 7: the login registration's entry (`setup_role::serve_login`).
    ServiceLogin { install_id: Incarnation },
}

/// Classify the process arguments (including the program name). `None` means
/// the arguments do not select a private role.
pub fn parse(arguments: &[OsString]) -> Option<Result<RoleRequest>> {
    if arguments.get(1).map(OsString::as_os_str) != Some(SELECTOR.as_ref()) {
        return None;
    }
    Some(parse_role(&arguments[2..]))
}

fn parse_role(arguments: &[OsString]) -> Result<RoleRequest> {
    let name = arguments
        .first()
        .and_then(|name| name.to_str())
        .context("role_admission_refused: a private role name is required")?;
    match name {
        "service-detached" => {
            let mut runtime_dir = None;
            let mut nonce = None;
            let mut rest = arguments[1..].iter();
            while let Some(flag) = rest.next() {
                let value = rest
                    .next()
                    .context("role_admission_refused: missing role argument value")?;
                match flag.to_str() {
                    Some("--runtime-dir") if runtime_dir.is_none() => {
                        runtime_dir = Some(PathBuf::from(value));
                    }
                    Some("--start-request") if nonce.is_none() => {
                        let value = value
                            .to_str()
                            .context("role_admission_refused: invalid start request")?;
                        nonce = Some(
                            value
                                .parse::<Incarnation>()
                                .context("role_admission_refused: invalid start request")?,
                        );
                    }
                    _ => bail!("role_admission_refused: unexpected role argument"),
                }
            }
            let runtime_dir =
                runtime_dir.context("role_admission_refused: --runtime-dir is required")?;
            ensure!(
                runtime_dir.is_absolute(),
                "role_admission_refused: --runtime-dir must be absolute"
            );
            Ok(RoleRequest::ServiceDetached {
                runtime_dir,
                nonce: nonce.context("role_admission_refused: --start-request is required")?,
            })
        }
        "service-foreground" => {
            let mut runtime_dir = None;
            let mut idle_exit_seconds = None;
            let mut rest = arguments[1..].iter();
            while let Some(flag) = rest.next() {
                let value = rest
                    .next()
                    .context("role_admission_refused: missing role argument value")?;
                match flag.to_str() {
                    Some("--runtime-dir") if runtime_dir.is_none() => {
                        runtime_dir = Some(PathBuf::from(value));
                    }
                    Some("--idle-exit-seconds") if idle_exit_seconds.is_none() => {
                        let seconds = value
                            .to_str()
                            .and_then(|value| value.parse::<u64>().ok())
                            .filter(|seconds| (1..=3600).contains(seconds))
                            .context("role_admission_refused: invalid --idle-exit-seconds")?;
                        idle_exit_seconds = Some(seconds);
                    }
                    _ => bail!("role_admission_refused: unexpected role argument"),
                }
            }
            let runtime_dir =
                runtime_dir.context("role_admission_refused: --runtime-dir is required")?;
            ensure!(
                runtime_dir.is_absolute(),
                "role_admission_refused: --runtime-dir must be absolute"
            );
            Ok(RoleRequest::ServiceForeground {
                runtime_dir,
                idle_exit_seconds,
            })
        }
        "record-error" => {
            let mut runtime_dir = None;
            let mut operation = None;
            let mut rest = arguments[1..].iter();
            while let Some(flag) = rest.next() {
                let value = rest
                    .next()
                    .context("role_admission_refused: missing role argument value")?;
                match flag.to_str() {
                    Some("--runtime-dir") if runtime_dir.is_none() => {
                        runtime_dir = Some(PathBuf::from(value));
                    }
                    Some("--operation") if operation.is_none() => {
                        operation = Some(
                            value
                                .to_str()
                                .context("role_admission_refused: invalid operation")?
                                .to_owned(),
                        );
                    }
                    _ => bail!("role_admission_refused: unexpected role argument"),
                }
            }
            let runtime_dir =
                runtime_dir.context("role_admission_refused: --runtime-dir is required")?;
            ensure!(
                runtime_dir.is_absolute(),
                "role_admission_refused: --runtime-dir must be absolute"
            );
            Ok(RoleRequest::RecordError {
                runtime_dir,
                operation: operation.context("role_admission_refused: --operation is required")?,
            })
        }
        "setup" => {
            let mut operation = None;
            let mut mode = None;
            let mut rest = arguments[1..].iter();
            while let Some(flag) = rest.next() {
                match flag.to_str() {
                    Some("--operation") if operation.is_none() => {
                        operation = Some(PathBuf::from(
                            rest.next()
                                .context("role_admission_refused: missing role argument value")?,
                        ));
                    }
                    Some("--plan") if mode.is_none() => {
                        mode = Some(crate::setup_role::SetupMode::Plan);
                    }
                    Some("--prepare") if mode.is_none() => {
                        mode = Some(crate::setup_role::SetupMode::Prepare);
                    }
                    _ => bail!("role_admission_refused: unexpected role argument"),
                }
            }
            let operation = operation.context("role_admission_refused: --operation is required")?;
            ensure!(
                operation.is_absolute(),
                "role_admission_refused: --operation must be absolute"
            );
            Ok(RoleRequest::Setup(crate::setup_role::SetupRequest {
                operation,
                mode: mode.unwrap_or(crate::setup_role::SetupMode::Complete),
            }))
        }
        "service-login" => {
            let mut install_id = None;
            let mut rest = arguments[1..].iter();
            while let Some(flag) = rest.next() {
                let value = rest
                    .next()
                    .context("role_admission_refused: missing role argument value")?;
                match flag.to_str() {
                    Some("--install-id") if install_id.is_none() => {
                        install_id = Some(
                            value
                                .to_str()
                                .and_then(|value| value.parse::<Incarnation>().ok())
                                .context("role_admission_refused: invalid --install-id")?,
                        );
                    }
                    _ => bail!("role_admission_refused: unexpected role argument"),
                }
            }
            Ok(RoleRequest::ServiceLogin {
                install_id: install_id
                    .context("role_admission_refused: --install-id is required")?,
            })
        }
        // The binary dispatches the interim role before role parsing.
        INTERIM => bail!("role_admission_refused: {INTERIM} is dispatched by the airk binary"),
        name if ROLES.contains(&name) => bail!("role_not_available: {name}"),
        _ => bail!("role_admission_refused: unknown private role"),
    }
}

/// The start-request file for `nonce` under a control root.
pub fn start_request_path(runtime_dir: &Path, nonce: Incarnation) -> PathBuf {
    runtime_dir.join("start").join(format!("{nonce}.json"))
}

/// Atomically take the single-use start request: rename it out of the way so a
/// second consumer finds nothing, check ownership, read it, then delete it.
/// The request is consumed even when it then fails validation.
pub fn consume_start_request(
    runtime_dir: &Path,
    nonce: Incarnation,
    now_unix_ms: u64,
) -> Result<StartMode> {
    let path = start_request_path(runtime_dir, nonce);
    let taken = path.with_extension("json.consumed");
    std::fs::rename(&path, &taken)
        .context("role_admission_refused: start request missing or already consumed")?;
    let result = (|| -> Result<StartMode> {
        require_owner_private(&taken)?;
        let mut bytes = Vec::new();
        std::fs::File::open(&taken)?
            .take(MAX_START_REQUEST_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        let request = StartRequest::decode(&bytes, nonce, now_unix_ms)
            .map_err(|refusal| anyhow::anyhow!("role_admission_refused: {}", refusal.code()))?;
        Ok(request.mode)
    })();
    let removed = std::fs::remove_file(&taken);
    let mode = result?;
    removed.context("role_admission_refused: start request could not be removed")?;
    Ok(mode)
}

/// The platform owner-private file check (`airk-ipc`, U84).
fn require_owner_private(path: &Path) -> Result<()> {
    crate::local_ipc::require_owner_private(path)
        .context("role_admission_refused: start request is not owner-private")
}

fn now_unix_ms() -> Result<u64> {
    Ok(u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

/// Run an admitted role.
pub async fn run(request: RoleRequest) -> Result<()> {
    match request {
        RoleRequest::ServiceDetached { runtime_dir, nonce } => {
            let mode = consume_start_request(&runtime_dir, nonce, now_unix_ms()?)?;
            // The saved Stop and any pending lifecycle record are settled once
            // the service holds its single-instance lease (step 3b.1e-2).
            crate::service_runtime::serve_admitted(&runtime_dir, mode).await
        }
        RoleRequest::ServiceForeground {
            runtime_dir,
            idle_exit_seconds,
        } => {
            // The single-instance lease and the owner-only control root are
            // the service's own admission, exactly as `airk serve` had them.
            crate::service_runtime::serve_with_idle(
                &runtime_dir,
                idle_exit_seconds.map(std::time::Duration::from_secs),
            )
            .await
        }
        RoleRequest::RecordError {
            runtime_dir,
            operation,
        } => {
            // The control root must already be this user's owner-only root.
            crate::local_ipc::require_existing_root(&runtime_dir)
                .context("role_admission_refused: control root is not owner-only")?;
            let mut input = Vec::new();
            std::io::stdin()
                .take(airk_core::service::MAX_CLIENT_ERROR_INPUT_BYTES as u64 + 1)
                .read_to_end(&mut input)?;
            ensure!(
                input.len() <= airk_core::service::MAX_CLIENT_ERROR_INPUT_BYTES,
                "client error input exceeds 16 KiB"
            );
            crate::service_logs::record_client_error(&runtime_dir, &operation, &input)
        }
        RoleRequest::Setup(request) => crate::setup_role::run(request).await,
        RoleRequest::ServiceLogin { install_id } => {
            crate::setup_role::serve_login(install_id).await
        }
    }
}

#[cfg(test)]
mod tests;
