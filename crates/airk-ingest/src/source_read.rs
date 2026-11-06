// SPDX-License-Identifier: Apache-2.0
//! Read-only source handles with no-follow traversal, independent of discovery metadata.
use anyhow::{Context, Result, ensure};
use std::{
    fs::File,
    io::Read,
    path::{Component, Path},
};

/// Read only a `.git` marker at the selected root or one direct child, using
/// protected no-follow traversal. At most 4097 bytes are returned; the extra byte
/// reports truncation and must never be parsed as a complete gitdir reference.
/// This starts no discovery, extraction, model, writer or indexing operation.
pub fn read_git_marker(root: &Path, relative: &Path) -> Result<Vec<u8>> {
    ensure!(
        root.is_absolute() && relative.as_os_str().len() <= 4096,
        "invalid repository marker root or locator size"
    );
    let components = relative.components().take(3).collect::<Vec<_>>();
    ensure!(
        (1..=2).contains(&components.len())
            && components
                .iter()
                .all(|part| matches!(part, Component::Normal(_)))
            && relative.file_name().is_some_and(|name| name == ".git"),
        "invalid repository marker locator"
    );
    read(root, &root.join(relative), 4096, 4097)
}

/// Hash a complete saved source without discovery, extraction or publication work.
/// The caller supplies current authorized roots/policy and retains its read permit
/// until this cooperative operation returns, even when a filesystem call blocks.
pub fn verify_saved_source(
    root: &Path,
    relative: &Path,
    expected: airk_core::identity::Sha256Digest,
    max_bytes: u64,
    deadline: std::time::Instant,
    cancelled: &dyn Fn() -> bool,
) -> airk_core::verification::SourceCheck {
    use airk_core::verification::{STREAM_BYTES, SourceCheck, SourceCheckState as State};
    use sha2::{Digest, Sha256};
    let Some(logical) = relative.to_str() else {
        return SourceCheck::unchecked(State::Unsupported, 0, "non_utf8_locator");
    };
    if !root.is_absolute()
        || logical.is_empty()
        || logical.contains(['\\', ':', '\0'])
        || logical
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return SourceCheck::unchecked(State::OutOfScope, 0, "invalid_source_locator");
    }
    let stopped = || cancelled() || std::time::Instant::now() >= deadline;
    if stopped() {
        return SourceCheck::unchecked(State::BudgetExhausted, 0, "deadline_or_cancellation");
    }
    let mut bytes_read = 0;
    let attempt = (|| -> Result<SourceCheck> {
        let mut held = open(root, relative)?;
        let before = held.file.metadata()?;
        ensure!(before.is_file(), "source is not a regular file");
        let original_identity = identity(&held.file)?;
        // Reserve one possible byte for an EOF probe. It consumes no raw-byte
        // budget at EOF, but a growing/pseudo file must never exceed the limit
        // or be certified merely because metadata reported the old length.
        if before.len() >= max_bytes {
            return Ok(SourceCheck::unchecked(
                State::BudgetExhausted,
                0,
                "aggregate_byte_budget_including_eof_probe",
            ));
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; STREAM_BYTES];
        while bytes_read < before.len() {
            if stopped() {
                return Ok(SourceCheck::unchecked(
                    State::BudgetExhausted,
                    bytes_read,
                    "deadline_or_cancellation",
                ));
            }
            let count = usize::try_from((before.len() - bytes_read).min(STREAM_BYTES as u64))?;
            let n = match held.file.read(&mut buffer[..count]) {
                Ok(n) => n,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            };
            if n == 0 {
                return Ok(SourceCheck::unchecked(
                    State::ChangedDuringCheck,
                    bytes_read,
                    "source_length_changed",
                ));
            }
            bytes_read += n as u64;
            hasher.update(&buffer[..n]);
        }
        loop {
            if stopped() {
                return Ok(SourceCheck::unchecked(
                    State::BudgetExhausted,
                    bytes_read,
                    "deadline_or_cancellation",
                ));
            }
            match held.file.read(&mut buffer[..1]) {
                Ok(0) => break,
                Ok(n) => {
                    return Ok(SourceCheck::unchecked(
                        State::ChangedDuringCheck,
                        bytes_read + n as u64,
                        "source_length_changed",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        if stopped() {
            return Ok(SourceCheck::unchecked(
                State::BudgetExhausted,
                bytes_read,
                "deadline_or_cancellation",
            ));
        }
        let after = held.file.metadata()?;
        if before.len() != after.len()
            || before.modified()? != after.modified()?
            || !unchanged(&before, &after)
            || original_identity != identity(&held.file)?
        {
            return Ok(SourceCheck::unchecked(
                State::ChangedDuringCheck,
                bytes_read,
                "source_metadata_changed",
            ));
        }
        let current = match open(root, relative) {
            Ok(current) => current,
            Err(_) => {
                return Ok(SourceCheck::unchecked(
                    State::ChangedDuringCheck,
                    bytes_read,
                    "source_binding_changed",
                ));
            }
        };
        if original_identity != identity(&current.file)?
            || held._directories.len() != current._directories.len()
            || held
                ._directories
                .iter()
                .zip(&current._directories)
                .any(|(before, after)| {
                    identity(before)
                        .ok()
                        .zip(identity(after).ok())
                        .is_none_or(|(before, after)| before != after)
                })
        {
            return Ok(SourceCheck::unchecked(
                State::ChangedDuringCheck,
                bytes_read,
                "source_binding_changed",
            ));
        }
        // Reopened handles still pin the binding. A second observation catches
        // detectable writes during pathname validation on platforms permitting them.
        let final_metadata = current.file.metadata()?;
        if after.len() != final_metadata.len()
            || after.modified()? != final_metadata.modified()?
            || !unchanged(&after, &final_metadata)
        {
            return Ok(SourceCheck::unchecked(
                State::ChangedDuringCheck,
                bytes_read,
                "source_metadata_changed",
            ));
        }
        if stopped() {
            return Ok(SourceCheck::unchecked(
                State::BudgetExhausted,
                bytes_read,
                "deadline_or_cancellation",
            ));
        }
        let observed = airk_core::identity::Sha256Digest::from_bytes(hasher.finalize().into());
        Ok(SourceCheck {
            state: if observed == expected {
                State::Matched
            } else {
                State::Mismatch
            },
            observed_hash: Some(observed),
            bytes_read,
            reason: None,
        })
    })();
    attempt.unwrap_or_else(|error| {
        let state = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<std::io::Error>())
            .map_or(State::Unsupported, |cause| {
                if cause.kind() == std::io::ErrorKind::NotFound {
                    State::Missing
                } else {
                    State::Unreadable
                }
            });
        SourceCheck::unchecked(state, bytes_read, "protected_source_capture_failed")
    })
}

/// Capture an explicit administrative input using one authorized canonical path.
/// Authorization runs before resolution and again on the exact resolved path
/// passed to protected traversal; no subsequent resolution may redirect capture.
pub fn capture_input(
    path: &Path,
    limit: usize,
    require_single_link: bool,
    authorize: &dyn Fn(&Path) -> Result<()>,
) -> Result<(std::path::PathBuf, Vec<u8>)> {
    ensure!(path.is_absolute(), "administrative input must be absolute");
    authorize(path)?;
    let canonical = path.canonicalize()?;
    authorize(&canonical)?;
    let bytes = read_protected_path(&canonical, limit, require_single_link)?;
    Ok((canonical, bytes))
}

/// Read an already selected absolute path without resolving any redirects.
/// Callers authorize this exact spelling; every component is opened no-follow.
pub fn read_protected_path(
    path: &Path,
    limit: usize,
    require_single_link: bool,
) -> Result<Vec<u8>> {
    ensure!(path.is_absolute(), "protected input must be absolute");
    let parent = path.parent().context("input has no parent")?;
    let bytes = read_with_alias_policy(
        parent,
        path,
        limit,
        limit.saturating_add(1),
        require_single_link,
    )?;
    ensure!(
        bytes.len() <= limit,
        "administrative input exceeds byte limit"
    );
    Ok(bytes)
}

/// Observe absence through the same no-follow pinned traversal as source
/// capture. Permission/type/redirect errors are uncertainty, never absence.
/// Does not enumerate directories or read bytes from an existing file.
pub(crate) fn missing_protected_path(path: &Path) -> Result<bool> {
    ensure!(
        path.is_absolute(),
        "protected missing locator must be absolute"
    );
    let root = path
        .ancestors()
        .last()
        .context("protected missing locator has no root")?;
    let relative = path.strip_prefix(root)?;
    match open(root, relative) {
        Ok(_) => Ok(false),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn read(root: &Path, path: &Path, limit: usize, remaining: usize) -> Result<Vec<u8>> {
    read_with_alias_policy(root, path, limit, remaining, false)
}

fn read_with_alias_policy(
    root: &Path,
    path: &Path,
    limit: usize,
    remaining: usize,
    require_single_link: bool,
) -> Result<Vec<u8>> {
    let relative = path
        .strip_prefix(root)
        .context("source escaped workspace")?;
    ensure!(
        relative
            .components()
            .all(|part| matches!(part, Component::Normal(_))),
        "invalid source path"
    );
    let mut held = open(root, relative)?;
    let before = held.file.metadata()?;
    ensure!(before.is_file(), "source is not a regular file");
    if require_single_link {
        require_unaliased(&held.file)?;
    }
    let mut bytes = Vec::new();
    held.file
        .by_ref()
        .take(limit.saturating_add(1).min(remaining) as u64)
        .read_to_end(&mut bytes)?;
    let after = held.file.metadata()?;
    ensure!(
        before.len() == after.len()
            && before.modified()? == after.modified()?
            && unchanged(&before, &after),
        "source changed during scan; retry indexing"
    );
    // Check the pathname still names the held file, using the same protected traversal.
    let current = open(root, relative)?;
    ensure!(
        identity(&held.file)? == identity(&current.file)?,
        "source replaced during scan; retry indexing"
    );
    ensure!(
        bytes.len() as u64 == before.len() || bytes.len() > limit,
        "workspace raw source read budget exhausted; previous generation retained"
    );
    Ok(bytes)
}

#[cfg(unix)]
fn require_unaliased(file: &File) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    ensure!(file.metadata()?.nlink() == 1, "aliased runtime file");
    Ok(())
}

#[cfg(windows)]
fn require_unaliased(file: &File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    // SAFETY: this native output structure admits zero initialization.
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the owned file handle and initialized output outlive the call.
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut information) } != 0,
        "cannot inspect runtime file aliases"
    );
    ensure!(information.nNumberOfLinks == 1, "aliased runtime file");
    Ok(())
}

struct Opened {
    file: File,
    // Windows directory handles deny deletion/rename while the source is read.
    // Unix openat traversal pins directory identities directly through handles.
    _directories: Vec<File>,
}

#[cfg(unix)]
fn open(root: &Path, relative: &Path) -> Result<Opened> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::OpenOptionsExt},
        },
    };
    ensure!(root.is_absolute(), "source root must be absolute");
    let mut directories = vec![
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")?,
    ];
    let joined = root.join(relative);
    let components: Vec<_> = joined
        .components()
        .filter(|part| !matches!(part, Component::RootDir))
        .collect();
    for (index, part) in components.iter().enumerate() {
        let Component::Normal(name) = part else {
            anyhow::bail!("invalid source traversal");
        };
        let name = CString::new(name.as_bytes())?;
        let directory = index + 1 != components.len();
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if directory { libc::O_DIRECTORY } else { 0 };
        // SAFETY: parent and name remain valid; successful openat returns an owned descriptor.
        let fd = unsafe {
            libc::openat(
                directories.last().unwrap().as_raw_fd(),
                name.as_ptr(),
                flags,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error())
                .context("open source without following links");
        }
        let file = unsafe { File::from_raw_fd(fd) };
        if directory {
            directories.push(file);
        } else {
            return Ok(Opened {
                file,
                _directories: directories,
            });
        }
    }
    anyhow::bail!("source file path is empty")
}

#[cfg(windows)]
fn open(root: &Path, relative: &Path) -> Result<Opened> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    ensure!(root.is_absolute(), "source root must be absolute");
    let joined = root.join(relative);
    let parts: Vec<_> = joined.components().collect();
    let mut current = std::path::PathBuf::new();
    let mut directories = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        ensure!(
            !matches!(part, Component::ParentDir | Component::CurDir),
            "invalid source traversal"
        );
        current.push(part.as_os_str());
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let directory = index + 1 != parts.len();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | if directory { FILE_SHARE_WRITE } else { 0 })
            .custom_flags(
                FILE_FLAG_OPEN_REPARSE_POINT
                    | if directory {
                        FILE_FLAG_BACKUP_SEMANTICS
                    } else {
                        0
                    },
            )
            .open(&current)
            .context("open source through pinned directories")?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "source traversal encountered a reparse point"
        );
        ensure!(
            if directory {
                metadata.is_dir()
            } else {
                metadata.is_file()
            },
            "source traversal changed type"
        );
        if directory {
            directories.push(file);
        } else {
            return Ok(Opened {
                file,
                _directories: directories,
            });
        }
    }
    anyhow::bail!("source file path is empty")
}

#[cfg(unix)]
fn identity(file: &File) -> Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn identity(file: &File) -> Result<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: valid owned handle and initialized writable information buffer.
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } != 0,
        "read source handle identity: {}",
        std::io::Error::last_os_error()
    );
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    ))
}

#[cfg(unix)]
fn unchanged(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    before.ctime() == after.ctime() && before.ctime_nsec() == after.ctime_nsec()
}
#[cfg(windows)]
fn unchanged(_: &std::fs::Metadata, _: &std::fs::Metadata) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn administrative_capture_keeps_exact_identity_bounds_and_link_policy() -> Result<()> {
        use std::cell::RefCell;
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?;
        let path = root.join("input.json");
        std::fs::write(&path, b"original input")?;
        let admitted = RefCell::new(Vec::new());
        let (canonical, bytes) = capture_input(&path, 14, true, &|candidate| {
            admitted.borrow_mut().push(candidate.to_path_buf());
            Ok(())
        })?;
        assert_eq!(canonical, path);
        assert_eq!(bytes, b"original input");
        assert_eq!(admitted.into_inner(), vec![path.clone(), path.clone()]);
        assert!(capture_input(&path, 13, true, &|_| Ok(())).is_err());
        let alias = root.join("alias.json");
        std::fs::hard_link(&path, &alias)?;
        assert!(capture_input(&path, 14, true, &|_| Ok(())).is_err());
        assert_eq!(read_protected_path(&path, 14, false)?, b"original input");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn administrative_capture_refuses_redirects_installed_after_canonical_admission() -> Result<()>
    {
        use std::{cell::Cell, os::unix::fs::symlink};
        for replace_parent in [false, true] {
            let owner = tempfile::tempdir()?;
            let root = owner.path().canonicalize()?;
            let allowed = root.join("allowed");
            let denied = root.join("denied");
            std::fs::create_dir(&allowed)?;
            std::fs::create_dir(&denied)?;
            let input = allowed.join("input.json");
            std::fs::write(&input, b"allowed input")?;
            std::fs::write(denied.join("input.json"), b"denied content")?;
            let checks = Cell::new(0);
            let result = capture_input(&input, 100, false, &|candidate| {
                ensure!(!candidate.starts_with(&denied), "denied input");
                checks.set(checks.get() + 1);
                if checks.get() == 2 {
                    if replace_parent {
                        std::fs::rename(&allowed, root.join("retained-allowed"))?;
                        symlink(&denied, &allowed)?;
                    } else {
                        std::fs::remove_file(&input)?;
                        symlink(denied.join("input.json"), &input)?;
                    }
                }
                Ok(())
            });
            assert!(result.is_err(), "redirected parent={replace_parent}");
            assert_eq!(checks.get(), 2);
            assert_eq!(std::fs::read(denied.join("input.json"))?, b"denied content");
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn administrative_capture_checks_resolved_target_before_reading() -> Result<()> {
        use std::{cell::Cell, os::unix::fs::symlink};
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?;
        let denied = root.join("denied.json");
        let allowed = root.join("allowed.json");
        std::fs::write(&denied, b"denied content")?;
        symlink(&denied, &allowed)?;
        let checks = Cell::new(0);
        assert!(
            capture_input(&allowed, 100, false, &|candidate| {
                checks.set(checks.get() + 1);
                ensure!(candidate != denied.as_path(), "denied input");
                Ok(())
            })
            .is_err()
        );
        assert_eq!(checks.get(), 2);
        Ok(())
    }

    #[test]
    fn repository_marker_reads_are_fixed_name_shallow_and_bounded() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?;
        std::fs::create_dir(root.join("child"))?;
        std::fs::write(root.join(".git"), "gitdir: outside\n")?;
        std::fs::write(root.join("child/.git"), vec![b'x'; 9000])?;
        assert_eq!(
            read_git_marker(&root, Path::new(".git"))?,
            b"gitdir: outside\n"
        );
        assert_eq!(read_git_marker(&root, Path::new("child/.git"))?.len(), 4097);
        for invalid in ["secret.txt", "../.git", "child/deeper/.git"] {
            assert!(read_git_marker(&root, Path::new(invalid)).is_err());
        }
        assert!(read_git_marker(&root, &root.join(".git")).is_err());
        assert_eq!(std::fs::metadata(root.join("child/.git"))?.len(), 9000);
        Ok(())
    }

    #[test]
    fn verification_hashes_complete_raw_sources_and_keeps_failures_unchecked() -> Result<()> {
        use airk_core::{identity::Sha256Digest, verification::SourceCheckState as State};
        use sha2::{Digest, Sha256};
        use std::time::{Duration, Instant};
        let source = tempfile::tempdir()?;
        let root = source.path().canonicalize()?;
        // Raw binary across several streaming buffers; no parser or text normalization.
        let bytes: Vec<u8> = (0..196_619).map(|n| (n % 251) as u8).collect();
        let expected = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
        std::fs::write(root.join("raw.bin"), &bytes)?;
        let deadline = || Instant::now() + Duration::from_secs(30);
        let matched = verify_saved_source(
            &root,
            Path::new("raw.bin"),
            expected,
            bytes.len() as u64 + 1,
            deadline(),
            &|| false,
        );
        assert_eq!(matched.state, State::Matched);
        assert_eq!(matched.observed_hash, Some(expected));
        assert_eq!(matched.bytes_read, bytes.len() as u64);
        let mismatch = verify_saved_source(
            &root,
            Path::new("raw.bin"),
            Sha256Digest::from_bytes([0; 32]),
            bytes.len() as u64 + 1,
            deadline(),
            &|| false,
        );
        assert_eq!(mismatch.state, State::Mismatch);
        assert_eq!(mismatch.observed_hash, Some(expected));
        let bounded = verify_saved_source(
            &root,
            Path::new("raw.bin"),
            expected,
            bytes.len() as u64 - 1,
            deadline(),
            &|| false,
        );
        assert_eq!(bounded.state, State::BudgetExhausted);
        assert_eq!(bounded.bytes_read, 0);
        assert!(bounded.observed_hash.is_none());
        let exact = verify_saved_source(
            &root,
            Path::new("raw.bin"),
            expected,
            bytes.len() as u64,
            deadline(),
            &|| false,
        );
        assert_eq!(
            exact.state,
            State::BudgetExhausted,
            "full EOF evidence needs one byte of prospective headroom"
        );
        assert_eq!(exact.bytes_read, 0);
        std::fs::write(root.join("empty.bin"), b"")?;
        let empty_hash = Sha256Digest::from_bytes(Sha256::digest(b"").into());
        let empty = verify_saved_source(
            &root,
            Path::new("empty.bin"),
            empty_hash,
            1,
            deadline(),
            &|| false,
        );
        assert_eq!(empty.state, State::Matched);
        assert_eq!(empty.bytes_read, 0);
        let missing = verify_saved_source(
            &root,
            Path::new("missing.bin"),
            expected,
            10,
            deadline(),
            &|| false,
        );
        assert_eq!(missing.state, State::Missing);
        let expired = verify_saved_source(
            &root,
            Path::new("raw.bin"),
            expected,
            u64::MAX,
            Instant::now(),
            &|| false,
        );
        assert_eq!(expired.state, State::BudgetExhausted);
        assert_eq!(expired.bytes_read, 0);
        for path in [
            "../outside.bin",
            "/outside.bin",
            "a//b",
            "a/./b",
            "a/../b",
            "raw.bin:stream",
            "a\\b",
            "raw.bin/",
        ] {
            let result = verify_saved_source(
                &root,
                Path::new(path),
                expected,
                u64::MAX,
                deadline(),
                &|| false,
            );
            assert_eq!(result.state, State::OutOfScope, "{path}");
            assert_eq!(result.bytes_read, 0);
        }
        assert_eq!(std::fs::read(root.join("raw.bin"))?, bytes);
        Ok(())
    }

    #[test]
    fn verification_cancellation_stops_between_buffers_without_a_partial_hash() -> Result<()> {
        use airk_core::{
            identity::Sha256Digest,
            verification::{STREAM_BYTES, SourceCheckState as State},
        };
        use std::{
            cell::Cell,
            time::{Duration, Instant},
        };
        let source = tempfile::tempdir()?;
        let root = source.path().canonicalize()?;
        std::fs::write(root.join("large.bin"), vec![1; STREAM_BYTES * 3])?;
        let checks = Cell::new(0);
        let result = verify_saved_source(
            &root,
            Path::new("large.bin"),
            Sha256Digest::from_bytes([0; 32]),
            u64::MAX,
            Instant::now() + Duration::from_secs(30),
            &|| {
                checks.set(checks.get() + 1);
                checks.get() >= 3
            },
        );
        assert_eq!(result.state, State::BudgetExhausted);
        assert_eq!(result.bytes_read, STREAM_BYTES as u64);
        assert_eq!(result.observed_hash, None);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn verification_rejects_links_and_detects_writes_during_a_check() -> Result<()> {
        use airk_core::{
            identity::Sha256Digest,
            verification::{STREAM_BYTES, SourceCheckState as State},
        };
        use std::{
            cell::Cell,
            time::{Duration, Instant},
        };
        let source = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let root = source.path().canonicalize()?;
        std::fs::write(outside.path().join("secret.bin"), b"outside")?;
        std::os::unix::fs::symlink(outside.path(), root.join("redirect"))?;
        let no_link = verify_saved_source(
            &root,
            Path::new("redirect/secret.bin"),
            Sha256Digest::from_bytes([0; 32]),
            u64::MAX,
            Instant::now() + Duration::from_secs(30),
            &|| false,
        );
        assert!(!matches!(no_link.state, State::Matched | State::Mismatch));
        assert_eq!(no_link.bytes_read, 0);
        std::fs::write(root.join("large.bin"), vec![1; STREAM_BYTES * 3])?;
        let checks = Cell::new(0);
        let raced = verify_saved_source(
            &root,
            Path::new("large.bin"),
            Sha256Digest::from_bytes([0; 32]),
            u64::MAX,
            Instant::now() + Duration::from_secs(30),
            &|| {
                checks.set(checks.get() + 1);
                if checks.get() == 3 {
                    std::fs::write(root.join("large.bin"), b"short").unwrap();
                }
                false
            },
        );
        assert_eq!(raced.state, State::ChangedDuringCheck);
        assert!(raced.observed_hash.is_none());
        Ok(())
    }

    #[test]
    fn regular_sources_are_bounded_and_ancestors_cannot_redirect_reads() -> Result<()> {
        let source = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let root = source.path().canonicalize()?;
        std::fs::write(root.join("safe.txt"), b"source")?;
        std::fs::write(outside.path().join("secret.txt"), b"not source")?;
        assert_eq!(read(&root, &root.join("safe.txt"), 16, 17)?, b"source");
        assert!(read(&root, &root.join("safe.txt"), 16, 3).is_err());
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.join("redirected"))?;
        #[cfg(windows)]
        {
            // Junctions need no symlink privilege. The command receives only fixture paths.
            let status = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(root.join("redirected"))
                .arg(outside.path())
                .output()?;
            ensure!(status.status.success(), "junction fixture creation failed");
        }
        assert!(read(&root, &root.join("redirected/secret.txt"), 16, 17).is_err());
        assert_eq!(
            std::fs::read(outside.path().join("secret.txt"))?,
            b"not source"
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn held_windows_source_denies_write_and_directory_replacement() -> Result<()> {
        let source = tempfile::tempdir()?;
        let root = source.path().canonicalize()?;
        std::fs::create_dir(root.join("nested"))?;
        std::fs::write(root.join("nested/source.txt"), b"stable")?;
        let held = open(&root, Path::new("nested/source.txt"))?;
        assert!(std::fs::write(root.join("nested/source.txt"), b"changed").is_err());
        assert!(std::fs::rename(root.join("nested"), root.join("replaced")).is_err());
        drop(held);
        std::fs::write(root.join("nested/source.txt"), b"changed")?;
        use std::os::windows::fs::OpenOptionsExt;
        let writer = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(root.join("nested/source.txt"))?;
        let check = verify_saved_source(
            &root,
            Path::new("nested/source.txt"),
            airk_core::identity::Sha256Digest::from_bytes([0; 32]),
            1024,
            std::time::Instant::now() + std::time::Duration::from_secs(1),
            &|| false,
        );
        assert_eq!(
            check.state,
            airk_core::verification::SourceCheckState::Unreadable
        );
        assert_eq!(check.bytes_read, 0);
        drop(writer);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn held_unix_source_does_not_follow_replaced_path() -> Result<()> {
        let source = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let root = source.path().canonicalize()?;
        std::fs::write(root.join("source.txt"), b"stable")?;
        std::fs::write(outside.path().join("secret.txt"), b"outside")?;
        let mut held = open(&root, Path::new("source.txt"))?;
        std::fs::rename(root.join("source.txt"), root.join("original.txt"))?;
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), root.join("source.txt"))?;
        let mut bytes = Vec::new();
        held.file.read_to_end(&mut bytes)?;
        assert_eq!(bytes, b"stable");
        assert!(open(&root, Path::new("source.txt")).is_err());
        Ok(())
    }
}
