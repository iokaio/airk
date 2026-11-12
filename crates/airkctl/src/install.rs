// SPDX-License-Identifier: Apache-2.0
//! The I4 installation reader (`docs/split/contracts.md` §5.2–§5.4; split step
//! 3b.1c): the per-user install pointer, the install record it names and the
//! pair of executables the record lists. Every check fails closed. Executable
//! signatures (§5.4.3; step 3b.1c-sig) are checked by the one implementation
//! in `airk_process::signature` against the C10 identity: Windows
//! Authenticode, macOS `codesign`, Linux digest only (documented weaker).
use airk_core::install_record::{
    Executable, InstallPointer, InstallRecord, MAX_POINTER_BYTES, MAX_RECORD_BYTES, Signing,
};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

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

/// The fixed per-user pointer path (§5.2): native folders, never inherited
/// `HOME`, `XDG_*` or `LOCALAPPDATA` overrides.
pub fn pointer_path() -> anyhow::Result<PathBuf> {
    airk_process::user_folders::install_pointer()
}

/// The signature check of the platform this build runs on (§5.4.3; step
/// 3b.1c-sig) for the verified executables. A Windows or macOS record must
/// name the C10 identity and both images must carry it; a Linux record
/// carries no signature and rests on the record digest. There is no unsigned
/// or development acceptance here: the record's `signing` has none.
pub fn platform_signature(
    signing: &Signing,
    executables: &[&Path],
) -> Result<(), (String, String)> {
    airk_process::signature::verify(claim(signing), executables)
        .map(|_| ())
        .map_err(|refusal| (refusal.code.to_owned(), refusal.message))
}

/// The record's signing as the shared check reads it.
fn claim(signing: &Signing) -> airk_process::signature::Claim<'_> {
    use airk_process::signature::Claim;
    match signing {
        Signing::Windows {
            publisher,
            identity_eku,
        } => Claim::Windows {
            publisher,
            identity_eku,
        },
        Signing::Macos { team_id } => Claim::Macos { team_id },
        Signing::Linux { signed, integrity } => Claim::Linux {
            signed: *signed,
            integrity,
        },
    }
}

/// Discover this user's installation.
pub fn discover() -> Discovery {
    match pointer_path() {
        Ok(path) => read(&path, platform_signature),
        Err(error) => unverified("installation_unverified", format!("{error:#}")),
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
pub(crate) fn owned_bytes(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    regular(path)?;
    airk_ipc::endpoint::require_owner_private(path)
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
    match std::fs::symlink_metadata(pointer_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Discovery::NotFound,
        Err(error) => return unverified("installation_unverified", error.to_string()),
        Ok(_) => {}
    }
    // The pointer folder and its parent are created by the installer; neither
    // may be a link (§5.1 authorization).
    for folder in pointer_path.ancestors().skip(1).take(2) {
        match std::fs::symlink_metadata(folder) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            _ => {
                return unverified(
                    "installation_unverified",
                    format!("{} is not a plain folder", folder.display()),
                );
            }
        }
    }
    let pointer = match owned_bytes(pointer_path, MAX_POINTER_BYTES) {
        Ok(bytes) => match InstallPointer::decode(&bytes) {
            Ok(pointer) => pointer,
            Err(refusal) => return unverified(refusal.code(), "the install pointer is invalid"),
        },
        Err(message) => return unverified("installation_unverified", message),
    };
    let program = PathBuf::from(&pointer.program);
    let record = match owned_bytes(&program.join(&pointer.record), MAX_RECORD_BYTES) {
        Ok(bytes) => match InstallRecord::decode(&bytes, &pointer) {
            Ok(record) => record,
            Err(refusal) => return unverified(refusal.code(), "the install record is invalid"),
        },
        Err(message) => return unverified("installation_unverified", message),
    };
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

#[cfg(test)]
mod tests;
