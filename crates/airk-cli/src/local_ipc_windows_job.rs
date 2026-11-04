// SPDX-License-Identifier: Apache-2.0
//! Current-user security for native named jobs. Validation never repairs an ACL
//! or establishes process/session absence; callers retain their native handle.
use super::{Allocation, current_user, descriptor, sid_string};
use anyhow::{Result, ensure};
use std::{
    io, mem,
    os::windows::io::{AsRawHandle, BorrowedHandle},
};
use windows_sys::Win32::{
    Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER,
        Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT},
        DACL_SECURITY_INFORMATION, GetAce, GetSecurityDescriptorControl,
        OWNER_SECURITY_INFORMATION, SE_DACL_PRESENT, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES,
    },
    System::SystemServices::ACCESS_ALLOWED_ACE_TYPE,
};

/// Keep the existing native descriptor allocation alive during object creation.
/// The SID is derived from this process's native token, never a frontend claim.
pub(crate) fn with_current_user_job_security<T>(
    action: impl FnOnce(&SECURITY_ATTRIBUTES, &str) -> Result<T>,
) -> Result<T> {
    let user = current_user()?;
    let security = descriptor(&format!("O:{user}D:P(A;;GA;;;{user})"))?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security.0,
        bInheritHandle: 0,
    };
    action(&attributes, &user)
}

/// Inspect the actual opened object's owner and protected DACL before the
/// caller adopts any destructive kill-on-drop responsibility for that job.
/// This proves user isolation only, not a boot/session or drain observation.
pub(crate) fn require_job_user(handle: BorrowedHandle<'_>, expected_user: &str) -> Result<()> {
    let user = current_user()?;
    ensure!(user == expected_user, "native job user changed");
    let (mut security, mut owner, mut dacl) = (
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    // SAFETY: the borrowed handle stays live, all outputs are writable, and
    // GetSecurityInfo owns the returned descriptor allocation on success.
    let status = unsafe {
        GetSecurityInfo(
            handle.as_raw_handle(),
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut security,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32).into());
    }
    let _security = Allocation(security);
    ensure!(
        !security.is_null() && !owner.is_null() && !dacl.is_null(),
        "native job has an unrestricted security descriptor"
    );
    ensure!(sid_string(owner)? == user, "foreign native job owner");
    let (mut control, mut revision) = (0, 0);
    // SAFETY: security is a live successful OS-returned descriptor, and both
    // scalar output pointers remain valid throughout the call.
    if unsafe { GetSecurityDescriptorControl(security, &mut control, &mut revision) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    ensure!(
        control & (SE_DACL_PRESENT | SE_DACL_PROTECTED) == SE_DACL_PRESENT | SE_DACL_PROTECTED,
        "native job DACL is not protected"
    );
    // SAFETY: dacl points into the live OS-returned security descriptor.
    let count = unsafe { (*dacl).AceCount };
    ensure!(
        (1..=64).contains(&count),
        "native job ACL exceeds owner-only bounds"
    );
    for index in 0..u32::from(count) {
        let mut ace = std::ptr::null_mut();
        // SAFETY: index is below the native ACL entry count, output is writable.
        ensure!(
            unsafe { GetAce(dacl, index, &mut ace) } != 0 && !ace.is_null(),
            "invalid native job ACL"
        );
        // SAFETY: GetAce returned a valid ACE in the live descriptor allocation.
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        ensure!(
            header.AceType == ACCESS_ALLOWED_ACE_TYPE as u8
                && header.AceFlags == 0
                && header.AceSize as usize >= mem::size_of::<ACCESS_ALLOWED_ACE>(),
            "unsupported native job ACE"
        );
        // SAFETY: the native ACE type and minimum size were checked above.
        let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        let sid = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
        ensure!(
            allowed.Mask != 0 && sid_string(sid)? == user,
            "native job ACL grants access outside its owner"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::{AsHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Storage::FileSystem::READ_CONTROL,
        System::{
            JobObjects::{CreateJobObjectW, OpenJobObjectW},
            SystemServices::JOB_OBJECT_QUERY,
        },
    };

    fn create(attributes: &SECURITY_ATTRIBUTES, name: *const u16) -> Result<OwnedHandle> {
        // SAFETY: caller keeps attributes/security and optional terminated name
        // live throughout creation; success transfers one native handle.
        let handle = unsafe { CreateJobObjectW(attributes, name) };
        if handle.is_null() {
            return Err(io::Error::last_os_error().into());
        }
        // SAFETY: successful creation transfers a real owned handle.
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }

    #[test]
    fn actual_named_job_open_validates_native_owner_and_protected_user_dacl() -> Result<()> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let name: Vec<u16> = format!(
            "Local\\AIRK-job-acl-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
        .encode_utf16()
        .chain(Some(0))
        .collect();
        with_current_user_job_security(|attributes, user| {
            assert_eq!(attributes.bInheritHandle, 0);
            let _created = create(attributes, name.as_ptr())?;
            // SAFETY: name is terminated and live; request only inspection rights.
            let opened =
                unsafe { OpenJobObjectW(JOB_OBJECT_QUERY | READ_CONTROL, 0, name.as_ptr()) };
            if opened.is_null() {
                return Err(io::Error::last_os_error().into());
            }
            // SAFETY: successful open transfers one real owned handle.
            let opened = unsafe { OwnedHandle::from_raw_handle(opened) };
            require_job_user(opened.as_handle(), user)
        })
    }

    #[test]
    fn native_job_rejects_expected_user_claim_not_matching_native_token() -> Result<()> {
        with_current_user_job_security(|attributes, _user| {
            let job = create(attributes, std::ptr::null())?;
            assert!(require_job_user(job.as_handle(), "not-a-native-sid").is_err());
            Ok(())
        })
    }

    #[test]
    fn native_job_rejects_foreign_unprotected_empty_and_null_dacls() -> Result<()> {
        let user = current_user()?;
        for sddl in [
            format!("O:{user}D:P(A;;GA;;;{user})(A;;GR;;;WD)"),
            format!("O:{user}D:(A;;GA;;;{user})"),
            format!("O:{user}D:P"),
            format!("O:{user}D:NO_ACCESS_CONTROL"),
        ] {
            let security = descriptor(&sddl)?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: security.0,
                bInheritHandle: 0,
            };
            let job = create(&attributes, std::ptr::null())?;
            assert!(
                require_job_user(job.as_handle(), &user).is_err(),
                "unsafe ACL accepted: {sddl}"
            );
        }
        Ok(())
    }
}
