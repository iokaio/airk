# SPDX-License-Identifier: Apache-2.0
"""Leave-one-out replays of a change set's Answer layers (plan 5.2 step 5), one restart per variant.

Each variant goes through these steps:

1. Stop the newest lifetime with ``fixture.py stop``, which settles it, and
   wait until its handle shows ``restartable: true`` and no service runs.
2. Start a new lifetime from it with ``--answer-layers <variant>``.
3. Wait until the lifetime is queryable and the repository is settled.
4. For each configuration (stages and the baseline result name), set the
   stages and replay the baseline's recorded first Answer calls with
   ``cs_replay.py``.
5. Stop the lifetime again.

The script leaves no lifetime running. The caller starts the next one (for
example ``--answer-layers all`` for the pass). Nothing is retried: a variant
whose lifetime does not come up stops the script, and the failure is
recorded in ``<out>/loo-run.jsonl``.

``python -I -B cs_loo.py --repo bookbuilder --series baseline-bookbuilder-r2 --variants all,-followups;all,-spread --out <R>/change-set-1/replays/bookbuilder``
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

PY = [sys.executable, "-I", "-B"]
CONFIGS = [  # (label suffix, focus profile, context profile, baseline result name)
    ("noModel", None, None, "noModel"),
    ("focus", "focus-static1024-cpu-cs1-software-w16", None, "focus-cpu"),
    ("compact", "focus-static1024-cpu-cs1-software-w16", "context-compact-cpu-cs1-provisional-read", "compact-cpu"),
]


class Loo:
    def __init__(self, args):
        self.args = args
        self.r = args.r_root
        self.fixture_dir = self.r / "fixture"
        self.out = args.out
        self.out.mkdir(parents=True, exist_ok=True)
        self.log_path = self.out / "loo-run.jsonl"

    def log(self, **event):
        common.append_jsonl(self.log_path, {"at": common.now_iso(), **event})
        print(json.dumps(event), flush=True)

    def run(self, label, command, timeout=3600):
        began = time.monotonic()
        proc = subprocess.run(command, capture_output=True, stdin=subprocess.DEVNULL, timeout=timeout,
                              cwd=str(common.REPO_ROOT), creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        common.write_bytes(self.out / "steps" / f"{time.strftime('%H%M%S', time.gmtime())}-{label}.log",
                           proc.stdout + b"\n--- stderr ---\n" + proc.stderr)
        self.log(step=label, exit_code=proc.returncode, elapsed_s=round(time.monotonic() - began, 1))
        common.require(proc.returncode == 0, f"{label} failed (exit {proc.returncode})")
        return proc

    def newest(self) -> Path:
        return common.newest_handle(self.fixture_dir)

    def stop_newest(self):
        handle = self.newest()
        raw = common.read_json(handle)
        lifetime = handle.parent.name
        if raw.get("restartable") is True and raw.get("stop_reason"):
            return lifetime
        self.run(f"stop-{lifetime}", [*PY, "-S", str(HERE / "fixture.py"), "stop", "--lifetime", lifetime], 300)
        end = time.monotonic() + 1800
        while time.monotonic() < end:
            raw = common.read_json(handle)
            if raw.get("restartable") is True and (raw.get("stop_reason") or raw.get("state") == "closed"):
                if not self.service_running(raw.get("service_pid")):
                    self.log(event="stopped", lifetime=lifetime, restartable=True)
                    return lifetime
            if raw.get("restartable") is False:
                raise common.HarnessError(f"{lifetime} is not restartable: {raw.get('restart_blockers')}")
            time.sleep(10)
        raise common.HarnessError(f"{lifetime} did not settle and close within 30 minutes")

    @staticmethod
    def service_running(pid) -> bool:
        """Whether this lifetime's own service process still runs (another agent's service is not ours)."""
        if not pid:
            return False
        proc = subprocess.run(["tasklist", "/FI", f"PID eq {int(pid)}", "/NH"], capture_output=True, text=True,
                              creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        return str(int(pid)) in proc.stdout

    def start(self, previous: str, name: str, layers: str) -> subprocess.Popen:
        a = self.args
        command = [*PY, "-S", str(HERE / "fixture.py"), "start", "--image", str(a.image), "--lifetime", name,
                   "--restart-from", previous, "--seconds", str(a.seconds), "--repositories", a.repositories,
                   "--selector", "change-set-1", "--change-set-inputs", str(a.inputs),
                   "--change-set-inputs-sha256", a.inputs_sha256, "--answer-layers", layers]
        log = (self.fixture_dir / f"{name}.start.log").open("wb")
        proc = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                                cwd=str(common.REPO_ROOT), creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.log(event="start", lifetime=name, layers=layers, pid=proc.pid, restart_from=previous)
        handle = self.fixture_dir / name / "handle.json"
        end = time.monotonic() + 3600
        while time.monotonic() < end:
            if handle.is_file():
                raw = common.read_json(handle)
                if raw.get("queryable") is True:
                    break
                if raw.get("stop_reason"):
                    raise common.HarnessError(f"{name} ended before readiness: {raw.get('stop_reason')}")
            if proc.poll() is not None:
                raise common.HarnessError(f"{name} start exited ({proc.returncode}) before readiness")
            time.sleep(10)
        ok = False
        while time.monotonic() < end and not ok:
            ok, snaps = settled.check(str(handle), self.fixture_dir, [a.repo], False)
            if not ok:
                time.sleep(30)
        common.require(ok, f"{a.repo} not settled on {name}")
        self.log(event="ready", lifetime=name, token=snaps[0].get("publication_token"))
        return proc

    def replays(self, layers: str):
        tag = layers.replace(",", "").replace("-", "minus-")
        for suffix, focus, context, result in CONFIGS:
            stages = {"focus": focus, "context": context}
            self.run(f"stages-{tag}-{suffix}", [*PY, str(HERE / "set_stages.py"), "--focus", focus or "none",
                                                "--context", context or "none", "--log-dir", str(self.out)], 1800)
            self.run(f"replay-{tag}-{suffix}", [*PY, str(HERE / "cs_replay.py"), "--series", self.args.series,
                                                "--configurations", result, "--label", f"loo-{tag}-{suffix}",
                                                "--out", str(self.out), "--repos", self.args.repo,
                                                "--stages", json.dumps(stages),
                                                "--answer-defaults", self.args.answer_defaults], 3600)

    def main(self):
        number = max(int("".join(c for c in p.parent.name[1:].split("-")[0] if c.isdigit()))
                     for p in self.fixture_dir.glob("L*/handle.json"))
        for layers in self.args.variants.split(";"):
            previous = self.stop_newest()
            number += 1
            name = f"L{number}-cs1loo"
            proc = self.start(previous, name, layers)
            self.replays(layers)
            self.stop_newest()
            proc.wait(timeout=600)
        self.log(event="done", variants=self.args.variants)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", required=True)
    parser.add_argument("--series", required=True)
    parser.add_argument("--variants", required=True, help="semicolon-separated ANSWER_LAYERS values")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    parser.add_argument("--image", type=Path, default=common.DEFAULT_R / "fixture" / "images" / "cs1d-d316beaf")
    parser.add_argument("--repositories", default="airk,bookbuilder,bookbuilder-v2",
                        help="indexes every restarted lifetime keeps (fixture.py start --repositories)")
    parser.add_argument("--answer-defaults",
                        default=json.dumps({"budget": {"calls": 12, "rounds": 3, "fanout": 12, "candidates": 32,
                                                       "max_bytes": 524288, "input_tokens": 8192,
                                                       "deadline_ms": 120000}}),
                        help="JSON merged into each replayed Answer call (decision 5: change set 1's budget)")
    parser.add_argument("--inputs", type=Path,
                        default=common.DEFAULT_R / "change-set-1" / "profiles" / "change-set-inputs-r2.json")
    parser.add_argument("--inputs-sha256", default="a0f26d834ec3fd322129e8daf28459133043f4690d17ec12d2b91afdf7094c59")
    parser.add_argument("--seconds", type=int, default=21000)
    args = parser.parse_args(argv)
    loo = Loo(args)
    try:
        loo.main()
    except (common.HarnessError, subprocess.TimeoutExpired) as error:
        loo.log(event="stopped", error=str(error))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
