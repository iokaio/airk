// SPDX-License-Identifier: Apache-2.0
//! The universal launcher: a version-free entry point that agent configurations name.
//!
//! The per-installation stable launcher lives inside its owned install root
//! (`…/AIRK/runtime/airk-launcher`, or a historical location). The universal launcher lives in an unversioned
//! sibling (`…/AIRK/launcher/airk-launcher`) next to `airk-launcher.json`, which names
//! the current install root. Every invocation reads that file again and hands its
//! arguments and stdio to the named installation's own pinned launcher, so a later
//! installation only rewrites the file; agent configurations never change. The universal
//! copy delegates unknown launcher implementations, keeping old copies compatible
//! with newer installations. An identical verified launcher can skip that extra hop.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

pub const CONFIG: &str = "airk-launcher.json";
const CONFIG_LIMIT: u64 = 4096;

/// `airk-launcher.json`: which installation the universal launcher forwards to.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pointer {
    pub schema: u32,
    /// Canonical install root whose own stable launcher receives the call.
    pub install: PathBuf,
    /// SHA-256 of the universal launcher binary beside this file; a copied or replaced
    /// binary does not inherit the pointer.
    pub launcher_sha256: String,
}

pub fn launcher_name() -> &'static str {
    if cfg!(windows) {
        "airk-launcher.exe"
    } else {
        "airk-launcher"
    }
}

fn digest(path: &Path) -> Result<String> {
    crate::runtime_pair::digest_file(path)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= CONFIG_LIMIT,
        "universal launcher configuration must be a small regular file"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(CONFIG_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= CONFIG_LIMIT,
        "universal launcher configuration is too large"
    );
    Ok(bytes)
}

/// Unix: the directory and pointer must belong to this user and not be group/other writable.
#[cfg(unix)]
fn owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: geteuid takes no pointers and has no failure mode.
    let user = unsafe { libc::geteuid() };
    for candidate in [path, path.parent().context("pointer parent missing")?] {
        let metadata = fs::symlink_metadata(candidate)?;
        ensure!(
            metadata.uid() == user && metadata.mode() & 0o022 == 0,
            "universal launcher files must be owned by this user and not group/other writable"
        );
    }
    Ok(())
}

/// Resolve the install root for a universal launcher at `executable`, reading the pointer now.
pub fn resolve(executable: &Path) -> Result<PathBuf> {
    let directory = executable
        .parent()
        .context("universal launcher parent missing")?;
    let config = directory.join(CONFIG);
    #[cfg(unix)]
    owner_only(&config)?;
    let pointer: Pointer = serde_json::from_slice(&read_bounded(&config)?)
        .context("universal launcher configuration is invalid")?;
    ensure!(
        pointer.schema == 1,
        "unsupported universal launcher configuration"
    );
    ensure!(
        digest(executable)? == pointer.launcher_sha256.to_ascii_lowercase(),
        "universal launcher binary does not match its configuration"
    );
    ensure!(
        pointer.install.is_absolute(),
        "universal launcher configuration must name an absolute install root"
    );
    let install = pointer.install.canonicalize().with_context(|| {
        "the installation named by the universal launcher is unavailable; reopen VS Code to refresh it"
    })?;
    ensure!(
        install != directory.canonicalize()?,
        "universal launcher must not forward to itself"
    );
    Ok(install)
}

/// Whether `executable` is a universal launcher (a pointer file sits beside it).
pub fn is_universal(executable: &Path) -> bool {
    executable.file_name() == Some(launcher_name().as_ref())
        && executable
            .parent()
            .is_some_and(|directory| directory.join(CONFIG).is_file())
}

/// Hand the call to the named installation's own verified stable launcher.
pub fn run(executable: &Path, args: Vec<OsString>) -> Result<()> {
    crate::client_owner::watch()?;
    let install = resolve(executable)?;
    let target = crate::runtime_install::verified_launcher(&install)?;
    if digest(&target)? == digest(executable)?
        && !args.iter().any(|arg| {
            arg == "--service-install-dir"
                || arg
                    .to_str()
                    .is_some_and(|arg| arg.starts_with("--service-install-dir="))
        })
        && let Ok((_, main, _)) = crate::runtime_install::selected(&install)
    {
        // Identical verified implementations can skip the intermediate launcher.
        // Missing/broken selections still delegate so runtime repair commands work.
        let mut command = Command::new(main);
        command
            .arg("--service-install-dir")
            .arg(&install)
            .args(args);
        return crate::native_process::forward_stdio(command);
    }
    let mut command = Command::new(target);
    command.args(args);
    crate::native_process::forward_stdio(command)
}

/// Install or refresh the universal launcher in `directory` for `install`. The binary is
/// copied once from the installation's verified launcher and then kept; only the pointer
/// changes. An unrecognized binary at the destination is never replaced.
pub fn publish_at(directory: &Path, install: &Path) -> Result<PathBuf> {
    let install = install.canonicalize()?;
    let source = crate::runtime_install::verified_launcher(&install)?;
    publish_from(directory, &install, &source)
}

/// `publish_at` with the installation's verified launcher already resolved.
fn publish_from(directory: &Path, install: &Path, source: &Path) -> Result<PathBuf> {
    let install = install.to_path_buf();
    fs::create_dir_all(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    let executable = directory.join(launcher_name());
    let config = directory.join(CONFIG);
    let existing: Option<Pointer> = if config.try_exists()? {
        Some(
            serde_json::from_slice(&read_bounded(&config)?)
                .context("existing universal launcher configuration is invalid")?,
        )
    } else {
        None
    };
    let launcher_sha256 = if executable.try_exists()? {
        let metadata = fs::symlink_metadata(&executable)?;
        ensure!(
            metadata.is_file(),
            "universal launcher must be a regular file"
        );
        let current = digest(&executable)?;
        let known = existing
            .as_ref()
            .map(|pointer| pointer.launcher_sha256.as_str());
        ensure!(
            known == Some(current.as_str()) || current == digest(source)?,
            "unrecognized universal launcher; refusing replacement"
        );
        current
    } else {
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        std::io::copy(&mut fs::File::open(source)?, &mut temporary)?;
        temporary.as_file().sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temporary
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o700))?;
        }
        let copied = digest(temporary.path())?;
        ensure!(
            copied == digest(source)?,
            "stable launcher changed while publishing the universal launcher"
        );
        temporary.persist_noclobber(&executable)?;
        copied
    };
    let pointer = Pointer {
        schema: 1,
        install,
        launcher_sha256,
    };
    if existing.as_ref() != Some(&pointer) {
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer_pretty(&mut temporary, &pointer)?;
        temporary.as_file().sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temporary
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        temporary.persist(&config)?;
    }
    Ok(executable)
}

/// The universal launcher for the native default installation of `channel`, if `install`
/// is that installation. Custom and managed install roots get none.
pub fn publish_for_default(
    install: &Path,
    channel: crate::user_paths::Channel,
) -> Result<Option<PathBuf>> {
    let paths = crate::user_paths::UserPaths::discover(channel)?;
    if crate::user_paths::canonical_location(&paths.install)? != install.canonicalize()? {
        return Ok(None);
    }
    publish_at(&paths.launcher, install).map(Some)
}

/// The published universal launcher that currently forwards to `install`, if any.
pub fn current_for(install: &Path, channel: crate::user_paths::Channel) -> Result<Option<PathBuf>> {
    let paths = crate::user_paths::UserPaths::discover(channel)?;
    let executable = paths.launcher.join(launcher_name());
    if !executable.is_file() {
        return Ok(None);
    }
    Ok(resolve(&executable)
        .ok()
        .filter(|target| {
            install
                .canonicalize()
                .is_ok_and(|install| install == *target)
        })
        .map(|_| executable))
}

/// Remove only the verified universal entry point for this installation. A saved
/// receipt resumes interruption between deleting the executable and its pointer.
pub(crate) fn retire_for(install: &Path, channel: crate::user_paths::Channel) -> Result<()> {
    let paths = crate::user_paths::UserPaths::discover(channel)?;
    retire_at(&paths.launcher, install)
}

fn retire_at(directory: &Path, install: &Path) -> Result<()> {
    let receipt = install.join("universal-uninstall.json");
    let executable = directory.join(launcher_name());
    let config = directory.join(CONFIG);
    let pointer: Pointer = if receipt.try_exists()? {
        crate::runtime_install::safe_file(&receipt)?;
        serde_json::from_slice(&read_bounded(&receipt)?)?
    } else {
        if !executable.try_exists()? || !config.try_exists()? {
            return Ok(());
        }
        if resolve(&executable)? != install.canonicalize()? {
            return Ok(());
        }
        crate::runtime_install::safe_file(&config)?;
        let bytes = read_bounded(&config)?;
        let pointer: Pointer = serde_json::from_slice(&bytes)?;
        use std::io::Write;
        let mut saved = tempfile::NamedTempFile::new_in(install)?;
        saved.write_all(&bytes)?;
        saved.as_file().sync_all()?;
        saved.persist_noclobber(&receipt)?;
        pointer
    };
    ensure!(
        pointer.schema == 1 && pointer.install == install.canonicalize()?,
        "universal uninstall receipt names another installation"
    );
    if config.try_exists()? {
        crate::runtime_install::safe_file(&config)?;
        ensure!(
            serde_json::from_slice::<Pointer>(&read_bounded(&config)?)? == pointer,
            "changed universal launcher configuration preserved during uninstall"
        );
    }
    if executable.try_exists()? {
        crate::runtime_install::safe_file(&executable)?;
        ensure!(
            digest(&executable)? == pointer.launcher_sha256,
            "changed universal launcher preserved during uninstall"
        );
        fs::remove_file(&executable)?;
    }
    if config.try_exists()? {
        fs::remove_file(config)?;
    }
    fs::remove_file(receipt)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninstall_removes_only_the_owned_entry_point_and_resumes_partial_removal() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let install = owner.path().canonicalize()?.join("install");
        let directory = owner.path().canonicalize()?.join("launcher");
        crate::local_ipc::prepare_root(&install)?;
        crate::local_ipc::prepare_root(&directory)?;
        let executable = directory.join(launcher_name());
        fs::write(&executable, b"owned fixture launcher")?;
        let pointer = Pointer {
            schema: 1,
            install: install.clone(),
            launcher_sha256: digest(&executable)?,
        };
        write_pointer(&directory, &pointer)?;
        let other = owner.path().canonicalize()?.join("other-install");
        crate::local_ipc::prepare_root(&other)?;
        retire_at(&directory, &other)?;
        assert!(executable.is_file());
        retire_at(&directory, &install)?;
        assert!(!executable.exists() && !directory.join(CONFIG).exists());
        assert!(!install.join("universal-uninstall.json").exists());
        retire_at(&directory, &install)?;
        // An interruption after deleting the binary retains proof for the pointer.
        write_pointer(&directory, &pointer)?;
        fs::write(
            install.join("universal-uninstall.json"),
            serde_json::to_vec(&pointer)?,
        )?;
        retire_at(&directory, &install)?;
        assert!(!directory.join(CONFIG).exists());
        // Changed binaries never inherit the original receipt's removal authority.
        write_pointer(&directory, &pointer)?;
        fs::write(&executable, b"changed fixture launcher")?;
        assert!(retire_at(&directory, &install).is_err());
        assert_eq!(fs::read(&executable)?, b"changed fixture launcher");
        Ok(())
    }

    fn write_pointer(directory: &Path, pointer: &Pointer) -> Result<()> {
        fs::write(directory.join(CONFIG), serde_json::to_vec(pointer)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.join(CONFIG), fs::Permissions::from_mode(0o600))?;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    #[test]
    fn the_pointer_is_read_on_every_call_and_must_match_the_binary() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let universal = directory.path().join("launcher");
        let (first, second) = (
            directory.path().join("service-1.4"),
            directory.path().join("service-next"),
        );
        for path in [&universal, &first, &second] {
            fs::create_dir_all(path)?;
        }
        let executable = universal.join(launcher_name());
        fs::write(&executable, b"universal launcher bytes")?;
        let sha = digest(&executable)?;
        write_pointer(
            &universal,
            &Pointer {
                schema: 1,
                install: first.clone(),
                launcher_sha256: sha.clone(),
            },
        )?;
        assert_eq!(resolve(&executable)?, first.canonicalize()?);
        assert!(is_universal(&executable));
        // A new installation only rewrites the pointer; the next call follows it.
        write_pointer(
            &universal,
            &Pointer {
                schema: 1,
                install: second.clone(),
                launcher_sha256: sha.clone(),
            },
        )?;
        assert_eq!(resolve(&executable)?, second.canonicalize()?);
        write_pointer(
            &universal,
            &Pointer {
                schema: 1,
                install: second.clone(),
                launcher_sha256: "0".repeat(64),
            },
        )?;
        assert!(
            resolve(&executable)
                .unwrap_err()
                .to_string()
                .contains("does not match")
        );
        write_pointer(
            &universal,
            &Pointer {
                schema: 1,
                install: universal.clone(),
                launcher_sha256: sha.clone(),
            },
        )?;
        assert!(
            resolve(&executable)
                .unwrap_err()
                .to_string()
                .contains("itself")
        );
        write_pointer(
            &universal,
            &Pointer {
                schema: 1,
                install: directory.path().join("missing"),
                launcher_sha256: sha.clone(),
            },
        )?;
        assert!(
            resolve(&executable)
                .unwrap_err()
                .to_string()
                .contains("reopen VS Code")
        );
        fs::write(
            universal.join(CONFIG),
            br#"{"schema":1,"install":"x","launcher_sha256":"y","extra":1}"#,
        )?;
        assert!(resolve(&executable).is_err(), "unknown fields are refused");
        fs::write(universal.join(CONFIG), vec![b' '; 5000])?;
        assert!(
            resolve(&executable).is_err(),
            "oversized pointers are refused"
        );
        Ok(())
    }

    #[test]
    fn publishing_copies_once_then_only_moves_the_pointer() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let universal = directory.path().join("launcher");
        let (old, new) = (
            directory.path().join("service-1.4"),
            directory.path().join("service-2.0"),
        );
        fs::create_dir_all(&old)?;
        fs::create_dir_all(&new)?;
        let (old_launcher, new_launcher) = (old.join(launcher_name()), new.join(launcher_name()));
        fs::write(&old_launcher, b"1.5 launcher")?;
        fs::write(&new_launcher, b"2.0 launcher")?;
        let executable = publish_from(&universal, &old.canonicalize()?, &old_launcher)?;
        assert_eq!(fs::read(&executable)?, b"1.5 launcher");
        assert_eq!(resolve(&executable)?, old.canonicalize()?);
        // A later installation repoints the pointer and keeps the published binary.
        publish_from(&universal, &new.canonicalize()?, &new_launcher)?;
        assert_eq!(
            fs::read(&executable)?,
            b"1.5 launcher",
            "the universal binary is never replaced"
        );
        assert_eq!(resolve(&executable)?, new.canonicalize()?);
        // A foreign binary at the destination is refused, and the pointer is left as it was.
        fs::write(&executable, b"someone else")?;
        let error = publish_from(&universal, &old.canonicalize()?, &old_launcher).unwrap_err();
        assert!(error.to_string().contains("refusing replacement"));
        let pointer: Pointer = serde_json::from_slice(&fs::read(universal.join(CONFIG))?)?;
        assert_eq!(pointer.install, new.canonicalize()?);
        Ok(())
    }

    #[test]
    fn a_binary_without_a_pointer_is_not_universal() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let executable = directory.path().join(launcher_name());
        fs::write(&executable, b"x")?;
        assert!(!is_universal(&executable));
        Ok(())
    }
}
