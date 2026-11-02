# SPDX-License-Identifier: Apache-2.0
"""Bounded runner capacity observations; never collect command lines or private state."""
import argparse
import ctypes
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess


def command(arguments):
    try:
        result = subprocess.run(arguments, text=True, encoding="utf-8", errors="replace",
                                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=10, check=False)
        return result.stdout if result.returncode == 0 else None
    except (OSError, subprocess.TimeoutExpired):
        return None


def snapshot():
    disk = shutil.disk_usage(Path.cwd())
    report = dict(schema="airk-ci-capacity/1", observed_utc=datetime.now(timezone.utc).isoformat(),
                  os=platform.system(), architecture=platform.machine(), logical_cpus=os.cpu_count(),
                  disk_total_bytes=disk.total, disk_free_bytes=disk.free,
                  memory=None, process_count=None, native_processes=[], native_processes_truncated=False)
    if os.name == "nt":
        class MemoryStatus(ctypes.Structure):
            _fields_ = [("length", ctypes.c_ulong), ("load", ctypes.c_ulong),
                        *[(name, ctypes.c_ulonglong) for name in
                          ("total", "available", "page_total", "page_available", "virtual_total", "virtual_available", "extended")]]
        memory = MemoryStatus()
        memory.length = ctypes.sizeof(memory)
        if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(memory)):
            report["memory"] = dict(total_bytes=memory.total, available_bytes=memory.available)
        # No executable paths, arguments, user identities or environment values.
        raw = command(["powershell.exe", "-NoProfile", "-NonInteractive", "-Command",
                       "$p=@(Get-Process); [pscustomobject]@{process_count=$p.Count; "
                       "native_processes=@($p | Where-Object {$_.ProcessName -match '^airk($|-)'} | "
                       "Select-Object -First 65 | ForEach-Object {"
                       "@{pid=$_.Id;name=$_.ProcessName;rss_bytes=$_.WorkingSet64}})} | ConvertTo-Json -Depth 4 -Compress"])
        if raw is not None:
            report.update(json.loads(raw))
    else:
        raw = command(["ps", "-axo", "pid=,ppid=,rss=,comm="])
        if raw is not None:
            rows = [line.split(None, 3) for line in raw.splitlines() if line.strip()]
            report["process_count"] = len(rows)
            for row in rows:
                if len(row) == 4 and Path(row[3]).name.startswith("airk"):
                    report["native_processes"].append(dict(pid=int(row[0]), ppid=int(row[1]),
                                                          rss_bytes=int(row[2]) * 1024, name=Path(row[3]).name))
        if platform.system() == "Darwin":
            raw = command(["vm_stat"])
            # Fixed kernel counters only; absence remains explicit.
            if raw is not None:
                report["memory"] = {"vm_stat": raw[:4096]}
        else:
            try:
                fields = dict(line.split(":", 1) for line in Path("/proc/meminfo").read_text().splitlines())
                report["memory"] = {"total_bytes": int(fields["MemTotal"].split()[0]) * 1024,
                                    "available_bytes": int(fields["MemAvailable"].split()[0]) * 1024}
            except (OSError, KeyError, ValueError):
                pass
    report["native_processes_truncated"] = len(report["native_processes"]) > 64
    report["native_processes"] = report["native_processes"][:64]
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(snapshot(), indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
