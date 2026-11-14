// SPDX-License-Identifier: Apache-2.0
//! Original-operation recovery custody, using the shared Rust payload owner.
use super::*;
use airk_core::install_record::{
    Executable, Signing,
    inno_handoff::{HANDOFF_FILE, InnoHandoff, MAX_HANDOFF_BYTES, is_uninstaller_name},
    payload_inventory::{MAX_INVENTORY_BYTES, PayloadInventory},
    setup_recovery::InstallationImage,
};
use airk_process::directory::OpenedDirectory;
use std::{
    fs::{File, OpenOptions},
    io::Read,
};

pub(super) fn inventory(
    program: &Path,
    verified: &Verified,
) -> Result<(Vec<u8>, PayloadInventory), ErrorLine> {
    let path = program.join(INVENTORY_FILE);
    airk_ipc::private_record::require_regular_unlinked(&path)
        .map_err(|error| failed(format!("inventory: {error:#}")))?;
    let mut bytes = Vec::new();
    File::open(&path)
        .and_then(|file| {
            file.take(MAX_INVENTORY_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| failed(format!("inventory: {error}")))?;
    let inventory = PayloadInventory::decode(
        &bytes,
        Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
    )
    .map_err(|error| failed(format!("inventory: {error:#}")))?;
    let record = &verified.record;
    if inventory.version != record.version
        || inventory.channel != record.channel
        || inventory.target != record.target
        || inventory.admin_assets != record.admin_assets
        || inventory.signing.as_ref() != Some(&record.signing)
        || inventory.executables.airk != record.executables.airk.path
        || inventory.executables.airkctl != record.executables.airkctl.path
    {
        return Err(failed(
            "payload inventory differs from the verified I4 installation",
        ));
    }
    for image in [&record.executables.airk, &record.executables.airkctl] {
        let Some(file) = inventory.files.get(&image.path) else {
            return Err(failed("payload inventory omits the verified executable"));
        };
        if file.sha256 != image.sha256 || file.bytes != image.bytes {
            return Err(failed("payload inventory changes the verified executable"));
        }
    }
    Ok((bytes, inventory))
}

fn private_parent(root: &OpenedDirectory, relative: &str) -> Result<OpenedDirectory, ErrorLine> {
    let mut parent = root.clone();
    let parts = relative.split('/').collect::<Vec<_>>();
    for part in &parts[..parts.len() - 1] {
        parent = match parent.open_child(std::ffi::OsStr::new(part)) {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                airk_ipc::endpoint::create_root_at(&parent, std::ffi::OsStr::new(part))
                    .map_err(|error| failed(format!("recovery parent: {error:#}")))?
            }
            Err(error) => return Err(failed(format!("recovery parent: {error}"))),
        };
        airk_ipc::endpoint::require_private_directory_handle(parent.as_file())
            .map_err(|error| failed(format!("recovery parent: {error:#}")))?;
    }
    Ok(parent)
}

pub(super) fn retain(
    program: &Path,
    folder: &Path,
    verified: &Verified,
    bytes: &[u8],
    inventory: &PayloadInventory,
    signature: impl Fn(&Signing, &[&Path]) -> Result<(), (String, String)>,
) -> Result<PathBuf, ErrorLine> {
    let recovery = folder.join("recovery-program");
    let folder_pin = OpenedDirectory::open(folder)
        .map_err(|error| failed(format!("recovery custody: {error}")))?;
    let recovery_pin =
        airk_ipc::endpoint::create_root_at(&folder_pin, std::ffi::OsStr::new("recovery-program"))
            .map_err(|error| failed(format!("recovery runtime: {error:#}")))?;
    let source_root = OpenedDirectory::open(program)
        .map_err(|error| failed(format!("source program root: {error}")))?;
    let mut parents = vec![recovery_pin.clone()];
    for (relative, expected) in &inventory.files {
        let source = program.join(relative);
        airk_ipc::private_record::require_regular_unlinked(&source)
            .map_err(|error| failed(format!("{}: {error:#}", source.display())))?;
        let parent = private_parent(&recovery_pin, relative)?;
        let destination_leaf = std::ffi::OsStr::new(
            relative
                .rsplit('/')
                .next()
                .ok_or_else(|| failed("payload leaf missing"))?,
        );
        let mut output = airk_ipc::endpoint::create_private_file_at(&parent, destination_leaf)
            .map_err(|error| failed(format!("recovery file: {error:#}")))?;
        let mut source_parent = source_root.clone();
        let mut parts = relative.split('/').peekable();
        let mut leaf = None;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                leaf = Some(part);
                break;
            }
            source_parent = source_parent
                .open_child(std::ffi::OsStr::new(part))
                .map_err(|error| failed(format!("source parent: {error}")))?;
        }
        let mut input = source_parent
            .open_file(std::ffi::OsStr::new(
                leaf.ok_or_else(|| failed("empty source payload name"))?,
            ))
            .map_err(|error| failed(format!("source payload: {error}")))?;
        #[cfg(windows)]
        source_parent
            .require_default_stream(std::ffi::OsStr::new(leaf.unwrap()), &input)
            .map_err(|error| failed(format!("source payload streams: {error}")))?;
        if input
            .metadata()
            .map_err(|error| failed(error.to_string()))?
            .len()
            != expected.bytes
        {
            return Err(failed("source payload changed size"));
        }
        let copied = std::io::copy(
            &mut (&mut input).take(expected.bytes.saturating_add(1)),
            &mut output,
        )
        .map_err(|error| failed(format!("copy recovery payload: {error}")))?;
        output
            .set_permissions(
                input
                    .metadata()
                    .map_err(|error| failed(error.to_string()))?
                    .permissions(),
            )
            .map_err(|error| failed(error.to_string()))?;
        output
            .sync_all()
            .map_err(|error| failed(format!("sync recovery payload: {error}")))?;
        drop(output);
        let mut copied_file = parent
            .open_file(destination_leaf)
            .map_err(|error| failed(format!("recovery file: {error}")))?;
        #[cfg(windows)]
        parent
            .require_default_stream(destination_leaf, &copied_file)
            .map_err(|error| failed(format!("recovery stream: {error}")))?;
        let mut actual = Sha256::new();
        std::io::copy(&mut copied_file, &mut actual)
            .map_err(|error| failed(format!("recovery hash: {error}")))?;
        if copied != expected.bytes
            || Sha256Digest::from_bytes(actual.finalize().into()) != expected.sha256
        {
            return Err(failed(format!(
                "{relative} changed while retaining recovery payload"
            )));
        }
        parents.push(parent);
    }
    airk_ipc::private_record::write_private_at(
        &folder_pin,
        INVENTORY_FILE,
        bytes,
        MAX_INVENTORY_BYTES,
        false,
    )
    .map_err(|error| failed(format!("recovery inventory: {error:#}")))?;
    for parent in parents.iter().rev() {
        parent
            .sync_all()
            .map_err(|error| failed(format!("sync recovery directory: {error}")))?;
    }
    let airk = recovery.join(&inventory.executables.airk);
    let airkctl = recovery.join(&inventory.executables.airkctl);
    signature(
        &verified.record.signing,
        &[airkctl.as_path(), airk.as_path()],
    )
    .map_err(|(code, message)| failed(format!("{code}: {message}")))?;
    Ok(recovery)
}

pub(super) struct Inno {
    pub path: PathBuf,
    // Release before launching Inno: it needs exclusive read/write DAT access.
    _files: Vec<File>,
}

pub(super) fn inno(
    program: &Path,
    image: &InstallationImage,
    signature: impl Fn(&Signing, &[&Path]) -> Result<(), (String, String)>,
) -> Result<Inno, ErrorLine> {
    let held = OpenedDirectory::open(program).map_err(|error| failed(error.to_string()))?;
    airk_ipc::endpoint::require_private_directory_handle(held.as_file())
        .map_err(|error| failed(format!("{error:#}")))?;
    let bytes = crate::install::owned_bytes(&program.join(HANDOFF_FILE), MAX_HANDOFF_BYTES)
        .map_err(failed)?;
    let handoff = InnoHandoff::decode(&bytes, image).map_err(|error| failed(format!("uninstaller handoff: {}; repair with the current installer or use its original uninstaller", error.code())))?;
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(program).map_err(|error| failed(error.to_string()))? {
        let entry = entry.map_err(|error| failed(error.to_string()))?;
        if let Some(name) = entry
            .file_name()
            .to_str()
            .filter(|name| is_uninstaller_name(name))
        {
            candidates.push(name.to_owned());
        }
    }
    if candidates.len() != 1 || candidates[0] != handoff.uninstaller.path {
        return Err(failed(
            "uninstaller handoff is ambiguous or names another uninstaller",
        ));
    }
    let hold = |file: &Executable| -> Result<File, ErrorLine> {
        let path = program.join(&file.path);
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options
                .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
                .custom_flags(
                    windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
                );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let held = options
            .open(&path)
            .map_err(|error| failed(error.to_string()))?;
        airk_ipc::private_record::require_regular_unlinked(&path)
            .map_err(|error| failed(format!("{error:#}")))?;
        airk_ipc::endpoint::require_owner_private(&path)
            .map_err(|error| failed(format!("{error:#}")))?;
        if held
            .metadata()
            .map_err(|error| failed(error.to_string()))?
            .len()
            != file.bytes
            || file_digest(&path)? != file.sha256
        {
            return Err(failed(format!(
                "{} differs from the finalized handoff",
                file.path
            )));
        }
        Ok(held)
    };
    let files = vec![hold(&handoff.uninstaller)?, hold(&handoff.data)?];
    #[cfg(windows)]
    for (entry, file) in [
        (&handoff.uninstaller, &files[0]),
        (&handoff.data, &files[1]),
    ] {
        airk_process::directory::require_default_stream(&program.join(&entry.path), file)
            .map_err(|error| failed(format!("Inno handoff streams: {error}")))?;
    }
    let path = program.join(&handoff.uninstaller.path);
    signature(&image.record.signing, &[&path])
        .map_err(|(code, message)| failed(format!("{code}: {message}")))?;
    Ok(Inno {
        path,
        _files: files,
    })
}

#[cfg(test)]
mod tests;
