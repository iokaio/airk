// SPDX-License-Identifier: Apache-2.0
//! Explicit per-user trust of AIRK's local HTTPS issuer (split U90): the OS
//! trust-store adapter (Windows CurrentUser Root through PowerShell, the
//! macOS login keychain through security(1)) and the durable ownership
//! receipt engine, shared by `airk` and `airkctl`'s offline trust (U75).
//! No request path invokes it. Moved from `airk-cli`
//! (`http_endpoint/trust.rs`) by `scripts/dev/split_u90b_cert_trust.py`.
use anyhow::{Context, Result, bail, ensure};
use rustls::pki_types::{CertificateDer, pem::PemObject};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fmt::Write as _,
    fs::OpenOptions,
    path::{Path, PathBuf},
};

/// The ownership receipt beside the issuer, and its byte bound.
pub const RECEIPT: &str = "user-trust-1.json";
pub const LIMIT: usize = 16_384;

/// Owner-private record access and the regular-file check, supplied by the
/// caller from `airk_ipc::private_record` (this crate links no AIRK crate).
/// Read `name` under `root` within a byte bound; `None` when absent.
pub type ReadRecord = fn(&Path, &str, usize) -> Result<Option<Vec<u8>>>;

#[derive(Clone, Copy)]
pub struct Files {
    pub read: ReadRecord,
    pub write: fn(&Path, &str, &[u8], usize, bool) -> Result<()>,
    pub regular: fn(&Path) -> Result<()>,
}

/// What the receipt engine needs from its caller: the record files, the
/// Windows PowerShell path, the check that the binding belongs to this user,
/// and the guard taken before mutating the active issuer (`<control>/http-1`):
/// validate the control root, take the issuer-rotation lock and refuse a
/// pending rotation, returning the held lock.
pub struct Ports<B> {
    pub files: Files,
    pub powershell: fn() -> Result<PathBuf>,
    pub validate_binding: fn(&B) -> Result<()>,
    pub active_guard: fn(&Path) -> Result<Box<dyn std::any::Any>>,
}

/// Lowercase hexadecimal SHA-256, the `Display` spelling of AIRK's digests.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut text = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Inspect,
    Install,
    Remove,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Prepared,
    Installed,
    RemovePrepared,
    Removed,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt<B, D> {
    pub schema: u32,
    pub binding: B,
    pub ca_sha256: D,
    pub store: PathBuf,
    pub phase: Phase,
}

#[derive(Serialize)]
pub struct TrustStatus<D> {
    pub schema: u32,
    pub ca_sha256: D,
    pub platform: &'static str,
    pub store: Option<PathBuf>,
    pub present: bool,
    pub trusted: bool,
    pub owned: bool,
    pub outcome: &'static str,
    pub guidance: &'static str,
}

#[derive(Clone, Copy)]
pub enum Host {
    Windows,
    Macos,
    Unsupported,
}
impl Host {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Unsupported
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Unsupported => "unsupported",
        }
    }
}

pub fn public_ca(files: &Files, root: &Path, expected: &str) -> Result<Vec<u8>> {
    let pem =
        (files.read)(root, "ca.pem", 8192)?.context("native HTTPS issuer is not provisioned")?;
    ensure!(
        !pem.windows(11).any(|part| part == b"PRIVATE KEY"),
        "private key in public trust material"
    );
    let certs = CertificateDer::pem_slice_iter(&pem).collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        certs.len() == 1 && sha256_hex(certs[0].as_ref()) == expected,
        "native HTTPS issuer digest changed"
    );
    Ok(certs[0].as_ref().to_vec())
}

pub fn ps_string(value: &str) -> Result<String> {
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

pub type Runner<'a> = dyn FnMut(&Path, &[String]) -> Result<(bool, String)> + 'a;

fn command(run: &mut Runner<'_>, program: &Path, args: &[String]) -> Result<String> {
    let (success, text) = run(program, args)?;
    ensure!(text.len() <= 65_536, "trust command exceeded output bound");
    ensure!(success, "user trust command failed: {text}");
    Ok(text)
}

pub fn observe(
    host: Host,
    powershell: fn() -> Result<PathBuf>,
    store: &Path,
    ca: &Path,
    expected: &str,
    run: &mut Runner<'_>,
) -> Result<(bool, bool)> {
    match host {
        Host::Windows => {
            let program = powershell()?;
            let script = format!(
                "{WINDOWS} 'inspect' {} {}",
                ps_string(expected)?,
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
                .filter(|der| sha256_hex(der.as_ref()) == expected)
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

pub fn mutate(
    host: Host,
    powershell: fn() -> Result<PathBuf>,
    store: &Path,
    ca: &Path,
    expected: &str,
    install: bool,
    run: &mut Runner<'_>,
) -> Result<()> {
    match host {
        Host::Windows => {
            let program = powershell()?;
            let script = format!(
                "{WINDOWS} '{}' {} {}",
                if install { "install" } else { "remove" },
                ps_string(expected)?,
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

/// Inspect, install or remove the exact issuer at `root/ca.pem` in this
/// user's OS trust store, with the durable ownership receipt beside it.
pub fn manage_with<B, D>(
    ports: &Ports<B>,
    root: &Path,
    binding: B,
    ca_sha256: D,
    action: Action,
    host: Host,
    run: &mut Runner<'_>,
) -> Result<TrustStatus<D>>
where
    B: PartialEq + Serialize + DeserializeOwned,
    D: Copy + PartialEq + std::fmt::Display + Serialize + DeserializeOwned,
{
    (ports.validate_binding)(&binding)?;
    // Serialize ordinary active-issuer mutations with Prepare/Commit before
    // taking the receipt lock. Staged roots are already owned by that rotation.
    let _rotation_lock =
        if action != Action::Inspect && root.file_name() == Some(std::ffi::OsStr::new("http-1")) {
            let control = root
                .parent()
                .context("active issuer control root missing")?;
            Some((ports.active_guard)(control)?)
        } else {
            None
        };
    let _der = public_ca(&ports.files, root, &ca_sha256.to_string())?;
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
        Host::Macos => crate::user_folders::home()?.join("Library/Keychains/login.keychain-db"),
        Host::Unsupported => unreachable!(),
    };
    if matches!(host, Host::Macos) {
        (ports.files.regular)(&store)?;
    }
    status.store = Some(store.clone());
    status.guidance = "Normal TLS validation is required. Verify the actual MCP host connection; an already running host may retain cached trust. Retry its connection after installing the reviewed issuer.";
    // Inspect never creates a file or mutates OS trust.
    let _lock = if action == Action::Inspect {
        None
    } else {
        if (ports.files.read)(root, "user-trust-1.lock", 0)?.is_none()
            && let Err(error) = (ports.files.write)(root, "user-trust-1.lock", b"", 0, false)
            && (ports.files.read)(root, "user-trust-1.lock", 0)?.is_none()
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
    let before = (ports.files.read)(root, RECEIPT, LIMIT)?;
    let mut receipt = if let Some(bytes) = &before {
        let receipt: Receipt<B, D> = serde_json::from_slice(bytes)?;
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
    let (present, trusted) = observe(
        host,
        ports.powershell,
        &store,
        &ca,
        &ca_sha256.to_string(),
        run,
    )?;
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
        (ports.files.read)(root, RECEIPT, LIMIT)? == before,
        "user trust receipt changed during review"
    );
    public_ca(&ports.files, root, &ca_sha256.to_string())?;
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
    (ports.files.write)(
        root,
        RECEIPT,
        &serde_json::to_vec(&next)?,
        LIMIT,
        before.is_some(),
    )?;
    mutate(
        host,
        ports.powershell,
        &next.store,
        &ca,
        &ca_sha256.to_string(),
        action == Action::Install,
        run,
    )?;
    public_ca(&ports.files, root, &ca_sha256.to_string())?;
    let (present, trusted) = observe(
        host,
        ports.powershell,
        &next.store,
        &ca,
        &ca_sha256.to_string(),
        run,
    )?;
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
    (ports.files.write)(root, RECEIPT, &serde_json::to_vec(&next)?, LIMIT, true)?;
    status.present = present;
    status.trusted = trusted;
    status.owned = action == Action::Install;
    status.outcome = if status.owned { "installed" } else { "removed" };
    Ok(status)
}

/// Local trust plan C: the ownership receipt an HTTPS build left, read
/// tolerantly. Its binding is kept as JSON; the caller checks its user.
#[derive(Deserialize)]
pub struct LegacyReceipt {
    pub schema: u32,
    pub binding: serde_json::Value,
    pub ca_sha256: String,
    pub store: PathBuf,
    pub phase: Phase,
}

impl LegacyReceipt {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= LIMIT, "user trust receipt exceeds bound");
        let receipt: Self = serde_json::from_slice(bytes)?;
        ensure!(receipt.schema == 1, "unsupported user trust receipt");
        ensure!(
            receipt.ca_sha256.len() == 64
                && receipt
                    .ca_sha256
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
            "user trust receipt names no exact certificate digest"
        );
        Ok(receipt)
    }

    /// The user the receipt's binding names, if any.
    pub fn user(&self) -> Option<&str> {
        self.binding.get("user").and_then(serde_json::Value::as_str)
    }
}

/// The per-user store an HTTPS build installed into on `host`.
pub fn legacy_store(host: Host) -> Result<Option<PathBuf>> {
    Ok(match host {
        Host::Windows => Some(PathBuf::from("CurrentUser/Root")),
        Host::Macos => {
            Some(crate::user_folders::home()?.join("Library/Keychains/login.keychain-db"))
        }
        Host::Unsupported => None,
    })
}

// Remove-only: select the one CurrentUser\Root entry whose DER SHA-256 is
// the receipt's, remove it, and prove it is gone. Never by subject name or
// thumbprint, never the machine store.
const WINDOWS_RETIRE: &str = r#"& { param($Expected)
$ErrorActionPreference='Stop'; $s=New-Object System.Security.Cryptography.X509Certificates.X509Store('Root','CurrentUser');
$s.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite);
try {
 $sha=[System.Security.Cryptography.SHA256]::Create();
 $found=@($s.Certificates | Where-Object { ([BitConverter]::ToString($sha.ComputeHash($_.RawData))).Replace('-','').ToLowerInvariant() -ceq $Expected });
 if($found.Count -gt 1){throw 'ambiguous issuer entries'}
 $outcome='already_absent'; if($found.Count -eq 1){ $s.Remove($found[0]); $outcome='removed' }
 $after=@($s.Certificates | Where-Object { ([BitConverter]::ToString($sha.ComputeHash($_.RawData))).Replace('-','').ToLowerInvariant() -ceq $Expected });
 if($after.Count -ne 0){throw 'issuer still present after removal'}
 @{outcome=$outcome} | ConvertTo-Json -Compress
} finally {$s.Close()}
}"#;

/// How many entries of the macOS keychain `store` have the SHA-256 digest
/// `expected`, from `security find-certificate -a -Z` (its `SHA-256 hash:`
/// lines), with no certificate parsing.
fn macos_count(store: &Path, expected: &str, run: &mut Runner<'_>) -> Result<usize> {
    let text = command(
        run,
        Path::new("/usr/bin/security"),
        &[
            "find-certificate".into(),
            "-a".into(),
            "-Z".into(),
            store.to_string_lossy().into_owned(),
        ],
    )?;
    Ok(text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("SHA-256 hash:"))
        .filter(|hash| hash.trim().eq_ignore_ascii_case(expected))
        .count())
}

/// Remove the exact certificate with DER SHA-256 `expected` from this user's
/// `store` (local trust plan C, upgrade only). `removed` or
/// `already_absent`; a failure or a certificate still present is an error.
/// macOS may ask the user, so this runs in setup, in the user's session.
pub fn retire_exact(
    host: Host,
    powershell: fn() -> Result<PathBuf>,
    store: &Path,
    expected: &str,
    run: &mut Runner<'_>,
) -> Result<&'static str> {
    ensure!(
        expected.len() == 64
            && expected
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
        "invalid certificate digest"
    );
    match host {
        Host::Windows => {
            let program = powershell()?;
            let text = command(
                run,
                &program,
                &[
                    "-NoLogo".into(),
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    format!("{WINDOWS_RETIRE} {}", ps_string(expected)?),
                ],
            )?;
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Outcome {
                outcome: String,
            }
            let value: Outcome = serde_json::from_str(text.trim())?;
            match value.outcome.as_str() {
                "removed" => Ok("removed"),
                "already_absent" => Ok("already_absent"),
                _ => bail!("unexpected trust removal outcome"),
            }
        }
        Host::Macos => {
            let count = macos_count(store, expected, run)?;
            ensure!(count <= 1, "ambiguous issuer entries");
            if count == 0 {
                return Ok("already_absent");
            }
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
            ensure!(
                macos_count(store, expected, run)? == 0,
                "issuer still present after removal"
            );
            Ok("removed")
        }
        Host::Unsupported => bail!("unsupported user trust store"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_retirement_selects_by_digest_and_proves_absence() -> Result<()> {
        let digest = "ab".repeat(32);
        let mut calls = Vec::new();
        let mut run = |program: &Path, args: &[String]| {
            calls.push((program.to_path_buf(), args.to_vec()));
            Ok((true, r#"{"outcome":"removed"}"#.to_owned()))
        };
        let outcome = retire_exact(
            Host::Windows,
            || Ok(PathBuf::from("powershell")),
            Path::new("CurrentUser/Root"),
            &digest,
            &mut run,
        )?;
        assert_eq!(outcome, "removed");
        assert_eq!(calls.len(), 1);
        let script = calls[0].1.last().unwrap();
        assert!(
            script.contains("('Root','CurrentUser')") && script.ends_with(&format!("'{digest}'"))
        );
        assert!(!script.contains("Add("));
        // macOS: found once, deleted with -t -Z by digest, then absent.
        let mut listed = 0;
        let mut calls = Vec::new();
        let mut run = |_: &Path, args: &[String]| {
            calls.push(args.to_vec());
            if args[0] == "find-certificate" {
                listed += 1;
                let text = if listed == 1 {
                    format!("SHA-256 hash: {}\nSHA-1 hash: 00\n", digest.to_uppercase())
                } else {
                    "SHA-256 hash: 00\n".to_owned()
                };
                return Ok((true, text));
            }
            Ok((true, String::new()))
        };
        let store = Path::new("/Users/u/Library/Keychains/login.keychain-db");
        assert_eq!(
            retire_exact(Host::Macos, || bail!("no"), store, &digest, &mut run)?,
            "removed"
        );
        assert_eq!(
            calls[1][..4],
            ["delete-certificate", "-t", "-Z", digest.as_str()]
        );
        assert_eq!(calls.len(), 3);
        // Nothing listed: already absent, nothing deleted.
        let mut deletes = 0;
        let mut run = |_: &Path, args: &[String]| {
            if args[0] == "delete-certificate" {
                deletes += 1;
            }
            Ok((true, String::new()))
        };
        assert_eq!(
            retire_exact(Host::Macos, || bail!("no"), store, &digest, &mut run)?,
            "already_absent"
        );
        assert_eq!(deletes, 0);
        // A noncanonical digest and an unsupported host refuse before running.
        let mut run = |_: &Path, _: &[String]| -> Result<(bool, String)> { panic!("ran") };
        assert!(retire_exact(Host::Windows, || bail!("no"), store, "AB", &mut run).is_err());
        assert!(retire_exact(Host::Unsupported, || bail!("no"), store, &digest, &mut run).is_err());
        Ok(())
    }

    #[test]
    fn digests_use_the_lowercase_hex_spelling_and_arguments_are_quoted() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(ps_string("a'b").unwrap(), "'a''b'");
        assert!(ps_string("a\rb").is_err());
    }
}
