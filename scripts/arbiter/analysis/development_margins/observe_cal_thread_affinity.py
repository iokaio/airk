# SPDX-License-Identifier: Apache-2.0
"""Read Windows CAL worker affinity, thread CPU deltas and machine-wide logical CPU activity."""
import argparse
import ctypes
from ctypes import wintypes as W
import datetime
import json
from pathlib import Path
import struct
import time


P = ctypes.c_void_p
U64 = ctypes.c_uint64


class Group(ctypes.Structure):
    _fields_ = [("mask", U64), ("group", W.WORD), ("reserved", W.WORD * 3)]


class Processor(ctypes.Structure):
    _fields_ = [("group", W.WORD), ("number", W.BYTE), ("reserved", W.BYTE)]


class Thread(ctypes.Structure):
    _fields_ = [("size", W.DWORD), ("usage", W.DWORD), ("tid", W.DWORD), ("pid", W.DWORD),
                ("base_priority", W.LONG), ("delta_priority", W.LONG), ("flags", W.DWORD)]


class CpuTime(ctypes.Structure):
    _fields_ = [("idle", U64), ("kernel", U64), ("user", U64), ("dpc", U64), ("interrupt", U64), ("count", W.ULONG)]


def checked(ok):
    if not ok:
        raise ctypes.WinError(ctypes.get_last_error())
    return ok


def observe(pid, seconds):
    k = ctypes.WinDLL("kernel32", use_last_error=True)
    n = ctypes.WinDLL("ntdll")
    def sig(name, result, *args):
        f = getattr(k, name)
        f.argtypes, f.restype = args, result
        return f
    snapshot = sig("CreateToolhelp32Snapshot", P, W.DWORD, W.DWORD)
    first = sig("Thread32First", W.BOOL, P, P)
    next_thread = sig("Thread32Next", W.BOOL, P, P)
    open_thread = sig("OpenThread", P, W.DWORD, W.BOOL, W.DWORD)
    close = sig("CloseHandle", W.BOOL, P)
    times = sig("GetThreadTimes", W.BOOL, P, P, P, P, P)
    group = sig("GetThreadGroupAffinity", W.BOOL, P, P)
    ideal = sig("GetThreadIdealProcessorEx", W.BOOL, P, P)
    query = sig("QueryFullProcessImageNameW", W.BOOL, P, W.DWORD, P, P)
    process = checked(sig("OpenProcess", P, W.DWORD, W.BOOL, W.DWORD)(0x1000, False, pid))
    masks = sig("GetProcessAffinityMask", W.BOOL, P, P, P)
    topology = sig("GetLogicalProcessorInformationEx", W.BOOL, ctypes.c_int, P, P)
    priority = sig("GetPriorityClass", W.DWORD, P)
    n.NtQuerySystemInformation.argtypes = [W.ULONG, P, W.ULONG, P]
    n.NtQuerySystemInformation.restype = W.LONG
    def sample():
        handle = snapshot(4, 0)
        if handle == P(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        rows = {}
        try:
            entry = Thread()
            entry.size = ctypes.sizeof(entry)
            ok = first(handle, ctypes.byref(entry))
            while ok:
                if entry.pid == pid:
                    t = open_thread(0x40, False, entry.tid)
                    if t:
                        try:
                            a, b, c, d = (U64() for _ in range(4))
                            g, i = Group(), Processor()
                            if times(t, *[ctypes.byref(x) for x in (a, b, c, d)]) and group(t, ctypes.byref(g)) and ideal(t, ctypes.byref(i)):
                                rows[entry.tid] = (a.value, c.value + d.value, g.group, g.mask, i.group, i.number, entry.base_priority)
                        finally:
                            close(t)
                ok = next_thread(handle, ctypes.byref(entry))
        finally:
            close(handle)
        data = (CpuTime * 64)()
        size = W.ULONG()
        result = n.NtQuerySystemInformation(8, data, ctypes.sizeof(data), ctypes.byref(size))
        if result != 0 or size.value != ctypes.sizeof(data):
            raise ValueError("expected exactly one 64-logical-CPU group")
        return rows, [(r.idle, r.kernel + r.user) for r in data]
    try:
        name = ctypes.create_unicode_buffer(32768)
        size = W.DWORD(len(name))
        checked(query(process, 0, name, ctypes.byref(size)))
        if Path(name.value).name != "fresh-cu15-paired-answer-service-diagnostic.exe":
            raise ValueError("selected process is not the CAL service")
        mask, system = U64(), U64()
        checked(masks(process, ctypes.byref(mask), ctypes.byref(system)))
        size = W.DWORD()
        topology(0, None, ctypes.byref(size))
        buf = ctypes.create_string_buffer(size.value)
        checked(topology(0, buf, ctypes.byref(size)))
        raw = buf.raw[:size.value]
        cores, pos = [], 0
        while pos < len(raw):
            rel, length = struct.unpack_from("<II", raw, pos)
            if rel != 0 or length < 48 or struct.unpack_from("<H", raw, pos + 30)[0] != 1:
                raise ValueError("unexpected core group topology")
            bits, g = struct.unpack_from("<QH", raw, pos + 32)
            if g != 0:
                raise ValueError("unexpected second processor group")
            cores.append([i for i in range(64) if bits & (1 << i)])
            pos += length
        start = datetime.datetime.now(datetime.timezone.utc).isoformat()
        a, cpu_a = sample()
        time.sleep(seconds)
        b, cpu_b = sample()
        hot = []
        for tid in a.keys() & b.keys():
            old, new = a[tid], b[tid]
            if old[0] == new[0]:
                hot.append({"tid": tid, "cpu_seconds": (new[1] - old[1]) / 1e7, "group": new[2],
                            "mask": hex(new[3]), "ideal": [new[4], new[5]], "base_priority": new[6]})
        activity = [round(100 * (1 - (y[0] - x[0]) / (y[1] - x[1])), 1) for x, y in zip(cpu_a, cpu_b, strict=True)]
        return {"pid": pid, "image": name.value, "start_utc": start, "seconds": seconds, "process_mask": hex(mask.value),
                "system_mask": hex(system.value), "priority_class": priority(process), "physical_siblings": cores,
                "machine_cpu_busy_percent": activity, "initial_threads": len(a), "final_threads": len(b),
                "threads": sorted(hot, key=lambda r: -r["cpu_seconds"])}
    finally:
        close(process)


def pick_cpus(sample, count):
    """Prefer different physical cores outside observed active thread ideal cores."""
    used_ideals = {row["ideal"][1] for row in sample["threads"] if row["cpu_seconds"] > 0 and row["ideal"][0] == 0}
    activity = sample["machine_cpu_busy_percent"]
    allowed = int(sample["system_mask"], 16)
    choices = []
    for group in sample["physical_siblings"]:
        if used_ideals.intersection(group) or not all(allowed & (1 << cpu) for cpu in group):
            continue
        cpu = min(group, key=lambda i: (activity[i], i))
        choices.append((max(activity[i] for i in group), sum(activity[i] for i in group), cpu))
    if len(choices) < count:
        raise ValueError("insufficient distinct physical cores outside active thread ideal cores")
    return [item[2] for item in sorted(choices)[:count]]


def place_optional_worker(pid, count):
    """Restrict this new worker only; the sampled CAL process is opened for query."""
    sample = observe(pid, 10)
    cpus = pick_cpus(sample, count)
    requested = sum(1 << cpu for cpu in cpus)
    k = ctypes.WinDLL("kernel32", use_last_error=True)
    k.GetCurrentProcess.restype = P
    current = k.GetCurrentProcess()
    k.GetProcessAffinityMask.argtypes = [P, P, P]
    k.SetProcessAffinityMask.argtypes = [P, U64]
    k.SetPriorityClass.argtypes = [P, W.DWORD]
    k.GetPriorityClass.argtypes = [P]
    before, system = U64(), U64()
    checked(k.GetProcessAffinityMask(current, ctypes.byref(before), ctypes.byref(system)))
    if before.value & requested != requested:
        raise ValueError("selected CPUs are outside optional worker's allowed affinity")
    checked(k.SetPriorityClass(current, 0x4000))
    checked(k.SetProcessAffinityMask(current, requested))
    actual = U64()
    checked(k.GetProcessAffinityMask(current, ctypes.byref(actual), ctypes.byref(system)))
    priority = k.GetPriorityClass(current)
    if actual.value != requested or priority != 0x4000:
        raise ValueError("optional worker did not receive requested BelowNormal placement")
    return {"cal_observation": sample, "logical_cpus": cpus, "initial_worker_mask": hex(before.value),
            "worker_mask": hex(actual.value), "priority_class": priority}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--seconds", type=int, choices=range(2, 31), default=10)
    args = parser.parse_args()
    print(json.dumps(observe(args.pid, args.seconds), allow_nan=False))
