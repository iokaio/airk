// SPDX-License-Identifier: Apache-2.0
//! Upgrade from a build that served loopback over HTTPS (local trust plan C,
//! decided 23 November 2026). That build created an installation CA in
//! `<control>/http-1`, and setup could install it in this user's OS trust
//! store, recording the ownership receipt `user-trust-1.json` beside it.
//!
//! This remove-only retirer is the one survivor of that machinery:
//! - a receipt in phase `installed` names the exact certificate, by DER
//!   SHA-256, that is removed from the store the receipt names (Windows
//!   CurrentUser\Root, the macOS login keychain); nothing is ever added;
//! - a receipt in phase `prepared` or `remove_prepared` records an
//!   interrupted change of unknown outcome: it is reported as residue and
//!   neither it nor `ca.pem` is touched;
//! - a receipt of another user, or one that cannot be read, is residue too;
//! - Linux never had trust: only files are deleted.
//!
//! Afterwards the TLS files are deleted by exact name inside the folder, each
//! inspected with `symlink_metadata` and never followed: `ca.pem`,
//! `tls-identity.json`, `retired-private-key.json`, `user-trust-1.json`,
//! `user-trust-1.lock`, `issuer-rotation.json`, `issuer-rotation.lock` and
//! `rotation-user-trust-preimage.json`. A pending rotation journal is
//! discarded. The rotation history folder `issuer-rotations` is reported, not
//! deleted. Setup runs this in the user's session, never the headless
//! service, because macOS may ask the user before removing trust.
use airk_process::cert_trust::{self as engine, Host, LegacyReceipt, Phase, RECEIPT, Runner};
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

/// Files of the HTTPS identity and trust, deleted once trust is settled.
const TRUST_FILES: [&str; 3] = ["ca.pem", RECEIPT, "user-trust-1.lock"];
const TLS_FILES: [&str; 5] = [
    "tls-identity.json",
    "retired-private-key.json",
    "issuer-rotation.json",
    "issuer-rotation.lock",
    "rotation-user-trust-preimage.json",
];

/// What one retirement did and left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LegacyTrust {
    /// The CA removed from the OS store, with the outcome (`removed` or
    /// `already_absent`).
    pub certificate: Option<(String, &'static str)>,
    /// Files deleted, by name.
    pub deleted: Vec<&'static str>,
    /// What was left for review, and why.
    pub residue: Vec<String>,
}

impl LegacyTrust {
    /// One line for the setup receipt.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        match &self.certificate {
            Some((sha, outcome)) => parts.push(format!("CA SHA-256 {sha}: {outcome}")),
            None => parts.push("no owned CA".to_owned()),
        }
        if !self.deleted.is_empty() {
            parts.push(format!("deleted {}", self.deleted.join(", ")));
        }
        for item in &self.residue {
            parts.push(format!("left: {item}"));
        }
        parts.join("; ")
    }
}

/// Retire the HTTPS trust of the installation at `control` on this host.
pub fn retire(control: &Path) -> Result<LegacyTrust> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        retire_with(control, Host::current(), &mut |program, args| {
            let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            airk_host::native_command::execute_native_observation(program, &refs)
        })
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        retire_with(control, Host::current(), &mut |_, _| {
            anyhow::bail!("unsupported user trust store")
        })
    }
}

fn powershell() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        airk_host::native_command::windows_tool("WindowsPowerShell/v1.0/powershell.exe")
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows trust commands unavailable")
    }
}

/// Delete the regular file `root/name` if it exists; links and directories
/// stay.
fn delete(root: &Path, name: &'static str, deleted: &mut Vec<&'static str>) -> Result<()> {
    let path = root.join(name);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            std::fs::remove_file(&path)?;
            deleted.push(name);
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

/// `retire` with the host and the trust command runner supplied (tests).
pub fn retire_with(control: &Path, host: Host, run: &mut Runner<'_>) -> Result<LegacyTrust> {
    let root = control.join("http-1");
    let mut report = LegacyTrust::default();
    match std::fs::symlink_metadata(&root) {
        Ok(metadata) => ensure!(
            metadata.file_type().is_dir(),
            "HTTP private folder is not a directory"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(error) => return Err(error.into()),
    }
    let mut keep_trust = false;
    let receipt = match crate::http_endpoint::read_private(&root, RECEIPT, engine::LIMIT) {
        Ok(bytes) => bytes,
        Err(error) => {
            report
                .residue
                .push(format!("{RECEIPT}: unreadable ({error:#})"));
            keep_trust = true;
            None
        }
    };
    if let Some(bytes) = receipt {
        let user = crate::local_ipc::current_user()?;
        match LegacyReceipt::decode(&bytes) {
            Err(error) => {
                report
                    .residue
                    .push(format!("{RECEIPT}: unreadable ({error:#})"));
                keep_trust = true;
            }
            Ok(receipt) if receipt.user() != Some(user.as_str()) => {
                report
                    .residue
                    .push(format!("{RECEIPT}: names another user"));
                keep_trust = true;
            }
            Ok(receipt) => match receipt.phase {
                Phase::Prepared | Phase::RemovePrepared => {
                    report.residue.push(format!(
                        "{RECEIPT}: an interrupted trust change of unknown outcome for CA SHA-256 {}; inspect the user trust store",
                        receipt.ca_sha256
                    ));
                    keep_trust = true;
                }
                Phase::Removed => {}
                Phase::Installed => match engine::legacy_store(host)? {
                    None => {
                        // No per-user trust ever existed on this host.
                    }
                    Some(store) => {
                        ensure!(
                            receipt.store == store,
                            "user trust receipt names another store; preserve it for review"
                        );
                        let outcome = engine::retire_exact(
                            host,
                            powershell,
                            &store,
                            &receipt.ca_sha256,
                            run,
                        )
                        .context("the old AIRK CA could not be removed from the user trust store; its receipt and ca.pem are kept")?;
                        report.certificate = Some((receipt.ca_sha256.clone(), outcome));
                    }
                },
            },
        }
    }
    if !keep_trust {
        for name in TRUST_FILES {
            delete(&root, name, &mut report.deleted)?;
        }
    }
    for name in TLS_FILES {
        delete(&root, name, &mut report.deleted)?;
    }
    if std::fs::symlink_metadata(root.join("issuer-rotations")).is_ok() {
        report
            .residue
            .push("issuer-rotations: old rotation history, not read".to_owned());
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
