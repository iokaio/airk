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
const PUBLICATION_LOCK: &str = "airk-launcher-publish.lock";

/// All publishers/retirements share this directory-level lease even when their
/// installation roots differ. Keep the zero-byte lock inode after uninstall:
/// unlinking a locked file could admit a second owner through a new inode.
fn publication_lock(directory: &Path) -> Result<fs::File> {
    crate::local_ipc::prepare_root(directory)?;
    let path = directory.join(PUBLICATION_LOCK);
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            airk_host::install_files::safe_file(&path)?;
            ensure!(
                fs::metadata(&path)?.len() == 0,
                "unrecognized universal publication lock"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    airk_host::install_files::safe_file(&path)?;
    ensure!(
        file.metadata()?.len() == 0,
        "unrecognized universal publication lock"
    );
    file.try_lock().context(
        "universal launcher publication or retirement is in progress; retry after it finishes",
    )?;
    crate::local_ipc::require_existing_root(directory)?;
    airk_host::install_files::safe_file(&path)?;
    Ok(file)
}

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
    airk_host::runtime_pair::digest_file(path)
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

/// The install root selected by the universal launcher in `directory`, or
/// `None` when that directory has no launcher configuration. Supplied to user
/// path discovery through `runtime_install::INSTALLATION_PROBE`.
pub(crate) fn configured_selection(directory: &Path) -> Result<Option<PathBuf>> {
    if !directory.join(CONFIG).try_exists()? {
        return Ok(None);
    }
    resolve(&directory.join(launcher_name())).map(Some)
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
    let install = resolve(executable)?;
    if let Ok((_, main, _)) = crate::runtime_install::selected(&install) {
        airk_host::runtime_pair::require_legacy_stdio(&main)?;
    }
    airk_process::client_owner::watch()?;
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
        return airk_host::native_process::forward_stdio(command);
    }
    let mut command = Command::new(target);
    command.args(args);
    airk_host::native_process::forward_stdio(command)
}

/// Install or refresh the universal launcher in `directory` for `install`. The binary is
/// copied from the installation's verified launcher. Mac updates refresh a known
/// image because old bundle-bound signatures may not execute after copying.
/// Other platforms retain the image and change only the pointer. Unknown bytes
/// at the destination are never replaced.
pub fn publish_at(directory: &Path, install: &Path) -> Result<PathBuf> {
    let install = install.canonicalize()?;
    let (_, main, _) = crate::runtime_install::selected(&install)?;
    airk_host::runtime_pair::require_legacy_stdio(&main)?;
    let source = crate::runtime_install::verified_launcher(&install)?;
    publish_from(directory, &install, &source)
}

/// `publish_at` with the installation's verified launcher already resolved.
fn publish_from(directory: &Path, install: &Path, source: &Path) -> Result<PathBuf> {
    publish_from_checkpoint(directory, install, source, || Ok(()))
}

fn publish_from_checkpoint(
    directory: &Path,
    install: &Path,
    source: &Path,
    checkpoint: impl FnOnce() -> Result<()>,
) -> Result<PathBuf> {
    let _publication = publication_lock(directory)?;
    let install = install.to_path_buf();
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
    let source_sha256 = digest(source)?;
    let previous_sha256 = if executable.try_exists()? {
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
            known == Some(current.as_str()) || current == source_sha256,
            "unrecognized universal launcher; refusing replacement"
        );
        Some(current)
    } else {
        None
    };
    // Prior signed Mac copies may be bundle-bound and cannot run to delegate to
    // the repaired stable launcher. Refresh only a recognized Mac image; other
    // platforms retain the existing keep-once version-free entry point.
    let replace_known = cfg!(target_os = "macos")
        && previous_sha256
            .as_ref()
            .is_some_and(|hash| hash != &source_sha256);
    let launcher_sha256 = if previous_sha256.is_none() || replace_known {
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
            copied == source_sha256 && digest(source)? == source_sha256,
            "stable launcher changed while publishing the universal launcher"
        );
        if replace_known {
            let observed: Pointer = serde_json::from_slice(&read_bounded(&config)?)?;
            ensure!(
                Some(digest(&executable)?) == previous_sha256
                    && existing.as_ref() == Some(&observed),
                "universal launcher changed during refresh; preserving current files"
            );
            temporary.persist(&executable)?;
        } else {
            temporary.persist_noclobber(&executable)?;
        }
        copied
    } else {
        previous_sha256.context("universal launcher identity missing")?
    };
    #[cfg(unix)]
    fs::File::open(directory)?.sync_all()?;
    checkpoint()?;
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
        #[cfg(unix)]
        fs::File::open(directory)?.sync_all()?;
    }
    Ok(executable)
}

/// The universal launcher for the native default installation of `channel`, if `install`
/// is that installation. Custom and managed install roots get none.
pub fn publish_for_default(
    install: &Path,
    channel: airk_host::user_paths::Channel,
) -> Result<Option<PathBuf>> {
    let (_, main, _) = crate::runtime_install::selected(install)?;
    if airk_host::runtime_pair::single_executable(&main)? {
        return Ok(None);
    }
    let paths = crate::runtime_install::discover_user_paths(channel)?;
    if airk_host::user_paths::canonical_location(&paths.install)? != install.canonicalize()? {
        return Ok(None);
    }
    publish_at(&paths.launcher, install).map(Some)
}

/// The published universal launcher that currently forwards to `install`, if any.
pub fn current_for(
    install: &Path,
    channel: airk_host::user_paths::Channel,
) -> Result<Option<PathBuf>> {
    let paths = crate::runtime_install::discover_user_paths(channel)?;
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

/// Exact owned image for post-migration retirement, including interrupted
/// deletion. Unlike discovery, malformed/partial publications cannot mean absent.
pub(crate) fn retirement_image_for(
    install: &Path,
    channel: airk_host::user_paths::Channel,
) -> Result<Option<PathBuf>> {
    let paths = crate::runtime_install::discover_user_paths(channel)?;
    retirement_image_at(&paths.launcher, install)
}

pub(crate) fn retirement_digest_for(
    install: &Path,
    channel: airk_host::user_paths::Channel,
    image: &Path,
) -> Result<String> {
    ensure!(
        retirement_image_for(install, channel)?.as_deref() == Some(image),
        "universal retirement image ownership changed"
    );
    let hash = if image.is_file() {
        digest(image)?
    } else {
        let saved: Pointer =
            serde_json::from_slice(&read_bounded(&install.join("universal-uninstall.json"))?)?;
        saved.launcher_sha256
    };
    ensure!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "invalid universal retirement image hash"
    );
    Ok(hash)
}

fn retirement_image_at(directory: &Path, install: &Path) -> Result<Option<PathBuf>> {
    let executable = directory.join(launcher_name());
    let config = directory.join(CONFIG);
    let receipt = install.join("universal-uninstall.json");
    let exists = |path: &Path| -> Result<bool> {
        match fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    };
    if exists(&receipt)? {
        airk_host::install_files::safe_file(&receipt)?;
        let saved: Pointer = serde_json::from_slice(&read_bounded(&receipt)?)?;
        ensure!(
            saved.schema == 1 && saved.install == install.canonicalize()?,
            "universal retirement receipt changed"
        );
        if exists(&executable)? {
            airk_host::install_files::safe_file(&executable)?;
            ensure!(
                digest(&executable)? == saved.launcher_sha256,
                "changed universal launcher preserved"
            );
        }
        if exists(&config)? {
            airk_host::install_files::safe_file(&config)?;
            ensure!(
                serde_json::from_slice::<Pointer>(&read_bounded(&config)?)? == saved,
                "changed universal pointer preserved"
            );
        }
        return Ok(Some(executable));
    }
    let image = exists(&executable)?;
    let pointer = exists(&config)?;
    if !image && !pointer {
        return Ok(None);
    }
    ensure!(
        image && pointer,
        "partial universal launcher publication preserved; repair ownership before retirement"
    );
    airk_host::install_files::safe_file(&executable)?;
    airk_host::install_files::safe_file(&config)?;
    Ok((resolve(&executable)? == install.canonicalize()?).then_some(executable))
}

/// Remove only the verified universal entry point for this installation. A saved
/// receipt resumes interruption between deleting the executable and its pointer.
pub(crate) fn retire_for(install: &Path, channel: airk_host::user_paths::Channel) -> Result<()> {
    let paths = crate::runtime_install::discover_user_paths(channel)?;
    retire_at(&paths.launcher, install)
}

fn retire_at(directory: &Path, install: &Path) -> Result<()> {
    let receipt = install.join("universal-uninstall.json");
    let executable = directory.join(launcher_name());
    let config = directory.join(CONFIG);
    if !receipt.try_exists()? {
        let image_exists = executable.try_exists()?;
        let config_exists = config.try_exists()?;
        // A custom installation has no authority over an unrelated user's
        // universal entry point, including its directory or publication lock.
        // These read-only exits do not require creating or validating that lock.
        if !image_exists && !config_exists {
            return Ok(());
        }
        if image_exists && config_exists && resolve(&executable)? != install.canonicalize()? {
            return Ok(());
        }
    }
    // Incomplete publication still enters the shared lease. Once acquired, repeat
    // every ownership/identity check before recording or removing anything.
    let _publication = publication_lock(directory)?;
    let pointer: Pointer = if receipt.try_exists()? {
        airk_host::install_files::safe_file(&receipt)?;
        serde_json::from_slice(&read_bounded(&receipt)?)?
    } else {
        if !executable.try_exists()? || !config.try_exists()? {
            return Ok(());
        }
        if resolve(&executable)? != install.canonicalize()? {
            return Ok(());
        }
        airk_host::install_files::safe_file(&config)?;
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
        airk_host::install_files::safe_file(&config)?;
        ensure!(
            serde_json::from_slice::<Pointer>(&read_bounded(&config)?)? == pointer,
            "changed universal launcher configuration preserved during uninstall"
        );
    }
    if executable.try_exists()? {
        airk_host::install_files::safe_file(&executable)?;
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
    fn retirement_discovery_refuses_partial_or_changed_publication_and_preserves_foreign_owner()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let install = owner.path().canonicalize()?.join("install");
        let other = owner.path().canonicalize()?.join("other");
        let directory = owner.path().canonicalize()?.join("launcher");
        crate::local_ipc::prepare_root(&install)?;
        crate::local_ipc::prepare_root(&other)?;
        assert!(retirement_image_at(&directory, &install)?.is_none());
        crate::local_ipc::prepare_root(&directory)?;
        let image = directory.join(launcher_name());
        fs::write(&image, b"real owned fixture bytes")?;
        assert!(retirement_image_at(&directory, &install).is_err());
        let mut pointer = Pointer {
            schema: 1,
            install: other,
            launcher_sha256: digest(&image)?,
        };
        write_pointer(&directory, &pointer)?;
        assert!(retirement_image_at(&directory, &install)?.is_none());
        pointer.install = install.clone();
        write_pointer(&directory, &pointer)?;
        assert_eq!(
            retirement_image_at(&directory, &install)?,
            Some(image.clone())
        );
        fs::write(
            install.join("universal-uninstall.json"),
            serde_json::to_vec(&pointer)?,
        )?;
        fs::remove_file(&image)?;
        assert_eq!(
            retirement_image_at(&directory, &install)?,
            Some(image.clone())
        );
        fs::write(&image, b"foreign replacement")?;
        assert!(retirement_image_at(&directory, &install).is_err());
        assert_eq!(fs::read(image)?, b"foreign replacement");
        Ok(())
    }

    #[test]
    fn unrelated_uninstall_never_creates_or_claims_a_universal_publication_lock() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let install = owner.path().canonicalize()?.join("install");
        let other = owner.path().canonicalize()?.join("other-install");
        crate::local_ipc::prepare_root(&install)?;
        crate::local_ipc::prepare_root(&other)?;
        let directory = owner.path().join("launcher");
        retire_at(&directory, &install)?;
        assert!(!directory.exists());
        crate::local_ipc::prepare_root(&directory)?;
        let lock = directory.join(PUBLICATION_LOCK);
        fs::write(&lock, b"unrecognized existing lock")?;
        retire_at(&directory, &install)?;
        assert_eq!(fs::read(&lock)?, b"unrecognized existing lock");
        let executable = directory.join(launcher_name());
        fs::write(&executable, b"another installation's launcher")?;
        let pointer = Pointer {
            schema: 1,
            install: other,
            launcher_sha256: digest(&executable)?,
        };
        write_pointer(&directory, &pointer)?;
        let config = fs::read(directory.join(CONFIG))?;
        retire_at(&directory, &install)?;
        assert_eq!(fs::read(&executable)?, b"another installation's launcher");
        assert_eq!(fs::read(directory.join(CONFIG))?, config);
        assert_eq!(fs::read(&lock)?, b"unrecognized existing lock");
        assert!(!install.join("universal-uninstall.json").exists());
        // The same unknown lock must still block actual owned removal.
        assert!(retire_at(&directory, &pointer.install).is_err());
        assert_eq!(fs::read(&executable)?, b"another installation's launcher");
        assert_eq!(fs::read(directory.join(CONFIG))?, config);
        assert!(!pointer.install.join("universal-uninstall.json").exists());
        Ok(())
    }

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
    fn publishers_from_different_installations_share_one_lease_and_recover_owned_output()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let directory = owner.path().join("launcher");
        let first = owner.path().join("first-install");
        let second = owner.path().join("second-install");
        for root in [&first, &second] {
            fs::create_dir(root)?;
        }
        let first = first.canonicalize()?;
        let second = second.canonicalize()?;
        let a = first.join(launcher_name());
        let b = second.join(launcher_name());
        fs::write(&a, b"first approved launcher")?;
        fs::write(&b, b"second approved launcher")?;
        let (ready, entered) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let (worker_directory, worker_first, worker_a) =
            (directory.clone(), first.clone(), a.clone());
        let worker = std::thread::spawn(move || {
            publish_from_checkpoint(&worker_directory, &worker_first, &worker_a, || {
                ready.send(()).unwrap();
                released.recv().unwrap();
                anyhow::bail!("injected interruption after image publication")
            })
        });
        entered.recv_timeout(std::time::Duration::from_secs(10))?;
        let busy_publish = publish_from(&directory, &second, &b);
        let busy_retire = retire_at(&directory, &first);
        release.send(())?;
        let interrupted = worker
            .join()
            .map_err(|_| anyhow::anyhow!("publisher fixture panicked"))?;
        assert!(
            busy_publish
                .unwrap_err()
                .to_string()
                .contains("in progress")
        );
        assert!(busy_retire.unwrap_err().to_string().contains("in progress"));
        assert!(
            interrupted
                .unwrap_err()
                .to_string()
                .contains("injected interruption")
        );
        // A different source cannot claim the first image after its owner exits.
        assert!(publish_from(&directory, &second, &b).is_err());
        assert_eq!(
            fs::read(directory.join(launcher_name()))?,
            b"first approved launcher"
        );
        let image = publish_from(&directory, &first, &a)?;
        assert_eq!(resolve(&image)?, first);
        publish_from(&directory, &second, &b)?;
        assert_eq!(resolve(&image)?, second);
        let pointer: Pointer = serde_json::from_slice(&fs::read(directory.join(CONFIG))?)?;
        assert_eq!(pointer.launcher_sha256, digest(&image)?);
        retire_at(&directory, &second)?;
        assert!(
            directory.join(PUBLICATION_LOCK).exists(),
            "retain the shared lock inode across lifecycle transitions"
        );
        assert!(!image.exists() && !directory.join(CONFIG).exists());
        Ok(())
    }

    #[test]
    fn unrecognized_publication_lock_is_preserved_without_touching_launchers() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let directory = owner.path().join("launcher");
        crate::local_ipc::prepare_root(&directory)?;
        let source = owner.path().join("source");
        fs::write(&source, b"approved source")?;
        fs::write(
            directory.join(PUBLICATION_LOCK),
            b"unrecognized lock contents",
        )?;
        assert!(publish_from(&directory, owner.path(), &source).is_err());
        assert_eq!(
            fs::read(directory.join(PUBLICATION_LOCK))?,
            b"unrecognized lock contents"
        );
        assert!(!directory.join(launcher_name()).exists());
        assert!(!directory.join(CONFIG).exists());
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
    fn publishing_preserves_known_identity_and_refreshes_mac_copies_only() -> Result<()> {
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
        // Mac repairs refresh the known signed image; other hosts retain it.
        publish_from(&universal, &new.canonicalize()?, &new_launcher)?;
        assert_eq!(
            fs::read(&executable)?,
            if cfg!(target_os = "macos") {
                b"2.0 launcher"
            } else {
                b"1.5 launcher"
            },
            "only recognized Mac copies are refreshed"
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

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_pointer_mismatch_after_image_refresh_fails_closed_and_resumes_exact_source() -> Result<()>
    {
        let owner = tempfile::tempdir()?;
        let directory = owner.path().join("launcher");
        let install = owner.path().join("install");
        fs::create_dir(&install)?;
        let source = install.join(launcher_name());
        fs::write(&source, b"preceding signed fixture")?;
        let image = publish_from(&directory, &install.canonicalize()?, &source)?;
        fs::write(&source, b"new standalone signed fixture")?;
        // Format-level interruption fixture: the refreshed image is durable but
        // the old pointer has not yet been replaced. No invocation can use it.
        fs::write(&image, fs::read(&source)?)?;
        assert!(resolve(&image).is_err());
        publish_from(&directory, &install.canonicalize()?, &source)?;
        assert_eq!(resolve(&image)?, install.canonicalize()?);
        assert_eq!(fs::read(image)?, b"new standalone signed fixture");
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
