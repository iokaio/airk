# SPDX-License-Identifier: Apache-2.0
"""Take one read-only Windows A6000 capacity snapshot for a campaign phase."""

import argparse
import csv
import ctypes
from ctypes import wintypes
import datetime as dt
import io
import json
import os
from pathlib import Path
import subprocess
import time
import uuid


class AdapterDescription(ctypes.Structure):
    _fields_ = [
        ("description", ctypes.c_wchar * 128), ("vendor", wintypes.UINT),
        ("device", wintypes.UINT), ("subsystem", wintypes.UINT), ("revision", wintypes.UINT),
        ("dedicated_video", ctypes.c_size_t), ("dedicated_system", ctypes.c_size_t),
        ("shared_system", ctypes.c_size_t), ("luid_low", wintypes.DWORD),
        ("luid_high", wintypes.LONG), ("flags", wintypes.UINT),
    ]


def method(interface, ordinal, result, *args):
    table = ctypes.cast(interface, ctypes.POINTER(ctypes.POINTER(ctypes.c_void_p))).contents
    return ctypes.WINFUNCTYPE(result, ctypes.c_void_p, *args)(table[ordinal])


def dxgi_adapters(system):
    # IDXGIFactory1::EnumAdapters1 and IDXGIAdapter1::GetDesc1 retain the
    # adapter LUID used by the Windows GPU performance counters.
    dll = ctypes.WinDLL(str(system / "dxgi.dll"))
    create = dll.CreateDXGIFactory1
    create.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
    create.restype = wintypes.LONG
    iid = uuid.UUID("770aae78-f26f-4dba-a829-253c83d1b387")
    factory = ctypes.c_void_p()
    if create(iid.bytes_le, ctypes.byref(factory)) != 0:
        raise OSError("DXGI factory unavailable")
    selected = []
    observed = []
    try:
        enumerate_adapter = method(factory, 12, wintypes.LONG, wintypes.UINT,
                                   ctypes.POINTER(ctypes.c_void_p))
        for ordinal in range(64):
            adapter = ctypes.c_void_p()
            result = enumerate_adapter(factory, ordinal, ctypes.byref(adapter))
            if result & 0xffffffff == 0x887a0002:
                break
            if result != 0:
                raise OSError(f"DXGI enumeration failed: {result:#x}")
            try:
                desc = AdapterDescription()
                if method(adapter, 10, wintypes.LONG, ctypes.POINTER(AdapterDescription))(
                        adapter, ctypes.byref(desc)) != 0:
                    raise OSError("DXGI adapter description unavailable")
                observed.append((desc.description, desc.vendor, desc.device, desc.luid_high, desc.luid_low, desc.flags))
                if desc.vendor == 0x10de and desc.device == 0x2230 and desc.description == "NVIDIA RTX A6000":
                    selected.append({"name": desc.description, "vendor": desc.vendor, "device": desc.device,
                                     "luid": f"luid_0x{desc.luid_high & 0xffffffff:08x}_0x{desc.luid_low:08x}",
                                     "dedicated_video_bytes": desc.dedicated_video})
            finally:
                method(adapter, 2, wintypes.ULONG)(adapter)
        else:
            raise ValueError("DXGI enumeration exceeds bound")
    finally:
        method(factory, 2, wintypes.ULONG)(factory)
    if not selected:
        raise ValueError(f"no matching A6000; observed {observed}")
    return selected


def snapshot(pid):
    if os.name != "nt" or (pid is not None and pid <= 0):
        raise ValueError("Windows and a positive process ID are required")
    system = Path(os.environ["SystemRoot"]) / "System32"
    start = time.monotonic()
    timestamp = dt.datetime.now(dt.timezone.utc).isoformat()
    candidates = dxgi_adapters(system)
    fields = ["name", "uuid", "driver_version", "driver_model.current", "memory.total", "memory.used", "memory.free"]
    raw = subprocess.check_output([system / "nvidia-smi.exe", "--query-gpu=" + ",".join(fields),
                                   "--format=csv,noheader,nounits"], text=True, timeout=10)
    cards = [dict(zip(fields, (s.strip() for s in row), strict=True)) for row in csv.reader(io.StringIO(raw))]
    card, = [item for item in cards if item["name"] == "NVIDIA RTX A6000"]
    if card["uuid"] != "GPU-08e2b3f6-6a39-3456-77cb-a57287a702ec" or card["driver_model.current"] != "WDDM":
        raise ValueError("campaign device or memory accounting mode differs")
    # Remote desktop can expose an extra DXGI description for the same GPU.
    # Bind only when exactly one matching LUID has a physical adapter counter.
    instances = [item["luid"] + "_phys_0" for item in candidates]
    names = ",".join("'" + name + "'" for name in instances)
    perf = "Win32_PerfRawData_GPUPerformanceCounters_GPU"
    query = (f"$names=@({names});$a=@(Get-CimInstance {perf}AdapterMemory | "
             "Where-Object { $_.Name -in $names });"
             "if($a.Count -ne 1){throw 'unique physical adapter counter required'};"
             "$p=$null;")
    if pid is not None:
        query += (f"$name='pid_{pid}_'+$a[0].Name;$r=@(Get-CimInstance {perf}ProcessMemory | "
                  "Where-Object { $_.Name -eq $name });"
                  "if($r.Count -ne 1){throw 'unique process row required'};$p=$r[0];")
    query += ("function row($x){if($null -ne $x){$x | Select-Object Name,DedicatedUsage,SharedUsage,TotalCommitted}};"
              "@{adapter=(row $a[0]);process=(row $p)} | ConvertTo-Json -Compress")
    powershell = system / "WindowsPowerShell/v1.0/powershell.exe"
    counters = json.loads(subprocess.check_output(
        [powershell, "-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop';" + query],
        text=True, timeout=15))
    adapter, = [a for a in candidates if a["luid"] + "_phys_0" == counters["adapter"]["Name"].lower()]
    if pid is None:
        counters["process"] = None
    elif counters["process"]["Name"].lower() != f"pid_{pid}_{adapter['luid']}_phys_0":
        raise ValueError("wrong process counter identity")
    for name in ("memory.total", "memory.used", "memory.free"):
        card[name] = int(card[name])
    return {"timestamp_utc": timestamp, "sample_elapsed_seconds": time.monotonic() - start,
            "dxgi": adapter, "nvidia_smi_mib": card, "wddm_bytes": counters, "requested_pid": pid}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int, help="live GPU process for an additional process-specific snapshot")
    print(json.dumps(snapshot(parser.parse_args().pid), allow_nan=False))
