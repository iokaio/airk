// SPDX-License-Identifier: Apache-2.0
use super::*;
use anyhow::ensure;
use std::{ffi::c_void, mem, os::windows::io::AsRawHandle};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER,
        Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
        DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetLengthSid, GetSecurityDescriptorControl,
        GetSecurityDescriptorLength, GetTokenInformation, IsValidSid, OWNER_SECURITY_INFORMATION,
        TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetDriveTypeW, GetFileInformationByHandle,
        GetFinalPathNameByHandleW, GetVolumeInformationByHandleW,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};
struct Descriptor(*mut c_void);
impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn information(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
    let mut info = mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: owned file handle and correctly sized writable output remain live.
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } != 0,
        "private storage identity inspection failed"
    );
    Ok(unsafe { info.assume_init() })
}
// Only a handle-returned volume GUID root can reach drive classification.
// This never accepts a caller-supplied drive letter/UNC/path-prefix guess.
fn volume_guid_root(path: &[u16]) -> Option<Vec<u16>> {
    let text = String::from_utf16(path).ok()?;
    let rest = text.strip_prefix(r"\\?\Volume{")?;
    let guid = rest.get(..36)?;
    if !guid.bytes().enumerate().all(|(index, byte)| {
        if [8, 13, 18, 23].contains(&index) {
            byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    }) || !rest.get(36..)?.starts_with("}\\")
    {
        return None;
    }
    Some(text.get(..49)?.encode_utf16().chain(Some(0)).collect())
}
fn fixed_volume(root: Option<&[u16]>, drive_type: u32) -> bool {
    root.is_some() && drive_type == 3 // DRIVE_FIXED; every unknown/remote/removable type refuses.
}
pub(super) fn local_volume(file: &File) -> Option<Vec<u16>> {
    let mut path = vec![0u16; 32_768];
    // SAFETY: owned file handle and bounded writable UTF-16 output live for call.
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            path.as_mut_ptr(),
            path.len() as u32,
            1, /* VOLUME_NAME_GUID | FILE_NAME_NORMALIZED */
        )
    } as usize;
    if length == 0 || length >= path.len() {
        return None;
    }
    let root = volume_guid_root(&path[..length])?;
    // SAFETY: root is terminated and extracted only from this actual handle.
    if !fixed_volume(Some(&root), unsafe { GetDriveTypeW(root.as_ptr()) }) {
        return None;
    }
    Some(root)
}

pub(super) fn inspect(file: &File, kind: Kind) -> Result<std::result::Result<Evidence, Unknown>> {
    inspect_policy(file, kind, false)
}
pub(super) fn native_boundary(
    file: &File,
    kind: Kind,
) -> Result<std::result::Result<Evidence, Unknown>> {
    inspect_policy(file, kind, true)
}
fn inspect_policy(
    file: &File,
    kind: Kind,
    native: bool,
) -> Result<std::result::Result<Evidence, Unknown>> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    let before = information(file)?;
    let Some(volume) = local_volume(file) else {
        return Ok(Err(Unknown::UnsupportedFilesystem));
    };
    let directory = kind != Kind::File;
    if before.dwFileAttributes & 0x400 != 0
        || (before.dwFileAttributes & 0x10 != 0) != directory
        || (!directory && before.nNumberOfLinks != 1)
    {
        return Ok(Err(Unknown::RedirectedOrAliased));
    }
    let mut filesystem = [0u16; 32];
    let mut flags = 0;
    // SAFETY: file owns handle; optional outputs are null and buffers are sized.
    if unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut flags,
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    } == 0
    {
        return Ok(Err(Unknown::UnsupportedFilesystem));
    }
    let length = filesystem
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(filesystem.len());
    if flags & 0x8 == 0 || String::from_utf16(&filesystem[..length])? != "NTFS" {
        return Ok(Err(Unknown::UnsupportedFilesystem));
    }
    let (mut owner, mut dacl, mut descriptor) = (
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    // SAFETY: readable security outputs are allocated by the OS for this handle.
    let error = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if error != 0 {
        return Err(std::io::Error::from_raw_os_error(error as i32).into());
    }
    let _descriptor = Descriptor(descriptor);
    if owner.is_null() || dacl.is_null() {
        return Ok(Err(Unknown::NotPrivate));
    }
    let mut token = std::ptr::null_mut();
    // SAFETY: current process pseudo handle is valid; output owns a real token handle.
    ensure!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0,
        "private storage token inspection failed"
    );
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut user = [0usize; 64];
    let mut returned = 0;
    // SAFETY: aligned bounded buffer exceeds TOKEN_USER plus native SID maximum.
    ensure!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                user.as_mut_ptr().cast(),
                mem::size_of_val(&user) as u32,
                &mut returned,
            )
        } != 0
            && returned as usize >= mem::size_of::<TOKEN_USER>(),
        "invalid private storage user token"
    );
    let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    if unsafe { IsValidSid(owner) } == 0 || unsafe { EqualSid(owner, sid) } == 0 {
        return Ok(Err(Unknown::NotPrivate));
    }
    let mut control = 0;
    let mut revision = 0;
    ensure!(
        unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } != 0,
        "invalid private storage descriptor"
    );
    // Root must stop inheritance from a broader ancestor. Children are separately
    // inspected; inheritance alone is never evidence of their actual descriptor.
    if !native && kind == Kind::RootDirectory && control & 0x1000 == 0 {
        return Ok(Err(Unknown::NotPrivate));
    }
    if native {
        use windows_sys::Win32::Security::{
            IsWellKnownSid, WinBuiltinAdministratorsSid, WinCreatorOwnerRightsSid,
            WinCreatorOwnerSid, WinLocalSystemSid,
        };
        let count = unsafe { (*dacl).AceCount };
        if count == 0 || count > 16 {
            return Ok(Err(Unknown::UnsupportedAcl));
        }
        let mut owner_grant = false;
        for index in 0..u32::from(count) {
            let mut ace = std::ptr::null_mut();
            ensure!(
                unsafe { GetAce(dacl, index, &mut ace) } != 0 && !ace.is_null(),
                "invalid native boundary ACL"
            );
            let header = unsafe { &*ace.cast::<ACE_HEADER>() };
            // Straightforward allow entries, including inheritance-only creator
            // templates, are sufficient for the native write boundary.
            // No inheritance-protection requirement: actual inherited descriptor
            // is checked; producer creation preserves this safe policy.
            if header.AceType != 0
                || (header.AceSize as usize) < mem::size_of::<ACCESS_ALLOWED_ACE>()
                || header.AceFlags & !0x1b != 0
            {
                return Ok(Err(Unknown::UnsupportedAcl));
            }
            let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
            let principal = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
            if unsafe { IsValidSid(principal) } == 0
                || 8usize + unsafe { GetLengthSid(principal) } as usize > header.AceSize as usize
            {
                return Ok(Err(Unknown::UnsupportedAcl));
            }
            // Owner has already been authenticated against current token above.
            let is_owner = unsafe { EqualSid(principal, sid) } != 0
                || unsafe { IsWellKnownSid(principal, WinCreatorOwnerRightsSid) } != 0;
            let privileged = unsafe { IsWellKnownSid(principal, WinLocalSystemSid) } != 0
                || unsafe { IsWellKnownSid(principal, WinBuiltinAdministratorsSid) } != 0;
            let creator_template = directory
                && header.AceFlags & 0x8 != 0
                && unsafe { IsWellKnownSid(principal, WinCreatorOwnerSid) } != 0;
            // Read/execute sharing does not permit another principal to alter
            // derived state. Keep strict per-object privacy a separate policy.
            let read_only = allowed.Mask & !(0x1200a9 | 0x80000000 | 0x20000000) == 0;
            if !(is_owner || privileged || creator_template || read_only) {
                return Ok(Err(Unknown::NotPrivate));
            }
            owner_grant |= is_owner
                && header.AceFlags & 0x8 == 0
                && allowed.Mask & 0x1f01ff == 0x1f01ff
                && (!directory || header.AceFlags & 3 == 3);
        }
        if !owner_grant {
            return Ok(Err(Unknown::NotPrivate));
        }
    } else {
        if unsafe { (*dacl).AceCount } != 1 {
            return Ok(Err(Unknown::UnsupportedAcl));
        }
        let mut ace = std::ptr::null_mut();
        ensure!(
            unsafe { GetAce(dacl, 0, &mut ace) } != 0 && !ace.is_null(),
            "invalid private storage ACL"
        );
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        if header.AceType != 0
            || (header.AceSize as usize) < mem::size_of::<ACCESS_ALLOWED_ACE>()
            || header.AceFlags & 0x8 != 0
        {
            return Ok(Err(Unknown::UnsupportedAcl));
        }
        let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        let allowed_sid = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
        if unsafe { IsValidSid(allowed_sid) } == 0
            || 8usize + unsafe { GetLengthSid(allowed_sid) } as usize > header.AceSize as usize
            || unsafe { EqualSid(allowed_sid, sid) } == 0
            || allowed.Mask & 0x1f01ff != 0x1f01ff
            || (directory && header.AceFlags & 3 != 3)
        {
            return Ok(Err(Unknown::NotPrivate));
        }
    }
    // Hash only a bounded self-relative descriptor, never pointer-valued layout.
    let length = unsafe { GetSecurityDescriptorLength(descriptor) } as usize;
    if control & 0x8000 == 0 || length == 0 || length > 65_536 {
        return Ok(Err(Unknown::UnsupportedAcl));
    }
    let mut security = Sha256::new();
    security.update(if native {
        b"airk.native-boundary.windows.v1\0".as_slice()
    } else {
        b"airk.private-object.windows.v1\0".as_slice()
    });
    for unit in volume {
        security.update(unit.to_le_bytes());
    }
    security.update(unsafe { std::slice::from_raw_parts(descriptor.cast::<u8>(), length) });
    let security: [u8; 32] = security.finalize().into();
    let after = information(file)?;
    if (
        before.dwVolumeSerialNumber,
        before.nFileIndexHigh,
        before.nFileIndexLow,
        before.dwFileAttributes,
        before.nNumberOfLinks,
    ) != (
        after.dwVolumeSerialNumber,
        after.nFileIndexHigh,
        after.nFileIndexLow,
        after.dwFileAttributes,
        after.nNumberOfLinks,
    ) {
        return Ok(Err(Unknown::Changed));
    }
    Ok(Ok(Evidence {
        identity: [
            before.dwVolumeSerialNumber.into(),
            before.nFileIndexHigh.into(),
            before.nFileIndexLow.into(),
        ],
        security,
        kind,
    }))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use anyhow::Context;
    use std::{
        fs,
        os::windows::{
            ffi::OsStrExt,
            fs::OpenOptionsExt,
            io::{FromRawHandle, OwnedHandle},
        },
    };
    use windows_sys::Win32::{
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                SDDL_REVISION_1,
            },
            SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::CreateDirectoryW,
    };
    fn user_sid() -> Result<String> {
        let mut token = std::ptr::null_mut();
        ensure!(
            unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0,
            "test token unavailable"
        );
        let token = unsafe { OwnedHandle::from_raw_handle(token) };
        let mut user = [0usize; 64];
        let mut returned = 0;
        ensure!(
            unsafe {
                GetTokenInformation(
                    token.as_raw_handle(),
                    TokenUser,
                    user.as_mut_ptr().cast(),
                    mem::size_of_val(&user) as u32,
                    &mut returned,
                )
            } != 0,
            "test user unavailable"
        );
        let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let mut text = std::ptr::null_mut();
        ensure!(
            unsafe { ConvertSidToStringSidW(sid, &mut text) } != 0,
            "test SID conversion failed"
        );
        let _owner = Descriptor(text.cast());
        let mut length = 0;
        while length < 256 && unsafe { *text.add(length) } != 0 {
            length += 1;
        }
        ensure!(length < 256, "test SID exceeded bound");
        Ok(String::from_utf16(unsafe {
            std::slice::from_raw_parts(text, length)
        })?)
    }
    pub(crate) fn private_root(path: &std::path::Path) -> Result<()> {
        directory_acl(path, false)
    }
    pub(crate) fn foreign_readable_root(path: &std::path::Path) -> Result<()> {
        directory_acl(path, true)
    }
    fn directory_acl(path: &std::path::Path, foreign: bool) -> Result<()> {
        let extra = if foreign { "(A;OICI;FR;;;WD)" } else { "" };
        directory_acl_with(path, extra)
    }
    fn directory_acl_with(path: &std::path::Path, extra: &str) -> Result<()> {
        let sid = user_sid()?;
        let text: Vec<u16> = format!("O:{sid}D:P(A;OICI;FA;;;{sid}){extra}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = std::ptr::null_mut();
        ensure!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            } != 0,
            "private test descriptor invalid"
        );
        let _owner = Descriptor(descriptor);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        ensure!(
            unsafe { CreateDirectoryW(path.as_ptr(), &attributes) } != 0,
            "private test root creation failed"
        );
        Ok(())
    }
    #[test]
    fn only_handle_volume_guid_with_positive_fixed_classification_is_local() {
        let local: Vec<u16> = r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\state"
            .encode_utf16()
            .collect();
        let root = volume_guid_root(&local).expect("well formed handle GUID result");
        assert!(fixed_volume(Some(&root), 3));
        for unsupported in [0, 1, 2, 4, 5, 6] {
            assert!(!fixed_volume(Some(&root), unsupported));
        }
        assert!(!fixed_volume(None, 3));
        for remote_or_unsupported in [
            r"\\server\share\state",
            r"\\?\UNC\server\share\state",
            r"C:\state",
            r"\Device\Mup\server\share\state",
            r"\\?\Volume{bad}\state",
        ] {
            let path: Vec<u16> = remote_or_unsupported.encode_utf16().collect();
            assert!(volume_guid_root(&path).is_none());
        }
    }
    #[test]
    #[ignore = "requires an explicitly provisioned SMB/remote read-only fixture"]
    fn real_remote_handle_never_qualifies_as_local() -> Result<()> {
        let path = std::env::var_os("AIRK_TEST_REMOTE_PRIVATE_STORAGE_FILE")
            .context("set AIRK_TEST_REMOTE_PRIVATE_STORAGE_FILE to the approved remote fixture")?;
        let file = File::open(path)?;
        assert!(
            local_volume(&file).is_none(),
            "fixture must be an actual remote/unsupported handle"
        );
        assert!(matches!(
            super::super::inspect(&file, Kind::File)?,
            Observation::Unknown(Unknown::UnsupportedFilesystem)
        ));
        Ok(())
    }

    #[test]
    fn read_sharing_qualifies_write_boundary_but_not_strict_object_privacy() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("foreign-readable");
        foreign_readable_root(&root)?;
        let directory = fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x02000000 | 0x00200000)
            .open(&root)?;
        assert!(super::native_boundary(&directory, Kind::RootDirectory)?.is_ok());
        assert!(matches!(
            super::super::inspect(&directory, Kind::RootDirectory)?,
            Observation::Unknown(_)
        ));
        Ok(())
    }

    #[test]
    fn foreign_write_access_still_requires_strict_fallback() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        for (name, acl) in [
            ("write", "(A;OICI;FW;;;WD)"),
            ("delete", "(A;OICI;SD;;;WD)"),
            ("change-acl", "(A;OICI;WD;;;WD)"),
            ("take-owner", "(A;OICI;WO;;;WD)"),
        ] {
            let root = temporary.path().join(name);
            directory_acl_with(&root, acl)?;
            let directory = fs::OpenOptions::new()
                .read(true)
                .custom_flags(0x02000000 | 0x00200000)
                .open(&root)?;
            assert!(
                matches!(
                    super::native_boundary(&directory, Kind::RootDirectory)?,
                    Err(Unknown::NotPrivate)
                ),
                "foreign {name} grant qualified"
            );
        }
        Ok(())
    }

    #[test]
    fn creator_templates_and_partial_privileged_grants_qualify_native_boundary() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("developer-layout");
        directory_acl_with(&root, "(A;OICIIO;FA;;;CO)(A;OICI;FR;;;BA)(A;OICI;FR;;;SY)")?;
        let directory = fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x02000000 | 0x00200000)
            .open(&root)?;
        assert!(super::native_boundary(&directory, Kind::RootDirectory)?.is_ok());
        Ok(())
    }

    #[test]
    fn explicit_current_user_root_and_actual_inherited_file_qualify() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("private");
        private_root(&root)?;
        let directory = fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x02000000 | 0x00200000)
            .open(&root)?;
        let Observation::Private(proof) = super::super::inspect(&directory, Kind::RootDirectory)?
        else {
            anyhow::bail!("Windows qualification requires local NTFS and exact owner ACL");
        };
        assert!(proof.recheck()?.is_ok());
        let path = root.join("state");
        fs::write(&path, b"inert inherited file")?;
        let file = File::open(&path)?;
        let Observation::Private(proof) = super::super::inspect(&file, Kind::File)? else {
            anyhow::bail!("actual inherited private file did not qualify");
        };
        assert!(proof.recheck()?.is_ok());
        fs::hard_link(&path, root.join("alias"))?;
        assert!(proof.recheck()?.is_err());
        Ok(())
    }
}
