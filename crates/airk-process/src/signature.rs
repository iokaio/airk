// SPDX-License-Identifier: Apache-2.0
//! Executable signature checks of the installed pair (`docs/split/contracts.md`
//! §5.4.3; split step 3b.1c-sig), shared by the `airkctl` I4 reader and the
//! `airk` setup role so there is one implementation.
//!
//! The expected identities come from the checked-in C10 file
//! `scripts/signing-identity.json` (contracts §7), embedded at build time;
//! they are never hand-copied. An install record's signing claim must name
//! exactly that identity, for the platform this build runs on:
//!
//! - Windows: `WinVerifyTrust` (`WINTRUST_ACTION_GENERIC_VERIFY_V2`) must
//!   trust each executable, and its signer certificate must have the C10
//!   publisher as its subject, carry the C10 identity EKU and be
//!   countersigned by a timestamp, the policy `scripts/verify-authenticode.ps1`
//!   applies for `scripts/sign-native.py` (status `Valid`, `-cne` publisher,
//!   EKU, `TimeStamperCertificate`). Revocation is not fetched: like
//!   `Get-AuthenticodeSignature`, the check is local and offline
//!   (`WTD_REVOKE_NONE`, cached URL retrieval only).
//! - macOS: `/usr/bin/codesign --verify --strict -R=<requirement>` with the
//!   C10 team's Developer ID requirement, the requirement
//!   `sign-native.py` verifies (`verify_app_bundle`).
//! - Linux: no signature; integrity rests on the owner-private record's
//!   digest (documented weaker, contracts §5.4.3).
//!
//! The decisions are pure (claim → required check, observation → verdict) and
//! tested on every host; the platform adapters only observe.
use serde::Deserialize;
use std::{fmt, path::Path, sync::OnceLock, time::Duration};

#[cfg(windows)]
mod authenticode;
#[cfg(test)]
mod tests;

/// The C10 signing identity file, embedded (contracts §7).
const IDENTITY_FILE: &str = include_str!("../../../scripts/signing-identity.json");
const IDENTITY_SCHEMA: &str = "airk-signing-identity/1";
/// The codesign bound; `sign-native.py` uses the same 30 s.
const CODESIGN_TIMEOUT: Duration = Duration::from_secs(30);
const CODESIGN: &str = "/usr/bin/codesign";

/// Refusal codes. Every one is `installation_unverified` class (exit 7) to
/// the reader.
pub const SIGNATURE_UNVERIFIED: &str = "signature_unverified";
pub const PLATFORM_MISMATCH: &str = "signing_platform_mismatch";
pub const IDENTITY_MISMATCH: &str = "signing_identity_mismatch";
pub const IDENTITY_UNAVAILABLE: &str = "signing_identity_unavailable";

/// A failed check: a stable code and a message for people.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
}

impl Refusal {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

/// The public identities C10 names that this check needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub windows_publisher: String,
    pub windows_identity_eku: String,
    pub macos_team_id: String,
}

// The owner (`sign-native.py`) refuses unknown fields; this reader takes only
// the public identity fields it needs and ignores the signing-service ones.
#[derive(Deserialize)]
struct IdentityFile {
    schema: String,
    windows: WindowsIdentity,
    macos: MacosIdentity,
}
#[derive(Deserialize)]
struct WindowsIdentity {
    publisher: String,
    identity_eku: String,
}
#[derive(Deserialize)]
struct MacosIdentity {
    team_id: String,
}

/// Parse a C10 identity file.
pub fn parse_identity(text: &str) -> Result<Identity, Refusal> {
    let unavailable = |message: String| Refusal::new(IDENTITY_UNAVAILABLE, message);
    let file: IdentityFile = serde_json::from_str(text)
        .map_err(|error| unavailable(format!("the signing identity file is invalid: {error}")))?;
    if file.schema != IDENTITY_SCHEMA {
        return Err(unavailable(format!(
            "the signing identity file has schema {:?}, expected {IDENTITY_SCHEMA}",
            file.schema
        )));
    }
    let identity = Identity {
        windows_publisher: file.windows.publisher,
        windows_identity_eku: file.windows.identity_eku,
        macos_team_id: file.macos.team_id,
    };
    if !identity.windows_publisher.starts_with("CN=")
        || !is_oid(&identity.windows_identity_eku)
        || !is_team_id(&identity.macos_team_id)
    {
        return Err(unavailable(
            "the signing identity file names a malformed identity".into(),
        ));
    }
    Ok(identity)
}

/// The embedded C10 identity, parsed once.
pub fn identity() -> Result<&'static Identity, Refusal> {
    static PARSED: OnceLock<Result<Identity, Refusal>> = OnceLock::new();
    PARSED
        .get_or_init(|| parse_identity(IDENTITY_FILE))
        .as_ref()
        .map_err(Clone::clone)
}

fn is_oid(value: &str) -> bool {
    !value.is_empty()
        && value
            .split('.')
            .all(|arc| !arc.is_empty() && arc.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_team_id(value: &str) -> bool {
    value.len() == 10
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

/// What an install record (or payload inventory) claims about its signing,
/// borrowed from `airk_core::install_record::Signing` by each caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim<'a> {
    Windows {
        publisher: &'a str,
        identity_eku: &'a str,
    },
    Macos {
        team_id: &'a str,
    },
    Linux {
        signed: bool,
        integrity: &'a str,
    },
}

/// The platform a check runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Windows,
    Macos,
    /// Unix other than macOS (the digest-only packages).
    Linux,
    Other,
}

impl Host {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(unix) {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

/// The check a claim requires on `host`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    Authenticode {
        publisher: String,
        identity_eku: String,
    },
    Codesign {
        team_id: String,
    },
    /// Linux: the record digest is the only integrity (weaker, documented).
    DigestOnly,
}

/// Pure: a claim for this host that names exactly the C10 identity, or the
/// refusal. A Windows or macOS record never falls back to digest only.
pub fn required_check(claim: Claim<'_>, host: Host, identity: &Identity) -> Result<Check, Refusal> {
    let mismatch = || {
        Refusal::new(
            PLATFORM_MISMATCH,
            "The install record's signing does not match this platform.",
        )
    };
    let other_identity = || {
        Refusal::new(
            IDENTITY_MISMATCH,
            "The install record names another signing identity than this build's.",
        )
    };
    match (claim, host) {
        (
            Claim::Windows {
                publisher,
                identity_eku,
            },
            Host::Windows,
        ) => {
            if publisher != identity.windows_publisher
                || identity_eku != identity.windows_identity_eku
            {
                return Err(other_identity());
            }
            Ok(Check::Authenticode {
                publisher: identity.windows_publisher.clone(),
                identity_eku: identity.windows_identity_eku.clone(),
            })
        }
        (Claim::Macos { team_id }, Host::Macos) => {
            if team_id != identity.macos_team_id {
                return Err(other_identity());
            }
            Ok(Check::Codesign {
                team_id: identity.macos_team_id.clone(),
            })
        }
        (Claim::Linux { signed, integrity }, Host::Linux) => {
            if signed || integrity != "record-sha256" {
                return Err(mismatch());
            }
            Ok(Check::DigestOnly)
        }
        _ => Err(mismatch()),
    }
}

/// The signer of a trusted Authenticode signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signer {
    /// The certificate subject in the order `X509Certificate.Subject` shows
    /// it (`CN=…, O=…, …, C=…`).
    pub subject: String,
    /// The certificate's enhanced key usage OIDs.
    pub ekus: Vec<String>,
    /// Whether the signature carries a timestamp countersignature.
    pub timestamped: bool,
}

/// What `WinVerifyTrust` observed for one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticodeObservation {
    /// The `WinVerifyTrust` result; 0 is trusted.
    pub status: i32,
    /// The signer, read only from a trusted result.
    pub signer: Option<Signer>,
}

/// Pure: the Authenticode verdict for one file.
pub fn authenticode_verdict(
    file: &str,
    observation: &AuthenticodeObservation,
    publisher: &str,
    identity_eku: &str,
) -> Result<(), Refusal> {
    let refused = |reason: String| Refusal::new(SIGNATURE_UNVERIFIED, format!("{file}: {reason}"));
    if observation.status != 0 {
        return Err(refused(format!(
            "Authenticode verification failed (0x{:08X})",
            observation.status as u32
        )));
    }
    let Some(signer) = &observation.signer else {
        return Err(refused("the signer certificate could not be read".into()));
    };
    if signer.subject != publisher {
        return Err(refused(format!(
            "the signer {:?} is not the AIRK publisher",
            signer.subject
        )));
    }
    if !signer.ekus.iter().any(|eku| eku == identity_eku) {
        return Err(refused("the signer lacks the AIRK identity EKU".into()));
    }
    if !signer.timestamped {
        return Err(refused("the signature has no timestamp".into()));
    }
    Ok(())
}

/// Pure: the designated requirement for the C10 team (Developer ID
/// Application leaf of that team, anchored at Apple).
pub fn codesign_requirement(team_id: &str) -> Result<String, Refusal> {
    if !is_team_id(team_id) {
        return Err(Refusal::new(
            IDENTITY_UNAVAILABLE,
            "the signing team identifier is malformed",
        ));
    }
    Ok(format!(
        "=anchor apple generic and certificate leaf[subject.OU] = \"{team_id}\" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
    ))
}

/// Pure: the codesign verdict for one file from its exit status and output.
pub fn codesign_verdict(file: &str, success: bool, output: &str) -> Result<(), Refusal> {
    if success {
        return Ok(());
    }
    let detail: String = output.trim().chars().take(512).collect();
    Err(Refusal::new(
        SIGNATURE_UNVERIFIED,
        format!("{file}: codesign verification failed: {detail}"),
    ))
}

/// How the pair was verified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Authenticode or codesign against C10.
    Signed,
    /// Linux: digest only.
    DigestOnly,
}

/// Verify `executables` against `claim` on this host (contracts §5.4.3).
/// The caller has already matched each executable's size and digest.
pub fn verify(claim: Claim<'_>, executables: &[&Path]) -> Result<Verdict, Refusal> {
    match required_check(claim, Host::current(), identity()?)? {
        Check::DigestOnly => Ok(Verdict::DigestOnly),
        Check::Authenticode {
            publisher,
            identity_eku,
        } => {
            for path in executables {
                let observation = observe_authenticode(path);
                authenticode_verdict(
                    &path.display().to_string(),
                    &observation,
                    &publisher,
                    &identity_eku,
                )?;
            }
            Ok(Verdict::Signed)
        }
        Check::Codesign { team_id } => {
            let requirement = codesign_requirement(&team_id)?;
            for path in executables {
                let file = path.display().to_string();
                let (success, output) = codesign(path, &requirement)?;
                codesign_verdict(&file, success, &output)?;
            }
            Ok(Verdict::Signed)
        }
    }
}

#[cfg(windows)]
fn observe_authenticode(path: &Path) -> AuthenticodeObservation {
    authenticode::observe(path)
}

/// Not reached: `required_check` asks for Authenticode only on Windows.
#[cfg(not(windows))]
fn observe_authenticode(_path: &Path) -> AuthenticodeObservation {
    AuthenticodeObservation {
        status: -1,
        signer: None,
    }
}

/// `/usr/bin/codesign --verify --strict -R=<requirement> <path>` (absolute
/// program, no PATH lookup) through the bounded native command runner.
fn codesign(path: &Path, requirement: &str) -> Result<(bool, String), Refusal> {
    let file = path.to_str().ok_or_else(|| {
        Refusal::new(
            SIGNATURE_UNVERIFIED,
            format!("{}: the path is not UTF-8", path.display()),
        )
    })?;
    let invocation = crate::bounded_command::Invocation::new(
        CODESIGN,
        &["--verify", "--strict", "-R", requirement, file],
    );
    crate::bounded_command::execute_with_timeout(&invocation, CODESIGN_TIMEOUT)
        .map(|output| (output.success, output.text))
        .map_err(|error| {
            Refusal::new(
                SIGNATURE_UNVERIFIED,
                format!("{file}: codesign could not run: {error:#}"),
            )
        })
}
