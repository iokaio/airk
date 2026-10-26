// SPDX-License-Identifier: Apache-2.0
//! Windows descendant containment. Never changes host job policy or requests breakaway.
use anyhow::Result;

#[cfg(windows)]
pub struct ChildJob(
    std::os::windows::io::OwnedHandle,
    std::sync::Mutex<Option<Vec<std::os::windows::io::OwnedHandle>>>,
);
#[cfg(not(windows))]
pub struct ChildJob;

impl ChildJob {
    pub fn drain(&self) {
        #[cfg(windows)]
        while self
            .terminate_and_wait(std::time::Duration::from_secs(2))
            .is_err()
        {
            // Unknown OS state retains ownership, rather than freeing capacity.
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    /// Killing a job is asynchronous. Capacity and owner locks must remain held
    /// until the kernel confirms that every process in this exact job exited.
    #[cfg(windows)]
    pub fn terminate_and_wait(&self, timeout: std::time::Duration) -> Result<()> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::{
            IsProcessInJob, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectBasicProcessIdList,
            JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
            TerminateJobObject,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            WaitForSingleObject,
        };
        let deadline = std::time::Instant::now() + timeout;
        let mut handles = self
            .1
            .lock()
            .map_err(|_| anyhow::anyhow!("job drain state unavailable"))?;
        if handles.is_none() {
            // A finite initial process ceiling bounds this snapshot. Lowering it
            // to one prevents any still-live job member from creating another
            // descendant while we capture exact waitable handles for termination.
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            // Preserve extraction memory/CPU limits while closing creation.
            anyhow::ensure!(
                unsafe {
                    QueryInformationJobObject(
                        self.0.as_raw_handle(),
                        JobObjectExtendedLimitInformation,
                        (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                        std::mem::size_of_val(&limits) as u32,
                        std::ptr::null_mut(),
                    )
                } != 0,
                "owned job limits unavailable"
            );
            limits.BasicLimitInformation.LimitFlags |=
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
            limits.BasicLimitInformation.ActiveProcessLimit = 1;
            anyhow::ensure!(
                unsafe {
                    SetInformationJobObject(
                        self.0.as_raw_handle(),
                        JobObjectExtendedLimitInformation,
                        (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                        std::mem::size_of_val(&limits) as u32,
                    )
                } != 0,
                "cannot close owned descendant admission: {}",
                std::io::Error::last_os_error()
            );
            #[repr(C)]
            struct ProcessList {
                assigned: u32,
                listed: u32,
                ids: [usize; 32],
            }
            let mut list = ProcessList {
                assigned: 0,
                listed: 0,
                ids: [0; 32],
            };
            anyhow::ensure!(
                unsafe {
                    QueryInformationJobObject(
                        self.0.as_raw_handle(),
                        JobObjectBasicProcessIdList,
                        (&mut list as *mut ProcessList).cast(),
                        std::mem::size_of_val(&list) as u32,
                        std::ptr::null_mut(),
                    )
                } != 0
                    && list.assigned <= 32
                    && list.listed <= 32,
                "owned descendant snapshot unavailable: {}",
                std::io::Error::last_os_error()
            );
            let mut captured = Vec::new();
            for &pid in &list.ids[..list.listed as usize] {
                let pid: u32 = pid.try_into()?;
                // Read/synchronize only; membership binds any concurrently reused PID.
                let raw = unsafe {
                    OpenProcess(
                        PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                        0,
                        pid,
                    )
                };
                if raw.is_null() {
                    let error = std::io::Error::last_os_error();
                    if error.raw_os_error() == Some(87) {
                        continue;
                    } // Already gone.
                    return Err(error.into());
                }
                let process = unsafe { OwnedHandle::from_raw_handle(raw) };
                let mut associated = 0;
                anyhow::ensure!(
                    unsafe {
                        IsProcessInJob(
                            process.as_raw_handle(),
                            self.0.as_raw_handle(),
                            &mut associated,
                        )
                    } != 0,
                    "cannot verify owned descendant membership"
                );
                if associated != 0 {
                    captured.push(process);
                }
            }
            *handles = Some(captured);
        }
        // SAFETY: the owned anonymous job cannot be substituted by a reused PID.
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        loop {
            // SAFETY: this fixed-size native value is initialized by the query.
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
                unsafe { std::mem::zeroed() };
            // SAFETY: live owned job, writable initialized output and matching size.
            if unsafe {
                QueryInformationJobObject(
                    self.0.as_raw_handle(),
                    JobObjectBasicAccountingInformation,
                    (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    std::mem::size_of_val(&accounting) as u32,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let mut exited = true;
            for handle in handles.as_ref().expect("job handles captured") {
                match unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) } {
                    0 => (),
                    258 => exited = false,
                    _ => return Err(std::io::Error::last_os_error().into()),
                }
            }
            if accounting.ActiveProcesses == 0 && exited {
                return Ok(());
            }
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "owned descendants have not exited; retain their capacity and owner locks"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

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
        use std::os::windows::io::AsRawHandle;
        let job = Self::unassigned()?;
        // SAFETY: both owned handles remain live; nested-job denial stays an error.
        if unsafe {
            windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(
                job.0.as_raw_handle(),
                child.as_raw_handle(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(job)
    }

    /// Construct before a suspended extraction child is launched. The caller may
    /// strengthen limits and assign that child before allowing execution.
    #[cfg(windows)]
    pub fn unassigned() -> Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
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
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        info.BasicLimitInformation.ActiveProcessLimit = 32;
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
        Ok(Self(handle, std::sync::Mutex::new(None)))
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawHandle for ChildJob {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        self.0.as_raw_handle()
    }
}

#[cfg(windows)]
impl Drop for ChildJob {
    fn drop(&mut self) {
        // Retain the exact job and enclosing owner on unknown kernel state. Closing
        // the handle alone requests termination but does not prove completion.
        // This runs on supervision/cleanup threads, never the health handler.
        self.drain();
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        process::{Command, Stdio},
        time::Duration,
    };

    #[test]
    fn descendant_fixture() -> Result<()> {
        let Ok(mode) = std::env::var("AIRK_JOB_DRAIN_FIXTURE") else {
            return Ok(());
        };
        if mode == "descendant" {
            std::thread::sleep(Duration::from_secs(60));
        } else {
            // The parent attaches the job before allowing any descendant launch.
            std::io::stdin().read_exact(&mut [0])?;
            let mut ids = Vec::new();
            for _ in 0..2 {
                let child = Command::new(std::env::current_exe()?)
                    .args([
                        "--exact",
                        "child_job::tests::descendant_fixture",
                        "--nocapture",
                    ])
                    .env("AIRK_JOB_DRAIN_FIXTURE", "descendant")
                    .stdin(Stdio::null())
                    .spawn()?;
                ids.push(child.id().to_string());
            }
            std::fs::write(mode, ids.join("\n"))?;
        }
        Ok(())
    }

    #[test]
    fn drain_confirms_descendant_exit_after_its_parent_has_already_exited() -> Result<()> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        let owner = tempfile::tempdir()?;
        let receipt = owner.path().join("owned-descendant.pid");
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "child_job::tests::descendant_fixture",
                "--nocapture",
            ])
            .env("AIRK_JOB_DRAIN_FIXTURE", &receipt)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let job = ChildJob::attach(&mut child)?;
        child.stdin.take().unwrap().write_all(&[1])?;
        anyhow::ensure!(child.wait()?.success(), "descendant fixture failed");
        let mut processes = Vec::new();
        for line in std::fs::read_to_string(receipt)?.lines() {
            let pid: u32 = line.parse()?;
            // SAFETY: read-only synchronization handle for the exact still-live child.
            let raw = unsafe {
                OpenProcess(
                    PROCESS_SYNCHRONIZE
                        | windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    pid,
                )
            };
            anyhow::ensure!(!raw.is_null(), "owned descendant exited before validation");
            // SAFETY: successful OpenProcess transfers this real handle.
            let process = unsafe { OwnedHandle::from_raw_handle(raw) };
            let mut associated = 0;
            assert_ne!(
                unsafe {
                    windows_sys::Win32::System::JobObjects::IsProcessInJob(
                        process.as_raw_handle(),
                        job.0.as_raw_handle(),
                        &mut associated,
                    )
                },
                0
            );
            assert_ne!(associated, 0, "descendant did not inherit its parent's job");
            assert_eq!(
                unsafe { WaitForSingleObject(process.as_raw_handle(), 0) },
                258
            );
            processes.push(process);
        }
        assert_eq!(processes.len(), 2);
        job.terminate_and_wait(Duration::from_secs(5))?;
        for process in processes {
            assert_eq!(
                unsafe { WaitForSingleObject(process.as_raw_handle(), 0) },
                0
            );
        }
        // Repeated cleanup must not target a PID or require a live group leader.
        job.terminate_and_wait(Duration::from_secs(1))?;
        Ok(())
    }
}
