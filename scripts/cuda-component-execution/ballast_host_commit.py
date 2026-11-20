# SPDX-License-Identifier: Apache-2.0
"""Read Windows host commit and the current worker's original Job accounting."""
import ctypes as ct
from ctypes import wintypes as wt


class Performance(ct.Structure):
    _fields_ = [("cb",wt.DWORD)] + [(name,ct.c_size_t) for name in
        ("commit","limit","peak","physical","available","cache","kernel","paged","nonpaged","page_size")] + [
        (name,wt.DWORD) for name in ("handles","processes","threads")]


class Basic(ct.Structure):
    _fields_ = [(name,ct.c_longlong) for name in ("process_time","job_time")] + [
        ("flags",wt.DWORD),("minimum",ct.c_size_t),("maximum",ct.c_size_t),("active",wt.DWORD),
        ("affinity",ct.c_size_t),("priority",wt.DWORD),("scheduling",wt.DWORD)]


class Extended(ct.Structure):
    _fields_ = [("basic",Basic),("io",ct.c_ulonglong*6)] + [(name,ct.c_size_t) for name in
        ("process_limit","job_limit","peak_process","peak_job")]


def host():
    kernel = ct.WinDLL("kernel32",use_last_error=True)
    info = Performance()
    info.cb = ct.sizeof(info)
    fn = kernel.K32GetPerformanceInfo
    fn.argtypes = [ct.POINTER(Performance),wt.DWORD]
    if not fn(ct.byref(info),info.cb):
        raise ct.WinError(ct.get_last_error())
    return {"commit_bytes":info.commit*info.page_size,"commit_limit_bytes":info.limit*info.page_size,
            "physical_available_bytes":info.available*info.page_size}


def worker(expected_limit=4 << 30):
    kernel = ct.WinDLL("kernel32",use_last_error=True)
    fn = kernel.QueryInformationJobObject
    fn.argtypes = [wt.HANDLE,ct.c_int,ct.c_void_p,wt.DWORD,ct.POINTER(wt.DWORD)]
    value = Extended()
    if not fn(None,9,ct.byref(value),ct.sizeof(value),None):
        raise ct.WinError(ct.get_last_error())
    if value.job_limit != expected_limit or value.process_limit != expected_limit:
        raise ValueError("original ballast worker Job limits differ")
    return dict(host(),job_peak_bytes=value.peak_job,process_peak_bytes=value.peak_process,
                job_limit_bytes=value.job_limit,process_limit_bytes=value.process_limit)


def reserve(observed, pending):
    remaining = observed["commit_limit_bytes"] - observed["commit_bytes"]
    if min(remaining,observed["physical_available_bytes"]) < pending + (8 << 30):
        raise ValueError("insufficient observed host commit or physical reserve")
