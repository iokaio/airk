// SPDX-License-Identifier: Apache-2.0
//! Remove-only retirement of the per-user trust an HTTPS build of AIRK
//! installed (local trust plan C, decided 23 November 2026): Windows
//! CurrentUser Root through PowerShell, the macOS login keychain through
//! security(1). It reads that build's ownership receipt and removes exactly
//! the certificate the receipt names, by DER SHA-256; nothing is ever added
//! and no request path invokes it. The install and inspect engine it
//! replaces (split U90) was removed with loopback TLS.
use anyhow::{Result, bail, ensure};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The ownership receipt beside the old issuer, and its byte bound.
pub const RECEIPT: &str = "user-trust-1.json";
pub const LIMIT: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Prepared,
    Installed,
    RemovePrepared,
    Removed,
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
}

pub fn ps_string(value: &str) -> Result<String> {
    ensure!(
        value.len() <= 8192 && !value.chars().any(char::is_control),
        "invalid trust argument"
    );
    Ok(format!("'{}'", value.replace('\'', "''")))
}

pub type Runner<'a> = dyn FnMut(&Path, &[String]) -> Result<(bool, String)> + 'a;

fn command(run: &mut Runner<'_>, program: &Path, args: &[String]) -> Result<String> {
    let (success, text) = run(program, args)?;
    ensure!(text.len() <= 65_536, "trust command exceeded output bound");
    ensure!(success, "user trust command failed: {text}");
    Ok(text)
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
    fn powershell_arguments_are_quoted() {
        assert_eq!(ps_string("a'b").unwrap(), "'a''b'");
        assert!(ps_string("a\rb").is_err());
    }
}
