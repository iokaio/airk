# SPDX-License-Identifier: Apache-2.0
"""Sample WDDM counters and host memory using the original Windows child handle."""

import ctypes as ct
from ctypes import wintypes as wt
import math
from pathlib import Path
import os
import re
import time


class RawCounter(ct.Structure):
    _fields_ = [("status", wt.DWORD), ("timestamp", wt.FILETIME), ("first", ct.c_int64),
                ("second", ct.c_int64), ("multiple", wt.DWORD)]


class ProcessMemory(ct.Structure):
    _fields_ = [("size", wt.DWORD), ("page_faults", wt.DWORD)] + [
        (name, ct.c_size_t) for name in ("peak_working_set", "working_set", "paged_pool_peak",
                                         "paged_pool", "nonpaged_pool_peak", "nonpaged_pool",
                                         "pagefile", "pagefile_peak", "private_usage")]


def bind(dll, name, result, *args):
    func = getattr(dll, name)
    func.restype = result
    func.argtypes = args
    return func


def check(status, operation):
    if status:
        raise OSError(f"{operation} failed with {status:#010x}")


class ProcessSampler:
    def __init__(self, info, adapter_luid, *, segment_fields=False):
        if os.name != "nt" or not re.fullmatch(r"luid_0x[0-9a-f]{8}_0x[0-9a-f]{8}", adapter_luid):
            raise ValueError("verified Windows adapter LUID required")
        system = Path(os.environ["SystemRoot"]) / "System32"
        pdh = ct.WinDLL(str(system / "pdh.dll"))
        kernel = ct.WinDLL(str(system / "kernel32.dll"), use_last_error=True)
        self.handle = info.process
        self.pid = int(info.pid)
        pid_of = bind(kernel, "GetProcessId", wt.DWORD, wt.HANDLE)
        if self.pid <= 0 or pid_of(self.handle) != self.pid:
            raise ValueError("original process handle and PID differ")
        self.memory = bind(kernel, "K32GetProcessMemoryInfo", wt.BOOL,
                           wt.HANDLE, ct.POINTER(ProcessMemory), wt.DWORD)
        self.collect = bind(pdh, "PdhCollectQueryData", wt.DWORD, wt.HANDLE)
        self.get = bind(pdh, "PdhGetRawCounterValue", wt.DWORD, wt.HANDLE,
                        ct.POINTER(wt.DWORD), ct.POINTER(RawCounter))
        self.close_query = bind(pdh, "PdhCloseQuery", wt.DWORD, wt.HANDLE)
        open_query = bind(pdh, "PdhOpenQueryW", wt.DWORD, wt.LPCWSTR,
                          ct.c_size_t, ct.POINTER(wt.HANDLE))
        add = bind(pdh, "PdhAddEnglishCounterW", wt.DWORD, wt.HANDLE, wt.LPCWSTR,
                   ct.c_size_t, ct.POINTER(wt.HANDLE))
        self.query = wt.HANDLE()
        self.counters = []
        self.segment_fields = segment_fields
        self.segment_unavailable = {}
        check(open_query(None, 0, ct.byref(self.query)), "PdhOpenQuery")
        try:
            instance = adapter_luid + "_phys_0"
            for group, obj, key in (("adapter", "GPU Adapter Memory", instance),
                                    ("process", "GPU Process Memory", f"pid_{self.pid}_{instance}")):
                for field in ("Dedicated Usage", "Shared Usage", "Total Committed"):
                    counter = wt.HANDLE()
                    path = f"\\{obj}({key})\\{field}"
                    check(add(self.query, path, 0, ct.byref(counter)), f"PdhAddEnglishCounter {path}")
                    self.counters.append((group, field, counter))
            if segment_fields:
                for field in ("Local Usage", "Non Local Usage"):
                    counter = wt.HANDLE()
                    path = f"\\GPU Process Memory(pid_{self.pid}_{instance})\\{field}"
                    code = add(self.query, path, 0, ct.byref(counter))
                    if code:
                        self.segment_unavailable[field] = code
                    else:
                        self.counters.append(("segment_bytes", field, counter))
        except BaseException:
            self.close()
            raise

    def close(self):
        if self.query:
            check(self.close_query(self.query), "PdhCloseQuery")
            self.query = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def sample(self):
        row = {"start_monotonic_ns": time.monotonic_ns(), "utc_unix_ns": time.time_ns(),
               "adapter": {}, "process": {}, "counter_filetime_100ns": {}}
        if self.segment_fields:
            row["segment_bytes"] = {"Local Usage":None, "Non Local Usage":None}
            row["segment_counter_status"] = self.segment_unavailable.copy()
        check(self.collect(self.query), "PdhCollectQueryData")
        for group, field, counter in self.counters:
            raw = RawCounter()
            status = self.get(counter, None, ct.byref(raw))
            code = status or raw.status
            if code in (0, 1):
                if raw.first < 0:
                    if group == "segment_bytes":
                        row.setdefault("segment_invalid_raw", {})[field] = raw.first
                        continue
                    raise ValueError("invalid negative memory counter")
                row[group][field] = raw.first
                row["counter_filetime_100ns"][group + "/" + field] = (
                    raw.timestamp.dwHighDateTime << 32) | raw.timestamp.dwLowDateTime
            elif group == "segment_bytes":
                row["segment_counter_status"][field] = code
            elif group == "process" and code in (0x800007d1, 0xc0000bba, 0xc0000bc6):
                # PDH reports no instance or invalid data before allocation/after release.
                row[group][field] = None
                row.setdefault("process_counter_status", {})[field] = code
            else:
                raise OSError(f"unavailable {group} {field}: {code:#010x}")
        memory = ProcessMemory()
        memory.size = ct.sizeof(memory)
        if not self.memory(self.handle, ct.byref(memory), memory.size):
            raise ct.WinError(ct.get_last_error())
        row.update(working_set_bytes=memory.working_set, peak_working_set_bytes=memory.peak_working_set,
                   private_commit_bytes=memory.private_usage, end_monotonic_ns=time.monotonic_ns())
        return row


def observe_process(api, info, deadline, adapter_luid, baseline=None, *, segment_fields=False):
    rows = []
    with ProcessSampler(info, adapter_luid, segment_fields=segment_fields) as sampler:
        while True:
            remaining_ms = math.floor((deadline - time.monotonic()) * 1000)
            if remaining_ms <= 0:
                raise TimeoutError("original Windows Job deadline reached")
            wait = api.WaitForSingleObject(info.process, 0)
            if wait == 0:
                break
            if wait != 258:
                raise OSError(f"process handle poll failed: {wait}")
            rows.append(sampler.sample())
            remaining_ms = math.floor((deadline - time.monotonic()) * 1000)
            if remaining_ms <= 0:
                raise TimeoutError("original Windows Job deadline reached")
            wait = api.WaitForSingleObject(info.process, min(100, remaining_ms))
            if wait == 0:
                break
            if wait != 258:
                raise OSError(f"process handle wait failed: {wait}")
    if time.monotonic() >= deadline:
        raise TimeoutError("process observation exceeded deadline")
    return {"pid": int(info.pid), "adapter_luid": adapter_luid, "before_resume": baseline,
            "observed_exit_monotonic_ns": time.monotonic_ns(), "samples": rows}
