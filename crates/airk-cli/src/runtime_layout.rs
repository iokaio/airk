// SPDX-License-Identifier: Apache-2.0
//! Durable install/state binding. Interrupted claims never fall back to another ledger.
use super::*;

const LAYOUT: &str = "runtime-layout.json";
const OWNER: &str = "airk-state-1.4.owner.json";
const CLAIM: &str = "airk-state-1.4.lock";
const OWNER_PART: &str = "airk-state-1.4.owner.part";
const MAX_LAYOUT: u64 = 32 * 1024;
const CONTROL_OWNER: &str = "airk-control-1.4.owner.json";
const CONTROL_PART: &str = "airk-control-1.4.owner.part";
#[path = "runtime_registry_binding.rs"]
mod registry_binding;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Layout {
    schema: u32,
    install: PathBuf,
    state: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logs: Option<PathBuf>,
    channel: RuntimeChannel,
    ready: bool,
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn publish_new(from: &Path, to: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from = from
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let to = to
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // SAFETY: terminated paths; zero flags forbid replacement of an existing destination.
        ensure!(
            unsafe {
                windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0)
            } != 0,
            "exclusive state publication failed: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(unix)]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let from = CString::new(from.as_os_str().as_bytes())?;
        let to = CString::new(to.as_os_str().as_bytes())?;
        #[cfg(target_os = "linux")]
        // SAFETY: terminated absolute paths and the documented exclusive-rename flag.
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        // SAFETY: terminated absolute paths and the documented exclusive-rename flag.
        let result = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        anyhow::bail!("exclusive state publication is unsupported on this platform");
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        ensure!(
            result == 0,
            "filesystem cannot publish state exclusively: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}
impl Layout {
    fn read(root: &Path) -> Result<Option<Self>> {
        if !exists(&root.join(LAYOUT))? {
            return Ok(None);
        }
        let layout: Self = serde_json::from_slice(&read_bounded(&root.join(LAYOUT), MAX_LAYOUT)?)?;
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
        Ok(Some(layout))
    }
    fn owner(&self) -> Self {
        let mut owner = self.clone();
        owner.ready = true;
        owner
    }
    fn claimed(&self) -> Result<bool> {
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
    fn claim(&self) -> Result<()> {
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

/// Call only while holding the installation lock, after approving the source payload.
pub(super) fn planned_state(
    root: &Path,
    requested: Option<&Path>,
    channel: RuntimeChannel,
) -> Result<PathBuf> {
    if let Some(requested) = requested {
        return crate::user_paths::canonical_location(requested);
    }
    if let Some(existing) = Layout::read(root)? {
        return Ok(existing.state);
    }
    let install = crate::user_paths::canonical_location(root)?;
    let native = crate::user_paths::UserPaths::discover(channel)?;
    if crate::user_paths::canonical_location(&native.install)? == install {
        crate::user_paths::canonical_location(&native.state)
    } else {
        Ok(install)
    }
}

/// Call only while holding the installation lock, after approving the source payload.
pub(super) fn initialize(
    root: &Path,
    requested: Option<&Path>,
    channel: RuntimeChannel,
) -> Result<()> {
    let install = root.canonicalize()?;
    let existing = Layout::read(root)?;
    let state = planned_state(root, requested, channel)?;
    ensure!(
        !state.starts_with(install.join("runtimes")),
        "service state must stay outside immutable runtime payloads"
    );
    if existing.is_none() && state != install && exists(&install.join("control"))? {
        ensure!(
            fs::read_dir(install.join("control"))?
                .next()
                .transpose()?
                .is_none(),
            "existing control state requires an explicit transition; it cannot be silently relocated"
        );
    }
    let logs = if let Some(existing) = &existing {
        existing.logs.clone()
    } else if state == install
        && exists(&install.join("control"))?
        && fs::read_dir(install.join("control"))?
            .next()
            .transpose()?
            .is_some()
    {
        // Explicit adoption of an earlier raw development namespace must keep
        // its already protected log location. Registered source paths may exist
        // elsewhere under this installation; never start writing into them.
        None
    } else {
        let native = crate::user_paths::UserPaths::discover(channel).ok();
        let native_logs = native
            .as_ref()
            .filter(|native| {
                crate::user_paths::canonical_location(&native.install).is_ok_and(|p| p == install)
                    && crate::user_paths::canonical_location(&native.state)
                        .is_ok_and(|p| p == state)
            })
            .map(|native| native.logs.clone());
        Some(crate::user_paths::canonical_location(
            &native_logs.unwrap_or_else(|| state.join("logs")),
        )?)
    };
    let mut layout = Layout {
        schema: 1,
        install,
        state,
        logs,
        channel,
        ready: false,
    };
    if existing.is_none() && exists(&root.join("selected.json"))? {
        ensure!(
            requested.is_some() && layout.claimed()?,
            "an existing selection has lost its state layout; supply the reviewed, previously claimed --state-dir instead of creating another ledger"
        );
    }
    if let Some(existing) = existing {
        ensure!(
            existing.owner() == layout.owner(),
            "runtime state location/channel is already pinned; inspect the existing layout"
        );
        layout.ready = existing.ready;
    } else {
        if !layout.claimed()? {
            ensure!(
                !exists(&layout.state.join(OWNER_PART))?,
                "unrecognized ownership partial predates this state claim; preserving it"
            );
            if layout.state != layout.install && exists(&layout.state)? {
                ensure!(
                    fs::read_dir(&layout.state)?.next().transpose()?.is_none(),
                    "unknown nonempty service-state directory preserved"
                );
            }
        }
        ensure!(
            serde_json::to_vec(&layout)?.len() as u64 <= MAX_LAYOUT,
            "runtime layout exceeds metadata bound"
        );
        // This is also the durable initialization intent. A failed claim remains
        // diagnosable and cannot silently route requests back to install/control.
        replace_json(root, LAYOUT, &layout)?;
    }
    layout.claim()?;
    if !layout.ready {
        layout.ready = true;
        replace_json(root, LAYOUT, &layout)?;
    }
    Ok(())
}

pub(super) fn control(root: &Path) -> Result<PathBuf> {
    match Layout::read(root)? {
        None => anyhow::bail!(
            "service state layout is missing; repeat approved staging with its reviewed state location"
        ),
        Some(layout) => {
            ensure!(
                layout.ready && layout.claimed()?,
                "service state initialization is incomplete; repeat approved staging or inspect the layout"
            );
            local_ipc::prepare_root(&layout.state)?;
            let control = layout.state.join("control");
            validate_control(&control)?;
            Ok(control)
        }
    }
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
fn require_control(control: &Path) -> Result<()> {
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
fn control_binding(control: &Path) -> Result<Option<Layout>> {
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
    registry_binding::validate(&owner, completed)?;
    Ok(Some(owner))
}
/// Must run before directory creation, including explicit foreground startup.
pub(super) fn validate_control(control: &Path) -> Result<()> {
    control_binding(&control_location(control)?)?;
    Ok(())
}
/// Log lookup may use verified layout ownership even when registry authority is unavailable.
pub(super) fn saved_control(root: &Path) -> Result<PathBuf> {
    let layout = Layout::read(root)?.context("runtime state layout missing")?;
    ensure!(
        layout.ready && layout.claimed()?,
        "runtime state layout is not claimed"
    );
    Ok(layout.state.join("control"))
}
pub(super) fn retained_parameters(root: &Path) -> Result<(PathBuf, RuntimeChannel)> {
    let layout = Layout::read(root)?.context("runtime state layout missing")?;
    ensure!(
        layout.ready && layout.claimed()?,
        "runtime state layout is not claimed"
    );
    Ok((layout.state, layout.channel))
}
/// Record initialization before any registry creation, under singleton ownership.
pub(super) fn record_control(lease: &airk_store::service::ServiceLease) -> Result<()> {
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

pub(super) fn record_registry(
    lease: &airk_store::service::ServiceLease,
    namespace: airk_core::identity::Incarnation,
) -> Result<()> {
    let Some(owner) = control_binding(&control_location(lease.root())?)? else {
        return Ok(());
    };
    registry_binding::finish(lease, &owner, namespace)
}

pub(super) fn log_parent(control: &Path) -> Result<PathBuf> {
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

pub(super) fn inspect(root: &Path) -> Result<serde_json::Value> {
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

pub(super) fn abort(root: &Path) -> Result<()> {
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

#[cfg(test)]
#[path = "runtime_layout_tests.rs"]
mod tests;
