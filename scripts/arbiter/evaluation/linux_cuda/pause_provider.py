# SPDX-License-Identifier: Apache-2.0
"""Briefly stop and resume the exact campaign-owned provider compiler session."""
import argparse
import json
import os
from pathlib import Path
import signal
import time

import checkpoint_ort
from stop_native_stage import native_command

WINDOWS = {"compact": (80, "r1"), "large": (120, "r2"), "compact-second": (80, "r3")}


def members(leader):
    result = {}
    for item in Path("/proc").iterdir():
        if not item.name.isdigit():
            continue
        try:
            fields = (item / "stat").read_text().rsplit(")", 1)[1].split()
            if int(fields[3]) != leader:
                continue
            if item.stat().st_uid != os.getuid():
                raise ValueError("unexpected owner in native build session")
            result[int(item.name)] = (fields[0], fields[19])
        except (FileNotFoundError, ProcessLookupError):
            continue
    return result


def stop_window(label, stage):
    if label in WINDOWS:
        limit, suffix = WINDOWS[label]
    else:
        kind, ordinal = label.split("-r")
        if kind not in ("compact", "large") or not ordinal.isdigit() or not 4 <= int(ordinal) <= 99:
            raise ValueError("expected an approved finite native diagnostic window")
        limit, suffix = (80 if kind == "compact" else 120), "r" + ordinal
    end = checkpoint_ort.CAMPAIGN / (f"linux-cuda-r4-{stage}-release-window-" + suffix + ".signal")
    exe, argv = native_command(stage)
    candidates = checkpoint_ort.processes(argv)
    if len(candidates) != 1 or end.exists():
        raise ValueError("one active native build and fresh resume signal required")
    proc = candidates[0]
    leader = int(proc.name)
    if (os.getsid(leader) != leader or os.getpgid(leader) != leader or not (proc / "exe").samefile(exe)
            or not (proc / "cwd").samefile(checkpoint_ort.BUILD) or proc not in checkpoint_ort.processes(argv)):
        raise ValueError("native Ninja identity differs")
    held = {}
    start = time.monotonic()
    try:
        while time.monotonic() - start < 4:
            rows = members(leader)
            if leader not in rows:
                raise ValueError("lost original native build session")
            for pid, (_, born) in rows.items():
                if pid in held:
                    continue
                try:
                    fd = os.pidfd_open(pid)
                except ProcessLookupError:
                    continue
                try:
                    current = members(leader).get(pid)
                    if current is None or current[1] != born:
                        continue
                    if current[0] == "T":
                        raise ValueError("native process previously suspended")
                    signal.pidfd_send_signal(fd, signal.SIGSTOP)
                    held[pid] = fd
                finally:
                    if pid not in held:
                        os.close(fd)
            again = members(leader)
            if all(state == "T" and pid in held for pid,(state,_) in again.items()) and again == rows:
                mem = {}
                for pid in again:
                    fields = Path(f"/proc/{pid}/status").read_text().splitlines()
                    mem[pid] = {n.split(":")[0]:int(n.split()[1])*1024 for n in fields
                                if n.startswith(("VmRSS:", "VmSize:", "VmData:", "VmSwap:"))}
                print(json.dumps({"frozen":True,"utc":time.time(),"pids":mem,"leader":leader,
                                  "deadline_utc":time.time()+limit-(time.monotonic()-start)}),flush=True)
                while time.monotonic() - start < limit and not end.exists():
                    time.sleep(0.1)
                break
            time.sleep(0.02)
        else:
            raise ValueError("native compiler session failed to quiesce")
    finally:
        for pid,fd in reversed(list(held.items())):
            try:
                signal.pidfd_send_signal(fd, signal.SIGCONT)
            except ProcessLookupError:
                pass
            finally:
                os.close(fd)
        print(json.dumps({"resumed":True,"elapsed":time.monotonic()-start}),flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("label", nargs="?", default="compact")
    parser.add_argument("--stage", choices=("provider", "core"), default="provider")
    options = parser.parse_args()
    stop_window(options.label, options.stage)
