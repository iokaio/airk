// SPDX-License-Identifier: Apache-2.0
//! Offline installation, immutable runtime selection and same-user on-demand election.
//! Approval is supplied by a trusted distributor or an explicit machine/user policy;
//! neither a workspace path nor a manifest's own checksum establishes that approval.
use crate::install_files::{MAC_LAUNCHER_IMAGE, launcher_path, main_name, safe_file};
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

/// Do not classify a dangling link as an absent owned artifact. Callers must
/// inspect/reject the existing directory entry before any cleanup or reuse.
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

mod activation;
pub(crate) use activation::verify_admission_release;
mod setup;
pub(crate) use setup::{SetupAdmission, SetupCancellation, installed_control_cooperative};
pub(crate) mod clean_break_background;
mod clean_break_cleanup;
mod data_migration;
mod http_cleanup;
mod inventory;
mod launcher_repair;
mod layout;
mod managed;
mod state_contract;
pub(crate) use state_contract::InitializedProbe;
mod upgrade_stop;
pub use managed::ManagedLifecycle;
mod http_rotation;
mod issuer_lifecycle;
mod legacy_caches;
mod legacy_index_adoption;
mod legacy_launchers;
mod legacy_maintenance;
mod lifecycle;
#[cfg(any(target_os = "macos", test))]
#[path = "runtime_macos_login.rs"]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) mod macos_login;
mod payload_repair;
mod retirement;
pub(crate) use retirement::uninstall as uninstall_receipt;
mod snapshot;
mod staging;

const MAX_MANIFEST: u64 = 512 * 1024;
/// Typed refusal for `adopt-legacy-index(es)`: the 1.8.16 clean-break baseline
/// rebuilds from original sources and never adopts standalone saved indexes.
const STANDALONE_ADOPTION_UNSUPPORTED: &str = "standalone_adoption_unsupported: AIRK no longer adopts standalone saved indexes; set up a new index from the original sources in Indexing";
/// Typed refusal for every in-place P5 catalog upgrade entry point: a raw
/// registry is never migrated in place; the 1.8.16 clean break creates fresh
/// state with Genesis instead.
const CATALOG_UPGRADE_UNSUPPORTED: &str =
    "catalog_upgrade_unsupported: AIRK no longer upgrades a registry in place; run the clean break";
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
    /// Explicitly stage, commit or finalize an authenticated local issuer rotation.
    HttpIssuerRotation {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long, value_enum)]
        action: HttpIssuerRotationAction,
        #[arg(long)]
        operation_id: Option<airk_core::identity::Incarnation>,
        #[arg(long)]
        expected_ca_sha256: Option<airk_core::identity::Sha256Digest>,
        #[arg(long)]
        expected_descriptor_sha256: Option<airk_core::identity::Sha256Digest>,
        #[arg(long)]
        expected_receipt_sha256: Option<airk_core::identity::Sha256Digest>,
        #[arg(long, default_value_t = 86400, value_parser = clap::value_parser!(u32).range(60..=86400))]
        overlap_seconds: u32,
    },
    /// Read the owner-authenticated local HTTPS endpoint without enrolling a client.
    HttpEndpoint {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Read an existing connection's verified references without issuing credentials.
    HttpConnectionInfo {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        connection_id: airk_core::identity::Incarnation,
    },
    /// Read-only typed connection ownership lookup; revoked or unavailable IDs never become foreign.
    LookupHttpConnection {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_active: String,
        #[arg(long)]
        connection_id: airk_core::identity::Incarnation,
        #[arg(long, value_parser = ["predecessor-cleanup"])]
        purpose: Option<String>,
        #[arg(
            long,
            requires = "purpose",
            required_if_eq("purpose", "predecessor-cleanup")
        )]
        predecessor: Option<String>,
    },
    /// Inspect or explicitly manage this native issuer in the current user's trust store.
    HttpTrust {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long, value_enum)]
        action: HttpTrustAction,
        #[arg(long,required_if_eq_any=[("action","install"),("action","remove")])]
        expected_ca_sha256: Option<airk_core::identity::Sha256Digest>,
    },
    /// Enroll a saved native selection and return protected credential references.
    HttpConnection {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        selection_file: PathBuf,
        #[arg(long)]
        connection_id: airk_core::identity::Incarnation,
        #[arg(long)]
        client: String,
        #[arg(long)]
        explorer_descriptor: Option<PathBuf>,
    },
    /// Revoke one local HTTPS connection while preserving its original receipt.
    HttpRevoke {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        connection_id: airk_core::identity::Incarnation,
    },

    /// Migrate exact owned predecessor configuration using existing imported authority.
    HttpLegacyConnection {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        registration_file: PathBuf,
        #[arg(long)]
        configuration_receipt: airk_core::identity::Sha256Digest,
        /// Exact native original declaration identity when several saved indexes share a configuration file and logical registration.
        #[arg(long)]
        origin_sha256: Option<airk_core::identity::Sha256Digest>,
        #[arg(long)]
        connection_id: airk_core::identity::Incarnation,
        #[arg(long)]
        client: String,
        #[arg(long)]
        explorer_descriptor: Option<PathBuf>,
        #[arg(long)]
        reattach_connection_id: Option<airk_core::identity::Incarnation>,
    },
    /// Migrate an original no-default multi-project declaration without widening authority.
    HttpLegacyUnionConnection {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        registrations_file: PathBuf,
        #[arg(long)]
        connection_id: airk_core::identity::Incarnation,
        #[arg(long)]
        client: String,
        #[arg(long)]
        explorer_descriptor: Option<PathBuf>,
    },
    /// Read one exact saved source scope, without electing a service or opening an index.
    SourceScope {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_control: PathBuf,
        #[arg(long)]
        expected_runtime: String,
        #[arg(long)]
        source_root: PathBuf,
        #[arg(long)]
        index_collection: Option<String>,
    },

    /// Run one externally supervised Linux container service from an approved read-only image pair.
    ContainerHost {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        approved_sha256: String,
        #[arg(long, value_enum, default_value = "stable")]
        channel: RuntimeChannel,
    },
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
    #[command(hide = true)]
    ManagedServe {
        #[arg(long)]
        install_dir: PathBuf,
    },
    #[command(hide = true)]
    LoginForeground {
        #[arg(long, value_enum)]
        channel: RuntimeChannel,
    },
    #[command(hide = true)]
    LoginServe {
        #[arg(long, value_enum)]
        channel: RuntimeChannel,
    },
    #[command(hide = true)]
    MacLogin {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        runtime_sha256: String,
        #[arg(long, value_enum)]
        channel: RuntimeChannel,
        #[arg(long)]
        action: String,
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
        #[arg(long, requires = "expected_active")]
        expected_candidate: Option<String>,
        #[arg(long, requires = "expected_candidate")]
        expected_active: Option<String>,
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
    /// Create an owned lightweight client launcher, including for a read-only managed payload.
    PrepareClient {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Drain the selected service and select a staged runtime only after native health succeeds.
    Activate {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        runtime_sha256: String,
        /// Refuse if the reviewed preceding selection changed; exact completed replay does not start a service.
        #[arg(long)]
        expected_active: Option<String>,
        /// Preserve previous state and automatically rebuild indexes in fresh storage from saved sources.
        #[arg(long, requires = "expected_active")]
        rebuild_indexes: bool,
    },
    /// Verify one immutable approved candidate without selecting or executing it.
    InspectCandidate {
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
    /// Read-only inventory of AIRK-owned per-user data and recorded project index roots.
    Inventory {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Preview, then with `--confirm <plan_digest>` delete, the contents of selected owned
    /// log/cache locations. Project indexes are removed only by unregistering projects.
    Purge {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long = "select", required = true)]
        select: Vec<String>,
        #[arg(long)]
        confirm: Option<String>,
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
    /// After configuration migration, remove all approved superseded payloads for HTTPS runtimes; legacy runtimes retire only the previous rollback payload. Preserve active runtime, User Stop and project data.
    RetirePrevious {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_active: String,
    },
    /// After HTTPS configuration migration, stop and remove only owned legacy launcher images and receipts.
    RetireLegacyLaunchers {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_active: String,
    },
    /// Read-only verification of retained legacy launcher configuration lineage.
    InspectLegacyLaunchers {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_active: String,
        #[arg(long, value_parser = ["predecessor-cleanup"])]
        purpose: Option<String>,
    },
    /// Read-only ownership inventory of obsolete extension helper caches.
    InspectLegacyCaches {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_active: String,
        #[arg(long)]
        global_storage: PathBuf,
        #[arg(long)]
        origin_locators_file: Option<PathBuf>,
    },
    /// Freeze the exact owned helper-cache inventory before changing MCP configurations.
    PrepareLegacyCaches {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_active: String,
        #[arg(long)]
        global_storage: PathBuf,
        #[arg(long)]
        origin_locators_file: Option<PathBuf>,
    },
    /// Explicitly re-review a pending cache inventory after newly appeared owned caches; preserve original proofs and require configuration migration against the new inspection.
    ReprepareLegacyCaches {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_active: String,
        #[arg(long)]
        global_storage: PathBuf,
        #[arg(long)]
        origin_locators_file: Option<PathBuf>,
        #[arg(long)]
        expected_inspection_sha256: String,
    },
    /// After HTTPS migration, stop and remove only caches in the accepted native ownership inventory.
    RetireLegacyCaches {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_active: String,
        #[arg(long)]
        expected_inspection_sha256: String,
    },
    /// Resume only an interrupted standalone adoption; a new one is refused (standalone_adoption_unsupported).
    AdoptLegacyIndex {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_active: String,
        #[arg(long)]
        operation_id: String,
        #[arg(long)]
        locator_file: PathBuf,
        #[arg(long)]
        expected_cache_inspection_sha256: String,
    },
    /// Resume only an interrupted standalone adoption batch; a new one is refused (standalone_adoption_unsupported).
    AdoptLegacyIndexes {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        expected_active: String,
        #[arg(long)]
        operation_id: String,
        #[arg(long)]
        locators_file: PathBuf,
        #[arg(long)]
        expected_cache_inspection_sha256: String,
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
    /// Refused (catalog_upgrade_unsupported): in-place catalog upgrade review was removed; run the clean break.
    LegacyRetirementCandidates {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        after: Option<airk_core::index_catalog::IndexId>,
    },
    /// Refused (catalog_upgrade_unsupported): in-place catalog upgrade review was removed; run the clean break.
    LegacyRetirementReview {
        #[arg(long)]
        install_dir: PathBuf,
        #[arg(long)]
        index_id: airk_core::index_catalog::IndexId,
    },
    /// Refused (catalog_upgrade_unsupported): in-place catalog upgrade retirement was removed; run the clean break.
    LegacyRetirementAuthorize {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Refused (catalog_upgrade_unsupported): in-place catalog upgrade retirement was removed; run the clean break.
    LegacyRetirementReceipt {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Refused (catalog_upgrade_unsupported): in-place catalog upgrade retirement was removed; run the clean break.
    LegacyRetirementResume {
        #[arg(long)]
        install_dir: PathBuf,
    },
    /// Bounded executable/paired-helper self-test, without collection access or inference.
    SelfTest,
    /// Refused (catalog_upgrade_unsupported): the in-place catalog upgrade was removed.
    #[command(hide = true)]
    CatalogUpgradeProbe {
        #[arg(long)]
        control_root: PathBuf,
        #[arg(long)]
        upgrade_id: airk_core::identity::Incarnation,
        #[arg(long)]
        nonce: airk_core::identity::Incarnation,
    },
    /// Installer-only, nonce-bound observation of the promoted inactive catalog.
    #[command(hide = true)]
    CatalogAuthorityProbe {
        #[arg(long)]
        control_root: PathBuf,
        #[arg(long)]
        operation: airk_core::identity::Incarnation,
        /// Refused (catalog_upgrade_unsupported): only a removed in-place
        /// legacy upgrade supplied it; fresh Genesis omits it.
        #[arg(long)]
        native_host: Option<airk_core::identity::Sha256Digest>,
        #[arg(long)]
        nonce: airk_core::identity::Incarnation,
    },
    /// Refused (catalog_upgrade_unsupported): the in-place catalog upgrade was removed.
    #[command(hide = true)]
    CatalogUpgradeMigrate {
        #[arg(long)]
        control_root: PathBuf,
        #[arg(long)]
        upgrade_id: airk_core::identity::Incarnation,
    },
}

pub(crate) fn validate_control_root(control: &Path) -> Result<()> {
    layout::validate_control(control)?;
    layout::require_managed_control_ready(control)?;
    airk_store::service::registry::data_relocation::require_settled(control)
}
pub(crate) fn require_baseline_control(control: &Path) -> Result<()> {
    state_contract::require_control(control)
}
pub(crate) fn require_baseline_service(control: &Path) -> Result<()> {
    state_contract::require_service(control)
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
    Ok(discover_user_paths(RuntimeChannel::Stable)?.install)
}

/// An owned stable launcher also identifies custom installations without PATH or environment guesses.
pub fn caller_install_root() -> Result<PathBuf> {
    let executable = std::env::current_exe()?.canonicalize()?;
    if executable.file_name()
        == Some(std::ffi::OsStr::new(if cfg!(windows) {
            "airk-launcher.exe"
        } else {
            "airk-launcher"
        }))
    {
        let root = executable.parent().context("launcher parent missing")?;
        ensure!(
            verified_launcher(root)?.canonicalize()? == executable,
            "launcher installation changed"
        );
        return Ok(root.to_path_buf());
    }
    let payload = runtime_pair::payload_root(&executable)?;
    if let Some(runtimes) = payload
        .parent()
        .filter(|path| path.file_name() == Some("runtimes".as_ref()))
    {
        let root = runtimes
            .parent()
            .context("runtime installation parent missing")?;
        let identity = payload
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid immutable runtime identity")?;
        digest(identity)?;
        let (_, selected_main, approval) = selected(root)?;
        ensure!(
            approval.manifest_sha256 == identity && selected_main.canonicalize()? == executable,
            "running main is not the exact approved selected runtime"
        );
        return Ok(root.to_path_buf());
    }
    default_install_root()
}

pub fn default_project_data(root: &Path) -> Result<PathBuf> {
    if let Some(data) = activation::rebuild::successor_default_data(root)? {
        return Ok(data);
    }
    let (_, channel) = layout::retained_parameters(root)?;
    Ok(discover_user_paths(channel)?.data)
}

/// Installed frontends use the elected broker and direct project transport.
/// They never initialize a local Kernel if installation, policy or health fails.
pub async fn bridge_installed(
    root: &Path,
    project: airk_core::service::ProjectBinding,
) -> Result<()> {
    crate::project_client::bridge(installed_control(root).await?, project).await
}

/// Verify the selected runtime and elected service without opening a collection.
pub async fn installed_control(root: &Path) -> Result<PathBuf> {
    // Keep managed policies on their original complete preflight/ensure route in
    // this slice. An external supervisor must never fall back to user election.
    if let Some(policy) = managed::read(root)? {
        return installed_control_legacy(root, policy.approval.clone()).await;
    }
    if layout::managed_origin(root)?.is_some() {
        ensure_running(root).await?;
        bail!("repeat_same_approved_managed_configure_to_resume");
    }
    let preflight = selected_preflight(root)?;
    let lock = connection_install_lock(root, Duration::from_secs(60)).await?;
    let authority = InstallerAuthority { root, _lock: &lock };
    let prepared = prepare_connection(&authority, &preflight)?;
    let health = match prepared {
        PreparedConnection::NoEffects(verified) => activation::start_verified(&verified).await?,
        PreparedConnection::SettlementRequired => {
            // No earlier proof survives lifecycle/activation/uncertain work.
            ensure_running_locked(&authority).await?
        }
    };
    require_matching_health(&preflight.approval, &health)?;
    let control = layout::control(root)?;
    ensure!(
        control == preflight.control,
        "selected service control changed"
    );
    Ok(control)
}

async fn installed_control_legacy(root: &Path, approval: Approval) -> Result<PathBuf> {
    ensure!(
        approval.version == env!("CARGO_PKG_VERSION"),
        "matching runtime version required"
    );
    let health = ensure_running(root).await?;
    require_matching_health(&approval, &health)?;
    layout::control(root)
}

fn require_matching_health(approval: &Approval, health: &ServiceHealth) -> Result<()> {
    ensure!(
        health.version == approval.version
            && health.runtime_sha256.as_deref() == Some(approval.manifest_sha256.as_str()),
        "installed service changed; reconnect through the selected launcher"
    );
    Ok(())
}

/// Validate frontend selection metadata without authorizing executable payloads.
pub fn installed_metadata_control(root: &Path) -> Result<PathBuf> {
    // External policy can be pinned before its supervisor starts the service.
    // Selection is published only after matching health, so the first client
    // must validate the pin before asking ensure to observe that health.
    let approval = match managed::read(root)? {
        // Provider pin/metadata remains authoritative without reading actual
        // images or model assets until the first native request.
        Some(policy) => policy.metadata_approval()?,
        // This authenticates bounded manifest/pair metadata and the selected
        // identity. It is not a VerifiedSelection and cannot authorize a start.
        None => selected_preflight(root)?.approval,
    };
    ensure!(
        approval.version == env!("CARGO_PKG_VERSION"),
        "matching runtime version required"
    );
    layout::control(root)
}

pub(crate) fn saved_control_root(root: &Path) -> Result<PathBuf> {
    layout::saved_control(root)
}

/// Installer-owned facts consulted by user path discovery.
pub(crate) const INSTALLATION_PROBE: crate::user_paths::InstallationProbe =
    crate::user_paths::InstallationProbe {
        saved_control: saved_control_root,
        universal_selection: crate::universal_launcher::configured_selection,
    };

/// Native user paths for `channel`, with this installer's layout facts.
pub(crate) fn discover_user_paths(channel: RuntimeChannel) -> Result<crate::user_paths::UserPaths> {
    crate::user_paths::UserPaths::discover(channel, INSTALLATION_PROBE)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum HttpIssuerRotationAction {
    Status,
    Prepare,
    TrustNew,
    Commit,
    Finalize,
    Abort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum HttpTrustAction {
    Inspect,
    Install,
    Remove,
}

#[derive(Subcommand)]
pub enum LoginCommand {
    /// Show what login startup registration would create; changes nothing.
    Plan,
    /// Report whether login startup is registered, loaded and running.
    Status,
    /// Register the service to start at login for this user.
    Install,
    /// Start the registered login service now.
    Start,
    /// Stop the registered login service.
    Stop,
    /// Remove the login startup registration.
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

/// Preserve reset semantics for damaged receipts; only healthy no-ops avoid publication.
fn clear_startup_attempts(root: &Path, identity: &str) -> Result<()> {
    let name = format!("startup-{identity}.json");
    let path = root.join(&name);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    // A dangling link is a present entry: retain the old guarded read/replacement path.
    // Previously this path replaced any safe existing receipt without parsing it.
    // Failed reads/parses must therefore retain that replacement/error behavior.
    if read_bounded(&path, 4096)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StartupAttempts>(&bytes).ok())
        .is_some_and(|state| state.attempts_unix_ms.is_empty())
    {
        return Ok(());
    }
    replace_json(root, &name, &StartupAttempts::default())
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
    let manifest = runtime_pair::payload_root(main)?.join("airk-install.json");
    if !manifest.try_exists()? {
        return Ok(None);
    }
    let bytes = read_bounded(&manifest, MAX_MANIFEST)?;
    Ok(Some(format!("{:x}", Sha256::digest(bytes))))
}

/// The separately signed Mac image survives copying outside the sealed bundle.
/// Legacy payloads keep their original inventory and launcher source.
fn standalone_launcher_name(os: &str, version: &str) -> Result<Option<&'static str>> {
    if os != "macos" {
        return Ok(None);
    }
    let numbers = version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(numbers.len() == 3, "invalid Mac runtime version");
    Ok(
        (numbers.as_slice() >= [1, 8, 8].as_slice() && numbers.as_slice() < [1, 8, 14].as_slice())
            .then_some(MAC_LAUNCHER_IMAGE),
    )
}

/// Call only with the inventory from the same completed approved payload check.
/// Recheck the selected image at its use boundary; its own file cannot grant approval.
fn launcher_image(directory: &Path, inventory: &InstallManifest) -> Result<PathBuf> {
    let name = standalone_launcher_name(&inventory.os, &inventory.version)?.unwrap_or(main_name());
    let expected = inventory
        .files
        .get(name)
        .context("approved launcher image missing")?;
    let source = payload_path(directory, name)?;
    safe_file(&source)?;
    ensure!(
        runtime_pair::digest_file(&source)? == *expected,
        "approved launcher image digest mismatch"
    );
    Ok(source)
}
fn helper_name() -> &'static str {
    if cfg!(windows) {
        "airk-embed.exe"
    } else if cfg!(target_os = "macos") {
        "AIRK.app/Contents/MacOS/airk-embed"
    } else {
        "airk-embed"
    }
}
fn open_install_lock(root: &Path) -> Result<File> {
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
    Ok(file)
}

fn install_lock(root: &Path) -> Result<File> {
    let file = open_install_lock(root)?;
    file.try_lock()
        .context("another AIRK installer/launcher is active; retry after it finishes")?;
    Ok(file)
}

/// Read-only observations may lock an existing installation, never initialize it.
fn existing_install_lock(root: &Path) -> Result<File> {
    ensure!(root.is_absolute(), "installation location must be absolute");
    let text = root.to_str().context("installation path must be UTF-8")?;
    ensure!(
        text.len() <= 4096 && !text.chars().any(char::is_control),
        "installation path exceeds native identity bound"
    );
    local_ipc::require_existing_root(root)?;
    let lock = root.join("install-1.4.lock");
    safe_file(&lock)?;
    ensure!(fs::metadata(&lock)?.len() == 0, "invalid install lock");
    let file = OpenOptions::new().read(true).write(true).open(&lock)?;
    safe_file(&lock)?;
    file.try_lock()
        .context("another AIRK installer/launcher is active; retry after it finishes")?;
    Ok(file)
}

async fn connection_install_lock(root: &Path, budget: Duration) -> Result<File> {
    let file = open_install_lock(root)?;
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(error) => {
                return Err(error).context("service connection installer wait expired or failed");
            }
        }
    }
}

#[cfg(test)]
mod connection_lock_tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_connections_wait_without_stealing_mutation_ownership() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("installation");
        let held = install_lock(&root)?;
        let wait = connection_install_lock(&root, Duration::from_millis(100));
        assert!(wait.await.is_err());
        assert!(install_lock(&root).is_err());
        let release = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(held);
        });
        let acquired = connection_install_lock(&root, Duration::from_secs(5)).await?;
        release.await?;
        assert!(install_lock(&root).is_err());
        drop(acquired);
        install_lock(&root)?;
        fs::write(root.join("install-1.4.lock"), b"foreign bytes")?;
        assert!(
            connection_install_lock(&root, Duration::from_secs(5))
                .await
                .is_err()
        );
        Ok(())
    }
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
    manifest_with_hasher(root, approved, runtime_pair::digest_file)
}

// Test injection counts actual inventory hash operations without process-global
// mutable state; production always supplies the existing guarded file hasher.
// Bounded metadata authentication only; no payload file hashes or execution.
fn manifest_metadata(root: &Path, approved: &str) -> Result<(Vec<u8>, InstallManifest)> {
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
    for name in [main_name(), "airk-runtime.json", "LICENSE"] {
        ensure!(
            manifest.files.contains_key(name),
            "offline payload is missing {name}"
        );
    }
    let pair: runtime_pair::RuntimePair =
        serde_json::from_slice(&read_bounded(&root.join("airk-runtime.json"), 4096)?)?;
    pair.validate_target(&manifest.version)?;
    if pair.is_single_executable() {
        ensure!(
            !manifest.files.contains_key(helper_name())
                && !manifest.files.contains_key(MAC_LAUNCHER_IMAGE)
                && !manifest.files.keys().any(|name| matches!(
                    Path::new(name).file_name().and_then(|n| n.to_str()),
                    Some(
                        "airk-launcher"
                            | "airk-launcher.exe"
                            | "airk-service"
                            | "airk-service.exe"
                            | "airk-index"
                            | "airk-index.exe"
                            | "airk-source"
                            | "airk-source.exe"
                            | "onnxruntime.dll"
                            | "libonnxruntime.so"
                            | "libonnxruntime.dylib"
                    )
                )),
            "single executable payload contains a legacy runtime process or dynamic inference library"
        );
    } else {
        ensure!(
            manifest.files.contains_key(helper_name()),
            "offline payload is missing inference helper"
        );
    }
    if let Some(name) = standalone_launcher_name(&manifest.os, &manifest.version)? {
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
    Ok((bytes, manifest))
}

fn validate_inventory_structure(root: &Path, manifest: &InstallManifest) -> Result<()> {
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
    Ok(())
}

fn manifest_with_hasher(
    root: &Path,
    approved: &str,
    hash: impl FnMut(&Path) -> Result<String>,
) -> Result<(Vec<u8>, InstallManifest)> {
    manifest_with_hasher_checked(root, approved, hash, &|| Ok(()))
}

fn manifest_checked(
    root: &Path,
    approved: &str,
    check: &dyn Fn() -> Result<()>,
) -> Result<(Vec<u8>, InstallManifest)> {
    manifest_with_hasher_checked(
        root,
        approved,
        |path| runtime_pair::digest_file_checked(path, check),
        check,
    )
}

fn manifest_with_hasher_checked(
    root: &Path,
    approved: &str,
    mut hash: impl FnMut(&Path) -> Result<String>,
    check: &dyn Fn() -> Result<()>,
) -> Result<(Vec<u8>, InstallManifest)> {
    check()?;
    let (bytes, manifest) = manifest_metadata(root, approved)?;
    validate_inventory_structure(root, &manifest)?;
    let mut total = 0u64;
    for (name, expected) in &manifest.files {
        check()?;
        digest(expected)?;
        ensure!(!staging::reserved(name), "reserved payload name");
        let file = payload_path(root, name)?;
        safe_file(&file)?;
        total = total
            .checked_add(fs::metadata(&file)?.len())
            .context("runtime payload byte overflow")?;
        ensure!(total <= MAX_PAYLOAD, "runtime payload exceeds disk budget");
        // Keep every listed path/leaf/size/budget check here. Images are hashed
        // once at the final pair checkpoint rather than earlier AND there.
        if name != main_name() && name != helper_name() {
            ensure!(
                hash(&file)? == *expected,
                "runtime payload digest mismatch: {name}"
            );
        }
    }
    // Preserve the old final image verification position and guards. Do not
    // trust a digest captured earlier in inventory traversal: another listed
    // file's processing could precede a changed image. The same real hasher
    // counts ALL manifest-owned image reads, with no hidden pair rehash.
    check()?;
    let (_, _, pair) =
        runtime_pair::read_installed_pair(&root.join(main_name()), &manifest.version)?;
    let main = payload_path(root, main_name())?;
    safe_file(&main)?;
    let main_hash = hash(&main)?;
    ensure!(
        main_hash == manifest.files[main_name()],
        "runtime payload digest mismatch: {}",
        main_name()
    );
    ensure!(
        main_hash == pair.airk_sha256,
        "runtime executable pair digest mismatch"
    );
    if !pair.is_single_executable() {
        let helper = payload_path(root, helper_name())?;
        safe_file(&helper)?;
        let helper_hash = hash(&helper)?;
        ensure!(
            helper_hash == manifest.files[helper_name()],
            "runtime payload digest mismatch: {}",
            helper_name()
        );
        ensure!(
            helper_hash == pair.embed_sha256,
            "runtime executable pair digest mismatch"
        );
    }
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

fn prepare_launcher(root: &Path, main: &Path, inventory: &InstallManifest) -> Result<()> {
    if runtime_pair::single_executable(main)? {
        return Ok(());
    }
    let image = launcher_image(runtime_pair::payload_root(main)?, inventory)?;
    let main = image.as_path();
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
    ensure!(
        sha256 == expected,
        "approved main changed while preparing the client launcher"
    );
    temporary.persist_noclobber(&destination)?;
    replace_json(
        root,
        "launcher.json",
        &LauncherReceipt { schema: 1, sha256 },
    )
}

fn prepare_client_launcher(
    root: &Path,
    main: &Path,
    approval: &Approval,
    inventory: &InstallManifest,
) -> Result<()> {
    if runtime_pair::single_executable(main)? {
        // A predecessor launcher remains owned migration state until its old
        // client configurations are retired; never create or replace one here.
        return Ok(());
    }
    if standalone_launcher_name(&inventory.os, &inventory.version)?.is_some() {
        let image = launcher_image(runtime_pair::payload_root(main)?, inventory)?;
        launcher_repair::repair_client_verified(root, &approval.manifest_sha256, &image)
    } else {
        launcher_repair::require_settled(root)?;
        prepare_launcher(root, main, inventory)
    }
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
    let (main, approval, _) = verify_runtime_inventory(root, identity)?;
    Ok((main, approval))
}

fn verify_runtime_inventory(
    root: &Path,
    identity: &str,
) -> Result<(PathBuf, Approval, InstallManifest)> {
    verify_runtime_inventory_checked(root, identity, &|| Ok(()))
}

fn verify_runtime_inventory_checked(
    root: &Path,
    identity: &str,
    check: &dyn Fn() -> Result<()>,
) -> Result<(PathBuf, Approval, InstallManifest)> {
    check()?;
    digest(identity)?;
    ensure!(root.is_dir(), "runtime installation is missing");
    local_ipc::prepare_root(root)?;
    if let Some(policy) = managed::read(root)? {
        ensure!(
            policy.approval.manifest_sha256 == identity,
            "managed runtime is pinned to another approved identity"
        );
        return policy.verify_inventory_checked(check);
    }
    let (main, approval, inventory) = owned_inventory_checked(root, identity, check)?;
    managed::verify_topology_checked(runtime_pair::payload_root(&main)?, &inventory, true, check)?;
    Ok((main, approval, inventory))
}

/// Ownership proof for bounded retirement. Unknown files still prevent execution,
/// but do not prevent saving the inventory needed to resume their safe retirement.
fn owned_inventory(root: &Path, identity: &str) -> Result<(PathBuf, Approval, InstallManifest)> {
    owned_inventory_checked(root, identity, &|| Ok(()))
}

fn owned_inventory_checked(
    root: &Path,
    identity: &str,
    check: &dyn Fn() -> Result<()>,
) -> Result<(PathBuf, Approval, InstallManifest)> {
    check()?;
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
    let (_, manifest) = manifest_checked(&directory, identity, check)?;
    ensure!(
        manifest.version == approval.version,
        "runtime approval version mismatch"
    );
    Ok((directory.join(main_name()), approval, manifest))
}

pub fn selected(root: &Path) -> Result<(Selection, PathBuf, Approval)> {
    let (selection, main, approval, _) = selected_inventory(root)?;
    Ok((selection, main, approval))
}

fn guarded_selection(root: &Path) -> Result<Selection> {
    payload_repair::require_settled(root)?;
    ensure!(root.is_dir(), "runtime installation is missing");
    local_ipc::require_existing_root(root)?;
    ensure!(
        !root.join("uninstall.json").try_exists()?,
        "runtime uninstall is in progress; resume uninstall before installing or launching"
    );
    let selection = selection_record(root)?;
    Ok(selection)
}

fn selected_inventory(root: &Path) -> Result<(Selection, PathBuf, Approval, InstallManifest)> {
    let selection = guarded_selection(root)?;
    let (main, approval, inventory) = verify_runtime_inventory(root, &selection.active)?;
    Ok((selection, main, approval, inventory))
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
    self_test_observation(main).map(|_| ())
}

fn self_test_observation(main: &Path) -> Result<serde_json::Value> {
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
    if !runtime_pair::single_executable(main)? {
        runtime_pair::validate_native_image(
            &main.parent().context("runtime directory missing")?.join(
                Path::new(helper_name())
                    .file_name()
                    .context("helper name missing")?,
            ),
        )?;
    }
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
                &runtime_pair::payload_root(main)?.join("airk-runtime.json"),
                4096,
            )?)?;
            validate_self_test_result(&bytes, &pair.version)?;
            return Ok(serde_json::from_slice(&bytes)?);
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
    let observation = health_identity(root, approval, expected_sha256).await?;
    ensure!(observation.policy.accepting, "service is draining");
    Ok(observation)
}

/// Authenticate a lifecycle owner even when its checkpoint deliberately closes
/// admission. This is not proof that a service can accept client work.
async fn health_identity(
    root: &Path,
    approval: &Approval,
    expected_sha256: &str,
) -> Result<ServiceHealth> {
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
    Ok(*observation)
}

async fn start(
    root: &Path,
    approval: &Approval,
    main: &Path,
    enforce_backoff: bool,
) -> Result<ServiceHealth> {
    start_with_digest(root, approval, main, enforce_backoff, None).await
}

async fn start_with_digest(
    root: &Path,
    approval: &Approval,
    main: &Path,
    enforce_backoff: bool,
    verified_digest: Option<&str>,
) -> Result<ServiceHealth> {
    start_with_digest_checked(root, approval, main, enforce_backoff, verified_digest, None).await
}

async fn start_with_digest_checked(
    root: &Path,
    approval: &Approval,
    main: &Path,
    enforce_backoff: bool,
    verified_digest: Option<&str>,
    cancel: Option<&setup::SetupCancellation>,
) -> Result<ServiceHealth> {
    if let Some(cancel) = cancel {
        cancel.checkpoint()?;
    }
    let control = layout::control(root)?;
    local_ipc::prepare_root(&control)?;
    let expected_sha256 = match verified_digest {
        Some(digest) => digest.to_owned(),
        None => runtime_pair::digest_file_checked(main, &|| {
            cancel.map_or(Ok(()), setup::SetupCancellation::checkpoint)
        })?,
    };
    start_with_observation(
        root,
        approval,
        main,
        enforce_backoff,
        cancel,
        &control,
        &expected_sha256,
        health(root, approval, &expected_sha256),
    )
    .await
}

#[allow(clippy::too_many_arguments)] // Keep verified inputs explicit at the injected health boundary.
async fn start_with_observation(
    root: &Path,
    approval: &Approval,
    main: &Path,
    enforce_backoff: bool,
    cancel: Option<&setup::SetupCancellation>,
    control: &Path,
    expected_sha256: &str,
    observation: impl std::future::Future<Output = Result<ServiceHealth>>,
) -> Result<ServiceHealth> {
    if cancel.is_some() {
        ensure!(
            enforce_backoff,
            "session startup requires a durable startup receipt"
        );
    }
    let (observation, _attempt) = setup::observe_or_effect(observation, cancel, || {
        if enforce_backoff {
            startup_attempt(root, &approval.manifest_sha256, unix_ms()?)
        } else {
            Ok(())
        }
    })
    .await?;
    if let Some(observation) = observation {
        if observation.uptime_ms >= 60_000 {
            if let Some(cancel) = cancel {
                cancel.effect(|| clear_startup_attempts(root, &approval.manifest_sha256))?;
            } else {
                clear_startup_attempts(root, &approval.manifest_sha256)?;
            }
        }
        return Ok(observation);
    }
    // The exact receipt-backed attempt above now settles once, even after EOF.
    let service_executable =
        crate::process_roles::executable(main, crate::process_roles::Role::Service)?;
    let mut command = Command::new(&service_executable);
    command
        .arg("serve")
        .arg("--runtime-dir")
        .arg(control)
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
        &service_executable,
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
        if let Ok(observation) = health(root, approval, expected_sha256).await {
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

// This capability is deliberately neither Clone nor serializable. Its borrow
// keeps the exact installer lock/root authority alive until startup completes.
// It is minted only after full current inventory AND topology verification.
struct InstallerAuthority<'a> {
    root: &'a Path,
    _lock: &'a File,
}
struct VerifiedSelection<'a> {
    authority: &'a InstallerAuthority<'a>,
    selection: Selection,
    main: PathBuf,
    approval: Approval,
    main_sha256: String,
}
// This is metadata, NOT a proof/capability and never authorizes process start.
// It is deliberately private, non-Clone and cannot be supplied over a protocol.
struct SelectedPreflight {
    root: PathBuf,
    control: PathBuf,
    approval: Approval,
}
/// Bounded authenticated delivery metadata only; never an execution capability.
fn approved_inventory_metadata(directory: &Path, approval: &Approval) -> Result<InstallManifest> {
    ensure!(approval.schema == 1, "runtime approval identity mismatch");
    let (_, inventory) = manifest_metadata(directory, &approval.manifest_sha256)?;
    ensure!(
        inventory.version == approval.version,
        "runtime approval version mismatch"
    );
    validate_inventory_structure(directory, &inventory)?;
    for (name, hash) in &inventory.files {
        digest(hash)?;
        ensure!(!staging::reserved(name), "reserved payload name");
    }
    // Authenticate the small pair receipt too, without reading executable bytes.
    // Metadata corruption remains pre-wait; ancillary image/file corruption is
    // intentionally checked only by the complete locked pass below.
    let bytes = read_bounded(&directory.join("airk-runtime.json"), 4096)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == inventory.files["airk-runtime.json"],
        "runtime pair receipt differs from approved inventory"
    );
    let pair: runtime_pair::RuntimePair = serde_json::from_slice(&bytes)?;
    pair.validate_target(&inventory.version)?;
    ensure!(
        pair.airk_sha256 == inventory.files[main_name()]
            && (pair.is_single_executable() || pair.embed_sha256 == inventory.files[helper_name()]),
        "runtime executable pair digest mismatch"
    );
    Ok(inventory)
}

fn selected_preflight(root: &Path) -> Result<SelectedPreflight> {
    let selection = guarded_selection(root)?;
    ensure!(
        managed::read(root)?.is_none(),
        "installed service changed; reconnect through the selected launcher"
    );
    let directory = root.join("runtimes").join(&selection.active);
    ensure!(directory.is_dir(), "approved runtime is not staged");
    local_ipc::require_existing_root(&directory)?;
    let approval: Approval =
        serde_json::from_slice(&read_bounded(&directory.join("approval.json"), 4096)?)?;
    ensure!(
        approval.schema == 1 && approval.manifest_sha256 == selection.active,
        "runtime approval identity mismatch"
    );
    approved_inventory_metadata(&directory, &approval)?;
    ensure!(
        approval.version == env!("CARGO_PKG_VERSION"),
        "matching runtime version required"
    );
    Ok(SelectedPreflight {
        root: root.canonicalize()?,
        control: layout::saved_control(root)?,
        approval,
    })
}

enum PreparedConnection<'a> {
    NoEffects(VerifiedSelection<'a>),
    SettlementRequired,
}
fn prepare_connection<'a>(
    authority: &'a InstallerAuthority<'a>,
    preflight: &SelectedPreflight,
) -> Result<PreparedConnection<'a>> {
    prepare_connection_with(authority, preflight, selected_verified)
}
// A local callback counts actual complete verification entries in focused tests;
// production always invokes the real full selected verifier, not saved metadata.
fn prepare_connection_with<'a>(
    authority: &'a InstallerAuthority<'a>,
    preflight: &SelectedPreflight,
    mut verify: impl FnMut(&'a InstallerAuthority<'a>) -> Result<VerifiedSelection<'a>>,
) -> Result<PreparedConnection<'a>> {
    let root = authority.root;
    ensure!(
        root.canonicalize()? == preflight.root
            && layout::saved_control(root)? == preflight.control
            && managed::read(root)?.is_none(),
        "installed service changed; reconnect through the selected launcher"
    );
    // Full CURRENT validation precedes all connection lifecycle effects, even
    // when metadata preflight qualified a formerly selected identity before wait.
    let verified = verify(authority)?;
    ensure!(
        verified.approval.version == preflight.approval.version
            && verified.selection.active == preflight.approval.manifest_sha256,
        "installed service changed; reconnect through the selected launcher"
    );
    let settlement = lifecycle::connection_settlement(root)?;
    if matches!(settlement, lifecycle::ConnectionSettlement::EffectsRequired)
        || activation::pending(root)?.is_some()
        || activation::rebuild::pending(root)?
    {
        // This includes unreadable/legacy saved state and pending same-identity
        // restart/stop. Success or unchanged selected.json is not a no-op witness.
        drop(verified);
        return Ok(PreparedConnection::SettlementRequired);
    }
    launcher_repair::require_settled(root)?;
    Ok(PreparedConnection::NoEffects(verified))
}

fn selected_verified<'a>(authority: &'a InstallerAuthority<'a>) -> Result<VerifiedSelection<'a>> {
    selected_verified_checked(authority, &|| Ok(()))
}

fn selected_verified_checked<'a>(
    authority: &'a InstallerAuthority<'a>,
    check: &dyn Fn() -> Result<()>,
) -> Result<VerifiedSelection<'a>> {
    check()?;
    let selection = guarded_selection(authority.root)?;
    let (main, approval, inventory) =
        verify_runtime_inventory_checked(authority.root, &selection.active, check)?;
    ensure!(
        selection.active == approval.manifest_sha256,
        "runtime selection identity mismatch"
    );
    let main_sha256 = inventory.files[main_name()].clone();
    Ok(VerifiedSelection {
        authority,
        selection,
        main,
        approval,
        main_sha256,
    })
}

pub async fn ensure_running(root: &Path) -> Result<ServiceHealth> {
    let lock = connection_install_lock(root, Duration::from_secs(60)).await?;
    let authority = InstallerAuthority { root, _lock: &lock };
    ensure_running_locked(&authority).await
}

async fn ensure_running_locked(authority: &InstallerAuthority<'_>) -> Result<ServiceHealth> {
    ensure_running_locked_checked(authority, None).await
}
// Preserve the real post-settlement F03 qualification gate for unrestricted callers.
#[cfg(test)]
async fn ensure_running_locked_after_settlement(
    authority: &InstallerAuthority<'_>,
    after_settlement: impl FnOnce() -> Result<()>,
) -> Result<ServiceHealth> {
    ensure_running_locked_checked_after_settlement(authority, None, after_settlement).await
}

async fn ensure_running_locked_checked(
    authority: &InstallerAuthority<'_>,
    cancel: Option<&setup::SetupCancellation>,
) -> Result<ServiceHealth> {
    ensure_running_locked_checked_after_settlement(authority, cancel, || Ok(())).await
}

async fn ensure_running_locked_checked_after_settlement(
    authority: &InstallerAuthority<'_>,
    cancel: Option<&setup::SetupCancellation>,
    after_settlement: impl FnOnce() -> Result<()>,
) -> Result<ServiceHealth> {
    if let Some(cancel) = cancel {
        cancel.checkpoint()?;
    }

    let root = authority.root;
    // Preserve direct callers' settlement order: a pending STOP authenticates
    // saved identity and must not require a runnable payload just to drain it.
    payload_repair::require_settled(root)?;
    if activation::rebuild::pending(root)? {
        setup::settle_then_checkpoint(
            async { activation::rebuild::resume(root).await.map(|_| ()) },
            cancel,
        )
        .await?;
    }
    managed::catalog::resume_before_lifecycle(authority, cancel)?;
    setup::settle_then_checkpoint(
        async {
            activation::resume_catalog_before_lifecycle(root, None)
                .await
                .map(|_| ())
        },
        cancel,
    )
    .await?;
    setup::settle_then_checkpoint(lifecycle::settle(authority), cancel).await?;
    if managed::read(root)?.is_some() {
        return managed::ensure_checked(authority, cancel, after_settlement).await;
    }
    ensure!(
        layout::managed_origin(root)?.is_none(),
        "repeat_same_approved_managed_configure_to_resume"
    );
    if activation::pending(root)?.is_some() {
        if let Some(cancel) = cancel {
            cancel.checkpoint()?;
        }
        // resume authenticates the saved exact Intent under this installer lock.
        setup::settle_then_checkpoint(activation::resume(root), cancel).await?;
    }
    launcher_repair::require_settled(root)?;
    after_settlement()?;
    let check = || cancel.map_or(Ok(()), setup::SetupCancellation::checkpoint);
    let verified = selected_verified_checked(authority, &check)?;
    if let Some(cancel) = cancel {
        cancel.checkpoint()?;
    }
    activation::start_verified_checked(&verified, cancel).await
}

pub(crate) fn startup_selected(root: &Path) -> Result<(PathBuf, Approval)> {
    payload_repair::require_settled(root)?;
    layout::require_managed_ready(root)?;
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
        let error = match quiescent(root) {
            Ok(guard) => {
                drop(guard);
                return Ok(());
            }
            Err(error) => error,
        };
        if !ownership_contention(&error) || tokio::time::Instant::now() >= until {
            return Err(error).context("service owners have not released after stop");
        }
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
        // Checkpointed owners deliberately close admission before shutdown.
        // Authenticate their identity without reopening or requiring admission.
        let observed = health_identity(root, approval, expected_sha256).await?;
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
        let error = match quiescent(root) {
            Ok(guard) => {
                drop(guard);
                return Ok(());
            }
            Err(error) => error,
        };
        if !ownership_contention(&error) || tokio::time::Instant::now() >= until {
            return Err(error).context("service drain failed; runtime selection preserved");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn ownership_contention(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::fs::TryLockError>()
        .is_some_and(|error| matches!(error, std::fs::TryLockError::WouldBlock))
        || airk_store::service::registry::is_read_contention(error)
}

struct Quiescence {
    catalog: Option<airk_store::service::registry::catalog::CatalogState>,
    _service: airk_store::service::ServiceLease,
    _projects: Vec<airk_store::service::ProjectLease>,
    _embedding: airk_store::service::EmbeddingLease,
    _source_watch: airk_store::service::SourceWatchLease,
}

impl Quiescence {
    /// Called under the same exclusive leases before changing the launcher.
    /// Legacy registries keep their existing path; only an activated new-format
    /// catalog requires an actual candidate capability handshake.
    fn require_catalog_compatible(&self, main: &Path) -> Result<()> {
        let Some(state) = &self.catalog else {
            return Ok(());
        };
        let observation = self_test_observation(main)?;
        require_catalog_capabilities(state, &observation)
    }
}

fn require_catalog_capabilities(
    state: &airk_store::service::registry::catalog::CatalogState,
    observation: &serde_json::Value,
) -> Result<()> {
    let capabilities:airk_core::index_catalog::CatalogCapabilities=serde_json::from_value(observation.get("index_catalog").cloned().context("selected runtime has no index catalog capability; preserve the current payload and resume or repair the upgrade")?)?;
    airk_store::service::registry::runtime_contract::RequiredCapabilities {
        minimum_registry_schema: state.schema,
        authority_contract: state.authority_contract,
        policy_contract: state.policy_contract,
    }
    .require_supported(&capabilities)
    .context("selected runtime cannot enforce the saved index catalog contracts; restore the qualified snapshot before new writes or use forward repair afterwards")
}

fn quiescent(root: &Path) -> Result<Quiescence> {
    let control = layout::control(root)?;
    local_ipc::prepare_root(&control)?;
    let service = airk_store::service::ServiceLease::acquire(&control)?;
    layout::validate_control(&control)?;
    let (bindings, catalog) = if control.join("service-1.4.sqlite3").try_exists()? {
        let registry = airk_store::service::registry::ServiceRegistry::open_saved(&control)?;
        (registry.bindings()?, registry.catalog_state()?)
    } else {
        ensure!(
            !control.join("service-1.4.owner.json").try_exists()?,
            "initialized service registry is missing"
        );
        (Vec::new(), None)
    };
    let projects = bindings
        .into_iter()
        .map(|binding| airk_store::service::ProjectLease::acquire(&control, binding))
        .collect::<Result<Vec<_>>>()?;
    let embedding = airk_store::service::EmbeddingLease::acquire(&control)?;
    let source_watch = airk_store::service::SourceWatchLease::acquire(&control)?;
    if control.join("service-1.4.sqlite3").try_exists()? {
        ensure!(
            crate::embedding_pool_process::shared::quiesce_for_maintenance(&service)?,
            std::fs::TryLockError::WouldBlock
        );
    }
    Ok(Quiescence {
        catalog,
        _service: service,
        _projects: projects,
        _embedding: embedding,
        _source_watch: source_watch,
    })
}

/// Call under the installer lock before accepting any competing installer
/// transaction. Ordinary Start and the issuer operation itself do not use it.
fn require_issuer_rotation_settled(root: &Path) -> Result<()> {
    if let Some(control) = layout::issuer_rotation_control(root)?
        && let Some(record) = crate::http_endpoint::rotation::read(&control)?
    {
        ensure!(
            matches!(
                record.phase,
                crate::http_endpoint::rotation::Phase::Finalized
                    | crate::http_endpoint::rotation::Phase::Aborted
            ),
            "issuer_rotation_pending_before_runtime_mutation: finish or abort the exact issuer operation first"
        );
    }
    Ok(())
}

/// Public manager registration/policy mutations must not invalidate a native
/// operation's captured supervisor. Stop and exact internal recovery retain
/// their separate ownership checks and do not call this admission gate.
fn require_manager_change_settled(root: &Path) -> Result<()> {
    require_issuer_rotation_settled(root)?;
    legacy_maintenance::require_manager_change_settled(root)
}

pub async fn activate(root: &Path, identity: &str) -> Result<Selection> {
    activate_with_expected(root, identity, None).await
}

pub async fn activate_reviewed(root: &Path, identity: &str, expected: &str) -> Result<Selection> {
    activate_with_expected(root, identity, Some(expected)).await
}

/// An explicit early rebuild choice avoids relying on classifications of failed
/// migration attempts. Native recovery still authenticates saved scope/settings
/// and preserves the complete predecessor state before creating a successor.
pub async fn activate_rebuilding_reviewed(
    root: &Path,
    identity: &str,
    expected: &str,
) -> Result<Selection> {
    let _lock = install_lock(root)?;
    if state_contract::required(root)? {
        return activation::rebuild::activate(root, identity, expected).await;
    }
    if activation::rebuild::pending(root)? || activation::rebuild::repair_pending(root, identity)? {
        return activation::rebuild::activate(root, identity, expected).await;
    }
    require_issuer_rotation_settled(root)?;
    retirement::require_previous_settled(root)?;
    legacy_launchers::require_settled(root)?;
    payload_repair::require_settled(root)?;
    activation::rebuild::activate(root, identity, expected).await
}

/// Canonical native observation, scoped by the actual worker binding. A saved
/// JSON observation supplies no authority to change installation or index state.
pub(crate) fn rebuild_observation_for_control(
    control: &Path,
    binding: airk_core::service::ProjectBinding,
) -> Result<Option<airk_core::rebuild::RebuildObservation>> {
    activation::rebuild::observation_for_control(control, binding)
}

/// Existing native metadata maintenance owns these bounded continuation turns.
pub(crate) fn continue_clean_break(
    root: &Path,
    owner: &airk_store::service::ServiceLease,
    namespace: airk_core::identity::Incarnation,
) -> Result<bool> {
    activation::rebuild::continue_clean_break(root, owner, namespace)?;
    activation::rebuild::continuation_pending(root, owner.root())
}

pub(crate) fn clean_break_continuation_pending(root: &Path, control: &Path) -> Result<bool> {
    activation::rebuild::continuation_pending(root, control)
}

pub(super) fn rebuild_report(
    root: &Path,
) -> (
    Option<airk_core::rebuild::RebuildObservation>,
    Option<String>,
) {
    match activation::rebuild::observation(root) {
        Ok(observation) => (observation, None),
        Err(error) => (None, Some(format!("{error:#}"))),
    }
}

async fn activate_with_expected(
    root: &Path,
    identity: &str,
    expected: Option<&str>,
) -> Result<Selection> {
    let _lock = install_lock(root)?;
    if activation::rebuild::pending(root)? || activation::rebuild::repair_pending(root, identity)? {
        let predecessor = activation::rebuild::activation_predecessor(root, expected)?;
        return activation::rebuild::activate(root, identity, &predecessor).await;
    }
    if state_contract::required(root)? {
        let selected = selection_record(root)?;
        ensure!(
            expected.is_none_or(|expected| expected == selected.active),
            "clean-break selection changed since review"
        );
        return activation::rebuild::activate(root, identity, &selected.active).await;
    }
    activation::rebuild::require_upgrade_allowed(root, identity)?;
    if activation::selection(root)?.is_some() {
        state_contract::candidate(root, identity)?;
    }
    require_issuer_rotation_settled(root)?;
    retirement::require_previous_settled(root)?;
    legacy_launchers::require_settled(root)?;
    if let Some(expected) = expected
        && activation::reviewed_request(root, identity, expected)?
    {
        return selection_record(root);
    }
    payload_repair::require_settled(root)?;
    if activation::resume_catalog_before_lifecycle(root, Some(identity)).await? {
        return selection_record(root);
    }
    lifecycle::require_settled(root)?;
    lifecycle::require_start_allowed(root)?;
    activation::begin(root, identity).await
}

mod commands;
mod http_commands;
pub use http_commands::existing_connection as existing_http_connection;
pub use http_commands::setup as setup_http_connection;
pub(crate) mod http_legacy;
mod managed_service;
pub use commands::run;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod native_connection_tests;
