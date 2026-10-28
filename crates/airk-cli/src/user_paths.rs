// SPDX-License-Identifier: Apache-2.0
//! Native user identity selects defaults; XDG locations remain explicit user environment policy.
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

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

fn checked(path: PathBuf) -> Result<PathBuf> {
    let text = path
        .to_str()
        .context("native user directory is not UTF-8")?;
    ensure!(
        path.is_absolute() && text.len() <= 4096 && !text.chars().any(char::is_control),
        "invalid native user directory"
    );
    Ok(path)
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

#[cfg(windows)]
fn known_folder(id: &windows_sys::core::GUID) -> Result<PathBuf> {
    use std::os::windows::{
        ffi::OsStringExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use windows_sys::Win32::{
        Security::{TOKEN_IMPERSONATE, TOKEN_QUERY},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    struct Allocated(*mut u16);
    impl Drop for Allocated {
        fn drop(&mut self) {
            // SAFETY: SHGetKnownFolderPath uses COM task allocation, including on failure.
            unsafe {
                windows_sys::Win32::System::Com::CoTaskMemFree(self.0.cast());
            }
        }
    }
    let mut token = std::ptr::null_mut();
    // SAFETY: scalar rights, this process's pseudo-handle and writable token output.
    ensure!(
        unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_QUERY | TOKEN_IMPERSONATE,
                &mut token,
            )
        } != 0,
        "native user token unavailable: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: successful OpenProcessToken transfers one owned real handle.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut pointer = std::ptr::null_mut();
    // SAFETY: valid folder GUID, current process identity and writable output.
    // DONT_VERIFY performs discovery without creating or probing the target directory.
    let result = unsafe {
        windows_sys::Win32::UI::Shell::SHGetKnownFolderPath(
            id,
            0x4000,
            token.as_raw_handle(),
            &mut pointer,
        )
    };
    let allocation = Allocated(pointer);
    ensure!(
        result >= 0 && !allocation.0.is_null(),
        "native user folder is unavailable (HRESULT {result:#x})"
    );
    let mut length = 0usize;
    // SAFETY: the successful SDK result is a null-terminated UTF-16 allocation.
    while length < 32768 && unsafe { *allocation.0.add(length) } != 0 {
        length += 1;
    }
    ensure!(length < 32768, "native user folder exceeds path allowance");
    // SAFETY: the bounded prefix belongs to the still-owned native allocation.
    checked(PathBuf::from(OsString::from_wide(unsafe {
        std::slice::from_raw_parts(allocation.0, length)
    })))
}

#[cfg(windows)]
pub(crate) fn home() -> Result<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Profile)
}

#[cfg(unix)]
pub(crate) fn home() -> Result<PathBuf> {
    use std::{ffi::CStr, os::unix::ffi::OsStringExt};
    // SAFETY: scalar identity and limit queries take no writable pointers.
    let uid = unsafe { libc::geteuid() };
    let suggested = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    let mut size = if suggested > 0 {
        (suggested as usize).clamp(1024, 1024 * 1024)
    } else {
        16384
    };
    loop {
        let mut buffer = vec![0u8; size];
        // SAFETY: output structure is initialized by getpwuid_r on success.
        let mut password: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result = std::ptr::null_mut();
        // SAFETY: writable output, correctly bounded scratch allocation and result pointer.
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                &mut password,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && size < 1024 * 1024 {
            size = (size * 2).min(1024 * 1024);
            continue;
        }
        ensure!(
            status == 0
                && !result.is_null()
                && password.pw_uid == uid
                && !password.pw_dir.is_null(),
            "native user home is unavailable"
        );
        // SAFETY: successful getpwuid_r stores a terminated string in our live scratch buffer.
        let bytes = unsafe { CStr::from_ptr(password.pw_dir) }.to_bytes();
        ensure!(
            bytes.len() <= 4096,
            "native user home exceeds path allowance"
        );
        return checked(PathBuf::from(OsString::from_vec(bytes.to_vec())));
    }
}

#[cfg(not(any(windows, unix)))]
pub(crate) fn home() -> Result<PathBuf> {
    anyhow::bail!("native user directories are unsupported on this platform")
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
        known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_RoamingAppData)
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

impl UserPaths {
    pub(crate) fn discover(channel: Channel) -> Result<Self> {
        #[cfg(unix)]
        let home = home()?;
        let leaf = channel.legacy_leaf();
        #[cfg(windows)]
        let (install, configuration, data, state, cache, logs) = {
            let local =
                known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData)?.join("AIRK");
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
            let mut paths = Self {
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
            paths.select_layout()?;
            Ok(paths)
        }
    }

    fn select_layout(&mut self) -> Result<()> {
        let install = self.install.with_file_name(self.channel.leaf("runtime"));
        // Preserve existing ownership locations. If both layouts exist, a
        // verified universal launcher already selecting the new installation
        // takes precedence over leftovers from an older one.
        let new_selected = self.install.try_exists()?
            && install.try_exists()?
            && self
                .launcher
                .join(crate::universal_launcher::CONFIG)
                .try_exists()?
            && crate::universal_launcher::resolve(
                &self
                    .launcher
                    .join(crate::universal_launcher::launcher_name()),
            )? == install.canonicalize()?;
        if self.install.try_exists()? && !new_selected {
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
    fn fresh_layout_separates_formats_from_runtime_and_preserves_existing_ownership_paths()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        for channel in [Channel::Stable, Channel::Preview] {
            let mut fresh = fixture_paths(owner.path(), channel);
            fresh.select_layout()?;
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
            existing.select_layout()?;
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
        paths.select_layout()?;
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
        let stable = UserPaths::discover(Channel::Stable)?;
        let preview = UserPaths::discover(Channel::Preview)?;
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
                serde_json::to_value(UserPaths::discover(Channel::Stable)?)?,
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
                serde_json::to_string(&UserPaths::discover(Channel::Stable)?)?,
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
