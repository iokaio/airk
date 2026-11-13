// SPDX-License-Identifier: Apache-2.0
//! Setup's file adapters (`docs/split/contracts.md` §5): the operation file,
//! the payload inventory, the setup receipt, the install pointer and record,
//! and the installation lock. Every read is bounded and refuses links,
//! reparse points and hard-link aliases; owner-private files are checked with
//! the shared `airk-ipc` checks. Every write goes through a temporary file
//! and a rename; the pointer and receipt folders are owner-only.
use super::plan::{self, INVENTORY_FILE};
use airk_core::{
    identity::Sha256Digest,
    install_record::{
        AdminAssets, InstallPointer, InstallRecord, MAX_POINTER_BYTES, MAX_RECORD_BYTES,
        MAX_REPLACEMENT_NOTICE_BYTES, MAX_SETUP_OPERATION_BYTES, MAX_SETUP_RECEIPT_BYTES,
        RECORD_FILE, REPLACEMENT_NOTICE_FILE, ReplacementNotice, SetupReceipt, Signing, canonical,
    },
};
use airk_ipc::private_record::{self, PrivateLock};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// `airk-service-payload/1`: the complete placed payload, written by the
/// packaging script beside the platform installer's files. Its SHA-256 is
/// pinned by the owner-private operation (`payload_inventory_sha256`).
pub(crate) const INVENTORY_SCHEMA: &str = "airk-service-payload/1";
const MAX_INVENTORY_BYTES: usize = 256 * 1024;
const MAX_INVENTORY_FILES: usize = 4096;
const MAX_LISTED_ENTRIES: usize = 8192;
const POINTER_FILE: &str = "current.json";
const LOCK_FILE: &str = "install.lock";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InventoryFile {
    pub sha256: Sha256Digest,
    pub bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InventoryExecutables {
    pub airkctl: String,
    pub airk: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PayloadInventory {
    pub schema: String,
    pub version: String,
    pub channel: String,
    pub target: String,
    pub executables: InventoryExecutables,
    /// Program-relative `/`-separated path → digest and size.
    pub files: BTreeMap<String, InventoryFile>,
    pub admin_assets: AdminAssets,
    /// `None` for a payload whose images were not signed.
    pub signing: Option<Signing>,
}

/// The current installation as the pointer names it.
pub(crate) struct Installed {
    pub pointer: InstallPointer,
    pub record: InstallRecord,
}

pub(crate) fn digest_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

/// SHA-256 and size of a regular, unlinked file.
pub(crate) fn digest_file(path: &Path) -> Result<(Sha256Digest, u64)> {
    private_record::require_regular_unlinked(path)
        .with_context(|| format!("{} is not a regular file", path.display()))?;
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let bytes = std::io::copy(&mut file, &mut hash)?;
    Ok((Sha256Digest::from_bytes(hash.finalize().into()), bytes))
}

/// A regular, unlinked file read within `limit` bytes; `owner_private`
/// additionally requires the owner-private descriptor or mode.
pub(crate) fn read_bounded(path: &Path, limit: usize, owner_private: bool) -> Result<Vec<u8>> {
    private_record::require_regular_unlinked(path)
        .with_context(|| format!("{} is not a regular file", path.display()))?;
    if owner_private {
        airk_ipc::endpoint::require_owner_private(path)
            .with_context(|| format!("{} is not owner-private", path.display()))?;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "{} exceeds {limit} bytes",
        path.display()
    );
    Ok(bytes)
}

/// The owner-private operation bytes (§5.5); decoded by the caller with its clock.
pub(crate) fn read_operation(path: &Path) -> Result<Vec<u8>> {
    if let Some(folder) = path.parent() {
        no_links(folder)?;
    }
    read_bounded(path, MAX_SETUP_OPERATION_BYTES, true)
        .context("setup_operation_refused: the operation file is not an owner-private regular file")
}

/// Read and validate the payload inventory whose digest the operation pins.
pub(crate) fn read_inventory(path: &Path, expected: Sha256Digest) -> Result<PayloadInventory> {
    let bytes = read_bounded(path, MAX_INVENTORY_BYTES, false)?;
    ensure!(
        digest_bytes(&bytes) == expected,
        "payload_inventory_mismatch: {} differs from the operation's payload inventory",
        path.display()
    );
    let inventory: PayloadInventory =
        serde_json::from_slice(&bytes).context("payload_inventory_malformed")?;
    ensure!(
        inventory.schema == INVENTORY_SCHEMA,
        "payload_inventory_malformed: unknown schema"
    );
    ensure!(
        inventory.files.len() <= MAX_INVENTORY_FILES,
        "payload_inventory_malformed: too many files"
    );
    for name in inventory.files.keys() {
        ensure!(
            plan::relative_inside(name) && name != INVENTORY_FILE && name != RECORD_FILE,
            "payload_inventory_malformed: invalid path {name:?}"
        );
    }
    for executable in [&inventory.executables.airk, &inventory.executables.airkctl] {
        ensure!(
            inventory.files.contains_key(executable),
            "payload_inventory_malformed: executable {executable:?} is not listed"
        );
    }
    Ok(inventory)
}

/// `<state>/setup`: the owner-only receipt folder.
fn receipt_folder(state: &Path) -> PathBuf {
    state.join("setup")
}

fn receipt_name(receipt_operation: &impl std::fmt::Display) -> String {
    format!("{receipt_operation}.json")
}

/// The receipt of `operation_id`, when one was written.
pub(crate) fn read_receipt(
    state: &Path,
    operation_id: &impl std::fmt::Display,
) -> Result<Option<SetupReceipt>> {
    let folder = receipt_folder(state);
    if std::fs::symlink_metadata(&folder).is_err() {
        return Ok(None);
    }
    let Some(bytes) = private_record::read_private(
        &folder,
        &receipt_name(operation_id),
        MAX_SETUP_RECEIPT_BYTES,
    )?
    else {
        return Ok(None);
    };
    SetupReceipt::decode(&bytes)
        .map(Some)
        .map_err(|refusal| anyhow::anyhow!("setup_receipt_invalid: {}", refusal.code()))
}

/// Durably replace the receipt.
pub(crate) fn write_receipt(state: &Path, receipt: &SetupReceipt) -> Result<()> {
    airk_ipc::endpoint::prepare_root(state)?;
    let folder = receipt_folder(state);
    airk_ipc::endpoint::prepare_root(&folder)?;
    private_record::write_private(
        &folder,
        &receipt_name(&receipt.operation_id),
        &canonical(receipt),
        MAX_SETUP_RECEIPT_BYTES,
        true,
    )
}

/// The fixed per-user pointer path (§5.2).
pub(crate) fn pointer_path() -> Result<PathBuf> {
    airk_process::user_folders::install_pointer()
}

fn pointer_folder() -> Result<PathBuf> {
    Ok(pointer_path()?
        .parent()
        .context("install pointer folder")?
        .to_path_buf())
}

/// The installation lock beside the pointer: one setup operation per user
/// at a time, whatever locations it chose (§5.6 phase 1).
pub(crate) fn lock() -> Result<PrivateLock> {
    let folder = pointer_folder()?;
    airk_ipc::endpoint::prepare_root(&folder)?;
    private_record::lock(
        &folder,
        LOCK_FILE,
        "installation_in_progress: another AIRK Service setup or uninstall is running for this user",
    )
}

/// The installation the pointer names, verified as the `airkctl` reader
/// verifies it (pointer, record and their relation); executables are not
/// hashed here. `None` when no pointer exists.
pub(crate) fn read_installation() -> Result<Option<Installed>> {
    let path = pointer_path()?;
    if std::fs::symlink_metadata(&path).is_err() {
        return Ok(None);
    }
    let folder = pointer_folder()?;
    let bytes = private_record::read_private(&folder, POINTER_FILE, MAX_POINTER_BYTES)?
        .context("install pointer disappeared")?;
    let pointer = InstallPointer::decode(&bytes)
        .map_err(|refusal| anyhow::anyhow!("installation_unverified: {}", refusal.code()))?;
    let record = read_record(&pointer)?;
    Ok(Some(Installed { pointer, record }))
}

fn read_record(pointer: &InstallPointer) -> Result<InstallRecord> {
    let program = PathBuf::from(&pointer.program);
    no_links(&program)?;
    let bytes = read_bounded(&program.join(&pointer.record), MAX_RECORD_BYTES, true)?;
    InstallRecord::decode(&bytes, pointer)
        .map_err(|refusal| anyhow::anyhow!("installation_unverified: {}", refusal.code()))
}

/// Write the pointer last, atomically (temporary file, rename, folder sync).
pub(crate) fn write_pointer(pointer: &InstallPointer) -> Result<()> {
    let folder = pointer_folder()?;
    airk_ipc::endpoint::prepare_root(&folder)?;
    private_record::write_private(
        &folder,
        POINTER_FILE,
        &canonical(pointer),
        MAX_POINTER_BYTES,
        true,
    )
}

/// Announce that setup replaces or removes the installed files (I4
/// `ReplacementNotice`): editor windows release the installed `airkctl`.
pub(crate) fn write_replacement_notice(notice: &ReplacementNotice) -> Result<()> {
    let folder = pointer_folder()?;
    airk_ipc::endpoint::prepare_root(&folder)?;
    private_record::write_private(
        &folder,
        REPLACEMENT_NOTICE_FILE,
        &canonical(notice),
        MAX_REPLACEMENT_NOTICE_BYTES,
        true,
    )
}

/// Withdraw the notice; `false` when none existed.
pub(crate) fn remove_replacement_notice() -> Result<bool> {
    let path = pointer_folder()?.join(REPLACEMENT_NOTICE_FILE);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
        Ok(_) => {
            private_record::private_file(&path)?;
            std::fs::remove_file(&path)?;
            Ok(true)
        }
    }
}

/// Remove the pointer; `false` when none existed.
pub(crate) fn remove_pointer() -> Result<bool> {
    let path = pointer_path()?;
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
        Ok(_) => {
            private_record::private_file(&path)?;
            std::fs::remove_file(&path)?;
            Ok(true)
        }
    }
}

/// Write the record into the program folder; returns its canonical bytes.
pub(crate) fn write_record(program: &Path, record: &InstallRecord) -> Result<Vec<u8>> {
    let bytes = canonical(record);
    ensure!(
        bytes.len() <= MAX_RECORD_BYTES,
        "install record exceeds its bound"
    );
    write_owner_file(program, RECORD_FILE, &bytes)?;
    Ok(bytes)
}

/// Read back the record bytes written into `program` (owner-private).
pub(crate) fn record_bytes(program: &Path) -> Result<Vec<u8>> {
    read_bounded(&program.join(RECORD_FILE), MAX_RECORD_BYTES, true)
}

/// Remove the record; `false` when none existed.
pub(crate) fn remove_record(program: &Path) -> Result<bool> {
    let path = program.join(RECORD_FILE);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
        Ok(_) => {
            private_record::require_regular_unlinked(&path)?;
            std::fs::remove_file(&path)?;
            Ok(true)
        }
    }
}

/// An owner-private file in a folder that is not itself owner-only (the
/// program folder, the operation folder): temporary sibling, mode 0600 on
/// Unix, rename over the target, then the owner-private check.
pub(crate) fn write_owner_file(folder: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    no_links(folder)?;
    let mut temporary = tempfile::NamedTempFile::new_in(folder)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    let path = folder.join(name);
    temporary.persist(&path).map_err(|error| error.error)?;
    airk_ipc::endpoint::require_owner_private(&path)?;
    #[cfg(unix)]
    std::fs::File::open(folder)?.sync_all()?;
    Ok(())
}

/// The record's data and cache folders exist and are private to this user
/// (created owner-only when missing, as the state folder is). The index setup
/// review needs the data folder; a fresh install and a reinstall after
/// "delete data" have neither.
pub(crate) fn prepare_locations(locations: &airk_core::install_record::Locations) -> Result<()> {
    for location in [&locations.data, &locations.cache] {
        airk_ipc::endpoint::prepare_root(Path::new(location))
            .with_context(|| format!("location_unavailable: {location}"))?;
    }
    Ok(())
}

/// No existing component of `path` is a link or reparse point (Unix: links
/// owned by root, such as macOS `/var`, are host aliases and allowed).
pub(crate) fn no_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors().filter(|part| !part.as_os_str().is_empty()) {
        let Ok(metadata) = std::fs::symlink_metadata(ancestor) else {
            continue;
        };
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0 && !metadata.file_type().is_symlink(),
                "{} is a link or reparse point",
                ancestor.display()
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                !metadata.file_type().is_symlink() || metadata.uid() == 0,
                "{} is a link",
                ancestor.display()
            );
        }
    }
    Ok(())
}

/// An existing location folder, which the I4 readers check as private
/// (Windows: owner and every allowed ACE limited to this user, SYSTEM and
/// Administrators, as `require_owner_private`; Unix: uid and no group/other
/// write). Machine-wide folders fail this check (U39).
pub(crate) fn owned_folder(path: &Path) -> Result<()> {
    no_links(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(metadata.is_dir(), "{} is not a folder", path.display());
    #[cfg(windows)]
    airk_ipc::endpoint::require_owner_private(path).with_context(|| {
        format!(
            "{} exists and is not private to this user; choose a folder that does not exist yet (AIRK creates its folders private)",
            path.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // SAFETY: geteuid takes no pointers and has no failure mode.
        let uid = unsafe { libc::geteuid() };
        ensure!(
            metadata.uid() == uid && metadata.permissions().mode() & 0o022 == 0,
            "{} is not owned by this user or is writable by others",
            path.display()
        );
    }
    Ok(())
}

/// The nearest existing parent of a location that does not exist yet: a
/// folder no other account can change (U39 "not writable by others"). On
/// Windows read-only grants to other accounts are accepted
/// (`require_not_writable_by_others`), because AIRK creates the location
/// below it owner-only; Unix keeps the `owned_folder` rule.
pub(crate) fn safe_parent(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        no_links(path)?;
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(metadata.is_dir(), "{} is not a folder", path.display());
        airk_ipc::endpoint::require_not_writable_by_others(path)
            .with_context(|| format!("{} cannot hold AIRK folders", path.display()))
    }
    #[cfg(unix)]
    owned_folder(path)
}

/// The nearest existing ancestor of `path` (the path itself when it exists).
pub(crate) fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|part| std::fs::symlink_metadata(part).is_ok())
        .map(Path::to_path_buf)
}

/// Every file below `folder`, relative and `/`-separated; links refused.
pub(crate) fn list_files(folder: &Path) -> Result<Vec<String>> {
    fn visit(
        folder: &Path,
        prefix: &str,
        found: &mut Vec<String>,
        budget: &mut usize,
    ) -> Result<()> {
        for entry in std::fs::read_dir(folder)? {
            let entry = entry?;
            *budget = budget
                .checked_sub(1)
                .context("program folder holds too many entries")?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("non-UTF-8 name in {}", folder.display()))?;
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let metadata = std::fs::symlink_metadata(entry.path())?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "{} is a reparse point",
                    entry.path().display()
                );
            }
            if metadata.file_type().is_symlink() {
                bail!("{} is a link", entry.path().display());
            } else if metadata.is_dir() {
                visit(&entry.path(), &relative, found, budget)?;
            } else {
                found.push(relative);
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    let mut budget = MAX_LISTED_ENTRIES;
    visit(folder, "", &mut found, &mut budget)?;
    found.sort();
    Ok(found)
}

/// Whether `folder` is absent or an empty folder.
pub(crate) fn absent_or_empty(folder: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(folder) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
        Ok(metadata) if metadata.is_dir() => Ok(std::fs::read_dir(folder)?.next().is_none()),
        Ok(_) => Ok(false),
    }
}
