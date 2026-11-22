# SPDX-License-Identifier: Apache-2.0
"""Run a baseline pass end to end, unattended: wait, stage, sessions, grading, reference arm, report, depth.

The script only sequences the frozen tools. ``settled.py`` waits for the index.
``fixture.py stage`` (run, never edited) switches the Arbiter stages that each
group of entries declares, and the stages are verified live afterwards.
``run_pass.py`` runs every step in its own process, so each step keeps its
guard and its own log. The order is:

1. Wait until every ``settle_repos`` index is settled (``settled.check``).
2. Run the entries without a path marker, grouped by declared stages: none,
   then Focus, then Focus with Compact, then Focus with Large.
3. Once the stage-free group has run, run coverage, judge and grade. The
   entries with ``@better-of-search-parse`` need those grades. They then run
   in the stage-free state, and stages are switched back if needed.
4. Run coverage, judge, grade and ``pass_report.py`` a final time. With
   ``--depth``, run ``depth.py`` if its output does not exist yet.

The script stops on the first failed step: an unsettled index, a changed
publication token, or a stage that cannot be set. It records the failure in
``<pass>/baseline-run.jsonl``. Nothing is retried except a refused stage
switch, at most 3 times 30 s apart. A stage switch is fixture plumbing, not a
measurement.

``python -I -B run_baseline.py --spec <R>/passes/<series>/spec.json [--depth <R>/passes/<series>/depth]``
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
STAGE_ORDER = [{"focus": None, "context": None}, {"focus": "focus-static1024-cpu", "context": None},
               {"focus": "focus-static1024-cpu", "context": "context-compact-cpu"},
               {"focus": "focus-static1024-cpu", "context": "context-large-cpu"}]


def wanted_stages(entry: dict) -> dict:
    return {"focus": None, "context": None, **(entry.get("stages") or {})}


def groups(spec: dict) -> list[tuple[dict, list[str]]]:
    """(stages, result names) in STAGE_ORDER, excluding entries whose path is a marker."""
    out = []
    for stages in STAGE_ORDER + [s for s in (wanted_stages(e) for e in spec["results"]) if s not in STAGE_ORDER]:
        names = [result_name(e) for e in spec["results"]
                 if wanted_stages(e) == stages and not e["path"].startswith("@")]
        if names and stages not in [g[0] for g in out]:
            out.append((stages, names))
    return out


def result_name(entry: dict) -> str:
    device = entry.get("device", "none")
    return entry["configuration"] if device in (None, "", "none") else f"{entry['configuration']}-{device}"


class Runner:
    def __init__(self, spec_path: Path, r_root: Path):
        self.spec_path = spec_path
        self.spec = common.read_json(spec_path)
        self.r_root = r_root
        self.pass_dir = r_root / "passes" / self.spec["series"]
        self.pass_dir.mkdir(parents=True, exist_ok=True)
        self.log_path = self.pass_dir / "baseline-run.jsonl"

    def log(self, **event) -> None:
        common.append_jsonl(self.log_path, {"at": common.now_iso(), **event})
        print(json.dumps(event), flush=True)

    def run(self, label: str, command: list[str]) -> None:
        began = time.monotonic()
        out = self.pass_dir / "steps"
        out.mkdir(exist_ok=True)
        stem = f"{time.strftime('%H%M%S', time.gmtime())}-{label}"
        with (out / f"{stem}.log").open("wb") as stream:
            proc = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                                  creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.log(step=label, exit_code=proc.returncode, elapsed_s=round(time.monotonic() - began, 1),
                 log=str(out / f"{stem}.log"), command=command)
        common.require(proc.returncode == 0, f"step {label} failed (exit {proc.returncode}); see {out / stem}.log")

    def pass_step(self, label: str, *args: str) -> None:
        self.run(label, [*PY, str(HERE / "run_pass.py"), "--spec", str(self.spec_path), "--r-root", str(self.r_root),
                         *args])

    def handle(self) -> common.Handle:
        return common.load_handle(common.resolve_handle(self.spec["handle"], self.r_root / "fixture"))

    def set_stages(self, wanted: dict) -> None:
        handle = self.handle()
        lifetime = handle.path.parent.name
        live = handle.stages()
        steps = []
        if live["context"] and live["context"] != wanted["context"]:
            steps.append((live["context"], False))
        if live["focus"] != wanted["focus"]:
            # Switching one enabled Focus profile straight to another is refused (arbiter_unavailable on cs1e,
            # 22 Nov 2026), so the live profile is disabled first.
            if live["focus"] is not None:
                steps.append((live["focus"], False))
            if wanted["focus"] is not None:
                steps.append((wanted["focus"], True))
        if wanted["context"] and live["context"] != wanted["context"]:
            steps.append((wanted["context"], True))
        for profile, enable in steps:
            for attempt in range(1, 4):
                command = [*PY, "-S", str(HERE / "fixture.py"), "stage", "--lifetime", lifetime, "--profile", profile,
                           "--enable" if enable else "--disable"]
                try:
                    self.run(f"stage-{'on' if enable else 'off'}-{profile}-a{attempt}", command)
                    break
                except common.HarnessError:
                    common.require(attempt < 3, f"stage {profile} could not be set")
                    time.sleep(30)
        live = self.handle().stages()
        common.require(live == wanted, f"stages are {live}, wanted {wanted}")
        self.log(stages=live, lifetime=lifetime)

    def wait_settled(self, timeout_s: float, interval_s: float) -> None:
        repos = self.spec.get("settle_repos") or self.spec.get("repos")
        end = time.monotonic() + timeout_s
        while True:
            ok, snaps = settled.check(self.spec["handle"], self.r_root / "fixture", repos, False)
            common.append_jsonl(self.pass_dir / "settled-wait.jsonl", {"at": common.now_iso(), "ok": ok, "snaps": snaps})
            if ok:
                self.log(settled=True, tokens={s["repo"]: s.get("publication_token") for s in snaps})
                return
            common.require(time.monotonic() < end, f"index not settled after {timeout_s:.0f} s")
            time.sleep(interval_s)

    def second_sessions(self) -> None:
        summary_path = self.pass_dir / "summary.json"
        summary = common.read_json(summary_path) if summary_path.is_file() else {}
        names = set()
        for name, config in (summary.get("configurations") or {}).items():
            for row in config.get("scenarios", []):
                if row["scenario"] in config.get("close_calls", []):
                    names.add(name)
                    if row.get("best_path"):
                        names.add(row["best_path"])
        if not names:
            self.log(event="no-close-calls")
            return
        by_name = {result_name(e): e for e in self.spec["results"]}
        for stages, group in groups(self.spec):
            if any(n in names for n in group):
                self.set_stages(stages)
                self.pass_step("second-" + "+".join(sorted(n for n in group if n in names)), "--stage", "second",
                               "--only-second", ",".join(sorted(n for n in group if n in names)))
        self.log(event="second-sessions", results=sorted(names), known=sorted(n for n in names if n in by_name))

    def main(self, depth_out: Path | None, wait_s: float, interval_s: float) -> None:
        self.log(event="start", spec=str(self.spec_path), spec_sha256=common.sha256_file(self.spec_path))
        self.wait_settled(wait_s, interval_s)
        markers = [result_name(e) for e in self.spec["results"] if e["path"].startswith("@")]
        for stages, names in groups(self.spec):
            self.set_stages(stages)
            self.pass_step("sessions-" + "+".join(names), "--stage", "sessions", "--only", ",".join(names))
            if stages == STAGE_ORDER[0] and markers:
                for stage in ("coverage", "judge", "grade"):
                    self.pass_step(f"{stage}-before-markers", "--stage", stage)
                self.pass_step("sessions-" + "+".join(markers), "--stage", "sessions", "--only", ",".join(markers))
        for stage in ("coverage", "judge", "grade"):
            self.pass_step(stage, "--stage", stage)
        # Decision (22 Nov 2026): every pass runs second sessions on every close call. The second sessions of
        # an entry need its stages, so each close-call group is run under its own stages.
        self.second_sessions()
        self.run("pass-report", [*PY, str(HERE / "pass_report.py"), "--pass-dir", str(self.pass_dir)])
        if depth_out is not None and not (depth_out / "depth.json").is_file():
            self.set_stages(STAGE_ORDER[0])
            repos = ",".join(self.spec.get("repos") or ["airk"])
            self.run("depth", [*PY, str(HERE / "depth.py"), "--handle", str(self.spec["handle"]), "--out",
                               str(depth_out), "--repos", repos, "--limit", "100", "--max-bytes", "65536"])
        self.log(event="done")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    parser.add_argument("--depth", type=Path, help="output folder for depth.py (run once, after the pass)")
    parser.add_argument("--wait", type=float, default=8 * 3600.0, help="seconds to wait for a settled index")
    parser.add_argument("--interval", type=float, default=60.0)
    args = parser.parse_args(argv)
    runner = Runner(args.spec, args.r_root)
    try:
        runner.main(args.depth, args.wait, args.interval)
    except common.HarnessError as error:
        runner.log(event="stopped", error=str(error))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
