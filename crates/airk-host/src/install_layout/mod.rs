// SPDX-License-Identifier: Apache-2.0
//! Durable install/state binding. Interrupted claims never fall back to another ledger.
//! Leaf: the layout receipts, control binding and registry pin, read and recorded
//! without the installer. Initialization and the successor writer stay in
//! `runtime_install::layout`. See `CONTRACT.md` beside this file.
use crate::install_files::{read_bounded, safe_file, sync_directory};
use crate::local_ipc;
use crate::user_paths::Channel as RuntimeChannel;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

pub const LAYOUT: &str = "runtime-layout.json";
pub const OWNER: &str = "airk-state-1.4.owner.json";
pub const CLAIM: &str = "airk-state-1.4.lock";
pub const OWNER_PART: &str = "airk-state-1.4.owner.part";
pub const MAX_LAYOUT: u64 = 32 * 1024;
pub const CONTROL_OWNER: &str = "airk-control-1.4.owner.json";
pub const CONTROL_PART: &str = "airk-control-1.4.owner.part";
pub mod registry_binding;
pub mod successor_receipt;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub schema: u32,
    pub install: PathBuf,
    pub state: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs: Option<PathBuf>,
    pub channel: RuntimeChannel,
    pub ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_catalog_origin:
        Option<airk_store::service::registry::catalog::genesis::ManagedGenesisOrigin>,
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

mod exclusive;
pub use exclusive::{publish_new, publish_new_at};
impl Layout {
    pub fn read(root: &Path) -> Result<Option<Self>> {
        if !exists(&root.join(LAYOUT))? {
            return Ok(None);
        }
        let layout: Self = serde_json::from_slice(&read_bounded(&root.join(LAYOUT), MAX_LAYOUT)?)?;
        if let Some(origin) = &layout.managed_catalog_origin {
            origin.validate()?;
            ensure!(
                origin.install == layout.install && origin.state == layout.state,
                "managed layout reservation changed"
            );
        }
        ensure!(
            layout.schema == 1 && layout.install == root.canonicalize()?,
            "runtime layout installation identity mismatch"
        );
        ensure!(
            crate::user_paths::canonical_location(&layout.state)? == layout.state
                && !layout.state.starts_with(layout.install.join("runtimes")),
            "runtime layout state is redirected or inside an immutable payload"
        );
        if let Some(logs) = &layout.logs {
            let text = logs.to_str().context("non-UTF-8 log location")?;
            ensure!(
                logs.is_absolute()
                    && text.len() <= 4096
                    && !text.chars().any(char::is_control)
                    && !logs
                        .components()
                        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
                    && !layout.install.starts_with(logs)
                    && !layout.state.starts_with(logs)
                    && !logs.starts_with(layout.install.join("runtimes"))
                    && !logs.starts_with(layout.state.join("control")),
                "runtime log location is redirected or overlaps protected execution/control state"
            );
        }
        successor_receipt::validate_binding(root, &layout)?;
        Ok(Some(layout))
    }
    pub fn owner(&self) -> Self {
        let mut owner = self.clone();
        owner.ready = true;
        owner
    }
    pub fn claimed(&self) -> Result<bool> {
        if !exists(&self.state.join(OWNER))? {
            return Ok(false);
        }
        let owner: Self =
            serde_json::from_slice(&read_bounded(&self.state.join(OWNER), MAX_LAYOUT)?)?;
        ensure!(
            owner == self.owner(),
            "service state belongs to another installation or channel"
        );
        Ok(true)
    }
    fn owner_prefix(&self) -> Result<bool> {
        let path = self.state.join(OWNER_PART);
        if !exists(&path)? {
            return Ok(false);
        }
        let bytes = read_bounded(&path, MAX_LAYOUT)?;
        ensure!(
            serde_json::to_vec(&self.owner())?.starts_with(&bytes),
            "unknown or modified state ownership partial preserved"
        );
        Ok(true)
    }
    pub fn claim(&self) -> Result<()> {
        if self.state != self.install && exists(&self.state)? && !self.claimed()? {
            for (index, entry) in fs::read_dir(&self.state)?.enumerate() {
                let entry = entry?;
                ensure!(
                    !self.ready && index < 2,
                    "unknown nonempty service-state directory preserved"
                );
                if entry.path() == self.state.join(CLAIM) {
                    safe_file(&entry.path())?;
                    ensure!(
                        entry.metadata()?.len() == 0,
                        "unknown state claim contents preserved"
                    );
                } else {
                    ensure!(
                        entry.path() == self.state.join(OWNER_PART) && self.owner_prefix()?,
                        "unknown service-state file preserved"
                    );
                }
            }
        }
        local_ipc::prepare_root(&self.state)?;
        let path = self.state.join(CLAIM);
        if exists(&path)? {
            safe_file(&path)?;
            ensure!(
                fs::metadata(&path)?.len() == 0,
                "foreign state claim lock preserved"
            );
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(&path)?;
        safe_file(&path)?;
        lock.try_lock()
            .context("another installer is claiming this service-state directory")?;
        if !self.claimed()? {
            let bytes = serde_json::to_vec(&self.owner())?;
            ensure!(
                bytes.len() as u64 <= MAX_LAYOUT,
                "state ownership metadata exceeds bound"
            );
            let partial = self.state.join(OWNER_PART);
            let mut options = OpenOptions::new();
            options.write(true);
            if self.owner_prefix()? {
                options.truncate(true);
            } else {
                options.create_new(true);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut temporary = options.open(&partial)?;
            temporary.write_all(&bytes)?;
            temporary.sync_all()?;
            drop(temporary);
            publish_new(&partial, &self.state.join(OWNER))?;
            sync_directory(&self.state)?;
        } else if self.owner_prefix()? {
            fs::remove_file(self.state.join(OWNER_PART))?;
            sync_directory(&self.state)?;
        }
        Ok(())
    }
}

pub fn preflight_layout(layout: &Layout) -> Result<()> {
    ensure!(
        serde_json::to_vec(layout)?.len() as u64 <= MAX_LAYOUT
            && serde_json::to_vec(&layout.owner())?.len() as u64 <= MAX_LAYOUT,
        "runtime layout exceeds metadata bound"
    );
    registry_binding::preflight(&layout.owner())
}

pub fn managed_origin(
    root: &Path,
) -> Result<Option<airk_store::service::registry::catalog::genesis::ManagedGenesisOrigin>> {
    Ok(Layout::read(root)?.and_then(|layout| layout.managed_catalog_origin))
}

pub fn require_managed_ready(root: &Path) -> Result<()> {
    if let Some(origin) = managed_origin(root)? {
        airk_store::service::registry::catalog::genesis::require_managed_settled(
            &origin.control,
            &origin,
        )?;
    }
    Ok(())
}

fn require_managed_control_ready(control: &Path) -> Result<()> {
    if let Some(layout) = control_binding(&control_location(control)?)?
        && let Some(origin) = layout.managed_catalog_origin
    {
        airk_store::service::registry::catalog::genesis::require_managed_settled(control, &origin)?;
    }
    Ok(())
}

// Resolve the parent before inspecting the child, so a redirected control
// directory cannot escape its state-level receipt and masquerade as raw mode.
fn control_location(control: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(control)?;
    let parent = crate::user_paths::canonical_location(
        absolute.parent().context("control parent missing")?,
    )?;
    Ok(parent.join(
        absolute
            .file_name()
            .context("control directory name missing")?,
    ))
}
pub fn require_control(control: &Path) -> Result<()> {
    ensure!(
        exists(control)?,
        "initialized service control directory is missing; restore the owned state instead of creating another registry"
    );
    let metadata = fs::symlink_metadata(control)?;
    ensure!(
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && control.canonicalize()? == control,
        "initialized service control directory is redirected or unavailable"
    );
    Ok(())
}
pub fn control_binding(control: &Path) -> Result<Option<Layout>> {
    control_binding_for_catalog(control, None)
}

fn control_binding_for_catalog(
    control: &Path,
    namespace: Option<airk_core::identity::Incarnation>,
) -> Result<Option<Layout>> {
    let state = control.parent().context("control parent missing")?;
    let marker = state.join(CONTROL_OWNER);
    let partial = state.join(CONTROL_PART);
    let completed = exists(&marker)?;
    let owner = if completed {
        serde_json::from_slice::<Layout>(&read_bounded(&marker, MAX_LAYOUT)?)?
    } else if exists(&state.join(OWNER))? {
        serde_json::from_slice::<Layout>(&read_bounded(&state.join(OWNER), MAX_LAYOUT)?)?
    } else {
        ensure!(
            !exists(&partial)?
                && !exists(&state.join(CLAIM))?
                && !exists(&state.join(OWNER_PART))?
                && !exists(&state.join(registry_binding::PIN))?
                && !exists(&state.join(registry_binding::PART))?,
            "service state ownership is incomplete; control initialization cannot fall back"
        );
        return Ok(None); // Explicit raw development namespace.
    };
    ensure!(
        owner.ready && owner.state == state && control == state.join("control"),
        "control initialization belongs to another state directory"
    );
    let layout = Layout::read(&owner.install)?.context("control installation binding missing")?;
    ensure!(
        layout == owner,
        "control initialization install/state ownership differs"
    );
    if exists(&partial)? {
        let bytes = read_bounded(&partial, MAX_LAYOUT)?;
        ensure!(
            serde_json::to_vec(&owner)?.starts_with(&bytes),
            "unknown control initialization partial preserved"
        );
        require_control(control)?;
    }
    if completed {
        require_control(control)?;
        if exists(&state.join(OWNER))? {
            let current: Layout =
                serde_json::from_slice(&read_bounded(&state.join(OWNER), MAX_LAYOUT)?)?;
            ensure!(
                current == owner,
                "control initialization conflicts with current state ownership"
            );
        }
    }
    if let Some(namespace) = namespace {
        registry_binding::validate_catalog(&owner, completed, namespace)?;
    } else {
        registry_binding::validate(&owner, completed)?;
    }
    Ok(Some(owner))
}
/// Must run before directory creation, including explicit foreground startup.
pub fn validate_control(control: &Path) -> Result<()> {
    control_binding(&control_location(control)?)?;
    Ok(())
}

/// Resolve only an already authenticated installed control binding. Raw native
/// development namespaces have no installation; this never creates state.
pub fn installation_for_control(control: &Path) -> Result<Option<PathBuf>> {
    Ok(control_binding(&control_location(control)?)?.map(|owner| owner.install))
}

/// Installer-only validation with a namespace already checked against the saved
/// catalog intent/owner. No pin repair, directory creation or SQLite open occurs.
pub fn validate_catalog_control(
    control: &Path,
    namespace: airk_core::identity::Incarnation,
) -> Result<()> {
    control_binding_for_catalog(&control_location(control)?, Some(namespace))?;
    Ok(())
}

/// Read-only routing for installer transaction exclusion, including incomplete
/// initial staging. A known issuer journal never falls back to another control.
pub fn issuer_rotation_control(root: &Path) -> Result<Option<PathBuf>> {
    let Some(layout) = Layout::read(root)? else {
        return Ok(None);
    };
    let control = layout.state.join("control");
    if !exists(&control.join("http-1/issuer-rotation.json"))? {
        return Ok(None);
    }
    ensure!(
        layout.ready && layout.claimed()?,
        "issuer journal exists in an unclaimed runtime layout"
    );
    require_control(&control)?;
    Ok(Some(control))
}

/// Log lookup may use verified layout ownership even when registry authority is unavailable.
pub fn saved_control(root: &Path) -> Result<PathBuf> {
    let layout = Layout::read(root)?.context("runtime state layout missing")?;
    ensure!(
        layout.ready && layout.claimed()?,
        "runtime state layout is not claimed"
    );
    Ok(layout.state.join("control"))
}

/// Owning installer initialization only. Preserve the ordinary no-redirect and
/// existing-directory guards without requiring a completed registry pin yet.
pub fn initialization_control(root: &Path) -> Result<PathBuf> {
    let control = saved_control(root)?;
    require_control(&control)?;
    Ok(control)
}
pub fn retained_parameters(root: &Path) -> Result<(PathBuf, RuntimeChannel)> {
    let layout = Layout::read(root)?.context("runtime state layout missing")?;
    ensure!(
        layout.ready && layout.claimed()?,
        "runtime state layout is not claimed"
    );
    Ok((layout.state, layout.channel))
}
/// Actual receipt-bound paths which may never contain registered index data.
pub fn retained_private_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let layout = Layout::read(root)?.context("runtime state layout missing")?;
    ensure!(
        layout.ready && layout.claimed()?,
        "runtime state layout is not claimed"
    );
    let mut paths = vec![layout.install, layout.state];
    paths.extend(layout.logs);
    if let Some(origin) = layout.managed_catalog_origin {
        paths.push(origin.source);
    }
    paths.extend(successor_receipt::preserved_private_paths(root)?);
    Ok(paths)
}
/// Record initialization before any registry creation, under singleton ownership.
pub fn record_control(lease: &airk_store::service::ServiceLease) -> Result<()> {
    let control = control_location(lease.root())?;
    ensure!(
        control == lease.root(),
        "control initialization lease differs"
    );
    let Some(owner) = control_binding(&control)? else {
        return Ok(());
    };
    require_control(&control)?;
    let marker = owner.state.join(CONTROL_OWNER);
    registry_binding::begin(lease, &owner, exists(&marker)?)?;
    if exists(&marker)? {
        return Ok(());
    }
    let bytes = serde_json::to_vec(&owner)?;
    ensure!(
        bytes.len() as u64 <= MAX_LAYOUT,
        "control initialization receipt exceeds bound"
    );
    let partial = owner.state.join(CONTROL_PART);
    let mut options = OpenOptions::new();
    options.write(true);
    if exists(&partial)? {
        options.truncate(true);
    } else {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&partial)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    publish_new(&partial, &marker)?;
    sync_directory(&owner.state)
}

pub fn record_registry(
    lease: &airk_store::service::ServiceLease,
    namespace: airk_core::identity::Incarnation,
) -> Result<()> {
    let Some(owner) = control_binding(&control_location(lease.root())?)? else {
        return Ok(());
    };
    registry_binding::finish(lease, &owner, namespace)
}

pub fn log_parent(control: &Path) -> Result<PathBuf> {
    let control = control_location(control)?;
    if exists(&control)? {
        require_control(&control)?;
    }
    let state = control.parent().context("control parent missing")?;
    let owner_path = state.join(OWNER);
    if !exists(&owner_path)? {
        ensure!(
            !exists(&state.join(CLAIM))? && !exists(&state.join(OWNER_PART))?,
            "service state ownership is incomplete; log location cannot fall back"
        );
        return Ok(control); // Explicit raw development namespace, no installer binding.
    }
    let owner: Layout = serde_json::from_slice(&read_bounded(&owner_path, MAX_LAYOUT)?)?;
    ensure!(
        owner.ready && owner.state == state && control == state.join("control"),
        "log location does not belong to this control namespace"
    );
    let layout = Layout::read(&owner.install)?.context("log installation binding missing")?;
    ensure!(
        layout == owner && layout.claimed()?,
        "log install/state ownership differs"
    );
    // Earlier development layouts keep their existing ring until explicitly replaced.
    let logs = layout.logs.unwrap_or(control);
    ensure!(
        crate::user_paths::canonical_location(&logs)? == logs,
        "native log location is unavailable or redirected"
    );
    Ok(logs)
}

pub fn inspect(root: &Path) -> Result<serde_json::Value> {
    Ok(match Layout::read(root)? {
        None => {
            serde_json::json!({"layout":"unconfigured","control":null})
        }
        Some(layout) => {
            let claimed = layout.claimed()?;
            let control = layout.state.join("control");
            let initialized = exists(&layout.state.join(CONTROL_OWNER))?;
            let interrupted = exists(&layout.state.join(CONTROL_PART))?;
            let error = validate_control(&control)
                .err()
                .map(|error| format!("{error:#}").chars().take(1024).collect::<String>());
            serde_json::json!({"layout":layout,"state_claimed":claimed,"control":control,
                "control_initialized":initialized,"control_initialization_interrupted":interrupted,"control_error":error})
        }
    })
}

pub fn abort(root: &Path) -> Result<()> {
    if let Some(layout) = Layout::read(root)? {
        ensure!(
            !layout.ready,
            "ready service state cannot be abandoned with abort-layout"
        );
        // Preserve even an earlier owned claim: no directory, state database,
        // lock, metadata receipt or foreign occupant is removed by this action.
        fs::remove_file(root.join(LAYOUT))?;
        sync_directory(root)?;
    }
    Ok(())
}

pub fn validate_control_root(control: &Path) -> Result<()> {
    validate_control(control)?;
    require_managed_control_ready(control)?;
    airk_store::service::registry::data_relocation::require_settled(control)
}
pub fn record_control_root(lease: &airk_store::service::ServiceLease) -> Result<()> {
    record_control(lease)
}
pub fn record_registry_root(
    lease: &airk_store::service::ServiceLease,
    namespace: airk_core::identity::Incarnation,
) -> Result<()> {
    record_registry(lease, namespace)
}
