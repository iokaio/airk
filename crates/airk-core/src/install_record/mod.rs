// SPDX-License-Identifier: Apache-2.0
//! I4: the install pointer, install record and setup operation file (split
//! plan §4.2 I4, §9.5–§9.6; `docs/split/contracts.md` §5).
//!
//! Pure record definitions and pure checks only. Reading files, ownership,
//! link and signature checks are adapters in the readers (`airkctl`, the
//! setup role and the extension), which share the fixtures in
//! `fixtures/split/i4/`. Persisted bytes are canonical: one compact JSON line
//! with fields in declaration order, then a newline.
use crate::identity::{Incarnation, Sha256Digest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const POINTER_SCHEMA: &str = "airk-install-pointer/1";
pub const RECORD_SCHEMA: &str = "airk-install-record/1";
pub const SETUP_OPERATION_SCHEMA: &str = "airk-setup-operation/1";
pub const MAX_POINTER_BYTES: usize = 4 * 1024;
pub const MAX_RECORD_BYTES: usize = 64 * 1024;
pub const MAX_SETUP_OPERATION_BYTES: usize = 16 * 1024;
/// A setup operation request is consumed within this many milliseconds.
pub const MAX_SETUP_OPERATION_LIFETIME_MS: u64 = 10 * 60 * 1000;
/// The record file name inside the program folder.
pub const RECORD_FILE: &str = "install-record.json";
pub const START_REQUEST_SCHEMA: &str = "airk-start-request/1";
pub const MAX_START_REQUEST_BYTES: usize = 1024;
/// A start request is consumed within this many milliseconds (plan §5.1).
pub const MAX_START_REQUEST_LIFETIME_MS: u64 = 30_000;

/// Why a pointer, record or operation was refused. Readers map every case to
/// `installation_unverified` (I2 exit 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Oversize,
    Malformed,
    UnknownSchema,
    NonCanonical,
    RecordDigestMismatch,
    InstallIdMismatch,
    VersionMismatch,
    RelativeLocation,
    ExecutableOutsideProgram,
    Expired,
    LifetimeTooLong,
    StartRequestNonceMismatch,
    StartRequestExpired,
    StartRequestLifetimeTooLong,
}

impl Refusal {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Oversize => "install_file_oversize",
            Self::Malformed => "install_file_malformed",
            Self::UnknownSchema => "install_file_unknown_schema",
            Self::NonCanonical => "install_file_non_canonical",
            Self::RecordDigestMismatch => "install_record_digest_mismatch",
            Self::InstallIdMismatch => "install_id_mismatch",
            Self::VersionMismatch => "install_version_mismatch",
            Self::RelativeLocation => "install_location_relative",
            Self::ExecutableOutsideProgram => "install_executable_outside_program",
            Self::Expired => "setup_operation_expired",
            Self::LifetimeTooLong => "setup_operation_lifetime_too_long",
            Self::StartRequestNonceMismatch => "start_request_nonce_mismatch",
            Self::StartRequestExpired => "start_request_expired",
            Self::StartRequestLifetimeTooLong => "start_request_lifetime_too_long",
        }
    }
}

/// The fixed per-user pointer naming the chosen locations (U39). Written last.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallPointer {
    pub schema: String,
    pub install_id: Incarnation,
    /// Absolute program folder.
    pub program: String,
    /// Record file name inside `program`.
    pub record: String,
    pub record_sha256: Sha256Digest,
    pub version: String,
    pub written_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contracts {
    pub i1_protocol: u32,
    pub i2_contract: u32,
    pub i3_admin: u32,
    pub i4_record: u32,
    pub state: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Executable {
    /// Relative to the program folder, `/`-separated.
    pub path: String,
    pub sha256: Sha256Digest,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Executables {
    pub airkctl: Executable,
    pub airk: Executable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminAssets {
    pub inventory_sha256: Sha256Digest,
    pub bundle_sha256: Sha256Digest,
}

/// Absolute locations chosen at install time (U39).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locations {
    pub program: String,
    pub data: String,
    pub state: String,
    pub cache: String,
    pub logs: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "platform", rename_all = "snake_case", deny_unknown_fields)]
pub enum Signing {
    Windows {
        publisher: String,
        identity_eku: String,
    },
    Macos {
        team_id: String,
    },
    /// Linux packages are unsigned; integrity rests on the record digest.
    Linux {
        signed: bool,
        integrity: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Installer {
    Inno,
    Dmg,
    AppFirstLaunch,
    InstallSh,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledBy {
    pub operation_id: Incarnation,
    pub installer: Installer,
    pub installed_unix_ms: u64,
}

/// The install record in the program folder. Written only after the complete
/// pair and asset inventory pass their signature or integrity checks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallRecord {
    pub schema: String,
    pub install_id: Incarnation,
    pub version: String,
    pub channel: String,
    pub target: String,
    pub contracts: Contracts,
    pub executables: Executables,
    pub admin_assets: AdminAssets,
    pub locations: Locations,
    pub signing: Signing,
    pub installed_by: InstalledBy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Install,
    Repair,
    Upgrade,
    Uninstall,
    Purge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgements {
    pub predecessor_removal: bool,
    pub index_reset: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustChoice {
    Ask,
    Skip,
}

/// Index handling on uninstall (U32): ask, keep (unattended default) or purge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexDeletion {
    Ask,
    Keep,
    Purge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupOptions {
    pub login_startup: bool,
    pub trust: TrustChoice,
    pub start: bool,
    pub delete_indexes: IndexDeletion,
}

/// One owner-private request consumed once by the setup role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupOperation {
    pub schema: String,
    pub operation_id: Incarnation,
    pub kind: OperationKind,
    pub created_unix_ms: u64,
    pub expires_unix_ms: u64,
    pub requested_by: String,
    pub acknowledgements: Acknowledgements,
    pub locations: Locations,
    pub payload_inventory_sha256: Sha256Digest,
    pub options: SetupOptions,
}

/// How a detached start treats a saved explicit Stop (plan §5.1): an explicit
/// start (a user's Start) clears it; an automatic start honors it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartMode {
    Explicit,
    Automatic,
}

/// An owner-private, single-use request that admits the `service-detached`
/// role (`docs/split/contracts.md` §5.7). The starter writes it; the role
/// consumes it atomically before opening any state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    pub schema: String,
    pub nonce: Incarnation,
    pub mode: StartMode,
    pub created_unix_ms: u64,
    pub expires_unix_ms: u64,
}

/// Durable phases of one setup transaction (plan §9.6), in order. Login,
/// trust and readiness are recorded separately so one success cannot hide
/// another's failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupPhase {
    Locked,
    PredecessorsRetired,
    ServiceDrained,
    Placed,
    RecordWritten,
    PointerWritten,
    LoginRegistered,
    LoginFailed,
    /// This platform's login startup is not implemented by this build; the
    /// step's detail names why (Phase 7).
    LoginUnsupported,
    TrustApplied,
    TrustDeclined,
    TrustUnsupported,
    Started,
    Healthy,
    // Uninstall and purge (U20, U32): each removal is recorded separately.
    LoginUnregistered,
    TrustRemoved,
    PointerRemoved,
    RecordRemoved,
    /// Only with `delete_indexes: purge` or the `purge` kind.
    DataPurged,
    Complete,
}

pub const SETUP_RECEIPT_SCHEMA: &str = "airk-setup-receipt/1";
pub const MAX_SETUP_RECEIPT_BYTES: usize = 256 * 1024;
/// Upper bounds of one receipt's lists and texts.
pub const MAX_SETUP_STEPS: usize = 64;
pub const MAX_SETUP_LEFTOVERS: usize = 256;
pub const MAX_SETUP_TEXT: usize = 2048;

/// One durable step of a setup transaction. `outcome` is a snake-case code;
/// `detail` is bounded plain text (paths, reasons), never a credential.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupStep {
    pub phase: SetupPhase,
    pub at_unix_ms: u64,
    pub outcome: String,
    pub detail: String,
}

/// An item setup found but did not remove (U14), with its reason.
/// `restarts_old_service` marks a leftover that can start a pre-split
/// service again, which is reported first (plan §9.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Leftover {
    pub item: String,
    pub reason: String,
    pub restarts_old_service: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupFailure {
    pub code: String,
    pub message: String,
    pub at_unix_ms: u64,
}

/// The setup receipt `<state>/setup/<operation_id>.json` (plan §9.6;
/// `docs/split/contracts.md` §5.6). It binds the exact operation bytes it was
/// started from (`operation_sha256`), records every phase separately, lists
/// what was retired and what was left, and is closed exactly once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupReceipt {
    pub schema: String,
    pub operation_id: Incarnation,
    pub operation_sha256: Sha256Digest,
    pub kind: OperationKind,
    pub expires_unix_ms: u64,
    pub steps: Vec<SetupStep>,
    pub retired: Vec<String>,
    pub leftovers: Vec<Leftover>,
    pub failure: Option<SetupFailure>,
    pub closed: bool,
}

impl SetupReceipt {
    /// Decode a receipt written by the setup role; canonical bytes only.
    pub fn decode(bytes: &[u8]) -> Result<Self, Refusal> {
        let receipt: Self = decode(
            bytes,
            MAX_SETUP_RECEIPT_BYTES,
            SETUP_RECEIPT_SCHEMA,
            |r: &Self| &r.schema,
        )?;
        let text_ok = |text: &str| text.len() <= MAX_SETUP_TEXT;
        if receipt.steps.len() > MAX_SETUP_STEPS
            || receipt.leftovers.len() > MAX_SETUP_LEFTOVERS
            || receipt.retired.len() > MAX_SETUP_LEFTOVERS
            || !receipt
                .steps
                .iter()
                .all(|step| text_ok(&step.outcome) && text_ok(&step.detail))
            || !receipt
                .leftovers
                .iter()
                .all(|left| text_ok(&left.item) && text_ok(&left.reason))
            || !receipt.retired.iter().all(|item| text_ok(item))
            || receipt
                .failure
                .as_ref()
                .is_some_and(|failure| !text_ok(&failure.code) || !text_ok(&failure.message))
        {
            return Err(Refusal::Malformed);
        }
        Ok(receipt)
    }

    /// Whether `phase` has been recorded.
    pub fn reached(&self, phase: SetupPhase) -> bool {
        self.steps.iter().any(|step| step.phase == phase)
    }
}

fn decode<T: serde::de::DeserializeOwned + Serialize>(
    bytes: &[u8],
    limit: usize,
    schema: &str,
    read_schema: impl Fn(&T) -> &str,
) -> Result<T, Refusal> {
    if bytes.len() > limit {
        return Err(Refusal::Oversize);
    }
    let value: T = serde_json::from_slice(bytes).map_err(|_| Refusal::Malformed)?;
    if read_schema(&value) != schema {
        return Err(Refusal::UnknownSchema);
    }
    if canonical(&value) != bytes {
        return Err(Refusal::NonCanonical);
    }
    Ok(value)
}

/// The canonical persisted bytes: compact JSON, declaration order, newline.
pub fn canonical<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(value).expect("I4 records serialize");
    bytes.push(b'\n');
    bytes
}

fn absolute(path: &str) -> bool {
    path.starts_with('/')
        || (path.len() > 3 && path.as_bytes()[0].is_ascii_alphabetic() && &path[1..3] == ":\\")
}

fn inside_program(relative: &str) -> bool {
    !relative.is_empty()
        && !absolute(relative)
        && !relative.contains('\\')
        && relative
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

impl InstallPointer {
    pub fn decode(bytes: &[u8]) -> Result<Self, Refusal> {
        let pointer: Self = decode(bytes, MAX_POINTER_BYTES, POINTER_SCHEMA, |p: &Self| {
            &p.schema
        })?;
        if !absolute(&pointer.program) {
            return Err(Refusal::RelativeLocation);
        }
        if pointer.record != RECORD_FILE {
            return Err(Refusal::Malformed);
        }
        Ok(pointer)
    }
}

impl InstallRecord {
    /// Decode a record and check it against the pointer that named it.
    pub fn decode(bytes: &[u8], pointer: &InstallPointer) -> Result<Self, Refusal> {
        let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
        if digest != pointer.record_sha256 {
            return Err(Refusal::RecordDigestMismatch);
        }
        let record: Self = decode(bytes, MAX_RECORD_BYTES, RECORD_SCHEMA, |r: &Self| &r.schema)?;
        if record.install_id != pointer.install_id {
            return Err(Refusal::InstallIdMismatch);
        }
        if record.version != pointer.version {
            return Err(Refusal::VersionMismatch);
        }
        let locations = &record.locations;
        if [
            &locations.program,
            &locations.data,
            &locations.state,
            &locations.cache,
            &locations.logs,
        ]
        .iter()
        .any(|path| !absolute(path))
            || locations.program != pointer.program
        {
            return Err(Refusal::RelativeLocation);
        }
        if !inside_program(&record.executables.airk.path)
            || !inside_program(&record.executables.airkctl.path)
        {
            return Err(Refusal::ExecutableOutsideProgram);
        }
        Ok(record)
    }
}

impl SetupOperation {
    /// Decode an operation request; `now_unix_ms` comes from the caller's clock.
    pub fn decode(bytes: &[u8], now_unix_ms: u64) -> Result<Self, Refusal> {
        let operation: Self = decode(
            bytes,
            MAX_SETUP_OPERATION_BYTES,
            SETUP_OPERATION_SCHEMA,
            |o: &Self| &o.schema,
        )?;
        if operation.expires_unix_ms <= operation.created_unix_ms
            || operation.expires_unix_ms - operation.created_unix_ms
                > MAX_SETUP_OPERATION_LIFETIME_MS
        {
            return Err(Refusal::LifetimeTooLong);
        }
        if now_unix_ms >= operation.expires_unix_ms || now_unix_ms < operation.created_unix_ms {
            return Err(Refusal::Expired);
        }
        let locations = &operation.locations;
        if [
            &locations.program,
            &locations.data,
            &locations.state,
            &locations.cache,
            &locations.logs,
        ]
        .iter()
        .any(|path| !absolute(path))
        {
            return Err(Refusal::RelativeLocation);
        }
        Ok(operation)
    }
}

impl StartRequest {
    /// Decode a request named by `expected` and check it against the caller's
    /// clock (`now_unix_ms`).
    pub fn decode(bytes: &[u8], expected: Incarnation, now_unix_ms: u64) -> Result<Self, Refusal> {
        let request: Self = decode(
            bytes,
            MAX_START_REQUEST_BYTES,
            START_REQUEST_SCHEMA,
            |r: &Self| &r.schema,
        )?;
        if request.nonce != expected {
            return Err(Refusal::StartRequestNonceMismatch);
        }
        if request.expires_unix_ms <= request.created_unix_ms
            || request.expires_unix_ms - request.created_unix_ms > MAX_START_REQUEST_LIFETIME_MS
        {
            return Err(Refusal::StartRequestLifetimeTooLong);
        }
        if now_unix_ms >= request.expires_unix_ms || now_unix_ms < request.created_unix_ms {
            return Err(Refusal::StartRequestExpired);
        }
        Ok(request)
    }
}

#[cfg(test)]
mod tests;
