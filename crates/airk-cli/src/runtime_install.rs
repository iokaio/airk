// SPDX-License-Identifier: Apache-2.0
//! Offline installation, immutable runtime selection and same-user on-demand election.
//! Approval is supplied by a trusted distributor or an explicit machine/user policy;
//! neither a workspace path nor a manifest's own checksum establishes that approval.
use crate::{local_ipc, runtime_pair, service_runtime};
use airk_core::service::{ControlMethod, ControlResponse, ServiceHealth};
use anyhow::{Context, Result, bail, ensure};
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

#[path = "runtime_activation.rs"]
mod activation;
#[path = "runtime_launcher_repair.rs"]
mod launcher_repair;
#[path = "runtime_layout.rs"]
mod layout;
#[path = "runtime_managed.rs"]
mod managed;
pub use managed::ManagedLifecycle;
#[path = "runtime_lifecycle.rs"]
mod lifecycle;
#[path = "runtime_payload_repair.rs"]
mod payload_repair;
#[path = "runtime_retirement.rs"]
mod retirement;
#[path = "runtime_snapshot.rs"]
mod snapshot;
#[path = "runtime_staging.rs"]
mod staging;

const MAX_MANIFEST: u64 = 512 * 1024;
const MAX_FILES: usize = 2048;
const MAX_PAYLOAD: u64 = 4 * 1024 * 1024 * 1024;
pub use crate::user_paths::Channel as RuntimeChannel;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum VerificationBasis {
    TrustedDistribution,
    OrganizationDigest,
}

#[derive(Subcommand)]
pub enum RuntimeCommand {
    /// Resolve the native user's stable-channel install location; no workspace settings or writes.
    Location {
        #[arg(long, value_enum, default_value = "stable")]
        channel: RuntimeChannel,
    },
    /// Optional login registration; never required by on-demand operation.
    Login {
        #[arg(long)]
        install_dir: PathBuf,
        #[command(subcommand)]
        action: LoginCommand,
    },
    /// Run the selected service under an explicit external/user startup manager.
    Foreground {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Stage an offline payload with an independently approved install-manifest digest.
    Stage {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        approved_sha256: String,
        #[arg(long, value_enum)]
        verification_basis: VerificationBasis,
        /// Writable service state; defaults to native state for a native install, otherwise this install root.
        #[arg(long)]
        state_dir: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "stable")]
        channel: RuntimeChannel,
    },
    /// Abandon an unfinished state-layout claim; preserve all state directories and their contents.
    AbortLayout {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Restore the preceding selection after an unfinished activation; preserve runtime payloads.
    AbortActivation {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Pin an approved organization-provided payload without copying or modifying it.
    Managed {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        approved_sha256: String,
        #[arg(long, value_enum)]
        verification_basis: VerificationBasis,
        #[arg(long, value_enum, default_value = "external")]
        lifecycle: ManagedLifecycle,
        #[arg(long)]
        state_dir: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "stable")]
        channel: RuntimeChannel,
    },
    /// Drain the selected service and select a staged runtime only after native health succeeds.
    Activate {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        runtime_sha256: String,
    },
    /// Launch the selected service on demand, without registering login startup.
    Ensure {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Explicitly start a deliberately stopped service; automatic ensure honors that stop.
    Start {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Stop/restart the observed instance using one bounded durable request read from stdin.
    Lifecycle {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Abandon one saved lifecycle recovery intent; preserve every running process and keep automatic starts disabled.
    AbandonLifecycle {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        request_id: airk_core::identity::Sha256Digest,
    },
    /// Inspect verified selection without starting a process or opening a project.
    Inspect {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Read saved bootstrap/project metadata without executing or trusting selected payload files.
    Snapshot {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        after: Option<airk_core::service::ProjectBinding>,
        #[arg(long)]
        revision: Option<u64>,
        #[arg(long, default_value_t = 16)]
        limit: usize,
    },
    /// Preview or explicitly repair the selected payload, preserving the complete damaged slot.
    RepairPayload {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        approved_sha256: String,
        /// Omit for read-only preview; supply the reviewed preview digest to apply.
        #[arg(long)]
        preview: Option<airk_core::identity::Sha256Digest>,
    },
    /// Remove one unused owned runtime; preserve active/rollback selections and unknown files.
    Remove {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        runtime_sha256: String,
    },
    /// Abort an ownership-proven interrupted copy; preserve promoted runtimes and unknown files.
    AbortStage {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        runtime_sha256: String,
    },
    /// Repair an absent or known older launcher from the selected approved payload; requires stopped owners.
    RepairLauncher {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Abort an owned unfinished launcher copy; an already replaced image must be finalized.
    AbortLauncherRepair {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Stop/unregister this service and retire owned runtime payloads; preserve project data.
    Uninstall {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Bounded bootstrap diagnostics work even when runtime selection or the service fails.
    Diagnose {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Explicitly clear the on-demand crash circuit after diagnosing a startup failure.
    ResetStartup {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Restore the preceding verified selection using the same drain and health transaction.
    Rollback {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Stable launcher: forward arguments and stdio to the selected verified executable.
    Launch {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(last = true, required = true)]
        args: Vec<OsString>,
    },
    /// Bounded executable/paired-helper self-test, without collection access or inference.
    SelfTest,
}

pub(crate) fn validate_control_root(control: &Path) -> Result<()> {
    layout::validate_control(control)
}
pub(crate) fn record_control_root(lease: &airk_store::service::ServiceLease) -> Result<()> {
    layout::record_control(lease)
}
pub(crate) fn record_registry_root(
    lease: &airk_store::service::ServiceLease,
    namespace: airk_core::identity::Incarnation,
) -> Result<()> {
    layout::record_registry(lease, namespace)
}

pub(crate) fn log_parent(control: &Path) -> Result<PathBuf> {
    layout::log_parent(control)
}

pub fn default_install_root() -> Result<PathBuf> {
    Ok(crate::user_paths::UserPaths::discover(RuntimeChannel::Stable)?.install)
}

#[derive(Subcommand)]
pub enum LoginCommand {
    Plan,
    Status,
    Install,
    Start,
    Stop,
    Uninstall,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallManifest {
    pub schema: u32,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub schema: u32,
    pub manifest_sha256: String,
    pub version: String,
    pub verification_basis: VerificationBasis,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    schema: u32,
    pub active: String,
    pub previous: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LauncherReceipt {
    schema: u32,
    sha256: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartupAttempts {
    attempts_unix_ms: Vec<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapObservation {
    state: String,
    observed_unix_ms: u64,
    process_id: Option<u32>,
    exit_code: Option<i32>,
}

fn bootstrap(root: &Path, state: &str, pid: Option<u32>, exit_code: Option<i32>) -> Result<()> {
    replace_json(
        root,
        "bootstrap.json",
        &BootstrapObservation {
            state: state.into(),
            observed_unix_ms: unix_ms()?,
            process_id: pid,
            exit_code,
        },
    )
}

fn unix_ms() -> Result<u64> {
    Ok(u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

fn startup_attempt(root: &Path, identity: &str, now: u64) -> Result<()> {
    digest(identity)?;
    let name = format!("startup-{identity}.json");
    let path = root.join(&name);
    let mut state: StartupAttempts = if path.try_exists()? {
        serde_json::from_slice(&read_bounded(&path, 4096)?)?
    } else {
        StartupAttempts::default()
    };
    ensure!(
        state.attempts_unix_ms.len() <= 3,
        "invalid startup circuit receipt"
    );
    // A backwards clock preserves the circuit instead of extending retry capacity.
    state
        .attempts_unix_ms
        .retain(|time| now.saturating_sub(*time) < 120_000);
    ensure!(
        state.attempts_unix_ms.len() < 3,
        "on-demand startup circuit open: three attempts in two minutes; inspect diagnostics before runtime reset-startup"
    );
    if let Some(last) = state.attempts_unix_ms.last() {
        let backoff = (1u64 << state.attempts_unix_ms.len()) * 1000;
        ensure!(
            now.saturating_sub(*last) >= backoff,
            "on-demand restart backoff active; retry later"
        );
    }
    state.attempts_unix_ms.push(now);
    replace_json(root, &name, &state)
}

fn digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid approved SHA-256 identity"
    );
    Ok(())
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    safe_file(path)?;
    ensure!(
        fs::metadata(path)?.len() <= limit,
        "runtime metadata exceeds byte limit"
    );
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "runtime metadata grew beyond byte limit"
    );
    Ok(bytes)
}

pub(crate) fn safe_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "runtime file is not a regular unlinked file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(metadata.nlink() == 1, "aliased runtime file");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use std::os::windows::io::AsRawHandle;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected runtime file"
        );
        let file = File::open(path)?;
        // SAFETY: initialized output storage and a live read-only file handle.
        let mut information: windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION =
            unsafe { std::mem::zeroed() };
        // SAFETY: native API writes exactly the fixed output structure.
        ensure!(
            unsafe {
                windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle(
                    file.as_raw_handle(),
                    &mut information,
                )
            } != 0,
            "cannot inspect runtime file aliases"
        );
        ensure!(information.nNumberOfLinks == 1, "aliased runtime file");
    }
    Ok(())
}

fn payload_path(root: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty()
            && name.is_ascii()
            && name.len() <= 1024
            && !name.contains('\\')
            && !name.chars().any(char::is_control),
        "invalid payload name"
    );
    ensure!(
        name.split('/').all(|part| !part.is_empty()
            && part != "."
            && part != ".."
            && !part.contains(':')
            && !part.ends_with(['.', ' '])
            && !matches!(
                part.split('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase()
                    .as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            )),
        "non-portable or aliased payload name"
    );
    let path = Path::new(name);
    ensure!(
        path.components().count() <= 8
            && path.components().all(|c| matches!(c, Component::Normal(_))),
        "escaping payload name"
    );
    let joined = root.join(path);
    for parent in joined
        .parent()
        .context("payload parent missing")?
        .ancestors()
    {
        if parent == root {
            break;
        }
        if let Ok(meta) = fs::symlink_metadata(parent) {
            ensure!(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "redirected payload directory"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    meta.file_attributes() & 0x400 == 0,
                    "redirected payload directory"
                );
            }
        }
    }
    Ok(joined)
}

pub(crate) fn installed_identity(main: &Path) -> Result<Option<String>> {
    let manifest = main
        .parent()
        .context("runtime parent missing")?
        .join("airk-install.json");
    if !manifest.try_exists()? {
        return Ok(None);
    }
    let bytes = read_bounded(&manifest, MAX_MANIFEST)?;
    Ok(Some(format!("{:x}", Sha256::digest(bytes))))
}

pub fn main_name() -> &'static str {
    if cfg!(windows) { "airk.exe" } else { "airk" }
}
fn helper_name() -> &'static str {
    if cfg!(windows) {
        "airk-embed.exe"
    } else {
        "airk-embed"
    }
}
pub fn launcher_path(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        "airk-launcher.exe"
    } else {
        "airk-launcher"
    })
}

fn install_lock(root: &Path) -> Result<File> {
    ensure!(
        root.is_absolute(),
        "installation location must be absolute and machine/user selected"
    );
    let text = root.to_str().context("installation path must be UTF-8")?;
    ensure!(
        text.len() <= 4096 && !text.chars().any(char::is_control),
        "installation path exceeds native identity bound"
    );
    local_ipc::prepare_root(root)?;
    let lock = root.join("install-1.4.lock");
    if lock.try_exists()? {
        safe_file(&lock)?;
        ensure!(fs::metadata(&lock)?.len() == 0, "invalid install lock");
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock)?;
    safe_file(&lock)?;
    file.try_lock()
        .context("another AIRK installer/launcher is active; retry after it finishes")?;
    Ok(file)
}

fn replace_json<T: Serialize>(root: &Path, name: &str, value: &T) -> Result<()> {
    let path = root.join(name);
    if path.try_exists()? {
        safe_file(&path)?;
    }
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() <= MAX_MANIFEST as usize,
        "runtime metadata exceeds byte limit"
    );
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    sync_directory(root)
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn manifest(root: &Path, approved: &str) -> Result<(Vec<u8>, InstallManifest)> {
    digest(approved)?;
    let bytes = read_bounded(&root.join("airk-install.json"), MAX_MANIFEST)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == approved,
        "install manifest does not match independently approved digest"
    );
    let manifest: InstallManifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema == 1
            && manifest.os == std::env::consts::OS
            && manifest.arch == std::env::consts::ARCH,
        "runtime payload does not match the native host architecture"
    );
    ensure!(
        !manifest.version.is_empty()
            && manifest.version.len() <= 64
            && manifest
                .version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b)),
        "invalid runtime version"
    );
    ensure!(
        (4..=MAX_FILES).contains(&manifest.files.len()),
        "invalid payload file count"
    );
    for name in [main_name(), helper_name(), "airk-runtime.json", "LICENSE"] {
        ensure!(
            manifest.files.contains_key(name),
            "offline payload is missing {name}"
        );
    }
    ensure!(
        manifest
            .files
            .keys()
            .any(|name| name.starts_with("notices/")),
        "offline dependency notices missing"
    );
    let mut topology = std::collections::BTreeSet::new();
    let mut portable_names = BTreeMap::new();
    for name in manifest.files.keys() {
        payload_path(root, name)?;
        if let Some(prior) = portable_names.insert(name.to_ascii_lowercase(), name.clone()) {
            ensure!(prior == *name, "case-aliased payload topology");
        }
        topology.insert(name.clone());
        let mut parent = Path::new(name).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            let directory = path
                .to_str()
                .context("non-UTF-8 payload path")?
                .replace('\\', "/");
            ensure!(
                !manifest.files.contains_key(&directory),
                "payload file/directory identity collision"
            );
            topology.insert(directory);
            let directory = path
                .to_str()
                .context("non-UTF-8 payload directory")?
                .replace('\\', "/");
            if let Some(prior) =
                portable_names.insert(directory.to_ascii_lowercase(), directory.clone())
            {
                ensure!(prior == directory, "case-aliased payload topology");
            }
            parent = path.parent();
        }
        ensure!(
            topology.len() <= 4094,
            "payload topology exceeds bounded removal inventory"
        );
    }
    let mut total = 0u64;
    for (name, expected) in &manifest.files {
        digest(expected)?;
        ensure!(!staging::reserved(name), "reserved payload name");
        let file = payload_path(root, name)?;
        safe_file(&file)?;
        total = total
            .checked_add(fs::metadata(&file)?.len())
            .context("runtime payload byte overflow")?;
        ensure!(total <= MAX_PAYLOAD, "runtime payload exceeds disk budget");
        ensure!(
            runtime_pair::digest_file(&file)? == *expected,
            "runtime payload digest mismatch: {name}"
        );
    }
    runtime_pair::verify_installed_pair(&root.join(main_name()), &manifest.version)?;
    Ok((bytes, manifest))
}

pub fn stage(
    source: &Path,
    root: &Path,
    approved: &str,
    basis: VerificationBasis,
) -> Result<Approval> {
    staging::stage(source, root, approved, basis)
}

fn prepare_launcher(root: &Path, main: &Path) -> Result<()> {
    let destination = launcher_path(root);
    let receipt = root.join("launcher.json");
    if destination.try_exists()? && receipt.try_exists()? {
        let known: LauncherReceipt = serde_json::from_slice(&read_bounded(&receipt, 4096)?)?;
        ensure!(
            known.schema == 1 && runtime_pair::digest_file(&destination)? == known.sha256,
            "stable launcher ownership/digest mismatch"
        );
        return Ok(());
    }
    let expected = runtime_pair::digest_file(main)?;
    if receipt.try_exists()? {
        let known: LauncherReceipt = serde_json::from_slice(&read_bounded(&receipt, 4096)?)?;
        ensure!(
            known.schema == 1 && known.sha256 == expected,
            "interrupted launcher belongs to a different approved runtime"
        );
    }
    if destination.try_exists()? {
        safe_file(&destination)?;
        ensure!(
            runtime_pair::digest_file(&destination)? == expected,
            "unrecognized stable launcher; refusing replacement"
        );
        return replace_json(
            root,
            "launcher.json",
            &LauncherReceipt {
                schema: 1,
                sha256: expected,
            },
        );
    }
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    std::io::copy(&mut File::open(main)?, &mut temporary)?;
    temporary.as_file().sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o700))?;
    }
    let sha256 = runtime_pair::digest_file(temporary.path())?;
    temporary.persist_noclobber(&destination)?;
    replace_json(
        root,
        "launcher.json",
        &LauncherReceipt { schema: 1, sha256 },
    )
}

pub fn verified_launcher(root: &Path) -> Result<PathBuf> {
    local_ipc::prepare_root(root)?;
    launcher_repair::require_settled(root)?;
    let launcher = launcher_path(root);
    safe_file(&launcher)?;
    let receipt: LauncherReceipt =
        serde_json::from_slice(&read_bounded(&root.join("launcher.json"), 4096)?)?;
    ensure!(
        receipt.schema == 1 && runtime_pair::digest_file(&launcher)? == receipt.sha256,
        "stable launcher ownership/digest mismatch"
    );
    Ok(launcher)
}

pub fn verify_runtime(root: &Path, identity: &str) -> Result<(PathBuf, Approval)> {
    digest(identity)?;
    ensure!(root.is_dir(), "runtime installation is missing");
    local_ipc::prepare_root(root)?;
    if let Some(policy) = managed::read(root)? {
        ensure!(
            policy.approval.manifest_sha256 == identity,
            "managed runtime is pinned to another approved identity"
        );
        return policy.verify();
    }
    let (main, approval, inventory) = owned_inventory(root, identity)?;
    managed::verify_topology(
        main.parent().context("runtime parent missing")?,
        &inventory,
        true,
    )?;
    Ok((main, approval))
}

/// Ownership proof for bounded retirement. Unknown files still prevent execution,
/// but do not prevent saving the inventory needed to resume their safe retirement.
fn owned_inventory(root: &Path, identity: &str) -> Result<(PathBuf, Approval, InstallManifest)> {
    digest(identity)?;
    ensure!(root.is_dir(), "runtime installation is missing");
    local_ipc::prepare_root(root)?;
    let directory = root.join("runtimes").join(identity);
    ensure!(directory.is_dir(), "approved runtime is not staged");
    local_ipc::prepare_root(&directory)?;
    let approval: Approval =
        serde_json::from_slice(&read_bounded(&directory.join("approval.json"), 4096)?)?;
    ensure!(
        approval.schema == 1 && approval.manifest_sha256 == identity,
        "runtime approval identity mismatch"
    );
    let (_, manifest) = manifest(&directory, identity)?;
    ensure!(
        manifest.version == approval.version,
        "runtime approval version mismatch"
    );
    Ok((directory.join(main_name()), approval, manifest))
}

pub fn selected(root: &Path) -> Result<(Selection, PathBuf, Approval)> {
    payload_repair::require_settled(root)?;
    ensure!(root.is_dir(), "runtime installation is missing");
    local_ipc::prepare_root(root)?;
    ensure!(
        !root.join("uninstall.json").try_exists()?,
        "runtime uninstall is in progress; resume uninstall before installing or launching"
    );
    let selection = selection_record(root)?;
    let (main, approval) = verify_runtime(root, &selection.active)?;
    Ok((selection, main, approval))
}

/// A private selection record protects active/rollback identities during cleanup;
/// execution additionally requires complete inventory and topology verification.
fn selection_record(root: &Path) -> Result<Selection> {
    let selection: Selection =
        serde_json::from_slice(&read_bounded(&root.join("selected.json"), 4096)?)?;
    ensure!(selection.schema == 1, "unsupported runtime selection");
    digest(&selection.active)?;
    if let Some(previous) = &selection.previous {
        digest(previous)?;
    }
    Ok(selection)
}

fn self_test(main: &Path) -> Result<()> {
    struct Reaped(std::process::Child);
    impl std::ops::Deref for Reaped {
        type Target = std::process::Child;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl std::ops::DerefMut for Reaped {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl Drop for Reaped {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }
    runtime_pair::validate_native_image(main)?;
    runtime_pair::validate_native_image(
        &main
            .parent()
            .context("runtime directory missing")?
            .join(helper_name()),
    )?;
    // A fixed native command, no shell, no source or collection parameters.
    let mut command = Command::new(main);
    command
        .args(["runtime", "self-test"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hidden(&mut command);
    let mut child = Reaped(crate::native_process::spawn(&mut command).context(
        "approved runtime cannot execute; inspect native execution policy/dependencies",
    )?);
    let _containment = crate::child_job::ChildJob::attach(&mut child)?;
    let output = child
        .stdout
        .take()
        .context("native self-test output is unavailable")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("airk-self-test-result".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = output.take(4097).read_to_end(&mut bytes).map(|_| bytes);
            let _ = sender.send(result);
        })?;
    let until = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "native runtime self-test failed");
            let bytes = receiver
                .recv_timeout(Duration::from_secs(1))
                .context("native runtime self-test did not return a bounded result")??;
            let pair: runtime_pair::RuntimePair = serde_json::from_slice(&read_bounded(
                &main
                    .parent()
                    .context("runtime directory missing")?
                    .join("airk-runtime.json"),
                4096,
            )?)?;
            return validate_self_test_result(&bytes, &pair.version);
        }
        if std::time::Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            bail!("native self-test exceeded 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn validate_self_test_result(bytes: &[u8], version: &str) -> Result<()> {
    ensure!(bytes.len() <= 4096, "native self-test result exceeds 4 KiB");
    let result: serde_json::Value = serde_json::from_slice(bytes)?;
    ensure!(
        result["protocol"] == 1 && result["self_test"] == "passed",
        "native self-test protocol or completion mismatch"
    );
    ensure!(
        result["version"].as_str() == Some(version),
        "native executable version does not match the approved payload"
    );
    ensure!(
        result["state_layout"] == 1
            && result["foreground_dispatch"] == 1
            && result["managed_reference"] == 1
            && result["activation_resume"] == 1,
        "approved runtime lacks required state-layout, foreground-dispatch, managed-reference or activation-resume capability"
    );
    Ok(())
}

fn hidden(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000 | 0x00000200);
    }
    #[cfg(not(windows))]
    let _ = command;
}

async fn health(root: &Path, approval: &Approval, expected_sha256: &str) -> Result<ServiceHealth> {
    let response = service_runtime::request_for_version(
        &layout::control(root)?,
        ControlMethod::Health,
        &approval.version,
    )
    .await?;
    let ControlResponse::Health { observation } = response else {
        bail!("service did not return native health");
    };
    ensure!(
        observation.executable_sha256 == expected_sha256,
        "running service belongs to another runtime; explicit activation/drain required"
    );
    ensure!(
        observation.runtime_sha256.as_deref() == Some(approval.manifest_sha256.as_str()),
        "running service belongs to another approved payload; explicit activation/drain required"
    );
    ensure!(observation.policy.accepting, "service is draining");
    Ok(*observation)
}

async fn start(
    root: &Path,
    approval: &Approval,
    main: &Path,
    enforce_backoff: bool,
) -> Result<ServiceHealth> {
    let control = layout::control(root)?;
    local_ipc::prepare_root(&control)?;
    let expected_sha256 = runtime_pair::digest_file(main)?;
    if let Ok(observation) = health(root, approval, &expected_sha256).await {
        if observation.uptime_ms >= 60_000 {
            replace_json(
                root,
                &format!("startup-{}.json", approval.manifest_sha256),
                &StartupAttempts::default(),
            )?;
        }
        return Ok(observation);
    }
    if enforce_backoff {
        startup_attempt(root, &approval.manifest_sha256, unix_ms()?)?;
    }
    let mut command = Command::new(main);
    command
        .arg("serve")
        .arg("--runtime-dir")
        .arg(&control)
        .args(["--idle-exit-seconds", "180"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hidden(&mut command);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid has no pointer arguments and runs before exec in the child.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }
    bootstrap(root, "starting", None, None)?;
    #[cfg(windows)]
    let launched = crate::native_process::spawn_detached(
        main,
        &[
            "serve".into(),
            "--runtime-dir".into(),
            control.as_os_str().to_owned(),
            "--idle-exit-seconds".into(),
            "180".into(),
        ],
    );
    #[cfg(not(windows))]
    let launched = crate::native_process::spawn(&mut command);
    let mut child = match launched {
        Ok(child) => child,
        Err(error) => {
            let _ = bootstrap(
                root,
                "execution_denied_or_unavailable",
                None,
                error.raw_os_error(),
            );
            return Err(error).context("cannot start approved user service; inspect execution policy and native dependencies");
        }
    };
    let until = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(observation) = health(root, approval, &expected_sha256).await {
            let _ = bootstrap(root, "ready", Some(observation.process_id), None);
            // Reap if this host process remains alive after its detached child exits.
            let _ = std::thread::Builder::new()
                .name("airk-service-reaper".into())
                .spawn(move || {
                    let _ = child.wait();
                });
            return Ok(observation);
        }
        if let Some(status) = child.try_wait()? {
            let _ = bootstrap(
                root,
                "exited_before_health",
                Some(child.id()),
                status.code(),
            );
            bail!("approved service exited before health: {status}");
        }
        if tokio::time::Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            let _ = bootstrap(root, "health_timeout", Some(child.id()), None);
            bail!("approved service did not become healthy within 60 seconds");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub async fn ensure_running(root: &Path) -> Result<ServiceHealth> {
    let _lock = install_lock(root)?;
    payload_repair::require_settled(root)?;
    lifecycle::settle(root).await?;
    if managed::read(root)?.is_some() {
        return managed::ensure(root).await;
    }
    if activation::pending(root)?.is_some() {
        activation::resume(root).await?;
    }
    launcher_repair::require_settled(root)?;
    let (_, main, approval) = selected(root)?;
    activation::start_selected(root, &approval, &main, None).await
}

pub(crate) fn startup_selected(root: &Path) -> Result<(PathBuf, Approval)> {
    payload_repair::require_settled(root)?;
    lifecycle::require_start_allowed(root)?;
    if let Some(selected) = activation::startup(root)? {
        return Ok(selected);
    }
    let (_, main, approval) = selected(root)?;
    Ok((main, approval))
}

async fn drain(root: &Path, main: &Path, approval: &Approval) -> Result<()> {
    let expected_sha256 = runtime_pair::digest_file(main)?;
    drain_identity(root, approval, &expected_sha256).await
}

/// Shutdown authenticates saved identity, without executing or qualifying a
/// payload. Missing/changed ancillary files must not disable an owned OS stop.
fn stop_identity(root: &Path) -> Result<(Approval, String)> {
    let selection = selection_record(root)?;
    let directory = payload_path(
        root,
        &format!("runtimes/{}/airk-install.json", selection.active),
    )?
    .parent()
    .context("shutdown runtime parent missing")?
    .to_path_buf();
    saved_payload_identity(&directory, &selection.active)
}

fn saved_payload_identity(directory: &Path, identity: &str) -> Result<(Approval, String)> {
    let approval: Approval =
        serde_json::from_slice(&read_bounded(&directory.join("approval.json"), 4096)?)?;
    let bytes = read_bounded(&directory.join("airk-install.json"), MAX_MANIFEST)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == identity,
        "shutdown inventory identity mismatch"
    );
    let inventory: InstallManifest = serde_json::from_slice(&bytes)?;
    ensure!(
        approval.schema == 1
            && inventory.schema == 1
            && approval.manifest_sha256 == identity
            && approval.version == inventory.version,
        "shutdown approval identity mismatch"
    );
    let hash = inventory
        .files
        .get(main_name())
        .context("shutdown executable identity missing")?;
    digest(hash)?;
    Ok((approval, hash.clone()))
}

async fn wait_quiescent(root: &Path) -> Result<()> {
    let until = tokio::time::Instant::now() + Duration::from_secs(45);
    loop {
        if let Ok(guard) = quiescent(root) {
            drop(guard);
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < until,
            "service owners have not released after stop"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn stop_login(
    root: &Path,
    manager: &crate::service_manager::ServiceManager,
    uninstall: bool,
) -> Result<crate::service_manager::ServiceStatus> {
    // A graceful drain is preferable, but its receipts/health may be damaged.
    // Only OS-held registration proof authorizes the fallback stop below.
    if let Ok((approval, hash)) = stop_identity(root) {
        let _ = drain_identity(root, &approval, &hash).await;
    }
    let status = manager.stop()?;
    wait_quiescent(root).await?;
    if uninstall {
        manager.uninstall()
    } else {
        Ok(status)
    }
}

async fn drain_identity(root: &Path, approval: &Approval, expected_sha256: &str) -> Result<()> {
    let control = layout::control(root)?;
    if !control.try_exists()? {
        return Ok(());
    }
    local_ipc::prepare_root(&control)?;
    if airk_store::service::ServiceLease::acquire(&control).is_err() {
        // Authenticate exact executable identity before requesting shutdown.
        let observed = health(root, approval, expected_sha256).await?;
        ensure!(
            matches!(
                service_runtime::request_for_version_and_instance(
                    &control,
                    ControlMethod::Shutdown,
                    &approval.version,
                    observed.instance
                )
                .await?,
                ControlResponse::Stopping
            ),
            "service refused drain"
        );
    }
    let until = tokio::time::Instant::now() + Duration::from_secs(45);
    loop {
        if let Ok(guard) = quiescent(root) {
            drop(guard);
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < until,
            "service has not released ownership; runtime selection preserved"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

struct Quiescence {
    _service: airk_store::service::ServiceLease,
    _projects: Vec<airk_store::service::ProjectLease>,
    _embedding: airk_store::service::EmbeddingLease,
    _source_watch: airk_store::service::SourceWatchLease,
}

fn quiescent(root: &Path) -> Result<Quiescence> {
    let control = layout::control(root)?;
    local_ipc::prepare_root(&control)?;
    let service = airk_store::service::ServiceLease::acquire(&control)?;
    layout::record_control(&service)?;
    let registry = airk_store::service::registry::ServiceRegistry::open(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    let projects = registry
        .bindings()?
        .into_iter()
        .map(|binding| airk_store::service::ProjectLease::acquire(&control, binding))
        .collect::<Result<Vec<_>>>()?;
    let embedding = airk_store::service::EmbeddingLease::acquire(&control)?;
    Ok(Quiescence {
        _service: service,
        _projects: projects,
        _embedding: embedding,
        _source_watch: airk_store::service::SourceWatchLease::acquire(&control)?,
    })
}

pub async fn activate(root: &Path, identity: &str) -> Result<Selection> {
    let _lock = install_lock(root)?;
    payload_repair::require_settled(root)?;
    lifecycle::require_settled(root)?;
    lifecycle::require_start_allowed(root)?;
    activation::begin(root, identity).await
}

#[path = "runtime_install_commands.rs"]
mod commands;
pub use commands::run;

#[cfg(test)]
#[path = "runtime_install_tests.rs"]
mod tests;
