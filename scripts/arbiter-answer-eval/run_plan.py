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
* ``{"switch_context": {"focus": <profile>|null, "context": <profile>|null,
  "repositories": "munarium,bookbuilder-v2", "receipt": <file>}}``. The named hook for switching the Context
  model within one lifetime (plan 12.5.2 item 4; ``Plan.switch_context``): disable the live CUDA Context (the
  release), then enable the wanted Focus and Context profiles (the warm), through ``gpu_fixture.py stage``
  (ordinary Configure). ``repositories`` names indexes of the lifetime; without it every index of the lifetime
  is staged. The receipt keeps release and warm time and the device memory before and after.
* ``{"gate": {"command": "replays"|"agreement"|"outcomes"|"concurrency"|"performance"|"precision", ...}}``.
  Runs ``gate.py <command>`` with the step's other keys as ``--key value`` options (a list repeats it).
* ``{"lifetimes": {"start": [<lifetime>, ...], "reserve_device_gib": 4, "reserve_host_gib": 16, "receipt": <file>}}``.
  Side-by-side arm lifetimes: checks that the declared device and host memory of every listed lifetime, plus
  the reserves, is free now (``nvidia-smi``; the OS's available memory), launches each with its own
  ``--fixtures`` root (its own service and control root), waits until all are queryable and settled, and
  enables each lifetime's stages. A lifetime (``make_loop_spec.py gate``) names its ``arm``, ``fixtures``,
  ``lifetime``, ``launcher`` (``fixture.py`` or ``gpu_fixture.py``), ``start_args``, ``device_gib``,
  ``host_gib``, ``repositories`` and ``stages``; placeholder ``<...>`` start options are refused.
  ``repositories`` are the scenario repositories the settled guard checks (default the loop pair, munarium
  and bookbuilder, resolved through the root's index aliases). The stages enrol every index of the lifetime
  unless the row names ``stage_repositories`` (index names such as ``munarium,bookbuilder-v2``).
* ``{"stop_lifetimes": {"stop": [<lifetime>, ...]}}``. Disables each CUDA Context stage, stops each lifetime and
  waits until each has closed.
* ``{"parallel": [<step>, ...]}``. Runs the steps at once (e.g. the arms' replays).
* ``sessions`` and ``second`` groups take ``"stager": "cuda"`` to stage through ``switch_context`` instead
  of ``set_stages.py``; a group of ``"@any", "@any"`` leaves the stages as they are (Search and Parse).
  ``second`` takes ``"jobs": [<second-jobs file>, ...]`` (``gate.py outcomes``) for the gate close calls.

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


def settle_repositories(row: dict) -> list[str]:
    """The scenario repositories whose indexes the settled guard checks for a lifetime row: its own
    ``repositories``, else the loop pair. A name is resolved through the fixture root's index aliases."""
    return [r for r in (row.get("repositories") or ",".join(common.REPOS)).split(",") if r]


def stage_repositories(row: dict) -> list[str]:
    """The ``--repositories`` option of a stage command: only when the row names ``stage_repositories`` (index
    names of the lifetime, not scenario repositories: ``bookbuilder`` is served by the index ``bookbuilder-v2``).
    Without it the launcher enrols every index of the lifetime."""
    return ["--repositories", row["stage_repositories"]] if row.get("stage_repositories") else []


class Plan:
    def __init__(self, args):
        self.args = args
        self.r = args.r_root
        self.fixture_dir = self.r / "fixture"
        self.log_path = args.log
        self.children: list[subprocess.Popen] = []
        # On a resumed plan (--from-step) the live CUDA stages are given, so the first switch releases them.
        self.live_cuda = json.loads(args.live_cuda) if getattr(args, "live_cuda", None) else None

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
                   "--repositories", spec.get("repositories", ",".join(common.LOOP_INDEXES))]
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
        repos = spec.get("settle", list(common.REPOS))
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

    # The live CUDA stages as this plan last set them (switch_context); None until the first switch.
    live_cuda: dict | None = None

    def lifetime_name(self) -> str:
        return self.newest().parent.name

    def switch_context(self, body: dict) -> dict:
        """The in-lifetime Context switch hook (plan 12.5.2 item 4, L5 design section 5.3): release first, then
        warm, so the two Context models are never resident together. A step whose stages are already live
        records a receipt with ``unchanged`` and runs nothing."""
        wanted = {"focus": body.get("focus"), "context": body.get("context")}
        # Arbiter only (arm A) switches Focus off. Before the plan's first switch it does not know which Focus
        # profile is live (a lifetime may start with Focus seeded), so it could not release it: refuse rather
        # than run arm A with Focus still on.
        common.require(wanted["focus"] is not None or self.live_cuda is not None,
                       "a switch to no Focus needs the live stages known: switch to an arm with Focus first")
        lifetime = self.lifetime_name()
        repos = body.get("repositories")  # index names; None stages every index of the lifetime
        receipt = {"schema": "airk.private.answer-eval-switch/1", "at": common.now_iso(), "lifetime": lifetime,
                   "from": dict(self.live_cuda) if self.live_cuda else None, "to": wanted}
        receipt["memory_before"] = self.gpu_memory()
        began = time.monotonic()
        if self.live_cuda == wanted:
            receipt.update(unchanged=True, release_s=0.0, warm_s=0.0)
        else:
            current = self.live_cuda or {}
            release_began = time.monotonic()
            for stage in ("context", "focus"):
                if current.get(stage) and current.get(stage) != wanted.get(stage):
                    self.stage_command(lifetime, current[stage], False, repos, stage)
            receipt["release_s"] = round(time.monotonic() - release_began, 3)
            warm_began = time.monotonic()
            for stage in ("focus", "context"):
                if wanted.get(stage) and wanted.get(stage) != current.get(stage):
                    self.stage_command(lifetime, wanted[stage], True, repos, stage)
            receipt["warm_s"] = round(time.monotonic() - warm_began, 3)
            self.live_cuda = wanted
        receipt["total_s"] = round(time.monotonic() - began, 3)
        receipt["memory_after"] = self.gpu_memory()
        receipt["ok"] = True
        if body.get("receipt"):
            common.write_json(Path(body["receipt"]), receipt)
        self.log(event="switch_context", **{k: receipt[k] for k in ("from", "to", "release_s", "warm_s")})
        return receipt

    def stage_command(self, lifetime: str, profile: str, enable: bool, repos: str | None, stage: str):
        self.run(f"stage-{'on' if enable else 'off'}-{profile}",
                 [*PY, "-S", str(HERE / "gpu_fixture.py"), "stage", "--lifetime", lifetime, "--profile", profile,
                  "--enable" if enable else "--disable", *stage_repositories({"stage_repositories": repos})], 3600)

    @staticmethod
    def gpu_memory():
        import gate  # local: only the gate steps sample device memory
        return gate.nvidia_smi_used()

    def stage_group(self, focus, context, log_dir: Path, stager: str | None):
        if focus == "@any" and context == "@any":
            return
        if stager == "cuda":
            self.switch_context({"focus": focus, "context": context})
        else:
            self.stages(focus, context, log_dir)

    def gate_step(self, body: dict):
        command = [*PY, str(HERE / "gate.py"), "--r-root", str(self.r), body["command"]]
        for key, value in body.items():
            if key == "command":
                continue
            for item in value if isinstance(value, list) else [value]:
                command += [f"--{key.replace('_', '-')}", str(item)]
        self.run(f"gate-{body['command']}-{body.get('arm') or Path(str(body.get('out', ''))).stem}", command,
                 8 * 3600)

    def pass_step(self, spec: Path, label: str, *args: str):
        self.run(label, [*PY, str(HERE / "run_pass.py"), "--spec", str(spec), "--r-root", str(self.r), *args])

    # -------------------------------------------------------------- side-by-side arm lifetimes

    @staticmethod
    def device_memory():
        import gate  # local: only the lifetime steps read device memory
        return gate.nvidia_smi_memory()

    @staticmethod
    def host_available_bytes():
        import gate
        return gate.host_available_bytes()

    def memory_check(self, rows: list[dict], reserve_device_gib: float, reserve_host_gib: float) -> dict:
        """Refuse a launch whose declared device or host memory, plus the reserve, exceeds what is free now."""
        gib = 1 << 30
        device = self.device_memory()
        need_device = sum(float(r.get("device_gib") or 0) for r in rows)
        need_host = sum(float(r.get("host_gib") or 0) for r in rows)
        check = {"at": common.now_iso(), "need_device_gib": need_device, "need_host_gib": need_host,
                 "reserve_device_gib": reserve_device_gib, "reserve_host_gib": reserve_host_gib, "device": device}
        if need_device > 0:
            common.require(device is not None, "nvidia-smi gave no device memory; the launch is refused")
            free = max(g["total_mib"] - g["used_mib"] for g in device.values()) / 1024
            check["free_device_gib"] = round(free, 2)
            common.require(free >= need_device + reserve_device_gib,
                           f"device memory: {free:.1f} GiB free, {need_device + reserve_device_gib:.1f} GiB needed")
        host = self.host_available_bytes()
        check["free_host_gib"] = None if host is None else round(host / gib, 2)
        if host is not None:
            common.require(host / gib >= need_host + reserve_host_gib,
                           f"host memory: {host / gib:.1f} GiB free, {need_host + reserve_host_gib:.1f} GiB needed")
        return check

    def launcher_command(self, row: dict, *tail: str) -> list[str]:
        common.require(row["launcher"] in ("fixture.py", "gpu_fixture.py"), f"unknown launcher {row['launcher']}")
        return [*PY, "-S", str(HERE / row["launcher"]), "--fixtures", str(row["fixtures"]), *tail]

    def start_lifetimes(self, body: dict) -> dict:
        """Launch every listed lifetime at once (each its own fixture root, service and control root), wait until
        each is queryable and settled, then enable its stages. Placeholder start options are refused."""
        rows = body["start"]
        for row in rows:
            bad = [a for a in row.get("start_args") or [] if str(a).startswith("<")]
            common.require(not bad, f"lifetime {row['lifetime']} still has placeholder start options {bad}")
        receipt = {"schema": "airk.private.answer-eval-lifetimes/1",
                   "memory": self.memory_check(rows, float(body.get("reserve_device_gib", 4)),
                                               float(body.get("reserve_host_gib", 16))),
                   "lifetimes": []}
        handles = {}
        for row in rows:
            Path(row["fixtures"]).mkdir(parents=True, exist_ok=True)
            command = self.launcher_command(row, "start", "--lifetime", row["lifetime"], *row["start_args"])
            log = (Path(row["fixtures"]) / f"{row['lifetime']}.start.log").open("wb")
            proc = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                                    cwd=str(common.REPO_ROOT), creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            self.children.append(proc)
            handles[row["lifetime"]] = (row, proc, Path(row["fixtures"]) / row["lifetime"] / "handle.json")
            self.log(event="start", lifetime=row["lifetime"], arm=row.get("arm"), command=command, pid=proc.pid)
        end = time.monotonic() + float(body.get("ready_seconds", 3 * 3600))
        pending = dict(handles)
        while pending and time.monotonic() < end:
            for name, (row, proc, handle) in list(pending.items()):
                if proc.poll() is not None:
                    raise common.HarnessError(f"{name} start exited ({proc.returncode}) before readiness")
                if not handle.is_file():
                    continue
                raw = common.read_json(handle)
                if raw.get("stop_reason"):
                    raise common.HarnessError(f"{name} ended before readiness: {raw.get('stop_reason')}")
                if raw.get("queryable") is not True:
                    continue
                ok, snaps = settled.check(str(handle), Path(row["fixtures"]),
                                          settle_repositories(row), False)
                if ok:
                    if row["launcher"] == "gpu_fixture.py":
                        # A CUDA lifetime's service must hold the product's CUDA math (GEMM2) before any FP16
                        # warm; the handle records what the launcher gave it (s5 shakedown, 23 Nov 2026).
                        import gpu_fixture
                        problems = gpu_fixture.math_problems(raw.get("service_environment") or {})
                        common.require(not problems, f"{name}: service math environment: {problems}")
                    del pending[name]
                    # The wrapper serves agents from the root's tools catalog; write it from this lifetime.
                    catalog = common.write_tools_catalog(common.load_handle(handle), Path(row["fixtures"]))
                    self.log(event="ready", lifetime=name, catalog=catalog,
                             tokens={s.get("repo"): s.get("publication_token") for s in snaps})
            if pending:
                time.sleep(float(body.get("poll_seconds", 30)))
        common.require(not pending, f"lifetimes not ready in time: {sorted(pending)}")
        for row in rows:
            for stage in row.get("stages") or []:
                self.run(f"stage-on-{row['arm']}-{stage['profile']}", self.launcher_command(
                    row, "stage", "--lifetime", row["lifetime"], "--profile", stage["profile"], "--enable",
                    *stage_repositories(row)), 3600)
            if not hasattr(self, "live"):
                self.live = {}
            self.live[row["lifetime"]] = row
            receipt["lifetimes"].append({k: row.get(k) for k in ("arm", "lifetime", "fixtures", "launcher",
                                                                 "start_args", "device_gib", "host_gib", "stages")})
        receipt["memory_after"] = self.device_memory()
        if body.get("receipt"):
            common.write_json(Path(body["receipt"]), receipt)
        return receipt

    ANSWER_GATE_COMMANDS = ("smoke", "replays", "concurrency")

    def ensure_stages(self, reason: str) -> None:
        """Before a step that issues Answers: every live CUDA lifetime's required stages are verified active in the
        service's status and re-enabled if not (``gpu_fixture.py ensure``; recorded in the lifetime's
        ``ensure.jsonl`` and this log). On 23 November 2026 shakedown s8's Focus unloaded between steps and
        later Answers ran without it. A fixture.py lifetime (CPU) is not checked here; the arm check still is."""
        for row in getattr(self, "live", {}).values():
            profiles = [s["profile"] for s in row.get("stages") or []]
            if row["launcher"] != "gpu_fixture.py" or not profiles:
                continue
            self.run(f"ensure-{row['arm']}-{reason}", self.launcher_command(
                row, "ensure", "--lifetime", row["lifetime"], "--profiles", ",".join(profiles)), 3600)

    def stop_lifetimes(self, body: dict) -> None:
        """Disable each lifetime's CUDA Context stage (GPU_FIXTURE.md: before stop), stop it, wait for the close."""
        rows = body["stop"]
        for row in rows:
            getattr(self, "live", {}).pop(row["lifetime"], None)
        for row in rows:
            for stage in reversed(row.get("stages") or []):
                if row["launcher"] == "gpu_fixture.py" and stage["stage"] == "context":
                    self.run(f"stage-off-{row['arm']}-{stage['profile']}", self.launcher_command(
                        row, "stage", "--lifetime", row["lifetime"], "--profile", stage["profile"], "--disable",
                        *stage_repositories(row)), 3600)
            self.run(f"stop-{row['lifetime']}", self.launcher_command(row, "stop", "--lifetime", row["lifetime"]), 300)
        end = time.monotonic() + float(body.get("close_seconds", 1800))
        open_ = {row["lifetime"]: Path(row["fixtures"]) / row["lifetime"] / "handle.json" for row in rows}
        while open_ and time.monotonic() < end:
            for name, handle in list(open_.items()):
                verdict = closed_verdict(common.read_json(handle), self.pid_alive) if handle.is_file() else "closed"
                if verdict == "blocked":
                    raise common.HarnessError(f"{name} is not restartable")
                if verdict == "closed":
                    del open_[name]
                    self.log(event="stopped", lifetime=name)
            if open_:
                time.sleep(float(body.get("poll_seconds", 10)))
        common.require(not open_, f"lifetimes did not close: {sorted(open_)}")

    def parallel(self, steps: list[dict]) -> None:
        """Run independent steps at once; the first failure is raised after all have finished."""
        import concurrent.futures
        if any(next(iter(i)) in ("sessions", "second") or (next(iter(i)) == "gate" and i["gate"].get("command")
                                                                in self.ANSWER_GATE_COMMANDS) for i in steps):
            self.ensure_stages("parallel")  # once, before the branches start; never concurrently
        with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, len(steps))) as pool:
            futures = [pool.submit(self.step, item, False) for item in steps]
            errors = [f.exception() for f in futures if f.exception() is not None]
        if errors:
            raise errors[0]

    def step(self, item: dict, ensure: bool = True):
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
            if ensure:
                self.ensure_stages(kind)
            spec = Path(body["spec"])
            pass_dir = self.r / "passes" / common.read_json(spec)["series"]
            jobs = body.get("jobs") or [None]
            for focus, context, names in body["groups"]:
                self.stage_group(focus, context, pass_dir, body.get("stager"))
                if kind == "sessions":
                    self.pass_step(spec, f"sessions-{names}", "--stage", "sessions", "--only", names)
                else:
                    for job in jobs:
                        extra = ["--second-jobs", str(job)] if job else []
                        self.pass_step(spec, f"second-{names}", "--stage", "second", "--only-second", names, *extra)
        elif kind == "switch_context":
            self.switch_context(body)
        elif kind == "gate":
            if ensure and body.get("command") in self.ANSWER_GATE_COMMANDS:
                self.ensure_stages(f"{body['command']}-{body.get('arm')}")
            self.gate_step(body)
        elif kind == "lifetimes":
            self.start_lifetimes(body)
        elif kind == "stop_lifetimes":
            self.stop_lifetimes(body)
        elif kind == "parallel":
            self.parallel(body)
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
    parser.add_argument("--live-cuda", help='JSON {"focus": ..., "context": ...}: the CUDA stages live at a resume')
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
