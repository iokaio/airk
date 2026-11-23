# SPDX-License-Identifier: Apache-2.0
"""Build one closed, restartable CPU base in a fresh fixture root (a shakedown or a gate spare root).

Evaluation tooling, never product. On 23 November 2026 the first vectors job of nearly every fresh base
failed at once (``project_operation_failed``, about 1 s after it started) and a restart of the base resumed
the embedding. So the restart is the normal path here: the base lifetime ``<name>-base`` is started; when it
ends on an error and is restartable, the occurrence is appended to ``--failures`` (root, time, error text,
the failed vectors job and whether another base was starting) and ``<name>-base<k>`` restarts from the
previous one, up to ``--restarts`` times. The base that reaches semantic readiness is stopped and waited
closed. The last closed lifetime is the root's restart origin; it is printed and written to
``<root>/base.json``.

One process owns each lifetime (``fixture.py start`` blocks until the lifetime closes); this driver only
starts, observes (``handle.json`` only) and stops them. On 23 November base s9 failed while starting with a
PermissionError on its own ``fixture/ready.json`` while a second watcher polled it; run one driver per root.

``python -I -B build_base.py --fixtures R/fixture-s10-shakedown --name s10-shakedown --image IMG --failures F``
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402

SCHEMA = "airk.private.answer-eval-base-build/1"
FAILURE_SCHEMA = "airk.private.answer-eval-first-vectors-failure/1"
PY = [sys.executable, "-I", "-B"]


def read(path: Path):
    try:
        return json.loads(Path(path).read_bytes())
    except (OSError, ValueError):
        return None


def failed_vectors_job(life: Path) -> dict | None:
    """The newest recorded progress observation of a failed vectors job in a lifetime's command outputs."""
    found = None
    for out in sorted((life / "commands").glob("*-progress.stdout")):
        text = out.read_text(encoding="utf-8", errors="replace")
        match = re.search(r'"job":\{"id":"([0-9a-f]+)"[^{}]*"kind":"vectors","state":"failed"[^{}]*'
                          r'"attempts":(\d+),"outcome":\{"state":"failed","code":"([a-z_]+)"', text)
        if match:
            stamp = re.search(r'"observed_unix_ms":(\d+)', text)
            found = {"job": match.group(1), "attempts": int(match.group(2)), "code": match.group(3),
                     "observed_unix_ms": int(stamp.group(1)) if stamp else None, "output": out.name}
    return found


def bases_starting(r_root: Path, own: Path) -> list[str]:
    """Other fixture roots whose newest lifetime is open (a base indexing or embedding), read-only."""
    busy = []
    for root in sorted(r_root.glob("fixture-*")):
        if root == own or not root.is_dir():
            continue
        for handle in root.glob("*/handle.json"):
            raw = read(handle)
            if isinstance(raw, dict) and raw.get("state") not in (None, "closed") and not raw.get("stop_reason"):
                busy.append(f"{root.name}/{handle.parent.name}")
    return busy


def run_lifetime(fixtures: Path, lifetime: str, image: str, origin: str | None, seconds: int, log: Path,
                 repositories: str = "munarium"):
    command = [*PY, str(HERE / "fixture.py"), "--fixtures", str(fixtures), "start", "--lifetime", lifetime,
               "--image", image, "--repositories", repositories, "--seconds", str(seconds)]
    if origin:
        command += ["--restart-from", origin]
    common.append_jsonl(log, {"at": common.now_iso(), "event": "start", "command": command})
    stream = (fixtures / f"{lifetime}.start.log").open("wb")
    return subprocess.Popen(command, stdout=stream, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                            cwd=str(common.REPO_ROOT))


def semantic_ready(handle: dict | None) -> bool:
    """As ``lg_bases.wait_verdict`` reads it: the lifetime is ready and its semantic publication is recorded."""
    handle = handle or {}
    return handle.get("state") == "ready" and bool(handle.get("semantic_ready_utc")) and not handle.get("error")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--fixtures", type=Path, required=True, help="a fresh, existing, empty root folder")
    parser.add_argument("--name", required=True, help="base lifetimes are <name>-base, <name>-base2, ...")
    parser.add_argument("--image", required=True)
    parser.add_argument("--failures", type=Path, required=True, help="JSONL of first-vectors failures (appended)")
    parser.add_argument("--repositories", default="munarium",
                        help="fixture.py order: a repository with denies first (bookbuilder-v2,munarium for a gate base)")
    parser.add_argument("--restarts", type=int, default=3)
    parser.add_argument("--seconds", type=int, default=7200)
    parser.add_argument("--poll", type=float, default=20.0)
    args = parser.parse_args(argv)
    fixtures = args.fixtures.resolve()
    common.require(fixtures.is_dir() and not any(fixtures.iterdir()), f"{fixtures} must exist and be empty")
    log = fixtures.parent / f"{fixtures.name}.build-base.jsonl"
    origin = None
    for attempt in range(1, args.restarts + 2):
        lifetime = f"{args.name}-base" + ("" if attempt == 1 else str(attempt))
        others = bases_starting(fixtures.parent, fixtures)
        proc = run_lifetime(fixtures, lifetime, args.image, origin, args.seconds, log, args.repositories)
        handle_path = fixtures / lifetime / "handle.json"
        stopped = False
        while proc.poll() is None:
            time.sleep(args.poll)
            handle = read(handle_path)
            if not stopped and semantic_ready(handle):
                subprocess.run([*PY, str(HERE / "fixture.py"), "--fixtures", str(fixtures), "stop",
                                "--lifetime", lifetime], check=True, stdin=subprocess.DEVNULL,
                               cwd=str(common.REPO_ROOT), capture_output=True)
                common.append_jsonl(log, {"at": common.now_iso(), "event": "stop", "lifetime": lifetime})
                stopped = True
        handle = read(handle_path) or {}
        common.append_jsonl(log, {"at": common.now_iso(), "event": "ended", "lifetime": lifetime,
                                  "exit": proc.returncode, "state": handle.get("state"),
                                  "stop_reason": handle.get("stop_reason"), "restartable": handle.get("restartable")})
        if stopped and handle.get("state") == "closed" and not handle.get("error"):
            result = {"schema": SCHEMA, "at": common.now_iso(), "root": str(fixtures), "base": lifetime,
                      "attempts": attempt, "image": args.image}
            common.write_json(fixtures / "base.json", result)
            print(json.dumps(result))
            return 0
        common.append_jsonl(args.failures, {
            "schema": FAILURE_SCHEMA, "at": common.now_iso(), "root": fixtures.name, "lifetime": lifetime,
            "error": handle.get("stop_reason") or handle.get("error"), "restartable": handle.get("restartable"),
            "vectors_job": failed_vectors_job(fixtures / lifetime), "other_bases_open_at_start": others})
        common.require(handle.get("restartable") is True, f"{lifetime} ended and is not restartable: "
                                                         f"{handle.get('stop_reason')}")
        origin = lifetime
    raise common.HarnessError(f"no base reached semantic readiness in {args.restarts + 1} attempts")


if __name__ == "__main__":
    sys.exit(main())
