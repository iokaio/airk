# SPDX-License-Identifier: Apache-2.0
# PRIVATE derivative of exact maintained Windows multiprocess owner.
# Only job-memory enforcement and original absolute-deadline strengthening differ.
import ctypes
import math
import os
import subprocess
import time
MAX_LOG = 64 * 1024 * 1024

def run_windows_owned(argv, cwd, output, timeout, environment, *, absolute_deadline=None):
    """Drain actual owned members within one five-second cleanup deadline.

    Leader exit alone is not job settlement. Retain native member handles for
    event waits, then require fresh empty PID and accounting observations. A
    forced or unobserved cleanup remains a failed qualification; emergency
    finally cleanup never upgrades that result.
    """
    from ctypes import wintypes as w
    import msvcrt

    if absolute_deadline is not None and (type(absolute_deadline) not in (int, float) or not math.isfinite(absolute_deadline) or absolute_deadline <= 0):
        raise ValueError("Invalid original absolute job deadline")

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
        "OpenProcess": ([w.DWORD, w.BOOL, w.DWORD], w.HANDLE),
        "QueryFullProcessImageNameW": ([w.HANDLE, w.DWORD, w.LPWSTR, ctypes.POINTER(w.DWORD)], w.BOOL),
        "IsProcessInJob": ([w.HANDLE, w.HANDLE, ctypes.POINTER(w.BOOL)], w.BOOL),
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
    emergency_members = []
    observed_members = {}
    try:
        limits = ExtendedLimits()
        limits.basic.flags = 0x2000 | 0x200  # KILL_ON_JOB_CLOSE | JOB_MEMORY
        limits.job_memory = 8 << 30  # Original whole-transport committed-memory ceiling.
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
            if absolute_deadline is not None and time.monotonic() >= absolute_deadline:
                raise TimeoutError("Original job phase expired before child resume")
            if api.ResumeThread(info.thread) == 0xFFFFFFFF:
                raise ctypes.WinError(ctypes.get_last_error())
            resumed = True
        end = time.monotonic() + timeout
        if absolute_deadline is not None:
            end = min(end, absolute_deadline)
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
        # A signaled leader can leave a console host finishing natural shutdown.
        # Observe every actual job member under the SAME five-second cleanup
        # deadline; names/images never exempt a member from the empty-job proof.
        cleanup_end = time.monotonic() + 5
        accounting = JobAccounting()

        def job_members(deadline=None):
            deadline = cleanup_end if deadline is None else deadline
            capacity = 16
            while True:
                class Members(ctypes.Structure):
                    _fields_ = [("assigned", w.DWORD), ("listed", w.DWORD),
                                ("pids", ctypes.c_size_t * capacity)]
                members = Members()
                observed = api.QueryInformationJobObject(job, 3, ctypes.byref(members), ctypes.sizeof(members), None)
                if observed:
                    if members.listed > capacity:
                        raise ValueError("Owned job PID census was truncated")
                    return [int(members.pids[index]) for index in range(members.listed)]
                if ctypes.get_last_error() != 234:  # ERROR_MORE_DATA: grow, never truncate.
                    raise ctypes.WinError(ctypes.get_last_error())
                if time.monotonic() >= deadline:
                    raise ValueError("Owned job PID census exceeded cleanup deadline")
                capacity = max(capacity * 2, int(members.assigned))

        def observe_job():
            members = job_members()
            checked(api.QueryInformationJobObject(job, 1, ctypes.byref(accounting), ctypes.sizeof(accounting), None))
            return members, int(accounting.active)

        def retain_owned_members(members):
            retained = []
            try:
                for pid in members:
                    handle = api.OpenProcess(0x100000 | 0x1000, False, pid)
                    if not handle:
                        if ctypes.get_last_error() == 87:
                            continue  # Exited before open; recensus the job.
                        raise ctypes.WinError(ctypes.get_last_error())
                    belongs = w.BOOL()
                    try:
                        checked(api.IsProcessInJob(handle, job, ctypes.byref(belongs)))
                    except BaseException:
                        api.CloseHandle(handle)
                        raise
                    if belongs.value:
                        retained.append(handle)
                        # Diagnostic only; retain ownership BEFORE allocations
                        # and never let image observation replace cleanup proof.
                        if pid not in observed_members and len(observed_members) < 128:
                            try:
                                image = ctypes.create_unicode_buffer(32768)
                                image_size = w.DWORD(len(image))
                                image_ok = api.QueryFullProcessImageNameW(handle, 0, image, ctypes.byref(image_size))
                                observed_members[pid] = {
                                    "pid": pid, "image": image.value[:1024] if image_ok else None,
                                    "image_truncated": bool(image_ok and image_size.value > 1024),
                                    "image_error": 0 if image_ok else ctypes.get_last_error(),
                                    "cleanup_remaining_seconds": round(max(0, cleanup_end - time.monotonic()), 6)}
                            except Exception as observation_error:
                                observed_members[pid] = {"pid": pid, "image": None,
                                    "image_truncated": False, "image_error": None,
                                    "observer_error": type(observation_error).__name__}
                    else:
                        api.CloseHandle(handle)  # A reused foreign PID is never waited on.
                return retained
            except BaseException:
                for handle in retained:
                    api.CloseHandle(handle)
                raise

        forced = timed_out or oversized
        if forced:
            checked(api.TerminateJobObject(job, 77))
        while True:
            members, active = observe_job()
            if not members and active == 0:
                settled = True
                break
            if time.monotonic() >= cleanup_end:
                if not forced:
                    # Retain actual handles BEFORE forcing: immediate accounting
                    # can disappear before inherited log handles finish closing.
                    emergency_members.extend(retain_owned_members(members))
                    checked(api.TerminateJobObject(job, 77))
                    forced = True
                # No fresh grace period after forcing. An immediate fresh proof
                # can establish settlement; otherwise retain an explicit refusal.
                members, active = observe_job()
                settled = not members and active == 0
                break
            handles = retain_owned_members(members)
            try:
                for handle in handles:
                    remaining = cleanup_end - time.monotonic()
                    if remaining <= 0:
                        break
                    milliseconds = max(1, int(remaining * 1000))
                    observed = api.WaitForSingleObject(handle, milliseconds)
                    if observed not in (0, 258):
                        raise ctypes.WinError(ctypes.get_last_error())
                    if observed == 258:
                        break
            finally:
                for handle in handles:
                    api.CloseHandle(handle)
        checked(api.GetExitCodeProcess(info.process, ctypes.byref(code)))
        result = {"exit_code": int(code.value), "timed_out": timed_out, "log_oversized": oversized,
                  "owned_descendants_settled": settled, "owned_job_forced_termination": forced,
                  "owned_job_observed_member_images": list(observed_members.values()),
                  "owned_job_final_active_processes": active, "owned_job_final_member_pids": members,
                  "parent_assigned_to_job": False, "child_assigned_before_resume": True}
        if not settled:
            result["cleanup_refusal"] = "Owned job did not settle within the original five-second cleanup deadline"
        return result
    finally:
        # Emergency cleanup has its original single five-second deadline, shared
        # by ALL retained handles. It never upgrades the qualification result.
        emergency_end = time.monotonic() + 5
        try:
            if info.process:
                if assigned:
                    if "retain_owned_members" in locals():
                        emergency_members.extend(retain_owned_members(job_members(emergency_end)))
                    api.TerminateJobObject(job, 77)
                elif not resumed:
                    api.TerminateProcess(info.process, 77)
                for handle in (info.process, *emergency_members):
                    remaining = max(0, emergency_end - time.monotonic())
                    observed = api.WaitForSingleObject(handle, int(remaining * 1000))
                    if observed != 0:
                        raise ValueError("Emergency owned process exit was not observed; retain all evidence")
        finally:
            if attributes_initialized:
                api.DeleteProcThreadAttributeList(buffer)
            for handle in (*emergency_members, info.thread, info.process, job):
                if handle:
                    api.CloseHandle(handle)

