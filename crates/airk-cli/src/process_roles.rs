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
    if let Some(hash) = files.get(crate::runtime_install::main_name()).cloned() {
        for role in ROLES {
            files.insert(role.name().into(), hash.clone());
        }
    }
}

pub(crate) fn install(main: &Path) -> Result<()> {
    let root = crate::runtime_pair::payload_root(main)?;
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
        crate::runtime_pair::digest_file(executable)? == crate::runtime_pair::digest_file(&main)?,
        "process alias differs from its engine"
    );
    Ok(main)
}

pub(crate) fn executable(main: &Path, role: Role) -> Result<PathBuf> {
    let main = main_image(main)?;
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
    #[test]
    fn roles_share_bytes_resolve_metadata_and_refuse_changed_aliases() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("payload");
        let main = root.join(crate::runtime_install::main_name());
        fs::create_dir_all(main.parent().unwrap())?;
        fs::write(&main, b"approved engine fixture")?;
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
