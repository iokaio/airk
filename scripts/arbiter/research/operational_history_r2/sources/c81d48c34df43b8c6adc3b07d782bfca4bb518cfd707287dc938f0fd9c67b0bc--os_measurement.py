# SPDX-License-Identifier: Apache-2.0
"""Private read-only Win32 process-memory observation. Not a process owner.

Image bytes are authenticated by the existing fixture process observation and
outer pre/post source/image seal; each measurement authenticates the live handle
by actual creation FILETIME and actual image path, preventing PID reuse.
"""
import ctypes
from ctypes import wintypes as w
import os
from pathlib import Path
import time

def sample(expected, phase):
    if os.name != 'nt':
        raise OSError('this selected measurement requires Windows')
    api = ctypes.WinDLL('kernel32', use_last_error=True)
    psapi = ctypes.WinDLL('psapi', use_last_error=True)
    api.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
    api.OpenProcess.restype = w.HANDLE
    api.CloseHandle.argtypes = [w.HANDLE]
    api.CloseHandle.restype = w.BOOL
    api.GetProcessTimes.argtypes = [w.HANDLE] + [ctypes.POINTER(ctypes.c_uint64)] * 4
    api.GetProcessTimes.restype = w.BOOL
    api.QueryFullProcessImageNameW.argtypes = [w.HANDLE, w.DWORD, w.LPWSTR, ctypes.POINTER(w.DWORD)]
    api.QueryFullProcessImageNameW.restype = w.BOOL
    class Counters(ctypes.Structure):
        _fields_ = [('cb', w.DWORD), ('PageFaultCount', w.DWORD)] + [
            (name, ctypes.c_size_t) for name in ('PeakWorkingSetSize', 'WorkingSetSize',
                'QuotaPeakPagedPoolUsage', 'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage',
                'QuotaNonPagedPoolUsage', 'PagefileUsage', 'PeakPagefileUsage', 'PrivateUsage')]
    psapi.GetProcessMemoryInfo.argtypes = [w.HANDLE, ctypes.POINTER(Counters), w.DWORD]
    psapi.GetProcessMemoryInfo.restype = w.BOOL
    handle = api.OpenProcess(0x1000 | 0x10, False, expected['process_id'])
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        times = [ctypes.c_uint64() for _ in range(4)]
        if not api.GetProcessTimes(handle, *(ctypes.byref(value) for value in times)):
            raise ctypes.WinError(ctypes.get_last_error())
        if times[0].value != expected['creation_filetime_100ns']:
            raise ValueError('actual measured PID creation identity changed')
        image = ctypes.create_unicode_buffer(32768)
        size = w.DWORD(len(image))
        if not api.QueryFullProcessImageNameW(handle, 0, image, ctypes.byref(size)):
            raise ctypes.WinError(ctypes.get_last_error())
        if str(Path(image.value).resolve()).casefold() != str(Path(expected['executable']).resolve()).casefold():
            raise ValueError('actual measured process image changed')
        counters = Counters()
        counters.cb = ctypes.sizeof(counters)
        if not psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
            raise ctypes.WinError(ctypes.get_last_error())
        return {'schema': 'airk.private.composed-os-memory/1', 'phase': phase,
                'observed_unix_ms': time.time_ns() // 1000000,
                'actual_process_observation': expected,
                'handle_creation_filetime_100ns': times[0].value,
                'handle_executable': image.value,
                'working_set_bytes': int(counters.WorkingSetSize),
                'peak_working_set_bytes': int(counters.PeakWorkingSetSize),
                'pagefile_usage_bytes': int(counters.PagefileUsage),
                'peak_pagefile_usage_bytes': int(counters.PeakPagefileUsage),
                'private_usage_bytes': int(counters.PrivateUsage),
                'scope': 'Actual entire fixture-process counters; lifetime peaks include prior cases, not per-profile attribution or sampled transient maxima.'}
    finally:
        if not api.CloseHandle(handle):
            raise ctypes.WinError(ctypes.get_last_error())
