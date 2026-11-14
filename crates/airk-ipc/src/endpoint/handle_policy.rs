// SPDX-License-Identifier: Apache-2.0
//! Privacy policy on an already-open object. Callers own no-follow traversal,
//! native identity, continued ancestor pins and publication/durability proof.
use anyhow::{Result, ensure};
use std::fs::File;

#[derive(Clone, Copy)]
enum Policy {
    PrivateDirectory,
    PrivateMetadata,
    #[cfg(windows)]
    PrivatePendingMetadata,
    ProtectedParent,
}

pub fn require_private_directory_handle(file: &File) -> Result<()> {
    require(file, Policy::PrivateDirectory)
}
pub fn require_private_metadata_handle(file: &File) -> Result<()> {
    require(file, Policy::PrivateMetadata)
}
#[cfg(windows)]
pub fn require_private_pending_metadata_handle(
    file: &File,
    proof: &airk_process::directory::PendingFileProof,
) -> Result<()> {
    proof.validate(file)?;
    require(file, Policy::PrivatePendingMetadata)
}

pub fn require_protected_parent_handle(file: &File) -> Result<()> {
    require(file, Policy::ProtectedParent)
}

fn require(file: &File, policy: Policy) -> Result<()> {
    let metadata = file.metadata()?;
    ensure!(
        match policy {
            Policy::PrivateMetadata => metadata.is_file(),
            #[cfg(windows)]
            Policy::PrivatePendingMetadata => metadata.is_file(),
            _ => metadata.is_dir(),
        },
        "private policy object has wrong type"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: scalar effective-UID observation has no pointers/failure mode.
        let user = unsafe { libc::geteuid() };
        let mode = metadata.mode();
        match policy {
            Policy::PrivateDirectory => ensure!(
                metadata.uid() == user && mode & 0o777 == 0o700,
                "directory is not owner-private"
            ),
            Policy::PrivateMetadata => ensure!(
                metadata.uid() == user && metadata.nlink() == 1 && mode & 0o777 == 0o600,
                "metadata file is not owner-private"
            ),
            Policy::ProtectedParent => ensure!(
                (metadata.uid() == user || metadata.uid() == 0)
                    && (mode & 0o022 == 0 || metadata.uid() == 0 && mode & 0o1000 != 0),
                "parent permits foreign mutation"
            ),
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, GetFileInformationByHandle,
        };
        ensure!(
            metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "redirected private policy object"
        );
        if matches!(policy, Policy::PrivateMetadata) {
            let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            // SAFETY: the live owned file and initialized fixed output are valid.
            ensure!(
                unsafe {
                    GetFileInformationByHandle(file.as_raw_handle().cast(), &mut information)
                } != 0,
                "cannot inspect private metadata aliases"
            );
            ensure!(information.nNumberOfLinks == 1, "aliased private metadata");
        }
        windows::require(file, policy)
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{ffi::c_void, io, mem, os::windows::io::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            ACCESS_ALLOWED_ACE,
            Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_FILE_OBJECT},
            DACL_SECURITY_INFORMATION, GetAce, OWNER_SECURITY_INFORMATION,
        },
        System::SystemServices::{ACCESS_ALLOWED_ACE_TYPE, ACCESS_DENIED_ACE_TYPE},
    };

    struct Allocation(*mut c_void);
    impl Drop for Allocation {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }

    fn sid_string(sid: *mut c_void) -> Result<String> {
        let mut text = std::ptr::null_mut();
        // SAFETY: SID belongs to a successful live OS security descriptor;
        // writable output receives an OS allocation, freed after conversion.
        ensure!(
            unsafe { ConvertSidToStringSidW(sid, &mut text) } != 0,
            "cannot inspect private owner SID"
        );
        let _allocation = Allocation(text.cast());
        let mut length = 0;
        // SAFETY: the successful conversion returns a terminated UTF-16 string.
        unsafe {
            while *text.add(length) != 0 {
                length += 1;
            }
        }
        Ok(String::from_utf16(unsafe {
            std::slice::from_raw_parts(text, length)
        })?)
    }

    pub(super) fn require(file: &File, policy: Policy) -> Result<()> {
        let user = crate::endpoint::current_user()?;
        let (mut descriptor, mut owner, mut dacl) = (
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        // SAFETY: live READ_CONTROL handle and writable output pointers. All
        // returned pointers remain inside the OS allocation until it is freed.
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle().cast(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32).into());
        }
        let _descriptor = Allocation(descriptor);
        ensure!(
            !owner.is_null() && !dacl.is_null(),
            "private object has unrestricted security"
        );
        let owner = sid_string(owner)?;
        let parent = matches!(policy, Policy::ProtectedParent);
        ensure!(
            owner == user || parent && matches!(owner.as_str(), "S-1-5-18" | "S-1-5-32-544"),
            "private object has a foreign owner"
        );
        // SAFETY: GetSecurityInfo returned a nonnull ACL in the live descriptor.
        let count = unsafe { (*dacl).AceCount };
        for index in 0..u32::from(count) {
            let mut ace = std::ptr::null_mut();
            // SAFETY: index is inside the native ACL; output pointer lives here.
            ensure!(
                unsafe { GetAce(dacl, index, &mut ace) } != 0 && !ace.is_null(),
                "invalid private ACL"
            );
            // SAFETY: successful GetAce returns an ACE with a valid header.
            let header = unsafe { &*ace.cast::<windows_sys::Win32::Security::ACE_HEADER>() };
            if header.AceType == ACCESS_DENIED_ACE_TYPE as u8 {
                continue;
            }
            ensure!(
                header.AceType == ACCESS_ALLOWED_ACE_TYPE as u8
                    && header.AceSize as usize >= mem::size_of::<ACCESS_ALLOWED_ACE>(),
                "unsupported private ACL entry"
            );
            // SAFETY: native ACE type and minimum layout were checked above.
            let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
            let principal = sid_string(std::ptr::addr_of!(allowed.SidStart).cast_mut().cast())?;
            if parent {
                ensure!(
                    !crate::endpoint::foreign_write_grant(&principal, &user, allowed.Mask),
                    "parent grants foreign mutation"
                );
            } else {
                ensure!(
                    allowed.Mask == 0
                        || principal == user
                        || matches!(principal.as_str(), "S-1-5-18" | "S-1-5-32-544"),
                    "private object grants foreign access"
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
