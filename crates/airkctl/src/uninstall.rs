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
mod custody;
pub(crate) mod recovery_child;

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
    airk_ipc::private_record::require_regular_unlinked(path)
        .map_err(|error| failed(format!("{}: {error:#}", path.display())))?;
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
    airk_ipc::endpoint::require_existing_root(folder)
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
    recovery_child::launch(airk, operation).map(|child| child.id())
}

/// Hand the uninstall of the verified installation to setup and return what
/// was started.
pub fn launch(verified: &Verified, purge: bool) -> Result<Value, ErrorLine> {
    let program = PathBuf::from(&verified.pointer.program);
    let program_pin = airk_process::directory::OpenedDirectory::open(&program)
        .map_err(|error| failed(format!("program directory: {error}")))?;
    airk_ipc::endpoint::require_private_directory_handle(program_pin.as_file())
        .map_err(|error| failed(format!("program directory: {error:#}")))?;
    let image = airk_core::install_record::setup_recovery::InstallationImage {
        pointer: verified.pointer.clone(),
        record: verified.record.clone(),
    };
    let expected=airk_ipc::install_reader::completed_inventory(&crate::install::pointer_path().map_err(|error|failed(error.to_string()))?,&image)
        .map_err(|error|failed(format!("original completed inventory proof refused: {error:#}")))?
        .ok_or_else(||failed("full recovery custody requires completed private original inventory proof; legacy artifacts were preserved; use the verified original installer or manual removal guidance"))?;
    let (inventory_bytes, inventory) = custody::inventory(&program, verified)?;
    let inventory_digest = Sha256Digest::from_bytes(Sha256::digest(&inventory_bytes).into());
    if inventory_digest != expected {
        return Err(failed(
            "original raw inventory changed; no recovery payload was copied",
        ));
    }
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| failed(format!("operation id: {error}")))?;
    let operation_id = Incarnation::from_bytes(nonce);
    let operation = operation(
        verified,
        operation_id,
        inventory_digest,
        purge,
        now_unix_ms(),
    );
    let temporary = std::env::temp_dir()
        .canonicalize()
        .map_err(|error| failed(format!("recovery parent: {error}")))?;
    if temporary.starts_with(
        program
            .canonicalize()
            .map_err(|error| failed(error.to_string()))?,
    ) {
        return Err(failed(
            "recovery runtime must be outside the tracked program directory",
        ));
    }
    let folder = temporary.join(format!("airk-setup-{operation_id}"));
    #[cfg(windows)]
    airk_ipc::endpoint::create_root(&folder)
        .map_err(|error| failed(format!("operation folder: {error:#}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&folder)
            .map_err(|error| failed(error.to_string()))?;
    }
    let path = write_operation(&folder, &operation)?;
    let recovery = custody::retain(
        &program,
        &folder,
        verified,
        &inventory_bytes,
        &inventory,
        crate::install::platform_signature,
    )
    .map_err(|line| recovery_error(line, &path))?;
    use airk_core::install_record::setup_recovery::active::{
        EXPECTED_POINTER_FILE, EXPECTED_RECORD_FILE,
    };
    for (name, bytes) in [
        (EXPECTED_POINTER_FILE, canonical(&verified.pointer)),
        (EXPECTED_RECORD_FILE, canonical(&verified.record)),
    ] {
        airk_ipc::private_record::write_private(
            &folder,
            name,
            &bytes,
            airk_core::install_record::MAX_RECORD_BYTES,
            false,
        )
        .map_err(|error| {
            failed(format!(
                "original I4 snapshot: {error:#}; retained operation {}",
                path.display()
            ))
        })?;
    }
    airk_process::directory::OpenedDirectory::open(&folder)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            failed(format!(
                "operation sync: {error}; retained operation {}",
                path.display()
            ))
        })?;
    let image = airk_core::install_record::setup_recovery::InstallationImage {
        pointer: verified.pointer.clone(),
        record: verified.record.clone(),
    };
    let inno = if uses_inno(verified.record.installed_by.installer, cfg!(windows)) {
        Some(
            custody::inno(&program, &image, crate::install::platform_signature)
                .map_err(|line| recovery_error(line, &path))?,
        )
    } else {
        None
    };
    let airk = recovery.join(&inventory.executables.airk);
    admit_removal(&airk, &path, &operation, &image)
        .map_err(|line| recovery_unknown(line, &path))?;
    let process = if let Some(inno) = inno {
        let launcher = inno.path.clone();
        drop(inno); // Inno opens its finalized DAT for exclusive read/write.
        spawn_inno(&launcher, &path, &airk)
    } else {
        spawn_setup(&airk, &path)
    }
    .map_err(|line| recovery_unknown(line, &path))?;
    let receipt = PathBuf::from(&verified.record.locations.state)
        .join("setup")
        .join(format!("{operation_id}.json"));
    Ok(pending(&operation, &path, &recovery, process, &receipt))
}

fn uses_inno(installer: airk_core::install_record::Installer, windows: bool) -> bool {
    use airk_core::install_record::Installer;
    match installer {
        Installer::Inno => windows,
        Installer::Portable | Installer::Dmg | Installer::AppFirstLaunch | Installer::InstallSh => {
            false
        }
    }
}

fn pending(
    operation: &SetupOperation,
    path: &Path,
    recovery: &Path,
    process: u32,
    receipt: &Path,
) -> Value {
    json!({
        "started": true,
        "operation_id": operation.operation_id,
        "kind": operation.kind,
        "delete_indexes": operation.options.delete_indexes,
        "process_id": process,
        "receipt": receipt,
        "operation": path,
        "recovery_program": recovery,
    })
}

fn recovery_error(line: ErrorLine, operation: &Path) -> ErrorLine {
    let mut retained = ErrorLine::new(
        line.class,
        &line.code,
        line.retryable,
        &format!(
            "retained original operation {}; {}",
            operation.display(),
            line.message
        ),
    );
    retained.retry_after_ms = line.retry_after_ms;
    retained.native = line.native;
    retained
}

fn recovery_unknown(line: ErrorLine, operation: &Path) -> ErrorLine {
    let mut result = ErrorLine::new(
        ExitClass::OutcomeUnknown,
        "uninstall_outcome_unknown",
        false,
        &format!(
            "retained original operation {}; native admission may have occurred; inspect the original receipt before recovery; {}",
            operation.display(),
            line.message
        ),
    );
    result.native = serde_json::to_value(line).ok();
    result
}

fn admit_removal(
    airk: &Path,
    path: &Path,
    operation: &SetupOperation,
    image: &airk_core::install_record::setup_recovery::InstallationImage,
) -> Result<(), ErrorLine> {
    let operation_path = path
        .to_str()
        .ok_or_else(|| failed("operation path is not UTF-8"))?;
    let invocation = airk_process::bounded_command::Invocation::new(
        airk,
        &[
            "__airk_role",
            "setup",
            "--operation",
            operation_path,
            "--admit-removal",
        ],
    );
    let output = airk_process::bounded_command::execute(&invocation)
        .map_err(|error| failed(format!("native admission unproven: {error:#}")))?;
    if !output.success {
        return Err(failed(format!("native admission refused: {}", output.text)));
    }
    let pointer = crate::install::pointer_path().map_err(|error| failed(error.to_string()))?;
    let folder = pointer
        .parent()
        .ok_or_else(|| failed("install pointer parent missing"))?;
    let bytes = airk_ipc::private_record::read_private(
        folder,
        "active-setup.json",
        airk_core::install_record::setup_recovery::MAX_RECOVERY_BYTES,
    )
    .map_err(|error| failed(format!("native admission unproven: {error:#}")))?
    .ok_or_else(|| failed("native admission did not persist its owner"))?;
    use airk_core::install_record::setup_recovery::active::{ActiveSetup, Target};
    let active = ActiveSetup::decode(&bytes, operation)
        .map_err(|error| failed(format!("native admission unproven: {}", error.code())))?;
    match active.target {
        Target::Remove { installation } if installation.as_ref() == image => Ok(()),
        _ => Err(failed(
            "native admission names another original installation",
        )),
    }
}

fn spawn_inno(
    uninstaller: &Path,
    operation: &Path,
    recovery_image: &Path,
) -> Result<u32, ErrorLine> {
    let arguments: Vec<std::ffi::OsString> = vec![
        "/VERYSILENT".into(),
        "/SUPPRESSMSGBOXES".into(),
        "/NORESTART".into(),
        format!("/AIRKOPERATION={}", operation.display()).into(),
        format!("/AIRKRECOVERYIMAGE={}", recovery_image.display()).into(),
    ];
    #[cfg(windows)]
    {
        airk_process::detached::spawn_detached(uninstaller, &arguments)
            .map(|child| child.id())
            .map_err(|error| failed(format!("uninstaller launch: {error}")))
    }
    #[cfg(not(windows))]
    {
        let _ = (uninstaller, arguments);
        Err(failed("Inno removal requires Windows"))
    }
}

#[cfg(test)]
mod tests;
