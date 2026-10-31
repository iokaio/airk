// SPDX-License-Identifier: Apache-2.0
//! Bounded Windows physical enumeration, never an eligibility constructor.
use crate::private_storage::{self, Kind, Observation};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    ffi::c_void,
    fs::File,
    mem,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Component, Path, PathBuf, Prefix},
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ID_BOTH_DIR_INFO, FILE_STREAM_INFO,
    GetFileInformationByHandle, GetFileInformationByHandleEx,
};

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}
#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root: *mut c_void,
    name: *mut UnicodeString,
    attributes: u32,
    descriptor: *mut c_void,
    qos: *mut c_void,
}
#[repr(C)]
struct IoStatus {
    status_or_pointer: usize,
    information: usize,
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtCreateFile(
        handle: *mut *mut c_void,
        access: u32,
        attributes: *mut ObjectAttributes,
        status: *mut IoStatus,
        allocation: *mut i64,
        file_attributes: u32,
        share: u32,
        disposition: u32,
        options: u32,
        ea: *mut c_void,
        ea_length: u32,
    ) -> i32;
}
fn info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
    let mut result = mem::MaybeUninit::uninit();
    // SAFETY: valid owned handle and correctly sized writable output.
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), result.as_mut_ptr()) } != 0,
        "Windows census identity unavailable"
    );
    Ok(unsafe { result.assume_init() })
}
fn file_id(file: &File) -> Result<u64> {
    let info = info(file)?;
    Ok((u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow))
}
fn valid_component(name: &[u16]) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != [46]
        && name != [46, 46]
        && !name.iter().any(|c| [0, 47, 92, 58, 42, 63].contains(c))
}
fn relative(parent: &File, name: &[u16]) -> Result<File> {
    ensure!(
        valid_component(name),
        "unsupported Windows census component"
    );
    native_open(Some(parent), name)
}
fn native_open(parent: Option<&File>, name: &[u16]) -> Result<File> {
    ensure!(
        name.len() * 2 <= u16::MAX as usize,
        "Windows census native name exceeds bound"
    );
    let mut name = name.to_vec();
    let mut unicode = UnicodeString {
        length: (name.len() * 2) as u16,
        maximum_length: (name.len() * 2) as u16,
        buffer: name.as_mut_ptr(),
    };
    let mut attributes = ObjectAttributes {
        length: mem::size_of::<ObjectAttributes>() as u32,
        root: parent.map_or(std::ptr::null_mut(), AsRawHandle::as_raw_handle),
        name: &mut unicode,
        attributes: 0,
        descriptor: std::ptr::null_mut(),
        qos: std::ptr::null_mut(),
    };
    let mut status = IoStatus {
        status_or_pointer: 0,
        information: 0,
    };
    let mut handle = std::ptr::null_mut();
    // SAFETY: parent remains owned; all bounded relative name/output structures
    // remain live. FILE_OPEN never creates; OPEN_REPARSE_POINT avoids traversal.
    let result = unsafe {
        NtCreateFile(
            &mut handle,
            0x0012_0081,
            &mut attributes,
            &mut status,
            std::ptr::null_mut(),
            0,
            3,           /* READ|WRITE, deliberately no DELETE sharing */
            1,           /* FILE_OPEN */
            0x0020_0020, /* OPEN_REPARSE_POINT|SYNCHRONOUS_NONALERT */
            std::ptr::null_mut(),
            0,
        )
    };
    ensure!(
        result >= 0 && !handle.is_null(),
        "Windows relative census open refused ({result:#x})"
    );
    // SAFETY: successful NtCreateFile transferred one real owned handle.
    let file = unsafe { File::from_raw_handle(handle) };
    let metadata = info(&file)?;
    ensure!(
        metadata.dwFileAttributes & (0x400 | 0x40) == 0,
        "Windows census refuses reparse/device object"
    );
    Ok(file)
}
struct AbsoluteRoot {
    file: File,
    _ancestors: Vec<File>,
}
fn absolute(path: &Path) -> Result<AbsoluteRoot> {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        anyhow::bail!("census requires local absolute path")
    };
    let disk = match prefix.kind() {
        Prefix::Disk(disk) | Prefix::VerbatimDisk(disk) => disk,
        _ => anyhow::bail!("unsupported Windows census root namespace"),
    };
    ensure!(
        matches!(components.next(), Some(Component::RootDir)),
        "census root is not absolute"
    );
    // This is only the drive-root anchor. Every caller component below is opened
    // relative to an actual pinned parent, never concatenated into an open path.
    let anchor = format!("\\??\\{}:\\", char::from(disk))
        .encode_utf16()
        .collect::<Vec<_>>();
    let mut parent = native_open(None, &anchor)?;
    ensure!(
        info(&parent)?.dwFileAttributes & 0x400 == 0,
        "redirected volume anchor"
    );
    ensure!(
        private_storage::same_volume(&parent, &parent),
        "Windows census anchor is not supported local storage"
    );
    let mut ancestors = Vec::new();
    for component in components {
        ensure!(
            ancestors.len() < 64,
            "Windows census ancestry bound exceeded"
        );
        let Component::Normal(name) = component else {
            anyhow::bail!("noncanonical census ancestry")
        };
        let child = relative(&parent, &name.encode_wide().collect::<Vec<_>>())?;
        ensure!(
            child.metadata()?.is_dir(),
            "Windows census ancestor is not directory"
        );
        ancestors.push(parent);
        parent = child;
    }
    Ok(AbsoluteRoot {
        file: parent,
        _ancestors: ancestors,
    })
}

struct Entry {
    name: Vec<u16>,
    id: u64,
    attributes: u32,
    special: bool,
}
fn open_entry(parent: &File, entry: &Entry) -> Result<File> {
    ensure!(
        entry.attributes & (0x400 | 0x40) == 0,
        "Windows census reparse/device entry"
    );
    let child = relative(parent, &entry.name)?;
    ensure!(
        file_id(&child)? == entry.id,
        "Windows census entry replaced before open"
    );
    Ok(child)
}
struct Cursor {
    buffer: Box<[u64; 512]>,
    offset: Option<usize>,
    restart: bool,
    ended: bool,
    dots: usize,
}
impl Cursor {
    fn new() -> Self {
        Self {
            buffer: Box::new([0; 512]),
            offset: None,
            restart: true,
            ended: false,
            dots: 0,
        }
    }
    fn next(&mut self, parent: &File) -> Result<Option<Entry>> {
        if self.ended {
            return Ok(None);
        }
        if self.offset.is_none() {
            self.buffer.fill(0);
            // SAFETY: aligned bounded writable buffer and owned directory.
            let ok = unsafe {
                GetFileInformationByHandleEx(
                    parent.as_raw_handle(),
                    if self.restart { 11 } else { 10 },
                    self.buffer.as_mut_ptr().cast(),
                    4096,
                )
            };
            if ok == 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(18) {
                    self.ended = true;
                    return Ok(None);
                }
                return Err(error).context("Windows directory census did not reach exact end");
            }
            self.restart = false;
            self.offset = Some(0);
        }
        let offset = self.offset.take().expect("buffer loaded");
        // SAFETY: complete initialized fixed buffer; each field is bounds-checked below.
        let bytes = unsafe { std::slice::from_raw_parts(self.buffer.as_ptr().cast::<u8>(), 4096) };
        let field = |at: usize| -> Result<u32> {
            let value = bytes
                .get(offset + at..offset + at + 4)
                .context("truncated census field")?;
            Ok(u32::from_ne_bytes(value.try_into()?))
        };
        let next = field(mem::offset_of!(FILE_ID_BOTH_DIR_INFO, NextEntryOffset))? as usize;
        let attributes = field(mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileAttributes))?;
        let length = field(mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength))? as usize;
        let start = offset + mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
        let limit = if next == 0 {
            4096
        } else {
            ensure!(
                next.is_multiple_of(8)
                    && next >= mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName)
                    && offset.checked_add(next).is_some_and(|end| end < 4096),
                "invalid census continuation offset"
            );
            self.offset = Some(offset + next);
            offset + next
        };
        ensure!(
            length > 0 && length.is_multiple_of(2) && length <= 510 && start + length <= limit,
            "unsupported census name bounds"
        );
        let name = bytes[start..start + length]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|value| u16::from_ne_bytes([value[0], value[1]]))
            .collect::<Vec<_>>();
        let at = offset + mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileId);
        let id = u64::from_ne_bytes(
            bytes
                .get(at..at + 8)
                .context("truncated census file ID")?
                .try_into()?,
        );
        if name == [46] || name == [46, 46] {
            self.dots += 1;
            ensure!(self.dots <= 2, "repeated special directory entries");
            return Ok(Some(Entry {
                name,
                id,
                attributes,
                special: true,
            }));
        }
        ensure!(
            valid_component(&name) && id != 0,
            "unsupported directory entry"
        );
        Ok(Some(Entry {
            name,
            id,
            attributes,
            special: false,
        }))
    }
}
fn no_alternate_streams(file: &File) -> Result<bool> {
    let mut buffer = [0u64; 512];
    let ok = unsafe {
        GetFileInformationByHandleEx(file.as_raw_handle(), 7, buffer.as_mut_ptr().cast(), 4096)
    };
    if ok == 0 {
        return Ok(std::io::Error::last_os_error().raw_os_error() == Some(38));
    }
    let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), 4096) };
    let at = mem::offset_of!(FILE_STREAM_INFO, StreamName);
    let length_at = mem::offset_of!(FILE_STREAM_INFO, StreamNameLength);
    let length = u32::from_ne_bytes(bytes[length_at..length_at + 4].try_into()?) as usize;
    let next = u32::from_ne_bytes(bytes[..4].try_into()?);
    if next != 0 || !length.is_multiple_of(2) || at + length > 4096 {
        return Ok(false);
    }
    let name = bytes[at..at + length]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_ne_bytes([c[0], c[1]]))
        .collect::<Vec<_>>();
    Ok(name == "::$DATA".encode_utf16().collect::<Vec<_>>())
}
struct Frame {
    file: File,
    cursor: Cursor,
    count: usize,
}
pub(crate) enum Outcome {
    Pending { objects: usize },
    Observed { objects: usize },
}
/// Observed is physical enumeration only; caller still fences owner/token and
/// mutation producers. It is deliberately not named Eligible/Private/Complete.
pub(crate) struct WindowsWalk {
    root: AbsoluteRoot,
    path: PathBuf,
    identity: [u64; 3],
    frames: Vec<Frame>,
    seen: BTreeSet<[u64; 3]>,
    failed: bool,
}
impl WindowsWalk {
    pub(crate) fn begin(path: &Path) -> Result<Self> {
        let root = absolute(path)?;
        let Observation::Private(proof) =
            private_storage::inspect(&root.file, Kind::RootDirectory)?
        else {
            anyhow::bail!("Windows census root lacks positive private evidence")
        };
        let identity = proof.identity();
        ensure!(
            no_alternate_streams(&root.file)?,
            "unsupported directory streams"
        );
        let file = root.file.try_clone()?;
        let mut seen = BTreeSet::new();
        seen.insert(identity);
        Ok(Self {
            root,
            path: path.into(),
            identity,
            frames: vec![Frame {
                file,
                cursor: Cursor::new(),
                count: 0,
            }],
            seen,
            failed: false,
        })
    }
    pub(crate) fn root_identity(&self) -> [u64; 3] {
        self.identity
    }
    pub(crate) fn check_current(&self) -> Result<()> {
        ensure!(!self.failed, "Windows census has failed");
        let current = absolute(&self.path)?;
        let Observation::Private(proof) =
            private_storage::inspect(&self.root.file, Kind::RootDirectory)?
        else {
            anyhow::bail!("Windows census root privacy changed")
        };
        ensure!(
            proof.identity() == self.identity && proof.check_current(&current.file)?,
            "Windows census root namespace changed"
        );
        Ok(())
    }
    pub(crate) fn step(
        &mut self,
        maximum: usize,
        fence: impl FnMut() -> Result<()>,
    ) -> Result<Outcome> {
        ensure!(
            !self.failed,
            "Windows census already failed; restart requires fresh owner authority"
        );
        let result = self.step_inner(maximum, fence);
        if result.is_err() {
            self.failed = true;
            self.frames.clear();
        }
        result
    }
    fn step_inner(
        &mut self,
        maximum: usize,
        mut fence: impl FnMut() -> Result<()>,
    ) -> Result<Outcome> {
        ensure!(
            (1..=64).contains(&maximum),
            "invalid Windows census step bound"
        );
        fence()?;
        self.check_current()?;
        for _ in 0..maximum {
            let Some(frame) = self.frames.last_mut() else {
                fence()?;
                return Ok(Outcome::Observed {
                    objects: self.seen.len(),
                });
            };
            let Some(entry) = frame.cursor.next(&frame.file)? else {
                self.frames.pop();
                continue;
            };
            frame.count += 1;
            ensure!(
                frame.count <= 4096 && self.seen.len() < 100_000,
                "Windows census inventory bound exceeded"
            );
            if entry.special {
                continue;
            }
            let child = open_entry(&frame.file, &entry)?;
            ensure!(
                private_storage::same_volume(&self.root.file, &child),
                "Windows census volume boundary"
            );
            let directory = child.metadata()?.is_dir();
            let Observation::Private(proof) = private_storage::inspect(
                &child,
                if directory {
                    Kind::Directory
                } else {
                    Kind::File
                },
            )?
            else {
                anyhow::bail!("Windows census child lacks positive evidence")
            };
            ensure!(
                self.seen.insert(proof.identity()),
                "Windows census duplicate physical object"
            );
            ensure!(
                no_alternate_streams(&child)?,
                "Windows census refuses alternate streams"
            );
            if directory {
                ensure!(
                    self.frames.len() < 64,
                    "Windows census depth bound exceeded"
                );
                self.frames.push(Frame {
                    file: child,
                    cursor: Cursor::new(),
                    count: 0,
                });
            }
        }
        fence()?;
        Ok(Outcome::Pending {
            objects: self.seen.len(),
        })
    }
}

/// Native creation-boundary root only; never enumerates or promotes a census.
pub(crate) struct NativeRoot {
    root: AbsoluteRoot,
    path: PathBuf,
    identity: [u64; 3],
}
impl NativeRoot {
    pub(crate) fn begin(path: &Path) -> Result<Self> {
        let root = absolute(path)?;
        let proof = private_storage::native_boundary(&root.file, Kind::RootDirectory)?
            .context("native Windows boundary policy unavailable")?;
        let identity = proof.identity();
        ensure!(
            no_alternate_streams(&root.file)?,
            "native root has unsupported streams"
        );
        Ok(Self {
            root,
            path: path.into(),
            identity,
        })
    }
    pub(crate) fn require_creation_chain(&self, boundary: [u64; 3]) -> Result<()> {
        let mut found = false;
        for file in self
            .root
            ._ancestors
            .iter()
            .chain(std::iter::once(&self.root.file))
        {
            let md = info(file)?;
            let id: [u64; 3] = [
                md.dwVolumeSerialNumber.into(),
                md.nFileIndexHigh.into(),
                md.nFileIndexLow.into(),
            ];
            if id == boundary {
                found = true;
            }
            if found {
                let proof = private_storage::native_boundary(file, Kind::RootDirectory)?
                    .context("native creation ancestor has unsupported inherited grants")?;
                ensure!(
                    proof.identity()[0] == boundary[0] && proof.recheck()?,
                    "native creation boundary changed"
                );
            }
        }
        ensure!(found, "native directory is outside actual private root");
        Ok(())
    }
    pub(crate) fn identity(&self) -> Result<[u64; 3]> {
        let current = absolute(&self.path)?;
        let proof = private_storage::native_boundary(&self.root.file, Kind::RootDirectory)?
            .context("native Windows boundary policy changed")?;
        let fresh = private_storage::native_boundary(&current.file, Kind::RootDirectory)?
            .context("native Windows boundary replaced")?;
        ensure!(
            proof.recheck()?
                && proof.identity() == self.identity
                && fresh.identity() == self.identity,
            "native Windows root identity changed"
        );
        Ok(self.identity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn fixture() -> Result<(tempfile::TempDir, PathBuf)> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("private");
        private_storage::test_private_root(&root)?;
        Ok((temporary, root))
    }
    #[test]
    fn actual_enumeration_resumes_in_bounded_steps_and_reaches_exact_eof() -> Result<()> {
        let (_temporary, root) = fixture()?;
        for index in 0..130 {
            fs::write(root.join(format!("state-{index:03}")), b"inert")?;
        }
        fs::create_dir(root.join("child"))?;
        fs::write(root.join("child/nested"), b"inert nested object")?;
        let mut walk = WindowsWalk::begin(&root)?;
        let mut previous = 1;
        for _ in 0..1024 {
            match walk.step(2, || Ok(()))? {
                Outcome::Pending { objects } => {
                    assert!(objects >= previous && objects <= previous + 2);
                    previous = objects;
                }
                Outcome::Observed { objects } => {
                    assert_eq!(objects, 133);
                    return Ok(());
                }
            }
        }
        anyhow::bail!("bounded actual enumeration never reached ERROR_NO_MORE_FILES")
    }
    #[test]
    fn actual_entry_replacement_between_enumeration_and_relative_open_refuses() -> Result<()> {
        let (_temporary, path) = fixture()?;
        fs::write(path.join("state"), b"first inert file")?;
        let root = absolute(&path)?;
        let mut cursor = Cursor::new();
        let mut entry = cursor.next(&root.file)?.context("fixture entry missing")?;
        for _ in 0..2 {
            if !entry.special {
                break;
            }
            entry = cursor.next(&root.file)?.context("fixture entry missing")?;
        }
        ensure!(!entry.special, "regular fixture entry missing");
        assert_eq!(file_id(&open_entry(&root.file, &entry)?)?, entry.id);
        // Parent remains pinned; the not-yet-retained child may be replaced.
        fs::rename(path.join("state"), path.join("previous-state"))?;
        fs::write(path.join("state"), b"replacement inert file")?;
        assert!(open_entry(&root.file, &entry).is_err());
        let current = relative(&root.file, &"state".encode_utf16().collect::<Vec<_>>())?;
        assert_ne!(file_id(&current)?, entry.id);
        Ok(())
    }
    #[test]
    fn root_namespace_is_pinned_and_foreign_readable_child_refuses_stickily() -> Result<()> {
        let (temporary, root) = fixture()?;
        private_storage::test_foreign_readable_root(&root.join("foreign"))?;
        let mut walk = WindowsWalk::begin(&root)?;
        assert!(
            fs::rename(&root, temporary.path().join("renamed")).is_err(),
            "held root must deny rename admission"
        );
        assert!(walk.step(64, || Ok(())).is_err());
        assert!(
            walk.step(64, || Ok(())).is_err(),
            "failed census cannot erase its first refusal"
        );
        Ok(())
    }
    #[test]
    fn actual_hardlink_alias_and_lost_owner_fence_never_finish() -> Result<()> {
        let (_temporary, root) = fixture()?;
        fs::write(root.join("state"), b"inert")?;
        fs::hard_link(root.join("state"), root.join("alias"))?;
        let mut walk = WindowsWalk::begin(&root)?;
        assert!(walk.step(64, || Ok(())).is_err());
        let mut walk = WindowsWalk::begin(&root)?;
        assert!(
            walk.step(1, || anyhow::bail!("fixture owner fence closed"))
                .is_err()
        );
        assert!(walk.step(1, || Ok(())).is_err());
        Ok(())
    }
    #[test]
    fn alternate_stream_and_parent_component_escape_refuse() -> Result<()> {
        let (_temporary, root) = fixture()?;
        fs::write(root.join("state"), b"inert")?;
        fs::write(root.join("state:unexpected"), b"inert alternate stream")?;
        let mut walk = WindowsWalk::begin(&root)?;
        assert!(walk.step(64, || Ok(())).is_err());
        let parent = absolute(&root)?;
        for invalid in ["..", ".", "child\\outside", "state:unexpected"] {
            assert!(relative(&parent.file, &invalid.encode_utf16().collect::<Vec<_>>()).is_err());
        }
        Ok(())
    }
    #[test]
    #[ignore = "requires administrator-provisioned private root containing one junction/volume mount"]
    fn actual_mount_reparse_entry_is_refused_before_target_enumeration() -> Result<()> {
        let root = std::env::var_os("AIRK_TEST_CENSUS_MOUNT_ROOT")
            .context("set AIRK_TEST_CENSUS_MOUNT_ROOT to the approved private mount fixture")?;
        let mut walk = WindowsWalk::begin(Path::new(&root))?;
        let error = walk
            .step(64, || Ok(()))
            .err()
            .context("mount entry was traversed")?;
        assert!(format!("{error:#}").contains("reparse"));
        assert_eq!(walk.seen.len(), 1);
        Ok(())
    }
    #[test]
    #[ignore = "requires explicitly qualified Windows symlink creation privilege or Developer Mode"]
    fn actual_directory_reparse_child_never_traverses_target() -> Result<()> {
        let (temporary, root) = fixture()?;
        let target = temporary.path().join("outside");
        private_storage::test_private_root(&target)?;
        fs::write(target.join("must-not-visit"), b"inert outside object")?;
        std::os::windows::fs::symlink_dir(&target, root.join("redirect"))?;
        let mut walk = WindowsWalk::begin(&root)?;
        assert!(walk.step(64, || Ok(())).is_err());
        assert_eq!(walk.seen.len(), 1);
        Ok(())
    }
}
