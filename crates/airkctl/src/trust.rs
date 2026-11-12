// SPDX-License-Identifier: Apache-2.0
//! `service trust inspect|install|remove` (split step 3e.5, U75, U90): this
//! installation's HTTPS issuer in the current user's OS trust store, without
//! the service and without opening an index, registry or store. It runs the
//! receipt engine and OS adapter `airk runtime http-trust` runs
//! (`airk_process::cert_trust`) on the same owner-private files under
//! `<control>/http-1` (`airk_ipc::private_record`), so the printed status is
//! the same document; the OS consent dialog appears for this process.
//!
//! Two inputs differ from `airk` because `airkctl` may not open the registry
//! or the pre-split install layout (U75): the namespace is the endpoint
//! descriptor's own, bound to the canonical control root and this user (as
//! `agents credential` binds the ledger), and the active-issuer guard checks
//! the owner-private control root and the saved rotation's phase instead of
//! `install_layout::validate_control_root` and the full rotation record.
use crate::contract::{ErrorLine, ExitClass};
use airk_core::{
    identity::Sha256Digest,
    service_http::{EndpointDescriptor, MAX_ENDPOINT_BYTES},
    service_http_credentials::InstallationBinding,
};
use airk_ipc::private_record;
use airk_process::cert_trust::{self as engine, Action, Host};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::path::{Path, PathBuf};

const PORTS: engine::Ports<InstallationBinding> = engine::Ports {
    files: engine::Files {
        read: private_record::read_private,
        write: private_record::write_private,
        regular: private_record::require_regular_unlinked,
    },
    powershell,
    validate_binding,
    active_guard,
};

fn powershell() -> Result<PathBuf> {
    airk_process::bounded_command::windows_tool("WindowsPowerShell/v1.0/powershell.exe")
}

/// `airk`'s `http_endpoint::validate_binding`: the binding belongs to this user.
fn validate_binding(binding: &InstallationBinding) -> Result<()> {
    binding.require_user(&airk_ipc::endpoint::current_user()?)
}

/// Before the receipt lock, as `airk` orders it: the owner-only control root,
/// the issuer-rotation lock, then no pending rotation (`airk`'s
/// `rotation::require_trust_change_allowed` message).
fn active_guard(control: &Path) -> Result<Box<dyn std::any::Any>> {
    airk_ipc::endpoint::require_existing_root(control)?;
    let lock = private_record::lock(
        &control.join("http-1"),
        "issuer-rotation.lock",
        "issuer rotation already active",
    )?;
    #[derive(serde::Deserialize)]
    struct Saved {
        phase: String,
    }
    if let Some(bytes) =
        private_record::read_private(&control.join("http-1"), "issuer-rotation.json", 32 * 1024)?
    {
        let saved: Saved = serde_json::from_slice(&bytes)?;
        ensure!(
            matches!(saved.phase.as_str(), "finalized" | "aborted"),
            "use the pending issuer rotation before changing active trust"
        );
    }
    Ok(Box::new(lock))
}

/// `airk`'s `trust::saved_identity` with the descriptor's own namespace: the
/// saved endpoint bound to this control root and user, and its public issuer.
fn saved_identity(control: &Path) -> Result<(InstallationBinding, EndpointDescriptor)> {
    let root = control.join("http-1");
    let bytes = private_record::read_private(&root, "endpoint.json", MAX_ENDPOINT_BYTES)?
        .context("saved HTTPS endpoint is unavailable; preserve trust metadata for review")?;
    let endpoint = EndpointDescriptor::decode(&bytes)?;
    let user = airk_ipc::endpoint::current_user()?;
    let binding = InstallationBinding::derive(&control.canonicalize()?, &user, endpoint.namespace)?;
    validate_binding(&binding)?;
    ensure!(
        endpoint.installation == binding.installation
            && endpoint.user == binding.user
            && endpoint.namespace == binding.namespace,
        "HTTP endpoint belongs to a different installation"
    );
    engine::public_ca(&PORTS.files, &root, &endpoint.ca_sha256.to_string())?;
    Ok((binding, endpoint))
}

fn manage(
    root: &Path,
    binding: InstallationBinding,
    ca: Sha256Digest,
    action: Action,
) -> Result<Value> {
    #[cfg(any(windows, target_os = "macos"))]
    let mut run = |program: &Path, args: &[String]| {
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        airk_process::bounded_command::execute_native_observation(program, &refs)
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut run = |_: &Path, _: &[String]| -> Result<(bool, String)> {
        anyhow::bail!("unsupported user trust store")
    };
    let status = engine::manage_with(&PORTS, root, binding, ca, action, Host::current(), &mut run)?;
    Ok(serde_json::to_value(status)?)
}

fn run(control: &Path, action: Action, expected: Option<Sha256Digest>) -> Result<Value> {
    ensure!(
        action == Action::Inspect || expected.is_some(),
        "install/remove requires the reviewed issuer SHA256"
    );
    let (binding, endpoint) = saved_identity(control)?;
    ensure!(
        expected.is_none_or(|reviewed| reviewed == endpoint.ca_sha256),
        "native HTTPS issuer changed after review"
    );
    manage(&control.join("http-1"), binding, endpoint.ca_sha256, action)
}

/// `service trust <action> [--expected-ca-sha256 <sha>]`: the trust status
/// `airk runtime http-trust` printed; install and remove require the reviewed
/// issuer digest. Errors carry the full chain, as `airk`'s `operation_failed`.
pub fn trust(
    control: &Path,
    action: Action,
    expected_ca_sha256: Option<Sha256Digest>,
) -> Result<Value, ErrorLine> {
    run(control, action, expected_ca_sha256).map_err(|error| {
        ErrorLine::new(
            ExitClass::Refused,
            "operation_failed",
            false,
            &format!("{error:#}"),
        )
    })
}
