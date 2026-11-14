// SPDX-License-Identifier: Apache-2.0
//! Shared bounded I4 filesystem reader. Platform folder discovery and signature
//! policy stay with callers; this module has no process or service dependency.
use airk_core::install_record::{
    Executable, InstallPointer, InstallRecord, MAX_POINTER_BYTES, MAX_RECORD_BYTES, Signing,
};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

mod history;
pub use history::completed_inventory;

/// A verified installation.
#[derive(Debug)]
pub struct Verified {
    pub pointer: InstallPointer,
    pub record: InstallRecord,
    pub airk: PathBuf,
    pub airkctl: PathBuf,
}

/// What discovery found.
#[derive(Debug)]
pub enum Discovery {
    /// No install pointer exists for this user.
    NotFound,
    Verified(Box<Verified>),
    /// A check failed; `code` is the refusal, never a partial result.
    Unverified {
        code: String,
        message: String,
    },
}

fn unverified(code: &str, message: impl Into<String>) -> Discovery {
    Discovery::Unverified {
        code: code.into(),
        message: message.into(),
    }
}

/// A regular, single-component file that is not a link.
fn regular(path: &Path) -> Result<std::fs::Metadata, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    Ok(metadata)
}

/// An owner-private regular file read within `limit` bytes.
pub fn owned_bytes(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    regular(path)?;
    crate::endpoint::require_owner_private(path)
        .map_err(|error| format!("{}: {error:#}", path.display()))?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(limit as u64 + 1).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err(format!("{} exceeds {limit} bytes", path.display()));
    }
    Ok(bytes)
}

/// An executable inside `program` whose size and digest match the record.
fn executable(program: &Path, listed: &Executable) -> Result<PathBuf, String> {
    let path = program.join(&listed.path);
    let metadata = regular(&path)?;
    if metadata.len() != listed.bytes {
        return Err(format!("{} changed size", path.display()));
    }
    let mut hash = Sha256::new();
    let mut file = std::fs::File::open(&path).map_err(|error| error.to_string())?;
    std::io::copy(&mut file, &mut hash).map_err(|error| error.to_string())?;
    let digest = airk_core::identity::Sha256Digest::from_bytes(hash.finalize().into());
    if digest != listed.sha256 {
        return Err(format!("{} changed content", path.display()));
    }
    Ok(path)
}

/// Read and verify the installation the pointer at `pointer_path` names.
pub fn read(
    pointer_path: &Path,
    signature: impl Fn(&Signing, &[&Path]) -> Result<(), (String, String)>,
) -> Discovery {
    let image = match read_metadata(pointer_path) {
        Ok(Some(image)) => image,
        Ok(None) => return Discovery::NotFound,
        Err((code, message)) => return unverified(&code, message),
    };
    let pointer = image.pointer;
    let record = image.record;
    let program = PathBuf::from(&pointer.program);
    let airk = match executable(&program, &record.executables.airk) {
        Ok(path) => path,
        Err(message) => return unverified("installation_unverified", message),
    };
    let airkctl = match executable(&program, &record.executables.airkctl) {
        Ok(path) => path,
        Err(message) => return unverified("installation_unverified", message),
    };
    let executables = [airkctl.as_path(), airk.as_path()];
    if let Err((code, message)) = signature(&record.signing, &executables) {
        return unverified(&code, message);
    }
    Discovery::Verified(Box::new(Verified {
        pointer,
        record,
        airk,
        airkctl,
    }))
}

/// Canonical owner-private I4 metadata only. This deliberately returns no
/// Verified value and grants no executable, start, adoption or write authority.
/// Explicit signed repair can inspect an otherwise damaged installation;
/// ordinary discovery still verifies both actual images and C10 afterwards.
pub fn read_metadata(
    pointer_path: &Path,
) -> Result<Option<airk_core::install_record::setup_recovery::InstallationImage>, (String, String)>
{
    let error = |code: &str, message: String| (code.to_owned(), message);
    match std::fs::symlink_metadata(pointer_path) {
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(failure) => return Err(error("installation_unverified", failure.to_string())),
        Ok(_) => {}
    }
    for folder in pointer_path.ancestors().skip(1).take(2) {
        match std::fs::symlink_metadata(folder) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            _ => {
                return Err(error(
                    "installation_unverified",
                    format!("{} is not a plain folder", folder.display()),
                ));
            }
        }
    }
    let pointer_bytes = owned_bytes(pointer_path, MAX_POINTER_BYTES)
        .map_err(|message| error("installation_unverified", message))?;
    let pointer = InstallPointer::decode(&pointer_bytes)
        .map_err(|refusal| error(refusal.code(), "the install pointer is invalid".into()))?;
    let program = PathBuf::from(&pointer.program);
    let record_bytes = owned_bytes(&program.join(&pointer.record), MAX_RECORD_BYTES)
        .map_err(|message| error("installation_unverified", message))?;
    let record = InstallRecord::decode(&record_bytes, &pointer)
        .map_err(|refusal| error(refusal.code(), "the install record is invalid".into()))?;
    Ok(Some(
        airk_core::install_record::setup_recovery::InstallationImage { pointer, record },
    ))
}
