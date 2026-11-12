// SPDX-License-Identifier: Apache-2.0
//! Explicit per-user trust enrollment. No request path invokes this adapter.
use super::{InstallationBinding, digest, read_private, write_private};
use airk_core::identity::Sha256Digest;
use anyhow::{Context, Result, bail, ensure};
use rustls::pki_types::{CertificateDer, pem::PemObject};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
};

mod rotation_receipts;
pub(crate) use rotation_receipts::{
    abort_rotation_new, commit_rotation_receipt, finalize_rotation_old, snapshot_rotation_receipt,
};

const RECEIPT: &str = "user-trust-1.json";
const LIMIT: usize = 16_384;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Inspect,
    Install,
    Remove,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    Installed,
    RemovePrepared,
    Removed,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    binding: InstallationBinding,
    ca_sha256: Sha256Digest,
    store: PathBuf,
    phase: Phase,
}

#[derive(Serialize)]
pub struct TrustStatus {
    pub schema: u32,
    pub ca_sha256: Sha256Digest,
    pub platform: &'static str,
    pub store: Option<PathBuf>,
    pub present: bool,
    pub trusted: bool,
    pub owned: bool,
    pub outcome: &'static str,
    pub guidance: &'static str,
}

#[derive(Clone, Copy)]
enum Host {
    Windows,
    Macos,
    Unsupported,
}
impl Host {
    fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Unsupported
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Unsupported => "unsupported",
        }
    }
}

fn public_ca(root: &Path, expected: Sha256Digest) -> Result<Vec<u8>> {
    let pem =
        read_private(root, "ca.pem", 8192)?.context("native HTTPS issuer is not provisioned")?;
    ensure!(
        !pem.windows(11).any(|part| part == b"PRIVATE KEY"),
        "private key in public trust material"
    );
    let certs = CertificateDer::pem_slice_iter(&pem).collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        certs.len() == 1 && digest(certs[0].as_ref()) == expected,
        "native HTTPS issuer digest changed"
    );
    Ok(certs[0].as_ref().to_vec())
}

fn ps_string(value: &str) -> Result<String> {
    ensure!(
        value.len() <= 8192 && !value.chars().any(char::is_control),
        "invalid trust argument"
    );
    Ok(format!("'{}'", value.replace('\'', "''")))
}

// The script selects certificate objects by DER SHA256, never by subject name,
// a caller supplied thumbprint, or a whole-machine trust store.
const WINDOWS: &str = r#"& { param($Action,$Expected,$File)
$ErrorActionPreference='Stop'; $s=New-Object System.Security.Cryptography.X509Certificates.X509Store('Root','CurrentUser');
$s.Open($(if($Action -eq 'inspect'){[System.Security.Cryptography.X509Certificates.OpenFlags]::ReadOnly}else{[System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite}));
try {
 $sha=[System.Security.Cryptography.SHA256]::Create();
 $found=@($s.Certificates | Where-Object { ([BitConverter]::ToString($sha.ComputeHash($_.RawData))).Replace('-','').ToLowerInvariant() -ceq $Expected });
 if($found.Count -gt 1){throw 'ambiguous issuer entries'}
 if($Action -eq 'install') {
  if($found.Count -ne 0){throw 'issuer appeared before installation'}
  $c=[System.Security.Cryptography.X509Certificates.X509Certificate2]::new($File);
  if(([BitConverter]::ToString($sha.ComputeHash($c.RawData))).Replace('-','').ToLowerInvariant() -cne $Expected){throw 'issuer file changed'}
  $s.Add($c);
 } elseif($Action -eq 'remove') {
  if($found.Count -ne 1){throw 'issuer disappeared before removal'}
  $s.Remove($found[0]);
 } elseif($Action -ne 'inspect'){throw 'invalid trust action'}
 $found=@($s.Certificates | Where-Object { ([BitConverter]::ToString($sha.ComputeHash($_.RawData))).Replace('-','').ToLowerInvariant() -ceq $Expected });
 if($found.Count -gt 1){throw 'ambiguous issuer entries'}
 @{present=($found.Count -eq 1);trusted=($found.Count -eq 1)} | ConvertTo-Json -Compress
} finally {$s.Close()}
}"#;

type Runner<'a> = dyn FnMut(&Path, &[String]) -> Result<(bool, String)> + 'a;

fn command(run: &mut Runner<'_>, program: &Path, args: &[String]) -> Result<String> {
    let (success, text) = run(program, args)?;
    ensure!(text.len() <= 65_536, "trust command exceeded output bound");
    ensure!(success, "user trust command failed: {text}");
    Ok(text)
}

fn windows_program() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        crate::native_command::windows_tool("WindowsPowerShell/v1.0/powershell.exe")
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
fn observe(
    host: Host,
    store: &Path,
    ca: &Path,
    expected: Sha256Digest,
    run: &mut Runner<'_>,
) -> Result<(bool, bool)> {
    match host {
        Host::Windows => {
            let program = windows_program()?;
            let script = format!(
                "{WINDOWS} 'inspect' {} {}",
                ps_string(&expected.to_string())?,
                ps_string(&ca.to_string_lossy())?
            );
            let text = command(
                run,
                &program,
                &[
                    "-NoLogo".into(),
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    script,
                ],
            )?;
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Observation {
                present: bool,
                trusted: bool,
            }
            let value: Observation = serde_json::from_str(text.trim())?;
            Ok((value.present, value.trusted))
        }
        Host::Macos => {
            let text = command(
                run,
                Path::new("/usr/bin/security"),
                &[
                    "find-certificate".into(),
                    "-a".into(),
                    "-p".into(),
                    store.to_string_lossy().into_owned(),
                ],
            )?;
            let certificates = CertificateDer::pem_slice_iter(text.as_bytes())
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let count = certificates
                .iter()
                .filter(|der| digest(der.as_ref()) == expected)
                .count();
            ensure!(count <= 1, "ambiguous issuer entries");
            if count == 0 {
                return Ok((false, false));
            }
            let (trusted, text) = run(
                Path::new("/usr/bin/security"),
                &[
                    "verify-cert".into(),
                    "-c".into(),
                    ca.to_string_lossy().into_owned(),
                    "-k".into(),
                    store.to_string_lossy().into_owned(),
                    "-p".into(),
                    "basic".into(),
                    "-R".into(),
                    "offline".into(),
                ],
            )?;
            ensure!(
                text.len() <= 65_536,
                "trust verification exceeded output bound"
            );
            Ok((true, trusted))
        }
        Host::Unsupported => bail!("unsupported user trust store"),
    }
}

fn mutate(
    host: Host,
    store: &Path,
    ca: &Path,
    expected: Sha256Digest,
    install: bool,
    run: &mut Runner<'_>,
) -> Result<()> {
    match host {
        Host::Windows => {
            let program = windows_program()?;
            let script = format!(
                "{WINDOWS} '{}' {} {}",
                if install { "install" } else { "remove" },
                ps_string(&expected.to_string())?,
                ps_string(&ca.to_string_lossy())?
            );
            command(
                run,
                &program,
                &[
                    "-NoLogo".into(),
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    script,
                ],
            )?;
        }
        Host::Macos => {
            if install {
                command(
                    run,
                    Path::new("/usr/bin/security"),
                    &[
                        "add-trusted-cert".into(),
                        "-r".into(),
                        "trustRoot".into(),
                        "-k".into(),
                        store.to_string_lossy().into_owned(),
                        ca.to_string_lossy().into_owned(),
                    ],
                )?;
            } else {
                // Actual security(1) help confirms -Z accepts SHA256 and -t
                // removes USER trust settings. No transaction atomicity assumed;
                // a failure retains RemovePrepared for explicit recovery review.
                command(
                    run,
                    Path::new("/usr/bin/security"),
                    &[
                        "delete-certificate".into(),
                        "-t".into(),
                        "-Z".into(),
                        expected.to_string(),
                        store.to_string_lossy().into_owned(),
                    ],
                )?;
            }
        }
        Host::Unsupported => bail!("unsupported user trust store"),
    }
    Ok(())
}

/// Read the protected endpoint against the existing native namespace without
/// starting a service or generating a replacement identity.
pub(crate) fn saved_identity(
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
pub(crate) fn remove_owned(control: &Path) -> Result<Option<TrustStatus>> {
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
                crate::native_command::execute_native_observation(program, &refs)
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
    crate::http_endpoint::validate_binding(&binding)?;
    // Serialize ordinary active-issuer mutations with Prepare/Commit before
    // taking the receipt lock. Staged roots are already owned by that rotation.
    let _rotation_lock =
        if action != Action::Inspect && root.file_name() == Some(std::ffi::OsStr::new("http-1")) {
            let control = root
                .parent()
                .context("active issuer control root missing")?;
            crate::install_layout::validate_control_root(control)?;
            let lock = super::rotation::lock(control)?;
            super::rotation::require_trust_change_allowed(control)?;
            Some(lock)
        } else {
            None
        };
    let _der = public_ca(root, ca_sha256)?;
    let mut status = TrustStatus {
        schema: 1,
        ca_sha256,
        platform: host.name(),
        store: None,
        present: false,
        trusted: false,
        owned: false,
        outcome: "unsupported",
        guidance: "Use a qualified client-specific additive CA bundle; this platform has no supported per-user OS trust installation. Never disable certificate validation.",
    };
    if matches!(host, Host::Unsupported) {
        return Ok(status);
    }
    let store = match host {
        Host::Windows => PathBuf::from("CurrentUser/Root"),
        Host::Macos => crate::user_paths::home()?.join("Library/Keychains/login.keychain-db"),
        Host::Unsupported => unreachable!(),
    };
    if matches!(host, Host::Macos) {
        crate::install_files::safe_file(&store)?;
    }
    status.store = Some(store.clone());
    status.guidance = "Normal TLS validation is required. Verify the actual MCP host connection; an already running host may retain cached trust. Retry its connection after installing the reviewed issuer.";
    // Inspect never creates a file or mutates OS trust.
    let _lock = if action == Action::Inspect {
        None
    } else {
        if read_private(root, "user-trust-1.lock", 0)?.is_none()
            && let Err(error) = write_private(root, "user-trust-1.lock", b"", 0, false)
            && read_private(root, "user-trust-1.lock", 0)?.is_none()
        {
            return Err(error);
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("user-trust-1.lock"))?;
        lock.try_lock()
            .context("user trust setup is already active")?;
        Some(lock)
    };
    let before = read_private(root, RECEIPT, LIMIT)?;
    let mut receipt = if let Some(bytes) = &before {
        let receipt: Receipt = serde_json::from_slice(bytes)?;
        ensure!(
            receipt.schema == 1
                && receipt.binding == binding
                && receipt.ca_sha256 == ca_sha256
                && receipt.store == store,
            "changed user trust ownership receipt"
        );
        Some(receipt)
    } else {
        None
    };
    let ca = root.join("ca.pem");
    let (present, trusted) = observe(host, &store, &ca, ca_sha256, run)?;
    status.present = present;
    status.trusted = trusted;
    status.owned = receipt
        .as_ref()
        .is_some_and(|r| r.phase == Phase::Installed);
    if receipt
        .as_ref()
        .is_some_and(|r| matches!(r.phase, Phase::Prepared | Phase::RemovePrepared))
    {
        status.outcome = "outcome_unknown";
        status.guidance = "A prior trust mutation was interrupted. Preserve the issuer and receipt, inspect the user trust store, and review recovery before retrying.";
        return Ok(status);
    }
    if action == Action::Inspect {
        status.outcome = if present && trusted {
            "trusted"
        } else {
            "not_trusted"
        };
        return Ok(status);
    }
    if action == Action::Install && present {
        ensure!(
            trusted,
            "preexisting issuer is untrusted; review user trust policy without replacing it"
        );
        status.outcome = "already_trusted";
        return Ok(status);
    }
    if action == Action::Remove && !status.owned {
        ensure!(
            !present,
            "refusing to remove a preexisting or unowned issuer"
        );
        status.outcome = "already_absent";
        return Ok(status);
    }
    if action == Action::Remove {
        ensure!(
            present && trusted,
            "owned issuer or trust policy changed; review before removal"
        );
    }
    ensure!(
        read_private(root, RECEIPT, LIMIT)? == before,
        "user trust receipt changed during review"
    );
    public_ca(root, ca_sha256)?;
    let mut next = receipt.take().unwrap_or(Receipt {
        schema: 1,
        binding,
        ca_sha256,
        store,
        phase: Phase::Prepared,
    });
    next.phase = if action == Action::Install {
        Phase::Prepared
    } else {
        Phase::RemovePrepared
    };
    write_private(
        root,
        RECEIPT,
        &serde_json::to_vec(&next)?,
        LIMIT,
        before.is_some(),
    )?;
    mutate(
        host,
        &next.store,
        &ca,
        ca_sha256,
        action == Action::Install,
        run,
    )?;
    public_ca(root, ca_sha256)?;
    let (present, trusted) = observe(host, &next.store, &ca, ca_sha256, run)?;
    ensure!(
        if action == Action::Install {
            present && trusted
        } else {
            !present
        },
        "user trust mutation could not be verified; ownership receipt retained for review"
    );
    next.phase = if action == Action::Install {
        Phase::Installed
    } else {
        Phase::Removed
    };
    write_private(root, RECEIPT, &serde_json::to_vec(&next)?, LIMIT, true)?;
    status.present = present;
    status.trusted = trusted;
    status.owned = action == Action::Install;
    status.outcome = if status.owned { "installed" } else { "removed" };
    Ok(status)
}

#[cfg(test)]
mod tests;
