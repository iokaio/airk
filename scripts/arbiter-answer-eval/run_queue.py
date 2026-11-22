# SPDX-License-Identifier: Apache-2.0
"""Run several pass specs one at a time, each as soon as its indexes are settled.

The passes share one service and its stages, so only one runs at a time. The
queue checks every waiting spec's ``settle_repos`` with ``settled.check`` and
starts the first spec that is settled (``run_baseline.py``, with its own
guard). It repeats until every spec has run, or until ``--wait`` runs out. A
spec that fails stops the queue, and the failure is recorded in
``--log``.

``python -I -B run_queue.py --specs <a>/spec.json,<b>/spec.json [--depth <spec>=<out dir>] --log <file>``
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import settled  # noqa: E402


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--specs", required=True)
    parser.add_argument("--depth", action="append", default=[], help="<spec path>=<depth output folder>")
    parser.add_argument("--log", type=Path, required=True)
    parser.add_argument("--wait", type=float, default=10 * 3600.0)
    parser.add_argument("--interval", type=float, default=60.0)
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    args = parser.parse_args(argv)
    waiting = [Path(s) for s in args.specs.split(",")]
    depth = dict(item.split("=", 1) for item in args.depth)
    end = time.monotonic() + args.wait

    def log(**event):
        common.append_jsonl(args.log, {"at": common.now_iso(), **event})
        print(json.dumps(event), flush=True)

    while waiting:
        started = None
        for spec_path in waiting:
            spec = common.read_json(spec_path)
            repos = spec.get("settle_repos") or spec.get("repos")
            ok, snaps = settled.check(spec["handle"], args.r_root / "fixture", repos, False)
            if ok:
                started = spec_path
                break
        if started is None:
            if time.monotonic() >= end:
                log(event="timeout", waiting=[str(s) for s in waiting])
                return 2
            time.sleep(args.interval)
            continue
        command = [sys.executable, "-I", "-B", str(HERE / "run_baseline.py"), "--spec", str(started),
                   "--interval", "30"]
        if str(started) in depth:
            command += ["--depth", depth[str(started)]]
        log(event="start", spec=str(started), command=command)
        began = time.monotonic()
        proc = subprocess.run(command, stdin=subprocess.DEVNULL, cwd=str(common.REPO_ROOT),
                              creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        log(event="finished", spec=str(started), exit_code=proc.returncode,
            elapsed_s=round(time.monotonic() - began, 1))
        if proc.returncode != 0:
            return 1
        waiting.remove(started)
    log(event="done")
    return 0


if __name__ == "__main__":
    sys.exit(main())
