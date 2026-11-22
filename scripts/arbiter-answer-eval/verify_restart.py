# SPDX-License-Identifier: Apache-2.0
"""Verify a lifetime restarted under a recorded close exception before any replay or session uses it.

The lifetime's handle must carry ``restart_exception`` (fixture.py ``--accept-unidle-close``).
The script then requires:

* every named repository settled (``settled.check``) with exactly the expected publication token.
  The token embeds the index generation, so equal tokens mean equal generations; the generation is
  also compared on its own;
* ``arbiter status-legacy`` showing Focus and Context with nothing queued or running in three
  consecutive samples, 10 s apart, after the handle's warm-up (``semantic_ready_utc``) and within
  ``--seconds``.

The receipt ``post-restart-verification.json`` is written beside the handle; fixture.py attaches it to
the handle when the lifetime closes. Exit 0 only when every check passed; otherwise stop and hand back.

``python -I -B verify_restart.py --expect airk=<token> --expect bookbuilder=<token>``
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


def generation(token: str | None) -> int | None:
    """``airk-r1:<index>:<generation>:<digest>`` -> generation."""
    parts = (token or "").split(":")
    return int(parts[2]) if len(parts) == 4 and parts[2].isdigit() else None


def stage_counts(handle: common.Handle, timeout_s: float = 60.0) -> dict | None:
    proc = subprocess.run([*handle.cli, "--output", "json", "--service-runtime-dir", handle.control,
                           "arbiter", "status-legacy"], capture_output=True, stdin=subprocess.DEVNULL,
                          env=common.tool_env(handle), timeout=timeout_s,
                          creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    try:
        state = json.loads(proc.stdout) if proc.returncode == 0 else None
    except ValueError:
        state = None
    if not isinstance(state, dict):
        return None
    return {kind: {"queued": (state.get(kind) or {}).get("queued"), "running": (state.get(kind) or {}).get("running")}
            for kind in ("focus", "context")}


def idle(counts: dict | None) -> bool:
    return bool(counts) and all(c["queued"] == 0 and c["running"] == 0 for c in counts.values())


def verify(handle_path: Path, expected: dict[str, str], seconds: float, sampler=stage_counts, checker=settled.check,
           sleep=time.sleep, samples_needed: int = 3) -> dict:
    raw = common.read_json(handle_path)
    result = {"schema": "airk.private.answer-eval-post-restart-verification/1", "at": common.now_iso(),
              "lifetime": raw.get("lifetime"), "restart_from": raw.get("restart_from"),
              "restart_exception": (raw.get("restart_exception") or {}).get("receipt"), "expected": expected,
              "problems": []}
    if not raw.get("restart_exception"):
        result["problems"].append("the handle carries no restart_exception")
    ok, snaps = checker(str(handle_path), handle_path.parent.parent, sorted(expected), False)
    result["snapshots"] = [{k: s.get(k) for k in ("repo", "publication_token", "settled", "problems", "at")}
                           for s in snaps]
    if not ok:
        result["problems"].append("not settled")
    for snap in snaps:
        want = expected.get(snap.get("repo"))
        got = snap.get("publication_token")
        if got != want:
            result["problems"].append(f"{snap.get('repo')}: token {got} differs from {want}")
        if generation(got) is None or generation(got) != generation(want):
            result["problems"].append(f"{snap.get('repo')}: generation {generation(got)} differs from "
                                      f"{generation(want)}")
    handle = common.load_handle(handle_path)
    rows, streak = [], 0
    end = time.monotonic() + seconds
    while streak < samples_needed and time.monotonic() < end:
        counts = sampler(handle)
        rows.append({"at": common.now_iso(), "counts": counts})
        streak = streak + 1 if idle(counts) else 0
        if streak < samples_needed:
            sleep(10)
    result["stage_samples"] = rows
    if streak < samples_needed:
        result["problems"].append(f"stages not idle in {samples_needed} consecutive samples within {seconds} s")
    result["passed"] = not result["problems"]
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--handle", default=common.NEWEST)
    parser.add_argument("--expect", action="append", required=True, metavar="REPO=TOKEN")
    parser.add_argument("--seconds", type=float, default=600.0)
    args = parser.parse_args(argv)
    path = common.resolve_handle(args.handle, common.DEFAULT_R / "fixture")
    expected = dict(item.split("=", 1) for item in args.expect)
    result = verify(path, expected, args.seconds)
    common.write_json(path.parent / "post-restart-verification.json", result)
    print(json.dumps({"passed": result["passed"], "problems": result["problems"]}))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
