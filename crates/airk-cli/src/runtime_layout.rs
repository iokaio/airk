// SPDX-License-Identifier: Apache-2.0
//! Durable install/state binding. Interrupted claims never fall back to another ledger.
use super::*;

const LAYOUT: &str = "runtime-layout.json";
const OWNER: &str = "airk-state-1.4.owner.json";
const CLAIM: &str = "airk-state-1.4.lock";
const MAX_LAYOUT: u64 = 32 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Layout {
    schema: u32,
    install: PathBuf,
    state: PathBuf,
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
    fn claim(&self) -> Result<()> {
        if self.state != self.install && exists(&self.state)? && !self.claimed()? {
            let mut entries = fs::read_dir(&self.state)?;
            if let Some(first) = entries.next().transpose()? {
                // A pending claim can die after creating its empty OS lock and
                // before publishing ownership. No ledger can exist at that point.
                ensure!(
                    !self.ready
                        && first.path() == self.state.join(CLAIM)
                        && entries.next().transpose()?.is_none(),
                    "unknown nonempty service-state directory preserved"
                );
                safe_file(&first.path())?;
                ensure!(
                    first.metadata()?.len() == 0,
                    "unknown state claim contents preserved"
                );
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
            let mut temporary = tempfile::NamedTempFile::new_in(&self.state)?;
            temporary.write_all(&bytes)?;
            temporary.as_file().sync_all()?;
            temporary.persist_noclobber(self.state.join(OWNER))?;
            sync_directory(&self.state)?;
        }
        Ok(())
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
    let state = if let Some(requested) = requested {
        crate::user_paths::canonical_location(requested)?
    } else if let Some(existing) = &existing {
        existing.state.clone()
    } else {
        let native = crate::user_paths::UserPaths::discover(channel)?;
        if crate::user_paths::canonical_location(&native.install)? == install {
            crate::user_paths::canonical_location(&native.state)?
        } else {
            install.clone()
        }
    };
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
    let mut layout = Layout {
        schema: 1,
        install,
        state,
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
            Ok(layout.state.join("control"))
        }
    }
}

pub(super) fn inspect(root: &Path) -> Result<serde_json::Value> {
    Ok(match Layout::read(root)? {
        None => {
            serde_json::json!({"layout":"unconfigured","control":null})
        }
        Some(layout) => {
            let claimed = layout.claimed()?;
            serde_json::json!({"layout":layout,"state_claimed":claimed,"control":layout.state.join("control")})
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
