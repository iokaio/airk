# SPDX-License-Identifier: Apache-2.0
"""Interrupt only the authenticated native compiler or archiver when its volume limit is reached."""
import argparse
import json
import os
from pathlib import Path
import runpy
import signal

from checkpoint_ort import BUILD, ROOT, owner, processes
from configure_ort import CPU, CPU_SHA, sha
from finish_ort import AGGREGATE


def native_command(stage):
    ninja, provider = owner()
    if stage == "provider":
        exe, argv = ninja, provider
    else:
        aggregate = CPU.with_name("static-onnx-aggregate.py")
        if sha(CPU) != CPU_SHA or sha(aggregate) != AGGREGATE:
            raise ValueError("unreviewed runtime archive owners")
        if stage == "core":
            core = runpy.run_path(str(CPU))
            rows = runpy.run_path(str(aggregate))["runtime_closure"](
                (BUILD / "build.ninja").read_text(), core["STATIC_COMPONENTS"], False, core["relative_name"])
            exe, argv = ninja, [*provider[:-1], *rows]
        elif stage == "archive":
            exe = Path("/usr/bin/ar").resolve(strict=True)
            argv = [str(exe), "-M"]
        else:
            raise ValueError("unknown native stage")
    return exe, argv


def stop(stage):
    exe, argv = native_command(stage)
    candidates = processes(argv)
    for proc in candidates:
        pid = int(proc.name)
        fd = os.pidfd_open(pid)
        try:
            if (os.getsid(pid) != pid or os.getpgid(pid) != pid or not (proc/"exe").samefile(exe)
                    or not (proc/"cwd").samefile(ROOT if stage == "archive" else BUILD) or proc not in processes(argv)):
                continue
            if stage == "archive" and not (proc/"fd/0").samefile(ROOT/"aggregate/inputs.mri"):
                continue
            signal.pidfd_send_signal(fd, signal.SIGINT)
            return {"stage":stage, "signalled":pid, "argv":argv}
        finally:
            os.close(fd)
    raise ValueError("pending or settled exact native process unavailable for checkpoint")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--stage", choices=("provider", "core", "archive"), required=True)
    print(json.dumps(stop(p.parse_args().stage)))
