// SPDX-License-Identifier: Apache-2.0
use super::ObjectIdentity;
use std::{fs::File, io};

/// Exact raw delete-pending facts. This grants no ACL, record or content policy.
/// Only the owning removal adapter can mint it; it cannot be cloned or decoded.
pub struct PendingFileProof {
    identity: ObjectIdentity,
}
impl PendingFileProof {
    #[cfg(windows)]
    pub(super) fn new(file: &File, identity: ObjectIdentity) -> io::Result<Self> {
        let proof = Self { identity };
        proof.validate(file)?;
        Ok(proof)
    }
    pub fn validate(&self, file: &File) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_REPARSE_POINT, FILE_STANDARD_INFO, FileStandardInfo,
                GetFileInformationByHandleEx,
            };
            let mut standard = FILE_STANDARD_INFO::default();
            // SAFETY: live owned handle, initialized exact-sized native output.
            if unsafe {
                GetFileInformationByHandleEx(
                    file.as_raw_handle().cast(),
                    FileStandardInfo,
                    (&mut standard as *mut FILE_STANDARD_INFO).cast(),
                    std::mem::size_of::<FILE_STANDARD_INFO>() as u32,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            let metadata = file.metadata()?;
            if super::identity(file)? != self.identity
                || !standard.DeletePending
                || standard.NumberOfLinks != 0
                || standard.Directory
                || !metadata.is_file()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                return Err(io::Error::other(
                    "pending file identity, state or link count changed",
                ));
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = (file, &self.identity);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "pending file proof is Windows-only",
            ))
        }
    }
}
