// SPDX-License-Identifier: Apache-2.0
//! Native-owned CLI alias receipts, selection and platform adapters.
use super::files;
use airk_core::identity::{Incarnation, Sha256Digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod intent;
mod note;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;
pub(super) use intent::extensions as executable_extensions;
pub(super) use intent::{AliasContext, bind, load_intent, probe, select};
pub(super) use note::{remove_note, write_note};
#[cfg(windows)]
pub(super) use windows::{available, install, remove};

pub(super) const RECEIPT: &str = "cli-alias-owner.json";
const LIMIT: usize = 32768;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    schema: String,
    install_id: Incarnation,
    alias: String,
    launcher_path: PathBuf,
    target: PathBuf,
    launcher_sha256: Sha256Digest,
}

pub(super) fn validate_alias(alias: &str) -> Result<()> {
    let bytes = alias.as_bytes();
    ensure!(
        !bytes.is_empty()
            && bytes.len() <= 32
            && bytes[0].is_ascii_alphabetic()
            && bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')),
        "CLI alias must be 1..32 ASCII letters/digits/_/-, starting with a letter"
    );
    let lower = alias.to_ascii_lowercase();
    ensure!(
        !matches!(
            lower.as_str(),
            "airk" | "airkctl" | "con" | "prn" | "aux" | "nul"
        ) && !(lower.len() == 4
            && (lower.starts_with("com") || lower.starts_with("lpt"))
            && lower.as_bytes()[3].is_ascii_digit()),
        "CLI alias is reserved"
    );
    Ok(())
}

#[cfg(any(unix, test))]
pub(super) fn posix_launcher(target: &Path) -> Result<Vec<u8>> {
    ensure!(target.is_absolute(), "CLI target must be absolute");
    let target = target.to_str().context("CLI target must be UTF-8")?;
    ensure!(!target.contains('\0'), "CLI target contains NUL");
    let quoted = target.replace('\'', "'\\''");
    let bytes =
        format!("#!/bin/sh\n# SPDX-License-Identifier: Apache-2.0\nexec '{quoted}' \"$@\"\n")
            .into_bytes();
    ensure!(bytes.len() <= LIMIT, "CLI launcher exceeds bound");
    Ok(bytes)
}

pub(super) fn launcher(target: &Path) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        ensure!(
            target.is_absolute() && target.file_name() == Some(std::ffi::OsStr::new("airkctl.exe")),
            "CLI target must be canonical airkctl.exe"
        );
        Ok(b"@echo off\r\nsetlocal EnableExtensions DisableDelayedExpansion\r\nset \"ERRORLEVEL=\"\r\n\"%~dp0airkctl.exe\" %*\r\nexit /b %errorlevel%\r\n".to_vec())
    }
    #[cfg(not(windows))]
    {
        posix_launcher(target)
    }
}

pub(super) fn expected(
    home: &Path,
    target: &Path,
    install_id: Incarnation,
    alias: &str,
) -> Result<Receipt> {
    validate_alias(alias)?;
    ensure!(home.is_absolute(), "CLI home must be absolute");
    #[cfg(windows)]
    let launcher_path = target
        .parent()
        .context("CLI target parent missing")?
        .join(format!("{alias}.cmd"));
    #[cfg(not(windows))]
    let launcher_path = home.join(".local/bin").join(alias);
    Ok(Receipt {
        schema: "airk-cli-alias-owner/1".into(),
        install_id,
        alias: alias.into(),
        launcher_path,
        target: target.to_owned(),
        launcher_sha256: files::digest_bytes(&launcher(target)?),
    })
}

pub(super) fn validate(bytes: &[u8], expected: &Receipt) -> Result<()> {
    ensure!(bytes.len() <= LIMIT, "CLI receipt exceeds bound");
    let actual: Receipt = serde_json::from_slice(bytes)?;
    ensure!(&actual == expected, "CLI ownership receipt changed");
    Ok(())
}
#[cfg(unix)]
mod unix {
    use super::*;
    use airk_ipc::{endpoint, private_record};
    use airk_process::directory::OpenedDirectory;
    use std::{
        ffi::OsStr,
        fs::File,
        io::{Read, Write},
        os::unix::fs::{MetadataExt, PermissionsExt},
    };

    fn parent(root: &OpenedDirectory) -> Result<()> {
        endpoint::require_protected_parent_handle(root.as_file())?;
        // SAFETY: scalar effective identity query.
        ensure!(
            root.as_file().metadata()?.uid() == unsafe { libc::geteuid() },
            "rk parent is not owned by this user"
        );
        Ok(())
    }

    fn directory(home: &Path, create: bool) -> Result<Option<OpenedDirectory>> {
        let mut root = OpenedDirectory::open(home)?;
        parent(&root)?;
        for name in [".local", "bin"] {
            root = match root.open_child(OsStr::new(name)) {
                Ok(child) => child,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                    endpoint::create_root_at(&root, OsStr::new(name))?
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            parent(&root)?;
        }
        Ok(Some(root))
    }

    fn read(root: &OpenedDirectory, name: &str, receipt: bool) -> Result<Option<Vec<u8>>> {
        let file = match root.open_file(OsStr::new(name)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if receipt {
            endpoint::require_private_metadata_handle(&file)?;
        } else {
            wrapper_file(&file)?;
        }
        let mut bytes = Vec::new();
        file.take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= LIMIT, "rk file exceeds bound");
        Ok(Some(bytes))
    }

    fn wrapper_file(file: &File) -> Result<()> {
        let metadata = file.metadata()?;
        // SAFETY: scalar effective identity query.
        ensure!(
            metadata.is_file()
                && metadata.uid() == unsafe { libc::geteuid() }
                && metadata.nlink() == 1
                && metadata.mode() & 0o7777 == 0o700,
            "rk is not the original owner-executable regular file"
        );
        Ok(())
    }

    pub(crate) fn available(
        program: &OpenedDirectory,
        home: &Path,
        target: &Path,
        install_id: Incarnation,
        alias: &str,
    ) -> Result<bool> {
        parent(program)?;
        let Some(bin) = directory(home, false)? else {
            return Ok(true);
        };
        let wanted = expected(home, target, install_id, alias)?;
        let receipt = read(program, RECEIPT, true)?;
        let owned = if let Some(bytes) = &receipt {
            let prior: Receipt = serde_json::from_slice(bytes)?;
            validate(bytes, &expected(home, target, install_id, &prior.alias)?)?;
            prior == wanted
        } else {
            false
        };
        for name in [
            alias.to_owned(),
            format!(".airk-cli-alias-{install_id}-{alias}.part"),
        ] {
            match std::fs::symlink_metadata(home.join(".local/bin").join(&name)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
                Ok(_) if !owned => return Ok(false),
                Ok(_) => {}
            }
            match read(&bin, &name, false) {
                Ok(Some(_)) if name == alias => {}
                Ok(Some(bytes)) if bytes == launcher(target)? => {}
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    // The callback is a failure-injection seam; production supplies a no-op.
    pub(crate) fn install(
        program: &OpenedDirectory,
        home: &Path,
        target: &Path,
        install_id: Incarnation,
        alias: &str,
        checkpoint: impl Fn(&str) -> Result<()>,
    ) -> Result<String> {
        parent(program)?;
        let bin = directory(home, true)?.context("rk directory missing")?;
        let wanted = expected(home, target, install_id, alias)?;
        let body = launcher(target)?;
        let old_receipt = read(program, RECEIPT, true)?;
        if let Some(bytes) = &old_receipt {
            validate(bytes, &wanted)?;
        }
        // Any preexisting rk without our receipt is unowned, even identical bytes.
        let current = read(&bin, alias, false)?;
        let replace = current.is_some();
        if let Some(bytes) = &current {
            ensure!(
                old_receipt.is_some(),
                "preexisting CLI alias is unowned and was preserved"
            );
            if bytes == &body {
                return Ok("owned CLI alias launcher unchanged".into());
            }
        }
        let stage = format!(".airk-cli-alias-{install_id}-{alias}.part");
        if old_receipt.is_none() {
            ensure!(
                read(&bin, &stage, false)?.is_none(),
                "preexisting rk staging file is unowned and was preserved"
            );
            private_record::write_owner_at(
                program,
                RECEIPT,
                &serde_json::to_vec(&wanted)?,
                LIMIT,
                false,
            )?;
        }
        checkpoint("receipt")?;
        let previous_stage = if old_receipt.is_some() {
            read(&bin, &stage, false)?
        } else {
            None
        };
        match previous_stage {
            Some(bytes) => ensure!(bytes == body, "changed rk staging file preserved"),
            None => {
                let mut file = bin.create_file(OsStr::new(&stage))?;
                endpoint::require_private_metadata_handle(&file)?;
                file.write_all(&body)?;
                file.set_permissions(std::fs::Permissions::from_mode(0o700))?;
                file.sync_all()?;
                wrapper_file(&file)?;
            }
        }
        checkpoint("staged")?;
        ensure!(
            read(&bin, alias, false)? == current,
            "CLI alias changed during staging"
        );
        ensure!(
            read(&bin, &stage, false)?.as_deref() == Some(body.as_slice()),
            "CLI alias staged bytes changed before publication"
        );
        bin.publish_file(OsStr::new(&stage), OsStr::new(alias), replace)?;
        bin.sync_all()?;
        checkpoint("published")?;
        ensure!(
            read(&bin, alias, false)? == Some(body),
            "rk publication changed"
        );
        Ok("owned rk launcher installed".into())
    }

    pub(crate) fn remove(
        program: &OpenedDirectory,
        home: &Path,
        target: &Path,
        install_id: Incarnation,
    ) -> Result<String> {
        parent(program)?;
        let Some(receipt) = read(program, RECEIPT, true)? else {
            return Ok("no rk launcher was owned".into());
        };
        let saved: Receipt = serde_json::from_slice(&receipt)?;
        let alias = saved.alias.as_str();
        validate(&receipt, &expected(home, target, install_id, alias)?)?;
        if let Some(bin) = directory(home, false)? {
            let body = launcher(target)?;
            if let Some(bytes) = read(&bin, alias, false)? {
                ensure!(bytes == body, "modified rk was preserved");
                // Held no-follow parent + setup lock, same Unix compare/unlink
                // limitation as the existing private-record owner.
                bin.unlink_file(OsStr::new(alias))?;
                bin.sync_all()?;
            }
            let stage = format!(".airk-cli-alias-{install_id}-{alias}.part");
            if let Some(bytes) = read(&bin, &stage, false)? {
                ensure!(bytes == body, "changed rk staging file preserved");
                bin.unlink_file(OsStr::new(&stage))?;
                bin.sync_all()?;
            }
        }
        private_record::remove_private_at(program, RECEIPT, &receipt)?;
        Ok("owned rk launcher removed".into())
    }
}

#[cfg(unix)]
pub(super) use unix::{available, install, remove};

pub(super) fn previous_alias(
    program: &airk_process::directory::OpenedDirectory,
    home: &Path,
    target: &Path,
    id: Incarnation,
) -> Result<Option<String>> {
    let Some(raw) = intent::read_owner(program, RECEIPT)? else {
        return Ok(None);
    };
    let old: Receipt = serde_json::from_slice(&raw)?;
    validate(&raw, &expected(home, target, id, &old.alias)?)?;
    Ok(Some(old.alias))
}
