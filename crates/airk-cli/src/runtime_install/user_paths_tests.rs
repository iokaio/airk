// SPDX-License-Identifier: Apache-2.0
//! `user_paths` layout and discovery tests that use the installer's
//! `INSTALLATION_PROBE` and universal launcher. Moved from `user_paths::tests`
//! (split step 2b) so `user_paths` compiles its tests below the installer.
use crate::user_paths::tests::fixture_paths;
use crate::user_paths::{Channel, UserPaths, home, xdg};
use anyhow::{Result, ensure};
use std::ffi::OsString;
use std::path::PathBuf;

#[test]
fn fresh_layout_separates_formats_from_runtime_and_preserves_existing_ownership_paths() -> Result<()>
{
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
    let stable = UserPaths::discover(Channel::Stable, crate::runtime_install::INSTALLATION_PROBE)?;
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
            "runtime_install::user_paths_tests::native_user_home_child_fixture",
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
