// SPDX-License-Identifier: Apache-2.0
//! Selected Windows CLI alias: private staged publication and exact-handle retirement.
use super::{LIMIT, RECEIPT, Receipt, expected, launcher, validate, validate_alias};
use airk_core::identity::Incarnation;
use airk_ipc::{endpoint, private_record};
use airk_process::directory::OpenedDirectory;
use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsStr,
    io::{self, Read, Write},
    path::Path,
};

fn directory(program: &OpenedDirectory, target: &Path) -> Result<Option<OpenedDirectory>> {
    endpoint::require_protected_parent_handle(program.as_file())?;
    ensure!(
        target.is_absolute() && target.file_name() == Some(OsStr::new("airkctl.exe")),
        "CLI alias target must be the canonical airkctl.exe"
    );
    let bin_path = target.parent().context("CLI target has no parent")?;
    ensure!(
        bin_path.file_name() == Some(OsStr::new("bin")),
        "CLI target must use the installed bin layout"
    );
    let program_path = bin_path
        .parent()
        .context("CLI target program root missing")?;
    // A textual path is not authority: re-open the admitted layout root and
    // compare its native identity before any destination observation or effect.
    let named = OpenedDirectory::open(program_path)?;
    ensure!(
        named.identity()? == program.identity()?,
        "CLI target belongs to another program root"
    );
    let bin = match program.open_child(OsStr::new("bin")) {
        Ok(bin) => bin,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    endpoint::require_protected_parent_handle(bin.as_file())?;
    Ok(Some(bin))
}
fn read(root: &OpenedDirectory, name: &str) -> Result<Option<Vec<u8>>> {
    let file = match root.open_file(OsStr::new(name)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    endpoint::require_private_metadata_handle(&file)?;
    root.require_default_stream(OsStr::new(name), &file)?;
    let mut bytes = Vec::new();
    file.take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= LIMIT, "CLI alias file exceeds bound");
    Ok(Some(bytes))
}

// Callers hold the identity-checked bin and its Windows ancestor pins.
fn exists(bin_path: &Path, name: &str) -> Result<bool> {
    match std::fs::symlink_metadata(bin_path.join(name)) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn stage_name(install_id: Incarnation, alias: &str) -> String {
    format!(".airk-cli-alias-{install_id}-{alias}.part")
}

/// Destination-only observation. Shared selection additionally checks PATH/PATHEXT.
pub(in crate::setup_role) fn available(
    program: &OpenedDirectory,
    home: &Path,
    target: &Path,
    install_id: Incarnation,
    alias: &str,
) -> Result<bool> {
    validate_alias(alias)?;
    let bin = directory(program, target)?;
    let wanted = expected(home, target, install_id, alias)?;
    let owned = if let Some(bytes) = read(program, RECEIPT)? {
        let prior: Receipt = serde_json::from_slice(&bytes)?;
        validate_alias(&prior.alias)?;
        validate(&bytes, &expected(home, target, install_id, &prior.alias)?)?;
        prior == wanted
    } else {
        false
    };
    let Some(bin) = bin else { return Ok(true) };
    let body = launcher(target)?;
    for name in [format!("{alias}.cmd"), stage_name(install_id, alias)] {
        if exists(target.parent().context("CLI parent missing")?, &name)? {
            if !owned {
                return Ok(false);
            }
            match read(&bin, &name) {
                Ok(Some(bytes)) if name == format!("{alias}.cmd") || bytes == body => {}
                _ => return Ok(false),
            }
        }
    }
    Ok(true)
}

pub(in crate::setup_role) fn install(
    program: &OpenedDirectory,
    home: &Path,
    target: &Path,
    install_id: Incarnation,
    alias: &str,
    checkpoint: impl Fn(&str) -> Result<()>,
) -> Result<String> {
    validate_alias(alias)?;
    let bin = directory(program, target)?.context("CLI bin has not been placed")?;
    let wanted = expected(home, target, install_id, alias)?;
    let body = launcher(target)?;
    let name = format!("{alias}.cmd");
    let stage = stage_name(install_id, alias);
    let prior = read(program, RECEIPT)?;
    if let Some(bytes) = &prior {
        validate(bytes, &wanted)?;
    }
    let previous = read(&bin, &name)?;
    if let Some(bytes) = &previous {
        ensure!(
            prior.is_some(),
            "preexisting CLI alias is unowned and preserved"
        );
        if bytes == &body {
            return Ok(format!("owned {alias} launcher unchanged"));
        }
        // Explicit user policy: a valid private same-installation receipt authorizes
        // same-alias repair. Uninstall remains exact-original-byte retirement.
    }
    if prior.is_none() {
        ensure!(
            !exists(target.parent().context("CLI parent missing")?, &stage)?,
            "unowned CLI staging entry preserved"
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
    let resumed = if prior.is_some() {
        read(&bin, &stage)?
    } else {
        None
    };
    if let Some(bytes) = resumed {
        ensure!(bytes == body, "modified CLI staging file preserved");
    } else {
        // Even after the receipt, the fresh branch never adopts a raced staging entry.
        let mut file = endpoint::create_private_file_at(&bin, OsStr::new(&stage))?;
        file.write_all(&body)?;
        file.sync_all()?;
        endpoint::require_private_metadata_handle(&file)?;
        bin.require_default_stream(OsStr::new(&stage), &file)?;
    }
    checkpoint("staged")?;
    ensure!(
        read(&bin, &stage)? == Some(body.clone()),
        "CLI staging changed before publication"
    );
    ensure!(
        read(&bin, &name)? == previous,
        "CLI destination changed before publication"
    );
    bin.publish_file(OsStr::new(&stage), OsStr::new(&name), previous.is_some())?;
    bin.sync_all()?;
    checkpoint("published")?;
    ensure!(
        read(&bin, &name)? == Some(body),
        "CLI alias publication changed"
    );
    Ok(format!("owned {alias} launcher installed"))
}

pub(super) fn remove_exact(root: &OpenedDirectory, name: &str, expected: &[u8]) -> Result<()> {
    let removed = root.remove_file_if(OsStr::new(name), |file, proof| {
        endpoint::require_private_pending_metadata_handle(file, proof)
            .map_err(|error| io::Error::other(format!("{error:#}")))?;
        root.require_default_stream(OsStr::new(name), file)?;
        let mut bytes = Vec::new();
        file.take(expected.len() as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes != expected {
            return Err(io::Error::other("modified CLI alias retained"));
        }
        Ok(true)
    });
    match removed {
        Ok(true) => Ok(()),
        Ok(false) => anyhow::bail!("CLI alias removal refused"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(in crate::setup_role) fn remove(
    program: &OpenedDirectory,
    home: &Path,
    target: &Path,
    install_id: Incarnation,
) -> Result<String> {
    endpoint::require_protected_parent_handle(program.as_file())?;
    let Some(bytes) = read(program, RECEIPT)? else {
        return Ok("no CLI alias owned".into());
    };
    let receipt: Receipt = serde_json::from_slice(&bytes)?;
    validate_alias(&receipt.alias)?;
    validate(&bytes, &expected(home, target, install_id, &receipt.alias)?)?;
    let bin = directory(program, target)?.context("Owned CLI bin is missing")?;
    let body = launcher(target)?;
    remove_exact(&bin, &format!("{}.cmd", receipt.alias), &body)?;
    remove_exact(&bin, &stage_name(install_id, &receipt.alias), &body)?;
    remove_exact(program, RECEIPT, &bytes)?;
    Ok(format!("owned {} launcher removed", receipt.alias))
}

#[cfg(test)]
mod tests;
