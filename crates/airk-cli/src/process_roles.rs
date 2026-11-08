// SPDX-License-Identifier: Apache-2.0
//! Install-local executable names. Distribution bytes and signed app bundles stay unchanged.
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy)]
pub(crate) enum Role {
    Service,
    Index,
    Source,
}
impl Role {
    pub(crate) fn name(self) -> &'static str {
        match (self, cfg!(windows)) {
            (Self::Service, true) => "airk-service.exe",
            (Self::Index, true) => "airk-index.exe",
            (Self::Source, true) => "airk-source.exe",
            (Self::Service, false) => "airk-service",
            (Self::Index, false) => "airk-index",
            (Self::Source, false) => "airk-source",
        }
    }
}
const ROLES: [Role; 3] = [Role::Service, Role::Index, Role::Source];

/// Only these fixed aliases, with exactly the approved main image's digest, are owned.
pub(crate) fn inventory(files: &mut BTreeMap<String, String>) {
    if !files.contains_key(if cfg!(windows) {
        "airk-embed.exe"
    } else if cfg!(target_os = "macos") {
        "AIRK.app/Contents/MacOS/airk-embed"
    } else {
        "airk-embed"
    }) {
        return; // schema-2 main owns every native role; never invent alias files.
    }
    if cfg!(target_os = "macos") && files.contains_key(crate::runtime_install::MAC_LAUNCHER_IMAGE) {
        return; // The sealed Mac main must run in its bundle, not as role copies.
    }
    if let Some(hash) = files.get(crate::runtime_install::main_name()).cloned() {
        for role in ROLES {
            files.insert(role.name().into(), hash.clone());
        }
    }
}

pub(crate) fn install(main: &Path) -> Result<()> {
    let root = crate::runtime_pair::payload_root(main)?;
    if crate::runtime_pair::single_executable(main)? {
        return require_no_bundle_aliases(root);
    }
    if standalone_mac_layout(main)? {
        return require_no_bundle_aliases(root);
    }
    let hash = crate::runtime_pair::digest_file(main)?;
    for role in ROLES {
        let destination = root.join(role.name());
        if fs::symlink_metadata(&destination).is_ok() {
            ensure!(
                crate::runtime_pair::digest_file(&destination)? == hash,
                "modified process alias preserved: {}",
                role.name()
            );
            continue;
        }
        // Runtime ownership forbids hard links. Copy locally, never package duplicate engines.
        // Persist without clobbering: interruption leaves no executable or a complete image.
        let mut temporary = tempfile::NamedTempFile::new_in(
            root.parent()
                .and_then(Path::parent)
                .context("installation parent missing")?,
        )?;
        std::io::copy(&mut fs::File::open(main)?, &mut temporary)?;
        temporary
            .as_file()
            .set_permissions(fs::metadata(main)?.permissions())?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&destination)?;
        #[cfg(unix)]
        fs::File::open(root)?.sync_all()?;
        ensure!(
            crate::runtime_pair::digest_file(&destination)? == hash,
            "process alias verification failed"
        );
    }
    Ok(())
}

fn standalone_mac_layout(main: &Path) -> Result<bool> {
    if !cfg!(target_os = "macos") {
        return Ok(false);
    }
    let image =
        crate::runtime_pair::payload_root(main)?.join(crate::runtime_install::MAC_LAUNCHER_IMAGE);
    match fs::symlink_metadata(&image) {
        Ok(_) => {
            crate::runtime_install::safe_file(&image)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn require_no_bundle_aliases(root: &Path) -> Result<()> {
    for role in ROLES {
        match fs::symlink_metadata(root.join(role.name())) {
            Ok(_) => anyhow::bail!(
                "unexpected process alias beside sealed Mac payload; preserve and inspect it"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Resolve helper/build metadata through the original image, including macOS's sealed app.
pub(crate) fn main_image(executable: &Path) -> Result<PathBuf> {
    if !ROLES
        .iter()
        .any(|role| executable.file_name() == Some(role.name().as_ref()))
    {
        return Ok(executable.into());
    }
    let main = executable
        .parent()
        .context("process alias parent missing")?
        .join(crate::runtime_install::main_name());
    ensure!(
        !crate::runtime_pair::single_executable(&main)?,
        "single executable runtime does not own copied process aliases"
    );
    ensure!(
        !standalone_mac_layout(&main)?,
        "process alias is not supported for a sealed Mac payload"
    );
    ensure!(
        crate::runtime_pair::digest_file(executable)? == crate::runtime_pair::digest_file(&main)?,
        "process alias differs from its engine"
    );
    Ok(main)
}

pub(crate) fn executable(main: &Path, role: Role) -> Result<PathBuf> {
    let main = main_image(main)?;
    if crate::runtime_pair::single_executable(&main)? {
        require_no_bundle_aliases(crate::runtime_pair::payload_root(&main)?)?;
        return Ok(main);
    }
    if standalone_mac_layout(&main)? {
        require_no_bundle_aliases(crate::runtime_pair::payload_root(&main)?)?;
        return Ok(main);
    }
    let path = crate::runtime_pair::payload_root(&main)?.join(role.name());
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            ensure!(
                crate::runtime_pair::digest_file(&path)?
                    == crate::runtime_pair::digest_file(&main)?,
                "process alias differs from approved engine"
            );
            Ok(path)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(main), // development/managed read-only payload
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn legacy_receipt(main: &Path) -> Result<()> {
        let pair = crate::runtime_pair::RuntimePair {
            schema: 1,
            version: env!("CARGO_PKG_VERSION").into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            airk_sha256: crate::runtime_pair::digest_file(main)?,
            embed_sha256: "b".repeat(64),
        };
        fs::write(
            crate::runtime_pair::payload_root(main)?.join("airk-runtime.json"),
            serde_json::to_vec(&pair)?,
        )?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn sealed_mac_payload_runs_in_bundle_without_copying_process_roles() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("payload");
        let main = root.join(crate::runtime_install::main_name());
        fs::create_dir_all(main.parent().unwrap())?;
        fs::write(&main, b"sealed main fixture")?;
        legacy_receipt(&main)?;
        fs::write(
            root.join(crate::runtime_install::MAC_LAUNCHER_IMAGE),
            b"standalone fixture",
        )?;
        install(&main)?;
        for role in ROLES {
            assert_eq!(executable(&main, role)?, main);
            assert!(!root.join(role.name()).exists());
        }
        let alias = root.join(Role::Index.name());
        fs::write(&alias, b"unrecognized alias preserved")?;
        assert!(install(&main).is_err());
        assert!(executable(&main, Role::Index).is_err());
        assert!(main_image(&alias).is_err());
        assert_eq!(fs::read(alias)?, b"unrecognized alias preserved");
        Ok(())
    }
    #[test]
    fn roles_share_bytes_resolve_metadata_and_refuse_changed_aliases() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("payload");
        let main = root.join(crate::runtime_install::main_name());
        fs::create_dir_all(main.parent().unwrap())?;
        fs::write(&main, b"approved engine fixture")?;
        legacy_receipt(&main)?;
        install(&main)?;
        install(&main)?;
        for role in ROLES {
            let alias = executable(&main, role)?;
            assert_ne!(alias, main);
            assert_eq!(main_image(&alias)?, main);
        }
        let alias = root.join(Role::Index.name());
        fs::remove_file(&alias)?;
        fs::write(&alias, b"foreign image")?;
        assert!(install(&main).is_err());
        assert!(executable(&main, Role::Index).is_err());
        assert!(main_image(&alias).is_err());
        assert_eq!(fs::read(main)?, b"approved engine fixture");
        Ok(())
    }
}
