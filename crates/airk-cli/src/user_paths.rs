// SPDX-License-Identifier: Apache-2.0
//! Native user identity selects defaults; XDG locations remain explicit user environment policy.
use anyhow::{Context, Result, ensure};
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
    fn leaf(self, role: &str) -> String {
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

pub(crate) fn canonical_location(path: &Path) -> Result<PathBuf> {
    checked(path.to_path_buf())?;
    ensure!(
        !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir)),
        "parent traversal is not a native location"
    );
    let mut parent = path.to_path_buf();
    let mut suffix = Vec::new();
    while !parent.try_exists()? {
        suffix.push(
            parent
                .file_name()
                .context("missing native location ancestor")?
                .to_owned(),
        );
        ensure!(parent.pop(), "missing native location ancestor");
    }
    ensure!(
        parent.is_dir(),
        "native location ancestor must be a directory"
    );
    let mut resolved = parent.canonicalize()?;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    checked(resolved)
}

#[cfg(any(target_os = "linux", test))]
fn xdg(value: Option<OsString>, home: &Path, fallback: &str) -> Result<PathBuf> {
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

    fn select_layout(&mut self, probe: InstallationProbe) -> Result<()> {
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
mod tests {
    use super::*;
    fn fixture_paths(base: &Path, channel: Channel) -> UserPaths {
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
    fn fresh_layout_separates_formats_from_runtime_and_preserves_existing_ownership_paths()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        for channel in [Channel::Stable, Channel::Preview] {
            let mut fresh = fixture_paths(owner.path(), channel);
            fresh.select_layout(crate::runtime_install::INSTALLATION_PROBE)?;
            assert!(!fresh.legacy_locations);
            assert_eq!(
                fresh.install.file_name().unwrap(),
                channel.leaf("runtime").as_str()
            );
            assert_eq!(
                fresh.data.file_name().unwrap(),
                channel.leaf("index-v1").as_str()
            );
            assert_eq!(
                fresh.state.file_name().unwrap(),
                channel.leaf("service-v1").as_str()
            );
            assert_eq!(fresh.logs, fresh.state.join("logs"));
            assert!(!fresh.install.exists() && !fresh.data.exists());
            let mut existing = fixture_paths(owner.path(), channel);
            std::fs::create_dir(&existing.install)?;
            let before = serde_json::to_value(&existing)?;
            existing.select_layout(crate::runtime_install::INSTALLATION_PROBE)?;
            assert_eq!(serde_json::to_value(existing)?, before);
        }
        Ok(())
    }
    #[test]
    fn selected_new_layout_is_not_shadowed_by_old_install_leftovers() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let mut paths = fixture_paths(owner.path(), Channel::Stable);
        std::fs::create_dir(&paths.install)?;
        let new_install = owner.path().join("runtime");
        std::fs::create_dir(&new_install)?;
        std::fs::create_dir(&paths.launcher)?;
        let executable = paths
            .launcher
            .join(crate::universal_launcher::launcher_name());
        std::fs::write(&executable, b"owned launcher fixture")?;
        std::fs::write(
            paths.launcher.join(crate::universal_launcher::CONFIG),
            serde_json::to_vec(&crate::universal_launcher::Pointer {
                schema: 1,
                install: new_install.canonicalize()?,
                launcher_sha256: crate::runtime_pair::digest_file(&executable)?,
            })?,
        )?;
        paths.select_layout(crate::runtime_install::INSTALLATION_PROBE)?;
        assert!(!paths.legacy_locations);
        assert_eq!(paths.install, new_install);
        assert_eq!(paths.data, owner.path().join("data/index-v1"));
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

    #[test]
    fn xdg_absolute_policy_and_separate_channels_do_not_create_directories() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let fake_home = owner.path().join("fictional-user");
        assert_eq!(xdg(None, &fake_home, ".config")?, fake_home.join(".config"));
        assert_eq!(
            xdg(Some(OsString::new()), &fake_home, ".config")?,
            fake_home.join(".config")
        );
        assert_eq!(
            xdg(Some("relative/workspace".into()), &fake_home, ".config")?,
            fake_home.join(".config")
        );
        assert_eq!(
            xdg(Some(owner.path().into()), &fake_home, ".config")?,
            owner.path()
        );
        assert!(!fake_home.exists());
        let stable =
            UserPaths::discover(Channel::Stable, crate::runtime_install::INSTALLATION_PROBE)?;
        let preview =
            UserPaths::discover(Channel::Preview, crate::runtime_install::INSTALLATION_PROBE)?;
        for (left, right) in [
            (stable.install, preview.install),
            (stable.configuration, preview.configuration),
            (stable.data, preview.data),
            (stable.state, preview.state),
            (stable.cache, preview.cache),
            (stable.logs, preview.logs),
        ] {
            assert_ne!(left, right);
            assert!(!left.starts_with(&right) && !right.starts_with(&left));
        }
        Ok(())
    }

    #[test]
    fn native_user_home_child_fixture() -> Result<()> {
        if let Some(expected) = std::env::var_os("AIRK_NATIVE_HOME_FIXTURE") {
            assert_eq!(home()?, PathBuf::from(expected));
            let expected: serde_json::Value =
                serde_json::from_str(&std::env::var("AIRK_NATIVE_PATHS_FIXTURE")?)?;
            assert_eq!(
                serde_json::to_value(UserPaths::discover(
                    Channel::Stable,
                    crate::runtime_install::INSTALLATION_PROBE
                )?)?,
                expected
            );
        }
        Ok(())
    }

    #[test]
    fn native_home_ignores_inherited_workspace_home_overrides() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let fake = owner.path().join("workspace-selected-home");
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "user_paths::tests::native_user_home_child_fixture",
                "--nocapture",
            ])
            .env("AIRK_NATIVE_HOME_FIXTURE", home()?)
            .env(
                "AIRK_NATIVE_PATHS_FIXTURE",
                serde_json::to_string(&UserPaths::discover(
                    Channel::Stable,
                    crate::runtime_install::INSTALLATION_PROBE,
                )?)?,
            )
            .env("HOME", &fake)
            .env("USERPROFILE", &fake)
            .env("LOCALAPPDATA", &fake)
            .env("APPDATA", &fake)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        let output = crate::native_process::without_error_dialogs(|| command.output())?;
        ensure!(
            output.status.success(),
            "native user identity fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!fake.exists());
        Ok(())
    }
}
