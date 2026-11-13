// SPDX-License-Identifier: Apache-2.0
//! The locations an installed service uses (split U39, U89): when the service
//! runs for an installation, its log rings live in the install record's
//! `locations.logs` instead of the pre-split layout (`install_layout::log_parent`).
//!
//! Only the roles that start or serve an installation adopt its record, once
//! per process and before any log is opened: `service-detached` whose control
//! root is the record's `locations.state`, `service-login` (always the
//! record's state) and `record-error` for that control root. The
//! `service-foreground` role (tests, development, an explicit control folder)
//! adopts nothing and keeps the pre-split selection unchanged.
//!
//! State is the control root itself (`airkctl service start` and
//! `service-login` start the service at `locations.state`). Index data roots
//! are chosen per registration by the client, which defaults them to
//! `locations.data`; the service derives no data or cache folder of its own.
use airk_core::install_record::Locations;
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[cfg(test)]
mod tests;

/// An adopted installation's locations, normalized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub(crate) state: PathBuf,
    pub(crate) logs: PathBuf,
}

static ADOPTED: OnceLock<Installed> = OnceLock::new();

/// The comparable form of a location: absolute, the existing prefix resolved
/// (the same normalization `airkctl` applies to the record).
fn normal(path: &Path) -> Result<PathBuf> {
    crate::user_paths::canonical_location(&std::path::absolute(path)?)
}

/// The record's locations, normalized.
pub fn installed(locations: &Locations) -> Result<Installed> {
    Ok(Installed {
        state: normal(Path::new(&locations.state)).context("install record state location")?,
        logs: normal(Path::new(&locations.logs)).context("install record logs location")?,
    })
}

/// Pure: the installation a control root belongs to, if any.
pub(crate) fn belongs(control: &Path, installed: Installed) -> Option<Installed> {
    (installed.state == control).then_some(installed)
}

/// Pure: the log folder of `control`: the adopted installation's `logs` when
/// `control` is its state folder, else `None` (the pre-split selection).
pub(crate) fn logs_for(control: &Path, adopted: Option<&Installed>) -> Option<PathBuf> {
    adopted
        .filter(|installed| installed.state == control)
        .map(|installed| installed.logs.clone())
}

/// Adopt `installed` for this process. A second adoption must be the same.
pub fn adopt(installed: Installed) -> Result<()> {
    let current = ADOPTED.get_or_init(|| installed.clone());
    ensure!(
        *current == installed,
        "service locations were already adopted for another installation"
    );
    Ok(())
}

/// Adopt this user's installation (`setup_role::installed_locations`, read by
/// the caller) when its state folder is `control` (`service-detached`,
/// `record-error`). No installation, or another one, leaves the pre-split
/// selection; an unreadable installation is the caller's error.
pub fn adopt_for_control(control: &Path, installation: Option<Locations>) -> Result<bool> {
    let Some(locations) = installation else {
        return Ok(false);
    };
    match belongs(&normal(control)?, installed(&locations)?) {
        Some(installed) => {
            adopt(installed)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// The folder holding the log rings of `control`: the adopted installation's
/// `locations.logs`, otherwise `install_layout::log_parent`.
pub(crate) fn log_parent(control: &Path) -> Result<PathBuf> {
    if let Some(adopted) = ADOPTED.get()
        && let Some(logs) = logs_for(&normal(control)?, Some(adopted))
    {
        return Ok(logs);
    }
    crate::install_layout::log_parent(control)
}
