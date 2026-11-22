# SPDX-License-Identifier: Apache-2.0
"""Run a change-set plan unattended: lifetime restarts, replays, session groups and grading, in order.

The plan (JSON) is a list of steps:

* ``{"restart": {"image": <folder>, "layers": "...", "search": "search_v2"|null,
  "inputs": <file>, "inputs_sha256": "...", "selector": "change-set-1"}}``. Stops the newest
  lifetime (it settles), waits until it is restartable and its service has exited, then starts
  the next lifetime from it and waits until it is queryable and its repositories are settled.
* ``{"replays": {"series": ..., "repos": ..., "out": ..., "plan": [[label, focus, context, result], ...],
  "answer_defaults": {...}}}``. Runs ``run_cs_replays.py``.
* ``{"sessions": {"spec": ..., "groups": [[focus, context, "name,name"], ...]}}``. For each group,
  sets the stages and runs ``run_pass.py --stage sessions --only``.
* ``{"grade": {"spec": ...}}``. Runs coverage, judge and grade, then ``pass_report.py`` and
  ``baseline_summary.py``.
* ``{"second": {"spec": ..., "groups": [[focus, context, "name,name"], ...]}}``. Runs the second
  sessions of the close calls for the named results, under each group's stages, then grades again.
* ``{"compare": {"a": <pass dir>, "b": <pass dir>, "out": <file>}}``. Runs ``compare_passes.py``.

The first failing step stops the plan. Each step is logged in ``--log`` with its exit code and
time. The plan leaves the newest lifetime running; its ``start`` process is a child of this
script, so the script then waits for that lifetime to close before exiting (``--hold``, the
default), or exits at once with ``--no-hold``.

``python -I -B run_plan.py --plan <plan.json> --log <file>``
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


def next_lifetime(previous: str, tag: str) -> str:
    """``L35-fix3`` and tag ``cs2a`` give ``L36-cs2a``."""
    digits = previous[1:].split("-")[0]
    common.require(previous.startswith("L") and digits.isdigit(), f"unexpected lifetime name {previous}")
    return f"L{int(digits) + 1}-{tag}"


def closed_verdict(raw: dict, pid_alive) -> str:
    """``closed`` once the handle is closed, restartable and its service has exited; ``blocked`` when the
    close found it not restartable; otherwise ``open``."""
    if raw.get("stop_reason") and raw.get("restartable") is False:
        return "blocked"  # the close wrote its restart blockers and refused to mark the handle closed
    if raw.get("state") != "closed":
        return "open"
    if raw.get("restartable") is not True:
        return "blocked"
    return "open" if pid_alive(raw.get("service_pid")) else "closed"


class Plan:
    def __init__(self, args):
        self.args = args
        self.r = args.r_root
        self.fixture_dir = self.r / "fixture"
        self.log_path = args.log
        self.children: list[subprocess.Popen] = []

    def log(self, **event):
        common.append_jsonl(self.log_path, {"at": common.now_iso(), **event})
        print(json.dumps(event), flush=True)

    def run(self, label, command, timeout=6 * 3600):
        began = time.monotonic()
        out = self.log_path.parent / "plan-steps"
        out.mkdir(parents=True, exist_ok=True)
        stem = f"{time.strftime('%H%M%S', time.gmtime())}-{label}"[:120]
        with (out / f"{stem}.log").open("wb") as stream:
            proc = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                                  cwd=str(common.REPO_ROOT), timeout=timeout,
                                  creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.log(step=label, exit_code=proc.returncode, elapsed_s=round(time.monotonic() - began, 1),
                 log=str(out / f"{stem}.log"))
        common.require(proc.returncode == 0, f"{label} failed (exit {proc.returncode})")

    # -------------------------------------------------------------- lifetimes

    def newest(self) -> Path:
        return common.newest_handle(self.fixture_dir)

    @staticmethod
    def pid_alive(pid) -> bool:
        if not pid:
            return False
        proc = subprocess.run(["tasklist", "/FI", f"PID eq {int(pid)}", "/NH"], capture_output=True, text=True,
                              creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        return str(int(pid)) in proc.stdout

    def stop_newest(self) -> str:
        handle = self.newest()
        lifetime = handle.parent.name
        raw = common.read_json(handle)
        if raw.get("state") != "closed":
            self.run(f"stop-{lifetime}", [*PY, "-S", str(HERE / "fixture.py"), "stop", "--lifetime", lifetime], 300)
        end = time.monotonic() + 1800
        while time.monotonic() < end:
            raw = common.read_json(handle)
            verdict = closed_verdict(raw, self.pid_alive)
            if verdict == "blocked":
                raise common.HarnessError(f"{lifetime} is not restartable: {raw.get('restart_blockers')}")
            if verdict == "closed":
                self.log(event="stopped", lifetime=lifetime)
                return lifetime
            time.sleep(10)
        raise common.HarnessError(f"{lifetime} did not close within 30 minutes")

    def restart(self, spec: dict):
        previous = self.stop_newest()
        name = next_lifetime(previous, spec.get("tag", "plan"))
        command = [*PY, "-S", str(HERE / "fixture.py"), "start", "--image", str(spec["image"]), "--lifetime", name,
                   "--restart-from", previous, "--seconds", str(spec.get("seconds", 21000)),
                   "--repositories", spec.get("repositories", "airk,bookbuilder,bookbuilder-v2")]
        if spec.get("selector"):
            command += ["--selector", spec["selector"], "--change-set-inputs", str(spec["inputs"]),
                        "--change-set-inputs-sha256", spec["inputs_sha256"], "--answer-layers", spec["layers"]]
        if spec.get("search"):
            command += ["--search", spec["search"]]
        log = (self.fixture_dir / f"{name}.start.log").open("wb")
        proc = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                                cwd=str(common.REPO_ROOT), creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.children.append(proc)
        self.log(event="start", lifetime=name, command=command, pid=proc.pid)
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
        repos = spec.get("settle", ["airk", "bookbuilder"])
        while time.monotonic() < end:
            ok, snaps = settled.check(str(handle), self.fixture_dir, repos, False)
            if ok:
                self.log(event="ready", lifetime=name, tokens={s.get("repo"): s.get("publication_token") for s in snaps})
                return
            time.sleep(30)
        raise common.HarnessError(f"{name} did not settle")

    # -------------------------------------------------------------- work

    def stages(self, focus, context, log_dir: Path):
        self.run(f"stages-{focus or 'none'}-{context or 'none'}",
                 [*PY, str(HERE / "set_stages.py"), "--focus", focus or "none", "--context", context or "none",
                  "--log-dir", str(log_dir)], 1800)

    def pass_step(self, spec: Path, label: str, *args: str):
        self.run(label, [*PY, str(HERE / "run_pass.py"), "--spec", str(spec), "--r-root", str(self.r), *args])

    def step(self, item: dict):
        kind, body = next(iter(item.items()))
        if kind == "restart":
            self.restart(body)
        elif kind == "replays":
            command = [*PY, str(HERE / "run_cs_replays.py"), "--series", body["series"], "--repos", body["repos"],
                       "--out", str(body["out"]), "--plan", json.dumps(body["plan"])]
            if body.get("answer_defaults") is not None:
                command += ["--answer-defaults", json.dumps(body["answer_defaults"])]
            self.run(f"replays-{body['repos']}-{Path(body['out']).name}", command, 4 * 3600)
        elif kind in ("sessions", "second"):
            spec = Path(body["spec"])
            pass_dir = self.r / "passes" / common.read_json(spec)["series"]
            for focus, context, names in body["groups"]:
                self.stages(focus, context, pass_dir)
                if kind == "sessions":
                    self.pass_step(spec, f"sessions-{names}", "--stage", "sessions", "--only", names)
                else:
                    self.pass_step(spec, f"second-{names}", "--stage", "second", "--only-second", names)
        elif kind == "grade":
            spec = Path(body["spec"])
            for stage in ("coverage", "judge", "grade"):
                self.pass_step(spec, f"{stage}-{spec.parent.name}", "--stage", stage)
            pass_dir = self.r / "passes" / common.read_json(spec)["series"]
            self.run(f"report-{spec.parent.name}", [*PY, str(HERE / "pass_report.py"), "--pass-dir", str(pass_dir)])
            self.run(f"summary-{spec.parent.name}", [*PY, str(HERE / "baseline_summary.py"), "--pass-dir",
                                                     str(pass_dir)])
        elif kind == "compare":
            self.run(f"compare-{Path(body['b']).name}", [*PY, str(HERE / "compare_passes.py"), "--a", str(body["a"]),
                                                         "--b", str(body["b"]), "--out", str(body["out"])])
        else:
            raise common.HarnessError(f"unknown plan step {kind}")

    def main(self) -> int:
        plan = common.read_json(self.args.plan)
        self.log(event="plan-start", plan=str(self.args.plan), steps=len(plan))
        for number, item in enumerate(plan, start=1):
            if number < self.args.from_step:
                continue
            self.log(event="step", number=number, kind=next(iter(item)))
            self.step(item)
        self.log(event="plan-done")
        if self.args.hold and self.children:
            self.log(event="holding", note="waiting for the last lifetime to close; stop it with fixture.py stop")
            self.children[-1].wait()
        return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--log", type=Path, required=True)
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    parser.add_argument("--from-step", type=int, default=1, help="resume at this 1-based step")
    parser.add_argument("--hold", dest="hold", action="store_true", default=True)
    parser.add_argument("--no-hold", dest="hold", action="store_false")
    args = parser.parse_args(argv)
    plan = Plan(args)
    try:
        return plan.main()
    except (common.HarnessError, subprocess.TimeoutExpired) as error:
        plan.log(event="stopped", error=str(error))
        if plan.children:
            plan.log(event="holding-after-failure", note="the last lifetime keeps running")
            plan.children[-1].wait()
        return 1


if __name__ == "__main__":
    sys.exit(main())
