// SPDX-License-Identifier: Apache-2.0
//! Through 1.9.0 inclusive, installation removes the owned predecessor instead
//! of upgrading in place. Explicit same-build installation also requires full
//! removal at every version. It stops that installation's
//! processes, moves its AIRK-owned folders aside and removes them, and the
//! caller then installs fresh. No index is created until the user asks for one.
//! Source folders are never touched: only AIRK's own locations are removed.
//!
//! Order (one-airk.md §10.3): every location's ownership is proven first, then
//! the owned login registration is removed, then the owned user trust is retired
//! through its receipt, and only then are processes stopped and folders moved.
//! The plan names every client reference the removal breaks (executables, the
//! old endpoint and its CA) so an agent owner can capture and rewrite agent
//! configurations before `--apply`.
use super::*;

/// In-place upgrades require both installed and candidate to exceed this version.
const REPLACEMENT_THROUGH: (u64, u64, u64) = (1, 9, 0);
const MAX_RECORD: u64 = 1024 * 1024;

mod operation;
#[cfg(test)]
mod tests;
use operation::Operation;

pub(super) fn pending_operation(install: &Path, candidate: &str) -> Result<Option<String>> {
    Operation::pending(install, candidate)
}

fn version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.').map(|part| part.parse::<u64>().ok());
    let parsed = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(parsed)
}

fn record(path: &Path) -> Option<serde_json::Value> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_RECORD {
        return None;
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// A record that is provably absent is `None`; a record that exists but cannot
/// be read, is not a bounded regular file or is not JSON is a typed refusal:
/// AIRK never decides to remove an installation it cannot identify.
fn identity_record(path: &Path) -> Result<Option<serde_json::Value>> {
    let refused = |reason: String| {
        anyhow::anyhow!(
            "remove_installation_refused: the installation record {} {reason}; AIRK does not remove an installation it cannot identify. Repair or remove that installation yourself, then install again",
            path.display()
        )
    };
    let metadata = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(refused(format!("cannot be inspected ({error})"))),
        Ok(metadata) => metadata,
    };
    if !metadata.is_file() || metadata.len() > MAX_RECORD {
        return Err(refused("is not a bounded regular file".into()));
    }
    let bytes = fs::read(path).map_err(|error| refused(format!("cannot be read ({error})")))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| refused(format!("is not valid JSON ({error})")))
}

/// Selected runtime identity and its declared version. Only a provably absent
/// record (no selection, or no inventory for the selected runtime) leaves a
/// value unknown; a present but unreadable or malformed record refuses.
fn installed(install: &Path) -> Result<(Option<String>, Option<String>)> {
    let Some(selection) = identity_record(&install.join("selected.json"))? else {
        return Ok((None, None));
    };
    let active = selection["active"]
        .as_str()
        .filter(|identity| digest(identity).is_ok())
        .map(str::to_owned)
        .with_context(|| {
            format!(
                "remove_installation_refused: the installation record {} names no valid selected runtime; AIRK does not remove an installation it cannot identify",
                install.join("selected.json").display()
            )
        })?;
    let inventory = install
        .join("runtimes")
        .join(&active)
        .join("airk-install.json");
    let version = match identity_record(&inventory)? {
        None => None,
        Some(value) => Some(
            value["version"]
                .as_str()
                .map(str::to_owned)
                .with_context(|| {
                    format!(
                        "remove_installation_refused: the installation record {} declares no version; AIRK does not remove an installation it cannot identify",
                        inventory.display()
                    )
                })?,
        ),
    };
    Ok((Some(active), version))
}

/// The decision, kept pure so every case is testable on any host.
fn decide(
    present: bool,
    active: Option<&str>,
    installed_version: Option<&str>,
    candidate_sha256: &str,
    _candidate_staged: bool,
    candidate_version: &str,
) -> (bool, &'static str) {
    if !present {
        return (false, "no_installation");
    }
    if active == Some(candidate_sha256) {
        // Matching an editor's ordinary reconnect is not an installation.
        return (false, "same_runtime");
    }
    let modern = |value: Option<&str>| {
        value
            .and_then(version)
            .is_some_and(|v| v > REPLACEMENT_THROUGH)
    };
    if modern(installed_version) && modern(Some(candidate_version)) {
        return (false, "ordinary_upgrade");
    }
    (true, "replace_before_1_9_0")
}

// Separate explicit installer admission from an editor's ordinary reconnect.
fn decide_install(
    present: bool,
    active: Option<&str>,
    installed_version: Option<&str>,
    candidate_sha256: &str,
    candidate_staged: bool,
    candidate_version: &str,
    reinstall: bool,
) -> (bool, &'static str) {
    if reinstall
        && present
        && (active == Some(candidate_sha256)
            || version(candidate_version).is_none_or(|v| v <= REPLACEMENT_THROUGH)
            || installed_version
                .and_then(version)
                .is_none_or(|v| v <= REPLACEMENT_THROUGH))
    {
        return (true, "replace_before_1_9_0");
    }
    decide(
        present,
        active,
        installed_version,
        candidate_sha256,
        candidate_staged,
        candidate_version,
    )
}

/// AIRK's own identity records for a state, logs or control folder that the
/// installation layout names outside the default locations.
const OWNER_RECORDS: [&str; 3] = [
    "airk-state-1.4.owner.json",
    "logs-1.4.owner.json",
    "control/airk-control-1.4.owner.json",
];

/// Discovery names are not ownership. State, logs, storage and launchers must
/// bind to this exact installation through their canonical native receipts.
fn owned_by_airk(location: &Path, install: &Path) -> Result<bool> {
    let has = |name: &str| fs::symlink_metadata(location.join(name)).is_ok_and(|m| m.is_file());
    let install = airk_host::user_paths::canonical_location(install)?;
    let layout = airk_host::install_layout::Layout::read(&install)?;
    if let Some(layout) = &layout {
        if !layout.claimed()? {
            return Ok(false);
        }
        let control = layout.state.join("control");
        if control.try_exists()? {
            ensure!(
                airk_host::install_layout::installation_for_control(&control)?
                    == Some(install.clone()),
                "control belongs to another installation"
            );
        }
    }
    if install == location {
        return Ok(has("selected.json")
            || has("runtime-layout.json")
            || fs::symlink_metadata(location.join("runtimes")).is_ok_and(|m| m.is_dir()));
    }
    let Some(layout) = layout else {
        return Ok(false);
    };
    if location == layout.state.as_path() {
        return layout.claimed();
    }
    let control = layout.state.join("control");
    if layout.logs.as_deref().unwrap_or(&control) == location {
        return airk_host::service_logs::has_saved_owner(location, &control);
    }
    if has("service-data-1.4.owner.json") {
        let namespace = airk_store::service::registry::retained_namespace(&control)?;
        airk_host::install_layout::validate_catalog_control(&control, namespace)?;
        ensure!(
            airk_host::install_layout::installation_for_control(&control)? == Some(install.clone()),
            "storage control belongs to another installation"
        );
        return airk_store::service::data_scope::has_saved_binding(location, &control, namespace);
    }
    Ok(
        crate::universal_launcher::retirement_image_for(&install, layout.channel)?
            .is_some_and(|image| image.parent() == Some(location)),
    )
}

fn require_user_installation(install: &Path) -> Result<()> {
    managed::require_unmanaged(install).map_err(|error| {
        anyhow::anyhow!(
            "remove_installation_refused: {} is not an AIRK location available for user replacement: {error:#}",
            install.display()
        )
    })
}

fn require_unmanaged_retired(install: &Path, actual: &Path) -> Result<()> {
    if install == actual {
        return require_user_installation(install);
    }
    // A moved receipt still names its original installation. Refuse managed
    // prefixes by presence instead of reinterpreting them against a new root.
    ensure!(
        !actual.join("managed-runtime.json").try_exists()?
            && !actual.join("managed-detach.json").try_exists()?,
        "remove_installation_refused: managed runtime retirement requires its managed lifecycle; saved provider policy and state are preserved"
    );
    // The installation itself may already have been removed on a partial
    // cleanup. Remaining state/control owner records retain the same native
    // managed-origin field and must also prevent user lifecycle retirement.
    for name in [
        airk_host::install_layout::LAYOUT,
        airk_host::install_layout::OWNER,
        airk_host::install_layout::CONTROL_OWNER,
    ] {
        ensure!(
            identity_record(&actual.join(name))?
                .is_none_or(|layout| layout["managed_catalog_origin"].is_null()),
            "remove_installation_refused: managed origin remains in {}; managed state is preserved",
            actual.join(name).display()
        );
    }
    Ok(())
}

fn locations(install: &Path) -> Result<Vec<PathBuf>> {
    locations_at(
        install,
        &airk_host::user_paths::owned_install_locations(install)?,
    )
}

fn locations_at(install: &Path, defaults: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut candidates = vec![install.to_path_buf()];
    if let Some(layout) = record(&install.join("runtime-layout.json")) {
        for field in ["state", "logs"] {
            if let Some(path) = layout[field].as_str().map(PathBuf::from)
                && path.is_absolute()
            {
                candidates.push(path);
            }
        }
    }
    candidates.extend_from_slice(defaults);
    if let Ok((_, channel)) = airk_host::install_layout::retained_parameters(install)
        && let Some(image) = crate::universal_launcher::retirement_image_for(install, channel)?
    {
        candidates.push(
            image
                .parent()
                .context("owned launcher has no folder")?
                .to_path_buf(),
        );
    }
    let mut existing = Vec::new();
    for path in candidates {
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            ensure!(
                !metadata.file_type().is_symlink(),
                "remove_installation_refused: redirected location {}",
                path.display()
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "remove_installation_refused: redirected location {}",
                    path.display()
                );
            }
        }
        let key = airk_host::user_paths::canonical_location(&path)?;
        if fs::symlink_metadata(&key).is_ok_and(|metadata| metadata.is_dir())
            && !existing.contains(&key)
        {
            existing.push(key);
        }
    }
    existing.sort();
    Ok(existing)
}

fn outer_locations(locations: &[PathBuf]) -> Vec<PathBuf> {
    // Validate every candidate first, including a separately claimed state or
    // logs directory inside another removal root. Collapse only the moves.
    locations
        .iter()
        .filter(|path| {
            !locations
                .iter()
                .any(|other| other != *path && path.starts_with(other))
        })
        .cloned()
        .collect()
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct IndexLocations {
    control: Option<PathBuf>,
    namespace: Option<airk_core::identity::Incarnation>,
    sources: Vec<PathBuf>,
    data: Vec<PathBuf>,
}

impl IndexLocations {
    fn identity(install: &Path) -> Result<Self> {
        if !install.join("runtime-layout.json").try_exists()? {
            return Ok(Self::default());
        }
        let control = airk_host::install_layout::saved_control(install)?;
        if !control.join("service-1.4.sqlite3").try_exists()? {
            return Ok(Self::default());
        }
        let namespace = airk_store::service::registry::retained_namespace(&control)?;
        airk_host::install_layout::validate_catalog_control(&control, namespace)?;
        Ok(Self {
            control: Some(control),
            namespace: Some(namespace),
            ..Self::default()
        })
    }
    fn capture(install: &Path) -> Result<Self> {
        if !install.join("runtime-layout.json").try_exists()? {
            return Ok(Self::default());
        }
        let control = airk_host::install_layout::saved_control(install)?;
        if !control.join("service-1.4.sqlite3").try_exists()? {
            return Ok(Self::default());
        }
        Self::capture_control(&control)
    }

    fn capture_control(control: &Path) -> Result<Self> {
        let registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
        let namespace = registry.incarnation()?;
        let registrations = registry
            .bindings()?
            .into_iter()
            .map(|binding| {
                registry
                    .retained_project(binding)
                    .map(|project| project.registration)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut result = Self {
            control: Some(control.to_path_buf()),
            namespace: Some(namespace),
            ..Self::default()
        };
        for registration in &registrations {
            for source in airk_store::service::collection::clean_break_retained_source_roots(
                &registry,
                registration,
                namespace,
                &registrations,
            )? {
                let source = PathBuf::from(source);
                result
                    .sources
                    .push(airk_host::user_paths::canonical_location(&source)?);
                result.sources.push(source);
            }
            let data = PathBuf::from(&registration.data_root);
            if data.try_exists()? {
                ensure!(
                    airk_store::service::data_scope::has_saved_binding(&data, control, namespace)?,
                    "remove_installation_refused: registered storage has no owned binding"
                );
                let data = data.canonicalize()?;
                airk_store::service::data_scope::validate_clean_break_storage(
                    &data,
                    control,
                    namespace,
                    &registrations,
                )?;
            }
            result
                .data
                .push(airk_host::user_paths::canonical_location(&data)?);
        }
        result.sources.sort();
        result.sources.dedup();
        result.data.sort();
        result.data.dedup();
        ensure!(
            serde_json::to_vec(&result)?.len() as u64 <= MAX_RECORD,
            "remove_installation_refused: index inventory exceeds bound"
        );
        Ok(result)
    }

    fn protect(&self, locations: &[PathBuf]) -> Result<()> {
        for target in locations {
            for source in &self.sources {
                ensure!(
                    !target.starts_with(source) && !source.starts_with(target),
                    "remove_installation_refused: original source overlaps removal location {}",
                    target.display()
                );
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct Plan {
    install: PathBuf,
    expected_predecessor: String,
    pub(super) required: bool,
    reason: &'static str,
    installed_runtime: Option<String>,
    installed_version: Option<String>,
    candidate_version: &'static str,
    locations: Vec<PathBuf>,
    client_references: ClientReferences,
    #[serde(skip)]
    index_locations: IndexLocations,
    #[serde(skip)]
    operation: Option<Operation>,
}

fn predecessor_stamp(
    install: &Path,
    runtime: Option<&str>,
    indexes: &IndexLocations,
    directories: &[(PathBuf, String)],
) -> Result<String> {
    Ok(airk_core::identity::framed_digest(
        b"airk/installation-predecessor/1",
        &[&serde_json::to_vec(&(
            airk_host::user_paths::canonical_location(install)?,
            runtime,
            &indexes.control,
            indexes.namespace,
            directories,
        ))?],
    )
    .to_string())
}

/// What an agent configuration can reference that the removal breaks.
#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Debug)]
pub(super) struct ClientReferences {
    /// Every executable under the folders to be removed (including the
    /// universal launcher and `runtimes/*/airk.exe`), plus a universal launcher
    /// elsewhere that forwards to this installation.
    executables: Vec<PathBuf>,
    /// The old loopback MCP endpoint, from its saved descriptor if present.
    http_endpoint: Option<String>,
    /// The old issuer CA digest, from the same descriptor.
    ca_sha256: Option<String>,
}

/// A display path without the Windows verbatim drive prefix, as agent
/// configurations write it.
fn plain(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        if let Some(Component::Prefix(prefix)) = path.components().next()
            && matches!(prefix.kind(), Prefix::VerbatimDisk(_))
            && let Some(text) = path.to_str()
            && let Some(stripped) = text.strip_prefix(r"\\?\")
        {
            return PathBuf::from(stripped);
        }
    }
    path
}

fn client_references(install: &Path, locations: &[PathBuf]) -> Result<ClientReferences> {
    let mut found = Vec::new();
    for location in locations {
        executables(location, &mut found)?;
    }
    let receipt = install.join("launcher.json");
    if receipt.try_exists()? {
        let saved: LauncherReceipt = serde_json::from_slice(&read_bounded(&receipt, 4096)?)?;
        ensure!(saved.schema == 1, "unsupported stable launcher receipt");
        digest(&saved.sha256)?;
        let path = launcher_path(install);
        if path.try_exists()? {
            ensure!(
                runtime_pair::digest_file(&path)? == saved.sha256,
                "changed stable launcher preserved"
            );
        }
        found.push(path);
    }
    if let Ok((_, channel)) = airk_host::install_layout::retained_parameters(install)
        && let Some(launcher) = crate::universal_launcher::retirement_image_for(install, channel)?
    {
        found.push(launcher);
    }
    let mut executables: Vec<PathBuf> = found.into_iter().map(plain).collect();
    executables.sort();
    executables.dedup();
    let endpoint = airk_host::install_layout::saved_control(install)
        .and_then(|control| airk_endpoint::http_endpoint::trust::saved_identity(&control))
        .ok()
        .map(|(_, endpoint)| endpoint);
    Ok(ClientReferences {
        executables,
        http_endpoint: endpoint.as_ref().map(|endpoint| endpoint.url.to_string()),
        ca_sha256: endpoint.map(|endpoint| endpoint.ca_sha256.to_string()),
    })
}

pub fn plan(
    install: &Path,
    candidate_sha256: &str,
    operation_id: Option<&str>,
    generation_id: Option<&str>,
) -> Result<Plan> {
    digest(candidate_sha256)?;
    plan_for_operation(
        install,
        candidate_sha256,
        env!("CARGO_PKG_VERSION"),
        operation_id,
        generation_id,
    )
}

#[cfg(test)]
fn plan_for(
    install: &Path,
    candidate_sha256: &str,
    candidate_version: &'static str,
) -> Result<Plan> {
    plan_for_operation(install, candidate_sha256, candidate_version, None, None)
}

fn plan_for_operation(
    install: &Path,
    candidate_sha256: &str,
    candidate_version: &'static str,
    operation_id: Option<&str>,
    generation_id: Option<&str>,
) -> Result<Plan> {
    plan_for_operation_at(
        install,
        candidate_sha256,
        candidate_version,
        operation_id,
        generation_id,
        None,
    )
}

fn plan_for_operation_at(
    install: &Path,
    candidate_sha256: &str,
    candidate_version: &'static str,
    operation_id: Option<&str>,
    generation_id: Option<&str>,
    defaults: Option<&[PathBuf]>,
) -> Result<Plan> {
    require_user_installation(install)?;
    let operation = Operation::observe(install, candidate_sha256, operation_id, generation_id)?;
    if let Some(saved) = operation
        .as_ref()
        .and_then(|operation| operation.saved.as_ref())
    {
        return saved.plan(
            candidate_version,
            operation.clone().expect("observed operation"),
        );
    }
    let candidates = match defaults {
        Some(candidates) => locations_at(install, candidates)?,
        None => locations(install)?,
    };
    let mut locations = outer_locations(&candidates);
    let (active, installed_version) = if locations.is_empty() {
        (None, None)
    } else {
        installed(install)?
    };
    let staged = install.join("runtimes").join(candidate_sha256).is_dir();
    let (required, reason) = decide_install(
        !locations.is_empty(),
        active.as_deref(),
        installed_version.as_deref(),
        candidate_sha256,
        staged,
        candidate_version,
        operation_id.is_some(),
    );
    let index_locations = if required {
        for location in &candidates {
            ensure!(
                owned_by_airk(location, install).with_context(|| format!(
                    "remove_installation_refused: {} is not an AIRK location owned by this installation",
                    location.display()
                ))?,
                "remove_installation_refused: {} is not an AIRK location",
                location.display()
            );
        }
        if !install.join("runtime-layout.json").try_exists()? {
            ensure!(
                active.is_none(),
                "remove_installation_legacy_state_unproven: selected runtime has no authenticated installation layout; original sources and storage are preserved"
            );
            for location in &locations {
                ensure!(
                    !location.join("control/service-1.4.sqlite3").try_exists()?
                        && !location.join("service-1.4.sqlite3").try_exists()?,
                    "remove_installation_legacy_state_unproven: {} has saved registry state without an authenticated installation layout; original sources and storage are preserved",
                    location.display()
                );
            }
        }
        let indexes = IndexLocations::capture(install)?;
        for data in &indexes.data {
            if data.try_exists()? {
                locations.push(data.clone());
            }
        }
        locations.sort();
        locations.dedup();
        locations = outer_locations(&locations);
        indexes.protect(&locations)?;
        indexes
    } else if operation_id.is_some() {
        IndexLocations::identity(install)?
    } else {
        // An ordinary reconnect admits no cleanup and must not authenticate
        // unused state/custom storage. Explicit installation stays strict.
        IndexLocations::default()
    };
    let client_references = if required {
        client_references(install, &locations)?
    } else {
        ClientReferences::default()
    };
    let directories = locations
        .iter()
        .map(|location| Ok((location.clone(), operation::directory_identity(location)?)))
        .collect::<Result<Vec<_>>>()?;
    let expected_predecessor =
        predecessor_stamp(install, active.as_deref(), &index_locations, &directories)?;
    Ok(Plan {
        install: install.to_path_buf(),
        expected_predecessor,
        required,
        reason,
        installed_runtime: active,
        installed_version,
        candidate_version,
        locations,
        client_references,
        index_locations,
        operation,
    })
}

fn executables(root: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
    let mut visited = 0;
    collect_executables(root, found, &mut visited)
}

fn collect_executables(root: &Path, found: &mut Vec<PathBuf>, visited: &mut usize) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| {
        format!(
            "remove_installation_inventory_unavailable: {}",
            root.display()
        )
    })? {
        let entry = entry?;
        *visited += 1;
        ensure!(
            *visited <= 131072 && found.len() <= 1024,
            "remove_installation_executable_inventory_budget"
        );
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                continue;
            }
        }
        if metadata.is_dir() {
            collect_executables(&path, found, visited)?;
        } else if metadata.is_file() && executable(&path, &metadata) {
            found.push(path);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn executable(path: &Path, _: &fs::Metadata) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
}
#[cfg(unix)]
fn executable(_: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

/// One replacement of an installation at a time. The lock lives beside the
/// installation, never inside a folder that will move: Windows cannot rename a
/// folder that holds an open file, so the in-installation install lock cannot
/// span the move. The loser is refused before any side effect.
fn replacement_lock(install: &Path) -> Result<Option<fs::File>> {
    let (Some(parent), Some(leaf)) = (install.parent(), install.file_name()) else {
        bail!("remove_installation_refused: the installation location has no parent folder");
    };
    if !parent.is_dir() {
        // Nothing can be installed below a missing folder; the plan is a no-op.
        return Ok(None);
    }
    let mut name = std::ffi::OsString::from(".");
    name.push(leaf);
    name.push(".replace-1.lock");
    let path = parent.join(name);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(
            metadata.is_file() && metadata.len() == 0,
            "remove_installation_refused: {} is not AIRK's replacement lock",
            path.display()
        );
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => bail!(
            "replacement_in_progress: another AIRK installer is replacing {}; retry after it finishes",
            install.display()
        ),
        Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
    }
}

/// Stop, move aside and remove every location the plan names, then report.
pub(super) fn apply(
    install: &Path,
    candidate_sha256: &str,
    operation_id: Option<&str>,
    generation_id: Option<&str>,
) -> Result<serde_json::Value> {
    apply_expected(install, candidate_sha256, operation_id, generation_id, None)
}

pub fn apply_expected(
    install: &Path,
    candidate_sha256: &str,
    operation_id: Option<&str>,
    generation_id: Option<&str>,
    expected_predecessor: Option<&str>,
) -> Result<serde_json::Value> {
    digest(candidate_sha256)?;
    if let Some(expected) = expected_predecessor {
        digest(expected)?;
    }
    require_user_installation(install)?;
    if operation_id.is_some() {
        let install = airk_host::user_paths::canonical_location(install)?;
        fs::create_dir_all(install.parent().context("installation parent missing")?)?;
    }
    // Recompute the plan only under the lock, so two windows cannot both act
    // on one observation.
    let _replacing = replacement_lock(install)?;
    let mut plan = plan(install, candidate_sha256, operation_id, generation_id)?;
    ensure!(
        !plan.required || operation_id.is_some(),
        "replacement_operation_required: destructive replacement requires --reinstall --operation-id so interrupted cleanup remains recoverable"
    );
    if plan.reason == "installation_already_replaced" {
        return Ok(serde_json::json!({"replaced":false,"plan":plan}));
    }
    ensure!(
        expected_predecessor.is_none_or(|expected| expected == plan.expected_predecessor),
        "replacement_predecessor_changed: installation identity changed after review; no predecessor cleanup was admitted"
    );
    if plan.required {
        require_outside(&std::env::current_exe()?, &plan.locations)?;
        if plan
            .operation
            .as_ref()
            .is_some_and(|operation| operation.saved.is_some())
        {
            return operation::resume(plan);
        }
        require_owned(&plan)?;
    }
    Operation::begin(&mut plan)?;
    let operation = plan.operation.clone();
    let result = apply_ordered(
        plan,
        || Ok(serde_json::to_value(remove_owned_startup(install)?)?),
        || retire_owned_trust(install),
        Duration::from_secs(1),
    )?;
    if let Some(operation) = operation {
        operation.complete()?;
    }
    Ok(result)
}

/// The installer must not stop or move the image it runs from.
fn require_outside(executable: &Path, locations: &[PathBuf]) -> Result<()> {
    // Native locations are directories; resolve the image's folder.
    let executable = airk_host::user_paths::canonical_location(
        executable
            .parent()
            .context("running AIRK image has no folder")?,
    )?
    .join(
        executable
            .file_name()
            .context("running AIRK image has no name")?,
    );
    for location in locations {
        ensure!(
            !executable.starts_with(location),
            "remove_installation_refused: this AIRK process runs from {}, inside a location it would remove; run the new installer from its own folder",
            executable.display()
        );
    }
    Ok(())
}

/// Every location is proven AIRK's own before anything changes.
fn require_owned(plan: &Plan) -> Result<()> {
    require_user_installation(&plan.install)?;
    let mut observed = locations(&plan.install)?;
    observed.extend_from_slice(&plan.locations);
    observed.sort();
    observed.dedup();
    for location in &observed {
        ensure!(
            plan.locations.iter().any(|root| location.starts_with(root)),
            "remove_installation_refused: location roster changed after review: {}",
            location.display()
        );
        ensure!(
            owned_by_airk(location, &plan.install).with_context(|| format!(
                "remove_installation_refused: {} has no canonical ownership proof for this installation",
                location.display()
            ))?,
            "remove_installation_refused: {} is not an AIRK location: it holds none of AIRK's ownership records ({}). Nothing was changed. If it is AIRK's, remove it yourself or correct runtime-layout.json, then install again",
            location.display(),
            OWNER_RECORDS.join(", ")
        );
    }
    Ok(())
}

/// The §10.3 order with its effects injected: ownership of every location,
/// then the owned login registration, then the owned user trust, then the
/// stop/move/remove of the folders. A trust failure refuses before folders move.
fn apply_ordered(
    plan: Plan,
    remove_startup: impl FnOnce() -> Result<serde_json::Value>,
    retire_trust: impl FnOnce() -> (serde_json::Value, Vec<String>),
    retry_pause: Duration,
) -> Result<serde_json::Value> {
    if !plan.required {
        let mut result = apply_plan(plan, retry_pause)?;
        result["startup_registration_removed"] = serde_json::Value::Null;
        return Ok(result);
    }
    require_owned(&plan)?;
    let startup = remove_startup()?;
    let (trust, trust_residue) = retire_trust();
    ensure!(
        trust_residue.is_empty(),
        "remove_installation_trust_incomplete: {}; no folders were moved",
        trust_residue.join("; ")
    );
    let mut result = apply_plan(plan, retry_pause)?;
    result["startup_registration_removed"] = startup;
    result["user_trust_removed"] = trust;
    if let Some(residue) = result["residue"].as_array_mut() {
        residue.extend(trust_residue.into_iter().map(serde_json::Value::String));
    }
    Ok(result)
}

/// Retire the installation's own CurrentUser/Root (or login keychain) issuer
/// through its ownership receipt under `control/http-1`, before that folder
/// moves. An issuer AIRK did not install is never removed; any failure is
/// returned as residue with the CA digest, never dropped.
fn retire_owned_trust(install: &Path) -> (serde_json::Value, Vec<String>) {
    let control = match airk_host::install_layout::saved_control(install) {
        Ok(control) => control,
        Err(error) => {
            // Without a readable layout, a surviving receipt is still reported.
            let receipt = ["control", "state/control"]
                .iter()
                .map(|leaf| install.join(leaf).join("http-1").join("user-trust-1.json"))
                .find(|path| path.is_file());
            return match receipt {
                Some(path) => (
                    serde_json::Value::Null,
                    vec![format!(
                        "user trust: receipt {} could not be authenticated ({error:#}); remove the AIRK issuer from the user trust store manually",
                        path.display()
                    )],
                ),
                None => (serde_json::Value::Null, Vec::new()),
            };
        }
    };
    match airk_endpoint::http_endpoint::trust::remove_owned(&control) {
        Ok(status) => (
            serde_json::to_value(status).unwrap_or(serde_json::Value::Null),
            Vec::new(),
        ),
        Err(error) => {
            let ca = airk_endpoint::http_endpoint::trust::saved_identity(&control)
                .map(|(_, endpoint)| endpoint.ca_sha256.to_string())
                .unwrap_or_else(|_| "unknown".into());
            (
                serde_json::Value::Null,
                vec![format!(
                    "user trust: the owned AIRK issuer (CA SHA-256 {ca}) was not removed: {error:#}; remove it from the user trust store manually"
                )],
            )
        }
    }
}

/// An owned login registration starts the installation being removed. Remove it
/// first: otherwise the next login runs a removed image and the fresh install
/// cannot register its own. Ownership is the install-side receipt the
/// registration wrote; an unowned OS entry is never altered. Failure stops the
/// replacement before anything moves.
fn remove_owned_startup(install: &Path) -> Result<Option<crate::service_manager::ServiceStatus>> {
    if !fs::symlink_metadata(install).is_ok_and(|metadata| metadata.is_dir()) {
        return Ok(None);
    }
    let manager = crate::service_manager::ServiceManager::for_runtime(install)?;
    if !manager.owned_registration_exists()? {
        return Ok(None);
    }
    manager.uninstall().map(Some).context(
        "remove_installation_blocked: the previous AIRK login startup registration could not be removed",
    )
}

fn apply_plan(plan: Plan, retry_pause: Duration) -> Result<serde_json::Value> {
    apply_plan_with(plan, retry_pause, &mut rename_owned)
}

fn rename_owned(from: &Path, to: &Path) -> std::io::Result<()> {
    airk_host::install_layout::publish_new(from, to).map_err(std::io::Error::other)
}

/// Move every location aside, or none: when a location cannot be moved after
/// the retries, the ones already moved are renamed back, newest first. A
/// rollback that itself fails names every folder still aside.
fn apply_plan_with(
    plan: Plan,
    retry_pause: Duration,
    rename: &mut dyn FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<serde_json::Value> {
    apply_plan_with_removal(plan, retry_pause, rename, &mut |path| {
        fs::remove_dir_all(path)
    })
}

fn apply_plan_with_removal(
    plan: Plan,
    retry_pause: Duration,
    rename: &mut dyn FnMut(&Path, &Path) -> std::io::Result<()>,
    remove: &mut dyn FnMut(&Path) -> std::io::Result<()>,
) -> Result<serde_json::Value> {
    let mut images = Vec::new();
    for path in &plan.client_references.executables {
        if path.try_exists()? {
            safe_file(path)?;
            images.push(legacy_launchers::Image {
                path: path.canonicalize()?,
                sha256: runtime_pair::digest_file(path)?,
            });
        } else {
            let audit = crate::legacy_processes::audit(path);
            ensure!(
                audit.complete
                    && audit.failure.is_none()
                    && audit.matching_executable_pids.is_empty(),
                "remove_installation_process_observation_unavailable: {}",
                path.display()
            );
        }
    }
    let paths = images
        .iter()
        .map(|image| image.path.clone())
        .collect::<Vec<_>>();
    let exclusion = std::cell::RefCell::new(None);
    let result = apply_plan_pipeline(
        plan,
        retry_pause,
        rename,
        &mut |_| {
            let proof = (|| -> Result<_> {
                upgrade_stop::stop_paths(&paths)?;
                let held = legacy_launchers::image_exclusion(&images)?;
                legacy_launchers::observe(&images, &held.images)?;
                Ok(held)
            })();
            match proof {
                Ok(held) => {
                    *exclusion.borrow_mut() = Some(held);
                    None
                }
                Err(error) => Some(format!("{error:#}")),
            }
        },
        &mut |plan| {
            let held = exclusion.borrow();
            let held = held
                .as_ref()
                .context("replacement image exclusion missing")?;
            Operation::admit_images(plan, &images, &held.images)?;
            unlink_image_names(&images, &held.images).context("replacement_image_retirement_incomplete: admitted executable names may already be retired; resume the same installation operation")?;
            Ok(images.len())
        },
        remove,
    );
    drop(exclusion);
    result
}

fn unlink_image_names(
    images: &[legacy_launchers::Image],
    exclusions: &[legacy_launchers::Image],
) -> Result<()> {
    unlink_image_names_with(images, exclusions, &mut |_| Ok(()))
}

fn unlink_image_names_with(
    images: &[legacy_launchers::Image],
    exclusions: &[legacy_launchers::Image],
    after_unlink: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<()> {
    legacy_launchers::observe(images, exclusions)?;
    for image in images {
        if image.path.try_exists()? {
            safe_file(&image.path)?;
            ensure!(
                runtime_pair::digest_file(&image.path)? == image.sha256,
                "replacement_image_changed: {}",
                image.path.display()
            );
            fs::remove_file(&image.path)?;
            sync_directory(
                image
                    .path
                    .parent()
                    .context("retired image parent missing")?,
            )?;
            after_unlink(&image.path)?;
        }
    }
    legacy_launchers::observe(images, exclusions)?;
    Ok(())
}

#[cfg(test)]
fn apply_plan_with_stop(
    plan: Plan,
    retry_pause: Duration,
    rename: &mut dyn FnMut(&Path, &Path) -> std::io::Result<()>,
    stop: &mut dyn FnMut(&[PathBuf]) -> Option<String>,
) -> Result<serde_json::Value> {
    apply_plan_effects(plan, retry_pause, rename, stop, &mut |path| {
        fs::remove_dir_all(path)
    })
}

#[cfg(test)]
fn apply_plan_effects(
    plan: Plan,
    retry_pause: Duration,
    rename: &mut dyn FnMut(&Path, &Path) -> std::io::Result<()>,
    stop: &mut dyn FnMut(&[PathBuf]) -> Option<String>,
    remove: &mut dyn FnMut(&Path) -> std::io::Result<()>,
) -> Result<serde_json::Value> {
    apply_plan_pipeline(plan, retry_pause, rename, stop, &mut |_| Ok(0), remove)
}

fn apply_plan_pipeline(
    mut plan: Plan,
    retry_pause: Duration,
    rename: &mut dyn FnMut(&Path, &Path) -> std::io::Result<()>,
    stop: &mut dyn FnMut(&[PathBuf]) -> Option<String>,
    before_move: &mut dyn FnMut(&mut Plan) -> Result<usize>,
    remove: &mut dyn FnMut(&Path) -> std::io::Result<()>,
) -> Result<serde_json::Value> {
    if !plan.required {
        return Ok(serde_json::json!({"replaced": false, "plan": plan}));
    }
    require_owned(&plan)?;
    plan.index_locations.protect(&plan.locations)?;
    let stamp = plan
        .operation
        .as_ref()
        .map(|operation| operation.id.clone())
        .unwrap_or(unix_ms()?.to_string());
    let mut moved = Vec::new();
    let mut pending: Vec<PathBuf> = plan.locations.clone();
    let mut last_error = String::new();
    if let Some(failure) = stop(&pending) {
        bail!(
            "remove_installation_blocked: predecessor shutdown was not proven: {failure}; no folders were moved"
        );
    }
    require_owned(&plan)?;
    ensure!(
        IndexLocations::capture(&plan.install)? == plan.index_locations,
        "remove_installation_refused: index ownership or source roster changed after review"
    );
    Operation::freeze(&mut plan)?;
    let retired_images = before_move(&mut plan)?;
    for attempt in 0..5 {
        let mut remaining = Vec::new();
        for location in pending {
            let name = location
                .file_name()
                .and_then(|value| value.to_str())
                .context("AIRK location has no name")?;
            let aside = location.with_file_name(format!("{name}.removed-{stamp}"));
            match rename(&location, &aside) {
                Ok(()) => moved.push((location, aside)),
                Err(error) => {
                    last_error = format!("{}: {error}", location.display());
                    remaining.push(location);
                }
            }
        }
        pending = remaining;
        if pending.is_empty() {
            break;
        }
        if attempt < 4 {
            std::thread::sleep(retry_pause);
        }
    }
    if !pending.is_empty() {
        let mut aside_left = Vec::new();
        for (location, aside) in moved.into_iter().rev() {
            if let Err(error) = rename(&aside, &location) {
                aside_left.push(format!(
                    "{} (could not be restored to {}: {error})",
                    aside.display(),
                    location.display()
                ));
            }
        }
        let rollback = if aside_left.is_empty() {
            " Every folder already moved was restored; a login registration or user trust removed before the move is not restored, so resume the same installation operation to finish."
                .to_owned()
        } else {
            format!(
                " Restoring the folders already moved failed; these AIRK folders remain aside and need review (residue): {}.",
                aside_left.join("; ")
            )
        };
        let image_retirement = if retired_images == 0 {
            String::new()
        } else {
            " Owned executable names were already retired under process exclusion and are not restored by folder rollback. The restored folders are not a runnable installation; resume the same installation operation to finish.".into()
        };
        bail!(
            "remove_installation_blocked: {last_error}. The owned folders could not all be moved; resume the same installation operation after resolving this filesystem refusal.{rollback}{image_retirement}"
        );
    }
    let mut removed = Vec::new();
    let mut residue = Vec::new();
    for (_, aside) in moved {
        match remove(&aside) {
            Ok(()) => removed.push(aside),
            Err(error) => residue.push(format!("{}: {error}", aside.display())),
        }
    }
    ensure!(
        residue.is_empty(),
        "remove_installation_incomplete: owned folders remain: {}",
        residue.join("; ")
    );
    let client_references = serde_json::to_value(&plan.client_references)?;
    Ok(serde_json::json!({
        "replaced": true,
        "plan": plan,
        "removed": removed,
        "residue": residue,
        "client_references": client_references
    }))
}
