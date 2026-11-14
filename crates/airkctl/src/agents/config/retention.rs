// SPDX-License-Identifier: Apache-2.0
//! One retained publication. This module never restores, replays or deletes archives.
use super::{edit_record::*, eligibility::Eligibility};
use airk_process::directory::OpenedDirectory;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fmt,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
mod platform;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditFailureKind {
    Refused,
    ConflictRetained,
    OutcomeUnknown,
}
#[derive(Debug)]
pub(crate) struct EditFailure {
    pub kind: EditFailureKind,
    message: String,
}
impl fmt::Display for EditFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl From<String> for EditFailure {
    fn from(message: String) -> Self {
        Self {
            kind: EditFailureKind::Refused,
            message,
        }
    }
}
impl From<&str> for EditFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
type Result<T> = std::result::Result<T, EditFailure>;

#[cfg(test)]
thread_local! { static FAIL_FINAL_RECORD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

#[cfg(test)]
fn with_final_record_failure<T>(run: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            FAIL_FINAL_RECORD.with(|flag| flag.set(self.0));
        }
    }
    let previous = FAIL_FINAL_RECORD.with(|flag| flag.replace(true));
    let _restore = Restore(previous);
    run()
}

fn unknown_receipt(path: &Path) -> EditFailure {
    EditFailure {
        kind: EditFailureKind::OutcomeUnknown,
        message: format!(
            "Configuration edit outcome is unknown. Keep every retained object and inspect {}; do not replay or restore automatically.",
            path.display()
        ),
    }
}
fn refused(error: impl fmt::Display) -> EditFailure {
    format!("Automatic config editing was refused; apply the edit manually: {error}").into()
}

fn same_parent(parent: &OpenedDirectory, path: &Path) -> std::io::Result<()> {
    if OpenedDirectory::open(path)?.identity()? != parent.identity()? {
        return Err(std::io::Error::other("target parent namespace changed"));
    }
    Ok(())
}

fn owner() -> std::result::Result<Owner, String> {
    let user = airk_ipc::endpoint::current_user().map_err(|error| error.to_string())?;
    #[cfg(windows)]
    {
        Ok(Owner::Windows { sid: user })
    }
    #[cfg(unix)]
    {
        Ok(Owner::Unix {
            uid: user.parse().map_err(|_| "invalid invoking UID")?,
        })
    }
}

fn capture(file: &mut File) -> std::io::Result<(Snapshot, Vec<u8>)> {
    let before = file.metadata()?;
    let identity = platform::identity(file)?;
    if !before.is_file() || before.len() > MAX_INPUT_BYTES {
        return Err(std::io::Error::other(
            "configuration exceeds original input bound",
        ));
    }
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    Read::by_ref(file)
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    if bytes.len() as u64 != before.len()
        || before.len() != after.len()
        || before.modified()? != after.modified()?
        || identity != platform::identity(file)?
    {
        return Err(std::io::Error::other(
            "configuration changed during bounded observation",
        ));
    }
    Ok((
        Snapshot {
            identity,
            length: bytes.len() as u64,
            sha256: airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
        },
        bytes,
    ))
}

fn observation(directory: &OpenedDirectory, path: &Path, name: &str) -> Observation {
    match platform::open_file(directory, OsStr::new(name), path, false, false) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Observation::Absent,
        Err(_) => Observation::Incomplete {
            identity: None,
            length: None,
            observed_bytes: 0,
            reason: IncompleteReason::Unreadable,
        },
        Ok(mut file) => {
            let identity = platform::identity(&file).ok();
            let length = file.metadata().ok().map(|m| m.len());
            if length.is_some_and(|n| n > MAX_INPUT_BYTES) {
                return Observation::Incomplete {
                    identity,
                    length,
                    observed_bytes: 0,
                    reason: IncompleteReason::Oversized,
                };
            }
            match capture(&mut file) {
                Ok((file, _)) => Observation::Complete { file },
                Err(_) => Observation::Incomplete {
                    identity,
                    length,
                    observed_bytes: 0,
                    reason: IncompleteReason::Changing,
                },
            }
        }
    }
}

fn directory(
    parent: &OpenedDirectory,
    parent_path: &Path,
    name: &str,
) -> Result<(OpenedDirectory, PathBuf, bool)> {
    let path = parent_path.join(name);
    let (directory, created) = match parent.open_child(OsStr::new(name)) {
        Ok(directory) => (directory, false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (
            platform::create_directory(parent, OsStr::new(name), &path).map_err(refused)?,
            true,
        ),
        Err(error) => return Err(refused(error)),
    };
    airk_ipc::endpoint::require_private_directory_handle(directory.as_file()).map_err(refused)?;
    Ok((directory, path, created))
}

fn write_metadata(file: &mut File, bytes: &[u8]) -> Result<()> {
    airk_ipc::endpoint::require_private_metadata_handle(file).map_err(refused)?;
    file.seek(SeekFrom::Start(0)).map_err(refused)?;
    file.set_len(0).map_err(refused)?;
    file.write_all(bytes).map_err(refused)?;
    file.sync_all().map_err(refused)
}

fn metadata(directory: &OpenedDirectory, path: &Path, name: &str) -> Result<Vec<u8>> {
    let mut file =
        platform::open_file(directory, OsStr::new(name), path, false, false).map_err(refused)?;
    airk_ipc::endpoint::require_private_metadata_handle(&file).map_err(refused)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_RECEIPT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(refused)?;
    if bytes.len() > MAX_RECEIPT_BYTES {
        return Err(refused("retained metadata exceeds its contract bound"));
    }
    Ok(bytes)
}

fn admit(
    root: &OpenedDirectory,
    root_path: &Path,
    marker: &RootMarker,
    available: u64,
) -> Result<()> {
    let mut count = 0u64;
    let mut bytes = 0u64;
    for entry in fs::read_dir(root_path).map_err(refused)? {
        let entry = entry.map_err(refused)?;
        if entry.file_name() == "owner.json" {
            continue;
        }
        count += 1;
        if count > MAX_TRANSACTIONS {
            return Err(refused(
                "retention transaction capacity exceeded; review and explicitly discard old transactions",
            ));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| refused("unknown retention root entry"))?;
        let transaction = name
            .parse()
            .map_err(|_| refused("unknown retention root entry"))?;
        let directory = root.open_child(OsStr::new(&name)).map_err(refused)?;
        airk_ipc::endpoint::require_private_directory_handle(directory.as_file())
            .map_err(refused)?;
        let receipt = Receipt::decode(&metadata(
            &directory,
            &entry.path().join("receipt.json"),
            "receipt.json",
        )?)
        .map_err(refused)?;
        receipt
            .validate_binding(marker, transaction, &receipt.target_basename)
            .map_err(refused)?;
        if !matches!(
            receipt.state,
            State::PublishedRetained | State::RefusedRetained
        ) {
            return Err(refused(format!(
                "a retained transaction at {} requires manual review before another edit",
                entry.path().join("receipt.json").display()
            )));
        }
        for artifact in fs::read_dir(entry.path()).map_err(refused)? {
            let artifact = artifact.map_err(refused)?;
            let artifact_name = artifact.file_name();
            if artifact_name == "receipt.json" {
                continue;
            }
            if artifact_name != "before" && artifact_name != "prepared" {
                return Err(refused("unknown retained artifact"));
            }
            let file =
                platform::open_file(&directory, &artifact_name, &artifact.path(), false, false)
                    .map_err(refused)?;
            bytes = bytes
                .checked_add(file.metadata().map_err(refused)?.len())
                .ok_or_else(|| refused("retention byte count overflow"))?;
        }
    }
    super::edit_record::admit(count, bytes, 0, available).map_err(refused)
}

struct ArchiveRoot {
    root: OpenedDirectory,
    root_path: PathBuf,
    marker: RootMarker,
    _lock: File,
}

fn open_archive(
    parent: &OpenedDirectory,
    parent_path: &Path,
    available: u64,
) -> Result<ArchiveRoot> {
    let (airk, airk_path, _) = directory(parent, parent_path, ".airk")?;
    let (edits, edits_path, new_edits) = directory(&airk, &airk_path, "agent-edits")?;
    let (root, root_path, new_root) = if new_edits {
        directory(&edits, &edits_path, "v1")?
    } else {
        let path = edits_path.join("v1");
        let root = edits
            .open_child(OsStr::new("v1"))
            .map_err(|_| refused("unmarked retention subtree is not adopted"))?;
        airk_ipc::endpoint::require_private_directory_handle(root.as_file()).map_err(refused)?;
        (root, path, false)
    };
    let marker = RootMarker {
        schema: ROOT_SCHEMA.into(),
        owner: owner().map_err(refused)?,
        root: platform::identity(root.as_file()).map_err(refused)?,
        parent: platform::identity(parent.as_file()).map_err(refused)?,
    };
    marker.validate().map_err(refused)?;
    let marker_path = root_path.join("owner.json");
    let marker_file = if new_root && new_edits {
        let mut file =
            platform::open_file(&root, OsStr::new("owner.json"), &marker_path, true, true)
                .map_err(refused)?;
        write_metadata(&mut file, &marker.encode().map_err(refused)?)?;
        platform::flush_directory(&root, &root_path).map_err(refused)?;
        platform::flush_directory(&edits, &edits_path).map_err(refused)?;
        platform::flush_directory(&airk, &airk_path).map_err(refused)?;
        platform::flush_directory(parent, parent_path).map_err(refused)?;
        file
    } else {
        let retained =
            RootMarker::decode(&metadata(&root, &marker_path, "owner.json")?).map_err(refused)?;
        if retained != marker {
            return Err(refused("retention root ownership binding changed"));
        }
        platform::open_file(&root, OsStr::new("owner.json"), &marker_path, false, true)
            .map_err(refused)?
    };
    airk_ipc::endpoint::require_private_metadata_handle(&marker_file).map_err(refused)?;
    marker_file
        .try_lock()
        .map_err(|_| refused("another configuration edit is active"))?;
    let _lock = marker_file;
    admit(&root, &root_path, &marker, available)?;
    Ok(ArchiveRoot {
        root,
        root_path,
        marker,
        _lock,
    })
}

pub(super) struct ExistingEdit {
    parent: OpenedDirectory,
    parent_path: PathBuf,
    target_name: std::ffi::OsString,
    target_path: PathBuf,
    eligibility: Eligibility,
    available: u64,
    original: Snapshot,
    before: String,
    permissions: fs::Permissions,
    archive: ArchiveRoot,
}
impl ExistingEdit {
    pub fn before(&self) -> &str {
        &self.before
    }
}

/// No original configuration bytes are read before namespace, privacy, native
/// volume/durability and Git/package admission have succeeded.
pub(super) fn review(path: &Path) -> Result<ExistingEdit> {
    let raw_parent = path
        .parent()
        .ok_or_else(|| refused("missing target parent"))?;
    let parent = OpenedDirectory::open(raw_parent).map_err(refused)?;
    let parent_path = raw_parent.canonicalize().map_err(refused)?;
    if parent_path.to_string_lossy().len() > 2048 {
        return Err(refused("target path exceeds bounded recovery guidance"));
    }
    if OpenedDirectory::open(&parent_path)
        .map_err(refused)?
        .identity()
        .map_err(refused)?
        != parent.identity().map_err(refused)?
    {
        return Err(refused("target parent binding changed"));
    }
    airk_ipc::endpoint::require_protected_parent_handle(parent.as_file()).map_err(refused)?;
    #[cfg(windows)]
    airk_ipc::endpoint::require_private_directory_handle(parent.as_file()).map_err(refused)?;
    let target_name = path
        .file_name()
        .ok_or_else(|| refused("invalid target basename"))?
        .to_owned();
    let target_path = parent_path.join(&target_name);
    super::edit_record::validate_target_basename(
        &owner().map_err(refused)?,
        target_name
            .to_str()
            .ok_or_else(|| refused("non-UTF-8 target basename"))?,
    )
    .map_err(refused)?;
    let mut source =
        platform::open_file(&parent, &target_name, &target_path, false, true).map_err(refused)?;
    #[cfg(windows)]
    airk_ipc::endpoint::require_private_metadata_handle(&source).map_err(refused)?;
    if source.metadata().map_err(refused)?.len() > MAX_INPUT_BYTES {
        return Err(refused("configuration exceeds original input bound"));
    }
    let available = platform::available_bytes(&parent, &parent_path).map_err(refused)?;
    super::edit_record::admit(0, 0, 0, available).map_err(refused)?;
    platform::flush_directory(&parent, &parent_path).map_err(refused)?;
    let eligibility = Eligibility::review(&parent_path).map_err(refused)?;
    let archive = open_archive(&parent, &parent_path, available)?;
    let permissions = source.metadata().map_err(refused)?.permissions();
    let (original, bytes) = capture(&mut source).map_err(refused)?;
    let before = String::from_utf8(bytes).map_err(|_| refused("configuration is not UTF-8"))?;
    // The snapshot is immutable evidence, not an exclusive lease on the source.
    // Windows MoveFileEx replacement refuses an open target even when that
    // handle shares deletion. Release it before invoking caller/editor work;
    // replace() separately reopens and compares the complete original snapshot.
    drop(source);
    Ok(ExistingEdit {
        parent,
        parent_path,
        target_name,
        target_path,
        eligibility,
        available,
        original,
        before,
        permissions,
        archive,
    })
}

pub(super) fn replace(context: ExistingEdit, after: &str, operation: Operation) -> Result<()> {
    let ExistingEdit {
        parent,
        parent_path,
        target_name,
        target_path,
        eligibility,
        available,
        original,
        before,
        permissions,
        archive,
    } = context;
    eligibility.revalidate().map_err(refused)?;
    if OpenedDirectory::open(&parent_path)
        .map_err(refused)?
        .identity()
        .map_err(refused)?
        != parent.identity().map_err(refused)?
    {
        return Err(refused("target parent binding changed"));
    }
    let mut current =
        platform::open_file(&parent, &target_name, &target_path, false, false).map_err(refused)?;
    let (observed, bytes) = capture(&mut current).map_err(refused)?;
    if observed != original || bytes != before.as_bytes() {
        return Err(
            "configuration changed while AIRK prepared the edit; nothing was written".into(),
        );
    }
    drop(current);
    let ArchiveRoot {
        root,
        root_path,
        marker,
        _lock,
    } = archive;
    admit(&root, &root_path, &marker, available)?;
    eligibility.revalidate().map_err(refused)?;
    let transaction_name = eligibility.transaction.to_string();
    let (transaction, transaction_path, created) = directory(&root, &root_path, &transaction_name)?;
    if !created {
        return Err(refused(
            "transaction identity already exists; no replay is permitted",
        ));
    }
    let prepared_path = transaction_path.join("prepared");
    let mut prepared = platform::open_file(
        &transaction,
        OsStr::new("prepared"),
        &prepared_path,
        true,
        true,
    )
    .map_err(refused)?;
    let prepared_identity = platform::identity(&prepared).map_err(refused)?;
    let mut receipt = Receipt {
        schema: RECEIPT_SCHEMA.into(),
        transaction: eligibility.transaction,
        owner: marker.owner.clone(),
        root: marker.root.clone(),
        parent: marker.parent.clone(),
        target_basename: target_name
            .to_str()
            .ok_or_else(|| refused("non-UTF-8 target basename"))?
            .into(),
        operation,
        original,
        prepared: Snapshot {
            identity: prepared_identity,
            length: after.len() as u64,
            sha256: airk_core::identity::Sha256Digest::from_bytes(
                Sha256::digest(after.as_bytes()).into(),
            ),
        },
        primitive: if cfg!(windows) {
            Primitive::ReplaceFile
        } else if cfg!(target_os = "macos") {
            Primitive::RenameSwap
        } else {
            Primitive::RenameExchange
        },
        state: State::Prepared,
        sequence: 0,
        after: None,
    };
    receipt.validate().map_err(refused)?;
    prepared.write_all(after.as_bytes()).map_err(refused)?;
    #[cfg(unix)]
    prepared.set_permissions(permissions).map_err(refused)?;
    #[cfg(windows)]
    let _ = permissions;
    prepared.sync_all().map_err(refused)?;
    drop(prepared);
    let receipt_path = transaction_path.join("receipt.json");
    let mut receipt_file = platform::open_file(
        &transaction,
        OsStr::new("receipt.json"),
        &receipt_path,
        true,
        true,
    )
    .map_err(refused)?;
    write_metadata(&mut receipt_file, &receipt.encode().map_err(refused)?)?;
    platform::flush_directory(&transaction, &transaction_path).map_err(refused)?;
    platform::flush_directory(&root, &root_path).map_err(refused)?;
    #[cfg(test)]
    super::io::observe_before_publish(&target_path);
    let mut started = receipt.clone();
    started.state = State::PublicationStarted;
    started.sequence = 1;
    receipt.validate_successor(&started).map_err(refused)?;
    write_metadata(&mut receipt_file, &started.encode().map_err(refused)?)?;
    receipt = started;
    let backup_path = transaction_path.join("before");
    let result = same_parent(&parent, &parent_path)
        .and_then(|_| platform::open_file(&parent, &target_name, &target_path, false, true))
        .and_then(|file| {
            #[cfg(windows)]
            airk_ipc::endpoint::require_private_metadata_handle(&file)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let result = platform::publish(
                &parent,
                &transaction,
                &target_name,
                &target_path,
                &prepared_path,
                &backup_path,
            );
            drop(file);
            result
        });
    let sync = (|| -> std::io::Result<()> {
        for (directory, path, name) in [
            (&parent, &target_path, target_name.as_os_str()),
            (&transaction, &prepared_path, OsStr::new("prepared")),
            (&transaction, &backup_path, OsStr::new("before")),
        ] {
            match platform::open_file(directory, name, path, false, true) {
                Ok(file) => file.sync_all()?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
        }
        platform::flush_directory(&transaction, &transaction_path)?;
        platform::flush_directory(&parent, &parent_path)?;
        same_parent(&parent, &parent_path)
    })();
    let after_observation = After {
        target: observation(&parent, &target_path, receipt.target_basename.as_str()),
        prepared: observation(&transaction, &prepared_path, "prepared"),
        before: observation(&transaction, &backup_path, "before"),
        native_error: result
            .as_ref()
            .err()
            .or_else(|| sync.as_ref().err())
            .and_then(|e| e.raw_os_error())
            .map(|e| e as u32),
    };
    let retained = if receipt.primitive == Primitive::ReplaceFile {
        &after_observation.before
    } else {
        &after_observation.prepared
    };
    let matches = |value: &Observation, snapshot: &Snapshot| matches!(value, Observation::Complete { file } if file == snapshot);
    let state = if sync.is_err() {
        State::OutcomeUnknown
    } else if matches(&after_observation.target, &receipt.prepared)
        && matches(retained, &receipt.original)
    {
        State::PublishedRetained
    } else if matches(&after_observation.target, &receipt.prepared)
        && matches!(
            retained,
            Observation::Complete { .. }
                | Observation::Incomplete {
                    identity: Some(_),
                    ..
                }
        )
    {
        State::ConflictRetained
    } else if result.is_err()
        && matches(&after_observation.prepared, &receipt.prepared)
        && !matches(&after_observation.target, &receipt.prepared)
        && (receipt.primitive != Primitive::ReplaceFile
            || after_observation.before == Observation::Absent)
    {
        State::RefusedRetained
    } else {
        State::OutcomeUnknown
    };
    let mut final_receipt = receipt.clone();
    final_receipt.state = state;
    final_receipt.sequence = 2;
    final_receipt.after = Some(after_observation);
    #[cfg(test)]
    if FAIL_FINAL_RECORD.with(|flag| flag.replace(false)) {
        return Err(unknown_receipt(&receipt_path));
    }
    let recorded = receipt
        .validate_successor(&final_receipt)
        .map_err(str::to_owned)
        .and_then(|_| final_receipt.encode())
        .map_err(refused)
        .and_then(|bytes| write_metadata(&mut receipt_file, &bytes))
        .and_then(|_| platform::flush_directory(&transaction, &transaction_path).map_err(refused));
    if recorded.is_err() || state == State::OutcomeUnknown {
        return Err(unknown_receipt(&receipt_path));
    }
    match state {
        State::PublishedRetained => Ok(()),
        State::ConflictRetained => Err(EditFailure {
            kind: EditFailureKind::ConflictRetained,
            message: format!(
                "The proposed edit was published, but a concurrent version was retained. Review {} and its actual displaced file; do not replay or restore automatically.",
                receipt_path.display()
            ),
        }),
        _ => Err(refused(format!(
            "the target was not replaced; retained proposal and observations are at {}",
            receipt_path.display()
        ))),
    }
}

#[cfg(test)]
mod tests;
