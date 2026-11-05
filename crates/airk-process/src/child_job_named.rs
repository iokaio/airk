// SPDX-License-Identifier: Apache-2.0
//! Explicit named ownership for a fixed guarded helper. No service metadata or
//! implicit recovery admission lives in this native adapter.
#![cfg(windows)]
use super::ChildJob;
use anyhow::{Result, ensure};
use std::{
    fs::File,
    io,
    mem::{self, size_of, size_of_val},
    os::windows::{
        ffi::OsStrExt,
        io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    path::Path,
    process::ExitStatus,
    sync::Mutex,
};
use windows_sys::Win32::{
    Foundation::{
        DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, HANDLE,
        SetLastError, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::SECURITY_ATTRIBUTES,
    System::{
        JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            OpenJobObjectW, QueryInformationJobObject, SetInformationJobObject,
        },
        Threading::{
            CREATE_NO_WINDOW, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess, GetExitCodeProcess, INFINITE,
            InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
            WaitForSingleObject,
        },
    },
};

pub enum NamedJobOpen {
    Missing,
    Owned(ChildJob),
}

fn name(value: &str) -> Result<Vec<u16>> {
    ensure!(
        value.starts_with("Global\\")
            && value.len() > 7
            && value.len() <= 240
            && value[7..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "named helper job requires a bounded global native name"
    );
    Ok(value.encode_utf16().chain(Some(0)).collect())
}

impl ChildJob {
    /// Caller supplies an owner-only descriptor and a compiled native name.
    /// A collision is refused without constructing a kill-on-drop job owner.
    pub fn unassigned_named(value: &str, attributes: &SECURITY_ATTRIBUTES) -> Result<Self> {
        ensure!(
            attributes.bInheritHandle == 0,
            "helper job handle must not be inherited"
        );
        let encoded = name(value)?;
        // SAFETY: clearing only this thread's native error slot.
        unsafe {
            SetLastError(0);
        }
        // SAFETY: descriptor and terminated native name remain live for the call.
        let raw = unsafe { CreateJobObjectW(attributes, encoded.as_ptr()) };
        let error = io::Error::last_os_error();
        if raw.is_null() {
            return Err(error.into());
        }
        // SAFETY: successful creation/open transfers this one real handle.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        ensure!(
            error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32),
            "native helper job name already exists"
        );
        // SAFETY: zero represents valid defaults for all remaining native fields.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_ACTIVE_PROCESS | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        limits.BasicLimitInformation.ActiveProcessLimit = 32;
        // SAFETY: owned handle and correctly sized initialized limit structure.
        ensure!(
            unsafe {
                SetInformationJobObject(
                    handle.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of_val(&limits) as u32,
                )
            } != 0,
            io::Error::last_os_error()
        );
        Ok(Self(handle, Mutex::new(None), 32))
    }

    /// This observes only an existing object. Native metadata and user/owner
    /// security are validated before an opened handle becomes a cleanup owner.
    pub fn open_named(
        value: &str,
        validate_owner: impl FnOnce(BorrowedHandle<'_>) -> Result<()>,
    ) -> Result<NamedJobOpen> {
        let encoded = name(value)?;
        // QUERY | TERMINATE | SET_ATTRIBUTES | READ_CONTROL. Setting the finite
        // active-process ceiling is part of canonical descendant drain.
        const ACCESS: u32 = 0x0004 | 0x0008 | 0x0002 | 0x0002_0000;
        // SAFETY: terminated name, fixed rights, and a non-inheritable handle.
        let raw = unsafe { OpenJobObjectW(ACCESS, 0, encoded.as_ptr()) };
        if raw.is_null() {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32) {
                Ok(NamedJobOpen::Missing)
            } else {
                Err(error.into())
            };
        }
        // SAFETY: successful native open transfers one owned handle. Closing
        // this observational handle never explicitly signals a foreign object.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        validate_owner(handle.as_handle())?;
        // SAFETY: zeroed writable output and exact native structure size.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
        ensure!(
            unsafe {
                QueryInformationJobObject(
                    handle.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of_val(&limits) as u32,
                    std::ptr::null_mut(),
                )
            } != 0,
            io::Error::last_os_error()
        );
        ensure!(
            limits.BasicLimitInformation.LimitFlags
                == JOB_OBJECT_LIMIT_ACTIVE_PROCESS | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                && (1..=32).contains(&limits.BasicLimitInformation.ActiveProcessLimit),
            "named helper job containment policy changed"
        );
        // Protected producer jobs start at 32. A previous drain may already
        // have closed creation at one while its descendants are still exiting.
        Ok(NamedJobOpen::Owned(Self(handle, Mutex::new(None), 32)))
    }

    /// Atomically places the child in this exact job during CreateProcess.
    /// No suspend/assign gap exists if the parent dies during startup. The
    /// caller permits the guarded helper only after durable native binding.
    pub fn spawn_guarded(&self, program: &Path, role: &str) -> Result<AtomicJobChild> {
        self.spawn_guarded_io(program, role, None)
    }

    /// Bounded read on an already owned native job. Query failure or an
    /// incomplete/racing census is unknown, never an empty/zero observation.
    pub fn member_pids(&self) -> Result<Vec<u32>> {
        use windows_sys::Win32::System::JobObjects::JobObjectBasicProcessIdList;
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
        // SAFETY: retained owned job and correctly aligned bounded output.
        ensure!(
            unsafe {
                QueryInformationJobObject(
                    self.0.as_raw_handle(),
                    JobObjectBasicProcessIdList,
                    (&mut list as *mut ProcessList).cast(),
                    size_of_val(&list) as u32,
                    std::ptr::null_mut(),
                )
            } != 0,
            io::Error::last_os_error()
        );
        ensure!(
            list.assigned == list.listed && list.listed <= 32,
            "native job membership is incomplete"
        );
        let mut result = Vec::with_capacity(list.listed as usize);
        for &pid in &list.ids[..list.listed as usize] {
            let pid = u32::try_from(pid)?;
            ensure!(pid > 1, "native job membership identity is invalid");
            result.push(pid);
        }
        result.sort_unstable();
        result.dedup();
        ensure!(
            result.len() == list.listed as usize,
            "native job membership is duplicated"
        );
        Ok(result)
    }

    /// Preserve a caller's existing stderr policy without adding a pipe-reader
    /// thread. The borrowed native file is duplicated into the explicit list.
    pub fn spawn_guarded_with_stderr(
        &self,
        program: &Path,
        role: &str,
        stderr: &File,
    ) -> Result<AtomicJobChild> {
        self.spawn_guarded_io(program, role, Some(stderr))
    }

    fn spawn_guarded_io(
        &self,
        program: &Path,
        role: &str,
        stderr: Option<&File>,
    ) -> Result<AtomicJobChild> {
        ensure!(
            program.is_absolute()
                && !role.is_empty()
                && role.len() <= 64
                && role
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            "helper spawn requires an absolute program and fixed native role"
        );
        let application = program.as_os_str().encode_wide().collect::<Vec<_>>();
        ensure!(
            !application.contains(&0) && !application.contains(&(b'"' as u16)),
            "invalid native helper program"
        );
        let mut command = vec![b'"' as u16];
        command.extend_from_slice(&application);
        command.extend([b'"' as u16, b' ' as u16]);
        command.extend(role.encode_utf16());
        command.push(0);
        let application = application.into_iter().chain(Some(0)).collect::<Vec<_>>();
        let (input_read, input_write) = std::io::pipe()?;
        let (output_read, output_write) = std::io::pipe()?;
        let (error_read, error_write) = std::io::pipe()?;
        let input = inheritable(input_read.as_handle())?;
        let output = inheritable(output_write.as_handle())?;
        let error = inheritable(match stderr {
            Some(file) => file.as_handle(),
            None => error_write.as_handle(),
        })?;
        let handles = [
            input.as_raw_handle(),
            output.as_raw_handle(),
            error.as_raw_handle(),
        ];
        let jobs = [self.0.as_raw_handle()];
        let attributes = Attributes::new(&handles, &jobs)?;
        // SAFETY: all zero values are valid defaults before explicit fields.
        let mut startup: STARTUPINFOEXW = unsafe { mem::zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = handles[0];
        startup.StartupInfo.hStdOutput = handles[1];
        startup.StartupInfo.hStdError = handles[2];
        startup.lpAttributeList = attributes.pointer();
        // SAFETY: native output is writable; program, mutable command, explicit
        // handle/job lists and startup structure all live through CreateProcess.
        let mut information: PROCESS_INFORMATION = unsafe { mem::zeroed() };
        ensure!(
            unsafe {
                CreateProcessW(
                    application.as_ptr(),
                    command.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    1,
                    CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                    std::ptr::null(),
                    std::ptr::null(),
                    &startup.StartupInfo,
                    &mut information,
                )
            } != 0,
            io::Error::last_os_error()
        );
        // SAFETY: successful creation transfers these two real handles.
        let process = unsafe { OwnedHandle::from_raw_handle(information.hProcess) };
        let thread = unsafe { OwnedHandle::from_raw_handle(information.hThread) };
        drop(thread);
        drop((input, output, error, input_read, output_write, error_write));
        Ok(AtomicJobChild {
            process,
            pid: information.dwProcessId,
            stdin: Some(File::from(OwnedHandle::from(input_write))),
            stdout: Some(File::from(OwnedHandle::from(output_read))),
            stderr: Some(File::from(OwnedHandle::from(error_read))),
        })
    }
}

fn inheritable(handle: BorrowedHandle<'_>) -> io::Result<OwnedHandle> {
    let mut duplicate = std::ptr::null_mut();
    // SAFETY: duplicate this exact live pipe handle into this process. The
    // explicit HANDLE_LIST, not ambient handle inheritance, selects child I/O.
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle.as_raw_handle(),
            GetCurrentProcess(),
            &mut duplicate,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful duplication transfers a real owned handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(duplicate) })
}

struct Attributes<'a> {
    storage: Vec<usize>,
    initialized: bool,
    borrowed_lists: std::marker::PhantomData<&'a [HANDLE]>,
}
impl<'a> Attributes<'a> {
    fn new(handles: &'a [HANDLE; 3], jobs: &'a [HANDLE; 1]) -> Result<Self> {
        let mut size = 0;
        // SAFETY: sizing query writes only the required byte count.
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut size);
        }
        ensure!(
            size > 0 && size <= 65536,
            "invalid native helper attribute size"
        );
        let mut value = Self {
            storage: vec![0; size.div_ceil(size_of::<usize>())],
            initialized: false,
            borrowed_lists: std::marker::PhantomData,
        };
        // SAFETY: aligned allocation contains the entire requested byte range.
        ensure!(
            unsafe { InitializeProcThreadAttributeList(value.pointer(), 2, 0, &mut size) } != 0,
            io::Error::last_os_error()
        );
        value.initialized = true;
        for (key, pointer, length) in [
            (
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
                handles.as_ptr(),
                size_of_val(handles),
            ),
            (
                PROC_THREAD_ATTRIBUTE_JOB_LIST,
                jobs.as_ptr(),
                size_of_val(jobs),
            ),
        ] {
            // SAFETY: borrowed lists remain live through CreateProcess, with
            // exact native element sizes; the attribute list owns no handles.
            ensure!(
                unsafe {
                    UpdateProcThreadAttribute(
                        value.pointer(),
                        0,
                        key as usize,
                        pointer.cast(),
                        length,
                        std::ptr::null_mut(),
                        std::ptr::null(),
                    )
                } != 0,
                io::Error::last_os_error()
            );
        }
        Ok(value)
    }
    fn pointer(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_ptr().cast_mut().cast()
    }
}
impl Drop for Attributes<'_> {
    fn drop(&mut self) {
        // SAFETY: only a successfully initialized list must reach this owner.
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }
}

pub struct AtomicJobChild {
    process: OwnedHandle,
    pid: u32,
    pub stdin: Option<File>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
}
impl AtomicJobChild {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn assigned_to(&self, job: &ChildJob) -> io::Result<bool> {
        let mut assigned = 0;
        // SAFETY: both handles name actual retained native objects; this is a
        // read-only verification before any guarded startup permit is issued.
        if unsafe {
            windows_sys::Win32::System::JobObjects::IsProcessInJob(
                self.process.as_raw_handle(),
                job.0.as_raw_handle(),
                &mut assigned,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(assigned != 0)
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        // SAFETY: actual retained process handle, never a numerical PID signal.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_OBJECT_0 => self.status().map(Some),
            WAIT_TIMEOUT => Ok(None),
            _ => Err(io::Error::last_os_error()),
        }
    }
    fn status(&self) -> io::Result<ExitStatus> {
        let mut code = 0;
        // SAFETY: live owned process and correctly sized writable output.
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(ExitStatus::from_raw(code))
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        // SAFETY: this exact process remains owned until actual termination.
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }
        self.status()
    }
    pub fn kill(&mut self) -> io::Result<()> {
        if self.try_wait()?.is_some() {
            return Ok(());
        }
        // SAFETY: this exact process handle cannot be rebound through PID reuse.
        if unsafe { TerminateProcess(self.process.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
impl Drop for AtomicJobChild {
    fn drop(&mut self) {
        // The enclosing job owner separately drains every descendant. This
        // child handle is retained until its own actual native exit is known.
        loop {
            if self.try_wait().is_ok_and(|status| status.is_some()) {
                break;
            }
            let _ = self.kill();
            if self.wait().is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

#[cfg(test)]
#[path = "child_job_named_tests.rs"]
mod tests;
