// SPDX-License-Identifier: Apache-2.0
//! Windows descendant containment. Never changes host job policy or requests breakaway.
use anyhow::Result;

#[cfg(windows)]
pub struct ChildJob(std::os::windows::io::OwnedHandle);
#[cfg(not(windows))]
pub struct ChildJob;

impl ChildJob {
    /// Failure is explicit: do not continue with an uncontained running child.
    pub fn attach(child: &mut std::process::Child) -> Result<Self> {
        match Self::try_attach(child) {
            Ok(job) => Ok(job),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(error)
            }
        }
    }

    #[cfg(not(windows))]
    fn try_attach(_child: &std::process::Child) -> Result<Self> {
        Ok(Self)
    }

    #[cfg(windows)]
    fn try_attach(child: &std::process::Child) -> Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        // SAFETY: anonymous, non-inheritable job with no caller-supplied pointers.
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful CreateJobObjectW transfers one real handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        // SAFETY: all-zero fields are valid defaults for this native value structure.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: live handle and initialized fixed-size structure with correct byte count.
        if unsafe {
            SetInformationJobObject(
                handle.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: both owned handles remain live; nested-job denial stays an error.
        if unsafe { AssignProcessToJobObject(handle.as_raw_handle(), child.as_raw_handle()) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for ChildJob {
    fn drop(&mut self) {
        // OwnedHandle closes after this method; reading it makes the lifetime explicit.
        use std::os::windows::io::AsRawHandle;
        let _ = self.0.as_raw_handle();
    }
}
