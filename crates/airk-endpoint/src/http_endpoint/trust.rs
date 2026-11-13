// SPDX-License-Identifier: Apache-2.0
//! Explicit per-user trust enrollment. No request path invokes this adapter.
//! The OS trust adapter and receipt engine live in `airk_process::cert_trust`
//! (split U90, `scripts/dev/split_u90b_cert_trust.py`); these are `airk`'s
//! callers, with its record files, PowerShell path and rotation guard.
use super::{InstallationBinding, read_private, write_private};
use airk_core::identity::Sha256Digest;
use airk_process::cert_trust as engine;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
};

mod rotation_receipts;
pub use rotation_receipts::{
    abort_rotation_new, commit_rotation_receipt, finalize_rotation_old, snapshot_rotation_receipt,
};

pub use engine::Action;
#[cfg(test)]
use engine::ps_string;
use engine::{Host, LIMIT, Phase, RECEIPT, Runner};
type Receipt = engine::Receipt<InstallationBinding, Sha256Digest>;
pub type TrustStatus = engine::TrustStatus<Sha256Digest>;

const PORTS: engine::Ports<InstallationBinding> = engine::Ports {
    files: engine::Files {
        read: read_private,
        write: write_private,
        regular: airk_host::install_files::safe_file,
    },
    powershell: windows_program,
    validate_binding: crate::http_endpoint::validate_binding,
    active_guard,
};

/// Serialize ordinary active-issuer mutations with Prepare/Commit before the
/// receipt lock: the verified control root, the rotation lock and no pending
/// rotation, in that order.
fn active_guard(control: &Path) -> Result<Box<dyn std::any::Any>> {
    airk_host::install_layout::validate_control_root(control)?;
    let lock = super::rotation::lock(control)?;
    super::rotation::require_trust_change_allowed(control)?;
    Ok(Box::new(lock))
}

pub fn public_ca(root: &Path, expected: Sha256Digest) -> Result<Vec<u8>> {
    engine::public_ca(&PORTS.files, root, &expected.to_string())
}

fn windows_program() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        airk_host::native_command::windows_tool("WindowsPowerShell/v1.0/powershell.exe")
    }
    #[cfg(all(not(windows), test))]
    {
        Ok(PathBuf::from("/fictional/powershell"))
    }
    #[cfg(all(not(windows), not(test)))]
    {
        bail!("Windows trust commands unavailable")
    }
}

/// Read the protected endpoint against the existing native namespace without
/// starting a service or generating a replacement identity.
pub fn saved_identity(
    control: &Path,
) -> Result<(
    InstallationBinding,
    airk_core::service_http::EndpointDescriptor,
)> {
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
    let binding = super::installation_binding(control, registry.incarnation()?)?;
    let endpoint = super::load_descriptor(&control.join("http-1"), &binding)?
        .context("saved HTTPS endpoint is unavailable; preserve trust metadata for review")?;
    public_ca(&control.join("http-1"), endpoint.ca_sha256)?;
    Ok((binding, endpoint))
}

/// Uninstall never adopts or removes a preexisting issuer. Unknown mutation
/// outcomes stop removal so the exact receipt and public CA remain recoverable.
pub fn remove_owned(control: &Path) -> Result<Option<TrustStatus>> {
    remove_owned_with(control, &mut |root, binding, sha| {
        manage(root, binding, sha, Action::Remove)
    })
}

fn remove_owned_with(
    control: &Path,
    remove: &mut dyn FnMut(&Path, InstallationBinding, Sha256Digest) -> Result<TrustStatus>,
) -> Result<Option<TrustStatus>> {
    super::rotation::require_cleanup_allowed(control)?;
    let root = control.join("http-1");
    match std::fs::symlink_metadata(&root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        result => {
            result?;
        }
    }
    let Some(bytes) = read_private(&root, RECEIPT, LIMIT)? else {
        return Ok(None);
    };
    let receipt: Receipt = serde_json::from_slice(&bytes)?;
    let (binding, endpoint) = saved_identity(control)?;
    ensure!(
        receipt.schema == 1
            && receipt.binding == binding
            && receipt.ca_sha256 == endpoint.ca_sha256,
        "changed uninstall trust ownership receipt"
    );
    ensure!(
        matches!(receipt.phase, Phase::Installed | Phase::Removed),
        "user trust removal outcome unknown; preserve issuer and receipt for review"
    );
    let status = remove(&root, binding, endpoint.ca_sha256)?;
    ensure!(
        matches!(status.outcome, "removed" | "already_absent"),
        "user trust cleanup did not prove removal; preserve issuer and receipt for review"
    );
    Ok(Some(status))
}

/// Call only after verified native Health or saved endpoint binding. Install/Remove are
/// explicit setup actions, separate from MCP enrollment and evidence reads.
pub fn manage(
    root: &Path,
    binding: InstallationBinding,
    ca_sha256: Sha256Digest,
    action: Action,
) -> Result<TrustStatus> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        manage_with(
            root,
            binding,
            ca_sha256,
            action,
            Host::current(),
            &mut |program, args| {
                let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                airk_host::native_command::execute_native_observation(program, &refs)
            },
        )
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        manage_with(
            root,
            binding,
            ca_sha256,
            action,
            Host::current(),
            &mut |_, _| bail!("unsupported user trust store"),
        )
    }
}

fn manage_with(
    root: &Path,
    binding: InstallationBinding,
    ca_sha256: Sha256Digest,
    action: Action,
    host: Host,
    run: &mut Runner<'_>,
) -> Result<TrustStatus> {
    engine::manage_with(&PORTS, root, binding, ca_sha256, action, host, run)
}

#[cfg(test)]
mod tests;
