// SPDX-License-Identifier: Apache-2.0
//! Native user identity selects defaults; XDG locations remain explicit user environment policy.
use anyhow::{Context, Result};
use serde::Serialize;
#[cfg(any(target_os = "linux", test))]
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Stable,
    Preview,
}
impl Channel {
    fn legacy_leaf(self) -> &'static str {
        match self {
            Self::Stable => "service-1.4",
            Self::Preview => "service-1.4-preview",
        }
    }
    pub(crate) fn leaf(self, role: &str) -> String {
        if self == Self::Preview {
            format!("{role}-preview")
        } else {
            role.into()
        }
    }
    /// Version-free sibling of the install root for the universal launcher.
    fn launcher_leaf(self) -> &'static str {
        match self {
            Self::Stable => "launcher",
            Self::Preview => "launcher-preview",
        }
    }
}

/// Installation facts that path discovery consults but does not own. The
/// installer supplies them (`runtime_install::INSTALLATION_PROBE`), so this
/// module depends on no installer or launcher module.
#[derive(Clone, Copy)]
pub(crate) struct InstallationProbe {
    /// Control root recorded by the claimed state layout of an installation root.
    pub(crate) saved_control: fn(&Path) -> Result<PathBuf>,
    /// The installation selected by the universal launcher in a launcher
    /// directory, or `None` when that directory has no launcher configuration.
    pub(crate) universal_selection: fn(&Path) -> Result<Option<PathBuf>>,
}

#[derive(Serialize)]
pub(crate) struct UserPaths {
    pub channel: Channel,
    /// Existing ownership records contain absolute paths. Reuse those locations
    /// until an explicit owned handover; never silently create a second service.
    pub legacy_locations: bool,
    pub install: PathBuf,
    /// Unversioned directory of the universal launcher that agent configurations name.
    pub launcher: PathBuf,
    pub configuration: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
}

/// Path validation, shared with `airk-process` (U84).
fn checked(path: PathBuf) -> Result<PathBuf> {
    airk_process::user_folders::checked(path)
}

/// The native user home (U84: `airk-process`).
pub(crate) fn home() -> Result<PathBuf> {
    airk_process::user_folders::home()
}

/// Moved to `airk-process` (U86); forwarded for existing callers.
pub(crate) fn canonical_location(path: &Path) -> Result<PathBuf> {
    airk_process::user_folders::canonical_location(path)
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn xdg(value: Option<OsString>, home: &Path, fallback: &str) -> Result<PathBuf> {
    // The specification requires relative and empty overrides to be ignored.
    match value.map(PathBuf::from).filter(|path| path.is_absolute()) {
        Some(path) => checked(path),
        None => checked(home.join(fallback)),
    }
}

pub(crate) fn configuration_base() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        airk_process::user_folders::known_folder(
            airk_process::user_folders::KnownFolder::RoamingAppData,
        )
        .map(|path| path.join("AIRK"))
    }
    #[cfg(target_os = "linux")]
    {
        xdg(std::env::var_os("XDG_CONFIG_HOME"), &home()?, ".config")
    }
    #[cfg(target_os = "macos")]
    {
        Ok(home()?.join("Library/Application Support/AIRK"))
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        anyhow::bail!("native service configuration directories are unsupported")
    }
}

#[cfg(any(target_os = "macos", test))]
fn mac_directories(
    home: &Path,
    leaf: &str,
) -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    let support = home.join("Library/Application Support/AIRK");
    (
        support.join(leaf),
        support.join("configuration").join(leaf),
        support.join("data").join(leaf),
        support.join("state").join(leaf),
        home.join("Library/Caches/AIRK").join(leaf),
        home.join("Library/Logs/AIRK").join(leaf),
    )
}

/// Only the default layout selected by this exact installation. Other channels
/// and another installation in the same channel retain independent ownership.
pub(crate) fn owned_install_locations(install: &Path) -> Result<Vec<PathBuf>> {
    if !is_default_install(install)? {
        return Ok(Vec::new());
    }
    let actual = canonical_location(install)?;
    for channel in [Channel::Stable, Channel::Preview] {
        let historical = UserPaths::historical(channel)?;
        if let Some(locations) = owned_layout_locations(historical, &actual)? {
            return Ok(locations);
        }
    }
    Ok(Vec::new())
}

fn owned_layout_locations(historical: UserPaths, actual: &Path) -> Result<Option<Vec<PathBuf>>> {
    let channel = historical.channel;
    if canonical_location(&historical.install)? == actual {
        return Ok(Some(vec![
            historical.install,
            historical.configuration,
            historical.data,
            historical.state,
            historical.cache,
            historical.logs,
        ]));
    }
    let current = historical.install.with_file_name(channel.leaf("runtime"));
    if canonical_location(&current)? == actual {
        let state = historical.state.with_file_name(channel.leaf("service-v1"));
        let logs = if historical.logs.starts_with(&historical.state) {
            state.join(historical.logs.strip_prefix(&historical.state)?)
        } else {
            historical.logs.with_file_name(channel.leaf("logs-v1"))
        };
        return Ok(Some(vec![
            current,
            historical
                .configuration
                .with_file_name(channel.leaf("settings")),
            historical.data.with_file_name(channel.leaf("index-v1")),
            state,
            historical.cache.with_file_name(channel.leaf("cache-v1")),
            logs,
        ]));
    }
    Ok(None)
}

/// True when `install` is one of the default installation roots.
pub(crate) fn is_default_install(install: &Path) -> Result<bool> {
    let actual = canonical_location(install)?;
    for channel in [Channel::Stable, Channel::Preview] {
        let historical = UserPaths::historical(channel)?;
        let current = historical.install.with_file_name(channel.leaf("runtime"));
        if canonical_location(&historical.install)? == actual
            || canonical_location(&current)? == actual
        {
            return Ok(true);
        }
    }
    Ok(false)
}

impl UserPaths {
    pub(crate) fn discover(channel: Channel, probe: InstallationProbe) -> Result<Self> {
        let mut paths = Self::historical(channel)?;
        paths.select_layout(probe)?;
        Ok(paths)
    }

    pub(crate) fn historical(channel: Channel) -> Result<Self> {
        #[cfg(unix)]
        let home = home()?;
        let leaf = channel.legacy_leaf();
        #[cfg(windows)]
        let (install, configuration, data, state, cache, logs) = {
            let local = airk_process::user_folders::known_folder(
                airk_process::user_folders::KnownFolder::LocalAppData,
            )?
            .join("AIRK");
            let install = local.join(leaf);
            let configuration = configuration_base()?.join(leaf);
            let data = local.join("data").join(leaf);
            let state = local.join("state").join(leaf);
            let cache = local.join("cache").join(leaf);
            let logs = state.join("logs");
            (install, configuration, data, state, cache, logs)
        };
        #[cfg(target_os = "linux")]
        let (install, configuration, data, state, cache, logs) = {
            let install = home.join(".local/lib/airk").join(leaf);
            let configuration = configuration_base()?.join("airk").join(leaf);
            let data = xdg(std::env::var_os("XDG_DATA_HOME"), &home, ".local/share")?
                .join("airk")
                .join(leaf);
            let state = xdg(std::env::var_os("XDG_STATE_HOME"), &home, ".local/state")?
                .join("airk")
                .join(leaf);
            let cache = xdg(std::env::var_os("XDG_CACHE_HOME"), &home, ".cache")?
                .join("airk")
                .join(leaf);
            let logs = state.join("logs");
            (install, configuration, data, state, cache, logs)
        };
        #[cfg(target_os = "macos")]
        let (install, configuration, data, state, cache, logs) = mac_directories(&home, leaf);
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            let _ = leaf;
            anyhow::bail!("native service directories are unsupported");
        }
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            let paths = Self {
                channel,
                legacy_locations: true,
                launcher: checked(
                    install
                        .parent()
                        .context("install root has no parent")?
                        .join(channel.launcher_leaf()),
                )?,
                install: checked(install)?,
                configuration: checked(configuration)?,
                data: checked(data)?,
                state: checked(state)?,
                cache: checked(cache)?,
                logs: checked(logs)?,
            };
            Ok(paths)
        }
    }

    pub(crate) fn select_layout(&mut self, probe: InstallationProbe) -> Result<()> {
        let install = self.install.with_file_name(self.channel.leaf("runtime"));
        // Preserve existing ownership locations. If both layouts exist, a
        // verified universal launcher already selecting the new installation
        // takes precedence over leftovers from an older one.
        let new_selected = self.install.try_exists()?
            && install.try_exists()?
            && match (probe.universal_selection)(&self.launcher)? {
                Some(selected) => selected == install.canonicalize()?,
                None => false,
            };
        if self.install.try_exists()? && !new_selected {
            self.select_migrated_data(probe.saved_control)?;
            return Ok(());
        }
        self.install = checked(install)?;
        self.configuration = checked(
            self.configuration
                .with_file_name(self.channel.leaf("settings")),
        )?;
        self.data = checked(self.data.with_file_name(self.channel.leaf("index-v1")))?;
        let previous_state = self.state.clone();
        self.state = checked(self.state.with_file_name(self.channel.leaf("service-v1")))?;
        self.cache = checked(self.cache.with_file_name(self.channel.leaf("cache-v1")))?;
        self.logs = checked(if self.logs.starts_with(&previous_state) {
            self.state.join(self.logs.strip_prefix(&previous_state)?)
        } else {
            self.logs.with_file_name(self.channel.leaf("logs-v1"))
        })?;
        self.legacy_locations = false;
        Ok(())
    }

    pub(crate) fn select_migrated_data(
        &mut self,
        saved_control: fn(&Path) -> Result<PathBuf>,
    ) -> Result<()> {
        if !self.install.join("runtime-layout.json").try_exists()? {
            return Ok(());
        }
        let control = saved_control(&self.install)?;
        let migration = airk_store::service::registry::data_relocation::require_settled(&control);
        if migration.is_err() {
            // Installation resumes its exact journal; discovery never starts a second root.
            return Ok(());
        }
        let destination = self.data.with_file_name(self.channel.leaf("index-v1"));
        if airk_store::service::registry::data_relocation::selected(
            &control,
            &canonical_location(&self.data)?,
            &canonical_location(&destination)?,
        )? {
            self.data = checked(destination)?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn fixture_paths(base: &Path, channel: Channel) -> UserPaths {
        let leaf = channel.legacy_leaf();
        UserPaths {
            channel,
            legacy_locations: true,
            install: base.join(leaf),
            launcher: base.join(channel.launcher_leaf()),
            configuration: base.join("configuration").join(leaf),
            data: base.join("data").join(leaf),
            state: base.join("state").join(leaf),
            cache: base.join("cache").join(leaf),
            logs: base.join("state").join(leaf).join("logs"),
        }
    }

    #[test]
    fn replacement_locations_preserve_other_channels_and_independent_default_layouts() -> Result<()>
    {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let stable = fixture_paths(&base, Channel::Stable);
        let preview = fixture_paths(&base, Channel::Preview);
        fs_fixture_sentinel(&preview.install)?;
        let current = stable.install.with_file_name("runtime");
        let locations =
            owned_layout_locations(stable, &current)?.context("stable current layout")?;
        assert!(locations.contains(&current));
        assert!(
            !locations.contains(&base.join("service-1.4")),
            "historical install is independent"
        );
        assert!(
            locations
                .iter()
                .all(|path| !path.starts_with(&preview.install)
                    && !path.starts_with(&preview.state)
                    && !path.starts_with(&preview.data))
        );
        assert!(
            owned_layout_locations(preview, &current)?.is_none(),
            "stable cannot authorize preview roots"
        );
        assert_eq!(
            std::fs::read(base.join("service-1.4-preview").join("source.txt"))?,
            b"other installation source"
        );
        Ok(())
    }

    fn fs_fixture_sentinel(path: &Path) -> Result<()> {
        std::fs::create_dir_all(path)?;
        std::fs::write(path.join("source.txt"), b"other installation source")?;
        Ok(())
    }

    #[test]
    fn mac_defaults_keep_collection_data_outside_installer_and_control_ownership() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let (install, configuration, data, state, cache, logs) =
            mac_directories(owner.path(), "service-1.4");
        for left in [&install, &configuration, &data, &state, &cache, &logs] {
            assert!(!left.exists());
            for right in [&install, &configuration, &data, &state, &cache, &logs] {
                if left != right {
                    assert!(!left.starts_with(right) && !right.starts_with(left));
                }
            }
        }
        assert!(data.starts_with(owner.path().join("Library/Application Support/AIRK/data")));
        Ok(())
    }
}
