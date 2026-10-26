// SPDX-License-Identifier: Apache-2.0
//! Explicit managed layout parsing; replacement incarnations keep a logical owner.
use anyhow::{Context, Result, ensure};
use std::path::Path;

pub(crate) struct ManagedPath<'a> {
    pub root: &'a Path,
    pub logical: &'a Path,
    pub namespace: &'a str,
    pub collection: &'a str,
    pub slot: Option<&'a str>,
}

pub(crate) fn validate_ancestors(directory: &Path) -> Result<()> {
    let managed = ManagedPath::parse(directory)?;
    for path in directory
        .ancestors()
        .take_while(|path| *path != managed.root)
    {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "linked managed collection ancestry"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "redirected managed collection ancestry"
            );
        }
    }
    Ok(())
}

pub(crate) fn reject_file_aliases(path: &Path, metadata: &std::fs::Metadata) -> Result<()> {
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "invalid managed file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            metadata.nlink() == 1,
            "hard-linked managed file (links={}, path={})",
            metadata.nlink(),
            path.display()
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected managed file"
        );
        let file = std::fs::File::open(path)?;
        let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        // SAFETY: the owned file handle and correctly sized output live through
        // the native call. No data/source file is opened for writing.
        ensure!(
            unsafe {
                GetFileInformationByHandle(file.as_raw_handle().cast(), information.as_mut_ptr())
            } != 0,
            "managed file identity inspection failed"
        );
        let information = unsafe { information.assume_init() };
        ensure!(
            information.nNumberOfLinks == 1,
            "hard-linked managed file (links={}, path={})",
            information.nNumberOfLinks,
            path.display()
        );
    }
    Ok(())
}

pub(crate) fn file_identity(path: &Path, metadata: &std::fs::Metadata) -> Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        Ok(format!("{}:{}", metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let _ = metadata;
        let file = std::fs::File::options()
            .read(true)
            .custom_flags(0x02000000)
            .open(path)?;
        let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        // SAFETY: valid owned handle and live correctly sized output storage.
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), info.as_mut_ptr()) }
                != 0,
            "managed file identity unavailable"
        );
        let info = unsafe { info.assume_init() };
        Ok(format!(
            "{}:{}:{}:{}:{}",
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
            info.ftCreationTime.dwHighDateTime,
            info.ftCreationTime.dwLowDateTime
        ))
    }
}

impl<'a> ManagedPath<'a> {
    pub fn parse(directory: &'a Path) -> Result<Self> {
        let parent = directory
            .parent()
            .context("managed directory parent missing")?;
        let (logical, slot) = if parent
            .file_name()
            .is_some_and(|name| name == "incarnations")
        {
            let slot = directory
                .file_name()
                .and_then(|name| name.to_str())
                .context("invalid incarnation directory")?;
            ensure!(
                slot.len() == 32
                    && slot
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "invalid incarnation directory identity"
            );
            (
                parent.parent().context("logical collection missing")?,
                Some(slot),
            )
        } else {
            (directory, None)
        };
        let namespace_path = logical.parent().context("collection namespace missing")?;
        let namespace = namespace_path
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid collection namespace")?;
        ensure!(
            matches!(namespace, "collections-1.2" | "collections-1.4")
                && (slot.is_none() || namespace == "collections-1.4"),
            "unsupported managed collection namespace"
        );
        let collection = logical
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid logical collection")?;
        crate::validate_collection(collection)?;
        Ok(Self {
            root: namespace_path
                .parent()
                .context("managed data root missing")?,
            logical,
            namespace,
            collection,
            slot,
        })
    }
}
