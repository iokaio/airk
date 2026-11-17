# SPDX-License-Identifier: Apache-2.0
def run_windows_owned(argv, cwd, output, timeout, environment):
    from ctypes import wintypes as w
    import msvcrt

    class Startup(ctypes.Structure):
        _fields_ = [("cb", w.DWORD), ("reserved", w.LPWSTR), ("desktop", w.LPWSTR),
                    ("title", w.LPWSTR), ("x", w.DWORD), ("y", w.DWORD), ("xsize", w.DWORD),
                    ("ysize", w.DWORD), ("xchars", w.DWORD), ("ychars", w.DWORD),
                    ("fill", w.DWORD), ("flags", w.DWORD), ("show", w.WORD), ("reserved2", w.WORD),
                    ("reserved_bytes", ctypes.c_void_p), ("stdin", w.HANDLE), ("stdout", w.HANDLE), ("stderr", w.HANDLE)]
    class ExtendedStartup(ctypes.Structure):
        _fields_ = [("startup", Startup), ("attributes", ctypes.c_void_p)]
    class ProcessInfo(ctypes.Structure):
        _fields_ = [("process", w.HANDLE), ("thread", w.HANDLE), ("pid", w.DWORD), ("tid", w.DWORD)]
    class Limits(ctypes.Structure):
        _fields_ = [("process_time", ctypes.c_longlong), ("job_time", ctypes.c_longlong),
                    ("flags", w.DWORD), ("min_working", ctypes.c_size_t), ("max_working", ctypes.c_size_t),
                    ("active_limit", w.DWORD), ("affinity", ctypes.c_size_t), ("priority", w.DWORD), ("scheduling", w.DWORD)]
    class IO(ctypes.Structure):
        _fields_ = [(name, ctypes.c_ulonglong) for name in ("read_ops", "write_ops", "other_ops", "read_bytes", "write_bytes", "other_bytes")]
    class ExtendedLimits(ctypes.Structure):
        _fields_ = [("basic", Limits), ("io", IO), ("process_memory", ctypes.c_size_t), ("job_memory", ctypes.c_size_t),
                    ("peak_process", ctypes.c_size_t), ("peak_job", ctypes.c_size_t)]
    class JobAccounting(ctypes.Structure):
        _fields_ = [(name, ctypes.c_longlong) for name in ("user", "kernel", "period_user", "period_kernel")] + [(name, w.DWORD) for name in ("faults", "total", "active", "terminated")]

    api = ctypes.WinDLL("kernel32", use_last_error=True)
    signatures = {
        "CreateJobObjectW": ([ctypes.c_void_p, w.LPCWSTR], w.HANDLE),
        "SetInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD], w.BOOL),
        "AssignProcessToJobObject": ([w.HANDLE, w.HANDLE], w.BOOL),
        "QueryInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.c_void_p], w.BOOL),
        "TerminateJobObject": ([w.HANDLE, w.UINT], w.BOOL),
        "TerminateProcess": ([w.HANDLE, w.UINT], w.BOOL),
        "CloseHandle": ([w.HANDLE], w.BOOL),
        "CreateProcessW": ([w.LPCWSTR, w.LPWSTR, ctypes.c_void_p, ctypes.c_void_p, w.BOOL, w.DWORD, ctypes.c_void_p, w.LPCWSTR, ctypes.c_void_p, ctypes.c_void_p], w.BOOL),
        "WaitForSingleObject": ([w.HANDLE, w.DWORD], w.DWORD),
        "GetExitCodeProcess": ([w.HANDLE, ctypes.POINTER(w.DWORD)], w.BOOL),
        "ResumeThread": ([w.HANDLE], w.DWORD),
        "InitializeProcThreadAttributeList": ([ctypes.c_void_p, w.DWORD, w.DWORD, ctypes.POINTER(ctypes.c_size_t)], w.BOOL),
        "UpdateProcThreadAttribute": ([ctypes.c_void_p, w.DWORD, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_void_p], w.BOOL),
        "DeleteProcThreadAttributeList": ([ctypes.c_void_p], None),
    }
    for name, (arguments, result) in signatures.items():
        getattr(api, name).argtypes = arguments
        getattr(api, name).restype = result
    def checked(value):
        if not value:
            raise ctypes.WinError(ctypes.get_last_error())
        return value
    job = checked(api.CreateJobObjectW(None, None))
    info = ProcessInfo()
    assigned = resumed = False
    attributes_initialized = False
    try:
        limits = ExtendedLimits()
        limits.basic.flags = 0x2000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        checked(api.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)))
        # Inherit exactly the output and null-input handles, not the runner's
        # account/token/pipe handles. Parent is never assigned to this job.
        with open(os.devnull, "rb") as input_file:
            handles = (w.HANDLE * 2)(msvcrt.get_osfhandle(input_file.fileno()), msvcrt.get_osfhandle(output.fileno()))
            for handle in handles:
                os.set_handle_inheritable(handle, True)
            size = ctypes.c_size_t()
            api.InitializeProcThreadAttributeList(None, 1, 0, ctypes.byref(size))
            buffer = ctypes.create_string_buffer(size.value)
            checked(api.InitializeProcThreadAttributeList(buffer, 1, 0, ctypes.byref(size)))
            attributes_initialized = True
            checked(api.UpdateProcThreadAttribute(buffer, 0, 0x20002, handles, ctypes.sizeof(handles), None, None))
            startup = ExtendedStartup()
            startup.startup.cb = ctypes.sizeof(startup)
            startup.startup.flags = 0x100  # STARTF_USESTDHANDLES
            startup.startup.stdin, startup.startup.stdout = handles[0], handles[1]
            startup.startup.stderr = handles[1]
            startup.attributes = ctypes.cast(buffer, ctypes.c_void_p)
            environment_block = None
            if environment is not None:
                environment_block = ctypes.create_unicode_buffer("\0".join(key + "=" + value for key, value in sorted(environment.items(), key=lambda row: row[0].upper())) + "\0\0")
            command = ctypes.create_unicode_buffer(subprocess.list2cmdline(argv))
            try:
                checked(api.CreateProcessW(None, command, None, None, True,
                        0x4 | 0x8000000 | 0x80000 | 0x400, environment_block, str(cwd), ctypes.byref(startup), ctypes.byref(info)))
            finally:
                for handle in handles:
                    os.set_handle_inheritable(handle, False)
            # The child is suspended: it cannot spawn an unowned compiler in
            # the interval between creation and native job assignment.
            checked(api.AssignProcessToJobObject(job, info.process))
            assigned = True
            if api.ResumeThread(info.thread) == 0xFFFFFFFF:
                raise ctypes.WinError(ctypes.get_last_error())
            resumed = True
        end = time.monotonic() + timeout
        timed_out = oversized = False
        while True:
            waited = api.WaitForSingleObject(info.process, 50)
            if waited not in (0, 258):
                raise ctypes.WinError(ctypes.get_last_error())
            oversized = os.fstat(output.fileno()).st_size > MAX_LOG
            timed_out = time.monotonic() >= end
            if waited == 0 or timed_out or oversized:
                break
        code = w.DWORD()
        checked(api.GetExitCodeProcess(info.process, ctypes.byref(code)))
        accounting = JobAccounting()
        checked(api.QueryInformationJobObject(job, 1, ctypes.byref(accounting), ctypes.sizeof(accounting), None))
        diagnostic = census(api, job, info, accounting)
        forced = accounting.active != 0
        if forced:
            checked(api.TerminateJobObject(job, 77))
        cleanup_end = time.monotonic() + 5
        while True:
            checked(api.QueryInformationJobObject(job, 1, ctypes.byref(accounting), ctypes.sizeof(accounting), None))
            if accounting.active == 0:
                break
            if time.monotonic() >= cleanup_end:
                raise ValueError("Owned static-build descendants did not settle; retain all evidence")
            time.sleep(0.05)
        checked(api.GetExitCodeProcess(info.process, ctypes.byref(code)))
        return {"exit_code": int(code.value), "timed_out": timed_out, "log_oversized": oversized,
                "owned_descendants_settled": True, "owned_job_forced_termination": forced, "decision_census": diagnostic,
                "parent_assigned_to_job": False, "child_assigned_before_resume": True}
    finally:
        if info.process:
            if assigned:
                api.TerminateJobObject(job, 77)
            elif not resumed:
                api.TerminateProcess(info.process, 77)
            api.WaitForSingleObject(info.process, 5000)
        if attributes_initialized:
            api.DeleteProcThreadAttributeList(buffer)
        for handle in (info.thread, info.process, job):
            if handle:
                api.CloseHandle(handle)
