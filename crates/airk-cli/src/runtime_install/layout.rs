// SPDX-License-Identifier: Apache-2.0
//! Durable install/state binding. Interrupted claims never fall back to another ledger.
use super::*;
use crate::install_layout::{
    LAYOUT, Layout, OWNER_PART, control_binding, preflight_layout, require_control,
    validate_control,
};
mod successor;
pub(super) use successor::{admit_successor, preflight_clean_break, successor_handoff_started};

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
    let native = crate::runtime_install::discover_user_paths(channel)?;
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
    initialize_with_origin(root, requested, channel, None)
}

pub(super) fn initialize_managed(
    root: &Path,
    requested: Option<&Path>,
    channel: RuntimeChannel,
    reservation: &managed::catalog::VerifiedManagedReservation,
) -> Result<()> {
    initialize_with_origin(root, requested, channel, Some(reservation.origin()))
}

fn initialize_with_origin(
    root: &Path,
    requested: Option<&Path>,
    channel: RuntimeChannel,
    origin: Option<&airk_store::service::registry::catalog::genesis::ManagedGenesisOrigin>,
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
        let native = crate::runtime_install::discover_user_paths(channel).ok();
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
        managed_catalog_origin: origin.cloned(),
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
        if let Some(origin) = origin {
            origin.validate()?;
            ensure!(
                origin.install == layout.install && origin.state == layout.state,
                "managed reservation state differs"
            );
        }
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
        preflight_layout(&layout)?;
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

/// Establish only the control directory already reserved by the exact initial
/// managed layout. This is an owning write path, never a read/help fallback.
pub(super) fn prepare_managed_control(
    authority: &InstallerAuthority<'_>,
    origin: &airk_store::service::registry::catalog::genesis::ManagedGenesisOrigin,
) -> Result<()> {
    let layout = Layout::read(authority.root)?.context("managed layout reservation missing")?;
    ensure!(
        layout.ready
            && layout.claimed()?
            && layout.managed_catalog_origin.as_ref() == Some(origin)
            && origin.install == authority.root.canonicalize()?
            && origin.state == layout.state
            && origin.control == layout.state.join("control"),
        "managed control creation differs from the claimed reservation"
    );
    local_ipc::require_existing_root(&layout.state)?;
    ensure!(
        crate::user_paths::canonical_location(&origin.control)? == origin.control,
        "managed control destination is redirected"
    );
    if !exists(&origin.control)? {
        // Existing initialization/namespace markers prohibit recreation of a
        // lost control directory. A genuine first pin has only state ownership.
        // Existing fenced directories resume through their owned catalog lane.
        control_binding(&origin.control)?;
    }
    // Both platforms validate preexisting directories rather than repairing
    // their ownership/permissions. The claimed parent is already present.
    local_ipc::prepare_root(&origin.control)
        .context("prepare reserved managed control directory")?;
    require_control(&origin.control)
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

#[cfg(test)]
mod tests;
