// SPDX-License-Identifier: Apache-2.0
//! Private role admission for `airk` (split plan §5.1; `docs/split/contracts.md`
//! §5.7; step 3.0).
//!
//! A private role is selected before command parsing with
//! `airk __airk_role <name> …`. A role starts only after its authority is
//! verified, before any state is opened. This step serves `service-detached`;
//! every other listed role is refused (`role_not_available`) until its step
//! lands, and a bare or unknown selector is refused (`role_admission_refused`).
//! The existing hidden commands (`serve`, workers, guardians, the PDF child)
//! keep working until Phase 4 retires the user CLI.
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
/// Roles named by the contract, in the order of `fixtures/split/i4/role_admission.json`.
pub const ROLES: [&str; 4] = ["service-detached", "service-login", "pdf-child", "setup"];

/// An admitted role with its verified-syntax inputs.
#[derive(Debug, PartialEq, Eq)]
pub enum RoleRequest {
    ServiceDetached {
        runtime_dir: PathBuf,
        nonce: Incarnation,
    },
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

#[cfg(windows)]
fn require_owner_private(path: &Path) -> Result<()> {
    crate::local_ipc::require_owner_private(path)
        .context("role_admission_refused: start request is not owner-private")
}

#[cfg(unix)]
fn require_owner_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file()
            && metadata.nlink() == 1
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0,
        "role_admission_refused: start request is not owner-private"
    );
    Ok(())
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
            if mode == StartMode::Explicit {
                clear_saved_stop(&runtime_dir)?;
            }
            crate::service_runtime::serve(&runtime_dir).await
        }
    }
}

/// An explicit start clears a saved explicit Stop, as a user's Start does today.
fn clear_saved_stop(runtime_dir: &Path) -> Result<()> {
    if !runtime_dir.join("service-1.4.sqlite3").try_exists()? {
        return Ok(());
    }
    let saved = airk_store::service::registry::ServiceRegistry::open_saved(runtime_dir)?;
    let namespace = saved.incarnation()?;
    drop(saved);
    airk_store::service::registry::ServiceRegistry::with_existing_lifecycle(
        runtime_dir,
        namespace,
        |ledger| ledger.clear_service_stop(),
    )
}

#[cfg(test)]
mod tests;
