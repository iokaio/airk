// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{fs, path::Path};

fn directory(path: &Path) -> Result<File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Fixture policy mutation needs WRITE_DAC; this is not a production
        // traversal helper or a claim that writable names are pinned here.
        options
            .access_mode(0x0002_0000 | 0x0004_0000 | 1 | 0x80)
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000);
    }
    Ok(options.open(path)?)
}

fn public_permissions(file: &File, write: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(if write {
            0o777
        } else {
            0o755
        }))?;
    }
    #[cfg(windows)]
    {
        use std::{
            ffi::c_void,
            os::windows::{ffi::OsStrExt, io::AsRawHandle},
        };
        use windows_sys::Win32::{
            Foundation::LocalFree,
            Security::{
                Authorization::{
                    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
                    SE_FILE_OBJECT, SetSecurityInfo,
                },
                DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl,
                PROTECTED_DACL_SECURITY_INFORMATION,
            },
        };
        let user = crate::endpoint::current_user()?;
        let permission = if write { "FA" } else { "FR" };
        let text = format!("D:P(A;OICI;FA;;;{user})(A;OICI;{permission};;;WD)");
        let wide: Vec<_> = std::ffi::OsStr::new(&text)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: terminated fixture SDDL, live output and fixed API revision.
        ensure!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    wide.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            } != 0,
            "fixture security descriptor unavailable"
        );
        struct Allocation(*mut c_void);
        impl Drop for Allocation {
            fn drop(&mut self) {
                // SAFETY: native conversion allocated this live descriptor.
                unsafe {
                    LocalFree(self.0);
                }
            }
        }
        let _descriptor = Allocation(descriptor);
        let (mut present, mut defaulted, mut dacl) = (0, 0, std::ptr::null_mut());
        // SAFETY: successful conversion owns a valid descriptor and outputs live.
        ensure!(
            unsafe {
                GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
            } != 0
                && present != 0,
            "fixture DACL missing"
        );
        // SAFETY: WRITE_DAC fixture handle and DACL remain live for this call.
        let status = unsafe {
            SetSecurityInfo(
                file.as_raw_handle().cast(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null_mut(),
            )
        };
        ensure!(status == 0, "fixture ACL mutation failed: {status}");
    }
    Ok(())
}

#[test]
fn private_handle_policy_checks_private_objects_and_types() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("private");
    crate::endpoint::prepare_root(&path)?;
    let held = directory(&path)?;
    require_private_directory_handle(&held)?;
    require_protected_parent_handle(&held)?;
    let metadata = tempfile::NamedTempFile::new_in(&path)?;
    require_private_metadata_handle(metadata.as_file())?;
    assert!(require_private_directory_handle(metadata.as_file()).is_err());
    assert!(require_private_metadata_handle(&held).is_err());
    assert!(require_protected_parent_handle(metadata.as_file()).is_err());
    Ok(())
}

#[test]
fn private_handle_policy_refuses_foreign_access_but_parent_allows_readers() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("private");
    crate::endpoint::prepare_root(&path)?;
    let held = directory(&path)?;
    public_permissions(&held, false)?;
    assert!(require_private_directory_handle(&held).is_err());
    require_protected_parent_handle(&held)?;
    public_permissions(&held, true)?;
    assert!(require_private_directory_handle(&held).is_err());
    assert!(require_protected_parent_handle(&held).is_err());
    Ok(())
}

#[test]
fn private_handle_policy_stays_bound_to_open_object_across_name_replacement() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("private");
    crate::endpoint::prepare_root(&path)?;
    let held = directory(&path)?;
    fs::rename(&path, temp.path().join("moved"))?;
    crate::endpoint::prepare_root(&path)?;
    let replacement = directory(&path)?;
    public_permissions(&replacement, false)?;
    require_private_directory_handle(&held)?;
    assert!(require_private_directory_handle(&replacement).is_err());
    Ok(())
}

#[test]
fn private_handle_policy_rejects_aliased_metadata_files() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("private");
    crate::endpoint::prepare_root(&path)?;
    let file = tempfile::NamedTempFile::new_in(&path)?;
    fs::hard_link(file.path(), path.join("alias"))?;
    assert!(require_private_metadata_handle(file.as_file()).is_err());
    Ok(())
}
