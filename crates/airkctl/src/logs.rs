// SPDX-License-Identifier: Apache-2.0
//! Local reads of the service's log rings for `airkctl service logs`
//! (U86/U88/U89; split step 3b.1f-2): the same ownership proof, preflight and
//! bounded reads the service performs, over the shared layout and parser in
//! `airk_core::service_logs`. The rings sit beside the control root (U89; the
//! install record's `locations.logs` takes over in Phase 7). These reads never
//! create or change a path.
use airk_core::service_logs::{
    LOG_DIRECTORY, LOG_FILES, LOG_OWNER, LogPage, MAX_LOG_FILE_BYTES, MAX_RETAINED_LOG_BYTES,
    RAW_LOG_DIRECTORY, RAW_LOG_OWNER, RECENT_READ_BYTES, finish, owner_digest, read_tail,
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

fn exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// A regular file that is not a link (on Unix, with one name).
fn safe_file(path: &Path) -> Result<std::fs::Metadata> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "unsafe diagnostic file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(metadata.nlink() == 1, "unsafe diagnostic file");
    }
    Ok(metadata)
}

/// The ring folder after its ownership proof, or `None` when the ring and its
/// record are both absent and `absent_is_empty`.
fn owned_folder(
    control: &Path,
    directory: &str,
    owner: &str,
    absent_is_empty: bool,
) -> Result<Option<PathBuf>> {
    let parent = control;
    if absent_is_empty && !exists(&parent.join(owner))? && !exists(&parent.join(directory))? {
        return Ok(None);
    }
    ensure!(exists(parent)?, "diagnostic log location missing");
    airk_ipc::endpoint::require_existing_root(parent)?;
    let receipt = parent.join(owner);
    ensure!(
        exists(&receipt)?,
        "diagnostic logs have no ownership receipt"
    );
    safe_file(&receipt)?;
    let mut bytes = Vec::new();
    File::open(&receipt)?.take(4097).read_to_end(&mut bytes)?;
    let expected = serde_json::json!({
        "schema": 1,
        "root": owner_digest(&airk_process::user_folders::canonical_location(
            &std::path::absolute(control)?,
        )?),
    });
    ensure!(
        bytes.len() <= 4096
            && serde_json::from_slice::<serde_json::Value>(&bytes).ok() == Some(expected),
        "foreign diagnostic log ownership"
    );
    let folder = parent.join(directory);
    ensure!(exists(&folder)?, "diagnostic log directory missing");
    airk_ipc::endpoint::require_existing_root(&folder)?;
    Ok(Some(folder))
}

/// The ring's stored bytes, refusing unknown, unsafe or oversized entries.
fn preflight(directory: &Path) -> Result<u64> {
    let mut bytes = 0;
    for (count, entry) in std::fs::read_dir(directory)?.enumerate() {
        ensure!(count <= LOG_FILES.len(), "unknown diagnostic log entries");
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("non-UTF-8 log entry")?;
        ensure!(
            LOG_FILES.contains(&name) || name == "writer.lock",
            "unknown diagnostic log file"
        );
        let length = safe_file(&entry.path())?.len();
        ensure!(
            length
                <= if name == "writer.lock" {
                    0
                } else {
                    MAX_LOG_FILE_BYTES
                },
            "oversized diagnostic log file"
        );
        bytes += length;
    }
    Ok(bytes)
}

fn read_retained(
    control: &Path,
    limit: usize,
    read_bytes: usize,
    output_bytes: usize,
) -> Result<LogPage> {
    let directory = owned_folder(control, LOG_DIRECTORY, LOG_OWNER, false)?
        .context("diagnostic log directory missing")?;
    let stored_bytes = preflight(&directory)?;
    let mut page = LogPage {
        schema: 1,
        records: Vec::new(),
        stored_bytes,
        malformed_records: 0,
        tail_truncated: false,
    };
    let mut remaining = read_bytes;
    for name in LOG_FILES {
        let path = directory.join(name);
        if !exists(&path)? {
            continue;
        }
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        let count = usize::try_from(length.min(remaining as u64))?;
        let start = length - count as u64;
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        file.take(count as u64).read_to_end(&mut bytes)?;
        remaining -= bytes.len();
        if read_tail(&mut page, &bytes, start > 0, limit) || remaining == 0 {
            break;
        }
    }
    finish(page, output_bytes)
}

/// The newest `limit` (1..=32) records, as `airk service logs` read them.
pub fn recent(control: &Path, limit: usize) -> Result<LogPage> {
    ensure!(
        (1..=32).contains(&limit),
        "invalid diagnostic log page bound"
    );
    read_retained(control, limit, RECENT_READ_BYTES, 48 << 10)
}

/// The entire retained ring.
pub fn export(control: &Path) -> Result<LogPage> {
    read_retained(
        control,
        usize::MAX,
        MAX_RETAINED_LOG_BYTES as usize,
        MAX_RETAINED_LOG_BYTES as usize + (1 << 20),
    )
}

/// The raw job-error ring's exact bytes, oldest file first; files are
/// newline-delimited. Returns the bytes written.
pub fn export_raw_to(control: &Path, output: &mut impl Write) -> Result<u64> {
    let Some(directory) = owned_folder(control, RAW_LOG_DIRECTORY, RAW_LOG_OWNER, true)? else {
        return Ok(0);
    };
    preflight(&directory)?;
    let mut total = 0u64;
    for name in LOG_FILES.into_iter().rev() {
        let path = directory.join(name);
        if !exists(&path)? {
            continue;
        }
        safe_file(&path)?;
        let mut file = File::open(&path)?;
        let mut remaining = file.metadata()?.len();
        ensure!(
            remaining <= MAX_LOG_FILE_BYTES,
            "raw diagnostic file exceeds bound"
        );
        let mut block = [0u8; 65536];
        let mut last = None;
        while remaining > 0 {
            let length = remaining.min(block.len() as u64) as usize;
            let count = file.read(&mut block[..length])?;
            if count == 0 {
                break;
            }
            total += count as u64;
            ensure!(
                total <= MAX_RETAINED_LOG_BYTES + LOG_FILES.len() as u64,
                "raw diagnostic capture exceeds retention bound"
            );
            output.write_all(&block[..count])?;
            last = block.get(count - 1).copied();
            remaining -= count as u64;
        }
        // Preserve an interrupted record's exact bytes and delimit files.
        if last.is_some_and(|byte| byte != b'\n') {
            output.write_all(b"\n")?;
            total += 1;
        }
    }
    Ok(total)
}
