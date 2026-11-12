// SPDX-License-Identifier: Apache-2.0
//! `airkctl service uninstall` (U20; `docs/split/contracts.md` §3.10.5): a
//! thin launcher. It writes one owner-private `airk-setup-operation/1` file
//! in a fresh owner-only folder, starts the verified installed `airk` as the
//! private `setup` role and exits, so no open `airkctl` holds the installed
//! files. Setup owns all removal logic and records it in its receipt.
use crate::contract::{ErrorLine, ExitClass};
use crate::install::Verified;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    install_record::{
        Acknowledgements, IndexDeletion, MAX_SETUP_OPERATION_LIFETIME_MS, OperationKind,
        SETUP_OPERATION_SCHEMA, SetupOperation, SetupOptions, TrustChoice, canonical,
    },
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The payload inventory setup verifies against (setup_role CONTRACT.md).
const INVENTORY_FILE: &str = "payload-inventory.json";

fn failed(message: impl Into<String>) -> ErrorLine {
    ErrorLine::new(
        ExitClass::Refused,
        "uninstall_not_started",
        false,
        &message.into(),
    )
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// SHA-256 of a regular file that is not a link.
fn file_digest(path: &Path) -> Result<Sha256Digest, ErrorLine> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| failed(format!("{}: {error}", path.display())))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(failed(format!("{} is not a regular file", path.display())));
    }
    let mut hash = Sha256::new();
    let mut file = std::fs::File::open(path)
        .map_err(|error| failed(format!("{}: {error}", path.display())))?;
    std::io::copy(&mut file, &mut hash).map_err(|error| failed(error.to_string()))?;
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}

/// The operation an uninstall hands to setup (pure; tested).
pub fn operation(
    verified: &Verified,
    operation_id: Incarnation,
    payload_inventory_sha256: Sha256Digest,
    purge: bool,
    now: u64,
) -> SetupOperation {
    SetupOperation {
        schema: SETUP_OPERATION_SCHEMA.into(),
        operation_id,
        kind: OperationKind::Uninstall,
        created_unix_ms: now,
        expires_unix_ms: now + MAX_SETUP_OPERATION_LIFETIME_MS,
        requested_by: "airkctl".into(),
        acknowledgements: Acknowledgements {
            predecessor_removal: false,
            index_reset: purge,
        },
        locations: verified.record.locations.clone(),
        payload_inventory_sha256,
        options: SetupOptions {
            login_startup: false,
            trust: TrustChoice::Skip,
            start: false,
            delete_indexes: if purge {
                IndexDeletion::Purge
            } else {
                IndexDeletion::Keep
            },
        },
    }
}

/// Write the operation through a temporary name in a fresh owner-only folder.
fn write_operation(folder: &Path, operation: &SetupOperation) -> Result<PathBuf, ErrorLine> {
    airk_ipc::endpoint::prepare_root(folder)
        .map_err(|error| failed(format!("operation folder: {error:#}")))?;
    let path = folder.join("operation.json");
    let temporary = folder.join("operation.json.writing");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&temporary)
        .and_then(|mut file| {
            use std::io::Write;
            file.write_all(&canonical(operation))?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temporary, &path))
        .map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            failed(format!("operation file: {error}"))
        })?;
    Ok(path)
}

/// Start the verified `airk` as the `setup` role, detached, inheriting no
/// handles and no terminal (`airk_process::detached`).
fn spawn_setup(airk: &Path, operation: &Path) -> Result<u32, ErrorLine> {
    let arguments: Vec<std::ffi::OsString> = vec![
        "__airk_role".into(),
        "setup".into(),
        "--operation".into(),
        operation.into(),
    ];
    #[cfg(windows)]
    let child = airk_process::detached::spawn_detached(airk, &arguments).map(|child| child.id());
    #[cfg(unix)]
    let child = {
        let mut command = std::process::Command::new(airk);
        command
            .args(&arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        airk_process::detached::new_session(&mut command);
        airk_process::detached::without_error_dialogs(|| command.spawn()).map(|child| child.id())
    };
    child.map_err(|error| failed(format!("{}: {error}", airk.display())))
}

/// Hand the uninstall of the verified installation to setup and return what
/// was started.
pub fn launch(verified: &Verified, purge: bool) -> Result<Value, ErrorLine> {
    let program = PathBuf::from(&verified.pointer.program);
    let inventory = file_digest(&program.join(INVENTORY_FILE))?;
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| failed(format!("operation id: {error}")))?;
    let operation_id = Incarnation::from_bytes(nonce);
    let operation = operation(verified, operation_id, inventory, purge, now_unix_ms());
    let folder = std::env::temp_dir().join(format!("airk-setup-{operation_id}"));
    let path = write_operation(&folder, &operation)?;
    let process = match spawn_setup(&verified.airk, &path) {
        Ok(process) => process,
        Err(line) => {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_dir(&folder);
            return Err(line);
        }
    };
    let receipt = PathBuf::from(&verified.record.locations.state)
        .join("setup")
        .join(format!("{operation_id}.json"));
    Ok(json!({
        "started": true,
        "operation_id": operation_id,
        "kind": operation.kind,
        "delete_indexes": operation.options.delete_indexes,
        "process_id": process,
        "receipt": receipt,
    }))
}

#[cfg(test)]
mod tests;
